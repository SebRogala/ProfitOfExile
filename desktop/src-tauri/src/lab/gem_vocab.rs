//! The gem reader's tooltip vocabulary, preloaded in the background (POE-391
//! M2): the reader never fetches on a press, it uses what is installed.
//!
//! The vocabulary depends on the lab mode, so every mode change bumps
//! `gem_vocab_generation` (under the `lab_settings` guard, see
//! `commands::change_lab_mode`). A preload records the generation it started
//! under; a response that raced a later flip is rejected at install, and one
//! installed before a flip is rejected at read.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use super::gem_icon_refs;
use super::gem_icons::GemIconRefs;
use crate::gem_matcher::GemMatcher;
use crate::lab_scan::LabScanState;
use crate::{app_log, AppState};

#[derive(Clone)]
pub struct GemVocab {
    pub generation: u64,
    pub matcher: Arc<GemMatcher>,
    /// The icon references, installed by the preload's second stage once the
    /// art is in (POE-397); `None` until then, and the tooltip path works
    /// without them.
    pub(crate) refs: Option<Arc<GemIconRefs>>,
}

/// The generation and the lab mode a preload fetches for, read under one
/// `lab_settings` acquisition so the pair is consistent.
pub(crate) fn preload_snapshot(state: &AppState) -> (u64, String) {
    let lab = state.lab_settings.lock().unwrap_or_else(|e| e.into_inner());
    (state.lab_scan.gem_vocab_generation.load(Ordering::SeqCst), lab.lab_mode.clone())
}

/// Fetch the dictionary for the current mode and install it when it arrives.
/// Called with no lab guard held. A duplicate in-flight fetch for the same
/// generation is harmless (same content).
pub(crate) fn spawn_gem_vocab_preload(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (generation, mode) = preload_snapshot(&state);
    let server = crate::settings::server_base(&state);
    let http = state.server_http.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let dict = super::commands::fetch_gem_dictionary(&app, &server, &http, &mode).await;
        let names = dict.merged();
        let count = names.len();
        let state = app.state::<AppState>();
        if install_gem_matcher(&state.lab_scan, generation, names) {
            app_log(&app, format!("Gem reader: loaded {} gem names ({})", count, mode));
        } else {
            if count == 0 {
                app_log(&app, "Gem reader: the gem dictionary loaded 0 names, so no tooltip read can match; the next CRAFT retries. The preceding 'gem names' lines say why.".to_string());
            }
            return;
        }
        let Ok(dir) = app.path().app_data_dir().map(|d| d.join("gem-art")) else {
            app_log(&app, "Gem reader: no app data directory, so the gem icons are not loaded".to_string());
            return;
        };
        let wanted: Vec<String> = dict.normal.iter().chain(&dict.transfigured).cloned().collect();
        let mut art = gem_icon_refs::load_art(&http, &server, &dir, &wanted).await;
        let built = tauri::async_runtime::spawn_blocking(move || {
            gem_icon_refs::build_refs(&dict, |name| {
                art.remove(name).unwrap_or(gem_icon_refs::ArtOutcome::Failed("not loaded".into()))
            })
        })
        .await;
        let Ok((refs, report)) = built else {
            app_log(&app, "Gem reader: building the gem icons failed".to_string());
            return;
        };
        for (name, why) in &report.failed {
            app_log(&app, format!("Gem reader: icon for {:?} not loaded: {}", name, why));
        }
        app_log(
            &app,
            format!(
                "Gem reader: {} normal + {} transfigured gem icons, {} dictionary names have no icon{}",
                refs.normal.len(),
                refs.transfigured.len(),
                report.iconless.len(),
                if refs.incomplete { "; some failed, so icon reads stay uncertain until the next preload" } else { "" },
            ),
        );
        if !install_gem_refs(&state.lab_scan, generation, Arc::new(refs)) {
            app_log(&app, "Gem reader: the lab mode changed while the gem icons loaded; discarded".to_string());
        }
    });
}

