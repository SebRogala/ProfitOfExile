//! The Divine Font gem reader (POE-391): one tooltip OCR of the `lab.gem`
//! band per read key press while a gem context is armed (CRAFT until
//! CONFIRM, FontClosed, ZoneChanged or 3/3).

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use super::{gem_vocab, once_per_context, report_engine_once, LogOnce};
use crate::capture::Capture;
use crate::gem_matcher::{GemMatch, GemMatcher};
use crate::lab_scan::LabScanState;
use crate::read_hotkey::{PendingRead, Press, ScreenReader};
use crate::{app_log, AppState, AppStatus};

const MAX_GEMS: usize = 3;
const MAX_REJECT_LOGS: usize = 12;

pub struct LabGemReader;

/// The token a press captures, or `None` when no gem context is armed.
pub(crate) fn armed_token(lab: &LabScanState) -> Option<u64> {
    Some(lab.gem_context.load(Ordering::SeqCst)).filter(|token| *token != 0)
}

impl ScreenReader<AppHandle> for LabGemReader {
    fn arm(&self, app: &AppHandle, _press: &Press) -> Option<Box<dyn PendingRead<AppHandle>>> {
        armed_token(&app.state::<AppState>().lab_scan).map(|token| Box::new(PendingGemRead { token }) as _)
    }
}

struct PendingGemRead {
    token: u64,
}

impl PendingRead<AppHandle> for PendingGemRead {
    fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, app: &AppHandle) {
        consume(app, self.token, frame);
    }
}

/// Per-context log bookkeeping on the dispatcher thread.
#[derive(Default)]
struct ContextLog {
    token: u64,
    presses: u32,
    rejects: HashSet<String>,
    rejects_suppressed: bool,
}

thread_local! {
    static LOGGED: std::cell::Cell<(u64, u8)> = const { std::cell::Cell::new((0, 0)) };
    static CONTEXT_LOG: std::cell::RefCell<ContextLog> = std::cell::RefCell::new(ContextLog::default());
}

fn with_context_log<R>(token: u64, f: impl FnOnce(&mut ContextLog) -> R) -> R {
    CONTEXT_LOG.with(|log| {
        let mut log = log.borrow_mut();
        if log.token != token {
            *log = ContextLog { token, ..ContextLog::default() };
        }
        f(&mut log)
    })
}

fn consume(app: &AppHandle, token: u64, frame: Result<&Arc<Capture>, &str>) {
    report_engine_once(app);
    let state = app.state::<AppState>();
    let once = |what: LogOnce| once_per_context(&LOGGED, token, what);
    if gem_vocab::current(&state.lab_scan).is_none() {
        if once(LogOnce::NoVocab) {
            app_log(app, "Gem read skipped: the gem dictionary has not loaded yet".to_string());
        }
        return;
    }
    let grab = match frame {
        Ok(grab) => grab,
        Err(e) => {
            if once(LogOnce::FrameError) {
                app_log(app, format!("Gem read: screen grab failed: {}", e));
            }
            return;
        }
    };
    let press = with_context_log(token, |log| {
        log.presses += 1;
        log.presses
    });
    let screen = *state.screen.lock().unwrap_or_else(|e| e.into_inner());
    let region = crate::capture_region(crate::ssot::placements_for(screen.as_ref()).lab.gem);
    if screen.is_none() && once(LogOnce::Unscaled) {
        app_log(
            app,
            format!(
                "lab OCR regions unscaled: no screen measured yet — the gem region assumes 1080p ({}, {}) {}x{}",
                region.x, region.y, region.w, region.h,
            ),
        );
    }
    let cropped = grab.image.crop_imm(region.x.max(0) as u32, region.y.max(0) as u32, region.w, region.h);
    if let Some(line) = crate::crop_shortfall(&region, cropped.width(), cropped.height()) {
        if once(LogOnce::ShortCrop) {
            app_log(app, line);
        }
    }
    if state.gem_band_dump.load(Ordering::SeqCst) {
        crate::dump_gem_band(app, &cropped, press);
    }

    // Two kernels on the one frame: Lanczos3 2× first, then bilinear when it
    // yields no candidates. Measured 2026-09-09 on band dumps: a hovered
    // "Split Arrow of Splitting" header read nothing through Lanczos3 2× and
    // cleanly through other kernels (the loop this replaced alternated them
    // per tick).
    let mut kernel = "lanczos";
    let mut lines = recognize(app, once, &crate::capture::preprocess_for_ocr(&cropped));
    if crate::ocr::extract_gem_candidates(&lines).is_empty() {
        kernel = "bilinear";
        lines = recognize(app, once, &crate::capture::preprocess_for_ocr_fast(&cropped));
    }
    let candidates = crate::ocr::extract_gem_candidates(&lines);
    if !candidates.is_empty() {
        app_log(app, format!("Gem OCR candidates ({}): {:?}", kernel, candidates));
    }
    let read = read_gem_lines(&state, token, &lines);
    publish_gem_read(app, token, read);
}

