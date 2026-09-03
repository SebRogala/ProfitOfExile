//! The retrospective and live boards, and what Sebastian actually did.
//!
//! Six boards come from `TEMPLE-CORE-RULES.md` §5 by way of the prototype's
//! `cases.py`; the live ones from §6d/§6e. Each fixture records the *decision*,
//! not the model's number, so a change that makes the advisor disagree with the
//! player fails here.

#![cfg(test)]

use crate::temple::lattice::Slot::{self, *};
use crate::temple::panel::ArchitectOffer;
use crate::temple::rooms::OfferKind::{Change, Upgrade};

use super::fixtures::{board, offer, JUNK};
use super::state::BoardState;

/// One walked board.
pub struct Case {
    /// `TEMPLE-CORE-RULES.md`'s own label, so a failure names the screenshot.
    pub name: &'static str,
    /// The board, with the player standing in it.
    pub state: BoardState,
    /// Both architect blocks as the panel printed them.
    pub offers: Vec<ArchitectOffer>,
    /// How many opening stones dropped.
    pub keys: u8,
    /// What Sebastian did, in prose, for the failure message.
    pub decision: &'static str,
}

// --- Case 1 — 2026-08-02_22-22-38, 9 left, in Tombs (D3) --------------------
// Fully isolated; both architects worthless. He opened toward Chamber of Iron
// (C2): the only neighbour *above* Tombs, and a merge of two singletons.
pub fn case_1_tombs() -> Case {
    Case {
        name: "1 Tombs",
        state: board(
            &[
                (B0, JUNK, 1),
                (B1, JUNK, 1),
                (C1, "explosive", 1),
                (C2, JUNK, 3),
                (D0, JUNK, 1),
                (E0, "gem", 1),
                (E2, "upgrade", 2),
            ],
            &[
                (A0, B0),
                (B0, B1),
                (B1, C1),
                (C0, C1),
                (C0, D0),
                (C0, D1),
                (D1, E1),
                (D2, E2),
                (E1, E2),
            ],
            D3,
            9,
        ),
        offers: vec![
            offer("Ticaba", Change, "Storage Room"),
            offer("Juatalotli", Change, "Sparring Room"),
        ],
        keys: 1,
        decision: "kill either; open D3-C2 (up, and merges two singletons)",
    }
}

// --- Case 2 — 2026-08-03_22-43-37, 7 left, in Corruption Chamber I (B1) -----
// The trap. He opened the Apex and never got back to connect the room. RV.
pub fn case_2_corruption_chamber() -> Case {
    Case {
        name: "2 CorruptionChamber",
        state: board(
            &[
                (B1, "corruption", 1),
                (C0, JUNK, 1),
                (C1, JUNK, 1),
                (D0, JUNK, 3),
                (D1, JUNK, 3),
                (D2, "explosive", 2),
                (D3, JUNK, 2),
                (E0, JUNK, 1),
                (E2, "gem", 1),
            ],
            &[
                (C0, C1),
                (C0, D0),
                (C0, D1),
                (C1, D2),
                (C2, D3),
                (D0, E0),
                (D1, E1),
                (D2, D3),
                (D3, E2),
                (E1, E2),
            ],
            B1,
            7,
        ),
        offers: vec![
            offer("Azcapa", Upgrade, "Catalyst of Corruption"),
            offer("Paquate", Change, "Jeweller's Workshop"),
        ],
        keys: 1,
        decision: "upgrade the corruption line; connect DOWN (B1-C1 or B1-C2)",
    }
}

// --- Case 3 — 2026-08-03_22-54-58, 2 left, in Chasm (B0) -------------------
// The inverse of case 2 on the same door: Locus is banked and connected, so
// nothing outranks the Apex any more.
pub fn case_3_chasm_late() -> Case {
    Case {
        name: "3 Chasm-late",
        state: board(
            &[
                (B1, "corruption", 3),
                (C0, JUNK, 1),
                (C1, JUNK, 2),
                (C2, JUNK, 1),
                (D0, JUNK, 3),
                (D1, JUNK, 3),
                (D2, "explosive", 2),
                (D3, JUNK, 3),
                (E0, JUNK, 3),
                (E2, "gem", 1),
            ],
            &[
                (B0, C1),
                (B1, C1),
                (C0, C1),
                (C0, D0),
                (C0, D1),
                (C1, D2),
                (C2, D3),
                (D0, E0),
                (D1, E1),
                (D2, D3),
                (D3, E2),
                (E1, E2),
            ],
            B0,
            2,
        ),
        offers: vec![
            offer("Xopec", Change, "Royal Meeting Room"),
            offer("Xipocado", Change, "Lightning Workshop"),
        ],
        keys: 1,
        decision: "kill either; open B0-A0 (the Apex is finally free value)",
    }
}

// --- Case 4 — 2026-08-02_16-41-11, 5 left, in Chasm (B1) ------------------
// "The only viable pick": the other two doors join rooms already in his own
// component, so they change nothing at all.
pub fn case_4_chasm_merge() -> Case {
    Case {
        name: "4 Chasm-merge",
        state: board(
            &[
                (B0, JUNK, 3),
                (C0, JUNK, 1),
                (C1, JUNK, 3),
                (D0, JUNK, 1),
                (D1, JUNK, 3),
                (D2, "corruption", 2),
                (D3, JUNK, 3),
                (E0, "gem", 1),
                (E2, JUNK, 2),
            ],
            &[
                (A0, B0),
                (A0, B1),
                (B0, C1),
                (C0, D0),
                (C1, D1),
                (C2, D2),
                (C2, D3),
                (D1, E0),
                (D2, E1),
                (D2, D3),
                (D2, E2),
            ],
            B1,
            5,
        ),
        offers: vec![
            offer("Uromoti", Change, "Hatchery"),
            offer("Citaqualotl", Change, "Surveyor's Study"),
        ],
        keys: 1,
        decision: "kill either; open B1-C2 (the only door that changes anything)",
    }
}

