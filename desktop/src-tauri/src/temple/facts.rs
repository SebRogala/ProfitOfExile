//! Temple game rules used by offer resolution and the production rollout.
//!
//! Sources, dates and validation status live in `docs/GAME-FACTS.md` (POE-415).

use super::rooms::{CRADLE_LINE_KEY, OfferKind};
use super::strategy::{Consolidation, Tier};

/// Operator note 6 (POE-415); validation record: `docs/GAME-FACTS.md`.
pub fn always_tier_three(line_key: &str) -> bool {
    line_key == CRADLE_LINE_KEY
}

/// The kill's landing tier; inferred rules are recorded in `docs/GAME-FACTS.md`.
/// An unread current tier yields no answer only when the selected row needs it.
pub fn landing_tier(
    option: Consolidation,
    current: Option<Tier>,
    kind: OfferKind,
    built_line: Option<&str>,
) -> Option<Tier> {
    if built_line.is_some_and(always_tier_three) {
        return Some(Tier::T3);
    }
    if current == Some(Tier::T0) {
        return Some(Tier::T1);
    }
    match (option, kind) {
        (Consolidation::NonResidentExtraTier | Consolidation::NotTaken, OfferKind::Change) => {
            Some(Tier::T1)
        }
        (Consolidation::NonResidentExtraTier, OfferKind::Upgrade) => {
            Tier::new((current?.get() + 2).min(Tier::MAX_VALUE))
        }
        _ => Tier::new((current?.get() + 1).min(Tier::MAX_VALUE)),
    }
}

/// Decode an upgrade's printed tier; source: `docs/GAME-FACTS.md`.
pub fn current_tier_from_upgrade_print(printed: Tier) -> Tier {
    Tier::new(printed.get().saturating_sub(1)).expect("a printed tier is bounded")
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::strategy::TempleConfig;

    mod keep_happy {
        use super::*;

        #[test]
        fn consolidation_lands_each_kill_at_its_ruled_tier() {
            use Consolidation::*;
            use OfferKind::*;
            // Current, upgrade, change: literal acceptance tiers.
            let rows = [
                (NonResidentExtraTier, [
                    (Some(Tier::T0), Some(Tier::T1), Some(Tier::T1)),
                    (Some(Tier::T1), Some(Tier::T3), Some(Tier::T1)),
                    (Some(Tier::T2), Some(Tier::T3), Some(Tier::T1)),
                    (Some(Tier::T3), Some(Tier::T3), Some(Tier::T1)),
                    (None, None, Some(Tier::T1)),
                ]),
                (ResidentCarry, [
                    (Some(Tier::T0), Some(Tier::T1), Some(Tier::T1)),
                    (Some(Tier::T1), Some(Tier::T2), Some(Tier::T2)),
                    (Some(Tier::T2), Some(Tier::T3), Some(Tier::T3)),
                    (Some(Tier::T3), Some(Tier::T3), Some(Tier::T3)),
                    (None, None, None),
                ]),
                (NotTaken, [
                    (Some(Tier::T0), Some(Tier::T1), Some(Tier::T1)),
                    (Some(Tier::T1), Some(Tier::T2), Some(Tier::T1)),
                    (Some(Tier::T2), Some(Tier::T3), Some(Tier::T1)),
                    (Some(Tier::T3), Some(Tier::T3), Some(Tier::T1)),
                    (None, None, Some(Tier::T1)),
                ]),
            ];
            for (option, tiers) in rows {
                for (current, upgrade, change) in tiers {
                    for (kind, expected) in [(Upgrade, upgrade), (Change, change)] {
                        assert_eq!(landing_tier(option, current, kind, None), expected,
                            "{option:?} {current:?} {kind:?}");
                    }
                }
            }
            assert_eq!(TempleConfig::default().played_consolidation(), NonResidentExtraTier);
        }
    }
}
