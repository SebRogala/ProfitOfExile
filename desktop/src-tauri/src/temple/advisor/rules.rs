//! The explicit rule layer that sits **on top of** the EV ranking (POE-170).
//!
//! Nothing in this file exists in the Python prototype, and that is the point:
//! measured on the real boards, the rollout splits the options R1/R2/RS decide
//! by 0.001–0.007 — noise, no opinion (TEMPLE-CORE-RULES §6c). The rules do not
//! *emerge* from the rollout and must be encoded, the same treatment RV already
//! had.
//!
//! # Which decision each layer owns
//!
//! **MEASURED over the 11-board live build.** Every EV *miss* on the
//! scoreboard is a door call, and all but one of its successes is an architect
//! call — board 5 (Passageways) was a door call the EV got right. Boards 5, 7,
//! 8, 11 and Case 5 separate the two kills by 3–7 points, while boards 1 (R1),
//! 3 (RS) and 4 (R2) split the doors by 0.001–0.007 and were all wrong. So the
//! two axes get opposite treatment:
//!
//! - the **kill** is ranked by EV, with the architect rules breaking ties
//!   inside the rollout's own noise band ([`noise_margin`]);
//! - the **door set** is ranked by the chain, with EV breaking *its* ties.
//!
//! That is not a hedge. Sebastian declines to connect to the Entrance while the
//! budget is long (*"we are still 10 incursions left, so no need to connect to
//! entrance at all — if the room would be gem/corruption, that's entrance
//! pick"*), and the score only counts what the Entrance reaches, so the rollout
//! will always want the merge. The spec's own answer is that connectivity is
//! **instrumental, not terminal**, and the chain is where that lives.
//!
//! # The chain
//!
//! `RV > RU > R1-apex > RD > R2 > R1-gradient > RS`, and generic
//! cluster-merging ranked below R2 while no cluster holds value.
//!
//! That order is not a description of [`DoorKey`] — it IS [`DoorKey`], read off
//! its field order by the derived lexicographic `Ord`. Reordering the fields
//! reorders the chain.
//!
//! RU and RD sit *inside* the chain at those two positions rather than beside
//! it, and each is bounded by what it sits under: RU vetoes everything below it
//! but never RV, so an option that dilutes a saturated upgrade room still wins
//! when it is the one that connects value; RD is a count of corridors opened
//! and outranks R2, but not R1-apex, so spending both keys beats a tidier merge
//! and still loses to banking the Apex.
//!
//! Two placements deserve their justification in the file rather than a commit
//! message:
//!
//! - **R1-gradient sits below R2, not beside R1-apex.** Live board 1's second
//!   key (Tombs row C over Cellar row D) had R2 tied, so it cannot order the
//!   two; live board 4 took a lone singleton over a 5+5 merge, which R2
//!   explains and a gradient above R2 could contradict. Below R2 is the only
//!   placement consistent with every measured board.
//! - **Generic cluster-merging is not a separate rule.** It is R2 scoring
//!   badly: [`DoorScore::r2`] sums the sizes of the components each door joins
//!   and prefers the smaller total, so "attach a singleton" (5+1) outranks
//!   "merge two fives" (5+5) without a second mechanism.
//!
//! Hub degree is **not** a rule (re-checked 2026-08-18: board 1 was decided by
//! the row gradient, and the hub reading of it was commentary).

use std::collections::BTreeSet;

use crate::temple::lattice::{Edge, Slot};
use crate::temple::rooms::OfferKind;
use crate::temple::strategy::{Line, StrategyProfile, Tier};

use super::rollout::Valuation;
use super::state::{
    bits, component, lattice_degree, mask_holds, mask_of, neighbour_masks, BoardState, SlotMask,
};

/// EV gap below which two options are treated as indistinguishable and the
/// priority chain decides instead.
///
/// A floor, not the whole margin — [`noise_margin`] widens it to the rollout's
/// own standard error. The floor exists so the chain still decides a genuine
/// coin flip at large rollout counts: live board 1 split by 0.007 and case 6 by
/// 0.001, both far under any sampling error.
pub const NOISE_FLOOR: f64 = 0.05;

/// How many standard errors wide the indistinguishable band is.
///
/// It gates the **architect** axis only. At 400 rollouts the standard error of
/// one option's mean is ≈0.15, so this is a band of ≈0.45; every board where
/// the two kills genuinely differ separates them by 3–7 points, an order of
/// magnitude clear of it, and every board where the kill is a free choice
/// (R0 — both architects worthless) has the two within noise by construction.
/// 3 is the conventional "cannot distinguish these" threshold and sits in the
/// middle of that gap.
const NOISE_SIGMAS: f64 = 3.0;

/// The margin two EVs must differ by before the ranking trusts the difference.
///
/// [`NOISE_SIGMAS`] standard errors of the best estimate, floored at
/// [`NOISE_FLOOR`]. The floor carries boards where the rollout count is high
/// enough that the standard error vanishes but the options are still a genuine
/// coin flip — live board 1 split by 0.007 and case 6 by 0.001.
pub fn noise_margin(best_stderr: f64) -> f64 {
    NOISE_FLOOR.max(NOISE_SIGMAS * best_stderr)
}

// ------------------------------------------------------------- decisions ----

/// One architect kill, already resolved through
/// [`crate::temple::rooms::resolve_offer`].
#[derive(Debug, Clone, PartialEq)]
pub struct ArchitectChoice {
    /// Index into the panel's architect list, so the overlay can point at the
    /// right block.
    pub offer_index: usize,
    /// The architect's own name, as the panel printed it.
    pub architect_name: String,
    /// Resident (`change`) or non-resident (`upgrade`).
    pub kind: OfferKind,
    /// The line the kill actually builds.
    pub line: Line,
    /// The tier the kill **guarantees**. An upgrade also rolls
    /// [`crate::temple::strategy::DOUBLE_TIER_CHANCE`] for one more; that lives
    /// in the rollout, never in this deterministic floor.
    pub built_tier: Tier,
    /// The room name to show — the resolved one, never the panel's wording.
    pub display_name: &'static str,
}

/// One legal move: a kill, and the set of corridors to spend the keys on.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    /// `None` only when no architect block could be resolved at all; the door
    /// half of the advice is still worth ranking.
    pub architect: Option<ArchitectChoice>,
    /// The corridors to open. Empty is legal and sometimes optimal (RU).
    pub doors: BTreeSet<Edge>,
}

