//! Strategy profiles and the Chase/Scarab mode selector (POE-167).
//!
//! Ported from the Python prototype `temple_model.py` (`SCORE_TABLE`,
//! `_table`, `ROOM_BASELINE`, `BLAST_DISCOUNT`, `LINES`), with the hard-coded
//! score table replaced by data on a [`StrategyProfile`] so a second strategy
//! is a second constructor rather than a second code path.
//!
//! # The objective function, in one paragraph
//!
//! A finished temple is scored from the rooms that are **reachable from the
//! Entrance**. A matching entry in [`StrategyProfile::combinations`] wins
//! outright; otherwise the reachable lines' per-tier values are summed. On top
//! of that sits the Apex term, then RD's per-room baseline. Traversal cost
//! (`path_cost`) is subtracted separately — see [`StrategyProfile::path_penalty`].
//!
//! Combinations exist because the ranking is **not additive**: Sebastian's
//! measured outcome ranking is Locus 9, Doryani 7, both 10. No per-room weights
//! reproduce that, so the joint outcome carries its own number.
//!
//! # Partial credit is essentially none
//!
//! Only the tier-3 room of a line pays. A finished temple holding Catalyst of
//! Corruption II is worth its RD baseline and nothing else — which is why
//! `room_values` for the Locus/Doryani Rush is zero at tiers 1 and 2.
//!
//! # What this module deliberately does not do
//!
//! It has no board, no lattice, no door set and no reachability search. Every
//! function takes the *already-resolved* reachable rooms; deciding what is
//! reachable (including the explosive-charge relaxation and the upgrade-room
//! resolution pass) belongs to POE-170. [`StrategyProfile::blend_blast`] is the
//! seam: POE-170 scores the natural component and the charge-relaxed component
//! and hands both back here.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Chance that killing a non-resident architect grants a *double* tier.
///
/// Resource Reallocation (40%) plus Incursion Upgrade Chance (10%). Both are
/// **assumed taken and are deliberately not configurable** — Sebastian:
/// *"without those, don't even run the farming strategy"*.
///
/// Two limits are part of the constant's meaning:
///
/// - it applies to **`upgrade` only, never to `change`** — both nodes read
///   "killing non-resident Architects", and non-resident is the upgrade side,
///   while Contested Development powers `change` with a flat +1 and no roll;
/// - it never produces **0 → 2**. A tier-0 room has no line to upgrade.
#[allow(dead_code)]
pub const DOUBLE_TIER_CHANCE: f64 = 0.50;

// --------------------------------------------------------------- room line --

/// Canonical key of the corruption line (Corruption Chamber → Catalyst of
/// Corruption → **Locus of Corruption**).
#[allow(dead_code)]
const KEY_CORRUPTION: &str = "corruption";
/// Canonical key of the gem line (Gemcutter's Workshop → Department of
/// Thaumaturgy → **Doryani's Institute**).
#[allow(dead_code)]
const KEY_GEM: &str = "gem";
/// Canonical key of the upgrade line (Shrine of Empowerment → Sanctum of Unity
/// → Temple Nexus).
#[allow(dead_code)]
const KEY_UPGRADE: &str = "upgrade";
/// Canonical key of the explosives line (Explosives Room → Demolition Lab →
/// Shrine of Unmaking).
#[allow(dead_code)]
const KEY_EXPLOSIVE: &str = "explosive";

/// A room *line* — the three-tier family a room belongs to, not one room.
///
/// # Why four variants plus an open tail
///
/// The temple has roughly 25 lines; only four are mechanically relevant to the
/// engine, and the prototype named exactly these four. The closed vocabulary
/// lands in POE-169, so [`Line::Other`] keeps the type usable before then and
/// keeps the long tail addressable afterwards: the advisor still has to reason
/// about junk rooms (R4 maxes them out of the drop pool, RD pays a baseline for
/// them), and it cannot do that if junk is unnameable.
///
/// # `Other` never holds a known key
///
/// Build every line from [`Line::named`], which canonicalises. Constructing
/// `Line::Other("corruption".into())` by hand produces a value that compares
/// unequal to [`Line::Corruption`] and silently scores zero.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(dead_code)]
pub enum Line {
    /// Ends in Locus of Corruption — priority #1 of the whole build.
    Corruption,
    /// Ends in Doryani's Institute. Called the "gem" line after the prototype.
    Gem,
    /// Upgrades other rooms by a tier. Instrumental, not a terminal outcome.
    Upgrade,
    /// Carries the explosive charges that relax reachability (RE).
    Explosive,
    /// Any other line, by its canonical key. Never one of the four above.
    Other(String),
}

#[allow(dead_code)]
impl Line {
    /// Canonical key for a line, and the wire form used by serde.
    pub fn key(&self) -> &str {
        match self {
            Line::Corruption => KEY_CORRUPTION,
            Line::Gem => KEY_GEM,
            Line::Upgrade => KEY_UPGRADE,
            Line::Explosive => KEY_EXPLOSIVE,
            Line::Other(key) => key,
        }
    }

