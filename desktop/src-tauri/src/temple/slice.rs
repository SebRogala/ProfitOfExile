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
use super::panel::{ARCHITECTS_PER_PANEL, ArchitectOffer, PanelReading, RoomReading};
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

/// The opening stones the player has in hand when the sheet is read: one.
///
/// The advisor keeps its `keys` parameter, and since POE-253 this constant is
/// its only production value. There is no setting behind it: stones drop from
/// the kill INSIDE the incursion, after the sheet has been read, so a count
/// asked for up front is a prediction the player cannot make. A second stone
/// in hand is `advisor::conditional_second_door`'s answer — the faint
/// second door it draws is what that case looks like on screen.
pub const KEYS_IN_HAND: u8 = 1;

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

/// Everything the temple module persists — the user's profile tuning and the
/// two config flags.
///
/// One `AppState` Mutex holds this whole struct while `settings.json` keeps two
/// separate fields, so a hand-edited file stays readable and one bad field
/// defaults on its own (`#[serde(default)]` per field on `Settings`).
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
    /// Vertolka's grade for the LINE this kill builds into, as it is written on
    /// his sheet — `"A++"`, `"C-"` (POE-249). The grade is the LINE's and not
    /// the built room's: he ranks each of the 25 families by what its tier-3
    /// room is worth, so a `change` that lands on tier 2 carries the same
    /// letter as one that lands on tier 3. [`line_top`](Self::line_top) is the
    /// room that letter is about.
    ///
    /// `None` when the printed target did not resolve — the same silence as
    /// [`display_name`](Self::display_name), and for the same reason: there is
    /// no line to have a grade.
    ///
    /// A string and not the `Grade` enum because `Grade` is a reasoning type
    /// with a derived `Ord` (worst first) and no `Serialize`; the sheet's own
    /// spelling is the wire form.
    #[serde(default)]
    pub grade: Option<String>,
    /// The tier-3 room of the line this kill builds into — what
    /// [`grade`](Self::grade) is a grade OF. `None` on the same failure.
    #[serde(default)]
    pub line_top: Option<String>,
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
    /// The corridor a SECOND Stone of Passage would buy, given the top
    /// recommendation — `"C1-C2"`, or `None` (POE-248).
    ///
    /// `advisor::conditional_second_door` owns the meaning and every reason it
    /// is absent — including the one that is not a missing corridor but an
    /// ANSWER: with the primary door's own singleton in the conditional
    /// ranking, RU can win it, which is the chain saying *do not spend a second
    /// key on this board*. This is the wire form. The overlay draws it as a faint purple
    /// seal beside the bright suggested one, which is what lets a player who
    /// finds a second stone mid-incursion act on it — and, since POE-253, is
    /// the WHOLE answer to a second stone: there is no count to configure.
    ///
    /// NOT a member of `recommendations[0].doors`: those are the doors to open
    /// NOW, and merging the conditional one into them would tell a one-key
    /// player to spend a key they do not have.
    ///
    /// `serde(default)` for the same reason [`Self::forced_kill`] carries one —
    /// a payload from a build before POE-248 decodes as "no second door" rather
    /// than failing the whole slice — and the webview mirror normalises the
    /// missing value to `null`, so the two ends agree about what silence means.
    #[serde(default)]
    pub secondary_door: Option<String>,
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
    /// Whether Alva has started an incursion the module has not yet found the
    /// temple sheet for (POE-249).
    ///
    /// The ONE new lifecycle state, and a flag rather than a status because the
    /// lifecycle in `docs/TEMPLE-LIFECYCLE.md` is not a second state machine —
    /// its five states are already spelled by the three fields together:
    ///
    /// | cycle state | this flag | [`Self::status`] | [`Self::advice`] |
    /// |---|---|---|---|
    /// | `idle` | false | anything | `None` |
    /// | `waiting` | **true** | not a board status, or a board that predates the new start line | — |
    /// | `reading` / `read` | false, or **true** while the board still predates the start line | `Reading` / `Read` | — |
    /// | `playing` | false | `PanelNotVisible` or stood down | `Some` |
    ///
    /// Set by [`start_cycle`] on one of the three measured START phrases and
    /// cleared by [`end_cycle`] on any other Alva line or a zone change
    /// (`super::trigger`), by [`project`] when a read completes, by
    /// `super::run::apply_status` when the loop anchors or stops looking, and
    /// by [`force_off`]. The overlay's own gate is
    /// `view.ts::overlayShowsWaiting`, which also refuses to draw the notice
    /// over a board — a START heard with the sheet already open must not blink
    /// it.
    ///
    /// `serde(default)` so a payload from a build before POE-249 decodes as
    /// "not waiting" rather than failing the whole slice; the webview mirror
    /// defaults it the same way (`normaliseTemple`).
    #[serde(default)]
    pub waiting_for_panel: bool,
    pub layout: Option<LayoutView>,
    pub panel: Option<PanelView>,
    /// `None` whenever there is no decision to make — no board, or no current
    /// room.
    pub advice: Option<AdviceView>,
    /// Chase or Scarab, from the profile's own selector. `None` with no
    /// advice.
    pub mode: Option<String>,
    /// The two config flags in force, echoed so the overlay does not need a
    /// second command to render its own controls — the page renders the
    /// controls it owns from ONE source, and there is no getter command to ask
    /// a second time. Settings, not a reading: unlike [`Self::advice`] it
    /// survives [`force_off`], because a switched-off module's settings are
    /// still the settings the next read will use.
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
        // Both off the resolved LINE rather than off the built room: the sheet
        // grades families, so an offer that builds tier 2 still publishes the
        // family's letter and the tier-3 name that letter was given for. A
        // surface printing the grade beside a tier-2 room without the tier-3
        // name would be attributing the family's rating to the room in hand.
        grade: resolved.as_ref().map(|r| r.line.grade().as_str().to_string()),
        line_top: resolved.as_ref().and_then(|r| r.line.name(Tier::T3).map(|n| n.to_string())),
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
        secondary_door: advice.secondary_door.map(|edge| edge.to_string()),
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
        // A completed read is the proof the sheet was found: the wait is over
        // whether or not anything else noticed it end.
        waiting_for_panel: false,
        layout: Some(layout_view(read)),
        panel: Some(panel_view(read)),
        advice: read.advice.map(advice_view),
        mode: read.advice.map(|a| mode_label(a.mode)),
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
/// time it is read, or one of POE-249's retries — up to `super::run::RETRIES`
/// re-reads of the board on screen — would reshuffle two options separated by
/// less than the sampling noise, and the overlay would flicker between them
/// while the player is deciding.
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
        KEYS_IN_HAND,
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

/// Alva opened a portal: the module is now waiting for the temple sheet
/// (POE-249).
///
/// The one writer that sets [`TempleSlice::waiting_for_panel`], called from
/// `super::trigger::on_client_line` on one of the three measured START phrases.
/// It touches nothing else — the board a previous incursion left behind is
/// still what the Temple page has to draw, and the advice has its own end
/// (`super::trigger::advice_end`).
///
/// A module that cannot look does not wait: an [`TempleStatus::Unavailable`]
/// slice is left alone, because a START line reaching the watcher after
/// `super::run::unavailable` parked the loop would otherwise raise a notice no
/// tick can ever answer — that publish is the loop's last.
pub fn start_cycle(slice: &mut TempleSlice) {
    if slice.status == TempleStatus::Unavailable {
        return;
    }
    slice.waiting_for_panel = true;
}

