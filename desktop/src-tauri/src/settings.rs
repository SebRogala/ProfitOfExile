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
    /// The gem OCR rect the user placed in Settings, or `None` when they never
    /// placed one — in which case the app derives it from
    /// [`crate::GEM_REGION_REF`] and the measured screen (POE-233).
    ///
    /// **Migration.** Before POE-233 this field was non-optional and every file
    /// ever written carried the shipped 1080p literal, whether or not the user
    /// had touched the region. A persisted rect EQUAL to
    /// [`crate::SHIPPED_GEM_REGION_1080P`] is therefore read back as `None`
    /// ([`user_set_region`]), so those users get the scaled default instead of a
    /// 1080p rect frozen as an override. The cost is exact and accepted: a user
    /// who deliberately placed the region on the shipped literal loses that
    /// choice and gains the same rect on a 1080p screen, and a different one on
    /// any other screen (on the 1200p machine the same user gets
    /// `{33, 50, 611, 83}`).
    ///
    /// **`skip_serializing_if` is a rollback guard, not tidiness.** Serialised
    /// as `"gem_region": null`, this field is REJECTED by the pre-POE-233 build
    /// whose field was non-optional — `load` there discards the whole file and
    /// the next persist overwrites it with defaults, so a beta-channel rollback
    /// would cost the user every setting. Omitting the key entirely reads as
    /// "absent" on both builds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_region: Option<CaptureRegion>,
    /// The font panel rect the user placed, or `None` — same migration rule as
    /// [`Settings::gem_region`], against [`crate::SHIPPED_FONT_PANEL_1080P`],
    /// and the same rollback guard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_region: Option<CaptureRegion>,
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
    /// The screen and game-UI scale the merc module last MEASURED on the frame
    /// (POE-214 D2), remembered across restarts. `None` on a fresh install and
    /// until the first recruit window is fitted.
    ///
    /// Loaded back as [`crate::ssot::ScreenScaleSource::Remembered`], which is
    /// the label a consumer weighs "not measured this run" by. It is NOT
    /// self-invalidating the way `temple_calibration` is — nothing re-verifies
    /// it — so a value carried over from another monitor stays readable until
    /// the merc module measures this one; see `apply_to_state`.
    #[serde(default)]
    pub screen_scale: Option<ScreenScaleSetting>,
    /// Where the user put each overlay WIDGET (POE-225), keyed
    /// `"<module>.<widget>"` — `"temple.board"`, `"temple.advice"`.
    ///
    /// A map rather than a field per widget because the widgets are declared in
    /// the frontend registry (`src/lib/overlay/widgets/widget-registry.ts`) and
    /// a module adding one must not need a Rust field, a getter, a setter and a
    /// line in [`persist_overlay_settings`] before it can be placed. A key this
    /// build no longer declares is carried through untouched rather than
    /// pruned: an id is dropped only when its module is removed, and silently
    /// deleting placements on a downgrade is worse than keeping a few dead
    /// rows.
    ///
    /// `BTreeMap`, so the file is written in a stable order and a diff of
    /// settings.json is readable.
    ///
    /// UNLIKE [`OverlaySettings`] this is owned by an `AppState` mutex
    /// (`AppState.widgets`) and therefore travels through [`from_state`] like
    /// any other owned field — it must NOT be added to
    /// [`persist_overlay_settings`], which would carry the file's stale copy
    /// back over what the owner just wrote.
    #[serde(default)]
    pub widgets: std::collections::BTreeMap<String, WidgetGeometry>,
}

/// One overlay widget's placement inside its module's fullscreen window
/// (POE-225).
///
/// PHYSICAL pixels, window-relative. The module's window IS one monitor and
/// every capture is that same monitor — the GAME's, on both sides, since
/// POE-237 — so window-relative physical px are also capture px, which is the
/// unit a game-anchored widget would have to be placed in anyway; user-placed
/// and game-anchored widgets therefore share one unit with no conversion
/// between them. Shipped defaults are CSS px
/// in the frontend registry and are converted once, by `physicalGeometry`, the
/// same way `MERC_OVERLAY_DEFAULTS` is.
///
/// `width`/`height` rather than `w`/`h` to match [`OverlaySettings`], which is
/// the other rectangle in this file.
///
/// `visible` is the user's Show checkbox, not a runtime state: a widget hidden
/// here is not rendered at all, and one that has never been configured has no
/// entry and renders at its shipped default.
///
/// `host_width`/`host_height` are the HOST WINDOW this rectangle was placed
/// against, in the same physical pixels (POE-239). They exist because the
/// rectangle alone does not say what it meant: a widget saved near the
/// bottom-right of a 3840x2160 monitor is a pinned-to-the-edge widget on a
/// 1920x1080 one, and the load-time clamp — the only thing that stopped it
/// rendering off-screen — throws the intent away permanently the next time the
/// user presses Save. With the host size stored, the frontend's `rebase()`
/// scales the rectangle back into proportion first and the clamp goes back to
/// being the last-resort safety it was meant to be.
///
/// `0` means UNKNOWN, which every row written before this field existed is:
/// `#[serde(default)]` fills it in, and `rebase` leaves an unknown-host row
/// exactly as it found it, so those rows behave as they always did.
///
/// snake_case on the wire, like the rest of this file. The webview mirror in
/// `overlay/widgets/widget-geometry.ts` spells them the same way; the other
/// four field names happen to be identical in both conventions, which is why
/// there is no `rename_all` here to copy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WidgetGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub visible: bool,
    #[serde(default)]
    pub host_width: u32,
    #[serde(default)]
    pub host_height: u32,
}

/// One entry of [`widgets_for_module`], in the shape the webview reads.
///
/// A struct rather than a `(String, WidgetGeometry)` tuple: serde renders a
/// tuple as a two-element ARRAY, so the TypeScript side would be indexing
/// `entry[0]` / `entry[1]` and a field added later would silently shift.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WidgetGeometryEntry {
    pub id: String,
    pub geometry: WidgetGeometry,
}

/// Every stored placement belonging to `module`, in key order.
///
/// The `.` is part of the prefix, so a module called `temple` does not collect
/// `temple2.board`. Pure, and separate from the command, so that boundary is
/// testable off a running app.
pub fn widgets_for_module(
    widgets: &std::collections::BTreeMap<String, WidgetGeometry>,
    module: &str,
) -> Vec<WidgetGeometryEntry> {
    let prefix = format!("{module}.");
    widgets
        .iter()
        .filter(|(id, _)| id.starts_with(&prefix))
        .map(|(id, geometry)| WidgetGeometryEntry {
            id: id.clone(),
            geometry: *geometry,
        })
        .collect()
}