    /// The only way to build a `Line` from text. A known key resolves to its
    /// variant so that [`Line::Other`] can never shadow one of the four.
    pub fn named(key: &str) -> Line {
        match key {
            KEY_CORRUPTION => Line::Corruption,
            KEY_GEM => Line::Gem,
            KEY_UPGRADE => Line::Upgrade,
            KEY_EXPLOSIVE => Line::Explosive,
            other => Line::Other(other.to_string()),
        }
    }
}

// Serialised as its bare key rather than as a Rust enum: `room_values` is a map
// KEYED by `Line`, and serde_json rejects a non-string map key — the derived
// representation of `Other(String)` is an object, so a derive here would make
// every profile unserialisable the moment settings persistence lands.
impl Serialize for Line {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for Line {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Ok(Line::named(&key))
    }
}

// -------------------------------------------------------------------- tier --

/// A room tier, `0..=3`. Tier 0 is filler: a slot the temple generated with no
/// line of its own, worth nothing and upgradeable by nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(dead_code)]
pub struct Tier(u8);

#[allow(dead_code)]
impl Tier {
    /// Filler — no line, no value, and never a legal upgrade target result.
    pub const T0: Tier = Tier(0);
    /// e.g. Corruption Chamber, Gemcutter's Workshop.
    pub const T1: Tier = Tier(1);
    /// e.g. Catalyst of Corruption, Department of Thaumaturgy.
    pub const T2: Tier = Tier(2);
    /// e.g. Locus of Corruption, Doryani's Institute. The tier that pays.
    pub const T3: Tier = Tier(3);

    /// The highest tier the game can produce.
    pub const MAX_VALUE: u8 = 3;

    /// `None` for anything above [`Tier::MAX_VALUE`] — the game has no tier 4.
    pub fn new(value: u8) -> Option<Tier> {
        (value <= Tier::MAX_VALUE).then_some(Tier(value))
    }

    /// The tier as a number.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl Serialize for Tier {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0)
    }
}

// Hand-written so that a hand-edited or stale settings file cannot smuggle a
// tier 7 past the 0..=3 invariant the rest of the module relies on.
impl<'de> Deserialize<'de> for Tier {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = u8::deserialize(deserializer)?;
        Tier::new(raw).ok_or_else(|| {
            serde::de::Error::custom(format!("temple tier {raw} is outside 0..={}", Tier::MAX_VALUE))
        })
    }
}

// -------------------------------------------------------------------- mode --

/// Which of the two farming modes the board is in.
///
/// The mode sits **above** the per-decision ranking; it is not another term in
/// the score. It decides which *map* the incursion budget is spent in, while
/// the score decides which *door* to open once there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum Mode {
    /// A target line is still missing. R5 applies: leave the map once the
    /// target room has been used, because staying risks a guaranteed-zero draw.
    Chase,
    /// Both target lines exist, so tiers stop mattering — the Incursion Scarab
    /// of Timelines re-rolls them in the itemised copy. Complete every entrance
    /// and farm the itemised re-rolls.
    Scarab,
}

/// The rule that picks the [`Mode`]. A profile field so the trigger can be
/// tightened or loosened per user without touching the selector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ModeRule {
    /// v1 (Sebastian, 2026-08-18): Scarab as soon as every named line is
    /// **built and connected to the Entrance**, at any tier ≥ 1. Built-but-
    /// unconnected stays Chase — it is a gamble Sebastian has taken and mostly
    /// won, but it is not the rule that ships first.
    ///
    /// An empty line set means "no requirement" and therefore always Scarab;
    /// no shipped constructor produces one.
    ///
    /// Named extension point: a future variant carrying the *odds* that an
    /// unconnected room still gets connected, letting the user choose the
    /// gamble instead of being held to the safe rule.
    LinesConnected(Vec<Line>),
}

// ------------------------------------------------------------- combination --

/// A whole-temple outcome that is worth more (or less) than the sum of its
/// rooms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Combination {
    /// Every `(line, tier)` that must be reachable. A reached room satisfies a
    /// requirement when its line matches and its tier is **at least** the
    /// required one — a higher tier of the same line is strictly better, so it
    /// can never fail a lower-tier requirement.
    pub requires: Vec<(Line, Tier)>,
    /// The score of the whole outcome. Replaces the per-room sum entirely.
    pub score: f64,
}

#[allow(dead_code)]
impl Combination {
    fn matches(&self, reached: &BTreeMap<Line, Tier>) -> bool {
        self.requires.iter().all(|(line, tier)| {
            reached
                .get(line)
                .is_some_and(|reached_tier| *reached_tier >= *tier)
        })
    }
}

// ---------------------------------------------------------------- profile ---

