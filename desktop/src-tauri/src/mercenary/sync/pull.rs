use super::*;

// ---------------------------------------------------------------------------
// Pull
// ---------------------------------------------------------------------------

/// What a finished pull came back with.
#[derive(Debug, Clone, PartialEq)]
pub enum PullOutcome {
    /// The server's corpus is byte-identical to the one this store was merged
    /// from — nothing to do.
    NotModified,
    /// A corpus, and the ETag to send next time.
    Corpus(PooledCorpus, Option<String>),
}

/// One pull attempt.
///
/// `None` for every failure — offline, non-2xx, unparseable body — because
/// there is no error a caller can act on differently: the answer to all of them
/// is "keep the local store and say so once". Same shape and same reason as
/// `ssot::fetch_league_once`.
pub async fn pull_once(app: &AppHandle, etag: Option<&str>) -> Option<PullOutcome> {
    let (server, http) = server_and_http(app);
    let url = format!("{server}/api/desktop/merc-templates?format_version={FORMAT_VERSION}");

    let mut request = http.get(&url).timeout(REQUEST_TIMEOUT);
    if let Some(tag) = etag {
        request = request.header(reqwest::header::IF_NONE_MATCH, tag);
    }
    let response = match request.send().await {
        Ok(r) => r,
        Err(e) => {
            note_failure(app, format!("pull: {e}"));
            return None;
        }
    };
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        note_success(app);
        return Some(PullOutcome::NotModified);
    }
    if !response.status().is_success() {
        note_failure(app, format!("pull: server returned {}", response.status()));
        return None;
    }
    let served_etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body: CorpusBody = match response.json().await {
        Ok(b) => b,
        Err(e) => {
            note_failure(app, format!("pull: unreadable corpus — {e}"));
            return None;
        }
    };
    note_success(app);
    let (corpus, dropped) = decode_corpus(body);
    if dropped > 0 {
        crate::app_log(
            app,
            format!("Merc: {dropped} pooled template(s) did not decode and were skipped"),
        );
    }
    Some(PullOutcome::Corpus(corpus, served_etag))
}

/// Start the module-start pull.
///
/// Two claims are taken here and they are NOT the same thing:
///
/// - the REQUEST claim (`pulling`) is single-flight — a module toggled
///   repeatedly gets one pull, not one per toggle;
/// - the SEAM claim (`startup_claim`) says a capture loop is starting up and
///   will apply whatever lands. It is taken unconditionally — including when
///   the request itself was not claimed, because a pull from a previous module
///   start is still in flight or because this start's registration reset
///   suppressed the ask. Those are the cases it exists for: an off→on toggle
///   across an in-flight pull used to leave the claim unset, so the corpus
///   applied itself against a store the restarting loop had not installed yet.
///
/// The caller stays on its own thread — this returns immediately.
pub fn spawn_pull(app: &AppHandle) {
    if !with_state(app, |s| claim_pull(s, super::super::run::now_ms())) {
        return;
    }
    tauri::async_runtime::spawn(pull_and_apply(app.clone()));
}

/// Start a pull because a recruit window was just detected (POE-210).
///
/// The other half of "a learning made on one machine reaches the others while
/// both are playing": module start alone means a device sees a hover made ten
/// minutes ago only after the player restarts the module. Opening the window is
/// the moment the corpus is worth having, and it is the user's own framing of
/// "refresh".
///
/// Two things it is NOT. It never touches `startup_claim` — that flag belongs
/// to the load seam, and setting it here would park the corpus for a seam that
/// finished minutes ago and lose it. And it is throttled by
/// [`REPULL_THROTTLE_MS`], because the detect it hangs off fires on window
/// churn as well as on a real open.
///
/// EDGE-TRIGGERED, and nothing re-arms it. A detect refused here — by the
/// in-flight guard or by the throttle — is not remembered and does not fire a
/// pull once the window passes; the next attempt is the next `Captured`
/// transition, which means the next time a recruit window is opened (or
/// replaced, or re-detected after a retire) past the throttle. A player who
/// opens one window and keeps it open for an hour therefore pulls once. That is
/// the intended cost of hanging off the transition rather than off a timer: the
/// corpus is worth having when a window OPENS, and a device that never opens
/// one has nothing to match against anyway.
///
/// The caller stays on its own thread — this returns immediately.
pub fn spawn_repull(app: &AppHandle) {
    if !with_state(app, |s| claim_repull(s, super::super::run::now_ms())) {
        return;
    }
    tauri::async_runtime::spawn(pull_and_apply(app.clone()));
}

