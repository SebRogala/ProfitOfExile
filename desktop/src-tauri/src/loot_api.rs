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
