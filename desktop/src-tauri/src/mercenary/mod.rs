//! Mercenary recruit-window capture (POE-165 slice 2).
//!
//! The Linux-testable core of the Merc OCR module: the vocabulary
//! (`vocab`), the pure panel geometry over OCR line rects (`geometry`), the
//! support-icon templates and the roman tier badge (`icons`), the gem-art
//! seeds those templates start from (`seed`), and the `mercenary` SSOT slice
//! published to every window (this file).
//!
//! # What lives where
//!
//! - **Pure, Linux-tested (here):** name matching, row/cell geometry, cell
//!   occupancy, badge reading, template NCC, the wire types.
//! - **Windows glue (WI-3, `modules.rs::spawn_mercenary` → `run_loop`):** the
//!   screen capture, the OCR call, the hover-confirm tick, the debug dump.
//!   None of it is needed to exercise anything in this directory.
//!
//! # Every tunable is a field, not a literal
//!
//! Every geometry number and every threshold is a field of [`MercGeometry`],
//! defaulted to the values measured on the ONE reference capture we have
//! (`scratchpad/recruit-cai.png`, 725×997, "Cai, the Lout"). They are
//! **provisional** until the first Windows debug dump. `load_override` merges
//! a partial `<app_data>/merc-geometry.json` over those defaults, so the first
//! correction is a file edit rather than a rebuild, and the slice reports
//! which source was used (`geometry_source`).
//!
//! # The capture loop
//!
//! `run` owns the thread (focus gate, detect cadence, hover-confirm), `read`
//! turns one detected layout plus one screen image into a capture, and `debug`
//! writes the calibration dump and the template-store commands. Two items here
//! stay `#[allow(dead_code)]` because only tests reach them
//! (`MercVocab::stats`, `vocab::default_thresholds`); every other WI-2 item now
//! has a production caller.

pub mod cellfit;
pub mod debug;
pub mod geometry;
pub mod icons;
pub mod read;
pub mod run;
pub mod search;
pub mod seed;
pub mod sources;
pub mod sync;
pub mod trigger;
pub mod vocab;

use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// D7 — the `mercenary` SSOT slice wire types
// ---------------------------------------------------------------------------

/// Which cue produced [`geometry::MercLayout::scale`] (POE-214).
///
/// The three are measurements of the same UI scale through different evidence:
/// `Ocr` is the OCR line pitch divided by [`MercGeometry::row_pitch`], the only
/// cue there was before the frame fit; `Frame` is the support grid's own gold
/// frame, measured on the pixels by [`cellfit::refine`] on THIS capture; `Held`
/// is that same frame measurement from an earlier capture of the same session,
/// re-applied because this one's fit declined. A capture at `Frame` or `Held`
/// is registered on the art; one at `Ocr` is the 6-12 px drift POE-214 exists
/// to remove, and every log line and debug report says which it is so a smoke
/// check can tell them apart without re-measuring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleSource {
    /// Derived from the OCR line pitch.
    Ocr,
    /// Measured off the support grid's gold frame, on this capture.
    Frame,
    /// The session's settled frame registration, re-applied to a capture whose
    /// own fit declined (a tooltip, art too dark for the ring). The rects are
    /// registered on the art; the measurement behind them is one or more ticks
    /// old. See [`cellfit::apply_held`].
    Held,
}

impl ScaleSource {
    /// The one word the log line prints after the scale.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ocr => "ocr",
            Self::Frame => "frame",
            Self::Held => "held",
        }
    }
}

/// Module status, in precedence order **off > unavailable > live > done >
/// scanning > idle**.
///
/// `Off` is applied by the SSOT composer (the module is disabled); the rest are
/// owned by the capture loop. The page treats this as authoritative over
/// `MercCapture::live`, because loop cleanup never runs on app exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MercStatus {
    /// The module is disabled — no work runs.
    Off,
    /// Running, and waiting for a trigger — no OCR is happening (POE-198).
    Idle,
    /// A burst is armed: the loop is looking for a recruit window right now.
    Scanning,
    /// A recruit window is captured right now.
    Live,
    /// The captured window is fully read and the OCR is PAUSED (2026-08-25).
    ///
    /// Still live — the window is on screen and the capture on the page is the
    /// current one — but every row, every cell and the header have been read,
    /// so another DETECT has nothing left to find. The loop drops to a 10 s
    /// liveness check (is the window still there?).
    ///
    /// The hover tick keeps running: "every cell was read" is not "every cell
    /// was read right", and the tooltip is the only thing that can correct a
    /// confident wrong match. Its idle cost is one cursor read per 400 ms, and
    /// re-reading a cell that already matched is bounded per cell
    /// (`run::HoverBudget`).
    ///
    /// It ranks BELOW `Live` because it is a narrower claim, and above
    /// `Scanning` because a fully-read window outranks a burst looking for one.
    /// Nothing publishes `Scanning` over it: the strip's verdict is on screen
    /// and current (`trigger::scan_outranks`).
    Done,
    /// Not Windows, or the OCR engine is missing.
    Unavailable,
}

/// Where the merc trade auto-search stands, as the page and the overlay read
/// it (POE-202).
///
/// Lives here rather than in `search.rs` because it is a SLICE type: the
/// windows read it off [`MercenarySlice::trade`], and every other wire type
/// the slice carries is declared in this file. `search` re-exports it so the
/// trigger policy reads as one vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MercTradeStatus {
    /// The module is off — nothing to say.
    Off,
    /// Nothing to search: no capture, no query, or the user turned the
    /// auto-search off.
    Idle,
    /// A query exists but the league is not resolved yet, so nothing was
    /// enqueued. Distinct from `Error`: nothing failed, the app just cannot
    /// address a trade site without a league.
    WaitingLeague,
    /// Handed to the trade queue, waiting for its turn behind the rate limiter.
    Queued,
    /// In flight.
    Searching,
    /// A result is on the slice.
    Done,
    /// The lookup failed; the message is on the slice.
    Error,
}

