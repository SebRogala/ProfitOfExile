//! Font panel OCR parser — extracts craft options from OCR text lines.
//!
//! Scans for keyword anchors in each line. Does NOT try to reconstruct
//! multi-line sentences — just detects what options are present and
//! extracts numeric values (quality %, experience amount, lens %).

use serde::Serialize;

/// A detected craft option from the font panel.
#[derive(Debug, Clone, Serialize)]
pub struct CraftOption {
    /// Machine-readable type for grouping in statistics.
    #[serde(rename = "type")]
    pub option_type: String,
    /// The raw OCR text that triggered detection.
    pub text: String,
    /// Numeric value if applicable (quality %, experience in millions, lens %).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<i32>,
}

/// Result of parsing font panel OCR lines.
#[derive(Debug, Clone, Serialize)]
pub struct FontPanelState {
    /// Detected craft options this round.
    pub options: Vec<CraftOption>,
    /// Crafts remaining (None if not detected).
    pub crafts_remaining: Option<i32>,
    /// Whether the "Transform a Skill Gem" anchor was found (font is active).
    pub font_active: bool,
    /// Whether the jackpot option was detected.
    pub jackpot_detected: bool,
}

/// Parse OCR lines from the font panel region into structured craft options.
pub fn parse_font_panel(lines: &[String]) -> FontPanelState {
    let mut options = Vec::new();
    let mut crafts_remaining = None;
    let mut font_active = false;
    let mut jackpot_detected = false;

    // Join all lines into one blob for multi-line keyword detection,
    // but also scan line-by-line for numeric extraction.
    let full_text = lines.join(" ");
    let full_lower = full_text.to_lowercase();

    // Standard transform (always first, always present when font is open)
    if full_lower.contains("random transfigured gem") {
        font_active = true;
        options.push(CraftOption {
            option_type: "transform_random".to_string(),
            text: find_line_containing(lines, "random Transfigured").unwrap_or_default(),
            value: None,
        });
    }

    // JACKPOT: direct transfigure
    if full_lower.contains("non-transfigured") {
        jackpot_detected = true;
        options.push(CraftOption {
            option_type: "transform_direct".to_string(),
            text: find_line_containing(lines, "non-Transfigured").unwrap_or_default(),
            value: None,
        });
    }

    // Exchange for Empower/Enlighten/Enhance
    if full_lower.contains("empower support") {
        options.push(CraftOption {
            option_type: "exchange_exceptional".to_string(),
            text: find_line_containing(lines, "Empower Support").unwrap_or_default(),
            value: None,
        });
    }

    // Add quality — anchor on "% quality" rather than "quality to a gem". The
    // "to"→"10" OCR misread (e.g. "Add +20% quality 10 a Gem") breaks the "to a gem"
    // anchor, but "% quality" survives it.
    if full_lower.contains("% quality") {
        let value = extract_percentage_near(lines, "quality");
        options.push(CraftOption {
            option_type: "quality".to_string(),
            text: find_line_containing(lines, "% quality").unwrap_or_default(),
            value,
        });
    }

    // Add experience — detect on the word "experience" alone, decoupled from the
    // amount (mirrors quality, which detects on "% quality" and tolerates a mangled
    // value). Requiring an "Nm" amount here would silently drop the whole option
    // when OCR garbles the number; instead the amount is extracted separately by
    // extract_experience_amount, which returns None gracefully. The word-exclusions
    // are load-bearing — they keep other "experience"-bearing lines out of this
    // bucket: the Facetor's Lens line ("…total experience stored as a Facetor's
    // Lens", excluded via stored/facetor/lens) and the player-XP sacrifice line
    // ("gain X% of your own experience", excluded via "your own" — handled by
    // sacrifice_experience below).
    if let Some(line) = lines.iter().find(|line| {
        let lower = line.to_lowercase();
        lower.contains("experience")
            && !lower.contains("stored")
            && !lower.contains("facetor")
            && !lower.contains("lens")
            && !lower.contains("your own")
    }) {
        let value = extract_experience_amount(lines);
        options.push(CraftOption {
            option_type: "experience".to_string(),
            text: line.clone(),
            value,
        });
    }

    // Sacrifice for Facetor's Lens
    if full_lower.contains("facetor") || full_lower.contains("faction") {
        // OCR might read "Facetor's" as "Faction's" — handle both
        let value = extract_percentage_near(lines, "facetor")
            .or_else(|| extract_percentage_near(lines, "faction"));
        options.push(CraftOption {
            option_type: "facetors_lens".to_string(),
            text: find_line_containing(lines, "Facetor")
                .or_else(|| find_line_containing(lines, "Faction"))
                .unwrap_or_default(),
            value,
        });
    }

    // Sacrifice for Treasure Keys
    if full_lower.contains("treasure keys") {
        options.push(CraftOption {
            option_type: "sacrifice_keys".to_string(),
            text: find_line_containing(lines, "Treasure Keys").unwrap_or_default(),
            value: None,
        });
    }

    // Sacrifice for Currency Items
    if full_lower.contains("currency items") {
        options.push(CraftOption {
            option_type: "sacrifice_currency".to_string(),
            text: find_line_containing(lines, "Currency Items").unwrap_or_default(),
            value: None,
        });
    }

    // Sacrifice for player experience
    if full_lower.contains("your own experience") {
        let value = extract_percentage_near(lines, "your own experience");
        options.push(CraftOption {
            option_type: "sacrifice_experience".to_string(),
            text: find_line_containing(lines, "your own experience").unwrap_or_default(),
            value,
        });
    }

    // Dedication: corrupted transfigured reroll
    if full_lower.contains("corrupted transfigured") {
        font_active = true;
        options.push(CraftOption {
            option_type: "corrupted_transfigured_reroll".to_string(),
            text: find_line_containing(lines, "Corrupted Transfigured").unwrap_or_default(),
            value: None,
        });
    }

    // Dedication: corrupted skill gem reroll (non-transfigured pool).
    // Check line-by-line: match lines containing "corrupted skill gem" but NOT "transfigured",
    // so it works even when the transfigured option is present in the same panel.
    {
        let has_non_transfig_reroll = lines.iter().any(|line| {
            let lower = line.to_lowercase();
            lower.contains("corrupted skill gem") && !lower.contains("transfigured")
        });
        if has_non_transfig_reroll {
            font_active = true;
            options.push(CraftOption {
                option_type: "corrupted_gem_reroll".to_string(),
                text: lines.iter()
                    .find(|l| {
                        let lower = l.to_lowercase();
                        lower.contains("corrupted skill gem") && !lower.contains("transfigured")
                    })
                    .cloned()
                    .unwrap_or_default(),
                value: None,
            });
        }
    }

    // Crafts Remaining: N
    if full_lower.contains("crafts remaining") {
        crafts_remaining = extract_number_after(lines, "Crafts Remaining");
    }

    FontPanelState {
        options,
        crafts_remaining,
        font_active,
        jackpot_detected,
    }
}

