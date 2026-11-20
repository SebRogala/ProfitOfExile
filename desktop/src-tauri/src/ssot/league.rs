use super::*;

/// First retry delay for the startup league fetch.
const LEAGUE_FETCH_INITIAL_BACKOFF: Duration = Duration::from_secs(2);
/// Ceiling for the exponential backoff — offline-at-launch keeps retrying at
/// most this often, never faster, until the server answers.
const LEAGUE_FETCH_MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Consecutive failed fetch attempts before the SSOT flips to `unreachable`.
/// With the 2→60s backoff this is ~30-60s of silence — long enough not to flag
/// a transient blip, short enough that a genuinely down server stops showing a
/// perpetual "Resolving…". The loop keeps retrying past this point.
const LEAGUE_UNREACHABLE_AFTER_ATTEMPTS: u32 = 4;

/// One process owns the resolver, including while its UI flag is idle.
static LEAGUE_SCHEDULER: LazyLock<LeagueScheduler> = LazyLock::new(LeagueScheduler::new);

struct LeagueScheduler {
    task: std::sync::Mutex<LeagueTask>,
    notify: Notify,
}

impl LeagueScheduler {
    fn new() -> Self {
        Self { task: std::sync::Mutex::new(LeagueTask::default()), notify: Notify::new() }
    }

    fn trigger(&self) -> bool {
        let spawn = self.task.lock().unwrap_or_else(|e| e.into_inner()).trigger();
        self.notify.notify_one();
        spawn
    }

    fn begin_request(&self) {
        // This request covers only triggers already received. Later triggers
        // remain pending through its response and require another status read.
        self.task.lock().unwrap_or_else(|e| e.into_inner()).pending = false;
    }

    async fn wait(&self, delay: Option<Duration>, runtime: &impl LeagueRuntime) -> bool {
        let wake = async {
            loop {
                let notified = self.notify.notified();
                if std::mem::take(&mut self.task.lock().unwrap_or_else(|e| e.into_inner()).pending) {
                    return;
                }
                notified.await;
            }
        };
        if let Some(delay) = delay {
            tokio::select! {
                _ = wake => true,
                _ = runtime.sleep(delay) => false,
            }
        } else {
            wake.await;
            true
        }
    }
}

#[derive(Default)]
struct LeagueTask {
    live: bool,
    pending: bool,
    url_generation: u64,
}

impl LeagueTask {
    fn trigger(&mut self) -> bool {
        self.pending = true;
        let spawn = !self.live;
        self.live = true;
        spawn
    }
}

/// League slice of the SSOT.
#[derive(Debug, Clone, Serialize)]
struct LeagueChanged {
    league: String,
    generation: u64,
}

/// Whether `consecutive_failures` failed fetch attempts should flip the SSOT to
/// `unreachable`. Pure so the threshold boundary is directly unit-testable.
fn should_flag_unreachable(consecutive_failures: u32) -> bool {
    consecutive_failures >= LEAGUE_UNREACHABLE_AFTER_ATTEMPTS
}

/// Mutate the sole holder and return a consumer event only for a real change.
fn write_league(
    ssot: &std::sync::Mutex<AppSsotSnapshot>,
    name: String,
) -> Option<LeagueChanged> {
    let mut guard = ssot.lock().unwrap_or_else(|e| e.into_inner());
    let changed = guard.league.name.as_ref().is_some_and(|old| old != &name);
    if guard.league.name.as_ref() != Some(&name) {
        guard.league.generation += 1;
    }
    guard.league.name = Some(name.clone());
    guard.resolving = false;
    guard.unreachable = false;
    changed.then(|| LeagueChanged { league: name, generation: guard.league.generation })
}

/// UI state describes an active request/retry window, not task liveness.
fn mark_resolving(ssot: &std::sync::Mutex<AppSsotSnapshot>) {
    ssot.lock().unwrap_or_else(|e| e.into_inner()).resolving = true;
}