fn recognize(app: &AppHandle, once: impl Fn(LogOnce) -> bool, image: &image::DynamicImage) -> Vec<String> {
    match crate::ocr::recognize_text(image) {
        Ok(lines) => lines,
        Err(e) => {
            if once(LogOnce::OcrError) {
                app_log(app, format!("Gem OCR failed: {}", e));
            }
            Vec::new()
        }
    }
}

/// What one press's tooltip lines did.
pub(crate) enum GemRead {
    /// No current vocabulary: skipped.
    NoVocab,
    /// No candidate matched; the rejections, for the capped log.
    NoMatch { rejects: Vec<String> },
    /// A new gem was committed.
    Committed(GemCommit),
    /// The match was not committed: a repeat, a full list, or a stale token
    /// or vocabulary.
    NotCommitted { matched: GemMatch },
}

/// A committed gem, as owned data built with no lab guard held.
#[derive(Debug, Clone)]
pub(crate) struct GemCommit {
    pub names: Vec<String>,
    pub added: String,
    pub complete: bool,
    pub status: AppStatus,
}

/// The best match among the candidates, and every rejection.
fn best_match(matcher: &GemMatcher, candidates: &[String]) -> (Option<GemMatch>, Vec<String>) {
    let mut best: Option<GemMatch> = None;
    let mut rejects = Vec::new();
    for candidate in candidates {
        match matcher.match_gem(candidate) {
            Ok(m) => {
                if best.as_ref().map_or(true, |b| m.score > b.score) {
                    best = Some(m);
                }
            }
            Err(reason) => rejects.push(format!("Gem OCR rejected {:?}: {}", candidate, reason)),
        }
    }
    (best, rejects)
}

/// The pure step of a gem read: match the lines against the current
/// vocabulary and commit the best match under the captured token and that
/// vocabulary's generation.
pub(crate) fn read_gem_lines(state: &AppState, captured: u64, lines: &[String]) -> GemRead {
    let Some(vocab) = gem_vocab::current(&state.lab_scan) else { return GemRead::NoVocab };
    let candidates = crate::ocr::extract_gem_candidates(lines);
    let (best, rejects) = best_match(&vocab.matcher, &candidates);
    let Some(matched) = best else { return GemRead::NoMatch { rejects } };
    match commit_gem_name(state, captured, vocab.generation, &matched.name) {
        Some(commit) => GemRead::Committed(commit),
        None => GemRead::NotCommitted { matched },
    }
}

/// Add `name` to the held gems if the context `captured` is still armed, the
/// vocabulary generation `vocab_gen` is still current (r2-H1), the name is new
/// and fewer than 3 are held. At 3/3 the context ends. The status is built
/// after the `detected_gems` guard is released (`build_status` takes it).
pub(crate) fn commit_gem_name(state: &AppState, captured: u64, vocab_gen: u64, name: &str) -> Option<GemCommit> {
    let lab = &state.lab_scan;
    let (names, complete) = {
        let mut gems = lab.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
        if lab.gem_context.load(Ordering::SeqCst) != captured
            || lab.gem_vocab_generation.load(Ordering::SeqCst) != vocab_gen
            || gems.len() >= MAX_GEMS
            || gems.iter().any(|held| held == name)
        {
            return None;
        }
        gems.push(name.to_string());
        let complete = gems.len() >= MAX_GEMS;
        if complete {
            lab.gem_context.store(0, Ordering::SeqCst);
        }
        (gems.clone(), complete)
    };
    if complete {
        let mut lab_state = state.lab_state.lock().unwrap_or_else(|e| e.into_inner());
        // A CRAFT re-arm in between stores its token before it sets
        // PickingGems, so it is never overwritten here.
        if lab.gem_context.load(Ordering::SeqCst) == 0 {
            *lab_state = crate::lab_state::LabState::Idle;
        }
    }
    let status = crate::build_status(state);
    Some(GemCommit { names, added: name.to_string(), complete, status })
}

/// The lock-free adapter: emit, send and log a commit.
pub(crate) fn publish_gem_commit(app: &AppHandle, commit: GemCommit) {
    if let Err(e) = app.emit("gem-detected", &commit.added) {
        log::warn!("emit gem-detected failed: {}", e);
    }
    if let Err(e) = app.emit("status-changed", commit.status) {
        log::warn!("emit status-changed failed: {}", e);
    }
    let names = commit.names.clone();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::send_gems_to_server(&app_clone, names).await;
    });
    if commit.complete {
        app_log(app, "Gem scan complete (3/3 gems detected)".to_string());
    }
}

