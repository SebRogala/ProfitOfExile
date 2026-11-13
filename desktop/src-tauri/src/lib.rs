mod capture;
mod clock;
mod fingerprint;
mod focus;
mod font_alerts;
mod font_ledger;
mod font_parser;
mod font_session;
mod gem_matcher;
mod http;
mod lab;
mod lab_detection;
mod lab_scan;
mod lab_navigation;
mod client_log;
mod lab_state;
mod log_watcher;
mod loot_capture;
mod loot_api;
mod loot_profiles;
mod mercenary;
mod modules;
mod ocr;
mod overlay_hook;
mod settings;
mod ssot;
mod temple;
mod trade;
mod updater_channel;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OcrRectView {
    pub key: String,
    pub label: String,
    pub rect: Option<[i32; 4]>,
    pub rows: Vec<[i32; 4]>,
    pub source: String,
}

/// The gem name tooltip region is a FIXED rect, not an anchor — there is
/// nothing on screen to anchor it to (owner ruling, 2026-09-08). The hovered
/// gem's name prints anchored to the TOP border of the screen, so the region is
/// the client's FULL-WIDTH top band, exactly one name line tall — never taller:
/// below the name sit the tag and level lines, which are not read.
///
/// Full width, not "up to the inventory" (owner, 2026-09-09): the tooltip
/// follows the hovered item horizontally, so a gem hovered in the inventory
/// prints its name across the inventory's left edge. The band that stopped at
/// that edge read `EXPLOSIVE CONCOCTION OF DESTRUCTIC` and, three times over
/// 30 s, `POISONOUS CONCOCTION O` (app.log 2026-09-09 01:01) — the same cut on
/// every read, so the band's edge and not OCR noise — and the third gem was
/// never detected. The inventory frame's top 40 px carry ornament only; its
/// title prints below the band.
///
/// `GEM_NAME_BAND_H_REF · ui_scale` is the band's height. `INVENTORY_PANEL_W_REF
/// · ui_scale` is the right-docked inventory's width, which the font rule uses
/// to centre the Divine Font panel in the space left of it. Both numbers are in
/// REFERENCE px — the 1920x1200 unit [`ssot::ScreenSlice::ui_scale`] measures,
/// where 1080p is 0.90.
///
/// **PROVISIONAL, measured off ONE screenshot** (the owner's, 2026-09-08,
/// 1920x1080 at ui_scale 0.90, taken 1:1): the inventory frame's left edge at
/// ≈1262 px → 658 px wide → 731 reference px; the name band ≈40 px → 44
/// reference px. Settings → OCR Regions → Preview shows the result on the
/// game; correct these two numbers there, not the rule.
pub const INVENTORY_PANEL_W_REF: i32 = 731;
/// See [`INVENTORY_PANEL_W_REF`].
pub const GEM_NAME_BAND_H_REF: i32 = 44;

/// The font panel region is a FIXED rule too, not an anchor and not a scaled
/// literal: the Divine Font panel opens CENTRED in the space left of the
/// right-docked inventory (its centre, the CRAFT button's and the "Crafts
/// Remaining" box's all sit at ≈630 px on a 1920 client whose inventory starts
/// at ≈1262 → (1920 − 658) / 2 = 631), top-anchored under the title bar. The
/// region is the panel's text-bearing interior column: `FONT_PANEL_W_REF ·
/// ui_scale` wide, centred on that space, from `FONT_PANEL_TOP_REF · ui_scale`
/// below the client's top edge (just under the title bar's rule) and
/// `FONT_PANEL_H_REF · ui_scale` tall (down past the "Crafts Remaining" box).
/// It must hold BOTH texts the parser reads — the option lines under the title
/// and the "Crafts Remaining: N" box under the CRAFT button — so the wheel and
/// the button between them are inside it; `font_parser` is keyword-anchored and
/// reads past them.
///
/// The centring is INFERRED from one 16:9 screenshot, where it is
/// indistinguishable from a fixed left offset; a 16:10 client is the
/// discriminator (centred: x ≈ 224 at ui_scale 1.0; a fixed offset: x ≈ 331),
/// and the OCR Regions preview on the 1920x1200 laptop settles it.
///
/// **PROVISIONAL, measured off ONE screenshot** (the owner's, 2026-09-09,
/// 1920x1080 at ui_scale 0.90, taken 1:1): the panel frame at ≈290 and ≈970 px,
/// the body interior ≈300–965 → 666 px → 740 reference px; the title bar's
/// rule at ≈214 px, the first option line's top at 240 → the region's top at
/// 220 px → 244 reference px; that panel listed FOUR options (rows at 240, 301,
/// 353, 404 — a 51.5 px pitch, the first row wrapped to two lines), the option
/// lines end at ≈417 and the "Crafts Remaining" box spans ≈694–740, so the
/// bottom that panel needs is 748 px → 528 px tall → 587 reference px. The
/// panel lists up to SIX options and grows DOWNWARD — the chrome's top stays
/// put, the list expands (owner, 2026-09-09) — so the top holds and the height
/// must cover the longest list. The six-option layout is UNMEASURED: the height
/// budgets two more rows at the measured pitch (57 reference px each), each
/// allowed to wrap like the first (28 more), 587 + 2 · 85 = 757 — 681 px at
/// 1080p, bottom at ≈901, still above the flask bar. A six-option panel in the
/// preview is what corrects it. Settings → OCR Regions → Preview shows the
/// result on the game; correct these three numbers there, not the rule.
pub const FONT_PANEL_W_REF: i32 = 740;
/// See [`FONT_PANEL_W_REF`].
pub const FONT_PANEL_TOP_REF: i32 = 244;
/// See [`FONT_PANEL_W_REF`].
pub const FONT_PANEL_H_REF: i32 = 757;

/// What the placement layer assumes when NOTHING has measured a screen: that
/// the game is running at 1080p. It is the `ui_scale` a 1920x1080 screen
/// measures (1080/1200).
///
/// This is deliberately NOT the README's fail-closed rule for
/// [`ssot::ScreenSlice`] readers, and the difference is worth being explicit
/// about: a widget that fails closed simply is not drawn, while an OCR loop that
/// fails closed stops reading gems at all. Assuming 1080p gives an unmeasured
/// screen the lab rules evaluated at 1080p (neither lab region has a shipped
/// literal any more), and the scan loops say so once per session in the app log
/// instead of the assumption being silent.
pub const ASSUMED_UI_SCALE: f32 = 0.90;

/// The one line a scan loop logs when the rect it asked for came back smaller
/// than it asked for, `None` when the crop is the requested size.
///
/// `image`'s `crop_imm` CLAMPS to the image bounds (`imageops::crop_dimms`:
/// `x = min(x, iwidth)`, `width = min(width, iwidth - x)`) — it does not panic
/// and it does not error. So a rect that hangs off the capture is silently a
/// SMALLER crop, and the OCR just quietly reads less of the panel. Comparing the
/// sizes is what turns that into a breadcrumb; no clipping is needed, because
/// the crate already clipped.
fn crop_shortfall(region: &CaptureRegion, cropped_w: u32, cropped_h: u32) -> Option<String> {
    if cropped_w == region.w && cropped_h == region.h {
        return None;
    }
    Some(format!(
        "lab OCR region exceeds the capture — asked for {}x{} at ({}, {}), got {}x{}; the read is of a partial region",
        region.w, region.h, region.x, region.y, cropped_w, cropped_h,
    ))
}

fn ocr_rect_view(
    key: &'static str,
    label: &'static str,
    rect: Option<[i32; 4]>,
    rows: Vec<[i32; 4]>,
    source: impl Into<String>,
) -> OcrRectView {
    OcrRectView {
        key: key.to_string(),
        label: label.to_string(),
        rect,
        rows,
        source: source.into(),
    }
}

fn capture_region(rect: [i32; 4]) -> CaptureRegion {
    CaptureRegion { x: rect[0], y: rect[1], w: rect[2].max(0) as u32, h: rect[3].max(0) as u32 }
}

/// How many gem-band ticks [`dump_gem_band`] keeps.
const GEM_BAND_RING: u32 = 40;

/// Debug instrument behind Dev Tools → "Gem band dump" (`AppState::gem_band_dump`):
/// the raw gem band crop the OCR was handed, kept as a ring of the last
/// [`GEM_BAND_RING`] ticks under `<app_data>/lab-debug/`
/// (`gem-band-NN.png`, NN = tick mod ring size; the file's mtime dates it).
///
/// 2026-09-09: a socketed gem's tooltip sat inside the band for 30 ticks and
/// the band read nothing, while the same header read cleanly from screenshots
/// and from a GDI grab of the same screen — so the open question is what the
/// APP's capture held on those ticks, and only the crop itself answers it. A
/// write failure is logged once per process; the loop never stops for it.
fn dump_gem_band(app: &AppHandle, band: &image::DynamicImage, tick: u32) {
    static FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let Ok(dir) = app.path().app_data_dir() else { return };
    let dir = dir.join("lab-debug");
    let path = dir.join(format!("gem-band-{:02}.png", tick % GEM_BAND_RING));
    let result = std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| {
        band.save(&path).map_err(|e| e.to_string())
    });
    if let Err(e) = result {
        if !FAILED.swap(true, Ordering::SeqCst) {
            app_log(app, format!("lab-debug: could not write {}: {e}", path.display()));
        }
    }
}

fn temple_rects_from_rois(
    rois: &[crate::temple::slice::RoiView],
) -> (Option<[i32; 4]>, Option<[i32; 4]>) {
    (
        rois
            .iter()
            .find(|roi| roi.kind == crate::temple::slice::PANEL_REGION)
            .map(|roi| roi.rect),
        rois
            .iter()
            .find(|roi| roi.kind == crate::temple::slice::REMAINING_REGION)
            .map(|roi| roi.rect),
    )
}

