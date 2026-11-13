//! Temple of Atzoatl **builder** advisor (POE-124 epic, POE-167 slice 0,
//! POE-168 slice 1, POE-169 slice 2).
//!
//! Scope of this directory is the *building* phase — the two decisions a player
//! makes per incursion (which architect to kill, which passage to open) while
//! the temple is assembled across maps. Running the finished temple is a
//! separate helper and is not modelled here.
//!
//! # What lives where
//!
//! - [`strategy`] (POE-167) — the pure objective function: the room-line
//!   vocabulary, the per-strategy [`strategy::StrategyProfile`], the
//!   Chase/Scarab [`strategy::Mode`] selector, and the config flags — Artefacts
//!   of the Vaal is user-facing; the Scarab of Timelines flag is kept in
//!   settings but hidden and inactive in 3.30 (POE-377). No Tauri, no state, no Windows code, no board graph.
//! - [`lattice`], [`anchor`], [`doors`], [`reader`] (POE-168) — the layout
//!   reader: a screenshot of the temple's layout panel in, a
//!   [`reader::TempleLayout`] out. Also pure, and equally free of Tauri and
//!   Windows code, so the whole of it runs in the Linux test container.
//! - [`rooms`], [`panel`], [`markers`] (POE-169) — room identity: the closed
//!   87-name vocabulary and the fuzzy matcher over it ([`rooms`]), the side
//!   panel's text behind a one-method OCR trait ([`panel`]), and the door
//!   markers on the panel's diamond, which settle the corridors POE-168 hands
//!   back as [`reader::TempleLayout::uncertain`] ([`markers`]). Pure apart
//!   from [`panel::SystemOcr`], the single seam that calls the engine.
//! - [`advisor`] (POE-170) — the board graph, the Monte-Carlo rollout and the
//!   rule layer that ranks `(architect kill, door set)` and decides whether to
//!   leave the map. Consumes every module above it and is pure.
//! - [`market`] (POE-257) — the desktop's mirror of POE-255's
//!   `GET /api/analysis/temple-market` payload ([`market::MarketInput`]): room
//!   sale deltas, recipe-member prices, the floor, the league. It does no HTTP
//!   — POE-258 owns the poll — and a stale read answers as if it carried
//!   nothing, in one place.
//! - [`valuation`] (POE-257) — what one room-tier is worth, in CHAOS: sale +
//!   drops + bonus, tiers 1 and 2 at a fraction of tier 3, every term a
//!   [`valuation::Driver`] carrying its count, price and provenance, and
//!   [`rooms::RoomRank`]'s ladder standing in only where nothing was summed.
//!   ADR-022.
//! - [`preset`] (POE-257) — the two valuations the player chooses between,
//!   Default and Custom ([`preset::Preset`]), the Custom table's rates and
//!   per-room overrides ([`preset::TempleCustomSettings`], persisted in its own
//!   settings block with a per-entry fallback), [`preset::value_table`], the
//!   one compute, and (POE-257 WI-3) the cache in front of it —
//!   [`preset::ValuationKey`], [`preset::cached_valuation`],
//!   [`preset::compute_valuation`], [`preset::store_valuation`] — which is why
//!   a read LOOKS the table up (`crate::temple::valuation::temple_valuation_now`) instead of
//!   building one. The three are separate so the accessor can build with the
//!   slot mutex DROPPED.
//!
//! The lifecycle these modules implement — what arms the reader, when a read
//! runs, when each overlay shows — is normative in `docs/TEMPLE-LIFECYCLE.md`.
//!
//! **Since POE-389 there is no capture loop.** One press of the app-wide read
//! key grabs one frame (`crate::read_hotkey`); [`press::TEMPLE_READER`] arms
//! only under Client.txt temple context ([`trigger::ArmReason::AlvaStart`] or
//! [`trigger::ArmReason::TempleArea`]) and hands that frame to the module
//! thread, which reads it once ([`run`]). The history bullets below name loop
//! pieces (`LoopState`, the gate, retries, sweeps, `cycle_complete`) that
//! POE-389 deleted; they stay as history only.
//!
//! Later changes that moved where things live (2026-09-03/04):
//! - POE-234: the anchor's cold start is `anchor::Scene::pyramid_sweep` behind
//!   `anchor_for_loop` (hint → `MEASURED_SCALES` → pyramid); the temple reads and
//!   writes the shared screen scale (`run::hint_for_capture`,
//!   `run::publish_anchor_scale`, gate `run::screen_from_anchor`) — ADR-020.
//! - POE-230: `run::panel_rect` / `run::diamond_rect` are keyed on the Entrance
//!   anchor, never the capture's edge; `run::read_rois` is the never-cover set.
//! - POE-243: [`panel`] reads BOXED lines; blocks group by geometry, offers and
//!   the title carry screen rects; `Warning::PartialArchitects`.
//! - POE-246: [`trigger`]'s gate reads the SCREEN as well as Client.txt (the
//!   panel on screen, the start-up probe).
//! - POE-244: [`markers`] publishes the seal ring / rhombus the overlay draws — ADR-019.
//! - POE-249: [`trigger::classify`] is the ONE owner of what a Client.txt line
//!   means ([`trigger::LineEvent`], [`trigger::ends_epoch`], the three measured
//!   START phrases, [`trigger::ArmReason::AlvaStart`] armed with no deadline);
//!   the loop reads a board ONCE per identity — `run::LoopState::gate` →
//!   `run::GateAnswer` over the epoch, the re-arm counter and
//!   [`slice::BoardFrame`], with `run::RETRIES` merged re-reads
//!   ([`slice::KeptRead`], [`slice::merge_reads`], [`slice::unclean`], and
//!   `run::kept_for`) and the placed-origin recheck plus explicit cold fallback
//!   owning anchor recovery. `slice::panel_signature` and the periodic panel
//!   re-OCR are gone;
//!   [`slice::layout_signature`] survives as the semantic half of the identity
//!   and `ReadGate` is now [`slice::RearmGate`], the re-arm counter alone.
//! - 2026-09-07 (WI-1): the capture has no tails left. `ALVA_TAIL_MS` and
//!   `PANEL_TAIL_MS` are gone with [`trigger::ArmReason`]'s `AlvaLine`, a
//!   non-START Alva line STANDS THE CAPTURE DOWN (the temple's own banter
//!   excepted), the panel input is one tick (`run::LoopState::live`) rather
//!   than 120 s, and the loop ends its own cycle once a board has been read and
//!   the sheet closes (`run::cycle_complete`,
//!   `trigger::ArmState::complete_cycle`). The stand-down names its cause
//!   ([`trigger::StandDown`]). Two guards from the same day's fix round: a tick
//!   whose screen GRAB failed is not evidence about the sheet and leaves the
//!   panel input alone (`run::LoopState::on_blind_tick`), and a completion is
//!   refused when the board key has moved on since the tick that observed it
//!   (`ArmState::complete_cycle`'s `key_current`).
//! - 2026-09-11 (POE-275, owner): the sheet retires on the SECOND consecutive
//!   clean miss (`run::RETIRE_AFTER` = 2), so the panel input above is two
//!   ticks. The first is a held miss (`run::DetectOutcome::HeldMiss`) that
//!   publishes nothing (`run::miss_publish`): it neither hides the sheet-bound
//!   overlays nor ends the cycle.
//! - POE-389: the loop above is gone. [`press`] arms on the read key under
//!   Client.txt temple context, and [`run`] reads each pressed frame once.
//!
//! - [`trigger`] (POE-242) — what puts an incursion in scope, and so whether
//!   [`press`] arms the temple reader at all: a pure state machine over the
//!   Client.txt lines Alva and the temple area write, plus a thin `AppState`
//!   wrapper.
//! - The overlay (POE-171) lands as a sibling module and is the only intended
//!   consumer of this one.
//!
//! # Where the pixels live
//!
//! Two different kinds of image, kept apart by role rather than by directory
//! symmetry:
//!
//! - **`src/temple/assets/entrance-plate.png`** — the Entrance template the
//!   anchor correlates against, `include_bytes!`-embedded by [`anchor`]. It is
//!   production input: the reader cannot run without it, so it ships in the
//!   binary and must not sit under `tests/`.
//! - **`tests/fixtures/temple/board-*.png`** and **`diamond-*.png`** — real
//!   boards and real side-panel diamonds, loaded from `CARGO_MANIFEST_DIR` by
//!   the [`reader`] and [`markers`] tests. This follows the convention
//!   POE-165 established with `tests/fixtures/merc-skills-panel.png`; each
//!   fixture's source file and crop box is recorded in the `Fixture` struct of
//!   the test module that loads it ([`reader`] for the boards, [`markers`] for
//!   the diamonds).
//!
//! # Architecture decision this encodes
//!
//! **One base strategy, per-user configurables** (Sebastian, 2026-08-18). The
//! base is mechanics plus connectivity rules — scarcity logic that does not
//! depend on what the player is farming. Everything a player might disagree on
//! (how much the Apex is worth, whether traversal time is priced in, whether a
//! junk-vs-junk kill should reroll) is a **field of a profile, never a code
//! branch**. POE-257 widened what counts as such a field — the whole 25 x 3
//! room-value table, in chaos, now reaches the profile from a market read plus
//! the player's own overrides ([`preset`], [`valuation`], ADR-022) — and added
//! no branch: `strategy::StrategyProfile::locus_doryani_rush` is still the one
//! structure every profile is built on.

