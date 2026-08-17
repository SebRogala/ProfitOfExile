//! Module lifecycle registry (POE-128).
//!
//! A **module** is an optional background unit the user can switch off: one
//! registry entry, one spawn fn, one persisted enabled flag. Enable spawns it,
//! disable cancels it. `AppState.modules_enabled` is the single owner of the
//! flags (always the *effective* state — registry defaults overlaid with the
//! user's persisted choices), `settings.json` stores the delta, and the
//! `modules` SSOT slice publishes the map to every window (see src/ssot.rs).
//!
//! # Recipe — adding a module
//!
//! 1. Write `fn spawn_<id>(app: AppHandle, cancel: watch::Receiver<bool>) ->
//!    ModuleJoin` that stops when the receiver fires. Async work
//!    `tokio::select!`s on `cancel.changed()` (the log-watcher idiom); a
//!    dedicated OS thread reads `*cancel.borrow()` once per iteration.
//!    The reaper's `abort()` is a backstop, not a kill: it only lands at an
//!    `.await` point, so an async module that never yields survives it and
//!    keeps running — every module must yield.
//! 2. Add a `ModuleDef` to `MODULES` with an explicit `disabled_means` — there
//!    is no default, the choice is a product decision (below).
//! 3. Add the id to the Sidebar "Modules" display-name map so the toggle has a
//!    user-facing label.
//!
//! Persistence, the SSOT slice, and start/stop need no further edits: they all
//! key off `MODULES`.
//!
//! # `disabled_means` is load-bearing
//!
//! - `NoWork` — the flag governs the work itself. Disabled means the module
//!   does not run.
//! - `NoWindow` — the flag governs a window/page ONLY. The background work is
//!   **unconditional**: reconcile always starts it and never stops it, whatever
//!   the flag says. Windows are gated where windows are created, not here.
//!   Two failures this prevents symmetrically: a toggle silently killing
//!   collection, and collection silently never running because the user left
//!   the toggle off.
//!   **Honesty rule:** a `NoWindow` module's Sidebar row must say what the
//!   toggle actually governs ("hides the window; collection keeps running").
//!   A row that reads as an off-switch while work continues is a lie.
//!
//! # The slice is intent, not liveness
//!
//! `modules` reports what is *supposed* to be running. A module that panicked
//! or returned early is still reported enabled. Surfacing real liveness needs a
//! task-exit signal and is future work — do not read the slice as a health
//! check.
//!
//! A disabled module's own SSOT data slice (whatever state it later projects
//! into `AppSsotSnapshot`) stays registered as inert memory — data shape is
//! cheap, work is not. Disable aborts tasks, never unregisters state.
//!
//! # Thread modules must poll
//!
//! Threads cannot be aborted. A `ModuleJoin::Thread` module is signalled and
//! detached, so it must check `*cancel.borrow()` every iteration and keep any
//! single blocking call under `MODULE_THREAD_POLL_CEILING` (5 s — a picked
//! ceiling, no codebase precedent). A thread that blocks longer than that
//! outlives its stop by that much.
//!
//! # Lock order
//!
//! `module_handles` is acquired FIRST and `modules_enabled` is read inside it;
//! never the inverse. Everything inside that critical section is non-`await`
//! and acquires no other module lock — but it is NOT free of I/O: the log seam
//! is `crate::app_log` (a synchronous file append + emit), one line per
//! start/stop. Accepted trade — lifecycle lines must be reachable in shipped
//! builds and the volume is one line per toggle. Do not add heavier work.
//! `persist_settings` does disk I/O and runs strictly OUTSIDE it — never hoist
//! it inside to "satisfy" the order. Acquiring `modules_enabled` alone, with
//! `module_handles` NOT held, is fine anywhere (`from_state`, `apply_to_state`,
//! `ssot::build_snapshot` and `set_module_enabled` all do it).
//! Module-owned state Mutexes sit OUTSIDE this order and are acquired alone:
//! `AppState.mercenary` and `AppState.merc_templates` (POE-165) are taken by
//! the merc loop with no module lock held — and never together, so they have no
//! order between them. `ssot::build_snapshot` takes `mercenary` only after the
//! `modules_enabled` guard has been dropped; neither is ever taken inside
//! `module_handles`.
//!
//! # Cleanup never runs on app exit
//!
//! Main-window close reaches `std::process::exit(0)` before any grace window
//! elapses: reaper tasks and detached thread modules die mid-work. Stop
//! cleanup is best-effort on *toggle* only. No module may rely on its cancel
//! branch for durability — flush as you go.
//!
//! # Not modules (yet)
//!
//! The four lab overlay `enabled` flags and `lab_overlays_enabled` are
//! near-miss concepts with their own persistence and window logic. They are
//! NOT registered here and must not be folded in without a deliberate
//! migration.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio::sync::watch;

