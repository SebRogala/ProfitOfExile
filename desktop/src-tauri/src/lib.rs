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
    pub logs: Mutex<Vec<String>>,
}

fn app_log(state: &AppState, msg: String) {
    let mut logs = state.logs.lock().unwrap_or_else(|e| e.into_inner());
    let timestamp = chrono::Local::now().format("%H:%M:%S");
    logs.push(format!("[{}] {}", timestamp, msg));
    // Keep last 50 entries
    if logs.len() > 50 {
        let excess = logs.len() - 50;
        logs.drain(0..excess);
    }
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
fn get_logs(state: tauri::State<AppState>) -> Vec<String> {
    state.logs.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
async fn send_test_gems(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let pair = state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let server = state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let url = format!("{}/api/desktop/gems", server);
    let gems = vec![
        "Earthquake of Fragility",
        "Boneshatter of Carnage",
        "Summon Stone Golem of Safeguarding",
    ];

    app_log(&state, format!("Sending test gems to {}", url));

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| {
            let msg = format!("HTTP client error: {}", e);
            app_log(&state, msg.clone());
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
            app_log(&state, msg.clone());
            msg
        })?;

    let status = res.status();
    app_log(&state, format!("Response: {} {}", status.as_u16(), status.canonical_reason().unwrap_or("")));

    if status.is_success() {
        let msg = "Test gems sent!".to_string();
        app_log(&state, msg.clone());
        Ok(msg)
    } else {
        let body = res.text().await.unwrap_or_else(|e| format!("<body read failed: {}>", e));
        let msg = format!("Server returned {}: {}", status, body);
        app_log(&state, msg.clone());
        Err(msg)
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
        server_url: Mutex::new(String::from("https://profitofexile.localhost")),
        detected_gems: Mutex::new(Vec::new()),
        lab_state: Mutex::new(lab_state::LabState::Idle),
        logs: Mutex::new(Vec::new()),
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
            get_logs,
            send_test_gems,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