/// Assemble the five display-only OCR preview rows from the derived placements.
/// Temple rows are generated from the placement's canonical Entrance origin;
/// they never read the last located layout. The merc row uses the same padded
/// crop as the merc detect tick and stays unlocated without the snapshot screen.
fn assemble_ocr_rects(
    placements: &ssot::Placements,
    screen: Option<&ssot::ScreenSlice>,
    temple_rects: Option<(Option<[i32; 4]>, Option<[i32; 4]>)>,
) -> Vec<OcrRectView> {
    let (temple_panel, temple_remaining) = temple_rects.unwrap_or((None, None));
    let anchors = screen.and_then(|screen| screen.anchors);
    let temple_source = if anchors.and_then(|a| a.temple_entrance).is_some() {
        "remembered"
    } else {
        "seed"
    };
    let (merc_rect, merc_rows) = match (placements.merc, screen) {
        (Some(merc), Some(screen)) => {
            // The preview draws the SEED's shipped reference row geometry. It
            // can differ from live tick geometry under a `merc-geometry.json`
            // `rowPitch` override or a held fit.
            let geometry = crate::mercenary::MercGeometry::default();
            let pitch = geometry.row_pitch * screen.ui_scale;
            (
                Some(crate::mercenary::geometry::placed_panel_crop(
                    merc.panel,
                    screen.ui_scale,
                    [screen.width, screen.height],
                )),
                crate::mercenary::geometry::placed_row_rects(
                    merc.panel,
                    &geometry,
                    screen.ui_scale,
                    pitch,
                    None,
                ),
            )
        }
        _ => (None, Vec::new()),
    };
    let merc_source = if merc_rect.is_some() {
        if anchors.and_then(|a| a.merc_panel).is_some() {
            "remembered"
        } else {
            "seed"
        }
    } else {
        "unlocated"
    };

    vec![
        ocr_rect_view(
            "lab.gem",
            "Gem tooltip",
            Some(placements.lab.gem),
            Vec::new(),
            "seed",
        ),
        ocr_rect_view(
            "lab.font",
            "Font panel",
            Some(placements.lab.font),
            Vec::new(),
            "seed",
        ),
        ocr_rect_view(
            "temple.panel",
            "Temple panel",
            temple_panel,
            Vec::new(),
            if temple_panel.is_some() { temple_source } else { "unlocated" },
        ),
        ocr_rect_view(
            "temple.remaining",
            "Temple remaining",
            temple_remaining,
            Vec::new(),
            if temple_remaining.is_some() { temple_source } else { "unlocated" },
        ),
        ocr_rect_view(
            "merc.panel",
            "Merc OCR crop",
            merc_rect,
            merc_rows,
            merc_source,
        ),
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub state: String,
    pub app_version: String,
    pub pair_code: String,
    pub detected_gems: Vec<String>,
    pub client_txt_path: String,
    pub client_txt_exists: bool,
    pub server_url: String,
    pub sidebar_open: bool,
    pub game_focused: bool,
    pub trade_stale_warn_secs: u32,
    pub trade_stale_critical_secs: u32,
    pub trade_auto_refresh_secs: u32,
    pub auto_trade_enabled: bool,
    pub device_id: String,
    /// `fingerprint::USES_DEV_SALT` — true means `device_id` is not the id a
    /// release build has on this PC. The identify dialog says so next to it.
    pub device_id_dev_salt: bool,
    /// Sealed rounds accumulated in the current font session. Drives the
    /// discard affordance: zero means there is nothing to throw away.
    pub font_session_rounds: usize,
    /// Set when a scan thread had to fall back off the en-US recognizer. The
    /// whole OCR pipeline (gem names, font options, merc rows) misreads on a
    /// profile-language recognizer, and the only other trace is one LOGS line
    /// that scrolls away, so it is surfaced in Settings as a standing warning.
    pub ocr_language_warning: Option<String>,
    /// Why every request is blocked this run (an unusable saved proxy);
    /// `None` with a usable proxy or none.
    pub proxy_error: Option<String>,
}

pub use font_session::{FontRound, FontSessionData};
pub(crate) use lab::commands::{fetch_gem_names, send_gems_to_server, send_font_session_data, record_lab_detection, publish_lab_detection};
#[cfg(test)]
pub(crate) use lab::commands::apply_manual_lab_mode;

pub struct AppState {
    pub loot: loot_capture::LootState,
    /// Hardware-based device fingerprint — computed once at startup, immutable.
    /// String is Sync so no Mutex needed.
    pub device_id: String,
    pub pair_code: Mutex<String>,
    pub client_txt_path: Mutex<String>,
    pub server_url: Mutex<String>,
    pub lab_scan: lab_scan::LabScanState,
    pub lab_state: Mutex<lab_state::LabState>,
    pub logs: Mutex<Vec<String>>,
    pub sidebar_open: Mutex<bool>,
    pub game_focused: Mutex<bool>,
    pub trade_client: trade::TradeApiClient,
    /// General-purpose HTTP client for server communication (separate from trade_client
    /// which has GGG-specific User-Agent/headers).
    pub server_http: reqwest::Client,
    /// The proxy `run()` read at startup and every transport uses until the
    /// next launch; the saved `Settings::proxy_url` may differ.
    pub proxy_in_effect: http::ProxyConfig,
    /// Cancel signal for the current log watcher. Send () to stop it.
    pub watcher_cancel: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    /// Cached comparator overlay data (results + trade data) shared between windows.
    pub comparator_data: Mutex<serde_json::Value>,
    /// Stop signal for the overlay mouse hook thread.
    pub overlay_hook_stop: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    pub focus_poller_stop: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    pub debug_mode: Mutex<bool>,
    /// Dev Tools → "Gem band dump": whether the lab gem scan writes its band
    /// crops ([`dump_gem_band`]). Session-only and off at start — an
    /// instrument, not a setting, and deliberately not tied to `debug_mode`,
    /// which the owner runs permanently (2026-09-09).
    pub gem_band_dump: std::sync::atomic::AtomicBool,
    /// Trade staleness thresholds and auto-trade — see `settings::TradeSettings`.
    /// Taken alone.
    pub trade_settings: Mutex<crate::settings::TradeSettings>,
    /// Aspirant's Trial entry count (reset on Aspirants' Plaza). Font OCR starts at 3.
    pub aspirant_trial_count: AtomicU32,
    /// True when player is inside the labyrinth (between PlazaEntered and LabExited).
    /// Used by lab_navigation to determine if a non-lab area entry is a lab exit.
    pub in_lab: AtomicBool,
    /// Whether the foreground window IS the game right now — the raw focus
    /// read, unlike `game_focused`, which is held over our own windows so
    /// overlay clicks do not blank the overlays. Screen-capture modules
    /// (merc OCR) read this one: over our own window they would otherwise
    /// capture the app itself (measured 2026-08-24).
    pub game_in_foreground: AtomicBool,
    /// The compass and shrine-warning selections — see
    /// `settings::CompassSettings`. Taken alone.
    pub compass_settings: Mutex<crate::settings::CompassSettings>,
    pub lab_overlays_enabled: Mutex<bool>,
    /// The lab selections — see `settings::LabSettings`. Taken alone.
    pub lab_settings: Mutex<crate::settings::LabSettings>,
    /// Schema-less UI view preferences (sort mode, colour filter, row limit…).
    /// The frontend owns the keys; Rust only stores and persists the map. A
    /// typed Settings field is warranted only when Rust itself reads the value.
    pub ui_prefs: Mutex<std::collections::HashMap<String, String>>,
    /// App-wide cross-window state SSOT (POE-128). Rust-owned; overlays read it
    /// by polling the `get_ssot` command. See src/ssot.rs.
    pub ssot: Mutex<ssot::AppSsotSnapshot>,
    /// Per-module enabled flags — the SINGLE owner, always holding the
    /// **effective** state (registry defaults overlaid with the persisted
    /// delta, written by `settings::apply_to_state`). See src/modules.rs.
    pub modules_enabled: Mutex<std::collections::HashMap<String, bool>>,
    /// Currently running modules, keyed by module id. Acquired BEFORE
    /// `modules_enabled` — never the inverse (lock order, see src/modules.rs).
    pub module_handles: Mutex<std::collections::HashMap<String, modules::ModuleHandle>>,
    /// Latched at the top of the main-window `CloseRequested` handler. A
    /// distinct `reconcile` input: while set, reconcile stops everything and
    /// starts nothing, so a racing `set_module_enabled` cannot respawn.
    pub modules_shutting_down: AtomicBool,
    /// Merc OCR capture state (POE-165) — the owner of the `mercenary` SSOT
    /// slice. Written by the capture loop, projected read-only into every
    /// snapshot by `ssot::build_snapshot`. See src/mercenary/mod.rs.
    pub mercenary: Mutex<mercenary::MercenarySlice>,
    /// The merc module's runtime state (POE-342) — templates, trade toggles,
    /// sync, burst gate and counters, each with its own lock. See
    /// `mercenary::MercState`.
    pub merc: mercenary::MercState,
    /// Temple builder state (POE-171) — the owner of the `temple` SSOT slice.
    /// Written by the temple capture loop and by the `temple_set_*` commands,
    /// projected read-only into every snapshot by `ssot::build_snapshot`.
    /// Acquired alone, never inside a module lock (lock order — see
    /// src/modules.rs).
    pub temple: Mutex<temple::slice::TempleSlice>,
    /// The temple module's runtime state (settings, market, valuation, arm,
    /// rearm, epoch) — see `temple::TempleState`.
    pub temple_state: temple::TempleState,
    /// The exceptional-support price read behind the Font alerts overlay (POE-328).
    ///
    /// Written ONLY by `font_alerts::spawn_exceptional_price_poll` and
    /// `font_alerts::on_server_url_changed`; not persisted (a copy of what a
    /// server said); acquired alone.
    pub exceptional_prices: Mutex<font_alerts::PriceMirror>,
    /// The screen the game is drawn on and the game-UI scale measured on it
    /// (POE-214) — the owner of the `screen` SSOT slice. Written by ONE writer,
    /// the merc detect tick, through `ssot::publish_screen`, and projected
    /// read-only into every snapshot by `ssot::build_snapshot`. Acquired alone,
    /// never inside a module lock (lock order — see src/modules.rs).
    ///
    /// Its own owner rather than a field of `AppState.mercenary`: the merc
    /// slice is retired when the recruit window closes, and the screen's scale
    /// is not — it is what the Lab capture regions and (once the temple's
    /// unit ratio is measured) the temple anchor want to read. `None` until
    /// something measures one; do not read a missing value as 1.0.
    pub screen: Mutex<Option<ssot::ScreenSlice>>,
    /// The display the GAME is on (POE-237) — resolved by the focus poller from
    /// the PoE window's own HWND on every transition into the game, and read by
    /// `capture::capture_screen` to decide which monitor to grab.
    ///
    /// `None` means nothing has seen the game window yet, which capture reads
    /// as "the primary monitor" — the pre-POE-237 behaviour, and the permanent
    /// state off Windows, where there is no poller and no capture.
    ///
    /// Its own owner rather than a field of the focus state: it outlives a blur
    /// (the game does not change display by being alt-tabbed away from) and its
    /// reader is the capture layer, which has nothing to do with the show/hide
    /// list the poller's other output drives.
    pub game_monitor: Mutex<Option<capture::GameMonitor>>,
    /// Where the user put each overlay WIDGET (POE-225), keyed
    /// `"<module>.<widget>"` — the owner of `Settings.widgets`.
    ///
    /// An owner rather than a read-through to the file because two windows want
    /// it: the module's fullscreen overlay places its widgets from it, and
    /// Settings lists and edits the same rows. A `settings::load` per read
    /// would put a disk round-trip in the overlay's first paint and would let
    /// the two windows disagree for as long as a save was in flight.
    ///
    /// Acquired alone, never inside a module lock (lock order — see
    /// src/modules.rs).
    pub widgets: Mutex<std::collections::BTreeMap<String, settings::WidgetGeometry>>,
}

/// Build the full AppStatus from current state. Used by get_status command and event emitting.
fn build_status(state: &AppState) -> AppStatus {
    let client_txt_path = state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let client_txt_exists = std::path::Path::new(&client_txt_path).exists();
    let trade = state.trade_settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
    AppStatus {
        state: format!("{:?}", *state.lab_state.lock().unwrap_or_else(|e| e.into_inner())),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        pair_code: state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        detected_gems: state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        client_txt_path,
        client_txt_exists,
        server_url: state.server_url.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        sidebar_open: *state.sidebar_open.lock().unwrap_or_else(|e| e.into_inner()),
        game_focused: *state.game_focused.lock().unwrap_or_else(|e| e.into_inner()),
        trade_stale_warn_secs: trade.trade_stale_warn_secs,
        trade_stale_critical_secs: trade.trade_stale_critical_secs,
        trade_auto_refresh_secs: trade.trade_auto_refresh_secs,
        auto_trade_enabled: trade.auto_trade_enabled,
        device_id: state.device_id.clone(),
        device_id_dev_salt: fingerprint::USES_DEV_SALT,
        font_session_rounds: state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner()).rounds.len(),
        ocr_language_warning: ocr_warning_field(),
        proxy_error: state.proxy_in_effect.blocked_reason().map(str::to_string),
    }
}

/// Save current settings to disk. Call after any persistent state change.
/// Preserves window and legacy overlay settings from the existing file.
pub(crate) fn persist_settings(app: &AppHandle) {
    settings::update(app, |existing| {
        let state = app.state::<AppState>();
        let mut s = settings::from_state(&state);
        // Keep the remembered screen scale when this session has nothing to replace
        // it with. `from_state`'s screen-scale projection is lossy — a `MercOcr`
        // slice maps to `None` — so without this every one of the call sites below
        // would null a stored measurement after the first OCR-only tick.
        // `preserve_screen_scale` states the rest of the rule, including why a
        // `MercFrame` measurement at different dimensions still overwrites. The
        // same merge keeps the window position and the overlay rows, which no
        // AppState mutex owns.
        settings::merge_stored_rows(existing, &mut s);
        s
    });
}

/// Emit the full app status to all frontend listeners.
pub(crate) fn emit_status(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Err(e) = app.emit("status-changed", build_status(&state)) { log::warn!("emit status-changed failed: {}", e); }
}

/// The `ocr_language_warning` field of `AppStatus`, as its own seam.
///
/// `build_status` needs a live `AppState` to call, so this is the part of the
/// payload a unit test can reach. Reads the cached warning and deliberately NOT
/// `ocr::engine_report()`: `build_status` runs on every command and event
/// thread, and resolving an engine there would construct an extra one — and
/// re-log the fallback — per thread.
fn ocr_warning_field() -> Option<String> {
    ocr::language_warning()
}

thread_local! {
    /// Whether this thread has already pushed the OCR language warning to the
    /// UI. Per-thread rather than global so each scan thread emits at most once
    /// while none of them depends on another having run.
    static OCR_WARNING_REPORTED: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

/// Resolve this thread's OCR recognizer, log which one it is, and push a cached
/// language warning to the UI once per thread.
///
/// Every scan thread calls this on start. The emit cannot be left to the spawn
/// site: `spawn_font_scan` emits before the thread exists, so at that point no
/// engine has resolved and `ocr_language_warning` is still `None`.
///
/// The trigger is the warning being PRESENT, not this thread having been the one
/// to cache it. Any engine resolution caches it — including one from a debug
/// command (`test_ocr_on_image`, the merc debug dump), which has no status to
/// emit — so keying on the cache transition would drop the notification whenever
/// a debug path resolved first.
fn report_ocr_engine(app: &AppHandle) {
    app_log(app, ocr::engine_report());
    // `replace` returns the previous value, so the first call through here on a
    // thread with a warning cached is the only one that emits.
    if ocr::language_warning().is_some() && !OCR_WARNING_REPORTED.with(|c| c.replace(true)) {
        emit_status(app);
    }
}

/// Emit the current logs array to all frontend listeners.
fn emit_logs(app: &AppHandle) {
    let state = app.state::<AppState>();
    let logs = state.logs.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Err(e) = app.emit("logs-changed", logs) { log::warn!("emit logs-changed failed: {}", e); }
}

/// Add a log entry: in-memory buffer (UI) + persistent file + emit to frontend.
pub(crate) fn app_log(app: &AppHandle, msg: String) {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let formatted = format!("[{}] {}", timestamp, msg);

    // In-memory buffer for UI
    let state = app.state::<AppState>();
    {
        let mut logs = state.logs.lock().unwrap_or_else(|e| e.into_inner());
        logs.push(formatted.clone());
        if logs.len() > 50 {
            let excess = logs.len() - 50;
            logs.drain(0..excess);
        }
    }

    // Persistent log file — same dir as settings
    if let Ok(dir) = app.path().app_data_dir() {
        let log_path = dir.join("app.log");
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let _ = writeln!(file, "{}", formatted);
        }
    }

    emit_logs(app);
}

fn generate_pair_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
    (0..4).map(|_| chars[rng.gen_range(0..chars.len())]).collect()
}

#[tauri::command]
fn get_status(state: tauri::State<AppState>) -> AppStatus {
    build_status(&state)
}

