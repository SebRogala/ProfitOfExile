mod capture;
mod font_parser;
mod gem_matcher;
mod lab_state;
mod log_watcher;
mod ocr;
mod settings;
mod trade;

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Default for CaptureRegion {
    fn default() -> Self {
        // Default for 1080p — gem name tooltip area
        Self { x: 30, y: 45, w: 550, h: 75 }
    }
}

impl CaptureRegion {
    /// Default for font panel area (1080p) — craft options + "Crafts Remaining"
    pub fn default_font_panel() -> Self {
        Self { x: 460, y: 270, w: 530, h: 350 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub state: String,
    pub pair_code: String,
    pub detected_gems: Vec<String>,
    pub client_txt_path: String,
    pub server_url: String,
    pub gem_region: CaptureRegion,
    pub font_region: CaptureRegion,
}

pub struct AppState {
    pub pair_code: Mutex<String>,
    pub client_txt_path: Mutex<String>,
    pub server_url: Mutex<String>,
    pub detected_gems: Mutex<Vec<String>>,
    pub lab_state: Mutex<lab_state::LabState>,
    pub logs: Mutex<Vec<String>>,
    pub gem_region: Mutex<CaptureRegion>,
    pub font_region: Mutex<CaptureRegion>,
    pub trade_client: trade::TradeApiClient,
    /// Cancel signal for the current log watcher. Send () to stop it.
    pub watcher_cancel: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
}

/// Build the full AppStatus from current state. Used by get_status command and event emitting.
fn build_status(state: &AppState) -> AppStatus {
    AppStatus {
        state: format!("{:?}", *state.lab_state.lock().unwrap_or_else(|e| e.into_inner())),
        pair_code: state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        detected_gems: state.detected_gems.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        client_txt_path: state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        server_url: state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        gem_region: state.gem_region.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        font_region: state.font_region.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }
}

/// Save current settings to disk. Call after any persistent state change.
/// Preserves window position from the existing file (only saved on close).
fn persist_settings(app: &AppHandle) {
    let state = app.state::<AppState>();
    let existing = settings::load(app);
    let mut s = settings::from_state(&state);
    s.window = existing.window; // preserve window settings from last close
    settings::save(app, &s);
}

/// Emit the full app status to all frontend listeners.
fn emit_status(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Err(e) = app.emit("status-changed", build_status(&state)) { log::warn!("emit status-changed failed: {}", e); }
}

/// Emit the current logs array to all frontend listeners.
fn emit_logs(app: &AppHandle) {
    let state = app.state::<AppState>();
    let logs = state.logs.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Err(e) = app.emit("logs-changed", logs) { log::warn!("emit logs-changed failed: {}", e); }
}

/// Add a log entry: in-memory buffer (UI) + persistent file + emit to frontend.
fn app_log(app: &AppHandle, msg: String) {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let formatted = format!("[{}] {}", timestamp, msg);

    // In-memory buffer for UI
    let state = app.state::<AppState>();
    {
        let mut logs = state.logs.lock().unwrap_or_else(|e| e.into_inner());
        logs.push(formatted.clone());
        if logs.len() > 50 {
            let excess = logs.len() - 50;
            logs.drain(0..excess);
        }
    }

    // Persistent log file — same dir as settings
    if let Ok(dir) = app.path().app_data_dir() {
        let log_path = dir.join("app.log");
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let _ = writeln!(file, "{}", formatted);
        }
    }

    emit_logs(app);
}

fn generate_pair_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
    (0..4).map(|_| chars[rng.gen_range(0..chars.len())]).collect()
}

#[tauri::command]
fn get_status(state: tauri::State<AppState>) -> AppStatus {
    build_status(&state)
}

