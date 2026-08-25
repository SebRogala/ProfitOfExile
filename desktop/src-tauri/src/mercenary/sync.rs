//! Shared icon-template pool — the desktop half (POE-201).
//!
//! One device's hover teaches every device. This module is the only thing in
//! the merc module that talks to the server: it pulls the corpus when the
//! module starts, offers locally-learned signatures back off the capture tick,
//! and turns a local forget into a server-side tombstone.
//!
//! # What travels
//!
//! Exactly `(family, tier, 576 grayscale bytes)` plus the format version. The
//! payload is built from [`super::icons::CellSig::gray`] in memory and the wire
//! types below have no field for anything else — the colour crop `save` writes
//! next to each template is GGG's art and never leaves the device, and nothing
//! here walks the store directory. What comes back names no device either; the
//! server attributes uploads internally and serves an anonymous corpus.
//!
//! # Fail-soft, everywhere
//!
//! A desktop build can be ahead of the server, the server can be down, and the
//! module has to work regardless — it did before a pool existed. Every failure
//! here leaves the local store standing and costs ONE log line per session, not
//! one per attempt: a mercenary module that spams the log when the user is
//! offline is worse than one that says nothing.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::watch;

use super::icons::{CellSig, PooledCorpus, PooledSample};
use crate::AppState;

/// The signature format this build speaks, and the ONLY one it merges.
///
/// It must equal the server's `mercenary.SupportedFormatVersion`
/// (`internal/mercenary/pool.go`) — the upload endpoint refuses any other
/// value outright, and the merge below refuses a corpus that declares one.
///
/// Version 1 is: a 24×24 grayscale crop of a support cell ([`super::icons::SIG_DIM`]),
/// the luma from `geometry::luma`, the badge corner masked at 0.45 × 0.35, and
/// zero-mean unit-stddev normalisation over the unmasked positions. Changing
/// ANY of those numbers is version 2, not a tweak to version 1: signatures from
/// the two do not correlate, so a shared pool that mixed them would poison every
/// device's matcher at once.
///
/// **What a bump means:** the pool refills. Version 2 starts empty on the
/// server, every device re-learns from hovers, and the version-1 rows stay
/// readable by version-1 clients until they are gone. Nothing has to be
/// migrated and nothing has to be deleted — that is the whole reason the
/// version is part of the key rather than a note in a changelog.
pub const FORMAT_VERSION: u16 = 1;

/// Templates per upload request.
///
/// Sized against the BODY cap, not against the server's stated ceiling of 64.
/// The server's own comment estimates "roughly 40 templates" from the signature
/// alone; the JSON around it is what the estimate misses. One template is 768
/// base64 characters plus the key and the punctuation — 841 bytes for the
/// longest family the vocabulary carries (`Power Charge on Critical Strike`,
/// 31 characters). Forty of those is 33,675 bytes, over the 32,768 cap: a full
/// first publish would 413 on every batch and place nothing.
///
/// 32 leaves ~5.8 KB of headroom, which is what absorbs a longer family name
/// arriving with a future league without a desktop release. The cost is a
/// request per 32 samples — a whole 792-sample store is 25 requests against a
/// budget of 60 per ten minutes.
///
/// [`SERVER_BODY_LIMIT`] and the test below are what keep this honest.
pub const MAX_TEMPLATES_PER_BATCH: usize = 32;

/// The upload body cap the server enforces (`mercTemplateBodyLimit`,
/// `internal/server/handlers/mercenary.go`). Mirrored here so the batch size is
/// checked against the server's real number rather than against a remembered
/// one — a test-only constant because the check IS the test: production never
/// measures its own body, it only sends batches the check has already sized.
#[cfg(test)]
const SERVER_BODY_LIMIT: usize = 32 * 1024;

/// Attempts per batch and per tombstone before it is left for the next module
/// start.
///
/// Bounded rather than persistent because the durable retry lives on disk: an
/// unplaced sample keeps `uploaded: false` in `index.json`, so the next module
/// start offers it again. Retrying here forever would only make the app hold a
/// queue nobody is watching.
const MAX_ATTEMPTS: u32 = 3;

/// Backoff before a retry when the server did not name one.
const RETRY_BASE: Duration = Duration::from_secs(2);

/// Ceiling on an honoured `Retry-After`.
///
/// The pool's rate limit is 60 writes per 10 minutes per device, so a genuine
/// 429 can name a wait of minutes. Sleeping it out inside a task that holds
/// nothing is fine, but a server naming an hour must not park a task for an
/// hour — past this the batch is dropped to the next module start instead.
const RETRY_CEILING: Duration = Duration::from_secs(120);

/// Fallback wait when `Retry-After` is missing or unparseable.
const RETRY_FALLBACK: Duration = Duration::from_secs(30);

/// How long the capture loop's start waits for the pull before giving up on
/// merging it into the store it is about to install.
///
/// The point is the seam, not the speed: a merge that lands inside this window
/// goes into the store BEFORE the loop's single whole-store write, so there is
/// one writer and no generation bump. A pull that misses it is applied later
/// under the same mutex, which is correct but noisier. 1.2 s is long enough for
/// a warm server and short enough that an offline user's module start is not
/// visibly delayed.
const STARTUP_WAIT: Duration = Duration::from_millis(1200);

/// Poll step while waiting out [`STARTUP_WAIT`].
const STARTUP_POLL: Duration = Duration::from_millis(50);

/// Request timeout for a pool call. The shared HTTP client sets none, and an
/// unbounded pull would hold the startup claim until the OS gave up.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Where the pull's ETag and the unacknowledged tombstones live, inside the
/// template directory.
const SYNC_FILE: &str = "pool-sync.json";

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// One template, in both directions — the mirror of the server's
/// `mercTemplateItem`.
///
/// There is deliberately no field for the colour crop, for a device id, or for
/// anything else the store holds: what cannot be named cannot be sent by
/// accident, and a test asserts this type serialises to exactly three keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireTemplate {
    pub family: String,
    pub tier: u8,
    pub signature_b64: String,
}

/// One key, for the tombstone list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireKey {
    pub family: String,
    pub tier: u8,
}

/// An upload request body.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UploadBody {
    pub format_version: u16,
    pub templates: Vec<WireTemplate>,
}

/// A tombstone request body.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct TombstoneBody {
    format_version: u16,
    family: String,
    tier: u8,
}

/// What the pool did with an upload.
///
/// Every field defaults: the server's response grew `rejected_unknown_family`
/// after this client was written, and a desktop build must parse both shapes —
/// the alternative is a released app that reads a successful upload as a
/// protocol error the first time the server ships a new counter.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
struct UploadAck {
    #[serde(default)]
    stored: u32,
    #[serde(default)]
    duplicate: u32,
    #[serde(default)]
    capped: u32,
    #[serde(default)]
    tombstoned: u32,
    /// Templates the server could not decode at all.
    #[serde(default)]
    rejected: u32,
    /// Templates whose family this server's vocabulary does not carry.
    ///
    /// Not a client bug and not retryable: it means the server is running an
    /// older support vocabulary than this build, which is a deploy-order
    /// problem. Logged once, named as such.
    #[serde(default)]
    rejected_unknown_family: u32,
}