#[tauri::command]
fn get_pair_code(state: tauri::State<AppState>) -> String {
    state.pair_code.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Returns the first 8 characters of the device fingerprint (for display in the identify dialog).
#[tauri::command]
fn get_device_id(state: tauri::State<AppState>) -> String {
    state.device_id.chars().take(8).collect()
}

#[tauri::command]
fn regenerate_pair_code(app: AppHandle) -> String {
    let state = app.state::<AppState>();
    let new_code = generate_pair_code();
    *state.pair_code.lock().unwrap_or_else(|e| e.into_inner()) = new_code.clone();
    emit_status(&app);
    new_code
}

/// Detect Client.txt path using multiple strategies (most reliable first).
/// 1. Running PoE process → derive logs path from executable location
/// 2. Hardcoded common paths (GGG standalone, Steam default, Epic Games)
fn detect_client_txt_path() -> String {
    // Strategy 1: Find running PoE process and derive logs path
    if let Some(path) = detect_from_running_process() {
        log::info!("Client.txt detected from running process: {}", path);
        return path;
    }

    // Strategy 2: Check common install paths
    let common_paths = [
        r"C:\Program Files (x86)\Grinding Gear Games\Path of Exile\logs\Client.txt",
        r"C:\Program Files (x86)\Steam\steamapps\common\Path of Exile\logs\Client.txt",
        r"C:\Program Files\Epic Games\PathOfExile\logs\Client.txt",
        // 32-bit GGG path
        r"C:\Program Files\Grinding Gear Games\Path of Exile\logs\Client.txt",
    ];

    for path in &common_paths {
        if std::path::Path::new(path).exists() {
            log::info!("Client.txt found at common path: {}", path);
            return path.to_string();
        }
    }

    // Strategy 3: Try Steam library folders from registry
    if let Some(path) = detect_from_steam_libraries() {
        log::info!("Client.txt detected from Steam library: {}", path);
        return path;
    }

    // Default fallback — user will see the warning
    log::warn!("Client.txt not found in any known location");
    common_paths[0].to_string()
}

/// Find Client.txt by detecting a running PathOfExile process.
fn detect_from_running_process() -> Option<String> {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    for process in sys.processes().values() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name.contains("pathofexile") && name.ends_with(".exe") {
            if let Some(exe_path) = process.exe() {
                let game_dir = exe_path.parent()?;
                let client_txt = game_dir.join("logs").join("Client.txt");
                if client_txt.exists() {
                    return Some(client_txt.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

/// Find Client.txt in Steam library folders by reading Steam's config.
fn detect_from_steam_libraries() -> Option<String> {
    #[cfg(windows)]
    {
        // Read Steam install path from registry
        use std::process::Command;
        let output = Command::new("reg")
            .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let steam_path = stdout.lines()
            .find(|l| l.contains("SteamPath"))?
            .split("REG_SZ")
            .nth(1)?
            .trim()
            .to_string();

        // Parse libraryfolders.vdf to find all library paths
        let vdf_path = format!(r"{}\steamapps\libraryfolders.vdf", steam_path);
        let vdf_content = std::fs::read_to_string(&vdf_path).ok()?;

        // Simple VDF parser: look for "path" values
        for line in vdf_content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"path\"") {
                let path = trimmed
                    .split('"')
                    .nth(3)?
                    .replace("\\\\", "\\");
                let client_txt = format!(
                    r"{}\steamapps\common\Path of Exile\logs\Client.txt",
                    path
                );
                if std::path::Path::new(&client_txt).exists() {
                    return Some(client_txt);
                }
            }
        }
    }
    None
}

#[tauri::command]
fn set_client_txt_path(path: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = path;
    persist_settings(&app);
    emit_status(&app);
    restart_log_watcher(app);
}

/// Nuclear reset: delete settings file and re-initialize with defaults.
#[tauri::command]
fn reset_all_settings(app: AppHandle) {
    // Delete the settings file
    if let Some(path) = settings::settings_path_pub(&app) {
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                app_log(&app, format!("Failed to delete settings: {}", e));
            } else {
                app_log(&app, format!("Settings file deleted: {:?}", path));
            }
        }
    }
    // Re-apply defaults to AppState
    let defaults = settings::Settings::default();
    let state = app.state::<AppState>();
    // Defaults cannot be rejected by the build that ships them, but the log
    // line is the contract, not the odds: if this ever prints, the shipped
    // defaults and the validators disagree and that is worth seeing.
    for rejection in settings::apply_to_state(&defaults, &state) {
        app_log(&app, rejection);
    }
    // The apply above rewrote `temple_state.settings.preset` and `.custom`, which are
    // two of the three inputs the valuation is keyed on, so the stored table is
    // now for settings nobody is running (POE-257 WI-3 fix round, 2026-09-07).
    // Warm it here for the same reason `temple_set_preset` and
    // `temple_set_custom` do: without it the next board read pays the 193-245 ms
    // rebuild itself. Nothing depends on it landing — the key is the guard.
    temple::valuation::warm_temple_valuation(&app);
    // Re-detect Client.txt
    let detected = detect_client_txt_path();
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = detected;
    // Save fresh settings
    persist_settings(&app);
    // `apply_to_state` above rewrote the module owner map to registry
    // defaults, so bring the running set back in line with it.
    modules::apply_reconcile(&app);
    // Nudge after the reconcile, exactly as `set_module_enabled` does: without
    // it every window keeps polling the pre-reset `modules` slice until the
    // next unrelated emit.
    ssot::emit_ssot(&app);
    // Tell the windows. Lab mode and both markets live in AppState and are
    // mirrored by LabPage, which reads them once at mount — without this the
    // mirror keeps showing the pre-reset market while every scan is stamped
    // with the reset one, and the paired web view follows Rust, not the mirror.
    if let Err(e) = app.emit("settings-reset", ()) {
        log::warn!("emit settings-reset failed: {}", e);
    }
    app_log(&app, "All settings reset to defaults".to_string());
    emit_status(&app);
    restart_log_watcher(app);
}

#[tauri::command]
fn reset_client_txt_path(app: AppHandle) {
    let state = app.state::<AppState>();
    let detected = detect_client_txt_path();
    app_log(&app, format!("Auto-detect Client.txt: {}", detected));
    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = detected;
    persist_settings(&app);
    emit_status(&app);
    restart_log_watcher(app);
}

#[tauri::command]
async fn browse_client_txt(app: AppHandle) -> Result<String, String> {
    let file = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("Text files", &["txt"])
            .set_title("Select Path of Exile Client.txt")
            .pick_file()
    }).await.map_err(|e| format!("dialog thread error: {}", e))?;

    match file {
        Some(path) => {
            let path_str = path.to_string_lossy().to_string();
            let state = app.state::<AppState>();
            *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = path_str.clone();
            persist_settings(&app);
            emit_status(&app);
            restart_log_watcher(app);
            Ok(path_str)
        }
        None => Err("No file selected".to_string()),
    }
}

fn restart_log_watcher(app: AppHandle) {
    let state = app.state::<AppState>();
    // Cancel the existing watcher
    if let Some(cancel_tx) = state.watcher_cancel.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = cancel_tx.send(true);
    }
    app_log(&app, "Restarting log watcher...".to_string());
    spawn_log_watcher(app.clone());
}

/// Write the server url, and report whether it actually moved.
///
/// The answer is what gates the reset below: saving the url ALREADY in force —
/// Settings' Save on an untouched field, a DEBUG/PROD click on the side the app
/// is already on — must not drop the temple market and re-resolve the league,
/// because nothing on screen came from a server the app has left. The webview
/// makes the same distinction for the same reason (`stores/status.svelte.ts`,
/// `noteServerUrl`: a status that repeats the url is not a change).
///
/// Takes the mutex rather than the `AppHandle` so the rule is testable without
/// a running app — the same extraction `write_debug_mode` makes, and poison is
/// recovered rather than propagated for the same reason.
fn write_server_url(held: &Mutex<String>, url: String) -> bool {
    let mut guard = held.lock().unwrap_or_else(|e| e.into_inner());
    if *guard == url {
        return false;
    }
    *guard = url;
    true
}

#[tauri::command]
fn set_server_url(url: String, app: AppHandle) {
    let changed = {
        let state = app.state::<AppState>();
        write_server_url(&state.server_url, url)
    };
    persist_settings(&app);
    // Everything keyed to the OLD server, dropped and re-asked (POE-258): the
    // temple market's prices and the resolved league they are keyed on. Only on
    // an actual change — the guard above is scoped so this reads the url just
    // written.
    if changed {
        // The league is resolved again. The name standing was read from the
        // other server, and the market poll keys on it: without this a payload from
        // the new server would be discarded as `WrongLeague` on every poll for the
        // life of the session. The resolver's success wakes the poller, so the new
        // league's prices land as soon as the league does.
        ssot::supersede_league_fetches();
        temple::market::on_server_url_changed(&app);
        font_alerts::on_server_url_changed(&app);
        ssot::rearm_league(app.clone());
        // Main-window nudge for the new `server` slice; overlays get it on the 3 s poll.
        ssot::emit_ssot(&app);
    }
    emit_status(&app);
}

/// Return the current OCR regions in capture pixels for the read-only preview.
/// This assembles owners' published geometry only; it never captures or OCRs.
#[tauri::command]
fn get_ocr_rects(state: tauri::State<AppState>) -> Vec<OcrRectView> {
    let snapshot = ssot::build_snapshot(&state);
    let temple_rects = snapshot.placements.temple.and_then(|temple| {
        let scale = snapshot
            .screen
            .as_ref()
            .map(|screen| screen.ui_scale)
            .filter(|scale| scale.is_finite() && *scale > 0.0)
            .map(crate::temple::anchor::scale_for_ui_scale)?;
        Some(temple_rects_from_rois(&crate::temple::run::read_rois(
            temple.entrance_origin,
            scale,
        )))
    });

    assemble_ocr_rects(
        &snapshot.placements,
        snapshot.screen.as_ref(),
        temple_rects,
    )
}

/// Read the cursor position. READ ONLY — nothing in this app moves the cursor
/// or sends input; injecting input into the PoE client is against GGG's ToS.
/// Shared with the merc hover-confirm tick (POE-165 D5), which calls it
/// directly rather than duplicating the `cfg` blocks.
#[tauri::command]
fn capture_mouse_position() -> Result<(i32, i32), String> {
    // Get current mouse cursor position on screen
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        use windows::Win32::Foundation::POINT;
        let mut point = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut point)
                .map_err(|e| format!("Failed to get cursor position: {}", e))?;
        }
        Ok((point.x, point.y))
    }
    #[cfg(not(windows))]
    {
        Err("Mouse capture not available on this platform".to_string())
    }
}

/// Cancel the pending GEM trade lookups. In-flight request completes but
/// queued lookups bail out without making GGG requests.
///
/// Scoped to `TradeSource::Gem`: this is the Comparator's cancel button, and
/// before POE-202 it also killed every other consumer's queued lookups.
#[tauri::command]
fn trade_cancel(app: AppHandle) {
    let state = app.state::<AppState>();
    let source = trade::TradeSource::Gem;
    let remaining = state.trade_client.cancel(source);
    use tauri::Emitter;
    if let Err(e) = app.emit("trade-queue", trade::TradeQueueEvent::Cancelled { source, remaining }) {
        log::warn!("emit trade-queue Cancelled failed: {}", e);
    }
    app_log(&app, format!("Trade queue cancelled ({} pending)", remaining));
}

/// Total attempts for a trade submit that the server sheds with 503 — the
/// first send plus two retries.
///
/// The insert behind the endpoint is idempotent (`ON CONFLICT DO NOTHING` on
/// `(league, time, gem, variant)`) and the submit is already detached, so a
/// retry can duplicate work but never rows. The bound is the point: the server
/// grew a queue and a 503 precisely to shed load, and an unbounded client-side
/// retry would hand that load straight back.
const TRADE_SUBMIT_ATTEMPTS: u32 = 3;
/// Wait used when a 503 carries no parseable `Retry-After`.
const TRADE_SUBMIT_RETRY_FALLBACK: std::time::Duration = std::time::Duration::from_secs(1);
/// Ceiling on an honoured `Retry-After`. The row is a cache-enrichment nicety;
/// holding a task for minutes because a header said so is not worth it.
const TRADE_SUBMIT_RETRY_MAX: std::time::Duration = std::time::Duration::from_secs(5);

/// Leading `max` bytes of a response body for a log line.
///
/// Cuts on a character boundary: `&body[..body.len().min(max)]` panics when the
/// truncation point lands mid-codepoint, and an error body is exactly the kind
/// of response that can carry a non-ASCII byte (a proxy's HTML error page, a
/// server message quoting user text). Panicking while logging a rejection would
/// lose the rejection.
pub(crate) fn body_excerpt(body: &str, max: usize) -> &str {
    if body.len() <= max {
        return body;
    }
    let mut end = max;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    &body[..end]
}

/// Delay to wait before retrying a shed submit.
///
/// Honours the delta-seconds form of `Retry-After` (what the server sends),
/// clamped to `TRADE_SUBMIT_RETRY_MAX`. The HTTP-date form is not parsed — it
/// falls back like any other unusable value.
fn retry_after_delay(header: Option<&str>) -> std::time::Duration {
    header
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
        .unwrap_or(TRADE_SUBMIT_RETRY_FALLBACK)
        .min(TRADE_SUBMIT_RETRY_MAX)
}

/// Direct trade API lookup against GGG from the desktop app.
/// Each user has their own IP → own rate limits (no shared server bottleneck).
/// Accepts optional divine rate for chaos normalization of divine-priced listings.
/// When mode is "dedication", the query targets corrupted 21/23 skill gems.
#[tauri::command]
async fn trade_lookup(
    gem: String,
    variant: String,
    divine_rate: Option<f64>,
    mode: Option<String>,
    app: AppHandle,
) -> Result<trade::TradeLookupResult, String> {
    let state = app.state::<AppState>();
    let rate = divine_rate.unwrap_or(0.0);
    let dedication = mode.as_deref() == Some("dedication");
    if rate <= 0.0 {
        log::warn!("Trade lookup: divine_rate is 0 — divine-priced listings will NOT be normalized to chaos");
    }
    app_log(&app, format!("Trade lookup: {} ({}) divine_rate={:.0} dedication={}", gem, variant, rate, dedication));

    let app_for_emit = app.clone();
    let league = state.ssot.lock().unwrap_or_else(|e| e.into_inner()).league.name.clone();
    let result = state.trade_client.lookup_gem_with_mode(league, &gem, &variant, rate, dedication, |event| {
        use tauri::Emitter;
        if let Err(e) = app_for_emit.emit("trade-queue", &event) {
            log::warn!("emit trade-queue failed: {}", e);
        }
    }).await
        .map_err(|e| {
            if e == trade::CANCELLED {
                return e; // Don't log cancellations as errors
            }
            app_log(&app, format!("Trade error: {}", e));
            e
        })?;

    app_log(
        &app,
        format!(
            "Trade result: {} total, {} listings, floor={:.1} {}",
            result.total,
            result.listings.len(),
            if result.listings.is_empty() { 0.0 } else { result.listings[0].price },
            if result.listings.is_empty() { "" } else { &result.listings[0].currency },
        ),
    );

    // Fire-and-forget: submit trade result to server for cache enrichment
    {
        let server_url = settings::server_base(&state);
        let http = state.server_http.clone();
        let submit_result = result.clone();
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            let url = format!("{}/api/trade/submit", server_url);
            trade::submit::submit_trade_result(&http, &url, &submit_result, app_clone.clone(),
                |line| app_log(&app_clone, line),
            ).await;
        });
    }

    Ok(result)
}

