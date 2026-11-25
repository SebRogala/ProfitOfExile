//! Monte-Carlo rollout over the random remainder (POE-170).
//!
//! Ported from `temple_model.py` (`resolve_upgrades`, `reachable_with_charges`,
//! `score`, `drop_pool`, `_offer`, `_apply_kill`, `_default_policy_door`,
//! `rollout`). The decision is one step under uncertainty — everything after
//! the current kill and key is *drawn*, not chosen — so an option is priced by
//! playing the rest of the temple out many times under a default policy and
//! averaging the finished board's score.
//!
//! # Why a second board representation
//!
//! [`Sim`] holds the board as bitmasks and small integers. A single `advise`
//! call runs tens of thousands of rollouts, each re-walking the component graph
//! several times per incursion; `BTreeSet<Edge>` and `Line::Other(String)`
//! would dominate the wall time. [`Valuation`] is the translation table
//! between the two, built once per call.
//!
//! # Prototype bugs deliberately not ported
//!
//! - `drop_pool()` included the Entrance. It is not a drop room.
//! - `visited_this_map` modelled whole-map no-revisit. Only the **next**
//!   incursion is guaranteed to differ.
//! - `_offer()` excluded the current line from the 25-line draw by value, so on
//!   a tier-0 room (line `None`) it excluded all 21 unnamed lines and offered
//!   one of the four mechanical lines with certainty. That inflated valuable-
//!   line acquisition on exactly the slots rollouts land on most.

use crate::temple::facts;
use crate::temple::rooms::OfferKind;
use crate::temple::lattice::Slot;
use crate::temple::strategy::{
    Line, Mode, ModeRule, StrategyProfile, TempleConfig, Tier, SCARAB_OF_TIMELINES_ACTIVE,
};

use super::state::{
    bits, component, hop_distances, closed_door_distances, mask_holds, mask_of, neighbour_masks,
    BoardState, SlotMask, UNREACHABLE,
};

/// How many rooms the upgrade line hits at each tier.
///
/// Shrine of Empowerment I upgrades one random adjacent **and connected** room,
/// Sanctum of Unity II two, Temple Nexus III **all adjacent rooms regardless of
/// connections** — which is why the tier-3 entry is not a count but "everything
/// in the pool", and why `UPGRADE_NEEDS_DOOR` is false there.
const UPGRADE_TARGETS: [usize; 4] = [0, 1, 2, usize::MAX];
/// Whether the upgrade line's pick at each tier is restricted to *connected*
/// neighbours.
const UPGRADE_NEEDS_DOOR: [bool; 4] = [false, true, true, false];

/// Explosive charges granted by each tier of the explosives line.
///
/// Tier 1 (Explosives Room, "one") is measured from the room's own text. Tiers
/// 2 and 3 read "several" and are **UNVERIFIED** — 2 and 3 are the prototype's
/// guess and are carried forward unchanged so the number has one home to fix.
const CHARGES: [u32; 4] = [0, 1, 2, 3];

/// Room lines the game can offer, total. `rooms::LINES` holds exactly 25.
const TOTAL_LINES: usize = 25;

/// The four lines with mechanics, in the order [`Valuation`] indexes them.
const MECHANICAL: [Line; 4] = [Line::Corruption, Line::Gem, Line::Upgrade, Line::Explosive];

// ------------------------------------------------------------------- rng ----

/// Seeded splitmix64. Deterministic, dependency-free, and good enough for a
/// rollout average — the advisor needs reproducibility, not cryptography.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// A stream from a seed. Two calls with the same seed produce the same
    /// advice, which is what makes the tests table-driven.
    pub fn seeded(seed: u64) -> Rng {
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform value in `0..n`. Returns 0 for an empty range.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

}

// ------------------------------------------------------------- valuation ----

/// The translation table between [`Line`] and the rollout's compact line tags.
///
/// Slot `0` of a [`Sim`]'s line table means "no line at all"; tag `n` means
/// `lines[n - 1]`. Everything the profile does not price and that carries no
/// mechanics collapses to [`Valuation::JUNK`], which is what the prototype's
/// `None` line meant.
#[derive(Debug, Clone)]
pub struct Valuation {
    /// Tag `n` ↔ `lines[n - 1]`. Always starts with the four mechanical lines.
    pub lines: Vec<Line>,
    /// The lines this strategy is chasing — RV's "valuable", the mode
    /// selector's requirement, and the source of [`Self::lost_threshold`].
    pub targets: Vec<u8>,
    /// Score below which a rollout counts as having **lost** the valuable room,
    /// used for the risk figure on an RV gamble.
    pub lost_threshold: f64,
}

impl Valuation {
    /// The tag for a line with no mechanics and no price.
    pub const JUNK: u8 = u8::MAX;

    /// Tag of the corruption line.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub const CORRUPTION: u8 = 1;
    /// Tag of the gem line.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub const GEM: u8 = 2;
    /// Tag of the upgrade line.
    pub const UPGRADE: u8 = 3;
    /// Tag of the explosives line.
    pub const EXPLOSIVE: u8 = 4;