/// The cycle is over: take the waiting notice down.
///
/// The one writer that clears the flag, so every exit says it the same way —
/// the sheet was found (`super::run::apply_status` on an anchored tick,
/// [`project`] on a completed read), the loop stopped looking (a stand-down or
/// a shutdown), Alva spoke again, or the zone changed. Idempotent, and the
/// caller compares before it calls when it wants a log line.
pub fn end_cycle(slice: &mut TempleSlice) {
    slice.waiting_for_panel = false;
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
/// What does NOT go is the settings echo (`config`, `profile`). Those
/// are not something the module read — they are what the user set, and the page
/// renders its own controls from them while the module is off (ADR-014: the
/// page reads the slice, never `ssot.modules`).
pub fn force_off(slice: &mut TempleSlice) {
    slice.status = TempleStatus::Off;
    // A notice that says the module is looking for the sheet, over a module
    // that is switched off, is the same lie the advice would be.
    end_cycle(slice);
    slice.advice = None;
    slice.mode = None;
    slice.read_notice = None;
    slice.last_error = None;
}

// ----------------------------------------------- one board, read and kept --

/// The two text regions `super::run::text_regions` crops, by name.
///
/// Declared here rather than spelled out at each use because [`unclean`] is the
/// second consumer: it is handed the names `super::run::clipped_text_rois`
/// reported as OUTSIDE the capture, and has to know which read failures each of
/// them explains. Two string literals in two files is a drift the compiler
/// cannot see — a renamed region would silently stop exempting anything.
pub const PANEL_REGION: &str = "panel";
/// The budget line's region — see [`PANEL_REGION`].
pub const REMAINING_REGION: &str = "remaining";

/// The board's SEMANTIC fingerprint: what the sheet says, with no reference to
/// where on screen it says it.
///
/// Three fields, all of them things only an incursion or a walk can change: the
/// room the player is standing in (`current`), the corridors the beam read open
/// (`doors`), and the ones it could not decide (`uncertain`). Cheap — it is
/// everything [`super::reader::read_layout`] produces WITHOUT OCR, so computing
/// it costs one anchor match and one beam-sampling pass and no OCR engine.
///
/// # Exactly what it does NOT cover, and why
///
/// `origin` and `scale` used to be hashed in here. They are not, because a hash
/// is the wrong shape for them: two frames of a sheet nobody touched can differ
/// by a pixel of anchor jitter, and a hash says only "different", which would
/// have made every such frame a new board. Those two moved into
/// [`BoardFrame`], which compares them with a TOLERANCE. This half stays exact,
/// because it is the half where a one-step difference is a real difference — a
/// corridor is open or it is not.
///
/// [`BoardFrame`] is the whole answer; this is its exact third.
pub fn layout_signature(layout: &TempleLayout) -> u64 {
    let mut hasher = DefaultHasher::new();
    layout.current.map(|s| s.index()).hash(&mut hasher);
    for edge in &layout.doors {
        edge.to_string().hash(&mut hasher);
    }
    for edge in &layout.uncertain {
        edge.to_string().hash(&mut hasher);
    }
    hasher.finish()
}

/// "Is this the same board as that one?", answered from pixels alone.
///
/// # Why it is not one hash (POE-249)
///
/// It was, and that had a defect nothing had measured: a hash answers only
/// "identical", and the anchor origin of a sheet nobody has touched is not
/// guaranteed identical across two frames — the plate is found by correlation
/// over a frame the game is still redrawing. A hash over `origin` therefore made
/// every jittered frame a NEW board, which for a clean board is one wasted
/// 28-call read per reopen and for an UNCLEAN one is worse: a new board restores
/// the retry budget, so a per-frame jitter would have re-read at the full
/// cadence for as long as the sheet was open.
///
/// So the comparison is split by what each field means:
///
/// - `semantic` ([`layout_signature`]) is EXACT. A corridor is open or closed;
///   the player is in one room or another. There is no "nearly".
/// - `origin` and `scale_milli` are compared with a BAND — see
///   [`Self::matches`]. Inside it the sheet has not moved, it has been re-found;
///   outside it the window was dragged or the UI scale changed, and every ROI
///   the read places from those numbers is somewhere else.
///
/// `scale` is carried as thousandths rather than as the `f32` so this type is
/// `Eq` and `Copy` and `super::run::LoopState` keeps its derives; the band is
/// two orders of magnitude wider than the rounding, so nothing is lost to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardFrame {
    /// The anchor origin this read was taken at, capture px.
    pub origin: (i32, i32),
    /// The anchor scale, in thousandths — `round(scale * 1000)`.
    pub scale_milli: u32,
    /// [`layout_signature`] of the same layout.
    pub semantic: u64,
}

/// How far the anchor origin may move before the sheet counts as MOVED, px.
///
/// # What two covers, in the terms this codebase actually quantifies
///
/// Not still-frame jitter — **nothing has measured the correlation peak's
/// per-frame stability on a still screen**, and that sentence is the honest
/// state of it. What IS quantified is the ROUTE the origin was found by, and the
/// two routes disagree by a known amount:
///
/// - The fast path, `super::anchor::detect_cheap`'s recheck, is a
///   full-resolution correlation over a fixed window at a fixed scale. Given the
///   same pixels it returns the same peak, so two frames of a still sheet that
///   both take this route differ by nothing. Two covers it exactly.
/// - The sweep path budgets more. `super::anchor`'s `SWEEP_FINE_RADIUS` is
///   `FINE_RADIUS + 4`, the four being the position error a coarse scale step
///   buys, and the measurement beside it is that the nominee at scale 1.00 sits
///   **2 px** from the true top-left. So two covers the measured case and half
///   the budgeted one.
///
/// A tick that flips route — recheck fails for one frame, the fallback chain
/// answers, then recheck resumes — can therefore land outside the band on a
/// board nothing touched. That case is not free and is not silent: it costs one
/// read, with the RETRY budget carried rather than restored
/// (`super::run::LoopState::note_read`), and it is bounded by
/// [`super::run::GEOMETRY_READS_CAP`]. It never produces a wrong board.
///
/// What the band stops is the unbounded version of the same thing: at zero
/// tolerance every re-found origin is a new board with a fresh retry budget, so
/// an anchor that would not sit still paid 28 OCR calls every 650 ms.
pub const FRAME_ORIGIN_TOLERANCE: i32 = 2;

/// How far the anchor scale may move before the sheet counts as RESCALED, as a
/// fraction denominator: 100 is one per cent.
///
/// One per cent is `super::anchor::SCALE_STEP` at scale 1.0 — the resolution the
/// anchor search itself works at — so a difference under it is not a scale the
/// search could have distinguished, and a difference over it is a step the
/// search took deliberately.
pub const FRAME_SCALE_TOLERANCE_DENOM: u64 = 100;

impl BoardFrame {
    /// The frame one resolved layout was read at.
    pub fn of(layout: &TempleLayout) -> Self {
        Self {
            origin: layout.origin,
            // Saturating, per Rust's float-to-int `as`: a scale is a small
            // positive number here and a NaN or a negative would land on 0,
            // which compares as a frame nothing matches rather than as a wrap.
            scale_milli: (layout.scale * 1000.0).round() as u32,
            semantic: layout_signature(layout),
        }
    }

    /// Whether these two frames describe the same board CONTENT — the exact
    /// third alone, with where it was drawn ignored.
    ///
    /// Named because two callers need this question rather than the whole one:
    /// [`Self::matches`] asks it first, and `super::run::LoopState::note_read`
    /// asks it ALONE, to tell a board that changed from a board that only moved.
    pub fn same_content(&self, other: &Self) -> bool {
        self.semantic == other.semantic
    }

