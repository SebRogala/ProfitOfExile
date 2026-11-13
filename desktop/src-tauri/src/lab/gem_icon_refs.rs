//! The gem reader's reference icons (POE-397): the server dictionary's two
//! halves, their art from `<app_data>/gem-art/` (fetched through `gem_art`
//! when missing), decoded once into [`GemIconRefs`].
//!
//! A dictionary name the icon route answers 404 for has no icon (unique-granted,
//! retired and Vaal transfigured gems) and is excluded. Any other failure makes
//! the refs `incomplete`, which keeps every icon slot uncertain.

use std::collections::HashMap;
use std::path::Path;

use image::RgbaImage;

use super::gem_icons::{GemIconRefs, IconRef};
use crate::gem_art::{self, ArtReply};

/// The server's gem dictionary for one lab mode, split by set. In Normal mode
/// `normal` is empty (the Font offers transfigured gems only).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct GemDictionary {
    pub normal: Vec<String>,
    pub transfigured: Vec<String>,
}

impl GemDictionary {
    /// Every name, as the tooltip matcher takes it.
    pub(crate) fn merged(&self) -> Vec<String> {
        if self.normal.is_empty() {
            return self.transfigured.clone();
        }
        let mut all: Vec<String> = self.normal.iter().chain(&self.transfigured).cloned().collect();
        all.sort();
        all.dedup();
        all
    }
}

/// What one name's art came to.
pub(crate) enum ArtOutcome {
    Loaded(RgbaImage),
    /// The route answered 404: the gem has no icon.
    Iconless,
    Failed(String),
}

/// Judge one icon-route reply.
pub(crate) fn art_outcome(reply: ArtReply) -> ArtOutcome {
    if matches!(reply, ArtReply::Response(404, _)) {
        return ArtOutcome::Iconless;
    }
    match gem_art::accept_art(reply) {
        Ok(bytes) => match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) {
            Ok(img) => ArtOutcome::Loaded(img.to_rgba8()),
            Err(e) => ArtOutcome::Failed(e.to_string()),
        },
        Err(why) => ArtOutcome::Failed(why),
    }
}

/// The names that were left out of a build, and why.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct RefsReport {
    pub iconless: Vec<String>,
    pub failed: Vec<(String, String)>,
}

/// Build the refs from each name's art outcome.
pub(crate) fn build_refs(dict: &GemDictionary, mut art: impl FnMut(&str) -> ArtOutcome) -> (GemIconRefs, RefsReport) {
    let mut report = RefsReport::default();
    let mut half = |names: &[String]| -> Vec<IconRef> {
        names
            .iter()
            .filter_map(|name| match art(name) {
                ArtOutcome::Loaded(img) => IconRef::new(name, &img).or_else(|| {
                    report.failed.push((name.clone(), "no opaque pixel".to_string()));
                    None
                }),
                ArtOutcome::Iconless => {
                    report.iconless.push(name.clone());
                    None
                }
                ArtOutcome::Failed(why) => {
                    report.failed.push((name.clone(), why));
                    None
                }
            })
            .collect()
    };
    let normal = half(&dict.normal);
    let transfigured = half(&dict.transfigured);
    let incomplete = !report.failed.is_empty();
    (GemIconRefs { normal, transfigured, incomplete }, report)
}

