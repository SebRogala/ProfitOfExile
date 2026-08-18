mod capture;
mod fingerprint;
mod font_ledger;
mod font_parser;
mod font_session;
mod gem_matcher;
mod lab_navigation;
mod lab_state;
mod log_watcher;
mod mercenary;
mod modules;
mod ocr;
mod settings;
mod ssot;
mod trade;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
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
    pub app_version: String,
    pub pair_code: String,
    pub detected_gems: Vec<String>,
    pub client_txt_path: String,
    pub client_txt_exists: bool,
    pub server_url: String,
    pub gem_region: CaptureRegion,
    pub font_region: CaptureRegion,
    pub sidebar_open: bool,
    pub game_focused: bool,
    pub trade_stale_warn_secs: u32,
    pub trade_stale_critical_secs: u32,
    pub trade_auto_refresh_secs: u32,
    pub auto_trade_enabled: bool,
    pub device_id: String,
    /// Sealed rounds accumulated in the current font session. Drives the
    /// discard affordance: zero means there is nothing to throw away.
    pub font_session_rounds: usize,
    /// Set when a scan thread had to fall back off the en-US recognizer. The
    /// whole OCR pipeline (gem names, font options, merc rows) misreads on a
    /// profile-language recognizer, and the only other trace is one LOGS line
    /// that scrolls away, so it is surfaced in Settings as a standing warning.
    pub ocr_language_warning: Option<String>,
}

pub use font_session::{FontRound, FontSessionData};

pub struct AppState {
    /// Hardware-based device fingerprint — computed once at startup, immutable.
    /// String is Sync so no Mutex needed.
    pub device_id: String,
    pub pair_code: Mutex<String>,
    pub client_txt_path: Mutex<String>,
    pub server_url: Mutex<String>,
    pub detected_gems: Mutex<Vec<String>>,
    pub lab_state: Mutex<lab_state::LabState>,
    pub logs: Mutex<Vec<String>>,
    pub gem_region: Mutex<CaptureRegion>,
    pub font_region: Mutex<CaptureRegion>,
    pub sidebar_open: Mutex<bool>,
    pub game_focused: Mutex<bool>,
    pub trade_client: trade::TradeApiClient,
    /// General-purpose HTTP client for server communication (separate from trade_client
    /// which has GGG-specific User-Agent/headers).
    pub server_http: reqwest::Client,
    /// Cancel signal for the current log watcher. Send () to stop it.
    pub watcher_cancel: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    /// Cached comparator overlay data (results + trade data) shared between windows.
    pub comparator_data: Mutex<serde_json::Value>,
    /// Stop signal for the overlay mouse hook thread.
    pub overlay_hook_stop: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    pub focus_poller_stop: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    pub debug_mode: Mutex<bool>,
    /// Trade staleness thresholds (seconds) — configurable from settings.
    pub trade_stale_warn_secs: Mutex<u32>,
    pub trade_stale_critical_secs: Mutex<u32>,
    pub trade_auto_refresh_secs: Mutex<u32>,
    pub auto_trade_enabled: Mutex<bool>,
    /// Generation counter for gem OCR scans. Incremented on each start trigger
    /// (FontOpened, manual scan). The capture loop checks this every iteration —
    /// if it doesn't match the generation it was spawned with, it exits.
    pub gem_scan_generation: AtomicU64,
    /// Generation counter for font panel OCR scans.
    pub font_scan_generation: AtomicU64,
    /// Generation of the font scan loop that is currently running, or 0 when
    /// none is. `FontOpened` re-arms the scan when it reads 0 — after a portal
    /// trip no further `LabFinished` fires, so the event is the only chance to
    /// bring the panel OCR back. Written by `spawn_font_scan` (its own
    /// generation), by the loop on exit (compare-exchange, so a stale loop
    /// cannot clear its replacement's token) and by anything that bumps
    /// `font_scan_generation` without starting a replacement.
    pub font_scan_live_gen: AtomicU64,
    /// Monotonic count of `FontOpened` events. The craft ledger gates every
    /// count change on it: the panel's count cannot change without a CRAFT
    /// click, and a CRAFT click always fires this event, so a count change with
    /// no new event is a misread. Never reset — the ledger stores the value it
    /// accepted at, and a counter going backwards would re-open accepted rounds.
    pub font_opened_seq: AtomicU64,
    /// Aspirant's Trial entry count (reset on Aspirants' Plaza). Font OCR starts at 3.
    pub aspirant_trial_count: AtomicU32,
    /// Font session data — accumulated rounds, shared between font scan loop and handlers.
    pub font_session: Mutex<FontSessionData>,
    /// True when player is inside the labyrinth (between PlazaEntered and LabExited).
    /// Used by lab_navigation to determine if a non-lab area entry is a lab exit.
    pub in_lab: AtomicBool,
    pub compass_mode: Mutex<String>,
    pub compass_strategy: Mutex<String>,
    pub compass_difficulty: Mutex<String>,
    pub shrine_warn_enabled: Mutex<bool>,
    pub shrine_warn_size: Mutex<String>,
    pub shrine_warn_corner: Mutex<String>,
    pub shrine_warn_on_take: Mutex<String>,
    pub lab_overlays_enabled: Mutex<bool>,
    /// Lab mode: "Normal" or "Dedication". Controls OCR vocabulary, font session
    /// metadata, and comparator behaviour. Persisted to settings.
    pub lab_mode: Mutex<String>,
    pub autoclear_minutes: Mutex<u32>,
    pub dedication_pool: Mutex<String>,
    /// Dedication corrupted variant: "21/23" or "21/20". Selects which corrupted
    /// market the Dedication views read. Persisted to settings.
    pub dedication_variant: Mutex<String>,
    pub normal_variant: Mutex<String>,
    pub show_low_confidence: Mutex<bool>,
    /// Schema-less UI view preferences (sort mode, colour filter, row limit…).
    /// The frontend owns the keys; Rust only stores and persists the map. A
    /// typed Settings field is warranted only when Rust itself reads the value.
    pub ui_prefs: Mutex<std::collections::HashMap<String, String>>,
    /// App-wide cross-window state SSOT (POE-128). Rust-owned; overlays read it
    /// by polling the `get_ssot` command. See src/ssot.rs.
    pub ssot: Mutex<ssot::AppSsotSnapshot>,
    /// Per-module enabled flags — the SINGLE owner, always holding the
    /// **effective** state (registry defaults overlaid with the persisted
    /// delta, written by `settings::apply_to_state`). See src/modules.rs.
    pub modules_enabled: Mutex<std::collections::HashMap<String, bool>>,
    /// Currently running modules, keyed by module id. Acquired BEFORE
    /// `modules_enabled` — never the inverse (lock order, see src/modules.rs).
    pub module_handles: Mutex<std::collections::HashMap<String, modules::ModuleHandle>>,
    /// Latched at the top of the main-window `CloseRequested` handler. A
    /// distinct `reconcile` input: while set, reconcile stops everything and
    /// starts nothing, so a racing `set_module_enabled` cannot respawn.
    pub modules_shutting_down: AtomicBool,
    /// Merc OCR capture state (POE-165) — the owner of the `mercenary` SSOT
    /// slice. Written by the capture loop, projected read-only into every
    /// snapshot by `ssot::build_snapshot`. See src/mercenary/mod.rs.
    pub mercenary: Mutex<mercenary::MercenarySlice>,
    /// Learned support-icon templates (POE-165 D4). Shared because two owners
    /// need it: the capture loop matches and learns through it, and the
    /// `merc_forget_template` / `merc_reset_templates` commands are the
    /// un-poison path a user reaches for while that loop is running. Acquired
    /// alone, never inside a module lock (lock order — see src/modules.rs).
    pub merc_templates: Mutex<mercenary::icons::TemplateStore>,
    /// Bumped whenever the template store is EDITED by the user
    /// (`merc_forget_template` / `merc_reset_templates`). The capture loop
    /// watches it to drop the confirmations it is still re-applying from
    /// memory — a forgotten template that keeps being re-applied is the
    /// un-poison button not working. An atomic, not a Mutex: it is read on
    /// every detect tick and never read together with the store.
    pub merc_template_generation: AtomicU64,
}

/// Build the full AppStatus from current state. Used by get_status command and event emitting.
fn build_status(state: &AppState) -> AppStatus {
    let client_txt_path = state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let client_txt_exists = std::path::Path::new(&client_txt_path).exists();
    AppStatus {
        state: format!("{:?}", *state.lab_state.lock().unwrap_or_else(|e| e.into_inner())),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        pair_code: state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        detected_gems: state.detected_gems.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        client_txt_path,
        client_txt_exists,
        server_url: state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        gem_region: state.gem_region.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        font_region: state.font_region.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        sidebar_open: *state.sidebar_open.lock().unwrap_or_else(|e| e.into_inner()),
        game_focused: *state.game_focused.lock().unwrap_or_else(|e| e.into_inner()),
        trade_stale_warn_secs: *state.trade_stale_warn_secs.lock().unwrap_or_else(|e| e.into_inner()),
        trade_stale_critical_secs: *state.trade_stale_critical_secs.lock().unwrap_or_else(|e| e.into_inner()),
        trade_auto_refresh_secs: *state.trade_auto_refresh_secs.lock().unwrap_or_else(|e| e.into_inner()),
        auto_trade_enabled: *state.auto_trade_enabled.lock().unwrap_or_else(|e| e.into_inner()),
        device_id: state.device_id.clone(),
        font_session_rounds: state.font_session.lock().unwrap_or_else(|e| e.into_inner()).rounds.len(),
        ocr_language_warning: ocr_warning_field(),
    }
}

/// Save current settings to disk. Call after any persistent state change.
/// Preserves window and overlay positions from the existing file (only updated by their own save paths).
fn persist_settings(app: &AppHandle) {
    let state = app.state::<AppState>();
    let existing = settings::load(app);
    let mut s = settings::from_state(&state);
    // Preserve fields that are saved separately (not via AppState):
    // window position (saved on close), overlay positions (saved via set_*_overlay_settings).
    // This is DRY — from_state handles AppState fields, persist_overlay_settings handles the rest.
    persist_overlay_settings(&existing, &mut s);
    settings::save(app, &s);
}

fn persist_overlay_settings(existing: &settings::Settings, target: &mut settings::Settings) {
    settings::persist_overlay_settings(existing, target);
}

/// Emit the full app status to all frontend listeners.
fn emit_status(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Err(e) = app.emit("status-changed", build_status(&state)) { log::warn!("emit status-changed failed: {}", e); }
}

/// The `ocr_language_warning` field of `AppStatus`, as its own seam.
///
/// `build_status` needs a live `AppState` to call, so this is the part of the
/// payload a unit test can reach. Reads the cached warning and deliberately NOT
/// `ocr::engine_report()`: `build_status` runs on every command and event
/// thread, and resolving an engine there would construct an extra one — and
/// re-log the fallback — per thread.
fn ocr_warning_field() -> Option<String> {
    ocr::language_warning()
}

