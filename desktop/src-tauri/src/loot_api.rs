//! Device-authenticated boss loot transport. The app's HTTP client owns headers.
use serde::{Deserialize, Serialize};
use crate::loot_profiles::{LootProfile, Selection};
use tauri::Manager;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRef { pub boss: String, pub profile_key: String, pub profile_version: u32 }
impl From<&LootProfile> for ProfileRef {
    fn from(p: &LootProfile) -> Self { Self { boss: p.boss.clone(), profile_key: p.key.clone(), profile_version: p.version } }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunItems { pub quantities: Selection, pub counted: Vec<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRun {
    #[serde(flatten)] pub profile: ProfileRef,
    pub id: String, pub league: String, pub run: u64, pub date: Option<String>,
    pub seed: Option<String>, pub version: u64, pub items: RunItems,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target { pub id: String, pub expected_version: u64 }
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitRun {
    #[serde(flatten)] pub profile: ProfileRef,
    pub league: String, pub seed: Option<String>, pub mode: String, pub items: RunItems,
    #[serde(skip_serializing_if = "Option::is_none")] pub target: Option<Target>,
    #[serde(skip_serializing_if = "Option::is_none")] pub incoming_positive_keys: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitOutcome { pub state: String, pub run: StoredRun, pub dropped_positive_keys: Vec<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunsPage { pub total: u64, pub page: u32, pub page_size: u32, pub items: Vec<StoredRun> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStats { pub drops: u64, pub eligible_runs: u64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BossStats { pub runs: u64, pub devices: u64, pub next_run: u64, pub items: std::collections::BTreeMap<String, ItemStats> }

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ApiError { Conflict, Unreachable, Rejected(String), Cancelled }
/// A registry receiver belongs to one module generation. A new generation
/// gets a new channel, so re-enabling never revives cancelled capture work.
pub fn work_cancelled(cancel: &tokio::sync::watch::Receiver<bool>) -> bool {
    *cancel.borrow() || cancel.has_changed().is_err()
}
pub trait LootTransport: Sync {
    fn submit(&self, request: &SubmitRun) -> impl std::future::Future<Output = Result<SubmitOutcome, ApiError>> + Send;
    fn latest(&self, profile: &ProfileRef) -> impl std::future::Future<Output = Result<Option<StoredRun>, ApiError>> + Send;
}
#[derive(Clone)]
pub struct LootApi { http: reqwest::Client, base: String, cancel: Option<tokio::sync::watch::Receiver<bool>> }
impl LootApi {
    pub fn from_state(state: &crate::AppState) -> Self {
        Self { http: state.server_http.clone(), base: state.server_url.lock().unwrap_or_else(|e| e.into_inner()).trim_end_matches('/').into(), cancel: None }
    }
    pub fn with_cancel(mut self, cancel: tokio::sync::watch::Receiver<bool>) -> Self {
        self.cancel = Some(cancel); self
    }
    fn check_cancel(&self) -> Result<(), ApiError> {
        if self.cancel.as_ref().is_some_and(work_cancelled) { Err(ApiError::Cancelled) } else { Ok(()) }
    }
    async fn send<T: serde::de::DeserializeOwned>(&self, request: reqwest::RequestBuilder) -> Result<T, ApiError> {
        self.check_cancel()?;
        let result = decode(request).await;
        self.check_cancel()?;
        result
    }
    pub async fn capture_context(&self, profile: &ProfileRef) -> Result<(Option<StoredRun>, u64), ApiError> {
        Ok((self.latest(profile).await?, self.stats(profile, "own").await?.next_run))
    }
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http.request(method, format!("{}/api/boss-loot/{path}", self.base)).timeout(std::time::Duration::from_secs(15))
    }
    fn query(profile: &ProfileRef) -> Vec<(&'static str, String)> {
        vec![("boss", profile.boss.clone()), ("profileKey", profile.profile_key.clone()), ("profileVersion", profile.profile_version.to_string())]
    }
    pub async fn list(&self, profile: &ProfileRef, page: u32, size: u32) -> Result<RunsPage, ApiError> {
        if page == 0 || page > 1_000_000 || size == 0 || size > 200 { return Err(ApiError::Rejected("invalid pagination".into())); }
        let mut query = Self::query(profile);
        query.extend([("page", page.to_string()), ("pageSize", size.to_string())]);
        self.send(self.request(reqwest::Method::GET, "runs").query(&query)).await
    }
    pub async fn stats(&self, profile: &ProfileRef, scope: &str) -> Result<BossStats, ApiError> {
        if !matches!(scope, "own" | "collective") { return Err(ApiError::Rejected("invalid scope".into())); }
        let mut query = Self::query(profile); query.push(("scope", scope.into()));
        self.send(self.request(reqwest::Method::GET, "stats").query(&query)).await
    }
}
impl LootTransport for LootApi {
    async fn submit(&self, request: &SubmitRun) -> Result<SubmitOutcome, ApiError> {
        self.send(self.request(reqwest::Method::POST, "runs").json(request)).await
    }
    async fn latest(&self, profile: &ProfileRef) -> Result<Option<StoredRun>, ApiError> {
        // List is date-ordered, whereas the merge transaction uses run_number.
        // Historical undated imports may follow dated runs in the list.
        let mut latest: Option<StoredRun> = None;
        let mut page = 1;
        let mut seen = 0;
        loop {
            self.check_cancel()?;
            let result = self.list(profile, page, 200).await?;
            if result.items.is_empty() && seen < result.total { return Err(ApiError::Rejected("incomplete history".into())); }
            seen += result.items.len() as u64;
            for run in result.items {
                if latest.as_ref().is_none_or(|previous| run.run > previous.run) { latest = Some(run); }
            }
            if seen >= result.total { return Ok(latest); }
            page += 1;
        }
    }
}
async fn decode<T: serde::de::DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<T, ApiError> {
    let response = request.send().await.map_err(|_| ApiError::Unreachable)?;
    if response.status() == reqwest::StatusCode::CONFLICT { return Err(ApiError::Conflict); }
    if !response.status().is_success() { return Err(ApiError::Rejected(format!("server returned {}", response.status().as_u16()))); }
    response.json().await.map_err(|_| ApiError::Rejected("invalid server response".into()))
}
fn valid_id(id: &str) -> Result<(), ApiError> {
    if id.parse::<u64>().is_ok_and(|n| n > 0) { Ok(()) } else { Err(ApiError::Rejected("invalid run id".into())) }
}
#[tauri::command]
pub async fn boss_loot_stats(app: tauri::AppHandle, profile: ProfileRef, scope: String) -> Result<BossStats, ApiError> {
    LootApi::from_state(&app.state::<crate::AppState>()).stats(&profile, &scope).await
}
#[tauri::command]
pub async fn boss_loot_runs(app: tauri::AppHandle, profile: ProfileRef, page: u32, page_size: u32) -> Result<RunsPage, ApiError> {
    LootApi::from_state(&app.state::<crate::AppState>()).list(&profile, page, page_size).await
}
#[tauri::command]
pub async fn boss_loot_edit(app: tauri::AppHandle, id: String, expected_version: u64, items: RunItems) -> Result<StoredRun, ApiError> {
    valid_id(&id)?;
    let api = LootApi::from_state(&app.state::<crate::AppState>());
    decode(api.request(reqwest::Method::PATCH, &format!("runs/{id}")).json(&serde_json::json!({"expectedVersion": expected_version, "items": items}))).await
}
#[tauri::command]
pub async fn boss_loot_delete(app: tauri::AppHandle, id: String, expected_version: u64) -> Result<(), ApiError> {
    valid_id(&id)?;
    let api = LootApi::from_state(&app.state::<crate::AppState>());
    let _: serde_json::Value = decode(api.request(reqwest::Method::POST, &format!("runs/{id}/delete")).json(&serde_json::json!({"expectedVersion": expected_version}))).await?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ExportOutcome { Saved, Cancelled }

async fn export_selected(
    selected: Option<std::path::PathBuf>,
    contents: impl std::future::Future<Output = Result<String, ApiError>>,
    write: impl FnOnce(&std::path::Path, &str) -> Result<(), ApiError>,
) -> Result<ExportOutcome, ApiError> {
    let Some(path) = selected else { return Ok(ExportOutcome::Cancelled) };
    write(&path, &contents.await?)?;
    Ok(ExportOutcome::Saved)
}

/// The native dialog is the only source of an export write destination.
#[tauri::command]
pub async fn boss_loot_export(app: tauri::AppHandle, profile: ProfileRef) -> Result<ExportOutcome, ApiError> {
    use tauri_plugin_dialog::DialogExt;
    let p = crate::loot_profiles::built_in_profiles().iter().find(|p| ProfileRef::from(*p) == profile)
        .ok_or_else(|| ApiError::Rejected("unknown profile".into()))?;
    let dialog_app = app.clone();
    let filename = format!("{}-runs.csv", p.boss);
    let selected = tokio::task::spawn_blocking(move || {
        dialog_app.dialog().file().add_filter("CSV", &["csv"]).set_file_name(filename)
            .blocking_save_file().map(|file| file.into_path()
                .map_err(|e| ApiError::Rejected(e.to_string()))).transpose()
    }).await.map_err(|e| ApiError::Rejected(e.to_string()))??;
    let api = LootApi::from_state(&app.state::<crate::AppState>());
    export_selected(selected, async {
        let mut runs = Vec::new();
        let mut page = 1;
        loop {
            let result = api.list(&profile, page, 200).await?;
            if result.items.is_empty() && (runs.len() as u64) < result.total { return Err(ApiError::Rejected("incomplete export".into())); }
            runs.extend(result.items);
            if (runs.len() as u64) >= result.total { break; }
            page += 1;
        }
        Ok(csv(p, &runs))
    }, |path, contents| std::fs::write(path, contents).map_err(|e| ApiError::Rejected(e.to_string()))).await
}
fn csv(profile: &LootProfile, runs: &[StoredRun]) -> String {
    let items: Vec<_> = profile.groups.iter().flat_map(|g| &g.items).chain(&profile.extra).chain(&profile.categories).collect();
    let cell = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut output = format!("Run,Date,Seed,{}\n", items.iter().map(|i| cell(&i.name)).collect::<Vec<_>>().join(","));
    for run in runs {
        output.push_str(&format!("{},{},{},{}\n", run.run, cell(run.date.as_deref().unwrap_or("")), cell(run.seed.as_deref().unwrap_or("")),
            items.iter().map(|i| run.items.quantities.get(&i.id).map(u32::to_string).unwrap_or_default()).collect::<Vec<_>>().join(",")));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn profile() -> ProfileRef { ProfileRef { boss: "catarina".into(), profile_key: "Normal".into(), profile_version: 1 } }
    fn run(number: u64) -> StoredRun {
        StoredRun { profile: profile(), id: number.to_string(), league: "test".into(), run: number, date: None,
            seed: Some("42".into()), version: 3,
            items: RunItems { quantities: [("Cane of Kulemak".into(), 1)].into_iter().collect(), counted: vec!["Cane of Kulemak".into()] } }
    }
    fn server(body: serde_json::Value, status: u16) -> (LootApi, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut byte = [0]; socket.read_exact(&mut byte).unwrap(); bytes.push(byte[0]);
            }
            let headers = String::from_utf8(bytes.clone()).unwrap();
            let length: usize = headers.lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length: ").map(str::to_string)).map(|s| s.parse().unwrap()).unwrap_or(0);
            let mut payload = vec![0; length]; socket.read_exact(&mut payload).unwrap(); bytes.extend(payload);
            let body = body.to_string();
            write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            String::from_utf8(bytes).unwrap()
        });
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Device-ID", "test-device".parse().unwrap());
        let http = reqwest::Client::builder().default_headers(headers).build().unwrap();
        (LootApi { base, http, cancel: None }, handle)
    }
    #[tokio::test]
    async fn loot_http_submit_preserves_provenance_and_returns_server_feedback() {
        let eye = "Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly";
        let (api, handle) = server(serde_json::json!({"state":"saved", "run":run(79), "droppedPositiveKeys":[eye]}), 200);
        let request = SubmitRun { profile: profile(), league: "test".into(), seed: Some("42".into()), mode: "merge".into(),
            items: run(79).items, target: Some(Target { id: "79".into(), expected_version: 2 }),
            incoming_positive_keys: Some(vec!["Cane of Kulemak".into(), eye.into()]) };
        let outcome = api.submit(&request).await.unwrap();
        let raw = handle.join().unwrap();
        assert!(raw.starts_with("POST /api/boss-loot/runs HTTP/1.1\r\n"));
        assert!(raw.to_ascii_lowercase().contains("x-device-id: test-device\r\n"));
        let body: serde_json::Value = serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["target"], serde_json::json!({"id":"79", "expectedVersion":2}));
        assert_eq!(body["incomingPositiveKeys"], serde_json::json!(["Cane of Kulemak", eye]));
        assert_eq!(body["items"], serde_json::json!({"quantities":{"Cane of Kulemak":1}, "counted":["Cane of Kulemak"]}));
        assert_eq!(outcome.run.run, 79);
        assert_eq!(outcome.dropped_positive_keys, [eye]);
        assert!(!outcome.run.items.quantities.contains_key(eye));
    }
    #[tokio::test]
    async fn loot_http_conflict_does_not_decode_an_error_as_a_saved_run() {
        let (api, handle) = server(serde_json::json!({"error":"stale version"}), 409);
        let request = SubmitRun { profile: profile(), league: "test".into(), seed: None, mode: "new".into(),
            items: run(1).items, target: None, incoming_positive_keys: None };
        assert!(matches!(api.submit(&request).await, Err(ApiError::Conflict)));
        handle.join().unwrap();
    }
    #[tokio::test]
    async fn loot_http_latest_uses_highest_run_number_in_date_ordered_history() {
        let (api, handle) = server(serde_json::json!({"total":2,"page":1,"pageSize":200,"items":[run(4),run(78)]}), 200);
        assert_eq!(api.latest(&profile()).await.unwrap().unwrap().id, "78");
        let request = handle.join().unwrap();
        let url = reqwest::Url::parse(&format!("http://test{}", request.lines().next().unwrap().split_whitespace().nth(1).unwrap())).unwrap();
        assert_eq!(url.path(), "/api/boss-loot/runs");
        let params: std::collections::HashMap<_,_> = url.query_pairs().into_owned().collect();
        assert_eq!(params.get("boss").unwrap(), "catarina");
        assert_eq!(params.get("profileKey").unwrap(), "Normal");
        assert_eq!(params.get("profileVersion").unwrap(), "1");
    }
    #[tokio::test]
    async fn loot_http_stats_decodes_per_item_eligible_denominators() {
        let (api, handle) = server(serde_json::json!({"runs":78,"devices":2,"nextRun":79,"items":{"Cane of Kulemak":{"drops":14,"eligibleRuns":70}}}), 200);
        let stats = api.stats(&profile(), "collective").await.unwrap();
        assert_eq!((stats.runs, stats.devices, stats.next_run), (78, 2, 79));
        assert_eq!((stats.items["Cane of Kulemak"].drops, stats.items["Cane of Kulemak"].eligible_runs), (14, 70));
        assert!(handle.join().unwrap().contains("scope=collective"));
    }
    #[test]
    fn loot_csv_uses_profile_order_and_item_names() {
        let p = &crate::loot_profiles::built_in_profiles()[0];
        let output = csv(p, &[run(79)]);
        let lines: Vec<_> = output.lines().collect();
        assert!(lines[0].starts_with("Run,Date,Seed,\"Cinderswallow Urn\",\"Spinehail\",\"Cane of Kulemak\""));
        assert!(lines[0].ends_with("\"Nook's Crown\""));
        assert!(lines[1].starts_with("79,\"\",\"42\",,,1,"));
        assert_eq!(lines.len(), 2);
    }
}

#[cfg(test)]
mod review_r1_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

