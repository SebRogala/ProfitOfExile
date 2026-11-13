//! The Divine Font gem reader (POE-391, POE-397): per read key press while a
//! gem context is armed (CRAFT until CONFIRM, FontClosed or ZoneChanged), the
//! three slot icons are matched (`gem_icons`); when the cursor is on a slot,
//! the tooltip band of the same frame is OCR'd and its name overrides that
//! slot. Only certain slots reach the held gems and the server.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use image::DynamicImage;
use tauri::{AppHandle, Emitter, Manager};

use super::gem_icons::{self, Candidate, GemSet, IconRead, IconSlot, SlotImage};
use super::{gem_vocab, once_per_context, report_engine_once, LogOnce};
use crate::capture::Capture;
use crate::gem_matcher::{GemMatch, GemMatcher};
use crate::lab_scan::LabScanState;
use crate::read_hotkey::{PendingRead, Press, ScreenReader};
use crate::ssot::LabPlacements;
use crate::{app_log, AppState, AppStatus};

const MAX_REJECT_LOGS: usize = 12;

pub struct LabGemReader;

/// The token a press captures, or `None` when no gem context is armed.
pub(crate) fn armed_token(lab: &LabScanState) -> Option<u64> {
    Some(lab.gem_context.load(Ordering::SeqCst)).filter(|token| *token != 0)
}

impl ScreenReader<AppHandle> for LabGemReader {
    fn arm(&self, app: &AppHandle, press: &Press) -> Option<Box<dyn PendingRead<AppHandle>>> {
        armed_token(&app.state::<AppState>().lab_scan)
            .map(|token| Box::new(PendingGemRead { token, cursor: press.cursor }) as _)
    }
}

struct PendingGemRead {
    token: u64,
    cursor: Option<(i32, i32)>,
}

impl PendingRead<AppHandle> for PendingGemRead {
    fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, app: &AppHandle) {
        consume(app, self.token, self.cursor, frame);
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

fn consume(app: &AppHandle, token: u64, cursor: Option<(i32, i32)>, frame: Result<&Arc<Capture>, &str>) {
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
    let lab = crate::ssot::placements_for(screen.as_ref()).lab;
    let region = crate::capture_region(lab.gem);
    if screen.is_none() && once(LogOnce::Unscaled) {
        app_log(
            app,
            format!(
                "lab OCR regions unscaled: no screen measured yet — the gem region and slots assume 1080p ({}, {}) {}x{}",
                region.x, region.y, region.w, region.h,
            ),
        );
    }
    let tooltip_ocr = |band: &DynamicImage| -> Vec<String> {
        if let Some(line) = crate::crop_shortfall(&region, band.width(), band.height()) {
            if once(LogOnce::ShortCrop) {
                app_log(app, line);
            }
        }
        if state.gem_band_dump.load(Ordering::SeqCst) {
            crate::dump_gem_band(app, band, press);
        }
        // Two kernels on the one frame: Lanczos3 2× first, then bilinear when
        // it yields no candidates. Measured 2026-09-09 on band dumps: a
        // hovered "Split Arrow of Splitting" header read nothing through
        // Lanczos3 2× and cleanly through other kernels.
        let mut kernel = "lanczos";
        let mut lines = recognize(app, once, &crate::capture::preprocess_for_ocr(band));
        if crate::ocr::extract_gem_candidates(&lines).is_empty() {
            kernel = "bilinear";
            lines = recognize(app, once, &crate::capture::preprocess_for_ocr_fast(band));
        }
        let candidates = crate::ocr::extract_gem_candidates(&lines);
        if !candidates.is_empty() {
            app_log(app, format!("Gem OCR candidates ({}): {:?}", kernel, candidates));
        }
        lines
    };
    let read = read_gem_frame(&state, token, &grab.image, grab.origin, cursor, &lab, tooltip_ocr);
    publish_gem_frame(app, token, read);
}

fn recognize(app: &AppHandle, once: impl Fn(LogOnce) -> bool, image: &DynamicImage) -> Vec<String> {
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

/// Where a certain slot's name came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SlotSource {
    Icon,
    Tooltip,
}

/// One slot of a gem read.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum GemSlot {
    /// No icon references were loaded and the slot was not hovered.
    Unread,
    /// `margin` is the icon margin; a tooltip name has none.
    Certain { name: String, margin: Option<f32>, source: SlotSource },
    Uncertain { candidates: Vec<Candidate> },
}

