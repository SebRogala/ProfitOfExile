//! The two valuation presets, and the table the second one keeps (POE-257).
//!
//! Epic lock L3 (POE-124): **Default and Custom, and no third**. Default is
//! Vertolka's drop table plus the live market read, priced with the shipped
//! [`Knobs`]. Custom is the same formula with the player's own rates, plus a
//! per-room-tier override table for the rooms they disagree with outright.
//!
//! # What is persisted, and why the two halves are separate
//!
//! The preset CHOICE and the Custom TABLE are two settings fields, not one.
//! The whole point of the pair is that switching Default -> Custom -> Default
//! -> Custom returns the player's numbers unchanged, which only works if the
//! table survives a period of not being in force. Folding them into one
//! "active preset" blob would delete the table the moment the player looked at
//! Default.
//!
//! # A malformed entry costs its own room and nothing else
//!
//! `settings.json` is hand-editable, and the temple block already had the
//! opposite rule: one bad field rejected the WHOLE profile
//! (`settings::apply_to_state`). That is right for the four scalars — a
//! negative `path_cost` is a sign error, and a scorer running on half a
//! profile is worse than one running on the default. It is wrong for a table
//! of 25 independent numbers, where the other 24 are still exactly what the
//! player meant. [`TempleCustomSettings::overrides`] refuses the offending
//! entry, names it, and loads the rest.
//!
//! # No rate is invented here
//!
//! [`Knobs`] lives in [`super::valuation`] and is re-exported, never
//! redefined: the formula owns its own parameters, and a second declaration of
//! the same five numbers is a second set of defaults to drift.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::market::MarketInput;
use super::rooms::LINES;
use super::strategy::Line;
use super::valuation::{RoomOverrides, Valued};

pub use super::valuation::Knobs;

// -------------------------------------------------------------- the preset --

/// Which valuation the advisor and the overlay are reading.
///
/// `snake_case` on the wire — `"default"`, `"custom"` — which is this app's
/// convention for enum VARIANTS (`desktop/src/lib/README.md`); the structs
/// around it rename their FIELDS to camelCase, which is a different rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Vertolka's table plus the market, with the shipped rates.
    #[default]
    Default,
    /// The player's own rates and their own per-room numbers.
    Custom,
}

// --------------------------------------------------------- the custom table --

/// The Custom preset's rates and per-room overrides, as they are persisted.
///
/// # Wire shape
///
/// `camelCase`, like every other temple settings struct and unlike the rest of
/// `settings.json` — see `settings::Settings::temple_profile` for why that
/// deviation is one block of the file rather than a DTO.
///
/// # `rooms` is a `Vec`, not a `[_; 3]`
///
/// Deliberate, and the whole reason the per-room fallback can exist: an array
/// arity is checked by serde, so one four-element entry in a hand-edited file
/// would fail the WHOLE field and take the other 24 rooms with it. A `Vec`
/// carries the malformed entry as far as
/// [`Self::overrides`], which refuses that entry by name and keeps the rest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TempleCustomSettings {
    /// [`Knobs::tier_fraction`].
    pub tier_fraction: f64,
    /// [`Knobs::c_per_quantity`].
    pub c_per_quantity: f64,
    /// [`Knobs::c_per_rarity`].
    pub c_per_rarity: f64,
    /// [`Knobs::drops_weight`]. A rusher sets this to `0`, which reduces the
    /// ranking to sale value alone.
    pub drops_weight: f64,
    /// [`Knobs::combo_premium`].
    pub combo_premium: f64,
    /// Per-room-tier chaos overrides, keyed by
    /// [`RoomLine::key`](super::rooms::RoomLine::key), tier 1 first. `null` in
    /// a slot means "use the formula for this tier"; a number replaces it.
    pub rooms: BTreeMap<String, Vec<Option<f64>>>,
}

impl Default for TempleCustomSettings {
    /// The shipped rates and no overrides — read off [`Knobs::default`] rather
    /// than typed in, so the two cannot drift.
    fn default() -> TempleCustomSettings {
        let knobs = Knobs::default();
        TempleCustomSettings {
            tier_fraction: knobs.tier_fraction,
            c_per_quantity: knobs.c_per_quantity,
            c_per_rarity: knobs.c_per_rarity,
            drops_weight: knobs.drops_weight,
            combo_premium: knobs.combo_premium,
            rooms: BTreeMap::new(),
        }
    }
}