impl Default for MercTradeStatus {
    /// `Off`, for the reason [`MercenarySlice::default`] gives about its own
    /// status: the module ships disabled, so a window that polls before the
    /// capture loop has published anything must not be told the search is
    /// idle-but-running. It matches the TS pre-poll default
    /// (`lib/mercenaries/capture.ts`) as well, so the page's first paint says
    /// the same thing whichever side it came from.
    fn default() -> Self {
        MercTradeStatus::Off
    }
}

/// Everything the windows need to render the captured mercenary's own trade
/// search (POE-202).
///
/// # The one invariant
///
/// [`Self::query_hash`] is the hash of the query [`Self::result`] and
/// [`Self::url`] describe — never "the newest hash the loop has seen". That is
/// what makes a late result safe to accept: a lookup that comes back carrying
/// a different hash is answering a question the capture has already moved on
/// from, and is dropped. Every transition that moves the hash therefore clears
/// the result in the same write.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercTradeState {
    pub status: MercTradeStatus,
    /// The capture identity the rest of this struct answers for.
    pub query_hash: Option<String>,
    /// The trade-site link for that query. `None` until the league resolves —
    /// a URL cannot name a search without one.
    pub url: Option<String>,
    pub result: Option<crate::trade::MercTradeResult>,
    /// Why the last lookup failed. Set only alongside
    /// [`MercTradeStatus::Error`].
    pub error: Option<String>,
    /// The hash GGG REJECTED — a 4xx, which is the same answer however often
    /// it is asked. `search::decide` refuses to retry it; a 5xx or a transport
    /// failure leaves this `None` and keeps the debounced retry.
    ///
    /// Off the wire (`serde(skip)`) because it is policy, not display: the page
    /// shows the failure through [`Self::error`], and the only reader is the
    /// trade session, which re-reads the slice on every tick. It rides on the
    /// slice rather than on the session because the lookup task that learns of
    /// the rejection can reach the slice and not the session.
    ///
    /// Self-invalidating: it names a hash, so a capture that moves on is never
    /// blocked by it and one that moves back is blocked again, correctly.
    #[serde(skip)]
    pub terminal_hash: Option<String>,
    /// Searches this capture session has spent, out of
    /// [`search::MAX_SEARCHES`]. Shown so a user who sees a stale price knows
    /// why the app stopped asking.
    pub searches_used: u8,
}

/// The shipped default for `merc_trade_auto`: the auto-search is ON.
///
/// Unlike the gem path's `auto_trade_enabled` (off by default,
/// `docs/TRADE-LIFECYCLE.md` §2) this one is opt-OUT, because it is bounded by
/// construction: at most [`search::MAX_SEARCHES`] searches per captured
/// mercenary, only while the merc module — itself default-off — is running.
pub const DEFAULT_TRADE_AUTO: bool = true;

/// The shipped default for `merc_tier_floor`: 3, the mercenary exactly as
/// read. Lowering it comps the capture against weaker grades of the same
/// links.
pub const DEFAULT_TIER_FLOOR: u8 = 3;

/// `merc_tier_floor` as a support tier, or the reason it is not one.
///
/// A separate function from the clamp in
/// [`search::build_capture_query`] on purpose: a COMMAND must refuse a value
/// the user's UI should never have sent, while a value already on disk is
/// clamped and reported rather than failing the whole settings load.
pub fn validate_tier_floor(floor: u8) -> Result<u8, String> {
    if (1..=3).contains(&floor) {
        Ok(floor)
    } else {
        Err(format!("tier floor {floor} is not a support tier (1..=3)"))
    }
}

/// How confident a single read (skill name or support cell) is.
///
/// Wire strings are pinned by a serde test here AND by a TS union literal in
/// `lib/mercenaries/capture.ts` — the verdict engine branches on them, so a
/// rename silently changes verdicts rather than failing loudly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadState {
    /// Confidently resolved to one vocabulary entry (or one id set).
    Matched,
    /// Above the LOW threshold but below MATCH, or without the required lead.
    LowConfidence,
    /// Nothing cleared LOW.
    Unknown,
    /// The user hovered the cell and the tooltip confirmed the identity.
    Confirmed,
    /// Resolved to a `(family, tier)` that more than one vocabulary entry
    /// shares (Greater vs Gilded at tier 3).
    Ambiguous,
}

impl Default for ReadState {
    /// A read nobody has filled in yet knows nothing — never `Matched`.
    fn default() -> Self {
        ReadState::Unknown
    }
}

/// One skill row's name read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercSkillRead {
    /// The OCR text the match was made from (pass-2 text when re-OCR ran).
    pub raw: String,
    /// Vocabulary ids the read resolves to. A SET, not a single id: entry
    /// `text` is not a key in GGG's vocabulary.
    pub ids: Vec<String>,
    pub name: Option<String>,
    pub score: f32,
    pub state: ReadState,
}


/// One support cell's read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercSupportRead {
    pub slot: u8,
    /// `[x, y, w, h]` in screen px — the hover tick needs it to tell whether
    /// the cursor is inside this cell.
    pub rect: [i32; 4],
    pub family: Option<String>,
    pub tier: Option<u8>,
    /// Vocabulary ids this `(family, tier)` resolves to. More than one means
    /// `state == Ambiguous`.
    pub ids: Vec<String>,
    pub name: Option<String>,
    pub score: f32,
    pub state: ReadState,
    /// Names considered when the read is ambiguous or low-confidence.
    pub candidates: Vec<String>,
}

