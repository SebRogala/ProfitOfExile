//! The shared Lab capture scheduler.
//!
//! Gem and font OCR stay on one dedicated OS thread because both use the
//! Windows OCR/COM path. The scheduler owns one capture per eligible tick;
//! the two processing states own only their own generation, timeout and
//! per-frame bookkeeping.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::{AppState, CaptureRegion, FontSessionData};

pub struct LabScanState {
    pub detected_gems: Mutex<Vec<String>>,
    /// Generation counter for gem OCR scans. Incremented on each start trigger
    /// (FontOpened, manual scan).
    pub gem_scan_generation: AtomicU64,
    /// Generation counter for font panel OCR scans.
    pub font_scan_generation: AtomicU64,
    /// Generation of the font scan session that is currently running, or 0 when
    /// none is. `FontOpened` re-arms the scan when it reads 0 — after a portal
    /// trip no further `LabFinished` fires, so the event is the only chance to
    /// bring the panel OCR back. Written by `spawn_font_scan` (its own
    /// generation), by the loop on exit (compare-exchange, so a stale loop
    /// cannot clear its replacement's token) and by anything that bumps
    /// `font_scan_generation` without starting a replacement.
    pub font_scan_live_gen: AtomicU64,
    /// Liveness token for the single merged lab OCR thread.
    pub lab_scan_live_gen: AtomicU64,
    /// Counter from which the merged lab thread mints its liveness token.
    pub lab_scan_generation: AtomicU64,
    /// Monotonic count of `FontOpened` events. The craft ledger gates every
    /// count change on it: the panel's count cannot change without a CRAFT
    /// click, and a CRAFT click always fires this event, so a count change with
    /// no new event is a misread. Never reset — the ledger stores the value it
    /// accepted at, and a counter going backwards would re-open accepted rounds.
    pub font_opened_seq: AtomicU64,
    /// Font session data — accumulated rounds, shared between the lab scan loop
    /// and handlers.
    pub font_session: Mutex<FontSessionData>,
}

const SCAN_INTERVAL: Duration = Duration::from_millis(250);
// 2.5 min (owner, 2026-09-09; was 45 s): reading the options, crafting and
// hovering three results took longer than 45 s, the scan expired under a
// player still hovering, and the manual restart was killed by the CONFIRM
// event seconds later. CONFIRM, ZoneChanged and 3/3 still end a scan early.
const GEM_TIMEOUT: Duration = Duration::from_secs(150);
const IDLE_LIMIT: Duration = Duration::from_secs(600);
const MAX_GEMS: u32 = 3;
const MAX_REJECT_LOGS: usize = 12;

struct GemProcessingState {
    generation_seen: u64,
    started_at: Option<Instant>,
    finished: bool,
    loop_count: u32,
    matcher: Option<crate::gem_matcher::GemMatcher>,
    names_rx: Option<tokio::sync::oneshot::Receiver<Vec<String>>>,
    seen_gems: HashSet<String>,
    gems_found: u32,
    logged_rejects: HashSet<String>,
    rejects_suppressed: bool,
    logged_repeats: HashSet<String>,
    // Ticks since the band last read text; starts "long ago" so the first read
    // of a scan logs.
    ticks_since_text: u32,
    ticks_read: u32,
    ticks_with_text: u32,
    logged_assumed_1080p: bool,
    logged_short_crop: bool,
}

impl Default for GemProcessingState {
    fn default() -> Self {
        Self {
            generation_seen: 0,
            started_at: None,
            finished: false,
            loop_count: 0,
            matcher: None,
            names_rx: None,
            seen_gems: HashSet::new(),
            gems_found: 0,
            logged_rejects: HashSet::new(),
            rejects_suppressed: false,
            logged_repeats: HashSet::new(),
            ticks_since_text: u32::MAX,
            ticks_read: 0,
            ticks_with_text: 0,
            logged_assumed_1080p: false,
            logged_short_crop: false,
        }
    }
}

