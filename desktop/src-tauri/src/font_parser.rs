//! Font panel OCR parser — extracts craft options from OCR text lines.
//!
//! Scans for keyword anchors in each line. Does NOT try to reconstruct
//! multi-line sentences — just detects what options are present and
//! extracts numeric values (quality %, experience amount, lens %).

use serde::Serialize;

/// A detected craft option from the font panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

/// How a frame read the panel's "Crafts Remaining" line.
///
/// The three states are not interchangeable for round tracking. An absent
/// label means the panel is on its last craft (the game hides the line when
/// only one craft is left) — a real, meaningful count. A present-but-garbled
/// label means the OCR failed on a panel that does have a count, which is no
/// information at all. Collapsing both into `None` makes a torn frame
/// indistinguishable from the last craft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CountRead {
    /// A number was read after the label.
    Count(i32),
    /// No "Crafts Remaining" text on the panel — the last craft.
    LabelAbsent,
    /// The label is present but no digits followed it — OCR noise.
    LabelUnreadable,
}

/// Result of parsing font panel OCR lines.
#[derive(Debug, Clone, Serialize)]
pub struct FontPanelState {
    /// Detected craft options this round.
    pub options: Vec<CraftOption>,
    /// Crafts remaining (None if not detected).
    pub crafts_remaining: Option<i32>,
    /// The same read, keeping "label garbled" apart from "no label" for the
    /// craft ledger (`font_ledger`).
    pub count_read: CountRead,
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
    let full_compact = collapse_whitespace(&full_lower);

    // Standard transform (always first, always present when font is open).
    // Matched on whitespace-collapsed text: the lines are joined with a single
    // space, so a plain wrap already matches, but a wrap that also carried a
    // trailing space (or a tab) leaves "random  Transfigured Gem" behind.
    if full_compact.contains("random transfigured gem") {
        font_active = true;
        options.push(CraftOption {
            option_type: "transform_random".to_string(),
            text: find_line_containing(lines, "random Transfigured").unwrap_or_default(),
            value: None,
        });
    }

    // JACKPOT: direct transfigure. OCR drops the hyphen ("nonTransfigured"),
    // replaces it with a space, or substitutes an en/em dash, so the anchor is
    // matched with separators removed. The flag and the option's text come from
    // ONE lookup: a flag raised by text no line can produce would push an option
    // with an empty `text`, and merge_options would keep that empty text for the
    // whole round.
    if let Some(text) = find_non_transfigured_text(lines) {
        jackpot_detected = true;
        options.push(CraftOption {
            option_type: "transform_direct".to_string(),
            text,
            value: None,
        });
    }

    // Exchange for Empower/Enlighten/Enhance
    if full_compact.contains("empower support") {
        options.push(CraftOption {
            option_type: "exchange_exceptional".to_string(),
            text: find_line_containing(lines, "Empower Support").unwrap_or_default(),
            value: None,
        });
    }