/// Start a pull because the Data & Privacy notice was just accepted (POE-395).
///
/// Every pull before acceptance was refused at the server client's gate, but
/// the refused startup pull still stamped the throttle clock. This takes the
/// re-pull's claim without [`REPULL_THROTTLE_MS`], so accepting within a minute
/// of module start still pulls. Single-flight and suppression rules hold.
///
/// The caller stays on its own thread — this returns immediately.
pub fn spawn_accept_pull(app: &AppHandle) {
    if !with_state(app, |s| claim_accept_pull(s, super::super::run::now_ms())) {
        return;
    }
    tauri::async_runtime::spawn(pull_and_apply(app.clone()));
}

/// [`claim_repull`] minus the throttle.
pub(super) fn claim_accept_pull(state: &mut SyncState, now_ms: u64) -> bool {
    if state.pull_suppressed_this_start || state.pulling {
        return false;
    }
    state.pulling = true;
    state.last_pull_started_ms = now_ms;
    true
}

/// Claim the module-start pull. `true` when the caller owns the REQUEST.
///
/// Pure over [`SyncState`] because the two claims it takes are the load seam's
/// half of the handshake `record_pull` and `claim_step` complete, and because
/// the SEAM claim is set on BOTH answers — a `false` here still means a capture
/// loop is starting.
pub(super) fn claim_pull(state: &mut SyncState, now_ms: u64) -> bool {
    // The seam is starting either way; only the request may already be
    // somebody else's.
    state.startup_claim = true;
    // AFTER the seam claim, and that order is load-bearing (POE-215 D5).
    // Suppression stops this start from ASKING. It cannot recall a pull a
    // PREVIOUS module start already issued, and that corpus — built from the
    // very pre-fit art the reset just moved into `pre-fit/` — is the one that
    // would otherwise apply itself against a store this start has not installed
    // yet. The seam claim is what parks it instead; `record_pull` then drops it
    // because this start is suppressed.
    //
    // It costs a suppressed start nothing. `claim_step` reads "done" off
    // `!pulling`, never off the claim, so with no request in flight the seam's
    // FIRST look releases the claim and `wait_for_pull` returns immediately
    // rather than sitting out `STARTUP_WAIT`.
    if state.pull_suppressed_this_start {
        return false;
    }
    if state.pulling {
        return false;
    }
    state.pulling = true;
    // Stamped only when the request was actually taken: the in-flight pull this
    // one yielded to already stamped its own start, and re-stamping would push
    // the throttle window out for a request nobody made.
    state.last_pull_started_ms = now_ms;
    true
}

/// Claim a detect-driven re-pull. `true` when the caller owns the request.
///
/// Refused while a pull is in flight (the same single-flight rule the startup
/// pull takes) and inside [`REPULL_THROTTLE_MS`] of the last attempt of EITHER
/// kind — a re-pull one second after the module start's own pull would ask for
/// a corpus this device already has.
///
/// `saturating_sub`, so a clock that steps backwards reads as "no time has
/// passed" and stays throttled rather than wrapping into a decade of elapsed
/// time and letting every detect through.
///
/// Never sets `startup_claim`. Pure over [`SyncState`] for the same reason
/// [`claim_pull`] is.
pub(super) fn claim_repull(state: &mut SyncState, now_ms: u64) -> bool {
    // The re-pull is the other door into the same corpus, and it fires on the
    // next recruit window — well inside the start the reset suppressed.
    if state.pull_suppressed_this_start {
        return false;
    }
    if state.pulling {
        return false;
    }
    if now_ms.saturating_sub(state.last_pull_started_ms) < REPULL_THROTTLE_MS {
        return false;
    }
    state.pulling = true;
    state.last_pull_started_ms = now_ms;
    true
}

