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
//!   Chase/Scarab [`strategy::Mode`] selector, and the two user-facing config
//!   flags. No Tauri, no state, no Windows code, no board graph.
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
//! - The advisor/board graph (POE-170) and the overlay (POE-171) land as
//!   sibling modules and are the only intended consumers of this one.
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
//! branch**.

// Every item in `strategy` is consumed by POE-169 (room identity) and POE-170
// (the advisor), neither of which exists yet, so in this slice only the unit
// tests reach the module. Each such item therefore carries its OWN
// `#[allow(dead_code)]` rather than the module carrying a blanket one: the
// attributes are the inventory of what is still uncalled, and it shrinks to
// nothing as the two consumers land instead of silently covering whatever is
// added next.
//
// POE-169 claimed `Line` (+ `key`/`named`), the four `KEY_*` keys and `Tier`;
// those attributes stay only because the module root is still unreachable from
// live code.
// Waiting on POE-170: `DOUBLE_TIER_CHANCE`, `Mode`, `ModeRule`, `Combination`,
// `StrategyProfile`, `highest_tier_per_line`, `TempleConfig`.

pub mod anchor;
pub mod doors;
pub mod lattice;
pub mod markers;
pub mod panel;
pub mod reader;
pub mod rooms;
pub mod strategy;

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
