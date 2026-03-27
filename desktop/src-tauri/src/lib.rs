mod gem_matcher;
mod lab_state;
mod log_watcher;

use serde::Serialize;
use std::sync::Mutex;
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub state: String,
    pub pair_code: String,
    pub detected_gems: Vec<String>,
    pub client_txt_path: String,
    pub server_url: String,
}

pub struct AppState {
    pub pair_code: Mutex<String>,
    pub client_txt_path: Mutex<String>,
    pub server_url: Mutex<String>,
    pub detected_gems: Mutex<Vec<String>>,
    pub lab_state: Mutex<lab_state::LabState>,
}

fn generate_pair_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
    (0..4).map(|_| chars[rng.gen_range(0..chars.len())]).collect()
}

#[tauri::command]
fn get_status(state: tauri::State<AppState>) -> AppStatus {
    AppStatus {
        state: format!("{:?}", *state.lab_state.lock().unwrap_or_else(|e| e.into_inner())),
        pair_code: state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        detected_gems: state.detected_gems.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        client_txt_path: state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        server_url: state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }
}

#[tauri::command]
fn get_pair_code(state: tauri::State<AppState>) -> String {
    state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
fn regenerate_pair_code(state: tauri::State<AppState>) -> String {
    let new_code = generate_pair_code();
    *state.pair_code.lock().unwrap_or_else(|e| e.into_inner()) = new_code.clone();
    log::info!("New pair code: {}", new_code);
    new_code
}

#[tauri::command]
fn set_client_txt_path(path: String, state: tauri::State<AppState>) {
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = path;
}

#[tauri::command]
fn set_server_url(url: String, state: tauri::State<AppState>) {
    *state.server_url.lock().unwrap_or_else(|e| e.into_inner()) = url;
}

#[tauri::command]
async fn send_test_gems(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let gems = vec![
        "Earthquake of Fragility",
        "Boneshatter of Carnage",
        "Summon Stone Golem of Safeguarding",
    ];

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{}/api/desktop/gems", server))
        .json(&serde_json::json!({
            "pair": pair,
            "gems": gems,
            "variant": "20/20"
        }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if res.status().is_success() {
        Ok("Test gems sent!".to_string())
    } else {
        let status = res.status();
        let body = res.text().await.unwrap_or_else(|e| format!("<body read failed: {}>", e));
        Err(format!("Server returned {}: {}", status, body))
    }
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
        server_url: Mutex::new(String::from("http://profitofexile.localhost")),
        detected_gems: Mutex::new(Vec::new()),
        lab_state: Mutex::new(lab_state::LabState::Idle),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_pair_code,
            regenerate_pair_code,
            set_client_txt_path,
            set_server_url,
            send_test_gems,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