/// The tail both spawns share: pull, record, and apply the corpus if the pull
/// is the one that has to.
///
/// One function rather than two, because the seam handshake in `record_pull`
/// only works if every pull answers it the same way — a re-pull that skipped
/// the parked-corpus branch would drop a corpus the seam was waiting for.
pub(super) async fn pull_and_apply(app: AppHandle) {
    let etag = icons_dir(&app).map(|dir| SyncFile::load(&dir)).and_then(|f| f.etag);
    let outcome = pull_once(&app, etag.as_deref()).await;
    if let Some((corpus, served_etag)) = finish_pull(&app, outcome) {
        // The seam had already released its claim, so the store it installs
        // is in place and this is free to merge into it. Same template mutex
        // the seam takes, so there is one writer at a time either way — this
        // path just costs a generation bump the in-seam merge does not.
        let Ok(data_dir) = app.path().app_data_dir() else {
            return;
        };
        // The user's own `icon_match` decides what counts as art the store
        // already has, exactly as it does for a hover-learned sample.
        // Re-read rather than defaulted: a device running a moved threshold
        // must dedupe the pool with the number it matches with.
        let thresholds = super::super::load_override(&data_dir).0.thresholds;
        apply_corpus(
            &app,
            &data_dir.join(super::super::ICONS_DIR),
            &corpus,
            served_etag,
            &thresholds,
            true,
        );
    }
}

/// Record a finished pull. Returns the corpus (and its ETag) when the caller has
/// to apply it itself — the load seam already released its claim.
pub(super) fn finish_pull(
    app: &AppHandle,
    outcome: Option<PullOutcome>,
) -> Option<(PooledCorpus, Option<String>)> {
    let now = super::super::run::now_ms();
    let (to_apply, discarded) = with_state(app, |s| record_pull(s, now, outcome));
    if discarded {
        crate::app_log(
            app,
            "Merc: pool corpus discarded — this start reset the store; it re-pulls next start"
                .to_string(),
        );
    }
    crate::ssot::emit_ssot(app);
    to_apply
}

/// Fold a finished pull into the sync state, answering who applies the corpus
/// and whether it was DISCARDED.
///
/// `None` means the caller has nothing to do — the corpus was parked for a load
/// seam that is still holding its claim, or there was no corpus at all; `Some`
/// means the seam is gone and the caller must apply it against the store the
/// seam already installed.
///
/// The bool is the third answer, and it exists because suppression cannot be
/// enforced at the ASK alone (POE-215 D5): `claim_pull` and `claim_repull`
/// refuse to issue a request on a suppressed start, but a request a PREVIOUS
/// start issued is already on the wire and its corpus was built from the
/// pre-fit art this start's reset just moved out of the store. This is the
/// landing seam every pull passes through, so it is where that corpus is
/// dropped — neither parked nor handed back. Nothing records its ETag either,
/// because only [`apply_corpus`] does that, so the next start pulls in full.
///
/// Pure over [`SyncState`] because this handshake is the only thing standing
/// between a merged corpus and a whole-store write that would erase it.
pub(super) fn record_pull(
    state: &mut SyncState,
    now_ms: u64,
    outcome: Option<PullOutcome>,
) -> (Option<(PooledCorpus, Option<String>)>, bool) {
    state.pulling = false;
    if state.pull_suppressed_this_start && matches!(outcome, Some(PullOutcome::Corpus(..))) {
        // The page's pull status is left where it was — this session took
        // nothing from the pool, and `Never` is exactly that. Stamping
        // `last_pull_ms` here would date a merge that did not happen.
        return (None, true);
    }
    state.status.last_pull_ms = Some(now_ms);
    match outcome {
        None => {
            state.status.last_pull = PullResult::Failed;
            (None, false)
        }
        Some(PullOutcome::NotModified) => {
            state.status.last_pull = PullResult::Unchanged;
            (None, false)
        }
        Some(PullOutcome::Corpus(corpus, etag)) => {
            state.status.last_pull = PullResult::Merged;
            if state.startup_claim {
                state.landed = Some((corpus, etag));
                (None, false)
            } else {
                (Some((corpus, etag)), false)
            }
        }
    }
}

