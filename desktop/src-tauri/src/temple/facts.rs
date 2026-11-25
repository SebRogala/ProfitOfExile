//! Temple game rules used by offer resolution and the production rollout.
//!
//! Sources, dates and validation status live in `docs/GAME-FACTS.md` (POE-415).

use super::rooms::{CRADLE_LINE_KEY, OfferKind};
use super::strategy::{Consolidation, TempleConfig, Tier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidationGlosses {
    pub non_resident_extra_tier: String,
    pub resident_carry: String,
    pub not_taken: String,
    pub cradle: String,
    pub unset_plays: Consolidation,
}

pub fn consolidation_glosses() -> ConsolidationGlosses {
    let option_gloss = |option| {
        let tier = |kind| {
            landing_tier(option, Some(Tier::T1), kind, None)
                .expect("a known current tier resolves every option")
                .get()
        };
        format!(
            "T1 room: upgrade → T{}, change → T{}",
            tier(OfferKind::Upgrade),
            tier(OfferKind::Change),
        )
    };
    let unset_plays = TempleConfig::default().played_consolidation();
    ConsolidationGlosses {
        non_resident_extra_tier: option_gloss(Consolidation::NonResidentExtraTier),
        resident_carry: option_gloss(Consolidation::ResidentCarry),
        not_taken: option_gloss(Consolidation::NotTaken),
        cradle: format!(
            "always built at tier {}",
            landing_tier(unset_plays, Some(Tier::T1), OfferKind::Change, Some(CRADLE_LINE_KEY))
                .expect("the Cradle line always resolves")
                .get(),
        ),
        unset_plays,
    }
}

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
        fn consolidation_glosses_render_the_ruled_landing_tiers() {
            assert_eq!(consolidation_glosses(), ConsolidationGlosses {
                non_resident_extra_tier: "T1 room: upgrade → T3, change → T1".into(),
                resident_carry: "T1 room: upgrade → T2, change → T2".into(),
                not_taken: "T1 room: upgrade → T2, change → T1".into(),
                cradle: "always built at tier 3".into(),
                unset_plays: Consolidation::NonResidentExtraTier,
            });
        }

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