use crate::AppState;

/// The lifecycle's log sink. The reconcile core takes this instead of an
/// `AppHandle` so it stays testable, and production hands it a closure over
/// `crate::app_log` — the only log channel a shipped build can reach
/// (`windows_subsystem = "windows"` leaves `log::` output with nowhere to go).
///
/// `Arc<dyn Fn>` rather than `&mut impl FnMut` because the reaper is a detached
/// task: it needs an owned, `Send + Sync`, `'static` logger. One shape for both
/// paths beats two.
pub type ModuleLogger = Arc<dyn Fn(String) + Send + Sync>;

/// How long a stopped `ModuleJoin::Task` gets to exit on its own before the
/// reaper aborts it. Graceful exit within the grace wins; abort is the backstop.
const MODULE_STOP_GRACE: Duration = Duration::from_secs(2);

/// Ceiling on any single blocking call inside a `ModuleJoin::Thread` module.
/// Threads cannot be aborted, so this bounds how long one can outlive its stop
/// signal. A picked number, not a measured one.
///
/// The merc capture loop is the first thread module and honours it by waiting
/// in 100 ms slices (`mercenary::run::TICK`, asserted against this constant
/// there). Nothing reads it at runtime — it is the recipe's normative number
/// for the next thread module to be measured against, not a loose constant.
#[allow(dead_code)]
pub const MODULE_THREAD_POLL_CEILING: Duration = Duration::from_secs(5);

/// What a module's `enabled = false` actually turns off. See the module doc —
/// this branches `reconcile`, it is not documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisabledSemantics {
    /// Disabled means the background work does not run.
    NoWork,
    /// Disabled means the window/page is not created; the background work runs
    /// unconditionally and is stop-immune to the flag.
    ///
    /// No registered module uses it yet — `reconcile` branches on it and the
    /// tests cover it, so the semantics ship with the contract, not after it.
    #[allow(dead_code)]
    NoWindow,
}

/// The two task shapes a module may take. Both are admitted because the WinRT
/// apartment rule forces capture/OCR work onto a dedicated OS thread, where the
/// async runtime deadlocks (see `spawn_gem_scan` in lib.rs).
pub enum ModuleJoin {
    /// Async task on the Tauri runtime — abortable, so it gets a reaper.
    ///
    /// No registered module uses it: the only module today is merc OCR, which
    /// the WinRT apartment rule forces onto a thread. The variant stays because
    /// `reconcile`/`stop_module` implement its (different, reaped) stop path and
    /// the tests cover it — the next non-capture module is a `Task`.
    #[allow(dead_code)]
    Task(tauri::async_runtime::JoinHandle<()>),
    /// Dedicated OS thread — NOT abortable, signal-and-detach only. The shape
    /// the WinRT apartment rule forces on capture/OCR work; `mercenary` uses it.
    Thread(std::thread::JoinHandle<()>),
}

/// The lifecycle half of a registry entry: everything `reconcile`,
/// `effective_modules` and `persistable_modules` need, with no `AppHandle` and
/// no fn pointer. Embedded in `ModuleDef` rather than duplicated, so tests and
/// the pure functions consume real registry data instead of a hand-copied list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleDefLite {
    pub id: &'static str,
    pub default_enabled: bool,
    pub disabled_means: DisabledSemantics,
}

/// A registry entry: its lifecycle facts plus how to start it.
pub struct ModuleDef {
    pub lifecycle: ModuleDefLite,
    pub spawn: fn(AppHandle, watch::Receiver<bool>) -> ModuleJoin,
}

/// The registry. Adding an entry here is the only registration step — the
/// settings delta, the SSOT slice and start/stop all derive from it.
pub const MODULES: &[ModuleDef] = &[ModuleDef {
    lifecycle: ModuleDefLite {
        id: "mercenary",
        default_enabled: false,
        disabled_means: DisabledSemantics::NoWork,
    },
    spawn: spawn_mercenary,
}];