// POE-171 (this module's overlay loop and SSOT slice) is the live caller every
// sub-module was waiting for: `run` captures and reads, `slice` projects, and
// `commands` exposes the settings. Every file-level `#![allow(dead_code)]` and
// every per-item allow in `strategy` has therefore come off. The attributes
// that remain — one per item, in `anchor`, `reader`, `rooms`, `panel` and
// `advisor` — are the inventory of what only the tests still reach, and each
// comes off with its first production caller.

pub mod advisor;
pub mod anchor;
pub mod commands;
pub mod doors;
pub mod drops;
pub mod lattice;
pub mod market;
pub mod markers;
pub mod panel;
pub mod preset;
pub mod press;
pub mod reader;
pub mod rooms;
pub mod run;
pub mod slice;
pub mod strategy;
pub mod trigger;
pub mod valuation;

/// Why a screenshot did not yield a board.
///
/// There is deliberately no "read it anyway" variant. A layout the reader is
/// not sure of is worse than none: anchoring at NCC 0.829 and 0.809 on two
/// live boards each invented an Apex corridor that was actually closed, which
/// would have driven a confident and wrong recommendation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReadError {
    /// No Entrance plate matched well enough at any searched scale.
    AnchorNotFound {
        /// The best fine score seen, for logging. `-inf` when no scale
        /// produced a template that fits the image at all.
        best_ncc: f32,
    },
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::AnchorNotFound { best_ncc } => write!(
                f,
                "no temple layout panel found (best NCC {best_ncc:.3}, floor {:.2})",
                anchor::NCC_FLOOR
            ),
        }
    }
}