    /// Build the table for one profile.
    ///
    /// The target lines are read off [`ModeRule::LinesConnected`]: a profile
    /// already declares there which lines it is farming, and RV, R5 and the
    /// mode switch all mean the same set. A second field naming the same thing
    /// could drift out of step with the mode rule and silently make RV protect
    /// a room the strategy no longer wants.
    pub fn for_profile(profile: &StrategyProfile) -> Valuation {
        let mut lines: Vec<Line> = MECHANICAL.to_vec();
        for line in profile.room_values.keys() {
            if !lines.contains(line) {
                lines.push(line.clone());
            }
        }
        for combination in &profile.combinations {
            for (line, _) in &combination.requires {
                if !lines.contains(line) {
                    lines.push(line.clone());
                }
            }
        }
        let ModeRule::LinesConnected(required) = &profile.mode_rule;
        let targets: Vec<u8> = required
            .iter()
            .filter_map(|line| lines.iter().position(|l| l == line))
            .map(|i| i as u8 + 1)
            .collect();

        // The lowest POSITIVE tier-3 value among the lines `mode_rule` names
        // as targets — the cheapest single-target outcome that still counts as
        // one.
        //
        // The prototype hard-coded 7.0, the rusher fixture's score for
        // "Doryani alone" on Sebastian's 1-10 ranking. Since POE-257 the
        // profile the app actually ranks with is denominated in CHAOS, so the
        // same expression yields 390 on the 2026-09-06 capture (Doryani's
        // Institute's sale delta) and 800 on a cold read (its A2 tier rung). No
        // rescale is applied or needed: this is DERIVED from `room_values`, so
        // it is already in whatever unit those are — which is why
        // `StrategyProfile::value_scale`'s table does not list it.
        let lost_threshold = required
            .iter()
            .filter_map(|line| profile.room_values.get(line))
            .map(|values| values[2])
            .filter(|v| *v > 0.0)
            .fold(f64::INFINITY, f64::min);
        let lost_threshold = if lost_threshold.is_finite() {
            lost_threshold
        } else {
            0.0
        };

        Valuation {
            lines,
            targets,
            lost_threshold,
        }
    }

    /// The tag for a line, or [`Self::JUNK`].
    pub fn tag(&self, line: &Line) -> u8 {
        match self.lines.iter().position(|l| l == line) {
            Some(i) => i as u8 + 1,
            None => Valuation::JUNK,
        }
    }

    /// The [`Line`] behind a tag, or `None` for junk.
    pub fn line(&self, tag: u8) -> Option<&Line> {
        if tag == Valuation::JUNK || tag == 0 {
            None
        } else {
            self.lines.get(tag as usize - 1)
        }
    }

    /// Whether a tag is one of the strategy's target lines.
    pub fn is_target(&self, tag: u8) -> bool {
        self.targets.contains(&tag)
    }
}

// -------------------------------------------------------------- the board ---

/// The board in the form the rollout runs on.
#[derive(Debug, Clone)]
pub struct Sim {
    /// Line tag per slot; `0` means no line (Entrance, Apex, filler).
    pub line: [u8; 13],
    /// Tier per slot, `0..=3`.
    pub tier: [u8; 13],
    /// Open-door adjacency masks.
    pub open: [SlotMask; 13],
    /// Incursions left in the whole temple budget.
    pub remaining: u8,
    /// The slot the previous incursion of this map used, if any.
    pub last_visited: Option<usize>,
    /// How many entrances of the current map have been spent.
    pub incursion_in_map: u8,
}

/// The move being priced: the kill and the keys spent this incursion.
#[derive(Debug, Clone, PartialEq)]
pub struct Opening {
    /// Where the player is standing.
    pub slot: usize,
    /// `(line tag, kind, resolved built tier)`, or `None` for an unread offer.
    pub kill: Option<(u8, OfferKind, Tier)>,
    /// The corridors the keys open, as slot index pairs.
    pub doors: Vec<(usize, usize)>,
}

const APEX: usize = 0;
const ENTRANCE: usize = 11;

impl Sim {
    /// Translate a [`BoardState`].
    pub fn from_board(board: &BoardState, valuation: &Valuation) -> Sim {
        debug_assert_eq!(Slot::APEX.index(), APEX);
        debug_assert_eq!(Slot::ENTRANCE.index(), ENTRANCE);
        let mut line = [0u8; 13];
        let mut tier = [0u8; 13];
        for i in 0..13 {
            let (room_line, room_tier) = &board.rooms[i];
            tier[i] = room_tier.get();
            if let Some(room_line) = room_line {
                line[i] = valuation.tag(room_line);
            }
        }
        Sim {
            line,
            tier,
            open: board.adjacency(),
            remaining: board.remaining,
            last_visited: board.last_visited.map(|s| s.index()),
            incursion_in_map: 0,
        }
    }

    /// Open the corridor between two slots.
    pub fn open_door(&mut self, a: usize, b: usize) {
        self.open[a] |= 1 << b;
        self.open[b] |= 1 << a;
    }

    fn has_door(&self, a: usize, b: usize) -> bool {
        self.open[a] & (1 << b) != 0
    }

