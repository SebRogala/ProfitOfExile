//! The temple capture loop (POE-171) — the module's Windows glue.
//!
//! `modules.rs::spawn_temple` delegates here. Like the merc loop, this is a
//! [`ModuleJoin::Thread`](crate::modules::ModuleJoin::Thread) rather than a
//! task: screen capture and `Windows.Media.Ocr` are apartment-threaded and
//! deadlock on the async runtime (see `spawn_gem_scan` in lib.rs). Threads
//! cannot be aborted, so every wait goes through [`nap`], which polls
//! `*cancel.borrow()` every 100 ms — two orders under the registry's 5 s
//! `MODULE_THREAD_POLL_CEILING`.
//!
//! # No `#[cfg(windows)]`, on purpose
//!
//! The platform difference arrives as an `Err` from three calls —
//! `capture::capture_screen`, `ocr::recognize_lines`, `ocr::engine_ready` —
//! each of which already has a non-Windows arm. Gating the loop body as well
//! would add a second place for the two arms to diverge and would stop the
//! Linux container compiling the code it is meant to protect. This follows
//! `mercenary::run`, which does the same.
//!
//! # Four gates before an expensive read
//!
//! A full read is 28 OCR calls: two bounded crops for the side panel and the
//! budget line, and two per plate (name band + tier numeral) for all 13. That
//! is far too much to run per frame, so the loop spends most of its time on the
//! cheap half:
//!
//! 1. **The detect tick** ([`DETECT_INTERVAL`]) — one
//!    [`anchor::detect_cheap`], which is at most two correlations and touches
//!    no OCR engine. It answers only "is anything plate-shaped on screen?",
//!    and it is the gate that decides whether this tick pays for a
//!    [`reader::read_layout_with_hint`] at all.
//! 2. **The pixel gate** — the [`reader::read_layout_with_hint`] the detect
//!    tick promoted to. Its fingerprint ([`slice::layout_signature`]) moves
//!    when the player moves, when a corridor opens, and when the panel moves
//!    or rescales.
//! 3. **The text tick** ([`PANEL_RECHECK_INTERVAL`]) — the two text crops
//!    ([`panel_text`]), run only while the pixel fingerprint is unchanged. It
//!    catches the case the pixels cannot see: a kill that changes a plate's
//!    name and tier without touching a corridor.
//! 4. **Panel lost → re-arm.** Closing the panel clears the gate, so
//!    "close, kill the architect, reopen" always re-reads.
//!
//! # The detect cadence
//!
//! Gate 1 exists because gates 2–4 all sit *behind* a read whose cost is
//! upside down: a capture with the panel OPEN anchors in two correlations,
//! while a capture with no panel on it runs the hint, the seeded band and the
//! full sweep — ~105 correlations, measured 3.9 s on a 1539 px board. A closed
//! panel is the state this loop lives in, so its steady state was its most
//! expensive one.
//!
//! So every [`DETECT_INTERVAL`] the loop runs [`anchor::detect_cheap`] (~1/80
//! of that, measured) and pays for the full read only when:
//!
//! - the cheap tick anchored the remembered plate, or nominated a new one; or
//! - a panel is already live — a live panel is the CHEAP input, and refusing
//!   to read one because the cheap tick could not see it is how a panel whose
//!   scale drifted would get retired instead of re-anchored; or
//! - the user pressed re-arm ([`slice::ReadGate::rearm_pending`]), which the
//!   promoting tick then spends ([`slice::ReadGate::note_rearm`]); or
//! - [`FULL_READ_EVERY_N_MISSES`] cheap ticks in a row have said nothing —
//!   the backstop for a UI-scale change, which is the one way a panel can be on
//!   screen and invisible to the cheap tick.
//!
//! [`wants_full_read`] is all four rules in one pure function, so the
//! composition is testable without a screen. A cheap tick that says nothing is
//! a MISS in the sense [`LoopState`] already meant it, so the retire-after-two
//! rule and the status machine are unchanged by all of this.
//!
//! The two timings above were taken by [`anchor::detect_cheap`]'s own
//! measurement, described in that function's note: `cargo test --release --lib`
//! on the Linux container, over deterministic noise cut to each committed board
//! fixture's dimensions. A **release** build — the ratio holds in debug but the
//! absolute numbers do not.
//!
//! # Every OCR crop is bounded
//!
//! No path here hands a whole frame to [`crate::capture::preprocess_for_ocr`].
//! That function upscales 2× unconditionally, so a 4K capture would become a
//! 33 Mpx buffer per tick; `capture.rs` states the invariant this module has to
//! keep ("the live capture paths crop from the primary monitor, so dimensions
//! stay bounded"). Both text ROIs — [`panel_rect`] and [`remaining_rect`] — are
//! derived from the anchor's scale and stay a fixed size in reference px
//! whatever the monitor is.
//!
//! On top of those, `temple_rearm` bumps a counter the gate watches, which is
//! the user's own escape when a read looks wrong.
//!
//! # Read-only, always
//!
//! This module reads the screen. It never moves the cursor and never sends
//! input — injecting input into the PoE client is against GGG's ToS.

use std::time::{Duration, Instant};

use image::DynamicImage;
use tauri::{AppHandle, Manager};
use tokio::sync::watch;

use crate::modules::ModuleJoin;
use crate::AppState;

use super::anchor::{self, CheapHint};
use super::lattice::{self, Lattice};
use super::markers;
use super::panel::{self, SystemOcr};
use super::reader::{self, TempleLayout};
use super::slice::{self, TempleSettings, TempleSlice, TempleStatus};

/// Loop quantum. Every wait is built out of these, so a stop signal is honoured
/// within one of them whatever the cadence above it says.
const TICK: Duration = Duration::from_millis(100);
/// Pixel-tick cadence: how often the loop looks for (or re-checks) the layout
/// panel. One anchor match plus one beam-sampling pass, no OCR.
const DETECT_INTERVAL: Duration = Duration::from_millis(1000);
/// Pixel-tick cadence after the backoff has fired.
const DETECT_INTERVAL_SLOW: Duration = Duration::from_millis(3000);
/// Text-tick cadence: how often an unchanged-looking board pays for the two
/// text crops ([`panel_text`]) to check whether the panel changed underneath
/// it.
///
/// Slower than the pixel tick because it is the expensive gate and the case it
/// catches — a kill inside the same room with no corridor change — is not one
/// the player is waiting on a sub-second answer for.
const PANEL_RECHECK_INTERVAL: Duration = Duration::from_millis(4000);
/// A **cheap** detect tick slower than this backs the detect cadence off, once,
/// for the life of the thread.
///
/// Cheap ticks only — a tick that promoted to the full read is excluded, and
/// the [`FULL_READ_EVERY_N_MISSES`] backstop is the reason that matters: it
/// deliberately costs seconds, and letting one of those trip a *sticky* backoff
/// would slow the loop permanently on the strength of a cost the loop chose to
/// pay. What this still measures is the thing that runs on every tick, which is
/// what the cadence has to fit inside.
const SLOW_TICK: Duration = Duration::from_millis(1500);
/// How long to idle between focus checks while the game is not focused.
const UNFOCUSED_NAP: Duration = Duration::from_millis(1000);
/// Distinct error messages logged before the loop stops repeating itself. The
/// failure path re-runs every second and an error carrying a varying number is
/// a different string every time, so without a cap one loop could fill the
/// 50-entry LOGS buffer on its own.
const MAX_DISTINCT_ERRORS: usize = 12;
/// Consecutive failed anchors that retire a live panel. Two, not one: the
/// anchor briefly loses a panel that is mid-fade, and retiring on the first
/// miss would re-arm the gate and buy a full re-read every time.
const RETIRE_AFTER: u8 = 2;
/// Cheap detect ticks that may say "nothing here" before one full read is
/// forced anyway.
///
/// [`anchor::detect_cheap`] recovers a panel that MOVED on the next tick, and
/// [`settings_for_capture`] drops the remembered scale the moment the capture
/// changes size, so this is not the recovery path for either of those. What it
/// covers is the case neither of them can see: a capture that is still the same
/// size and still holds a panel whose scale has drifted far enough from
/// `width / REFERENCE_SCREEN_WIDTH` that the nominating pass no longer clears
/// [`anchor::COARSE_CANDIDATE_FLOOR`] — the game's own UI-scale slider is the
/// way that happens.
///
/// 30 ticks is 30 s at [`DETECT_INTERVAL`] and 90 s once
/// [`DETECT_INTERVAL_SLOW`] has fired. Long, deliberately: the case is rare and
/// the forced read costs ~80× a cheap tick (see [`anchor::detect_cheap`]), so
/// this is the one place the loop still pays the old price and it should be
/// paid as seldom as the recovery it buys allows.
const FULL_READ_EVERY_N_MISSES: u32 = 30;

/// Spawn the capture loop. Called through `MODULES` — see `modules.rs`.
pub fn spawn(app: AppHandle, cancel: watch::Receiver<bool>) -> ModuleJoin {
    ModuleJoin::Thread(std::thread::spawn(move || run_loop(app, cancel)))
}

// ---------------------------------------------------------------------------
// Pure pieces
// ---------------------------------------------------------------------------

