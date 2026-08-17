//! The mercenary stat vocabulary (POE-165 D3).
//!
//! GGG's `Mercenary`-labelled block of `/api/trade/data/stats`, compiled into
//! the binary from the same committed fixture the TypeScript rulesets are
//! asserted against (`lib/mercenaries/__fixtures__/mercenary-stats.json`), so
//! Rust and the webview can never drift onto two different vocabularies.
//!
//! # This is NOT the player-gem dictionary
//!
//! `gem_matcher.rs` matches Font/Dedication outcomes and carries two rules
//! that are wrong here and must not be inherited:
//!
//! - its **support-shape reject** ("the text ends in Support → not a gem") —
//!   here support links are half the vocabulary, and a trailing " Support" in
//!   a tooltip title is *stripped*, not a reason to discard the read;
//! - its **" of " connector gate** (a transfigured gem name needs the
//!   connector) — merc skills such as "Ball Lightning of Orbiting Trap" are
//!   plain vocabulary entries with no transfigured/base distinction.
//!
//! Only the scoring *algorithm* (lowercased Jaro-Winkler, best score with a
//! lead over the runner-up) is shared, and it is re-implemented here rather
//! than reused, because the gates around it are the part that differs.
//!
//! # Names are display text, not keys
//!
//! `Gilded Extra Targets (Tier 3)` exists under two different ids. Every
//! lookup here therefore resolves to an id **set**; the verdict engine tests
//! presence by set intersection, never by single-id equality.

use serde::Deserialize;
use strsim::jaro_winkler;

use super::{MercGeometry, ReadState, Thresholds};

/// The vocabulary block, exactly as `GET /api/trade/data/stats` returns it.
const RAW_VOCAB: &str = include_str!(
    "../../../src/lib/mercenaries/__fixtures__/mercenary-stats.json"
);

/// Id prefix marking an active skill entry.
const SKILL_PREFIX: &str = "mercenary.skill_";
/// Id prefix marking a support-link entry.
const SUPPORT_PREFIX: &str = "mercenary.support_";

/// Grade prefixes a support name can carry. Stripping them yields the icon
/// FAMILY, which is what a learned template is keyed on together with the
/// tier: `Lesser Chain (Tier 1)`, `Chain (Tier 2)` and `Gilded Chain (Tier 3)`
/// are three tiers of one family.
const GRADE_PREFIXES: [&str; 3] = ["Lesser ", "Greater ", "Gilded "];

#[derive(Deserialize)]
struct RawVocab {
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    id: String,
    text: String,
}

/// Which half of the vocabulary an entry belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MercRole {
    /// An active skill — read from the skill-name column by OCR.
    Skill,
    /// A support link — read from a support cell by icon template + badge.
    Support,
}

/// One vocabulary entry, with the family/tier decomposition supports need.
#[derive(Debug, Clone, PartialEq)]
pub struct MercStat {
    pub id: String,
    /// Display text, verbatim — including the `(Tier N)` suffix.
    pub name: String,
    pub role: MercRole,
    /// `name` minus a leading grade word and minus the `(Tier N)` suffix.
    /// Derived for skills too (one code path), but only meaningful for
    /// supports, which are the only entries [`MercVocab::resolve`] returns.
    pub family: String,
    /// Support tier 1-3. `None` for skills, which carry no tier.
    pub tier: Option<u8>,
    /// `name` minus only the `(Tier N)` suffix — the form a tooltip may show.
    pub qualified: String,
}

/// A name read: the ids it resolves to, and how confident it is.
#[derive(Debug, Clone, PartialEq)]
pub struct NameRead {
    pub ids: Vec<String>,
    pub name: Option<String>,
    pub score: f32,
    pub state: ReadState,
    /// The runner-up's score — surfaced so the debug report can show WHY a
    /// read lost its lead.
    pub runner_up: f32,
}

/// A hover-tooltip title read (D5): which support family it names, and the
/// tier if the title carried one.
#[derive(Debug, Clone, PartialEq)]
pub struct SupportTitleRead {
    pub family: Option<String>,
    pub tier: Option<u8>,
    pub name: Option<String>,
    pub ids: Vec<String>,
    pub score: f32,
    pub state: ReadState,
}