/// One player's answer to "what is a finished temple worth".
///
/// Everything a player might disagree on lives here as data. The mechanics and
/// connectivity rules (RV/R1/R2/RS/RD/RE/RU/RC) are strategy-independent and do
/// not appear.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct StrategyProfile {
    /// Per-line, per-tier value, indexed `[tier1, tier2, tier3]`. Tier 0 always
    /// scores zero and has no slot. A line absent from the map is worth zero.
    pub room_values: BTreeMap<Line, [f64; 3]>,
    /// What the Apex is worth **on its own**. On a board that already carries
    /// value the Apex instead adds [`Self::apex_mixed_increment`].
    pub apex_score: f64,
    /// What the Apex adds to a board that *already* carries value, as opposed
    /// to the full [`Self::apex_score`] paid when the Apex is all there is.
    ///
    /// **ASSUMED, not measured.** Sebastian supplied four numbers — Locus+
    /// Doryani 10, Locus 9, Doryani 7, Apex alone 2 — and said nothing about
    /// what the Apex adds to a board that already has a tier-3 room. The
    /// prototype's `SCORE_TABLE` guessed +0.5 for the three mixed states and
    /// flagged the guess; the Rush's value is that guess. It is a profile field
    /// rather than a constant because a player who rates the Apex highly
    /// disagrees about *both* Apex numbers, not only the standalone one — with
    /// a fixed increment the Apex weight would be inert on every board that
    /// already has a tier-3 room, which is most of them.
    pub apex_mixed_increment: f64,
    /// Whole-outcome scores that override the per-room sum. The highest-scoring
    /// matching entry wins.
    pub combinations: Vec<Combination>,
    /// RD — every connected built room pays a little, because each room grants
    /// global or adjacent bonuses *and* the chance to open a given door may not
    /// come again. Small enough never to outrank a combination, large enough to
    /// make "open something" beat "open nothing".
    pub room_baseline: f64,
    /// RE — a room reachable only by spending explosive charges is worth this
    /// fraction of a naturally connected one. Charges are a safety net, not a
    /// plan: blasting in costs the player a detour at run time.
    pub blast_discount: f64,
    /// Which trigger flips [`Mode::Chase`] to [`Mode::Scarab`].
    pub mode_rule: ModeRule,
    /// Run-time traversal weight, charged per BFS hop from the Entrance. Zero
    /// for the Locus/Doryani Rush (rush in, grab the room, leave); high for
    /// Vertolka, who builds a straight rushable path to the Apex and routes the
    /// rest around the farming run.
    pub path_cost: f64,
    /// poewiki strategy item 1: with no favourable line anywhere on the board,
    /// prefer `change` over `upgrade` until one exists; once one does, R4
    /// (deliberately max junk out of the drop pool) takes over. Read by the
    /// architect-choice advisor (POE-170), not by the scorer.
    pub reroll_until_favourable: bool,
    /// R4's carve-out: keep a slot in the drop pool while an adjacent upgrade
    /// room can still hit it, instead of maxing it out of the pool.
    ///
    /// **PROPOSED, pending Sebastian's explicit yes** (TEMPLE-CORE-RULES §6e).
    /// The evidence is live board 8: the transcript records him typing
    /// *"upgrade here is correct one"*, but the next board shows that Workshop
    /// as **Cultivar Chamber II** — the poison line at tier 2, i.e. he actually
    /// *changed*, keeping the slot as the adjacent Sanctum's only live target.
    /// The reply and the board disagree, so the rule ships behind a field
    /// rather than silently.
    ///
    /// It is a profile field and not a constant because §4 settles that
    /// *"everything a player might disagree on is a profile field"*, and R4 vs
    /// staying in the pool is §6e's one explicitly unresolved tension.
    pub r4_keep_upgrade_targets: bool,
}

#[allow(dead_code)]
impl StrategyProfile {
    /// Sebastian's Locus / Doryani Rush.
    ///
    /// The numbers are his measured outcome ranking (1–10): Locus of Corruption
    /// plus Doryani's Institute 10, Locus alone 9, Doryani alone 7, Apex alone
    /// 2 — *"very nice to have, totally not needed"*. Tiers 1 and 2 score zero
    /// because partial credit is essentially none.
    ///
    /// Note the split: the two single-line outcomes are plain `room_values`, so
    /// the joint outcome needs exactly one combination — the one number no per-
    /// room weighting can reproduce, since 9 + 7 must yield 10.
    pub fn locus_doryani_rush() -> StrategyProfile {
        let mut room_values = BTreeMap::new();
        room_values.insert(Line::Corruption, [0.0, 0.0, 9.0]);
        room_values.insert(Line::Gem, [0.0, 0.0, 7.0]);
        // The upgrade and explosive lines are instrumental — they move tiers
        // and reachability, which the score already sees through the resulting
        // rooms. Valuing them again here would double-count.
        room_values.insert(Line::Upgrade, [0.0, 0.0, 0.0]);
        room_values.insert(Line::Explosive, [0.0, 0.0, 0.0]);

        StrategyProfile {
            room_values,
            apex_score: 2.0,
            apex_mixed_increment: 0.5,
            combinations: vec![Combination {
                requires: vec![(Line::Corruption, Tier::T3), (Line::Gem, Tier::T3)],
                score: 10.0,
            }],
            // Free parameters, both carried over from the prototype and both
            // still untuned against real outcomes.
            room_baseline: 0.05,
            blast_discount: 0.5,
            mode_rule: ModeRule::LinesConnected(vec![Line::Corruption, Line::Gem]),
            path_cost: 0.0,
            reroll_until_favourable: false,
            r4_keep_upgrade_targets: true,
        }
    }