/// The persisted form of [`crate::ssot::ScreenSlice`] (POE-214 D2).
///
/// snake_case inside, like the rest of this file and unlike the two temple
/// structs above: nothing sends THIS struct to a webview. A window reads the
/// scale from `ssot.screen`, which is `ScreenSlice`'s own camelCase.
///
/// **No `source` field, deliberately.** Only a frame measurement is ever
/// written here (see [`Self::from_slice`]) and everything read back is
/// `Remembered`, so a stored label could only restate one of those two facts —
/// and a hand-edited one would invite a reader to trust a cue that never
/// measured anything.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScreenScaleSetting {
    /// Screen width in physical px, as captured.
    pub width: u32,
    /// Screen height in physical px, as captured.
    pub height: u32,
    /// Game-UI px per reference-fixture px — the unit is documented on
    /// [`crate::ssot::ScreenSlice`], which owns it.
    pub ui_scale: f32,
    /// Unix ms the measurement was taken at, carried through unchanged so an
    /// age survives the restart with the number it belongs to.
    pub measured_at_ms: u64,
    /// WHICH display it was measured on (POE-237) — `crate::capture::Capture`'s
    /// id space, mirrored from [`crate::ssot::ScreenSlice::monitor_id`].
    ///
    /// Persisted because the startup seed is exactly where it is needed: a
    /// player whose two monitors are the same resolution used to load last
    /// session's scale and have nothing able to notice the game had moved, and
    /// the lazy prune (`ssot::drop_if_mismatched`) can only compare what the
    /// file carried. `#[serde(default)]` fills `0` — UNKNOWN — for every file
    /// written before this field existed, and an unknown id prunes on the
    /// dimensions alone, which is what those files have always done.
    #[serde(default)]
    pub monitor_id: u32,
    /// The measured display's top-left in virtual-desktop PHYSICAL px.
    /// `#[serde(default)]` fills `(0, 0)`, which is both the primary monitor
    /// and the unknown value — the id, not this, is the identity.
    #[serde(default)]
    pub origin: (i32, i32),
}

impl ScreenScaleSetting {
    /// The persistable form of `slice`, or `None` when it must not be stored.
    ///
    /// This is the normative answer to "what may be remembered", and the
    /// persist TRIGGER ([`crate::ssot::should_remember_screen`]) only decides
    /// when to spend a write.
    ///
    /// `verified_this_session` is DROPPED here — this struct has no such field
    /// (POE-240). Verification is a statement about the run that made it, so
    /// carrying it into the file would let next session's startup seed claim a
    /// screen it has not looked at.
    ///
    /// - `MercFrame` — stored. The gold frame is the cue POE-214 exists to
    ///   measure.
    /// - `MercOcr` — refused. It is the line-pitch estimate that sits 6-12 px
    ///   off the frame, and next session would read it back under the same
    ///   `remembered` label a real measurement gets, with no way to tell them
    ///   apart. **Refusing it must not null what is already stored**, and this
    ///   function cannot tell the difference: `persist_settings` rewrites the
    ///   whole file from `from_state`, so on its own a `None` here would erase
    ///   a remembered measurement on the first save after an OCR-only tick —
    ///   for exactly the users whose frame fit never lands. That is what
    ///   [`preserve_screen_scale`] exists to prevent; the two are one rule read
    ///   together.
    /// - `Remembered` — stored unchanged, which is what lets a session that
    ///   loaded a value and never measured write back what it loaded instead of
    ///   nulling it on the first unrelated save.
    pub fn from_slice(slice: &crate::ssot::ScreenSlice) -> Option<Self> {
        match slice.source {
            crate::ssot::ScreenScaleSource::MercOcr => None,
            crate::ssot::ScreenScaleSource::MercFrame
            | crate::ssot::ScreenScaleSource::Remembered => Some(Self {
                width: slice.width,
                height: slice.height,
                ui_scale: slice.ui_scale,
                measured_at_ms: slice.measured_at_ms,
                monitor_id: slice.monitor_id,
                origin: slice.origin,
            }),
        }
    }

    /// The stored numbers as a slice, always labelled `Remembered` — a load is
    /// not a measurement, and the label is the only thing that says so.
    ///
    /// `verified_this_session` is seeded from
    /// [`crate::ssot::verifies_the_screen`], which answers `false` here for the
    /// same reason: nothing this run has looked at the screen (POE-240). The
    /// field is deliberately absent from the STORED struct, so the flag cannot
    /// survive a restart even by accident.
    pub fn to_slice(&self) -> crate::ssot::ScreenSlice {
        let source = crate::ssot::ScreenScaleSource::Remembered;
        crate::ssot::ScreenSlice {
            width: self.width,
            height: self.height,
            ui_scale: self.ui_scale,
            source,
            measured_at_ms: self.measured_at_ms,
            verified_this_session: crate::ssot::verifies_the_screen(source),
            monitor_id: self.monitor_id,
            origin: self.origin,
        }
    }