/// The registry's lifecycle facts. The single accessor the pure functions and
/// the settings touch points go through, so none of them re-derives the list.
pub fn module_lifecycles() -> Vec<ModuleDefLite> {
    MODULES.iter().map(|d| d.lifecycle).collect()
}

/// A running module: its cancel channel and the join shape to reap.
pub struct ModuleHandle {
    cancel: watch::Sender<bool>,
    join: ModuleJoin,
}

/// What `reconcile` decided. Ids only — the caller owns spawning.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReconcilePlan {
    pub start: Vec<String>,
    pub stop: Vec<String>,
}

/// Diff desired enablement against what is running. Pure — no locks, no
/// `AppHandle`, no side effects — so the whole semantics matrix is directly
/// unit-testable.
///
/// `shutting_down` is a distinct input, NOT a desired-map rewrite: it bypasses
/// the semantics branch entirely (start nothing, stop everything running). An
/// all-off desired map could not express this, because `NoWindow` is
/// structurally stop-immune to the flag.
///
/// Desired keys with no registry entry are ignored; running ids with no
/// registry entry are left alone outside shutdown (they cannot occur — starts
/// come from `defs` — but shutdown still sweeps them).
fn reconcile(
    desired: &HashMap<String, bool>,
    running: &HashSet<&str>,
    defs: &[ModuleDefLite],
    shutting_down: bool,
) -> ReconcilePlan {
    if shutting_down {
        let mut stop: Vec<String> = running.iter().map(|id| (*id).to_string()).collect();
        // HashSet order is arbitrary; sort so the stop sequence is reproducible.
        stop.sort();
        return ReconcilePlan { start: Vec::new(), stop };
    }
    let mut plan = ReconcilePlan::default();
    for def in defs {
        let is_running = running.contains(def.id);
        let want_running = match def.disabled_means {
            DisabledSemantics::NoWork => {
                desired.get(def.id).copied().unwrap_or(def.default_enabled)
            }
            DisabledSemantics::NoWindow => true,
        };
        if want_running && !is_running {
            plan.start.push(def.id.to_string());
        } else if !want_running && is_running {
            plan.stop.push(def.id.to_string());
        }
    }
    plan
}

/// Registry defaults overlaid with the persisted map — the *effective* state
/// `AppState.modules_enabled` holds from birth. Unknown persisted keys survive
/// verbatim so a downgrade/upgrade round-trip does not eat a future module's
/// choice.
pub fn effective_modules(
    persisted: &HashMap<String, bool>,
    defs: &[ModuleDefLite],
) -> HashMap<String, bool> {
    let mut out: HashMap<String, bool> = defs
        .iter()
        .map(|d| (d.id.to_string(), d.default_enabled))
        .collect();
    for (id, enabled) in persisted {
        out.insert(id.clone(), *enabled);
    }
    out
}

/// The inverse: what actually goes to disk. Only entries that DIFFER from the
/// registry default are persisted, plus unknown keys verbatim.
///
/// Consequence, by design: toggling a module back to its default value means
/// "follow the default", not "pin it" — a later version that flips
/// `default_enabled` reaches users who never made an explicit choice.
pub fn persistable_modules(
    owner: &HashMap<String, bool>,
    defs: &[ModuleDefLite],
) -> HashMap<String, bool> {
    owner
        .iter()
        .filter(|(id, enabled)| match defs.iter().find(|d| d.id == id.as_str()) {
            Some(def) => **enabled != def.default_enabled,
            None => true,
        })
        .map(|(id, enabled)| (id.clone(), *enabled))
        .collect()
}

/// Whether `id` names a registered module. Validate-on-write: the setter
/// rejects an unknown id rather than storing a key nothing will ever start.
fn validate_module_id(id: &str, defs: &[ModuleDefLite]) -> Result<(), String> {
    if defs.iter().any(|d| d.id == id) {
        Ok(())
    } else {
        Err(format!("unknown module id: {}", id))
    }
}