/// Find the first line containing a case-sensitive substring.
fn find_line_containing(lines: &[String], needle: &str) -> Option<String> {
    let needle_lower = needle.to_lowercase();
    lines
        .iter()
        .find(|l| l.to_lowercase().contains(&needle_lower))
        .cloned()
}

/// Extract a percentage (e.g., "+20%" or "30%") from lines near a keyword.
fn extract_percentage_near(lines: &[String], keyword: &str) -> Option<i32> {
    let keyword_lower = keyword.to_lowercase();
    for line in lines {
        if line.to_lowercase().contains(&keyword_lower) {
            if let Some(pct) = extract_percentage_from_text(line) {
                return Some(pct);
            }
        }
    }
    // Check adjacent lines (value might be on the line before the keyword)
    for (i, line) in lines.iter().enumerate() {
        if line.to_lowercase().contains(&keyword_lower) && i > 0 {
            if let Some(pct) = extract_percentage_from_text(&lines[i - 1]) {
                return Some(pct);
            }
        }
    }
    None
}

/// Extract a percentage number from text like "+20%" or "30%" or "60%".
fn extract_percentage_from_text(text: &str) -> Option<i32> {
    let re_like: Vec<&str> = text.split('%').collect();
    if re_like.len() < 2 {
        return None;
    }
    // Get the number just before the %
    let before_pct = re_like[0].trim();
    // Find the last number in the string before %
    let num_str: String = before_pct
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '+')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    num_str.trim_start_matches('+').parse().ok()
}