/// What the delayed click-through setup ended up observing.
///
/// The setup itself is Windows-only and needs a real WebView2 HWND, so the
/// MAPPING from what it saw to what the command answers is split out below
/// (`clickthrough_outcome`): that half is what decides whether the caller keeps
/// the window or tears it down, and it is the half a test can drive.
///
/// Every variant but `Armed` is constructed on Windows only — off Windows there
/// is no extended style to fail at — so a `cargo check` on Linux would call the
/// other four unconstructed. The mapping below runs on BOTH platforms and its
/// tests build all five, which is what the allow is scoped to.
/// `make desktop-check-windows` type-checks the Windows half.
///
/// The variants are NOT equally likely. `WindowGone`, `HwndUnavailable` and
/// `NotTransparent` are the live signals; `IgnoreCursorFailed` is close to
/// unreachable, because tao's `set_ignore_cursor_events` returns `Ok`
/// unconditionally on Windows and Tauri only errors when the event loop is
/// already gone. It is kept because "close to unreachable" is not "cannot
/// happen" and the alternative is swallowing it, which is the defect this
/// command was fixed for.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
enum ClickthroughSetup {
    /// Click-through installed — or deliberately NOT re-armed because the user
    /// is already arranging widgets in this window — and the window registered
    /// with the mouse hook. The only success.
    Armed,
    /// The label was gone by the time the delay elapsed; there was nothing to
    /// arm. Ordinary during a fast module off→on, and still a failed creation:
    /// the caller must not report a window it no longer has. Its message
    /// carries [`CLICKTHROUGH_WINDOW_GONE`] so the caller can tell this
    /// ordinary case from a window that is live and eating clicks.
    WindowGone,
    /// `set_ignore_cursor_events(true)` refused. Near-unreachable in practice
    /// (see the type comment): the Windows implementation cannot fail, so this
    /// really means the event loop has gone away underneath us.
    IgnoreCursorFailed(String),
    /// It was applied, and `WS_EX_TRANSPARENT` did not read back set. The
    /// window is opaque to the mouse, and the hook only repairs a window the
    /// cursor is already over — so nothing will fix this on its own.
    NotTransparent,
    /// The HWND never became available, so the window was never registered and
    /// the hook cannot repair its style later either.
    HwndUnavailable,
}

/// The marker `WindowGone`'s message starts with.
///
/// A machine-readable prefix rather than a structured error, because the whole
/// contract is one string across the Tauri boundary and exactly ONE of these
/// failures is ordinary: a window destroyed inside the command's 1 s wait is
/// what an overlay toggled off mid-creation looks like, and reporting it as
/// "this window may be eating your clicks" would cry wolf on every fast toggle.
/// The caller matching it is `clickthroughReport` in
/// `src/lib/overlay/clickthrough-report.ts`, which keeps the same literal; the
/// test below is what stops this end of the pair drifting.
const CLICKTHROUGH_WINDOW_GONE: &str = "window-gone";

#[cfg(any(windows, test))]
fn clickthrough_failure_log_level(message: &str) -> log::Level {
    if message.starts_with(CLICKTHROUGH_WINDOW_GONE) {
        log::Level::Info
    } else {
        log::Level::Error
    }
}

/// Turn what the setup observed into the answer `set_overlay_clickthrough`
/// gives its caller.
///
/// Only `Armed` is success. Every other variant leaves a transparent,
/// always-on-top window that takes clicks meant for the game — and for a
/// widget-engine window, which is the size of the monitor, that is EVERY click
/// on the screen until the window is destroyed. So the failure is returned
/// rather than logged: the module-coupled callers destroy the half-built window
/// and let `module-lifecycle.ts` retry it, and the rest at least say so in the
/// app log.
fn clickthrough_outcome(label: &str, setup: ClickthroughSetup) -> Result<(), String> {
    match setup {
        ClickthroughSetup::Armed => Ok(()),
        ClickthroughSetup::WindowGone => Err(format!(
            "{}: Overlay '{}' not found after the setup delay",
            CLICKTHROUGH_WINDOW_GONE, label
        )),
        ClickthroughSetup::IgnoreCursorFailed(e) => Err(format!(
            "set_ignore_cursor_events failed for '{}': {}",
            label, e
        )),
        ClickthroughSetup::NotTransparent => Err(format!(
            "Overlay '{}' is not click-through after setup: WS_EX_TRANSPARENT did not read back",
            label
        )),
        ClickthroughSetup::HwndUnavailable => Err(format!(
            "Overlay '{}' HWND not available after the setup delay — not registered with the hook",
            label
        )),
    }
}

/// Set up an overlay window for click-through and REGISTER it with the hook.
/// Call from JS after the window is created. Delays 1s for HWND availability.
///
/// Every overlay registers, not only the interactive one: the hook is the only
/// thing that repairs the `WS_EX_TRANSPARENT` WebView2 strips off when it
/// rebuilds child windows, and it repairs what it tracks. Which clicks a window
/// then CLAIMS is a separate declaration — `set_overlay_hot_rects`, sent by the
/// window's own page — so this command no longer takes an interactive width.
///
/// AWAITED, and the outcome is the caller's to act on. The 1 s wait stays (the
/// WebView2 HWND is not available sooner — see the guide's runtime-earned
/// observations), but it is now spent inside the command rather than on a
/// detached thread the caller cannot observe. It used to return immediately and
/// only LOG a failed `set_ignore_cursor_events` or a missing HWND, which meant
/// a window that never became click-through was indistinguishable from one that
/// did: for the monitor-sized widget window that is an invisible, always-on-top
/// rectangle eating every click on the screen until it is destroyed.
///
/// The 500 ms `set_noactivate` repair below stays fire-and-forget on its own
/// thread. It is a REPAIR of a style WebView2 may strip while it builds its
/// children, not a gate — the window is already click-through without it, and
/// making creation wait another half-second on it would buy nothing.
#[tauri::command]
async fn set_overlay_clickthrough(label: String, app: AppHandle) -> Result<(), String> {
    apply_overlay_clickthrough(label, app).await
}

/// Nothing to arm where there is no Win32 window style to arm it with. Reported
/// as success so a Linux dev build's creation paths behave like the shipped
/// ones instead of tearing every overlay down on startup.
#[cfg(not(windows))]
async fn apply_overlay_clickthrough(label: String, app: AppHandle) -> Result<(), String> {
    let _ = app;
    clickthrough_outcome(&label, ClickthroughSetup::Armed)
}

#[cfg(windows)]
async fn apply_overlay_clickthrough(label: String, app: AppHandle) -> Result<(), String> {
    let for_thread = label.clone();
    // `spawn_blocking`, not `spawn`: the body sleeps a second and then makes
    // blocking Tauri and Win32 calls. Awaiting the join handle is what makes
    // the failure reach the caller.
    let setup = tauri::async_runtime::spawn_blocking(move || clickthrough_setup(for_thread, app))
        .await
        .map_err(|e| format!("click-through setup for '{}' did not run: {}", label, e))?;

    let outcome = clickthrough_outcome(&label, setup);
    match &outcome {
        Ok(()) => log::info!(
            "Overlay clickthrough setup complete for '{}' (registered with the mouse hook)",
            label
        ),
        // Logged here as well as returned: the caller reports through the app
        // log too, but a Rust-side line is what pairs the failure with the
        // Win32 detail that produced it.
        Err(msg) => log::log!(clickthrough_failure_log_level(msg), "{}", msg),
    }
    outcome
}

/// The blocking half: wait for the HWND, arm the window, register it, and
/// report what happened. Runs off the main thread.
#[cfg(windows)]
fn clickthrough_setup(label: String, app: AppHandle) -> ClickthroughSetup {
    use windows::Win32::Foundation::HWND;

    // WebView2 HWND not available immediately — wait for init
    std::thread::sleep(std::time::Duration::from_millis(1000));

    let window = match app.get_webview_window(&label) {
        Some(w) => w,
        None => return ClickthroughSetup::WindowGone,
    };

    // This setup runs a full second after the window was created, and
    // the user can have opened widget configuration inside that second.
    // Re-asserting click-through then would leave the window neither
    // interactive (cursor events ignored again) nor hooked (the hook
    // still sees `config_mode` and keeps its hands off it), with
    // nothing to undo it until config mode is closed. Registration
    // itself is unconditional — it is what the hook needs to repair
    // WS_EX_TRANSPARENT once config mode ends, and `register` keeps the
    // flag for a same-HWND re-register.
    let in_config = overlay_hook::config_mode(&label);
    if in_config {
        log::info!(
            "Overlay '{}' is in widget-configuration mode — registering without re-arming click-through",
            label
        );
    } else if let Err(e) = window.set_ignore_cursor_events(true) {
        // The call that makes the whole window click-through. Its refusal used
        // to be logged and swallowed; it is now the command's answer.
        return ClickthroughSetup::IgnoreCursorFailed(e.to_string());
    }

    // ALSO A BARRIER, not just a lookup. `set_ignore_cursor_events` above posts
    // its work to the event loop and returns; `hwnd()` is a blocking round-trip
    // to that same loop, so by the time it answers the style call has been
    // serviced. That is what makes the belt at the foot of this function a
    // read-back rather than a race. Moving the belt above this call, or making
    // this lookup non-blocking, breaks that ordering silently.
    let hwnd = match window.hwnd() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            // The variant carries no payload — the Win32 reason is worth a line
            // of its own rather than a wider enum only this log would read.
            log::warn!("Overlay '{}' HWND lookup failed: {}", label, e);
            return ClickthroughSetup::HwndUnavailable;
        }
    };
    let h = HWND(hwnd.0 as *mut _);
    if !in_config {
        unsafe { overlay_hook::set_noactivate(h); }
    }

    overlay_hook::register(&label, h);

    // Idempotent: the first overlay to get here installs the hook,
    // every later one reuses it and gets `None`.
    if let Some(tx) = overlay_hook::install_hook(app.clone()) {
        let state = app.state::<AppState>();
        *state.overlay_hook_stop.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    }

    // Re-apply WS_EX_NOACTIVATE after WebView2 children are created.
    // Re-checked, not inherited from `in_config`: config mode can be
    // entered during these 500 ms too, and WS_EX_NOACTIVATE on a
    // window the user is dragging widgets in stops it taking the
    // focus its own drag handles need.
    //
    // It resolves the LABEL again rather than reusing the handle captured here,
    // and that is the point of the re-lookup: this command can now report a
    // failure, and its callers answer one by DESTROYING the window. Win32
    // recycles HWND values, so a raw handle half a second old can name somebody
    // else's window by the time this runs. No window, no repair — and a window
    // rebuilt under the same label in the meantime is a window that wants
    // WS_EX_NOACTIVATE anyway.
    let label3 = label.clone();
    let app_for_repair = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let Some(window) = app_for_repair.get_webview_window(&label3) else {
            log::info!(
                "Overlay '{}' is gone — WS_EX_NOACTIVATE not re-applied",
                label3
            );
            return;
        };
        if overlay_hook::config_mode(&label3) {
            log::info!(
                "Overlay '{}' entered widget-configuration mode — WS_EX_NOACTIVATE not re-applied",
                label3
            );
            return;
        }
        match window.hwnd() {
            Ok(h) => unsafe { overlay_hook::set_noactivate(HWND(h.0 as *mut _)) },
            Err(e) => log::warn!(
                "Overlay '{}' HWND unavailable — WS_EX_NOACTIVATE not re-applied: {}",
                label3, e
            ),
        }
    });

    // The belt: ask the window back what it now carries. `set_ignore_cursor_events`
    // answering `Ok` is not proof the extended style took, and the one thing that
    // would otherwise repair it — the hook — only acts on a window the cursor is
    // already over. Read AFTER registration rather than instead of it: the
    // registration is wanted even on the path that reports a failure, because a
    // caller that keeps the window still wants it repairable.
    if !in_config && !clickthrough_belt_passes(h) {
        return ClickthroughSetup::NotTransparent;
    }

    ClickthroughSetup::Armed
}

/// How many times the belt reads the style back before calling it missing, and
/// how long it waits between reads.
///
/// More than one because a verdict that destroys the user's overlay must not
/// rest on a single sample: the `hwnd()` barrier above orders the style call
/// ahead of this read, but WebView2 is still building children underneath it,
/// and it is that child-building which strips `WS_EX_TRANSPARENT` in the first
/// place. Three reads 20 ms apart cost 40 ms on the failing path and nothing at
/// all on the passing one, which is the first read.
#[cfg(windows)]
const CLICKTHROUGH_BELT_READS: u32 = 3;
#[cfg(windows)]
const CLICKTHROUGH_BELT_GAP_MS: u64 = 20;

/// Whether the window reads back as click-through.
///
/// Only a style that is READABLE AND MISSING on every attempt fails. An
/// unreadable style is UNKNOWN, not missing (see `overlay_hook::is_transparent`),
/// and tearing down a window we could not measure would cost working overlays on
/// a Win32 hiccup.
#[cfg(windows)]
fn clickthrough_belt_passes(h: windows::Win32::Foundation::HWND) -> bool {
    for attempt in 1..=CLICKTHROUGH_BELT_READS {
        match unsafe { overlay_hook::is_transparent(h) } {
            Some(false) => {
                if attempt == CLICKTHROUGH_BELT_READS {
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(CLICKTHROUGH_BELT_GAP_MS));
            }
            _ => return true,
        }
    }
    true
}

/// Dev Tools → "Gem band dump" (see `AppState::gem_band_dump`).
#[tauri::command]
fn set_gem_band_dump(app: AppHandle, on: bool) {
    app.state::<AppState>().gem_band_dump.store(on, Ordering::SeqCst);
    app_log(&app, format!("lab-debug: gem band dump {}", if on { "on" } else { "off" }));
}

/// The current "Gem band dump" flag, so the Dev Tools page shows the state
/// Rust holds when it mounts rather than an unchecked box over a running dump.
#[tauri::command]
fn get_gem_band_dump(app: AppHandle) -> bool {
    app.state::<AppState>().gem_band_dump.load(Ordering::SeqCst)
}

