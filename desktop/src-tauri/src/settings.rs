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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            client_txt_path: crate::DEFAULT_CLIENT_TXT_PATH.to_string(),
            server_url: String::from("https://poe.softsolution.pro"),
            gem_region: CaptureRegion::default(),
            font_region: CaptureRegion::default_font_panel(),
        }
    }
}

/// Get the settings file path inside the Tauri app data directory.
fn settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join(SETTINGS_FILENAME)
}

/// Load settings from disk. Returns defaults if file doesn't exist or is invalid.
pub fn load(app: &tauri::AppHandle) -> Settings {
    let path = settings_path(app);
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
        Err(_) => {
            log::info!("No settings file found, using defaults");
            Settings::default()
        }
    }
}

/// Save current settings to disk.
pub fn save(app: &tauri::AppHandle, settings: &Settings) {
    let path = settings_path(app);
    match serde_json::to_string_pretty(settings) {
        Ok(json) => {
            if let Err(e) = fs::write(&path, json) {
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
    }
}

/// Apply loaded settings to AppState.
pub fn apply_to_state(settings: &Settings, state: &crate::AppState) {
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = settings.client_txt_path.clone();
    *state.server_url.lock().unwrap_or_else(|e| e.into_inner()) = settings.server_url.clone();
    *state.gem_region.lock().unwrap_or_else(|e| e.into_inner()) = settings.gem_region.clone();
    *state.font_region.lock().unwrap_or_else(|e| e.into_inner()) = settings.font_region.clone();
}