/// Extract experience amount in millions (e.g., "150m" or "30m") from lines.
fn extract_experience_amount(lines: &[String]) -> Option<i32> {
    for line in lines {
        let lower = line.to_lowercase();
        if lower.contains("experience") || lower.contains("exp") {
            // Look for patterns like "150m" or "30m" or just numbers
            if let Some(val) = extract_millions_from_text(line) {
                return Some(val);
            }
        }
    }
    // Also look for "Add X experience" pattern — number before "experience"
    let joined = lines.join(" ");
    if let Some(idx) = joined.to_lowercase().find("experience to a gem") {
        let before = &joined[..idx];
        let words: Vec<&str> = before.split_whitespace().collect();
        if let Some(last) = words.last() {
            let cleaned = last.trim_end_matches('m').trim_end_matches('M');
            if let Ok(val) = cleaned.parse::<i32>() {
                return Some(val);
            }
        }
    }
    None
}

/// Extract a number followed by 'm' (millions) from text.
fn extract_millions_from_text(text: &str) -> Option<i32> {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let mut num = String::new();
            num.push(c);
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() {
                    num.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            // Check if followed by 'm'
            if let Some(&next) = chars.peek() {
                if next == 'm' || next == 'M' {
                    if let Ok(val) = num.parse::<i32>() {
                        return Some(val);
                    }
                }
            }
        }
    }
    None
}