/// The loop's panel state machine: what cadence to run at, and when a panel
/// that has stopped anchoring has been missing long enough to retire.
///
/// Separated from the loop so both rules — retire after two misses, back off
/// after a slow tick — are testable without a screen or a clock. Same shape as
/// `mercenary::run::LoopState`, deliberately: the two loops solve the same
/// cadence problem and a second shape would be a second thing to reason about.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoopState {
    /// A layout panel is on screen.
    pub live: bool,
    /// Consecutive failed anchors since the last successful one.
    pub misses: u8,
    /// The slow-tick backoff has fired.
    ///
    /// Sticky for the life of the thread: it means "this machine takes over
    /// 1.5 s to run a cheap detect tick on a screen this size", which does not
    /// become false again, and flapping between cadences would flap the log
    /// line that announces it.
    pub backed_off: bool,
    /// Cheap detect ticks since the last full read — see
    /// [`FULL_READ_EVERY_N_MISSES`].
    pub cheap_misses: u32,
}

/// What one pixel tick did to the panel state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectOutcome {
    /// A panel was found where there was none.
    Found,
    /// A panel that was already live anchored again.
    Held,
    /// Nothing anchored, and nothing was live (or not enough misses yet).
    Missed,
    /// The live panel just retired after [`RETIRE_AFTER`] misses.
    Retired,
}

impl LoopState {
    /// How long to wait before the next pixel tick.
    ///
    /// One cadence whether or not a panel is live: the tick costs the same
    /// either way — it is the same `read_layout_with_hint` call — so there is
    /// nothing for a second number to buy.
    pub fn detect_interval(&self) -> Duration {
        if self.backed_off {
            DETECT_INTERVAL_SLOW
        } else {
            DETECT_INTERVAL
        }
    }

