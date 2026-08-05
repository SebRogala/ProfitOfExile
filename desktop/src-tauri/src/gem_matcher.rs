use serde::Serialize;
use strsim::jaro_winkler;

#[derive(Debug, Clone, Serialize)]
pub struct GemMatch {
    pub name: String,
    pub score: f64,
    pub ocr_raw: String,
}

/// Last-token similarity at or above which OCR text is treated as a support-gem
/// name and rejected outright.
///
/// Measured 2026-08-05 against the local gem dictionary (818 distinct names —
/// 225 support, 593 in the matcher vocabulary), scoring each name's final
/// whitespace token against "support":
///   - highest among the 593 vocabulary names: 0.771 ("Summon Raging Spirit"),
///     then 0.764 ("Toxic Rain of Sporeburst");
///   - lowest among the 225 support names: 1.000 (every one ends in " Support");
///   - plausible OCR manglings of the token: 0.905 ("5upport") to 0.975
///     ("suppport", "Support.").
/// 0.85 sits in the empty band between 0.771 and 0.905.
const SUPPORT_TOKEN_THRESHOLD: f64 = 0.85;

pub struct GemMatcher {
    names: Vec<String>,
    names_lower: Vec<String>,
}

/// Whether OCR text has the shape of a support-gem name: its final token is
/// "support", allowing for OCR noise in that token.
///
/// Fuzzy rather than an exact `ends_with(" support")` because the token is what
/// OCR mangles ("Suppon", "5upport", "supporl"), and a strict suffix test would
/// hand exactly those reads back to the fall-through this gate exists to close.
fn is_support_shaped(query_lower: &str) -> bool {
    query_lower
        .split_whitespace()
        .next_back()
        .is_some_and(|last| jaro_winkler(last, "support") >= SUPPORT_TOKEN_THRESHOLD)
}

impl GemMatcher {
    pub fn new(names: Vec<String>) -> Self {
        let names_lower = names.iter().map(|n| n.to_lowercase()).collect();
        Self { names, names_lower }
    }