/// Store the ETag a merged corpus was served under.
///
/// Called only AFTER that corpus is in the store and on disk. Storing it at the
/// moment the pull finished would be a promise the device might not keep: a save
/// that fails leaves the tag naming a corpus the next start cannot see, and the
/// 304 it earns would keep the pool's art out forever.
///
/// Skipped when the merge held anything back for a pending forget — the
/// suppressed key still owes this device its served samples, and a 304 is the
/// one answer that can never deliver them.
pub fn persist_etag(app: &AppHandle, dir: &Path, tag: Option<String>, suppressed: usize) {
    let Some(tag) = tag else {
        return;
    };
    if suppressed > 0 {
        return;
    }
    let _guard = SYNC_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = SyncFile::load(dir);
    file.etag = Some(tag);
    if let Err(e) = file.save(dir) {
        crate::app_log(app, format!("Merc: could not save pool sync state — {e}"));
    }
}

/// One step of the load seam's wait, over the sync state.
///
/// Returns the parked corpus if there is one, and whether the caller must keep
/// waiting. The claim is dropped in the SAME acquisition that takes the corpus,
/// so a pull finishing between the two cannot fall through the gap: after this
/// returns "stop", every later corpus takes [`record_pull`]'s apply-it-yourself
/// branch against a store the seam has already installed.
///
/// Pure over [`SyncState`] for the same reason `record_pull` is — this is the
/// other half of one handshake.
pub(super) fn claim_step(
    state: &mut SyncState,
    expired: bool,
) -> (Option<(PooledCorpus, Option<String>)>, bool) {
    let landed = state.landed.take();
    // Nothing more to wait for once the corpus is in hand, the window has
    // closed, or the request itself is over (it failed, or it was somebody
    // else's and has already finished).
    let done = landed.is_some() || expired || !state.pulling;
    if done {
        state.startup_claim = false;
    }
    (landed, !done)
}

/// Wait out the load seam's window for a corpus, then release the seam claim.
///
/// Called from the capture loop's start AFTER the loaded store has been
/// installed into `merc.templates`. That ordering is the single-writer rule: the
/// store is in place before the claim drops, so whichever side ends up merging —
/// this one or a later [`spawn_pull`] task — merges into the installed store
/// under its mutex, and no merge can be erased by a whole-store write that
/// follows it.
///
/// Blocking, bounded by [`STARTUP_WAIT`] and by `cancel`: the loop's first tick
/// is a detect that only matters once a mercenary speaks, so a second of wait
/// costs the user nothing — but a module switched off during that second must
/// stop here rather than finish the window, which is the loop's own shutdown
/// invariant.
pub fn wait_for_pull(
    app: &AppHandle,
    cancel: &watch::Receiver<bool>,
) -> Option<(PooledCorpus, Option<String>)> {
    let deadline = Instant::now() + STARTUP_WAIT;
    loop {
        // A cancelled module expires the window rather than abandoning it: the
        // claim still has to drop, or a pull landing afterwards would park a
        // corpus nobody is waiting for and lose it.
        let expired = Instant::now() >= deadline || *cancel.borrow();
        let (landed, keep_waiting) = with_state(app, |s| claim_step(s, expired));
        if let Some(landed) = landed {
            return Some(landed);
        }
        if !keep_waiting {
            return None;
        }
        std::thread::sleep(STARTUP_POLL);
    }
}