/// Signal a module to stop and let go of it.
///
/// A failed `send` means every receiver is already gone, which the two shapes
/// read differently: a `Task` may simply have exited on its own (the reaper is
/// the backstop either way), but a `Thread` cannot be aborted, so a receiver-less
/// thread is unstoppable and that is worth saying out loud.
///
/// For a `Task`, a detached reaper waits out `MODULE_STOP_GRACE`, aborts
/// unconditionally (abort on a finished task is a documented no-op) and then
/// reports what actually happened: a panic, or a task that outlived its abort.
/// Nothing here blocks and no lock is held by the reaper.
fn stop_module(id: &str, handle: ModuleHandle, log: &ModuleLogger) {
    let ModuleHandle { cancel, join } = handle;
    let signalled = cancel.send(true);
    match join {
        ModuleJoin::Task(task) => {
            let id = id.to_string();
            let log = log.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(MODULE_STOP_GRACE).await;
                let overran_grace = !task.inner().is_finished();
                task.abort();
                if overran_grace {
                    log(format!(
                        "Module {}: abort issued after grace; task had not finished",
                        id
                    ));
                }
                // A task that never reaches an `.await` ignores `abort()`, and
                // awaiting it here would hang the reaper forever — bound it.
                match tokio::time::timeout(MODULE_STOP_GRACE, task).await {
                    Err(_) => log(format!(
                        "Module {}: task did not terminate after abort — possible duplicate on re-enable",
                        id
                    )),
                    Ok(Err(tauri::Error::JoinError(e))) if e.is_panic() => {
                        log(format!("Module {}: task panicked", id))
                    }
                    Ok(_) => {}
                }
            });
        }
        ModuleJoin::Thread(_join) => {
            // Signal-and-detach: threads cannot be aborted, so the handle is
            // dropped here. The variant keeps the JoinHandle as the seam for a
            // future join-with-grace stop path (module-exit diagnostics).
            if signalled.is_err() {
                log(format!(
                    "Module {}: stop signal had no receiver — thread may run until exit",
                    id
                ));
            }
        }
    }
    log(format!("Module {}: stop signalled", id));
}

/// The reconcile-and-apply core, without `AppHandle` or locks so the lifecycle
/// is testable with closure spawners.
///
/// Each start gets a FRESH `watch` pair, so a re-enable can never inherit the
/// previous generation's already-fired cancel.
fn apply_reconcile_with(
    handles: &mut HashMap<String, ModuleHandle>,
    desired: &HashMap<String, bool>,
    defs: &[ModuleDefLite],
    shutting_down: bool,
    log: &ModuleLogger,
    mut spawner: impl FnMut(&str, watch::Receiver<bool>) -> ModuleJoin,
) {
    let plan = {
        let running: HashSet<&str> = handles.keys().map(String::as_str).collect();
        reconcile(desired, &running, defs, shutting_down)
    };
    for id in &plan.stop {
        if let Some(handle) = handles.remove(id) {
            stop_module(id, handle, log);
        }
    }
    for id in &plan.start {
        let (cancel, rx) = watch::channel(false);
        let join = spawner(id, rx);
        handles.insert(id.clone(), ModuleHandle { cancel, join });
        log(format!("Module {}: started", id));
    }
}

/// Bring running modules in line with `AppState.modules_enabled`.
///
/// The ONE critical section: `module_handles` is held across the whole diff and
/// every start/stop, and `modules_enabled` is read inside it (lock order — see
/// the module doc). Nothing inside blocks or awaits.
pub fn apply_reconcile(app: &AppHandle) {
    let state = app.state::<AppState>();
    let defs = module_lifecycles();
    let app_for_spawn = app.clone();
    let app_for_log = app.clone();
    let log: ModuleLogger = Arc::new(move |msg: String| crate::app_log(&app_for_log, msg));

    let mut handles = state.module_handles.lock().unwrap_or_else(|e| e.into_inner());
    // Read the latch INSIDE the critical section: a close racing this call must
    // either be seen here or serialize behind it, never be read stale and then
    // acted on with a start.
    let shutting_down = state.modules_shutting_down.load(Ordering::SeqCst);
    let desired = state
        .modules_enabled
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    apply_reconcile_with(&mut handles, &desired, &defs, shutting_down, &log, |id, rx| {
        // Unreachable by construction: `reconcile` draws its start set from
        // `defs`, which is `MODULES` itself.
        let def = MODULES
            .iter()
            .find(|d| d.lifecycle.id == id)
            .expect("reconcile started an id that is not in MODULES");
        (def.spawn)(app_for_spawn.clone(), rx)
    });
}

