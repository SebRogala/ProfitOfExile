//! App-wide cross-window state SSOT (POE-128 chunk 1).
//!
//! Rust-owned single source of truth for state that overlay windows need to
//! agree on. Delivery to overlays is Rust-backed **polling** via the `get_ssot`
//! command, NOT cross-window JavaScript events: WebView2 cross-window events
//! have returned stale data / failed silently (see docs/OVERLAY-GUIDE.md
//! "Runtime-earned observations"). `emit_ssot` provides an optional eager
//! `ssot-changed` nudge for the main window; overlays must still poll.
//!
//! Chunk 1 built the core types + the poll-target command. Chunk 3 adds the
//! league resolution seam: a start-only fetch task (`spawn_league_fetch`), the
//! `set_league` / `refresh_league` mutator commands, and the dual-write
//! (`write_league`) that keeps the SSOT slice and the trade client in lockstep.
//! The webview store lands in a later chunk.

use std::sync::LazyLock;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::trade::TradeApiClient;
use crate::AppState;

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

/// Wake signal for the live retry loop. `refresh_league` notifies this instead
/// of spawning a duplicate loop when a resolve is already in flight, so the
/// Settings Refresh button forces an immediate retry (and backoff reset) while
/// "Server unreachable" is showing.
///
/// Module-local `static` rather than an `AppState` field: the resolver and its
/// wake are wholly contained in this file, nothing else references the signal,
/// and there is exactly one resolver at a time (single-flight via `resolving`),
/// so a process-global singleton is semantically correct. Keeping it here holds
/// the blast radius to `ssot.rs` instead of churning the ~40-field `AppState`
/// literal for state no other module touches.
static RETRY_NOTIFY: LazyLock<Notify> = LazyLock::new(Notify::new);

/// League slice of the SSOT.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LeagueSlice {
    /// Resolved league name.
    ///
    /// `None` means **not yet fetched** — callers must fail closed and MUST NOT
    /// treat it as "always valid" or "no active league". A real league name is
    /// written only once it has been resolved (later chunks).
    pub name: Option<String>,
}

/// Which cue measured [`ScreenSlice::ui_scale`] (POE-214).
///
/// A confidence LABEL, not a rank. The merc module is the sole writer and the
/// latest measurement always wins, so a `MercOcr` reading published after a
/// `MercFrame` one replaces it rather than losing to it. What the label buys a
/// reader — and a smoke check — is the ability to tell a scale measured on the
/// support grid's gold frame from the OCR line-pitch estimate that stood in for
/// it before POE-214, and drifted 6-12 px doing so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScreenScaleSource {
    /// Measured off the merc support grid's gold frame
    /// (`mercenary::cellfit::refine`) — either on this capture, or carried onto
    /// it from an earlier capture of the same session
    /// ([`crate::mercenary::ScaleSource::Held`], which is the same frame
    /// measurement, one or more ticks old).
    MercFrame,
    /// Derived from the merc OCR line pitch — the pre-POE-214 cue. Kept as its
    /// own label rather than folded into `MercFrame` because it is the reading
    /// a session whose fit never landed is running on, and a consumer that
    /// cares how exact its rects are has to be able to see that.
    MercOcr,
    /// Loaded from settings at startup, not measured this run (WI-B2).
    Remembered,
}

/// The screen the game is drawn on and the game-UI scale measured on it
/// (POE-214).
///
/// **Unit.** `ui_scale` is game-UI px per px of the reference fixture:
/// `desktop/src-tauri/tests/fixtures/merc-skills-panel.png`, cut from a
/// 1920x1200 screen, is 1.0 by definition, and the 1080p machine measures
/// 0.90 = 1080/1200 — the game's UI scales with screen HEIGHT. The temple's
/// `AnchorCalibration::scale` is a DIFFERENT unit (relative to its own
/// `REFERENCE_SCREEN_WIDTH`, 1374) and the ratio between the two is
/// **unmeasured**, so the temple cannot read this slice until someone measures
/// it. Said here rather than implied, because the two fields spell the same
/// word.
///
/// **Reader rule.** A non-merc consumer reads THIS slice, never
/// `mercenary.capture.scale`. Both are written from the same settled
/// `MercLayout::scale` on the same tick, so they never disagree — but the
/// capture is `None` until a recruit window opens and is retired again when it
/// closes, so a reader keyed on it would lose the screen's scale every time the
/// player shuts the window. This slice outlives the capture.
///
/// **Writer.** The merc detect tick is the SOLE writer, through
/// [`publish_screen`]; WI-B2 adds the startup load of a persisted value under
/// [`ScreenScaleSource::Remembered`]. There is no rank between writers because
/// there is only one: the latest measurement wins.
///
/// **No `PartialEq`, deliberately.** Whole-struct equality would compare
/// `measured_at_ms` and `ui_scale` exactly — the two fields the publish gate
/// must NOT compare that way (the stamp moves every tick, the scale wobbles;
/// see [`screen_changed`]). Deriving it would put a `current == next` one-liner
/// within reach that silently re-emits on every tick, so the derive is left off
/// and [`screen_changed`] is the only equality question anyone gets to ask.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSlice {
    /// Captured screen width in physical px.
    pub width: u32,
    /// Captured screen height in physical px.
    pub height: u32,
    /// Game-UI px per reference-fixture px — see the unit note above.
    pub ui_scale: f32,
    /// What measured `ui_scale`. A label, not a precedence.
    pub source: ScreenScaleSource,
    /// Unix ms the measurement was taken at.
    pub measured_at_ms: u64,
}

