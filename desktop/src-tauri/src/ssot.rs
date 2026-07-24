//! App-wide cross-window state SSOT (POE-128 chunk 1).
//!
//! Rust-owned single source of truth for state that overlay windows need to
//! agree on. Delivery to overlays is Rust-backed **polling** via the `get_ssot`
//! command, NOT cross-window JavaScript events: WebView2 cross-window events
//! have returned stale data / failed silently (see docs/OVERLAY-GUIDE.md
//! "Runtime-earned observations"). `emit_ssot` provides an optional eager
//! `ssot-changed` nudge for the main window; overlays must still poll.
//!
//! Chunk 1 builds only the core types + the poll-target command. Mutators
//! (`set_league`, `refresh_league`), the fetch task, and the webview store land
//! in later chunks.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

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
    // future slices (e.g. account, config) added here as later tasks land.
}

/// Emit `ssot-changed` with the current snapshot.
///
/// Optional eager nudge for the main window; overlay windows poll `get_ssot`
/// instead. Lock-then-emit discipline: the `ssot` guard is scoped to a block
/// that ends **before** `app.emit(...)`, so the mutex is never held across the
/// emit call (mirrors `emit_logs` and the lab_state pattern in lib.rs).
///
/// Unused in chunk 1 — later chunks call this after mutating the SSOT.
#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks the fail-closed default: a fresh snapshot reports the league as
    /// not-yet-fetched (`None`), never a spuriously "valid" empty name.
    #[test]
    fn default_snapshot_league_is_unfetched() {
        let snap = AppSsotSnapshot::default();
        assert_eq!(snap.league.name, None);
    }
}
