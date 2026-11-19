//! Temple option simulation: how often a whole temple ends with its target
//! rooms built, per Atlas setup and skip policy (POE-415).
//!
//! Analysis, not advice. It plays fresh temples from an all-tier-0 board to
//! the end of the incursion budget and counts outcomes, so the 3.30 atlas
//! choices (Power Struggle: Consolidation of Power option 1 or 2, Artefacts of
//! the Vaal) can be compared on the same targets. The advisor does not read it;
//! its own arithmetic (`rollout::Sim::apply_kill`, `DOUBLE_TIER_CHANCE`) is
//! untouched until POE-415 lands, and [`Consolidation::PreRework`] reproduces
//! it here as a reference.
//!
//! Rerun with other targets by editing [`SimulationParams`] in
//! `cradle_and_locus_option_comparison` and running, from the repo root:
//!
//! ```text
//! docker compose run --rm -T -w /app/desktop/src-tauri desktop \
//!   cargo test --lib option_simulation -- --ignored --nocapture
//! ```
//!
//! # Rules and where each comes from
//!
//! - Observed (owner, in game, 2026-11-19): option 1 makes an upgrade +2
//!   tiers, with no chance roll; the 40% / 10% upgrade-chance nodes are gone;
//!   a tier-0 room offers two changes and always lands at tier 1; a map holds
//!   4 incursions with Artefacts of the Vaal, 3 without; a room visited in the
//!   current map cannot be drawn again in that map.
//! - Not validated (POE-415 "Inferred"): under option 2 a change lands at the
//!   killed room's tier + 1 ([`ResidentCarry::TierPlusOne`]); without option 2
//!   a change lands at tier 1. [`ResidentCarry::TierKept`] is the sensitivity
//!   reading "the new room gets the killed room's tier, no +1".
//! - Assumed, carried from the rollout and not measured: the dropped-into room
//!   is uniform over every non-Apex, non-Entrance room below tier 3; a change
//!   offer is uniform over the 25 lines of `rooms::LINES` minus the current
//!   line (a line already elsewhere in the temple can be offered again).
//! - Assumed here: a fresh temple has 12 incursions (the `12 Incursions
//!   Remaining` panel fixture in `panel.rs`), set by [`SimulationParams::budget`].
//!
//! # Player policy
//!
//! Not the advisor's rollout policy, which prices doors and junk rooms for a
//! valuation profile. Here the player takes the kill that most raises the sum,
//! over targets, of the best tier built on that target's line (capped at its
//! goal tier); a tie goes to the upgrade, which also pushes a junk room out of
//! the drop pool. Doors, explosives and the upgrade line's room bonus are not
//! modelled: the outcome is a built tier, not a connected one.

use crate::temple::lattice::Slot;
use crate::temple::rooms::LINES;
use crate::temple::strategy::{Line, TempleConfig, Tier, DOUBLE_TIER_CHANCE};

use super::rollout::Rng;
use super::state::SlotMask;

/// Which Consolidation of Power option the player took, or the pre-3.30
/// nodes the advisor still assumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consolidation {
    /// Today's advisor: Contested Development (a change lands at tier + 1)
    /// and a [`DOUBLE_TIER_CHANCE`] second tier on an upgrade above tier 0.
    PreRework,
    /// No Consolidation option: an upgrade is +1, a change lands at tier 1.
    NotTaken,
    /// Option 1, killing non-resident architects adds an extra tier: an
    /// upgrade is +2, a change lands at tier 1.
    NonResidentExtraTier,
    /// Option 2, killing resident architects carries their tier: an upgrade
    /// is +1, a change lands per the [`ResidentCarry`] reading.
    ResidentTierCarries(ResidentCarry),
}

/// The two readings of option 2's "adds their Upgrade Tier".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidentCarry {
    /// The changed room lands at the killed room's tier + 1.
    TierPlusOne,
    /// The changed room lands at the killed room's tier.
    TierKept,
}

/// Which architect a kill removes, named by its offer's verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillKind {
    /// `Kill to upgrade`: the non-resident dies, the room keeps its line.
    Upgrade,
    /// `Kill to change`: the resident dies, the room takes the other line.
    Change,
}

/// When the player stops spending incursions early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipPolicy {
    /// Every incursion of the budget is played.
    Never,
    /// The temple stops spending once every target is reached.
    WhenDone,
    /// As [`Self::WhenDone`], and the rest of a map is skipped once every
    /// unfinished target room still in the drop pool was visited in it: the
    /// advisor's R5 (leave after using the target room), widened to several
    /// targets.
    WhenDoneOrTargetsVisited,
}

