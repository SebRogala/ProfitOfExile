//! The lab command group (POE-342): Compass and lab settings, the gem and
//! font scan spawns, lab-mode switching and detection, and gem/font session
//! submission to the server.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager};

use crate::{
    app_log, body_excerpt, capture, emit_status, font_alerts, font_session, gem_matcher, lab_detection,
    lab_navigation, lab_scan, lab_state, ocr, persist_settings, AppState, FontSessionData,
};

#[tauri::command]
pub(crate) fn get_compass_settings(app: AppHandle) -> serde_json::Value {
    let state = app.state::<AppState>();
    let compass = state.compass_settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
    serde_json::json!({
        "mode": compass.compass_mode, "strategy": compass.compass_strategy, "difficulty": compass.compass_difficulty,
        "shrine_warn_enabled": compass.shrine_warn_enabled,
        "shrine_warn_size": compass.shrine_warn_size,
        "shrine_warn_corner": compass.shrine_warn_corner,
        "shrine_warn_on_take": compass.shrine_warn_on_take,
    })
}

#[tauri::command]
pub(crate) fn set_compass_mode(mode: String, app: AppHandle) {
    let state = app.state::<AppState>();
    state.compass_settings.lock().unwrap_or_else(|e| e.into_inner()).compass_mode = mode;
    persist_settings(&app);
}

#[tauri::command]
pub(crate) fn set_compass_strategy(strategy: String, app: AppHandle) {
    let state = app.state::<AppState>();
    state.compass_settings.lock().unwrap_or_else(|e| e.into_inner()).compass_strategy = strategy;
    persist_settings(&app);
}

#[tauri::command]
pub(crate) fn set_compass_difficulty(difficulty: String, app: AppHandle) {
    let state = app.state::<AppState>();
    state.compass_settings.lock().unwrap_or_else(|e| e.into_inner()).compass_difficulty = difficulty;
    persist_settings(&app);
}

#[tauri::command]
pub(crate) fn set_shrine_warn(enabled: bool, size: String, corner: String, on_take: String, app: AppHandle) {
    let state = app.state::<AppState>();
    {
        let mut compass = state.compass_settings.lock().unwrap_or_else(|e| e.into_inner());
        compass.shrine_warn_enabled = enabled;
        compass.shrine_warn_size = size;
        compass.shrine_warn_corner = corner;
        compass.shrine_warn_on_take = on_take;
    }
    persist_settings(&app);
}

/// Returns recent lab nav events for overlays to catch up on mount.
/// Reads last 32KB of Client.txt and replays from last PlazaEntered/LabExited.
#[tauri::command]
pub(crate) fn get_lab_catchup(app: AppHandle) -> serde_json::Value {
    let state = app.state::<AppState>();
    let client_txt = state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let (events, in_lab) = lab_navigation::replay_recent_log(
        std::path::Path::new(&client_txt),
    );
    serde_json::json!({ "events": events, "in_lab": in_lab })
}

/// Start a gem OCR scan. Used by both FontOpened and manual trigger.
/// - Clears comparator (gems-cleared)
/// - Bumps generation counter (cancels any running scan)
/// - Sets state to PickingGems
/// - Ensures the single merged Lab capture loop is running
pub(crate) fn spawn_gem_scan(app: &AppHandle, source: &str) {
    let state = app.state::<AppState>();

    // Bump generation — the merged capture loop resets its gem-local state on
    // the next tick.
    let gen = state.lab_scan.gem_scan_generation.fetch_add(1, Ordering::SeqCst) + 1;

    // Clear frontend comparator.
    *state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner()) = Vec::new();
    if let Err(e) = app.emit("gems-cleared", ()) { log::warn!("emit gems-cleared failed: {}", e); }

    // Set state to PickingGems (capture loop checks this + generation).
    *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) = lab_state::LabState::PickingGems;
    emit_status(app);

    app_log(app, format!("Gem scan started ({}, gen={})", source, gen));

    ensure_lab_scan(app);
}

