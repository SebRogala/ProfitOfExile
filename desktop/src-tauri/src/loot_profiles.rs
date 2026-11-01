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

#[derive(Debug, PartialEq, Eq)]
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

pub fn built_in_profiles() -> &'static [LootProfile] {
    static PROFILES: OnceLock<Vec<LootProfile>> = OnceLock::new();
    PROFILES.get_or_init(|| vec![catarina()])
}

pub fn profile_for_area(area: Option<&crate::lab_state::AreaChange<'_>>) -> Option<&'static LootProfile> {
    let area = area?;
    if area.kind != crate::lab_state::AreaKind::Combat {
        return None;
    }
    built_in_profiles().iter().find(|profile| profile.area == area.area && profile.level == area.level)
}

impl LootProfile {
    pub fn match_line(&self, line: &str) -> LineMatch {
        let normalized = normalize(line);
        let candidates: Vec<String> = self.groups.iter().flat_map(|group| &group.items)
            .filter(|item| normalize(&item.base) == normalized || normalize(&item.name) == normalized)
            // Additional drops carry descriptive categories in `base`; only
            // their item names identify a drop. Manual categories never match OCR.
            .chain(self.extra.iter().filter(|item| normalize(&item.name) == normalized))
            .map(|item| item.id.clone()).collect();
        match candidates.len() {
            0 => LineMatch::Unknown,
            1 => LineMatch::Item(candidates.into_iter().next().unwrap()),
            _ => LineMatch::Ambiguous(candidates),
        }
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
    use crate::lab_state::parse_area_change;
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
            key: value["key"].as_str().unwrap().into(), version: 1,
            area: value["area"].as_str().unwrap().into(),
            level: value["level"].as_u64().unwrap_or(0) as u32,
            groups: serde_json::from_value(value["groups"].clone()).unwrap(),
            extra: serde_json::from_value(value["extra"].clone()).unwrap(),
            categories: serde_json::from_value(value["categories"].clone()).unwrap(),
        };
        let aliases = [
            ("eye", "Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly"),
            ("veiledex", "Metadata/Items/Currency/CurrencyRerollRareVeiled"),
            ("allflame", "Metadata/Items/MapFragments/BetrayalAllflamePack"),
            ("astrolabe", "Metadata/Items/Currency/AstrolabeGeneric"),
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
        for text in ["Templar Astrolabe", "Rare, two veiled modifiers"] {
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
        assert_eq!(fixture_profile("catarina").validate_selection(&selected(&["Cane of Kulemak"])), Ok(()));
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
            ("category:test:1:manual".into(), 4)].into_iter().collect();
        assert_eq!(profile.validate_selection(&selection), Ok(()));
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
        full_name: "Catarina, Master of Undeath".into(), key: "Normal".into(), version: 1,
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
        }],
        extra: vec![
            catalogue_item("Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly", "Map fragment", "~50%"),
            catalogue_item("Metadata/Items/Currency/CurrencyRerollRareVeiled", "Currency", "25%"),
            catalogue_item("Metadata/Items/MapFragments/BetrayalAllflamePack", "Allflame Ember", "60%"),
            communion,
            catalogue_item("Metadata/Items/DivinationCards/DivinationCardNooksCrown", "Divination card", "<0.5%"),
            catalogue_item("Metadata/Items/DivinationCards/DivinationCardKeepersCorruption", "Divination card", "—"),
        ],
        categories: vec![],
    }
}

/// Mirrors loot_capture::normalize: case, whitespace, apostrophes,
/// edge punctuation and Superior. Full normalized labels must still be equal.
fn normalize(line: &str) -> String {
    let upper = line.chars().filter(|c| !matches!(c, '\'' | '’' | '`' | '‘'))
        .collect::<String>().to_uppercase();
    let collapsed = upper.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|c: char| !c.is_alphanumeric());
    trimmed.strip_prefix("SUPERIOR ").unwrap_or(trimmed).to_string()
}