/// One scenario.
#[derive(Debug, Clone)]
pub struct SimulationParams {
    /// The rooms to build, as `(line, goal tier)`.
    pub targets: Vec<(Line, Tier)>,
    pub consolidation: Consolidation,
    /// Read for [`TempleConfig::entrances_per_map`] only.
    pub config: TempleConfig,
    pub skip: SkipPolicy,
    /// Incursions in a fresh temple.
    pub budget: u8,
    pub rollouts: u32,
    pub seed: u64,
}

/// The board as the simulation needs it: a line and a built tier per slot.
#[derive(Debug, Clone, PartialEq)]
pub struct TempleBoard {
    /// `0` is no line (tier-0 filler, Apex, Entrance); `n` is `LINES[n - 1]`.
    pub line: [u8; 13],
    pub tier: [u8; 13],
}

impl TempleBoard {
    /// A fresh temple: every room tier-0 filler.
    pub fn fresh() -> TempleBoard {
        TempleBoard {
            line: [0; 13],
            tier: [0; 13],
        }
    }

    fn best_tier(&self, tag: u8) -> u8 {
        (0..13)
            .filter(|s| self.line[*s] == tag)
            .map(|s| self.tier[s])
            .max()
            .unwrap_or(0)
    }
}

/// A target compiled to the board's line tag.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Target {
    tag: u8,
    goal: u8,
}

fn compile_targets(targets: &[(Line, Tier)]) -> Vec<Target> {
    targets
        .iter()
        .map(|(line, goal)| {
            let index = LINES
                .iter()
                .position(|l| l.key() == line.key())
                .unwrap_or_else(|| panic!("target line {:?} is not in rooms::LINES", line.key()));
            Target {
                tag: index as u8 + 1,
                goal: goal.get(),
            }
        })
        .collect()
}

/// The tier a kill lands at, before any chance roll.
pub fn landing_floor(consolidation: Consolidation, tier: u8, kind: KillKind) -> u8 {
    if tier == 0 {
        return 1;
    }
    let landed = match (kind, consolidation) {
        (KillKind::Upgrade, Consolidation::NonResidentExtraTier) => tier + 2,
        (KillKind::Upgrade, _) => tier + 1,
        (KillKind::Change, Consolidation::PreRework) => tier + 1,
        (KillKind::Change, Consolidation::NotTaken | Consolidation::NonResidentExtraTier) => 1,
        (KillKind::Change, Consolidation::ResidentTierCarries(ResidentCarry::TierPlusOne)) => {
            tier + 1
        }
        (KillKind::Change, Consolidation::ResidentTierCarries(ResidentCarry::TierKept)) => tier,
    };
    landed.min(3)
}

/// The tier a kill lands at, with [`Consolidation::PreRework`]'s double-tier
/// roll applied.
pub fn kill_tier(consolidation: Consolidation, tier: u8, kind: KillKind, rng: &mut Rng) -> u8 {
    let floor = landing_floor(consolidation, tier, kind);
    let rolls = consolidation == Consolidation::PreRework && kind == KillKind::Upgrade && tier > 0;
    if rolls && rng.chance(DOUBLE_TIER_CHANCE) {
        (floor + 1).min(3)
    } else {
        floor
    }
}

/// A change offer's line: uniform over the 25 lines minus `excluded`.
fn draw_line(excluded: &[u8], rng: &mut Rng) -> u8 {
    let pool: Vec<u8> = (1..=LINES.len() as u8)
        .filter(|tag| !excluded.contains(tag))
        .collect();
    pool[rng.below(pool.len())]
}

/// The two architect offers of `slot`: two distinct changes on a tier-0 room
/// (both architects are non-resident), else an upgrade of the room's own line
/// and a change to another.
pub fn offers_at(board: &TempleBoard, slot: usize, rng: &mut Rng) -> [(u8, KillKind); 2] {
    if board.tier[slot] == 0 {
        let first = draw_line(&[], rng);
        let second = draw_line(&[first], rng);
        [(first, KillKind::Change), (second, KillKind::Change)]
    } else {
        let own = board.line[slot];
        [(own, KillKind::Upgrade), (draw_line(&[own], rng), KillKind::Change)]
    }
}