/// Start the one dedicated OS thread shared by gem and font OCR. The liveness
/// token prevents a gem trigger and a font trigger from starting two monitor
/// grabs for the same Lab tick.
fn ensure_lab_scan(app: &AppHandle) {
    let state = app.state::<AppState>();
    let generation = font_session::next_scan_generation(&state.lab_scan.lab_scan_generation);
    if state
        .lab_scan
        .lab_scan_live_gen
        .compare_exchange(0, generation, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let app_capture = app.clone();
    // Screen capture and OCR use blocking Windows COM APIs and must stay on a
    // dedicated OS thread.
    std::thread::spawn(move || lab_scan::run(app_capture, generation));
}

#[tauri::command]
pub(crate) async fn start_scanning(app: AppHandle) -> Result<(), String> {
    spawn_gem_scan(&app, "manual");
    Ok(())
}

#[tauri::command]
pub(crate) fn stop_scanning(app: AppHandle) {
    let state = app.state::<AppState>();
    app_log(&app, "Manual scan stopped".to_string());
    // Bump generation to cancel any running scan.
    state.lab_scan.gem_scan_generation.fetch_add(1, Ordering::SeqCst);
    *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) = lab_state::LabState::Idle;
    emit_status(&app);
}

#[tauri::command]
pub(crate) fn set_comparator_data(payload: serde_json::Value, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.comparator_data.lock().unwrap_or_else(|e| e.into_inner()) = payload;
}

#[tauri::command]
pub(crate) fn get_comparator_data(state: tauri::State<AppState>) -> serde_json::Value {
    state.comparator_data.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub(crate) fn get_lab_overlays_enabled(app: AppHandle) -> bool {
    let state = app.state::<AppState>();
    let val = *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner());
    val
}

#[tauri::command]
pub(crate) fn set_lab_overlays_enabled(enabled: bool, app: AppHandle) {
    {
        let state = app.state::<AppState>();
        *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) = enabled;
    }
    persist_settings(&app);
}

#[tauri::command]
pub(crate) fn get_lab_mode(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let val = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).lab_mode.clone();
    val
}

#[tauri::command]
pub(crate) fn set_lab_mode(mode: String, app: AppHandle) {
    if mode != "Normal" && mode != "Dedication" {
        return;
    }
    let state = app.state::<AppState>();
    apply_manual_lab_mode(&state, &mode, |event| {
        if let Err(e) = app.emit(event, ()) {
            log::warn!("emit {} failed: {}", event, e);
        }
    });
    persist_settings(&app);
}

pub(crate) fn apply_manual_lab_mode(state: &AppState, mode: &str, mut emit: impl FnMut(&str)) {
    let changed = {
        let mut detection = state.lab_scan.detection.lock().unwrap_or_else(|e| e.into_inner());
        detection.override_mode();
        change_lab_mode(state, mode)
    };
    if changed.is_some() {
        // A completed scan is Idle and will not restart on the generation
        // bump: retain its gems so Comparator's mode reaction can reprice.
        // An active scan resets its vocabulary and duplicate tracking in
        // GemProcessingState::reset_for_generation, so discard partial picks.
        let scanning = *state.lab_state.lock().unwrap_or_else(|e| e.into_inner())
            == lab_state::LabState::PickingGems;
        if scanning {
            state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
        emit("lab-mode-changed");
        if scanning {
            emit("gems-cleared");
        }
    }
}

/// Called under the detection lock so a manual override cannot race an auto-switch.
fn change_lab_mode(state: &AppState, mode: &str) -> Option<String> {
    let mut lab = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner());
    if lab.lab_mode == mode {
        return None;
    }
    let previous = std::mem::replace(&mut lab.lab_mode, mode.to_string());
    // GemProcessingState::reset_for_generation drops the old matcher and any
    // pending dictionary response, then fetches vocabulary for the new mode.
    state.lab_scan.gem_scan_generation.fetch_add(1, Ordering::SeqCst);
    Some(previous)
}

fn emit_lab_mode_change(app: &AppHandle) {
    app.state::<AppState>().lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner()).clear();
    if let Err(e) = app.emit("lab-mode-changed", ()) {
        log::warn!("emit lab-mode-changed failed: {}", e);
    }
    if let Err(e) = app.emit("gems-cleared", ()) {
        log::warn!("emit gems-cleared failed: {}", e);
    }
}