    struct HistoryServer {
        api: LootApi,
        requests: Arc<std::sync::Mutex<Vec<String>>>,
        first: Arc<tokio::sync::Notify>,
        release: std::sync::mpsc::Sender<()>,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl HistoryServer {
        fn new(total: u64) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            listener.set_nonblocking(true).unwrap();
            let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
            let first = Arc::new(tokio::sync::Notify::new());
            let stop = Arc::new(AtomicBool::new(false));
            let (release, released) = std::sync::mpsc::channel();
            let (record, signal, stopping) = (requests.clone(), first.clone(), stop.clone());
            let thread = std::thread::spawn(move || {
                while !stopping.load(Ordering::SeqCst) {
                    let (mut socket, _) = match listener.accept() {
                        Ok(connection) => connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(1)); continue;
                        }
                        Err(error) => panic!("accept: {error}"),
                    };
                    socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                    let mut bytes = Vec::new();
                    while !bytes.ends_with(b"\r\n\r\n") {
                        let mut byte = [0]; socket.read_exact(&mut byte).unwrap(); bytes.push(byte[0]);
                    }
                    let request = String::from_utf8(bytes).unwrap();
                    let path = request.lines().next().unwrap().split_whitespace().nth(1).unwrap().to_string();
                    let n = { let mut records = record.lock().unwrap(); records.push(path.clone()); records.len() };
                    if n == 1 { signal.notify_one(); released.recv_timeout(std::time::Duration::from_secs(5)).unwrap(); }
                    let body = if path.starts_with("/api/boss-loot/stats?") {
                        serde_json::json!({"runs":0,"devices":0,"nextRun":1,"items":{}})
                    } else {
                        let url = reqwest::Url::parse(&format!("http://test{path}")).unwrap();
                        let page: u64 = url.query_pairs().find(|(k,_)| k == "page").unwrap().1.parse().unwrap();
                        let items: Vec<_> = (0..if page == 1 { total.min(200) } else { total - 200 }).map(|n| serde_json::json!({
                            "boss":"catarina", "profileKey":"Normal", "profileVersion":1,
                            "id":(n+1).to_string(), "league":"test", "run":n+1,"date":null,"seed":null,"version":1,
                            "items":{"quantities":{},"counted":[]}
                        })).collect();
                        serde_json::json!({"total":total,"page":page,"pageSize":200,"items":items})
                    }.to_string();
                    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
                }
            });
            Self { api: LootApi { http: reqwest::Client::new(), base, cancel: None }, requests, first, release, stop, thread: Some(thread) }
        }
    }
    impl Drop for HistoryServer {
        fn drop(&mut self) {
            let _ = self.release.send(());
            self.stop.store(true, Ordering::SeqCst);
            self.thread.take().unwrap().join().unwrap();
        }
    }
    fn profile() -> ProfileRef { ProfileRef { boss: "catarina".into(), profile_key: "Normal".into(), profile_version: 1 } }

    async fn cancel_first_read(total: u64) {
        let server = HistoryServer::new(total);
        let (stop, cancel) = tokio::sync::watch::channel(false);
        let api = server.api.clone().with_cancel(cancel);
        let p = profile();
        let load = api.capture_context(&p);
        let cancel_work = async {
            server.first.notified().await;
            stop.send(true).unwrap();
            server.release.send(()).unwrap();
        };
        let (result, _) = tokio::join!(load, cancel_work);
        let requests = server.requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 1, "cancelled generation started another HTTP request: {requests:?}");
        assert!(requests[0].starts_with("/api/boss-loot/runs?"));
        assert!(matches!(result, Err(ApiError::Cancelled)));
    }
    #[tokio::test]
    async fn cancellation_during_history_starts_no_stats_request() { cancel_first_read(0).await; }
    #[tokio::test]
    async fn cancellation_during_paginated_history_starts_no_next_page() { cancel_first_read(201).await; }
    #[tokio::test]
    async fn rapid_reenable_does_not_revive_the_old_generations_http_chain() {
        let old = HistoryServer::new(201);
        let current = HistoryServer::new(0);
        let (old_stop, old_cancel) = tokio::sync::watch::channel(false);
        let (current_stop, current_cancel) = tokio::sync::watch::channel(false);
        let old_api = old.api.clone().with_cancel(old_cancel);
        let current_api = current.api.clone().with_cancel(current_cancel);
        let p = profile();
        let old_load = old_api.capture_context(&p);
        let restart = async {
            old.first.notified().await;
            old_stop.send(true).unwrap();
            // A fresh receiver is the same generation boundary the registry creates on re-enable.
            assert!(!*current_stop.borrow());
            old.release.send(()).unwrap();
            current.release.send(()).unwrap();
            current_api.capture_context(&p).await
        };
        let (stopped, fresh) = tokio::join!(old_load, restart);
        assert_eq!(old.requests.lock().unwrap().len(), 1, "old work launched another history/stats request");
        assert!(matches!(stopped, Err(ApiError::Cancelled)));
        let (latest, next) = fresh.unwrap();
        assert!(latest.is_none());
        assert_eq!(next, 1);
        let requests = current.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].starts_with("/api/boss-loot/stats?"));
    }
}

