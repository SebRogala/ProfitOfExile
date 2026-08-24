//! The merc capture loop (POE-165 D5, D6) — the module's Windows glue.
//!
//! `modules.rs::spawn_mercenary` delegates here. The loop is a
//! [`ModuleJoin::Thread`](crate::modules::ModuleJoin::Thread) because screen
//! capture and `Windows.Media.Ocr` are apartment-threaded: the async runtime
//! and `spawn_blocking` both deadlock on them (see `spawn_gem_scan` in lib.rs).
//! Threads cannot be aborted, so every wait in here goes through [`nap`], which
//! polls `*cancel.borrow()` every 100 ms — two orders under the registry's 5 s
//! ceiling.
//!
//! # What is pure and what is not
//!
//! The cadence, the retirement rule, the log deduplicator, the hover rect and
//! the cursor hit-test are plain functions over plain data, tested here on
//! Linux. Everything platform-specific arrives through three calls that return
//! `Err` off Windows — `capture::capture_screen`, `ocr::recognize_lines`,
//! `crate::capture_mouse_position` — so the loop body itself carries no `cfg`
//! and compiles identically on both hosts.
//!
//! # Read-only, always
//!
//! Hover-confirm READS the cursor position; it never moves it and never sends
//! input. Injecting input into the PoE client is against GGG's ToS, and this
//! module is the one place in the app that would be tempted.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tokio::sync::watch;

use crate::modules::ModuleJoin;
use crate::AppState;

use super::geometry::{self, OcrLineBox};
use super::icons::{CellSig, TemplateStore};
use super::read::{build_capture, pass2_texts};
use super::vocab::{classify_resolution, MercVocab, SupportTitleRead};
use super::{
    MercCapture, MercGeometry, MercSkillRead, MercStatus, MercSupportRead, MercenarySlice,
    ReadState,
};

/// Loop quantum. Every wait is built out of these so a stop signal is honoured
/// within one of them, whatever the cadence above it says.
const TICK: Duration = Duration::from_millis(100);
/// Detect cadence while no window is captured (D6).
const DETECT_INTERVAL: Duration = Duration::from_millis(1000);
/// Detect cadence after the backoff has fired.
const DETECT_INTERVAL_SLOW: Duration = Duration::from_millis(3000);
/// Re-detect cadence while a window IS captured.
const REDETECT_INTERVAL: Duration = Duration::from_millis(2000);
/// Hover-confirm cadence while a window is captured.
const HOVER_INTERVAL: Duration = Duration::from_millis(400);
/// A capture tick (detect, plus the hover confirm that follows it in the same
/// iteration) slower than this backs the detect cadence off.
const SLOW_TICK: Duration = Duration::from_millis(1500);
/// How long to idle between focus checks while the game is not focused.
const UNFOCUSED_NAP: Duration = Duration::from_millis(1000);
/// Consecutive failed detections that retire a live capture (D6).
const RETIRE_AFTER: u8 = 2;
/// Distinct error messages logged before the loop starts suppressing them.
const MAX_DISTINCT_ERRORS: usize = 12;

/// Spawn the capture loop. Called through `MODULES` — see `modules.rs`.
pub fn spawn(app: AppHandle, cancel: watch::Receiver<bool>) -> ModuleJoin {
    ModuleJoin::Thread(std::thread::spawn(move || run_loop(app, cancel)))
}

// ---------------------------------------------------------------------------
// Pure pieces
// ---------------------------------------------------------------------------

/// What a detect tick did to the loop's capture state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectOutcome {
    /// A window was found where there was none — log it, publish `live`.
    Captured,
    /// A window that was already live was re-read.
    Refreshed,
    /// Nothing found, and nothing was live (or not enough misses yet).
    Missed,
    /// The live capture just retired after [`RETIRE_AFTER`] misses.
    Retired,
}

/// The loop's capture state machine: what cadence to run at, and when a live
/// capture has been missing long enough to retire.
///
/// Separated from the loop so the two rules that decide whether the page shows
/// a stale window — retire after two misses, back off after a slow tick — are
/// testable without a screen, an OCR engine or a clock.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoopState {
    /// A capture is on screen.
    pub live: bool,
    /// Consecutive failed detections since the last successful one.
    pub misses: u8,
    /// The slow-tick backoff has fired.
    ///
    /// Sticky for the life of the thread: it means "this machine takes over
    /// 1.5 s to OCR a screen", which does not become false again. Flapping
    /// between cadences would also flap the log line that announces it.
    pub backed_off: bool,
}

impl LoopState {
    /// How long to wait before the next detect tick.
    ///
    /// A live capture re-detects on its own cadence (2 s) and is NOT subject to
    /// the backoff: the backoff exists to stop a slow machine spending all its
    /// time hunting for a window, and a live window has already been found.
    pub fn detect_interval(&self) -> Duration {
        if self.live {
            REDETECT_INTERVAL
        } else if self.backed_off {
            DETECT_INTERVAL_SLOW
        } else {
            DETECT_INTERVAL
        }
    }

    /// Fold one detect result into the state.
    pub fn on_detect(&mut self, found: bool) -> DetectOutcome {
        if found {
            self.misses = 0;
            if self.live {
                DetectOutcome::Refreshed
            } else {
                self.live = true;
                DetectOutcome::Captured
            }
        } else if !self.live {
            DetectOutcome::Missed
        } else {
            self.misses += 1;
            if self.misses >= RETIRE_AFTER {
                self.live = false;
                self.misses = 0;
                DetectOutcome::Retired
            } else {
                DetectOutcome::Missed
            }
        }
    }

    /// Record how long a whole capture tick took — the detect AND the hover
    /// confirm that ran after it, because both are screen grabs plus OCR and
    /// both are what "this machine is too slow to hunt at 1 Hz" is measuring.
    /// `true` the one time the backoff fires, so the caller logs it once.
    pub fn note_tick_duration(&mut self, took: Duration) -> bool {
        if took > SLOW_TICK && !self.backed_off {
            self.backed_off = true;
            true
        } else {
            false
        }
    }
}

/// A log sink that says each distinct thing once.
///
/// The loop re-runs its whole failure path every second, so an unguarded error
/// line would fill the 50-entry LOGS buffer with one repeated message and push
/// every other diagnostic out of it. The cap bounds the other failure mode: an
/// error message carrying a varying number is a different string every time.
#[derive(Debug, Default)]
pub struct OnceLog {
    seen: HashSet<String>,
    suppressed: bool,
}

impl OnceLog {
    /// The line to log for `msg`, or `None` when it has been said already.
    ///
    /// Past the cap, the FIRST rejected message returns the suppression notice
    /// (so the log says why it went quiet) and every later one returns `None`.
    pub fn admit(&mut self, msg: &str) -> Option<String> {
        if self.seen.contains(msg) {
            return None;
        }
        if self.seen.len() >= MAX_DISTINCT_ERRORS {
            if self.suppressed {
                return None;
            }
            self.suppressed = true;
            return Some(format!(
                "Merc: {MAX_DISTINCT_ERRORS} distinct errors logged — further errors suppressed"
            ));
        }
        self.seen.insert(msg.to_string());
        Some(msg.to_string())
    }
}