/// One skill row: its name and the support cells attached to it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercRow {
    pub index: u8,
    pub skill: MercSkillRead,
    pub supports: Vec<MercSupportRead>,
}

/// Best-effort recruit-window header. Every field is `Option` — a missing
/// field is `None`, never a guess.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercHeader {
    pub name: Option<String>,
    pub class: Option<String>,
    pub level: Option<u32>,
    pub wager: Option<u64>,
}

/// One recruit-window capture.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercCapture {
    pub captured_at_ms: u64,
    /// The window was still on screen at the last detect tick. Display-only —
    /// `MercenarySlice::status` is authoritative (see [`MercStatus`]).
    pub live: bool,
    pub scale: f32,
    /// `[width, height]` of the screen the capture came from.
    pub screen: [u32; 2],
    pub header: MercHeader,
    pub rows: Vec<MercRow>,
}

/// The `mercenary` SSOT slice. Rust-owned; the webview only reads it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercenarySlice {
    pub status: MercStatus,
    /// The last capture, live or retired. Retired captures stay so the page
    /// keeps showing the verdict after the window closes.
    pub capture: Option<MercCapture>,
    /// Everything in the template store as `"<family>--<tier>"`, sorted —
    /// exactly [`icons::TemplateStore::learned_keys`].
    ///
    /// A machine-readable key, not a display label: the page splits the
    /// trailing `--<digits>` back into the two arguments of
    /// `merc_forget_template(family, tier)`, which is how a mistimed
    /// hover-confirm gets un-poisoned. Prettifying this breaks that button.
    pub learned_families: Vec<String>,
    /// The subset of [`Self::learned_families`] this device knows only because
    /// the shared pool taught it — no local hover confirmed any of them
    /// (POE-201). Exactly [`icons::TemplateStore::pooled_keys`], same shape and
    /// same parse, so the page can mark a chip without a second list to keep in
    /// step.
    #[serde(default)]
    pub pooled_families: Vec<String>,
    /// The families this device knows only from the gem art it seeded itself
    /// with (POE-208) — exactly [`icons::TemplateStore::seeded_families`].
    ///
    /// FAMILY NAMES, not `"<family>--<tier>"` keys, and that is the whole
    /// difference from the two lists above: a seed is installed once per family
    /// under the family's lowest vocabulary tier, so the tier in the key is an
    /// implementation detail of the store rather than something the player
    /// chose, and printing it would invite a `merc_forget_template(family,
    /// tier)` call for a tier nobody read. The seed's own door is
    /// `merc_forget_seed(family)`.
    ///
    /// NOT a subset of [`Self::learned_families`] — the two are independent
    /// lists over the same store, and a family can be in both: a hover confirm
    /// of a seeded family stores a Local sample BESIDE the seed (the same-key
    /// "already known" check ignores seeds), so the page shows one chip in each
    /// group and the two ✕ buttons remove different things.
    #[serde(default)]
    pub seeded_families: Vec<String>,
    pub last_error: Option<String>,
    /// Who the module HEARD, for the burst it is scanning under (2026-08-25).
    ///
    /// `Some` only alongside [`MercStatus::Scanning`], and only for a
    /// Client.txt burst — Scan now names nobody. Written by the same publish
    /// that arms the status ([`trigger::arm`]) so the strip can say "heard
    /// Fennik, of Unshakeable Faith · scanning…" the moment the voice line
    /// lands, instead of "waiting" until the loop's next tick.
    ///
    /// Not an echo of the gate: the gate is a burst state machine with its own
    /// expiry and the slice is what the windows read, so the speaker is written
    /// and cleared with the status it qualifies. A speaker without `Scanning`
    /// would be a name the strip attaches to the wrong thing.
    #[serde(default)]
    pub burst_speaker: Option<String>,
    /// `"default"` or `"file"` — which [`MercGeometry`] the module is running.
    pub geometry_source: String,
    /// The guides taking NO part in the verdict, in [`sources::SOURCE_IDS`]
    /// order (POE-199).
    ///
    /// A settings ECHO, not a reading: the capture loop never writes it, and
    /// the stored slice always holds the empty default. It is composed onto
    /// every snapshot at read time from `AppState.merc_sources_off`
    /// (`ssot::compose_snapshot`, the same way `normal_variant` is), so there
    /// is no second copy to keep in step and the page and the overlay cannot
    /// evaluate one capture against two different guide sets.
    #[serde(default)]
    pub sources_off: Vec<String>,
    /// The shared template pool's state (POE-201) — when it was last pulled,
    /// how that went, how many samples came from it, and how many local ones
    /// are still waiting to be offered.
    ///
    /// A settings-style ECHO like [`Self::sources_off`], and for the same
    /// reason: the pull and the uploader run on their own tasks, so composing
    /// it at read time from `AppState.merc_sync` keeps the slice's only writer
    /// the capture loop.
    #[serde(default)]
    pub sync: sync::MercSyncStatus,
    /// The captured mercenary's own trade search (POE-202) — status, link and
    /// listings.
    ///
    /// Unlike [`Self::sources_off`] and [`Self::sync`] this is STORED, not
    /// composed: the capture loop's trade tick and the lookup task both write
    /// it through `run::publish`, so the slice keeps the writer it already
    /// had. Only `trade.status` is overridden at read time, and only to `Off`
    /// when the module is disabled.
    #[serde(default)]
    pub trade: MercTradeState,
    /// Whether the auto-search runs at all — a settings ECHO of
    /// `AppState.merc_trade_auto`, composed like [`Self::sources_off`] and for
    /// the same reason: the page renders the toggle from it while the module
    /// is off and no loop will ever publish it.
    #[serde(default)]
    pub trade_auto: bool,
    /// The lowest support tier the search accepts, 1..=3 — a settings ECHO of
    /// `AppState.merc_tier_floor`, composed like [`Self::trade_auto`].
    #[serde(default)]
    pub tier_floor: u8,
}