/// Full app-wide SSOT snapshot. Cloned for both the poll response and the
/// eager event payload, so it stays cheap and `Send`.
///
/// The `Default` gives `league.name == None` (fail-closed) — locked by the
/// unit test below. Future slices are added as sibling fields here.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSsotSnapshot {
    pub league: LeagueSlice,
    /// A league resolve is currently in flight (the bounded-retry fetch task is
    /// running). Exposed in the polled snapshot so the Settings UI can show an
    /// honest "Resolving…" for the whole retry window instead of a one-frame
    /// flash. Doubles as the single-flight guard (see `try_begin_resolving`):
    /// while `true`, a second `refresh_league` wakes the live loop rather than
    /// stacking a duplicate infinite-retry loop. `Default` is `false`.
    pub resolving: bool,
    /// The league resolver has failed `LEAGUE_UNREACHABLE_AFTER_ATTEMPTS`
    /// consecutive times and is now treating the server as unreachable — while
    /// STILL retrying. Set alongside `resolving == true`; distinguishes an
    /// honest "server down, still trying" (offer an actionable Refresh) from a
    /// fresh "Resolving…" flash (Refresh disabled). Cleared on the next
    /// successful fetch. `Default` is `false`.
    pub unreachable: bool,
    /// The selected Normal-mode market ("20/20", "20/0", "1/20", "1/0").
    ///
    /// Composed at read time from `AppState.normal_variant`, which stays the
    /// owner — this field is NOT stored in `AppState.ssot`, so there is no
    /// second copy to keep in sync on every `set_normal_variant`. `Default` is
    /// the empty string, which the webview must reject as "unknown".
    pub normal_variant: String,
    /// The selected Dedication corrupted market ("21/23", "21/20").
    ///
    /// Composed at read time from `AppState.dedication_variant`, which stays
    /// the owner — not stored in `AppState.ssot`. `Default` is the empty
    /// string, which the webview must reject as "unknown".
    pub dedication_variant: String,
    /// The selected Dedication rankings pool ("skill", "transfigured").
    ///
    /// Composed at read time from `AppState.dedication_pool`, which stays the
    /// owner — not stored in `AppState.ssot`. `Default` is the empty string,
    /// which the webview must reject as "unknown".
    pub dedication_pool: String,
    /// Per-module enabled flags, projected unchanged from the owner map
    /// `AppState.modules_enabled` (see src/modules.rs). **Intent, not
    /// liveness**: a module that panicked still reports enabled. `Default` is
    /// the empty map, which the webview must read as "not yet known".
    pub modules: std::collections::HashMap<String, bool>,
    /// Merc OCR capture state (POE-165), projected from the owner
    /// `AppState.mercenary` (see src/mercenary/mod.rs). `status` is forced to
    /// `Off` here when the module is disabled — the composer owns that one
    /// precedence step (off > unavailable > live > done > scanning > idle), the
    /// capture loop owns the other five. `Default` is the `Off` slice, which the page
    /// renders as "module off".
    pub mercenary: crate::mercenary::MercenarySlice,
    /// Temple builder state (POE-171), projected from the owner
    /// `AppState.temple` (see src/temple/slice.rs). `status` is forced to `Off`
    /// here when the module is disabled — and the advice is dropped with it, so
    /// a disabled module cannot leave a stale recommendation on screen under an
    /// "off" badge. `Default` is the `Idle` slice with no board.
    pub temple: crate::temple::slice::TempleSlice,
    /// The screen and its measured game-UI scale (POE-214), projected from the
    /// owner `AppState.screen`. `None` until something measures one — fail
    /// closed, and specifically do NOT substitute 1.0: that is a real
    /// measurement (the 1920x1200 reference) and assuming it would silently
    /// mis-scale every rect on a 1080p machine by 11%.
    pub screen: Option<ScreenSlice>,
    // future slices (e.g. account, config) added here as later tasks land.
}

/// Return `base` with the three market fields, the module map and the
/// mercenary slice replaced by the given values.
///
/// The enabled-guide set (`merc_sources_off`, POE-199) is composed onto the
/// mercenary slice here rather than stored in it — the owner is
/// `AppState.merc_sources_off`, exactly as `normal_variant` owns its market.
///
/// The mercenary slice's `status` is forced to `Off` when the module is
/// disabled. This is the single place module enablement reaches that slice, so
/// the page never has to read `ssot.modules` to know whether the toggle is on
/// (ADR-014: the page reads slices, not module state).
///
/// Pure so the composition is unit-testable without an `AppHandle` or a full
/// `AppState` — same reason `should_flag_unreachable` and
/// `clear_resolution_flags` are extracted.
fn compose_snapshot(
    base: AppSsotSnapshot,
    normal_variant: String,
    dedication_variant: String,
    dedication_pool: String,
    modules: std::collections::HashMap<String, bool>,
    mut mercenary: crate::mercenary::MercenarySlice,
    merc_sources_off: Vec<String>,
    merc_sync: crate::mercenary::sync::MercSyncStatus,
    merc_trade_auto: bool,
    merc_tier_floor: u8,
    mut temple: crate::temple::slice::TempleSlice,
    screen: Option<ScreenSlice>,
) -> AppSsotSnapshot {
    if modules.get(MERCENARY_MODULE_ID) != Some(&true) {
        mercenary.status = crate::mercenary::MercStatus::Off;
        // The speaker belongs to `scanning` and to nothing else: it is what the
        // strip prints beside "scanning for the recruit window", so leaving it
        // on a forced-`off` slice would hand the windows a name attached to a
        // scan that is not running. The loop clears it on every other exit from
        // `scanning`; the force-off is the one path the loop never gets to run.
        mercenary.burst_speaker = None;
        // The trade state's own force-off, for the reason the status has one:
        // `off` tells the page the module is not running (badge only) — a
        // module that is not running is not going to search anything. Only the
        // STATUS is forced — the link and the listings stay, so switching the
        // module back on does not lose an answer that is still young enough to
        // be true.
        mercenary.trade.status = crate::mercenary::MercTradeStatus::Off;
    }
    // The settings echo, written AFTER the force-off for the reason the temple
    // slice keeps its own echo through `force_off`: what the user set is not
    // something the module read, and the page renders its guide toggles from
    // it while the module is switched off.
    mercenary.sources_off = merc_sources_off;
    // The pool's status, composed for the same reason and after the same
    // force-off: the pull and the uploader are tasks, not the capture loop, so
    // the slice keeps one writer and this is where their state joins it. Kept
    // when the module is off so the page can still say what the last pull did.
    mercenary.sync = merc_sync;
    // Two more settings echoes, composed after the force-off like the guide set
    // and for the same reason: the page renders the auto toggle and the tier
    // select from them while the module is switched off and no loop will ever
    // publish them.
    mercenary.trade_auto = merc_trade_auto;
    mercenary.tier_floor = merc_tier_floor;
    if modules.get(TEMPLE_MODULE_ID) != Some(&true) {
        crate::temple::slice::force_off(&mut temple);
    }
    AppSsotSnapshot {
        normal_variant,
        dedication_variant,
        dedication_pool,
        modules,
        mercenary,
        temple,
        screen,
        ..base
    }
}

/// The module id the mercenary slice belongs to (see `modules.rs::MODULES`).
const MERCENARY_MODULE_ID: &str = "mercenary";

/// The module id the temple slice belongs to (see `modules.rs::MODULES`).
const TEMPLE_MODULE_ID: &str = "temple";

