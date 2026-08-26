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
//! # Nothing runs until something asks
//!
//! The loop does NO screen work of its own (POE-198). A Client.txt voice line
//! or the page's Scan now button arms the gate in [`super::trigger`]; only then
//! does it look, and only while the game is the foreground window.
//!
//! The two asks buy different looks (POE-204 WI-C). A voice line buys two
//! [`probe_tick`]s — a band OCR at 500 ms and, if that saw nothing, one more at
//! 1.5 s — and then the gate stands down, because a mercenary speaks on
//! approach as often as on click and most lines are for a window nobody opened.
//! Scan now buys one full [`detect_tick`], because a person asking has already
//! answered the question the probe would ask. Either way, the first detected
//! window disarms the gate and the live behaviour below takes over unchanged;
//! a gate that finds nothing says so once and the loop goes back to waiting.
//!
//! # A tooltip is not a closed window
//!
//! Hovering a cell opens a game tooltip ON the panel, and the rows underneath
//! it stop being readable — so the detect that follows a hover finds no layout
//! at all. Two of those retire the capture. Two rules keep the player's work
//! through that (2026-08-25 smoke):
//!
//! - a detect that finds nothing while the cursor is inside the live capture's
//!   panel rect is [`DetectOutcome::Occluded`] — no miss counted, nothing
//!   published, the capture held — until [`OCCLUDED_MAX`] of CONTINUOUS
//!   occlusion, after which every tick counts again and the ordinary two-miss
//!   retire lands one cadence later ([`OcclusionRun`]);
//! - a retire hands its confirmations AND its header to a one-slot
//!   [`Retained`] instead of dropping them, and the next detect takes them back
//!   only on positive evidence that the panel is the same one
//!   ([`same_panel_positive`], not the live path's abstaining rule).
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
use super::read::{build_capture, capture_complete, fold_header, same_panel_positive, pass2_texts};
use super::search::{self, MercTradeSession};
use super::vocab::{classify_resolution, MercVocab, SupportTitleRead};
use super::sync;
use super::trigger;
use super::{
    MercCapture, MercGeometry, MercHeader, MercSkillRead, MercStatus, MercSupportRead,
    MercenarySlice, ReadState,
};

/// Loop quantum. Every wait is built out of these so a stop signal is honoured
/// within one of them, whatever the cadence above it says.
const TICK: Duration = Duration::from_millis(100);
/// Detect cadence while no window is captured (D6).
///
/// Since POE-204 WI-C almost nothing runs at it. The gate answers a voice line
/// with two band probes rather than a cadence of detects, and Scan now with one
/// detect; what is left for this constant is the SECOND detect of a hunt that
/// has not landed yet — a probe hit whose detect found nothing, on the tick
/// after. It is kept at 1 s because the backoff below still measures against
/// it, and because a hunt with nothing to hunt for now ends in one iteration
/// rather than ten seconds of them.
const DETECT_INTERVAL: Duration = Duration::from_millis(1000);
/// Detect cadence after the backoff has fired.
const DETECT_INTERVAL_SLOW: Duration = Duration::from_millis(3000);
/// Re-detect cadence while a window IS captured.
const REDETECT_INTERVAL: Duration = Duration::from_millis(2000);
/// Detect cadence while a captured window is fully read (2026-08-25).
///
/// A complete capture ([`super::read::capture_complete`]) has nothing left for
/// another pass to improve, so the only question left is whether the window is
/// still on screen — and that is worth one detect every ten seconds, not one
/// every two. The cost of the slower answer is bounded and stated: a window
/// that closes is noticed up to `2 × this` late (two misses retire it), which
/// delays the strip's "recruit window gone" marker and nothing else.
const LIVENESS_INTERVAL: Duration = Duration::from_millis(10_000);
/// Hover-confirm cadence while a window is captured.
const HOVER_INTERVAL: Duration = Duration::from_millis(400);
/// A FULL-frame detect slower than this backs the detect cadence off.
///
/// Two things are excluded from the measurement, and both for one reason: what
/// the backoff decides is [`DETECT_INTERVAL`] vs [`DETECT_INTERVAL_SLOW`], and
/// those two cadences govern nothing but the full-screen HUNT for a window.
///
/// - **Not the hover confirm** that shares the iteration with the detect.
///   MEASURED 2026-08-26 (app.log 09:40:06): one 4504 ms reading of
///   detect+hover latched the backoff for the life of the thread, so every
///   FIRST detect after a voice line waited [`DETECT_INTERVAL_SLOW`] — 3 s of
///   "nothing happening" bought by a number that was mostly a tooltip OCR the
///   backoff has no say over.
/// - **Not a cropped re-detect** (POE-204 WI-B review). A crop of a known panel
///   is a fraction of a full-screen OCR by construction, so feeding those in
///   would decay the backoff on evidence about a cheaper question — and the
///   `crop→full` re-take is the opposite bias, a tick carrying two OCRs. Both
///   run only while a window is live, where the cadence is
///   [`REDETECT_INTERVAL`] and the backoff has no say either way. See
///   [`LoopState::note_tick_duration`].
const SLOW_TICK: Duration = Duration::from_millis(1500);
/// Consecutive detects at or under [`SLOW_TICK`] that clear the backoff.
///
/// The backoff used to be sticky for the life of the thread, on the reasoning
/// that "this machine is slow" does not become false. The cropped re-detect
/// (POE-204 WI-B) makes it false on purpose — the same machine that took
/// 4.5 s on a full-screen tick reads a crop of the known panel in a fraction
/// of it — and a slow FIRST detect would otherwise hold the 3 s hunt cadence
/// over every later window of the session. Three in a row rather than one so a
/// single fast frame between slow ones cannot flap the cadence, and with it the
/// log line that announces the cadence.
const BACKOFF_DECAY_DETECTS: u8 = 3;
/// How long to idle between focus checks while the game is not focused.
const UNFOCUSED_NAP: Duration = Duration::from_millis(1000);
/// How long to idle between gate checks while nothing has asked for a scan
/// (POE-198). Short because it is the latency a burst pays before its first
/// detect, and cheap because the check is one mutex read — no screen, no OCR.
const IDLE_NAP: Duration = Duration::from_millis(250);
/// Consecutive failed detections that retire a live capture (D6).
const RETIRE_AFTER: u8 = 2;
/// How long a live capture may be held through detects that find nothing while
/// the cursor sits inside the panel (2026-08-25 smoke).
///
/// The tolerance cannot be unbounded: a window closed with the cursor parked
/// where it used to be would never retire, and the strip would show a verdict
/// for a panel that is not on screen. Fifteen seconds is far longer than any
/// tooltip read and far shorter than a session, so the visible cost of the cap
/// firing is the ordinary two-miss retire arriving late.
const OCCLUDED_MAX: Duration = Duration::from_secs(15);
/// How long a retired capture's confirmations stay available to a re-detect of
/// the SAME panel (2026-08-25 smoke).
///
/// Long enough to cover a retire the player caused by hovering (~4 s from
/// tooltip to re-detect), short enough that a panel reopened much later is read
/// fresh rather than inheriting a stale session's opinions.
const RETAINED_TTL: Duration = Duration::from_secs(60);
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
    /// Nothing found, but the cursor is inside the live capture's panel — the
    /// game has drawn something over it. The capture is untouched: no miss
    /// counted, nothing published. See [`miss_kind`].
    Occluded,
}

/// What a detect that found no layout means for the live capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissKind {
    /// Count it: advance the retire counter and publish it.
    Miss,
    /// Hold the capture and count nothing.
    Occluded,
}

/// Whether a detect that found nothing is evidence the window is gone.
///
/// MEASURED 2026-08-25 (app.log 19:03): hovering a support cell opened a
/// tooltip ON the panel, the next two detects read 55 OCR lines and 14 skill
/// candidates but no layout, [`RETIRE_AFTER`] fired, and the confirmation the
/// hover had just made went away with the capture — the cell flipped back to ✕
/// while the player was still looking at it.
///
/// The cursor is the proof. The game only opens a tooltip under the cursor, so
/// a cursor inside the panel's own bounds is the one thing that separates "the
/// panel is covered" from "the panel is gone" without another screen grab.
///
/// `occluded_for` is how long the CURRENT occlusion run has been going, and
/// [`OCCLUDED_MAX`] caps it so a window closed with the cursor parked over it
/// still retires. A cursor outside the panel, or no live capture, is an
/// ordinary miss whatever the clock says.
pub fn miss_kind(live: bool, cursor_in_panel: bool, occluded_for: Duration) -> MissKind {
    if live && cursor_in_panel && occluded_for < OCCLUDED_MAX {
        MissKind::Occluded
    } else {
        MissKind::Miss
    }
}

/// One continuous run of detects that found nothing while the cursor sat on the
/// panel — the clock [`miss_kind`] measures against, and the once-per-run gate
/// for the log line.
///
/// A type rather than a field on [`Session`] because the run has exactly one
/// subtle rule and it was got wrong the first time: **a run that hits the cap
/// stays OPEN.** Clearing it on the miss it just produced restarts the clock,
/// so the next tick is occluded again and the two-miss retire needs two full
/// caps to land — ~30 s at the 2 s re-detect cadence and ~50 s at the liveness
/// cadence. TAKE ITEM sits inside [`super::geometry::panel_bounds`] — since
/// `PANEL_FOOTER_PITCHES` extended that rect over the footer, and NOT before:
/// the one-pitch rect this comment was written against stopped above the
/// buttons, so a cursor on TAKE ITEM was outside the panel and the bug it
/// describes could not fire from there. It can now, which is what the cap is
/// for: without the open-run rule a closed window's `done` verdict would stay
/// on screen for the better part of a minute while the cursor rested on the
/// button that closed it.
///
/// The run ends on the four things that actually end it: the panel is found
/// again ([`Self::on_hit`]), the cursor leaves the panel (handled inside
/// [`Self::on_occluded`], which is already told where the cursor is), the
/// capture retires ([`Self::on_retired`]), and the game goes behind us
/// ([`Self::on_focus_lost`], because no detect runs there and the clock would
/// otherwise count minutes nothing looked at).
#[derive(Debug, Default)]
pub struct OcclusionRun {
    /// When the open run started. `None` — no run open.
    started: Option<Instant>,
    /// Whether the open run has had its log line.
    announced: bool,
}

impl OcclusionRun {
    /// Fold one detect that found no layout into the run.
    ///
    /// `now` is a parameter rather than an `Instant::now()` inside so the cap
    /// and its aftermath are testable as a sequence without sleeping.
    pub fn on_occluded(&mut self, live: bool, cursor_in_panel: bool, now: Instant) -> MissKind {
        let elapsed = self
            .started
            .map(|since| now.saturating_duration_since(since))
            .unwrap_or_default();
        match miss_kind(live, cursor_in_panel, elapsed) {
            MissKind::Occluded => {
                self.started.get_or_insert(now);
                MissKind::Occluded
            }
            // The cursor is still on the panel, so this Miss is the CAP firing.
            // The run stays open: every later tick must count too, or the
            // retire this cap exists to allow never arrives.
            MissKind::Miss if live && cursor_in_panel => MissKind::Miss,
            // The cursor left (or nothing is live) — whatever was covering the
            // panel is no longer the explanation, so the next occlusion is a
            // new run with a fresh cap.
            MissKind::Miss => {
                self.reset();
                MissKind::Miss
            }
        }
    }

    /// The panel was detected: whatever was covering it is gone.
    pub fn on_hit(&mut self) {
        self.reset();
    }

    /// The capture retired. There is no panel left to be occluded.
    pub fn on_retired(&mut self) {
        self.reset();
    }

    /// The game is no longer the foreground window, so no detect runs.
    pub fn on_focus_lost(&mut self) {
        self.reset();
    }

    /// Whether a run is open — a detect has already come back with no layout
    /// while the cursor sat on the panel, and none of the four things that end
    /// a run has happened since.
    ///
    /// Read by [`detect_step`], which stops holding the re-detect once this is
    /// true: the hold's premise is that the cursor makes the detect redundant,
    /// and an open run is that premise already disproved.
    pub fn is_open(&self) -> bool {
        self.started.is_some()
    }

    /// The once-per-run log gate: `true` the first time it is asked in each
    /// run, `false` for every later tick of that same run.
    pub fn announce(&mut self) -> bool {
        if self.started.is_none() || self.announced {
            return false;
        }
        self.announced = true;
        true
    }

    fn reset(&mut self) {
        self.started = None;
        self.announced = false;
    }
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
    /// The slow-detect backoff has fired.
    ///
    /// NOT sticky since POE-204 WI-B — see [`BACKOFF_DECAY_DETECTS`].
    pub backed_off: bool,
    /// Consecutive detects at or under [`SLOW_TICK`] while backed off. Reset by
    /// any slow detect, and by the decay it feeds.
    pub fast_detects: u8,
    /// The live capture is fully read — every row, every cell, the header.
    ///
    /// NOT sticky, unlike [`Self::backed_off`]: it is a statement about the
    /// capture on screen, so it is cleared when that capture retires and when
    /// Scan now asks for another look ([`Self::resume`]).
    pub complete: bool,
}

impl LoopState {
    /// How long to wait before the next detect tick.
    ///
    /// A live capture re-detects on its own cadence (2 s) and is NOT subject to
    /// the backoff: the backoff exists to stop a slow machine spending all its
    /// time hunting for a window, and a live window has already been found.
    ///
    /// A live capture that is COMPLETE drops to the liveness cadence — the
    /// re-read has nothing left to improve, and the 2026-08-25 smoke showed
    /// what the pointless re-reads cost: the header blinked between two OCR
    /// readings of the same unchanged pixels every two seconds.
    pub fn detect_interval(&self) -> Duration {
        if self.live && self.complete {
            LIVENESS_INTERVAL
        } else if self.live {
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
                // The completeness belonged to the capture that just went
                // away. Carrying it would leave the next window being hunted
                // at the liveness cadence.
                self.complete = false;
                DetectOutcome::Retired
            } else {
                DetectOutcome::Missed
            }
        }
    }

    /// Fold the completeness of the capture just published into the state.
    ///
    /// `true` the one tick it BECOMES complete, so the caller says so once
    /// rather than on every liveness check. A capture that stops being complete
    /// (a cell the player hovered away from, a re-read that lost the class)
    /// puts the loop straight back on the working cadence.
    pub fn note_complete(&mut self, complete: bool) -> bool {
        let became = complete && !self.complete;
        self.complete = complete;
        became
    }

    /// **Scan now** asked for another look at the captured window. `true` when
    /// this actually resumed a paused read, so the caller logs the transition
    /// and not every armed tick.
    ///
    /// One caller, and it is the gate's `FullDetect` step (`run_loop`). A VOICE
    /// LINE cannot reach here: `trigger::capture_held` drops a line arriving
    /// over a `live`/`done` capture before it can arm, and `trigger::
    /// disarm_probe` takes the slot away on every live tick for the line that
    /// races that check. So the only thing that resumes a paused read is a
    /// person pressing the button — which is the whole of what "scan a window
    /// that is already open" can mean.
    ///
    /// The miss counter is cleared WHEN THIS RESUMES SOMETHING, because that is
    /// the moment the cadence changes and [`RETIRE_AFTER`] counts ticks, not
    /// time. MEASURED 2026-08-26 (app.log 09:41:52 → 09:41:57), on the shape
    /// this replaced: one miss at the 10 s liveness cadence, an arm four
    /// seconds later, and the first re-detect at the 2 s cadence made two —
    /// "window gone" while the recruit window was on screen, restored seven
    /// seconds afterwards. Two misses are meant to be evidence of a window that
    /// closed; a miss counted before the cadence changed is not part of that
    /// evidence.
    ///
    /// The clear happens on the TRANSITION only, so a Scan now over a capture
    /// that is NOT paused leaves the misses alone. There they are the evidence:
    /// they were counted at the cadence still running, two of them mean the
    /// window closed, and zeroing them on each press would let a player leaning
    /// on the button hold a closed window's capture on screen indefinitely.
    pub fn resume(&mut self) -> bool {
        let was = self.complete;
        self.complete = false;
        if was {
            self.misses = 0;
        }
        was
    }

    /// Record how long a DETECT took — not the hover confirm that shares the
    /// iteration with it, which is why the loop times `detect_tick` alone and
    /// runs the hover before it.
    ///
    /// `full_frame` is the gate, not a label: a tick that OCR'd a crop of a
    /// known panel says nothing about what a full-screen hunt costs on this
    /// machine, in either direction, so it moves neither the backoff nor the
    /// decay run behind it. See [`SLOW_TICK`].
    ///
    /// `Some` on the ticks the cadence actually moves, so the caller logs each
    /// transition once and nothing in between.
    pub fn note_tick_duration(
        &mut self,
        full_frame: bool,
        took: Duration,
    ) -> Option<BackoffChange> {
        if !full_frame {
            return None;
        }
        if took > SLOW_TICK {
            self.fast_detects = 0;
            return (!std::mem::replace(&mut self.backed_off, true))
                .then_some(BackoffChange::BackedOff);
        }
        if !self.backed_off {
            return None;
        }
        self.fast_detects += 1;
        if self.fast_detects < BACKOFF_DECAY_DETECTS {
            return None;
        }
        self.backed_off = false;
        self.fast_detects = 0;
        Some(BackoffChange::Recovered)
    }
}

/// A move of the detect cadence, reported by [`LoopState::note_tick_duration`]
/// so the log carries both directions and neither is printed twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffChange {
    /// A slow detect put the hunt on [`DETECT_INTERVAL_SLOW`].
    BackedOff,
    /// [`BACKOFF_DECAY_DETECTS`] fast detects took it back off again.
    Recovered,
}

/// What one loop iteration does about the detect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectStep {
    /// Detect this iteration.
    Run,
    /// The cadence is not up yet.
    Wait,
    /// The cursor is inside a captured cell: the player is reading a tooltip,
    /// and the hover confirm is the work that matters. See [`detect_step`].
    HoldForConfirm,
}

/// Whether this iteration detects, and why not when it does not.
///
/// The hold is the latency fix of POE-204 WI-B. MEASURED 2026-08-26 (app.log
/// 09:40:06-09:40:57): confirms landed 4-10 s apart while the player hovered
/// cell after cell, because each one had to wait behind a re-detect of a panel
/// that had not moved — a full-screen grab and a full-screen OCR, 4.5 s on the
/// measured machine, for an answer the cursor already gave. A cursor inside a
/// captured cell is proof the window is on screen, so the detect it would
/// displace has nothing to add.
///
/// **A hold is not a miss.** It does not reach [`LoopState::on_detect`] at all,
/// so it cannot advance the retire counter — which matters, because the hold
/// fires exactly when a tooltip is up and that is the frame the detect fails on
/// (WI-A's phantom retire). Holding is the stronger version of that fix: the
/// frame is never taken.
///
/// **The hold has a ceiling**, and it is [`LIVENESS_INTERVAL`]. A cursor parked
/// on a cell must not stop the loop ever noticing a window that closed — the
/// player can alt-tab, or close the panel with the mouse still over where it
/// was — so once a whole liveness interval has passed since the last detect,
/// the detect runs whatever the cursor is doing. The `since_detect` clock is
/// NOT reset by a hold, which is what makes that ceiling arrive.
///
/// **An open occlusion run cancels the hold** (`occluded`, POE-204 WI-B
/// review). The hold rests on the cursor being proof the window is on screen;
/// once a detect has come back with no layout while the cursor was on the
/// panel, that proof is exactly what is in question, and holding turns the
/// answer into a 10 s cadence. Composed with [`OcclusionRun`]'s own cap the
/// arithmetic was: ceiling detect, then [`OCCLUDED_MAX`] of held ticks at 10 s
/// each, then two more 10 s ticks to retire — 40 s of `done` verdict for a
/// window that closed. Dropping the hold puts those ticks back on
/// [`REDETECT_INTERVAL`], which is the cadence the cap was sized against, and
/// the same close retires in 28 s. It costs one cropped re-detect every 2 s
/// while a tooltip is up, and only after one has already failed.
pub fn detect_step(
    state: &LoopState,
    cursor_on_cell: bool,
    occluded: bool,
    since_detect: Duration,
) -> DetectStep {
    if since_detect < state.detect_interval() {
        DetectStep::Wait
    } else if state.live && cursor_on_cell && !occluded && since_detect < LIVENESS_INTERVAL {
        DetectStep::HoldForConfirm
    } else {
        DetectStep::Run
    }
}