/// Merge a corpus into the INSTALLED store, save, and publish.
///
/// The one merge path, shared by the load seam and by a pull that landed after
/// the seam let go. Both take `merc.templates` and do the whole read-modify-write
/// inside that one acquisition, so the merged store is never visible half-done
/// and never overwritten by a caller that loaded it earlier.
///
/// `bump` is false for the seam, whose session has not read the generation yet,
/// and true for the late path, where the loop is already re-applying
/// confirmations a tombstoned key must stop reaching.
pub fn apply_corpus(
    app: &AppHandle,
    dir: &Path,
    corpus: &PooledCorpus,
    etag: Option<String>,
    thresholds: &super::super::Thresholds,
    bump: bool,
) {
    let suppressed = SyncFile::load(dir).suppressed();

    let (outcome, learned, pooled, seeded, lost_seeds, pooled_samples, save_err) = {
        let state = app.state::<AppState>();
        // The DIRECTORY lock around the whole merge-and-write, outside the
        // store's own mutex: the loop's off-tick writer holds the store's mutex
        // only long enough to clone, so this write and that one would otherwise
        // interleave in the same directory. See `icons::writing_icons_dir` for
        // the lock order and what the interleave costs.
        super::super::icons::writing_icons_dir(&state.merc.icons_write, || {
            let mut store = state
                .merc.templates
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            // Snapshotted INSIDE the one acquisition that holds the merge:
            // `MergeOutcome` counts evictions but cannot name them, and a
            // snapshot taken outside could be separated from the merge by the
            // hover tick's own eviction — which would then be blocklisted twice
            // and, worse, attributed to the pool.
            let seeded = store.seeded_families();
            let outcome = store.merge_pulled(corpus, &suppressed, thresholds);
            let lost_seeds = store.seeds_lost_since(&seeded);
            let save_err = if outcome.changed() {
                store.save(dir).err()
            } else {
                None
            };
            (
                outcome,
                store.learned_keys(),
                store.pooled_keys(),
                store.seeded_families(),
                lost_seeds,
                store.pooled_samples(),
                save_err,
            )
        })
    };

    crate::app_log(app, merge_line(&outcome));
    // Every seed eviction blocklists, whichever door it came through (POE-208),
    // or the next module start re-derives the family from the art still in the
    // cache and the pool evicts it again. Outside the locks above, like the
    // hover tick's eviction and for the same reason — the served sample that
    // evicted the seed is normally in the store, so an install pass racing
    // this write yields to it; where it is not (refused later, or a full key)
    // the seed can return for one session and the entry written here retires
    // it at the next start. Lock order: `seed::SEED_BLOCKLIST_LOCK`.
    for family in &lost_seeds {
        super::super::debug::block_seed(app, dir, family);
    }
    match save_err {
        // The tag is stored only once the corpus it names is on disk — a tag
        // ahead of the store earns a 304 that keeps the pool's art out for good.
        None => persist_etag(app, dir, etag, outcome.suppressed),
        Some(e) => crate::app_log(app, format!("Merc: pooled templates not saved — {e}")),
    }
    with_state(app, |s| s.status.pooled_samples = pooled_samples);
    if bump && owes_bump(&outcome) {
        // The loop is holding confirmations from before the merge, and a
        // tombstoned key it still re-applies is the forget not working.
        super::super::debug::bump_generation(app);
    }
    super::super::run::publish(app, |slice| {
        slice.learned_families = learned;
        slice.pooled_families = pooled;
        slice.seeded_families = seeded;
    });
    // `pooled_samples` above is on the SYNC status, which `publish` does not
    // carry — it emits only when the mercenary slice's own fields moved. A
    // merge that added samples without adding a KEY (a second sample of a key
    // the store already had) changed nothing `publish` compares, so the page's
    // "N samples from the pool" sat on the previous number until something else
    // emitted. Cheap and unconditional rather than diffed, because the SSOT
    // snapshot is what decides whether an emit reaches the window at all.
    //
    // No lock is held here: the store and directory block closed above, and
    // `build_snapshot` takes `merc.sync` and `mercenary` only.
    crate::ssot::emit_ssot(app);
}

/// Whether a finished merge owes the session a generation bump.
///
/// `changed()` alone is not enough. It answers "is a SAVE owed" and excludes
/// [`super::icons::MergeOutcome::seeds_evicted`] on purpose — seeds are never
/// written to disk, so an eviction moves nothing there. It still moves the
/// STORE, and the loop is holding confirmations made against the seed that just
/// went: without the bump they are re-applied on the next detect and the
/// evicted family keeps answering under the name the pool just contradicted.
///
/// In practice the served sample's own install usually sets `added` as well, so
/// the two agree; the property this function protects is the case where it does
/// not.
pub fn owes_bump(outcome: &super::super::icons::MergeOutcome) -> bool {
    outcome.changed() || outcome.seeds_evicted > 0
}

/// One line describing a merge, for the log.
pub fn merge_line(outcome: &super::super::icons::MergeOutcome) -> String {
    if outcome.foreign_version {
        return format!(
            "Merc: pooled corpus declares another signature format — nothing merged (this build reads version {FORMAT_VERSION})"
        );
    }
    format!(
        "Merc: pool merged — {} added, {} replaced by a tombstone, {} already known, {} held for a pending forget, \
         {} refused as one art under two families ({} already-pooled sample(s) dropped with them), \
         {} seed(s) evicted",
        outcome.added,
        outcome.replaced,
        outcome.skipped,
        outcome.suppressed,
        outcome.conflicting,
        outcome.dropped,
        outcome.seeds_evicted,
    )
}