/// Why an option is where it is.
#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    /// RV — the door set shortens the hop distance from a valuable room to the
    /// Entrance component. `None` hops means "no path at all yet".
    Rv {
        line: Line,
        before: Option<usize>,
        after: Option<usize>,
    },
    /// RV, on the excluded side: this option leaves the valuable room stranded.
    RvGamble { line: Line, hops: Option<usize> },
    /// RV generalised — the door set brings a cluster that already holds a
    /// target-line room into the Entrance component.
    RvMerge { rooms: usize },
    /// R1-apex — a door whose far end is the Apex, or one of the only two
    /// slots that can ever open an Apex corridor.
    R1Apex,
    /// R2 — merges the smallest components available.
    R2 { joined: usize },
    /// R1 gradient — connects toward the top of the board.
    R1Gradient { row: u8 },
    /// RS — connects the scarcer slot, the one with fewer lattice neighbours.
    Rs { degree: usize },
    /// RU — the option would dilute a saturated upgrade room's target pool.
    Ru { slot: Slot, targets: usize, connected: usize },
    /// RD — opening a door beats opening none, absent a reason not to.
    Rd,
    /// RC — next to a built upgrade room, spend the architect on the line and
    /// let the shrine supply the tier.
    Rc { upgrader: Slot },
    /// RT — take the upgrade line speculatively while no valuable room exists.
    Rt,
    /// R4 — max a junk room to tier 3 so it leaves the drop pool.
    R4,
    /// R4's carve-out — keep this slot in the pool, an adjacent Shrine/Sanctum
    /// can still hit it.
    R4PoolCarveOut { upgrader: Slot },
    /// R4's carve-out, on the excluded side: this kill could take the slot to
    /// tier 3 and destroy the only thing an adjacent upgrade room can still
    /// lift.
    R4CarveOutVeto { upgrader: Slot },
    /// `reroll_until_favourable` — no favourable line exists, so change rather
    /// than upgrade junk.
    RerollUntilFavourable,
    /// The kill advances one of the strategy's target lines.
    AdvancesTarget { line: Line },
    /// No key dropped, so there is no door decision to make.
    ZeroKey,
    /// A key dropped but every corridor from here is pointless — its far end
    /// already shares this room's component — so there is nothing to buy.
    NoUsableDoor,
    /// RU declined the key: every corridor worth a key would dilute an upgrade
    /// room whose picks are currently certainties. Case 5's actual reason.
    RuDeclined { slot: Slot },
    /// Ranked on expected value alone; no rule discriminated.
    ExpectedValue,
}

impl Reason {
    /// One short line for the overlay. A bare score cannot be audited.
    pub fn describe(&self) -> String {
        match self {
            Reason::Rv { line, before, after } => format!(
                "RV: connects the {} room toward the Entrance ({} → {} hops)",
                line.key(),
                describe_hops(*before),
                describe_hops(*after)
            ),
            Reason::RvGamble { line, hops } => format!(
                "RV gamble: leaves the {} room {} from the Entrance",
                line.key(),
                match hops {
                    Some(hops) => format!("{hops} hops"),
                    None => "unreachable".to_string(),
                }
            ),
            Reason::RvMerge { rooms } => format!(
                "RV: connects {rooms} target-line room(s) to the Entrance"
            ),
            Reason::R1Apex => "R1: reaches the Apex or an Apex-adjacent slot — \
                 only B0/B1 can ever open the Apex"
                .to_string(),
            Reason::R2 { joined } => format!("R2: merges the smallest clusters ({joined} rooms)"),
            Reason::R1Gradient { row } => {
                format!("R1: connects upward, to row {}", (b'A' + row) as char)
            }
            Reason::Rs { degree } => {
                format!("RS: connects the scarcer slot ({degree} lattice neighbours)")
            }
            Reason::Ru {
                slot,
                targets,
                connected,
            } => format!(
                "RU: would dilute {} — {targets} picks over {connected} connected neighbours",
                slot.as_str()
            ),
            Reason::Rd => "RD: an open room pays, and the chance may not come again".to_string(),
            Reason::Rc { upgrader } => format!(
                "RC: {} supplies the tier, so spend the architect on the line",
                upgrader.as_str()
            ),
            Reason::Rt => "RT: no valuable room on the board yet, so take the upgrade line"
                .to_string(),
            Reason::R4 => "R4: maxes a junk room out of the drop pool".to_string(),
            Reason::R4PoolCarveOut { upgrader } => format!(
                "R4 carve-out: stays in the pool as a live target of {}",
                upgrader.as_str()
            ),
            Reason::R4CarveOutVeto { upgrader } => format!(
                "R4 carve-out: maxing this room would cost {} its only live target",
                upgrader.as_str()
            ),
            Reason::RerollUntilFavourable => {
                "no favourable line on the board, so change rather than upgrade junk".to_string()
            }
            Reason::AdvancesTarget { line } => {
                format!("advances the {} line", line.key())
            }
            Reason::ZeroKey => "no key dropped — there is no passage to buy".to_string(),
            Reason::NoUsableDoor => {
                "no corridor left worth a key — every closed one leads back into this room's \
                 own cluster"
                    .to_string()
            }
            Reason::RuDeclined { slot } => format!(
                "RU: every corridor a key could buy would dilute {}, so open nothing",
                slot.as_str()
            ),
            Reason::ExpectedValue => "ranked on expected value; no rule discriminated".to_string(),
        }
    }
}

// ----------------------------------------------------------- enumeration ----

/// Every legal door set for the current room.
///
/// - **Pointless doors are excluded.** A corridor whose endpoints already share
///   a component cannot change reachability, so it is exactly equal to opening
///   nothing and strictly dominated by any other door. It was polluting the
///   prototype's rankings until 2026-08-07.
/// - **Redundant pairs are excluded** for the same reason one step later: two
///   keys spent on two doors into the *same* component buy one merge.
/// - The **empty set is always legal** — RU makes it optimal on Case 5, and a
///   zero-key incursion makes it the only option.
///
/// **Known gap, measured on Case 5.** "Cannot change reachability" is not the
/// same as "cannot change anything": opening a corridor between two rooms that
/// already share a component still raises an adjacent upgrade room's connected
/// degree, which is precisely what RU prices. Both of Poison Garden's closed
/// corridors are pointless by this filter, so the advisor reaches Sebastian's
/// answer — open nothing — by never enumerating the doors he declined, and
/// prints [`Reason::NoUsableDoor`] where his own reason was RU. On Case 5 that
/// is the same advice with a different explanation — but not in general: a
/// **tier-2** upgrade room with one open corridor and a second neighbour already
/// in its component gains a second *certain* target from a "pointless" corridor,
/// a real move this filter cannot enumerate (tier 1 cannot produce it — one
/// pick, already saturated at degree ≥ 1). The correct narrowing — keep a
/// corridor whenever either endpoint is, or the kill makes it, a tier-1/2
/// upgrade room — makes enumeration architect-dependent and moves the seam every
/// rule reads, so it is deferred to the follow-up with POE-171 rather than done
/// here.
pub fn door_sets(board: &BoardState, position: Slot, keys: u8) -> Vec<BTreeSet<Edge>> {
    let mut sets = vec![BTreeSet::new()];
    if keys == 0 {
        return sets;
    }
    let open = board.adjacency();
    let mine = component(&open, position);
    let candidates: Vec<Edge> = board
        .closed_doors_from(position)
        .into_iter()
        .filter(|edge| {
            let (a, b) = edge.ends();
            let far = if a == position { b } else { a };
            !mask_holds(mine, far.index())
        })
        .collect();

    for (i, first) in candidates.iter().enumerate() {
        sets.push(BTreeSet::from([*first]));
        if keys < 2 {
            continue;
        }
        for second in &candidates[i + 1..] {
            let (a, b) = (far_end(*first, position), far_end(*second, position));
            let mut trial = open;
            let (p, q) = first.ends();
            trial[p.index()] |= mask_of(q);
            trial[q.index()] |= mask_of(p);
            if mask_holds(component(&trial, a), b.index()) {
                // The second key would buy a merge the first already bought.
                continue;
            }
            sets.push(BTreeSet::from([*first, *second]));
        }
    }
    sets
}

/// R1-apex's predicate.
///
/// The Apex itself, or one of the only two slots that can ever open an Apex
/// corridor. Reaching B0 or B1 buys the *opportunity* to open the Apex, which
/// is the scarcity R1-apex is about — the Apex is never a drop room, so 2 of 12
/// slots are the whole supply.
fn reaches_apex(far: Slot) -> bool {
    far == Slot::APEX || far == Slot::B0 || far == Slot::B1
}