/// Extract a number after a keyword (e.g., "Crafts Remaining: 7" → 7).
fn extract_number_after(lines: &[String], keyword: &str) -> Option<i32> {
    let keyword_lower = keyword.to_lowercase();
    for line in lines {
        if let Some(idx) = line.to_lowercase().find(&keyword_lower) {
            let after = &line[idx + keyword.len()..];
            let num_str: String = after
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            if let Ok(val) = num_str.parse() {
                return Some(val);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_standard_transform() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.font_active);
        assert!(!state.jackpot_detected);
        assert_eq!(state.options.len(), 1);
        assert_eq!(state.options[0].option_type, "transform_random");
    }

    #[test]
    fn detects_jackpot() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Transform a non-Transfigured Skill Gem".to_string(),
            "to a Transfigured version".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.font_active);
        assert!(state.jackpot_detected);
        assert_eq!(state.options.len(), 2);
        assert_eq!(state.options[1].option_type, "transform_direct");
    }

    #[test]
    fn detects_quality_with_value() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Add +20% quality to a Gem".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(state.options.len(), 2);
        assert_eq!(state.options[1].option_type, "quality");
        assert_eq!(state.options[1].value, Some(20));
    }

    #[test]
    fn detects_experience_with_value() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Add 150m experience to a Gem".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(state.options[1].option_type, "experience");
        assert_eq!(state.options[1].value, Some(150));
    }

    #[test]
    fn detects_facetors_lens_with_percentage() {
        let lines = vec![
            "Sacrifice a Gem to gain 60% of the gem's".to_string(),
            "total experience stored as a Facetor's Lens".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(state.options.len(), 1);
        assert_eq!(state.options[0].option_type, "facetors_lens");
        assert_eq!(state.options[0].value, Some(60));
    }

    #[test]
    fn detects_crafts_remaining() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Sacrifice a Gem for Currency Items".to_string(),
            "Crafts Remaining: 7".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(state.crafts_remaining, Some(7));
    }

    #[test]
    fn no_crafts_remaining_means_last_craft() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Add +8% quality to a Gem".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.font_active);
        assert_eq!(state.crafts_remaining, None); // last craft
    }

    #[test]
    fn empty_panel_not_active() {
        let lines = vec!["CRAFT".to_string()];
        let state = parse_font_panel(&lines);
        assert!(!state.font_active);
        assert!(state.options.is_empty());
    }

    #[test]
    fn handles_ocr_misread_factions_lens() {
        // OCR sometimes reads "Facetor's" as "Faction's"
        let lines = vec![
            "Sacrifice a Gem to gain 30% of the gem's".to_string(),
            "total experience stored as a Faction's Lens".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(state.options[0].option_type, "facetors_lens");
        assert_eq!(state.options[0].value, Some(30));
    }

    #[test]
    fn detects_multiple_options() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Add 30m experience to a Gem".to_string(),
            "Sacrifice a Gem for Treasure Keys".to_string(),
            "Sacrifice a Gem for Currency Items".to_string(),
            "Crafts Remaining: 6".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.font_active);
        assert_eq!(state.options.len(), 4);
        assert_eq!(state.crafts_remaining, Some(6));
    }

    #[test]
    fn detects_corrupted_gem_reroll() {
        let lines = vec![
            "Transform a Corrupted Skill Gem into a".to_string(),
            "random Corrupted Skill Gem of the same colour".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.font_active);
        assert_eq!(state.options.len(), 1);
        assert_eq!(state.options[0].option_type, "corrupted_gem_reroll");
    }

    #[test]
    fn detects_corrupted_transfigured_reroll() {
        let lines = vec![
            "Transform a Corrupted Transfigured Skill Gem".to_string(),
            "into a random Corrupted Transfigured Skill Gem".to_string(),
            "of the same colour".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.font_active);
        assert_eq!(state.options.len(), 1);
        assert_eq!(state.options[0].option_type, "corrupted_transfigured_reroll");
    }

    #[test]
    fn detects_both_dedication_options() {
        // Both options can appear in the same Dedication font panel.
        let lines = vec![
            "Transform a Corrupted Skill Gem into a random".to_string(),
            "Corrupted Skill Gem of the same colour".to_string(),
            "Transform a Corrupted Transfigured Skill Gem".to_string(),
            "into a random Corrupted Transfigured Skill Gem".to_string(),
            "of the same colour".to_string(),
            "Crafts Remaining: 3".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.font_active);
        assert_eq!(state.crafts_remaining, Some(3));
        // Both options detected — line-level check prevents the "transfigured" guard
        // from suppressing the non-transfigured option when both are present.
        let types: Vec<&str> = state.options.iter().map(|o| o.option_type.as_str()).collect();
        assert!(types.contains(&"corrupted_transfigured_reroll"));
        assert!(types.contains(&"corrupted_gem_reroll"));
    }

    #[test]
    fn detects_quality_with_to_misread() {
        // "to"→"10" OCR misread: "quality to a Gem" becomes "quality 10 a Gem".
        // The "% quality" anchor survives it.
        let lines = vec!["Add +20% quality 10 a Gem".to_string()];
        let state = parse_font_panel(&lines);
        let quality = state.options.iter().find(|o| o.option_type == "quality");
        assert!(quality.is_some(), "quality option should be detected despite the misread");
        assert_eq!(quality.unwrap().value, Some(20));
    }

    #[test]
    fn detects_experience_with_to_misread() {
        // "to"→"10" OCR misread: "experience to a Gem" becomes "experience 10 a Gem".
        // The "Nm experience" amount pattern survives it.
        let lines = vec!["Add 30m experience 10 a Gem".to_string()];
        let state = parse_font_panel(&lines);
        let exp = state.options.iter().find(|o| o.option_type == "experience");
        assert!(exp.is_some(), "experience option should be detected despite the misread");
        assert_eq!(exp.unwrap().value, Some(30));
    }

    #[test]
    fn facetors_lens_line_does_not_yield_experience() {
        // Regression guard: the Facetor's Lens line contains the word "experience"
        // but must NOT be detected as an Add-experience option (it has no Nm amount
        // and mentions stored/facetor/lens).
        let lines = vec![
            "Sacrifice a Gem to gain 60% of the gem's".to_string(),
            "total experience stored as a Facetor's Lens".to_string(),
        ];
        let state = parse_font_panel(&lines);
        let types: Vec<&str> = state.options.iter().map(|o| o.option_type.as_str()).collect();
        assert!(types.contains(&"facetors_lens"));
        assert!(
            !types.contains(&"experience"),
            "Facetor's Lens line must not be mistaken for an Add-experience option"
        );
    }

    #[test]
    fn facetor_line_with_millions_token_is_not_experience() {
        // Detection is decoupled from the Nm amount, so the stored/facetor/lens
        // guard is now the ONLY thing keeping the Facetor's Lens line out of the
        // experience bucket — even when it carries an Nm-shaped token ("60m").
        let lines = vec![
            "Sacrifice a Gem to gain 60m of the gem's total experience stored as a Facetor's Lens".to_string(),
        ];
        let state = parse_font_panel(&lines);
        let types: Vec<&str> = state.options.iter().map(|o| o.option_type.as_str()).collect();
        assert!(types.contains(&"facetors_lens"), "expected facetors_lens in {:?}", types);
        assert!(
            !types.contains(&"experience"),
            "Facetor's Lens line with an Nm token must not yield experience: {:?}",
            types
        );
    }

    #[test]
    fn player_xp_line_is_not_experience() {
        // "gain X% of your own experience" is the player-XP sacrifice — detected as
        // sacrifice_experience. The "your own" exclusion keeps it out of the
        // Add-experience bucket now that detection no longer requires an Nm amount.
        let lines = vec!["Sacrifice a Gem to gain 15% of your own experience".to_string()];
        let state = parse_font_panel(&lines);
        let types: Vec<&str> = state.options.iter().map(|o| o.option_type.as_str()).collect();
        assert!(
            types.contains(&"sacrifice_experience"),
            "expected sacrifice_experience in {:?}",
            types
        );
        assert!(
            !types.contains(&"experience"),
            "player-XP line must not be mistaken for an Add-experience option: {:?}",
            types
        );
    }

    #[test]
    fn mangled_experience_amount_still_detects_option() {
        // Decoupling win: the amount is dropped/garbled ("Add experience to a Gem")
        // but the option is still reported with value: None instead of vanishing —
        // the old `extract_millions_from_text(...).is_some()` gate dropped it whole.
        let lines = vec!["Add experience to a Gem".to_string()];
        let state = parse_font_panel(&lines);
        let exp = state.options.iter().find(|o| o.option_type == "experience");
        assert!(exp.is_some(), "experience option should be detected despite the mangled amount");
        assert_eq!(exp.unwrap().value, None);
    }

    #[test]
    fn parses_misread_panel_from_field_log() {
        // Exact misread panel captured in the field: "to"→"10" throughout, plus OCR
        // noise ("년口2"). All four real craft options must still be detected.
        let lines = vec![
            "DIVINE FONT".to_string(),
            "Transform a skill Gem 10 be a random Transfigured Gem Of the same colour".to_string(),
            "Add 30m experience 10 a Gem".to_string(),
            "Add +20% quality 10 a Gem".to_string(),
            "sacrifice a Gem for Treasure Keys".to_string(),
            "년口2".to_string(),
            "CRAFT".to_string(),
        ];
        let state = parse_font_panel(&lines);
        let types: Vec<&str> = state.options.iter().map(|o| o.option_type.as_str()).collect();
        assert!(state.font_active);
        assert!(types.contains(&"transform_random"), "missing transform_random in {:?}", types);
        assert!(types.contains(&"experience"), "missing experience in {:?}", types);
        assert!(types.contains(&"quality"), "missing quality in {:?}", types);
        assert!(types.contains(&"sacrifice_keys"), "missing sacrifice_keys in {:?}", types);
        // The amount survives the "to"→"10" misread ("Add 30m experience 10 a Gem").
        let exp = state.options.iter().find(|o| o.option_type == "experience").unwrap();
        assert_eq!(exp.value, Some(30));
        // The noise lines ("년口2", "CRAFT") produced NO spurious options — the
        // detected set is exactly these four (pin against over-detection creep).
        let mut sorted = types.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            vec!["experience", "quality", "sacrifice_keys", "transform_random"]
        );
    }

    #[test]
    fn extract_percentage_works() {
        assert_eq!(extract_percentage_from_text("Add +20% quality"), Some(20));
        assert_eq!(extract_percentage_from_text("gain 60% of the"), Some(60));
        assert_eq!(extract_percentage_from_text("gain 30% of the"), Some(30));
        assert_eq!(extract_percentage_from_text("no percentage here"), None);
    }
}
