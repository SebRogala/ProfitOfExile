//! The temple **builder** advisor: which architect to kill, which keys to
//! spend, and whether to leave the map (POE-170).
//!
//! # Shape of the problem
//!
//! One step under uncertainty. The only choices ever available are *(which
//! architect to kill, which doors to open — a set of 0, 1 or 2 keys, including
//! none)* in the room you are standing in. Everything else — which rooms
//! appear, where you land, what the upgrade rooms target — is **drawn, not
//! chosen** (Sebastian: *"there is no possibility to chase Doryani — nothing at
//! all that can be done"*). So the engine enumerates every legal option, prices
//! each by Monte-Carlo rollout over the random remainder, and then applies an
//! explicit rule layer on top.
//!
//! # Why the rule layer is not optional
//!
//! Measured on the real boards, the rollout splits the options R1/R2/RS decide
//! by 0.001–0.007 — a coin flip with no opinion. Every miss in the live
//! 11-board build was the rules layer, never the reader. See [`rules`].
//!
//! # What lives where
//!
//! - [`state`] — the board and the graph helpers (components, hop distance,
//!   RE's closed-door distance).
//! - [`rollout`] — the ported prototype: upgrade resolution, charge-relaxed
//!   reachability, scoring through [`StrategyProfile`], the drop pool, the
//!   default future-turn policy, and the seeded RNG.
//! - [`rules`] — the priority chain, RV as a hard constraint, RU/RD/RC/RT/R4,
//!   and the reasons every recommendation carries.
//!
//! The split is by *kind of authority*: `rollout` is a model of the game,
//! `rules` is a record of what Sebastian actually does. They disagree on real
//! boards, and keeping them in one file would hide which of the two decided.
//!
//! # Deviations from the spec, deliberately not closed
//!
//! - **The 11 live boards of §6d/§6e are not encoded as fixtures.** §6d records
//!   each board's *relations* — which rule beat which — and never its slot
//!   placement, so replaying them is impossible without inventing a board that
//!   would then be testing the invention. The priority-chain tests therefore
//!   rebuild the recorded relation on synthetic slots and say so; only the six
//!   §5 boards, which are walked room by room, are encoded verbatim in
//!   [`cases`].
//! - **R3, the budget gate, is not in the door chain.** `remaining` reaches the
//!   rollout as its horizon; R4 and its carve-out also require a future draw.
//!   The spec frames R3 as the `n` in the opportunity probability R1/R2 approximate
//!   rather than as a separate term, and §6e's *"RV at any budget"* removes the
//!   one place a budget term was expected to bite. Left out until a measured
//!   board disagrees with the chain because of the count.

// POE-171 (the overlay) is the only intended caller and does not exist yet, so
// the whole module is reached only by its own tests. One `allow` per file, as
// `lattice`/`reader` do: it comes off in one edit when POE-171 calls in.

mod cases;
mod fixtures;
// POE-171 calls `advise` on every completed read, so the file-level
// `#![allow(dead_code)]` this module shipped with is gone. The handful of items
// still reached only by tests — in `state`, `rollout` and `fixtures` — carry
// their own attribute instead.
pub mod convenience;
pub mod rollout;
pub mod rules;
pub mod state;

use std::collections::BTreeSet;

use crate::temple::lattice::{Edge, Slot};
use crate::temple::panel::{self, ArchitectOffer};
use crate::temple::rooms::{self, OfferKind};
use crate::temple::strategy::{Line, Mode, StrategyProfile, TempleConfig, Tier};

use rollout::{Estimate, Opening, Rng, Sim, Valuation};
use rules::{ArchKey, ArchitectChoice, Decision, DoorKey, Reason, RvVerdict};
use state::BoardState;

/// Whether to spend the rest of this map's incursions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapAction {
    /// Run the map's remaining entrances.
    Continue,
    /// R5 — leave. The next incursion of this map is guaranteed **not** to be
    /// the room just used, so running it burns budget for a zero chance at the
    /// target, and budget is future draws.
    LeaveMap,
}

/// A ranked recommendation.
#[derive(Debug, Clone)]
pub struct Ranked {
    /// The move.
    pub option: Decision,
    /// Mean finished-temple score over the rollouts.
    pub ev: f64,
    /// Which rules put it here. A bare score cannot be audited — live board 6
    /// was right for ambiguous reasons and nobody could tell.
    pub reasons: Vec<Reason>,
}

/// An option RV excluded, surfaced with its price rather than hidden.
///
/// Sebastian, 2026-08-18: *"show the user real estimated risk chance and let
/// user decide."*
#[derive(Debug, Clone)]
pub struct Gamble {
    /// The move.
    pub option: Decision,
    /// Mean finished-temple score over the rollouts.
    pub ev: f64,
    /// Fraction of rollouts that finished below the profile's "lost the room"
    /// threshold — the model's own number, not a hand-set warning.
    pub risk: f64,
    /// Which rules put it on this side of the line.
    pub reasons: Vec<Reason>,
}

/// Something the caller should know about the read that produced this advice.
#[derive(Debug, Clone, PartialEq)]
pub enum Warning {
    /// The panel was open between rooms, so there is no decision to make.
    NoPosition,
    /// No architect block resolved to a known room, so only the door half of
    /// the advice is modelled.
    UnresolvedArchitects,
    /// Fewer architect blocks were READ off the panel than the panel prints
    /// (POE-243).
    ///
    /// A different claim from [`Warning::UnresolvedArchitects`], which is about
    /// the vocabulary: this one is about the OCR. The panel always prints
    /// [`crate::temple::panel::ARCHITECTS_PER_PANEL`], so a read that produced
    /// one block did not see the other — and every kill the ranking then offers
    /// is the only kill there was, presented with the same confidence as a
    /// chosen one. That was invisible before this variant: a block that never
    /// parsed produced no warning at all, because `UnresolvedArchitects` fires
    /// only when EVERY offer is missing.
    PartialArchitects { read: usize, expected: usize },
    /// One architect's printed target did not resolve.
    UnresolvedOffer { printed: String },
    /// More keys than there are corridors worth opening.
    KeysUnspendable { keys: u8, usable: usize },
    /// `N Incursions Remaining` was not legible, so every rollout terminates
    /// immediately and the scores are the current board's, not a forecast.
    NoBudget,
    /// Neither source named the current room, and the selected
    /// [`crate::temple::facts::landing_tier`] rule needs its tier (POE-229).
    UnknownCurrentTier,
    /// The side-panel title and the current layout plate both read, and they
    /// named different rooms. The plate is the one used — it is positionally
    /// pinned and numeral-cross-checked — but one of the two OCR passes is
    /// wrong, and which one is not decidable, so the player is told.
    CurrentRoomDisagreement {
        title: &'static str,
        plate: &'static str,
    },
}

impl Warning {
    /// One line for the overlay.
    pub fn describe(&self) -> String {
        match self {
            Warning::NoPosition => "no current room on the panel — nothing to decide".to_string(),
            Warning::UnresolvedArchitects => {
                "neither architect's target was readable; only the door advice is modelled"
                    .to_string()
            }
            // Two wordings because the two states differ in what the player can
            // do about them. With one block read there IS a kill on screen and
            // the honest thing to say is that it was not chosen; with none, the
            // advice is doors only and saying "the kill shown is forced" would
            // point at a kill that is not there.
            Warning::PartialArchitects { read: 0, expected } => format!(
                "no architect block was read — the panel prints {expected}, so only the \
                 door advice is modelled"
            ),
            Warning::PartialArchitects { read, expected } => format!(
                "{read} of {expected} architects read — the kill shown is forced, not chosen"
            ),
            Warning::UnresolvedOffer { printed } => {
                format!("architect target {printed:?} is not a known room")
            }
            Warning::KeysUnspendable { keys, usable } => {
                format!("{keys} keys but only {usable} corridors worth opening")
            }
            Warning::NoBudget => {
                "incursions remaining was not legible; scores reflect the board as it stands"
                    .to_string()
            }
            Warning::UnknownCurrentTier => {
                "current room tier unknown — cannot resolve a 'change' offer".to_string()
            }
            Warning::CurrentRoomDisagreement { title, plate } => format!(
                "the side panel says {title:?} but the current plate says {plate:?}; \
                 the plate is used"
            ),
        }
    }
}

/// Everything one decision needs.
#[derive(Debug, Clone)]
pub struct Advice {
    /// The effective mode: Chase or Scarab from the profile's own selector, but
    /// always Chase while `strategy::SCARAB_OF_TIMELINES_ACTIVE` is false
    /// (3.30, POE-377). It sits **above** the
    /// per-decision ranking: it decides which *map* the budget is spent in,
    /// while the ranking decides which door.
    pub mode: Mode,
    /// Best first.
    pub recommendations: Vec<Ranked>,
    /// The RV-excluded options, best first.
    pub gambles: Vec<Gamble>,
    /// The corridor a SECOND Stone of Passage would buy, given the top
    /// recommendation — see [`conditional_second_door`], which owns both the
    /// meaning and every reason this is `None`.
    ///
    /// Beside the top recommendation and not inside [`Ranked`] on purpose: it
    /// is not a ranked move of its own, it is one fact about the move at the
    /// head of the list, and every option below the head would need a
    /// conditional ranking of its own to carry the field honestly.
    pub secondary_door: Option<Edge>,
    /// The door to open with the key when the top recommendation opens
    /// NOTHING — see [`convenience::convenience_door`], which owns the ranking
    /// (the walk it shortens) and every reason this is `None`.
    ///
    /// Beside the top recommendation for the same reason
    /// [`Self::secondary_door`] is: one fact about the move at the head of the
    /// list, not a move of its own. The two are exclusive by construction —
    /// the second stone's door needs a primary door to be second to, this one
    /// needs there to be none — so the widget's one faint seal is never asked
    /// to show both.
    pub convenience_door: Option<convenience::ConvenienceDoor>,
    /// R5's verdict for the top recommendation.
    pub map_action: MapAction,
    /// What was unreadable or unspendable.
    pub warnings: Vec<Warning>,
    /// The ranked kill that outranks every other preference, when one does.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub priority: Option<AdvicePriority>,
}

/// One kill the advice puts first whatever the rest of the ranking says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvicePriority {
    /// The offer's index on the panel, as [`ArchitectChoice::offer_index`].
    pub architect_index: usize,
    /// One short line saying why.
    pub reason: String,
}

/// Rank every legal move for the room the player is standing in.
///
/// `keys` is how many opening stones dropped — 0, 1 or 2. The panel does not
/// print it, so the caller supplies it; 1 is the common case and 0 is legal
/// (every passage from the room is already open, live boards 7–11).
pub fn advise(
    board: &BoardState,
    offers: &[ArchitectOffer],
    keys: u8,
    profile: &StrategyProfile,
    config: &TempleConfig,
    rollouts: u32,
    seed: u64,
) -> Advice {
    let valuation = Valuation::for_profile(profile);
    let mode = rollout::mode_of(board, profile);
    let mut warnings = Vec::new();
    if board.remaining == 0 {
        warnings.push(Warning::NoBudget);
    }
    if let Some((title, plate)) = board.current_room_disagreement {
        warnings.push(Warning::CurrentRoomDisagreement { title, plate });
    }

    let Some(position) = board.position else {
        warnings.push(Warning::NoPosition);
        return Advice {
            mode,
            recommendations: Vec::new(),
            gambles: Vec::new(),
            secondary_door: None,
            convenience_door: None,
            map_action: MapAction::Continue,
            warnings,
            priority: None,
        };
    };

    // Before resolution, because this is a claim about the OCR and not about
    // the vocabulary: it is true whether or not the block that WAS read
    // resolves. The gate is the evidence that the panel was on screen at all —
    // one block read is that evidence on its own; with none, the title is
    // (see `BoardState::panel_title_read`).
    if offers.len() < panel::ARCHITECTS_PER_PANEL && (!offers.is_empty() || board.panel_title_read) {
        warnings.push(Warning::PartialArchitects {
            read: offers.len(),
            expected: panel::ARCHITECTS_PER_PANEL,
        });
    }

    let architects = resolve_architects(offers, board.current_tier, config, &mut warnings);
    let choices: Vec<Option<ArchitectChoice>> = if architects.is_empty() {
        // "Neither target was readable" is a claim about the OCR, and it is
        // false when the targets read fine and it was the current room that did
        // not. `UnknownCurrentTier` already says the true thing; adding this on
        // top would send the player looking at the architect blocks.
        if !offers.is_empty() && !warnings.contains(&Warning::UnknownCurrentTier) {
            warnings.push(Warning::UnresolvedArchitects);
        }
        vec![None]
    } else {
        architects.into_iter().map(Some).collect()
    };

    // Enumeration is architect-dependent: the kill can make the current room a
    // degree-priced upgrade room, which widens the corridors worth a key.
    let door_sets_per_choice: Vec<_> = choices
        .iter()
        .map(|choice| rules::door_sets(board, position, keys, choice.as_ref()))
        .collect();
    let usable = door_sets_per_choice
        .iter()
        .flatten()
        .map(|s| s.len())
        .max()
        .unwrap_or(0);
    if usable < keys as usize {
        warnings.push(Warning::KeysUnspendable { keys, usable });
    }
    // RV governs the passage, so it only speaks when a passage can satisfy it.
    // Connectability reads reachability only, which no kill changes — the
    // per-choice enumerations differ only in degree-raising corridors that stay
    // inside one component, so any choice's sets answer it.
    let connectable = rules::rv_connectable(board, position, &door_sets_per_choice[0]);

    let (mut safe, mut risky) = score_options(
        board,
        position,
        &choices,
        &door_sets_per_choice,
        keys,
        profile,
        &valuation,
        config,
        rollouts,
        seed,
        connectable,
    );

    // RV never leaves the player with nothing to do. Unreachable since
    // `rv_connectable` gated the split — RV only speaks when some door set
    // satisfies it, and that set is Allowed for every architect, so `safe`
    // always holds something. Kept as the prototype's own guard
    // (`rv_filter`'s `kept or opts`) against the two predicates ever drifting
    // apart: an empty recommendation list is the one failure the overlay
    // cannot render.
    if safe.is_empty() {
        safe = std::mem::take(&mut risky);
    }

    let recommendations = rank(safe, profile);
    let priority = recommendations.first().and_then(|ranked| {
        let architect = ranked.option.architect.as_ref()?;
        let reason = ranked
            .reasons
            .iter()
            .find(|reason| **reason == Reason::ArchitectPriority)?;
        Some(AdvicePriority {
            architect_index: architect.offer_index,
            reason: reason.describe(),
        })
    });
    let mut gambles: Vec<Gamble> = risky
        .into_iter()
        .map(|scored| Gamble {
            option: scored.option,
            ev: scored.estimate.mean,
            risk: scored.estimate.risk,
            reasons: scored.reasons,
        })
        .collect();
    gambles.sort_by(|a, b| b.ev.total_cmp(&a.ev));

    let map_action = map_action(board, recommendations.first(), mode, config, &valuation);
    // One extra ranking per read, over the door just recommended and the pairs
    // that contain it — nothing else is priced. On case 8 that is TWO options
    // (the singleton and the board's one legal pair), measured at 3-7 ms
    // against 13-22 ms for the whole read at the production rollouts
    // (`the_conditional_ranking_cost_on_case_eight`, `#[ignore]`d).
    let secondary_door = recommendations
        .first()
        .and_then(|primary| {
            conditional_second_door(
                board,
                position,
                primary,
                keys,
                profile,
                &valuation,
                config,
                rollouts,
                seed,
                connectable,
            )
            .door
        });

    // Only when the move opens nothing, and only with a key to spend: the
    // faint seal is what to do with a key the move has no use for, and with a
    // door already recommended that seal is the second stone's.
    let convenience_door = recommendations
        .first()
        .filter(|primary| keys > 0 && primary.option.doors.is_empty())
        .and_then(|primary| {
            convenience::convenience_door(
                board,
                position,
                primary.option.architect.as_ref(),
                &valuation,
            )
        });

    Advice {
        mode,
        recommendations,
        gambles,
        secondary_door,
        convenience_door,
        map_action,
        warnings,
        priority,
    }
}

struct Scored {
    option: Decision,
    estimate: Estimate,
    door: DoorKey,
    architect_key: ArchKey,
    reasons: Vec<Reason>,
}

/// Price every (kill, door set) pair, split by RV's verdict.
///
/// Pulled out of [`advise`] so the CONDITIONAL ranking
/// ([`conditional_second_door`]) prices its options through exactly the same
/// chain — `evaluate_rules`, `rv_verdict`, the common-random-number rollout —
/// rather than through a second copy of it. A fork here would be two answers to
/// "what is this door worth", and the faint seal the overlay draws would drift
/// away from the bright one beside it with nothing to fail.
///
/// The RV fallback (*"an empty recommendation list is the one failure the
/// overlay cannot render"*) is deliberately NOT here: it belongs to the caller
/// that has to publish something whatever happens. The conditional caller has
/// the opposite duty — with nothing on the safe side it publishes nothing, so
/// the widget never marks an RV-excluded corridor as the door a second stone
/// buys.
#[allow(clippy::too_many_arguments)]
fn score_options(
    board: &BoardState,
    position: Slot,
    choices: &[Option<ArchitectChoice>],
    door_sets_per_choice: &[Vec<BTreeSet<Edge>>],
    keys: u8,
    profile: &StrategyProfile,
    valuation: &Valuation,
    config: &TempleConfig,
    rollouts: u32,
    seed: u64,
    connectable: bool,
) -> (Vec<Scored>, Vec<Scored>) {
    let paired_upgrade = choices.iter().flatten()
        .find(|a| a.kind == OfferKind::Upgrade).map(|a| a.built_tier);
    let base = Sim::from_board(board, valuation);
    let mut safe: Vec<Scored> = Vec::new();
    let mut risky: Vec<Scored> = Vec::new();

    for (architect, door_sets) in choices.iter().zip(door_sets_per_choice) {
        for doors in door_sets {
            let mut verdict = rules::evaluate_rules(
                board,
                position,
                architect.as_ref(),
                doors,
                keys,
                profile,
                valuation,
                paired_upgrade,
            );
            rules::limit_r4_by_grade(&mut verdict, choices);
            let rv = rules::rv_verdict(
                board,
                position,
                architect.as_ref().map(|a| &a.line),
                doors,
                valuation,
                connectable,
            );
            let opening = opening_for(position, architect.as_ref(), doors, valuation);
            // Common random numbers: every option is priced from the same seed.
            let mut rng = Rng::seeded(seed);
            let estimate = rollout::evaluate(
                &base,
                &opening,
                profile,
                valuation,
                config,
                rollouts,
                &mut rng,
            );
            let mut reasons = verdict.reasons.clone();
            let gamble = match rv {
                RvVerdict::Allowed(Some(reason)) => {
                    reasons.insert(0, reason);
                    false
                }
                RvVerdict::Allowed(None) => false,
                RvVerdict::Gamble(reason) => {
                    reasons.insert(0, reason);
                    true
                }
            };
            let scored = Scored {
                option: Decision {
                    architect: architect.clone(),
                    doors: doors.clone(),
                },
                estimate,
                door: verdict.door,
                architect_key: verdict.architect,
                reasons,
            };
            if gamble {
                risky.push(scored);
            } else {
                safe.push(scored);
            }
        }
    }
    (safe, risky)
}

/// What one conditional ranking answered, and what it cost.
///
/// The count is here because the cost claim in [`conditional_second_door`]'s
/// header is about the SIZE of the extra pass, and a wall-clock number cannot
/// hold anyone to it — the pass widening to every architect or every pair is a
/// change in how many options get priced, which is machine-independent and
/// assertable. `the_conditional_ranking_cost_on_case_eight` is what asserts it.
struct Conditional {
    /// The corridor a second stone would buy, or `None` — see
    /// [`conditional_second_door`] for every reason there is no answer.
    door: Option<Edge>,
    /// How many (kill, door set) options the pass priced.
    ///
    /// Read only by `the_conditional_ranking_cost_on_case_eight`, and that is
    /// the point: the alternative is a test that re-derives the enumeration it
    /// is checking, which asserts nothing. Comes off with its first production
    /// caller — a log line, most likely.
    #[allow(dead_code)]
    priced: usize,
}

impl Conditional {
    /// The question did not arise: nothing priced, nothing to say.
    fn unasked() -> Self {
        Conditional { door: None, priced: 0 }
    }
}