/// Set the icon references on the installed vocabulary if both it and the
/// current generation are still `generation`.
pub(crate) fn install_gem_refs(lab: &LabScanState, generation: u64, refs: Arc<GemIconRefs>) -> bool {
    let mut slot = lab.gem_vocab.lock().unwrap_or_else(|e| e.into_inner());
    match slot.as_mut() {
        Some(vocab)
            if vocab.generation == generation
                && lab.gem_vocab_generation.load(Ordering::SeqCst) == generation =>
        {
            vocab.refs = Some(refs);
            true
        }
        _ => false,
    }
}

/// Whether a CRAFT should start a preload: no current vocabulary, or icon refs
/// that loaded incomplete (the preload retries the missing art).
pub(crate) fn needs_preload(lab: &LabScanState) -> bool {
    current(lab).map_or(true, |vocab| vocab.refs.is_some_and(|refs| refs.incomplete))
}

/// Install a matcher built from `names` if `generation` is still current.
/// Empty `names` (every failure path of `fetch_gem_names`) installs nothing.
pub(crate) fn install_gem_matcher(lab: &LabScanState, generation: u64, names: Vec<String>) -> bool {
    if names.is_empty() {
        return false;
    }
    let mut slot = lab.gem_vocab.lock().unwrap_or_else(|e| e.into_inner());
    if lab.gem_vocab_generation.load(Ordering::SeqCst) != generation {
        return false;
    }
    *slot = Some(GemVocab { generation, matcher: Arc::new(GemMatcher::new(names)), refs: None });
    true
}