/// Write `on` into the debug-mode flag.
///
/// The state half of the [`set_debug_mode`] command, split out because it is
/// the half that can be tested without an `AppHandle`.
///
/// IDEMPOTENT, and that is the contract. The command used to be a toggle with
/// no argument while its only caller — the Ctrl+Shift+F12 handler in
/// `(app)/+layout.svelte` — invoked it on the ON press alone. The UI's own
/// `debugMode` therefore flipped on every press and Rust's flipped on every
/// other one, so the second time debug mode was switched on, `debug_mode`
/// was already true, the command turned it OFF, and every debug-gated log line
/// in the app went silent while the UI reported debug mode ON (2026-08-26
/// smoke). A command that is told the state to be in cannot drift from the
/// state its caller thinks it is in.
///
/// Poison is recovered rather than propagated, as everywhere else this flag is
/// touched: a panicking reader must not cost the user their debug logging.
fn write_debug_mode(flag: &std::sync::Mutex<bool>, on: bool) {
    *flag.lock().unwrap_or_else(|e| e.into_inner()) = on;
}

/// Put debug mode into the state the caller asks for: the flag every
/// debug-gated log line reads, and — on `true` — every overlay force-shown
/// regardless of game focus.
///
/// Named for the flag rather than the overlays because the flag is the part
/// that binds on BOTH values of `on`; the force-show is what the `true` branch
/// additionally does. The old name (`force_show_overlays`) described a
/// one-directional side effect of an idempotent setter and read as a no-op on
/// the off press.
#[tauri::command]
fn set_debug_mode(app: AppHandle, on: bool) {
    let state = app.state::<AppState>();
    write_debug_mode(&state.debug_mode, on);
    if on {
        // Turning debug ON — show all overlays
        if let Some(win) = app.get_webview_window("lab") {
            if let Err(e) = win.show() {
                log::warn!("Failed to force-show lab: {}", e);
            }
        }
        // The temple overlay belongs here for the same reason the others do:
        // the focus poller hides it when the game loses focus, so without this
        // line debug mode could show every overlay except the one the user is
        // most likely debugging out of the game's foreground.
        if let Some(win) = app.get_webview_window("temple") {
            if let Err(e) = win.show() {
                log::warn!("Failed to force-show temple: {}", e);
            }
        }
        // Same reason as the temple: the focus poller hides the merc verdict
        // overlay with the game, so debug mode must force it up too.
        if let Some(win) = app.get_webview_window("mercenary") {
            if let Err(e) = win.show() {
                log::warn!("Failed to force-show mercenary: {}", e);
            }
        }
        log::info!("Debug mode ON — overlays force-shown");
    } else {
        log::info!("Debug mode OFF");
    }
}

#[tauri::command]
fn set_devtools(open: bool, window: tauri::WebviewWindow) {
    // Devtools has no JS API in Tauri 2 — calling `openDevtools()` on the
    // WebviewWindow object throws synchronously. This command is the only path.
    if open {
        window.open_devtools();
    } else {
        window.close_devtools();
    }
}

/// Tell the hook whether `label` is drawing anything.
///
/// Per window, not process-wide: an empty Lab comparator widget must pass its
/// clicks through while the merc strip is still claiming its own.
#[tauri::command]
fn set_overlay_has_content(label: String, has_content: bool) {
    #[cfg(windows)]
    overlay_hook::set_has_content(&label, has_content);

    #[cfg(not(windows))]
    let _ = (label, has_content);
}

/// Declare the window-relative PHYSICAL rectangles `label` claims clicks in.
///
/// Replaces the right-edge interactive width: a page measures the elements it
/// wants clickable and sends their rects, so nothing between them is taken from
/// the game. Sending an empty list withdraws the claim.
///
/// Takes the handle so the overlap report `overlay_hook::set_hot_rects` returns
/// reaches `app_log` — the in-app buffer and `app.log`. It cannot log itself:
/// `env_logger` is initialised with no filter and `RUST_LOG` is unset, so
/// anything below Error is dropped, and this is the diagnostic that explains a
/// click going to the window the user did not mean.
#[tauri::command]
fn set_overlay_hot_rects(label: String, rects: Vec<overlay_hook::HotRect>, app: AppHandle) {
    #[cfg(windows)]
    for line in overlay_hook::set_hot_rects(&label, rects) {
        app_log(&app, line);
    }

    #[cfg(not(windows))]
    let _ = (label, rects, app);
}

/// Put `label` in or out of widget-configuration mode.
///
/// On, the window becomes genuinely interactive so its page can handle drags
/// and Save/Cancel itself, and the hook leaves it alone — it neither repairs
/// `WS_EX_TRANSPARENT` (which would undo the interactivity within one mouse
/// event) nor intercepts its clicks. Off restores click-through, re-asserts
/// `WS_EX_NOACTIVATE`, and hands the window back to the hook.
///
/// The flag it sets is what `set_overlay_clickthrough` consults before
/// re-arming click-through.
#[tauri::command]
fn set_overlay_config_mode(label: String, on: bool, app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("Window '{}' not found", label))?;

    if on {
        // Flag first: between the two calls the hook must already be leaving
        // this window alone, or it can re-apply WS_EX_TRANSPARENT on the very
        // next mouse event and the window never becomes interactive.
        #[cfg(windows)]
        overlay_hook::set_config_mode(&label, true);

        if let Err(e) = window.set_ignore_cursor_events(false) {
            #[cfg(windows)]
            overlay_hook::set_config_mode(&label, false);
            let msg = format!("set_ignore_cursor_events(false) failed for '{}': {}", label, e);
            log::error!("{}", msg);
            return Err(msg);
        }
    } else {
        // The flag is cleared even when the call below FAILED. It used to be
        // left set, on the reasoning that a still-interactive window should not
        // be told otherwise — but the two outcomes are not symmetric. Leaving it
        // set strands a monitor-sized window that eats every click over the game
        // with no way back, because the hook skips a config-mode window and will
        // not repair it; clearing it costs at most a WS_EX_TRANSPARENT the hook
        // re-applies on the next mouse move. So the whole exit path runs and the
        // failure is reported to the caller afterwards.
        let failed = window.set_ignore_cursor_events(true).err();
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::HWND;
            match window.hwnd() {
                Ok(hwnd) => unsafe { overlay_hook::set_noactivate(HWND(hwnd.0 as *mut _)) },
                Err(e) => log::warn!(
                    "set_overlay_config_mode({label}): HWND unavailable, WS_EX_NOACTIVATE not re-applied: {e}"
                ),
            }
        }
        #[cfg(windows)]
        overlay_hook::set_config_mode(&label, false);
        if let Some(e) = failed {
            let msg = format!("set_ignore_cursor_events(true) failed for '{}': {}", label, e);
            log::error!("{}", msg);
            return Err(msg);
        }
    }

    Ok(())
}

/// Whether `label` is currently in widget-configuration mode.
///
/// The catch-up half of the ordering contract in `docs/OVERLAY-GUIDE.md`
/// ("Widget overlays"): Settings sets this flag BEFORE it emits
/// `widget-config`, so a window that Settings had to CREATE for the config
/// session — and which therefore had no listener when the event went out —
/// asks this on mount and finds out anyway. The event stays the fast path for a
/// window that was already open.
///
/// Always false off Windows: the flag lives in the mouse-hook registry, which
/// is a Windows structure, and a Linux dev build has no click-through to leave.
#[tauri::command]
fn get_overlay_config_mode(label: String) -> bool {
    // One `let` per platform rather than two block expressions: a cfg'd-out
    // trailing block would leave the Windows build with a `bool` block in
    // statement position and no tail expression.
    #[cfg(windows)]
    let on = overlay_hook::config_mode(&label);

    #[cfg(not(windows))]
    let on = {
        let _ = label;
        false
    };

    on
}

#[tauri::command]
fn request_trade_refresh(gem: String, variant: String, app: AppHandle) {
    if let Err(e) = app.emit("overlay-trade-refresh", serde_json::json!({ "name": gem, "variant": variant })) {
        log::warn!("emit overlay-trade-refresh failed: {}", e);
    }
}

