//! Opening one capture-loop session.
//!
//! `run` owns the loop. This child owns what happens once per module start
//! before the first tick: the geometry read, the template store opened in the
//! order [`open_template_store`] holds, and the [`Session`] the loop carries.
//! The store half talks to the app only through [`SessionOpenPorts`], so its
//! ordering is pinned by the tests below rather than only by comments.

use std::path::Path;
use std::sync::Mutex;

use super::*;
use super::super::icons::{PooledCorpus, PurgedStore};

/// The two locks and the directory the store open writes under.
///
/// Lock order is `dir_lock` (`writing_icons_dir`) then `store`
/// (`merc.templates`), the order every other writer takes.
pub(super) struct StoreHandles<'a> {
    pub(super) dir: &'a Path,
    pub(super) dir_lock: &'a Mutex<()>,
    pub(super) store: &'a Mutex<TemplateStore>,
}

/// What opening the store leaves for the rest of the session start.
pub(super) struct StoreOpened {
    /// One line per index entry the load could not read.
    pub(super) problems: Vec<String>,
}

/// The app-bound effects of opening the template store.
pub(super) trait SessionOpenPorts {
    fn log(&mut self, line: String);
    /// `sync::begin_session`.
    fn begin_session(&mut self);
    /// `sync::suppress_pull_this_start`.
    fn suppress_pull(&mut self);
    /// `sync::spawn_pull`.
    fn spawn_pull(&mut self);
    /// `seed::spawn_fetch`.
    fn spawn_seed_fetch(&mut self);
    /// `sync::record_tombstones`. Takes `SYNC_FILE_LOCK`, so it is called
    /// with neither store lock held.
    fn record_tombstones(&mut self, keys: &[(String, u8)]);
    /// `seed::wait_for_install`: seeds into the installed store.
    fn install_seeds(&mut self, h: &StoreHandles);
    /// `sync::wait_for_pull`: the corpus this start's pull brought, if any.
    fn take_corpus(&mut self) -> Option<(PooledCorpus, Option<String>)>;
    /// `sync::apply_corpus`: merge into the installed store and save.
    fn apply_corpus(&mut self, h: &StoreHandles, corpus: &PooledCorpus, etag: Option<String>);
    /// `sync::set_pooled_samples`.
    fn set_pooled_samples(&mut self, count: usize);
}

/// The real ports: today's calls, against the running app.
struct AppPorts<'a> {
    app: &'a AppHandle,
    cancel: &'a watch::Receiver<bool>,
    geometry: &'a MercGeometry,
}

impl SessionOpenPorts for AppPorts<'_> {
    fn log(&mut self, line: String) {
        crate::app_log(self.app, line);
    }
    fn begin_session(&mut self) {
        sync::begin_session(self.app);
    }
    fn suppress_pull(&mut self) {
        sync::suppress_pull_this_start(self.app);
    }
    fn spawn_pull(&mut self) {
        sync::spawn_pull(self.app);
    }
    fn spawn_seed_fetch(&mut self) {
        seed::spawn_fetch(self.app);
    }
    fn record_tombstones(&mut self, keys: &[(String, u8)]) {
        sync::record_tombstones(self.app, keys);
    }
    fn install_seeds(&mut self, h: &StoreHandles) {
        seed::wait_for_install(self.app, self.cancel, h.dir, self.geometry);
    }
    fn take_corpus(&mut self) -> Option<(PooledCorpus, Option<String>)> {
        sync::wait_for_pull(self.app, self.cancel)
    }
    fn apply_corpus(&mut self, h: &StoreHandles, corpus: &PooledCorpus, etag: Option<String>) {
        sync::apply_corpus(self.app, h.dir, corpus, etag, &self.geometry.thresholds, false);
    }
    fn set_pooled_samples(&mut self, count: usize) {
        sync::set_pooled_samples(self.app, count);
    }
}

/// What a store purge says in the log.
///
/// The version the old index declared, not a hard-coded "format 1": that is
/// the case every current install hits, but a downgrade meets a later index
/// and a half-written one parses as nothing, and the three have different
/// causes. A line that named the wrong one would send the next reader after
/// an upgrade that never happened.
fn purge_log_line(purged: &PurgedStore) -> String {
    match purged.version {
        Some(version) => format!(
            "Merc: dropped {} format-{version} template(s) (format {})",
            purged.dropped,
            sync::FORMAT_VERSION,
        ),
        None => format!(
            "Merc: dropped an unreadable template index (format {})",
            sync::FORMAT_VERSION,
        ),
    }
}

/// The tombstones a re-keying load owes the pool, deduplicated (POE-211).
///
/// Only a LOCAL sample the pool had already accepted owes one: that is the
/// only case where THIS device put art on the server under the old key. A
/// pooled sample under the old name was somebody else's upload — retiring it
/// from here would delete their art on a name this device merely read — and an
/// unuploaded local sample was never on the server at all.
///
/// The dedup is the point of returning keys rather than the entries: one key
/// can hold [`super::super::icons::TemplateStore::MAX_SAMPLES_PER_KEY`]
/// samples, and a tombstone is keyed `(version, family, tier)`, so three
/// re-keyed samples of one key owe ONE POST, not three of the same one.
fn tombstones_owed(rekeyed: &[Rekeyed]) -> Vec<(String, u8)> {
    let mut owed: Vec<(String, u8)> = Vec::new();
    for entry in rekeyed.iter().filter(|r| r.uploaded_local) {
        let key = (entry.old_family.clone(), entry.tier);
        if !owed.contains(&key) {
            owed.push(key);
        }
    }
    owed
}

