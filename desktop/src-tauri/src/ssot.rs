//! App-wide cross-window state SSOT (POE-128 chunk 1).
//!
//! Rust-owned single source of truth for state that overlay windows need to
//! agree on. Delivery to overlays is Rust-backed **polling** via the `get_ssot`
//! command, NOT cross-window JavaScript events: WebView2 cross-window events
//! have returned stale data / failed silently (see docs/OVERLAY-GUIDE.md
//! "Runtime-earned observations"). `emit_ssot` provides an optional eager
//! `ssot-changed` nudge for the main window; overlays must still poll.
//!
//! Measured 2026-09-06 (`[temple-overlay] board read at … (via nudge)` in
//! app.log, docs/TEMPLE-LIFECYCLE.md "Cadences and budgets"): the nudge DID
//! reach the temple overlay window on every read, +13 to +44 ms after the
//! publish, debug and release builds alike. The poll is the backstop the note
//! above describes, not the path a read travels on. The stale-data observation
//! behind that note was about JS-to-JS cross-window events, not Rust emits.
//!
//! Chunk 1 built the core types + the poll-target command. Chunk 3 adds the
//! league resolution service: startup, write 409, Mercure hints, Refresh and
//! server-URL changes wake one task. Success leaves it idle, with no status
//! poll. `LeagueSlice` is the sole holder; trade callers capture it pre-queue.

use std::sync::LazyLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::AppState;

/// First retry delay for the startup league fetch.
const LEAGUE_FETCH_INITIAL_BACKOFF: Duration = Duration::from_secs(2);
/// Ceiling for the exponential backoff — offline-at-launch keeps retrying at
/// most this often, never faster, until the server answers.
const LEAGUE_FETCH_MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Consecutive failed fetch attempts before the SSOT flips to `unreachable`.
/// With the 2→60s backoff this is ~30-60s of silence — long enough not to flag
/// a transient blip, short enough that a genuinely down server stops showing a
/// perpetual "Resolving…". The loop keeps retrying past this point.
const LEAGUE_UNREACHABLE_AFTER_ATTEMPTS: u32 = 4;

/// One process owns the resolver, including while its UI flag is idle.
static LEAGUE_SCHEDULER: LazyLock<LeagueScheduler> = LazyLock::new(LeagueScheduler::new);

struct LeagueScheduler {
    task: std::sync::Mutex<LeagueTask>,
    notify: Notify,
}

impl LeagueScheduler {
    fn new() -> Self {
        Self { task: std::sync::Mutex::new(LeagueTask::default()), notify: Notify::new() }
    }

    fn trigger(&self) -> bool {
        let spawn = self.task.lock().unwrap_or_else(|e| e.into_inner()).trigger();
        self.notify.notify_one();
        spawn
    }

    fn begin_request(&self) {
        // This request covers only triggers already received. Later triggers
        // remain pending through its response and require another status read.
        self.task.lock().unwrap_or_else(|e| e.into_inner()).pending = false;
    }

    async fn wait(&self, delay: Option<Duration>, runtime: &impl LeagueRuntime) -> bool {
        let wake = async {
            loop {
                let notified = self.notify.notified();
                if std::mem::take(&mut self.task.lock().unwrap_or_else(|e| e.into_inner()).pending) {
                    return;
                }
                notified.await;
            }
        };
        if let Some(delay) = delay {
            tokio::select! {
                _ = wake => true,
                _ = runtime.sleep(delay) => false,
            }
        } else {
            wake.await;
            true
        }
    }
}

#[derive(Default)]
struct LeagueTask {
    live: bool,
    pending: bool,
    url_generation: u64,
}

impl LeagueTask {
    fn trigger(&mut self) -> bool {
        self.pending = true;
        let spawn = !self.live;
        self.live = true;
        spawn
    }
}

/// League slice of the SSOT.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LeagueSlice {
    /// Resolved league name.
    ///
    /// `None` means **not yet fetched** — callers must fail closed and MUST NOT
    /// treat it as "always valid" or "no active league". A real league name is
    /// written only once it has been resolved (later chunks).
    pub name: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone, Serialize)]
struct LeagueChanged {
    league: String,
    generation: u64,
}

/// Which cue measured [`ScreenSlice::ui_scale`] (POE-214).
///
/// A statement about the CUE, and — since POE-240 — the input [`accepts`]
/// reads to decide whether a measurement may replace the one standing. It is
/// still not a rank: there is no ordering to consult, only the one rule
/// [`accepts`] states, under which a `MercOcr` reading that agrees with the
/// `MercFrame` value already published leaves it alone instead of overwriting
/// it with the 6-12 px line-pitch drift POE-214 diagnosed. What the label buys
/// a reader — and a smoke check — is the ability to tell a scale measured on
/// the support grid's gold frame from that estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScreenScaleSource {
    /// Measured off the merc support grid's gold frame
    /// (`mercenary::cellfit::refine`) — either on this capture, or carried onto
    /// it from an earlier capture of the same session
    /// (`mercenary::ScaleSource::Held`, which is the same frame
    /// measurement, one or more ticks old).
    MercFrame,
    /// Derived from the merc OCR line pitch — the pre-POE-214 cue. Kept as its
    /// own label rather than folded into `MercFrame` because it is the reading
    /// a session whose fit never landed is running on, and a consumer that
    /// cares how exact its rects are has to be able to see that.
    MercOcr,
    /// Converted from a temple Entrance-plate anchor that cleared
    /// `temple::anchor::NCC_FLOOR` (POE-234 WI-2), through
    /// `temple::anchor::TEMPLE_SCALE_PER_UI_SCALE`.
    ///
    /// A cue that VERIFIES ([`verifies_the_screen`]) — it is a full-resolution
    /// template match on this run's pixels, not a file read — but one whose
    /// number reaches THIS unit through `k`, a coefficient documented as good
    /// to about a per cent. That is why [`accepts`] gives it the same
    /// band-limited rule as [`Self::MercOcr`] rather than `MercFrame`'s
    /// unconditional one: the two carry different uncertainties of the same
    /// size, and neither may walk a session off the other inside it.
    TempleAnchor,
    /// Loaded from settings at startup, not measured this run (WI-B2).
    Remembered,
    /// Derived from the capture's own geometry — `height /
    /// [`UI_SCALE_REFERENCE_HEIGHT`]` — with no cue on the game's art at all
    /// (POE-278). The Settings **Recalibrate** press is its one writer.
    ///
    /// It exists because the two cues that came before it are both owned by a
    /// MODULE: nothing measured a screen until a recruit window or a temple
    /// panel was opened, so a player who runs neither module had the lab rects
    /// cut from `crate::ASSUMED_UI_SCALE` and a hard 1080p client forever —
    /// on a 3840x2160 screen that is the top-left QUARTER of the gem band.
    ///
    /// **What is measured and what is derived is not the same half.** The
    /// dimensions, the monitor id, the origin and the client rectangle are read
    /// straight off `crate::capture::Capture` and are as true as any other
    /// writer's. Only `ui_scale` is derived, and it is exact for as long as one
    /// assumption holds: that the game's UI scales with screen height and
    /// nothing else. PoE has no UI-scale control — the graphics options carry a
    /// render-resolution ratio and an upscale mode (NIS/FSR), which change how
    /// the frame is drawn and not how big the UI is in it (owner-confirmed,
    /// 2026-09-10) — so there is no user multiplier for this to be missing.
    /// What it cannot see is a NON-LINEARITY in the game's own scaling at a
    /// resolution nobody has measured; that is what makes it derived rather
    /// than measured, and why it verifies nothing ([`verifies_the_screen`])
    /// and yields to every real cue ([`accepts`]).
    Capture,
}

/// The screen height [`ScreenSlice::ui_scale`] calls 1.0 — the height of the
/// merc reference fixture, and the denominator every derived scale divides by.
///
/// This is the unit note's own number, stated as a constant because POE-278
/// gave it an arithmetic USER: [`ui_scale_for_height`] is what a Recalibrate
/// press turns a capture into. `temple::run` deliberately restates it instead
/// of importing it (see the const there) — that file does not read this slice
/// and does not want a dependency on it.
pub const UI_SCALE_REFERENCE_HEIGHT: f32 = 1200.0;

/// The `ui_scale` a screen of this height implies, under the one assumption
/// [`ScreenScaleSource::Capture`] documents: the game's UI scales with screen
/// HEIGHT and nothing else.
///
/// `crate::ASSUMED_UI_SCALE` is this function evaluated at 1080 and frozen —
/// which is precisely the bug POE-278 fixes, because a constant cannot notice
/// that the screen is 2160 tall.
///
/// A zero height answers `0.0` — the height is the NUMERATOR and the constant
/// denominator is never zero, so this needs no guard and cannot produce a
/// `NaN`. [`valid_scale`] refuses the `0.0` at the consuming end, which is
/// where a screen that measured nothing is supposed to be caught. Stated
/// because the obvious defence here (an `if height == 0` early return) is dead
/// code that reads like it is load-bearing.
///
/// Pure so the one piece of arithmetic in the derivation is testable without a
/// screen.
pub fn ui_scale_for_height(height: u32) -> f32 {
    height as f32 / UI_SCALE_REFERENCE_HEIGHT
}

/// The screen the game is drawn on and the game-UI scale measured on it
/// (POE-214).
///
/// **Unit.** `ui_scale` is game-UI px per px of the reference fixture:
/// `desktop/src-tauri/tests/fixtures/merc-skills-panel.png`, cut from a
/// 1920x1200 screen, is 1.0 by definition, and the 1080p machine measures
/// 0.90 = 1080/1200 — the game's UI scales with screen HEIGHT. The temple's
/// `AnchorCalibration::scale` is a DIFFERENT unit (relative to its own
/// `REFERENCE_SCREEN_WIDTH`, 1374). The ratio between the two is
/// `temple::anchor::TEMPLE_SCALE_PER_UI_SCALE`, 1.1111 (POE-234), which is good
/// to about 1% — its own doc states which half of it is measured and which is
/// nominal. The temple converts through it in BOTH directions (POE-234 WI-2):
/// it derives its anchor hint from this slice, and publishes what it anchors
/// back onto it under [`ScreenScaleSource::TempleAnchor`]. Said here rather
/// than implied, because the two fields spell the same word and substituting
/// one for the other is an 11% error on a 1080p screen.
///
/// **Reader rule.** A non-merc consumer reads THIS slice, never
/// `mercenary.capture.scale`. Both are written from the same settled
/// `MercLayout::scale` on the same tick, so they never disagree — but the
/// capture is `None` until a recruit window opens and is retired again when it
/// closes, so a reader keyed on it would lose the screen's scale every time the
/// player shuts the window. This slice outlives the capture.
///
/// **Writers.** Two detect ticks write here through [`publish_screen`] — the
/// merc one and, since POE-234 WI-2, the temple's — plus, since POE-278, the
/// Settings **Recalibrate** command ([`geometry_recalibrate`]), which publishes
/// a capture-derived base under [`ScreenScaleSource::Capture`] and is the one
/// writer that needs no module running. The startup load of a persisted value
/// comes in under [`ScreenScaleSource::Remembered`] (WI-B2).
/// The temple gates its own OFFER before this rule ever sees it
/// (`temple::run::screen_from_anchor`): an anchor has to be corroborated by the
/// standing measurement, or by the capture's height when there is none, so what
/// arrives here is a scale within a per cent of something already known. That is
/// a writer declining to teach, not a second acceptance rule — [`accepts`] below
/// is still the only thing that decides what stands.
/// There is still no rank between them: not every measurement either offers is
/// taken, and [`accepts`] is the whole of that rule. It refuses a `MercOcr` or
/// `TempleAnchor` reading that only re-states the value already standing
/// (POE-240), so a session whose merc fit landed once is not walked off it by
/// the drifting cue, nor by a temple number that reached this unit through a
/// coefficient of the same accuracy as the gap between them.
///
/// **No `PartialEq`, deliberately.** Whole-struct equality would compare
/// `measured_at_ms` and `ui_scale` exactly — the two fields the publish gate
/// must NOT compare that way (the stamp moves every tick, the scale wobbles;
/// see [`screen_changed`]). Deriving it would put a `current == next` one-liner
/// within reach that silently re-emits on every tick, so the derive is left off
/// and [`screen_changed`] is the only equality question anyone gets to ask.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSlice {
    /// Captured screen width in physical px.
    pub width: u32,
    /// Captured screen height in physical px.
    pub height: u32,
    /// Game-UI px per reference-fixture px — see the unit note above.
    pub ui_scale: f32,
    /// What measured `ui_scale`. A label, not a precedence.
    pub source: ScreenScaleSource,
    /// Unix ms the measurement was taken at.
    pub measured_at_ms: u64,
    /// Whether a cue that VERIFIES the screen produced this value in THIS run
    /// (POE-240), as [`verifies_the_screen`] decides.
    ///
    /// The README's lifecycle says a scale is "remembered and TRUSTED at start,
    /// VERIFIED on first use"; before POE-240 that second half had no
    /// representation, so a remembered value and a value the gold frame
    /// re-measured this session looked identical to every consumer and to the
    /// Settings card. `false` is the honest answer for a startup seed and for
    /// an OCR-only session, and it is what carries the blind spot
    /// [`screen_matches`] cannot see.
    ///
    /// NOT persisted: [`crate::settings::ScreenScaleSetting::from_slice`] drops
    /// it and `to_slice` seeds it from [`verifies_the_screen`] — which answers
    /// `false` for a load, because a load is not a verification.
    pub verified_this_session: bool,
    /// The Windows handle of the display this was measured on (POE-237) —
    /// `crate::capture::Capture`'s `monitor_id`, carried straight through by the
    /// writer. `0` when unknown (a settings file written before POE-237).
    ///
    /// Carried for the log and the Settings card, NOT compared: the handle is
    /// not stable for one display. One 1920x1080 screen at `(0, 0)` logged
    /// 65622, 65641, 161748971 and 65622 again between 2026-09-10 and
    /// 2026-09-14, and each change dropped a good scale, which left the temple
    /// to search for its panel with no hint. [`origin`](Self::origin) is the
    /// identity ([`different_display`]).
    pub monitor_id: u32,
    /// The measured display's top-left in virtual-desktop PHYSICAL px, so a
    /// consumer holding a rect measured inside a capture can place it on the
    /// desktop. `(0, 0)` for the primary monitor and for an unknown one.
    ///
    /// WHICH display this was measured on: two displays of one desktop cannot
    /// share a top-left, so this is what tells a second monitor of the same
    /// resolution from the remembered one ([`different_display`]). Accepted
    /// trade-off: rearranging displays in Windows moves the origin and drops the
    /// scale once, and the next panel re-measures it. An unknown `(0, 0)` reads
    /// as the primary display, so a scale stored before the origin was recorded
    /// survives on the primary and is re-measured anywhere else.
    ///
    /// No rect is placed from it yet: every rect in the app today is window- or
    /// capture-relative.
    pub origin: (i32, i32),
    /// The capture rectangle in capture-relative physical px: x, y, w, h.
    /// POE-268's capture path is fullscreen, so this is the whole capture,
    /// `[0, 0, width, height]`; POE-272 will make it a live client rectangle.
    pub client: [i32; 4],
    /// Capture-relative origins learned by a module's later calibration pass.
    /// `None` means the corresponding placement still uses its seed.
    pub anchors: Option<Anchors>,
}

/// Capture-relative placement anchors learned after the screen seed was
/// published. The temple value is the Entrance origin; the merc value is the
/// panel's top-left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchors {
    pub temple_entrance: Option<[i32; 2]>,
    pub merc_panel: Option<[i32; 2]>,
}

/// The only module origins that the shared placement state can remember.
/// Taking this enum at the public seam keeps an unknown string from becoming a
/// silent no-op or a misspelled persisted anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorModule {
    TempleEntrance,
    MercPanel,
}

