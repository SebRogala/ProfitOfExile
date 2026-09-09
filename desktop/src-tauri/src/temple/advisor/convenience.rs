//! The door to open with a key the move has no use for: the one that shortens
//! the walk.
//!
//! Sebastian, 2026-09-05: *"if all rooms have the connections, app doesn't
//! suggest to open the doors anymore at all"*. Once every closed corridor out
//! of the room leads back into its own cluster, [`super::rules::door_sets`]
//! enumerates nothing — a door that changes no reachability is worth nothing
//! to the score — and the widget went silent. The key still drops, and a door
//! still shortens somebody's walk: his own on the farming run, or whoever the
//! temple is handed to. His ask is that the widget keeps a suggestion, drawn
//! FAINT, the same mark as the second stone's door: not the move, but what to
//! do with a key the move has no use for.
//!
//! # What it ranks, in his order
//!
//! Four walks, lexicographic — the first one a corridor shortens decides, and
//! the next is reached only on a tie:
//!
//! 1. **Entrance → Apex.** A straight line of open rooms up to the Apex is four
//!    hops, one per row; every hop over that is a detour.
//! 2. **Entrance → the wanted rooms.** Every built room of a target line —
//!    Locus and Doryani's today, and whatever Vertolka's grading adds, because
//!    the set is the profile's ([`Valuation::is_target`]) and not this file's.
//! 3. **The wanted rooms → the Apex.**
//! 4. **The longest open loop**, as the fallback: the closed corridor whose two
//!    rooms are farthest apart by open doors closes the longest walk-around on
//!    the board — *"let's make life easier to others (or potentially to
//!    ourselves)"*.
//!
//! Walks 1–3 are [`super::state::Walks`], the measure the chain's own RA
//! (*"faster arrival"*, [`super::rules::DoorKey::walk`]) ranks door sets by,
//! so the faint mark and the bright one agree on what "shorter" means. Walks
//! 2 and 3 are SUMS over the wanted rooms, so a corridor that brings two rooms
//! one hop closer outranks one that brings one room one hop closer, and a room
//! the Entrance cannot reach is left out of both sums. That is sound only
//! because every candidate here changes no reachability: the set of reachable
//! rooms is the same before and after, so the two sums are over the same
//! rooms.
//!
//! # What it is NOT
//!
//! Not a rule of the chain, and not ranked by the rollout. A corridor that
//! merges two clusters is [`super::rules::door_sets`]'s business and the
//! ranking's answer; this file refuses it ([`convenience_door`] reads only
//! corridors inside the position's own cluster), so the faint mark can never
//! contradict the bright one. RU is the one rule it does honour: a corridor
//! that would dilute a saturated upgrade room is not a convenience, it is the
//! move the chain just declined, and it is filtered out by the same predicate
//! ([`super::rules::ru_violation`]).

use std::collections::BTreeSet;

use crate::temple::lattice::{Edge, Slot};
use super::rollout::Valuation;
use super::rules::{self, ArchitectChoice};
use super::state::{component, hop_distances, mask_holds, BoardState, Walks};

/// The convenience door, and the walk it shortens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConvenienceDoor {
    /// The corridor to open.
    pub door: Edge,
    /// Which of the four walks decided it.
    pub why: Why,
}

/// The walk a convenience door shortens — the first of the four (see the
/// module header) the corridor improves, or the loop it closes when it
/// improves none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Entrance → Apex, in hops.
    ApexWalk { before: usize, after: usize },
    /// Entrance → every wanted room the Entrance reaches, summed.
    TargetWalk {
        rooms: usize,
        before: usize,
        after: usize,
    },
    /// Every wanted room → the Apex, summed.
    TargetsToApex {
        rooms: usize,
        before: usize,
        after: usize,
    },
    /// No walk shortened; the corridor closes the longest loop — its far end
    /// is `hops` away by open doors.
    Loop { far: Slot, hops: usize },
}