impl GemProcessingState {
    fn reset_for_generation(
        &mut self,
        app: &AppHandle,
        state: &AppState,
        generation: u64,
    ) -> u64 {
        let previous = self.generation_seen;
        if generation == previous {
            return previous;
        }

        if previous != 0 {
            crate::app_log(
                app,
                format!(
                    "Gem scan stopped (new scan or manual stop; {} gems found, the band read text on {} of {} ticks)",
                    self.gems_found, self.ticks_with_text, self.ticks_read,
                ),
            );
            crate::report_ocr_engine(app);
        }

        self.generation_seen = generation;
        self.started_at = None;
        self.finished = false;
        self.loop_count = 0;
        self.matcher = None;
        self.names_rx = None;
        self.seen_gems.clear();
        self.gems_found = 0;
        self.logged_rejects.clear();
        self.rejects_suppressed = false;
        self.logged_repeats.clear();
        self.ticks_since_text = u32::MAX;
        self.ticks_read = 0;
        self.ticks_with_text = 0;
        self.logged_assumed_1080p = false;
        self.logged_short_crop = false;

        // Start the dictionary request at the generation boundary, but hand
        // its result back without blocking this shared capture thread. The
        // gem timeout starts only after the vocabulary is ready, so a request
        // cannot stall the font session or spend its scan budget.
        if generation != 0 {
            let server = state
                .server_url
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            let http = state.server_http.clone();
            let lab_mode = state
                .lab_mode
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            let app_for_fetch = app.clone();
            let (tx, rx) = tokio::sync::oneshot::channel();
            tauri::async_runtime::spawn(async move {
                let gem_names =
                    crate::fetch_gem_names(&app_for_fetch, &server, &http, &lab_mode).await;
                let _ = tx.send(gem_names);
            });
            self.names_rx = Some(rx);
        }

        previous
    }

    fn active(&self, generation: u64, picking_gems: bool) -> bool {
        generation != 0 && picking_gems && !self.finished
    }

    fn frame_consumers(&self, gem_active: bool, font_active: bool) -> (bool, bool) {
        (gem_active && self.matcher.is_some(), font_active)
    }

    fn accept_dictionary(&mut self, names: Vec<String>) {
        self.matcher = Some(crate::gem_matcher::GemMatcher::new(names));
        self.started_at = Some(Instant::now());
    }

    /// Poll the non-blocking dictionary result. Returns true when this scan
    /// became inactive because the dictionary was empty.
    fn poll_dictionary(&mut self, app: &AppHandle, state: &AppState, generation: u64) -> bool {
        let dictionary = match self.names_rx.as_mut() {
            Some(rx) => match rx.try_recv() {
                Ok(names) => Some(names),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Some(Vec::new()),
            },
            None => None,
        };
        let Some(names) = dictionary else { return false };

        self.names_rx = None;
        if names.is_empty() {
            crate::app_log(
                app,
                "Gem scan aborted — the gem dictionary loaded 0 names, so no OCR read could match. Either the request failed or this league has no gem dictionary yet; the preceding 'gem names' lines say which."
                    .to_string(),
            );
            if state.lab_scan.gem_scan_generation.load(Ordering::SeqCst) == generation {
                self.finished = true;
                let mut lab_state = state
                    .lab_state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if *lab_state == crate::lab_state::LabState::PickingGems {
                    *lab_state = crate::lab_state::LabState::Idle;
                    drop(lab_state);
                    crate::emit_status(app);
                }
                return true;
            }
            return false;
        }

        crate::app_log(app, format!("Gem scan: loaded {} gem names", names.len()));
        self.accept_dictionary(names);
        false
    }

    fn timed_out(&self) -> bool {
        self.started_at
            .is_some_and(|started_at| started_at.elapsed() >= GEM_TIMEOUT)
    }

    fn stop_for_timeout(&mut self, app: &AppHandle, state: &AppState, generation: u64) {
        crate::app_log(
            app,
            format!(
                "Gem scan timed out after {}s ({} gems found; the band read text on {} of {} ticks)",
                GEM_TIMEOUT.as_secs(),
                self.gems_found,
                self.ticks_with_text,
                self.ticks_read,
            ),
        );
        self.finished = true;
        let mut lab_state = state
            .lab_state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if state.lab_scan.gem_scan_generation.load(Ordering::SeqCst) == generation
            && *lab_state == crate::lab_state::LabState::PickingGems
        {
            *lab_state = crate::lab_state::LabState::Idle;
            drop(lab_state);
            crate::emit_status(app);
        }
    }

    fn accept_gem(&mut self, expected_generation: u64, current_generation: u64, name: &str) -> bool {
        if expected_generation != current_generation || !self.seen_gems.insert(name.to_string()) {
            return false;
        }
        self.gems_found += 1;
        true
    }