/// The served corpus.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
struct CorpusBody {
    #[serde(default)]
    format_version: u16,
    #[serde(default)]
    dedupe_threshold: f32,
    #[serde(default)]
    known_family_count: u32,
    #[serde(default)]
    templates: Vec<WireTemplate>,
    #[serde(default)]
    tombstones: Vec<WireKey>,
}

// ---------------------------------------------------------------------------
// Slice-facing status
// ---------------------------------------------------------------------------

/// How the last pull ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullResult {
    /// No pull has finished this session.
    #[default]
    Never,
    /// A corpus came back and was merged (possibly adding nothing).
    Merged,
    /// The server answered 304 — the local copy already holds what it serves.
    Unchanged,
    /// The pull failed. The store is untouched and the module is running on
    /// local templates.
    Failed,
}

/// What the page shows about the shared pool.
///
/// Composed onto the `mercenary` slice at read time from `AppState.merc_sync`,
/// the same way the enabled-guide echo is: the uploader and the pull run on
/// their own tasks, and giving them a second copy of the slice to write would
/// put two writers on it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercSyncStatus {
    /// Unix ms of the last finished pull, or `None` if none has finished.
    pub last_pull_ms: Option<u64>,
    pub last_pull: PullResult,
    /// Samples in the store that came from the pool.
    pub pooled_samples: usize,
    /// Local samples still waiting to be offered.
    pub queued_uploads: usize,
    /// Why the last pool call failed, or `None`. Distinct from the slice's own
    /// `last_error`: the module working fine on local templates while the pool
    /// is unreachable is not a capture error.
    pub last_error: Option<String>,
}

// ---------------------------------------------------------------------------
// Persisted sync state
// ---------------------------------------------------------------------------

/// `pool-sync.json` — what the pull and the tombstones need to survive a
/// restart.
///
/// Separate from `index.json` because it is about the SERVER conversation, not
/// about the store's contents: the index describes samples, this describes what
/// the pool has been told and what it last served.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncFile {
    #[serde(default)]
    pub format_version: u16,
    /// The ETag of the corpus this store was last merged from. Sent as
    /// `If-None-Match`, which turns an up-to-date pull into 33 bytes of header.
    #[serde(default)]
    pub etag: Option<String>,
    /// Keys forgotten locally whose tombstone the server has not confirmed.
    ///
    /// They suppress the pooled samples for those keys on every pull until the
    /// POST lands, because until then the corpus still serves the art the user
    /// disowned and merging it would undo the forget once per module start.
    #[serde(default)]
    pub pending_tombstones: Vec<WireKey>,
}

impl SyncFile {
    /// Read the file, or a fresh one.
    ///
    /// A file written under a different format version is discarded whole
    /// rather than partially honoured: its ETag names a corpus in another
    /// version's key space, and its pending tombstones were POSTed with that
    /// version. Starting over is what "a bump refills the pool" means on this
    /// side.
    pub fn load(dir: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(dir.join(SYNC_FILE)) else {
            return Self::fresh();
        };
        match serde_json::from_str::<Self>(&raw) {
            Ok(file) if file.format_version == FORMAT_VERSION => file,
            _ => Self::fresh(),
        }
    }

    fn fresh() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            etag: None,
            pending_tombstones: Vec::new(),
        }
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(SYNC_FILE), json).map_err(|e| e.to_string())
    }

    /// The suppression list [`super::icons::TemplateStore::merge_pulled`] takes.
    pub fn suppressed(&self) -> Vec<(String, u8)> {
        self.pending_tombstones
            .iter()
            .map(|k| (k.family.clone(), k.tier))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// In-memory sync state
// ---------------------------------------------------------------------------

/// One signature queued for the pool.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingSample {
    pub family: String,
    pub tier: u8,
    /// The 576 signature bytes, copied out of the store while its mutex was
    /// held. Copied rather than referenced on purpose: the uploader outlives
    /// the lock, and a forget may drop the sample while a batch is in flight.
    pub gray: Vec<u8>,
}

/// The pool conversation's live state — `AppState.merc_sync`.
#[derive(Debug, Default)]
pub struct SyncState {
    /// Samples waiting to be offered.
    queue: Vec<PendingSample>,
    /// Whether a drain task is running. Single-flight: the queue has exactly
    /// one reader.
    uploading: bool,
    /// Whether a pull is in flight. Single-flight: a module toggled repeatedly
    /// must not start a pull storm.
    pulling: bool,
    /// Whether the capture loop's start is still willing to merge a landed
    /// corpus itself. While set, a finished pull parks its corpus in `landed`
    /// instead of applying it.
    startup_claim: bool,
    /// A corpus, and the ETag it was served under, waiting for the loop's start
    /// to pick it up. The tag travels WITH the corpus because it may only be
    /// stored once that corpus is merged and saved.
    landed: Option<(PooledCorpus, Option<String>)>,
    /// Whether this session has already said the server is unreachable.
    unreachable_logged: bool,
    /// What the page shows.
    status: MercSyncStatus,
}