#[tauri::command]
fn get_logs(state: tauri::State<AppState>) -> Vec<String> {
    state.logs.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Frontend can log messages to the app log (visible in Settings > Logs).
#[tauri::command]
fn app_log_from_frontend(msg: String, app: AppHandle) {
    app_log(&app, msg);
}

/// Focus poller result: where the foreground window belongs.
#[cfg(windows)]
pub(crate) enum FocusState {
    /// Path of Exile is the foreground window.
    Game,
    /// Our own process (overlay, main window) is foreground.
    OwnWindow,
    /// Some other application is foreground.
    Other,
}

fn spawn_log_watcher(app: AppHandle) {
    let state = app.state::<AppState>();
    let client_txt = state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let exists = std::path::Path::new(&client_txt).exists();
    app_log(&app, format!("Starting log watcher: {} (exists: {})", client_txt, exists));
    if !exists {
        app_log(&app, "WARNING: Client.txt not found — check path in Settings".to_string());
    }

    // Create cancel channel and store the sender
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    *state.watcher_cancel.lock().unwrap_or_else(|e| e.into_inner()) = Some(cancel_tx);

    tauri::async_runtime::spawn(async move {
        let watcher = log_watcher::LogWatcher::new(&client_txt);
        let mut rx = match watcher.watch().await {
            Ok(rx) => rx,
            Err(e) => {
                app_log(&app, format!("Log watcher failed to start: {}", e));
                emit_status(&app);
                return;
            }
        };

        app_log(&app, "Log watcher active".to_string());

        // One read of the catch-up tail, shared by the lab replay, the temple
        // seed and the loot attribution below.
        let catch_up_tail = client_log::recent_log_tail(std::path::Path::new(&client_txt));

        lab::catch_up(&app, std::path::Path::new(&client_txt), catch_up_tail.as_deref());

        // The temple trigger's own catch-up (POE-242), over the SAME 32 KB tail
        // the lab replay's first step reads. `replay_recent_tail` answers the
        // lab's question and returns nav events, so an app started INSIDE the
        // temple would otherwise sit disarmed for the whole run — no further
        // Alva line, no further area change.
        temple::trigger::catch_up(&app, catch_up_tail.as_deref());
        app.state::<AppState>().loot.boss.lock().unwrap_or_else(|e| e.into_inner()).catch_up(catch_up_tail.as_deref());

        emit_status(&app);

        // The merc OCR trigger's dialogue corpus (POE-280). Loaded once here
        // because this task is the only Client.txt reader in the app — the
        // trigger must not add a second tailer. The history is also owned by
        // this task, so every accepted speaker line is remembered before the
        // module and capture gates are checked.
        let merc_voice_lines = mercenary::trigger::MercVoiceLines::shipped();
        app_log(
            &app,
            format!(
                "Merc: loaded {} texts ({} inspect texts)",
                merc_voice_lines.len(),
                merc_voice_lines.inspect_len()
            ),
        );
        let mut merc_line_history = mercenary::trigger::MercSpeakerHistory::default();

        let mut lab_watch = lab::LabWatchState::new();
        let _matcher = gem_matcher::GemMatcher::new(vec![]); // TODO: fetch from server
        let mut line_count: u64 = 0;

        loop {
            tokio::select! {
                _ = cancel_rx.changed() => {
                    app_log(&app, "Log watcher cancelled (path changed)".to_string());
                    break;
                }
                line = rx.recv() => {
                    let line = match line {
                        Some(l) => l,
                        None => break,
                    };

                    line_count += 1;
                    // Log first line and then every 100th to confirm watcher is reading
                    if line_count == 1 {
                        app_log(&app, format!("Log watcher: first line received (len={})", line.len()));
                    } else if line_count % 100 == 0 {
                        app_log(&app, format!("Log watcher: {} lines processed", line_count));
                    }

                    // --- Aspirant's Trial / Plaza tracking (outside state machine) ---
                    lab::on_trial_line(&app, &line, &mut lab_watch);

                    // --- Merc OCR trigger (POE-198 / POE-280) ---
                    // Inspect text arms the OCR gate; the trigger owns the
                    // per-speaker history even when no gate is armed.
                    mercenary::trigger::on_client_line(
                        &app,
                        &line,
                        &merc_voice_lines,
                        &mut merc_line_history,
                    );

                    // --- Temple capture trigger (POE-242) ---
                    // Alva's voice lines and the temple's own area line are what
                    // arm the temple capture loop. Cheap by construction, and
                    // deliberately NOT gated on the module flag: the arm state
                    // is a fact about the game, so a module switched on inside a
                    // temple finds it already armed.
                    temple::trigger::on_client_line(&app, &line);

                    // --- Lab navigation events, then the lab state machine ---
                    lab::on_client_line(&app, &line, &mut lab_watch);

                    // Upload the previous zone's font session before observing
                    // the new area. Area detection also runs while OCR is idle.
                    if let Some(change) = client_log::parse_area_change(&line) {
                        let state = app.state::<AppState>();
                        state.loot.boss.lock().unwrap_or_else(|e| e.into_inner()).observe_area(&change);
                        lab::on_area_change(&app, &change);
                    }
                }
            }
        }

        app_log(&app, "Log watcher stopped".to_string());
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    let context = tauri::generate_context!();
    // Before the builder: WebView2 reads the variable when the first webview
    // creates its environment, and every later webview must share those args.
    let settings_path = settings::settings_path_before_app(&context.config().identifier);
    let webview_gpu_disabled = settings_path
        .as_deref()
        .is_some_and(settings::webview_gpu_disabled_in);
    // Also before the builder: the proxy goes into the WebView2 args and into
    // both reqwest clients below.
    let proxy_in_effect = settings::proxy_config_at(settings_path.as_deref());
    let prior = std::env::var(WEBVIEW2_ARGS_ENV).unwrap_or_default();
    if !prior.is_empty() {
        log::warn!("{} already set ({:?}); appending our flags", WEBVIEW2_ARGS_ENV, prior);
    }
    std::env::set_var(
        WEBVIEW2_ARGS_ENV,
        webview2_browser_args(&prior, webview_gpu_disabled, &proxy_in_effect.mode),
    );

    let pair_code = generate_pair_code();
    log::info!("Pair code: {}", pair_code);

    let device_id = fingerprint::compute_device_id();
    log::info!("Device ID: {}... ({}, {} salt)", &device_id[..device_id.len().min(8)],
        if device_id.len() == 64 { "hardware" } else { "volatile" },
        if fingerprint::USES_DEV_SALT { "dev" } else { "release" });

    // Build server HTTP client with default device headers.
    let version = env!("CARGO_PKG_VERSION");
    let mut default_headers = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(&device_id) {
        default_headers.insert("X-Device-ID", v);
    }
    if let Ok(v) = reqwest::header::HeaderValue::from_str(version) {
        default_headers.insert("X-App-Version", v);
    }
    let server_http = http::client_builder(&proxy_in_effect, http::Transport::Other)
        .default_headers(default_headers)
        .build()
        .expect("failed to build server HTTP client");

    let app_state = AppState {
        loot: loot_capture::LootState::new(),
        device_id: device_id.clone(),
        pair_code: Mutex::new(pair_code),
        client_txt_path: Mutex::new(String::from(
            r"C:\Program Files (x86)\Grinding Gear Games\Path of Exile\logs\Client.txt",
        )),
        server_url: Mutex::new(String::from(settings::DEFAULT_SERVER_URL)),
        lab_scan: lab_scan::LabScanState {
            detection: Mutex::new(lab_detection::LabDetection::default()),
            detected_gems: Mutex::new(Vec::new()),
            gem_scan_generation: AtomicU64::new(0),
            font_scan_generation: AtomicU64::new(0),
            font_scan_live_gen: AtomicU64::new(0),
            lab_scan_live_gen: AtomicU64::new(0),
            lab_scan_generation: AtomicU64::new(0),
            font_opened_seq: AtomicU64::new(0),
            font_session: Mutex::new(FontSessionData::default()),
            font_alert_seen: Mutex::new(font_alerts::AlertSeen::default()),
        },
        lab_state: Mutex::new(lab_state::LabState::Idle),
        logs: Mutex::new(Vec::new()),
        sidebar_open: Mutex::new(true),
        game_focused: Mutex::new(false),
        trade_client: trade::TradeApiClient::new(&proxy_in_effect),
        server_http,
        proxy_in_effect: proxy_in_effect.clone(),
        watcher_cancel: Mutex::new(None),
        comparator_data: Mutex::new(serde_json::json!({"results":[],"tradeData":{}})),
        overlay_hook_stop: Mutex::new(None),
        focus_poller_stop: Mutex::new(None),
        debug_mode: Mutex::new(false),
        gem_band_dump: std::sync::atomic::AtomicBool::new(false),
        trade_settings: Mutex::new(settings::TradeSettings::default()),
        aspirant_trial_count: AtomicU32::new(0),
        in_lab: AtomicBool::new(false),
        game_in_foreground: AtomicBool::new(false),
        compass_settings: Mutex::new(settings::CompassSettings::default()),
        lab_overlays_enabled: Mutex::new(true),
        lab_settings: Mutex::new(settings::LabSettings::default()),
        ui_prefs: Mutex::new(std::collections::HashMap::new()),
        ssot: Mutex::new(ssot::AppSsotSnapshot::default()),
        modules_enabled: Mutex::new(std::collections::HashMap::new()),
        module_handles: Mutex::new(std::collections::HashMap::new()),
        modules_shutting_down: AtomicBool::new(false),
        mercenary: Mutex::new(mercenary::MercenarySlice::default()),
        merc: mercenary::MercState::new(),
        temple: Mutex::new(temple::slice::TempleSlice::default()),
        temple_state: temple::TempleState::new(),
        exceptional_prices: Mutex::new(font_alerts::PriceMirror::default()),
        screen: Mutex::new(None),
        game_monitor: Mutex::new(None),
        widgets: Mutex::new(std::collections::BTreeMap::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_status,
            ssot::get_ssot,
            ssot::set_league,
            ssot::league_command::note_server_league,
            ssot::refresh_league,
            ssot::geometry_recalibrate,
            capture::get_game_monitor,
            loot_capture::loot_capture_view,
            loot_capture::boss_loot_profiles,
            loot_capture::boss_loot_set_settings,
            loot_capture::set_number_format,
            loot_capture::boss_loot_action,
            loot_capture::boss_loot_submit,
            loot_capture::boss_loot_rebind,
            loot_api::boss_loot_stats,
            loot_api::boss_loot_runs,
            loot_api::boss_loot_edit,
            loot_api::boss_loot_delete,
            loot_api::boss_loot_export,
            modules::set_module_enabled,
            get_pair_code,
            get_device_id,
            regenerate_pair_code,
            set_client_txt_path,
            reset_client_txt_path,
            reset_all_settings,
            browse_client_txt,
            lab::commands::emit_lab_nav,
            app_log_from_frontend,
            set_server_url,
            settings::commands::set_sidebar_open,
            settings::commands::set_trade_staleness_settings,
            settings::commands::set_auto_trade,
            get_logs,
            get_ocr_rects,
            capture_mouse_position,
            lab::commands::start_scanning,
            lab::commands::stop_scanning,
            trade_lookup,
            trade_cancel,
            lab::commands::send_test_gems,
            lab::commands::test_ocr_on_image,
            mercenary::debug::merc_debug_capture,
            mercenary::debug::merc_scan_now,
            mercenary::debug::merc_forget_template,
            mercenary::debug::merc_forget_seed,
            mercenary::debug::merc_reset_templates,
            mercenary::sources::merc_set_sources_off,
            mercenary::search::merc_set_trade_auto,
            mercenary::search::merc_set_divine_rate,
            mercenary::search::merc_set_tier_floor,
            temple::commands::temple_set_config,
            temple::commands::temple_set_profile,
            temple::commands::temple_set_preset,
            temple::commands::temple_set_custom,
            temple::commands::temple_value_table,
            temple::commands::temple_rearm,
            temple::commands::temple_debug_capture,
            set_debug_mode,
            set_gem_band_dump,
            get_gem_band_dump,
            set_devtools,
            lab::commands::set_comparator_data,
            set_overlay_has_content,
            set_overlay_hot_rects,
            set_overlay_config_mode,
            get_overlay_config_mode,
            lab::commands::get_comparator_data,
            font_alerts::get_font_alerts,
            font_alerts::get_exceptional_support_prices,
            set_overlay_clickthrough,
            request_trade_refresh,
            settings::commands::get_widget_geometries,
            settings::commands::set_widget_geometry,
            settings::commands::get_timer_appearance,
            settings::commands::get_webview_gpu_disabled,
            settings::commands::set_webview_gpu_disabled,
            settings::commands::get_proxy_settings,
            settings::commands::set_proxy_url,
            settings::commands::set_proxy_trade,
            settings::commands::set_timer_appearance,
            lab::commands::get_lab_overlays_enabled,
            lab::commands::set_lab_overlays_enabled,
            lab::commands::get_compass_settings,
            lab::commands::get_lab_catchup,
            lab::commands::set_compass_mode,
            lab::commands::set_compass_strategy,
            lab::commands::set_compass_difficulty,
            lab::commands::set_shrine_warn,
            lab::commands::get_lab_mode,
            lab::commands::set_lab_mode,
            lab::commands::get_autoclear_minutes,
            lab::commands::set_autoclear_minutes,
            lab::commands::get_dedication_pool,
            lab::commands::set_dedication_pool,
            lab::commands::get_dedication_variant,
            lab::commands::set_dedication_variant,
            lab::commands::get_normal_variant,
            lab::commands::set_normal_variant,
            lab::commands::get_show_low_confidence,
            lab::commands::set_show_low_confidence,
            settings::commands::get_ui_prefs,
            settings::commands::set_ui_pref,
            lab::commands::discard_font_session,
            updater_channel::check_update_from_endpoint,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            // Load persisted settings and apply to state.
            // If no settings file exists, write defaults so the file is always present.
            let saved = settings::load(&handle);
            let state = handle.state::<AppState>();
            // A value this build refuses falls back rather than failing the
            // load, so the file and the running value disagree from here on.
            // Logging is the only place that difference is visible.
            for rejection in settings::apply_to_state(&saved, &state) {
                app_log(&handle, rejection);
            }
            if webview_gpu_disabled {
                app_log(&handle, "WebView2 started with --disable-gpu (GPU-free rendering setting)".into());
            }
            if let http::ProxyMode::Via(proxy) = &proxy_in_effect.mode {
                let trade = if proxy_in_effect.trade { "" } else { " (trade requests direct)" };
                app_log(&handle, format!("Proxy in effect: {}{trade}", proxy.as_str()));
            }
            if let Some(reason) = proxy_in_effect.blocked_reason() {
                app_log(&handle, format!("Proxy blocked, every connection refused: {reason}"));
            }

            // If saved Client.txt path doesn't exist, re-detect
            {
                let current_path = state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if !std::path::Path::new(&current_path).exists() {
                    let detected = detect_client_txt_path();
                    app_log(&handle, format!("Saved Client.txt not found ({}), re-detected: {}", current_path, detected));
                    *state.client_txt_path.lock().unwrap_or_else(|e| e.into_inner()) = detected;
                }
            }

            // Write settings on startup so the file always exists
            // Use persist_settings to preserve window + overlay settings
            persist_settings(&handle);
            app_log(&handle, "Settings initialized".to_string());

            // Restore window position/size from saved settings.
            if let Some(ref win_settings) = saved.window {
                if let Some(win) = app.get_webview_window("main") {
                    let visible = win.available_monitors()
                        .map(|monitors| {
                            monitors.iter().any(|m| {
                                let pos = m.position();
                                let size = m.size();
                                let mx = pos.x as i32;
                                let my = pos.y as i32;
                                let mw = size.width as i32;
                                let mh = size.height as i32;
                                win_settings.x < mx + mw && win_settings.x + 100 > mx
                                    && win_settings.y < my + mh && win_settings.y + 50 > my
                            })
                        })
                        .unwrap_or(true);

                    if visible {
                        let _ = win.set_position(tauri::PhysicalPosition::new(win_settings.x, win_settings.y));
                    } else {
                        log::info!("Saved window position ({}, {}) is off-screen, centering", win_settings.x, win_settings.y);
                        let _ = win.center();
                    }
                    let _ = win.set_size(tauri::PhysicalSize::new(win_settings.width, win_settings.height));
                    if win_settings.maximized {
                        let _ = win.maximize();
                    }
                }
            }

            spawn_log_watcher(handle.clone());
            focus::spawn_focus_poller(handle.clone());
            // Resolve the active league from the server (start-only, bounded
            // retry). Until it succeeds the SSOT stays unresolved and every
            // trade lookup fails closed — by design (POE-128 chunk 3).
            ssot::spawn_league_fetch(handle.clone());
            // The temple market poll (POE-258). Unconditional like the two
            // spawns above and independent of the temple module's toggle: the
            // Temple page states the age of the prices its board was valued at
            // whether or not the capture loop is running, and the poll's first
            // attempt waits on the league fetch above rather than on a timer.
            temple::market::spawn_temple_market_poll(handle.clone());
            font_alerts::spawn_exceptional_price_poll(handle.clone());
            emit_status(&handle);
            emit_logs(&handle);
            // Modules start LAST: the owner map is effective by now
            // (`apply_to_state` above) and settings are on disk, and module
            // spawn must never delay or reorder the unconditional spawns
            // above. The `ssot-changed` nudge is best-effort here — no window
            // is listening yet, and overlays poll `get_ssot` anyway.
            modules::apply_reconcile(&handle);
            ssot::emit_ssot(&handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // The mouse hook hit-tests clicks against a cached window rect and
            // derives overlay-relative coordinates from it. Anything that moves
            // the overlay outside this event path — a DPI or
            // resolution change, a monitor switch, the game toggling
            // fullscreen — would otherwise leave that rect stale for the rest
            // of the session, shifting both the interactive zone and every
            // click coordinate (POE-148).
            #[cfg(windows)]
            if matches!(
                event,
                tauri::WindowEvent::Moved(_)
                    | tauri::WindowEvent::Resized(_)
                    | tauri::WindowEvent::ScaleFactorChanged { .. }
            ) {
                // By label, for ANY registered overlay. The singleton this
                // replaced keyed on the HWND because it tracked exactly one
                // window and the label could not tell it which; the registry
                // holds the label already, so this also saves a `hwnd()` call
                // per window event.
                overlay_hook::invalidate_label(window.label());
            }
            // Drop a destroyed overlay from the hook's registry, and tear the
            // hook down when that was the last one. Keyed to the comparator
            // before the registry existed — now any hooked overlay can be the
            // one that closes, and the hook has to outlive the others.
            if let tauri::WindowEvent::Destroyed = event {
                #[cfg(windows)]
                if overlay_hook::unregister(window.label()) {
                    let app = window.app_handle();
                    let state = app.state::<AppState>();
                    // Send stop signal — the hook thread will unhook and exit
                    if let Some(tx) = state.overlay_hook_stop.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(());
                    }
                    log::info!(
                        "overlay_hook: '{}' was the last hooked overlay — hook torn down",
                        window.label()
                    );
                }
            }
            // Save window position/size on close
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if window.label() == "main" {
                    let app = window.app_handle();
                    let state = app.state::<AppState>();

                    // Latch shutdown BEFORE any stop, so a `set_module_enabled`
                    // racing the close can only stop, never respawn.
                    state.modules_shutting_down.store(true, Ordering::SeqCst);

                    // Stop all background threads before exit.
                    // Registered modules — stops everything running, including
                    // NoWindow modules the enabled flag cannot touch.
                    modules::apply_reconcile(app);
                    // Mouse hook
                    if let Some(tx) = state.overlay_hook_stop.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(());
                    }
                    // Focus poller
                    if let Some(tx) = state.focus_poller_stop.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(());
                    }
                    // Log watcher
                    if let Some(tx) = state.watcher_cancel.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(true);
                    }
                    // Gem/font scan loops — bump generations so they exit
                    state.lab_scan.gem_scan_generation.fetch_add(1, Ordering::SeqCst);
                    state.lab_scan.font_scan_generation.fetch_add(1, Ordering::SeqCst);
                    state.lab_scan.font_scan_live_gen.store(0, Ordering::SeqCst);

                    let is_maximized = window.is_maximized().unwrap_or(false);
                    // Only save position/size if not maximized (restore to normal position)
                    let win_settings = if is_maximized {
                        // Save maximized flag but keep last known normal position from file.
                        // AppState doesn't store window settings, so from_state returns None.
                        let existing = settings::load(app);
                        settings::WindowSettings {
                            x: existing.window.as_ref().map_or(100, |w| w.x),
                            y: existing.window.as_ref().map_or(100, |w| w.y),
                            width: existing.window.as_ref().map_or(1024, |w| w.width),
                            height: existing.window.as_ref().map_or(768, |w| w.height),
                            maximized: true,
                        }
                    } else {
                        let pos = window.outer_position().unwrap_or(tauri::PhysicalPosition::new(100, 100));
                        let size = window.outer_size().unwrap_or(tauri::PhysicalSize::new(1024, 768));
                        settings::WindowSettings {
                            x: pos.x,
                            y: pos.y,
                            width: size.width,
                            height: size.height,
                            maximized: false,
                        }
                    };
                    settings::update(app, |existing| {
                        let state = app.state::<AppState>();
                        settings::close_save_settings(existing, settings::from_state(&state), win_settings)
                    });

                    // Force exit — background threads (focus poller, mouse hook, scan loops)
                    // may still be mid-sleep and keeping the process alive.
                    std::process::exit(0);
                }
            }
        })
        .run(context)
        .expect("error while running tauri application");
}