/// The rect a detect tick OCRs, or `None` for the whole screen.
///
/// One place answers "is there a known panel to re-read", rather than every
/// reader of `Session::crop` re-deriving it. The two fields are cleared
/// together on retire, so the filter looks redundant — it is not: the crop is
/// a rect measured from a layout, and using it while nothing is live would
/// hunt for a new window inside the last one's outline and never find one
/// anywhere else on screen.
pub fn detect_frame(crop: Option<[i32; 4]>, panel: Option<[i32; 4]>) -> Option<[i32; 4]> {
    crop.filter(|_| panel.is_some())
}

/// What one detect tick reports back to the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectTick {
    /// What it did to the capture state. `None` when the tick bailed on the
    /// stop signal after pass 1 — neither a hit nor a miss (see
    /// [`burst_satisfied`]).
    pub outcome: Option<DetectOutcome>,
    /// Whether this tick OCR'd the WHOLE screen and nothing else — the only
    /// kind of tick whose duration the backoff may read. A crop, and the
    /// `crop→full` re-take that carries two OCRs, are both excluded. See
    /// [`SLOW_TICK`].
    pub full_frame: bool,
}

impl DetectTick {
    /// What a PROBE reports, whatever it saw.
    ///
    /// `full_frame: false` is the load-bearing half: a band OCR says nothing
    /// about what a full-screen hunt costs on this machine, and the probe runs
    /// twice per voice line in an arena full of mercenaries — reporting `true`
    /// would decay a backoff the hunt earned, on evidence about a cheaper
    /// question, within seconds. `outcome: None` rather than
    /// [`DetectOutcome::Missed`]: a probe that saw no chrome has not detected
    /// anything and has not missed anything either.
    ///
    /// Named here rather than written inline at [`probe_tick`]'s two return
    /// sites so both say the same thing and a test can read the claim.
    pub fn probe() -> Self {
        Self { outcome: None, full_frame: false }
    }
}

/// Whether the cursor was over a rect, given THIS frame's rect and the one the
/// last detect left on the session.
///
/// Both, because either is the wrong answer on its own. This frame's rect is
/// missing entirely on the occlusion path — a detect that found no layout has
/// nothing to measure — and it is SHRUNKEN when the detect found only part of
/// the panel: a tooltip over rows 3-6 leaves a two-row layout whose rect stops
/// above the very cursor that caused it, so keying on this frame alone reads
/// "cursor off the panel" at the exact moment the cursor is provably on it.
/// The session's rect is the last frame that saw the whole panel, and the
/// window does not move while it is open, so the union of the two is the
/// honest answer.
///
/// The union is safe in the direction that matters: the session's rect is
/// cleared on retire, so it cannot vote for a window that closed.
pub fn cursor_on_panel(
    this: Option<[i32; 4]>,
    last: Option<[i32; 4]>,
    cursor: Option<(i32, i32)>,
) -> bool {
    let Some(c) = cursor else {
        return false;
    };
    [this, last]
        .into_iter()
        .flatten()
        .any(|rect| geometry::contains(rect, c))
}

/// The header a frame is allowed to publish, given where the cursor was when
/// it was grabbed.
///
/// A cursor on the panel means the game has drawn a tooltip there — it opens
/// one nowhere else — and a tooltip's lines land in the header band set TALLER
/// than the title, which is exactly how `geometry::parse_header` ranks
/// candidates. MEASURED 2026-08-26 (app.log 09:41:09): `SUPPORTED SKILLS
/// PENETRATE 100/GlRE` became the mercenary's name and went to GGG as a trade
/// query's label.
///
/// So an occluded frame's name and class are WITHHELD rather than published.
/// The rows and cells it read are still good — the tooltip covers a corner of
/// the panel, not the grid — but its header is a read of the wrong pixels, and
/// `None` is precisely what [`super::read::merge_header`] treats as "not read
/// this tick": the last clean frame's header stands, and a FIRST detect under a
/// tooltip publishes no name rather than the tooltip's.
///
/// The zone is [`super::geometry::header_guard_bounds`], NOT the occlusion
/// rect: the question here is whether a tooltip could have landed lines in the
/// header band, and a cursor down on the footer — three pitches below the last
/// row, which the occlusion rect deliberately covers — is nowhere near it. The
/// two rects are separate functions so that widening the occlusion reach can
/// never silently widen this one.
///
/// A FIRST read taken entirely under a tooltip therefore publishes no name at
/// all, and that is the intended outcome rather than a gap: `read::
/// header_complete` refuses the capture, no trade session opens, and the loop
/// keeps reading until a clean frame supplies the name. The diagnostic is
/// [`header_log_line`], which prints `name ?` for exactly that state.
///
/// The withholding is NOT conditional on there being an older name to protect.
/// Making it overwrite-only — publish the tooltip's line when the header is
/// still empty, withhold it only when a good name already exists — would hand
/// the corrupt read the one case it actually caused damage in: 2026-08-26's
/// `SUPPORTED SKILLS PENETRATE 100/GlRE` WAS the first name the capture had.
///
/// The shape test in `geometry::is_name_shaped` and this rule catch the same
/// bug from opposite ends and neither subsumes the other: a tooltip line that
/// happens to look like a name gets through the shape test, and a tooltip the
/// cursor has already left (the frame after the player moved on) gets through
/// this one.
///
/// The level and the wager stay. They are digits behind their own labels: the
/// tooltip either covers them, in which case they are `None` already, or it
/// does not. Dropping the level would also blind
/// [`super::read::panel_replaced`] to a REMATCH, which is the one thing that
/// must never be missed.
pub fn publishable_header(header: MercHeader, cursor_in_header_guard: bool) -> MercHeader {
    if cursor_in_header_guard {
        MercHeader { name: None, class: None, ..header }
    } else {
        header
    }
}

/// The header a detected frame may publish, and the header-guard rect it
/// leaves for the next frame.
///
/// The rect CHOICE lives here rather than at the call site, which is the whole
/// point of the extraction (WI-A review carry-over). Two rects come off one
/// layout — [`geometry::panel_bounds`] reaches over the footer so a cursor on
/// TAKE ITEM holds the capture, [`geometry::header_guard_bounds`] stops a pitch
/// below the last row because that is as far as a tooltip can be and still put
/// lines in the header band — and handing the occlusion rect to
/// [`publishable_header`] would throw away every clean header read taken while
/// the player's cursor rests on the button that ends the window. The two rects
/// are one `bounds` call apart and the miswiring is invisible at a call site;
/// inside a function it is one test.
///
/// `last_guard` is the previous detect's rect. Both count, for the reason
/// [`cursor_on_panel`] states: a tooltip over the lower rows shrinks THIS
/// frame's rect above the very cursor that caused it.
pub fn publishable_header_for(
    layout: &geometry::MercLayout,
    g: &MercGeometry,
    last_guard: Option<[i32; 4]>,
    cursor: Option<(i32, i32)>,
    header: MercHeader,
) -> (MercHeader, Option<[i32; 4]>) {
    let guard = geometry::header_guard_bounds(layout, g);
    (
        publishable_header(header, cursor_on_panel(guard, last_guard, cursor)),
        guard,
    )
}

/// The log line for a header parse, or `None` when it would repeat `last`.
///
/// The three fields the strip prints, and the three the corruption of
/// 2026-08-26 was invisible in: the only trace `SUPPORTED SKILLS PENETRATE
/// 100/GlRE` left in the log was a trade error, minutes after the parse that
/// produced it. An unread field prints as `?` rather than being omitted, so the
/// line has a fixed shape and "the class was not read" and "the class read as
/// nothing" are the same visible claim.
///
/// Gated on the RENDERED line, not on the header value: the header also carries
/// the wager, which this does not print, and a wager that changed alone would
/// otherwise reprint an identical line.
pub fn header_log_line(header: &MercHeader, last: &Option<String>) -> Option<String> {
    let line = format!(
        "Merc: header — name {}, class {}, lvl {}",
        header.name.as_deref().unwrap_or("?"),
        header.class.as_deref().unwrap_or("?"),
        header.level.map_or_else(|| "?".to_string(), |l| l.to_string()),
    );
    (last.as_deref() != Some(line.as_str())).then_some(line)
}

/// What the loop does with one iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopStep {
    /// The game is not the foreground window — nothing on screen to read.
    Unfocused,
    /// The game is in front, and nothing has asked for a scan.
    Idle,
    /// The gate is armed but its next probe is not due yet.
    ///
    /// A third state rather than a second `Idle`, and it exists for one number:
    /// [`IDLE_NAP`] is 250 ms, so a probe due at [`trigger::PROBE_DELAY_MS`]
    /// would be served at the next 250 ms boundary — a 500 ms probe fired at
    /// 750 ms, half again late on a gate whose whole design is two deadlines.
    /// Waiting naps [`TICK`] instead.
    Waiting,
    /// Look at the screen this iteration.
    Work,
}

/// Whether this iteration does any screen work, and why not when it does not.
///
/// The three negative answers are deliberately different states rather than one
/// "skip": they nap for different lengths (a focus check can wait a second, an
/// armed gate cannot wait a quarter of one), and they are the whole of
/// POE-198's promise — no OCR runs unless the gate asked or a capture is
/// already live. A predicate rather than an `if` chain in the loop body so that
/// promise is testable without a screen.
///
/// A LIVE capture works whatever the gate says: retirement takes two misses and
/// the liveness check is a detect like any other, so dropping to Idle under a
/// resting gate would strand a window on screen for good.
pub fn next_step(live: bool, gate: trigger::GateStep, focused: bool) -> LoopStep {
    if !focused {
        LoopStep::Unfocused
    } else if live
        || matches!(gate, trigger::GateStep::Probe | trigger::GateStep::FullDetect)
    {
        LoopStep::Work
    } else if gate == trigger::GateStep::Waiting {
        LoopStep::Waiting
    } else {
        LoopStep::Idle
    }
}

/// What a working iteration points at the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Nothing this iteration — the live cadence is not up.
    None,
    /// The gate's cheap anchor band ([`probe_tick`]).
    Probe,
    /// A full detect ([`detect_tick`]).
    Detect,
}

/// Which of the two looks this iteration takes, if either.
///
/// The precedence is what keeps the probe cheap. Read top to bottom:
///
/// - **Scan now first.** A person asked for a full detect; the band probe could
///   only turn their answer into a stand-down, and the full detect answers the
///   probe's question as a side effect.
/// - **A live capture next, on its own cadence.** [`detect_step`] owns that
///   cadence and its hold, and a gate armed over a live window (which
///   `trigger::capture_held` makes rare, not impossible — a line can land in
///   the gap between the arm and the detect that captures) must not displace a
///   re-detect with a probe that has nothing to add.
/// - **Then the probe.** This is the only branch that reaches [`probe_tick`],
///   which is what makes "a voice line costs one band OCR" a property of one
///   function rather than of the loop body's shape.
pub fn look_step(live: bool, gate: trigger::GateStep, detect: DetectStep) -> Look {
    if gate == trigger::GateStep::FullDetect {
        Look::Detect
    } else if live {
        if detect == DetectStep::Run {
            Look::Detect
        } else {
            Look::None
        }
    } else if gate == trigger::GateStep::Probe {
        Look::Probe
    } else {
        Look::None
    }
}

/// Whether a detect tick's outcome means the gate found what it was armed for.
///
/// NOT `LoopState::live`, which is still true after the first of the two misses
/// that retire a capture: a gate armed for a SECOND mercenary while the first
/// one's window was still on screen would disarm itself on a tick that found
/// nothing. `None` is a tick that bailed on the stop signal without detecting —
/// and it is also what a probe that saw no chrome reports, which is the same
/// claim: nothing was detected and nothing was missed.
pub fn burst_satisfied(outcome: Option<DetectOutcome>) -> bool {
    matches!(
        outcome,
        Some(DetectOutcome::Captured) | Some(DetectOutcome::Refreshed)
    )
}

/// The rect one probe OCRs.
///
/// `remembered` is [`geometry::probe_band_bounds`] of the last panel this
/// SESSION saw — not `Session::panel`, which a retire clears. A window that was
/// on screen a minute ago is overwhelmingly likely to reopen where it was, and
/// remembering the band is what makes the ordinary probe 7% of a screen instead
/// of 40%.
///
/// **The retry drops it.** `attempt` 0 uses the remembered band; every later
/// one uses [`geometry::default_probe_band`]. The remembered band has exactly
/// one failure mode — the player moved the window, or changed the UI scale —
/// and it is silent: the probe looks at empty screen, sees no chrome, and the
/// gate stands down on a window that is plainly open. The retry exists to cover
/// lag, and covering a moved window with it costs nothing that was not already
/// being spent.
///
/// `attempt` is [`trigger::BurstGate::looks`] — probes spent by this ARMING,
/// across every voice line that re-armed it — and not the current line's own
/// count. A new line resets that one, so keying the band on it would let a
/// mercenary who talks every second hold the probe on the remembered rect for
/// the whole of the gate's life, which is precisely the case whose window is
/// most likely to have moved.
///
/// **A band from a different SCREEN is dropped outright**, not left to the
/// retry. The band survives retires by design and so it survives a resolution
/// change too, and a rect past the new screen's edge is not merely a bad guess:
/// `crop_imm` clamps it to nothing, `recognize_lines` fails on the empty image,
/// and the player gets an OCR error on the slice for walking past a mercenary
/// after changing their display settings. One `encloses` is cheaper than that
/// diagnosis.
pub fn probe_band(remembered: Option<[i32; 4]>, screen: [u32; 2], attempt: u32) -> [i32; 4] {
    remembered
        .filter(|_| attempt == 0)
        .filter(|band| geometry::encloses([0, 0, screen[0] as i32, screen[1] as i32], *band))
        .unwrap_or_else(|| geometry::default_probe_band(screen))
}

/// How many times a cell that ALREADY reads as `Matched` may be re-OCR'd by the
/// hover tick, per capture.
///
/// Three, not one: the first read can land while the tooltip is still fading in
/// and the useful confirmation comes a tick later. Not unbounded, because a
/// cursor parked on a matched cell would otherwise buy a full screen grab plus
/// an OCR every 400 ms for as long as the player leaves it there — the cost the
/// completed-capture pause exists to remove.
pub const MATCHED_HOVER_ATTEMPTS: u8 = 3;

/// The per-cell hover budget (2026-08-25).
///
/// The tick keeps running over a completed capture so a hover can still CORRECT
/// a confident wrong read — a matched cell is the module's opinion, and the
/// tooltip is the game's. What is bounded is how often that opinion is
/// re-litigated, per `(row key, slot)` so a budget spent on one cell never
/// silences another.
///
/// The states are treated differently on purpose:
///
/// - `Confirmed` — the user already told us; nothing to buy;
/// - `Matched` — re-readable, up to [`MATCHED_HOVER_ATTEMPTS`];
/// - everything else — unbounded, because those are the cells the hover exists
///   for and the player is looking at one BECAUSE the strip asked them to.
///
/// Keyed on `row_key` (the skill's vocabulary id) rather than the row index, so
/// it survives a re-detect that renumbers the rows — the same key
/// `Session::confirmed` uses.
#[derive(Debug, Default)]
pub struct HoverBudget {
    spent: HashMap<(String, u8), u8>,
}

impl HoverBudget {
    /// Whether this hover may run the tooltip OCR — and charge it if it may.
    pub fn take(&mut self, cell: (String, u8), state: ReadState) -> bool {
        match state {
            ReadState::Confirmed => false,
            ReadState::Matched => {
                let spent = self.spent.entry(cell).or_insert(0);
                if *spent >= MATCHED_HOVER_ATTEMPTS {
                    return false;
                }
                *spent += 1;
                true
            }
            _ => true,
        }
    }