    /// Fold one anchor result into the state.
    pub fn on_detect(&mut self, found: bool) -> DetectOutcome {
        if found {
            self.misses = 0;
            if self.live {
                DetectOutcome::Held
            } else {
                self.live = true;
                DetectOutcome::Found
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

    /// Fold one cheap detect into the state, and say whether this tick pays for
    /// the full read.
    ///
    /// Four ways in, and the counter resets on all four so a promotion for any
    /// reason restarts the periodic one:
    ///
    /// 1. the cheap tick saw something ([`anchor::CheapDetect::worth_reading`]);
    /// 2. a panel is already live. The loop lives in `live == false`, so the
    ///    gate keeps all of its value — and what this buys is the case the
    ///    cheap tick is blind to: an OPEN panel whose UI scale drifted past
    ///    [`anchor::COARSE_CANDIDATE_FLOOR`] with a hint that no longer
    ///    matches. Without it two such ticks retire a panel that is on screen
    ///    and the board goes away for [`FULL_READ_EVERY_N_MISSES`] ticks;
    /// 3. the user pressed re-arm — which must force a read even while nothing
    ///    is anchored, or the button does nothing on a panel that is open and
    ///    unchanged;
    /// 4. [`FULL_READ_EVERY_N_MISSES`] cheap ticks have said nothing.
    pub fn note_cheap_detect(&mut self, detected: bool, rearmed: bool) -> bool {
        if detected || self.live || rearmed || self.cheap_misses + 1 >= FULL_READ_EVERY_N_MISSES {
            self.cheap_misses = 0;
            true
        } else {
            self.cheap_misses += 1;
            false
        }
    }

    /// Record how long one detect tick took. `true` the one time the backoff
    /// fires, so the caller logs it once.
    ///
    /// `promoted` ticks are ignored rather than filtered by the caller, so the
    /// rule sits in the tested surface: a tick that paid for the full read is
    /// not evidence about the cadence, and the periodic backstop deliberately
    /// costs seconds — letting one of those trip a *sticky* backoff would slow
    /// the loop for the rest of the session on the strength of a cost it chose
    /// to pay. See [`SLOW_TICK`].
    pub fn note_tick_duration(&mut self, took: Duration, promoted: bool) -> bool {
        if promoted {
            return false;
        }
        if took > SLOW_TICK && !self.backed_off {
            self.backed_off = true;
            true
        } else {
            false
        }
    }
}

/// The detect tick's whole decision: does this tick pay for the full read?
///
/// Pure over both state machines so the composition is testable without a
/// screen or a clock — and the composition is where the interesting rule lives:
/// **a promotion that happened because of a re-arm has to spend the bump right
/// here.** [`slice::ReadGate::layout_wants_read`], the other place that spends
/// it, is reached only after a read that SUCCEEDED, so a re-arm pressed while
/// no panel is on screen would stay pending on every subsequent tick and pin
/// the loop into the full read for the rest of the session. The settings
/// commands re-arm on every change, so that is not a corner case.
pub fn wants_full_read(
    state: &mut LoopState,
    gate: &mut slice::ReadGate,
    cheap: &anchor::CheapDetect,
    rearm: u64,
) -> bool {
    let rearmed = gate.rearm_pending(rearm);
    let read = state.note_cheap_detect(cheap.worth_reading(), rearmed);
    if read && rearmed {
        gate.note_rearm(rearm);
    }
    read
}

// --------------------------------------------------- the status machine --

/// What one loop event says about the module's state.
///
/// The cadence half of the loop is [`LoopState`]; this is the half the page
/// sees. Both are separated from the thread for the same reason: the rules are
/// worth testing without a screen or a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickOutcome {
    /// The tick failed before it could tell whether a panel is there — the
    /// capture or the OCR engine returned an error.
    Failed,
    /// The tick ran clean and nothing anchored.
    NoPanel,
    /// The tick ran clean and a panel anchored. A full read follows, which
    /// publishes its own status through [`slice::project`].
    Anchored,
    /// The loop is shutting down.
    Stopping,
}

/// The status one loop event leaves behind, and whether it ends the last error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusUpdate {
    pub status: TempleStatus,
    /// `true` when the event means the last error is over.
    pub clear_error: bool,
}

/// The module's status machine.
///
/// # The rule this exists for
///
/// A transient failure — one `capture_screen` error while the player is not on
/// the temple screen — writes [`TempleStatus::Error`] plus a message. Every
/// tick after it is a *clean miss*: the loop looked, there was no panel, and
/// there is nothing wrong. Clearing `last_error` on that tick without moving
/// the status leaves the page showing `error` with no message under it, and it
/// stays that way until the player next opens a layout panel — which may be the
/// rest of the session. The status and the message are written together here so
/// the two cannot drift apart.
///
/// `prev` is read for one rule: [`TempleStatus::Unavailable`] is not a tick
/// result. It means capture or OCR is missing for the life of the process, so
/// no later event makes it available again — including the shutdown publish,
/// which is why that path comes through here too rather than repeating the
/// check.
pub fn next_status(prev: TempleStatus, outcome: TickOutcome) -> StatusUpdate {
    if prev == TempleStatus::Unavailable {
        return StatusUpdate {
            status: TempleStatus::Unavailable,
            clear_error: false,
        };
    }
    let (status, clear_error) = match outcome {
        TickOutcome::Failed => (TempleStatus::Error, false),
        TickOutcome::NoPanel => (TempleStatus::PanelNotVisible, true),
        TickOutcome::Anchored => (TempleStatus::Reading, true),
        // A stopped loop is not reporting a live board, and the reason the last
        // error happened is no longer something the user can act on.
        TickOutcome::Stopping => (TempleStatus::Idle, true),
    };
    StatusUpdate {
        status,
        clear_error,
    }
}

/// Fold one loop event into the slice — the only writer of
/// [`TempleSlice::status`] outside [`slice::project`].
pub fn apply_status(slice: &mut TempleSlice, outcome: TickOutcome) {
    let update = next_status(slice.status, outcome);
    slice.status = update.status;
    if update.clear_error {
        slice.last_error = None;
    }
}

// ------------------------------------------------------------ text ROIs --

/// Left edge of the side panel's OCR region, in reference px measured from the
/// capture's RIGHT edge. The region runs from here to the right edge.
///
/// **Measured** on all seven source screenshots (the five diamond fixtures'
/// sources plus the two board fixtures'), by finding the panel's own left
/// border column:
///
/// | source screenshot | width | scale | panel left, ref px from the right edge |
/// |---|---|---|---|
/// | `2026-08-02_22-22-38` | 1374 | 1.0000 | 490.0 |
/// | `2026-08-07_19-28-36` | 1539 | 1.1321 | 493.8 |
///
/// The two hand-verified boards agree to 4 ref px; 540 gives ~46 ref px of
/// margin, and the region was then checked to contain the whole panel — border,
/// title, both architect blocks and the `Enter Incursion` button — on all seven.
pub const PANEL_LEFT_REF: f32 = 540.0;
/// Bottom edge of the side panel's OCR region, in reference px from the
/// capture's TOP edge. The region starts at the top edge.
///
/// Measured panel bottoms: 387.0 ref px (1374 board) and 388.7 (1539 board).
/// 430 gives ~42 ref px of margin — see [`PANEL_LEFT_REF`].
pub const PANEL_BOTTOM_REF: f32 = 430.0;

/// The `N Incursions Remaining` line's OCR region, relative to the Entrance
/// plate centre ([`TempleLayout::origin`]), in reference px.
///
/// The game centres this line under the Entrance plate, so it is keyed on the
/// anchor rather than on a screen edge — which also makes it the one text ROI a
/// windowed client cannot displace (see [`diamond_rect`]'s note).
///
/// **Measured** as the line's glyph bounding box on all seven source
/// screenshots. Horizontal extent from the origin: −108.9 … +108.9 ref px
/// (worst case; ±105 typical). Vertical: +73.3 … +88.0 ref px. The constants
/// below are that box with ~40 ref px of horizontal and ~14 ref px of vertical
/// margin.
pub const REMAINING_HALF_W_REF: f32 = 150.0;
/// Top of the budget line's region, ref px below the Entrance centre. Kept
/// clear of the Entrance plate's own bottom border, which sits at +42.
pub const REMAINING_TOP_REF: f32 = 58.0;
/// Bottom of the budget line's region — see [`REMAINING_HALF_W_REF`].
pub const REMAINING_BOTTOM_REF: f32 = 104.0;

/// The side panel's region, given a capture size and the anchor's scale.
///
/// `[x, y, w, h]`. Anchored to the capture's top-right corner, like
/// [`diamond_rect`] and for the same measured reason — and carrying the same
/// windowed-client failure mode, which here degrades to "the panel's text is
/// not read" rather than to a wrong answer: [`panel::read_panel`] returns an
/// unread title and no offers, and the advisor warns rather than inventing one.
pub fn panel_rect(screen: (u32, u32), scale: f32) -> [i32; 4] {
    let w = (PANEL_LEFT_REF * scale).round() as i32;
    let h = (PANEL_BOTTOM_REF * scale).round() as i32;
    [screen.0 as i32 - w, 0, w, h]
}

/// The `N Incursions Remaining` region, given the Entrance centre and the
/// anchor's scale. `[x, y, w, h]` — see [`REMAINING_HALF_W_REF`].
pub fn remaining_rect(origin: (i32, i32), scale: f32) -> [i32; 4] {
    let half_w = (REMAINING_HALF_W_REF * scale).round() as i32;
    let top = origin.1 + (REMAINING_TOP_REF * scale).round() as i32;
    let bottom = origin.1 + (REMAINING_BOTTOM_REF * scale).round() as i32;
    [origin.0 - half_w, top, 2 * half_w, bottom - top]
}

/// Crop `rect` from `img`, clipped to the frame. `None` when nothing overlaps.
///
/// Clipped, unlike [`diamond_rect`]'s consumer: a text ROI that hangs off the
/// frame still has readable text in the part that does not, and there is no
/// count or angle here for a bad rect to corrupt — the worst a clipped crop
/// does is read fewer lines. An empty intersection is `None` rather than a
/// zero-sized image, which `preprocess_for_ocr` would panic on.
pub fn crop_clipped(img: &DynamicImage, rect: [i32; 4]) -> Option<DynamicImage> {
    let [x, y, w, h] = rect;
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(img.width() as i32);
    let y1 = (y + h).min(img.height() as i32);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(img.crop_imm(
        x0 as u32,
        y0 as u32,
        (x1 - x0) as u32,
        (y1 - y0) as u32,
    ))
}

// ------------------------------------------------------ the diamond rect --

/// Diamond centre offset from the capture's TOP-RIGHT corner, in reference px
/// (the [`super::anchor::REFERENCE_SCREEN_WIDTH`] = 1374 space).
///
/// **Measured, provisional, and the weakest number in this module.**
///
/// Re-measured over all **five** committed diamond fixtures. Each crop box is
/// the one `markers.rs`'s fixture table records, and the crops are cut centred
/// on the diamond, so the box's centre IS the fitted centre. The scale is the
/// one `reader::read_layout` recovers from the *source screenshot* (not from
/// the requantised board fixture, which reads up to 0.01 lower):
///
/// | source screenshot | width | scale | diamond centre | dx from right edge | dy from top |
/// |---|---|---|---|---|---|
/// | `2026-08-02_16-41-11` | 1352 | 1.0164 | (1128, 160) | 220.4 | 157.4 |
/// | `2026-08-03_22-54-58` | 1358 | 1.0001 | (1122, 183) | 236.0 | 183.0 |
/// | `2026-08-02_22-22-38` | 1374 | 1.0000 | (1126, 186) | 248.0 | 186.0 |
/// | `2026-08-03_11-58-28` | 1376 | 1.0012 | (1126, 176) | 249.7 | 175.8 |
/// | `2026-08-07_19-28-36` | 1539 | 1.1321 | (1249, 218) | 256.2 | 192.6 |
///
/// The band is **220–256 ref px** horizontally and **157–193 ref px**
/// vertically — a 36 ref px spread on both axes, which is the same band the
/// POE-169 tracker task's Delivery note recorded. The constants below sit near
/// the top of it because the two widest boards are the two the fixtures were
/// first cut from; they are a *first guess with a hard failure mode*, not a
/// locator.
///
/// # What this costs in practice
///
/// A 36 ref px spread against a 242 × 200 ref px rect is roughly 15–18% of the
/// rect on each axis, and the seals sit near its edges. So **v1 is expected to
/// fall back to `doors − uncertain` on a share of real boards** — the two
/// low-`dy` boards above are ~33 ref px off the constant, enough to clip or
/// rotate the seal fan. That is a degraded read, not a wrong one (see below),
/// but it is not rare enough to call an edge case.
///
/// # Why a wrong guess is safe
///
/// [`markers::read_door_markers`] fails on a seal count that does not equal the
/// slot's lattice degree, and [`markers::assign_markers`] fails on any seal
/// more than [`markers::MAX_RESIDUAL_DEG`] (22°) from every modelled corridor
/// direction. A centre far enough off to rotate the fan therefore produces an
/// **error**, not a confident wrong door set — and [`read_markers`] falls back
/// to `doors − uncertain` with the incident corridors surfaced as unresolved.
/// That property is what makes shipping a five-point estimate honest.
///
/// # Follow-up
///
/// The fix is a **diamond locator** — correlate the diamond's own outline the
/// way [`super::anchor`] correlates the Entrance plate, or fit its four seal
/// blobs — replacing this constant pair outright. A per-user calibration
/// captured through `temple_debug_capture`, which dumps this crop for exactly
/// that purpose, is the cheaper interim. Tracked as a POE-171 follow-up; do not
/// tune these constants against a single new screenshot without re-measuring
/// the other four.
pub const DIAMOND_DX_REF: f32 = 253.6;
/// Vertical half of [`DIAMOND_DX_REF`]'s offset — see that constant's note.
pub const DIAMOND_DY_REF: f32 = 190.4;
/// Diamond rect size in reference px. Measured 242×202 and 241×195 on the two
/// widest fixtures above; the committed crops are cut centred on the diamond,
/// so the crop IS the rect.
pub const DIAMOND_W_REF: f32 = 242.0;
/// See [`DIAMOND_W_REF`].
pub const DIAMOND_H_REF: f32 = 200.0;

/// Where to look for the side panel's diamond, given a capture size and the
/// anchor's scale.
///
/// `[x, y, w, h]`, clamped to nothing — an off-screen rect is
/// [`markers::MarkerError::RectOutsideImage`], which is a fallback like any
/// other error and not something to paper over by sliding the rect back into
/// frame (a slid rect is a wrong rect that no longer trips the gate).
///
/// # The windowed-client failure mode
///
/// The offset is taken from the CAPTURE's right edge, and
/// [`crate::capture::capture_screen`] grabs a whole monitor — the game's since
/// POE-237, the primary one before it. On a
/// fullscreen (or borderless-fullscreen) client those two edges coincide, which
/// is the case every fixture above was measured in. On a **windowed** client
/// they do not: the game draws the panel against the window's right edge, the
/// rect is computed from the monitor's, and the difference is however far the
/// window sits from the screen edge — typically far more than the 36 ref px
/// band the estimate already carries. The rect then lands on desktop or on the
/// board, [`markers::read_door_markers`] finds the wrong number of seals, and
/// the module falls back to `doors − uncertain` **permanently**, for as long as
/// the client stays windowed. That is honest (nothing is reported as settled
/// that was not) but it is also permanent: no re-arm, no re-read and no
/// re-anchor recovers it. The diamond locator in [`DIAMOND_DX_REF`]'s follow-up
/// is what closes it, because a locator does not depend on the capture's edges
/// at all.
pub fn diamond_rect(screen: (u32, u32), scale: f32) -> [i32; 4] {
    let cx = screen.0 as f32 - DIAMOND_DX_REF * scale;
    let cy = DIAMOND_DY_REF * scale;
    let w = DIAMOND_W_REF * scale;
    let h = DIAMOND_H_REF * scale;
    [
        (cx - w / 2.0).round() as i32,
        (cy - h / 2.0).round() as i32,
        w.round() as i32,
        h.round() as i32,
    ]
}

/// Settle the current room's corridors with the side panel's seals, or say why
/// not.
///
/// `Ok(set)` is the settled door set. `Err(msg)` is the fallback signal: the
/// caller then uses `doors − uncertain` and publishes the incident corridors as
/// unresolved. Both halves of that are [`slice::project`]'s job — this function
/// only decides which one applies.
pub fn read_markers(img: &DynamicImage, layout: &TempleLayout) -> Result<std::collections::BTreeSet<super::lattice::Edge>, String> {
    let Some(current) = layout.current else {
        return Err(markers::MarkerError::NoCurrentRoom.to_string());
    };
    let degree = lattice::neighbours(current).len();
    let rect = diamond_rect((img.width(), img.height()), layout.scale);
    let read = markers::read_door_markers(img, rect, degree).map_err(|e| e.to_string())?;
    markers::apply_markers(layout, &read).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// The thread
// ---------------------------------------------------------------------------

/// Wall-clock now, in unix ms, for the slice's `last_read_at`.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

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
/// The loop touches the slice on every tick and the SSOT is polled by every
/// window, so emitting an identical snapshot once a second would be pure churn.
/// The `temple` guard is dropped before `emit_ssot`, which locks the same mutex
/// to compose the snapshot.
pub fn publish(app: &AppHandle, mutate: impl FnOnce(&mut TempleSlice)) {
    let changed = {
        let state = app.state::<AppState>();
        let mut slice = state.temple.lock().unwrap_or_else(|e| e.into_inner());
        let before = slice.clone();
        mutate(&mut slice);
        *slice != before
    };
    if changed {
        crate::ssot::emit_ssot(app);
    }
}

/// A snapshot of the persisted settings. Taken per read rather than held,
/// because the commands write them from the webview thread and a read that
/// straddles a change should use one of the two, not half of each.
pub fn settings_snapshot(app: &AppHandle) -> TempleSettings {
    let state = app.state::<AppState>();
    let settings = state.temple_settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
    settings
}

/// The settings one read should use, and whether the stored hint was stale.
///
/// A calibration measured at another capture size is dropped HERE, before the
/// anchor sees it. The anchor would ignore it anyway
/// ([`super::anchor::AnchorCalibration::applies_to`]), so this is not about the
/// read — it is about the two places the dead hint would otherwise sit
/// forever: `settings.json`, and [`TempleSlice::calibration`], which the page
/// renders as the scale in force. `true` means the caller must forget it in the
/// owner and on disk too.
///
/// Pure so the rule is testable without a screen, and taken by value so the
/// loop cannot accidentally read the pre-prune settings — see [`tick`], where
/// this is the only source of a `TempleSettings`.
pub fn settings_for_capture(
    stored: &TempleSettings,
    screen: (u32, u32),
) -> (TempleSettings, bool) {
    let mut settings = stored.clone();
    let pruned = settings.prune_calibration(screen);
    (settings, pruned)
}

/// Drop the stored calibration from the OWNER only.
///
/// The pruning decision is [`settings_for_capture`]'s; this is only the clear.
///
/// Split out of [`forget_calibration`] so a caller that has its own file write
/// does not have to make two (POE-227): `ssot::geometry_recalibrate` clears
/// this alongside the shared screen scale and then writes settings.json ONCE,
/// through `settings::persist_forgetting_screen_scale`, which rebuilds the file
/// from `AppState` — this cleared owner included. Two writes there would have
/// the first one (`crate::persist_settings`, whose `preserve_screen_scale`
/// merge restores a stored measurement over an empty projection) put the stale
/// scale back on disk, correct only for as long as the second write followed.
pub(crate) fn clear_calibration(app: &AppHandle) {
    let state = app.state::<AppState>();
    state
        .temple_settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .calibration = None;
}

/// Drop the stored calibration from the owner AND from disk.
///
/// The temple tick's own path: a capture whose dimensions disagree with the
/// stored hint prunes it and has nothing else to write, so the clear and the
/// save are one call.
fn forget_calibration(app: &AppHandle) {
    clear_calibration(app);
    crate::persist_settings(app);
}

/// Store the calibration this capture measured, and persist it when it moved.
///
/// Persisting only on a change is what keeps a 1 Hz loop off the disk: the
/// scale is stable for as long as the window size is.
fn remember_calibration(app: &AppHandle, layout: &TempleLayout) {
    let changed = {
        let state = app.state::<AppState>();
        let mut settings = state.temple_settings.lock().unwrap_or_else(|e| e.into_inner());
        if settings.calibration == Some(layout.calibration) {
            false
        } else {
            settings.calibration = Some(layout.calibration);
            true
        }
    };
    if changed {
        crate::persist_settings(app);
    }
}

fn game_focused(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let focused = *state.game_focused.lock().unwrap_or_else(|e| e.into_inner());
    focused
}

fn rearm_counter(app: &AppHandle) -> u64 {
    let state = app.state::<AppState>();
    let counter = state.temple_rearm.load(std::sync::atomic::Ordering::SeqCst);
    counter
}

/// The loop's "have I already logged this?" filter.
///
/// A failure path re-runs every second and an error carrying a varying number
/// is a different string every time, so without a cap one loop could fill the
/// 50-entry LOGS buffer on its own. Separated from [`Session`] so the once-only
/// rules — once per distinct message, once for the cap itself — are testable
/// without an `AppHandle`.
#[derive(Debug, Default)]
pub struct ErrorLog {
    seen: std::collections::HashSet<String>,
    capped: bool,
}

impl ErrorLog {
    /// The line to log for `msg`, or `None` when it has already been said.
    ///
    /// Past [`MAX_DISTINCT_ERRORS`] distinct messages this stops repeating
    /// itself and says so — **once**. A silent cap is indistinguishable from a
    /// loop that stopped failing, which is the opposite of what it means; a cap
    /// that announced itself per dropped message would be the flood it exists
    /// to prevent.
    pub fn note(&mut self, msg: &str) -> Option<String> {
        if self.seen.contains(msg) {
            return None;
        }
        if self.seen.len() < MAX_DISTINCT_ERRORS {
            self.seen.insert(msg.to_string());
            return Some(msg.to_string());
        }
        if self.capped {
            return None;
        }
        self.capped = true;
        Some(format!(
            "Temple: {MAX_DISTINCT_ERRORS} distinct errors logged — further distinct errors dropped from the log (the module's last error still updates)"
        ))
    }
}

/// Everything the loop carries between ticks.
struct Session {
    state: LoopState,
    gate: slice::ReadGate,
    errors: ErrorLog,
    last_panel_check: Instant,
    /// Where the last successful read found the Entrance plate, for
    /// [`anchor::detect_cheap`] to look first.
    ///
    /// In memory, not in `settings.json`: it is a property of where the game
    /// window sits, which the capture size the persisted
    /// [`anchor::AnchorCalibration`] is keyed on does not pin. Never cleared —
    /// [`anchor::AnchorCalibration::applies_to`] discards it on a capture-size
    /// change, and a hint that is merely in the wrong PLACE costs one windowed
    /// correlation and is caught by the nominating pass on the same tick.
    cheap_hint: Option<CheapHint>,
}

fn run_loop(app: AppHandle, cancel: watch::Receiver<bool>) {
    crate::app_log(&app, "Temple: capture loop started".to_string());
    crate::report_ocr_engine(&app);

    // The user's settings belong on the slice from the first frame — the page
    // and the overlay render their own controls from them, and a zeroed key
    // count would read as "you have no keys" rather than "not loaded yet".
    let settings = settings_snapshot(&app);
    publish(&app, |slice| {
        slice.status = TempleStatus::Idle;
        slice.keys = settings.keys;
        slice.config = settings.config.clone();
        slice.profile = settings.profile.clone();
        slice.last_error = None;
    });

    if let Err(e) = crate::ocr::engine_ready() {
        return unavailable(&app, &cancel, e);
    }

    let mut session = Session {
        state: LoopState::default(),
        gate: slice::ReadGate::default(),
        errors: ErrorLog::default(),
        last_panel_check: Instant::now(),
        cheap_hint: None,
    };
    // Backdated so the first iteration ticks immediately rather than after a
    // full cadence of doing nothing.
    let mut last_detect = Instant::now() - DETECT_INTERVAL_SLOW;

    loop {
        if *cancel.borrow() {
            break;
        }

        if !game_focused(&app) {
            // No capture while alt-tabbed: the layout panel is not on screen,
            // and a full-screen anchor match every second would be pure heat.
            if !nap(&cancel, UNFOCUSED_NAP) {
                break;
            }
            continue;
        }

        if last_detect.elapsed() >= session.state.detect_interval() {
            let started = Instant::now();
            let promoted = tick(&app, &mut session, &cancel);
            last_detect = Instant::now();
            if session.state.note_tick_duration(started.elapsed(), promoted) {
                crate::app_log(
                    &app,
                    format!(
                        "Temple: detect tick took {} ms — cadence backing off to {} s",
                        started.elapsed().as_millis(),
                        DETECT_INTERVAL_SLOW.as_secs()
                    ),
                );
            }
        }

        if !nap(&cancel, TICK) {
            break;
        }
    }

    // A retired panel must not be left claiming a board is on screen.
    // Best-effort by contract: on app exit the process is gone before this
    // runs, which is why `status` — forced to `Off` by the SSOT composer once
    // the module is disabled — is what the page trusts.
    //
    // NOTE (deferred, POE-171 finding 15): a module switched off and straight
    // back on can have the retiring thread's publish land after the new
    // thread's first one, overwriting it. Inherited from `mercenary::run`,
    // which has the same shape; the shared fix is a slice generation counter
    // both loops stamp, not a change here.
    publish(&app, |slice| apply_status(slice, TickOutcome::Stopping));
    crate::app_log(&app, "Module temple: stopped".to_string());
}

/// Park the module as `unavailable` and idle until the stop signal.
///
/// The thread stays alive rather than returning so the module's running set
/// still reflects reality: it was started, it is switched on, and it is doing
/// nothing for a stated reason.
fn unavailable(app: &AppHandle, cancel: &watch::Receiver<bool>, reason: String) {
    crate::app_log(app, format!("Temple: capture unavailable — {reason}"));
    publish(app, |slice| {
        slice.status = TempleStatus::Unavailable;
        slice.last_error = Some(reason.clone());
    });
    while nap(cancel, UNFOCUSED_NAP) {}
    crate::app_log(app, "Module temple: stopped".to_string());
}

/// Log `msg` the first time this loop sees it, and record it as `last_error`.
///
/// `last_error` is written whatever [`ErrorLog`] decides about the log: it is
/// one field, not a buffer, so there is nothing to flood.
fn fail(app: &AppHandle, session: &mut Session, msg: String) {
    if let Some(line) = session.errors.note(&msg) {
        crate::app_log(app, line);
    }
    publish(app, |slice| {
        apply_status(slice, TickOutcome::Failed);
        slice.last_error = Some(msg);
    });
}

/// One detect tick: grab the screen, ask [`anchor::detect_cheap`] whether
/// anything is there, and read the board when it is.
///
/// Returns whether this tick paid for the full read — the caller times only the
/// ticks that did not, per [`SLOW_TICK`].
fn tick(app: &AppHandle, session: &mut Session, cancel: &watch::Receiver<bool>) -> bool {
    let grab = match crate::capture::capture_screen(app) {
        Ok(grab) => grab,
        Err(e) => {
            fail(app, session, format!("Temple: screen capture failed — {e}"));
            miss(app, session, true);
            return false;
        }
    };
    let img = grab.image;
    // Before ANY remembered geometry is read (POE-227): a screen scale measured
    // on another monitor is dropped from the shared slice on the first capture
    // whose dimensions disagree with it. The temple does not consume that slice
    // yet — its unit ratio is unmeasured (see `ssot::ScreenSlice`) — but it does
    // capture screens, and the prune belongs to whichever module looks first.
    crate::ssot::drop_if_mismatched(app, (img.width(), img.height()), grab.monitor_id);
    // The ONLY place the loop obtains its settings, so the stale-hint prune
    // cannot be skipped without the compile failing.
    let (settings, pruned) = settings_for_capture(&settings_snapshot(app), (img.width(), img.height()));
    if pruned {
        forget_calibration(app);
        crate::app_log(
            app,
            format!(
                "Temple: capture is now {}×{} — dropping the remembered calibration",
                img.width(),
                img.height()
            ),
        );
    }
    // The cheap gate. A closed panel is what this loop looks at nearly all the
    // time, and it is the most expensive input the reader has — see
    // `anchor::detect_cheap`, which answers "anything here?" for ~1/80 of the
    // price of finding out the long way.
    let rearm = rearm_counter(app);
    let cheap = anchor::detect_cheap(&img, session.cheap_hint.as_ref());
    if !wants_full_read(&mut session.state, &mut session.gate, &cheap, rearm) {
        miss(app, session, false);
        return false;
    }

    // A cheap tick that anchored has already done the expensive half of the
    // read's own first step, at full resolution and against the same floor —
    // so the promoted read takes that anchor instead of finding the plate a
    // second time. Every other promotion has no anchor to hand over.
    let layout = match cheap {
        anchor::CheapDetect::Anchored(found) => reader::read_layout_at(&img, found),
        _ => match reader::read_layout_with_hint(&img, settings.calibration.as_ref()) {
            Ok(layout) => layout,
            Err(_) => {
                // Not an error path: "no layout panel on screen" is the state
                // the loop spends most of its life in, and reporting it as a
                // failure would put a permanent red line on the page.
                // `AnchorNotFound` carries its best NCC, which IS worth seeing
                // when the panel never anchors — but it varies per frame, so it
                // belongs in `temple_debug_capture`'s report rather than in a
                // log line the loop would rewrite every second. A panel that
                // anchors but reads badly is a different case, and reaches the
                // slice as `layout.confidence`.
                miss(app, session, false);
                // This tick DID pay for the read; it just found nothing.
                return true;
            }
        },
    };

    if session.state.on_detect(true) == DetectOutcome::Found {
        crate::app_log(
            app,
            format!(
                "Temple: layout panel found (scale {:.3}, NCC {:.3})",
                layout.scale, layout.ncc
            ),
        );
    }
    remember_calibration(app, &layout);
    session.cheap_hint = Some(CheapHint {
        calibration: layout.calibration,
        origin: layout.origin,
    });

    let layout_sig = slice::layout_signature(&layout);
    if session.gate.layout_wants_read(layout_sig, rearm) {
        full_read(app, session, cancel, &img, layout, &settings, layout_sig, None);
        return true;
    }

    // The board looks the same. Pay for the text gate only on its own, slower
    // cadence — and only after checking the stop signal, because two OCR calls
    // are not something a cancelled thread should still be buying.
    if session.last_panel_check.elapsed() < PANEL_RECHECK_INTERVAL || *cancel.borrow() {
        return true;
    }
    session.last_panel_check = Instant::now();
    let Some(lines) = panel_text(app, session, &img, &layout) else {
        return true;
    };
    let read = panel::read_panel(&lines);
    if session.gate.panel_wants_read(slice::panel_signature(&read)) {
        full_read(app, session, cancel, &img, layout, &settings, layout_sig, Some(read));
    }
    true
}

/// A tick that produced no layout — nothing on screen, or the grab failed.
///
/// A failed grab counts as a failed DETECTION rather than as its own kind of
/// event: the loop cannot see the panel either way, and a board held alive
/// through repeated failures would leave the page showing advice for a panel
/// that closed two minutes ago.
///
/// `errored` is what keeps [`fail`]'s message on the page: a clean miss — the
/// loop looked and there was no panel — means the last error is over, and
/// clearing it there is what stops a one-off capture failure sitting on the
/// page for the rest of the session. [`next_status`] moves the status in the
/// same publish, because clearing the message while leaving `error` standing
/// leaves the page red with nothing under it.
fn miss(app: &AppHandle, session: &mut Session, errored: bool) {
    let retired = session.state.on_detect(false) == DetectOutcome::Retired;
    if retired {
        // The panel left the screen: the next one is a new decision even if it
        // looks identical. This is what makes "close, kill, reopen" re-read.
        session.gate.on_panel_lost();
        crate::app_log(app, "Temple: layout panel gone".to_string());
    }
    if !retired && errored {
        // Nothing to say: `fail` has already written the status and the
        // message, and the board stands until it has been missed twice.
        return;
    }
    let outcome = if errored {
        TickOutcome::Failed
    } else {
        TickOutcome::NoPanel
    };
    publish(app, |slice| {
        apply_status(slice, outcome);
        if retired {
            // The advice goes, the layout and panel stay. A recommendation is
            // a move the player could still act on; the board is a record of
            // what was last read, and keeping it is what lets the overlay stay
            // useful for the seconds after the panel closes.
            slice.advice = None;
            slice.mode = None;
        }
    });
}

/// The panel's text: two bounded crops, as plain lines.
///
/// Never the whole frame — see the module note. The side panel's region comes
/// first and the budget line second, which is the order they are drawn in and
/// the order [`panel::read_panel`]'s positional title rule reads them in.
///
/// A crop that lands outside the capture contributes nothing rather than
/// failing the read: the two regions are independent, and losing the budget
/// line costs a warning, not the board.
fn panel_text(
    app: &AppHandle,
    session: &mut Session,
    img: &DynamicImage,
    layout: &TempleLayout,
) -> Option<Vec<String>> {
    let regions = [
        panel_rect((img.width(), img.height()), layout.scale),
        remaining_rect(layout.origin, layout.scale),
    ];
    let mut lines = Vec::new();
    for rect in regions {
        let Some(crop) = crop_clipped(img, rect) else {
            continue;
        };
        let prepared = crate::capture::preprocess_for_ocr(&crop);
        match crate::ocr::recognize_lines(&prepared) {
            Ok(read) => lines.extend(read.into_iter().map(|l| l.text)),
            Err(e) => {
                fail(app, session, format!("Temple: OCR failed — {e}"));
                return None;
            }
        }
    }
    Some(lines)
}

/// The expensive half: 13 plates, the side panel, the diamond, the advisor.
/// Screen height the shared `ui_scale` unit calls 1.0 — the height of the merc
/// reference fixture, restated here rather than imported so this file does not
/// grow a dependency on a slice it deliberately does not read. The number's
/// owner is [`crate::ssot::ScreenSlice`]'s unit note.
const UI_SCALE_REFERENCE_HEIGHT: f32 = 1200.0;

/// The line POE-227 D3 exists to print: `k`, the ratio between the temple's own
/// scale unit and the shared `ui_scale` unit, on a board that actually anchored.
///
/// The two units are measured against different references — the temple's
/// against `anchor::REFERENCE_SCREEN_WIDTH` (1374, a WIDTH), the slice's against
/// a 1920x1200 fixture whose scale tracks HEIGHT — and nothing in the repo can
/// relate them offline, because every temple fixture is a crop of a panel rather
/// than a whole screen. So the ratio is measured in play: one temple session
/// prints this line, `k` is read off it, and the constant lands in a follow-up
/// that switches the temple onto the shared slice. Until then a reader must NOT
/// substitute one scale for the other.
///
/// Pure, and separated from the log call, so the arithmetic and the format are
/// testable without a screen or an `AppHandle`.
fn unit_ratio_line(scale: f32, capture_width: u32, capture_height: u32) -> String {
    let k = scale / (capture_height as f32 / UI_SCALE_REFERENCE_HEIGHT);
    format!(
        "temple unit ratio k={k:.4} (scale {scale:.3}, capture {capture_width}x{capture_height})"
    )
}

fn full_read(
    app: &AppHandle,
    session: &mut Session,
    cancel: &watch::Receiver<bool>,
    img: &DynamicImage,
    layout: TempleLayout,
    settings: &TempleSettings,
    layout_sig: u64,
    already_read: Option<panel::PanelReading>,
) {
    publish(app, |slice| apply_status(slice, TickOutcome::Anchored));
    // POE-227 D3 instrumentation, and the ONLY thing this batch does about the
    // temple's unit: print the ratio between the two scales on a real board so
    // the constant can be read off a live session's log. Nothing consumes it —
    // the temple still measures and stores its own scale, and still does not
    // write the shared screen slice.
    crate::app_log(
        app,
        unit_ratio_line(layout.scale, img.width(), img.height()),
    );

    let panel = match already_read {
        Some(panel) => panel,
        None => match panel_text(app, session, img, &layout) {
            Some(lines) => panel::read_panel(&lines),
            None => return,
        },
    };
    let panel_sig = slice::panel_signature(&panel);

    // 26 more OCR calls follow — two per plate. A stop that arrived during the
    // text OCR must not buy them: this is the loop's longest blocking stretch
    // and a detached thread cannot be aborted out of it. The check is passed
    // INTO `read_board` as well, so a stop lands between two plate crops rather
    // than after all 26. Bailing leaves the gate unrecorded, so the next start
    // re-reads from scratch.
    if *cancel.borrow() {
        return;
    }
    let lattice = Lattice::new(layout.origin, layout.scale);
    let stop = || *cancel.borrow();
    let rooms = panel::read_board(&SystemOcr, img, &lattice, &stop);
    if *cancel.borrow() {
        return;
    }

    // Both name-sources for the room the player is standing in read, and they
    // disagree: the advice carries a warning for the overlay, and this puts the
    // same fact in the app log, which is what a user can send back. `log::` is
    // not that — it goes nowhere under `windows_subsystem = "windows"`.
    if let Some((title, plate)) =
        slice::current_identity(layout.current, &slice::identities(&rooms), &panel).disagreement
    {
        crate::app_log(
            app,
            format!(
                "Temple: side panel says {title:?} but the current plate says {plate:?}; using the plate"
            ),
        );
    }

    let (settled, marker_error) = match read_markers(img, &layout) {
        Ok(set) => (Some(set), None),
        Err(e) => (None, Some(e)),
    };
    let advice = slice::advise_read(&layout, &rooms, &panel, settled.as_ref(), settings);

    let projected = slice::project(
        &slice::ReadResult {
            layout: &layout,
            rooms: &rooms,
            panel: &panel,
            settled: settled.as_ref(),
            marker_error,
            advice: advice.as_ref(),
            // The settings THIS tick started with. A setter that lands
            // mid-read echoes its new value onto the slice and then loses it
            // again for one tick when this projection overwrites it — the
            // setters' own `rearm` forces the next read, which restores it.
            keys: settings.keys,
            config: settings.config.clone(),
            profile: settings.profile.clone(),
            read_at: now_ms(),
        },
        // The calibration THIS capture measured, not the one the snapshot was
        // taken with: on the first read after a resolution change the snapshot
        // still carries the stale hint `remember_calibration` has just
        // replaced, and publishing that would show the user a scale the reader
        // no longer uses.
        Some(layout.calibration),
    );
    session.gate.record(layout_sig, panel_sig);
    publish(app, |slice| *slice = projected);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::MODULE_THREAD_POLL_CEILING;

    /// The loop's stop discipline, as a number rather than as a comment: a
    /// detached thread can outlive its stop by at most one [`TICK`]. Fails if
    /// the quantum is raised past the registry's ceiling.
    #[test]
    fn the_poll_quantum_stays_inside_the_registry_ceiling() {
        assert!(
            TICK <= MODULE_THREAD_POLL_CEILING,
            "a thread module must poll cancel at least every {MODULE_THREAD_POLL_CEILING:?}",
        );
    }

    /// The measurement POE-227 D3 exists to take. On the reference screen the
    /// shared unit is 1.0 by definition, so `k` is the temple's own scale
    /// unchanged — which makes this the case that pins the DIVISOR: an
    /// instrumentation line that divided by the width, or by the wrong
    /// reference, would print a number the follow-up commit would then bake in
    /// as a wrong constant, with nothing downstream to contradict it.
    #[test]
    fn on_the_reference_screen_the_unit_ratio_is_the_temple_scale_itself() {
        let line = unit_ratio_line(0.9600, 1920, 1200);

        assert_eq!(line, "temple unit ratio k=0.9600 (scale 0.960, capture 1920x1200)");
    }

    /// The case the ratio is FOR: a 1080p screen, where the shared unit is 0.90
    /// and the temple's is not, so `k` and the scale visibly differ. Fails if
    /// the height is ignored — which on the reference screen alone would look
    /// exactly right.
    #[test]
    fn a_shorter_screen_scales_the_unit_ratio_up() {
        // 0.864 / (1080 / 1200) = 0.864 / 0.9 = 0.96.
        let line = unit_ratio_line(0.864, 1920, 1080);

        assert_eq!(line, "temple unit ratio k=0.9600 (scale 0.864, capture 1920x1080)");
    }

    /// A panel is not retired on its first miss — the anchor loses a fading
    /// panel for a frame. Fails if `RETIRE_AFTER` is applied off by one.
    #[test]
    fn a_live_panel_survives_one_missed_anchor() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        assert_eq!(state.on_detect(false), DetectOutcome::Missed);
        assert!(state.live, "one miss does not retire a panel");
        assert_eq!(state.on_detect(false), DetectOutcome::Retired);
        assert!(!state.live);
    }

    /// A successful anchor between two misses resets the counter, so a panel
    /// that flickers is never retired. Fails if `on_detect` does not clear
    /// `misses` on success.
    #[test]
    fn an_anchor_between_misses_resets_the_retirement_count() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        state.on_detect(false);
        assert_eq!(state.on_detect(true), DetectOutcome::Held);
        assert_eq!(state.on_detect(false), DetectOutcome::Missed, "the count restarted");
        assert!(state.live);
    }

