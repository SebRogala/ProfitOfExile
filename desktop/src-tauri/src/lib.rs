mod capture;
#[allow(dead_code)] // Font panel OCR — not yet wired into the scan pipeline
mod font_parser;
mod gem_matcher;
mod lab_state;
mod log_watcher;
mod ocr;
mod settings;
mod trade;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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
    pub pair_code: String,
    pub detected_gems: Vec<String>,
    pub client_txt_path: String,
    pub server_url: String,
    pub gem_region: CaptureRegion,
    pub font_region: CaptureRegion,
    pub sidebar_open: bool,
    pub game_focused: bool,
    pub trade_stale_warn_secs: u32,
    pub trade_stale_critical_secs: u32,
    pub trade_auto_refresh_secs: u32,
}

/// Accumulated font session data — shared between font scan loop and event handlers.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FontSessionData {
    /// Sealed rounds (one per FontOpened/CRAFT click).
    pub rounds: Vec<FontRound>,
    /// Current round's detected options (not yet sealed by FontOpened).
    /// None = no options detected yet this round.
    #[serde(skip)]
    pub current_options: Option<Vec<font_parser::CraftOption>>,
    /// Crafts remaining from the last detected options.
    #[serde(skip)]
    pub current_crafts_remaining: Option<i32>,
}

/// A sealed font round — options captured before user clicked CRAFT.
#[derive(Debug, Clone, Serialize)]
pub struct FontRound {
    pub options: Vec<font_parser::CraftOption>,
    pub crafts_remaining: Option<i32>,
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
    /// Generation counter for gem OCR scans. Incremented on each start trigger
    /// (FontOpened, manual scan). The capture loop checks this every iteration —
    /// if it doesn't match the generation it was spawned with, it exits.
    pub gem_scan_generation: AtomicU64,
    /// Generation counter for font panel OCR scans.
    pub font_scan_generation: AtomicU64,
    /// Aspirant's Trial entry count (reset on Aspirants' Plaza). Font OCR starts at 3.
    pub aspirant_trial_count: AtomicU32,
    /// Font session data — accumulated rounds, shared between font scan loop and handlers.
    pub font_session: Mutex<FontSessionData>,
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
        sidebar_open: *state.sidebar_open.lock().unwrap_or_else(|e| e.into_inner()),
        game_focused: *state.game_focused.lock().unwrap_or_else(|e| e.into_inner()),
        trade_stale_warn_secs: *state.trade_stale_warn_secs.lock().unwrap_or_else(|e| e.into_inner()),
        trade_stale_critical_secs: *state.trade_stale_critical_secs.lock().unwrap_or_else(|e| e.into_inner()),
        trade_auto_refresh_secs: *state.trade_auto_refresh_secs.lock().unwrap_or_else(|e| e.into_inner()),
    }
}

