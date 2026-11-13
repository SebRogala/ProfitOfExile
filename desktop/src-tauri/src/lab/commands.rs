//! The lab command group (POE-342): Compass and lab settings, the gem
//! reader's arm and disarm, lab-mode switching and detection, and gem
//! submission to the server. The font context lives in `font_context`.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager};

use crate::{
    app_log, capture, emit_status, font_alerts, font_session, gem_matcher, lab_detection,
    lab_navigation, lab_state, ocr, persist_settings, AppState, FontSessionData,
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

/// Arm the gem reader for a new gem choice (CRAFT): mint a token and clear the
/// held gems, both under the `detected_gems` guard. Returns the token.
pub(crate) fn arm_gem_context(state: &AppState) -> u64 {
    let mut gems = state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
    let token = font_session::next_scan_generation(&state.lab_scan.read_token_seq);
    state.lab_scan.gem_context.store(token, Ordering::SeqCst);
    state.lab_scan.choice_seq.fetch_add(1, Ordering::SeqCst);
    gems.clear();
    token
}

/// End the gem context under the `detected_gems` guard; `clear` also drops
/// the held gems (CONFIRM, ZoneChanged). FontClosed keeps them for the
/// comparator. Returns whether a context was armed.
pub(crate) fn disarm_gem_context(state: &AppState, clear: bool) -> bool {
    let mut gems = state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
    let was_armed = state.lab_scan.gem_context.swap(0, Ordering::SeqCst) != 0;
    if clear {
        gems.clear();
    }
    was_armed
}

/// The CRAFT adapter: arm, then (no lab guard held) `PickingGems`,
/// `gems-cleared`, the status, and the vocabulary preload when none is current.
pub(crate) fn start_gem_context(app: &AppHandle, source: &str) {
    let state = app.state::<AppState>();
    let token = arm_gem_context(&state);
    *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) = lab_state::LabState::PickingGems;
    if let Err(e) = app.emit("gems-cleared", ()) { log::warn!("emit gems-cleared failed: {}", e); }
    emit_status(app);
    app_log(app, format!("Gem reader armed ({}, token={}) — press the read key while hovering a gem", source, token));
    if super::gem_vocab::current(&state.lab_scan).is_none() {
        super::gem_vocab::spawn_gem_vocab_preload(app);
    }
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
    let changed = apply_manual_lab_mode(&state, &mode, |event| {
        if let Err(e) = app.emit(event, ()) {
            log::warn!("emit {} failed: {}", event, e);
        }
    });
    if changed {
        super::gem_vocab::spawn_gem_vocab_preload(&app);
    }
    persist_settings(&app);
}

/// Returns whether the mode changed.
pub(crate) fn apply_manual_lab_mode(state: &AppState, mode: &str, mut emit: impl FnMut(&str)) -> bool {
    // The gem choice as it stands before the generation bump, read under the
    // `detected_gems` guard alone.
    let (armed_before, seq_before) = {
        let _gems = state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
        (
            state.lab_scan.gem_context.load(Ordering::SeqCst) != 0,
            state.lab_scan.choice_seq.load(Ordering::SeqCst),
        )
    };
    #[cfg(test)]
    after_mode_snapshot(state);
    let changed = {
        let mut detection = state.lab_scan.detection.lock().unwrap_or_else(|e| e.into_inner());
        detection.override_mode();
        change_lab_mode(state, mode)
    };
    if changed.is_none() {
        return false;
    }
    // After the generation bump, under the `detected_gems` guard alone
    // (r2-H1; `build_status` holds `detected_gems` while it takes
    // `font_session`): a commit that passed its generation check before the
    // bump is wiped here, and one that checks after sees the new generation.
    // The choice is cleared if it was armed at the snapshot or changed since
    // (an arm or a commit bumps `choice_seq`); a choice finished before the
    // change keeps its gems so the comparator can reprice them.
    let scanning = {
        let mut gems = state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
        let scanning = armed_before || state.lab_scan.choice_seq.load(Ordering::SeqCst) != seq_before;
        if scanning {
            gems.clear();
        }
        scanning
    };
    emit("lab-mode-changed");
    if scanning {
        emit("gems-cleared");
    }
    true
}

/// Called under the detection lock so a manual override cannot race an
/// auto-switch. Bumps `gem_vocab_generation` under the `lab_settings` guard,
/// so the (mode, generation) pair is written atomically. The caller clears the
/// held gems after its guards are released (`apply_manual_lab_mode`,
/// `emit_lab_mode_change`).
fn change_lab_mode(state: &AppState, mode: &str) -> Option<LabModeChange> {
    let mut lab = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner());
    if lab.lab_mode == mode {
        return None;
    }
    let previous = std::mem::replace(&mut lab.lab_mode, mode.to_string());
    state.lab_scan.gem_vocab_generation.fetch_add(1, Ordering::SeqCst);
    Some(LabModeChange { previous })
}

/// Test seam: runs in a manual mode change right after its gem-choice
/// snapshot and before the generation bump, for the state it was installed
/// for. No other arrangement pauses the change inside that gap.
#[cfg(test)]
type ModeHook = Box<dyn Fn() + Send>;
#[cfg(test)]
pub(crate) static AFTER_MODE_SNAPSHOT: std::sync::Mutex<Option<(usize, ModeHook)>> = std::sync::Mutex::new(None);
#[cfg(test)]
fn after_mode_snapshot(state: &AppState) {
    let hook = AFTER_MODE_SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((key, f)) = hook.as_ref() {
        if *key == state as *const AppState as usize {
            f();
        }
    }
}

/// A lab-mode change: the previous mode.
struct LabModeChange {
    previous: String,
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
            change_lab_mode(state, mode).map(|change| (change.previous, mode))
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
        super::gem_vocab::spawn_gem_vocab_preload(app);
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
    let server = crate::settings::server_base(&state);
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
fn current_lab_mode_tag(state: &AppState) -> &'static str {
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
pub(super) fn current_gem_variant(state: &AppState) -> String {
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
    let server = crate::settings::server_base(&state);
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

    let server = crate::settings::server_base(&state);
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

/// Throw away the accumulated font session without sending it (POE-163 D4).
///
/// The escape hatch for a session captured against the wrong market: the
/// stamp is read once at send time, so an unwanted run has to be discarded
/// rather than corrected after the fact.
///
/// Deliberately does NOT touch `font_context` or `font_idle_seq` (discarding
/// data must not disarm the font reader) or `lab_state` (the player may still
/// be in the lab).
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