/// Derived capture-relative geometry consumed by OCR and preview surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplePlacements {
    pub entrance_origin: (i32, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MercPlacements {
    pub panel: [i32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabPlacements {
    pub gem: [i32; 4],
    pub font: [i32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placements {
    pub temple: Option<TemplePlacements>,
    pub merc: Option<MercPlacements>,
    pub lab: LabPlacements,
}

/// **PROVISIONAL** temple seed from one measurement: a 1920x1080 laptop at UI
/// scale 0.90 reported the Entrance origin at `(960, 713)`; the y seed is that
/// point converted back to reference px, and its y edge was not measured.
pub const TEMPLE_ENTRANCE_Y_REF: f32 = 792.2;

/// **PROVISIONAL** merc seed from the documented 1920x1200 laptop panel
/// geometry in `mercenary/geometry.rs` (the panel literal around lines
/// 1112-1117). The x anchor has now been measured on two machines: it is the
/// panel centre relative to the client centre, not a left-edge offset. The y
/// anchor is measured on one machine and remains top-anchored; the panel edge
/// and size are still the reference seed.
pub const MERC_PANEL_REF: [i32; 4] = [698, 615, 555, 477];

impl Default for Placements {
    fn default() -> Self {
        placements_for(None)
    }
}

fn valid_scale(screen: Option<&ScreenSlice>) -> f32 {
    screen
        .map(|screen| screen.ui_scale)
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .unwrap_or(crate::ASSUMED_UI_SCALE)
}

fn scale_value(value: i32, scale: f32) -> i32 {
    (value as f32 * scale).round() as i32
}

/// The gem name region for a client rect: the client's full-width top band, one
/// name line tall — see [`crate::INVENTORY_PANEL_W_REF`] for the rule and its
/// provenance. Never taller than the name line, by the owner's ruling; full
/// width because the tooltip follows the hovered item across the inventory's
/// left edge.
fn gem_region(client: [i32; 4], scale: f32) -> [i32; 4] {
    [
        client[0],
        client[1],
        client[2].max(0),
        scale_value(crate::GEM_NAME_BAND_H_REF, scale).max(0),
    ]
}

/// The font panel region for a client rect: `FONT_PANEL_W_REF · scale` wide,
/// centred in the space left of the right-docked inventory (whose width is
/// [`crate::INVENTORY_PANEL_W_REF`] · scale), `FONT_PANEL_TOP_REF
/// · scale` below the client's top and `FONT_PANEL_H_REF · scale` tall — see
/// [`crate::FONT_PANEL_W_REF`] for the rule and its provenance. The centring
/// halves in integer px, so an odd gap lands the region one px left of centre.
fn font_region(client: [i32; 4], scale: f32) -> [i32; 4] {
    let space_w = client[2] - scale_value(crate::INVENTORY_PANEL_W_REF, scale);
    let w = scale_value(crate::FONT_PANEL_W_REF, scale).max(0);
    [
        client[0] + (space_w - w) / 2,
        client[1] + scale_value(crate::FONT_PANEL_TOP_REF, scale),
        w,
        scale_value(crate::FONT_PANEL_H_REF, scale).max(0),
    ]
}

/// The client an unmeasured screen is assumed to have: fullscreen 1080p, the
/// same assumption [`crate::ASSUMED_UI_SCALE`] makes.
const ASSUMED_1080P_CLIENT: [i32; 4] = [0, 0, 1920, 1080];

/// Derive all capture-relative placements from the current screen slice.
/// Seeds are scaled reference geometry; a remembered anchor replaces only the
/// origin it owns and leaves the documented seed size intact.
pub fn placements(screen: &ScreenSlice) -> Placements {
    placements_for(Some(screen))
}

/// Derive placements when the screen has not been measured yet. Lab retains its
/// existing 1080p assumption; temple and merc remain unlocated until a capture
/// establishes a client rectangle.
pub fn placements_for(screen: Option<&ScreenSlice>) -> Placements {
    let Some(screen) = screen else {
        return Placements {
            temple: None,
            merc: None,
            lab: LabPlacements {
                gem: gem_region(ASSUMED_1080P_CLIENT, crate::ASSUMED_UI_SCALE),
                font: font_region(ASSUMED_1080P_CLIENT, crate::ASSUMED_UI_SCALE),
            },
        };
    };

    let scale = valid_scale(Some(screen));
    let client = screen.client;
    let lab = LabPlacements { gem: gem_region(client, scale), font: font_region(client, scale) };
    let temple_seed = (
        client[0] + client[2] / 2,
        client[1] + (TEMPLE_ENTRANCE_Y_REF * scale).round() as i32,
    );
    let temple_origin = screen
        .anchors
        .and_then(|anchors| anchors.temple_entrance)
        .map(|[x, y]| (x, y))
        .unwrap_or(temple_seed);
    let merc_width_ref = MERC_PANEL_REF[2] as f32 * scale;
    let merc_width = merc_width_ref.round().max(0.0) as i32;
    let merc_seed = [
        (client[0] as f32
            + client[2] as f32 / 2.0
            + (MERC_PANEL_REF[0] as f32 + MERC_PANEL_REF[2] as f32 / 2.0 - 960.0) * scale
            - merc_width_ref / 2.0)
            .round() as i32,
        client[1] + scale_value(MERC_PANEL_REF[1], scale),
        merc_width,
        scale_value(MERC_PANEL_REF[3], scale).max(0),
    ];
    let merc_origin = screen
        .anchors
        .and_then(|anchors| anchors.merc_panel)
        .unwrap_or([merc_seed[0], merc_seed[1]]);

    Placements {
        temple: Some(TemplePlacements { entrance_origin: temple_origin }),
        merc: Some(MercPlacements {
            panel: [merc_origin[0], merc_origin[1], merc_seed[2], merc_seed[3]],
        }),
        lab,
    }
}

/// Full app-wide SSOT snapshot. Cloned for both the poll response and the
/// eager event payload, so it stays cheap and `Send`.
///
/// The `Default` gives `league.name == None` (fail-closed) — locked by the
/// unit test below. Future slices are added as sibling fields here.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSsotSnapshot {
    pub boss_loot: crate::loot_capture::LootSlice,
    pub league: LeagueSlice,
    /// A league resolve is currently in flight (the bounded-retry fetch task is
    /// running). Exposed in the polled snapshot so the Settings UI can show an
    /// honest "Resolving…" for the whole retry window instead of a one-frame
    /// flash. Task liveness is independent: after success the task waits for
    /// a trigger with this flag false. `Default` is `false`.
    pub resolving: bool,
    /// The league resolver has failed `LEAGUE_UNREACHABLE_AFTER_ATTEMPTS`
    /// consecutive times and is now treating the server as unreachable — while
    /// STILL retrying. Set alongside `resolving == true`; distinguishes an
    /// honest "server down, still trying" (offer an actionable Refresh) from a
    /// fresh "Resolving…" flash (Refresh disabled). Cleared on the next
    /// successful fetch. `Default` is `false`.
    pub unreachable: bool,
    /// The selected Normal-mode market ("20/20", "20/0", "1/20", "1/0").
    ///
    /// Composed at read time from `AppState.lab_settings`, which stays the
    /// owner — this field is NOT stored in `AppState.ssot`, so there is no
    /// second copy to keep in sync on every `set_normal_variant`. `Default` is
    /// the empty string, which the webview must reject as "unknown".
    pub normal_variant: String,
    /// The selected Dedication corrupted market ("21/23", "21/20").
    ///
    /// Composed at read time from `AppState.lab_settings`, which stays
    /// the owner — not stored in `AppState.ssot`. `Default` is the empty
    /// string, which the webview must reject as "unknown".
    pub dedication_variant: String,
    /// The selected Dedication rankings pool ("skill", "transfigured").
    ///
    /// Composed at read time from `AppState.lab_settings`, which stays the
    /// owner — not stored in `AppState.ssot`. `Default` is the empty string,
    /// which the webview must reject as "unknown".
    pub dedication_pool: String,
    /// Per-module enabled flags, projected unchanged from the owner map
    /// `AppState.modules_enabled` (see src/modules.rs). **Intent, not
    /// liveness**: a module that panicked still reports enabled. `Default` is
    /// the empty map, which the webview must read as "not yet known".
    pub modules: std::collections::HashMap<String, bool>,
    /// Merc OCR capture state (POE-165), projected from the owner
    /// `AppState.mercenary` (see src/mercenary/mod.rs). `status` is forced to
    /// `Off` here when the module is disabled — the composer owns that one
    /// precedence step (off > unavailable > live > done > scanning > idle), the
    /// capture loop owns the other five. `Default` is the `Off` slice, which the page
    /// renders as "module off".
    pub mercenary: crate::mercenary::MercenarySlice,
    /// Temple builder state (POE-171), projected from the owner
    /// `AppState.temple` (see src/temple/slice.rs). `status` is forced to `Off`
    /// here when the module is disabled — and the advice is dropped with it, so
    /// a disabled module cannot leave a stale recommendation on screen under an
    /// "off" badge. `Default` is the `Idle` slice with no board.
    pub temple: crate::temple::slice::TempleSlice,
    /// The screen and its measured game-UI scale (POE-214), projected from the
    /// owner `AppState.screen`. `None` until something measures one — fail
    /// closed, and specifically do NOT substitute 1.0: that is a real
    /// measurement (the 1920x1200 reference) and assuming it would silently
    /// mis-scale every rect on a 1080p machine by 11%.
    pub screen: Option<ScreenSlice>,
    /// Capture-relative OCR and module placements derived from `screen`.
    /// These are projected at snapshot time, never stored as a second owner.
    pub placements: Placements,
    // future slices (e.g. account, config) added here as later tasks land.
}

/// Return `base` with the three market fields, the module map and the
/// mercenary slice replaced by the given values.
///
/// The enabled-guide set (`merc_sources_off`, POE-199) is composed onto the
/// mercenary slice here rather than stored in it — the owner is
/// `AppState.merc_sources_off`, exactly as `lab_settings` owns the markets.
///
/// The mercenary slice's `status` is forced to `Off` when the module is
/// disabled. This is the single place module enablement reaches that slice, so
/// the page never has to read `ssot.modules` to know whether the toggle is on
/// (ADR-014: the page reads slices, not module state).
///
/// Pure so the composition is unit-testable without an `AppHandle` or a full
/// `AppState` — same reason `should_flag_unreachable` is extracted.
fn compose_snapshot(
    base: AppSsotSnapshot,
    normal_variant: String,
    dedication_variant: String,
    dedication_pool: String,
    modules: std::collections::HashMap<String, bool>,
    mut mercenary: crate::mercenary::MercenarySlice,
    merc_sources_off: Vec<String>,
    merc_sync: crate::mercenary::sync::MercSyncStatus,
    merc_trade_auto: bool,
    merc_tier_floor: u8,
    mut temple: crate::temple::slice::TempleSlice,
    screen: Option<ScreenSlice>,
) -> AppSsotSnapshot {
    if modules.get(MERCENARY_MODULE_ID) != Some(&true) {
        mercenary.force_off();
    }
    // The settings echo, written AFTER the force-off for the reason the temple
    // slice keeps its own echo through `force_off`: what the user set is not
    // something the module read, and the page renders its guide toggles from
    // it while the module is switched off.
    mercenary.sources_off = merc_sources_off;
    // The pool's status, composed for the same reason and after the same
    // force-off: the pull and the uploader are tasks, not the capture loop, so
    // the slice keeps one writer and this is where their state joins it. Kept
    // when the module is off so the page can still say what the last pull did.
    mercenary.sync = merc_sync;
    // Two more settings echoes, composed after the force-off like the guide set
    // and for the same reason: the page renders the auto toggle and the tier
    // select from them while the module is switched off and no loop will ever
    // publish them.
    mercenary.trade_auto = merc_trade_auto;
    mercenary.tier_floor = merc_tier_floor;
    if modules.get(TEMPLE_MODULE_ID) != Some(&true) {
        temple.force_off();
    }
    let placements = placements_for(screen.as_ref());
    AppSsotSnapshot {
        normal_variant,
        dedication_variant,
        dedication_pool,
        modules,
        mercenary,
        temple,
        screen,
        placements,
        ..base
    }
}

/// The module id the mercenary slice belongs to (see `modules.rs::MODULES`).
const MERCENARY_MODULE_ID: &str = "mercenary";

/// The module id the temple slice belongs to (see `modules.rs::MODULES`).
const TEMPLE_MODULE_ID: &str = "temple";

pub fn league_name(state: &AppState) -> Option<String> {
    state.ssot.lock().unwrap_or_else(|e| e.into_inner()).league.name.clone()
}

/// Build the full snapshot: the stored `AppState.ssot` slice plus the market
/// fields and the module map, composed from their owning `AppState` Mutexes.
///
/// The `ssot` guard is dropped before the market Mutexes are locked (never two
/// guards at once), matching the lock-then-emit discipline documented on
/// `emit_ssot`. Both `get_ssot` and `emit_ssot` go through here so the two
/// paths cannot compose different snapshots.
pub fn build_snapshot(state: &AppState) -> AppSsotSnapshot {
    let mut base = {
        let guard = state.ssot.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    };
    base.boss_loot = crate::loot_capture::snapshot(state);
    let (normal_variant, dedication_variant, dedication_pool) = {
        let lab = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner());
        (lab.normal_variant.clone(), lab.dedication_variant.clone(), lab.dedication_pool.clone())
    };
    // Lone acquisition of `modules_enabled` — `module_handles` is not held here
    // (lock order, see src/modules.rs).
    let modules = state.modules_enabled.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // Same lock-then-drop discipline: the guard ends with this statement, so
    // the merc mutex is never held while another is taken or while emitting.
    let mercenary = state.mercenary.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // Its own lone acquisition, like every other owner read here.
    let merc_sources_off = state
        .merc_sources_off
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    // Its own lone acquisition too — `MercSyncStatus::current` takes and drops the
    // `merc_sync` guard inside one statement.
    let merc_sync = crate::mercenary::sync::MercSyncStatus::current(state);
    // Lone acquisitions again, each ending with its own statement.
    let merc_trade_auto = *state.merc_trade_auto.lock().unwrap_or_else(|e| e.into_inner());
    let merc_tier_floor = *state.merc_tier_floor.lock().unwrap_or_else(|e| e.into_inner());
    // Same discipline again: the temple guard ends with this statement, so it
    // is never held alongside the merc one or across the compose.
    let temple = state.temple.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // One more lone acquisition, ending with its own statement — which is what
    // lets `publish_screen` re-enter here through `emit_ssot` after dropping
    // its own guard. `ScreenSlice` is `Copy`, so the deref clones it.
    let screen = *state.screen.lock().unwrap_or_else(|e| e.into_inner());
    compose_snapshot(
        base,
        normal_variant,
        dedication_variant,
        dedication_pool,
        modules,
        mercenary,
        merc_sources_off,
        merc_sync,
        merc_trade_auto,
        merc_tier_floor,
        temple,
        screen,
    )
}

/// Whether `consecutive_failures` failed fetch attempts should flip the SSOT to
/// `unreachable`. Pure so the threshold boundary is directly unit-testable.
fn should_flag_unreachable(consecutive_failures: u32) -> bool {
    consecutive_failures >= LEAGUE_UNREACHABLE_AFTER_ATTEMPTS
}

/// How far [`ScreenSlice::ui_scale`] must move before it counts as a different
/// screen scale.
///
/// The published number is the merc session's SETTLED scale, and settled does
/// not mean constant: `mercenary::run::next_fitted_scale`'s same-`cell_px` path
/// returns `..fresh`, so every grid-fitting tick takes the fresh measurement's
/// scale while holding the cell size. One `cellfit::P_STEP` of the pitch grid
/// moves that number by **0.0031** — an exact `!=` therefore republishes on a
/// panel that has not moved a pixel.
///
/// 0.01 sits between the two numbers that bound it: above the 0.0031 wobble, and
/// well below the 1/40 = 0.025 a whole px of cell is worth. So the band swallows
/// the measurement noise and still catches every scale change a consumer could
/// cut a different rect from.
const UI_SCALE_EPS: f32 = 0.01;

/// Whether `next` is worth emitting over what is already published.
///
/// The three MEASURED fields and the label: anything a consumer scales a rect
/// with, or judges the exactness of its rects by. `measured_at_ms` alone is
/// deliberately NOT one of them — the merc detect tick re-measures the same
/// screen every few seconds for as long as a recruit window is open, and a
/// timestamp-only diff would emit `ssot-changed` and spin every overlay's poll
/// on a value none of them can act on.
///
/// `ui_scale` gets the same treatment for the same reason, one step weaker: it
/// is a MEASUREMENT and it wobbles by 0.0031 between ticks of a panel that has
/// not moved (see [`UI_SCALE_EPS`]), so it is compared to within that band
/// rather than exactly. Width, height, the label and — since POE-237 — the
/// monitor id are discrete and are compared exactly.
///
/// The monitor id has to be one of them: a player who drags the game from one
/// 1920x1080 display to another changes every rect's screen-absolute position
/// and nothing else, so without this term the move would never reach a
/// consumer. It is compared with a bare `!=` because this is an emit gate, not
/// an identity: a new handle is news for the Settings card even on the same
/// display, which is the different question [`different_display`] answers.
///
/// `origin` is absent. A slice whose origin disagrees with the capture is
/// dropped by [`drop_if_mismatched`] before anything re-publishes, and a
/// publish into an empty slot is always a change.
///
/// The client rectangle and remembered anchors are included: either changes
/// the capture-relative geometry a consumer reads.
///
/// `None` — nothing published yet — is always a change.
///
/// Pure so the gate is unit-testable without an `AppHandle`, the same reason
/// `should_flag_unreachable` is extracted.
fn screen_changed(current: Option<&ScreenSlice>, next: &ScreenSlice) -> bool {
    match current {
        None => true,
        Some(current) => {
            current.width != next.width
                || current.height != next.height
                || (current.ui_scale - next.ui_scale).abs() >= UI_SCALE_EPS
                || current.source != next.source
                || current.monitor_id != next.monitor_id
                || current.client != next.client
                || current.anchors != next.anchors
        }
    }
}

/// Whether two virtual-desktop origins describe DIFFERENT displays (POE-237).
///
/// The origin and not the monitor handle, because the handle changes for one
/// display between sessions (see [`ScreenSlice::monitor_id`]) while two
/// displays of one desktop cannot share a top-left.
///
/// One function rather than the same `!=` at three call sites
/// ([`screen_matches`], [`accepts`], `same_screen_geometry`), because the three
/// are one rule: what makes two screens the same display.
fn different_display(a: (i32, i32), b: (i32, i32)) -> bool {
    a != b
}

/// How far from the standing value a reading may sit and still be explained by
/// the way it was taken, in `ui_scale` units. The band [`accepts`] holds
/// [`ScreenScaleSource::MercOcr`] and [`ScreenScaleSource::TempleAnchor`] to.
///
/// Two different uncertainties, and they are the same size — which is why one
/// constant serves both rather than two that would be maintained apart:
///
/// - **OCR line pitch.** POE-214 measured the estimate at 6-12 px off the gold
///   frame. The unit's denominator is the reference HEIGHT, 1200 px, so the
///   wide end of that range is `12 / 1200 = 0.01` of `ui_scale`.
/// - **A temple anchor converted through `k`.** The anchor itself is exact to a
///   `SCALE_STEP` of its own unit, but `temple::anchor::TEMPLE_SCALE_PER_UI_SCALE`
///   is documented as good to about a per cent, and a per cent of the 0.90 a
///   1080p screen measures is 0.009.
///
/// A reading further from the standing value than this is describing something
/// neither can explain — a real change of screen — and [`accepts`] takes it.
const OCR_DRIFT_BAND: f32 = 0.01;

/// Whether a cue that VERIFIES the screen is what produced a measurement
/// (POE-240) — the one rule behind [`ScreenSlice::verified_this_session`].
///
/// Two cues do: the merc gold frame, and a temple Entrance-plate anchor that
/// cleared `temple::anchor::NCC_FLOOR` (POE-234 WI-2). Both looked at THIS
/// run's pixels of THIS screen and matched art against a template, which is
/// what the flag claims. `MercOcr` is the line-pitch estimate that sits within
/// [`OCR_DRIFT_BAND`] of the truth at best, and `Remembered` is a file read:
/// neither looked at anything this run that could confirm the screen is the
/// screen the number was measured on.
///
/// **Verifying is not the same question as replacing**, and this is the one
/// place the two visibly part company: `TempleAnchor` answers `true` here and
/// still gets [`accepts`]' band-limited rule, because "something looked at this
/// screen" and "this number is better than the one standing" are different
/// claims. The second is what `k`'s accuracy bounds.
///
/// Pure, and called by all three writers — `mercenary::run`'s publish,
/// `temple::run`'s, and [`crate::settings::ScreenScaleSetting::to_slice`] — so
/// the flag has one producer rather than a literal at each site that can drift
/// apart.
pub fn verifies_the_screen(source: ScreenScaleSource) -> bool {
    matches!(
        source,
        ScreenScaleSource::MercFrame | ScreenScaleSource::TempleAnchor
    )
}

/// Whether `next` may replace what is already in the screen slot (POE-240).
///
/// The rule, by the cue that produced `next`:
///
/// - **`MercFrame`** — always. It is the measurement everything else is a
///   stand-in for, and a fresh one is never worse than what it replaces.
/// - **`MercOcr` and `TempleAnchor`** — only when saying something their own
///   uncertainty cannot explain: a different DISPLAY ([`different_display`],
///   POE-237), different dimensions, or a `ui_scale` further than
///   [`OCR_DRIFT_BAND`] from the standing value. Inside the band, on the same
///   screen, an OCR reading is that screen re-described 6-12 px worse, and
///   taking it would walk a session off a landed frame fit for every remaining
///   tick of an open recruit window — the bug POE-240 was filed for. A temple
///   anchor inside the band is the same shape of non-news for a different
///   reason (POE-234 WI-2): it converts through `k`, whose own accuracy is
///   about this band, so it cannot claim to improve on the number standing —
///   and re-publishing it would flip `source` back and forth between the two
///   modules for as long as both panels are open, each change waking every
///   overlay's poll and spending a settings write. A different display is not
///   that case at all: the standing value describes a screen the game has left,
///   so even a band-grade reading of the one it is on now is the better answer,
///   and the band must not be allowed to hold the old monitor's number in
///   place.
/// - **`Remembered`** — never over anything. It is the startup seed: it fills
///   an empty slot and has nothing to say about a screen that has since been
///   measured.
/// - **A standing `Capture` value** — replaced by ANY cue on the art, including
///   the two band-limited ones. Stated as its own rule because it is the term
///   that keeps the derivation from being a one-way door: see the note in the
///   `MercOcr | TempleAnchor` arm below.
/// - **`Capture` as the incoming value** — never over anything, and for a
///   sharper reason than `Remembered`'s (POE-278). It is DERIVED from the capture's height, so on a
///   screen where the game's scaling is linear it agrees with a `MercFrame` fit
///   by construction and has nothing to add; and on a screen where it does NOT
///   agree, the disagreement means the derivation's assumption is wrong THERE
///   and the cue that looked at the art is the one to keep. Either way the
///   answer is the same, which is why it gets `Remembered`'s flat refusal
///   rather than the band the two estimating cues share: a band would let
///   exactly the readings that prove it wrong overwrite the ones that are
///   right. Its writer ([`geometry_recalibrate`]) empties the slot first, so
///   the refusal never blocks the press that asked for it.
///
/// An empty slot takes anything — there is nothing to protect.
///
/// `settings::apply_to_state` writes the remembered seed directly into an empty
/// slot at startup and is the only caller that bypasses [`record_screen`]; any
/// future settings-reload path must go through [`record_screen`], or it will
/// overwrite a measurement this rule exists to protect.
///
/// A refusal is not silent, but the log line is the CALLER's (see
/// `mercenary::run`, which dedupes it per distinct source and rounded scale);
/// this stays pure so the rule is unit-testable without an `AppHandle`.
pub(crate) fn accepts(current: Option<&ScreenSlice>, next: &ScreenSlice) -> bool {
    let Some(current) = current else {
        return true;
    };
    match next.source {
        ScreenScaleSource::MercFrame => true,
        ScreenScaleSource::Remembered | ScreenScaleSource::Capture => false,
        ScreenScaleSource::MercOcr | ScreenScaleSource::TempleAnchor => {
            // A cue that looked at the game's art ALWAYS replaces a derived
            // value, band or no band (POE-278). This term is what stops the
            // derivation from becoming a trap: `Capture` cannot see a
            // non-linearity in the game's own scaling, and the only cues that
            // CAN see one are these — so holding them off because they agree
            // with the arithmetic to within the band would lock out the one
            // correction that matters, and on a screen where the arithmetic is
            // right it would still pin the slice to an unverified label for the
            // rest of the session. At 1080p a temple anchor converts to exactly
            // the 0.90 the height implies, so without this the press would make
            // `temple-anchor` unreachable on the machine the button was pressed
            // on.
            current.source == ScreenScaleSource::Capture
                || different_display(current.origin, next.origin)
                || current.width != next.width
                || current.height != next.height
                || current.client != next.client
                || (current.ui_scale - next.ui_scale).abs() > OCR_DRIFT_BAND
        }
    }
}

/// What [`record_screen`] did with a measurement.
///
/// Two bools rather than one because the caller acts on each separately: a
/// refusal is worth a log line, a change is worth an emit and a persist, and
/// "not a change" and "not taken at all" are different answers that used to
/// arrive as the same `false`.
#[derive(Debug, Clone, Copy)]
pub struct ScreenRecord {
    /// Whether the slot took `next`. `false` — [`accepts`] refused it and the
    /// value already standing is untouched, `measured_at_ms` included.
    pub accepted: bool,
    /// Whether the overlays are worth waking. Always `false` on a refusal.
    pub changed: bool,
    /// The `ui_scale` in force once this call returned: `next`'s when it was
    /// taken, the survivor's when it was refused.
    ///
    /// Carried out rather than re-read, because the refusal log line names
    /// both numbers and this call is the only place that holds them together
    /// under one guard — by the time the caller has the record back, a
    /// [`geometry_recalibrate`] on another thread may have emptied the slot.
    pub standing_ui_scale: f32,
}

/// Store `next` in the screen slot if [`accepts`] allows it, and report both
/// halves of what happened.
///
/// **Accept first, then gate the emit.** An accepted measurement overwrites the
/// slot whatever the emit gate says, so `measured_at_ms` tracks the last
/// ACCEPTED measurement — WI-B2 persists this slot and an age belonging to an
/// older tick would be a lie. A REFUSED one changes nothing at all, stamp
/// included: the slice describes the value in force, and that value was
/// measured when it says it was (POE-240).
///
/// Split out of [`publish_screen`] with no `AppHandle` precisely so those rules
/// are pinned by tests rather than by the prose above them.
fn same_screen_geometry(current: &ScreenSlice, next: &ScreenSlice) -> bool {
    current.width == next.width
        && current.height == next.height
        && !different_display(current.origin, next.origin)
        && current.client == next.client
        && (current.ui_scale - next.ui_scale).abs() < UI_SCALE_EPS
}

pub(crate) fn record_screen(current: &mut Option<ScreenSlice>, mut next: ScreenSlice) -> ScreenRecord {
    if !accepts(current.as_ref(), &next) {
        // A refusal is only ever over a value that is there — an empty slot
        // takes anything — so the scale that survived is the standing one,
        // never `next`'s.
        let standing = current.as_ref().unwrap_or(&next).ui_scale;
        return ScreenRecord { accepted: false, changed: false, standing_ui_scale: standing };
    }
    if next.anchors.is_none()
        && current
            .as_ref()
            .is_some_and(|standing| same_screen_geometry(standing, &next))
    {
        next.anchors = current.as_ref().and_then(|standing| standing.anchors);
    }
    let changed = screen_changed(current.as_ref(), &next);
    *current = Some(next);
    ScreenRecord { accepted: true, changed, standing_ui_scale: next.ui_scale }
}

/// Publish a screen measurement into the SSOT, emitting only on a real change.
///
/// Lock-then-drop-then-emit, the shape `mercenary::run::publish` and
/// `temple::run::publish_anchor_scale` both use: the `screen` guard is scoped
/// to the block below, so it is dropped before `emit_ssot` — which re-takes it
/// inside `build_snapshot` — and no other mutex is held while it is open.
///
/// What to store and what to emit is [`record_screen`]'s decision (and
/// [`screen_changed`]'s underneath it); both are `AppHandle`-free and tested.
/// All that is left here is the lock-drop-emit sequence the doc above states,
/// which is what makes calling this on every detect tick the right thing to do.
///
/// Returns [`record_screen`]'s whole answer: `changed` is the bool the emit is
/// gated on, handed back so the caller can gate a persist on it too (WI-B2)
/// without asking the slot a second question the deadband would have to answer
/// twice, and `accepted` is what lets the caller say once that a measurement
/// was refused (POE-240). Ignoring it is an ordinary call, so it is not
/// `#[must_use]`.
pub fn publish_screen(app: &AppHandle, next: ScreenSlice) -> ScreenRecord {
    let record = {
        let state = app.state::<AppState>();
        let mut current = state.screen.lock().unwrap_or_else(|e| e.into_inner());
        record_screen(&mut current, next)
    };
    if record.changed {
        emit_ssot(app);
    }
    record
}

/// Whether a just-published slice is worth writing to settings (POE-214 WI-B2).
///
/// Both halves are load-bearing:
///
/// - **`changed`** — [`record_screen`]'s answer, which carries [`UI_SCALE_EPS`],
///   so the per-tick wobble of a panel that has not moved is not a change.
///   `crate::persist_settings` is a settings read plus a whole-file write, both
///   blocking, on the detect thread; without this the loop would pay two disk
///   ops per tick for as long as a recruit window is open.
/// - **A cue that measured something** — the cheap half of one rule that
///   [`crate::settings::ScreenScaleSetting::from_slice`] owns and states in
///   full: an OCR-derived scale is not persistable, so triggering a save on one
///   would spend a whole-file write to null the stored value. `TempleAnchor` is
///   persistable for the same reason `MercFrame` is — it is a template match
///   against [`ScreenSlice::verified_this_session`]'s bar, not an estimate —
///   and remembering it is what lets the temple hand the next session's merc a
///   seed on a machine whose recruit window has never been opened (POE-234
///   WI-2). `Remembered` is refused here for a different reason — it is what
///   was just LOADED, and nothing is learned by writing it straight back.
///
/// The variant set here is the same one [`verifies_the_screen`] answers `true`
/// for, and it is spelled out again rather than delegated: "may be written to
/// disk" is a different question from "looked at the screen this run", and a
/// future cue that verified without being persistable would have to be able to
/// say so in one of the two places without silently moving the other.
///
/// Pure so the trigger is unit-testable without an `AppHandle`, the same reason
/// [`should_flag_unreachable`] and [`record_screen`] are extracted.
pub fn should_remember_screen(changed: bool, source: ScreenScaleSource) -> bool {
    changed
        && matches!(
            source,
            ScreenScaleSource::MercFrame | ScreenScaleSource::TempleAnchor
        )
}

/// Whether a remembered measurement still describes the screen just captured
/// (POE-227), now that a capture says which DISPLAY it came off (POE-237).
///
/// Two terms, and the display is the stronger one: two origins that differ are
/// a mismatch whatever the dimensions say ([`different_display`]), which is
/// what catches the second 1920x1080 monitor a scale used to survive onto. The
/// monitor handle is not consulted: it changes for one display between
/// sessions, and pruning on it threw away a good scale at every change.
///
/// Dimensions stay exact and `ui_scale` is still not consulted, because the
/// whole point is that a scale measured on another monitor is unverifiable from
/// here — the size and the display are the two things a capture states about
/// itself.
///
/// `None` matches everything: there is nothing remembered to be wrong, and
/// answering `false` would make the caller "drop" a slot that is already empty
/// and spend a log line and a file write saying so on every tick.
///
/// One blind spot is left, and it is not fixable from here: an in-game UI-scale
/// change made while no verifying panel is on screen. The consuming module's
/// explicit fallback is responsible for recovering from that case; this
/// geometry predicate only decides whether the stored slice belongs to the
/// capture.
///
/// Pure so the rule is unit-testable without an `AppHandle`, the same reason
/// [`should_flag_unreachable`] and [`record_screen`] are extracted.
pub fn screen_matches(
    current: &Option<ScreenSlice>,
    capture: (u32, u32),
    origin: (i32, i32),
    client: [i32; 4],
) -> bool {
    match current {
        None => true,
        Some(current) => {
            !different_display(current.origin, origin)
                && current.width == capture.0
                && current.height == capture.1
                && current.client == client
        }
    }
}

/// Forget the remembered screen scale when this capture has different geometry
/// (POE-227). Returns whether anything was dropped.
///
/// This is the lazy prune the startup load could not do: `settings::apply_to_state`
/// loads and trusts a stored `screen_scale` with no `AppHandle` and no idea what
/// size the screen is, so a value carried over from another monitor stays
/// readable until someone captures one. Every detect tick that grabs a screen
/// calls this first, which makes the FIRST capture after a resolution or client
/// geometry change the moment the stale value dies. It was the temple's own `settings_for_capture` /
/// `forget_calibration` pair before POE-234 WI-2 generalised it here; the temple
/// now derives its hint from this slice, so this prune IS the temple's prune.
///
/// Dropping means dropping BOTH copies. The owner is cleared here, and
/// [`crate::settings::persist_forgetting_screen_scale`] writes the file with the
/// field empty — `crate::persist_settings` cannot be used for that, because its
/// `preserve_screen_scale` merge would read the cleared owner as "this session
/// has nothing to write" and put the stale value straight back.
///
/// Lock-then-drop-then-emit, like [`publish_screen`]: the `screen` guard is
/// scoped to the block, so it is dropped before the log, the emit and the write.
pub fn drop_if_mismatched(
    app: &AppHandle,
    capture: (u32, u32),
    origin: (i32, i32),
    client: [i32; 4],
) -> bool {
    let dropped = {
        let state = app.state::<AppState>();
        let mut current = state.screen.lock().unwrap_or_else(|e| e.into_inner());
        if screen_matches(&current, capture, origin, client) {
            None
        } else {
            current.take()
        }
    };
    let Some(stale) = dropped else {
        return false;
    };
    crate::app_log(
        app,
        format!(
            "screen is now {}×{} at {},{} — dropping the remembered scale (was {}×{} at {},{})",
            capture.0,
            capture.1,
            origin.0,
            origin.1,
            stale.width,
            stale.height,
            stale.origin.0,
            stale.origin.1
        ),
    );
    emit_ssot(app);
    crate::settings::persist_forgetting_screen_scale(app);
    true
}

fn set_anchor(anchors: &mut Option<Anchors>, module: AnchorModule, origin: [i32; 2]) -> bool {
    match module {
        AnchorModule::TempleEntrance => {
            let anchors = anchors.get_or_insert(Anchors { temple_entrance: None, merc_panel: None });
            if anchors.temple_entrance == Some(origin) { return false; }
            anchors.temple_entrance = Some(origin);
            true
        }
        AnchorModule::MercPanel => {
            let anchors = anchors.get_or_insert(Anchors { temple_entrance: None, merc_panel: None });
            if anchors.merc_panel == Some(origin) { return false; }
            anchors.merc_panel = Some(origin);
            true
        }
    }
}

/// Remember a module's capture-relative placement origin for later ticks.
/// Temple calls this after a successful read from its explicit fallback; the
/// ordinary placed path reads the origin already held by the current slice. A
/// screen that has not been measured is a no-op and returns `false`; the enum
/// makes an unknown module impossible at this boundary.
pub fn remember_anchor(app: &AppHandle, module: AnchorModule, origin: [i32; 2]) -> bool {
    let changed = {
        let state = app.state::<AppState>();
        let mut screen = state.screen.lock().unwrap_or_else(|e| e.into_inner());
        screen.as_mut().is_some_and(|screen| set_anchor(&mut screen.anchors, module, origin))
    };
    remember_anchor_effects(changed, || crate::persist_settings(app), || emit_ssot(app))
}

fn remember_anchor_effects<P, E>(changed: bool, persist: P, emit: E) -> bool
where
    P: FnOnce(),
    E: FnOnce(),
{
    if changed {
        persist();
        emit();
    }
    changed
}

/// The slice a screen grab implies on its own, with no cue on the game's art
/// (POE-278) — the value [`geometry_recalibrate`] publishes.
///
/// Everything but the scale is carried straight off the capture; the scale is
/// [`ui_scale_for_height`]. `anchors` is `None` because a grab has located no
/// module panel, and on the one path that publishes this the standing anchors
/// are already gone — [`geometry_recalibrate`] empties the whole slot before it
/// measures, which is the forgetting half of the press doing exactly what it
/// says. A remembered Entrance origin is re-found by the temple's own recheck;
/// carrying one across a Recalibrate would defeat the button.
///
/// `measured_at_ms` is a parameter rather than a clock read, so the assembly is
/// testable without one — the same reason `record_screen` takes its slot.
pub fn screen_from_geometry(
    width: u32,
    height: u32,
    monitor_id: u32,
    origin: (i32, i32),
    client: [i32; 4],
    measured_at_ms: u64,
) -> ScreenSlice {
    let source = ScreenScaleSource::Capture;
    ScreenSlice {
        width,
        height,
        ui_scale: ui_scale_for_height(height),
        source,
        measured_at_ms,
        // Not a verification and it must not claim to be one: nothing here
        // looked at a pixel of the game's UI.
        verified_this_session: verifies_the_screen(source),
        monitor_id,
        origin,
        client,
        anchors: None,
    }
}

/// [`screen_from_geometry`] applied to a grab, stamped now. The adapter is
/// separate so the arithmetic above never needs a `DynamicImage` to be tested.
pub fn screen_from_capture(capture: &crate::capture::Capture) -> ScreenSlice {
    screen_from_geometry(
        capture.image.width(),
        capture.image.height(),
        capture.monitor_id,
        capture.origin,
        capture.client,
        crate::clock::unix_ms(),
    )
}

/// Forget every remembered geometry measurement and force a fresh one
/// (POE-227) — the Settings **Recalibrate** button.
///
/// The manual arm of the lifecycle contract (`desktop/src/lib/README.md`,
/// "Screen geometry"): a measurement is otherwise re-taken only when the
/// capture's dimensions change or the consuming module's own verification
/// fails. This is what a user presses when neither fired and the numbers are
/// still wrong.
///
/// # It MEASURES, it does not only forget (POE-278)
///
/// Forgetting was the whole of this command until POE-278, and on the machine
/// that needed it most it did nothing a user could see: both writers of the
/// screen slot are owned by a MODULE (`mercenary::run`, `temple::run`), so a
/// player running neither had an empty slot, pressed a button that emptied it
/// again, and kept the `crate::ASSUMED_UI_SCALE` 1080p rects — which on a
/// 3840x2160 screen cut the gem band at a quarter of the size, and which the
/// log line then promised would be "re-measured" by modules that were switched
/// off. A grab of the screen is the one measurement that needs no module and no
/// panel on screen, so this takes one.
///
/// What it establishes is the base every other placement is arithmetic on: the
/// dimensions, the display, the origin, the client rectangle, and a `ui_scale`
/// derived from the height ([`screen_from_geometry`]). Merc and temple still
/// refine the scale from the art whenever they next run, and
/// [`ScreenScaleSource::Capture`]'s [`accepts`] rule guarantees this value
/// yields to them rather than the other way round.
///
/// A capture that fails is not a failed press: the forget half has already
/// happened and is worth keeping (it is what un-sticks a wrong remembered
/// value), so the command logs what went wrong and leaves the slot empty for a
/// module to fill, exactly as it behaved before.
///
/// There is ONE remembered geometry to go (POE-234 WI-2, which retired the
/// temple's separate `temple_calibration` store): the shared screen scale, in
/// its owner and in the file. The temple's hint is derived from this slice
/// through `k` on every tick, so emptying the slice is what un-calibrates the
/// temple — there is no second store left to clear, and none to forget to.
/// The per-key fallback is sufficient because the shared slice is corroborated
/// across modules (ADR-020), while the placed origin is verified on every tick
/// by the temple recheck.
///
/// Clearing it is not enough on its own, because neither module re-measures
/// just because the store is empty:
///
/// - the temple's placed-origin recheck reads the newly empty slice as a null
///   placement, so its one cold-start fallback per `(temple_epoch,
///   temple_rearm)` key can locate the panel again. If that fallback finds an
///   anchor but its proposed slice is withheld, the key is released for one
///   retry; a second withheld sweep keeps it spent until the
///   `(temple_epoch, temple_rearm)` key changes. A successful fallback read
///   remembers the discovered origin through this module. There is no second
///   temple plate-memory store to clear. **Amended 2026-09-11 (POE-275
///   WI-2):** the null-slice fallback is no longer one per key — it sweeps
///   every `NULL_SWEEP_EVERY` consecutive clean misses up to `NULL_SWEEP_CAP`
///   per key, off the loop thread (`temple::run::cold_sweep_reason`,
///   `docs/TEMPLE-LIFECYCLE.md` "Cadences and budgets");
/// - the merc session HOLDS the frame registration it settled on and carries it
///   across every tick that cannot see the frame (`mercenary::run`'s
///   `next_fitted_scale` keeps it, `cellfit::apply_held` re-applies it). The
///   next tick after a Recalibrate would therefore republish that same held
///   number onto this slice and persist it again. `merc_refit` is bumped so
///   that tick drops `session.fitted` first and measures the panel afresh.
///
/// ONE settings write: `settings::persist_forgetting_screen_scale` rebuilds the
/// whole file from `AppState` with the screen scale explicitly emptied. Plain
/// `crate::persist_settings` cannot be used, because its `preserve_screen_scale`
/// merge writes the stale scale back out.
///
/// # Both re-arms are bumped BEFORE the write, and that order is load-bearing
///
/// The merc capture loop runs on its own thread and can tick between any two
/// statements here. A tick that has not yet seen `merc_refit` still holds the
/// registration it settled on, republishes it through [`publish_screen`] and
/// persists it — so a write placed before the bump can have the forgotten
/// number back on disk milliseconds after it was emptied, and the press does
/// nothing the user can see. With the bumps first, the earliest tick that can
/// reach the file is one that has already dropped its held fit and measured the
/// panel afresh; a fresh measurement landing in the file is the outcome
/// Recalibrate is asking for, not the one it is guarding against.
#[tauri::command]
pub fn geometry_recalibrate(app: AppHandle) {
    {
        let state = app.state::<AppState>();
        *state.screen.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    {
        let state = app.state::<AppState>();
        state
            .temple_rearm
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        state
            .merc_refit
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    crate::settings::persist_forgetting_screen_scale(&app);

    // The measuring half. Ordered AFTER the forget for the same reason the
    // re-arms come before the write: what this publishes must land on an empty
    // slot, and `persist_forgetting_screen_scale` is what empties the FILE the
    // `persist_settings` below then rewrites.
    let grab = match crate::capture::capture_screen(&app) {
        Ok(grab) => grab,
        Err(e) => {
            crate::app_log(
                &app,
                format!(
                    "Recalibrate: dropped the remembered screen scale, but could not grab the \
                     screen to measure a new one ({e}) — the next capture of either module \
                     re-measures"
                ),
            );
            emit_ssot(&app);
            return;
        }
    };
    let measured = screen_from_capture(&grab);
    // The record is READ, not discarded, and that is load-bearing here in a way
    // it is not on a detect tick. The window between the clear above and this
    // publish is no longer microseconds: it holds a whole-file settings write
    // AND a full-monitor grab. A merc or temple tick landing inside it fills
    // the slot with a cue that looked at real pixels, and `accepts` then gives
    // this derived value its flat refusal — correctly. Logging "measured" and
    // spending a persist on a refusal would report the base as established when
    // what actually stands is the other measurement, and in the `MercOcr` case
    // — which `from_slice` refuses to store — would leave the FILE empty while
    // claiming success, i.e. the exact POE-278 state on the next restart.
    let record = publish_screen(&app, measured);
    if !record.accepted {
        crate::app_log(
            &app,
            format!(
                "Recalibrate: dropped the remembered screen scale, and a module measured this \
                 screen before the grab finished — its ui scale {:.3} stands instead of the \
                 {:.3} derived from the screen height",
                record.standing_ui_scale, measured.ui_scale
            ),
        );
        return;
    }
    // The press is the write: `should_remember_screen` is the DETECT TICK's
    // trigger and deliberately does not name this cue, while
    // `settings::ScreenScaleSetting::from_slice` — the normative answer to what
    // may be stored — does. Without this the base would have to be re-derived
    // by a second press after every restart.
    crate::persist_settings(&app);
    crate::app_log(
        &app,
        format!(
            "Recalibrate: measured {}x{} on monitor {} — ui scale {:.3} derived from the \
             screen height; merc and temple refine it from the game's art when they next run",
            measured.width, measured.height, measured.monitor_id, measured.ui_scale
        ),
    );
}

/// Emit `ssot-changed` with the current snapshot.
///
/// Optional eager nudge for the main window; overlay windows poll `get_ssot`
/// instead. Lock-then-emit discipline: the `ssot` guard is scoped to a block
/// that ends **before** `app.emit(...)`, so the mutex is never held across the
/// emit call (mirrors `emit_logs` and the lab_state pattern in lib.rs).
///
/// Called by `apply_league` after every SSOT mutation.
pub fn emit_ssot(app: &AppHandle) {
    let snapshot = {
        let state = app.state::<AppState>();
        build_snapshot(&state)
    };
    if let Err(e) = app.emit("ssot-changed", snapshot) {
        log::warn!("emit ssot-changed failed: {}", e);
    }
}

/// Poll target for overlay windows: compose the snapshot from the stored slice
/// plus the market fields, and return it. Serialized to the webview.
#[tauri::command]
pub fn get_ssot(state: tauri::State<AppState>) -> AppSsotSnapshot {
    build_snapshot(&state)
}

/// Mutate the sole holder and return a consumer event only for a real change.
fn write_league(
    ssot: &std::sync::Mutex<AppSsotSnapshot>,
    name: String,
) -> Option<LeagueChanged> {
    let mut guard = ssot.lock().unwrap_or_else(|e| e.into_inner());
    let changed = guard.league.name.as_ref().is_some_and(|old| old != &name);
    if guard.league.name.as_ref() != Some(&name) {
        guard.league.generation += 1;
    }
    guard.league.name = Some(name.clone());
    guard.resolving = false;
    guard.unreachable = false;
    changed.then(|| LeagueChanged { league: name, generation: guard.league.generation })
}

/// UI state describes an active request/retry window, not task liveness.
fn mark_resolving(ssot: &std::sync::Mutex<AppSsotSnapshot>) {
    ssot.lock().unwrap_or_else(|e| e.into_inner()).resolving = true;
}

/// Trim an incoming league name; return `Some(trimmed)` only when non-empty.
///
/// The single fail-closed gate at the mutation seam: a blank/whitespace name is
/// rejected (`None`) so `set_league` — the advertised user-choice sink — can
/// never write `Some("")` and defeat the fail-closed contract. Pure so it is
/// directly unit-testable without an `AppHandle`.
fn normalize_league(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Write via the sole holder, then deliver the snapshot before a change event.
///
/// Blank/whitespace names are rejected here (see `normalize_league`) so
/// fail-closed cannot be defeated at the seam: nothing changed, so no state
/// is written and no `ssot-changed` nudge is emitted. Locks are taken and
/// dropped inside `write_league`; `emit_ssot` re-locks briefly to clone the
/// snapshot. Nothing is held across the emit.
fn apply_league(app: &AppHandle, name: String) {
    let Some(name) = normalize_league(&name) else {
        // Blank/whitespace: reject before `write_league`, so `resolving` is left
        // untouched (not cleared). A manual `set_league("")` is not the retry
        // loop and must not flip the loop's flag; the loop clears `resolving`
        // itself when it lands a real name. Nothing is written, nothing emitted.
        return;
    };
    let changed = {
        let state = app.state::<AppState>();
        write_league(&state.ssot, name)
    };
    publish_league_resolution(app, changed);
}

fn publish_league_resolution(app: &AppHandle, changed: Option<LeagueChanged>) {
    // The temple market poll keys every payload on this name (POE-258), so a
    // league landing is the moment its first poll can succeed. Woken
    // unconditionally rather than on a CHANGE: the two servers behind the
    // DEBUG/PROD toggle usually name the same league, and that switch still has
    // to re-fetch, because the prices came from the other one.
    deliver_league_resolution(changed, || TEMPLE_MARKET_NOTIFY.notify_one(), || emit_ssot(app), |changed| {
        if let Err(error) = app.emit("league-changed", changed) {
            log::warn!("emit league-changed failed: {}", error);
        }
    });
}

fn deliver_league_resolution(
    changed: Option<LeagueChanged>,
    wake_market: impl FnOnce(),
    snapshot: impl FnOnce(),
    consumer_change: impl FnOnce(LeagueChanged),
) {
    wake_market();
    snapshot();
    if let Some(changed) = changed { consumer_change(changed); }
}

/// Extract the league name from a `/api/analysis/status` JSON body.
///
/// Returns `None` when the `league` field is absent, non-string, or blank so
/// the caller **fails closed** (the server omits `league` entirely when its
/// cache is not yet populated — see `AnalysisStatus`). A `None` here leaves the
/// SSOT unresolved and the trade client failing every lookup, by design.
pub(crate) fn parse_league_from_status(body: &serde_json::Value) -> Option<String> {
    body.get("league")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// One league fetch attempt. Return true only for an accepted status response;
/// failures and superseded server requests retry. Clone the HTTP client and
/// request identity before awaiting, with no lock across the round-trip.
async fn fetch_league_once(app: &AppHandle) -> bool {
    let (server_url, http, url_generation) = {
        let state = app.state::<AppState>();
        let url = state
            .server_url
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let generation = LEAGUE_SCHEDULER.task.lock().unwrap_or_else(|e| e.into_inner()).url_generation;
        (url, state.server_http.clone(), generation)
    };
    let Some(name) = fetch_league_from_server(&http, &server_url).await else {
        return false;
    };
    let changed = {
        let state = app.state::<AppState>();
        let current_url = state.server_url.lock().unwrap_or_else(|e| e.into_inner());
        let task = LEAGUE_SCHEDULER.task.lock().unwrap_or_else(|e| e.into_inner());
        let Some(changed) = accept_league_response(
            &state.ssot, (&current_url, task.url_generation), (&server_url, url_generation), name,
        ) else {
            return false;
        };
        changed
    };
    publish_league_resolution(app, changed);
    true
}

/// The URL owner stays locked through this synchronous acceptance step.
fn accept_league_response(
    ssot: &std::sync::Mutex<AppSsotSnapshot>,
    current: (&str, u64),
    requested: (&str, u64),
    name: String,
) -> Option<Option<LeagueChanged>> {
    if current != requested { return None; }
    Some(write_league(ssot, name))
}

async fn fetch_league_from_server(http: &reqwest::Client, server_url: &str) -> Option<String> {
    let url = format!("{}/api/analysis/status", server_url);

    let resp = match http.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("league fetch: request to {} failed: {}", url, e);
            return None;
        }
    };
    if !resp.status().is_success() {
        log::warn!("league fetch: {} returned {}", url, resp.status());
        return None;
    }
    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("league fetch: bad JSON from {}: {}", url, e);
            return None;
        }
    };
    parse_league_from_status(&body)
}

/// Exercise Refresh's authoritative status read without constructing windows.
#[cfg(test)]
pub(crate) async fn refresh_league_for_test(
    server_url: &str,
    ssot: &std::sync::Mutex<AppSsotSnapshot>,
) {
    mark_resolving(ssot);
    let league = fetch_league_from_server(&reqwest::Client::new(), server_url)
        .await.expect("Refresh must resolve the stub's league");
    write_league(ssot, league);
}

/// Inject network and clock at the production loop boundary. Trigger
/// bookkeeping and Notify waiting remain shared with the application.
trait LeagueRuntime {
    fn scheduler(&self) -> &LeagueScheduler;
    fn begin(&self);
    async fn fetch(&self) -> bool;
    fn failed(&self, failures: u32);
    async fn sleep(&self, delay: Duration);
}

async fn run_league_resolver(runtime: &impl LeagueRuntime) {
    let mut backoff = LEAGUE_FETCH_INITIAL_BACKOFF;
    let mut failures = 0;
    loop {
        runtime.scheduler().begin_request();
        runtime.begin();
        if runtime.fetch().await {
            failures = 0;
            backoff = LEAGUE_FETCH_INITIAL_BACKOFF;
            runtime.scheduler().wait(None, runtime).await;
        } else {
            failures += 1;
            runtime.failed(failures);
            if runtime.scheduler().wait(Some(backoff), runtime).await {
                backoff = LEAGUE_FETCH_INITIAL_BACKOFF;
            } else {
                backoff = (backoff * 2).min(LEAGUE_FETCH_MAX_BACKOFF);
            }
        }
    }
}

struct AppLeagueRuntime(AppHandle);

impl LeagueRuntime for AppLeagueRuntime {
    fn scheduler(&self) -> &LeagueScheduler { &LEAGUE_SCHEDULER }

    fn begin(&self) {
        let state = self.0.state::<AppState>();
        mark_resolving(&state.ssot);
        emit_ssot(&self.0);
    }

    async fn fetch(&self) -> bool {
        fetch_league_once(&self.0).await
    }

    fn failed(&self, failures: u32) {
        if should_flag_unreachable(failures) {
            let changed = {
                let state = self.0.state::<AppState>();
                let mut snapshot = state.ssot.lock().unwrap_or_else(|e| e.into_inner());
                let changed = !snapshot.unreachable;
                snapshot.unreachable = true;
                changed
            };
            if changed { emit_ssot(&self.0); }
        }
    }

    async fn sleep(&self, delay: Duration) {
        tokio::time::sleep(delay).await;
    }
}

/// Start one live service. After success it waits for a trigger, never a poll.
pub fn spawn_league_fetch(app: AppHandle) {
    let spawn = LEAGUE_SCHEDULER.trigger();
    if !spawn { return; }
    tauri::async_runtime::spawn(async move {
        run_league_resolver(&AppLeagueRuntime(app)).await;
    });
}

/// Retained POE-126 command seam; no user-choice UI currently calls it.
#[tauri::command]
pub fn set_league(name: String, app: AppHandle) {
    // The server remains authoritative: an in-flight response can replace this
    // explicit mutation. User selection semantics remain deferred to POE-126.
    {
        let state = app.state::<AppState>();
        let in_flight = state.ssot.lock().unwrap_or_else(|e| e.into_inner()).resolving;
        if in_flight {
            log::warn!(
                "set_league({:?}) during resolution; the server response may replace this value (POE-126)",
                name
            );
        }
    }
    apply_league(&app, name);
}

/// Settings Refresh wakes the shared service, including during retry backoff.
#[tauri::command]
pub fn refresh_league(app: AppHandle) {
    rearm_league(app);
}

/// Window-independent access to the existing resolver's state and scheduling.
/// Tests observe the scheduling boundary without replacing hint dispatch.
pub(crate) trait LeagueResolver {
    fn current_league(&self) -> Option<String>;
    fn rearm(self);
}

impl LeagueResolver for AppHandle {
    fn current_league(&self) -> Option<String> {
        let state = self.state::<AppState>();
        let snapshot = state.ssot.lock().unwrap_or_else(|e| e.into_inner());
        snapshot.league.name.clone()
    }

    fn rearm(self) {
        rearm_league(self);
    }
}

/// Server hints only trigger a status read; they never relabel captured prices.
pub(crate) fn note_server_league(resolver: impl LeagueResolver, observed: Option<&str>) {
    let current = resolver.current_league();
    if observed.is_none() || observed != current.as_deref() {
        resolver.rearm();
    }
}

pub mod league_command {
    use super::*;

    #[tauri::command]
    pub fn note_server_league(app: AppHandle, observed: Option<String>) {
        super::note_server_league(app, observed.as_deref());
    }
}

/// Resolve the league again, now.
///
/// Startup, Settings Refresh, URL changes, write 409s and Mercure hints share
/// one scheduling gate. Liveness stays true through both idle and backoff.
pub(crate) fn rearm_league(app: AppHandle) {
    spawn_league_fetch(app);
}

/// Supersede any league fetch in flight: its answer came from the server the
/// app is leaving. The first step of a server-url change (`lib.rs`).
pub(crate) fn supersede_league_fetches() {
    LEAGUE_SCHEDULER.task.lock().unwrap_or_else(|e| e.into_inner()).url_generation += 1;
}

/// Wake signal for the market poller: poll NOW rather than at the next tick.
///
/// Notified when the league resolves (the poll cannot key a payload before
/// then, so the first one would otherwise wait out a whole period) and when the
/// server url changes. A module-local `static` for the reason [`LEAGUE_SCHEDULER`]
/// is one: there is exactly one poller, woken here by `publish_league_resolution`,
/// awaited by `temple::market::spawn_temple_market_poll`.
pub(crate) static TEMPLE_MARKET_NOTIFY: LazyLock<Notify> = LazyLock::new(Notify::new);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn league_name_reads_the_resolved_league() {
        let state = crate::settings::tests::test_app_state();
        state.ssot.lock().unwrap().league.name = Some("Mirage".into());
        assert_eq!(league_name(&state), Some("Mirage".into()));
    }

    #[test]
    fn league_name_is_none_before_resolution() {
        assert_eq!(league_name(&crate::settings::tests::test_app_state()), None);
    }

    /// Locks the fail-closed default: a fresh snapshot reports the league as
    /// not-yet-fetched (`None`), never a spuriously "valid" empty name.
    #[test]
    fn default_snapshot_league_is_unfetched() {
        let snap = AppSsotSnapshot::default();
        assert_eq!(snap.league.name, None);
    }

    /// The resolved identity belongs to the SSOT slice alone.
    #[test]
    fn write_league_updates_the_ssot_holder() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        assert_eq!(ssot.lock().unwrap().league.name, None);

        write_league(&ssot, "Mirage".to_string());

        assert_eq!(
            ssot.lock().unwrap().league.name,
            Some("Mirage".to_string()),
            "SSOT slice must hold the resolved league",
        );
        assert_eq!(ssot.lock().unwrap().league.generation, 1);
    }

    /// Single-flight contract: the first caller acquires the resolve right and
    /// marks `resolving`; a second caller is refused while a resolve is in
    /// flight, so repeated `refresh_league` clicks can't stack duplicate loops.
    #[test]
    fn resolver_liveness_is_independent_of_resolving_ui() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        let mut task = LeagueTask::default();
        assert!(task.trigger());
        mark_resolving(&ssot);
        write_league(&ssot, "A".into());
        assert!(!ssot.lock().unwrap().resolving);
        assert!(!task.trigger(), "idle task retains the single-flight claim");
    }

    /// The resolve path clears `resolving` in the same write that sets the
    /// league: starting from an in-flight snapshot, `write_league` must land the
    /// league AND drop the flag, so a poller never sees league-set + resolving.
    #[test]
    fn write_league_clears_resolving_when_it_sets_the_league() {
        let ssot = Mutex::new(AppSsotSnapshot {
            resolving: true,
            ..Default::default()
        });
        // Precondition: resolve in flight, league not yet written.
        assert!(ssot.lock().unwrap().resolving);
        assert_eq!(ssot.lock().unwrap().league.name, None);

        write_league(&ssot, "Mirage".to_string());

        let guard = ssot.lock().unwrap();
        assert_eq!(
            guard.league.name,
            Some("Mirage".to_string()),
            "league must be written",
        );
        assert!(
            !guard.resolving,
            "resolving must be cleared in the same write that sets the league",
        );
    }

    /// A blank/whitespace league name is rejected at the seam: `normalize_league`
    /// returns `None`, so the holder stays fail-closed.
    /// Guards the `set_league`/`refresh_league` user-choice sink that skips the
    /// server-side `parse_league_from_status` filter.
    #[test]
    fn blank_league_at_seam_leaves_holder_unresolved() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        assert_eq!(ssot.lock().unwrap().league.name, None);

        // The guard `apply_league` applies before writing. Whitespace-only.
        match normalize_league("   ") {
            None => {} // rejected — no write, exactly what `apply_league` does.
            Some(n) => { write_league(&ssot, n); }
        }

        assert_eq!(
            ssot.lock().unwrap().league.name,
            None,
            "SSOT slice must stay unresolved after a blank set_league",
        );
        assert_eq!(ssot.lock().unwrap().league.generation, 0);
    }

    /// The pure seam gate: whitespace-only trims to empty and is rejected,
    /// while a padded real name is accepted and trimmed. Locks the exact
    /// blank-rejection contract `apply_league` relies on.
    #[test]
    fn normalize_league_rejects_blank_and_trims_real() {
        assert_eq!(normalize_league(""), None);
        assert_eq!(normalize_league("   "), None);
        assert_eq!(normalize_league("\t\n"), None);
        assert_eq!(normalize_league("  Settlers  "), Some("Settlers".to_string()));
    }

    /// The sole success-exit mutation drops BOTH flags. Starting from an
    /// in-flight, unreachable snapshot (the exact state a recovered-after-down
    /// server exits through), `write_league` must leave `resolving` and
    /// `unreachable` both false — asserted on real post-state, not a return code.
    #[test]
    fn successful_resolution_drops_both_flags() {
        let ssot = Mutex::new(AppSsotSnapshot {
            resolving: true,
            unreachable: true,
            ..Default::default()
        });

        write_league(&ssot, "Mirage".into());

        let guard = ssot.lock().unwrap();
        assert!(!guard.resolving, "resolving must be cleared on success");
        assert!(!guard.unreachable, "unreachable must be cleared on success");
    }

    /// Threshold boundary for the unreachable flip: below the constant stays
    /// false, at/above flips true. Locks the grace window so a transient blip of
    /// a few failures does not flag the server down.
    #[test]
    fn should_flag_unreachable_at_threshold_boundary() {
        assert!(!should_flag_unreachable(0));
        assert!(!should_flag_unreachable(LEAGUE_UNREACHABLE_AFTER_ATTEMPTS - 1));
        assert!(should_flag_unreachable(LEAGUE_UNREACHABLE_AFTER_ATTEMPTS));
        assert!(should_flag_unreachable(LEAGUE_UNREACHABLE_AFTER_ATTEMPTS + 1));
    }

    #[test]
    fn parse_league_pulls_league_field() {
        let body = serde_json::json!({ "cached": true, "league": "Mirage" });
        assert_eq!(parse_league_from_status(&body), Some("Mirage".to_string()));
    }

    #[test]
    fn parse_league_missing_field_is_unresolved() {
        // Server omits `league` entirely when its cache is empty.
        let body = serde_json::json!({ "cached": false, "transfigure": 0 });
        assert_eq!(parse_league_from_status(&body), None);
    }

    #[test]
    fn parse_league_blank_field_is_unresolved() {
        // A blank/whitespace league must fail closed, not resolve to "".
        assert_eq!(
            parse_league_from_status(&serde_json::json!({ "league": "" })),
            None,
        );
        assert_eq!(
            parse_league_from_status(&serde_json::json!({ "league": "   " })),
            None,
        );
    }

    #[test]
    fn parse_league_non_string_field_is_unresolved() {
        assert_eq!(
            parse_league_from_status(&serde_json::json!({ "league": 42 })),
            None,
        );
    }

    /// The composition contract: every one of the three market values lands in
    /// the returned snapshot, each in its own field. Distinct values so a
    /// crossed assignment (pool into variant, etc.) fails instead of passing.
    #[test]
    fn compose_snapshot_writes_all_three_market_values() {
        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "1/20".to_string(),
            "21/20".to_string(),
            "transfigured".to_string(),
            std::collections::HashMap::new(),
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.normal_variant, "1/20");
        assert_eq!(out.dedication_variant, "21/20");
        assert_eq!(out.dedication_pool, "transfigured");
    }

    /// The module map is projected into the snapshot unchanged. Without this
    /// the field falls back to `..base` (always the empty default), and every
    /// window would poll an empty `modules` slice forever.
    #[test]
    fn compose_snapshot_projects_the_module_map() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), true)].into_iter().collect();

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.modules.get("mercenary"), Some(&true));
    }

    /// The mercenary slice is projected through composition while the module
    /// is enabled: the loop owns `live`/`idle`/`unavailable`, and the composer
    /// must not overwrite them. Without the projection the field falls back to
    /// `..base` (always the `Off` default) and the page would poll a
    /// permanently-off slice while a capture was on screen.
    #[test]
    fn compose_snapshot_projects_the_mercenary_slice_while_the_module_is_on() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), true)].into_iter().collect();
        let slice = crate::mercenary::MercenarySlice {
            burst_speaker: Some("Fennik, of Unshakeable Faith".to_string()),
            learned_families: vec!["Chain--2".to_string()],
            geometry_source: "file".to_string(),
            ..Default::default()
        };

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            slice,
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(
            out.mercenary.burst_speaker,
            Some("Fennik, of Unshakeable Faith".to_string())
        );
        assert_eq!(out.mercenary.learned_families, vec!["Chain--2".to_string()]);
        assert_eq!(out.mercenary.geometry_source, "file");
    }

    /// A module switched off mid-capture is forced off by the composer, or
    /// the page keeps rendering a live verdict for a loop that has stopped.
    /// What the force-off clears is `MercenarySlice::force_off`'s own tests;
    /// this one pins that the composer applies it.
    #[test]
    fn a_disabled_mercenary_module_is_forced_off() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), false)].into_iter().collect();
        let slice = crate::mercenary::MercenarySlice {
            burst_speaker: Some("Fennik, of Unshakeable Faith".to_string()),
            ..Default::default()
        };

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            slice,
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(
            out.mercenary.burst_speaker, None,
            "the speaker belongs to a scan that is no longer running",
        );
    }

    /// The enabled-guide set is what the page and the overlay both evaluate
    /// against (POE-199 L5), so it must reach the snapshot from the OWNER —
    /// not from the stored slice, which nothing ever writes it into.
    #[test]
    fn compose_snapshot_echoes_the_enabled_guide_set_onto_the_mercenary_slice() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), true)].into_iter().collect();

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            vec!["guide-a".to_string()],
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.mercenary.sources_off, vec!["guide-a".to_string()]);
    }

    /// The echo survives the module being switched off, for the reason the
    /// temple's `config`/`profile` echo does: it is what the USER set, and the
    /// page renders its guide toggles from it while the module is off
    /// (ADR-014 — the page reads the slice, never `ssot.modules`).
    #[test]
    fn a_disabled_module_keeps_the_enabled_guide_set() {
        let modules: std::collections::HashMap<String, bool> =
            [("mercenary".to_string(), false)].into_iter().collect();

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            vec!["guide-b".to_string()],
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.mercenary.sources_off, vec!["guide-b".to_string()]);
    }

    /// A module map that does not mention the module yet (nothing has written
    /// the owner map) reads as OFF, not as on. Fail-closed: claiming a module
    /// is running when its enablement is unknown is the worse lie.
    #[test]
    fn an_unknown_mercenary_module_flag_is_forced_off() {
        let slice = crate::mercenary::MercenarySlice {
            burst_speaker: Some("Fennik, of Unshakeable Faith".to_string()),
            ..Default::default()
        };

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            std::collections::HashMap::new(),
            slice,
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.mercenary.burst_speaker, None);
    }

    /// The temple slice is projected through composition while the module is
    /// enabled. Without the projection the field falls back to `..base` (the
    /// empty default) and every overlay would poll a boardless slice while a
    /// board was on screen.
    #[test]
    fn compose_snapshot_projects_the_temple_slice_while_the_module_is_on() {
        let modules: std::collections::HashMap<String, bool> =
            [("temple".to_string(), true)].into_iter().collect();
        let slice = crate::temple::slice::TempleSlice {
            unknown_rooms: vec!["A0".to_string()],
            mode: Some("chase".to_string()),
            ..Default::default()
        };

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            slice,
            None,
        );

        assert_eq!(out.temple.unknown_rooms, vec!["A0".to_string()]);
        assert_eq!(out.temple.mode, Some("chase".to_string()));
    }

    /// A disabled temple module is forced off by the composer, so the page
    /// cannot render a stale recommendation under an off badge. What the
    /// force-off clears is `TempleSlice::force_off`'s own tests; this one pins
    /// that the composer applies it.
    #[test]
    fn a_disabled_temple_module_is_forced_off() {
        let modules: std::collections::HashMap<String, bool> =
            [("temple".to_string(), false)].into_iter().collect();
        let slice = crate::temple::slice::TempleSlice {
            mode: Some("chase".to_string()),
            unknown_rooms: vec!["A0".to_string()],
            ..Default::default()
        };

        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            slice,
            None,
        );

        assert_eq!(out.temple.mode, None);
    }

    /// A module map that does not mention the temple module yet reads as OFF,
    /// not as on. Fail-closed, same as the merc slice.
    #[test]
    fn an_unknown_temple_module_flag_is_forced_off() {
        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            std::collections::HashMap::new(),
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice {
                mode: Some("chase".to_string()),
                ..Default::default()
            },
            None,
        );

        assert_eq!(out.temple.mode, None);
    }

    /// The snapshot's own key for the temple slice, which the TS store reads.
    /// Dropping `rename_all` on either struct silently breaks the store.
    #[test]
    fn the_snapshot_exposes_the_temple_slice_under_its_camel_case_keys() {
        let json = serde_json::to_value(AppSsotSnapshot::default()).unwrap();

        assert_eq!(json["temple"]["status"], "idle");
        assert_eq!(json["temple"]["layout"], serde_json::Value::Null);
        assert_eq!(json["temple"]["unknownRooms"], serde_json::json!([]));
        assert_eq!(json["temple"]["lastReadAt"], serde_json::Value::Null);
    }

    /// The snapshot's own key for the slice, which the TS store reads.
    #[test]
    fn the_snapshot_exposes_the_mercenary_slice_under_its_camel_case_key() {
        let json = serde_json::to_value(AppSsotSnapshot::default()).unwrap();

        assert_eq!(json["mercenary"]["status"], "off");
        assert_eq!(json["mercenary"]["geometrySource"], "default");
        assert_eq!(json["mercenary"]["capture"], serde_json::Value::Null);
    }

    /// Composition must not disturb the league slice it wraps: the stored
    /// `AppState.ssot` fields survive the market overlay untouched, so a poll
    /// never trades a resolved league for a market selection.
    #[test]
    fn compose_snapshot_preserves_the_stored_league_slice() {
        let base = AppSsotSnapshot {
            league: LeagueSlice { name: Some("Mirage".to_string()), generation: 1 },
            resolving: true,
            unreachable: true,
            ..Default::default()
        };

        let out = compose_snapshot(
            base,
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            std::collections::HashMap::new(),
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        assert_eq!(out.league.name, Some("Mirage".to_string()));
        assert!(out.resolving, "resolving must survive composition");
        assert!(out.unreachable, "unreachable must survive composition");
    }

    /// Wire contract: the TypeScript store reads camelCase keys, and the Rust
    /// field names are snake_case — the `rename_all` attribute is what bridges
    /// them. Dropping it renames these keys and silently breaks the store.
    #[test]
    fn snapshot_serializes_market_fields_as_camel_case() {
        let snap = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/0".to_string(),
            "21/20".to_string(),
            "transfigured".to_string(),
            std::collections::HashMap::new(),
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice::default(),
            None,
        );

        let json = serde_json::to_value(&snap).unwrap();

        assert_eq!(json.get("normalVariant").and_then(|v| v.as_str()), Some("20/0"));
        assert_eq!(json.get("dedicationVariant").and_then(|v| v.as_str()), Some("21/20"));
        assert_eq!(json.get("dedicationPool").and_then(|v| v.as_str()), Some("transfigured"));
    }

    /// The same `rename_all` must leave the pre-existing keys alone — they are
    /// single words, so camelCase is a no-op on them. The store already reads
    /// `league` / `resolving` / `unreachable` by these exact names.
    #[test]
    fn snapshot_serialization_keeps_the_existing_league_keys() {
        let snap = AppSsotSnapshot {
            league: LeagueSlice { name: Some("Mirage".to_string()), generation: 1 },
            resolving: true,
            unreachable: true,
            ..Default::default()
        };

        let json = serde_json::to_value(&snap).unwrap();

        assert_eq!(
            json.get("league").and_then(|v| v.get("name")).and_then(|v| v.as_str()),
            Some("Mirage"),
        );
        assert_eq!(json.get("resolving").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(json.get("unreachable").and_then(|v| v.as_bool()), Some(true));
    }

    /// Fail-closed default: a snapshot that was never composed from `AppState`
    /// carries empty markets, never a plausible-looking "20/20". The webview
    /// treats empty as unknown, so a default can't be mistaken for a real
    /// selection.
    #[test]
    fn default_snapshot_has_no_market_selection() {
        let snap = AppSsotSnapshot::default();

        assert_eq!(snap.normal_variant, "");
        assert_eq!(snap.dedication_variant, "");
        assert_eq!(snap.dedication_pool, "");
    }

    // --------------------------------------------------------- screen slice --

    /// The reference measurement, as a starting point every screen test edits
    /// one field of. 1920x1200 at 1.0 IS the reference fixture (POE-214 D3), so
    /// a test that changes nothing is asking about a real, whole screen.
    fn reference_screen() -> ScreenSlice {
        ScreenSlice {
            width: 1920,
            height: 1200,
            ui_scale: 1.0,
            source: ScreenScaleSource::MercFrame,
            measured_at_ms: 1_700_000_000_000,
            verified_this_session: true,
            // A REAL handle, not the `0` sentinel, so a test that reads it back
            // can tell a carried handle from an unknown one.
            monitor_id: REFERENCE_MONITOR,
            origin: REFERENCE_ORIGIN,
            client: [0, 0, 1920, 1200],
            anchors: None,
        }
    }

    /// The display [`reference_screen`] was measured on, and the id every test
    /// that means "the same monitor" passes.
    const REFERENCE_MONITOR: u32 = 65_537;

    /// A second handle, same id space. Only its DIFFERENCE from
    /// [`REFERENCE_MONITOR`] matters.
    const OTHER_MONITOR: u32 = 131_074;

    /// Where [`reference_screen`]'s display sits on the virtual desktop: the
    /// primary's top-left.
    const REFERENCE_ORIGIN: (i32, i32) = (0, 0);

    /// A second display of the same desktop, to the primary's right. Two
    /// displays cannot share a top-left, so this is what "another display" is.
    const OTHER_ORIGIN: (i32, i32) = (1920, 0);

    /// A measurement as the merc writer builds one: the flag is never a literal
    /// at a call site, it is [`verifies_the_screen`]'s answer for the cue. Used
    /// by every test that asks what the SLOT ends up holding, so a change to
    /// that rule reaches those assertions instead of being papered over by a
    /// hand-written `true`.
    fn measured_by(source: ScreenScaleSource, ui_scale: f32) -> ScreenSlice {
        ScreenSlice {
            ui_scale,
            source,
            verified_this_session: verifies_the_screen(source),
            ..reference_screen()
        }
    }

    /// Fail-closed default: a snapshot nothing has measured a screen for says
    /// so, rather than carrying the reference 1.0. A consumer that took a
    /// default as a measurement would scale every rect 11% wrong on a 1080p
    /// machine and have no way to know it.
    #[test]
    fn default_snapshot_has_no_screen_measurement() {
        let snap = AppSsotSnapshot::default();

        assert!(snap.screen.is_none(), "a default snapshot has measured nothing");
    }

    /// The owner's value is projected into the snapshot. Without the projection
    /// the field falls back to `..base` (always `None`, since nothing writes it
    /// into `AppState.ssot`) and every window would poll an unmeasured screen
    /// forever while the merc loop was publishing one.
    #[test]
    fn compose_snapshot_projects_the_screen_slice() {
        let out = compose_snapshot(
            AppSsotSnapshot::default(),
            "20/20".to_string(),
            "21/23".to_string(),
            "skill".to_string(),
            std::collections::HashMap::new(),
            crate::mercenary::MercenarySlice::default(),
            Vec::new(),
            crate::mercenary::sync::MercSyncStatus::default(),
            crate::mercenary::MercenarySlice::default().trade_auto,
            crate::mercenary::MercenarySlice::default().tier_floor,
            crate::temple::slice::TempleSlice::default(),
            Some(ScreenSlice { ui_scale: 0.9, ..reference_screen() }),
        );

        let screen = out.screen.expect("the composed snapshot carries the measurement");
        assert_eq!(screen.ui_scale, 0.9);
        assert_eq!(screen.source, ScreenScaleSource::MercFrame);
        assert_eq!(out.placements.lab.gem, [0, 0, 1920, 40]);
        assert_eq!(
            out.placements.temple.expect("a measured screen has a temple placement").entrance_origin,
            (960, 713),
        );
    }

    #[test]
    fn unmeasured_placements_keep_the_1080p_lab_assumption_only() {
        let placements = placements_for(None);

        assert_eq!(placements.temple, None);
        assert_eq!(placements.merc, None);
        assert_eq!(placements.lab.gem, [0, 0, 1920, 40]);
        // The font rule at 1080p: round(740 · 0.9) = 666 wide, centred in the
        // 1262 px left of the inventory → x = (1262 − 666) / 2 = 298; the top at
        // round(244 · 0.9 = 219.6) = 220, round(757 · 0.9 = 681.3) = 681 tall.
        assert_eq!(placements.lab.font, [298, 220, 666, 681]);
    }

    // ------------------------------------ capture-derived base (POE-278) --

    /// The unit's definition, at the height it was defined on. `ASSUMED_UI_SCALE`
    /// is documented as "this function evaluated at 1080 and frozen", and this
    /// is what holds the two to that claim — a changed denominator breaks it.
    #[test]
    fn the_derived_scale_at_1080p_is_the_constant_it_replaced() {
        assert_eq!(ui_scale_for_height(1080), crate::ASSUMED_UI_SCALE);
    }

    /// The screen POE-278 was filed on. The frozen constant answers 0.90 here
    /// whatever the screen is; the rule answers what 2160 px of height implies.
    #[test]
    fn the_derived_scale_on_a_4k_screen_is_twice_the_1080p_one() {
        assert_eq!(ui_scale_for_height(2160), 1.8);
    }

    /// A screen that measured nothing derives `0.0`, which `valid_scale` is what
    /// refuses. Locked because the alternative — deriving some non-zero
    /// stand-in here — would put an unmeasured screen past the one check that
    /// exists to catch it.
    #[test]
    fn a_zero_height_derives_a_zero_scale() {
        assert_eq!(ui_scale_for_height(0), 0.0);
    }

    /// The whole point of the change, stated as the rect a 4K player gets. The
    /// pre-POE-278 answer was `[0, 0, 1920, 40]` — the top-left QUARTER of this
    /// band — because the lab fell back to a hard 1080p client and a frozen
    /// scale whatever the screen was.
    #[test]
    fn a_4k_capture_places_the_gem_band_across_the_whole_screen() {
        let screen = screen_from_geometry(3840, 2160, 77, (0, 0), [0, 0, 3840, 2160], 1_700_000_000_000);

        let placements = placements(&screen);

        // The client's full 3840 width, round(44 · 1.8 = 79.2) = 79 tall.
        assert_eq!(placements.lab.gem, [0, 0, 3840, 79]);
        // 740 · 1.8 = 1332 wide, centred in the 3840 − round(731 · 1.8) = 2524
        // px left of the inventory → x = (2524 − 1332) / 2 = 596; top
        // round(244 · 1.8 = 439.2) = 439, round(757 · 1.8 = 1362.6) = 1363 tall.
        assert_eq!(placements.lab.font, [596, 439, 1332, 1363]);
    }

    /// The derived slice carries the display it was grabbed off, so the lazy
    /// prune and the overlay placement have the same identity to work with that
    /// a module-measured slice would have given them.
    #[test]
    fn a_derived_slice_carries_the_captures_display_and_client() {
        let screen = screen_from_geometry(3840, 2160, 77, (-1920, 120), [0, 0, 3840, 2160], 42);

        assert_eq!(screen.width, 3840);
        assert_eq!(screen.height, 2160);
        assert_eq!(screen.monitor_id, 77);
        assert_eq!(screen.origin, (-1920, 120));
        assert_eq!(screen.client, [0, 0, 3840, 2160]);
        assert_eq!(screen.measured_at_ms, 42);
        assert_eq!(screen.source, ScreenScaleSource::Capture);
    }

    /// Derived is not verified: nothing here looked at a pixel of the game's UI,
    /// and the Settings card's confidence row reads this flag.
    #[test]
    fn a_derived_slice_claims_no_verification() {
        let screen = screen_from_geometry(3840, 2160, 77, (0, 0), [0, 0, 3840, 2160], 42);

        assert!(!screen.verified_this_session);
    }

    /// A grab has located no panel, so it must not invent an anchor — a seeded
    /// origin presented as a remembered one would place the temple on arithmetic
    /// while claiming a plate had been found.
    #[test]
    fn a_derived_slice_remembers_no_module_anchors() {
        let screen = screen_from_geometry(1920, 1080, 77, (0, 0), [0, 0, 1920, 1080], 42);

        assert_eq!(screen.anchors, None);
    }

    /// The press has to land, and it lands on the slot its own first step
    /// emptied.
    #[test]
    fn a_derived_measurement_fills_an_empty_slot() {
        let mut slot = None;

        let record = record_screen(&mut slot, screen_from_geometry(3840, 2160, 77, (0, 0), [0, 0, 3840, 2160], 42));

        assert!(record.accepted);
        assert_eq!(slot.expect("the press measured the screen").ui_scale, 1.8);
    }

    /// The yield rule. A gold-frame fit that disagrees with the derivation is
    /// the case where the derivation's assumption is wrong on this screen, so
    /// the cue that read the art must survive the arithmetic — a band-limited
    /// rule would let exactly this reading through, because the disagreement is
    /// what makes it exceed the band.
    #[test]
    fn a_derived_measurement_never_displaces_a_measured_one() {
        let standing = ScreenSlice {
            width: 3840,
            height: 2160,
            ui_scale: 1.5,
            client: [0, 0, 3840, 2160],
            ..reference_screen()
        };
        let mut slot = Some(standing);

        let record = record_screen(
            &mut slot,
            screen_from_geometry(3840, 2160, REFERENCE_MONITOR, (0, 0), [0, 0, 3840, 2160], 99),
        );

        assert!(!record.accepted);
        let survivor = slot.expect("the gold-frame fit stands");
        assert_eq!(survivor.ui_scale, 1.5);
        assert_eq!(survivor.source, ScreenScaleSource::MercFrame);
    }

    /// The refusal is a property of the CUE, not of the numbers: a derivation
    /// for a different display must not take the slice either, because the
    /// prune (`drop_if_mismatched`) is what owns "the game moved screens" and
    /// it empties the slot first. Without this, the display term alone would
    /// let a derived value overturn a gold-frame fit.
    #[test]
    fn a_derived_measurement_is_refused_even_from_a_different_display() {
        let standing = ScreenSlice {
            width: 1920,
            height: 1080,
            ui_scale: 0.9,
            client: [0, 0, 1920, 1080],
            ..reference_screen()
        };
        let mut slot = Some(standing);

        let record = record_screen(
            &mut slot,
            screen_from_geometry(3840, 2160, REFERENCE_MONITOR + 1, (1920, 0), [0, 0, 3840, 2160], 99),
        );

        assert!(!record.accepted);
        let survivor = slot.expect("the gold-frame fit stands");
        assert_eq!(survivor.monitor_id, REFERENCE_MONITOR);
        assert_eq!(survivor.ui_scale, 0.9);
    }

    /// The override that keeps the derivation from being a one-way door. At
    /// 1080p a temple anchor converts to exactly the 0.90 the height implies —
    /// inside [`OCR_DRIFT_BAND`] — so without the `Capture` term in [`accepts`]
    /// a Recalibrate press would make `temple-anchor` unreachable on that
    /// machine, and no cue could ever correct a derivation that was wrong.
    #[test]
    fn a_verifying_cue_takes_the_slice_back_from_a_derived_value_inside_the_band() {
        let derived = screen_from_geometry(1920, 1080, REFERENCE_MONITOR, (0, 0), [0, 0, 1920, 1080], 42);
        let mut slot = Some(derived);
        let anchored = ScreenSlice {
            width: 1920,
            height: 1080,
            ui_scale: derived.ui_scale,
            source: ScreenScaleSource::TempleAnchor,
            client: [0, 0, 1920, 1080],
            measured_at_ms: 99,
            ..reference_screen()
        };

        let record = record_screen(&mut slot, anchored);

        assert!(record.accepted, "an agreeing anchor still replaces arithmetic");
        let published = slot.expect("the anchor stands");
        assert_eq!(published.source, ScreenScaleSource::TempleAnchor);
        assert!(published.verified_this_session);
    }

    /// The same override must NOT fire against a value that already came off a
    /// cue: two cues that agree inside the band are POE-240's non-news, and
    /// letting them trade the slice back and forth spends a settings write and
    /// wakes every overlay poll for as long as both panels are open.
    #[test]
    fn an_agreeing_cue_still_does_not_displace_another_cue_inside_the_band() {
        let mut slot = Some(reference_screen());
        let agreeing = ScreenSlice {
            source: ScreenScaleSource::TempleAnchor,
            measured_at_ms: reference_screen().measured_at_ms + 5_000,
            ..reference_screen()
        };

        let record = record_screen(&mut slot, agreeing);

        assert!(!record.accepted);
        assert_eq!(slot.expect("the frame fit stands").source, ScreenScaleSource::MercFrame);
    }

    #[test]
    fn reference_screen_placements_scale_from_the_client_origin() {
        let placements = placements(&reference_screen());

        assert_eq!(
            placements.temple.expect("the reference screen has a temple placement").entrance_origin,
            (960, 792),
        );
        assert_eq!(placements.merc.expect("the reference screen has a merc placement").panel, [698, 615, 555, 477]);
        // The full 1920 client width, 44 · 1.0 tall: the rule at the reference scale.
        assert_eq!(placements.lab.gem, [0, 0, 1920, 44]);
        // 740 wide centred in the 1189 px left of the inventory: (1189 − 740) / 2
        // = 224 in integer px (224.5 exact); top 244, 757 tall.
        assert_eq!(placements.lab.font, [224, 244, 740, 757]);
    }

    #[test]
    fn zero_ui_scale_keeps_the_1080p_lab_gem_rect() {
        let screen = ScreenSlice { ui_scale: 0.0, ..reference_screen() };

        // The invalid scale falls back to 0.90 while the client stays the
        // reference screen's 1920 wide: round(44 · 0.9) = 40 tall.
        assert_eq!(placements(&screen).lab.gem, [0, 0, 1920, 40]);
    }

    #[test]
    fn nan_ui_scale_keeps_the_1080p_lab_gem_rect() {
        let screen = ScreenSlice { ui_scale: f32::NAN, ..reference_screen() };

        assert_eq!(placements(&screen).lab.gem, [0, 0, 1920, 40]);
    }

    #[test]
    fn a_fractional_ui_scale_rounds_the_lab_gem_rect() {
        let screen = ScreenSlice { ui_scale: 1.2, ..reference_screen() };

        // The full 1920 wide, round(44 · 1.2 = 52.8) = 53 tall.
        assert_eq!(placements(&screen).lab.gem, [0, 0, 1920, 53]);
    }

    #[test]
    fn the_two_machine_x_anchor_reproduces_the_recorded_laptop_points() {
        let screen = ScreenSlice {
            width: 1920,
            height: 1080,
            ui_scale: 0.9,
            client: [0, 0, 1920, 1080],
            ..reference_screen()
        };
        let placements = placements(&screen);

        assert_eq!(
            placements.temple.expect("the measured screen has a temple placement").entrance_origin,
            (960, 713),
        );
        assert_eq!(placements.merc.expect("the measured screen has a merc placement").panel, [724, 554, 500, 429]);
        assert_eq!(placements.lab.gem, [0, 0, 1920, 40]);
        assert_eq!(placements.lab.font, [298, 220, 666, 681]);
    }

    #[test]
    fn placements_add_the_client_offset_before_scaling_reference_geometry() {
        let screen = ScreenSlice {
            width: 1920,
            height: 1080,
            ui_scale: 0.9,
            client: [120, 40, 1600, 900],
            ..reference_screen()
        };
        let placements = placements(&screen);

        assert_eq!(
            placements.temple.expect("the client has a temple placement").entrance_origin,
            (920, 753),
        );
        assert_eq!(placements.merc.expect("the client has a merc placement").panel, [684, 594, 500, 429]);
        // The gem region starts at the client origin and spans the client's
        // full 1600 width, one 40 px name line tall.
        assert_eq!(placements.lab.gem, [120, 40, 1600, 40]);
        // The font region is centred in the 1600 − 658 = 942 px left of the
        // inventory and offset by the client origin: 120 + (942 − 666) / 2 = 258,
        // 40 + 220 = 260.
        assert_eq!(placements.lab.font, [258, 260, 666, 681]);
    }

    #[test]
    fn remembered_anchors_replace_only_their_owned_placement_origins() {
        let screen = ScreenSlice {
            width: 1920,
            height: 1080,
            ui_scale: 0.9,
            client: [0, 0, 1920, 1080],
            anchors: Some(Anchors {
                temple_entrance: Some([111, 222]),
                merc_panel: Some([120, 220]),
            }),
            ..reference_screen()
        };
        let placements = placements(&screen);

        assert_eq!(
            placements.temple.expect("the remembered temple anchor is located").entrance_origin,
            (111, 222),
        );
        assert_eq!(placements.merc.expect("the remembered merc anchor is located").panel, [120, 220, 500, 429]);
        assert_eq!(placements.lab.gem, [0, 0, 1920, 40]);
    }

    #[test]
    fn a_repeat_measurement_keeps_remembered_anchors_when_the_writer_has_none() {
        let anchors = Some(Anchors {
            temple_entrance: Some([111, 222]),
            merc_panel: Some([120, 220]),
        });
        let current = ScreenSlice { anchors, ..reference_screen() };
        let mut slot = Some(current);
        let next = ScreenSlice {
            measured_at_ms: current.measured_at_ms + 5_000,
            anchors: None,
            ..current
        };

        let record = record_screen(&mut slot, next);

        assert!(record.accepted);
        assert!(!record.changed);
        assert_eq!(slot.expect("the repeated measurement remains").anchors, anchors);
    }

    /// Anchors are capture-relative to ONE display: carried onto another one,
    /// they would place the temple and merc crops on arithmetic from the screen
    /// the game left. Fails if `same_screen_geometry` stops comparing origins.
    #[test]
    fn a_display_change_drops_remembered_anchors_with_the_old_geometry() {
        let current = ScreenSlice {
            anchors: Some(Anchors {
                temple_entrance: Some([111, 222]),
                merc_panel: Some([120, 220]),
            }),
            ..reference_screen()
        };
        let mut slot = Some(current);
        let next = ScreenSlice { origin: OTHER_ORIGIN, anchors: None, ..current };

        record_screen(&mut slot, next);

        assert_eq!(slot.expect("the other display's measurement is stored").anchors, None);
    }

    #[test]
    fn a_client_change_drops_remembered_anchors_with_the_old_geometry() {
        let current = ScreenSlice {
            anchors: Some(Anchors {
                temple_entrance: Some([111, 222]),
                merc_panel: Some([120, 220]),
            }),
            ..reference_screen()
        };
        let mut slot = Some(current);
        let next = ScreenSlice {
            client: [20, 0, 1920, 1200],
            anchors: None,
            ..current
        };

        let record = record_screen(&mut slot, next);

        assert!(record.accepted);
        assert!(record.changed);
        assert_eq!(slot.expect("the changed client is stored").anchors, None);
    }

    #[test]
    fn a_scale_change_drops_remembered_anchors_with_the_old_geometry() {
        let current = ScreenSlice {
            anchors: Some(Anchors {
                temple_entrance: Some([111, 222]),
                merc_panel: Some([120, 220]),
            }),
            ..reference_screen()
        };
        let mut slot = Some(current);
        let next = ScreenSlice {
            ui_scale: 1.2,
            anchors: None,
            ..current
        };

        let record = record_screen(&mut slot, next);

        assert!(record.accepted);
        assert!(record.changed);
        assert_eq!(slot.expect("the changed scale is stored").anchors, None);
    }

    /// The handle is not the display: Windows hands one screen a new handle
    /// between sessions, and a remeasure under it must keep the anchors the
    /// module learned on that screen.
    #[test]
    fn a_new_monitor_handle_on_the_same_display_keeps_anchors() {
        let current = ScreenSlice {
            monitor_id: OTHER_MONITOR,
            anchors: Some(Anchors {
                temple_entrance: Some([111, 222]),
                merc_panel: None,
            }),
            ..reference_screen()
        };
        let mut slot = Some(current);
        let next = ScreenSlice {
            monitor_id: REFERENCE_MONITOR,
            anchors: None,
            ..current
        };

        record_screen(&mut slot, next);

        assert_eq!(
            slot.expect("the remeasure of the same display is stored").anchors,
            current.anchors,
        );
    }

    #[test]
    fn anchor_helper_updates_each_known_module() {
        let mut anchors = None;

        assert!(set_anchor(&mut anchors, AnchorModule::TempleEntrance, [111, 222]));
        assert!(set_anchor(&mut anchors, AnchorModule::MercPanel, [120, 220]));
        assert_eq!(
            anchors,
            Some(Anchors {
                temple_entrance: Some([111, 222]),
                merc_panel: Some([120, 220]),
            }),
        );
    }

    #[test]
    fn anchor_helper_ignores_a_repeated_origin() {
        let mut anchors = None;

        assert!(set_anchor(&mut anchors, AnchorModule::TempleEntrance, [111, 222]));
        assert!(!set_anchor(&mut anchors, AnchorModule::TempleEntrance, [111, 222]));
    }

    #[test]
    fn a_changed_anchor_persists_before_it_emits() {
        let effects = std::cell::RefCell::new(Vec::new());

        assert!(remember_anchor_effects(
            true,
            || effects.borrow_mut().push("persist"),
            || effects.borrow_mut().push("emit"),
        ));
        assert_eq!(*effects.borrow(), vec!["persist", "emit"]);
    }

    #[test]
    fn an_unchanged_anchor_skips_persistence_and_emit() {
        let effects = std::cell::RefCell::new(Vec::new());

        assert!(!remember_anchor_effects(
            false,
            || effects.borrow_mut().push("persist"),
            || effects.borrow_mut().push("emit"),
        ));
        assert!(effects.borrow().is_empty());
    }

    /// The wire contract the TS `ScreenSlice` mirrors: camelCase keys on the
    /// slice, under the snapshot's own `screen` key. Dropping either
    /// `rename_all` renames these and the store reads `undefined` in silence.
    #[test]
    fn the_snapshot_exposes_the_screen_slice_under_its_camel_case_keys() {
        let snap = AppSsotSnapshot {
            screen: Some(reference_screen()),
            placements: placements(&reference_screen()),
            ..Default::default()
        };

        let json = serde_json::to_value(&snap).unwrap();

        assert_eq!(json["screen"]["width"], 1920);
        assert_eq!(json["screen"]["height"], 1200);
        assert_eq!(json["screen"]["uiScale"], 1.0);
        assert_eq!(json["screen"]["measuredAtMs"], 1_700_000_000_000u64);
        assert_eq!(json["screen"]["verifiedThisSession"], true);
        assert_eq!(json["screen"]["monitorId"], REFERENCE_MONITOR);
        assert_eq!(json["screen"]["origin"], serde_json::json!([0, 0]));
        assert_eq!(json["screen"]["client"], serde_json::json!([0, 0, 1920, 1200]));
        assert_eq!(json["screen"]["anchors"], serde_json::Value::Null);
        assert_eq!(json["placements"]["temple"]["entranceOrigin"], serde_json::json!([960, 792]));
        assert_eq!(json["placements"]["merc"]["panel"], serde_json::json!([698, 615, 555, 477]));
        assert_eq!(json["placements"]["lab"]["gem"], serde_json::json!([0, 0, 1920, 44]));
        assert_eq!(json["placements"]["lab"]["font"], serde_json::json!([224, 244, 740, 757]));
    }

    /// The five source strings, exactly. They are read by a TS union
    /// (`stores/ssot.svelte.ts`), rendered by `geometry/view.ts` and used by the
    /// WI-B2 settings round-trip, so a rename here is a silent break on all
    /// three — and `kebab-case` is the one `rename_all` that produces them, so
    /// this pins the attribute as much as the variants.
    #[test]
    fn the_screen_scale_sources_serialise_as_their_wire_strings() {
        let wire = |source| {
            let snap = AppSsotSnapshot {
                screen: Some(ScreenSlice { source, ..reference_screen() }),
                ..Default::default()
            };
            serde_json::to_value(&snap).unwrap()["screen"]["source"].clone()
        };

        assert_eq!(wire(ScreenScaleSource::MercFrame), "merc-frame");
        assert_eq!(wire(ScreenScaleSource::MercOcr), "merc-ocr");
        assert_eq!(wire(ScreenScaleSource::TempleAnchor), "temple-anchor");
        assert_eq!(wire(ScreenScaleSource::Remembered), "remembered");
        assert_eq!(wire(ScreenScaleSource::Capture), "capture");
    }

    /// The first measurement always publishes — there is nothing behind it, so
    /// nobody downstream knows the screen at all yet.
    #[test]
    fn the_first_screen_measurement_is_published() {
        assert!(screen_changed(None, &reference_screen()));
    }

    /// The gate's whole purpose: the detect tick re-measures the same screen
    /// every few seconds for as long as the recruit window is open, and none of
    /// those repeats may wake an overlay poll.
    #[test]
    fn an_identical_screen_measurement_is_not_published() {
        let current = reference_screen();

        assert!(!screen_changed(Some(&current), &reference_screen()));
    }

    /// A new timestamp on an otherwise identical measurement is the repeat case
    /// as it actually arrives — every tick stamps a fresh `measured_at_ms`. If
    /// this field were compared, the gate would pass every single tick and be
    /// no gate at all.
    #[test]
    fn a_fresh_timestamp_alone_is_not_published() {
        let current = reference_screen();
        let next = ScreenSlice { measured_at_ms: current.measured_at_ms + 5_000, ..current };

        assert!(!screen_changed(Some(&current), &next));
    }

    /// The measurement consumers scale rects with. 0.974 to 1.0 is the exact
    /// step POE-214's frame fit makes on the reference machine.
    #[test]
    fn a_changed_ui_scale_is_published() {
        let current = ScreenSlice { ui_scale: 0.974, ..reference_screen() };

        assert!(screen_changed(Some(&current), &reference_screen()));
    }

    /// The wobble [`UI_SCALE_EPS`] exists for: `run::next_fitted_scale`'s
    /// same-`cell_px` path returns `..fresh`, so the published scale takes the
    /// fresh measurement on every grid-fitting tick, and one `cellfit::P_STEP`
    /// of the pitch grid moves it by 0.0031. Compared exactly, that is a second
    /// `ssot-changed` per tick on a panel that has not moved a pixel.
    #[test]
    fn a_ui_scale_wobble_inside_the_band_is_not_published() {
        let current = reference_screen();
        let next = ScreenSlice { ui_scale: current.ui_scale + 0.003, ..current };

        assert!(!screen_changed(Some(&current), &next));
    }

    /// The far side of the same band. 0.03 is past the 1/40 = 0.025 a whole px
    /// of merc cell is worth, so it is a scale a consumer would cut different
    /// rects from — a band wide enough to swallow it would be hiding a real
    /// change of screen.
    #[test]
    fn a_ui_scale_step_past_the_band_is_published() {
        let current = reference_screen();
        let next = ScreenSlice { ui_scale: current.ui_scale + 0.03, ..current };

        assert!(screen_changed(Some(&current), &next));
    }

    /// A resolution change is a different screen, and every fraction-of-the-
    /// screen rect derived from the slice moves with it. Width and height are
    /// asserted apart so a gate that compares only one names which.
    #[test]
    fn a_changed_screen_width_is_published() {
        let current = reference_screen();
        let next = ScreenSlice { width: 2560, ..current };

        assert!(screen_changed(Some(&current), &next));
    }

    /// The half of the resolution the game's UI scale actually follows.
    #[test]
    fn a_changed_screen_height_is_published() {
        let current = reference_screen();
        let next = ScreenSlice { height: 1080, ..current };

        assert!(screen_changed(Some(&current), &next));
    }

    /// The label moving at an unchanged scale is still news: a consumer that
    /// weighs how exact its rects are reads `source`, and a session that fell
    /// back from the frame to the OCR cue has to be able to say so even when
    /// the two agreed on the number.
    #[test]
    fn a_changed_screen_scale_source_is_published() {
        let current = reference_screen();
        let next = ScreenSlice { source: ScreenScaleSource::MercOcr, ..current };

        assert!(screen_changed(Some(&current), &next));
    }

    /// The emit half of the gate: a repeat of the measurement already in the
    /// slot is accepted — it is a fresh frame fit of the same screen — but is
    /// not worth waking every overlay's poll, however fresh its stamp.
    #[test]
    fn record_screen_reports_no_change_for_a_repeat_measurement() {
        let base = reference_screen();
        let mut slot = Some(base);
        let next = ScreenSlice { measured_at_ms: base.measured_at_ms + 5_000, ..base };

        let record = record_screen(&mut slot, next);

        assert!(record.accepted, "a fresh frame fit of the same screen is taken");
        assert!(!record.changed);
    }

    /// The store half, and the one WI-B2 depends on: an ACCEPTED measurement
    /// overwrites the slot whatever the emit gate says, so `measured_at_ms` is
    /// the age of the last accepted measurement. Gated storage would make it
    /// the age of the last CHANGE, and a persisted scale would then claim to be
    /// hours older than it is.
    #[test]
    fn an_accepted_screen_measurement_the_emit_gate_refused_is_still_stored() {
        let base = reference_screen();
        let mut slot = Some(base);
        let next = ScreenSlice { measured_at_ms: base.measured_at_ms + 5_000, ..base };

        record_screen(&mut slot, next);

        assert_eq!(
            slot.expect("the slot still holds a measurement").measured_at_ms,
            base.measured_at_ms + 5_000
        );
    }

    // ------------------------------------------------- source acceptance --

    /// The bug POE-240 was filed for, from the consumer's side: a session whose
    /// frame fit landed once keeps that number for as long as the OCR cue only
    /// re-describes the same screen. Before the rule, every OCR-only tick of an
    /// open recruit window overwrote the frame value with the 6-12 px drift.
    ///
    /// 0.005 is written out rather than derived from [`OCR_DRIFT_BAND`] so this
    /// pins the BAND's value too: it is 6 px of the 1200-px reference height,
    /// the near end of POE-214's measured drift, and a band narrowed below it
    /// would start taking exactly the readings it exists to turn down.
    #[test]
    fn an_ocr_measurement_inside_the_drift_band_does_not_replace_a_frame_measurement() {
        let frame = measured_by(ScreenScaleSource::MercFrame, 1.0);
        let mut slot = Some(frame);
        let drifted = ScreenSlice {
            measured_at_ms: frame.measured_at_ms + 5_000,
            ..measured_by(ScreenScaleSource::MercOcr, 1.0 - 0.005)
        };

        let record = record_screen(&mut slot, drifted);

        assert!(!record.accepted);
        assert!(
            !record.changed,
            "a refusal changed nothing, so it must not wake every overlay's poll",
        );
        let standing = slot.expect("the frame measurement is still there");
        assert_eq!(standing.ui_scale, 1.0);
        assert_eq!(standing.source, ScreenScaleSource::MercFrame);
    }

    /// The stamp is part of "the value in force", not a separate fact: a
    /// refusal must not advance `measured_at_ms`, because the number that
    /// survived really was measured when it says it was. Freshening the stamp
    /// on a refusal would make the Settings card age a value that never moved.
    #[test]
    fn a_refused_measurement_leaves_the_standing_stamp_alone() {
        let frame = measured_by(ScreenScaleSource::MercFrame, 1.0);
        let mut slot = Some(frame);
        let drifted = ScreenSlice {
            measured_at_ms: frame.measured_at_ms + 5_000,
            ..measured_by(ScreenScaleSource::MercOcr, 1.0 - 0.005)
        };

        record_screen(&mut slot, drifted);

        assert_eq!(
            slot.expect("the frame measurement is still there").measured_at_ms,
            frame.measured_at_ms,
        );
    }

    /// The refusal log line names the number that SURVIVED, and this call is
    /// the only place still holding it — a caller re-reading the slot can find
    /// it emptied by a Recalibrate. A record that echoed the refused reading
    /// back would have the log say "kept 0.99" on a session standing at 1.0.
    #[test]
    fn a_refusal_reports_the_scale_that_survived_it() {
        let mut slot = Some(measured_by(ScreenScaleSource::MercFrame, 1.0));
        let drifted = measured_by(ScreenScaleSource::MercOcr, 1.0 - 0.005);

        let record = record_screen(&mut slot, drifted);

        assert_eq!(record.standing_ui_scale, 1.0);
    }

    /// Different dimensions are a different screen, and no drift band can
    /// explain them — so the OCR reading is the only description of the screen
    /// actually in front of the player and it wins. Without this arm a
    /// resolution change on an OCR-only session would strand the old scale.
    #[test]
    fn an_ocr_measurement_of_different_dimensions_replaces_a_frame_measurement() {
        let mut slot = Some(measured_by(ScreenScaleSource::MercFrame, 1.0));
        let elsewhere = ScreenSlice {
            height: 1080,
            ..measured_by(ScreenScaleSource::MercOcr, 1.0)
        };

        let record = record_screen(&mut slot, elsewhere);

        assert!(record.accepted);
        assert_eq!(slot.expect("the new screen is stored").height, 1080);
    }

    /// The far side of the band. 0.02 is 24 px of the 1200-px reference height,
    /// twice the wide end of the drift POE-214 measured: not the cue being
    /// imprecise about the same screen, but the screen having changed. Written
    /// out rather than derived from [`OCR_DRIFT_BAND`] so a band widened to
    /// swallow it — which would pin a session to a scale the game no longer
    /// draws at — fails here.
    #[test]
    fn an_ocr_measurement_outside_the_drift_band_replaces_a_frame_measurement() {
        let mut slot = Some(measured_by(ScreenScaleSource::MercFrame, 1.0));
        let far = measured_by(ScreenScaleSource::MercOcr, 1.0 - 0.02);

        let record = record_screen(&mut slot, far);

        assert!(record.accepted);
        assert_eq!(slot.expect("the new scale is stored").source, ScreenScaleSource::MercOcr);
    }

    /// A frame fit needs no argument to be taken: it is the measurement every
    /// other cue stands in for. Here it replaces an OCR value that agrees with
    /// it to well inside the band — the case a symmetric "only if it differs"
    /// rule would wrongly refuse, leaving the slice labelled `merc-ocr` and
    /// unverified for the rest of the session.
    #[test]
    fn a_frame_measurement_replaces_an_ocr_value_it_agrees_with() {
        let mut slot = Some(measured_by(ScreenScaleSource::MercOcr, 1.0));
        let frame = measured_by(ScreenScaleSource::MercFrame, 1.0);

        let record = record_screen(&mut slot, frame);

        assert!(record.accepted);
        assert_eq!(slot.expect("the frame fit is stored").source, ScreenScaleSource::MercFrame);
    }

    /// The startup seed is not a measurement and must never overwrite one. The
    /// load happens once, into an empty slot; a `Remembered` slice arriving
    /// after a live cue has spoken is a stale file read, and taking it would
    /// undo the session's own reading of the screen in front of the player.
    #[test]
    fn a_remembered_seed_never_replaces_a_live_measurement() {
        let mut slot = Some(measured_by(ScreenScaleSource::MercOcr, 1.0));
        let seed = measured_by(ScreenScaleSource::Remembered, 0.9);

        let record = record_screen(&mut slot, seed);

        assert!(!record.accepted);
        assert_eq!(slot.expect("the live measurement stands").ui_scale, 1.0);
    }

    /// An empty slot has nothing to protect, so the startup seed fills it —
    /// which is the whole point of persisting a scale. A rule that refused
    /// `Remembered` outright would make every launch unmeasured until a recruit
    /// window opened.
    #[test]
    fn a_remembered_seed_fills_an_empty_slot() {
        let mut slot = None;

        let record = record_screen(&mut slot, measured_by(ScreenScaleSource::Remembered, 0.9));

        assert!(record.accepted);
        assert_eq!(slot.expect("the seed is stored").ui_scale, 0.9);
    }

    // ------------------------------------------- verified this session --

    /// Only a cue that matched art on THIS run's pixels verifies: the gold
    /// frame, and the temple's Entrance-plate anchor. The OCR line pitch is the
    /// estimate that drifts and a load looked at nothing this run, so neither
    /// may claim the screen was confirmed — this is the rule all three writers
    /// (the two publishes and `settings::ScreenScaleSetting::to_slice`) seed the
    /// flag from.
    ///
    /// The temple row is deliberately NOT the same answer [`accepts`] gives it
    /// (POE-234 WI-2): an anchor confirms the screen and still may not displace
    /// a standing merc value inside the band, because "something looked" and
    /// "this number is better" are different claims. Fails if the two rules are
    /// ever folded into one.
    #[test]
    fn a_cue_verifies_the_screen_only_if_it_looked_at_it_this_run() {
        assert!(verifies_the_screen(ScreenScaleSource::MercFrame));
        assert!(
            verifies_the_screen(ScreenScaleSource::TempleAnchor),
            "an anchor is a full-resolution template match on this run's pixels",
        );
        assert!(
            !verifies_the_screen(ScreenScaleSource::MercOcr),
            "the drifting cue confirms nothing",
        );
        assert!(
            !verifies_the_screen(ScreenScaleSource::Remembered),
            "a file read is not a look at the screen",
        );

        let standing = measured_by(ScreenScaleSource::MercFrame, 0.90);
        let restated = measured_by(ScreenScaleSource::TempleAnchor, 0.905);
        assert!(
            !accepts(Some(&standing), &restated),
            "verifying the screen must not have become a licence to replace",
        );
    }

    /// An OCR reading accepted because the screen actually changed is still not
    /// a verification — it is the drifting cue describing a screen nothing has
    /// confirmed. Carrying the replaced value's `true` across would be the
    /// worst version of the flag: a claim of confirmation attached to the one
    /// number nobody checked.
    #[test]
    fn an_accepted_ocr_measurement_on_a_new_screen_is_unverified() {
        let mut slot = Some(measured_by(ScreenScaleSource::MercFrame, 1.0));
        let elsewhere = ScreenSlice {
            height: 1080,
            ..measured_by(ScreenScaleSource::MercOcr, 0.9)
        };

        record_screen(&mut slot, elsewhere);

        assert!(!slot.expect("the new screen is stored").verified_this_session);
    }

    /// The startup path a persisted scale actually takes (WI-B2): the seed
    /// fills an empty slot unverified — a file read looked at nothing this run
    /// — and the session's first gold-frame fit of the SAME screen is what
    /// makes the claim honest. The scales agree to within [`UI_SCALE_EPS`], so
    /// nothing but the CUE has changed, and that is the point: `accepts` takes
    /// `MercFrame` over anything standing, unconditionally. A rule that kept
    /// the seed because the numbers already agreed would leave the Settings
    /// card saying "remembered, unconfirmed" for the rest of a session that
    /// has since confirmed it.
    #[test]
    fn a_remembered_seed_starts_unverified_and_a_matching_frame_measurement_verifies_it() {
        let mut slot = None;
        record_screen(&mut slot, measured_by(ScreenScaleSource::Remembered, 1.0));
        assert!(
            !slot.expect("the seed filled the empty slot").verified_this_session,
            "the arrangement this test needs: the seed is in the slot, unverified",
        );

        let frame = ScreenSlice {
            measured_at_ms: reference_screen().measured_at_ms + 5_000,
            ..measured_by(ScreenScaleSource::MercFrame, 1.0 + UI_SCALE_EPS / 2.0)
        };
        record_screen(&mut slot, frame);

        let standing = slot.expect("the frame measurement replaced the seed");
        assert!(
            standing.verified_this_session,
            "the frame fit looked at the screen, so the slot says so",
        );
        assert_eq!(standing.source, ScreenScaleSource::MercFrame);
    }

    /// Dragging the game to a second monitor of the same resolution changes
    /// nothing the other three terms compare — same size, same cue, same scale
    /// — so without the display in the discrete list the move would never wake
    /// a single overlay's poll and every widget would stay placed against the
    /// screen the game had left.
    #[test]
    fn moving_to_a_same_size_monitor_is_published() {
        let current = reference_screen();
        let next = ScreenSlice { monitor_id: OTHER_MONITOR, ..current };

        assert!(screen_changed(Some(&current), &next));
    }

    /// The other side of that term, and the reason it is a bare `!=`: a session
    /// that loaded a pre-POE-237 seed and then measured the display it is
    /// really on has learned something the Settings card shows, even though a
    /// handle is never enough to overturn the measurement itself (see
    /// [`accepts`]).
    #[test]
    fn learning_which_display_a_screen_is_on_is_published() {
        let seeded = ScreenSlice { monitor_id: 0, ..reference_screen() };
        let measured = reference_screen();

        assert!(screen_changed(Some(&seeded), &measured));
    }

    /// A different DISPLAY is not the drift the band exists to swallow: the
    /// standing value describes a screen the game has left, so even a
    /// drift-grade reading of the one it is on now is the better answer. The
    /// scale here is deliberately INSIDE the band (0.005 of 0.01), so the test
    /// fails the moment the display term is dropped from `accepts` and the band
    /// is allowed to hold the old display's number in place.
    #[test]
    fn an_ocr_measurement_from_another_display_replaces_a_frame_measurement() {
        let mut slot = Some(measured_by(ScreenScaleSource::MercFrame, 1.0));
        let elsewhere = ScreenSlice {
            origin: OTHER_ORIGIN,
            ..measured_by(ScreenScaleSource::MercOcr, 1.0 - 0.005)
        };

        let record = record_screen(&mut slot, elsewhere);

        assert!(record.accepted);
        let standing = slot.expect("the new display's measurement is stored");
        assert_eq!(standing.origin, OTHER_ORIGIN);
        assert_eq!(standing.ui_scale, 1.0 - 0.005);
    }

    /// A new HANDLE is not another display: Windows renumbers one screen
    /// between sessions (2026-09-10..14: 65622, 65641, 161748971, 65622 at
    /// `(0, 0)`). A drifting cue read under the new handle on the same screen
    /// must keep the frame fit, or every renumbering hands the session back to
    /// the POE-240 bug. Fails if `accepts` compares handles again.
    #[test]
    fn an_ocr_measurement_under_a_new_handle_stays_inside_the_drift_band() {
        let mut slot = Some(measured_by(ScreenScaleSource::MercFrame, 1.0));
        let drifted = ScreenSlice {
            monitor_id: OTHER_MONITOR,
            ..measured_by(ScreenScaleSource::MercOcr, 1.0 - 0.005)
        };

        let record = record_screen(&mut slot, drifted);

        assert!(!record.accepted);
        assert_eq!(slot.expect("the seeded measurement stands").ui_scale, 1.0);
    }

    /// The persist trigger, both halves. A tick that re-measures the same
    /// screen must not spend two blocking disk ops, and a cue that is not
    /// persistable at all (`MercOcr` — see
    /// `settings::ScreenScaleSetting::from_slice`) must not spend a write that
    /// could only null the stored value. `Remembered` is what was just loaded,
    /// so writing it back learns nothing.
    ///
    /// `TempleAnchor` is persistable for the same reason `MercFrame` is (POE-234
    /// WI-2): it is a template match, not an estimate, and on a machine whose
    /// recruit window is never opened it is the only measurement there will ever
    /// be. Fails if a source is added to this rule without being added to
    /// `from_slice`'s, or the other way round — the two are one rule, stated in
    /// full there.
    #[test]
    fn only_a_changed_measurement_from_a_cue_that_looked_is_worth_remembering() {
        assert!(should_remember_screen(true, ScreenScaleSource::MercFrame));
        assert!(
            should_remember_screen(true, ScreenScaleSource::TempleAnchor),
            "an anchor is the only measurement some machines will ever have",
        );
        assert!(
            !should_remember_screen(false, ScreenScaleSource::MercFrame),
            "a re-measurement of the same screen must not reach the disk",
        );
        assert!(
            !should_remember_screen(false, ScreenScaleSource::TempleAnchor),
            "…and neither must a re-anchor of the same board",
        );
        assert!(
            !should_remember_screen(true, ScreenScaleSource::MercOcr),
            "the drifting cue is not persistable, so it is not worth a write",
        );
        assert!(
            !should_remember_screen(true, ScreenScaleSource::Remembered),
            "a just-loaded value has nothing to write back",
        );
    }

    /// Nothing remembered matches every capture. The prune is what makes the
    /// caller log a line and rewrite settings.json, and a `false` here would
    /// spend both on an empty slot on every detect tick of every session that
    /// has never measured a screen.
    #[test]
    fn an_unmeasured_screen_matches_any_capture() {
        assert!(screen_matches(&None, (1920, 1080), REFERENCE_ORIGIN, [0, 0, 1920, 1080]));
    }

    /// The keep case: the remembered measurement describes the screen just
    /// captured, so it survives. A `false` here would drop and re-measure the
    /// scale on every tick, which is the whole thing WI-B2 exists to avoid.
    #[test]
    fn a_measurement_of_the_same_screen_is_kept() {
        assert!(screen_matches(
            &Some(reference_screen()),
            (1920, 1200),
            REFERENCE_ORIGIN,
            [0, 0, 1920, 1200],
        ));
    }

    #[test]
    fn a_different_client_rect_is_not_the_same_screen_geometry() {
        assert!(!screen_matches(
            &Some(reference_screen()),
            (1920, 1200),
            REFERENCE_ORIGIN,
            [20, 0, 1920, 1200],
        ));
    }

    /// A different WIDTH is a different screen. Fails if the predicate compares
    /// the height alone — the case that matters, because the game's UI scales
    /// with height, so an ultrawide swap keeps the height and changes nothing
    /// else the app can see.
    #[test]
    fn a_measurement_from_a_different_width_is_dropped() {
        assert!(!screen_matches(
            &Some(reference_screen()),
            (2560, 1200),
            REFERENCE_ORIGIN,
            [0, 0, 2560, 1200],
        ));
    }

    /// A different HEIGHT is a different screen. Fails if the predicate
    /// compares the width alone — the 1920x1200 → 1920x1080 swap, which is the
    /// exact pair POE-214 measured (1.0 vs 0.90) and the one an unpruned scale
    /// gets 11% wrong.
    #[test]
    fn a_measurement_from_a_different_height_is_dropped() {
        assert!(!screen_matches(
            &Some(reference_screen()),
            (1920, 1080),
            REFERENCE_ORIGIN,
            [0, 0, 1920, 1080],
        ));
    }

    /// The scale is NOT part of the question. Two runs on the same monitor can
    /// settle 0.0031 apart (see [`UI_SCALE_EPS`]), and a predicate that read the
    /// scale would call that a monitor change and throw away a good
    /// measurement — the dimensions are the only thing a capture states about
    /// itself.
    #[test]
    fn a_wobbled_scale_on_the_same_screen_is_still_a_match() {
        let drifted = ScreenSlice { ui_scale: 0.87, ..reference_screen() };

        assert!(screen_matches(
            &Some(drifted),
            (1920, 1200),
            REFERENCE_ORIGIN,
            [0, 0, 1920, 1200],
        ));
    }

    /// The blind spot POE-237 closes, and the one POE-227 could not: two
    /// monitors of the SAME resolution. Dimensions alone call this a match and
    /// the game keeps running on a scale measured on the display it left; the
    /// origin is what tells them apart. Fails the moment the display term is
    /// dropped from the predicate.
    #[test]
    fn a_measurement_from_a_different_monitor_of_the_same_size_is_dropped() {
        assert!(!screen_matches(
            &Some(reference_screen()),
            (1920, 1200),
            OTHER_ORIGIN,
            [0, 0, 1920, 1200],
        ));
    }

    macro_rules! snapshot_echoes {
        ($($name:ident: $field:ident = $value:expr;)*) => {$(
            #[test]
            fn $name() {
                let state = crate::settings::tests::test_app_state();
                state.lab_settings.lock().unwrap().$field = $value.into();
                assert_eq!(build_snapshot(&state).$field, $value);
            }
        )*};
    }

    snapshot_echoes! {
        ssot_snapshot_echoes_the_selected_normal_variant: normal_variant = "1/0";
        ssot_snapshot_echoes_the_selected_dedication_variant: dedication_variant = "21/20";
        ssot_snapshot_echoes_the_selected_dedication_pool: dedication_pool = "transfigured";
    }

    // ------------------------------------------------- snapshot wire golden --

    /// Writer of `desktop/contracts/ssot-snapshot.json`; the reader is
    /// `desktop/src/lib/stores/ssot-contract.test.ts`, which checks its keys
    /// against the TS `SsotSnapshot`. Regenerate from the repo root with
    /// `docker compose run --rm -T -e SSOT_CONTRACT_UPDATE=1 -w /app/desktop/src-tauri desktop cargo test --lib ssot::tests::ssot_snapshot_contract`.
    #[test]
    fn ssot_snapshot_contract_golden() {
        let base = AppSsotSnapshot {
            league: LeagueSlice { name: Some("Mirage".into()), generation: 3 },
            resolving: true,
            unreachable: true,
            ..AppSsotSnapshot::default()
        };
        // One key only: a multi-key HashMap would serialize in a random order.
        let modules: std::collections::HashMap<String, bool> =
            [(TEMPLE_MODULE_ID.to_string(), true)].into_iter().collect();
        let screen = ScreenSlice {
            anchors: Some(Anchors {
                temple_entrance: Some([960, 713]),
                merc_panel: Some([100, 200]),
            }),
            ..reference_screen()
        };
        let snap = compose_snapshot(
            base,
            "20/20".into(),
            "21/23".into(),
            "skill".into(),
            modules,
            crate::mercenary::MercenarySlice::default(),
            vec!["guide-b".into()],
            crate::mercenary::sync::MercSyncStatus::default(),
            true,
            2,
            crate::temple::slice::TempleSlice::default(),
            Some(screen),
        );
        let actual = serde_json::to_string_pretty(&snap).unwrap() + "\n";
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../contracts/ssot-snapshot.json");
        if std::env::var("SSOT_CONTRACT_UPDATE").as_deref() == Ok("1") {
            std::fs::write(path, &actual).unwrap();
            return;
        }
        let golden = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            golden == actual,
            "{path} differs from the composed snapshot; regenerate with SSOT_CONTRACT_UPDATE=1 cargo test --lib ssot::tests::ssot_snapshot_contract"
        );
    }
}

