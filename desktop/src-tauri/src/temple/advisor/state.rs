//! The board the advisor reasons over, and the graph helpers every other
//! sub-module shares (POE-170).
//!
//! [`BoardState`] is the *readable* form: a `BTreeSet<Edge>` door set and a
//! per-slot `(line, tier)` table, built straight from what POE-168/169 read off
//! the screen. The rollout kernel converts it once into a bitmask form
//! ([`super::rollout`]) because it re-walks the graph tens of thousands of
//! times per decision; keeping the two apart is deliberate — this type is what
//! a human debugs, that one is what the CPU runs.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use crate::temple::lattice::{self, Edge, Slot};
use crate::temple::panel::PanelReading;
use crate::temple::reader::TempleLayout;
use crate::temple::rooms::RoomIdentity;
use crate::temple::strategy::{Line, Tier};

/// A set of slots, one bit per [`Slot::index`].
pub type SlotMask = u16;

/// Every slot, as a mask.
#[allow(dead_code)] // POE-171: only the tests reach this.
pub const ALL_SLOTS: SlotMask = (1 << 13) - 1;

/// The single-bit mask of one slot.
pub fn mask_of(slot: Slot) -> SlotMask {
    1 << slot.index()
}

/// The slot at a bit index. Only valid for `0..13`.
#[allow(dead_code)] // POE-171: only the tests reach this.
pub fn slot_at(index: usize) -> Slot {
    Slot::ALL[index]
}

/// Whether a mask holds a slot.
pub fn mask_holds(mask: SlotMask, index: usize) -> bool {
    mask & (1 << index) != 0
}

/// The set bits of a mask, ascending.
pub fn bits(mask: SlotMask) -> impl Iterator<Item = usize> {
    (0..13).filter(move |i| mask_holds(mask, *i))
}

/// Lattice adjacency as masks, indexed by [`Slot::index`].
///
/// Derived once from [`lattice::neighbours`] rather than typed in, so the
/// advisor's graph and the reader's geometry can never disagree.
pub fn neighbour_masks() -> &'static [SlotMask; 13] {
    static MASKS: OnceLock<[SlotMask; 13]> = OnceLock::new();
    MASKS.get_or_init(|| {
        let mut out = [0; 13];
        for &slot in &Slot::ALL {
            let mut mask = 0;
            for n in lattice::neighbours(slot) {
                mask |= mask_of(n);
            }
            out[slot.index()] = mask;
        }
        out
    })
}

/// How many corridors a slot could ever have — the quantity RS ranks on.
///
/// Raw *lattice* degree, not connected degree: RS is about how many future
/// chances exist to link a slot at all, which the door set cannot change.
pub fn lattice_degree(slot: Slot) -> usize {
    neighbour_masks()[slot.index()].count_ones() as usize
}

/// The connected component of `start` under an open-door adjacency table.
pub fn component(open: &[SlotMask; 13], start: Slot) -> SlotMask {
    let mut seen = mask_of(start);
    let mut frontier = seen;
    while frontier != 0 {
        let mut next = 0;
        for i in bits(frontier) {
            next |= open[i];
        }
        next &= !seen;
        seen |= next;
        frontier = next;
    }
    seen
}

/// BFS hop distance from `start` to every slot over open doors; `None` where
/// the slot is unreachable.
pub fn hop_distances(open: &[SlotMask; 13], start: Slot) -> [Option<usize>; 13] {
    let mut dist = [None; 13];
    dist[start.index()] = Some(0);
    let mut frontier = mask_of(start);
    let mut seen = frontier;
    let mut depth = 0;
    while frontier != 0 {
        depth += 1;
        let mut next = 0;
        for i in bits(frontier) {
            next |= open[i];
        }
        next &= !seen;
        for i in bits(next) {
            dist[i] = Some(depth);
        }
        seen |= next;
        frontier = next;
    }
    dist
}