fn with_state<T>(app: &AppHandle, f: impl FnOnce(&mut SyncState) -> T) -> T {
    let state = app.state::<AppState>();
    let mut guard = state.merc_sync.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// The status the SSOT composes onto the mercenary slice.
pub fn status(state: &AppState) -> MercSyncStatus {
    state
        .merc_sync
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .status
        .clone()
}

/// Re-arm the once-per-session log gate. Called from the capture loop's start,
/// so "session" means "one module run": a user who toggles the module off and
/// on after fixing their network gets told the truth again.
pub fn begin_session(app: &AppHandle) {
    with_state(app, |s| s.unreachable_logged = false);
}

/// Log a pool failure at most once per session, and record it for the page.
///
/// The page always sees the latest failure — that field is a status, not a log
/// — but the log itself gets one line. A capture loop that appends "pool
/// unreachable" on every tick buries the lines that matter.
fn note_failure(app: &AppHandle, detail: String) {
    let first = with_state(app, |s| {
        s.status.last_error = Some(detail.clone());
        let first = !s.unreachable_logged;
        s.unreachable_logged = true;
        first
    });
    if first {
        crate::app_log(app, format!("Merc: template pool unreachable — {detail} (running on local templates; this is the only line this session)"));
    }
}

fn note_success(app: &AppHandle) {
    with_state(app, |s| {
        s.status.last_error = None;
        s.unreachable_logged = false;
    });
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Split queued samples into request bodies no server has to reject.
///
/// Pure so the batch boundary is testable without a server, and shared by the
/// hover path (one sample) and the first-publish backfill (up to 792).
pub fn build_batches(samples: &[PendingSample]) -> Vec<UploadBody> {
    samples
        .chunks(MAX_TEMPLATES_PER_BATCH)
        .map(|chunk| UploadBody {
            format_version: FORMAT_VERSION,
            templates: chunk.iter().map(wire_template).collect(),
        })
        .collect()
}

/// One sample as it goes on the wire.
fn wire_template(sample: &PendingSample) -> WireTemplate {
    WireTemplate {
        family: sample.family.clone(),
        tier: sample.tier,
        signature_b64: BASE64.encode(&sample.gray),
    }
}

/// How long to wait after a 429.
///
/// Handles the delta-seconds form ONLY. `Retry-After` also has an HTTP-date
/// form, which this does not parse and which falls through to
/// [`RETRY_FALLBACK`] like any other unreadable value — deliberate, because the
/// one server that sends this header builds it with `strconv.Itoa` on a second
/// count (`allowMercWrite`, `internal/server/handlers/mercenary.go`), and a date
/// parser for a form that endpoint cannot emit would be untested code guarding
/// nothing. The fallback is what keeps the unparsed case safe: 30 s of backoff,
/// never a zero-wait hammer.
///
/// Deliberately NOT `crate::retry_after_delay`: that one clamps to five seconds
/// because it backs off a saturated persist queue, where waiting longer than the
/// queue takes to drain is pointless. This backs off a rate limit whose window is
/// ten minutes, so honouring a wait of minutes is the correct behaviour and
/// clamping to five seconds would just burn the remaining budget.
fn retry_after_delay(header: Option<&str>) -> Duration {
    header
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(RETRY_FALLBACK)
}

/// Turn a served corpus into the plain form the merge takes.
///
/// A template whose base64 does not decode to a usable signature is DROPPED,
/// not fatal: the rest of the corpus is good art, and one bad row must not cost
/// a device the whole pool. Returns the corpus and how many entries were
/// dropped.
fn decode_corpus(body: CorpusBody) -> (PooledCorpus, usize) {
    let mut dropped = 0usize;
    let mut samples = Vec::with_capacity(body.templates.len());
    for item in body.templates {
        let Ok(bytes) = BASE64.decode(item.signature_b64.as_bytes()) else {
            dropped += 1;
            continue;
        };
        let Some(sig) = CellSig::from_gray(bytes) else {
            dropped += 1;
            continue;
        };
        samples.push(PooledSample {
            family: item.family,
            tier: item.tier,
            sig,
        });
    }
    let corpus = PooledCorpus {
        format_version: body.format_version,
        samples,
        tombstones: body
            .tombstones
            .into_iter()
            .map(|k| (k.family, k.tier))
            .collect(),
    };
    (corpus, dropped)
}

/// The template directory, or `None` when there is no app data directory —
/// the same condition under which the store itself is never loaded or saved.
fn icons_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(super::ICONS_DIR))
}

fn server_and_http(app: &AppHandle) -> (String, reqwest::Client) {
    let state = app.state::<AppState>();
    let url = state
        .server_url
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    (url, state.server_http.clone())
}

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
///   will apply whatever lands. It is taken unconditionally, including when the
///   request itself was not claimed because a pull from a previous module start
///   is still in flight. That case is the whole reason it is unconditional: an
///   off→on toggle across an in-flight pull used to leave the claim unset, so
///   the corpus applied itself against a store the restarting loop had not
///   installed yet.
///
/// The caller stays on its own thread — this returns immediately.
pub fn spawn_pull(app: &AppHandle) {
    let claimed = with_state(app, |s| {
        // The seam is starting either way; only the request may already be
        // somebody else's.
        s.startup_claim = true;
        if s.pulling {
            return false;
        }
        s.pulling = true;
        true
    });
    if !claimed {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
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
            let thresholds = super::load_override(&data_dir).0.thresholds;
            apply_corpus(
                &app,
                &data_dir.join(super::ICONS_DIR),
                &corpus,
                served_etag,
                &thresholds,
                true,
            );
        }
    });
}

/// Record a finished pull. Returns the corpus (and its ETag) when the caller has
/// to apply it itself — the load seam already released its claim.
fn finish_pull(
    app: &AppHandle,
    outcome: Option<PullOutcome>,
) -> Option<(PooledCorpus, Option<String>)> {
    let now = super::run::now_ms();
    let to_apply = with_state(app, |s| record_pull(s, now, outcome));
    crate::ssot::emit_ssot(app);
    to_apply
}