thread_local! {
    /// Whether this thread has already pushed the OCR language warning to the
    /// UI. Per-thread rather than global so each scan thread emits at most once
    /// while none of them depends on another having run.
    static OCR_WARNING_REPORTED: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

/// Resolve this thread's OCR recognizer, log which one it is, and push a cached
/// language warning to the UI once per thread.
///
/// Every scan thread calls this on start. The emit cannot be left to the spawn
/// site: `spawn_font_scan` emits before the thread exists, so at that point no
/// engine has resolved and `ocr_language_warning` is still `None`.
///
/// The trigger is the warning being PRESENT, not this thread having been the one
/// to cache it. Any engine resolution caches it — including one from a debug
/// command (`test_ocr_on_image`, the merc debug dump), which has no status to
/// emit — so keying on the cache transition would drop the notification whenever
/// a debug path resolved first.
fn report_ocr_engine(app: &AppHandle) {
    app_log(app, ocr::engine_report());
    // `replace` returns the previous value, so the first call through here on a
    // thread with a warning cached is the only one that emits.
    if ocr::language_warning().is_some() && !OCR_WARNING_REPORTED.with(|c| c.replace(true)) {
        emit_status(app);
    }
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

/// Returns the first 8 characters of the device fingerprint (for display in the identify dialog).
#[tauri::command]
fn get_device_id(state: tauri::State<AppState>) -> String {
    state.device_id.chars().take(8).collect()
}

#[tauri::command]
fn regenerate_pair_code(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let new_code = generate_pair_code();
    *state.pair_code.lock().unwrap_or_else(|e| e.into_inner()) = new_code.clone();
    emit_status(&app);
    new_code
}

/// Detect Client.txt path using multiple strategies (most reliable first).
/// 1. Running PoE process → derive logs path from executable location
/// 2. Hardcoded common paths (GGG standalone, Steam default, Epic Games)
fn detect_client_txt_path() -> String {
    // Strategy 1: Find running PoE process and derive logs path
    if let Some(path) = detect_from_running_process() {
        log::info!("Client.txt detected from running process: {}", path);
        return path;
    }

    // Strategy 2: Check common install paths
    let common_paths = [
        r"C:\Program Files (x86)\Grinding Gear Games\Path of Exile\logs\Client.txt",
        r"C:\Program Files (x86)\Steam\steamapps\common\Path of Exile\logs\Client.txt",
        r"C:\Program Files\Epic Games\PathOfExile\logs\Client.txt",
        // 32-bit GGG path
        r"C:\Program Files\Grinding Gear Games\Path of Exile\logs\Client.txt",
    ];

    for path in &common_paths {
        if std::path::Path::new(path).exists() {
            log::info!("Client.txt found at common path: {}", path);
            return path.to_string();
        }
    }

    // Strategy 3: Try Steam library folders from registry
    if let Some(path) = detect_from_steam_libraries() {
        log::info!("Client.txt detected from Steam library: {}", path);
        return path;
    }

    // Default fallback — user will see the warning
    log::warn!("Client.txt not found in any known location");
    common_paths[0].to_string()
}

/// Find Client.txt by detecting a running PathOfExile process.
fn detect_from_running_process() -> Option<String> {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    for process in sys.processes().values() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name.contains("pathofexile") && name.ends_with(".exe") {
            if let Some(exe_path) = process.exe() {
                let game_dir = exe_path.parent()?;
                let client_txt = game_dir.join("logs").join("Client.txt");
                if client_txt.exists() {
                    return Some(client_txt.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

/// Find Client.txt in Steam library folders by reading Steam's config.
fn detect_from_steam_libraries() -> Option<String> {
    #[cfg(windows)]
    {
        // Read Steam install path from registry
        use std::process::Command;
        let output = Command::new("reg")
            .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let steam_path = stdout.lines()
            .find(|l| l.contains("SteamPath"))?
            .split("REG_SZ")
            .nth(1)?
            .trim()
            .to_string();

        // Parse libraryfolders.vdf to find all library paths
        let vdf_path = format!(r"{}\steamapps\libraryfolders.vdf", steam_path);
        let vdf_content = std::fs::read_to_string(&vdf_path).ok()?;

        // Simple VDF parser: look for "path" values
        for line in vdf_content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"path\"") {
                let path = trimmed
                    .split('"')
                    .nth(3)?
                    .replace("\\\\", "\\");
                let client_txt = format!(
                    r"{}\steamapps\common\Path of Exile\logs\Client.txt",
                    path
                );
                if std::path::Path::new(&client_txt).exists() {
                    return Some(client_txt);
                }
            }
        }
    }
    None
}

#[tauri::command]
fn set_client_txt_path(path: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = path;
    persist_settings(&app);
    emit_status(&app);
    restart_log_watcher(app);
}

/// Nuclear reset: delete settings file and re-initialize with defaults.
#[tauri::command]
fn reset_all_settings(app: AppHandle) {
    // Delete the settings file
    if let Some(path) = settings::settings_path_pub(&app) {
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                app_log(&app, format!("Failed to delete settings: {}", e));
            } else {
                app_log(&app, format!("Settings file deleted: {:?}", path));
            }
        }
    }
    // Re-apply defaults to AppState
    let defaults = settings::Settings::default();
    let state = app.state::<AppState>();
    settings::apply_to_state(&defaults, &state);
    // Re-detect Client.txt
    let detected = detect_client_txt_path();
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = detected;
    // Save fresh settings
    persist_settings(&app);
    // `apply_to_state` above rewrote the module owner map to registry
    // defaults, so bring the running set back in line with it.
    modules::apply_reconcile(&app);
    // Nudge after the reconcile, exactly as `set_module_enabled` does: without
    // it every window keeps polling the pre-reset `modules` slice until the
    // next unrelated emit.
    ssot::emit_ssot(&app);
    // Tell the windows. Lab mode and both markets live in AppState and are
    // mirrored by LabPage, which reads them once at mount — without this the
    // mirror keeps showing the pre-reset market while every scan is stamped
    // with the reset one, and the paired web view follows Rust, not the mirror.
    if let Err(e) = app.emit("settings-reset", ()) {
        log::warn!("emit settings-reset failed: {}", e);
    }
    app_log(&app, "All settings reset to defaults".to_string());
    emit_status(&app);
    restart_log_watcher(app);
}

#[tauri::command]
fn reset_client_txt_path(app: AppHandle) {
    let state = app.state::<AppState>();
    let detected = detect_client_txt_path();
    app_log(&app, format!("Auto-detect Client.txt: {}", detected));
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = detected;
    persist_settings(&app);
    emit_status(&app);
    restart_log_watcher(app);
}

#[tauri::command]
async fn browse_client_txt(app: AppHandle) -> Result<String, String> {
    let file = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("Text files", &["txt"])
            .set_title("Select Path of Exile Client.txt")
            .pick_file()
    }).await.map_err(|e| format!("dialog thread error: {}", e))?;

    match file {
        Some(path) => {
            let path_str = path.to_string_lossy().to_string();
            let state = app.state::<AppState>();
            *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = path_str.clone();
            persist_settings(&app);
            emit_status(&app);
            restart_log_watcher(app);
            Ok(path_str)
        }
        None => Err("No file selected".to_string()),
    }
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
fn set_sidebar_open(open: bool, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.sidebar_open.lock().unwrap_or_else(|e| e.into_inner()) = open;
    persist_settings(&app);
    emit_status(&app);
}

#[tauri::command]
fn set_trade_staleness_settings(warn_secs: u32, critical_secs: u32, auto_refresh_secs: u32, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.trade_stale_warn_secs.lock().unwrap_or_else(|e| e.into_inner()) = warn_secs;
    *state.trade_stale_critical_secs.lock().unwrap_or_else(|e| e.into_inner()) = critical_secs;
    *state.trade_auto_refresh_secs.lock().unwrap_or_else(|e| e.into_inner()) = auto_refresh_secs;
    persist_settings(&app);
    emit_status(&app);
}

#[tauri::command]
fn set_auto_trade(enabled: bool, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.auto_trade_enabled.lock().unwrap_or_else(|e| e.into_inner()) = enabled;
    persist_settings(&app);
    emit_status(&app);
}

#[tauri::command]
fn get_compass_settings(app: AppHandle) -> serde_json::Value {
    let state = app.state::<AppState>();
    let mode = state.compass_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let strategy = state.compass_strategy.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let difficulty = state.compass_difficulty.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let shrine_enabled = *state.shrine_warn_enabled.lock().unwrap_or_else(|e| e.into_inner());
    let shrine_size = state.shrine_warn_size.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let shrine_corner = state.shrine_warn_corner.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let shrine_on_take = state.shrine_warn_on_take.lock().unwrap_or_else(|e| e.into_inner()).clone();
    serde_json::json!({
        "mode": mode, "strategy": strategy, "difficulty": difficulty,
        "shrine_warn_enabled": shrine_enabled,
        "shrine_warn_size": shrine_size,
        "shrine_warn_corner": shrine_corner,
        "shrine_warn_on_take": shrine_on_take,
    })
}

#[tauri::command]
fn set_compass_mode(mode: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.compass_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode;
    persist_settings(&app);
}

#[tauri::command]
fn set_compass_strategy(strategy: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.compass_strategy.lock().unwrap_or_else(|e| e.into_inner()) = strategy;
    persist_settings(&app);
}

#[tauri::command]
fn set_compass_difficulty(difficulty: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.compass_difficulty.lock().unwrap_or_else(|e| e.into_inner()) = difficulty;
    persist_settings(&app);
}

#[tauri::command]
fn set_shrine_warn(enabled: bool, size: String, corner: String, on_take: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.shrine_warn_enabled.lock().unwrap_or_else(|e| e.into_inner()) = enabled;
    *state.shrine_warn_size.lock().unwrap_or_else(|e| e.into_inner()) = size;
    *state.shrine_warn_corner.lock().unwrap_or_else(|e| e.into_inner()) = corner;
    *state.shrine_warn_on_take.lock().unwrap_or_else(|e| e.into_inner()) = on_take;
    persist_settings(&app);
}

/// Returns recent lab nav events for overlays to catch up on mount.
/// Reads last 32KB of Client.txt and replays from last PlazaEntered/LabExited.
#[tauri::command]
fn get_lab_catchup(app: AppHandle) -> serde_json::Value {
    let state = app.state::<AppState>();
    let client_txt = state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let (events, in_lab) = lab_navigation::replay_recent_log(
        std::path::Path::new(&client_txt),
    );
    serde_json::json!({ "events": events, "in_lab": in_lab })
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

/// Read the cursor position. READ ONLY — nothing in this app moves the cursor
/// or sends input; injecting input into the PoE client is against GGG's ToS.
/// Shared with the merc hover-confirm tick (POE-165 D5), which calls it
/// directly rather than duplicating the `cfg` blocks.
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

/// Start a gem OCR scan. Used by both FontOpened and manual trigger.
/// - Clears comparator (gems-cleared)
/// - Bumps generation counter (cancels any running scan)
/// - Sets state to PickingGems
/// - Spawns a new capture loop with the current generation
fn spawn_gem_scan(app: &AppHandle, source: &str) {
    let state = app.state::<AppState>();

    // Bump generation — any running capture loop will see the mismatch and exit.
    let gen = state.gem_scan_generation.fetch_add(1, Ordering::SeqCst) + 1;

    // Clear frontend comparator.
    *state.detected_gems.lock().unwrap_or_else(|e| e.into_inner()) = Vec::new();
    if let Err(e) = app.emit("gems-cleared", ()) { log::warn!("emit gems-cleared failed: {}", e); }

    // Set state to PickingGems (capture loop checks this + generation).
    *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) = lab_state::LabState::PickingGems;
    emit_status(app);

    app_log(app, format!("Gem scan started ({}, gen={})", source, gen));

    let app_capture = app.clone();
    // Capture loop uses blocking Windows COM APIs (screen capture + OCR).
    // Must run on a dedicated OS thread — tokio's runtime and spawn_blocking
    // pool both cause deadlocks with apartment-threaded WinRT objects.
    std::thread::spawn(move || {
        gem_scan_loop(app_capture, gen);
    });
}

#[tauri::command]
async fn start_scanning(app: AppHandle) -> Result<(), String> {
    spawn_gem_scan(&app, "manual");
    Ok(())
}

#[tauri::command]
fn stop_scanning(app: AppHandle) {
    let state = app.state::<AppState>();
    app_log(&app, "Manual scan stopped".to_string());
    // Bump generation to cancel any running scan.
    state.gem_scan_generation.fetch_add(1, Ordering::SeqCst);
    *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) = lab_state::LabState::Idle;
    emit_status(&app);
}

/// Cancel all pending trade lookups. In-flight request completes but
/// queued lookups bail out without making GGG requests.
#[tauri::command]
fn trade_cancel(app: AppHandle) {
    let state = app.state::<AppState>();
    let remaining = state.trade_client.cancel();
    use tauri::Emitter;
    if let Err(e) = app.emit("trade-queue", trade::TradeQueueEvent::Cancelled { remaining }) {
        log::warn!("emit trade-queue Cancelled failed: {}", e);
    }
    app_log(&app, format!("Trade queue cancelled ({} pending)", remaining));
}

/// Total attempts for a trade submit that the server sheds with 503 — the
/// first send plus two retries.
///
/// The insert behind the endpoint is idempotent (`ON CONFLICT DO NOTHING` on
/// `(league, time, gem, variant)`) and the submit is already detached, so a
/// retry can duplicate work but never rows. The bound is the point: the server
/// grew a queue and a 503 precisely to shed load, and an unbounded client-side
/// retry would hand that load straight back.
const TRADE_SUBMIT_ATTEMPTS: u32 = 3;
/// Wait used when a 503 carries no parseable `Retry-After`.
const TRADE_SUBMIT_RETRY_FALLBACK: std::time::Duration = std::time::Duration::from_secs(1);
/// Ceiling on an honoured `Retry-After`. The row is a cache-enrichment nicety;
/// holding a task for minutes because a header said so is not worth it.
const TRADE_SUBMIT_RETRY_MAX: std::time::Duration = std::time::Duration::from_secs(5);

/// Leading `max` bytes of a response body for a log line.
///
/// Cuts on a character boundary: `&body[..body.len().min(max)]` panics when the
/// truncation point lands mid-codepoint, and an error body is exactly the kind
/// of response that can carry a non-ASCII byte (a proxy's HTML error page, a
/// server message quoting user text). Panicking while logging a rejection would
/// lose the rejection.
fn body_excerpt(body: &str, max: usize) -> &str {
    if body.len() <= max {
        return body;
    }
    let mut end = max;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    &body[..end]
}

/// Delay to wait before retrying a shed submit.
///
/// Honours the delta-seconds form of `Retry-After` (what the server sends),
/// clamped to `TRADE_SUBMIT_RETRY_MAX`. The HTTP-date form is not parsed — it
/// falls back like any other unusable value.
fn retry_after_delay(header: Option<&str>) -> std::time::Duration {
    header
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
        .unwrap_or(TRADE_SUBMIT_RETRY_FALLBACK)
        .min(TRADE_SUBMIT_RETRY_MAX)
}

/// Direct trade API lookup against GGG from the desktop app.
/// Each user has their own IP → own rate limits (no shared server bottleneck).
/// Accepts optional divine rate for chaos normalization of divine-priced listings.
/// When mode is "dedication", the query targets corrupted 21/23 skill gems.
#[tauri::command]
async fn trade_lookup(
    gem: String,
    variant: String,
    divine_rate: Option<f64>,
    mode: Option<String>,
    app: AppHandle,
) -> Result<trade::TradeLookupResult, String> {
    let state = app.state::<AppState>();
    let rate = divine_rate.unwrap_or(0.0);
    let dedication = mode.as_deref() == Some("dedication");
    if rate <= 0.0 {
        log::warn!("Trade lookup: divine_rate is 0 — divine-priced listings will NOT be normalized to chaos");
    }
    app_log(&app, format!("Trade lookup: {} ({}) divine_rate={:.0} dedication={}", gem, variant, rate, dedication));

    let app_for_emit = app.clone();
    let result = state.trade_client.lookup_gem_with_mode(&gem, &variant, rate, dedication, |event| {
        use tauri::Emitter;
        if let Err(e) = app_for_emit.emit("trade-queue", &event) {
            log::warn!("emit trade-queue failed: {}", e);
        }
    }).await
        .map_err(|e| {
            if e == "cancelled" {
                return e; // Don't log cancellations as errors
            }
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

    // Fire-and-forget: submit trade result to server for cache enrichment
    {
        let server_url = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let http = state.server_http.clone();
        let submit_result = result.clone();
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            let url = format!("{}/api/trade/submit", server_url);
            for attempt in 1..=TRADE_SUBMIT_ATTEMPTS {
                match http.post(&url).json(&submit_result).send().await {
                    Ok(res) if res.status().is_success() => {
                        app_log(&app_clone, format!("Trade submitted to server: {} ({})", submit_result.gem, submit_result.variant));
                        return;
                    }
                    // 503 is the server shedding a saturated persist queue, not
                    // a rejection of this row — it carries Retry-After and the
                    // insert is idempotent, so re-sending is safe.
                    Ok(res)
                        if res.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE
                            && attempt < TRADE_SUBMIT_ATTEMPTS =>
                    {
                        let delay = retry_after_delay(
                            res.headers()
                                .get(reqwest::header::RETRY_AFTER)
                                .and_then(|v| v.to_str().ok()),
                        );
                        app_log(&app_clone, format!(
                            "Trade submit shed by server (503), retrying in {:.1}s (attempt {}/{})",
                            delay.as_secs_f64(), attempt, TRADE_SUBMIT_ATTEMPTS,
                        ));
                        tokio::time::sleep(delay).await;
                    }
                    Ok(res) if res.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE => {
                        app_log(&app_clone, format!(
                            "Trade submit shed by server after {} attempts — history row lost: {} ({})",
                            TRADE_SUBMIT_ATTEMPTS, submit_result.gem, submit_result.variant,
                        ));
                        return;
                    }
                    Ok(res) => {
                        // A 400/422 loses the row AND the reason unless the body
                        // is read — the status alone cannot say which field the
                        // server objected to. Same shape as the font session
                        // rejection path below.
                        let status = res.status();
                        let body = res.text().await.unwrap_or_default();
                        app_log(&app_clone, format!(
                            "Trade submit rejected: {} — {}",
                            status,
                            body_excerpt(&body, 200),
                        ));
                        return;
                    }
                    Err(e) => {
                        app_log(&app_clone, format!("Trade submit failed: {}", e));
                        return;
                    }
                }
            }
        });
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Overlay click-through system
//
// Problem: WebView2 creates child HWNDs (Chrome_WidgetWin_0/1, Intermediate
// D3D Window) that handle hit-testing independently. Subclassing the parent
// with WM_NCHITTEST → HTTRANSPARENT does NOT work because WebView2's child
// windows intercept mouse input before the parent sees it. WebView2 also
// strips WS_EX_TRANSPARENT when creating/updating child windows.
//
// Solution:
//   1. Set WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE on the
//      overlay window — always fully click-through, game never sees it.
//   2. Install a global WH_MOUSE_LL hook.
//   3. Hook re-applies WS_EX_TRANSPARENT on every mouse event (WebView2 fix).
//   4. When a click lands in the interactive zone (rightmost N pixels),
//      buffer the coordinates and consume the click (game doesn't see it).
//   5. Message loop drains the buffer and emits Tauri `overlay-click` events.
//   6. Frontend uses elementFromPoint + data-action attributes to map clicks.
//   7. HAS_CONTENT flag gates interception — empty overlay passes clicks through.
// ---------------------------------------------------------------------------

/// Overlay click-through system for Windows/WebView2.
///
/// The overlay is ALWAYS fully click-through (WS_EX_TRANSPARENT). The game
/// never sees the overlay window — cursor and input pass through completely.
///
/// A WH_MOUSE_LL hook intercepts clicks in the interactive zone (rightmost N
/// pixels) and emits Tauri events instead. The hook consumes these clicks so
/// they don't reach the game. The overlay frontend maps click coordinates to
/// button actions.
/// Cached overlay geometry for the click-through hook, and the rule for when
/// that cache may be trusted.
///
/// A type rather than four loose statics so the trust rule is testable off
/// Windows. The rule exists entirely for the case a test cannot provoke — a
/// failed `GetWindowRect` — but every consequence of that failure lives here,
/// so this is the seam. Compiled on every platform under `test`; only Windows
/// builds instantiate it (POE-148).
#[cfg(any(windows, test))]
// Off Windows this compiles for tests only, with no hook to call `invalidate`.
#[cfg_attr(not(windows), allow(dead_code))]
mod rect_cache {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Mutex as StdMutex;

    pub struct RectCache {
        /// (left, top, right, bottom) in physical pixels.
        rect: StdMutex<(i32, i32, i32, i32)>,
        dirty: AtomicBool,
        valid: AtomicBool,
        read_failures: AtomicU32,
    }

    impl RectCache {
        pub const fn new() -> Self {
            Self {
                rect: StdMutex::new((0, 0, 0, 0)),
                // Starts dirty and untrusted: (0,0,0,0) describes no window.
                dirty: AtomicBool::new(true),
                valid: AtomicBool::new(false),
                read_failures: AtomicU32::new(0),
            }
        }

        /// Mark the cache stale so the next attempt re-reads it.
        pub fn invalidate(&self) {
            self.dirty.store(true, Ordering::Relaxed);
        }

        /// Whether a refresh is due, clearing the flag in the same operation.
        pub fn take_dirty(&self) -> bool {
            self.dirty.swap(false, Ordering::Relaxed)
        }

        /// Record a successful read.
        ///
        /// Poison-tolerant, like every reader: taking the lock with `if let Ok`
        /// dropped the update on a poisoned mutex *after* the dirty flag had
        /// already been cleared, so a single panic while holding the lock froze
        /// the rect for the rest of the session with no signal.
        pub fn store(&self, rect: (i32, i32, i32, i32)) {
            *self.rect.lock().unwrap_or_else(|e| e.into_inner()) = rect;
            self.valid.store(true, Ordering::Relaxed);
        }

        /// Record a failed read.
        ///
        /// The stored tuple still holds the *previous* geometry, so trust is
        /// withdrawn rather than the value replaced — consumers must decline,
        /// not compute against another window's rect. Staying dirty lets the
        /// cache self-heal on the next attempt that can read; the accepted
        /// trade-off is that a persistently unreadable window is retried on
        /// every mouse event, which the failure count makes visible.
        pub fn record_failure(&self) {
            self.dirty.store(true, Ordering::Relaxed);
            self.valid.store(false, Ordering::Relaxed);
            self.read_failures.fetch_add(1, Ordering::Relaxed);
        }

        /// Forget which window the cache describes, without counting a failure.
        pub fn reset_for_new_window(&self) {
            self.dirty.store(true, Ordering::Relaxed);
            self.valid.store(false, Ordering::Relaxed);
        }

        /// Whether the stored rect came from a successful read of the current
        /// window. False means every consumer must decline.
        pub fn is_valid(&self) -> bool {
            self.valid.load(Ordering::Relaxed)
        }

        pub fn get(&self) -> (i32, i32, i32, i32) {
            *self.rect.lock().unwrap_or_else(|e| e.into_inner())
        }

        /// Failures since the last drain. The hook cannot log — it must return
        /// inside `LowLevelHooksTimeout` — so its message loop drains this.
        pub fn take_failures(&self) -> u32 {
            self.read_failures.swap(0, Ordering::Relaxed)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::RectCache;

        #[test]
        fn a_fresh_cache_is_untrusted() {
            // (0,0,0,0) would hit-test as a degenerate zone at the screen
            // origin, so the cache must not be trusted before its first read.
            assert!(!RectCache::new().is_valid());
        }

        #[test]
        fn a_failed_read_withdraws_trust_while_leaving_the_stale_rect_in_place() {
            // The exact POE-148 regression: before this, a failed GetWindowRect
            // only re-set the dirty flag, and the hook went straight on to
            // hit-test and translate coordinates against the tuple below.
            let cache = RectCache::new();
            cache.store((100, 200, 400, 500));

            cache.record_failure();

            assert!(!cache.is_valid());
            assert_eq!(
                cache.get(),
                (100, 200, 400, 500),
                "the stale rect is still readable — withdrawing trust is what stops it being used"
            );
        }

        #[test]
        fn a_failed_read_leaves_the_cache_dirty_so_it_can_self_heal() {
            // Mirrors the refresh sequence: the dirty flag is consumed before
            // the read, so a successful read leaves the cache clean.
            let cache = RectCache::new();
            assert!(cache.take_dirty(), "a fresh cache is due for its first read");
            cache.store((1, 2, 3, 4));
            assert!(!cache.take_dirty(), "a successful read leaves nothing to refresh");

            cache.record_failure();

            assert!(cache.take_dirty());
        }

        #[test]
        fn a_later_successful_read_restores_trust() {
            let cache = RectCache::new();
            cache.record_failure();

            cache.store((10, 20, 30, 40));

            assert!(cache.is_valid());
            assert_eq!(cache.get(), (10, 20, 30, 40));
        }

        #[test]
        fn failed_reads_are_counted_and_drained_by_the_reporter() {
            let cache = RectCache::new();
            cache.record_failure();
            cache.record_failure();

            assert_eq!(cache.take_failures(), 2);
            assert_eq!(cache.take_failures(), 0, "the drain must not re-report");
        }

        #[test]
        fn adopting_a_new_window_withdraws_trust_without_counting_a_failure() {
            let cache = RectCache::new();
            cache.store((100, 200, 400, 500));

            cache.reset_for_new_window();

            assert!(!cache.is_valid());
            assert_eq!(cache.take_failures(), 0);
        }

        #[test]
        fn a_poisoned_rect_lock_does_not_freeze_the_cache() {
            // Every reader already recovers from poisoning with `into_inner`, so
            // a writer that gives up on `Err` leaves them serving the old tuple
            // for the rest of the session. One panic while holding this lock
            // used to be enough.
            let cache = RectCache::new();
            cache.store((1, 2, 3, 4));
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = cache.rect.lock().unwrap();
                panic!("poison the rect lock");
            }));
            assert!(cache.rect.is_poisoned(), "the arrange step must have poisoned the lock");

            cache.store((50, 60, 70, 80));

            assert_eq!(cache.get(), (50, 60, 70, 80));
            assert!(cache.is_valid());
        }
    }
}

#[cfg(windows)]
mod overlay_clickthrough {
    use super::rect_cache::RectCache;
    use std::sync::atomic::{AtomicI32, AtomicIsize, AtomicBool, Ordering};
    use std::sync::Mutex as StdMutex;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, BOOL, RECT};
    use windows::Win32::UI::WindowsAndMessaging::*;

    struct SendHook(HHOOK);
    unsafe impl Send for SendHook {}

    static HOOK_HANDLE: StdMutex<Option<SendHook>> = StdMutex::new(None);
    static OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);
    /// Cached overlay geometry. See `rect_cache::RectCache` for the trust rule.
    static RECT_CACHE: RectCache = RectCache::new();
    /// Width of interactive zone on the right edge (physical pixels).
    static INTERACTIVE_WIDTH: AtomicI32 = AtomicI32::new(48);
    /// Click buffer — hook pushes overlay-relative coordinates, message loop drains and emits events.
    static CLICK_BUFFER: StdMutex<Vec<(i32, i32)>> = StdMutex::new(Vec::new());
    /// Whether the overlay has visible content. When false, clicks pass through entirely.
    static HAS_CONTENT: AtomicBool = AtomicBool::new(false);

    /// Re-read the overlay rect into `RECT_CACHE` when it is marked dirty.
    ///
    /// Deliberately cached rather than read per event: this runs inside a
    /// `WH_MOUSE_LL` hook, which Windows silently unhooks when it exceeds
    /// `LowLevelHooksTimeout`, and the hook sees every mouse event system-wide.
    fn refresh_rect_if_dirty() {
        let hwnd_val = OVERLAY_HWND.load(Ordering::Relaxed);
        if hwnd_val == 0 { return; }
        if !RECT_CACHE.take_dirty() { return; }

        unsafe {
            let hwnd = HWND(hwnd_val as *mut _);
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                RECT_CACHE.store((rect.left, rect.top, rect.right, rect.bottom));
            } else {
                RECT_CACHE.record_failure();
            }
        }
    }

    /// Check if cursor is in the interactive zone. Also refreshes cached rect if dirty.
    fn check_interactive(cx: i32, cy: i32) -> bool {
        if OVERLAY_HWND.load(Ordering::Relaxed) == 0 { return false; }

        refresh_rect_if_dirty();
        // Decline rather than hit-test against a rect we could not read. The
        // overlay is fully click-through, so declining passes the click to the
        // game — the same outcome as the cursor being outside the zone, and
        // strictly better than claiming a click at coordinates translated
        // against another window's geometry.
        if !RECT_CACHE.is_valid() { return false; }

        let (left, top, right, bottom) = RECT_CACHE.get();
        let iw = INTERACTIVE_WIDTH.load(Ordering::Relaxed);
        cx >= left && cx < right && cy >= top && cy < bottom && cx >= (right - iw)
    }

    unsafe extern "system" fn mouse_hook_proc(
        n_code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if n_code >= 0 {
            let mouse = &*(l_param.0 as *const MSLLHOOKSTRUCT);
            let cx = mouse.pt.x;
            let cy = mouse.pt.y;
            let msg_id = w_param.0 as u32;

            // A click is the one event rare enough to afford a fresh
            // GetWindowRect (clicks are ~3 orders of magnitude rarer than
            // moves, and the move path must stay inside LowLevelHooksTimeout).
            // A successful read bounds a missed move/resize/DPI event to a
            // single click instead of misplacing the interactive zone and every
            // emitted coordinate for the rest of the session (POE-148). A failed
            // read gives no such bound — it invalidates the cache instead, and
            // the consumers below decline until a read succeeds.
            if msg_id == WM_LBUTTONDOWN {
                RECT_CACHE.invalidate();
                refresh_rect_if_dirty();
            }

            // WebView2 may strip WS_EX_TRANSPARENT when creating/updating child
            // windows. Re-apply when it's missing. Only check when cursor is near
            // the overlay to avoid per-mouse-event Win32 calls system-wide.
            let hwnd_val = OVERLAY_HWND.load(Ordering::Relaxed);
            // Cache validity gates this too: "near the overlay" is meaningless
            // when the cached rect belongs to a window we could not read.
            if hwnd_val != 0 && RECT_CACHE.is_valid() {
                let hwnd = HWND(hwnd_val as *mut _);
                // Only do the GetWindowLongW check when cursor is near the overlay
                // (within the full window rect, not just the interactive zone).
                let (left, top, right, bottom) = RECT_CACHE.get();
                if cx >= left && cx < right && cy >= top && cy < bottom {
                    let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
                    // ex == 0 can mean failure OR "no styles" — only re-apply if we
                    // got a nonzero result that's missing WS_EX_TRANSPARENT.
                    if ex != 0 && ex & WS_EX_TRANSPARENT.0 as i32 == 0 {
                        SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_TRANSPARENT.0 as i32);
                    }
                }
            }

            if check_interactive(cx, cy) && HAS_CONTENT.load(Ordering::Relaxed) {
                // Consume clicks in the interactive zone — don't pass to game.
                if msg_id == WM_LBUTTONDOWN || msg_id == WM_LBUTTONUP {
                    if msg_id == WM_LBUTTONDOWN {
                        // Buffer overlay-relative coordinates for the message loop to emit.
                        let (left, top, _, _) = RECT_CACHE.get();
                        CLICK_BUFFER.lock().unwrap_or_else(|e| e.into_inner())
                            .push((cx - left, cy - top));
                    }
                    return LRESULT(1); // Consume — don't pass to game or CallNextHookEx
                }
            }
        }
        CallNextHookEx(None, n_code, w_param, l_param)
    }

    /// Install the global mouse hook. The message loop drains click events and
    /// emits Tauri `overlay-click` events. Returns a stop-signal sender.
    pub fn install_hook(app: tauri::AppHandle) -> Option<std::sync::mpsc::Sender<()>> {
        if HOOK_HANDLE.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            log::info!("Overlay mouse hook already installed — reusing existing hook");
            return None;
        }
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            unsafe {
                let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) {
                    Ok(h) => h,
                    Err(e) => { log::error!("Mouse hook install failed: {}", e); return; }
                };
                *HOOK_HANDLE.lock().unwrap_or_else(|e| e.into_inner()) = Some(SendHook(hook));
                log::info!("Overlay mouse hook installed (fully click-through mode)");

                let mut msg = MSG::default();
                // Throttled: a persistently unreadable rect stays dirty, so the
                // hook retries on every mouse event and the counter can climb by
                // thousands a second. One line per second reports the volume
                // without flooding the 50-entry LOGS buffer.
                let mut last_failure_report = std::time::Instant::now();
                loop {
                    if stop_rx.try_recv().is_ok() { break; }

                    if last_failure_report.elapsed() >= std::time::Duration::from_secs(1) {
                        last_failure_report = std::time::Instant::now();
                        let failures = RECT_CACHE.take_failures();
                        if failures > 0 {
                            crate::app_log(&app, format!(
                                "Overlay rect read failed {} time(s) in the last second — interactive-zone clicks passed through to the game until a read succeeds",
                                failures,
                            ));
                        }
                    }

                    // Drain click buffer → emit Tauri events.
                    {
                        let mut buf = CLICK_BUFFER.lock().unwrap_or_else(|e| e.into_inner());
                        for (x, y) in buf.drain(..) {
                            use tauri::Emitter;
                            if let Err(e) = app.emit("overlay-click", serde_json::json!({ "x": x, "y": y })) {
                                log::warn!("emit overlay-click failed: {}", e);
                            }
                        }
                    }

                    if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    } else {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }

                if let Some(SendHook(h)) = HOOK_HANDLE.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    if let Err(e) = UnhookWindowsHookEx(h) {
                        log::error!("Failed to unhook mouse hook: {} — hook may leak", e);
                    }
                }
                log::info!("Overlay mouse hook removed");
            }
        });
        Some(stop_tx)
    }



    pub fn set_overlay_hwnd(hwnd: HWND) {
        OVERLAY_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
        // The cached rect describes the previous overlay until the first
        // successful read of this one.
        RECT_CACHE.reset_for_new_window();
    }

    pub fn set_has_content(has: bool) {
        HAS_CONTENT.store(has, Ordering::Relaxed);
    }

    pub fn set_interactive_width(px: i32) {
        INTERACTIVE_WIDTH.store(px, Ordering::Relaxed);
    }

    pub fn invalidate_rect() {
        RECT_CACHE.invalidate();
    }

    /// Invalidate the cached rect when `hwnd` is the overlay this hook tracks.
    ///
    /// Keyed on the HWND rather than a window label so it stays correct for
    /// whichever overlay currently owns the interactive zone.
    pub fn invalidate_if_tracked(hwnd: HWND) {
        if OVERLAY_HWND.load(Ordering::Relaxed) == hwnd.0 as isize {
            RECT_CACHE.invalidate();
        }
    }

    pub unsafe fn set_noactivate(hwnd: HWND) {
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as i32);

        unsafe extern "system" fn enum_child(child: HWND, _: LPARAM) -> BOOL {
            let ex = GetWindowLongW(child, GWL_EXSTYLE);
            SetWindowLongW(child, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as i32);
            BOOL(1)
        }
        let _ = EnumChildWindows(hwnd, Some(enum_child), LPARAM(0));
    }
}