    /// The first anchor after nothing is `Found`, which is the log line.
    #[test]
    fn the_first_anchor_reports_found_and_later_ones_do_not() {
        let mut state = LoopState::default();

        assert_eq!(state.on_detect(true), DetectOutcome::Found);
        assert_eq!(state.on_detect(true), DetectOutcome::Held);
    }

    /// The backoff is sticky and announces itself exactly once. Fails if
    /// `note_tick_duration` re-announces, or if it fires on a fast tick.
    #[test]
    fn the_slow_tick_backoff_fires_once_and_stays() {
        let mut state = LoopState::default();

        assert!(!state.note_tick_duration(Duration::from_millis(200), false));
        assert_eq!(state.detect_interval(), DETECT_INTERVAL);

        assert!(state.note_tick_duration(SLOW_TICK + Duration::from_millis(1), false));
        assert_eq!(state.detect_interval(), DETECT_INTERVAL_SLOW);
        assert!(
            !state.note_tick_duration(SLOW_TICK + Duration::from_millis(1), false),
            "the backoff announces itself once",
        );
        assert!(
            !state.note_tick_duration(Duration::from_millis(10), false),
            "and does not come back off",
        );
    }

    /// A tick that paid for the full read is not evidence about the cadence.
    /// The periodic backstop costs seconds by design, so one of those must not
    /// trip a backoff that is sticky for the life of the thread.
    ///
    /// Fails if the promoted tick is timed like a cheap one — every 30th tick
    /// would then permanently slow a loop that is running perfectly well.
    #[test]
    fn a_slow_promoted_tick_does_not_back_the_cadence_off() {
        let mut state = LoopState::default();

        assert!(!state.note_tick_duration(SLOW_TICK * 3, true));

        assert!(!state.backed_off);
        assert_eq!(state.detect_interval(), DETECT_INTERVAL);
    }

