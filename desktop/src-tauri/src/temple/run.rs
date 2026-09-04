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
//! # Before any of it: the arm gate (POE-242, POE-246)
//!
//! The four gates below are all *inside* a tick. Ahead of them sits the one
//! that decides whether a tick runs at all: the loop captures only while
//! something has put an incursion in scope ([`super::trigger::arm_source`]).
//! Until then it publishes [`TempleStatus::Waiting`] once and naps — no
//! capture, no correlation, nothing. The module being ON is not the trigger.
//!
//! Three things open it, and only the first is Client.txt's: an Alva line or
//! the temple area, the layout panel this loop can still SEE
//! ([`LoopState::panel_seen_ms`], POE-246), and the one probe tick a starting
//! loop runs before it may believe an empty screen
//! ([`LoopState::probe_pending`]). The panel input is what makes stand-down mean
//! "nothing has been on screen for [`super::trigger::PANEL_TAIL_MS`]" instead of
//! "Client.txt has been quiet for that long" — measured 2026-09-03, the latter
//! took the overlay off a panel the player was still reading.
//!
//! [`loop_step`] is that gate plus the focus check and the cadence check, as
//! one pure function, so the property that matters ("a disarmed loop never
//! reaches `capture_screen`") is a property of the step rather than of a
//! status.
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
//!    [`reader::read_layout_for_loop`] at all.
//! 2. **The pixel gate** — the [`reader::read_layout_for_loop`] the detect
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
//! - this is the start-up probe tick (POE-246), whose cheap half has no
//!   remembered plate to re-match and so cannot see a panel that is on screen;
//!   or
//! - [`FULL_READ_EVERY_N_MISSES`] cheap ticks in a row have said nothing —
//!   the backstop for a UI-scale change, which is the one way a panel can be on
//!   screen and invisible to the cheap tick.
//!
//! [`wants_full_read`] is all four rules in one pure function, so the
//! composition is testable without a screen. A cheap tick that says nothing is
//! a MISS in the sense [`LoopState`] already meant it, so the retire-after-two
//! rule and the status machine are unchanged by all of this.
//!
//! # The cold start (POE-234)
//!
//! All of the above assumes the cheap tick can SEE a panel that is on screen,
//! and on a capture size nobody has measured it may not: its nominating scale
//! is a guess, and a guess that misses is indistinguishable from an empty
//! screen. Measured 2026-09-03 on a 1920x1080 laptop — panel open, true scale
//! 1.000, guess 1.397, cheap score 0.66 against a 0.70 floor — the loop never
//! promoted, never read, and sat on "looking for the layout panel" for the
//! whole session.
//!
//! So a tick whose cheap detect did not verify an anchor buys a cold-start
//! sweep — on the [`FULL_READ_EVERY_N_MISSES`] cadence, not once. Once is not
//! enough: the loop arms on Client.txt when Alva speaks, which is seconds to
//! minutes before the player opens the layout panel, so a single sweep almost
//! always lands on a closed panel and finds nothing.
//!
//! [`SweepGate`] is that cadence. A screen with NO calibration sweeps on the
//! first such tick and every Nth after; one WITH a calibration skips the first
//! and keeps the cadence, because a hinted recheck that has missed for a whole
//! cadence is the README's "the consuming module's own verification failing"
//! and is the one shape of stale scale no prune can see. The sweep is the
//! loop's longest blocking call (5.3 s on a 1920x1080 capture in the Linux
//! container, release) and polls the stop signal inside itself.
//!
//! # The exhaustive sweep is not reachable from here
//!
//! `anchor::anchor_with_hint`'s last resort is `anchor::full_sweep`, measured
//! at 28.4 s in the container and 347.8 s on the laptop. Every anchoring call
//! in this file goes through the loop-facing chain instead —
//! [`anchor::anchor_for_loop`] and [`reader::read_layout_for_loop`], which are
//! that chain with the pyramid sweep in the last-resort slot. The exhaustive one
//! stays reachable from `super::commands::temple_debug_capture`, where a user
//! pressed a button and is waiting for it.
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
//! stay bounded"). All three ROIs — [`panel_rect`], [`remaining_rect`] and
//! [`diamond_rect`] — are placed from the anchor's origin and scale, so each
//! stays a fixed size in reference px whatever the monitor is, and each lands
//! where the game drew the panel rather than where the capture happens to end.
//! [`full_read`] prints all three, once per distinct value (`Temple: rois …`),
//! which is what makes a fallback traceable to a rect from `app.log` alone.
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
use super::trigger;

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
/// `crate::ssot::drop_if_mismatched` drops the remembered scale the moment the
/// capture changes size, so this is not the recovery path for either of those.
/// What it covers is the case neither of them can see: a capture that is still
/// the same size and still holds a panel whose scale has drifted far enough from
/// the one the shared slice remembers — and from `anchor::height_seed_scale`,
/// which is what the nominating pass falls back on — that the pass no longer
/// clears [`anchor::COARSE_CANDIDATE_FLOOR`]. The game's own UI-scale slider is
/// the way that happens.
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
    /// When a tick last SAW the layout panel ([`now_ms`]), `None` until one has
    /// (POE-246).
    ///
    /// The arm gate's third clock — [`trigger::arm_source`] keeps the loop armed
    /// for [`trigger::PANEL_TAIL_MS`] past this, so the loop stands down for the
    /// panel being ABSENT rather than for Client.txt having gone quiet. Written
    /// by [`Self::on_detect`] on a sighting and by nothing else.
    ///
    /// **Retirement does not clear it**, deliberately: retiring is two missed
    /// ticks (~2 s), and the tail is the window a player has to close a panel and
    /// reopen it. Clearing it here would stand the loop down two seconds after
    /// every panel close, which is the tail not existing.
    pub panel_seen_ms: Option<u64>,
    /// Whether the one detect a starting loop runs before it may stand down has
    /// been spent (POE-246 — see `trigger`'s start-up probe note).
    ///
    /// Spelled as SPENT rather than pending so `Default` still derives to the
    /// right answer: a loop that has run no tick owes one.
    pub probe_spent: bool,
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
    /// either way — it is the same `read_layout_for_loop` call — so there is
    /// nothing for a second number to buy.
    pub fn detect_interval(&self) -> Duration {
        if self.backed_off {
            DETECT_INTERVAL_SLOW
        } else {
            DETECT_INTERVAL
        }
    }

    /// Fold one anchor result into the state.
    ///
    /// `seen_at` is the moment the panel was seen ([`now_ms`]), and `None` is a
    /// tick that found nothing. A timestamp rather than a `bool` because of what
    /// it feeds: [`trigger::arm_source`]'s panel clock is what now decides when
    /// the loop may stand down, and a shape where a miss carries no stamp is one
    /// where a miss cannot extend the arm by accident (POE-246).
    ///
    /// Every tick calls this exactly once — through [`miss`] or through the
    /// anchored path — which is what makes it the place the start-up probe is
    /// spent, whatever the tick found.
    pub fn on_detect(&mut self, seen_at: Option<u64>) -> DetectOutcome {
        self.probe_spent = true;
        if let Some(at) = seen_at {
            self.panel_seen_ms = Some(at);
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

    /// Whether the loop still owes itself the one detect it runs before it may
    /// stand down (POE-246). [`trigger::arm_source`]'s third input.
    pub fn probe_pending(&self) -> bool {
        !self.probe_spent
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

// ------------------------------------------------------- the cold sweep --

/// The screen one cold-start sweep was run for.
///
/// Monitor id AND capture size, because either can change without the other:
/// dragging the game to a second display of the same resolution changes only
/// the id, and a resolution change changes only the size. `monitor_id` `0` is
/// `crate::capture::Capture`'s unknown, and is carried here as an ordinary
/// value rather than excluded — this is a "have I already tried this?" key, not
/// an identity claim, so two unknown displays sharing a key costs one skipped
/// sweep and never a wrong scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SweepKey {
    pub monitor_id: u32,
    pub width: u32,
    pub height: u32,
}

/// How often the cold-start sweep may run, and when it stops.
///
/// # What it is budgeting
///
/// The cold-start sweep is the expensive answer to "what scale is this
/// screen?" —
/// measured 5.3 s in the Linux container (release) on a 1920x1080 capture, and
/// the exhaustive path it replaces measured 347.8 s on the laptop that reported
/// the bug. It runs on the capture loop's own thread, so a loop that ran it on
/// every cheap miss would spend its whole life sweeping a screen with no panel
/// on it.
///
/// # Why a cadence and not once
///
/// The loop arms on Client.txt when Alva speaks (POE-242), and the player opens
/// the layout panel seconds to minutes after that. A single sweep therefore
/// almost always lands on a CLOSED panel, finds nothing correctly, and — if
/// that were the end of it — would leave the screen exactly as blind as before
/// when the panel does open, because on an uncalibrated screen the cheap tick's
/// nominating scale is the guess that started all this.
///
/// So the sweep repeats, on
/// [`FULL_READ_EVERY_N_MISSES`] — the same cadence, and for the same reason, as
/// the periodic full read: it is the interval this loop already treats as "long
/// enough that an expensive answer is worth re-asking". 30 ticks is 30 s at
/// [`DETECT_INTERVAL`] and 90 s once [`DETECT_INTERVAL_SLOW`] has fired.
///
/// # What `calibrated` means since POE-234 WI-2, and what changed with it
///
/// It is "this tick has a hint" — [`hint_for_capture`] answered `Some`, which
/// means the shared `crate::ssot::ScreenSlice` holds a scale for THIS screen. It
/// was "the temple's own `settings.json` calibration is present"; that store is
/// gone, and the provenance is wider now: ANY source counts, including a
/// `Remembered` value the startup load put there and a `MercFrame` one the merc
/// module measured, neither of which the temple has looked at.
///
/// **The head start moved with it, and that is a real delta.** A screen whose
/// remembered value is WRONG but whose capture size has not changed — a scale
/// carried in from a machine whose in-game UI slider differs, say — now counts
/// as calibrated on the first tick and waits a whole
/// [`FULL_READ_EVERY_N_MISSES`] cadence before its first sweep, where WI-1 swept
/// on the first non-verified tick. That is the price of the hint being worth
/// trying at all: it is one correlation against 5.3 s, and it is the case the
/// whole WI exists for (a scale merc measured serving the temple with no
/// search). The recovery is unchanged and one cadence away.
///
/// What a calibration buys, then, is the FIRST-tick sweep and nothing else: a
/// hinted path that re-anchors the panel in one windowed match means a screen
/// that has just been answered must not pay 5.3 s the moment Alva speaks — the
/// panel is closed then, and the loop arms on every incursion.
///
/// It does not close the gate. `desktop/src/lib/README.md`'s "Screen Geometry
/// (SSOT)" lifecycle re-measures on "the consuming module's own verification
/// failing", and a hint whose recheck has missed for
/// [`FULL_READ_EVERY_N_MISSES`] consecutive ticks IS that failure — it is the
/// in-game UI-scale change [`FULL_READ_EVERY_N_MISSES`]'s own note describes,
/// which no prune can see because the capture size never moved. So a calibrated
/// screen keeps the cadence and loses only the head start.
///
/// # What that costs, at worst
///
/// One sweep is 5.3 s (Linux container, release, 1920x1080). The cadence caps
/// it at one per [`FULL_READ_EVERY_N_MISSES`] ticks — 30 s at
/// [`DETECT_INTERVAL`], 90 s once [`DETECT_INTERVAL_SLOW`] has fired — and only
/// while the loop is ARMED, which POE-242 bounds to Alva's window rather than
/// to the session (and POE-246 extends by [`super::trigger::PANEL_TAIL_MS`] past
/// the last panel SIGHTING, which no screen without a panel on it ever gets). A
/// player who never opens the layout panel during an incursion pays it at most
/// twice.
///
/// # Why `temple_rearm` is not an input
///
/// It was, and that was wrong. The settings commands bump that counter on
/// EVERY change (see [`wants_full_read`]), and none of those is a reason to pay
/// 5.3 s — a user adjusting three settings would have bought three sweeps.
/// What the user actually presses when the geometry is wrong is Recalibrate,
/// and `ssot::geometry_recalibrate` reaches this gate the honest way: it empties
/// the shared screen scale, so [`hint_for_capture`] answers `None`, `calibrated`
/// goes false, and a screen that has just LOST its scale restarts the countdown
/// rather than serving out one some earlier state left running. That path needs
/// [`cheap_hint_for`] to hold: a session still holding its remembered plate
/// would re-anchor at the old scale on the next tick, and a verified tick never
/// reaches this gate at all.
///
/// Pure over plain data — no `AppHandle`, no image — so the whole rule is
/// testable without a screen.
#[derive(Debug, Default)]
pub struct SweepGate {
    /// The screen the countdown belongs to. A different one starts over, which
    /// is what makes a resolution or monitor change sweep immediately.
    key: Option<SweepKey>,
    /// Whether a hint existed **as of the last non-verified tick**, so the loss
    /// of one is detectable — that transition is how Recalibrate reaches this
    /// gate.
    ///
    /// The caveat is load-bearing, because [`Self::allow`] is deliberately not
    /// called on a tick whose cheap detect VERIFIED an anchor. What makes the
    /// transition observable at all is [`cheap_hint_for`]: a Recalibrate empties
    /// a slice that had answered, the remembered plate goes with it, and the
    /// next tick's cheap detect therefore has nothing to re-match and cannot
    /// verify — so it reaches this field with `calibrated` false even while the
    /// layout panel is continuously on screen. Without that drop, every tick in
    /// that window would verify at the pre-Recalibrate scale, the transition
    /// would never be observed here, and the press would re-publish the number
    /// the user asked the app to forget. `ssot::geometry_recalibrate` also bumps
    /// `temple_rearm`, which [`wants_full_read`] spends to force the read
    /// itself; the two are the sweep and the read halves of one press.
    calibrated: bool,
    /// Ticks still owed before the next sweep on [`Self::key`]. `0` means the
    /// next one sweeps.
    countdown: u32,
}

impl SweepGate {
    /// Whether this tick may pay for a cold-start sweep, spending the budget if
    /// it may.
    ///
    /// Call it ONCE per tick, on every tick whose cheap detect did NOT verify
    /// an anchor, and nowhere else.
    ///
    /// **Once**, because two paths in `tick` reach the same sweep — the cold
    /// one calls it directly, and a promoted read reaches it as
    /// [`anchor::anchor_for_loop`]'s last resort — so a budget consulted on
    /// only one of them is not a budget: a screen whose background nominates
    /// above [`anchor::COARSE_CANDIDATE_FLOOR`] promotes on every tick and
    /// would pay 5.3 s on every tick through the other.
    ///
    /// **Not on a verified tick**, because a hint that re-matched IS the
    /// calibration's own verification succeeding, and that is the event the
    /// calibrated cadence counts the absence of. Letting a working panel
    /// decrement the countdown would turn "N consecutive verification failures"
    /// into "N ticks", which is a different and weaker thing.
    pub fn allow(&mut self, key: SweepKey, calibrated: bool) -> bool {
        // A new screen, or one that has just lost its calibration, starts its
        // countdown over rather than serving out one that belonged to another
        // state — losing a calibration is how Recalibrate reaches this gate.
        if self.key != Some(key) || (self.calibrated && !calibrated) {
            self.key = Some(key);
            // An unknown scale is owed an answer NOW: `0` sweeps on this very
            // tick. A known one is owed nothing until its own verification has
            // failed a full cadence, so it starts a whole cadence away and the
            // first sweep lands on tick N + 1.
            self.countdown = if calibrated {
                FULL_READ_EVERY_N_MISSES
            } else {
                0
            };
        }
        self.calibrated = calibrated;
        if self.countdown > 0 {
            self.countdown -= 1;
            return false;
        }
        // One short of the cadence: this call IS the first of the group, so
        // `FULL_READ_EVERY_N_MISSES - 1` refusals put the next sweep exactly
        // that many ticks later — the same arithmetic
        // `LoopState::note_cheap_detect` does with `cheap_misses + 1`.
        self.countdown = FULL_READ_EVERY_N_MISSES.saturating_sub(1);
        true
    }

    /// Hand back a head start the START-UP PROBE spent on an empty screen
    /// (POE-246).
    ///
    /// An uncalibrated screen is owed its first sweep NOW, and the probe tick is
    /// usually the wrong tick to spend it on: it runs before anything has armed
    /// the loop, so it lands on a closed panel, finds nothing correctly, and — if
    /// it kept the budget — would leave the first ARMED tick with the panel
    /// actually open waiting a whole [`FULL_READ_EVERY_N_MISSES`] cadence for the
    /// answer. The probe still SWEEPS, because a module switched on over an open
    /// panel is exactly what it exists to catch; it just does not pay for the
    /// tick that finds nothing.
    ///
    /// # Only a tick the PROBE armed
    ///
    /// `source` is the gate's answer for this iteration
    /// ([`trigger::arm_source`]), and only
    /// [`trigger::ArmSource::StartupProbe`] refunds. An app started INSIDE a
    /// temple is the case that needs the distinction: Client.txt arms it, its
    /// first tick is an ordinary armed tick, and the loop keeps ticking after it
    /// — refunding there buys a second 5.3 s sweep on the very next tick. That
    /// source is also proof the tick is the loop's first, because
    /// `arm_source` reaches the probe branch only while the probe is unspent, so
    /// no second "is this the first tick?" argument is needed.
    ///
    /// The conditions are arguments rather than an `if` at the call site,
    /// following [`LoopState::note_tick_duration`]: the rule belongs in the
    /// tested surface. A sweep that ANCHORED spends the budget like any other —
    /// it bought the answer the budget is for.
    ///
    /// The key guard is defensive and unreachable on the present call path
    /// ([`Self::allow`] set this very key a few lines earlier, on this capture).
    /// It stays because it is the method's one invariant — a countdown is given
    /// back to the screen that spent it — and the call site cannot state it.
    pub fn refund_probe(
        &mut self,
        key: SweepKey,
        source: Option<trigger::ArmSource>,
        anchored: bool,
    ) {
        if !matches!(source, Some(trigger::ArmSource::StartupProbe))
            || anchored
            || self.key != Some(key)
        {
            return;
        }
        self.countdown = 0;
    }
}

/// Whether this tick's cheap result leaves anything for a cold-start sweep to
/// answer.
///
/// `false` for a verified anchor and nothing else. [`anchor::CheapDetect::Anchored`]
/// is a full-resolution match against [`anchor::NCC_FLOOR`] — the calibration's
/// own verification succeeding — so that tick already has its scale and must not
/// spend a budget kept for the ticks that do not. It is the event
/// [`SweepGate`]'s calibrated cadence counts the ABSENCE of, which is what makes
/// that cadence "N consecutive verification failures" rather than "N ticks".
///
/// `true` for the other two, because both can reach the sweep: a `Candidate`
/// promotes to a read whose last resort is [`anchor::anchor_for_loop`]'s sweep,
/// and `Nothing` is the cold path itself.
///
/// A function rather than a `matches!` at the call site so the rule has a seam:
/// it is one line in [`tick`], which needs a screen and an `AppHandle`.
pub fn sweep_could_help(cheap: &anchor::CheapDetect) -> bool {
    !matches!(cheap, anchor::CheapDetect::Anchored(_))
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
///
/// `swept` is the cold-start sweep's answer (POE-234), folded in as a fifth way
/// in rather than short-circuiting around this function: a sweep that anchored
/// is a detection by any reading, and routing it through here is what keeps
/// [`LoopState::cheap_misses`] and the re-arm bump with one owner apiece.
///
/// `first_tick` is the sixth (POE-246): a loop's FIRST tick promotes whatever
/// the cheap tick said, whoever opened the gate for it. It has to, and the
/// reason is [`anchor::detect_cheap`]'s input rather than its floor — a fresh
/// session holds no [`CheapHint`], because that hint carries an ORIGIN and only a
/// previous read produces one, so the cheap tick on a starting loop is the
/// nominating pass and nothing else. That is the pass POE-234 measured at 0.66
/// against a 0.70 floor on a 1080p laptop with the panel open. Promoting reaches
/// the read's own hinted chain instead, which is the remembered scale searched
/// over the whole capture — [`anchor::anchor_for_loop`]'s note prices its two
/// non-sweep steps at two correlations, so a first tick that finds nothing costs
/// about what the cheap tick it followed cost.
///
/// # What it does to the sweep budget
///
/// Nothing here, and one thing next door. This promotion adds no sweep TRIGGER:
/// the promoted read is handed [`SweepGate`]'s single per-tick answer like every
/// other promotion, so an uncalibrated screen sweeps on the cadence it already
/// had and a calibrated one pays the hint and the table.
///
/// What POE-246 did change is that the tick EXISTS. The start-up probe opens the
/// gate for one iteration that a disarmed loop would not have run at all, so
/// [`SweepGate::allow`] is asked on it — and on an uncalibrated screen that is
/// the head-start sweep. [`SweepGate::refund_probe`] is the other half: it gives
/// that head start back when the probe's sweep found nothing, so the first
/// ARMED tick over an open panel still gets it.
pub fn wants_full_read(
    state: &mut LoopState,
    gate: &mut slice::ReadGate,
    cheap: &anchor::CheapDetect,
    swept: bool,
    first_tick: bool,
    rearm: u64,
) -> bool {
    let rearmed = gate.rearm_pending(rearm);
    let read = state.note_cheap_detect(cheap.worth_reading() || swept || first_tick, rearmed);
    if read && rearmed {
        gate.note_rearm(rearm);
    }
    read
}

// ------------------------------------------------------ the loop's step --

/// What one iteration of the loop does, decided before anything is captured.
///
/// The whole of the POE-242 gate is here, as a total function of three
/// booleans, so "a disarmed loop never captures" is a property of the STEP
/// rather than of a status the loop happens to publish. `capture_screen` is
/// reached from exactly one arm of the loop's `match` — [`Self::Detect`] — and
/// [`loop_step`] is the only thing that can return it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopStep {
    /// The game is not the foreground window. The layout panel is not on
    /// screen, so there is nothing to look at.
    UnfocusedNap,
    /// Nothing has armed the module ([`super::trigger`]): the player is in a
    /// map, or a town, and Alva has not spoken.
    DisarmedNap,
    /// Armed, and the next detect tick is not due yet.
    Quantum,
    /// Armed and due: run [`tick`], which captures.
    Detect,
}

impl LoopStep {
    /// How long the loop waits after this step.
    ///
    /// The two nap steps take the SECOND, not the loop quantum: a disarmed loop
    /// is the state a session spends nearly all its time in, and waking it ten
    /// times a second to re-ask a question whose answer arrives on another
    /// thread would spend most of what the gate just saved. Cancellation is
    /// unaffected — [`nap`] polls the stop signal every [`TICK`] whatever it is
    /// handed.
    pub fn nap(self) -> Duration {
        match self {
            LoopStep::UnfocusedNap | LoopStep::DisarmedNap => UNFOCUSED_NAP,
            LoopStep::Quantum | LoopStep::Detect => TICK,
        }
    }
}

/// The loop's gate, in one pure function. Focus first, the arm second, the
/// cadence last.
pub fn loop_step(focused: bool, armed: bool, detect_due: bool) -> LoopStep {
    if !focused {
        return LoopStep::UnfocusedNap;
    }
    if !armed {
        return LoopStep::DisarmedNap;
    }
    if detect_due {
        LoopStep::Detect
    } else {
        LoopStep::Quantum
    }
}

/// The status to publish for a gate that just moved — or for one whose
/// announcement something else wrote over — and `None` while neither happened.
///
/// `said` is the armed-ness the loop last announced (`None` before the first
/// one) and `status` is what the slice holds right now. Publishing on the
/// TRANSITION rather than every iteration is what keeps the gate from writing
/// over a board that is on screen: an armed loop that has read a panel sits at
/// [`TempleStatus::Read`], and re-announcing `Idle` under it once a second
/// would mark the board stale ten times a temple.
///
/// # Why `status` is read at all
///
/// A transition-only gate is a WRITE-ONCE announcement, and POE-171 finding 15
/// is the case that loses it: a retiring loop's `Stopping → Idle` publish can
/// land after the new loop's `Waiting`, and a disarmed loop that already `said`
/// `false` would never republish — the page would sit on `idle` ("about to
/// read") for the rest of a session that is not looking at all. So the DISARMED
/// half is re-asserted whenever applying it would still move the status: while
/// the loop is not looking, it owns the status outright.
///
/// The ARMED half is not re-asserted, because `Reading` / `Read` over an
/// `Idle` announcement is the loop's own work rather than a foreign write.
///
/// Re-assertion is keyed on [`next_status`] rather than on `status ==
/// Waiting` so a status no tick result can leave ([`TempleStatus::Unavailable`])
/// does not turn into one publish and one log line per second.
pub fn gate_announcement(
    said: Option<bool>,
    armed: bool,
    status: TempleStatus,
) -> Option<TickOutcome> {
    let outcome = if armed {
        TickOutcome::Armed
    } else {
        TickOutcome::Disarmed
    };
    if said != Some(armed) {
        return Some(outcome);
    }
    if armed {
        return None;
    }
    (next_status(status, TickOutcome::Disarmed).status != status).then_some(outcome)
}

/// The app-log line for a gate whose SOURCE has moved — one line per distinct
/// source, `None` while it has not moved.
///
/// **The capture loop is the one owner of the arm/disarm app-log line.**
/// `trigger::on_client_line` writes the arm STATE and says nothing: it fires on
/// every Client.txt transition whether or not the module is running, so letting
/// it log too put two lines in `app.log` for one event whenever the module was
/// on. This one is the fact a smoke run is checking — the capture loop saying it
/// has started (or stopped) looking — and it covers the two transitions no
/// Client.txt line announces at all: an [`super::trigger::ALVA_TAIL_MS`] arm
/// expiring, and (POE-246) the layout panel going off screen.
///
/// # Why the source and not the publish
///
/// Keyed on [`trigger::ArmSource`] rather than on the publish
/// [`gate_announcement`] asks for, which POE-246 changed for two reasons. A gate
/// that stays open while the REASON changes hands says so — Alva's tail expiring
/// under a panel that is still on screen is the loop's whole new behaviour, and
/// it is invisible in an armed-ness bit. And the re-assertion that corrects a
/// foreign status write (POE-171 finding 15) stops putting a second `stood down`
/// line in `app.log` for one stand-down: the publish is the correction, the line
/// never was.
///
/// `said` starts `None`, which is the same claim this line's `None` arm makes —
/// a loop that has said nothing has not started looking — so the first source it
/// does find is always announced.
fn gate_line(
    said: &mut Option<trigger::ArmSource>,
    source: Option<trigger::ArmSource>,
) -> Option<String> {
    if *said == source {
        return None;
    }
    *said = source;
    Some(match source {
        Some(source) => format!(
            "Temple: capture armed by {} — looking for the layout panel",
            source.label()
        ),
        None => "Temple: capture stood down — waiting for Alva (Re-arm forces a read)".to_string(),
    })
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
    /// The arm gate closed (POE-242): nothing in Client.txt puts an incursion
    /// in scope, so the loop is not capturing. Not a tick — no tick ran.
    Disarmed,
    /// The arm gate opened. Also not a tick: it is the announcement that the
    /// loop has started looking again, published BEFORE the first read.
    ///
    /// What it moves is the STATUS, not the board — [`apply_status`] writes
    /// `status` and `last_error` and touches nothing else. The overlay is the
    /// surface that reacts: `Idle` is not in the webview's
    /// `OVERLAY_VISIBLE_STATUSES`, so the board stops floating over the game
    /// while the loop looks for a new one. The Temple PAGE keeps drawing the
    /// last board it was given, under a badge that now reads
    /// `watching for the layout panel`.
    Armed,
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
        // The two gate events clear for the same reason [`TickOutcome::Stopping`]
        // does: a loop that is not looking is not reporting a live board, and
        // the failure it had while it WAS looking is no longer something the
        // user can act on. Leaving the message standing under a `waiting` badge
        // is the drift this machine exists to prevent, one status further on.
        TickOutcome::Disarmed => (TempleStatus::Waiting, true),
        TickOutcome::Armed => (TempleStatus::Idle, true),
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

/// Fold one ARM-GATE event into the slice.
///
/// **Nothing but the status, since POE-248** — and the function survives its
/// own body for the same reason it was written: the loop's gate publish goes
/// through one named seam, so what a gate event may touch is a question with a
/// place to be answered rather than a line in a closure no test can reach.
///
/// # What it stopped doing, and why
///
/// POE-244 dropped the advice here, at the stand-down. It was already the
/// second guess at where a move expires — [`miss`]'s retire was the first, and
/// that note explains why it was wrong — and it is wrong for the same reason
/// one step further out. Owner, 2026-09-04, on the first live session: the door
/// diamond *"disappeared when the layout panel closed"*, and the log says why —
/// `12:32:10 capture armed by the panel on screen` … `12:39:05 capture stood
/// down`, with the player still in the room the widget was describing.
///
/// The rule now is that the kill callout lives with the PANEL and the room
/// widget lives with the INCURSION. A gate is a statement about whether
/// anything is LOOKING at the screen, and the incursion is not over because the
/// module stopped looking. What ends the advice is a fact about the game:
/// [`super::trigger::advice_end`]'s zone change or next Alva line, a read that
/// replaces it, or the module being switched off ([`slice::force_off`]).
pub fn apply_gate(slice: &mut TempleSlice, outcome: TickOutcome) {
    apply_status(slice, outcome);
}

// ------------------------------------------------------------ text ROIs --

/// The side panel's own border box, in reference px relative to the Entrance
/// plate centre ([`TempleLayout::origin`]): `[left, top, right, bottom]`, `+x`
/// right and `+y` down. The panel is drawn above and to the right of the
/// Entrance, so both vertical figures are negative.
///
/// **Measured** as the panel's border rectangle on the three captures whose
/// origin AND scale are both recorded — the committed full frame and the two
/// board fixtures' source screenshots:
///
/// | capture | size | scale | origin | panel border box | in ref px from the origin |
/// |---|---|---|---|---|---|
/// | `screen-live-1920x1080.png` | 1920x1080 | 1.0000 | (960, 713) | x 1171–1655, y 44–418 | +211, −669, +695, −295 |
/// | `2026-08-02_22-22-38` | 1374x862 | 1.0000 | (673, 682) | x 884–1368, y 13–387 | +211, −669, +695, −295 |
/// | `2026-08-07_19-28-36` | 1539x968 | 1.13 recorded | (745, 768) | x 980–1517, y 24–440 | +208, −658, +683, −290 |
/// | …the same capture at the 1.111 its own border implies | | | | | +212, −670, +695, −295 |
///
/// The two scale-1.0 captures agree **to the pixel on all four edges**, a month
/// apart and 546 px of capture width apart. That agreement is the constant.
///
/// The third row does NOT disagree with them — its ANCHOR does. Its panel
/// border measures 537 x 416 px, and `537 / 484 = 1.1095`, `416 / 374 = 1.1123`:
/// at scale **1.111** the box above reproduces that capture to under a pixel on
/// every edge. The 1.13 it is recorded at is an anchor error of ~1.7%, which is
/// [`DIAMOND_DX_REF`]'s anchor-accuracy note and POE-247's subject. So there is
/// no board-to-board spread here — there is one error, the anchor's, and it is
/// what [`PANEL_MARGIN_REF`] absorbs.
///
/// # Why the origin and not the capture's right edge
///
/// Until POE-230 this region was `540 × 430` ref px hung off the capture's
/// top-RIGHT corner. Measured 2026-09-03 on the laptop (dump
/// `temple-debug/1788438639673`, the frame committed as the fixture above): that
/// put the crop at `[1380, 0, 540, 430]`, which cuts the panel in half. The
/// title read `NG WORKSHOP`, and the lower-left architect block — Xopec, whose
/// box on that frame is x 1189–1347 — was **entirely** outside the crop, so the
/// dump reported one architect on a board that has two. The panel is drawn
/// against the LAYOUT, not against the screen; the table above is that fact
/// measured, and the third row is what a screen-edge offset was really tracking.
pub const PANEL_BOX_REF: [f32; 4] = [211.0, -669.0, 695.0, -295.0];

/// How far past [`PANEL_BOX_REF`] the OCR crop reaches on the left, top and
/// bottom — in reference px. The right side is [`PANEL_RIGHT_MARGIN_REF`], which
/// is smaller and says why.
///
/// What a margin has to absorb is **anchor error**, since the box itself is the
/// same on every capture that reproduces its own scale. The recorded band is
/// −4% (POE-247's hint chain answering 0.96 where the peak is 1.00) to +1.7%
/// (the 1539 row of [`PANEL_BOX_REF`]'s table). Against a −669 ref px top offset
/// −4% is 27 px, so 40 clears the worst recorded case on the axis where the
/// offsets are largest, and it is also what the retired screen-edge constants
/// documented (~46 horizontal, ~42 vertical), carried across.
///
/// Nothing sits outside the panel on those three sides on the committed fixture,
/// so the only cost of 40 there is buffer size.
pub const PANEL_MARGIN_REF: f32 = 40.0;

/// The RIGHT margin, in reference px — smaller than [`PANEL_MARGIN_REF`] because
/// it is the one side where the crop reaches into somebody else's text.
///
/// **Measured on the committed fixture.** The map's own info block is drawn
/// BEHIND the panel; its leftmost glyph column sits **4 ref px** past the panel
/// border (+699 against the border's +695), so *every* positive right margin
/// admits a strip of it, and the volume is what the constant buys:
///
/// Offsets below are from the origin like everything else here, so the title's
/// own band — absolute y 70–112 on the fixture — is **−643 … −601**. A "run" is
/// a group of glyph rows merged across gaps of two rows or fewer; counting rows
/// strictly adjacent gives a higher figure for the same ink.
///
/// | right margin | ink px admitted | row runs | runs inside the title's band |
/// |---|---|---|---|
/// | 40 | 1250 | 14 | −652 … −641 |
/// | 20 | 622 | 12 | −643 … −641 |
/// | 16 | 456 | 12 | −643 … −641 |
///
/// The band matters because a run overlapping the title's rows can be grouped
/// into the title LINE, and `rooms::match_room_name` rejects a run-together read
/// by [`super::rooms::RATIO_MAX`] (1.45) — a short name like `Chasm` has the
/// least room for an appended fragment. 40 was the worst case for that: its run
/// starts 9 rows higher (−652 against −643), so it overlaps the band by more
/// than the 3-row fragment the other two admit.
///
/// **20, not 16.** The floor is not the ink, it is the Hayoxi block, whose own
/// right edge is at +681: a −4% anchor puts the crop's right edge at
/// `(695 + m) × 0.96`, so `m = 16` retains that text by **1.6 ref px** and
/// `m = 20` by **5.4**. 16 is inside the rounding of the thing it has to
/// protect. 20 halves 40's admitted ink, keeps the title-band overlap to the
/// same 3-row fragment 16 does, and still clears the recorded anchor band.
pub const PANEL_RIGHT_MARGIN_REF: f32 = 20.0;

/// The `N Incursions Remaining` line's OCR region, relative to the Entrance
/// plate centre ([`TempleLayout::origin`]), in reference px.
///
/// The game centres this line under the Entrance plate, so it is keyed on the
/// anchor rather than on a screen edge. It was the FIRST region keyed that way
/// and the shape POE-230 moved the other two onto — see [`PANEL_BOX_REF`].
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

/// The side panel's region, given the Entrance centre and the anchor's scale.
///
/// `[x, y, w, h]`. Keyed on the anchor, like [`remaining_rect`] and
/// [`diamond_rect`] and for the reason [`PANEL_BOX_REF`] measures: the game
/// draws this panel against the layout. A crop that still misses — the panel is
/// only partly captured — degrades to "the panel's text is not read" rather than
/// to a wrong answer: [`crop_clipped`] hands back the readable part,
/// [`panel::read_panel`] returns an unread title and whichever offers survived,
/// and the advisor warns rather than inventing one.
pub fn panel_rect(origin: (i32, i32), scale: f32) -> [i32; 4] {
    let [left, top, right, bottom] = PANEL_BOX_REF;
    let x0 = origin.0 + ((left - PANEL_MARGIN_REF) * scale).round() as i32;
    let y0 = origin.1 + ((top - PANEL_MARGIN_REF) * scale).round() as i32;
    let x1 = origin.0 + ((right + PANEL_RIGHT_MARGIN_REF) * scale).round() as i32;
    let y1 = origin.1 + ((bottom + PANEL_MARGIN_REF) * scale).round() as i32;
    [x0, y0, x1 - x0, y1 - y0]
}

/// The `N Incursions Remaining` region, given the Entrance centre and the
/// anchor's scale. `[x, y, w, h]` — see [`REMAINING_HALF_W_REF`].
pub fn remaining_rect(origin: (i32, i32), scale: f32) -> [i32; 4] {
    let half_w = (REMAINING_HALF_W_REF * scale).round() as i32;
    let top = origin.1 + (REMAINING_TOP_REF * scale).round() as i32;
    let bottom = origin.1 + (REMAINING_BOTTOM_REF * scale).round() as i32;
    [origin.0 - half_w, top, 2 * half_w, bottom - top]
}

/// Crop `rect` from `img`, clipped to the frame, with the corner it was
/// actually taken from. `None` when nothing overlaps.
///
/// Clipped, unlike [`diamond_rect`]'s consumer: a text ROI that hangs off the
/// frame still has readable text in the part that does not, and there is no
/// count or angle here for a bad rect to corrupt — the worst a clipped crop
/// does is read fewer lines. An empty intersection is `None` rather than a
/// zero-sized image, which `preprocess_for_ocr` would panic on.
///
/// The origin comes back with the image because the OCR boxes have to be moved
/// out of the crop's pixels and into the capture's (POE-243), and the corner to
/// add is the CLIPPED one, not `rect`'s: a ROI starting at −20 is cropped at 0,
/// and placing its lines against −20 would put every box 20 px off screen.
/// Returning both from the one function is what keeps the two from drifting.
pub fn crop_clipped(img: &DynamicImage, rect: [i32; 4]) -> Option<(DynamicImage, (i32, i32))> {
    let [x, y, w, h] = rect;
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(img.width() as i32);
    let y1 = (y + h).min(img.height() as i32);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((
        img.crop_imm(x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32),
        (x0, y0),
    ))
}

// ------------------------------------------------------ the diamond rect --

/// Diamond centre offset from the Entrance plate centre
/// ([`TempleLayout::origin`]), in reference px. `+x` right, `+y` down — the
/// diamond is drawn above the Entrance, so the vertical figure is negative.
///
/// **Measured** on the two captures whose origin and scale are both recorded:
///
/// | capture | size | scale | origin | diamond centre | in ref px from the origin |
/// |---|---|---|---|---|---|
/// | `screen-live-1920x1080.png` | 1920x1080 | 1.0000 | (960, 713) | (1413, 217) | +453, −496 |
/// | `2026-08-02_22-22-38` | 1374x862 | 1.0000 | (673, 682) | (1126, 186) | +453, −496 |
///
/// Two independent captures, the same pair of numbers. The fixture's centre is
/// the mean of its three opposite seal-pair midpoints as the shipped detector
/// reports them — (1413.3, 218.5) — and of the gold outline's bounding box
/// (x 1299–1529, y 126–312, centre (1414, 219)), which agree to ~1 px. The 1374
/// row is the centre the retired screen-edge table already recorded for that
/// board, re-expressed against its origin.
///
/// That agreement is what the retired form could not produce. Its five-point
/// table spanned **220–256 ref px** horizontally and **157–193** vertically — a
/// 36 ref px band, and the reason its own note said the read was "expected to
/// fall back on a share of real boards". The band was the capture's right edge
/// moving under a panel that had not moved.
///
/// # Anchor accuracy, which is now the only error left
///
/// `2026-08-07_19-28-36` (1539 px, origin (745, 768), diamond centre
/// (1249, 218)) reads +446, −487 against its RECORDED scale of 1.13. It is not a
/// third measurement of this constant — it is a measurement of the anchor. Its
/// panel border is 537 x 416 px against the reference 484 x 374, which implies
/// **1.1095 / 1.1123**; at 1.111 this constant puts the rect's centre at
/// (1248, 217) against the measured (1249, 218), and [`PANEL_BOX_REF`] puts the
/// border inside a pixel on every edge. The board is right and the anchor is
/// ~1.7% high (POE-247's subject, recorded there as a hint chain answering 0.96
/// where the peak is 1.00 — the same failure with the other sign).
///
/// The retired screen-edge table's five scales are worth keeping for that reason
/// alone, as the anchor-accuracy record they turn out to be: **1.0164**
/// (`2026-08-02_16-41-11`), **1.0001** (`2026-08-03_22-54-58`), **1.0000**
/// (`2026-08-02_22-22-38`), **1.0012** (`2026-08-03_11-58-28`), **1.1321**
/// (`2026-08-07_19-28-36`). Only two of those can be cross-checked from
/// committed material: the 1374 row, whose panel border confirms 1.0000 exactly,
/// and the 1539 row above. The other three — 1.0164, 1.0001 and 1.0012 — are the
/// recorded anchor scales and nothing more; their panel borders have not been
/// re-measured, because none of those screenshots is in the repository.
///
/// # The locator follow-up is closed by this, not deferred by it
///
/// The retired constant's own note called for a **diamond locator** — correlate
/// the diamond outline the way [`super::anchor`] correlates the Entrance plate —
/// to replace a five-point estimate. What that locator was for was finding the
/// diamond when the screen edge could not; the anchor already finds it, and the
/// table above is the same answer for a thousandth of the cost.
///
/// What is left is the anchor's own accuracy, and a locator would not have been
/// the instrument for it either: the **panel border box is**. It is 484 x 374 ref
/// px, it has a hard edge on all four sides, and dividing a capture's measured
/// border by it recovers the scale directly — 537 x 416 → 1.1095 / 1.1123 on the
/// board above, against an anchor that said 1.13. That is a cheap second opinion
/// on any anchor, and it is the shape a follow-up should take.
pub const DIAMOND_DX_REF: f32 = 453.0;
/// Vertical half of [`DIAMOND_DX_REF`]'s offset — see that constant's note.
pub const DIAMOND_DY_REF: f32 = -496.0;

/// Diamond rect width in reference px, centred on [`DIAMOND_DX_REF`].
///
/// **The rect's centre is the projection's origin** —
/// [`markers::assign_markers`] measures every seal's angle from `(x + w/2,
/// y + h/2)` — so the rect stays symmetric about the measured centre and the
/// size is the only free variable. Measured on `screen-live-1920x1080.png`,
/// whose room (Lightning Workshop at C1) has the 6-neighbour shape, i.e. the
/// widest fan the lattice draws.
///
/// # The right edge is what the width buys
///
/// The seals' ink spans x 1329–1501 and the upper-right architect block's ink —
/// its second line is drawn in the same red as a closed seal — starts at
/// x 1514, 13 px further right. At this width both ends of the horizontal
/// envelope are that one edge: at dx −22 it has fallen to x 1491 and clips the
/// rightmost seal past what survives [`markers::MIN_BLOB_HEIGHT`] (5 seals for
/// a 6-neighbour room), and at dx +14 it has reached x 1527 and takes enough of
/// the architect line to pass the same filters (7).
///
/// The LEFT edge never causes a failure, at any width in the table below — but
/// at this one it comes close, sitting at x 1326 against the leftmost seal ink
/// at 1329 when dx is at its +13 limit. So widening does not widen the
/// envelope, it slides it; and past 208 it stops sliding cleanly, because the
/// angular gate below starts firing before either edge reaches anything.
///
/// # Why 200
///
/// Measured on the fixture with [`markers::read_door_markers`] AND
/// [`markers::assign_markers`] — the second one matters, because a rect can
/// return six seals whose fan has been rotated past
/// [`markers::MAX_RESIDUAL_DEG`] (22°) and the read then fails as `Unmappable`
/// rather than on the count. Envelope of pure origin error at scale 1.0:
///
/// | width | dx | dy |
/// |---|---|---|
/// | 176 | −10 … +16 | −20 … +26 |
/// | 192 | −18 … +20 | −27 … +30 |
/// | **200** | **−22 … +13** | **−29 … +30** |
/// | 208 | −25 … +9 | −29 … +30 |
/// | 224 | −25 … +4 | −29 … +30 |
/// | 240 | none — 7 markers at dx 0 (right edge 1533 admits the architect ink) | — |
///
/// From 200 up the dy column saturates at −29 … +30 and stops responding to the
/// height, because past those the 22° gate fires before either horizontal edge
/// reaches anything: the vertical limit is the fan ROTATING, not the rect
/// clipping. dx keeps moving with the width because the right edge is what runs
/// into the seals below and the architect ink above.
///
/// **200 gives the widest dx band that still holds the +1.7% anchor** — 208
/// loses it at +2%, 224 and 240 lose more, and 192 and below start clipping
/// seals. Its scale envelope, measured at 0.0005 steps around each end, is
/// **0.962 … 1.024**: 0.9615 fails `Unmappable` at 22.1°, 1.025 reads a seventh
/// marker off the architect block.
///
/// # POE-247's −4% is not a width problem
///
/// **No width holds 0.96.** At that scale the rect's centre is (1395, 237)
/// whatever the width is — the centre is [`DIAMOND_DX_REF`] × scale and the
/// width does not enter it — and the fan is rotated 22.1–22.8° about it. 176 and
/// 192 fail on the count; 200, 208, 224 and 240 return six seals and fail the
/// angular gate. Widening cannot fix a rotation, so POE-247's low anchor has to
/// be fixed **at the anchor**; this constant only decides how much of the
/// remaining budget the crop spends.
///
/// The two envelopes above also do not compose, which is why they are quoted
/// per axis and the scale envelope is quoted separately: a scale error is a
/// DIAGONAL displacement. 0.96 is (−18, +20), which sits inside dx −22 … +13 and
/// inside dy −29 … +30, and still fails.
///
/// At scale 1.0 the rect leaves 16/12 ref px of margin past the fan
/// horizontally and 29/24 vertically. Thinner than [`PANEL_MARGIN_REF`] because
/// the game leaves less room here — 13 px between the fan and the architect ink
/// is the whole horizontal budget, and it is shared with the anchor.
pub const DIAMOND_W_REF: f32 = 200.0;
/// See [`DIAMOND_W_REF`]. Square: dy is −29 … +30 at this height and does not
/// widen with more of it, so height is not what the size decision is about.
pub const DIAMOND_H_REF: f32 = 200.0;

/// Where to look for the side panel's diamond, given the Entrance centre and the
/// anchor's scale.
///
/// `[x, y, w, h]`, clamped to nothing — an off-screen rect is
/// [`markers::MarkerError::RectOutsideImage`], which is a fallback like any
/// other error and not something to paper over by sliding the rect back into
/// frame (a slid rect is a wrong rect that no longer trips the gate).
///
/// # What is left of the windowed-client failure mode
///
/// Keying on the anchor retires it as a *displacement*. The origin is found in
/// the capture, so a client drawn anywhere inside the monitor carries this rect
/// with it, and the offset a windowed client used to add is exactly what
/// [`PANEL_BOX_REF`]'s note measures away.
///
/// What survives is CLIPPING: a window pushed far enough off the monitor's top
/// or right that the panel is only partly captured. The rect then leaves the
/// frame, [`markers::read_door_markers`] returns `RectOutsideImage`, and the
/// module falls back to `doors − uncertain` with the incident corridors surfaced
/// as unresolved — honest, and still permanent for as long as the window sits
/// there. It is now a window half off the screen rather than a window merely not
/// maximised, and [`full_read`]'s `Temple: rois` line prints the rect it used so
/// the difference is readable from `app.log`.
pub fn diamond_rect(origin: (i32, i32), scale: f32) -> [i32; 4] {
    let cx = origin.0 as f32 + DIAMOND_DX_REF * scale;
    let cy = origin.1 as f32 + DIAMOND_DY_REF * scale;
    let w = DIAMOND_W_REF * scale;
    let h = DIAMOND_H_REF * scale;
    [
        (cx - w / 2.0).round() as i32,
        (cy - h / 2.0).round() as i32,
        w.round() as i32,
        h.round() as i32,
    ]
}

/// Every rectangle a read takes its INPUT from, given the anchor.
///
/// The never-cover set POE-244's overlay places itself against, and the reason
/// it is built here: five sources own these rects — [`panel_rect`],
/// [`diamond_rect`] and [`remaining_rect`] above, [`panel::name_strip`] /
/// [`panel::numeral_box`], and [`Lattice::edge_midpoint`] with
/// [`lattice::PATCH_HALF`] — and the overlay needs all five at once. A second
/// list of them anywhere (least of all in TypeScript, which cannot import
/// these) drifts silently: a constant moves, the module still reads correctly,
/// and the overlay quietly starts drawing over the crop it is reading.
///
/// The two OCR boxes per plate are published as their UNION, one rect per
/// plate. It is a superset of both — the numeral's band starts above the name's
/// and the name's ends at the plate's bottom edge — and a superset is the safe
/// direction for a rule whose only job is to keep something OUT: the cost is a
/// few px of screen the overlay will not use, and the alternative is 26 rects
/// carrying a distinction no consumer of this list makes.
///
/// 42 rects on a full board: 3 panel regions, 13 plates, 26 corridors.
pub fn read_rois(origin: (i32, i32), scale: f32) -> Vec<slice::RoiView> {
    let lattice = Lattice::new(origin, scale);
    let mut out = vec![
        slice::RoiView { kind: "panel".to_string(), of: None, rect: panel_rect(origin, scale) },
        slice::RoiView { kind: "diamond".to_string(), of: None, rect: diamond_rect(origin, scale) },
        slice::RoiView {
            kind: "remaining".to_string(),
            of: None,
            rect: remaining_rect(origin, scale),
        },
    ];
    for slot in lattice::Slot::ALL {
        out.push(slice::RoiView {
            kind: "plate".to_string(),
            of: Some(slot.as_str().to_string()),
            rect: union_rect(panel::name_strip(&lattice, slot), panel::numeral_box(&lattice, slot)),
        });
    }
    // The same half-width the beam sampler uses, taken from the same constant
    // and truncated the same way — `doors::read_doors` computes `hw` exactly
    // like this, and a rect one pixel short of the patch is a rect that admits
    // ink into the read.
    let hw = (lattice::PATCH_HALF * scale as f64) as i32;
    for edge in lattice::edges() {
        let (mx, my) = lattice.edge_midpoint(edge);
        out.push(slice::RoiView {
            kind: "corridor".to_string(),
            of: Some(edge.to_string()),
            rect: [mx - hw, my - hw, 2 * hw, 2 * hw],
        });
    }
    out
}

/// The smallest `[x, y, w, h]` containing both. Used only by [`read_rois`].
fn union_rect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    let right = (a[0] + a[2]).max(b[0] + b[2]);
    let bottom = (a[1] + a[3]).max(b[1] + b[3]);
    [x, y, right - x, bottom - y]
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
    let rect = diamond_rect(layout.origin, layout.scale);
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

/// The anchor hint this capture's remembered screen scale implies (POE-234
/// WI-2) — the temple's whole READ of `crate::ssot::ScreenSlice`.
///
/// The shared slice is the app's one store of "what scale is this screen drawn
/// at", written by whichever module could see its own UI first. This converts it
/// into the temple's own unit through [`anchor::scale_for_ui_scale`] and hands
/// it to [`anchor::anchor_for_loop`] as the hint — a single-scale coarse pass
/// over the whole capture, verified against [`anchor::NCC_FLOOR`] like any other
/// candidate. So a scale the MERC module measured saves the temple its 5.3 s
/// cold-start sweep, and a scale the temple anchored is what merc's next session
/// starts from; neither module keeps a second answer to the same question. The
/// temple had one until this commit (`Settings::temple_calibration`), and it is
/// gone: [`anchor::AnchorCalibration`] is now derived state — produced by a read
/// (`TempleLayout::calibration`), remembered within a session as
/// [`anchor::CheapHint`]'s (scale, origin) pair, and produced HERE from the
/// slice — never a second persisted store.
///
/// # Four sources can be behind the number, and one of them drifts
///
/// `MercFrame`, `TempleAnchor` and a `Remembered` load are all measurements. The
/// fourth is `MercOcr` — the line-pitch estimate POE-214 measured 6-12 px off
/// the gold frame — which reaches the slice when it is the first value or when
/// it is outside `ssot::accepts`' band. Nothing here filters it out, and that is
/// deliberate: the bound is small and known. 0.01 of `ui_scale` is the band, so
/// the worst hint an OCR seed can produce is `0.01 * k` = 0.011 of temple scale,
/// about one [`anchor::SCALE_STEP`] — a single-scale search one step off the
/// truth still clears [`anchor::NCC_FLOOR`] with room (0.9603 against the peak's
/// 0.9936 on `board-ref-1374.png`).
///
/// What such a hint can NOT do is launder itself: the temple anchors at exactly
/// the scale it was handed, so its republish reproduces the standing value,
/// `ssot::accepts` refuses it as a restatement, and the slice keeps saying
/// `merc-ocr`. An OCR seed is therefore a slightly worse starting point and
/// never a promotion — and when the gold frame does land, the correction reaches
/// the temple on the next tick through [`cheap_hint_for`].
///
/// `None` — no hint, and the caller is uncalibrated — in exactly three cases:
///
/// - nothing has measured a screen (fresh install, or the tick right after
///   `ssot::geometry_recalibrate`);
/// - the remembered measurement is not of THIS capture. The rule is
///   `ssot::screen_matches`', reused rather than restated so the temple cannot
///   grow its own opinion of what "the same screen" means — in particular the
///   POE-237 one about `monitor_id == 0` being UNKNOWN and never compared as an
///   identity. In the capture loop this branch is nearly unreachable, because
///   `ssot::drop_if_mismatched` runs first on the same pixels and empties the
///   slot; `super::commands::temple_debug_capture` is the caller that reaches
///   it, since it can be handed an image file of any size;
/// - the stored `ui_scale` cannot describe a screen. `settings::ScreenScaleSetting::is_sane`
///   refuses those at load and both writers measure rather than invent, so this
///   is the conversion being a total function rather than a claim that a zero
///   is reachable — the cost of being sure is one comparison, and the cost of
///   being wrong is a zero-size template.
pub fn hint_for_capture(
    screen: Option<&crate::ssot::ScreenSlice>,
    capture: (u32, u32),
    monitor_id: u32,
) -> Option<anchor::AnchorCalibration> {
    let screen = screen?;
    if !crate::ssot::screen_matches(&Some(*screen), capture, monitor_id) {
        return None;
    }
    if !screen.ui_scale.is_finite() || screen.ui_scale <= 0.0 {
        return None;
    }
    Some(anchor::AnchorCalibration {
        screen_w: capture.0,
        screen_h: capture.1,
        scale: anchor::scale_for_ui_scale(screen.ui_scale),
    })
}

/// The session's remembered plate, kept only while the shared slice still
/// agrees with it (POE-234 WI-2).
///
/// [`anchor::CheapHint`] is where the loop remembers WHERE it last saw the
/// Entrance plate, and it carries the scale it saw it at. That makes it a second
/// place a scale can live, and — until this function existed — one nothing could
/// clear: `ssot::geometry_recalibrate` empties the shared slice, but a panel
/// that is still on screen re-matches at the remembered scale on the very next
/// tick, reports [`anchor::CheapDetect::Anchored`], keeps [`sweep_could_help`]
/// from ever asking for a sweep, and hands [`publish_anchor_scale`] the number
/// the user just asked the app to forget — which then goes back into the emptied
/// slot and back into `settings.json`. Measured as unreachable it was not: it is
/// the ordinary case of pressing Recalibrate with the layout panel open.
///
/// So the slice is the authority over this too. Two rules, and the second is
/// what makes it more than a Recalibrate fix:
///
/// - **A hint that disagrees by more than one [`anchor::SCALE_STEP`] wins.** One
///   step is the finest disagreement this module can express, so anything larger
///   is the slice describing a screen the remembered plate is not on. This is
///   also what lets a merc frame fit CORRECT the temple mid-session: without it,
///   a session that first anchored on a drifting `MercOcr` seed would re-verify
///   its own copy of that scale for the rest of its life and never notice the
///   gold frame's better answer landing in the slice beside it.
/// - **A hint that was there and is GONE takes the plate with it.** That is the
///   Recalibrate case, and `answered` is what makes it distinguishable from the
///   other empty slice — the screen nothing has measured yet.
///
/// # Why the empty slice needs `answered` and cannot simply drop the plate
///
/// [`screen_from_anchor`] withholds a measurement the capture's height does not
/// corroborate, so there is a real configuration — a non-default in-game
/// UI-scale slider, on a machine whose recruit window is never opened — where
/// the temple anchors correctly every tick and the slice stays empty forever.
/// Dropping the plate on an empty slice alone would take the cheap tick's hinted
/// path away from exactly that user for the whole session: every tick would fall
/// to the nominating pass, whose seed is the one that is wrong there, and the
/// board would be read once per [`FULL_READ_EVERY_N_MISSES`] sweep instead of
/// once a second. `answered` costs one bool and confines the drop to a slice
/// that HAS held a scale for this session — an emptying, which is a decision,
/// rather than an emptiness, which is just an unanswered question.
///
/// The residue is honest and small: on that same machine Recalibrate cannot drop
/// a plate, because the module has never put a scale into the shared store for
/// the button to undo. What it does drop there is nothing, which is the correct
/// number of things.
///
/// Pure over plain data — the two hints and one bool — so all of it is testable
/// without a screen.
fn cheap_hint_for(
    hint: Option<anchor::AnchorCalibration>,
    held: Option<anchor::CheapHint>,
    answered: bool,
) -> Option<anchor::CheapHint> {
    let held = held?;
    match hint {
        Some(hint) => {
            ((hint.scale - held.calibration.scale).abs() <= anchor::SCALE_STEP).then_some(held)
        }
        None => (!answered).then_some(held),
    }
}

/// The hint the loop should use for this capture, read under the slice's own
/// lock and dropped before anything is done with it.
///
/// Lock-then-drop, like every other reader of an `AppState` mutex on this
/// thread: the anchor search that follows takes seconds, and holding the screen
/// slot across it would block `ssot::publish_screen` on the merc thread for all
/// of them.
fn hint_from_slice(
    app: &AppHandle,
    capture: (u32, u32),
    monitor_id: u32,
) -> (Option<anchor::AnchorCalibration>, Option<crate::ssot::ScreenScaleSource>) {
    let screen = {
        let state = app.state::<AppState>();
        let slot = state.screen.lock().unwrap_or_else(|e| e.into_inner());
        *slot
    };
    (
        hint_for_capture(screen.as_ref(), capture, monitor_id),
        screen.map(|s| s.source),
    )
}

/// The line to log when the loop takes its hint from a scale ANOTHER module
/// measured, or `None` when there is nothing to say.
///
/// Nothing to say covers three cases: no hint at all, a hint the loop already
/// announced (the slice is stable for as long as the screen is, and this runs
/// once a second), and a hint derived from the temple's own published value —
/// converting a number this module put there and reading it back is not news,
/// and saying so once a session per screen would still be one line claiming a
/// cross-module handoff that did not happen.
///
/// The cue is printed as its Rust variant name (`{:?}`) rather than as the
/// kebab-case wire string the Settings card renders: the source vocabulary
/// already has three spellings (the enum, `serde`'s wire strings, and
/// `geometry/view.ts`'s labels) and a fourth, hand-written one here would be the
/// one that drifts.
///
/// Pure over plain data, with the "already said" memory passed in, so the
/// once-per-value rule is testable without an `AppHandle`.
fn hint_line(
    said: &mut Option<anchor::AnchorCalibration>,
    hint: Option<anchor::AnchorCalibration>,
    source: Option<crate::ssot::ScreenScaleSource>,
) -> Option<String> {
    let hint = hint?;
    if *said == Some(hint) {
        return None;
    }
    let source = source?;
    if source == crate::ssot::ScreenScaleSource::TempleAnchor {
        // Still remembered: the temple's own value must not be re-announced if
        // merc later replaces it with a number that converts to the same hint.
        *said = Some(hint);
        return None;
    }
    *said = Some(hint);
    Some(format!(
        "Temple: anchoring on the remembered screen scale ({source:?}, ui_scale {:.3}) — \
         temple scale {:.3}, no search",
        anchor::ui_scale_for_scale(hint.scale),
        hint.scale
    ))
}

/// Publish what this capture anchored onto the shared screen slice (POE-234
/// WI-2) — the temple's whole WRITE of `crate::ssot::ScreenSlice`.
///
/// Called on every tick that produced a layout, which is every tick whose
/// anchor cleared [`anchor::NCC_FLOOR`] — the temple's half of the README's
/// "VERIFIED by the consuming module on first use". Two gates stand between an
/// anchor and the shared slice: [`screen_from_anchor`]'s `k` check here, and
/// `ssot::accepts` inside [`crate::ssot::publish_screen`], which refuses a
/// temple reading that only re-states a standing merc measurement within the
/// drift band. So calling this every tick is cheap by construction — a refusal
/// at either gate and an unchanged value all stop here.
///
/// Same shape as `mercenary::run`'s publish, deliberately: `publish_screen`
/// drops the screen guard before it returns, and `persist_settings` re-takes the
/// owner mutexes through `settings::from_state`, so no lock is held across
/// either.
///
/// **The early return on a withheld measurement is ahead of the persist, and
/// nothing tests that ordering** — both halves need an `AppHandle`, so there is
/// no seam to assert it through. What the tests do cover is the decision the
/// return is taken on ([`screen_from_anchor`], pure) and the rule the persist is
/// gated by (`ssot::should_remember_screen`, pure); the two-line composition
/// between them is read, not asserted.
///
/// # The one case where the two writers can push against each other
///
/// A merc frame fit ALWAYS replaces, and a temple anchor replaces whenever it is
/// outside the band. Both cannot happen at once any more — [`screen_from_anchor`]
/// refuses to publish anything the capture's own height does not corroborate,
/// and a temple scale within `K_TOLERANCE` of that is within the band of any
/// merc reading that is too — so the loops cannot overwrite each other tick by
/// tick. What is left is one publish apiece on a real change of screen.
fn publish_anchor_scale(
    app: &AppHandle,
    session: &mut Session,
    layout: &TempleLayout,
    hint: Option<anchor::AnchorCalibration>,
    capture: (u32, u32),
    monitor_id: u32,
    origin: (i32, i32),
) {
    let next = match screen_from_anchor(layout.scale, hint, capture, monitor_id, origin, now_ms())
    {
        Ok(next) => next,
        Err(line) => {
            // Once per distinct line, not once per tick: the condition holds for
            // as long as the panel is on screen at that scale, and this loop
            // runs at 1 Hz.
            if session.k_said.as_deref() != Some(line.as_str()) {
                session.k_said = Some(line.clone());
                crate::app_log(app, line);
            }
            return;
        }
    };
    let record = crate::ssot::publish_screen(app, next);
    if record.changed {
        crate::app_log(
            app,
            format!(
                "screen scale from temple anchor: ui_scale {:.3} (temple scale {:.3}, k {:.4})",
                next.ui_scale,
                layout.scale,
                anchor::TEMPLE_SCALE_PER_UI_SCALE
            ),
        );
    }
    // WI-B2's rule, unchanged: a measurement is written to disk, an estimate is
    // not, and the deadband inside `changed` is what keeps a 1 Hz loop off it.
    if crate::ssot::should_remember_screen(record.changed, next.source) {
        crate::persist_settings(app);
    }
}

/// The screen slice one temple anchor may publish, or the line saying why it may
/// not.
///
/// # Why an anchor above [`anchor::NCC_FLOOR`] is not automatically publishable
///
/// [`anchor::sweep_range`]'s ceiling is SOFT: the fine pass refines one nominate
/// step past the top nominee, so a capture whose true scale is above the ceiling
/// does not fail, it anchors APPROXIMATELY. Measured 2026-09-03 on a synthetic
/// plate at scale 2.10 against a 2.00 ceiling: the sweep answered **2.05 at NCC
/// 0.9390**, well above the floor. Before this gate that number would have been
/// converted through `k`, published as the screen's geometry, persisted, and
/// then used by POE-233 to place the lab OCR rects — a 2.5% error in a module
/// that never looked at a temple.
///
/// So the anchor has to be corroborated by something that is not the anchor, and
/// there are two such things. In order:
///
/// 1. **The standing hint**, when the slice holds one. It is a measurement of
///    this screen that did not come from this board, so an anchor within one
///    [`anchor::SCALE_STEP`] of it is corroborated by the strongest evidence
///    available — and, since the hint is what the anchor was searched at, this is
///    the ordinary case. The publish then goes on to `ssot::accepts`, which
///    refuses it as a restatement of the value it agrees with. That is the whole
///    point: a merc-frame-measured screen with a non-default UI slider reaches
///    the acceptance rule and is turned down there, instead of being stopped
///    here by an arithmetic that knows nothing about the slider.
/// 2. **The capture's own height**, when the slice is empty and there is nothing
///    else to ask. At the game's DEFAULT UI scale the temple scale is
///    `k * (height / 1200)` by both units' definitions, so a scale more than
///    [`K_TOLERANCE`] from it is one the screen does not account for.
///
/// # What this makes the temple, stated plainly
///
/// **With this gate the temple can only ever publish a scale within 1% of the
/// nominal one, or of a measurement already standing. It corroborates and
/// persists a verified seed; it does not teach the slice a new number.**
///
/// Everything else follows from that sentence. An empty slice is filled only
/// with a value the capture height predicts — which is what a temple-only
/// machine at the default slider needs, and it is a real measurement rather
/// than an assumption, because the anchor had to clear [`anchor::NCC_FLOOR`] to
/// get here. A slice that already holds something is only ever confirmed. And
/// the two cases the temple therefore cannot report are the two it cannot tell
/// apart anyway: a soft-ceiling approximation, and a genuine off-nominal UI
/// slider on a machine no other module has measured. The first must not be
/// published; the second is a real number the temple is choosing not to be the
/// sole source of. Merc's gold frame measures the slider case directly, and
/// after it does, the temple corroborates it through arm 1 for the rest of the
/// session.
///
/// A withheld measurement leaves a consumer failing closed, which the README's
/// placement rule already requires; a wrong one mis-scales every rect derived
/// from it with nothing on screen to say so.
///
/// Pure, so every arm is testable without a screen or an `AppHandle`.
fn screen_from_anchor(
    scale: f32,
    hint: Option<anchor::AnchorCalibration>,
    capture: (u32, u32),
    monitor_id: u32,
    origin: (i32, i32),
    measured_at_ms: u64,
) -> Result<crate::ssot::ScreenSlice, String> {
    let withheld = match hint {
        Some(hint) => hint_disagreement_line(scale, hint),
        None => unit_ratio_line(scale, capture.0, capture.1),
    };
    match withheld {
        Some(line) => Err(line),
        None => Ok(anchored_screen(scale, capture, monitor_id, origin, measured_at_ms)),
    }
}

/// The line to log when an anchor disagrees with the hint it was searched
/// against, or `None` when the two corroborate each other.
///
/// One [`anchor::SCALE_STEP`] is the same threshold [`cheap_hint_for`] uses on
/// the same two numbers, and for the same reason: it is the finest disagreement
/// this module's scale grid can express. Inside it the anchor confirms the
/// standing measurement; outside it, the anchor came from somewhere other than
/// the hint — the table row, or the sweep after the hint missed — and the temple
/// is not the module that gets to overrule a measurement of this screen with a
/// board it read (see [`screen_from_anchor`]'s second section).
///
/// Pure, and separate from the height check, because the two withhold for
/// genuinely different reasons and a user reading `app.log` needs to know which.
fn hint_disagreement_line(scale: f32, hint: anchor::AnchorCalibration) -> Option<String> {
    if (scale - hint.scale).abs() <= anchor::SCALE_STEP {
        return None;
    }
    Some(format!(
        "temple anchor not corroborated by the remembered screen scale: anchored at \
         {scale:.3} against a hint of {:.3} (ui_scale {:.3}) — the measurement was \
         withheld, and the shared screen scale is left to whatever else measures this \
         screen",
        hint.scale,
        anchor::ui_scale_for_scale(hint.scale),
    ))
}

/// The screen slice one temple anchor publishes.
///
/// Pure and separate from [`publish_anchor_scale`] so both derived fields are
/// pinned by tests: the unit conversion (`ssot`'s unit is not this module's, and
/// [`anchor::ui_scale_for_scale`] is the one place that crosses between them),
/// and whether the cue VERIFIES the screen — `ssot::verifies_the_screen`'s call,
/// not a literal here, for the same reason `mercenary::run::published_screen`
/// asks rather than answers.
///
/// Whether it MAY be published is [`screen_from_anchor`]'s question, not this
/// one: this builds the value, that decides the screen corroborates it.
///
/// `monitor_id` and `origin` come from the same `crate::capture::Capture` as the
/// pixels and are copied through untouched (POE-237): they are what lets
/// `ssot::screen_matches` tell a second 1920x1080 monitor from the remembered
/// one, so they must never be re-derived from anything else.
pub fn anchored_screen(
    scale: f32,
    capture: (u32, u32),
    monitor_id: u32,
    origin: (i32, i32),
    measured_at_ms: u64,
) -> crate::ssot::ScreenSlice {
    let source = crate::ssot::ScreenScaleSource::TempleAnchor;
    crate::ssot::ScreenSlice {
        width: capture.0,
        height: capture.1,
        ui_scale: anchor::ui_scale_for_scale(scale),
        source,
        measured_at_ms,
        verified_this_session: crate::ssot::verifies_the_screen(source),
        monitor_id,
        origin,
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
    /// How recently this session paid for a cold-start sweep on the screen it
    /// is looking at, and whether that screen still needs one.
    sweeps: SweepGate,
    /// The armed-ness the loop last announced, `None` before the first one.
    /// See [`gate_announcement`] — it is what makes `Waiting` one publish
    /// rather than one per nap. Not the only input: the slice's own status is
    /// read alongside it, so an announcement another thread wrote over comes
    /// back.
    gate_said: Option<bool>,
    /// The arm SOURCE the loop last put in `app.log`, `None` for "stood down"
    /// and for a loop that has not looked yet — one claim, not two. The publish
    /// above is keyed on armed-ness and this on the source, because the two
    /// answer different questions; [`gate_line`] owns the rule.
    source_said: Option<trigger::ArmSource>,
    /// The slice-derived hint the loop last ANNOUNCED, so the line saying it is
    /// running on another module's measurement is one line per value rather
    /// than one per second. See [`hint_line`], which owns the rule.
    hint_said: Option<anchor::AnchorCalibration>,
    /// The last `k` disagreement announced, for the same reason: the condition
    /// holds for as long as the panel is on screen at that scale, and
    /// [`publish_anchor_scale`] is reached on every anchored tick.
    k_said: Option<String>,
    /// The last [`rois_line`] announced, for the same reason and with a sharper
    /// cost: `app_log` keeps 50 entries, [`full_read`] runs at up to 1 Hz for as
    /// long as the panel is on screen, and an unconditional line there would
    /// evict every other diagnostic in the buffer within a minute. The rects are
    /// a function of `(origin, scale)` alone, so one line per distinct value
    /// says everything a repeat would.
    rois_said: Option<String>,
    /// The outside-set [`clipped_roi_announcement`] last announced, `None`
    /// before the loop has looked. Its own memory rather than a message in
    /// [`ErrorLog`] — see that function for why a rect-keyed, session-capped
    /// seam is the wrong shape for a condition a mouse drag re-states every
    /// tick.
    clipped_said: Option<Vec<&'static str>>,
    /// Whether the shared slice has held a scale for this screen at any point in
    /// this session. [`cheap_hint_for`]'s second input, and it is what separates
    /// a slice that was EMPTIED — Recalibrate — from one that was never filled.
    ///
    /// Per SCREEN, not per session: `tick` clears it whenever
    /// `ssot::drop_if_mismatched` reports that the capture no longer matches the
    /// remembered measurement, because after that the question "has anything
    /// measured this screen?" starts over with a new answer. Without the reset a
    /// player who moves the game to an unmeasured second monitor would carry the
    /// first monitor's `true` across, and the plate would be dropped every tick
    /// there on the strength of an emptying that belonged to another display.
    slice_answered: bool,
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
        sweeps: SweepGate::default(),
        gate_said: None,
        source_said: None,
        hint_said: None,
        k_said: None,
        rois_said: None,
        clipped_said: None,
        slice_answered: false,
    };
    // Backdated so the first iteration ticks immediately rather than after a
    // full cadence of doing nothing.
    let mut last_detect = Instant::now() - DETECT_INTERVAL_SLOW;

    loop {
        if *cancel.borrow() {
            break;
        }

        // No capture while alt-tabbed: the layout panel is not on screen, and a
        // full-screen anchor match every second would be pure heat. The arm
        // gate (POE-242, POE-246) is asked only behind it, for two reasons: the
        // loop publishes nothing at all while the game is not in front (see
        // `TempleStatus::Idle`), and an unfocused iteration does the same thing
        // either way. The panel clock is unaffected by the wait — it is a
        // deadline, not a countdown, so an alt-tab that outlasts it stands the
        // loop down on the tick focus comes back.
        let focused = game_focused(&app);
        let source = if focused {
            trigger::arm_source(
                trigger::arm_state(&app),
                session.state.panel_seen_ms,
                session.state.probe_pending(),
                now_ms(),
            )
        } else {
            None
        };
        let armed = source.is_some();
        if focused {
            // Decided UNDER the slice lock, against the status the slice
            // actually holds, so a foreign write (POE-171 finding 15) is
            // corrected on the next iteration rather than standing forever —
            // see `gate_announcement`.
            let said = session.gate_said;
            let mut announced = None;
            publish(&app, |slice| {
                if let Some(outcome) = gate_announcement(said, armed, slice.status) {
                    apply_gate(slice, outcome);
                    announced = Some(outcome);
                }
            });
            if announced.is_some() {
                session.gate_said = Some(armed);
            }
            // Separately from the publish: the source can change hands while the
            // gate stays open, and that transition is the one a smoke run reads.
            if let Some(line) = gate_line(&mut session.source_said, source) {
                crate::app_log(&app, line);
            }
        }

        let step = loop_step(
            focused,
            armed,
            last_detect.elapsed() >= session.state.detect_interval(),
        );
        // A `match` and not an `if`, so a fifth [`LoopStep`] cannot be added
        // without deciding here whether it captures.
        match step {
            LoopStep::Detect => {
                let started = Instant::now();
                let promoted = tick(&app, &mut session, &cancel, source);
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
            // Nothing to do but wait: `step.nap()` below is the whole of it.
            LoopStep::UnfocusedNap | LoopStep::DisarmedNap | LoopStep::Quantum => {}
        }

        if !nap(&cancel, step.nap()) {
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
    //
    // The ARM GATE is no longer exposed to it (POE-242): a disarmed loop
    // publishes `Waiting` once and then never again, so this `Idle` landing on
    // top of it would have stuck for the session. `gate_announcement` reads the
    // slice's own status and re-asserts the disarmed half, so the next
    // iteration puts `Waiting` back.
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
fn tick(
    app: &AppHandle,
    session: &mut Session,
    cancel: &watch::Receiver<bool>,
    source: Option<trigger::ArmSource>,
) -> bool {
    let grab = match crate::capture::capture_screen(app) {
        Ok(grab) => grab,
        Err(e) => {
            fail(app, session, format!("Temple: screen capture failed — {e}"));
            // Through `miss`, which spends the start-up probe like any other
            // tick (POE-246): a machine whose capture never succeeds must not
            // hold the gate open for the session.
            miss(app, session, true);
            return false;
        }
    };
    let monitor_id = grab.monitor_id;
    let origin = grab.origin;
    let img = grab.image;
    let capture = (img.width(), img.height());
    // Before ANY remembered geometry is read (POE-227): a screen scale measured
    // on another monitor or at another resolution is dropped from the shared
    // slice on the first capture that disagrees with it. FIRST, because the
    // hint below is derived from that slice — a stale value read one line
    // earlier would be the temple anchoring on the screen the game has left.
    if crate::ssot::drop_if_mismatched(app, capture, monitor_id) {
        // `slice_answered` is a claim about THIS screen, and this capture is a
        // different one — so the emptying that just happened is a resolution or
        // monitor change, not a Recalibrate, and the plate must not be dropped
        // as though the user had asked for it. (It is dropped anyway on the same
        // tick, by `anchor::AnchorCalibration::applies_to` inside the cheap
        // detect, which is the rule that owns capture-size staleness.)
        session.slice_answered = false;
    }
    let settings = settings_snapshot(app);
    // The temple's READ of the shared slice (POE-234 WI-2): whatever measured
    // this screen — merc's gold frame, an earlier temple anchor, last session
    // remembered, or the merc OCR line pitch, which is the one that drifts and
    // is bounded to about one `SCALE_STEP` here (see `hint_for_capture`) —
    // converted into this module's unit and handed to the anchor as its hint.
    // There is no second store to prune any more; the prune above IS this
    // module's prune.
    let (hint, hint_source) = hint_from_slice(app, capture, monitor_id);
    if let Some(line) = hint_line(&mut session.hint_said, hint, hint_source) {
        crate::app_log(app, line);
    }
    // BEFORE the cheap tick, because the plate the session remembers carries a
    // scale of its own and the slice is the authority over that too — see
    // `cheap_hint_for`. Dropping it here is what makes Recalibrate work with the
    // panel on screen, and what lets a merc frame fit correct a session that
    // anchored on a worse seed.
    session.cheap_hint = cheap_hint_for(hint, session.cheap_hint, session.slice_answered);
    session.slice_answered |= hint.is_some();
    // The cheap gate. A closed panel is what this loop looks at nearly all the
    // time, and it is the most expensive input the reader has — see
    // `anchor::detect_cheap`, which answers "anything here?" for ~1/80 of the
    // price of finding out the long way.
    let rearm = rearm_counter(app);
    let cheap = anchor::detect_cheap(&img, session.cheap_hint.as_ref());

    // The cold start (POE-234). The cheap tick's nominating scale is a GUESS on
    // a capture size nobody has measured, and a guess that misses looks exactly
    // like an empty screen: measured 2026-09-03 on a 1920x1080 laptop, the
    // panel was open, the true scale was 1.000, the width-derived guess was
    // 1.397 and the tick scored 0.66 — so the loop sat on "looking for the
    // layout panel" for the whole session and never once paid to find out.
    //
    // So a tick buys a sweep on the `FULL_READ_EVERY_N_MISSES` cadence — see
    // `SweepGate`, which owns that rule and the head start an uncalibrated
    // screen gets on it. ONE decision, taken here, for BOTH the cold path below
    // and the promoted read after it: those reach the same sweep, and a budget
    // spent on only one of them leaves the other paying 5.3 s per tick on a
    // screen whose background happens to nominate.
    //
    // Asked only when the cheap tick did NOT verify an anchor: a hint that
    // re-matched is the calibration's own verification succeeding, and a tick
    // that verified must not spend a budget kept for the ticks that could not.
    // That is what makes the calibrated cadence mean "N consecutive
    // verification failures" — the README's re-measure trigger — rather than
    // "N ticks".
    //
    // "Calibrated" is "this tick has a hint", which since WI-2 means the shared
    // slice holds a scale for this screen. That is the same transition the gate
    // was built on — `ssot::geometry_recalibrate` empties the slice, so the hint
    // goes with it and a screen that has just LOST its scale restarts the
    // countdown — with one store instead of two behind it.
    let calibrated = hint.is_some();
    let screen = SweepKey {
        monitor_id,
        width: capture.0,
        height: capture.1,
    };
    // Read BEFORE `on_detect` spends it, later in this tick: this is the loop's
    // FIRST tick, which has no remembered plate for the cheap half to re-match
    // whoever opened the gate for it (POE-246).
    let first_tick = session.state.probe_pending();
    let may_sweep = sweep_could_help(&cheap) && session.sweeps.allow(screen, calibrated);

    let mut sweep_ran = false;
    let mut swept = None;
    if !cheap.worth_reading() && may_sweep {
        sweep_ran = true;
        swept = cold_sweep(app, &img, hint.as_ref(), cancel);
        // A sweep the START-UP PROBE armed is the one that lands on a closed
        // panel by design — see `SweepGate::refund_probe`, which owns every
        // condition, the arm source included.
        session.sweeps.refund_probe(screen, source, swept.is_some());
    }

    if !wants_full_read(
        &mut session.state,
        &mut session.gate,
        &cheap,
        swept.is_some(),
        first_tick,
        rearm,
    ) {
        miss(app, session, false);
        // A sweep that found nothing still COST what a promoted tick costs, so
        // it is reported as one: `LoopState::note_tick_duration` ignores
        // promoted ticks, and letting seconds of deliberate work trip the
        // sticky `SLOW_TICK` backoff would slow the loop for the rest of the
        // session on the strength of a price it chose to pay once.
        return sweep_ran;
    }

    // A cheap tick that anchored has already done the expensive half of the
    // read's own first step, at full resolution and against the same floor —
    // so the promoted read takes that anchor instead of finding the plate a
    // second time. A sweep that anchored is the same fact from the cold path.
    // Every other promotion has no anchor to hand over.
    let layout = match (swept, cheap) {
        (Some(found), _) | (None, anchor::CheapDetect::Anchored(found)) => {
            reader::read_layout_at(&img, found)
        }
        // The sweep just paid for the pyramid on this frame. Re-running the
        // chain would re-pay it, and the two steps ahead of it are the cheap
        // ones: the same hint `detect_cheap` already tried in a window this
        // tick, and the table row for a capture size the sweep just searched
        // past. Nothing there can answer what the sweep could not.
        (None, _) if sweep_ran => {
            miss(app, session, false);
            return true;
        }
        // `may_sweep` is still unspent here: the only way to reach this arm
        // with a spent budget is the one above, which returns. So a promotion
        // over an uncalibrated screen sweeps on the same cadence as the cold
        // path, and a promotion that arrives between cadences does the hint and
        // table steps and reports a miss rather than blocking for seconds.
        (None, _) => match reader::read_layout_for_loop(
            &img,
            hint.as_ref(),
            may_sweep,
            &|| *cancel.borrow(),
        ) {
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

    // The sighting the arm gate's panel clock is measured from (POE-246): stamped
    // on every anchored tick, so the tail restarts while the panel is on screen
    // and only starts running once it is not.
    if session.state.on_detect(Some(now_ms())) == DetectOutcome::Found {
        crate::app_log(
            app,
            format!(
                "Temple: layout panel found (scale {:.3}, NCC {:.3})",
                layout.scale, layout.ncc
            ),
        );
    }
    // The temple's WRITE of the shared slice (POE-234 WI-2). Here rather than in
    // `full_read`, so all three anchor paths — the cheap tick's verified hint,
    // the cold sweep, and the promoted read — publish, including the ticks whose
    // board looked unchanged and bought no read. `ssot::accepts` is what makes
    // that affordable on a 1 Hz loop.
    publish_anchor_scale(app, session, &layout, hint, capture, monitor_id, origin);
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

/// One cold-start sweep of this capture, logged either way.
///
/// `None` on a miss AND on a cancelled sweep — both mean "no anchor came out of
/// this", which is what the caller needs. They are logged differently because
/// they mean different things to a user reading `app.log`, and neither is an
/// error: a screen with no layout panel on it is the state the loop lives in.
///
/// The miss line repeats on [`SweepGate`]'s cadence rather than once per
/// screen, which is the honest reading of what it says: the loop IS still
/// waiting for the panel, and one line every 30 s while an armed incursion has
/// no readable board is the record of that. [`ErrorLog`] does not cap it,
/// deliberately — this is not an error path and the cap exists for a failure
/// that re-runs every second.
///
/// # Blocking
///
/// This is the loop's longest single call: 5.3 s on a 1920x1080 capture in the
/// Linux container (release). `cancel` is polled inside it, between coarse
/// correlations, so a module switched off mid-sweep stops within roughly a
/// twenty-third of it rather than after all of it — see
/// [`anchor::anchor_for_loop`].
fn cold_sweep(
    app: &AppHandle,
    img: &DynamicImage,
    hint: Option<&anchor::AnchorCalibration>,
    cancel: &watch::Receiver<bool>,
) -> Option<anchor::Anchor> {
    // The whole loop-facing chain, hint included. A CALIBRATED screen reaches
    // here too now — on its cadence tick, when its hinted recheck has been
    // failing — and handing that calibration in is worth one coarse pass at
    // that one scale: `anchor_for_loop` searches it over the WHOLE capture,
    // where `detect_cheap`'s `recheck` only looked in a window around the
    // remembered origin, so a plate that MOVED is found for a fraction of the
    // sweep behind it. `anchor.rs` verifies the result against `NCC_FLOOR` like
    // anything else, so a hint that is wrong is never believed, only tried
    // first.
    let found = anchor::anchor_for_loop(img, hint, true, &|| *cancel.borrow());
    if *cancel.borrow() {
        return None;
    }
    match found {
        Ok(found) => Some(found),
        Err(_) => {
            crate::app_log(
                app,
                format!(
                    "Temple: sweep found no layout panel at {}x{} — waiting for the panel",
                    img.width(),
                    img.height()
                ),
            );
            None
        }
    }
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
///
/// # A retire keeps the ADVICE (POE-244)
///
/// It used to drop `advice` and `mode`, on the reasoning that a recommendation
/// is a move the player could still act on while the board is only a record.
/// That reasoning had the case backwards. `TempleStatus::PanelNotVisible` is
/// reached ONLY through this function, after [`RETIRE_AFTER`]
/// misses — which is to say it is what the whole INCURSION looks like, the
/// panel being closed the moment the player steps through the door. Dropping
/// the advice there left POE-244's door widget with no purple seal, no
/// `open <edge>` line and no architect name at exactly the point those are the
/// only things still on screen.
///
/// So the advice now outlives the panel, and the two things that end it are the
/// two that should: the next anchored read REPLACES it (`slice::project` writes
/// the whole slice), and a stand-down drops it with the module — which is also
/// when the overlay hides, so nothing stale is ever drawn. The bound on how
/// stale it can be is [`super::trigger::PANEL_TAIL_MS`], not this function.
fn miss(app: &AppHandle, session: &mut Session, errored: bool) {
    // `None`: a tick that found nothing leaves `panel_seen_ms` where it was, so
    // a miss can never extend the arm (POE-246).
    let retired = session.state.on_detect(None) == DetectOutcome::Retired;
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
    publish(app, |slice| apply_status(slice, outcome));
}

/// The text crops this module OCRs, named, in the order they are read.
///
/// **The one list.** [`panel_text`] reads it, [`clipped_text_rois`] checks it,
/// and `commands::temple_debug_capture` dumps it — three callers that must not
/// disagree about what "the text regions" are. Before this they each spelled
/// the pair out, so a third region added to one of them would have been read
/// without ever being checked for falling off the capture, or checked without
/// being dumped. A `[_; 2]` rather than a `Vec` so the arity is in the type: a
/// third region is a deliberate edit here and every caller recompiles.
///
/// The names are what the log line and the Temple page print, so they are
/// `&'static str` and are the KEY the once-per-value memory in
/// [`clipped_roi_announcement`] compares on.
pub fn text_regions(layout: &TempleLayout) -> [(&'static str, [i32; 4]); 2] {
    [
        ("panel", panel_rect(layout.origin, layout.scale)),
        ("remaining", remaining_rect(layout.origin, layout.scale)),
    ]
}

/// The panel's text: two bounded crops, as lines carrying their boxes in
/// CAPTURE px.
///
/// Never the whole frame — see the module note. The side panel's region is read
/// first and the budget line second, the order they are drawn in; since POE-243
/// that order is a convenience rather than a contract, because each line knows
/// where it was read and [`panel::read_panel`] sorts by that.
///
/// A crop that lands outside the capture contributes nothing rather than
/// failing the read: the two regions are independent, and losing the budget
/// line costs a warning, not the board.
fn panel_text(
    app: &AppHandle,
    session: &mut Session,
    img: &DynamicImage,
    layout: &TempleLayout,
) -> Option<Vec<crate::mercenary::geometry::OcrLineBox>> {
    let mut lines = Vec::new();
    for (_, rect) in text_regions(layout) {
        let Some((crop, origin)) = crop_clipped(img, rect) else {
            continue;
        };
        match panel::crop_lines(&crop, origin) {
            Ok(read) => lines.extend(read),
            Err(e) => {
                fail(app, session, format!("Temple: OCR failed — {e}"));
                return None;
            }
        }
    }
    Some(lines)
}

/// The three ROIs this read is about to use, as one line for `app.log`.
///
/// POE-230's measurement instrument. The rects are placed from `(origin, scale)`
/// and nothing else, so a fallback — an unread panel, a seal count that misses —
/// is either explained by the rect printed here or is not a geometry problem at
/// all. Before this, the only way to see a rect was to press the Debug button
/// and read the dump, which is not something a user hits while the bad read is
/// on screen.
///
/// `None` when the same line has already been said. `(origin, scale)` is stable
/// while the player stands still, [`full_read`] can run every second, and
/// `app_log`'s buffer is 50 entries deep — an unconditional line here would push
/// every other diagnostic out of it inside a minute, which is the opposite of
/// what a diagnostic is for.
///
/// Pure over plain data, with the "already said" memory passed in, so both the
/// format and the once-per-value rule are testable without an `AppHandle` — the
/// same shape [`hint_line`] uses.
fn rois_line(said: &mut Option<String>, origin: (i32, i32), scale: f32) -> Option<String> {
    let line = format!(
        "Temple: rois panel {:?} diamond {:?} remaining {:?}",
        panel_rect(origin, scale),
        diamond_rect(origin, scale),
        remaining_rect(origin, scale),
    );
    if said.as_deref() == Some(line.as_str()) {
        return None;
    }
    *said = Some(line.clone());
    Some(line)
}

/// The named text ROIs that fall ENTIRELY outside the capture, in region order.
///
/// POE-230 keyed these rects on the layout anchor, which is what stopped them
/// cutting the panel in half. It left one silence behind: [`crop_clipped`]
/// answers `None` for a rect with no pixels in the frame and [`panel_text`]
/// `continue`s, so a panel crop that has walked off the capture reads as a
/// panel with nothing printed on it. The board still publishes, the advisor
/// still ranks, and the only symptom is an offer list that is quietly empty.
///
/// A rect that is merely CLIPPED is not reported: the crop is smaller and the
/// read is still a read, which is the case the clipping exists for. What is
/// reported is the empty intersection.
fn clipped_text_rois(
    regions: &[(&'static str, [i32; 4])],
    width: u32,
    height: u32,
) -> Vec<(&'static str, [i32; 4])> {
    // The same arithmetic `crop_clipped` refuses on. Restated rather than
    // called: this asks "was anything there at all?", and the answer must not
    // change under a `crop_clipped` that grows a margin or a minimum size.
    let outside = |[x, y, w, h]: [i32; 4]| {
        x.max(0) >= (x + w).min(width as i32) || y.max(0) >= (y + h).min(height as i32)
    };
    regions.iter().copied().filter(|(_, rect)| outside(*rect)).collect()
}

/// What the user is told about one region that fell outside.
///
/// One sentence, used verbatim in `app.log` AND on the Temple page
/// ([`slice::TempleSlice::read_notice`]), so a screenshot and a pasted log say
/// the same thing. The wording names the one cause that produces this on a
/// working install: the capture is the whole monitor
/// (`capture::capture_screen`), so a read region outside it means the game is
/// not filling the monitor the module is grabbing.
fn clipped_roi_line(name: &str, rect: [i32; 4]) -> String {
    format!("Temple: {name} ROI {rect:?} is outside the capture — windowed client?")
}

/// The clipped-ROI lines to put in `app.log` this tick, or `None` when this
/// state has already been announced.
///
/// **Keyed on the region NAMES that are outside — not on their rects, and not
/// on the message.** A windowed client being DRAGGED moves both rects every
/// tick while the fact they report ("the panel crop is off the capture") does
/// not change, so a rect-keyed memory would say it at 1 Hz for the length of the
/// drag. The names are also the whole of what a reader needs repeated; the rects
/// are in the line, and the geometry itself is already reported by
/// [`rois_line`].
///
/// This is why the notice does NOT go through [`ErrorLog`], which is otherwise
/// the loop's say-once seam: that keys on the message, the message carries the
/// rect, and its [`MAX_DISTINCT_ERRORS`] slots are a SESSION-wide budget — a few
/// seconds of dragging would spend all of them and take every later temple error
/// down with them. A once-per-value memory of its own costs one `Session` field
/// and cannot exhaust anything.
///
/// A region that comes BACK inside and then leaves again IS announced again:
/// the second failure is news, and this must not degrade into say-it-once-ever.
/// (Whether the empty state is stored or the memory is cleared for it is not
/// observable — both leave the next non-empty set as news — so nothing depends
/// on which.)
///
/// Pure over plain data with the memory passed in, the shape [`rois_line`] and
/// [`hint_line`] use.
fn clipped_roi_announcement(
    said: &mut Option<Vec<&'static str>>,
    outside: &[(&'static str, [i32; 4])],
) -> Option<Vec<String>> {
    let names: Vec<&'static str> = outside.iter().map(|(name, _)| *name).collect();
    if said.as_deref() == Some(names.as_slice()) {
        return None;
    }
    *said = Some(names);
    if outside.is_empty() {
        // The state changed back to "everything is in frame". Worth remembering,
        // not worth a line — nothing is wrong, and a log that narrates recovery
        // is a log that evicts the failure it recovered from.
        return None;
    }
    Some(outside.iter().map(|(name, rect)| clipped_roi_line(name, *rect)).collect())
}

/// The expensive half: 13 plates, the side panel, the diamond, the advisor.
/// Screen height the shared `ui_scale` unit calls 1.0 — the height of the merc
/// reference fixture, restated here rather than imported so this file does not
/// grow a dependency on a slice it deliberately does not read. The number's
/// owner is [`crate::ssot::ScreenSlice`]'s unit note.
const UI_SCALE_REFERENCE_HEIGHT: f32 = 1200.0;

/// The `k` CHECK (POE-234 WI-2): the line that fires when the ratio this board
/// implies disagrees with the constant the app converts through — and, since it
/// is [`screen_from_anchor`]'s gate, the reason a measurement is withheld from
/// the shared slice.
///
/// It was POE-227 D3's unconditional instrumentation line, printed on every full
/// read so a second machine's `k` could be collected. That job is done — the
/// reading is committed as [`anchor::TEMPLE_SCALE_PER_UI_SCALE`] and both
/// directions of the conversion now run through it — so what is left worth
/// saying is the DISAGREEMENT: this board's own `temple_scale / (height / 1200)`
/// against the constant, when the two are more than [`K_TOLERANCE`] apart.
///
/// Why that is the right thing to print rather than the ratio itself: the
/// constant is documented as good to about a per cent, which is one
/// [`anchor::SCALE_STEP`] at scale 1.0 and lands well clear of
/// [`anchor::NCC_FLOOR`]. Inside that, a printed ratio is noise the reader has
/// to decide about; outside it, the anchor and the screen it came off disagree
/// about the same fact, and that is worth both a line and a withheld publish.
///
/// The denominator is the shared unit's DEFINITION (`height / 1200`), not the
/// `ui_scale` standing in the slice. Deliberately, and it is what makes this a
/// second opinion rather than a mirror: the slice's value may be one the temple
/// itself published, and dividing a temple scale by a `ui_scale` derived from a
/// temple scale would compare `k` with itself and never fire.
///
/// Pure, and separated from the log call, so both the arithmetic and the
/// threshold are testable without a screen or an `AppHandle`.
fn unit_ratio_line(scale: f32, capture_width: u32, capture_height: u32) -> Option<String> {
    let k = scale / (capture_height as f32 / UI_SCALE_REFERENCE_HEIGHT);
    let off = (k - anchor::TEMPLE_SCALE_PER_UI_SCALE).abs() / anchor::TEMPLE_SCALE_PER_UI_SCALE;
    if off <= K_TOLERANCE {
        return None;
    }
    Some(format!(
        "temple anchor not corroborated by the capture: unit ratio k={k:.4} differs from \
         the {:.4} this app converts through by {:.1}% (scale {scale:.3}, capture \
         {capture_width}x{capture_height}) — the measurement was withheld, and the shared \
         screen scale is left to whatever else measures this screen",
        anchor::TEMPLE_SCALE_PER_UI_SCALE,
        off * 100.0
    ))
}

/// How far this board's own unit ratio may sit from
/// [`anchor::TEMPLE_SCALE_PER_UI_SCALE`] before [`unit_ratio_line`] says so.
///
/// One per cent, which is the accuracy the constant itself claims — see its doc
/// for which half of it is measured and which is nominal — and one
/// `anchor::SCALE_STEP` at scale 1.0. A hint that far off the truth still
/// anchors well clear of [`anchor::NCC_FLOOR`] (0.9603 against the peak's 0.9936
/// on `board-ref-1374.png`), so inside this the conversion is doing its job and
/// there is nothing to report.
const K_TOLERANCE: f32 = 0.01;

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
    // Before any crop, so a read that fails halfway still leaves the geometry it
    // was working from in the log.
    if let Some(line) = rois_line(&mut session.rois_said, layout.origin, layout.scale) {
        crate::app_log(app, line);
    }
    // …and the honesty half of the same measurement. `panel_text` steps over a
    // crop with no pixels in the frame, which is the right thing to do with the
    // budget line and the wrong thing to do SILENTLY: an empty panel crop
    // produces an empty offer list that reads exactly like a panel with no
    // architects on it. The LOG is said once per outside-set (see
    // `clipped_roi_announcement`); the SLICE carries the notice on every read it
    // is true of, because the page shows a state and not a history.
    let clipped = clipped_text_rois(&text_regions(&layout), img.width(), img.height());
    if let Some(lines) = clipped_roi_announcement(&mut session.clipped_said, &clipped) {
        for line in lines {
            crate::app_log(app, line);
        }
    }
    let read_notice = (!clipped.is_empty()).then(|| {
        clipped
            .iter()
            .map(|(name, rect)| clipped_roi_line(name, *rect))
            .collect::<Vec<_>>()
            .join("; ")
    });

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
            read_notice,
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
        // The calibration THIS capture measured, which is what the page's
        // "anchor calibration" row means: the scale the board in front of the
        // user was actually read at, not a remembered one. Since POE-234 WI-2
        // there is no remembered one to confuse it with — the module's only
        // store is the shared screen slice, and this is the read's own answer.
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

    /// The check says nothing while the constant holds — which is the state
    /// every correctly-converting machine is in, so this is the assertion that
    /// keeps `app.log` readable.
    ///
    /// The input is derived, not copied: `k * (height / 1200)` is the temple
    /// scale a 1080p screen must anchor at IF the constant is right, so this is
    /// the ratio agreeing with itself and the line must not fire.
    #[test]
    fn a_board_that_agrees_with_the_constant_says_nothing() {
        let scale = anchor::scale_for_ui_scale(1080.0 / UI_SCALE_REFERENCE_HEIGHT);

        assert_eq!(unit_ratio_line(scale, 1920, 1080), None);
    }

    /// The reference screen is the case that pins the DIVISOR: the shared unit
    /// is 1.0 by definition at 1200 px, so a board anchoring there at exactly
    /// `k` agrees, and one anchoring at the temple scale a 1080p screen would
    /// give does not. A check that divided by the WIDTH, or by the wrong
    /// reference height, gets both of these backwards.
    #[test]
    fn the_divisor_is_the_capture_height_against_the_shared_units_own_reference() {
        assert_eq!(
            unit_ratio_line(anchor::TEMPLE_SCALE_PER_UI_SCALE, 1920, 1200),
            None,
            "at the reference height the ratio IS the scale, and it agrees",
        );

        let line = unit_ratio_line(anchor::TEMPLE_SCALE_PER_UI_SCALE, 1920, 1080)
            .expect("the same scale on a shorter screen is a different ratio");
        assert!(
            line.contains("k=1.2346"),
            "1.1111 / (1080/1200) = 1.2346, not {line}",
        );
    }

    /// The threshold is `K_TOLERANCE` either side, and the line names the gap.
    ///
    /// Derived from the tolerance rather than from a literal ratio: half a
    /// tolerance off must stay silent and two of them must not, so a constant
    /// edited without its threshold cannot pass this by accident.
    #[test]
    fn only_a_ratio_outside_the_tolerance_is_worth_a_line() {
        let agrees = 1080.0 / UI_SCALE_REFERENCE_HEIGHT;

        assert_eq!(
            unit_ratio_line(
                anchor::scale_for_ui_scale(agrees) * (1.0 + K_TOLERANCE / 2.0),
                1920,
                1080,
            ),
            None,
            "inside the accuracy the constant claims there is nothing to report",
        );

        let line = unit_ratio_line(
            anchor::scale_for_ui_scale(agrees) * (1.0 + 2.0 * K_TOLERANCE),
            1920,
            1080,
        )
        .expect("twice the tolerance is worth saying");
        assert!(
            line.contains("2.0%"),
            "the line must name the gap so a user can send back how far off it is: {line}",
        );
    }

    // --------------------------------------------- the panel state machine --

    /// A moment for the sighting stamps, in the shape [`now_ms`] returns
    /// (2025-09-02 08:00 UTC). Nothing reads its value — the assertions are all
    /// on differences from it.
    const SEEN: u64 = 1_756_800_000_000;

    /// A panel is not retired on its first miss — the anchor loses a fading
    /// panel for a frame. Fails if `RETIRE_AFTER` is applied off by one.
    ///
    /// Ported to `Option<u64>` with POE-246: a miss is a tick with no sighting
    /// to stamp, which is the same input this always meant.
    #[test]
    fn a_live_panel_survives_one_missed_anchor() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        assert_eq!(state.on_detect(None), DetectOutcome::Missed);
        assert!(state.live, "one miss does not retire a panel");
        assert_eq!(state.on_detect(None), DetectOutcome::Retired);
        assert!(!state.live);
    }

    /// A successful anchor between two misses resets the counter, so a panel
    /// that flickers is never retired. Fails if `on_detect` does not clear
    /// `misses` on success.
    #[test]
    fn an_anchor_between_misses_resets_the_retirement_count() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        state.on_detect(None);
        assert_eq!(state.on_detect(Some(SEEN)), DetectOutcome::Held);
        assert_eq!(state.on_detect(None), DetectOutcome::Missed, "the count restarted");
        assert!(state.live);
    }

    /// The first anchor after nothing is `Found`, which is the log line.
    #[test]
    fn the_first_anchor_reports_found_and_later_ones_do_not() {
        let mut state = LoopState::default();

        assert_eq!(state.on_detect(Some(SEEN)), DetectOutcome::Found);
        assert_eq!(state.on_detect(Some(SEEN + 1_000)), DetectOutcome::Held);
    }

    /// The write POE-246's whole stand-down rule rests on: every anchored tick
    /// re-stamps the sighting, so the tail is measured from the LAST one.
    ///
    /// Fails if the stamp is written once (on `Found` only) — the loop would
    /// then stand down [`trigger::PANEL_TAIL_MS`] after a panel OPENED rather
    /// than after it closed, which is the 14:37:00 bug with a longer fuse.
    #[test]
    fn every_anchored_tick_re_stamps_the_sighting() {
        let mut state = LoopState::default();

        state.on_detect(Some(SEEN));
        state.on_detect(Some(SEEN + 30_000));

        assert_eq!(state.panel_seen_ms, Some(SEEN + 30_000));
    }

    /// And a miss never does. Fails if `on_detect` stamps unconditionally: a
    /// loop looking at an empty screen would then hold its own gate open
    /// forever, which is the free-running capture POE-242 removed.
    #[test]
    fn a_tick_that_found_nothing_leaves_the_last_sighting_where_it_was() {
        let mut state = LoopState { live: true, ..LoopState::default() };
        state.on_detect(Some(SEEN));

        state.on_detect(None);

        assert_eq!(state.panel_seen_ms, Some(SEEN));
    }

    /// Retiring a panel is not the same event as losing sight of it: the tail
    /// keeps running from the last sighting for the seconds a player needs to
    /// close a panel and reopen it. Fails if retirement clears the stamp — the
    /// loop would stand down two ticks after every close.
    #[test]
    fn retiring_a_panel_does_not_clear_the_sighting_the_tail_is_measured_from() {
        let mut state = LoopState::default();
        state.on_detect(Some(SEEN));

        state.on_detect(None);
        assert_eq!(state.on_detect(None), DetectOutcome::Retired);

        assert_eq!(state.panel_seen_ms, Some(SEEN));
    }

    /// The start-up probe is a debt one tick settles, whatever that tick found —
    /// a clean miss and a FAILED screen grab alike, because [`miss`] folds both
    /// through this call with no sighting to stamp.
    ///
    /// Fails if the probe is spent only on a sighting, or only on a tick that
    /// ran clean: either way a machine whose capture keeps failing holds the
    /// gate open for the rest of the session, which is the free-running capture
    /// POE-242 removed with an error message on top of it.
    #[test]
    fn the_first_tick_spends_the_start_up_probe_even_when_it_could_not_look() {
        let mut state = LoopState::default();
        assert!(state.probe_pending(), "a loop that has not looked owes one look");

        state.on_detect(None);

        assert!(!state.probe_pending());
    }

    /// The 17:28:31 case end to end, over the two pure pieces the loop composes:
    /// a module switched on with the panel already open and Alva silent gets its
    /// probe tick, the tick anchors, and the panel itself holds the gate open
    /// from there.
    ///
    /// Fails if the probe does not reach the gate, or if the sighting it takes
    /// does not — either way the loop stands down in the second it started and
    /// the advice blinks and disappears.
    #[test]
    fn a_probe_tick_that_anchors_hands_the_gate_over_to_the_panel() {
        let mut state = LoopState::default();
        let arm = trigger::ArmState::default();
        assert_eq!(
            trigger::arm_source(arm, state.panel_seen_ms, state.probe_pending(), SEEN),
            Some(trigger::ArmSource::StartupProbe),
            "the probe tick is allowed to run",
        );

        state.on_detect(Some(SEEN));

        assert_eq!(
            trigger::arm_source(arm, state.panel_seen_ms, state.probe_pending(), SEEN + 1_000),
            Some(trigger::ArmSource::PanelOnScreen),
        );
    }

    /// The other half: a probe that finds nothing stands the loop down on the
    /// next iteration, which is POE-242's behaviour for a screen with no panel
    /// on it. Fails if the probe survives its own tick.
    #[test]
    fn a_probe_tick_that_finds_nothing_stands_the_loop_down() {
        let mut state = LoopState::default();

        state.on_detect(None);

        assert_eq!(
            trigger::arm_source(
                trigger::ArmState::default(),
                state.panel_seen_ms,
                state.probe_pending(),
                SEEN,
            ),
            None,
        );
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

    // -------------------------------------------- the cold-start sweep gate --

    fn screen(monitor_id: u32, width: u32, height: u32) -> SweepKey {
        SweepKey {
            monitor_id,
            width,
            height,
        }
    }

    /// The first tick on a screen nobody has measured sweeps at once — this is
    /// the bug POE-234 opened on, and a gate that made the user wait out a
    /// cadence for the FIRST answer would leave it unfixed for 30 s.
    #[test]
    fn the_first_tick_on_an_uncalibrated_screen_sweeps_at_once() {
        let mut gate = SweepGate::default();

        assert!(gate.allow(screen(7, 1920, 1080), false));
    }

    /// …and the ticks after it wait out [`FULL_READ_EVERY_N_MISSES`] before the
    /// next one. The sweep is seconds of work on the loop's own thread, so the
    /// tick after a sweep must not buy another.
    ///
    /// Pinned as the exact tick the next sweep lands on, not as "eventually":
    /// an off-by-one either sweeps twice in a row (2× the cost for one answer)
    /// or drifts the cadence apart from the periodic full read it is
    /// deliberately tied to.
    #[test]
    fn an_uncalibrated_screen_sweeps_again_only_after_the_cadence() {
        let mut gate = SweepGate::default();
        let laptop = screen(7, 1920, 1080);
        assert!(gate.allow(laptop, false), "precondition: the first sweeps");

        let mut swept_on = Vec::new();
        for tick in 2..=(2 * FULL_READ_EVERY_N_MISSES) {
            if gate.allow(laptop, false) {
                swept_on.push(tick);
            }
        }

        assert_eq!(
            swept_on,
            vec![FULL_READ_EVERY_N_MISSES + 1],
            "the sweeps after the first landed on ticks {swept_on:?}, not on \
             tick {} alone",
            FULL_READ_EVERY_N_MISSES + 1
        );
    }

    /// A screen whose scale is already known loses the FIRST-tick sweep and
    /// keeps the cadence: refused for [`FULL_READ_EVERY_N_MISSES`] ticks, then
    /// swept on the one after.
    ///
    /// Both halves are load-bearing and they pull opposite ways. Refusing the
    /// first tick is what stops every armed incursion costing 5.3 s over a
    /// closed panel the hint would have re-anchored anyway. Sweeping on the
    /// cadence is `desktop/src/lib/README.md`'s "Screen Geometry (SSOT)"
    /// lifecycle: a measurement is re-taken when the consuming module's own
    /// verification fails, and a hinted recheck that has missed a whole cadence
    /// is that failure — the in-game UI-scale change no prune can see, because
    /// the capture size never moved.
    ///
    /// Fails both ways: a gate that refuses outright leaves that screen with no
    /// automatic recovery at all, and one that sweeps at once makes the
    /// calibration worthless.
    #[test]
    fn a_calibrated_screen_skips_the_first_tick_and_keeps_the_cadence() {
        let mut gate = SweepGate::default();
        let laptop = screen(7, 1920, 1080);

        let mut swept_on = Vec::new();
        for tick in 1..=(2 * FULL_READ_EVERY_N_MISSES) {
            if gate.allow(laptop, true) {
                swept_on.push(tick);
            }
        }

        assert_eq!(
            swept_on,
            vec![FULL_READ_EVERY_N_MISSES + 1],
            "a calibrated screen swept on ticks {swept_on:?}; it must skip the \
             first and sweep once, on tick {}",
            FULL_READ_EVERY_N_MISSES + 1
        );
    }

    /// A screen that LOSES its calibration sweeps at once rather than serving
    /// out a countdown that belonged to another state.
    ///
    /// This is how Recalibrate reaches this gate, and the only way it does:
    /// `ssot::geometry_recalibrate` empties the shared screen scale, so
    /// [`hint_for_capture`] stops answering and `calibrated` goes false, while
    /// the `temple_rearm` counter it bumps alongside is deliberately NOT an
    /// input here. Fails if the gate only restarts on a key change — the user
    /// would press the button and wait 29 more ticks for the sweep it exists to
    /// buy.
    #[test]
    fn a_screen_that_loses_its_calibration_sweeps_without_waiting_out_the_cadence() {
        let mut gate = SweepGate::default();
        let laptop = screen(7, 1920, 1080);
        assert!(gate.allow(laptop, false), "the uncalibrated screen sweeps");
        assert!(!gate.allow(laptop, true), "then a calibration lands");

        assert!(gate.allow(laptop, false), "and Recalibrate clears it");
    }

    /// Nothing else restarts the countdown. The settings commands bump
    /// `temple_rearm` on every change and this gate cannot see it, so three
    /// settings edits in a row cost no sweeps at all — which is the whole
    /// reason that counter was taken out of the signature.
    ///
    /// Fails if a second input is reintroduced that resets the countdown:
    /// re-running the same call must decrement, never restart.
    #[test]
    fn repeated_identical_ticks_only_ever_decrement_the_countdown() {
        let mut gate = SweepGate::default();
        let laptop = screen(7, 1920, 1080);
        assert!(gate.allow(laptop, false), "precondition: the first sweeps");

        let sweeps = (0..FULL_READ_EVERY_N_MISSES - 1)
            .filter(|_| gate.allow(laptop, false))
            .count();

        assert_eq!(
            sweeps, 0,
            "{sweeps} of the {} ticks inside the cadence bought a sweep",
            FULL_READ_EVERY_N_MISSES - 1
        );
    }

    /// The probe's sweep is free when it finds nothing: the tick that has
    /// something to find still gets the uncalibrated screen's head start.
    ///
    /// Fails if the refund is dropped — the first ARMED tick over an open panel
    /// then waits a whole cadence for the sweep that would have read it, on the
    /// strength of a sweep spent seconds earlier on a closed one.
    #[test]
    fn a_probe_sweep_that_found_nothing_gives_the_head_start_back() {
        let mut gate = SweepGate::default();
        let laptop = screen(7, 1920, 1080);
        assert!(gate.allow(laptop, false), "precondition: the probe tick sweeps");

        gate.refund_probe(laptop, Some(trigger::ArmSource::StartupProbe), false);

        assert!(gate.allow(laptop, false), "the next tick still sweeps at once");
    }

    /// A probe sweep that ANCHORED pays like any other: it bought the answer the
    /// budget exists for. Fails if the refund ignores the outcome, which would
    /// let a screen whose panel is being read sweep again on the next
    /// non-verified tick.
    #[test]
    fn a_probe_sweep_that_anchored_spends_the_budget_like_any_other() {
        let mut gate = SweepGate::default();
        let laptop = screen(7, 1920, 1080);
        assert!(gate.allow(laptop, false), "precondition: the probe tick sweeps");

        gate.refund_probe(laptop, Some(trigger::ArmSource::StartupProbe), true);

        assert!(!gate.allow(laptop, false), "the cadence is running");
    }

    /// A first tick CLIENT.TXT armed pays like any other, and the app started
    /// inside a temple is that case: the loop keeps ticking after it, so a refund
    /// there buys a second 5.3 s sweep on the very next tick.
    ///
    /// Fails if the refund reads "this is the first tick" instead of "the probe
    /// is what opened the gate" — the two are the same tick only when nothing
    /// else armed the loop.
    #[test]
    fn a_first_sweep_under_a_live_client_txt_arm_is_not_given_back() {
        let mut gate = SweepGate::default();
        let laptop = screen(7, 1920, 1080);
        assert!(gate.allow(laptop, false), "precondition: this tick sweeps");

        gate.refund_probe(
            laptop,
            Some(trigger::ArmSource::Trigger(trigger::ArmReason::AlvaLine)),
            false,
        );

        assert!(!gate.allow(laptop, false), "the cadence is running");
    }

    /// A different capture size is a different screen and sweeps immediately —
    /// the scale is a property of the render resolution, so nothing measured at
    /// 1920x1080 says anything about 2560x1440, and the countdown the old size
    /// was part-way through does not apply to it.
    #[test]
    fn a_capture_size_change_sweeps_without_waiting_out_the_cadence() {
        let mut gate = SweepGate::default();
        assert!(gate.allow(screen(7, 1920, 1080), false));
        assert!(
            !gate.allow(screen(7, 1920, 1080), false),
            "precondition: the cadence is running",
        );

        assert!(gate.allow(screen(7, 2560, 1440), false));
    }

    /// …and so is a different DISPLAY at the same resolution. Fails if the key
    /// is the dimensions alone, which is the case POE-237 added the monitor id
    /// for: two identical 1080p monitors are not one screen.
    #[test]
    fn a_monitor_change_at_the_same_resolution_sweeps_without_waiting() {
        let mut gate = SweepGate::default();
        assert!(gate.allow(screen(7, 1920, 1080), false));
        assert!(
            !gate.allow(screen(7, 1920, 1080), false),
            "precondition: the cadence is running",
        );

        assert!(gate.allow(screen(9, 1920, 1080), false));
    }

    /// A tick whose hint re-anchored the panel has its scale, so it neither
    /// needs a sweep nor may spend the budget for one — that is what keeps the
    /// calibrated cadence counting verification FAILURES rather than ticks.
    /// The other two outcomes can both reach the sweep and both count.
    ///
    /// Fails if the guard is inverted or dropped: a working panel would then
    /// walk the countdown down and buy a 5.3 s sweep it has no use for.
    #[test]
    fn only_a_tick_that_verified_an_anchor_is_kept_off_the_sweep_budget() {
        let anchored = anchor::CheapDetect::Anchored(anchor::Anchor {
            origin: (960, 713),
            scale: 1.0,
            ncc: 0.99,
        });

        assert!(!sweep_could_help(&anchored), "a verified anchor needs no sweep");
        assert!(sweep_could_help(&saw_something()), "a candidate can reach it");
        assert!(sweep_could_help(&saw_nothing()), "and so can an empty screen");
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
            !wants_full_read(&mut state, &mut gate, &saw_nothing(), false, false, 0),
            "precondition: a quiet screen is a cheap tick — the loop has been\n             running, so its start-up probe is long spent (POE-246)",
        );

        assert!(
            wants_full_read(&mut state, &mut gate, &saw_nothing(), false, false, 1),
            "the bump buys a read",
        );
        assert!(
            !wants_full_read(&mut state, &mut gate, &saw_nothing(), false, false, 1),
            "and exactly one — the tick after it is cheap again",
        );
        assert!(!wants_full_read(&mut state, &mut gate, &saw_nothing(), false, false, 1));
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

        assert!(wants_full_read(&mut state, &mut gate, &saw_something(), false, false, 1));

        assert!(
            gate.layout_wants_read(board, 1),
            "the board the re-arm was pressed over must be read again",
        );
    }

    /// A sweep that anchored buys the read, whatever the cheap tick said.
    ///
    /// The cold path's whole point: on the screen POE-234 was opened on the
    /// cheap tick reports NOTHING while the panel is open, so a promotion rule
    /// that read only `CheapDetect::worth_reading` would throw the anchor the
    /// sweep just paid 5.3 s for straight back into `miss` — no read, no scale
    /// published to the shared slice, and the next tick's sweep gated behind
    /// the cadence.
    ///
    /// Fails if `|| swept` is dropped from the promotion.
    #[test]
    fn a_sweep_that_anchored_buys_the_read() {
        let mut state = LoopState::default();
        let mut gate = slice::ReadGate::default();

        assert!(wants_full_read(
            &mut state,
            &mut gate,
            &saw_nothing(),
            true,
            false,
            0
        ));
    }

    /// The probe tick promotes whatever the cheap tick said, and that is what
    /// makes a module toggled on over an open panel read it.
    ///
    /// The cheap half cannot answer on a starting loop: `detect_cheap`'s hint
    /// carries an origin only a previous read produces, so tick one is the
    /// nominating pass — measured at 0.66 against a 0.70 floor on the 1080p
    /// laptop this was reported from, with the panel open. Fails if the probe is
    /// dropped from the promotion, which leaves the probe looking through the
    /// one pass that cannot see the panel it exists to find.
    #[test]
    fn the_start_up_probe_tick_pays_for_the_read_a_cheap_tick_would_have_skipped() {
        let mut state = LoopState::default();
        let mut gate = slice::ReadGate::default();

        assert!(wants_full_read(&mut state, &mut gate, &saw_nothing(), false, true, 0));
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

        assert!(wants_full_read(&mut state, &mut gate, &saw_nothing(), false, false, 0));
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

    /// The rect reproduces the diamond centre measured on BOTH captures whose
    /// origin and scale are recorded, at their own scales and window sizes —
    /// [`DIAMOND_DX_REF`]'s table.
    ///
    /// The tolerance is ±2 px, not the ±7 the screen-edge form needed: two
    /// independent captures put this centre at the same reference offset, so
    /// what is left is integer rounding. A constant nudged by 3 ref px fails
    /// here, which is what the retired form could not detect.
    ///
    /// The third recorded board (`2026-08-07_19-28-36`) is asserted separately
    /// in [`the_diamond_rect_reproduces_the_third_board_at_the_scale_its_panel_implies`],
    /// at the scale its own panel border implies rather than the one its anchor
    /// recorded.
    #[test]
    fn the_diamond_rect_reproduces_both_measured_centres() {
        // (origin, scale, measured diamond centre)
        let boards = [
            ((960i32, 713i32), 1.0000f32, (1413i32, 217i32)),
            ((673, 682), 1.0000, (1126, 186)),
        ];
        for (origin, scale, want) in boards {
            let [x, y, w, h] = diamond_rect(origin, scale);
            let centre = (x + w / 2, y + h / 2);
            assert!(
                (centre.0 - want.0).abs() <= 2 && (centre.1 - want.1).abs() <= 2,
                "{origin:?} @ {scale}: expected a centre at {want:?}, got {centre:?}",
            );
        }
    }

    /// The third recorded capture, at the scale its own panel border implies —
    /// **1.111**, not the 1.13 its anchor recorded. See [`DIAMOND_DX_REF`]'s
    /// anchor-accuracy note: 537 x 416 px of panel border against the reference
    /// 484 x 374 gives 1.1095/1.1123, and at 1.111 this constant reproduces that
    /// board's measured diamond centre to the same ±2 px the other two get.
    ///
    /// Which makes it a third measurement of the constant, not an exception to
    /// it. Fails if the offsets are ever re-fitted to split the difference with
    /// the anchor's 1.13, which would move them off all three captures at once.
    #[test]
    fn the_diamond_rect_reproduces_the_third_board_at_the_scale_its_panel_implies() {
        let [x, y, w, h] = diamond_rect((745, 768), 1.111);
        let centre = (x + w / 2, y + h / 2);

        assert!(
            (centre.0 - 1249).abs() <= 2 && (centre.1 - 218).abs() <= 2,
            "at 1.111 the 1539 board's diamond centre is (1249, 218), got {centre:?}",
        );
    }

    /// What the same board costs at the scale it is actually ANCHORED at, which
    /// is the error budget [`DIAMOND_W_REF`] is sized against: the rect lands
    /// (8, −10) px off, and both of the things that bound its width still hold
    /// from there.
    ///
    /// Three assertions, because three different wrong changes are in scope and
    /// each breaks exactly one: the displacement pins the offsets, the fan check
    /// fails if the width is trimmed, and the architect-ink check fails if it is
    /// widened.
    ///
    /// The ink check is against the SURVIVAL bound, not the ink itself, and the
    /// difference is the finding: a 1.7% high anchor puts this crop's right edge
    /// **10 px past** where the board drew that ink. What keeps the read correct
    /// is that a clipped text fragment fails [`markers::MIN_BLOB_HEIGHT`], which
    /// [`DIAMOND_W_REF`]'s sweep measures as holding until the edge is 12 ref px
    /// past the ink (x 1526 against 1514 on the fixture). So the real clearance
    /// on this board is **3 ref px of filter tolerance**, not of geometry — which
    /// is the honest reason the width cannot go up.
    #[test]
    fn the_anchors_own_error_on_that_board_stays_inside_the_rects_margins() {
        let scale = 1.13f32;
        let origin = (745i32, 768i32);
        let [x, y, w, h] = diamond_rect(origin, scale);
        let centre = (x + w / 2, y + h / 2);
        let (off_x, off_y) = (centre.0 - 1249, centre.1 - 218);

        assert!(
            (off_x - 8).abs() <= 1 && (off_y + 10).abs() <= 1,
            "a 1.7% high anchor displaces this rect by (8, -10), got ({off_x}, {off_y})",
        );

        // The fan is where the board drew it, so the rect has to cover it from
        // wherever the anchor put the rect. Half-extents are DIAMOND_W_REF's
        // measured ±88 x ±76 ref px, at the board's own scale.
        let (fan_w, fan_h) = ((88.0 * 1.111) as i32, (76.0 * 1.111) as i32);
        assert!(
            w / 2 - off_x.abs() >= fan_w && h / 2 - off_y.abs() >= fan_h,
            "an ({off_x}, {off_y}) px error leaves {}x{} of half-rect for a {fan_w}x{fan_h} fan",
            w / 2 - off_x.abs(),
            h / 2 - off_y.abs(),
        );

        // …and must still stop short of the point past the architect block's red
        // second line where a clipped fragment stops failing the blob filters.
        // Both offsets are taken at the scale the BOARD drew them at (1.111),
        // since that is where the ink is; only the rect moves with the anchor.
        let ink = origin.0 + ((DIAMOND_DX_REF + 101.0) * 1.111) as i32;
        let survives_to = origin.0 + ((DIAMOND_DX_REF + 101.0 + 12.0) * 1.111) as i32;
        assert!(
            x + w <= survives_to,
            "the crop reaches {}, {} px past the architect ink at {ink} and past the \
             {survives_to} where a clipped fragment starts reading as a seal",
            x + w,
            x + w - ink,
        );
    }

    /// The rect scales with the UI, both in position and in size. Fails if a
    /// constant is applied unscaled — which would put the rect in the right
    /// place on a 1374px client and nowhere near it on a 4K one.
    ///
    /// The expected numbers come from the constants' definition (`origin +
    /// offset × scale`, sized `W × scale`), not from a second call to the
    /// function, so a sign flip or a dropped `scale` cannot satisfy both sides.
    #[test]
    fn the_diamond_rect_scales_with_the_anchor() {
        let origin = (673, 682);
        let small = diamond_rect(origin, 1.0);
        let large = diamond_rect(origin, 2.0);

        assert_eq!(
            (small[2], small[3]),
            (DIAMOND_W_REF as i32, DIAMOND_H_REF as i32),
            "at scale 1 the rect IS the reference box",
        );
        assert_eq!(large[2], small[2] * 2, "the rect's width scales");
        assert_eq!(large[3], small[3] * 2, "the rect's height scales");

        for (rect, scale) in [(small, 1.0f32), (large, 2.0f32)] {
            let centre = (rect[0] + rect[2] / 2, rect[1] + rect[3] / 2);
            let want = (
                origin.0 + (DIAMOND_DX_REF * scale) as i32,
                origin.1 + (DIAMOND_DY_REF * scale) as i32,
            );
            assert!(
                (centre.0 - want.0).abs() <= 1 && (centre.1 - want.1).abs() <= 1,
                "scale {scale}: centre {centre:?} is not the origin plus the scaled offset {want:?}",
            );
        }
    }

    // ------------------------------------------------------- the arm gate --

    /// The POE-242 bug, as an invariant over the whole input space: the only
    /// iteration that captures is a focused, armed one whose cadence is due.
    ///
    /// Fails if the arm gate is dropped, or placed AFTER the cadence check, or
    /// read as "armed or due" — each of which puts `capture_screen` back on a
    /// map, which is the owner report this work item answers.
    #[test]
    fn only_a_focused_armed_iteration_on_cadence_reaches_the_capture_step() {
        for focused in [false, true] {
            for armed in [false, true] {
                for due in [false, true] {
                    assert_eq!(
                        loop_step(focused, armed, due) == LoopStep::Detect,
                        focused && armed && due,
                        "focused={focused} armed={armed} due={due}",
                    );
                }
            }
        }
    }

    /// A disarmed loop naps like an alt-tabbed one, not like a loop between
    /// ticks. Fails if the disarmed step takes the cadence quantum — the loop
    /// would then wake ten times a second for the whole of a session it is
    /// meant to be asleep for.
    #[test]
    fn a_disarmed_loop_naps_the_full_second_rather_than_the_cadence_quantum() {
        assert_eq!(loop_step(true, false, true).nap(), UNFOCUSED_NAP);
    }

    /// An armed loop that is not due yet still waits on the quantum, so the
    /// next tick lands on the cadence rather than a second late. Fails if
    /// `Quantum` and `DisarmedNap` are collapsed into one step — which is one
    /// wrong change, so the step and the wait it buys are one outcome here.
    #[test]
    fn an_armed_loop_between_ticks_waits_one_quantum() {
        assert_eq!(loop_step(true, true, false), LoopStep::Quantum);
        assert_eq!(LoopStep::Quantum.nap(), TICK);
    }

    /// `Waiting` is published on the way into the disarmed state and NOT on
    /// every nap after it. Fails if the announcement is unconditional: the loop
    /// would then take the slice mutex and clone the slice once a second for
    /// the whole of a session that has no incursion in it.
    #[test]
    fn a_disarmed_loop_announces_waiting_exactly_once() {
        assert_eq!(
            gate_announcement(None, false, TempleStatus::Idle),
            Some(TickOutcome::Disarmed),
        );

        assert_eq!(
            gate_announcement(Some(false), false, TempleStatus::Waiting),
            None,
            "and not again",
        );
    }

    /// POE-171 finding 15, as it reaches this gate: a retiring loop's
    /// `Stopping → Idle` lands after the new loop's `Waiting`. A disarmed loop
    /// publishes nothing more, so a transition-only gate would leave the page
    /// reading `idle` ("about to read") for the rest of a session that is not
    /// looking at all.
    ///
    /// Fails if the announcement is keyed on `said` alone — which is what it
    /// was before this test existed.
    #[test]
    fn a_foreign_idle_over_a_disarmed_loop_is_corrected_on_the_next_iteration() {
        assert_eq!(
            gate_announcement(Some(false), false, TempleStatus::Idle),
            Some(TickOutcome::Disarmed),
        );
    }

    /// The re-assertion must not turn a status no tick result can leave into a
    /// publish and a log line every second. `Unavailable` is that status: it
    /// means capture or OCR is missing for the life of the process, and
    /// [`next_status`] holds it against every outcome.
    ///
    /// Fails if the re-assertion is keyed on `status == Waiting` rather than on
    /// whether applying `Disarmed` would move the status.
    #[test]
    fn a_disarmed_loop_does_not_re_announce_over_an_unavailable_module() {
        assert_eq!(
            gate_announcement(Some(false), false, TempleStatus::Unavailable),
            None,
        );
    }

    /// The gate opening is announced too — otherwise the page would sit on
    /// `waiting` until the first read landed. Fails if only the disarm is
    /// announced.
    #[test]
    fn a_gate_that_opens_announces_itself() {
        assert_eq!(
            gate_announcement(Some(false), true, TempleStatus::Waiting),
            Some(TickOutcome::Armed),
        );
    }

    /// An armed loop that has read a board must not have `Idle` written over it
    /// every iteration. Fails if the armed half is re-asserted the way the
    /// disarmed half is — the page's board would be marked stale once a second
    /// for the length of a temple.
    #[test]
    fn an_armed_loop_does_not_re_announce_over_a_board_it_has_read() {
        assert_eq!(gate_announcement(Some(true), true, TempleStatus::Read), None);
    }

    /// The two lines `docs/OVERLAY-GUIDE.md` smoke item 12 tells the runner to
    /// look for. Fails if the arms are swapped — the log would then say the
    /// capture armed at the moment it stood down, which is the one thing that
    /// item is measuring.
    #[test]
    fn the_gate_line_says_which_way_the_gate_moved() {
        let mut said = None;

        assert_eq!(
            gate_line(&mut said, Some(trigger::ArmSource::Trigger(trigger::ArmReason::AlvaLine))),
            Some("Temple: capture armed by Alva — looking for the layout panel".to_string()),
        );
        assert_eq!(
            gate_line(&mut said, None),
            Some("Temple: capture stood down — waiting for Alva (Re-arm forces a read)".to_string()),
        );
    }

    /// POE-246's own line: the gate stays open while the reason changes hands,
    /// and the log says which one is holding it. Fails if the source vocabulary
    /// stops at `ArmReason` — a smoke run then cannot tell a loop kept alive by
    /// the panel on screen from one Client.txt is still arming.
    #[test]
    fn the_gate_line_names_the_panel_that_is_holding_the_gate_open() {
        let mut said = Some(trigger::ArmSource::Trigger(trigger::ArmReason::AlvaLine));

        assert_eq!(
            gate_line(&mut said, Some(trigger::ArmSource::PanelOnScreen)),
            Some(
                "Temple: capture armed by the panel on screen — looking for the layout panel"
                    .to_string()
            ),
        );
    }

    /// One line per source, not one per iteration. Fails if the rule is dropped:
    /// the loop reaches this once a second for as long as it is armed, and an
    /// unconditional line would evict every other diagnostic from the 50-entry
    /// buffer within a minute.
    #[test]
    fn the_gate_line_is_said_once_per_source() {
        let mut said = None;
        let source = Some(trigger::ArmSource::PanelOnScreen);

        assert!(gate_line(&mut said, source).is_some());

        assert_eq!(gate_line(&mut said, source), None, "and not again");
    }

    /// The status the arm gate publishes, and the one the plan names: "on,
    /// waiting for Alva". Fails if the disarmed loop keeps publishing `idle`,
    /// which reads as "running and about to read" — the exact wrong answer to
    /// "why is nothing happening?".
    #[test]
    fn a_disarmed_gate_publishes_waiting() {
        let mut slice = TempleSlice::default();

        apply_status(&mut slice, TickOutcome::Disarmed);

        assert_eq!(slice.status, TempleStatus::Waiting);
    }

    /// POE-244's core fix. The panel leaving the screen is what an INCURSION
    /// looks like — the player stepped through the door and the layout panel
    /// closed behind them — and `PanelNotVisible` is reached only through
    /// `miss`'s retire. Dropping the advice there left the door widget with no
    /// purple seal, no `open <edge>` line and no architect name at exactly the
    /// point they are the only things still on screen.
    ///
    /// **What this pins is [`apply_status`], not [`miss`]'s publish closure.**
    /// That closure takes an `AppHandle` and the slice mutex, so it has no unit
    /// seam here; what it does now is call this function and nothing else, and
    /// this is the assertion that the function it calls leaves the advice
    /// alone. A future edit that put a `slice.advice = None` back inside the
    /// closure would pass this test — the guard against that is the reviewed
    /// diff and the incursion smoke item in `docs/OVERLAY-GUIDE.md`, which is
    /// where the original defect was found.
    #[test]
    fn a_panel_that_left_the_screen_keeps_the_advice_it_was_read_with() {
        let mut slice = TempleSlice {
            status: TempleStatus::Read,
            advice: Some(slice::AdviceView {
                recommendations: Vec::new(),
                gambles: Vec::new(),
                secondary_door: None,
                map_action: "continue".to_string(),
                warnings: Vec::new(),
                forced_kill: false,
            }),
            mode: Some("chase".to_string()),
            ..TempleSlice::default()
        };

        apply_status(&mut slice, TickOutcome::NoPanel);

        assert_eq!(slice.status, TempleStatus::PanelNotVisible);
        assert!(slice.advice.is_some(), "the door widget has nothing to draw without it");
        assert_eq!(slice.mode.as_deref(), Some("chase"));
    }

    /// …and the stand-down does not end it either, since POE-248.
    ///
    /// The measured failure: `12:32:10 capture armed by the panel on screen` …
    /// `12:39:05 capture stood down`, and the door diamond went with it while
    /// the player was still in the room it described. A gate says whether
    /// anything is LOOKING; the incursion is not over because the module
    /// stopped looking. Fails if the POE-244 drop is put back.
    ///
    /// The two lines that DO end it are `trigger::advice_end`'s, tested there.
    #[test]
    fn standing_the_loop_down_leaves_the_room_widget_its_advice() {
        let mut slice = TempleSlice {
            status: TempleStatus::PanelNotVisible,
            advice: Some(slice::AdviceView {
                recommendations: Vec::new(),
                gambles: Vec::new(),
                secondary_door: None,
                map_action: "continue".to_string(),
                warnings: Vec::new(),
                forced_kill: false,
            }),
            mode: Some("chase".to_string()),
            ..TempleSlice::default()
        };

        apply_gate(&mut slice, TickOutcome::Disarmed);

        assert_eq!(slice.status, TempleStatus::Waiting);
        assert!(
            slice.advice.is_some(),
            "the room widget lives with the incursion, not with the capture",
        );
        assert_eq!(slice.mode.as_deref(), Some("chase"));
    }

    /// Arming does NOT clear it: the next read replaces the whole slice, and
    /// blanking here would empty the page for the seconds between Alva's line
    /// and the first anchor.
    #[test]
    fn arming_leaves_the_standing_advice_for_the_next_read_to_replace() {
        let mut slice = TempleSlice {
            status: TempleStatus::Waiting,
            // The advice itself, not just the mode label beside it: a fixture
            // that left this `None` would assert nothing about the field the
            // test is named for, and would pass against a version that cleared
            // it (review, POE-244).
            advice: Some(slice::AdviceView {
                recommendations: Vec::new(),
                gambles: Vec::new(),
                secondary_door: None,
                map_action: "continue".to_string(),
                warnings: Vec::new(),
                forced_kill: false,
            }),
            mode: Some("chase".to_string()),
            ..TempleSlice::default()
        };

        apply_gate(&mut slice, TickOutcome::Armed);

        assert_eq!(slice.status, TempleStatus::Idle);
        assert!(slice.advice.is_some(), "arming must not blank the standing board");
        assert_eq!(slice.mode.as_deref(), Some("chase"));
    }

    /// Arming returns the module to `Idle` BEFORE the first read, so a board
    /// read during the previous incursion is not still presented as current
    /// while the loop looks for a new one. Fails if `Armed` leaves `Waiting`
    /// standing, or lands on `Reading` (which would claim a read in flight).
    #[test]
    fn arming_returns_the_module_to_idle_before_the_first_read() {
        let mut slice = TempleSlice {
            status: TempleStatus::Waiting,
            ..TempleSlice::default()
        };

        apply_status(&mut slice, TickOutcome::Armed);

        assert_eq!(slice.status, TempleStatus::Idle);
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
            TickOutcome::Disarmed,
            TickOutcome::Armed,
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
    /// Since POE-230 none of the three functions takes a capture size at all, so
    /// "computed from the frame" is no longer something a test can catch — the
    /// signature does. What is left to pin is the size itself: each rect must be
    /// its constants' box times the scale, and the expected numbers here are
    /// built from those constants rather than read back off the function, so a
    /// rect that stopped scaling cannot satisfy both halves.
    #[test]
    fn all_three_rois_stay_a_fixed_size_in_reference_px() {
        // 4K at the same UI scale ratio the reference board was measured at.
        let uhd_scale = 3840.0 / 1374.0;
        let [left, top, right, bottom] = PANEL_BOX_REF;
        let panel_ref = (
            (right - left + PANEL_MARGIN_REF + PANEL_RIGHT_MARGIN_REF) as i32,
            (bottom - top + 2.0 * PANEL_MARGIN_REF) as i32,
        );

        let [_, _, pw, ph] = panel_rect((673, 682), 1.0);
        let [_, _, uw, uh] = panel_rect((1880, 1900), uhd_scale);
        assert_eq!(
            (pw, ph),
            panel_ref,
            "the reference-scale panel ROI is the measured border box plus its margin",
        );
        assert!(
            (uw as f32 / pw as f32 - uhd_scale).abs() < 0.01
                && (uh as f32 / ph as f32 - uhd_scale).abs() < 0.01,
            "the panel ROI must scale with the anchor: {uw}×{uh}",
        );
        // The biggest of the three, and the one that bounds the OCR buffer:
        // 1520×1268 = 1.93 Mpx at this scale, 7.1% under the quarter-frame bound.
        assert_bounded("the panel", uw, uh);

        let [_, _, dw, dh] = diamond_rect((673, 682), 1.0);
        let [_, _, udw, udh] = diamond_rect((1880, 1900), uhd_scale);
        assert_eq!(
            (dw, dh),
            (DIAMOND_W_REF as i32, DIAMOND_H_REF as i32),
            "the reference-scale diamond ROI is the measured box",
        );
        assert!(
            (udw as f32 / dw as f32 - uhd_scale).abs() < 0.01
                && (udh as f32 / dh as f32 - uhd_scale).abs() < 0.01,
            "the diamond ROI must scale with the anchor: {udw}×{udh}",
        );
        assert_bounded("the diamond", udw, udh);

        let [_, _, rw, rh] = remaining_rect((673, 682), 1.0);
        let [_, _, urw, urh] = remaining_rect((1880, 1900), uhd_scale);
        assert_eq!((rw, rh), (300, 46), "the reference-scale budget ROI is the measured box");
        assert!(
            (urw as f32 / rw as f32 - uhd_scale).abs() < 0.01
                && (urh as f32 / rh as f32 - uhd_scale).abs() < 0.02,
            "the budget ROI must scale with the anchor: {urw}×{urh}",
        );
        assert_bounded("the budget line", urw, urh);
    }

    /// A 4K-anchored ROI must still be a small fraction of a 4K frame — the
    /// module note's reason for having ROIs at all, since
    /// `preprocess_for_ocr` upscales 2x and a full frame would be 33 Mpx a tick.
    ///
    /// A quarter-frame is the bound the retired assertion used and the panel is
    /// the one that approaches it: 1520×1268 = 1.93 Mpx of 2.07, i.e. 7.1% under
    /// at the 3840/1374 scale.
    fn assert_bounded(what: &str, w: i32, h: i32) {
        assert!(
            (w as u64 * h as u64) < (3840 * 2160) / 4,
            "{what} ROI must stay a small fraction of a 4K frame, got {w}×{h}",
        );
    }

    /// The panel ROI covers the panel's border box on all three captures whose
    /// origin and scale are recorded, with the margin [`PANEL_MARGIN_REF`]
    /// claims. Reproduces [`PANEL_BOX_REF`]'s table.
    ///
    /// This is the POE-230 regression test. The first row is the frame the bug
    /// was measured on: its panel starts at x 1171 where the retired right-edge
    /// crop started at 1380, so a region keyed on anything but the origin fails
    /// here by 209 px.
    ///
    /// The 1539 capture appears twice on purpose. At **1.111**, the scale its own
    /// border implies, it is a third measurement of [`PANEL_BOX_REF`]. At
    /// **1.13**, the scale its anchor recorded, it is the margin's worst case —
    /// the crop is computed from a scale 1.7% off the one the panel was drawn at,
    /// and must still cover it. A trimmed margin fails the second row only.
    #[test]
    fn the_panel_roi_contains_every_measured_panel() {
        // (origin, scale, measured panel border box `[left, top, right, bottom]`)
        let boards = [
            ((960i32, 713i32), 1.0000f32, [1171, 44, 1655, 418]),
            ((673, 682), 1.0000, [884, 13, 1368, 387]),
            ((745, 768), 1.111, [980, 24, 1517, 440]),
            ((745, 768), 1.13, [980, 24, 1517, 440]),
        ];
        for (origin, scale, [l, t, r, b]) in boards {
            let [x, y, w, h] = panel_rect(origin, scale);
            assert!(
                x <= l && y <= t && x + w >= r && y + h >= b,
                "{origin:?} @ {scale}: ROI {:?} does not contain the panel [{l}, {t}, {r}, {b}]",
                [x, y, w, h],
            );
        }
    }

    /// The budget ROI covers the `N Incursions Remaining` glyph box on both
    /// measured boards. Keyed on the Entrance centre, which since POE-230 is
    /// what all three regions are keyed on.
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

        let (clipped, origin) =
            crop_clipped(&img, [-20, -10, 60, 40]).expect("the overlap is readable");
        assert_eq!((clipped.width(), clipped.height()), (40, 30));
        assert_eq!(
            origin,
            (0, 0),
            "the corner the crop was taken from is the CLIPPED one — a box placed \
             against the rect's own -20 would sit off screen",
        );

        assert!(crop_clipped(&img, [100, 0, 40, 40]).is_none(), "no overlap, no crop");
        assert!(crop_clipped(&img, [0, -50, 40, 40]).is_none(), "no overlap, no crop");
    }

    // ------------------------------- the shared screen scale, read and written --

    use crate::ssot::{ScreenScaleSource, ScreenSlice};

    /// A remembered measurement of one screen, as the slice carries it.
    fn remembered(
        source: ScreenScaleSource,
        width: u32,
        height: u32,
        ui_scale: f32,
        monitor_id: u32,
    ) -> ScreenSlice {
        ScreenSlice {
            width,
            height,
            ui_scale,
            source,
            measured_at_ms: 1_700_000_000_000,
            verified_this_session: crate::ssot::verifies_the_screen(source),
            monitor_id,
            origin: (0, 0),
        }
    }

    /// The one capture size where BOTH units have been measured, so the
    /// conversion can be checked against something other than itself: a
    /// 1920x1080 screen measures `ui_scale` 1080/1200 = 0.90 by the shared
    /// unit's definition, and `anchor::MEASURED_SCALES` says the temple anchors
    /// that capture at 1.000.
    ///
    /// The expected hint is therefore the TABLE's number — not `0.90 * k`,
    /// which is the arithmetic under test. The tolerance is `k`'s own stated
    /// accuracy, ~1%, which is one `anchor::SCALE_STEP` at this scale.
    ///
    /// Fails if the conversion is inverted (0.90 / 1.1111 = 0.81, a 19% miss) or
    /// if `k` is edited without a measurement behind it.
    #[test]
    fn a_remembered_scale_for_this_screen_becomes_the_hint_the_table_measured() {
        let screen = remembered(ScreenScaleSource::MercFrame, 1920, 1080, 0.90, 7);

        let hint = hint_for_capture(Some(&screen), (1920, 1080), 7).expect("this screen has one");

        let measured = anchor::table_scale(1920, 1080).expect("1920x1080 is the measured row");
        assert!(
            (hint.scale - measured).abs() <= measured * 0.01,
            "the shared unit's 0.90 on this screen must convert to the {measured} the temple \
             measured there, not {}",
            hint.scale,
        );
        assert_eq!((hint.screen_w, hint.screen_h), (1920, 1080));
    }

    /// A scale measured on ANOTHER display is not a hint, even at the same
    /// resolution (POE-237). Fails if the temple compares dimensions alone,
    /// which is what let a scale survive onto a second 1920x1080 monitor.
    #[test]
    fn a_scale_measured_on_another_display_is_not_a_hint() {
        let screen = remembered(ScreenScaleSource::MercFrame, 1920, 1080, 0.90, 7);

        assert_eq!(hint_for_capture(Some(&screen), (1920, 1080), 9), None);
    }

    /// `monitor_id == 0` is UNKNOWN, not an identity: a slice persisted before
    /// POE-237 and a capture whose handle truncated to zero both carry it, and
    /// comparing it as a real id would refuse every remembered scale on the
    /// first capture after an upgrade. The dimensions decide instead.
    ///
    /// Fails if either side's unknown is read as "a display that differs".
    #[test]
    fn an_unknown_display_id_is_no_opinion_and_the_dimensions_decide() {
        let no_id = remembered(ScreenScaleSource::Remembered, 1920, 1080, 0.90, 0);
        let known = remembered(ScreenScaleSource::Remembered, 1920, 1080, 0.90, 7);

        assert!(
            hint_for_capture(Some(&no_id), (1920, 1080), 7).is_some(),
            "a pre-POE-237 stored scale still hints on a capture of its size",
        );
        assert!(
            hint_for_capture(Some(&known), (1920, 1080), 0).is_some(),
            "a capture with no display id still gets the hint its size earns",
        );
        assert_eq!(
            hint_for_capture(Some(&no_id), (2560, 1440), 7),
            None,
            "…and an unknown id does not excuse a different resolution",
        );
    }

    /// A measurement of a different resolution is not a hint. The capture loop
    /// reaches this only through `temple_debug_capture`'s image-file path —
    /// `ssot::drop_if_mismatched` empties the slot first on a live tick — and
    /// the answer has to be the same either way.
    #[test]
    fn a_scale_measured_at_another_resolution_is_not_a_hint() {
        let screen = remembered(ScreenScaleSource::MercFrame, 2560, 1440, 1.20, 7);

        assert_eq!(hint_for_capture(Some(&screen), (1920, 1080), 7), None);
    }

    /// The whole Recalibrate path, through the decisions the tick actually
    /// makes: the slice is empty, so there is no hint AND no remembered plate,
    /// so the cheap detect cannot verify, so the gate sweeps at once — and the
    /// anchor that sweep produces lands in the empty slot and is written back.
    ///
    /// The counterfactual is half the test and is what fails without
    /// [`cheap_hint_for`]: with the plate kept, the same capture re-anchors at
    /// the pre-Recalibrate scale, [`sweep_could_help`] answers false, the gate is
    /// never asked, and `publish_anchor_scale` puts the forgotten number back —
    /// with the layout panel on screen, which is when a user presses the button.
    ///
    /// Run on the real capture and through `anchor::detect_cheap`, not on a
    /// hand-called `allow`: the property is a composition of four rules and
    /// asserting the last one in isolation would pass with the first three
    /// broken.
    #[test]
    fn recalibrate_leaves_the_temple_sweeping_and_republishing() {
        let img = live_capture();
        let capture = (img.width(), img.height());
        let key = SweepKey { monitor_id: 7, width: capture.0, height: capture.1 };
        // The state the button is pressed in: a panel on screen, anchored, and
        // the session holding the plate it last saw.
        let held = anchor::CheapHint {
            calibration: anchor::AnchorCalibration {
                screen_w: capture.0,
                screen_h: capture.1,
                scale: LIVE_CAPTURE_SCALE,
            },
            origin: LIVE_CAPTURE_ORIGIN,
        };
        assert!(
            matches!(
                anchor::detect_cheap(&img, Some(&held)),
                anchor::CheapDetect::Anchored(_)
            ),
            "the pre-press state has to be a verifying tick, or this proves nothing",
        );

        // The press: `ssot::geometry_recalibrate` empties the slice. Everything
        // below is what the next tick then decides.
        let emptied: Option<&crate::ssot::ScreenSlice> = None;
        let hint = hint_for_capture(emptied, capture, 7);
        assert_eq!(hint, None, "an empty slice hints nothing");

        // `true`: the slice HAD answered for this screen — that is what the
        // press emptied, and what tells the loop this is a decision rather than
        // a screen nothing has measured yet.
        let plate = cheap_hint_for(hint, Some(held), true);
        assert_eq!(plate, None, "…and the remembered plate goes with it");

        let cheap = anchor::detect_cheap(&img, plate.as_ref());
        assert!(
            sweep_could_help(&cheap),
            "with nothing to re-match, the tick cannot verify: got {cheap:?}",
        );

        let mut gate = SweepGate::default();
        assert!(!gate.allow(key, true), "a calibrated screen owes a whole cadence");
        assert!(
            gate.allow(key, hint.is_some()),
            "losing the shared scale must sweep on the very next tick",
        );

        // And what that sweep finds is published and written back.
        let mut slot = None;
        let swept =
            screen_from_anchor(LIVE_CAPTURE_SCALE, hint, capture, 7, (0, 0), 1_700_000_000_002)
                .expect("with the slice emptied, the capture height corroborates the anchor");
        let record = crate::ssot::record_screen(&mut slot, swept);
        assert!(record.accepted && record.changed, "the re-measurement must land and wake the app");
        assert!(
            crate::ssot::should_remember_screen(record.changed, swept.source),
            "…and be written back, or the next launch starts blind again",
        );
    }

    /// A merc frame fit landing in the slice CORRECTS a session that anchored on
    /// a worse seed, on the next tick, by dropping the plate it was re-verifying.
    ///
    /// Without this the drifting `MercOcr` cue would lock a session: the temple
    /// would re-match its own copy of that scale every tick, never fail
    /// verification, never sweep, and never notice the gold frame's better answer
    /// arriving beside it. One [`anchor::SCALE_STEP`] is the threshold, so the
    /// per-frame wobble of a plate that has not moved keeps its hint.
    #[test]
    fn a_hint_that_moved_more_than_one_step_drops_the_remembered_plate() {
        let held = anchor::CheapHint {
            calibration: anchor::AnchorCalibration {
                screen_w: 1920,
                screen_h: 1080,
                scale: 1.00,
            },
            origin: (960, 713),
        };
        let at = |scale| anchor::AnchorCalibration { screen_w: 1920, screen_h: 1080, scale };

        assert_eq!(
            cheap_hint_for(Some(at(1.00)), Some(held), true),
            Some(held),
            "a slice that agrees leaves the session where it is",
        );
        assert_eq!(
            cheap_hint_for(Some(at(1.00 + anchor::SCALE_STEP)), Some(held), true),
            Some(held),
            "and so does one exactly a step away — that is the resolution of the grid",
        );
        assert_eq!(
            cheap_hint_for(Some(at(1.00 + 2.0 * anchor::SCALE_STEP)), Some(held), true),
            None,
            "two steps is the slice describing a screen this plate is not on",
        );
    }

    /// An empty slice takes the plate only when it was EMPTIED. A slice that has
    /// never answered is a screen nothing has measured — including the machine
    /// whose slider [`screen_from_anchor`] withholds a publish for — and the
    /// plate is the only thing on it that knows the scale.
    ///
    /// Fails if the two empty slices are collapsed into one rule, which costs
    /// that machine its hinted cheap tick for the whole session: every tick would
    /// fall to a nominating seed that is wrong there, and the board would be read
    /// once per sweep cadence instead of once a second.
    ///
    /// The third way into the `false` arm — moving the game to a screen nothing
    /// has measured — has no pure seam and is not asserted here: `answered` is
    /// reset in `tick`, on `ssot::drop_if_mismatched`'s return value, and both
    /// sides of that need an `AppHandle`. What IS pinned is the rule the reset
    /// feeds, which is this function.
    #[test]
    fn an_empty_slice_takes_the_plate_only_if_it_had_answered_before() {
        let held = anchor::CheapHint {
            calibration: anchor::AnchorCalibration {
                screen_w: 1920,
                screen_h: 1080,
                scale: 1.00,
            },
            origin: (960, 713),
        };

        assert_eq!(
            cheap_hint_for(None, Some(held), true),
            None,
            "a slice that answered and is now empty was emptied on purpose",
        );
        assert_eq!(
            cheap_hint_for(None, Some(held), false),
            Some(held),
            "a slice that never answered has no claim to overrule the plate with",
        );
    }

    /// A `ui_scale` that cannot describe a screen produces no hint rather than a
    /// zero-size template. `settings::ScreenScaleSetting::is_sane` refuses these
    /// at load and both writers measure rather than invent, so this is the
    /// conversion being total — one comparison against a template the anchor
    /// would have to reject downstream.
    #[test]
    fn a_scale_that_cannot_describe_a_screen_is_not_a_hint() {
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let screen = remembered(ScreenScaleSource::MercFrame, 1920, 1080, bad, 7);

            assert_eq!(
                hint_for_capture(Some(&screen), (1920, 1080), 7),
                None,
                "ui_scale {bad} is not a screen",
            );
        }
    }

    /// The one full-screen capture the module has: a 1920x1080 laptop frame with
    /// the layout panel open, which anchors at [`LIVE_CAPTURE_SCALE`] and
    /// [`LIVE_CAPTURE_ORIGIN`] (laptop dump `temple-debug/1788438639673`,
    /// 2026-09-03, NCC 0.99999).
    ///
    /// The board fixtures are panel CROPS and cannot stand in: they carry no
    /// diamond and only the lower part of the panel, and the rules under test
    /// here read the capture's own size or the panel's own pixels.
    fn live_capture() -> DynamicImage {
        let path = format!(
            "{}/tests/fixtures/temple/screen-live-1920x1080.png",
            env!("CARGO_MANIFEST_DIR")
        );
        image::open(&path).unwrap_or_else(|e| panic!("{path} loads: {e}"))
    }

    /// See [`live_capture`].
    const LIVE_CAPTURE_SCALE: f32 = 1.00;
    /// See [`live_capture`].
    const LIVE_CAPTURE_ORIGIN: (i32, i32) = (960, 713);

    // ------------------------------------- the ROIs on the live capture --

    /// The layout the committed frame reads at its recorded anchor.
    ///
    /// Built from [`reader::read_layout_at`] with the measurement rather than
    /// through a sweep: the anchor is what `anchor.rs` tests, and paying 5 s of
    /// pyramid sweep here would test it twice and make these ROI assertions
    /// depend on it.
    fn live_layout(img: &DynamicImage) -> TempleLayout {
        reader::read_layout_at(
            img,
            anchor::Anchor {
                origin: LIVE_CAPTURE_ORIGIN,
                scale: LIVE_CAPTURE_SCALE,
                ncc: 0.99999,
            },
        )
    }

    /// Every block of text the panel read needs, as hand-measured glyph extents
    /// on the live capture (2026-09-03) — and everything the retired screen-edge
    /// constant claimed its region held.
    const PANEL_TEXT_BOXES: [(&str, [u32; 4]); 4] = [
        ("the title", [1222, 70, 1541, 112]),
        ("the Hayoxi block", [1480, 115, 1641, 167]),
        ("the Xopec block", [1189, 289, 1347, 327]),
        ("the Enter Incursion button", [1314, 353, 1514, 388]),
    ];

    /// Glyph pixels in a box on the live capture — text against the panel's own
    /// dark ground.
    ///
    /// Measured over the four text boxes below and over two 101x51 empty patches
    /// of the same panel: the text boxes hold 858–1635 pixels above this
    /// threshold and the empty patches hold **none**. That separation is what
    /// makes this usable as "there is text here" without OCR — Tesseract is
    /// Windows-only, so no assertion here may depend on it.
    fn glyph_pixels(rgb: &image::RgbImage, [x0, y0, x1, y1]: [u32; 4]) -> usize {
        let mut n = 0;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let [r, g, b] = rgb.get_pixel(x, y).0;
                let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
                if lum > 90.0 {
                    n += 1;
                }
            }
        }
        n
    }

    /// **The POE-230 bug, on the frame it was measured on.** The derived panel
    /// ROI takes the whole panel and BOTH architect blocks; the retired
    /// right-edge crop, `[1380, 0, 540, 430]` on this capture, took neither the
    /// lower-left block nor the left half of the title.
    ///
    /// The four boxes are hand-measured glyph extents on the fixture
    /// (2026-09-03), and they are everything the retired constant claimed its
    /// region held — title, both architect blocks, the button:
    ///
    /// | box | glyph extent |
    /// |---|---|
    /// | `Lightning Workshop` | x 1222–1541, y 70–112 |
    /// | `Hayoxi, Architect of / Destruction / (Kill to upgrade to Omnitect / Reactor Plant)` | x 1480–1641, y 115–167 |
    /// | `Xopec, Architect of Power / (Kill to change to Explosives / Room)` | x 1189–1347, y 289–327 |
    /// | the `Enter Incursion` button, frame included | x 1314–1514, y 353–388 |
    ///
    /// Each box is checked to HOLD text before it is checked to be inside the
    /// crop, so a mis-measured box cannot pass by being empty. The panel's own
    /// border box is not repeated here — it is the first row of
    /// [`the_panel_roi_contains_every_measured_panel`].
    #[test]
    fn the_live_captures_panel_roi_takes_every_readable_block_of_the_panel() {
        let img = live_capture();
        let rgb = img.to_rgb8();
        let [x, y, w, h] = panel_rect(LIVE_CAPTURE_ORIGIN, LIVE_CAPTURE_SCALE);

        for (what, [bx0, by0, bx1, by1]) in PANEL_TEXT_BOXES {
            assert!(
                glyph_pixels(&rgb, [bx0, by0, bx1, by1]) > 700,
                "{what} {:?} holds no text — the measurement is wrong, not the rect",
                [bx0, by0, bx1, by1],
            );
            assert!(
                x <= bx0 as i32 && y <= by0 as i32 && x + w >= bx1 as i32 && y + h >= by1 as i32,
                "the panel ROI {:?} does not contain {what} {:?}",
                [x, y, w, h],
                [bx0, by0, bx1, by1],
            );
        }

        // Fully inside the capture, so the crop is the rect and `crop_clipped`
        // loses nothing. A windowed client is where that stops holding.
        let (crop, origin) = crop_clipped(&img, [x, y, w, h]).expect("the ROI overlaps the capture");
        assert_eq!(origin, (x, y), "nothing was clipped, so the corner is the rect's own");
        assert_eq!(
            (crop.width(), crop.height()),
            (w as u32, h as u32),
            "the ROI hangs off the 1920x1080 frame it was measured on",
        );
    }

    /// The margins survive the worst anchor error on record, which is what they
    /// are sized for: at 0.96 — POE-247's hint chain answering 0.96 where the
    /// peak is 1.00 — every block of panel text is still inside the crop.
    ///
    /// This is the assertion [`PANEL_RIGHT_MARGIN_REF`] is really making. The
    /// right margin is the tight one (20 ref px against the other three sides'
    /// 40) because everything past the panel border there is the map's own info
    /// block, and the floor under it is the Hayoxi block at +681: at 0.96 the
    /// crop's right edge lands at x 1646 against that block's 1641. A right
    /// margin of 16 would leave 2 px there and 8 would cut the text outright.
    #[test]
    fn the_panel_roi_survives_the_worst_recorded_anchor_error() {
        let img = live_capture();
        let rgb = img.to_rgb8();
        let [x, y, w, h] = panel_rect(LIVE_CAPTURE_ORIGIN, 0.96);

        for (what, [bx0, by0, bx1, by1]) in PANEL_TEXT_BOXES {
            assert!(
                glyph_pixels(&rgb, [bx0, by0, bx1, by1]) > 700,
                "{what} {:?} holds no text — the measurement is wrong, not the rect",
                [bx0, by0, bx1, by1],
            );
            assert!(
                x <= bx0 as i32 && y <= by0 as i32 && x + w >= bx1 as i32 && y + h >= by1 as i32,
                "at a 4% low anchor the panel ROI {:?} loses {what} {:?}",
                [x, y, w, h],
                [bx0, by0, bx1, by1],
            );
        }
    }

    /// The derived diamond ROI settles every corridor of the room the player is
    /// standing in, on the frame where the retired rect read five seals for a
    /// six-neighbour room and fell back to the beam read.
    ///
    /// The board: Lightning Workshop at **C1**, six neighbours, six seals, one
    /// of them green — the Omnitect Reactor Plant corridor at **C2**. So the
    /// settled set must contain `C1-C2` and none of C1's other five corridors,
    /// which is also the assertion that the fan is not merely counted but
    /// mapped: a rect off far enough to rotate the fan puts the green seal on a
    /// different neighbour.
    ///
    /// It is load-bearing over the fallback: the beam reader flags all six of
    /// C1's corridors uncertain on this board, so `doors − uncertain` — what the
    /// module publishes when this read fails — drops `C1-C2` and reports the one
    /// open door as closed.
    #[test]
    fn the_live_captures_diamond_roi_settles_every_corridor_of_the_current_room() {
        let img = live_capture();
        let layout = live_layout(&img);
        assert_eq!(layout.current, Some(lattice::Slot::C1), "the fixture's board");

        let rect = diamond_rect(layout.origin, layout.scale);
        let read = markers::read_door_markers(&img, rect, 6)
            .expect("six seals for the six-neighbour room the fixture stands in");
        assert_eq!(
            read.markers.iter().filter(|m| m.open).count(),
            1,
            "one green seal on this board",
        );

        let settled = read_markers(&img, &layout).expect("the seals settle C1's corridors");
        let incident: Vec<String> = settled
            .iter()
            .filter(|e| e.ends().0 == lattice::Slot::C1 || e.ends().1 == lattice::Slot::C1)
            .map(|e| e.to_string())
            .collect();
        assert_eq!(incident, vec!["C1-C2".to_string()], "C1's only open corridor");

        assert!(
            layout.uncertain.contains(&lattice::Edge::new(
                lattice::Slot::C1,
                lattice::Slot::C2
            )),
            "the beam read leaves C1-C2 uncertain here, which is what makes the seal read \
             the difference between an open door and a closed one",
        );
    }

    /// A wrong anchor moves the diamond rect and nothing else, so the +1.7%
    /// error the 1539 board records still settles the current room's corridors.
    ///
    /// The scale is applied to the RECT derivation, not to the image: the seals
    /// stay where the game drew them and the crop arrives at the wrong place and
    /// size, which is exactly what a mis-anchored read does. The beam data is
    /// the true read's, because that is what [`markers::apply_markers`]
    /// cross-checks against and it does not move with the anchor either.
    #[test]
    fn the_recorded_high_anchor_error_still_settles_the_current_rooms_corridors() {
        let img = live_capture();
        let layout = TempleLayout { scale: 1.017, ..live_layout(&img) };

        let settled = read_markers(&img, &layout).expect("a 1.7% high anchor still reads");
        let incident: Vec<String> = settled
            .iter()
            .filter(|e| e.ends().0 == lattice::Slot::C1 || e.ends().1 == lattice::Slot::C1)
            .map(|e| e.to_string())
            .collect();

        assert_eq!(incident, vec!["C1-C2".to_string()], "C1's only open corridor");
    }

    /// **POE-247's low anchor is not a rect-size problem.** At 0.96 the fan is
    /// rotated 22.7° about the rect's centre, past
    /// [`markers::MAX_RESIDUAL_DEG`]'s 22°, and the read fails.
    ///
    /// Which is the behaviour worth pinning, in both halves. It fails rather
    /// than naming the wrong neighbour — the property the whole fallback rests
    /// on. And it fails at every width: the rect's centre is
    /// `origin + DIAMOND_DX_REF × scale`, which the width does not enter, so
    /// [`DIAMOND_W_REF`]'s table cannot buy this back and the anchor is where it
    /// has to be fixed. A future change that "fixes" this by widening the rect
    /// or by raising the angular gate makes this test pass for the wrong reason;
    /// one that fixes the anchor makes it unreachable, which is the point.
    #[test]
    fn the_recorded_low_anchor_error_fails_the_diamond_read_rather_than_mapping_it_wrong() {
        let img = live_capture();
        let layout = TempleLayout { scale: 0.96, ..live_layout(&img) };

        let err = read_markers(&img, &layout).expect_err("a 4% low anchor rotates the fan past 22°");
        assert!(
            err.contains("from every corridor direction"),
            "the fan is rotated, so the failure must be the angular gate, got {err:?}",
        );
    }

    /// The line POE-230 added to the full read names all three regions, in the
    /// order the read uses them, with the numbers this capture produces.
    ///
    /// Pinned as a literal because its job is to be pasted back from `app.log`
    /// by a user whose read fell back: a line that printed the same rect twice,
    /// or dropped one, would still look plausible.
    #[test]
    fn the_roi_line_names_all_three_rects_of_the_live_capture() {
        assert_eq!(
            rois_line(&mut None, LIVE_CAPTURE_ORIGIN, LIVE_CAPTURE_SCALE).as_deref(),
            Some(
                "Temple: rois panel [1131, 4, 544, 454] diamond [1313, 117, 200, 200] \
                 remaining [810, 771, 300, 46]"
            ),
        );
    }

    /// One line per distinct geometry, not one per read.
    ///
    /// `app_log` keeps 50 entries and `full_read` can run at 1 Hz for as long as
    /// the panel is on screen, so a line said unconditionally here evicts every
    /// other diagnostic in the buffer inside a minute — the failure this rule
    /// exists to prevent. The third call is what stops the rule from being
    /// "say it once ever": a board read at a new scale is new geometry and has
    /// to be reported.
    #[test]
    fn the_roi_line_is_said_once_per_distinct_geometry() {
        let mut said = None;

        assert!(
            rois_line(&mut said, LIVE_CAPTURE_ORIGIN, LIVE_CAPTURE_SCALE).is_some(),
            "the first read of a geometry says it",
        );
        assert_eq!(
            rois_line(&mut said, LIVE_CAPTURE_ORIGIN, LIVE_CAPTURE_SCALE),
            None,
            "the same rects a second later are not news",
        );
        assert!(
            rois_line(&mut said, LIVE_CAPTURE_ORIGIN, 1.13).is_some(),
            "a re-anchor at another scale moves every rect and must be reported",
        );
    }

    /// A text ROI with no pixels in the frame is REPORTED, not stepped over in
    /// silence.
    ///
    /// The silence POE-230 left behind: `crop_clipped` answers `None` for an
    /// empty intersection, `panel_text` `continue`s, and the read publishes a
    /// board whose offer list is empty for a reason nothing states. The rect is
    /// the live capture's own panel crop (`[1131, 4, 544, 454]`) moved out to the
    /// frame's right edge: its first column is x = 1920 on a 1920-wide capture,
    /// so the intersection is empty by one pixel and nothing is read at all.
    #[test]
    fn a_text_roi_entirely_outside_the_capture_is_named_with_its_rect() {
        let outside = clipped_text_rois(&[("panel", [1920, 4, 544, 454])], 1920, 1080);

        assert_eq!(outside, vec![("panel", [1920, 4, 544, 454])]);
        assert_eq!(
            clipped_roi_line(outside[0].0, outside[0].1),
            "Temple: panel ROI [1920, 4, 544, 454] is outside the capture — windowed client?",
        );
    }

    /// A rect that is merely CLIPPED reads, so it says nothing.
    ///
    /// The boundary, and the reason this is not "does the rect fit": clipping is
    /// what `crop_clipped` is FOR, and a line on every read whose panel touches
    /// the frame edge would be the buffer-eviction problem `rois_line`'s
    /// once-per-value rule exists to avoid. One column of pixels inside the
    /// frame is a read.
    #[test]
    fn a_text_roi_clipped_at_the_edge_is_still_a_read_and_says_nothing() {
        let regions = [
            // Its last column is x = 1919: inside.
            ("panel", [1919, 4, 544, 454]),
            // Hanging off the LEFT and the TOP, one row and one column in.
            ("remaining", [-299, -45, 300, 46]),
        ];

        assert!(clipped_text_rois(&regions, 1920, 1080).is_empty());
    }

    /// Both regions outside means both are named — the loop is over the
    /// regions, not a first-hit answer.
    ///
    /// Fails if the check short-circuits on the panel: the budget line is the
    /// one whose loss POE-230 called "a warning, not the board", and a report
    /// that only ever names the panel would leave that loss exactly as silent
    /// as it was before.
    #[test]
    fn every_outside_region_is_named_rather_than_the_first() {
        let regions = [("panel", [4000, 4, 544, 454]), ("remaining", [810, 2000, 300, 46])];

        let outside = clipped_text_rois(&regions, 1920, 1080);

        assert_eq!(
            outside.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            vec!["panel", "remaining"],
        );
    }

    /// The notice is announced once per OUTSIDE-SET, and a rect that moves while
    /// staying outside is not a new announcement.
    ///
    /// The case this is keyed for: a windowed client being DRAGGED. The panel
    /// rect is a function of `(origin, scale)`, which moves with the window, so
    /// every tick of the drag produces a different message about the same fact.
    /// Keying on the message — which is what `ErrorLog` does — would say it at
    /// 1 Hz for the length of the drag AND spend the session-wide
    /// `MAX_DISTINCT_ERRORS` budget doing it, so every later temple error would
    /// be dropped from the log by a mouse gesture.
    ///
    /// Fails if the memory keys on the rect or on the line.
    #[test]
    fn the_clipped_notice_is_said_once_while_the_same_regions_stay_outside() {
        let mut said = None;

        let first = clipped_roi_announcement(&mut said, &[("panel", [1920, 4, 544, 454])])
            .expect("the first clipped read says so");
        assert_eq!(first.len(), 1);
        assert!(first[0].contains("[1920, 4, 544, 454]"), "{first:?}");

        assert_eq!(
            clipped_roi_announcement(&mut said, &[("panel", [1975, 60, 544, 454])]),
            None,
            "the same region, still outside, at a rect the drag moved — not news",
        );
    }

    /// A SECOND region going outside is news, and so is the same region going
    /// outside again after coming back.
    ///
    /// The other half of the memory: it must not degrade into "say it once
    /// ever", which is the shape a `said.is_some()` guard would give it. Coming
    /// back in frame is not itself announced — nothing is wrong, and a log that
    /// narrates recovery evicts the failure it recovered from — but it does not
    /// suppress the next report either.
    #[test]
    fn the_clipped_notice_returns_when_the_set_changes_or_the_fault_comes_back() {
        let mut said = None;
        let panel = ("panel", [1920, 4, 544, 454]);
        let remaining = ("remaining", [810, 2000, 300, 46]);

        assert!(clipped_roi_announcement(&mut said, &[panel]).is_some());
        let both = clipped_roi_announcement(&mut said, &[panel, remaining])
            .expect("a second region outside is a different fact");
        assert_eq!(both.len(), 2, "{both:?}");

        assert_eq!(
            clipped_roi_announcement(&mut said, &[]),
            None,
            "coming back in frame is remembered, not narrated",
        );
        assert!(
            clipped_roi_announcement(&mut said, &[panel]).is_some(),
            "the fault returning is news again, not suppressed by the first report",
        );
    }

    /// An anchor the capture's own height does not corroborate is WITHHELD from
    /// the shared slice, and says why.
    ///
    /// The measured case (2026-09-03): the sweep's ceiling is soft, so a plate at
    /// true scale 2.10 against a 2.00 ceiling answers **2.05 at NCC 0.9390** —
    /// above [`anchor::NCC_FLOOR`], and so a "successful" anchor by every test
    /// this module applies to itself. Published, it would have become the
    /// geometry POE-233 places the lab OCR rects from, in a module that never
    /// looked at a temple, and persisted across restarts.
    ///
    /// The 1.000 row is the other arm: an anchor the height DOES corroborate is
    /// published, so the gate is not simply refusing everything.
    #[test]
    fn an_anchor_the_capture_height_does_not_corroborate_is_not_published() {
        let refused = screen_from_anchor(2.05, None, (1920, 1080), 7, (0, 0), 1_700_000_000_000)
            .expect_err("2.05 on a 1080p capture is 2.28 of unit ratio against k's 1.11");
        assert!(
            refused.contains("not corroborated by the capture"),
            "the withheld publish has to say why: {refused}",
        );

        let published = screen_from_anchor(
            anchor::table_scale(1920, 1080).expect("the measured row"),
            None,
            (1920, 1080),
            7,
            (0, 0),
            1_700_000_000_000,
        )
        .expect("the measured anchor on the capture it was measured from");
        assert_eq!(published.source, crate::ssot::ScreenScaleSource::TempleAnchor);
    }

    /// A STANDING measurement corroborates an anchor the capture height alone
    /// would refuse — and is the only thing that can.
    ///
    /// This is the machine whose in-game UI slider is off default: the scale is
    /// real, the height check cannot know that, and merc's gold frame can. Once
    /// the slice holds that reading the temple's anchor agrees with it, the
    /// publish is offered, and `ssot::accepts` refuses it as a restatement —
    /// which is where a value that says nothing new is supposed to be turned
    /// down. Fails if the height check is applied over a standing hint, which
    /// would stop the offer here and make `screen_from_anchor`'s own claim about
    /// reaching `accepts` false.
    ///
    /// The second arm is the guard that survives: an anchor that disagrees with
    /// the hint by more than a step did not come from the hint — the table row
    /// or a sweep answered — and the temple does not overrule a measurement of
    /// this screen with a board it read.
    #[test]
    fn a_standing_measurement_corroborates_an_anchor_the_height_would_refuse() {
        // 1080p at a raised slider: ui_scale 1.00 where the height implies 0.90.
        let slider = anchor::AnchorCalibration {
            screen_w: 1920,
            screen_h: 1080,
            scale: anchor::scale_for_ui_scale(1.00),
        };
        assert!(
            unit_ratio_line(slider.scale, 1920, 1080).is_some(),
            "the case is only interesting if the height check refuses it on its own",
        );

        let published =
            screen_from_anchor(slider.scale, Some(slider), (1920, 1080), 7, (0, 0), 1_700_000_000_000)
                .expect("the standing measurement is what corroborates it");
        let standing = crate::ssot::ScreenSlice {
            ui_scale: 1.00,
            ..remembered(ScreenScaleSource::MercFrame, 1920, 1080, 1.00, 7)
        };
        let mut slot = Some(standing);
        assert!(
            !crate::ssot::record_screen(&mut slot, published).accepted,
            "…and `accepts` is where it is turned down, as a restatement",
        );

        let elsewhere = screen_from_anchor(
            slider.scale + 2.0 * anchor::SCALE_STEP,
            Some(slider),
            (1920, 1080),
            7,
            (0, 0),
            1_700_000_000_000,
        )
        .expect_err("an anchor two steps off the hint did not come from it");
        assert!(
            elsewhere.contains("not corroborated by the remembered screen scale"),
            "the two withholdings must be distinguishable in the log: {elsewhere}",
        );
    }

    /// What the temple publishes, in the SHARED unit and with the capture's own
    /// display carried through.
    ///
    /// The scale is checked against the same two-sided measurement the hint test
    /// uses, from the other direction: the temple's measured 1.000 at 1920x1080
    /// is the shared unit's 1080/1200 = 0.90 there. Fails if the conversion is
    /// inverted, if the cue does not verify (POE-240 — an anchor is a
    /// full-resolution template match on this run's pixels), or if the monitor
    /// is re-derived from anything but the capture that produced the pixels.
    #[test]
    fn an_anchor_publishes_the_scale_the_shared_unit_calls_it() {
        let measured = anchor::table_scale(1920, 1080).expect("1920x1080 is the measured row");

        let next = anchored_screen(measured, (1920, 1080), 7, (-1920, 0), 1_700_000_000_000);

        let definition = 1080.0 / UI_SCALE_REFERENCE_HEIGHT;
        assert!(
            (next.ui_scale - definition).abs() <= definition * 0.01,
            "the temple's {measured} on a 1080p screen is {definition} in the shared unit, \
             not {}",
            next.ui_scale,
        );
        assert_eq!(next.source, ScreenScaleSource::TempleAnchor);
        assert!(next.verified_this_session, "an anchor looked at THIS run's pixels");
        assert_eq!((next.width, next.height), (1920, 1080));
        assert_eq!(next.monitor_id, 7);
        assert_eq!(next.origin, (-1920, 0));
    }

    /// The three answers `ssot::accepts` gives a temple anchor, which is what
    /// makes publishing on every anchored tick affordable.
    ///
    /// Restating a standing merc value is refused: the temple's number reaches
    /// this unit through a `k` whose own accuracy IS that band, so it cannot
    /// claim to improve on it, and taking it would flip `source` back and forth
    /// for as long as both panels are open. A different DISPLAY is taken
    /// whatever the band says — the standing value describes a screen the game
    /// has left. An empty slot takes anything.
    #[test]
    fn a_temple_anchor_replaces_a_merc_value_only_when_the_band_cannot_explain_it() {
        let merc = remembered(ScreenScaleSource::MercFrame, 1920, 1080, 0.900, 7);
        let restated = anchored_screen(
            anchor::scale_for_ui_scale(0.905),
            (1920, 1080),
            7,
            (0, 0),
            1_700_000_000_001,
        );
        let elsewhere = anchored_screen(
            anchor::scale_for_ui_scale(0.905),
            (1920, 1080),
            9,
            (-1920, 0),
            1_700_000_000_001,
        );
        let disagreeing = anchored_screen(
            anchor::scale_for_ui_scale(0.94),
            (1920, 1080),
            7,
            (0, 0),
            1_700_000_000_001,
        );

        let mut empty = None;
        assert!(
            crate::ssot::record_screen(&mut empty, restated).accepted,
            "an empty slot takes anything — a machine whose recruit window never opens \
             has only this",
        );

        let mut slot = Some(merc);
        assert!(
            !crate::ssot::record_screen(&mut slot, restated).accepted,
            "0.005 apart is inside the drift band: nothing new was said",
        );
        assert_eq!(
            slot.expect("the merc value stands").measured_at_ms,
            merc.measured_at_ms,
            "a refusal changes nothing at all, the stamp included",
        );

        let mut slot = Some(merc);
        assert!(
            crate::ssot::record_screen(&mut slot, elsewhere).accepted,
            "a reading off another display is not the band's business",
        );

        let mut slot = Some(merc);
        assert!(
            crate::ssot::record_screen(&mut slot, disagreeing).accepted,
            "0.04 apart is not drift — it is a different screen state",
        );
    }

    /// The other module's half of the handoff: a temple-sourced value is a seed
    /// merc treats like any other, and a gold-frame fit replaces it outright.
    ///
    /// Merc's own registration (`mercenary::run::next_fitted_scale`) reads its
    /// session's fit and never this slice, so there is nothing there to
    /// special-case a source label in. What merc DOES read is what `accepts`
    /// leaves standing, and this pins that a temple value neither blocks a frame
    /// fit nor is treated as more than the band-limited reading it is.
    #[test]
    fn a_merc_frame_fit_replaces_a_temple_sourced_value_like_any_other() {
        let temple = anchored_screen(
            anchor::table_scale(1920, 1080).expect("the measured row"),
            (1920, 1080),
            7,
            (0, 0),
            1_700_000_000_000,
        );

        let mut slot = Some(temple);
        let frame = remembered(ScreenScaleSource::MercFrame, 1920, 1080, 0.8985, 7);
        assert!(
            crate::ssot::record_screen(&mut slot, frame).accepted,
            "the gold frame always replaces — the temple's label buys it no protection",
        );
        assert_eq!(slot.expect("stored").source, ScreenScaleSource::MercFrame);

        let mut slot = Some(temple);
        let ocr = remembered(ScreenScaleSource::MercOcr, 1920, 1080, 0.8985, 7);
        assert!(
            !crate::ssot::record_screen(&mut slot, ocr).accepted,
            "…and an OCR estimate inside the band may not walk the session off it either",
        );
    }

    /// A temple anchor is written to disk and comes back as the ordinary
    /// `remembered` seed — which is how a machine whose recruit window is never
    /// opened hands merc a starting scale.
    ///
    /// Fails if `from_slice` refuses the new source (the value would be lost on
    /// every restart) or if `to_slice` carries the verification across a launch,
    /// which would have a file read claiming it looked at the screen.
    #[test]
    fn a_temple_anchor_survives_a_restart_as_a_remembered_seed() {
        let next = anchored_screen(
            anchor::table_scale(1920, 1080).expect("the measured row"),
            (1920, 1080),
            7,
            (0, 0),
            1_700_000_000_000,
        );

        assert!(
            crate::ssot::should_remember_screen(true, next.source),
            "a change measured by an anchor is worth a write",
        );
        let stored = crate::settings::ScreenScaleSetting::from_slice(&next)
            .expect("a temple anchor is persistable");
        let reloaded = stored.to_slice();

        assert_eq!(reloaded.ui_scale, next.ui_scale, "the number must come back bit-equal");
        assert_eq!(reloaded.monitor_id, 7);
        assert_eq!(reloaded.source, ScreenScaleSource::Remembered);
        assert!(!reloaded.verified_this_session, "a load is not a verification");
    }

    /// The hint line is said once per value, and never for the temple reading
    /// back a number it published itself.
    ///
    /// One line per second for the life of a session is not a log, and a line
    /// claiming a cross-module handoff that did not happen is worse than none.
    #[test]
    fn the_hint_line_is_said_once_per_value_and_never_for_the_temples_own() {
        let hint = anchor::AnchorCalibration { screen_w: 1920, screen_h: 1080, scale: 1.0 };
        let mut said = None;

        let first = hint_line(&mut said, Some(hint), Some(ScreenScaleSource::MercFrame));
        assert!(first.is_some_and(|l| l.contains("1.000")), "the first tick says so");
        assert_eq!(
            hint_line(&mut said, Some(hint), Some(ScreenScaleSource::MercFrame)),
            None,
            "and every tick after it stays quiet",
        );

        let mut said = None;
        assert_eq!(
            hint_line(&mut said, Some(hint), Some(ScreenScaleSource::TempleAnchor)),
            None,
            "reading back its own published scale is not a handoff",
        );
        assert_eq!(
            hint_line(&mut said, Some(hint), Some(ScreenScaleSource::MercFrame)),
            None,
            "…and it is remembered, so merc republishing the same hint is not news either",
        );

        let mut said = None;
        assert_eq!(hint_line(&mut said, None, None), None, "no hint, nothing to say");
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
    ///
    /// The frame is the live capture's size and origin so the derived rect lands
    /// INSIDE it — which is the case under test. An origin high enough in the
    /// frame to push the rect off the top is the other failure (`RectOutsideImage`),
    /// and it is [`a_diamond_rect_off_the_top_of_the_frame_is_refused`]'s.
    #[test]
    fn a_diamond_rect_over_blank_pixels_fails_rather_than_reporting_no_doors() {
        let img = DynamicImage::new_rgb8(1920, 1080);
        let layout = blank_layout((960, 713), 1.0, (1920, 1080));

        let err = read_markers(&img, &layout).expect_err("blank pixels carry no seals");
        assert!(
            err.contains("marker"),
            "the failure must name the seal count, got {err:?}",
        );
    }

    /// A panel drawn too near the capture's top edge — the window pushed part
    /// way off the monitor — leaves the diamond rect outside the frame, and that
    /// is reported as such rather than slid back in.
    ///
    /// Measured on the board fixture's own geometry: origin (673, 494) at scale
    /// 0.99 puts the diamond centre 3 px below the top of the frame, so the rect
    /// starts at y −100. A rect slid back into the frame would read whatever
    /// happened to be at the top of the capture and could return a confident
    /// door set from it.
    #[test]
    fn a_diamond_rect_off_the_top_of_the_frame_is_refused() {
        let img = DynamicImage::new_rgb8(1374, 773);
        let layout = blank_layout((673, 494), 0.99, (1374, 773));

        assert!(
            diamond_rect(layout.origin, layout.scale)[1] < 0,
            "the case only exists while the rect leaves the top of the frame",
        );
        let err = read_markers(&img, &layout).expect_err("the rect is not in the capture");
        assert_eq!(err, markers::MarkerError::RectOutsideImage.to_string());
    }

    /// A layout with no beam-read doors, for the rect tests that only need an
    /// origin, a scale and a current room.
    fn blank_layout(origin: (i32, i32), scale: f32, screen: (u32, u32)) -> TempleLayout {
        TempleLayout {
            origin,
            scale,
            ncc: 0.94,
            confidence: crate::temple::doors::Confidence::High,
            current: Some(crate::temple::lattice::Slot::B0),
            doors: Default::default(),
            uncertain: Default::default(),
            slots: [(0, 0); 13],
            thresholds: crate::temple::doors::Thresholds { horizontal: 0.2, diagonal: 0.2 },
            calibration: crate::temple::anchor::AnchorCalibration {
                screen_w: screen.0,
                screen_h: screen.1,
                scale,
            },
        }
    }
}