/// Set up an overlay window for click-through with an interactive zone.
/// Call from JS after the window is created. Delays 1s for HWND availability.
///
/// - `label`: Tauri window label
/// - `interactive_width`: width in physical pixels of the interactive zone on the right edge
#[tauri::command]
fn set_overlay_clickthrough(label: String, interactive_width: i32, app: AppHandle) {
    #[cfg(not(windows))]
    {
        let _ = (label, interactive_width, app);
    }

    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;

        let app2 = app.clone();
        let label2 = label.clone();
        std::thread::spawn(move || {
            // WebView2 HWND not available immediately — wait for init
            std::thread::sleep(std::time::Duration::from_millis(1000));

            let window = match app2.get_webview_window(&label2) {
                Some(w) => w,
                None => { log::warn!("Overlay '{}' not found after delay", label2); return; }
            };

            // Make entire window click-through
            if let Err(e) = window.set_ignore_cursor_events(true) {
                log::error!("set_ignore_cursor_events failed for '{}': {}", label2, e);
                return;
            }

            if let Ok(hwnd) = window.hwnd() {
                let h = HWND(hwnd.0 as *mut _);
                unsafe { overlay_clickthrough::set_noactivate(h); }

                // Only set interactive zone globals for overlays that need click
                // interception (interactive_width > 0). Non-interactive overlays
                // (compass, pathstrip) must not overwrite the comparator's HWND.
                if interactive_width > 0 {
                    overlay_clickthrough::set_interactive_width(interactive_width);
                    overlay_clickthrough::set_overlay_hwnd(h);

                    if let Some(tx) = overlay_clickthrough::install_hook(app2.clone()) {
                        let state = app2.state::<AppState>();
                        *state.overlay_hook_stop.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
                    }
                }

                // Re-apply WS_EX_NOACTIVATE after WebView2 children are created
                let hwnd_raw = hwnd.0 as isize;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    unsafe {
                        overlay_clickthrough::set_noactivate(HWND(hwnd_raw as *mut _));
                    }
                });

                log::info!("Overlay clickthrough setup complete for '{}' (interactive={}px)", label2, interactive_width);
            } else {
                log::warn!("Overlay '{}' HWND not available after delay", label2);
            }
        });
    }
}