pub(crate) struct LabDetectionUpdate {
    pub(crate) detected: Option<&'static str>,
    changed_type: bool,
    pub(crate) mode_change: Option<(String, &'static str)>,
}

/// Record under the font session lock when called from OCR, so a concurrent
/// upload sees its type. Keep gem clearing and events in publish_lab_detection:
/// build_status acquires the gem mutex before the font session mutex.
pub(crate) fn record_lab_detection(
    state: &AppState,
    update: impl FnOnce(&mut lab_detection::LabDetection),
) -> LabDetectionUpdate {
    let (detected, changed_type, mode_change) = {
        let mut detection = state.lab_scan.detection.lock().unwrap_or_else(|e| e.into_inner());
        let previous = detection.detected_type();
        update(&mut detection);
        let detected = detection.detected_type();
        let mode_change = detection.automatic_mode().and_then(|mode| {
            change_lab_mode(state, mode).map(|old| (old, mode))
        });
        (detected, previous != detected, mode_change)
    };
    LabDetectionUpdate { detected, changed_type, mode_change }
}

pub(crate) fn publish_lab_detection(app: &AppHandle, update: LabDetectionUpdate) {
    if update.changed_type {
        if let Some(kind) = update.detected {
            app_log(app, format!("Lab type auto-detected: {}", kind));
        }
    }
    if let Some((old, mode)) = update.mode_change {
        app_log(app, format!("Lab mode auto-switch: {} -> {} (detected {})", old, mode, update.detected.unwrap_or("Unknown")));
        emit_lab_mode_change(app);
    }
}

#[tauri::command]
pub(crate) fn get_autoclear_minutes(app: AppHandle) -> u32 {
    let state = app.state::<AppState>();
    let value = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).autoclear_minutes;
    value
}

#[tauri::command]
pub(crate) fn set_autoclear_minutes(minutes: u32, app: AppHandle) {
    let state = app.state::<AppState>();
    state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).autoclear_minutes = minutes;
    persist_settings(&app);
}

#[tauri::command]
pub(crate) fn get_dedication_pool(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let value = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).dedication_pool.clone();
    value
}

#[tauri::command]
pub(crate) fn set_dedication_pool(pool: String, app: AppHandle) {
    let state = app.state::<AppState>();
    state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).dedication_pool = pool;
    persist_settings(&app);
}

#[tauri::command]
pub(crate) fn get_dedication_variant(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let value = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).dedication_variant.clone();
    value
}

#[tauri::command]
pub(crate) fn set_dedication_variant(variant: String, app: AppHandle) {
    let state = app.state::<AppState>();
    state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).dedication_variant = variant;
    persist_settings(&app);
}

#[tauri::command]
pub(crate) fn get_normal_variant(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let value = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).normal_variant.clone();
    value
}

#[tauri::command]
pub(crate) fn set_normal_variant(variant: String, app: AppHandle) {
    let state = app.state::<AppState>();
    state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).normal_variant = variant;
    persist_settings(&app);
}

#[tauri::command]
pub(crate) fn get_show_low_confidence(app: AppHandle) -> bool {
    let state = app.state::<AppState>();
    let value = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).show_low_confidence;
    value
}

#[tauri::command]
pub(crate) fn set_show_low_confidence(show: bool, app: AppHandle) {
    let state = app.state::<AppState>();
    state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).show_low_confidence = show;
    persist_settings(&app);
}

/// Dev-only: emit a fake lab-nav event to all windows for testing the compass overlay.
#[tauri::command]
pub(crate) fn emit_lab_nav(event_json: serde_json::Value, app: AppHandle) {
    app_log(&app, format!("Dev: emitting lab-nav {:?}", event_json));
    if let Err(e) = app.emit("lab-nav", &event_json) {
        log::warn!("emit lab-nav failed: {}", e);
    }
}

