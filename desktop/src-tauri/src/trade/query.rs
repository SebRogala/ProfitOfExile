#[cfg(test)]
fn build_search_query(gem: &str, variant: &str) -> serde_json::Value {
    build_search_query_with_mode(gem, variant, false)
}

/// Build the GGG trade search query JSON.
///
/// Mirrors Go's buildSearchQuery in client.go. See that file for detailed
/// field reference (type vs term, securable, collapse, etc.).
///
/// When `dedication` is true, the query targets the corrupted gems of the
/// requested variant (21/23 or 21/20):
/// - Removes `corrupted: false` (both variants imply corrupted)
/// - Sets `gem_level` from the variant's level and pins `quality` to it exactly.
///   Quality is exact rather than a minimum because 21/20 and 21/23 are separate
///   markets: a minimum would price the cheaper pool off the dearer one's listings.
/// - Uses `gem.activegem` category (skills only, no supports)
pub fn build_search_query_with_mode(gem: &str, variant: &str, dedication: bool) -> serde_json::Value {
    let (gem_level, gem_quality) = parse_variant(variant);

    let misc_filters = if dedication {
        // Dedication mode: no corrupted:false filter (these variants are corrupted
        // by definition), level from the variant and its exact quality.
        serde_json::json!({
            "gem_level": {"min": gem_level},
            "quality": {"min": gem_quality, "max": gem_quality}
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
///
/// The corrupted suffix is tolerated: the API and the analysis rows speak the DB
/// form ("21/20c"), the UI speaks the display form ("21/20"), and both reach this
/// function. Without stripping it, `"20c".parse()` fails to 0 and the caller
/// builds a `quality {min:0, max:0}` search — a silently empty market rather than
/// a parse error.
pub fn parse_variant(variant: &str) -> (i32, i32) {
    fn number(part: &str) -> i32 {
        part.trim_end_matches('c').parse().unwrap_or(0)
    }
    let parts: Vec<&str> = variant.splitn(2, '/').collect();
    let level = number(parts[0]);
    let quality = if parts.len() == 2 { number(parts[1]) } else { 0 };
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

    // 21/20 and 21/23 are separate markets. A quality minimum would let the
    // dearer 23-quality listings set the price of the 20-quality pool.
    #[test]
    fn dedication_query_pins_quality_to_the_requested_variant() {
        let q = build_search_query_with_mode("Earthquake of Fragility", "21/20", true);
        let filters = &q["query"]["filters"]["misc_filters"]["filters"];
        assert_eq!(filters["gem_level"]["min"], 21);
        assert_eq!(filters["quality"]["min"], 20);
        assert_eq!(filters["quality"]["max"], 20, "21/23 listings must not answer a 21/20 lookup");
    }

    // The DB form reaches this function from the API and from analysis rows; a
    // dropped suffix used to parse as quality 0, i.e. an empty market.
    #[test]
    fn parse_variant_tolerates_the_corrupted_suffix() {
        assert_eq!(parse_variant("21/20c"), (21, 20));
        assert_eq!(parse_variant("21/23c"), (21, 23));
        assert_eq!(parse_variant("21c"), (21, 0));
    }

    #[test]
    fn dedication_query_from_db_form_variant_matches_display_form() {
        let db = build_search_query_with_mode("Earthquake of Fragility", "21/20c", true);
        let display = build_search_query_with_mode("Earthquake of Fragility", "21/20", true);
        assert_eq!(db, display, "the two spellings of one market must query the same market");
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