/// The one line a re-keying load writes to the log (POE-211).
///
/// It names both halves of the fold and the tombstone count, because those are
/// the two things the Windows smoke check reads: that the migration ran at all
/// (it runs ONCE — the next start finds no old family in the index), and that
/// the pool was told about the key it retired.
///
/// `not_removed` — the old files [`super::super::icons::persist_rekey`] could
/// not unlink — is appended only when it is non-zero. An orphan no index names
/// costs nothing to read past, but a directory that keeps growing across starts
/// is a bug nobody could report off a log that never mentioned it.
fn rekey_log_line(rekeyed: &[Rekeyed], not_removed: usize) -> String {
    let mut folds: Vec<String> = Vec::new();
    for entry in rekeyed {
        let fold = format!("{} to {}", entry.old_family, entry.new_family);
        if !folds.contains(&fold) {
            folds.push(fold);
        }
    }
    let line = format!(
        "Merc: re-keyed {} sample(s) from {} ({} tombstone(s) offered)",
        rekeyed.len(),
        folds.join(", "),
        tombstones_owed(rekeyed).len(),
    );
    if not_removed == 0 {
        line
    } else {
        format!("{line}, {not_removed} file(s) not removed")
    }
}

/// Open the session: geometry, template store, vocabulary, and the
/// [`Session`] the loop carries. `Err` is the reason the loop parks as
/// unavailable.
pub(super) fn open_session(
    app: &AppHandle,
    cancel: &watch::Receiver<bool>,
) -> Result<Session, String> {
    crate::app_log(app, "Merc: capture loop started".to_string());
    crate::report_ocr_engine(app);

    // The gate outlives the loop — it is `AppState`, and the module can be
    // switched off and on again inside one session. Whatever it was holding
    // when the last loop stopped is about a screen from before the module was
    // off: an armed probe would fire on this loop's first tick for a window
    // that closed minutes ago, and a Scan now would be served long after the
    // player stopped waiting for it. A fresh loop starts owing nothing.
    trigger::disarm(app);

    let data_dir = app.path().app_data_dir().ok();
    let (geometry, geometry_source, geometry_err) = match &data_dir {
        Some(dir) => super::super::load_override(dir),
        None => (
            MercGeometry::default(),
            super::super::GEOMETRY_SOURCE_DEFAULT,
            Some("no app data directory — geometry override cannot be read".to_string()),
        ),
    };
    if let Some(err) = &geometry_err {
        crate::app_log(app, format!("Merc: {err}"));
    }
    crate::app_log(
        app,
        format!("Merc: geometry source {geometry_source} (row pitch {:.1})", geometry.row_pitch),
    );
    for warning in matcher_geometry_warnings(&geometry) {
        crate::app_log(app, warning);
    }

    // Load the learned templates before the first detect, so a restart does not
    // re-report every already-confirmed cell as unknown.
    let icons_dir = data_dir.as_ref().map(|d| d.join(super::super::ICONS_DIR));
    let mut template_problems = Vec::new();
    if let Some(dir) = &icons_dir {
        let state = app.state::<AppState>();
        let handles = StoreHandles {
            dir,
            dir_lock: &state.merc.icons_write,
            store: &state.merc.templates,
        };
        // Read off `pool-sync.json`, which the purge that runs first inside
        // `open_template_store` does not touch.
        let needs_reset = sync::needs_registration_reset(dir);
        let mut ports = AppPorts { app, cancel, geometry: &geometry };
        template_problems = open_template_store(&handles, needs_reset, &mut ports).problems;
    }
    for problem in &template_problems {
        crate::app_log(app, format!("Merc: template store — {problem}"));
    }

    let learned = learned_keys(app);
    let pooled = pooled_keys(app);
    let seeded = seeded_keys(app);
    let source = geometry_source.to_string();
    publish(app, |slice| {
        slice.status = MercStatus::Idle;
        slice.geometry_source = source;
        slice.learned_families = learned;
        slice.pooled_families = pooled;
        slice.seeded_families = seeded;
        slice.last_error = geometry_err;
    });

    // Anything hover-learned that the pool has not seen — including a whole
    // store learned before the pool existed. Off-tick like every other upload.
    sync::enqueue_backfill(app);
    // And every forget whose tombstone never landed. Without this a POST that
    // failed three times is never retried: the key stays suppressed on this
    // device for good, and each start re-downloads the corpus and re-runs the
    // tombstone replace it can never finish.
    sync::retry_pending_tombstones(app);

    let vocab = MercVocab::load()?;
    crate::ocr::engine_ready()?;

    Ok(Session {
        geometry,
        vocab,
        state: LoopState::default(),
        errors: OnceLog::default(),
        screen_refusals: OnceLog::default(),
        current: None,
        sigs: SigCache::new(),
        pending_confirm: None,
        confirmed: HashMap::new(),
        hover_budget: HoverBudget::default(),
        template_generation: template_generation(app),
        saves: icons_dir.clone().map(|dir| {
            let app = app.clone();
            // The worker's own once-sink. A store directory that cannot be
            // written fails the same way on every confirm, and this thread has
            // no `Session` to reach the loop's [`OnceLog`] through.
            let mut errors = OnceLog::default();
            SaveQueue::spawn(move || {
                let state = app.state::<AppState>();
                // SNAPSHOT under the store's mutex, WRITE outside it: the
                // detect tick's `match_family` runs against the same mutex, and
                // holding it across the PNG writes would move the stall rather
                // than remove it. Both halves sit inside the DIRECTORY lock,
                // which is what stops `sync`'s writes interleaving with this
                // one — and what stops the snapshot going stale between being
                // taken and being written. See [`icons::writing_icons_dir`].
                let result =
                    super::super::icons::writing_icons_dir(&state.merc.icons_write, || {
                        let snapshot = {
                            let store =
                                state.merc.templates.lock().unwrap_or_else(|e| e.into_inner());
                            store.clone()
                        };
                        snapshot.save(&dir)
                    });
                if let Err(e) = result {
                    let msg = format!("Merc: template store save failed — {e}");
                    if let Some(line) = errors.admit(&msg) {
                        crate::app_log(&app, line);
                    }
                    publish(&app, |slice| slice.last_error = Some(msg));
                }
            })
        }),
        miss_logged: false,
        panel: None,
        header_guard: None,
        fallback: FallbackBudget::default(),
        column_trusted_said: None,
        crop_detect_logged: false,
        occlusion: OcclusionRun::default(),
        header_logged: None,
        row_mismatch_logged: None,
        retained: None,
        trade: None,
        fitted: None,
        scale_source: ScaleSource::Ocr,
        // Seeded, not zeroed: a press that happened while no loop was running
        // has nothing to undo — this session holds no registration yet.
        refit_seen: refit_counter(app),
        refit_located: refit_counter(app),
        geometry_changed: false,
        revision: 0,
    })
}