fn progress(board: &TempleBoard, targets: &[Target]) -> u32 {
    targets
        .iter()
        .map(|t| board.best_tier(t.tag).min(t.goal) as u32)
        .sum()
}

fn done(board: &TempleBoard, targets: &[Target]) -> bool {
    targets.iter().all(|t| board.best_tier(t.tag) >= t.goal)
}

/// The slots the next incursion of this map can drop into: never the Apex or
/// the Entrance, never a tier-3 room, never a room already visited this map.
pub fn drawable(board: &TempleBoard, visited: SlotMask) -> Vec<usize> {
    let (apex, entrance) = (Slot::APEX.index(), Slot::ENTRANCE.index());
    (0..13)
        .filter(|s| *s != apex && *s != entrance)
        .filter(|s| board.tier[*s] < 3)
        .filter(|s| visited & (1 << s) == 0)
        .collect()
}

/// Whether the map's remaining incursions can no longer reach a target room
/// directly: an unfinished target's room below its goal exists, and every
/// such room was visited this map.
fn targets_visited(board: &TempleBoard, visited: SlotMask, targets: &[Target]) -> bool {
    let open: Vec<usize> = (0..13)
        .filter(|s| {
            targets.iter().any(|t| {
                board.line[*s] == t.tag
                    && board.tier[*s] < t.goal
                    && board.best_tier(t.tag) < t.goal
            })
        })
        .collect();
    !open.is_empty() && open.iter().all(|s| visited & (1 << s) != 0)
}

/// Whether [`SkipPolicy`] leaves the current map before its next incursion.
pub fn leaves_map(
    skip: SkipPolicy,
    board: &TempleBoard,
    visited: SlotMask,
    targets: &[(Line, Tier)],
) -> bool {
    skip == SkipPolicy::WhenDoneOrTargetsVisited
        && targets_visited(board, visited, &compile_targets(targets))
}

fn play_incursion(
    board: &mut TempleBoard,
    slot: usize,
    targets: &[Target],
    consolidation: Consolidation,
    rng: &mut Rng,
) {
    let offers = offers_at(board, slot, rng);
    let mut best = offers[0];
    let mut best_key = (0u32, false);
    for (i, (tag, kind)) in offers.iter().enumerate() {
        let mut trial = board.clone();
        trial.line[slot] = *tag;
        trial.tier[slot] = landing_floor(consolidation, board.tier[slot], *kind);
        let key = (progress(&trial, targets), *kind == KillKind::Upgrade);
        if i == 0 || key > best_key {
            best = (*tag, *kind);
            best_key = key;
        }
    }
    let (tag, kind) = best;
    board.tier[slot] = kill_tier(consolidation, board.tier[slot], kind, rng);
    board.line[slot] = tag;
}

/// One temple played to its end.
#[derive(Debug, Clone, PartialEq)]
pub struct TempleOutcome {
    /// Per target, in [`SimulationParams::targets`] order.
    pub reached: Vec<bool>,
    pub incursions: u8,
    /// Maps with at least one incursion played.
    pub maps: u32,
    /// The temple stopped with budget left because every target was reached.
    pub stopped_when_done: bool,
    /// Map entrances left unplayed by [`SkipPolicy::WhenDoneOrTargetsVisited`]
    /// leaving a map early. These cost a map, not budget.
    pub entrances_abandoned: u32,
}

/// Play one temple from `board`.
pub fn play_temple(
    mut board: TempleBoard,
    params: &SimulationParams,
    rng: &mut Rng,
) -> TempleOutcome {
    let targets = compile_targets(&params.targets);
    let entrances = params.config.entrances_per_map() as u32;
    let mut remaining = params.budget;
    let mut maps = 0u32;
    let mut stopped_when_done = false;
    let mut entrances_abandoned = 0u32;
    'temple: while remaining > 0 {
        let mut visited: SlotMask = 0;
        for entrance in 0..entrances {
            if remaining == 0 {
                break 'temple;
            }
            if params.skip != SkipPolicy::Never && done(&board, &targets) {
                stopped_when_done = true;
                break 'temple;
            }
            if entrance > 0
                && params.skip == SkipPolicy::WhenDoneOrTargetsVisited
                && targets_visited(&board, visited, &targets)
            {
                entrances_abandoned += entrances - entrance;
                continue 'temple;
            }
            let pool = drawable(&board, visited);
            if pool.is_empty() {
                if visited == 0 {
                    break 'temple;
                }
                continue 'temple;
            }
            if entrance == 0 {
                maps += 1;
            }
            let slot = pool[rng.below(pool.len())];
            play_incursion(&mut board, slot, &targets, params.consolidation, rng);
            visited |= 1 << slot;
            remaining -= 1;
        }
    }
    TempleOutcome {
        reached: targets.iter().map(|t| board.best_tier(t.tag) >= t.goal).collect(),
        incursions: params.budget - remaining,
        maps,
        stopped_when_done,
        entrances_abandoned,
    }
}