/// The corridor a SECOND Stone of Passage would buy, given the primary answer
/// (POE-248).
///
/// The overlay draws it as a faint purple seal beside the bright one, so the
/// player who finds a second stone mid-incursion already knows where it goes.
/// Since POE-253 this is the ONLY way a second stone is planned for: the
/// "opening stones dropped" setting is gone (stones drop after the sheet is
/// read, so it was a guess), and the caller always ranks with one key in hand.
///
/// # It is CONDITIONAL, and that is the whole definition
///
/// Not "the 2-key answer": that is a different move, and showing its two doors
/// would contradict the one door the widget is telling the player to open right
/// now. This re-ranks, at TWO keys, the primary door's own singleton against
/// every legal pair that contains it, and answers with the partner of the
/// winner — *given* the move being recommended, what does the second key add.
///
/// **The singleton is in the ranking on purpose, and it is the whole of the
/// "spend nothing more" answer.** [`DoorKey`] puts `ru_ok` above `opens`, so a
/// pair every RU vetoes loses to the un-vetoed one-door set even with two keys
/// in hand — which is the advisor saying *do not spend the second key on this
/// board*. Ranking only the pairs would have taken that answer away and
/// published the partner of the best RU-vetoed pair, i.e. exactly the corridor
/// the chain had just declined. When the singleton wins there is no partner and
/// the answer is `None`, which falls out of the same expression rather than
/// needing a branch.
///
/// The primary's own architect is the only one enumerated. The widget draws ONE
/// kill glyph, and a second door that only pays off under the other kill would
/// be a door the player cannot act on as drawn. It also keeps the enumeration
/// honest: [`rules::door_sets`] is architect-dependent (a kill can make this
/// room a degree-priced upgrade room), so the sets asked about here are the
/// sets the recommended kill actually creates.
///
/// `connectable` is the caller's, computed over the ONE-key sets, and reusing
/// it here is sound rather than convenient: every candidate corridor is
/// incident on the room the player is standing in, so a pair reaches the
/// Entrance component exactly when one of its two doors does — the predicate
/// cannot answer differently for a set built out of corridors it already saw.
///
/// # When it answers `None`
///
/// - `keys != 1`. At two the player HAS both stones and the advisor's own
///   answer is already the two-key one — including RU's answer that the second
///   is better not spent, which comes back as a ONE-door recommendation and is
///   not a primary waiting for a partner. At zero there is no first stone for a
///   second to follow, and the primary opens nothing anyway;
/// - the primary opens no door — there is nothing for a second key to be second
///   to;
/// - the primary's singleton wins the conditional ranking (above);
/// - every option is an RV gamble. The faint seal is advice, and RV's
///   exclusions are exactly the moves that must not be given one.
#[allow(clippy::too_many_arguments)]
fn conditional_second_door(
    board: &BoardState,
    position: Slot,
    primary: &Ranked,
    keys: u8,
    profile: &StrategyProfile,
    valuation: &Valuation,
    config: &TempleConfig,
    rollouts: u32,
    seed: u64,
    connectable: bool,
) -> Conditional {
    if keys != 1 {
        return Conditional::unasked();
    }
    let mut doors = primary.option.doors.iter();
    let Some(&first) = doors.next() else {
        return Conditional::unasked();
    };
    // Unreachable under the gate above — one key enumerates no two-door set —
    // and stated here so the ONE-door precondition is a property of this
    // function rather than something inferred from a caller's argument.
    if doors.next().is_some() {
        return Conditional::unasked();
    }

    let choices = [primary.option.architect.clone()];
    // Every set containing the primary door: its own singleton, which
    // `door_sets` enumerates at two keys as it does at one, and every pair the
    // redundancy filter left standing.
    let sets: Vec<BTreeSet<Edge>> = rules::door_sets(board, position, 2, choices[0].as_ref())
        .into_iter()
        .filter(|set| set.contains(&first))
        .collect();

    let (safe, risky) = score_options(
        board,
        position,
        &choices,
        std::slice::from_ref(&sets),
        2,
        profile,
        valuation,
        config,
        rollouts,
        seed,
        connectable,
    );
    let priced = safe.len() + risky.len();
    let Some(best) = rank(safe, profile).into_iter().next() else {
        return Conditional { door: None, priced };
    };
    Conditional {
        // `None` when the winner IS the singleton: there is no second door.
        door: best.option.doors.into_iter().find(|edge| *edge != first),
        priced,
    }
}

fn priority_architect(architect: &ArchitectChoice) -> bool {
    cradle_line(&architect.line)
}

fn cradle_line(line: &Line) -> bool {
    line.key() == rooms::CRADLE_LINE_KEY
}

/// Whether a read values its chests in the Cradle context (POE-470 D3): a
/// Cradle of Creation on `board`, or a panel kill in `offers` that builds on
/// its line.
pub fn cradle_in_play(board: &BoardState, offers: &[ArchitectOffer]) -> bool {
    board.rooms.iter().any(|(line, tier)| {
        *tier == Tier::T3 && line.as_ref().is_some_and(cradle_line)
    }) || offers.iter().any(|offer| {
        rooms::match_room_name(&offer.printed_target).identity()
            .and_then(|identity| identity.line())
            .is_some_and(|line| line.key() == rooms::CRADLE_LINE_KEY)
    })
}

/// Rank priority kills first, then by EV; rank door sets by the rule chain.
///
/// See [`rules`]'s module header for the measurement behind the split. Two
/// stages, in order:
///
/// 1. **The kill.** Each architect is priced by its best door set. Architects
///    R4's carve-out vetoed sort below every un-vetoed one whatever the EV says
///    — live board 8 is why. The rest are ordered by EV, and those inside
///    [`rules::noise_margin`] of the leader are re-ordered by the architect
///    rules — R0's "both architects are worthless, the kill is free" is exactly
///    this band. Resolved Cradle-line kills are then stably
///    promoted above this ordering, including R4's veto (POE-465).
/// 2. **The door set**, within each architect, by [`DoorKey`] with EV breaking
///    its ties.
///
/// The stages do not cross: a brilliant door can never promote a kill the
/// rollout says is 3 points worse. Case 2 is why — *"change to Jeweller's
/// Workshop, open the Apex"* scores R1-apex, and letting the door axis lead
/// would recommend Sebastian's own historical loss over upgrading the
/// corruption line.
fn rank(scored: Vec<Scored>, profile: &StrategyProfile) -> Vec<Ranked> {
    if scored.is_empty() {
        return Vec::new();
    }

    // Group by architect, keeping first-seen order so the result is stable.
    let mut groups: Vec<(Option<usize>, Vec<Scored>)> = Vec::new();
    for option in scored {
        let id = option.option.architect.as_ref().map(|a| a.offer_index);
        match groups.iter_mut().find(|(key, _)| *key == id) {
            Some((_, bucket)) => bucket.push(option),
            None => groups.push((id, vec![option])),
        }
    }

    // -- stage 1: the kill -------------------------------------------------
    let mut summaries: Vec<(usize, Estimate, ArchKey)> = groups
        .iter()
        .enumerate()
        .map(|(index, (_, bucket))| {
            let best = bucket
                .iter()
                .max_by(|a, b| a.estimate.mean.total_cmp(&b.estimate.mean))
                .expect("a group is never empty");
            (index, best.estimate, best.architect_key)
        })
        .collect();
    summaries.sort_by(|a, b| {
        a.2.vetoed()
            .cmp(&b.2.vetoed())
            .then_with(|| b.1.mean.total_cmp(&a.1.mean))
    });
    let leader = summaries[0].1;
    let vetoed_leader = summaries[0].2.vetoed();
    let cutoff = leader.mean - rules::noise_margin(profile, leader.stderr);
    let band = summaries
        .iter()
        .position(|(_, estimate, key)| key.vetoed() != vetoed_leader || estimate.mean < cutoff)
        .unwrap_or(summaries.len());
    // The grade ladder already orders D above E in chaos. Its live values
    // can be too small for Monte-Carlo means to distinguish, and same-letter
    // positions deliberately share a price. Keep that price contract: use
    // the existing rank only when EVERY contender in the band is fallback,
    // after the architect rules tie. Clear EV differences remain authoritative.
    let fallback_ranks: Vec<_> = groups
        .iter()
        .map(|(_, bucket)| {
            let architect = bucket[0].option.architect.as_ref()?;
            let fallback = profile.grade_fallback.get(&architect.line)?;
            let tier_index = architect.built_tier.get().checked_sub(1)? as usize;
            if !fallback[tier_index] {
                return None;
            }
            rooms::LINES
                .iter()
                .find(|line| line.mechanical_line() == architect.line)
                .map(|line| line.rank())
        })
        .collect();
    let grade_band = summaries[..band]
        .iter()
        .all(|(index, _, _)| fallback_ranks[*index].is_some());
    summaries[..band].sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| {
                if grade_band {
                    fallback_ranks[a.0].cmp(&fallback_ranks[b.0])
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| b.1.mean.total_cmp(&a.1.mean))
    });

    // The band ends either at the EV cutoff or where the veto flips. Only the
    // first is the rollout's doing: a vetoed kill sits below an un-vetoed one
    // whatever its EV, so neither side of a veto boundary was placed by EV.
    let ev_cut = summaries
        .get(band)
        .is_some_and(|(_, _, key)| key.vetoed() == vetoed_leader);

    let priorities: Vec<_> = groups
        .iter()
        .map(|(_, bucket)| bucket[0].option.architect.as_ref().is_some_and(priority_architect))
        .collect();
    let priority_decided = priorities.contains(&true) && priorities.contains(&false);
    let mut ordered: Vec<_> = summaries
        .iter()
        .enumerate()
        .map(|(rank, (index, _, key))| {
            // Outside the band, on the leader's side of the veto, the EV alone put
            // this kill where it is; at rank 0 with a band of one cut by the EV,
            // the EV alone put the *leader* where it is, which is just as much a
            // fact about the ranking and just as auditable. A band that covers
            // every architect is the opposite claim — the rollout had no opinion —
            // and a band cut by the veto is the veto's claim, not the rollout's,
            // so `ExpectedValue` must stay off both.
            let ev_decided = if rank >= band {
                key.vetoed() == vetoed_leader
            } else {
                band == 1 && ev_cut
            };
            let grade_decided = rank < band
                && grade_band
                && summaries[..band].iter().any(|(other, _, other_key)| {
                    *other_key == *key && fallback_ranks[*other] != fallback_ranks[*index]
                });
            (*index, ev_decided, grade_decided)
        })
        .collect();
    ordered.sort_by_key(|(index, _, _)| !priorities[*index]);

    // -- stage 2: the door set --------------------------------------------
    let mut out = Vec::new();
    for (index, ev_decided, grade_decided) in ordered {
        let mut bucket = std::mem::take(&mut groups[index].1);
        bucket.sort_by(|a, b| {
            b.door
                .cmp(&a.door)
                .then_with(|| b.estimate.mean.total_cmp(&a.estimate.mean))
        });
        out.extend(bucket.into_iter().map(|option| {
            let mut reasons = option.reasons;
            if ev_decided && !priority_decided {
                reasons.insert(0, Reason::ExpectedValue);
            } else if grade_decided && !priority_decided {
                reasons.insert(0, Reason::GradeFallback);
            }
            if priorities[index] {
                reasons.insert(0, Reason::ArchitectPriority);
            }
            Ranked {
                option: option.option,
                ev: option.estimate.mean,
                reasons,
            }
        }));
    }
    out
}

/// Resolve both architect blocks through [`rooms::resolve_offer_for`].
/// It delegates landing to [`crate::temple::facts::landing_tier`], including
/// deciding whether an unread current tier prevents resolution.
fn resolve_architects(
    offers: &[ArchitectOffer],
    current_tier: Option<Tier>,
    config: &TempleConfig,
    warnings: &mut Vec<Warning>,
) -> Vec<ArchitectChoice> {
    let mut out = Vec::new();
    for (index, offer) in offers.iter().enumerate() {
        match rooms::resolve_offer_for(&offer.printed_target, offer.kind, current_tier, config.played_consolidation()) {
            rooms::OfferResolution::Built(resolved) => out.push(ArchitectChoice {
                offer_index: index,
                architect_name: offer.architect_name.clone(),
                kind: offer.kind,
                line: resolved.line.mechanical_line(),
                built_tier: resolved.built_tier,
                display_name: resolved.display_name,
            }),
            rooms::OfferResolution::UnknownName => warnings.push(Warning::UnresolvedOffer {
                printed: offer.printed_target.clone(),
            }),
            rooms::OfferResolution::UnknownCurrentTier => {
                if !warnings.contains(&Warning::UnknownCurrentTier) {
                    warnings.push(Warning::UnknownCurrentTier);
                }
            }
        }
    }
    out
}

fn opening_for(
    position: Slot,
    architect: Option<&ArchitectChoice>,
    doors: &BTreeSet<Edge>,
    valuation: &Valuation,
) -> Opening {
    Opening {
        slot: position.index(),
        kill: architect.map(|a| (valuation.tag(&a.line), a.kind, a.built_tier)),
        doors: doors
            .iter()
            .map(|edge| {
                let (a, b) = edge.ends();
                (a.index(), b.index())
            })
            .collect(),
    }
}

/// R5 — leave the map once the target room has been used.
///
/// Narrowed to what is actually guaranteed (Sebastian, 2026-08-07: *"not all
/// rooms — it's more that 'next'"*): only the immediately next incursion is
/// certain to differ. That is still enough — staying spends a guaranteed-zero
/// draw against the target — but the earlier "0% for the whole map" claim was
/// too strong.
///
/// Three conditions, all necessary:
///
/// - [`Mode::Chase`]: in Scarab mode tiers stop mattering, because the itemised
///   copy re-rolls them, and the scarab needs every entrance completed anyway
///   (Scarab mode is held inactive in 3.30, so this always holds);
/// - [`TempleConfig::r5_applies`]: the Incursion Scarab of Timelines takes the
///   choice away (ignored in 3.30, POE-377);
/// - the recommended kill advanced a still-missing target line to **below tier
///   3**. At tier 3 the room has left the drop pool on its own and R5 has
///   nothing left to protect.
///
/// **Withheld on the wire since 2026-09-07** (owner): still computed and tested
/// here, but `slice::R5_WITHHELD` publishes `continue` whatever this returns,
/// until the rule is reworked.
fn map_action(
    board: &BoardState,
    top: Option<&Ranked>,
    mode: Mode,
    config: &TempleConfig,
    valuation: &Valuation,
) -> MapAction {
    if mode != Mode::Chase || !config.r5_applies() {
        return MapAction::Continue;
    }
    let Some(top) = top else {
        return MapAction::Continue;
    };
    let Some(architect) = &top.option.architect else {
        return MapAction::Continue;
    };
    if !valuation.is_target(valuation.tag(&architect.line)) {
        return MapAction::Continue;
    }
    if architect.built_tier == Tier::T3 {
        return MapAction::Continue;
    }
    // Chase is the effective mode, forced while the scarab is inactive, so it
    // no longer implies a missing target line; leaving only makes sense while
    // this one is what the map still owes.
    let connected = board.connected_rooms();
    let already_banked = connected
        .iter()
        .any(|(line, tier)| *line == architect.line && *tier == Tier::T3);
    if already_banked {
        MapAction::Continue
    } else {
        MapAction::LeaveMap
    }
}

/// The lines a decision names, for callers that only want the headline.
impl Decision {
    /// `"upgrade → Locus of Corruption"`, or `"kill either"` when no architect
    /// resolved.
    pub fn headline(&self) -> String {
        match &self.architect {
            Some(a) => format!(
                "{} → {}",
                match a.kind {
                    OfferKind::Change => "change",
                    OfferKind::Upgrade => "upgrade",
                },
                a.display_name
            ),
            None => "kill either".to_string(),
        }
    }