    /// Score a finished temple.
    ///
    /// `reached` is every identified room the Entrance can reach, as
    /// `(line, tier)`; tier-0 entries are ignored, and a line appearing more
    /// than once counts **once, at its highest tier** — `room_values` prices a
    /// line's presence, not a room count, and per-room accrual is `room_baseline`'s
    /// job. `built_connected_count` is that RD tally: how many connected rooms
    /// have tier ≥ 1, junk included — **excluding the Entrance slot, which pays
    /// no baseline** (prototype `temple_model.py:179`, `s != ENTRANCE`).
    ///
    /// The two inputs are independent on purpose. Sebastian's 10/9/7/2 ranking
    /// is an *outcome* ranking with no board attached, so it is reproduced by
    /// passing a tally of zero; a real board adds its baseline on top.
    ///
    /// Traversal cost is **not** applied here — see [`Self::aggregate_with_path`].
    pub fn aggregate(
        &self,
        reached: &[(Line, Tier)],
        apex_reached: bool,
        built_connected_count: usize,
    ) -> f64 {
        let best: BTreeMap<Line, Tier> = highest_tier_per_line(reached);

        let combination = self.best_combination(&best);
        let core = combination.unwrap_or_else(|| self.sum_room_values(&best));

        // "The board already carries value" is a question about what was
        // *matched*, not about the sign of the number: a profile is free to
        // price an outcome at zero (or below), and such a board is still not a
        // bare Apex.
        let carries_value = combination.is_some() || core > 0.0;

        let apex = if !apex_reached {
            0.0
        } else if carries_value {
            self.apex_mixed_increment
        } else {
            self.apex_score
        };

        core + apex + self.room_baseline * built_connected_count as f64
    }

    /// [`Self::aggregate`] minus the traversal term.
    ///
    /// **Decision made in POE-167, not handed down:** the task left it open
    /// whether `path_cost` subtracts from the score or only breaks ties. It
    /// subtracts, `path_cost × hops`, and it lives in its own function so the
    /// choice is reversible without touching the objective function. Subtracting
    /// was chosen because Vertolka's rationale — *"better to spend time building
    /// than running to dead ends"* — is a claim that a long route costs real
    /// value, not merely that it loses coin flips; a tie-break could never make
    /// a nearer mediocre board beat a distant good one, which is exactly the
    /// trade he describes making.
    ///
    /// `hops` is the BFS distance from the Entrance to whatever the caller is
    /// pricing (the Apex, or the target room). Defining that distance is
    /// POE-170's job; this function only charges for it.
    pub fn aggregate_with_path(
        &self,
        reached: &[(Line, Tier)],
        apex_reached: bool,
        built_connected_count: usize,
        hops: usize,
    ) -> f64 {
        self.aggregate(reached, apex_reached, built_connected_count) - self.path_penalty(hops)
    }

    /// The traversal term on its own.
    pub fn path_penalty(&self, hops: usize) -> f64 {
        self.path_cost * hops as f64
    }

    /// RE — blend the naturally reachable score with the charge-relaxed one.
    ///
    /// Everything the Entrance reaches through open doors counts in full;
    /// whatever only explosive charges add counts at `blast_discount`. Both
    /// inputs come from [`Self::aggregate`] over the two reachability sets.
    pub fn blend_blast(&self, natural: f64, with_charges: f64) -> f64 {
        natural + self.blast_discount * (with_charges - natural)
    }

    /// Chase or Scarab for this board.
    ///
    /// `connected` is every identified room the Entrance can reach, as
    /// `(line, tier)`. Built-but-stranded rooms are deliberately *not* an
    /// input: v1 treats them as absent, and the connection-odds extension will
    /// bring its own input for the gamble it prices rather than inheriting an
    /// unread parameter from here.
    ///
    /// Reads [`Self::mode_rule`]; see [`ModeRule::LinesConnected`] for what v1
    /// requires.
    pub fn select_mode(&self, connected: &[(Line, Tier)]) -> Mode {
        match &self.mode_rule {
            ModeRule::LinesConnected(required) => {
                // `highest_tier_per_line` has already dropped the tier-0
                // filler slots, so presence in the map IS "built at tier >= 1".
                let connected = highest_tier_per_line(connected);
                let all_present = required.iter().all(|line| connected.contains_key(line));
                if all_present {
                    Mode::Scarab
                } else {
                    Mode::Chase
                }
            }
        }
    }