impl Default for MercenarySlice {
    /// `Off`, because the module defaults to disabled (see `modules.rs`) and a
    /// window that polls before the loop has published anything must not be
    /// told the module is idle-but-running.
    fn default() -> Self {
        Self {
            status: MercStatus::Off,
            capture: None,
            learned_families: Vec::new(),
            pooled_families: Vec::new(),
            seeded_families: Vec::new(),
            last_error: None,
            burst_speaker: None,
            geometry_source: GEOMETRY_SOURCE_DEFAULT.to_string(),
            sources_off: Vec::new(),
            sync: sync::MercSyncStatus::default(),
            trade: MercTradeState::default(),
            // The shipped values, so a snapshot read before `compose_snapshot`
            // has echoed the owners still says what the app actually does.
            trade_auto: DEFAULT_TRADE_AUTO,
            tier_floor: DEFAULT_TIER_FLOOR,
        }
    }
}

/// `geometry_source` when the built-in reference values are in force.
pub const GEOMETRY_SOURCE_DEFAULT: &str = "default";
/// `geometry_source` when `<app_data>/merc-geometry.json` was merged in.
pub const GEOMETRY_SOURCE_FILE: &str = "file";
/// The override file's name inside the app data directory.
pub const GEOMETRY_OVERRIDE_FILE: &str = "merc-geometry.json";
/// The NPC-denylist override file's name inside the app data directory
/// (POE-198) — one speaker per line, merged over the shipped fixture.
pub const DENYLIST_OVERRIDE_FILE: &str = "merc-npc-denylist.txt";
/// This module's registry id.
///
/// `modules.rs` still spells it as a literal on purpose: `manager.test.ts`
/// parses the `MODULES` array for `id: "..."` and a constant would read as a
/// renamed-away registry entry.
pub const MODULE_ID: &str = "mercenary";
/// The learned icon templates' directory inside the app data directory.
pub const ICONS_DIR: &str = "merc-icons";
/// Where `merc_debug_capture` writes its per-capture dump directories.
pub const DEBUG_DIR: &str = "merc-debug";

// ---------------------------------------------------------------------------
// D1 — reference geometry + thresholds
// ---------------------------------------------------------------------------

/// Fuzzy-match thresholds, in one place so the debug report can print them and
/// the JSON override can move them without a rebuild.
///
/// All values are PROVISIONAL — chosen from one reference capture and the
/// vocabulary, not from a measured error distribution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Thresholds {
    /// Jaro-Winkler score at which a name read is `Matched`.
    pub name_match: f32,
    /// Jaro-Winkler score at which a name read is `LowConfidence`.
    pub name_low: f32,
    /// Lead over the runner-up a `Matched` read needs…
    pub name_lead: f32,
    /// …unless it clears this outright.
    pub name_no_lead: f32,
    /// Icon NCC at which a template match is `Matched`.
    ///
    /// **Measured, not D4's provisional 0.80.** Over all 66 pairs of the
    /// reference panel's 12 real cells: the closest pair of genuinely
    /// DIFFERENT icons (a plain silver gem vs a silver gem under crossed
    /// golden shafts) correlates at **0.8552**, and the loosest pair showing
    /// the SAME art (one blue double-orb, a px of rect misalignment apart) at
    /// **0.9034**. 0.80 would therefore have merged two supports into one
    /// family. 0.88 is the midpoint of that 0.8552..0.9034 band — a thin one,
    /// off 12 cells, so this is among the first numbers the Windows dump
    /// should re-derive.
    pub icon_match: f32,
    /// Icon NCC at which a template match is `LowConfidence`.
    pub icon_low: f32,
    /// Lead the best family needs over the best OTHER family.
    pub icon_lead: f32,
    /// Jaro-Winkler score the recruit window's "Wager" label must reach, taken
    /// against the line's leading word (`geometry::is_wager_line`).
    ///
    /// 0.90 since POE-217. MEASURED head scores against "wager", all with
    /// strsim 0.11's unbounded prefix bonus:
    ///
    /// | head     | score | past this bar |
    /// |----------|-------|---------------|
    /// | `wager`  | 1.000 | admitted      |
    /// | `wagers` | 0.972 | admitted      |
    /// | `wagger` | 0.961 | admitted      |
    /// | `wagner` | 0.961 | admitted      |
    /// | `wage`   | 0.960 | admitted      |
    /// | `waggr`  | 0.907 | admitted      |
    /// | `waqer`  | 0.893 | refused       |
    /// | `vvager` | 0.822 | refused       |
    ///
    /// It was 0.98, chosen to refuse `Wagner` because "Wagner has entered the
    /// area" is an ordinary PoE chat line. 2026-08-27, 1920×1080: OCR returned
    /// the label as `Waggr: 6 231` (0.907), the footer buttons were hidden
    /// under the player's skill bar, and the capture was lost. 0.98 was buying
    /// a rejection the TEXT test does not have to make.
    ///
    /// `wagers`, `wagger`, `wagner` and `wage` are admitted BY THIS BAR and
    /// refused by the rest of `geometry::is_wager_line`: the head score is
    /// only half the predicate, and the other half is that the line carries an
    /// amount after the head word. The panel always draws one — the incident's
    /// own line was `Waggr: 6 231` — and none of "Wagner has entered the
    /// area", "wagers" or "Wage" does, so the digit rule is what keeps them
    /// out wherever on the screen they are drawn. That is what made dropping
    /// the bar from 0.98 affordable: 0.98 existed to refuse `Wagner` by score,
    /// and the digit rule refuses it on the thing that actually distinguishes
    /// a label from a word.
    ///
    /// Two further gates stand behind both halves. `geometry::detect` reaches
    /// this predicate only after two skill names have clustered into a
    /// left-aligned column, and then only accepts the line if it sits ABOVE
    /// row 1 within `wager_search_pitches` of it.
    pub wager_anchor: f32,
    /// Grayscale stddev above which a cell's inner region counts as occupied.
    /// Measured on the reference panel: occupied 42.7-60.9, empty 1.1-2.0.
    pub empty_cell_stddev: f32,
    /// Hover-confirm OCR region around the cursor, in reference px (D5).
    pub hover_w: u32,
    pub hover_up: u32,
    pub hover_down: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            name_match: 0.92,
            name_low: 0.85,
            name_lead: 0.03,
            name_no_lead: 0.97,
            wager_anchor: 0.90,
            icon_match: 0.88,
            icon_low: 0.78,
            icon_lead: 0.05,
            empty_cell_stddev: 18.0,
            hover_w: 600,
            hover_up: 500,
            hover_down: 120,
        }
    }
}