/// Set a module's enabled flag: owner map, disk, running set, then the nudge.
///
/// The nudge is last so an eager re-fetch reads post-reconcile state.
/// `persist_settings` runs between the owner-map write and `apply_reconcile`,
/// outside the `module_handles` critical section (lock order — see module doc).
#[tauri::command]
pub fn set_module_enabled(id: String, enabled: bool, app: AppHandle) -> Result<(), String> {
    if let Err(e) = validate_module_id(&id, &module_lifecycles()) {
        // The caller sees the Err; without this the rejection leaves no trace
        // anywhere the user or a log dump can reach it.
        crate::app_log(&app, format!("Module toggle rejected: {}", e));
        return Err(e);
    }
    {
        let state = app.state::<AppState>();
        state
            .modules_enabled
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.clone(), enabled);
    }
    crate::persist_settings(&app);
    apply_reconcile(&app);
    crate::ssot::emit_ssot(&app);
    Ok(())
}

/// The merc OCR capture module (POE-165). A `Thread`, not a `Task`: screen
/// capture and `Windows.Media.Ocr` are apartment-threaded and deadlock on the
/// async runtime (see `spawn_gem_scan` in lib.rs).
///
/// Threads are signalled and detached, never aborted, so the loop's own poll
/// discipline is the whole stop mechanism — `mercenary::run` waits in 100 ms
/// slices, well inside `MODULE_THREAD_POLL_CEILING`.
fn spawn_mercenary(app: AppHandle, cancel: watch::Receiver<bool>) -> ModuleJoin {
    crate::mercenary::run::spawn(app, cancel)
}