/// Strip a trailing `(Tier N)` suffix, returning the rest and the tier.
fn split_tier(text: &str) -> (&str, Option<u8>) {
    let trimmed = text.trim_end();
    let Some(open) = trimmed.rfind(" (Tier ") else {
        return (trimmed, None);
    };
    if !trimmed.ends_with(')') {
        return (trimmed, None);
    }
    let digits = &trimmed[open + " (Tier ".len()..trimmed.len() - 1];
    match digits.parse::<u8>() {
        Ok(t) => (&trimmed[..open], Some(t)),
        Err(_) => (trimmed, None),
    }
}

/// Strip a leading grade word (`Lesser `/`Greater `/`Gilded `).
fn strip_grade(name: &str) -> &str {
    for p in GRADE_PREFIXES {
        if let Some(rest) = name.strip_prefix(p) {
            return rest;
        }
    }
    name
}

/// Drop a trailing " Support" from a tooltip title (fuzzy — OCR mangles that
/// token as readily as any other). Titles that are ONLY that token are left
/// alone; there is nothing to match on afterwards.
fn strip_support_suffix(text: &str) -> &str {
    let trimmed = text.trim();
    let mut it = trimmed.rsplitn(2, char::is_whitespace);
    let (Some(last), Some(head)) = (it.next(), it.next()) else {
        return trimmed;
    };
    // The same scorer `gem_matcher` gates on, called for the opposite action:
    // there a support-shaped read is discarded, here its suffix is removed and
    // the rest is matched. Sharing the fn keeps one threshold to recalibrate.
    if crate::gem_matcher::is_support_shaped(&last.to_lowercase()) {
        head.trim_end()
    } else {
        trimmed
    }
}

/// One scored candidate during matching.
struct Scored<'a> {
    stat: &'a MercStat,
    /// Which spelling of the entry scored — the label reported back.
    label: &'a str,
    score: f64,
}

pub struct MercVocab {
    stats: Vec<MercStat>,
}

impl MercVocab {
    /// Parse the compiled-in vocabulary.
    ///
    /// `Err` is only reachable if the committed fixture stops being the shape
    /// GGG serves (a unit test here pins the parse), so the capture loop can
    /// surface it as `last_error` instead of the module dying on an
    /// `unwrap`.
    pub fn load() -> Result<Self, String> {
        let raw: RawVocab = serde_json::from_str(RAW_VOCAB)
            .map_err(|e| format!("mercenary-stats.json did not parse: {e}"))?;
        let stats = raw
            .entries
            .into_iter()
            .filter_map(|e| {
                let role = if e.id.starts_with(SKILL_PREFIX) {
                    MercRole::Skill
                } else if e.id.starts_with(SUPPORT_PREFIX) {
                    MercRole::Support
                } else {
                    // An id shape this build does not know: skipped rather
                    // than guessed at, so a future `mercenary.*` kind cannot
                    // silently land in the skill or the support pool.
                    return None;
                };
                let (qualified, tier) = split_tier(&e.text);
                Some(MercStat {
                    id: e.id,
                    family: strip_grade(qualified).to_string(),
                    qualified: qualified.to_string(),
                    name: e.text,
                    role,
                    tier,
                })
            })
            .collect();
        Ok(Self { stats })
    }

    /// Build a vocabulary from hand-made entries.
    ///
    /// Test-only: the shipped vocabulary cannot express a skill carrying a
    /// tier, which is the case [`Self::resolve`]'s role filter defends
    /// against, so that filter has no reachable seam without this.
    #[cfg(test)]
    pub fn from_stats(stats: Vec<MercStat>) -> Self {
        Self { stats }
    }

    /// The whole vocabulary. Test-only in this build — the two read paths go
    /// through `by_role` / `resolve`, and the parse-conformance tests are what
    /// walk the raw list.
    #[allow(dead_code)]
    pub fn stats(&self) -> &[MercStat] {
        &self.stats
    }

    /// Every entry of one role.
    pub fn by_role(&self, role: MercRole) -> impl Iterator<Item = &MercStat> {
        self.stats.iter().filter(move |s| s.role == role)
    }

    /// Support entries sharing a `(family, tier)`.
    ///
    /// Returns a SET because `(Pierce, 3)` is two different links (Greater and
    /// Gilded) — the icon cannot tell them apart, so the read is reported as
    /// ambiguous rather than resolved to whichever came first.
    pub fn resolve(&self, family: &str, tier: u8) -> Vec<&MercStat> {
        self.stats
            .iter()
            .filter(|s| s.role == MercRole::Support && s.family == family && s.tier == Some(tier))
            .collect()
    }

