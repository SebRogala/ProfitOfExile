//! The plain settings commands (POE-342): sidebar, trade staleness and
//! auto-trade, timer appearance, GPU-free webview, proxy, UI prefs and widget
//! geometry get/set.

use tauri::{AppHandle, Emitter, Manager};

use crate::{app_log, emit_status, http, persist_settings, settings, AppState};

#[tauri::command]
pub(crate) fn set_sidebar_open(open: bool, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.sidebar_open.lock().unwrap_or_else(|e| e.into_inner()) = open;
    persist_settings(&app);
    emit_status(&app);
}

#[tauri::command]
pub(crate) fn set_trade_staleness_settings(warn_secs: u32, critical_secs: u32, auto_refresh_secs: u32, app: AppHandle) {
    let state = app.state::<AppState>();
    {
        let mut trade = state.trade_settings.lock().unwrap_or_else(|e| e.into_inner());
        trade.trade_stale_warn_secs = warn_secs;
        trade.trade_stale_critical_secs = critical_secs;
        trade.trade_auto_refresh_secs = auto_refresh_secs;
    }
    persist_settings(&app);
    emit_status(&app);
}

#[tauri::command]
pub(crate) fn set_auto_trade(enabled: bool, app: AppHandle) {
    let state = app.state::<AppState>();
    state.trade_settings.lock().unwrap_or_else(|e| e.into_inner()).auto_trade_enabled = enabled;
    persist_settings(&app);
    emit_status(&app);
}

/// Every stored placement for one module's widgets (POE-225).
///
/// Read from the OWNER, not from the file: the overlay window asks for this on
/// its first paint and Settings asks for the same rows while a save may be in
/// flight, and a `settings::load` per call would let the two answers disagree
/// (and would put a disk read in the overlay's first frame).
///
/// A widget with no entry is absent from the answer rather than filled in with
/// a default — the shipped defaults are CSS pixels in the frontend registry
/// (`src/lib/overlay/widgets/widget-registry.ts`), and inventing a physical one
/// here would need this command to know the display's scale factor.
#[tauri::command]
pub(crate) fn get_widget_geometries(
    module: String,
    state: tauri::State<'_, AppState>,
) -> Vec<settings::WidgetGeometryEntry> {
    let widgets = state.widgets.lock().unwrap_or_else(|e| e.into_inner());
    settings::widgets_for_module(&widgets, &module)
}

/// Place one widget and persist the whole map (POE-225).
///
/// Owner first, file second, through `persist_settings` — the same order every
/// owned setting uses, and the reason `Settings.widgets` must never be added to
/// `persist_overlay_settings`: that function's job is fields no `AppState`
/// mutex owns, and carrying the file's copy over this one would undo the write
/// that just happened.
///
/// The id is not validated against a widget list. The registry lives in the
/// frontend, so the only check available here would be a duplicate of it that
/// could fall out of date, and the failure it would prevent is a dead map entry
/// nothing ever reads.
#[tauri::command]
pub(crate) fn set_widget_geometry(
    id: String,
    geometry: settings::WidgetGeometry,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) {
    state
        .widgets
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id.clone(), geometry);
    persist_settings(&app);
    // Tell the module's own window that its map moved (POE-226). Settings' Show
    // checkbox writes through this command, and without the notice a widget
    // switched off stayed on screen until the overlay was next rebuilt — the
    // checkbox looked like it had done nothing.
    //
    // `emit_to` the module's window rather than a global `emit`: the host
    // listens window-scoped, which is the guide's rule for anything Rust sends
    // an overlay, and a broadcast would wake every other overlay for a map they
    // do not read.
    let Some((module, _)) = id.split_once('.') else {
        log::warn!("set_widget_geometry: id '{}' has no module half, not notifying", id);
        return;
    };
    if let Err(e) = app.emit_to(module, "widget-geometry-changed", serde_json::json!({
        "module": module,
    })) {
        log::warn!("emit widget-geometry-changed to '{}' failed: {}", module, e);
    }
}

#[tauri::command]
pub(crate) fn get_timer_appearance(app: AppHandle) -> serde_json::Value {
    let s = settings::load(&app);
    serde_json::json!({
        "bg_opacity": s.timer_bg_opacity.unwrap_or(0.75),
        "text_stroke": s.timer_text_stroke.unwrap_or(true),
    })
}

#[tauri::command]
pub(crate) fn set_timer_appearance(bg_opacity: f32, text_stroke: bool, app: AppHandle) {
    settings::update(&app, |existing| {
        let mut s = existing.clone();
        s.timer_bg_opacity = Some(bg_opacity.clamp(0.0, 1.0));
        s.timer_text_stroke = Some(text_stroke);
        s
    });
    if let Err(e) = app.emit("timer-appearance-changed", serde_json::json!({
        "bg_opacity": bg_opacity.clamp(0.0, 1.0),
        "text_stroke": text_stroke,
    })) {
        log::warn!("emit timer-appearance-changed failed: {}", e);
    }
}