    /// Whether these two frames are the same board in the same place.
    ///
    /// The exact third and the two banded thirds, all of which must hold.
    ///
    /// **Symmetric**, and that is load-bearing rather than incidental: the two
    /// call orders are opposite — `super::run`'s gate asks `recorded.matches(&
    /// fresh)` and [`merge_reads`] asks `fresh.matches(&kept)` — so an
    /// asymmetric predicate would let the gate admit a retry the merge then
    /// discarded. Both bands are symmetric by construction: `abs_diff` is, and
    /// the scale band is taken against the LARGER of the two rather than against
    /// `self`. The property is argued from that `max`, not tested — at the
    /// scales the anchor produces (about 850-1200 thousandths) the two
    /// directions cannot be made to disagree by any input a test could write.
    pub fn matches(&self, other: &Self) -> bool {
        if !self.same_content(other) {
            return false;
        }
        let moved = self.origin.0.abs_diff(other.origin.0) > FRAME_ORIGIN_TOLERANCE as u32
            || self.origin.1.abs_diff(other.origin.1) > FRAME_ORIGIN_TOLERANCE as u32;
        if moved {
            return false;
        }
        // Multiplied out rather than divided. At the scales the anchor produces
        // the two forms agree — a divide truncates 990/100 to 9 against a band
        // of 9.9, which no test here can tell apart — so this is the form that
        // stays right if a much smaller `scale_milli` ever reaches it, not a
        // form any test pins.
        let drift = u64::from(self.scale_milli.abs_diff(other.scale_milli));
        let larger = u64::from(self.scale_milli.max(other.scale_milli));
        drift * FRAME_SCALE_TOLERANCE_DENOM <= larger
    }
}

/// One completed read of one board, kept so a retry can fill in what it missed.
///
/// The five things a read produces that a LATER read of the same board could
/// improve on — everything [`ReadResult`] is built from except the settings
/// echo and the timestamps, which belong to the tick rather than to the board.
/// Owned rather than borrowed because it outlives the tick that read it: it
/// lives on `super::run::Session`, which is the loop's own state, and the
/// projection is rebuilt from it on every retry.
///
/// It is deliberately NOT on `super::run::LoopState`, which derives `Eq` and is
/// the loop's cheap bookkeeping. `super::run::BoardRead` is the bookkeeping half
/// — a key, a status, a retry budget — and this is the payload.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptRead {
    pub layout: TempleLayout,
    pub rooms: Vec<RoomReading>,
    pub panel: PanelReading,
    /// The door set the diamond settled, or `None` on the marker fallback.
    pub settled: Option<BTreeSet<Edge>>,
    /// Why the diamond read failed, when it did.
    pub marker_error: Option<String>,
}

/// Fold a retry into the read it is retrying, region by region.
///
/// POE-249 row 2: a read whose regions did not all come out clean is re-taken
/// at most [`super::run::RETRIES`] more times, and each re-take is a WHOLE read
/// — the anchor moved by a pixel, the crops are new, and OCR is not
/// deterministic across two frames of a game that is still drawing. So a retry
/// can be WORSE than the read it followed, region by region, and merging is
/// what stops the second attempt at a board undoing the first one's answers.
///
/// The rule is the same everywhere: a region the fresh read RESOLVED wins,
/// because it is the newer look at the same panel; a region it did not resolve
/// falls back to the kept read's answer. Nothing is invented and nothing that
/// was known is ever replaced by an unknown.
///
/// # The precondition, and why the layout is the thing it tests
///
/// [`BoardFrame::of(&fresh.layout).matches(&BoardFrame::of(&kept.layout))`](BoardFrame::matches),
/// or the fresh read is returned ALONE and the kept one is dropped. The frame
/// covers the room the player is standing in, both corridor sets, the anchor
/// origin and the scale, which is exactly the set that makes two reads
/// comparable: the settled door markers are indexed off `current`, and the plate
/// crops are placed from `origin`/`scale`. A merge across a moved board would
/// file the old room's corridors under the new room's name.
///
/// **The same predicate the read gate uses** (`super::run::LoopState::same_board`),
/// and deliberately the same one: a retry the gate let through as "the same
/// board" that this then refused to merge would spend a retry and throw away
/// what it bought. The two used to be different tests for the same question.
///
/// Which makes the guard below REDUNDANT BY DESIGN on the production path:
/// `super::run::kept_for` applies the same predicate to the same two frames
/// before it hands anything here, so nothing this loop calls can reach the
/// `return fresh` arm. It stays because this is a pure function that a caller
/// may hand any two reads — its own precondition is not something to inherit
/// from one caller's discipline — and because the redundancy is the point: a
/// future loosening of "same board" is then visibly a TWO-place edit, and a
/// one-place loosening fails this file's tests rather than shipping.
///
/// So the tolerance is here too. Inside [`FRAME_ORIGIN_TOLERANCE`] px and
/// [`FRAME_SCALE_TOLERANCE_DENOM`]'s one per cent, a re-anchored sheet is the
/// same board and the retry merges into what the first read got; beyond it, the
/// window was dragged or the UI was rescaled and the fresh read stands alone,
/// which it must, because the merged read takes the FRESH layout and every ROI
/// the slice publishes is placed from it.
///
/// # The one region that is taken wholesale
///
/// The architect blocks, when the two reads printed different NUMBERS of them.
/// The panel always draws two ([`super::panel::ARCHITECTS_PER_PANEL`], measured
/// on every reference board), so a read that found one found half a panel —
/// and a per-index merge over lists of different lengths would pair block 0 of
/// a one-block read against block 0 of a two-block read, which are not
/// necessarily the same offer (POE-243 sorts them by where they were drawn).
/// The longer read is the one that saw the whole panel, so it is taken whole,
/// rects included.
pub fn merge_reads(kept: &KeptRead, fresh: KeptRead) -> KeptRead {
    if !BoardFrame::of(&fresh.layout).matches(&BoardFrame::of(&kept.layout)) {
        return fresh;
    }
    let KeptRead {
        layout,
        rooms: fresh_rooms,
        panel: fresh_panel,
        settled: fresh_settled,
        marker_error: fresh_marker_error,
    } = fresh;
    let PanelReading {
        room,
        room_rect,
        architects,
        incursions_remaining,
    } = fresh_panel;

    // Per SLOT rather than per position: `read_board` returns one reading per
    // plate, and matching on the slot is what keeps a short or reordered
    // vector from filing one plate's identity under another's.
    let rooms = fresh_rooms
        .into_iter()
        .map(|reading| {
            if reading.identity.is_known() {
                return reading;
            }
            match kept.rooms.iter().find(|k| k.slot == reading.slot) {
                Some(known) if known.identity.is_known() => known.clone(),
                _ => reading,
            }
        })
        .collect();

    // The title and its box move together: the rect is where THAT reading found
    // the name, so a fresh rect under a kept identity would point the overlay at
    // a line it is not describing (POE-243).
    let (room, room_rect) = if room.identity().is_some() {
        (room, room_rect)
    } else {
        (kept.panel.room, kept.panel.room_rect)
    };

    let architects = if architects.len() == kept.panel.architects.len() {
        architects
            .into_iter()
            .zip(kept.panel.architects.iter())
            .map(|(fresh_offer, kept_offer)| {
                if fresh_offer.target.identity().is_some() {
                    fresh_offer
                } else {
                    kept_offer.clone()
                }
            })
            .collect()
    } else if architects.len() > kept.panel.architects.len() {
        architects
    } else {
        kept.panel.architects.clone()
    };

    // A fresh marker error is the whole diamond failing, not one seal: the read
    // has no door set at all, so the kept one's set AND the reason it might
    // itself carry are what the projection still has to work from.
    let (settled, marker_error) = if fresh_marker_error.is_none() {
        (fresh_settled, None)
    } else {
        (kept.settled.clone(), kept.marker_error.clone())
    };

    KeptRead {
        // The FRESH anchor, always: every ROI the slice publishes is placed
        // from it, and the retry is the capture the player is looking at.
        layout,
        rooms,
        panel: PanelReading {
            room,
            room_rect,
            architects,
            incursions_remaining: incursions_remaining.or(kept.panel.incursions_remaining),
        },
        settled,
        marker_error,
    }
}

