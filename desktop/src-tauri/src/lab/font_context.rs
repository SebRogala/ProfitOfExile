//! The font context lifecycle (POE-391): arm, disarm, record a read and
//! finalize, each under one `font_session` acquisition.
//!
//! Every write of `font_context` and `font_idle_seq` happens while the
//! `font_session` guard is held, the guard `record_font_read` checks the
//! token under. A read that wins the guard first bumps the idle seq, so an
//! expiring timer no-ops; a finalize that wins first leaves `font_context` 0
//! before any reader can see the reset session, so the late read writes
//! nothing. The alert set is cleared inside the same guard (r2-M1), so a
//! replacement context, which can only arm once the guard is released, never
//! has its alert cleared by the old finalize.
//!
//! The pure halves take `&AppState`; the adapters below them add the logs,
//! emits, timer and POST.

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::font_parser::FontPanelState;
use crate::font_session::{self, FrameOutcome, SealedRound};
use crate::lab::commands::LabDetectionUpdate;
use crate::{app_log, body_excerpt, emit_status, font_alerts, AppState, FontSessionData};

/// No font read that saw the panel for this long seals and sends the session
/// (C7).
pub(crate) const IDLE_LIMIT: Duration = Duration::from_secs(600);

/// A freshly armed context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FontArm {
    pub token: u64,
    pub idle_seq: u64,
}

/// What one current font read did.
pub(crate) struct FontReadRecord {
    /// Set when the read carried options.
    pub outcome: Option<FrameOutcome>,
    pub detection: Option<LabDetectionUpdate>,
    /// The idle seq the adapter spawns its timer for.
    pub idle_seq: u64,
}

/// What a finalize took out of the session.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FontUpload {
    pub sealed_leftover: Option<SealedRound>,
    /// The `POST /api/desktop/font-session` body; `None` with no rounds.
    pub payload: Option<serde_json::Value>,
    pub rounds: usize,
}

fn bump_idle(state: &AppState) -> u64 {
    state.lab_scan.font_idle_seq.fetch_add(1, Ordering::SeqCst) + 1
}

/// Mint a token, store it, reset the session and clear the alerts.
fn arm_locked(state: &AppState, session: &mut FontSessionData) -> FontArm {
    let token = font_session::next_scan_generation(&state.lab_scan.read_token_seq);
    state.lab_scan.font_context.store(token, Ordering::SeqCst);
    *session = FontSessionData::default();
    font_alerts::clear_alert_reads(&state.lab_scan.font_alert_seen);
    FontArm { token, idle_seq: bump_idle(state) }
}

/// LabFinished: arm a fresh font context.
pub(crate) fn arm_font_context(state: &AppState) -> FontArm {
    let mut session = state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner());
    arm_locked(state, &mut session)
}

/// FontOpened: arm only when no context is armed (the portal return; a
/// `LabFinished` does not fire again after it).
pub(crate) fn arm_font_context_if_unarmed(state: &AppState) -> Option<FontArm> {
    let mut session = state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner());
    if state.lab_scan.font_context.load(Ordering::SeqCst) != 0 {
        return None;
    }
    Some(arm_locked(state, &mut session))
}

/// LabExited: end the context without sending anything (as before POE-391).
/// Returns whether one was armed.
pub(crate) fn disarm_font_context(state: &AppState) -> bool {
    let _session = state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner());
    let was_armed = state.lab_scan.font_context.swap(0, Ordering::SeqCst) != 0;
    bump_idle(state);
    font_alerts::clear_alert_reads(&state.lab_scan.font_alert_seen);
    was_armed
}

/// Record one read of an active panel captured under `captured`. `None` when
/// that context is no longer armed.
pub(crate) fn record_font_read(
    state: &AppState,
    captured: u64,
    panel: &FontPanelState,
    event_seq: u64,
) -> Option<FontReadRecord> {
    let mut session = state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner());
    if state.lab_scan.font_context.load(Ordering::SeqCst) != captured {
        return None;
    }
    let idle_seq = bump_idle(state);
    if panel.options.is_empty() {
        return Some(FontReadRecord { outcome: None, detection: None, idle_seq });
    }
    // Resolved before the session lock is released: a finalize may seal and
    // upload this session the moment it is.
    let detection = crate::record_lab_detection(state, |detection| {
        detection.observe_font(
            panel.options.iter().map(|option| option.option_type.as_str()),
            panel.crafts_remaining,
        );
    });
    let outcome = font_session::apply_font_frame(&mut session, panel, event_seq);
    Some(FontReadRecord { outcome: Some(outcome), detection: Some(detection), idle_seq })
}

