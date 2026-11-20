//! Client.txt discovery (POE-443): auto-detecting the game's log file and the
//! commands that set, reset and browse for its path.

use tauri::{AppHandle, Manager};

use crate::{app_log, emit_status, persist_settings, restart_log_watcher, AppState};

/// Detect Client.txt path using multiple strategies (most reliable first).
/// 1. Running PoE process → derive logs path from executable location
/// 2. Hardcoded common paths (GGG standalone, Steam default, Epic Games)
pub(crate) fn detect_client_txt_path() -> String {
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
pub(crate) fn set_client_txt_path(path: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = path;
    persist_settings(&app);
    emit_status(&app);
    restart_log_watcher(app);
}

#[tauri::command]
pub(crate) fn reset_client_txt_path(app: AppHandle) {
    let state = app.state::<AppState>();
    let detected = detect_client_txt_path();
    app_log(&app, format!("Auto-detect Client.txt: {}", detected));
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = detected;
    persist_settings(&app);
    emit_status(&app);
    restart_log_watcher(app);
}

#[tauri::command]
pub(crate) async fn browse_client_txt(app: AppHandle) -> Result<String, String> {
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