#[tauri::command]
pub(crate) async fn send_test_gems(app: AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let url = format!("{}/api/desktop/gems", server);
    let gems = vec![
        "Earthquake of Fragility",
        "Boneshatter of Carnage",
        "Summon Stone Golem of Safeguarding",
    ];

    app_log(&app, format!("Sending test gems to {}", url));

    let http = state.server_http.clone();

    // Same wire format and same receiver as the real scan, so it has to carry
    // the same market — otherwise "test the pairing pipe" tests a shape the pipe
    // never sends in Dedication.
    let variant = current_gem_variant(&state);
    let mode = current_lab_mode_tag(&state);

    let res = http
        .post(&url)
        .json(&serde_json::json!({
            "pair": pair,
            "gems": gems,
            "variant": variant,
            "mode": mode
        }))
        .send()
        .await
        .map_err(|e| {
            let msg = format!("Request failed: {} (is_connect: {}, is_timeout: {})",
                e, e.is_connect(), e.is_timeout());
            app_log(&app, msg.clone());
            msg
        })?;

    let status = res.status();
    app_log(&app, format!("Response: {} {}", status.as_u16(), status.canonical_reason().unwrap_or("")));

    if status.is_success() {
        let msg = "Test gems sent!".to_string();
        app_log(&app, msg.clone());
        Ok(msg)
    } else {
        let body = res.text().await.unwrap_or_else(|e| format!("<body read failed: {}>", e));
        let msg = format!("Server returned {}: {}", status, body);
        app_log(&app, msg.clone());
        Err(msg)
    }
}

/// The mode those gems were scanned in, in the web view's own vocabulary. Sent
/// alongside the market because a market alone cannot be validated: the paired
/// view only knows its OWN mode's markets, so a market from the other mode fails
/// its check and — before this — was dropped without a word.
fn current_lab_mode_tag(state: &tauri::State<'_, AppState>) -> &'static str {
    let lab_mode = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).lab_mode.clone();
    if lab_mode == "Dedication" { "dedication" } else { "normal" }
}

/// The market OCR'd gems are priced against, and the single source of truth for
/// it: the selected corrupted market in Dedication, the selected plain market
/// otherwise. Both live here rather than in a window, because two windows and
/// the paired web view all have to agree on which market they are describing.
///
/// It used to be a hardcoded "20/20", which is not even one of the Dedication
/// markets — the web view took it, failed to match it, and silently priced
/// corrupted gems against the uncorrupted market.
fn current_gem_variant(state: &tauri::State<'_, AppState>) -> String {
    let lab = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner());
    if lab.lab_mode == "Dedication" {
        lab.dedication_variant.clone()
    } else {
        lab.normal_variant.clone()
    }
}

pub(crate) async fn send_gems_to_server(app: &AppHandle, gems: Vec<String>) {
    let state = app.state::<AppState>();
    let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let url = format!("{}/api/desktop/gems", server);
    let http = state.server_http.clone();

    // The web view picks its market from this field. A hardcoded 20/20 does not
    // exist in Dedication mode, so its comparator silently fell back to the
    // first Dedication market (21/20) whatever the player had selected.
    let variant = current_gem_variant(&state);
    let mode = current_lab_mode_tag(&state);

    app_log(app, format!("Sending {} gems to server", gems.len()));

    match http
        .post(&url)
        .json(&serde_json::json!({
            "pair": pair,
            "gems": gems,
            "variant": variant,
            "mode": mode
        }))
        .send()
        .await
    {
        Ok(res) => {
            app_log(app, format!("Server response: {}", res.status()));
        }
        Err(e) => {
            app_log(app, format!("Send failed: {}", e));
        }
    }
}

