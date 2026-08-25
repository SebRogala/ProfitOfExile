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
    /// precedence step (off > unavailable > live > scanning > idle), the
    /// capture loop owns the other four. `Default` is the `Off` slice, which the page
    /// renders as "module off".
    pub mercenary: crate::mercenary::MercenarySlice,
    /// Temple builder state (POE-171), projected from the owner
    /// `AppState.temple` (see src/temple/slice.rs). `status` is forced to `Off`
    /// here when the module is disabled — and the advice is dropped with it, so
    /// a disabled module cannot leave a stale recommendation on screen under an
    /// "off" badge. `Default` is the `Idle` slice with no board.
    pub temple: crate::temple::slice::TempleSlice,
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
    mut temple: crate::temple::slice::TempleSlice,
) -> AppSsotSnapshot {
    if modules.get(MERCENARY_MODULE_ID) != Some(&true) {
        mercenary.status = crate::mercenary::MercStatus::Off;
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
    // Same discipline again: the temple guard ends with this statement, so it
    // is never held alongside the merc one or across the compose.
    let temple = state.temple.lock().unwrap_or_else(|e| e.into_inner()).clone();
    compose_snapshot(
        base,
        normal_variant,
        dedication_variant,
        dedication_pool,
        modules,
        mercenary,
        merc_sources_off,
        merc_sync,
        temple,
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
            crate::temple::slice::TempleSlice::default(),
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
            crate::temple::slice::TempleSlice::default(),
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
            crate::temple::slice::TempleSlice::default(),
        );

        assert_eq!(out.mercenary.status, crate::mercenary::MercStatus::Live);
        assert_eq!(out.mercenary.learned_families, vec!["Chain--2".to_string()]);
        assert_eq!(out.mercenary.geometry_source, "file");
    }

    /// `off` beats every other status (D6 precedence off > unavailable > live
    /// > idle). A module switched off mid-capture must publish `off`, or the
    /// page keeps rendering a live verdict for a loop that has stopped.
    #[test]
    fn a_disabled_module_forces_the_mercenary_status_to_off() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), false)].into_iter().collect();
        let slice = crate::mercenary::MercenarySlice {
            status: crate::mercenary::MercStatus::Live,
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
            crate::temple::slice::TempleSlice::default(),
        );

        assert_eq!(out.mercenary.status, crate::mercenary::MercStatus::Off);
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
            crate::temple::slice::TempleSlice::default(),
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
            crate::temple::slice::TempleSlice::default(),
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
            crate::temple::slice::TempleSlice::default(),
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
            slice,
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
            slice,
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
            crate::temple::slice::TempleSlice {
                status: crate::temple::slice::TempleStatus::Read,
                ..Default::default()
            },
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
            crate::temple::slice::TempleSlice::default(),
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
            crate::temple::slice::TempleSlice::default(),
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
}