    fn process_frame(
        &mut self,
        app: &AppHandle,
        state: &AppState,
        grab: &crate::capture::Capture,
        region: &CaptureRegion,
        generation: u64,
    ) {
        let cropped = grab.image.crop_imm(
            region.x.max(0) as u32,
            region.y.max(0) as u32,
            region.w,
            region.h,
        );
        if !self.logged_short_crop {
            if let Some(line) = crate::crop_shortfall(region, cropped.width(), cropped.height()) {
                self.logged_short_crop = true;
                crate::app_log(app, line);
            }
        }
        if state
            .gem_band_dump
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            crate::dump_gem_band(app, &cropped, self.loop_count);
        }

        // The two upscale kernels alternate tick by tick: Lanczos3 on even
        // ticks, bilinear on odd. Measured 2026-09-09 on the app's own band
        // dumps: "Split Arrow of Splitting" hovered in the font socket sat
        // in the band for 51 ticks and the Lanczos3 2× image returned NO
        // lines from Windows OCR, while the same crop read at 1×, at 3× and
        // with Triangle 2× — the kernel's ringing on that header, not the
        // capture. A second pass on every empty tick would run twice per
        // tick for the whole scan (the band is empty until the player gets
        // from CRAFT to a hover), so the kernels take turns instead: one
        // OCR per tick, and a hovered header is read by whichever kernel
        // can within two ticks (owner, 2026-09-09).
        let bilinear_tick = self.loop_count % 2 == 1;
        let processed = if bilinear_tick {
            crate::capture::preprocess_for_ocr_fast(&cropped)
        } else {
            crate::capture::preprocess_for_ocr(&cropped)
        };
        match crate::ocr::recognize_text(&processed) {
            Ok(lines) => {
                let candidates = crate::ocr::extract_gem_candidates(&lines);
                self.ticks_read += 1;
                let band_has_text = !candidates.is_empty();
                if band_has_text {
                    self.ticks_with_text += 1;
                }
                // Logged when the band reads text after at least two empty ticks — both
                // kernels' turns, so a header only one kernel reads does not log on every
                // other tick — then every 8th tick while it keeps reading. A hover shorter
                // than eight ticks still leaves one line, so a scan with none of these lines
                // read NOTHING off the band (2026-09-09: a third gem went undetected with no
                // trace of what the band saw).
                if band_has_text
                    && (self.ticks_since_text >= 2 || self.loop_count % 8 == 1)
                {
                    crate::app_log(
                        app,
                        format!(
                            "Gem OCR candidates ({}): {:?}",
                            if bilinear_tick { "bilinear" } else { "lanczos" },
                            candidates
                        ),
                    );
                }
                self.ticks_since_text = if band_has_text {
                    0
                } else {
                    self.ticks_since_text.saturating_add(1)
                };

                if let Some(matcher) = self.matcher.as_ref() {
                    let mut best: Option<crate::gem_matcher::GemMatch> = None;
                    for candidate in &candidates {
                        match matcher.match_gem(candidate) {
                            Ok(m) => {
                                if best.as_ref().map_or(true, |b| m.score > b.score) {
                                    best = Some(m);
                                }
                            }
                            Err(reason) => {
                                let line =
                                    format!("Gem OCR rejected {:?}: {}", candidate, reason);
                                if self.logged_rejects.len() < MAX_REJECT_LOGS {
                                    if self.logged_rejects.insert(line.clone()) {
                                        crate::app_log(app, line);
                                    }
                                } else if !self.rejects_suppressed {
                                    self.rejects_suppressed = true;
                                    crate::app_log(
                                        app,
                                        format!(
                                            "Gem OCR: {} distinct rejections logged — further rejections suppressed for this scan",
                                            MAX_REJECT_LOGS,
                                        ),
                                    );
                                }
                            }
                        }
                    }

                    if let Some(gem_match) = best {
                        let current_generation =
                            state.lab_scan.gem_scan_generation.load(Ordering::SeqCst);
                        if current_generation == generation
                            && self.accept_gem(generation, current_generation, &gem_match.name)
                        {
                            crate::app_log(
                                app,
                                format!(
                                    "Gem detected: {} (score: {:.2}) [{}/{}] from OCR {:?}",
                                    gem_match.name,
                                    gem_match.score,
                                    self.gems_found,
                                    MAX_GEMS,
                                    gem_match.ocr_raw
                                ),
                            );
                            let all_gems = {
                                let mut gems = state
                                    .lab_scan
                                    .detected_gems
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner());
                                gems.push(gem_match.name.clone());
                                let cloned = gems.clone();
                                drop(gems);
                                cloned
                            };
                            if let Err(e) = app.emit("gem-detected", &gem_match.name) {
                                log::warn!("emit gem-detected failed: {}", e);
                            }
                            crate::emit_status(app);
                            let app_clone = app.clone();
                            tauri::async_runtime::spawn(async move {
                                crate::send_gems_to_server(&app_clone, all_gems).await;
                            });
                            if self.gems_found >= MAX_GEMS {
                                crate::app_log(
                                    app,
                                    "Gem scan complete (3/3 gems detected)".to_string(),
                                );
                                self.finished = true;
                                let mut lab_state = state
                                    .lab_state
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner());
                                if state.lab_scan.gem_scan_generation.load(Ordering::SeqCst) == generation {
                                    *lab_state = crate::lab_state::LabState::Idle;
                                    drop(lab_state);
                                    crate::emit_status(app);
                                }
                            }
                        } else if current_generation == generation
                            && self.logged_repeats.insert(gem_match.name.clone())
                        {
                            // A re-read of a gem this scan already holds was invisible: a
                            // third gem whose text resolves to a detected name (a duplicate,
                            // or a base name that jaro-winkler lands on its transfigured
                            // sibling) left the log silent. Once per name per scan.
                            crate::app_log(
                                app,
                                format!(
                                    "Gem OCR: {:?} read as {} again — already detected this scan",
                                    gem_match.ocr_raw, gem_match.name,
                                ),
                            );
                        }
                    }
                }
            }
            Err(e) => {
                if self.loop_count % 20 == 1 {
                    crate::app_log(app, format!("Gem OCR failed: {}", e));
                }
            }
        }
    }
}