    /// The upgrade line's resolution pass: a room's **final** tier is not its
    /// built tier.
    ///
    /// Targets are drawn from the room's lattice neighbours, **excluding the
    /// Apex** (Sebastian: *"tier-0 is different from the apex, apex doesn't
    /// count"*) but **including tier-0 rooms, on which the pick is simply
    /// wasted** (*"it could choose a t0 room, so nothing happens"*). Excluding
    /// the wasted picks would understate dilution and overstate the odds on a
    /// real target, which is the whole of RU.
    ///
    /// **UNVERIFIED:** whether upgrade rooms chain. This reads each pick's
    /// tier from the partially-resolved table, so an earlier upgrade room's
    /// result can feed a later one — the prototype's behaviour, carried over
    /// because no measurement exists either way.
    pub fn resolve_upgrades(&self, rng: &mut Rng) -> [u8; 13] {
        let mut out = self.tier;
        for (s, adjacent) in neighbour_masks().iter().enumerate() {
            if self.line[s] != Valuation::UPGRADE || self.tier[s] == 0 {
                continue;
            }
            let t = self.tier[s] as usize;
            let mut pool: Vec<usize> = bits(*adjacent)
                .filter(|n| *n != APEX)
                .filter(|n| !UPGRADE_NEEDS_DOOR[t] || self.has_door(s, *n))
                .collect();
            let k = UPGRADE_TARGETS[t];
            if k < pool.len() {
                // Sample without replacement: k random draws off the front.
                for i in 0..k {
                    let j = i + rng.below(pool.len() - i);
                    pool.swap(i, j);
                }
                pool.truncate(k);
            }
            for p in pool {
                if out[p] == 0 {
                    continue; // wasted pick
                }
                out[p] = (out[p] + 1).min(3);
            }
        }
        out
    }

    /// Everything the Entrance reaches through open doors.
    pub fn natural_reach(&self) -> SlotMask {
        component(&self.open, Slot::ENTRANCE)
    }

    /// RE — the Entrance component relaxed by the explosive charges the temple
    /// will actually hold.
    ///
    /// Charges only count from an explosives room that is itself **built and
    /// connected**: the player has to walk to it before blasting anything.
    pub fn charge_reach(&self, final_tier: &[u8; 13], natural: SlotMask) -> SlotMask {
        let charges: u32 = (0..13)
            .filter(|s| {
                self.line[*s] == Valuation::EXPLOSIVE
                    && final_tier[*s] > 0
                    && mask_holds(natural, *s)
            })
            .map(|s| CHARGES[final_tier[s] as usize])
            .sum();
        if charges == 0 {
            return natural;
        }
        let distance = closed_door_distances(&self.open, natural);
        let mut reach = natural;
        for (i, d) in distance.iter().enumerate() {
            if *d != UNREACHABLE && *d as u32 <= charges {
                reach |= 1 << i;
            }
        }
        reach
    }

    /// Score the finished temple.
    pub fn score(&self, rng: &mut Rng, profile: &StrategyProfile, valuation: &Valuation) -> f64 {
        let final_tier = self.resolve_upgrades(rng);
        let natural = self.natural_reach();
        let blasted = self.charge_reach(&final_tier, natural);
        let base = self.table(profile, valuation, &final_tier, natural);
        if blasted == natural {
            return base;
        }
        let with_charges = self.table(profile, valuation, &final_tier, blasted);
        profile.blend_blast(base, with_charges)
    }

    fn table(
        &self,
        profile: &StrategyProfile,
        valuation: &Valuation,
        final_tier: &[u8; 13],
        reach: SlotMask,
    ) -> f64 {
        let mut reached: Vec<(Line, Tier)> = Vec::new();
        let mut built = 0usize;
        for i in bits(reach) {
            let t = final_tier[i];
            if t == 0 {
                continue;
            }
            if i != ENTRANCE {
                built += 1;
            }
            if let Some(line) = valuation.line(self.line[i]) {
                reached.push((line.clone(), Tier::new(t).unwrap_or(Tier::T3)));
            }
        }
        let hops = self.route_hops(profile, valuation, final_tier, reach);
        profile.aggregate_with_path(&reached, mask_holds(reach, APEX), built, hops)
    }

    /// `path_cost`'s hop count: how far the player has to walk to reach what
    /// they came for.
    ///
    /// **ASSUMED, and inert in every shipped profile** — `path_cost` is 0 in
    /// the Locus/Doryani Rush, so nothing here has ever been measured against
    /// an outcome. The definition follows Vertolka's rationale (*"connect the
    /// rest with the farming route in mind — better to spend time building than
    /// running to dead ends"*): the cost of a temple is the walk to the
    /// farthest thing worth walking to, which is the Apex plus every room the
    /// profile prices above zero.
    fn route_hops(
        &self,
        profile: &StrategyProfile,
        valuation: &Valuation,
        final_tier: &[u8; 13],
        reach: SlotMask,
    ) -> usize {
        if profile.path_cost == 0.0 {
            return 0;
        }
        let distance = hop_distances(&self.open, Slot::ENTRANCE);
        let mut worst = 0;
        for i in bits(reach) {
            let priced = i == APEX
                || valuation
                    .line(self.line[i])
                    .and_then(|line| profile.room_values.get(line))
                    .map(|values| final_tier[i] > 0 && values[final_tier[i] as usize - 1] > 0.0)
                    .unwrap_or(false);
            if !priced {
                continue;
            }
            // `reach` is charge-relaxed, so a slot in it may have no open path
            // at all — it is blasted into, and has no hop count. Skipping it is
            // **equivalent to the `unwrap_or(0)` this replaced** while the term
            // is a maximum, so nothing about the score moves; what changes is
            // that the code no longer states that a room the player cannot walk
            // to sits at distance 0. It does not, and the moment this term
            // becomes a sum or a mean that claim would price the unreachable
            // room as the nearest thing on the board. What a blast-only room
            // *should* cost the walk is unmeasured — `path_cost` is 0 in every
            // shipped profile — so it is excluded rather than given an invented
            // distance; the blast itself is already priced by `blast_discount`.
            if let Some(hops) = distance[i] {
                worst = worst.max(hops);
            }
        }
        worst
    }