/// The installed vocabulary, only if it belongs to the current generation.
pub(crate) fn current(lab: &LabScanState) -> Option<GemVocab> {
    let slot = lab.gem_vocab.lock().unwrap_or_else(|e| e.into_inner());
    slot.as_ref()
        .filter(|vocab| vocab.generation == lab.gem_vocab_generation.load(Ordering::SeqCst))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::gem_reader::{read_gem_frame, GemFrameRead};
    use crate::settings::tests::test_app_state;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| n.to_string()).collect()
    }

    /// Normal mode's dictionary is transfigured names only.
    fn normal_names() -> Vec<String> {
        names(&["Dark Pact of Trarthus", "Blade Vortex of the Scythe"])
    }

    /// Dedication merges both halves.
    fn dedication_names() -> Vec<String> {
        names(&["Dark Pact", "Dark Pact of Trarthus", "Blade Vortex", "Blade Vortex of the Scythe"])
    }

    fn armed_in(mode: &str) -> (crate::AppState, u64) {
        let state = test_app_state();
        state.lab_settings.lock().unwrap().lab_mode = mode.to_string();
        let token = crate::lab::commands::arm_gem_context(&state);
        (state, token)
    }

    /// A press hovering the middle slot of a blank 1080p frame whose tooltip
    /// OCR reads `tooltip`.
    fn hover_press(state: &crate::AppState, token: u64, tooltip: &[&str]) -> GemFrameRead {
        let lab = crate::ssot::placements_for(None).lab;
        let r = lab.gem_slots[1];
        let frame = image::DynamicImage::new_rgb8(1920, 1080);
        read_gem_frame(state, token, &frame, (0, 0), Some((r[0] + r[2] / 2, r[1] + r[3] / 2)), &lab, |_| names(tooltip))
    }

    fn committed(read: GemFrameRead) -> Option<Vec<String>> {
        match read {
            GemFrameRead::Read { commit: Some(commit), .. } => Some(commit.names),
            _ => None,
        }
    }

    #[test]
    fn a_stale_preload_completing_last_does_not_replace_the_current_vocabulary() {
        let (state, token) = armed_in("Normal");
        let (g1, _) = preload_snapshot(&state);
        crate::apply_manual_lab_mode(&state, "Dedication", |_| {});
        let (g2, mode) = preload_snapshot(&state);
        assert_eq!(mode, "Dedication");

        assert!(install_gem_matcher(&state.lab_scan, g2, dedication_names()));
        assert!(!install_gem_matcher(&state.lab_scan, g1, normal_names()));

        assert_eq!(committed(hover_press(&state, token, &["Dark Pact"])), Some(names(&["Dark Pact"])));
    }

    #[test]
    fn a_vocabulary_installed_before_a_mode_flip_is_not_used_after_it() {
        let (state, token) = armed_in("Normal");
        let (g1, _) = preload_snapshot(&state);
        assert!(install_gem_matcher(&state.lab_scan, g1, dedication_names()));
        crate::apply_manual_lab_mode(&state, "Dedication", |_| {});

        assert!(current(&state.lab_scan).is_none());
        assert!(matches!(hover_press(&state, token, &["Dark Pact"]), GemFrameRead::NoVocab));
        assert!(state.lab_scan.detected_gems.lock().unwrap().is_empty());

        let (g2, _) = preload_snapshot(&state);
        assert!(install_gem_matcher(&state.lab_scan, g2, dedication_names()));
        assert_eq!(committed(hover_press(&state, token, &["Dark Pact"])), Some(names(&["Dark Pact"])));
    }

    #[test]
    fn a_failed_preload_installs_nothing_and_leaves_the_read_skipped() {
        let (state, token) = armed_in("Normal");
        let (g, _) = preload_snapshot(&state);

        assert!(!install_gem_matcher(&state.lab_scan, g, Vec::new()));

        assert!(current(&state.lab_scan).is_none(), "the next CRAFT's preload condition must hold");
        assert!(matches!(hover_press(&state, token, &["Dark Pact of Trarthus"]), GemFrameRead::NoVocab));
        assert!(state.lab_scan.detected_gems.lock().unwrap().is_empty());
    }

    fn empty_refs() -> Arc<GemIconRefs> {
        Arc::new(GemIconRefs { normal: Vec::new(), transfigured: Vec::new(), incomplete: false })
    }

    #[test]
    fn icon_refs_built_under_a_stale_generation_are_not_installed() {
        let (state, _) = armed_in("Normal");
        let (g1, _) = preload_snapshot(&state);
        assert!(install_gem_matcher(&state.lab_scan, g1, normal_names()));
        crate::apply_manual_lab_mode(&state, "Dedication", |_| {});
        let (g2, _) = preload_snapshot(&state);
        assert!(install_gem_matcher(&state.lab_scan, g2, dedication_names()));

        assert!(!install_gem_refs(&state.lab_scan, g1, empty_refs()));

        assert!(current(&state.lab_scan).expect("g2 vocabulary").refs.is_none());
    }

    #[test]
    fn icon_refs_for_the_current_generation_install_on_its_vocabulary() {
        let (state, _) = armed_in("Normal");
        let (g, _) = preload_snapshot(&state);
        assert!(install_gem_matcher(&state.lab_scan, g, normal_names()));

        assert!(install_gem_refs(&state.lab_scan, g, empty_refs()));

        assert!(current(&state.lab_scan).expect("vocabulary").refs.is_some());
    }

    #[test]
    fn incomplete_icon_refs_ask_the_next_craft_to_preload_again() {
        let (state, _) = armed_in("Normal");
        let (g, _) = preload_snapshot(&state);
        assert!(install_gem_matcher(&state.lab_scan, g, normal_names()));
        assert!(install_gem_refs(
            &state.lab_scan,
            g,
            Arc::new(GemIconRefs { normal: Vec::new(), transfigured: Vec::new(), incomplete: true }),
        ));

        assert!(needs_preload(&state.lab_scan));
    }

    #[test]
    fn complete_icon_refs_need_no_preload() {
        let (state, _) = armed_in("Normal");
        let (g, _) = preload_snapshot(&state);
        assert!(install_gem_matcher(&state.lab_scan, g, normal_names()));
        assert!(install_gem_refs(&state.lab_scan, g, empty_refs()));

        assert!(!needs_preload(&state.lab_scan));
    }
}
