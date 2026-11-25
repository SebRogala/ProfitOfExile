//! The Divine Font gem reader (POE-391, POE-397): a title-plus-lit-CONFIRM
//! match selects the pressed frame, preparing a context if needed. The three
//! slot icons are matched (`gem_icons`); when the cursor is on a slot,
//! the tooltip band of the same frame is OCR'd and its name overrides that
//! slot. A certain slot is held as its name; an uncertain one, read under
//! complete references, as its best candidate, flagged in the read prompt.

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
    fn id(&self) -> crate::read_hotkey::ReaderId { crate::read_hotkey::ReaderId::Gem }
    fn prepare(&self, app: &AppHandle, press: &Press) -> Option<Box<dyn PendingRead<AppHandle>>> {
        let state = app.state::<AppState>();
        if !*state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) { return None; }
        Some(Box::new(PendingGemRead {
            identity: super::commands::gem_read_identity(&state),
            generation: gem_vocab::preload_snapshot(&state).0,
            cursor: press.cursor,
            prompt: crate::read_prompt::armed_id_in(&state, crate::read_prompt::Mechanic::DivineFont),
            lab: None,
        }) as _)
    }
}

struct PendingGemRead {
    identity: super::commands::GemReadIdentity,
    generation: u64,
    cursor: Option<(i32, i32)>,
    prompt: Option<u64>,
    lab: Option<LabPlacements>,
}

impl PendingGemRead {
    fn probe_in(&mut self, frame: &Capture, state: &AppState) -> bool {
        if !*state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) { return false; }
        super::font_reader::seed_empty_screen(state, frame);
        let resolved = crate::ssot::regions::current(state, Some((frame.image.width(), frame.image.height())));
        if resolved.unread.contains(&crate::ssot::regions::RegionKey::LabGemSlots) { return false; }
        let lab = resolved.placements.lab;
        if super::panel_probe::probe(frame, &lab).is_none() { return false; }
        // An unread tooltip band suppresses hover OCR while preserving the
        // selected frame's icon read through consume and read_gem_frame_with_vocab.
        if resolved.unread.contains(&crate::ssot::regions::RegionKey::LabGem) { self.cursor = None; }
        self.lab = Some(lab);
        true
    }
}

impl PendingRead<AppHandle> for PendingGemRead {
    fn probe(&mut self, frame: &Arc<Capture>, app: &AppHandle) -> Result<bool, String> {
        Ok(super::font_reader::probe_on_reconciled_screen(app, frame, |state| self.probe_in(frame, state)))
    }
    fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, app: &AppHandle) {
        let Ok(frame) = frame else { return; };
        let Some(lab) = self.lab else { return; };
        let state = app.state::<AppState>();
        let ready = match tauri::async_runtime::block_on(ready_gem_frame(&state, self.identity, frame.clone(), gem_vocab::ensure_gem_vocab(app, self.generation))) {
            Ok(Some(ready)) => ready,
            Ok(None) => return,
            Err(error) => { app_log(app, format!("Gem read: {error}")); return; }
        };
        consume(app, ready.token, self.cursor, self.prompt, Ok(&ready.frame), &lab, &ready.vocab);
    }
}

struct ReadyGemFrame {
    frame: Arc<Capture>,
    token: u64,
    vocab: gem_vocab::GemVocab,
}