/// One press's read of the three slots, as `last_gem_read` keeps it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LabGemRead {
    /// The gem context it was read under.
    pub token: u64,
    /// The icon set chosen, `None` without icon references.
    pub set: Option<GemSet>,
    pub slots: [GemSlot; 3],
}

impl LabGemRead {
    /// The certain names, left to right, each once.
    pub(crate) fn certain_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for slot in &self.slots {
            if let GemSlot::Certain { name, .. } = slot {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
    }

    /// This read over the previous one of the same context: a slot this press
    /// did not read keeps its previous value, and a slot a tooltip resolved
    /// stays resolved when this press's icon match is uncertain.
    fn over(mut self, previous: Option<&LabGemRead>) -> Self {
        let Some(previous) = previous.filter(|p| p.token == self.token) else { return self };
        for (slot, before) in self.slots.iter_mut().zip(&previous.slots) {
            let keep = match (&*slot, before) {
                (GemSlot::Unread, _) => true,
                (GemSlot::Uncertain { .. }, GemSlot::Certain { source: SlotSource::Tooltip, .. }) => true,
                _ => false,
            };
            if keep {
                *slot = before.clone();
            }
        }
        self.set = self.set.or(previous.set);
        self
    }
}

/// What the hovered slot's tooltip did.
#[derive(Debug)]
pub(crate) enum TooltipOutcome {
    /// The cursor was on no slot: no tooltip OCR ran.
    NotHovered,
    Matched { slot: usize, matched: GemMatch },
    /// No candidate matched; the rejections, for the capped log.
    NoMatch { slot: usize, rejects: Vec<String> },
}

/// What one press did.
pub(crate) enum GemFrameRead {
    /// No current vocabulary: skipped.
    NoVocab,
    /// The presence gate rejected the frame; nothing was written.
    NoPanel { joint: f32 },
    Read {
        /// `None` when the context or the lab mode changed since the press.
        commit: Option<GemCommit>,
        /// The icon references were not loaded yet.
        icons_skipped: bool,
        tooltip: TooltipOutcome,
    },
}

/// A committed read, as owned data built with no lab guard held.
#[derive(Debug, Clone)]
pub(crate) struct GemCommit {
    /// The held gems after the commit.
    pub names: Vec<String>,
    /// Whether the held list changed.
    pub replaced: bool,
    pub status: AppStatus,
    /// The read as stored in `last_gem_read`.
    pub read: LabGemRead,
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

fn icon_slot(slot: IconSlot) -> GemSlot {
    match slot {
        IconSlot::Certain { name, margin } => GemSlot::Certain { name, margin: Some(margin), source: SlotSource::Icon },
        IconSlot::Uncertain { candidates } => GemSlot::Uncertain { candidates },
    }
}

/// The slot whose rect holds `cursor` (screen px), mapped into capture px.
fn hovered_slot(lab: &LabPlacements, origin: (i32, i32), cursor: Option<(i32, i32)>) -> Option<usize> {
    let (x, y) = cursor.map(|(x, y)| (x - origin.0, y - origin.1))?;
    lab.gem_slots
        .iter()
        .position(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3])
}

/// The pure step of a press: match the icons, run `tooltip_ocr` on the gem
/// band only when the cursor is on a slot, and commit under the captured token
/// and the vocabulary's generation.
pub(crate) fn read_gem_frame(
    state: &AppState,
    captured: u64,
    image: &DynamicImage,
    origin: (i32, i32),
    cursor: Option<(i32, i32)>,
    lab: &LabPlacements,
    mut tooltip_ocr: impl FnMut(&DynamicImage) -> Vec<String>,
) -> GemFrameRead {
    let Some(vocab) = gem_vocab::current(&state.lab_scan) else { return GemFrameRead::NoVocab };
    let mut read = LabGemRead { token: captured, set: None, slots: [GemSlot::Unread, GemSlot::Unread, GemSlot::Unread] };
    let icons_skipped = vocab.refs.is_none();
    if let Some(refs) = &vocab.refs {
        let slots = lab.gem_slots.map(|rect| SlotImage::from_frame(image, rect));
        match gem_icons::classify_slots(refs, &slots) {
            IconRead::NoPanel { joint } => return GemFrameRead::NoPanel { joint },
            IconRead::Panel { set, slots, .. } => {
                read.set = Some(set);
                read.slots = slots.map(icon_slot);
            }
        }
    }
    let tooltip = match hovered_slot(lab, origin, cursor) {
        None => TooltipOutcome::NotHovered,
        Some(slot) => {
            let band = crate::capture_region(lab.gem);
            let band = image.crop_imm(band.x.max(0) as u32, band.y.max(0) as u32, band.w, band.h);
            let lines = tooltip_ocr(&band);
            let candidates = crate::ocr::extract_gem_candidates(&lines);
            match best_match(&vocab.matcher, &candidates) {
                (Some(matched), _) => {
                    read.slots[slot] = GemSlot::Certain { name: matched.name.clone(), margin: None, source: SlotSource::Tooltip };
                    TooltipOutcome::Matched { slot, matched }
                }
                (None, rejects) => TooltipOutcome::NoMatch { slot, rejects },
            }
        }
    };
    let commit = commit_gem_read(state, captured, vocab.generation, &read);
    GemFrameRead::Read { commit, icons_skipped, tooltip }
}

/// Store `read` (merged over the context's previous read) as `last_gem_read`
/// and make its certain names the held gems, if the context `captured` is
/// still armed and the vocabulary generation `vocab_gen` is still current
/// (r2-H1). The status is built after the `detected_gems` guard is released
/// (`build_status` takes it).
pub(crate) fn commit_gem_read(state: &AppState, captured: u64, vocab_gen: u64, read: &LabGemRead) -> Option<GemCommit> {
    let lab = &state.lab_scan;
    let (names, replaced, stored) = {
        let mut gems = lab.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
        if lab.gem_context.load(Ordering::SeqCst) != captured
            || lab.gem_vocab_generation.load(Ordering::SeqCst) != vocab_gen
        {
            return None;
        }
        #[cfg(test)]
        after_commit_check(state);
        let stored = {
            let mut last = lab.last_gem_read.lock().unwrap_or_else(|e| e.into_inner());
            let stored = LabGemRead { token: captured, ..read.clone() }.over(last.as_ref());
            *last = Some(stored.clone());
            stored
        };
        let names = stored.certain_names();
        let replaced = *gems != names;
        if replaced {
            *gems = names.clone();
            lab.choice_seq.fetch_add(1, Ordering::SeqCst);
        }
        (names, replaced, stored)
    };
    let status = crate::build_status(state);
    Some(GemCommit { names, replaced, status, read: stored })
}

/// Test seam: runs inside `commit_gem_read` after its checks passed, while the
/// `detected_gems` guard is held, for the state it was installed for. No
/// other arrangement pauses a commit between its generation check and its write.
#[cfg(test)]
type CommitHook = Box<dyn Fn() + Send>;
#[cfg(test)]
static AFTER_COMMIT_CHECK: std::sync::Mutex<Option<(usize, CommitHook)>> = std::sync::Mutex::new(None);
#[cfg(test)]
fn after_commit_check(state: &AppState) {
    let hook = AFTER_COMMIT_CHECK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((key, f)) = hook.as_ref() {
        if *key == state as *const AppState as usize {
            f();
        }
    }
}

fn describe(read: &LabGemRead) -> String {
    let slots: Vec<String> = read
        .slots
        .iter()
        .map(|slot| match slot {
            GemSlot::Unread => "unread".to_string(),
            GemSlot::Certain { name, margin: Some(m), .. } => format!("{} (icon, margin {:.2})", name, m),
            GemSlot::Certain { name, margin: None, .. } => format!("{} (tooltip)", name),
            GemSlot::Uncertain { candidates } => format!(
                "uncertain [{}]",
                candidates.iter().map(|c| format!("{} {:.1}", c.name, c.rmse)).collect::<Vec<_>>().join(", "),
            ),
        })
        .collect();
    format!("{:?} set: {}", read.set, slots.join(" | "))
}

/// The lock-free adapter: emit, send and log a commit.
pub(crate) fn publish_gem_commit(app: &AppHandle, commit: GemCommit) {
    app_log(
        app,
        format!("Gem read: {} — held {:?}{}", describe(&commit.read), commit.names, if commit.replaced { "" } else { " (unchanged)" }),
    );
    if !commit.replaced {
        return;
    }
    if let Err(e) = app.emit("gems-cleared", ()) {
        log::warn!("emit gems-cleared failed: {}", e);
    }
    for name in &commit.names {
        if let Err(e) = app.emit("gem-detected", name) {
            log::warn!("emit gem-detected failed: {}", e);
        }
    }
    if let Err(e) = app.emit("status-changed", commit.status) {
        log::warn!("emit status-changed failed: {}", e);
    }
    let names = commit.names;
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::send_gems_to_server(&app_clone, names).await;
    });
}