    /// Match OCR text against known gem names.
    /// Returns the best match if score >= min_threshold and gap to second-best >= min_gap.
    pub fn match_gem(&self, ocr_text: &str) -> Option<GemMatch> {
        let query = ocr_text.trim().to_lowercase();
        if query.is_empty() {
            return None;
        }

        // Support-gem reject gate. Game rule: the Font of Enhancement and the
        // Dedication to the Goddess hand out skill gems only, so a support gem is
        // never a possible outcome of either craft and its name must never resolve
        // to one.
        //
        // POE-144 enforced that by deleting the 224 " Support"-suffixed names from
        // the transfigured=false dictionary. That stopped support text matching
        // *itself*, but not from reaching this matcher — extract_gem_candidates
        // accepts any OCR line over 2 chars that does not start with a stat prefix,
        // so a socketed item tooltip or an adjacent inventory gem is enough. With
        // the name gone, jaro_winkler redirected it to the nearest surviving
        // neighbour instead: measured 2026-08-05 over the local dictionary, 13 of
        // the 225 removed names cleared the 0.80 threshold, topped by
        // "Ancestral Call Support" -> "Ancestral Cry" (0.894) and
        // "Barrage Support" -> "Barrage" (0.893). Which 13 depends on the
        // vocabulary the server happens to serve, which is why the cure is keyed on
        // the name's shape rather than on a list of names.
        //
        // The reject rule is the client half of the server's isSupportGem
        // (internal/lab/repository.go), which documents the suffix test as
        // exhaustive: gem_snapshots is written only from poe.ninja's SkillGem
        // endpoint, whose only non-active-skill category is support gems, and every
        // support gem's name ends in " Support". POE-142 is expected to fold both
        // halves into one gem-eligibility source of truth.
        //
        // Scoped to the outcome pool, which is the only thing this matcher reads.
        // A future OCR path for Dedication *feed* gems — where a support gem IS
        // legal (isDedicationFeed, internal/lab/dedication.go) — needs its own
        // matcher or an opt-out, not a relaxation of this gate.
        if is_support_shaped(&query) {
            log::debug!("Rejected support-gem candidate '{}' (support gems are never a Font or Dedication outcome)", ocr_text);
            return None;
        }

        let min_threshold = 0.80;
        let min_gap = 0.05;

        let mut best_score = 0.0_f64;
        let mut best_idx = 0;
        let mut second_score = 0.0_f64;

        for (i, name) in self.names_lower.iter().enumerate() {
            let score = jaro_winkler(&query, name);
            if score > best_score {
                second_score = best_score;
                best_score = score;
                best_idx = i;
            } else if score > second_score {
                second_score = score;
            }
        }

        // Transfigured-gem shape gate, keyed on the MATCHED name (not the mode):
        // a transfigured name always contains " of " (e.g. "Divine Ire of
        // Disintegration"). If the winning name has " of " but the OCR text does
        // not, the text is arbitrary UI/area/chat noise that jaro_winkler merely
        // prefix-inflated — reject it. This is the false-positive fix. Keying on
        // the name (not a mode flag) covers the Dedication pool too, where
        // transfigured and non-transfigured skill gems share one matcher: skill
        // gems have no " of " in their name so this gate never touches them.
        if self.names_lower[best_idx].contains(" of ") && !query.contains(" of ") {
            return None;
        }

        if best_score >= min_threshold && (best_score - second_score) >= min_gap {
            Some(GemMatch {
                name: self.names[best_idx].clone(),
                score: best_score,
                ocr_raw: ocr_text.to_string(),
            })
        } else if best_score >= 0.95 {
            // Very high confidence — accept even without a gap to second-best.
            // Retained deliberately: it absorbs OCR character confusions (l<->I,
            // I<->1, etc.) that push a clean read to a near-tie with a sibling
            // variant. The name-keyed shape gate above already blocks the
            // false-positive class, so this no-gap path no longer admits noise.
            Some(GemMatch {
                name: self.names[best_idx].clone(),
                score: best_score,
                ocr_raw: ocr_text.to_string(),
            })
        } else {
            log::debug!(
                "No match for '{}': best={:.3} ({}), second={:.3}, gap={:.3}",
                ocr_text,
                best_score,
                self.names[best_idx],
                second_score,
                best_score - second_score
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_matcher() -> GemMatcher {
        GemMatcher::new(vec![
            "Earthquake of Fragility".into(),
            "Boneshatter of Carnage".into(),
            "Summon Stone Golem of Safeguarding".into(),
            "Summon Ice Golem of Hordes".into(),
            "Summon Stone Golem of Hordes".into(),
            "Vaal Impurity of Ice".into(),
            "Vaal Impurity of Fire".into(),
        ])
    }

    #[test]
    fn exact_match_returns_high_score() {
        let m = test_matcher();
        let result = m.match_gem("Earthquake of Fragility").unwrap();
        assert_eq!(result.name, "Earthquake of Fragility");
        assert!((result.score - 1.0).abs() < 0.001);
    }

    #[test]
    fn ocr_typo_still_matches() {
        let m = test_matcher();
        // Simulated OCR error: 'i' -> 'l'
        let result = m.match_gem("Earthquale of Fraglllty");
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "Earthquake of Fragility");
    }

    #[test]
    fn garbage_returns_none() {
        let m = test_matcher();
        let result = m.match_gem("xyzzy random garbage text");
        assert!(result.is_none());
    }

    #[test]
    fn close_names_with_clear_input_match_correctly() {
        let m = test_matcher();
        let result = m.match_gem("Summon Stone Golem of Safeguarding").unwrap();
        assert_eq!(result.name, "Summon Stone Golem of Safeguarding");
    }

    #[test]
    fn empty_input_returns_none() {
        let m = test_matcher();
        assert!(m.match_gem("").is_none());
        assert!(m.match_gem("   ").is_none());
    }

    #[test]
    fn text_without_of_connector_does_not_match_transfigured_name() {
        // Core false-positive guard: OCR text lacking " of " must not match a
        // transfigured (" of "-bearing) name, even when prefix-similar — the
        // gate keys on the matched name, so it holds regardless of mode.
        // "earthquake fragility" drops the " of " the target name carries.
        let m = test_matcher();
        assert!(m.match_gem("earthquake fragility").is_none());
    }

    #[test]
    fn skill_gem_without_of_in_name_still_matches() {
        // Non-transfigured skill gems (Dedication pool) have no " of " in their
        // name, so the name-keyed gate never applies — they must still match.
        let m = GemMatcher::new(vec!["Barrage".into(), "Rage Vortex".into()]);
        let result = m.match_gem("Barrage").unwrap();
        assert_eq!(result.name, "Barrage");
    }

    #[test]
    fn no_of_text_does_not_match_transfigured_in_mixed_pool() {
        // Dedication uses ONE matcher holding both skill gems and transfigured
        // gems. The name-keyed gate must still protect the transfigured names
        // here: non-" of " OCR noise must not resolve to a transfigured gem,
        // even though the pool also contains gateless skill gems.
        let m = GemMatcher::new(vec![
            "Barrage".into(),
            "Rage Vortex".into(),
            "Divine Ire of Disintegration".into(),
        ]);
        // Text prefix-similar to the transfigured name but lacking " of ".
        assert!(m.match_gem("divine ire disintegration").is_none());
    }

    /// The nine skill gems the POE-145 measurement recorded as absorbing a
    /// support-gem read, plus the pair measured locally on 2026-08-05.
    fn support_neighbour_matcher() -> GemMatcher {
        GemMatcher::new(vec![
            "Barrage".into(),
            "Rage Vortex".into(),
            "Corrupting Fever".into(),
            "Damage Infusion".into(),
            "Ignite".into(),
            "Energy Blade".into(),
            "Devouring Totem".into(),
            "General's Cry".into(),
            "Winter Orb".into(),
            "Ancestral Cry".into(),
        ])
    }

    #[test]
    fn support_gem_text_is_rejected_not_redirected_to_nearest_skill_gem() {
        // POE-145 regression set. Each of these is a support-gem name POE-144
        // deleted from the dictionary; with the name gone, jaro_winkler handed the
        // read to the neighbour in the comment, above the 0.80 threshold, so the
        // player saw a priced card for a gem the lab cannot hand out.
        //
        // Scores are this matcher's own, replayed 2026-08-05 against the
        // vocabulary above with the reject gate disabled. Eight reproduce the
        // POE-145 figures exactly; "Generosity Support" does not — it scores 0.694
        // against "General's Cry", not the 0.816 the ticket recorded, and
        // jaro_winkler over a fixed pair is vocabulary-independent, so that row of
        // the ticket is wrong. It stays in the replay as the reported set; the
        // other nine are what make this test fail if the gate goes.
        let m = support_neighbour_matcher();
        for name in [
            "Ancestral Call Support",       // 0.894 -> Ancestral Cry
            "Barrage Support",              // 0.893 -> Barrage
            "Rage Support",                 // 0.879 -> Rage Vortex
            "Corrupting Cry Support",       // 0.859 -> Corrupting Fever
            "Damage on Full Life Support",  // 0.854 -> Damage Infusion
            "Ignite Proliferation Support", // 0.843 -> Ignite
            "Energy Leech Support",         // 0.840 -> Energy Blade
            "Devour Support",               // 0.826 -> Devouring Totem
            "Windburst Support",            // 0.815 -> Winter Orb
            "Generosity Support",           // 0.694 -> General's Cry (below threshold)
        ] {
            assert!(
                m.match_gem(name).is_none(),
                "'{}' must be rejected as a support gem, got {:?}",
                name,
                m.match_gem(name).map(|g| (g.name, g.score))
            );
        }
    }

    #[test]
    fn support_gem_read_with_a_mangled_support_token_is_still_rejected() {
        // The reject gate is fuzzy on the final token because OCR is what mangles
        // it. An exact " Support" suffix test would pass "Barrage Suppon" straight
        // through to "Barrage" at 0.900 — the very fall-through the gate closes.
        let m = support_neighbour_matcher();
        for name in ["Barrage Suppon", "Barrage 5upport", "Barrage supporl"] {
            assert!(
                m.match_gem(name).is_none(),
                "'{}' must be rejected as a support gem, got {:?}",
                name,
                m.match_gem(name).map(|g| (g.name, g.score))
            );
        }
    }

    #[test]
    fn skill_gem_whose_last_token_resembles_support_still_matches() {
        // Accept-side boundary. "Spirit" scores 0.771 against "support" — the
        // closest any of the 593 dictionary names measured on 2026-08-05 comes to
        // the reject token, and the name below the 0.85 gate by the widest
        // relevant margin. Raising the gate's tolerance far enough to swallow it
        // costs a real Dedication option.
        let m = GemMatcher::new(vec![
            "Summon Raging Spirit".into(),
            "Toxic Rain of Sporeburst".into(),
        ]);
        let result = m
            .match_gem("Summon Raging Spirit")
            .expect("'Summon Raging Spirit' must still match — the support reject gate is too wide");
        assert_eq!(result.name, "Summon Raging Spirit");
        assert!((result.score - 1.0).abs() < 0.001);
    }
}