    /// Match OCR text from the skill-name column against the skill half of the
    /// vocabulary.
    pub fn match_skill(&self, text: &str, thresholds: &Thresholds) -> NameRead {
        let query = text.trim().to_lowercase();
        let scored: Vec<Scored> = self
            .by_role(MercRole::Skill)
            .map(|stat| Scored {
                stat,
                label: &stat.name,
                score: jaro_winkler(&query, &stat.name.to_lowercase()),
            })
            .collect();
        self.finish(&query, scored, thresholds)
    }

    /// Match a hover-tooltip title against the support half.
    ///
    /// The title's spelling is unknown until the first Windows dump (does it
    /// carry the tier? the " Support" suffix?), so all three spellings compete
    /// at once: the full name (`Greater Pierce (Tier 3)`), the qualified name
    /// (`Greater Pierce`) and the bare family (`Pierce`). A qualified or full
    /// hit carries a tier; a bare-family hit does not, and the caller supplies
    /// the badge tier.
    pub fn match_support_title(&self, text: &str, thresholds: &Thresholds) -> SupportTitleRead {
        let stripped = strip_support_suffix(text);
        let query = stripped.trim().to_lowercase();

        let mut scored: Vec<Scored> = Vec::with_capacity(self.stats.len() * 3);
        for stat in self.by_role(MercRole::Support) {
            for label in [
                stat.name.as_str(),
                stat.qualified.as_str(),
                stat.family.as_str(),
            ] {
                scored.push(Scored {
                    stat,
                    label,
                    score: jaro_winkler(&query, &label.to_lowercase()),
                });
            }
        }

        let Some(best) = best_of(&scored) else {
            return SupportTitleRead {
                family: None,
                tier: None,
                name: None,
                ids: Vec::new(),
                score: 0.0,
                state: ReadState::Unknown,
            };
        };
        // The lead that matters is over a DIFFERENT family: the same entry
        // scoring three times (full / qualified / family) is not competition.
        let runner_up = scored
            .iter()
            .filter(|s| s.stat.family != best.stat.family)
            .map(|s| s.score)
            .fold(0.0_f64, f64::max);
        let state = classify(best.score, runner_up, thresholds);
        if state == ReadState::Unknown {
            return SupportTitleRead {
                family: None,
                tier: None,
                name: None,
                ids: Vec::new(),
                score: best.score as f32,
                state,
            };
        }

        // A family-only hit names no tier, so it names no ids either.
        let tier = if best.label == best.stat.family.as_str() {
            None
        } else {
            best.stat.tier
        };
        let ids = match tier {
            Some(t) => self
                .stats
                .iter()
                .filter(|s| {
                    s.role == MercRole::Support
                        && s.tier == Some(t)
                        && s.qualified == best.stat.qualified
                })
                .map(|s| s.id.clone())
                .collect(),
            None => Vec::new(),
        };
        SupportTitleRead {
            family: Some(best.stat.family.clone()),
            tier,
            name: tier.map(|_| best.stat.name.clone()),
            ids,
            score: best.score as f32,
            state,
        }
    }

    /// Best-of + threshold classification shared by the name matchers.
    fn finish(&self, query: &str, scored: Vec<Scored>, thresholds: &Thresholds) -> NameRead {
        if query.is_empty() {
            return NameRead {
                ids: Vec::new(),
                name: None,
                score: 0.0,
                state: ReadState::Unknown,
                runner_up: 0.0,
            };
        }
        let Some(best) = best_of(&scored) else {
            return NameRead {
                ids: Vec::new(),
                name: None,
                score: 0.0,
                state: ReadState::Unknown,
                runner_up: 0.0,
            };
        };
        let runner_up = scored
            .iter()
            .filter(|s| s.stat.name != best.stat.name)
            .map(|s| s.score)
            .fold(0.0_f64, f64::max);
        let state = classify(best.score, runner_up, thresholds);
        let ids = if state == ReadState::Unknown {
            Vec::new()
        } else {
            scored
                .iter()
                .filter(|s| s.stat.name == best.stat.name)
                .map(|s| s.stat.id.clone())
                .collect()
        };
        NameRead {
            ids,
            name: (state != ReadState::Unknown).then(|| best.stat.name.clone()),
            score: best.score as f32,
            state,
            runner_up: runner_up as f32,
        }
    }
}