    // ------------------------------------------------ the cheap detect gate --

    /// A cheap outcome that saw nothing, for the gate tests.
    fn saw_nothing() -> anchor::CheapDetect {
        anchor::CheapDetect::Nothing { best_ncc: 0.2 }
    }

    /// A cheap outcome that nominated something.
    fn saw_something() -> anchor::CheapDetect {
        anchor::CheapDetect::Candidate { coarse_ncc: 0.94 }
    }

    /// The bug the composition exists to prevent: the settings commands re-arm
    /// on every change, and a re-arm pressed while no panel is on screen must
    /// buy ONE full read — not pin the loop into one on every tick for the rest
    /// of the session.
    ///
    /// Fails if the promoting tick does not spend the bump, which is what
    /// happens when the only writer is `ReadGate::layout_wants_read`: that runs
    /// after a read that SUCCEEDED, and a read over a closed panel does not.
    #[test]
    fn a_rearm_with_no_panel_on_screen_buys_exactly_one_full_read() {
        let mut state = LoopState::default();
        let mut gate = slice::ReadGate::default();

        assert!(
            !wants_full_read(&mut state, &mut gate, &saw_nothing(), 0),
            "precondition: a quiet screen is a cheap tick",
        );

        assert!(
            wants_full_read(&mut state, &mut gate, &saw_nothing(), 1),
            "the bump buys a read",
        );
        assert!(
            !wants_full_read(&mut state, &mut gate, &saw_nothing(), 1),
            "and exactly one — the tick after it is cheap again",
        );
        assert!(!wants_full_read(&mut state, &mut gate, &saw_nothing(), 1));
    }