    /// The slots the next incursion can drop the player into.
    ///
    /// Never the Apex, never the Entrance, never a tier-3 room (it has left the
    /// pool for good), and never the slot the previous incursion of this map
    /// used.
    pub fn drop_pool(&self) -> Vec<usize> {
        (0..13)
            .filter(|s| *s != APEX && *s != ENTRANCE)
            .filter(|s| self.tier[*s] < 3)
            .filter(|s| Some(*s) != self.last_visited)
            .collect()
    }

    /// The change architect's offered line.
    ///
    /// Uniform over the game's [`TOTAL_LINES`] lines; everything past the ones
    /// [`Valuation`] names collapses to junk. The current line is excluded when
    /// it is a named one — the game never offers a change to the room you are
    /// already standing in.
    fn offered_change(&self, slot: usize, valuation: &Valuation, rng: &mut Rng) -> u8 {
        let named = valuation.lines.len();
        for _ in 0..8 {
            let draw = rng.below(TOTAL_LINES);
            let tag = if draw < named {
                draw as u8 + 1
            } else {
                Valuation::JUNK
            };
            if tag != self.line[slot] || tag == Valuation::JUNK {
                return tag;
            }
        }
        Valuation::JUNK
    }

    /// Apply a future kill through [`facts::landing_tier`].
    pub fn apply_kill(
        &mut self,
        slot: usize,
        line: u8,
        kind: OfferKind,
        valuation: &Valuation,
        config: &TempleConfig,
    ) {
        let current = Tier::new(self.tier[slot]).expect("sim tiers are bounded");
        let built = facts::landing_tier(
            config.played_consolidation(),
            Some(current),
            kind,
            valuation.line(line).map(Line::key),
        ).expect("the sim always knows the current tier");
        self.line[slot] = line;
        self.tier[slot] = built.get();
    }

    /// Apply the panel's resolved opening; unread rooms use T0 in the sim,
    /// so deriving this tier again would disagree with the panel.
    pub fn open(&mut self, opening: &Opening) {
        if let Some((line, _, tier)) = opening.kill {
            self.line[opening.slot] = line;
            self.tier[opening.slot] = tier.get();
        }
        for (a, b) in &opening.doors {
            self.open_door(*a, *b);
        }
    }

    /// The stand-in for a human on every future turn: connect the most
    /// valuable thing, else nothing.
    ///
    /// Ported whole, including the RU dilution guard and the strictly-positive
    /// gain floor — a door that grows the Entrance component by nothing is
    /// never taken, which is the prototype's pointless-door exclusion in its
    /// cheapest form.
    pub fn default_policy_door(&self, slot: usize) -> Option<usize> {
        let neighbours = neighbour_masks();
        let before = self.natural_reach().count_ones() as f64;
        let mut best = None;
        let mut best_gain = 0.0;
        for n in bits(neighbours[slot]) {
            if self.has_door(slot, n) {
                continue;
            }
            let mut trial = self.clone();
            trial.open_door(slot, n);
            let mut gain = trial.natural_reach().count_ones() as f64 - before;
            let dilutes = [slot, n].iter().any(|m| {
                self.line[*m] == Valuation::UPGRADE && (self.tier[*m] == 1 || self.tier[*m] == 2)
            });
            if dilutes {
                gain -= 1.5;
            }
            if gain > best_gain {
                best = Some(n);
                best_gain = gain;
            }
        }
        best
    }

    /// Book one incursion: spend a point of the temple budget, remember the
    /// slot so the next draw differs, and roll the map over once its entrances
    /// are used up.
    ///
    /// The map boundary is what clears [`Self::last_visited`]: the "next
    /// incursion differs" guarantee is scoped to a map, so a new map makes the
    /// slot drawable again.
    pub fn spend_incursion(&mut self, slot: usize, entrances: u8) {
        self.remaining = self.remaining.saturating_sub(1);
        self.last_visited = Some(slot);
        self.incursion_in_map += 1;
        if self.incursion_in_map >= entrances {
            self.incursion_in_map = 0;
            self.last_visited = None;
        }
    }

    /// Apply the resolved opening, then simulate the rest of the temple.
    pub fn rollout_from(
        mut self,
        opening: &Opening,
        profile: &StrategyProfile,
        valuation: &Valuation,
        config: &TempleConfig,
        rng: &mut Rng,
    ) -> f64 {
        self.open(opening);
        // **ASSUMED:** the incursion being priced is the first of its map. The
        // panel prints the temple's remaining budget, never the map's, so the
        // true index is not readable. The cost of being wrong is that the
        // rollout's map boundaries sit up to `entrances - 1` incursions late,
        // which only shifts when a slot becomes drawable again.
        self.incursion_in_map = 0;
        self.spend_incursion(opening.slot, config.entrances_per_map());
        self.rollout(profile, valuation, config, rng)
    }