fn publish_gem_read(app: &AppHandle, token: u64, read: GemRead) {
    match read {
        GemRead::NoVocab => {}
        GemRead::NoMatch { rejects } => with_context_log(token, |log| {
            for line in rejects {
                if log.rejects.len() < MAX_REJECT_LOGS {
                    if log.rejects.insert(line.clone()) {
                        app_log(app, line);
                    }
                } else if !log.rejects_suppressed {
                    log.rejects_suppressed = true;
                    app_log(
                        app,
                        format!("Gem OCR: {} distinct rejections logged — further rejections suppressed for this gem choice", MAX_REJECT_LOGS),
                    );
                }
            }
        }),
        GemRead::Committed(commit) => {
            app_log(
                app,
                format!("Gem detected: {} [{}/{}]", commit.added, commit.names.len(), MAX_GEMS),
            );
            publish_gem_commit(app, commit);
        }
        GemRead::NotCommitted { matched } => app_log(
            app,
            format!(
                "Gem OCR: {:?} read as {} — not added (already held, 3/3, or the gem choice or lab mode changed)",
                matched.ocr_raw, matched.name,
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::commands::{arm_gem_context, disarm_gem_context};
    use crate::settings::tests::test_app_state;
    use std::time::Duration;

    fn held(state: &AppState) -> Vec<String> {
        state.lab_scan.detected_gems.lock().unwrap().clone()
    }

    fn with_vocab(state: &AppState) -> u64 {
        let generation = state.lab_scan.gem_vocab_generation.load(Ordering::SeqCst);
        assert!(gem_vocab::install_gem_matcher(
            &state.lab_scan,
            generation,
            ["Fireball of Trarthus", "Blade Vortex of the Scythe", "Reave of Refraction", "Blade Flurry of Incision"]
                .map(String::from)
                .to_vec(),
        ));
        generation
    }

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn a_tooltip_press_commits_the_gem_with_a_status_built_after_the_guard() {
        let state = Arc::new(test_app_state());
        let token = arm_gem_context(&state);
        with_vocab(&state);

        let (tx, rx) = std::sync::mpsc::channel();
        let worker_state = state.clone();
        std::thread::spawn(move || {
            let read = read_gem_lines(&worker_state, token, &lines(&["Blade Vortex of the Scythe", "Level: 1"]));
            let _ = tx.send(match read {
                GemRead::Committed(commit) => Some((commit.names, commit.added, commit.status.detected_gems)),
                _ => None,
            });
        });
        let got = rx.recv_timeout(Duration::from_secs(5)).expect("the commit deadlocked or panicked");

        let name = "Blade Vortex of the Scythe".to_string();
        assert_eq!(got, Some((vec![name.clone()], name.clone(), vec![name.clone()])));
        assert_eq!(held(&state), vec![name]);
    }

    #[test]
    fn a_repeat_name_is_not_committed_twice() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        let generation = with_vocab(&state);
        commit_gem_name(&state, token, generation, "Reave of Refraction").unwrap();

        assert!(commit_gem_name(&state, token, generation, "Reave of Refraction").is_none());
        assert_eq!(held(&state), vec!["Reave of Refraction".to_string()]);
    }

    #[test]
    fn the_third_gem_completes_the_choice_and_ends_the_context() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        *state.lab_state.lock().unwrap() = crate::lab_state::LabState::PickingGems;
        let generation = with_vocab(&state);
        commit_gem_name(&state, token, generation, "Blade Vortex of the Scythe").unwrap();
        let second = commit_gem_name(&state, token, generation, "Reave of Refraction").unwrap();

        let third = commit_gem_name(&state, token, generation, "Blade Flurry of Incision").unwrap();

        assert!(!second.complete);
        assert!(third.complete);
        assert_eq!(third.names.len(), 3);
        assert_eq!(state.lab_scan.gem_context.load(Ordering::SeqCst), 0);
        assert_eq!(*state.lab_state.lock().unwrap(), crate::lab_state::LabState::Idle);
        assert_eq!(third.status.state, "Idle");
    }

    #[test]
    fn a_press_captured_before_confirm_and_craft_writes_nothing_into_the_new_choice() {
        let state = test_app_state();
        let old = arm_gem_context(&state);
        let generation = with_vocab(&state);
        disarm_gem_context(&state, true);
        let new = arm_gem_context(&state);

        assert_ne!(new, old);
        assert!(commit_gem_name(&state, old, generation, "Reave of Refraction").is_none());
        assert!(held(&state).is_empty());
    }

    #[test]
    fn font_closed_keeps_the_held_gems_and_rejects_the_old_token() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        let generation = with_vocab(&state);
        commit_gem_name(&state, token, generation, "Reave of Refraction").unwrap();

        disarm_gem_context(&state, false);

        assert!(commit_gem_name(&state, token, generation, "Blade Flurry of Incision").is_none());
        assert_eq!(held(&state), vec!["Reave of Refraction".to_string()]);
    }

    #[test]
    fn a_commit_matched_under_the_vocabulary_before_a_mode_flip_is_rejected_after_it() {
        // r2-H1: `current` at g1, then the flip (bump + clear), then the g1 commit.
        let state = test_app_state();
        state.lab_settings.lock().unwrap().lab_mode = "Normal".to_string();
        let token = arm_gem_context(&state);
        with_vocab(&state);
        let g1 = gem_vocab::current(&state.lab_scan).expect("installed").generation;
        commit_gem_name(&state, token, g1, "Reave of Refraction").unwrap();

        assert!(crate::apply_manual_lab_mode(&state, "Dedication", |_| {}));
        let late = commit_gem_name(&state, token, g1, "Blade Flurry of Incision");

        assert!(late.is_none(), "a g1 commit after the flip must produce no publishable GemCommit");
        assert!(held(&state).is_empty(), "the flip clears and the stale commit must not refill");
    }
}