/// Highest scorer, ties broken by the earlier entry (stable).
fn best_of<'a, 'b>(scored: &'b [Scored<'a>]) -> Option<&'b Scored<'a>> {
    scored
        .iter()
        .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal))
}

/// The D3 threshold rule, in one place: MATCH with a lead over the runner-up,
/// unless the score is high enough that the lead stops being required.
fn classify(best: f64, runner_up: f64, t: &Thresholds) -> ReadState {
    let best = best as f32;
    let runner_up = runner_up as f32;
    if best >= t.name_match && (best >= t.name_no_lead || best - runner_up >= t.name_lead) {
        ReadState::Matched
    } else if best >= t.name_low {
        ReadState::LowConfidence
    } else {
        ReadState::Unknown
    }
}

/// Classify a `(family, tier)` resolution into the read it produces.
///
/// Zero entries is `Unknown` (the tier and the family disagree); more than one
/// NAME is `Ambiguous` (Greater vs Gilded at tier 3 — the icon cannot tell
/// them apart); one name is `Matched`, with every id that name carries.
pub fn classify_resolution(matches: &[&MercStat]) -> (Vec<String>, Option<String>, ReadState, Vec<String>) {
    let mut names: Vec<String> = Vec::new();
    for m in matches {
        if !names.contains(&m.name) {
            names.push(m.name.clone());
        }
    }
    let ids: Vec<String> = matches.iter().map(|m| m.id.clone()).collect();
    match names.len() {
        0 => (Vec::new(), None, ReadState::Unknown, Vec::new()),
        1 => (ids, Some(names[0].clone()), ReadState::Matched, Vec::new()),
        _ => (ids, None, ReadState::Ambiguous, names),
    }
}