/// Trim an incoming league name; return `Some(trimmed)` only when non-empty.
///
/// The single fail-closed gate at the mutation seam: a blank/whitespace name is
/// rejected (`None`) so `set_league` — the advertised user-choice sink — can
/// never write `Some("")` and defeat the fail-closed contract. Pure so it is
/// directly unit-testable without an `AppHandle`.
fn normalize_league(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Write via the sole holder, then deliver the snapshot before a change event.
///
/// Blank/whitespace names are rejected here (see `normalize_league`) so
/// fail-closed cannot be defeated at the seam: nothing changed, so no state
/// is written and no `ssot-changed` nudge is emitted. Locks are taken and
/// dropped inside `write_league`; `emit_ssot` re-locks briefly to clone the
/// snapshot. Nothing is held across the emit.
fn apply_league(app: &AppHandle, name: String) {
    let Some(name) = normalize_league(&name) else {
        // Blank/whitespace: reject before `write_league`, so `resolving` is left
        // untouched (not cleared). A manual `set_league("")` is not the retry
        // loop and must not flip the loop's flag; the loop clears `resolving`
        // itself when it lands a real name. Nothing is written, nothing emitted.
        return;
    };
    let changed = {
        let state = app.state::<AppState>();
        write_league(&state.ssot, name)
    };
    publish_league_resolution(app, changed);
}

fn publish_league_resolution(app: &AppHandle, changed: Option<LeagueChanged>) {
    // The temple market poll keys every payload on this name (POE-258), so a
    // league landing is the moment its first poll can succeed. Woken
    // unconditionally rather than on a CHANGE: the two servers behind the
    // DEBUG/PROD toggle usually name the same league, and that switch still has
    // to re-fetch, because the prices came from the other one.
    deliver_league_resolution(changed, || TEMPLE_MARKET_NOTIFY.notify_one(), || emit_ssot(app), |changed| {
        if let Err(error) = app.emit("league-changed", changed) {
            log::warn!("emit league-changed failed: {}", error);
        }
    });
}

fn deliver_league_resolution(
    changed: Option<LeagueChanged>,
    wake_market: impl FnOnce(),
    snapshot: impl FnOnce(),
    consumer_change: impl FnOnce(LeagueChanged),
) {
    wake_market();
    snapshot();
    if let Some(changed) = changed { consumer_change(changed); }
}

/// Extract the league name from a `/api/analysis/status` JSON body.
///
/// Returns `None` when the `league` field is absent, non-string, or blank so
/// the caller **fails closed** (the server omits `league` entirely when its
/// cache is not yet populated — see `AnalysisStatus`). A `None` here leaves the
/// SSOT unresolved and the trade client failing every lookup, by design.
pub(crate) fn parse_league_from_status(body: &serde_json::Value) -> Option<String> {
    body.get("league")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// One league fetch attempt. Return true only for an accepted status response;
/// failures and superseded server requests retry. Clone the HTTP client and
/// request identity before awaiting, with no lock across the round-trip.
async fn fetch_league_once(app: &AppHandle) -> bool {
    let (server_url, http, url_generation) = {
        let state = app.state::<AppState>();
        let url = crate::settings::server_base(&state);
        let generation = LEAGUE_SCHEDULER.task.lock().unwrap_or_else(|e| e.into_inner()).url_generation;
        (url, state.server_http.clone(), generation)
    };
    let Some(name) = fetch_league_from_server(&http, &server_url).await else {
        return false;
    };
    let state = app.state::<AppState>();
    let Some(changed) = accept_for_current_server(&state, &LEAGUE_SCHEDULER.task, (&server_url, url_generation), name)
    else {
        return false;
    };
    publish_league_resolution(app, changed);
    true
}

/// Holds the URL owner through acceptance: `set_server_url` writes the URL
/// before it bumps `url_generation`, so a response read against the old URL in
/// that gap would otherwise still match.
fn accept_for_current_server(
    state: &AppState,
    task: &std::sync::Mutex<LeagueTask>,
    requested: (&str, u64),
    name: String,
) -> Option<Option<LeagueChanged>> {
    crate::settings::with_server_base(state, |current_url| {
        let task = task.lock().unwrap_or_else(|e| e.into_inner());
        accept_league_response(&state.ssot, (current_url, task.url_generation), requested, name)
    })
}

fn accept_league_response(
    ssot: &std::sync::Mutex<AppSsotSnapshot>,
    current: (&str, u64),
    requested: (&str, u64),
    name: String,
) -> Option<Option<LeagueChanged>> {
    if current != requested { return None; }
    Some(write_league(ssot, name))
}

async fn fetch_league_from_server(http: &reqwest::Client, server_url: &str) -> Option<String> {
    let url = format!("{}/api/analysis/status", server_url);

    let resp = match http.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("league fetch: request to {} failed: {}", url, e);
            return None;
        }
    };
    if !resp.status().is_success() {
        log::warn!("league fetch: {} returned {}", url, resp.status());
        return None;
    }
    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("league fetch: bad JSON from {}: {}", url, e);
            return None;
        }
    };
    parse_league_from_status(&body)
}

