//! Temple of Atzoatl **builder** advisor (POE-124 epic, POE-167 slice 0).
//!
//! Scope of this directory is the *building* phase — the two decisions a player
//! makes per incursion (which architect to kill, which passage to open) while
//! the temple is assembled across maps. Running the finished temple is a
//! separate helper and is not modelled here.
//!
//! # What lives where
//!
//! - [`strategy`] (this slice) — the pure objective function: the room-line
//!   vocabulary, the per-strategy [`strategy::StrategyProfile`], the
//!   Chase/Scarab [`strategy::Mode`] selector, and the two user-facing config
//!   flags. No Tauri, no state, no Windows code, no board graph.
//! - Room identity (POE-169) and the advisor/board graph (POE-170) land as
//!   sibling modules and are the only intended consumers of this one.
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
// Waiting on POE-169: `Line` (+ `key`/`named`), the four `KEY_*` keys, `Tier`.
// Waiting on POE-170: `DOUBLE_TIER_CHANCE`, `Mode`, `ModeRule`, `Combination`,
// `StrategyProfile`, `highest_tier_per_line`, `TempleConfig`.

pub mod strategy;