/// End the context and take the session out in one `font_session` hold.
///
/// With `Some(seq)` (the idle timer) this acts only if no arm, disarm or
/// panel read happened since the timer was spawned and a context is armed.
/// `None` (ZoneChanged) always acts.
pub(crate) fn take_font_upload(state: &AppState, expected_idle_seq: Option<u64>) -> Option<FontUpload> {
    // Read before the session guard: `build_status` holds `pair_code` while it
    // takes `font_session`.
    let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mut session = state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(seq) = expected_idle_seq {
        if state.lab_scan.font_idle_seq.load(Ordering::SeqCst) != seq
            || state.lab_scan.font_context.load(Ordering::SeqCst) == 0
        {
            return None;
        }
    }
    state.lab_scan.font_context.store(0, Ordering::SeqCst);
    bump_idle(state);
    font_alerts::clear_alert_reads(&state.lab_scan.font_alert_seen);

    let sealed_leftover = font_session::seal_leftover_round(&mut session);
    let lab_mode = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).lab_mode.clone();
    // The observed lab type is independent of the comparator override. If the
    // watcher missed lab entry, retain the manual-mode fallback.
    let lab_type = state
        .lab_scan
        .detection
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .detected_type()
        .unwrap_or(if lab_mode == "Dedication" { "Dedication" } else { "Unknown" });
    let variant = super::commands::current_gem_variant(state);
    let payload = font_session::finalize_font_session(&session, lab_type, &variant, &state.device_id, &pair);
    let rounds = session.rounds.len();
    *session = FontSessionData::default();
    Some(FontUpload { sealed_leftover, payload, rounds })
}

pub(crate) fn log_sealed_round(app: &AppHandle, sealed: &SealedRound) {
    app_log(
        app,
        format!(
            "Font round {} sealed ({} options{})",
            sealed.number,
            sealed.round.options.len(),
            sealed.round.crafts_remaining.map_or(", last craft".to_string(), |n| format!(", {} remaining", n)),
        ),
    );
}

/// The finalize adapter: take the upload, log, emit the status and POST.
/// Returns whether it acted.
pub(crate) fn finalize_font_context(app: &AppHandle, expected_idle_seq: Option<u64>) -> bool {
    let state = app.state::<AppState>();
    let Some(upload) = take_font_upload(&state, expected_idle_seq) else { return false };
    if let Some(sealed) = &upload.sealed_leftover {
        log_sealed_round(app, sealed);
    }
    if let Some(payload) = upload.payload {
        app_log(app, format!("Sending font session: {} rounds", upload.rounds));
        let server = crate::settings::server_base(&state);
        let http = state.server_http.clone();
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            let url = format!("{}/api/desktop/font-session", server);
            match http.post(&url).json(&payload).send().await {
                Ok(res) if res.status().is_success() => {
                    app_log(&app_clone, "Font session sent successfully".to_string());
                }
                Ok(res) => {
                    let status = res.status();
                    let body = res.text().await.unwrap_or_default();
                    app_log(&app_clone, format!("Font session rejected: {} — {}", status, body_excerpt(&body, 200)));
                }
                Err(e) => {
                    app_log(&app_clone, format!("Font session send failed: {}", e));
                }
            }
        });
    }
    emit_status(app);
    true
}

/// Finalize `IDLE_LIMIT` after `seq`, unless anything bumped the seq first.
pub(crate) fn spawn_font_idle_timer(app: &AppHandle, seq: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(IDLE_LIMIT).await;
        if finalize_font_context(&app, Some(seq)) {
            app_log(&app, "Font session sealed (no font panel read for 10 minutes)".to_string());
        }
    });
}

/// LabFinished adapter.
pub(crate) fn start_font_context(app: &AppHandle) {
    let arm = arm_font_context(&app.state::<AppState>());
    after_arm(app, arm, "Font reader armed");
}

/// FontOpened adapter.
pub(crate) fn rearm_font_context(app: &AppHandle) {
    if let Some(arm) = arm_font_context_if_unarmed(&app.state::<AppState>()) {
        after_arm(app, arm, "Font reader re-armed");
    }
}