    /// …and the read it forced actually happens. Fails if spending the bump
    /// records the counter without dropping the recorded read — the promoted
    /// tick would then match its own fingerprint and skip, so the button would
    /// cost a full read and change nothing on screen.
    #[test]
    fn the_read_a_rearm_forced_is_not_skipped_as_unchanged() {
        let mut state = LoopState::default();
        let mut gate = slice::ReadGate::default();
        let (board, panel) = (77u64, 88u64);
        gate.record(board, panel);
        assert!(!gate.layout_wants_read(board, 0), "precondition: already read");

        assert!(wants_full_read(&mut state, &mut gate, &saw_something(), 1));

        assert!(
            gate.layout_wants_read(board, 1),
            "the board the re-arm was pressed over must be read again",
        );
    }

    /// A panel already on screen is read whatever the cheap tick says. It is
    /// the CHEAP input, and the case this covers is the one the cheap tick is
    /// blind to: an open panel whose UI scale drifted, with a hint that no
    /// longer matches.
    ///
    /// Fails if the promotion is gated on the cheap outcome alone — two such
    /// ticks would then retire a panel that is on screen, and the board would
    /// disappear until the periodic backstop 30 ticks later.
    #[test]
    fn a_live_panel_is_read_even_when_the_cheap_tick_sees_nothing() {
        let mut state = LoopState {
            live: true,
            ..LoopState::default()
        };
        let mut gate = slice::ReadGate::default();

        assert!(wants_full_read(&mut state, &mut gate, &saw_nothing(), 0));
    }

