//! Persistent settings — saved to JSON in the Tauri app data directory.
//!
//! Loaded on startup, saved on every change. Settings that aren't in the file
//! use defaults (forward-compatible with new fields).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

use crate::CaptureRegion;

const SETTINGS_FILENAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub client_txt_path: String,
    pub server_url: String,
    pub gem_region: CaptureRegion,
    pub font_region: CaptureRegion,
    pub window: Option<WindowSettings>,
    pub sidebar_open: bool,
    pub comparator_overlay: Option<OverlaySettings>,
    pub compass_overlay: Option<OverlaySettings>,
    pub pathstrip_overlay: Option<OverlaySettings>,
    pub timer_overlay: Option<OverlaySettings>,
    /// Master toggle for all lab overlays (compass + pathstrip + timer).
    pub lab_overlays_enabled: bool,
    /// Yellow indicator threshold for trade data age (seconds).
    pub trade_stale_warn_secs: u32,
    /// Red indicator threshold for trade data age (seconds).
    pub trade_stale_critical_secs: u32,
    /// Auto-refresh trade data after this many seconds.
    pub trade_auto_refresh_secs: u32,
    /// Whether auto-trade is enabled (fetch trade data automatically on compare).
    pub auto_trade_enabled: bool,
    pub compass_mode: String,
    pub compass_strategy: String,
    pub compass_difficulty: String,
    pub shrine_warn_enabled: bool,
    pub shrine_warn_size: String,
    pub shrine_warn_corner: String,
    pub shrine_warn_on_take: String,
    /// Timer overlay background opacity (0.0–1.0, default 0.75).
    pub timer_bg_opacity: Option<f32>,
    /// Timer overlay text stroke/outline enabled (default true).
    pub timer_text_stroke: Option<bool>,
    /// Lab mode: "Normal" (default) or "Dedication".
    /// Controls OCR vocabulary, font session metadata, and comparator behaviour.
    #[serde(default = "default_lab_mode")]
    pub lab_mode: String,
    /// Session queue auto-clear timer in minutes (default: 2).
    #[serde(default = "default_autoclear_minutes")]
    pub autoclear_minutes: u32,
    /// Dedication rankings pool selector: "skill" (default) or "transfigured".
    #[serde(default = "default_dedication_pool")]
    pub dedication_pool: String,
    /// Dedication corrupted variant selector: "21/23" (default) or "21/20".
    /// Each variant is its own market, so it selects the EV table, the rankings
    /// and the comparator together.
    #[serde(default = "default_dedication_variant")]
    pub dedication_variant: String,
    /// Normal-mode market selector: "20/20" (default), "20/0", "1/20" or "1/0".
    /// The Normal counterpart of `dedication_variant`, and read for the same
    /// reason: it is the market OCR'd gems are priced against, on this window
    /// and on any paired web view.
    #[serde(default = "default_normal_variant")]
    pub normal_variant: String,
    /// Show low-confidence gems in rankings (default: false).
    #[serde(default)]
    pub show_low_confidence: bool,
    /// Schema-less UI view preferences (sort mode, colour filter, row limit…).
    /// The frontend owns the keys and values; Rust stores and persists the map
    /// blindly. Add a typed field here only when Rust itself reads the value.
    #[serde(default)]
    pub ui_prefs: std::collections::HashMap<String, String>,
    /// Per-module enabled flags — a DELTA, not the full registry map. Only
    /// entries that differ from the module's `default_enabled` are written
    /// (plus keys this build does not recognise, kept verbatim), so a later
    /// version that flips a default still reaches users who never chose.
    /// See src/modules.rs. Container-level `#[serde(default)]` covers the
    /// absent-field case; `modules_or_default` covers a present-but-wrong one.
    #[serde(default, deserialize_with = "modules_or_default")]
    pub modules: std::collections::HashMap<String, bool>,
}

