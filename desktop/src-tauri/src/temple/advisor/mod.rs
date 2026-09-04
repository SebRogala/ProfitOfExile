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
//!   rollout as its horizon and nothing else, so no rule reads it. The spec
//!   frames R3 as the `n` in the opportunity probability R1/R2 approximate
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
pub mod rollout;
pub mod rules;
pub mod state;

use std::collections::BTreeSet;

use crate::temple::lattice::{Edge, Slot};
use crate::temple::panel::{self, ArchitectOffer};
use crate::temple::rooms::{self, OfferKind};
use crate::temple::strategy::{Mode, StrategyProfile, TempleConfig, Tier};

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
    /// Neither the side-panel title nor the current plate named the room the
    /// player is standing in, so a `change` offer cannot be resolved: its built
    /// tier is `currentTier + 1` and nothing on screen carried `currentTier`
    /// (POE-229).
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
    /// Chase or Scarab, from the profile's own selector. It sits **above** the
    /// per-decision ranking: it decides which *map* the budget is spent in,
    /// while the ranking decides which door.
    pub mode: Mode,
    /// Best first.
    pub recommendations: Vec<Ranked>,
    /// The RV-excluded options, best first.
    pub gambles: Vec<Gamble>,
    /// R5's verdict for the top recommendation.
    pub map_action: MapAction,
    /// What was unreadable or unspendable.
    pub warnings: Vec<Warning>,
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
            map_action: MapAction::Continue,
            warnings,
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

    let architects = resolve_architects(offers, board.current_tier, &mut warnings);
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

    let base = Sim::from_board(board, &valuation);
    let mut safe: Vec<Scored> = Vec::new();
    let mut risky: Vec<Scored> = Vec::new();

    for (architect, door_sets) in choices.iter().zip(&door_sets_per_choice) {
        for doors in door_sets {
            let verdict = rules::evaluate_rules(
                board,
                position,
                architect.as_ref(),
                doors,
                keys,
                profile,
                &valuation,
            );
            let rv = rules::rv_verdict(
                board,
                position,
                architect.as_ref().map(|a| &a.line),
                doors,
                &valuation,
                connectable,
            );
            let opening = opening_for(position, architect.as_ref(), doors, &valuation);
            // Common random numbers: every option is priced from the same seed.
            let mut rng = Rng::seeded(seed);
            let estimate = rollout::evaluate(
                &base,
                &opening,
                profile,
                &valuation,
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

    let recommendations = rank(safe);
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

    Advice {
        mode,
        recommendations,
        gambles,
        map_action,
        warnings,
    }
}

struct Scored {
    option: Decision,
    estimate: Estimate,
    door: DoorKey,
    architect_key: ArchKey,
    reasons: Vec<Reason>,
}

/// Rank the kill by EV, the door set by the rule chain.
///
/// See [`rules`]'s module header for the measurement behind the split. Two
/// stages, in order:
///
/// 1. **The kill.** Each architect is priced by its best door set. Architects
///    R4's carve-out vetoed sort below every un-vetoed one whatever the EV says
///    — that is the one place a rule outranks the rollout on this axis, and
///    live board 8 is why. The rest are ordered by EV, and those inside
///    [`rules::noise_margin`] of the leader are re-ordered by the architect
///    rules — R0's "both architects are worthless, the kill is free" is exactly
///    this band.
/// 2. **The door set**, within each architect, by [`DoorKey`] with EV breaking
///    its ties.
///
/// The stages do not cross: a brilliant door can never promote a kill the
/// rollout says is 3 points worse. Case 2 is why — *"change to Jeweller's
/// Workshop, open the Apex"* scores R1-apex, and letting the door axis lead
/// would recommend Sebastian's own historical loss over upgrading the
/// corruption line.
fn rank(scored: Vec<Scored>) -> Vec<Ranked> {
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
    let cutoff = leader.mean - rules::noise_margin(leader.stderr);
    let band = summaries
        .iter()
        .position(|(_, estimate, key)| key.vetoed() != vetoed_leader || estimate.mean < cutoff)
        .unwrap_or(summaries.len());
    summaries[..band].sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.1.mean.total_cmp(&a.1.mean)));

    // -- stage 2: the door set --------------------------------------------
    let mut out = Vec::new();
    for (rank, (index, _, _)) in summaries.iter().enumerate() {
        let mut bucket = std::mem::take(&mut groups[*index].1);
        bucket.sort_by(|a, b| {
            b.door
                .cmp(&a.door)
                .then_with(|| b.estimate.mean.total_cmp(&a.estimate.mean))
        });
        // Outside the band the EV alone put this kill where it is; at rank 0
        // with a band of one, the EV alone put the *leader* where it is, which
        // is just as much a fact about the ranking and just as auditable. A
        // band that covers every architect is the opposite claim — the rollout
        // had no opinion — so `ExpectedValue` must stay off it.
        let ev_decided = rank >= band || (band == 1 && summaries.len() > 1);
        out.extend(bucket.into_iter().map(|option| {
            let mut reasons = option.reasons;
            if ev_decided {
                reasons.insert(0, Reason::ExpectedValue);
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

/// Resolve both architect blocks through the room vocabulary.
///
/// The panel's own wording is **not** the answer: with Contested Development
/// taken, *"Kill to change to Shrine of Empowerment"* on a tier-1 room builds
/// Sanctum of Unity II. Every offer goes through
/// [`rooms::resolve_offer_for`], which is where that arithmetic lives.
///
/// `current_tier` is [`BoardState::current_tier`] — `None` when the read never
/// named the room the player is standing in. An `upgrade` still resolves there
/// (its printed name IS the built room); a `change` does not, and says so
/// rather than resolving to the tier-1 room the kill will not build (POE-229).
fn resolve_architects(
    offers: &[ArchitectOffer],
    current_tier: Option<Tier>,
    warnings: &mut Vec<Warning>,
) -> Vec<ArchitectChoice> {
    let mut out = Vec::new();
    for (index, offer) in offers.iter().enumerate() {
        match rooms::resolve_offer_for(&offer.printed_target, offer.kind, current_tier) {
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
        kill: architect.map(|a| (valuation.tag(&a.line), a.kind == OfferKind::Upgrade)),
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
///   copy re-rolls them, and the scarab needs every entrance completed anyway;
/// - [`TempleConfig::r5_applies`]: the Incursion Scarab of Timelines takes the
///   choice away;
/// - the recommended kill advanced a still-missing target line to **below tier
///   3**. At tier 3 the room has left the drop pool on its own and R5 has
///   nothing left to protect.
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
    // Chase mode already told us at least one target line is missing; leaving
    // only makes sense while this one is what the map still owes.
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
    use crate::temple::strategy::{Line, ModeRule, StrategyProfile};

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
        StrategyProfile::locus_doryani_rush()
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

    // RV. The Apex door is the move that cost Sebastian the temple; the room is
    // valuable and unconnected, so only doors that shorten the distance to the
    // Entrance component are recommended.
    #[test]
    fn case_two_corruption_chamber_connects_down_rather_than_taking_the_apex() {
        let case = cases::case_2_corruption_chamber();
        let advice = advise_case(&case);
        let top = &advice.recommendations[0];
        let architect = top.option.architect.as_ref().expect("resolved");
        assert_eq!(architect.kind, Upgrade, "{}", case.decision);
        assert_eq!(architect.line, Line::Corruption);
        let doors = top_doors(&advice, B1);
        assert!(
            doors == vec![C1] || doors == vec![C2],
            "{}: expected a downward connection, got {doors:?}",
            case.decision
        );
        assert!(
            advice
                .recommendations
                .iter()
                .all(|r| opened_toward(&r.option.doors, B1) != vec![A0]
                    || r.option.architect.as_ref().is_some_and(|a| a.line != Line::Corruption)),
            "no corruption-line option may recommend the Apex door"
        );
    }

    // The excluded play is shown, not hidden, with the model's own risk number.
    #[test]
    fn case_two_surfaces_the_apex_play_as_a_gamble_carrying_its_measured_risk() {
        let case = cases::case_2_corruption_chamber();
        let advice = advise_case(&case);
        let apex = advice
            .gambles
            .iter()
            .find(|g| opened_toward(&g.option.doors, B1) == vec![A0])
            .expect("the Apex play must be offered as a gamble");
        // The number the user is shown, not a band wide enough to accept any
        // number: Sebastian's live guess was "50/50" and his considered figure
        // *"really just 8% risk"*, so a model that drifted to 30% would be
        // telling him something he has already rejected. Measured at N = 400
        // rollouts on SEED — both are fixed in this module, and this assertion
        // moves if either does.
        assert!(
            (0.03..=0.13).contains(&apex.risk),
            "the recorded risk of losing the room is 8%, got {}",
            apex.risk
        );
        assert!(has_reason(
            &Ranked {
                option: apex.option.clone(),
                ev: apex.ev,
                reasons: apex.reasons.clone()
            },
            |r| matches!(r, Reason::RvGamble { .. })
        ));
    }

    // The inverse of case 2 on the same door: Locus is banked and connected, so
    // nothing outranks the Apex any more.
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

    // Both lines are built and connected, so the itemised copy's tier re-roll
    // makes tiers stop mattering.
    #[test]
    fn case_three_switches_to_scarab_mode_with_both_lines_connected() {
        let advice = advise_case(&cases::case_3_chasm_late());
        assert_eq!(advice.mode, Mode::Scarab);
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
    #[test]
    fn case_five_poison_garden_builds_the_sanctum_and_opens_nothing() {
        let case = cases::case_5_poison_garden();
        let advice = advise_case(&case);
        let top = &advice.recommendations[0];
        assert!(top.option.doors.is_empty(), "{}", case.decision);
        let architect = top.option.architect.as_ref().expect("resolved");
        assert_eq!(architect.kind, Change);
        assert_eq!(
            architect.display_name, "Sanctum of Unity",
            "Contested Development turns the printed Shrine of Empowerment into \
             its tier-2 room"
        );
        assert!(
            has_reason(top, |r| matches!(r, Reason::ExpectedValue)),
            "the rollout separated the two kills by 3+ points here, and the \
             leader must say so rather than look rule-decided: {:?}",
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

    // ============================================ RV constrains the passage

    /// A stranded Corruption Chamber at B1, and the two kills Case 2 offers.
    ///
    /// `doors` decides whether anything on the board can still connect it.
    fn stranded_corruption(doors: &[(Slot, Slot)]) -> (BoardState, Vec<ArchitectOffer>) {
        let state = board(
            &[
                (B1, "corruption", 1),
                (B0, JUNK, 1),
                (C1, JUNK, 1),
                (C2, JUNK, 1),
            ],
            doors,
            B1,
            7,
        );
        let offers = vec![
            offer("Azcapa", Upgrade, "Catalyst of Corruption"),
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
    // passage, so the corruption upgrade Sebastian calls a no-brainer must not
    // be filed as a gamble while the junk change stays safe — that ranking
    // recommends changing the corruption room to junk.
    #[test]
    fn a_zero_key_incursion_never_turns_the_valuable_kill_into_a_gamble() {
        let (state, offers) = stranded_corruption(&[(C1, D1), (D1, E1)]);
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
        let (state, offers) = stranded_corruption(&[(B1, C1), (C1, C2), (B0, C1), (A0, B0)]);
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
        let (state, offers) = stranded_corruption(&[(C1, D1), (D1, E1)]);
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
        // to a lone Chasm at B0, and to a two-room cluster holding a Locus.
        let state = board(
            &[(C2, "corruption", 3), (D3, JUNK, 1)],
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
        // one tier off Doryani's Institute — so its single pick is currently a
        // certainty. Standing in D1, two corridors merge that same cluster in:
        // D1-C0 leaves the Shrine alone, D1-C1 dilutes it.
        let state = board(
            &[(C1, "upgrade", 1), (C0, "gem", 2)],
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

    // RC. Live board 5: beside a connected Shrine, spend the architect on the
    // line and let the shrine supply the tier — an upgrade wastes its pick half
    // the time.
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
            &TempleConfig::default(),
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
    #[test]
    fn the_upgrade_line_is_taken_speculatively_while_the_board_holds_no_target() {
        let state = board(&[(D1, JUNK, 1)], &[(D1, E1), (E1, E2)], D1, 9);
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
            &[(D1, JUNK, 1), (C2, "corruption", 1)],
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
            "RT must not fire while a corruption room is on the board"
        );
    }

    // R4. Maxing a junk room removes it from the drop pool, concentrating
    // future drops on the rooms that still matter.
    #[test]
    fn a_junk_room_is_maxed_to_tier_three_to_shrink_the_drop_pool() {
        // Both kills land at tier 2 deterministically; only the upgrade rolls
        // the 50% double tier, so only the upgrade can take the room out of the
        // pool this incursion.
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
    /// The kills are *"upgrade → Engineering Department"* (which rolls the
    /// double tier that would empty the slot) against *"change → Storage
    /// Room"* (which keeps it in the pool).
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
    // double-tier roll for R4's own reason (a smaller pool) and separates the
    // two kills past the noise band, so a tie-break would never fire.
    #[test]
    fn maxing_a_sanctums_only_live_target_is_vetoed_however_the_rollout_prices_it() {
        let (state, offers) = board_eight();
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

    // Live board 10: the corruption line was just taken below tier 3, so the
    // next incursion of this map is a guaranteed zero against it.
    #[test]
    fn r5_leaves_the_map_after_the_chased_line_is_used_below_tier_three() {
        let state = board(&[(D1, JUNK, 1)], &[(D1, E1), (E1, E2)], D1, 8);
        let offers = vec![offer("Paquate", Change, "Corruption Chamber")];
        let advice = advise(
            &state,
            &offers,
            0,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(advice.mode, Mode::Chase);
        assert_eq!(advice.map_action, MapAction::LeaveMap);
    }

    // The scarab requires completing every entrance, so it takes R5 away. Same
    // board, one config flag.
    #[test]
    fn the_timelines_scarab_keeps_the_player_in_the_map_r5_would_leave() {
        let state = board(&[(D1, JUNK, 1)], &[(D1, E1), (E1, E2)], D1, 8);
        let offers = vec![offer("Paquate", Change, "Corruption Chamber")];
        let config = TempleConfig {
            artefacts_of_the_vaal: true,
            scarab_of_timelines: true,
        };
        let advice = advise(&state, &offers, 0, &rush(), &config, N, SEED);
        assert_eq!(advice.map_action, MapAction::Continue);
    }

    // R5 stops once the room is at tier 3 — it has left the drop pool anyway.
    #[test]
    fn r5_does_not_fire_when_the_kill_takes_the_line_to_tier_three() {
        let state = board(&[(D1, "corruption", 2)], &[(D1, E1), (E1, E2)], D1, 8);
        let offers = vec![offer("Azcapa", Upgrade, "Locus of Corruption")];
        let advice = advise(
            &state,
            &offers,
            0,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(advice.map_action, MapAction::Continue);
    }

    // The mode switch: one line connected is Chase, both is Scarab, and the
    // second board must not still be telling the player to leave.
    #[test]
    fn one_connected_target_line_is_chase_and_both_is_scarab() {
        let offers = vec![offer("Paquate", Change, "Corruption Chamber")];
        let chasing = board(
            &[(D1, JUNK, 1), (E0, "gem", 1)],
            &[(D1, E1), (E0, E1)],
            D1,
            8,
        );
        let both = board(
            &[(D1, JUNK, 1), (E0, "gem", 1), (E2, "corruption", 1)],
            &[(D1, E1), (E0, E1), (E1, E2)],
            D1,
            8,
        );
        let chase = advise(
            &chasing,
            &offers,
            0,
            &rush(),
            &TempleConfig::default(),
            N,
            SEED,
        );
        let scarab = advise(&both, &offers, 0, &rush(), &TempleConfig::default(), N, SEED);
        assert_eq!(chase.mode, Mode::Chase);
        assert_eq!(chase.map_action, MapAction::LeaveMap);
        assert_eq!(scarab.mode, Mode::Scarab);
        assert_eq!(
            scarab.map_action,
            MapAction::Continue,
            "in Scarab mode every entrance is completed"
        );
    }

    // A built-but-unconnected target line is treated as absent by v1.
    #[test]
    fn a_stranded_target_line_does_not_switch_the_mode() {
        let state = board(
            &[(E0, "gem", 1), (C2, "corruption", 1)],
            &[(E0, E1), (E1, E2)],
            D1,
            8,
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
        assert_eq!(advice.mode, Mode::Chase);
    }

    // The mode rule is profile data, so a third required line changes it.
    #[test]
    fn the_mode_rule_is_profile_data_not_a_hard_coded_pair() {
        let state = board(
            &[(E0, "gem", 1), (E2, "corruption", 1)],
            &[(E0, E1), (E1, E2)],
            D1,
            8,
        );
        let mut profile = rush();
        profile.mode_rule = ModeRule::LinesConnected(vec![
            Line::Corruption,
            Line::Gem,
            Line::named("factory"),
        ]);
        let advice = advise(
            &state,
            &junk_offers(),
            0,
            &profile,
            &TempleConfig::default(),
            N,
            SEED,
        );
        assert_eq!(advice.mode, Mode::Chase);
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
    // The `change` wins because Contested Development prices it off the room
    // the player is standing IN: a tier-1 Armourer's Workshop taking
    // "Shrine of Empowerment" builds Sanctum of Unity II, against an `upgrade`
    // to a tier-2 Armoury on the line already under the player.
    //
    // The DOOR is deliberately not asserted. The rule chain picks it, no one
    // walked this board, and pinning a corridor nobody chose would make this a
    // change-detector for the chain rather than a check on the kill.
    #[test]
    fn the_pc_board_kills_the_change_architect_once_both_blocks_are_read() {
        let case = cases::case_7_armourers_workshop();

        let advice = advise_case(&case);

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

        let advice = advise(
            &case.state,
            &case.offers,
            case.keys,
            &rush(),
            &TempleConfig::default(),
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
        let expected: [&[&str]; 7] = [
            // 1 Tombs — a free kill, and a door decided by the scarcity chain.
            // `Ru` is on the corridor into E2's Sanctum, not on the pick.
            &["Rd", "R2", "R1Gradient", "Rs", "Ru"],
            // 2 Corruption Chamber — RV on both sides of the line, and the
            // rollout separating the two kills.
            &[
                "ExpectedValue",
                "Rv",
                "RvMerge",
                "Rd",
                "R2",
                "R1Gradient",
                "Rs",
                "AdvancesTarget",
                "R1Apex",
            ],
            // 3 Chasm late — the Apex, free at last.
            &["R1Apex", "Rd", "R2", "R1Gradient", "Rs"],
            // 4 Chasm merge — one door changes anything, and it is RV that
            // makes it worth a key.
            &["RvMerge", "Rd", "R2", "R1Gradient", "Rs"],
            // 5 Poison Garden — the Sanctum kill makes every corridor
            // degree-priced, so the doors ARE enumerated and each is declined
            // by RU (the top pick still opens nothing); the Cultivar kill
            // prices none of them, so its side still reads NoUsableDoor. The
            // rollout separates the kills by 3+ points. `ZeroKey` here would
            // be a lie: a key did drop.
            //
            // `R1Apex` was in this list until POE-248 and its absence is the
            // FIX, not a re-recording: the corridor that scored it is C0-B0,
            // and on this board B0 is already inside C0's own component (via
            // B0-C1 and C0-C1) while that component holds no Apex. So the door
            // buys neither the Apex nor a new way toward it, and `apex_reach`
            // is silent — see its note. The recorded decision is untouched: the
            // top pick still opens nothing, for RU's reason.
            &[
                "ExpectedValue",
                "NoUsableDoor",
                "R1Gradient",
                "R2",
                "R4",
                "Rd",
                "Rs",
                "Ru",
                "RuDeclined",
            ],
            // 6 Cloister — the blind board, decided by R1 and R2 alone.
            &["Rd", "R2", "R1Gradient", "Rs"],
            // 8 Lightning Workshop — R1-apex fires on both corridors (reaching
            // on B0, adjacent on B1) and the rest of the door chain ranks them.
            // On the kill: RC, because C2's Sanctum is adjacent and connected
            // and can supply the tier; R4, because the `upgrade` maxes C1 out
            // of the drop pool. R4's carve-out does NOT veto that — C2 already
            // holds three open corridors against two picks, so C1 is not a live
            // target it could lose. No `ExpectedValue`: neither kill is vetoed
            // and the rollout cannot separate them, so the band covers both.
            &["R1Apex", "Rd", "R2", "R1Gradient", "Rs", "Rc", "R4"],
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
}