#[cfg(test)]
mod league_generation_tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn first_resolution_establishes_generation_without_consumer_event() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        assert!(write_league(&ssot, "A".into()).is_none());
        let snapshot = ssot.lock().unwrap();
        assert_eq!(snapshot.league.name.as_deref(), Some("A"));
        assert_eq!(snapshot.league.generation, 1);
    }

    #[test]
    fn identical_resolution_keeps_generation() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "A".into());
        assert!(write_league(&ssot, "A".into()).is_none());
        assert_eq!(ssot.lock().unwrap().league.generation, 1);
    }

    #[test]
    fn returning_to_a_previous_name_advances_generation() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "A".into());
        let changed = write_league(&ssot, "B".into()).unwrap();
        assert_eq!((changed.league.as_str(), changed.generation), ("B", 2));
        let changed = write_league(&ssot, "A".into()).unwrap();
        assert_eq!((changed.league.as_str(), changed.generation), ("A", 3));
    }
}

#[cfg(test)]
mod league_resolver_tests {
    use super::*;
    use std::future::Future;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll, Wake, Waker};

    struct NoopWake;
    impl Wake for NoopWake { fn wake(self: Arc<Self>) {} }

    struct Runtime {
        scheduler: LeagueScheduler,
        ssot: Mutex<AppSsotSnapshot>,
        replies: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<Option<&'static str>>>,
        waits: Mutex<Vec<Duration>>,
        attempts: Mutex<usize>,
        clock: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<()>>,
    }

    impl LeagueRuntime for Runtime {
        fn scheduler(&self) -> &LeagueScheduler { &self.scheduler }
        fn begin(&self) { mark_resolving(&self.ssot); }
        async fn fetch(&self) -> bool {
            *self.attempts.lock().unwrap() += 1;
            let reply = self.replies.lock().await.recv().await.expect("network remains open");
            match reply {
                Some(name) => {
                    accept_league_response(&self.ssot, ("server", 0), ("server", 0), name.into());
                    true
                }
                None => false,
            }
        }
        fn failed(&self, failures: u32) {
            if should_flag_unreachable(failures) { self.ssot.lock().unwrap().unreachable = true; }
        }
        async fn sleep(&self, delay: Duration) {
            self.waits.lock().unwrap().push(delay);
            self.clock.lock().await.recv().await.expect("clock remains open");
        }
    }

    fn runtime(replies: Vec<Option<&'static str>>) -> (
        Runtime,
        tokio::sync::mpsc::UnboundedSender<()>,
        tokio::sync::mpsc::UnboundedSender<Option<&'static str>>,
    ) {
        let (clock, receiver) = tokio::sync::mpsc::unbounded_channel();
        let (network, responses) = tokio::sync::mpsc::unbounded_channel();
        for reply in replies { network.send(reply).unwrap(); }
        let scheduler = LeagueScheduler::new();
        assert!(scheduler.trigger(), "startup claims the sole resolver task");
        (Runtime {
            scheduler,
            ssot: Mutex::new(AppSsotSnapshot::default()),
            replies: tokio::sync::Mutex::new(responses), waits: Mutex::new(Vec::new()),
            attempts: Mutex::new(0), clock: tokio::sync::Mutex::new(receiver),
        }, clock, network)
    }

    fn advance(future: std::pin::Pin<&mut impl Future<Output = ()>>) {
        let waker = Waker::from(Arc::new(NoopWake));
        assert!(matches!(future.poll(&mut Context::from_waker(&waker)), Poll::Pending));
    }

    #[test]
    fn success_waits_indefinitely_for_each_isolated_trigger() {
        let (runtime, _clock, _network) = runtime(vec![Some("A"), Some("B"), Some("B"), Some("B")]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        // Polling the executor without a trigger represents arbitrarily much
        // idle time: the production loop must not even arm a status timer.
        for _ in 0..100 { advance(resolver.as_mut()); }
        assert_eq!(*runtime.attempts.lock().unwrap(), 1);
        assert!(runtime.waits.lock().unwrap().is_empty(), "idle must not arm a timer");
        for attempt in 2..=4 {
            // The shared production scheduler receives all trigger sources.
            assert!(!runtime.scheduler.trigger());
            advance(resolver.as_mut());
            assert_eq!(*runtime.attempts.lock().unwrap(), attempt);
            assert!(runtime.waits.lock().unwrap().is_empty());
            assert!(!runtime.ssot.lock().unwrap().resolving);
        }
        assert_eq!(runtime.ssot.lock().unwrap().league.name.as_deref(), Some("B"));
        assert_eq!(runtime.ssot.lock().unwrap().league.generation, 2);
    }

    #[test]
    fn failures_retry_with_capped_backoff_until_recovery() {
        let (runtime, clock, _network) = runtime(vec![None, None, None, None, None, None, None, None, Some("A")]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        for _ in 0..7 { clock.send(()).unwrap(); advance(resolver.as_mut()); }
        assert_eq!(*runtime.waits.lock().unwrap(), [2, 4, 8, 16, 32, 60, 60, 60].map(Duration::from_secs));
        assert!(runtime.ssot.lock().unwrap().unreachable);
        clock.send(()).unwrap();
        advance(resolver.as_mut());
        assert_eq!(runtime.waits.lock().unwrap().len(), 8, "recovery arms no idle timer");
        let snapshot = runtime.ssot.lock().unwrap();
        assert_eq!(snapshot.league.name.as_deref(), Some("A"));
        assert!(!snapshot.unreachable);
        assert!(!snapshot.resolving);
    }

    #[test]
    fn trigger_wakes_backoff_without_erasing_the_resolved_baseline() {
        let (runtime, clock, _network) = runtime(vec![Some("A"), None, None, Some("B")]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        runtime.scheduler.trigger(); advance(resolver.as_mut());
        clock.send(()).unwrap(); advance(resolver.as_mut());
        assert_eq!(runtime.waits.lock().unwrap().last(), Some(&Duration::from_secs(4)));
        let snapshot = runtime.ssot.lock().unwrap();
        assert_eq!((snapshot.league.name.as_deref(), snapshot.league.generation), (Some("A"), 1));
        drop(snapshot);
        runtime.scheduler.trigger(); advance(resolver.as_mut());
        assert_eq!(runtime.waits.lock().unwrap().len(), 2, "recovery arms no idle timer");
        assert_eq!(runtime.ssot.lock().unwrap().league.generation, 2);
    }

    #[test]
    fn trigger_during_backoff_resets_the_retry_delay() {
        let (runtime, clock, _network) = runtime(vec![None, None, None]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        clock.send(()).unwrap(); advance(resolver.as_mut());
        runtime.scheduler.trigger(); advance(resolver.as_mut());
        assert_eq!(*runtime.waits.lock().unwrap(), [2, 4, 2].map(Duration::from_secs));
    }

    #[test]
    fn concurrent_hints_claim_only_one_live_resolver() {
        let task = Arc::new(Mutex::new(LeagueTask::default()));
        let workers: Vec<_> = (0..16).map(|_| {
            let task = task.clone();
            std::thread::spawn(move || task.lock().unwrap().trigger())
        }).collect();
        let spawned = workers.into_iter().map(|worker| usize::from(worker.join().unwrap())).sum::<usize>();
        assert_eq!(spawned, 1);
        assert!(task.lock().unwrap().pending);
        // A resolved UI flag does not release the live task's claim.
        assert!(!task.lock().unwrap().trigger());
    }

    #[test]
    fn superseded_server_response_cannot_replace_current_resolution() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "B".into());
        assert!(accept_league_response(&ssot, ("new-server", 1), ("old-server", 0), "A".into()).is_none());
        let snapshot = ssot.lock().unwrap();
        assert_eq!((snapshot.league.name.as_deref(), snapshot.league.generation), (Some("B"), 1));
    }

    #[test]
    fn returning_to_a_server_url_still_rejects_its_old_response() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "B".into());
        assert!(accept_league_response(&ssot, ("server", 2), ("server", 0), "A".into()).is_none());
        let snapshot = ssot.lock().unwrap();
        assert_eq!((snapshot.league.name.as_deref(), snapshot.league.generation), (Some("B"), 1));
    }

    struct Hint<'a>(&'a Runtime);
    impl LeagueResolver for Hint<'_> {
        fn current_league(&self) -> Option<String> { self.0.ssot.lock().unwrap().league.name.clone() }
        fn rearm(self) { self.0.scheduler.trigger(); }
    }

    #[test]
    fn event_hint_resolves_authoritative_status_instead_of_applying_the_hint() {
        let (runtime, _clock, _network) = runtime(vec![Some("A"), Some("authoritative")]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        note_server_league(Hint(&runtime), Some("untrusted-hint"));
        assert_eq!(runtime.ssot.lock().unwrap().league.name.as_deref(), Some("A"));
        advance(resolver.as_mut());
        assert_eq!(*runtime.attempts.lock().unwrap(), 2);
        assert_eq!(runtime.ssot.lock().unwrap().league.name.as_deref(), Some("authoritative"));
        assert!(runtime.waits.lock().unwrap().is_empty());
    }

    enum PendingTrigger { HintsAndRefresh, Unknown, Refresh }

    fn parked_response_preserves_trigger(trigger: PendingTrigger) {
        let (runtime, _clock, network) = runtime(vec![Some("A")]);
        let mut resolver = std::pin::pin!(run_league_resolver(&runtime));
        advance(resolver.as_mut());
        assert_eq!(runtime.ssot.lock().unwrap().league.name.as_deref(), Some("A"));
        // Start the parked A request from a real idle Notify wait. Removing
        // the production wake must break this regression before any response.
        assert!(!runtime.scheduler.trigger(), "Refresh wakes the existing idle task");
        advance(resolver.as_mut());
        assert_eq!(*runtime.attempts.lock().unwrap(), 2, "Refresh's A request is parked on the network");

        match trigger {
            PendingTrigger::HintsAndRefresh => {
                note_server_league(Hint(&runtime), Some("B"));
                note_server_league(Hint(&runtime), None);
                assert!(!runtime.scheduler.trigger(), "Refresh wakes the existing task");
            }
            PendingTrigger::Unknown => note_server_league(Hint(&runtime), None),
            PendingTrigger::Refresh => {
                assert!(!runtime.scheduler.trigger(), "Refresh wakes the existing task");
            }
        }
        assert_eq!(runtime.ssot.lock().unwrap().league.name.as_deref(), Some("A"));
        network.send(Some("A")).unwrap();
        advance(resolver.as_mut());
        assert_eq!(*runtime.attempts.lock().unwrap(), 3, "a trigger received during A requires a subsequent authoritative request");

        network.send(Some("B")).unwrap();
        advance(resolver.as_mut());
        let snapshot = runtime.ssot.lock().unwrap();
        assert_eq!((snapshot.league.name.as_deref(), snapshot.league.generation), (Some("B"), 2));
        assert!(!snapshot.resolving);
        drop(snapshot);
        for _ in 0..100 { advance(resolver.as_mut()); }
        assert_eq!(*runtime.attempts.lock().unwrap(), 3, "all prior triggers coalesce into the B request, then idle");
        assert!(runtime.waits.lock().unwrap().is_empty(), "successful requests never arm a timer");
    }

    #[test]
    fn hints_and_refresh_during_a_request_resolve_again_before_idle() {
        parked_response_preserves_trigger(PendingTrigger::HintsAndRefresh);
    }

    #[test]
    fn unknown_hint_during_a_request_resolves_again_before_idle() {
        parked_response_preserves_trigger(PendingTrigger::Unknown);
    }

    #[test]
    fn refresh_during_a_request_resolves_again_before_idle() {
        parked_response_preserves_trigger(PendingTrigger::Refresh);
    }
}

