//! Built-in boss loot pools. Exchange-backed names and keys come from the
//! generated subset; unique names and category keys belong to the profile.
#![allow(dead_code)] // WI-5 wires the capture owner to this model.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LootItem {
    pub id: String,
    pub name: String,
    pub base: String,
    pub wiki_rate: String,
    #[serde(default)]
    pub quantity: bool,
    #[serde(default)]
    pub not_counted_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootGroup {
    pub id: String,
    pub label: String,
    pub min: usize,
    pub max: usize,
    pub items: Vec<LootItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LootProfile {
    pub boss: String,
    pub name: String,
    pub full_name: String,
    pub key: String,
    #[serde(rename = "profileVersion")]
    pub version: u32,
    pub area: String,
    pub level: u32,
    pub groups: Vec<LootGroup>,
    pub extra: Vec<LootItem>,
    pub categories: Vec<LootItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineMatch {
    Item(String),
    /// Keep all candidates for confirmation; never choose the first one.
    Ambiguous(Vec<String>),
    Unknown,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SelectionError {
    UnknownItem(String),
    InvalidQuantity(String),
    Group { id: String, min: usize, max: usize, selected: usize },
}

pub type Selection = BTreeMap<String, u32>;

fn outcome_of(mut ids: Vec<String>) -> LineMatch {
    match ids.len() {
        0 => LineMatch::Unknown,
        1 => LineMatch::Item(ids.remove(0)),
        _ => LineMatch::Ambiguous(ids),
    }
}

pub fn built_in_profiles() -> &'static [LootProfile] {
    static PROFILES: OnceLock<Vec<LootProfile>> = OnceLock::new();
    PROFILES.get_or_init(|| vec![catarina()])
}

pub fn profile_for_area(area: Option<&crate::client_log::AreaChange<'_>>) -> Option<&'static LootProfile> {
    let area = area?;
    if area.kind != crate::client_log::AreaKind::Combat {
        return None;
    }
    built_in_profiles().iter().find(|profile| profile.area == area.area && profile.level == area.level)
}

/// Outcome of `LootProfile::match_line_scored`, with the detail the capture
/// log names.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredLineMatch {
    pub outcome: LineMatch,
    /// True when the line equals a label after `normalize`.
    pub exact: bool,
    /// Normalized label behind `score`: the matched one, or for an unknown line
    /// the closest one. `None` only for a profile without labels.
    pub label: Option<String>,
    /// 1.0 for an exact match, otherwise the label's Jaro score.
    pub score: f64,
}

/// Jaro score a non-exact line needs against a profile label to resolve to it.
///
/// Plain `strsim::jaro`, not the gem matcher's Jaro-Winkler: strsim 0.11's
/// prefix bonus is unbounded (`mercenary/mod.rs:662`), and loot labels share
/// long prefixes (`ALLFLAME EMBER OF …`), so Jaro-Winkler scores every pair of
/// Catarina Embers, and the fragments `ALLFLAME EMBER` / `ZORATHS EYE`, at 1.000.
///
/// MEASURED 2026-11-19 over the built-in Catarina profile (strsim 0.11, test
/// `loot_tolerance_measurements`):
///
/// | read                            | best label                     | score | runner-up |
/// |---------------------------------|--------------------------------|-------|-----------|
/// | `LORATHS EYE OF THE ENDLESS` AC-1    | Zorath's Eye of the Endless    | 0.974 | 0.630 |
/// | `LLFLAME EMBER OF THE ETHERE` AC-2   | Allflame Ember of the Ethereal | 0.942 | 0.759 |
/// | `ALLFLAME EMBER OF THE ETHEREA` AC-2 | Allflame Ember of the Ethereal | 0.989 | 0.830 |
/// | `ALLFLAME EMBER` fragment       | Allflame Ember of Kulemak      | 0.853 | 0.828 |
/// | `ZORATHS EYE` fragment          | Zorath's Eye of the Endless    | 0.808 | 0.577 |
/// | `TANIUM SPIRI` fragment         | Titanium Spirit Shield         | 0.710 | 0.629 |
/// | `OTHER SERPENTINE STAFF` fragment | Serpentine Staff             | 0.805 | 0.695 |
///
/// Highest sibling-label pair: Allflame Ember of Kulemak / of the Ethereal,
/// 0.831. 0.90 sits above it and above every fragment (highest 0.853).
pub const LOOT_MATCH_THRESHOLD: f64 = 0.90;

/// Lead the best label needs over the runner-up label. The single-wrong-letter
/// sweep (`keep_critical_single_wrong_letter_never_resolves_to_another_item`)
/// passes with it.
pub const LOOT_MATCH_GAP: f64 = 0.05;

impl LootProfile {
    pub fn match_line(&self, line: &str) -> LineMatch {
        self.match_line_scored(line).outcome
    }

    pub fn match_line_scored(&self, line: &str) -> ScoredLineMatch {
        let normalized = normalize(line);
        let candidates: Vec<String> = self.groups.iter().flat_map(|group| &group.items)
            .filter(|item| normalize(&item.base) == normalized || normalize(&item.name) == normalized)
            // Additional drops carry descriptive categories in `base`; only
            // their item names identify a drop. Manual categories never match OCR.
            .chain(self.extra.iter().filter(|item| normalize(&item.name) == normalized))
            .map(|item| item.id.clone()).collect();
        if !candidates.is_empty() {
            return ScoredLineMatch { outcome: outcome_of(candidates), exact: true, label: Some(normalized), score: 1.0 };
        }
        let labels = self.labels();
        let names: Vec<&str> = labels.iter().map(|(label, _)| label.as_str()).collect();
        let Some(ranking) = crate::gem_matcher::rank(&normalized, &names, strsim::jaro) else {
            return ScoredLineMatch { outcome: LineMatch::Unknown, exact: false, label: None, score: 0.0 };
        };
        let (label, ids) = &labels[ranking.best_idx];
        let accepted = ranking.best_score >= LOOT_MATCH_THRESHOLD
            && ranking.best_score - ranking.second_score >= LOOT_MATCH_GAP;
        ScoredLineMatch {
            outcome: if accepted { outcome_of(ids.clone()) } else { LineMatch::Unknown },
            exact: false,
            label: Some(label.clone()),
            score: ranking.best_score,
        }
    }

    /// Distinct normalized labels with the ids that carry them: the label set
    /// of the exact pass in `match_line_scored`.
    fn labels(&self) -> Vec<(String, Vec<String>)> {
        let mut labels: Vec<(String, Vec<String>)> = Vec::new();
        let group_labels = self.groups.iter().flat_map(|group| &group.items)
            .flat_map(|item| [(item, normalize(&item.base)), (item, normalize(&item.name))]);
        let extra_labels = self.extra.iter().map(|item| (item, normalize(&item.name)));
        for (item, label) in group_labels.chain(extra_labels) {
            if label.is_empty() {
                continue;
            }
            match labels.iter_mut().find(|(known, _)| *known == label) {
                Some((_, ids)) if !ids.contains(&item.id) => ids.push(item.id.clone()),
                Some(_) => {}
                None => labels.push((label, vec![item.id.clone()])),
            }
        }
        labels
    }

    pub fn validate_selection(&self, selection: &Selection) -> Result<(), SelectionError> {
        for (id, quantity) in selection {
            let item = self.groups.iter().flat_map(|group| &group.items)
                .chain(&self.extra).chain(&self.categories).find(|item| &item.id == id)
                .ok_or_else(|| SelectionError::UnknownItem(id.clone()))?;
            let category = self.categories.iter().any(|item| &item.id == id);
            if *quantity > 1 && !item.quantity && !category {
                return Err(SelectionError::InvalidQuantity(id.clone()));
            }
        }
        for group in &self.groups {
            let selected = group.items.iter()
                .filter(|item| selection.get(&item.id).copied().unwrap_or(0) > 0).count();
            if selected < group.min || selected > group.max {
                return Err(SelectionError::Group {
                    id: group.id.clone(), min: group.min, max: group.max, selected,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_log::parse_area_change;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../src/lib/boss-loot/__fixtures__/boss-loot.json")).unwrap()
    }

    // Alias conversion exists only at this fixture boundary. Production uses
    // canonical exchange keys, natural unique names and profile category keys.
    fn fixture_profile(boss: &str) -> LootProfile {
        let fixture = fixture();
        let value = fixture["profiles"].get(boss)
            .unwrap_or(&fixture["testProfiles"][boss]);
        let mut profile = LootProfile {
            boss: boss.into(), name: value["name"].as_str().unwrap().into(),
            full_name: value["fullName"].as_str().unwrap_or(value["name"].as_str().unwrap()).into(),
            key: value["key"].as_str().unwrap().into(), version: 2,
            area: value["area"].as_str().unwrap().into(),
            level: value["level"].as_u64().unwrap_or(0) as u32,
            groups: serde_json::from_value(value["groups"].clone()).unwrap(),
            extra: serde_json::from_value(value["extra"].clone()).unwrap(),
            categories: serde_json::from_value(value["categories"].clone()).unwrap(),
        };
        let aliases = [
            ("eye", "Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly"),
            ("veiledex", "Metadata/Items/Currency/CurrencyRerollRareVeiled"),
            ("propagation", "Metadata/Items/MapFragments/PropagationAllflamePack"),
            ("allflame", "Metadata/Items/MapFragments/BetrayalAllflamePack"),
            ("ethereal", "Metadata/Items/MapFragments/CleansedAllflamePack"),
            ("nooks", "Metadata/Items/DivinationCards/DivinationCardNooksCrown"),
            ("keepers", "Metadata/Items/DivinationCards/DivinationCardKeepersCorruption"),
        ];
        for item in profile.groups.iter_mut().flat_map(|g| g.items.iter_mut()).chain(profile.extra.iter_mut()) {
            item.id = if boss == "catarina" {
                aliases.iter().find(|(alias, _)| *alias == item.id)
                    .map(|(_, id)| id.to_string()).unwrap_or_else(|| item.name.clone())
            } else {
                item.name.clone()
            };
        }
        for item in &mut profile.categories {
            item.id = if boss == "catarina" {
                "category:catarina:1:rare-two-veiled-modifiers".into()
            } else {
                format!("category:{boss}:1:{}", item.name)
            };
        }
        profile
    }

    fn selected(names: &[&str]) -> Selection {
        names.iter().map(|name| (name.to_string(), 1)).collect()
    }

    #[test]
    fn production_pool_matches_catarina_fixture() {
        assert_eq!(built_in_profiles(), &[fixture_profile("catarina")]);
    }

    #[test]
    fn boss_area_attributes_catarina() {
        let area = parse_area_change(r#"Generating level 83 area "BetrayalMastermindFight" with seed 2667568808"#).unwrap();
        assert_eq!(profile_for_area(Some(&area)).map(|p| p.boss.as_str()), Some("catarina"));
    }

    #[test]
    fn wrong_level_refuses_attribution() {
        let area = parse_area_change(r#"Generating level 82 area "BetrayalMastermindFight" with seed 1"#).unwrap();
        assert_eq!(profile_for_area(Some(&area)), None);
    }

    #[test]
    fn unmatched_zone_refuses_attribution() {
        let area = parse_area_change(r#"Generating level 83 area "MapWorldsStrand" with seed 1"#).unwrap();
        assert_eq!(profile_for_area(Some(&area)), None);
    }

    #[test]
    fn missing_area_refuses_attribution() {
        assert_eq!(profile_for_area(None), None);
    }

    #[test]
    fn towns_refuse_attribution() {
        let area = parse_area_change(r#"Generating level 83 area "2_10_town" with seed 1"#).unwrap();
        assert_eq!(profile_for_area(Some(&area)), None);
    }

    #[test]
    fn hideouts_refuse_attribution() {
        let area = parse_area_change(r#"Generating level 83 area "HideoutLuxurious" with seed 1"#).unwrap();
        assert_eq!(profile_for_area(Some(&area)), None);
    }

    #[test]
    fn silver_flask_matches_urn_exact_base() {
        assert_eq!(fixture_profile("catarina").match_line("SILVER FLASK"), LineMatch::Item("Cinderswallow Urn".into()));
    }

    #[test]
    fn serpentine_staff_matches_cane_exact_base() {
        assert_eq!(fixture_profile("catarina").match_line("Serpentine Staff"), LineMatch::Item("Cane of Kulemak".into()));
    }

    #[test]
    fn ocr_whitespace_punctuation_quality_are_normalized() {
        assert_eq!(fixture_profile("catarina").match_line(" * superior  SILVER\tFLASK. "), LineMatch::Item("Cinderswallow Urn".into()));
    }

    #[test]
    fn additional_item_matches_full_name() {
        assert_eq!(fixture_profile("catarina").match_line("ZORATH’S EYE OF THE ENDLESS"),
            LineMatch::Item("Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly".into()));
    }

    #[test]
    fn additional_descriptive_base_is_not_an_item() {
        assert_eq!(fixture_profile("catarina").match_line("Currency"), LineMatch::Unknown);
    }

    #[test]
    fn spidersilk_robe_stays_unknown() {
        assert_eq!(fixture_profile("catarina").match_line("Spidersilk Robe"), LineMatch::Unknown);
    }

    #[test]
    fn removed_catarina_entries_stay_unknown() {
        let profile = &built_in_profiles()[0];
        for text in ["Templar Astrolabe", "Rare, two veiled modifiers", "Keeper's Corruption"] {
            assert_eq!(profile.match_line(text), LineMatch::Unknown, "{text}");
        }
    }

    #[test]
    fn old_suffix_category_fragment_matches_stay_unknown() {
        let profile = fixture_profile("catarina");
        for text in ["Flask", "Granite Flask", "Staff", "Other Serpentine Staff", "TANIUM SPIRI", "ALLFLAME EMBER", "ZORATHS EYE", "SUPPORT", "Rare, two veiled modifiers"] {
            assert_eq!(profile.match_line(text), LineMatch::Unknown, "{text}");
        }
    }

    #[test]
    fn maven_shared_base_retains_all_candidates() {
        assert_eq!(fixture_profile("maven").match_line("Cloth Belt"), LineMatch::Ambiguous(vec![
            "Graven's Secret".into(), "Arn's Anguish".into(), "Olesya's Delight".into()
        ]));
    }

    #[test]
    fn maven_named_item_resolves_shared_base() {
        assert_eq!(fixture_profile("maven").match_line("ARN’S ANGUISH"), LineMatch::Item("Arn's Anguish".into()));
    }

    #[test]
    fn fear_requires_each_independent_group() {
        let profile = fixture_profile("fear");
        assert_eq!(profile.validate_selection(&selected(&["Servant of Decay"])), Err(SelectionError::Group {
            id: "astrolabe".into(), min: 1, max: 1, selected: 0
        }));
    }

    #[test]
    fn fear_accepts_each_group_minimum() {
        assert_eq!(fixture_profile("fear").validate_selection(&selected(&["Servant of Decay", "Templar Astrolabe"])), Ok(()));
    }

    #[test]
    fn blackstar_accepts_no_guaranteed_drops() {
        assert_eq!(fixture_profile("blackstar").validate_selection(&Selection::new()), Ok(()));
    }

    #[test]
    fn guaranteed_group_rejects_below_minimum() {
        assert_eq!(fixture_profile("catarina").validate_selection(&Selection::new()), Err(SelectionError::Group {
            id: "unique".into(), min: 1, max: 1, selected: 0
        }));
    }

    #[test]
    fn guaranteed_group_accepts_maximum() {
        assert_eq!(fixture_profile("catarina").validate_selection(&selected(&["Cane of Kulemak", "Metadata/Items/MapFragments/BetrayalAllflamePack"])), Ok(()));
    }

    #[test]
    fn guaranteed_group_rejects_above_maximum() {
        assert_eq!(fixture_profile("catarina").validate_selection(&selected(&["Cane of Kulemak", "Cinderswallow Urn"])), Err(SelectionError::Group {
            id: "unique".into(), min: 1, max: 1, selected: 2
        }));
    }

    #[test]
    fn zero_quantity_does_not_fill_guaranteed_group() {
        let selection = [("Cane of Kulemak".into(), 0)].into_iter().collect();
        assert_eq!(fixture_profile("catarina").validate_selection(&selection), Err(SelectionError::Group {
            id: "unique".into(), min: 1, max: 1, selected: 0
        }));
    }

    #[test]
    fn selection_rejects_unknown_item() {
        assert_eq!(fixture_profile("blackstar").validate_selection(&selected(&["Unknown"])), Err(SelectionError::UnknownItem("Unknown".into())));
    }

    #[test]
    fn selection_rejects_multiple_nonquantity_items() {
        let selection = [("Cane of Kulemak".into(), 2)].into_iter().collect();
        assert_eq!(fixture_profile("catarina").validate_selection(&selection), Err(SelectionError::InvalidQuantity("Cane of Kulemak".into())));
    }

    #[test]
    fn quantity_items_allow_multiple_units() {
        let selection = [("Grand Eldritch Ember".into(), 3)].into_iter().collect();
        assert_eq!(fixture_profile("blackstar").validate_selection(&selection), Ok(()));
    }

    #[test]
    fn manual_categories_allow_multiple_items() {
        let mut profile = fixture_profile("catarina");
        let mut category = item("Manual category", "any equipment", "—");
        category.id = "category:test:1:manual".into();
        profile.categories.push(category);
        let selection = [("Cane of Kulemak".into(), 1),
            ("Metadata/Items/MapFragments/BetrayalAllflamePack".into(), 1),
            ("category:test:1:manual".into(), 4)].into_iter().collect();
        assert_eq!(profile.validate_selection(&selection), Ok(()));
    }

    const PROPAGATION: &str = "Metadata/Items/MapFragments/PropagationAllflamePack";
    const KULEMAK: &str = "Metadata/Items/MapFragments/BetrayalAllflamePack";
    const ETHEREAL: &str = "Metadata/Items/MapFragments/CleansedAllflamePack";

    #[test]
    fn catarina_accepts_unique_with_one_ember() {
        assert_eq!(built_in_profiles()[0].validate_selection(&selected(&["Cane of Kulemak", ETHEREAL])), Ok(()));
    }

    #[test]
    fn catarina_rejects_unique_without_an_ember() {
        assert_eq!(built_in_profiles()[0].validate_selection(&selected(&["Cane of Kulemak"])), Err(SelectionError::Group {
            id: "allflame".into(), min: 1, max: 1, selected: 0
        }));
    }

    #[test]
    fn catarina_rejects_two_embers() {
        assert_eq!(built_in_profiles()[0].validate_selection(&selected(&["Cane of Kulemak", PROPAGATION, KULEMAK])), Err(SelectionError::Group {
            id: "allflame".into(), min: 1, max: 1, selected: 2
        }));
    }

    #[test]
    fn each_ember_full_name_resolves_to_its_item() {
        let profile = &built_in_profiles()[0];
        for (text, id) in [("ALLFLAME EMBER OF PROPAGATION", PROPAGATION), ("Allflame Ember of Kulemak", KULEMAK), ("allflame ember of the ethereal", ETHEREAL)] {
            assert_eq!(profile.match_line(text), LineMatch::Item(id.into()), "{text}");
        }
    }

    #[test]
    fn catalogue_holds_the_three_embers_without_the_astrolabe() {
        let catalogue: Value = serde_json::from_str(include_str!("loot_item_catalogue.json")).unwrap();
        let ids: Vec<&str> = catalogue["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
        assert!(!ids.contains(&"Metadata/Items/Currency/AstrolabeGeneric"), "{ids:?}");
        for id in [PROPAGATION, KULEMAK, ETHEREAL] {
            assert!(ids.contains(&id), "{id} missing from {ids:?}");
        }
    }

    #[test]
    fn catarina_profile_is_version_two() {
        assert_eq!(built_in_profiles()[0].version, 2);
    }

    #[test]
    fn wider_group_accepts_inclusive_minimum() {
        let mut profile = fixture_profile("maven");
        profile.groups[0].min = 2;
        profile.groups[0].max = 3;
        assert_eq!(profile.validate_selection(&selected(&["Legacy of Fury", "Graven's Secret"])), Ok(()));
    }

    #[test]
    fn wider_group_accepts_inclusive_maximum() {
        let mut profile = fixture_profile("maven");
        profile.groups[0].min = 2;
        profile.groups[0].max = 3;
        assert_eq!(profile.validate_selection(&selected(&["Legacy of Fury", "Graven's Secret", "Arn's Anguish"])), Ok(()));
    }

    #[test]
    fn wider_group_rejects_below_minimum() {
        let mut profile = fixture_profile("maven");
        profile.groups[0].min = 2;
        profile.groups[0].max = 3;
        assert_eq!(profile.validate_selection(&selected(&["Legacy of Fury"])), Err(SelectionError::Group {
            id: "unique".into(), min: 2, max: 3, selected: 1
        }));
    }

    #[test]
    fn wider_group_rejects_above_maximum() {
        let mut profile = fixture_profile("maven");
        profile.groups[0].min = 2;
        profile.groups[0].max = 3;
        assert_eq!(profile.validate_selection(&selected(&["Legacy of Fury", "Graven's Secret", "Arn's Anguish", "Olesya's Delight"])), Err(SelectionError::Group {
            id: "unique".into(), min: 2, max: 3, selected: 4
        }));
    }

    const ZORATH: &str = "Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly";

    #[test]
    fn keep_regression_lorath_resolves_to_zoraths_eye() {
        let scored = built_in_profiles()[0].match_line_scored("LORATH'S EYE OF THE ENDLESS");
        assert_eq!(scored.outcome, LineMatch::Item(ZORATH.into()));
        assert!(!scored.exact);
    }

    #[test]
    fn keep_regression_clipped_ember_reads_resolve_to_ethereal() {
        let profile = &built_in_profiles()[0];
        for text in ["LLFLAME EMBER OF THE ETHERE", "ALLFLAME EMBER OF THE ETHEREA"] {
            assert_eq!(profile.match_line(text), LineMatch::Item(ETHEREAL.into()), "{text}");
        }
    }

    #[test]
    fn unknown_line_reports_its_closest_label() {
        let scored = built_in_profiles()[0].match_line_scored("ZORATHS EYE");
        assert_eq!(scored.outcome, LineMatch::Unknown);
        assert_eq!(scored.label.as_deref(), Some("ZORATHS EYE OF THE ENDLESS"));
        assert!(scored.score < LOOT_MATCH_THRESHOLD, "{}", scored.score);
    }

    #[test]
    fn exact_line_is_scored_as_exact() {
        let scored = built_in_profiles()[0].match_line_scored("Silver Flask");
        assert!(scored.exact);
        assert_eq!(scored.score, 1.0);
    }

    // A wrong resolution is silent: a plausible wrong item is recorded in a run.
    #[test]
    fn keep_critical_single_wrong_letter_never_resolves_to_another_item() {
        const CONFUSIONS: [char; 13] = ['I', 'L', '1', 'O', '0', 'Z', 'S', '5', 'B', '8', 'E', 'F', 'Q'];
        let mut resolved = 0;
        for profile in built_in_profiles() {
            for (label, ids) in profile.labels() {
                let chars: Vec<char> = label.chars().collect();
                for i in 0..chars.len() {
                    for c in CONFUSIONS.into_iter().filter(|c| *c != chars[i]) {
                        let mut read = chars.clone();
                        read[i] = c;
                        let read: String = read.into_iter().collect();
                        match profile.match_line(&read) {
                            LineMatch::Unknown => {}
                            LineMatch::Item(id) => {
                                assert!(ids.contains(&id), "{read} resolved to {id}, label {label}");
                                resolved += 1;
                            }
                            LineMatch::Ambiguous(got) => assert!(
                                got.iter().any(|id| ids.contains(id)), "{read} ambiguous {got:?}, label {label}"),
                        }
                    }
                }
            }
        }
        assert!(resolved > 0, "the sweep resolved nothing; tolerance is off");
    }

    #[test]
    #[ignore = "measurement table for LOOT_MATCH_THRESHOLD; run with --ignored --nocapture"]
    fn loot_tolerance_measurements() {
        let profile = &built_in_profiles()[0];
        let labels = profile.labels();
        let mut sibling = (0.0, String::new(), String::new());
        for (a, _) in &labels {
            for (b, _) in &labels {
                let score = strsim::jaro(a, b);
                if a != b && score > sibling.0 { sibling = (score, a.clone(), b.clone()); }
            }
        }
        println!("highest sibling pair {:.3} {} / {}", sibling.0, sibling.1, sibling.2);
        let names: Vec<&str> = labels.iter().map(|(l, _)| l.as_str()).collect();
        for read in ["LORATHS EYE OF THE ENDLESS", "LLFLAME EMBER OF THE ETHERE", "ALLFLAME EMBER OF THE ETHEREA",
            "ALLFLAME EMBER", "ZORATHS EYE", "TANIUM SPIRI", "OTHER SERPENTINE STAFF", "GRANITE FLASK", "SPIDERSILK ROBE"] {
            let r = crate::gem_matcher::rank(read, &names, strsim::jaro).unwrap();
            println!("{read}: {} {:.3} second {:.3}", names[r.best_idx], r.best_score, r.second_score);
        }
    }
}


fn item(name: &str, base: &str, wiki_rate: &str) -> LootItem {
    LootItem {
        id: name.into(), name: name.into(), base: base.into(), wiki_rate: wiki_rate.into(),
        quantity: false, not_counted_default: false,
    }
}

fn catarina() -> LootProfile {
    #[derive(Deserialize)]
    struct CatalogueItem { id: String, name: String }
    #[derive(Deserialize)]
    struct Catalogue { items: Vec<CatalogueItem> }
    let catalogue: Catalogue = serde_json::from_str(include_str!("loot_item_catalogue.json"))
        .expect("generated boss loot catalogue must be valid");
    let catalogue_item = |id: &str, base: &str, rate: &str| {
        let entry = catalogue.items.iter().find(|entry| entry.id == id)
            .expect("built-in profile item must exist in generated catalogue");
        LootItem { id: entry.id.clone(), ..item(&entry.name, base, rate) }
    };
    let mut communion = item("Communion Support", "Support gem", "10%");
    communion.not_counted_default = true;
    LootProfile {
        boss: "catarina".into(), name: "Catarina".into(),
        full_name: "Catarina, Master of Undeath".into(), key: "Normal".into(), version: 2,
        area: "BetrayalMastermindFight".into(), level: 83,
        groups: vec![LootGroup {
            id: "unique".into(), label: "Unique".into(), min: 1, max: 1,
            items: vec![
                item("Cinderswallow Urn", "Silver Flask", "30%"),
                item("Spinehail", "Vile Arrow Quiver", "20%"),
                item("Cane of Kulemak", "Serpentine Staff", "18%"),
                item("The Devouring Diadem", "Necromancer Circlet", "16%"),
                item("Bitterbind Point", "Titanium Spirit Shield", "10%"),
                item("The Queen's Hunger", "Vaal Regalia", "6%"),
            ],
        }, LootGroup {
            id: "allflame".into(), label: "Allflame Ember".into(), min: 1, max: 1,
            items: [
                "Metadata/Items/MapFragments/PropagationAllflamePack",
                "Metadata/Items/MapFragments/BetrayalAllflamePack",
                "Metadata/Items/MapFragments/CleansedAllflamePack",
            ].into_iter().map(|id| {
                // Full names as bases: a shared "Allflame Ember" base would make OCR Ambiguous.
                let mut ember = catalogue_item(id, "", "—");
                ember.base = ember.name.clone();
                ember
            }).collect(),
        }],
        extra: vec![
            catalogue_item("Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly", "Map fragment", "~50%"),
            catalogue_item("Metadata/Items/Currency/CurrencyRerollRareVeiled", "Currency", "25%"),
            communion,
            catalogue_item("Metadata/Items/DivinationCards/DivinationCardNooksCrown", "Divination card", "<0.5%"),
        ],
        categories: vec![],
    }
}

/// Normalizes an OCR line or a label: case, whitespace, apostrophes, edge
/// punctuation and Superior. Labels compared after this are equal for an exact
/// match and Jaro-scored otherwise (`match_line_scored`).
fn normalize(line: &str) -> String {
    let upper = line.chars().filter(|c| !matches!(c, '\'' | '’' | '`' | '‘'))
        .collect::<String>().to_uppercase();
    let collapsed = upper.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|c: char| !c.is_alphanumeric());
    trimmed.strip_prefix("SUPERIOR ").unwrap_or(trimmed).to_string()
}
