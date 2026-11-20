use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::preset::{Preset, TempleCustomSettings};
use super::super::rooms;
use super::super::strategy::{Combination, Line, StrategyProfile, TempleConfig, Tier};
use super::super::valuation::Valued;

// ---------------------------------------------------------------- settings --

/// The profile fields a user may set (POE-167 §4: *everything a player might
/// disagree on is a profile field, never a code branch*).
///
/// Four of [`StrategyProfile`]'s fields, not the whole struct — and since
/// POE-257 the other three no longer divide the way this doc used to say.
///
/// `room_values` and `combinations` ARE settings-fed now, just not from this
/// struct: the board is priced in chaos from a market read plus the Custom
/// preset's own override table and `combo_premium`
/// ([`Self::to_profile`], `temple/preset.rs`, ADR-022). What used to read
/// "those arrive as a second shipped profile, not as settings JSON" holds for
/// `mode_rule` alone — it names the two lines the build chases, and a user
/// editing it is choosing a different strategy rather than tuning this one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TempleProfileSettings {
    /// What the Apex is worth on its own, **in units where the top tier-3 room
    /// is worth 9** ([`REFERENCE_TOP_ROOM_VALUE`](super::strategy::REFERENCE_TOP_ROOM_VALUE)).
    ///
    /// Relative, not chaos, and deliberately so: 2.0 means "the Apex is worth
    /// a bit under a quarter of the best room on the board", which is a
    /// judgement that survives the currency moving and a league ending. The
    /// bridge multiplies it by
    /// [`StrategyProfile::value_scale`](super::strategy::StrategyProfile::value_scale)
    /// so the same 2.0 is 188 c beside a top room worth 846. POE-259's control
    /// must label it in these units — a slider that reads "2" next to a board
    /// of three-figure chaos numbers is a slider nobody can set.
    pub apex_score: f64,
    /// Run-time traversal weight per BFS hop from the Entrance, in the SAME
    /// relative units as [`Self::apex_score`]: 9 is the top tier-3 room. 0 for
    /// the Rush (rush in, grab the room, leave).
    pub path_cost: f64,
    /// Prefer `change` over `upgrade` while no favourable line exists.
    pub reroll_until_favourable: bool,
    /// R4's carve-out: keep a slot in the drop pool while an adjacent tier-1/2
    /// upgrade room can still hit it (never for a Temple Nexus).
    pub r4_keep_upgrade_targets: bool,
}

impl Default for TempleProfileSettings {
    /// The Rush's own values, read off [`StrategyProfile::locus_doryani_rush`]
    /// rather than typed in — a defaults table that can drift from the profile
    /// it defaults to is a defaults table that will.
    fn default() -> Self {
        let rush = StrategyProfile::locus_doryani_rush();
        TempleProfileSettings {
            apex_score: rush.apex_score,
            path_cost: rush.path_cost,
            reroll_until_favourable: rush.reroll_until_favourable,
            r4_keep_upgrade_targets: rush.r4_keep_upgrade_targets,
        }
    }
}

/// The opening stones the player has in hand when the sheet is read: one.
///
/// The advisor keeps its `keys` parameter, and since POE-253 this constant is
/// its only production value. There is no setting behind it: stones drop from
/// the kill INSIDE the incursion, after the sheet has been read, so a count
/// asked for up front is a prediction the player cannot make. A second stone
/// in hand is `advisor::conditional_second_door`'s answer — the faint
/// second door it draws is what that case looks like on screen.
pub const KEYS_IN_HAND: u8 = 1;