/// Fold a finished pull into the sync state, answering who applies the corpus.
///
/// `None` means it was PARKED for a load seam that is still holding its claim;
/// `Some` means the seam is gone and the caller must apply it against the store
/// the seam already installed. A corpus is never dropped on this branch — the
/// two answers are "somebody else will" and "you must", and there is no third.
///
/// Pure over [`SyncState`] because this handshake is the only thing standing
/// between a merged corpus and a whole-store write that would erase it.
fn record_pull(
    state: &mut SyncState,
    now_ms: u64,
    outcome: Option<PullOutcome>,
) -> Option<(PooledCorpus, Option<String>)> {
    state.pulling = false;
    state.status.last_pull_ms = Some(now_ms);
    match outcome {
        None => {
            state.status.last_pull = PullResult::Failed;
            None
        }
        Some(PullOutcome::NotModified) => {
            state.status.last_pull = PullResult::Unchanged;
            None
        }
        Some(PullOutcome::Corpus(corpus, etag)) => {
            state.status.last_pull = PullResult::Merged;
            if state.startup_claim {
                state.landed = Some((corpus, etag));
                None
            } else {
                Some((corpus, etag))
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
fn claim_step(
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
/// installed into `merc_templates`. That ordering is the single-writer rule: the
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
/// the seam let go. Both take `merc_templates` and do the whole read-modify-write
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
    thresholds: &super::Thresholds,
    bump: bool,
) {
    let suppressed = SyncFile::load(dir).suppressed();

    let (outcome, learned, pooled, pooled_samples, save_err) = {
        let state = app.state::<AppState>();
        let mut store = state
            .merc_templates
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let outcome = store.merge_pulled(corpus, &suppressed, thresholds);
        let save_err = if outcome.changed() {
            store.save(dir).err()
        } else {
            None
        };
        (
            outcome,
            store.learned_keys(),
            store.pooled_keys(),
            store.pooled_samples(),
            save_err,
        )
    };

    crate::app_log(app, merge_line(&outcome));
    match save_err {
        // The tag is stored only once the corpus it names is on disk — a tag
        // ahead of the store earns a 304 that keeps the pool's art out for good.
        None => persist_etag(app, dir, etag, outcome.suppressed),
        Some(e) => crate::app_log(app, format!("Merc: pooled templates not saved — {e}")),
    }
    with_state(app, |s| s.status.pooled_samples = pooled_samples);
    if bump && outcome.changed() {
        // The loop is holding confirmations from before the merge, and a
        // tombstoned key it still re-applies is the forget not working.
        super::debug::bump_generation(app);
    }
    super::run::publish(app, |slice| {
        slice.learned_families = learned;
        slice.pooled_families = pooled;
    });
}

/// One line describing a merge, for the log.
pub fn merge_line(outcome: &super::icons::MergeOutcome) -> String {
    if outcome.foreign_version {
        return format!(
            "Merc: pooled corpus declares another signature format — nothing merged (this build reads version {FORMAT_VERSION})"
        );
    }
    format!(
        "Merc: pool merged — {} added, {} replaced by a tombstone, {} already known, {} held for a pending forget",
        outcome.added, outcome.replaced, outcome.skipped, outcome.suppressed
    )
}

// ---------------------------------------------------------------------------
// Upload
// ---------------------------------------------------------------------------

/// Offer samples to the pool.
///
/// Returns immediately: the POST happens on a task, never on the capture tick.
/// The first Windows smoke measured a 4 s tick with OCR alone, and a
/// synchronous upload there would stall the read the user is waiting on.
pub fn enqueue(app: &AppHandle, samples: Vec<PendingSample>) {
    if samples.is_empty() {
        return;
    }
    let start = with_state(app, |s| {
        s.queue.extend(samples);
        s.status.queued_uploads = s.queue.len();
        if s.uploading {
            return false;
        }
        s.uploading = true;
        true
    });
    crate::ssot::emit_ssot(app);
    if !start {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { drain_queue(app).await });
}

/// Send everything queued, one batch at a time, then stop.
///
/// One drain task at a time (`uploading`), so the queue has a single reader and
/// batches keep their order.
async fn drain_queue(app: AppHandle) {
    loop {
        let batch = with_state(&app, |s| {
            let take = s.queue.len().min(MAX_TEMPLATES_PER_BATCH);
            let batch: Vec<PendingSample> = s.queue.drain(..take).collect();
            if batch.is_empty() {
                s.uploading = false;
            }
            s.status.queued_uploads = s.queue.len();
            batch
        });
        if batch.is_empty() {
            crate::ssot::emit_ssot(&app);
            return;
        }
        // Only an acknowledgement that SETTLES the offer closes it out. An
        // outcome the pool invites a retry on leaves the samples owed, and the
        // whole batch is offered again at the next module start — the server
        // dedupes, so re-offering the settled part of it costs one request —
        // per module start, for as long as a key in the batch sits at the
        // cap (a steady state until someone retires a sample), so the
        // recurring "N at the cap" log line is expected, not a fault.
        if let Some(ack) = send_batch(&app, &batch).await {
            if should_mark_published(&ack) {
                mark_uploaded(&app, &batch);
            }
        }
        crate::ssot::emit_ssot(&app);
    }
}

/// Whether an acknowledged batch may be recorded as published.
///
/// Two outcomes are NOT settled and must leave the samples owed:
///
/// - `capped` — the pool refused only because the key already holds three live
///   samples. A tombstone can free a slot at any time, and the server's own
///   comment calls this the outcome that "invites a retry once a slot frees".
/// - `rejected_unknown_family` — the SERVER's support vocabulary is older than
///   this build's. A deploy fixes it, and marking the offer published would
///   retire art the pool wants as soon as it ships.
///
/// The other two ARE settled: `duplicate` means the pool already holds this art,
/// and `rejected` means this build sent bytes the server cannot decode. Sending
/// the identical payload again changes neither.
fn should_mark_published(ack: &UploadAck) -> bool {
    ack.capped == 0 && ack.rejected_unknown_family == 0
}

/// POST one batch, honouring the rate limit. The acknowledgement when the pool
/// answered, `None` when it never did.
///
/// A batch the pool never answered is DROPPED rather than requeued: its samples
/// are still `uploaded: false` on disk, so the next module start offers them
/// again. Requeuing inside the session would turn an outage into a task that
/// never ends.
async fn send_batch(app: &AppHandle, batch: &[PendingSample]) -> Option<UploadAck> {
    let (server, http) = server_and_http(app);
    let url = format!("{server}/api/desktop/merc-templates");
    let bodies = build_batches(batch);
    let Some(body) = bodies.first() else {
        return None;
    };

    for attempt in 1..=MAX_ATTEMPTS {
        let response = http
            .post(&url)
            .timeout(REQUEST_TIMEOUT)
            .json(body)
            .send()
            .await;
        match response {
            Ok(res) if res.status().is_success() => {
                note_success(app);
                let ack: UploadAck = res.json().await.unwrap_or_default();
                if ack.rejected_unknown_family > 0 {
                    crate::app_log(app, format!(
                        "Merc: the pool refused {} template(s) as unknown families — its support vocabulary is older than this build's; they are not retried",
                        ack.rejected_unknown_family,
                    ));
                }
                crate::app_log(app, format!(
                    "Merc: offered {} template(s) to the pool — {} stored, {} already pooled, {} at the cap, {} retired, {} malformed",
                    body.templates.len(), ack.stored, ack.duplicate, ack.capped, ack.tombstoned, ack.rejected,
                ));
                return Some(ack);
            }
            Ok(res) if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS => {
                let wait = retry_after_delay(
                    res.headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()),
                );
                if wait > RETRY_CEILING || attempt == MAX_ATTEMPTS {
                    note_failure(
                        app,
                        format!("upload: rate limited, retry in {}s", wait.as_secs()),
                    );
                    return None;
                }
                tokio::time::sleep(wait).await;
            }
            // A 4xx that is not the rate limit is this build's problem — a
            // format version the server refuses, a body it cannot read, or a 413
            // for a batch over the 32 KB cap. Retrying sends the identical bytes
            // and gets the identical answer, so the batch is dropped and left
            // owed on disk; a 413 in particular would come back at the SAME size,
            // because nothing here re-splits a rejected batch. The guard against
            // it is upstream, in [`MAX_TEMPLATES_PER_BATCH`] and the vocabulary
            // body-cap test that sizes it — not a retry loop.
            Ok(res) if res.status().is_client_error() => {
                let status = res.status();
                let text = res.text().await.unwrap_or_default();
                note_failure(app, format!("upload: {status} — {}", text.trim()));
                return None;
            }
            Ok(res) => {
                if attempt == MAX_ATTEMPTS {
                    note_failure(app, format!("upload: server returned {}", res.status()));
                    return None;
                }
                tokio::time::sleep(RETRY_BASE * attempt).await;
            }
            Err(e) => {
                if attempt == MAX_ATTEMPTS {
                    note_failure(app, format!("upload: {e}"));
                    return None;
                }
                tokio::time::sleep(RETRY_BASE * attempt).await;
            }
        }
    }
    None
}

/// Mark a placed batch in the store and persist it, so a restart does not
/// re-offer what the pool already has.
fn mark_uploaded(app: &AppHandle, batch: &[PendingSample]) {
    let Some(dir) = icons_dir(app) else {
        return;
    };
    let save_err = {
        let state = app.state::<AppState>();
        let mut store = state
            .merc_templates
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut touched = false;
        for sample in batch {
            touched |= store.mark_uploaded(&sample.family, sample.tier, &sample.gray);
        }
        if touched {
            store.save(&dir).err()
        } else {
            None
        }
    };
    if let Some(e) = save_err {
        crate::app_log(app, format!("Merc: upload flags not saved — {e}"));
    }
}

/// Offer every local sample the pool has not seen. Called once per module
/// start, which is what publishes a store learned while the pool did not exist
/// yet (or while the device was offline).
pub fn enqueue_backfill(app: &AppHandle) {
    let pending: Vec<PendingSample> = {
        let state = app.state::<AppState>();
        let store = state
            .merc_templates
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        store
            .pending_uploads()
            .into_iter()
            .map(|(family, tier, gray)| PendingSample { family, tier, gray })
            .collect()
    };
    if pending.is_empty() {
        return;
    }
    crate::app_log(
        app,
        format!("Merc: offering {} learned template(s) to the pool", pending.len()),
    );
    enqueue(app, pending);
}

// ---------------------------------------------------------------------------
// Tombstone
// ---------------------------------------------------------------------------

/// Make a local forget stick for everyone.
///
/// Recorded locally FIRST and only then sent: between the forget and the
/// server's acknowledgement the corpus still serves the disowned art, so the
/// key has to be suppressed on the local side or the very next pull puts it
/// back. The record is dropped when the POST lands.
///
/// Fire-and-forget with bounded retry, like the upload — a forget must not
/// block the page's button on a network round-trip.
pub fn spawn_tombstone(app: &AppHandle, family: String, tier: u8) {
    if let Some(dir) = icons_dir(app) {
        if let Err(e) = record_pending_in(&dir, &family, tier) {
            crate::app_log(app, format!("Merc: could not record the forget — {e}"));
        }
    }
    send_and_clear(app, family, tier);
}

/// Offer every forget the server has not acknowledged.
///
/// Called once per module start, next to [`enqueue_backfill`]. Without it the
/// three attempts a forget gets are the only ones it ever gets: a device that
/// was offline when the user clicked ✕ would suppress that key locally forever,
/// and — because a suppressed merge never stores the pull's ETag — would also
/// re-download the whole corpus and re-run its tombstone replace at every start.
pub fn retry_pending_tombstones(app: &AppHandle) {
    let Some(dir) = icons_dir(app) else {
        return;
    };
    let owed = pending_tombstones(&dir);
    if owed.is_empty() {
        return;
    }
    crate::app_log(
        app,
        format!("Merc: retrying {} forget(s) the pool has not acknowledged", owed.len()),
    );
    for (family, tier) in owed {
        send_and_clear(app, family, tier);
    }
}

/// POST one tombstone on a task and clear its local record if it lands.
///
/// Fire-and-forget with bounded retry: a forget must not block the page's button
/// on a network round-trip, and a retry budget that outlived the session would be
/// a task nobody is watching. The durable retry is the record on disk, which
/// [`retry_pending_tombstones`] offers again at the next module start.
fn send_and_clear(app: &AppHandle, family: String, tier: u8) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if send_tombstone(&app, &family, tier).await {
            clear_pending_tombstone(&app, &family, tier);
            crate::app_log(
                &app,
                format!("Merc: pool tombstoned {family} (tier {tier})"),
            );
        } else {
            crate::app_log(&app, format!(
                "Merc: pool was not told about forgetting {family} (tier {tier}) — the key stays suppressed locally, and the next module start offers it again",
            ));
        }
    });
}