#[tauri::command]
fn force_show_overlays(app: AppHandle) {
    let state = app.state::<AppState>();
    let mut debug_guard = state.debug_mode.lock().unwrap_or_else(|e| e.into_inner());
    let was_debug = *debug_guard;
    *debug_guard = !was_debug;
    drop(debug_guard);
    if !was_debug {
        // Turning debug ON — show all overlays
        if let Some(win) = app.get_webview_window("comparator") {
            if let Err(e) = win.show() {
                log::warn!("Failed to force-show overlay: {}", e);
            }
        }
        if let Some(win) = app.get_webview_window("compass") {
            if let Err(e) = win.show() {
                log::warn!("Failed to force-show compass: {}", e);
            }
        }
        if let Some(win) = app.get_webview_window("pathstrip") {
            if let Err(e) = win.show() {
                log::warn!("Failed to force-show pathstrip: {}", e);
            }
        }
        if let Some(win) = app.get_webview_window("timer") {
            if let Err(e) = win.show() {
                log::warn!("Failed to force-show timer: {}", e);
            }
        }
        log::info!("Debug mode ON — overlays force-shown");
    } else {
        log::info!("Debug mode OFF");
    }
}

#[tauri::command]
fn set_devtools(open: bool, window: tauri::WebviewWindow) {
    // Devtools has no JS API in Tauri 2 — calling `openDevtools()` on the
    // WebviewWindow object throws synchronously. This command is the only path.
    if open {
        window.open_devtools();
    } else {
        window.close_devtools();
    }
}

#[tauri::command]
fn set_comparator_data(payload: serde_json::Value, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.comparator_data.lock().unwrap_or_else(|e| e.into_inner()) = payload;
}

#[tauri::command]
fn set_overlay_has_content(has_content: bool) {
    #[cfg(windows)]
    overlay_clickthrough::set_has_content(has_content);

    #[cfg(not(windows))]
    let _ = has_content;
}

#[tauri::command]
fn set_overlay_interactive_width(width: i32) {
    #[cfg(windows)]
    overlay_clickthrough::set_interactive_width(width);

    #[cfg(not(windows))]
    let _ = width;
}

#[tauri::command]
fn get_comparator_data(state: tauri::State<AppState>) -> serde_json::Value {
    state.comparator_data.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
fn request_trade_refresh(gem: String, variant: String, app: AppHandle) {
    if let Err(e) = app.emit("overlay-trade-refresh", serde_json::json!({ "name": gem, "variant": variant })) {
        log::warn!("emit overlay-trade-refresh failed: {}", e);
    }
}

#[tauri::command]
fn move_overlay(label: String, x: i32, y: i32, w: u32, h: u32, app: AppHandle) -> Result<(), String> {
    let window = app.get_webview_window(&label)
        .ok_or_else(|| format!("Window '{}' not found", label))?;
    window.set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|e| format!("set_position failed: {}", e))?;
    window.set_size(tauri::PhysicalSize::new(w, h))
        .map_err(|e| format!("set_size failed: {}", e))?;
    // Invalidate cached rect so the mouse hook picks up the new position
    #[cfg(windows)]
    if label == "comparator" {
        overlay_clickthrough::invalidate_rect();
    }
    Ok(())
}

#[tauri::command]
fn comparator_moved(x: i32, y: i32, w: u32, h: u32, app: AppHandle) {
    let mut s = settings::load(&app);
    s.comparator_overlay = Some(settings::OverlaySettings {
        x, y, width: w, height: h, enabled: true,
    });
    settings::save(&app, &s);
    // Invalidate cached rect so the mouse hook picks up the new position
    #[cfg(windows)]
    overlay_clickthrough::invalidate_rect();
    // Emit via Rust — guaranteed to reach all windows
    if let Err(e) = app.emit("comparator-moved", serde_json::json!({ "x": x, "y": y, "w": w, "h": h })) {
        log::warn!("emit comparator-moved failed: {}", e);
    }
}

#[tauri::command]
fn get_comparator_overlay_settings(app: AppHandle) -> Option<settings::OverlaySettings> {
    settings::load(&app).comparator_overlay
}

#[tauri::command]
fn set_comparator_overlay_settings(x: i32, y: i32, w: u32, h: u32, enabled: bool, app: AppHandle) {
    let mut s = settings::load(&app);
    s.comparator_overlay = Some(settings::OverlaySettings {
        x, y, width: w, height: h, enabled,
    });
    settings::save(&app, &s);
}

#[tauri::command]
fn get_compass_overlay_settings(app: AppHandle) -> Option<settings::OverlaySettings> {
    settings::load(&app).compass_overlay
}

#[tauri::command]
fn set_compass_overlay_settings(x: i32, y: i32, w: u32, h: u32, enabled: bool, app: AppHandle) {
    let mut s = settings::load(&app);
    s.compass_overlay = Some(settings::OverlaySettings {
        x, y, width: w, height: h, enabled,
    });
    settings::save(&app, &s);
}

#[tauri::command]
fn get_pathstrip_overlay_settings(app: AppHandle) -> Option<settings::OverlaySettings> {
    settings::load(&app).pathstrip_overlay
}