struct FontProcessingState {
    generation_seen: u64,
    loop_count: u32,
    last_active: Instant,
    frame_saw_panel: bool,
    logged_assumed_1080p: bool,
    logged_short_crop: bool,
}

impl Default for FontProcessingState {
    fn default() -> Self {
        Self {
            generation_seen: 0,
            loop_count: 0,
            last_active: Instant::now(),
            frame_saw_panel: false,
            logged_assumed_1080p: false,
            logged_short_crop: false,
        }
    }
}

impl FontProcessingState {
    fn reset_for_generation(&mut self, app: &AppHandle, generation: u64) -> u64 {
        let previous = self.generation_seen;
        if generation == previous {
            return previous;
        }
        if previous != 0 {
            crate::app_log(app, "Font scan stopped (generation mismatch)".to_string());
            crate::report_ocr_engine(app);
        }
        self.generation_seen = generation;
        self.loop_count = 0;
        self.last_active = Instant::now();
        self.frame_saw_panel = false;
        self.logged_assumed_1080p = false;
        self.logged_short_crop = false;
        previous
    }

    fn refresh_active(
        &mut self,
        app: &AppHandle,
        state: &AppState,
        generation: u64,
    ) -> bool {
        let mut active = crate::font_session::font_scan_is_live(
            state.lab_scan.font_scan_live_gen.load(Ordering::SeqCst),
        );
        if active {
            let (deadline, idle_expired) = crate::font_session::idle_tick(
                self.last_active,
                Instant::now(),
                self.frame_saw_panel,
                IDLE_LIMIT,
            );
            self.last_active = deadline;
            self.frame_saw_panel = false;
            if idle_expired {
                crate::app_log(
                    app,
                    "Font scan stopped (no font panel for 10 minutes)".to_string(),
                );
                crate::send_font_session_data(app);
                if state
                    .lab_scan
                    .font_scan_live_gen
                    .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    active = false;
                } else {
                    active = crate::font_session::font_scan_is_live(
                        state.lab_scan.font_scan_live_gen.load(Ordering::SeqCst),
                    );
                }
            }
        }
        active
    }

    fn frame_is_current(
        &self,
        expected_generation: u64,
        current_generation: u64,
        live_generation: u64,
    ) -> bool {
        expected_generation == current_generation
            && crate::font_session::font_scan_is_live(live_generation)
    }

    fn process_frame(
        &mut self,
        app: &AppHandle,
        state: &AppState,
        grab: &crate::capture::Capture,
        region: &CaptureRegion,
        generation: u64,
    ) {
        let cropped = grab.image.crop_imm(
            region.x.max(0) as u32,
            region.y.max(0) as u32,
            region.w,
            region.h,
        );
        if !self.logged_short_crop {
            if let Some(line) = crate::crop_shortfall(region, cropped.width(), cropped.height()) {
                self.logged_short_crop = true;
                crate::app_log(app, line);
            }
        }
        let processed = crate::capture::preprocess_for_ocr(&cropped);
        match crate::ocr::recognize_text(&processed) {
            Ok(lines) => {
                let panel = crate::font_parser::parse_font_panel(&lines);
                self.frame_saw_panel = panel.font_active;
                if !panel.font_active && !lines.is_empty() && self.loop_count % 40 == 1 {
                    crate::app_log(
                        app,
                        format!(
                            "Font OCR raw (inactive, {} lines): {}",
                            lines.len(),
                            lines.join(" | ")
                        ),
                    );
                }
                if panel.font_active && !panel.options.is_empty() {
                    let outcome = {
                        let mut session = state
                            .lab_scan
                            .font_session
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        if !self.frame_is_current(
                            generation,
                            state.lab_scan.font_scan_generation.load(Ordering::SeqCst),
                            state.lab_scan.font_scan_live_gen.load(Ordering::SeqCst),
                        ) {
                            None
                        } else {
                            let event_seq = state.lab_scan.font_opened_seq.load(Ordering::SeqCst);
                            Some(crate::font_session::apply_font_frame(
                                &mut session,
                                &panel,
                                event_seq,
                            ))
                        }
                    };
                    if let Some(outcome) = outcome {
                        if let Some(sealed) = &outcome.sealed {
                            crate::app_log(
                                app,
                                format!(
                                    "Font round {} sealed ({} options{})",
                                    sealed.number,
                                    sealed.round.options.len(),
                                    sealed.round.crafts_remaining.map_or(
                                        ", last craft".to_string(),
                                        |n| format!(", {} remaining", n),
                                    ),
                                ),
                            );
                            crate::emit_status(app);
                        }
                        if outcome.buffer_grew {
                            crate::app_log(
                                app,
                                format!(
                                    "Font OCR raw ({} lines): {}",
                                    lines.len(),
                                    lines.join(" | ")
                                ),
                            );
                            crate::app_log(
                                app,
                                format!(
                                    "Font options captured: {} options{}{}",
                                    outcome.buffer.len(),
                                    if panel.jackpot_detected {
                                        " *** JACKPOT! ***"
                                    } else {
                                        ""
                                    },
                                    outcome.crafts_remaining.map_or(
                                        " (last craft)".to_string(),
                                        |n| format!(" (remaining: {})", n),
                                    ),
                                ),
                            );
                            for opt in &outcome.buffer {
                                crate::app_log(
                                    app,
                                    format!(
                                        "  - {} {}",
                                        opt.option_type,
                                        opt.value
                                            .map(|v| format!("({})", v))
                                            .unwrap_or_default()
                                    ),
                                );
                            }
                            if panel.jackpot_detected {
                                if let Err(e) = app.emit("font-jackpot", true) {
                                    log::warn!("emit font-jackpot failed: {}", e);
                                }
                            }
                        }
                    } else {
                        crate::app_log(
                            app,
                            "Font scan stopped (generation mismatch)".to_string(),
                        );
                    }
                }
            }
            Err(e) => {
                if self.loop_count % 40 == 1 {
                    crate::app_log(app, format!("Font scan: OCR failed: {}", e));
                }
            }
        }
    }
}

