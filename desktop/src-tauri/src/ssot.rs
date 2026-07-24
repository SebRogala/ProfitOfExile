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

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::trade::TradeApiClient;
use crate::AppState;

/// First retry delay for the startup league fetch.
const LEAGUE_FETCH_INITIAL_BACKOFF: Duration = Duration::from_secs(2);
/// Ceiling for the exponential backoff — offline-at-launch keeps retrying at
/// most this often, never faster, until the server answers.
const LEAGUE_FETCH_MAX_BACKOFF: Duration = Duration::from_secs(60);

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
pub struct AppSsotSnapshot {
    pub league: LeagueSlice,
    /// A league resolve is currently in flight (the bounded-retry fetch task is
    /// running). Exposed in the polled snapshot so the Settings UI can show an
    /// honest "Resolving…" for the whole retry window instead of a one-frame
    /// flash. Doubles as the single-flight guard (see `try_begin_resolving`):
    /// while `true`, a second `refresh_league` no-ops rather than stacking a
    /// duplicate infinite-retry loop. `Default` is `false` (not resolving).
    pub resolving: bool,
    // future slices (e.g. account, config) added here as later tasks land.
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
        let guard = state.ssot.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    };
    if let Err(e) = app.emit("ssot-changed", snapshot) {
        log::warn!("emit ssot-changed failed: {}", e);
    }
}

/// Poll target for overlay windows: lock, clone the snapshot, drop the guard,
/// return the clone. Serialized to the webview.
#[tauri::command]
pub fn get_ssot(state: tauri::State<AppState>) -> AppSsotSnapshot {
    state.ssot.lock().unwrap_or_else(|e| e.into_inner()).clone()
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
        loop {
            if let Some(league) = fetch_league_once(&app).await {
                log::info!("league resolved: {}", league);
                apply_league(&app, league);
                return;
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(LEAGUE_FETCH_MAX_BACKOFF);
        }
    });
}

/// Manually set the resolved league (dual-write + emit). The POE-126 seam for a
/// future user league-choice UI; also the sink the fetch task writes through.
#[tauri::command]
pub fn set_league(name: String, app: AppHandle) {
    apply_league(&app, name);
}

/// Manually re-arm the start-only league fetch (e.g. a Settings button after a
/// server rollover). Fire-and-forget: it spawns the same bounded-retry task.
/// There is deliberately no background poller — nothing to poll until a server
/// rollover signal exists.
#[tauri::command]
pub fn refresh_league(app: AppHandle) {
    spawn_league_fetch(app);
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
