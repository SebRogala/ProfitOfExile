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

use crate::gem_matcher::GemMatcher;
use crate::lab_scan::LabScanState;
use crate::{app_log, AppState};

#[derive(Clone)]
pub struct GemVocab {
    pub generation: u64,
    pub matcher: Arc<GemMatcher>,
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
        let names = crate::fetch_gem_names(&app, &server, &http, &mode).await;
        let count = names.len();
        let state = app.state::<AppState>();
        if install_gem_matcher(&state.lab_scan, generation, names) {
            app_log(&app, format!("Gem reader: loaded {} gem names ({})", count, mode));
        } else if count == 0 {
            app_log(&app, "Gem reader: the gem dictionary loaded 0 names, so no tooltip read can match; the next CRAFT retries. The preceding 'gem names' lines say why.".to_string());
        }
    });
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
    *slot = Some(GemVocab { generation, matcher: Arc::new(GemMatcher::new(names)) });
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
    use crate::lab::gem_reader::{read_gem_lines, GemRead};
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

    fn committed(read: GemRead) -> Option<Vec<String>> {
        match read {
            GemRead::Committed(commit) => Some(commit.names),
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

        let read = read_gem_lines(&state, token, &names(&["Dark Pact"]));
        assert_eq!(committed(read), Some(names(&["Dark Pact"])));
    }

    #[test]
    fn a_vocabulary_installed_before_a_mode_flip_is_not_used_after_it() {
        let (state, token) = armed_in("Normal");
        let (g1, _) = preload_snapshot(&state);
        assert!(install_gem_matcher(&state.lab_scan, g1, dedication_names()));
        crate::apply_manual_lab_mode(&state, "Dedication", |_| {});

        assert!(current(&state.lab_scan).is_none());
        assert!(matches!(read_gem_lines(&state, token, &names(&["Dark Pact"])), GemRead::NoVocab));
        assert!(state.lab_scan.detected_gems.lock().unwrap().is_empty());

        let (g2, _) = preload_snapshot(&state);
        assert!(install_gem_matcher(&state.lab_scan, g2, dedication_names()));
        let read = read_gem_lines(&state, token, &names(&["Dark Pact"]));
        assert_eq!(committed(read), Some(names(&["Dark Pact"])));
    }

    #[test]
    fn a_failed_preload_installs_nothing_and_leaves_the_read_skipped() {
        let (state, token) = armed_in("Normal");
        let (g, _) = preload_snapshot(&state);

        assert!(!install_gem_matcher(&state.lab_scan, g, Vec::new()));

        assert!(current(&state.lab_scan).is_none(), "the next CRAFT's preload condition must hold");
        assert!(matches!(read_gem_lines(&state, token, &names(&["Dark Pact of Trarthus"])), GemRead::NoVocab));
        assert!(state.lab_scan.detected_gems.lock().unwrap().is_empty());
    }
}