#[tauri::command]
fn get_pair_code(state: tauri::State<AppState>) -> String {
    state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
fn regenerate_pair_code(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let new_code = generate_pair_code();
    *state.pair_code.lock().unwrap_or_else(|e| e.into_inner()) = new_code.clone();
    emit_status(&app);
    new_code
}

const DEFAULT_CLIENT_TXT_PATH: &str = r"C:\Program Files (x86)\Grinding Gear Games\Path of Exile\logs\Client.txt";

#[tauri::command]
fn set_client_txt_path(path: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = path;
    persist_settings(&app);
    emit_status(&app);
    restart_log_watcher(app);
}

#[tauri::command]
fn reset_client_txt_path(app: AppHandle) {
    let state = app.state::<AppState>();
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = DEFAULT_CLIENT_TXT_PATH.to_string();
    persist_settings(&app);
    emit_status(&app);
    restart_log_watcher(app);
}

fn restart_log_watcher(app: AppHandle) {
    let state = app.state::<AppState>();
    // Cancel the existing watcher
    if let Some(cancel_tx) = state.watcher_cancel.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = cancel_tx.send(true);
    }
    app_log(&app, "Restarting log watcher...".to_string());
    spawn_log_watcher(app.clone());
}

#[tauri::command]
fn set_server_url(url: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.server_url.lock().unwrap_or_else(|e| e.into_inner()) = url;
    persist_settings(&app);
    emit_status(&app);
}

#[tauri::command]
fn get_gem_region(state: tauri::State<AppState>) -> CaptureRegion {
    state.gem_region.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
fn set_gem_region(x: i32, y: i32, w: u32, h: u32, app: AppHandle) {
    let state = app.state::<AppState>();
    let region = CaptureRegion { x, y, w, h };
    app_log(&app, format!("Region set: ({}, {}) {}x{}", x, y, w, h));
    *state.gem_region.lock().unwrap_or_else(|e| e.into_inner()) = region;
    persist_settings(&app);
    emit_status(&app);
}

#[tauri::command]
fn get_font_region(state: tauri::State<AppState>) -> CaptureRegion {
    state.font_region.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
fn set_font_region(x: i32, y: i32, w: u32, h: u32, app: AppHandle) {
    let state = app.state::<AppState>();
    let region = CaptureRegion { x, y, w, h };
    app_log(&app, format!("Font region set: ({}, {}) {}x{}", x, y, w, h));
    *state.font_region.lock().unwrap_or_else(|e| e.into_inner()) = region;
    persist_settings(&app);
    emit_status(&app);
}

#[tauri::command]
fn capture_mouse_position() -> Result<(i32, i32), String> {
    // Get current mouse cursor position on screen
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        use windows::Win32::Foundation::POINT;
        let mut point = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut point)
                .map_err(|e| format!("Failed to get cursor position: {}", e))?;
        }
        Ok((point.x, point.y))
    }
    #[cfg(not(windows))]
    {
        Err("Mouse capture not available on this platform".to_string())
    }
}

#[tauri::command]
async fn start_scanning(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let current = state.lab_state.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if current == lab_state::LabState::PickingGems {
        return Err("Already scanning".to_string());
    }

    app_log(&app, "Manual scan started".to_string());
    *state.detected_gems.lock().unwrap_or_else(|e| e.into_inner()) = Vec::new();
    *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) = lab_state::LabState::PickingGems;
    emit_status(&app);

    let app_capture = app.clone();
    tauri::async_runtime::spawn(async move {
        run_capture_loop(&app_capture).await;
    });

    Ok(())
}

#[tauri::command]
fn stop_scanning(app: AppHandle) {
    let state = app.state::<AppState>();
    app_log(&app, "Manual scan stopped".to_string());
    *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) = lab_state::LabState::Idle;
    emit_status(&app);
}

/// Direct trade API lookup against GGG from the desktop app.
/// Each user has their own IP → own rate limits (no shared server bottleneck).
/// Divine rate normalization is skipped for now (raw currency values returned).
#[tauri::command]
async fn trade_lookup(
    gem: String,
    variant: String,
    app: AppHandle,
) -> Result<trade::TradeLookupResult, String> {
    let state = app.state::<AppState>();
    app_log(&app, format!("Trade lookup: {} ({})", gem, variant));

    // TODO: fetch divine rate from server/poe.ninja for chaos normalization
    let divine_rate = 0.0;
    let result = state.trade_client.lookup_gem(&gem, &variant, divine_rate).await
        .map_err(|e| {
            app_log(&app, format!("Trade error: {}", e));
            e
        })?;

    app_log(
        &app,
        format!(
            "Trade result: {} total, {} listings, floor={:.1} {}",
            result.total,
            result.listings.len(),
            if result.listings.is_empty() { 0.0 } else { result.listings[0].price },
            if result.listings.is_empty() { "" } else { &result.listings[0].currency },
        ),
    );

    Ok(result)
}