/// How many tiers an override entry states. The game has three, and an entry
/// of any other length is not a room's worth of numbers.
const OVERRIDE_TIERS: usize = 3;

impl TempleCustomSettings {
    /// The five rates as the formula takes them.
    ///
    /// No validation here, and the reason differs per knob. The four the
    /// FORMULA reads — `tier_fraction` and the three rate-like ones — are
    /// floored where they are read (`valuation::rate`, `valuation::tier_fraction`),
    /// which is one place rather than two. `combo_premium` is the odd one out:
    /// the formula never reads it, so its only reader is
    /// `TempleProfileSettings::to_profile`, and that is where a malformed one
    /// is refused.
    pub fn knobs(&self) -> Knobs {
        Knobs {
            tier_fraction: self.tier_fraction,
            c_per_quantity: self.c_per_quantity,
            c_per_rarity: self.c_per_rarity,
            drops_weight: self.drops_weight,
            combo_premium: self.combo_premium,
        }
    }

    /// The overrides this table states, and one line per entry it could not
    /// accept.
    ///
    /// Three refusals, all per entry and none fatal to the table:
    ///
    /// - a key that is not one of the 25 room lines — this build has nothing
    ///   to apply it to;
    /// - an entry that does not state exactly [`OVERRIDE_TIERS`] values — a
    ///   list of four numbers does not say which tier is which;
    /// - a value that is not a chaos amount (`NaN`, infinite, negative) — that
    ///   SLOT falls back to the formula while the entry's other tiers stand.
    ///
    /// Zero is accepted throughout: "this room is worth nothing to me" is a
    /// position a player can hold, and it is the one a rusher holds about most
    /// of the board.
    ///
    /// # One warning that refuses nothing
    ///
    /// Both target lines priced at zero. `mode_rule` and RV name `corruption`
    /// and `gem` structurally, not by their value, so zeroing both stops them
    /// scoring without stopping the advisor protecting them — a number that
    /// will be HONOURED and will not do what the player expects, which is
    /// worse than one that is refused.
    ///
    /// There is deliberately NO warning about the two
    /// [`INSTRUMENTAL_LINES`](super::strategy::INSTRUMENTAL_LINES) any more.
    /// An override on Temple Nexus or Shrine of Unmaking now reaches the
    /// ranking exactly like any other room's, because the bridge copies those
    /// two lines out of the valuation instead of zeroing them.
    ///
    /// The warnings are returned rather than logged so the caller can put them
    /// where the user will see them — `settings::apply_to_state` puts them in
    /// the same `rejected` list every other refused setting reports through.
    pub fn overrides(&self) -> (RoomOverrides, Vec<String>) {
        let mut out = RoomOverrides::new();
        let mut warnings = Vec::new();

        for (key, values) in &self.rooms {
            if !LINES.iter().any(|line| line.key() == key) {
                warnings.push(format!(
                    "temple custom: {key:?} is not a room line, ignoring its values"
                ));
                continue;
            }
            if values.len() != OVERRIDE_TIERS {
                warnings.push(format!(
                    "temple custom: {key:?} states {} values, not {OVERRIDE_TIERS}, ignoring it",
                    values.len()
                ));
                continue;
            }

            let mut row = [None; OVERRIDE_TIERS];
            for (index, value) in values.iter().enumerate() {
                match value {
                    Some(chaos) if chaos.is_finite() && *chaos >= 0.0 => row[index] = Some(*chaos),
                    Some(chaos) => warnings.push(format!(
                        "temple custom: {key:?} tier {} is {chaos}, not a chaos amount — \
                         using the computed value for it",
                        index + 1
                    )),
                    None => {}
                }
            }
            if row.iter().any(Option::is_some) {
                out.insert(key.clone(), row);
            }
        }

        if self.both_target_lines_priced_at_zero() {
            warnings.push(
                "temple custom: \"corruption\" and \"gem\" are both priced 0 at tier 3 — \
                 RV and the scarab-mode rule still protect those two lines, so zeroing them \
                 stops them scoring but does not stop the advisor chasing them"
                    .to_string(),
            );
        }

        (out, warnings)
    }