impl TempleProfileSettings {
    /// The Rush's structure, priced in chaos from `valuation` (POE-257 D5).
    ///
    /// Three things happen here, and the third is the one that is easy to
    /// miss.
    ///
    /// 1. **`room_values` becomes the whole board.** All 25 lines at all three
    ///    tiers, in chaos, keyed through
    ///    [`RoomLine::mechanical_line`](super::rooms::RoomLine::mechanical_line)
    ///    so the four mechanical keys resolve to their own variants and the
    ///    other 21 to `Line::Other(key)`. No exceptions, including the two
    ///    [`INSTRUMENTAL_LINES`](super::strategy::INSTRUMENTAL_LINES): the
    ///    valuation prices those at their grade rung
    ///    (`valuation::instrumental` — Temple Nexus 20 cold, 21.15 on the
    ///    capture) and the bridge copies that like any other line, so the
    ///    number the box shows is the number the ranking used for every one of
    ///    the 25.
    /// 2. **The combination is gone by default.** The orchestrator settled
    ///    that Locus + Doryani is worth the sum of the two above-floor deltas
    ///    and no more, and the per-room sum already IS that sum. A
    ///    [`Combination`] REPLACES the sum rather than adding to it, so
    ///    emitting one worth exactly the sum would be a no-op on a two-room
    ///    board and a silent deletion of every other line's value on a real
    ///    one. Only a non-zero [`Knobs::combo_premium`](super::preset::Knobs::combo_premium)
    ///    brings it back — and it carries that same replacement rule with it,
    ///    which is the price of stating a premium at all.
    /// 3. **Every absolute magnitude is rescaled.** `apex_score`,
    ///    `apex_mixed_increment`, `room_baseline` and `path_cost` were all
    ///    chosen against a top room worth 9 (see
    ///    [`REFERENCE_TOP_ROOM_VALUE`](super::strategy::REFERENCE_TOP_ROOM_VALUE));
    ///    left alone next to a top room worth 846 they would be dust, and RD
    ///    would stop being able to say "open something beats opening nothing".
    ///    `blast_discount` is a fraction of a score difference and is left
    ///    exactly as it is, and so is `mode_rule`, which names lines rather
    ///    than pricing them.
    ///
    /// # The two instrumental lines are paid twice, on purpose
    ///
    /// `rollout::UPGRADE_TARGETS` and `CHARGES` already credit what the
    /// upgrade and explosives lines DO — the tiers they lift, the rooms they
    /// clear — and this bridge now also copies their grade rung into
    /// `room_values`. That is a double payment, and it is the accepted trade,
    /// with retrospective board 7 as the evidence: while those two lines
    /// scored zero, the app recommended *upgrade to Armoury* (Chamber of Iron,
    /// 3 c of rarity bonus since POE-369) over Sebastian's *change to Sanctum of Unity*,
    /// on the cold board and on the capture alike. The mechanical credit alone
    /// did not survive a noise band 39 c wide. The alternative — valuing a
    /// `change` onto an instrumental line by its own downstream effect — needs
    /// the board's upgrade-reachable set at scoring time, which `room_values`
    /// has no shape for; it is the better model and a larger change.
    ///
    /// What the rung is NOT is the room's own drops: `valuation::instrumental`
    /// deliberately refuses to sum Temple Nexus's own area bonus (3 c of
    /// rarity since POE-369, 12 % x 0.25), which is the number that produced the board-7 miss in the first place.
    ///
    /// # `mode_rule` is not a valuation and does not become one
    ///
    /// It stays [`ModeRule::LinesConnected`](super::strategy::ModeRule)
    /// `[Corruption, Gem]` whatever the player prices those two lines at. The
    /// rule decides when the build flips from chasing to scarab-farming (a flip
    /// held inactive in 3.30, POE-377), and
    /// RV — the hard constraint that refuses to bank an unreachable target —
    /// reads the same two lines. A Custom table that prices both of them at
    /// zero therefore does NOT stop the advisor protecting them; it only stops
    /// them scoring. That is a real gap between what the table says and what
    /// the advisor does, so
    /// [`TempleCustomSettings::overrides`](super::preset::TempleCustomSettings::overrides)
    /// warns about exactly that pair rather than letting the player discover
    /// it on a board.
    ///
    /// [`StrategyProfile::locus_doryani_rush`] is untouched and stays the
    /// fixture the pinned 10/9/7/2 suites rank with.
    pub fn to_profile(&self, valuation: &Valued) -> StrategyProfile {
        let rush = StrategyProfile::locus_doryani_rush();

        let mut room_values = BTreeMap::new();
        let mut grade_fallback = BTreeMap::new();
        for line in rooms::LINES.iter() {
            grade_fallback.insert(
                line.mechanical_line(),
                [Tier::T1, Tier::T2, Tier::T3].map(|tier| {
                    valuation
                        .get(line.key(), tier)
                        .is_some_and(|value| value.priced == super::super::valuation::Priced::Fallback)
                }),
            );
            room_values.insert(
                line.mechanical_line(),
                [
                    tier_total(valuation, line.key(), Tier::T1),
                    tier_total(valuation, line.key(), Tier::T2),
                    tier_total(valuation, line.key(), Tier::T3),
                ],
            );
        }

        // Only a POSITIVE premium builds a combination. Zero is the default
        // and means "the pair is the sum of its two deltas", which the
        // per-room sum already says. Negative is refused rather than honoured:
        // a `Combination` REPLACES the sum, so a premium of -500 would make
        // reaching Locus AND Doryani score less than reaching Locus alone —
        // an inversion no player is asking for, reachable only from a
        // hand-edited file, and not a thing the UI can express. Non-finite is
        // refused for the reason every float in the ranking is checked: one
        // `NaN` makes the whole ordering arbitrary.
        let premium = valuation.knobs().combo_premium;
        let combinations = if premium.is_finite() && premium > 0.0 {
            vec![Combination {
                requires: vec![(Line::Corruption, Tier::T3), (Line::Gem, Tier::T3)],
                score: tier_total(valuation, "corruption", Tier::T3)
                    + tier_total(valuation, "gem", Tier::T3)
                    + premium,
            }]
        } else {
            Vec::new()
        };

        // Built first so the scale is read off the values that will actually
        // be ranked, rather than from a second copy of the same maximum.
        let scaled = StrategyProfile {
            room_values,
            grade_fallback,
            combinations,
            ..rush.clone()
        };
        let scale = scaled.value_scale();

        StrategyProfile {
            apex_score: self.apex_score * scale,
            apex_mixed_increment: rush.apex_mixed_increment * scale,
            room_baseline: rush.room_baseline * scale,
            path_cost: self.path_cost * scale,
            reroll_until_favourable: self.reroll_until_favourable,
            r4_keep_upgrade_targets: self.r4_keep_upgrade_targets,
            ..scaled
        }
    }