/// Whether this read is worth spending a retry on — POE-249 row 2's four
/// causes, minus the ones a clipped crop explains.
///
/// `clipped` is the region names `super::run::clipped_text_rois` reported as
/// falling entirely OUTSIDE the capture this read was taken from. A region that
/// is not on screen cannot read better next time, so the failures it explains
/// are not counted: with [`REMAINING_REGION`] outside, the budget is missing
/// because the crop was empty, and with [`PANEL_REGION`] outside, so are the
/// architect blocks. Without this a windowed client whose panel crop has walked
/// off the monitor would pay three full reads per board forever and publish the
/// same empty panel every time.
///
/// The plates and the door diamond have no such exemption: they are placed off
/// the anchor inside the board itself, so a plate that did not read is a plate
/// OCR could not name, which is exactly what a retry is for.
pub fn unclean(read: &KeptRead, clipped: &[&'static str]) -> bool {
    if read.rooms.iter().any(|reading| !reading.identity.is_known()) {
        return true;
    }
    if read.marker_error.is_some() {
        return true;
    }
    if !clipped.contains(&REMAINING_REGION) && read.panel.incursions_remaining.is_none() {
        return true;
    }
    if !clipped.contains(&PANEL_REGION) {
        if read.panel.architects.len() < ARCHITECTS_PER_PANEL {
            return true;
        }
        if read
            .panel
            .architects
            .iter()
            .any(|offer| offer.target.identity().is_none())
        {
            return true;
        }
    }
    false
}

/// The user's Re-arm button, as a counter the loop spends once.
///
/// It was `ReadGate`, and carried the whole "is what I am looking at the thing I
/// already read?" question: two fingerprints, a panel-lost reset, and this. All
/// of that moved to the board key in POE-249 — `super::run::LoopState::board`,
/// keyed on `(temple_epoch, temple_rearm)` — and what is left is the one input
/// that is not derived from the screen at all.
///
/// The rearm counter is HALF that key, so a bump invalidates the board on its
/// own. This exists for the other half of the button's contract: a re-arm
/// pressed while nothing is anchored must still force an anchor ATTEMPT, and
/// nothing on an empty screen produces a key to compare. See
/// `super::run::wants_full_read`, which peeks here and spends the bump on the
/// tick it promoted for.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RearmGate {
    rearm_seen: u64,
}

impl RearmGate {
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
    /// **The detect tick must call this on the tick it promoted for a bump**, or
    /// a re-arm pressed while no panel is on screen stays pending forever and
    /// pins the loop into an anchor attempt on every tick — the exact cost the
    /// detect tick exists to remove, re-entered through the settings commands,
    /// which re-arm on every change.
    pub fn note_rearm(&mut self, rearm: u64) {
        self.rearm_seen = rearm;
    }
}

// ------------------------------------------------- fixtures, shared -------
//
// At module level and `pub(crate)` rather than inside this file's `mod tests`,
// because `super::run`'s tests build boards too: `run::kept_for` decides what
// happens to a whole [`KeptRead`], and a second constructor over there would be
// a second answer to "what does a read look like?" for the two files that have
// to agree about it. This file's own helpers build on these.

/// The origin and scale [`fixture_layout`] builds its board at — a plausible
/// anchored Entrance on a 1374-wide capture. Named so a test can rebuild the
/// same lattice without copying two numbers out of the helper.
#[cfg(test)]
pub(crate) const FIXTURE_ORIGIN: (i32, i32) = (673, 494);
/// See [`FIXTURE_ORIGIN`].
#[cfg(test)]
pub(crate) const FIXTURE_SCALE: f32 = 0.99;

#[cfg(test)]
pub(crate) fn fixture_calibration() -> AnchorCalibration {
    AnchorCalibration {
        screen_w: 1374,
        screen_h: 773,
        scale: 0.99,
    }
}

/// A layout with the given current room and door sets, anchored at
/// [`FIXTURE_ORIGIN`]; every other field is a plausible constant, because
/// nothing reading this cares about them.
#[cfg(test)]
pub(crate) fn fixture_layout(
    current: Option<Slot>,
    doors: &[(Slot, Slot)],
    uncertain: &[(Slot, Slot)],
) -> TempleLayout {
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
        thresholds: super::doors::Thresholds { horizontal: 0.20, diagonal: 0.20 },
        calibration: fixture_calibration(),
    }
}