fn far_end(edge: Edge, from: Slot) -> Slot {
    let (a, b) = edge.ends();
    if a == from {
        b
    } else {
        a
    }
}

// ------------------------------------------------------------------- RV -----

/// RV's verdict on one option.
#[derive(Debug, Clone, PartialEq)]
pub enum RvVerdict {
    /// RV does not apply, or the option satisfies it.
    Allowed(Option<Reason>),
    /// The option strands a valuable room. Surfaced as a labelled gamble with
    /// its EV and its measured risk, never hidden — Sebastian, 2026-08-05:
    /// *"make it as 'choosing the Apex is gamble' info, and system still
    /// favours going down"*.
    Gamble(Reason),
}

/// RV — a valuable room must be reachable, and this outranks everything else.
///
/// It is a **hard constraint layered over the EV ranking**, not something the
/// rollout learns: with pure EV the model picks the Apex on Case 2 — Sebastian's
/// own historical loss — because it rates late recovery above 90%. CVaR-25 did
/// not fix it either, so the disagreement is in the recovery model, not in risk
/// attitude. Product decision, 2026-08-05; revisit against real outcome data.
pub fn rv_verdict(
    board: &BoardState,
    position: Slot,
    becomes: Option<&Line>,
    doors: &BTreeSet<Edge>,
    valuation: &Valuation,
    connectable: bool,
) -> RvVerdict {
    // RV constrains the **passage**, never the kill. With no door set on offer
    // that shortens the walk, there is no passage to constrain and every option
    // is allowed — otherwise the kill Sebastian calls a no-brainer (Case 2's
    // corruption upgrade) lands in `gambles` on a zero-key board while the junk
    // change stays safe, and the advisor recommends changing to junk.
    if !connectable {
        return RvVerdict::Allowed(None);
    }
    let open = board.adjacency();
    let entrance = component(&open, Slot::ENTRANCE);
    if mask_holds(entrance, position.index()) {
        return RvVerdict::Allowed(None);
    }
    let Some(line) = becomes else {
        return RvVerdict::Allowed(None);
    };
    if !valuation.is_target(valuation.tag(line)) {
        return RvVerdict::Allowed(None);
    }

    let before = hops_to(&open, position, entrance);
    let mut trial = open;
    for edge in doors {
        let (a, b) = edge.ends();
        trial[a.index()] |= mask_of(b);
        trial[b.index()] |= mask_of(a);
    }
    let after = hops_to(&trial, position, entrance);
    if shortens(before, after) {
        RvVerdict::Allowed(Some(Reason::Rv {
            line: line.clone(),
            before,
            after,
        }))
    } else {
        RvVerdict::Gamble(Reason::RvGamble {
            line: line.clone(),
            hops: after,
        })
    }
}

/// Whether RV has any opinion to give on this board.
///
/// True when at least one enumerated door set shortens the walk from `position`
/// to the Entrance component. When nothing does — a zero-key incursion, or a
/// room whose every closed corridor leads back into its own cluster — RV cannot
/// be satisfied by any choice, so it must not split the options into safe and
/// risky: the split would be about the kill, which RV never governs.
pub fn rv_connectable(board: &BoardState, position: Slot, sets: &[BTreeSet<Edge>]) -> bool {
    let open = board.adjacency();
    let entrance = component(&open, Slot::ENTRANCE);
    if mask_holds(entrance, position.index()) {
        return false;
    }
    let before = hops_to(&open, position, entrance);
    sets.iter().any(|doors| {
        let mut trial = open;
        for edge in doors {
            let (a, b) = edge.ends();
            trial[a.index()] |= mask_of(b);
            trial[b.index()] |= mask_of(a);
        }
        shortens(before, hops_to(&trial, position, entrance))
    })
}

/// `None` is "no path at all", which must compare as worse than any number.
fn shortens(before: Option<usize>, after: Option<usize>) -> bool {
    match (before, after) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(before), Some(after)) => after < before,
    }
}

fn describe_hops(hops: Option<usize>) -> String {
    match hops {
        Some(hops) => hops.to_string(),
        None => "∞".to_string(),
    }
}

/// Hops from `from` to the nearest slot of `target`, over open doors; `None`
/// when no open path exists.
fn hops_to(open: &[SlotMask; 13], from: Slot, target: SlotMask) -> Option<usize> {
    let mut seen = mask_of(from);
    let mut frontier = seen;
    let mut depth = 0;
    while frontier != 0 {
        if frontier & target != 0 {
            return Some(depth);
        }
        let mut next = 0;
        for i in bits(frontier) {
            next |= open[i];
        }
        next &= !seen;
        seen |= next;
        frontier = next;
        depth += 1;
    }
    None
}

// -------------------------------------------------------- the rule score ----

/// The door half of the priority chain, as a lexicographic key. Greater is
/// better.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DoorKey {
    /// RV, generalised past the room you are standing in: how many rooms of a
    /// target line the set newly connects to the Entrance.
    ///
    /// This is what the spec means by *"a merge is worth a key only once a
    /// cluster holds something worth reaching — and at that point it is RV
    /// firing, not R1"*. It is the reason generic cluster-merging ranks below
    /// R2 **while no cluster holds value**, and above everything once one does.
    pub connects_value: u8,
    /// RU: 1 unless the option dilutes a saturated upgrade room. A veto over
    /// everything below it, but not over RV.
    pub ru_ok: u8,
    /// R1-apex: how many Apex or Apex-adjacent corridors the set opens.
    ///
    /// It sits **above** RD's count because the redundancy filter can drop the
    /// pair that would have spent both keys: a two-set of junk corridors must
    /// not outrank the one-set that banks the scarcest connectivity on the
    /// board.
    pub r1_apex: u8,
    /// RD: how many corridors the set opens.
    ///
    /// A **count**, not a flag: keys are use-it-or-lose-it, so with two of them
    /// the two-door set is the move and a one-door set banks nothing. It sits
    /// above R2 because R2 sums over the set and would otherwise punish
    /// spending the second key.
    pub opens: u8,
    /// R2: negated total size of the components each door joins, so smaller
    /// merges rank higher and a generic 5+5 merge falls below a 5+1 attach.
    pub r2: i32,
    /// R1 gradient: negated best (lowest) row index the set reaches.
    pub r1_gradient: i32,
    /// RS: negated smallest lattice degree among the set's **far** ends.
    pub rs: i32,
}

/// The architect half. Greater is better.
///
/// Byte 0 is R4's carve-out **veto** — 0 when the kill would take an upgrade
/// room's only live target out of the drop pool. It is not a tie-break: the
/// ranking reads it above the EV ordering, because on live board 8 the rollout
/// prefers the double-tier roll for R4's own reason and separates the two kills
/// past the noise band. The remaining bytes are the tie-breaks, in precedence
/// order: advances a target line, RT, RC, then R4 or `reroll_until_favourable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArchKey(pub [u8; 5]);

impl ArchKey {
    /// Whether R4's carve-out vetoed this kill. See [`ArchKey`]'s byte 0.
    pub fn vetoed(self) -> bool {
        self.0[0] == 0
    }
}

/// Everything the rule layer computed about one option.
#[derive(Debug, Clone)]
pub struct RuleVerdict {
    /// How the door set ranks.
    pub door: DoorKey,
    /// How the kill ranks.
    pub architect: ArchKey,
    /// Every rule that fired, for the overlay to print.
    pub reasons: Vec<Reason>,
}