    fn best_combination(&self, reached: &BTreeMap<Line, Tier>) -> Option<f64> {
        self.combinations
            .iter()
            .filter(|combination| combination.matches(reached))
            .map(|combination| combination.score)
            .fold(None, |best: Option<f64>, score| match best {
                Some(current) if current >= score => Some(current),
                _ => Some(score),
            })
    }

    // `reached` always comes from `highest_tier_per_line`, which is the single
    // tier-0 guard: every tier here is 1..=3, so `tier - 1` indexes the
    // `[f64; 3]` in bounds.
    fn sum_room_values(&self, reached: &BTreeMap<Line, Tier>) -> f64 {
        reached
            .iter()
            .filter_map(|(line, tier)| {
                let values = self.room_values.get(line)?;
                // Not a second guard — an assertion that names the invariant,
                // so a future caller that skipped the collapse fails loudly
                // instead of quietly scoring the filler room as zero.
                let index = usize::from(tier.get())
                    .checked_sub(1)
                    .expect("tier-0 rooms are dropped by highest_tier_per_line");
                Some(values[index])
            })
            .sum()
    }
}

/// Collapse `(line, tier)` pairs to the best tier seen per line, dropping the
/// tier-0 filler rooms that carry no line at all.
#[allow(dead_code)]
fn highest_tier_per_line(rooms: &[(Line, Tier)]) -> BTreeMap<Line, Tier> {
    let mut best: BTreeMap<Line, Tier> = BTreeMap::new();
    for (line, tier) in rooms {
        if *tier == Tier::T0 {
            continue;
        }
        best.entry(line.clone())
            .and_modify(|current| {
                if *tier > *current {
                    *current = *tier;
                }
            })
            .or_insert(*tier);
    }
    best
}

// ----------------------------------------------------------------- config ---

/// The only two things the user configures.
///
/// The three tier-modifying Atlas nodes (Contested Development, Resource
/// Reallocation, Incursion Upgrade Chance) are **assumed taken and are not
/// surfaced** — see [`DOUBLE_TIER_CHANCE`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct TempleConfig {
    /// Atlas passive: *"Your Maps with Incursions always have four Incursions"*.
    pub artefacts_of_the_vaal: bool,
    /// The Incursion Scarab of Timelines drops an itemised copy of the temple
    /// when the **final** architect in the area is slain, which means every
    /// incursion in the map must be completed — so R5 cannot apply.
    pub scarab_of_timelines: bool,
}

impl Default for TempleConfig {
    fn default() -> Self {
        TempleConfig {
            artefacts_of_the_vaal: true,
            scarab_of_timelines: false,
        }
    }
}

#[allow(dead_code)]
impl TempleConfig {
    /// Incursion entrances per map — the rate at which the temple budget is
    /// spent.
    pub fn entrances_per_map(&self) -> u8 {
        if self.artefacts_of_the_vaal {
            4
        } else {
            3
        }
    }