/// One [`KeptRead`] over `layout`: no plate named, no title, no offer, no
/// diamond. The payload a test can watch a merge or a DROP move around without
/// any of it standing in for the decision being tested.
#[cfg(test)]
pub(crate) fn fixture_read(layout: TempleLayout) -> KeptRead {
    KeptRead {
        layout,
        rooms: Vec::new(),
        panel: PanelReading {
            room: Match::Unknown,
            room_rect: None,
            architects: Vec::new(),
            incursions_remaining: None,
        },
        settled: None,
        marker_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The shared fixtures above, under the short names this file's 40-odd call
    // sites already read at.
    use super::fixture_layout as layout;
    use crate::temple::advisor::rules::ArchitectChoice;
    use crate::temple::lattice::{Lattice, Slot};
    use crate::temple::rooms::{match_room_name, resolve_name};

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

        assert!(advise_read(&layout, &rooms, &panel, None, &TempleSettings::default()).is_none());
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
        let settings = TempleSettings::default();

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
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::default()).expect("ranks");

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
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::default()).expect("ranks");

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

    /// The conditional door reaches the slice as its OWN field, beside the top
    /// recommendation and never inside it (POE-248).
    ///
    /// The projection is the whole subject: `advisor::conditional_second_door`
    /// decides what the answer is, and this asserts that what it decided
    /// arrives on the wire as the edge label a surface can compare against
    /// `SealView.edge`, and that it did not get folded into `doors` — which
    /// with one key in hand would be an instruction to spend a key the player
    /// does not have.
    #[test]
    fn the_conditional_second_door_is_projected_beside_the_top_recommendation() {
        // Every corridor out of C1 closed, so there is a second one to buy.
        let layout = layout(Some(Slot::C1), &[], &[]);
        let rooms = board_rooms(&[(Slot::C1, "Chasm")]);
        let panel = panel(
            "Chasm",
            Some(6),
            vec![
                offer("Quipolatl", "Armoury", OfferKind::Upgrade),
                offer("Tacati", "Storage Room", OfferKind::Change),
            ],
        );
        let advice =
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::default()).expect("ranks");
        let door = advice
            .secondary_door
            .expect("precondition: an all-closed room has a second corridor to buy")
            .to_string();

        let view = advice_view(&advice);

        assert_eq!(view.secondary_door.as_deref(), Some(door.as_str()));
        assert!(
            !view.recommendations[0].doors.contains(&door),
            "the conditional door must not be one of the doors to open now: {:?}",
            view.recommendations[0].doors
        );
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
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::default()).expect("ranks");
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
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::default()).expect("ranks");
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
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::default()).expect("ranks");

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
            advise_read(&layout, &rooms, &panel, None, &TempleSettings::default()).expect("ranks");

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

    /// A resolved offer publishes Vertolka's letter for the LINE it builds into
    /// and the tier-3 room that letter was given for (POE-249) — the two halves
    /// of the rating line the offer boxes print.
    ///
    /// The corruption line is asserted literally because a grade read off
    /// `LINES` at test time would pass against any mapping at all, including
    /// none: `A++` is what the sheet says about Locus of Corruption, and it is
    /// the one letter a wrong lookup could not produce by accident.
    #[test]
    fn a_resolved_offer_publishes_its_line_grade_and_the_tier_three_room() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        // Tier 2 in hand, so Contested Development's +1 lands the kill on the
        // line's own tier-3 room.
        let rooms = board_rooms(&[(Slot::B0, "Catalyst of Corruption")]);
        let panel = panel(
            "Catalyst of Corruption",
            Some(6),
            vec![offer("Guatelitzi", "Corruption Chamber", OfferKind::Change)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        let first = slice.panel.expect("panel").offers.into_iter().next().expect("one block");
        assert_eq!(first.grade.as_deref(), Some("A++"));
        assert_eq!(first.line_top.as_deref(), Some("Locus of Corruption"));
        assert_eq!(
            first.built_tier,
            Some(3),
            "precondition: this kill lands on the tier the grade is about",
        );
    }

    /// An offer whose printed target is not in the vocabulary publishes NO
    /// rating rather than a default one. There is no line, so there is nothing
    /// graded; a letter here would be a rating invented for a room the app
    /// could not name.
    #[test]
    fn an_unresolvable_offer_publishes_no_grade_and_no_line_top() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Chasm")]);
        let panel = panel(
            "Chasm",
            Some(6),
            vec![offer("Guatelitzi", "Qwertz Chamber", OfferKind::Upgrade)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        let first = slice.panel.expect("panel").offers.into_iter().next().expect("one block");
        assert_eq!(first.grade, None);
        assert_eq!(first.line_top, None);
    }

    /// The rating is about the LINE, not about the room the kill hands over.
    /// A `change` taken on a tier-1 room builds tier 2, and the published
    /// `line_top` is still the family's tier-3 room — which is what makes the
    /// letter beside it honest. Fails if `offer_view` ever names
    /// `display_name`'s own tier here.
    #[test]
    fn a_change_resolved_at_tier_two_still_names_the_lines_tier_three_room() {
        let layout = layout(Some(Slot::B0), &[], &[]);
        let rooms = board_rooms(&[(Slot::B0, "Corruption Chamber")]);
        let panel = panel(
            "Corruption Chamber",
            Some(6),
            vec![offer("Guatelitzi", "Corruption Chamber", OfferKind::Change)],
        );

        let slice = project(&read(&layout, &rooms, &panel, None, None), None);

        let first = slice.panel.expect("panel").offers.into_iter().next().expect("one block");
        assert_eq!(first.display_name.as_deref(), Some("Catalyst of Corruption"));
        assert_eq!(first.built_tier, Some(2));
        assert_eq!(first.line_top.as_deref(), Some("Locus of Corruption"));
        assert_eq!(first.grade.as_deref(), Some("A++"));
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

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::default())
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

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::default())
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

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::default())
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

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::default())
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

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::default())
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

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::default())
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

        let advice = advise_read(&layout, &rooms, &panel, None, &TempleSettings::default())
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

    // ------------------------------------- the merge precondition (layout) --

    /// The semantic fingerprint ignores the two fields that move between two
    /// captures of a board nobody touched: `ncc` is a correlation score against
    /// a frame the game redraws, and `confidence` is derived from it.
    ///
    /// This is what makes both consumers usable at all. [`merge_reads`]'s
    /// precondition has to say YES on the ordinary retry, or a second look at an
    /// unclean board would drop the first one's answers instead of filling its
    /// gaps; and `super::run::LoopState::same_board` has to say YES on the
    /// ordinary reopen, or every reopen would pay 28 OCR calls.
    ///
    /// Fails if it folds in either field — the mutation is one line,
    /// `layout.ncc.to_bits().hash(&mut hasher)`.
    #[test]
    fn a_rescored_read_of_an_unmoved_board_hashes_the_same() {
        let before = layout(Some(Slot::B0), &[(Slot::B0, Slot::C1)], &[]);
        let mut rescored = layout(Some(Slot::B0), &[(Slot::B0, Slot::C1)], &[]);
        rescored.ncc = 0.87;
        rescored.confidence = Confidence::Low;

        assert_eq!(layout_signature(&before), layout_signature(&rescored));
    }

    /// A door that opened moves the fingerprint, so a retry taken after it
    /// cannot be merged into the read before it. Fails if `layout_signature`
    /// skips `doors`.
    #[test]
    fn an_opened_corridor_moves_the_layout_signature() {
        let before = layout(Some(Slot::B0), &[], &[]);
        let after = layout(Some(Slot::B0), &[(Slot::B0, Slot::C1)], &[]);

        assert_ne!(layout_signature(&before), layout_signature(&after));
    }

    /// Moving to another room moves it too — and this is the one the settled
    /// door markers depend on, because they are indexed off `current`. Fails if
    /// `layout_signature` skips `current`.
    #[test]
    fn a_new_current_room_moves_the_layout_signature() {
        let before = layout(Some(Slot::B0), &[], &[]);
        let after = layout(Some(Slot::C1), &[], &[]);

        assert_ne!(layout_signature(&before), layout_signature(&after));
    }

    // ------------------------------------------------ merging a retry read --

    /// A read of the fixture board: `named` plates, `title`/`remaining`/`offers`
    /// on the panel, a settled door set and no marker error.
    fn kept_read(
        named: &[(Slot, &str)],
        title: &str,
        remaining: Option<u8>,
        offers: Vec<ArchitectOffer>,
    ) -> KeptRead {
        KeptRead {
            rooms: board_rooms(named),
            panel: panel(title, remaining, offers),
            settled: Some(BTreeSet::from([Edge::new(Slot::B0, Slot::C1)])),
            ..fixture_read(layout(Some(Slot::B0), &[(Slot::B0, Slot::C1)], &[]))
        }
    }

    /// Every plate named, both offers resolved, a budget and a settled diamond —
    /// the read [`unclean`] must call clean.
    fn clean_read() -> KeptRead {
        kept_read(
            &Slot::ALL.map(|slot| (slot, "Chasm")),
            "Chasm",
            Some(6),
            vec![
                offer("Guatelitzi", "Corruption Chamber", OfferKind::Change),
                offer("Ticaba", "Storage Room", OfferKind::Upgrade),
            ],
        )
    }

    /// A plate the retry could not name keeps the identity the first read got.
    /// Fails if the merge takes the fresh plates wholesale — the board would
    /// lose a room the player had already been shown.
    #[test]
    fn a_known_plate_is_not_replaced_by_an_unknown_retry() {
        let kept = kept_read(&[(Slot::B0, "Chasm")], "Chasm", Some(6), Vec::new());
        let fresh = kept_read(&[], "Chasm", Some(6), Vec::new());

        let merged = merge_reads(&kept, fresh);

        assert_eq!(
            merged.rooms[Slot::B0.index()].identity.identity().map(|id| id.display_name()),
            Some("Chasm"),
        );
    }

    /// …and the other direction: a plate the first read missed is filled in by
    /// the retry, which is the whole reason the retry runs. Fails if the merge
    /// prefers the kept plate unconditionally.
    #[test]
    fn an_unknown_plate_is_filled_in_by_a_retry_that_read_it() {
        let kept = kept_read(&[], "Chasm", Some(6), Vec::new());
        let fresh = kept_read(&[(Slot::B0, "Chasm")], "Chasm", Some(6), Vec::new());

        let merged = merge_reads(&kept, fresh);

        assert_eq!(
            merged.rooms[Slot::B0.index()].identity.identity().map(|id| id.display_name()),
            Some("Chasm"),
        );
    }

    /// An offer the retry could not resolve keeps the resolved one at the same
    /// index — and the RECT comes with it, because a box from one read under an
    /// identity from another points the overlay at a line it is not describing.
    ///
    /// Fails if the merge takes the fresh block whatever it resolved to.
    #[test]
    fn a_resolved_offer_is_not_replaced_by_an_unresolved_one_at_the_same_index() {
        let mut resolved = offer("Guatelitzi", "Corruption Chamber", OfferKind::Change);
        resolved.rect = Some([10, 20, 30, 40]);
        let mut unread = offer("Guatelitzi", "qqqq zzzz", OfferKind::Change);
        unread.rect = Some([99, 99, 9, 9]);
        let kept = kept_read(&[], "Chasm", Some(6), vec![resolved, offer("Ticaba", "Storage Room", OfferKind::Upgrade)]);
        let fresh = kept_read(&[], "Chasm", Some(6), vec![unread, offer("Ticaba", "Storage Room", OfferKind::Upgrade)]);

        let merged = merge_reads(&kept, fresh);

        assert_eq!(merged.panel.architects[0].printed_target, "Corruption Chamber");
        assert_eq!(
            merged.panel.architects[0].rect,
            Some([10, 20, 30, 40]),
            "the rect travels with the block it belongs to",
        );
    }

    /// The panel always prints two blocks, so a retry that found one found half
    /// a panel — the merge keeps the two-block read whole rather than pairing
    /// block 0 of one against block 0 of the other.
    ///
    /// Fails if the unequal-length case falls through to the per-index merge:
    /// POE-243 sorts blocks by where they were drawn, so a one-block read's
    /// block 0 is not necessarily the same offer.
    #[test]
    fn a_one_offer_retry_does_not_shorten_a_two_offer_read() {
        let kept = kept_read(
            &[],
            "Chasm",
            Some(6),
            vec![
                offer("Guatelitzi", "Corruption Chamber", OfferKind::Change),
                offer("Ticaba", "Storage Room", OfferKind::Upgrade),
            ],
        );
        let fresh = kept_read(&[], "Chasm", Some(6), vec![offer("Ticaba", "Storage Room", OfferKind::Upgrade)]);

        let merged = merge_reads(&kept, fresh);

        assert_eq!(merged.panel.architects.len(), 2);
        assert_eq!(merged.panel.architects[0].architect_name, "Guatelitzi");
    }

    /// And the same rule the other way up: a retry that finally read BOTH blocks
    /// replaces a one-block read wholesale. Fails if the merge keeps the longer
    /// of the two by accident of ordering rather than by length.
    #[test]
    fn a_two_offer_retry_replaces_a_one_offer_read_wholesale() {
        let kept = kept_read(&[], "Chasm", Some(6), vec![offer("Ticaba", "Storage Room", OfferKind::Upgrade)]);
        let fresh = kept_read(
            &[],
            "Chasm",
            Some(6),
            vec![
                offer("Guatelitzi", "Corruption Chamber", OfferKind::Change),
                offer("Ticaba", "Storage Room", OfferKind::Upgrade),
            ],
        );

        let merged = merge_reads(&kept, fresh);

        assert_eq!(merged.panel.architects.len(), 2);
        assert_eq!(merged.panel.architects[0].architect_name, "Guatelitzi");
    }

    /// The budget is a single number the footer either printed or did not, so
    /// whichever read got it wins — in both directions. Fails if the merge
    /// takes the fresh value unconditionally (a retry that lost the footer would
    /// blank a budget the advisor scores every rollout against) or the kept one
    /// unconditionally (the first read's `None` would never be filled).
    #[test]
    fn a_budget_that_either_read_got_survives_the_merge() {
        let with_budget = kept_read(&[], "Chasm", Some(6), Vec::new());
        let without = kept_read(&[], "Chasm", None, Vec::new());

        assert_eq!(
            merge_reads(&with_budget, without.clone()).panel.incursions_remaining,
            Some(6),
            "a retry that lost the footer keeps the budget",
        );
        assert_eq!(
            merge_reads(&without, with_budget).panel.incursions_remaining,
            Some(6),
            "and a retry that found it fills one in",
        );
    }

    /// The title follows the same rule as the plates: a retry that could not
    /// read it keeps the name the first read got, and the title's own rect goes
    /// with it. Fails if `room`/`room_rect` are taken from different reads.
    #[test]
    fn an_unread_title_keeps_the_name_and_the_rect_of_the_read_that_got_it() {
        let mut kept = kept_read(&[], "Chasm", Some(6), Vec::new());
        kept.panel.room_rect = Some([1, 2, 3, 4]);
        let mut fresh = kept_read(&[], "qqqq zzzz", Some(6), Vec::new());
        fresh.panel.room_rect = Some([50, 60, 70, 80]);

        let merged = merge_reads(&kept, fresh);

        assert_eq!(merged.panel.room.identity().map(|id| id.display_name()), Some("Chasm"));
        assert_eq!(merged.panel.room_rect, Some([1, 2, 3, 4]));
    }

    /// A diamond the retry read replaces the one before it — a newer look at the
    /// same corridors is the better answer. Fails if the kept door set is
    /// preferred.
    #[test]
    fn a_retry_that_read_the_diamond_replaces_the_settled_doors() {
        let mut kept = kept_read(&[], "Chasm", Some(6), Vec::new());
        kept.settled = Some(BTreeSet::from([Edge::new(Slot::B0, Slot::C0)]));
        let mut fresh = kept_read(&[], "Chasm", Some(6), Vec::new());
        fresh.settled = Some(BTreeSet::from([Edge::new(Slot::B0, Slot::C1)]));

        let merged = merge_reads(&kept, fresh);

        assert_eq!(merged.settled, Some(BTreeSet::from([Edge::new(Slot::B0, Slot::C1)])));
        assert_eq!(merged.marker_error, None);
    }

    /// A retry whose diamond FAILED keeps the door set the first read settled,
    /// and the reason that read carried — a marker error is the whole diamond
    /// failing, not one seal, so there is nothing partial to take from it.
    ///
    /// Fails if the fresh `settled`/`marker_error` are taken unconditionally:
    /// the room widget would lose its corridors on a retry the read was supposed
    /// to improve.
    #[test]
    fn a_failed_retry_diamond_keeps_the_doors_the_first_read_settled() {
        let kept = kept_read(&[], "Chasm", Some(6), Vec::new());
        let mut fresh = kept_read(&[], "Chasm", Some(6), Vec::new());
        fresh.settled = None;
        fresh.marker_error = Some("read 3 door markers for a 4-neighbour room".to_string());

        let merged = merge_reads(&kept, fresh);

        assert_eq!(merged.settled, Some(BTreeSet::from([Edge::new(Slot::B0, Slot::C1)])));
        assert_eq!(merged.marker_error, None, "the kept read had no error to carry");
    }

    /// BOTH diamonds failed, so the merged read has no door set — and the kept
    /// read's REASON is what it carries out, because that is the only
    /// explanation either read produced.
    ///
    /// The `marker_error` is not decoration: `unclean` reads it, so a merged
    /// read with no doors and no error is a board the retry budget stops
    /// improving with nothing on the page saying why. Fails on the mutation
    /// `(kept.settled.clone(), None)` — the fallback arm dropping the kept
    /// reason while keeping the kept (absent) doors.
    #[test]
    fn two_failed_diamonds_carry_the_kept_reads_reason_out() {
        // Otherwise CLEAN on both sides, so the `unclean` assertion below can
        // only be answered by the marker error the merge carried out.
        let mut kept = clean_read();
        kept.settled = None;
        kept.marker_error = Some("the diamond rect fell outside the capture".to_string());
        let mut fresh = clean_read();
        fresh.settled = None;
        fresh.marker_error = Some("read 3 door markers for a 4-neighbour room".to_string());

        let merged = merge_reads(&kept, fresh);

        assert_eq!(merged.settled, None);
        assert_eq!(
            merged.marker_error.as_deref(),
            Some("the diamond rect fell outside the capture"),
        );
        assert!(unclean(&merged, &[]), "a board with no doors and no reason is not clean");
    }

    /// The precondition. A board whose frame moved is a DIFFERENT board — the
    /// player walked through a door — so the fresh read stands alone and the
    /// kept one is dropped.
    ///
    /// Fails if the merge runs anyway: the old room's settled corridors would be
    /// published under the new room's name, and a plate the player has already
    /// left would fill in one they are looking at.
    #[test]
    fn a_read_of_a_moved_board_is_returned_unmerged() {
        let mut kept = kept_read(&[(Slot::B0, "Chasm")], "Chasm", Some(6), Vec::new());
        kept.layout = layout(Some(Slot::B0), &[], &[]);
        let mut fresh = kept_read(&[], "qqqq zzzz", None, Vec::new());
        fresh.layout = layout(Some(Slot::C1), &[], &[]);

        let merged = merge_reads(&kept, fresh.clone());

        assert_eq!(merged, fresh, "nothing of the old board may reach the new one");
    }

    /// The band, on the merge side. An anchor origin that landed one px away is
    /// the same sheet re-found, so the retry FOLDS into what the first read got
    /// rather than replacing it.
    ///
    /// This is the half that makes the tolerance worth having: at bit-exact
    /// origins a jittered retry threw away the plate the first read had already
    /// named, on a board the player was looking at. Fails if `merge_reads` stops
    /// using `BoardFrame::matches` — a bare `==` on the frames, or the old
    /// `layout_signature` comparison with origin folded back into it.
    #[test]
    fn a_one_pixel_wobble_still_merges_into_the_kept_read() {
        let kept = kept_read(&[(Slot::B0, "Chasm")], "Chasm", Some(6), Vec::new());
        let mut fresh = kept_read(&[], "Chasm", Some(6), Vec::new());
        fresh.layout.origin = (kept.layout.origin.0 + 1, kept.layout.origin.1);

        let merged = merge_reads(&kept, fresh);

        assert_eq!(
            merged.rooms[Slot::B0.index()].identity.identity().map(|id| id.display_name()),
            Some("Chasm"),
        );
    }

    /// …and past the band it does not. The window was dragged, every ROI is
    /// somewhere else, and the fresh read is the only one that describes the
    /// frame the player is looking at.
    ///
    /// Fails if the tolerance is widened past `FRAME_ORIGIN_TOLERANCE`, or
    /// dropped entirely: a plate read at the old coordinates would be published
    /// as a plate at the new ones.
    #[test]
    fn a_move_past_the_tolerance_returns_the_fresh_read_unmerged() {
        let kept = kept_read(&[(Slot::B0, "Chasm")], "Chasm", Some(6), Vec::new());
        let mut fresh = kept_read(&[], "Chasm", Some(6), Vec::new());
        fresh.layout.origin = (
            kept.layout.origin.0 + FRAME_ORIGIN_TOLERANCE + 1,
            kept.layout.origin.1,
        );

        let merged = merge_reads(&kept, fresh.clone());

        assert_eq!(merged, fresh, "nothing of the old frame may reach the new one");
    }

    // ---------------------------------------- what is worth a retry at all --

    /// The clean case: every plate named, both offers resolved, a budget and a
    /// diamond. Fails if any cause is evaluated inverted — the loop would then
    /// spend its whole retry budget on every board.
    #[test]
    fn a_read_with_nothing_missing_is_clean() {
        assert!(!unclean(&clean_read(), &[]));
    }

    /// An unread plate is worth a retry: the crop is inside the board, so a
    /// redraw can fix it. Fails if the plates are left out of the check.
    #[test]
    fn an_unread_plate_is_worth_a_retry() {
        let mut read = clean_read();
        read.rooms[Slot::C1.index()].identity = Match::Unknown;

        assert!(unclean(&read, &[]));
    }

    /// A panel that printed only one architect block is a partial read of a
    /// panel that always prints two. Fails if the arity is left out.
    #[test]
    fn a_panel_with_one_architect_block_is_worth_a_retry() {
        let mut read = clean_read();
        read.panel.architects.truncate(1);

        assert!(unclean(&read, &[]));
    }

    /// Both blocks read, but one names a room the vocabulary does not have —
    /// the offer is printed and unusable. Fails if the check counts blocks
    /// without looking at what they resolved to.
    #[test]
    fn an_offer_that_did_not_resolve_is_worth_a_retry() {
        let mut read = clean_read();
        read.panel.architects[1] = offer("Ticaba", "qqqq zzzz", OfferKind::Upgrade);

        assert!(unclean(&read, &[]));
    }

    /// A diamond that failed to read is worth a retry — the seals are drawn
    /// inside the board and a selection frame moves. Fails if `marker_error` is
    /// left out.
    #[test]
    fn a_failed_diamond_read_is_worth_a_retry() {
        let mut read = clean_read();
        read.marker_error = Some("read 3 door markers for a 4-neighbour room".to_string());

        assert!(unclean(&read, &[]));
    }

    /// An unread budget is worth a retry, because the advisor scores every
    /// rollout against it. Fails if `incursions_remaining` is left out.
    #[test]
    fn an_unread_budget_is_worth_a_retry() {
        let mut read = clean_read();
        read.panel.incursions_remaining = None;

        assert!(unclean(&read, &[]));
    }

    /// …unless the budget's own crop fell off the capture, which is a windowed
    /// client and not something a retry can fix. Fails if the exemption is
    /// missing: such a machine would pay three full reads for every board it
    /// ever sees and publish the same empty footer each time.
    #[test]
    fn a_budget_whose_crop_is_off_the_capture_is_not_worth_a_retry() {
        let mut read = clean_read();
        read.panel.incursions_remaining = None;

        assert!(!unclean(&read, &[REMAINING_REGION]));
    }

    /// The exemption is per REGION, not a blanket amnesty: the budget's crop
    /// being off the capture says nothing about the plates, which are read from
    /// the board itself.
    ///
    /// Fails if a clipped region short-circuits the whole check.
    #[test]
    fn a_clipped_budget_does_not_excuse_an_unread_plate() {
        let mut read = clean_read();
        read.rooms[Slot::C1.index()].identity = Match::Unknown;

        assert!(unclean(&read, &[REMAINING_REGION]));
    }

    /// The panel crop being off the capture explains BOTH offer causes at once —
    /// no blocks were read because there was nothing to read them from. Fails if
    /// the exemption covers only one of the two.
    #[test]
    fn offers_missing_because_the_panel_crop_is_off_the_capture_are_not_worth_a_retry() {
        let mut read = clean_read();
        read.panel.architects = Vec::new();

        assert!(!unclean(&read, &[PANEL_REGION]));
    }

    /// `rearm_pending` is a pure peek: it reports the bump and spends nothing.
    /// [`RearmGate::note_rearm`] is the single place the bump is spent, on the
    /// tick that promoted for it.
    ///
    /// Fails if `rearm_pending` consumes the bump itself — the tick that peeked
    /// would then be the only one to ever see it, and the anchor attempt the
    /// button exists to force would never happen.
    #[test]
    fn rearm_pending_peeks_and_note_rearm_is_what_spends_the_bump() {
        let mut gate = RearmGate::default();
        assert!(!gate.rearm_pending(0), "nothing pressed yet");

        assert!(gate.rearm_pending(1), "the bump is visible");
        assert!(gate.rearm_pending(1), "and peeking does not spend it");

        gate.note_rearm(1);

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
                secondary_door: None,
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
    /// `force_off` blanks it: the page renders the flags/profile controls
    /// from the slice alone, so a wipe here would show every control at its
    /// derive default while `settings.json` says otherwise, with the module off
    /// and no loop left to correct it.
    #[test]
    fn forcing_a_disabled_slice_off_keeps_the_settings_echo() {
        let profile = TempleProfileSettings { apex_score: 42.0, ..Default::default() };
        let config = TempleConfig { artefacts_of_the_vaal: false, scarab_of_timelines: true };
        let mut s = TempleSlice {
            status: TempleStatus::Read,
            config: config.clone(),
            profile: profile.clone(),
            ..TempleSlice::default()
        };

        force_off(&mut s);

        assert_eq!(s.config, config);
        assert_eq!(s.profile, profile);
    }

    // -------------------------------------------------------- the cycle --

    /// Alva opened a portal: the module is waiting for the temple sheet.
    /// Fails if the writer is inverted or does nothing — the notice would never
    /// appear, and the whole of POE-249's row 1 with it.
    #[test]
    fn starting_a_cycle_puts_the_module_in_the_waiting_state() {
        let mut s = TempleSlice::default();

        start_cycle(&mut s);

        assert!(s.waiting_for_panel);
    }

    /// A host with no capture or no OCR never gets a tick, so a START line
    /// there would raise a notice nothing could take down until Alva spoke
    /// again. The watcher keeps delivering lines after `super::run::unavailable`
    /// has parked the loop, and that park was the loop's last publish, so the
    /// refusal has to live here. Fails if the guard is dropped from
    /// [`start_cycle`].
    #[test]
    fn an_unavailable_module_does_not_start_waiting_for_a_panel() {
        let mut s = TempleSlice {
            status: TempleStatus::Unavailable,
            ..TempleSlice::default()
        };

        start_cycle(&mut s);

        assert!(!s.waiting_for_panel);
    }

    /// …and the cycle's end takes it down again. Fails if the two writers do
    /// the same thing, which would leave the notice up for the rest of the
    /// session after the first start line.
    #[test]
    fn ending_a_cycle_takes_the_waiting_state_off() {
        let mut s = TempleSlice { waiting_for_panel: true, ..TempleSlice::default() };

        end_cycle(&mut s);

        assert!(!s.waiting_for_panel);
    }

    /// A completed read is the proof the sheet was found, so the wait is over
    /// whatever else noticed it end.
    ///
    /// This is the belt to `run::apply_status`'s braces (a sighting clears it
    /// one step earlier, at `TickOutcome::Anchored`): `project` replaces the
    /// whole slice, so a field it did not set would be the derive default here
    /// anyway — what the test pins is that the default it lands on is the right
    /// one. Fails if the projection ever carries a live wait onto a board.
    #[test]
    fn a_completed_read_ends_the_wait_for_the_panel() {
        let layout = layout(Some(Slot::E1), &[], &[]);
        let rooms = board_rooms(&[(Slot::E1, "Chamber of Iron")]);
        let panel = panel("Chamber of Iron", Some(6), Vec::new());

        let s = project(&read(&layout, &rooms, &panel, None, None), None);

        assert!(!s.waiting_for_panel);
    }

    /// A switched-off module is not waiting for anything. Fails if `force_off`
    /// leaves the flag standing: the notice would float over the game with the
    /// module off, which is the same lie the advice drop exists to stop.
    #[test]
    fn forcing_a_disabled_slice_off_ends_the_wait_too() {
        let mut s = TempleSlice {
            status: TempleStatus::Idle,
            waiting_for_panel: true,
            ..TempleSlice::default()
        };

        force_off(&mut s);

        assert!(!s.waiting_for_panel);
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
            config: config.clone(),
            profile: profile.clone(),
            ..read(&layout, &rooms, &panel, None, None)
        };

        let published = project(&result, None);

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
            r#"{"status":"idle","waitingForPanel":false,"layout":null,"panel":null,"advice":null,"mode":null,"config":{"artefactsOfTheVaal":true,"scarabOfTimelines":false},"profile":{"apexScore":2.0,"pathCost":0.0,"rerollUntilFavourable":false,"r4KeepUpgradeTargets":true},"unknownRooms":[],"lastReadAt":null,"calibration":null,"readNotice":null,"lastError":null}"#,
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
            // `true` on purpose, like every other non-default value here: the
            // pin is a check on the wire SHAPE, and a `false` would pin only
            // the field's presence while the mirror's boolean has two branches.
            waiting_for_panel: true,
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
                    // The sadist's-den line's own letter and its tier-3 room
                    // (POE-249). Hand-built like every other value here, but
                    // taken from `LINES` rather than invented, so the pair
                    // still reads as a real offer: the grade is the LINE's, and
                    // `lineTop` is the room it was given for — which is not the
                    // tier-2 room `displayName` names.
                    grade: Some("C".to_string()),
                    line_top: Some("Sadist's Den".to_string()),
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
                // Non-null on purpose: `None` would pin only the field's
                // presence, and the mirror's `string | null` has two branches.
                // Like every other value in this sample it is hand-built and
                // states nothing about what the projection would pair with a
                // two-door recommendation —
                // `case_eight_lightning_workshop_names_the_conditional_second_door`
                // is where the pairing is asserted.
                secondary_door: Some("C1-D2".to_string()),
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
    const SAMPLE_SLICE_JSON: &str = r#"{"status":"read","waitingForPanel":true,"layout":{"slots":[{"slot":"A0","name":"Apex of Atzoatl","tier":0,"exact":true,"known":true,"current":false}],"doors":["C1-C2"],"uncertain":["B0-C1"],"unresolvedIncident":["B0-C1"],"markerError":"the diamond rect fell outside the capture","current":"C1","scale":0.99,"ncc":0.94,"confidence":"high","origin":[900,900],"centres":[[900,465],[795,569],[1005,569],[690,673],[900,673],[1110,673],[585,777],[795,777],[1005,777],[1215,777],[690,881],[900,900],[1110,881]],"rois":[{"kind":"panel","of":null,"rect":[1100,40,500,400]},{"kind":"corridor","of":"C1-C2","rect":[991,659,27,27]}],"diamond":{"corners":[[1.4,-0.1],[-0.1,1.2],[-1.4,0.1],[0.1,-1.2]],"seals":[{"neighbour":"C2","edge":"C1-C2","pos":[1.0,-0.9]}],"topIcon":[0.34,-0.3],"bottomIcon":[-0.34,0.3]}},"panel":{"room":"Locus of Corruption","roomRect":[1300,100,152,20],"offers":[{"index":0,"architectName":"Guatelitzi","kind":"upgrade","printedTarget":"Sadist's Den","displayName":"Torment Cells","builtTier":2,"grade":"C","lineTop":"Sadist's Den","rect":[1300,140,280,43]}],"incursionsRemaining":6},"advice":{"recommendations":[{"headline":"upgrade → Locus of Corruption","doorsLabel":"C1-C2, B0-C1","doors":["C1-C2","B0-C1"],"architectIndex":0,"ev":12.5,"risk":null,"reasons":["R1: connects toward the top"]}],"gambles":[{"headline":"kill either","doorsLabel":"no door","doors":[],"architectIndex":null,"ev":14.0,"risk":0.31,"reasons":["RV: excluded above the risk threshold"]}],"secondaryDoor":"C1-D2","mapAction":"leaveMap","warnings":["the incursion budget was not legible","1 of 2 architects read — the kill shown is forced, not chosen"],"forcedKill":true},"mode":"chase","config":{"artefactsOfTheVaal":false,"scarabOfTimelines":true},"profile":{"apexScore":3.5,"pathCost":1.25,"rerollUntilFavourable":true,"r4KeepUpgradeTargets":false},"unknownRooms":["D3"],"lastReadAt":1700000000000,"calibration":{"screen_w":2560,"screen_h":1440,"scale":0.99},"readNotice":"Temple: remaining ROI [810, 771, 300, 46] is outside the capture — windowed client?","lastError":"Temple: OCR failed"}"#;

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
    /// is dropped from the profile.
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
