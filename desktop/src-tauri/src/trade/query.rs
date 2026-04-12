/// Build the GGG trade search query JSON.
///
/// Mirrors Go's buildSearchQuery in client.go. See that file for detailed
/// field reference (type vs term, securable, collapse, etc.).
///
/// When `dedication` is true, the query targets corrupted 21/23 gems:
/// - Removes `corrupted: false` (21/23 implies corrupted)
/// - Sets `gem_level: min 21`, `quality: min 23`
/// - Uses `gem.activegem` category (skills only, no supports)
pub fn build_search_query(gem: &str, variant: &str) -> serde_json::Value {
    build_search_query_with_mode(gem, variant, false)
}

pub fn build_search_query_with_mode(gem: &str, variant: &str, dedication: bool) -> serde_json::Value {
    let (gem_level, gem_quality) = parse_variant(variant);

    let misc_filters = if dedication {
        // Dedication mode: no corrupted:false filter (21/23c are corrupted by definition),
        // set min level 21 and min quality 23.
        serde_json::json!({
            "gem_level": {"min": 21},
            "quality": {"min": 23}
        })
    } else {
        let mut filters = serde_json::json!({
            "corrupted": {"option": "false"}
        });

        // Level 20+ = exact filter (20/20 is a distinct market from 1/0)
        if gem_level >= 20 {
            filters["gem_level"] = serde_json::json!({"min": gem_level, "max": gem_level});
        }
        // Quality 20 = exact 20%. Quality 0 = no filter (competes in full market).
        if gem_quality == 20 {
            filters["quality"] = serde_json::json!({"min": 20, "max": 20});
        }
        filters
    };

    // Transfigured gems (" of ") use "term" for fuzzy match.
    // Base gems use "type" for exact match (prevents cross-matching).
    let name_field = if gem.contains(" of ") {
        "term"
    } else {
        "type"
    };

    // Dedication mode uses activegem category (skills only, excludes supports).
    let category = if dedication { "gem.activegem" } else { "gem" };

    // Build query object, then insert gem name under the dynamic key.
    // serde_json::json! treats bare identifiers as literal string keys,
    // so we must insert the variable key separately.
    let mut query_inner = serde_json::json!({
        "stats": [{"type": "and", "filters": []}],
        "filters": {
            "type_filters": {
                "filters": {
                    "category": {"option": category}
                }
            },
            "misc_filters": {
                "filters": misc_filters
            },
            "trade_filters": {
                "filters": {
                    "sale_type": {"option": "priced"},
                    "collapse": {"option": "true"}
                }
            }
        },
        "status": {"option": "securable"}
    });
    query_inner[name_field] = serde_json::json!(gem);

    serde_json::json!({
        "query": query_inner,
        "sort": {"price": "asc"}
    })
}

/// Parse variant "20/20" → (level, quality). "20" → (20, 0).
pub fn parse_variant(variant: &str) -> (i32, i32) {
    let parts: Vec<&str> = variant.splitn(2, '/').collect();
    let level = parts[0].parse().unwrap_or(0);
    let quality = if parts.len() == 2 {
        parts[1].parse().unwrap_or(0)
    } else {
        0
    };
    (level, quality)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_variant_full() {
        assert_eq!(parse_variant("20/20"), (20, 20));
    }

    #[test]
    fn parse_variant_level_only() {
        assert_eq!(parse_variant("20"), (20, 0));
    }

    #[test]
    fn parse_variant_level_zero_quality() {
        assert_eq!(parse_variant("1/0"), (1, 0));
    }

    #[test]
    fn search_query_transfigured_uses_term() {
        let q = build_search_query("Earthquake of Fragility", "20/20");
        assert!(q["query"]["term"].is_string());
        assert!(q["query"].get("type").is_none());
    }

    #[test]
    fn search_query_base_gem_uses_type() {
        let q = build_search_query("Empower Support", "1/0");
        assert!(q["query"]["type"].is_string());
        assert!(q["query"].get("term").is_none());
    }

    #[test]
    fn search_query_level_20_has_gem_level_filter() {
        let q = build_search_query("Empower Support", "20/20");
        let filters = &q["query"]["filters"]["misc_filters"]["filters"];
        assert_eq!(filters["gem_level"]["min"], 20);
        assert_eq!(filters["gem_level"]["max"], 20);
    }

    #[test]
    fn search_query_level_1_no_gem_level_filter() {
        let q = build_search_query("Empower Support", "1/0");
        let filters = &q["query"]["filters"]["misc_filters"]["filters"];
        assert!(filters.get("gem_level").is_none());
    }

    #[test]
    fn dedication_query_no_corrupted_false() {
        let q = build_search_query_with_mode("Earthquake of Fragility", "21/23", true);
        let filters = &q["query"]["filters"]["misc_filters"]["filters"];
        assert!(filters.get("corrupted").is_none(), "Dedication mode should not have corrupted:false");
    }

    #[test]
    fn dedication_query_has_level_21_quality_23() {
        let q = build_search_query_with_mode("Earthquake of Fragility", "21/23", true);
        let filters = &q["query"]["filters"]["misc_filters"]["filters"];
        assert_eq!(filters["gem_level"]["min"], 21);
        assert_eq!(filters["quality"]["min"], 23);
    }

    #[test]
    fn dedication_query_uses_activegem_category() {
        let q = build_search_query_with_mode("Earthquake of Fragility", "21/23", true);
        let category = &q["query"]["filters"]["type_filters"]["filters"]["category"]["option"];
        assert_eq!(category, "gem.activegem");
    }

    #[test]
    fn normal_query_uses_gem_category() {
        let q = build_search_query("Earthquake of Fragility", "20/20");
        let category = &q["query"]["filters"]["type_filters"]["filters"]["category"]["option"];
        assert_eq!(category, "gem");
    }

    #[test]
    fn dedication_query_base_gem_uses_type() {
        let q = build_search_query_with_mode("Spark", "21/23", true);
        assert!(q["query"]["type"].is_string());
        assert!(q["query"].get("term").is_none());
    }
}
