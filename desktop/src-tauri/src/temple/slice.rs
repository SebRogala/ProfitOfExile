//! The `temple` SSOT slice and the pure projection that fills it (POE-171).
//!
//! Everything in this file is a **wire type plus the pure function that builds
//! it**: a [`reader::TempleLayout`], the per-plate [`panel::RoomReading`]s, the
//! [`panel::PanelReading`] and an optional [`advisor::Advice`] go in, a
//! [`TempleSlice`] comes out. No capture, no OCR, no Tauri — so the whole
//! projection is exercised on Linux, and the Windows glue in [`super::run`] is
//! only the part that fetches the inputs.
//!
//! # Why the domain types are not published directly
//!
//! `Slot`, `Edge`, `Match`, `Decision` and `Reason` are reasoning types, not
//! wire types: none of them is `Serialize`, several carry `&'static` references
//! into the vocabulary tables, and their shapes are chosen for the rollout
//! kernel rather than for a webview. Projecting into flat strings and numbers
//! here is what keeps POE-167..170 free of serde and free of any obligation to
//! the overlay's shape. It follows `mercenary::MercenarySlice`, which does the
//! same for the merc capture.
//!
//! # What the projection refuses to do
//!
//! - **Guess an unread plate.** [`rooms::Match::Unknown`] lands in
//!   [`TempleSlice::unknown_rooms`] and the plate is published with
//!   `known: false`. The advisor still runs; Unknown is junk, not a stop.
//! - **Rank without a position.** `layout.current == None` publishes the layout
//!   and [`TempleStatus::NoCurrentRoom`] with `advice: None` — the advisor is
//!   not called at all, because "between rooms" is not a decision.
//! - **Invent the current room's corridors.** When the diamond read fails, the
//!   door set falls back to `doors − uncertain` and every edge incident to the
//!   current room is published in
//!   [`LayoutView::unresolved_incident`] instead of being silently included or
//!   silently dropped. See [`super::run::diamond_rect`].

use std::collections::BTreeSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::advisor::{self, Advice, MapAction, Warning};
use super::anchor::AnchorCalibration;
use super::doors::Confidence;
use super::lattice::{Edge, Lattice, Slot};
use super::markers;
use super::panel::{ArchitectOffer, PanelReading, RoomReading};
use super::reader::TempleLayout;
use super::rooms::{self, Match, OfferKind, RoomIdentity};
use super::strategy::{Mode, StrategyProfile, TempleConfig, Tier};

// ---------------------------------------------------------------- settings --

/// The profile fields a user may set (POE-167 §4: *everything a player might
/// disagree on is a profile field, never a code branch*).
///
/// Four of [`StrategyProfile`]'s fields, not the whole struct: `room_values`,
/// `combinations` and `mode_rule` describe what "Locus + Doryani Rush" *is*,
/// and a user editing them is choosing a different strategy rather than tuning
/// this one. Those arrive as a second shipped profile, not as settings JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TempleProfileSettings {
    /// What the Apex is worth on its own.
    pub apex_score: f64,
    /// Run-time traversal weight per BFS hop from the Entrance. 0 for the Rush.
    pub path_cost: f64,
    /// Prefer `change` over `upgrade` while no favourable line exists.
    pub reroll_until_favourable: bool,
    /// R4's carve-out: keep a slot in the drop pool while an adjacent upgrade
    /// room can still hit it.
    pub r4_keep_upgrade_targets: bool,
}

impl Default for TempleProfileSettings {
    /// The Rush's own values, read off [`StrategyProfile::locus_doryani_rush`]
    /// rather than typed in — a defaults table that can drift from the profile
    /// it defaults to is a defaults table that will.
    fn default() -> Self {
        let rush = StrategyProfile::locus_doryani_rush();
        TempleProfileSettings {
            apex_score: rush.apex_score,
            path_cost: rush.path_cost,
            reroll_until_favourable: rush.reroll_until_favourable,
            r4_keep_upgrade_targets: rush.r4_keep_upgrade_targets,
        }
    }
}

/// Largest number of opening stones one incursion can drop.
pub const MAX_KEYS: u8 = 2;

impl TempleProfileSettings {
    /// The Rush with these four fields applied.
    pub fn to_profile(&self) -> StrategyProfile {
        StrategyProfile {
            apex_score: self.apex_score,
            path_cost: self.path_cost,
            reroll_until_favourable: self.reroll_until_favourable,
            r4_keep_upgrade_targets: self.r4_keep_upgrade_targets,
            ..StrategyProfile::locus_doryani_rush()
        }
    }

    /// Reject a profile the scorer cannot use.
    ///
    /// Both weights are *magnitudes* the objective function adds and
    /// subtracts. A negative `apex_score` would make the Apex a penalty and a
    /// negative `path_cost` would pay the player to walk further — neither is
    /// a preference, both are a sign error. NaN is rejected for the reason
    /// every comparison in the ranking is a float compare: one NaN makes the
    /// whole ordering arbitrary.
    pub fn validate(&self) -> Result<(), String> {
        if !self.apex_score.is_finite() || self.apex_score < 0.0 {
            return Err(format!(
                "apex_score must be a finite number ≥ 0, got {}",
                self.apex_score
            ));
        }
        if !self.path_cost.is_finite() || self.path_cost < 0.0 {
            return Err(format!(
                "path_cost must be a finite number ≥ 0, got {}",
                self.path_cost
            ));
        }
        Ok(())
    }
}

/// Everything the temple module persists — the user's profile tuning, the two
/// config flags and the key count.
///
/// One `AppState` Mutex holds this whole struct while `settings.json` keeps
/// three separate fields, so a hand-edited file stays readable and one bad
/// field defaults on its own (`#[serde(default)]` per field on `Settings`).
///
/// **No calibration field.** It held one until POE-234 WI-2 — the remembered
/// anchor scale, keyed on the capture size — and that was the app's SECOND
/// store of one fact: what scale this screen draws the game's UI at. The shared
/// `crate::ssot::ScreenSlice` is the first and now the only one; the temple
/// derives its hint from it through `anchor::TEMPLE_SCALE_PER_UI_SCALE` on every
/// tick (`super::run::hint_for_capture`) and publishes what it anchors back.
/// [`AnchorCalibration`] survives as what it always described — a (screen size,
/// scale) pair produced BY a read — on [`super::reader::TempleLayout`],
/// [`super::anchor::CheapHint`] and [`TempleSlice::calibration`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TempleSettings {
    pub profile: TempleProfileSettings,
    pub config: TempleConfig,
    /// Opening stones this incursion dropped. The panel does not print it, so
    /// the user sets it; 1 is the common case.
    pub keys: u8,
}

/// The common case: one opening stone dropped. `u8::default()` is 0, which is
/// a legal but uncommon board, so this is the number every default path uses.
pub fn default_keys() -> u8 {
    1
}

impl TempleSettings {
    /// The shipped defaults. `Default::default()` gives `keys: 0`, which is a
    /// legal but uncommon board — go through here, never through the derive.
    pub fn shipped() -> TempleSettings {
        TempleSettings {
            profile: TempleProfileSettings::default(),
            config: TempleConfig::default(),
            keys: default_keys(),
        }
    }
}

/// Reject a key count the game cannot produce.
pub fn validate_keys(keys: u8) -> Result<(), String> {
    if keys > MAX_KEYS {
        return Err(format!(
            "an incursion drops at most {MAX_KEYS} opening stones, got {keys}"
        ));
    }
    Ok(())
}

// ------------------------------------------------------------- wire types --

/// What the module is doing, in the one field a page can switch on.
///
/// `Error` carries no message: the text lives in [`TempleSlice::last_error`],
/// matching [`crate::mercenary::MercStatus`]. One flat string enum keeps the
/// TypeScript side a plain union instead of a tagged one, and the two fields
/// are written together in the same publish.
///
/// `snake_case` wire strings, matching [`crate::mercenary::MercStatus`] and the
/// convention `desktop/src/lib/README.md` records for this app's enums
/// (camelCase FIELDS, snake_case enum VARIANTS). The struct views in this file
/// keep `camelCase` because that attribute renames their fields, not variants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempleStatus {
    /// The module is disabled — no loop, and no overlay.
    Off,
    /// Running, and nothing read yet.
    ///
    /// Not the unfocused state: the loop keeps whatever it last published while
    /// the game is alt-tabbed and does not publish at all (see
    /// [`super::run`]'s focus gate), so an alt-tab leaves the previous status
    /// standing rather than reverting to this one.
    #[default]
    Idle,
    /// Running, and NOT capturing: nothing in Client.txt has put an incursion
    /// in scope (POE-242).
    ///
    /// The loop's resting state, and where a session spends nearly all of its
    /// time. Distinct from [`Self::PanelNotVisible`] on the axis that matters
    /// to a reader wondering why nothing is happening: `panel_not_visible` is
    /// the module having LOOKED and seen no panel, this is the module not
    /// looking. See `super::trigger` for what arms it — an Alva voice line, the
    /// temple area, or the Re-arm button.
    Waiting,
    /// The loop looked and found no layout panel.
    PanelNotVisible,
    /// A full read is in flight.
    Reading,
    /// A board is published and current.
    Read,
    /// The panel was open between rooms — the layout is published, the advice
    /// is not.
    NoCurrentRoom,
    /// Capture or OCR is not available on this host / in this build.
    Unavailable,
    /// The last attempt failed; see [`TempleSlice::last_error`].
    Error,
}

/// One of the 13 plates, as read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotView {
    /// `"A0"`… `"E2"`.
    pub slot: String,
    /// The game's own name for what was read, or `None` for an unread plate.
    pub name: Option<String>,
    /// 0 for the Entrance, the Apex, a filler and an unread plate.
    pub tier: u8,
    /// The name matched the vocabulary exactly (as opposed to fuzzily).
    pub exact: bool,
    /// `false` means the plate is unread — drawn as such, never guessed.
    pub known: bool,
    /// The room the player is standing in.
    pub current: bool,
}

/// The board, as pixels gave it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutView {
    /// 13 entries, in [`Slot::ALL`] order.
    pub slots: Vec<SlotView>,
    /// Corridors to act on — the settled set when the diamond read succeeded,
    /// `doors − uncertain` when it did not.
    pub doors: Vec<String>,
    /// Every corridor incident to the current room, exactly as the reader
    /// reported it — a DIAGNOSTIC about the read, never a door state (POE-248).
    ///
    /// [`super::doors::read_doors`] puts all of them here unconditionally,
    /// before any open/closed judgement, because the gold selection frame
    /// covers their midpoints. What settles them is the diamond read, and its
    /// answer is already in [`Self::doors`] — so an edge the seals read GREEN
    /// is in BOTH lists, and the webview's `edgeState` used to colour it as
    /// unsettled on the strength of this one. It no longer reads it; nothing
    /// should. The honest "nothing settled this" signal is
    /// [`Self::unresolved_incident`].
    pub uncertain: Vec<String>,
    /// Corridors incident to the current room that NOTHING settled — populated
    /// only on the diamond-read fallback. Surfaced, never guessed.
    pub unresolved_incident: Vec<String>,
    /// Why the corridors are unresolved, when they are.
    pub marker_error: Option<String>,
    pub current: Option<String>,
    pub scale: f32,
    pub ncc: f32,
    /// `"high"` or `"low"` — [`Confidence::Low`] means the door sets are a best
    /// effort over an unreadable panel and nothing should act on them.
    pub confidence: String,
    /// Entrance plate centre in CAPTURE px — the origin the board hangs off
    /// (POE-227). Published so a surface that draws ON the game (a door arrow,
    /// a kill marker) can place itself against the board the reader actually
    /// read, instead of re-deriving a lattice from `scale` and guessing where it
    /// starts.
    ///
    /// Capture px means whole-primary-monitor px (`crate::capture`), which is
    /// also window-relative px for a monitor-sized overlay — no conversion.
    /// NOT reference px and NOT CSS px.
    pub origin: [i32; 2],
    /// The 13 plate centres in capture px, in [`Slot::ALL`] order — the same
    /// order and the same unit as `origin` above, and the same order `slots`
    /// uses, so index `i` of one describes the plate at index `i` of the other.
    ///
    /// Published rather than derived on the far side: they are
    /// `Lattice::new(origin, scale)`'s centres verbatim, and a second
    /// implementation of that rounding in TypeScript would be a second answer
    /// to where a plate is.
    ///
    /// A fixed 13 rather than a `Vec` (which `slots` above is): the board has
    /// exactly 13 plates by construction — [`Lattice`] carries the same array —
    /// so a length is not something a consumer should have to check.
    pub centres: [[i32; 2]; 13],
    /// Every rectangle on screen this read took its INPUT from, in capture px
    /// (POE-244). The never-cover set: a surface drawing over the game must
    /// keep clear of all of them, because the module reads them again on the
    /// next tick and a panel drawn over one is OCR input the module wrote
    /// itself.
    ///
    /// Published rather than re-derived by the consumer. There are five sources
    /// — [`super::run::panel_rect`], [`super::run::diamond_rect`],
    /// [`super::run::remaining_rect`], [`super::panel::name_strip`] /
    /// [`super::panel::numeral_box`] and [`Lattice::edge_midpoint`] — and a
    /// TypeScript copy of any of them would be a second answer to where the
    /// module is looking, with nothing to fail when the two drifted.
    /// [`super::run::read_rois`] is the one builder.
    pub rois: Vec<RoiView>,
    /// The current room's own diamond — the shape the side panel draws and the
    /// seals on it, for a surface that has to draw it after the panel is gone
    /// (POE-244). `None` when the read settled no current room, which is when
    /// there is no room to draw.
    pub diamond: Option<DiamondView>,
}

/// One rectangle the read takes input from, in CAPTURE px.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoiView {
    /// `"panel"`, `"diamond"`, `"remaining"`, `"plate"` or `"corridor"` — what
    /// the module reads there. Carried so a debug surface can name a rect; the
    /// never-cover rule itself treats all five the same.
    pub kind: String,
    /// The board element the rect belongs to: a slot key for `plate`, an edge
    /// id for `corridor`, `None` for the three panel regions, which belong to
    /// the panel rather than to a plate.
    pub of: Option<String>,
    /// `[x, y, w, h]`, capture px — the same unit as [`LayoutView::origin`].
    pub rect: [i32; 4],
}

/// The room's isometric diamond, as the side panel draws it.
///
/// A UNIT shape, not a screen rectangle: the panel's own diamond has its rect
/// in [`LayoutView::rois`], and this is the geometry a widget needs to draw the
/// SAME shape somewhere else, at whatever size the user has dragged it to. Both
/// fields are in the units of [`super::markers::diamond_corners`] — centre at
/// the origin, `+y` down — so a consumer scales the corners into its box and
/// puts every seal through the same transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiamondView {
    /// The outline, four corners in ring order.
    pub corners: [[f64; 2]; 4],
    /// One seal per corridor the current room has, in
    /// [`super::lattice::neighbours`] order.
    pub seals: Vec<SealView>,
    /// The architect icon spot in the room's TOP-RIGHT half, in
    /// [`Self::corners`]' units (POE-248) — the one the panel's first (topmost)
    /// architect block belongs to.
    ///
    /// Carried rather than derived by the widget for the reason every other
    /// number here is: it is a MEASUREMENT of the panel
    /// ([`super::markers::ARCHITECT_ICON_OFFSET`]), and a TypeScript copy of it
    /// would be a second answer that a re-measure leaves behind.
    ///
    /// Named for the HALF and not for a kind of kill: which architect's icon
    /// the game draws where is the one thing the measurement does not settle,
    /// and the webview keys the glyph on the chosen block's own OCR rect.
    pub top_icon: [f64; 2],
    /// The spot in the room's BOTTOM-LEFT half — the mirror of
    /// [`Self::top_icon`] through the room's centre, and the second block's.
    pub bottom_icon: [f64; 2],
}