async fn ready_gem_frame(
    state: &AppState,
    identity: super::commands::GemReadIdentity,
    frame: Arc<Capture>,
    load: impl std::future::Future<Output = Result<gem_vocab::GemVocab, String>>,
) -> Result<Option<ReadyGemFrame>, String> {
    let vocab = load.await?;
    Ok(super::commands::prepare_gem_context(state, identity)
        .map(|token| ReadyGemFrame { frame, token, vocab }))
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

fn consume(app: &AppHandle, token: u64, cursor: Option<(i32, i32)>, prompt: Option<u64>, frame: Result<&Arc<Capture>, &str>, lab: &LabPlacements, vocab: &gem_vocab::GemVocab) {
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
    let read = read_gem_frame_with_vocab(&state, token, &grab.image, grab.origin, cursor, lab, vocab, tooltip_ocr);
    publish_gem_frame(app, token, read, prompt);
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
    /// The held names, left to right, each once: every certain name and,
    /// when `hold_best`, every uncertain slot's best candidate.
    pub(crate) fn held_names(&self, hold_best: bool) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for slot in &self.slots {
            let name = match slot {
                GemSlot::Certain { name, .. } => Some(name),
                GemSlot::Uncertain { candidates } if hold_best => candidates.first().map(|c| &c.name),
                _ => None,
            };
            if let Some(name) = name.filter(|name| !names.contains(name)) {
                names.push(name.clone());
            }
        }
        names
    }

    /// This read over the previous one of the same context: a slot this press
    /// did not read keeps its previous value, and a slot a tooltip resolved
    /// stays resolved until another tooltip reads it: the tooltip prints the
    /// name, while an icon match can take a tinted sibling variant.
    fn over(mut self, previous: Option<&LabGemRead>) -> Self {
        let Some(previous) = previous.filter(|p| p.token == self.token) else { return self };
        for (slot, before) in self.slots.iter_mut().zip(&previous.slots) {
            let keep = match (&*slot, before) {
                (GemSlot::Unread, _) => true,
                (GemSlot::Certain { source: SlotSource::Tooltip, .. }, _) => false,
                (_, GemSlot::Certain { source: SlotSource::Tooltip, .. }) => true,
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
        /// No icon evidence: the references were not loaded yet, or too
        /// few loaded to judge the frame.
        icons_skipped: bool,
        tooltip: TooltipOutcome,
    },
}

/// A committed read, as owned data built with no lab guard held.
#[derive(Debug, Clone)]
pub(crate) struct GemCommit {
    generation: u64,
    /// The held gems after the commit.
    pub names: Vec<String>,
    /// Whether the held list changed.
    pub replaced: bool,
    pub status: AppStatus,
    /// The read as stored in `last_gem_read`.
    pub read: LabGemRead,
    /// The read prompt's uncertain slots changed (the SSOT carries them).
    pub prompt_changed: bool,
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
#[cfg(test)]
pub(crate) fn read_gem_frame(
    state: &AppState,
    captured: u64,
    image: &DynamicImage,
    origin: (i32, i32),
    cursor: Option<(i32, i32)>,
    lab: &LabPlacements,
    tooltip_ocr: impl FnMut(&DynamicImage) -> Vec<String>,
) -> GemFrameRead {
    let Some(vocab) = gem_vocab::current(&state.lab_scan) else { return GemFrameRead::NoVocab };
    read_gem_frame_with_vocab(state, captured, image, origin, cursor, lab, &vocab, tooltip_ocr)
}

fn read_gem_frame_with_vocab(
    state: &AppState, captured: u64, image: &DynamicImage, origin: (i32, i32), cursor: Option<(i32, i32)>,
    lab: &LabPlacements, vocab: &gem_vocab::GemVocab, mut tooltip_ocr: impl FnMut(&DynamicImage) -> Vec<String>,
) -> GemFrameRead {
    if vocab.generation != state.lab_scan.gem_vocab_generation.load(Ordering::SeqCst) {
        return GemFrameRead::NoVocab;
    }
    let mut read = LabGemRead { token: captured, set: None, slots: [GemSlot::Unread, GemSlot::Unread, GemSlot::Unread] };
    let mut icons_skipped = vocab.refs.is_none();
    if let Some(refs) = &vocab.refs {
        let slots = lab.gem_slots.map(|rect| SlotImage::from_frame(image, rect));
        match gem_icons::classify_slots(refs, &slots) {
            IconRead::NoPanel { joint } => return GemFrameRead::NoPanel { joint },
            // No icon evidence either way: read as without references.
            IconRead::Unavailable { .. } => icons_skipped = true,
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
/// and make its held names the held gems, if the context `captured` is
/// still armed and the vocabulary generation `vocab_gen` is still current
/// (r2-H1). The status is built after the `detected_gems` guard is released
/// (`build_status` takes it).
pub(crate) fn commit_gem_read(state: &AppState, captured: u64, vocab_gen: u64, read: &LabGemRead) -> Option<GemCommit> {
    let lab = &state.lab_scan;
    // Incomplete references may lack the true gem, making the best candidate
    // its sibling, so only complete ones hold an uncertain slot. Read from the
    // installed vocabulary, not the press's: a slot classified just before a
    // complete reinstall is held (accepted trade-off).
    let hold_best = gem_vocab::current(lab)
        .filter(|vocab| vocab.generation == vocab_gen)
        .and_then(|vocab| vocab.refs)
        .is_some_and(|refs| !refs.incomplete);
    let (names, replaced, stored, prompt_changed) = {
        let mut gems = lab.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
        if captured == 0 || lab.gem_context.load(Ordering::SeqCst) != captured
            || lab.gem_vocab_generation.load(Ordering::SeqCst) != vocab_gen
            || !*state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner())
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
        let names = stored.held_names(hold_best);
        let replaced = *gems != names;
        if replaced {
            *gems = names.clone();
            lab.choice_seq.fetch_add(1, Ordering::SeqCst);
        }
        let uncertain = std::array::from_fn(|i| matches!(stored.slots[i], GemSlot::Uncertain { .. }));
        let prompt_changed = crate::read_prompt::gem_read_in(state, uncertain);
        (names, replaced, stored, prompt_changed)
    };
    let status = crate::build_status(state);
    Some(GemCommit { names, replaced, status, read: stored, generation: vocab_gen, prompt_changed })
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
    if !gem_publication_current(&app.state::<AppState>(), &commit) { return; }
    if commit.prompt_changed {
        crate::ssot::emit_ssot(app);
    }
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
    if let Err(e) = app.emit("status-changed", &commit.status) {
        log::warn!("emit status-changed failed: {}", e);
    }
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        deferred_gem_relay(&app_clone.state::<AppState>(), commit, |payload| {
            super::commands::send_gem_payload(&app_clone, payload)
        }).await;
    });
}

async fn deferred_gem_relay<F: std::future::Future<Output = ()>>(
    state: &AppState, commit: GemCommit, send: impl FnOnce(serde_json::Value) -> F,
) {
    let Some(payload) = super::commands::selected_gem_publication_payload(
        state, commit.read.token, commit.generation, &commit.names,
    ) else { return; };
    send(payload).await;
}

fn gem_publication_current(state: &AppState, commit: &GemCommit) -> bool {
    let _gems = state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner());
    state.lab_scan.gem_context.load(Ordering::SeqCst) == commit.read.token
        && state.lab_scan.gem_vocab_generation.load(Ordering::SeqCst) == commit.generation
        && *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner())
}

fn publish_gem_frame(app: &AppHandle, token: u64, read: GemFrameRead, prompt: Option<u64>) {
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
        app_log(app, "Gem read: icon match skipped, the gem icons are not loaded (or too many failed to load); hover a gem and press".to_string());
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
        Some(commit) => {
            crate::read_prompt::read_done(app, crate::read_prompt::Mechanic::DivineFont, prompt);
            publish_gem_commit(app, commit);
        }
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

    type LoadedRefs = Result<Arc<gem_icons::GemIconRefs>, String>;
    struct DeferredWorld {
        state: AppState,
        gate: std::sync::Mutex<Option<tokio::sync::oneshot::Receiver<LoadedRefs>>>,
        started: std::sync::mpsc::Sender<()>,
        result: std::sync::Mutex<Option<Result<Option<serde_json::Value>, String>>>,
        captures: std::sync::atomic::AtomicUsize,
        loads: std::sync::atomic::AtomicUsize,
        negative: bool,
    }
    struct DeferredReader;
    struct DeferredRead { identity: super::super::commands::GemReadIdentity }
    impl ScreenReader<Arc<DeferredWorld>> for DeferredReader {
        fn id(&self) -> crate::read_hotkey::ReaderId { crate::read_hotkey::ReaderId::Gem }
        fn prepare(&self, world: &Arc<DeferredWorld>, _: &Press) -> Option<Box<dyn PendingRead<Arc<DeferredWorld>>>> {
            Some(Box::new(DeferredRead { identity: super::super::commands::gem_read_identity(&world.state) }))
        }
    }
    impl PendingRead<Arc<DeferredWorld>> for DeferredRead {
        fn probe(&mut self, frame: &Arc<Capture>, _: &Arc<DeferredWorld>) -> Result<bool, String> {
            Ok(super::super::panel_probe::probe(frame, &lab_1080p()).is_some())
        }
        fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, world: &Arc<DeferredWorld>) {
            let frame = frame.unwrap();
            let gate = world.gate.lock().unwrap().take().unwrap();
            let generation = gem_vocab::preload_snapshot(&world.state).0;
            let load = gem_vocab::await_gem_vocab(&world.state.lab_scan, generation, || async {
                world.loads.fetch_add(1, Ordering::SeqCst);
                if gem_vocab::current(&world.state.lab_scan).is_none() { with_vocab(&world.state); }
                world.started.send(()).unwrap();
                let refs = gate.await.map_err(|_| "loader disconnected".to_string())??;
                if !gem_vocab::install_gem_refs(&world.state.lab_scan, generation, refs) { return Err("mode changed".into()); }
                Ok(gem_vocab::current(&world.state.lab_scan).unwrap())
            });
            let ready = tauri::async_runtime::block_on(ready_gem_frame(&world.state, self.identity, frame.clone(), load));
            let result = ready.map(|ready| ready.and_then(|ready| {
                assert!(Arc::ptr_eq(frame, &ready.frame), "the selected read retains the captured Arc");
                let read = read_gem_frame_with_vocab(&world.state, ready.token, &ready.frame.image, ready.frame.origin,
                    None, &lab_1080p(), &ready.vocab, |_| panic!("no hovered slot"));
                committed(read).map(|commit| super::super::commands::gem_publication_payload(&world.state, &commit.names))
            }));
            *world.result.lock().unwrap() = Some(result);
        }
    }

    fn deferred_world(matcher_ready: bool, negative: bool) -> (Arc<DeferredWorld>, std::sync::mpsc::Receiver<()>, tokio::sync::oneshot::Sender<LoadedRefs>) {
        let state = test_app_state();
        *state.pair_code.lock().unwrap() = "fixture-pair".into();
        if matcher_ready { with_vocab(&state); }
        let (started, waiting) = std::sync::mpsc::channel();
        let (release, gate) = tokio::sync::oneshot::channel();
        (Arc::new(DeferredWorld { state, gate: std::sync::Mutex::new(Some(gate)), started,
            result: std::sync::Mutex::new(None), captures: 0.into(), loads: 0.into(), negative }), waiting, release)
    }

    fn dispatch_deferred(world: Arc<DeferredWorld>) -> std::thread::JoinHandle<crate::read_hotkey::DispatchOutcome> {
        std::thread::spawn(move || {
            let press = crate::read_hotkey::prepare_press(Press { cursor: None }, Duration::ZERO, &[&DeferredReader], &world);
            crate::read_hotkey::dispatch_press(press, &world, &mut |_| {}, &mut || {
                world.captures.fetch_add(1, Ordering::SeqCst);
                Ok(Capture { image: if world.negative { blank() } else { fixture_frame() }, monitor_id:41,
                    origin:(0,0), client:[0,0,1920,1080], timing:Default::default() })
            }, &mut |_| panic!("no dismissal"))
        })
    }

    fn deferred_success(matcher_ready: bool) {
        let (world, waiting, release) = deferred_world(matcher_ready, false);
        let dispatch = dispatch_deferred(world.clone());
        waiting.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(world.result.lock().unwrap().is_none(), "matcher alone cannot finish the read");
        assert_eq!(world.captures.load(Ordering::SeqCst), 1);
        release.send(Ok(fixture_refs().clone())).ok().unwrap();
        assert_eq!(dispatch.join().unwrap(), crate::read_hotkey::DispatchOutcome::Selected(crate::read_hotkey::ReaderId::Gem));
        assert_eq!(*world.result.lock().unwrap(), Some(Ok(Some(serde_json::json!({
            "pair":"fixture-pair", "gems":["Blade Vortex of the Scythe","Reave of Refraction","Blade Flurry of Incision"],
            "variant":"20/20", "mode":"normal"
        })))));
        assert_eq!(world.captures.load(Ordering::SeqCst), 1);
        assert_eq!(world.state.lab_scan.font_context.load(Ordering::SeqCst), 0);
        assert_eq!(world.state.lab_scan.font_idle_seq.load(Ordering::SeqCst), 0);
        assert!(world.state.lab_scan.detection.lock().unwrap().detected_type().is_none());
    }

    #[test]
    fn initially_unloaded_selected_frame_waits_and_reads_without_recapture() { deferred_success(false); }

    #[test]
    fn installed_matcher_waits_for_icon_stage_before_reading_saved_frame() { deferred_success(true); }

    #[test]
    fn failed_loader_does_not_recapture_or_commit_gems() {
        let (world, waiting, release) = deferred_world(false, false);
        let dispatch = dispatch_deferred(world.clone());
        waiting.recv_timeout(Duration::from_secs(10)).unwrap();
        release.send(Err("fixture loader failed".into())).ok().unwrap();
        dispatch.join().unwrap();
        assert_eq!(*world.result.lock().unwrap(), Some(Err("fixture loader failed".into())));
        assert_eq!(world.captures.load(Ordering::SeqCst), 1);
        assert_eq!(world.state.lab_scan.gem_context.load(Ordering::SeqCst), 0);
        assert!(held(&world.state).is_empty());
    }

    #[test]
    fn negative_probe_starts_no_loader_or_gem_context() {
        let (world, _waiting, _release) = deferred_world(false, true);
        assert_eq!(dispatch_deferred(world.clone()).join().unwrap(), crate::read_hotkey::DispatchOutcome::NoMatch);
        assert_eq!(world.captures.load(Ordering::SeqCst), 1);
        assert_eq!(world.loads.load(Ordering::SeqCst), 0);
        assert!(world.result.lock().unwrap().is_none());
        assert_eq!(world.state.lab_scan.gem_context.load(Ordering::SeqCst), 0);
        assert!(held(&world.state).is_empty());
    }

    #[test]
    fn lifecycle_change_while_loading_prevents_the_saved_frame_commit() {
        for change in ["close", "confirm", "mode", "disable"] {
            let (world, waiting, release) = deferred_world(false, false);
            let dispatch = dispatch_deferred(world.clone());
            waiting.recv_timeout(Duration::from_secs(10)).unwrap();
            match change {
                "close" => { disarm_gem_context(&world.state, false); }
                "confirm" => { disarm_gem_context(&world.state, true); }
                "mode" => { crate::apply_manual_lab_mode(&world.state, "Dedication", |_| {}); }
                _ => { super::super::commands::set_lab_overlays_enabled_in(&world.state, false); }
            }
            release.send(Ok(fixture_refs().clone())).ok().unwrap();
            dispatch.join().unwrap();
            let result = world.result.lock().unwrap();
            assert!(!matches!(result.as_ref(), Some(Ok(Some(_)))), "{change} committed a stale frame");
            assert_eq!(world.captures.load(Ordering::SeqCst), 1);
            assert!(held(&world.state).is_empty());
        }
    }

    #[test]
    fn unarmed_selected_frame_produces_the_paired_gem_payload_without_font_accounting() {
        let state = test_app_state();
        *state.pair_code.lock().unwrap() = "fixture-pair".into();
        let generation = with_refs(&state, fixture_refs().clone());
        let image = fixture_frame();
        let capture = Capture { image, monitor_id: 7, origin: (0,0), client: [0,0,1920,1080], timing: Default::default() };
        let identity = super::super::commands::gem_read_identity(&state);
        assert!(super::super::panel_probe::probe(&capture, &lab_1080p()).is_some());
        let token = super::super::commands::prepare_gem_context(&state, identity).unwrap();
        let vocab = gem_vocab::current(&state.lab_scan).unwrap();
        let commit = committed(read_gem_frame_with_vocab(&state, token, &capture.image, capture.origin, None, &lab_1080p(), &vocab, |_| Vec::new())).unwrap();
        assert!(commit.replaced);
        assert_eq!(held(&state), vec!["Blade Vortex of the Scythe", "Reave of Refraction", "Blade Flurry of Incision"]);
        assert_eq!(super::super::commands::gem_publication_payload(&state, &commit.names), serde_json::json!({
            "pair": "fixture-pair", "gems": ["Blade Vortex of the Scythe", "Reave of Refraction", "Blade Flurry of Incision"],
            "variant": "20/20", "mode": "normal"
        }));
        assert_eq!(state.lab_scan.font_context.load(Ordering::SeqCst), 0);
        assert_eq!(state.lab_scan.font_idle_seq.load(Ordering::SeqCst), 0);
        assert!(state.lab_scan.font_session.lock().unwrap().current_options.is_none());
        assert_eq!(state.lab_scan.gem_vocab_generation.load(Ordering::SeqCst), generation);
    }

    #[test]
    fn ordinary_commit_keeps_a_later_queued_screen_read_valid() {
        let state = test_app_state();
        with_vocab(&state);
        let first_identity = super::super::commands::gem_read_identity(&state);
        let queued_identity = super::super::commands::gem_read_identity(&state);
        let token = super::super::commands::prepare_gem_context(&state, first_identity).unwrap();
        let first = committed(press(&state, token, &blank(), centre_of(0), &["Reave of Ruin"])).unwrap();
        assert_eq!(first.names, vec!["Reave of Ruin"]);
        assert_eq!(super::super::commands::prepare_gem_context(&state, queued_identity), Some(token));
        let second = committed(press(&state, token, &blank(), centre_of(1), &["Blade Vortex of the Scythe"])).unwrap();
        assert!(second.replaced);
        assert_eq!(second.names, vec!["Reave of Ruin", "Blade Vortex of the Scythe"]);
        assert_eq!(super::super::commands::gem_publication_payload(&state, &second.names)["gems"],
            serde_json::json!(["Reave of Ruin", "Blade Vortex of the Scythe"]));
    }

    #[test]
    fn deferred_relay_suppresses_reads_invalidated_after_scheduling() {
        for change in ["close", "confirm", "zone", "disable", "mode", "rearm"] {
            let state = Arc::new(test_app_state());
            with_vocab(&state);
            let token = arm_gem_context(&state);
            let commit = committed(press(&state, token, &blank(), centre_of(0), &["Reave of Ruin"])).unwrap();
            assert!(gem_publication_current(&state, &commit));
            let (task, release, requests) = queue_relay(state.clone(), commit);
            match change {
                "close" => { disarm_gem_context(&state, false); }
                "confirm" | "zone" => { disarm_gem_context(&state, true); }
                "disable" => { super::super::commands::set_lab_overlays_enabled_in(&state, false); }
                "mode" => { crate::apply_manual_lab_mode(&state, "Dedication", |_| {}); }
                _ => { arm_gem_context(&state); }
            }
            release.send(()).unwrap();
            tauri::async_runtime::block_on(task).unwrap();
            assert_eq!(requests.try_iter().collect::<Vec<_>>(), Vec::<serde_json::Value>::new(),
                "{change} must suppress the already scheduled request");
        }
    }

    fn queue_relay(state: Arc<AppState>, commit: GemCommit) -> (
        tauri::async_runtime::JoinHandle<()>, tokio::sync::oneshot::Sender<()>,
        std::sync::mpsc::Receiver<serde_json::Value>,
    ) {
        let (release, gate) = tokio::sync::oneshot::channel();
        let (send, requests) = std::sync::mpsc::channel();
        let task = tauri::async_runtime::spawn(async move {
            gate.await.unwrap();
            deferred_gem_relay(&state, commit, |payload| {
                // The transport may acquire these owners: snapshot guards must be gone.
                assert!(state.lab_scan.detected_gems.try_lock().is_ok());
                assert!(state.lab_settings.try_lock().is_ok());
                assert!(state.lab_overlays_enabled.try_lock().is_ok());
                async move { send.send(payload).unwrap(); }
            }).await;
        });
        (task, release, requests)
    }

    #[test]
    fn deferred_relay_delivers_concrete_paired_payloads_for_subsequent_valid_reads() {
        for (mode, variant) in [("Normal", "1/20"), ("Dedication", "21/20")] {
            let state = Arc::new(test_app_state());
            crate::apply_manual_lab_mode(&state, mode, |_| {});
            {
                let mut lab = state.lab_settings.lock().unwrap();
                lab.normal_variant = "1/20".into();
                lab.dedication_variant = "21/20".into();
            }
            *state.pair_code.lock().unwrap() = "fixture-pair".into();
            with_vocab(&state);
            let token = arm_gem_context(&state);
            for (slot, name, expected) in [
                (0, "Reave of Ruin", vec!["Reave of Ruin"]),
                (1, "Blade Vortex of the Scythe", vec!["Reave of Ruin", "Blade Vortex of the Scythe"]),
            ] {
                let commit = committed(press(&state, token, &blank(), centre_of(slot), &[name])).unwrap();
                let (task, release, requests) = queue_relay(state.clone(), commit);
                assert!(requests.try_recv().is_err());
                release.send(()).unwrap();
                tauri::async_runtime::block_on(task).unwrap();
                assert_eq!(requests.try_iter().collect::<Vec<_>>(), vec![serde_json::json!({
                    "pair": "fixture-pair", "gems": expected, "variant": variant,
                    "mode": if mode == "Normal" { "normal" } else { "dedication" }
                })]);
            }
        }
    }

    #[test]
    fn closed_context_cannot_publish_a_previously_committed_result() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_vocab(&state);
        let commit = committed(press(&state, token, &blank(), centre_of(0), &["Reave of Ruin"])).unwrap();
        assert!(gem_publication_current(&state, &commit));
        disarm_gem_context(&state, false);
        assert!(!gem_publication_current(&state, &commit));
        assert_eq!(held(&state), vec!["Reave of Ruin"]);
    }

    #[test]
    fn mode_change_cannot_publish_a_previously_committed_result() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_vocab(&state);
        let commit = committed(press(&state, token, &blank(), centre_of(0), &["Reave of Ruin"])).unwrap();
        crate::apply_manual_lab_mode(&state, "Dedication", |_| {});
        assert!(!gem_publication_current(&state, &commit));
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

    mod keep_happy {
        use super::*;

        fn prompt_slots(state: &AppState) -> [bool; 3] {
            state.read_prompt.lock().unwrap().snapshot().gem_uncertain
        }

        #[test]
        fn an_uncertain_read_asks_the_read_prompt_for_another_press_on_its_slots() {
            let state = test_app_state();
            let token = arm_gem_context(&state);
            with_refs(&state, fixture_refs().clone());
            let (frame, _) = blended_frame("Dark Pact", "Dark Bargain");

            committed(press(&state, token, &frame, None, &[])).expect("stored");

            assert_eq!(prompt_slots(&state), [true, true, true]);
        }

        #[test]
        fn a_second_press_that_reads_every_slot_withdraws_the_request() {
            let state = test_app_state();
            let token = arm_gem_context(&state);
            with_refs(&state, fixture_refs().clone());
            let (blend, _) = blended_frame("Dark Pact", "Dark Bargain");
            committed(press(&state, token, &blend, None, &[])).expect("first press stored");

            let second = committed(press(&state, token, &fixture_frame(), None, &[])).expect("second press stored");

            assert_eq!(second.names, truth());
            assert_eq!(prompt_slots(&state), [false, false, false]);
        }

        #[test]
        fn closing_the_choice_withdraws_the_request() {
            let state = test_app_state();
            let token = arm_gem_context(&state);
            with_refs(&state, fixture_refs().clone());
            let (blend, _) = blended_frame("Dark Pact", "Dark Bargain");
            committed(press(&state, token, &blend, None, &[])).expect("stored");

            disarm_gem_context(&state, false);

            assert_eq!(prompt_slots(&state), [false, false, false]);
        }

        /// The reported slot row read under references that lack its slot-3
        /// gem (Detonate Dead of Chain Reaction), unhovered.
        fn missing_reference_press(state: &AppState) -> GemCommit {
            use crate::lab::gem_icon_refs::tests::fixture_refs_transfigured_without;
            use crate::lab::gem_icons::tests::{reported_frame, REPORTED_RECTS, REPORTED_TRUTH};
            let token = arm_gem_context(state);
            with_refs(state, Arc::new(fixture_refs_transfigured_without(REPORTED_TRUTH[2])));
            let lab = LabPlacements { gem_slots: REPORTED_RECTS, ..lab_1080p() };
            let frame = DynamicImage::ImageRgba8(reported_frame());
            committed(read_gem_frame(state, token, &frame, (0, 0), None, &lab, |_| Vec::new())).expect("stored")
        }

        const MISSING_REFERENCE_PICKS: [&str; 3] = ["Toxic Rain of Sporeburst", "Fire Trap of Blasting", "Detonate Dead of Scavenging"];

        #[test]
        fn a_slot_without_its_reference_is_held_as_its_best_candidate_flagged_uncertain() {
            let state = test_app_state();

            let commit = missing_reference_press(&state);

            assert_eq!(commit.names, MISSING_REFERENCE_PICKS, "{:?}", commit.read);
            assert_eq!(held(&state), MISSING_REFERENCE_PICKS);
            assert_eq!(prompt_slots(&state), [false, false, true]);
        }

        #[test]
        fn a_flagged_pick_is_sent_to_the_server_like_a_certain_pick() {
            let state = Arc::new(test_app_state());
            *state.pair_code.lock().unwrap() = "fixture-pair".into();
            let commit = missing_reference_press(&state);

            let (task, release, requests) = queue_relay(state.clone(), commit);
            release.send(()).unwrap();
            tauri::async_runtime::block_on(task).unwrap();

            let sent: Vec<serde_json::Value> = requests.try_iter().map(|payload| payload["gems"].clone()).collect();
            assert_eq!(sent, vec![serde_json::json!(MISSING_REFERENCE_PICKS)]);
        }
    }

    mod keep_critical {
        use super::*;

        /// Silent consequence without it: a later unhovered press replaces
        /// the name the tooltip printed with the icon's sibling guess, and
        /// the overlay prices the wrong variant.
        #[test]
        fn a_tooltip_resolved_slot_stays_resolved_when_a_later_icon_match_is_certain() {
            let state = test_app_state();
            let token = arm_gem_context(&state);
            with_refs(&state, fixture_refs().clone());
            let frame = fixture_frame();
            committed(press(&state, token, &frame, centre_of(1), &["Reave of Ruin"])).unwrap();

            let later = committed(press(&state, token, &frame, None, &[])).unwrap();

            assert_eq!(later.names, ["Blade Vortex of the Scythe", "Reave of Ruin", "Blade Flurry of Incision"]);
            assert!(matches!(&later.read.slots[1], GemSlot::Certain { source: SlotSource::Tooltip, .. }), "{:?}", later.read);

            with_refs(&state, Arc::new(fixture_refs_marked_incomplete()));
            let uncertain = committed(press(&state, token, &frame, None, &[])).unwrap();
            assert_eq!(uncertain.names, ["Reave of Ruin"]);
            assert_eq!(held(&state), ["Reave of Ruin"]);

            let next = committed(press(&state, token, &frame, centre_of(1), &["Reave of Refraction"])).unwrap();
            assert_eq!(next.names, ["Reave of Refraction"]);
            assert_eq!(held(&state), ["Reave of Refraction"]);
        }
    }

    #[test]
    fn an_uncertain_blend_is_held_as_one_of_its_blended_gems() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_refs(&state, fixture_refs().clone());
        let (frame, _) = blended_frame("Dark Pact", "Dark Bargain");

        let commit = committed(press(&state, token, &frame, None, &[])).expect("stored");

        let held = held(&state);
        assert!(held.len() == 1 && ["Dark Pact", "Dark Bargain"].contains(&held[0].as_str()), "{held:?}");
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

    /// The fixture population with `failed` names' art failing in transport.
    fn refs_with_failed(failed: &[&str]) -> Arc<gem_icons::GemIconRefs> {
        use crate::gem_art::ArtReply;
        use crate::lab::gem_icon_refs::{art_outcome, build_refs, tests::{gem_sets, ART_DIR}, GemDictionary};
        let sets = gem_sets();
        let dict = GemDictionary { normal: sets.normal, transfigured: sets.transfigured };
        let (refs, _) = build_refs(&dict, |name| {
            if failed.contains(&name) {
                return art_outcome(ArtReply::Unreachable("timed out".into()));
            }
            match std::fs::read(crate::gem_art::art_file(std::path::Path::new(ART_DIR), name)) {
                Ok(bytes) => art_outcome(ArtReply::Response(200, bytes)),
                Err(_) => art_outcome(ArtReply::Response(404, Vec::new())),
            }
        });
        Arc::new(refs)
    }

    #[test]
    fn with_every_reference_failed_a_hovered_tooltip_still_commits_only_its_name() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        let empty_failed = Arc::new(gem_icons::GemIconRefs { normal: Vec::new(), transfigured: Vec::new(), incomplete: true });
        with_refs(&state, empty_failed);

        let read = press(&state, token, &fixture_frame(), centre_of(1), &["Reave of Refraction", "Level: 1"]);

        assert_eq!(committed(read).map(|c| c.names), Some(vec!["Reave of Refraction".to_string()]));
    }

    #[test]
    fn with_every_reference_failed_an_unhovered_press_guesses_no_icon() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        let empty_failed = Arc::new(gem_icons::GemIconRefs { normal: Vec::new(), transfigured: Vec::new(), incomplete: true });
        with_refs(&state, empty_failed);

        let commit = committed(press(&state, token, &fixture_frame(), None, &[])).expect("stored");

        assert!(commit.names.is_empty());
        assert!(commit.read.slots.iter().all(|s| *s == GemSlot::Unread), "{:?}", commit.read);
    }

    #[test]
    fn with_the_matching_references_failed_a_hovered_tooltip_still_commits_only_its_name() {
        let state = test_app_state();
        let token = arm_gem_context(&state);
        with_refs(&state, refs_with_failed(&TRUTH));

        let read = press(&state, token, &fixture_frame(), centre_of(1), &["Reave of Refraction", "Level: 1"]);

        assert_eq!(committed(read).map(|c| c.names), Some(vec!["Reave of Refraction".to_string()]));
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

    mod manual_regions {
        use super::*;
        use crate::ssot::{screen_from_geometry, ManualRegions, OcrRegionsMode, OcrRegionsSetting, ScreenRef};

        const SET_ON_2560: ScreenRef = ScreenRef { width: 2560, height: 1440, origin: [0, 0], at_ms: 1_732_400_000_000 };
        /// The ultrawide setup's placed slot strip (AC-2).
        const STRIP: [i32; 4] = [826, 373, 246, 48];
        const MANUAL_SLOTS: [[i32; 4]; 3] = [[826, 373, 48, 48], [925, 373, 48, 48], [1024, 373, 48, 48]];

        fn manual_slots(lab_gem_slots: Option<[i32; 4]>) -> OcrRegionsSetting {
            OcrRegionsSetting {
                mode: OcrRegionsMode::Manual,
                manual: Some(ManualRegions {
                    set_on: SET_ON_2560,
                    edited: true,
                    lab_gem: None,
                    lab_font: None,
                    lab_gem_slots,
                    temple_panel: None,
                    merc_panel: None,
                }),
            }
        }

        /// A test `AppState` on the POE-472 whole-capture fallback slice (a
        /// 2560x1440 capture whose client is the whole monitor, ui 1.2).
        fn state_on_whole_capture(setting: OcrRegionsSetting) -> AppState {
            let state = test_app_state();
            *state.screen.lock().unwrap() = Some(screen_from_geometry(2560, 1440, 1, (0, 0), [0, 0, 2560, 1440], 0));
            *state.ocr_regions.lock().unwrap() = setting;
            state
        }

        fn pending(state: &AppState) -> PendingGemRead {
            PendingGemRead {
                identity: crate::lab::commands::gem_read_identity(state),
                generation: 0,
                cursor: None,
                prompt: None,
                lab: None,
            }
        }

        /// `panel_probe`'s real 1080p gem-choice fixture.
        fn fixture_1080p() -> DynamicImage {
            image::load_from_memory(include_bytes!("../../tests/fixtures/lab/font-gems-1080p.png")).unwrap()
        }

        fn capture_2560x1440(image: DynamicImage) -> Capture {
            Capture { image, monitor_id: 1, origin: (0, 0), client: [0, 0, 2560, 1440], timing: Default::default() }
        }

        /// The fixture at its own 1080p scale (slots [[506, 373, 48, 48], …])
        /// translated onto a black 2560x1440 capture so its slots sit at
        /// [`MANUAL_SLOTS`].
        fn capture_with_panel_at_manual_slots() -> Capture {
            let mut image = DynamicImage::new_rgb8(2560, 1440);
            image::imageops::overlay(&mut image, &fixture_1080p(), 826 - 506, 0);
            capture_2560x1440(image)
        }

        /// The fixture scaled to the whole 2560x1440 capture, so its slots sit
        /// where Auto places them at ui 1.2: [[675, 497, 64, 64], …].
        fn capture_with_panel_at_auto_slots() -> Capture {
            capture_2560x1440(fixture_1080p().resize_exact(2560, 1440, image::imageops::FilterType::Lanczos3))
        }

        /// Precondition for the tests whose frame holds the panel only at the
        /// Auto-at-1.2 slots: Auto places its slots there and its probe
        /// selects that frame, so a Manual `false` is the Manual geometry's.
        fn assert_auto_selects(frame: &Capture) {
            let state = state_on_whole_capture(OcrRegionsSetting::default());
            let mut auto = pending(&state);
            assert!(auto.probe_in(frame, &state), "control: the Auto probe selects the panel at its own slots");
            assert_eq!(auto.lab.map(|lab| lab.gem_slots[0]), Some([675, 497, 64, 64]), "control: Auto-at-1.2 slots");
        }

        mod keep_happy {
            use super::*;

            // keep:happy — AC-2: the ultrawide 2560x1080-on-1440 setup's
            // placed strip selects the gem choice drawn at its slots, and the
            // press carries those slots into its consume.
            #[test]
            fn a_manual_strip_on_the_whole_capture_slice_probes_the_panel_at_its_slots() {
                let state = state_on_whole_capture(manual_slots(Some(STRIP)));
                let mut read = pending(&state);

                let selected = read.probe_in(&capture_with_panel_at_manual_slots(), &state);

                assert!(selected, "panel_probe must find the fixture panel at the Manual slots");
                assert_eq!(read.lab.map(|lab| lab.gem_slots), Some(MANUAL_SLOTS));
            }
        }

        mod keep_critical {
            use super::*;

            /// Silent consequence: an unset or past-edge Manual strip keeps
            /// Auto's display-only slots in the placements; probing and
            /// icon-matching those slots selects whatever panel lies there and
            /// commits its gems as the player's choice with nothing to say so
            /// (AC-5: unset or unreadable regions are not read).
            #[test]
            fn an_unread_manual_strip_selects_no_frame_at_the_auto_fallback_slots() {
                let frame = capture_with_panel_at_auto_slots();
                assert_auto_selects(&frame);
                let cases = [
                    ("slot strip unset", None),
                    ("slot strip crossing x = 2560", Some([2400, 497, 328, 64])),
                ];

                for (case, strip) in cases {
                    let state = state_on_whole_capture(manual_slots(strip));
                    let mut read = pending(&state);

                    let selected = read.probe_in(&frame, &state);

                    assert_eq!((selected, read.lab), (false, None), "{case}: an unread strip selects no frame");
                }
            }

            /// Silent consequence: with the Manual tooltip band unset or past
            /// the capture edge, a hovered press would crop the band at a
            /// fallback or past-edge rect on every hover and override the
            /// slot's icon read with a silently wrong gem name.
            #[test]
            fn an_unread_manual_tooltip_band_selects_the_frame_without_its_hover() {
                let frame = capture_with_panel_at_manual_slots();
                let on_slot_0 = Some((850, 397));
                let with_band = |lab_gem: Option<[i32; 4]>| {
                    let mut setting = manual_slots(Some(STRIP));
                    if let Some(manual) = setting.manual.as_mut() { manual.lab_gem = lab_gem; }
                    setting
                };
                let control = state_on_whole_capture(with_band(Some([826, 0, 600, 53])));
                let mut read = PendingGemRead { cursor: on_slot_0, ..pending(&control) };
                assert!(read.probe_in(&frame, &control), "control: a readable band selects the frame");
                assert_eq!(read.cursor, on_slot_0, "control: a readable band keeps the hover");
                let cases = [
                    ("tooltip band unset", None),
                    ("tooltip band past x = 2560", Some([2600, 0, 400, 53])),
                ];

                for (case, band) in cases {
                    let state = state_on_whole_capture(with_band(band));
                    let mut read = PendingGemRead { cursor: on_slot_0, ..pending(&state) };

                    let selected = read.probe_in(&frame, &state);

                    assert_eq!(
                        (selected, read.lab.map(|lab| lab.gem_slots), read.cursor),
                        (true, Some(MANUAL_SLOTS), None),
                        "{case}: the icon read keeps the frame, the unread band drops the hover",
                    );
                }
            }
        }

        mod keep_regression {
            use super::*;

            /// POE-472(b), Operator-reported: on the whole-capture fallback
            /// slice the gem probe ran at the Auto-at-1.2 slots
            /// [[675, 497, 64, 64], …], off the 2560x1080 client's panel. With
            /// the Manual strip saved, a panel at those Auto slots is not the
            /// choice the player placed.
            #[test]
            fn a_manual_strip_does_not_probe_the_auto_at_1_2_slots() {
                let frame = capture_with_panel_at_auto_slots();
                assert_auto_selects(&frame);
                let state = state_on_whole_capture(manual_slots(Some(STRIP)));
                let mut read = pending(&state);

                let selected = read.probe_in(&frame, &state);

                assert_eq!((selected, read.lab), (false, None), "the Manual strip probes its own slots only");
            }
        }
    }
}