/// The fewest **closed** doors that must be blasted to reach each slot from
/// the seed set, walking open doors for free (RE's `minClosedDoorsToReach`).
///
/// A 0-1 BFS over the full lattice, so every slot gets an answer — including
/// the Apex, which is tier 0 and is exactly the case an earlier prototype fix
/// got wrong by excluding tier-0 rooms from blasting.
///
/// **Deviation from the prototype, deliberate.** `temple_model.py`'s
/// `reachable_with_charges` expanded one arbitrary set-ordered slice of the
/// frontier and charged the whole budget for it, which is order-dependent and
/// spends charges it did not use. This computes the distance the spec actually
/// names (`minClosedDoorsToReach(target, EntranceComponent) <= charges`) and is
/// deterministic. It is *more* generous: everything within `charges` closed
/// doors counts, where the prototype capped the total number of slots opened.
pub fn closed_door_distances(open: &[SlotMask; 13], from: SlotMask) -> [usize; 13] {
    let neighbours = neighbour_masks();
    let mut dist = [UNREACHABLE; 13];
    let mut layer = free_closure(from, open);
    let mut assigned: SlotMask = 0;
    let mut cost = 0;
    while layer != 0 {
        for i in bits(layer) {
            dist[i] = cost;
        }
        assigned |= layer;
        let mut next = 0;
        for i in bits(layer) {
            next |= neighbours[i];
        }
        next &= !assigned;
        layer = free_closure(next, open) & !assigned;
        cost += 1;
    }
    dist
}

/// Sentinel for a slot no number of charges can reach — impossible on the
/// connected lattice, so it only ever appears for an empty seed set.
pub const UNREACHABLE: usize = usize::MAX;

/// Grow a seed set by every open door, to fixpoint.
fn free_closure(seed: SlotMask, open: &[SlotMask; 13]) -> SlotMask {
    let mut grown = seed;
    loop {
        let mut next = grown;
        for i in bits(grown) {
            next |= open[i];
        }
        if next == grown {
            return grown;
        }
        grown = next;
    }
}

/// What one slot holds.
///
/// Tier 0 covers three different things the board treats identically — the
/// Entrance, the Apex and a filler room — because none of them has a line an
/// architect can advance and none of them scores.
pub type Room = (Option<Line>, Tier);

/// The board, as the advisor sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardState {
    /// `(line, tier)` per slot, indexed by [`Slot::index`].
    pub rooms: [Room; 13],
    /// Corridors currently open.
    pub doors: BTreeSet<Edge>,
    /// The room the player is standing in. `None` when the panel was read
    /// between rooms — there is no decision to make then.
    pub position: Option<Slot>,
    /// `N Incursions Remaining` — the budget for the whole temple.
    pub remaining: u8,
    /// The slot the *previous* incursion of this map used.
    ///
    /// Only the immediately next incursion is guaranteed to differ (Sebastian,
    /// 2026-08-07: *"not all rooms — it's more that 'next'"*), so this is one
    /// slot and not a per-map visited set. The prototype's `visited_this_map`
    /// modelled the stronger, wrong claim.
    pub last_visited: Option<Slot>,
}

impl BoardState {
    /// An empty board: 13 tier-0 slots, no doors.
    #[allow(dead_code)] // POE-171: only the tests reach this.
    pub fn empty() -> BoardState {
        BoardState {
            rooms: std::array::from_fn(|_| (None, Tier::T0)),
            doors: BTreeSet::new(),
            position: None,
            remaining: 0,
            last_visited: None,
        }
    }