/// One seal on the room's diamond.
///
/// What it is NOT carrying is deliberate. Open/closed/uncertain is
/// [`LayoutView::doors`] / `uncertain` / `unresolved_incident` read through the
/// consumer's own edge-state rule, which every temple surface already shares;
/// and whether the advisor wants this door opened is membership of
/// [`RankedView::doors`]. Repeating either here would be a second answer to a
/// question the slice already answers, and the two could disagree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SealView {
    /// The slot this corridor leads to — `"C2"`.
    pub neighbour: String,
    /// The corridor itself — `"C1-C2"`, the key `doors` and `uncertain` use.
    pub edge: String,
    /// `[x, y]` ON THE ROOM'S WALL, in [`DiamondView::corners`]' units
    /// (POE-248).
    ///
    /// Not a unit vector and not a ring: the room is a rectangle, a door is a
    /// hole in one of its four walls, and this is where the corridor's own
    /// direction leaves the outline
    /// ([`super::markers::seal_position`]). The two same-row corridors land at
    /// exactly 1.0 — the midpoint of a short wall — and the four diagonals at
    /// 0.938 and 1.034, two to each long wall.
    pub pos: [f64; 2],
}

/// One architect block, resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferView {
    /// Position in the panel, so the overlay can point at the right block.
    pub index: usize,
    pub architect_name: String,
    /// `"change"` or `"upgrade"`.
    pub kind: String,
    /// What the panel printed. Kept because it is what the player sees.
    pub printed_target: String,
    /// What the kill actually builds — `None` when the printed name did not
    /// resolve. **Not** the printed name (POE-169: Contested Development turns
    /// `change` into `currentTier + 1` of the named line).
    pub display_name: Option<String>,
    /// The tier the kill guarantees. An `upgrade` also rolls one more at 50%.
    pub built_tier: Option<u8>,
    /// `[x, y, w, h]` of the block on screen, CAPTURE px — the union of the
    /// boxes of the OCR lines it was read from (POE-243). `null` when the read
    /// carried no boxes.
    ///
    /// Capture px is whole-primary-monitor px, which is also window-relative px
    /// for a monitor-sized overlay: the same unit as `LayoutView::origin` and
    /// `centres`, and no conversion for a surface drawing over the game.
    pub rect: Option<[i32; 4]>,
}

/// The side panel, as text gave it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelView {
    /// The panel's title — the current room's name.
    pub room: Option<String>,
    /// `[x, y, w, h]` of the title line on screen, CAPTURE px — the same unit
    /// and the same purpose as [`OfferView::rect`]. `null` when the title was
    /// unread or the read carried no boxes.
    pub room_rect: Option<[i32; 4]>,
    pub offers: Vec<OfferView>,
    /// `None` means the line was not legible; every rollout then terminates
    /// immediately and the scores are the board as it stands.
    pub incursions_remaining: Option<u8>,
}

/// One ranked move.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedView {
    /// `"upgrade → Locus of Corruption"`, or `"kill either"`.
    pub headline: String,
    /// `"C1-C2, B0-C1"`, or `"no door"`.
    pub doors_label: String,
    pub doors: Vec<String>,
    /// Which architect block to point at.
    pub architect_index: Option<usize>,
    pub ev: f64,
    /// Fraction of rollouts that finished below the profile's "lost the room"
    /// threshold. `None` on the recommended side — RV did not exclude it.
    pub risk: Option<f64>,
    /// One line per rule that put the option here. A bare score cannot be
    /// audited.
    pub reasons: Vec<String>,
}

/// The decision, with everything needed to justify it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdviceView {
    /// Best first.
    pub recommendations: Vec<RankedView>,
    /// The RV-excluded options, best first, each with its measured risk.
    pub gambles: Vec<RankedView>,
    /// `"continue"` or `"leaveMap"`. R5's verdict for the top recommendation —
    /// as prominent as the kill when it says leave.
    pub map_action: String,
    pub warnings: Vec<String>,
    /// Whether the kill on the top recommendation is the ONLY kill the read
    /// saw, rather than the best of the two the panel prints (POE-243).
    ///
    /// The typed half of `Warning::PartialArchitects`: `warnings` carries its
    /// prose, which is what a surface prints, and this carries the fact, which
    /// is what a surface branches on. A page that had to recognise the warning
    /// by its text would break the first time the wording changed.
    ///
    /// False when nothing was read at all, and false when the one block that
    /// was read did not resolve — there is no kill on the headline to qualify
    /// in either case, and the warning says what happened in its own words.
    ///
    /// `serde(default)` so a payload from a build before POE-243 decodes as
    /// "not forced" rather than failing the whole slice. The webview mirror
    /// treats a missing value the same way (`forcedKillNote`), so the two ends
    /// agree about what silence means.
    #[serde(default)]
    pub forced_kill: bool,
}

/// The `temple` SSOT slice (POE-171).
///
/// Owned by `AppState.temple`, written by [`super::run`] and the commands,
/// projected read-only into every snapshot by `ssot::build_snapshot`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TempleSlice {
    pub status: TempleStatus,
    pub layout: Option<LayoutView>,
    pub panel: Option<PanelView>,
    /// `None` whenever there is no decision to make — no board, or no current
    /// room.
    pub advice: Option<AdviceView>,
    /// Chase or Scarab, from the profile's own selector. `None` with no
    /// advice.
    pub mode: Option<String>,
    /// The user's key count, echoed so the overlay does not need a second
    /// command to render its own control.
    pub keys: u8,
    /// The two config flags in force, echoed for the same reason as [`Self::keys`]
    /// — the page renders the controls it owns from ONE source, and there is no
    /// getter command to ask a second time. Settings, not a reading: unlike
    /// [`Self::advice`] it survives [`force_off`], because a switched-off module's
    /// settings are still the settings the next read will use.
    pub config: TempleConfig,
    /// The four tunable profile fields in force. Same ownership and same
    /// survival rule as [`Self::config`].
    pub profile: TempleProfileSettings,
    /// Slots whose plate did not resolve, by key. Surfaced, never hidden — the
    /// player can then see which rooms the advisor is treating as junk.
    pub unknown_rooms: Vec<String>,
    /// Unix ms of the last completed read.
    pub last_read_at: Option<u64>,
    /// The anchor scale in force, keyed on the capture size it was measured at.
    pub calibration: Option<AnchorCalibration>,
    /// Something the last read could not do, stated as a WARNING rather than as
    /// a failure (POE-230 follow-up): today, a text ROI that fell entirely
    /// outside the capture, which produces an empty panel read that looks
    /// exactly like a panel with nothing printed on it.
    ///
    /// Deliberately NOT [`Self::last_error`]. That field belongs to the
    /// status/message machine — `super::run::fail` writes it together with
    /// [`TempleStatus::Error`], and the page renders it in red as "Last error".
    /// A read that COMPLETED and published a board is not an error and must not
    /// wear one; the difference between "this module is broken" and "this read
    /// was short one region" is the whole point of a second field. Written and
    /// cleared by [`project`] alone, from the read it describes, so it can
    /// never outlive the read the way a status-machine field can.
    #[serde(default)]
    pub read_notice: Option<String>,
    pub last_error: Option<String>,
}

// ------------------------------------------------------------ projection --

/// Everything one completed read produced, before it becomes a slice.
///
/// A struct rather than six positional arguments because the two that are
/// `Option` — the advice and the settled door set — are exactly the two a
/// caller could otherwise swap or drop silently.
pub struct ReadResult<'a> {
    pub layout: &'a TempleLayout,
    pub rooms: &'a [RoomReading],
    pub panel: &'a PanelReading,
    /// The door set the diamond settled, or `None` on the marker fallback.
    pub settled: Option<&'a BTreeSet<Edge>>,
    /// Why the diamond read failed, when it did.
    pub marker_error: Option<String>,
    /// A warning about this read that is not a failure of it — see
    /// [`TempleSlice::read_notice`]. `None` on a read with nothing to report,
    /// which is what CLEARS the field on the slice.
    pub read_notice: Option<String>,
    pub advice: Option<&'a Advice>,
    pub keys: u8,
    /// The config flags this read was ranked under — echoed onto the slice, not
    /// used by the projection. Carried here rather than read from state because
    /// `project` is pure and the whole slice is written in one replace: a field
    /// the projection does not set is a field every read would blank.
    pub config: TempleConfig,
    /// The profile this read was ranked under. Same reason as [`Self::config`].
    pub profile: TempleProfileSettings,
    pub read_at: u64,
}

fn edge_labels(edges: impl IntoIterator<Item = Edge>) -> Vec<String> {
    edges.into_iter().map(|e| e.to_string()).collect()
}

/// The identity a plate read, with the two facts the slice publishes about it.
fn slot_view(reading: &RoomReading, current: Option<Slot>) -> SlotView {
    let identity = reading.identity.identity();
    SlotView {
        slot: reading.slot.as_str().to_string(),
        name: identity.map(|id| id.display_name().to_string()),
        tier: identity.map_or(0, |id| id.tier().get()),
        exact: matches!(reading.identity, Match::Exact(_)),
        known: identity.is_some(),
        current: current == Some(reading.slot),
    }
}

/// The 13 identities in the shape [`advisor::state::BoardState::from_reading`]
/// takes.
///
/// Indexed by [`Slot::index`] rather than by position in `rooms`, so a caller
/// that hands over a short or reordered vector gets `None` plates instead of a
/// board with the wrong rooms in it.
pub fn identities(rooms: &[RoomReading]) -> [Option<RoomIdentity>; 13] {
    let mut out = [None; 13];
    for reading in rooms {
        out[reading.slot.index()] = reading.identity.identity();
    }
    out
}

/// The plates that did not resolve, by slot key, in board order.
pub fn unknown_rooms(rooms: &[RoomReading]) -> Vec<String> {
    let mut out: Vec<(usize, String)> = rooms
        .iter()
        .filter(|r| !r.identity.is_known())
        .map(|r| (r.slot.index(), r.slot.as_str().to_string()))
        .collect();
    out.sort_by_key(|(index, _)| *index);
    out.into_iter().map(|(_, key)| key).collect()
}

fn layout_view(read: &ReadResult<'_>) -> LayoutView {
    let layout = read.layout;
    let doors: BTreeSet<Edge> = match read.settled {
        Some(settled) => settled.clone(),
        None => layout.doors.difference(&layout.uncertain).copied().collect(),
    };
    // On the fallback the current room's corridors were never settled by
    // anything: the beam sampler flagged them uncertain precisely because the
    // selection frame covers them. Publishing them as unresolved is the whole
    // honesty guard — the alternative is a page that cannot tell "closed" from
    // "we could not see it".
    let unresolved = match read.settled {
        Some(_) => Vec::new(),
        None => edge_labels(layout.uncertain.iter().copied()),
    };
    LayoutView {
        slots: read
            .rooms
            .iter()
            .map(|r| slot_view(r, layout.current))
            .collect(),
        doors: edge_labels(doors),
        uncertain: edge_labels(layout.uncertain.iter().copied()),
        unresolved_incident: unresolved,
        marker_error: read.marker_error.clone(),
        current: layout.current.map(|s| s.as_str().to_string()),
        scale: layout.scale,
        ncc: layout.ncc,
        confidence: match layout.confidence {
            Confidence::High => "high".to_string(),
            Confidence::Low => "low".to_string(),
        },
        origin: [layout.origin.0, layout.origin.1],
        // `TempleLayout.slots` IS `Lattice::new(origin, scale).centres` — the
        // reader fills it from exactly that (`reader::read_layout_at`), so this
        // republishes the lattice the board was read off rather than rebuilding
        // one that could round differently.
        centres: layout.slots.map(|(x, y)| [x, y]),
        rois: super::run::read_rois(layout.origin, layout.scale),
        diamond: layout.current.map(|current| diamond_view(layout, current)),
    }
}

/// The current room's diamond, as [`super::markers`]' fitted projection puts
/// it.
///
/// One seal per NEIGHBOUR the board gives the room — `lattice::neighbours`,
/// which is the same source the marker reader counts its expected seals from
/// (`run::read_markers` passes that degree in). Not one per OPEN corridor: a
/// closed door is a red seal the player has to see, and a room whose seals were
/// never settled still has walls in those directions.
fn diamond_view(layout: &TempleLayout, current: Slot) -> DiamondView {
    let lattice = Lattice::new(layout.origin, layout.scale);
    let (top, bottom) = markers::architect_icons();
    DiamondView {
        corners: markers::diamond_corners().map(|(x, y)| [x, y]),
        seals: super::lattice::neighbours(current)
            .into_iter()
            .map(|to| {
                // The wall IS the seal's place: the room is a rectangle and a
                // door is a hole in one of its sides, so this is the corridor's
                // direction intersected with the outline (`markers`, POE-248).
                // Nothing to scale here, and one home for the projection.
                let (x, y) = markers::seal_position(&lattice, current, to);
                SealView {
                    neighbour: to.as_str().to_string(),
                    edge: Edge::new(current, to).to_string(),
                    pos: [x, y],
                }
            })
            .collect(),
        top_icon: [top.0, top.1],
        bottom_icon: [bottom.0, bottom.1],
    }
}

/// The room the player is standing in, as this read established it, together
/// with the reason to distrust it — the ONE place that question is answered
/// (POE-229).
///
/// # Why the layout plate comes first
///
/// The plate is *positionally pinned*: it is cropped from the slot the lattice
/// puts under the cursor, and its name is cross-checked against the tier
/// numeral printed on the same plate ([`rooms::cross_check_numeral`]), which
/// demotes a disagreeing read to [`Match::Unknown`] rather than passing it on.
/// The side-panel title has neither guard: [`super::panel::read_panel`] picks
/// it heuristically out of whole-screen OCR, where the layout panel's own plate
/// names interleave with the side panel's lines — [`super::panel::SCREEN_FURNITURE`]
/// exists because that pick goes wrong.
///
/// So the title is the *fallback*, and it earns its place: the gold selection
/// frame overhangs the current plate, so that plate is the one most likely to
/// read `Unknown`, and the title is then the only source left. That is POE-229 —
/// standing in **Office of Cartography** with C2 unread, an offer was ranked
/// and printed against tier 0.
///
/// # `None` is not tier 0
///
/// A filler, the Entrance and the Apex are all genuinely tier 0, and an offer
/// taken there really does build tier 1. `None` is the different fact that
/// *neither source named the room*, and [`rooms::resolve_offer_for`] refuses to
/// do Contested Development's arithmetic against it rather than printing a room
/// the kill will not build.
pub fn current_identity(
    current: Option<Slot>,
    identities: &[Option<RoomIdentity>; 13],
    panel: &PanelReading,
) -> CurrentRoom {
    let plate = current.and_then(|slot| identities[slot.index()]);
    let title = panel.room.identity();
    // Both read and they name different rooms: one of the two OCR passes is
    // wrong and nothing here can tell which, so the guarded source is kept and
    // the conflict is handed back to be said out loud. Resolving it silently is
    // what makes a wrong room look like a certain one.
    let disagreement = match (title, plate) {
        (Some(title), Some(plate)) if title != plate => {
            Some((title.display_name(), plate.display_name()))
        }
        _ => None,
    };
    CurrentRoom {
        identity: plate.or(title),
        disagreement,
    }
}

/// What [`current_identity`] established about the room the player is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentRoom {
    /// The room, or `None` when neither source named it.
    pub identity: Option<RoomIdentity>,
    /// `(title, plate)` display names, when both sources resolved and named
    /// different rooms. [`Self::identity`] is the plate's in that case.
    pub disagreement: Option<(&'static str, &'static str)>,
}