/// Open the template store in the order the session depends on: purge,
/// registration reset, pull + seed fetch spawned, load and install, the
/// registration warnings, the re-key, the seeds, then the corpus merge.
///
/// Each step's position is pinned by a test in this file's `tests` module.
pub(super) fn open_template_store(
    h: &StoreHandles,
    needs_reset: bool,
    ports: &mut impl SessionOpenPorts,
) -> StoreOpened {
    let dir = h.dir;
    // THE PURGE COMES FIRST (POE-207). A store written by format 1 holds
    // luma signatures this build cannot correlate against anything, and
    // both the pull below and the load after it would otherwise merge into
    // it. Dropping it here — before the session begins, so the pull's ETag
    // is asked for on an empty store — is what makes the format bump a
    // clean restart rather than a poisoned mix. Pinned by
    // `a_format_one_store_is_purged_before_the_corpus_is_merged` and
    // `a_format_one_store_is_purged_before_it_is_loaded`.
    let purged = super::super::icons::writing_icons_dir(h.dir_lock, || {
        super::super::icons::purge_stale_store(dir)
    });
    if let Some(purged) = purged {
        ports.log(purge_log_line(&purged));
    }

    // AND THEN THE ONE-SHOT REGISTRATION RESET (POE-215 D2), its own
    // sibling scope: `writing_icons_dir` is a non-reentrant
    // `std::sync::Mutex<()>`, so the purge above released it and this takes
    // it for itself. Before the pull, for the same reason the purge is:
    // whatever the store holds when the corpus lands has to be the store
    // this build vouches for, and dropping art AFTER a merge would drop
    // what the merge just installed. Pinned by
    // `a_pre_fit_sample_is_reset_before_the_store_is_loaded`.
    //
    // Not a purge: the signatures here are format 2 and they correlate
    // fine. They were just cut 6-12 px off the art, at the OCR guess
    // POE-214 replaced. See `icons::REGISTRATION`.
    let mut suppress_pull = false;
    if needs_reset {
        let report = super::super::icons::writing_icons_dir(h.dir_lock, || {
            super::super::icons::reset_unregistered(dir)
        });
        ports.log(super::super::icons::reset_log_line(&report));
        // OUTSIDE the directory guard, because this takes `SYNC_FILE_LOCK`
        // and nothing may hold two of the module's locks in a new order.
        // AFTER the move, so a crash in between re-arms a reset that finds
        // nothing left to move rather than skipping one that never ran —
        // and only on a CLEAN pass, so a directory the reset could not
        // finish writing is tried again next start rather than being
        // marked done. A pass that rewrote the index and failed only on a
        // file converges anyway: the retry finds nothing left to drop.
        if report.problems.is_empty() {
            if let Err(e) = sync::record_registration_reset(dir) {
                ports.log(format!(
                    "Merc: the registration reset ran but could not be recorded, so it \
                     runs again next start — {e}"
                ));
            }
        } else {
            ports.log(
                "Merc: the registration reset did not finish — the pool ETag is UNCHANGED \
                 and the reset stays armed for the next start"
                    .to_string(),
            );
        }
        suppress_pull = report.dropped_anything();
    }

    // Ask the shared pool BEFORE reading the disk, so the round-trip
    // overlaps the load instead of following it (POE-201). One pull per
    // module start, single-flight inside `spawn_pull`.
    ports.begin_session();
    // AFTER `begin_session`, which clears the flag for this start (POE-215
    // D5). A non-empty reset means the pool may still be serving the very
    // art that was just dropped, and this start has no way to tell — so it
    // does not ask. The ETag went with the reset, so the next start pulls a
    // full corpus and the runbook's purge is what makes it a clean one.
    // Pinned by `a_reset_that_dropped_art_keeps_this_starts_corpus_out`.
    if suppress_pull {
        ports.suppress_pull();
    }
    ports.spawn_pull();
    // Beside the pull, for the same reason: the network overlaps the disk
    // load. A warm cache issues no request at all and `wait_for_install`
    // below is released as soon as the pass has looked at the directory.
    ports.spawn_seed_fetch();

    // READ AND INSTALL UNDER ONE DIRECTORY GUARD (POE-210 fix-in-context).
    // `apply_corpus` writes `index.json` and the PNGs beside it while
    // holding `writing_icons_dir`, and a re-pull can now land at any moment
    // rather than only in this start's own window — so a module toggled on
    // mid-save would otherwise read a half-written index. The guard covers
    // the INSTALL as well as the load, because a corpus landing between the
    // two would merge into the installed store, save, persist its ETag, and
    // then be erased in memory by the assignment: a 304 promising art
    // nobody will rewrite. Lock order is unchanged — `writing_icons_dir`
    // then `merc.templates`, the order every other writer takes.
    //
    // INSTALL FIRST, MERGE SECOND. The whole-store write happens here, while
    // the seam still holds its pull claim, and every merge — this start's or
    // a later task's — then runs against the INSTALLED store under its
    // mutex. Merging into the local copy first and installing afterwards is
    // what made this seam a second writer, the same erasure described above.
    // The re-key persist below obeys the same discipline: it is a WRITER of
    // the installed store, so it runs behind this assignment and saves what
    // the mutex holds, never the local copy. Pinned by
    // `a_corpus_merged_during_the_open_survives_in_the_installed_store`.
    //
    // COST: the guard is now held across the whole store load — up to 1377
    // PNG decodes at the corpus ceiling. A re-pull or a backfill landing
    // mid-load blocks its tokio worker on a `std::sync::Mutex` for that
    // long. Degradation, never deadlock: the seam takes no other lock while
    // holding this one, and the wait it does perform for a corpus
    // (`sync::wait_for_pull`) is outside the guard entirely.
    //
    // The purge above released its guard and the re-key below takes its own
    // — three sibling scopes, no nesting.
    let (loaded, problems, rekeyed) = super::super::icons::writing_icons_dir(h.dir_lock, || {
        let LoadedStore { store, problems, rekeyed } = TemplateStore::load(dir);
        let loaded = store.len();
        *h.store.lock().unwrap_or_else(|e| e.into_inner()) = store;
        (loaded, problems, rekeyed)
    });
    ports.log(format!("Merc: {loaded} learned templates loaded"));

    // WHAT CAME OFF DISK, IN REGISTRATION TERMS (POE-215 D4). Here rather
    // than in `matcher_geometry_warnings`, which runs before the store is
    // read and takes only geometry — and before the pull's merge below, so
    // the pooled-unknown count is what this device has been carrying rather
    // than what this start's corpus just added. Pinned by
    // `the_registration_warning_counts_only_what_came_off_disk`.
    let warnings = {
        let store = h.store.lock().unwrap_or_else(|e| e.into_inner());
        super::super::icons::registration_warnings(&store)
    };
    for warning in warnings {
        ports.log(warning);
    }

    // THE FAMILY RE-KEY IS PERSISTED HERE (POE-211). `load` folded the
    // aliased families in memory and reported what it touched; this is the
    // write that makes the fold stick, and the OLD KEY LEAVING `index.json`
    // IS THE ONE-SHOT MARKER — no marker file, because a file could
    // disagree with the index it describes.
    //
    // THE SAVE RUNS ONLY ON A CLEAN INDEX READ. It rewrites `index.json`
    // from the store in memory, and an entry `load` could not decode is not
    // in it — a transient AV lock, a OneDrive placeholder, a truncated PNG.
    // The gate keeps THIS writer off the index; it does NOT protect that
    // entry. Any later writer of the installed store — `apply_corpus`,
    // `mark_uploaded`, the off-tick `SaveQueue`, a forget or a reset —
    // rewrites the index out of the same memory and drops it just as surely,
    // and that erasure PREDATES POE-211: `load` already skipped undecodable
    // entries, and the first `apply_corpus` of a session already rewrote the
    // index without them. What the gate buys is that the fold is not what
    // spends the one-shot marker on a start whose read was incomplete, so a
    // transient lock costs a deferral rather than the migration. Art that is
    // corrupt for good defers the fold on this device until
    // `merc_reset_templates`; it is reported as a problem on every start.
    //
    // The purge above released the directory lock (it is a non-reentrant
    // `std::sync::Mutex<()>`), so this takes it for itself, then the store's
    // mutex inside it — `writing_icons_dir` → `merc.templates`, the order
    // `apply_corpus` takes. Under one acquisition of both, so no forget or
    // reset landing between the load and the save can rewrite an index
    // naming PNGs this is about to unlink.
    if !rekeyed.is_empty() {
        // OWED THE MOMENT THE FOLDED STORE IS INSTALLED, not the moment it
        // reaches disk — and the install above ran whether or not the save
        // below does. Any later writer of that store — the corpus merge,
        // `mark_uploaded`, the off-tick `SaveQueue`, a forget or a reset —
        // persists the fold and spends the one-shot marker, on a DEFERRED
        // start exactly as on a failed save, so the record belongs outside
        // both gates. `record_pending_in` dedupes against what the file
        // already holds, so a deferred start that records the same key
        // again changes nothing.
        //
        // Before the directory guard below is taken, so the two locks are
        // never held at the same time (`record_tombstones` takes
        // `SYNC_FILE_LOCK`, and so does `apply_corpus`'s sync-file work,
        // outside its own directory guard). `retry_pending_tombstones`, a
        // few lines below, is what SENDS these; POSTing from here as well
        // would send each key twice. Pinned by
        // `a_deferred_re_key_still_records_its_tombstone_and_keeps_the_old_index`
        // and `a_clean_read_persists_the_re_key_after_recording_its_tombstone`.
        ports.record_tombstones(&tombstones_owed(&rekeyed));
        if problems.is_empty() {
            let persisted = super::super::icons::writing_icons_dir(h.dir_lock, || {
                let store = h.store.lock().unwrap_or_else(|e| e.into_inner());
                super::super::icons::persist_rekey(dir, &store, &rekeyed)
            });
            match persisted {
                Ok(not_removed) => ports.log(rekey_log_line(&rekeyed, not_removed)),
                // NOT "the next start folds again": a later successful save
                // by any writer completes the fold minus the unlink, leaving
                // the old PNGs orphaned (harmless — no index names them).
                // What is durable either way is the tombstone record above.
                Err(e) => ports.log(format!("Merc: the re-keyed store could not be written — {e}")),
            }
        } else {
            ports.log(format!(
                "Merc: the family re-key is deferred — {} unreadable index entr(ies)",
                problems.len()
            ));
        }
    }

    // AFTER the whole-store assignment, BEFORE the pull's merge (POE-208).
    // A seed is memory-only, so it has to be put into the store the session
    // will use rather than into the copy the line above replaced — and it
    // has to be in place before the merge, because a served sample that
    // collides with a seed evicts it, which is the resolution the eviction
    // rule describes and not a race. Pinned by
    // `a_served_sample_colliding_with_a_seed_evicts_it`.
    ports.install_seeds(h);

    // Bounded, and it releases the seam claim whether or not a corpus
    // arrives — after this line a later corpus applies itself.
    if let Some((corpus, etag)) = ports.take_corpus() {
        ports.apply_corpus(h, &corpus, etag);
    }
    let pooled_samples = {
        let store = h.store.lock().unwrap_or_else(|e| e.into_inner());
        store.pooled_samples()
    };
    ports.set_pooled_samples(pooled_samples);
    if pooled_samples > 0 {
        ports.log(format!("Merc: {pooled_samples} of them came from the shared pool"));
    }
    StoreOpened { problems }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mercenary::icons::{PooledSample, REGISTRATION};
    use crate::mercenary::seed::{self as seed_mod, SeedBlocklist};

    // -- the store purge's log line (POE-207) --------------------------------

    /// The upgrade case: the line names the version the index actually
    /// declared, so the log says which store was dropped and which one this
    /// build reads.
    #[test]
    fn a_purged_format_one_store_is_logged_with_its_version_and_count() {
        let line = purge_log_line(&PurgedStore {
            version: Some(1),
            dropped: 61,
        });

        assert_eq!(line, "Merc: dropped 61 format-1 template(s) (format 2)");
    }

    /// A downgrade meets an index from a LATER version, and it must not be
    /// reported as the format-1 upgrade — the causes are opposite (an old
    /// store this build replaces, versus a newer store this build destroys)
    /// and only one of them is expected.
    #[test]
    fn a_purged_store_from_a_later_version_is_logged_with_that_version() {
        let line = purge_log_line(&PurgedStore {
            version: Some(3),
            dropped: 7,
        });

        assert!(line.contains("format-3"), "{line}");
    }

    /// An index that parsed as nothing has no version and no count. Saying
    /// "dropped 0 format-1 templates" would be two wrong facts about a
    /// half-written or corrupt file.
    #[test]
    fn a_purged_unreadable_index_is_logged_as_unreadable() {
        let line = purge_log_line(&PurgedStore {
            version: None,
            dropped: 0,
        });

        assert_eq!(line, "Merc: dropped an unreadable template index (format 2)");
    }

    // -- the family re-key's decisions (POE-211) -----------------------------

    fn rekeyed(tier: u8, uploaded_local: bool) -> Rekeyed {
        Rekeyed {
            old_family: "Increased Area of Effect".to_string(),
            new_family: "Area of Effect".to_string(),
            tier,
            old_files: Vec::new(),
            uploaded_local,
        }
    }

    /// Only art this device actually put on the server under the old key owes
    /// a tombstone. A sample the pool never saw needs nothing retired, and
    /// POSTing for it would retire every OTHER device's art under that key —
    /// a tombstone carries no device filter.
    #[test]
    fn only_a_sample_the_pool_accepted_owes_a_tombstone() {
        let owed = tombstones_owed(&[rekeyed(2, true), rekeyed(3, false)]);

        assert_eq!(owed, [("Increased Area of Effect".to_string(), 2)]);
    }

    /// One key can hold three samples and a tombstone is keyed
    /// `(version, family, tier)`, so three re-keyed samples of one key owe ONE
    /// POST — not the same one three times.
    #[test]
    fn several_re_keyed_samples_of_one_key_owe_a_single_tombstone() {
        let owed = tombstones_owed(&[rekeyed(2, true), rekeyed(2, true), rekeyed(2, true)]);

        assert_eq!(owed, [("Increased Area of Effect".to_string(), 2)]);
    }

    /// The log line names both halves of the fold and the count, because those
    /// are what the smoke check reads: that the migration ran, and that the
    /// pool was told about the key it retired.
    #[test]
    fn the_re_key_line_names_the_fold_and_the_tombstone_count() {
        let line = rekey_log_line(&[rekeyed(2, true)], 0);

        assert_eq!(
            line,
            "Merc: re-keyed 1 sample(s) from Increased Area of Effect to Area of Effect (1 tombstone(s) offered)",
        );
    }

    /// A fold that retires nothing says zero rather than staying silent: "the
    /// store moved but the pool was not told" is the state to be able to read
    /// off the log, not one to infer from a missing line.
    #[test]
    fn a_re_key_that_owes_no_tombstone_reports_zero() {
        let line = rekey_log_line(&[rekeyed(2, false)], 0);

        assert!(line.ends_with("(0 tombstone(s) offered)"), "{line}");
    }

    /// One fold, however many samples went through it, is named ONCE. Two tiers
    /// of the same family are two samples and two tombstones — the counts still
    /// have to say so — but repeating "Increased Area of Effect to Area of
    /// Effect" once per sample turns a 61-sample store's line into a wall.
    #[test]
    fn the_re_key_line_names_one_fold_once_however_many_samples_took_it() {
        let line = rekey_log_line(&[rekeyed(2, true), rekeyed(3, true)], 0);

        assert_eq!(
            line,
            "Merc: re-keyed 2 sample(s) from Increased Area of Effect to Area of Effect (2 tombstone(s) offered)",
        );
    }

    /// An old file that would not go away is REPORTED. The migration is still
    /// correct — no index names the orphan — but a directory that keeps growing
    /// across starts is a bug nobody could report off a silent log.
    #[test]
    fn the_re_key_line_reports_old_files_that_could_not_be_removed() {
        let line = rekey_log_line(&[rekeyed(2, true)], 2);

        assert!(line.ends_with("(1 tombstone(s) offered), 2 file(s) not removed"), "{line}");
    }

    // -- the store open's order ----------------------------------------------

    /// The ports a test opens the store through. Production-faithful where the
    /// order is decided: the merge is the real `merge_pulled` + save under the
    /// directory guard (as `sync::apply_corpus`), the seeds go in through the
    /// real `seed::install_all`, and the pull follows `sync`'s suppression —
    /// `begin_session` clears the flag (`sync::begin`), a suppressed start
    /// issues no request (`claim_pull`) and a corpus landing on one is
    /// dropped (`record_pull`).
    struct FakePorts<'a> {
        dir_lock: &'a Mutex<()>,
        /// What the pool would serve to a request this start issued.
        served: Option<PooledCorpus>,
        seeds: Vec<(String, u8, CellSig)>,
        pull_suppressed: bool,
        pull_requested: bool,
        lines: Vec<String>,
        tombstones: Vec<(String, u8)>,
        /// Whether the directory lock was free each time tombstones were
        /// recorded.
        tombstones_saw_dir_lock_free: Vec<bool>,
        lost_seeds: Vec<String>,
    }

    impl<'a> FakePorts<'a> {
        fn new(dir_lock: &'a Mutex<()>) -> Self {
            Self {
                dir_lock,
                served: None,
                seeds: Vec::new(),
                pull_suppressed: false,
                pull_requested: false,
                lines: Vec::new(),
                tombstones: Vec::new(),
                tombstones_saw_dir_lock_free: Vec::new(),
                lost_seeds: Vec::new(),
            }
        }
    }

    impl SessionOpenPorts for FakePorts<'_> {
        fn log(&mut self, line: String) {
            self.lines.push(line);
        }
        fn begin_session(&mut self) {
            self.pull_suppressed = false;
        }
        fn suppress_pull(&mut self) {
            self.pull_suppressed = true;
        }
        fn spawn_pull(&mut self) {
            self.pull_requested = !self.pull_suppressed;
        }
        fn spawn_seed_fetch(&mut self) {}
        fn record_tombstones(&mut self, keys: &[(String, u8)]) {
            self.tombstones_saw_dir_lock_free.push(self.dir_lock.try_lock().is_ok());
            self.tombstones.extend_from_slice(keys);
        }
        fn install_seeds(&mut self, h: &StoreHandles) {
            let mut store = h.store.lock().unwrap();
            seed_mod::install_all(&mut store, &self.seeds, &SeedBlocklist::default(), &thresholds());
        }
        fn take_corpus(&mut self) -> Option<(PooledCorpus, Option<String>)> {
            if !self.pull_requested || self.pull_suppressed {
                return None;
            }
            self.served.take().map(|c| (c, None))
        }
        fn apply_corpus(&mut self, h: &StoreHandles, corpus: &PooledCorpus, _etag: Option<String>) {
            let suppressed = sync::SyncFile::load(h.dir).suppressed();
            let lost = super::super::super::icons::writing_icons_dir(h.dir_lock, || {
                let mut store = h.store.lock().unwrap();
                let seeded = store.seeded_families();
                let outcome = store.merge_pulled(corpus, &suppressed, &thresholds());
                let lost = store.seeds_lost_since(&seeded);
                if outcome.changed() {
                    store.save(h.dir).expect("the merged store saves");
                }
                lost
            });
            self.lost_seeds.extend(lost);
        }
        fn set_pooled_samples(&mut self, _count: usize) {}
    }

    fn thresholds() -> crate::mercenary::Thresholds {
        MercGeometry::default().thresholds
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "poe-merc-session-open-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// A signature of its own art: a seeded pseudo-random RGB buffer, so two
    /// different seeds correlate at about zero and one seed with itself at 1.
    fn sig(seed: u32) -> CellSig {
        let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(12_345);
        let bytes = (0..super::super::super::icons::SIG_BYTES)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (x >> 16) as u8
            })
            .collect();
        CellSig::from_rgb(bytes).expect("a varied buffer normalizes")
    }

    fn corpus(samples: &[(&str, u8, u32)]) -> PooledCorpus {
        PooledCorpus {
            format_version: sync::FORMAT_VERSION,
            samples: samples
                .iter()
                .map(|&(family, tier, art)| PooledSample { family: family.to_string(), tier, sig: sig(art) })
                .collect(),
            tombstones: Vec::new(),
        }
    }

    /// Write a store holding one local sample per `(family, tier, art,
    /// registration)`.
    fn write_local_store(dir: &Path, samples: &[(&str, u8, u32, u16)]) {
        let mut store = TemplateStore::new();
        for &(family, tier, art, registration) in samples {
            store.learn(family, tier, sig(art), None, registration, &thresholds());
        }
        store.save(dir).expect("save the fixture store");
    }

    /// The families `index.json` names, in index order.
    fn indexed_families(dir: &Path) -> Vec<String> {
        let raw = std::fs::read_to_string(dir.join("index.json")).expect("read the index");
        let index: serde_json::Value = serde_json::from_str(&raw).expect("the index parses");
        index["entries"]
            .as_array()
            .expect("an entries array")
            .iter()
            .map(|e| e["family"].as_str().expect("a family").to_string())
            .collect()
    }

    fn open(dir: &Path, store: &Mutex<TemplateStore>, needs_reset: bool, ports: &mut FakePorts) -> StoreOpened {
        let handles = StoreHandles { dir, dir_lock: ports.dir_lock, store };
        open_template_store(&handles, needs_reset, ports)
    }

    /// A format-1 index with one PNG beside it, under a name the served
    /// sample cannot overwrite.
    fn write_format_one_store(dir: &Path) {
        std::fs::write(
            dir.join("index.json"),
            r#"[{"family":"Chain","tier":2,"file":"stale-format-one.png"}]"#,
        )
        .expect("write the format-1 index");
        image::GrayImage::new(24, 24)
            .save(dir.join("stale-format-one.png"))
            .expect("write the format-1 png");
    }

    /// THE PURGE COMES FIRST, before the merge: a format-1 store is cleared
    /// before the served corpus is written beside it, so the directory ends up
    /// holding the served art and none of the format-1 art.
    #[test]
    fn a_format_one_store_is_purged_before_the_corpus_is_merged() {
        let dir = temp_dir("purge-before-merge");
        write_format_one_store(&dir);
        let (dir_lock, store) = (Mutex::new(()), Mutex::new(TemplateStore::new()));
        let mut ports = FakePorts::new(&dir_lock);
        ports.served = Some(corpus(&[("Pierce", 3, 1)]));

        open(&dir, &store, false, &mut ports);

        let reloaded = TemplateStore::load(&dir).store;
        assert!(reloaded.get("Pierce", 3).is_some(), "the served sample is not on disk");
        assert!(
            !dir.join("stale-format-one.png").exists(),
            "the format-1 png survived the open",
        );
    }

    /// THE PURGE COMES FIRST, before the load: the load meets the purged
    /// (empty, format-2) index, so it reports no foreign-format problem.
    #[test]
    fn a_format_one_store_is_purged_before_it_is_loaded() {
        let dir = temp_dir("purge-before-load");
        write_format_one_store(&dir);
        let (dir_lock, store) = (Mutex::new(()), Mutex::new(TemplateStore::new()));
        let mut ports = FakePorts::new(&dir_lock);

        let opened = open(&dir, &store, false, &mut ports);

        assert!(opened.problems.is_empty(), "{:?}", opened.problems);
    }

    /// The registration reset runs before the load, so a pre-fit sample on
    /// disk never reaches the installed store.
    #[test]
    fn a_pre_fit_sample_is_reset_before_the_store_is_loaded() {
        let dir = temp_dir("reset-before-load");
        write_local_store(&dir, &[("Chain", 2, 1, 0)]);
        let (dir_lock, store) = (Mutex::new(()), Mutex::new(TemplateStore::new()));
        let mut ports = FakePorts::new(&dir_lock);

        open(&dir, &store, true, &mut ports);

        assert!(
            store.lock().unwrap().get("Chain", 2).is_none(),
            "the pre-fit sample was installed",
        );
    }

    /// A reset that dropped art suppresses this start's pull AFTER
    /// `begin_session` has cleared the flag, so the corpus the pool serves is
    /// not merged.
    #[test]
    fn a_reset_that_dropped_art_keeps_this_starts_corpus_out() {
        let dir = temp_dir("reset-suppresses");
        write_local_store(&dir, &[("Chain", 2, 1, 0)]);
        let (dir_lock, store) = (Mutex::new(()), Mutex::new(TemplateStore::new()));
        let mut ports = FakePorts::new(&dir_lock);
        ports.served = Some(corpus(&[("Pierce", 3, 2)]));

        open(&dir, &store, true, &mut ports);

        assert!(
            store.lock().unwrap().get("Pierce", 3).is_none(),
            "a suppressed start merged the served corpus",
        );
    }

    /// The negative of the above: a reset that dropped nothing does not
    /// suppress, and the served corpus is merged.
    #[test]
    fn a_reset_that_dropped_nothing_lets_the_corpus_in() {
        let dir = temp_dir("reset-keeps-pull");
        write_local_store(&dir, &[("Chain", 2, 1, REGISTRATION)]);
        let (dir_lock, store) = (Mutex::new(()), Mutex::new(TemplateStore::new()));
        let mut ports = FakePorts::new(&dir_lock);
        ports.served = Some(corpus(&[("Pierce", 3, 2)]));

        open(&dir, &store, true, &mut ports);

        assert!(
            store.lock().unwrap().get("Pierce", 3).is_some(),
            "a reset that dropped nothing kept the corpus out",
        );
    }

    /// INSTALL FIRST, MERGE SECOND: the corpus merged during the open is still
    /// in the installed store afterwards, alongside what came off disk.
    #[test]
    fn a_corpus_merged_during_the_open_survives_in_the_installed_store() {
        let dir = temp_dir("install-before-merge");
        write_local_store(&dir, &[("Chain", 2, 1, REGISTRATION)]);
        let (dir_lock, store) = (Mutex::new(()), Mutex::new(TemplateStore::new()));
        let mut ports = FakePorts::new(&dir_lock);
        ports.served = Some(corpus(&[("Pierce", 3, 2)]));

        open(&dir, &store, false, &mut ports);

        let store = store.lock().unwrap();
        assert!(store.get("Pierce", 3).is_some(), "the merged corpus was erased");
        assert!(store.get("Chain", 2).is_some(), "the disk sample was not installed");
    }

    /// The registration warning counts what came off disk, not what this
    /// start's corpus added: one pooled sample of unknown registration on
    /// disk, two more served.
    #[test]
    fn the_registration_warning_counts_only_what_came_off_disk() {
        let dir = temp_dir("warnings-disk-only");
        let mut on_disk = TemplateStore::new();
        on_disk.merge_pulled(&corpus(&[("Chain", 2, 1)]), &[], &thresholds());
        on_disk.save(&dir).expect("save the fixture store");
        let (dir_lock, store) = (Mutex::new(()), Mutex::new(TemplateStore::new()));
        let mut ports = FakePorts::new(&dir_lock);
        ports.served = Some(corpus(&[("Pierce", 3, 2), ("Fork", 1, 3)]));

        open(&dir, &store, false, &mut ports);

        let warnings: Vec<&String> =
            ports.lines.iter().filter(|l| l.contains("pooled samples of unknown registration")).collect();
        assert_eq!(
            warnings,
            ["Merc: 1 pooled samples of unknown registration — the pool accepts uploads from \
              pre-fit installs until they upgrade"],
        );
    }

    /// The family the alias table folds, as an older index spells it.
    const OLD_AOE: &str = "Increased Area of Effect";

    /// An uploaded local sample under the old family: the re-key owes the pool
    /// one tombstone for it.
    fn write_uploaded_aliased_store(dir: &Path) {
        let mut store = TemplateStore::new();
        let art = sig(1);
        store.learn(OLD_AOE, 2, art.clone(), None, REGISTRATION, &thresholds());
        store.mark_uploaded(OLD_AOE, 2, art.bytes());
        store.learn("Chain", 2, sig(2), None, REGISTRATION, &thresholds());
        store.save(dir).expect("save the fixture store");
    }

    /// A re-key on a start whose index read was incomplete is deferred — the
    /// index still names the old family — but its tombstone is recorded
    /// anyway, with the directory lock free.
    #[test]
    fn a_deferred_re_key_still_records_its_tombstone_and_keeps_the_old_index() {
        let dir = temp_dir("rekey-deferred");
        write_uploaded_aliased_store(&dir);
        std::fs::write(dir.join("chain--t2.png"), b"not a png").expect("corrupt one entry");
        let (dir_lock, store) = (Mutex::new(()), Mutex::new(TemplateStore::new()));
        let mut ports = FakePorts::new(&dir_lock);

        let opened = open(&dir, &store, false, &mut ports);

        assert_eq!(opened.problems.len(), 1, "{:?}", opened.problems);
        assert_eq!(ports.tombstones, [(OLD_AOE.to_string(), 2)]);
        assert_eq!(ports.tombstones_saw_dir_lock_free, [true]);
        assert!(
            indexed_families(&dir).iter().any(|f| f == OLD_AOE),
            "a deferred re-key rewrote the index: {:?}",
            indexed_families(&dir),
        );
    }

    /// On a clean read the re-key is persisted — the old family leaves the
    /// index — and its tombstone was recorded with the directory lock free.
    #[test]
    fn a_clean_read_persists_the_re_key_after_recording_its_tombstone() {
        let dir = temp_dir("rekey-clean");
        write_uploaded_aliased_store(&dir);
        let (dir_lock, store) = (Mutex::new(()), Mutex::new(TemplateStore::new()));
        let mut ports = FakePorts::new(&dir_lock);

        open(&dir, &store, false, &mut ports);

        assert_eq!(ports.tombstones, [(OLD_AOE.to_string(), 2)]);
        assert_eq!(ports.tombstones_saw_dir_lock_free, [true]);
        assert_eq!(indexed_families(&dir), ["Area of Effect", "Chain"]);
    }

    /// The seeds go in before the merge, so a served sample of another family
    /// on the same art evicts the seed (and the seed is reported lost) rather
    /// than the seed yielding to it afterwards.
    #[test]
    fn a_served_sample_colliding_with_a_seed_evicts_it() {
        let dir = temp_dir("seed-before-merge");
        let (dir_lock, store) = (Mutex::new(()), Mutex::new(TemplateStore::new()));
        let mut ports = FakePorts::new(&dir_lock);
        ports.seeds = vec![("Brittle Chance".to_string(), 3, sig(5))];
        ports.served = Some(corpus(&[("Physical as Extra Chaos", 3, 5)]));

        open(&dir, &store, false, &mut ports);

        assert_eq!(ports.lost_seeds, ["Brittle Chance"]);
        assert!(store.lock().unwrap().get("Physical as Extra Chaos", 3).is_some());
    }
}