/// Save current settings to disk. Call after any persistent state change.
/// Preserves window position from the existing file (only saved on close).
fn persist_settings(app: &AppHandle) {
    let state = app.state::<AppState>();
    let existing = settings::load(app);
    let mut s = settings::from_state(&state);
    s.window = existing.window; // preserve window settings from last close
    s.comparator_overlay = existing.comparator_overlay; // preserve overlay settings
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

/// Direct trade API lookup against GGG from the desktop app.
/// Each user has their own IP → own rate limits (no shared server bottleneck).
/// Divine rate normalization is skipped for now (raw currency values returned).
#[tauri::command]
async fn trade_lookup(
    gem: String,
    variant: String,
    divine_rate: Option<f64>,
    app: AppHandle,
) -> Result<trade::TradeLookupResult, String> {
    let state = app.state::<AppState>();
    let rate = divine_rate.unwrap_or(0.0);
    app_log(&app, format!("Trade lookup: {} ({}) divine_rate={:.0}", gem, variant, rate));

    let result = state.trade_client.lookup_gem(&gem, &variant, rate).await
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

    // Fire-and-forget: submit trade result to server for cache enrichment
    {
        let server_url = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let http = state.server_http.clone();
        let submit_result = result.clone();
        tokio::spawn(async move {
            let url = format!("{}/api/trade/submit", server_url);
            match http.post(&url).json(&submit_result).send().await {
                Ok(res) if res.status().is_success() => {
                    log::info!("Trade result submitted to server for {} ({})", submit_result.gem, submit_result.variant);
                }
                Ok(res) => {
                    log::warn!("Trade submit rejected by server: {}", res.status());
                }
                Err(e) => {
                    log::warn!("Trade submit to server failed: {}", e);
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
// windows intercept mouse input before the parent sees it.
//
// Solution (same approach as Electron's setIgnoreMouseEvents + forward):
//   1. Set WS_EX_TRANSPARENT | WS_EX_LAYERED on the overlay window via
//      Tauri's set_ignore_cursor_events(true) — makes entire window click-through.
//   2. Install a global WH_MOUSE_LL hook to track cursor position.
//   3. When cursor enters the interactive button column on the right edge,
//      call set_ignore_cursor_events(false) so buttons become clickable.
//   4. When cursor leaves, call set_ignore_cursor_events(true) again.
//   5. Set WS_EX_NOACTIVATE on all HWNDs so clicking buttons doesn't steal
//      focus from the game.
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
#[cfg(windows)]
mod overlay_clickthrough {
    use std::sync::atomic::{AtomicI32, AtomicIsize, AtomicBool, Ordering};
    use std::sync::Mutex as StdMutex;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, BOOL, RECT};
    use windows::Win32::UI::WindowsAndMessaging::*;

    struct SendHook(HHOOK);
    unsafe impl Send for SendHook {}

    static HOOK_HANDLE: StdMutex<Option<SendHook>> = StdMutex::new(None);
    static OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);
    static WIN_RECT: StdMutex<(i32, i32, i32, i32)> = StdMutex::new((0, 0, 0, 0));
    static RECT_DIRTY: AtomicBool = AtomicBool::new(true);
    /// Width of interactive zone on the right edge (physical pixels).
    static INTERACTIVE_WIDTH: AtomicI32 = AtomicI32::new(48);
    /// Click buffer — hook pushes overlay-relative coordinates, message loop drains and emits events.
    static CLICK_BUFFER: StdMutex<Vec<(i32, i32)>> = StdMutex::new(Vec::new());

    /// Check if cursor is in the interactive zone. Also refreshes cached rect if dirty.
    fn check_interactive(cx: i32, cy: i32) -> bool {
        let hwnd_val = OVERLAY_HWND.load(Ordering::Relaxed);
        if hwnd_val == 0 { return false; }

        unsafe {
            let hwnd = HWND(hwnd_val as *mut _);
            if RECT_DIRTY.swap(false, Ordering::Relaxed) {
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    if let Ok(mut wr) = WIN_RECT.lock() {
                        *wr = (rect.left, rect.top, rect.right, rect.bottom);
                    }
                } else {
                    RECT_DIRTY.store(true, Ordering::Relaxed);
                }
            }
        }

        let (left, top, right, bottom) = *WIN_RECT.lock().unwrap_or_else(|e| e.into_inner());
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

            // WebView2 may strip WS_EX_TRANSPARENT when creating/updating child
            // windows. Re-apply on every mouse event to keep the overlay fully
            // click-through.
            let hwnd_val = OVERLAY_HWND.load(Ordering::Relaxed);
            if hwnd_val != 0 {
                let hwnd = HWND(hwnd_val as *mut _);
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
                if ex & WS_EX_TRANSPARENT.0 as i32 == 0 {
                    SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_TRANSPARENT.0 as i32);
                }
            }

            if check_interactive(cx, cy) {
                let msg_id = w_param.0 as u32;
                // Consume clicks in the interactive zone — don't pass to game.
                if msg_id == WM_LBUTTONDOWN || msg_id == WM_LBUTTONUP {
                    if msg_id == WM_LBUTTONDOWN {
                        // Buffer overlay-relative coordinates for the message loop to emit.
                        let (left, top, _, _) = *WIN_RECT.lock().unwrap_or_else(|e| e.into_inner());
                        if let Ok(mut buf) = CLICK_BUFFER.lock() {
                            buf.push((cx - left, cy - top));
                        }
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
                loop {
                    if stop_rx.try_recv().is_ok() { break; }

                    // Drain click buffer → emit Tauri events.
                    if let Ok(mut buf) = CLICK_BUFFER.lock() {
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
        RECT_DIRTY.store(true, Ordering::Relaxed);
    }

    pub fn set_interactive_width(px: i32) {
        INTERACTIVE_WIDTH.store(px, Ordering::Relaxed);
    }

    pub fn invalidate_rect() {
        RECT_DIRTY.store(true, Ordering::Relaxed);
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

                overlay_clickthrough::set_interactive_width(interactive_width);
                overlay_clickthrough::set_overlay_hwnd(h);

                if let Some(tx) = overlay_clickthrough::install_hook(app2.clone()) {
                    let state = app2.state::<AppState>();
                    *state.overlay_hook_stop.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
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
        log::info!("Debug mode ON — overlays force-shown");
    } else {
        log::info!("Debug mode OFF");
    }
}

#[tauri::command]
fn set_comparator_data(payload: serde_json::Value, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.comparator_data.lock().unwrap_or_else(|e| e.into_inner()) = payload;
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
    // Save position
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
    let http = state.server_http.clone();
    let gem_names = fetch_gem_names(&app, &server, &http).await;
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
    let gem_names = tauri::async_runtime::block_on(fetch_gem_names(&app, &server, &http));
    if gem_names.is_empty() {
        app_log(&app, "Gem scan aborted — no gem names loaded (server unreachable?)".to_string());
        if state.gem_scan_generation.load(Ordering::SeqCst) == generation {
            *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) = lab_state::LabState::Idle;
            emit_status(&app);
        }
        return;
    }
    let matcher = gem_matcher::GemMatcher::new(gem_names.clone());
    app_log(&app, format!("Gem scan: loaded {} gem names", gem_names.len()));

    let mut seen_gems: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut gems_found = 0u32;
    let mut loop_count = 0u32;
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
                gems_found += 1;
                app_log(&app, format!(
                    "Gem detected: {} (score: {:.2}) [{}/{}]",
                    gem_match.name, gem_match.score, gems_found, MAX_GEMS
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
    let gen = state.font_scan_generation.fetch_add(1, Ordering::SeqCst) + 1;

    // Reset font session for the new scan.
    *state.font_session.lock().unwrap_or_else(|e| e.into_inner()) = FontSessionData::default();

    app_log(app, format!("Font scan started (gen={})", gen));

    let app_capture = app.clone();
    std::thread::spawn(move || {
        font_scan_loop(app_capture, gen);
    });
}

/// Font panel OCR loop on a dedicated OS thread.
///
/// Scans the font region every 250ms looking for craft options (CRAFT screen).
/// Stores detected options in AppState.font_session. Stops when:
///   - Generation mismatch (zone change, manual stop, new scan started)
///   - Last craft completed (no "Crafts Remaining" seen → FontOpened seals it → done)
fn font_scan_loop(app: AppHandle, generation: u64) {
    let state = app.state::<AppState>();
    let mut loop_count = 0u32;
    let start = std::time::Instant::now();
    const SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300); // 5 min safety net

    loop {
        // Check generation.
        if state.font_scan_generation.load(Ordering::SeqCst) != generation {
            app_log(&app, "Font scan stopped (generation mismatch)".to_string());
            break;
        }

        if start.elapsed() >= TIMEOUT {
            app_log(&app, "Font scan timed out after 5 minutes".to_string());
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

        if panel.font_active && !panel.options.is_empty() {
            // Check dedup and update session under lock, then log/emit outside.
            let is_new = {
                let mut session = state.font_session.lock().unwrap_or_else(|e| e.into_inner());
                let new_types: Vec<&str> = panel.options.iter().map(|o| o.option_type.as_str()).collect();
                let same = session.current_options.as_ref().map_or(false, |existing| {
                    let existing_types: Vec<&str> = existing.iter().map(|o| o.option_type.as_str()).collect();
                    existing_types == new_types
                });
                if !same {
                    session.current_options = Some(panel.options.clone());
                    session.current_crafts_remaining = panel.crafts_remaining;
                    true
                } else {
                    false
                }
            }; // lock released

            if is_new {
                app_log(&app, format!(
                    "Font options captured: {} options{}{}",
                    panel.options.len(),
                    if panel.jackpot_detected { " *** JACKPOT! ***" } else { "" },
                    panel.crafts_remaining.map_or(
                        " (last craft)".to_string(),
                        |n| format!(" (remaining: {})", n),
                    ),
                ));
                for opt in &panel.options {
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
}

/// Called by FontOpened handler to seal the current font round.
/// Moves current_options into the rounds list. Returns true if this was the last craft.
fn seal_font_round(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let mut session = state.font_session.lock().unwrap_or_else(|e| e.into_inner());

    let options = match session.current_options.take() {
        Some(opts) => opts,
        None => {
            app_log(app, "Font round seal skipped — OCR did not capture options before CRAFT click".to_string());
            return false;
        }
    };

    let crafts_remaining = session.current_crafts_remaining.take();
    let is_last = crafts_remaining.is_none();

    let round_num = session.rounds.len() + 1;
    app_log(app, format!(
        "Font round {} sealed ({} options{})",
        round_num,
        options.len(),
        if is_last { ", last craft" } else { "" },
    ));

    session.rounds.push(FontRound {
        options,
        crafts_remaining,
    });

    is_last
}

/// Send accumulated font session to the server and reset.
fn send_font_session_data(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut session = state.font_session.lock().unwrap_or_else(|e| e.into_inner());

    // Seal any unsent current round (user left without clicking CRAFT).
    if let Some(options) = session.current_options.take() {
        let crafts_remaining = session.current_crafts_remaining.take();
        session.rounds.push(FontRound { options, crafts_remaining });
    }

    if session.rounds.is_empty() {
        return;
    }

    let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();

    let session_data = serde_json::json!({
        "lab_type": "Unknown",
        "total_crafts": session.rounds.len(),
        "variant": "20/20",
        "device_id": "desktop",
        "pair_code": pair,
        "rounds": session.rounds.iter().map(|r| serde_json::json!({
            "craft_options": r.options,
            "crafts_remaining": r.crafts_remaining,
        })).collect::<Vec<_>>(),
    });

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
                app_log(&app_clone, format!("Font session rejected: {} — {}", status, &body[..body.len().min(200)]));
            }
            Err(e) => {
                app_log(&app_clone, format!("Font session send failed: {}", e));
            }
        }
    });
}

/// Fetch gem names from the server API for fuzzy matching.
async fn fetch_gem_names(app: &AppHandle, server_url: &str, client: &reqwest::Client) -> Vec<String> {
    let url = format!("{}/api/analysis/gems/names?q=of+&limit=500", server_url);
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
                use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

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
                            let mut buf = [0u16; 256];
                            let len = GetWindowTextW(fg, &mut buf);
                            if len > 0 {
                                let title = String::from_utf16_lossy(&buf[..len as usize]);
                                if title.contains("Path of Exile") {
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

                    // Hide/show overlay windows (skip hide in debug mode)
                    let debug = *state.debug_mode.lock().unwrap_or_else(|e| e.into_inner());
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
        let _matcher = gem_matcher::GemMatcher::new(vec![]); // TODO: fetch from server

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
                    let preview = if line.len() > 60 { &line[..60] } else { &line };
                    app_log(&app, format!("Log: {}", preview));

                    // --- Aspirant's Trial / Plaza tracking (outside state machine) ---
                    if line.contains("You have entered") {
                        let state = app.state::<AppState>();
                        if line.contains("Aspirants' Plaza") || line.contains("Aspirant's Plaza") {
                            state.aspirant_trial_count.store(0, Ordering::SeqCst);
                            app_log(&app, "Aspirants' Plaza — trial counter reset".to_string());
                        } else if line.contains("Aspirant's Trial") {
                            let count = state.aspirant_trial_count.fetch_add(1, Ordering::SeqCst) + 1;
                            app_log(&app, format!("Aspirant's Trial #{}", count));
                            if count >= 3 {
                                app_log(&app, "3rd+ Aspirant's Trial — starting font panel OCR".to_string());
                                spawn_font_scan(&app);
                            }
                        }
                    }

                    if let Some(event) = state_machine.process_line(&line) {
                        let state = app.state::<AppState>();
                        match &event {
                            lab_state::LabEvent::FontOpened => {
                                // FontOpened = user clicked CRAFT → 3 gems on screen.
                                // Always (re)start gem scan — bumps generation to cancel
                                // any running scan, clears comparator, spawns fresh scan.
                                detected_gems.clear();
                                spawn_gem_scan(&app, "font");

                                // Seal the current font round (options captured by font scan).
                                let is_last = seal_font_round(&app);
                                if is_last {
                                    // No "Crafts Remaining" was seen → this was the last craft.
                                    // Stop font scan — no more options to capture.
                                    app_log(&app, "Last font craft — stopping font scan".to_string());
                                    state.font_scan_generation.fetch_add(1, Ordering::SeqCst);
                                }
                            }
                            lab_state::LabEvent::ZoneChanged { area } => {
                                app_log(&app, format!("Zone changed: {} — stopping", area));

                                // Stop both gem and font scans.
                                state.gem_scan_generation.fetch_add(1, Ordering::SeqCst);
                                state.font_scan_generation.fetch_add(1, Ordering::SeqCst);
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

    let pair_code = generate_pair_code();
    log::info!("Pair code: {}", pair_code);

    let server_http = reqwest::Client::new();

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
        sidebar_open: Mutex::new(true),
        game_focused: Mutex::new(false),
        trade_client: trade::TradeApiClient::new("Mirage"),
        server_http,
        watcher_cancel: Mutex::new(None),
        comparator_data: Mutex::new(serde_json::json!({"results":[],"tradeData":{}})),
        overlay_hook_stop: Mutex::new(None),
        focus_poller_stop: Mutex::new(None),
        debug_mode: Mutex::new(false),
        trade_stale_warn_secs: Mutex::new(settings::DEFAULT_TRADE_STALE_WARN_SECS),
        trade_stale_critical_secs: Mutex::new(settings::DEFAULT_TRADE_STALE_CRITICAL_SECS),
        trade_auto_refresh_secs: Mutex::new(settings::DEFAULT_TRADE_AUTO_REFRESH_SECS),
        gem_scan_generation: AtomicU64::new(0),
        font_scan_generation: AtomicU64::new(0),
        aspirant_trial_count: AtomicU32::new(0),
        font_session: Mutex::new(FontSessionData::default()),
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
            set_sidebar_open,
            set_trade_staleness_settings,
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
            force_show_overlays,
            set_comparator_data,
            get_comparator_data,
            set_overlay_clickthrough,
            request_trade_refresh,
            move_overlay,
            comparator_moved,
            get_comparator_overlay_settings,
            set_comparator_overlay_settings,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // Load persisted settings and apply to state.
            // If no settings file exists, write defaults so the file is always present.
            let saved = settings::load(&handle);
            let state = handle.state::<AppState>();
            settings::apply_to_state(&saved, &state);
            // Write settings on startup so the file always exists
            // Use persist_settings to preserve window + overlay settings
            persist_settings(&handle);
            app_log(&handle, "Settings initialized".to_string());

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
            spawn_focus_poller(handle.clone());
            emit_status(&handle);
            emit_logs(&handle);
            Ok(())
        })
        .on_window_event(|window, event| {
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
                    let existing = settings::load(app);
                    let mut s = settings::from_state(&state);
                    s.window = Some(win_settings);
                    s.comparator_overlay = existing.comparator_overlay;
                    settings::save(app, &s);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