/// The screen region a hover-confirm OCRs, clamped to the screen (D5).
///
/// The tooltip is placed by the game, not by us, and where it lands relative to
/// the cursor is unknown until the first Windows run — hence a generous box
/// mostly ABOVE the cursor (`hover_up` 500 vs `hover_down` 120), scaled with
/// the panel so a 4K client gets a proportionally bigger one. All three numbers
/// are `Thresholds` fields precisely because this is the guess most likely to
/// be wrong.
///
/// `None` when the clamped box is empty — a cursor off the captured screen.
pub fn hover_region(
    cursor: (i32, i32),
    scale: f32,
    t: &super::Thresholds,
    screen: [u32; 2],
) -> Option<[i32; 4]> {
    let half = (t.hover_w as f32 * scale / 2.0).round() as i32;
    let up = (t.hover_up as f32 * scale).round() as i32;
    let down = (t.hover_down as f32 * scale).round() as i32;
    let x0 = (cursor.0 - half).max(0);
    let y0 = (cursor.1 - up).max(0);
    let x1 = (cursor.0 + half).min(screen[0] as i32);
    let y1 = (cursor.1 + down).min(screen[1] as i32);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some([x0, y0, x1 - x0, y1 - y0])
}

/// Which captured cell the cursor is inside, as `(row index, support index)`.
///
/// Indices into the capture's own vectors, not `(row.index, slot)` — the caller
/// mutates the read it finds, and a slot number is not a position in a vector
/// whose earlier slots may have been skipped.
pub fn cell_at(capture: &MercCapture, cursor: (i32, i32)) -> Option<(usize, usize)> {
    for (ri, row) in capture.rows.iter().enumerate() {
        for (si, cell) in row.supports.iter().enumerate() {
            let [x, y, w, h] = cell.rect;
            if cursor.0 >= x && cursor.0 < x + w && cursor.1 >= y && cursor.1 < y + h {
                return Some((ri, si));
            }
        }
    }
    None
}

/// What a hover-confirm established about one cell.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmedCell {
    pub family: String,
    pub tier: u8,
    pub ids: Vec<String>,
    pub name: Option<String>,
    pub score: f32,
}

/// The key a confirmation is remembered under: the row's skill, plus the slot.
///
/// D5: confirmations survive re-detection of the SAME window. The row index is
/// not stable enough for that on its own — a wrapped name or a missed line
/// renumbers the rows — so the row's identity is its skill id, falling back to
/// its raw text when the skill did not resolve.
pub fn row_key(skill: &MercSkillRead) -> String {
    match skill.ids.first() {
        Some(id) => id.clone(),
        None => skill.raw.trim().to_lowercase(),
    }
}

/// Re-apply remembered confirmations to a freshly read capture.
///
/// A confirmed cell outranks whatever the template store said this tick: the
/// user told us what it is. The score comes from the tooltip read, not from the
/// icon correlation, so the page's tooltip does not claim an icon match that
/// never happened.
pub fn apply_confirmed(
    capture: &mut MercCapture,
    confirmed: &HashMap<(String, u8), ConfirmedCell>,
) {
    for row in &mut capture.rows {
        let key = row_key(&row.skill);
        for cell in &mut row.supports {
            let Some(c) = confirmed.get(&(key.clone(), cell.slot)) else {
                continue;
            };
            cell.family = Some(c.family.clone());
            cell.tier = Some(c.tier);
            cell.ids = c.ids.clone();
            cell.name = c.name.clone();
            cell.score = c.score;
            cell.state = ReadState::Confirmed;
            cell.candidates.clear();
        }
    }
}

/// The pre-hover crop cache: one signature (and the colour crop it came from)
/// per `(row index, slot)`.
pub type SigCache = HashMap<(u8, u8), (CellSig, Option<image::RgbaImage>)>;

/// Fold a fresh detect's crops into the cached ones, protecting the cell the
/// cursor is inside.
///
/// D5's pre-hover rule is not satisfied by "crop at detect time" alone: the
/// loop re-detects every 2 s WHILE the user hovers, so the second detect's crop
/// of the hovered cell is exactly the highlighted art the rule exists to avoid.
/// The cell under the cursor therefore keeps whatever cold crop it already had,
/// and gets NO entry when it has none — a confirm then reports `NoCrop` and
/// learns nothing, which is the honest outcome. Every other cell takes the
/// fresh crop, so a moved or rescaled window re-caches normally.
pub fn merge_sigs(mut previous: SigCache, fresh: SigCache, hovered: Option<(u8, u8)>) -> SigCache {
    let mut out = SigCache::with_capacity(fresh.len());
    for (key, sig) in fresh {
        if Some(key) == hovered {
            if let Some(cold) = previous.remove(&key) {
                out.insert(key, cold);
            }
            continue;
        }
        out.insert(key, sig);
    }
    out
}

/// The `(row index, slot)` of the cell the cursor is inside, if any.
pub fn hovered_key(capture: &MercCapture, cursor: Option<(i32, i32)>) -> Option<(u8, u8)> {
    let (ri, si) = cell_at(capture, cursor?)?;
    Some((capture.rows[ri].index, capture.rows[ri].supports[si].slot))
}

/// Whether the template store changed since `seen`, recording the new value.
///
/// `merc_forget_template` / `merc_reset_templates` are the un-poison path for a
/// mistimed hover — but a forgotten template is still remembered in the loop's
/// `confirmed` map, which re-applies it to every later capture. The generation
/// counter is how the loop learns to drop those remembered confirmations; a
/// plain "reload the store" would not, because the confirmations do not live in
/// the store.
pub fn generation_changed(seen: &mut u64, current: u64) -> bool {
    if *seen == current {
        return false;
    }
    *seen = current;
    true
}

/// One line of a hover-tooltip read, with how far it fell from the cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct TooltipLine {
    pub text: String,
    /// Squared px distance from the cursor to the nearest point of the line's
    /// rect. Squared because only the ORDER is ever used, and integers do not
    /// need a total-order dance to sort.
    pub distance_sq: i64,
}

/// Squared distance from `cursor` to the nearest point of `rect` — 0 inside it.
pub fn distance_sq(rect: [i32; 4], cursor: (i32, i32)) -> i64 {
    let [x, y, w, h] = rect;
    let dx = (x - cursor.0).max(cursor.0 - (x + w)).max(0) as i64;
    let dy = (y - cursor.1).max(cursor.1 - (y + h)).max(0) as i64;
    dx * dx + dy * dy
}

/// Map a hover crop's OCR lines back to screen space and score them by cursor
/// distance.
///
/// `upscale` is the factor `preprocess_for_ocr` applied to the crop, read off
/// the processed image rather than assumed:
/// every rect the OCR reports is in the PROCESSED image's pixel space, so
/// skipping the division would put every line at twice its real offset.
pub fn tooltip_lines(
    ocr: &[OcrLineBox],
    region: [i32; 4],
    upscale: (f32, f32),
    cursor: (i32, i32),
) -> Vec<TooltipLine> {
    let (sx, sy) = (upscale.0.max(f32::EPSILON), upscale.1.max(f32::EPSILON));
    ocr.iter()
        .map(|l| {
            let rect = [
                region[0] + (l.x as f32 / sx).round() as i32,
                region[1] + (l.y as f32 / sy).round() as i32,
                (l.w as f32 / sx).round().max(1.0) as i32,
                (l.h as f32 / sy).round().max(1.0) as i32,
            ];
            TooltipLine {
                text: l.text.clone(),
                distance_sq: distance_sq(rect, cursor),
            }
        })
        .collect()
}

