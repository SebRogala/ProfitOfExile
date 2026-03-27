use serde::Serialize;
use strsim::jaro_winkler;

#[derive(Debug, Clone, Serialize)]
pub struct GemMatch {
    pub name: String,
    pub score: f64,
    pub ocr_raw: String,
}

pub struct GemMatcher {
    names: Vec<String>,
    names_lower: Vec<String>,
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

        if best_score >= min_threshold && (best_score - second_score) >= min_gap {
            Some(GemMatch {
                name: self.names[best_idx].clone(),
                score: best_score,
                ocr_raw: ocr_text.to_string(),
            })
        } else if best_score >= 0.95 {
            // Very high confidence — accept even without gap
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
}