    /// `"C1, C2"`, or `"no door"`.
    pub fn doors_label(&self) -> String {
        if self.doors.is_empty() {
            return "no door".to_string();
        }
        self.doors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::cases::{self, opened_toward};
    use super::fixtures::{board, junk_offers, offer, JUNK};
    use super::rules::Reason;
    use super::*;
    use crate::temple::lattice::Slot::*;
    use crate::temple::rooms::OfferKind::{Change, Upgrade};
    use crate::temple::rooms::CRADLE_LINE_KEY;
    use crate::temple::strategy::{Consolidation, Line, ModeRule, StrategyProfile};

    /// Rollouts per option in the tests.
    ///
    /// 400 keeps the whole advisor suite under a second while leaving the
    /// architect axis — the only axis the rollout decides — separated by 3+
    /// points on every board that separates at all. The door axis is decided by
    /// the rule chain and does not depend on this number.
    const N: u32 = 400;
    /// One fixed seed, so every assertion below is reproducible.
    const SEED: u64 = 7;

    fn rush() -> StrategyProfile {
        StrategyProfile::cradle_apex()
    }

    /// The default config with the Consolidation of Power option set to
    /// `option` (`None` is unset, which plays option 1).
    fn played(option: Option<Consolidation>) -> TempleConfig {
        TempleConfig {
            consolidation: option,
            ..TempleConfig::default()
        }
    }

    /// The architect choice the advice made for one offer block, wherever it
    /// ranked, or `None` when the block never reached the ranking.
    fn choice_for(advice: &Advice, offer_index: usize) -> Option<&ArchitectChoice> {
        advice
            .recommendations
            .iter()
            .map(|r| &r.option)
            .chain(advice.gambles.iter().map(|g| &g.option))
            .filter_map(|option| option.architect.as_ref())
            .find(|a| a.offer_index == offer_index)
    }

    /// The nine walked boards: the eight Sebastian decided, plus the one the
    /// app got wrong, which is a real board and therefore a real regression
    /// surface for the production profile.
    fn all_walked_boards() -> Vec<cases::Case> {
        let mut boards = cases::retrospective();
        boards.push(cases::case_7_armourers_workshop());
        boards
    }

    /// The profile the SHIPPED app ranks with (POE-257): the settings defaults
    /// bridged over a cold valuation — the one compute `run::full_read`'s
    /// lookup resolves to (`preset::value_table` via
    /// `valuation::temple_valuation_now`) whenever POE-258's poll has no prices in
    /// hand. Every room is priced at
    /// its cold grade rung, so the whole board carries a value where the fixture
    /// priced four lines and left 21 at zero.
    fn production_profile() -> StrategyProfile {
        crate::temple::slice::TempleProfileSettings::default().to_profile(
            &crate::temple::slice::value_read(
                &crate::temple::slice::TempleSettings::default(),
                &crate::temple::market::MarketInput::none(),
            ),
        )
    }

    /// The same bridge over the COMMITTED ALLFLAME CAPTURE — what the app ranks
    /// with whenever POE-258's poll is holding prices.
    ///
    /// The difference from [`production_profile`] is not cosmetic: the cold
    /// ladder tops out at 800 with every room on a five-tier scale, while the
    /// capture pays 846 for Locus and puts thirteen rooms under 10 c. A board
    /// that decides the same way under both is a board the market cannot move.
    fn live_production_profile() -> StrategyProfile {
        crate::temple::slice::TempleProfileSettings::default().to_profile(
            &crate::temple::slice::value_read(
                &crate::temple::slice::TempleSettings::default(),
                &crate::temple::market::allflame(),
            ),
        )
    }

    /// Zero rollouts isolate the rule ranking from future-policy EV. The
    /// change offer is first so only an active preference can promote upgrade.
    fn grade_limited_r4_scores(offers: &[(OfferKind, &str)], carve_out: bool) -> Vec<Scored> {
        let current_line = offers.iter()
            .find(|(kind, _)| *kind == Upgrade)
            .map_or("museum_of_artefacts", |(_, line)| *line);
        let mut state = board(
            &[(D1, current_line, 1), (C2, "corruption", 1)],
            &[(D1, E1)], D1, 5,
        );
        if carve_out {
            state.set_room(C1, Some(Line::Upgrade), Tier::T2);
            state.doors.insert(Edge::new(C1, D1));
        }
        // Option 1's landing from the tier-1 room: upgrade 3, change 1.
        let choices: Vec<_> = offers.iter().enumerate().map(|(index, (kind, line))| {
            Some(ArchitectChoice {
                offer_index: index,
                architect_name: "fixture".to_string(),
                kind: *kind,
                line: Line::named(line),
                built_tier: if *kind == Upgrade { Tier::T3 } else { Tier::T1 },
                display_name: "fixture",
            })
        }).collect();
        let sets = vec![vec![BTreeSet::new()]; choices.len()];
        let mut profile = rush();
        profile.r4_keep_upgrade_targets = carve_out;
        let valuation = Valuation::for_profile(&profile);
        let (scored, _) = score_options(
            &state, D1, &choices, &sets, 0, &profile, &valuation,
            &TempleConfig::default(), 0, SEED, false,
        );
        scored
    }

    fn grade_limited_r4(offers: &[(OfferKind, &str)], carve_out: bool) -> Vec<Ranked> {
        rank(grade_limited_r4_scores(offers, carve_out), &rush())
    }

    fn assert_upgrade_r4(offers: &[(OfferKind, &str)], expected: bool) {
        let ranked = grade_limited_r4(offers, false);
        let upgrade = ranked.iter().find(|option| {
            option.option.architect.as_ref().unwrap().kind == Upgrade
        }).expect("fixture has an upgrade");
        assert_eq!(upgrade.reasons.contains(&Reason::R4), expected);
        assert_eq!(ranked[0].option.architect.as_ref().unwrap().kind,
            if expected { Upgrade } else { Change });
    }

    #[test]
    fn storage_upgrade_loses_r4_against_cultivar_two_letters_better() {
        assert_upgrade_r4(&[(Change, "toxic_grove"), (Upgrade, "museum_of_artefacts")], false);
    }

    #[test]
    fn the_grade_limit_also_handles_upgrade_first() {
        let ranked = grade_limited_r4(
            &[(Upgrade, "museum_of_artefacts"), (Change, "toxic_grove")], false,
        );
        let upgrade = ranked.iter().find(|option| {
            option.option.architect.as_ref().unwrap().kind == Upgrade
        }).unwrap();
        assert!(!upgrade.reasons.contains(&Reason::R4));
    }

    #[test]
    fn torment_upgrade_loses_r4_against_defense_two_letters_better() {
        assert_upgrade_r4(&[(Change, "defense_research_lab"), (Upgrade, "sadists_den")], false);
    }

    #[test]
    fn armoury_upgrade_keeps_r4_against_reactor_one_letter_better() {
        assert_upgrade_r4(&[(Change, "conduit_of_lightning"), (Upgrade, "chamber_of_iron")], true);
    }

    #[test]
    fn r4_keeps_a_change_offer_worse_than_the_upgrade() {
        assert_upgrade_r4(&[(Change, "museum_of_artefacts"), (Upgrade, "factory")], true);
    }

    #[test]
    fn r4_keeps_an_equal_letter_change_offer() {
        assert_upgrade_r4(&[(Change, "court_of_sealed_death"), (Upgrade, "museum_of_artefacts")], true);
    }

    #[test]
    fn a_lone_upgrade_keeps_r4() {
        assert_upgrade_r4(&[(Upgrade, "museum_of_artefacts")], true);
    }

    #[test]
    fn an_unranked_change_preserves_r4() {
        assert_upgrade_r4(&[(Change, "unread_line"), (Upgrade, "museum_of_artefacts")], true);
    }

    #[test]
    fn an_unranked_upgrade_preserves_r4() {
        assert_upgrade_r4(&[(Change, "conduit_of_lightning"), (Upgrade, "unread_line")], true);
    }

    #[test]
    fn two_upgrade_offers_preserve_r4() {
        let ranked = grade_limited_r4(
            &[(Upgrade, "museum_of_artefacts"), (Upgrade, "conduit_of_lightning")], false,
        );
        assert_eq!(ranked.len(), 2);
        assert!(ranked.iter().all(|option| option.reasons.contains(&Reason::R4)));
    }

    #[test]
    fn two_change_offers_preserve_existing_pool_removal_r4() {
        let mut state = board(&[(D1, "museum_of_artefacts", 2)], &[(D1, E1)], D1, 5);
        state.current_tier = Some(Tier::T2);
        let offers = [offer("fixture", Change, "Cultivar Chamber"),
            offer("fixture", Change, "Grand Reactor")];
        // Option 2 lands a change from tier 2 at tier 3, out of the pool.
        let advice = advise(
            &state, &offers, 0, &rush(), &played(Some(Consolidation::ResidentCarry)), 0, SEED,
        );
        assert_eq!(advice.recommendations.len(), 2);
        assert!(advice.recommendations.iter().all(|option| option.reasons.contains(&Reason::R4)));
    }

    #[test]
    fn the_grade_limit_preserves_the_carve_out_veto() {
        let scored = grade_limited_r4_scores(
            &[(Change, "toxic_grove"), (Upgrade, "museum_of_artefacts")], true,
        );
        let upgrade = scored.iter().find(|option| {
            option.option.architect.as_ref().unwrap().kind == Upgrade
        }).unwrap();
        assert!(upgrade.architect_key.vetoed());
        assert!(upgrade.reasons.contains(&Reason::R4CarveOutVeto { upgrader: C1 }));
    }

    #[test]
    fn the_grade_limit_preserves_the_carve_out_pool_preference() {
        let scored = grade_limited_r4_scores(
            &[(Change, "toxic_grove"), (Upgrade, "museum_of_artefacts")], true,
        );
        let change = scored.iter().find(|option| {
            option.option.architect.as_ref().unwrap().kind == Change
        }).unwrap();
        assert_eq!(change.architect_key.0[4], 1);
        assert!(change.reasons.contains(&Reason::R4PoolCarveOut { upgrader: C1 }));
    }

    /// Synthetic rollout estimates isolate the noise-band regression from
    /// sampling and from any legitimate downstream board effect. Values and
    /// fallback provenance still come through the production valuation bridge.
    fn grade_pair(better: &str, worse: &str, gap: f64, priced: bool) -> Vec<Ranked> {
        let profile = if priced {
            let overrides = crate::temple::valuation::RoomOverrides::from([
                (better.to_string(), [Some(0.001), None, None]),
            ]);
            let valued = crate::temple::valuation::Valued::compute_with(
                &crate::temple::market::allflame(),
                &crate::temple::valuation::Knobs::default(),
                &overrides,
            );
            crate::temple::slice::TempleProfileSettings::default().to_profile(&valued)
        } else {
            live_production_profile()
        };
        let state = board(&[], &[], D1, 3);
        let valuation = Valuation::for_profile(&profile);
        let scored = [(better, -1504.59), (worse, -1504.59 + gap)]
            .into_iter().enumerate().map(|(offer_index, (line, mean))| {
                let architect = ArchitectChoice {
                    offer_index,
                    architect_name: "fixture".to_string(),
                    kind: Change,
                    line: Line::named(line),
                    built_tier: Tier::T1,
                    display_name: "fixture",
                };
                let verdict = rules::evaluate_rules(
                    &state, D1, Some(&architect), &BTreeSet::new(), 0,
                    &profile, &valuation, Some(architect.built_tier),
                );
                Scored {
                    option: Decision { architect: Some(architect), doors: BTreeSet::new() },
                    estimate: Estimate { mean, risk: 0.0, stderr: 10.0 },
                    door: verdict.door,
                    architect_key: verdict.architect,
                    reasons: verdict.reasons,
                }
            }).collect();
        rank(scored, &profile)
    }

    #[test]
    fn fallback_d2_beats_e4_inside_rollout_noise() {
        let ranked = grade_pair("sadists_den", "court_of_sealed_death", 12.16, false);
        assert_eq!(ranked[0].option.architect.as_ref().unwrap().line, Line::named("sadists_den"));
        assert!(ranked[0].reasons.contains(&Reason::GradeFallback));
    }

    #[test]
    fn the_fallback_band_reason_names_no_grade_letter() {
        let ranked = grade_pair("sadists_den", "court_of_sealed_death", 12.16, false);
        let text = ranked[0]
            .reasons
            .iter()
            .find(|reason| matches!(reason, Reason::GradeFallback))
            .expect("the fallback band decided this pair")
            .describe();
        assert_eq!(
            text,
            "unpriced fallback: rollouts within noise, so the sheet's room order breaks equal architect rules"
        );
        let bytes = text.as_bytes();
        let letter = bytes.windows(2).any(|w| (b'A'..=b'E').contains(&w[0]) && w[1].is_ascii_digit());
        assert!(!letter, "reason prints a grade letter: {text}");
        assert!(!text.to_lowercase().contains("grade"), "reason names the grade: {text}");
    }

    #[test]
    fn fallback_d3_beats_e3_inside_rollout_noise() {
        let ranked = grade_pair("apex_of_ascension", "wealth_of_the_vaal", 12.16, false);
        assert_eq!(ranked[0].option.architect.as_ref().unwrap().line, Line::named("apex_of_ascension"));
    }

    #[test]
    fn equal_letter_fallbacks_follow_the_existing_pick_order() {
        let ranked = grade_pair("wealth_of_the_vaal", "court_of_sealed_death", 12.16, false);
        assert_eq!(ranked[0].option.architect.as_ref().unwrap().line, Line::named("wealth_of_the_vaal"));
    }

    #[test]
    fn a_clear_rollout_difference_is_not_overridden_by_grade() {
        let ranked = grade_pair("sadists_den", "court_of_sealed_death", 31.0, false);
        assert_eq!(ranked[0].option.architect.as_ref().unwrap().line, Line::named("court_of_sealed_death"));
        assert!(ranked[0].reasons.contains(&Reason::ExpectedValue));
    }

    #[test]
    fn a_per_tier_override_does_not_borrow_the_fallback_grade_order() {
        let ranked = grade_pair("sadists_den", "court_of_sealed_death", 12.16, true);
        assert_eq!(ranked[0].option.architect.as_ref().unwrap().line, Line::named("court_of_sealed_death"));
        assert!(!ranked[0].reasons.iter().any(|reason| matches!(reason, Reason::GradeFallback)));
    }

    /// Synthetic board with the report's displayed 36c/41c and an explicit
    /// 0.6 lower-tier fraction. It is not a replay of the missing market read.
    #[test]
    fn the_last_incursion_prefers_the_higher_double_tier_expected_value() {
        let state = board(&[(D1, "sanctum_of_immortality", 1)], &[(D1, E1)], D1, 1);
        let offers = vec![
            offer("fixture", Upgrade, "Sanctum of Vitality"),
            offer("fixture", Change, "Flame Workshop"),
        ];
        let mut profile = rush();
        profile.room_values.insert(Line::named("sanctum_of_immortality"), [36.0, 36.0, 60.0]);
        profile.room_values.insert(Line::named("crucible_of_flame"), [41.0, 41.0, 41.0 / 0.6]);
        let advice = advise(&state, &offers, 0, &profile, &TempleConfig::default(), N, SEED);
        let top = &advice.recommendations[0];
        assert_eq!(top.option.architect.as_ref().unwrap().kind, Upgrade);
    }

    #[test]
    fn expected_value_explanation_allows_eligible_architect_rules() {
        assert_eq!(Reason::ExpectedValue.describe(),
            "architect ranked on expected value; architect tie-breaks did not decide it");
    }

    /// What one board's top recommendation kills, as
    /// `(kind, line key, built tier)`, or `None` for a door-only pick.
    fn top_kill_of(advice: &Advice) -> Option<(crate::temple::rooms::OfferKind, String, u8)> {
        let architect = advice.recommendations[0].option.architect.as_ref()?;
        Some((
            architect.kind,
            architect.line.key().to_string(),
            architect.built_tier.get(),
        ))
    }

    fn advise_case_with(case: &cases::Case, profile: &StrategyProfile) -> Advice {
        advise(
            &case.state,
            &case.offers,
            case.keys,
            profile,
            &TempleConfig::default(),
            N,
            SEED,
        )
    }

    /// `case` at this suite's `N`/`SEED` under `profile` and `config`.
    fn advise_case_under(case: &cases::Case, profile: &StrategyProfile, config: &TempleConfig) -> Advice {
        advise(&case.state, &case.offers, case.keys, profile, config, N, SEED)
    }

    fn advise_case(case: &cases::Case) -> Advice {
        advise(
            &case.state,
            &case.offers,
            case.keys,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        )
    }

    /// The far endpoints of the doors the best recommendation opens.
    fn top_doors(advice: &Advice, from: Slot) -> Vec<Slot> {
        opened_toward(&advice.recommendations[0].option.doors, from)
    }

    fn has_reason(ranked: &Ranked, want: impl Fn(&Reason) -> bool) -> bool {
        ranked.reasons.iter().any(want)
    }

    // ================================================= the seven walked boards

    // R1 (Chamber of Iron is the only neighbour above Tombs) AND R2 (it merges
    // two singletons rather than attaching one room to the 10-cluster). Both
    // architects are worthless, so the kill is free — R0.
    #[test]
    fn case_one_tombs_opens_upward_into_the_singleton_rather_than_the_main_cluster() {
        let case = cases::case_1_tombs();
        let advice = advise_case(&case);
        assert_eq!(
            top_doors(&advice, D3),
            vec![C2],
            "{}: Sebastian's move was {}",
            case.name,
            case.decision
        );
        let top = &advice.recommendations[0];
        assert!(
            has_reason(top, |r| matches!(r, Reason::R2 { joined: 2 })),
            "the merge is two singletons, not an attach to the 10-cluster: {:?}",
            top.reasons
        );
        assert!(
            has_reason(top, |r| matches!(r, Reason::R1Gradient { row: 2 })),
            "Chamber of Iron is row C, one above Tombs: {:?}",
            top.reasons
        );
    }

    // R0 — with both architects offering junk the kill is a free choice. That
    // is a claim about the *rollout*: the two kills must land inside the noise
    // band, which is exactly the case in which no recommendation is labelled
    // [`Reason::ExpectedValue`]. Asserting only that the two door labels match
    // would pass on any board, free kill or not.
    #[test]
    fn case_one_prices_both_worthless_architects_inside_the_noise_band() {
        let case = cases::case_1_tombs();
        let advice = advise_case(&case);
        assert!(
            advice
                .recommendations
                .iter()
                .all(|r| !has_reason(r, |x| matches!(x, Reason::ExpectedValue))),
            "a free kill means the rollout separated nothing: {:?}",
            advice.recommendations[0].reasons
        );
        let best_per_architect: Vec<String> = [0usize, 1]
            .into_iter()
            .map(|index| {
                advice
                    .recommendations
                    .iter()
                    .find(|r| r.option.architect.as_ref().expect("resolved").offer_index == index)
                    .expect("both architects must be ranked")
                    .option
                    .doors_label()
            })
            .collect();
        assert_eq!(
            best_per_architect[0], best_per_architect[1],
            "a free kill must not change the door advice"
        );
    }

    // The Corruption Chamber is an ordinary room under the Cradle target set,
    // so RV has nothing stranded to protect and the Apex door — the move
    // Sebastian played on the 3.29 board — is the pick, reaching the Apex.
    #[test]
    fn case_two_corruption_chamber_takes_the_apex_now_the_room_is_no_target() {
        let case = cases::case_2_corruption_chamber();
        let advice = advise_case(&case);
        let top = &advice.recommendations[0];
        let architect = top.option.architect.as_ref().expect("resolved");
        assert_eq!(architect.kind, Upgrade, "{}", case.decision);
        assert_eq!(architect.line, Line::Corruption);
        assert_eq!(top_doors(&advice, B1), vec![A0], "{}", case.decision);
        assert!(has_reason(top, |r| matches!(
            r,
            Reason::R1Apex { reaches_apex: true }
        )));
    }

    // With no target room stranded, RV files no option as a gamble: the Apex
    // play is an ordinary recommendation.
    #[test]
    fn case_two_files_no_gamble_now_that_no_target_room_is_stranded() {
        let case = cases::case_2_corruption_chamber();
        let advice = advise_case(&case);
        assert!(
            advice.gambles.is_empty(),
            "{:?}",
            advice.gambles.iter().map(|g| g.option.headline()).collect::<Vec<_>>()
        );
    }

    // Case 2's board one step on: nothing on it is stranded, so nothing
    // outranks the Apex.
    #[test]
    fn case_three_chasm_takes_the_apex_once_the_locus_is_banked() {
        let case = cases::case_3_chasm_late();
        let advice = advise_case(&case);
        assert_eq!(top_doors(&advice, B0), vec![A0], "{}", case.decision);
        assert!(has_reason(&advice.recommendations[0], |r| matches!(
            r,
            Reason::R1Apex { .. }
        )));
    }

    // No Cradle-line room is connected on this board, and the Incursion Scarab
    // of Timelines is gone in 3.30 (POE-377): the advisor stays in Chase.
    #[test]
    fn case_three_stays_in_chase_mode_while_the_scarab_is_inactive() {
        let advice = advise_case(&cases::case_3_chasm_late());
        assert_eq!(advice.mode, Mode::Chase);
    }

    // Pointless doors: the other two corridors from Chasm join rooms already in
    // its own component, so they are dropped and the merge is the only option.
    #[test]
    fn case_four_chasm_offers_only_the_door_that_changes_a_component() {
        let case = cases::case_4_chasm_merge();
        let advice = advise_case(&case);
        assert_eq!(top_doors(&advice, B1), vec![C2], "{}", case.decision);
        let offered: Vec<Vec<Slot>> = advice
            .recommendations
            .iter()
            .map(|r| opened_toward(&r.option.doors, B1))
            .collect();
        assert!(
            offered.iter().all(|d| d.is_empty() || *d == vec![C2]),
            "a pointless door reached the recommendations: {offered:?}"
        );
    }

    // RU by way of its cheapest form: every remaining corridor would dilute the
    // Sanctum's saturated pool, so nothing is worth opening, and the kill that
    // builds the Sanctum wins outright.
    // Played under Consolidation option 2 (`ResidentCarry`): the change from
    // tier 1 lands at tier 2, the Sanctum the recorded play built.
    #[test]
    fn case_five_poison_garden_builds_the_sanctum_and_opens_nothing() {
        let case = cases::case_5_poison_garden();
        let advice = advise_case_under(&case, &rush(), &played(Some(Consolidation::ResidentCarry)));
        let top = &advice.recommendations[0];
        assert!(top.option.doors.is_empty(), "{}", case.decision);
        let architect = top.option.architect.as_ref().expect("resolved");
        assert_eq!(architect.kind, Change);
        assert_eq!(
            architect.display_name, "Sanctum of Unity",
            "option 2 turns the printed Shrine of Empowerment into its tier-2 room"
        );
        assert!(
            has_reason(top, |r| matches!(r, Reason::Rt)),
            "no Cradle-line room is on the board, so the rollout leaves the \
             kills inside the band and RT takes the upgrade line: {:?}",
            top.reasons
        );
        assert!(
            has_reason(top, |r| matches!(r, Reason::RuDeclined { .. })),
            "the key was DECLINED by RU — Sebastian's own reason on this board \
             — not unusable and not undropped: {:?}",
            top.reasons
        );
    }

    // The blind board. Sebastian's own reason was R1 + R2 — one row up, and a
    // singleton — and the recommendation must name both.
    #[test]
    fn case_six_cloister_opens_toward_the_cellar_naming_r1_and_r2() {
        let case = cases::case_6_cloister();
        let advice = advise_case(&case);
        assert_eq!(top_doors(&advice, D1), vec![C1], "{}", case.decision);
        let top = &advice.recommendations[0];
        assert!(
            has_reason(top, |r| matches!(r, Reason::R2 { joined: 4 }))
                && has_reason(top, |r| matches!(r, Reason::R1Gradient { row: 2 })),
            "the pick must be explained by R1 and R2: {:?}",
            top.reasons
        );
    }

    // POE-248. One key in Lightning Workshop (C1), and the only two corridors
    // that change a component both land on a B slot. Under a single
    // Apex/Apex-adjacent flag they tied, and R2 decided: it prefers the smaller
    // merge, so it took the lone singleton B1 (the Vault) over B0, which
    // already shares a two-slot component with the Apex because A0-B0 is open.
    // Sebastian's rule is that B0 REACHES the Apex, which outranks adjacency.
    #[test]
    fn case_eight_lightning_workshop_opens_the_corridor_that_already_reaches_the_apex() {
        let case = cases::case_8_lightning_workshop();
        let advice = advise_case(&case);
        assert_eq!(
            top_doors(&advice, C1),
            vec![B0],
            "{}: Sebastian's move was {}",
            case.name,
            case.decision
        );
        let top = &advice.recommendations[0];
        assert!(
            has_reason(top, |r| matches!(
                r,
                Reason::R1Apex { reaches_apex: true }
            )),
            "{}: and the overlay must print the reaching level, not adjacency: {:?}",
            case.name,
            top.reasons
        );
    }

    // RA. Both corridors into the Entrance cluster connect the same value and
    // merge the same two clusters; before RA the row gradient took Banquet
    // Hall (C0, row C) over Strongbox Chamber (D1, row D), the corridor that
    // leaves the Entrance seven hops from the Apex instead of five. With the
    // Corruption Chamber no longer a target neither corridor is RV's, so the
    // pick is the Apex-adjacent B0 and RA orders the two below it.
    #[test]
    fn case_nine_corruption_chamber_ranks_the_corridor_that_arrives_faster_above_banquet_hall() {
        let case = cases::case_9_corruption_chamber_arrival();
        let advice = advise_case(&case);
        let rank_of = |far: Slot| {
            advice
                .recommendations
                .iter()
                .position(|r| {
                    opened_toward(&r.option.doors, C1) == vec![far]
                        && r.option.architect.as_ref().map(|a| a.line.key()) == Some("corruption")
                })
                .expect("both corridors are enumerated on the upgrade kill")
        };
        assert!(
            rank_of(D1) < rank_of(C0),
            "{}: Strongbox Chamber must rank above Banquet Hall\n{}",
            case.name,
            ranking_dump(&advice)
        );
    }

    // What the page prints beside each of the two corridors: the walk it
    // buys, so the player can see WHY the middle one is the pick.
    #[test]
    fn case_nine_corruption_chamber_prints_the_walk_each_corridor_buys() {
        let case = cases::case_9_corruption_chamber_arrival();
        let advice = advise_case(&case);
        let arrival = |far: Slot| {
            advice
                .recommendations
                .iter()
                .find(|r| {
                    opened_toward(&r.option.doors, C1) == vec![far]
                        && r.option.architect.as_ref().map(|a| a.line.key()) == Some("corruption")
                })
                .expect("both corridors are enumerated on the upgrade kill")
                .reasons
                .iter()
                .find_map(|r| match r {
                    Reason::FasterArrival {
                        apex,
                        targets,
                        rooms,
                    } => Some((*apex, *targets, *rooms)),
                    _ => None,
                })
        };
        assert_eq!(
            arrival(D1),
            Some((Some(5), 1, 1)),
            "{}: Strongbox Chamber — Entrance → Apex 5 hops, Entrance → the Royal Meeting Room 1",
            case.name
        );
        assert_eq!(
            arrival(C0),
            Some((Some(7), 1, 1)),
            "{}: Banquet Hall — Entrance → Apex 7 hops, Entrance → the Royal Meeting Room 1",
            case.name
        );
    }

    // At the production rollouts, for the reason case 8's pair carries.
    #[test]
    fn case_nine_corruption_chamber_arrives_faster_at_the_production_rollouts() {
        use crate::temple::slice::{ROLLOUTS as PROD_ROLLOUTS, SEED as PROD_SEED};

        let case = cases::case_9_corruption_chamber_arrival();
        let advice = advise(
            &case.state,
            &case.offers,
            case.keys,
            &rush(),
            &TempleConfig::default(),
            PROD_ROLLOUTS,
            PROD_SEED,
        );
        let rank_of = |far: Slot| {
            advice
                .recommendations
                .iter()
                .position(|r| {
                    opened_toward(&r.option.doors, C1) == vec![far]
                        && r.option.architect.as_ref().map(|a| a.line.key()) == Some("corruption")
                })
                .expect("both corridors are enumerated on the upgrade kill")
        };
        assert!(
            rank_of(D1) < rank_of(C0),
            "{}: Strongbox Chamber must rank above Banquet Hall\n{}",
            case.name,
            ranking_dump(&advice)
        );
    }

    // The same board at the numbers the APP runs, for the reason the PC board's
    // pair below carries: `slice::advise_read` passes `slice::ROLLOUTS` and
    // `slice::SEED`, not this suite's `N`/`SEED`, and the rollout breaks the
    // chain's own ties — so an ordering that holds at one (n, seed) is not one
    // that holds at another. Imported rather than retyped, so a test carrying
    // its own copy of the numbers cannot outlive production's.
    #[test]
    fn case_eight_lightning_workshop_reaches_the_apex_at_the_production_rollouts() {
        use crate::temple::slice::{ROLLOUTS as PROD_ROLLOUTS, SEED as PROD_SEED};

        let case = cases::case_8_lightning_workshop();

        let advice = advise(
            &case.state,
            &case.offers,
            case.keys,
            &rush(),
            &TempleConfig::default(),
            PROD_ROLLOUTS,
            PROD_SEED,
        );

        assert_eq!(
            top_doors(&advice, C1),
            vec![B0],
            "{}: the verdict at N={PROD_ROLLOUTS} seed={PROD_SEED:#x} — the numbers a read \
             actually uses — must be the one the suite pins at N=400 seed=7; Sebastian's move \
             was {}",
            case.name,
            case.decision,
        );
    }

    /// Case 8's CONDITIONAL door, at the numbers a read actually uses.
    ///
    /// **Not a walked decision.** Sebastian played the primary (B0-C1); nobody
    /// has played a second stone on this board. This pins what the chain
    /// PRODUCES so a change to it is visible, and the owner has not confirmed
    /// the answer.
    ///
    /// Why it comes out B1-C1, and it is the enumeration rather than the
    /// ranking that decides: with A0-B0 already open, B0 and B1 are the only
    /// two corridors out of C1 that change reachability at all — C1-C0, C1-D1
    /// and C1-D2 all lead back into C1's own ten-slot component, which
    /// [`rules::door_sets`] excludes as strictly equal to opening nothing. So
    /// {B0-C1, B1-C1} is the ONLY legal two-key set containing the primary —
    /// asserted below, because the claim is what makes the rest of this
    /// docstring true — and the conditional ranking has one pair to weigh
    /// against the primary's own singleton. The second stone buys the Vault
    /// (B1, Wealth of the Vaal), the other Apex-adjacent slot, which is one of
    /// the corridors the owner named as plausible.
    #[test]
    fn case_eight_lightning_workshop_names_the_conditional_second_door() {
        use crate::temple::slice::{ROLLOUTS as PROD_ROLLOUTS, SEED as PROD_SEED};

        let case = cases::case_8_lightning_workshop();
        let advice = advise(
            &case.state,
            &case.offers,
            case.keys,
            &rush(),
            &TempleConfig::default(),
            PROD_ROLLOUTS,
            PROD_SEED,
        );

        assert_eq!(
            top_doors(&advice, C1),
            vec![B0],
            "{}: the primary is still Sebastian's move — {}",
            case.name,
            case.decision
        );
        assert_eq!(
            conditional_pairs(&case.state, C1, &advice.recommendations[0]),
            vec![BTreeSet::from([Edge::new(B0, C1), Edge::new(B1, C1)])],
            "{}: the primary door is in exactly one legal pair — C0/D1/D2 lead \
             back into C1's own component and buy nothing",
            case.name
        );
        assert_eq!(
            advice.secondary_door,
            Some(Edge::new(B1, C1)),
            "{}: a second stone buys the other Apex-adjacent slot",
            case.name
        );
    }

    /// The conditional answer is the PARTNER of the primary in a legal two-key
    /// set, not a door ranked on its own.
    ///
    /// The property, rather than the value case 8 happens to produce: whatever
    /// comes back, `{primary, secondary}` must be a set [`rules::door_sets`]
    /// would enumerate for the recommended architect at two keys. That is what
    /// makes the faint seal readable as *"and this one too"* — a door ranked
    /// independently could be one the pair filter excludes as buying a merge
    /// the primary already bought.
    #[test]
    fn the_conditional_door_completes_a_legal_two_key_set_with_the_primary() {
        let case = cases::case_8_lightning_workshop();
        let advice = advise_case(&case);
        let primary = &advice.recommendations[0];
        let first = *primary
            .option
            .doors
            .iter()
            .next()
            .expect("the top recommendation opens one door");
        let second = advice
            .secondary_door
            .expect("case 8 has a second corridor worth a key");

        assert_ne!(second, first, "the second door is a DIFFERENT corridor");
        assert!(
            conditional_pairs(&case.state, C1, primary)
                .contains(&BTreeSet::from([first, second])),
            "{:?} is not a legal two-key set for the recommended kill",
            [first.to_string(), second.to_string()]
        );
    }

    /// One corridor worth a key means no second door to be conditional about.
    ///
    /// Every corridor out of C1 is open but one, so the closed one is the only
    /// candidate and the conditional ranking has nothing but the primary's own
    /// singleton in it. The widget draws the bright seal and no faint one.
    #[test]
    fn there_is_no_conditional_door_when_the_room_has_one_corridor_to_buy() {
        use crate::temple::lattice::neighbours;

        let mut open: Vec<(Slot, Slot)> = neighbours(C1).into_iter().map(|n| (C1, n)).collect();
        let stranded = open.pop().expect("C1 has corridors").1;
        // Junk at tier 1 everywhere, so no room is the degree-priced upgrade
        // room whose corridors survive the same-component exclusion.
        let rooms: Vec<(Slot, &str, u8)> = std::iter::once(C1)
            .chain(neighbours(C1))
            .map(|slot| (slot, JUNK, 1))
            .collect();
        let state = board(&rooms, &open, C1, 5);

        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );

        assert_eq!(
            top_doors(&advice, C1),
            vec![stranded],
            "the one corridor that changes reachability is the primary"
        );
        assert!(
            conditional_pairs(&state, C1, &advice.recommendations[0]).is_empty(),
            "precondition: no pair to weigh against the singleton"
        );
        assert_eq!(
            advice.secondary_door, None,
            "there is no second corridor, so nothing a second stone could buy"
        );
    }