// Known gap: the stop BACKSTOP — the reaper's grace sleep, its `abort()`, the
// panic report and the did-not-terminate timeout — is exercised only indirectly
// here. Those branches live in a detached task on the global Tauri runtime and
// only fire after `MODULE_STOP_GRACE` of wall clock, which a unit test can
// neither drive nor observe (tokio's time pause does not reach that runtime).
// The tests below cover everything up to the detach; changes inside the reaper
// need manual scrutiny, not test cover, and should be re-read against
// `stop_module`'s doc comment.
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const NOWORK: ModuleDefLite = ModuleDefLite {
        id: "nowork",
        default_enabled: false,
        disabled_means: DisabledSemantics::NoWork,
    };
    const NOWORK_ON_BY_DEFAULT: ModuleDefLite = ModuleDefLite {
        id: "nowork_on",
        default_enabled: true,
        disabled_means: DisabledSemantics::NoWork,
    };
    const NOWINDOW: ModuleDefLite = ModuleDefLite {
        id: "nowindow",
        default_enabled: false,
        disabled_means: DisabledSemantics::NoWindow,
    };

    fn desired(pairs: &[(&str, bool)]) -> HashMap<String, bool> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn running<'a>(ids: &[&'a str]) -> HashSet<&'a str> {
        ids.iter().copied().collect()
    }

    /// The logger for tests that do not read the log.
    fn silent() -> ModuleLogger {
        Arc::new(|_| {})
    }

    /// A logger plus the lines it collected.
    fn collecting_logger() -> (ModuleLogger, Arc<Mutex<Vec<String>>>) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        (Arc::new(move |msg: String| sink.lock().unwrap().push(msg)), lines)
    }

    // --- reconcile: the semantics matrix -------------------------------------

    #[test]
    fn reconcile_starts_an_enabled_module_that_is_not_running() {
        let plan = reconcile(&desired(&[("nowork", true)]), &running(&[]), &[NOWORK], false);

        assert_eq!(plan.start, vec!["nowork".to_string()]);
        assert!(plan.stop.is_empty(), "nothing was running to stop");
    }

    #[test]
    fn reconcile_stops_a_disabled_nowork_module_that_is_running() {
        let plan = reconcile(
            &desired(&[("nowork", false)]),
            &running(&["nowork"]),
            &[NOWORK],
            false,
        );

        assert_eq!(plan.stop, vec!["nowork".to_string()]);
        assert!(plan.start.is_empty(), "a running module must not be restarted");
    }

    #[test]
    fn reconcile_leaves_an_enabled_module_that_is_already_running() {
        let plan = reconcile(
            &desired(&[("nowork", true)]),
            &running(&["nowork"]),
            &[NOWORK],
            false,
        );

        assert_eq!(plan, ReconcilePlan::default(), "no work when state matches");
    }

    #[test]
    fn reconcile_ignores_a_desired_key_with_no_registry_entry() {
        let plan = reconcile(&desired(&[("ghost", true)]), &running(&[]), &[NOWORK], false);

        assert!(
            plan.start.is_empty(),
            "an unregistered id has no spawn fn and must never be started",
        );
    }

    #[test]
    fn reconcile_falls_back_to_default_enabled_when_desired_has_no_entry() {
        let plan = reconcile(&desired(&[]), &running(&[]), &[NOWORK_ON_BY_DEFAULT], false);

        assert_eq!(
            plan.start,
            vec!["nowork_on".to_string()],
            "an absent flag means the registry default, not off",
        );
    }

    #[test]
    fn reconcile_starts_a_nowindow_module_whose_flag_is_off() {
        let plan = reconcile(
            &desired(&[("nowindow", false)]),
            &running(&[]),
            &[NOWINDOW],
            false,
        );

        assert_eq!(
            plan.start,
            vec!["nowindow".to_string()],
            "NoWindow work is unconditional — the flag governs the window only",
        );
    }

    #[test]
    fn reconcile_keeps_a_running_nowindow_module_whose_flag_is_off() {
        let plan = reconcile(
            &desired(&[("nowindow", false)]),
            &running(&["nowindow"]),
            &[NOWINDOW],
            false,
        );

        assert!(
            plan.stop.is_empty(),
            "a NoWindow module must be stop-immune to its flag",
        );
    }

    #[test]
    fn shutdown_reconcile_stops_a_running_nowindow_module() {
        let plan = reconcile(
            &desired(&[("nowindow", false)]),
            &running(&["nowindow"]),
            &[NOWINDOW],
            true,
        );

        assert_eq!(
            plan.stop,
            vec!["nowindow".to_string()],
            "shutdown bypasses the semantics branch and stops everything",
        );
    }

    #[test]
    fn shutdown_reconcile_starts_nothing_it_would_otherwise_start() {
        let plan = reconcile(
            &desired(&[("nowork", true)]),
            &running(&[]),
            &[NOWORK, NOWINDOW],
            true,
        );

        assert!(
            plan.start.is_empty(),
            "a set_module_enabled racing shutdown must not respawn",
        );
    }

    // --- effective_modules / persistable_modules ------------------------------

    #[test]
    fn effective_modules_uses_the_registry_default_for_an_absent_key() {
        let out = effective_modules(&desired(&[]), &[NOWORK_ON_BY_DEFAULT]);

        assert_eq!(out.get("nowork_on"), Some(&true));
    }

    #[test]
    fn effective_modules_lets_a_persisted_value_override_the_default() {
        let out = effective_modules(&desired(&[("nowork_on", false)]), &[NOWORK_ON_BY_DEFAULT]);

        assert_eq!(out.get("nowork_on"), Some(&false));
    }

    #[test]
    fn effective_modules_preserves_a_persisted_key_with_no_registry_entry() {
        let out = effective_modules(&desired(&[("from_the_future", true)]), &[NOWORK]);

        assert_eq!(
            out.get("from_the_future"),
            Some(&true),
            "an unknown key must survive so a downgrade does not eat the choice",
        );
    }

    #[test]
    fn persistable_modules_drops_an_entry_equal_to_its_default() {
        let out = persistable_modules(&desired(&[("nowork", false)]), &[NOWORK]);

        assert!(
            !out.contains_key("nowork"),
            "settings.json must not pin an unchosen default",
        );
    }

    #[test]
    fn persistable_modules_keeps_an_entry_that_differs_from_its_default() {
        let out = persistable_modules(&desired(&[("nowork", true)]), &[NOWORK]);

        assert_eq!(out.get("nowork"), Some(&true));
    }

    #[test]
    fn persistable_modules_keeps_a_key_with_no_registry_entry_verbatim() {
        let out = persistable_modules(&desired(&[("from_the_future", false)]), &[NOWORK]);

        assert_eq!(out.get("from_the_future"), Some(&false));
    }

    /// The delta is only useful if it survives a load. Real registry data, so
    /// this breaks if `MODULES` and the two pure functions ever disagree.
    #[test]
    fn a_non_default_choice_survives_the_persist_then_load_round_trip() {
        let defs = module_lifecycles();
        let chosen = desired(&[("mercenary", true)]);

        let on_disk = persistable_modules(&chosen, &defs);
        let reloaded = effective_modules(&on_disk, &defs);

        assert_eq!(reloaded.get("mercenary"), Some(&true));
    }

    // --- validation -----------------------------------------------------------

    #[test]
    fn validate_module_id_rejects_an_unregistered_id() {
        let err = validate_module_id("not_a_module", &module_lifecycles())
            .expect_err("an unregistered id must be rejected on write");

        assert!(
            err.contains("not_a_module"),
            "the error must name the rejected id, got {:?}",
            err,
        );
    }

    #[test]
    fn validate_module_id_accepts_a_registered_id() {
        assert!(validate_module_id("mercenary", &module_lifecycles()).is_ok());
    }

    /// The id is the key for the owner map, the settings delta, the SSOT slice
    /// and the running set. Two entries sharing one would silently make the
    /// second unreachable — `find` returns the first, and the running set can
    /// only ever hold one of them.
    #[test]
    fn every_registered_module_has_a_unique_id() {
        let ids: Vec<&str> = module_lifecycles().iter().map(|d| d.id).collect();

        let unique: HashSet<&str> = ids.iter().copied().collect();

        assert_eq!(unique.len(), ids.len(), "duplicate module id in MODULES: {:?}", ids);
    }

    // --- apply_reconcile_with: the lifecycle ----------------------------------

    /// Records the receiver handed to each spawn generation, so tests can assert
    /// on the cancel state a module actually observed.
    #[derive(Default)]
    struct SpawnLog {
        receivers: Vec<watch::Receiver<bool>>,
    }

    fn recording_spawner(
        log: Arc<Mutex<SpawnLog>>,
    ) -> impl FnMut(&str, watch::Receiver<bool>) -> ModuleJoin {
        move |_id, rx| {
            log.lock().unwrap().receivers.push(rx.clone());
            let mut rx = rx;
            ModuleJoin::Task(tauri::async_runtime::spawn(async move {
                let _ = rx.changed().await;
            }))
        }
    }

    #[tokio::test]
    async fn stopping_a_module_cancels_the_generation_it_detaches() {
        let log = Arc::new(Mutex::new(SpawnLog::default()));
        let mut handles: HashMap<String, ModuleHandle> = HashMap::new();

        apply_reconcile_with(
            &mut handles,
            &desired(&[("nowork", true)]),
            &[NOWORK],
            false,
            &silent(),
            recording_spawner(log.clone()),
        );
        assert!(handles.contains_key("nowork"), "precondition: started");
        assert!(
            !*log.lock().unwrap().receivers[0].borrow(),
            "precondition: the live generation is not cancelled",
        );

        apply_reconcile_with(
            &mut handles,
            &desired(&[("nowork", false)]),
            &[NOWORK],
            false,
            &silent(),
            recording_spawner(log.clone()),
        );

        assert!(
            *log.lock().unwrap().receivers[0].borrow(),
            "the cancel must be sent before the handle is let go",
        );
        assert!(
            !handles.contains_key("nowork"),
            "a stopped module must leave the running set",
        );
    }

    #[tokio::test]
    async fn re_enabling_a_module_hands_the_new_generation_an_uncancelled_receiver() {
        let log = Arc::new(Mutex::new(SpawnLog::default()));
        let mut handles: HashMap<String, ModuleHandle> = HashMap::new();

        for enabled in [true, false, true] {
            apply_reconcile_with(
                &mut handles,
                &desired(&[("nowork", enabled)]),
                &[NOWORK],
                false,
                &silent(),
                recording_spawner(log.clone()),
            );
        }

        let log = log.lock().unwrap();
        assert_eq!(log.receivers.len(), 2, "expected exactly two spawn generations");
        assert!(
            !*log.receivers[1].borrow(),
            "the re-enabled generation must get a fresh watch pair, not a fired one",
        );
    }

    #[tokio::test]
    async fn reconciling_twice_against_an_unchanged_desired_map_does_not_respawn() {
        let log = Arc::new(Mutex::new(SpawnLog::default()));
        let mut handles: HashMap<String, ModuleHandle> = HashMap::new();
        let want = desired(&[("nowork", true)]);

        apply_reconcile_with(&mut handles, &want, &[NOWORK], false, &silent(), recording_spawner(log.clone()));
        apply_reconcile_with(&mut handles, &want, &[NOWORK], false, &silent(), recording_spawner(log.clone()));

        assert_eq!(
            log.lock().unwrap().receivers.len(),
            1,
            "an already-running module must not be spawned a second time",
        );
    }

    #[tokio::test]
    async fn shutdown_apply_cancels_and_drops_a_stop_immune_nowindow_module() {
        let log = Arc::new(Mutex::new(SpawnLog::default()));
        let mut handles: HashMap<String, ModuleHandle> = HashMap::new();

        apply_reconcile_with(
            &mut handles,
            &desired(&[("nowindow", false)]),
            &[NOWINDOW],
            false,
            &silent(),
            recording_spawner(log.clone()),
        );
        assert!(handles.contains_key("nowindow"), "precondition: running");

        apply_reconcile_with(
            &mut handles,
            &desired(&[("nowindow", false)]),
            &[NOWINDOW],
            true,
            &silent(),
            recording_spawner(log.clone()),
        );

        assert!(handles.is_empty(), "shutdown must clear the running set");
        assert!(
            *log.lock().unwrap().receivers[0].borrow(),
            "shutdown must cancel even a flag-immune NoWindow module",
        );
    }

    // --- what the lifecycle says out loud --------------------------------------

    /// A start that leaves no trace is a start nobody can debug. The seam must
    /// be called on the spawn path, not just on the stop path.
    #[tokio::test]
    async fn starting_a_module_announces_it_on_the_log_seam() {
        let spawns = Arc::new(Mutex::new(SpawnLog::default()));
        let (log, lines) = collecting_logger();
        let mut handles: HashMap<String, ModuleHandle> = HashMap::new();

        apply_reconcile_with(
            &mut handles,
            &desired(&[("nowork", true)]),
            &[NOWORK],
            false,
            &log,
            recording_spawner(spawns),
        );

        assert_eq!(
            *lines.lock().unwrap(),
            vec!["Module nowork: started".to_string()],
        );
    }

    /// The stop counterpart: the signal is sent inside the critical section but
    /// the module dies later, so this line is the only in-app evidence that a
    /// stop was ever issued.
    #[tokio::test]
    async fn stopping_a_module_announces_the_signal_on_the_log_seam() {
        let spawns = Arc::new(Mutex::new(SpawnLog::default()));
        let (log, lines) = collecting_logger();
        let mut handles: HashMap<String, ModuleHandle> = HashMap::new();

        apply_reconcile_with(
            &mut handles,
            &desired(&[("nowork", true)]),
            &[NOWORK],
            false,
            &log,
            recording_spawner(spawns.clone()),
        );
        lines.lock().unwrap().clear();

        apply_reconcile_with(
            &mut handles,
            &desired(&[("nowork", false)]),
            &[NOWORK],
            false,
            &log,
            recording_spawner(spawns),
        );

        assert_eq!(
            *lines.lock().unwrap(),
            vec!["Module nowork: stop signalled".to_string()],
        );
    }

    /// A `Thread` module whose receiver is gone cannot be stopped at all —
    /// threads are not abortable, so the failed `send` is the last chance to
    /// say so. The `Task` arm deliberately stays quiet: its reaper aborts.
    #[test]
    fn stopping_a_thread_module_with_no_receiver_left_warns_that_it_is_unstoppable() {
        let (cancel, rx) = watch::channel(false);
        drop(rx);
        let (log, lines) = collecting_logger();
        let handle = ModuleHandle {
            cancel,
            join: ModuleJoin::Thread(std::thread::spawn(|| {})),
        };

        stop_module("threadmod", handle, &log);

        assert!(
            lines
                .lock()
                .unwrap()
                .contains(&"Module threadmod: stop signal had no receiver — thread may run until exit".to_string()),
            "expected the unstoppable-thread warning, got {:?}",
            lines.lock().unwrap(),
        );
    }
}