// --- Case 5 — 2026-08-03_11-42-02, 4 left, in Poison Garden I (C0) --------
// Connectivity as a liability. Change yields Sanctum of Unity II, which
// upgrades TWO random connected neighbours — and C0 has exactly two. Opening a
// third door would turn a certainty into 2-of-3.
pub fn case_5_poison_garden() -> Case {
    Case {
        name: "5 PoisonGarden",
        state: board(
            &[
                (B1, JUNK, 1),
                (C0, "toxic_grove", 1),
                (C1, "gem", 2),
                (C2, JUNK, 1),
                (D0, JUNK, 2),
                (D1, "explosive", 2),
                (D2, JUNK, 3),
                (E0, JUNK, 2),
            ],
            &[
                (C0, C1),
                (C0, D0),
                (B0, C1),
                (B1, C1),
                (C1, D2),
                (C2, D2),
                (D0, E0),
                (D1, D2),
                (D1, E1),
                (D2, D3),
                (D2, E2),
            ],
            C0,
            4,
        ),
        offers: vec![
            offer("Quipolatl", Upgrade, "Cultivar Chamber"),
            offer("Tacati", Change, "Shrine of Empowerment"),
        ],
        keys: 1,
        decision: "change to Sanctum of Unity II; open NO door (RU)",
    }
}

// --- Case 6 — 2026-08-03_11-58-28, 11 left, in Cloister (D1) — BLIND ------
// Not walked before the model answered. Sebastian confirmed Cellar (C1), and
// his reason was R1 + R2: one row up, and a singleton.
pub fn case_6_cloister() -> Case {
    Case {
        name: "6 Cloister-blind",
        state: board(
            &[(B0, JUNK, 1), (B1, "corruption", 1), (D0, JUNK, 2), (D2, JUNK, 1)],
            &[(C0, D1), (D0, D1), (D2, E1), (D2, E2), (E1, E2)],
            D1,
            11,
        ),
        offers: vec![
            offer("Ticaba", Change, "Sparring Room"),
            offer("Juatalotli", Change, "Storage Room"),
        ],
        keys: 1,
        decision: "kill either; open D1-C1 (Cellar: one row up AND a singleton)",
    }
}

// --- Case 7 — 2026-09-03_13-56-40, 6 left, in Armourer's Workshop I (C2) ---
// The board POE-243 was opened on, and the reason it is here is what the app
// SHOWED on it: `upgrade → Armoury · B1-C2`, which is the ranking with
// Atmohua's block missing from the parsed panel. With both blocks read the kill
// is not close — the `change` builds Sanctum of Unity II off a tier-1 room
// (Contested Development), against an `upgrade` to a tier-2 Armoury on the line
// the player is already standing on.
//
// This is a REGRESSION FIXTURE for the ranking, not a walked decision:
// Sebastian did not play the board, the advisor did, and wrongly. The board is
// hand-encoded from the screenshot; the door the chain picks is deliberately
// not part of the claim.
pub fn case_7_armourers_workshop() -> Case {
    Case {
        name: "7 ArmourersWorkshop-PC",
        state: board(
            &[
                (B0, "house_of_the_others", 1),
                (B1, "wealth_of_the_vaal", 1),
                (C1, "conduit_of_lightning", 1),
                // The room the player is standing in — tier 1, which is what
                // makes the `change` land on tier 2.
                (C2, "chamber_of_iron", 1),
                (D0, "hall_of_champions", 1),
                (D1, "apex_of_ascension", 1),
                (D2, "storm_of_corruption", 1),
                (D3, "gem", 2),
                (E0, "court_of_sealed_death", 3),
                (E2, "hybridisation_chamber", 3),
            ],
            &[
                (C0, D1),
                (D1, D2),
                (D2, D3),
                (D0, E0),
                (D1, E0),
                (D1, E1),
                (D2, E2),
            ],
            C2,
            6,
        ),
        offers: vec![
            offer("Quipolatl", Upgrade, "Armoury"),
            offer("Atmohua", Change, "Shrine of Empowerment"),
        ],
        keys: 1,
        decision: "change → Sanctum of Unity (the overlay said upgrade → Armoury)",
    }
}

/// The six WALKED boards, in order.
///
/// [`case_7_armourers_workshop`] is deliberately not among them: it records
/// what the app got wrong rather than what Sebastian decided, and the suites
/// that iterate this list assert against his play.
pub fn retrospective() -> Vec<Case> {
    vec![
        case_1_tombs(),
        case_2_corruption_chamber(),
        case_3_chasm_late(),
        case_4_chasm_merge(),
        case_5_poison_garden(),
        case_6_cloister(),
    ]
}

/// The far endpoint of the single door a recommendation opens, for the
/// assertions that only care about which corridor was picked.
pub fn opened_toward(doors: &std::collections::BTreeSet<crate::temple::lattice::Edge>, from: Slot) -> Vec<Slot> {
    doors
        .iter()
        .map(|edge| {
            let (a, b) = edge.ends();
            if a == from {
                b
            } else {
                a
            }
        })
        .collect()
}