/// The tier of the slot the player is standing in — what
/// [`rooms::resolve_offer_for`] needs to turn a printed target into the room the
/// kill actually builds. `None` when [`current_identity`] named no room.
fn current_tier(read: &ReadResult<'_>) -> Option<Tier> {
    current_identity(read.layout.current, &identities(read.rooms), read.panel)
        .identity
        .map(|id| id.tier())
}

fn offer_view(index: usize, offer: &ArchitectOffer, current_tier: Option<Tier>) -> OfferView {
    let resolved = match rooms::resolve_offer_for(&offer.printed_target, offer.kind, current_tier) {
        rooms::OfferResolution::Built(resolved) => Some(resolved),
        // Both failures publish the printed target with no resolved room: the
        // page has the architect's own wording and nothing invented on top of
        // it. Which failure it was reaches the page as an advisor warning.
        rooms::OfferResolution::UnknownName | rooms::OfferResolution::UnknownCurrentTier => None,
    };
    OfferView {
        index,
        architect_name: offer.architect_name.clone(),
        kind: match offer.kind {
            OfferKind::Change => "change".to_string(),
            OfferKind::Upgrade => "upgrade".to_string(),
        },
        printed_target: offer.printed_target.clone(),
        display_name: resolved.as_ref().map(|r| r.display_name.to_string()),
        built_tier: resolved.as_ref().map(|r| r.built_tier.get()),
        // Published verbatim: the reader already put it in capture px, and
        // re-deriving a rect from anything else here would be a second answer
        // to where the panel drew the block.
        rect: offer.rect,
    }
}

fn panel_view(read: &ReadResult<'_>) -> PanelView {
    let tier = current_tier(read);
    PanelView {
        room: read
            .panel
            .room
            .identity()
            .map(|id| id.display_name().to_string()),
        room_rect: read.panel.room_rect,
        offers: read
            .panel
            .architects
            .iter()
            .enumerate()
            .map(|(index, offer)| offer_view(index, offer, tier))
            .collect(),
        incursions_remaining: read.panel.incursions_remaining,
    }
}

fn advice_view(advice: &Advice) -> AdviceView {
    AdviceView {
        recommendations: advice
            .recommendations
            .iter()
            .map(|r| RankedView {
                headline: r.option.headline(),
                doors_label: r.option.doors_label(),
                doors: edge_labels(r.option.doors.iter().copied()),
                architect_index: r.option.architect.as_ref().map(|a| a.offer_index),
                ev: r.ev,
                risk: None,
                reasons: r.reasons.iter().map(|reason| reason.describe()).collect(),
            })
            .collect(),
        gambles: advice
            .gambles
            .iter()
            .map(|g| RankedView {
                headline: g.option.headline(),
                doors_label: g.option.doors_label(),
                doors: edge_labels(g.option.doors.iter().copied()),
                architect_index: g.option.architect.as_ref().map(|a| a.offer_index),
                ev: g.ev,
                risk: Some(g.risk),
                reasons: g.reasons.iter().map(|reason| reason.describe()).collect(),
            })
            .collect(),
        map_action: match advice.map_action {
            MapAction::Continue => "continue".to_string(),
            MapAction::LeaveMap => "leaveMap".to_string(),
        },
        warnings: advice.warnings.iter().map(|w| w.describe()).collect(),
        // Two conditions, and the second is not redundant.
        //
        // A partial read with `read > 0` is not on its own enough: the one
        // block that WAS read can still fail to resolve, and the ranking then
        // falls back to the architect-free `kill either`. Marking THAT forced
        // renders "kill either (only architect read)", which points at an
        // architect the advice is not recommending. So the flag also asks
        // whether the top recommendation actually carries a kill.
        //
        // The top one alone decides, because with one resolved offer every
        // ranked option carries the same architect — there is only one to
        // carry — which is what lets a surface mark the whole list from one
        // flag.
        forced_kill: advice
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::PartialArchitects { read, .. } if *read > 0))
            && advice
                .recommendations
                .first()
                .is_some_and(|r| r.option.architect.is_some()),
    }
}

fn mode_label(mode: Mode) -> String {
    match mode {
        Mode::Chase => "chase".to_string(),
        Mode::Scarab => "scarab".to_string(),
    }
}

/// Project one completed read into the slice.
///
/// The status is derived here rather than passed in, because it is a *function
/// of the read*: a board with no current room is `NoCurrentRoom` whoever asked
/// for the read, and a board with one is `Read`. The loop's own transient
/// states (`Idle`, `Waiting`, `Reading`, `PanelNotVisible`, `Error`) never come
/// through this function — they are written directly, with no board to project.
///
/// The settings echo is the read's own snapshot, so a setting changed while
/// this read was running is echoed back at its OLD value for one tick; the
/// setters re-arm, and the next read publishes the new one.
pub fn project(read: &ReadResult<'_>, calibration: Option<AnchorCalibration>) -> TempleSlice {
    TempleSlice {
        status: if read.layout.current.is_none() {
            TempleStatus::NoCurrentRoom
        } else {
            TempleStatus::Read
        },
        layout: Some(layout_view(read)),
        panel: Some(panel_view(read)),
        advice: read.advice.map(advice_view),
        mode: read.advice.map(|a| mode_label(a.mode)),
        keys: read.keys,
        config: read.config.clone(),
        profile: read.profile.clone(),
        unknown_rooms: unknown_rooms(read.rooms),
        last_read_at: Some(read.read_at),
        calibration,
        // Set AND cleared here: every read states its own notice, so a
        // condition that has gone away takes its warning off the page on the
        // next read rather than standing until something else writes the slice.
        read_notice: read.read_notice.clone(),
        last_error: None,
    }
}

/// The rollouts one decision runs. 2000 rather than the advisor suite's 400:
/// this runs once per panel open, not once per assertion, and the extra
/// samples buy a stabler ordering inside the noise margin.
pub const ROLLOUTS: u32 = 2000;

/// The advisor's RNG seed.
///
/// **Fixed on purpose.** The same board must produce the same advice every
/// time it is read, or a re-read triggered by an unrelated pixel change would
/// reshuffle two options separated by less than the sampling noise, and the
/// overlay would flicker between them while the player is deciding.
pub const SEED: u64 = 0x_7065_0171;

/// Rank one read, or refuse to.
///
/// Refuses exactly once: with no current room there is no position, and
/// `advise` would return an empty `Advice` carrying `Warning::NoPosition`.
/// Publishing `None` instead is the contract POE-171 states — the layout is
/// still worth showing, an empty recommendation list is not.
pub fn advise_read(
    layout: &TempleLayout,
    rooms: &[RoomReading],
    panel: &PanelReading,
    settled: Option<&BTreeSet<Edge>>,
    settings: &TempleSettings,
) -> Option<Advice> {
    layout.current?;
    let board = advisor::state::BoardState::from_reading(layout, &identities(rooms), panel, settled);
    Some(advisor::advise(
        &board,
        &panel.architects,
        settings.keys,
        &settings.profile.to_profile(),
        &settings.config,
        ROLLOUTS,
        SEED,
    ))
}

/// Drop the move the module is recommending, keeping the board it was read
/// from (POE-248).
///
/// The room widget lives with the INCURSION and the kill callout with the
/// PANEL, and this is the incursion's end. Its callers are
/// [`super::trigger::advice_end`]'s two Client.txt lines — the player left the
/// zone, or Alva spoke again after the read — and NOT the capture standing
/// down, which POE-244 used and POE-248 removed: `TempleStatus::PanelNotVisible`
/// is what the whole incursion looks like from the loop's side, and the door
/// diamond is the only surface on screen through it.
///
/// The LAYOUT stays. The Temple page keeps drawing the last board it was given
/// under a badge that says when it was read, which is the same thing it does
/// while the module is looking for the next one; what goes is the ranked move,
/// because a move is a thing the player can still act on and the board is only
/// a record. `mode` goes with it — it is read off the advice and would be a
/// label with nothing under it.
pub fn clear_advice(slice: &mut TempleSlice) {
    slice.advice = None;
    slice.mode = None;
}

/// Force a disabled module's slice to [`TempleStatus::Off`] and drop what it
/// was advising.
///
/// The SSOT composer owns this one precedence step, the loop owns the rest —
/// the same split `ssot::compose_snapshot` uses for the merc slice, and the
/// reason the page never has to read `ssot.modules` to know whether the toggle
/// is on (ADR-014: the page reads slices, not module state).
///
/// The advice goes with the status, unlike the merc slice's capture: a stale
/// recommendation under an "off" badge is a move the player could still act on,
/// whereas a retired capture is only a record of what was on screen.
///
/// `last_error` goes with it for the same reason: the message describes a read
/// the disabled module is no longer attempting, and a red line under an "off"
/// badge reads as "this module is broken" rather than "you switched it off".
/// [`TempleSlice::read_notice`] goes with them on the same argument — it is a
/// warning about an attempt nobody is making any more.
///
/// What does NOT go is the settings echo (`keys`, `config`, `profile`). Those
/// are not something the module read — they are what the user set, and the page
/// renders its own controls from them while the module is off (ADR-014: the
/// page reads the slice, never `ssot.modules`).
pub fn force_off(slice: &mut TempleSlice) {
    slice.status = TempleStatus::Off;
    slice.advice = None;
    slice.mode = None;
    slice.read_notice = None;
    slice.last_error = None;
}

// ------------------------------------------------------ change detection --

/// The cheap per-tick fingerprint of a board: everything
/// [`super::reader::read_layout`] produces WITHOUT OCR.
///
/// Pixels only, so computing it costs one anchor match and one beam-sampling
/// pass — no per-plate crops, no OCR engine. It moves when the player moves
/// (`current`), when a corridor opens (`doors`), and when the panel itself
/// moves or rescales (`origin`, `scale`).
///
/// What it deliberately does NOT see is the plate TEXT: a kill that upgrades
/// the room the player is standing in changes a name and a numeral and nothing
/// else. [`panel_signature`] is the second gate that catches those, and
/// [`ReadGate::on_panel_lost`] is the third — closing the panel always re-arms.
pub fn layout_signature(layout: &TempleLayout) -> u64 {
    let mut hasher = DefaultHasher::new();
    layout.origin.hash(&mut hasher);
    // `scale` is an f32 and has no `Hash`; the bit pattern is the right key
    // here anyway — two scales that differ at all are two different reads.
    layout.scale.to_bits().hash(&mut hasher);
    layout.current.map(|s| s.index()).hash(&mut hasher);
    for edge in &layout.doors {
        edge.to_string().hash(&mut hasher);
    }
    for edge in &layout.uncertain {
        edge.to_string().hash(&mut hasher);
    }
    hasher.finish()
}

/// The fingerprint of the side panel's TEXT — the two bounded OCR crops
/// [`super::run::panel_text`] takes, no per-plate crops.
///
/// Hashes what [`panel::read_panel`] PARSED, not the lines it parsed from. The
/// distinction is the whole value of this gate: the crops still catch stray
/// text — a tooltip, a floating combat number, a chat line drawn over the
/// panel's corner — and a raw-line hash moves for every one of them, which
/// buys a full 26-crop re-read every [`super::run::PANEL_RECHECK_INTERVAL`]
/// for the whole time the noise is on screen. The parsed fields move only when
/// the board did.
///
/// The rule for what belongs here is **exactly the panel fields
/// [`panel_view`] publishes** — title identity, each offer's architect, kind
/// and printed target, and the budget. A published field left out of the hash
/// is a change the gate can never see, so the slice would keep showing the
/// pre-kill panel until something else re-armed the read.
///
/// **One carve-out, and it is deliberate: the rects** (`room_rect`,
/// `ArchitectOffer::rect`, POE-243). They are published and they are NOT
/// hashed. A glyph bounding box moves by a pixel between two reads of a panel
/// that has not changed — anti-aliasing on a frame the game redrew, a
/// sub-pixel anchor difference — and hashing it would spend a 27-OCR-call full
/// read on that. What the rects describe is where text was drawn, which is a
/// property of the same panel this hash is asking about; when the panel really
/// changes, one of the fields above moves with it and the rects come along on
/// the re-read.
pub fn panel_signature(panel: &PanelReading) -> u64 {
    let mut hasher = DefaultHasher::new();
    // The identity, not the OCR text: `panel_view` publishes the identity, so
    // two spellings of the same room are one published value and re-reading
    // because OCR wobbled a glyph is churn. An unread title hashes as `None`,
    // so "unread → read" still re-arms.
    panel.room.identity().map(|id| id.display_name()).hash(&mut hasher);
    for offer in &panel.architects {
        offer.architect_name.trim().hash(&mut hasher);
        // `OfferKind` is not `Hash`; the discriminant is what matters.
        matches!(offer.kind, OfferKind::Upgrade).hash(&mut hasher);
        // The printed name, not the resolved one: `OfferView` publishes it
        // verbatim because it is what the player reads off the panel, and two
        // targets that both fail to resolve are still two different offers.
        offer.printed_target.trim().hash(&mut hasher);
    }
    panel.incursions_remaining.hash(&mut hasher);
    hasher.finish()
}

/// "Read once per panel open, not per frame."
///
/// Three ways a re-read is armed, in the order the loop reaches them:
///
/// 1. the layout fingerprint moved — the player moved, a door opened, the
///    panel moved;
/// 2. the panel's text moved — a kill changed a plate without changing a
///    corridor;
/// 3. the panel was lost and found again, or the user pressed re-arm.
///
/// A pure state machine over `u64`s so all three are testable without a
/// screen. It holds no `Instant`: the loop owns the cadence, this owns only
/// the question "is what I am looking at the thing I already read?".
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReadGate {
    read: Option<(u64, u64)>,
    rearm_seen: u64,
}

impl ReadGate {
    /// Whether the user's re-arm counter has moved since the last read, WITHOUT
    /// spending the bump.
    ///
    /// The detect tick reaches this before anything has been read, and it only
    /// *asks*: a re-arm while nothing is anchored has to force the read, or
    /// pressing the button with the panel closed would do nothing. Spending it
    /// is [`Self::note_rearm`]'s job and belongs on the tick that promoted for
    /// it.
    pub fn rearm_pending(&self, rearm: u64) -> bool {
        self.rearm_seen != rearm
    }

    /// Spend a re-arm bump.
    ///
    /// Drops the recorded read as well as recording the counter, because a
    /// bump means "read this board again even though it looks identical" —
    /// recording the counter alone would let the read it forced match its own
    /// fingerprint and skip.
    ///
    /// **The detect tick must call this on the tick it promoted for a bump.**
    /// [`Self::layout_wants_read`] is reached only after a read that SUCCEEDED,
    /// so a re-arm pressed while no panel is on screen would otherwise stay
    /// pending forever and pin the loop into the full read on every tick — the
    /// exact cost [`super::run`]'s detect tick exists to remove, re-entered
    /// through the settings commands, which re-arm on every change.
    pub fn note_rearm(&mut self, rearm: u64) {
        self.rearm_seen = rearm;
        self.read = None;
    }

    /// Whether the layout fingerprint alone already justifies a full read.
    ///
    /// `rearm` is the user's re-arm counter; a bump forces one read and is then
    /// spent, so holding the button down does not pin the loop into re-reading
    /// forever. A bump the detect tick already spent is no longer pending here,
    /// which is why this and that tick share one writer.
    pub fn layout_wants_read(&mut self, layout: u64, rearm: u64) -> bool {
        if self.rearm_pending(rearm) {
            self.note_rearm(rearm);
            return true;
        }
        !matches!(self.read, Some((seen, _)) if seen == layout)
    }

    /// Whether the panel's text has moved since the last completed read.
    ///
    /// Only consulted when [`Self::layout_wants_read`] said no — it costs the
    /// two bounded OCR crops [`super::run::panel_text`] takes, which the cheap
    /// tick exists to avoid.
    pub fn panel_wants_read(&self, panel: u64) -> bool {
        !matches!(self.read, Some((_, seen)) if seen == panel)
    }