#[cfg(test)]
mod export_dialog_tests {
    use super::*;
    #[tokio::test]
    async fn loot_export_writes_only_the_dialog_selected_path() {
        let selected = std::path::PathBuf::from("dialog-selected.csv");
        let mut writes = Vec::new();
        let outcome = export_selected(Some(selected.clone()), async { Ok("Run,Date\n1,\n".to_string()) }, |path, contents| {
            writes.push((path.to_path_buf(), contents.to_string())); Ok(())
        }).await.unwrap();
        assert_eq!(outcome, ExportOutcome::Saved);
        assert_eq!(writes, vec![(selected, "Run,Date\n1,\n".to_string())]);
    }
    #[tokio::test]
    async fn loot_export_cancel_does_not_fetch_or_write() {
        let outcome = export_selected(None, async { panic!("cancel must not read history") }, |_, _| {
            panic!("cancel must not write a file")
        }).await.unwrap();
        assert_eq!(outcome, ExportOutcome::Cancelled);
    }
    #[tokio::test]
    async fn loot_export_failed_history_does_not_write() {
        let result = export_selected(Some("dialog-selected.csv".into()), async { Err(ApiError::Unreachable) }, |_, _| {
            panic!("incomplete history must not write")
        }).await;
        assert_eq!(result.unwrap_err(), ApiError::Unreachable);
    }
    #[test]
    fn loot_export_command_accepts_only_app_and_profile() {
        fn signature<F, R>(_: F) where F: Fn(tauri::AppHandle, ProfileRef) -> R {}
        signature(boss_loot_export);
    }
}