#[tauri::command]
fn set_pathstrip_overlay_settings(x: i32, y: i32, w: u32, h: u32, enabled: bool, app: AppHandle) {
    let mut s = settings::load(&app);
    s.pathstrip_overlay = Some(settings::OverlaySettings {
        x, y, width: w, height: h, enabled,
    });
    settings::save(&app, &s);
}

#[tauri::command]
fn get_timer_overlay_settings(app: AppHandle) -> Option<settings::OverlaySettings> {
    settings::load(&app).timer_overlay
}

#[tauri::command]
fn set_timer_overlay_settings(x: i32, y: i32, w: u32, h: u32, enabled: bool, app: AppHandle) {
    let mut s = settings::load(&app);
    s.timer_overlay = Some(settings::OverlaySettings {
        x, y, width: w, height: h, enabled,
    });
    settings::save(&app, &s);
}

#[tauri::command]
fn get_timer_appearance(app: AppHandle) -> serde_json::Value {
    let s = settings::load(&app);
    serde_json::json!({
        "bg_opacity": s.timer_bg_opacity.unwrap_or(0.75),
        "text_stroke": s.timer_text_stroke.unwrap_or(true),
    })
}

#[tauri::command]
fn set_timer_appearance(bg_opacity: f32, text_stroke: bool, app: AppHandle) {
    let mut s = settings::load(&app);
    s.timer_bg_opacity = Some(bg_opacity.clamp(0.0, 1.0));
    s.timer_text_stroke = Some(text_stroke);
    settings::save(&app, &s);
    if let Err(e) = app.emit("timer-appearance-changed", serde_json::json!({
        "bg_opacity": bg_opacity.clamp(0.0, 1.0),
        "text_stroke": text_stroke,
    })) {
        log::warn!("emit timer-appearance-changed failed: {}", e);
    }
}

#[tauri::command]
fn get_lab_overlays_enabled(app: AppHandle) -> bool {
    let state = app.state::<AppState>();
    let val = *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner());
    val
}

#[tauri::command]
fn set_lab_overlays_enabled(enabled: bool, app: AppHandle) {
    {
        let state = app.state::<AppState>();
        *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) = enabled;
    }
    persist_settings(&app);
}

#[tauri::command]
fn get_lab_mode(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let val = state.lab_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();
    val
}

#[tauri::command]
fn set_lab_mode(mode: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.lab_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode;
    persist_settings(&app);
}

#[tauri::command]
fn get_autoclear_minutes(app: AppHandle) -> u32 {
    let state = app.state::<AppState>();
    let value = *state.autoclear_minutes.lock().unwrap_or_else(|e| e.into_inner());
    value
}

#[tauri::command]
fn set_autoclear_minutes(minutes: u32, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.autoclear_minutes.lock().unwrap_or_else(|e| e.into_inner()) = minutes;
    persist_settings(&app);
}

#[tauri::command]
fn get_dedication_pool(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let value = state.dedication_pool.lock().unwrap_or_else(|e| e.into_inner()).clone();
    value
}

#[tauri::command]
fn set_dedication_pool(pool: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.dedication_pool.lock().unwrap_or_else(|e| e.into_inner()) = pool;
    persist_settings(&app);
}

#[tauri::command]
fn get_dedication_variant(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let value = state.dedication_variant.lock().unwrap_or_else(|e| e.into_inner()).clone();
    value
}

#[tauri::command]
fn set_dedication_variant(variant: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.dedication_variant.lock().unwrap_or_else(|e| e.into_inner()) = variant;
    persist_settings(&app);
}

#[tauri::command]
fn get_normal_variant(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let value = state.normal_variant.lock().unwrap_or_else(|e| e.into_inner()).clone();
    value
}

#[tauri::command]
fn set_normal_variant(variant: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.normal_variant.lock().unwrap_or_else(|e| e.into_inner()) = variant;
    persist_settings(&app);
}

#[tauri::command]
fn get_show_low_confidence(app: AppHandle) -> bool {
    let state = app.state::<AppState>();
    let value = *state.show_low_confidence.lock().unwrap_or_else(|e| e.into_inner());
    value
}

#[tauri::command]
fn set_show_low_confidence(show: bool, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.show_low_confidence.lock().unwrap_or_else(|e| e.into_inner()) = show;
    persist_settings(&app);
}

#[tauri::command]
fn get_ui_prefs(app: AppHandle) -> std::collections::HashMap<String, String> {
    let state = app.state::<AppState>();
    let prefs = state.ui_prefs.lock().unwrap_or_else(|e| e.into_inner()).clone();
    prefs
}

#[tauri::command]
fn set_ui_pref(key: String, value: String, app: AppHandle) {
    let state = app.state::<AppState>();
    state.ui_prefs.lock().unwrap_or_else(|e| e.into_inner()).insert(key, value);
    persist_settings(&app);
}