/// Each name's art: the cache in `dir` first, then the server, writing what it
/// fetched into the cache.
pub(crate) async fn load_art(http: &reqwest::Client, server: &str, dir: &Path, names: &[String]) -> HashMap<String, ArtOutcome> {
    let mut out = HashMap::new();
    for name in names {
        if out.contains_key(name) {
            continue;
        }
        let outcome = match gem_art::read_art(dir, name) {
            Some(img) => ArtOutcome::Loaded(img),
            None => match gem_art::fetch_art(http, server, name).await {
                None => ArtOutcome::Failed(format!("server address {server:?} is not usable")),
                Some(reply) => {
                    let bytes = match &reply {
                        ArtReply::Response(200, body) => Some(body.clone()),
                        _ => None,
                    };
                    let outcome = art_outcome(reply);
                    if let (ArtOutcome::Loaded(_), Some(bytes)) = (&outcome, bytes) {
                        if let Err(e) = gem_art::write_art(dir, name, &bytes) {
                            log::warn!("gem art cache write failed: {}", e);
                        }
                    }
                    outcome
                }
            },
        };
        out.insert(name.clone(), outcome);
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::OnceLock;

    /// Fetched (never committed — see its README) by `make lab-gem-art`.
    pub(crate) const ART_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lab-gem-art");
    const SETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lab/gem-sets.json");
    const FETCH_HINT: &str = "the lab gem art is not committed (CC-BY-NC-SA icons — see its README); \
        fetch it with `make lab-gem-art`";

    #[derive(serde::Deserialize)]
    pub(crate) struct GemSets {
        pub normal: Vec<String>,
        pub transfigured: Vec<String>,
        pub iconless: Vec<String>,
    }

    pub(crate) fn gem_sets() -> GemSets {
        serde_json::from_str(&std::fs::read_to_string(SETS).expect(SETS)).expect("gem-sets.json")
    }

    /// The fixture dir as the icon route: a file is a 200, a missing one a 404.
    fn route(name: &str) -> ArtReply {
        match std::fs::read(gem_art::art_file(Path::new(ART_DIR), name)) {
            Ok(bytes) => ArtReply::Response(200, bytes),
            Err(_) => ArtReply::Response(404, Vec::new()),
        }
    }

    fn fixture_build() -> (GemIconRefs, RefsReport) {
        let sets = gem_sets();
        let dict = GemDictionary { normal: sets.normal, transfigured: sets.transfigured };
        build_refs(&dict, |name| art_outcome(route(name)))
    }

    /// Built once per test binary; it decodes 540 icons.
    pub(crate) fn fixture_refs() -> &'static std::sync::Arc<GemIconRefs> {
        static REFS: OnceLock<std::sync::Arc<GemIconRefs>> = OnceLock::new();
        REFS.get_or_init(|| std::sync::Arc::new(fixture_build().0))
    }

    /// The fixture refs with one eligible name's art failing to load.
    pub(crate) fn fixture_refs_marked_incomplete() -> GemIconRefs {
        let sets = gem_sets();
        let dict = GemDictionary { normal: sets.normal, transfigured: sets.transfigured };
        build_refs(&dict, |name| {
            if name == "Ice Nova of Frostbolts" {
                art_outcome(ArtReply::Unreachable("timed out".into()))
            } else {
                art_outcome(route(name))
            }
        })
        .0
    }

    #[test]
    fn every_eligible_reference_decodes() {
        let sets = gem_sets();
        let (refs, report) = fixture_build();

        assert!(report.failed.is_empty(), "eligible art failed: {:?}", report.failed);
        let mut iconless = report.iconless.clone();
        iconless.sort();
        let mut expected = sets.iconless.clone();
        expected.sort();
        let missing: Vec<_> = iconless.iter().filter(|n| !expected.contains(n)).collect();
        assert!(missing.is_empty(), "eligible names with no art in {ART_DIR}: {missing:?} — {FETCH_HINT}");
        assert_eq!(iconless, expected, "the excluded names are exactly gem-sets.json's iconless list");
        assert_eq!(
            refs.normal.len() + refs.transfigured.len(),
            sets.normal.len() + sets.transfigured.len() - sets.iconless.len(),
        );
        assert!(!refs.incomplete);
    }

    fn build_with_envy(status: u16) -> (GemIconRefs, RefsReport) {
        let dict = GemDictionary { normal: vec!["Envy".into()], transfigured: vec!["Reave of Refraction".into()] };
        build_refs(&dict, |name| match name {
            "Envy" => art_outcome(ArtReply::Response(status, Vec::new())),
            _ => art_outcome(route(name)),
        })
    }

    #[test]
    fn a_404_excludes_the_name_without_marking_the_refs_incomplete() {
        let (refs, report) = build_with_envy(404);

        assert_eq!(report, RefsReport { iconless: vec!["Envy".into()], failed: vec![] });
        assert!(!refs.incomplete);
    }

    #[test]
    fn a_502_marks_the_refs_incomplete() {
        let (refs, report) = build_with_envy(502);

        assert_eq!(report.failed, vec![("Envy".to_string(), "server returned 502".to_string())]);
        assert!(refs.incomplete);
        assert_eq!(refs.transfigured.len(), 1, "what loaded still installs");
    }

    #[test]
    fn the_merged_view_of_a_dedication_dictionary_holds_both_halves_once() {
        let dict = GemDictionary {
            normal: vec!["Dark Pact".into(), "Arc".into()],
            transfigured: vec!["Dark Pact of Trarthus".into(), "Arc".into()],
        };

        assert_eq!(dict.merged(), ["Arc", "Dark Pact", "Dark Pact of Trarthus"]);
    }
}