    /// Build the board from one screen read.
    ///
    /// - `identities` is the room each plate resolved to, indexed by
    ///   [`Slot::index`]; `None` is a plate whose name OCR could not place, and
    ///   is treated as tier-0 filler because that is the only assumption that
    ///   cannot invent value.
    /// - `settled` is [`crate::temple::markers::apply_markers`]'s door set when
    ///   the side panel's diamond was read. Without it the current room's
    ///   corridors are unknown, so `layout.doors − layout.uncertain` is used:
    ///   the reader's own contract is that `uncertain` edges may be wrong, and
    ///   an invented corridor drives a confident wrong recommendation.
    /// - `remaining` comes from the panel footer; an illegible footer reads 0,
    ///   which makes every rollout terminate immediately and is visible in the
    ///   result rather than silently optimistic.
    pub fn from_reading(
        layout: &TempleLayout,
        identities: &[Option<RoomIdentity>; 13],
        panel: &PanelReading,
        settled: Option<&BTreeSet<Edge>>,
    ) -> BoardState {
        let doors = match settled {
            Some(settled) => settled.clone(),
            None => layout
                .doors
                .difference(&layout.uncertain)
                .copied()
                .collect(),
        };
        let rooms = std::array::from_fn(|i| match identities[i] {
            Some(RoomIdentity::Room { line, tier }) => (Some(line.mechanical_line()), tier),
            _ => (None, Tier::T0),
        });
        BoardState {
            rooms,
            doors,
            position: layout.current,
            remaining: panel.incursions_remaining.unwrap_or(0),
            last_visited: None,
        }
    }

    /// The line built in `slot`, if any.
    pub fn line(&self, slot: Slot) -> Option<&Line> {
        self.rooms[slot.index()].0.as_ref()
    }

    /// The tier built in `slot`.
    pub fn tier(&self, slot: Slot) -> Tier {
        self.rooms[slot.index()].1
    }

    /// Put a room in a slot.
    pub fn set_room(&mut self, slot: Slot, line: Option<Line>, tier: Tier) {
        self.rooms[slot.index()] = (line, tier);
    }

    /// Open-door adjacency, one mask per slot.
    pub fn adjacency(&self) -> [SlotMask; 13] {
        let mut open = [0; 13];
        for edge in &self.doors {
            let (a, b) = edge.ends();
            open[a.index()] |= mask_of(b);
            open[b.index()] |= mask_of(a);
        }
        open
    }

    /// Everything the Entrance reaches through open doors.
    pub fn entrance_component(&self) -> SlotMask {
        component(&self.adjacency(), Slot::ENTRANCE)
    }

    /// The corridors out of `slot` that are still closed.
    pub fn closed_doors_from(&self, slot: Slot) -> Vec<Edge> {
        lattice::neighbours(slot)
            .into_iter()
            .map(|n| Edge::new(slot, n))
            .filter(|edge| !self.doors.contains(edge))
            .collect()
    }