/// Score one option against the whole chain.
///
/// `keys` is how many opening stones dropped. It is not redundant with `doors`:
/// an empty door set means three different things — no key fell, a key fell
/// with nowhere to spend it, or RU declined every corridor a key could buy —
/// and the overlay must not print the wrong one.
pub fn evaluate_rules(
    board: &BoardState,
    position: Slot,
    architect: Option<&ArchitectChoice>,
    doors: &BTreeSet<Edge>,
    keys: u8,
    profile: &StrategyProfile,
    valuation: &Valuation,
) -> RuleVerdict {
    let mut reasons = Vec::new();
    let open = board.adjacency();

    // -- the door half ----------------------------------------------------
    let mut r1_apex = 0u8;
    let mut r2_total = 0i32;
    let mut best_row = u8::MAX;
    let mut scarcest = usize::MAX;
    let mut trial = open;
    for edge in doors {
        let (a, b) = edge.ends();
        let far = far_end(*edge, position);
        if reaches_apex(far) {
            r1_apex += 1;
        }
        r2_total += component(&trial, a).count_ones() as i32;
        r2_total += component(&trial, b).count_ones() as i32;
        trial[a.index()] |= mask_of(b);
        trial[b.index()] |= mask_of(a);
        best_row = best_row.min(far.row());
        // The **far** end only: the room you are standing in is a constant
        // across every option, so folding its degree in can only mask the
        // difference RS exists to read.
        scarcest = scarcest.min(lattice_degree(far));
    }

    let connects_value = newly_connected_targets(board, &open, &trial, valuation);
    if connects_value > 0 {
        reasons.push(Reason::RvMerge {
            rooms: connects_value as usize,
        });
    }
    if r1_apex > 0 {
        reasons.push(Reason::R1Apex);
    }
    if !doors.is_empty() {
        reasons.push(Reason::Rd);
        reasons.push(Reason::R2 {
            joined: r2_total as usize,
        });
        reasons.push(Reason::R1Gradient { row: best_row });
        reasons.push(Reason::Rs { degree: scarcest });
    } else if let Some(reason) = empty_set_reason(board, position, architect, keys) {
        reasons.push(reason);
    }

    // -- RU ---------------------------------------------------------------
    let after = applied(board, position, architect, doors);
    let ru = ru_violation(board, &after, doors);
    if let Some(reason) = &ru {
        reasons.push(reason.clone());
    }

    // -- the architect half -----------------------------------------------
    let architect_key =
        architect_rules(board, position, architect, valuation, profile, &mut reasons);

    RuleVerdict {
        door: DoorKey {
            connects_value,
            ru_ok: u8::from(ru.is_none()),
            r1_apex,
            opens: doors.len() as u8,
            r2: -r2_total,
            r1_gradient: if doors.is_empty() {
                0
            } else {
                -(best_row as i32)
            },
            rs: if doors.is_empty() {
                0
            } else {
                -(scarcest as i32)
            },
        },
        architect: ArchKey(architect_key),
        reasons,
    }
}

/// How many rooms of a target line the door set newly brings into the Entrance
/// component.
///
/// Counts *rooms*, not clusters: a merge that hands the Entrance both a
/// Corruption Chamber and a Gemcutter's Workshop is worth more than one that
/// hands it either alone.
fn newly_connected_targets(
    board: &BoardState,
    before: &[SlotMask; 13],
    after: &[SlotMask; 13],
    valuation: &Valuation,
) -> u8 {
    let was = component(before, Slot::ENTRANCE);
    let now = component(after, Slot::ENTRANCE);
    let gained = now & !was;
    if gained == 0 {
        return 0;
    }
    bits(gained)
        .filter(|i| {
            let (line, tier) = &board.rooms[*i];
            *tier != Tier::T0
                && line
                    .as_ref()
                    .is_some_and(|line| valuation.is_target(valuation.tag(line)))
        })
        .count() as u8
}

/// Why this option opens nothing — three different facts, and the overlay must
/// not print "no key dropped" for the other two.
///
/// Ordered by what the player can act on: no key at all, a key with every
/// corridor pointless, and finally RU's own refusal — the only one of the three
/// that is a *choice*.
fn empty_set_reason(
    board: &BoardState,
    position: Slot,
    architect: Option<&ArchitectChoice>,
    keys: u8,
) -> Option<Reason> {
    if keys == 0 {
        return Some(Reason::ZeroKey);
    }
    let spendable: Vec<BTreeSet<Edge>> = door_sets(board, position, keys)
        .into_iter()
        .filter(|set| !set.is_empty())
        .collect();
    if spendable.is_empty() {
        return Some(Reason::NoUsableDoor);
    }
    let mut declined: Option<Slot> = None;
    for set in &spendable {
        let after = applied(board, position, architect, set);
        let Some(Reason::Ru { slot, .. }) = ru_violation(board, &after, set) else {
            // One clean corridor exists, so opening nothing is a ranking, not a
            // reason. RD is what puts it last.
            return None;
        };
        declined = declined.or(Some(slot));
    }
    declined.map(|slot| Reason::RuDeclined { slot })
}

/// The board after this option is played, used by the rules that need to see
/// the room the kill *creates* rather than the one it replaces (RU on Case 5).
fn applied(
    board: &BoardState,
    position: Slot,
    architect: Option<&ArchitectChoice>,
    doors: &BTreeSet<Edge>,
) -> BoardState {
    let mut after = board.clone();
    if let Some(architect) = architect {
        after.set_room(position, Some(architect.line.clone()), architect.built_tier);
    }
    after.doors.extend(doors.iter().copied());
    after
}

/// RU — an upgrade room's connected degree is a resource, and opening a door
/// can spend it.
///
/// The upgrade line picks `k` random targets from its connected neighbours, so
/// a specific target's odds are `min(1, k / degree)`. Below `k` connections
/// every neighbour is hit and a new door is free; at or above `k` a new door
/// turns a certainty into a lottery. Case 5 is the whole rule: Poison Garden
/// changes to Sanctum of Unity II with exactly two connected neighbours, so
/// **do not open a third door**.
fn ru_violation(before: &BoardState, after: &BoardState, doors: &BTreeSet<Edge>) -> Option<Reason> {
    if doors.is_empty() {
        return None;
    }
    let before_open = before.adjacency();
    let after_open = after.adjacency();
    let neighbours = neighbour_masks();
    for &slot in &Slot::ALL {
        if after.line(slot) != Some(&Line::Upgrade) {
            continue;
        }
        let tier = after.tier(slot).get();
        if tier != 1 && tier != 2 {
            continue; // Temple Nexus III ignores connections entirely.
        }
        let targets = tier as usize;
        let pool = neighbours[slot.index()] & !mask_of(Slot::APEX);
        let was = (before_open[slot.index()] & pool).count_ones() as usize;
        let now = (after_open[slot.index()] & pool).count_ones() as usize;
        if now > was && was >= targets {
            return Some(Reason::Ru {
                slot,
                targets,
                connected: now,
            });
        }
    }
    None
}