    /// Play the rest of the temple out once and score the result.
    pub fn rollout(
        mut self,
        profile: &StrategyProfile,
        valuation: &Valuation,
        config: &TempleConfig,
        rng: &mut Rng,
    ) -> f64 {
        let entrances = config.entrances_per_map();
        while self.remaining > 0 {
            let pool = self.drop_pool();
            if pool.is_empty() {
                break;
            }
            let slot = pool[rng.below(pool.len())];

            let upgrade_line = (self.tier[slot] > 0).then_some(self.line[slot]);
            let change_line = self.offered_change(slot, valuation, rng);

            // Prefer whichever offer advances a target line. The upgrade
            // wins a tie: facts::landing_tier never lands it below the change.
            let target = [upgrade_line, Some(change_line)]
                .into_iter()
                .flatten()
                .find(|tag| valuation.is_target(*tag));
            let near_shrine = bits(neighbour_masks()[slot]).any(|n| {
                self.line[n] == Valuation::UPGRADE && self.tier[n] > 0 && self.has_door(slot, n)
            });

            let current = Tier::new(self.tier[slot]).expect("sim tiers are bounded");
            let change_tier = facts::landing_tier(
                config.played_consolidation(), Some(current), OfferKind::Change,
                valuation.line(change_line).map(Line::key),
            ).expect("the sim knows the current tier");
            let paired_upgrade = upgrade_line.and_then(|line| facts::landing_tier(
                config.played_consolidation(), Some(current), OfferKind::Upgrade,
                valuation.line(line).map(Line::key),
            ));
            let rc = near_shrine && paired_upgrade.is_some_and(|upgrade| {
                (change_tier.get() + 1).min(Tier::MAX_VALUE) >= upgrade.get()
            });

            if let Some(target) = target {
                let kind = if upgrade_line == Some(target) {
                    OfferKind::Upgrade
                } else {
                    OfferKind::Change
                };
                self.apply_kill(slot, target, kind, valuation, config);
            } else if rc {
                // RC — spend the architect on the line and let the shrine
                // supply the tier, so the shrine's scarce pick is not wasted.
                self.apply_kill(slot, change_line, OfferKind::Change, valuation, config);
            } else if let Some(line) = upgrade_line {
                // R4 — max the junk room, so it leaves the drop pool.
                self.apply_kill(slot, line, OfferKind::Upgrade, valuation, config);
            } else {
                self.apply_kill(slot, change_line, OfferKind::Change, valuation, config);
            }

            if let Some(door) = self.default_policy_door(slot) {
                self.open_door(slot, door);
            }

            self.spend_incursion(slot, entrances);
        }
        self.score(rng, profile, valuation)
    }
}

/// What `n` rollouts of one already-applied board say about it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    /// Mean finished-temple score — the EV the ranking sorts on.
    pub mean: f64,
    /// Fraction of rollouts that finished below
    /// [`Valuation::lost_threshold`] — the number Sebastian asked to see on an
    /// RV gamble rather than have the advisor hide the option.
    pub risk: f64,
    /// Standard error of [`Self::mean`]. The rule layer widens its noise
    /// margin to this, so the priority chain is not silently disabled by
    /// sampling noise at the small rollout counts an overlay can afford.
    pub stderr: f64,
}

/// Run `n` rollouts of one option.
///
/// Every option is evaluated from the **same** seed. That is common random
/// numbers, deliberately: the ranking cares about the *difference* between two
/// options, and sharing the draws removes most of the sampling noise from that
/// difference at no cost in bias.
pub fn evaluate(
    sim: &Sim,
    opening: &Opening,
    profile: &StrategyProfile,
    valuation: &Valuation,
    config: &TempleConfig,
    n: u32,
    rng: &mut Rng,
) -> Estimate {
    if n == 0 {
        return Estimate {
            mean: 0.0,
            risk: 0.0,
            stderr: 0.0,
        };
    }
    let mut total = 0.0;
    let mut total_sq = 0.0;
    let mut lost = 0u32;
    for _ in 0..n {
        let value = sim
            .clone()
            .rollout_from(opening, profile, valuation, config, rng);
        total += value;
        total_sq += value * value;
        if value < valuation.lost_threshold {
            lost += 1;
        }
    }
    let n_f = n as f64;
    let mean = total / n_f;
    let variance = (total_sq / n_f - mean * mean).max(0.0);
    Estimate {
        mean,
        risk: lost as f64 / n_f,
        stderr: (variance / n_f).sqrt(),
    }
}

/// The board's effective [`Mode`], from the profile's own selector — always
/// [`Mode::Chase`] while [`SCARAB_OF_TIMELINES_ACTIVE`] is false, because the
/// Scarab mode recommends running an item 3.30 removed (POE-377).
pub fn mode_of(board: &BoardState, profile: &StrategyProfile) -> Mode {
    if SCARAB_OF_TIMELINES_ACTIVE {
        profile.select_mode(&board.connected_rooms())
    } else {
        Mode::Chase
    }
}

