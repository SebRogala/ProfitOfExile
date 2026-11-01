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
pub enum ApiError { Conflict, Unreachable, Rejected(String) }
pub trait LootTransport: Sync {
    fn submit(&self, request: &SubmitRun) -> impl std::future::Future<Output = Result<SubmitOutcome, ApiError>> + Send;
    fn latest(&self, profile: &ProfileRef) -> impl std::future::Future<Output = Result<Option<StoredRun>, ApiError>> + Send;
}
pub struct LootApi { http: reqwest::Client, base: String }
impl LootApi {
    pub fn from_state(state: &crate::AppState) -> Self {
        Self { http: state.server_http.clone(), base: state.server_url.lock().unwrap_or_else(|e| e.into_inner()).trim_end_matches('/').into() }
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
        decode(self.request(reqwest::Method::GET, "runs").query(&query)).await
    }
    pub async fn stats(&self, profile: &ProfileRef, scope: &str) -> Result<BossStats, ApiError> {
        if !matches!(scope, "own" | "collective") { return Err(ApiError::Rejected("invalid scope".into())); }
        let mut query = Self::query(profile); query.push(("scope", scope.into()));
        decode(self.request(reqwest::Method::GET, "stats").query(&query)).await
    }
}
impl LootTransport for LootApi {
    async fn submit(&self, request: &SubmitRun) -> Result<SubmitOutcome, ApiError> {
        decode(self.request(reqwest::Method::POST, "runs").json(request)).await
    }
    async fn latest(&self, profile: &ProfileRef) -> Result<Option<StoredRun>, ApiError> {
        // List is date-ordered, whereas the merge transaction uses run_number.
        // Historical undated imports may follow dated runs in the list.
        let mut latest: Option<StoredRun> = None;
        let mut page = 1;
        let mut seen = 0;
        loop {
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

/// Fetch this device's complete history before writing the user-chosen file.
#[tauri::command]
pub async fn boss_loot_export(app: tauri::AppHandle, profile: ProfileRef, destination: String) -> Result<(), ApiError> {
    if destination.is_empty() { return Err(ApiError::Rejected("export destination required".into())); }
    let p = crate::loot_profiles::built_in_profiles().iter().find(|p| ProfileRef::from(*p) == profile)
        .ok_or_else(|| ApiError::Rejected("unknown profile".into()))?;
    let api = LootApi::from_state(&app.state::<crate::AppState>());
    let mut runs = Vec::new();
    let mut page = 1;
    loop {
        let result = api.list(&profile, page, 200).await?;
        if result.items.is_empty() && (runs.len() as u64) < result.total { return Err(ApiError::Rejected("incomplete export".into())); }
        runs.extend(result.items);
        if (runs.len() as u64) >= result.total { break; }
        page += 1;
    }
    std::fs::write(destination, csv(p, &runs)).map_err(|e| ApiError::Rejected(e.to_string()))
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
        (LootApi { base, http }, handle)
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
        assert!(lines[0].ends_with("\"Rare, two veiled modifiers\""));
        assert!(lines[1].starts_with("79,\"\",\"42\",,,1,"));
        assert_eq!(lines.len(), 2);
    }
}