    /// Forget every charge. Called when the capture this budget was counted
    /// against is gone — retired, replaced, or invalidated by a template
    /// forget/reset, which is exactly when a re-read becomes worth buying
    /// again.
    pub fn clear(&mut self) {
        self.spent.clear();
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
/// A serialised, off-tick writer.
///
/// `TemplateStore::save` writes one PNG per sample plus the index, and it used
/// to run inline on the hover confirm — the read the player is waiting on. The
/// first Windows smoke measured a 4 s tick, and every confirm paid a whole
/// store rewrite into it.
///
/// One worker thread and one channel, so two confirms landing back to back
/// cannot write the same directory at once: the second request waits for the
/// first write to finish rather than racing it. A burst of requests coalesces
/// into one write, which is correct because `save` writes the WHOLE store —
/// the write that follows the last request has the last request's state in it.
///
/// The worker exits when the queue is dropped, which is what the loop's own
/// exit does; a request already in the channel is still written.
pub struct SaveQueue {
    tx: std::sync::mpsc::Sender<()>,
}

impl SaveQueue {
    /// Spawn the worker. `save` runs on it, never on the caller's thread.
    pub fn spawn(mut save: impl FnMut() + Send + 'static) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            while rx.recv().is_ok() {
                // Coalesce the burst: the write is of the whole store, so one
                // after the last request says everything the queued ones did.
                while rx.try_recv().is_ok() {}
                // A PANICKING save must not take the queue with it. `save`
                // walks learned art into an image encoder and a JSON
                // serialiser; if one of those ever panics on a particular
                // sample, an uncaught unwind ends this thread, every later
                // `request` finds a dead channel, and the store silently stops
                // reaching disk for the rest of the session — over a confirm
                // the player watched succeed. Caught, the same sample fails
                // again next time and everything else is still written. The
                // panic message is already on stderr and in the app's own
                // panic hook; this thread has no handle to log through.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut save));
            }
        });
        Self { tx }
    }

    /// Ask for a write and RETURN — the point of the type.
    ///
    /// `false` when the worker is gone, which is the one thing the caller can
    /// usefully know: the store is still correct in memory and the next start
    /// reloads what is on disk, but nothing this session confirms will be
    /// written, and that is worth saying once. The caller says it — through
    /// [`OnceLog`], because a hover confirm repeats.
    #[must_use]
    pub fn request(&self) -> bool {
        self.tx.send(()).is_ok()
    }
}

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
            if geometry::contains(cell.rect, cursor) {
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
/// The status a CAPTURED window is in, from the loop's own state.
///
/// One owner for the `live` / `done` choice, because three places make it — the
/// detect that publishes a capture, the burst-expiry reconciliation, and any
/// future path that has to restate the status of a window already on screen. A
/// second copy is how a strip ends up saying `scanning` over a window it
/// finished reading.
pub fn live_status(complete: bool) -> MercStatus {
    if complete {
        MercStatus::Done
    } else {
        MercStatus::Live
    }
}

/// Publish `Scanning` together with the mercenary the burst heard.
///
/// The reconciliation path's counterpart to the arming site's own publish.
/// [`trigger::scan_outranks`] lets that site announce over `Idle` and nothing
/// else, so a gate armed while the slice held any other status is announced
/// HERE instead — on the first tick after the slice falls back to a waiting
/// status, with the gate still armed and still owed its probes.
///
/// The name is read off the GATE rather than remembered here, which is why it
/// is an argument and not a field: a speaker kept on this side would outlive
/// the line it belongs to and the strip would go on naming a mercenary the
/// module stopped looking for.
pub fn publish_scanning(app: &AppHandle, speaker: Option<String>) {
    publish(app, |slice| {
        slice.status = MercStatus::Scanning;
        slice.burst_speaker = speaker;
    });
}

/// Publish a status, keeping the burst speaker attached to the one status it
/// qualifies.
///
/// `burst_speaker` names who the module HEARD, which is only a statement about
/// a scan that is running. Clearing it anywhere else would leave the strip
/// saying "heard Fennik…" over a window it is already reading, or over nothing
/// at all.
pub fn publish_status(app: &AppHandle, want: MercStatus) {
    publish(app, |slice| {
        slice.status = want;
        if want != MercStatus::Scanning {
            slice.burst_speaker = None;
        }
    });
}

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

/// What a retired capture left behind for a re-detect of the same panel.
///
/// One slot, replaced by each retire. The 2026-08-25 smoke is the whole reason
/// it exists: a tooltip retires the capture, the mercenary's voice line re-arms
/// a burst two seconds later, and the SAME window is detected fresh — so
/// without this the player's confirmations are gone every time they hover.
struct Retained {
    /// The capture as last published. Its header and named-skill set are the
    /// key: `read::same_panel_positive` weighs them against the next detect.
    capture: MercCapture,
    confirmed: HashMap<(String, u8), ConfirmedCell>,
    /// Carried with the confirmations so a cell that already spent its re-read
    /// budget does not get a fresh one out of the retire.
    hover_budget: HoverBudget,
    at: Instant,
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
    /// How much tooltip OCR each already-read cell has been allowed.
    hover_budget: HoverBudget,
    /// The template-store generation this session's `confirmed` map agrees
    /// with. See [`generation_changed`].
    template_generation: u64,
    /// The off-tick writer for the template store. `None` when there is no directory
    /// to write to. See [`SaveQueue`].
    saves: Option<SaveQueue>,
    /// Whether the first clean miss of this focus session has been logged.
    /// Reset when the game loses focus, so each return to the game says once
    /// what the loop saw.
    miss_logged: bool,
    /// The live capture's panel rect in screen px, from the layout that
    /// produced it. `None` when nothing is captured. See [`miss_kind`].
    panel: Option<[i32; 4]>,
    /// The live capture's HEADER-guard rect — the same grid without the footer
    /// band. A separate field rather than a shrink of [`Self::panel`] because
    /// the two are built from different footer reaches and only the layout
    /// knows the pitch. See [`super::geometry::header_guard_bounds`] and
    /// [`publishable_header`]. Cleared with `panel` on retire.
    header_guard: Option<[i32; 4]>,
    /// The rect the next re-detect grabs and OCRs, from
    /// [`geometry::crop_around`]. `None` — no known panel, so the next
    /// detect takes the whole screen. Cleared with `panel` on retire.
    crop: Option<[i32; 4]>,
    /// The band the next PROBE OCRs, from [`geometry::probe_band_bounds`].
    ///
    /// Deliberately NOT cleared on retire, unlike every other rect on this
    /// struct. The three above are statements about a capture the loop is
    /// holding and must not outlive it; this one is a statement about where
    /// recruit windows appear on this player's screen, and its whole value is
    /// that it survives the window it was measured on — the probe that uses it
    /// runs when nothing is captured, by construction. See [`probe_band`].
    probe_band: Option<[i32; 4]>,
    /// The open run of detects the panel was covered for. See [`OcclusionRun`].
    occlusion: OcclusionRun,
    /// The header line as last LOGGED — the once-per-change gate for
    /// [`header_log_line`], so a re-detect does not reprint the same three
    /// fields every cadence.
    header_logged: Option<String>,
    /// What the last retire left for a re-detect of the same panel.
    retained: Option<Retained>,
    /// The trade-search budget for the capture on screen (POE-202).
    ///
    /// `Some` from the tick a capture becomes COMPLETE until the tick it
    /// retires, which is what makes the 3-search ceiling a per-mercenary
    /// budget rather than a per-app-run one. A re-detected window opens a new
    /// session and gets a new budget; the result cache is what stops it paying
    /// twice for the same question.
    trade: Option<MercTradeSession>,
    /// Bumped by the detect tick and by a hover confirmation — the two writes
    /// of [`Self::current`] that outlive the iteration that made them.
    /// (`restore_retained` also writes it, mid-detect, and is overwritten by
    /// the same tick's own write a few lines later.)
    ///
    /// A version, not a change detector: `MercCapture` carries
    /// `captured_at_ms`, so two equal readings of one panel are never equal
    /// values, and there is nothing cheaper than the trade query itself to
    /// compare. What it buys is the difference between rebuilding that query
    /// once per detect (1 Hz, or 0.1 Hz once the capture settles) and
    /// rebuilding it on every 100 ms trade tick — see `search::tick`.
    revision: u64,
}

/// What a store purge says in the log.
///
/// The version the old index declared, not a hard-coded "format 1": that is
/// the case every current install hits, but a downgrade meets a later index
/// and a half-written one parses as nothing, and the three have different
/// causes. A line that named the wrong one would send the next reader after
/// an upgrade that never happened.
fn purge_log_line(purged: &super::icons::PurgedStore) -> String {
    match purged.version {
        Some(version) => format!(
            "Merc: dropped {} format-{version} template(s) (format {})",
            purged.dropped,
            sync::FORMAT_VERSION,
        ),
        None => format!(
            "Merc: dropped an unreadable template index (format {})",
            sync::FORMAT_VERSION,
        ),
    }
}

/// What the icon matcher is running with, and whether the user has moved it
/// off the numbers format 2 was measured on (POE-207).
///
/// The first line always goes to the log: when a cell reads `?` the first
/// question is which thresholds were in force, and a log that only says so
/// when they are unusual makes the usual case unanswerable.
///
/// The warnings exist because `merc-geometry.json` is the user's file and
/// stays the user's — nothing here clamps or overrides it. But two of its
/// blocks silently un-measure the matcher. The thresholds ARE the measurement
/// (0.88/0.78/0.05 was attainable only after the format-2 derivation).
///
/// And `cellSize`/`cellInset` decide the inner crop the alignment window is
/// cut out of. At the live scale 0.974 the cell is 43 px, the default inset 2
/// leaves 39 px of inner crop, and the ±3 px margin leaves a 33 px window —
/// which is the window every pooled signature was derived from. Move either
/// number and the derivation moves with it: at `cellInset` 6 the window is
/// 25 px, still aligned but no longer the same signature as the pool's, so
/// every shared template stops matching. At 7 it is 23 px, under `SIG_DIM`,
/// and `icons::shift_window` gives up the alignment entirely. Both read as
/// "the matcher stopped working" rather than as "I edited a geometry file".
fn matcher_geometry_warnings(g: &MercGeometry) -> Vec<String> {
    let d = MercGeometry::default();
    let (t, dt) = (&g.thresholds, &d.thresholds);
    let mut out = vec![format!(
        "Merc: icon thresholds match {:.2} / low {:.2} / lead {:.2}",
        t.icon_match, t.icon_low, t.icon_lead,
    )];
    if t.icon_match != dt.icon_match || t.icon_low != dt.icon_low || t.icon_lead != dt.icon_lead {
        out.push(format!(
            "Merc: icon thresholds overridden — format {} was measured at {:.2}/{:.2}/{:.2}",
            sync::FORMAT_VERSION, dt.icon_match, dt.icon_low, dt.icon_lead,
        ));
    }
    if g.cell_size != d.cell_size || g.cell_inset != d.cell_inset {
        out.push(format!(
            "Merc: cell geometry overridden (size {:.1} / inset {:.1}) — the ±{} px alignment \
             window is cut out of size − 2·inset, measured at {:.1}/{:.1}",
            g.cell_size,
            g.cell_inset,
            super::icons::SHIFT_MAX,
            d.cell_size,
            d.cell_inset,
        ));
    }
    out
}

fn run_loop(app: AppHandle, cancel: watch::Receiver<bool>) {
    crate::app_log(&app, "Merc: capture loop started".to_string());
    crate::report_ocr_engine(&app);

    // The gate outlives the loop — it is `AppState`, and the module can be
    // switched off and on again inside one session. Whatever it was holding
    // when the last loop stopped is about a screen from before the module was
    // off: an armed probe would fire on this loop's first tick for a window
    // that closed minutes ago, and a Scan now would be served long after the
    // player stopped waiting for it. A fresh loop starts owing nothing.
    trigger::disarm(&app);

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
    for warning in matcher_geometry_warnings(&geometry) {
        crate::app_log(&app, warning);
    }

    // Load the learned templates before the first detect, so a restart does not
    // re-report every already-confirmed cell as unknown.
    let icons_dir = data_dir.as_ref().map(|d| d.join(super::ICONS_DIR));
    let mut template_problems = Vec::new();
    if let Some(dir) = &icons_dir {
        // THE PURGE COMES FIRST (POE-207). A store written by format 1 holds
        // luma signatures this build cannot correlate against anything, and
        // both the pull below and the load after it would otherwise merge into
        // it. Dropping it here — before the session begins, so the pull's ETag
        // is asked for on an empty store — is what makes the format bump a
        // clean restart rather than a poisoned mix.
        let purged = {
            let state = app.state::<AppState>();
            super::icons::writing_icons_dir(&state.merc_icons_write, || {
                super::icons::purge_stale_store(dir)
            })
        };
        if let Some(purged) = purged {
            crate::app_log(&app, purge_log_line(&purged));
        }

        // Ask the shared pool BEFORE reading the disk, so the round-trip
        // overlaps the load instead of following it (POE-201). One pull per
        // module start, single-flight inside `spawn_pull`.
        sync::begin_session(&app);
        sync::spawn_pull(&app);

        let (store, problems) = TemplateStore::load(dir);
        template_problems = problems;
        let loaded = store.len();

        // INSTALL FIRST, MERGE SECOND. The whole-store write happens here, while
        // the seam still holds its pull claim, and every merge — this start's or
        // a later task's — then runs against the INSTALLED store under its
        // mutex. Merging into the local copy first and installing afterwards is
        // what made this seam a second writer: a corpus that landed in the gap
        // was saved to disk and then erased by the assignment below, taking its
        // ETag with it (the next pull answering 304 for art the store no longer
        // held).
        {
            let state = app.state::<AppState>();
            *state.merc_templates.lock().unwrap_or_else(|e| e.into_inner()) = store;
        }
        crate::app_log(&app, format!("Merc: {loaded} learned templates loaded"));

        // Bounded, and it releases the seam claim whether or not a corpus
        // arrives — after this line a later corpus applies itself.
        if let Some((corpus, etag)) = sync::wait_for_pull(&app, &cancel) {
            sync::apply_corpus(&app, dir, &corpus, etag, &geometry.thresholds, false);
        }
        let pooled_samples = {
            let state = app.state::<AppState>();
            let store = state.merc_templates.lock().unwrap_or_else(|e| e.into_inner());
            store.pooled_samples()
        };
        sync::set_pooled_samples(&app, pooled_samples);
        if pooled_samples > 0 {
            crate::app_log(
                &app,
                format!("Merc: {pooled_samples} of them came from the shared pool"),
            );
        }
    }
    for problem in &template_problems {
        crate::app_log(&app, format!("Merc: template store — {problem}"));
    }

    let learned = learned_keys(&app);
    let pooled = pooled_keys(&app);
    let source = geometry_source.to_string();
    publish(&app, |slice| {
        slice.status = MercStatus::Idle;
        slice.geometry_source = source;
        slice.learned_families = learned;
        slice.pooled_families = pooled;
        slice.last_error = geometry_err;
    });

    // Anything hover-learned that the pool has not seen — including a whole
    // store learned before the pool existed. Off-tick like every other upload.
    sync::enqueue_backfill(&app);
    // And every forget whose tombstone never landed. Without this a POST that
    // failed three times is never retried: the key stays suppressed on this
    // device for good, and each start re-downloads the corpus and re-runs the
    // tombstone replace it can never finish.
    sync::retry_pending_tombstones(&app);

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
        hover_budget: HoverBudget::default(),
        template_generation: template_generation(&app),
        saves: icons_dir.clone().map(|dir| {
            let app = app.clone();
            // The worker's own once-sink. A store directory that cannot be
            // written fails the same way on every confirm, and this thread has
            // no `Session` to reach the loop's [`OnceLog`] through.
            let mut errors = OnceLog::default();
            SaveQueue::spawn(move || {
                let state = app.state::<AppState>();
                // SNAPSHOT under the store's mutex, WRITE outside it: the
                // detect tick's `match_family` runs against the same mutex, and
                // holding it across the PNG writes would move the stall rather
                // than remove it. Both halves sit inside the DIRECTORY lock,
                // which is what stops `sync`'s writes interleaving with this
                // one — and what stops the snapshot going stale between being
                // taken and being written. See [`icons::writing_icons_dir`].
                let result = super::icons::writing_icons_dir(&state.merc_icons_write, || {
                    let snapshot = {
                        let store =
                            state.merc_templates.lock().unwrap_or_else(|e| e.into_inner());
                        store.clone()
                    };
                    snapshot.save(&dir)
                });
                if let Err(e) = result {
                    let msg = format!("Merc: template store save failed — {e}");
                    if let Some(line) = errors.admit(&msg) {
                        crate::app_log(&app, line);
                    }
                    publish(&app, |slice| slice.last_error = Some(msg));
                }
            })
        }),
        miss_logged: false,
        panel: None,
        header_guard: None,
        crop: None,
        probe_band: None,
        occlusion: OcclusionRun::default(),
        header_logged: None,
        retained: None,
        trade: None,
        revision: 0,
    };

    // Backdated so the first iteration detects immediately rather than after a
    // full cadence of doing nothing.
    let mut last_detect = Instant::now() - DETECT_INTERVAL_SLOW;
    let mut last_hover = Instant::now() - HOVER_INTERVAL;

    loop {
        if *cancel.borrow() {
            break;
        }

        // Gate bookkeeping runs BEFORE the focus gate: a gate armed while the
        // player is reading this app must still give up on its own schedule,
        // and the badge must say "scanning" while it waits for the game to come
        // back — that wait is the alt-tab case the trigger exists to cover.
        let now = now_ms();
        // A LIVE capture takes the voice-line slot away first. `capture_held`
        // keeps a line from arming over a window on screen, but it reads the
        // PUBLISHED status from the log-watcher thread — so a line landing
        // between the detect that captures and the publish that says so arms a
        // gate this loop will never probe (a live re-detect outranks the probe,
        // `look_step`), and ten seconds later that gate would print a
        // stand-down for a window the player is looking at. The Scan now slot
        // survives: its grace is a promise to the player, and `take_stood_down`
        // below is what still reports it.
        if session.state.live {
            trigger::disarm_probe(&app);
        }
        if let Some(stood_down) = trigger::take_stood_down(&app, now) {
            crate::app_log(&app, stood_down.line());
            // A Scan now can give up while a window is on screen. The
            // waiting-status reconciliation below only runs when nothing is
            // live, so without this the slice would keep whatever the gate left
            // behind and the strip would go on reporting a scan that is over.
            if session.state.live {
                publish_status(&app, live_status(session.state.complete));
            }
        }
        let gate = trigger::step(&app, now);
        // Scan now over a fully-read window means the player believes something
        // changed on screen that the paused loop would not have looked for —
        // they recruited, or rematched. Resuming here rather than in the detect
        // keeps the pause a property of the STATE machine, which is where its
        // cadence is decided.
        //
        // A VOICE LINE no longer reaches this: `trigger::capture_held` drops it
        // before it can arm (rule 5). MEASURED 2026-08-26 09:41:52-09:41:57 —
        // the arm resumed the read, the resumed cadence spent its first tick on
        // a frame a tooltip was covering, and "window gone" was published over
        // a window that was plainly open.
        if gate == trigger::GateStep::FullDetect && session.state.resume() {
            crate::app_log(
                &app,
                "Merc: Scan now over a completed capture — OCR resumed".to_string(),
            );
        }
        if !session.state.live {
            // Read before writing: `publish` clones the whole slice to tell
            // whether anything changed, and this runs several times a second in
            // the state where the module is supposed to be doing nothing.
            let want = if gate == trigger::GateStep::Resting {
                MercStatus::Idle
            } else {
                MercStatus::Scanning
            };
            if status(&app) != want {
                if want == MercStatus::Scanning {
                    // Off the GATE, not remembered: this branch is reached
                    // after a retire cleared the speaker, and the line that is
                    // still armed is the one that knows whose voice it was.
                    publish_scanning(&app, trigger::speaker(&app));
                } else {
                    publish_status(&app, want);
                }
            }
        }

        // The trade tick, BEFORE the focus gate and on every iteration.
        //
        // Before the gate because the results are read in THIS app: the player
        // alt-tabs to the Mercenaries page to look at them, which is exactly
        // when `game_focused` goes false — a tick behind the gate would stall
        // the debounce for as long as the user was reading. Every iteration
        // because that is what makes the debounce work without a timer; see
        // `search::tick`, which does nothing at all until the query has
        // actually moved.
        //
        // Gated on COMPLETE, not merely on an open session: `LoopState::resume`
        // puts a fully-read capture back on the working cadence when a new
        // burst arms over it, and the re-read that follows passes through
        // half-filled rows. A query built from one of those describes a
        // mercenary nobody has, and it would cost one of three searches to
        // learn that.
        if session.state.complete {
            if let (Some(capture), Some(trade)) =
                (session.current.as_ref(), session.trade.as_mut())
            {
                search::tick(&app, capture, &session.vocab, trade, session.revision, now);
            }
        }

        match next_step(session.state.live, gate, game_focused(&app)) {
            // No capture while alt-tabbed: the recruit window is not on screen,
            // and a full-screen OCR every second would be pure heat.
            LoopStep::Unfocused => {
                session.miss_logged = false;
                session.occlusion.on_focus_lost();
                if !nap(&cancel, UNFOCUSED_NAP) {
                    break;
                }
                continue;
            }
            // Nothing on screen and nobody asking: no grab, no OCR. This is the
            // module's resting state, and it is the whole point of POE-198.
            LoopStep::Idle => {
                session.miss_logged = false;
                if !nap(&cancel, IDLE_NAP) {
                    break;
                }
                continue;
            }
            // A voice line is armed and its probe is not due yet. One quantum,
            // not an idle nap: see [`LoopStep::Waiting`].
            LoopStep::Waiting => {
                session.miss_logged = false;
                if !nap(&cancel, TICK) {
                    break;
                }
                continue;
            }
            LoopStep::Work => {}
        }

        // ONE cursor read per iteration, and it happens FIRST — the claim is
        // now true (POE-204 WI-B review: `detect_tick` used to take a second
        // read of its own). THREE readers depend on it: the hover confirm, the
        // detect gate below, and the detect's own header-withholding rule,
        // which asks where the cursor was while the frame was taken. Reading it
        // twice inside one iteration lets them answer about different
        // positions.
        //
        // Only on the iterations one of them actually asks, so the loop's
        // 100 ms quantum does not turn into a 10 Hz cursor poll. NOT gated on a
        // live capture, unlike the hover: the FIRST detect of a window is
        // exactly the one whose header a tooltip corrupts (2026-08-26), and
        // that read needs the cursor with nothing captured yet.
        let hover_due = session.state.live && last_hover.elapsed() >= HOVER_INTERVAL;
        // The gate's two asks count as "due" alongside the live cadence. A Scan
        // now over a PAUSED capture is the case that needs saying: its detect
        // runs off the gate rather than off `detect_interval`, so keying the
        // cursor read on the cadence alone would hand that detect a `None`
        // cursor and switch the header-withholding rule off for exactly the
        // frame a player is most likely to have a tooltip open on.
        let detect_due = last_detect.elapsed() >= session.state.detect_interval()
            || matches!(gate, trigger::GateStep::Probe | trigger::GateStep::FullDetect);
        let cursor = if hover_due || detect_due {
            read_cursor(&app, &mut session)
        } else {
            None
        };
        // An unreadable cursor is not an excuse: no evidence the player is
        // confirming means the detect runs, exactly as `cursor_on_panel`
        // refuses to excuse a miss it cannot prove.
        let on_cell = match (session.current.as_ref(), cursor) {
            (Some(capture), Some(c)) => cell_at(capture, c).is_some(),
            _ => false,
        };

        // HOVER BEFORE DETECT. It used to run after, so a confirm got at most
        // one slot per detect and the detect was costing 4.5 s — measured
        // 2026-08-26, confirms 4-10 s apart while the player hovered cell
        // after cell. The hover is the read the player is waiting on; the
        // detect is a question the cursor has already answered.
        //
        // It keeps running over a COMPLETED capture, unlike the detect: "every
        // cell was read" is not "every cell was read right", and the tooltip is
        // the only thing that can correct a confident wrong match. Its idle
        // cost is one cursor read per 400 ms — the grab and the OCR are behind
        // the cursor hit-test — and the re-read of a cell that already matched
        // is bounded by [`HoverBudget`].
        if let Some(c) = cursor.filter(|_| hover_due) {
            // STAMPED BEFORE THE CALL. Stamping it after made the hover's
            // period `HOVER_INTERVAL` PLUS the tick's own grab, preprocess and
            // OCR — measured at 4-10 s between confirmations on 2026-08-26,
            // for a cadence that reads 400 ms. The clock the player feels is
            // how often the cursor is looked at, not how long the last look
            // took.
            //
            // A hover whose read overruns 400 ms is then due again the instant
            // it returns, and `HoverBudget` does NOT bound that on its own: it
            // charges `Confirmed` and `Matched` cells, and an `Unknown` cell —
            // exactly the cell a player rests on while waiting for a tooltip —
            // is unlimited. Stamped before and never after, the loop would sit
            // on a tooltip-less cell at a 100% duty cycle, one full-screen grab
            // and one OCR after another.
            //
            // So the stamp is REPLACED when the tick confirmed nothing. That
            // splits the two cases the player cares about, and gives each the
            // clock it wants: a cell with no tooltip yet is re-read every
            // 400 ms PLUS the read, an idle poll with a bounded duty cycle; a
            // cell whose tooltip DID answer keeps the before-stamp, so the
            // next read of it is due immediately and the budget is what stops
            // it — which is the case the budget was written for.
            last_hover = Instant::now();
            if !hover_tick(&app, &mut session, c) {
                last_hover = Instant::now();
            }
        }

        // A stop that arrived during the hover must not buy a screen grab and
        // an OCR call: the detect is as expensive as the hover, and a detached
        // thread cannot be aborted out of it.
        if *cancel.borrow() {
            break;
        }

        // What this iteration points at the screen: the gate's band, the live
        // cadence's detect, or nothing. See [`look_step`] for the precedence.
        let look = look_step(
            session.state.live,
            gate,
            detect_step(
                &session.state,
                on_cell,
                session.occlusion.is_open(),
                last_detect.elapsed(),
            ),
        );

        // The probe, and the frame it hands on when it saw the chrome. A hit
        // runs the full detect in THIS iteration rather than the next one:
        // waiting a cadence is the 2-4 s of "nothing happening" WI-B measured,
        // and the probe has already paid for the grab.
        let mut probed: Option<image::DynamicImage> = None;
        // When the probe's GRAB started, on the iterations a hit hands that
        // grab to the detect below. See the timing comment there.
        let mut probe_started: Option<Instant> = None;
        if look == Look::Probe {
            let started = Instant::now();
            let (tick, image) = probe_tick(&app, &mut session);
            trigger::note_probe(&app, now);
            // Reported for the same reason a detect is, and it is always a
            // no-op: a band OCR carries `full_frame: false`, which is precisely
            // the duration the backoff must not read. Going through the state
            // machine anyway is what stops a later reader deciding the probe is
            // the exempt path.
            session.state.note_tick_duration(tick.full_frame, Duration::ZERO);
            probed = image;
            probe_started = probed.is_some().then_some(started);
        }

        // Timed around the DETECT alone. The hover above is not part of what
        // "this machine cannot hunt at 1 Hz" measures, and folding it in is
        // what latched the backoff on 2026-08-26. The tick then says whether
        // its frame was the whole screen, which is the other half of the same
        // rule — see [`SLOW_TICK`].
        //
        // On the PROBE-HIT path the clock starts before the probe's grab, not
        // here: the detect then runs on that grab instead of taking one of its
        // own, so a clock started here would report a full-screen detect at OCR
        // cost only. That reading feeds the backoff (a probe hit on an unknown
        // panel OCRs the whole screen, so it carries `full_frame: true`), and a
        // machine that cannot hunt at 1 Hz would look like one that can for
        // exactly the ticks that hunt.
        if look == Look::Detect || probed.is_some() {
            let started = probe_started.unwrap_or_else(Instant::now);
            let tick = detect_tick(&app, &mut session, cursor, &cancel, probed);
            let took = started.elapsed();
            last_detect = Instant::now();
            // A Scan now is owed ONE detect, and this was it — whatever it
            // found. Leaving it armed on a miss would turn the button into the
            // 60 s hunt the gate exists to remove.
            if gate == trigger::GateStep::FullDetect {
                trigger::note_full_detect(&app);
            }
            if burst_satisfied(tick.outcome) {
                // The gate did its job. Disarming here rather than letting it
                // stand down is what keeps the stand-down log honest, and what
                // sends the loop back to waiting once this window retires.
                trigger::disarm(&app);
            }
            match session.state.note_tick_duration(tick.full_frame, took) {
                Some(BackoffChange::BackedOff) => crate::app_log(
                    &app,
                    format!(
                        "Merc: capture tick took {} ms — detect cadence backing off to {} s",
                        took.as_millis(),
                        DETECT_INTERVAL_SLOW.as_secs()
                    ),
                ),
                Some(BackoffChange::Recovered) => crate::app_log(
                    &app,
                    format!(
                        "Merc: {BACKOFF_DECAY_DETECTS} fast detects — detect cadence back to {} s",
                        DETECT_INTERVAL.as_secs()
                    ),
                ),
                None => {}
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
    // Same reason a retire does it: a queued merc lookup outliving the module
    // that asked for it would publish into a slice nothing is reading.
    search::close_session(&app);
    publish(&app, |slice| {
        slice.status = MercStatus::Idle;
        slice.burst_speaker = None;
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
        slice.burst_speaker = None;
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

/// The status as last published — one enum out from under the lock, so the
/// waiting states can be reconciled without cloning the capture behind them.
pub fn status(app: &AppHandle) -> MercStatus {
    let state = app.state::<AppState>();
    let status = state
        .mercenary
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .status;
    status
}

fn learned_keys(app: &AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let store = state.merc_templates.lock().unwrap_or_else(|e| e.into_inner());
    store.learned_keys()
}

/// The keys no local hover taught — the pool's contribution (POE-201).
fn pooled_keys(app: &AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let store = state.merc_templates.lock().unwrap_or_else(|e| e.into_inner());
    store.pooled_keys()
}

/// The template store's edit counter — bumped by the forget/reset commands.
fn template_generation(app: &AppHandle) -> u64 {
    let state = app.state::<AppState>();
    let generation = state
        .merc_template_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    generation
}

pub(super) fn debug_mode(app: &AppHandle) -> bool {
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
fn miss(app: &AppHandle, session: &mut Session, errored: bool) -> DetectOutcome {
    let outcome = session.state.on_detect(false);
    let retired = outcome == DetectOutcome::Retired;
    if retired {
        // ONLY here, and never on a counted miss that did not retire: a run
        // that hit the cap must stay open so its second miss lands one cadence
        // later. See [`OcclusionRun`].
        session.occlusion.on_retired();
        let capture = session.current.take();
        let confirmed = std::mem::take(&mut session.confirmed);
        let hover_budget = std::mem::take(&mut session.hover_budget);
        // The confirmations move to the retained slot rather than being
        // dropped: a retire is very often a tooltip the loop could not see
        // past, and the SAME window is re-detected seconds later (2026-08-25
        // smoke). They only come back onto a panel something POSITIVELY says
        // is this one — see [`restore_retained`]. Nothing confirmed
        // means nothing to protect, and the slot holds a whole capture for
        // [`RETAINED_TTL`], so that case keeps the old drop-everything path.
        session.retained = match (capture, confirmed.is_empty()) {
            (Some(capture), false) => Some(Retained {
                capture,
                confirmed,
                hover_budget,
                at: Instant::now(),
            }),
            _ => None,
        };
        session.panel = None;
        session.header_guard = None;
        session.crop = None;
        session.sigs.clear();
        // The budget dies with the window. Cancels a merc lookup still on the
        // shared queue — and only then, see `search::close_session`. The
        // `trade` state itself stays on the slice, like the capture it belongs
        // to: the page keeps showing the retired mercenary's verdict, and the
        // listings are part of that verdict.
        session.trade = None;
        search::close_session(app);
        crate::app_log(app, "Merc: window gone".to_string());
    }
    if !retired && errored {
        // Nothing to say: the error is already in `last_error`, and the capture
        // stands until it has been missed twice.
        return outcome;
    }
    publish(app, |slice| {
        if retired {
            slice.status = MercStatus::Idle;
            // The window this scan was armed for is gone; a name beside a
            // status that is no longer `scanning` belongs to nothing.
            slice.burst_speaker = None;
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
    outcome
}

/// One targeted probe: is the recruit window's chrome on screen?
///
/// The voice-line gate's whole screen cost (POE-204 WI-C). It grabs the screen
/// like a detect does — the platform layer captures a monitor, not a region —
/// and then OCRs [`probe_band`] alone: on the 2026-08-24 dump's geometry that
/// is 7% of the pixels a detect reads, and the OCR is the expensive half.
///
/// On a HIT it returns the image it grabbed, so the full detect that follows
/// runs on the SAME frame rather than waiting a cadence for one of its own.
/// That is the difference between "the probe found the window" and "the probe
/// found the window and the player saw it a second later".
///
/// Its tick reports `full_frame: false` whatever it saw. A band OCR says
/// nothing about what a full-screen hunt costs on this machine, and feeding one
/// to the backoff would decay a cadence on evidence about a cheaper question —
/// the same rule the cropped re-detect follows ([`SLOW_TICK`]).
///
/// `outcome` is `None` on a miss, never [`DetectOutcome::Missed`]: a probe that
/// saw no chrome has not detected anything and has not MISSED anything either,
/// and routing it through [`LoopState::on_detect`] would let a probe advance
/// the retire counter of a capture it was never asked about.
fn probe_tick(app: &AppHandle, session: &mut Session) -> (DetectTick, Option<image::DynamicImage>) {
    let tick = DetectTick::probe();

    let started = Instant::now();
    let img = match crate::capture::capture_screen() {
        Ok(img) => img,
        Err(e) => {
            fail(app, session, format!("Merc: screen capture failed — {e}"));
            return (tick, None);
        }
    };
    let (iw, ih) = {
        use image::GenericImageView;
        img.dimensions()
    };
    let screen = [iw, ih];
    let band = probe_band(session.probe_band, screen, trigger::looks(app));
    let cropped = img.crop_imm(band[0] as u32, band[1] as u32, band[2] as u32, band[3] as u32);
    let frame = geometry::Frame::probe((band[0], band[1]), screen);

    // Through the frame like every other OCR result, though nothing downstream
    // of `probe_hit` reads a box: ONE seam between OCR space and screen space,
    // taken by every line vector that leaves the engine, is what keeps the next
    // reader from being the exempt one (`geometry::Frame`).
    let lines = match crate::ocr::recognize_lines(&cropped) {
        Ok(lines) => frame.to_screen(lines),
        Err(e) => {
            fail(app, session, format!("Merc: OCR failed — {e}"));
            return (tick, None);
        }
    };
    let hit = geometry::probe_hit(&lines, &session.geometry);
    if debug_mode(app) {
        crate::app_log(
            app,
            format!(
                "Merc: probe on the {} frame {band:?} took {} ms, {} lines — {}",
                frame.describe(),
                started.elapsed().as_millis(),
                lines.len(),
                if hit { "chrome found" } else { "no chrome" },
            ),
        );
    }
    (tick, hit.then_some(img))
}

/// One detect tick: grab the screen, OCR it, and publish what it holds.
///
/// `cursor` is the loop's ONE read for this iteration, taken before the hover
/// confirm and so before this grab — which is what the header-withholding rule
/// below needs it to be. It is up to one hover-OCR older than the frame it
/// judges, and that is the honest trade against a second read: two reads inside
/// one iteration let the hold decision and the withholding decision disagree
/// about where the cursor is, and a disagreement there publishes a tooltip's
/// text as the mercenary's name.
///
/// `grabbed` is the frame a probe has already taken this iteration. Passing it
/// in is what makes a probe hit cost one grab rather than two, and — the part
/// that matters — what makes the detect read the SAME pixels the probe accepted
/// on. Re-grabbing would leave a window that closed in the millisecond between
/// them looking like a probe that lied.
fn detect_tick(
    app: &AppHandle,
    session: &mut Session,
    cursor: Option<(i32, i32)>,
    cancel: &watch::Receiver<bool>,
    grabbed: Option<image::DynamicImage>,
) -> DetectTick {
    // A KNOWN panel is re-read on a crop of itself. The full-screen OCR is the
    // tick's dominant cost, and once the panel has been found the whole answer
    // lives inside `geometry::crop_around`. The grab stays full — the
    // platform layer captures a monitor, not a region — so what the crop buys
    // is the OCR, which is the expensive half.
    //
    // Decided before the grab so every exit below, the failed ones included,
    // reports the same frame kind to the backoff.
    let crop = detect_frame(session.crop, session.panel);
    let full_frame = crop.is_none();
    let report = |outcome: Option<DetectOutcome>| DetectTick { outcome, full_frame };

    let started = Instant::now();
    let img = match grabbed {
        Some(img) => img,
        None => match crate::capture::capture_screen() {
            Ok(img) => img,
            Err(e) => {
                fail(app, session, format!("Merc: screen capture failed — {e}"));
                return report(Some(miss(app, session, true)));
            }
        },
    };
    let (iw, ih) = {
        use image::GenericImageView;
        img.dimensions()
    };
    let screen = [iw, ih];

    let cropped = crop.map(|r| img.crop_imm(r[0] as u32, r[1] as u32, r[2] as u32, r[3] as u32));
    let mut view = cropped.as_ref().unwrap_or(&img);
    let mut frame = match crop {
        Some(r) => geometry::Frame::cropped((r[0], r[1]), screen),
        None => geometry::Frame::full(screen),
    };

    // TRANSLATED THE INSTANT IT COMES BACK. Windows OCR reports boxes in the
    // pixels it was handed, and every rule below this line — the known-panel
    // anchor, the column-x test, the cell rects the hover tick hit-tests the
    // real cursor against — is screen-absolute. See `geometry::Frame`.
    let mut lines = match crate::ocr::recognize_lines(view) {
        Ok(lines) => frame.to_screen(lines),
        Err(e) => {
            fail(app, session, format!("Merc: OCR failed — {e}"));
            return report(Some(miss(app, session, true)));
        }
    };

    // The last known panel rect goes IN: a tooltip over the footer deletes the
    // anchor line this frame would otherwise need, and rows landing in the rect
    // the panel was last measured at say the same thing the missing line did.
    // It is the UNION of every rect this capture has been measured at, so a
    // partial read cannot shrink the anchor out from under the next full one.
    // See `geometry::panel_anchor` and `geometry::union_rect`.
    //
    // `detect_reason`, not `detect`: the miss branch below prints the step that
    // gave up, and a `None` cannot say which one it was.
    let mut layout =
        geometry::detect_reason(&lines, &session.geometry, &session.vocab, session.panel);

    // A crop that came back empty — or with a panel that does not FIT inside it
    // — has not seen the whole screen, and a window that MOVED is not a window
    // that closed. One full look before any of this counts. It costs the OCR
    // again but not the grab: the full frame is already in hand.
    let mut retook = false;
    if geometry::crop_needs_full_look(
        crop,
        layout.as_ref().ok().and_then(|l| geometry::panel_bounds(l, &session.geometry)),
        screen,
    ) {
        view = &img;
        frame = geometry::Frame::full(screen);
        // Through `to_screen` like the first pass, though this frame IS the
        // screen and the translation is the identity: ONE seam between OCR
        // space and screen space, taken by every line vector that leaves the
        // engine. A second, exempt path is how the next frame kind gets it
        // wrong (`geometry::Frame`).
        lines = match crate::ocr::recognize_lines(view) {
            Ok(lines) => frame.to_screen(lines),
            Err(e) => {
                fail(app, session, format!("Merc: OCR failed — {e}"));
                return report(Some(miss(app, session, true)));
            }
        };
        layout =
            geometry::detect_reason(&lines, &session.geometry, &session.vocab, session.panel);
        retook = true;
    }

    let took = started.elapsed().as_millis();
    // `crop→full` is its own state in the log, not a plain `full`: it is what a
    // window that MOVED looks like from here, and it costs two OCRs.
    let how = if retook { "crop→full" } else { frame.describe() };
    if debug_mode(app) {
        crate::app_log(
            app,
            format!("Merc: detect on the {how} frame took {took} ms, {} lines", lines.len()),
        );
    }

    let layout = match layout {
        Ok(layout) => layout,
        Err(why) => {
            // Every number the 2026-08-26 smoke wanted and did not have, on the
            // frame that lost the window: which rect the anchor was weighed
            // against, which frame the OCR ran on, how many skill names came back,
            // where their column sat, and which step of `detect_reason` returned.
            // Debug-gated because a miss is the ordinary state of a loop watching
            // an empty screen.
            if debug_mode(app) {
                crate::app_log(
                    app,
                    format!(
                        "Merc: no layout on the {how} frame — {why}; known panel {:?}, cursor {:?}",
                        session.panel, cursor
                    ),
                );
            }
            // A tooltip the player just opened sits ON the panel and hides the rows
            // the detect needs. The cursor is the proof — the game opens one only
            // under it — so this tick is not evidence the window closed.
            // No layout means no rect from THIS frame; the session's is all there
            // is. See [`cursor_on_panel`].
            let in_panel = cursor_on_panel(None, session.panel, cursor);
            let live = session.state.live;
            if session.occlusion.on_occluded(live, in_panel, Instant::now()) == MissKind::Occluded {
                if session.occlusion.announce() {
                    crate::app_log(
                        app,
                        "Merc: panel occluded (cursor over it) — holding the capture".to_string(),
                    );
                }
                return report(Some(DetectOutcome::Occluded));
            }
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
                        "Merc: looked, no recruit window — {} OCR lines, {} skill candidates \
                         ({how} frame, {took} ms)",
                        lines.len(),
                        skills
                    ),
                );
            }
            return report(Some(miss(app, session, false)));
        }
    };

    // From the LAYOUT, not the capture: the capture drops the cells past the
    // first empty slot, and the panel is as wide as its grid either way.
    // Computed here rather than at the end of the tick because the header fold
    // below needs to know whether the cursor is on the panel.
    let panel = geometry::panel_bounds(&layout, &session.geometry);
    // The band the next voice line's probe will look in. Same layout, a
    // different question — see `geometry::probe_band_bounds`.
    let next_band = geometry::probe_band_bounds(&layout, &session.geometry, screen);

    // Pass 2 is up to `max_rows` more OCR calls. A stop signal that arrived
    // during pass 1 stops here, leaving the state exactly as it was.
    if *cancel.borrow() {
        return report(None);
    }
    let texts = pass2_texts(view, frame, &layout, &session.geometry);
    let mut result = {
        let state = app.state::<AppState>();
        let store = state.merc_templates.lock().unwrap_or_else(|e| e.into_inner());
        build_capture(
            view,
            frame,
            &layout,
            &texts,
            now_ms(),
            &session.geometry,
            &session.vocab,
            &store,
        )
    };
    // Before ANY use of this frame's header — the fold below, and the
    // completeness check that opens a trade session with it. The cursor was
    // read before the grab, so it says where it was WHILE the frame was taken;
    // the rect the withholding keys on is chosen inside
    // [`publishable_header_for`], never here.
    let (published, header_guard) = publishable_header_for(
        &layout,
        &session.geometry,
        session.header_guard,
        cursor,
        std::mem::take(&mut result.capture.header),
    );
    result.capture.header = published;
    // A forget/reset while this capture was live means the user disowned a
    // confirmation; re-applying it here is exactly what the un-poison button
    // was pressed to stop.
    if generation_changed(&mut session.template_generation, template_generation(app)) {
        session.confirmed.clear();
        session.hover_budget.clear();
        // The retained slot holds the same disowned confirmations one retire
        // back. Leaving it would let the un-poison button be undone by the next
        // re-detect.
        session.retained = None;
    }

    // Nothing live means this is the first look at a panel since the last
    // retire — the moment the retained slot exists for. Before the header fold,
    // because `apply_confirmed` below reads what this restores.
    if session.current.is_none() {
        if let Some(line) = restore_retained(session, &result.capture).log_line() {
            crate::app_log(app, line);
        }
    }

    // IDENTITY FIRST, then everything the loop remembered. The header merge and
    // the remembered confirmations are both statements about ONE recruit
    // window, and a REMATCH swaps the mercenary behind a panel that looks the
    // same — with the liveness pause the loop can take ~20 s to notice a window
    // that closed, so "a capture exists" is not evidence it is the same one.
    // A different panel therefore drops the lot rather than merging into it.
    let (header, replaced) = fold_header(session.current.as_ref(), &result.capture);
    result.capture.header = header;
    if replaced {
        crate::app_log(app, "Merc: recruit window replaced — reading it fresh".to_string());
        session.current = None;
        session.confirmed.clear();
        session.hover_budget.clear();
        session.sigs.clear();
        // The rects are NOT cleared here. `replaced` is carried down to
        // [`geometry::next_panel`], which is the one place that decides
        // whether a remembered rect may be grown by this frame — see its doc
        // for why a replaced panel takes the fresh measurement alone.
    }
    // AFTER the identity check: a confirmation belongs to the window it was
    // made on, and re-applying the old window's cells to a new mercenary's rows
    // is the same inheritance bug one layer down.
    apply_confirmed(&mut result.capture, &session.confirmed);

    // A replaced panel is a NEW window however the state machine reads: the
    // loop was live for the panel that is gone, so `on_detect` would call this
    // a refresh and the log would never say a different mercenary is on screen.
    let outcome = match session.state.on_detect(true) {
        _ if replaced => DetectOutcome::Captured,
        outcome => outcome,
    };
    if outcome == DetectOutcome::Captured {
        crate::app_log(
            app,
            format!(
                "Merc: recruit window detected ({} rows, scale {:.3}) — {how} frame, {took} ms",
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
    session.revision += 1;
    // GROW-ONLY within one live capture. A partial read under a tooltip
    // measures a shorter panel, and writing that rect over the full one turns
    // the known-panel anchor against the next FULL read — six row centres, a
    // rect that holds two — which is how a window still on screen retired at
    // 16:08:28 in the 2026-08-26 smoke. The two exceptions — a REPLACED window
    // and a panel whose column moved — are the fold's own, not the caller's:
    // see [`geometry::next_panel`]. Cleared on retire, so nothing here can
    // span two windows the fold never saw.
    let column_tolerance = geometry::column_tolerance(&session.geometry, layout.scale);
    session.panel = geometry::next_panel(session.panel, panel, replaced, column_tolerance);
    session.header_guard =
        geometry::next_panel(session.header_guard, header_guard, replaced, column_tolerance);
    // FROM THE RECT THE LOOP NOW HOLDS, never from this frame's layout. The
    // crop is what the NEXT re-detect gets to see, and the rect above is the
    // panel the next anchor will be measured against; deriving the crop from a
    // partial layout instead would hand the next full read a frame cropped out
    // of the rows it is expected to find. [`geometry::crop_around`] reaches
    // outward on every axis, so a crop built from the held rect encloses it —
    // and the header band above it — by construction.
    session.crop = session.panel.map(|held| {
        geometry::crop_around(
            held,
            geometry::effective_pitch(&layout, &session.geometry),
            screen,
        )
    });
    // Only ever replaced by a better measurement, never cleared: a band from
    // the window that just closed is the best guess available for the next one.
    if next_band.is_some() {
        session.probe_band = next_band;
    }
    session.occlusion.on_hit();

    // The header as the player will see it, once per CHANGE. Every tick would
    // be a line every 2 s saying the same three fields; nothing would be a
    // header that silently went wrong (2026-08-26) with no record of when. The
    // gate is the rendered line, so a wager the loop does not print cannot
    // trigger a duplicate.
    if let Some(line) = header_log_line(&result.capture.header, &session.header_logged) {
        crate::app_log(app, line.clone());
        session.header_logged = Some(line);
    }

    // Nothing left for a DETECT to find: the cadence drops to the liveness
    // check (2026-08-25 smoke). The hover tick stays on — it is the only path
    // that can correct a confident wrong match.
    let complete = capture_complete(&result.capture);
    if session.state.note_complete(complete) {
        crate::app_log(
            app,
            format!(
                "Merc: capture complete — OCR paused (liveness every {} s)",
                LIVENESS_INTERVAL.as_secs()
            ),
        );
        // The settle edge OPENS a trade session if this capture has none yet
        // (POE-202). Here rather than at the first detect because a half-read
        // panel builds a query for a mercenary nobody has, and each of those
        // would cost one of three searches.
        //
        // `get_or_insert_with`, never a fresh session: `note_complete` is a
        // rising edge, but `LoopState::resume` drops `complete` whenever a new
        // voice line or a Scan now arms over a finished window, so one capture
        // crosses this edge as often as the player triggers a re-read. A new
        // session per edge would hand that capture a new 3-search budget each
        // time, which is unbounded searching dressed up as a ceiling. ONE
        // session per capture: opened here, cleared only by the retire in
        // [`miss`].
        session.trade.get_or_insert_with(MercTradeSession::new);
    }
    publish(app, |slice| {
        slice.status = live_status(complete);
        // Whatever armed this scan has been answered by the window on screen.
        slice.burst_speaker = None;
        slice.capture = Some(result.capture);
        slice.last_error = None;
    });
    report(Some(outcome))
}

/// Whether a retired capture's confirmations may be re-applied to `next`.
///
/// The identity rule is [`same_panel_positive`], NOT the live path's
/// [`panel_replaced`]. The live path abstains on absence and re-detects two
/// seconds later; across a retire the same abstention would write one
/// mercenary's supports onto another's rows as `Confirmed`, which no hover can
/// undo. The retained slot therefore restores only on positive evidence.
///
/// `age` bounds it: past [`RETAINED_TTL`] the panel is read fresh, because a
/// window reopened a minute later is no longer the one the player was working
/// on even when it looks identical.
pub fn retained_applies(retired: &MercCapture, next: &MercCapture, age: Duration) -> bool {
    age <= RETAINED_TTL && same_panel_positive(retired, next)
}

/// What a detect did with the retained slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    /// No slot was held — the ordinary first detect.
    Nothing,
    /// The slot applied: this many confirmations, plus the retired capture's
    /// header, are back on the session.
    Applied(usize),
    /// The slot was dropped, for the stated reason.
    Dropped(&'static str),
}

impl Restore {
    /// The line to log, or `None` when there is nothing to say.
    fn log_line(self) -> Option<String> {
        match self {
            Restore::Nothing => None,
            Restore::Applied(n) => Some(format!(
                "Merc: same recruit window as the retired one — {n} confirmation(s) and its header restored"
            )),
            Restore::Dropped(why) => Some(format!(
                "Merc: the retired confirmations were dropped — {why}"
            )),
        }
    }
}

/// Put a retired capture's confirmations back, if [`retained_applies`] says so.
///
/// The slot is CONSUMED either way: it holds one retire's worth of state, and a
/// detect that rejected it has answered the only question it was kept for.
///
/// Takes no [`AppHandle`] and returns what it did, so the whole rule — the
/// identity gate, the TTL, and what lands back on the session — is testable off
/// Windows. The caller does the logging.
fn restore_retained(session: &mut Session, next: &MercCapture) -> Restore {
    let Some(retained) = session.retained.take() else {
        return Restore::Nothing;
    };
    // One reading of the clock, so the decision and the reason it reports
    // cannot land on opposite sides of the TTL.
    let age = retained.at.elapsed();
    if !retained_applies(&retained.capture, next, age) {
        return Restore::Dropped(if age > RETAINED_TTL {
            "the slot expired"
        } else {
            "nothing positively says it is the same recruit window"
        });
    }
    let restored = retained.confirmed.len();
    session.confirmed = retained.confirmed;
    session.hover_budget = retained.hover_budget;
    // The retired capture goes back as `current` so the header fold downstream
    // SEES it. Name, class and level are as much a property of one window as
    // the confirmations are, and `fold_header` is already the single rule that
    // merges them — its own `panel_replaced` gate cannot fire here, because
    // `same_panel_positive` just required that rule to say no.
    session.current = Some(retained.capture);
    Restore::Applied(restored)
}

/// The cursor, with a failed read surfaced once.
///
/// One reader for the whole iteration: the hover confirm and the detect gate
/// ([`detect_step`]) are two questions about the same cursor, and two reads
/// would let them answer about different positions.
fn read_cursor(app: &AppHandle, session: &mut Session) -> Option<(i32, i32)> {
    match crate::capture_mouse_position() {
        Ok(c) => Some(c),
        Err(e) => {
            fail(app, session, format!("Merc: cursor position failed — {e}"));
            None
        }
    }
}

/// One hover tick: if the cursor sits in an unconfirmed captured cell, read the
/// tooltip and let it name the cell (D5).
///
/// `cursor` comes from the loop, not from a read of its own — see
/// [`read_cursor`].
///
/// Returns whether this tick actually CONFIRMED a cell. The loop uses that to
/// decide which clock the next hover is due on: see the stamp at the call
/// site. Every early exit — no capture, no cell under the cursor, a spent
/// budget, a failed grab, a tooltip that named nothing — is a `false`.
fn hover_tick(app: &AppHandle, session: &mut Session, cursor: (i32, i32)) -> bool {
    let Some(capture) = session.current.clone() else {
        return false;
    };
    let Some((ri, si)) = cell_at(&capture, cursor) else {
        return false;
    };
    // The budget is charged HERE, before the screen grab and the OCR that
    // follow it — everything above this line is one cursor read, which is what
    // makes leaving the tick running over a finished capture cheap.
    let cell_key = (row_key(&capture.rows[ri].skill), capture.rows[ri].supports[si].slot);
    if !session
        .hover_budget
        .take(cell_key, capture.rows[ri].supports[si].state)
    {
        return false;
    }
    let Some(region) = hover_region(cursor, capture.scale, &session.geometry.thresholds, capture.screen)
    else {
        return false;
    };

    // A FRESH grab: the tooltip is only on screen now, and was not in the
    // detect frame. The template still comes from the detect frame's crop.
    let grab_started = Instant::now();
    let img = match crate::capture::capture_screen() {
        Ok(img) => img,
        Err(e) => {
            fail(app, session, format!("Merc: hover capture failed — {e}"));
            return false;
        }
    };
    let grab_ms = grab_started.elapsed().as_millis();
    let (iw, ih) = {
        use image::GenericImageView;
        img.dimensions()
    };
    if (region[0] + region[2]) as u32 > iw || (region[1] + region[3]) as u32 > ih {
        return false;
    }
    let crop = img.crop_imm(
        region[0] as u32,
        region[1] as u32,
        region[2] as u32,
        region[3] as u32,
    );
    // The FAST preprocess, not the detect path's. See
    // [`crate::capture::preprocess_for_ocr_fast`]: this runs at the cursor's
    // pace over large tooltip type, which is where the sharper resampler buys
    // least.
    let preprocess_started = Instant::now();
    let processed = crate::capture::preprocess_for_ocr_fast(&crop);
    let preprocess_ms = preprocess_started.elapsed().as_millis();
    // RECTS, not just strings: the region deliberately overlaps the panel, so
    // which line is nearest the cursor is the only thing separating the tooltip
    // title from the skill column behind it.
    let ocr_started = Instant::now();
    let ocr_lines = match crate::ocr::recognize_lines(&processed) {
        Ok(lines) => lines,
        Err(e) => {
            fail(app, session, format!("Merc: hover OCR failed — {e}"));
            return false;
        }
    };
    let ocr_ms = ocr_started.elapsed().as_millis();
    // Where the 400 ms cadence actually goes. Three numbers rather than one
    // total: the grab is a whole-screen copy the loop cannot avoid, the
    // preprocess is the part this change traded accuracy for, and the OCR is
    // the part that scales with the region — which is why the region's size is
    // on the line too.
    if debug_mode(app) {
        crate::app_log(
            app,
            format!(
                "Merc: hover read {}×{} px — grab {grab_ms} ms, preprocess {preprocess_ms} ms, \
                 OCR {ocr_ms} ms",
                region[2], region[3]
            ),
        );
    }
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
        return false;
    };
    let family = confirmation.family.clone();
    let tier = confirmation.tier;

    let row_index = capture.rows[ri].index;
    let cached = session.sigs.get(&(row_index, cell.slot)).cloned();
    let (learned, needs_save, offer) = match cached {
        Some((sig, raw)) => {
            // The bytes the pool gets, taken before the signature is moved into
            // the store. Copied here rather than read back out of the store so
            // the payload is built from memory on the one path that has it
            // (POE-201 L4) — the store directory is never walked.
            let bytes = sig.bytes().to_vec();
            let state = app.state::<AppState>();
            let mut store = state.merc_templates.lock().unwrap_or_else(|e| e.into_inner());
            // The crop is the DETECT frame's, cached before the cursor ever
            // reached this cell (D5): a hovered cell may be drawn highlighted,
            // and a template learned from the highlight matches nothing later.
            let learned = store.learn(&family, tier, sig, raw, &session.geometry.thresholds);
            (
                if learned {
                    Learned::Saved
                } else {
                    Learned::AlreadyKnown
                },
                learned,
                // Only a sample the store actually took: art it already had was
                // offered to the pool when it was first learned.
                learned.then(|| sync::PendingSample {
                    family: family.clone(),
                    tier,
                    bytes,
                }),
            )
        }
        // The confirmation still stands — it names the cell. Only the template
        // is missing, and saying so is the difference between "we already knew
        // this art" and "we never had the crop".
        None => (Learned::NoCrop, false, None),
    };
    // Off the tick, like the pool upload below it: the write is one PNG per
    // sample plus the index, and the player is waiting on this read. The queue
    // is what keeps two confirms in a row from writing the directory at once —
    // see [`SaveQueue`].
    if needs_save {
        // Said once per session, not once per confirm: a writer that has
        // stopped stops for every later confirm too, and `fail` is the loop's
        // de-duplicating sink.
        if session.saves.as_ref().map(SaveQueue::request) == Some(false) {
            fail(
                app,
                session,
                "Merc: the template-store writer stopped — confirmations hold until a restart"
                    .to_string(),
            );
        }
    }
    // Off the tick: `enqueue` parks the sample and returns, the POST happens on
    // a task. A synchronous upload here would stall the read the user is
    // waiting on — the first Windows smoke already measured a 4 s tick.
    if let Some(sample) = offer {
        sync::enqueue(app, vec![sample]);
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
    session.revision += 1;
    let learned_families = learned_keys(app);
    // A hover on a key the pool taught makes it locally-known, so the pooled
    // list has to move with the learned one — they are two readings of one
    // store and a stale second list would leave the chip claiming otherwise.
    let pooled_families = pooled_keys(app);
    publish(app, |slice| {
        slice.capture = Some(updated);
        slice.learned_families = learned_families;
        slice.pooled_families = pooled_families;
    });
    true
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

    /// A detect that OCR'd the whole screen — the only frame kind the backoff
    /// is allowed to read. See [`SLOW_TICK`].
    const FULL: bool = true;
    /// A detect that OCR'd a crop of a known panel.
    const CROP: bool = false;

    /// The backoff fires once and only once, and only above the threshold.
    #[test]
    fn a_slow_detect_tick_backs_the_idle_cadence_off_once() {
        let mut st = LoopState::default();

        assert_eq!(st.note_tick_duration(FULL, SLOW_TICK), None, "at the threshold is not over it");
        assert_eq!(st.detect_interval(), DETECT_INTERVAL);
        assert_eq!(
            st.note_tick_duration(FULL, SLOW_TICK + Duration::from_millis(1)),
            Some(BackoffChange::BackedOff),
        );
        assert_eq!(st.detect_interval(), DETECT_INTERVAL_SLOW);
        assert_eq!(
            st.note_tick_duration(FULL, Duration::from_secs(9)),
            None,
            "the backoff line is logged once, not on every slow tick",
        );
    }

    /// The decay. MEASURED 2026-08-26 (app.log 09:40:06): ONE 4504 ms reading
    /// latched the backoff for the life of the thread, and every first detect
    /// after a voice line from then on waited 3 s. Once the machine is
    /// demonstrably keeping up again — which the cropped re-detect is what
    /// makes possible — the hunt goes back to 1 Hz.
    #[test]
    fn a_run_of_fast_detects_takes_the_backoff_off_again() {
        let mut st = LoopState::default();
        st.note_tick_duration(FULL, SLOW_TICK + Duration::from_millis(1));
        assert_eq!(st.detect_interval(), DETECT_INTERVAL_SLOW, "arrange: backed off");

        for _ in 1..BACKOFF_DECAY_DETECTS {
            assert_eq!(st.note_tick_duration(FULL, SLOW_TICK), None, "one fast detect is not a trend");
            assert_eq!(st.detect_interval(), DETECT_INTERVAL_SLOW);
        }

        assert_eq!(st.note_tick_duration(FULL, SLOW_TICK), Some(BackoffChange::Recovered));
        assert_eq!(st.detect_interval(), DETECT_INTERVAL);
    }

    /// The run has to be CONSECUTIVE, or a machine alternating fast and slow
    /// detects would flap the cadence — and with it the log line that
    /// announces it.
    #[test]
    fn a_slow_detect_restarts_the_run_the_decay_counts() {
        let mut st = LoopState::default();
        st.note_tick_duration(FULL, SLOW_TICK + Duration::from_millis(1));
        for _ in 1..BACKOFF_DECAY_DETECTS {
            st.note_tick_duration(FULL, SLOW_TICK);
        }

        assert_eq!(
            st.note_tick_duration(FULL, SLOW_TICK + Duration::from_millis(1)),
            None,
            "already backed off — nothing to announce",
        );

        assert_eq!(st.note_tick_duration(FULL, SLOW_TICK), None, "the run starts over");
        assert_eq!(st.detect_interval(), DETECT_INTERVAL_SLOW);
    }

    /// Recovery is announced once, exactly like the backoff.
    #[test]
    fn a_recovered_cadence_is_not_announced_again_on_the_next_fast_detect() {
        let mut st = LoopState::default();
        st.note_tick_duration(FULL, SLOW_TICK + Duration::from_millis(1));
        for _ in 0..BACKOFF_DECAY_DETECTS {
            st.note_tick_duration(FULL, SLOW_TICK);
        }
        assert_eq!(st.detect_interval(), DETECT_INTERVAL, "arrange: recovered");

        assert_eq!(st.note_tick_duration(FULL, SLOW_TICK), None);
    }

    /// The crop gate. A cropped re-detect is a fraction of a full-screen OCR by
    /// construction, so a slow one is not evidence that hunting at 1 Hz is
    /// unaffordable — and the cadence it would slow down does not run while a
    /// panel is known anyway.
    #[test]
    fn a_slow_cropped_re_detect_does_not_back_the_hunt_off() {
        let mut st = LoopState::default();

        assert_eq!(st.note_tick_duration(CROP, Duration::from_secs(9)), None);
        assert_eq!(st.detect_interval(), DETECT_INTERVAL, "the hunt is untouched");
    }

    /// The PROBE is on the same side of that gate, and it is the one that
    /// would break it fastest: a band OCR is nearly free and the gate fires two
    /// of them per voice line, so a probe reporting `full_frame: true` would
    /// take a backoff off within seconds of walking through an arena — on
    /// evidence about 7% of the screen.
    #[test]
    fn a_probes_tick_never_decays_the_backoff() {
        let mut st = LoopState::default();
        st.note_tick_duration(FULL, SLOW_TICK + Duration::from_millis(1));
        assert_eq!(st.detect_interval(), DETECT_INTERVAL_SLOW, "arrange: backed off");

        for _ in 0..BACKOFF_DECAY_DETECTS * 2 {
            assert_eq!(
                st.note_tick_duration(DetectTick::probe().full_frame, Duration::ZERO),
                None,
            );
        }

        assert_eq!(st.detect_interval(), DETECT_INTERVAL_SLOW);
    }

    /// …and the same in the other direction: fast crops must not decay a
    /// backoff the full-screen hunt earned, or the cadence would recover on
    /// evidence about a cheaper question.
    #[test]
    fn fast_cropped_re_detects_do_not_decay_the_backoff() {
        let mut st = LoopState::default();
        st.note_tick_duration(FULL, SLOW_TICK + Duration::from_millis(1));

        for _ in 0..BACKOFF_DECAY_DETECTS * 2 {
            assert_eq!(st.note_tick_duration(CROP, Duration::from_millis(50)), None);
        }

        assert_eq!(st.detect_interval(), DETECT_INTERVAL_SLOW);
    }

    // -- whether an iteration detects at all --------------------------------

    /// No occlusion run is open: the detect the hold would displace has not yet
    /// failed. Every hold test below is in this state.
    const CLEAR: bool = false;
    /// A detect HAS already come back with no layout while the cursor sat on
    /// the panel.
    const OCCLUDED: bool = true;

    /// The cadence still gates everything: a hold is a decision taken when a
    /// detect was otherwise DUE, not a reason to look early.
    #[test]
    fn an_iteration_inside_the_cadence_waits_whatever_the_cursor_is_doing() {
        let mut st = LoopState::default();
        st.on_detect(true);

        assert_eq!(
            detect_step(&st, true, CLEAR, REDETECT_INTERVAL - Duration::from_millis(1)),
            DetectStep::Wait,
        );
    }

    /// The latency fix. MEASURED 2026-08-26 (app.log 09:40:06-09:40:57):
    /// confirms landed 4-10 s apart because each waited behind a 4.5 s
    /// re-detect of a panel that had not moved. A cursor inside a captured cell
    /// is proof the window is on screen.
    #[test]
    fn a_cursor_inside_a_captured_cell_holds_the_re_detect() {
        let mut st = LoopState::default();
        st.on_detect(true);

        assert_eq!(detect_step(&st, true, CLEAR, REDETECT_INTERVAL), DetectStep::HoldForConfirm);
    }

    /// With the cursor off the grid there is nothing being confirmed, and the
    /// re-detect is the only thing watching the window.
    #[test]
    fn a_cursor_off_the_grid_does_not_hold_the_re_detect() {
        let mut st = LoopState::default();
        st.on_detect(true);

        assert_eq!(detect_step(&st, false, CLEAR, REDETECT_INTERVAL), DetectStep::Run);
    }

    /// Nothing captured means no cell to be inside and nothing to confirm — the
    /// hunt must not be holdable, or an armed burst could never find a window.
    #[test]
    fn nothing_live_is_never_held() {
        let st = LoopState::default();

        assert_eq!(detect_step(&st, true, CLEAR, DETECT_INTERVAL), DetectStep::Run);
    }

    /// The ceiling. A cursor parked on a cell must not stop the loop ever
    /// noticing a window that closed, so a whole liveness interval without a
    /// detect ends the hold whatever the cursor is doing.
    #[test]
    fn a_cursor_parked_on_a_cell_still_gets_its_liveness_re_detect() {
        let mut st = LoopState::default();
        st.on_detect(true);

        assert_eq!(detect_step(&st, true, CLEAR, LIVENESS_INTERVAL), DetectStep::Run);
    }

    /// A hold is NOT a miss. Composed the way the loop composes them: the hold
    /// never reaches `on_detect`, so it cannot advance the retire counter — and
    /// it fires exactly when a tooltip is up, which is the frame the detect
    /// fails on. Without the hold these two iterations are two misses and the
    /// capture the player is confirming retires under them.
    #[test]
    fn holding_the_re_detect_while_the_player_confirms_never_retires_the_capture() {
        let mut st = LoopState::default();
        st.on_detect(true);
        let mut since = Duration::ZERO;

        for _ in 0..RETIRE_AFTER {
            since += REDETECT_INTERVAL;
            if detect_step(&st, true, CLEAR, since) == DetectStep::Run {
                // What a detect under a tooltip returns.
                st.on_detect(false);
                since = Duration::ZERO;
            }
        }

        assert!(st.live, "the window the player is hovering is still on screen");
        assert_eq!(st.misses, 0, "a held iteration is not evidence of a closed window");
    }

    /// A cursor parked on a cell must not hold a CLOSED window's verdict on
    /// screen for ever — and the schedule it comes off on is the composition of
    /// three rules, not `detect_step`'s ceiling alone. Composed here the way
    /// the loop composes it, because the arithmetic over `detect_step` and
    /// `on_detect` by themselves describes a path no closed window takes: a
    /// cursor on a cell is a cursor inside the panel, so every failed detect
    /// goes through [`OcclusionRun`] first and is HELD, not counted, until the
    /// cap.
    ///
    /// The 28 s is [`LIVENESS_INTERVAL`] for the held ceiling to arrive
    /// (10 s), then [`OCCLUDED_MAX`] of occluded ticks rounded up to the
    /// [`REDETECT_INTERVAL`] grid the hold-suppression puts them back on
    /// (16 s), then one more cadence for the second of [`RETIRE_AFTER`]
    /// misses (2 s). Without the suppression those last three ticks are 10 s
    /// apart and the same close takes 40 s.
    #[test]
    fn a_window_closed_under_a_parked_cursor_retires_once_the_occlusion_cap_has_passed() {
        let mut st = LoopState::default();
        st.on_detect(true);
        let mut run = OcclusionRun::default();
        let start = Instant::now();
        let (mut since, mut elapsed, mut retired_at) = (Duration::ZERO, Duration::ZERO, None);

        // The loop's own quantum, so the cadences fall where they really fall.
        while elapsed < Duration::from_secs(120) {
            since += TICK;
            elapsed += TICK;
            if detect_step(&st, true, run.is_open(), since) != DetectStep::Run {
                continue;
            }
            since = Duration::ZERO;
            // A detect that found no layout, with the cursor inside the panel:
            // exactly what `detect_tick` does with one.
            if run.on_occluded(st.live, true, start + elapsed) == MissKind::Occluded {
                continue;
            }
            if st.on_detect(false) == DetectOutcome::Retired {
                run.on_retired();
                retired_at = Some(elapsed);
                break;
            }
        }

        assert_eq!(retired_at, Some(Duration::from_secs(28)));
        assert!(!st.live);
    }

    /// The hold's premise is that the cursor proves the window is on screen. A
    /// detect that already came back with no layout under that cursor is that
    /// premise disproved, so the loop stops holding and goes back to the
    /// re-detect cadence — which is the cadence [`OCCLUDED_MAX`] is sized
    /// against.
    #[test]
    fn an_open_occlusion_run_ends_the_hold() {
        let mut st = LoopState::default();
        st.on_detect(true);

        assert_eq!(detect_step(&st, true, OCCLUDED, REDETECT_INTERVAL), DetectStep::Run);
    }

    // -- which frame a detect takes -----------------------------------------

    /// The crop is a rect measured from a layout that is no longer on screen.
    /// Re-using it with nothing captured would hunt for the NEXT recruit window
    /// inside the last one's outline and never look anywhere else.
    #[test]
    fn a_leftover_crop_is_not_taken_when_no_panel_is_known() {
        assert_eq!(detect_frame(Some([100, 100, 400, 400]), None), None);
    }

    /// The ordinary re-detect: a known panel is re-read on a crop of itself,
    /// which is what takes the full-screen OCR out of the tick.
    #[test]
    fn a_known_panels_crop_is_the_frame_the_re_detect_takes() {
        let crop = [100, 100, 400, 400];

        assert_eq!(detect_frame(Some(crop), Some([150, 150, 200, 200])), Some(crop));
    }

    /// A panel known but no crop measured for it — the state a layout with no
    /// bounds leaves — is the full screen, not a panic and not a stale rect.
    #[test]
    fn a_known_panel_with_no_crop_measured_takes_the_full_screen() {
        assert_eq!(detect_frame(None, Some([150, 150, 200, 200])), None);
    }

    // -- the off-tick template-store write ----------------------------------

    /// The point of the queue: the caller does not wait for the write. The
    /// save here blocks until the test lets it go, and `request` still returns.
    #[test]
    fn a_save_request_returns_without_waiting_for_the_write() {
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let queue = SaveQueue::spawn(move || {
            started_tx.send(()).ok();
            release_rx.recv().ok();
        });

        assert!(queue.request(), "the worker is alive");

        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the write runs on the worker, not on the caller");
        release_tx.send(()).ok();
    }

    /// A save that panics must not take the queue with it. Uncaught, the worker
    /// thread dies and every later confirm is silently never written — the
    /// store would be correct in memory and empty on disk until the next start.
    #[test]
    fn a_panicking_save_does_not_take_the_queue_with_it() {
        use std::sync::atomic::{AtomicU8, Ordering};
        let (done_tx, done_rx) = std::sync::mpsc::channel::<u8>();
        let calls = AtomicU8::new(0);
        let queue = SaveQueue::spawn(move || {
            let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
            if n == 1 {
                panic!("an image encoder gave up on one sample");
            }
            done_tx.send(n).ok();
        });

        assert!(queue.request(), "the first write is asked for");
        assert!(
            done_rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "arrange: the first save panicked instead of finishing",
        );
        assert!(queue.request(), "the queue is still there to ask");

        assert_eq!(
            done_rx.recv_timeout(Duration::from_secs(5)),
            Ok(2),
            "the confirm after the panic is written",
        );
    }

    /// …and when the worker IS gone, the caller is told rather than left
    /// believing its confirmation reached disk. `hover_tick` turns this into
    /// one log line per session.
    #[test]
    fn a_request_with_no_worker_left_says_so() {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        drop(rx);
        let queue = SaveQueue { tx };

        assert!(!queue.request());
    }

    /// Two confirms back to back must not write the store directory at once,
    /// and each must be written: the second confirm's sample is only on disk
    /// if a write ran after it.
    #[test]
    fn two_confirms_in_a_row_are_written_one_after_the_other() {
        use std::sync::atomic::{AtomicU8, Ordering};
        use std::sync::{Arc, Mutex};
        let learned = Arc::new(AtomicU8::new(0));
        let in_flight = Arc::new(AtomicU8::new(0));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let queue = {
            let (learned, in_flight, seen) = (learned.clone(), in_flight.clone(), seen.clone());
            SaveQueue::spawn(move || {
                assert_eq!(
                    in_flight.fetch_add(1, Ordering::SeqCst),
                    0,
                    "two writes of the same directory overlapped",
                );
                let store = learned.load(Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(10));
                seen.lock().unwrap().push(store);
                in_flight.fetch_sub(1, Ordering::SeqCst);
                done_tx.send(()).ok();
            })
        };

        learned.store(1, Ordering::SeqCst);
        assert!(queue.request(), "the worker is alive");
        done_rx.recv_timeout(Duration::from_secs(5)).expect("the first confirm is written");
        learned.store(2, Ordering::SeqCst);
        assert!(queue.request(), "the worker is alive");
        done_rx.recv_timeout(Duration::from_secs(5)).expect("the second confirm is written too");

        assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
    }

    /// Confirms that land WHILE a write is running collapse into one more
    /// write — the write is of the whole store, so one after the last request
    /// says everything the queued ones did. What must not happen is the burst
    /// being swallowed: the last confirm's sample would never reach disk, and
    /// the next start would re-learn art the player already confirmed.
    #[test]
    fn a_burst_of_confirms_ends_in_one_write_of_the_last_one() {
        use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
        use std::sync::{Arc, Mutex};
        let learned = Arc::new(AtomicU8::new(0));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let queue = {
            let (learned, seen) = (learned.clone(), seen.clone());
            let first = AtomicBool::new(true);
            SaveQueue::spawn(move || {
                let store = learned.load(Ordering::SeqCst);
                if first.swap(false, Ordering::SeqCst) {
                    started_tx.send(()).ok();
                    gate_rx.recv().ok();
                }
                seen.lock().unwrap().push(store);
                done_tx.send(()).ok();
            })
        };

        learned.store(1, Ordering::SeqCst);
        assert!(queue.request(), "the worker is alive");
        started_rx.recv_timeout(Duration::from_secs(5)).expect("the first write started");
        learned.store(2, Ordering::SeqCst);
        assert!(queue.request(), "the worker is alive");
        learned.store(3, Ordering::SeqCst);
        assert!(queue.request(), "the worker is alive");
        gate_tx.send(()).ok();

        done_rx.recv_timeout(Duration::from_secs(5)).expect("the held write finishes");
        done_rx.recv_timeout(Duration::from_secs(5)).expect("the burst is written");
        assert_eq!(*seen.lock().unwrap(), vec![1, 3]);
        assert!(
            done_rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "two queued confirms are one write, not two",
        );
    }

    /// The backoff governs the HUNT, not a found window: a live capture keeps
    /// its 2 s re-detect even on a slow machine.
    #[test]
    fn the_backoff_does_not_slow_a_live_capture() {
        let mut st = LoopState::default();
        st.note_tick_duration(FULL, Duration::from_secs(3));
        st.on_detect(true);

        assert_eq!(st.detect_interval(), REDETECT_INTERVAL);
    }

    /// The 2026-08-25 fix: a capture with nothing left to read drops from the
    /// 2 s re-read to the 10 s liveness check. Those re-reads are what made the
    /// header blink between two OCR readings of the same unchanged pixels.
    #[test]
    fn a_completed_capture_drops_to_the_liveness_cadence() {
        let mut st = LoopState::default();
        st.on_detect(true);
        assert_eq!(st.detect_interval(), REDETECT_INTERVAL);

        assert!(st.note_complete(true), "the pause is announced on the tick it starts");

        assert_eq!(st.detect_interval(), LIVENESS_INTERVAL);
    }

    /// Said once, not on every liveness check — the loop keeps publishing a
    /// complete capture for as long as the window is on screen.
    #[test]
    fn a_capture_that_is_still_complete_is_not_announced_again() {
        let mut st = LoopState::default();
        st.on_detect(true);
        st.note_complete(true);

        assert!(!st.note_complete(true));
    }

    /// Not sticky, unlike the backoff: a re-read that lost a field has
    /// something to find again, so the working cadence comes back.
    #[test]
    fn a_capture_that_stops_being_complete_returns_to_the_working_cadence() {
        let mut st = LoopState::default();
        st.on_detect(true);
        st.note_complete(true);

        st.note_complete(false);

        assert_eq!(st.detect_interval(), REDETECT_INTERVAL);
    }

    /// Scan now over a completed capture means the player believes something on
    /// screen changed that the paused loop would not have looked for — they
    /// recruited, or rematched. It is the only caller: a voice line over a
    /// held capture never arms (`trigger::capture_held`, `disarm_probe`).
    #[test]
    fn a_new_burst_resumes_a_paused_capture() {
        let mut st = LoopState::default();
        st.on_detect(true);
        st.note_complete(true);

        assert!(st.resume(), "the resume is announced only when it resumed something");

        assert_eq!(st.detect_interval(), REDETECT_INTERVAL);
        assert!(!st.resume(), "a second press over a capture being read says nothing");
    }

    /// A resume changes the CADENCE, and [`RETIRE_AFTER`] counts ticks rather
    /// than time — so a miss accumulated at the 10 s liveness cadence is not
    /// part of the evidence that a window seen 2 s ago has closed. MEASURED
    /// 2026-08-26 (app.log 09:41:52 → 09:41:57) on the shape this replaced: one
    /// liveness miss, an arm four seconds later, one re-detect miss, "window
    /// gone" — with the recruit window still open, and restored seven seconds
    /// afterwards.
    #[test]
    fn a_resume_clears_the_misses_the_previous_cadence_counted() {
        let mut st = LoopState::default();
        st.on_detect(true);
        st.note_complete(true);
        st.on_detect(false);

        st.resume();

        assert_eq!(
            st.on_detect(false),
            DetectOutcome::Missed,
            "the first miss after a resume is the first miss, not the second",
        );
        assert!(st.live);
    }

    /// A resume that resumes NOTHING must leave the miss counter alone. Scan
    /// now is reachable while the loop is already re-detecting, and there the
    /// misses are the evidence: they were counted at the cadence still running,
    /// and two of them mean the window closed. Zeroing on every press would let
    /// a player leaning on the button hold a closed window's capture on screen
    /// indefinitely.
    #[test]
    fn a_burst_over_a_capture_already_being_read_keeps_its_misses() {
        let mut st = LoopState::default();
        st.on_detect(true);
        st.on_detect(false);

        assert!(!st.resume(), "arrange: nothing was paused, so nothing resumed");

        assert_eq!(
            st.on_detect(false),
            DetectOutcome::Retired,
            "the second consecutive miss still retires",
        );
    }

    /// The completeness belongs to the capture, not to the thread: the next
    /// window must be hunted at the hunting cadence, not at the liveness one.
    #[test]
    fn retiring_a_completed_capture_clears_the_pause() {
        let mut st = LoopState::default();
        st.on_detect(true);
        st.note_complete(true);

        for _ in 0..RETIRE_AFTER {
            st.on_detect(false);
        }

        assert!(!st.complete);
        assert_eq!(st.detect_interval(), DETECT_INTERVAL);
    }

    /// The status of a window already on screen, from the loop's own state.
    /// One owner, because three paths publish it and a second copy is how a
    /// strip ends up saying `scanning` over a window it finished reading.
    #[test]
    fn a_captured_window_is_done_when_it_is_complete_and_live_otherwise() {
        assert_eq!(live_status(true), MercStatus::Done);
        assert_eq!(live_status(false), MercStatus::Live);
    }

    /// The cell the strip asked the player to hover. This is what the tick
    /// exists for, so it is never charged — the player is looking at it BECAUSE
    /// the strip said it was unread.
    #[test]
    fn an_unread_cell_may_be_hovered_without_limit() {
        let mut budget = HoverBudget::default();

        for _ in 0..MATCHED_HOVER_ATTEMPTS as u32 * 4 {
            assert!(budget.take(("mercenary.skill_1".into(), 0), ReadState::Unknown));
        }
        assert!(budget.take(("mercenary.skill_1".into(), 0), ReadState::LowConfidence));
        assert!(budget.take(("mercenary.skill_1".into(), 0), ReadState::Ambiguous));
    }

    /// A matched cell IS re-readable — "every cell was read" is not "every cell
    /// was read right", and the tooltip is the only thing that can correct a
    /// confident wrong match. What is bounded is how often.
    #[test]
    fn a_matched_cell_is_re_read_a_few_times_and_then_left_alone() {
        let mut budget = HoverBudget::default();
        let cell = ("mercenary.skill_1".to_string(), 0);

        for attempt in 0..MATCHED_HOVER_ATTEMPTS {
            assert!(
                budget.take(cell.clone(), ReadState::Matched),
                "attempt {attempt} is inside the budget",
            );
        }

        assert!(
            !budget.take(cell, ReadState::Matched),
            "a cursor parked on a matched cell must stop buying screen grabs",
        );
    }

    /// The user already told us what this cell is. Re-reading it could only
    /// disagree with them.
    #[test]
    fn a_confirmed_cell_is_never_re_read() {
        let mut budget = HoverBudget::default();

        assert!(!budget.take(("mercenary.skill_1".into(), 0), ReadState::Confirmed));
    }

    /// Per cell, not per capture: a budget spent on one icon must not silence
    /// the one next to it.
    #[test]
    fn each_cell_carries_its_own_budget() {
        let mut budget = HoverBudget::default();
        for _ in 0..MATCHED_HOVER_ATTEMPTS {
            budget.take(("mercenary.skill_1".into(), 0), ReadState::Matched);
        }

        assert!(budget.take(("mercenary.skill_1".into(), 1), ReadState::Matched), "other slot");
        assert!(budget.take(("mercenary.skill_2".into(), 0), ReadState::Matched), "other row");
    }

    /// The charges belong to the capture they were counted against. A template
    /// forget/reset, a retire or a replaced panel all mean the read is worth
    /// buying again — and a budget that outlived its capture would leave the
    /// cell unre-readable for the rest of the session.
    #[test]
    fn clearing_the_budget_makes_a_spent_cell_readable_again() {
        let mut budget = HoverBudget::default();
        let cell = ("mercenary.skill_1".to_string(), 0);
        for _ in 0..MATCHED_HOVER_ATTEMPTS {
            budget.take(cell.clone(), ReadState::Matched);
        }
        assert!(!budget.take(cell.clone(), ReadState::Matched));

        budget.clear();

        assert!(budget.take(cell, ReadState::Matched));
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
        let bytes: Vec<u8> = (0..super::super::icons::SIG_BYTES as u32)
            .map(|i| (i as u8).wrapping_mul(7).wrapping_add(seed))
            .collect();
        CellSig::from_rgb(bytes).expect("a gradient signature is not flat")
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
    /// `nap` slices every wait into `TICK`s and clamps the last one, so TICK
    /// under the ceiling is the compliance argument. The per-constant check is
    /// the backstop for a wait that is ever slept RAW rather than through
    /// `nap` — that one blocks for its whole length.
    ///
    /// [`LIVENESS_INTERVAL`] is deliberately NOT in the list, and it is the
    /// constant that shows what the list actually means: at 10 s it is over the
    /// ceiling, and it is safe anyway because a CADENCE is not a wait. The loop
    /// naps `TICK` at a time and asks at each wake whether the cadence has
    /// elapsed, so a longer cadence costs a stop signal nothing. Sleeping one
    /// raw is what the rule forbids.
    #[test]
    fn every_wait_stays_under_the_module_poll_ceiling() {
        let ceiling = crate::modules::MODULE_THREAD_POLL_CEILING;
        assert!(TICK < ceiling, "TICK {TICK:?} must stay well under {ceiling:?}");
        for wait in [
            TICK,
            IDLE_NAP,
            UNFOCUSED_NAP,
            HOVER_INTERVAL,
            DETECT_INTERVAL,
            DETECT_INTERVAL_SLOW,
            REDETECT_INTERVAL,
        ] {
            assert!(wait < ceiling, "{wait:?} must stay under the {ceiling:?} ceiling");
        }
    }

    /// POE-198's promise in one assertion: with no capture live and a resting
    /// gate, the loop does no screen work at all.
    #[test]
    fn a_focused_loop_with_nothing_asked_of_it_does_no_work() {
        assert_eq!(next_step(false, trigger::GateStep::Resting, true), LoopStep::Idle);
    }

    #[test]
    fn a_due_probe_makes_a_focused_loop_work() {
        assert_eq!(next_step(false, trigger::GateStep::Probe, true), LoopStep::Work);
    }

    #[test]
    fn a_scan_now_makes_a_focused_loop_work() {
        assert_eq!(next_step(false, trigger::GateStep::FullDetect, true), LoopStep::Work);
    }

    /// The state IDLE_NAP would round up. A gate waiting for a 500 ms probe
    /// naps a quantum, so the probe lands within 100 ms of its deadline rather
    /// than at the next 250 ms boundary — and it must not be Work either, or an
    /// armed gate would grab the screen at 10 Hz for the half second before its
    /// probe is due.
    #[test]
    fn a_gate_waiting_for_its_probe_neither_works_nor_takes_the_idle_nap() {
        assert_eq!(next_step(false, trigger::GateStep::Waiting, true), LoopStep::Waiting);
    }

    /// The gate waits for the game rather than being spent on our own window —
    /// the alt-tab case the trigger exists to cover.
    #[test]
    fn an_armed_gate_does_not_work_while_the_game_is_not_in_front() {
        for gate in [
            trigger::GateStep::Probe,
            trigger::GateStep::FullDetect,
            trigger::GateStep::Waiting,
        ] {
            assert_eq!(next_step(false, gate, false), LoopStep::Unfocused, "{gate:?}");
        }
    }

    /// A live capture keeps its own cadence with a resting gate: retirement
    /// takes two misses, and dropping to Idle after one would strand it.
    #[test]
    fn a_live_capture_keeps_working_without_a_gate() {
        assert_eq!(next_step(true, trigger::GateStep::Resting, true), LoopStep::Work);
    }

    #[test]
    fn an_unfocused_game_stops_a_live_capture_too() {
        assert_eq!(next_step(true, trigger::GateStep::FullDetect, false), LoopStep::Unfocused);
    }

    /// A paused capture still WORKS — the liveness check is a detect tick like
    /// any other, and what makes it cheap is its cadence, not a skipped step.
    /// Idling here instead would leave a retired window on screen for good.
    #[test]
    fn a_completed_capture_still_works_so_the_liveness_check_can_run() {
        let paused = LoopState { live: true, complete: true, ..LoopState::default() };

        assert_eq!(next_step(paused.live, trigger::GateStep::Resting, true), LoopStep::Work);
        assert_eq!(paused.detect_interval(), LIVENESS_INTERVAL);
    }

    // -- what a working iteration points at the screen ----------------------

    /// The gate's own promise, in the one function that can break it: a voice
    /// line buys the BAND, never the 39-line full screen. Anything that reached
    /// `Look::Detect` here would be the burst back, one probe at a time.
    #[test]
    fn a_due_probe_looks_at_the_band_and_not_the_screen() {
        assert_eq!(
            look_step(false, trigger::GateStep::Probe, DetectStep::Run),
            Look::Probe,
        );
    }

    /// Scan now bypasses the band. A person asking has already answered the
    /// question the probe would ask, and the probe could only turn that answer
    /// into a stand-down.
    #[test]
    fn scan_now_looks_at_the_whole_screen() {
        assert_eq!(
            look_step(false, trigger::GateStep::FullDetect, DetectStep::Wait),
            Look::Detect,
        );
    }

    /// Over a HELD capture Scan now is a full RE-detect and nothing else — it
    /// runs even where the live cadence says wait, which is the whole of what
    /// "scan a window that is already open" can mean.
    #[test]
    fn scan_now_re_detects_a_held_capture_off_cadence() {
        assert_eq!(
            look_step(true, trigger::GateStep::FullDetect, DetectStep::Wait),
            Look::Detect,
        );
    }

    /// A live capture keeps its own cadence: `detect_step` owns the hold and
    /// the liveness interval, and the gate must not displace a re-detect with a
    /// probe that has nothing to add.
    #[test]
    fn a_live_capture_re_detects_rather_than_probing() {
        assert_eq!(
            look_step(true, trigger::GateStep::Probe, DetectStep::Run),
            Look::Detect,
        );
    }

    #[test]
    fn a_live_capture_that_is_holding_for_a_confirm_looks_at_nothing() {
        assert_eq!(
            look_step(true, trigger::GateStep::Resting, DetectStep::HoldForConfirm),
            Look::None,
        );
    }

    /// POE-198's promise at the other end of the loop: a resting gate with
    /// nothing captured touches the screen not at all.
    #[test]
    fn a_resting_gate_over_no_capture_looks_at_nothing() {
        assert_eq!(
            look_step(false, trigger::GateStep::Resting, DetectStep::Run),
            Look::None,
        );
    }

    // -- which band a probe reads -------------------------------------------

    /// A window reopens where it opened last, so the remembered band is the
    /// cheap one: 7% of the screen against the default's 40%.
    #[test]
    fn the_first_probe_reads_the_band_the_last_panel_left() {
        let remembered = [650, 900, 651, 240];

        assert_eq!(probe_band(Some(remembered), [1920, 1200], 0), remembered);
    }

    /// The remembered band has exactly one failure mode — the player moved the
    /// window or changed the UI scale — and it is SILENT: the probe reads empty
    /// screen, sees no chrome, and the gate stands down on a window that is
    /// plainly open. The retry is already being spent, so widening it there
    /// costs nothing and covers the case.
    #[test]
    fn the_retry_widens_to_the_default_band() {
        let remembered = [650, 900, 651, 240];

        assert_eq!(
            probe_band(Some(remembered), [1920, 1200], 1),
            geometry::default_probe_band([1920, 1200]),
        );
    }

    /// The band outlives the capture it was measured on, so it outlives a
    /// resolution change too. A rect past the new screen's edge crops to
    /// nothing and the OCR call FAILS on the empty image — the player would get
    /// "Merc: OCR failed" on the slice for walking past a mercenary after
    /// changing their display settings.
    #[test]
    fn a_band_from_a_bigger_screen_is_dropped_rather_than_cropped_to_nothing() {
        let remembered = [1600, 900, 300, 240];

        assert_eq!(
            probe_band(Some(remembered), [1280, 1024], 0),
            geometry::default_probe_band([1280, 1024]),
        );
    }

    #[test]
    fn a_session_that_has_never_seen_a_panel_reads_the_default_band() {
        assert_eq!(
            probe_band(None, [1920, 1200], 0),
            geometry::default_probe_band([1920, 1200]),
        );
    }

    /// A burst armed for a SECOND mercenary while the first one's window is
    /// still live must survive a tick that found nothing: after one miss the
    /// capture is STILL live, so liveness cannot stand in for a hit.
    #[test]
    fn a_missed_tick_under_a_live_capture_does_not_satisfy_the_burst() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        let outcome = state.on_detect(false);

        assert_eq!(outcome, DetectOutcome::Missed);
        assert!(state.live, "one miss must not retire the capture");
        assert!(!burst_satisfied(Some(outcome)));
    }

    #[test]
    fn a_detected_window_satisfies_the_burst() {
        let mut state = LoopState::default();

        assert!(burst_satisfied(Some(state.on_detect(true))));
    }

    #[test]
    fn a_re_read_of_a_live_window_satisfies_the_burst() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        assert!(burst_satisfied(Some(state.on_detect(true))));
    }

    #[test]
    fn a_retiring_tick_does_not_satisfy_the_burst() {
        let mut state = LoopState { live: true, misses: RETIRE_AFTER - 1, ..LoopState::default() };

        assert_eq!(state.on_detect(false), DetectOutcome::Retired);
        assert!(!burst_satisfied(Some(DetectOutcome::Retired)));
    }

    /// A tick that bailed on the stop signal detected nothing and missed
    /// nothing; treating it as a hit would disarm a burst that never looked.
    #[test]
    fn a_cancelled_tick_does_not_satisfy_the_burst() {
        assert!(!burst_satisfied(None));
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

    // -- the header a frame may publish ------------------------------------

    /// The corruption of 2026-08-26, end to end over the two pure pieces that
    /// stop it: a tooltip frame's header is withheld, and the sticky merge
    /// reads the withheld fields as "not read this tick" and keeps the good
    /// name. Without the withholding, `better_read` scores the tooltip's 31
    /// alphanumerics over `Arith, the Quickshot`'s 18 and the name is gone for
    /// the life of the window.
    #[test]
    fn an_occluded_frames_header_does_not_overwrite_a_good_name() {
        let good = MercHeader {
            name: Some("Arith, the Quickshot".into()),
            class: Some("Fallen Reverend".into()),
            level: Some(83),
            wager: None,
        };
        let under_tooltip = MercHeader {
            name: Some(crate::mercenary::geometry::TOOLTIP_NAME.into()),
            class: Some("SUPPORTED SKILLS".into()),
            ..good.clone()
        };

        let merged = crate::mercenary::read::merge_header(
            &good,
            &publishable_header(under_tooltip, true),
        );

        assert_eq!(merged.name.as_deref(), Some("Arith, the Quickshot"));
        assert_eq!(merged.class.as_deref(), Some("Fallen Reverend"));
    }

    /// The case this needs two rects for. A tooltip over the lower rows costs
    /// the detect those rows, so the frame comes back as a TWO-row layout whose
    /// rect stops ABOVE the cursor that caused it. Keyed on this frame alone
    /// the cursor reads as off the panel, the header is published, and the
    /// tooltip's own lines become the mercenary's name — the 2026-08-26 bug
    /// walking in through the door the withholding rule was built to shut.
    #[test]
    fn a_cursor_below_a_shrunken_frame_rect_is_still_on_the_panel_the_session_knows() {
        let six_rows = [100, 200, 500, 300];
        let two_rows = [100, 200, 500, 100];
        let on_row_six = (300, 460);
        assert!(
            !geometry::contains(two_rows, on_row_six),
            "arrange: this frame's rect stops above the cursor",
        );

        assert!(cursor_on_panel(Some(two_rows), Some(six_rows), Some(on_row_six)));
    }

    /// The FIRST detect has no session rect, and its own is the whole answer.
    #[test]
    fn the_frames_own_rect_answers_when_the_session_has_none() {
        assert!(cursor_on_panel(Some([100, 200, 500, 300]), None, Some((300, 300))));
    }

    /// A detect that found nothing has no rect of its own — the occlusion path
    /// — and the session's is what decides whether the miss is a tooltip.
    #[test]
    fn the_session_rect_answers_when_the_frame_found_no_layout() {
        assert!(cursor_on_panel(None, Some([100, 200, 500, 300]), Some((300, 300))));
    }

    /// Neither rect holding the cursor is the ordinary case, and it has to stay
    /// negative: this bool is what excuses a miss, and an excuse the cursor did
    /// not earn is a window that never retires.
    #[test]
    fn a_cursor_in_neither_rect_is_off_the_panel() {
        assert!(!cursor_on_panel(
            Some([100, 200, 500, 100]),
            Some([100, 200, 500, 300]),
            Some((300, 900)),
        ));
    }

    /// No cursor reading is not evidence of a cursor on the panel. The excuse
    /// has to be earned by a positive fix, however big the rects are.
    #[test]
    fn an_unreadable_cursor_is_not_on_the_panel() {
        assert!(!cursor_on_panel(Some([0, 0, 4000, 4000]), Some([0, 0, 4000, 4000]), None));
    }

    /// The level survives the withholding: it is what
    /// [`super::super::read::panel_replaced`] uses to notice a REMATCH, and a
    /// tooltip over the grid is no reason to stop watching for one.
    #[test]
    fn an_occluded_frame_still_reports_the_level_it_read() {
        let header = MercHeader {
            name: Some(crate::mercenary::geometry::TOOLTIP_NAME.into()),
            class: None,
            level: Some(83),
            wager: Some(1028),
        };

        let published = publishable_header(header, true);

        assert_eq!(published.level, Some(83));
        assert_eq!(published.wager, Some(1028));
    }

    /// A frame the cursor was NOWHERE near publishes what it read. The rule is
    /// about tooltips, not a blanket distrust of the header parse.
    #[test]
    fn a_frame_read_with_the_cursor_off_the_panel_publishes_its_header() {
        let header = MercHeader {
            name: Some("Arith, the Quickshot".into()),
            class: Some("Fallen Reverend".into()),
            level: Some(83),
            wager: None,
        };

        assert_eq!(publishable_header(header.clone(), false), header);
    }

    // -- the rect the header rule keys on ----------------------------------

    /// A two-row panel, built the way the loop builds one: from OCR lines
    /// through `geometry::detect`, so the rects under test are the rects the
    /// real layout produces.
    fn detected_layout() -> geometry::MercLayout {
        let lines = vec![
            OcrLineBox { text: "Wager: 1 028".into(), x: 100, y: 40, w: 90, h: 16 },
            OcrLineBox { text: "Ice Shot".into(), x: 100, y: 92, w: 64, h: 16 },
            OcrLineBox { text: "Conductivity".into(), x: 100, y: 141, w: 96, h: 16 },
        ];
        geometry::detect(&lines, &MercGeometry::default(), &vocab(), None)
            .expect("the reference lines detect as a panel")
    }

    fn named_header() -> MercHeader {
        MercHeader {
            name: Some("Arith, the Quickshot".into()),
            class: Some("Fallen Reverend".into()),
            level: Some(83),
            wager: Some(1028),
        }
    }

    /// The WI-A review carry-over, as a test. The occlusion rect and the header
    /// guard are one `bounds` call apart, and wiring this to `panel_bounds`
    /// would blank the name at the exact moment the player's cursor is on TAKE
    /// ITEM — the click that ends the window and the moment the name matters
    /// most. A cursor on the footer is inside the panel and outside the guard,
    /// and the header it read stands.
    #[test]
    fn a_cursor_on_the_footer_still_publishes_the_header_the_frame_read() {
        let g = MercGeometry::default();
        let layout = detected_layout();
        let panel = geometry::panel_bounds(&layout, &g).expect("two rows have bounds");
        let guard = geometry::header_guard_bounds(&layout, &g).expect("two rows have a guard");
        let on_footer = (panel[0] + panel[2] / 2, guard[1] + guard[3] + 4);
        assert!(geometry::contains(panel, on_footer), "arrange: the cursor is on the footer");
        assert!(!geometry::contains(guard, on_footer), "arrange: and below the header guard");

        let (published, _) =
            publishable_header_for(&layout, &g, None, Some(on_footer), named_header());

        assert_eq!(published, named_header());
    }

    /// The other half of the same choice: a cursor inside the guard IS a
    /// tooltip that could have put lines in the header band, and the name and
    /// class it read are withheld.
    #[test]
    fn a_cursor_inside_the_header_guard_withholds_the_name_and_class() {
        let g = MercGeometry::default();
        let layout = detected_layout();
        let guard = geometry::header_guard_bounds(&layout, &g).expect("two rows have a guard");
        let on_grid = (guard[0] + guard[2] / 2, guard[1] + guard[3] / 2);

        let (published, _) = publishable_header_for(
            &layout,
            &g,
            None,
            Some(on_grid),
            MercHeader {
                name: Some(geometry::TOOLTIP_NAME.into()),
                ..named_header()
            },
        );

        assert_eq!(published.name, None);
        assert_eq!(published.class, None);
        assert_eq!(published.level, Some(83), "the level survives — it is how a REMATCH is seen");
    }

    /// The rect handed back for the next frame is the HEADER guard. The session
    /// unions it with the next frame's own, so returning the footer-extended
    /// rect here would widen the withholding rule one frame later — the same
    /// miswiring, delayed.
    ///
    /// Measured against the panel rect rather than against another call of the
    /// function under test: the two differ by exactly the footer reach
    /// `geometry` keeps out of the header question
    /// (`PANEL_FOOTER_PITCHES` 3 less `HEADER_GUARD_FOOTER_PITCHES` 1), and
    /// the same box everywhere else.
    #[test]
    fn the_rect_left_for_the_next_frame_is_the_header_guard_not_the_panel() {
        let g = MercGeometry::default();
        let layout = detected_layout();
        let panel = geometry::panel_bounds(&layout, &g).expect("two rows have a panel rect");

        let (_, guard) = publishable_header_for(&layout, &g, None, None, named_header());

        let guard = guard.expect("two rows have a guard rect");
        assert_eq!(
            [guard[0], guard[1], guard[2]],
            [panel[0], panel[1], panel[2]],
            "the same box left, right and on top",
        );
        assert_eq!(
            panel[3] - guard[3],
            (layout.row_pitch * 2.0).round() as i32,
            "and two pitches shorter — the footer the header rule does not want",
        );
    }

    /// The union with the LAST frame's guard, inside the unit now. A tooltip
    /// over the lower rows costs the detect those rows, so this frame's guard
    /// stops above the cursor that caused it; the session's rect is the last
    /// frame that saw the whole panel.
    #[test]
    fn a_cursor_below_this_frames_guard_but_inside_the_last_one_still_withholds() {
        let g = MercGeometry::default();
        let layout = detected_layout();
        let guard = geometry::header_guard_bounds(&layout, &g).expect("two rows have a guard");
        let taller = [guard[0], guard[1], guard[2], guard[3] * 3];
        let below = (guard[0] + guard[2] / 2, guard[1] + guard[3] + 10);
        assert!(!geometry::contains(guard, below), "arrange: outside this frame's guard");

        let (published, _) =
            publishable_header_for(&layout, &g, Some(taller), Some(below), named_header());

        assert_eq!(published.name, None);
    }

    /// The log line exists to date a header that went wrong, so it prints the
    /// three fields the strip shows and marks the ones nothing read.
    #[test]
    fn the_header_line_names_every_field_and_marks_the_unread_ones() {
        let header = MercHeader {
            name: Some("Arith, the Quickshot".into()),
            class: None,
            level: Some(83),
            wager: None,
        };

        assert_eq!(
            header_log_line(&header, &None).as_deref(),
            Some("Merc: header — name Arith, the Quickshot, class ?, lvl 83"),
        );
    }

    /// Once per CHANGE, not per tick: the re-detect runs every 2 s and the
    /// header is the same three fields each time.
    #[test]
    fn an_unchanged_header_is_not_logged_again() {
        let header = MercHeader {
            name: Some("Arith, the Quickshot".into()),
            class: Some("Fallen Reverend".into()),
            level: Some(83),
            wager: None,
        };
        let last = header_log_line(&header, &None);

        assert_eq!(header_log_line(&header, &last), None);
    }

    /// …and a field that CHANGED is logged, which is the whole point of
    /// keeping the line at all.
    #[test]
    fn a_header_whose_class_arrived_is_logged_again() {
        let mut header = MercHeader {
            name: Some("Arith, the Quickshot".into()),
            class: None,
            level: Some(83),
            wager: None,
        };
        let last = header_log_line(&header, &None);
        header.class = Some("Fallen Reverend".into());

        assert!(header_log_line(&header, &last).is_some());
    }

    /// The wager is not on the line, so a wager the OCR only just read cannot
    /// reprint an identical one.
    #[test]
    fn a_wager_arriving_alone_does_not_reprint_the_header() {
        let mut header = MercHeader {
            name: Some("Arith, the Quickshot".into()),
            class: Some("Fallen Reverend".into()),
            level: Some(83),
            wager: None,
        };
        let last = header_log_line(&header, &None);
        header.wager = Some(1028);

        assert_eq!(header_log_line(&header, &last), None);
    }

    // -- occlusion: a tooltip is not a closed window -----------------------

    /// MEASURED 2026-08-25: the hover the user just made opened a tooltip over
    /// the panel, the detect under it read no layout, and two of those retired
    /// the capture — taking the confirmation with it. The cursor is what tells
    /// the two apart.
    #[test]
    fn a_detect_that_fails_under_the_cursor_holds_the_capture() {
        assert_eq!(
            miss_kind(true, true, Duration::ZERO),
            MissKind::Occluded
        );
    }

    /// Nothing is covering the panel from over there, so this tick really is
    /// evidence the window went away.
    #[test]
    fn a_detect_that_fails_away_from_the_panel_is_an_ordinary_miss() {
        assert_eq!(miss_kind(true, false, Duration::ZERO), MissKind::Miss);
    }

    /// The cap, at its boundary: a window closed with the cursor parked where
    /// it used to be must still retire, or the strip shows a verdict for a
    /// panel that is not on screen.
    #[test]
    fn an_occlusion_run_at_the_cap_stops_holding_the_capture() {
        assert_eq!(miss_kind(true, true, OCCLUDED_MAX), MissKind::Miss);
    }

    /// One tick under the cap is still held — the boundary is exclusive, and
    /// without this the cap would be off by one whole tick.
    #[test]
    fn an_occlusion_run_just_under_the_cap_still_holds_the_capture() {
        assert_eq!(
            miss_kind(true, true, OCCLUDED_MAX - TICK),
            MissKind::Occluded
        );
    }

    /// With no capture live there is nothing to occlude: the cursor happens to
    /// be where a panel USED to be, which is not a reason to invent one.
    #[test]
    fn a_cursor_over_no_capture_is_never_occluded() {
        assert_eq!(miss_kind(false, true, Duration::ZERO), MissKind::Miss);
    }

    /// An occluded tick found no window, so a burst armed for a second
    /// mercenary must keep looking.
    #[test]
    fn an_occluded_tick_does_not_satisfy_the_burst() {
        assert!(!burst_satisfied(Some(DetectOutcome::Occluded)));
    }

    // -- confirmations across a retire -------------------------------------

    /// A panel whose rows carry DISTINCT skill names, which is what
    /// `same_panel_positive` reasons over.
    fn merc_panel(skills: &[&str], level: Option<u32>) -> MercCapture {
        let mut capture = capture_with(
            skills
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let mut r = row(
                        i as u8,
                        &name.to_lowercase(),
                        vec![cell(0, [100, 100 + 50 * i as i32, 44, 44])],
                    );
                    r.skill.raw = (*name).to_string();
                    r.skill.name = Some((*name).to_string());
                    r
                })
                .collect(),
        );
        capture.header.level = level;
        capture
    }

    fn confirmation() -> ConfirmedCell {
        ConfirmedCell {
            family: "Added Fire Damage".into(),
            tier: 2,
            ids: vec!["support-added-fire-2".into()],
            name: Some("Added Fire Damage".into()),
            score: 0.97,
        }
    }

    /// A session with nothing in it, for the restore path. Every field is a
    /// default the loop would itself start from.
    fn bare_session() -> Session {
        Session {
            geometry: MercGeometry::default(),
            vocab: vocab(),
            state: LoopState::default(),
            errors: OnceLog::default(),
            current: None,
            sigs: SigCache::new(),
            confirmed: HashMap::new(),
            hover_budget: HoverBudget::default(),
            template_generation: 0,
            saves: None,
            miss_logged: false,
            panel: None,
            header_guard: None,
            crop: None,
            occlusion: OcclusionRun::default(),
            probe_band: None,
            header_logged: None,
            retained: None,
            trade: None,
            revision: 0,
        }
    }

    fn slot_of(capture: MercCapture, age: Duration) -> Retained {
        let mut confirmed = HashMap::new();
        confirmed.insert(("ice shot".to_string(), 0u8), confirmation());
        Retained {
            capture,
            confirmed,
            hover_budget: HoverBudget::default(),
            at: Instant::now() - age,
        }
    }

    /// The smoke's complaint, as the identity question alone: the capture
    /// retired under a tooltip and the SAME window came back seconds later.
    #[test]
    fn a_re_detect_of_the_retired_panel_is_the_same_panel() {
        let retired = merc_panel(&["Ice Shot", "Conductivity"], Some(83));
        let fresh = merc_panel(&["Ice Shot", "Conductivity"], Some(83));

        assert!(retained_applies(&retired, &fresh, Duration::from_secs(3)));
    }

    /// …and the same question at the far edge of the slot's life. Inclusive,
    /// so the boundary is not off by one whole tick.
    #[test]
    fn a_slot_exactly_at_the_ttl_is_still_restored() {
        let retired = merc_panel(&["Ice Shot", "Conductivity"], Some(83));
        let fresh = merc_panel(&["Ice Shot", "Conductivity"], Some(83));

        assert!(retained_applies(&retired, &fresh, RETAINED_TTL));
    }

    /// A panel reopened a minute later is not the one the player was working
    /// on, however identical it reads.
    #[test]
    fn a_slot_older_than_the_ttl_is_not_restored() {
        let retired = merc_panel(&["Ice Shot", "Conductivity"], Some(83));
        let fresh = merc_panel(&["Ice Shot", "Conductivity"], Some(83));

        assert!(!retained_applies(
            &retired,
            &fresh,
            RETAINED_TTL + Duration::from_secs(1)
        ));
    }

    /// THE REMATCH, one layer down from `fold_header`'s: the panel looks the
    /// same and the mercenary behind it is not. Restoring here would put the
    /// previous mercenary's supports on the new one's rows — a confident wrong
    /// read on the surface the player pays from, and one no hover can correct.
    #[test]
    fn a_re_detect_of_a_different_mercenary_does_not_get_them_back() {
        let retired = merc_panel(&["Ice Shot", "Conductivity"], Some(83));
        let fresh = merc_panel(&["Ice Shot", "Conductivity"], Some(68));

        assert!(!retained_applies(&retired, &fresh, Duration::from_secs(3)));
    }

    /// The other half of the same evidence: a rematch rolls a whole new skill
    /// list, so two disjoint sets are a different window even at the same
    /// level.
    #[test]
    fn a_re_detect_with_a_disjoint_skill_list_does_not_get_them_back() {
        let retired = merc_panel(&["Ice Shot", "Conductivity"], None);
        let fresh = merc_panel(&["Cyclone", "Enfeeble"], None);

        assert!(!retained_applies(&retired, &fresh, Duration::from_secs(3)));
    }

    /// The burden FLIP, at the retained path's own boundary: a first tick that
    /// named nothing and read no level keeps a LIVE capture (the abstention
    /// rule) but must not restore a retired one.
    #[test]
    fn a_first_tick_that_read_nothing_at_all_does_not_get_them_back() {
        let retired = merc_panel(&["Ice Shot", "Conductivity"], Some(83));
        let mut fresh = merc_panel(&["Ice Shot", "Conductivity"], None);
        for row in &mut fresh.rows {
            row.skill.name = None;
            row.skill.state = ReadState::Unknown;
        }

        assert!(!retained_applies(&retired, &fresh, Duration::from_secs(3)));
    }

    /// …and the same thin tick WITH the header line read does restore: the
    /// level is the positive fact, and this is the ordinary first tick after a
    /// re-detect.
    #[test]
    fn a_first_tick_that_read_only_the_level_gets_them_back() {
        let retired = merc_panel(&["Ice Shot", "Conductivity"], Some(83));
        let mut fresh = merc_panel(&["Ice Shot", "Conductivity"], Some(83));
        for row in &mut fresh.rows {
            row.skill.name = None;
            row.skill.state = ReadState::Unknown;
        }

        assert!(retained_applies(&retired, &fresh, Duration::from_secs(3)));
    }

    // -- what a restore actually puts back ---------------------------------

    #[test]
    fn restoring_a_slot_puts_the_confirmations_back_on_the_session() {
        let mut session = bare_session();
        session.retained = Some(slot_of(
            merc_panel(&["Ice Shot", "Conductivity"], Some(83)),
            Duration::from_secs(3),
        ));
        let fresh = merc_panel(&["Ice Shot", "Conductivity"], Some(83));

        assert_eq!(restore_retained(&mut session, &fresh), Restore::Applied(1));
        assert_eq!(
            session.confirmed[&("ice shot".to_string(), 0u8)].family,
            "Added Fire Damage"
        );
    }

    /// The header travels with them. The retired capture goes back as
    /// `current` precisely so the fold downstream has something to merge — a
    /// restore that dropped it would leave the strip's name and level blank
    /// over a window the module has already read.
    #[test]
    fn restoring_a_slot_hands_the_header_fold_the_retired_capture() {
        let mut retired = merc_panel(&["Ice Shot", "Conductivity"], Some(83));
        retired.header.name = Some("Fennik, of Unshakeable Faith".into());
        retired.header.class = Some("Fallen Reverend".into());
        let mut session = bare_session();
        session.retained = Some(slot_of(retired, Duration::from_secs(3)));
        let fresh = merc_panel(&["Ice Shot", "Conductivity"], None);

        restore_retained(&mut session, &fresh);
        let (header, replaced) = fold_header(session.current.as_ref(), &fresh);

        assert!(!replaced);
        assert_eq!(header.name.as_deref(), Some("Fennik, of Unshakeable Faith"));
        assert_eq!(header.class.as_deref(), Some("Fallen Reverend"));
        assert_eq!(header.level, Some(83));
    }

    /// A rejected slot must leave the session exactly as it found it — and be
    /// consumed, so the next detect is not asked the same question again.
    #[test]
    fn a_rejected_slot_leaves_the_session_untouched_and_is_consumed() {
        let mut session = bare_session();
        session.retained = Some(slot_of(
            merc_panel(&["Ice Shot", "Conductivity"], Some(83)),
            Duration::from_secs(3),
        ));
        let fresh = merc_panel(&["Cyclone", "Enfeeble"], Some(68));

        let outcome = restore_retained(&mut session, &fresh);

        assert!(matches!(outcome, Restore::Dropped(_)));
        assert!(session.confirmed.is_empty());
        assert!(session.current.is_none());
        assert!(session.retained.is_none(), "the slot holds one retire, not a queue");
    }

    #[test]
    fn a_detect_with_no_slot_held_restores_nothing() {
        let mut session = bare_session();
        let fresh = merc_panel(&["Ice Shot", "Conductivity"], Some(83));

        assert_eq!(restore_retained(&mut session, &fresh), Restore::Nothing);
    }

    /// A restored confirmation outranks whatever the icon store said this tick.
    #[test]
    fn a_restored_confirmation_marks_its_cell_confirmed() {
        let mut fresh = merc_panel(&["Ice Shot", "Conductivity"], Some(83));
        let mut confirmed = HashMap::new();
        confirmed.insert(("ice shot".to_string(), 0u8), confirmation());

        apply_confirmed(&mut fresh, &confirmed);

        assert_eq!(fresh.rows[0].supports[0].state, ReadState::Confirmed);
        assert_eq!(
            fresh.rows[0].supports[0].name.as_deref(),
            Some("Added Fire Damage")
        );
    }

    // -- the occlusion run, as a sequence ----------------------------------

    #[test]
    fn the_first_occluded_detect_holds_the_capture() {
        let mut run = OcclusionRun::default();

        assert_eq!(
            run.on_occluded(true, true, Instant::now()),
            MissKind::Occluded
        );
    }

    /// The cap, measured from the run's START and not from this tick.
    #[test]
    fn a_run_that_reaches_the_cap_counts_the_miss() {
        let t0 = Instant::now();
        let mut run = OcclusionRun::default();
        run.on_occluded(true, true, t0);
        run.on_occluded(true, true, t0 + Duration::from_secs(2));

        assert_eq!(run.on_occluded(true, true, t0 + OCCLUDED_MAX), MissKind::Miss);
    }

    /// THE REGRESSION THIS TYPE EXISTS FOR. Clearing the run on the miss it
    /// just produced restarts the clock, so the next tick is occluded again and
    /// the two-miss retire needs TWO full caps — ~30 s at the re-detect cadence
    /// and ~50 s at the liveness one. TAKE ITEM is inside the panel rect, so
    /// that left a closed window's verdict on screen while the cursor rested on
    /// the button that closed it.
    #[test]
    fn a_capped_run_stays_capped_so_the_second_miss_lands_one_cadence_later() {
        let t0 = Instant::now();
        let mut run = OcclusionRun::default();
        run.on_occluded(true, true, t0);
        assert_eq!(run.on_occluded(true, true, t0 + OCCLUDED_MAX), MissKind::Miss);

        assert_eq!(
            run.on_occluded(true, true, t0 + OCCLUDED_MAX + Duration::from_secs(2)),
            MissKind::Miss
        );
    }

    /// The cursor leaving the panel ends the run, so a later hover starts with
    /// a full cap rather than inheriting a spent one.
    #[test]
    fn a_cursor_that_left_the_panel_ends_the_run() {
        let t0 = Instant::now();
        let mut run = OcclusionRun::default();
        run.on_occluded(true, true, t0);
        run.on_occluded(true, false, t0 + Duration::from_secs(1));

        assert_eq!(
            run.on_occluded(true, true, t0 + Duration::from_secs(20)),
            MissKind::Occluded
        );
    }

    /// Finding the panel again ends it too — whatever was covering it is gone.
    #[test]
    fn a_detected_panel_ends_the_run() {
        let t0 = Instant::now();
        let mut run = OcclusionRun::default();
        run.on_occluded(true, true, t0);
        run.on_hit();

        assert_eq!(
            run.on_occluded(true, true, t0 + Duration::from_secs(20)),
            MissKind::Occluded
        );
    }

    /// No detect runs while the game is behind us, so the clock must not count
    /// the alt-tab.
    #[test]
    fn losing_focus_ends_the_run() {
        let t0 = Instant::now();
        let mut run = OcclusionRun::default();
        run.on_occluded(true, true, t0);
        run.on_focus_lost();

        assert_eq!(
            run.on_occluded(true, true, t0 + Duration::from_secs(20)),
            MissKind::Occluded
        );
    }

    /// A retire ends it: there is no panel left to be occluded.
    #[test]
    fn a_retire_ends_the_run() {
        let t0 = Instant::now();
        let mut run = OcclusionRun::default();
        run.on_occluded(true, true, t0);
        run.on_retired();

        assert_eq!(
            run.on_occluded(true, true, t0 + Duration::from_secs(20)),
            MissKind::Occluded
        );
    }

    /// The log says it once per run, not once per tick — the loop re-runs this
    /// path every cadence and would otherwise fill the 50-entry buffer.
    #[test]
    fn the_occlusion_line_is_announced_once_per_run() {
        let t0 = Instant::now();
        let mut run = OcclusionRun::default();
        run.on_occluded(true, true, t0);

        assert!(run.announce());
        assert!(!run.announce());
    }

    /// …and a NEW run says it again, or a second hover would go unlogged.
    #[test]
    fn a_new_run_is_announced_again() {
        let t0 = Instant::now();
        let mut run = OcclusionRun::default();
        run.on_occluded(true, true, t0);
        run.announce();
        run.on_hit();
        run.on_occluded(true, true, t0 + Duration::from_secs(20));

        assert!(run.announce());
    }

    /// Nothing to announce before a run has opened.
    #[test]
    fn a_closed_run_announces_nothing() {
        assert!(!OcclusionRun::default().announce());
    }

    // -- the store purge's log line (POE-207) --------------------------------

    /// The upgrade case: the line names the version the index actually
    /// declared, so the log says which store was dropped and which one this
    /// build reads.
    #[test]
    fn a_purged_format_one_store_is_logged_with_its_version_and_count() {
        let line = purge_log_line(&super::super::icons::PurgedStore {
            version: Some(1),
            dropped: 61,
        });

        assert_eq!(line, "Merc: dropped 61 format-1 template(s) (format 2)");
    }

    /// A downgrade meets an index from a LATER version, and it must not be
    /// reported as the format-1 upgrade — the causes are opposite (an old
    /// store this build replaces, versus a newer store this build destroys)
    /// and only one of them is expected.
    #[test]
    fn a_purged_store_from_a_later_version_is_logged_with_that_version() {
        let line = purge_log_line(&super::super::icons::PurgedStore {
            version: Some(3),
            dropped: 7,
        });

        assert!(line.contains("format-3"), "{line}");
    }

    /// An index that parsed as nothing has no version and no count. Saying
    /// "dropped 0 format-1 templates" would be two wrong facts about a
    /// half-written or corrupt file.
    #[test]
    fn a_purged_unreadable_index_is_logged_as_unreadable() {
        let line = purge_log_line(&super::super::icons::PurgedStore {
            version: None,
            dropped: 0,
        });

        assert_eq!(line, "Merc: dropped an unreadable template index (format 2)");
    }

    // -- the matcher's geometry warning (POE-207) ----------------------------

    /// Default geometry says the thresholds in force and warns about nothing.
    ///
    /// The "warns about nothing" half is the one that matters: a warning that
    /// fires on every start is a warning nobody reads, and the smoke checklist
    /// for the format-2 build is "no override warning with default geometry".
    #[test]
    fn default_geometry_logs_the_thresholds_and_warns_about_nothing() {
        let lines = matcher_geometry_warnings(&MercGeometry::default());

        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0], "Merc: icon thresholds match 0.88 / low 0.78 / lead 0.05");
    }

    /// An overridden threshold is named as an override, with the numbers
    /// format 2 was measured at — the answer to "why is nothing matching any
    /// more" for a user who edited `merc-geometry.json` months ago.
    #[test]
    fn an_overridden_icon_threshold_is_warned_about() {
        let mut g = MercGeometry::default();
        g.thresholds.icon_match = 0.80;

        let lines = matcher_geometry_warnings(&g);

        assert_eq!(lines[0], "Merc: icon thresholds match 0.80 / low 0.78 / lead 0.05");
        assert!(
            lines.iter().skip(1).any(|l| l.contains("overridden") && l.contains("0.88/0.78/0.05")),
            "no override warning naming the measured defaults: {lines:?}",
        );
    }

    /// A `cellInset` override is warned about too, because it is what decides
    /// what the alignment window IS. At the live 43 px cell, inset 6 leaves a
    /// 25 px window — still aligned, but no longer the 33 px window every
    /// pooled signature was derived from, so the shared corpus stops matching
    /// while nothing looks broken. (Inset 7 is the harder break: a 23 px
    /// window is under `SIG_DIM` and alignment is dropped altogether —
    /// `icons::tests::corpus` pins both sides of that boundary.)
    #[test]
    fn an_overridden_cell_inset_is_warned_about() {
        let mut g = MercGeometry::default();
        g.cell_inset = 6.0;

        let lines = matcher_geometry_warnings(&g);

        assert!(
            lines.iter().any(|l| l.contains("cell geometry overridden") && l.contains("inset 6.0")),
            "no cell-geometry warning: {lines:?}",
        );
    }

    /// The two overrides are independent: an untouched threshold block must
    /// not be reported as overridden just because the cell geometry was.
    #[test]
    fn a_cell_geometry_override_does_not_report_the_thresholds_as_overridden() {
        let mut g = MercGeometry::default();
        g.cell_size = 40.0;

        let lines = matcher_geometry_warnings(&g);

        assert!(
            !lines.iter().any(|l| l.contains("thresholds overridden")),
            "{lines:?}",
        );
    }
}
