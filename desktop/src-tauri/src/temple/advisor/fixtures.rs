//! Hand-encoded boards shared by the advisor's tests.
//!
//! The six retrospective screenshots come from the prototype's `cases.py`,
//! which is itself the hand-encoding of the boards `TEMPLE-CORE-RULES.md` §5
//! walks through with Sebastian, together with the decision he actually made.
//! The live boards come from §6d/§6e.

#![cfg(test)]

use std::collections::BTreeSet;

use crate::temple::lattice::{Edge, Slot};
use crate::temple::panel::ArchitectOffer;
use crate::temple::rooms::{self, OfferKind};
use crate::temple::strategy::{Line, Tier};

use super::state::BoardState;

/// The key `cases.py` writes junk rooms under.
pub const JUNK: &str = "junk";

/// Build a board.
///
/// `rooms` is `(slot, line key, tier)`; an empty key is a tier-0 slot with no
/// line. `doors` is the open corridors.
pub fn board(
    rooms: &[(Slot, &str, u8)],
    doors: &[(Slot, Slot)],
    position: Slot,
    remaining: u8,
) -> BoardState {
    let mut state = BoardState::empty();
    for (slot, key, tier) in rooms {
        let tier = Tier::new(*tier).expect("fixture tier is 0..=3");
        let line = (!key.is_empty()).then(|| Line::named(key));
        state.set_room(*slot, line, tier);
    }
    state.doors = doors.iter().map(|(a, b)| Edge::new(*a, *b)).collect();
    state.position = Some(position);
    state.remaining = remaining;
    state
}

/// An architect block as the panel prints it.
pub fn offer(name: &str, kind: OfferKind, printed: &str) -> ArchitectOffer {
    ArchitectOffer {
        architect_name: name.to_string(),
        kind,
        printed_target: printed.to_string(),
        target: rooms::match_room_name(printed),
    }
}

/// Two junk `change` offers — the "both architects worthless, the kill is free"
/// case (R0).
pub fn junk_offers() -> Vec<ArchitectOffer> {
    vec![
        offer("Ticaba", OfferKind::Change, "Storage Room"),
        offer("Juatalotli", OfferKind::Change, "Sparring Room"),
    ]
}

/// A door set, for asserting on a recommendation.
pub fn doors(edges: &[(Slot, Slot)]) -> BTreeSet<Edge> {
    edges.iter().map(|(a, b)| Edge::new(*a, *b)).collect()
}