/// Exercise Refresh's authoritative status read without constructing windows.
#[cfg(test)]
pub(crate) async fn refresh_league_for_test(
    server_url: &str,
    ssot: &std::sync::Mutex<AppSsotSnapshot>,
) {
    mark_resolving(ssot);
    let league = fetch_league_from_server(&reqwest::Client::new(), server_url)
        .await.expect("Refresh must resolve the stub's league");
    write_league(ssot, league);
}

/// Inject network and clock at the production loop boundary. Trigger
/// bookkeeping and Notify waiting remain shared with the application.
trait LeagueRuntime {
    fn scheduler(&self) -> &LeagueScheduler;
    fn begin(&self);
    async fn fetch(&self) -> bool;
    fn failed(&self, failures: u32);
    async fn sleep(&self, delay: Duration);
}

async fn run_league_resolver(runtime: &impl LeagueRuntime) {
    let mut backoff = LEAGUE_FETCH_INITIAL_BACKOFF;
    let mut failures = 0;
    loop {
        runtime.scheduler().begin_request();
        runtime.begin();
        if runtime.fetch().await {
            failures = 0;
            backoff = LEAGUE_FETCH_INITIAL_BACKOFF;
            runtime.scheduler().wait(None, runtime).await;
        } else {
            failures += 1;
            runtime.failed(failures);
            if runtime.scheduler().wait(Some(backoff), runtime).await {
                backoff = LEAGUE_FETCH_INITIAL_BACKOFF;
            } else {
                backoff = (backoff * 2).min(LEAGUE_FETCH_MAX_BACKOFF);
            }
        }
    }
}

struct AppLeagueRuntime(AppHandle);

impl LeagueRuntime for AppLeagueRuntime {
    fn scheduler(&self) -> &LeagueScheduler { &LEAGUE_SCHEDULER }

    fn begin(&self) {
        let state = self.0.state::<AppState>();
        mark_resolving(&state.ssot);
        emit_ssot(&self.0);
    }

    async fn fetch(&self) -> bool {
        fetch_league_once(&self.0).await
    }

    fn failed(&self, failures: u32) {
        if should_flag_unreachable(failures) {
            let changed = {
                let state = self.0.state::<AppState>();
                let mut snapshot = state.ssot.lock().unwrap_or_else(|e| e.into_inner());
                let changed = !snapshot.unreachable;
                snapshot.unreachable = true;
                changed
            };
            if changed { emit_ssot(&self.0); }
        }
    }

    async fn sleep(&self, delay: Duration) {
        tokio::time::sleep(delay).await;
    }
}

/// Start one live service. After success it waits for a trigger, never a poll.
pub fn spawn_league_fetch(app: AppHandle) {
    let spawn = LEAGUE_SCHEDULER.trigger();
    if !spawn { return; }
    tauri::async_runtime::spawn(async move {
        run_league_resolver(&AppLeagueRuntime(app)).await;
    });
}

/// Retained POE-126 command seam; no user-choice UI currently calls it.
#[tauri::command]
pub fn set_league(name: String, app: AppHandle) {
    // The server remains authoritative: an in-flight response can replace this
    // explicit mutation. User selection semantics remain deferred to POE-126.
    {
        let state = app.state::<AppState>();
        let in_flight = state.ssot.lock().unwrap_or_else(|e| e.into_inner()).resolving;
        if in_flight {
            log::warn!(
                "set_league({:?}) during resolution; the server response may replace this value (POE-126)",
                name
            );
        }
    }
    apply_league(&app, name);
}

/// Settings Refresh wakes the shared service, including during retry backoff.
#[tauri::command]
pub fn refresh_league(app: AppHandle) {
    rearm_league(app);
}

/// Window-independent access to the existing resolver's state and scheduling.
/// Tests observe the scheduling boundary without replacing hint dispatch.
pub(crate) trait LeagueResolver {
    fn current_league(&self) -> Option<String>;
    fn rearm(self);
}