/// The slot mask of one slot — re-exported so the rules layer does not have to
/// reach past this module for it.
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub fn slot_mask(slot: Slot) -> SlotMask {
    mask_of(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::lattice::Slot::*;
    use crate::temple::strategy::Line;

    use super::super::state::{mask_of, BoardState};

    fn rush() -> StrategyProfile {
        StrategyProfile::locus_doryani_rush()
    }

    fn valuation() -> Valuation {
        Valuation::for_profile(&rush())
    }

    fn sim(rooms: &[(Slot, &str, u8)], doors: &[(Slot, Slot)]) -> Sim {
        let mut board = BoardState::empty();
        for (slot, key, tier) in rooms {
            let line = (!key.is_empty()).then(|| Line::named(key));
            board.set_room(*slot, line, Tier::new(*tier).expect("0..=3"));
        }
        board.doors = doors
            .iter()
            .map(|(a, b)| crate::temple::lattice::Edge::new(*a, *b))
            .collect();
        Sim::from_board(&board, &valuation())
    }

    mod keep_happy {
        use super::*;

        // Whole-map no-revisit: every slot an incursion used stays out of the
        // drop pool until the map's last entrance is spent, then all of them
        // return together. Tabled over the map index, so the second map starts
        // from a clean mask rather than one the first map left behind.
        #[test]
        fn the_last_incursion_of_a_map_clears_the_no_repeat_guarantee() {
            let entrances = TempleConfig::default().entrances_per_map();
            let maps: [[Slot; 4]; 2] = [[D1, D2, D3, C1], [D2, C0, D1, C2]];
            let mut sim = sim(&[], &[]);
            sim.remaining = 9;
            for (map, visits) in maps.iter().enumerate() {
                let visits = &visits[..entrances as usize];
                for (k, slot) in visits[..visits.len() - 1].iter().enumerate() {
                    sim.spend_incursion(slot.index(), entrances);
                    let pool = sim.drop_pool();
                    for visited in &visits[..=k] {
                        assert!(
                            !pool.contains(&visited.index()),
                            "map {map}, after incursion {k}: {visited:?} must stay blocked, pool {pool:?}"
                        );
                    }
                }
                sim.spend_incursion(visits[visits.len() - 1].index(), entrances);
                let pool = sim.drop_pool();
                for visited in visits {
                    assert!(
                        pool.contains(&visited.index()),
                        "map {map} rolled over: {visited:?} must return, pool {pool:?}"
                    );
                }
            }
            assert_eq!(sim.remaining, 9 - 2 * entrances);
        }

        // Both architects of a tier-0 room offer a change, so the policy kills
        // for a target line that only the second offer names. Seed 46 draws D1
        // (the only drop slot), then a junk change line, then Corruption.
        #[test]
        fn a_tier_zero_room_kills_for_a_target_line_only_its_second_architect_offers() {
            let mut profile = rush();
            profile.room_values.insert(Line::Corruption, [5.0, 0.0, 9.0]);
            let valuation = Valuation::for_profile(&profile);
            let others: Vec<(Slot, &str, u8)> = Slot::ALL
                .iter()
                .filter(|s| ![Slot::APEX, Slot::ENTRANCE, D1].contains(*s))
                .map(|s| (*s, "junk", 3))
                .collect();
            let mut board = sim(&others, &[(D1, E1)]);
            board.remaining = 1;
            assert_eq!(board.drop_pool(), vec![D1.index()], "D1 is the only draw");

            let score = board.rollout(&profile, &valuation, &TempleConfig::default(), &mut Rng::seeded(46));

            // Corruption at T1 on D1 pays 5.0; D1 and the junk room the policy
            // door connects pay 0.05 each. A junk kill on D1 would score 0.1.
            assert!((score - 5.1).abs() < 1e-9, "D1 must hold Corruption T1, score {score}");
        }
    }

    mod keep_critical {
        use super::*;
        use crate::temple::rooms::CRADLE_LINE_KEY;
        use crate::temple::strategy::Consolidation;

        // Silent consequence: rollout prices tiers the panel never shows.
        #[test]
        fn future_kills_use_literal_landing_tiers_for_every_seed() {
            use Consolidation::*;
            use OfferKind::*;
            let mut profile = rush();
            profile.room_values.insert(Line::named(CRADLE_LINE_KEY), [0.0, 0.0, 9.0]);
            let valuation = Valuation::for_profile(&profile);
            let cradle = valuation.tag(&Line::named(CRADLE_LINE_KEY));
            let rows = [
                (NonResidentExtraTier, 0, 1, 1),
                (NonResidentExtraTier, 1, 3, 1),
                (NonResidentExtraTier, 2, 3, 1),
                (ResidentCarry, 0, 1, 1),
                (ResidentCarry, 1, 2, 2),
                (ResidentCarry, 2, 3, 3),
                (NotTaken, 0, 1, 1),
                (NotTaken, 1, 2, 1),
                (NotTaken, 2, 3, 1),
            ];
            for seed in 0..64 {
                let mut rng = Rng::seeded(seed);
                let slot = rng.below(13);
                for (option, current, upgrade, change) in rows {
                    let config = TempleConfig { consolidation: Some(option), ..TempleConfig::default() };
                    for (kind, expected) in [(Upgrade, upgrade), (Change, change)] {
                        for (line, want) in [(Valuation::CORRUPTION, expected), (cradle, 3)] {
                            let mut sim = sim(&[], &[]);
                            sim.tier[slot] = current;
                            sim.apply_kill(slot, line, kind, &valuation, &config);
                            assert_eq!(sim.tier[slot], want,
                                "seed {seed}, {option:?}, tier {current}, {kind:?}, tag {line}");
                            assert_eq!(sim.line[slot], line);
                        }
                    }
                }
            }
        }

        // Silent consequence: the unread room's T0 stand-in underprices the
        // opening, despite the panel resolving the upgrade to T3.
        #[test]
        fn the_opening_uses_the_panels_resolved_upgrade_tier() {
            let profile = rush();
            let valuation = Valuation::for_profile(&profile);
            let config = TempleConfig::default();
            let mut board = BoardState::empty();
            board.position = Some(D1);
            board.current_tier = None;
            let offer = crate::temple::panel::ArchitectOffer {
                architect_name: "Quipolatl".into(),
                printed_target: "Armoury".into(),
                target: crate::temple::rooms::match_room_name("Armoury"),
                kind: OfferKind::Upgrade,
                rect: None,
            };
            let choices = crate::temple::advisor::resolve_architects(
                &[offer], None, &config, &mut Vec::new(),
            );
            assert_eq!(choices[0].built_tier, Tier::T3);
            let opening = crate::temple::advisor::opening_for(
                D1, Some(&choices[0]), &std::collections::BTreeSet::new(), &valuation,
            );
            let mut sim = Sim::from_board(&board, &valuation);
            sim.open(&opening);
            assert_eq!(sim.tier[D1.index()], 3);
            assert_eq!(sim.tier[D1.index()], choices[0].built_tier.get());
        }
    }

    // -------------------------------------------------------- drop pool --

    // The Entrance is not a drop room. The prototype's `drop_pool()` included
    // it, which put an incursion's kill on a slot that has no architects.
    #[test]
    fn the_entrance_is_never_in_the_drop_pool() {
        let pool = sim(&[], &[]).drop_pool();
        assert!(!pool.contains(&Slot::ENTRANCE.index()));
    }

    // The Apex is never a drop room either — which is why its two corridors can
    // only ever be opened from B0 or B1, and therefore why R1-apex exists.
    #[test]
    fn the_apex_is_never_in_the_drop_pool() {
        let pool = sim(&[], &[]).drop_pool();
        assert!(!pool.contains(&Slot::APEX.index()));
    }

    // A tier-3 room has left the pool for good. This is the mechanic R4 spends
    // architects on.
    #[test]
    fn a_tier_three_room_has_left_the_drop_pool() {
        let pool = sim(&[(D2, "junk", 3), (D3, "junk", 2)], &[]).drop_pool();
        assert!(!pool.contains(&D2.index()));
        assert!(pool.contains(&D3.index()), "tier 2 is still drawable");
    }

    // The slot an incursion used leaves the drop pool for the next draw of the
    // same map.
    #[test]
    fn the_slot_the_previous_incursion_used_is_not_drawn_next() {
        let entrances = TempleConfig::default().entrances_per_map();
        let mut sim = sim(&[], &[]);
        sim.remaining = 9;
        sim.spend_incursion(D2.index(), entrances);
        let pool = sim.drop_pool();
        assert!(!pool.contains(&D2.index()));
        assert!(pool.contains(&D3.index()));
    }

    // Without Artefacts of the Vaal a map is three entrances, so the boundary
    // arrives sooner. The config flag must actually reach the rollout.
    #[test]
    fn the_map_length_follows_the_artefacts_of_the_vaal_flag() {
        let three = TempleConfig {
            artefacts_of_the_vaal: false,
            scarab_of_timelines: false,
            consolidation: None,
        }
        .entrances_per_map();
        let visits = [D1.index(), D2.index(), D3.index()];
        let mut sim = sim(&[], &[]);
        sim.remaining = 9;
        for slot in &visits[..2] {
            sim.spend_incursion(*slot, three);
        }
        let pool = sim.drop_pool();
        assert!(
            !pool.contains(&visits[0]) && !pool.contains(&visits[1]),
            "mid-map both visited slots stay out of the pool: {pool:?}"
        );
        sim.spend_incursion(visits[2], three);
        let pool = sim.drop_pool();
        for slot in visits {
            assert!(pool.contains(&slot), "slot {slot} returns after the third incursion: {pool:?}");
        }
    }

    // ------------------------------------------------- upgrade resolution --

    // The Apex is excluded from an upgrade room's target pool outright —
    // Sebastian: "tier-0 is different from the apex, apex doesn't count".
    #[test]
    fn an_upgrade_room_never_targets_the_apex() {
        // A Temple Nexus III at B0 hits every adjacent room regardless of doors.
        let sim = sim(&[(B0, "upgrade", 3), (A0, "junk", 1), (C0, "junk", 1)], &[]);
        for seed in 0..30u64 {
            let mut rng = Rng::seeded(seed);
            let final_tier = sim.resolve_upgrades(&mut rng);
            assert_eq!(final_tier[A0.index()], 1, "seed {seed} lifted the Apex");
            assert_eq!(final_tier[C0.index()], 2);
        }
    }

    // Tier-0 rooms stay in the pool and the pick is simply wasted on them —
    // excluding them would understate dilution and overstate the odds on a real
    // target, which is the whole of RU.
    #[test]
    fn an_upgrade_rooms_pick_can_be_wasted_on_a_tier_zero_neighbour() {
        // Shrine of Empowerment I at C1 with two connected neighbours, one of
        // them empty. One pick, so the real target is hit about half the time.
        let sim = sim(
            &[(C1, "upgrade", 1), (C0, "gem", 2), (C2, "", 0)],
            &[(C0, C1), (C1, C2)],
        );
        let lifted = (0..200u64)
            .filter(|seed| {
                let mut rng = Rng::seeded(*seed);
                sim.resolve_upgrades(&mut rng)[C0.index()] == 3
            })
            .count();
        assert!(
            (60..140).contains(&lifted),
            "the empty neighbour must dilute the pick, got {lifted}/200"
        );
    }

    // Tiers 1 and 2 need an open corridor. Counted over many seeds rather than
    // asserted on one, because C1 has six lattice neighbours and a shrine that
    // ignored corridors would still miss the target most of the time.
    #[test]
    fn a_shrine_never_reaches_a_neighbour_it_has_no_open_corridor_to() {
        let shrine = sim(&[(C1, "upgrade", 1), (C0, "gem", 2)], &[]);
        let lifted = (0..100u64)
            .filter(|seed| shrine.resolve_upgrades(&mut Rng::seeded(*seed))[C0.index()] == 3)
            .count();
        assert_eq!(lifted, 0, "no corridor, no upgrade");
    }

    // With exactly one connected neighbour the single pick is a certainty —
    // which is the mechanic RU exists to protect.
    #[test]
    fn a_shrines_only_connected_neighbour_is_upgraded_every_time() {
        let shrine = sim(&[(C1, "upgrade", 1), (C0, "gem", 2)], &[(C0, C1)]);
        let lifted = (0..100u64)
            .filter(|seed| shrine.resolve_upgrades(&mut Rng::seeded(*seed))[C0.index()] == 3)
            .count();
        assert_eq!(lifted, 100);
    }

    // The Temple Nexus ignores connections outright — bare lattice adjacency is
    // enough, which is why lattice adjacency is load-bearing independently of
    // the corridor graph.
    #[test]
    fn a_temple_nexus_upgrades_an_unconnected_neighbour() {
        let nexus = sim(&[(C1, "upgrade", 3), (C0, "gem", 2)], &[]);
        let lifted = (0..100u64)
            .filter(|seed| nexus.resolve_upgrades(&mut Rng::seeded(*seed))[C0.index()] == 3)
            .count();
        assert_eq!(lifted, 100);
    }

    // ------------------------------------------------------- reachability --

    // RE. Charges only count from an explosives room that is itself built and
    // connected — the player has to walk to it before blasting anything.
    #[test]
    fn charges_relax_reachability_only_from_a_connected_explosives_room() {
        let connected = sim(
            &[(D1, "explosive", 1), (C2, "corruption", 3)],
            &[(D1, E1)],
        );
        let final_tier = connected.tier;
        let natural = connected.natural_reach();
        assert!(
            connected.charge_reach(&final_tier, natural) & mask_of(C1) != 0,
            "one charge reaches a slot one closed corridor out"
        );

        let stranded = sim(
            &[(D3, "explosive", 1), (C2, "corruption", 3)],
            &[(D3, C2)],
        );
        let natural = stranded.natural_reach();
        assert_eq!(
            stranded.charge_reach(&stranded.tier, natural),
            natural,
            "an explosives room the Entrance cannot reach grants nothing"
        );
    }

    // A charge-only room is worth `blast_discount` of a connected one:
    // explosives are a safety net, not the plan.
    #[test]
    fn a_room_reachable_only_by_blasting_scores_at_the_blast_discount() {
        let profile = rush();
        let valuation = valuation();
        // Locus at C1, one closed corridor from the Entrance, plus a connected
        // Explosives Room to pay for the blast.
        let blasted = sim(
            &[(C1, "corruption", 3), (D1, "explosive", 1)],
            &[(D1, E1)],
        );
        let mut rng = Rng::seeded(5);
        let with_charge = blasted.score(&mut rng, &profile, &valuation);

        let natural = sim(
            &[(C1, "corruption", 3), (D1, "explosive", 1)],
            &[(D1, E1), (C1, D1)],
        );
        let mut rng = Rng::seeded(5);
        let connected = natural.score(&mut rng, &profile, &valuation);

        assert!(
            with_charge < connected,
            "blasting must be worth less than a natural connection: \
             {with_charge} vs {connected}"
        );
        assert!(
            with_charge > 0.5,
            "but it must still be worth something: {with_charge}"
        );
    }

    // The score is the profile's, not the rollout's: a board with both tier-3
    // rooms connected is the combination value plus RD's per-room baseline.
    #[test]
    fn a_finished_board_scores_the_profiles_combination_plus_the_room_baseline() {
        let profile = rush();
        let board = sim(
            &[(E0, "corruption", 3), (E2, "gem", 3)],
            &[(E0, E1), (E1, E2)],
        );
        let mut rng = Rng::seeded(1);
        let score = board.score(&mut rng, &profile, &valuation());
        // 10.0 for Locus + Doryani, plus 0.05 for each of the two built rooms;
        // the Entrance itself pays no baseline.
        assert!((score - 10.1).abs() < 1e-9, "got {score}");
    }

    // ------------------------------------------------------------ profile --

    // The "lost the room" threshold is derived from the profile's own target
    // values, not hard-coded to Sebastian's 7.0.
    #[test]
    fn the_lost_room_threshold_is_the_cheapest_target_outcome_in_the_profile() {
        assert!((valuation().lost_threshold - 7.0).abs() < 1e-9);
        let mut profile = rush();
        profile
            .room_values
            .insert(Line::Gem, [0.0, 0.0, 4.0]);
        assert!((Valuation::for_profile(&profile).lost_threshold - 4.0).abs() < 1e-9);
    }

    // The rollout is reproducible from its seed, which is what makes every
    // assertion in this crate stable.
    #[test]
    fn the_same_seed_produces_the_same_estimate() {
        let profile = rush();
        let valuation = valuation();
        let config = TempleConfig::default();
        let mut board = sim(&[(D1, "junk", 1)], &[(D1, E1)]);
        board.remaining = 5;
        let opening = Opening {
            slot: D1.index(),
            kill: Some((Valuation::CORRUPTION, OfferKind::Change, Tier::T1)),
            doors: vec![(D1.index(), E1.index())],
        };
        let first = evaluate(&board, &opening, &profile, &valuation, &config, 50, &mut Rng::seeded(9));
        let again = evaluate(&board, &opening, &profile, &valuation, &config, 50, &mut Rng::seeded(9));
        assert_eq!(first, again);
        let other = evaluate(&board, &opening, &profile, &valuation, &config, 50, &mut Rng::seeded(10));
        assert_ne!(first.mean, other.mean, "a different seed must sample differently");
    }
}