/// Read a confirmation out of a hover tooltip (D5).
///
/// The matching line NEAREST the cursor wins, not the first one read. The hover
/// region is ~600×620 scaled px and deliberately overlaps the panel it was
/// opened from, so it contains the skill-name column too — and the two
/// vocabularies overlap (`Frenzy` is both a merc skill and a support family).
/// Taking the first match would let a skill name three rows up confirm the cell
/// under the cursor with the wrong identity, which is worse than not
/// confirming: it is a confident wrong id in front of the verdict engine.
///
/// `cell_tier` is the badge tier, used only when the tooltip title carried no
/// tier of its own. No tier at all → no confirmation: the family alone names up
/// to three different links.
pub fn confirm_from_tooltip(
    lines: &[TooltipLine],
    cell_tier: Option<u8>,
    vocab: &MercVocab,
    thresholds: &super::Thresholds,
) -> Option<ConfirmedCell> {
    let mut best: Option<(&TooltipLine, SupportTitleRead)> = None;
    for line in lines {
        let read = vocab.match_support_title(&line.text, thresholds);
        if read.state != ReadState::Matched {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(near, _)| line.distance_sq < near.distance_sq)
        {
            best = Some((line, read));
        }
    }
    let (_, title) = best?;
    let family = title.family?;
    let tier = title.tier.or(cell_tier)?;
    // A title that named its own tier already resolved to ids; a bare family
    // name did not, so the badge tier has to do the resolving.
    let (ids, name) = if title.tier.is_some() {
        (title.ids, title.name)
    } else {
        let matches = vocab.resolve(&family, tier);
        let (ids, name, _, _) = classify_resolution(&matches);
        (ids, name)
    };
    Some(ConfirmedCell {
        family,
        tier,
        ids,
        name,
        score: title.score,
    })
}

// ---------------------------------------------------------------------------
// The thread
// ---------------------------------------------------------------------------

/// Sleep in [`TICK`] slices, stopping early on cancel. `false` = cancelled.
fn nap(cancel: &watch::Receiver<bool>, total: Duration) -> bool {
    let mut left = total;
    while left > Duration::ZERO {
        if *cancel.borrow() {
            return false;
        }
        let step = left.min(TICK);
        std::thread::sleep(step);
        left = left.saturating_sub(step);
    }
    !*cancel.borrow()
}

/// Write the slice and emit only when something actually changed.
///
/// The loop touches the slice on every tick; the SSOT is polled by every
/// window, so emitting an identical snapshot 2-3× a second would be pure churn.
/// The `mercenary` guard is dropped before `emit_ssot` — it locks the same
/// mutex to compose the snapshot.
pub fn publish(app: &AppHandle, mutate: impl FnOnce(&mut MercenarySlice)) {
    let changed = {
        let state = app.state::<AppState>();
        let mut slice = state.mercenary.lock().unwrap_or_else(|e| e.into_inner());
        let before = slice.clone();
        mutate(&mut slice);
        *slice != before
    };
    if changed {
        crate::ssot::emit_ssot(app);
    }
}

/// Everything the loop carries between ticks.
struct Session {
    geometry: MercGeometry,
    vocab: MercVocab,
    state: LoopState,
    errors: OnceLog,
    /// The capture as last published — the hover tick mutates this copy.
    current: Option<MercCapture>,
    /// Pre-hover cell crops from the most recent detect, keyed `(row, slot)`.
    sigs: SigCache,
    confirmed: HashMap<(String, u8), ConfirmedCell>,
    /// The template-store generation this session's `confirmed` map agrees
    /// with. See [`generation_changed`].
    template_generation: u64,
    /// Where the template store lives, when there is an app data dir at all.
    icons_dir: Option<std::path::PathBuf>,
    /// Whether the first clean miss of this focus session has been logged.
    /// Reset when the game loses focus, so each return to the game says once
    /// what the loop saw.
    miss_logged: bool,
}

fn run_loop(app: AppHandle, cancel: watch::Receiver<bool>) {
    crate::app_log(&app, "Merc: capture loop started".to_string());
    crate::report_ocr_engine(&app);

    let data_dir = app.path().app_data_dir().ok();
    let (geometry, geometry_source, geometry_err) = match &data_dir {
        Some(dir) => super::load_override(dir),
        None => (
            MercGeometry::default(),
            super::GEOMETRY_SOURCE_DEFAULT,
            Some("no app data directory — geometry override cannot be read".to_string()),
        ),
    };
    if let Some(err) = &geometry_err {
        crate::app_log(&app, format!("Merc: {err}"));
    }
    crate::app_log(
        &app,
        format!("Merc: geometry source {geometry_source} (row pitch {:.1})", geometry.row_pitch),
    );

    // Load the learned templates before the first detect, so a restart does not
    // re-report every already-confirmed cell as unknown.
    let icons_dir = data_dir.as_ref().map(|d| d.join(super::ICONS_DIR));
    let mut template_problems = Vec::new();
    if let Some(dir) = &icons_dir {
        let (store, problems) = TemplateStore::load(dir);
        template_problems = problems;
        let learned = store.learned_keys();
        {
            let state = app.state::<AppState>();
            *state.merc_templates.lock().unwrap_or_else(|e| e.into_inner()) = store;
        }
        crate::app_log(&app, format!("Merc: {} learned templates loaded", learned.len()));
    }
    for problem in &template_problems {
        crate::app_log(&app, format!("Merc: template store — {problem}"));
    }

    let learned = learned_keys(&app);
    let source = geometry_source.to_string();
    publish(&app, |slice| {
        slice.status = MercStatus::Idle;
        slice.geometry_source = source;
        slice.learned_families = learned;
        slice.last_error = geometry_err;
    });

    let vocab = match MercVocab::load() {
        Ok(v) => v,
        Err(e) => return unavailable(&app, &cancel, e),
    };
    if let Err(e) = crate::ocr::engine_ready() {
        return unavailable(&app, &cancel, e);
    }

    let mut session = Session {
        geometry,
        vocab,
        state: LoopState::default(),
        errors: OnceLog::default(),
        current: None,
        sigs: SigCache::new(),
        confirmed: HashMap::new(),
        template_generation: template_generation(&app),
        icons_dir,
        miss_logged: false,
    };

    // Backdated so the first iteration detects immediately rather than after a
    // full cadence of doing nothing.
    let mut last_detect = Instant::now() - DETECT_INTERVAL_SLOW;
    let mut last_hover = Instant::now() - HOVER_INTERVAL;

    loop {
        if *cancel.borrow() {
            break;
        }

        if !game_focused(&app) {
            // No capture while alt-tabbed: the recruit window is not on screen,
            // and a full-screen OCR every second would be pure heat.
            session.miss_logged = false;
            if !nap(&cancel, UNFOCUSED_NAP) {
                break;
            }
            continue;
        }

        // Timed from before the detect to after the hover: they are one tick's
        // work, two screen grabs and two OCR calls, and the backoff is about
        // how long that whole thing takes on this machine.
        let mut tick_started = None;
        if last_detect.elapsed() >= session.state.detect_interval() {
            tick_started = Some(Instant::now());
            detect_tick(&app, &mut session, &cancel);
            last_detect = Instant::now();
        }

        // A stop that arrived during the detect must not buy another screen
        // grab and another OCR call: the hover tick is as expensive as the
        // detect, and a detached thread cannot be aborted out of it.
        if *cancel.borrow() {
            break;
        }

        if session.state.live && last_hover.elapsed() >= HOVER_INTERVAL {
            hover_tick(&app, &mut session);
            last_hover = Instant::now();
        }

        if let Some(started) = tick_started {
            let took = started.elapsed();
            if session.state.note_tick_duration(took) {
                crate::app_log(
                    &app,
                    format!(
                        "Merc: capture tick took {} ms — detect cadence backing off to {} s",
                        took.as_millis(),
                        DETECT_INTERVAL_SLOW.as_secs()
                    ),
                );
            }
        }

        if !nap(&cancel, TICK) {
            break;
        }
    }

    // A retired capture must not be left claiming it is on screen. Best-effort
    // by contract: on app exit the process is gone before this runs, which is
    // why `status` — forced to `off` by the SSOT composer once the module is
    // disabled — is what the page trusts.
    publish(&app, |slice| {
        slice.status = MercStatus::Idle;
        if let Some(capture) = slice.capture.as_mut() {
            capture.live = false;
        }
    });
    crate::app_log(&app, "Module mercenary: stopped".to_string());
}