fn publish_gem_frame(app: &AppHandle, token: u64, read: GemFrameRead) {
    let (commit, icons_skipped, tooltip) = match read {
        GemFrameRead::NoVocab => return,
        GemFrameRead::NoPanel { joint } => {
            app_log(
                app,
                format!(
                    "Gem read: no gem panel (joint icon RMSE {:.1} > {:.1})",
                    joint,
                    gem_icons::GEM_PANEL_JOINT_RMSE_MAX,
                ),
            );
            return;
        }
        GemFrameRead::Read { commit, icons_skipped, tooltip, .. } => (commit, icons_skipped, tooltip),
    };
    if icons_skipped && once_per_context(&LOGGED, token, LogOnce::NoIcons) {
        app_log(app, "Gem read: icon match skipped, the gem icons have not loaded yet; hover a gem and press".to_string());
    }
    match tooltip {
        TooltipOutcome::NotHovered => {}
        TooltipOutcome::Matched { slot, matched } => app_log(
            app,
            format!("Gem tooltip: slot {} read {:?} as {}", slot + 1, matched.ocr_raw, matched.name),
        ),
        TooltipOutcome::NoMatch { slot, rejects } => with_context_log(token, |log| {
            if rejects.is_empty() {
                app_log(app, format!("Gem tooltip: slot {} hovered, no gem name read", slot + 1));
            }
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
    }
    match commit {
        Some(commit) => publish_gem_commit(app, commit),
        None => app_log(app, "Gem read: not stored — the gem choice or the lab mode changed since the press".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::commands::{arm_gem_context, disarm_gem_context};
    use crate::lab::gem_icon_refs::tests::{fixture_refs, fixture_refs_marked_incomplete};
    use crate::lab::gem_icons::tests::{blended_frame, fixture_frame, TRUTH};
    use crate::settings::tests::test_app_state;
    use std::cell::Cell;
    use std::time::Duration;

    fn held(state: &AppState) -> Vec<String> {
        state.lab_scan.detected_gems.lock().unwrap().clone()
    }

    fn last(state: &AppState) -> Option<LabGemRead> {
        state.lab_scan.last_gem_read.lock().unwrap().clone()
    }

    fn truth() -> Vec<String> {
        TRUTH.map(String::from).to_vec()
    }

    fn with_vocab(state: &AppState) -> u64 {
        let generation = state.lab_scan.gem_vocab_generation.load(Ordering::SeqCst);
        assert!(gem_vocab::install_gem_matcher(
            &state.lab_scan,
            generation,
            ["Fireball of Trarthus", "Blade Vortex of the Scythe", "Reave of Refraction", "Reave of Ruin", "Blade Flurry of Incision"]
                .map(String::from)
                .to_vec(),
        ));
        generation
    }

    fn with_refs(state: &AppState, refs: Arc<gem_icons::GemIconRefs>) -> u64 {
        let generation = with_vocab(state);
        assert!(gem_vocab::install_gem_refs(&state.lab_scan, generation, refs));
        generation
    }

    fn lab_1080p() -> LabPlacements {
        crate::ssot::placements_for(None).lab
    }

    fn centre_of(slot: usize) -> Option<(i32, i32)> {
        let r = lab_1080p().gem_slots[slot];
        Some((r[0] + r[2] / 2, r[1] + r[3] / 2))
    }

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|l| l.to_string()).collect()
    }

    fn press(state: &AppState, token: u64, frame: &DynamicImage, cursor: Option<(i32, i32)>, tooltip: &[&str]) -> GemFrameRead {
        read_gem_frame(state, token, frame, (0, 0), cursor, &lab_1080p(), |_| lines(tooltip))
    }

    fn committed(read: GemFrameRead) -> Option<GemCommit> {
        match read {
            GemFrameRead::Read { commit, .. } => commit,
            _ => None,
        }
    }

    fn blank() -> DynamicImage {
        DynamicImage::new_rgb8(1920, 1080)
    }

    #[test]
    fn one_press_on_the_real_frame_commits_the_three_icon_names_with_a_status_built_after_the_guard() {
        let state = Arc::new(test_app_state());
        let token = arm_gem_context(&state);
        with_refs(&state, fixture_refs().clone());

        let (tx, rx) = std::sync::mpsc::channel();
        let worker_state = state.clone();
        std::thread::spawn(move || {
            let commit = committed(press(&worker_state, token, &fixture_frame(), None, &[]));
            let _ = tx.send(commit.map(|c| (c.names, c.replaced, c.status.detected_gems)));
        });
        let got = rx.recv_timeout(Duration::from_secs(60)).expect("the commit deadlocked or panicked");

        assert_eq!(got, Some((truth(), true, truth())));
        assert_eq!(held(&state), truth());
    }

    #[test]
    fn a_hovered_slot_takes_the_tooltip_name_over_its_icon() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_refs(&state, fixture_refs().clone());

        let read = press(&state, token, &fixture_frame(), centre_of(1), &["Reave of Ruin", "Level: 1"]);

        let commit = committed(read).expect("committed");
        assert_eq!(commit.names, ["Blade Vortex of the Scythe", "Reave of Ruin", "Blade Flurry of Incision"]);
        assert!(matches!(&commit.read.slots[1], GemSlot::Certain { source: SlotSource::Tooltip, .. }));
    }

    #[test]
    fn a_press_with_the_cursor_on_no_slot_runs_no_tooltip_ocr() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_refs(&state, fixture_refs().clone());
        let calls = Cell::new(0);
        let off_slot = Some((960, 900));

        let read = read_gem_frame(&state, token, &fixture_frame(), (0, 0), off_slot, &lab_1080p(), |_| {
            calls.set(calls.get() + 1);
            lines(&["Reave of Ruin"])
        });

        assert_eq!(calls.get(), 0);
        assert_eq!(committed(read).map(|c| c.names), Some(truth()));
    }

    #[test]
    fn the_cursor_is_mapped_into_the_capture_by_its_origin() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_vocab(&state);
        let (x, y) = centre_of(1).unwrap();
        let origin = (1920, 0);

        let read = read_gem_frame(&state, token, &blank(), origin, Some((x + 1920, y)), &lab_1080p(), |_| lines(&["Reave of Ruin"]));

        assert_eq!(committed(read).map(|c| c.names), Some(vec!["Reave of Ruin".to_string()]));
    }

    #[test]
    fn a_second_identical_read_does_not_replace_the_held_gems() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_refs(&state, fixture_refs().clone());
        let frame = fixture_frame();
        committed(press(&state, token, &frame, None, &[])).expect("first commit");
        let seq = state.lab_scan.choice_seq.load(Ordering::SeqCst);

        let second = committed(press(&state, token, &frame, None, &[])).expect("second commit");

        assert!(!second.replaced);
        assert_eq!(held(&state), truth());
        assert_eq!(state.lab_scan.choice_seq.load(Ordering::SeqCst), seq);
    }

    #[test]
    fn a_read_captured_before_confirm_and_craft_writes_nothing_into_the_new_choice() {
        let state = test_app_state();
        let old = arm_gem_context(&state);
        let generation = with_vocab(&state);
        let read = LabGemRead {
            token: old,
            set: None,
            slots: [
                GemSlot::Certain { name: "Reave of Refraction".into(), margin: None, source: SlotSource::Tooltip },
                GemSlot::Unread,
                GemSlot::Unread,
            ],
        };
        disarm_gem_context(&state, true);
        let new = arm_gem_context(&state);

        assert_ne!(new, old);
        assert!(commit_gem_read(&state, old, generation, &read).is_none());
        assert!(held(&state).is_empty());
        assert_eq!(last(&state), None);
    }

    #[test]
    fn font_closed_keeps_the_held_gems_and_rejects_the_old_token() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_vocab(&state);
        committed(press(&state, token, &blank(), centre_of(0), &["Reave of Refraction"])).unwrap();

        disarm_gem_context(&state, false);

        assert!(committed(press(&state, token, &blank(), centre_of(1), &["Blade Flurry of Incision"])).is_none());
        assert_eq!(held(&state), vec!["Reave of Refraction".to_string()]);
    }

    #[test]
    fn incomplete_refs_commit_no_icon_name() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_refs(&state, Arc::new(fixture_refs_marked_incomplete()));

        let commit = committed(press(&state, token, &fixture_frame(), None, &[])).expect("stored");

        assert!(commit.names.is_empty());
        assert!(held(&state).is_empty());
        assert!(commit.read.slots.iter().all(|s| matches!(s, GemSlot::Uncertain { .. })), "{:?}", commit.read);
    }

    #[test]
    fn a_tooltip_resolved_slot_stays_resolved_when_a_later_icon_match_is_uncertain() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_refs(&state, Arc::new(fixture_refs_marked_incomplete()));
        let frame = fixture_frame();
        committed(press(&state, token, &frame, centre_of(1), &["Reave of Refraction"])).unwrap();

        let later = committed(press(&state, token, &frame, None, &[])).unwrap();

        assert_eq!(later.names, ["Reave of Refraction"]);
        assert_eq!(held(&state), ["Reave of Refraction"]);
    }

    #[test]
    fn an_uncertain_blend_keeps_its_candidates_out_of_the_held_gems() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_refs(&state, fixture_refs().clone());
        let (frame, _) = blended_frame("Dark Pact", "Dark Bargain");

        let commit = committed(press(&state, token, &frame, None, &[])).expect("stored");

        assert!(held(&state).is_empty());
        let Some(LabGemRead { slots, .. }) = last(&state) else { panic!("no last read") };
        let GemSlot::Uncertain { candidates } = &slots[0] else { panic!("{:?}", commit.read) };
        assert!(candidates.iter().any(|c| c.name == "Dark Bargain"), "{candidates:?}");
    }

    #[test]
    fn panel_chrome_is_no_gem_panel_and_writes_nothing() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_refs(&state, fixture_refs().clone());
        let chrome = LabPlacements { gem_slots: [530, 628, 728].map(|x| [x - 24, 300 - 24, 48, 48]), ..lab_1080p() };

        let read = read_gem_frame(&state, token, &fixture_frame(), (0, 0), None, &chrome, |_| Vec::new());

        assert!(matches!(read, GemFrameRead::NoPanel { .. }));
        assert!(held(&state).is_empty());
        assert_eq!(last(&state), None);
    }

    #[test]
    fn without_icon_refs_a_hovered_tooltip_still_commits_its_name() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_vocab(&state);

        let read = press(&state, token, &blank(), centre_of(1), &["Reave of Refraction", "Level: 1"]);

        let GemFrameRead::Read { commit, icons_skipped, .. } = read else { panic!("not read") };
        assert!(icons_skipped);
        assert_eq!(commit.map(|c| c.names), Some(vec!["Reave of Refraction".to_string()]));
    }

    #[test]
    fn without_icon_refs_tooltip_reads_of_two_slots_accumulate() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_vocab(&state);
        committed(press(&state, token, &blank(), centre_of(2), &["Blade Flurry of Incision"])).unwrap();

        committed(press(&state, token, &blank(), centre_of(0), &["Blade Vortex of the Scythe"])).unwrap();

        assert_eq!(held(&state), ["Blade Vortex of the Scythe", "Blade Flurry of Incision"]);
    }

    #[test]
    fn a_read_under_the_vocabulary_before_a_mode_flip_is_rejected_after_it() {
        // r2-H1: `current` at g1, then the flip (bump + clear), then the g1 commit.
        let state = test_app_state();
        state.lab_settings.lock().unwrap().lab_mode = "Normal".to_string();
        let token = arm_gem_context(&state);
        with_vocab(&state);
        let g1 = gem_vocab::current(&state.lab_scan).expect("installed").generation;
        committed(press(&state, token, &blank(), centre_of(0), &["Reave of Refraction"])).unwrap();
        let read = LabGemRead {
            token,
            set: None,
            slots: [
                GemSlot::Unread,
                GemSlot::Certain { name: "Blade Flurry of Incision".into(), margin: None, source: SlotSource::Tooltip },
                GemSlot::Unread,
            ],
        };

        assert!(crate::apply_manual_lab_mode(&state, "Dedication", |_| {}));
        let late = commit_gem_read(&state, token, g1, &read);

        assert!(late.is_none(), "a g1 commit after the flip must produce no publishable GemCommit");
        assert!(held(&state).is_empty(), "the flip clears and the stale commit must not refill");
        assert_eq!(last(&state), None, "the flip clears last_gem_read and the stale commit must not refill it");
    }

    fn tooltip_read(token: u64, name: &str) -> LabGemRead {
        LabGemRead {
            token,
            set: None,
            slots: [
                GemSlot::Certain { name: name.into(), margin: None, source: SlotSource::Tooltip },
                GemSlot::Unread,
                GemSlot::Unread,
            ],
        }
    }

    #[test]
    fn a_mode_flip_started_during_a_commit_waits_for_its_guard_before_bumping() {
        // A commit passes its checks and holds the gem guard; a mode flip
        // starts meanwhile. The flip's snapshot needs that guard, so it cannot
        // bump until the commit is done; the context is still armed, so the
        // flip then clears what the commit wrote.
        let state = Arc::new(test_app_state());
        state.lab_settings.lock().unwrap().lab_mode = "Normal".to_string();
        let token = arm_gem_context(&state);
        let g1 = with_vocab(&state);

        let (checked_tx, checked_rx) = std::sync::mpsc::channel::<()>();
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let (checked_tx, started_rx) = (std::sync::Mutex::new(checked_tx), std::sync::Mutex::new(started_rx));
        let bumped_during_commit = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (watched, bumped) = (state.clone(), bumped_during_commit.clone());
        *AFTER_COMMIT_CHECK.lock().unwrap() = Some((
            Arc::as_ptr(&state) as usize,
            Box::new(move || {
                checked_tx.lock().unwrap().send(()).unwrap();
                started_rx.lock().unwrap().recv_timeout(Duration::from_secs(5)).expect("the flip never started");
                // Give the flip time to run as far as it can while the guard is held.
                std::thread::sleep(Duration::from_millis(200));
                bumped.store(watched.lab_scan.gem_vocab_generation.load(Ordering::SeqCst) != g1, Ordering::SeqCst);
            }),
        ));
        let committer = {
            let state = state.clone();
            std::thread::spawn(move || commit_gem_read(&state, token, g1, &tooltip_read(token, "Blade Flurry of Incision")).is_some())
        };
        checked_rx.recv_timeout(Duration::from_secs(5)).expect("the commit never passed its checks");
        let flipper = {
            let state = state.clone();
            std::thread::spawn(move || {
                started_tx.send(()).unwrap();
                crate::apply_manual_lab_mode(&state, "Dedication", |_| {})
            })
        };
        let committed = committer.join().unwrap();
        let flipped = flipper.join().unwrap();
        *AFTER_COMMIT_CHECK.lock().unwrap() = None;

        assert_eq!((committed, flipped), (true, true));
        assert!(!bumped_during_commit.load(Ordering::SeqCst), "the flip bumped while a commit held the gem guard");
        assert!(held(&state).is_empty());
        assert_eq!(last(&state), None);
    }

    #[test]
    fn a_choice_armed_after_the_mode_snapshot_and_committed_across_the_bump_leaves_no_stale_gems() {
        // The mode change snapshots an unarmed choice and pauses; CRAFT arms,
        // a g1 commit passes its g1 check; the change bumps to g2 and waits
        // for the gem guard; the commit writes before the change's clear runs.
        use crate::lab::commands::AFTER_MODE_SNAPSHOT;
        let state = Arc::new(test_app_state());
        state.lab_settings.lock().unwrap().lab_mode = "Normal".to_string();
        let g1 = with_vocab(&state);
        let key = Arc::as_ptr(&state) as usize;

        let (paused_tx, paused_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (paused_tx, release_rx) = (std::sync::Mutex::new(paused_tx), std::sync::Mutex::new(release_rx));
        *AFTER_MODE_SNAPSHOT.lock().unwrap() = Some((
            key,
            Box::new(move || {
                paused_tx.lock().unwrap().send(()).unwrap();
                release_rx.lock().unwrap().recv_timeout(Duration::from_secs(5)).expect("never released");
            }),
        ));
        let flipper = {
            let state = state.clone();
            std::thread::spawn(move || crate::apply_manual_lab_mode(&state, "Dedication", |_| {}))
        };
        paused_rx.recv_timeout(Duration::from_secs(5)).expect("the mode change never reached its snapshot");

        let token = arm_gem_context(&state);
        let (checked_tx, checked_rx) = std::sync::mpsc::channel::<()>();
        let checked_tx = std::sync::Mutex::new(checked_tx);
        let watched = state.clone();
        *AFTER_COMMIT_CHECK.lock().unwrap() = Some((
            key,
            Box::new(move || {
                checked_tx.lock().unwrap().send(()).unwrap();
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                while watched.lab_scan.gem_vocab_generation.load(Ordering::SeqCst) == g1 {
                    assert!(std::time::Instant::now() < deadline, "the mode flip never bumped");
                    std::thread::yield_now();
                }
            }),
        ));
        let committer = {
            let state = state.clone();
            std::thread::spawn(move || commit_gem_read(&state, token, g1, &tooltip_read(token, "Reave of Refraction")).is_some())
        };
        checked_rx.recv_timeout(Duration::from_secs(5)).expect("the commit never passed its checks");
        release_tx.send(()).unwrap();
        let committed = committer.join().unwrap();
        let flipped = flipper.join().unwrap();
        *AFTER_COMMIT_CHECK.lock().unwrap() = None;
        *AFTER_MODE_SNAPSHOT.lock().unwrap() = None;

        assert_eq!((committed, flipped), (true, true), "the arrangement must reproduce the race");
        assert!(held(&state).is_empty(), "g1 gems of a choice armed across the flip must not survive it");
        assert_eq!(last(&state), None);
    }
}