/// Where the roman tier badge sits inside a support cell, and what counts as
/// badge ink. All box values are fractions of the cell's inner region, so they
/// survive a resolution change without rescaling.
///
/// Measured on the reference panel's 12 real cells: the numerals are GOLD
/// serif glyphs (core stroke 255/215/142 sRGB) sitting on a common baseline
/// 6 px above the cell's inner bottom edge, ~8 px tall, centred ~14 px left of
/// the inner right edge. Icon art bleeds into the same corner and can be just
/// as bright, which is why the gold mask alone is not the accept rule — see
/// [`icons::read_tier`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct BadgeGeometry {
    /// Badge box width, as a fraction of the cell's inner width.
    pub width_frac: f32,
    /// Badge box top, as a fraction of the inner height above the bottom edge.
    pub top_frac: f32,
    /// Badge box bottom, as a fraction of the inner height above the bottom
    /// edge (the cell's own frame lives below this).
    pub bottom_frac: f32,
    /// Scanline band inside the badge box, as fractions of its height. Runs
    /// are counted here only, so serifs and art above/below cannot add one.
    pub band_lo_frac: f32,
    pub band_hi_frac: f32,
    /// Fraction of the scanline band a column must be ink in to count as part
    /// of a stroke.
    pub column_fill: f32,
    /// Minimum luma for badge ink.
    pub ink_luma_min: u8,
    /// Minimum red-minus-blue for badge ink (the numerals are gold; grey and
    /// blue-white icon highlights are not).
    pub ink_gold_delta: i32,
    /// Strokes must share a baseline within this many px.
    pub baseline_tolerance: i32,
    /// …and a top within this many px.
    pub top_tolerance: i32,
    /// Widest stroke may be at most this many times the narrowest.
    pub width_ratio_max: f32,
    /// Tallest stroke may be at most this many times the shortest.
    pub height_ratio_max: f32,
    /// A stroke must be at least this fraction of the inner cell height tall.
    pub min_height_frac: f32,
    /// …and at most this fraction tall.
    ///
    /// With only the ratio rules above, a numeral of ONE stroke (tier I) is
    /// judged on nothing — "comparable width" and "comparable height" are
    /// vacuous at n = 1, so a lone bar of gold art in the badge corner would
    /// read as tier I. These two absolute caps are what a single stroke is
    /// actually held to. Measured on the reference panel: real strokes are 8-9
    /// px tall and 1-2 px wide inside a 40 px cell (0.20-0.225 and
    /// 0.025-0.05), so 0.30 and 0.15 clear them with a third of headroom while
    /// rejecting the taller and fatter bars icon art produces. The height cap
    /// has to stay under the badge BOX (0.35 of the cell), or a bar filling
    /// the whole box passes it by construction.
    pub max_height_frac: f32,
    /// A stroke must be at most this fraction of the inner cell width wide.
    pub max_width_frac: f32,
}

impl Default for BadgeGeometry {
    fn default() -> Self {
        Self {
            width_frac: 0.55,
            top_frac: 0.425,
            bottom_frac: 0.075,
            band_lo_frac: 0.30,
            band_hi_frac: 0.70,
            column_fill: 0.6,
            ink_luma_min: 150,
            ink_gold_delta: 40,
            baseline_tolerance: 2,
            top_tolerance: 2,
            width_ratio_max: 2.0,
            height_ratio_max: 1.5,
            min_height_frac: 0.15,
            max_height_frac: 0.30,
            max_width_frac: 0.15,
        }
    }
}