    // Add quality — see `is_quality_option_line` for the anchor and its guard.
    if let Some(idx) = lines.iter().position(|line| is_quality_option_line(line)) {
        let line = &lines[idx];
        // The anchor line's own percentage first. Only one other line may donate
        // a number: an immediate predecessor that ENDS in a percentage, which is
        // the wrap that split "Add +20%" from "quality to a Gem". A full option
        // line above (the Facetor's "…gain 60% of the gem's…") does not end in
        // its percentage, and must not donate — merge_options would lock the
        // borrowed value in for the rest of the round.
        let mut value = extract_percentage_from_text(line);
        if value.is_none() && idx > 0 {
            let previous = &lines[idx - 1];
            if previous.trim_end().ends_with('%') {
                value = extract_percentage_from_text(previous);
            }
        }
        options.push(CraftOption {
            option_type: "quality".to_string(),
            text: line.clone(),
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
    if full_compact.contains("treasure keys") {
        options.push(CraftOption {
            option_type: "sacrifice_keys".to_string(),
            text: find_line_containing(lines, "Treasure Keys").unwrap_or_default(),
            value: None,
        });
    }

    // Sacrifice for Currency Items
    if full_compact.contains("currency items") {
        options.push(CraftOption {
            option_type: "sacrifice_currency".to_string(),
            text: find_line_containing(lines, "Currency Items").unwrap_or_default(),
            value: None,
        });
    }

    // Sacrifice for player experience
    if full_compact.contains("your own experience") {
        let value = extract_percentage_near(lines, "your own experience");
        options.push(CraftOption {
            option_type: "sacrifice_experience".to_string(),
            text: find_line_containing(lines, "your own experience").unwrap_or_default(),
            value,
        });
    }

    // Dedication: corrupted transfigured reroll
    if full_compact.contains("corrupted transfigured") {
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
        let is_reroll_line = |line: &&String| {
            let lower = collapse_whitespace(&line.to_lowercase());
            lower.contains("corrupted skill gem") && !lower.contains("transfigured")
        };
        if let Some(line) = lines.iter().find(is_reroll_line) {
            font_active = true;
            options.push(CraftOption {
                option_type: "corrupted_gem_reroll".to_string(),
                text: line.clone(),
                value: None,
            });
        }
    }

    // Crafts Remaining: N
    let count_read = if full_compact.contains("crafts remaining") {
        match extract_number_after(lines, "Crafts Remaining") {
            Some(n) => CountRead::Count(n),
            None => CountRead::LabelUnreadable,
        }
    } else {
        CountRead::LabelAbsent
    };
    if let CountRead::Count(n) = count_read {
        crafts_remaining = Some(n);
    }

    FontPanelState {
        options,
        crafts_remaining,
        count_read,
        font_active,
        jackpot_detected,
    }
}

/// Union-merge a round's accumulated craft options with a freshly parsed frame.
///
/// OCR frames of one static panel disagree: a torn frame can miss options the
/// previous frame read, and a frame can read a numeric value that an earlier
/// frame garbled. Merging keyed on `option_type` keeps everything seen in any
/// frame of the round instead of letting the newest frame overwrite the buffer.
///
/// Rules:
/// - An existing option is kept when `incoming` omits its type — a torn frame
///   must not delete a prior read.
/// - An existing option with no `value` is replaced by an incoming read that
///   carries one.
/// - An existing `Some(value)` is never overwritten, neither by `None` nor by a
///   different `Some`: the panel cannot change without ending the round, so a
///   later disagreeing number is OCR noise.
/// - First-seen order of `option_type` is preserved; newly seen types append.
pub fn merge_options(existing: &[CraftOption], incoming: &[CraftOption]) -> Vec<CraftOption> {
    let mut merged: Vec<CraftOption> = existing.to_vec();
    for option in incoming {
        match merged
            .iter_mut()
            .find(|seen| seen.option_type == option.option_type)
        {
            Some(seen) => {
                if seen.value.is_none() && option.value.is_some() {
                    *seen = option.clone();
                }
            }
            None => merged.push(option.clone()),
        }
    }
    merged
}

/// Collapse runs of whitespace (spaces, tabs) into single spaces.
///
/// OCR of a wrapped line can leave a double space or a tab inside an anchor
/// phrase; matching on the collapsed form keeps the anchor intact.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Remove whitespace and every dash-like character.
///
/// The "non-transfigured" wording is the only jackpot signal, and OCR renders
/// its hyphen inconsistently ("nonTransfigured", "non Transfigured",
/// "non–Transfigured"). Squashing separators makes all of them one string.
fn squash_separators(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !is_dash(*c))
        .collect()
}

/// The ASCII hyphen plus the Unicode dashes OCR substitutes for it.
fn is_dash(c: char) -> bool {
    matches!(c, '-' | '\u{2010}'..='\u{2015}' | '\u{2212}')
}

/// Locate the "non-transfigured" anchor, hyphen or not, and return the text the
/// jackpot option should carry.
///
/// Looks inside each line first, then at each adjacent PAIR joined the way the
/// panel reads them — a wrap can split the phrase ("…a non" / "Transfigured
/// Skill Gem"). Adjacent-only, not the whole blob: two lines that merely happen
/// to end in "non" and start with "Transfigured" pages apart are not one phrase.
fn find_non_transfigured_text(lines: &[String]) -> Option<String> {
    let carries_anchor =
        |text: &str| squash_separators(&text.to_lowercase()).contains("nontransfigured");
    if let Some(line) = lines.iter().find(|l| carries_anchor(l)) {
        return Some(line.clone());
    }
    lines.windows(2).find_map(|pair| {
        let joined = format!("{} {}", pair[0], pair[1]);
        carries_anchor(&joined).then_some(joined)
    })
}

/// Whether a line is the panel's "Add quality" craft option.
///
/// One admission rule: the word "quality" plus "add" or "gem" as a WHOLE word.
/// Whole-word matching is the guard — as substrings, "Additional" contains
/// "add", and the gem tooltip's "Additional Effects From 1-20% Quality" would
/// otherwise be read as a craft option offering 20% quality.
///
/// Precision is what this branch needs most: it is not gated on `font_active`,
/// so gem-tooltip text bleeding into a torn frame pushes a bogus option whose
/// value `merge_options` then locks in for the rest of the round.
///
/// The recall this buys back is real but bounded — the option survives the
/// "to"→"10" misread ("Add +20% quality 10 a Gem") and a swallowed value
/// ("Add qualityto a Gem", 2026-07-27 field log). It is lost only when "Add"
/// AND "Gem" both garble in the same frame ("+7% quality to a Gcm"), an
/// accepted trade: a shape-only anchor such as "% quality" cannot tell that
/// line apart from the tooltip above it.
fn is_quality_option_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("quality") && contains_word(&lower, &["add", "adds", "gem", "gems"])
}

/// Whether lowercase `text` carries one of `words` as a whole token, with
/// surrounding punctuation trimmed.
fn contains_word(text_lower: &str, words: &[&str]) -> bool {
    text_lower.split_whitespace().any(|token| {
        let trimmed = token.trim_matches(|c: char| !c.is_alphanumeric());
        words.contains(&trimmed)
    })
}

/// Find the first line containing a case-insensitive substring, compared on
/// whitespace-collapsed text so a doubled space inside the line still matches.
fn find_line_containing(lines: &[String], needle: &str) -> Option<String> {
    let needle_lower = needle.to_lowercase();
    lines
        .iter()
        .find(|l| collapse_whitespace(&l.to_lowercase()).contains(&needle_lower))
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
        // Collapsed, so a doubled space inside the label ("Crafts  Remaining: 7")
        // still finds the keyword — the detection gate above is collapsed too, and
        // a label the gate accepts but this misses reads as LabelUnreadable.
        let lower = collapse_whitespace(&line.to_lowercase());
        if let Some(idx) = lower.find(&keyword_lower) {
            let after = &lower[idx + keyword_lower.len()..];
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
    fn a_readable_count_line_reads_as_a_count() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Crafts Remaining: 7".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(state.count_read, CountRead::Count(7));
    }

    #[test]
    fn a_count_line_without_digits_reads_as_unreadable() {
        // OCR found the label but garbled the number. This is not the last
        // craft — the ledger must not treat it as a count change.
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Crafts Remaining: |".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(state.count_read, CountRead::LabelUnreadable);
    }

    #[test]
    fn a_panel_with_no_count_line_reads_as_the_last_craft() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Add +8% quality to a Gem".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(state.count_read, CountRead::LabelAbsent);
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
        // The "quality" anchor and its "add"/"gem" guard survive it.
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
    fn quality_is_detected_when_ocr_swallows_the_percentage() {
        // 2026-07-27 field log: OCR dropped the value and the space, leaving
        // "Add qualityto a Gem". The old "% quality" anchor lost the option whole.
        let lines = vec!["Add qualityto a Gem".to_string()];
        let state = parse_font_panel(&lines);
        assert_eq!(types_of(&state.options), vec!["quality"]);
        assert_eq!(state.options[0].value, None);
    }

    #[test]
    fn a_gem_tooltip_quality_line_yields_no_quality_option() {
        // A gem tooltip bleeding into a torn frame carries "Quality: +20%" with
        // neither "add" nor "gem" on the line. Admitting it would push a bogus
        // option whose value merge_options then locks in for the whole round.
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Quality: +20% (augmented)".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(types_of(&state.options), vec!["transform_random"]);
    }

    #[test]
    fn a_quality_and_an_experience_line_yield_one_option_each() {
        // Both lines carry "Gem", and the experience line is the one that could
        // slip into the quality bucket — the "quality" word is what keeps it out.
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Add +20% quality to a Gem".to_string(),
            "Add 150m experience to a Gem".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(
            types_of(&state.options),
            vec!["transform_random", "quality", "experience"]
        );
        let quality = state.options.iter().find(|o| o.option_type == "quality").unwrap();
        assert_eq!(quality.text, "Add +20% quality to a Gem");
    }

    #[test]
    fn a_double_spaced_transform_wrap_still_matches_the_anchor() {
        // A wrap that also carried a trailing space leaves two spaces inside the
        // anchor phrase once the lines are joined.
        let lines =
            vec!["Transform a Skill Gem to be a random  Transfigured Gem".to_string()];
        let state = parse_font_panel(&lines);
        assert!(state.font_active);
        assert_eq!(types_of(&state.options), vec!["transform_random"]);
    }

    #[test]
    fn a_double_spaced_line_is_still_found_for_the_option_text() {
        // Separate from the anchor match: the option's `text` comes from a
        // line lookup, which does its own collapsing.
        let lines =
            vec!["Transform a Skill Gem to be a random  Transfigured Gem".to_string()];
        let state = parse_font_panel(&lines);
        assert_eq!(
            state.options[0].text,
            "Transform a Skill Gem to be a random  Transfigured Gem"
        );
    }

    #[test]
    fn jackpot_is_detected_when_ocr_drops_the_hyphen() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Transform a nonTransfigured Skill Gem".to_string(),
            "to a Transfigured version".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.jackpot_detected);
        let direct = state
            .options
            .iter()
            .find(|o| o.option_type == "transform_direct")
            .expect("jackpot option missing");
        assert_eq!(direct.text, "Transform a nonTransfigured Skill Gem");
    }

    #[test]
    fn jackpot_is_detected_when_ocr_replaces_the_hyphen_with_a_space() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Transform a non Transfigured Skill Gem".to_string(),
            "to a Transfigured version".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.jackpot_detected);
        let direct = state
            .options
            .iter()
            .find(|o| o.option_type == "transform_direct")
            .expect("jackpot option missing");
        assert_eq!(direct.text, "Transform a non Transfigured Skill Gem");
    }

    #[test]
    fn a_tooltip_range_line_without_colon_yields_no_quality_option() {
        // Gem tooltip line: "Additional" CONTAINS "add" and the line carries a
        // "% Quality" shape with a number. Admitting it would push a quality
        // option valued 20 that merge_options keeps for the whole round. Only
        // whole-word "add"/"gem" matching rejects it — inside an active panel.
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Additional Effects From 1-20% Quality".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(types_of(&state.options), vec!["transform_random"]);
    }

    #[test]
    fn a_quality_line_with_add_and_gem_both_garbled_yields_no_option() {
        // The accepted cost of whole-word admission: with "Add" dropped and
        // "Gem" read as "Gcm", nothing on the line is admissible. Recovering it
        // would mean anchoring on the "% quality" shape, which the gem tooltip
        // "Additional Effects From 1-20% Quality" also has — a swap of a rare
        // miss for a value-poisoning false positive.
        let lines = vec!["+7% quality to a Gcm".to_string()];
        let state = parse_font_panel(&lines);
        assert!(state.options.is_empty(), "unexpected options: {:?}", state.options);
    }

    #[test]
    fn a_preceding_facetor_line_does_not_donate_its_percentage_to_quality() {
        // The Facetor line is a complete option of its own, not the head of a
        // wrapped quality line — its 60% must not become the quality value, which
        // merge_options would then keep for the whole round.
        let lines = vec![
            "Sacrifice a Gem to gain 60% of the gem's total experience stored as a Facetor's Lens"
                .to_string(),
            "Add qualityto a Gem".to_string(),
        ];
        let state = parse_font_panel(&lines);
        let quality = state
            .options
            .iter()
            .find(|o| o.option_type == "quality")
            .expect("quality option missing");
        assert_eq!(quality.value, None);
    }

    #[test]
    fn a_wrapped_quality_value_is_read_from_the_line_above() {
        // The wrap cut the option right after its value, so the previous line
        // ends in the percentage — the one case that may donate a number.
        let lines = vec!["Add +20%".to_string(), "quality to a Gem".to_string()];
        let state = parse_font_panel(&lines);
        let quality = state
            .options
            .iter()
            .find(|o| o.option_type == "quality")
            .expect("quality option missing");
        assert_eq!(quality.value, Some(20));
    }

    #[test]
    fn jackpot_is_detected_when_the_anchor_wraps_across_two_lines() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Transform a non".to_string(),
            "Transfigured Skill Gem".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.jackpot_detected);
        let direct = state
            .options
            .iter()
            .find(|o| o.option_type == "transform_direct")
            .expect("jackpot option missing");
        // The flag and the text come from one lookup, so a wrap-detected jackpot
        // still carries the joined phrase instead of an empty string.
        assert_eq!(direct.text, "Transform a non Transfigured Skill Gem");
    }

    #[test]
    fn jackpot_is_detected_when_ocr_substitutes_an_en_dash() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Transform a non\u{2013}Transfigured Skill Gem".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert!(state.jackpot_detected);
        let direct = state
            .options
            .iter()
            .find(|o| o.option_type == "transform_direct")
            .expect("jackpot option missing");
        assert_eq!(direct.text, "Transform a non\u{2013}Transfigured Skill Gem");
    }

    #[test]
    fn a_double_spaced_crafts_remaining_line_still_reads_as_a_count() {
        let lines = vec![
            "Transform a Skill Gem to be a random".to_string(),
            "Transfigured Gem of the same colour".to_string(),
            "Crafts  Remaining: 7".to_string(),
        ];
        let state = parse_font_panel(&lines);
        assert_eq!(state.count_read, CountRead::Count(7));
        assert_eq!(state.crafts_remaining, Some(7));
    }

    /// Build a craft option the way `parse_font_panel` would, without going
    /// through OCR — `merge_options` only reads `option_type` and `value`.
    fn opt(option_type: &str, value: Option<i32>) -> CraftOption {
        CraftOption {
            option_type: option_type.to_string(),
            text: format!("{} line", option_type),
            value,
        }
    }

    fn types_of(options: &[CraftOption]) -> Vec<&str> {
        options.iter().map(|o| o.option_type.as_str()).collect()
    }

    #[test]
    fn merges_two_disjoint_frames_into_one_option_set() {
        let seen_so_far = vec![opt("transform_random", None), opt("quality", Some(20))];
        let next_frame = vec![opt("experience", Some(30)), opt("sacrifice_keys", None)];

        let merged = merge_options(&seen_so_far, &next_frame);

        assert_eq!(
            types_of(&merged),
            vec!["transform_random", "quality", "experience", "sacrifice_keys"]
        );
        assert_eq!(merged[1].value, Some(20));
        assert_eq!(merged[2].value, Some(30));
    }

    #[test]
    fn torn_one_option_frame_keeps_the_options_it_missed() {
        let seen_so_far = vec![
            opt("transform_random", None),
            opt("quality", Some(20)),
            opt("sacrifice_keys", None),
        ];
        let torn_frame = vec![opt("quality", Some(20))];

        let merged = merge_options(&seen_so_far, &torn_frame);

        assert_eq!(
            types_of(&merged),
            vec!["transform_random", "quality", "sacrifice_keys"]
        );
    }

    #[test]
    fn known_facetor_value_survives_a_later_valueless_read() {
        // Negative case: a frame that garbled the percentage must not erase the
        // percentage an earlier frame read cleanly.
        let seen_so_far = vec![opt("facetors_lens", Some(60))];
        let valueless_frame = vec![opt("facetors_lens", None)];

        let merged = merge_options(&seen_so_far, &valueless_frame);

        assert_eq!(types_of(&merged), vec!["facetors_lens"]);
        assert_eq!(merged[0].value, Some(60));
    }

    #[test]
    fn missing_facetor_value_is_upgraded_by_a_later_read() {
        let seen_so_far = vec![opt("facetors_lens", None)];
        let readable_frame = vec![opt("facetors_lens", Some(60))];

        let merged = merge_options(&seen_so_far, &readable_frame);

        assert_eq!(types_of(&merged), vec!["facetors_lens"]);
        assert_eq!(merged[0].value, Some(60));
    }

    #[test]
    fn merging_three_frames_keeps_first_seen_option_order() {
        let first = vec![opt("transform_random", None), opt("facetors_lens", None)];
        let second = vec![opt("facetors_lens", None), opt("experience", Some(30))];
        // The third frame re-reads a type first seen in frame one, this time
        // with a readable value: the in-place upgrade must leave the option at
        // its first-seen index, while the genuinely new type appends at the end.
        let third = vec![opt("sacrifice_keys", None), opt("facetors_lens", Some(60))];

        let merged = merge_options(&merge_options(&first, &second), &third);

        assert_eq!(
            types_of(&merged),
            vec!["transform_random", "facetors_lens", "experience", "sacrifice_keys"]
        );
        assert_eq!(merged[1].value, Some(60));
    }

    #[test]
    fn first_read_value_wins_over_a_later_disagreeing_value() {
        // The panel cannot change mid-round, so a second, different number is
        // OCR noise — the value already read stands.
        let seen_so_far = vec![opt("facetors_lens", Some(60))];
        let disagreeing_frame = vec![opt("facetors_lens", Some(30))];

        let merged = merge_options(&seen_so_far, &disagreeing_frame);

        assert_eq!(types_of(&merged), vec!["facetors_lens"]);
        assert_eq!(merged[0].value, Some(60));
    }

    #[test]
    fn an_empty_incoming_frame_leaves_the_buffer_unchanged() {
        let seen_so_far = vec![opt("transform_random", None), opt("quality", Some(20))];

        let merged = merge_options(&seen_so_far, &[]);

        assert_eq!(merged, seen_so_far);
    }

    #[test]
    fn merging_into_an_empty_buffer_yields_the_incoming_frame() {
        let first_frame = vec![opt("transform_random", None), opt("quality", Some(20))];

        let merged = merge_options(&[], &first_frame);

        assert_eq!(merged, first_frame);
    }

    #[test]
    fn extract_percentage_works() {
        assert_eq!(extract_percentage_from_text("Add +20% quality"), Some(20));
        assert_eq!(extract_percentage_from_text("gain 60% of the"), Some(60));
        assert_eq!(extract_percentage_from_text("gain 30% of the"), Some(30));
        assert_eq!(extract_percentage_from_text("no percentage here"), None);
    }
}
