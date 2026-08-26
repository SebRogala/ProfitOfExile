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
    /// The merc verdict overlay's geometry (POE-199). Placed by the user in
    /// Settings → Overlay Positions; the `mercenary` MODULE flag, not
    /// `enabled`, decides whether the window exists.
    pub mercenary_overlay: Option<OverlaySettings>,
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
    /// The temple reader's remembered anchor scale (POE-171 D0), keyed on the
    /// capture dimensions it was measured at. Self-invalidating: the reader
    /// ignores it at any other capture size and re-verifies it against the NCC
    /// floor every time, so a stale one costs one extra match and never a wrong
    /// board. `None` on a fresh install and after a resolution change.
    #[serde(default)]
    pub temple_calibration: Option<crate::temple::anchor::AnchorCalibration>,
    /// The four tunable fields of the temple strategy profile. Absent means the
    /// Locus/Doryani Rush's own values — see `TempleProfileSettings::default`.
    ///
    /// **camelCase inside**, unlike the rest of this file. Both temple structs
    /// are the webview's wire types first (they are command arguments and slice
    /// fields), and one struct with two serialisations plus a DTO to convert
    /// between them is a worse trade than a documented deviation in one block
    /// of the file. See `TempleConfig`'s own note.
    #[serde(default)]
    pub temple_profile: crate::temple::slice::TempleProfileSettings,
    /// The two temple config flags: the Atlas passive and the scarab.
    /// camelCase inside — see `temple_profile`.
    #[serde(default)]
    pub temple_config: crate::temple::strategy::TempleConfig,
    /// Opening stones per incursion, 0..=2. The panel does not print the count,
    /// so it is the one board fact the user supplies.
    #[serde(default = "default_temple_keys")]
    pub temple_keys: u8,
    /// The guides taking NO part in the merc verdict (POE-199).
    ///
    /// A TYPED field rather than a `ui_prefs` entry, and deliberately against
    /// ADR-013's default: two windows read this value, so the map — fetched
    /// once per webview and written back with no notification — could leave
    /// the page and the overlay printing different headlines for one
    /// mercenary. Rust owns it and `ssot::compose_snapshot` echoes it.
    ///
    /// `None` means NEVER WRITTEN, which is what the one-time migration from
    /// the old `mercSourcesOff` preference keys on — see
    /// `mercenary::sources::migrate_sources_off`. It is not the same as
    /// `Some(vec![])`, which means the user chose "every guide on".
    #[serde(default)]
    pub merc_sources_off: Option<Vec<String>>,
    /// Whether the captured mercenary is auto-searched on the trade site
    /// (POE-202).
    ///
    /// TYPED for the same reason as `merc_sources_off`: the page and the
    /// verdict overlay both render off the merc slice, and the Rust capture
    /// loop is the thing that has to READ this to decide whether to spend a
    /// search — a `ui_prefs` entry lives in the webview and the loop cannot see
    /// it. Default ON (`mercenary::DEFAULT_TRADE_AUTO`).
    #[serde(default = "default_merc_trade_auto")]
    pub merc_trade_auto: bool,
    /// The lowest support tier the captured search accepts, 1..=3 (POE-202).
    /// Typed for the same reason as `merc_trade_auto`. CLAMPED on load, not
    /// refused and not reset: a hand-edited 0 means "as loose as it goes" and
    /// becomes 1, a 9 means "exactly as read" and becomes 3, which is what the
    /// query builder would do with either anyway. Reported, because the file
    /// and the running value now disagree.
    #[serde(default = "default_merc_tier_floor")]
    pub merc_tier_floor: u8,
}

/// The shipped auto-search default — ON, see `mercenary::DEFAULT_TRADE_AUTO`.
fn default_merc_trade_auto() -> bool {
    crate::mercenary::DEFAULT_TRADE_AUTO
}

/// The shipped tier floor — 3, the mercenary exactly as read.
fn default_merc_tier_floor() -> u8 {
    crate::mercenary::DEFAULT_TIER_FLOOR
}

