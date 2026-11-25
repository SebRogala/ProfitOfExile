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
// He opened the Apex and never got back to connect the room — the trap while
// the corruption line was a target. Under the Cradle target set (POE-415) the
// Corruption Chamber is an ordinary room and the Apex door is the pick.
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
        decision: "upgrade the corruption line; open B1-A0 (no target room is stranded, \
                   so nothing outranks the Apex)",
    }
}

// --- Case 3 — 2026-08-03_22-54-58, 2 left, in Chasm (B0) -------------------
// Case 2's board one step on: nothing on it is stranded, so nothing outranks
// the Apex.
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

// --- Case 8 — 2026-09-03, 3 left, in Lightning Workshop (C1) --------------
// POE-248. One key, and only two corridors worth enumerating: C1-B0 and
// C1-B1 — every other closed door from C1 leads back into its own component.
// Both far ends are B slots, so the advisor scored them equal on R1-apex and
// fell through to R2, which prefers the smaller merge and therefore took
// B1 (a lone singleton) over B0 (a two-slot component with the Apex in it,
// because A0-B0 is already open). Sebastian says B0: connecting to a slot that
// is ALREADY connected to the Apex *reaches* the Apex, and that outranks a slot
// which is merely Apex-adjacent.
pub fn case_8_lightning_workshop() -> Case {
    Case {
        name: "8 LightningWorkshop",
        state: board(
            &[
                (B0, "house_of_the_others", 3),
                (B1, "wealth_of_the_vaal", 1),
                // Chasm — a tier-0 filler, no line at all.
                (C0, "", 0),
                // The room he is standing in.
                (C1, "conduit_of_lightning", 1),
                (C2, "upgrade", 2),
                (D0, "hall_of_champions", 3),
                (D1, "apex_of_ascension", 1),
                (D2, "storm_of_corruption", 1),
                (D3, "gem", 2),
                (E0, "court_of_sealed_death", 3),
                (E2, "hybridisation_chamber", 3),
            ],
            &[
                (A0, B0),
                (C0, D0),
                (C0, D1),
                (C1, C2),
                (C2, D2),
                (C2, D3),
                (D0, E0),
                (D1, D2),
                (D1, E0),
                (D1, E1),
                (D2, D3),
                (D3, E2),
            ],
            C1,
            3,
        ),
        offers: vec![
            offer("Hayoxi", Upgrade, "Grand Reactor"),
            offer("Xopec", Change, "Explosives Room"),
        ],
        keys: 1,
        decision: "open B0-C1 (A0-B0 is already open, so B0 reaches the Apex; \
                   B1 is only Apex-adjacent)",
    }
}

// --- Case 9 — 2026-09-09, 10 left, in Corruption Chamber I (C1) ------------
// The Apex is already in the room's own cluster (C1-B1-A0), and the Entrance
// cluster (E1, E0, D1, D0, C0) touches it on two corridors: Banquet Hall (C0)
// and Strongbox Chamber (D1). Either connects the same value and merges the
// same two clusters; the app opened toward Banquet Hall for being one row
// higher. Sebastian: *"both are open, and the middle is faster to arrive"* —
// through D1 the Entrance is five hops from the Apex and three from the
// Chamber, through C0 seven and five. RA.
pub fn case_9_corruption_chamber_arrival() -> Case {
    Case {
        name: "9 CorruptionChamber-arrival",
        state: board(
            &[
                (B0, "toxic_grove", 1),
                // Halls, Banquet Hall, Cellar — tier-0 fillers, no line at all.
                (B1, "", 0),
                (C0, "", 0),
                // The room he is standing in.
                (C1, "corruption", 1),
                (C2, "defense_research_lab", 2),
                (D0, "museum_of_artefacts", 1),
                (D1, "court_of_sealed_death", 1),
                (D2, "", 0),
                (D3, "chamber_of_iron", 1),
                (E0, "throne_of_atziri", 1),
                (E2, "hall_of_champions", 1),
            ],
            &[
                (A0, B1),
                (B1, C1),
                (C0, D0),
                (C2, D3),
                (D0, D1),
                (D1, E0),
                (D2, E2),
                (E0, E1),
            ],
            C1,
            10,
        ),
        offers: vec![
            offer("Hayoxi", Upgrade, "Catalyst of Corruption"),
            offer("Paquate", Change, "Explosives Room"),
        ],
        keys: 1,
        decision: "open C1-D1 (Strongbox Chamber: both corridors join the same two clusters, \
                   and the middle one arrives faster — Entrance → Apex 5 hops, not 7)",
    }
}