/// Park the module as `unavailable` and idle until the stop signal.
///
/// The thread stays alive rather than returning so the module's running set
/// still reflects reality: it was started, it is switched on, and it is doing
/// nothing for a stated reason.
fn unavailable(app: &AppHandle, cancel: &watch::Receiver<bool>, reason: String) {
    crate::app_log(app, format!("Merc: capture unavailable — {reason}"));
    publish(app, |slice| {
        slice.status = MercStatus::Unavailable;
        slice.last_error = Some(reason.clone());
        if let Some(capture) = slice.capture.as_mut() {
            capture.live = false;
        }
    });
    while nap(cancel, UNFOCUSED_NAP) {}
    crate::app_log(app, "Module mercenary: stopped".to_string());
}

fn game_focused(app: &AppHandle) -> bool {
    // The RAW foreground read, not `game_focused`: that one is held over our
    // own windows so overlay clicks keep the overlays up, and under it this
    // loop captured the app itself.
    app.state::<AppState>()
        .game_in_foreground
        .load(std::sync::atomic::Ordering::SeqCst)
}

fn learned_keys(app: &AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let store = state.merc_templates.lock().unwrap_or_else(|e| e.into_inner());
    store.learned_keys()
}

/// The template store's edit counter — bumped by the forget/reset commands.
fn template_generation(app: &AppHandle) -> u64 {
    let state = app.state::<AppState>();
    let generation = state
        .merc_template_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    generation
}

fn debug_mode(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let debug = *state.debug_mode.lock().unwrap_or_else(|e| e.into_inner());
    debug
}

/// Log `msg` the first time this loop sees it, and record it as `last_error`.
fn fail(app: &AppHandle, session: &mut Session, msg: String) {
    if let Some(line) = session.errors.admit(&msg) {
        crate::app_log(app, line);
    }
    publish(app, |slice| slice.last_error = Some(msg));
}

/// A detect tick that produced no layout — because nothing was on screen, or
/// because the screen grab or the OCR failed.
///
/// A failed grab counts as a failed DETECTION, not as a separate kind of
/// event: the loop cannot see the recruit window either way, and a capture kept
/// alive through repeated failures would leave the page showing a verdict for a
/// window that closed two minutes ago. The error itself is already in
/// `last_error` and in the log.
fn miss(app: &AppHandle, session: &mut Session, errored: bool) {
    let retired = session.state.on_detect(false) == DetectOutcome::Retired;
    if retired {
        session.current = None;
        session.sigs.clear();
        session.confirmed.clear();
        crate::app_log(app, "Merc: window gone".to_string());
    }
    if !retired && errored {
        // Nothing to say: the error is already in `last_error`, and the capture
        // stands until it has been missed twice.
        return;
    }
    publish(app, |slice| {
        if retired {
            slice.status = MercStatus::Idle;
            if let Some(capture) = slice.capture.as_mut() {
                capture.live = false;
            }
        }
        // A clean miss — the loop looked and saw no recruit window — means the
        // last error is over. Leaving it set would keep a one-off OCR failure
        // on the page for the rest of the session.
        if !errored {
            slice.last_error = None;
        }
    });
}

/// One detect tick: grab the screen, OCR it, and publish what it holds.
fn detect_tick(app: &AppHandle, session: &mut Session, cancel: &watch::Receiver<bool>) {
    // Read the cursor BEFORE the grab, so the crop-merge rule below judges the
    // frame by where the cursor was while it was being taken.
    let cursor = crate::capture_mouse_position().ok();
    let img = match crate::capture::capture_screen() {
        Ok(img) => img,
        Err(e) => {
            fail(app, session, format!("Merc: screen capture failed — {e}"));
            return miss(app, session, true);
        }
    };
    let lines = match crate::ocr::recognize_lines(&img) {
        Ok(lines) => lines,
        Err(e) => {
            fail(app, session, format!("Merc: OCR failed — {e}"));
            return miss(app, session, true);
        }
    };

    let Some(layout) = geometry::detect(&lines, &session.geometry, &session.vocab) else {
        // Logged once per focus session: a loop that never detects would
        // otherwise leave no trace of having looked at all.
        if !session.miss_logged {
            session.miss_logged = true;
            let skills = lines
                .iter()
                .filter(|l| {
                    session.vocab.match_skill(&l.text, &session.geometry.thresholds).state
                        != ReadState::Unknown
                })
                .count();
            crate::app_log(
                app,
                format!(
                    "Merc: looked, no recruit window — {} OCR lines, {} skill candidates",
                    lines.len(),
                    skills
                ),
            );
        }
        return miss(app, session, false);
    };

    // Pass 2 is up to `max_rows` more OCR calls. A stop signal that arrived
    // during pass 1 stops here, leaving the state exactly as it was.
    if *cancel.borrow() {
        return;
    }
    let texts = pass2_texts(&img, &layout, &session.geometry);
    let mut result = {
        let state = app.state::<AppState>();
        let store = state.merc_templates.lock().unwrap_or_else(|e| e.into_inner());
        build_capture(
            &img,
            &layout,
            &texts,
            now_ms(),
            &session.geometry,
            &session.vocab,
            &store,
        )
    };
    // A forget/reset while this capture was live means the user disowned a
    // confirmation; re-applying it here is exactly what the un-poison button
    // was pressed to stop.
    if generation_changed(&mut session.template_generation, template_generation(app)) {
        session.confirmed.clear();
    }
    apply_confirmed(&mut result.capture, &session.confirmed);

    let outcome = session.state.on_detect(true);
    if outcome == DetectOutcome::Captured {
        crate::app_log(
            app,
            format!(
                "Merc: recruit window detected ({} rows, scale {:.3})",
                result.capture.rows.len(),
                result.capture.scale
            ),
        );
    }
    session.sigs = merge_sigs(
        std::mem::take(&mut session.sigs),
        result.sigs,
        hovered_key(&result.capture, cursor),
    );
    session.current = Some(result.capture.clone());
    publish(app, |slice| {
        slice.status = MercStatus::Live;
        slice.capture = Some(result.capture);
        slice.last_error = None;
    });
}