/// RC, RT, R4 and `reroll_until_favourable`, in that order of precedence.
fn architect_rules(
    board: &BoardState,
    position: Slot,
    architect: Option<&ArchitectChoice>,
    valuation: &Valuation,
    profile: &StrategyProfile,
    reasons: &mut Vec<Reason>,
) -> [u8; 5] {
    // Byte 0 is the carve-out veto, so "no architect resolved" must read as
    // *not vetoed* rather than as the worst kill on the board.
    let Some(architect) = architect else {
        return [1, 0, 0, 0, 0];
    };
    let mut key = [1u8, 0, 0, 0, 0];

    let advances = valuation.is_target(valuation.tag(&architect.line));
    if advances {
        key[1] = 1;
        reasons.push(Reason::AdvancesTarget {
            line: architect.line.clone(),
        });
    }

    let board_has_target = Slot::ALL.iter().any(|s| {
        board.tier(*s) != Tier::T0
            && board
                .line(*s)
                .is_some_and(|line| valuation.is_target(valuation.tag(line)))
    });

    // RT — while nothing valuable exists, the upgrade line's neighbours are all
    // still in the drop pool, so any of them may become the thing it upgrades.
    // Once a valuable room exists, placement becomes deliberate.
    if !board_has_target && architect.line == Line::Upgrade {
        key[2] = 1;
        reasons.push(Reason::Rt);
    }

    // RC — an adjacent, connected upgrade room supplies +1 deterministically,
    // so `change` gets the same tier while leaving the shrine's scarce pick for
    // something else. Tier 3 (Temple Nexus) ignores connections.
    let upgrader = neighbouring_upgrader(board, position);
    if let Some(upgrader) = upgrader {
        if architect.kind == OfferKind::Change {
            key[3] = 1;
            reasons.push(Reason::Rc { upgrader });
        }
    }

    // R4 vs reroll_until_favourable — mutually exclusive by construction: the
    // reroll flag only speaks while no favourable line exists, which is exactly
    // when the spec hands the decision to it instead of R4.
    //
    // **What actually discriminates the two offers.** With Contested
    // Development both kinds land at `currentTier + 1`, so `built_tier` is
    // identical for both architects and cannot express R4 at all. The
    // difference is the 50% double-tier roll, which only `upgrade` gets — so
    // "max this room out of the pool" means *take the upgrade*, and "keep it in
    // the pool" means *take the change*. That is also exactly the trade RC
    // describes from the other side.
    if profile.reroll_until_favourable && !board_has_target {
        if architect.kind == OfferKind::Change {
            key[4] = 1;
            reasons.push(Reason::RerollUntilFavourable);
        }
    } else if !advances {
        // "Would this kill take the slot out of the pool?" — a guaranteed tier
        // 3, or the `upgrade` that rolls `DOUBLE_TIER_CHANCE` for one more.
        let leaves_pool = architect.built_tier == Tier::T3
            || (architect.kind == OfferKind::Upgrade && board.tier(position) != Tier::T0);
        let carve_out = profile
            .r4_keep_upgrade_targets
            .then(|| live_upgrade_target(board, position))
            .flatten();
        match carve_out {
            // A **veto**, not a tie-break. On live board 8 the rollout prefers
            // the double-tier roll for R4's own reason and separates the two
            // kills past the noise band, so a tie-break inside that band would
            // never fire and the advisor would recommend destroying the
            // Sanctum's only live target. A kill that builds a target line is
            // never vetoed — an upgrade to Locus or Doryani outranks any pool
            // argument.
            Some(upgrader) => {
                if leaves_pool {
                    key[0] = 0;
                    reasons.push(Reason::R4CarveOutVeto { upgrader });
                } else {
                    key[4] = 1;
                    reasons.push(Reason::R4PoolCarveOut { upgrader });
                }
            }
            None => {
                if leaves_pool {
                    key[4] = 1;
                    reasons.push(Reason::R4);
                }
            }
        }
    }

    key
}

/// An adjacent upgrade room that will supply this slot a tier: tiers 1–2 need
/// an open corridor, tier 3 (Temple Nexus) does not.
fn neighbouring_upgrader(board: &BoardState, slot: Slot) -> Option<Slot> {
    let open = board.adjacency();
    bits(neighbour_masks()[slot.index()])
        .map(|i| Slot::ALL[i])
        .find(|n| {
            board.line(*n) == Some(&Line::Upgrade)
                && board.tier(*n) != Tier::T0
                && (board.tier(*n) == Tier::T3 || mask_holds(open[slot.index()], n.index()))
        })
}