fn dispatch_captured_frame<T, Gem, Font>(
    frame: &T,
    gem_ready: bool,
    font_active: bool,
    mut process_gem: Gem,
    mut process_font: Font,
) where
    Gem: FnMut(&T),
    Font: FnMut(&T),
{
    if gem_ready {
        process_gem(frame);
    }
    if font_active {
        process_font(frame);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum IdleScanDecision {
    Wait,
    Reclaimed,
    Exit,
}

fn settle_idle_scan<F>(
    live_token: &AtomicU64,
    own_generation: u64,
    pending_after_clear: F,
) -> IdleScanDecision
where
    F: FnOnce() -> bool,
{
    if !crate::font_session::try_clear_live(live_token, own_generation) {
        return IdleScanDecision::Wait;
    }
    // The token is clear now, so a trigger arriving after this point can claim it
    // and start a replacement thread. A trigger that arrived before the clear was
    // rejected while this thread still owned the token; if work is now pending,
    // reclaim it so this worker handles it instead of exiting with no worker.
    if pending_after_clear()
        && live_token
            .compare_exchange(0, own_generation, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        IdleScanDecision::Reclaimed
    } else {
        IdleScanDecision::Exit
    }
}

pub(crate) fn run(app: AppHandle, lab_generation: u64) {
    let state = app.state::<AppState>();
    let mut gem = GemProcessingState::default();
    let mut font = FontProcessingState::default();
    let mut logged_tick_cost = false;

    crate::report_ocr_engine(&app);

    loop {
        let gem_generation = state.lab_scan.gem_scan_generation.load(Ordering::SeqCst);
        let previous_gem_generation =
            gem.reset_for_generation(&app, &state, gem_generation);

        let font_generation = state.lab_scan.font_scan_generation.load(Ordering::SeqCst);
        let _previous_font_generation = font.reset_for_generation(&app, font_generation);

        let picking_gems = *state
            .lab_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            == crate::lab_state::LabState::PickingGems;
        let mut gem_active = gem.active(gem_generation, picking_gems);
        if gem_generation != previous_gem_generation
            && previous_gem_generation != 0
            && !picking_gems
        {
            gem.finished = true;
            gem_active = false;
        }
        if gem_generation != 0
            && previous_gem_generation != 0
            && !picking_gems
            && !gem.finished
        {
            gem.finished = true;
            gem_active = false;
            crate::app_log(&app, "Gem scan stopped (state changed)".to_string());
        }
        if gem_active && gem.timed_out() {
            gem.stop_for_timeout(&app, &state, gem_generation);
            gem_active = false;
        }

        let font_active = font.refresh_active(&app, &state, font_generation);

        if gem_active && gem.matcher.is_none() {
            if gem.poll_dictionary(&app, &state, gem_generation) {
                gem_active = false;
            }
        }

        if !gem_active && !font_active {
            let pending = state.lab_scan.font_scan_live_gen.load(Ordering::SeqCst) != 0
                || (*state
                    .lab_state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    == crate::lab_state::LabState::PickingGems);
            if pending {
                // A trigger can land between the active-state read and this
                // branch. Keep the shared thread idle without spinning while
                // the replacement generation becomes visible.
                std::thread::sleep(SCAN_INTERVAL);
                continue;
            }
            match settle_idle_scan(&state.lab_scan.lab_scan_live_gen, lab_generation, || {
                state.lab_scan.font_scan_live_gen.load(Ordering::SeqCst) != 0
                    || (*state
                        .lab_state
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        == crate::lab_state::LabState::PickingGems)
            }) {
                IdleScanDecision::Reclaimed => continue,
                IdleScanDecision::Exit => break,
                IdleScanDecision::Wait => continue,
            }
        }

        let screen = *state
            .screen
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let lab = crate::ssot::placements_for(screen.as_ref()).lab;
        let gem_region = crate::capture_region(lab.gem);
        let font_region = crate::capture_region(lab.font);
        if screen.is_none() {
            if !gem.logged_assumed_1080p && gem_active {
                gem.logged_assumed_1080p = true;
                crate::app_log(
                    &app,
                    format!(
                        "lab OCR regions unscaled: no screen measured yet — the gem region assumes 1080p ({}, {}) {}x{}",
                        gem_region.x, gem_region.y, gem_region.w, gem_region.h,
                    ),
                );
            }
            if !font.logged_assumed_1080p && font_active {
                font.logged_assumed_1080p = true;
                crate::app_log(
                    &app,
                    format!(
                        "lab OCR regions unscaled: no screen measured yet — the font panel region assumes 1080p ({}, {}) {}x{}",
                        font_region.x, font_region.y, font_region.w, font_region.h,
                    ),
                );
            }
        }

        if gem_active {
            gem.loop_count += 1;
        }
        if font_active {
            font.loop_count += 1;
        }
        let tick_started = Instant::now();
        let grab = match crate::capture::capture_screen(&app) {
            Ok(grab) => grab,
            Err(e) => {
                if gem_active && gem.loop_count % 20 == 1 {
                    crate::app_log(&app, format!("Screen capture failed: {}", e));
                }
                if font_active && font.loop_count % 40 == 1 {
                    crate::app_log(&app, format!("Font scan: screen capture failed: {}", e));
                }
                if *state.debug_mode.lock().unwrap_or_else(|e| e.into_inner())
                    && !logged_tick_cost
                {
                    logged_tick_cost = true;
                    crate::app_log(
                        &app,
                        format!("Lab scan tick: {} ms", tick_started.elapsed().as_millis()),
                    );
                }
                std::thread::sleep(SCAN_INTERVAL);
                continue;
            }
        };

        let (gem_ready, font_ready) = gem.frame_consumers(gem_active, font_active);
        dispatch_captured_frame(
            &grab,
            gem_ready,
            font_ready,
            |frame| gem.process_frame(&app, &state, frame, &gem_region, gem_generation),
            |frame| font.process_frame(&app, &state, frame, &font_region, font_generation),
        );

        if *state.debug_mode.lock().unwrap_or_else(|e| e.into_inner())
            && !logged_tick_cost
        {
            logged_tick_cost = true;
            crate::app_log(
                &app,
                format!("Lab scan tick: {} ms", tick_started.elapsed().as_millis()),
            );
        }
        std::thread::sleep(SCAN_INTERVAL);
    }

    crate::font_session::try_clear_live(&state.lab_scan.lab_scan_live_gen, lab_generation);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_pending_does_not_prevent_font_frame_processing() {
        let gem = GemProcessingState::default();
        let (gem_ready, font_ready) = gem.frame_consumers(true, true);
        let mut gem_calls = 0;
        let mut font_value = None;

        dispatch_captured_frame(
            &17_u32,
            gem_ready,
            font_ready,
            |_| gem_calls += 1,
            |frame| font_value = Some(*frame),
        );

        assert_eq!(gem_calls, 0);
        assert_eq!(font_value, Some(17));
    }

    #[test]
    fn one_captured_frame_is_shared_by_both_consumers() {
        let frame = 17_u32;
        let address = (&frame as *const u32) as usize;
        let mut gem_address = None;
        let mut font_address = None;

        dispatch_captured_frame(
            &frame,
            true,
            true,
            |same| gem_address = Some(same as *const u32 as usize),
            |same| font_address = Some(same as *const u32 as usize),
        );

        assert_eq!(gem_address, Some(address));
        assert_eq!(font_address, Some(address));
    }

    #[test]
    fn dictionary_completion_starts_the_gem_timeout() {
        let mut gem = GemProcessingState::default();

        assert!(gem.started_at.is_none());
        gem.accept_dictionary(vec!["Fireball".to_string()]);

        assert!(gem.started_at.is_some());
        assert!(gem.matcher.is_some());
    }

    #[test]
    fn a_superseded_gem_is_rejected_before_consuming_a_slot() {
        let mut gem = GemProcessingState::default();

        assert!(!gem.accept_gem(7, 8, "Fireball"));
        assert_eq!(gem.gems_found, 0);
        assert!(gem.seen_gems.is_empty());
    }

    #[test]
    fn a_duplicate_gem_does_not_consume_a_new_slot() {
        let mut gem = GemProcessingState::default();

        assert!(gem.accept_gem(7, 7, "Fireball"));
        assert!(!gem.accept_gem(7, 7, "Fireball"));
        assert_eq!(gem.gems_found, 1);
        assert_eq!(gem.seen_gems.len(), 1);
    }

    #[test]
    fn a_superseded_font_frame_is_rejected_before_session_update() {
        let font = FontProcessingState::default();

        assert!(!font.frame_is_current(7, 8, 7));
        assert!(!font.frame_is_current(7, 7, 0));
        assert!(font.frame_is_current(7, 7, 7));
    }

    #[test]
    fn an_old_lab_loop_cannot_clear_a_replacement_token() {
        let token = AtomicU64::new(8);

        assert_eq!(
            settle_idle_scan(&token, 7, || false),
            IdleScanDecision::Wait
        );
        assert_eq!(token.load(Ordering::SeqCst), 8);
    }
}