#[tauri::command]
fn get_logs(state: tauri::State<AppState>) -> Vec<String> {
    state.logs.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
async fn send_test_gems(app: AppHandle) -> Result<String, String> {
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

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| {
            let msg = format!("HTTP client error: {}", e);
            app_log(&app, msg.clone());
            msg
        })?;

    let res = client
        .post(&url)
        .json(&serde_json::json!({
            "pair": pair,
            "gems": gems,
            "variant": "20/20"
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

async fn send_gems_to_server(app: &AppHandle, gems: Vec<String>) {
    let state = app.state::<AppState>();
    let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let url = format!("{}/api/desktop/gems", server);

    app_log(app, format!("Sending {} gems to server", gems.len()));

    let client = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            app_log(app, format!("HTTP client error: {}", e));
            return;
        }
    };

    match client
        .post(&url)
        .json(&serde_json::json!({
            "pair": pair,
            "gems": gems,
            "variant": "20/20"
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
async fn test_ocr_on_image(path: String, app: AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    app_log(&app, format!("Testing OCR on: {}", path));

    let img = image::open(&path).map_err(|e| format!("Failed to open image: {}", e))?;
    app_log(&app, format!("Image loaded: {}x{}", img.width(), img.height()));

    let processed = capture::preprocess_for_ocr(&img);
    app_log(&app, format!("Preprocessed: {}x{}", processed.width(), processed.height()));

    let lines = ocr::recognize_text(&processed).map_err(|e| {
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
    let gem_names = fetch_gem_names(app, &server).await;
    let matcher = gem_matcher::GemMatcher::new(gem_names);

    let mut best_match: Option<gem_matcher::GemMatch> = None;
    for candidate in &candidates {
        if let Some(m) = matcher.match_gem(candidate) {
            if best_match.as_ref().map_or(true, |b| m.score > b.score) {
                best_match = Some(m);
            }
        }
    }

    if let Some(m) = best_match {
        let result = format!("Matched: {} (score: {:.2})", m.name, m.score);
        app_log(&app, result.clone());

        // Send to server
        let mut gems = state.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
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

/// Capture loop: periodically captures the screen, runs OCR for gem names
/// AND font panel craft options. Tracks full font session for statistics.
/// Runs until the lab state leaves PickingGems.
async fn run_capture_loop(app: &AppHandle) {
    let state = app.state::<AppState>();
    app_log(app, "Capture loop started".to_string());

    let mut seen_gems: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Load gem names for matching — try server first, fall back to empty
    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let gem_names = fetch_gem_names(app, &server).await;
    let matcher = gem_matcher::GemMatcher::new(gem_names.clone());
    app_log(app, format!("Loaded {} gem names for matching", gem_names.len()));

    // Error throttling — log capture/OCR errors every 20 iterations (~10s) to avoid spam
    let mut loop_count = 0u32;

    // Font panel tracking
    let mut font_miss_count = 0u32;
    let mut session_rounds: Vec<serde_json::Value> = Vec::new();
    let mut current_round_options: Option<Vec<font_parser::CraftOption>> = None;
    let mut current_round_gems: Vec<String> = Vec::new();
    let mut last_crafts_remaining: Option<i32> = None;

    loop {
        // Check if we're still in PickingGems state
        {
            let current = state.lab_state.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if current != lab_state::LabState::PickingGems {
                app_log(app, "Capture loop stopped (state changed)".to_string());
                break;
            }
        }

        loop_count += 1;

        // --- Region 1: Gem tooltip OCR ---
        let gem_region = state.gem_region.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match capture::capture_region(gem_region.x.max(0) as u32, gem_region.y.max(0) as u32, gem_region.w, gem_region.h) {
            Err(e) => {
                if loop_count % 20 == 1 {
                    app_log(app, format!("Gem capture failed: {}", e));
                }
            }
            Ok(img) => {
            let processed = capture::preprocess_for_ocr(&img);
            match ocr::recognize_text(&processed) {
                Err(e) => {
                    if loop_count % 20 == 1 {
                        app_log(app, format!("Gem OCR failed: {}", e));
                    }
                }
                Ok(lines) => {
                let candidates = ocr::extract_gem_candidates(&lines);
                let mut best: Option<gem_matcher::GemMatch> = None;
                for candidate in &candidates {
                    if let Some(m) = matcher.match_gem(candidate) {
                        if best.as_ref().map_or(true, |b| m.score > b.score) {
                            best = Some(m);
                        }
                    }
                }
                if let Some(gem_match) = best {
                    if !seen_gems.contains(&gem_match.name) {
                        seen_gems.insert(gem_match.name.clone());
                        current_round_gems.push(gem_match.name.clone());
                        app_log(app, format!(
                            "Gem detected: {} (score: {:.2}) [{}/3]",
                            gem_match.name, gem_match.score, current_round_gems.len()
                        ));

                        let mut gems = state.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
                        gems.push(gem_match.name.clone());
                        if let Err(e) = app.emit("gem-detected", &gem_match.name) { log::warn!("emit gem-detected failed: {}", e); }
                        emit_status(app);

                        let all_gems = gems.clone();
                        drop(gems);
                        let app_clone = app.clone();
                        tauri::async_runtime::spawn(async move {
                            send_gems_to_server(&app_clone, all_gems).await;
                        });
                    }
                }
            } // Ok(lines)
            } // match ocr
            } // Ok(img)
        } // match capture region 1

        // --- Region 2: Font panel OCR ---
        let font_region = state.font_region.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match capture::capture_region(font_region.x.max(0) as u32, font_region.y.max(0) as u32, font_region.w, font_region.h) {
            Err(e) => {
                if loop_count % 20 == 1 {
                    app_log(app, format!("Font capture failed: {}", e));
                }
            }
            Ok(img) => {
            let processed = capture::preprocess_for_ocr(&img);
            match ocr::recognize_text(&processed) {
                Err(e) => {
                    if loop_count % 20 == 1 {
                        app_log(app, format!("Font OCR failed: {}", e));
                    }
                }
                Ok(lines) => {
                let panel = font_parser::parse_font_panel(&lines);

                if panel.font_active {
                    font_miss_count = 0;

                    // Store current round options (first time we see them this round)
                    if current_round_options.is_none() && !panel.options.is_empty() {
                        app_log(app, format!(
                            "Font panel: {} options detected{}",
                            panel.options.len(),
                            if panel.jackpot_detected { " *** JACKPOT! ***" } else { "" }
                        ));
                        for opt in &panel.options {
                            app_log(app, format!("  - {} {}", opt.option_type,
                                opt.value.map(|v| format!("({})", v)).unwrap_or_default()));
                        }
                        current_round_options = Some(panel.options.clone());
                        last_crafts_remaining = panel.crafts_remaining;

                        if panel.jackpot_detected {
                            if let Err(e) = app.emit("font-jackpot", true) { log::warn!("emit font-jackpot failed: {}", e); }
                        }
                    }
                } else {
                    font_miss_count += 1;

                    // If we had options and now don't → round completed (user used a craft)
                    if current_round_options.is_some() {
                        // Save the completed round
                        let round = serde_json::json!({
                            "craft_options": current_round_options.take().unwrap(),
                            "gems_offered": if current_round_gems.is_empty() { serde_json::Value::Null } else { serde_json::json!(current_round_gems.clone()) },
                            "gem_picked": current_round_gems.last().cloned(),
                            "crafts_remaining": last_crafts_remaining,
                        });
                        session_rounds.push(round);
                        app_log(app, format!("Font round {} complete ({} gems captured)", session_rounds.len(), current_round_gems.len()));

                        // Reset for next round
                        current_round_gems.clear();
                        seen_gems.clear();
                    }

                    // 3 consecutive misses → font is done
                    if font_miss_count >= 3 && !session_rounds.is_empty() {
                        app_log(app, format!("Font session complete: {} rounds", session_rounds.len()));

                        // Send session to server
                        let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        let session_data = serde_json::json!({
                            "lab_type": "Unknown", // TODO: detect from Client.txt
                            "total_crafts": session_rounds.len(),
                            "variant": "20/20",
                            "device_id": "desktop",
                            "pair_code": pair,
                            "rounds": session_rounds.clone(),
                        });

                        let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        let app_clone = app.clone();
                        let data = session_data.clone();
                        tauri::async_runtime::spawn(async move {
                            send_font_session(&app_clone, &server, data).await;
                        });

                        session_rounds.clear();
                    }
                }
            } // Ok(lines)
            } // match ocr
            } // Ok(img)
        } // match capture region 2

        // Capture every 500ms
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    // If session has unsent rounds on exit, send them
    if !session_rounds.is_empty() {
        app_log(app, format!("Sending {} unsent font rounds on exit", session_rounds.len()));
        let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let session_data = serde_json::json!({
            "lab_type": "Unknown",
            "total_crafts": session_rounds.len(),
            "variant": "20/20",
            "device_id": "desktop",
            "pair_code": pair,
            "rounds": session_rounds,
        });
        send_font_session(app, &server, session_data).await;
    }
}

/// Send a completed font session to the server.
async fn send_font_session(app: &AppHandle, server_url: &str, data: serde_json::Value) {
    let url = format!("{}/api/desktop/font-session", server_url);

    let client = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            app_log(app, format!("HTTP client error: {}", e));
            return;
        }
    };

    match client.post(&url).json(&data).send().await {
        Ok(res) if res.status().is_success() => {
            app_log(app, "Font session sent successfully".to_string());
        }
        Ok(res) => {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            app_log(app, format!("Font session rejected: {} — {}", status, &body[..body.len().min(200)]));
        }
        Err(e) => {
            app_log(app, format!("Font session send failed: {}", e));
        }
    }
}

/// Fetch gem names from the server API for fuzzy matching.
async fn fetch_gem_names(app: &AppHandle, server_url: &str) -> Vec<String> {
    let url = format!("{}/api/analysis/gems/names?q=of+&limit=500", server_url);
    let client = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            app_log(app, format!("Gem names: HTTP client error: {}", e));
            return Vec::new();
        }
    };

    match client.get(&url).send().await {
        Ok(res) if res.status().is_success() => {
            match res.json::<serde_json::Value>().await {
                Ok(body) => {
                    if let Some(names) = body.get("names").and_then(|n| n.as_array()) {
                        return names
                            .iter()
                            .filter_map(|n| n.as_str().map(String::from))
                            .collect();
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

fn spawn_log_watcher(app: AppHandle) {
    let state = app.state::<AppState>();
    let client_txt = state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
    app_log(&app, format!("Starting log watcher: {}", client_txt));

    // Create cancel channel and store the sender
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    *state.watcher_cancel.lock().unwrap_or_else(|e| e.into_inner()) = Some(cancel_tx);

    tauri::async_runtime::spawn(async move {
        let watcher = log_watcher::LogWatcher::new(&client_txt);
        let mut rx = match watcher.watch().await {
            Ok(rx) => rx,
            Err(e) => {
                app_log(&app, format!("Log watcher failed to start: {}", e));
                emit_status(&app);
                return;
            }
        };

        app_log(&app, "Log watcher active".to_string());
        emit_status(&app);

        let mut state_machine = lab_state::LabStateMachine::new();
        let mut detected_gems: Vec<String> = Vec::new();
        let matcher = gem_matcher::GemMatcher::new(vec![]);

        loop {
            tokio::select! {
                _ = cancel_rx.changed() => {
                    app_log(&app, "Log watcher cancelled (path changed)".to_string());
                    break;
                }
                line = rx.recv() => {
                    let line = match line {
                        Some(l) => l,
                        None => break,
                    };
                    let state = app.state::<AppState>();
                    let preview = if line.len() > 60 { &line[..60] } else { &line };
                    app_log(&app, format!("Log: {}", preview));

                    if let Some(event) = state_machine.process_line(&line) {
                        let state = app.state::<AppState>();
                        match &event {
                            lab_state::LabEvent::FontOpened => {
                                app_log(&app, "Font opened! Starting screen reader.".to_string());
                                *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) =
                                    lab_state::LabState::FontReady;
                                detected_gems.clear();
                                *state.detected_gems.lock().unwrap_or_else(|e| e.into_inner()) =
                                    Vec::new();
                                emit_status(&app);
                                state_machine.start_picking();
                                *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) =
                                    lab_state::LabState::PickingGems;

                                let app_capture = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    run_capture_loop(&app_capture).await;
                                });
                            }
                            lab_state::LabEvent::ZoneChanged { area } => {
                                app_log(&app, format!("Zone changed: {} — stopping", area));
                                *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) =
                                    lab_state::LabState::Idle;

                                if !detected_gems.is_empty() {
                                    let gems = detected_gems.clone();
                                    let app_clone = app.clone();
                                    tauri::async_runtime::spawn(async move {
                                        send_gems_to_server(&app_clone, gems).await;
                                    });
                                    detected_gems.clear();
                                }

                                emit_status(&app);
                            }
                            lab_state::LabEvent::FontClosed => {
                                app_log(&app, "Font closed".to_string());
                                *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) =
                                    lab_state::LabState::Idle;
                                emit_status(&app);
                            }
                        }
                    }
                }
            }
        }

        app_log(&app, "Log watcher stopped".to_string());
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    let pair_code = generate_pair_code();
    log::info!("Pair code: {}", pair_code);

    let app_state = AppState {
        pair_code: Mutex::new(pair_code),
        client_txt_path: Mutex::new(String::from(
            r"C:\Program Files (x86)\Grinding Gear Games\Path of Exile\logs\Client.txt",
        )),
        server_url: Mutex::new(String::from("https://poe.softsolution.pro")),
        detected_gems: Mutex::new(Vec::new()),
        lab_state: Mutex::new(lab_state::LabState::Idle),
        logs: Mutex::new(Vec::new()),
        gem_region: Mutex::new(CaptureRegion::default()),
        font_region: Mutex::new(CaptureRegion::default_font_panel()),
        trade_client: trade::TradeApiClient::new("Mirage"),
        watcher_cancel: Mutex::new(None),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_pair_code,
            regenerate_pair_code,
            set_client_txt_path,
            reset_client_txt_path,
            set_server_url,
            get_logs,
            get_gem_region,
            set_gem_region,
            get_font_region,
            set_font_region,
            capture_mouse_position,
            start_scanning,
            stop_scanning,
            trade_lookup,
            send_test_gems,
            test_ocr_on_image,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // Load persisted settings and apply to state
            let saved = settings::load(&handle);
            let state = handle.state::<AppState>();
            settings::apply_to_state(&saved, &state);

            // Restore window position/size from saved settings
            if let Some(ref win_settings) = saved.window {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.set_position(tauri::PhysicalPosition::new(win_settings.x, win_settings.y));
                    let _ = win.set_size(tauri::PhysicalSize::new(win_settings.width, win_settings.height));
                    if win_settings.maximized {
                        let _ = win.maximize();
                    }
                }
            }

            spawn_log_watcher(handle.clone());
            emit_status(&handle);
            emit_logs(&handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Save window position/size on close
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if window.label() == "main" {
                    let app = window.app_handle();
                    let is_maximized = window.is_maximized().unwrap_or(false);
                    // Only save position/size if not maximized (restore to normal position)
                    let win_settings = if is_maximized {
                        // Save maximized flag but keep last known normal position
                        let state = app.state::<AppState>();
                        let current = settings::from_state(&state);
                        settings::WindowSettings {
                            x: current.window.as_ref().map_or(100, |w| w.x),
                            y: current.window.as_ref().map_or(100, |w| w.y),
                            width: current.window.as_ref().map_or(1024, |w| w.width),
                            height: current.window.as_ref().map_or(768, |w| w.height),
                            maximized: true,
                        }
                    } else {
                        let pos = window.outer_position().unwrap_or(tauri::PhysicalPosition::new(100, 100));
                        let size = window.outer_size().unwrap_or(tauri::PhysicalSize::new(1024, 768));
                        settings::WindowSettings {
                            x: pos.x,
                            y: pos.y,
                            width: size.width,
                            height: size.height,
                            maximized: false,
                        }
                    };
                    let state = app.state::<AppState>();
                    let mut s = settings::from_state(&state);
                    s.window = Some(win_settings);
                    settings::save(app, &s);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