/// The stored GPU-free webview choice; the running process may differ until
/// the next launch (`webview2_browser_args` applies it at startup only).
#[tauri::command]
pub(crate) fn get_webview_gpu_disabled(app: AppHandle) -> bool {
    settings::load(&app).webview_gpu_disabled
}

/// The saved proxy and the one this process started with; they differ until
/// the next launch.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxySettings {
    saved: String,
    in_effect: String,
    saved_trade: bool,
    trade_in_effect: bool,
    /// The in-effect proxy as the updater's `check({ proxy })` takes it
    /// (`socks5h://` for SOCKS); empty when none.
    updater_proxy: String,
    /// Why every request is blocked this run; empty when not blocked.
    blocked: String,
}

#[tauri::command]
pub(crate) fn get_proxy_settings(app: AppHandle) -> ProxySettings {
    proxy_settings(&settings::load(&app), &app.state::<AppState>().proxy_in_effect)
}

/// The Settings page's view of the saved proxy and the one in effect.
fn proxy_settings(saved: &settings::Settings, in_effect: &http::ProxyConfig) -> ProxySettings {
    ProxySettings {
        saved: saved.proxy_url.clone(),
        in_effect: match &in_effect.mode {
            http::ProxyMode::Via(p) => p.as_str().to_string(),
            _ => String::new(),
        },
        saved_trade: saved.proxy_trade,
        trade_in_effect: in_effect.trade,
        updater_proxy: in_effect
            .for_transport(http::Transport::Other)
            .ok()
            .flatten()
            .map(http::ProxyUrl::for_http_clients)
            .unwrap_or_default(),
        blocked: in_effect.blocked_reason().unwrap_or_default().to_string(),
    }
}

/// Stores whether trade requests use the proxy; applies after a restart.
#[tauri::command]
pub(crate) fn set_proxy_trade(enabled: bool, app: AppHandle) {
    settings::update(&app, |existing| {
        let mut s = existing.clone();
        s.proxy_trade = enabled;
        s
    });
    app_log(&app, format!("Proxy for trade requests {} — applies after restart", if enabled { "on" } else { "off" }));
}

/// Stores a validated proxy (empty clears it); applies after a restart.
#[tauri::command]
pub(crate) fn set_proxy_url(url: String, app: AppHandle) -> Result<(), String> {
    let proxy = http::ProxyUrl::parse(&url)?;
    settings::update(&app, |existing| {
        let mut s = existing.clone();
        s.proxy_url = proxy.as_ref().map(|p| p.as_str().to_string()).unwrap_or_default();
        s
    });
    let shown = proxy.as_ref().map_or("none", |p| p.as_str());
    app_log(&app, format!("Proxy set to {shown} — applies after restart"));
    Ok(())
}

#[tauri::command]
pub(crate) fn set_webview_gpu_disabled(disabled: bool, app: AppHandle) {
    settings::update(&app, |existing| {
        let mut s = existing.clone();
        s.webview_gpu_disabled = disabled;
        s
    });
    app_log(&app, format!("GPU-free webview rendering {} — applies after restart", if disabled { "on" } else { "off" }));
}

#[tauri::command]
pub(crate) fn get_ui_prefs(app: AppHandle) -> std::collections::HashMap<String, String> {
    let state = app.state::<AppState>();
    let prefs = state.ui_prefs.lock().unwrap_or_else(|e| e.into_inner()).clone();
    prefs
}

#[tauri::command]
pub(crate) fn set_ui_pref(key: String, value: String, app: AppHandle) {
    let state = app.state::<AppState>();
    state.ui_prefs.lock().unwrap_or_else(|e| e.into_inner()).insert(key, value);
    persist_settings(&app);
}

#[cfg(test)]
mod tests {
    fn blocked_config() -> crate::http::ProxyConfig {
        crate::http::ProxyConfig { mode: crate::http::ProxyMode::blocked("R"), trade: true }
    }

    #[test]
    fn proxy_settings_echo_the_blocked_reason() {
        assert_eq!(
            super::proxy_settings(&crate::settings::Settings::default(), &blocked_config()).blocked,
            "R. Nothing is sent until you fix or clear the proxy in Settings and restart the app."
        );
    }

    #[test]
    fn proxy_settings_offer_the_updater_no_proxy_while_blocked() {
        assert_eq!(super::proxy_settings(&crate::settings::Settings::default(), &blocked_config()).updater_proxy, "");
    }
}