impl LeagueResolver for AppHandle {
    fn current_league(&self) -> Option<String> {
        let state = self.state::<AppState>();
        let snapshot = state.ssot.lock().unwrap_or_else(|e| e.into_inner());
        snapshot.league.name.clone()
    }

    fn rearm(self) {
        rearm_league(self);
    }
}

/// Server hints only trigger a status read; they never relabel captured prices.
pub(crate) fn note_server_league(resolver: impl LeagueResolver, observed: Option<&str>) {
    let current = resolver.current_league();
    if observed.is_none() || observed != current.as_deref() {
        resolver.rearm();
    }
}

pub mod league_command {
    use super::*;

    #[tauri::command]
    pub fn note_server_league(app: AppHandle, observed: Option<String>) {
        super::note_server_league(app, observed.as_deref());
    }
}

/// Resolve the league again, now.
///
/// Startup, Settings Refresh, URL changes, write 409s and Mercure hints share
/// one scheduling gate. Liveness stays true through both idle and backoff.
pub(crate) fn rearm_league(app: AppHandle) {
    spawn_league_fetch(app);
}

/// Supersede any league fetch in flight: its answer came from the server the
/// app is leaving. The first step of a server-url change (`lib.rs`).
pub(crate) fn supersede_league_fetches() {
    LEAGUE_SCHEDULER.task.lock().unwrap_or_else(|e| e.into_inner()).url_generation += 1;
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The resolved identity belongs to the SSOT slice alone.
    #[test]
    fn write_league_updates_the_ssot_holder() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        assert_eq!(ssot.lock().unwrap().league.name, None);

        write_league(&ssot, "Mirage".to_string());

        assert_eq!(
            ssot.lock().unwrap().league.name,
            Some("Mirage".to_string()),
            "SSOT slice must hold the resolved league",
        );
        assert_eq!(ssot.lock().unwrap().league.generation, 1);
    }

    /// Single-flight contract: the first caller acquires the resolve right and
    /// marks `resolving`; a second caller is refused while a resolve is in
    /// flight, so repeated `refresh_league` clicks can't stack duplicate loops.
    #[test]
    fn resolver_liveness_is_independent_of_resolving_ui() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        let mut task = LeagueTask::default();
        assert!(task.trigger());
        mark_resolving(&ssot);
        write_league(&ssot, "A".into());
        assert!(!ssot.lock().unwrap().resolving);
        assert!(!task.trigger(), "idle task retains the single-flight claim");
    }

    /// The resolve path clears `resolving` in the same write that sets the
    /// league: starting from an in-flight snapshot, `write_league` must land the
    /// league AND drop the flag, so a poller never sees league-set + resolving.
    #[test]
    fn write_league_clears_resolving_when_it_sets_the_league() {
        let ssot = Mutex::new(AppSsotSnapshot {
            resolving: true,
            ..Default::default()
        });
        // Precondition: resolve in flight, league not yet written.
        assert!(ssot.lock().unwrap().resolving);
        assert_eq!(ssot.lock().unwrap().league.name, None);

        write_league(&ssot, "Mirage".to_string());

        let guard = ssot.lock().unwrap();
        assert_eq!(
            guard.league.name,
            Some("Mirage".to_string()),
            "league must be written",
        );
        assert!(
            !guard.resolving,
            "resolving must be cleared in the same write that sets the league",
        );
    }

    /// A blank/whitespace league name is rejected at the seam: `normalize_league`
    /// returns `None`, so the holder stays fail-closed.
    /// Guards the `set_league`/`refresh_league` user-choice sink that skips the
    /// server-side `parse_league_from_status` filter.
    #[test]
    fn blank_league_at_seam_leaves_holder_unresolved() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        assert_eq!(ssot.lock().unwrap().league.name, None);

        // The guard `apply_league` applies before writing. Whitespace-only.
        match normalize_league("   ") {
            None => {} // rejected — no write, exactly what `apply_league` does.
            Some(n) => { write_league(&ssot, n); }
        }

        assert_eq!(
            ssot.lock().unwrap().league.name,
            None,
            "SSOT slice must stay unresolved after a blank set_league",
        );
        assert_eq!(ssot.lock().unwrap().league.generation, 0);
    }

    /// The pure seam gate: whitespace-only trims to empty and is rejected,
    /// while a padded real name is accepted and trimmed. Locks the exact
    /// blank-rejection contract `apply_league` relies on.
    #[test]
    fn normalize_league_rejects_blank_and_trims_real() {
        assert_eq!(normalize_league(""), None);
        assert_eq!(normalize_league("   "), None);
        assert_eq!(normalize_league("\t\n"), None);
        assert_eq!(normalize_league("  Settlers  "), Some("Settlers".to_string()));
    }

    /// The sole success-exit mutation drops BOTH flags. Starting from an
    /// in-flight, unreachable snapshot (the exact state a recovered-after-down
    /// server exits through), `write_league` must leave `resolving` and
    /// `unreachable` both false — asserted on real post-state, not a return code.
    #[test]
    fn successful_resolution_drops_both_flags() {
        let ssot = Mutex::new(AppSsotSnapshot {
            resolving: true,
            unreachable: true,
            ..Default::default()
        });

        write_league(&ssot, "Mirage".into());

        let guard = ssot.lock().unwrap();
        assert!(!guard.resolving, "resolving must be cleared on success");
        assert!(!guard.unreachable, "unreachable must be cleared on success");
    }

    /// Threshold boundary for the unreachable flip: below the constant stays
    /// false, at/above flips true. Locks the grace window so a transient blip of
    /// a few failures does not flag the server down.
    #[test]
    fn should_flag_unreachable_at_threshold_boundary() {
        assert!(!should_flag_unreachable(0));
        assert!(!should_flag_unreachable(LEAGUE_UNREACHABLE_AFTER_ATTEMPTS - 1));
        assert!(should_flag_unreachable(LEAGUE_UNREACHABLE_AFTER_ATTEMPTS));
        assert!(should_flag_unreachable(LEAGUE_UNREACHABLE_AFTER_ATTEMPTS + 1));
    }

    #[test]
    fn parse_league_pulls_league_field() {
        let body = serde_json::json!({ "cached": true, "league": "Mirage" });
        assert_eq!(parse_league_from_status(&body), Some("Mirage".to_string()));
    }

    #[test]
    fn parse_league_missing_field_is_unresolved() {
        // Server omits `league` entirely when its cache is empty.
        let body = serde_json::json!({ "cached": false, "transfigure": 0 });
        assert_eq!(parse_league_from_status(&body), None);
    }

    #[test]
    fn parse_league_blank_field_is_unresolved() {
        // A blank/whitespace league must fail closed, not resolve to "".
        assert_eq!(
            parse_league_from_status(&serde_json::json!({ "league": "" })),
            None,
        );
        assert_eq!(
            parse_league_from_status(&serde_json::json!({ "league": "   " })),
            None,
        );
    }

    #[test]
    fn parse_league_non_string_field_is_unresolved() {
        assert_eq!(
            parse_league_from_status(&serde_json::json!({ "league": 42 })),
            None,
        );
    }
}