    /// `(line, tier)` for every built room the Entrance can reach — the input
    /// [`crate::temple::strategy::StrategyProfile::select_mode`] takes.
    pub fn connected_rooms(&self) -> Vec<(Line, Tier)> {
        let reach = self.entrance_component();
        bits(reach)
            .filter_map(|i| {
                let (line, tier) = &self.rooms[i];
                line.clone().map(|line| (line, *tier))
            })
            .filter(|(_, tier)| *tier != Tier::T0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::doors::{Confidence, Thresholds};
    use crate::temple::lattice::{Corridor, Slot::*};
    use crate::temple::panel::PanelReading;
    use crate::temple::rooms::{self, RoomIdentity};
    use crate::temple::anchor::AnchorCalibration;

    fn open(edges: &[(Slot, Slot)]) -> [SlotMask; 13] {
        let mut out = [0; 13];
        for (a, b) in edges {
            out[a.index()] |= mask_of(*b);
            out[b.index()] |= mask_of(*a);
        }
        out
    }

    fn slots(mask: SlotMask) -> Vec<Slot> {
        bits(mask).map(slot_at).collect()
    }

    // The graph the whole advisor reasons over. Fails if a door is treated as
    // directed, or if the walk stops at depth one.
    #[test]
    fn a_component_follows_open_doors_transitively_and_stops_at_a_closed_one() {
        let open = open(&[(E1, D1), (D1, C0), (C0, B0)]);
        assert_eq!(slots(component(&open, E1)), vec![B0, C0, D1, E1]);
        assert_eq!(slots(component(&open, C2)), vec![C2], "C2 has no open door");
    }

    // RE's `minClosedDoorsToReach`. The Apex is tier 0 and must still be a legal
    // blast target — excluding tier-0 rooms is the prototype bug this replaces.
    #[test]
    fn the_closed_door_distance_reaches_the_apex_from_the_entrance_component() {
        // Entrance reaches B0 through open doors; A0 is one closed corridor on.
        let open = open(&[(E1, D1), (D1, C0), (C0, B0)]);
        let seed = component(&open, Slot::ENTRANCE);
        let distance = closed_door_distances(&open, seed);
        assert_eq!(distance[Slot::ENTRANCE.index()], 0);
        assert_eq!(distance[B0.index()], 0, "open doors are free");
        assert_eq!(distance[A0.index()], 1, "one closed corridor from B0");
        assert_eq!(distance[C1.index()], 1, "C1 touches C0 and D1");
    }

    // A slot behind two closed corridors must cost two, or one charge would
    // reach everything.
    #[test]
    fn the_closed_door_distance_charges_one_per_closed_corridor_crossed() {
        let open = open(&[(E1, E2)]);
        let distance = closed_door_distances(&open, component(&open, Slot::ENTRANCE));
        assert_eq!(distance[D2.index()], 1, "D2 touches E1");
        assert_eq!(distance[C1.index()], 2, "C1 is two corridors from row E");
        assert_eq!(distance[A0.index()], 4);
    }

    // An open corridor beyond the first blast must still be free, or a single
    // charge would stop at the cluster it opened into.
    #[test]
    fn an_open_corridor_past_a_blasted_one_costs_nothing_extra() {
        // B0 and C0 are joined to each other but to nothing else; the Entrance
        // is alone. Two closed corridors separate row E from C0, and B0 rides
        // in on C0's open door.
        let open = open(&[(B0, C0)]);
        let distance = closed_door_distances(&open, component(&open, Slot::ENTRANCE));
        assert_eq!(distance[D1.index()], 1);
        assert_eq!(distance[C0.index()], 2);
        assert_eq!(
            distance[B0.index()],
            2,
            "B0 rides in free on the corridor it shares with C0"
        );
    }

    // Hop distance is what `path_cost` charges; an unreachable slot has no
    // distance rather than a large one.
    #[test]
    fn hop_distance_is_none_for_a_slot_no_open_door_reaches() {
        let open = open(&[(E1, D1), (D1, C0)]);
        let distance = hop_distances(&open, Slot::ENTRANCE);
        assert_eq!(distance[D1.index()], Some(1));
        assert_eq!(distance[C0.index()], Some(2));
        assert_eq!(distance[A0.index()], None);
    }

    // RS reads raw lattice degree. Fails if it counted open doors instead.
    #[test]
    fn lattice_degree_is_the_slots_corridor_count_not_its_open_doors() {
        assert_eq!(lattice_degree(A0), 2, "the Apex has only B0 and B1");
        assert_eq!(lattice_degree(D3), 3);
        assert_eq!(lattice_degree(D2), 6);
    }

    fn layout(doors: &[(Slot, Slot)], uncertain: &[(Slot, Slot)]) -> TempleLayout {
        TempleLayout {
            origin: (673, 682),
            scale: 1.0,
            ncc: 0.99,
            confidence: Confidence::High,
            current: Some(D1),
            doors: doors.iter().map(|(a, b)| Edge::new(*a, *b)).collect(),
            uncertain: uncertain.iter().map(|(a, b)| Edge::new(*a, *b)).collect(),
            slots: [(0, 0); 13],
            thresholds: Thresholds {
                horizontal: 0.2,
                diagonal: 0.2,
            },
            calibration: AnchorCalibration {
                screen_w: 1374,
                screen_h: 773,
                scale: 1.0,
            },
        }
    }

    fn panel(remaining: Option<u8>) -> PanelReading {
        PanelReading {
            room: rooms::match_room_name("Cloister"),
            architects: Vec::new(),
            incursions_remaining: remaining,
        }
    }

    // Without the side panel's diamond the current room's corridors are unknown,
    // and the reader's contract is that they may be wrong. An invented corridor
    // drives a confident wrong recommendation, so it must be dropped.
    #[test]
    fn a_reading_without_markers_drops_the_corridors_the_reader_is_unsure_of() {
        let layout = layout(&[(D1, E1), (C0, D1), (E1, E2)], &[(C0, D1)]);
        let identities = [None; 13];
        let board = BoardState::from_reading(&layout, &identities, &panel(Some(7)), None);
        assert!(board.doors.contains(&Edge::new(D1, E1)));
        assert!(
            !board.doors.contains(&Edge::new(C0, D1)),
            "an uncertain corridor must not become a door"
        );
        assert_eq!(board.position, Some(D1));
        assert_eq!(board.remaining, 7);
    }

    // With the diamond read, the marker result is authoritative and replaces the
    // beam-sampled set outright.
    #[test]
    fn a_reading_with_markers_takes_the_settled_door_set_verbatim() {
        let layout = layout(&[(D1, E1), (E1, E2)], &[(C0, D1)]);
        let settled: BTreeSet<Edge> = [Edge::new(C0, D1), Edge::new(E1, E2)]
            .into_iter()
            .collect();
        let board =
            BoardState::from_reading(&layout, &[None; 13], &panel(Some(3)), Some(&settled));
        assert_eq!(board.doors, settled);
    }

    // Entrance, Apex and filler all score as tier 0 — none of them has a line an
    // architect can advance — while a real room keeps its line and tier.
    #[test]
    fn room_identities_map_to_lines_with_the_three_line_less_kinds_at_tier_zero() {
        let mut identities = [None; 13];
        identities[Slot::ENTRANCE.index()] = Some(RoomIdentity::Entrance);
        identities[Slot::APEX.index()] = Some(RoomIdentity::Apex);
        identities[D0.index()] = Some(RoomIdentity::Filler("Cellar"));
        identities[C2.index()] = rooms::resolve_name("Catalyst of Corruption");
        let board =
            BoardState::from_reading(&layout(&[], &[]), &identities, &panel(Some(5)), None);
        assert_eq!(board.tier(Slot::ENTRANCE), Tier::T0);
        assert_eq!(board.tier(Slot::APEX), Tier::T0);
        assert_eq!(board.line(D0), None);
        assert_eq!(board.line(C2), Some(&Line::Corruption));
        assert_eq!(board.tier(C2), Tier::T2);
    }

    // `select_mode` prices connected lines only, so the board must not hand it a
    // stranded room — nor a tier-0 slot, which has no line at all.
    #[test]
    fn connected_rooms_lists_built_lines_the_entrance_reaches_and_nothing_else() {
        let mut board = BoardState::empty();
        board.set_room(E2, Some(Line::Gem), Tier::T1);
        board.set_room(C2, Some(Line::Corruption), Tier::T3);
        board.set_room(D1, Some(Line::named("junk")), Tier::T0);
        board.doors = [Edge::new(E1, E2), Edge::new(D1, E1)].into_iter().collect();
        assert_eq!(board.connected_rooms(), vec![(Line::Gem, Tier::T1)]);
    }

    // Corridor families are lattice knowledge, not advisor knowledge — this only
    // guards the assumption the masks are built from.
    #[test]
    fn every_neighbour_mask_is_symmetric_with_the_lattice_edge_set() {
        let masks = neighbour_masks();
        for &slot in &Slot::ALL {
            for n in bits(masks[slot.index()]).map(slot_at) {
                assert!(
                    mask_holds(masks[n.index()], slot.index()),
                    "{slot:?} lists {n:?} but not the reverse"
                );
                assert!(Edge::new(slot, n).kind().is_some());
            }
        }
        assert_eq!(
            masks.iter().map(|m| m.count_ones()).sum::<u32>(),
            52,
            "26 corridors, counted from both ends"
        );
        assert_ne!(Corridor::Horizontal, Corridor::Diagonal);
    }
}