    /// A board where the chain's answer is *do not spend a second key*, and the
    /// conditional pass must say so by saying nothing.
    ///
    /// Every neighbour of D1 but the Entrance is a saturated tier-1 upgrade
    /// room, so opening any of those corridors dilutes a certainty into a
    /// lottery and RU vetoes it. D1-E1 is the one RU-safe corridor.
    ///
    /// [`DoorKey`] puts `ru_ok` above `opens`, so the un-vetoed SINGLETON
    /// outranks every vetoed pair — with one key in hand and with two. That is
    /// the shape a conditional pass ranking only PAIRS gets wrong: it would
    /// re-rank exactly the vetoed pairs, find a winner among them because they
    /// are all that is left, and publish the partner of a set the chain had
    /// already declined. Both tests below measured that answer (`C0-D1`) before
    /// the singleton joined the ranking.
    fn ru_saturated_board() -> BoardState {
        board(
            &[
                (C0, "upgrade", 1),
                (C1, "upgrade", 1),
                (D0, "upgrade", 1),
                (D2, "upgrade", 1),
                (E0, "upgrade", 1),
                (C2, JUNK, 1),
                (D3, JUNK, 1),
                (D1, JUNK, 1),
            ],
            &[(C0, C1), (D0, E0), (D2, D3)],
            D1,
            6,
        )
    }

    #[test]
    fn the_conditional_answer_is_none_when_the_primary_singleton_outranks_every_pair() {
        let state = ru_saturated_board();
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );

        assert_eq!(
            top_doors(&advice, D1),
            vec![E1],
            "precondition: the one RU-safe corridor is the primary"
        );
        assert!(
            !conditional_pairs(&state, D1, &advice.recommendations[0]).is_empty(),
            "precondition: there ARE pairs — the answer must come from the ranking, \
             not from an empty enumeration"
        );
        assert_eq!(
            advice.secondary_door, None,
            "RU says do not spend the second key, and the pass must not publish the \
             partner of a pair it vetoed"
        );
    }

    #[test]
    fn two_keys_publish_no_conditional_door_because_the_player_holds_both_stones() {
        // The same board at the setting the conditional answer exists to spare
        // the player. The advisor's own recommendation here opens ONE door —
        // RU declining the second key is a two-key answer — so a gate keyed on
        // "the primary already opens two" would let this through and publish a
        // partner from a vetoed pair.
        let state = ru_saturated_board();
        let advice = advise(
            &state,
            &junk_offers(),
            2,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );

        assert_eq!(
            advice.recommendations[0].option.doors.len(),
            1,
            "precondition: the two-KEY answer is a one-DOOR set, which is RU declining \
             to spend the second"
        );
        assert_eq!(advice.secondary_door, None);

        // And the plain shape of the same gate: two keys, two doors, nothing
        // conditional left to say.
        let case = cases::case_8_lightning_workshop();
        let advice = advise(
            &case.state,
            &case.offers,
            2,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(advice.recommendations[0].option.doors.len(), 2);
        assert_eq!(advice.secondary_door, None);
    }

    /// Every legal two-key PAIR containing the primary's door, for the kill the
    /// primary carries — the enumeration the conditional pass ranks, minus the
    /// primary's own singleton.
    fn conditional_pairs(
        board: &BoardState,
        position: Slot,
        primary: &Ranked,
    ) -> Vec<BTreeSet<Edge>> {
        let Some(&first) = primary.option.doors.iter().next() else {
            return Vec::new();
        };
        rules::door_sets(board, position, 2, primary.option.architect.as_ref())
            .into_iter()
            .filter(|set| set.len() == 2 && set.contains(&first))
            .collect()
    }

    /// What the conditional ranking costs, measured rather than assumed.
    ///
    /// `#[ignore]` for the reason [`crate::temple::anchor`]'s exhaustive sweep
    /// is: a wall-clock number is a MEASUREMENT of the machine that took it.
    /// Run it with `cargo test --release the_conditional_ranking_cost --
    /// --ignored --nocapture`.
    ///
    /// Measured 2026-09-04, release, in the repo's docker image on WSL2, over
    /// four runs: **13-22 ms** for the whole `advise` on case 8 at the
    /// production rollouts, of which **3-7 ms** is the conditional pass. The
    /// spread is the box, not the code — which is exactly why the assertion
    /// below is not a time.
    ///
    /// Those figures are case 8's TWO-option pass: the primary's own singleton
    /// plus the board's single legal pair. A six-corridor room with nothing
    /// already open enumerates about five pairs, so budget roughly three times
    /// that; it is still a fraction of the read, and it happens once per read.
    ///
    /// The ASSERTION is the machine-independent half, and it is a COUNT rather
    /// than a time: the pass prices the primary's singleton and every legal
    /// pair containing the primary door, for the primary's architect, and
    /// nothing else. A ratio could not catch the pass widening — 14.6 % against
    /// a 50 % bound leaves room to triple — while the count fails the moment
    /// the enumeration takes in a second architect or a pair the primary is not
    /// in.
    #[test]
    #[ignore]
    fn the_conditional_ranking_cost_on_case_eight() {
        use crate::temple::slice::{ROLLOUTS as PROD_ROLLOUTS, SEED as PROD_SEED};
        use std::time::Instant;

        let case = cases::case_8_lightning_workshop();
        let profile = rush();
        let config = TempleConfig::default();
        let valuation = rollout::Valuation::for_profile(&profile);
        let advise_once = || {
            advise(
                &case.state,
                &case.offers,
                case.keys,
                &profile,
                &config,
                PROD_ROLLOUTS,
                PROD_SEED,
            )
        };
        for _ in 0..2 {
            let _ = advise_once();
        }

        let mut whole = Vec::new();
        let mut conditional = Vec::new();
        let mut priced = Vec::new();
        for _ in 0..5 {
            let started = Instant::now();
            let advice = advise_once();
            whole.push(started.elapsed().as_secs_f64() * 1000.0);

            let primary = &advice.recommendations[0];
            let connectable = rules::rv_connectable(
                &case.state,
                C1,
                &rules::door_sets(&case.state, C1, case.keys, primary.option.architect.as_ref()),
            );
            let started = Instant::now();
            let out = conditional_second_door(
                &case.state,
                C1,
                primary,
                case.keys,
                &profile,
                &valuation,
                &config,
                PROD_ROLLOUTS,
                PROD_SEED,
                connectable,
            );
            conditional.push(started.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(out.door, Some(Edge::new(B1, C1)), "the pass under measurement did its job");
            priced.push((out.priced, conditional_pairs(&case.state, C1, primary).len()));
        }

        let median = |mut v: Vec<f64>| {
            v.sort_by(f64::total_cmp);
            v[v.len() / 2]
        };
        let (whole, cond) = (median(whole), median(conditional));
        // Printed and not asserted: see the header. The COUNT below is the
        // claim a machine cannot argue with.
        println!(
            "case 8 at N={PROD_ROLLOUTS}: whole advise {whole:.2} ms, conditional pass {cond:.2} ms \
             over {} options",
            priced[0].0
        );
        for (options, pairs) in priced {
            assert_eq!(
                options,
                pairs + 1,
                "the pass prices the primary's own singleton and every pair containing it, \
                 for ONE architect, and nothing else"
            );
        }
    }

    // ============================================ RV constrains the passage

    /// A stranded Royal Meeting Room (the Cradle line, tier 1) at B1, an
    /// upgrade toward the Cradle and a junk change.
    ///
    /// `doors` decides whether anything on the board can still connect it.
    fn stranded_cradle(doors: &[(Slot, Slot)]) -> (BoardState, Vec<ArchitectOffer>) {
        let state = board(
            &[
                (B1, CRADLE_LINE_KEY, 1),
                (B0, JUNK, 1),
                (C1, JUNK, 1),
                (C2, JUNK, 1),
            ],
            doors,
            B1,
            7,
        );
        let offers = vec![
            offer("Azcapa", Upgrade, "Hall of Lords"),
            offer("Paquate", Change, "Jeweller's Workshop"),
        ];
        (state, offers)
    }

    /// Which kill the ranking put first, and how it named it.
    fn top_kill(advice: &Advice) -> (OfferKind, &Vec<Reason>) {
        let top = &advice.recommendations[0];
        (
            top.option.architect.as_ref().expect("resolved").kind,
            &top.reasons,
        )
    }

    // RV constrains the passage, never the kill. With no key there is no
    // passage, so the upgrade toward the Cradle must not be filed as a gamble
    // while the junk change stays safe — that ranking recommends changing the
    // Cradle-line room to junk.
    #[test]
    fn a_zero_key_incursion_never_turns_the_valuable_kill_into_a_gamble() {
        let (state, offers) = stranded_cradle(&[(C1, D1), (D1, E1)]);
        let advice = advise(
            &state,
            &offers,
            0,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        let (kind, reasons) = top_kill(&advice);
        assert_eq!(kind, Upgrade, "{reasons:?}");
        assert!(
            advice.gambles.is_empty(),
            "RV has nothing to protect and nothing to exclude: {:?}",
            advice.gambles.iter().map(|g| g.option.headline()).collect::<Vec<_>>()
        );
        assert!(reasons.iter().any(|r| matches!(r, Reason::ZeroKey)));
    }

    // The same, one key later: the key exists but every corridor from B1 leads
    // back into B1's own cluster, so no door set can shorten the walk and RV
    // still has no opinion to give.
    #[test]
    fn a_key_with_no_connecting_corridor_never_turns_the_valuable_kill_into_a_gamble() {
        let (state, offers) = stranded_cradle(&[(B1, C1), (C1, C2), (B0, C1), (A0, B0)]);
        let advice = advise(
            &state,
            &offers,
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        let (kind, reasons) = top_kill(&advice);
        assert_eq!(kind, Upgrade, "{reasons:?}");
        assert!(advice.gambles.is_empty(), "nothing on offer satisfies RV");
        assert!(reasons.iter().any(|r| matches!(r, Reason::NoUsableDoor)));
        assert!(advice.warnings.contains(&Warning::KeysUnspendable {
            keys: 1,
            usable: 0
        }));
    }

    // And the contrast that makes the two above a rule rather than a blanket
    // amnesty: put one connecting corridor on the board and RV speaks again —
    // the connecting door is the recommendation, the Apex door is the gamble.
    #[test]
    fn a_connecting_corridor_puts_the_apex_back_on_the_gamble_side() {
        let (state, offers) = stranded_cradle(&[(C1, D1), (D1, E1)]);
        let advice = advise(
            &state,
            &offers,
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        let (kind, _) = top_kill(&advice);
        assert_eq!(kind, Upgrade);
        assert_eq!(
            top_doors(&advice, B1),
            vec![C1],
            "the only corridor that shortens the walk to the Entrance"
        );
        let apex = advice
            .gambles
            .iter()
            .find(|g| opened_toward(&g.option.doors, B1) == vec![A0])
            .expect("the Apex play must be offered as a gamble");
        assert!(apex
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::RvGamble { .. })));
        assert!(
            advice.recommendations.iter().any(|r| r
                .option
                .architect
                .as_ref()
                .is_some_and(|a| a.kind == Upgrade)),
            "RV excluded a passage, not a kill"
        );
    }

    // ===================================================== the priority chain

    // R1-apex > R2, in live board 1's own shape: standing in an isolated
    // Banquet Hall, R1 pointed at a **Vault that sat in a 2-cluster** while R2
    // pointed at three singletons, and Sebastian took the Vault — *"Vault is
    // way more important, it's connecting top."*
    //
    // The *slots* are synthetic — §6d records the board's relations, never its
    // placement — so this is the relation under test, not the screenshot. The
    // relation is what makes it discriminating: R2 prefers a singleton here (a
    // 1+1 merge against 1+2), so only R1-apex can produce the recorded pick.
    #[test]
    fn an_apex_corridor_outranks_merging_a_singleton_it_ranks_below_on_r2() {
        // C2 is isolated. Of its four corridors, B1 leads into a 2-cluster and
        // reaches the only pair of slots that can ever open the Apex; C1, D2
        // and D3 are singletons and score better on R2.
        let state = board(
            &[
                (B0, JUNK, 1),
                (B1, JUNK, 1),
                (C1, JUNK, 1),
                (C2, JUNK, 1),
                (D2, JUNK, 1),
                (D3, JUNK, 1),
            ],
            &[(B0, B1)],
            C2,
            8,
        );
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(
            top_doors(&advice, C2),
            vec![B1],
            "the Apex-adjacent corridor is the scarcest on the board — only B0 \
             and B1 can ever open an Apex door"
        );
        let top = &advice.recommendations[0];
        assert!(
            has_reason(top, |r| matches!(r, Reason::R1Apex { .. })),
            "the pick must name the rule that produced it: {:?}",
            top.reasons
        );
        // The relation, made explicit: the winner is the option R2 ranks
        // *lower*, so deleting R1-apex hands the board to a singleton.
        assert!(
            has_reason(top, |r| matches!(r, Reason::R2 { joined: 3 })),
            "the Vault corridor merges 1 + 2: {:?}",
            top.reasons
        );
        let singleton = advice
            .recommendations
            .iter()
            .find(|r| opened_toward(&r.option.doors, C2) == vec![C1])
            .expect("the singleton corridors are still enumerated");
        assert!(
            has_reason(singleton, |r| matches!(r, Reason::R2 { joined: 2 })),
            "and a singleton merges 1 + 1: {:?}",
            singleton.reasons
        );
    }

    // R2 > generic merging. Live board 4: attaching a singleton beats merging
    // two large clusters while neither holds anything worth reaching.
    #[test]
    fn attaching_a_singleton_outranks_merging_two_large_clusters() {
        // C2 sits alone; D2 leads into the five-room Entrance side.
        let state = board(
            &[
                (B0, JUNK, 1),
                (C0, JUNK, 1),
                (C1, JUNK, 1),
                (C2, JUNK, 1),
                (D0, JUNK, 1),
                (D1, JUNK, 1),
                (D2, JUNK, 1),
                (E0, JUNK, 1),
                (E2, JUNK, 1),
            ],
            &[(B0, C0), (C0, D0), (D0, E0), (D1, E1), (E1, E2)],
            D3,
            8,
        );
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(
            top_doors(&advice, D3),
            vec![C2],
            "connectivity is instrumental: spend the key on the scarce merge, \
             the big one will still be there"
        );
    }

    // RS. Live board 3: with R1 and R2 both tied — two row-D singletons — the
    // scarcer slot wins. Cellar has three lattice neighbours, Sparring Room six.
    #[test]
    fn a_tie_on_r1_and_r2_is_broken_by_the_scarcer_slot() {
        // C2's other two corridors (B1, C1) are already open, so the only
        // candidates are its two row-D neighbours: D2 with six lattice
        // neighbours, D3 with three. Both are singletons, so R2 ties, and both
        // are row D, so the gradient ties too.
        let state = board(&[], &[(B1, C2), (C1, C2)], C2, 10);
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(
            top_doors(&advice, C2),
            vec![D3],
            "RS: the low-degree slot has fewer future chances to be linked"
        );
        assert!(has_reason(&advice.recommendations[0], |r| matches!(
            r,
            Reason::Rs { degree: 3 }
        )));
    }

    // R2 > R1-gradient, on a board where the two rules disagree. Every row-C
    // corridor out of D2 merges into the seven-room main cluster; the only
    // singleton left is D3, a row below. R2 takes the singleton, the gradient
    // would take the row — so swapping the two fields of [`DoorKey`] flips this
    // board.
    #[test]
    fn a_singleton_attach_outranks_a_higher_row_merge_into_the_main_cluster() {
        let state = board(
            &[
                (B1, JUNK, 1),
                (C1, JUNK, 1),
                (C2, JUNK, 1),
                (D1, JUNK, 1),
                (D3, JUNK, 1),
                (E0, JUNK, 1),
                (E2, JUNK, 1),
            ],
            &[(B1, C1), (C1, C2), (C1, D1), (D1, E1), (E0, E1), (E1, E2)],
            D2,
            8,
        );
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(
            top_doors(&advice, D2),
            vec![D3],
            "R2 outranks the row gradient: the singleton is the scarce merge, \
             and the big cluster will still be there"
        );
        let upward = advice
            .recommendations
            .iter()
            .find(|r| opened_toward(&r.option.doors, D2) == vec![C2])
            .expect("the row-C corridors are still enumerated");
        assert!(
            has_reason(upward, |r| matches!(r, Reason::R1Gradient { row: 2 })),
            "the gradient really does point the other way here: {:?}",
            upward.reasons
        );
    }

    // R1-gradient > RS. Live board 1's second key: two singleton merges, so R2
    // ties, and the higher row wins even though it is the *less* scarce slot.
    #[test]
    fn a_tie_on_r2_prefers_the_higher_row_over_the_scarcer_slot() {
        let state = board(&[], &[(D1, E1), (E1, E2)], D2, 8);
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        // From D2: C1 (row C, six neighbours) and C2 (row C, four) are up; D3
        // (row D, three) is the scarcest slot on offer.
        let doors = top_doors(&advice, D2);
        assert!(
            doors == vec![C1] || doors == vec![C2],
            "the row gradient decides before scarcity does, got {doors:?}"
        );
    }

    // RV generalised: once a cluster holds a target-line room, merging it is RV
    // firing and outranks every scarcity rule — including an Apex corridor.
    #[test]
    fn merging_a_cluster_that_holds_a_target_room_outranks_the_apex_corridor() {
        // B1 sits in the Entrance component; its corridors lead to the Apex,
        // to a lone Chasm at B0, and to a two-room cluster holding a Cradle.
        let state = board(
            &[(C2, CRADLE_LINE_KEY, 3), (D3, JUNK, 1)],
            &[(C2, D3), (B1, C1), (C1, D1), (D1, E1)],
            B1,
            8,
        );
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(
            top_doors(&advice, B1),
            vec![C2],
            "a merge is worth a key once the cluster holds something worth \
             reaching — and then it is RV, not R1"
        );
        assert!(has_reason(&advice.recommendations[0], |r| matches!(
            r,
            Reason::RvMerge { rooms: 1 }
        )));
    }

    // ============================================================ RU, RC, RT

    // RU. The Shrine at C1 already has its one target, so a second connection
    // turns a certainty into a coin flip — and the door is otherwise a perfectly
    // good merge.
    #[test]
    fn a_door_that_dilutes_a_saturated_upgrade_room_loses_to_the_merge_that_does_not() {
        // The Shrine at C1 has exactly one connected neighbour, C0, which is
        // one tier off the Cradle of Creation — so its single pick is
        // currently a certainty. Standing in D1, two corridors merge that same
        // cluster in: D1-C0 leaves the Shrine alone, D1-C1 dilutes it.
        let state = board(
            &[(C1, "upgrade", 1), (C0, CRADLE_LINE_KEY, 2)],
            &[(C0, C1), (D1, E1)],
            D1,
            6,
        );
        let advice = advise(
            &state,
            &[offer("Tacati", Change, "Storage Room")],
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        let top = &advice.recommendations[0];
        assert_eq!(
            opened_toward(&top.option.doors, D1),
            vec![C0],
            "the merge that does not dilute the Shrine wins: {:?}",
            top.reasons
        );
        let diluting = advice
            .recommendations
            .iter()
            .find(|r| opened_toward(&r.option.doors, D1) == vec![C1])
            .expect("the diluting door is still enumerated");
        assert!(
            has_reason(diluting, |x| matches!(
                x,
                Reason::Ru {
                    slot: C1,
                    targets: 1,
                    connected: 2
                }
            )),
            "RU must name the room it protects: {:?}",
            diluting.reasons
        );
    }

    // RU with the room the *kill* creates: the same board decided by what the
    // architect builds rather than by what is already there (Case 5's shape).
    #[test]
    fn opening_nothing_wins_when_the_kill_itself_creates_a_saturated_shrine() {
        // D1 has exactly two connected neighbours and one closed corridor into
        // a different component; changing it to Sanctum of Unity II makes both
        // connections certain upgrades.
        let state = board(
            &[(D1, "toxic_grove", 1), (C1, "gem", 2), (E1, JUNK, 1), (C2, JUNK, 1)],
            &[(C1, D1), (D1, E1), (C2, D3)],
            D1,
            5,
        );
        let advice = advise(
            &state,
            &[offer("Tacati", Change, "Shrine of Empowerment")],
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        let top = &advice.recommendations[0];
        assert!(
            top.option.doors.is_empty(),
            "a third connection would turn a guaranteed Doryani into 2-of-3, \
             got {}",
            top.option.doors_label()
        );
        assert!(
            has_reason(top, |r| matches!(r, Reason::RuDeclined { slot: D1 })),
            "opening nothing here is RU's choice, not a missing key: {:?}",
            top.reasons
        );
    }

    // RC. Live board 5, played under Consolidation option 2 (`ResidentCarry`):
    // both kills land at tier 2 there, so beside a connected Shrine the change
    // spends the architect on the line and lets the shrine supply the tier.
    // Option 1 is the option-comparison acceptance test's subject.
    #[test]
    fn beside_a_connected_shrine_the_change_architect_outranks_the_upgrade() {
        let state = board(
            &[(D1, "toxic_grove", 1), (C1, "upgrade", 1)],
            &[(C1, D1), (D1, E1), (E1, E2)],
            D1,
            6,
        );
        let offers = vec![
            offer("Quipolatl", Upgrade, "Cultivar Chamber"),
            offer("Tacati", Change, "Armourer's Workshop"),
        ];
        let advice = advise(
            &state,
            &offers,
            0,
            &rush(),
            &played(Some(Consolidation::ResidentCarry)),
            N,
            SEED,
        );
        let top = &advice.recommendations[0];
        assert_eq!(
            top.option.architect.as_ref().expect("resolved").kind,
            Change,
            "RC: {:?}",
            top.reasons
        );
        assert!(has_reason(top, |r| matches!(r, Reason::Rc { upgrader: C1 })));
    }

    // RT. While no valuable room exists anywhere the upgrade line is worth
    // taking wherever it is offered, because every neighbour is still in the
    // drop pool.
    //
    // Played under Consolidation option 2 (`ResidentCarry`): both changes from
    // tier 1 land at tier 2, so the line alone tells the kills apart. One
    // incursion is left, so no future draw separates the two kills and the
    // architect chain decides.
    #[test]
    fn the_upgrade_line_is_taken_speculatively_while_the_board_holds_no_target() {
        let state = board(&[(D1, JUNK, 1)], &[(D1, E1), (E1, E2)], D1, 1);
        let offers = vec![
            offer("Tacati", Change, "Shrine of Empowerment"),
            offer("Ticaba", Change, "Storage Room"),
        ];
        let advice = advise(
            &state,
            &offers,
            0,
            &rush(),
            &played(Some(Consolidation::ResidentCarry)),
            N,
            SEED,
        );
        let top = &advice.recommendations[0];
        assert_eq!(
            top.option.architect.as_ref().expect("resolved").line,
            Line::Upgrade,
            "RT: {:?}",
            top.reasons
        );
        assert!(has_reason(top, |r| matches!(r, Reason::Rt)));
    }

    // RT's other half: once a target room exists, placing the upgrade line
    // becomes deliberate and the speculative preference is gone.
    #[test]
    fn the_upgrade_line_loses_its_speculative_preference_once_a_target_exists() {
        let state = board(
            &[(D1, JUNK, 1), (C2, CRADLE_LINE_KEY, 1)],
            &[(D1, E1), (E1, E2)],
            D1,
            9,
        );
        let offers = vec![
            offer("Tacati", Change, "Shrine of Empowerment"),
            offer("Ticaba", Change, "Storage Room"),
        ];
        let advice = advise(
            &state,
            &offers,
            0,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert!(
            advice
                .recommendations
                .iter()
                .all(|r| !has_reason(r, |x| matches!(x, Reason::Rt))),
            "RT must not fire while a Cradle-line room is on the board"
        );
    }

    // R4. Maxing a junk room removes it from the drop pool, concentrating
    // future drops on the rooms that still matter.
    #[test]
    fn a_junk_room_is_maxed_to_tier_three_to_shrink_the_drop_pool() {
        // The default option's landing comes from facts::landing_tier;
        // only this upgrade removes the slot from the pool.
        let state = board(
            &[(D1, "hall_of_champions", 1), (C2, "corruption", 1)],
            &[(D1, E1), (E1, E2)],
            D1,
            6,
        );
        let offers = vec![
            offer("Ticaba", Upgrade, "Arena of Valour"),
            offer("Juatalotli", Change, "Storage Room"),
        ];
        let advice = advise(
            &state,
            &offers,
            0,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        let top = &advice.recommendations[0];
        assert_eq!(
            top.option.architect.as_ref().expect("resolved").kind,
            Upgrade,
            "R4: {:?}",
            top.reasons
        );
        assert!(has_reason(top, |r| matches!(r, Reason::R4)));
    }

    /// Live board 8's shape: a junk room that is the only thing an adjacent,
    /// connected Sanctum can still upgrade.
    ///
    /// Under option 1, facts::landing_tier makes "upgrade → Engineering
    /// Department" leave the pool; "change → Storage Room" stays in it.
    fn board_eight() -> (BoardState, Vec<ArchitectOffer>) {
        let state = board(
            &[(D1, "factory", 1), (C1, "upgrade", 2), (C2, "corruption", 1)],
            &[(C1, D1), (D1, E1), (E1, E2)],
            D1,
            6,
        );
        let offers = vec![
            offer("Ticaba", Upgrade, "Engineering Department"),
            offer("Juatalotli", Change, "Storage Room"),
        ];
        (state, offers)
    }

    // R4's carve-out (PROPOSED — see [`StrategyProfile::r4_keep_upgrade_targets`]).
    // Live board 8: maxing the slot would destroy the Sanctum's only live
    // target permanently, so the carve-out is a **veto** rather than a
    // tie-break. It has to be: the rollout independently prefers the
    // pool removal for R4's own reason and separates the
    // two kills past the noise band, so a tie-break would never fire.
    #[test]
    fn maxing_a_sanctums_only_live_target_is_vetoed_however_the_rollout_prices_it() {
        let (state, offers) = board_eight();
        let mut profile = rush();
        profile.r4_keep_upgrade_targets = true;
        let advice = advise(
            &state,
            &offers,
            0,
            &profile,
            &TempleConfig::default(),
            N,
            SEED,
        );
        let top = &advice.recommendations[0];
        assert_eq!(
            top.option.architect.as_ref().expect("resolved").kind,
            Change,
            "board 8's recorded play is the change — the next board shows that \
             Workshop as Cultivar Chamber II: {:?}",
            top.reasons
        );
        assert!(
            has_reason(top, |r| matches!(r, Reason::R4PoolCarveOut { upgrader: C1 })),
            "the carve-out must name the Sanctum it is protecting: {:?}",
            top.reasons
        );
        let maxed = advice
            .recommendations
            .iter()
            .find(|r| r.option.architect.as_ref().expect("resolved").kind == Upgrade)
            .expect("the vetoed kill is still ranked, with its reason");
        assert!(
            has_reason(maxed, |r| matches!(r, Reason::R4CarveOutVeto { upgrader: C1 })),
            "a demoted kill must say why: {:?}",
            maxed.reasons
        );
        assert!(
            maxed.ev > top.ev,
            "the veto is only meaningful while the rollout disagrees: {} vs {}",
            maxed.ev,
            top.ev
        );
        assert!(
            advice
                .recommendations
                .iter()
                .all(|r| !has_reason(r, |x| matches!(x, Reason::R4))),
            "R4 and its carve-out must never both fire on one board"
        );
    }

    // The carve-out is a profile field because §6e records the R4-vs-stay-in-
    // the-pool tension as explicitly unresolved. Turn it off and the same board
    // goes back to R4's answer.
    #[test]
    fn the_carve_out_flag_hands_board_eight_back_to_the_rollout() {
        let (state, offers) = board_eight();
        let mut profile = rush();
        profile.r4_keep_upgrade_targets = false;
        let advice = advise(
            &state,
            &offers,
            0,
            &profile,
            &TempleConfig::default(),
            N,
            SEED,
        );
        let top = &advice.recommendations[0];
        assert_eq!(
            top.option.architect.as_ref().expect("resolved").kind,
            Upgrade,
            "with the carve-out off nothing outranks the EV: {:?}",
            top.reasons
        );
        assert!(
            advice
                .recommendations
                .iter()
                .all(|r| !has_reason(r, |x| matches!(
                    x,
                    Reason::R4PoolCarveOut { .. } | Reason::R4CarveOutVeto { .. }
                ))),
            "the carve-out must be silent on both sides when it is off"
        );
    }

    /// Live board 5's shape on board 8's lattice: the upgrade room beside the
    /// slot is a **Temple Nexus** (C1 at tier 3), not a Sanctum, and the
    /// upgrade's line is worth something on its own — board 5's Jewellery
    /// Forge was, under the live-priced profile. Valuing `factory` (the line
    /// Engineering Department belongs to) is what gives the rollout a reason
    /// to prefer the upgrade; it stays off `mode_rule`, so it is not
    /// a target line and the kill is still one R4's carve-out could veto.
    fn nexus_beside_board_eight() -> (BoardState, Vec<ArchitectOffer>, StrategyProfile) {
        let (_, offers) = board_eight();
        let state = board(
            &[(D1, "factory", 1), (C1, "upgrade", 3), (C2, "corruption", 1)],
            &[(C1, D1), (D1, E1), (E1, E2)],
            D1,
            6,
        );
        let mut profile = rush();
        profile.r4_keep_upgrade_targets = true;
        profile.room_values.insert(Line::named("factory"), [0.0, 1.0, 3.0]);
        (state, offers, profile)
    }

    // A Temple Nexus lifts every neighbour, so maxing one of them costs it no
    // scarce pick: the carve-out stands down and the rollout's preferred kill
    // wins. On live board 5 the veto fired here and recommended the lower-EV
    // change "to protect" the Nexus's only live target.
    #[test]
    fn a_temple_nexus_beside_the_slot_leaves_the_kill_to_the_rollout() {
        let (state, offers, profile) = nexus_beside_board_eight();
        let advice = advise(
            &state,
            &offers,
            0,
            &profile,
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert!(
            advice
                .recommendations
                .iter()
                .all(|r| !has_reason(r, |x| matches!(x, Reason::R4CarveOutVeto { .. }))),
            "a Nexus has no scarce pick to protect: {:?}",
            advice.recommendations[0].reasons
        );
        let top = &advice.recommendations[0];
        let change = advice
            .recommendations
            .iter()
            .find(|r| r.option.architect.as_ref().expect("resolved").kind == Change)
            .expect("both kills are ranked");
        assert_eq!(
            top.option.architect.as_ref().expect("resolved").kind,
            Upgrade,
            "the higher-EV kill must win: {:?}",
            top.reasons
        );
        assert!(
            top.ev > change.ev,
            "the board only tests the veto while the rollout prefers the upgrade: {} vs {}",
            top.ev,
            change.ev
        );
    }

    // When the rollout separated the kills past the noise band and nothing
    // vetoed either, the leader says the EV put it there.
    #[test]
    fn a_leader_the_rollout_separated_says_expected_value_decided_it() {
        let (state, offers, profile) = nexus_beside_board_eight();
        let advice = advise(
            &state,
            &offers,
            0,
            &profile,
            &TempleConfig::default(),
            N,
            SEED,
        );
        let top = &advice.recommendations[0];
        assert!(
            has_reason(top, |r| matches!(r, Reason::ExpectedValue)),
            "the rollout decided this board: {:?}",
            top.reasons
        );
    }

    // When the veto decided, the EV did not: the leader sits on top BECAUSE its
    // rival was vetoed, and the vetoed kill sits below whatever its EV. Neither
    // may read "ranked on expected value; no rule discriminated" — the veto is
    // exactly the rule that did.
    #[test]
    fn a_ranking_the_veto_decided_never_claims_expected_value_decided_it() {
        let (state, offers) = board_eight();
        let mut profile = rush();
        profile.r4_keep_upgrade_targets = true;
        let advice = advise(
            &state,
            &offers,
            0,
            &profile,
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert!(
            advice
                .recommendations
                .iter()
                .any(|r| has_reason(r, |x| matches!(x, Reason::R4CarveOutVeto { .. }))),
            "the board must still be one the veto decides"
        );
        for ranked in &advice.recommendations {
            assert!(
                !has_reason(ranked, |r| matches!(r, Reason::ExpectedValue)),
                "{:?} claims the EV decided a veto: {:?}",
                ranked.option.architect.as_ref().expect("resolved").kind,
                ranked.reasons
            );
        }
    }

    // `reroll_until_favourable` is a profile flag, so it changes the advice
    // without changing the code path.
    #[test]
    fn the_reroll_flag_prefers_change_over_upgrading_junk_on_an_empty_board() {
        let state = board(&[(D1, "hall_of_champions", 1)], &[(D1, E1), (E1, E2)], D1, 9);
        let offers = vec![
            offer("Ticaba", Upgrade, "Arena of Valour"),
            offer("Juatalotli", Change, "Storage Room"),
        ];
        let mut profile = rush();
        profile.reroll_until_favourable = true;
        let advice = advise(
            &state,
            &offers,
            0,
            &profile,
            &TempleConfig::default(),
            N,
            SEED,
        );
        let top = &advice.recommendations[0];
        assert_eq!(
            top.option.architect.as_ref().expect("resolved").kind,
            Change,
            "with the flag on, an empty board rerolls rather than maxing junk: {:?}",
            top.reasons
        );
        assert!(has_reason(top, |r| matches!(
            r,
            Reason::RerollUntilFavourable
        )));
    }

    // ======================================================== R5 and the mode

    /// A profile chasing Hybridisation Chamber, priced 9 at tier 3.
    ///
    /// R5 reads only the chased line, and every Cradle-line kill builds tier
    /// 3, so the Rush itself never reaches R5: its mechanism is pinned on a
    /// line a kill can take below tier 3.
    fn hybridisation_chase() -> StrategyProfile {
        let line = Line::named("hybridisation_chamber");
        let mut profile = rush();
        profile.room_values.insert(line.clone(), [0.0, 0.0, 9.0]);
        profile.mode_rule = ModeRule::LinesConnected(vec![line]);
        profile
    }

    // Live board 10: the chased line was just taken below tier 3, so the
    // next incursion of this map is a guaranteed zero against it.
    #[test]
    fn r5_leaves_the_map_after_the_chased_line_is_used_below_tier_three() {
        let state = board(&[(D1, JUNK, 1)], &[(D1, E1), (E1, E2)], D1, 8);
        let offers = vec![offer("Paquate", Change, "Hatchery")];
        let advice = advise(
            &state,
            &offers,
            0,
            &hybridisation_chase(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(advice.mode, Mode::Chase);
        assert_eq!(advice.map_action, MapAction::LeaveMap);
    }

    // A persisted Scarab of Timelines flag no longer takes R5 away: the scarab
    // is gone in 3.30 (POE-377). Same board as above, flag set.
    #[test]
    fn a_persisted_timelines_scarab_does_not_take_r5_away() {
        let state = board(&[(D1, JUNK, 1)], &[(D1, E1), (E1, E2)], D1, 8);
        let offers = vec![offer("Paquate", Change, "Hatchery")];
        let config = TempleConfig {
            artefacts_of_the_vaal: true,
            scarab_of_timelines: true,
            consolidation: None,
        };
        let advice = advise(&state, &offers, 0, &hybridisation_chase(), &config, N, SEED);
        assert_eq!(advice.map_action, MapAction::LeaveMap);
    }

    // R5 stops once the room is at tier 3 — it has left the drop pool anyway.
    #[test]
    fn r5_does_not_fire_when_the_kill_takes_the_line_to_tier_three() {
        let state = board(&[(D1, "hybridisation_chamber", 2)], &[(D1, E1), (E1, E2)], D1, 8);
        let offers = vec![offer("Azcapa", Upgrade, "Hybridisation Chamber")];
        let advice = advise(
            &state,
            &offers,
            0,
            &hybridisation_chase(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(advice.map_action, MapAction::Continue);
    }

    // The chased line connected would select Scarab mode, but the scarab is
    // gone in 3.30 (POE-377): the advisor stays in Chase and R5 still leaves.
    #[test]
    fn a_connected_chased_line_stays_chase_while_the_scarab_is_inactive() {
        let offers = vec![offer("Paquate", Change, "Hatchery")];
        let both = chased_line_board();
        let advice = advise(&both, &offers, 0, &hybridisation_chase(), &TempleConfig::default(), N, SEED);
        assert_eq!(advice.mode, Mode::Chase);
        assert_eq!(advice.map_action, MapAction::LeaveMap);
    }

    // A persisted Scarab of Timelines flag does not switch the mode either.
    #[test]
    fn a_persisted_timelines_scarab_on_a_chased_line_board_stays_chase() {
        let offers = vec![offer("Paquate", Change, "Hatchery")];
        let config = TempleConfig {
            artefacts_of_the_vaal: true,
            scarab_of_timelines: true,
            consolidation: None,
        };
        let advice = advise(&chased_line_board(), &offers, 0, &hybridisation_chase(), &config, N, SEED);
        assert_eq!(advice.mode, Mode::Chase);
    }

    /// The chased Hybridisation Chamber line built and connected at E0.
    fn chased_line_board() -> BoardState {
        board(
            &[(D1, JUNK, 1), (E0, "hybridisation_chamber", 1)],
            &[(D1, E1), (E0, E1)],
            D1,
            8,
        )
    }

    // A built-but-unconnected target line is treated as absent by v1.
    #[test]
    fn a_stranded_target_line_does_not_switch_the_mode() {
        let state = board(
            &[(E0, "gem", 1), (C2, CRADLE_LINE_KEY, 1)],
            &[(E0, E1), (E1, E2)],
            D1,
            8,
        );
        assert_eq!(rush().select_mode(&state.connected_rooms()), Mode::Chase);
    }

    // The mode rule is profile data, so a second required line changes it.
    #[test]
    fn the_mode_rule_is_profile_data_not_a_hard_coded_line() {
        let state = board(&[(E0, CRADLE_LINE_KEY, 1)], &[(E0, E1)], D1, 8);
        let mut profile = rush();
        profile.mode_rule =
            ModeRule::LinesConnected(vec![Line::named(CRADLE_LINE_KEY), Line::named("factory")]);
        assert_eq!(profile.select_mode(&state.connected_rooms()), Mode::Chase);
    }

    // =================================================================== RK

    // Two keys means the decision is a SET, and the two doors must not buy the
    // same merge twice.
    #[test]
    fn two_keys_are_recommended_as_a_set_of_two_distinct_merges() {
        let state = board(&[], &[(D1, E1), (E1, E2)], C1, 9);
        let advice = advise(
            &state,
            &junk_offers(),
            2,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        let top = &advice.recommendations[0];
        assert_eq!(
            top.option.doors.len(),
            2,
            "both keys should be spent: {}",
            top.option.doors_label()
        );
        let far = opened_toward(&top.option.doors, C1);
        assert_eq!(far.len(), 2);
        assert_ne!(far[0], far[1]);
    }

    // A second key into a component the first key already joined buys nothing.
    #[test]
    fn a_second_key_into_an_already_merged_component_is_never_offered() {
        // D1 and D2 are already joined, so C1-D1 plus C1-D2 is one merge and
        // the second key buys nothing.
        let state = board(&[], &[(D1, D2), (D2, E2), (E1, E2)], C1, 9);
        let advice = advise(
            &state,
            &junk_offers(),
            2,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert!(
            advice.recommendations.iter().any(|r| r.option.doors.len() == 2),
            "sanity: two distinct merges are available from C1"
        );
        for ranked in &advice.recommendations {
            let far = opened_toward(&ranked.option.doors, C1);
            assert!(
                !(far.contains(&D1) && far.contains(&D2)),
                "redundant pair offered: {}",
                ranked.option.doors_label()
            );
        }
    }

    // Zero keys is legal — every passage from the room is already open, which
    // is where live boards 7–11 spent their whole run.
    #[test]
    fn a_zero_key_incursion_is_an_architect_only_decision() {
        let state = board(&[(D1, JUNK, 1)], &[(D1, E1), (E1, E2)], D1, 5);
        let advice = advise(
            &state,
            &junk_offers(),
            0,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert!(!advice.recommendations.is_empty());
        assert!(advice
            .recommendations
            .iter()
            .all(|r| r.option.doors.is_empty()));
        assert!(has_reason(&advice.recommendations[0], |r| matches!(
            r,
            Reason::ZeroKey
        )));
    }

    // ============================================================== boundaries

    // A panel read between rooms has no decision to make, and must say so
    // rather than recommending a move for a room nobody is standing in.
    #[test]
    fn a_board_without_a_current_room_yields_no_recommendation() {
        let mut state = board(&[], &[(D1, E1)], D1, 5);
        state.position = None;
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert!(advice.recommendations.is_empty());
        assert!(advice.warnings.contains(&Warning::NoPosition));
    }

    // An unreadable architect block must not silently become a kill.
    #[test]
    fn an_unresolvable_architect_target_is_reported_and_dropped() {
        let state = board(&[], &[(D1, E1), (E1, E2)], D1, 5);
        let offers = vec![
            offer("Ticaba", Change, "Storage Room"),
            offer("Nobody", Change, "Definitely Not A Room"),
        ];
        let advice = advise(
            &state,
            &offers,
            0,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert!(advice.warnings.iter().any(|w| matches!(
            w,
            Warning::UnresolvedOffer { printed } if printed == "Definitely Not A Room"
        )));
        assert_eq!(
            advice.recommendations.len(),
            1,
            "only the readable architect may be recommended"
        );
    }

    // Both blocks unreadable: the door advice is still worth having, and the
    // caller is told the kill is unmodelled.
    #[test]
    fn both_architects_unreadable_still_yields_door_advice_with_a_warning() {
        let state = board(&[], &[(D1, E1), (E1, E2)], C1, 6);
        let offers = vec![
            offer("Nobody", Change, "Definitely Not A Room"),
            offer("Nobody Else", Change, "Also Not A Room"),
        ];
        let advice = advise(
            &state,
            &offers,
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert!(advice.warnings.contains(&Warning::UnresolvedArchitects));
        assert!(!advice.recommendations.is_empty());
        assert!(advice.recommendations[0].option.architect.is_none());
    }

    // The invisible state POE-243 exists for: the panel prints two blocks and
    // the read produced one. The kill on offer is then the only kill there
    // was, and the advice has to say so — before this, a block that never
    // parsed produced NO warning at all, because `UnresolvedArchitects` fires
    // only when every offer is missing.
    //
    // Fails if the check is dropped, or keyed on the offers that RESOLVED
    // rather than on the blocks that were read.
    #[test]
    fn a_panel_read_that_produced_one_architect_block_says_the_kill_was_forced() {
        let state = board(&[], &[(D1, E1), (E1, E2)], D1, 5);
        let offers = vec![offer("Quipolatl", Upgrade, "Armoury")];

        let advice = advise(&state, &offers, 0, &rush(), &TempleConfig::default(), N, SEED);

        assert!(
            advice.warnings.contains(&Warning::PartialArchitects { read: 1, expected: 2 }),
            "one block read of the two the panel prints: {:?}",
            advice.warnings,
        );
        assert!(
            !advice.warnings.contains(&Warning::UnresolvedArchitects),
            "the block that WAS read resolved fine; the complaint is about the missing one",
        );
    }

    // The other side of the same gate: both blocks read, nothing to warn
    // about. Fails if the warning fires on every read, which would make it
    // noise the player learns to ignore.
    #[test]
    fn a_panel_read_that_produced_both_blocks_says_nothing_about_a_partial_read() {
        let state = board(&[], &[(D1, E1), (E1, E2)], D1, 5);

        let advice = advise(
            &state,
            &junk_offers(),
            0,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );

        assert!(
            !advice
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::PartialArchitects { .. })),
            "two of two is a whole read: {:?}",
            advice.warnings,
        );
    }

    // Zero blocks read, and the panel title proves the panel WAS on screen —
    // so the silence is the OCR's, not the crop's, and it is reportable. The
    // wording differs from the one-block case because there is no kill on
    // screen to call forced.
    //
    // Fails if the zero case is excluded, which is the state that reached the
    // 2026-09-03 laptop board: no offers, no warning, doors-only advice
    // rendered with full confidence.
    #[test]
    fn a_legible_title_with_no_architect_block_at_all_is_still_a_partial_read() {
        let mut state = board(&[], &[(D1, E1), (E1, E2)], D1, 5);
        state.panel_title_read = true;

        let advice = advise(&state, &[], 0, &rush(), &TempleConfig::default(), N, SEED);

        let partial = advice
            .warnings
            .iter()
            .find(|w| matches!(w, Warning::PartialArchitects { .. }))
            .unwrap_or_else(|| panic!("a partial read must be reported: {:?}", advice.warnings));
        assert_eq!(*partial, Warning::PartialArchitects { read: 0, expected: 2 });
        assert!(
            partial.describe().contains("no architect block was read"),
            "with no kill on screen the wording must not call one forced, got {:?}",
            partial.describe(),
        );
    }

    // …and with nothing read AND no title, there is no evidence the panel was
    // ever in the crop, so there is no claim to make about what it printed.
    // Fails if the zero case is unconditional: every read taken between rooms,
    // or with the panel crop off target, would then assert that two architects
    // were printed and missed.
    #[test]
    fn a_read_with_neither_a_title_nor_a_block_claims_nothing_about_the_panel() {
        let state = board(&[], &[(D1, E1), (E1, E2)], D1, 5);
        assert!(!state.panel_title_read, "nothing named the room");

        let advice = advise(&state, &[], 0, &rush(), &TempleConfig::default(), N, SEED);

        assert!(
            !advice
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::PartialArchitects { .. })),
            "{:?}",
            advice.warnings,
        );
    }

    // The acceptance board for POE-243 (2026-09-03, PC): with BOTH architect
    // blocks read, the kill is the `change`. The overlay showed
    // `upgrade → Armoury` on this board, and the only state that reproduces
    // that output is Atmohua's block missing from the parsed panel — so this
    // is the assertion that says the ranking was never the problem.
    //
    // This pin selects option 2's arithmetic through facts::landing_tier,
    // preserving the change recommendation on the recorded board.
    //
    // The DOOR is deliberately not asserted. The rule chain picks it, no one
    // walked this board, and pinning a corridor nobody chose would make this a
    // change-detector for the chain rather than a check on the kill.
    //
    // Played under Consolidation option 2 (`ResidentCarry`), the arithmetic
    // the recorded play assumed: the change from tier 1 builds tier 2.
    #[test]
    fn the_pc_board_kills_the_change_architect_once_both_blocks_are_read() {
        let case = cases::case_7_armourers_workshop();

        let advice = advise_case_under(&case, &rush(), &played(Some(Consolidation::ResidentCarry)));

        let top = advice.recommendations.first().expect("the board ranks");
        let architect = top
            .option
            .architect
            .as_ref()
            .unwrap_or_else(|| panic!("{}: the top move must carry a kill", case.name));
        assert_eq!(
            (architect.offer_index, architect.kind, architect.display_name),
            (1, Change, "Sanctum of Unity"),
            "{}: Sebastian's read was {}, and the overlay said upgrade → Armoury",
            case.name,
            case.decision,
        );
        assert!(
            !advice
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::PartialArchitects { .. })),
            "precondition: both blocks are read on this fixture, {:?}",
            advice.warnings,
        );
    }

    // The same acceptance board at the numbers the APP runs.
    //
    // The test above is asserted at this suite's `N`/`SEED` (400 and 7), which
    // are not what a player's read uses: `slice::advise_read` passes
    // `slice::ROLLOUTS` (2000) and `slice::SEED` (0x_7065_0171). The rollout is
    // a sampler, so an ordering that holds at one (n, seed) is not an ordering
    // that holds at another — and POE-243's acceptance criterion is about what
    // the OVERLAY says, which only these two numbers decide.
    //
    // Imported rather than retyped: a test carrying its own copy of 2000 would
    // keep passing after production moved off it, which is the whole failure
    // this test exists to close.
    //
    // NOT ignored, measured rather than assumed: 2000 rollouts over this one
    // board is 0.02 s in the release container (2026-09-04), so the five-fold
    // rollout count buys nothing worth a lane exclusion. `N = 400` stays what
    // the OTHER hundred-odd assertions in this module run at — the suite cost
    // is the whole suite, not one case.
    #[test]
    fn the_pc_board_kills_the_change_architect_at_the_production_rollouts() {
        use crate::temple::slice::{ROLLOUTS as PROD_ROLLOUTS, SEED as PROD_SEED};

        let case = cases::case_7_armourers_workshop();

        // The same option as the N = 400 pin above.
        let advice = advise(
            &case.state,
            &case.offers,
            case.keys,
            &rush(),
            &played(Some(Consolidation::ResidentCarry)),
            PROD_ROLLOUTS,
            PROD_SEED,
        );

        let top = advice.recommendations.first().expect("the board ranks");
        let architect = top
            .option
            .architect
            .as_ref()
            .unwrap_or_else(|| panic!("{}: the top move must carry a kill", case.name));
        assert_eq!(
            (architect.offer_index, architect.kind, architect.display_name),
            (1, Change, "Sanctum of Unity"),
            "{}: the verdict at N={PROD_ROLLOUTS} seed={PROD_SEED:#x} — the numbers a read \
             actually uses — must be the one the suite pins at N=400 seed=7",
            case.name,
        );
    }

    // An illegible footer must not read as an optimistic forecast.
    #[test]
    fn an_illegible_incursion_count_is_reported_rather_than_assumed() {
        let state = board(&[], &[(D1, E1), (E1, E2)], D1, 0);
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert!(advice.warnings.contains(&Warning::NoBudget));
    }

    // A key with nowhere useful to go is surfaced, not silently dropped.
    // (Case 5 no longer demonstrates this: its corridors are enumerable now
    // that the kill makes Poison Garden a degree-priced Sanctum — RU declines
    // them, which is a choice, not unspendability. This board has junk kills
    // and no upgrade room, so its same-component corridors stay unenumerable.)
    #[test]
    fn a_key_with_no_useful_corridor_is_reported_as_unspendable() {
        let state = board(
            &[],
            &[(C1, D1), (C1, D2), (C0, D1), (B0, C0), (B1, C2), (C2, D2)],
            C1,
            5,
        );
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert!(advice.warnings.contains(&Warning::KeysUnspendable {
            keys: 1,
            usable: 0
        }));
    }

    // ================================================= the convenience door

    // The same board — every closed corridor out of C1 leads back into its own
    // cluster, so the ranking opens nothing — is exactly where the faint seal
    // has to say something (owner, 2026-09-05). No Apex is reachable and no
    // wanted room is built, so it is the loop fallback: B0 and B1 are both
    // three hops from C1 and the smaller edge wins the tie.
    #[test]
    fn a_move_that_opens_nothing_still_carries_a_convenience_door() {
        let state = board(
            &[],
            &[(C1, D1), (C1, D2), (C0, D1), (B0, C0), (B1, C2), (C2, D2)],
            C1,
            5,
        );
        let advice = advise(
            &state,
            &junk_offers(),
            1,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );

        assert!(advice.recommendations[0].option.doors.is_empty());
        assert!(has_reason(&advice.recommendations[0], |r| matches!(
            r,
            Reason::NoUsableDoor
        )));
        let door = advice.convenience_door.expect("a key and four in-cluster corridors");
        assert_eq!(door.door, Edge::new(B0, C1));
        assert_eq!(door.why, convenience::Why::Loop { far: B0, hops: 3 });
        assert_eq!(
            advice.secondary_door, None,
            "with no primary door there is nothing for a second stone to be second to"
        );
    }

    // With a door recommended the faint seal is the second stone's, and the
    // convenience door stays out of the way — case 8 opens B0-C1.
    #[test]
    fn a_move_that_opens_a_door_carries_no_convenience_door() {
        let advice = advise_case(&cases::case_8_lightning_workshop());

        assert!(!advice.recommendations[0].option.doors.is_empty());
        assert_eq!(advice.convenience_door, None);
    }

    // No key, no door to spend it on: the convenience door is about the key
    // the move has no use for, and with none dropped there is no key.
    #[test]
    fn a_zero_key_incursion_carries_no_convenience_door() {
        let state = board(
            &[],
            &[(C1, D1), (C1, D2), (C0, D1), (B0, C0), (B1, C2), (C2, D2)],
            C1,
            5,
        );
        let advice = advise(
            &state,
            &junk_offers(),
            0,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );

        assert_eq!(advice.convenience_door, None);
    }

    /// A [`Reason`]'s variant name, for asserting *which* rules spoke.
    ///
    /// Taken from the `Debug` output rather than a hand-written match so a new
    /// variant cannot quietly join an expected set.
    fn reason_kind(reason: &Reason) -> String {
        let printed = format!("{reason:?}");
        printed
            .split([' ', '{', '('])
            .next()
            .expect("a non-empty debug rendering")
            .to_string()
    }

    // Every recommendation must be auditable — live board 6 was right for
    // ambiguous reasons and nobody could tell. Asserting the reasons are merely
    // *non-empty* is not that test: before POE-170's fix round every empty door
    // set carried `ZeroKey` ("no key dropped") whether or not a key had
    // dropped, so a keyed board could satisfy a non-empty check with a reason
    // that was false. So pin the whole set each board produces.
    #[test]
    fn each_walked_board_is_explained_by_exactly_the_rules_that_decided_it() {
        let expected: [&[&str]; 8] = [
            // 1 Tombs — a free kill, and a door decided by the scarcity chain.
            // `Ru` is on the corridor into E2's Sanctum, not on the pick.
            &["Rd", "R2", "FasterArrival", "R1Gradient", "Rs", "Ru"],
            // 2 Corruption Chamber — no target room on the board, so no RV:
            // the door chain decides, R1-apex on the Apex corridors, and R4
            // on the corruption upgrade that builds tier 3.
            &["R1Apex", "Rd", "R2", "FasterArrival", "R1Gradient", "Rs", "R4"],
            // 3 Chasm late — the Apex, free at last. Xopec's kill is the
            // priority kill (POE-465): it builds on the Cradle of Creation
            // line, the Rush's target, so it advances the target rather than
            // drawing R4. Xipocado's change builds Lightning Workshop, which
            // is not on the line.
            &[
                "R1Apex",
                "Rd",
                "R2",
                "FasterArrival",
                "R1Gradient",
                "Rs",
                "ArchitectPriority",
                "AdvancesTarget",
            ],
            // 4 Chasm merge — one door changes anything; with no target room
            // on the board the door chain alone ranks it.
            &["Rd", "R2", "FasterArrival", "R1Gradient", "Rs"],
            // 5 Poison Garden — the Sanctum kill makes every corridor
            // degree-priced, so the doors ARE enumerated and each is declined
            // by RU (the top pick still opens nothing); the Cultivar kill
            // prices none of them, so its side still reads NoUsableDoor. With
            // no target room on the board the kills sit inside the band and RT
            // takes the upgrade line. `ZeroKey` here would be a lie: a key did
            // drop.
            //
            // `R1Apex` was in this list until POE-248 and its absence is the
            // FIX, not a re-recording: the corridor that scored it is C0-B0,
            // and on this board B0 is already inside C0's own component (via
            // B0-C1 and C0-C1) while that component holds no Apex. So the door
            // buys neither the Apex nor a new way toward it, and `apex_reach`
            // is silent — see its note. The recorded decision is untouched: the
            // top pick still opens nothing, for RU's reason.
            &[
                "NoUsableDoor",
                "FasterArrival",
                "R1Gradient",
                "R2",
                "R4",
                "Rd",
                "Rs",
                "Rt",
                "Ru",
                "RuDeclined",
            ],
            // 6 Cloister — the blind board, decided by R1 and R2 alone.
            &["Rd", "R2", "FasterArrival", "R1Gradient", "Rs"],
            // 8 Lightning Workshop — R1-apex fires on both corridors (reaching
            // on B0, adjacent on B1) and the rest of the door chain ranks them.
            // On the kill: R4, because the `upgrade` builds tier 3 under
            // option 1 and maxes C1 out of the drop pool. RC is silent: the
            // change builds tier 1, and C2's Sanctum lifting it to 2 does not
            // reach the upgrade's 3 (POE-415). R4's carve-out does NOT veto the
            // upgrade — C2 already holds three open corridors against two
            // picks, so C1 is not a live target it could lose. No
            // `ExpectedValue`: neither kill is vetoed and the rollout cannot
            // separate them, so the band covers both.
            &["R1Apex", "Rd", "R2", "FasterArrival", "R1Gradient", "Rs", "R4"],
            // 9 Corruption Chamber arrival — the corruption upgrade builds an
            // ordinary room, so no RV and R4 on its tier 3; R1Apex on the B0
            // corridor, which is Apex-adjacent even though the Apex is already
            // in the room's own cluster; the door chain on every keyed set, RA
            // included; the rollout separates options past the band, so they
            // carry ExpectedValue.
            &[
                "ExpectedValue",
                "R1Apex",
                "Rd",
                "R2",
                "FasterArrival",
                "R1Gradient",
                "Rs",
                "R4",
            ],
        ];

        for (case, want) in cases::retrospective().into_iter().zip(expected) {
            let advice = advise_case(&case);
            let mut got: Vec<String> = Vec::new();
            for ranked in &advice.recommendations {
                for reason in &ranked.reasons {
                    assert!(!reason.describe().is_empty());
                    let kind = reason_kind(reason);
                    if !got.contains(&kind) {
                        got.push(kind);
                    }
                }
            }
            got.sort();
            let mut want: Vec<String> = want.iter().map(|s| s.to_string()).collect();
            want.sort();
            assert_eq!(got, want, "{}", case.name);
            assert!(
                !advice.recommendations[0].reasons.is_empty(),
                "{}: the pick the overlay prints must always be auditable",
                case.name
            );
        }
    }


    // ============================ the production path over the walked boards

    /// The door the app actually recommends on each walked board, pinned.
    ///
    /// Under the Cradle target set two boards leave Sebastian's recorded
    /// door: board 2 takes the Apex he played, and board 9 the Apex-adjacent
    /// B0 rather than his C1-D1.
    ///
    /// The recorded decisions are mostly about the DOOR, and the door axis is
    /// decided by the rule chain rather than by the rollout — R1, R2 and RS
    /// read the graph, not the prices. So this is NOT written as "the same
    /// door as the fixture": comparing two runs of the same rule chain cannot
    /// fail, and no value-side change moves any of these (a `room_baseline`
    /// left unscaled, a `room_baseline` x50 and a whole extra chaos hop of
    /// `path_cost` were all tried and moved nothing). What can move them is a
    /// change to the door chain itself, and what these literals are for is
    /// stating that the profile the app SHIPS with still reproduces
    /// Sebastian's recorded play — the fixture suites above only say the
    /// fixture does. Verified against `best_row.min(far.row())` inverted to
    /// `.max` in `rules::door_key`, which moves board 1 off Chamber of Iron.
    #[test]
    fn the_production_profile_opens_the_pinned_door_on_every_walked_board() {
        let production = production_profile();

        let opened: Vec<(&str, Vec<Slot>)> = all_walked_boards()
            .iter()
            .map(|case| {
                let advice = advise_case_with(case, &production);
                (
                    case.name,
                    opened_toward(
                        &advice.recommendations[0].option.doors,
                        case.state.position.expect("every walked board has a position"),
                    ),
                )
            })
            .collect();

        assert_eq!(
            opened,
            vec![
                // "open D3-C2 (up, and merges two singletons)"
                ("1 Tombs", vec![C2]),
                // "open B1-A0": the Corruption Chamber is no target
                ("2 CorruptionChamber", vec![A0]),
                // "open B0-A0 (the Apex is finally free value)"
                ("3 Chasm-late", vec![A0]),
                // "open B1-C2 (the only door that changes anything)"
                ("4 Chasm-merge", vec![C2]),
                // "open NO door (RU)"
                ("5 PoisonGarden", Vec::new()),
                // "open D1-C1 (Cellar: one row up AND a singleton)"
                ("6 Cloister-blind", vec![C1]),
                // "open B0-C1 (B0 already reaches the Apex)"
                ("8 LightningWorkshop", vec![B0]),
                // Sebastian opened C1-D1; with no RV the Apex-adjacent B0 wins
                ("9 CorruptionChamber-arrival", vec![B0]),
                // Board 7 records a MISS on the kill; its door was never in
                // dispute.
                ("7 ArmourersWorkshop-PC", vec![B1]),
            ],
        );
    }

    /// The 2026-10-17 tier rungs change one kill from the strategy fixture,
    /// under Consolidation option 1, the played default. These are rollout
    /// outcomes under the WI-5 draw rules (POE-415): whole-map no-revisit, two
    /// changes on a tier-0 room, pending-room carry-over. Board 1 records
    /// "kill either". Board 7 no longer moves: under the Cradle target set the
    /// fixture takes the Chamber of Iron upgrade the shipped profile takes.
    /// Board 8 does not move: with its upgrade building tier 3 and its
    /// explosives change tier 1, the fixture takes the Conduit upgrade too
    /// (POE-415). Board 2 still upgrades corruption. Board 3 does not move:
    /// under either profile the Cradle-line priority (POE-465) kills Xopec's
    /// throne_of_atziri change. Pin the exact differences so a ranking change
    /// cannot silently move another board.
    #[test]
    fn the_production_tier_rungs_change_one_fixture_kill() {
        let production = production_profile();

        let moved: Vec<(&str, Option<(OfferKind, String, u8)>, Option<(OfferKind, String, u8)>)> =
            all_walked_boards()
                .iter()
                .filter_map(|case| {
                    let fixture = top_kill_of(&advise_case(case));
                    let shipped = top_kill_of(&advise_case_with(case, &production));
                    (fixture != shipped).then(|| (case.name, fixture, shipped))
                })
                .collect();

        assert_eq!(
            moved,
            vec![(
                "1 Tombs",
                Some((Change, "museum_of_artefacts".to_string(), 1)),
                Some((Change, "hall_of_champions".to_string(), 1)),
            )],
        );
    }

    /// Board 7 plays Sebastian's kill on BOTH production reads.
    ///
    /// The board the app got wrong, and the reason the two instrumental lines
    /// are priced at their letter at all. He played *change to Sanctum of
    /// Unity*; the overlay said *upgrade to Armoury*, and so did every
    /// chaos-denominated profile while Temple Nexus was scored at zero — the
    /// 6 c junk room beat the shrine that lifts a tier. Its own recorded
    /// `decision` string is the assertion message, so a failure names his play
    /// rather than a tuple.
    ///
    /// Played under Consolidation option 2 (`ResidentCarry`), the arithmetic
    /// his recorded change to the tier-2 Sanctum assumed.
    ///
    /// Fails if the bridge zeroes the instrumental lines again, on either read.
    #[test]
    fn the_production_profile_plays_sebastians_kill_on_board_seven_cold_and_live() {
        let case = cases::case_7_armourers_workshop();
        let sebastian = Some((Change, "upgrade".to_string(), 2));
        let option_two = played(Some(Consolidation::ResidentCarry));

        for (label, profile) in [
            ("cold", production_profile()),
            ("live", live_production_profile()),
        ] {
            assert_eq!(
                top_kill_of(&advise_case_under(&case, &profile, &option_two)),
                sebastian,
                "{label}: {}",
                case.decision,
            );
        }
    }

    /// Board 2's recorded kill, and the production profile reaches it too.
    ///
    /// *"Upgrade the corruption line"*: Locus of Corruption is no target any
    /// more, but it is still the board's richest room at its chaos price.
    /// Fails if the corruption line stops outranking a junk `change` once
    /// every other line on the board carries a value of its own. Under option
    /// 1, the played default, the upgrade from tier 1 builds tier 3.
    #[test]
    fn the_production_profile_still_upgrades_the_corruption_line_on_board_two() {
        let case = cases::case_2_corruption_chamber();

        let advice = advise_case_with(&case, &production_profile());

        assert_eq!(
            top_kill_of(&advice),
            Some((Upgrade, "corruption".to_string(), 3)),
            "recorded decision: {}",
            case.decision,
        );
    }


    /// The door on every walked board, at the capture's prices.
    ///
    /// The same literals as
    /// `the_production_profile_opens_the_pinned_door_on_every_walked_board`,
    /// and that is the finding: a board whose value spread goes from 800/2 to
    /// 846/2.1 opens the same corridor every time, because the door axis is the
    /// rule chain's and the rule chain reads the graph. Verified against
    /// `best_row.min(far.row())` inverted to `.max` in `rules::door_key`, which
    /// moves board 1 off Chamber of Iron.
    #[test]
    fn the_live_priced_profile_opens_the_pinned_door_on_every_walked_board() {
        let live = live_production_profile();

        let opened: Vec<(&str, Vec<Slot>)> = all_walked_boards()
            .iter()
            .map(|case| {
                let advice = advise_case_with(case, &live);
                (
                    case.name,
                    opened_toward(
                        &advice.recommendations[0].option.doors,
                        case.state.position.expect("every walked board has a position"),
                    ),
                )
            })
            .collect();

        assert_eq!(
            opened,
            vec![
                ("1 Tombs", vec![C2]),
                ("2 CorruptionChamber", vec![A0]),
                ("3 Chasm-late", vec![A0]),
                ("4 Chasm-merge", vec![C2]),
                ("5 PoisonGarden", Vec::new()),
                ("6 Cloister-blind", vec![C1]),
                ("8 LightningWorkshop", vec![B0]),
                ("9 CorruptionChamber-arrival", vec![B0]),
                ("7 ArmourersWorkshop-PC", vec![B1]),
            ],
        );
    }

    /// The kill on every walked board at the capture's prices, under
    /// Consolidation option 1, the played default. These are the valuation's
    /// current answers, including the C1 instrumental rung for Temple Nexus.
    /// These are rollout outcomes under the WI-5 draw rules (POE-415): whole-
    /// map no-revisit, two changes on a tier-0 room, pending-room carry-over.
    /// Board 3 kills Xopec's Cradle-line change, the POE-465 priority, which
    /// builds tier 3 (POE-415). Under the Cradle target set board 7 takes the
    /// Sanctum change at tier 1 here, and board 6 the Hall of Champions change;
    /// board 7's recorded tier-2 Sanctum is pinned under option 2 by
    /// `the_production_profile_plays_sebastians_kill_on_board_seven_cold_and_live`.
    /// Fails if the bridge stops copying the valuation into the profile.
    #[test]
    fn the_live_priced_profile_kills_this_on_every_walked_board() {
        let live = live_production_profile();

        let killed: Vec<(&str, Option<(OfferKind, String, u8)>)> = all_walked_boards()
            .iter()
            .map(|case| (case.name, top_kill_of(&advise_case_with(case, &live))))
            .collect();

        assert_eq!(
            killed,
            vec![
                ("1 Tombs", Some((Change, "hall_of_champions".to_string(), 1))),
                ("2 CorruptionChamber", Some((Upgrade, "corruption".to_string(), 3))),
                ("3 Chasm-late", Some((Change, "throne_of_atziri".to_string(), 3))),
                ("4 Chasm-merge", Some((Change, "hybridisation_chamber".to_string(), 1))),
                ("5 PoisonGarden", Some((Change, "upgrade".to_string(), 1))),
                ("6 Cloister-blind", Some((Change, "hall_of_champions".to_string(), 1))),
                ("8 LightningWorkshop", Some((Upgrade, "conduit_of_lightning".to_string(), 3))),
                ("9 CorruptionChamber-arrival", Some((Upgrade, "corruption".to_string(), 3))),
                ("7 ArmourersWorkshop-PC", Some((Change, "upgrade".to_string(), 1))),
            ],
        );
    }


    // ================================ POE-465: the Cradle of Creation line

    /// The reason a priority kill carries, as the overlay prints it. The
    /// surviving architect acts, so the kill that builds the Cradle is the
    /// one that leaves Xipocado alive.
    const CRADLE_PRIORITY_REASON: &str =
        "priority: Cradle of Creation — kill this architect so Xipocado builds it";

    /// Each recommendation as `(offer index, doors)`, best first.
    fn ranked_sequence(advice: &Advice) -> Vec<(Option<usize>, String)> {
        advice
            .recommendations
            .iter()
            .map(|r| {
                (
                    r.option.architect.as_ref().map(|a| a.offer_index),
                    r.option.doors_label(),
                )
            })
            .collect()
    }

    /// Every recommendation with its EV and the rules that spoke, for a
    /// failure message that shows the whole ranking.
    fn ranking_dump(advice: &Advice) -> String {
        advice
            .recommendations
            .iter()
            .map(|r| {
                format!(
                    "  {:?} [{}] ev={:.3} {:?}",
                    r.option.architect.as_ref().map(|a| a.offer_index),
                    r.option.doors_label(),
                    r.ev,
                    r.reasons.iter().map(reason_kind).collect::<Vec<_>>()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The best EV among the options whose kill is offer `index` (or is not).
    fn best_ev(advice: &Advice, is_offer: impl Fn(Option<usize>) -> bool) -> f64 {
        advice
            .recommendations
            .iter()
            .filter(|r| is_offer(r.option.architect.as_ref().map(|a| a.offer_index)))
            .map(|r| r.ev)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// What makes a row a real test of the priority rather than of the
    /// ranking it overrides.
    enum Precondition {
        /// The qualifying kill's best option prices below the rival's.
        LowerEv,
        /// R4's carve-out vetoes the qualifying kill.
        Vetoed,
        /// Every kill on the board qualifies.
        AllQualify,
        /// A Xipocado block that does not build on the Cradle line sits
        /// outside `qualifying`, so the architect's name alone must not
        /// qualify it.
        XipocadoOffTheLine,
    }

    struct PriorityRow {
        name: &'static str,
        state: BoardState,
        offers: Vec<ArchitectOffer>,
        keys: u8,
        profile: StrategyProfile,
        /// The panel indexes of the offers that qualify for the priority.
        qualifying: &'static [usize],
        precondition: Precondition,
        /// `(offer index, doors)` per recommendation, best first.
        expected: Vec<(Option<usize>, &'static str)>,
    }

    /// Every way the advice for `row` misses the priority contract.
    fn priority_failures(row: &PriorityRow) -> Vec<String> {
        let advice = advise(
            &row.state,
            &row.offers,
            row.keys,
            &row.profile,
            &TempleConfig::default(),
            N,
            SEED,
        );
        let dump = ranking_dump(&advice);
        let qualifies = |index: Option<usize>| index.is_some_and(|i| row.qualifying.contains(&i));
        let mut failures = Vec::new();

        let precondition_holds = match row.precondition {
            Precondition::LowerEv => {
                best_ev(&advice, qualifies) < best_ev(&advice, |i| !qualifies(i))
            }
            Precondition::Vetoed => advice.recommendations.iter().any(|r| {
                qualifies(r.option.architect.as_ref().map(|a| a.offer_index))
                    && has_reason(r, |x| matches!(x, Reason::R4CarveOutVeto { .. }))
            }),
            Precondition::AllQualify => advice
                .recommendations
                .iter()
                .all(|r| qualifies(r.option.architect.as_ref().map(|a| a.offer_index))),
            Precondition::XipocadoOffTheLine => row.offers.iter().enumerate().any(|(i, o)| {
                o.architect_name.eq_ignore_ascii_case("Xipocado") && !row.qualifying.contains(&i)
            }),
        };
        if !precondition_holds {
            failures.push(format!("{}: the board no longer tests the priority:\n{dump}", row.name));
        }

        let got = ranked_sequence(&advice);
        let want: Vec<(Option<usize>, String)> = row
            .expected
            .iter()
            .map(|(index, doors)| (*index, doors.to_string()))
            .collect();
        if got != want {
            failures.push(format!(
                "{}: ranking\n  got  {got:?}\n  want {want:?}\n{dump}",
                row.name
            ));
        }

        let want_priority = row.expected.first().and_then(|(index, _)| *index).map(|top| {
            AdvicePriority {
                architect_index: top,
                reason: CRADLE_PRIORITY_REASON.to_string(),
            }
        });
        if advice.priority != want_priority {
            failures.push(format!(
                "{}: priority {:?}, want {want_priority:?}",
                row.name, advice.priority
            ));
        }

        for ranked in &advice.recommendations {
            let index = ranked.option.architect.as_ref().map(|a| a.offer_index);
            let marked = has_reason(ranked, |r| matches!(r, Reason::ArchitectPriority));
            if marked != qualifies(index) {
                failures.push(format!(
                    "{}: option {index:?} [{}] carries ArchitectPriority={marked}: {:?}",
                    row.name,
                    ranked.option.doors_label(),
                    ranked.reasons
                ));
            }
        }
        failures
    }

    fn assert_priority_rows(rows: &[PriorityRow]) {
        let failures: Vec<String> = rows.iter().flat_map(priority_failures).collect();
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    /// A tier-`tier` room of `line` at D1, beside a connected Doryani's
    /// Institute.
    fn priority_board(line: &str, tier: u8) -> BoardState {
        board(&[(D1, line, tier), (E2, "gem", 3)], &[(D1, E1), (E1, E2)], D1, 5)
    }

    /// The door sets [`priority_board`] enumerates at one key, in the order
    /// the door chain ranks them inside every kill's group.
    const PRIORITY_BOARD_DOORS: [&str; 6] =
        ["C0-D1", "C1-D1", "D0-D1", "D1-D2", "D1-E0", "no door"];

    /// `(offer index, doors)` for whole kill groups, `kills` best first.
    fn kill_groups(kills: &[usize], doors: &[&'static str]) -> Vec<(Option<usize>, &'static str)> {
        kills
            .iter()
            .flat_map(|kill| doors.iter().map(move |door| (Some(*kill), *door)))
            .collect()
    }

    fn carve_out_profile() -> StrategyProfile {
        let mut profile = rush();
        profile.r4_keep_upgrade_targets = true;
        profile
    }

    /// The Rush with the corruption line priced 100 at tier 3, far above the
    /// Cradle's 9, so a rival corruption kill carries the higher EV and a
    /// priority row tests the priority rather than the ranking it overrides.
    /// Corruption is an ordinary priced room here, not a target.
    fn rival_priced() -> StrategyProfile {
        let mut profile = rush();
        profile.room_values.insert(Line::Corruption, [0.0, 0.0, 100.0]);
        profile
    }

    /// R4's carve-out on a profile that does not chase the Cradle line.
    ///
    /// A target-line kill is never vetoed, and the Rush's one target is the
    /// Cradle, so the veto the priority overrides is only reachable while the
    /// Cradle is off the target set.
    fn carve_out_off_the_cradle() -> StrategyProfile {
        let mut profile = hybridisation_chase();
        profile.r4_keep_upgrade_targets = true;
        profile
    }

    mod keep_happy {
        use super::*;

        /// The built tier `advise` resolved for each offer block, by index.
        fn built_tiers(advice: &Advice, blocks: usize) -> Vec<Option<u8>> {
            (0..blocks)
                .map(|index| choice_for(advice, index).map(|a| a.built_tier.get()))
                .collect()
        }

        // keep:happy — POE-415 AC (a), through `advise`. Standing in a tier-1
        // room with an upgrade (block 0) and a change (block 1) on the panel,
        // each Consolidation of Power option lands the two kills at its ruled
        // tiers, with no randomness: option 1 upgrade 3 / change 1, option 2
        // 2 / 2, not taken 2 / 1, unset plays option 1. In a tier-0 room the
        // only kill is a change, and it lands at tier 1 under every option.
        #[test]
        fn each_consolidation_option_lands_the_kills_at_its_ruled_tiers() {
            let tier_one = || board(&[(D1, "chamber_of_iron", 1)], &[(D1, E1)], D1, 5);
            let tier_zero = || board(&[(D1, "", 0)], &[(D1, E1)], D1, 5);
            let both = || {
                vec![
                    offer("Quipolatl", Upgrade, "Armoury"),
                    offer("Ticaba", Change, "Storage Room"),
                ]
            };
            let change_only = || vec![offer("Ticaba", Change, "Storage Room")];
            let rows: Vec<(&str, BoardState, Vec<ArchitectOffer>, Option<Consolidation>, Vec<Option<u8>>)> = vec![
                ("tier 1, option 1", tier_one(), both(), Some(Consolidation::NonResidentExtraTier), vec![Some(3), Some(1)]),
                ("tier 1, option 2", tier_one(), both(), Some(Consolidation::ResidentCarry), vec![Some(2), Some(2)]),
                ("tier 1, not taken", tier_one(), both(), Some(Consolidation::NotTaken), vec![Some(2), Some(1)]),
                ("tier 1, unset", tier_one(), both(), None, vec![Some(3), Some(1)]),
                ("tier 0, option 1", tier_zero(), change_only(), Some(Consolidation::NonResidentExtraTier), vec![Some(1)]),
                ("tier 0, option 2", tier_zero(), change_only(), Some(Consolidation::ResidentCarry), vec![Some(1)]),
                ("tier 0, not taken", tier_zero(), change_only(), Some(Consolidation::NotTaken), vec![Some(1)]),
                ("tier 0, unset", tier_zero(), change_only(), None, vec![Some(1)]),
            ];
            let mut failures = Vec::new();
            for (name, state, offers, option, want) in &rows {
                // Every seed must agree: the landing is deterministic.
                for seed in [SEED, SEED + 1, SEED + 2] {
                    let advice = advise(state, offers, 0, &rush(), &played(*option), 0, seed);
                    let got = built_tiers(&advice, offers.len());
                    if got != *want {
                        failures.push(format!(
                            "{name} (seed {seed}): built tiers (upgrade, change) got {got:?}, want {want:?}"
                        ));
                    }
                }
            }
            assert!(failures.is_empty(), "\n{}", failures.join("\n"));
        }

        // keep:happy — POE-415 AC (b). A tier-1 room beside a connected
        // Shrine of Empowerment I, both offered lines junk to the rusher, so
        // the two kills sit inside the noise band and the architect rules
        // decide. Under option 1 the upgrade builds tier 3 and the change
        // only tier 1, so the Shrine cannot make the change catch up and the
        // upgrade is taken; under option 2 both build tier 2 and the change
        // plus the Shrine's +1 is the better kill.
        #[test]
        fn option_one_takes_the_upgrade_where_option_two_takes_the_change() {
            let state = board(
                &[(D1, "chamber_of_iron", 1), (C1, "upgrade", 1)],
                &[(C1, D1), (D1, E1)],
                D1,
                3,
            );
            let offers = vec![
                offer("Quipolatl", Upgrade, "Armoury"),
                offer("Ticaba", Change, "Storage Room"),
            ];
            let top = |option: Consolidation| {
                let advice = advise(&state, &offers, 0, &rush(), &played(Some(option)), N, SEED);
                (top_kill_of(&advice), ranking_dump(&advice))
            };
            let (one, one_dump) = top(Consolidation::NonResidentExtraTier);
            let (two, two_dump) = top(Consolidation::ResidentCarry);
            assert_eq!(
                one.as_ref().map(|(kind, _, _)| *kind),
                Some(Upgrade),
                "option 1 takes the upgrade: {one:?}\n{one_dump}"
            );
            assert_eq!(
                two.as_ref().map(|(kind, _, _)| *kind),
                Some(Change),
                "option 2 takes the change: {two:?}\n{two_dump}"
            );
        }

        // keep:happy — POE-415 AC (d), Operator note 6. An offer to Cradle of
        // Creation builds Cradle of Creation at tier 3 under every
        // Consolidation option and unset: a change from a tier-0, tier-1 or
        // tier-2 room or with the current room unread, and an upgrade on a
        // legacy Queen-line read (Royal Meeting Room I prints Hall of Lords).
        #[test]
        fn a_cradle_of_creation_offer_builds_tier_three_under_every_option() {
            let standing = |tier: u8, line: &str| {
                let key = if tier == 0 { "" } else { line };
                board(&[(D1, key, tier)], &[(D1, E1)], D1, 5)
            };
            let unread = || {
                let mut state = board(&[(D1, "", 0)], &[(D1, E1)], D1, 5);
                state.current_tier = None;
                state
            };
            let boards: Vec<(&str, BoardState, ArchitectOffer)> = vec![
                ("change from tier 0", standing(0, "corruption"), offer("Tzamoto", Change, "Cradle of Creation")),
                ("change from tier 1", standing(1, "corruption"), offer("Tzamoto", Change, "Cradle of Creation")),
                ("change from tier 2", standing(2, "corruption"), offer("Tzamoto", Change, "Cradle of Creation")),
                ("change, current room unread", unread(), offer("Tzamoto", Change, "Cradle of Creation")),
                ("upgrade on Royal Meeting Room I", standing(1, "throne_of_atziri"), offer("Xopec", Upgrade, "Hall of Lords")),
            ];
            let options = [
                Some(Consolidation::NonResidentExtraTier),
                Some(Consolidation::ResidentCarry),
                Some(Consolidation::NotTaken),
                None,
            ];
            let mut failures = Vec::new();
            for (name, state, block) in &boards {
                for option in options {
                    let advice = advise(state, std::slice::from_ref(block), 0, &rush(), &played(option), 0, SEED);
                    let got = choice_for(&advice, 0).map(|a| (a.built_tier.get(), a.display_name));
                    if got != Some((3, "Cradle of Creation")) {
                        failures.push(format!(
                            "{name}, {option:?}: got {got:?}, want Some((3, \"Cradle of Creation\")); warnings {:?}",
                            advice.warnings
                        ));
                    }
                }
            }
            assert!(failures.is_empty(), "\n{}", failures.join("\n"));
        }

        // keep:happy — AC-1. A kill that builds on the Cradle of Creation
        // line ranks first under either offer kind, whoever prints it,
        // although it prices below the rival kill. Every other option stays,
        // after it.
        #[test]
        fn a_cradle_line_offer_is_killed_first_over_a_higher_expected_value() {
            assert_priority_rows(&[
                PriorityRow {
                    name: "change printing Cradle of Creation, lower EV",
                    state: priority_board("corruption", 1),
                    offers: vec![
                        offer("Tzamoto", Change, "Cradle of Creation"),
                        offer("Azcapa", Upgrade, "Catalyst of Corruption"),
                    ],
                    keys: 1,
                    profile: rival_priced(),
                    qualifying: &[0],
                    precondition: Precondition::LowerEv,
                    expected: kill_groups(&[0, 1], &PRIORITY_BOARD_DOORS),
                },
                PriorityRow {
                    name: "upgrade to Hall of Lords in the second block, lower EV",
                    state: priority_board("throne_of_atziri", 1),
                    offers: vec![
                        offer("Azcapa", Change, "Corruption Chamber"),
                        offer("Xopec", Upgrade, "Hall of Lords"),
                    ],
                    keys: 1,
                    profile: rival_priced(),
                    qualifying: &[1],
                    precondition: Precondition::LowerEv,
                    expected: kill_groups(&[1, 0], &PRIORITY_BOARD_DOORS),
                },
            ]);
        }
    }

    mod keep_critical {
        use super::*;

        // keep:critical — POE-415, RC's reference tier. Silent consequence:
        // with the current room unread the board carries a tier-0 stand-in,
        // and an RC measured against it promotes the change, a plausible
        // ranking that inverts the option-1 reading, and nothing flags it.
        // Under option 1 the change builds tier 1 and the upgrade (printed at
        // tier 2) builds tier 3, so the connected Shrine's +1 cannot close the
        // gap: RC stays silent. The same board read at tier 1 under option 2
        // lands both kills at tier 2, and RC fires.
        #[test]
        fn rc_measures_the_change_against_the_upgrade_it_competes_with() {
            let offers = vec![
                offer("Quipolatl", Upgrade, "Armoury"),
                offer("Ticaba", Change, "Storage Room"),
            ];
            let mut unread = board(&[(D1, "", 0), (C1, "upgrade", 1)], &[(C1, D1), (D1, E1)], D1, 5);
            unread.current_tier = None;
            let read_at_one =
                board(&[(D1, "chamber_of_iron", 1), (C1, "upgrade", 1)], &[(C1, D1), (D1, E1)], D1, 5);
            let rows = [
                ("current room unread, option 1", unread, Consolidation::NonResidentExtraTier, (3, 1), false),
                ("read at tier 1, option 2", read_at_one, Consolidation::ResidentCarry, (2, 2), true),
            ];
            let mut failures = Vec::new();
            for (name, state, option, (upgrade_tier, change_tier), want_rc) in rows {
                let advice = advise(&state, &offers, 0, &rush(), &played(Some(option)), 0, SEED);
                let tiers = (
                    choice_for(&advice, 0).map(|a| a.built_tier.get()),
                    choice_for(&advice, 1).map(|a| a.built_tier.get()),
                );
                if tiers != (Some(upgrade_tier), Some(change_tier)) {
                    failures.push(format!(
                        "{name}: built tiers (upgrade, change) {tiers:?}, want ({upgrade_tier}, {change_tier}); warnings {:?}",
                        advice.warnings
                    ));
                    continue;
                }
                let change_rc: Vec<bool> = advice
                    .recommendations
                    .iter()
                    .filter(|r| r.option.architect.as_ref().map(|a| a.offer_index) == Some(1))
                    .map(|r| has_reason(r, |x| matches!(x, Reason::Rc { upgrader: C1 })))
                    .collect();
                if change_rc.is_empty() || change_rc.iter().any(|rc| *rc != want_rc) {
                    failures.push(format!(
                        "{name}: RC on the change {change_rc:?}, want {want_rc}\n{}",
                        ranking_dump(&advice)
                    ));
                }
            }
            assert!(failures.is_empty(), "\n{}", failures.join("\n"));
        }

        // keep:critical — AC-1. Silent consequence: the overlay names
        // Xipocado's box as the priority kill, a plausible instruction the
        // player trusts, and killing Xipocado is the one move that guarantees
        // Xipocado cannot build the Cradle. A Xipocado block whose kill does
        // not build on the Cradle line, on a board with no Cradle-line offer,
        // gets no priority under either offer kind, any letter case or R4's
        // carve-out veto: the ranking it would have overridden stands.
        #[test]
        fn a_xipocado_offer_off_the_cradle_line_gets_no_priority() {
            let (eight, _) = board_eight();
            let rows: [(&str, BoardState, Vec<ArchitectOffer>, u8, StrategyProfile, usize); 3] = [
                (
                    "change, upper case, lower EV",
                    priority_board("corruption", 1),
                    vec![
                        offer("XIPOCADO", Change, "Grand Reactor"),
                        offer("Azcapa", Upgrade, "Catalyst of Corruption"),
                    ],
                    1,
                    rival_priced(),
                    1,
                ),
                (
                    "upgrade, lower case, lower EV",
                    priority_board("conduit_of_lightning", 1),
                    vec![
                        offer("Azcapa", Change, "Corruption Chamber"),
                        offer("xipocado", Upgrade, "Grand Reactor"),
                    ],
                    1,
                    rival_priced(),
                    0,
                ),
                (
                    "upgrade the R4 carve-out vetoes",
                    eight,
                    vec![
                        offer("Xipocado", Upgrade, "Engineering Department"),
                        offer("Juatalotli", Change, "Storage Room"),
                    ],
                    0,
                    carve_out_profile(),
                    1,
                ),
            ];
            let mut failures = Vec::new();
            for (name, state, offers, keys, profile, top) in rows {
                let advice =
                    advise(&state, &offers, keys, &profile, &TempleConfig::default(), N, SEED);
                let first = advice
                    .recommendations
                    .first()
                    .and_then(|r| r.option.architect.as_ref().map(|a| a.offer_index));
                let marked = advice
                    .recommendations
                    .iter()
                    .any(|r| has_reason(r, |x| matches!(x, Reason::ArchitectPriority)));
                if advice.priority.is_some() || marked || first != Some(top) {
                    failures.push(format!(
                        "{name}: priority {:?}, ArchitectPriority marked={marked}, top kill {first:?} (want {top})\n{}",
                        advice.priority,
                        ranking_dump(&advice)
                    ));
                }
            }
            assert!(failures.is_empty(), "\n{}", failures.join("\n"));
        }

        // keep:critical — AC-2 and AC-3. Silent consequence: the overlay
        // points at the wrong architect block, a plausible kill the player
        // trusts, and the Cradle of Creation line is lost to a rival move. Any architect's kill that builds on the throne_of_atziri line
        // ranks first at every tier a kill can build; with two priority kills
        // the old order between them stands.
        #[test]
        fn a_kill_on_the_cradle_line_is_ranked_first_at_every_buildable_tier() {
            let throne_eight = board(
                &[(D1, "throne_of_atziri", 1), (C1, "upgrade", 2), (C2, "corruption", 1)],
                &[(C1, D1), (D1, E1), (E1, E2)],
                D1,
                6,
            );
            let three = cases::case_3_chasm_late();
            assert_priority_rows(&[
                PriorityRow {
                    name: "Hall of Lords (tier 2), lower EV",
                    state: priority_board("corruption", 1),
                    offers: vec![
                        offer("Azcapa", Upgrade, "Catalyst of Corruption"),
                        offer("Xopec", Change, "Royal Meeting Room"),
                    ],
                    keys: 1,
                    profile: rival_priced(),
                    qualifying: &[1],
                    precondition: Precondition::LowerEv,
                    expected: kill_groups(&[1, 0], &PRIORITY_BOARD_DOORS),
                },
                PriorityRow {
                    name: "Cradle of Creation (tier 3), lower EV",
                    state: priority_board("corruption", 2),
                    offers: vec![
                        offer("Xopec", Change, "Royal Meeting Room"),
                        offer("Azcapa", Upgrade, "Locus of Corruption"),
                    ],
                    keys: 1,
                    profile: rival_priced(),
                    qualifying: &[0],
                    precondition: Precondition::LowerEv,
                    expected: kill_groups(&[0, 1], &PRIORITY_BOARD_DOORS),
                },
                PriorityRow {
                    name: "Hall of Lords upgrade across the R4 carve-out veto",
                    state: throne_eight,
                    offers: vec![
                        offer("Ticaba", Upgrade, "Hall of Lords"),
                        offer("Juatalotli", Change, "Storage Room"),
                    ],
                    keys: 0,
                    profile: carve_out_off_the_cradle(),
                    qualifying: &[0],
                    precondition: Precondition::Vetoed,
                    expected: kill_groups(&[0, 1], &["no door"]),
                },
                // Both build the same tier-2 room on the Cradle line, so the
                // priority separates neither: the order the ranking gave them
                // stands.
                PriorityRow {
                    name: "two priority kills keep their ranked order",
                    state: priority_board("corruption", 1),
                    offers: vec![
                        offer("Xopec", Change, "Royal Meeting Room"),
                        offer("Tzamoto", Change, "Cradle of Creation"),
                    ],
                    keys: 1,
                    profile: rush(),
                    qualifying: &[0, 1],
                    precondition: Precondition::AllQualify,
                    expected: kill_groups(&[0, 1], &PRIORITY_BOARD_DOORS),
                },
                PriorityRow {
                    name: "case 3: Royal Meeting Room (tier 1) beside an off-line Xipocado",
                    state: three.state,
                    offers: three.offers,
                    keys: three.keys,
                    profile: rush(),
                    qualifying: &[0],
                    precondition: Precondition::XipocadoOffTheLine,
                    expected: kill_groups(&[0, 1], &["A0-B0", "no door"]),
                },
            ]);
        }

        // keep:critical — POE-470 AC-1. Silent consequence: a read that
        // misjudges its Cradle context ranks and shows every chest room at
        // the wrong unique's price (Mask of the Spirit Drinker's 195 c where
        // the temple drops Mask of the Stitched Demon's 427 c, or the
        // reverse), a plausible total the advice and the overlay both trust.
        // The context is a Cradle of Creation on the board or a panel kill
        // that builds on its line, at whichever tier the kill builds.
        #[test]
        fn a_read_values_its_chests_in_the_cradle_context_its_board_and_panel_name() {
            use crate::temple::market::allflame;
            use crate::temple::valuation::{DriverKind, Knobs, RoomOverrides, Valued};

            let unrelated = || {
                vec![
                    offer("Azcapa", Upgrade, "Catalyst of Corruption"),
                    offer("Ticaba", Change, "Storage Room"),
                ]
            };
            let standing_in = |tier: u8, extra: &[(Slot, &'static str, u8)]| {
                let mut rooms = vec![(D1, if tier == 0 { "" } else { "corruption" }, tier)];
                rooms.extend_from_slice(extra);
                board(&rooms, &[(D1, E1)], D1, 5)
            };
            let rows: Vec<(&str, BoardState, Vec<ArchitectOffer>, bool)> = vec![
                (
                    "Cradle of Creation on the board, unrelated offers",
                    standing_in(1, &[(C1, "throne_of_atziri", 3)]),
                    unrelated(),
                    true,
                ),
                (
                    "a change from filler builds Cradle of Creation",
                    standing_in(0, &[]),
                    vec![offer("Tzamoto", Change, "Cradle of Creation"), unrelated().remove(0)],
                    true,
                ),
                (
                    "a change from tier 1 builds Cradle of Creation",
                    standing_in(1, &[]),
                    vec![unrelated().remove(0), offer("Tzamoto", Change, "Cradle of Creation")],
                    true,
                ),
                (
                    "a change from tier 2 builds Cradle of Creation",
                    standing_in(2, &[]),
                    vec![offer("Tzamoto", Change, "Cradle of Creation"), unrelated().remove(0)],
                    true,
                ),
                (
                    "a change with the current room unread builds Cradle of Creation",
                    {
                        let mut state = standing_in(0, &[]);
                        state.current_tier = None;
                        state
                    },
                    vec![offer("Tzamoto", Change, "Cradle of Creation"), unrelated().remove(0)],
                    true,
                ),
                (
                    "Hall of Lords on the board is no Cradle, unrelated offers",
                    standing_in(1, &[(C1, "throne_of_atziri", 2)]),
                    unrelated(),
                    false,
                ),
                ("unrelated offers on a board with no Cradle", standing_in(1, &[]), unrelated(), false),
                ("no offer read on a board with no Cradle", standing_in(1, &[]), Vec::new(), false),
            ];

            let market = allflame();
            let mut failures = Vec::new();
            for (name, state, offers, want) in &rows {
                let cradle = cradle_in_play(state, offers);
                if cradle != *want {
                    failures.push(format!("{name}: cradle_in_play {cradle}, want {want}"));
                }
                let valued = Valued::compute_in(&market, &Knobs::default(), &RoomOverrides::new(), cradle);
                let sanctum = valued
                    .get("sanctum_of_immortality", Tier::new(3).expect("tier 3"))
                    .expect("sanctum is valued");
                let unique = sanctum
                    .drivers
                    .iter()
                    .find(|d| d.kind == DriverKind::UniqueDrop)
                    .expect("sanctum has a chest driver");
                let (want_name, want_chaos) = if *want {
                    ("Mask of the Stitched Demon", 106.75)
                } else {
                    ("Mask of the Spirit Drinker", 48.75)
                };
                if unique.name != want_name || unique.chaos != Some(want_chaos) {
                    failures.push(format!(
                        "{name}: sanctum counts {} at {:?}, want {want_name} at {want_chaos}",
                        unique.name, unique.chaos
                    ));
                }
            }
            assert!(failures.is_empty(), "\n{}", failures.join("\n"));
        }
    }

    mod keep_regression {
        use super::*;

        // keep:regression — POE-415 WI-6, Operator-reported
        // (evidence/temple-door-halls-vs-corruption.png). The shipped profile
        // sealed Corruption Chamber (C2-D2) on the Gemcutter's Workshop board
        // because the Locus/Doryani target set made the Workshop a stranded
        // target room. With the Cradle as the only target line nothing here
        // is worth connecting to the Entrance, and the door is Halls, beside
        // the Apex.
        #[test]
        fn the_gemcutters_workshop_board_opens_halls_beside_the_apex() {
            let case = cases::case_11_gemcutters_workshop_halls();
            let advice = advise_case_with(&case, &production_profile());
            let top = &advice.recommendations[0];
            assert_eq!(
                opened_toward(&top.option.doors, C2),
                vec![B1],
                "{}: {}\n{}",
                case.name,
                case.decision,
                ranking_dump(&advice)
            );
        }

        // keep:regression — POE-415 (c), Operator-reported 2026-11-24: on the
        // upgrade-vs-explosives board, under option 1 (the played option),
        // Hayoxi's upgrade builds the conduit line at tier 3 and is the top
        // kill on both the cold and the live production profile. The
        // explosives change builds tier 1 only, so the connected Shrine's +1
        // no longer makes it the equal kill RC assumed.
        #[test]
        fn the_upgrade_vs_explosives_board_takes_the_grand_reactor_upgrade() {
            let case = cases::case_10_lightning_workshop_upgrade();
            let config = played(Some(Consolidation::NonResidentExtraTier));
            let mut failures = Vec::new();
            for (label, profile) in [("cold", production_profile()), ("live", live_production_profile())] {
                let advice = advise(&case.state, &case.offers, case.keys, &profile, &config, N, SEED);
                let got = top_kill_of(&advice);
                if got != Some((Upgrade, "conduit_of_lightning".to_string(), 3)) {
                    failures.push(format!(
                        "{label}: top kill {got:?}, want the upgrade to the conduit line at tier 3; \
                         top reasons {:?}\n{}",
                        advice.recommendations.first().map(|r| &r.reasons),
                        ranking_dump(&advice)
                    ));
                }
            }
            assert!(failures.is_empty(), "{}: {}\n{}", case.name, case.decision, failures.join("\n"));
        }

        // keep:regression — POE-465 AC-1, the reported board. Standing in
        // Torment Cells (torture line, tier 1), Xipocado's box prints Torture
        // Cages and Tzamoto's prints Cradle of Creation. The surviving
        // architect acts, so killing Tzamoto (block 1) is what builds the
        // Cradle: that kill ranks first, carries the priority, and Xipocado's
        // kill carries none. Fails while the architect's name alone qualifies
        // block 0.
        #[test]
        fn the_torment_cells_board_puts_the_kill_that_builds_the_cradle_first() {
            let offers = vec![
                offer("Xipocado", Upgrade, "Torture Cages"),
                offer("Tzamoto", Change, "Cradle of Creation"),
            ];
            let advice = advise(
                &priority_board("sadists_den", 1),
                &offers,
                1,
                &rush(),
                &TempleConfig::default(),
                N,
                SEED,
            );
            let dump = ranking_dump(&advice);

            assert_eq!(
                advice
                    .recommendations
                    .first()
                    .and_then(|r| r.option.architect.as_ref().map(|a| a.offer_index)),
                Some(1),
                "the top recommendation must kill Tzamoto's block:\n{dump}"
            );
            assert_eq!(
                advice.priority,
                Some(AdvicePriority {
                    architect_index: 1,
                    reason: CRADLE_PRIORITY_REASON.to_string(),
                }),
                "\n{dump}"
            );
            let marked_xipocado: Vec<String> = advice
                .recommendations
                .iter()
                .filter(|r| r.option.architect.as_ref().map(|a| a.offer_index) == Some(0))
                .filter(|r| has_reason(r, |x| matches!(x, Reason::ArchitectPriority)))
                .map(|r| r.option.doors_label())
                .collect();
            assert!(
                marked_xipocado.is_empty(),
                "Xipocado's kill options carry ArchitectPriority: {marked_xipocado:?}\n{dump}"
            );
        }

        // keep:regression — AC-4. Boards without a Cradle-line kill keep
        // their recommendation order and publish no priority. Case 8 prints
        // Grand Reactor from another architect, so the printed target alone
        // must not qualify.
        #[test]
        fn a_board_without_a_qualifying_offer_keeps_its_ranking_and_no_priority() {
            let rows: [(cases::Case, Vec<(Option<usize>, &str)>); 2] = [
                (
                    cases::case_2_corruption_chamber(),
                    vec![
                        (Some(0), "A0-B1"),
                        (Some(0), "B0-B1"),
                        (Some(0), "B1-C2"),
                        (Some(0), "B1-C1"),
                        (Some(0), "no door"),
                        (Some(1), "A0-B1"),
                        (Some(1), "B0-B1"),
                        (Some(1), "B1-C2"),
                        (Some(1), "B1-C1"),
                        (Some(1), "no door"),
                    ],
                ),
                // Under option 1, the played default, the upgrade builds tier 3
                // and the change tier 1: RC is silent and R4 puts the upgrade
                // first (POE-415).
                (
                    cases::case_8_lightning_workshop(),
                    vec![
                        (Some(0), "B0-C1"),
                        (Some(0), "B1-C1"),
                        (Some(0), "no door"),
                        (Some(1), "B0-C1"),
                        (Some(1), "B1-C1"),
                        (Some(1), "no door"),
                    ],
                ),
            ];
            let mut failures = Vec::new();
            for (case, expected) in rows {
                let advice = advise_case(&case);
                let got = ranked_sequence(&advice);
                let want: Vec<(Option<usize>, String)> = expected
                    .iter()
                    .map(|(index, doors)| (*index, doors.to_string()))
                    .collect();
                if got != want || advice.priority.is_some() {
                    failures.push(format!(
                        "{}: priority {:?}\n  got  {got:?}\n  want {want:?}\n{}",
                        case.name,
                        advice.priority,
                        ranking_dump(&advice)
                    ));
                }
            }
            assert!(failures.is_empty(), "\n{}", failures.join("\n"));
        }

        // keep:regression — AC-4. An offer that cannot be ranked — its printed
        // target unknown — gets no priority, and the warnings say what they
        // said before. A Cradle-line change with no current tier is no longer
        // unrankable: the POE-415 Cradle rule builds tier 3 whatever the
        // current tier (`a_cradle_of_creation_offer_builds_tier_three_under_every_option`).
        #[test]
        fn an_unrankable_qualifying_offer_gets_no_priority() {
            let rival = || offer("Azcapa", Upgrade, "Catalyst of Corruption");
            let rows = [
                (
                    "Xipocado's printed target is not a room",
                    priority_board("corruption", 1),
                    vec![offer("Xipocado", Upgrade, "Definitely Not A Room"), rival()],
                    vec![Warning::UnresolvedOffer {
                        printed: "Definitely Not A Room".to_string(),
                    }],
                ),
            ];
            for (name, state, offers, warnings) in rows {
                let advice = advise(&state, &offers, 0, &rush(), &TempleConfig::default(), N, SEED);
                assert_eq!(advice.priority, None, "{name}");
                assert_eq!(advice.warnings, warnings, "{name}");
                assert!(!advice.recommendations.is_empty(), "{name}: the rival is still ranked");
                assert!(
                    advice
                        .recommendations
                        .iter()
                        .all(|r| r.option.architect.as_ref().map(|a| a.offer_index) == Some(1)),
                    "{name}: only the rankable offer may be recommended:\n{}",
                    ranking_dump(&advice)
                );
            }
        }
    }
}