/// The common case: one opening stone. `u8`'s own default is 0, which is a
/// legal but uncommon board, so this field carries its own default fn.
fn default_temple_keys() -> u8 {
    crate::temple::slice::default_keys()
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
            mercenary_overlay: None,
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
            temple_calibration: None,
            temple_profile: Default::default(),
            temple_config: Default::default(),
            temple_keys: default_temple_keys(),
            // `None`, not the empty list: never written is what the one-time
            // migration from the `mercSourcesOff` preference keys on.
            merc_sources_off: None,
            merc_trade_auto: default_merc_trade_auto(),
            merc_tier_floor: default_merc_tier_floor(),
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
    let temple = state.temple_settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
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
        mercenary_overlay: None, // Overlay settings saved separately, not from AppState
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
        // One AppState Mutex, four settings fields: the aggregate is what the
        // loop and the commands share, but splitting it on disk keeps a
        // hand-edited file readable and lets one bad field default on its own.
        temple_calibration: temple.calibration,
        temple_profile: temple.profile,
        temple_config: temple.config,
        temple_keys: temple.keys,
        // Written from the owner, so the first save after the migration turns
        // the `None` that keeps re-reading the old preference into a real
        // value — which is what ends the migration.
        merc_sources_off: Some(
            state.merc_sources_off.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        ),
        merc_trade_auto: *state.merc_trade_auto.lock().unwrap_or_else(|e| e.into_inner()),
        merc_tier_floor: *state.merc_tier_floor.lock().unwrap_or_else(|e| e.into_inner()),
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
    target.mercenary_overlay = existing.mercenary_overlay.clone();
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
            font_scan_live_gen: AtomicU64::new(0),
            font_opened_seq: AtomicU64::new(0),
            aspirant_trial_count: AtomicU32::new(0),
            font_session: Mutex::new(crate::FontSessionData::default()),
            in_lab: AtomicBool::new(false),
            game_in_foreground: AtomicBool::new(false),
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
            merc_sources_off: Mutex::new(Vec::new()),
            merc_trade_auto: Mutex::new(crate::mercenary::DEFAULT_TRADE_AUTO),
            merc_tier_floor: Mutex::new(crate::mercenary::DEFAULT_TIER_FLOOR),
            merc_trade_cache: Mutex::new(std::collections::HashMap::new()),
            merc_sync: Mutex::new(crate::mercenary::sync::SyncState::default()),
            merc_burst: Mutex::new(crate::mercenary::trigger::BurstGate::default()),
            merc_template_generation: AtomicU64::new(0),
            temple: Mutex::new(crate::temple::slice::TempleSlice::default()),
            temple_settings: Mutex::new(crate::temple::slice::TempleSettings::shipped()),
            temple_rearm: AtomicU64::new(0),
        }
    }

    /// Fresh install: `apply_to_state` must fill the owner map with the registry
    /// defaults (effective from birth), and `from_state` must then write NOTHING
    /// — pinning an unchosen default in settings.json would freeze a user on it
    /// when a later version flips that default.
    #[test]
    fn a_module_left_at_its_default_is_not_written_to_settings() {
        let state = test_app_state();

        let _ = apply_to_state(&Settings::default(), &state);
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
        let _ = apply_to_state(&saved, &reloaded);
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

    /// The temple settings survive the five-touch-point cycle: one AppState
    /// Mutex out to four settings fields and back. Fails if `from_state` or
    /// `apply_to_state` drops one of the four — which would silently reset the
    /// user's tuning, or the anchor calibration, on next launch.
    #[test]
    fn temple_settings_round_trip_through_state() {
        let state = test_app_state();
        let chosen = crate::temple::slice::TempleSettings {
            calibration: Some(crate::temple::anchor::AnchorCalibration {
                screen_w: 1539,
                screen_h: 865,
                scale: 1.13,
            }),
            profile: crate::temple::slice::TempleProfileSettings {
                apex_score: 6.5,
                path_cost: 0.75,
                reroll_until_favourable: true,
                r4_keep_upgrade_targets: false,
            },
            config: crate::temple::strategy::TempleConfig {
                artefacts_of_the_vaal: false,
                scarab_of_timelines: true,
            },
            keys: 2,
        };
        *state.temple_settings.lock().unwrap() = chosen.clone();

        let saved = from_state(&state);
        assert_eq!(saved.temple_keys, 2);
        assert_eq!(saved.temple_calibration, chosen.calibration);

        // Next launch: a fresh state loads that file.
        let reloaded = test_app_state();
        let _ = apply_to_state(&saved, &reloaded);

        assert_eq!(*reloaded.temple_settings.lock().unwrap(), chosen);
    }

    /// The slice's settings echo is seeded at load, not at loop start.
    ///
    /// The page and the overlay render the keys, config flags and profile
    /// controls from `AppState.temple` alone (ADR-014: a page reads slices, not
    /// module state). With the module switched OFF no loop ever publishes, so
    /// without this seeding every control would sit at the derive default —
    /// keys 0 — while `settings.json` said otherwise, with nothing to correct
    /// it. Fails if `apply_to_state` stops seeding, or seeds from the raw file
    /// rather than from the validated owner.
    #[test]
    fn loading_settings_seeds_the_slice_echo_the_page_renders_its_controls_from() {
        let settings = Settings {
            temple_keys: 2,
            temple_config: crate::temple::strategy::TempleConfig {
                artefacts_of_the_vaal: false,
                scarab_of_timelines: true,
            },
            temple_profile: crate::temple::slice::TempleProfileSettings {
                apex_score: 6.5,
                path_cost: 0.75,
                reroll_until_favourable: true,
                r4_keep_upgrade_targets: false,
            },
            ..Settings::default()
        };
        let state = test_app_state();

        let _ = apply_to_state(&settings, &state);

        let slice = state.temple.lock().unwrap().clone();
        assert_eq!(slice.keys, 2);
        assert_eq!(slice.config, settings.temple_config);
        assert_eq!(slice.profile, settings.temple_profile);
    }

    /// A file written before POE-202 carries neither trade field. The
    /// auto-search must come up ON and the floor at "exactly as read" — a
    /// `bool`/`u8` default would give false and 0, which is a tier that does
    /// not exist.
    #[test]
    fn a_settings_file_without_the_trade_fields_loads_the_shipped_defaults() {
        let parsed: Settings = serde_json::from_str(r#"{"server_url":"https://kept.example"}"#)
            .expect("an older file must still parse");
        let state = test_app_state();

        let _ = apply_to_state(&parsed, &state);

        assert!(*state.merc_trade_auto.lock().unwrap(), "the auto-search ships on");
        assert_eq!(*state.merc_tier_floor.lock().unwrap(), 3);
    }

    /// A hand-edited 0 is a user asking for the loosest search there is.
    /// Clamped UP to the loosest tier that exists — resetting to the shipped 3
    /// would answer "as loose as possible" with the tightest query there is —
    /// and reported, because the file and the running value now disagree.
    #[test]
    fn a_tier_floor_below_the_lowest_tier_is_clamped_to_it_and_reported() {
        let settings = Settings { merc_tier_floor: 0, ..Settings::default() };
        let state = test_app_state();

        let rejected = apply_to_state(&settings, &state);

        assert_eq!(*state.merc_tier_floor.lock().unwrap(), 1);
        assert!(
            rejected.iter().any(|line| line.contains("tier floor 0") && line.contains("using 1")),
            "the disagreement must be surfaced: {rejected:?}",
        );
    }

    /// …and the other end, where the clamp goes the other way.
    #[test]
    fn a_tier_floor_above_the_highest_tier_is_clamped_to_it_and_reported() {
        let settings = Settings { merc_tier_floor: 9, ..Settings::default() };
        let state = test_app_state();

        let rejected = apply_to_state(&settings, &state);

        assert_eq!(*state.merc_tier_floor.lock().unwrap(), 3);
        assert!(
            rejected.iter().any(|line| line.contains("tier floor 9") && line.contains("using 3")),
            "the disagreement must be surfaced: {rejected:?}",
        );
    }

    /// The clamp must not touch a tier that IS a tier, and must say nothing
    /// about it — a rejection line for a legal value would train the user to
    /// ignore the list.
    #[test]
    fn a_tier_floor_inside_the_range_is_applied_as_written() {
        let settings = Settings { merc_tier_floor: 2, ..Settings::default() };
        let state = test_app_state();

        let rejected = apply_to_state(&settings, &state);

        assert_eq!(*state.merc_tier_floor.lock().unwrap(), 2);
        assert!(!rejected.iter().any(|line| line.contains("tier floor")), "{rejected:?}");
    }

    /// The two commands persist through this path, so a user who switches the
    /// auto-search off and loosens the floor must find both still set on the
    /// next launch.
    #[test]
    fn the_trade_toggle_and_floor_round_trip_through_state() {
        let state = test_app_state();
        *state.merc_trade_auto.lock().unwrap() = false;
        *state.merc_tier_floor.lock().unwrap() = 1;

        let saved = from_state(&state);
        let reloaded = test_app_state();
        let _ = apply_to_state(&saved, &reloaded);

        assert!(!*reloaded.merc_trade_auto.lock().unwrap());
        assert_eq!(*reloaded.merc_tier_floor.lock().unwrap(), 1);
    }

    /// A settings file written before POE-199 carries the guide set only in
    /// the ADR-013 preference map. Loading it must move the user's choice
    /// across once, or every guide they switched off comes back on.
    #[test]
    fn a_pre_poe199_file_migrates_the_guide_set_out_of_the_prefs_map() {
        let settings = Settings {
            ui_prefs: [(
                crate::mercenary::sources::LEGACY_PREF_KEY.to_string(),
                "guide-a".to_string(),
            )]
            .into_iter()
            .collect(),
            ..Settings::default()
        };
        let state = test_app_state();

        let _ = apply_to_state(&settings, &state);

        assert_eq!(
            *state.merc_sources_off.lock().unwrap(),
            vec!["guide-a".to_string()],
        );
    }

    /// And it happens exactly once: the first save writes the typed field, and
    /// from then on the stale preference must not switch a guide the user has
    /// since turned back on off again.
    #[test]
    fn a_written_guide_set_outranks_the_stale_preference_on_the_next_load() {
        let state = test_app_state();
        *state.merc_sources_off.lock().unwrap() = Vec::new();
        let mut saved = from_state(&state);
        saved.ui_prefs.insert(
            crate::mercenary::sources::LEGACY_PREF_KEY.to_string(),
            "guide-a".to_string(),
        );
        assert_eq!(
            saved.merc_sources_off,
            Some(Vec::new()),
            "saving must turn the never-written None into a real value",
        );

        let reloaded = test_app_state();
        let _ = apply_to_state(&saved, &reloaded);

        assert!(
            reloaded.merc_sources_off.lock().unwrap().is_empty(),
            "the typed field said every guide is on — the old pref is ignored",
        );
    }

    /// A guide id this build does not know is dropped rather than failing the
    /// load, and the rejection is reported so the file and the running value
    /// disagreeing is visible.
    #[test]
    fn an_unknown_guide_in_the_file_is_dropped_and_reported() {
        let settings = Settings {
            merc_sources_off: Some(vec!["guide-b".to_string(), "guide-zzz".to_string()]),
            ..Settings::default()
        };
        let state = test_app_state();

        let rejected = apply_to_state(&settings, &state);

        assert_eq!(
            *state.merc_sources_off.lock().unwrap(),
            vec!["guide-b".to_string()],
        );
        assert!(
            rejected.iter().any(|line| line.contains("guide-zzz")),
            "the dropped id must be reported, got {rejected:?}",
        );
    }

    /// The echo is the value IN FORCE, not the value on disk.
    ///
    /// A rejected key count falls back to the default and the module runs on
    /// it; echoing the file's 9 would show the user a control set to a number
    /// nothing is using. Fails if the seeding reads `settings` instead of the
    /// owner it just wrote.
    #[test]
    fn a_rejected_setting_is_echoed_as_the_value_actually_in_force() {
        let settings = Settings { temple_keys: 9, ..Settings::default() };
        let state = test_app_state();

        let _ = apply_to_state(&settings, &state);

        assert_eq!(state.temple.lock().unwrap().keys, crate::temple::slice::default_keys());
    }

    /// A file with no temple keys at all — every build before POE-171 — loads
    /// as the Rush with one key, not as zeros. Fails if a `#[serde(default)]`
    /// is dropped or if `temple_keys` falls back to `u8::default()`.
    #[test]
    fn a_settings_file_without_temple_keys_loads_the_shipped_defaults() {
        let parsed: Settings = serde_json::from_str(r#"{"server_url":"https://kept.example"}"#)
            .expect("an older file must still parse");

        let state = test_app_state();
        let _ = apply_to_state(&parsed, &state);

        assert_eq!(
            *state.temple_settings.lock().unwrap(),
            crate::temple::slice::TempleSettings::shipped(),
        );
    }

    /// A hand-edited file carrying a key count or a profile weight this build
    /// rejects must not become the running value. `apply_to_state` has no error
    /// channel, so the fallback is the only honest option — and a stored NaN
    /// would make every later ranking arbitrary with nothing on screen to say
    /// why. Fails if either field is copied through unvalidated.
    #[test]
    fn out_of_range_temple_settings_fall_back_instead_of_being_applied() {
        let settings = Settings {
            temple_keys: 9,
            temple_profile: crate::temple::slice::TempleProfileSettings {
                apex_score: f64::NAN,
                ..Default::default()
            },
            ..Settings::default()
        };
        let state = test_app_state();

        let _ = apply_to_state(&settings, &state);

        let applied = state.temple_settings.lock().unwrap().clone();
        assert_eq!(applied.keys, 1, "9 keys is not a board the game produces");
        assert_eq!(
            applied.profile,
            crate::temple::slice::TempleProfileSettings::default(),
            "a NaN weight must not reach the ranking",
        );
    }

    /// The other half of the fallback: it is REPORTED. Without this the file
    /// says 9 keys, the module runs on 1, and nothing anywhere tells the user
    /// which of the two they are looking at.
    ///
    /// Fails if a rejection stops being returned, or if the line drops the
    /// field name or the validator's reason — both are what make the log line
    /// actionable rather than just alarming.
    #[test]
    fn a_rejected_temple_setting_is_reported_with_its_field_and_reason() {
        let settings = Settings {
            temple_keys: 9,
            temple_profile: crate::temple::slice::TempleProfileSettings {
                apex_score: f64::NAN,
                ..Default::default()
            },
            ..Settings::default()
        };
        let state = test_app_state();

        let rejected = apply_to_state(&settings, &state);

        assert_eq!(rejected.len(), 2, "both bad fields report, got {rejected:?}");
        let profile = rejected
            .iter()
            .find(|line| line.contains("profile"))
            .expect("the profile rejection must name the field");
        assert!(
            profile.contains("apex_score") && profile.contains("using default"),
            "the line must carry the validator's own reason, got {profile:?}",
        );
        let keys = rejected
            .iter()
            .find(|line| line.contains("keys"))
            .expect("the key rejection must name the field");
        assert!(
            keys.contains('9') && keys.contains("using default"),
            "the line must name the rejected value, got {keys:?}",
        );
    }

    /// A clean file reports nothing. Fails if `apply_to_state` reports on the
    /// happy path — a log line on every launch is noise that trains the user to
    /// ignore the one that matters.
    #[test]
    fn a_settings_file_this_build_accepts_reports_no_rejections() {
        let state = test_app_state();

        assert!(apply_to_state(&Settings::default(), &state).is_empty());
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
            mercenary_overlay: Some(OverlaySettings { x: 300, y: 40, width: 460, height: 150, enabled: true }),
            ..Settings::default()
        };

        // Simulate from_state (overlays are None)
        let mut target = Settings::default();
        assert!(target.compass_overlay.is_none());
        assert!(target.pathstrip_overlay.is_none());
        assert!(target.timer_overlay.is_none());
        assert!(target.mercenary_overlay.is_none());

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

        let mercenary = target
            .mercenary_overlay
            .expect("mercenary_overlay lost during persist cycle");
        assert_eq!(mercenary.x, 300);
        assert_eq!(mercenary.width, 460);
        assert!(mercenary.enabled);
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
///
/// Returns one ready-to-log line per value this build refused, in the order the
/// fields are applied. Empty on a clean load, which is the common case.
///
/// A fallback that leaves no trace is the failure mode this return value
/// exists for: the file still says `temple_keys: 9`, the module runs on 1, and
/// nothing anywhere tells the user which of the two they are looking at. There
/// is no error channel here — a rejected value must not become the running one
/// and a rejected value must not fail the whole load either — so the rejections
/// come back as text and the caller logs them.
#[must_use = "a silent settings rejection is the bug this return value exists to prevent"]
pub fn apply_to_state(settings: &Settings, state: &crate::AppState) -> Vec<String> {
    let mut rejected: Vec<String> = Vec::new();
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
    // The four temple fields recombine into the one Mutex the module reads. A
    // key count from a file this build would reject falls back to the default
    // rather than poisoning every later ranking — and says so, through
    // `rejected`, because the file and the running value now disagree.
    *state.temple_settings.lock().unwrap_or_else(|e| e.into_inner()) =
        crate::temple::slice::TempleSettings {
            calibration: settings.temple_calibration,
            profile: match settings.temple_profile.validate() {
                Ok(()) => settings.temple_profile.clone(),
                Err(why) => {
                    rejected.push(format!(
                        "temple settings: profile rejected ({why}), using default"
                    ));
                    Default::default()
                }
            },
            config: settings.temple_config.clone(),
            keys: match crate::temple::slice::validate_keys(settings.temple_keys) {
                Ok(()) => settings.temple_keys,
                Err(why) => {
                    rejected.push(format!(
                        "temple settings: keys rejected ({why}), using default"
                    ));
                    crate::temple::slice::default_keys()
                }
            },
        };

    // The enabled-guide set (POE-199), with its one-time migration from the
    // ADR-013 preference. A stored id this build does not know is dropped
    // rather than failing the load, and says so — the file and the running
    // value now disagree, which is exactly what `rejected` is for.
    {
        let legacy = settings
            .ui_prefs
            .get(crate::mercenary::sources::LEGACY_PREF_KEY);
        let (accepted, refused) = crate::mercenary::sources::migrate_sources_off(
            settings.merc_sources_off.as_ref(),
            legacy,
        );
        for id in refused {
            rejected.push(format!(
                "merc settings: {id:?} is not a guide, ignoring it in the off-list"
            ));
        }
        *state.merc_sources_off.lock().unwrap_or_else(|e| e.into_inner()) = accepted;
    }

    // The trade auto-search (POE-202). The floor is CLAMPED rather than
    // refused: a file holding a tier that is not a tier still describes a user
    // who wants loosening, and failing the load over it would take every other
    // preference in the file with it.
    //
    // Clamped to the nearest legal tier and NOT reset to the default: 0 is a
    // user asking for the loosest search there is, and answering it with 3 —
    // the tightest — inverts what the file says. Reported either way, because
    // the file and the running value now disagree.
    *state.merc_trade_auto.lock().unwrap_or_else(|e| e.into_inner()) = settings.merc_trade_auto;
    *state.merc_tier_floor.lock().unwrap_or_else(|e| e.into_inner()) =
        match crate::mercenary::validate_tier_floor(settings.merc_tier_floor) {
            Ok(floor) => floor,
            Err(why) => {
                let clamped = settings.merc_tier_floor.clamp(1, 3);
                rejected.push(format!("merc settings: {why}, using {clamped}"));
                clamped
            }
        };

    // Seed the slice's settings echo, so the page and the overlay render the
    // persisted key count, flags and profile from the first poll — including
    // while the module is OFF and no loop will ever publish them. Taken from
    // the owner just written rather than from `settings`, so a rejected value
    // is echoed as the one actually in force. The `temple_settings` guard is
    // released before `temple` is taken; nothing else locks the two together.
    let temple = state.temple_settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
    {
        let mut slice = state.temple.lock().unwrap_or_else(|e| e.into_inner());
        slice.keys = temple.keys;
        slice.config = temple.config;
        slice.profile = temple.profile;
    }

    rejected
}