/// Recruit-window geometry in **reference px at scale 1.0**, plus the
/// thresholds that go with it. The runtime scale `s` is derived per capture
/// from the observed row pitch (see [`geometry::detect`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct MercGeometry {
    /// Vertical distance between skill rows.
    pub row_pitch: f32,
    /// Height of one OCR text line — the single-row fallback's only scale cue.
    pub ref_line_height: f32,
    /// Distance from the skill-name column's left edge to the first support
    /// cell's left edge.
    pub cell_offset_x: f32,
    /// Distance between adjacent support cells' left edges.
    pub cell_pitch: f32,
    /// Support cell size (outer, including its frame).
    pub cell_size: f32,
    /// Frame inset — the region occupancy and icon matching read.
    pub cell_inset: f32,
    /// Support slots scanned per row before giving up.
    pub max_slots: u8,
    /// Rows the pass-2 re-OCR will read before it stops (D2 pass 2).
    ///
    /// Pass 2 costs ONE OCR call per row inside a single tick, so an
    /// over-clustered detect — a chat column, a stash page, anything that
    /// yields twenty left-aligned "rows" — turns one tick into twenty OCR
    /// calls and blows the loop's poll budget. 8 clears the 6 rows the
    /// reference panel has with room to spare; rows past it keep their pass-1
    /// text rather than being dropped.
    pub max_rows: u8,
    /// Lines within this fraction of a line height of the column's median x0
    /// belong to the skill-name column.
    pub column_x_tolerance_frac: f32,
    /// Lines whose centre gap is at most this many line heights apart belong
    /// to the SAME row (a wrapped name), not to the next one.
    pub row_cluster_factor: f32,
    /// Skill-name candidates needed before a panel is a panel.
    pub min_skill_candidates: usize,
    /// How far from the rows a text anchor may sit, in row pitches: the
    /// "Wager" and recruit-verdict lines above row 1, a footer button below
    /// the last row.
    pub wager_search_pitches: f32,
    pub badge: BadgeGeometry,
    pub thresholds: Thresholds,
}

impl Default for MercGeometry {
    /// Measured on `scratchpad/recruit-cai.png` (the only capture we have).
    /// The row pitch is the D1 reference value 49.3; the same panel's OCR line
    /// centres yield 48 (`s ≈ 0.974`), which is what a detect over that panel
    /// must report — the two numbers are a reference constant and a
    /// measurement, not a discrepancy.
    ///
    /// Two of them are KNOWN HIGH and deliberately left alone here: the
    /// committed fixture's own gold frame measures a slot pitch of 48.67 and a
    /// row pitch of 48.4, so `cell_pitch` is 0.7 % and `row_pitch` 1.8 % over
    /// truth. `cellfit` owns the true unit (`REF_PITCH`) so the frame fit does
    /// not inherit the error; correcting these two moves the OCR-path window,
    /// the seed store's memo key and three geometry tests at once, which is its
    /// own change — see POE-216.
    fn default() -> Self {
        Self {
            row_pitch: 49.3,
            ref_line_height: 16.0,
            cell_offset_x: 238.0,
            cell_pitch: 49.0,
            cell_size: 44.0,
            cell_inset: 2.0,
            max_slots: 6,
            max_rows: 8,
            column_x_tolerance_frac: 0.15,
            row_cluster_factor: 1.5,
            min_skill_candidates: 2,
            wager_search_pitches: 12.0,
            badge: BadgeGeometry::default(),
            thresholds: Thresholds::default(),
        }
    }
}