// --- Case 10 — 2026-11-24, 5 left, in Lightning Workshop I (B0) -----------
// POE-415, the Operator's "upgrade vs explosives" board
// (evidence/temple-upgrade-vs-explosives.png). Lightning Workshop I sits under
// the Apex with Shrine of Empowerment I connected below it. Hayoxi prints
// "upgrade to Grand Reactor", Xopec "change to Explosives Room". Under
// Consolidation option 1 the upgrade builds the conduit line at tier 3 while
// the change builds Explosives Room I, and the Operator takes the upgrade.
// Not a walked board: it stays out of [`retrospective`].
pub fn case_10_lightning_workshop_upgrade() -> Case {
    Case {
        name: "10 LightningWorkshop-upgrade",
        state: board(
            &[
                // The room he is standing in.
                (B0, "conduit_of_lightning", 1),
                (B1, "hall_of_champions", 3),
                (C0, "apex_of_ascension", 1),
                (C1, "upgrade", 1),
                (C2, "gem", 3),
                // Banquet Hall, Cellar — tier-0 fillers, no line at all.
                (D0, "", 0),
                (D1, "", 0),
                (D2, "corruption", 1),
                (D3, "chamber_of_iron", 1),
                (E0, "hybridisation_chamber", 1),
                (E2, "museum_of_artefacts", 3),
            ],
            &[
                (A0, B0),
                (A0, B1),
                (B0, B1),
                (B0, C1),
                (B1, C2),
                (C0, D1),
                (D1, E0),
                (D1, E1),
                (D2, D3),
                (D2, E1),
                (D2, E2),
                (E0, E1),
                (E1, E2),
            ],
            B0,
            5,
        ),
        offers: vec![
            offer("Hayoxi", Upgrade, "Grand Reactor"),
            offer("Xopec", Change, "Explosives Room"),
        ],
        keys: 1,
        decision: "upgrade → Grand Reactor (the conduit line at tier 3 under option 1), \
                   not change → Explosives Room",
    }
}

// --- Case 11 — 2026-11-24, 11 left, in Gemcutter's Workshop I (C2) --------
// POE-415, the Operator's "Halls vs Corruption Chamber" board
// (evidence/temple-door-halls-vs-corruption.png). The Entrance cluster is E1
// with Hatchery (E0), Cellar (D1) to Sacrificial Chamber (C0), and Corruption
// Chamber (D2) to Armourer's Workshop (D3); C2 touches it through D2 and D3.
// Jiquani prints "upgrade to Department of Thaumaturgy", Zalati "change to
// Workshop". The app sealed Corruption Chamber (C2-D2); the Operator wants
// Halls (C2-B1), the corridor beside the Apex. No Cradle is on the board, so
// nothing here is worth connecting to the Entrance.
// Not a walked board: it stays out of [`retrospective`].
pub fn case_11_gemcutters_workshop_halls() -> Case {
    Case {
        name: "11 GemcuttersWorkshop-halls",
        state: board(
            &[
                // Passageways, Halls, Tombs, Banquet Hall, Cellar — tier-0
                // fillers, no line at all.
                (B0, "", 0),
                (B1, "", 0),
                (C0, "apex_of_ascension", 1),
                (C1, "", 0),
                // The room he is standing in.
                (C2, "gem", 1),
                (D0, "", 0),
                (D1, "", 0),
                (D2, "corruption", 1),
                (D3, "chamber_of_iron", 1),
                (E0, "hybridisation_chamber", 1),
                (E2, "museum_of_artefacts", 1),
            ],
            &[(C0, D1), (D1, E1), (D2, E1), (D2, D3), (E0, E1)],
            C2,
            11,
        ),
        offers: vec![
            offer("Jiquani", Upgrade, "Department of Thaumaturgy"),
            offer("Zalati", Change, "Workshop"),
        ],
        keys: 1,
        decision: "open C2-B1 (Halls, beside the Apex), not C2-D2 (Corruption Chamber)",
    }
}

/// The eight WALKED boards, in order.
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
        case_8_lightning_workshop(),
        case_9_corruption_chamber_arrival(),
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