/// What `rollouts` temples of one scenario add up to.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub rollouts: u32,
    /// Temples that reached every target.
    pub all_reached: u32,
    /// Per target, temples that reached it.
    pub each_reached: Vec<u32>,
    /// Temples by maps used: `maps_histogram[m]` used `m` maps.
    pub maps_histogram: Vec<u32>,
    pub incursions_total: u64,
    pub stopped_when_done: u32,
    /// Budget left unspent over all temples.
    pub budget_saved_total: u64,
    /// Temples that left at least one map early.
    pub left_a_map: u32,
    pub entrances_abandoned_total: u64,
}

impl Summary {
    fn share(&self, count: u32) -> f64 {
        count as f64 / self.rollouts as f64
    }

    /// A share and its binomial standard error.
    pub fn proportion(&self, count: u32) -> (f64, f64) {
        let p = self.share(count);
        (p, (p * (1.0 - p) / self.rollouts as f64).sqrt())
    }

    /// Mean maps per temple and its standard error.
    pub fn maps_mean(&self) -> (f64, f64) {
        let n = self.rollouts as f64;
        let (sum, sum_sq) = self
            .maps_histogram
            .iter()
            .enumerate()
            .fold((0.0, 0.0), |(s, q), (m, c)| {
                (s + (m as f64) * *c as f64, q + (m as f64).powi(2) * *c as f64)
            });
        let mean = sum / n;
        (mean, ((sum_sq / n - mean * mean).max(0.0) / n).sqrt())
    }

    pub fn maps_median(&self) -> usize {
        let half = (self.rollouts as u64).div_ceil(2);
        let mut seen = 0u64;
        for (m, c) in self.maps_histogram.iter().enumerate() {
            seen += *c as u64;
            if seen >= half {
                return m;
            }
        }
        0
    }
}