/// Build the full snapshot: the stored `AppState.ssot` slice plus the market
/// fields and the module map, composed from their owning `AppState` Mutexes.
///
/// The `ssot` guard is dropped before the market Mutexes are locked (never two
/// guards at once), matching the lock-then-emit discipline documented on
/// `emit_ssot`. Both `get_ssot` and `emit_ssot` go through here so the two
/// paths cannot compose different snapshots.
pub fn build_snapshot(state: &AppState) -> AppSsotSnapshot {
    let base = {
        let guard = state.ssot.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    };
    let normal_variant = state.normal_variant.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let dedication_variant = state.dedication_variant.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let dedication_pool = state.dedication_pool.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // Lone acquisition of `modules_enabled` — `module_handles` is not held here
    // (lock order, see src/modules.rs).
    let modules = state.modules_enabled.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // Same lock-then-drop discipline: the guard ends with this statement, so
    // the merc mutex is never held while another is taken or while emitting.
    let mercenary = state.mercenary.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // Its own lone acquisition, like every other owner read here.
    let merc_sources_off = state
        .merc_sources_off
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    // Its own lone acquisition too — `sync::status` takes and drops the
    // `merc_sync` guard inside one statement.
    let merc_sync = crate::mercenary::sync::status(state);
    // Lone acquisitions again, each ending with its own statement.
    let merc_trade_auto = *state.merc_trade_auto.lock().unwrap_or_else(|e| e.into_inner());
    let merc_tier_floor = *state.merc_tier_floor.lock().unwrap_or_else(|e| e.into_inner());
    // Same discipline again: the temple guard ends with this statement, so it
    // is never held alongside the merc one or across the compose.
    let temple = state.temple.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // One more lone acquisition, ending with its own statement — which is what
    // lets `publish_screen` re-enter here through `emit_ssot` after dropping
    // its own guard. `ScreenSlice` is `Copy`, so the deref clones it.
    let screen = *state.screen.lock().unwrap_or_else(|e| e.into_inner());
    compose_snapshot(
        base,
        normal_variant,
        dedication_variant,
        dedication_pool,
        modules,
        mercenary,
        merc_sources_off,
        merc_sync,
        merc_trade_auto,
        merc_tier_floor,
        temple,
        screen,
    )
}

/// Whether `consecutive_failures` failed fetch attempts should flip the SSOT to
/// `unreachable`. Pure so the threshold boundary is directly unit-testable.
fn should_flag_unreachable(consecutive_failures: u32) -> bool {
    consecutive_failures >= LEAGUE_UNREACHABLE_AFTER_ATTEMPTS
}

/// Clear both in-flight flags — the resolver's sole success-exit mutation.
/// Extracted (taking just the mutex) so the "success drops BOTH `resolving` and
/// `unreachable`" contract is unit-testable without a Tauri `AppHandle`.
fn clear_resolution_flags(ssot: &std::sync::Mutex<AppSsotSnapshot>) {
    let mut guard = ssot.lock().unwrap_or_else(|e| e.into_inner());
    guard.resolving = false;
    guard.unreachable = false;
}

/// How far [`ScreenSlice::ui_scale`] must move before it counts as a different
/// screen scale.
///
/// The published number is the merc session's SETTLED scale, and settled does
/// not mean constant: `mercenary::run::next_fitted_scale`'s same-`cell_px` path
/// returns `..fresh`, so every grid-fitting tick takes the fresh measurement's
/// scale while holding the cell size. One `cellfit::P_STEP` of the pitch grid
/// moves that number by **0.0031** — an exact `!=` therefore republishes on a
/// panel that has not moved a pixel.
///
/// 0.01 sits between the two numbers that bound it: above the 0.0031 wobble, and
/// well below the 1/40 = 0.025 a whole px of cell is worth. So the band swallows
/// the measurement noise and still catches every scale change a consumer could
/// cut a different rect from.
const UI_SCALE_EPS: f32 = 0.01;

/// Whether `next` is worth emitting over what is already published.
///
/// The three MEASURED fields and the label: anything a consumer scales a rect
/// with, or judges the exactness of its rects by. `measured_at_ms` alone is
/// deliberately NOT one of them — the merc detect tick re-measures the same
/// screen every few seconds for as long as a recruit window is open, and a
/// timestamp-only diff would emit `ssot-changed` and spin every overlay's poll
/// on a value none of them can act on.
///
/// `ui_scale` gets the same treatment for the same reason, one step weaker: it
/// is a MEASUREMENT and it wobbles by 0.0031 between ticks of a panel that has
/// not moved (see [`UI_SCALE_EPS`]), so it is compared to within that band
/// rather than exactly. Width, height and the label are discrete and are
/// compared exactly.
///
/// `None` — nothing published yet — is always a change.
///
/// Pure so the gate is unit-testable without an `AppHandle`, the same reason
/// `should_flag_unreachable` and `clear_resolution_flags` are extracted.
fn screen_changed(current: Option<&ScreenSlice>, next: &ScreenSlice) -> bool {
    match current {
        None => true,
        Some(current) => {
            current.width != next.width
                || current.height != next.height
                || (current.ui_scale - next.ui_scale).abs() >= UI_SCALE_EPS
                || current.source != next.source
        }
    }
}

/// Store `next` in the screen slot and report whether it is worth emitting.
///
/// **Store always, gate only the emit.** The slot is overwritten whatever the
/// gate says, so `measured_at_ms` tracks the LAST measurement — WI-B2 persists
/// this slot and an age belonging to an older tick would be a lie, and a
/// consumer asking "how old is this scale?" would get the wrong answer on every
/// tick the gate refused. The bool is only about waking the overlays.
///
/// Split out of [`publish_screen`] with no `AppHandle` precisely so that rule is
/// pinned by a test rather than by the prose above it.
pub(crate) fn record_screen(current: &mut Option<ScreenSlice>, next: ScreenSlice) -> bool {
    let changed = screen_changed(current.as_ref(), &next);
    *current = Some(next);
    changed
}

/// Publish a screen measurement into the SSOT, emitting only on a real change.
///
/// Lock-then-drop-then-emit, the shape `mercenary::run::publish` and
/// `temple::run::remember_calibration` both use: the `screen` guard is scoped
/// to the block below, so it is dropped before `emit_ssot` — which re-takes it
/// inside `build_snapshot` — and no other mutex is held while it is open.
///
/// What to store and what to emit is [`record_screen`]'s decision (and
/// [`screen_changed`]'s underneath it); both are `AppHandle`-free and tested.
/// All that is left here is the lock-drop-emit sequence the doc above states,
/// which is what makes calling this on every detect tick the right thing to do.
pub fn publish_screen(app: &AppHandle, next: ScreenSlice) {
    let changed = {
        let state = app.state::<AppState>();
        let mut current = state.screen.lock().unwrap_or_else(|e| e.into_inner());
        record_screen(&mut current, next)
    };
    if changed {
        emit_ssot(app);
    }
}

