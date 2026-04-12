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

    // Add quality
    if full_lower.contains("quality to a gem") {
        let value = extract_percentage_near(lines, "quality");
        options.push(CraftOption {
            option_type: "quality".to_string(),
            text: find_line_containing(lines, "quality to a Gem").unwrap_or_default(),
            value,
        });
    }

    // Add experience
    if full_lower.contains("experience to a gem") {
        let value = extract_experience_amount(lines);
        options.push(CraftOption {
            option_type: "experience".to_string(),
            text: find_line_containing(lines, "experience to a Gem").unwrap_or_default(),
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
    fn extract_percentage_works() {
        assert_eq!(extract_percentage_from_text("Add +20% quality"), Some(20));
        assert_eq!(extract_percentage_from_text("gain 60% of the"), Some(60));
        assert_eq!(extract_percentage_from_text("gain 30% of the"), Some(30));
        assert_eq!(extract_percentage_from_text("no percentage here"), None);
    }
}