const WEBVIEW2_ARGS_ENV: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";

/// The WebView2 browser arguments this process starts with.
///
/// Always keeps the renderer alive when the window is backgrounded — PoE
/// alt-tab steals focus and Chromium's default backgrounding pauses timers and
/// drops the SSE socket. `gpu_disabled` adds `--disable-gpu`
/// (`Settings::webview_gpu_disabled`); `proxy` adds the proxy server and its
/// local bypass list (`Settings::proxy_url`) or, when the saved proxy is
/// unusable, a dead proxy that blocks every webview request. A value the outer shell already set is
/// kept first: the variable is additive, and operators may set their own flags
/// for debugging or workarounds.
fn webview2_browser_args(prior: &str, gpu_disabled: bool, proxy: &http::ProxyMode) -> String {
    let mut args = String::from(prior.trim());
    let mut push = |flag: &str| {
        if !args.is_empty() {
            args.push(' ');
        }
        args.push_str(flag);
    };
    push("--disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows");
    if gpu_disabled {
        push("--disable-gpu");
    }
    if let Some(flags) = http::webview2_proxy_args(proxy) {
        push(&flags);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::{
        body_excerpt, webview2_browser_args, clickthrough_failure_log_level, clickthrough_outcome, crop_shortfall, assemble_ocr_rects,
        ocr_warning_field,
        retry_after_delay, temple_rects_from_rois, write_debug_mode, write_server_url, CaptureRegion,
        ClickthroughSetup,
        CLICKTHROUGH_WINDOW_GONE,
    };
    use std::sync::Mutex;
    use std::time::Duration;

    /// A user's own WebView2 flags stay first and intact; the GPU flag is
    /// appended only when the setting asks for it.
    #[test]
    fn webview2_browser_args_keeps_prior_flags_and_adds_gpu_flag_only_when_set() {
        let keepalive = "--disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows";
        assert_eq!(webview2_browser_args("", false, &crate::http::ProxyMode::Direct), keepalive);
        assert_eq!(webview2_browser_args("", true, &crate::http::ProxyMode::Direct), format!("{} --disable-gpu", keepalive));
        assert_eq!(
            webview2_browser_args("--remote-debugging-port=9222 ", true, &crate::http::ProxyMode::Direct),
            format!("--remote-debugging-port=9222 {} --disable-gpu", keepalive)
        );
        assert_eq!(
            webview2_browser_args("--remote-debugging-port=9222", false, &crate::http::ProxyMode::Direct),
            format!("--remote-debugging-port=9222 {}", keepalive)
        );
    }

    /// The proxy flags come last and only when a proxy is in effect; the
    /// keep-alive and GPU flags are unchanged by them.
    #[test]
    fn webview2_browser_args_appends_proxy_flags_only_when_set() {
        let keepalive = "--disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows";
        let proxy = crate::http::ProxyUrl::parse("socks5://127.0.0.1:1080").unwrap().unwrap();
        assert_eq!(
            webview2_browser_args("", true, &crate::http::ProxyMode::Via(proxy)),
            format!(
                "{} --disable-gpu --proxy-server=socks5://127.0.0.1:1080 --proxy-bypass-list=localhost;127.0.0.1;[::1];*.localhost",
                keepalive
            )
        );
        assert!(!webview2_browser_args("", true, &crate::http::ProxyMode::Direct).contains("--proxy"));
    }

    #[test]
    fn webview2_browser_args_appends_the_blocked_flags_when_blocked() {
        let keepalive = "--disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows";
        assert_eq!(
            webview2_browser_args("", false, &crate::http::ProxyMode::blocked("R")),
            format!("{keepalive} --proxy-server=http://127.0.0.1:1 --proxy-bypass-list=localhost;127.0.0.1;[::1];*.localhost --host-resolver-rules=\"MAP * ~NOTFOUND , EXCLUDE localhost , EXCLUDE *.localhost\"")
        );
    }

    /// The bug the `on` argument exists for. The command was an
    /// argument-less toggle, and the Ctrl+Shift+F12 handler invoked it only on
    /// the press that turned debug mode ON — so the second such press found
    /// the flag already true and turned it off, silencing every debug-gated
    /// log line while the UI said debug mode was on (2026-08-26 smoke).
    /// Setting the state asked for is idempotent; flipping it is not.
    #[test]
    fn asking_for_debug_mode_twice_leaves_it_on() {
        let flag = Mutex::new(false);

        write_debug_mode(&flag, true);
        write_debug_mode(&flag, true);

        assert!(*flag.lock().expect("an unpoisoned flag"));
    }

    /// Pointing the app at a different server is a change, and the url in force
    /// is the new one.
    ///
    /// The bool is what `set_server_url` gates the market drop and the league
    /// re-resolve on, so a writer that reported no change would leave the board
    /// on the old server's prices until the next five-minute poll.
    #[test]
    fn a_new_server_url_is_a_change() {
        let held = Mutex::new(String::from("https://profitofexile.top"));

        let changed = write_server_url(&held, String::from("http://localhost:8080"));

        assert!(changed);
        assert_eq!(*held.lock().expect("an unpoisoned url"), "http://localhost:8080");
    }

    /// Re-saving the url already in force is NOT a change.
    ///
    /// Settings' Save on an untouched field, or a DEBUG/PROD click on the side
    /// the app is already on: nothing on screen came from a server the app has
    /// left, so the reset must not fire — it would blank a good market read and
    /// restart league resolution for nothing. Fails if the equality guard goes.
    #[test]
    fn re_saving_the_same_server_url_is_not_a_change() {
        let held = Mutex::new(String::from("https://profitofexile.top"));

        let changed = write_server_url(&held, String::from("https://profitofexile.top"));

        assert!(!changed);
    }

    // --- the click-through setup's outcome ----------------------------------
    //
    // Scope, precisely: what is pinned here is `clickthrough_outcome`, the
    // mapping from an observation to the answer the caller acts on. It does not
    // cover what PRODUCES those observations (`clickthrough_setup` needs a real
    // WebView2 HWND), nor the wiring that carries them (the `spawn_blocking` the
    // command awaits) — `make desktop-check-windows` type-checks both, and their
    // behaviour is a Windows smoke check (`docs/OVERLAY-GUIDE.md`).

    #[test]
    fn a_window_that_armed_reports_success() {
        assert_eq!(clickthrough_outcome("temple", ClickthroughSetup::Armed), Ok(()));
    }

    /// The reason the command stopped being fire-and-forget. A refused
    /// `set_ignore_cursor_events` used to be logged and swallowed, so a
    /// monitor-sized window that never became click-through looked exactly like
    /// one that did — and swallowed every click on the screen until it was
    /// destroyed.
    #[test]
    fn a_refused_ignore_cursor_call_fails_the_setup() {
        let out = clickthrough_outcome(
            "temple",
            ClickthroughSetup::IgnoreCursorFailed("no window handle".into()),
        );

        assert_eq!(
            out,
            Err("set_ignore_cursor_events failed for 'temple': no window handle".to_string())
        );
    }

    /// The belt. Tauri answering `Ok` is not proof the extended style took, and
    /// the hook only repairs a window the cursor is already over — so a style
    /// that did not read back is reported, not left to be discovered by the
    /// player's next click.
    #[test]
    fn a_style_that_did_not_read_back_fails_the_setup() {
        assert!(clickthrough_outcome("temple", ClickthroughSetup::NotTransparent)
            .is_err_and(|e| e.contains("WS_EX_TRANSPARENT")));
    }

    /// No HWND means the window was never registered, so the hook cannot repair
    /// its style later either. Reported rather than warned: the caller can
    /// still destroy and retry, and the old warn-and-return could not.
    #[test]
    fn a_window_with_no_hwnd_fails_the_setup() {
        assert!(clickthrough_outcome("temple", ClickthroughSetup::HwndUnavailable).is_err());
    }

    /// A label that vanished during the 1 s wait — a fast module off→on — is a
    /// failed creation, not a quiet success. Reporting success here would have
    /// `module-lifecycle.ts` record a window that does not exist and refuse to
    /// build one until the module is toggled again.
    #[test]
    fn a_window_gone_by_the_time_setup_ran_fails_the_setup() {
        assert!(clickthrough_outcome("temple", ClickthroughSetup::WindowGone).is_err());
    }

    /// Half of a cross-language pair: `clickthroughReport` in
    /// `src/lib/overlay/clickthrough-report.ts` matches this prefix to tell the
    /// ordinary failure (an overlay toggled off inside the setup wait) from a
    /// window that is live and swallowing the player's clicks.
    ///
    /// The LITERAL is asserted, not the constant. Asserting
    /// `starts_with(CLICKTHROUGH_WINDOW_GONE)` alone would survive a rename on
    /// this side — both halves of the comparison move together — while the
    /// TypeScript half kept the old string and silently downgraded every genuine
    /// warning to an info line. Renaming the marker must fail here AND in
    /// `clickthrough-report.test.ts`, which asserts the same literal.
    #[test]
    fn a_vanished_window_is_reported_with_the_marker_the_caller_matches() {
        let out = clickthrough_outcome("temple", ClickthroughSetup::WindowGone);

        assert_eq!(CLICKTHROUGH_WINDOW_GONE, "window-gone");
        assert!(out.is_err_and(|e| e.starts_with("window-gone")));
    }

    #[test]
    fn a_vanished_window_is_logged_at_info() {
        let message = clickthrough_outcome("temple", ClickthroughSetup::WindowGone)
            .expect_err("a vanished window fails setup");

        assert_eq!(clickthrough_failure_log_level(&message), log::Level::Info);
    }

    #[test]
    fn a_live_window_setup_failure_is_logged_at_error() {
        for setup in [
            ClickthroughSetup::NotTransparent,
            ClickthroughSetup::HwndUnavailable,
            ClickthroughSetup::IgnoreCursorFailed("no event loop".into()),
        ] {
            let message = clickthrough_outcome("temple", setup)
                .expect_err("a live window failed setup");

            assert_eq!(clickthrough_failure_log_level(&message), log::Level::Error, "{message}");
        }
    }

    /// The other four must NOT carry it, or a live window eating clicks would be
    /// reported as an ordinary toggle-off and the warning would never be seen.
    #[test]
    fn a_window_that_is_still_there_is_not_reported_as_gone() {
        for setup in [
            ClickthroughSetup::NotTransparent,
            ClickthroughSetup::HwndUnavailable,
            ClickthroughSetup::IgnoreCursorFailed("no event loop".into()),
        ] {
            let out = clickthrough_outcome("temple", setup);

            assert!(out.is_err_and(|e| !e.starts_with(CLICKTHROUGH_WINDOW_GONE)));
        }
    }

    #[test]
    fn asking_for_debug_mode_off_turns_it_off() {
        let flag = Mutex::new(true);

        write_debug_mode(&flag, false);

        assert!(!*flag.lock().expect("an unpoisoned flag"));
    }

    /// A panic anywhere else that touched this flag must not cost the user
    /// their debug logging — every other reader of `debug_mode` recovers the
    /// poison rather than propagating it, and the writer has to agree.
    #[test]
    fn a_poisoned_flag_is_still_written() {
        let flag = Mutex::new(false);
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = flag.lock().expect("the first lock succeeds");
            panic!("a reader panicked while holding the flag");
        }));
        assert!(poisoned.is_err(), "the flag must actually be poisoned");

        write_debug_mode(&flag, true);

        assert!(*flag.lock().unwrap_or_else(|e| e.into_inner()));
    }

    #[test]
    fn status_reports_the_blocked_proxy_reason() {
        let mut state = crate::settings::tests::test_app_state();
        state.proxy_in_effect = crate::http::ProxyConfig { mode: crate::http::ProxyMode::blocked("R"), trade: true };
        assert_eq!(
            super::build_status(&state).proxy_error,
            Some("R. Nothing is sent until you fix or clear the proxy in Settings and restart the app.".to_string())
        );
    }

    #[test]
    fn status_has_no_proxy_error_without_a_blocked_proxy() {
        assert_eq!(super::build_status(&crate::settings::tests::test_app_state()).proxy_error, None);
    }


    #[test]
    fn the_status_ocr_warning_field_reports_the_cached_warning() {
        // The status payload's only reachable seam: `build_status` needs a live
        // AppState, so this pins the field's SOURCE — a warning cached by the
        // engine path is what the payload carries, not a hardcoded None.
        //
        // Asserts against the cache rather than a literal because the cache is
        // process-wide and never cleared: another test in this binary may have
        // recorded first, and this must hold whichever write won.
        crate::ocr::record_language_warning_globally(
            "en-US unavailable — fell back to the Windows profile language",
        );
        let cached = crate::ocr::language_warning();
        assert!(cached.is_some(), "recording must leave a warning cached");
        assert_eq!(ocr_warning_field(), cached);
    }

    #[test]
    fn a_short_body_is_logged_whole() {
        assert_eq!(body_excerpt("gem is required", 200), "gem is required");
    }

    #[test]
    fn a_long_body_is_cut_to_the_limit() {
        let body = "x".repeat(500);
        assert_eq!(body_excerpt(&body, 200), "x".repeat(200));
    }

    #[test]
    fn a_cut_landing_mid_codepoint_backs_up_to_a_boundary() {
        // "é" is two bytes, so a limit of 5 lands inside the third one. Slicing
        // there panics, and panicking while logging a rejection loses it.
        assert_eq!(body_excerpt("ééé", 5), "éé");
    }

    #[test]
    fn retry_after_delay_honours_delta_seconds() {
        assert_eq!(retry_after_delay(Some("2")), Duration::from_secs(2));
    }

    #[test]
    fn retry_after_delay_clamps_a_long_wait_to_the_ceiling() {
        assert_eq!(retry_after_delay(Some("120")), Duration::from_secs(5));
    }

    #[test]
    fn retry_after_delay_falls_back_when_the_header_is_absent() {
        assert_eq!(retry_after_delay(None), Duration::from_secs(1));
    }

    #[test]
    fn retry_after_delay_falls_back_on_the_http_date_form() {
        assert_eq!(
            retry_after_delay(Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            Duration::from_secs(1),
        );
    }

    // --- derived lab placements and read-only preview assembly (POE-268) ---

    /// `image`'s `crop_imm` clamps rather than failing, so a region that hangs
    /// off the bottom of the capture yields a SHORTER crop and the OCR quietly
    /// reads a partial panel. The shortfall line is the only breadcrumb, so it
    /// has to carry both sizes — "something was wrong with the region" is not
    /// actionable, "asked for 550x75, got 550x40" is.
    #[test]
    fn a_crop_shorter_than_the_region_reports_both_sizes() {
        let line = crop_shortfall(&CaptureRegion { x: 30, y: 45, w: 550, h: 75 }, 550, 40)
            .expect("a short crop must be reported");

        // The phrases, not the bare sizes: "asked for" and "got" are what make
        // the line readable, and a swapped argument pair would satisfy a
        // `contains("550x75")` while telling the reader the exact opposite.
        assert!(line.contains("asked for 550x75"), "the requested size is missing from: {line}");
        assert!(line.contains("got 550x40"), "the actual size is missing from: {line}");
    }

    /// The other axis, because the two bounds are clamped independently: a
    /// region pushed off the RIGHT edge of the capture loses width alone.
    #[test]
    fn a_crop_narrower_than_the_region_is_reported() {
        assert!(crop_shortfall(&CaptureRegion { x: 30, y: 45, w: 550, h: 75 }, 480, 75).is_some());
    }

    /// The common case: the region fits, so the loop logs nothing. A check that
    /// fired on a good crop would put a line in every scan session's log and
    /// train the reader to ignore it.
    #[test]
    fn a_crop_of_the_requested_size_is_not_reported() {
        assert_eq!(crop_shortfall(&CaptureRegion { x: 30, y: 45, w: 550, h: 75 }, 550, 75), None);
    }

    // --- read-only OCR region preview assembly (POE-268) -------------------

    fn measured_screen(
        width: u32,
        height: u32,
        ui_scale: f32,
        anchors: Option<crate::ssot::Anchors>,
    ) -> crate::ssot::ScreenSlice {
        crate::ssot::ScreenSlice {
            width,
            height,
            ui_scale,
            source: crate::ssot::ScreenScaleSource::MercFrame,
            measured_at_ms: 1_700_000_000_000,
            verified_this_session: true,
            monitor_id: 1,
            origin: (0, 0),
            client: [0, 0, width as i32, height as i32],
            anchors,
        }
    }

    #[test]
    fn temple_preview_rects_use_published_rois_at_known_origin_and_scale() {
        let screen = measured_screen(1920, 1080, 0.9, None);
        let placements = crate::ssot::placements(&screen);
        let origin = placements.temple.expect("a measured screen has a temple seed").entrance_origin;
        let rois = crate::temple::run::read_rois(
            origin,
            crate::temple::anchor::scale_for_ui_scale(screen.ui_scale),
        );
        let temple_rects = Some(temple_rects_from_rois(&rois));
        let rects = assemble_ocr_rects(&placements, Some(&screen), temple_rects);

        let panel = rects
            .iter()
            .find(|rect| rect.key == "temple.panel")
            .expect("temple panel preview row must exist");
        let remaining = rects
            .iter()
            .find(|rect| rect.key == "temple.remaining")
            .expect("temple remaining preview row must exist");

        assert_eq!(panel.rect, Some([1131, 4, 544, 454]));
        assert_eq!(remaining.rect, Some([810, 771, 300, 46]));
        assert_eq!(panel.source, "seed");
        assert_eq!(remaining.source, "seed");
    }

    #[test]
    fn lab_preview_rects_use_derived_regions_and_report_seed_sources() {
        let screen = measured_screen(1920, 1080, 0.9, None);
        let placements = crate::ssot::placements(&screen);
        let rects = assemble_ocr_rects(&placements, Some(&screen), None);
        let gem_view = rects.iter().find(|rect| rect.key == "lab.gem").unwrap();
        let font_view = rects.iter().find(|rect| rect.key == "lab.font").unwrap();
        assert_eq!(gem_view.rect, Some([0, 0, 1920, 40]));
        assert_eq!(font_view.rect, Some([298, 220, 666, 681]));
        assert_eq!(gem_view.source, "seed");
        assert_eq!(font_view.source, "seed");
    }

    #[test]
    fn a_remembered_temple_anchor_drives_preview_origin_and_source() {
        let anchors = Some(crate::ssot::Anchors {
            temple_entrance: Some([111, 222]),
            merc_panel: None,
        });
        let screen = measured_screen(1920, 1200, 1.0, anchors);
        let placements = crate::ssot::placements(&screen);
        let origin = placements.temple.expect("the temple placement is located").entrance_origin;
        let rois = crate::temple::run::read_rois(origin, 1.0);
        let rects = assemble_ocr_rects(
            &placements,
            Some(&screen),
            Some(temple_rects_from_rois(&rois)),
        );

        for key in ["temple.panel", "temple.remaining"] {
            let view = rects.iter().find(|rect| rect.key == key).unwrap();
            assert_eq!(view.source, "remembered");
        }
    }

    #[test]
    fn a_remembered_merc_anchor_drives_preview_crop_and_source() {
        let anchors = Some(crate::ssot::Anchors {
            temple_entrance: None,
            merc_panel: Some([120, 220]),
        });
        let screen = measured_screen(1920, 1200, 1.0, anchors);
        let placements = crate::ssot::placements(&screen);
        let rects = assemble_ocr_rects(&placements, Some(&screen), None);

        let view = rects.iter().find(|rect| rect.key == "merc.panel").unwrap();
        let merc = placements.merc.expect("merc placement is located");
        assert_eq!(merc.panel, [120, 220, 555, 477]);
        // The anchor sits nearer the edges than the padding is wide, so the crop
        // clamps to x=0, y=0.
        assert_eq!(view.rect, Some([0, 0, 872, 697]));
        assert_eq!(view.source, "remembered");
    }

    #[test]
    fn a_seed_merc_placement_has_a_rect_and_seed_source() {
        let screen = measured_screen(1920, 1200, 1.0, None);
        let placements = crate::ssot::placements(&screen);
        let rects = assemble_ocr_rects(&placements, Some(&screen), None);

        let view = rects.iter().find(|rect| rect.key == "merc.panel").unwrap();
        assert_eq!(view.rect, Some([501, 0, 949, 1092]));
        assert_eq!(view.source, "seed");
    }

    #[test]
    fn the_merc_preview_row_is_labelled_merc_ocr_crop() {
        let screen = measured_screen(1920, 1080, 0.9, None);
        let placements = crate::ssot::placements(&screen);
        let rects = assemble_ocr_rects(&placements, Some(&screen), None);

        let view = rects.iter().find(|rect| rect.key == "merc.panel").unwrap();
        assert_eq!(view.label, "Merc OCR crop");
    }

    #[test]
    fn the_merc_preview_draws_the_padded_ocr_crop_for_the_snapshot_screen() {
        let screen = measured_screen(1920, 1080, 0.9, None);
        let placements = crate::ssot::placements(&screen);
        let rects = assemble_ocr_rects(&placements, Some(&screen), None);

        let view = rects.iter().find(|rect| rect.key == "merc.panel").unwrap();
        let merc = placements.merc.expect("merc placement is located");
        let expected = crate::mercenary::geometry::placed_panel_crop(
            merc.panel,
            screen.ui_scale,
            [screen.width, screen.height],
        );

        assert_eq!(view.rect, Some(expected));
    }

    #[test]
    fn the_merc_preview_publishes_vertical_seed_row_bands_in_capture_pixels() {
        let screen = measured_screen(1920, 1200, 1.0, None);
        let placements = crate::ssot::placements(&screen);
        let rects = assemble_ocr_rects(&placements, Some(&screen), None);
        let view = rects.iter().find(|rect| rect.key == "merc.panel").unwrap();

        // At scale 1, the default vertical pitch is 49.3 and the cell is 44:
        // row 0's cell top is round(615 + 49.3 + 22 - 22) = 664; each later
        // band advances by 49.3 before integer rounding. The x band starts at
        // the pass-2 name crop's left edge — the name column at 720 less the
        // 4 px pad and 8 px lead (`geometry::name_crop_left`) — through the
        // last slot at 1247.
        assert_eq!(
            view.rows,
            vec![
                [708, 664, 539, 44],
                [708, 714, 539, 44],
                [708, 763, 539, 44],
                [708, 812, 539, 44],
                [708, 862, 539, 44],
                [708, 911, 539, 44],
            ],
        );
    }

    #[test]
    fn non_merc_preview_rows_are_empty() {
        let screen = measured_screen(1920, 1200, 1.0, None);
        let placements = crate::ssot::placements(&screen);
        let rects = assemble_ocr_rects(&placements, Some(&screen), None);

        for key in ["lab.gem", "temple.panel"] {
            let view = rects.iter().find(|rect| rect.key == key).unwrap();
            assert!(view.rows.is_empty(), "{key} must not publish merc row bands");
        }
    }

    #[test]
    fn the_merc_preview_needs_the_screen_slice_even_when_a_placement_exists() {
        let screen = measured_screen(1920, 1080, 0.9, None);
        let placements = crate::ssot::placements(&screen);
        // Defensive pairing only: get_ocr_rects cannot produce it because placements derive from the same screen option.
        let rects = assemble_ocr_rects(&placements, None, None);

        let view = rects.iter().find(|rect| rect.key == "merc.panel").unwrap();
        assert_eq!(view.rect, None);
        assert_eq!(view.source, "unlocated");
    }

    #[test]
    fn merc_preview_rect_is_unlocated_without_a_published_panel() {
        let placements = crate::ssot::placements_for(None);
        let rects = assemble_ocr_rects(&placements, None, None);
        let view = rects.iter().find(|rect| rect.key == "merc.panel").unwrap();
        assert_eq!(view.rect, None);
        assert_eq!(view.source, "unlocated");
    }

    #[test]
    fn temple_rects_from_rois_preserves_each_published_region() {
        let rois = vec![
            crate::temple::slice::RoiView {
                kind: crate::temple::slice::PANEL_REGION.to_string(),
                of: None,
                rect: [1, 2, 3, 4],
            },
            crate::temple::slice::RoiView {
                kind: crate::temple::slice::REMAINING_REGION.to_string(),
                of: None,
                rect: [11, 12, 13, 14],
            },
        ];
        assert_eq!(
            temple_rects_from_rois(&rois),
            (Some([1, 2, 3, 4]), Some([11, 12, 13, 14]))
        );
    }

    #[test]
    fn temple_preview_rects_are_unlocated_without_a_published_layout() {
        let placements = crate::ssot::placements_for(None);
        let rects = assemble_ocr_rects(&placements, None, None);

        for key in ["temple.panel", "temple.remaining"] {
            let view = rects
                .iter()
                .find(|rect| rect.key == key)
                .expect("temple preview row must exist");
            assert_eq!(view.rect, None);
            assert_eq!(view.source, "unlocated");
        }
    }

    /// What every watcher-start catch-up pass does when the shared tail is
    /// `None`: the lab replays nothing and is not in lab, a temple arm the app
    /// already holds stays armed, and loot has no attribution to capture under.
    fn assert_no_tail_leaves_each_catch_up_without_an_answer(path: &std::path::Path) {
        let shared_tail = crate::client_log::recent_log_tail(path);
        assert_eq!(shared_tail, None);

        let (events, in_lab) = crate::lab_navigation::replay_recent_tail(path, shared_tail.as_deref());
        assert!(events.is_empty(), "no lab events: {events:?}");
        assert!(!in_lab);

        let armed = crate::temple::trigger::ArmState {
            arm: crate::temple::trigger::TempleArm::Armed {
                until_ms: None,
                reason: crate::temple::trigger::ArmReason::TempleArea,
            },
            stood_down: crate::temple::trigger::StandDown::CycleComplete,
        };
        let mut temple = armed;
        assert_eq!(crate::temple::trigger::apply_catch_up_tail(&mut temple, shared_tail.as_deref()), None);
        assert_eq!(temple, armed, "no log is no answer, so the arm stands");

        let settings = crate::loot_capture::LootSettings::default();
        let mut loot = crate::loot_capture::LootOwner::default();
        loot.catch_up(shared_tail.as_deref());
        assert!(loot.begin("test".into(), [0; 4], &settings, 1).is_err());
        assert_eq!(loot.frame.as_ref().unwrap().phase, "refused");
    }

    #[test]
    fn a_missing_client_txt_gives_every_catch_up_no_answer() {
        let path = std::env::temp_dir().join("poe_test_catch_up_missing").join("Client.txt");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());

        assert_no_tail_leaves_each_catch_up_without_an_answer(&path);
    }

    #[test]
    fn an_empty_client_txt_gives_every_catch_up_no_answer() {
        let dir = std::env::temp_dir().join("poe_test_catch_up_empty");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("Client.txt");
        std::fs::File::create(&path).unwrap();

        assert_no_tail_leaves_each_catch_up_without_an_answer(&path);

        let _ = std::fs::remove_dir_all(&dir);
    }

    macro_rules! status_echoes {
        ($($name:ident: $field:ident = $value:expr;)*) => {$(
            #[test]
            fn $name() {
                let state = crate::settings::tests::test_app_state();
                state.trade_settings.lock().unwrap().$field = $value;
                assert_eq!(super::build_status(&state).$field, $value);
            }
        )*};
    }

    status_echoes! {
        status_echoes_trade_stale_warn_secs: trade_stale_warn_secs = 121;
        status_echoes_trade_stale_critical_secs: trade_stale_critical_secs = 601;
        status_echoes_trade_auto_refresh_secs: trade_auto_refresh_secs = 901;
        status_echoes_auto_trade_enabled: auto_trade_enabled = true;
    }
}