/// One hover tick: if the cursor sits in an unconfirmed captured cell, read the
/// tooltip and let it name the cell (D5).
fn hover_tick(app: &AppHandle, session: &mut Session) {
    let Some(capture) = session.current.clone() else {
        return;
    };
    let cursor = match crate::capture_mouse_position() {
        Ok(c) => c,
        Err(e) => return fail(app, session, format!("Merc: cursor position failed — {e}")),
    };
    let Some((ri, si)) = cell_at(&capture, cursor) else {
        return;
    };
    if capture.rows[ri].supports[si].state == ReadState::Confirmed {
        return;
    }
    let Some(region) = hover_region(cursor, capture.scale, &session.geometry.thresholds, capture.screen)
    else {
        return;
    };

    // A FRESH grab: the tooltip is only on screen now, and was not in the
    // detect frame. The template still comes from the detect frame's crop.
    let img = match crate::capture::capture_screen() {
        Ok(img) => img,
        Err(e) => return fail(app, session, format!("Merc: hover capture failed — {e}")),
    };
    let (iw, ih) = {
        use image::GenericImageView;
        img.dimensions()
    };
    if (region[0] + region[2]) as u32 > iw || (region[1] + region[3]) as u32 > ih {
        return;
    }
    let crop = img.crop_imm(
        region[0] as u32,
        region[1] as u32,
        region[2] as u32,
        region[3] as u32,
    );
    let processed = crate::capture::preprocess_for_ocr(&crop);
    // RECTS, not just strings: the region deliberately overlaps the panel, so
    // which line is nearest the cursor is the only thing separating the tooltip
    // title from the skill column behind it.
    let ocr_lines = match crate::ocr::recognize_lines(&processed) {
        Ok(lines) => lines,
        Err(e) => return fail(app, session, format!("Merc: hover OCR failed — {e}")),
    };
    let upscale = (
        processed.width() as f32 / crop.width().max(1) as f32,
        processed.height() as f32 / crop.height().max(1) as f32,
    );
    let lines = tooltip_lines(&ocr_lines, region, upscale, cursor);

    let cell = &capture.rows[ri].supports[si];
    let Some(confirmation) =
        confirm_from_tooltip(&lines, cell.tier, &session.vocab, &session.geometry.thresholds)
    else {
        // Only in debug mode: a hover that names no support is the NORMAL case
        // for a cursor resting on an unlearned cell whose tooltip has not opened
        // yet, and logging every read would bury the confirmations.
        if debug_mode(app) {
            let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
            crate::app_log(
                app,
                format!(
                    "Merc: hover over row {} slot {} confirmed nothing: {texts:?}",
                    ri, cell.slot
                ),
            );
        }
        return;
    };
    let family = confirmation.family.clone();
    let tier = confirmation.tier;

    let row_index = capture.rows[ri].index;
    let cached = session.sigs.get(&(row_index, cell.slot)).cloned();
    let (learned, save_err) = match cached {
        Some((sig, raw)) => {
            let state = app.state::<AppState>();
            let mut store = state.merc_templates.lock().unwrap_or_else(|e| e.into_inner());
            // The crop is the DETECT frame's, cached before the cursor ever
            // reached this cell (D5): a hovered cell may be drawn highlighted,
            // and a template learned from the highlight matches nothing later.
            let learned = store.learn(&family, tier, sig, raw);
            let err = session
                .icons_dir
                .as_ref()
                .filter(|_| learned)
                .and_then(|dir| store.save(dir).err());
            (
                if learned {
                    Learned::Saved
                } else {
                    Learned::AlreadyKnown
                },
                err,
            )
        }
        // The confirmation still stands — it names the cell. Only the template
        // is missing, and saying so is the difference between "we already knew
        // this art" and "we never had the crop".
        None => (Learned::NoCrop, None),
    };
    if let Some(e) = save_err {
        fail(app, session, format!("Merc: template store save failed — {e}"));
    }

    crate::app_log(
        app,
        format!(
            "Merc: confirmed {} at row {row_index} slot {} (family {family}, tier {tier}) — {}",
            confirmation.name.as_deref().unwrap_or(&family),
            cell.slot,
            learned.describe(),
        ),
    );

    let key = (row_key(&capture.rows[ri].skill), cell.slot);
    session.confirmed.insert(key, confirmation.clone());

    let mut updated = capture;
    apply_one(&mut updated.rows[ri].supports[si], &confirmation);
    session.current = Some(updated.clone());
    let learned_families = learned_keys(app);
    publish(app, |slice| {
        slice.capture = Some(updated);
        slice.learned_families = learned_families;
    });
}

/// What a hover-confirm did to the template store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Learned {
    /// A new `(family, tier)` sample was recorded and flushed to disk.
    Saved,
    /// The store already held this pair; a confirmed sample is never
    /// overwritten (`TemplateStore::learn`).
    AlreadyKnown,
    /// No pre-hover crop was cached for the cell — the capture that produced it
    /// has been replaced since. The cell is still confirmed.
    NoCrop,
}

impl Learned {
    fn describe(self) -> &'static str {
        match self {
            Learned::Saved => "template saved",
            Learned::AlreadyKnown => "template already known",
            Learned::NoCrop => "no pre-hover crop cached, template not learned",
        }
    }
}