    /// Whether the table states an explicit zero for BOTH target lines'
    /// tier-3 rooms.
    ///
    /// A stated zero only — a line the table does not name is on the formula
    /// and is not the player claiming anything about it.
    fn both_target_lines_priced_at_zero(&self) -> bool {
        [Line::Corruption, Line::Gem].iter().all(|line| {
            self.rooms
                .get(line.key())
                .is_some_and(|values| values.len() == OVERRIDE_TIERS && values[2] == Some(0.0))
        })
    }
}

// -------------------------------------------------------------- the tables --

/// The 25 x 3 table the active preset produces for one market read.
///
/// ONE call per read (POE-257 D6): the advisor ranks on the returned table and
/// the offer views are projected from the same object, so the number shown is
/// the number ranked.
///
/// The warnings [`TempleCustomSettings::overrides`] produces are dropped here
/// on purpose. This runs on every read; the place a refused entry has to be
/// reported is the load, once, where a user can act on it.
pub fn value_table(preset: Preset, custom: &TempleCustomSettings, market: &MarketInput) -> Valued {
    match preset {
        Preset::Default => Valued::compute(market, &Knobs::default()),
        Preset::Custom => {
            let (overrides, _) = custom.overrides();
            Valued::compute_with(market, &custom.knobs(), &overrides)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::market::allflame;
    use crate::temple::strategy::Tier;

    fn tier(n: u8) -> Tier {
        Tier::new(n).expect("test tier is 0..=3")
    }

    fn total(valued: &Valued, key: &str, n: u8) -> f64 {
        valued
            .get(key, tier(n))
            .unwrap_or_else(|| panic!("no value for {key} tier {n}"))
            .total
    }

    #[test]
    fn the_default_preset_ignores_the_custom_table_entirely() {
        // Custom carries a table AND rates that would change every room; the
        // Default preset must produce the shipped valuation regardless.
        let custom = TempleCustomSettings {
            drops_weight: 0.0,
            rooms: BTreeMap::from([("corruption".to_string(), vec![None, None, Some(1.0)])]),
            ..Default::default()
        };
        let market = allflame();

        let default = value_table(Preset::Default, &custom, &market);

        assert_eq!(
            total(&default, "corruption", 3),
            total(
                &Valued::compute(&market, &Knobs::default()),
                "corruption",
                3
            ),
        );
    }

    #[test]
    fn a_custom_override_replaces_the_formula_for_the_tier_it_names() {
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([("corruption".to_string(), vec![None, None, Some(100.0)])]),
            ..Default::default()
        };

        let valued = value_table(Preset::Custom, &custom, &allflame());

        assert_eq!(total(&valued, "corruption", 3), 100.0);
    }

    #[test]
    fn a_tier_three_override_carries_down_to_the_tiers_that_state_none() {
        // Epic lock L2: tier 1 and tier 2 are `tier_fraction` of whatever tier
        // 3 is worth. Overriding tier 3 and not the others must therefore move
        // all three rows, not just the one named.
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([("corruption".to_string(), vec![None, None, Some(100.0)])]),
            ..Default::default()
        };

        let valued = value_table(Preset::Custom, &custom, &allflame());

        assert_eq!(total(&valued, "corruption", 1), 80.0);
        assert_eq!(total(&valued, "corruption", 2), 80.0);
    }

    #[test]
    fn a_tier_one_override_moves_nothing_but_itself() {
        let market = allflame();
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([("corruption".to_string(), vec![Some(7.0), None, None])]),
            ..Default::default()
        };
        let shipped = Valued::compute(&market, &Knobs::default());

        let valued = value_table(Preset::Custom, &custom, &market);

        assert_eq!(total(&valued, "corruption", 1), 7.0);
        assert_eq!(
            total(&valued, "corruption", 3),
            total(&shipped, "corruption", 3),
        );
    }

    #[test]
    fn the_custom_rates_price_every_room_the_table_does_not_name() {
        // `drops_weight = 0` is the rusher's setting: the ranking collapses to
        // sale value plus bonus. Locus keeps its 846 c sale delta and loses
        // nothing else, because it has no priced drop term on this capture.
        let market = allflame();
        let custom = TempleCustomSettings {
            drops_weight: 0.0,
            ..Default::default()
        };

        let valued = value_table(Preset::Custom, &custom, &market);

        assert_eq!(total(&valued, "corruption", 3), 846.0);
        assert!(
            total(&valued, "corruption", 3) > total(&valued, "gem", 3),
            "the feed's own order survives a zero drops weight",
        );
    }

    #[test]
    fn a_zero_override_is_accepted_as_the_position_it_is() {
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([("corruption".to_string(), vec![None, None, Some(0.0)])]),
            ..Default::default()
        };

        let (overrides, warnings) = custom.overrides();

        assert_eq!(overrides.get("corruption"), Some(&[None, None, Some(0.0)]));
        assert!(warnings.is_empty(), "zero is a number, not a malformation");
        assert_eq!(
            total(
                &value_table(Preset::Custom, &custom, &allflame()),
                "corruption",
                3
            ),
            0.0
        );
    }

    #[test]
    fn a_negative_value_costs_its_own_tier_and_no_other() {
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([(
                "corruption".to_string(),
                vec![Some(-5.0), None, Some(100.0)],
            )]),
            ..Default::default()
        };

        let (overrides, warnings) = custom.overrides();

        assert_eq!(
            overrides.get("corruption"),
            Some(&[None, None, Some(100.0)])
        );
        assert_eq!(warnings.len(), 1, "one refusal, got {warnings:?}");
        assert!(
            warnings[0].contains("corruption") && warnings[0].contains("tier 1"),
            "the warning names the offending key and tier, got {warnings:?}",
        );
        // The refused tier falls back to the formula, which is 80 % of the
        // surviving tier-3 override.
        let valued = value_table(Preset::Custom, &custom, &allflame());
        assert_eq!(total(&valued, "corruption", 1), 80.0);
        assert_eq!(total(&valued, "corruption", 3), 100.0);
    }

    #[test]
    fn a_wrong_arity_entry_costs_its_own_room_and_the_rest_of_the_table_loads() {
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([
                ("corruption".to_string(), vec![Some(1.0), Some(2.0)]),
                ("gem".to_string(), vec![None, None, Some(50.0)]),
            ]),
            ..Default::default()
        };

        let (overrides, warnings) = custom.overrides();

        assert!(
            overrides.get("corruption").is_none(),
            "the short entry is refused"
        );
        assert_eq!(overrides.get("gem"), Some(&[None, None, Some(50.0)]));
        assert_eq!(warnings.len(), 1, "one refusal, got {warnings:?}");
        assert!(
            warnings[0].contains("corruption") && warnings[0].contains("2 values"),
            "the warning names the key and what was wrong with it, got {warnings:?}",
        );
        // And the survivor really is in force, while the refused room is back
        // on the formula rather than at zero.
        let valued = value_table(Preset::Custom, &custom, &allflame());
        assert_eq!(total(&valued, "gem", 3), 50.0);
        assert_eq!(total(&valued, "corruption", 3), 846.0);
    }

    #[test]
    fn a_key_this_build_does_not_know_is_refused_by_name() {
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([(
                "locus_of_corruption".to_string(),
                vec![None, None, Some(9.0)],
            )]),
            ..Default::default()
        };

        let (overrides, warnings) = custom.overrides();

        assert!(overrides.is_empty());
        assert_eq!(warnings.len(), 1, "got {warnings:?}");
        assert!(
            warnings[0].contains("locus_of_corruption"),
            "the warning names the key, got {warnings:?}",
        );
    }

    #[test]
    fn a_non_finite_value_never_reaches_the_valuation() {
        // Not reachable through JSON — `NaN` is not a JSON number — but it is
        // reachable through the setter command a UI will call, and one NaN in
        // `room_values` makes the whole ranking's float ordering arbitrary.
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([("gem".to_string(), vec![None, None, Some(f64::NAN)])]),
            ..Default::default()
        };

        let (overrides, warnings) = custom.overrides();

        assert!(overrides.is_empty());
        assert_eq!(warnings.len(), 1, "got {warnings:?}");
        assert!(total(&value_table(Preset::Custom, &custom, &allflame()), "gem", 3).is_finite());
    }

    /// An override on an instrumental line is kept and NOT warned about.
    ///
    /// It used to be warned about, because the bridge zeroed those two lines
    /// whatever the table said. Since the board-7 fix the valuation prices
    /// them at their grade rung and the bridge copies that, so a player's own
    /// number for Temple Nexus reaches the ranking exactly like any other
    /// room's — and a warning saying otherwise would now be false.
    ///
    /// Fails if the warning comes back, and fails if the entry stops being
    /// honoured.
    #[test]
    fn an_override_on_an_instrumental_line_is_kept_without_a_warning() {
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([("upgrade".to_string(), vec![None, None, Some(500.0)])]),
            ..Default::default()
        };

        let (overrides, warnings) = custom.overrides();

        assert_eq!(overrides.get("upgrade"), Some(&[None, None, Some(500.0)]));
        assert!(warnings.is_empty(), "got {warnings:?}");
        assert_eq!(
            total(
                &value_table(Preset::Custom, &custom, &allflame()),
                "upgrade",
                3
            ),
            500.0,
        );
    }

    /// Pricing BOTH target lines at zero warns that the advisor still chases
    /// them.
    ///
    /// `mode_rule` and RV name `corruption` and `gem` structurally, not by
    /// their value: the mode flip and the hard "do not bank an unreachable
    /// target" constraint read the two line identities. So a table that zeroes
    /// both stops them scoring without stopping the advisor protecting them,
    /// and a player who did it on purpose is owed that sentence rather than a
    /// board they cannot explain.
    ///
    /// Fails if the check is dropped, and fails if it is loosened to either
    /// line — see the sibling test.
    #[test]
    fn pricing_both_target_lines_at_zero_warns_that_they_are_still_protected() {
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([
                ("corruption".to_string(), vec![None, None, Some(0.0)]),
                ("gem".to_string(), vec![None, None, Some(0.0)]),
            ]),
            ..Default::default()
        };

        let (overrides, warnings) = custom.overrides();

        assert_eq!(overrides.len(), 2, "both entries are honoured");
        assert_eq!(warnings.len(), 1, "got {warnings:?}");
        assert!(
            warnings[0].contains("RV") && warnings[0].contains("scarab-mode"),
            "the warning names what still reads the two lines: {warnings:?}",
        );
    }

    /// Zeroing ONE target line is not the case that warning is about.
    ///
    /// A rusher who prices the gem line at nothing and keeps the corruption
    /// line is stating a strategy, not walking into a gap: the advisor still
    /// scores the line he cares about. Fails if the check is loosened from
    /// "both" to "either", which would warn on an ordinary rusher table.
    #[test]
    fn zeroing_one_target_line_alone_produces_no_warning() {
        let custom = TempleCustomSettings {
            rooms: BTreeMap::from([("gem".to_string(), vec![None, None, Some(0.0)])]),
            ..Default::default()
        };

        let (_, warnings) = custom.overrides();

        assert!(warnings.is_empty(), "got {warnings:?}");
    }

    #[test]
    fn the_shipped_defaults_are_the_formulas_own_knobs() {
        // Both halves: the five values are the ones POE-257 shipped, and each
        // one arrives on its own field rather than on a neighbour's — a
        // crossed pair would leave `Knobs::default()` equal to itself and this
        // assertion is what refuses it.
        let knobs = TempleCustomSettings::default().knobs();

        assert_eq!(knobs, Knobs::default());
        assert_eq!(knobs.tier_fraction, 0.8);
        assert_eq!(knobs.c_per_quantity, 0.5);
        assert_eq!(knobs.c_per_rarity, 0.25);
        assert_eq!(knobs.drops_weight, 1.0);
        assert_eq!(knobs.combo_premium, 0.0);
    }

    #[test]
    fn the_preset_choice_is_a_snake_case_wire_string() {
        // The slice publishes this verbatim and the webview mirror spells the
        // same two literals. A `rename_all` dropped here would send the page
        // `"Custom"`, which its union has no branch for.
        assert_eq!(
            serde_json::to_string(&Preset::Custom).expect("a preset serialises"),
            "\"custom\"",
        );
        assert_eq!(
            serde_json::to_string(&Preset::default()).expect("a preset serialises"),
            "\"default\"",
        );
    }
}