    /// Remember what a completed read saw.
    pub fn record(&mut self, layout: u64, panel: u64) {
        self.read = Some((layout, panel));
    }

    /// The panel left the screen. The next one that appears is a new decision
    /// even if it looks identical — this is what makes "close the panel, kill
    /// the architect, reopen" re-read rather than show the pre-kill board.
    pub fn on_panel_lost(&mut self) {
        self.read = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::advisor::rules::ArchitectChoice;
    use crate::temple::doors::Thresholds;
    use crate::temple::lattice::{Lattice, Slot};
    use crate::temple::rooms::{match_room_name, resolve_name};

    fn calibration() -> AnchorCalibration {
        AnchorCalibration {
            screen_w: 1374,
            screen_h: 773,
            scale: 0.99,
        }
    }

    /// The origin and scale [`layout`] builds its board at — a plausible
    /// anchored Entrance on a 1374-wide capture. Named so a test can rebuild
    /// the same lattice without copying two numbers out of the helper.
    const FIXTURE_ORIGIN: (i32, i32) = (673, 494);
    const FIXTURE_SCALE: f32 = 0.99;

    /// A layout with the given current room and door set; every other field is
    /// a plausible constant, because nothing below reads them.
    fn layout(current: Option<Slot>, doors: &[(Slot, Slot)], uncertain: &[(Slot, Slot)]) -> TempleLayout {
        // `slots` is the lattice the anchor fixes — `reader::read_layout_at`
        // fills it from exactly this expression. A filler `[(0, 0); 13]` was
        // harmless while nothing read the field; it is not now that the
        // projection publishes the centres (POE-227).
        let lattice = Lattice::new(FIXTURE_ORIGIN, FIXTURE_SCALE);
        TempleLayout {
            origin: lattice.origin,
            scale: lattice.scale,
            ncc: 0.94,
            confidence: Confidence::High,
            current,
            doors: doors.iter().map(|(a, b)| Edge::new(*a, *b)).collect(),
            uncertain: uncertain.iter().map(|(a, b)| Edge::new(*a, *b)).collect(),
            slots: lattice.centres,
            thresholds: Thresholds { horizontal: 0.20, diagonal: 0.20 },
            calibration: calibration(),
        }
    }

    /// 13 plates: `named` maps a slot to the room name printed on it,
    /// everything else reads Unknown.
    fn board_rooms(named: &[(Slot, &str)]) -> Vec<RoomReading> {
        Slot::ALL
            .into_iter()
            .map(|slot| RoomReading {
                slot,
                identity: match named.iter().find(|(s, _)| *s == slot) {
                    Some((_, name)) => match_room_name(name),
                    None => Match::Unknown,
                },
            })
            .collect()
    }

    fn panel(room: &str, remaining: Option<u8>, offers: Vec<ArchitectOffer>) -> PanelReading {
        PanelReading {
            room: match_room_name(room),
            room_rect: None,
            architects: offers,
            incursions_remaining: remaining,
        }
    }

    fn offer(name: &str, target: &str, kind: OfferKind) -> ArchitectOffer {
        ArchitectOffer {
            architect_name: name.to_string(),
            kind,
            printed_target: target.to_string(),
            target: match_room_name(target),
            rect: None,
        }
    }

    fn read<'a>(
        layout: &'a TempleLayout,
        rooms: &'a [RoomReading],
        panel: &'a PanelReading,
        settled: Option<&'a BTreeSet<Edge>>,
        advice: Option<&'a Advice>,
    ) -> ReadResult<'a> {
        ReadResult {
            layout,
            rooms,
            panel,
            settled,
            marker_error: None,
            read_notice: None,
            advice,
            keys: 1,
            config: TempleConfig::default(),
            profile: TempleProfileSettings::default(),
            read_at: 1_700_000_000_000,
        }
    }

    // ------------------------------------------------------- projection --

    /// The headline projection: a plate that read a real room reaches the
    /// slice with the game's own name and its tier, and the current room is
    /// flagged. Fails if `slot_view` drops the identity, the tier, or the
    /// current-room comparison.
    #[test]
    fn a_read_plate_projects_its_name_tier_and_current_flag() {
        let layout = layout(Some(Slot::D3), &[], &[]);
        let rooms = board_rooms(&[(Slot::D3, "Tombs"), (Slot::C1, "Locus of Corruption")]);
        let panel = panel("Tombs", Some(6), Vec::new());
        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        let view = slice.layout.expect("a read publishes its layout");
        let d3 = view
            .slots
            .iter()
            .find(|s| s.slot == "D3")
            .expect("D3 is one of the 13 slots");
        assert_eq!(d3.name.as_deref(), Some("Tombs"));
        assert!(d3.current, "D3 is the current room");

        let c1 = view
            .slots
            .iter()
            .find(|s| s.slot == "C1")
            .expect("C1 is one of the 13 slots");
        assert_eq!(c1.name.as_deref(), Some("Locus of Corruption"));
        assert_eq!(c1.tier, 3, "Locus of Corruption is the tier-3 corruption room");
        assert!(!c1.current, "only the layout's own current room is current");
    }

    /// The pixel geometry a game-anchored surface places itself against
    /// (POE-227): every published centre is the lattice's own centre for the
    /// slot at the SAME index, in capture px.
    ///
    /// Fails if the projection drops the field, publishes a filler, transposes
    /// x and y, or walks `slots` in an order that is not `Slot::ALL` — any of
    /// which would put a door arrow on the wrong plate with nothing on screen
    /// to say so.
    #[test]
    fn the_published_centres_are_the_lattice_the_board_was_read_off() {
        let layout = layout(Some(Slot::D3), &[], &[]);
        let rooms = board_rooms(&[]);
        let panel = panel("Tombs", Some(6), Vec::new());

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        let view = slice.layout.expect("a read publishes its layout");
        let lattice = Lattice::new(FIXTURE_ORIGIN, FIXTURE_SCALE);
        for (i, slot) in Slot::ALL.into_iter().enumerate() {
            let (x, y) = lattice.centre(slot);
            assert_eq!(
                view.centres[i],
                [x, y],
                "{slot:?} is published at the wrong pixel centre",
            );
        }
    }

    /// The origin is published, and it is the Entrance plate's centre — the one
    /// relation that holds independently of the lattice arithmetic, because the
    /// Entrance's offset IS `(0, 0)`.
    ///
    /// Fails if `origin` carries the anchor's top-left, the panel rect, or the
    /// board's first slot instead — every one of which would look plausible in
    /// the JSON and shift a whole overlay.
    #[test]
    fn the_published_origin_is_the_entrance_plate_centre() {
        let layout = layout(Some(Slot::D3), &[], &[]);
        let rooms = board_rooms(&[]);
        let panel = panel("Tombs", Some(6), Vec::new());

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        let view = slice.layout.expect("a read publishes its layout");
        assert_eq!(view.origin, [FIXTURE_ORIGIN.0, FIXTURE_ORIGIN.1]);
        assert_eq!(
            view.centres[Slot::ENTRANCE.index()],
            view.origin,
            "the Entrance sits at the origin by construction",
        );
    }

    /// Unknown plates are SURFACED, not hidden: they carry `known: false` and
    /// they are listed. Fails if `unknown_rooms` filters on the wrong side of
    /// `is_known`, or if an unread plate is silently dropped from `slots`.
    #[test]
    fn unread_plates_are_listed_and_marked_unknown() {
        let layout = layout(Some(Slot::D3), &[], &[]);
        let rooms = board_rooms(&[(Slot::D3, "Tombs")]);
        let panel = panel("Tombs", Some(6), Vec::new());

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        assert_eq!(
            slice.unknown_rooms.len(),
            12,
            "12 of the 13 plates did not resolve, got {:?}",
            slice.unknown_rooms,
        );
        assert!(
            !slice.unknown_rooms.contains(&"D3".to_string()),
            "the plate that DID resolve must not be listed as unknown",
        );
        let view = slice.layout.expect("a read publishes its layout");
        assert_eq!(view.slots.len(), 13, "every plate is published, read or not");
        let unknown = view
            .slots
            .iter()
            .find(|s| s.slot == "A0")
            .expect("A0 is one of the 13 slots");
        assert!(!unknown.known);
        assert_eq!(unknown.name, None, "an unread plate is never given a name");
    }

    /// The no-current-room contract: layout published, advice `None`, status
    /// says why. Fails if `project` ranks anyway or if the status collapses to
    /// `Read`.
    #[test]
    fn a_board_with_no_current_room_publishes_the_layout_without_advice() {
        let layout = layout(None, &[(Slot::B0, Slot::C1)], &[]);
        let rooms = board_rooms(&[(Slot::C1, "Locus of Corruption")]);
        let panel = panel("Tombs", Some(6), Vec::new());

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        assert_eq!(slice.status, TempleStatus::NoCurrentRoom);
        assert!(slice.advice.is_none(), "no position, no ranking");
        assert!(slice.mode.is_none(), "the mode comes from the advice");
        let view = slice.layout.expect("the layout is still worth showing");
        assert_eq!(view.doors, vec!["B0-C1".to_string()]);
        assert_eq!(view.current, None);
    }

    /// `advise_read` is the seam that refuses. Fails if it calls `advise` and
    /// returns its empty `NoPosition` advice instead of `None`.
    #[test]
    fn advise_read_refuses_to_rank_between_rooms() {
        let layout = layout(None, &[], &[]);
        let rooms = board_rooms(&[]);
        let panel = panel("Tombs", Some(6), Vec::new());

        assert!(advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped()).is_none());
    }

    /// Unknown plates do not stop the advisor: with a current room it still
    /// ranks, and the unknown rooms are still surfaced alongside. Fails if the
    /// projection gates the advice on a clean board read.
    #[test]
    fn unknown_plates_still_produce_advice() {
        let layout = layout(Some(Slot::B0), &[(Slot::B0, Slot::C1)], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel(
            "Chasm",
            Some(6),
            vec![offer("Guatelitzi", "Corruption Chamber", OfferKind::Change)],
        );
        let settings = TempleSettings::shipped();

        let advice = advise_read(&layout, &rooms, &panel, None, &settings)
            .expect("a board with a current room ranks");
        let slice = project(
            &read(&layout, &rooms, &panel, None, Some(&advice)),
            None,
        );

        assert_eq!(slice.status, TempleStatus::Read);
        let published = slice.advice.expect("the advice reaches the slice");
        assert!(
            !published.recommendations.is_empty(),
            "an offer plus a corridor is at least one ranked option",
        );
        assert!(
            !slice.unknown_rooms.is_empty(),
            "the unread plates are still surfaced next to the advice",
        );
    }

    /// Every recommendation carries its reasons as human strings — the audit
    /// trail is the point. Fails if `advice_view` drops `reasons` or publishes
    /// the enum's `Debug` instead of `describe()`.
    #[test]
    fn recommendations_carry_their_reasons_as_prose() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel(
            "Chasm",
            Some(6),
            vec![offer("Guatelitzi", "Corruption Chamber", OfferKind::Change)],
        );
        let advice =
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped()).expect("ranks");

        let view = advice_view(&advice);

        let top = view
            .recommendations
            .first()
            .expect("at least one recommendation");
        assert!(
            top.headline.starts_with("change → "),
            "the headline names the resolved room, got {:?}",
            top.headline,
        );
        assert!(
            top.reasons.iter().all(|r| !r.is_empty()),
            "every reason is a rendered line, got {:?}",
            top.reasons,
        );
    }

    /// The warning the advisor raises for an illegible budget reaches the page
    /// as text. Fails if `advice_view` drops `warnings`.
    #[test]
    fn an_illegible_incursion_count_reaches_the_slice_as_a_warning() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel("Chasm", None, Vec::new());
        let advice =
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped()).expect("ranks");

        assert!(
            advice.warnings.contains(&Warning::NoBudget),
            "precondition: an unread budget warns",
        );
        let view = advice_view(&advice);
        assert!(
            view.warnings.iter().any(|w| w.contains("incursions remaining")),
            "the warning must reach the page as prose, got {:?}",
            view.warnings,
        );
    }

    /// The block rects the reader measured reach the slice unchanged, in
    /// capture px, so a surface can point at the block without re-deriving
    /// where the panel drew it (POE-243; POE-244 is the consumer).
    ///
    /// Fails if `offer_view` or `panel_view` drops the rect, or invents one
    /// from the layout geometry — which would be a second answer to a question
    /// the OCR already answered.
    #[test]
    fn the_panel_publishes_the_screen_rects_its_lines_were_read_at() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let mut panel = panel(
            "Chasm",
            Some(6),
            vec![offer("Quipolatl", "Armoury", OfferKind::Upgrade)],
        );
        panel.room_rect = Some([1300, 100, 152, 20]);
        panel.architects[0].rect = Some([1300, 140, 280, 43]);

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        let view = slice.panel.expect("a read publishes its panel");
        assert_eq!(view.room_rect, Some([1300, 100, 152, 20]));
        assert_eq!(
            view.offers.first().expect("one block").rect,
            Some([1300, 140, 280, 43]),
        );
    }

    /// A read with no boxes publishes no rects rather than zeroes. Fails if
    /// the projection defaults them, which a surface would draw at the screen
    /// origin — over the game's own UI and nowhere near the block.
    #[test]
    fn a_read_with_no_boxes_publishes_no_rects() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel(
            "Chasm",
            Some(6),
            vec![offer("Quipolatl", "Armoury", OfferKind::Upgrade)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        let view = slice.panel.expect("a read publishes its panel");
        assert_eq!(view.room_rect, None);
        assert_eq!(view.offers.first().expect("one block").rect, None);
    }

    /// A one-of-two read reaches the surfaces BOTH ways: as prose in
    /// `warnings`, which the overlay prints, and as `forced_kill`, which it
    /// branches on to mark the headline. A surface that had to recognise the
    /// warning by its text would break the first time the wording changed.
    ///
    /// Fails if `advice_view` drops either half.
    #[test]
    fn a_one_of_two_architect_read_reaches_the_slice_as_prose_and_as_a_flag() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel(
            "Chasm",
            Some(6),
            vec![offer("Quipolatl", "Armoury", OfferKind::Upgrade)],
        );
        let advice =
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped()).expect("ranks");
        assert!(
            advice.warnings.contains(&Warning::PartialArchitects { read: 1, expected: 2 }),
            "precondition: one block of two warns, got {:?}",
            advice.warnings,
        );

        let view = advice_view(&advice);

        assert!(view.forced_kill, "the kill on screen is the only kill there was");
        assert!(
            view.warnings.iter().any(|w| w.contains("1 of 2 architects read")),
            "the same fact must be printable, got {:?}",
            view.warnings,
        );
    }

    /// One block read, and it did not resolve: the ranking falls back to the
    /// architect-free `kill either`, so there is no kill on the headline for
    /// the mark to qualify. The partial read is still REPORTED — both warnings
    /// are there — it is only the headline mark that is withheld.
    ///
    /// Fails if `forced_kill` keys on the partial-read warning alone: the
    /// overlay then renders "kill either (only architect read)", which points
    /// at an architect the advice is not recommending.
    #[test]
    fn a_single_unresolvable_block_is_reported_but_leaves_the_headline_unmarked() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel(
            "Chasm",
            Some(6),
            vec![offer("Nobody", "Definitely Not A Room", OfferKind::Change)],
        );
        let advice =
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped()).expect("ranks");
        assert!(
            advice
                .recommendations
                .first()
                .is_some_and(|r| r.option.architect.is_none()),
            "precondition: nothing resolved, so the top move carries no kill",
        );

        let view = advice_view(&advice);

        assert!(!view.forced_kill, "there is no kill on the headline to mark");
        assert!(
            view.warnings.iter().any(|w| w.contains("1 of 2 architects read")),
            "the partial read is still reported, got {:?}",
            view.warnings,
        );
        assert!(
            view.warnings.iter().any(|w| w.contains("Definitely Not A Room")),
            "and so is the reason the one block was no use, got {:?}",
            view.warnings,
        );
    }

    /// Both blocks read: no flag, so the headline is presented as the choice it
    /// is. Fails if `forced_kill` is hard-coded true, or keyed on anything
    /// other than the partial-read warning.
    #[test]
    fn a_two_of_two_architect_read_is_not_marked_forced() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel(
            "Chasm",
            Some(6),
            vec![
                offer("Quipolatl", "Armoury", OfferKind::Upgrade),
                offer("Atmohua", "Shrine of Empowerment", OfferKind::Change),
            ],
        );
        let advice =
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped()).expect("ranks");

        assert!(!advice_view(&advice).forced_kill);
    }

    /// Nothing read at all is NOT a forced kill: there is no kill on screen to
    /// qualify, and the warning says so in its own words. Fails if
    /// `forced_kill` keys on "fewer than two" rather than on "one or more".
    #[test]
    fn a_read_with_no_architect_block_is_warned_about_but_not_marked_forced() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel("Chasm", Some(6), Vec::new());
        let advice =
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped()).expect("ranks");

        let view = advice_view(&advice);

        assert!(!view.forced_kill);
        assert!(
            view.warnings.iter().any(|w| w.contains("no architect block was read")),
            "the read is still reported as partial, got {:?}",
            view.warnings,
        );
    }

    /// The panel's printed target is NOT the answer — the slice publishes the
    /// room the kill actually builds. On a tier-2 room, "change to Corruption
    /// Chamber" builds the tier-3 room of that line. Fails if `offer_view`
    /// echoes `printed_target` into `display_name`.
    #[test]
    fn an_offer_resolves_to_the_room_the_kill_actually_builds() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        // B0 holds a tier-2 room, so Contested Development's +1 lands on 3.
        let rooms = board_rooms(&[(Slot::B0, "Catalyst of Corruption")]);
        let panel = panel(
            "Catalyst of Corruption",
            Some(6),
            vec![offer("Guatelitzi", "Corruption Chamber", OfferKind::Change)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        let view = slice.panel.expect("a read publishes its panel");
        let first = view.offers.first().expect("one architect block");
        assert_eq!(first.printed_target, "Corruption Chamber");
        assert_eq!(
            first.display_name.as_deref(),
            Some("Locus of Corruption"),
            "tier 2 + 1 is the tier-3 corruption room, not the printed tier-1 name",
        );
        assert_eq!(first.built_tier, Some(3));
        assert_eq!(first.kind, "change");
    }

    /// An architect target outside the vocabulary leaves `display_name` empty
    /// rather than echoing the OCR text as if it resolved. Fails if
    /// `offer_view` falls back to `printed_target`.
    #[test]
    fn an_unresolvable_offer_publishes_no_display_name() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel(
            "Chasm",
            Some(6),
            vec![offer("Guatelitzi", "Qwertz Chamber", OfferKind::Upgrade)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        let first = slice
            .panel
            .expect("panel")
            .offers
            .into_iter()
            .next()
            .expect("one architect block");
        assert_eq!(first.display_name, None);
        assert_eq!(first.built_tier, None);
        assert_eq!(
            first.printed_target, "Qwertz Chamber",
            "what the panel printed is still shown — it is what the player sees",
        );
    }

    // ------------------------------------------- POE-229: the current room --

    /// The 2026-09-02 screenshot's corridors: the Entrance (E1) reaches C2 by
    /// way of D2, and C2's neighbour C1 holds the finished corruption room.
    const SCREENSHOT_DOORS: [(Slot, Slot); 3] = [
        (Slot::E1, Slot::D2),
        (Slot::D2, Slot::C2),
        (Slot::C2, Slot::C1),
    ];

    /// The 2026-09-02 screenshot board: standing in **Office of Cartography**
    /// (tier 2) at C2 with that plate UNREAD — the gold selection frame
    /// overhangs the current plate, which is why it is the one that fails to
    /// read — and **Locus of Corruption** (tier 3) already built next door at
    /// C1. Two incursions left.
    ///
    /// `title` is what the side panel printed for the room the player is in, so
    /// a test can also take it away.
    fn screenshot_board(
        title: &str,
        offers: Vec<ArchitectOffer>,
    ) -> (TempleLayout, Vec<RoomReading>, PanelReading) {
        (
            layout(Some(Slot::C2), &SCREENSHOT_DOORS, &[]),
            board_rooms(&[(Slot::C1, "Locus of Corruption")]),
            panel(title, Some(2), offers),
        )
    }

    /// The architect choice the advisor made for one offer block, if it made
    /// one at all.
    fn ranked_choice(advice: &Advice, offer_index: usize) -> Option<&ArchitectChoice> {
        advice
            .recommendations
            .iter()
            .map(|r| &r.option)
            .chain(advice.gambles.iter().map(|g| &g.option))
            .filter_map(|option| option.architect.as_ref())
            .find(|a| a.offer_index == offer_index)
    }

    /// POE-229. The current plate is the one the selection frame covers, so it
    /// is the one most likely to read Unknown — and the side panel's title is
    /// then the only source left for the room the player is in.
    ///
    /// A `change` is the discriminating kind: its built tier is
    /// `currentTier + 1` of a DIFFERENT line, so nothing in the printed text
    /// carries it. From Office of Cartography II, *"change to Gemcutter's
    /// Workshop"* builds the gem line's tier 3.
    ///
    /// Fails if the title is not consulted when the plate is unread, or if the
    /// tier falls back to `T0` — the shipped bug, which printed and ranked the
    /// tier-1 room.
    #[test]
    fn a_change_offer_on_an_unread_current_plate_advances_from_the_title_tier() {
        let (layout, rooms, panel) = screenshot_board(
            "Office of Cartography",
            vec![offer("Uromoti", "Gemcutter's Workshop", OfferKind::Change)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);
        let published = slice
            .panel
            .expect("panel")
            .offers
            .into_iter()
            .next()
            .expect("one architect block");
        assert_eq!(
            published.display_name.as_deref(),
            Some("Doryani's Institute"),
            "the gem line at tier 2 + 1, not the printed Gemcutter's Workshop",
        );
        assert_eq!(published.built_tier, Some(3));

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped())
            .expect("a board with a current room ranks");
        let choice = ranked_choice(&advice, 0).expect("the change offer is ranked");
        assert_eq!(choice.built_tier, Tier::T3);
        assert_eq!(choice.display_name, "Doryani's Institute");
    }

    /// Neither source named the room: the plate is unread AND the panel title
    /// did not resolve. A `change` offer's built tier is a fact about the room
    /// the player is standing in, so there is no answer — and the advisor says
    /// so instead of inventing tier 1.
    ///
    /// Fails if the unknown tier collapses back to `T0`, which would publish a
    /// room name the kill will not build with no warning attached.
    #[test]
    fn a_change_offer_with_no_readable_current_room_warns_instead_of_guessing() {
        let (layout, rooms, panel) = screenshot_board(
            "Qwertz Chamber",
            vec![offer("Uromoti", "Gemcutter's Workshop", OfferKind::Change)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);
        let published = slice
            .panel
            .expect("panel")
            .offers
            .into_iter()
            .next()
            .expect("one architect block");
        assert_eq!(published.display_name, None);
        assert_eq!(published.built_tier, None);

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped())
            .expect("a board with a current room still ranks its doors");
        assert!(
            advice.warnings.contains(&Warning::UnknownCurrentTier),
            "the unknown tier must be stated, got {:?}",
            advice.warnings,
        );
        assert!(
            ranked_choice(&advice, 0).is_none(),
            "an unresolvable change offer must not reach the ranking as a choice",
        );
        assert!(
            !advice.warnings.contains(&Warning::UnresolvedArchitects),
            "the architect's target read fine — it was the current room that did not: {:?}",
            advice.warnings,
        );
    }

    /// The same unreadable current room, but an `upgrade`: that kind prints
    /// tier `current + 1` of the room's OWN line, so the printed name IS the
    /// built room and the current tier is not needed at all.
    ///
    /// Fails if the upgrade is refused alongside the change — the whole point
    /// of splitting the two kinds is that only one of them needs the tier.
    #[test]
    fn an_upgrade_offer_resolves_from_its_own_printed_name_with_no_current_room() {
        let (layout, rooms, panel) = screenshot_board(
            "Qwertz Chamber",
            vec![offer("Zalatl", "Atlas of Worlds", OfferKind::Upgrade)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);
        let published = slice
            .panel
            .expect("panel")
            .offers
            .into_iter()
            .next()
            .expect("one architect block");
        assert_eq!(published.display_name.as_deref(), Some("Atlas of Worlds"));
        assert_eq!(published.built_tier, Some(3));

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped())
            .expect("ranks");
        assert!(
            !advice.warnings.contains(&Warning::UnknownCurrentTier),
            "an upgrade needs no current tier, so nothing is unknown: {:?}",
            advice.warnings,
        );
        let choice = ranked_choice(&advice, 0).expect("the upgrade offer is ranked");
        assert_eq!(choice.built_tier, Tier::T3);
    }

    /// The same `upgrade`, but with the title RESOLVABLE: standing in Office of
    /// Cartography II with C2 unread, *"upgrade to Atlas of Worlds"*.
    ///
    /// This is the other side of `resolve_offer_for`. The case above takes the
    /// `current_tier == None` branch, where an upgrade answers from its own
    /// printed name; here the title supplies tier 2, so the function DELEGATES
    /// to `resolve_offer` and the answer comes from `min(3, current + 1)` of the
    /// printed room's line instead. Both branches must land on Atlas of Worlds
    /// III — an upgrade prints tier `current + 1` of the room's own line, so the
    /// two roads lead to the same room by construction, and an implementation
    /// where they disagree is one that got the arithmetic or the line wrong.
    ///
    /// Fails if the delegation branch refuses an upgrade for want of a tier it
    /// has, or advances the wrong line.
    #[test]
    fn an_upgrade_offer_with_a_readable_title_resolves_through_the_current_tier() {
        let (layout, rooms, panel) = screenshot_board(
            "Office of Cartography",
            vec![offer("Zalatl", "Atlas of Worlds", OfferKind::Upgrade)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);
        let published = slice
            .panel
            .expect("panel")
            .offers
            .into_iter()
            .next()
            .expect("one architect block");
        assert_eq!(published.display_name.as_deref(), Some("Atlas of Worlds"));
        assert_eq!(published.built_tier, Some(3));

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped())
            .expect("ranks");
        assert!(
            !advice.warnings.contains(&Warning::UnknownCurrentTier),
            "the title named the room, so the tier is known: {:?}",
            advice.warnings,
        );
        let choice = ranked_choice(&advice, 0).expect("the upgrade offer is ranked");
        assert_eq!(choice.built_tier, Tier::T3);
        assert_eq!(choice.display_name, "Atlas of Worlds");
    }

    /// The plate read and the title did not: the plate is the FIRST source, and
    /// this is the case that proves it is consulted at all. A `change` is the
    /// discriminating kind — it resolves only if some source supplied the tier.
    ///
    /// Fails if the title becomes the only source, which is what the ordering in
    /// `current_identity` would collapse to if `plate.or(title)` lost its first
    /// operand.
    #[test]
    fn a_read_current_plate_supplies_the_tier_when_the_title_does_not() {
        let layout = layout(Some(Slot::C2), &SCREENSHOT_DOORS, &[]);
        let rooms = board_rooms(&[
            (Slot::C1, "Locus of Corruption"),
            (Slot::C2, "Office of Cartography"),
        ]);
        let panel = panel(
            "Qwertz Chamber",
            Some(2),
            vec![offer("Uromoti", "Gemcutter's Workshop", OfferKind::Change)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);
        let published = slice
            .panel
            .expect("panel")
            .offers
            .into_iter()
            .next()
            .expect("one architect block");
        assert_eq!(published.display_name.as_deref(), Some("Doryani's Institute"));
        assert_eq!(published.built_tier, Some(3));
    }

    /// The board the two sources disagree about: the plate reads Surveyor's
    /// Study (tier 1) and the title reads Office of Cartography (tier 2), so
    /// the same `change` offer resolves to two different rooms.
    fn disagreeing_board() -> (TempleLayout, Vec<RoomReading>, PanelReading) {
        (
            layout(Some(Slot::C2), &SCREENSHOT_DOORS, &[]),
            board_rooms(&[
                (Slot::C1, "Locus of Corruption"),
                (Slot::C2, "Surveyor's Study"),
            ]),
            panel(
                "Office of Cartography",
                Some(2),
                vec![offer("Uromoti", "Gemcutter's Workshop", OfferKind::Change)],
            ),
        )
    }

    /// Both sources read and they disagree: the plate wins. It is cropped from
    /// the slot the lattice pins under the cursor and cross-checked against the
    /// tier numeral on the same plate, while the title is a heuristic pick out
    /// of whole-screen OCR that can land on another plate's name.
    ///
    /// Fails if the title is preferred, or if the sources are merged some third
    /// way: the plate's tier 1 builds Department of Thaumaturgy, the title's
    /// tier 2 builds Doryani's Institute.
    #[test]
    fn a_disagreeing_current_plate_beats_the_side_panel_title() {
        let (layout, rooms, panel) = disagreeing_board();

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);
        let published = slice
            .panel
            .expect("panel")
            .offers
            .into_iter()
            .next()
            .expect("one architect block");
        assert_eq!(
            published.display_name.as_deref(),
            Some("Department of Thaumaturgy"),
            "the plate's tier 1, not the title's tier 2 (which would build Doryani's Institute)",
        );
        assert_eq!(published.built_tier, Some(2));
    }

    /// Picking the plate is not the same as being sure of it — one of the two
    /// OCR passes is wrong and nothing in the reader can tell which, so the
    /// conflict is published rather than resolved out of sight.
    ///
    /// Fails if the disagreement is swallowed, or if the warning does not name
    /// both readings.
    #[test]
    fn a_current_room_the_two_sources_disagree_about_reaches_the_page_as_a_warning() {
        let (layout, rooms, panel) = disagreeing_board();

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped())
            .expect("ranks");

        assert!(
            advice.warnings.contains(&Warning::CurrentRoomDisagreement {
                title: "Office of Cartography",
                plate: "Surveyor's Study",
            }),
            "the conflict must be stated, got {:?}",
            advice.warnings,
        );
        let rendered = advice_view(&advice).warnings;
        assert!(
            rendered
                .iter()
                .any(|w| w.contains("Office of Cartography") && w.contains("Surveyor's Study")),
            "the page gets both readings, got {rendered:?}",
        );
    }

    /// A plate that read a filler is tier 0, and tier 0 is an ANSWER: an offer
    /// taken in a filler really does build tier 1. Only "no source named the
    /// room" is unknown.
    ///
    /// Fails if `current_identity` collapses the line-less kinds to `None`,
    /// which would refuse every `change` offer taken in a filler, the Entrance
    /// or the Apex.
    #[test]
    fn a_current_plate_that_read_a_filler_is_tier_zero_rather_than_unknown() {
        let layout = layout(Some(Slot::C2), &SCREENSHOT_DOORS, &[]);
        let rooms = board_rooms(&[(Slot::C1, "Locus of Corruption"), (Slot::C2, "Chasm")]);
        let panel = panel(
            "Qwertz Chamber",
            Some(2),
            vec![offer("Uromoti", "Gemcutter's Workshop", OfferKind::Change)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);
        let published = slice
            .panel
            .expect("panel")
            .offers
            .into_iter()
            .next()
            .expect("one architect block");
        assert_eq!(
            published.display_name.as_deref(),
            Some("Gemcutter's Workshop"),
            "tier 0 + 1 is the gem line's tier 1",
        );
        assert_eq!(published.built_tier, Some(1));
    }

    /// The unknown current tier is one fact about the read, not one per
    /// architect block: two unresolvable `change` offers still produce a single
    /// line for the overlay.
    ///
    /// Fails if the warning is pushed per offer, which would print the same
    /// sentence twice under a two-architect panel — the common case.
    #[test]
    fn two_change_offers_with_no_readable_current_room_warn_once() {
        let (layout, rooms, panel) = screenshot_board(
            "Qwertz Chamber",
            vec![
                offer("Uromoti", "Gemcutter's Workshop", OfferKind::Change),
                offer("Zalatl", "Surveyor's Study", OfferKind::Change),
            ],
        );

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped())
            .expect("ranks");

        assert_eq!(
            advice
                .warnings
                .iter()
                .filter(|w| **w == Warning::UnknownCurrentTier)
                .count(),
            1,
            "one unknown current room is one warning, got {:?}",
            advice.warnings,
        );
    }

    /// The owner's report, end to end: on the screenshot board the swap that
    /// completes Locus + Doryani must outrank the Atlas upgrade that completes
    /// nothing. It was ranked below it because the change resolved to the
    /// tier-1 Gemcutter's Workshop.
    ///
    /// Fails if the change offer is resolved at the wrong tier, or if the top
    /// recommendation is the upgrade.
    #[test]
    fn the_swap_that_completes_the_target_pair_outranks_the_upgrade_that_completes_nothing() {
        let (layout, rooms, panel) = screenshot_board(
            "Office of Cartography",
            vec![
                offer("Zalatl", "Atlas of Worlds", OfferKind::Upgrade),
                offer("Uromoti", "Gemcutter's Workshop", OfferKind::Change),
            ],
        );

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::shipped())
            .expect("ranks");

        let top = advice
            .recommendations
            .first()
            .expect("at least one recommendation");
        let choice = top
            .option
            .architect
            .as_ref()
            .expect("the top recommendation kills an architect");
        assert_eq!(
            choice.offer_index, 1,
            "the change offer completes the pair; the upgrade is worth nothing here",
        );
        assert_eq!(choice.built_tier, Tier::T3);
        assert_eq!(choice.display_name, "Doryani's Institute");
        assert_eq!(top.option.headline(), "change → Doryani's Institute");
    }

    // --------------------------------------------------- marker fallback --

    /// The diamond read succeeded: the settled set is what the slice
    /// publishes, and nothing is flagged unresolved. The paired failure case
    /// is the next test.
    #[test]
    fn a_settled_door_set_is_published_with_nothing_unresolved() {
        let layout = layout(
            Some(Slot::B0),
            &[(Slot::C1, Slot::C2)],
            &[(Slot::B0, Slot::C1), (Slot::B0, Slot::C0)],
        );
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel("Chasm", Some(6), Vec::new());
        let settled: BTreeSet<Edge> = [
            Edge::new(Slot::C1, Slot::C2),
            Edge::new(Slot::B0, Slot::C1),
        ]
        .into_iter()
        .collect();

        let slice = project(&read(&layout, &rooms, &panel, Some(&settled), None), None);

        let view = slice.layout.expect("layout");
        assert_eq!(
            view.doors,
            vec!["B0-C1".to_string(), "C1-C2".to_string()],
            "the seals settled B0-C1 open, which `doors - uncertain` would have dropped",
        );
        assert!(
            view.unresolved_incident.is_empty(),
            "a settled read leaves nothing unresolved",
        );
        // The shape POE-248's overlay bug turned on, pinned here so a consumer
        // reading it as a verdict has something to fail against: on the settled
        // path a corridor the SEALS read open is in BOTH lists. `uncertain` is
        // the beam's self-doubt about a corridor the frame covered, and the
        // diamond read is precisely what answers it — see the field's note.
        assert!(
            view.uncertain.contains(&"B0-C1".to_string())
                && view.doors.contains(&"B0-C1".to_string()),
            "`uncertain` is a diagnostic and overlaps `doors`: {:?} / {:?}",
            view.uncertain,
            view.doors,
        );
    }

    /// The fallback: with no settled set the door list is `doors − uncertain`
    /// and EVERY uncertain corridor is surfaced as unresolved with the marker
    /// error attached. Fails if the fallback unions `uncertain` into `doors`
    /// (the mistake `apply_markers`' doc names) or drops the unresolved list.
    #[test]
    fn a_failed_diamond_read_falls_back_and_surfaces_the_unresolved_corridors() {
        let layout = layout(
            Some(Slot::B0),
            &[(Slot::C1, Slot::C2), (Slot::B0, Slot::C1)],
            &[(Slot::B0, Slot::C1), (Slot::B0, Slot::C0)],
        );
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel("Chasm", Some(6), Vec::new());
        let mut result = read(&layout, &rooms, &panel, None, None);
        result.marker_error = Some("read 3 door markers for a 4-neighbour room".to_string());

        let slice = project(&result, None);

        let view = slice.layout.expect("layout");
        assert_eq!(
            view.doors,
            vec!["C1-C2".to_string()],
            "B0-C1 is uncertain, so the fallback must NOT act on it",
        );
        assert_eq!(
            view.unresolved_incident,
            vec!["B0-C0".to_string(), "B0-C1".to_string()],
            "both corridors the frame covers are surfaced, not guessed",
        );
        assert_eq!(
            view.marker_error.as_deref(),
            Some("read 3 door markers for a 4-neighbour room"),
        );
    }

    // -------------------------------------------------- change detection --

    /// The gate's whole purpose: an unchanged board does not buy a second
    /// read. Fails if `layout_wants_read` ignores the recorded fingerprint.
    #[test]
    fn an_unchanged_layout_does_not_re_read() {
        let before = layout(Some(Slot::B0), &[(Slot::B0, Slot::C1)], &[]);
        let again = layout(Some(Slot::B0), &[(Slot::B0, Slot::C1)], &[]);
        let mut gate = ReadGate::default();

        assert!(gate.layout_wants_read(layout_signature(&before), 0), "the first sight is always a read");
        gate.record(layout_signature(&before), 7);

        assert!(
            !gate.layout_wants_read(layout_signature(&again), 0),
            "the same board must not be re-read",
        );
    }

    /// A door that opened moves the fingerprint. Fails if `layout_signature`
    /// skips `doors`.
    #[test]
    fn an_opened_corridor_re_arms_the_read() {
        let before = layout(Some(Slot::B0), &[], &[]);
        let after = layout(Some(Slot::B0), &[(Slot::B0, Slot::C1)], &[]);
        let mut gate = ReadGate::default();
        gate.record(layout_signature(&before), 7);

        assert!(gate.layout_wants_read(layout_signature(&after), 0));
    }

    /// Moving to another room moves the fingerprint. Fails if
    /// `layout_signature` skips `current`.
    #[test]
    fn a_new_current_room_re_arms_the_read() {
        let before = layout(Some(Slot::B0), &[], &[]);
        let after = layout(Some(Slot::C1), &[], &[]);
        let mut gate = ReadGate::default();
        gate.record(layout_signature(&before), 7);

        assert!(gate.layout_wants_read(layout_signature(&after), 0));
    }

    /// The second gate: the plates changed but no corridor did — a kill that
    /// upgraded the room the player is standing in. Fails if `panel_signature`
    /// ignores the panel's contents, or if `panel_wants_read` compares against
    /// the layout half of the pair.
    #[test]
    fn a_changed_panel_re_arms_a_read_the_layout_alone_would_skip() {
        let board = layout(Some(Slot::B0), &[], &[]);
        let mut gate = ReadGate::default();
        let before = panel_signature(&panel("Corruption Chamber", Some(6), Vec::new()));
        gate.record(layout_signature(&board), before);

        assert!(
            !gate.layout_wants_read(layout_signature(&board), 0),
            "precondition: the pixels did not move",
        );
        assert!(
            gate.panel_wants_read(panel_signature(&panel(
                "Catalyst of Corruption",
                Some(5),
                Vec::new(),
            ))),
            "a changed panel must re-arm",
        );
        assert!(
            !gate.panel_wants_read(before),
            "an unchanged panel must not",
        );
    }

    /// The point of hashing the PARSED panel: text the crop caught but the
    /// panel did not print — screen furniture, a tooltip, a chat line — leaves
    /// the fingerprint alone, so it cannot buy a 26-crop re-read every four
    /// seconds for as long as it is on screen.
    ///
    /// The stray line is a MIS-READ of the window header, `tample of atzoatl`,
    /// which `panel.rs` measures at 0.9190 against `Apex of Atzoatl` — a
    /// confident, wrong room name. Two guards keep it out of the parse
    /// (`is_screen_furniture`, and `title_match`'s exact-only rule for the two
    /// fixed slots), and `panel.rs` documents the second as the backstop for
    /// the first: gut BOTH and the stray line becomes the title, which moves
    /// the fingerprint and re-arms the read on nothing. Verified as the
    /// mutation — either guard alone still holds this line, which is what
    /// "backstop" means.
    #[test]
    fn text_the_panel_did_not_print_does_not_move_the_panel_fingerprint() {
        let printed = [
            "Tombs".to_string(),
            "Ticaba, Architect of the Arena".to_string(),
            "(Kill to change to Storage Room)".to_string(),
            "9 Incursions Remaining".to_string(),
        ];
        // Between the title and the architect block, which is exactly where
        // `read_panel`'s positional rule looks first — anywhere else and the
        // real title would win for a reason that has nothing to do with the
        // guards this pins.
        let with_stray = [
            printed[0].clone(),
            "Tample of Atzoatl".to_string(),
            printed[1].clone(),
            printed[2].clone(),
            printed[3].clone(),
        ];

        assert_eq!(
            panel_signature(&crate::temple::panel::read_panel(&printed)),
            panel_signature(&crate::temple::panel::read_panel(&with_stray)),
            "text the panel did not print must not re-arm the read",
        );
    }

    /// The other half of the same rule: a field the panel DOES publish moves
    /// the fingerprint. Fails if `panel_signature` drops the offers, which
    /// would leave the slice showing the pre-kill architect block until
    /// something else re-armed.
    #[test]
    fn a_changed_architect_offer_moves_the_panel_fingerprint() {
        let before = panel(
            "Chasm",
            Some(6),
            vec![offer("Guatelitzi", "Corruption Chamber", OfferKind::Change)],
        );
        let after = panel(
            "Chasm",
            Some(6),
            vec![offer("Guatelitzi", "Storage Room", OfferKind::Change)],
        );

        assert_ne!(panel_signature(&before), panel_signature(&after));
    }

    /// The room the panel is titled after re-arms the read. This is the case
    /// the pixel gate cannot see at all: a kill that upgrades the room the
    /// player is standing in changes a name and a numeral and no corridor.
    ///
    /// Fails if `panel_signature` drops the title, which would leave the slice
    /// showing the pre-kill room until the player moved.
    #[test]
    fn a_changed_room_title_moves_the_panel_fingerprint() {
        let before = panel("Catalyst of Corruption", Some(6), Vec::new());
        let after = panel("Locus of Corruption", Some(6), Vec::new());

        assert_ne!(panel_signature(&before), panel_signature(&after));
    }

    /// A budget that ticked down re-arms even when nothing else moved. Fails
    /// if `panel_signature` drops `incursions_remaining`, which the advisor
    /// scores every rollout against.
    #[test]
    fn a_spent_incursion_moves_the_panel_fingerprint() {
        let before = panel("Chasm", Some(6), Vec::new());
        let after = panel("Chasm", Some(5), Vec::new());

        assert_ne!(panel_signature(&before), panel_signature(&after));
    }

    /// Closing and reopening the panel re-arms even on an identical board.
    /// Fails if `on_panel_lost` does not clear the recorded read.
    #[test]
    fn losing_the_panel_re_arms_the_next_identical_board() {
        let board = layout(Some(Slot::B0), &[], &[]);
        let mut gate = ReadGate::default();
        gate.record(layout_signature(&board), 7);
        assert!(!gate.layout_wants_read(layout_signature(&board), 0), "precondition");

        gate.on_panel_lost();

        assert!(gate.layout_wants_read(layout_signature(&board), 0));
    }

    /// The explicit re-arm forces exactly ONE read, not a permanent one.
    /// Fails if the counter is never recorded (re-reads forever) or recorded
    /// without forcing the read (the button does nothing).
    #[test]
    fn an_explicit_rearm_forces_one_read_and_then_stops() {
        let board = layout(Some(Slot::B0), &[], &[]);
        let mut gate = ReadGate::default();
        gate.record(layout_signature(&board), 7);

        assert!(gate.layout_wants_read(layout_signature(&board), 1), "the bump forces a read");
        gate.record(layout_signature(&board), 7);
        assert!(
            !gate.layout_wants_read(layout_signature(&board), 1),
            "the same counter value must not force a second read",
        );
    }

    /// `rearm_pending` is a pure peek: it reports the bump and spends nothing.
    /// [`ReadGate::note_rearm`] is the single place the bump is spent (the
    /// promoting detect tick calls it; so does `layout_wants_read`).
    ///
    /// Fails if `rearm_pending` consumes the bump itself — the tick that
    /// peeked would then be the only one to ever see it, and a read that
    /// spends it through `note_rearm` would no longer drop the recorded board.
    #[test]
    fn rearm_pending_peeks_and_note_rearm_is_what_spends_the_bump() {
        let board = layout(Some(Slot::B0), &[], &[]);
        let mut gate = ReadGate::default();
        gate.record(layout_signature(&board), 7);
        assert!(!gate.rearm_pending(0), "nothing pressed yet");

        assert!(gate.rearm_pending(1), "the bump is visible");
        assert!(gate.rearm_pending(1), "and peeking does not spend it");
        assert!(
            gate.layout_wants_read(layout_signature(&board), 1),
            "the read still sees the bump it was promoted for",
        );
        assert!(!gate.rearm_pending(1), "which is what spends it");
    }

    /// A disabled module publishes no advice, whatever the loop left behind.
    /// Fails if `force_off` only rewrites the status — a stale recommendation
    /// under an "off" badge is exactly the lie the precedence step prevents.
    #[test]
    fn forcing_a_disabled_slice_off_also_drops_its_advice() {
        let mut s = TempleSlice {
            status: TempleStatus::Read,
            advice: Some(AdviceView {
                recommendations: Vec::new(),
                gambles: Vec::new(),
                map_action: "leaveMap".to_string(),
                warnings: Vec::new(),
                forced_kill: false,
            }),
            mode: Some("chase".to_string()),
            unknown_rooms: vec!["A0".to_string()],
            last_error: Some("Temple: screen capture failed — no monitor".to_string()),
            ..TempleSlice::default()
        };

        force_off(&mut s);

        assert_eq!(s.status, TempleStatus::Off);
        assert_eq!(s.advice, None);
        assert_eq!(s.mode, None);
        assert_eq!(
            s.last_error, None,
            "a red line under an off badge reads as broken, not as switched off",
        );
        assert_eq!(
            s.unknown_rooms,
            vec!["A0".to_string()],
            "only the acting half is forced — what was read stays readable",
        );
    }

    /// The settings echo survives the module being switched off. Fails if
    /// `force_off` blanks it: the page renders the keys/flags/profile controls
    /// from the slice alone, so a wipe here would show every control at its
    /// derive default while `settings.json` says otherwise, with the module off
    /// and no loop left to correct it.
    #[test]
    fn forcing_a_disabled_slice_off_keeps_the_settings_echo() {
        let profile = TempleProfileSettings { apex_score: 42.0, ..Default::default() };
        let config = TempleConfig { artefacts_of_the_vaal: false, scarab_of_timelines: true };
        let mut s = TempleSlice {
            status: TempleStatus::Read,
            keys: 2,
            config: config.clone(),
            profile: profile.clone(),
            ..TempleSlice::default()
        };

        force_off(&mut s);

        assert_eq!(s.keys, 2);
        assert_eq!(s.config, config);
        assert_eq!(s.profile, profile);
    }

    /// A read that lost a text region says so on the SLICE, as a notice and not
    /// as an error.
    ///
    /// The failure this closes: the panel crop falls entirely outside the
    /// capture, `run::panel_text` steps over it, and the read completes with an
    /// empty offer list that is indistinguishable on screen from a panel with
    /// no architects printed on it. The board is real — the plates come off the
    /// lattice and are unaffected — so the read is not a failure and
    /// `last_error` (red, "Last error", written by the status machine beside
    /// `TempleStatus::Error`) is the wrong channel for it.
    ///
    /// Fails if `project` drops the field, or if it routes the notice into
    /// `last_error`.
    #[test]
    fn a_read_that_lost_a_text_region_publishes_it_as_a_notice_not_an_error() {
        let layout = layout(Some(Slot::E1), &[], &[]);
        let rooms = board_rooms(&[(Slot::E1, "Chamber of Iron")]);
        let panel = panel("Chamber of Iron", None, Vec::new());
        let notice = "Temple: panel ROI [1920, 4, 544, 454] is outside the capture — windowed client?";
        let result = ReadResult {
            read_notice: Some(notice.to_string()),
            ..read(&layout, &rooms, &panel, None, None)
        };

        let s = project(&result, None);

        assert_eq!(s.read_notice.as_deref(), Some(notice));
        assert_eq!(s.last_error, None, "a completed read is not an error");
        assert_eq!(s.status, TempleStatus::Read, "and it is not an error STATUS either");
    }

    /// …and the next read that gets its regions back clears it.
    ///
    /// `project` writes the whole slice, so this is what makes the notice a
    /// STATE rather than a history: a warning that outlived the condition would
    /// have the player chasing a windowed client they have already maximised.
    /// Fails if `read_notice` is only ever set.
    #[test]
    fn a_read_with_every_region_in_frame_clears_the_notice() {
        let layout = layout(Some(Slot::E1), &[], &[]);
        let rooms = board_rooms(&[(Slot::E1, "Chamber of Iron")]);
        let panel = panel("Chamber of Iron", Some(6), Vec::new());

        let s = project(&read(&layout, &rooms, &panel, None, None), None);

        assert_eq!(s.read_notice, None);
    }

    /// A completed read republishes the settings it was ranked under.
    ///
    /// `project` writes the WHOLE slice (`*slice = projected` in `run`), so a
    /// field the projection forgets is a field every read blanks — the control
    /// on the page would snap back to its default one second after the user
    /// moved it. Fails if the echo is dropped from `project`.
    #[test]
    fn a_read_republishes_the_settings_it_was_ranked_under() {
        let layout = layout(Some(Slot::E1), &[], &[]);
        let rooms = board_rooms(&[(Slot::E1, "Chamber of Iron")]);
        let panel = panel("Chamber of Iron", Some(6), Vec::new());
        let config = TempleConfig { artefacts_of_the_vaal: false, scarab_of_timelines: true };
        let profile = TempleProfileSettings { path_cost: 3.5, ..Default::default() };
        let result = ReadResult {
            keys: 2,
            config: config.clone(),
            profile: profile.clone(),
            ..read(&layout, &rooms, &panel, None, None)
        };

        let published = project(&result, None);

        assert_eq!(published.keys, 2);
        assert_eq!(published.config, config);
        assert_eq!(published.profile, profile);
    }

    // ------------------------------------ the never-cover set (POE-244) --

    /// Every region the module reads reaches the slice, and each is the SAME
    /// rectangle the reader will use on the next tick.
    ///
    /// The overlay's whole guarantee rests on this: a rect missing from the
    /// list is a rect a callout may be placed over, and the failure is a read
    /// that quietly degrades on a machine nobody is looking at. Asserted
    /// against the owning functions rather than against numbers, because what
    /// has to hold is that they are the same answer, not that they are any
    /// particular value.
    #[test]
    fn every_read_region_reaches_the_slice_as_a_never_cover_rect() {
        let layout = layout(Some(Slot::C1), &[], &[]);
        let rooms = board_rooms(&[]);
        let panel = panel("Chamber of Iron", Some(6), Vec::new());

        let published = project(&read(&layout, &rooms, &panel, None, None), None)
            .layout
            .expect("a read publishes a layout");
        let rois = published.rois;

        let of_kind = |kind: &str| -> Vec<&RoiView> {
            rois.iter().filter(|r| r.kind == kind).collect()
        };
        assert_eq!(
            of_kind("panel").iter().map(|r| r.rect).collect::<Vec<_>>(),
            vec![super::super::run::panel_rect(FIXTURE_ORIGIN, FIXTURE_SCALE)]
        );
        assert_eq!(
            of_kind("diamond").iter().map(|r| r.rect).collect::<Vec<_>>(),
            vec![super::super::run::diamond_rect(FIXTURE_ORIGIN, FIXTURE_SCALE)]
        );
        assert_eq!(
            of_kind("remaining").iter().map(|r| r.rect).collect::<Vec<_>>(),
            vec![super::super::run::remaining_rect(FIXTURE_ORIGIN, FIXTURE_SCALE)]
        );
        // One per plate and one per corridor, named by the board key a debug
        // surface would print — 3 + 13 + 26.
        assert_eq!(
            of_kind("plate").iter().filter_map(|r| r.of.clone()).collect::<Vec<_>>(),
            Slot::ALL.iter().map(|s| s.as_str().to_string()).collect::<Vec<_>>()
        );
        assert_eq!(
            of_kind("corridor").iter().filter_map(|r| r.of.clone()).collect::<Vec<_>>(),
            crate::temple::lattice::edges().iter().map(|e| e.to_string()).collect::<Vec<_>>()
        );
        assert_eq!(rois.len(), 42);
    }

    /// A plate's one published rect covers BOTH boxes that plate is OCR'd from.
    ///
    /// The union is the deliberate simplification (`run::read_rois`), and this
    /// is the property that makes it safe to publish one rect where the reader
    /// crops two: nothing kept out of the union can be inside either crop.
    #[test]
    fn a_plate_roi_contains_both_of_the_boxes_that_plate_is_read_from() {
        let lattice = Lattice::new(FIXTURE_ORIGIN, FIXTURE_SCALE);
        let rois = super::super::run::read_rois(FIXTURE_ORIGIN, FIXTURE_SCALE);
        let contains = |outer: [i32; 4], inner: [i32; 4]| {
            outer[0] <= inner[0]
                && outer[1] <= inner[1]
                && outer[0] + outer[2] >= inner[0] + inner[2]
                && outer[1] + outer[3] >= inner[1] + inner[3]
        };

        for slot in Slot::ALL {
            let roi = rois
                .iter()
                .find(|r| r.kind == "plate" && r.of.as_deref() == Some(slot.as_str()))
                .unwrap_or_else(|| panic!("no plate rect for {}", slot.as_str()));
            for inner in [
                crate::temple::panel::name_strip(&lattice, slot),
                crate::temple::panel::numeral_box(&lattice, slot),
            ] {
                assert!(
                    contains(roi.rect, inner),
                    "{}: {:?} does not contain {inner:?}",
                    slot.as_str(),
                    roi.rect
                );
            }
        }
    }

    /// A corridor's rect is the patch the beam sampler actually measures —
    /// same centre, same half-width, same truncation. One pixel short on any
    /// side is a pixel the overlay may cover and the sampler still reads.
    #[test]
    fn a_corridor_roi_is_the_patch_the_beam_sampler_measures() {
        let lattice = Lattice::new(FIXTURE_ORIGIN, FIXTURE_SCALE);
        let hw = (crate::temple::lattice::PATCH_HALF * FIXTURE_SCALE as f64) as i32;
        let rois = super::super::run::read_rois(FIXTURE_ORIGIN, FIXTURE_SCALE);

        for edge in crate::temple::lattice::edges() {
            let roi = rois
                .iter()
                .find(|r| r.kind == "corridor" && r.of.as_deref() == Some(&edge.to_string()))
                .unwrap_or_else(|| panic!("no corridor rect for {edge}"));
            let (mx, my) = lattice.edge_midpoint(edge);
            assert_eq!(roi.rect, [mx - hw, my - hw, 2 * hw, 2 * hw], "{edge}");
        }
    }

    // --------------------------------------- the room's diamond (POE-244) --

    /// One seal per corridor the room HAS, open or shut, and each one keyed by
    /// both halves a consumer needs: the neighbour it leads to, and the edge id
    /// `doors` / `uncertain` are spelled in.
    #[test]
    fn the_current_rooms_diamond_carries_one_seal_per_neighbour() {
        // C1 is the only six-corridor shape on the board.
        let layout = layout(Some(Slot::C1), &[(Slot::C1, Slot::C2)], &[]);
        let rooms = board_rooms(&[]);
        let panel = panel("Chamber of Iron", Some(6), Vec::new());

        let diamond = project(&read(&layout, &rooms, &panel, None, None), None)
            .layout
            .and_then(|l| l.diamond)
            .expect("a current room publishes a diamond");

        assert_eq!(
            diamond.seals.iter().map(|s| s.neighbour.clone()).collect::<Vec<_>>(),
            crate::temple::lattice::neighbours(Slot::C1)
                .iter()
                .map(|s| s.as_str().to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            diamond.seals.iter().map(|s| s.edge.clone()).collect::<Vec<_>>(),
            vec!["B0-C1", "B1-C1", "C0-C1", "C1-C2", "C1-D1", "C1-D2"]
        );
        // The outline is `markers::diamond_corners()` verbatim — the shape the
        // seal positions are normalised onto, so a consumer scaling one and not
        // the other draws seals off the walls.
        assert_eq!(diamond.corners, markers::diamond_corners().map(|(x, y)| [x, y]));
    }

    /// Every published seal is ON the outline the same payload carries
    /// (POE-248), which is where the panel draws them.
    ///
    /// The version POE-244 shipped asserted the seals lay on the published
    /// OUTLINE and was right to; what was wrong was the outline — a rhombus
    /// fitted to the seals, with the seals themselves then put on a constant
    /// ring INSIDE it. POE-248 measured the gold border instead and the two
    /// claims became one again: the room is a rectangle, and a door is a hole
    /// in a wall. This is the consumer-visible half of that — a widget scales
    /// the corners into its box and every seal is on the polygon it drew.
    ///
    /// The test is a point-in-polygon check on the PUBLISHED corners rather
    /// than a call back into `markers`: a projection that changed both sides at
    /// once would still have to keep them agreeing.
    #[test]
    fn every_published_seal_sits_on_the_published_outline() {
        for current in Slot::ALL {
            let layout = layout(Some(current), &[], &[]);
            let rooms = board_rooms(&[]);
            let panel = panel("Chamber of Iron", Some(6), Vec::new());
            let diamond = project(&read(&layout, &rooms, &panel, None, None), None)
                .layout
                .and_then(|l| l.diamond)
                .expect("a current room publishes a diamond");

            assert!(!diamond.seals.is_empty(), "{} has no seals", current.as_str());
            for seal in &diamond.seals {
                // Distance to the nearest of the four edges, as a fraction of
                // the shape: zero means the point is on the boundary.
                let worst = (0..4)
                    .map(|i| {
                        let [ax, ay] = diamond.corners[i];
                        let [bx, by] = diamond.corners[(i + 1) % 4];
                        let (ex, ey) = (bx - ax, by - ay);
                        let (px, py) = (seal.pos[0] - ax, seal.pos[1] - ay);
                        // |cross| / |edge| — the perpendicular distance to the
                        // line the edge lies on.
                        (ex * py - ey * px).abs() / ex.hypot(ey)
                    })
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    worst < 1e-9,
                    "{} -> {} is {worst} off every wall of the outline",
                    current.as_str(),
                    seal.neighbour,
                );
            }
        }
    }

    /// The two architect icon spots reach the wire, mirrored and inside the
    /// room (POE-248).
    ///
    /// The kill glyph the overlay draws is placed from these two fields alone,
    /// so a payload that dropped them (or published the same point twice) would
    /// put the glyph on the wrong architect's half with nothing to fail.
    #[test]
    fn a_published_diamond_carries_both_architect_icon_spots() {
        let layout = layout(Some(Slot::C1), &[], &[]);
        let rooms = board_rooms(&[]);
        let panel = panel("Chamber of Iron", Some(6), Vec::new());

        let diamond = project(&read(&layout, &rooms, &panel, None, None), None)
            .layout
            .and_then(|l| l.diamond)
            .expect("a current room publishes a diamond");

        let (top, bottom) = (diamond.top_icon, diamond.bottom_icon);
        assert_eq!(
            [top[0] + bottom[0], top[1] + bottom[1]],
            [0.0, 0.0],
            "the two spots must be reflections through the room's centre",
        );
        // `+AXIS_X` is right and up on screen, so the field named for the
        // top-right half must actually be in it.
        assert!(
            top[0] > 0.0 && top[1] < 0.0,
            "the top spot belongs in the top-right half: {top:?}",
        );
        let half_long = diamond.corners[0][0].hypot(diamond.corners[0][1]);
        let reach = top[0].hypot(top[1]) / half_long;
        assert!(
            (0.2..0.45).contains(&reach),
            "the icon sits well inside the room; it is at {reach} of the way to a corner",
        );
    }

    /// Between rooms there is no room to draw, so there is no diamond — the
    /// same rule `advice` already follows. A widget that got an empty one would
    /// draw a doorless box over the game.
    #[test]
    fn a_board_with_no_current_room_publishes_no_diamond() {
        let layout = layout(None, &[], &[]);
        let rooms = board_rooms(&[]);
        let panel = panel("Chamber of Iron", Some(6), Vec::new());

        let published = project(&read(&layout, &rooms, &panel, None, None), None);

        assert_eq!(published.status, TempleStatus::NoCurrentRoom);
        assert_eq!(published.layout.and_then(|l| l.diamond), None);
    }

    /// The whole default slice, as JSON, character for character.
    ///
    /// This string is COPIED into `desktop/src/lib/temple/slice.test.ts`, where
    /// it is asserted against `templeSliceDefault()`. That is the whole point:
    /// the two mirrors cannot be checked against each other at build time, so
    /// one pinned string is checked from both sides and a rename, a dropped
    /// `rename_all`, an added field or a changed default fails HERE and in the
    /// TS suite rather than rendering an empty control on the page.
    ///
    /// Note the calibration's keys: `AnchorCalibration` carries no
    /// `rename_all`, so its fields stay snake_case INSIDE a camelCase parent.
    #[test]
    fn the_default_slice_json_is_pinned_for_the_typescript_mirror() {
        let json = serde_json::to_string(&TempleSlice::default()).expect("slice serialises");

        assert_eq!(
            json,
            r#"{"status":"idle","layout":null,"panel":null,"advice":null,"mode":null,"keys":0,"config":{"artefactsOfTheVaal":true,"scarabOfTimelines":false},"profile":{"apexScore":2.0,"pathCost":0.0,"rerollUntilFavourable":false,"r4KeepUpgradeTargets":true},"unknownRooms":[],"lastReadAt":null,"calibration":null,"readNotice":null,"lastError":null}"#,
        );
    }

    /// A fully populated slice, as JSON, character for character.
    ///
    /// The sibling of the default pin, and the one that actually exercises the
    /// nested views: `LayoutView`, `SlotView`, `PanelView`, `OfferView`,
    /// `AdviceView`, `RankedView` and `AnchorCalibration`. Copied verbatim into
    /// `desktop/src/lib/temple/slice.test.ts`, which parses it as a
    /// `TempleSlice` — so a field this side renames and the TS side does not is
    /// a failure in both suites rather than a silently missing value on the page.
    #[test]
    fn a_populated_slice_json_is_pinned_for_the_typescript_mirror() {
        let s = TempleSlice {
            status: TempleStatus::Read,
            layout: Some(LayoutView {
                slots: vec![SlotView {
                    slot: "A0".to_string(),
                    name: Some("Apex of Atzoatl".to_string()),
                    tier: 0,
                    exact: true,
                    known: true,
                    current: false,
                }],
                doors: vec!["C1-C2".to_string()],
                uncertain: vec!["B0-C1".to_string()],
                unresolved_incident: vec!["B0-C1".to_string()],
                marker_error: Some("the diamond rect fell outside the capture".to_string()),
                current: Some("C1".to_string()),
                scale: 0.99,
                ncc: 0.94,
                confidence: "high".to_string(),
                // The board `Lattice::new((900, 900), 0.99)` builds — the same
                // origin and scale a reader would have measured, so the pinned
                // JSON below carries real geometry rather than filler.
                origin: [900, 900],
                centres: [
                    [900, 465],
                    [795, 569],
                    [1005, 569],
                    [690, 673],
                    [900, 673],
                    [1110, 673],
                    [585, 777],
                    [795, 777],
                    [1005, 777],
                    [1215, 777],
                    [690, 881],
                    [900, 900],
                    [1110, 881],
                ],
                // TWO of the 42 a real read publishes, and one seal of C1's
                // four. Like every other field of this sample these are
                // hand-built: the pin is a check on the wire SHAPE, and
                // `every_read_region_reaches_the_slice_as_a_never_cover_rect`
                // and `the_current_rooms_diamond_carries_one_seal_per_neighbour`
                // are what assert the projection's real content. A full board
                // here would be a 4 kB literal nobody could read a rename out
                // of.
                rois: vec![
                    RoiView { kind: "panel".to_string(), of: None, rect: [1100, 40, 500, 400] },
                    RoiView {
                        kind: "corridor".to_string(),
                        of: Some("C1-C2".to_string()),
                        rect: [991, 659, 27, 27],
                    },
                ],
                // Round, obviously synthetic numbers — NOT the fitted shape.
                // The rectangle's real corners are irrational in every
                // coordinate, and a sample carrying them would read as the
                // measurement while being a second copy of it;
                // `the_current_rooms_diamond_carries_one_seal_per_neighbour`
                // asserts the projection itself. What is pinned here is that
                // four corners, a seal and BOTH icon spots reach the wire under
                // these names.
                diamond: Some(DiamondView {
                    corners: [[1.4, -0.1], [-0.1, 1.2], [-1.4, 0.1], [0.1, -1.2]],
                    seals: vec![SealView {
                        neighbour: "C2".to_string(),
                        edge: "C1-C2".to_string(),
                        pos: [1.0, -0.9],
                    }],
                    top_icon: [0.34, -0.3],
                    bottom_icon: [-0.34, 0.3],
                }),
            }),
            panel: Some(PanelView {
                room: Some("Locus of Corruption".to_string()),
                // Capture px, as the reader publishes them (POE-243): the
                // title's own line, and the union of the two lines the offer
                // wrapped over.
                room_rect: Some([1300, 100, 152, 20]),
                offers: vec![OfferView {
                    index: 0,
                    architect_name: "Guatelitzi".to_string(),
                    kind: "upgrade".to_string(),
                    printed_target: "Sadist's Den".to_string(),
                    display_name: Some("Torment Cells".to_string()),
                    built_tier: Some(2),
                    rect: Some([1300, 140, 280, 43]),
                }],
                incursions_remaining: Some(6),
            }),
            advice: Some(AdviceView {
                recommendations: vec![RankedView {
                    headline: "upgrade → Locus of Corruption".to_string(),
                    doors_label: "C1-C2, B0-C1".to_string(),
                    doors: vec!["C1-C2".to_string(), "B0-C1".to_string()],
                    architect_index: Some(0),
                    ev: 12.5,
                    risk: None,
                    reasons: vec!["R1: connects toward the top".to_string()],
                }],
                gambles: vec![RankedView {
                    headline: "kill either".to_string(),
                    doors_label: "no door".to_string(),
                    doors: Vec::new(),
                    architect_index: None,
                    ev: 14.0,
                    risk: Some(0.31),
                    reasons: vec!["RV: excluded above the risk threshold".to_string()],
                }],
                map_action: "leaveMap".to_string(),
                // Two warnings, and the second is the one `forced_kill`
                // mirrors. Hand-built, like every other field of this sample —
                // the pin is a check on the wire SHAPE, and nothing here
                // asserts the projection would pair these particular values.
                // `a_one_of_two_architect_read_reaches_the_slice_as_prose_and_as_a_flag`
                // is what asserts the pairing.
                warnings: vec![
                    "the incursion budget was not legible".to_string(),
                    "1 of 2 architects read — the kill shown is forced, not chosen".to_string(),
                ],
                forced_kill: true,
            }),
            mode: Some("chase".to_string()),
            keys: 2,
            config: TempleConfig { artefacts_of_the_vaal: false, scarab_of_timelines: true },
            profile: TempleProfileSettings {
                apex_score: 3.5,
                path_cost: 1.25,
                reroll_until_favourable: true,
                r4_keep_upgrade_targets: false,
            },
            unknown_rooms: vec!["D3".to_string()],
            last_read_at: Some(1_700_000_000_000),
            calibration: Some(AnchorCalibration { screen_w: 2560, screen_h: 1440, scale: 0.99 }),
            read_notice: Some(
                "Temple: remaining ROI [810, 771, 300, 46] is outside the capture — windowed client?"
                    .to_string(),
            ),
            last_error: Some("Temple: OCR failed".to_string()),
        };

        assert_eq!(serde_json::to_string(&s).expect("slice serialises"), SAMPLE_SLICE_JSON);
    }

    /// The pinned sample. Kept as a constant so the string the TS suite copies
    /// is one literal rather than a value spread across an assertion.
    const SAMPLE_SLICE_JSON: &str = r#"{"status":"read","layout":{"slots":[{"slot":"A0","name":"Apex of Atzoatl","tier":0,"exact":true,"known":true,"current":false}],"doors":["C1-C2"],"uncertain":["B0-C1"],"unresolvedIncident":["B0-C1"],"markerError":"the diamond rect fell outside the capture","current":"C1","scale":0.99,"ncc":0.94,"confidence":"high","origin":[900,900],"centres":[[900,465],[795,569],[1005,569],[690,673],[900,673],[1110,673],[585,777],[795,777],[1005,777],[1215,777],[690,881],[900,900],[1110,881]],"rois":[{"kind":"panel","of":null,"rect":[1100,40,500,400]},{"kind":"corridor","of":"C1-C2","rect":[991,659,27,27]}],"diamond":{"corners":[[1.4,-0.1],[-0.1,1.2],[-1.4,0.1],[0.1,-1.2]],"seals":[{"neighbour":"C2","edge":"C1-C2","pos":[1.0,-0.9]}],"topIcon":[0.34,-0.3],"bottomIcon":[-0.34,0.3]}},"panel":{"room":"Locus of Corruption","roomRect":[1300,100,152,20],"offers":[{"index":0,"architectName":"Guatelitzi","kind":"upgrade","printedTarget":"Sadist's Den","displayName":"Torment Cells","builtTier":2,"rect":[1300,140,280,43]}],"incursionsRemaining":6},"advice":{"recommendations":[{"headline":"upgrade → Locus of Corruption","doorsLabel":"C1-C2, B0-C1","doors":["C1-C2","B0-C1"],"architectIndex":0,"ev":12.5,"risk":null,"reasons":["R1: connects toward the top"]}],"gambles":[{"headline":"kill either","doorsLabel":"no door","doors":[],"architectIndex":null,"ev":14.0,"risk":0.31,"reasons":["RV: excluded above the risk threshold"]}],"mapAction":"leaveMap","warnings":["the incursion budget was not legible","1 of 2 architects read — the kill shown is forced, not chosen"],"forcedKill":true},"mode":"chase","keys":2,"config":{"artefactsOfTheVaal":false,"scarabOfTimelines":true},"profile":{"apexScore":3.5,"pathCost":1.25,"rerollUntilFavourable":true,"r4KeepUpgradeTargets":false},"unknownRooms":["D3"],"lastReadAt":1700000000000,"calibration":{"screen_w":2560,"screen_h":1440,"scale":0.99},"readNotice":"Temple: remaining ROI [810, 771, 300, 46] is outside the capture — windowed client?","lastError":"Temple: OCR failed"}"#;

    /// Every `TempleStatus` variant's wire string, pinned one by one.
    ///
    /// `snake_case`, matching `MercStatus` and the app's enum convention (see
    /// the type's own note). The page switches on these strings, so a silent
    /// `rename_all` change would leave every branch falling through to the
    /// default rather than erroring.
    #[test]
    fn every_temple_status_wire_string_is_pinned() {
        let statuses = [
            (TempleStatus::Off, "off"),
            (TempleStatus::Idle, "idle"),
            (TempleStatus::Waiting, "waiting"),
            (TempleStatus::PanelNotVisible, "panel_not_visible"),
            (TempleStatus::Reading, "reading"),
            (TempleStatus::Read, "read"),
            (TempleStatus::NoCurrentRoom, "no_current_room"),
            (TempleStatus::Unavailable, "unavailable"),
            (TempleStatus::Error, "error"),
        ];
        for (status, wire) in statuses {
            assert_eq!(serde_json::to_value(status).unwrap(), wire);
        }
    }

    // ------------------------------------------------------- validation --

    /// Keys are 0, 1 or 2 — the game drops no more. Fails if the bound is
    /// exclusive or absent.
    #[test]
    fn key_counts_above_two_are_rejected() {
        assert!(validate_keys(0).is_ok(), "zero keys is a legal board");
        assert!(validate_keys(2).is_ok(), "two keys is the maximum");
        let err = validate_keys(3).expect_err("three keys is not a board the game produces");
        assert!(err.contains('3'), "the message names the rejected value, got {err:?}");
    }

    /// Both profile weights are magnitudes. Fails if `validate` accepts a sign
    /// error or a NaN into the float ordering.
    #[test]
    fn negative_and_non_finite_profile_weights_are_rejected() {
        let mut p = TempleProfileSettings::default();
        assert!(p.validate().is_ok(), "the shipped defaults must validate");

        p.apex_score = -1.0;
        assert!(p.validate().is_err(), "a negative apex score is a sign error");

        p = TempleProfileSettings::default();
        p.path_cost = -0.1;
        assert!(p.validate().is_err(), "a negative path cost pays for walking");

        p = TempleProfileSettings::default();
        p.apex_score = f64::NAN;
        assert!(p.validate().is_err(), "NaN makes the whole ranking arbitrary");
    }

    /// The four settings fields reach the profile they tune, and the fields
    /// they do NOT own are left at the Rush's values. Fails if `to_profile`
    /// drops a field or rebuilds the profile from scratch.
    #[test]
    fn profile_settings_override_only_their_own_four_fields() {
        let settings = TempleProfileSettings {
            apex_score: 8.5,
            path_cost: 0.4,
            reroll_until_favourable: true,
            r4_keep_upgrade_targets: false,
        };

        let profile = settings.to_profile();

        assert_eq!(profile.apex_score, 8.5);
        assert_eq!(profile.path_cost, 0.4);
        assert!(profile.reroll_until_favourable);
        assert!(!profile.r4_keep_upgrade_targets);
        assert_eq!(
            profile.combinations,
            StrategyProfile::locus_doryani_rush().combinations,
            "the strategy's identity is not a settings field",
        );
    }

    // ------------------------------------------------ settings round-trip --

    /// The whole settings block survives a JSON round-trip. Fails if a field is
    /// `skip`ped or renamed on one side only.
    #[test]
    fn temple_settings_round_trip_through_json() {
        let settings = TempleSettings {
            profile: TempleProfileSettings {
                apex_score: 9.0,
                path_cost: 1.5,
                reroll_until_favourable: true,
                r4_keep_upgrade_targets: false,
            },
            config: TempleConfig {
                artefacts_of_the_vaal: false,
                scarab_of_timelines: true,
            },
            keys: 2,
        };

        let json = serde_json::to_string(&settings).expect("settings serialise");
        let back: TempleSettings = serde_json::from_str(&json).expect("settings parse back");

        assert_eq!(back, settings);
    }

    /// `TempleConfig` is camelCase on the wire, like every other temple type —
    /// it is the `temple_set_config` argument and a `TempleSettings` field
    /// before it is a `settings.json` key.
    ///
    /// Fails if the `rename_all` is dropped, which would leave the command
    /// silently rejecting what the webview sends (serde has no key to bind) and
    /// the persisted block half snake, half camel.
    #[test]
    fn temple_config_is_camel_case_on_the_wire() {
        let json = serde_json::to_value(TempleConfig::default()).expect("config serialises");

        assert_eq!(json["artefactsOfTheVaal"], serde_json::json!(true));
        assert_eq!(json["scarabOfTimelines"], serde_json::json!(false));
        assert_eq!(
            json.get("artefacts_of_the_vaal"),
            None,
            "the snake_case key must be gone, not merely joined",
        );
    }

    /// A `temple_config` object carrying only one of the two keys fills the
    /// other from `Default` instead of failing.
    ///
    /// This is not a nicety: `Settings` deserialises as a unit, so a partial
    /// object would abort the WHOLE file and `load` would fall back to
    /// `Settings::default()` — silently resetting every unrelated preference.
    /// Fails if the struct-level `#[serde(default)]` is dropped.
    #[test]
    fn a_partial_temple_config_object_fills_the_missing_flag() {
        let parsed: TempleConfig = serde_json::from_str(r#"{"scarabOfTimelines":true}"#)
            .expect("a partial config must still parse");

        assert!(parsed.scarab_of_timelines, "the key that was present wins");
        assert_eq!(
            parsed.artefacts_of_the_vaal,
            TempleConfig::default().artefacts_of_the_vaal,
            "the missing key comes from Default, not from `bool::default()`",
        );
    }

    /// A settings file written by an older build — every temple key missing —
    /// loads as the Rush rather than as zeros. Fails if a `#[serde(default)]`
    /// is dropped or if `Default` is used where `shipped()` is meant.
    #[test]
    fn missing_profile_keys_default_to_the_rush() {
        let parsed: TempleProfileSettings =
            serde_json::from_str("{}").expect("an empty object is a valid partial profile");

        let rush = StrategyProfile::locus_doryani_rush();
        assert_eq!(parsed.apex_score, rush.apex_score);
        assert_eq!(parsed.path_cost, rush.path_cost);
        assert_eq!(parsed.reroll_until_favourable, rush.reroll_until_favourable);
        assert_eq!(parsed.r4_keep_upgrade_targets, rush.r4_keep_upgrade_targets);
    }

    /// Fresh install defaults: one key, not zero. Fails if a caller reaches
    /// for `TempleSettings::default()` where `shipped()` is meant.
    #[test]
    fn the_shipped_default_is_one_key() {
        assert_eq!(TempleSettings::shipped().keys, 1);
        assert_eq!(
            TempleSettings::shipped().config,
            TempleConfig::default(),
            "Artefacts of the Vaal on, scarab off",
        );
    }

    /// `identities` is indexed by slot, not by position, so a short or
    /// reordered reading vector cannot put a room in the wrong slot. Fails if
    /// it `collect`s positionally.
    #[test]
    fn identities_are_placed_by_slot_not_by_position() {
        let readings = vec![RoomReading {
            slot: Slot::C1,
            identity: match_room_name("Locus of Corruption"),
        }];

        let out = identities(&readings);

        assert_eq!(out[Slot::C1.index()], resolve_name("Locus of Corruption"));
        assert!(
            out.iter().filter(|id| id.is_some()).count() == 1,
            "one reading fills exactly one slot",
        );
        assert_eq!(out[0], None, "the first slot was not the one read");
    }
}