impl ConvenienceDoor {
    /// One line for the page, naming the door and the walk.
    pub fn describe(&self) -> String {
        match self.why {
            Why::ApexWalk { before, after } => format!(
                "convenience door {}: shortens the Entrance → Apex walk, {before} → {after} hops",
                self.door
            ),
            Why::TargetWalk {
                rooms,
                before,
                after,
            } => format!(
                "convenience door {}: shortens the Entrance → wanted rooms walk, {before} → \
                 {after} hops over {rooms} room{}",
                self.door,
                plural(rooms)
            ),
            Why::TargetsToApex {
                rooms,
                before,
                after,
            } => format!(
                "convenience door {}: shortens the wanted rooms → Apex walk, {before} → {after} \
                 hops over {rooms} room{}",
                self.door,
                plural(rooms)
            ),
            Why::Loop { far, hops } => format!(
                "convenience door {}: closes the longest open loop — {} is {hops} hops away by \
                 open doors",
                self.door,
                far.as_str()
            ),
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// The ranking key, greater is better — the module header's four walks, in
/// that order, read off the field order by the derived `Ord`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    apex_gain: usize,
    target_gain: usize,
    targets_apex_gain: usize,
    loop_hops: usize,
}

/// The corridor to open when the move opens nothing, or `None` when there is
/// no closed corridor inside the position's own cluster that RU allows.
///
/// `architect` is the kill the ranking recommends: the board is read AFTER it,
/// so a kill that builds a wanted room in this very slot counts it, and RU
/// sees the upgrade room the kill creates rather than the one it replaces —
/// the same reading [`super::rules::evaluate_rules`] takes.
///
/// # Ties
///
/// A tie on all four walks falls to the smaller [`Edge`], which is the far end
/// nearer the top of the board (slots are ordered A0 first). That is not a
/// fifth rule, only a deterministic answer — the widget must draw one seal.
pub fn convenience_door(
    board: &BoardState,
    position: Slot,
    architect: Option<&ArchitectChoice>,
    valuation: &Valuation,
) -> Option<ConvenienceDoor> {
    let after_kill = rules::applied(board, position, architect, &BTreeSet::new());
    let open = after_kill.adjacency();
    let mine = component(&open, position);
    let wanted = rules::wanted_rooms(&after_kill, valuation);
    let before = Walks::measure(&open, &wanted);
    let from_position = hop_distances(&open, position);

    let mut best: Option<(Key, ConvenienceDoor)> = None;
    for edge in after_kill.closed_doors_from(position) {
        let far = rules::far_end(edge, position);
        // A merge is the ranking's answer, never this file's.
        if !mask_holds(mine, far.index()) {
            continue;
        }
        let doors = BTreeSet::from([edge]);
        let with_door = rules::applied(board, position, architect, &doors);
        if rules::ru_violation(board, &with_door, &doors).is_some() {
            continue;
        }
        let after = Walks::measure(&with_door.adjacency(), &wanted);
        // Inside one cluster the far end always has a distance; `0` is only
        // ever the defensive arm.
        let loop_hops = from_position[far.index()].unwrap_or(0);
        let key = Key {
            apex_gain: match (before.apex, after.apex) {
                (Some(before), Some(after)) => before.saturating_sub(after),
                _ => 0,
            },
            target_gain: before.targets.saturating_sub(after.targets),
            targets_apex_gain: before.targets_apex.saturating_sub(after.targets_apex),
            loop_hops,
        };
        let why = if key.apex_gain > 0 {
            Why::ApexWalk {
                before: before.apex.unwrap_or(0),
                after: after.apex.unwrap_or(0),
            }
        } else if key.target_gain > 0 {
            Why::TargetWalk {
                rooms: wanted.len(),
                before: before.targets,
                after: after.targets,
            }
        } else if key.targets_apex_gain > 0 {
            Why::TargetsToApex {
                rooms: wanted.len(),
                before: before.targets_apex,
                after: after.targets_apex,
            }
        } else {
            Why::Loop {
                far,
                hops: loop_hops,
            }
        };
        let candidate = ConvenienceDoor { door: edge, why };
        // Strictly greater: `closed_doors_from` walks the edges in slot order,
        // so the first of a tie — the smaller edge — is the one kept.
        if best.is_none_or(|(held, _)| key > held) {
            best = Some((key, candidate));
        }
    }
    best.map(|(_, door)| door)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::lattice::Slot::*;
    use crate::temple::strategy::StrategyProfile;

    use super::super::fixtures::{board, JUNK};
    use super::super::rules::ArchitectChoice;
    use crate::temple::rooms::OfferKind;
    use crate::temple::strategy::{Line, Tier};

    fn rush() -> Valuation {
        Valuation::for_profile(&StrategyProfile::locus_doryani_rush())
    }

    fn edge(a: Slot, b: Slot) -> Edge {
        Edge::new(a, b)
    }

    /// `convenience_door` with no kill on the board.
    fn pick(state: &BoardState) -> Option<ConvenienceDoor> {
        convenience_door(state, state.position.expect("fixture has a position"), None, &rush())
    }

    // Walk 1. The Apex is reached the long way round — E1, D1, C1, C0, B0, A0 is
    // five hops — and the closed C1-B0 corridor straightens it to four. C1-D2
    // is the other closed corridor inside the cluster and shortens nothing on
    // the Apex walk, so it loses whatever loop it closes.
    #[test]
    fn a_corridor_that_straightens_the_entrance_to_apex_walk_wins() {
        let state = board(
            &[(C1, JUNK, 1)],
            &[(E1, D1), (D1, C1), (C1, C0), (C0, B0), (B0, A0), (D1, D2)],
            C1,
            5,
        );

        let got = pick(&state).expect("two corridors inside the cluster");

        assert_eq!(got.door, edge(B0, C1));
        assert_eq!(
            got.why,
            Why::ApexWalk {
                before: 5,
                after: 4
            }
        );
    }

    // Walk 2. The Apex walk is already straight, so the corridor that brings the
    // Locus closer to the Entrance decides: C2 holds it, reached today via E2,
    // D3, D2 (four hops from E1); C1-C2 makes it three. C1-D2 is the competitor
    // and leaves the Locus where it was.
    #[test]
    fn with_the_apex_walk_straight_the_wanted_rooms_walk_decides() {
        let state = board(
            &[(C1, JUNK, 1), (C2, "corruption", 3)],
            &[
                (E1, D1),
                (D1, C1),
                (C1, B0),
                (B0, A0),
                (E1, E2),
                (E2, D3),
                (D3, D2),
                (D2, C2),
            ],
            C1,
            5,
        );

        let got = pick(&state).expect("corridors inside the cluster");

        assert_eq!(got.door, edge(C1, C2));
        assert_eq!(
            got.why,
            Why::TargetWalk {
                rooms: 1,
                before: 4,
                after: 3
            }
        );
    }

    // Walk 3. Neither the Apex walk nor the Entrance → Locus walk can improve —
    // the Locus at D2 is one hop from E1 already — but its walk UP to the Apex
    // is D2, D1, C1, B0, A0 (four) and C1-D2 makes it three.
    #[test]
    fn with_both_entrance_walks_settled_the_wanted_rooms_to_apex_walk_decides() {
        let state = board(
            &[(C1, JUNK, 1), (D2, "corruption", 3)],
            &[(E1, D1), (D1, C1), (C1, B0), (B0, A0), (E1, D2), (D1, D2), (D1, C0)],
            C1,
            5,
        );

        let got = pick(&state).expect("corridors inside the cluster");

        assert_eq!(got.door, edge(C1, D2));
        assert_eq!(
            got.why,
            Why::TargetsToApex {
                rooms: 1,
                before: 4,
                after: 3
            }
        );
    }

    // Walk 4. Nothing to shorten — a straight Apex walk and no wanted room —
    // so the fallback picks the corridor whose far end is farthest away by
    // open doors: C2 is four hops from C1 (D1, D2, D3, C2), D2 is two.
    #[test]
    fn with_nothing_to_shorten_the_longest_loop_is_closed() {
        let state = board(
            &[(C1, JUNK, 1)],
            &[(E1, D1), (D1, C1), (C1, B0), (B0, A0), (D1, D2), (D2, D3), (D3, C2)],
            C1,
            5,
        );

        let got = pick(&state).expect("corridors inside the cluster");

        assert_eq!(got.door, edge(C1, C2));
        assert_eq!(got.why, Why::Loop { far: C2, hops: 4 });
    }

    // A corridor into a saturated upgrade room is the move RU just declined,
    // not a convenience: C2 is a Sanctum of Unity II with its two picks already
    // certain, so C1-C2 is filtered out and the other corridor is the answer
    // even though C2 is the farther end.
    #[test]
    fn a_corridor_ru_would_veto_is_never_the_convenience_door() {
        let state = board(
            &[(C1, JUNK, 1), (C2, "upgrade", 2)],
            &[(E1, D1), (D1, C1), (C1, B0), (B0, A0), (D1, D2), (D2, D3), (D3, C2), (D2, C2)],
            C1,
            5,
        );

        let got = pick(&state).expect("C1-D2 is still inside the cluster");

        assert_eq!(got.door, edge(C1, D2));
    }

    // A corridor into ANOTHER cluster is a merge — the ranking's answer, which
    // this file must never second-guess. With every in-cluster corridor open,
    // the answer is none, not the merge.
    #[test]
    fn a_merge_is_the_rankings_business_and_yields_no_convenience_door() {
        let state = board(
            &[(C1, JUNK, 1)],
            &[(E1, D1), (D1, C1), (C1, B0), (B0, A0), (C1, C0), (C1, C2), (C1, D2)],
            C1,
            5,
        );

        // C1-B1 is the one closed corridor from C1, and B1 is a singleton.
        assert_eq!(pick(&state), None);
    }

    // The board is read AFTER the kill. C1 is junk today and reached from E1
    // the long way round (E1, D2, D3, C2, B1, C1: five hops); with no wanted
    // room on the board the fallback closes the longest loop, which is C1-D1
    // (D1 is five hops away). A `change` that builds the Locus in this very
    // slot makes C1 a wanted room, and then C1-D2 — one hop off E1 — wins on
    // the Entrance → wanted rooms walk even though its loop is the shorter.
    #[test]
    fn the_kill_is_applied_before_the_walks_are_measured() {
        let state = board(
            &[(C1, JUNK, 1)],
            &[(E1, D2), (D2, D3), (D3, C2), (C2, B1), (B1, C1), (D1, D2)],
            C1,
            5,
        );
        let kill = ArchitectChoice {
            offer_index: 0,
            architect_name: "Tacati".to_string(),
            kind: OfferKind::Change,
            line: Line::Corruption,
            built_tier: Tier::T3,
            display_name: "Locus of Corruption",
        };

        let without = pick(&state).expect("corridors inside the cluster");
        let with = convenience_door(&state, C1, Some(&kill), &rush())
            .expect("corridors inside the cluster");

        assert_eq!(without.door, edge(C1, D1));
        assert_eq!(without.why, Why::Loop { far: D1, hops: 5 });
        assert_eq!(with.door, edge(C1, D2));
        assert_eq!(
            with.why,
            Why::TargetWalk {
                rooms: 1,
                before: 5,
                after: 2
            }
        );
    }

    // Every corridor open, or none inside the cluster: nothing to suggest.
    #[test]
    fn a_room_with_every_corridor_open_has_no_convenience_door() {
        let state = board(
            &[(C1, JUNK, 1)],
            &[(C1, B0), (C1, B1), (C1, C0), (C1, C2), (C1, D1), (C1, D2), (E1, D1)],
            C1,
            5,
        );

        assert_eq!(pick(&state), None);
    }

    #[test]
    fn the_description_names_the_door_and_the_walk() {
        let door = ConvenienceDoor {
            door: edge(B0, C1),
            why: Why::ApexWalk {
                before: 5,
                after: 4
            },
        };
        assert_eq!(
            door.describe(),
            "convenience door B0-C1: shortens the Entrance → Apex walk, 5 → 4 hops"
        );
        let door = ConvenienceDoor {
            door: edge(C1, C2),
            why: Why::Loop { far: C2, hops: 4 },
        };
        assert_eq!(
            door.describe(),
            "convenience door C1-C2: closes the longest open loop — C2 is 4 hops away by open doors"
        );
    }
}