fn after_arm(app: &AppHandle, arm: FontArm, what: &str) {
    app_log(app, format!("{} (token={}) — press the read key with the options panel open", what, arm.token));
    emit_status(app);
    spawn_font_idle_timer(app, arm.idle_seq);
    super::gem_vocab::spawn_gem_vocab_preload(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_alerts::{alert_crafts, record_current_alert_read};
    use crate::font_parser::{CountRead, CraftOption};
    use crate::settings::tests::test_app_state;

    fn opt(t: &str) -> CraftOption {
        CraftOption { option_type: t.into(), text: format!("{t} line"), value: None }
    }

    fn panel(types: &[&str], count: i32) -> FontPanelState {
        FontPanelState {
            options: types.iter().map(|t| opt(t)).collect(),
            crafts_remaining: Some(count),
            count_read: CountRead::Count(count),
            font_active: true,
            jackpot_detected: false,
        }
    }

    fn three_options() -> FontPanelState {
        panel(&["transform_random", "exchange_exceptional", "quality"], 3)
    }

    fn alerts(state: &AppState) -> Vec<&'static str> {
        alert_crafts(&state.lab_scan.font_alert_seen.lock().unwrap())
    }

    fn font_context(state: &AppState) -> u64 {
        state.lab_scan.font_context.load(Ordering::SeqCst)
    }

    /// What the reader does with a current read: alerts under their own lock,
    /// then the session.
    fn read(state: &AppState, token: u64, panel: &FontPanelState, event_seq: u64) -> Option<FontReadRecord> {
        record_current_alert_read(
            &state.lab_scan.font_alert_seen,
            || font_context(state) == token,
            panel.font_active,
            &panel.options,
        );
        record_font_read(state, token, panel, event_seq)
    }

    #[test]
    fn a_read_captured_before_a_disarm_writes_nothing_into_the_rearmed_context() {
        let state = test_app_state();
        let old = arm_font_context(&state).token;
        take_font_upload(&state, None);
        let new = arm_font_context_if_unarmed(&state).expect("unarmed after the finalize").token;

        assert_ne!(new, old);
        assert!(read(&state, old, &three_options(), 1).is_none());
        let session = state.lab_scan.font_session.lock().unwrap().clone();
        assert!(session.current_options.is_none() && session.rounds.is_empty());
        assert!(alerts(&state).is_empty());
        assert_eq!(take_font_upload(&state, None).unwrap().payload, None);
    }

    #[test]
    fn a_panel_read_refreshes_the_idle_deadline() {
        let state = test_app_state();
        let arm = arm_font_context(&state);
        let s2 = record_font_read(&state, arm.token, &three_options(), 1).unwrap().idle_seq;

        assert_eq!(take_font_upload(&state, Some(arm.idle_seq)), None);
        assert!(state.lab_scan.font_session.lock().unwrap().current_options.is_some());

        let upload = take_font_upload(&state, Some(s2)).expect("the latest seq expires the context");
        assert_eq!(upload.rounds, 1);
        assert_eq!(upload.payload.unwrap()["rounds"].as_array().map(Vec::len), Some(1));
        assert_eq!(font_context(&state), 0);
    }

    #[test]
    fn a_timer_from_before_a_rearm_neither_seals_nor_disarms_the_replacement() {
        let state = test_app_state();
        let a = arm_font_context(&state);
        take_font_upload(&state, None);
        let b = arm_font_context_if_unarmed(&state).unwrap();
        record_font_read(&state, b.token, &three_options(), 1).unwrap();

        assert_eq!(take_font_upload(&state, Some(a.idle_seq)), None);
        assert_eq!(font_context(&state), b.token);
        assert!(state.lab_scan.font_session.lock().unwrap().current_options.is_some());
    }

    #[test]
    fn a_read_pending_when_its_context_expires_writes_nothing() {
        let state = test_app_state();
        let a = arm_font_context(&state);
        assert!(take_font_upload(&state, Some(a.idle_seq)).is_some());

        assert!(read(&state, a.token, &three_options(), 1).is_none());
        assert!(state.lab_scan.font_session.lock().unwrap().current_options.is_none());
        assert!(alerts(&state).is_empty());
    }

    #[test]
    fn a_zone_change_seals_the_buffered_round_in_the_same_transition() {
        let state = test_app_state();
        let a = arm_font_context(&state);
        record_font_read(&state, a.token, &three_options(), 1).unwrap();

        let upload = take_font_upload(&state, None).unwrap();

        assert_eq!(upload.sealed_leftover.map(|s| s.round.options), Some(three_options().options));
        assert_eq!(upload.payload.unwrap()["rounds"].as_array().map(Vec::len), Some(1));
        let session = state.lab_scan.font_session.lock().unwrap().clone();
        assert!(session.rounds.is_empty() && session.current_options.is_none());
    }

    #[test]
    fn a_read_after_lab_exit_is_rejected_and_nothing_is_sent() {
        let state = test_app_state();
        let a = arm_font_context(&state);

        assert!(disarm_font_context(&state));
        assert!(record_font_read(&state, a.token, &three_options(), 1).is_none());
        assert_eq!(take_font_upload(&state, None).unwrap().payload, None);
    }

    #[test]
    fn the_old_finalize_never_clears_the_replacement_contexts_alert() {
        let state = test_app_state();
        let a = arm_font_context(&state);
        read(&state, a.token, &three_options(), 1).unwrap();
        take_font_upload(&state, None);
        let b = arm_font_context_if_unarmed(&state).unwrap();
        read(&state, b.token, &three_options(), 2).unwrap();

        assert_eq!(alerts(&state), vec!["exchange_exceptional"]);
    }

    #[test]
    fn a_finalize_clears_the_alerts_before_it_releases_the_session() {
        // Structural form of r2-M1: with the alert lock held by the test, the
        // finalize must block on it while still holding `font_session`; a
        // release-then-clear order would have let the session go by then.
        let state = std::sync::Arc::new(test_app_state());
        let a = arm_font_context(&state);
        read(&state, a.token, &three_options(), 1).unwrap();

        let alert_guard = state.lab_scan.font_alert_seen.lock().unwrap();
        let worker = {
            let state = state.clone();
            std::thread::spawn(move || take_font_upload(&state, None).is_some())
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while font_context(&state) != 0 {
            assert!(std::time::Instant::now() < deadline, "the finalize never stored font_context = 0");
            std::thread::yield_now();
        }
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            state.lab_scan.font_session.try_lock().is_err(),
            "the finalize released font_session before clearing the alerts",
        );
        drop(alert_guard);

        assert!(worker.join().unwrap());
        assert!(alerts(&state).is_empty());
        assert_eq!(font_context(&state), 0);
    }
}