/// Load `<dir>/merc-geometry.json` over the reference defaults.
///
/// Every level of the file is optional (`#[serde(default)]` on each struct, so
/// serde fills missing fields from that struct's `Default`): a file carrying
/// only `{"rowPitch": 52}` moves the row pitch and leaves everything else at
/// its reference value.
///
/// Unknown keys are REJECTED (`deny_unknown_fields`). Silently ignoring them
/// is the failure mode that matters here: a misspelled `rowPich` would
/// otherwise load "successfully", report `geometry_source: "file"`, and run on
/// the defaults — leaving Sebastian recalibrating a number that never moves.
///
/// A missing file, an unreadable file, malformed JSON and an unknown key all
/// fall back to the defaults, and every case but the missing file returns the
/// error so the loop can surface it in `last_error`.
pub fn load_override(dir: &Path) -> (MercGeometry, &'static str, Option<String>) {
    let path = dir.join(GEOMETRY_OVERRIDE_FILE);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(_) => return (MercGeometry::default(), GEOMETRY_SOURCE_DEFAULT, None),
    };
    match serde_json::from_str::<MercGeometry>(&raw) {
        Ok(g) => (g, GEOMETRY_SOURCE_FILE, None),
        Err(e) => (
            MercGeometry::default(),
            GEOMETRY_SOURCE_DEFAULT,
            Some(format!("{} is not valid geometry JSON: {}", path.display(), e)),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wire contract for the whole slice. The TS mirror
    /// (`lib/mercenaries/capture.ts`) and the verdict engine branch on these
    /// exact strings; a `rename_all` drop or a variant rename would leave both
    /// sides compiling while the page renders "unknown" for everything.
    #[test]
    fn slice_serializes_camel_case_keys_and_snake_case_states() {
        let slice = MercenarySlice {
            status: MercStatus::Live,
            capture: Some(MercCapture {
                captured_at_ms: 1_700_000_000_000,
                live: true,
                scale: 0.974,
                screen: [2560, 1440],
                header: MercHeader {
                    name: Some("Cai, the Lout".into()),
                    class: Some("Shock Ambusher".into()),
                    level: Some(70),
                    wager: Some(1028),
                },
                rows: vec![MercRow {
                    index: 0,
                    skill: MercSkillRead {
                        raw: "Conductivity".into(),
                        ids: vec!["mercenary.skill_41484".into()],
                        name: Some("Conductivity".into()),
                        score: 1.0,
                        state: ReadState::Matched,
                    },
                    supports: vec![MercSupportRead {
                        slot: 0,
                        rect: [312, 13, 44, 44],
                        family: Some("Pierce".into()),
                        tier: Some(3),
                        ids: vec![
                            "mercenary.support_1".into(),
                            "mercenary.support_2".into(),
                        ],
                        name: None,
                        score: 0.84,
                        state: ReadState::Ambiguous,
                        candidates: vec![
                            "Greater Pierce (Tier 3)".into(),
                            "Gilded Pierce (Tier 3)".into(),
                        ],
                    }],
                }],
            }),
            learned_families: vec!["Pierce--3".into()],
            pooled_families: vec!["Pierce--3".into()],
            // POE-208. A family name, deliberately NOT a `--<tier>` key and
            // deliberately not one of the two lists above: the page renders it
            // as its own chip group whose ✕ calls `merc_forget_seed(family)`.
            seeded_families: vec!["Fork".into()],
            last_error: None,
            burst_speaker: Some("Fennik, of Unshakeable Faith".into()),
            geometry_source: GEOMETRY_SOURCE_FILE.into(),
            sources_off: vec!["guide-a".into()],
            sync: sync::MercSyncStatus {
                last_pull_ms: Some(1_700_000_000_000),
                last_pull: sync::PullResult::Unchanged,
                pooled_samples: 4,
                queued_uploads: 2,
                last_error: None,
            },
            // POE-202. The wire strings these carry are pinned by the chunk-4
            // tests; here they only keep the literal exhaustive, which is what
            // makes a new slice field a decision rather than a default.
            trade: MercTradeState::default(),
            trade_auto: DEFAULT_TRADE_AUTO,
            tier_floor: DEFAULT_TIER_FLOOR,
        };

        let v = serde_json::to_value(&slice).expect("slice serializes");

        assert_eq!(v["status"], "live");
        assert_eq!(v["geometrySource"], "file");
        assert_eq!(v["learnedFamilies"][0], "Pierce--3");
        // The enabled-guide echo (POE-199): the overlay and the page both read
        // `sourcesOff` off this slice, so the key's spelling is a contract.
        assert_eq!(v["sourcesOff"], serde_json::json!(["guide-a"]));
        // The shared pool's two contracts (POE-201): which chips the page marks
        // as the pool's, and what it says about the last pull. `unchanged` is a
        // wire string the page branches on, so a variant rename must fail here
        // and not on screen.
        assert_eq!(v["pooledFamilies"], serde_json::json!(["Pierce--3"]));
        // The gem-art seeds (POE-208), the page's third chip group. The key
        // spelling is the contract, and so is the SHAPE of its members: a
        // `"Fork--1"` here would send the page's ✕ to `merc_forget_seed` with a
        // family name no store key carries, which forgets nothing and reports
        // success.
        assert_eq!(v["seededFamilies"], serde_json::json!(["Fork"]));
        // The speaker the strip prints beside "scanning" (2026-08-25 smoke).
        assert_eq!(v["burstSpeaker"], "Fennik, of Unshakeable Faith");
        assert_eq!(v["sync"]["lastPull"], "unchanged");
        assert_eq!(v["sync"]["lastPullMs"], 1_700_000_000_000u64);
        assert_eq!(v["sync"]["pooledSamples"], 4);
        assert_eq!(v["sync"]["queuedUploads"], 2);
        assert_eq!(v["lastError"], serde_json::Value::Null);
        let cap = &v["capture"];
        assert_eq!(cap["capturedAtMs"], 1_700_000_000_000u64);
        assert_eq!(cap["live"], true);
        assert_eq!(cap["screen"], serde_json::json!([2560, 1440]));
        assert_eq!(cap["header"]["class"], "Shock Ambusher");
        assert_eq!(cap["header"]["wager"], 1028);
        let row = &cap["rows"][0];
        assert_eq!(row["skill"]["state"], "matched");
        assert_eq!(row["skill"]["ids"], serde_json::json!(["mercenary.skill_41484"]));
        let cell = &row["supports"][0];
        assert_eq!(cell["state"], "ambiguous");
        assert_eq!(cell["rect"], serde_json::json!([312, 13, 44, 44]));
        assert_eq!(cell["ids"].as_array().map(|a| a.len()), Some(2));
        assert_eq!(cell["candidates"][1], "Gilded Pierce (Tier 3)");
    }

    /// Every slice enum variant's wire string, pinned one by one. The verdict
    /// engine treats `low_confidence` / `unknown` / `ambiguous` as UNKNOWN and
    /// `matched` / `confirmed` as presence, and `trade-view.ts` switches on
    /// every `MercTradeStatus` spelling to pick a label and a tone — so a
    /// silent rename flips verdicts or blanks the trade badge rather than
    /// erroring.
    #[test]
    fn every_slice_enum_wire_string_is_pinned() {
        let states = [
            (ReadState::Matched, "matched"),
            (ReadState::LowConfidence, "low_confidence"),
            (ReadState::Unknown, "unknown"),
            (ReadState::Confirmed, "confirmed"),
            (ReadState::Ambiguous, "ambiguous"),
        ];
        for (state, wire) in states {
            assert_eq!(serde_json::to_value(state).unwrap(), wire);
        }

        let statuses = [
            (MercStatus::Off, "off"),
            (MercStatus::Idle, "idle"),
            (MercStatus::Scanning, "scanning"),
            (MercStatus::Live, "live"),
            (MercStatus::Done, "done"),
            (MercStatus::Unavailable, "unavailable"),
        ];
        for (status, wire) in statuses {
            assert_eq!(serde_json::to_value(status).unwrap(), wire);
        }

        // POE-202. `waiting-league` is the one kebab-cased spelling here, so
        // the rename_all attribute itself is pinned and not just the variant
        // names.
        let trade = [
            (MercTradeStatus::Off, "off"),
            (MercTradeStatus::Idle, "idle"),
            (MercTradeStatus::WaitingLeague, "waiting-league"),
            (MercTradeStatus::Queued, "queued"),
            (MercTradeStatus::Searching, "searching"),
            (MercTradeStatus::Done, "done"),
            (MercTradeStatus::Error, "error"),
        ];
        for (status, wire) in trade {
            assert_eq!(serde_json::to_value(status).unwrap(), wire);
        }
    }

    /// A window polling before the loop has published anything must be told
    /// the module is OFF, not idle-but-running: `idle` reads as "on and
    /// watching", which would make the page's empty state a lie. The trade
    /// status says it too — `mercenarySliceDefault()` in
    /// `lib/mercenaries/capture.ts` opens on `off`, and a Rust default of
    /// `idle` would make the badge change wording on the first poll without
    /// anything having happened.
    #[test]
    fn default_slice_is_off_with_no_capture() {
        let slice = MercenarySlice::default();

        assert_eq!(slice.status, MercStatus::Off);
        assert_eq!(slice.trade.status, MercTradeStatus::Off);
        assert!(slice.capture.is_none());
        assert_eq!(slice.geometry_source, "default");
        assert!(slice.learned_families.is_empty());
        // Nothing is seeded until the art is fetched and installed (POE-208),
        // so a pre-poll slice claiming a seeded family would put a chip on the
        // page for a template the store does not hold.
        assert!(slice.seeded_families.is_empty());
        assert!(slice.last_error.is_none());
    }

    fn write_override(dir: &Path, body: &str) {
        std::fs::write(dir.join(GEOMETRY_OVERRIDE_FILE), body).expect("write override");
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "poe-merc-geom-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// No file at all is the normal case — the defaults stand and the source
    /// reads "default", which the status bar shows.
    #[test]
    fn a_missing_override_file_leaves_the_reference_defaults() {
        let dir = temp_dir("missing");

        let (g, source, err) = load_override(&dir);

        assert_eq!(g, MercGeometry::default());
        assert_eq!(source, GEOMETRY_SOURCE_DEFAULT);
        assert!(err.is_none());
    }

    /// The whole point of the override: recalibrating ONE number must not
    /// reset the other twenty to zero. This is the failure `#[serde(default)]`
    /// on the STRUCT prevents and per-field `#[serde(default)]` would cause
    /// (a missing `f32` field would deserialize as 0.0, not as its reference
    /// value).
    #[test]
    fn a_partial_override_merges_over_the_defaults() {
        let dir = temp_dir("partial");
        write_override(&dir, r#"{"rowPitch": 61.5}"#);

        let (g, source, err) = load_override(&dir);

        assert_eq!(source, GEOMETRY_SOURCE_FILE);
        assert!(err.is_none());
        assert_eq!(g.row_pitch, 61.5);
        assert_eq!(g.cell_pitch, MercGeometry::default().cell_pitch);
        assert_eq!(g.cell_size, MercGeometry::default().cell_size);
        assert_eq!(g.thresholds, Thresholds::default());
        assert_eq!(g.badge, BadgeGeometry::default());
    }

    /// The nested blocks merge the same way — an override that moves one
    /// threshold must not zero the other ten, nor the badge block it did not
    /// mention.
    #[test]
    fn a_partial_nested_override_merges_field_by_field() {
        let dir = temp_dir("nested");
        write_override(&dir, r#"{"thresholds": {"emptyCellStddev": 25.5}}"#);

        let (g, _, _) = load_override(&dir);

        assert_eq!(g.thresholds.empty_cell_stddev, 25.5);
        assert_eq!(g.thresholds.name_match, Thresholds::default().name_match);
        assert_eq!(g.thresholds.hover_w, Thresholds::default().hover_w);
        assert_eq!(g.badge, BadgeGeometry::default());
        assert_eq!(g.row_pitch, MercGeometry::default().row_pitch);
    }

    /// A misspelled key is the recalibration failure that would otherwise be
    /// invisible: with unknown keys ignored, `rowPich` loads clean, the status
    /// bar reports "file", and the number Sebastian is trying to move stays at
    /// its default with nothing to say so.
    #[test]
    fn a_misspelled_override_key_is_rejected_rather_than_ignored() {
        let dir = temp_dir("typo");
        write_override(&dir, r#"{"rowPich": 52.0}"#);

        let (g, source, err) = load_override(&dir);

        assert_eq!(
            source, GEOMETRY_SOURCE_DEFAULT,
            "a rejected file must not be reported as in force",
        );
        assert_eq!(g.row_pitch, MercGeometry::default().row_pitch);
        let err = err.expect("an unknown key must report an error");
        assert!(err.contains("rowPich"), "the error must name the bad key, got {err:?}");
    }

    /// The same rule one level down, in BOTH nested blocks — threshold and
    /// badge keys are the ones a calibration session actually edits, so a typo
    /// is likelier there than in the top-level geometry.
    #[test]
    fn a_misspelled_key_in_any_nested_block_is_rejected() {
        for (block, body) in [
            ("thresholds", r#"{"thresholds": {"emptyCelStddev": 25.5}}"#),
            ("badge", r#"{"badge": {"inkLumaMn": 200}}"#),
        ] {
            let dir = temp_dir(&format!("typo-{block}"));
            write_override(&dir, body);

            let (g, source, err) = load_override(&dir);

            assert_eq!(source, GEOMETRY_SOURCE_DEFAULT, "{block}");
            assert_eq!(g, MercGeometry::default(), "{block}");
            assert!(err.is_some(), "an unknown {block} key must report an error");
        }
    }

    /// A typo'd recalibration must not silently run on defaults: the loop gets
    /// the error to surface in `last_error`, and the source stays "default" so
    /// the status bar does not claim a file is in force.
    #[test]
    fn malformed_override_json_falls_back_and_reports_the_error() {
        let dir = temp_dir("malformed");
        write_override(&dir, "{ not json at all");

        let (g, source, err) = load_override(&dir);

        assert_eq!(g, MercGeometry::default());
        assert_eq!(source, GEOMETRY_SOURCE_DEFAULT);
        let err = err.expect("a malformed override must report an error");
        assert!(
            err.contains(GEOMETRY_OVERRIDE_FILE),
            "the error must name the file to edit, got {err:?}",
        );
    }
}