    /// A cheap tick that saw something buys the full read. Fails if the
    /// promotion on a detection is dropped — the loop would then only read on
    /// the periodic backstop, i.e. up to 30 s after the panel opened.
    #[test]
    fn a_cheap_detect_that_saw_something_promotes_to_the_full_read() {
        let mut state = LoopState::default();

        assert!(state.note_cheap_detect(true, false));
    }

    /// Re-arm promotes even while the cheap tick sees nothing. Fails if the
    /// button is only honoured behind a detection, which would make it dead on
    /// exactly the tick the user pressed it for.
    #[test]
    fn a_rearm_promotes_while_the_cheap_tick_sees_nothing() {
        let mut state = LoopState::default();

        assert!(state.note_cheap_detect(false, true));
    }

    /// The backstop fires on the Nth consecutive miss and NOT before it, and
    /// the count restarts afterwards.
    ///
    /// Fails if the periodic path is removed (a panel the cheap tick cannot see
    /// — a UI-scale change — would then never be read again), if it is off by
    /// one, or if the counter is not reset (the backstop would fire on every
    /// tick from the Nth onwards, which is the cost this whole gate removes).
    #[test]
    fn only_the_nth_consecutive_cheap_miss_promotes_and_the_count_restarts() {
        let mut state = LoopState::default();

        for i in 1..FULL_READ_EVERY_N_MISSES {
            assert!(
                !state.note_cheap_detect(false, false),
                "miss {i} of {FULL_READ_EVERY_N_MISSES} must not promote",
            );
        }
        assert!(
            state.note_cheap_detect(false, false),
            "the {FULL_READ_EVERY_N_MISSES}th miss is the backstop",
        );

        assert!(
            !state.note_cheap_detect(false, false),
            "the count restarts, so the tick after the backstop is cheap again",
        );
    }

    /// A promotion for any reason restarts the backstop's count. Fails if
    /// `cheap_misses` is only cleared on the periodic path — a panel that is
    /// open and being read would still drag the counter up to N and buy a
    /// redundant forced read.
    #[test]
    fn a_detection_restarts_the_backstops_count() {
        let mut state = LoopState::default();
        for _ in 1..FULL_READ_EVERY_N_MISSES {
            state.note_cheap_detect(false, false);
        }

        assert!(state.note_cheap_detect(true, false), "precondition: a detection");

        assert!(
            !state.note_cheap_detect(false, false),
            "the next miss is the FIRST of a new run, not the backstop",
        );
    }

    /// The diamond rect is measured from the capture's RIGHT edge and scales
    /// with the anchor, so the same panel maps to the same reference offset at
    /// two UI scales. Reproduces the two widest rows of [`DIAMOND_DX_REF`]'s
    /// table — the two the constants were fitted to — at the scales
    /// `read_layout` recovers from those source screenshots.
    ///
    /// The other three rows are deliberately NOT asserted here: they are
    /// 20–33 ref px from the constant, which is the whole point of that
    /// constant's "expected to fall back on a share of real boards" note.
    ///
    /// NOTE (deferred, POE-171 finding 16): the ±7 px tolerance is wider than
    /// the rounding it exists for and would swallow a small constant drift. It
    /// tightens with the diamond LOCATOR, not before — re-fitting the tolerance
    /// against a two-point estimate would only pin the estimate harder.
    #[test]
    fn the_diamond_rect_reproduces_both_measured_centres() {
        let [x, y, w, h] = diamond_rect((1374, 862), 1.0000);
        let centre = (x + w / 2, y + h / 2);
        assert!(
            (centre.0 - 1126).abs() <= 7 && (centre.1 - 186).abs() <= 7,
            "1374px board: expected a centre near (1126, 186), got {centre:?}",
        );

        let [x, y, w, h] = diamond_rect((1539, 968), 1.1321);
        let centre = (x + w / 2, y + h / 2);
        assert!(
            (centre.0 - 1249).abs() <= 7 && (centre.1 - 218).abs() <= 7,
            "1539px board: expected a centre near (1249, 218), got {centre:?}",
        );
    }

    /// The rect scales with the UI, both in position and in size. Fails if a
    /// constant is applied unscaled — which would put the rect in the right
    /// place on a 1374px client and nowhere near it on a 4K one.
    #[test]
    fn the_diamond_rect_scales_with_the_anchor() {
        let small = diamond_rect((1374, 773), 1.0);
        let large = diamond_rect((2748, 1546), 2.0);

        assert_eq!(large[2], small[2] * 2, "the rect's width scales");
        assert_eq!(large[3], small[3] * 2, "the rect's height scales");
        // ±1 px: the rect is rounded to integers and its centre is recovered
        // by integer halving, so doubling the scale can shed a pixel of
        // rounding. Anything larger is a constant applied unscaled.
        let small_offset = 1374 - (small[0] + small[2] / 2);
        let large_offset = 2748 - (large[0] + large[2] / 2);
        assert!(
            (large_offset - 2 * small_offset).abs() <= 1,
            "the offset from the right edge must scale too: {small_offset} → {large_offset}",
        );
    }

    // ---------------------------------------------------- status machine --

    /// The bug this machine exists for: one transient capture failure while no
    /// panel is on screen leaves `error` on the page for the rest of the
    /// session. The next clean miss ends it — status AND message together.
    ///
    /// Fails if `next_status` clears the message without moving the status
    /// (the page then shows a red `error` with nothing under it), or moves the
    /// status without clearing the message.
    #[test]
    fn a_clean_miss_after_an_error_ends_both_the_status_and_the_message() {
        let mut slice = TempleSlice {
            status: TempleStatus::Error,
            last_error: Some("Temple: screen capture failed — no monitor".to_string()),
            ..TempleSlice::default()
        };

        apply_status(&mut slice, TickOutcome::NoPanel);

        assert_eq!(slice.status, TempleStatus::PanelNotVisible);
        assert_eq!(slice.last_error, None);
    }

    /// The read path ends it too: a panel that anchors after an error clears
    /// the message on the way into the read, and `project` lands `read`.
    /// Fails if the error survives into a successful read.
    #[test]
    fn a_read_after_an_error_ends_it() {
        let mut slice = TempleSlice {
            status: TempleStatus::Error,
            last_error: Some("Temple: OCR failed — engine missing".to_string()),
            ..TempleSlice::default()
        };

        apply_status(&mut slice, TickOutcome::Anchored);

        assert_eq!(slice.status, TempleStatus::Reading);
        assert_eq!(slice.last_error, None, "the read is the error being over");
    }

    /// The negative case that keeps the clearing honest: a tick that FAILED
    /// does not clear the message `fail` is about to write. Fails if
    /// `clear_error` is unconditional, which would erase every error one line
    /// after writing it.
    #[test]
    fn a_failed_tick_does_not_clear_its_own_message() {
        let mut slice = TempleSlice {
            status: TempleStatus::PanelNotVisible,
            last_error: Some("Temple: screen capture failed — no monitor".to_string()),
            ..TempleSlice::default()
        };

        apply_status(&mut slice, TickOutcome::Failed);

        assert_eq!(slice.status, TempleStatus::Error);
        assert!(slice.last_error.is_some());
    }

    /// `Unavailable` is not a tick result — it means capture or OCR is missing
    /// for the life of the process. Fails if a tick outcome can move it, which
    /// would make the parked loop claim it is watching for a panel.
    #[test]
    fn an_unavailable_module_is_not_moved_by_a_tick() {
        for outcome in [
            TickOutcome::Failed,
            TickOutcome::NoPanel,
            TickOutcome::Anchored,
            TickOutcome::Stopping,
        ] {
            let mut slice = TempleSlice {
                status: TempleStatus::Unavailable,
                last_error: Some("no OCR engine".to_string()),
                ..TempleSlice::default()
            };

            apply_status(&mut slice, outcome);

            assert_eq!(slice.status, TempleStatus::Unavailable, "moved by {outcome:?}");
            assert!(slice.last_error.is_some(), "cleared by {outcome:?}");
        }
    }

    /// A stopped loop must not leave a board claiming to be current. Fails if
    /// the shutdown publish stops going through the machine and re-grows its
    /// own copy of the `Unavailable` rule.
    #[test]
    fn a_stopping_loop_falls_back_to_idle() {
        let mut slice = TempleSlice {
            status: TempleStatus::Read,
            ..TempleSlice::default()
        };

        apply_status(&mut slice, TickOutcome::Stopping);

        assert_eq!(slice.status, TempleStatus::Idle);
    }

    // --------------------------------------------------------- error log --

    /// A failure path that re-runs every second says each thing once. Fails if
    /// the filter stops de-duplicating, which would let one loop flush the
    /// 50-entry LOGS buffer on its own.
    #[test]
    fn a_repeated_error_is_logged_once() {
        let mut log = ErrorLog::default();

        assert_eq!(log.note("capture failed"), Some("capture failed".to_string()));
        assert_eq!(log.note("capture failed"), None);
        assert_eq!(log.note("OCR failed"), Some("OCR failed".to_string()));
    }