/// The [`ScreenScaleSource`] label a merc-side [`crate::mercenary::ScaleSource`]
/// publishes under.
///
/// `Held` maps to [`ScreenScaleSource::MercFrame`], not to a label of its own:
/// `Held` is a FRAME measurement carried onto a tick that could not see the
/// frame (`cellfit::apply_held` is what carries it), so the cue behind the
/// number is the gold frame either way and a reader asking "is this registered
/// on the art?" gets yes for both. Only a session whose fit has never landed —
/// [`crate::mercenary::ScaleSource::Ocr`] — reports the line-pitch estimate that
/// drifts 6-12 px.
///
/// Lives here, next to the enum it produces, so the mapping is one testable
/// decision instead of a `match` buried in the detect tick's struct literal.
pub fn screen_scale_source(s: crate::mercenary::ScaleSource) -> ScreenScaleSource {
    match s {
        crate::mercenary::ScaleSource::Frame | crate::mercenary::ScaleSource::Held => {
            ScreenScaleSource::MercFrame
        }
        crate::mercenary::ScaleSource::Ocr => ScreenScaleSource::MercOcr,
    }
}

/// Emit `ssot-changed` with the current snapshot.
///
/// Optional eager nudge for the main window; overlay windows poll `get_ssot`
/// instead. Lock-then-emit discipline: the `ssot` guard is scoped to a block
/// that ends **before** `app.emit(...)`, so the mutex is never held across the
/// emit call (mirrors `emit_logs` and the lab_state pattern in lib.rs).
///
/// Called by `apply_league` after every SSOT mutation.
pub fn emit_ssot(app: &AppHandle) {
    let snapshot = {
        let state = app.state::<AppState>();
        build_snapshot(&state)
    };
    if let Err(e) = app.emit("ssot-changed", snapshot) {
        log::warn!("emit ssot-changed failed: {}", e);
    }
}

/// Poll target for overlay windows: compose the snapshot from the stored slice
/// plus the market fields, and return it. Serialized to the webview.
#[tauri::command]
pub fn get_ssot(state: tauri::State<AppState>) -> AppSsotSnapshot {
    build_snapshot(&state)
}

/// Dual-write the resolved league into both sources of truth.
///
/// Writes the SSOT `league.name` slice, then the trade client, each under its
/// own scoped lock — no guard is held across the other write, and neither is
/// held across an `.await` (this fn is synchronous). This is the POE-126 seam:
/// auto-fetch and any future user-choice path both funnel through here so the
/// two stores can never diverge.
fn write_league(
    ssot: &std::sync::Mutex<AppSsotSnapshot>,
    trade_client: &TradeApiClient,
    name: String,
) {
    {
        let mut guard = ssot.lock().unwrap_or_else(|e| e.into_inner());
        guard.league.name = Some(name.clone());
        // Resolve complete: clear the in-flight flag in the SAME locked mutation
        // that writes the league, so a poller can never observe league-set with
        // `resolving` still true. This is the sole clear path for the flag.
        guard.resolving = false;
    }
    trade_client.set_league(name);
}

/// Single-flight gate for the league resolver. Returns `true` (and marks the
/// SSOT `resolving`) only when no resolve is in flight; returns `false` and
/// changes nothing when one already is. This is what stops repeated
/// `refresh_league` clicks against a down server from stacking N concurrent
/// infinite-retry loops. The lock is scoped to this fn and never held across an
/// `.await` (the fn is synchronous). Extracted for direct unit testing.
fn try_begin_resolving(ssot: &std::sync::Mutex<AppSsotSnapshot>) -> bool {
    let mut guard = ssot.lock().unwrap_or_else(|e| e.into_inner());
    if guard.resolving {
        return false;
    }
    guard.resolving = true;
    true
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

/// Dual-write via the app state, then emit the `ssot-changed` nudge.
///
/// Blank/whitespace names are rejected here (see `normalize_league`) so
/// fail-closed cannot be defeated at the seam: nothing changed, so neither store
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
    {
        let state = app.state::<AppState>();
        write_league(&state.ssot, &state.trade_client, name);
    }
    emit_ssot(app);
}

