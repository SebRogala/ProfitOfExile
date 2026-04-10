//! Persistent settings — saved to JSON in the Tauri app data directory.
//!
//! Loaded on startup, saved on every change. Settings that aren't in the file
//! use defaults (forward-compatible with new fields).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

use crate::CaptureRegion;

const SETTINGS_FILENAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub client_txt_path: String,
    pub server_url: String,
    pub gem_region: CaptureRegion,
    pub font_region: CaptureRegion,
    pub window: Option<WindowSettings>,
    pub sidebar_open: bool,
    pub comparator_overlay: Option<OverlaySettings>,
    pub compass_overlay: Option<OverlaySettings>,
    pub pathstrip_overlay: Option<OverlaySettings>,
    pub timer_overlay: Option<OverlaySettings>,
    /// Master toggle for all lab overlays (compass + pathstrip + timer).
    pub lab_overlays_enabled: bool,
    /// Yellow indicator threshold for trade data age (seconds).
    pub trade_stale_warn_secs: u32,
    /// Red indicator threshold for trade data age (seconds).
    pub trade_stale_critical_secs: u32,
    /// Auto-refresh trade data after this many seconds.
    pub trade_auto_refresh_secs: u32,
    /// Whether auto-trade is enabled (fetch trade data automatically on compare).
    pub auto_trade_enabled: bool,
    pub compass_mode: String,
    pub compass_strategy: String,
    pub compass_difficulty: String,
    pub shrine_warn_enabled: bool,
    pub shrine_warn_size: String,
    pub shrine_warn_corner: String,
    pub shrine_warn_on_take: String,
    /// Timer overlay background opacity (0.0–1.0, default 0.75).
    pub timer_bg_opacity: Option<f32>,
    /// Timer overlay text stroke/outline enabled (default true).
    pub timer_text_stroke: Option<bool>,
}

pub const DEFAULT_TRADE_STALE_WARN_SECS: u32 = 120;
pub const DEFAULT_TRADE_STALE_CRITICAL_SECS: u32 = 600;
pub const DEFAULT_TRADE_AUTO_REFRESH_SECS: u32 = 900;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlaySettings {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSettings {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            client_txt_path: crate::detect_client_txt_path(),
            server_url: String::from(option_env!("POE_SERVER_URL").unwrap_or("https://profitofexile.localhost")),
            gem_region: CaptureRegion::default(),
            font_region: CaptureRegion::default_font_panel(),
            window: None,
            sidebar_open: true,
            comparator_overlay: None,
            compass_overlay: None,
            pathstrip_overlay: None,
            timer_overlay: None,
            lab_overlays_enabled: true,
            trade_stale_warn_secs: DEFAULT_TRADE_STALE_WARN_SECS,
            trade_stale_critical_secs: DEFAULT_TRADE_STALE_CRITICAL_SECS,
            trade_auto_refresh_secs: DEFAULT_TRADE_AUTO_REFRESH_SECS,
            auto_trade_enabled: false,
            compass_mode: String::from("minimap"),
            compass_strategy: String::from("shortest"),
            compass_difficulty: String::from("Uber"),
            shrine_warn_enabled: true,
            shrine_warn_size: String::from("medium"),
            shrine_warn_corner: String::from("bottom-right"),
            shrine_warn_on_take: String::from("green"),
            timer_bg_opacity: None,
            timer_text_stroke: None,
        }
    }
}

/// Get the settings file path inside the Tauri app data directory.
pub fn settings_path_pub(app: &tauri::AppHandle) -> Option<PathBuf> {
    settings_path(app)
}

fn settings_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(e) => {
            log::error!("Cannot resolve app data directory: {}", e);
            return None;
        }
    };
    if let Err(e) = fs::create_dir_all(&dir) {
        log::error!("Cannot create settings directory {:?}: {}", dir, e);
        return None;
    }
    Some(dir.join(SETTINGS_FILENAME))
}

