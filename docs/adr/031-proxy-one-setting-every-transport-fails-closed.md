---
uid: 29f45a73-0827-44e6-be3d-5a394b1287b2
---

# ADR-031: Proxy: One Setting for Every Transport, and It Fails Closed

## Status

Accepted 2026-11-12 (POE-360). Last verified 2026-11-12.

## Enforcement

Tier: required

Scope: Every outbound request of the desktop app (`desktop/src-tauri`, `desktop/src`).

Summary: One saved proxy, one application point per transport. A saved proxy the app cannot use blocks every transport; the app never falls back to a direct connection.

## Context

Some players can reach pathofexile.com or the ProfitOfExile server only
through a proxy, and a per-app proxy setting is how they route the desktop
app (POE-360). The app sends requests through three transports: the reqwest
clients in Rust (the server client `server_http` and `TradeApiClient`), the
updater (the plugin's stable check from the webview, the beta check in Rust),
and the WebView2 windows themselves (`fetch`, EventSource, images). A proxy
setting that reaches only some of them, or that silently connects directly
when the saved value cannot be used, leaks the user's real address to the
hosts they meant to reach only through the proxy. The Operator ruled FAIL
CLOSED on 2026-11-12.

## Decision

The saved setting is `proxy_url` (`http://`, `https://` or `socks5://`, no
credentials; empty = none) plus `proxy_trade` (whether trade requests use it
too; default on). `settings::proxy_config_at` reads both once at startup into
`http::ProxyConfig`; a change applies after a restart.

Each transport has one application point:

| Transport | Applied at |
|---|---|
| `server_http`, `TradeApiClient` | `http::client_builder`, the only place a reqwest client is built |
| Beta update check (Rust) | `updater_channel::endpoint_updater` |
| Stable update check (webview) | `updater/check.ts` `checkForUpdate()`; the updater plugin stays registered |
| WebView2 windows | `http::webview2_proxy_args`, appended to `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` by `run()` |

How the startup read decides (`ProxyMode` `Direct`, `Via` or `Blocked`):

- no settings file (fresh install), or no settings path (off Windows): Direct
- file present but unreadable: Blocked
- file present but not valid JSON: Blocked
- `proxy_url` absent or blank: Direct
- `proxy_url` present but not text: Blocked
- `proxy_url` rejected by `ProxyUrl::parse` (scheme, host, or credentials): Blocked
- `proxy_url` valid: Via

The read is narrow: an unrelated bad field in the file does not block.

When Blocked:

- `client_builder` returns a builder whose connector layer refuses every
  connection before a socket opens, so every request from `server_http` and
  `TradeApiClient` fails through its call site's existing error path.
  Blocked overrides `proxy_trade = false`: trade requests are blocked too.
- `endpoint_updater` returns the blocked reason before building an updater.
- `checkForUpdate()` throws the blocked reason before calling either arm.
- WebView2 starts with a dead proxy (`--proxy-server=http://127.0.0.1:1`),
  a resolver that finds no host (`--host-resolver-rules`), and the local
  bypass list kept so the dev server still loads.
- `settings::update_at` refuses to write while the existing file is unreadable
  or not JSON, so a save cannot reset the file to defaults and unblock the
  next launch.

Interim B: credentials (`user:pass@`) are rejected, because WebView2's
`--proxy-server` cannot carry them and such a URL would route only part of the
app.

The source-scan test in `http.rs`
(`no_production_source_builds_an_http_client_outside_the_factory`) fails when
production code outside `http.rs` calls `Client::new(`, `Client::builder(`,
`ClientBuilder::new(` or `reqwest::get(`.

The blocked reason is visible in two places: on the Settings Proxy field
(`AppStatus.proxy_error`) and as the top-bar `Proxy error` badge, whose
tooltip is the reason. `run()` also writes it to `app.log`.

## Consequences

- Updates are off while the proxy is blocked. This is intended by fail-closed.
- While `settings.json` is unreadable or not valid JSON, the app neither sends
  nor writes settings, so every launch stays blocked until the user fixes or
  deletes `settings.json`.
- A file that is valid JSON but fails the full `Settings` parse keeps today's
  reset-to-defaults on the first save; that save can drop a valid saved proxy.
- Not verified on Windows:
  - whether the WebView2 flags stop every request in all windows;
  - whether the quoted `--host-resolver-rules` value survives
    `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` parsing;
  - whether the loopback bypass still loads the dev server;
  - the real updater behavior: the beta check through
    `check_update_from_endpoint` is exercised only against the mock app.
  Linux tests pin the argument strings only. The stable update check is
  refused in JavaScript only; the updater plugin's initialization `Builder`
  has no proxy or refusal option (a proxy can be set only per check).