#[cfg(test)]
mod league_publication_tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::Mutex;

    fn resolve(ssot: &Mutex<AppSsotSnapshot>, name: &str) -> Vec<String> {
        let changed = write_league(ssot, name.into());
        let events = RefCell::new(Vec::new());
        deliver_league_resolution(changed,
            || { events.borrow_mut().push("market-wake".into()); },
            || {
                // The snapshot emitter must be able to re-lock the holder.
                let snapshot = ssot.try_lock().expect("no lock across emission");
                events.borrow_mut().push(format!("ssot:{}:{}", snapshot.league.name.as_deref().unwrap(), snapshot.league.generation));
            },
            |changed| { events.borrow_mut().push(format!("changed:{}:{}", changed.league, changed.generation)); });
        events.into_inner()
    }

    #[test]
    fn first_resolution_publishes_snapshot_without_consumer_change() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        assert_eq!(resolve(&ssot, "A"), ["market-wake", "ssot:A:1"]);
    }

    #[test]
    fn real_change_publishes_one_consumer_event_after_snapshot() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "A".into());
        assert_eq!(resolve(&ssot, "B"), ["market-wake", "ssot:B:2", "changed:B:2"]);
    }

    #[test]
    fn same_name_resolution_still_wakes_market() {
        let ssot = Mutex::new(AppSsotSnapshot::default());
        write_league(&ssot, "A".into());
        assert_eq!(resolve(&ssot, "A"), ["market-wake", "ssot:A:1"]);
    }
}