#[tauri::command]
pub(crate) async fn test_ocr_on_image(path: String, app: AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    app_log(&app, format!("Testing OCR on: {}", path));

    let img = image::open(&path).map_err(|e| format!("Failed to open image: {}", e))?;
    app_log(&app, format!("Image loaded: {}x{}", img.width(), img.height()));

    // Preprocessing and recognition are both blocking CPU work — a 2x Lanczos
    // resize of an arbitrary image the user picked, then a synchronous WinRT OCR
    // call. This command is async, so running them inline would stall the async
    // worker (and every other command sharing it) for the duration.
    let (proc_w, proc_h, ocr_result) = tauri::async_runtime::spawn_blocking(move || {
        let processed = capture::preprocess_for_ocr(&img);
        let dims = (processed.width(), processed.height());
        (dims.0, dims.1, ocr::recognize_text(&processed))
    })
    .await
    .map_err(|e| format!("OCR task failed to run: {}", e))?;
    app_log(&app, format!("Preprocessed: {}x{}", proc_w, proc_h));

    let lines = ocr_result.map_err(|e| {
        app_log(&app, format!("OCR failed: {}", e));
        e
    })?;

    app_log(&app, format!("OCR found {} lines", lines.len()));
    for (i, line) in lines.iter().enumerate() {
        app_log(&app, format!("  Line {}: {}", i, line));
    }

    // Try all OCR lines against the matcher — pick the best match
    let candidates = ocr::extract_gem_candidates(&lines);
    app_log(&app, format!("{} candidate lines to match", candidates.len()));

    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let http = state.server_http.clone();
    let lab_mode = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).lab_mode.clone();
    let gem_names = fetch_gem_names(&app, &server, &http, &lab_mode).await;
    let matcher = gem_matcher::GemMatcher::new(gem_names);

    let mut best_match: Option<gem_matcher::GemMatch> = None;
    for candidate in &candidates {
        match matcher.match_gem(candidate) {
            Ok(m) => {
                if best_match.as_ref().map_or(true, |b| m.score > b.score) {
                    best_match = Some(m);
                }
            }
            // Unthrottled and undeduplicated, unlike the scan loop: this command
            // is a one-shot manual probe over a fixed candidate list, so every
            // rejection is a result the operator asked for.
            Err(reason) => app_log(&app, format!("  Rejected {:?}: {}", candidate, reason)),
        }
    }

    if let Some(m) = best_match {
        let result = format!("Matched: {} (score: {:.2})", m.name, m.score);
        app_log(&app, result.clone());

        // Send to server
        let mut gems = state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
        if !gems.contains(&m.name) {
            gems.push(m.name.clone());
            let all_gems = gems.clone();
            drop(gems);
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                send_gems_to_server(&app_clone, all_gems).await;
            });
            app_log(&app, format!("Sent {} to comparator", m.name));
        }

        Ok(result)
    } else {
        let result = format!("No match in {} candidates", candidates.len());
        app_log(&app, result.clone());
        Ok(result)
    }
}

/// Start font panel OCR in the shared Lab scan loop. Bumps the font generation
/// to cancel the previous font session, claims its liveness token before the
/// reset is visible to `FontOpened`, and then reuses an existing Lab thread when
/// one is already running.
pub(crate) fn spawn_font_scan(app: &AppHandle) {
    let state = app.state::<AppState>();
    let generation = font_session::next_scan_generation(&state.lab_scan.font_scan_generation);
    state.lab_scan.font_scan_live_gen.store(generation, Ordering::SeqCst);

    *state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner()) = FontSessionData::default();
    font_alerts::clear_alert_reads(&state.lab_scan.font_alert_seen);
    app_log(app, format!("Font scan started (gen={})", generation));
    emit_status(app);

    ensure_lab_scan(app);
}

/// Seal the round buffer left over when the session ends without another craft.
///
/// Tauri-side adapter over `font_session::seal_leftover_round`: locks, seals,
/// logs and emits. Rounds are otherwise sealed by the craft-count ledger inside
/// `lab_scan::run`; `FontOpened` seals nothing, because it fires on font open
/// as well as on CRAFT.
fn seal_font_round(app: &AppHandle) {
    let state = app.state::<AppState>();

    // The guard lives in this block only: `emit_status` -> `build_status` locks
    // `font_session` to read `font_session_rounds`, and std Mutex is not
    // re-entrant, so emitting while the guard is alive deadlocks this thread.
    let sealed = {
        let mut session = state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner());
        font_session::seal_leftover_round(&mut session)
    };

    // Nothing buffered. This is the ordinary case on the send path: every
    // ZoneChanged calls it, and most of them happen with no font session at
    // all. `rounds` did not change, so there is nothing to log and the status
    // the frontend already holds is still accurate.
    let Some(sealed) = sealed else { return };

    app_log(app, format!(
        "Font round {} sealed ({} options{})",
        sealed.number,
        sealed.round.options.len(),
        sealed.round.crafts_remaining.map_or(
            ", last craft".to_string(),
            |n| format!(", {} remaining", n),
        ),
    ));

    // `rounds` just grew, and `font_session_rounds` is what gates the Discard
    // affordance and its round count.
    emit_status(app);
}