#[tauri::command]
fn get_logs(state: tauri::State<AppState>) -> Vec<String> {
    state.logs.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Dev-only: emit a fake lab-nav event to all windows for testing the compass overlay.
#[tauri::command]
fn emit_lab_nav(event_json: serde_json::Value, app: AppHandle) {
    app_log(&app, format!("Dev: emitting lab-nav {:?}", event_json));
    if let Err(e) = app.emit("lab-nav", &event_json) {
        log::warn!("emit lab-nav failed: {}", e);
    }
}

/// Frontend can log messages to the app log (visible in Settings > Logs).
#[tauri::command]
fn app_log_from_frontend(msg: String, app: AppHandle) {
    app_log(&app, msg);
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
    let lab_mode = state.lab_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();
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
    let lab_mode = state.lab_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if lab_mode == "Dedication" {
        state.dedication_variant.lock().unwrap_or_else(|e| e.into_inner()).clone()
    } else {
        state.normal_variant.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

async fn send_gems_to_server(app: &AppHandle, gems: Vec<String>) {
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
async fn test_ocr_on_image(path: String, app: AppHandle) -> Result<String, String> {
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
    let lab_mode = state.lab_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();
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

/// Gem-only OCR scan on a dedicated OS thread.
///
/// Scans the gem tooltip region every 250ms looking for transfigured gem names.
/// Stops when:
///   - 3 gems detected (all options scanned)
///   - 45s timeout (user walked away or didn't hover all gems)
///   - Generation mismatch (new scan started or manual stop)
///   - Lab state changed to non-PickingGems (zone change)
fn gem_scan_loop(app: AppHandle, generation: u64) {
    let state = app.state::<AppState>();

    // Load gem names for matching — abort early if server unreachable.
    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let http = state.server_http.clone();
    let lab_mode = state.lab_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let gem_names = tauri::async_runtime::block_on(fetch_gem_names(&app, &server, &http, &lab_mode));
    if gem_names.is_empty() {
        // States its own cause rather than deferring upward: the abort is shared
        // by both modes (Normal has no halves to fail), and an empty dictionary
        // reaches here from a legitimate 200 as well as from a request failure.
        app_log(&app, "Gem scan aborted — the gem dictionary loaded 0 names, so no OCR read could match. Either the request failed or this league has no gem dictionary yet; the preceding 'gem names' lines say which.".to_string());
        if state.gem_scan_generation.load(Ordering::SeqCst) == generation {
            *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) = lab_state::LabState::Idle;
            emit_status(&app);
        }
        return;
    }
    let matcher = gem_matcher::GemMatcher::new(gem_names.clone());
    app_log(&app, format!("Gem scan: loaded {} gem names", gem_names.len()));
    // Surface which OCR recognizer is active (and any en-US fallback warning) —
    // this is the only breadcrumb the LOGS panel gets for a silent CJK fallback.
    report_ocr_engine(&app);

    let mut seen_gems: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut gems_found = 0u32;
    let mut loop_count = 0u32;
    // Rejection log, deduplicated by message and capped per scan.
    //
    // Every rejection is worth a line — a player whose third option never
    // appears otherwise gets no breadcrumb at all, and the throttled raw
    // candidate dump below fires on one loop in eight and cannot say which gate
    // discarded the text. But the loop reads the same tooltip four times a
    // second for up to 45s, and the LOGS panel keeps only 50 entries, so
    // unbounded reject lines would push every other diagnostic out of the
    // buffer. Deduplicating collapses the repeats; the cap bounds OCR text that
    // differs slightly frame to frame.
    const MAX_REJECT_LOGS: usize = 12;
    let mut logged_rejects: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut rejects_suppressed = false;
    let start = std::time::Instant::now();
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
    const MAX_GEMS: u32 = 3;
    const SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

    loop {
        // Check generation — if bumped, a new scan was started or we were stopped.
        if state.gem_scan_generation.load(Ordering::SeqCst) != generation {
            app_log(&app, "Gem scan stopped (new scan or manual stop)".to_string());
            break;
        }

        // Check lab state — zone change sets this to Idle.
        {
            let current = state.lab_state.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if current != lab_state::LabState::PickingGems {
                app_log(&app, "Gem scan stopped (state changed)".to_string());
                break;
            }
        }

        // Check timeout.
        if start.elapsed() >= TIMEOUT {
            app_log(&app, format!("Gem scan timed out after 45s ({} gems found)", gems_found));
            break;
        }

        loop_count += 1;

        let gem_region = state.gem_region.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let screen = match capture::capture_screen() {
            Ok(s) => s,
            Err(e) => {
                if loop_count % 20 == 1 {
                    app_log(&app, format!("Screen capture failed: {}", e));
                }
                std::thread::sleep(SCAN_INTERVAL);
                continue;
            }
        };

        let cropped = screen.crop_imm(gem_region.x.max(0) as u32, gem_region.y.max(0) as u32, gem_region.w, gem_region.h);
        let processed = capture::preprocess_for_ocr(&cropped);
        let lines = match ocr::recognize_text(&processed) {
            Ok(l) => l,
            Err(e) => {
                if loop_count % 20 == 1 {
                    app_log(&app, format!("Gem OCR failed: {}", e));
                }
                std::thread::sleep(SCAN_INTERVAL);
                continue;
            }
        };

        let candidates = ocr::extract_gem_candidates(&lines);
        // Diagnostic: log raw OCR candidates (throttled ~2s) so false positives
        // can be traced to the on-screen text that triggered them. See POE gem
        // OCR false-positive investigation ("Divine Ire of Disintegration").
        if !candidates.is_empty() && loop_count % 8 == 1 {
            app_log(&app, format!("Gem OCR candidates: {:?}", candidates));
        }
        let mut best: Option<gem_matcher::GemMatch> = None;
        for candidate in &candidates {
            match matcher.match_gem(candidate) {
                Ok(m) => {
                    if best.as_ref().map_or(true, |b| m.score > b.score) {
                        best = Some(m);
                    }
                }
                Err(reason) => {
                    let line = format!("Gem OCR rejected {:?}: {}", candidate, reason);
                    if logged_rejects.len() < MAX_REJECT_LOGS {
                        if logged_rejects.insert(line.clone()) {
                            app_log(&app, line);
                        }
                    } else if !rejects_suppressed {
                        rejects_suppressed = true;
                        app_log(&app, format!(
                            "Gem OCR: {} distinct rejections logged — further rejections suppressed for this scan",
                            MAX_REJECT_LOGS,
                        ));
                    }
                }
            }
        }

        if let Some(gem_match) = best {
            if !seen_gems.contains(&gem_match.name) {
                seen_gems.insert(gem_match.name.clone());
                gems_found += 1;
                app_log(&app, format!(
                    "Gem detected: {} (score: {:.2}) [{}/{}] from OCR {:?}",
                    gem_match.name, gem_match.score, gems_found, MAX_GEMS, gem_match.ocr_raw
                ));

                let all_gems = {
                    let mut gems = state.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
                    gems.push(gem_match.name.clone());
                    let cloned = gems.clone();
                    drop(gems);
                    cloned
                };
                if let Err(e) = app.emit("gem-detected", &gem_match.name) { log::warn!("emit gem-detected failed: {}", e); }
                emit_status(&app);
                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    send_gems_to_server(&app_clone, all_gems).await;
                });

                // All 3 gems found — stop scanning.
                if gems_found >= MAX_GEMS {
                    app_log(&app, "Gem scan complete (3/3 gems detected)".to_string());
                    break;
                }
            }
        }

        std::thread::sleep(SCAN_INTERVAL);
    }

    // Transition state back to Idle if WE are still the active scan.
    // Hold the lab_state lock while checking generation to prevent TOCTOU
    // race with spawn_gem_scan (which bumps generation then sets PickingGems).
    {
        let mut lab = state.lab_state.lock().unwrap_or_else(|e| e.into_inner());
        if state.gem_scan_generation.load(Ordering::SeqCst) == generation {
            *lab = lab_state::LabState::Idle;
            drop(lab);
            emit_status(&app);
        }
    }
}

/// Start font panel OCR. Bumps generation to cancel any running scan, spawns a new loop.
fn spawn_font_scan(app: &AppHandle) {
    let state = app.state::<AppState>();
    // Minted, not `fetch_add` + 1: the liveness token reads generation 0 as
    // "no scan running", so no loop may ever own it.
    let gen = font_session::next_scan_generation(&state.font_scan_generation);
    // Claim the liveness token before the thread starts, so a `FontOpened`
    // arriving in between does not re-arm a scan that is already on its way.
    state.font_scan_live_gen.store(gen, Ordering::SeqCst);

    // Reset font session for the new scan. The temporary guard is dropped at the
    // end of this statement, so the emit below cannot self-deadlock on
    // `build_status` re-reading `font_session`.
    *state.font_session.lock().unwrap_or_else(|e| e.into_inner()) = FontSessionData::default();

    app_log(app, format!("Font scan started (gen={})", gen));
    // `font_session_rounds` dropped back to 0 — clear the Discard affordance
    // left over from the previous run instead of waiting for the next emit.
    emit_status(app);

    let app_capture = app.clone();
    std::thread::spawn(move || {
        font_scan_loop(app_capture, gen);
    });
}

/// Font panel OCR loop on a dedicated OS thread.
///
/// Scans the font region every 250ms looking for craft options (CRAFT screen).
/// Stores detected options in AppState.font_session, sealing a round whenever
/// the panel's craft count changes (`font_ledger`).
///
/// Stops on a generation bump — `ZoneChanged`, lab exit, a replacement scan or
/// app shutdown — or after `IDLE_LIMIT` with no active font panel on screen.
/// There is deliberately no wall-clock timeout: a font run has no bounded length
/// (stash trips, town portals, a player reading the options), and the scan
/// expiring under a still-open panel silently lost every remaining craft. The
/// idle limit measures from the last frame that saw the panel, so it can only
/// fire on a scan that has nothing left to read — the case where every stop
/// event was missed, e.g. the player never opened the font and the game was
/// killed rather than exited. That path sends the session itself, because no
/// caller follows it and the next re-arm would reset the rounds it sealed.
fn font_scan_loop(app: AppHandle, generation: u64) {
    let state = app.state::<AppState>();
    let mut loop_count = 0u32;
    const SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
    const IDLE_LIMIT: std::time::Duration = std::time::Duration::from_secs(600);
    let mut last_active = std::time::Instant::now();
    // What the previous iteration's frame saw. The tick reads it at the top of
    // the loop rather than where the panel is parsed, so the capture- and
    // OCR-failure paths below cannot `continue` past the expiry check — a
    // permanently failing capture is exactly the case this guard exists for.
    let mut frame_saw_panel = false;

    // Surface which OCR recognizer is active (and any en-US fallback warning) so a
    // silent CJK fallback shows up in the LOGS panel, not just stderr.
    report_ocr_engine(&app);

    loop {
        // Check generation.
        if state.font_scan_generation.load(Ordering::SeqCst) != generation {
            app_log(&app, "Font scan stopped (generation mismatch)".to_string());
            break;
        }

        let (deadline, idle_expired) = font_session::idle_tick(
            last_active,
            std::time::Instant::now(),
            frame_saw_panel,
            IDLE_LIMIT,
        );
        last_active = deadline;
        frame_saw_panel = false;
        if idle_expired {
            app_log(&app, "Font scan stopped (no font panel for 10 minutes)".to_string());
            // The only stop that no sender follows: ZoneChanged and the send
            // path seal and POST for themselves, and the next re-arm's
            // `spawn_font_scan` resets the session. Without this, rounds sealed
            // before the player wandered off are dropped.
            send_font_session_data(&app);
            break;
        }

        loop_count += 1;

        let font_region = state.font_region.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let screen = match capture::capture_screen() {
            Ok(s) => s,
            Err(e) => {
                if loop_count % 40 == 1 {
                    app_log(&app, format!("Font scan: screen capture failed: {}", e));
                }
                std::thread::sleep(SCAN_INTERVAL);
                continue;
            }
        };

        let cropped = screen.crop_imm(
            font_region.x.max(0) as u32,
            font_region.y.max(0) as u32,
            font_region.w,
            font_region.h,
        );
        let processed = capture::preprocess_for_ocr(&cropped);
        let lines = match ocr::recognize_text(&processed) {
            Ok(l) => l,
            Err(e) => {
                if loop_count % 40 == 1 {
                    app_log(&app, format!("Font scan: OCR failed: {}", e));
                }
                std::thread::sleep(SCAN_INTERVAL);
                continue;
            }
        };

        let panel = font_parser::parse_font_panel(&lines);
        frame_saw_panel = panel.font_active;

        // Silent-failure breadcrumb: OCR produced text but nothing parsed as an
        // active font panel — usually the panel garbled (e.g. a wrong-language
        // recognizer). Throttled like the capture/OCR-failure logs above.
        if !panel.font_active && !lines.is_empty() && loop_count % 40 == 1 {
            app_log(&app, format!("Font OCR raw (inactive, {} lines): {}", lines.len(), lines.join(" | ")));
        }

        if panel.font_active && !panel.options.is_empty() {
            // Apply the frame under lock, then log/emit outside it.
            let outcome = {
                let mut session = state.font_session.lock().unwrap_or_else(|e| e.into_inner());
                // Re-check the generation while holding the session lock: this
                // frame was captured before the check at the top of the
                // iteration, and `spawn_font_scan` resets the session under this
                // same lock right after bumping the generation. Without the
                // re-check a stale frame lands in the fresh session.
                if state.font_scan_generation.load(Ordering::SeqCst) != generation {
                    None
                } else {
                    // Read the event counter as late as possible. A `FontOpened`
                    // that lands between the capture and here at worst makes a
                    // pre-click frame read as the already-accepted count (a
                    // `Same`); reading it earlier would instead make the frame
                    // that first shows the new count look like a misread.
                    let event_seq = state.font_opened_seq.load(Ordering::SeqCst);
                    Some(font_session::apply_font_frame(&mut session, &panel, event_seq))
                }
            }; // lock released

            let outcome = match outcome {
                Some(outcome) => outcome,
                None => {
                    app_log(&app, "Font scan stopped (generation mismatch)".to_string());
                    break;
                }
            };

            if let Some(sealed) = &outcome.sealed {
                app_log(&app, format!(
                    "Font round {} sealed ({} options{})",
                    sealed.number,
                    sealed.round.options.len(),
                    sealed.round.crafts_remaining.map_or(
                        ", last craft".to_string(),
                        |n| format!(", {} remaining", n),
                    ),
                ));
                // `rounds` just grew, and `font_session_rounds` is what gates
                // the Discard affordance and its round count.
                emit_status(&app);
            }

            if outcome.buffer_grew {
                // Re-log the raw OCR each time the round buffer grows so later
                // captures in the same scan aren't blinded (was a one-shot flag).
                app_log(&app, format!("Font OCR raw ({} lines): {}", lines.len(), lines.join(" | ")));
                app_log(&app, format!(
                    "Font options captured: {} options{}{}",
                    outcome.buffer.len(),
                    if panel.jackpot_detected { " *** JACKPOT! ***" } else { "" },
                    outcome.crafts_remaining.map_or(
                        " (last craft)".to_string(),
                        |n| format!(" (remaining: {})", n),
                    ),
                ));
                for opt in &outcome.buffer {
                    app_log(&app, format!("  - {} {}", opt.option_type,
                        opt.value.map(|v| format!("({})", v)).unwrap_or_default()));
                }

                if panel.jackpot_detected {
                    if let Err(e) = app.emit("font-jackpot", true) {
                        log::warn!("emit font-jackpot failed: {}", e);
                    }
                }
            }
        }

        std::thread::sleep(SCAN_INTERVAL);
    }

    // Release the liveness token unless a replacement scan already claimed it,
    // so the next `FontOpened` knows whether the panel is still being watched.
    font_session::try_clear_live(&state.font_scan_live_gen, generation);
}

/// Seal the round buffer left over when the session ends without another craft.
///
/// Tauri-side adapter over `font_session::seal_leftover_round`: locks, seals,
/// logs and emits. Rounds are otherwise sealed by the craft-count ledger inside
/// `font_scan_loop`; `FontOpened` seals nothing, because it fires on font open
/// as well as on CRAFT.
fn seal_font_round(app: &AppHandle) {
    let state = app.state::<AppState>();

    // The guard lives in this block only: `emit_status` -> `build_status` locks
    // `font_session` to read `font_session_rounds`, and std Mutex is not
    // re-entrant, so emitting while the guard is alive deadlocks this thread.
    let sealed = {
        let mut session = state.font_session.lock().unwrap_or_else(|e| e.into_inner());
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
fn discard_font_session(app: AppHandle) {
    let state = app.state::<AppState>();
    let discarded = {
        let mut session = state.font_session.lock().unwrap_or_else(|e| e.into_inner());
        let discarded = session.rounds.len();
        *session = FontSessionData::default();
        discarded
    };
    app_log(&app, format!("Font session discarded ({} rounds)", discarded));
    emit_status(&app);
}

/// Send accumulated font session to the server and reset.
fn send_font_session_data(app: &AppHandle) {
    // Seal the round still on screen (the player left without another craft).
    // Takes and releases the session lock itself, so it runs before ours.
    seal_font_round(app);

    let state = app.state::<AppState>();
    let mut session = state.font_session.lock().unwrap_or_else(|e| e.into_inner());

    let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let lab_mode = state.lab_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();

    let device_id = state.device_id.clone();

    // Map lab mode to session metadata. The run is against the selected market
    // whichever mode it is in, so the session records that one — a fixed
    // "20/20" attributed every Normal run to a market the player may not have
    // been farming, and font_sessions.variant is crowd-sourced data.
    let lab_type = if lab_mode == "Dedication" { "Dedication" } else { "Unknown" };
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
async fn fetch_gem_names(app: &AppHandle, server_url: &str, client: &reqwest::Client, lab_mode: &str) -> Vec<String> {
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

/// Focus poller result: where the foreground window belongs.
#[cfg(windows)]
enum FocusState {
    /// Path of Exile is the foreground window.
    Game,
    /// Our own process (overlay, main window) is foreground.
    OwnWindow,
    /// Some other application is foreground.
    Other,
}

/// Poll GetForegroundWindow to detect game focus changes.
/// More reliable than Client.txt log events (no latency, works if PoE crashes).
/// Runs every 1 second on a dedicated thread.
///
/// Three-state logic:
///   - Foreground is PoE → game focused, show overlay
///   - Foreground is our own process (overlay/main window) → neutral, keep current state
///   - Foreground is anything else → game not focused, hide overlay
fn spawn_focus_poller(app: AppHandle) {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    {
        let state = app.state::<AppState>();
        *state.focus_poller_stop.lock().unwrap_or_else(|e| {
            log::warn!("focus_poller_stop mutex poisoned, recovering");
            e.into_inner()
        }) = Some(stop_tx);
    }

    std::thread::spawn(move || {
        #[cfg(windows)]
        let mut was_focused = false;
        #[cfg(windows)]
        let our_pid = std::process::id();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(1000));

            // Check stop signal
            if stop_rx.try_recv().is_ok() {
                log::info!("Focus poller stopped");
                break;
            }

            #[cfg(windows)]
            {
                use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

                let focus_state = unsafe {
                    let fg = GetForegroundWindow();
                    if fg.0.is_null() {
                        FocusState::Other // no foreground window → treat as blur
                    } else {
                        // Check if this window belongs to our process (overlay, main window).
                        let mut fg_pid: u32 = 0;
                        let tid = GetWindowThreadProcessId(fg, Some(&mut fg_pid));
                        if tid == 0 {
                            // HWND invalidated between GetForegroundWindow and here (TOCTOU).
                            // Fall through to title-based detection.
                            FocusState::Other
                        } else if fg_pid == our_pid {
                            FocusState::OwnWindow
                        } else {
                            // Check window class (more reliable than title — avoids
                            // matching browser tabs like "Path of Exile Trade")
                            use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
                            let mut cls_buf = [0u16; 256];
                            let cls_len = GetClassNameW(fg, &mut cls_buf);
                            if cls_len > 0 {
                                let class_name = String::from_utf16_lossy(&cls_buf[..cls_len as usize]);
                                if class_name == "POEWindowClass" {
                                    FocusState::Game
                                } else {
                                    FocusState::Other
                                }
                            } else {
                                FocusState::Other
                            }
                        }
                    }
                };

                // When foreground is our own window (overlay button click, main app),
                // don't change game_focused — preserve the last known state.
                if matches!(focus_state, FocusState::OwnWindow) {
                    continue;
                }

                let is_focused = matches!(focus_state, FocusState::Game);

                if is_focused != was_focused {
                    was_focused = is_focused;
                    let state = app.state::<AppState>();
                    *state.game_focused.lock().unwrap_or_else(|e| {
                        log::warn!("game_focused mutex poisoned, recovering");
                        e.into_inner()
                    }) = is_focused;
                    if let Err(e) = app.emit("game-focus-changed", is_focused) {
                        log::warn!("emit game-focus-changed failed: {}", e);
                    }
                    emit_status(&app);

                    // Hide/show overlay windows based on game focus.
                    // Comparator: shows whenever game is focused (used everywhere).
                    // Compass + Pathstrip: only show when game is focused AND in lab.
                    // Skip hide in debug mode.
                    let debug = *state.debug_mode.lock().unwrap_or_else(|e| e.into_inner());
                    let in_lab = state.in_lab.load(Ordering::SeqCst);

                    // Comparator: game focus only
                    if let Some(win) = app.get_webview_window("comparator") {
                        if is_focused {
                            if let Err(e) = win.show() {
                                log::warn!("Failed to show comparator overlay: {}", e);
                            }
                        } else if !debug {
                            if let Err(e) = win.hide() {
                                log::warn!("Failed to hide comparator overlay: {}", e);
                            }
                        }
                    }

                    // Lab overlays: game focus + in_lab
                    for overlay_name in &["compass", "pathstrip", "timer"] {
                        if let Some(win) = app.get_webview_window(overlay_name) {
                            if is_focused && in_lab {
                                if let Err(e) = win.show() {
                                    log::warn!("Failed to show {} overlay: {}", overlay_name, e);
                                }
                            } else if !debug {
                                if let Err(e) = win.hide() {
                                    log::warn!("Failed to hide {} overlay: {}", overlay_name, e);
                                }
                            }
                        }
                    }
                }
            }

            #[cfg(not(windows))]
            {
                let _ = &app;
                break;
            }
        }
    });
}

fn spawn_log_watcher(app: AppHandle) {
    let state = app.state::<AppState>();
    let client_txt = state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let exists = std::path::Path::new(&client_txt).exists();
    app_log(&app, format!("Starting log watcher: {} (exists: {})", client_txt, exists));
    if !exists {
        app_log(&app, "WARNING: Client.txt not found — check path in Settings".to_string());
    }

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

        // --- Catchup: replay recent Client.txt history to reconstruct lab state ---
        {
            let (replay_events, was_in_lab) = lab_navigation::replay_recent_log(
                std::path::Path::new(&client_txt),
            );
            if !replay_events.is_empty() {
                app_log(&app, format!("Catchup: replaying {} events from Client.txt history", replay_events.len()));
                let state = app.state::<AppState>();
                state.in_lab.store(was_in_lab, std::sync::atomic::Ordering::SeqCst);
                // Show/hide overlays based on reconstructed state
                for name in &["compass", "pathstrip", "timer"] {
                    if let Some(win) = app.get_webview_window(name) {
                        let action = if was_in_lab { win.show() } else { win.hide() };
                        if let Err(e) = action {
                            log::warn!("catchup: failed to show/hide {}: {}", name, e);
                        }
                    } else {
                        app_log(&app, format!("Catchup: overlay '{}' not yet created, skipping show/hide", name));
                    }
                }
                // Emit all replay events to frontend so overlays reconstruct navigation
                for event in &replay_events {
                    if let Err(e) = app.emit("lab-nav", event) {
                        log::warn!("catchup emit failed: {}", e);
                    }
                }
                app_log(&app, format!("Catchup complete: in_lab={}", was_in_lab));
            }
        }

        emit_status(&app);

        let mut state_machine = lab_state::LabStateMachine::new();
        let mut detected_gems: Vec<String> = Vec::new();
        let _matcher = gem_matcher::GemMatcher::new(vec![]); // TODO: fetch from server
        let mut last_trial_entered = std::time::Instant::now() - std::time::Duration::from_secs(10);
        let mut line_count: u64 = 0;
        let mut font_opened_count: u32 = 0;

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

                    line_count += 1;
                    // Log first line and then every 100th to confirm watcher is reading
                    if line_count == 1 {
                        app_log(&app, format!("Log watcher: first line received (len={})", line.len()));
                    } else if line_count % 100 == 0 {
                        app_log(&app, format!("Log watcher: {} lines processed", line_count));
                    }

                    // --- Aspirant's Trial / Plaza tracking (outside state machine) ---
                    if line.contains("You have entered") {
                        let state = app.state::<AppState>();
                        if line.contains("Aspirants' Plaza") || line.contains("Aspirant's Plaza") {
                            state.aspirant_trial_count.store(0, Ordering::SeqCst);
                            font_opened_count = 0;
                            app_log(&app, "Aspirants' Plaza — trial counter reset".to_string());
                        } else if line.contains("Aspirant's Trial") {
                            // Dedup: same Trial zone within 0.5s is a log batch artifact.
                            if last_trial_entered.elapsed() >= std::time::Duration::from_millis(500) {
                                last_trial_entered = std::time::Instant::now();
                                let count = state.aspirant_trial_count.fetch_add(1, Ordering::SeqCst) + 1;
                                app_log(&app, format!("Aspirant's Trial #{}", count));
                                if count == 3 {
                                    app_log(&app, "3rd Aspirant's Trial — final Izaro fight".to_string());
                                }
                            }
                        }
                    }

                    // --- Lab navigation events (outside state machine) ---
                    {
                        let state = app.state::<AppState>();
                        let in_lab = state.in_lab.load(Ordering::SeqCst);
                        let mut nav_emitted = false;
                        if let Some(nav_event) = lab_navigation::parse_nav_event(&line, in_lab) {
                            match &nav_event {
                                lab_navigation::NavEvent::PlazaEntered => {
                                    state.in_lab.store(true, Ordering::SeqCst);
                                    app_log(&app, "Lab nav: Plaza entered".to_string());
                                    // Show lab overlays on lab entry
                                    for name in &["compass", "pathstrip", "timer"] {
                                        if let Some(win) = app.get_webview_window(name) {
                                            let _ = win.show();
                                        }
                                    }
                                }
                                lab_navigation::NavEvent::LabStarted => {
                                    app_log(&app, "Lab nav: Izaro started".to_string());
                                }
                                lab_navigation::NavEvent::RoomChanged { name } => {
                                    app_log(&app, format!("Lab nav: room {}", name));
                                }
                                lab_navigation::NavEvent::LabExited => {
                                    state.in_lab.store(false, Ordering::SeqCst);
                                    app_log(&app, "Lab nav: exited lab".to_string());
                                    // The state machine only emits `ZoneChanged`
                                    // from FontReady/PickingGems, so a scan
                                    // spawned at LabFinished for a player who
                                    // never opened the font has no other stop.
                                    state.font_scan_generation.fetch_add(1, Ordering::SeqCst);
                                    let was_live = font_session::font_scan_is_live(
                                        state.font_scan_live_gen.swap(0, Ordering::SeqCst),
                                    );
                                    if was_live {
                                        app_log(&app, "Font scan stopped (lab exited)".to_string());
                                    }
                                    // Emit event BEFORE hiding overlays — timer needs
                                    // LabExited to submit the run before being hidden.
                                    if let Err(e) = app.emit("lab-nav", &nav_event) {
                                        log::warn!("emit lab-nav (LabExited) failed: {}", e);
                                    }
                                    // Hide lab overlays after event delivery
                                    for name in &["compass", "pathstrip", "timer"] {
                                        if let Some(win) = app.get_webview_window(name) {
                                            let _ = win.hide();
                                        }
                                    }
                                    nav_emitted = true;
                                }
                                lab_navigation::NavEvent::LabFinished => {
                                    app_log(&app, "Lab nav: Izaro defeated! Starting font panel OCR".to_string());
                                    font_opened_count = 0;
                                    spawn_font_scan(&app);
                                }
                                lab_navigation::NavEvent::SectionFinished => {
                                    app_log(&app, "Lab nav: section finished".to_string());
                                }
                                lab_navigation::NavEvent::IzaroBattleStarted => {
                                    app_log(&app, "Lab nav: Izaro battle started".to_string());
                                }
                                lab_navigation::NavEvent::PortalSpawned => {
                                    app_log(&app, "Lab nav: portal spawned".to_string());
                                }
                                lab_navigation::NavEvent::DarkshrineActivated => {
                                    app_log(&app, "Lab nav: darkshrine activated".to_string());
                                }
                            }
                            if !nav_emitted {
                                if let Err(e) = app.emit("lab-nav", &nav_event) {
                                    log::warn!("emit lab-nav failed: {}", e);
                                }
                            }
                        }
                    }

                    if let Some(event) = state_machine.process_line(&line) {
                        let state = app.state::<AppState>();
                        match &event {
                            lab_state::LabEvent::FontOpened => {
                                // Publish the event before anything else: the
                                // craft ledger reads this counter per OCR frame
                                // and a frame captured after the click must see
                                // the new value, or the count change it carries
                                // reads as a misread.
                                state.font_opened_seq.fetch_add(1, Ordering::SeqCst);
                                // The event cannot seal a round — it fires on
                                // font open as well as on CRAFT, an unbounded
                                // number of times per craft. Only the panel's
                                // craft count delimits rounds (`font_ledger`).
                                font_opened_count += 1;
                                // A portal trip out of the lab stops the font
                                // scan for good (`LabFinished` does not fire
                                // again on return), so the liveness token is
                                // what decides whether this event has to bring
                                // the panel OCR back.
                                let font_scan_live = font_session::font_scan_is_live(
                                    state.font_scan_live_gen.load(Ordering::SeqCst),
                                );
                                for effect in font_session::font_opened_effects(font_opened_count, font_scan_live) {
                                    match effect {
                                        font_session::FontOpenedEffect::StartGemScan => {
                                            // Odd: CRAFT click → start gem scan
                                            detected_gems.clear();
                                            spawn_gem_scan(&app, "font");
                                            app_log(&app, format!("FontOpened #{} — CRAFT, gem scan started", font_opened_count));
                                        }
                                        font_session::FontOpenedEffect::StopGemScan => {
                                            // Even: CONFIRM click → stop scanning, clear comparator
                                            state.gem_scan_generation.fetch_add(1, Ordering::SeqCst);
                                            detected_gems.clear();
                                            *state.detected_gems.lock().unwrap_or_else(|e| e.into_inner()) = Vec::new();
                                            if let Err(e) = app.emit("gems-cleared", ()) { log::warn!("emit gems-cleared failed: {}", e); }
                                            app_log(&app, format!("FontOpened #{} — CONFIRM, gem scan stopped", font_opened_count));
                                        }
                                        font_session::FontOpenedEffect::RearmFontScan => {
                                            // `ZoneChanged` owns the session
                                            // reset, so this starts a fresh
                                            // segment rather than resuming one.
                                            app_log(&app, "Font scan re-armed".to_string());
                                            spawn_font_scan(&app);
                                        }
                                    }
                                }
                            }
                            lab_state::LabEvent::ZoneChanged { area } => {
                                app_log(&app, format!("Zone changed: {} — stopping", area));

                                // Stop both gem and font scans + reset state.
                                state.gem_scan_generation.fetch_add(1, Ordering::SeqCst);
                                state.font_scan_generation.fetch_add(1, Ordering::SeqCst);
                                // Synchronously, not by waiting for the loop to
                                // notice: a `FontOpened` arriving in that window
                                // must still see a dead scan and re-arm it.
                                state.font_scan_live_gen.store(0, Ordering::SeqCst);
                                font_opened_count = 0;
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

                                // Send accumulated font session data to server.
                                send_font_session_data(&app);

                                // Clear frontend comparator — player left the area.
                                *state.detected_gems.lock().unwrap_or_else(|e| e.into_inner()) =
                                    Vec::new();
                                if let Err(e) = app.emit("gems-cleared", ()) { log::warn!("emit gems-cleared failed: {}", e); }
                                emit_status(&app);
                            }
                            lab_state::LabEvent::FontClosed => {
                                app_log(&app, "Font closed".to_string());
                                *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) =
                                    lab_state::LabState::Idle;
                                emit_status(&app);
                            }
                            // GameFocused/GameBlurred handled by the focus poller
                            // (GetForegroundWindow — more reliable than Client.txt)
                            lab_state::LabEvent::GameFocused | lab_state::LabEvent::GameBlurred => {}
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

    // Keep WebView2 renderer alive when the window is backgrounded — PoE alt-tab steals
    // focus and Chromium's default backgrounding pauses timers and drops the SSE socket.
    // If the outer shell already set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS, append to it
    // rather than silently clobbering — that env var is additive and operators may have
    // set their own flags for debugging or workaround purposes.
    let prior = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
    let ours = "--disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows";
    let combined = if prior.is_empty() {
        ours.to_string()
    } else {
        log::warn!(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS already set ({:?}); appending our flags",
            prior
        );
        format!("{} {}", prior, ours)
    };
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", &combined);

    let pair_code = generate_pair_code();
    log::info!("Pair code: {}", pair_code);

    let device_id = fingerprint::compute_device_id();
    log::info!("Device ID: {}... ({})", &device_id[..device_id.len().min(8)],
        if device_id.len() == 64 { "hardware" } else { "volatile" });

    // Build server HTTP client with default device headers.
    let version = env!("CARGO_PKG_VERSION");
    let mut default_headers = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(&device_id) {
        default_headers.insert("X-Device-ID", v);
    }
    if let Ok(v) = reqwest::header::HeaderValue::from_str(version) {
        default_headers.insert("X-App-Version", v);
    }
    let server_http = reqwest::Client::builder()
        .default_headers(default_headers)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let app_state = AppState {
        device_id: device_id.clone(),
        pair_code: Mutex::new(pair_code),
        client_txt_path: Mutex::new(String::from(
            r"C:\Program Files (x86)\Grinding Gear Games\Path of Exile\logs\Client.txt",
        )),
        server_url: Mutex::new(String::from(option_env!("POE_SERVER_URL").unwrap_or("https://profitofexile.localhost"))),
        detected_gems: Mutex::new(Vec::new()),
        lab_state: Mutex::new(lab_state::LabState::Idle),
        logs: Mutex::new(Vec::new()),
        gem_region: Mutex::new(CaptureRegion::default()),
        font_region: Mutex::new(CaptureRegion::default_font_panel()),
        sidebar_open: Mutex::new(true),
        game_focused: Mutex::new(false),
        trade_client: trade::TradeApiClient::new(),
        server_http,
        watcher_cancel: Mutex::new(None),
        comparator_data: Mutex::new(serde_json::json!({"results":[],"tradeData":{}})),
        overlay_hook_stop: Mutex::new(None),
        focus_poller_stop: Mutex::new(None),
        debug_mode: Mutex::new(false),
        trade_stale_warn_secs: Mutex::new(settings::DEFAULT_TRADE_STALE_WARN_SECS),
        trade_stale_critical_secs: Mutex::new(settings::DEFAULT_TRADE_STALE_CRITICAL_SECS),
        trade_auto_refresh_secs: Mutex::new(settings::DEFAULT_TRADE_AUTO_REFRESH_SECS),
        auto_trade_enabled: Mutex::new(false),
        gem_scan_generation: AtomicU64::new(0),
        font_scan_generation: AtomicU64::new(0),
        font_scan_live_gen: AtomicU64::new(0),
        font_opened_seq: AtomicU64::new(0),
        aspirant_trial_count: AtomicU32::new(0),
        font_session: Mutex::new(FontSessionData::default()),
        in_lab: AtomicBool::new(false),
        compass_mode: Mutex::new(String::from("minimap")),
        compass_strategy: Mutex::new(String::from("shortest")),
        compass_difficulty: Mutex::new(String::from("Uber")),
        shrine_warn_enabled: Mutex::new(true),
        shrine_warn_size: Mutex::new(String::from("medium")),
        shrine_warn_corner: Mutex::new(String::from("bottom-right")),
        shrine_warn_on_take: Mutex::new(String::from("green")),
        lab_overlays_enabled: Mutex::new(true),
        lab_mode: Mutex::new(String::from("Normal")),
        autoclear_minutes: Mutex::new(2),
        dedication_pool: Mutex::new(String::from("skill")),
        dedication_variant: Mutex::new(String::from("21/23")),
        normal_variant: Mutex::new(String::from("20/20")),
        show_low_confidence: Mutex::new(false),
        ui_prefs: Mutex::new(std::collections::HashMap::new()),
        ssot: Mutex::new(ssot::AppSsotSnapshot::default()),
        modules_enabled: Mutex::new(std::collections::HashMap::new()),
        module_handles: Mutex::new(std::collections::HashMap::new()),
        modules_shutting_down: AtomicBool::new(false),
        mercenary: Mutex::new(mercenary::MercenarySlice::default()),
        merc_templates: Mutex::new(mercenary::icons::TemplateStore::new()),
        merc_template_generation: AtomicU64::new(0),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_status,
            ssot::get_ssot,
            ssot::set_league,
            ssot::refresh_league,
            modules::set_module_enabled,
            get_pair_code,
            get_device_id,
            regenerate_pair_code,
            set_client_txt_path,
            reset_client_txt_path,
            reset_all_settings,
            browse_client_txt,
            emit_lab_nav,
            app_log_from_frontend,
            set_server_url,
            set_sidebar_open,
            set_trade_staleness_settings,
            set_auto_trade,
            get_logs,
            get_gem_region,
            set_gem_region,
            get_font_region,
            set_font_region,
            capture_mouse_position,
            start_scanning,
            stop_scanning,
            trade_lookup,
            trade_cancel,
            send_test_gems,
            test_ocr_on_image,
            mercenary::debug::merc_debug_capture,
            mercenary::debug::merc_forget_template,
            mercenary::debug::merc_reset_templates,
            force_show_overlays,
            set_devtools,
            set_comparator_data,
            set_overlay_has_content,
            set_overlay_interactive_width,
            get_comparator_data,
            set_overlay_clickthrough,
            request_trade_refresh,
            move_overlay,
            comparator_moved,
            get_comparator_overlay_settings,
            set_comparator_overlay_settings,
            get_compass_overlay_settings,
            set_compass_overlay_settings,
            get_pathstrip_overlay_settings,
            set_pathstrip_overlay_settings,
            get_timer_overlay_settings,
            set_timer_overlay_settings,
            get_timer_appearance,
            set_timer_appearance,
            get_lab_overlays_enabled,
            set_lab_overlays_enabled,
            get_compass_settings,
            get_lab_catchup,
            set_compass_mode,
            set_compass_strategy,
            set_compass_difficulty,
            set_shrine_warn,
            get_lab_mode,
            set_lab_mode,
            get_autoclear_minutes,
            set_autoclear_minutes,
            get_dedication_pool,
            set_dedication_pool,
            get_dedication_variant,
            set_dedication_variant,
            get_normal_variant,
            set_normal_variant,
            get_show_low_confidence,
            set_show_low_confidence,
            get_ui_prefs,
            set_ui_pref,
            discard_font_session,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // Load persisted settings and apply to state.
            // If no settings file exists, write defaults so the file is always present.
            let saved = settings::load(&handle);
            let state = handle.state::<AppState>();
            settings::apply_to_state(&saved, &state);

            // If saved Client.txt path doesn't exist, re-detect
            {
                let current_path = state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if !std::path::Path::new(&current_path).exists() {
                    let detected = detect_client_txt_path();
                    app_log(&handle, format!("Saved Client.txt not found ({}), re-detected: {}", current_path, detected));
                    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = detected;
                }
            }

            // Write settings on startup so the file always exists
            // Use persist_settings to preserve window + overlay settings
            persist_settings(&handle);
            app_log(&handle, "Settings initialized".to_string());

            // Restore window position/size from saved settings.
            if let Some(ref win_settings) = saved.window {
                if let Some(win) = app.get_webview_window("main") {
                    let visible = win.available_monitors()
                        .map(|monitors| {
                            monitors.iter().any(|m| {
                                let pos = m.position();
                                let size = m.size();
                                let mx = pos.x as i32;
                                let my = pos.y as i32;
                                let mw = size.width as i32;
                                let mh = size.height as i32;
                                win_settings.x < mx + mw && win_settings.x + 100 > mx
                                    && win_settings.y < my + mh && win_settings.y + 50 > my
                            })
                        })
                        .unwrap_or(true);

                    if visible {
                        let _ = win.set_position(tauri::PhysicalPosition::new(win_settings.x, win_settings.y));
                    } else {
                        log::info!("Saved window position ({}, {}) is off-screen, centering", win_settings.x, win_settings.y);
                        let _ = win.center();
                    }
                    let _ = win.set_size(tauri::PhysicalSize::new(win_settings.width, win_settings.height));
                    if win_settings.maximized {
                        let _ = win.maximize();
                    }
                }
            }

            spawn_log_watcher(handle.clone());
            spawn_focus_poller(handle.clone());
            // Resolve the active league from the server (start-only, bounded
            // retry). Until it succeeds the SSOT stays unresolved and every
            // trade lookup fails closed — by design (POE-128 chunk 3).
            ssot::spawn_league_fetch(handle.clone());
            emit_status(&handle);
            emit_logs(&handle);
            // Modules start LAST: the owner map is effective by now
            // (`apply_to_state` above) and settings are on disk, and module
            // spawn must never delay or reorder the unconditional spawns
            // above. The `ssot-changed` nudge is best-effort here — no window
            // is listening yet, and overlays poll `get_ssot` anyway.
            modules::apply_reconcile(&handle);
            ssot::emit_ssot(&handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // The mouse hook hit-tests clicks against a cached window rect and
            // derives overlay-relative coordinates from it. Anything that moves
            // the overlay without going through `move_overlay` — a DPI or
            // resolution change, a monitor switch, the game toggling
            // fullscreen — would otherwise leave that rect stale for the rest
            // of the session, shifting both the interactive zone and every
            // click coordinate (POE-148).
            #[cfg(windows)]
            if matches!(
                event,
                tauri::WindowEvent::Moved(_)
                    | tauri::WindowEvent::Resized(_)
                    | tauri::WindowEvent::ScaleFactorChanged { .. }
            ) {
                if let Ok(hwnd) = window.hwnd() {
                    overlay_clickthrough::invalidate_if_tracked(
                        windows::Win32::Foundation::HWND(hwnd.0 as *mut _),
                    );
                }
            }
            // Clean up overlay mouse hook when comparator window is destroyed
            if let tauri::WindowEvent::Destroyed = event {
                if window.label() == "comparator" {
                    #[cfg(windows)]
                    {
                        let app = window.app_handle();
                        let state = app.state::<AppState>();
                        // Send stop signal — the hook thread will unhook and exit
                        if let Some(tx) = state.overlay_hook_stop.lock().unwrap_or_else(|e| e.into_inner()).take() {
                            let _ = tx.send(());
                        }
                        log::info!("overlay_clickthrough: cleaned up on comparator destroy");
                    }
                }
            }
            // Save window position/size on close
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if window.label() == "main" {
                    let app = window.app_handle();
                    let state = app.state::<AppState>();

                    // Latch shutdown BEFORE any stop, so a `set_module_enabled`
                    // racing the close can only stop, never respawn.
                    state.modules_shutting_down.store(true, Ordering::SeqCst);

                    // Stop all background threads before exit.
                    // Registered modules — stops everything running, including
                    // NoWindow modules the enabled flag cannot touch.
                    modules::apply_reconcile(app);
                    // Mouse hook
                    if let Some(tx) = state.overlay_hook_stop.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(());
                    }
                    // Focus poller
                    if let Some(tx) = state.focus_poller_stop.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(());
                    }
                    // Log watcher
                    if let Some(tx) = state.watcher_cancel.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(true);
                    }
                    // Gem/font scan loops — bump generations so they exit
                    state.gem_scan_generation.fetch_add(1, Ordering::SeqCst);
                    state.font_scan_generation.fetch_add(1, Ordering::SeqCst);
                    state.font_scan_live_gen.store(0, Ordering::SeqCst);

                    let is_maximized = window.is_maximized().unwrap_or(false);
                    // Only save position/size if not maximized (restore to normal position)
                    let win_settings = if is_maximized {
                        // Save maximized flag but keep last known normal position from file.
                        // AppState doesn't store window settings, so from_state returns None.
                        let existing = settings::load(app);
                        settings::WindowSettings {
                            x: existing.window.as_ref().map_or(100, |w| w.x),
                            y: existing.window.as_ref().map_or(100, |w| w.y),
                            width: existing.window.as_ref().map_or(1024, |w| w.width),
                            height: existing.window.as_ref().map_or(768, |w| w.height),
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
                    let existing = settings::load(app);
                    let mut s = settings::from_state(&state);
                    persist_overlay_settings(&existing, &mut s);
                    // Capture live overlay positions/sizes (user may have resized since last save)
                    for (label, setter) in [
                        ("compass", "compass_overlay" as &str),
                        ("pathstrip", "pathstrip_overlay"),
                        ("timer", "timer_overlay"),
                    ] {
                        if let Some(win) = app.get_webview_window(label) {
                            match (win.outer_position(), win.outer_size()) {
                                (Ok(pos), Ok(size)) => {
                                    let overlay = settings::OverlaySettings {
                                        x: pos.x,
                                        y: pos.y,
                                        width: size.width,
                                        height: size.height,
                                        enabled: match setter {
                                            "compass_overlay" => s.compass_overlay.as_ref().map_or(false, |o| o.enabled),
                                            "pathstrip_overlay" => s.pathstrip_overlay.as_ref().map_or(false, |o| o.enabled),
                                            "timer_overlay" => s.timer_overlay.as_ref().map_or(false, |o| o.enabled),
                                            _ => false,
                                        },
                                    };
                                    match setter {
                                        "compass_overlay" => s.compass_overlay = Some(overlay),
                                        "pathstrip_overlay" => s.pathstrip_overlay = Some(overlay),
                                        "timer_overlay" => s.timer_overlay = Some(overlay),
                                        _ => {}
                                    }
                                }
                                (pos, size) => {
                                    log::warn!("on-close: failed to capture {} overlay: pos={:?} size={:?}",
                                        label, pos.err(), size.err());
                                }
                            }
                        }
                    }
                    s.window = Some(win_settings); // AFTER persist_overlay, so it's not overwritten
                    settings::save(app, &s);

                    // Force exit — background threads (focus poller, mouse hook, scan loops)
                    // may still be mid-sleep and keeping the process alive.
                    std::process::exit(0);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{body_excerpt, dictionary_reject_reason, ocr_warning_field, retry_after_delay};
    use std::time::Duration;

    #[test]
    fn the_status_ocr_warning_field_reports_the_cached_warning() {
        // The status payload's only reachable seam: `build_status` needs a live
        // AppState, so this pins the field's SOURCE — a warning cached by the
        // engine path is what the payload carries, not a hardcoded None.
        //
        // Asserts against the cache rather than a literal because the cache is
        // process-wide and never cleared: another test in this binary may have
        // recorded first, and this must hold whichever write won.
        crate::ocr::record_language_warning_globally(
            "en-US unavailable — fell back to the Windows profile language",
        );
        let cached = crate::ocr::language_warning();
        assert!(cached.is_some(), "recording must leave a warning cached");
        assert_eq!(ocr_warning_field(), cached);
    }

    #[test]
    fn a_short_body_is_logged_whole() {
        assert_eq!(body_excerpt("gem is required", 200), "gem is required");
    }

    #[test]
    fn a_long_body_is_cut_to_the_limit() {
        let body = "x".repeat(500);
        assert_eq!(body_excerpt(&body, 200), "x".repeat(200));
    }

    #[test]
    fn a_cut_landing_mid_codepoint_backs_up_to_a_boundary() {
        // "é" is two bytes, so a limit of 5 lands inside the third one. Slicing
        // there panics, and panicking while logging a rejection loses it.
        assert_eq!(body_excerpt("ééé", 5), "éé");
    }

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

    #[test]
    fn retry_after_delay_honours_delta_seconds() {
        assert_eq!(retry_after_delay(Some("2")), Duration::from_secs(2));
    }

    #[test]
    fn retry_after_delay_clamps_a_long_wait_to_the_ceiling() {
        assert_eq!(retry_after_delay(Some("120")), Duration::from_secs(5));
    }

    #[test]
    fn retry_after_delay_falls_back_when_the_header_is_absent() {
        assert_eq!(retry_after_delay(None), Duration::from_secs(1));
    }

    #[test]
    fn retry_after_delay_falls_back_on_the_http_date_form() {
        assert_eq!(
            retry_after_delay(Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            Duration::from_secs(1),
        );
    }
}