#[cfg(test)]
mod league_generation_tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn first_resolution_establishes_generation_without_consumer_event() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        assert!(write_league(&ssot, "A".into()).is_none());
        let snapshot = ssot.lock().unwrap();
        assert_eq!(snapshot.league.name.as_deref(), Some("A"));
        assert_eq!(snapshot.league.generation, 1);
    }

    #[test]
    fn identical_resolution_keeps_generation() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "A".into());
        assert!(write_league(&ssot, "A".into()).is_none());
        assert_eq!(ssot.lock().unwrap().league.generation, 1);
    }

    #[test]
    fn returning_to_a_previous_name_advances_generation() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "A".into());
        let changed = write_league(&ssot, "B".into()).unwrap();
        assert_eq!((changed.league.as_str(), changed.generation), ("B", 2));
        let changed = write_league(&ssot, "A".into()).unwrap();
        assert_eq!((changed.league.as_str(), changed.generation), ("A", 3));
    }
}

#[cfg(test)]
mod league_resolver_tests {
    use super::*;
    use std::future::Future;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll, Wake, Waker};

    struct NoopWake;
    impl Wake for NoopWake { fn wake(self: Arc<Self>) {} }

    struct Runtime {
        scheduler: LeagueScheduler,
        ssot: Mutex<AppSsotSnapshot>,
        replies: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<Option<&'static str>>>,
        waits: Mutex<Vec<Duration>>,
        attempts: Mutex<usize>,
        clock: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<()>>,
    }

    impl LeagueRuntime for Runtime {
        fn scheduler(&self) -> &LeagueScheduler { &self.scheduler }
        fn begin(&self) { mark_resolving(&self.ssot); }
        async fn fetch(&self) -> bool {
            *self.attempts.lock().unwrap() += 1;
            let reply = self.replies.lock().await.recv().await.expect("network remains open");
            match reply {
                Some(name) => {
                    accept_league_response(&self.ssot, ("server", 0), ("server", 0), name.into());
                    true
                }
                None => false,
            }
        }
        fn failed(&self, failures: u32) {
            if should_flag_unreachable(failures) { self.ssot.lock().unwrap().unreachable = true; }
        }
        async fn sleep(&self, delay: Duration) {
            self.waits.lock().unwrap().push(delay);
            self.clock.lock().await.recv().await.expect("clock remains open");
        }
    }

    fn runtime(replies: Vec<Option<&'static str>>) -> (
        Runtime,
        tokio::sync::mpsc::UnboundedSender<()>,
        tokio::sync::mpsc::UnboundedSender<Option<&'static str>>,
    ) {
        let (clock, receiver) = tokio::sync::mpsc::unbounded_channel();
        let (network, responses) = tokio::sync::mpsc::unbounded_channel();
        for reply in replies { network.send(reply).unwrap(); }
        let scheduler = LeagueScheduler::new();
        assert!(scheduler.trigger(), "startup claims the sole resolver task");
        (Runtime {
            scheduler,
            ssot: Mutex::new(AppSsotSnapshot::default()),
            replies: tokio::sync::Mutex::new(responses), waits: Mutex::new(Vec::new()),
            attempts: Mutex::new(0), clock: tokio::sync::Mutex::new(receiver),
        }, clock, network)
    }

    fn advance(future: std::pin::Pin<&mut impl Future<Output = ()>>) {
        let waker = Waker::from(Arc::new(NoopWake));
        assert!(matches!(future.poll(&mut Context::from_waker(&waker)), Poll::Pending));
    }

    #[test]
    fn success_waits_indefinitely_for_each_isolated_trigger() {
        let (runtime, _clock, _network) = runtime(vec![Some("A"), Some("B"), Some("B"), Some("B")]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        // Polling the executor without a trigger represents arbitrarily much
        // idle time: the production loop must not even arm a status timer.
        for _ in 0..100 { advance(resolver.as_mut()); }
        assert_eq!(*runtime.attempts.lock().unwrap(), 1);
        assert!(runtime.waits.lock().unwrap().is_empty(), "idle must not arm a timer");
        for attempt in 2..=4 {
            // The shared production scheduler receives all trigger sources.
            assert!(!runtime.scheduler.trigger());
            advance(resolver.as_mut());
            assert_eq!(*runtime.attempts.lock().unwrap(), attempt);
            assert!(runtime.waits.lock().unwrap().is_empty());
            assert!(!runtime.ssot.lock().unwrap().resolving);
        }
        assert_eq!(runtime.ssot.lock().unwrap().league.name.as_deref(), Some("B"));
        assert_eq!(runtime.ssot.lock().unwrap().league.generation, 2);
    }

    #[test]
    fn failures_retry_with_capped_backoff_until_recovery() {
        let (runtime, clock, _network) = runtime(vec![None, None, None, None, None, None, None, None, Some("A")]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        for _ in 0..7 { clock.send(()).unwrap(); advance(resolver.as_mut()); }
        assert_eq!(*runtime.waits.lock().unwrap(), [2, 4, 8, 16, 32, 60, 60, 60].map(Duration::from_secs));
        assert!(runtime.ssot.lock().unwrap().unreachable);
        clock.send(()).unwrap();
        advance(resolver.as_mut());
        assert_eq!(runtime.waits.lock().unwrap().len(), 8, "recovery arms no idle timer");
        let snapshot = runtime.ssot.lock().unwrap();
        assert_eq!(snapshot.league.name.as_deref(), Some("A"));
        assert!(!snapshot.unreachable);
        assert!(!snapshot.resolving);
    }

    #[test]
    fn trigger_wakes_backoff_without_erasing_the_resolved_baseline() {
        let (runtime, clock, _network) = runtime(vec![Some("A"), None, None, Some("B")]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        runtime.scheduler.trigger(); advance(resolver.as_mut());
        clock.send(()).unwrap(); advance(resolver.as_mut());
        assert_eq!(runtime.waits.lock().unwrap().last(), Some(&Duration::from_secs(4)));
        let snapshot = runtime.ssot.lock().unwrap();
        assert_eq!((snapshot.league.name.as_deref(), snapshot.league.generation), (Some("A"), 1));
        drop(snapshot);
        runtime.scheduler.trigger(); advance(resolver.as_mut());
        assert_eq!(runtime.waits.lock().unwrap().len(), 2, "recovery arms no idle timer");
        assert_eq!(runtime.ssot.lock().unwrap().league.generation, 2);
    }

    #[test]
    fn trigger_during_backoff_resets_the_retry_delay() {
        let (runtime, clock, _network) = runtime(vec![None, None, None]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        clock.send(()).unwrap(); advance(resolver.as_mut());
        runtime.scheduler.trigger(); advance(resolver.as_mut());
        assert_eq!(*runtime.waits.lock().unwrap(), [2, 4, 2].map(Duration::from_secs));
    }

    #[test]
    fn concurrent_hints_claim_only_one_live_resolver() {
        let task = Arc::new(Mutex::new(LeagueTask::default()));
        let workers: Vec<_> = (0..16).map(|_| {
            let task = task.clone();
            std::thread::spawn(move || task.lock().unwrap().trigger())
        }).collect();
        let spawned = workers.into_iter().map(|worker| usize::from(worker.join().unwrap())).sum::<usize>();
        assert_eq!(spawned, 1);
        assert!(task.lock().unwrap().pending);
        // A resolved UI flag does not release the live task's claim.
        assert!(!task.lock().unwrap().trigger());
    }

    #[test]
    fn superseded_server_response_cannot_replace_current_resolution() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "B".into());
        assert!(accept_league_response(&ssot, ("new-server", 1), ("old-server", 0), "A".into()).is_none());
        let snapshot = ssot.lock().unwrap();
        assert_eq!((snapshot.league.name.as_deref(), snapshot.league.generation), (Some("B"), 1));
    }

    #[test]
    fn returning_to_a_server_url_still_rejects_its_old_response() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "B".into());
        assert!(accept_league_response(&ssot, ("server", 2), ("server", 0), "A".into()).is_none());
        let snapshot = ssot.lock().unwrap();
        assert_eq!((snapshot.league.name.as_deref(), snapshot.league.generation), (Some("B"), 1));
    }

    #[test]
    fn a_server_url_write_waits_for_league_acceptance_to_finish() {
        let state = std::sync::Arc::new(crate::settings::tests::test_app_state());
        let owner = &state.server_url;
        *owner.lock().unwrap() = "https://a.example".into();
        let task = std::sync::Arc::new(Mutex::new(LeagueTask::default()));
        let blocked_task = task.lock().unwrap();
        let acceptance = {
            let (state, task) = (state.clone(), task.clone());
            std::thread::spawn(move || {
                accept_for_current_server(&state, &task, ("https://a.example", 0), "A".into()).is_some()
            })
        };
        let owner_held = || matches!(owner.try_lock(), Err(std::sync::TryLockError::WouldBlock));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !owner_held() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        let held_while_waiting = owner_held();
        drop(blocked_task);
        assert!(acceptance.join().unwrap(), "the response for the still-current URL is accepted");
        assert!(held_while_waiting, "a URL write could land between the current-URL read and acceptance");
    }

    struct Hint<'a>(&'a Runtime);
    impl LeagueResolver for Hint<'_> {
        fn current_league(&self) -> Option<String> { self.0.ssot.lock().unwrap().league.name.clone() }
        fn rearm(self) { self.0.scheduler.trigger(); }
    }

    #[test]
    fn event_hint_resolves_authoritative_status_instead_of_applying_the_hint() {
        let (runtime, _clock, _network) = runtime(vec![Some("A"), Some("authoritative")]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        note_server_league(Hint(&runtime), Some("untrusted-hint"));
        assert_eq!(runtime.ssot.lock().unwrap().league.name.as_deref(), Some("A"));
        advance(resolver.as_mut());
        assert_eq!(*runtime.attempts.lock().unwrap(), 2);
        assert_eq!(runtime.ssot.lock().unwrap().league.name.as_deref(), Some("authoritative"));
        assert!(runtime.waits.lock().unwrap().is_empty());
    }

    enum PendingTrigger { HintsAndRefresh, Unknown, Refresh }

    fn parked_response_preserves_trigger(trigger: PendingTrigger) {
        let (runtime, _clock, network) = runtime(vec![Some("A")]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        assert_eq!(runtime.ssot.lock().unwrap().league.name.as_deref(), Some("A"));
        // Start the parked A request from a real idle Notify wait. Removing
        // the production wake must break this regression before any response.
        assert!(!runtime.scheduler.trigger(), "Refresh wakes the existing idle task");
        advance(resolver.as_mut());
        assert_eq!(*runtime.attempts.lock().unwrap(), 2, "Refresh's A request is parked on the network");

        match trigger {
            PendingTrigger::HintsAndRefresh => {
                note_server_league(Hint(&runtime), Some("B"));
                note_server_league(Hint(&runtime), None);
                assert!(!runtime.scheduler.trigger(), "Refresh wakes the existing task");
            }
            PendingTrigger::Unknown => note_server_league(Hint(&runtime), None),
            PendingTrigger::Refresh => {
                assert!(!runtime.scheduler.trigger(), "Refresh wakes the existing task");
            }
        }
        assert_eq!(runtime.ssot.lock().unwrap().league.name.as_deref(), Some("A"));
        network.send(Some("A")).unwrap();
        advance(resolver.as_mut());
        assert_eq!(*runtime.attempts.lock().unwrap(), 3, "a trigger received during A requires a subsequent authoritative request");

        network.send(Some("B")).unwrap();
        advance(resolver.as_mut());
        let snapshot = runtime.ssot.lock().unwrap();
        assert_eq!((snapshot.league.name.as_deref(), snapshot.league.generation), (Some("B"), 2));
        assert!(!snapshot.resolving);
        drop(snapshot);
        for _ in 0..100 { advance(resolver.as_mut()); }
        assert_eq!(*runtime.attempts.lock().unwrap(), 3, "all prior triggers coalesce into the B request, then idle");
        assert!(runtime.waits.lock().unwrap().is_empty(), "successful requests never arm a timer");
    }

    #[test]
    fn hints_and_refresh_during_a_request_resolve_again_before_idle() {
        parked_response_preserves_trigger(PendingTrigger::HintsAndRefresh);
    }

    #[test]
    fn unknown_hint_during_a_request_resolves_again_before_idle() {
        parked_response_preserves_trigger(PendingTrigger::Unknown);
    }

    #[test]
    fn refresh_during_a_request_resolves_again_before_idle() {
        parked_response_preserves_trigger(PendingTrigger::Refresh);
    }
}

#[cfg(test)]
mod league_publication_tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::Mutex;

    fn resolve(ssot: &Mutex<AppSsotSnapshot>, name: &str) -> Vec<String> {
        let changed = write_league(ssot, name.into());
        let events = RefCell::new(Vec::new());
        deliver_league_resolution(changed,
            || { events.borrow_mut().push("market-wake".into()); },
            || {
                // The snapshot emitter must be able to re-lock the holder.
                let snapshot = ssot.try_lock().expect("no lock across emission");
                events.borrow_mut().push(format!("ssot:{}:{}", snapshot.league.name.as_deref().unwrap(), snapshot.league.generation));
            },
            |changed| { events.borrow_mut().push(format!("changed:{}:{}", changed.league, changed.generation)); });
        events.into_inner()
    }

    #[test]
    fn first_resolution_publishes_snapshot_without_consumer_change() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        assert_eq!(resolve(&ssot, "A"), ["market-wake", "ssot:A:1"]);
    }

    #[test]
    fn real_change_publishes_one_consumer_event_after_snapshot() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "A".into());
        assert_eq!(resolve(&ssot, "B"), ["market-wake", "ssot:B:2", "changed:B:2"]);
    }

    #[test]
    fn same_name_resolution_still_wakes_market() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "A".into());
        assert_eq!(resolve(&ssot, "A"), ["market-wake", "ssot:A:1"]);
    }
}