/// The forgets a module start owes the server: the keys recorded locally whose
/// tombstone the pool has not acknowledged.
pub fn pending_tombstones(dir: &Path) -> Vec<(String, u8)> {
    SyncFile::load(dir).suppressed()
}

/// Record a forget as owed. `true` when the file changed. App-free so the
/// record → offer → clear cycle is testable without a server.
fn record_pending_in(dir: &Path, family: &str, tier: u8) -> Result<bool, String> {
    let mut file = SyncFile::load(dir);
    let key = WireKey {
        family: family.to_string(),
        tier,
    };
    if file.pending_tombstones.contains(&key) {
        return Ok(false);
    }
    file.pending_tombstones.push(key);
    file.save(dir)?;
    Ok(true)
}

/// Drop an acknowledged forget from the owed list, and drop the cached ETag with
/// it. `true` when the file changed.
///
/// The ETag goes because the suppression on this key has just lifted, and the
/// last pull skipped the key's served samples on account of it. Keeping the tag
/// would earn a 304 that can never deliver them.
fn clear_pending_in(dir: &Path, family: &str, tier: u8) -> Result<bool, String> {
    let mut file = SyncFile::load(dir);
    let before = file.pending_tombstones.len();
    file.pending_tombstones
        .retain(|k| !(k.family == family && k.tier == tier));
    if file.pending_tombstones.len() == before {
        return Ok(false);
    }
    file.etag = None;
    file.save(dir)?;
    Ok(true)
}

async fn send_tombstone(app: &AppHandle, family: &str, tier: u8) -> bool {
    let (server, http) = server_and_http(app);
    let url = format!("{server}/api/desktop/merc-templates/tombstone");
    let body = TombstoneBody {
        format_version: FORMAT_VERSION,
        family: family.to_string(),
        tier,
    };
    for attempt in 1..=MAX_ATTEMPTS {
        match http
            .post(&url)
            .timeout(REQUEST_TIMEOUT)
            .json(&body)
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => {
                note_success(app);
                return true;
            }
            Ok(res) if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS => {
                let wait = retry_after_delay(
                    res.headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()),
                );
                if wait > RETRY_CEILING || attempt == MAX_ATTEMPTS {
                    note_failure(app, "tombstone: rate limited".to_string());
                    return false;
                }
                tokio::time::sleep(wait).await;
            }
            Ok(res) if res.status().is_client_error() => {
                note_failure(app, format!("tombstone: {}", res.status()));
                return false;
            }
            Ok(res) => {
                if attempt == MAX_ATTEMPTS {
                    note_failure(app, format!("tombstone: server returned {}", res.status()));
                    return false;
                }
                tokio::time::sleep(RETRY_BASE * attempt).await;
            }
            Err(e) => {
                if attempt == MAX_ATTEMPTS {
                    note_failure(app, format!("tombstone: {e}"));
                    return false;
                }
                tokio::time::sleep(RETRY_BASE * attempt).await;
            }
        }
    }
    false
}

fn clear_pending_tombstone(app: &AppHandle, family: &str, tier: u8) {
    let Some(dir) = icons_dir(app) else {
        return;
    };
    if let Err(e) = clear_pending_in(&dir, family, tier) {
        crate::app_log(app, format!("Merc: could not clear the forget record — {e}"));
    }
}

/// Forget everything the pool told this device about the conversation so far.
///
/// The local-only reset ([`super::debug::merc_reset_templates`]) empties the
/// store; without this the cached ETag would still name the corpus that store
/// was built from, and the next pull would answer 304 — leaving a device that
/// asked for a clean slate with a permanently empty store. Nothing is sent:
/// a reset says nothing about the pool's art being wrong.
pub fn forget_etag(app: &AppHandle) {
    let Some(dir) = icons_dir(app) else {
        return;
    };
    let mut file = SyncFile::load(&dir);
    if file.etag.is_none() {
        return;
    }
    file.etag = None;
    if let Err(e) = file.save(&dir) {
        crate::app_log(app, format!("Merc: could not clear the pool ETag — {e}"));
    }
}