/// Tolerant reader for `Settings.modules`: keeps every key whose value is a
/// real bool and drops the rest, so neither a wrong-shaped map nor one bad
/// entry can take out anything else.
///
/// Without this, one bad `modules` value discards the ENTIRE settings file —
/// `load` falls back to `Settings::default()`, so every unrelated preference
/// (server URL, capture regions, overlay layout) is silently reset. And a
/// whole-map typed deserialize fails as a unit, so a single non-bool entry
/// (a newer build's key, a hand edit) would erase the user's valid choices
/// alongside it. The map is a schema-less key/value area written by two
/// different code paths, which is exactly where a wrong shape can turn up.
fn modules_or_default<'de, D>(
    deserializer: D,
) -> Result<std::collections::HashMap<String, bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Settings are JSON only, so the value is always representable; taking it
    // as a `Value` first is what makes the salvage possible at all — a failed
    // typed deserialize has already consumed the deserializer.
    let raw = serde_json::Value::deserialize(deserializer)?;
    let Some(entries) = raw.as_object() else {
        return Ok(Default::default());
    };
    Ok(entries
        .iter()
        .filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b)))
        .collect())
}

fn default_lab_mode() -> String {
    "Normal".to_string()
}

fn default_autoclear_minutes() -> u32 {
    2
}

fn default_dedication_pool() -> String {
    "skill".to_string()
}

fn default_dedication_variant() -> String {
    "21/23".to_string()
}

fn default_normal_variant() -> String {
    "20/20".to_string()
}

pub const DEFAULT_TRADE_STALE_WARN_SECS: u32 = 120;
pub const DEFAULT_TRADE_STALE_CRITICAL_SECS: u32 = 600;
pub const DEFAULT_TRADE_AUTO_REFRESH_SECS: u32 = 900;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlaySettings {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSettings {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            client_txt_path: crate::detect_client_txt_path(),
            server_url: String::from(option_env!("POE_SERVER_URL").unwrap_or("https://profitofexile.localhost")),
            gem_region: CaptureRegion::default(),
            font_region: CaptureRegion::default_font_panel(),
            window: None,
            sidebar_open: true,
            comparator_overlay: None,
            compass_overlay: None,
            pathstrip_overlay: None,
            timer_overlay: None,
            lab_overlays_enabled: true,
            trade_stale_warn_secs: DEFAULT_TRADE_STALE_WARN_SECS,
            trade_stale_critical_secs: DEFAULT_TRADE_STALE_CRITICAL_SECS,
            trade_auto_refresh_secs: DEFAULT_TRADE_AUTO_REFRESH_SECS,
            auto_trade_enabled: false,
            compass_mode: String::from("minimap"),
            compass_strategy: String::from("darkshrines-on-route"),
            compass_difficulty: String::from("Uber"),
            shrine_warn_enabled: true,
            shrine_warn_size: String::from("medium"),
            shrine_warn_corner: String::from("bottom-right"),
            shrine_warn_on_take: String::from("green"),
            timer_bg_opacity: None,
            timer_text_stroke: None,
            lab_mode: default_lab_mode(),
            autoclear_minutes: default_autoclear_minutes(),
            dedication_pool: default_dedication_pool(),
            dedication_variant: default_dedication_variant(),
            normal_variant: default_normal_variant(),
            // Default ON — see the note in the web BestPlays component: at
            // 20-level variants a thin market is normal, so hiding flagged gems
            // by default reproduces the POE-131 ranking gap.
            show_low_confidence: true,
            ui_prefs: std::collections::HashMap::new(),
            modules: std::collections::HashMap::new(),
        }
    }
}

/// Get the settings file path inside the Tauri app data directory.
pub fn settings_path_pub(app: &tauri::AppHandle) -> Option<PathBuf> {
    settings_path(app)
}

fn settings_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(e) => {
            log::error!("Cannot resolve app data directory: {}", e);
            return None;
        }
    };
    if let Err(e) = fs::create_dir_all(&dir) {
        log::error!("Cannot create settings directory {:?}: {}", dir, e);
        return None;
    }
    Some(dir.join(SETTINGS_FILENAME))
}