    /// Whether R5 (leave the map once the target room has been used) is
    /// available. Running the scarab requires finishing every entrance, so it
    /// takes the choice away.
    ///
    /// This is only the *config* half of the rule. The composition POE-170 is
    /// expected to apply is `mode == Mode::Chase && config.r5_applies()` — R5
    /// is a chase behaviour, and Scarab mode already completes every entrance.
    pub fn r5_applies(&self) -> bool {
        !self.scarab_of_timelines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-9;

    fn assert_score(got: f64, want: f64, what: &str) {
        assert!(
            (got - want).abs() < EPSILON,
            "{what}: expected {want}, got {got}"
        );
    }

    fn rush() -> StrategyProfile {
        StrategyProfile::locus_doryani_rush()
    }

    // -- the four measured outcomes -------------------------------------------
    // Sebastian's ranking is an outcome ranking with no board attached, so the
    // RD tally is zero in these four; a real board adds its baseline on top.

    #[test]
    fn locus_together_with_doryani_scores_the_combination_value_of_ten() {
        let board = [(Line::Corruption, Tier::T3), (Line::Gem, Tier::T3)];
        assert_score(rush().aggregate(&board, false, 0), 10.0, "Locus + Doryani");
    }

    #[test]
    fn locus_on_its_own_scores_nine() {
        let board = [(Line::Corruption, Tier::T3)];
        assert_score(rush().aggregate(&board, false, 0), 9.0, "Locus alone");
    }

    #[test]
    fn doryani_on_its_own_scores_seven() {
        let board = [(Line::Gem, Tier::T3)];
        assert_score(rush().aggregate(&board, false, 0), 7.0, "Doryani alone");
    }

    #[test]
    fn the_apex_on_its_own_scores_two() {
        assert_score(rush().aggregate(&[], true, 0), 2.0, "Apex alone");
    }

    #[test]
    fn the_apex_beside_locus_adds_only_the_assumed_mixed_increment() {
        let board = [(Line::Corruption, Tier::T3)];
        assert_score(
            rush().aggregate(&board, true, 0),
            9.5,
            "Locus + Apex (9 plus the assumed 0.5)",
        );
    }

    #[test]
    fn doryani_beside_the_apex_scores_seven_point_five() {
        let board = [(Line::Gem, Tier::T3)];
        assert_score(
            rush().aggregate(&board, true, 0),
            7.5,
            "Doryani + Apex (7 plus the assumed 0.5)",
        );
    }

    #[test]
    fn both_target_rooms_plus_the_apex_score_ten_point_five() {
        // The Apex is charged on top of a *matched combination*, not only on
        // top of a room-value sum: the prototype's SCORE_TABLE tops out at
        // 10.5, not at the combination's 10.
        let board = [(Line::Corruption, Tier::T3), (Line::Gem, Tier::T3)];
        assert_score(
            rush().aggregate(&board, true, 0),
            10.5,
            "Locus + Doryani + Apex",
        );
    }

    // -- partial credit --------------------------------------------------------

    #[test]
    fn a_catalyst_of_corruption_scores_nothing_beyond_the_room_baseline() {
        // Tier 2 of the corruption line, connected and built, and worth only
        // what RD pays any built room. Partial credit is essentially none.
        let board = [(Line::Corruption, Tier::T2)];
        assert_score(
            rush().aggregate(&board, false, 1),
            0.05,
            "Catalyst II alone",
        );
    }

    #[test]
    fn a_tier_zero_filler_room_never_reaches_the_score() {
        // Filler carries no line, so it cannot be priced by room_values even
        // when the map hands it the corruption slot. Scored against a profile
        // that DOES pay at tier 1, so an off-by-one on the tier index would
        // surface here rather than hiding behind the Rush's zeroes.
        let mut profile = rush();
        profile
            .room_values
            .insert(Line::Corruption, [3.0, 6.0, 9.0]);
        let board = [(Line::Corruption, Tier::T0)];
        assert_score(profile.aggregate(&board, false, 0), 0.0, "filler room");
    }

    #[test]
    fn a_second_room_of_the_same_line_does_not_pay_twice() {
        let board = [(Line::Gem, Tier::T3), (Line::Gem, Tier::T3)];
        assert_score(
            rush().aggregate(&board, false, 0),
            7.0,
            "two Doryani's Institutes",
        );
    }

    #[test]
    fn a_lower_tier_of_a_line_does_not_drag_down_its_best_room() {
        // Gemcutter's Workshop (tier 1, worth 0) alongside Doryani's Institute
        // must still score the Institute.
        let board = [(Line::Gem, Tier::T1), (Line::Gem, Tier::T3)];
        assert_score(
            rush().aggregate(&board, false, 0),
            7.0,
            "Workshop plus Institute",
        );
    }

    #[test]
    fn the_room_baseline_is_charged_once_per_connected_built_room() {
        let board = [(Line::Corruption, Tier::T3)];
        assert_score(
            rush().aggregate(&board, false, 6),
            9.3,
            "Locus on a board of six built rooms",
        );
    }

    // -- combinations override the sum ----------------------------------------

    #[test]
    fn a_matching_combination_replaces_the_room_value_sum_rather_than_adding_to_it() {
        // The whole reason combinations exist: summing would give 16.
        let board = [(Line::Corruption, Tier::T3), (Line::Gem, Tier::T3)];
        let summed = rush().room_values[&Line::Corruption][2] + rush().room_values[&Line::Gem][2];
        let scored = rush().aggregate(&board, false, 0);
        assert!(
            scored < summed,
            "combination must override the sum: sum {summed}, scored {scored}"
        );
        assert_score(scored, 10.0, "combination score");
    }

    #[test]
    fn the_highest_scoring_matching_combination_wins() {
        let mut profile = rush();
        profile.combinations.push(Combination {
            requires: vec![(Line::Corruption, Tier::T3)],
            score: 4.0,
        });
        let board = [(Line::Corruption, Tier::T3), (Line::Gem, Tier::T3)];
        assert_score(
            profile.aggregate(&board, false, 0),
            10.0,
            "both combinations match; the richer one wins",
        );
    }

    #[test]
    fn a_higher_tier_still_satisfies_a_lower_tier_requirement() {
        let mut profile = rush();
        profile.combinations = vec![Combination {
            requires: vec![(Line::Corruption, Tier::T2)],
            score: 3.0,
        }];
        let board = [(Line::Corruption, Tier::T3)];
        assert_score(
            profile.aggregate(&board, false, 0),
            3.0,
            "Locus III satisfies a Catalyst II requirement",
        );
    }

    #[test]
    fn an_unmet_requirement_leaves_the_combination_out_of_the_running() {
        let board = [(Line::Corruption, Tier::T3), (Line::Gem, Tier::T2)];
        assert_score(
            rush().aggregate(&board, false, 0),
            9.0,
            "Department of Thaumaturgy is not Doryani's Institute",
        );
    }

    #[test]
    fn a_matched_combination_worth_zero_still_counts_as_a_board_that_carries_value() {
        // The Apex pays its full standalone score only when it is all there
        // is. A profile is free to price an outcome at zero, and a matched
        // outcome is still an outcome — so the Apex is a bystander here and
        // pays the mixed increment. Reading "carries value" off the sign of
        // the score instead of off the match would pay the full 2.0.
        let mut profile = rush();
        profile.combinations = vec![Combination {
            requires: vec![(Line::Corruption, Tier::T3)],
            score: 0.0,
        }];
        let board = [(Line::Corruption, Tier::T3)];
        assert_score(
            profile.aggregate(&board, true, 0),
            0.5,
            "a zero-valued outcome beside the Apex",
        );
    }

    // -- the profile is data, not code ----------------------------------------

    #[test]
    fn raising_only_the_apex_score_flips_the_ranking_of_two_boards() {
        // Board A is the bare Apex; board B is Doryani's Institute with no
        // Apex. The Rush prefers B; a profile that differs ONLY in apex_score
        // prefers A. Same aggregate() both times.
        let apex_only: [(Line, Tier); 0] = [];
        let doryani_only = [(Line::Gem, Tier::T3)];

        let rush = rush();
        assert!(
            rush.aggregate(&apex_only, true, 0) < rush.aggregate(&doryani_only, false, 0),
            "the Rush must prefer Doryani's Institute to a bare Apex"
        );

        let mut apex_lover = rush.clone();
        apex_lover.apex_score = 9.0;
        assert!(
            apex_lover.aggregate(&apex_only, true, 0)
                > apex_lover.aggregate(&doryani_only, false, 0),
            "an Apex-weighted profile must prefer the bare Apex"
        );
    }

    #[test]
    fn raising_only_the_mixed_increment_flips_a_ranking_on_boards_that_already_pay() {
        // The bare-Apex board above is the one place `apex_score` is read; on
        // every board that already carries value the Apex weight is
        // `apex_mixed_increment`. Board A is Locus III with the Apex reached,
        // board B is Locus III plus Doryani's Institute without it. The Rush
        // prefers B (10 over 9.5); a profile differing ONLY in the increment
        // prefers A.
        let locus_and_apex = [(Line::Corruption, Tier::T3)];
        let both_rooms = [(Line::Corruption, Tier::T3), (Line::Gem, Tier::T3)];

        let rush = rush();
        assert!(
            rush.aggregate(&locus_and_apex, true, 0) < rush.aggregate(&both_rooms, false, 0),
            "the Rush must prefer both target rooms to Locus beside the Apex"
        );

        let mut apex_lover = rush.clone();
        apex_lover.apex_mixed_increment = 2.0;
        assert!(
            apex_lover.aggregate(&locus_and_apex, true, 0)
                > apex_lover.aggregate(&both_rooms, false, 0),
            "an Apex-weighted profile must prefer Locus beside the Apex"
        );
    }

    // -- traversal cost --------------------------------------------------------

    #[test]
    fn a_positive_path_cost_prefers_the_board_that_is_fewer_hops_away() {
        let board = [(Line::Corruption, Tier::T3)];
        let mut profile = rush();
        profile.path_cost = 0.5;

        let near = profile.aggregate_with_path(&board, false, 0, 3);
        let far = profile.aggregate_with_path(&board, false, 0, 6);

        assert_score(near, 7.5, "Locus three hops in at 0.5/hop");
        assert!(near > far, "near {near} must beat far {far}");
    }

    #[test]
    fn a_zero_path_cost_leaves_two_boards_of_different_length_tied() {
        let board = [(Line::Corruption, Tier::T3)];
        let rush = rush();

        let near = rush.aggregate_with_path(&board, false, 0, 3);
        let far = rush.aggregate_with_path(&board, false, 0, 6);

        assert_score(near, far, "the Rush prices no traversal");
        assert_score(near, 9.0, "and charges nothing against the outcome");
    }

    // -- explosive-charge blend (RE) -------------------------------------------

    #[test]
    fn the_blast_discount_halves_what_charge_only_reachability_adds() {
        // Naturally reachable: 2. Charges bring the Locus into range: 9.
        assert_score(
            rush().blend_blast(2.0, 9.0),
            5.5,
            "half of the 7-point gain, on top of the natural 2",
        );
    }

    #[test]
    fn a_fully_connected_temple_gains_nothing_from_charges() {
        assert_score(
            rush().blend_blast(9.0, 9.0),
            9.0,
            "charges are worthless once everything is already reachable",
        );
    }

    // -- mode selection --------------------------------------------------------

    #[test]
    fn both_target_lines_connected_selects_scarab_mode() {
        // Tier 1 of each — the scarab re-rolls tiers, so only the lines matter.
        let connected = [(Line::Corruption, Tier::T1), (Line::Gem, Tier::T1)];
        assert_eq!(rush().select_mode(&connected), Mode::Scarab);
    }

    #[test]
    fn a_gem_room_the_entrance_cannot_reach_stays_in_chase_mode() {
        // The v1 boundary. Whether the gem room was never built or is built
        // but stranded, the Entrance reaches only the Locus — and v1 prices
        // both the same, because a scarab fired now would copy a temple whose
        // gem room may never be connected.
        let connected = [(Line::Corruption, Tier::T3)];
        assert_eq!(rush().select_mode(&connected), Mode::Chase);
    }

    #[test]
    fn a_connected_tier_zero_slot_does_not_count_as_a_built_line() {
        let connected = [(Line::Corruption, Tier::T3), (Line::Gem, Tier::T0)];
        assert_eq!(rush().select_mode(&connected), Mode::Chase);
    }

    #[test]
    fn the_mode_rule_is_data_so_a_third_required_line_changes_the_verdict() {
        let connected = [(Line::Corruption, Tier::T1), (Line::Gem, Tier::T1)];
        let mut profile = rush();
        profile.mode_rule =
            ModeRule::LinesConnected(vec![Line::Corruption, Line::Gem, Line::Explosive]);
        assert_eq!(profile.select_mode(&connected), Mode::Chase);
    }

    #[test]
    fn an_empty_required_line_set_selects_scarab_on_a_bare_board() {
        // The documented boundary of `all()` over an empty set: no requirement
        // means nothing to chase. No shipped constructor produces this rule,
        // so nothing but this test pins it.
        let mut profile = rush();
        profile.mode_rule = ModeRule::LinesConnected(vec![]);
        assert_eq!(profile.select_mode(&[]), Mode::Scarab);
    }

    // -- line vocabulary -------------------------------------------------------

    #[test]
    fn a_known_key_never_resolves_to_the_open_variant() {
        // Guards the round-trip: Other("corruption") would compare unequal to
        // Line::Corruption and silently score zero.
        assert_eq!(Line::named("corruption"), Line::Corruption);
        assert_eq!(Line::named("gem"), Line::Gem);
        assert_eq!(Line::named("upgrade"), Line::Upgrade);
        assert_eq!(Line::named("explosive"), Line::Explosive);
    }

    #[test]
    fn an_unknown_key_is_kept_verbatim_in_the_open_variant() {
        assert_eq!(
            Line::named("tempest_generator"),
            Line::Other("tempest_generator".to_string())
        );
    }

    // -- tier bounds -----------------------------------------------------------

    #[test]
    fn the_highest_legal_tier_is_three() {
        assert_eq!(Tier::new(3).map(Tier::get), Some(3));
    }

    #[test]
    fn a_tier_above_three_is_rejected() {
        assert_eq!(Tier::new(4), None);
    }

    #[test]
    fn deserialising_a_tier_above_three_fails_rather_than_truncating() {
        let error = serde_json::from_str::<Tier>("4").expect_err("tier 4 must be rejected");
        assert!(
            error.to_string().contains("outside 0..=3"),
            "unhelpful message: {error}"
        );
    }

    // -- persistence -----------------------------------------------------------

    #[test]
    fn a_profile_survives_a_json_round_trip_with_line_names_as_map_keys() {
        // room_values is keyed by Line; the derived enum encoding would make
        // that map unserialisable, so this is the guard on the hand-written
        // Serialize impl.
        let profile = rush();
        let json = serde_json::to_string(&profile).expect("profile serialises");
        assert!(
            json.contains("\"corruption\":["),
            "line keys must be bare strings: {json}"
        );

        let restored: StrategyProfile = serde_json::from_str(&json).expect("profile parses back");
        assert_eq!(restored, profile);
    }

    // -- config ----------------------------------------------------------------

    #[test]
    fn artefacts_of_the_vaal_gives_four_entrances_per_map() {
        let config = TempleConfig {
            artefacts_of_the_vaal: true,
            ..TempleConfig::default()
        };
        assert_eq!(config.entrances_per_map(), 4);
    }

    #[test]
    fn without_artefacts_of_the_vaal_a_map_has_three_entrances() {
        let config = TempleConfig {
            artefacts_of_the_vaal: false,
            ..TempleConfig::default()
        };
        assert_eq!(config.entrances_per_map(), 3);
    }

    #[test]
    fn the_default_configuration_assumes_artefacts_of_the_vaal_is_taken() {
        assert_eq!(TempleConfig::default().entrances_per_map(), 4);
    }

    #[test]
    fn r5_applies_while_the_timelines_scarab_is_off() {
        let config = TempleConfig {
            scarab_of_timelines: false,
            ..TempleConfig::default()
        };
        assert!(config.r5_applies());
    }

    #[test]
    fn the_timelines_scarab_disables_r5() {
        let config = TempleConfig {
            scarab_of_timelines: true,
            ..TempleConfig::default()
        };
        assert!(!config.r5_applies());
    }
}
