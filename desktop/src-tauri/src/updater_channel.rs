//! Beta-channel update check (POE-203).
//!
//! **Why this exists.** `@tauri-apps/plugin-updater` has no per-call endpoint
//! option: its `check` command takes `headers`, `timeout`, `proxy`, `target`
//! and `allowDowngrades` and nothing else — verified against the vendored
//! 2.10.0 source and the published 2.10.1 tarball, the newest release. A
//! webview therefore cannot ask the plugin for a manifest other than the one
//! in `tauri.conf.json`, so a second (beta) manifest needs a command of our
//! own. The Rust `UpdaterBuilder` does expose `endpoints`, which is all this
//! file uses.
//!
//! **Why the webview still installs through the plugin.** The answer carries
//! the same fields the plugin's own `check` returns, including an `rid` for an
//! `Update` registered in THIS webview's resource table. The webview wraps the
//! answer in the plugin's exported `Update` class, so `download`, `install`
//! and `downloadAndInstall` resolve that resource and work unchanged — there
//! is no second download or install path to keep in step with the first.
//!
//! **Signature checking is unaffected.** The builder is seeded from the same
//! `plugins.updater` config, so a beta artifact must be signed with the same
//! public key as a stable one. This command widens WHERE a manifest may be
//! read from, not WHAT will be installed from it.

use serde::Serialize;
use tauri::{Manager, ResourceId, Runtime, Webview};
use tauri_plugin_updater::UpdaterExt;

/// The plugin's `Metadata`, which is `pub(crate)` there and so re-declared.
///
/// The field names must stay camelCase on the wire: the webview hands this
/// object straight to the plugin's `Update` constructor, which reads
/// `currentVersion` / `rawJson` as written by the plugin's own command.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    rid: ResourceId,
    current_version: String,
    version: String,
    date: Option<String>,
    body: Option<String>,
    raw_json: serde_json::Value,
}

/// Check one caller-supplied manifest for an update.
///
/// `Ok(None)` means the endpoint answered and offers nothing newer than the
/// running version — the updater's default comparator is `update > current`,
/// which is what makes a beta device on `1.2.0-beta.3` stop being offered
/// `1.2.0-beta.3` while still being offered a stable `1.2.0` by the other arm.
///
/// Errors come back as strings rather than a typed error because the webview's
/// only handling is to log the arm that failed and keep the other one's
/// answer; see `$lib/updater/check.ts`.
///
/// `endpoint` is a parameter rather than a constant here because the manifest
/// URL belongs with the channel logic: the ONE caller pins it to the
/// `BETA_MANIFEST_URL` constant in `$lib/updater/check.ts`, and the signature
/// check on whatever the manifest points at is unaffected either way.
#[tauri::command]
pub async fn check_update_from_endpoint<R: Runtime>(
    webview: Webview<R>,
    endpoint: String,
) -> Result<Option<UpdateMetadata>, String> {
    // `reqwest::Url` IS `url::Url` — the same crate the updater builder takes,
    // re-exported by a dependency this crate already has.
    let url = reqwest::Url::parse(&endpoint).map_err(|e| format!("bad updater endpoint: {e}"))?;

    let updater = webview
        .updater_builder()
        .endpoints(vec![url])
        .map_err(|e| format!("updater endpoint rejected: {e}"))?
        .build()
        .map_err(|e| format!("updater build failed: {e}"))?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("update check failed: {e}"))?;

    let Some(update) = update else {
        return Ok(None);
    };

    // A date the manifest wrote but we cannot format is dropped rather than
    // failing the whole check: nothing in the app renders it, and losing an
    // available update over a malformed timestamp would be the worse trade.
    let date = update
        .date
        .and_then(|d| d.format(&time::format_description::well_known::Rfc3339).ok());

    Ok(Some(UpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        date,
        body: update.body.clone(),
        raw_json: update.raw_json.clone(),
        rid: webview.resources_table().add(update),
    }))
}