/// Extract the league name from a `/api/analysis/status` JSON body.
///
/// Returns `None` when the `league` field is absent, non-string, or blank so
/// the caller **fails closed** (the server omits `league` entirely when its
/// cache is not yet populated — see `AnalysisStatus`). A `None` here leaves the
/// SSOT unresolved and the trade client failing every lookup, by design.
fn parse_league_from_status(body: &serde_json::Value) -> Option<String> {
    body.get("league")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// One league fetch attempt. Returns `Some(league)` on a clean success, `None`
/// on any failure (offline, non-2xx, bad JSON, missing field) so the caller can
/// retry. The server-URL guard and the cloned HTTP client are scoped before the
/// first `.await`, so no lock is held across the network round-trip.
async fn fetch_league_once(app: &AppHandle) -> Option<String> {
    let (server_url, http) = {
        let state = app.state::<AppState>();
        let url = state
            .server_url
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        (url, state.server_http.clone())
    };
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

/// Spawn the start-only league resolver: fetch `/api/analysis/status`, and on
/// success dual-write the league and stop. On failure it retries with bounded
/// exponential backoff — offline-at-launch is not fatal; the SSOT simply stays
/// unresolved (fail-closed) until the server answers. There is **no** poller:
/// once resolved the task returns. `refresh_league` re-arms it on demand.
pub fn spawn_league_fetch(app: AppHandle) {
    // Single-flight: claim the resolve right before spawning. If a resolve is
    // already in flight, do NOT spawn a duplicate loop — the live one owns the
    // retry window and its `resolving` flag already tells the UI we're working.
    {
        let state = app.state::<AppState>();
        if !try_begin_resolving(&state.ssot) {
            return;
        }
    }
    // Publish `resolving = true` to pollers/main window before the first attempt.
    emit_ssot(&app);
    tauri::async_runtime::spawn(async move {
        let mut backoff = LEAGUE_FETCH_INITIAL_BACKOFF;
        let mut failures: u32 = 0;
        loop {
            if let Some(league) = fetch_league_once(&app).await {
                log::info!("league resolved: {}", league);
                apply_league(&app, league);
                // Sole success exit: clear BOTH flags here, decoupled from
                // `apply_league`. `write_league` also clears `resolving`, but only
                // downstream of `normalize_league`'s blank re-check — if that gate
                // ever drifted from the fetch's own blank-gate it could reject an
                // already-accepted value and leave the loop wedged. This
                // unconditional clear cannot be gated by that re-validation, and
                // also drops `unreachable` (which `write_league` never touches).
                {
                    let state = app.state::<AppState>();
                    clear_resolution_flags(&state.ssot);
                }
                emit_ssot(&app);
                return;
            }
            failures += 1;
            if should_flag_unreachable(failures) {
                // Server looks down. Flip `unreachable` (still retrying) so the UI
                // stops showing a perpetual "Resolving…" and offers an actionable
                // Refresh. Emit only on the transition to avoid nudge spam.
                let newly_unreachable = {
                    let state = app.state::<AppState>();
                    let mut guard = state.ssot.lock().unwrap_or_else(|e| e.into_inner());
                    let changed = !guard.unreachable;
                    guard.unreachable = true;
                    changed
                };
                if newly_unreachable {
                    emit_ssot(&app);
                }
            }
            // Wait out the backoff, but let a `refresh_league` wake short-circuit
            // it: a manual Refresh retries immediately and resets the backoff.
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {
                    backoff = (backoff * 2).min(LEAGUE_FETCH_MAX_BACKOFF);
                }
                _ = RETRY_NOTIFY.notified() => {
                    backoff = LEAGUE_FETCH_INITIAL_BACKOFF;
                }
            }
        }
    });
}

/// Manually set the resolved league (dual-write + emit). The POE-126 seam for a
/// future user league-choice UI; also the sink the fetch task writes through.
#[tauri::command]
pub fn set_league(name: String, app: AppHandle) {
    // POE-126 latent hazard: the start-only resolver loop is not cancelled here.
    // If a manual set lands while a resolve is still in flight, that loop lives on
    // and will overwrite this manual choice when the server recovers. The real fix
    // (abort the JoinHandle / generation-guard the write) belongs with POE-126 —
    // the user-choice UI that actually arms this path; there is no manual-set UI
    // today. For now, make the collision visible if it is ever hit.
    {
        let state = app.state::<AppState>();
        let in_flight = state.ssot.lock().unwrap_or_else(|e| e.into_inner()).resolving;
        if in_flight {
            log::warn!(
                "set_league({:?}) landing over an active resolver; leaving an orphan \
                 retry loop that may clobber this manual set on server recovery (POE-126)",
                name
            );
        }
    }
    apply_league(&app, name);
}

/// Manually re-arm the start-only league fetch (e.g. a Settings button after a
/// server rollover). Fire-and-forget: it spawns the same bounded-retry task.
/// There is deliberately no background poller — nothing to poll until a server
/// rollover signal exists.
#[tauri::command]
pub fn refresh_league(app: AppHandle) {
    // If a resolve is already looping (e.g. "Server unreachable"), a fresh spawn
    // would be refused by the single-flight guard anyway — so instead WAKE the
    // live loop: it retries immediately and resets its backoff. Only when nothing
    // is in flight do we spawn a new resolver.
    let in_flight = {
        let state = app.state::<AppState>();
        let resolving = state.ssot.lock().unwrap_or_else(|e| e.into_inner()).resolving;
        resolving
    };
    if in_flight {
        RETRY_NOTIFY.notify_one();
    } else {
        spawn_league_fetch(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Locks the fail-closed default: a fresh snapshot reports the league as
    /// not-yet-fetched (`None`), never a spuriously "valid" empty name.
    #[test]
    fn default_snapshot_league_is_unfetched() {
        let snap = AppSsotSnapshot::default();
        assert_eq!(snap.league.name, None);
    }

    /// The dual-write contract — the whole point of the seam. One `write_league`
    /// call must land the league in BOTH the SSOT snapshot and the trade client,
    /// asserted on the real post-state of each (not a status flag).
    #[test]
    fn write_league_updates_both_ssot_and_trade_client() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        let trade_client = TradeApiClient::new();
        // Precondition: both unresolved (fail-closed).
        assert_eq!(ssot.lock().unwrap().league.name, None);
        assert!(trade_client.league().is_err());

        write_league(&ssot, &trade_client, "Mirage".to_string());

        assert_eq!(
            ssot.lock().unwrap().league.name,
            Some("Mirage".to_string()),
            "SSOT slice must hold the resolved league",
        );
        assert_eq!(
            trade_client.league().unwrap(),
            "Mirage",
            "trade client must now resolve to the same league",
        );
    }

    /// Single-flight contract: the first caller acquires the resolve right and
    /// marks `resolving`; a second caller is refused while a resolve is in
    /// flight, so repeated `refresh_league` clicks can't stack duplicate loops.
    #[test]
    fn try_begin_resolving_refuses_second_caller_while_resolving() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        assert!(!ssot.lock().unwrap().resolving, "precondition: not resolving");

        assert!(
            try_begin_resolving(&ssot),
            "first caller must acquire the resolve right",
        );
        assert!(
            ssot.lock().unwrap().resolving,
            "flag must be set after the right is acquired",
        );
        assert!(
            !try_begin_resolving(&ssot),
            "second caller must be refused while a resolve is in flight",
        );
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
        let trade_client = TradeApiClient::new();
        // Precondition: resolve in flight, league not yet written.
        assert!(ssot.lock().unwrap().resolving);
        assert_eq!(ssot.lock().unwrap().league.name, None);

        write_league(&ssot, &trade_client, "Mirage".to_string());

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
    /// returns `None`, so neither store is written and both stay fail-closed.
    /// Guards the `set_league`/`refresh_league` user-choice sink that skips the
    /// server-side `parse_league_from_status` filter.
    #[test]
    fn blank_league_at_seam_leaves_both_stores_unresolved() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        let trade_client = TradeApiClient::new();
        // Precondition: both unresolved (fail-closed) before the guarded path.
        assert_eq!(ssot.lock().unwrap().league.name, None);
        assert!(trade_client.league().is_err());

        // The guard `apply_league` applies before writing. Whitespace-only.
        match normalize_league("   ") {
            None => {} // rejected — no write, exactly what `apply_league` does.
            Some(n) => write_league(&ssot, &trade_client, n),
        }

        assert_eq!(
            ssot.lock().unwrap().league.name,
            None,
            "SSOT slice must stay unresolved after a blank set_league",
        );
        assert!(
            trade_client.league().is_err(),
            "trade client must stay unresolved after a blank set_league",
        );
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
    /// server exits through), `clear_resolution_flags` must leave `resolving` and
    /// `unreachable` both false — asserted on real post-state, not a return code.
    #[test]
    fn clear_resolution_flags_drops_both_flags() {
        let ssot = Mutex::new(AppSsotSnapshot {
            resolving: true,
            unreachable: true,
            ..Default::default()
        });

        clear_resolution_flags(&ssot);

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

    /// The composition contract: every one of the three market values lands in
    /// the returned snapshot, each in its own field. Distinct values so a
    /// crossed assignment (pool into variant, etc.) fails instead of passing.
    #[test]
    fn compose_snapshot_writes_all_three_market_values() {
        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "1/20".to_string(),
            "21/20".to_string(),
            "transfigured".to_string(),
            std::collections::HashMap::new(),
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.normal_variant, "1/20");
        assert_eq!(out.dedication_variant, "21/20");
        assert_eq!(out.dedication_pool, "transfigured");
    }

    /// The module map is projected into the snapshot unchanged. Without this
    /// the field falls back to `..base` (always the empty default), and every
    /// window would poll an empty `modules` slice forever.
    #[test]
    fn compose_snapshot_projects_the_module_map() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), true)].into_iter().collect();

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.modules.get("mercenary"), Some(&true));
    }

    /// The mercenary slice is projected through composition while the module
    /// is enabled: the loop owns `live`/`idle`/`unavailable`, and the composer
    /// must not overwrite them. Without the projection the field falls back to
    /// `..base` (always the `Off` default) and the page would poll a
    /// permanently-off slice while a capture was on screen.
    #[test]
    fn compose_snapshot_projects_the_mercenary_slice_while_the_module_is_on() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), true)].into_iter().collect();
        let slice = crate::mercenary::MercenarySlice {
            status: crate::mercenary::MercStatus::Live,
            learned_families: vec!["Chain--2".to_string()],
            geometry_source: "file".to_string(),
            ..Default::default()
        };

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            slice,
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.mercenary.status, crate::mercenary::MercStatus::Live);
        assert_eq!(out.mercenary.learned_families, vec!["Chain--2".to_string()]);
        assert_eq!(out.mercenary.geometry_source, "file");
    }

    /// `off` beats every other status (precedence off > unavailable > live >
    /// done > scanning > idle). A module switched off mid-capture must publish
    /// `off`, or the page keeps rendering a live verdict for a loop that has
    /// stopped — and the burst speaker goes with it, because a name beside a
    /// scan that is not running is a claim about a loop that no longer exists.
    #[test]
    fn a_disabled_module_forces_the_mercenary_status_to_off() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), false)].into_iter().collect();
        let slice = crate::mercenary::MercenarySlice {
            status: crate::mercenary::MercStatus::Scanning,
            burst_speaker: Some("Fennik, of Unshakeable Faith".to_string()),
            capture: Some(crate::mercenary::MercCapture::default()),
            ..Default::default()
        };

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            slice,
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.mercenary.status, crate::mercenary::MercStatus::Off);
        assert_eq!(
            out.mercenary.burst_speaker, None,
            "the speaker belongs to a scan that is no longer running",
        );
        assert!(
            out.mercenary.capture.is_some(),
            "only the status is forced — the last capture stays readable",
        );
    }

    /// The enabled-guide set is what the page and the overlay both evaluate
    /// against (POE-199 L5), so it must reach the snapshot from the OWNER —
    /// not from the stored slice, which nothing ever writes it into.
    #[test]
    fn compose_snapshot_echoes_the_enabled_guide_set_onto_the_mercenary_slice() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), true)].into_iter().collect();

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            vec!["guide-a".to_string()],
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.mercenary.sources_off, vec!["guide-a".to_string()]);
    }

    /// The echo survives the module being switched off, for the reason the
    /// temple's `keys`/`config` echo does: it is what the USER set, and the
    /// page renders its guide toggles from it while the module is off
    /// (ADR-014 — the page reads the slice, never `ssot.modules`).
    #[test]
    fn a_disabled_module_keeps_the_enabled_guide_set() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), false)].into_iter().collect();

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            vec!["guide-b".to_string()],
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.mercenary.status, crate::mercenary::MercStatus::Off);
        assert_eq!(out.mercenary.sources_off, vec!["guide-b".to_string()]);
    }

    /// A module map that does not mention the module yet (nothing has written
    /// the owner map) reads as OFF, not as on. Fail-closed: claiming a module
    /// is running when its enablement is unknown is the worse lie.
    #[test]
    fn an_unknown_module_flag_forces_the_mercenary_status_to_off() {
        let slice = crate::mercenary::MercenarySlice {
            status: crate::mercenary::MercStatus::Idle,
            ..Default::default()
        };

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            std::collections::HashMap::new(),
            slice,
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.mercenary.status, crate::mercenary::MercStatus::Off);
    }

    /// The temple slice is projected through composition while the module is
    /// enabled. Without the projection the field falls back to `..base` (the
    /// empty default) and every overlay would poll a boardless slice while a
    /// board was on screen.
    #[test]
    fn compose_snapshot_projects_the_temple_slice_while_the_module_is_on() {
        let modules: std::collections::HashMap<String, bool> =
            [("temple".to_string(), true)].into_iter().collect();
        let slice = crate::temple::slice::TempleSlice {
            status: crate::temple::slice::TempleStatus::Read,
            keys: 2,
            unknown_rooms: vec!["A0".to_string()],
            mode: Some("chase".to_string()),
            ..Default::default()
        };

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            slice,
            None,
        );

        assert_eq!(out.temple.status, crate::temple::slice::TempleStatus::Read);
        assert_eq!(out.temple.keys, 2);
        assert_eq!(out.temple.unknown_rooms, vec!["A0".to_string()]);
        assert_eq!(out.temple.mode, Some("chase".to_string()));
    }

    /// A disabled temple module publishes `off` AND drops its advice, so the
    /// page cannot render a stale recommendation under an off badge. The
    /// layout is left readable — same split as the merc slice's capture.
    #[test]
    fn a_disabled_module_forces_the_temple_status_off_and_drops_its_advice() {
        let modules: std::collections::HashMap<String, bool> =
            [("temple".to_string(), false)].into_iter().collect();
        let slice = crate::temple::slice::TempleSlice {
            status: crate::temple::slice::TempleStatus::Read,
            advice: Some(crate::temple::slice::AdviceView {
                recommendations: Vec::new(),
                gambles: Vec::new(),
                map_action: "leaveMap".to_string(),
                warnings: Vec::new(),
            }),
            mode: Some("chase".to_string()),
            unknown_rooms: vec!["A0".to_string()],
            ..Default::default()
        };

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            slice,
            None,
        );

        assert_eq!(out.temple.status, crate::temple::slice::TempleStatus::Off);
        assert_eq!(out.temple.advice, None);
        assert_eq!(out.temple.mode, None);
        assert_eq!(
            out.temple.unknown_rooms,
            vec!["A0".to_string()],
            "only the acting half is forced — the read stays readable",
        );
    }

    /// A module map that does not mention the temple module yet reads as OFF,
    /// not as on. Fail-closed, same as the merc slice.
    #[test]
    fn an_unknown_module_flag_forces_the_temple_status_to_off() {
        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            std::collections::HashMap::new(),
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice {
                status: crate::temple::slice::TempleStatus::Read,
                ..Default::default()
            },
            None,
        );

        assert_eq!(out.temple.status, crate::temple::slice::TempleStatus::Off);
    }

    /// The snapshot's own key for the temple slice, which the TS store reads.
    /// Dropping `rename_all` on either struct silently breaks the store.
    #[test]
    fn the_snapshot_exposes_the_temple_slice_under_its_camel_case_keys() {
        let json = serde_json::to_value(AppSsotSnapshot::default()).unwrap();

        assert_eq!(json["temple"]["status"], "idle");
        assert_eq!(json["temple"]["layout"], serde_json::Value::Null);
        assert_eq!(json["temple"]["unknownRooms"], serde_json::json!([]));
        assert_eq!(json["temple"]["lastReadAt"], serde_json::Value::Null);
    }

    /// The snapshot's own key for the slice, which the TS store reads.
    #[test]
    fn the_snapshot_exposes_the_mercenary_slice_under_its_camel_case_key() {
        let json = serde_json::to_value(AppSsotSnapshot::default()).unwrap();

        assert_eq!(json["mercenary"]["status"], "off");
        assert_eq!(json["mercenary"]["geometrySource"], "default");
        assert_eq!(json["mercenary"]["capture"], serde_json::Value::Null);
    }

    /// Composition must not disturb the league slice it wraps: the stored
    /// `AppState.ssot` fields survive the market overlay untouched, so a poll
    /// never trades a resolved league for a market selection.
    #[test]
    fn compose_snapshot_preserves_the_stored_league_slice() {
        let base = AppSsotSnapshot {
            league: LeagueSlice { name: Some("Mirage".to_string()) },
            resolving: true,
            unreachable: true,
            ..Default::default()
        };

        let out = compose_snapshot(
            base,
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            std::collections::HashMap::new(),
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.league.name, Some("Mirage".to_string()));
        assert!(out.resolving, "resolving must survive composition");
        assert!(out.unreachable, "unreachable must survive composition");
    }

    /// Wire contract: the TypeScript store reads camelCase keys, and the Rust
    /// field names are snake_case — the `rename_all` attribute is what bridges
    /// them. Dropping it renames these keys and silently breaks the store.
    #[test]
    fn snapshot_serializes_market_fields_as_camel_case() {
        let snap = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/0".to_string(),
            "21/20".to_string(),
            "transfigured".to_string(),
            std::collections::HashMap::new(),
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        let json = serde_json::to_value(&snap).unwrap();

        assert_eq!(json.get("normalVariant").and_then(|v| v.as_str()), Some("20/0"));
        assert_eq!(json.get("dedicationVariant").and_then(|v| v.as_str()), Some("21/20"));
        assert_eq!(json.get("dedicationPool").and_then(|v| v.as_str()), Some("transfigured"));
    }

    /// The same `rename_all` must leave the pre-existing keys alone — they are
    /// single words, so camelCase is a no-op on them. The store already reads
    /// `league` / `resolving` / `unreachable` by these exact names.
    #[test]
    fn snapshot_serialization_keeps_the_existing_league_keys() {
        let snap = AppSsotSnapshot {
            league: LeagueSlice { name: Some("Mirage".to_string()) },
            resolving: true,
            unreachable: true,
            ..Default::default()
        };

        let json = serde_json::to_value(&snap).unwrap();

        assert_eq!(
            json.get("league").and_then(|v| v.get("name")).and_then(|v| v.as_str()),
            Some("Mirage"),
        );
        assert_eq!(json.get("resolving").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(json.get("unreachable").and_then(|v| v.as_bool()), Some(true));
    }

    /// Fail-closed default: a snapshot that was never composed from `AppState`
    /// carries empty markets, never a plausible-looking "20/20". The webview
    /// treats empty as unknown, so a default can't be mistaken for a real
    /// selection.
    #[test]
    fn default_snapshot_has_no_market_selection() {
        let snap = AppSsotSnapshot::default();

        assert_eq!(snap.normal_variant, "");
        assert_eq!(snap.dedication_variant, "");
        assert_eq!(snap.dedication_pool, "");
    }

    // --------------------------------------------------------- screen slice --

    /// The reference measurement, as a starting point every screen test edits
    /// one field of. 1920x1200 at 1.0 IS the reference fixture (POE-214 D3), so
    /// a test that changes nothing is asking about a real, whole screen.
    fn reference_screen() -> ScreenSlice {
        ScreenSlice {
            width: 1920,
            height: 1200,
            ui_scale: 1.0,
            source: ScreenScaleSource::MercFrame,
            measured_at_ms: 1_700_000_000_000,
        }
    }

    /// Fail-closed default: a snapshot nothing has measured a screen for says
    /// so, rather than carrying the reference 1.0. A consumer that took a
    /// default as a measurement would scale every rect 11% wrong on a 1080p
    /// machine and have no way to know it.
    #[test]
    fn default_snapshot_has_no_screen_measurement() {
        let snap = AppSsotSnapshot::default();

        assert!(snap.screen.is_none(), "a default snapshot has measured nothing");
    }

    /// The owner's value is projected into the snapshot. Without the projection
    /// the field falls back to `..base` (always `None`, since nothing writes it
    /// into `AppState.ssot`) and every window would poll an unmeasured screen
    /// forever while the merc loop was publishing one.
    #[test]
    fn compose_snapshot_projects_the_screen_slice() {
        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            std::collections::HashMap::new(),
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::DEFAULT_TRADE_AUTO,
            crate::mercenary::DEFAULT_TIER_FLOOR,
            crate::temple::slice::TempleSlice::default(),
            Some(ScreenSlice { ui_scale: 0.9, ..reference_screen() }),
        );

        let screen = out.screen.expect("the composed snapshot carries the measurement");
        assert_eq!(screen.ui_scale, 0.9);
        assert_eq!(screen.source, ScreenScaleSource::MercFrame);
    }

    /// The wire contract the TS `ScreenSlice` mirrors: camelCase keys on the
    /// slice, under the snapshot's own `screen` key. Dropping either
    /// `rename_all` renames these and the store reads `undefined` in silence.
    #[test]
    fn the_snapshot_exposes_the_screen_slice_under_its_camel_case_keys() {
        let snap = AppSsotSnapshot { screen: Some(reference_screen()), ..Default::default() };

        let json = serde_json::to_value(&snap).unwrap();

        assert_eq!(json["screen"]["width"], 1920);
        assert_eq!(json["screen"]["height"], 1200);
        assert_eq!(json["screen"]["uiScale"], 1.0);
        assert_eq!(json["screen"]["measuredAtMs"], 1_700_000_000_000u64);
    }

    /// The three source strings, exactly. They are read by a TS union and by
    /// the WI-B2 settings round-trip, so a rename here is a silent break on
    /// both sides — and `kebab-case` is the one `rename_all` that produces
    /// them, so this pins the attribute as much as the variants.
    #[test]
    fn the_screen_scale_sources_serialise_as_their_wire_strings() {
        let wire = |source| {
            let snap = AppSsotSnapshot {
                screen: Some(ScreenSlice { source, ..reference_screen() }),
                ..Default::default()
            };
            serde_json::to_value(&snap).unwrap()["screen"]["source"].clone()
        };

        assert_eq!(wire(ScreenScaleSource::MercFrame), "merc-frame");
        assert_eq!(wire(ScreenScaleSource::MercOcr), "merc-ocr");
        assert_eq!(wire(ScreenScaleSource::Remembered), "remembered");
    }

    /// The first measurement always publishes — there is nothing behind it, so
    /// nobody downstream knows the screen at all yet.
    #[test]
    fn the_first_screen_measurement_is_published() {
        assert!(screen_changed(None, &reference_screen()));
    }

    /// The gate's whole purpose: the detect tick re-measures the same screen
    /// every few seconds for as long as the recruit window is open, and none of
    /// those repeats may wake an overlay poll.
    #[test]
    fn an_identical_screen_measurement_is_not_published() {
        let current = reference_screen();

        assert!(!screen_changed(Some(&current), &reference_screen()));
    }

    /// A new timestamp on an otherwise identical measurement is the repeat case
    /// as it actually arrives — every tick stamps a fresh `measured_at_ms`. If
    /// this field were compared, the gate would pass every single tick and be
    /// no gate at all.
    #[test]
    fn a_fresh_timestamp_alone_is_not_published() {
        let current = reference_screen();
        let next = ScreenSlice { measured_at_ms: current.measured_at_ms + 5_000, ..current };

        assert!(!screen_changed(Some(&current), &next));
    }

    /// The measurement consumers scale rects with. 0.974 to 1.0 is the exact
    /// step POE-214's frame fit makes on the reference machine.
    #[test]
    fn a_changed_ui_scale_is_published() {
        let current = ScreenSlice { ui_scale: 0.974, ..reference_screen() };

        assert!(screen_changed(Some(&current), &reference_screen()));
    }

    /// The wobble [`UI_SCALE_EPS`] exists for: `run::next_fitted_scale`'s
    /// same-`cell_px` path returns `..fresh`, so the published scale takes the
    /// fresh measurement on every grid-fitting tick, and one `cellfit::P_STEP`
    /// of the pitch grid moves it by 0.0031. Compared exactly, that is a second
    /// `ssot-changed` per tick on a panel that has not moved a pixel.
    #[test]
    fn a_ui_scale_wobble_inside_the_band_is_not_published() {
        let current = reference_screen();
        let next = ScreenSlice { ui_scale: current.ui_scale + 0.003, ..current };

        assert!(!screen_changed(Some(&current), &next));
    }

    /// The far side of the same band. 0.03 is past the 1/40 = 0.025 a whole px
    /// of merc cell is worth, so it is a scale a consumer would cut different
    /// rects from — a band wide enough to swallow it would be hiding a real
    /// change of screen.
    #[test]
    fn a_ui_scale_step_past_the_band_is_published() {
        let current = reference_screen();
        let next = ScreenSlice { ui_scale: current.ui_scale + 0.03, ..current };

        assert!(screen_changed(Some(&current), &next));
    }

    /// A resolution change is a different screen, and every fraction-of-the-
    /// screen rect derived from the slice moves with it. Width and height are
    /// asserted apart so a gate that compares only one names which.
    #[test]
    fn a_changed_screen_width_is_published() {
        let current = reference_screen();
        let next = ScreenSlice { width: 2560, ..current };

        assert!(screen_changed(Some(&current), &next));
    }

    /// The half of the resolution the game's UI scale actually follows.
    #[test]
    fn a_changed_screen_height_is_published() {
        let current = reference_screen();
        let next = ScreenSlice { height: 1080, ..current };

        assert!(screen_changed(Some(&current), &next));
    }

    /// The label moving at an unchanged scale is still news: a consumer that
    /// weighs how exact its rects are reads `source`, and a session that fell
    /// back from the frame to the OCR cue has to be able to say so even when
    /// the two agreed on the number.
    #[test]
    fn a_changed_screen_scale_source_is_published() {
        let current = reference_screen();
        let next = ScreenSlice { source: ScreenScaleSource::MercOcr, ..current };

        assert!(screen_changed(Some(&current), &next));
    }

    /// The emit half of "store always, gate only the emit": a repeat of the
    /// measurement already in the slot is not worth waking every overlay's
    /// poll, however fresh its stamp.
    #[test]
    fn record_screen_reports_no_change_for_a_repeat_measurement() {
        let base = reference_screen();
        let mut slot = Some(base);
        let next = ScreenSlice { measured_at_ms: base.measured_at_ms + 5_000, ..base };

        assert!(!record_screen(&mut slot, next));
    }

    /// The store half, and the one WI-B2 depends on: the slot takes the refused
    /// measurement anyway, so `measured_at_ms` is the age of the last
    /// MEASUREMENT. Gated storage would make it the age of the last CHANGE, and
    /// a persisted scale would then claim to be hours older than it is.
    #[test]
    fn a_refused_screen_measurement_is_still_stored() {
        let base = reference_screen();
        let mut slot = Some(base);
        let next = ScreenSlice { measured_at_ms: base.measured_at_ms + 5_000, ..base };

        record_screen(&mut slot, next);

        assert_eq!(
            slot.expect("the slot still holds a measurement").measured_at_ms,
            base.measured_at_ms + 5_000
        );
    }

    /// `Frame` is the plain case: this tick measured the gold frame, so the
    /// slice says the frame measured it.
    #[test]
    fn a_frame_fit_publishes_under_the_frame_label() {
        assert_eq!(
            screen_scale_source(crate::mercenary::ScaleSource::Frame),
            ScreenScaleSource::MercFrame
        );
    }

    /// The arm with the argument behind it. `Held` is a FRAME measurement
    /// carried onto a tick that could not see the frame, so it publishes the
    /// frame label — folding it into `MercOcr` would tell a consumer the number
    /// came from the 6-12 px line-pitch estimate it never came from.
    #[test]
    fn a_held_frame_scale_publishes_under_the_frame_label() {
        assert_eq!(
            screen_scale_source(crate::mercenary::ScaleSource::Held),
            ScreenScaleSource::MercFrame
        );
    }

    /// The one arm that must not report the frame: a session whose fit has
    /// never landed is running on the line-pitch estimate, and the label is the
    /// only way a consumer judging how exact its rects are can tell.
    #[test]
    fn an_ocr_only_scale_publishes_under_the_ocr_label() {
        assert_eq!(
            screen_scale_source(crate::mercenary::ScaleSource::Ocr),
            ScreenScaleSource::MercOcr
        );
    }
}