/// The default thresholds, for callers that have no [`MercGeometry`] at hand.
///
/// Test-only in this build: WI-3's capture loop and debug command both build a
/// [`MercGeometry`] (defaults merged with the JSON override) and pass its
/// `thresholds`, which is the only way a recalibration reaches the matcher.
/// Kept because every matcher test would otherwise reach through a geometry it
/// does not care about.
#[allow(dead_code)]
pub fn default_thresholds() -> Thresholds {
    MercGeometry::default().thresholds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab() -> MercVocab {
        MercVocab::load().expect("the compiled-in vocabulary parses")
    }

    fn t() -> Thresholds {
        default_thresholds()
    }

    /// The compiled-in fixture is the whole vocabulary and splits into the two
    /// halves the two read paths use. Counts are the committed file's
    /// (decoded 2026-08-17, league Allflame); a re-fetch that changes them
    /// should fail here and be looked at, not absorbed silently.
    #[test]
    fn the_vocabulary_parses_into_268_skills_and_266_supports() {
        let v = vocab();

        assert_eq!(v.stats().len(), 534);
        assert_eq!(v.by_role(MercRole::Skill).count(), 268);
        assert_eq!(v.by_role(MercRole::Support).count(), 266);
    }

    /// Family/tier derivation on the named D3 example plus the three grade
    /// prefixes and the ungraded middle tier — this decomposition is what the
    /// icon template store is keyed on, so a wrong strip mis-keys every
    /// learned template.
    #[test]
    fn support_names_decompose_into_family_and_tier() {
        let v = vocab();
        let by_name = |n: &str| {
            v.stats()
                .iter()
                .find(|s| s.name == n)
                .unwrap_or_else(|| panic!("{n} is in the vocabulary"))
                .clone()
        };

        let greater = by_name("Greater Pierce (Tier 3)");
        assert_eq!(greater.family, "Pierce");
        assert_eq!(greater.tier, Some(3));
        assert_eq!(greater.qualified, "Greater Pierce");
        assert_eq!(greater.role, MercRole::Support);

        assert_eq!(by_name("Lesser Chain (Tier 1)").family, "Chain");
        assert_eq!(by_name("Lesser Chain (Tier 1)").tier, Some(1));
        assert_eq!(by_name("Chain (Tier 2)").family, "Chain");
        assert_eq!(by_name("Chain (Tier 2)").tier, Some(2));
        assert_eq!(by_name("Gilded Caustic Conversion (Tier 3)").family, "Caustic Conversion");
    }

    /// Skills carry no tier — deriving one would let a badge tier "resolve"
    /// against a skill entry.
    #[test]
    fn skills_carry_no_tier() {
        let v = vocab();

        let skills_with_a_tier: Vec<&str> = v
            .by_role(MercRole::Skill)
            .filter(|s| s.tier.is_some())
            .map(|s| s.name.as_str())
            .collect();

        assert!(
            skills_with_a_tier.is_empty(),
            "skills must have no tier, got {skills_with_a_tier:?}",
        );
    }

    /// `resolve` is set-valued because `(Pierce, 3)` is genuinely two links.
    /// Returning the first would name a support the player may not have.
    #[test]
    fn resolving_pierce_tier_3_yields_both_greater_and_gilded() {
        let v = vocab();

        let hits = v.resolve("Pierce", 3);

        let mut names: Vec<&str> = hits.iter().map(|s| s.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["Gilded Pierce (Tier 3)", "Greater Pierce (Tier 3)"]);
    }

    /// The same family at a tier only one grade reaches resolves to one link.
    #[test]
    fn resolving_pierce_tier_1_yields_only_the_lesser_link() {
        let v = vocab();

        let hits = v.resolve("Pierce", 1);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Lesser Pierce (Tier 1)");
    }

    /// A tier a family SKIPS must resolve to nothing rather than to the
    /// nearest one. "Multiple Projectiles" exists at tiers 1 and 3 only, so
    /// tier 2 is a real hole in a real family — the case a
    /// nearest-tier fallback would paper over and a nonexistent family name
    /// would not reach.
    #[test]
    fn resolving_a_tier_a_family_skips_yields_nothing() {
        let v = vocab();
        let tiers: Vec<Option<u8>> = v
            .by_role(MercRole::Support)
            .filter(|s| s.family == "Multiple Projectiles")
            .map(|s| s.tier)
            .collect();
        assert_eq!(
            tiers,
            [Some(1), Some(3)],
            "precondition: the family really does skip tier 2",
        );

        assert!(v.resolve("Multiple Projectiles", 2).is_empty());
    }

    /// A family name that does not exist at all resolves to nothing — the
    /// other way a lookup can miss.
    #[test]
    fn resolving_an_unknown_family_yields_nothing() {
        let v = vocab();

        assert!(v.resolve("Not A Family", 3).is_empty());
    }

    fn stat(id: &str, name: &str, role: MercRole, family: &str, tier: Option<u8>) -> MercStat {
        MercStat {
            id: id.to_string(),
            name: name.to_string(),
            qualified: name.to_string(),
            family: family.to_string(),
            role,
            tier,
        }
    }

    /// `resolve` only ever returns supports. Today's vocabulary gives skills
    /// no tier, so the tier test alone would hide a missing role filter — the
    /// entries here are hand-made precisely so a skill CAN carry a tier, which
    /// is what a future vocabulary change would introduce.
    #[test]
    fn resolve_never_returns_a_skill_even_when_one_carries_a_tier() {
        let v = MercVocab::from_stats(vec![
            stat("skill_x", "Chain", MercRole::Skill, "Chain", Some(1)),
            stat("support_x", "Lesser Chain", MercRole::Support, "Chain", Some(1)),
        ]);

        let hits = v.resolve("Chain", 1);

        assert_eq!(hits.len(), 1, "got {hits:?}");
        assert_eq!(hits[0].id, "support_x");
    }

    /// The shipped vocabulary really does contain the family collision the
    /// filter guards — "Stormcall" is derived as a family by a SKILL
    /// ("Greater Stormcall") as well as being a skill name.
    #[test]
    fn a_skill_family_collides_with_a_support_family_in_the_real_vocabulary() {
        let v = vocab();

        assert!(
            v.by_role(MercRole::Skill).any(|s| s.family == "Stormcall"),
            "a skill derives the family 'Stormcall'",
        );
    }

    /// The read path: an OCR mangling of a real skill name still matches it.
    /// "Ice Shot" mangled to "lce Shot" (capital I read as lowercase L) is the
    /// canonical PoE OCR failure.
    #[test]
    fn a_mangled_skill_name_still_matches() {
        let v = vocab();

        let read = v.match_skill("Vaal lce Shot", &t());

        assert_eq!(read.state, ReadState::Matched, "read was {read:?}");
        assert_eq!(read.name.as_deref(), Some("Vaal Ice Shot"));
        assert_eq!(read.ids.len(), 1);
    }

    /// The lead rule's reason for existing: "Vaal Ice Shot" and "Ice Shot" are
    /// both real skills, and the shorter one is a substring of the longer. A
    /// clean read of the longer must resolve to the longer.
    #[test]
    fn vaal_ice_shot_does_not_collapse_into_ice_shot() {
        let v = vocab();

        let read = v.match_skill("Vaal Ice Shot", &t());

        assert_eq!(read.name.as_deref(), Some("Vaal Ice Shot"));
        assert_eq!(read.state, ReadState::Matched);
        assert!(
            read.runner_up < read.score,
            "the runner-up must not tie the winner: {read:?}",
        );
    }

    /// …and the reverse: a clean read of the shorter name must not be inflated
    /// into the longer one by the prefix bonus.
    #[test]
    fn ice_shot_does_not_inflate_into_vaal_ice_shot() {
        let v = vocab();

        let read = v.match_skill("Ice Shot", &t());

        assert_eq!(read.name.as_deref(), Some("Ice Shot"));
        assert_eq!(read.state, ReadState::Matched);
    }

    /// Text that is nothing like a skill must not be handed the nearest name.
    /// "Wager: 1 028" is a line the detector really does see on this panel.
    #[test]
    fn panel_chrome_text_does_not_match_a_skill() {
        let v = vocab();

        let read = v.match_skill("Wager: 1 028", &t());

        assert_eq!(read.state, ReadState::Unknown, "read was {read:?}");
        assert!(read.ids.is_empty());
        assert!(read.name.is_none());
    }

    /// Empty text is the boundary: no vocabulary entry, no score, no guess.
    #[test]
    fn empty_text_matches_nothing() {
        let v = vocab();

        let read = v.match_skill("   ", &t());

        assert_eq!(read.state, ReadState::Unknown);
        assert_eq!(read.score, 0.0);
        assert!(read.ids.is_empty());
    }

    /// A read that clears LOW but not MATCH is reported as low-confidence, not
    /// promoted and not discarded — the verdict engine needs the difference
    /// (low-confidence propagates as UNKNOWN, never as presence).
    #[test]
    fn a_score_between_low_and_match_is_low_confidence() {
        let v = vocab();
        // Measured, not constructed: two of PoE's own OCR confusions on a
        // short name ("Fl" read as "Fi", "h" as "n") land at 0.880 against
        // "Flame Dash" — inside the 0.85..0.92 band.
        let read = v.match_skill("Fiame Dasn", &t());

        assert_eq!(read.state, ReadState::LowConfidence, "read was {read:?}");
        assert!(
            read.score >= t().name_low && read.score < t().name_match,
            "score {} must sit in the LOW..MATCH band",
            read.score,
        );
        assert_eq!(read.name.as_deref(), Some("Flame Dash"), "still names its best guess");
    }

    /// The tooltip title path with the suffix the vocabulary does not carry.
    /// The gem matcher would REJECT this text outright; here it must resolve.
    #[test]
    fn a_tooltip_title_with_a_support_suffix_resolves_to_the_link() {
        let v = vocab();

        let read = v.match_support_title("Greater Pierce (Tier 3) Support", &t());

        assert_eq!(read.state, ReadState::Matched, "read was {read:?}");
        assert_eq!(read.family.as_deref(), Some("Pierce"));
        assert_eq!(read.tier, Some(3));
        assert_eq!(read.name.as_deref(), Some("Greater Pierce (Tier 3)"));
        assert_eq!(read.ids.len(), 1);
    }

    /// The strip has to happen BEFORE scoring, not be absorbed by the fuzzy
    /// match: on a one-word family the un-stripped title scores 0.877 (low
    /// confidence) and the stripped one scores 1.0. The long titles above
    /// would match either way, which is why this short one is here.
    #[test]
    fn a_short_title_needs_its_support_suffix_stripped_to_match() {
        let v = vocab();

        let read = v.match_support_title("Chain Support", &t());

        assert_eq!(read.state, ReadState::Matched, "read was {read:?}");
        assert_eq!(read.family.as_deref(), Some("Chain"));
    }

    /// A title without the tier suffix still names the tier, because the
    /// qualified name is unique per tier across the whole vocabulary.
    #[test]
    fn a_qualified_title_without_the_tier_suffix_still_names_the_tier() {
        let v = vocab();

        let read = v.match_support_title("Lesser Chain", &t());

        assert_eq!(read.family.as_deref(), Some("Chain"));
        assert_eq!(read.tier, Some(1));
        assert_eq!(read.name.as_deref(), Some("Lesser Chain (Tier 1)"));
    }

    /// A bare family title names the family and NO tier: the badge supplies
    /// it. Reporting a tier here would invent one out of whichever grade
    /// happened to score first.
    #[test]
    fn a_bare_family_title_names_no_tier_and_no_ids() {
        let v = vocab();

        let read = v.match_support_title("Pierce", &t());

        assert_eq!(read.family.as_deref(), Some("Pierce"));
        assert_eq!(read.tier, None);
        assert!(read.ids.is_empty(), "no tier means no id set: {read:?}");
    }

    /// Tooltip lines that are not a support title (D5 logs them and leaves the
    /// cell alone) must not be forced onto the nearest family.
    #[test]
    fn a_non_title_tooltip_line_matches_no_support() {
        let v = vocab();

        let read = v.match_support_title("Requires Level 38", &t());

        assert_eq!(read.state, ReadState::Unknown, "read was {read:?}");
        assert!(read.family.is_none());
    }

    /// The duplicate-name pair resolves to BOTH its ids — presence is set
    /// intersection, so dropping one would make a real link unmatchable.
    #[test]
    fn the_duplicate_name_resolves_to_both_of_its_ids() {
        let v = vocab();

        let read = v.match_support_title("Gilded Extra Targets (Tier 3)", &t());

        assert_eq!(read.state, ReadState::Matched);
        assert_eq!(read.ids.len(), 2, "read was {read:?}");
        assert_ne!(read.ids[0], read.ids[1]);
    }

    /// `(Pierce, 3)` carries two DIFFERENT names, which the icon cannot
    /// separate — the read must say so rather than pick one.
    #[test]
    fn a_two_name_resolution_is_ambiguous_with_both_candidates_listed() {
        let v = vocab();

        let (ids, name, state, candidates) = classify_resolution(&v.resolve("Pierce", 3));

        assert_eq!(state, ReadState::Ambiguous);
        assert!(name.is_none(), "an ambiguous read must not name one link");
        assert_eq!(ids.len(), 2);
        assert_eq!(candidates.len(), 2);
    }

    /// A one-name resolution that spans two ids is MATCHED, not ambiguous:
    /// the ambiguity that matters is which LINK it is, not which id row GGG
    /// filed it under.
    #[test]
    fn a_one_name_two_id_resolution_is_matched() {
        let v = vocab();

        let (ids, name, state, candidates) = classify_resolution(&v.resolve("Extra Targets", 3));

        assert_eq!(state, ReadState::Matched);
        assert_eq!(name.as_deref(), Some("Gilded Extra Targets (Tier 3)"));
        assert_eq!(ids.len(), 2);
        assert!(candidates.is_empty());
    }

    /// The ids the seven committed saved searches actually gate on.
    fn ruleset_ids() -> Vec<String> {
        const FIXTURES: [&str; 7] = [
            include_str!("../../../src/lib/mercenaries/__fixtures__/WvKGjV8Kfm.json"),
            include_str!("../../../src/lib/mercenaries/__fixtures__/LgkKKmllTn.json"),
            include_str!("../../../src/lib/mercenaries/__fixtures__/5nd22GvKCa.json"),
            include_str!("../../../src/lib/mercenaries/__fixtures__/7nRvBzl2S5.json"),
            include_str!("../../../src/lib/mercenaries/__fixtures__/BgzkZKGQF8.json"),
            include_str!("../../../src/lib/mercenaries/__fixtures__/LgkGrPO5Fn.json"),
            include_str!("../../../src/lib/mercenaries/__fixtures__/zbrQyEqah4.json"),
        ];
        let mut ids = Vec::new();
        for raw in FIXTURES {
            let v: serde_json::Value = serde_json::from_str(raw).expect("fixture parses");
            for group in v["query"]["stats"].as_array().expect("stats array") {
                for f in group["filters"].as_array().expect("filters array") {
                    ids.push(f["id"].as_str().expect("filter id").to_string());
                }
            }
        }
        ids
    }

    /// The duplicate-name hazard, bounded against the real rulesets: no id a
    /// saved search gates on shares its display text with a different id. If
    /// one ever did, a name-keyed lookup would resolve it to a set containing
    /// an id the search does not want, and set-intersection presence would
    /// fire on the wrong link.
    #[test]
    fn no_ruleset_id_sits_in_a_duplicate_name_pair() {
        let v = vocab();
        let ids = ruleset_ids();
        assert!(ids.len() > 20, "precondition: the fixtures carry ids, got {}", ids.len());

        let offenders: Vec<String> = ids
            .iter()
            .filter_map(|id| {
                let stat = v.stats().iter().find(|s| &s.id == id)?;
                let sharing = v.stats().iter().filter(|s| s.name == stat.name).count();
                (sharing > 1).then(|| format!("{} ({})", stat.name, id))
            })
            .collect();

        assert!(
            offenders.is_empty(),
            "these ruleset ids share a display text with another id: {offenders:?}",
        );
    }

    /// Every id the rulesets gate on exists in the vocabulary — the two
    /// fixtures were captured from the same API and must agree, or a rule
    /// would reference a link the capture can never produce.
    #[test]
    fn every_ruleset_id_exists_in_the_vocabulary() {
        let v = vocab();

        let missing: Vec<String> = ruleset_ids()
            .into_iter()
            .filter(|id| !v.stats().iter().any(|s| &s.id == id))
            .collect();

        assert!(missing.is_empty(), "ids absent from the vocabulary: {missing:?}");
    }

    /// `(Tier N)` stripping is a suffix rule, not a substring rule — a name
    /// merely containing the words must survive intact.
    #[test]
    fn split_tier_only_strips_a_real_trailing_suffix() {
        assert_eq!(split_tier("Greater Pierce (Tier 3)"), ("Greater Pierce", Some(3)));
        assert_eq!(split_tier("Ball Lightning of Orbiting Trap"), ("Ball Lightning of Orbiting Trap", None));
        assert_eq!(split_tier("Odd (Tier X)"), ("Odd (Tier X)", None));
        assert_eq!(split_tier("Truncated (Tier 3"), ("Truncated (Tier 3", None));
    }

    /// Grade stripping must only fire on a leading grade WORD: "Gildedwing"
    /// is not a Gilded anything.
    #[test]
    fn strip_grade_requires_the_trailing_space() {
        assert_eq!(strip_grade("Gilded Pierce"), "Pierce");
        assert_eq!(strip_grade("Gildedwing"), "Gildedwing");
        assert_eq!(strip_grade("Pierce"), "Pierce");
    }

    /// The suffix stripper is fuzzy because OCR mangles that token too, and it
    /// must never eat the whole title.
    #[test]
    fn the_support_suffix_stripper_handles_ocr_noise_and_one_word_titles() {
        assert_eq!(strip_support_suffix("Greater Pierce Support"), "Greater Pierce");
        assert_eq!(strip_support_suffix("Greater Pierce 5upport"), "Greater Pierce");
        assert_eq!(strip_support_suffix("Greater Pierce"), "Greater Pierce");
        assert_eq!(strip_support_suffix("Support"), "Support");
    }

    /// The shared token test must not fire on ordinary merc vocabulary words,
    /// or every title's last word would be eaten. `gem_matcher` owns the
    /// threshold; what this pins is that the merc vocabulary is safe under it,
    /// which that module's own tests (scored against the PLAYER gem
    /// dictionary) do not cover.
    #[test]
    fn the_shared_support_token_test_does_not_fire_on_merc_vocabulary_words() {
        use crate::gem_matcher::is_support_shaped;

        assert!(is_support_shaped("greater pierce support"));
        assert!(!is_support_shaped("summon skitterbots"));
        assert!(!is_support_shaped("ball lightning of orbiting trap"));
        let v = vocab();
        let tripped: Vec<&str> = v
            .stats()
            .iter()
            .filter(|s| is_support_shaped(&s.name.to_lowercase()))
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            tripped.is_empty(),
            "no vocabulary name may look support-shaped, got {tripped:?}",
        );
    }
}