    /// Whether these numbers can describe a screen at all.
    ///
    /// Every consumer of the slice multiplies a rect by `ui_scale`, so a
    /// hand-edited `0` would not degrade an answer, it would erase one — and
    /// unlike `temple_calibration`, which its reader re-verifies against the
    /// NCC floor on every board, nothing downstream re-checks this value.
    /// Refused at load instead; see `apply_to_state`.
    ///
    /// `> 0.0` already refuses `NaN` (every comparison with it is false), but
    /// NOT an infinity: serde deserialises an `f32` through `f64` and narrows
    /// with a saturating `as` cast, so a literal serde_json accepts as f64 yet
    /// exceeding `f32::MAX` (`1e300`) arrives here as `f32::INFINITY`, which
    /// is `> 0.0`. Only `1e999`-class literals are refused by the parser. Hence
    /// the explicit `is_finite()`; the `f32::INFINITY` row of the refusal
    /// table pins it.
    fn is_sane(&self) -> bool {
        self.width > 0 && self.height > 0 && self.ui_scale.is_finite() && self.ui_scale > 0.0
    }
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
            gem_region: None,
            font_region: None,
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
            screen_scale: None,
            // Empty, not a seeded map of the shipped defaults: an unconfigured
            // widget must render at whatever the registry ships TODAY, and
            // writing today's numbers into the file would pin a user to them
            // the way an unchosen module default would (see `modules`).
            widgets: std::collections::BTreeMap::new(),
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
/// Read a persisted lab OCR rect as an OVERRIDE: a rect that is the shipped
/// 1080p literal was never a choice, so it comes back as `None`.
///
/// Pure and separate from [`load`] because it is the whole of the POE-233
/// migration — every settings file written before that change carries the
/// shipped literal in a field that had no way to say "unset", and reading those
/// back as overrides would freeze every existing user on a 1080p rect for good.
fn user_set_region(
    persisted: Option<CaptureRegion>,
    shipped: &CaptureRegion,
) -> Option<CaptureRegion> {
    match persisted {
        Some(ref rect) if rect == shipped => None,
        other => other,
    }
}

pub fn load(app: &tauri::AppHandle) -> Settings {
    let path = match settings_path(app) {
        Some(p) => p,
        None => return Settings::default(),
    };
    match fs::read_to_string(&path) {
        Ok(contents) => {
            match serde_json::from_str::<Settings>(&contents) {
                Ok(mut s) => {
                    log::info!("Settings loaded from {:?}", path);
                    s.gem_region = user_set_region(s.gem_region, &crate::SHIPPED_GEM_REGION_1080P);
                    s.font_region =
                        user_set_region(s.font_region, &crate::SHIPPED_FONT_PANEL_1080P);
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
    // Bound HERE rather than inside the struct literal below: a temporary
    // guard in a literal lives to the end of the whole statement, and this
    // mutex is one of the module-owned ones that are taken alone (see
    // modules.rs "Lock order"). `ScreenSlice` is `Copy`, so the deref copies.
    let screen = *state.screen.lock().unwrap_or_else(|e| e.into_inner());
    // Bound here rather than in the struct literal below, for the same reason
    // `screen` is: a guard created inside a struct literal lives to the end of
    // the whole statement, and this one is taken alone (modules.rs "Lock
    // order").
    let owner_modules = state.modules_enabled.lock().unwrap_or_else(|e| e.into_inner()).clone();
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
        modules: crate::modules::persistable_modules(&owner_modules, &crate::modules::module_lifecycles()),
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
        // Read here — beside the temple's calibration and for the same reason:
        // `persist_settings` rewrites the WHOLE file from this function and
        // `Settings` has no `skip_serializing_if`, so a field not read here is
        // nulled by the next save from any unrelated command. What is
        // persistable at all is `ScreenScaleSetting::from_slice`'s call.
        //
        // Unlike every other field here this projection is LOSSY — an
        // OCR-derived slice maps to `None` — so a `None` from this line means
        // "nothing to write", never "erase what is stored".
        // [`preserve_screen_scale`] is the half that says so, and
        // `persist_settings` runs it over this struct before the write.
        screen_scale: screen.as_ref().and_then(ScreenScaleSetting::from_slice),
        // Owned by an AppState mutex, so it is read HERE and not carried
        // forward by `persist_overlay_settings` — the owner is what
        // `set_widget_geometry` just wrote to, and the file is the stale copy.
        widgets: state.widgets.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }
}

/// Keep the stored screen scale when this session has nothing to replace it
/// with (POE-214 WI-B2).
///
/// [`from_state`]'s screen-scale projection is the one lossy field in it: an
/// `MercOcr` slice maps to `None` (see [`ScreenScaleSetting::from_slice`]), and
/// `crate::persist_settings` rewrites the whole file, so without this merge the
/// first OCR-only tick would null a remembered measurement on the next save
/// from ANY of the 33 unrelated commands — silently, and precisely for the
/// users whose frame fit never lands.
///
/// The label is what makes the merge correct rather than merely convenient:
/// `Remembered` says "not measured this run", so a live OCR estimate does not
/// falsify the frame measurement that produced the stored number — it just
/// fails to improve on it.
///
/// Deliberately NOT folded into [`persist_overlay_settings`]: that function's
/// contract is fields no `AppState` mutex owns, and `state.screen` owns this
/// one. A measurement DOES win — a `MercFrame` slice makes `from_slice` return
/// `Some`, so `target.screen_scale` is already filled and this leaves it alone,
/// whatever dimensions the stored value had (a frame measurement always
/// replaces — `ssot::accepts` — which is also why no separate stale-dimensions
/// prune exists; see `apply_to_state`).
///
/// A stored value that cannot describe a screen is dropped rather than carried
/// forward — the same [`ScreenScaleSetting::is_sane`] gate `apply_to_state`
/// refused it by, so a save does not write a hand-edited `0` back out after the
/// load already rejected it.
pub fn preserve_screen_scale(existing: &Settings, target: &mut Settings) {
    if target.screen_scale.is_none() {
        target.screen_scale = existing.screen_scale.filter(|stored| stored.is_sane());
    }
}

/// Write settings with the remembered screen scale explicitly EMPTIED
/// (POE-227) — the one save path [`preserve_screen_scale`] does not run on.
///
/// `crate::persist_settings` cannot express this. Its merge exists because
/// [`from_state`]'s projection is lossy, so it reads an empty projection as
/// "this session has nothing to write" and restores the stored value — which is
/// exactly the shape a deliberate drop has. A caller that cleared
/// `AppState.screen` and then called `persist_settings` would get the stale
/// value back on disk and, on the next start, back in the owner.
///
/// Everything else about the write is `persist_settings`': the whole file is
/// rebuilt from [`from_state`], and [`persist_overlay_settings`] carries the
/// window/overlay rows no `AppState` mutex owns. Only the screen-scale merge is
/// left out.
///
/// The caller is expected to have cleared the owner first — this does not clear
/// it, so that the two SSOT writes ([`crate::ssot::drop_if_mismatched`] and
/// [`crate::ssot::geometry_recalibrate`]) keep the lock-then-drop-then-write
/// shape the rest of the module uses.
pub fn persist_forgetting_screen_scale(app: &tauri::AppHandle) {
    let existing = load(app);
    let mut target = {
        let state = app.state::<crate::AppState>();
        from_state(&state)
    };
    forget_screen_scale(&existing, &mut target);
    save(app, &target);
}

/// The save-time composition a deliberate drop uses — [`preserve_screen_scale`]'s
/// opposite number, and everything about the write that is a DECISION.
///
/// Extracted from [`persist_forgetting_screen_scale`] with no `AppHandle` so
/// both halves are unit-testable: that the stored measurement is emptied rather
/// than merged back, and that emptying it does not also empty the rows no
/// `AppState` mutex owns.
fn forget_screen_scale(existing: &Settings, target: &mut Settings) {
    // Not `preserve_screen_scale`. That merge reads an empty projection as
    // "this session measured nothing" and restores the stored value — which is
    // indistinguishable, from inside the merge, from a caller that has just
    // decided to throw it away.
    target.screen_scale = None;
    // Everything `crate::persist_settings` carries forward is still carried
    // forward: dropping the geometry must not also drop the window position and
    // the five overlay rects, which `from_state` deliberately leaves `None`.
    persist_overlay_settings(existing, target);
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
            game_monitor: Mutex::new(None),
            pair_code: Mutex::new(String::new()),
            client_txt_path: Mutex::new(String::new()),
            server_url: Mutex::new(String::new()),
            detected_gems: Mutex::new(Vec::new()),
            lab_state: Mutex::new(crate::lab_state::LabState::Idle),
            logs: Mutex::new(Vec::new()),
            gem_region: Mutex::new(None),
            font_region: Mutex::new(None),
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
            merc_icons_write: Mutex::new(()),
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
            merc_refit: AtomicU64::new(0),
            screen: Mutex::new(None),
            widgets: Mutex::new(std::collections::BTreeMap::new()),
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

    /// A frame measurement fixed at a known instant, so a round trip has real
    /// numbers to lose. 1920x1200 at 1.0 is the reference fixture's own screen
    /// (see `ssot::ScreenSlice`'s unit note).
    fn measured_screen(source: crate::ssot::ScreenScaleSource) -> crate::ssot::ScreenSlice {
        crate::ssot::ScreenSlice {
            width: 1920,
            height: 1080,
            ui_scale: 0.9,
            source,
            // A real display, so a round trip that dropped the identity is
            // visible as a `0` rather than as the same `0` it started with.
            monitor_id: 65_537,
            origin: (-1920, 0),
            // As the writers build one — never a literal, so the round-trip
            // tests below run against the flag the app actually publishes.
            verified_this_session: crate::ssot::verifies_the_screen(source),
            measured_at_ms: 1_724_000_000_000,
        }
    }

    /// The five-touch-point cycle for the screen scale: a frame measurement
    /// reaches settings.json through `from_state` and comes back through
    /// `apply_to_state` LABELLED as remembered, with every number intact.
    /// Fails if either touch point is dropped — the next session would then
    /// have no idea what this screen's UI scale is until a recruit window
    /// happened to open, which is the whole point of persisting it.
    #[test]
    fn a_frame_measured_screen_scale_round_trips_through_state_as_remembered() {
        let state = test_app_state();
        *state.screen.lock().unwrap() =
            Some(measured_screen(crate::ssot::ScreenScaleSource::MercFrame));

        let saved = from_state(&state);
        let stored = saved
            .screen_scale
            .expect("a frame measurement must reach settings.json");
        assert_eq!((stored.width, stored.height), (1920, 1080));
        assert_eq!(stored.ui_scale, 0.9);
        assert_eq!(stored.measured_at_ms, 1_724_000_000_000);

        // Next launch: a fresh state loads that file.
        let reloaded = test_app_state();
        let _ = apply_to_state(&saved, &reloaded);

        let loaded = reloaded
            .screen
            .lock()
            .unwrap()
            .expect("the load must fill the slice");
        assert_eq!((loaded.width, loaded.height), (1920, 1080));
        assert_eq!(loaded.ui_scale, 0.9);
        assert_eq!(
            loaded.measured_at_ms, 1_724_000_000_000,
            "the age must survive with the number it belongs to",
        );
        assert_eq!(
            loaded.source,
            crate::ssot::ScreenScaleSource::Remembered,
            "a loaded scale was not measured this run and must say so",
        );
    }

    /// POE-237's half of the same cycle: WHICH display the number was measured
    /// on has to survive the restart, or the lazy prune has nothing to compare
    /// against and a player with two 1920x1080 monitors loads a scale measured
    /// on the other one with no way for anything to notice. Fails if
    /// `from_slice` or `to_slice` drops either field, or if the struct never
    /// grew them.
    #[test]
    fn the_display_a_scale_was_measured_on_round_trips_through_settings() {
        let state = test_app_state();
        *state.screen.lock().unwrap() =
            Some(measured_screen(crate::ssot::ScreenScaleSource::MercFrame));

        let saved = from_state(&state);
        let stored = saved.screen_scale.expect("a frame measurement must reach settings.json");
        assert_eq!(stored.monitor_id, 65_537);
        assert_eq!(stored.origin, (-1920, 0));

        let reloaded = test_app_state();
        let _ = apply_to_state(&saved, &reloaded);

        let loaded = reloaded.screen.lock().unwrap().expect("the load must fill the slice");
        assert_eq!(
            loaded.monitor_id, 65_537,
            "a remembered scale that cannot say which display it came off cannot be pruned",
        );
        assert_eq!(loaded.origin, (-1920, 0));
    }

    /// Every settings.json written before POE-237 stored a scale with no
    /// display. It must load as UNKNOWN — the `0` `ssot::different_monitor`
    /// declines to answer on — rather than failing the file or defaulting to a
    /// plausible id, which would prune every remembered scale on the first
    /// capture after the upgrade.
    #[test]
    fn a_screen_scale_written_before_the_display_was_recorded_loads_as_unknown() {
        let parsed: Settings = serde_json::from_str(
            r#"{"screen_scale":{"width":1920,"height":1080,"ui_scale":0.9,"measured_at_ms":1724000000000}}"#,
        )
        .expect("a stored scale without the display must still parse");

        let stored = parsed.screen_scale.expect("the scale itself must load");
        assert_eq!((stored.width, stored.height), (1920, 1080), "the measurement is untouched");
        assert_eq!(stored.monitor_id, 0, "0 is what the prune reads as 'no opinion'");
        assert_eq!(stored.origin, (0, 0));
    }

    /// The one number a restart must NOT carry (POE-240). A frame measurement
    /// that verified the screen last session says nothing about the screen this
    /// one is drawn on — the monitor may have changed, or the game's UI scale —
    /// so the load comes back unverified and the Settings card says "trusted
    /// from last session" until a gold frame confirms it again. Fails if
    /// `ScreenScaleSetting` grows the field, or if `to_slice` hard-codes `true`.
    #[test]
    fn a_verified_screen_scale_comes_back_unverified_after_a_restart() {
        let state = test_app_state();
        let verified = measured_screen(crate::ssot::ScreenScaleSource::MercFrame);
        assert!(
            verified.verified_this_session,
            "the frame measurement this test saves must be a verified one",
        );
        *state.screen.lock().unwrap() = Some(verified);

        let saved = from_state(&state);
        let reloaded = test_app_state();
        let _ = apply_to_state(&saved, &reloaded);

        let loaded = reloaded
            .screen
            .lock()
            .unwrap()
            .expect("the load must fill the slice");
        assert!(
            !loaded.verified_this_session,
            "a restart has looked at nothing and must not claim it verified the screen",
        );
    }

    /// The OCR-derived scale is never written. It sits 6-12 px off the frame
    /// (POE-214's diagnosis), and next session would read it back under the
    /// same `remembered` label a real measurement gets, with nothing to tell
    /// the two apart.
    #[test]
    fn an_ocr_measured_screen_scale_is_not_persisted() {
        let state = test_app_state();
        *state.screen.lock().unwrap() =
            Some(measured_screen(crate::ssot::ScreenScaleSource::MercOcr));

        assert_eq!(
            from_state(&state).screen_scale,
            None,
            "the drifting cue must not reach settings.json",
        );
    }

    /// A session that loaded a scale and never measured one writes back what
    /// it loaded. Without this the first save from any command nulls the field
    /// (the whole file is rewritten from `from_state`) and the remembered
    /// value is gone for good — the worst case being a user whose merc module
    /// is off.
    #[test]
    fn a_remembered_screen_scale_is_written_back_unchanged() {
        let state = test_app_state();
        *state.screen.lock().unwrap() =
            Some(measured_screen(crate::ssot::ScreenScaleSource::Remembered));

        let stored = from_state(&state)
            .screen_scale
            .expect("a loaded scale must survive a session that never measured");

        assert_eq!((stored.width, stored.height), (1920, 1080));
        assert_eq!(stored.ui_scale, 0.9);
        assert_eq!(stored.measured_at_ms, 1_724_000_000_000);
    }

    /// A save triggered by some OTHER command must not lose the remembered
    /// scale — the `test_overlay_settings_survive_persist_cycle` hazard, one
    /// door along: `persist_settings` rewrites the whole file from
    /// `from_state`, and `Settings` has no `skip_serializing_if`, so a field
    /// that function does not read is nulled by the next unrelated save.
    #[test]
    fn a_save_from_another_command_keeps_the_remembered_screen_scale() {
        let state = test_app_state();
        let loaded = Settings {
            screen_scale: Some(ScreenScaleSetting {
                width: 1920,
                height: 1200,
                ui_scale: 1.0,
                measured_at_ms: 1_724_000_000_000,
                // A file written before POE-237: the display is unknown, which is
                // what `#[serde(default)]` fills in and what the prune falls back
                // to comparing dimensions alone for.
                monitor_id: 0,
                origin: (0, 0),
            }),
            ..Settings::default()
        };
        let _ = apply_to_state(&loaded, &state);

        // Another command changes an unrelated preference and persists.
        *state.sidebar_open.lock().unwrap() = false;
        let saved = from_state(&state);

        assert!(!saved.sidebar_open, "precondition: the unrelated change is what is being saved");
        assert_eq!(
            saved.screen_scale, loaded.screen_scale,
            "the remembered scale must survive an unrelated save",
        );
    }

    /// The on-disk key and its inner keys. The file is hand-editable and
    /// forward-compatible by convention; renaming either half silently drops
    /// every user's remembered scale on the next launch.
    #[test]
    fn the_remembered_scale_is_stored_under_snake_case_keys() {
        let settings = Settings {
            screen_scale: Some(ScreenScaleSetting {
                width: 1920,
                height: 1080,
                ui_scale: 0.9,
                measured_at_ms: 1_724_000_000_000,
                // A file written before POE-237: the display is unknown, which is
                // what `#[serde(default)]` fills in and what the prune falls back
                // to comparing dimensions alone for.
                monitor_id: 0,
                origin: (0, 0),
            }),
            ..Settings::default()
        };

        let json = serde_json::to_value(&settings).expect("settings must serialize");

        assert_eq!(json["screen_scale"]["width"], 1920);
        assert_eq!(json["screen_scale"]["height"], 1080);
        assert_eq!(json["screen_scale"]["measured_at_ms"], 1_724_000_000_000u64);
        assert_eq!(
            json["screen_scale"]["ui_scale"].as_f64().expect("ui_scale must be a number") as f32,
            0.9_f32,
        );
    }

    /// The measured scale is an `f32` that goes to disk through `f64` JSON and
    /// comes back. It has to return BIT-EQUAL: a consumer multiplies a rect by
    /// it, and this test is what says a widened or truncated field would be
    /// caught rather than showing up as a few px of drift next session.
    /// 0.8985 is a real fit value, not a round one.
    #[test]
    fn a_measured_ui_scale_survives_the_json_text_round_trip_exactly() {
        let settings = Settings {
            screen_scale: Some(ScreenScaleSetting {
                width: 2560,
                height: 1440,
                ui_scale: 0.8985_f32,
                measured_at_ms: 1_724_000_000_123,
                // A file written before POE-237: the display is unknown, which is
                // what `#[serde(default)]` fills in and what the prune falls back
                // to comparing dimensions alone for.
                monitor_id: 0,
                origin: (0, 0),
            }),
            ..Settings::default()
        };

        let text = serde_json::to_string(&settings).expect("settings must serialize");
        let parsed: Settings = serde_json::from_str(&text).expect("its own output must parse");

        let stored = parsed.screen_scale.expect("the scale must survive the file");
        assert_eq!((stored.width, stored.height), (2560, 1440));
        assert_eq!(stored.ui_scale, 0.8985_f32, "the fit must come back bit-equal");
        assert_eq!(stored.measured_at_ms, 1_724_000_000_123);
    }

    /// The shape a save from an OCR-only session used to write, and the one a
    /// hand-edit writes to forget a screen: an explicit `null` is a present key
    /// with no value, which the container-level `#[serde(default)]` does not
    /// cover. It must load as unmeasured rather than fail the whole file.
    #[test]
    fn an_explicit_null_screen_scale_loads_as_unmeasured() {
        let parsed: Settings =
            serde_json::from_str(r#"{"server_url":"https://kept.example","screen_scale":null}"#)
                .expect("an explicit null must still parse");

        assert_eq!(parsed.server_url, "https://kept.example");
        assert_eq!(parsed.screen_scale, None);
    }

    /// A settings.json written before POE-214 carries no `screen_scale`. It
    /// must load with the field absent, not fail the whole file (which would
    /// reset every unrelated preference in it). The attribute that holds here
    /// is the CONTAINER-level `#[serde(default)]` on `Settings` — it fills
    /// every missing field from `Settings::default()`, so this passes with or
    /// without the field-level one.
    #[test]
    fn a_settings_file_without_a_screen_scale_loads_with_none() {
        let parsed: Settings = serde_json::from_str(r#"{"server_url":"https://kept.example"}"#)
            .expect("an older file must still parse");
        assert_eq!(parsed.server_url, "https://kept.example");
        assert_eq!(parsed.screen_scale, None);

        let state = test_app_state();
        let _ = apply_to_state(&parsed, &state);

        assert!(
            state.screen.lock().unwrap().is_none(),
            "no stored scale means the slice stays unmeasured, not 1.0",
        );
    }

    /// A hand-edited scale that cannot describe a screen is refused at load and
    /// reported — one row per clause of `is_sane`, because a gate that dropped
    /// any single clause would still pass the others. Every consumer multiplies
    /// a rect by `ui_scale`, so loading a 0 would erase the answer rather than
    /// degrade it, and nothing downstream re-verifies this value the way the
    /// temple's reader re-verifies its own calibration.
    #[test]
    fn a_screen_scale_that_cannot_describe_a_screen_is_refused_and_reported() {
        for (width, height, ui_scale, what) in [
            (0_u32, 1080_u32, 0.9_f32, "a zero width"),
            (1920, 0, 0.9, "a zero height"),
            (1920, 1080, 0.0, "a zero ui_scale"),
            (1920, 1080, -1.0, "a negative ui_scale"),
            (1920, 1080, f32::NAN, "a NaN ui_scale"),
            (1920, 1080, f32::INFINITY, "an infinite ui_scale"),
        ] {
            let settings = Settings {
                screen_scale: Some(ScreenScaleSetting {
                    width,
                    height,
                    ui_scale,
                    measured_at_ms: 1_724_000_000_000,
                    // A file written before POE-237: the display is unknown, which is
                    // what `#[serde(default)]` fills in and what the prune falls back
                    // to comparing dimensions alone for.
                    monitor_id: 0,
                    origin: (0, 0),
                }),
                ..Settings::default()
            };
            let state = test_app_state();

            let rejected = apply_to_state(&settings, &state);

            assert!(
                state.screen.lock().unwrap().is_none(),
                "{what} must not reach the slice",
            );
            let line = rejected
                .iter()
                .find(|line| line.contains("screen scale"))
                .unwrap_or_else(|| panic!("{what} must be reported, not silently dropped"));
            assert!(
                line.contains(&format!("{width}x{height}")),
                "the line must name the refused value, got {line:?}",
            );
        }
    }

    /// The data-loss guard, and the case that motivates it: a session whose
    /// frame fit never lands still ends up with an `MercOcr` slice, which
    /// `from_state` projects to `None`. Any of the 33 unrelated
    /// `persist_settings` call sites then rewrites the whole file, so without
    /// the merge the remembered measurement is gone — silently, and for exactly
    /// the users who most need it.
    #[test]
    fn preserve_screen_scale_keeps_the_stored_measurement_when_this_session_only_ocr_fitted() {
        let state = test_app_state();
        *state.screen.lock().unwrap() =
            Some(measured_screen(crate::ssot::ScreenScaleSource::MercOcr));
        let existing = Settings {
            screen_scale: Some(ScreenScaleSetting {
                width: 1920,
                height: 1200,
                ui_scale: 1.0,
                measured_at_ms: 1_724_000_000_000,
                // A file written before POE-237: the display is unknown, which is
                // what `#[serde(default)]` fills in and what the prune falls back
                // to comparing dimensions alone for.
                monitor_id: 0,
                origin: (0, 0),
            }),
            ..Settings::default()
        };

        let mut about_to_save = from_state(&state);
        assert_eq!(
            about_to_save.screen_scale, None,
            "precondition: the OCR estimate is not persistable",
        );

        preserve_screen_scale(&existing, &mut about_to_save);

        assert_eq!(
            about_to_save.screen_scale, existing.screen_scale,
            "an unmeasurable session must write back what it loaded",
        );
    }

    /// The other half: the merge is a floor, not a freeze. A frame measurement
    /// this session fills `from_state`'s field, so the stored value must NOT be
    /// merged over it — a frame measurement always replaces (`ssot::accepts`),
    /// whatever dimensions it measured.
    #[test]
    fn preserve_screen_scale_lets_a_fresh_measurement_replace_the_stored_one() {
        let state = test_app_state();
        *state.screen.lock().unwrap() = Some(crate::ssot::ScreenSlice {
            width: 2560,
            height: 1440,
            ui_scale: 1.25,
            source: crate::ssot::ScreenScaleSource::MercFrame,
            measured_at_ms: 1_724_000_600_000,
            monitor_id: 65_537,
            origin: (0, 0),
            verified_this_session: crate::ssot::verifies_the_screen(
                crate::ssot::ScreenScaleSource::MercFrame,
            ),
        });
        let existing = Settings {
            screen_scale: Some(ScreenScaleSetting {
                width: 1920,
                height: 1200,
                ui_scale: 1.0,
                measured_at_ms: 1_724_000_000_000,
                // A file written before POE-237: the display is unknown, which is
                // what `#[serde(default)]` fills in and what the prune falls back
                // to comparing dimensions alone for.
                monitor_id: 0,
                origin: (0, 0),
            }),
            ..Settings::default()
        };

        let mut about_to_save = from_state(&state);
        preserve_screen_scale(&existing, &mut about_to_save);

        let kept = about_to_save.screen_scale.expect("the measurement must be there");
        assert_eq!((kept.width, kept.height), (2560, 1440));
        assert_eq!(kept.ui_scale, 1.25);
        assert_eq!(kept.measured_at_ms, 1_724_000_600_000);
    }

    /// The drop path's whole point (POE-227): a save that is FORGETTING the
    /// geometry writes an empty field, even though `from_state` produces the
    /// same empty field a session with nothing to say produces.
    ///
    /// Fails the moment this composition is replaced by `crate::persist_settings`
    /// (or grows a `preserve_screen_scale` call), which is the one mistake that
    /// makes Recalibrate and the stale-monitor prune both silently no-ops: the
    /// owner would be cleared, the file would keep the stale value, and the
    /// next start would load it straight back.
    #[test]
    fn forget_screen_scale_empties_a_stored_measurement_instead_of_merging_it_back() {
        let state = test_app_state();
        // The post-drop owner: cleared by `ssot::drop_if_mismatched` /
        // `ssot::geometry_recalibrate` before the write.
        *state.screen.lock().unwrap() = None;
        let existing = Settings {
            screen_scale: Some(ScreenScaleSetting {
                width: 1920,
                height: 1200,
                ui_scale: 1.0,
                measured_at_ms: 1_724_000_000_000,
                // A file written before POE-237: the display is unknown, which is
                // what `#[serde(default)]` fills in and what the prune falls back
                // to comparing dimensions alone for.
                monitor_id: 0,
                origin: (0, 0),
            }),
            ..Settings::default()
        };
        let mut about_to_save = from_state(&state);

        forget_screen_scale(&existing, &mut about_to_save);

        assert_eq!(
            about_to_save.screen_scale, None,
            "a deliberate drop must reach the file, not be merged away",
        );
    }

    /// The other half: forgetting the geometry must not forget the geometry of
    /// our own WINDOWS. `from_state` leaves the window row and the five overlay
    /// rects `None` on purpose (their own save commands own them), so a drop
    /// path that skipped the carry-forward would wipe every overlay position
    /// the first time a monitor changed or Recalibrate was pressed.
    #[test]
    fn forget_screen_scale_keeps_the_window_and_overlay_rows() {
        let state = test_app_state();
        *state.screen.lock().unwrap() = None;
        let existing = Settings {
            window: Some(WindowSettings { x: 40, y: 60, width: 1280, height: 800, maximized: false }),
            comparator_overlay: Some(OverlaySettings {
                x: 1500,
                y: 200,
                width: 630,
                height: 250,
                enabled: true,
            }),
            ..Settings::default()
        };
        let mut about_to_save = from_state(&state);

        forget_screen_scale(&existing, &mut about_to_save);

        let window = about_to_save.window.expect("the window row must survive the drop");
        assert_eq!((window.x, window.y, window.width, window.height), (40, 60, 1280, 800));
        let comparator = about_to_save
            .comparator_overlay
            .expect("the comparator's rect must survive the drop");
        assert_eq!((comparator.x, comparator.y), (1500, 200));
    }

    /// The third half, and the reason Recalibrate is ONE write (POE-227): the
    /// temple's calibration is cleared in its OWNER
    /// (`temple::run::clear_calibration`) and reaches the file through
    /// `from_state`'s projection, so the save-time composition must not carry
    /// the stored one forward the way it carries the window rows.
    ///
    /// Fails if `temple_calibration` is ever added to `persist_overlay_settings`
    /// — which would leave Recalibrate clearing the owner while settings.json
    /// kept the hint, and the next start loading it straight back.
    #[test]
    fn forget_screen_scale_does_not_carry_a_cleared_temple_calibration_back() {
        let state = test_app_state();
        *state.screen.lock().unwrap() = None;
        // What `clear_calibration` leaves behind: the owner's hint is gone, the
        // rest of the temple's settings are untouched.
        state.temple_settings.lock().unwrap().calibration = None;
        let existing = Settings {
            temple_calibration: Some(crate::temple::anchor::AnchorCalibration {
                screen_w: 2560,
                screen_h: 1440,
                scale: 0.99,
            }),
            ..Settings::default()
        };
        let mut about_to_save = from_state(&state);

        forget_screen_scale(&existing, &mut about_to_save);

        assert_eq!(
            about_to_save.temple_calibration, None,
            "the owner's clear is what reaches the file — one write, not two",
        );
    }

    /// A placement written by `set_widget_geometry` has to be there on the next
    /// start, so the whole chain is exercised: owner → [`from_state`] → the
    /// JSON text that actually reaches the disk → [`apply_to_state`] → owner.
    ///
    /// Every field is asserted separately because they are separately losable:
    /// the `x`/`y` pair is `i32` and the size pair is `u32`, so a field crossed
    /// in either projection puts the widget somewhere plausible rather than
    /// somewhere obviously wrong. Deliberately asymmetric numbers for the same
    /// reason — the host pair included, which is a THIRD `u32` pair to cross
    /// (POE-239) and the one whose loss is silent: a placement that comes back
    /// with an unknown host is never rebased, so a 4K rectangle simply pins to
    /// the edge of a 1080p monitor exactly as it did before the field existed.
    #[test]
    fn a_widget_placement_round_trips_through_state_and_the_file() {
        let saved_from = test_app_state();
        saved_from.widgets.lock().unwrap().insert(
            "temple.advice".to_string(),
            WidgetGeometry {
                x: 250,
                y: 41,
                width: 402,
                height: 203,
                visible: true,
                host_width: 3840,
                host_height: 2160,
            },
        );

        let text = serde_json::to_string(&from_state(&saved_from)).expect("must serialize");
        let parsed: Settings = serde_json::from_str(&text).expect("its own output must parse");
        let loaded_into = test_app_state();
        let _ = apply_to_state(&parsed, &loaded_into);

        let widgets = loaded_into.widgets.lock().unwrap();
        let placed = widgets
            .get("temple.advice")
            .copied()
            .expect("the placement must survive the file");
        assert_eq!(placed.x, 250);
        assert_eq!(placed.y, 41);
        assert_eq!(placed.width, 402);
        assert_eq!(placed.height, 203);
        assert!(placed.visible, "a shown widget must not come back hidden");
        assert_eq!(placed.host_width, 3840);
        assert_eq!(placed.host_height, 2160);
    }

    /// Every settings.json written before POE-239 has widget rows with no host
    /// size. They must load as UNKNOWN — the zero `rebase` refuses to scale
    /// against — rather than failing the file or, worse, defaulting to some
    /// plausible monitor and rebasing every existing placement on the next
    /// start.
    #[test]
    fn a_widget_row_written_before_the_host_size_existed_loads_with_an_unknown_host() {
        let parsed: Settings = serde_json::from_str(
            r#"{"widgets":{"temple.board":{"x":40,"y":60,"width":0,"height":0,"visible":true}}}"#,
        )
        .expect("a row without the host pair must still parse");

        let row = parsed.widgets.get("temple.board").copied().expect("the row must load");
        assert_eq!((row.x, row.y), (40, 60), "the placement itself is untouched");
        assert_eq!(
            (row.host_width, row.host_height),
            (0, 0),
            "0 is what the frontend reads as 'never rebase this row'",
        );
    }

    /// The Show checkbox is the half that is easy to lose: `false` is also
    /// `bool`'s default, so a field dropped from the projection reads as
    /// "hidden" on the way out and the widget silently reappears on the way
    /// back in.
    #[test]
    fn a_hidden_widget_comes_back_hidden() {
        let saved_from = test_app_state();
        saved_from.widgets.lock().unwrap().insert(
            "temple.board".to_string(),
            WidgetGeometry {
                x: 40,
                y: 40,
                width: 200,
                height: 200,
                visible: false,
                host_width: 0,
                host_height: 0,
            },
        );

        let text = serde_json::to_string(&from_state(&saved_from)).expect("must serialize");
        let parsed: Settings = serde_json::from_str(&text).expect("its own output must parse");
        let loaded_into = test_app_state();
        let _ = apply_to_state(&parsed, &loaded_into);

        assert_eq!(
            loaded_into.widgets.lock().unwrap().get("temple.board").map(|g| g.visible),
            Some(false),
            "the user's Show choice is the state, not a runtime flag",
        );
    }

    /// Every settings.json written before POE-225 has no `widgets` key. It must
    /// load as "nothing has been placed" — the `#[serde(default)]` — rather
    /// than failing the whole file and resetting every other preference in it.
    #[test]
    fn a_settings_file_without_widgets_loads_with_no_placements() {
        let parsed: Settings =
            serde_json::from_str(r#"{"server_url":"https://kept.example"}"#).expect("must parse");

        assert_eq!(parsed.server_url, "https://kept.example");
        assert!(parsed.widgets.is_empty());
    }

    /// The widget map is OWNED by an `AppState` mutex, so the save-time
    /// composition must let [`from_state`]'s copy stand — exactly like
    /// `temple_calibration` above and unlike the window and overlay rows.
    ///
    /// Fails if `widgets` is ever added to [`persist_overlay_settings`], which
    /// would carry the file's stale placement back over the one
    /// `set_widget_geometry` just wrote and make every drag revert on the next
    /// save from any unrelated command.
    #[test]
    fn forget_screen_scale_does_not_carry_a_stale_widget_placement_back() {
        let state = test_app_state();
        *state.screen.lock().unwrap() = None;
        state.widgets.lock().unwrap().insert(
            "temple.board".to_string(),
            WidgetGeometry {
                x: 900,
                y: 120,
                width: 200,
                height: 200,
                visible: true,
                host_width: 0,
                host_height: 0,
            },
        );
        // What the file still says — the placement before the user dragged it.
        let existing = Settings {
            widgets: [(
                "temple.board".to_string(),
                WidgetGeometry {
                    x: 40,
                    y: 40,
                    width: 200,
                    height: 200,
                    visible: true,
                    host_width: 0,
                    host_height: 0,
                },
            )]
            .into_iter()
            .collect(),
            ..Settings::default()
        };
        let mut about_to_save = from_state(&state);

        forget_screen_scale(&existing, &mut about_to_save);

        assert_eq!(
            about_to_save.widgets.get("temple.board").map(|g| (g.x, g.y)),
            Some((900, 120)),
            "the owner's placement is what reaches the file, not the file's own",
        );
    }

    #[test]
    fn widgets_for_module_returns_only_that_module_s_placements() {
        let placed = WidgetGeometry {
            x: 1,
            y: 2,
            width: 3,
            height: 4,
            visible: true,
            host_width: 0,
            host_height: 0,
        };
        let widgets: std::collections::BTreeMap<String, WidgetGeometry> = [
            ("temple.advice".to_string(), placed),
            ("temple.board".to_string(), placed),
            ("mercenary.strip".to_string(), placed),
        ]
        .into_iter()
        .collect();

        let ids: Vec<String> = widgets_for_module(&widgets, "temple")
            .into_iter()
            .map(|entry| entry.id)
            .collect();

        assert_eq!(ids, vec!["temple.advice".to_string(), "temple.board".to_string()]);
    }

    /// The separator is part of the prefix. Without it a module named `temple`
    /// would collect a future `temple2`'s widgets and place them in the wrong
    /// window — the class of bug a bare `starts_with(module)` always has.
    #[test]
    fn widgets_for_module_does_not_collect_a_module_whose_name_merely_starts_the_same() {
        let widgets: std::collections::BTreeMap<String, WidgetGeometry> = [(
            "temple2.board".to_string(),
            WidgetGeometry {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
                visible: true,
                host_width: 0,
                host_height: 0,
            },
        )]
        .into_iter()
        .collect();

        assert_eq!(widgets_for_module(&widgets, "temple"), vec![]);
    }

    /// A widget that has never been configured has no row, so the answer for a
    /// module nobody has placed yet is empty — not a seeded default. The
    /// frontend registry owns the shipped placement.
    #[test]
    fn widgets_for_module_answers_nothing_for_a_module_with_no_placements() {
        assert_eq!(widgets_for_module(&Default::default(), "temple"), vec![]);
    }

    /// The merge must not resurrect what the load refused. `apply_to_state`
    /// drops a hand-edited value that cannot describe a screen, which leaves
    /// the slice empty — and an empty slice is exactly the condition the merge
    /// fires on, so an ungated merge would write the bad value straight back
    /// out and make the refusal permanent noise instead of a one-off.
    #[test]
    fn preserve_screen_scale_drops_a_stored_value_that_cannot_describe_a_screen() {
        let existing = Settings {
            screen_scale: Some(ScreenScaleSetting {
                width: 1920,
                height: 1080,
                ui_scale: 0.0,
                measured_at_ms: 1_724_000_000_000,
                // A file written before POE-237: the display is unknown, which is
                // what `#[serde(default)]` fills in and what the prune falls back
                // to comparing dimensions alone for.
                monitor_id: 0,
                origin: (0, 0),
            }),
            ..Settings::default()
        };
        let mut about_to_save = Settings::default();

        preserve_screen_scale(&existing, &mut about_to_save);

        assert_eq!(
            about_to_save.screen_scale, None,
            "the load already refused this value; a save must not write it back",
        );
    }

    /// Why WI-B2 needs no stale-dimensions prune: `ssot::accepts` takes every
    /// frame fit, so the first one of the real screen replaces a value
    /// remembered at another monitor's size outright, and the same tick is
    /// worth a write. A prune at load would only be racing this.
    #[test]
    fn a_frame_measurement_of_a_different_screen_replaces_the_remembered_dimensions() {
        let state = test_app_state();
        let loaded = Settings {
            screen_scale: Some(ScreenScaleSetting {
                width: 1920,
                height: 1200,
                ui_scale: 1.0,
                measured_at_ms: 1_724_000_000_000,
                // A file written before POE-237: the display is unknown, which is
                // what `#[serde(default)]` fills in and what the prune falls back
                // to comparing dimensions alone for.
                monitor_id: 0,
                origin: (0, 0),
            }),
            ..Settings::default()
        };
        let _ = apply_to_state(&loaded, &state);

        let measured = crate::ssot::ScreenSlice {
            width: 2560,
            height: 1440,
            ui_scale: 1.25,
            source: crate::ssot::ScreenScaleSource::MercFrame,
            measured_at_ms: 1_724_000_600_000,
            monitor_id: 65_537,
            origin: (0, 0),
            verified_this_session: crate::ssot::verifies_the_screen(
                crate::ssot::ScreenScaleSource::MercFrame,
            ),
        };
        let record = {
            let mut slot = state.screen.lock().unwrap();
            crate::ssot::record_screen(&mut slot, measured)
        };

        let stored = from_state(&state)
            .screen_scale
            .expect("the new measurement must be persistable");
        assert_eq!((stored.width, stored.height), (2560, 1440));
        assert_eq!(stored.ui_scale, 1.25);
        assert_eq!(stored.measured_at_ms, 1_724_000_600_000);
        assert!(
            crate::ssot::should_remember_screen(record.changed, measured.source),
            "a screen this different from the remembered one is worth a write",
        );
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

    // --- the lab OCR regions (POE-233) ---------------------------------------
    //
    // These pin the MIGRATION, which is the half of POE-233 that can silently
    // hurt existing users: the resolution arithmetic itself is pinned beside
    // `effective_region` in lib.rs.

    /// Every settings.json written before POE-233 carries the shipped 1080p
    /// rect in a field that had no way to say "the user never set one". Read
    /// back as an override it would pin every existing user to a 1080p rect
    /// forever — on a 1440p screen, permanently 25% too small — and no UI would
    /// say why, because the row would look identical to a deliberate choice.
    #[test]
    fn a_persisted_rect_equal_to_the_shipped_default_loads_as_unset() {
        let loaded = user_set_region(
            Some(crate::SHIPPED_GEM_REGION_1080P),
            &crate::SHIPPED_GEM_REGION_1080P,
        );

        assert_eq!(loaded, None, "an untouched region must not survive as an override");
    }

    /// The other side of the same rule, and the one that makes it safe: a rect
    /// the user actually placed is not the shipped literal, so it is kept.
    #[test]
    fn a_persisted_rect_the_user_placed_loads_as_an_override() {
        let placed = CaptureRegion { x: 120, y: 64, w: 700, h: 90 };

        let loaded = user_set_region(Some(placed.clone()), &crate::SHIPPED_GEM_REGION_1080P);

        assert_eq!(loaded, Some(placed));
    }

    /// A file written AFTER POE-233 by a user who never placed a region has no
    /// rect at all (serde fills the field from `Settings::default`). It must
    /// stay unset rather than acquiring the shipped literal on the way in,
    /// which would re-create the very override this migration removes.
    #[test]
    fn an_absent_rect_stays_unset() {
        assert_eq!(user_set_region(None, &crate::SHIPPED_FONT_PANEL_1080P), None);
    }

    /// The whole chain for a placed region: owner → [`from_state`] → the JSON
    /// text that reaches the disk → [`apply_to_state`] → owner. Asymmetric
    /// numbers on all four fields because a crossed pair (`x`/`y` are `i32`,
    /// `w`/`h` are `u32`) would put the crop somewhere plausible instead of
    /// somewhere obviously wrong.
    #[test]
    fn a_placed_gem_region_round_trips_through_state_and_the_file() {
        let saved_from = test_app_state();
        *saved_from.gem_region.lock().unwrap() =
            Some(CaptureRegion { x: 118, y: 64, w: 702, h: 91 });

        let text = serde_json::to_string(&from_state(&saved_from)).expect("must serialize");
        let parsed: Settings = serde_json::from_str(&text).expect("its own output must parse");
        let loaded_into = test_app_state();
        let _ = apply_to_state(&parsed, &loaded_into);

        assert_eq!(
            *loaded_into.gem_region.lock().unwrap(),
            Some(CaptureRegion { x: 118, y: 64, w: 702, h: 91 }),
        );
    }

    /// The unset case must survive the file as unset. It is the one that
    /// regresses invisibly: a projection that wrote the RESOLVED rect instead
    /// of the override would turn "follows the screen" into a frozen override
    /// on the next save, for every user, without anyone touching a setting.
    #[test]
    fn an_unset_font_region_round_trips_through_state_and_the_file_as_unset() {
        let saved_from = test_app_state();
        *saved_from.font_region.lock().unwrap() = None;

        let text = serde_json::to_string(&from_state(&saved_from)).expect("must serialize");
        let parsed: Settings = serde_json::from_str(&text).expect("its own output must parse");
        let loaded_into = test_app_state();
        *loaded_into.font_region.lock().unwrap() =
            Some(CaptureRegion { x: 1, y: 2, w: 3, h: 4 });
        let _ = apply_to_state(&parsed, &loaded_into);

        assert_eq!(*loaded_into.font_region.lock().unwrap(), None);
    }

    /// The rollback guard. Written as `"gem_region": null`, this field is
    /// rejected by the pre-POE-233 build whose `gem_region` was non-optional:
    /// its `load` fails the whole parse, falls back to `Settings::default()`,
    /// and the next persist overwrites the file — every stored setting gone
    /// because one field said `null`. The beta channel makes that rollback a
    /// real path, so the unset region must leave NO key behind.
    #[test]
    fn an_unset_gem_region_writes_no_key_at_all() {
        let text = serde_json::to_string(&Settings { gem_region: None, ..Settings::default() })
            .expect("must serialize");

        assert!(!text.contains("gem_region"), "an unset region must be absent, not null: {text}");
    }

    /// Same guard for the font panel, whose field carries the same attribute
    /// and the same rollback consequence.
    #[test]
    fn an_unset_font_region_writes_no_key_at_all() {
        let text = serde_json::to_string(&Settings { font_region: None, ..Settings::default() })
            .expect("must serialize");

        assert!(!text.contains("font_region"), "an unset region must be absent, not null: {text}");
    }

    /// The other side of the skip: a region the user placed still reaches the
    /// file, with its values. A `skip_serializing` (no `_if`) would satisfy the
    /// two tests above while silently dropping every user's override.
    #[test]
    fn a_placed_gem_region_still_writes_its_key_and_values() {
        let text = serde_json::to_string(&Settings {
            gem_region: Some(CaptureRegion { x: 118, y: 64, w: 702, h: 91 }),
            ..Settings::default()
        })
        .expect("must serialize");

        assert!(
            text.contains(r#""gem_region":{"x":118,"y":64,"w":702,"h":91}"#),
            "a placed region must reach the file verbatim: {text}",
        );
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
    // Widget placements, straight through: unlike `modules` there is no
    // registry default to overlay here — a widget with no entry renders at the
    // shipped CSS default the frontend registry holds, and an entry for a
    // widget this build does not declare is simply never looked up.
    *state.widgets.lock().unwrap_or_else(|e| e.into_inner()) = settings.widgets.clone();
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

    // The remembered screen scale (POE-214 D2), loaded under `Remembered` so a
    // consumer can see it was not measured this run. Numbers that cannot
    // describe a screen are refused rather than loaded — and REPORTED, because
    // the file and the running value now disagree (see `is_sane`).
    //
    // **No prune here, and this is the deliberate half of it.** A value stored
    // at another monitor's size is left in place: `apply_to_state` has no
    // `AppHandle`, so the primary monitor's size is not cheaply known at this
    // seam, and inventing a lock order to go and get one would cost more than
    // the staleness does. The merc tick is the cure — `ssot::accepts` takes
    // every frame fit, so the first frame measurement of the real screen
    // overwrites this seed outright, whatever its dims were. A session that
    // never measures (merc module off) therefore runs on a `remembered` label,
    // which is exactly what that label is for: a consumer that cannot afford a
    // stale screen weighs the label and waits for a measurement.
    let remembered = match settings.screen_scale {
        Some(s) if !s.is_sane() => {
            rejected.push(format!(
                "screen scale: {}x{} at ui_scale {} is not a screen, ignoring it",
                s.width, s.height, s.ui_scale
            ));
            None
        }
        other => other.map(|s| s.to_slice()),
    };
    *state.screen.lock().unwrap_or_else(|e| e.into_inner()) = remembered;

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
