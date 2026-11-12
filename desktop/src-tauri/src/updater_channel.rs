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

/// The updater for one manifest, routed through `proxy` when one is in effect.
fn endpoint_updater<R: Runtime, M: Manager<R>>(
    manager: &M,
    endpoint: reqwest::Url,
    proxy: Option<&crate::http::ProxyUrl>,
) -> Result<tauri_plugin_updater::Updater, String> {
    let mut builder = manager.updater_builder();
    if let Some(proxy) = proxy {
        builder = builder.proxy(reqwest::Url::parse(&proxy.for_http_clients()).map_err(|e| format!("bad proxy: {e}"))?);
    }
    builder
        .endpoints(vec![endpoint])
        .map_err(|e| format!("updater endpoint rejected: {e}"))?
        .build()
        .map_err(|e| format!("updater build failed: {e}"))
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

    let updater = endpoint_updater(&webview, url, webview.state::<crate::AppState>().proxy_in_effect.for_transport(crate::http::Transport::Other))?;

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

#[cfg(test)]
mod tests {
    use super::endpoint_updater;
    use crate::http::ProxyUrl;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    /// A mock app with the updater plugin registered the way `run()` does.
    fn app() -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3QK", "endpoints": [] }),
        );
        tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap()
    }

    /// Records the first request line it receives and answers 204 (no update).
    fn recording_listener() -> (u16, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let _ = tx.send(request.lines().next().unwrap_or_default().to_string());
                let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n");
            }
        });
        (port, rx)
    }

    /// The beta arm's manifest request goes to the proxy in effect: the
    /// listener sees the absolute-form request line only a proxy receives.
    #[tokio::test]
    async fn endpoint_updater_sends_the_manifest_request_through_the_proxy() {
        let app = app();
        let (proxy_port, seen) = recording_listener();
        let proxy = ProxyUrl::parse(&format!("http://127.0.0.1:{proxy_port}")).unwrap().unwrap();
        let endpoint = reqwest::Url::parse("http://beta.invalid/latest.json").unwrap();
        let updater = endpoint_updater(app.handle(), endpoint, Some(&proxy)).unwrap();
        let _ = updater.check().await;
        assert_eq!(
            seen.recv_timeout(Duration::from_secs(5)).unwrap(),
            "GET http://beta.invalid/latest.json HTTP/1.1"
        );
    }

    /// A socks5 proxy in effect reaches the updater as socks5h: the CONNECT
    /// request names the manifest host by domain (ATYP 0x03), so DNS
    /// resolves at the proxy.
    #[tokio::test]
    async fn endpoint_updater_leaves_dns_to_a_socks5_proxy() {
        let app = app();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut greeting = [0u8; 3];
            if stream.read_exact(&mut greeting).is_err() {
                return;
            }
            let _ = stream.write_all(&[0x05, 0x00]);
            let mut head = [0u8; 4];
            if stream.read_exact(&mut head).is_ok() {
                let _ = tx.send(head[3]);
            }
        });
        let proxy = ProxyUrl::parse(&format!("socks5://127.0.0.1:{port}")).unwrap().unwrap();
        let endpoint = reqwest::Url::parse("http://beta.invalid/latest.json").unwrap();
        let updater = endpoint_updater(app.handle(), endpoint, Some(&proxy)).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(3), updater.check()).await;
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 0x03, "address type must be a domain name");
    }
}