/// Write a confirmation into a single cell. Shared with [`apply_confirmed`] so
/// a confirmed cell looks the same whether it was just confirmed or restored
/// onto a later capture.
fn apply_one(cell: &mut MercSupportRead, c: &ConfirmedCell) {
    cell.family = Some(c.family.clone());
    cell.tier = Some(c.tier);
    cell.ids = c.ids.clone();
    cell.name = c.name.clone();
    cell.score = c.score;
    cell.state = ReadState::Confirmed;
    cell.candidates.clear();
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mercenary::{MercRow, Thresholds};

    fn vocab() -> MercVocab {
        MercVocab::load().expect("the compiled-in vocabulary parses")
    }

    fn thresholds() -> crate::mercenary::Thresholds {
        MercGeometry::default().thresholds
    }

    fn cell(slot: u8, rect: [i32; 4]) -> MercSupportRead {
        MercSupportRead {
            slot,
            rect,
            family: None,
            tier: None,
            ids: Vec::new(),
            name: None,
            score: 0.0,
            state: ReadState::Unknown,
            candidates: Vec::new(),
        }
    }

    fn capture_with(rows: Vec<MercRow>) -> MercCapture {
        MercCapture {
            captured_at_ms: 0,
            live: true,
            scale: 1.0,
            screen: [2560, 1440],
            header: Default::default(),
            rows,
        }
    }

    fn row(index: u8, skill_id: &str, cells: Vec<MercSupportRead>) -> MercRow {
        MercRow {
            index,
            skill: MercSkillRead {
                raw: "Ice Shot".into(),
                ids: vec![skill_id.to_string()],
                name: Some("Ice Shot".into()),
                score: 0.99,
                state: ReadState::Matched,
            },
            supports: cells,
        }
    }

    /// A window that blinks for one tick must NOT be retired — the page would
    /// drop a verdict the user is still looking at. Two consecutive misses is
    /// the rule (D6).
    #[test]
    fn one_missed_detection_keeps_the_capture_live() {
        let mut st = LoopState::default();
        st.on_detect(true);

        let outcome = st.on_detect(false);

        assert_eq!(outcome, DetectOutcome::Missed);
        assert!(st.live, "one miss must not retire a live capture");
    }

    #[test]
    fn two_consecutive_missed_detections_retire_the_capture() {
        let mut st = LoopState::default();
        st.on_detect(true);
        st.on_detect(false);

        let outcome = st.on_detect(false);

        assert_eq!(outcome, DetectOutcome::Retired);
        assert!(!st.live);
    }

    /// The misses must be CONSECUTIVE: a hit between two misses resets the
    /// count, or a flickering read would retire a window that never left.
    #[test]
    fn a_successful_detection_between_misses_resets_the_miss_count() {
        let mut st = LoopState::default();
        st.on_detect(true);
        st.on_detect(false);

        assert_eq!(st.on_detect(true), DetectOutcome::Refreshed);
        assert_eq!(st.on_detect(false), DetectOutcome::Missed);
        assert!(st.live, "the earlier miss must not count toward this one");
    }

    /// Retiring twice in a row must not need four misses the second time.
    #[test]
    fn the_miss_count_resets_after_a_retirement() {
        let mut st = LoopState::default();
        st.on_detect(true);
        st.on_detect(false);
        st.on_detect(false);

        st.on_detect(true);
        st.on_detect(false);

        assert_eq!(st.on_detect(false), DetectOutcome::Retired);
    }

    /// A miss with nothing live is a no-op, not a retirement — otherwise the
    /// idle loop would log "window gone" every second forever.
    #[test]
    fn missing_a_window_that_was_never_there_is_not_a_retirement() {
        let mut st = LoopState::default();

        assert_eq!(st.on_detect(false), DetectOutcome::Missed);
        assert_eq!(st.on_detect(false), DetectOutcome::Missed);
        assert!(!st.live);
    }

    #[test]
    fn finding_a_window_for_the_first_time_reports_a_capture() {
        let mut st = LoopState::default();

        assert_eq!(st.on_detect(true), DetectOutcome::Captured);
        assert_eq!(st.on_detect(true), DetectOutcome::Refreshed);
    }

    /// The idle cadence is 1 s; a live one re-detects at 2 s (D6).
    #[test]
    fn a_live_capture_re_detects_on_the_slower_cadence() {
        let mut st = LoopState::default();
        assert_eq!(st.detect_interval(), DETECT_INTERVAL);

        st.on_detect(true);

        assert_eq!(st.detect_interval(), REDETECT_INTERVAL);
    }

    /// The backoff fires once and only once, and only above the threshold.
    #[test]
    fn a_slow_detect_tick_backs_the_idle_cadence_off_once() {
        let mut st = LoopState::default();

        assert!(!st.note_tick_duration(SLOW_TICK), "at the threshold is not over it");
        assert_eq!(st.detect_interval(), DETECT_INTERVAL);
        assert!(st.note_tick_duration(SLOW_TICK + Duration::from_millis(1)));
        assert_eq!(st.detect_interval(), DETECT_INTERVAL_SLOW);
        assert!(
            !st.note_tick_duration(Duration::from_secs(9)),
            "the backoff line is logged once, not on every slow tick",
        );
    }

    /// The backoff governs the HUNT, not a found window: a live capture keeps
    /// its 2 s re-detect even on a slow machine.
    #[test]
    fn the_backoff_does_not_slow_a_live_capture() {
        let mut st = LoopState::default();
        st.note_tick_duration(Duration::from_secs(3));
        st.on_detect(true);

        assert_eq!(st.detect_interval(), REDETECT_INTERVAL);
    }

    #[test]
    fn the_same_error_is_logged_once_and_a_different_one_still_gets_through() {
        let mut log = OnceLog::default();

        assert_eq!(log.admit("boom").as_deref(), Some("boom"));
        assert_eq!(log.admit("boom"), None);
        assert_eq!(log.admit("other").as_deref(), Some("other"));
    }

    /// Past the cap the loop says so ONCE and then goes quiet — an error
    /// carrying a varying number would otherwise be a new string every tick.
    #[test]
    fn past_the_cap_one_suppression_notice_replaces_further_errors() {
        let mut log = OnceLog::default();
        for i in 0..MAX_DISTINCT_ERRORS {
            assert!(log.admit(&format!("error {i}")).is_some());
        }

        let notice = log.admit("one too many").expect("the cap must announce itself");

        assert!(notice.contains("suppressed"), "got {notice:?}");
        assert_eq!(log.admit("another"), None);
        assert_eq!(
            log.admit("error 0").as_deref(),
            None,
            "an already-seen message stays deduplicated after the cap",
        );
    }

    /// The hover box is mostly ABOVE the cursor, scaled with the panel — the
    /// numbers are the tooltip guess, and this is what makes them checkable
    /// against the first Windows dump.
    #[test]
    fn the_hover_region_is_centred_horizontally_and_biased_upward() {
        let t = Thresholds::default();

        let [x, y, w, h] = hover_region((1000, 800), 1.0, &t, [2560, 1440]).expect("on screen");

        assert_eq!((x, w), (700, 600));
        assert_eq!(y, 300, "hover_up above the cursor");
        assert_eq!(h, 620, "hover_up + hover_down tall");
    }

    /// A 4K client draws a bigger tooltip; the region scales with the panel.
    #[test]
    fn the_hover_region_scales_with_the_capture() {
        let t = Thresholds::default();

        // Far enough from every edge that the clamp does not participate —
        // this test is about the scale factor, and the clamp has its own.
        let [_, _, w, h] = hover_region((1900, 1200), 2.0, &t, [3840, 2160]).expect("on screen");

        assert_eq!(w, 1200, "hover_w 600 at scale 2");
        assert_eq!(h, 1240, "(hover_up 500 + hover_down 120) at scale 2");
    }

    /// Clamped to the screen — an unclamped rect would make `crop_imm` panic
    /// on a cursor near an edge, which is where tooltips actually get read.
    #[test]
    fn the_hover_region_is_clamped_to_the_screen() {
        let t = Thresholds::default();

        let [x, y, w, h] = hover_region((10, 10), 1.0, &t, [1920, 1080]).expect("on screen");

        assert_eq!((x, y), (0, 0));
        assert_eq!(w, 310, "clipped at the left edge, not shifted");
        assert_eq!(h, 130);
    }

    #[test]
    fn a_cursor_off_the_captured_screen_has_no_hover_region() {
        let t = Thresholds::default();

        assert!(hover_region((-4000, 500), 1.0, &t, [1920, 1080]).is_none());
    }

    /// The hit-test is what decides whether a hover means anything, and it must
    /// answer with VECTOR indices — the caller mutates `supports[si]`.
    #[test]
    fn the_cursor_maps_to_the_cell_it_is_inside() {
        let capture = capture_with(vec![
            row(0, "skill.a", vec![cell(0, [100, 100, 44, 44]), cell(1, [149, 100, 44, 44])]),
            row(1, "skill.b", vec![cell(0, [100, 150, 44, 44])]),
        ]);

        assert_eq!(cell_at(&capture, (110, 110)), Some((0, 0)));
        assert_eq!(cell_at(&capture, (160, 120)), Some((0, 1)));
        assert_eq!(cell_at(&capture, (110, 160)), Some((1, 0)));
    }

    /// The gaps between cells are not cells: a cursor there must not confirm
    /// the neighbouring icon with whatever tooltip happens to be up.
    #[test]
    fn a_cursor_in_the_gap_between_cells_hits_nothing() {
        let capture = capture_with(vec![row(
            0,
            "skill.a",
            vec![cell(0, [100, 100, 44, 44]), cell(1, [149, 100, 44, 44])],
        )]);

        assert_eq!(cell_at(&capture, (146, 110)), None);
    }

    /// Right/bottom edges are exclusive, left/top inclusive — the cell pitch is
    /// 49 for a 44 px cell, so an inclusive right edge would overlap nothing,
    /// but an off-by-one at 44 would mis-slot a cursor on the boundary.
    #[test]
    fn the_cell_hit_test_boundaries_are_half_open() {
        let capture = capture_with(vec![row(0, "skill.a", vec![cell(0, [100, 100, 44, 44])])]);

        assert_eq!(cell_at(&capture, (100, 100)), Some((0, 0)));
        assert_eq!(cell_at(&capture, (143, 143)), Some((0, 0)));
        assert_eq!(cell_at(&capture, (144, 120)), None);
        assert_eq!(cell_at(&capture, (120, 144)), None);
    }

    /// D5: a confirmation survives the next detect of the same window. Keyed on
    /// the SKILL, so it lands on the right row even when the rows renumber.
    #[test]
    fn a_confirmation_is_restored_onto_a_later_capture_of_the_same_row() {
        let mut confirmed = HashMap::new();
        confirmed.insert(
            ("skill.b".to_string(), 1),
            ConfirmedCell {
                family: "Chain".into(),
                tier: 2,
                ids: vec!["mercenary.support_9".into()],
                name: Some("Greater Chain (Tier 2)".into()),
                score: 0.99,
            },
        );
        // The row that was index 1 at confirm time is index 0 now.
        let mut capture = capture_with(vec![row(
            0,
            "skill.b",
            vec![cell(0, [0, 0, 44, 44]), cell(1, [49, 0, 44, 44])],
        )]);

        apply_confirmed(&mut capture, &confirmed);

        let restored = &capture.rows[0].supports[1];
        assert_eq!(restored.state, ReadState::Confirmed);
        assert_eq!(restored.name.as_deref(), Some("Greater Chain (Tier 2)"));
        assert_eq!(restored.tier, Some(2));
        assert_eq!(restored.ids, vec!["mercenary.support_9".to_string()]);
        assert_eq!(
            capture.rows[0].supports[0].state,
            ReadState::Unknown,
            "only the confirmed slot is upgraded",
        );
    }

    /// The key is (row identity, slot): the same slot number on a DIFFERENT
    /// skill row is a different cell and must not inherit the confirmation.
    #[test]
    fn a_confirmation_does_not_leak_onto_another_skill_row() {
        let mut confirmed = HashMap::new();
        confirmed.insert(
            ("skill.a".to_string(), 0),
            ConfirmedCell {
                family: "Chain".into(),
                tier: 2,
                ids: vec!["mercenary.support_9".into()],
                name: Some("Greater Chain (Tier 2)".into()),
                score: 0.99,
            },
        );
        let mut capture = capture_with(vec![row(0, "skill.z", vec![cell(0, [0, 0, 44, 44])])]);

        apply_confirmed(&mut capture, &confirmed);

        assert_eq!(capture.rows[0].supports[0].state, ReadState::Unknown);
    }

    /// A signature whose pixel values are a deterministic function of `seed`,
    /// so two of them are distinguishable and neither is flat.
    fn sig(seed: u8) -> CellSig {
        let gray: Vec<u8> = (0..24u32 * 24)
            .map(|i| (i as u8).wrapping_mul(7).wrapping_add(seed))
            .collect();
        CellSig::from_gray(gray).expect("a gradient signature is not flat")
    }

    fn cache(entries: &[((u8, u8), u8)]) -> SigCache {
        entries
            .iter()
            .map(|(key, seed)| (*key, (sig(*seed), None)))
            .collect()
    }

    /// THE pre-hover rule (D5). The loop re-detects every 2 s while the user
    /// hovers, so the fresh crop of the hovered cell can be of HIGHLIGHTED art.
    /// Taking it would teach the store the highlight and the template would
    /// match nothing afterwards.
    #[test]
    fn the_hovered_cells_crop_is_kept_cold_across_a_re_detect() {
        let previous = cache(&[((0, 0), 1), ((0, 1), 2)]);
        let fresh = cache(&[((0, 0), 9), ((0, 1), 9)]);

        let merged = merge_sigs(previous, fresh, Some((0, 0)));

        assert_eq!(
            merged[&(0, 0)].0,
            sig(1),
            "the hovered cell must keep the crop taken before the cursor arrived",
        );
        assert_eq!(
            merged[&(0, 1)].0,
            sig(9),
            "every other cell takes the fresh crop, so a moved window re-caches",
        );
    }

    /// A cell first seen WHILE hovered has no cold crop to keep. Caching the
    /// hovered one anyway is the bug; caching nothing makes the confirm report
    /// `NoCrop` and learn nothing, which is the honest outcome.
    #[test]
    fn a_cell_first_seen_while_hovered_caches_no_crop_at_all() {
        let merged = merge_sigs(SigCache::new(), cache(&[((0, 0), 9)]), Some((0, 0)));

        assert!(merged.is_empty());
    }

    /// With the cursor outside every cell, the merge is a plain replacement —
    /// the cache must track a window that moved or rescaled.
    #[test]
    fn with_no_cell_hovered_every_crop_is_replaced() {
        let merged = merge_sigs(cache(&[((0, 0), 1)]), cache(&[((0, 0), 9)]), None);

        assert_eq!(merged[&(0, 0)].0, sig(9));
    }

    /// Cells the fresh detect no longer sees are dropped: their rects are stale,
    /// and a crop keyed to a rect that no longer exists can only mislearn.
    #[test]
    fn a_cell_the_new_detect_did_not_see_is_dropped_from_the_cache() {
        let merged = merge_sigs(cache(&[((0, 0), 1), ((5, 3), 2)]), cache(&[((0, 0), 9)]), None);

        assert_eq!(merged.len(), 1);
        assert!(!merged.contains_key(&(5, 3)));
    }

    /// The hovered key is the CELL's own `(row index, slot)`, not the vector
    /// positions `cell_at` answers with — the crop cache is keyed by identity.
    #[test]
    fn the_hovered_key_is_the_rows_index_and_the_cells_slot() {
        let capture = capture_with(vec![row(
            4,
            "skill.a",
            vec![cell(2, [100, 100, 44, 44]), cell(3, [149, 100, 44, 44])],
        )]);

        assert_eq!(hovered_key(&capture, Some((160, 110))), Some((4, 3)));
        assert_eq!(hovered_key(&capture, Some((10, 10))), None);
        assert_eq!(hovered_key(&capture, None), None);
    }

    /// Forgetting a template must also drop the CONFIRMATION the loop is still
    /// re-applying from memory — otherwise the un-poison button changes the
    /// store and the page keeps showing the disowned identity.
    #[test]
    fn a_bumped_template_generation_is_reported_once() {
        let mut seen = 0;

        assert!(!generation_changed(&mut seen, 0), "no edit, nothing to drop");
        assert!(generation_changed(&mut seen, 1), "the forget must be noticed");
        assert!(
            !generation_changed(&mut seen, 1),
            "and noticed once — clearing every tick would drop live confirmations",
        );
        assert!(generation_changed(&mut seen, 2));
    }

    /// Distance is to the NEAREST point of the rect, and zero inside it, so a
    /// tooltip line the cursor sits on always wins.
    #[test]
    fn a_lines_distance_is_measured_to_its_nearest_edge() {
        let rect = [100, 100, 40, 20];

        assert_eq!(distance_sq(rect, (110, 105)), 0, "inside the rect");
        assert_eq!(distance_sq(rect, (143, 105)), 9, "3 px right of the edge");
        assert_eq!(distance_sq(rect, (110, 96)), 16, "4 px above");
        assert_eq!(distance_sq(rect, (137, 124)), 16, "below, still inside in x");
    }

    /// OCR runs on the UPSCALED crop, so every rect comes back at 2× the crop's
    /// own coordinates. Skipping the division would put every line at twice its
    /// real offset and hand the nearest-line rule garbage.
    #[test]
    fn tooltip_line_rects_are_mapped_back_through_the_upscale_and_the_region() {
        let ocr = vec![OcrLineBox { text: "Greater Chain".into(), x: 40, y: 20, w: 200, h: 32 }];

        let lines = tooltip_lines(&ocr, [700, 300, 600, 620], (2.0, 2.0), (720, 312));

        // 40/2 + 700 = 720, 20/2 + 300 = 310 — the cursor is 2 px below the top
        // of a 16 px tall line, so it is INSIDE the mapped rect.
        assert_eq!(lines[0].distance_sq, 0);
        // Without the division the rect would start at x=740, 20 px away.
        assert!(tooltip_lines(&ocr, [700, 300, 600, 620], (1.0, 1.0), (720, 312))[0].distance_sq > 0);
    }

    fn tooltip(text: &str, distance_sq: i64) -> TooltipLine {
        TooltipLine { text: text.to_string(), distance_sq }
    }

    /// The hover region deliberately overlaps the panel, so the skill column is
    /// in it — and `Frenzy` is the ONE name that is both a merc skill and a
    /// support family (checked against the vocabulary). First-match would let a
    /// skill row three rows up name the cell under the cursor; nearest-match
    /// takes the tooltip that is actually open.
    #[test]
    fn the_matching_line_nearest_the_cursor_wins_over_an_earlier_one() {
        // `Chain (Tier 2)` is the vocabulary's real spelling — the tier-2 rung
        // of this family carries no grade word.
        let lines = vec![tooltip("Frenzy", 40_000), tooltip("Chain (Tier 2)", 100)];

        let confirmed = confirm_from_tooltip(&lines, Some(2), &vocab(), &thresholds())
            .expect("the near line confirms");

        assert_eq!(confirmed.family, "Chain");
        assert_eq!(confirmed.tier, 2);
    }

    /// The rule is distance, not a `Frenzy` blocklist: when the Frenzy support
    /// tooltip IS the nearest line, it confirms normally.
    #[test]
    fn a_far_match_still_confirms_when_it_is_the_only_one() {
        let lines = vec![tooltip("Frenzy", 40_000)];

        let confirmed = confirm_from_tooltip(&lines, Some(3), &vocab(), &thresholds())
            .expect("the only match confirms");

        assert_eq!(confirmed.family, "Frenzy");
        assert_eq!(confirmed.name.as_deref(), Some("Gilded Frenzy (Tier 3)"));
        assert_eq!(confirmed.ids.len(), 1);
    }

    /// A title spelled as a bare family name carries no tier, so the badge's
    /// tier resolves it — that is the only path from "Chain" to an id.
    #[test]
    fn a_bare_family_title_takes_its_tier_from_the_badge() {
        let confirmed = confirm_from_tooltip(&[tooltip("Chain", 0)], Some(2), &vocab(), &thresholds())
            .expect("a bare family plus a badge tier confirms");

        assert_eq!(confirmed.tier, 2);
        assert!(
            !confirmed.ids.is_empty(),
            "the badge tier is what turns a family into vocabulary ids",
        );
    }

    /// No tier from either side is no confirmation: the family alone names up
    /// to three different links, and a guess would be a confident wrong id.
    #[test]
    fn a_bare_family_title_with_no_badge_tier_confirms_nothing() {
        assert!(confirm_from_tooltip(&[tooltip("Chain", 0)], None, &vocab(), &thresholds()).is_none());
    }

    /// Lines that name no support confirm nothing — the normal case for a
    /// cursor resting on a cell whose tooltip has not opened yet.
    #[test]
    fn tooltip_lines_that_name_no_support_confirm_nothing() {
        let lines = vec![tooltip("Wager: 1 028", 10), tooltip("TAKE ITEM", 20)];

        assert!(confirm_from_tooltip(&lines, Some(2), &vocab(), &thresholds()).is_none());
    }

    /// The registry's rule for thread modules: no single blocking call may
    /// outlast the poll ceiling, because a detached thread cannot be aborted.
    /// Every wait in this loop is built out of `TICK`, so this is the whole
    /// compliance argument in one assertion.
    #[test]
    fn every_wait_is_built_out_of_slices_under_the_module_poll_ceiling() {
        assert!(
            TICK < crate::modules::MODULE_THREAD_POLL_CEILING,
            "TICK {TICK:?} must stay well under the {:?} ceiling",
            crate::modules::MODULE_THREAD_POLL_CEILING,
        );
        for cadence in [DETECT_INTERVAL, DETECT_INTERVAL_SLOW, REDETECT_INTERVAL, UNFOCUSED_NAP] {
            assert_eq!(
                cadence.as_millis() % TICK.as_millis(),
                0,
                "{cadence:?} must divide into whole TICK slices, or the last slice overshoots",
            );
        }
    }

    /// A row whose skill did not resolve still needs a stable identity, or its
    /// confirmations would be lost on every re-detect.
    #[test]
    fn an_unmatched_row_is_keyed_by_its_raw_text() {
        let skill = MercSkillRead {
            raw: "  Ba11 Lightning  ".into(),
            ids: Vec::new(),
            name: None,
            score: 0.4,
            state: ReadState::Unknown,
        };

        assert_eq!(row_key(&skill), "ba11 lightning");
    }
}