/// Load settings from disk. Returns defaults if file doesn't exist or is invalid.
pub fn load(app: &tauri::AppHandle) -> Settings {
    let path = match settings_path(app) {
        Some(p) => p,
        None => return Settings::default(),
    };
    match fs::read_to_string(&path) {
        Ok(contents) => {
            match serde_json::from_str::<Settings>(&contents) {
                Ok(s) => {
                    log::info!("Settings loaded from {:?}", path);
                    s
                }
                Err(e) => {
                    log::warn!("Settings file invalid, using defaults: {}", e);
                    // `log::` is unreachable in a shipped build; this discards
                    // every stored preference, so it must reach the app log.
                    crate::app_log(
                        app,
                        format!("Settings file invalid — ALL settings reset to defaults: {}", e),
                    );
                    Settings::default()
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::info!("No settings file found, using defaults");
            Settings::default()
        }
        Err(e) => {
            log::error!("Failed to read settings file {:?}: {} — using defaults", path, e);
            Settings::default()
        }
    }
}

/// Save current settings to disk.
pub fn save(app: &tauri::AppHandle, settings: &Settings) {
    let path = match settings_path(app) {
        Some(p) => p,
        None => return,
    };
    match serde_json::to_string_pretty(settings) {
        Ok(json) => {
            // Write-temp-then-rename: a plain fs::write leaves a truncated
            // settings file if the process dies mid-write, and this path runs
            // on every persisted preference change, not just discrete toggles.
            let tmp = path.with_extension("json.tmp");
            if let Err(e) = fs::write(&tmp, &json) {
                log::error!("Failed to write settings to {:?}: {}", tmp, e);
                crate::app_log(app, format!("Failed to write settings to {:?}: {}", tmp, e));
                return;
            }
            if let Err(e) = fs::rename(&tmp, &path) {
                log::error!("Failed to move settings into place at {:?}: {}", path, e);
                crate::app_log(
                    app,
                    format!("Failed to move settings into place at {:?}: {}", path, e),
                );
            }
        }
        Err(e) => {
            log::error!("Failed to serialize settings: {}", e);
            crate::app_log(app, format!("Failed to serialize settings: {}", e));
        }
    }
}

/// Build a Settings struct from the current AppState.
pub fn from_state(state: &crate::AppState) -> Settings {
    Settings {
        client_txt_path: state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        server_url: state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        gem_region: state.gem_region.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        font_region: state.font_region.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        window: None, // Window settings are saved separately on close, not from AppState
        sidebar_open: *state.sidebar_open.lock().unwrap_or_else(|e| e.into_inner()),
        comparator_overlay: None, // Overlay settings saved separately, not from AppState
        compass_overlay: None,    // Overlay settings saved separately, not from AppState
        pathstrip_overlay: None,  // Overlay settings saved separately, not from AppState
        timer_overlay: None,     // Overlay settings saved separately, not from AppState
        lab_overlays_enabled: *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()),
        trade_stale_warn_secs: *state.trade_stale_warn_secs.lock().unwrap_or_else(|e| e.into_inner()),
        trade_stale_critical_secs: *state.trade_stale_critical_secs.lock().unwrap_or_else(|e| e.into_inner()),
        trade_auto_refresh_secs: *state.trade_auto_refresh_secs.lock().unwrap_or_else(|e| e.into_inner()),
        auto_trade_enabled: *state.auto_trade_enabled.lock().unwrap_or_else(|e| e.into_inner()),
        compass_mode: state.compass_mode.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        compass_strategy: state.compass_strategy.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        compass_difficulty: state.compass_difficulty.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        shrine_warn_enabled: *state.shrine_warn_enabled.lock().unwrap_or_else(|e| e.into_inner()),
        shrine_warn_size: state.shrine_warn_size.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        shrine_warn_corner: state.shrine_warn_corner.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        shrine_warn_on_take: state.shrine_warn_on_take.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        timer_bg_opacity: None,    // Appearance settings saved separately, not from AppState
        timer_text_stroke: None,   // Appearance settings saved separately, not from AppState
        lab_mode: state.lab_mode.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        autoclear_minutes: *state.autoclear_minutes.lock().unwrap_or_else(|e| e.into_inner()),
        dedication_pool: state.dedication_pool.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        dedication_variant: state.dedication_variant.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        normal_variant: state.normal_variant.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        show_low_confidence: *state.show_low_confidence.lock().unwrap_or_else(|e| e.into_inner()),
        ui_prefs: state.ui_prefs.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        // Delta only — the owner map holds every registry id, but persisting
        // unchosen defaults would pin them forever (see modules.rs).
        modules: crate::modules::persistable_modules(
            &state.modules_enabled.lock().unwrap_or_else(|e| e.into_inner()),
            &crate::modules::module_lifecycles(),
        ),
    }
}

/// Copy overlay/window settings from existing file into the new settings struct.
/// These fields are managed by their own save commands, not by AppState.
pub fn persist_overlay_settings(existing: &Settings, target: &mut Settings) {
    target.window = existing.window.clone();
    target.comparator_overlay = existing.comparator_overlay.clone();
    target.compass_overlay = existing.compass_overlay.clone();
    target.pathstrip_overlay = existing.pathstrip_overlay.clone();
    target.timer_overlay = existing.timer_overlay.clone();
    target.timer_bg_opacity = existing.timer_bg_opacity;
    target.timer_text_stroke = existing.timer_text_stroke;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
    use std::sync::Mutex;

    /// A bare `AppState` for the settings round-trip tests — the same literal
    /// `run()` builds, minus the runtime-only bits. Deliberately not a
    /// `Default` impl: `run()` stays the one production construction site, and
    /// a new field breaking this helper is the intended signal to decide
    /// whether it also needs a settings touch point.
    fn test_app_state() -> crate::AppState {
        crate::AppState {
            device_id: String::new(),
            pair_code: Mutex::new(String::new()),
            client_txt_path: Mutex::new(String::new()),
            server_url: Mutex::new(String::new()),
            detected_gems: Mutex::new(Vec::new()),
            lab_state: Mutex::new(crate::lab_state::LabState::Idle),
            logs: Mutex::new(Vec::new()),
            gem_region: Mutex::new(CaptureRegion::default()),
            font_region: Mutex::new(CaptureRegion::default_font_panel()),
            sidebar_open: Mutex::new(true),
            game_focused: Mutex::new(false),
            trade_client: crate::trade::TradeApiClient::new(),
            server_http: reqwest::Client::new(),
            watcher_cancel: Mutex::new(None),
            comparator_data: Mutex::new(serde_json::json!({})),
            overlay_hook_stop: Mutex::new(None),
            focus_poller_stop: Mutex::new(None),
            debug_mode: Mutex::new(false),
            trade_stale_warn_secs: Mutex::new(DEFAULT_TRADE_STALE_WARN_SECS),
            trade_stale_critical_secs: Mutex::new(DEFAULT_TRADE_STALE_CRITICAL_SECS),
            trade_auto_refresh_secs: Mutex::new(DEFAULT_TRADE_AUTO_REFRESH_SECS),
            auto_trade_enabled: Mutex::new(false),
            gem_scan_generation: AtomicU64::new(0),
            font_scan_generation: AtomicU64::new(0),
            font_opened_seq: AtomicU64::new(0),
            aspirant_trial_count: AtomicU32::new(0),
            font_session: Mutex::new(crate::FontSessionData::default()),
            font_exhausted: AtomicBool::new(false),
            gem_scan_done: AtomicBool::new(false),
            in_lab: AtomicBool::new(false),
            compass_mode: Mutex::new(String::new()),
            compass_strategy: Mutex::new(String::new()),
            compass_difficulty: Mutex::new(String::new()),
            shrine_warn_enabled: Mutex::new(true),
            shrine_warn_size: Mutex::new(String::new()),
            shrine_warn_corner: Mutex::new(String::new()),
            shrine_warn_on_take: Mutex::new(String::new()),
            lab_overlays_enabled: Mutex::new(true),
            lab_mode: Mutex::new(String::new()),
            autoclear_minutes: Mutex::new(2),
            dedication_pool: Mutex::new(String::new()),
            dedication_variant: Mutex::new(String::new()),
            normal_variant: Mutex::new(String::new()),
            show_low_confidence: Mutex::new(false),
            ui_prefs: Mutex::new(std::collections::HashMap::new()),
            ssot: Mutex::new(crate::ssot::AppSsotSnapshot::default()),
            modules_enabled: Mutex::new(std::collections::HashMap::new()),
            module_handles: Mutex::new(std::collections::HashMap::new()),
            modules_shutting_down: AtomicBool::new(false),
            mercenary: Mutex::new(crate::mercenary::MercenarySlice::default()),
            merc_templates: Mutex::new(crate::mercenary::icons::TemplateStore::new()),
            merc_template_generation: AtomicU64::new(0),
        }
    }

    /// Fresh install: `apply_to_state` must fill the owner map with the registry
    /// defaults (effective from birth), and `from_state` must then write NOTHING
    /// — pinning an unchosen default in settings.json would freeze a user on it
    /// when a later version flips that default.
    #[test]
    fn a_module_left_at_its_default_is_not_written_to_settings() {
        let state = test_app_state();

        apply_to_state(&Settings::default(), &state);
        assert!(
            state
                .modules_enabled
                .lock()
                .unwrap()
                .contains_key("mercenary"),
            "precondition: the owner map is effective, so it carries every registry id",
        );

        let saved = from_state(&state);

        assert!(
            saved.modules.is_empty(),
            "an unchosen default must not reach settings.json, got {:?}",
            saved.modules,
        );
    }

    /// The five-touch-point cycle end to end: an explicit choice and a key this
    /// build does not recognise both survive `from_state` → `apply_to_state` →
    /// `from_state`. Guards the wiring, not just the pure helpers — dropping
    /// either touch point silently loses the user's choice on next launch.
    #[test]
    fn a_non_default_module_choice_round_trips_through_state() {
        let state = test_app_state();
        *state.modules_enabled.lock().unwrap() = [
            ("mercenary".to_string(), true),
            ("from_the_future".to_string(), true),
        ]
        .into_iter()
        .collect();

        let saved = from_state(&state);
        assert_eq!(
            saved.modules.get("mercenary"),
            Some(&true),
            "an explicit non-default choice must be persisted",
        );
        assert_eq!(
            saved.modules.get("from_the_future"),
            Some(&true),
            "a key from a newer build must be persisted verbatim",
        );

        // Next launch: a fresh state loads that file.
        let reloaded = test_app_state();
        apply_to_state(&saved, &reloaded);
        assert_eq!(
            reloaded.modules_enabled.lock().unwrap().get("mercenary"),
            Some(&true),
            "the persisted choice must beat the registry default on load",
        );

        assert_eq!(
            from_state(&reloaded).modules,
            saved.modules,
            "the delta must be stable across a full cycle",
        );
    }

    /// A `modules` value of the wrong SHAPE must cost only that field. Serde
    /// aborts the whole struct on a field error and `load` then falls back to
    /// `Settings::default()` — so without the tolerant reader, one bad map
    /// wipes every unrelated preference in the file.
    #[test]
    fn a_wrong_typed_modules_value_does_not_discard_the_rest_of_the_file() {
        let json = r#"{"server_url":"https://kept.example","modules":{"x":"yes"}}"#;

        let parsed: Settings = serde_json::from_str(json).expect("the file must still parse");

        assert_eq!(parsed.server_url, "https://kept.example");
        assert!(parsed.modules.is_empty(), "the unreadable entry is dropped");
    }

    /// One bad ENTRY must cost only that entry, not its valid siblings — a
    /// whole-map typed deserialize fails as a unit, which would erase the
    /// user's real choices next persist (delta absent → default reasserted).
    #[test]
    fn a_bad_modules_entry_does_not_erase_the_valid_ones() {
        let json =
            r#"{"server_url":"https://kept.example","modules":{"mercenary":true,"future_mod":"on"}}"#;

        let parsed: Settings = serde_json::from_str(json).expect("the file must still parse");

        assert_eq!(
            parsed.modules.get("mercenary"),
            Some(&true),
            "the valid bool entry survives its bad sibling"
        );
        assert!(!parsed.modules.contains_key("future_mod"), "the non-bool entry is dropped");
    }

    /// Same rule for an explicit null — the shape a hand-edited or
    /// partially-written settings file most easily ends up with.
    #[test]
    fn a_null_modules_value_does_not_discard_the_rest_of_the_file() {
        let json = r#"{"server_url":"https://kept.example","modules":null}"#;

        let parsed: Settings = serde_json::from_str(json).expect("the file must still parse");

        assert_eq!(parsed.server_url, "https://kept.example");
        assert!(parsed.modules.is_empty(), "null means no choices, not a broken file");
    }

    /// Overlay settings must survive the from_state→persist_overlay→save cycle.
    /// Regression test: from_state returns None for overlays (they're not in AppState),
    /// so persist_overlay_settings must copy them from the existing file.
    #[test]
    fn test_overlay_settings_survive_persist_cycle() {
        let existing = Settings {
            compass_overlay: Some(OverlaySettings { x: 50, y: 60, width: 400, height: 350, enabled: true }),
            pathstrip_overlay: Some(OverlaySettings { x: 100, y: 200, width: 500, height: 200, enabled: true }),
            comparator_overlay: Some(OverlaySettings { x: 10, y: 20, width: 630, height: 250, enabled: false }),
            timer_overlay: Some(OverlaySettings { x: 200, y: 500, width: 160, height: 50, enabled: true }),
            ..Settings::default()
        };

        // Simulate from_state (overlays are None)
        let mut target = Settings::default();
        assert!(target.compass_overlay.is_none());
        assert!(target.pathstrip_overlay.is_none());
        assert!(target.timer_overlay.is_none());

        // persist_overlay_settings must restore them
        super::persist_overlay_settings(&existing, &mut target);

        let compass = target.compass_overlay.expect("compass_overlay lost during persist cycle");
        assert_eq!(compass.x, 50);
        assert_eq!(compass.y, 60);
        assert_eq!(compass.width, 400);
        assert_eq!(compass.height, 350);
        assert!(compass.enabled);

        let pathstrip = target.pathstrip_overlay.expect("pathstrip_overlay lost during persist cycle");
        assert_eq!(pathstrip.x, 100);
        assert_eq!(pathstrip.width, 500);

        let comparator = target.comparator_overlay.expect("comparator_overlay lost during persist cycle");
        assert_eq!(comparator.x, 10);
        assert!(!comparator.enabled);

        let timer = target.timer_overlay.expect("timer_overlay lost during persist cycle");
        assert_eq!(timer.x, 200);
        assert_eq!(timer.width, 160);
        assert!(timer.enabled);
    }

    /// Window settings must not overwrite overlay settings in the save cycle.
    /// Regression guard: if persist_overlay_settings were ever called AFTER
    /// s.window = Some(...), it would overwrite the freshly set window settings
    /// with the file's stale version. Correct order: persist_overlay THEN set window.
    #[test]
    fn test_window_settings_not_overwritten_by_overlay_persist() {
        let existing = Settings {
            window: Some(WindowSettings { x: 0, y: 0, width: 800, height: 600, maximized: false }),
            compass_overlay: Some(OverlaySettings { x: 50, y: 60, width: 400, height: 350, enabled: true }),
            ..Settings::default()
        };

        let mut target = Settings::default();
        // Simulate the on-close save order: persist_overlay THEN set window
        super::persist_overlay_settings(&existing, &mut target);
        target.window = Some(WindowSettings { x: 200, y: 300, width: 1024, height: 768, maximized: false });

        // Window should be the NEW value, not the existing file value
        let win = target.window.expect("window settings lost");
        assert_eq!(win.x, 200);
        assert_eq!(win.width, 1024);

        // Overlay should still be from existing file
        let compass = target.compass_overlay.expect("compass_overlay lost");
        assert_eq!(compass.x, 50);
    }
}

/// Apply loaded settings to AppState.
pub fn apply_to_state(settings: &Settings, state: &crate::AppState) {
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = settings.client_txt_path.clone();
    *state.server_url.lock().unwrap_or_else(|e| e.into_inner()) = settings.server_url.clone();
    *state.gem_region.lock().unwrap_or_else(|e| e.into_inner()) = settings.gem_region.clone();
    *state.font_region.lock().unwrap_or_else(|e| e.into_inner()) = settings.font_region.clone();
    *state.sidebar_open.lock().unwrap_or_else(|e| e.into_inner()) = settings.sidebar_open;
    *state.trade_stale_warn_secs.lock().unwrap_or_else(|e| e.into_inner()) = settings.trade_stale_warn_secs;
    *state.trade_stale_critical_secs.lock().unwrap_or_else(|e| e.into_inner()) = settings.trade_stale_critical_secs;
    *state.trade_auto_refresh_secs.lock().unwrap_or_else(|e| e.into_inner()) = settings.trade_auto_refresh_secs;
    *state.auto_trade_enabled.lock().unwrap_or_else(|e| e.into_inner()) = settings.auto_trade_enabled;
    *state.compass_mode.lock().unwrap_or_else(|e| e.into_inner()) = settings.compass_mode.clone();
    *state.compass_strategy.lock().unwrap_or_else(|e| e.into_inner()) = settings.compass_strategy.clone();
    *state.compass_difficulty.lock().unwrap_or_else(|e| e.into_inner()) = settings.compass_difficulty.clone();
    *state.shrine_warn_enabled.lock().unwrap_or_else(|e| e.into_inner()) = settings.shrine_warn_enabled;
    *state.shrine_warn_size.lock().unwrap_or_else(|e| e.into_inner()) = settings.shrine_warn_size.clone();
    *state.shrine_warn_corner.lock().unwrap_or_else(|e| e.into_inner()) = settings.shrine_warn_corner.clone();
    *state.shrine_warn_on_take.lock().unwrap_or_else(|e| e.into_inner()) = settings.shrine_warn_on_take.clone();
    *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) = settings.lab_overlays_enabled;
    *state.lab_mode.lock().unwrap_or_else(|e| e.into_inner()) = settings.lab_mode.clone();
    *state.autoclear_minutes.lock().unwrap_or_else(|e| e.into_inner()) = settings.autoclear_minutes;
    *state.dedication_pool.lock().unwrap_or_else(|e| e.into_inner()) = settings.dedication_pool.clone();
    *state.dedication_variant.lock().unwrap_or_else(|e| e.into_inner()) = settings.dedication_variant.clone();
    *state.normal_variant.lock().unwrap_or_else(|e| e.into_inner()) = settings.normal_variant.clone();
    *state.show_low_confidence.lock().unwrap_or_else(|e| e.into_inner()) = settings.show_low_confidence;
    *state.ui_prefs.lock().unwrap_or_else(|e| e.into_inner()) = settings.ui_prefs.clone();
    // The owner map holds the EFFECTIVE state, not the persisted delta:
    // registry defaults overlaid with what the user chose (see modules.rs).
    *state.modules_enabled.lock().unwrap_or_else(|e| e.into_inner()) =
        crate::modules::effective_modules(&settings.modules, &crate::modules::module_lifecycles());
}