/// Throw away the accumulated font session without sending it (POE-163 D4).
///
/// The escape hatch for a session captured against the wrong market: the
/// stamp is read once at send time, so an unwanted run has to be discarded
/// rather than corrected after the fact.
///
/// Deliberately does NOT touch `font_scan_generation` (discarding data must
/// not stop an in-progress scan) or `lab_state` (the player may still be in
/// the lab).
#[tauri::command]
pub(crate) fn discard_font_session(app: AppHandle) {
    let state = app.state::<AppState>();
    let discarded = {
        let mut session = state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner());
        let discarded = session.rounds.len();
        *session = FontSessionData::default();
        discarded
    };
    font_alerts::clear_alert_reads(&state.lab_scan.font_alert_seen);
    app_log(&app, format!("Font session discarded ({} rounds)", discarded));
    emit_status(&app);
}

/// Send accumulated font session to the server and reset.
pub(crate) fn send_font_session_data(app: &AppHandle) {
    // Seal the round still on screen (the player left without another craft).
    // Takes and releases the session lock itself, so it runs before ours.
    seal_font_round(app);

    let state = app.state::<AppState>();
    let mut session = state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner());

    let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let lab_mode = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner()).lab_mode.clone();

    let device_id = state.device_id.clone();

    // The observed lab type is independent of the comparator override. If the
    // watcher missed lab entry, retain the previous manual-mode fallback.
    let lab_type = state.lab_scan.detection.lock().unwrap_or_else(|e| e.into_inner())
        .detected_type()
        .unwrap_or(if lab_mode == "Dedication" { "Dedication" } else { "Unknown" });
    // Record the selected market: a fixed "20/20" previously attributed every
    // Normal run to a market the player may not have been farming.
    let variant = current_gem_variant(&state);

    let session_data = match font_session::finalize_font_session(
        &session,
        lab_type,
        variant.as_str(),
        &device_id,
        &pair,
    ) {
        Some(data) => data,
        // No sealed rounds — nothing was captured, so there is nothing to send
        // and nothing to reset.
        None => return,
    };

    app_log(app, format!("Sending font session: {} rounds", session.rounds.len()));

    let http = state.server_http.clone();

    // Reset session.
    *session = FontSessionData::default();
    drop(session);
    font_alerts::clear_alert_reads(&state.lab_scan.font_alert_seen);

    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let url = format!("{}/api/desktop/font-session", server);
        match http.post(&url).json(&session_data).send().await {
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

/// Why a fetched gem dictionary is unusable, or `None` when it is usable.
///
/// Split out of `fetch_gem_names` so the rule is testable without a live
/// server, and because "no half failed" and "usable" are different questions: a
/// 200 carrying `{"names":[]}` is a documented legitimate response
/// (`GemDictionary`, `internal/server/handlers/collective.go`), so it clears
/// `failed` while still leaving the matcher with nothing to match against. That
/// is what a fresh league returns before the first poe.ninja collection —
/// precisely when lab farming matters most (POE-146).
fn dictionary_reject_reason(failed: &[&str], loaded: usize) -> Option<String> {
    if !failed.is_empty() {
        return Some(format!(
            "{} half failed — matcher would be incomplete, aborting scan",
            failed.join(" + "),
        ));
    }
    if loaded == 0 {
        return Some(
            "every half returned 0 names — the server has no gem dictionary for this league yet"
                .to_string(),
        );
    }
    None
}

/// Fetch gem names from the server API for fuzzy matching.
///
/// In Normal mode: fetches transfigured gem names (contain "of").
/// In Dedication mode: fetches BOTH corrupted skill gem names AND corrupted
/// transfigured gem names, merged into a single vocabulary for the matcher
/// (both font options are available per run).
pub(crate) async fn fetch_gem_names(app: &AppHandle, server_url: &str, client: &reqwest::Client, lab_mode: &str) -> Vec<String> {
    if lab_mode == "Dedication" {
        // Fetch both pools in parallel and merge.
        // Static game-data dictionary — OCR must recognise a gem whether or not the
        // market prices it (a fresh league prices almost nothing for the first hours).
        let url_skills = format!("{}/api/analysis/gems/dictionary?transfigured=false", server_url);
        let url_transfig = format!("{}/api/analysis/gems/dictionary?transfigured=true", server_url);

        let (skills_res, transfig_res) = tokio::join!(
            client.get(&url_skills).send(),
            client.get(&url_transfig).send(),
        );

        let mut all_names = Vec::new();
        // A half-loaded vocabulary is worse than none: the matcher rejects a
        // candidate whose winning name contains " of " when the OCR text does
        // not, so losing the plain-skill half turns a clean read of "Barrage"
        // into zero matches, not a partial one. Track each half and abort the
        // scan rather than silently shipping a truncated dictionary (POE-146).
        let mut failed: Vec<&str> = Vec::new();
        for (label, res) in [("skills", skills_res), ("transfigured", transfig_res)] {
            match res {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(body) => {
                            if let Some(names) = body.get("names").and_then(|n| n.as_array()) {
                                let count = names.len();
                                for n in names {
                                    if let Some(s) = n.as_str() {
                                        all_names.push(s.to_string());
                                    }
                                }
                                app_log(app, format!("Dedication gem names ({}): {} loaded", label, count));
                            } else {
                                failed.push(label);
                                app_log(app, format!("Dedication gem names ({}): response missing 'names' field", label));
                            }
                        }
                        Err(e) => {
                            failed.push(label);
                            app_log(app, format!("Dedication gem names ({}): parse failed: {}", label, e));
                        }
                    }
                }
                Ok(resp) => {
                    failed.push(label);
                    app_log(app, format!("Dedication gem names ({}): server returned {}", label, resp.status()));
                }
                Err(e) => {
                    failed.push(label);
                    app_log(app, format!("Dedication gem names ({}): request failed: {}", label, e));
                }
            }
        }

        if let Some(reason) = dictionary_reject_reason(&failed, all_names.len()) {
            app_log(app, format!("Dedication gem names: {}", reason));
            return Vec::new();
        }

        // Deduplicate (transfigured names are a subset of all corrupted gems).
        all_names.sort();
        all_names.dedup();
        all_names
    } else {
        // Normal mode: transfigured gem names only.
        let url = format!("{}/api/analysis/gems/dictionary?transfigured=true", server_url);
        match client.get(&url).send().await {
            Ok(res) if res.status().is_success() => {
                match res.json::<serde_json::Value>().await {
                    Ok(body) => {
                        if let Some(names) = body.get("names").and_then(|n| n.as_array()) {
                            let loaded: Vec<String> = names
                                .iter()
                                .filter_map(|n| n.as_str().map(String::from))
                                .collect();
                            // Logged on the success path too: a 200 carrying
                            // {"names":[]} is legitimate (a league with nothing
                            // collected yet), and without this line that case
                            // produced no log output whatsoever.
                            app_log(app, format!("Gem names (transfigured): {} loaded", loaded.len()));
                            return loaded;
                        }
                        app_log(app, "Gem names: response missing 'names' field".to_string());
                        Vec::new()
                    }
                    Err(e) => {
                        app_log(app, format!("Gem names: failed to parse response: {}", e));
                        Vec::new()
                    }
                }
            }
            Ok(res) => {
                app_log(app, format!("Gem names: server returned {}", res.status()));
                Vec::new()
            }
            Err(e) => {
                app_log(app, format!("Gem names: request failed: {}", e));
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::dictionary_reject_reason;

    #[test]
    fn a_dictionary_with_both_halves_loaded_is_usable() {
        assert_eq!(dictionary_reject_reason(&[], 593), None);
    }

    #[test]
    fn a_failed_half_names_itself_in_the_reject_reason() {
        let reason = dictionary_reject_reason(&["skills"], 412)
            .expect("a failed half must reject the dictionary even when the other half loaded");
        assert!(
            reason.starts_with("skills half failed"),
            "reason must name the half that failed, got {:?}",
            reason
        );
    }

    #[test]
    fn both_failed_halves_are_named_in_the_reject_reason() {
        let reason = dictionary_reject_reason(&["skills", "transfigured"], 0)
            .expect("two failed halves must reject the dictionary");
        assert!(
            reason.starts_with("skills + transfigured half failed"),
            "reason must name both halves, got {:?}",
            reason
        );
    }

    #[test]
    fn a_successful_but_empty_dictionary_is_rejected() {
        // POE-146's remaining hole: 200 + {"names":[]} is a legitimate response,
        // so no half is recorded as failed — but the matcher still has nothing
        // to match against, and the scan must not proceed as though it had.
        let reason = dictionary_reject_reason(&[], 0)
            .expect("an empty dictionary is unusable even when no request failed");
        assert!(
            reason.starts_with("every half returned 0 names"),
            "reason must say the dictionary was empty rather than blaming a failed half, got {:?}",
            reason
        );
    }
}