    /// Reject a profile the scorer cannot use.
    ///
    /// Both weights are *magnitudes* the objective function adds and
    /// subtracts. A negative `apex_score` would make the Apex a penalty and a
    /// negative `path_cost` would pay the player to walk further — neither is
    /// a preference, both are a sign error. NaN is rejected for the reason
    /// every comparison in the ranking is a float compare: one NaN makes the
    /// whole ordering arbitrary.
    pub fn validate(&self) -> Result<(), String> {
        if !self.apex_score.is_finite() || self.apex_score < 0.0 {
            return Err(format!(
                "apex_score must be a finite number ≥ 0 in units where the top \
                 tier-3 room is worth 9, got {}",
                self.apex_score
            ));
        }
        if !self.path_cost.is_finite() || self.path_cost < 0.0 {
            return Err(format!(
                "path_cost must be a finite number ≥ 0 in units where the top \
                 tier-3 room is worth 9, got {}",
                self.path_cost
            ));
        }
        Ok(())
    }
}

/// One room-tier's chaos total, or `0.0`.
///
/// The zero is not a default standing in for a missing number: [`Valued`]
/// tables every one of the 25 keys at all three tiers by construction, so a
/// miss here would be a key that is not a room line — a line the board can
/// never hold, and therefore worth nothing on it.
fn tier_total(valuation: &Valued, key: &str, tier: Tier) -> f64 {
    valuation.get(key, tier).map_or(0.0, |value| value.total)
}

/// Everything the temple module persists — the user's profile tuning and the
/// two config flags.
///
/// One `AppState` Mutex holds this whole struct while `settings.json` keeps two
/// separate fields, so a hand-edited file stays readable and one bad field
/// defaults on its own (`#[serde(default)]` per field on `Settings`).
///
/// **No calibration field.** It held one until POE-234 WI-2 — the remembered
/// anchor scale, keyed on the capture size — and that was the app's SECOND
/// store of one fact: what scale this screen draws the game's UI at. The shared
/// `crate::ssot::ScreenSlice` is the first and now the only one; the temple
/// derives its hint from it through `anchor::TEMPLE_SCALE_PER_UI_SCALE` on every
/// tick (`super::run::hint_for_capture`) and publishes what it anchors back.
/// [`AnchorCalibration`] survives as what it always described — a (screen size,
/// scale) pair produced BY a read — on [`super::reader::TempleLayout`],
/// [`super::anchor::CheapHint`] and [`TempleSlice::calibration`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TempleSettings {
    pub profile: TempleProfileSettings,
    pub config: TempleConfig,
    /// Which valuation is in force (POE-257 D4).
    pub preset: Preset,
    /// The Custom preset's rates and per-room overrides — kept whether or not
    /// Custom is the preset in force, which is the whole reason it is a field
    /// of its own: switching away and back has to return the player's numbers
    /// unchanged.
    pub custom: TempleCustomSettings,
}