impl std::error::Error for ReadError {}

/// The temple module's runtime state, embedded by value in `AppState` as
/// `AppState.temple_state` (POE-342). Every field keeps its own lock or
/// atomic — the struct is a grouping, never a lock of its own.
pub struct TempleState {
    /// The temple module's persisted settings: the cached anchor calibration,
    /// the four tunable profile fields and the two config flags. Shared because
    /// two owners need it — the capture loop reads a snapshot per read and
    /// writes the calibration back, and the `temple_set_*` commands are what
    /// the user edits while that loop is running.
    pub(crate) settings: std::sync::Mutex<slice::TempleSettings>,
    /// The market read the temple valuation prices a board against (POE-258).
    ///
    /// Written ONLY by `temple::market::spawn_temple_market_poll` and by
    /// `temple::market::on_server_url_changed`; read through `temple::market::temple_market_now`,
    /// which is what applies the staleness judgement. Nothing reads this field
    /// directly — the raw payload carries `stale: false` as it arrived from the
    /// wire, and a board priced off it would be priced off a market of any age.
    ///
    /// Not a setting and not persisted: it is a copy of what a server said, and
    /// a copy reloaded from disk at the next launch would be a price with no
    /// server behind it. Acquired alone, like every other module-owned Mutex.
    pub(crate) market: std::sync::Mutex<market::MarketInput>,
    /// The valuation table computed from the preset, the Custom table and the
    /// market read above — stored so a read LOOKS IT UP instead of rebuilding
    /// it (POE-257 WI-3, 2026-09-07; the owner's "computed on update, and the
    /// module uses already computed weightings each time").
    ///
    /// Written and read through `temple::valuation::temple_valuation_now` alone, which is
    /// the total guard: it rebuilds the `temple::preset::ValuationKey` from the
    /// CALLER'S settings and the market on every call and hands back what is
    /// stored only when the two keys are equal. The five warm-up callers
    /// (`temple::valuation::warm_temple_valuation` from the market poll, from
    /// `temple::market::on_server_url_changed`, from `temple_set_preset` /
    /// `temple_set_custom` and from `reset_all_settings`) are what make a read's
    /// lookup a HIT; a missed one costs that read a recompute, never a wrong
    /// table.
    ///
    /// Not persisted, for the reason `market` is not: it is a function
    /// of a market read, and one reloaded from disk would be a valuation with
    /// no server behind it. Acquired ALONE — after the `market` guard
    /// has been dropped, never inside it — and never held across the table
    /// BUILD: the accessor takes it to compare keys, drops it, builds, and
    /// re-takes it to store, so a read is never queued behind another thread's
    /// 193-245 ms compute.
    pub(crate) valuation: std::sync::Mutex<Option<preset::CachedValuation>>,
    /// Whether Client.txt has put an incursion in scope (POE-242) — with, since
    /// 2026-09-07 (WI-1), the word the next stand-down is reported with
    /// (`temple::trigger::StandDown`). The single owner of "may the temple loop
    /// capture right now". Written by the log watcher on every line, by
    /// `temple_rearm`, and by the capture loop when it has read a board and
    /// watched the sheet close; read once per loop iteration. Acquired alone,
    /// like every other module-owned Mutex.
    ///
    /// Kept current even while the temple module is OFF: it is a fact about the
    /// game, and a player who switches the module on inside a temple must get a
    /// read without pressing anything.
    pub(crate) arm: std::sync::Mutex<trigger::ArmState>,
    /// Bumped by `temple_rearm` (and by every settings command that invalidates
    /// the current advice). The loop's read gate watches it to force one full
    /// re-read of a board it would otherwise skip as unchanged. An atomic, not
    /// a Mutex: it is read on every tick and never read together with the
    /// settings.
    ///
    /// **INVARIANT, stated at both fields:** this counter FORCES a read with
    /// nothing sighted; `epoch` only INVALIDATES one already read. An
    /// Alva line must never bump this one — that would promote an anchor
    /// attempt on an empty screen for every voice line, which is the
    /// free-running capture POE-242 removed.
    pub(crate) rearm: std::sync::atomic::AtomicU64,
    /// Bumped by `temple::trigger::on_client_line` on every CYCLE BOUNDARY
    /// (`trigger::ends_epoch`: a start line, any other Alva line, a zone change
    /// out of the temple) — POE-249.
    ///
    /// The board's identity: within one epoch no incursion has happened, so the
    /// sheet cannot have changed, and a sheet re-opened in the same epoch shows
    /// what was already read rather than being read again. The capture loop
    /// keys its board on it and INVALIDATES that board when it moves.
    ///
    /// **INVARIANT, stated at both fields:** `rearm` FORCES a read with
    /// nothing sighted; this one only invalidates. It must never be read as a
    /// reason to look at the screen — Alva speaks several times per incursion,
    /// and each of those lines would otherwise buy an anchor attempt over
    /// whatever the player happens to be doing.
    ///
    /// An atomic for the same reason as its sibling: read once per tick, never
    /// read together with anything else.
    pub(crate) epoch: std::sync::atomic::AtomicU64,
    /// The temple read worker's inbox (POE-389): `Some` while the module thread
    /// runs with an OCR engine, `None` otherwise. The read key's
    /// [`press::TEMPLE_READER`] hands each pressed frame down it; `None` means
    /// the temple takes no grab.
    pub(crate) press: std::sync::Mutex<Option<std::sync::mpsc::Sender<press::PressedFrame>>>,
}

impl TempleState {
    /// The one construction source for `run()` and `test_app_state`.
    /// Deliberately not a `Default` impl, like `test_app_state`: a new field
    /// breaking this constructor is the intended signal to decide whether it
    /// also needs a settings touch point.
    pub(crate) fn new() -> Self {
        Self {
            settings: std::sync::Mutex::new(slice::TempleSettings::default()),
            market: std::sync::Mutex::new(market::MarketInput::none()),
            valuation: std::sync::Mutex::new(None),
            arm: std::sync::Mutex::new(trigger::ArmState::default()),
            rearm: std::sync::atomic::AtomicU64::new(0),
            epoch: std::sync::atomic::AtomicU64::new(0),
            press: std::sync::Mutex::new(None),
        }
    }
}