    /// The cap announces itself, once, and then goes quiet. A silent cap is
    /// indistinguishable from a loop that stopped failing.
    ///
    /// Fails if the cap is silent, if it re-announces per dropped message (the
    /// flood it exists to prevent), or if the filter keeps growing past the
    /// cap — the last is asserted through the announcement, which can only
    /// fire once the set has stopped accepting.
    #[test]
    fn the_distinct_error_cap_announces_itself_exactly_once() {
        let mut log = ErrorLog::default();
        for i in 0..MAX_DISTINCT_ERRORS {
            assert_eq!(
                log.note(&format!("error {i}")),
                Some(format!("error {i}")),
                "everything up to the cap is logged verbatim",
            );
        }

        let announced = log
            .note("one too many")
            .expect("the cap must say it has been reached");
        assert!(
            announced.contains("dropped"),
            "the line must say messages are now being dropped, got {announced:?}",
        );

        assert_eq!(log.note("another new one"), None, "announced once, not per message");
        assert_eq!(log.note("error 0"), None, "a known message is still de-duplicated");
    }

    // -------------------------------------------------------- text ROIs --

    /// The whole point of the ROIs: no OCR crop is a function of the monitor's
    /// area. `preprocess_for_ocr` upscales 2× unconditionally, so a full-frame
    /// crop on 4K is a 33 Mpx buffer per tick.
    ///
    /// Fails if either rect is ever computed from the capture rather than from
    /// the anchor scale — the 4K rects below would then be ~9× the reference
    /// ones instead of ~7.8× (the scale ratio squared).
    #[test]
    fn both_text_rois_stay_a_fixed_size_in_reference_px() {
        // 4K at the same UI scale ratio the reference board was measured at.
        let uhd_scale = 3840.0 / 1374.0;

        let [_, _, pw, ph] = panel_rect((1374, 862), 1.0);
        let [_, _, uw, uh] = panel_rect((3840, 2160), uhd_scale);
        assert_eq!((pw, ph), (540, 430), "the reference-scale panel ROI is the measured box");
        assert!(
            (uw as f32 / pw as f32 - uhd_scale).abs() < 0.01
                && (uh as f32 / ph as f32 - uhd_scale).abs() < 0.01,
            "the panel ROI must scale with the anchor, not with the frame: {uw}×{uh}",
        );
        assert!(
            (uw as u64 * uh as u64) < (3840 * 2160) / 4,
            "a bounded ROI must be a small fraction of a 4K frame, got {uw}×{uh}",
        );

        let [_, _, rw, rh] = remaining_rect((673, 682), 1.0);
        let [_, _, urw, urh] = remaining_rect((1880, 1900), uhd_scale);
        assert_eq!((rw, rh), (300, 46), "the reference-scale budget ROI is the measured box");
        assert!(
            (urw as f32 / rw as f32 - uhd_scale).abs() < 0.01
                && (urh as f32 / rh as f32 - uhd_scale).abs() < 0.02,
            "the budget ROI must scale with the anchor: {urw}×{urh}",
        );
    }

    /// The panel ROI covers the panel on both measured boards, with the margin
    /// its constant claims. Reproduces the measurements in
    /// [`PANEL_LEFT_REF`]'s table.
    ///
    /// Fails if the region stops being anchored to the capture's RIGHT edge, or
    /// if a constant is trimmed below what the panel actually occupies.
    #[test]
    fn the_panel_roi_contains_both_measured_panels() {
        // (capture size, scale, measured panel box `[left, top, right, bottom]`)
        let boards = [
            ((1374u32, 862u32), 1.0000f32, [884, 13, 1368, 387]),
            ((1539, 968), 1.1321, [980, 24, 1517, 440]),
        ];
        for (screen, scale, [left, top, right, bottom]) in boards {
            let [x, y, w, h] = panel_rect(screen, scale);
            assert!(
                x <= left && y <= top && x + w >= right && y + h >= bottom,
                "{screen:?}: ROI {:?} does not contain the panel [{left}, {top}, {right}, {bottom}]",
                [x, y, w, h],
            );
        }
    }

    /// The budget ROI covers the `N Incursions Remaining` glyph box on both
    /// measured boards. Keyed on the Entrance centre, not on a screen edge —
    /// so unlike the panel it survives a windowed client.
    ///
    /// Fails if the vertical band is narrowed onto the Entrance plate's own
    /// bottom border (+42 ref px), which would put the plate's name in the
    /// crop and the budget line out of it.
    #[test]
    fn the_budget_roi_contains_both_measured_lines() {
        // (origin, scale, measured glyph box `[left, top, right, bottom]`)
        let boards = [
            ((673i32, 682i32), 1.0000f32, [568, 756, 777, 770]),
            ((745, 768), 1.1321, [631, 851, 858, 865]),
        ];
        for (origin, scale, [left, top, right, bottom]) in boards {
            let [x, y, w, h] = remaining_rect(origin, scale);
            assert!(
                x <= left && y <= top && x + w >= right && y + h >= bottom,
                "{origin:?}: ROI {:?} does not contain the line [{left}, {top}, {right}, {bottom}]",
                [x, y, w, h],
            );
            assert!(
                y > origin.1 + (42.0 * scale) as i32,
                "the ROI must start below the Entrance plate, got y={y}",
            );
        }
    }

    /// A rect hanging off the frame is clipped, not refused: the readable half
    /// is still readable, and there is no count or angle here for a bad rect to
    /// corrupt. A rect entirely outside is `None`, because
    /// `preprocess_for_ocr` cannot take a zero-sized image.
    #[test]
    fn a_text_rect_off_the_frame_is_clipped_and_one_fully_outside_is_none() {
        let img = DynamicImage::new_rgb8(100, 80);

        let clipped = crop_clipped(&img, [-20, -10, 60, 40]).expect("the overlap is readable");
        assert_eq!((clipped.width(), clipped.height()), (40, 30));

        assert!(crop_clipped(&img, [100, 0, 40, 40]).is_none(), "no overlap, no crop");
        assert!(crop_clipped(&img, [0, -50, 40, 40]).is_none(), "no overlap, no crop");
    }

    // ------------------------------------------------ calibration pruning --

    /// A capture at a new size reads WITHOUT the remembered scale, and says so
    /// so the caller can drop it from disk. The anchor would ignore it anyway;
    /// what this stops is a dead hint sitting in `settings.json` and in the
    /// slice's published `calibration` forever.
    ///
    /// Fails if `settings_for_capture` hands the stored settings back
    /// unpruned — which is exactly what "the loop forgot to prune" looks like,
    /// since `tick` has no other source of a `TempleSettings`.
    #[test]
    fn a_capture_at_a_new_size_reads_without_the_stale_hint() {
        let stored = TempleSettings {
            calibration: Some(crate::temple::anchor::AnchorCalibration {
                screen_w: 1374,
                screen_h: 862,
                scale: 1.0,
            }),
            ..TempleSettings::shipped()
        };

        let (settings, pruned) = settings_for_capture(&stored, (1539, 968));

        assert!(pruned, "the caller must be told to forget it on disk too");
        assert_eq!(settings.calibration, None, "the read must not carry it");
        assert_eq!(
            settings.keys, stored.keys,
            "only the calibration is pruned",
        );
    }

    /// The same capture size keeps the hint — the speed-up this whole
    /// mechanism exists for. Fails if the prune fires unconditionally, which
    /// would make every tick pay for a full scale sweep and rewrite
    /// `settings.json` once a second.
    #[test]
    fn a_capture_at_the_remembered_size_keeps_the_hint() {
        let stored = TempleSettings {
            calibration: Some(crate::temple::anchor::AnchorCalibration {
                screen_w: 1374,
                screen_h: 862,
                scale: 1.0,
            }),
            ..TempleSettings::shipped()
        };

        let (settings, pruned) = settings_for_capture(&stored, (1374, 862));

        assert!(!pruned);
        assert_eq!(settings.calibration, stored.calibration);
    }

    /// A board read between rooms has no diamond to read. Fails if
    /// `read_markers` builds a rect and calls the detector anyway — which
    /// would report a seal-count mismatch instead of the real reason.
    #[test]
    fn markers_are_not_read_between_rooms() {
        let img = DynamicImage::new_rgb8(1374, 773);
        let layout = TempleLayout {
            origin: (673, 494),
            scale: 0.99,
            ncc: 0.94,
            confidence: crate::temple::doors::Confidence::High,
            current: None,
            doors: Default::default(),
            uncertain: Default::default(),
            slots: [(0, 0); 13],
            thresholds: crate::temple::doors::Thresholds { horizontal: 0.2, diagonal: 0.2 },
            calibration: crate::temple::anchor::AnchorCalibration {
                screen_w: 1374,
                screen_h: 773,
                scale: 0.99,
            },
        };

        let err = read_markers(&img, &layout).expect_err("no current room, no diamond");
        assert!(
            err.contains("no current room"),
            "the failure must name the real reason, got {err:?}",
        );
    }

    /// A blank capture has no seals, so the diamond read fails rather than
    /// returning an empty (i.e. "everything closed") door set. This is the
    /// property the whole fallback rests on: a wrong rect errors, it does not
    /// lie.
    #[test]
    fn a_diamond_rect_over_blank_pixels_fails_rather_than_reporting_no_doors() {
        let img = DynamicImage::new_rgb8(1374, 773);
        let layout = TempleLayout {
            origin: (673, 494),
            scale: 0.99,
            ncc: 0.94,
            confidence: crate::temple::doors::Confidence::High,
            current: Some(crate::temple::lattice::Slot::B0),
            doors: Default::default(),
            uncertain: Default::default(),
            slots: [(0, 0); 13],
            thresholds: crate::temple::doors::Thresholds { horizontal: 0.2, diagonal: 0.2 },
            calibration: crate::temple::anchor::AnchorCalibration {
                screen_w: 1374,
                screen_h: 773,
                scale: 0.99,
            },
        };

        let err = read_markers(&img, &layout).expect_err("blank pixels carry no seals");
        assert!(
            err.contains("marker"),
            "the failure must name the seal count, got {err:?}",
        );
    }
}