/// R4's carve-out condition: an adjacent upgrade room that can still hit this
/// slot, and whose pick on it is a certainty rather than a lottery ticket.
///
/// Three gates:
///
/// - the slot must be liftable at all — tier 0 wastes the pick and tier 3
///   cannot be lifted, so neither is worth keeping in the drop pool for;
/// - the upgrade room must be able to reach it: an **open corridor** at tiers
///   1–2, bare **adjacency** at tier 3, because the Temple Nexus ignores doors;
/// - at tiers 1–2 its picks must be **unsaturated** — at most as many connected
///   neighbours as picks, so this slot is certain to be lifted. A shrine whose
///   pick is already a lottery is not a synergy worth spending a drop-pool slot
///   on, and RU is the rule that keeps it that way.
fn live_upgrade_target(board: &BoardState, slot: Slot) -> Option<Slot> {
    if board.tier(slot) != Tier::T1 && board.tier(slot) != Tier::T2 {
        return None;
    }
    let open = board.adjacency();
    let neighbours = neighbour_masks();
    bits(neighbours[slot.index()])
        .map(|i| Slot::ALL[i])
        .find(|n| {
            if board.line(*n) != Some(&Line::Upgrade) {
                return false;
            }
            let picks = board.tier(*n).get();
            if picks == 3 {
                return true;
            }
            if picks == 0 || !mask_holds(open[slot.index()], n.index()) {
                return false;
            }
            let pool = neighbours[n.index()] & !mask_of(Slot::APEX);
            (open[n.index()] & pool).count_ones() as usize <= picks as usize
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::lattice::Slot::*;
    use crate::temple::rooms::OfferKind;
    use crate::temple::strategy::Tier;

    use super::super::rollout::Valuation;
    use super::super::state::BoardState;

    fn rush() -> StrategyProfile {
        StrategyProfile::locus_doryani_rush()
    }

    fn board(rooms: &[(Slot, &str, u8)], doors: &[(Slot, Slot)]) -> BoardState {
        let mut state = BoardState::empty();
        for (slot, key, tier) in rooms {
            let line = (!key.is_empty()).then(|| Line::named(key));
            state.set_room(*slot, line, Tier::new(*tier).expect("0..=3"));
        }
        state.doors = doors.iter().map(|(a, b)| Edge::new(*a, *b)).collect();
        state
    }

    fn choice(kind: OfferKind, key: &str, tier: u8) -> ArchitectChoice {
        ArchitectChoice {
            offer_index: 0,
            architect_name: "Ticaba".to_string(),
            kind,
            line: Line::named(key),
            built_tier: Tier::new(tier).expect("0..=3"),
            display_name: "test room",
        }
    }

    fn verdict(
        state: &BoardState,
        position: Slot,
        architect: Option<&ArchitectChoice>,
        doors: &[(Slot, Slot)],
    ) -> RuleVerdict {
        verdict_with(state, position, architect, doors, 1, &rush())
    }

    fn verdict_with(
        state: &BoardState,
        position: Slot,
        architect: Option<&ArchitectChoice>,
        doors: &[(Slot, Slot)],
        keys: u8,
        profile: &StrategyProfile,
    ) -> RuleVerdict {
        let doors: BTreeSet<Edge> = doors.iter().map(|(a, b)| Edge::new(*a, *b)).collect();
        evaluate_rules(
            state,
            position,
            architect,
            &doors,
            keys,
            profile,
            &Valuation::for_profile(&rush()),
        )
    }

    fn far(sets: &[BTreeSet<Edge>], from: Slot) -> Vec<Vec<Slot>> {
        sets.iter()
            .map(|set| {
                set.iter()
                    .map(|e| {
                        let (a, b) = e.ends();
                        if a == from {
                            b
                        } else {
                            a
                        }
                    })
                    .collect()
            })
            .collect()
    }

    // ------------------------------------------------------- enumeration --

    // A corridor whose endpoints already share a component cannot change
    // reachability, so it is exactly equal to opening nothing. It was polluting
    // the prototype's rankings until 2026-08-07.
    #[test]
    fn a_corridor_between_two_rooms_of_one_component_is_never_offered() {
        // C1-D1 is closed, but C1 and D1 are already one component by way of
        // D2 — so opening it cannot change reachability. C1-B0 can.
        let state = board(&[], &[(C1, D2), (D1, D2)]);
        let offered = far(&door_sets(&state, C1, 1), C1);
        assert!(
            !offered.contains(&vec![D1]),
            "pointless corridor offered: {offered:?}"
        );
        assert!(
            offered.contains(&vec![B0]),
            "a corridor that does change a component must survive: {offered:?}"
        );
    }

    // The empty set is always legal — RU makes it optimal, and a zero-key
    // incursion makes it the only move.
    #[test]
    fn opening_nothing_is_always_among_the_legal_sets() {
        assert_eq!(door_sets(&board(&[], &[]), C1, 0), vec![BTreeSet::new()]);
        assert!(door_sets(&board(&[], &[]), C1, 2).contains(&BTreeSet::new()));
    }

    // With two keys the sets grow to pairs; with one they never do.
    #[test]
    fn the_key_count_caps_the_size_of_a_door_set() {
        let state = board(&[], &[]);
        assert!(door_sets(&state, C1, 1).iter().all(|set| set.len() <= 1));
        assert!(door_sets(&state, C1, 2).iter().any(|set| set.len() == 2));
    }

    // Two keys into the same component buy one merge, so the pair is dropped.
    #[test]
    fn a_pair_of_doors_into_one_component_is_dropped_as_redundant() {
        // D1 and D2 are joined to each other, so C1-D1 plus C1-D2 is one merge.
        let state = board(&[], &[(D1, D2)]);
        let offered = far(&door_sets(&state, C1, 2), C1);
        for set in &offered {
            assert!(
                !(set.contains(&D1) && set.contains(&D2)),
                "redundant pair offered: {set:?}"
            );
        }
        // The same two keys spent on genuinely separate merges survive.
        assert!(
            offered.contains(&vec![B0, D1]),
            "two distinct merges must still be offered: {offered:?}"
        );
    }

    // --------------------------------------------------------------- RV ---

    // RV does not apply while the room you are standing in is already
    // connected: it protects a *stranded* valuable room.
    #[test]
    fn rv_is_silent_while_the_current_room_is_in_the_entrance_component() {
        let state = board(&[(D1, "corruption", 1)], &[(D1, E1)]);
        let doors = BTreeSet::from([Edge::new(D1, C1)]);
        let verdict = rv_verdict(
            &state,
            D1,
            Some(&Line::Corruption),
            &doors,
            &Valuation::for_profile(&rush()),
            true,
        );
        assert_eq!(verdict, RvVerdict::Allowed(None));
    }

    // A stranded valuable room's connection is the scarcest opportunity on the
    // board: you only get to open its doors while standing in it.
    #[test]
    fn rv_keeps_only_the_doors_that_shorten_the_walk_to_the_entrance() {
        // B1 holds a Corruption Chamber and is stranded; C1 leads to the
        // Entrance side, A0 (the Apex) leads nowhere.
        let state = board(&[(B1, "corruption", 1)], &[(C1, D1), (D1, E1)]);
        let valuation = Valuation::for_profile(&rush());
        let down = rv_verdict(
            &state,
            B1,
            Some(&Line::Corruption),
            &BTreeSet::from([Edge::new(B1, C1)]),
            &valuation,
            true,
        );
        assert!(matches!(
            down,
            RvVerdict::Allowed(Some(Reason::Rv {
                before: None,
                after: Some(1),
                ..
            }))
        ));
        let apex = rv_verdict(
            &state,
            B1,
            Some(&Line::Corruption),
            &BTreeSet::from([Edge::new(B1, A0)]),
            &valuation,
            true,
        );
        assert!(matches!(apex, RvVerdict::Gamble(Reason::RvGamble { .. })));
    }

    // RV constrains the passage, never the kill: with no door set on offer that
    // shortens the walk, the same Apex door stops being a gamble, because
    // declining it buys the stranded room nothing either.
    #[test]
    fn rv_has_no_opinion_when_no_door_set_can_shorten_the_walk() {
        let state = board(&[(B1, "corruption", 1)], &[(C1, D1), (D1, E1)]);
        let apex = rv_verdict(
            &state,
            B1,
            Some(&Line::Corruption),
            &BTreeSet::from([Edge::new(B1, A0)]),
            &Valuation::for_profile(&rush()),
            false,
        );
        assert_eq!(apex, RvVerdict::Allowed(None));
    }

    // The gate itself: a zero-key incursion offers only the empty set, which can
    // never shorten anything, while one key onto a connecting corridor can.
    #[test]
    fn rv_is_connectable_only_while_some_enumerated_set_shortens_the_walk() {
        let state = board(&[(B1, "corruption", 1)], &[(C1, D1), (D1, E1)]);
        assert!(!rv_connectable(&state, B1, &door_sets(&state, B1, 0)));
        assert!(rv_connectable(&state, B1, &door_sets(&state, B1, 1)));

        // Every closed corridor from B1 leads back into B1's own cluster, so a
        // key buys nothing and RV again has no opinion.
        let boxed_in = board(
            &[(B1, "corruption", 1)],
            &[(B1, C1), (C1, C2), (B0, C1), (A0, B0)],
        );
        assert!(!rv_connectable(&boxed_in, B1, &door_sets(&boxed_in, B1, 1)));
    }

    // Changing the room to something worthless destroys it yourself, so RV has
    // nothing left to protect and the option is not a gamble.
    #[test]
    fn rv_is_silent_when_the_kill_does_not_build_a_target_line() {
        let state = board(&[(B1, "corruption", 1)], &[(C1, D1), (D1, E1)]);
        let verdict = rv_verdict(
            &state,
            B1,
            Some(&Line::named("glittering_halls")),
            &BTreeSet::from([Edge::new(B1, A0)]),
            &Valuation::for_profile(&rush()),
            true,
        );
        assert_eq!(verdict, RvVerdict::Allowed(None));
    }

    // ------------------------------------------------------- the ordering --

    // The chain's shape, asserted directly on the key so a reordering of the
    // fields fails here rather than silently changing a board's advice.
    #[test]
    fn the_door_key_orders_rv_above_ru_above_r1_apex_above_rd() {
        let base = DoorKey {
            connects_value: 0,
            ru_ok: 1,
            r1_apex: 0,
            opens: 1,
            r2: -4,
            r1_gradient: -2,
            rs: -3,
        };
        let rv = DoorKey {
            connects_value: 1,
            ru_ok: 0,
            r1_apex: 0,
            opens: 0,
            r2: -99,
            r1_gradient: -9,
            rs: -9,
        };
        assert!(rv > base, "RV outranks every scarcity rule, RU included");
        let diluting = DoorKey {
            ru_ok: 0,
            r1_apex: 9,
            ..base
        };
        assert!(base > diluting, "RU vetoes an Apex door that dilutes a shrine");
        let nothing = DoorKey { opens: 0, r2: 0, r1_gradient: 0, rs: 0, ..base };
        assert!(base > nothing, "RD: opening something beats opening nothing");
        let both_keys = DoorKey { opens: 2, r2: -20, ..base };
        assert!(both_keys > base, "keys are use-it-or-lose-it");
        // The redundancy filter can drop the pair that would have spent both
        // keys, so the two-key set on offer is sometimes two junk corridors.
        let apex_one_key = DoorKey { r1_apex: 1, ..base };
        assert!(
            apex_one_key > both_keys,
            "one key on the Apex corridor beats two keys on corridors that are \
             not scarce"
        );
    }

    // R2 sums the components each door joins, so a generic 5+5 merge scores
    // worse than attaching a singleton — no second mechanism needed.
    #[test]
    fn r2_ranks_a_generic_merge_below_a_singleton_attach() {
        let state = board(
            &[],
            &[(B0, C0), (C0, D0), (D0, E0), (D1, E1), (E1, E2)],
        );
        let singleton = verdict(&state, D3, None, &[(D3, C2)]);
        let merge = verdict(&state, D3, None, &[(D3, E2)]);
        assert!(
            singleton.door > merge.door,
            "singleton {:?} should outrank merge {:?}",
            singleton.door,
            merge.door
        );
        assert_eq!(singleton.door.r2, -2, "two singletons");
        assert_eq!(merge.door.r2, -4, "one room into a three-room cluster");
    }

    // RS reads the corridor's **far** end. The room you are standing in is the
    // same for every option, so folding its degree in can only mask the
    // difference — C2 has four lattice neighbours and would cap both of these
    // at -4.
    #[test]
    fn rs_scores_the_far_end_of_the_corridor_not_the_room_you_stand_in() {
        let state = board(&[], &[]);
        let scarce = verdict(&state, C2, None, &[(C2, D3)]);
        let common = verdict(&state, C2, None, &[(C2, D2)]);
        assert_eq!(scarce.door.rs, -3, "D3 has three lattice neighbours");
        assert_eq!(common.door.rs, -6, "D2 has six");
        assert!(
            scarce.door.rs > common.door.rs,
            "the scarcer far end wins: {:?} vs {:?}",
            scarce.door,
            common.door
        );
    }

    // --------------------------------------------------------------- RU ---

    // Below `k` connections every neighbour is hit, so a new door is free.
    #[test]
    fn ru_is_silent_while_a_shrine_has_fewer_connections_than_picks() {
        // Sanctum of Unity II takes two targets and has one connection.
        let state = board(&[(C1, "upgrade", 2), (C0, "gem", 2)], &[(C0, C1)]);
        let verdict = verdict(&state, D1, None, &[(D1, C1)]);
        assert_eq!(verdict.door.ru_ok, 1);
    }

    // At or above `k` a new door turns a certainty into a lottery.
    #[test]
    fn ru_vetoes_a_door_that_pushes_a_shrine_past_its_pick_count() {
        let state = board(&[(C1, "upgrade", 1), (C0, "gem", 2)], &[(C0, C1)]);
        let verdict = verdict(&state, D1, None, &[(D1, C1)]);
        assert_eq!(verdict.door.ru_ok, 0);
        assert!(verdict.reasons.iter().any(|r| matches!(
            r,
            Reason::Ru {
                slot: C1,
                targets: 1,
                connected: 2
            }
        )));
    }

    // The Temple Nexus ignores connections entirely, so nothing dilutes it.
    #[test]
    fn ru_is_silent_for_a_temple_nexus() {
        let state = board(&[(C1, "upgrade", 3), (C0, "gem", 2)], &[(C0, C1)]);
        assert_eq!(verdict(&state, D1, None, &[(D1, C1)]).door.ru_ok, 1);
    }

    // RU must see the room the kill *creates*, not the one it replaces — that
    // is the whole of Case 5.
    #[test]
    fn ru_sees_the_shrine_the_architect_is_about_to_build() {
        // C0 is a Poison Garden with two connected neighbours; changing it
        // yields Sanctum of Unity II, whose two picks are then certainties.
        let state = board(
            &[(C0, "toxic_grove", 1), (C1, "gem", 2), (D0, "junk", 1)],
            &[(C0, C1), (C0, D0)],
        );
        let sanctum = choice(OfferKind::Change, "upgrade", 2);
        assert_eq!(
            verdict(&state, C0, Some(&sanctum), &[(C0, B0)]).door.ru_ok,
            0,
            "a third connection turns 2-of-2 into 2-of-3"
        );
        let junk = choice(OfferKind::Change, "museum_of_artefacts", 2);
        assert_eq!(
            verdict(&state, C0, Some(&junk), &[(C0, B0)]).door.ru_ok,
            1,
            "without the Sanctum there is nothing to dilute"
        );
    }

    // -------------------------------------------- opening nothing, why ---

    // Three different facts share one empty door set, and the overlay must not
    // print "no key dropped" for the other two.
    #[test]
    fn no_key_dropped_is_told_apart_from_a_key_with_nowhere_to_spend_it() {
        // Every closed corridor from B1 leads back into B1's own cluster.
        let boxed_in = board(&[], &[(B1, C1), (C1, C2), (B0, C1), (A0, B0)]);
        let none = verdict_with(&boxed_in, B1, None, &[], 0, &rush());
        assert!(
            none.reasons.contains(&Reason::ZeroKey),
            "no key dropped: {:?}",
            none.reasons
        );
        let one = verdict_with(&boxed_in, B1, None, &[], 1, &rush());
        assert!(
            one.reasons.contains(&Reason::NoUsableDoor),
            "a key fell but every corridor is pointless: {:?}",
            one.reasons
        );
        assert!(!one.reasons.contains(&Reason::ZeroKey));
    }

    // Case 5's reason. A key fell, corridors are on offer, and RU turns every
    // one of them down: opening nothing is a **choice**, not a shortage.
    #[test]
    fn ru_declining_every_corridor_is_named_as_such_not_as_a_missing_key() {
        // C0 changes to Sanctum of Unity II and already has exactly two
        // connected neighbours, so any third corridor turns 2-of-2 into 2-of-3.
        let state = board(
            &[(C0, "toxic_grove", 1), (C1, "gem", 2), (D0, "junk", 1)],
            &[(C0, C1), (C0, D0)],
        );
        let sanctum = choice(OfferKind::Change, "upgrade", 2);
        let verdict = verdict_with(&state, C0, Some(&sanctum), &[], 1, &rush());
        assert!(
            verdict.reasons.contains(&Reason::RuDeclined { slot: C0 }),
            "RU declined the key: {:?}",
            verdict.reasons
        );
        assert!(!verdict.reasons.contains(&Reason::ZeroKey));

        // The same board with a junk kill has nothing to protect, so opening
        // nothing carries no reason of its own — RD is what ranks it last.
        let junk = choice(OfferKind::Change, "museum_of_artefacts", 2);
        let unprotected = verdict_with(&state, C0, Some(&junk), &[], 1, &rush());
        assert!(
            !unprotected
                .reasons
                .iter()
                .any(|r| matches!(r, Reason::RuDeclined { .. } | Reason::ZeroKey)),
            "{:?}",
            unprotected.reasons
        );
    }

    // ---------------------------------------------- the architect rules ---

    // R4's carve-out declines the double-tier roll that could take the
    // Sanctum's only live target out of the pool. PROPOSED — see
    // [`R4_POOL_CARVE_OUT`].
    #[test]
    fn the_carve_out_ranks_change_above_upgrade_beside_a_live_sanctum() {
        let state = board(
            &[(D1, "factory", 1), (C1, "upgrade", 2), (C2, "corruption", 1)],
            &[(C1, D1), (D1, E1)],
        );
        let change = verdict(&state, D1, Some(&choice(OfferKind::Change, "museum_of_artefacts", 2)), &[]);
        let upgrade = verdict(&state, D1, Some(&choice(OfferKind::Upgrade, "factory", 2)), &[]);
        assert!(
            change.architect > upgrade.architect,
            "change {:?} should outrank upgrade {:?}",
            change.architect,
            upgrade.architect
        );
        assert!(change
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::R4PoolCarveOut { upgrader: C1 })));
        assert!(
            upgrade.architect.vetoed(),
            "the carve-out is a veto, not a tie-break: {:?}",
            upgrade.architect
        );
        assert!(
            upgrade
                .reasons
                .iter()
                .any(|r| matches!(r, Reason::R4CarveOutVeto { upgrader: C1 })),
            "a demoted kill must say why: {:?}",
            upgrade.reasons
        );
    }

    // The carve-out is a profile field because §6e leaves R4-vs-staying-in-the-
    // pool explicitly unresolved. With it off, R4 speaks and the veto is gone.
    #[test]
    fn turning_the_carve_out_off_hands_the_slot_back_to_r4() {
        let state = board(
            &[(D1, "factory", 1), (C1, "upgrade", 2), (C2, "corruption", 1)],
            &[(C1, D1), (D1, E1)],
        );
        let mut profile = rush();
        profile.r4_keep_upgrade_targets = false;
        let upgrade = verdict_with(
            &state,
            D1,
            Some(&choice(OfferKind::Upgrade, "factory", 2)),
            &[],
            1,
            &profile,
        );
        let change = verdict_with(
            &state,
            D1,
            Some(&choice(OfferKind::Change, "museum_of_artefacts", 2)),
            &[],
            1,
            &profile,
        );
        assert!(!upgrade.architect.vetoed());
        assert!(upgrade.reasons.iter().any(|r| matches!(r, Reason::R4)));
        assert!(
            !change
                .reasons
                .iter()
                .any(|r| matches!(r, Reason::R4PoolCarveOut { .. })),
            "the carve-out must be silent on both sides when it is off: {:?}",
            change.reasons
        );
        // RC still points at `change` on this board — the carve-out is not what
        // orders the two kills here, the veto is what removes `upgrade` from
        // contention, and only the veto is under test.
    }

    // A kill that builds a target line is never vetoed — an upgrade toward
    // Locus outranks any drop-pool argument.
    #[test]
    fn the_carve_out_never_vetoes_a_kill_that_builds_a_target_line() {
        let state = board(
            &[(D1, "corruption", 1), (C1, "upgrade", 2)],
            &[(C1, D1), (D1, E1)],
        );
        let upgrade = verdict(
            &state,
            D1,
            Some(&choice(OfferKind::Upgrade, "corruption", 2)),
            &[],
        );
        assert!(!upgrade.architect.vetoed(), "{:?}", upgrade.reasons);
    }

    // A shrine whose pick is already a lottery is not a synergy worth spending
    // a drop-pool slot on, so the carve-out stands down and R4 takes over.
    #[test]
    fn the_carve_out_ignores_a_shrine_whose_picks_are_already_diluted() {
        // C1 is a Shrine of Empowerment: one pick, two connected neighbours.
        let state = board(
            &[(D1, "factory", 1), (C1, "upgrade", 1), (C0, "junk", 1), (C2, "corruption", 1)],
            &[(C0, C1), (C1, D1), (D1, E1)],
        );
        let upgrade = verdict(
            &state,
            D1,
            Some(&choice(OfferKind::Upgrade, "factory", 2)),
            &[],
        );
        assert!(!upgrade.architect.vetoed(), "{:?}", upgrade.reasons);
        assert!(upgrade.reasons.iter().any(|r| matches!(r, Reason::R4)));
    }

    // The Temple Nexus lifts every adjacent room whether or not a corridor is
    // open, so adjacency alone keeps the slot worth having in the pool.
    #[test]
    fn a_temple_nexus_carves_the_slot_out_without_an_open_corridor() {
        let state = board(
            &[(D1, "factory", 1), (C1, "upgrade", 3), (C2, "corruption", 1)],
            &[(D1, E1)],
        );
        let upgrade = verdict(
            &state,
            D1,
            Some(&choice(OfferKind::Upgrade, "factory", 2)),
            &[],
        );
        assert!(upgrade.architect.vetoed(), "{:?}", upgrade.reasons);
        assert!(upgrade
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::R4CarveOutVeto { upgrader: C1 })));
    }

    // Without a shrine that can still hit the slot, R4 takes over and points
    // the other way: max the junk room out of the pool.
    #[test]
    fn r4_ranks_upgrade_above_change_when_no_shrine_can_still_hit_the_slot() {
        let state = board(&[(D1, "factory", 1), (C2, "corruption", 1)], &[(D1, E1)]);
        let change = verdict(&state, D1, Some(&choice(OfferKind::Change, "museum_of_artefacts", 2)), &[]);
        let upgrade = verdict(&state, D1, Some(&choice(OfferKind::Upgrade, "factory", 2)), &[]);
        assert!(
            upgrade.architect > change.architect,
            "only the upgrade rolls the double tier that empties the slot"
        );
        assert!(upgrade.reasons.iter().any(|r| matches!(r, Reason::R4)));
    }

    // R4 never applies to a target line — you do not want the corruption room
    // out of the pool for the sake of a smaller pool.
    #[test]
    fn r4_is_silent_on_a_kill_that_builds_a_target_line() {
        let state = board(&[(D1, "corruption", 1)], &[(D1, E1)]);
        let verdict = verdict(&state, D1, Some(&choice(OfferKind::Upgrade, "corruption", 2)), &[]);
        assert!(!verdict.reasons.iter().any(|r| matches!(r, Reason::R4)));
        assert!(verdict
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::AdvancesTarget { .. })));
    }

    // RC needs the shrine to be *connected* at tiers 1–2; adjacency alone is
    // only enough for the Temple Nexus.
    #[test]
    fn rc_needs_an_open_corridor_to_a_shrine_but_not_to_a_nexus() {
        let unconnected = board(&[(D1, "toxic_grove", 1), (C1, "upgrade", 1)], &[(D1, E1)]);
        let change = choice(OfferKind::Change, "museum_of_artefacts", 2);
        assert!(!verdict(&unconnected, D1, Some(&change), &[])
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::Rc { .. })));

        let nexus = board(&[(D1, "toxic_grove", 1), (C1, "upgrade", 3)], &[(D1, E1)]);
        assert!(verdict(&nexus, D1, Some(&change), &[])
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::Rc { upgrader: C1 })));
    }

    // ------------------------------------------------------ the margin ----

    // The band must never collapse to zero, or the chain stops breaking genuine
    // coin flips at high rollout counts.
    #[test]
    fn the_noise_margin_never_falls_below_its_floor() {
        assert_eq!(noise_margin(0.0), NOISE_FLOOR);
        assert!(noise_margin(1.0) > NOISE_FLOOR);
        assert!(noise_margin(0.15) > noise_margin(0.05));
    }
}