/// Run one scenario from fresh temples.
pub fn simulate(params: &SimulationParams) -> Summary {
    let mut rng = Rng::seeded(params.seed);
    let mut summary = Summary {
        rollouts: params.rollouts,
        all_reached: 0,
        each_reached: vec![0; params.targets.len()],
        maps_histogram: Vec::new(),
        incursions_total: 0,
        stopped_when_done: 0,
        budget_saved_total: 0,
        left_a_map: 0,
        entrances_abandoned_total: 0,
    };
    for _ in 0..params.rollouts {
        let outcome = play_temple(TempleBoard::fresh(), params, &mut rng);
        if outcome.reached.iter().all(|r| *r) {
            summary.all_reached += 1;
        }
        for (count, reached) in summary.each_reached.iter_mut().zip(&outcome.reached) {
            *count += *reached as u32;
        }
        let maps = outcome.maps as usize;
        if summary.maps_histogram.len() <= maps {
            summary.maps_histogram.resize(maps + 1, 0);
        }
        summary.maps_histogram[maps] += 1;
        summary.incursions_total += outcome.incursions as u64;
        summary.stopped_when_done += outcome.stopped_when_done as u32;
        summary.budget_saved_total += (params.budget - outcome.incursions) as u64;
        summary.left_a_map += (outcome.entrances_abandoned > 0) as u32;
        summary.entrances_abandoned_total += outcome.entrances_abandoned as u64;
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::lattice::Slot::*;

    fn cradle() -> Line {
        Line::named("throne_of_atziri")
    }

    fn tag(line: &Line) -> u8 {
        compile_targets(&[(line.clone(), Tier::T3)])[0].tag
    }

    fn params(consolidation: Consolidation, skip: SkipPolicy, budget: u8) -> SimulationParams {
        SimulationParams {
            targets: vec![(cradle(), Tier::T3), (Line::Corruption, Tier::T3)],
            consolidation,
            config: TempleConfig {
                artefacts_of_the_vaal: true,
                scarab_of_timelines: false,
            },
            skip,
            budget,
            rollouts: 1,
            seed: 1,
        }
    }

    const ALL: [Consolidation; 5] = [
        Consolidation::PreRework,
        Consolidation::NotTaken,
        Consolidation::NonResidentExtraTier,
        Consolidation::ResidentTierCarries(ResidentCarry::TierPlusOne),
        Consolidation::ResidentTierCarries(ResidentCarry::TierKept),
    ];

    // Option 1's extra tier is certain: no seed leaves a tier-1 upgrade short
    // of tier 3.
    #[test]
    fn option_one_upgrades_a_tier_one_room_to_tier_three_on_every_seed() {
        for seed in 0..50u64 {
            let landed = kill_tier(
                Consolidation::NonResidentExtraTier,
                1,
                KillKind::Upgrade,
                &mut Rng::seeded(seed),
            );
            assert_eq!(landed, 3, "seed {seed}");
        }
    }

    // Owner correction 2026-11-19: tier 0 goes to tier 1 only, under any option.
    #[test]
    fn a_tier_zero_kill_lands_at_tier_one_under_every_option() {
        for consolidation in ALL {
            for seed in 0..20u64 {
                let landed = kill_tier(consolidation, 0, KillKind::Change, &mut Rng::seeded(seed));
                assert_eq!(landed, 1, "{consolidation:?}, seed {seed}");
            }
        }
    }

    // Both architects of a tier-0 room are non-resident, so neither offer is an
    // upgrade, and the two name different lines.
    #[test]
    fn a_tier_zero_room_offers_two_different_changes() {
        let board = TempleBoard::fresh();
        for seed in 0..50u64 {
            let [(a, kind_a), (b, kind_b)] = offers_at(&board, D2.index(), &mut Rng::seeded(seed));
            assert_eq!((kind_a, kind_b), (KillKind::Change, KillKind::Change), "seed {seed}");
            assert_ne!(a, b, "seed {seed}");
        }
    }

    #[test]
    fn option_two_lands_a_change_at_the_killed_rooms_tier_plus_one() {
        let landed = landing_floor(
            Consolidation::ResidentTierCarries(ResidentCarry::TierPlusOne),
            2,
            KillKind::Change,
        );
        assert_eq!(landed, 3);
    }

    #[test]
    fn option_one_lands_a_change_at_tier_one() {
        assert_eq!(landing_floor(Consolidation::NonResidentExtraTier, 2, KillKind::Change), 1);
    }

    #[test]
    fn a_room_visited_this_map_is_not_drawable() {
        let pool = drawable(&TempleBoard::fresh(), 1 << D2.index());
        assert!(!pool.contains(&D2.index()));
        assert!(pool.contains(&D3.index()));
    }

    // Two drawable rooms and four entrances: no-revisit ends the map after two
    // incursions, so a budget of four takes two maps. Without the rule all four
    // would fit in one.
    #[test]
    fn no_revisit_spreads_incursions_over_maps() {
        let mut board = TempleBoard::fresh();
        for slot in Slot::ALL {
            if ![D2, D3, Slot::APEX, Slot::ENTRANCE].contains(&slot) {
                board.tier[slot.index()] = 3;
            }
        }
        let outcome = play_temple(
            board,
            &params(Consolidation::NotTaken, SkipPolicy::Never, 4),
            &mut Rng::seeded(3),
        );
        assert_eq!((outcome.incursions, outcome.maps), (4, 2));
    }

    fn board_with_cradle_at_d2() -> TempleBoard {
        let mut board = TempleBoard::fresh();
        board.line[D2.index()] = tag(&cradle());
        board.tier[D2.index()] = 1;
        board
    }

    #[test]
    fn the_map_is_left_once_every_open_target_room_was_visited() {
        let targets = params(Consolidation::NotTaken, SkipPolicy::Never, 1).targets;
        let board = board_with_cradle_at_d2();
        let visited = 1 << D2.index();
        let leaves = leaves_map(SkipPolicy::WhenDoneOrTargetsVisited, &board, visited, &targets);
        assert!(leaves);
    }

    #[test]
    fn the_map_is_kept_while_a_target_room_is_unvisited() {
        let targets = params(Consolidation::NotTaken, SkipPolicy::Never, 1).targets;
        let board = board_with_cradle_at_d2();
        let visited = 1 << D3.index();
        let leaves = leaves_map(SkipPolicy::WhenDoneOrTargetsVisited, &board, visited, &targets);
        assert!(!leaves);
    }

    // With no target room on the board a fresh map draws from the same pool,
    // so leaving would gain nothing.
    #[test]
    fn the_map_is_kept_while_no_target_room_exists() {
        let targets = params(Consolidation::NotTaken, SkipPolicy::Never, 1).targets;
        let board = TempleBoard::fresh();
        let leaves = leaves_map(SkipPolicy::WhenDoneOrTargetsVisited, &board, u16::MAX, &targets);
        assert!(!leaves);
    }

    fn finished_board() -> TempleBoard {
        let mut board = TempleBoard::fresh();
        board.line[D2.index()] = tag(&cradle());
        board.tier[D2.index()] = 3;
        board.line[D3.index()] = tag(&Line::Corruption);
        board.tier[D3.index()] = 3;
        board
    }

    #[test]
    fn a_finished_temple_spends_no_budget_under_the_when_done_policy() {
        let outcome = play_temple(
            finished_board(),
            &params(Consolidation::NotTaken, SkipPolicy::WhenDone, 12),
            &mut Rng::seeded(1),
        );
        assert_eq!((outcome.incursions, outcome.stopped_when_done), (0, true));
    }

    #[test]
    fn a_finished_temple_spends_the_whole_budget_when_never_skipping() {
        let outcome = play_temple(
            finished_board(),
            &params(Consolidation::NotTaken, SkipPolicy::Never, 12),
            &mut Rng::seeded(1),
        );
        assert_eq!(outcome.incursions, 12);
    }

    // The Cradle of Creation + Locus of Corruption comparison for POE-415.
    // Prints one row per scenario; edit the grid or the targets and rerun.
    #[test]
    #[ignore = "long Monte Carlo run; prints the option table"]
    fn cradle_and_locus_option_comparison() {
        const ROLLOUTS: u32 = 200_000;
        const SEED: u64 = 0x5EED_0330;
        let consolidations = [
            ("opt 1 (upgrade +2)", Consolidation::NonResidentExtraTier),
            (
                "opt 2 (change = tier + 1)",
                Consolidation::ResidentTierCarries(ResidentCarry::TierPlusOne),
            ),
            (
                "opt 2 (change = tier)",
                Consolidation::ResidentTierCarries(ResidentCarry::TierKept),
            ),
            ("no Consolidation node", Consolidation::NotTaken),
            ("pre-3.30 (advisor today)", Consolidation::PreRework),
        ];
        let skips = [
            ("never", SkipPolicy::Never),
            ("when done", SkipPolicy::WhenDone),
            ("done + leave map", SkipPolicy::WhenDoneOrTargetsVisited),
        ];
        println!(
            "| scenario | AoV | skip | P(both) | P(Cradle) | P(Locus) | maps mean | maps median \
             | incursions | stopped done | budget saved | left a map | entrances abandoned |"
        );
        println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
        for (name, consolidation) in consolidations {
            for aov in [true, false] {
                for (skip_name, skip) in skips {
                    let params = SimulationParams {
                        targets: vec![(cradle(), Tier::T3), (Line::Corruption, Tier::T3)],
                        consolidation,
                        config: TempleConfig {
                            artefacts_of_the_vaal: aov,
                            scarab_of_timelines: false,
                        },
                        skip,
                        budget: 12,
                        rollouts: ROLLOUTS,
                        seed: SEED,
                    };
                    let s = simulate(&params);
                    let n = s.rollouts as f64;
                    let (both, both_se) = s.proportion(s.all_reached);
                    let (maps, maps_se) = s.maps_mean();
                    println!(
                        "| {name} | {} | {skip_name} | {:.3} ±{:.4} | {:.3} | {:.3} \
                         | {:.2} ±{:.3} | {} | {:.2} | {:.1}% | {:.2} | {:.1}% | {:.2} |",
                        if aov { 4 } else { 3 },
                        both,
                        both_se,
                        s.proportion(s.each_reached[0]).0,
                        s.proportion(s.each_reached[1]).0,
                        maps,
                        maps_se,
                        s.maps_median(),
                        s.incursions_total as f64 / n,
                        100.0 * s.proportion(s.stopped_when_done).0,
                        s.budget_saved_total as f64 / n,
                        100.0 * s.proportion(s.left_a_map).0,
                        s.entrances_abandoned_total as f64 / n,
                    );
                }
            }
        }
    }
}