/// Record the store's pooled-sample count for the page. Called from the load
/// seam, where the merged store is in hand.
pub fn set_pooled_samples(app: &AppHandle, count: usize) {
    with_state(app, |s| s.status.pooled_samples = count);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(family: &str, tier: u8, fill: u8) -> PendingSample {
        PendingSample {
            family: family.to_string(),
            tier,
            gray: vec![fill; (super::super::icons::SIG_DIM * super::super::icons::SIG_DIM) as usize],
        }
    }

    /// The wire type is the whole defence against a colour crop leaving the
    /// device: it must serialise to the three fields the server names and
    /// nothing else. Asserting on the KEY SET rather than on a rendered string
    /// is what makes this fail if someone adds `raw` to the struct.
    #[test]
    fn an_upload_template_carries_only_the_key_and_the_signature() {
        let body = &build_batches(&[sample("Chain", 2, 7)])[0];

        let json = serde_json::to_value(&body.templates[0]).expect("serialises");
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();

        assert_eq!(keys, ["family", "signature_b64", "tier"]);
    }

    /// The signature on the wire is the store's bytes, base64 of exactly 576 of
    /// them — the server rejects any other length outright.
    #[test]
    fn an_upload_template_encodes_the_576_signature_bytes() {
        let body = &build_batches(&[sample("Chain", 2, 7)])[0];

        let decoded = BASE64
            .decode(body.templates[0].signature_b64.as_bytes())
            .expect("round-trips");

        assert_eq!(decoded.len(), 576);
        assert!(decoded.iter().all(|b| *b == 7), "the store's own bytes");
    }

    /// The batch boundary is what keeps a full first publish under the server's
    /// 32 KB body cap.
    #[test]
    fn a_full_store_is_split_into_batches_at_the_cap() {
        let samples: Vec<PendingSample> = (0..MAX_TEMPLATES_PER_BATCH + 1)
            .map(|i| sample("Chain", 2, i as u8))
            .collect();

        let batches = build_batches(&samples);

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].templates.len(), MAX_TEMPLATES_PER_BATCH);
        assert_eq!(batches[1].templates.len(), 1, "the remainder is its own batch");
    }

    /// A full batch has to fit the server's body cap with the WORST family name
    /// the shipped vocabulary carries — the batch size is a byte budget, and a
    /// batch over the cap is a 413 that places nothing.
    ///
    /// Derived from the vocabulary rather than from a remembered name, so a
    /// league that adds a longer family fails here instead of on a user's first
    /// publish.
    #[test]
    fn a_full_batch_fits_the_servers_body_cap_for_the_longest_family() {
        let vocab = super::super::vocab::MercVocab::load().expect("the vocabulary parses");
        let longest = vocab
            .by_role(super::super::vocab::MercRole::Support)
            .map(|s| s.family.as_str())
            .max_by_key(|f| f.len())
            .expect("the vocabulary carries supports");
        let samples: Vec<PendingSample> = (0..MAX_TEMPLATES_PER_BATCH)
            .map(|i| sample(longest, 3, i as u8))
            .collect();

        let body = &build_batches(&samples)[0];
        let encoded = serde_json::to_vec(body).expect("serialises");

        assert!(
            encoded.len() <= SERVER_BODY_LIMIT,
            "a full batch of {longest:?} is {} bytes, over the server's {SERVER_BODY_LIMIT}",
            encoded.len(),
        );
    }

    /// A batch that exactly fills the cap must not produce a trailing empty
    /// request — the server answers 400 to `templates: []`.
    #[test]
    fn an_exactly_full_batch_produces_one_request() {
        let samples: Vec<PendingSample> = (0..MAX_TEMPLATES_PER_BATCH)
            .map(|i| sample("Chain", 2, i as u8))
            .collect();

        let batches = build_batches(&samples);

        assert_eq!(batches.len(), 1);
    }

    #[test]
    fn nothing_queued_produces_no_requests() {
        assert!(build_batches(&[]).is_empty());
    }

    /// A rate limit's `Retry-After` is honoured as given — this is the header
    /// that decides whether the next attempt spends the last of the device's
    /// budget or waits for it to refill.
    #[test]
    fn retry_after_honours_the_seconds_the_server_named() {
        assert_eq!(retry_after_delay(Some("45")), Duration::from_secs(45));
    }

    /// Whitespace is legal around a header value.
    #[test]
    fn retry_after_tolerates_surrounding_whitespace() {
        assert_eq!(retry_after_delay(Some(" 45 ")), Duration::from_secs(45));
    }

    /// A 429 without the header still has to back off — and by more than the
    /// couple of seconds a queue-shedding retry uses, because the budget it is
    /// waiting on refills over ten minutes.
    #[test]
    fn retry_after_falls_back_when_the_header_is_absent() {
        assert_eq!(retry_after_delay(None), RETRY_FALLBACK);
    }

    /// An HTTP-date `Retry-After` is legal and this parser does not read it;
    /// it must fall back rather than treat the unparsed value as zero and
    /// hammer the endpoint.
    #[test]
    fn retry_after_falls_back_on_a_value_it_cannot_parse() {
        assert_eq!(
            retry_after_delay(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            RETRY_FALLBACK
        );
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "poe-merc-sync-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// The ETag is what turns an up-to-date pull into a 304, so it has to
    /// survive a restart — together with the forgets the server has not
    /// acknowledged.
    #[test]
    fn the_sync_file_round_trips_the_etag_and_the_pending_forgets() {
        let dir = temp_dir("roundtrip");
        let file = SyncFile {
            format_version: FORMAT_VERSION,
            etag: Some("\"abc123\"".to_string()),
            pending_tombstones: vec![WireKey {
                family: "Chain".to_string(),
                tier: 2,
            }],
        };
        file.save(&dir).expect("save");

        let loaded = SyncFile::load(&dir);

        assert_eq!(loaded.etag.as_deref(), Some("\"abc123\""));
        assert_eq!(loaded.suppressed(), [("Chain".to_string(), 2)]);
    }

    /// A missing file is a fresh conversation, not an error — every first run.
    #[test]
    fn a_missing_sync_file_loads_as_a_fresh_one() {
        let dir = temp_dir("absent").join("never-created");

        let loaded = SyncFile::load(&dir);

        assert_eq!(loaded.format_version, FORMAT_VERSION);
        assert!(loaded.etag.is_none());
        assert!(loaded.pending_tombstones.is_empty());
    }

    /// A file from another signature format must not lend its ETag to this
    /// one: the tag names a corpus in a different key space, and honouring it
    /// would answer 304 for a pool this build has never read.
    #[test]
    fn a_sync_file_from_another_format_version_is_discarded() {
        let dir = temp_dir("foreign");
        let file = SyncFile {
            format_version: FORMAT_VERSION + 1,
            etag: Some("\"from-v2\"".to_string()),
            pending_tombstones: vec![WireKey {
                family: "Chain".to_string(),
                tier: 2,
            }],
        };
        file.save(&dir).expect("save");

        let loaded = SyncFile::load(&dir);

        assert_eq!(loaded.format_version, FORMAT_VERSION);
        assert!(loaded.etag.is_none(), "the foreign tag is not reused");
        assert!(loaded.pending_tombstones.is_empty());
    }

    /// The response grew a counter after this client shipped. Both shapes have
    /// to parse, or a released build reads a successful upload as a protocol
    /// error the day the server deploys.
    #[test]
    fn an_upload_ack_parses_without_the_newer_counters() {
        let ack: UploadAck =
            serde_json::from_str(r#"{"stored":2,"duplicate":1,"capped":0,"tombstoned":0,"rejected":0}"#)
                .expect("parses");

        assert_eq!(ack.stored, 2);
        assert_eq!(ack.rejected_unknown_family, 0);
    }

    #[test]
    fn an_upload_ack_reads_the_unknown_family_count() {
        let ack: UploadAck = serde_json::from_str(
            r#"{"stored":0,"duplicate":0,"capped":0,"tombstoned":0,"rejected":1,"rejected_unknown_family":3}"#,
        )
        .expect("parses");

        assert_eq!(ack.rejected_unknown_family, 3);
        assert_eq!(ack.rejected, 1, "malformed and unknown-family stay separate");
    }

    /// The corpus envelope grew `known_family_count` next to the threshold;
    /// an older server's body must still decode.
    #[test]
    fn a_corpus_parses_with_or_without_the_known_family_count() {
        let older: CorpusBody = serde_json::from_str(
            r#"{"format_version":1,"dedupe_threshold":0.88,"templates":[],"tombstones":[]}"#,
        )
        .expect("parses");
        let newer: CorpusBody = serde_json::from_str(
            r#"{"format_version":1,"dedupe_threshold":0.88,"known_family_count":154,"templates":[],"tombstones":[]}"#,
        )
        .expect("parses");

        assert_eq!(older.known_family_count, 0);
        assert_eq!(newer.known_family_count, 154);
    }

    /// A corpus row whose base64 is not a usable signature is dropped, and the
    /// rest of the pool still arrives — one bad row must not cost a device
    /// every icon.
    #[test]
    fn a_corpus_drops_only_the_rows_it_cannot_decode() {
        let good = BASE64.encode(gradient());
        let body = CorpusBody {
            format_version: FORMAT_VERSION,
            dedupe_threshold: 0.88,
            known_family_count: 154,
            templates: vec![
                WireTemplate {
                    family: "Chain".to_string(),
                    tier: 2,
                    signature_b64: good,
                },
                WireTemplate {
                    family: "Pierce".to_string(),
                    tier: 1,
                    signature_b64: "!!!not base64!!!".to_string(),
                },
                WireTemplate {
                    family: "Ash".to_string(),
                    tier: 1,
                    signature_b64: BASE64.encode([1u8, 2, 3]),
                },
            ],
            tombstones: vec![WireKey {
                family: "Crush".to_string(),
                tier: 3,
            }],
        };

        let (corpus, dropped) = decode_corpus(body);

        assert_eq!(dropped, 2, "bad base64 and a wrong-length payload");
        assert_eq!(corpus.samples.len(), 1);
        assert_eq!(corpus.samples[0].family, "Chain");
        assert_eq!(corpus.tombstones, [("Crush".to_string(), 3)]);
    }

    /// A flat payload is not an icon — `CellSig` refuses it, and the corpus
    /// decoder must not smuggle it into the matcher as a sample that
    /// correlates with every empty slot.
    #[test]
    fn a_flat_corpus_row_is_dropped() {
        let body = CorpusBody {
            format_version: FORMAT_VERSION,
            templates: vec![WireTemplate {
                family: "Chain".to_string(),
                tier: 2,
                signature_b64: BASE64.encode(vec![9u8; 576]),
            }],
            ..CorpusBody::default()
        };

        let (corpus, dropped) = decode_corpus(body);

        assert_eq!(dropped, 1);
        assert!(corpus.samples.is_empty());
    }

    /// 576 bytes with real variance, so `CellSig::from_gray` accepts them.
    fn gradient() -> Vec<u8> {
        (0..576u32).map(|i| (i % 251) as u8).collect()
    }

    // -----------------------------------------------------------------------
    // The load-seam handshake
    // -----------------------------------------------------------------------

    fn one_corpus() -> PooledCorpus {
        PooledCorpus {
            format_version: FORMAT_VERSION,
            samples: Vec::new(),
            tombstones: vec![("Chain".to_string(), 2)],
        }
    }

    /// A seam that is still holding its claim gets the corpus handed to it, and
    /// the pull does NOT apply it — the seam has not installed its store yet.
    #[test]
    fn a_corpus_landing_while_the_seam_holds_the_claim_is_parked_for_it() {
        let mut state = SyncState {
            pulling: true,
            startup_claim: true,
            ..SyncState::default()
        };

        let to_apply = record_pull(
            &mut state,
            5,
            Some(PullOutcome::Corpus(one_corpus(), Some("\"e1\"".into()))),
        );

        assert!(to_apply.is_none(), "the seam applies it, not the pull");
        assert!(state.landed.is_some(), "and it is waiting for the seam");
    }

    /// The bug this pins: a corpus arriving after the seam let go must be handed
    /// BACK to be applied against the store the seam installed — not parked in a
    /// slot nobody will read again, and not dropped.
    #[test]
    fn a_corpus_landing_after_the_claim_is_released_is_handed_back_to_be_applied() {
        let mut state = SyncState {
            pulling: true,
            startup_claim: true,
            ..SyncState::default()
        };
        // The seam ran out of window and installed its store.
        let (landed, keep_waiting) = claim_step(&mut state, true);
        assert!(landed.is_none());
        assert!(!keep_waiting);
        assert!(!state.startup_claim, "the seam let go");

        let to_apply = record_pull(
            &mut state,
            5,
            Some(PullOutcome::Corpus(one_corpus(), Some("\"e1\"".into()))),
        );

        assert!(to_apply.is_some(), "the pull must apply it itself");
        assert!(state.landed.is_none(), "and it is not parked where nobody looks");
    }

    /// The seam takes the parked corpus and drops its claim in the same step, so
    /// a pull finishing between the two cannot fall through the gap.
    #[test]
    fn the_seam_takes_a_parked_corpus_and_drops_its_claim_together() {
        let mut state = SyncState {
            pulling: false,
            startup_claim: true,
            landed: Some((one_corpus(), Some("\"e1\"".into()))),
            ..SyncState::default()
        };

        let (landed, keep_waiting) = claim_step(&mut state, false);

        assert!(landed.is_some());
        assert!(!keep_waiting);
        assert!(!state.startup_claim);
    }

    /// While the request is still in flight and the window is open, the seam
    /// keeps waiting — that wait is what lets the merge happen against a store
    /// the seam has just installed.
    #[test]
    fn the_seam_keeps_waiting_while_a_pull_is_still_in_flight() {
        let mut state = SyncState {
            pulling: true,
            startup_claim: true,
            ..SyncState::default()
        };

        let (landed, keep_waiting) = claim_step(&mut state, false);

        assert!(landed.is_none());
        assert!(keep_waiting);
        assert!(state.startup_claim, "the claim is still the seam's");
    }

    /// A request that already finished (it failed, or it was another module
    /// start's and has landed) ends the wait immediately rather than burning the
    /// whole window on a corpus that is never coming.
    #[test]
    fn the_seam_stops_waiting_once_the_request_is_over() {
        let mut state = SyncState {
            pulling: false,
            startup_claim: true,
            ..SyncState::default()
        };

        let (_, keep_waiting) = claim_step(&mut state, false);

        assert!(!keep_waiting);
        assert!(!state.startup_claim);
    }

    /// A failed pull leaves no corpus for anybody and says so on the slice.
    #[test]
    fn a_failed_pull_hands_nothing_to_either_side() {
        let mut state = SyncState {
            pulling: true,
            startup_claim: true,
            ..SyncState::default()
        };

        let to_apply = record_pull(&mut state, 5, None);

        assert!(to_apply.is_none());
        assert!(state.landed.is_none());
        assert_eq!(state.status.last_pull, PullResult::Failed);
        assert!(!state.pulling, "the single-flight claim is released");
    }

    /// A 304 leaves the store alone: there is nothing to merge and nothing to
    /// hand over.
    #[test]
    fn an_unchanged_pull_hands_nothing_to_either_side() {
        let mut state = SyncState {
            pulling: true,
            startup_claim: true,
            ..SyncState::default()
        };

        let to_apply = record_pull(&mut state, 5, Some(PullOutcome::NotModified));

        assert!(to_apply.is_none());
        assert!(state.landed.is_none());
        assert_eq!(state.status.last_pull, PullResult::Unchanged);
    }

    // -----------------------------------------------------------------------
    // The tombstone retry cycle
    // -----------------------------------------------------------------------

    /// A forget the server never acknowledged is offered again at the next
    /// module start. Without the retry the three in-session attempts are all a
    /// forget ever gets, and a device that was offline when the user clicked ✕
    /// suppresses that key for good.
    #[test]
    fn a_forget_the_pool_never_acknowledged_is_offered_again() {
        let dir = temp_dir("pending-offered");
        record_pending_in(&dir, "Chain", 2).expect("record");

        assert_eq!(pending_tombstones(&dir), [("Chain".to_string(), 2)]);
    }

    /// An acknowledged forget is NOT offered again — the retry must converge.
    #[test]
    fn an_acknowledged_forget_is_not_offered_again() {
        let dir = temp_dir("pending-cleared");
        record_pending_in(&dir, "Chain", 2).expect("record");
        record_pending_in(&dir, "Pierce", 1).expect("record");

        assert!(clear_pending_in(&dir, "Chain", 2).expect("clear"));

        assert_eq!(
            pending_tombstones(&dir),
            [("Pierce".to_string(), 1)],
            "only the acknowledged key stops being owed"
        );
    }

    /// Acknowledging a forget drops the cached ETag with it. The pull that ran
    /// while the key was suppressed skipped its served samples, and a 304 is the
    /// one answer that can never deliver them.
    #[test]
    fn acknowledging_a_forget_drops_the_cached_etag() {
        let dir = temp_dir("pending-etag");
        record_pending_in(&dir, "Chain", 2).expect("record");
        let mut file = SyncFile::load(&dir);
        file.etag = Some("\"served-while-suppressed\"".into());
        file.save(&dir).expect("save");

        clear_pending_in(&dir, "Chain", 2).expect("clear");

        assert!(SyncFile::load(&dir).etag.is_none());
    }

    /// Clicking ✕ twice on one key must not owe the pool two POSTs.
    #[test]
    fn recording_the_same_forget_twice_owes_it_once() {
        let dir = temp_dir("pending-twice");
        assert!(record_pending_in(&dir, "Chain", 2).expect("record"));

        assert!(!record_pending_in(&dir, "Chain", 2).expect("record again"));

        assert_eq!(pending_tombstones(&dir).len(), 1);
    }

    /// Clearing a key nobody owes changes nothing and says so — the caller must
    /// not rewrite the file, and in particular must not drop a live ETag for a
    /// no-op.
    #[test]
    fn clearing_a_forget_nobody_owes_leaves_the_etag_alone() {
        let dir = temp_dir("pending-noop");
        let mut file = SyncFile::load(&dir);
        file.etag = Some("\"live\"".into());
        file.save(&dir).expect("save");

        assert!(!clear_pending_in(&dir, "Chain", 2).expect("clear"));

        assert_eq!(SyncFile::load(&dir).etag.as_deref(), Some("\"live\""));
    }

    // -----------------------------------------------------------------------
    // Which acknowledgements settle an offer
    // -----------------------------------------------------------------------

    fn ack(overrides: UploadAck) -> UploadAck {
        overrides
    }

    #[test]
    fn a_stored_offer_is_settled() {
        assert!(should_mark_published(&ack(UploadAck {
            stored: 3,
            ..UploadAck::default()
        })));
    }

    /// The pool already holds this art. Sending it again changes nothing, so the
    /// offer is closed out rather than retried forever.
    #[test]
    fn a_duplicate_offer_is_settled() {
        assert!(should_mark_published(&ack(UploadAck {
            duplicate: 3,
            ..UploadAck::default()
        })));
    }

    /// The key was full. A tombstone can free a slot at any time, and the server
    /// itself calls this the outcome that invites a retry — so the samples stay
    /// owed and the next module start offers them again.
    #[test]
    fn a_capped_offer_stays_owed() {
        assert!(!should_mark_published(&ack(UploadAck {
            stored: 1,
            capped: 2,
            ..UploadAck::default()
        })));
    }

    /// The SERVER's vocabulary is older than this build's. A deploy fixes it,
    /// and closing the offer out would retire art the pool wants as soon as it
    /// ships.
    #[test]
    fn an_unknown_family_offer_stays_owed() {
        assert!(!should_mark_published(&ack(UploadAck {
            rejected_unknown_family: 1,
            ..UploadAck::default()
        })));
    }

    /// Bytes the server cannot decode are this build's problem, and re-sending
    /// the identical payload gets the identical answer. Settled.
    #[test]
    fn a_malformed_offer_is_settled() {
        assert!(should_mark_published(&ack(UploadAck {
            rejected: 2,
            ..UploadAck::default()
        })));
    }

    /// The foreign-version log line has to name the version this build reads,
    /// because the only fix is a desktop update and the line is the whole
    /// diagnosis.
    #[test]
    fn the_merge_line_names_this_builds_version_when_the_corpus_is_foreign() {
        let line = merge_line(&super::super::icons::MergeOutcome {
            foreign_version: true,
            ..Default::default()
        });

        assert!(line.contains(&FORMAT_VERSION.to_string()), "{line}");
        assert!(line.contains("nothing merged"), "{line}");
    }
}