/// Load settings from disk. Returns defaults if file doesn't exist or is invalid.
pub fn load(app: &tauri::AppHandle) -> Settings {
    let path = match settings_path(app) {
        Some(p) => p,
        None => return Settings::default(),
    };
    match fs::read_to_string(&path) {
        Ok(contents) => {
            match serde_json::from_str::<Settings>(&contents) {
                Ok(s) => {
                    log::info!("Settings loaded from {:?}", path);
                    s
                }
                Err(e) => {
                    log::warn!("Settings file invalid, using defaults: {}", e);
                    Settings::default()
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::info!("No settings file found, using defaults");
            Settings::default()
        }
        Err(e) => {
            log::error!("Failed to read settings file {:?}: {} — using defaults", path, e);
            Settings::default()
        }
    }
}

/// Save current settings to disk.
pub fn save(app: &tauri::AppHandle, settings: &Settings) {
    let path = match settings_path(app) {
        Some(p) => p,
        None => return,
    };
    match serde_json::to_string_pretty(settings) {
        Ok(json) => {
            if let Err(e) = fs::write(&path, &json) {
                log::error!("Failed to write settings to {:?}: {}", path, e);
            }
        }
        Err(e) => {
            log::error!("Failed to serialize settings: {}", e);
        }
    }
}

/// Build a Settings struct from the current AppState.
pub fn from_state(state: &crate::AppState) -> Settings {
    Settings {
        client_txt_path: state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        server_url: state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        gem_region: state.gem_region.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        font_region: state.font_region.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        window: None, // Window settings are saved separately on close, not from AppState
        sidebar_open: *state.sidebar_open.lock().unwrap_or_else(|e| e.into_inner()),
        comparator_overlay: None, // Overlay settings saved separately, not from AppState
        compass_overlay: None,    // Overlay settings saved separately, not from AppState
        pathstrip_overlay: None,  // Overlay settings saved separately, not from AppState
        timer_overlay: None,     // Overlay settings saved separately, not from AppState
        lab_overlays_enabled: *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()),
        trade_stale_warn_secs: *state.trade_stale_warn_secs.lock().unwrap_or_else(|e| e.into_inner()),
        trade_stale_critical_secs: *state.trade_stale_critical_secs.lock().unwrap_or_else(|e| e.into_inner()),
        trade_auto_refresh_secs: *state.trade_auto_refresh_secs.lock().unwrap_or_else(|e| e.into_inner()),
        auto_trade_enabled: *state.auto_trade_enabled.lock().unwrap_or_else(|e| e.into_inner()),
        compass_mode: state.compass_mode.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        compass_strategy: state.compass_strategy.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        compass_difficulty: state.compass_difficulty.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        shrine_warn_enabled: *state.shrine_warn_enabled.lock().unwrap_or_else(|e| e.into_inner()),
        shrine_warn_size: state.shrine_warn_size.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        shrine_warn_corner: state.shrine_warn_corner.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        shrine_warn_on_take: state.shrine_warn_on_take.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        timer_bg_opacity: None,    // Appearance settings saved separately, not from AppState
        timer_text_stroke: None,   // Appearance settings saved separately, not from AppState
    }
}

/// Copy overlay/window settings from existing file into the new settings struct.
/// These fields are managed by their own save commands, not by AppState.
pub fn persist_overlay_settings(existing: &Settings, target: &mut Settings) {
    target.window = existing.window.clone();
    target.comparator_overlay = existing.comparator_overlay.clone();
    target.compass_overlay = existing.compass_overlay.clone();
    target.pathstrip_overlay = existing.pathstrip_overlay.clone();
    target.timer_overlay = existing.timer_overlay.clone();
    target.timer_bg_opacity = existing.timer_bg_opacity;
    target.timer_text_stroke = existing.timer_text_stroke;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Overlay settings must survive the from_state→persist_overlay→save cycle.
    /// Regression test: from_state returns None for overlays (they're not in AppState),
    /// so persist_overlay_settings must copy them from the existing file.
    #[test]
    fn test_overlay_settings_survive_persist_cycle() {
        let existing = Settings {
            compass_overlay: Some(OverlaySettings { x: 50, y: 60, width: 400, height: 350, enabled: true }),
            pathstrip_overlay: Some(OverlaySettings { x: 100, y: 200, width: 500, height: 200, enabled: true }),
            comparator_overlay: Some(OverlaySettings { x: 10, y: 20, width: 630, height: 250, enabled: false }),
            timer_overlay: Some(OverlaySettings { x: 200, y: 500, width: 160, height: 50, enabled: true }),
            ..Settings::default()
        };

        // Simulate from_state (overlays are None)
        let mut target = Settings::default();
        assert!(target.compass_overlay.is_none());
        assert!(target.pathstrip_overlay.is_none());
        assert!(target.timer_overlay.is_none());

        // persist_overlay_settings must restore them
        super::persist_overlay_settings(&existing, &mut target);

        let compass = target.compass_overlay.expect("compass_overlay lost during persist cycle");
        assert_eq!(compass.x, 50);
        assert_eq!(compass.y, 60);
        assert_eq!(compass.width, 400);
        assert_eq!(compass.height, 350);
        assert!(compass.enabled);

        let pathstrip = target.pathstrip_overlay.expect("pathstrip_overlay lost during persist cycle");
        assert_eq!(pathstrip.x, 100);
        assert_eq!(pathstrip.width, 500);

        let comparator = target.comparator_overlay.expect("comparator_overlay lost during persist cycle");
        assert_eq!(comparator.x, 10);
        assert!(!comparator.enabled);

        let timer = target.timer_overlay.expect("timer_overlay lost during persist cycle");
        assert_eq!(timer.x, 200);
        assert_eq!(timer.width, 160);
        assert!(timer.enabled);
    }

    /// Window settings must not overwrite overlay settings in the save cycle.
    /// Regression guard: if persist_overlay_settings were ever called AFTER
    /// s.window = Some(...), it would overwrite the freshly set window settings
    /// with the file's stale version. Correct order: persist_overlay THEN set window.
    #[test]
    fn test_window_settings_not_overwritten_by_overlay_persist() {
        let existing = Settings {
            window: Some(WindowSettings { x: 0, y: 0, width: 800, height: 600, maximized: false }),
            compass_overlay: Some(OverlaySettings { x: 50, y: 60, width: 400, height: 350, enabled: true }),
            ..Settings::default()
        };

        let mut target = Settings::default();
        // Simulate the on-close save order: persist_overlay THEN set window
        super::persist_overlay_settings(&existing, &mut target);
        target.window = Some(WindowSettings { x: 200, y: 300, width: 1024, height: 768, maximized: false });

        // Window should be the NEW value, not the existing file value
        let win = target.window.expect("window settings lost");
        assert_eq!(win.x, 200);
        assert_eq!(win.width, 1024);

        // Overlay should still be from existing file
        let compass = target.compass_overlay.expect("compass_overlay lost");
        assert_eq!(compass.x, 50);
    }
}

/// Apply loaded settings to AppState.
pub fn apply_to_state(settings: &Settings, state: &crate::AppState) {
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = settings.client_txt_path.clone();
    *state.server_url.lock().unwrap_or_else(|e| e.into_inner()) = settings.server_url.clone();
    *state.gem_region.lock().unwrap_or_else(|e| e.into_inner()) = settings.gem_region.clone();
    *state.font_region.lock().unwrap_or_else(|e| e.into_inner()) = settings.font_region.clone();
    *state.sidebar_open.lock().unwrap_or_else(|e| e.into_inner()) = settings.sidebar_open;
    *state.trade_stale_warn_secs.lock().unwrap_or_else(|e| e.into_inner()) = settings.trade_stale_warn_secs;
    *state.trade_stale_critical_secs.lock().unwrap_or_else(|e| e.into_inner()) = settings.trade_stale_critical_secs;
    *state.trade_auto_refresh_secs.lock().unwrap_or_else(|e| e.into_inner()) = settings.trade_auto_refresh_secs;
    *state.auto_trade_enabled.lock().unwrap_or_else(|e| e.into_inner()) = settings.auto_trade_enabled;
    *state.compass_mode.lock().unwrap_or_else(|e| e.into_inner()) = settings.compass_mode.clone();
    *state.compass_strategy.lock().unwrap_or_else(|e| e.into_inner()) = settings.compass_strategy.clone();
    *state.compass_difficulty.lock().unwrap_or_else(|e| e.into_inner()) = settings.compass_difficulty.clone();
    *state.shrine_warn_enabled.lock().unwrap_or_else(|e| e.into_inner()) = settings.shrine_warn_enabled;
    *state.shrine_warn_size.lock().unwrap_or_else(|e| e.into_inner()) = settings.shrine_warn_size.clone();
    *state.shrine_warn_corner.lock().unwrap_or_else(|e| e.into_inner()) = settings.shrine_warn_corner.clone();
    *state.shrine_warn_on_take.lock().unwrap_or_else(|e| e.into_inner()) = settings.shrine_warn_on_take.clone();
    *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) = settings.lab_overlays_enabled;
}
