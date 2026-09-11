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
//! Three things open it, and only the first is Client.txt's: Alva's START line
//! or the temple area, the layout panel this loop's last tick SAW
//! ([`LoopState::live`], POE-246), and the one probe tick a starting loop runs
//! before it may believe an empty screen ([`LoopState::probe_pending`]). The
//! panel input is what stops a stand-down landing on a sheet the player is still
//! reading — measured 2026-09-03, before it existed, the overlay went off a
//! panel that was open.
//!
//! # What shuts it, and the cycle that WI-1 added (2026-09-07)
//!
//! Client.txt shuts it on a non-START Alva line or a zone change
//! ([`super::trigger::apply_line`]). The fourth way is this loop's own, and it
//! is the owner's rule that there are no tails any more: once a board has been
//! READ, the retire after it — the sheet closing — completes the cycle and the
//! loop stops capturing entirely ([`cycle_complete`],
//! [`super::trigger::ArmState::complete_cycle`]). A sheet closed BEFORE any read
//! completed does not complete anything: the loop keeps probing, because the
//! sheet it was armed for has not been read yet.
//!
//! The retire is the SECOND consecutive clean miss since 2026-09-11 (POE-275,
//! owner — [`RETIRE_AFTER`]); from WI-1 until then it was the first. The miss
//! before it is a held miss ([`DetectOutcome::HeldMiss`]): it publishes
//! nothing ([`miss_publish`]), keeps [`LoopState::live`] and with it the gate,
//! and completes nothing — one miss is a misread or a tooltip over the plate,
//! not a close.
//!
//! The offer boxes come down on the retiring miss ([`miss_publish`] answers
//! [`TickOutcome::NoPanel`] there) and the room diamond survives it (POE-248),
//! so what the completion adds is the capture stopping. Accepted cost, owner
//! 2026-09-07: reopening the sheet later in the same incursion shows no boxes
//! until Re-arm, because nothing is looking any more.
//!
//! [`loop_step`] is that gate plus the focus check and the cadence check, as
//! one pure function, so the property that matters ("a disarmed loop never
//! reaches `capture_screen`") is a property of the step rather than of a
//! status.
//!
//! # Two gates before an expensive read (POE-249, POE-269)
//!
//! A full read is 28 OCR calls: two bounded text crops and two per plate for
//! all 13. The detect half therefore does one full-resolution, windowed NCC at
//! the Entrance origin supplied by [`crate::ssot::placements`]. That recheck is
//! both the panel-presence test and the only steady-state anchor work.
//!
//! A score below [`anchor::NCC_FLOOR`] is a miss. When the slice is null or has
//! no anchored Entrance origin, the loop counts its consecutive clean misses and
//! starts a cold sweep on every [`NULL_SWEEP_EVERY`]-th of them, up to
//! [`NULL_SWEEP_CAP`] sweeps per `(temple_epoch, temple_rearm)` key (POE-275
//! WI-2, owner 2026-09-11). Until then the FIRST such miss per key spent the one
//! sweep — before the sheet could be open — and a sweep that found nothing left
//! the key spent, so the player needed Re-arm or Recalibrate. If a sweep finds an
//! anchor whose proposed slice is withheld, the cadence goes on once; a second
//! withheld result ends that key's null sweeps until the key changes. A placed
//! board gets one explicit fallback sweep per key only under the Manual arm
//! (Re-arm): a miss under AlvaStart or TempleArea is the sheet not being open
//! yet, not a wrong placement — see [`cold_sweep_reason`]. No sweep starts over a
//! live panel. A sweep that finds another origin logs the contradiction, uses
//! that origin for this read, and the successful read remembers it through the
//! SSOT effects seam. The null-slice cadence is the only sweep cadence; there is
//! no moving-origin budget or session plate memory. The per-key budget is
//! sufficient because the shared slice is corroborated across modules (ADR-020),
//! while the placed origin is verified on every tick by the recheck.
//!
//! The OCR gate remains independent: [`LoopState::gate`] compares the board key
//! and [`slice::BoardFrame`], re-shows an already-read board, or pays for the
//! read and its bounded [`RETRIES`] partial rounds. [`LoopState::on_detect`]
//! and [`publish_anchor_scale`] still run before that gate on every sighting.
//!
//! # The detect cadence
//!
//! Every [`DETECT_INTERVAL`] the loop captures and runs the placed-origin
//! recheck. Its NCC and elapsed milliseconds are logged once for the first
//! placed recheck that anchors in a session. A successful read logs [`read_timings_line`];
//! these timings are measurements, not gates.
//!
//! # The cold fallback (POE-234, POE-269, POE-275)
//!
//! [`cold_sweep`] uses [`anchor::anchor_for_loop`]'s coarse-to-fine pyramid
//! only on the explicit fallback paths above. A null or unplaced screen slice
//! spends it every [`NULL_SWEEP_EVERY`] consecutive clean misses, up to
//! [`NULL_SWEEP_CAP`] times per `(temple_epoch, temple_rearm)` key; a second
//! found-but-withheld result ends that key's null sweeps until it changes. A
//! placed miss spends it once per the same key under the Manual arm (Re-arm)
//! only, since 2026-09-09 — until then any trigger arm could.
//!
//! Since 2026-09-11 (POE-275 WI-2, owner) the sweep runs on a thread of its own
//! over its own frame ([`SweepSlot`]), one at a time, and the loop goes on
//! ticking at [`DETECT_INTERVAL`] while it searches. Until then it ran inside the
//! tick and no tick ran for its whole duration: 5.3 s in the release container,
//! ~30 s on the PC's debug build (app.log 2026-09-08/09). A placed recheck that
//! anchors mid-sweep cancels it and its origin wins; a key change, a stand-down
//! and the module stopping cancel it too. A sweep that FOUND the panel is read
//! only after a later capture confirms its origin ([`confirm_swept`]), because
//! the sheet can close during a multi-second search. Every sweep writes one
//! [`sweep_line`] when it ends — the measurement the two numbers above are to be
//! corrected from.
//!
//! # The exhaustive sweep is not reachable from here
//!
//! `anchor::anchor_with_hint`'s last resort is `anchor::full_sweep`, measured
//! at 28.4 s in the container and 347.8 s on the laptop. Every anchoring call
//! in this file goes through [`cold_sweep`] and [`anchor::anchor_for_loop`],
//! whose pyramid sweep is the explicit fallback. The exhaustive one stays
//! reachable from `super::commands::temple_debug_capture`, where a user pressed
//! a button and is waiting for it.
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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
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
/// Presence-tick cadence: how often the loop looks for (or re-checks) the
/// layout panel. One anchor match plus one beam-sampling pass, no OCR.
///
/// 650 ms, owner-decided (docs/TEMPLE-LIFECYCLE.md, "Owner decisions"): the
/// sheet is on screen for as long as the player is reading it, and the tick is
/// what both hides the sheet-bound overlays after it closes and notices it
/// opening. A second of either is a second of the overlay disagreeing with the
/// screen. It is affordable at this rate because it is *only* the cheap half —
/// since POE-249 an anchored tick no longer implies OCR (see [`LoopState::board`]).
const DETECT_INTERVAL: Duration = Duration::from_millis(650);
/// A cheap detect tick slower than this is LOGGED — [`slow_tick_line`], at most
/// one line per [`SLOW_TICK_LOG_EVERY`] — and nothing else.
///
/// It is [`DETECT_INTERVAL`] on purpose: a tick that takes longer than the
/// cadence is the loop running below its nominal rate, which is the one fact
/// about a tick worth a line. It is a MEASUREMENT, never a switch.
///
/// # There is no slow-machine backoff (owner decision, 2026-09-06)
///
/// Until 2026-09-06 a cheap tick over 1.5 s backed the cadence off to 3 s for
/// the life of the thread. The one time it fired on the PC (app.log,
/// `2026-09-06 21:57:12`, 1519 ms) it was a single screen-capture stall as a
/// fight started, and it cost every later incursion of that session up to 3 s
/// of detect latency — the sheet had to wait for a 3 s tick to be seen. The
/// owner's rule replaces it: *"if the cheap probe is really cheap — we can just
/// straight go to it"* (docs/TEMPLE-LIFECYCLE.md, "Owner decisions").
///
/// Nothing is lost for correctness. The loop is self-pacing, and it sleeps the
/// interval AFTER the tick rather than around it — `last_detect` is stamped when
/// `tick` returns — so a slow tick already delays the next one by its whole
/// duration, and a machine that cannot hold 650 ms runs at tick + 650 ms per
/// detect. What that costs is CPU while ARMED, which POE-242 bounds to Alva's
/// window.
const SLOW_TICK: Duration = DETECT_INTERVAL;
/// At most one slow-tick line per this much wall time. A machine that is slow
/// on EVERY tick would otherwise write one line per tick into `app_log`'s
/// 50-entry buffer; the file log is append-only either way.
const SLOW_TICK_LOG_EVERY: Duration = Duration::from_secs(10);
/// How long to idle between focus checks while the game is not focused.
const UNFOCUSED_NAP: Duration = Duration::from_millis(1000);
/// Distinct error messages logged before the loop stops repeating itself. The
/// failure path re-runs on every tick and an error carrying a varying number is
/// a different string every time, so without a cap one loop could fill the
/// 50-entry LOGS buffer on its own.
const MAX_DISTINCT_ERRORS: usize = 12;
/// Consecutive failed anchors that retire a live panel.
///
/// **Two since 2026-09-11** (POE-275, owner) — the last section below; the
/// sections before it are the history of one, left standing.
///
/// **One, since POE-249** (docs/TEMPLE-LIFECYCLE.md row 3: hide every
/// sheet-bound overlay on the first miss). Two was the number when a retire
/// re-armed the read gate — a panel briefly lost mid-fade cost a full 28-call
/// re-read on the way back, so the second miss was worth waiting for. Neither
/// half of that is true any more: [`miss`] already publishes
/// [`TickOutcome::NoPanel`] on the FIRST clean miss, so the overlays came down
/// then regardless, and a reopened sheet inside the same incursion re-shows
/// what was read instead of reading again ([`LoopState::board`]).
///
/// So what two bought was not a delayed hide — it was [`LoopState::live`], the
/// `layout panel gone` log line and the arm gate's view of the panel
/// disagreeing with the status the user could see, for one tick. One makes them
/// agree.
///
/// # What WI-1 added to it (2026-09-07)
///
/// The retire is now also what ENDS THE CYCLE once a board has been read
/// ([`cycle_complete`]), so the constant decides how long a sheet may drop out
/// of the anchor's sight before the loop concludes the player closed it — and
/// the conclusion is no longer recoverable by looking again, because the loop
/// stops looking.
///
/// The residual is one anchor miss over a sheet that is still open: the offer
/// boxes hide (which [`miss`] already did on the first clean miss at two as
/// well) and, unlike before, do not come back on the next tick. It is bounded
/// by the same Re-arm the owner accepted the reopen cost against, and it is
/// bounded on the read side too — the board itself is already published, so what
/// is lost is the boxes rather than the advice or the room widget. Raising the
/// constant would trade that for a stand-down the player waits an extra tick for
/// after every sheet they really did close, which is the thing POE-249 measured
/// and shortened.
///
/// # Two, since 2026-09-11 (POE-275, owner)
///
/// *"RETIRE_AFTER goes from 1 to 2 - one missed probe is easy to get (a
/// misread, a tooltip over the plate) and must neither hide the sheet-bound
/// overlays nor end the cycle; two consecutive misses do both."* It reverses the
/// POE-249 rule and the WI-1 residual above.
///
/// The constant alone did not deliver it, because [`miss`] published
/// [`TickOutcome::NoPanel`] on every clean miss. The first miss over a live
/// panel is now [`DetectOutcome::HeldMiss`], which publishes nothing
/// ([`miss_publish`]): the status the last sighting wrote stays on the slice,
/// and [`LoopState::live`] — with it the arm gate — stays true. The second
/// consecutive miss retires the panel, takes the sheet-bound overlays down and,
/// over a board already read, completes the cycle. A sighting between the two
/// resets the count; a failed grab does not touch it.
///
/// What the second miss costs: a sheet the player really did close hides its
/// overlays and stands the capture down one [`DETECT_INTERVAL`] tick (650 ms)
/// later than it did at one.
const RETIRE_AFTER: u8 = 2;
/// Consecutive clean misses on a null or unplaced screen slice that start the
/// next cold sweep (POE-275 WI-2, owner 2026-09-11) — [`cold_sweep_reason`].
///
/// Three: about 2 s of recheck at [`DETECT_INTERVAL`]. The count restarts when a
/// sweep STARTS and does not move while one is in flight, so the next sweep
/// needs three misses after the last one ended rather than following it
/// back-to-back (a 5.3 s sweep spans eight ticks). A held miss, a retire and a
/// failed grab do not count; a sighting starts the count again.
///
/// A placed recheck that anchors on a screen with no ANCHORED origin — the seed
/// was right — ENDS the key's null sweeps ([`SweepBudget::on_recheck`]): a sweep
/// under that key could only find what the recheck found, and without the end
/// the cadence would restart after every close in the same key.
///
/// Why not one, which was the rule until 2026-09-11: the first miss after
/// Alva's start line is the sheet not being open yet, and on a null slice the
/// sweep it spent was the key's only one — with no hint `anchor::detect_cheap`
/// then answers `Nothing` for the rest of the key, and the player needed
/// Re-arm or Recalibrate (the owner's friend hit exactly this on a first run).
///
/// Provisional, like [`NULL_SWEEP_CAP`]: both are to be corrected from the
/// per-sweep [`sweep_line`] in `app.log`.
const NULL_SWEEP_EVERY: u8 = 3;
/// Null-slice cold sweeps one `(temple_epoch, temple_rearm)` key may start
/// (POE-275 WI-2). A key change — Re-arm, a new epoch — resets it together with
/// the [`NULL_SWEEP_EVERY`] count.
///
/// Ten bounds what an unplaceable screen costs one incursion: ten sweeps
/// (53 s of one core in the release container) spread over at least
/// 10 × 3 ticks of recheck between them. Provisional — see [`NULL_SWEEP_EVERY`].
const NULL_SWEEP_CAP: u8 = 10;

/// Spawn the capture loop. Called through `MODULES` — see `modules.rs`.
pub fn spawn(app: AppHandle, cancel: watch::Receiver<bool>) -> ModuleJoin {
    ModuleJoin::Thread(std::thread::spawn(move || run_loop(app, cancel)))
}

// ---------------------------------------------------------------------------
// Pure pieces
// ---------------------------------------------------------------------------

/// The loop's panel state machine: when a panel that has stopped anchoring has
/// been missing long enough to retire.
///
/// Separated from the loop so the rule — retire after [`RETIRE_AFTER`] misses —
/// is testable without a screen or a clock. Same shape as
/// `mercenary::run::LoopState`, deliberately: the two loops solve the same
/// cadence problem and a second shape would be a second thing to reason about.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoopState {
    /// A layout panel is on screen — the loop's LAST detect tick found one, or
    /// (since 2026-09-11, POE-275) missed it once after a tick that did: a held
    /// miss ([`DetectOutcome::HeldMiss`]) leaves this set, and only the retire
    /// after [`RETIRE_AFTER`] consecutive clean misses clears it.
    ///
    /// Since WI-1 (2026-09-07) this is also the arm gate's whole view of the
    /// screen ([`trigger::arm_source`]'s `panel_live`): a sheet the player is
    /// holding open keeps the loop armed whatever Client.txt says, and the tick
    /// that retires it is the one that takes the gate with it. The 120 s
    /// `panel_seen_ms` clock POE-246 measured this with is retired — see the
    /// module doc.
    pub live: bool,
    /// Consecutive failed anchors since the last successful one.
    ///
    /// A live count again since 2026-09-11 (POE-275): at [`RETIRE_AFTER`]
    /// `= 2` a first clean miss over a live panel leaves it at `1`
    /// ([`DetectOutcome::HeldMiss`]) and the second retires the panel and zeroes
    /// it. A sighting zeroes it; a failed grab ([`Self::on_blind_tick`]) leaves
    /// it alone. From POE-249 until then it was only ever `0`, the constant
    /// being `1`.
    pub misses: u8,
    /// Whether the one detect a starting loop runs before it may stand down has
    /// been spent (POE-246 — see `trigger`'s start-up probe note).
    ///
    /// Spelled as SPENT rather than pending so `Default` still derives to the
    /// right answer: a loop that has run no tick owes one.
    pub probe_spent: bool,
    /// The board this loop has already read, and what it still owes it
    /// (POE-249). `None` until the first completed read.
    ///
    /// The OCR gate's whole state. See [`BoardRead`] for the key and
    /// [`Self::wants_read`] for the rule; the read's PAYLOAD is not here but on
    /// `Session::kept`, because this struct is `Eq` and cheap and a
    /// [`slice::KeptRead`] is neither.
    pub board: Option<BoardRead>,
}

/// What the loop read for one board, and how much of a retry budget is left.
///
/// # The identity is `(epoch, rearm)` PLUS the pixel frame
///
/// `epoch` is `crate::AppState::temple_epoch`, bumped when Alva speaks or the
/// player changes zone — the two events that BRACKET an incursion cycle
/// (POE-249 WI-1, docs/TEMPLE-LIFECYCLE.md row 4). Only an incursion adds a room
/// or an upgrade, and `super::trigger::ends_epoch` bumps on the START line as
/// well as the END one, so a completed incursion always has a bump on each side
/// of it. What that does NOT mean is that the contents hold still between two
/// bumps: the kill itself happens INSIDE the epoch it opened — see "what is
/// left" below.
///
/// `rearm` is `crate::AppState::temple_rearm`, which the Re-arm button and
/// every settings command bump. It is in the key because the user pressing it
/// means "read that again" — a frame could not express that, and this is
/// the accepted cost of an epoch that a MISSED Alva line would leave running
/// (measured 1 orphan in 342 lines; the manual override is the answer).
///
/// # Why the key alone is not enough
///
/// The key is blind to everything that moves INSIDE one epoch, and three things
/// do. The player walks to the next room, which moves `layout.current` — the
/// Temple of Atzoatl run, where the sheet is the navigation aid and no
/// `EnteredTemple` bumps anything. A corridor the beam could not see resolves,
/// which moves `layout.doors`. The game window is dragged or the UI scale
/// changes, which moves `origin`/`scale`. A reopen answered on the key alone
/// would put back the PREVIOUS room's outline, seals, advice and never-cover
/// set over the frame in front of the player (ADR-019).
///
/// So the identity carries a [`slice::BoardFrame`] as well: what the sheet says
/// (`current`, `doors`, `uncertain`, compared exactly) and where it says it
/// (`origin`, `scale`, compared inside a BAND). It is pixels-only, so it costs
/// the one anchor match this tick has already paid for. A frame that moved is a
/// NEW board: it is read, and it starts from a whole [`RETRIES`] budget rather
/// than out of the old board's.
///
/// It is not the OLD gate. That one hashed the panel TEXT too, so it needed the
/// 28 OCR calls to compute what it was deciding whether to spend, and it was
/// fooled in both directions: a re-drawn frame moved the hash and bought a
/// re-read, and a kill that changed nothing the hash covered was invisible. The
/// key answers the second; the frame answers only what pixels can see, and its
/// band is what stops the first.
///
/// # The band, and what is left outside it
///
/// Inside [`slice::FRAME_ORIGIN_TOLERANCE`] px and
/// [`slice::FRAME_SCALE_TOLERANCE_DENOM`]'s one per cent, the sheet has not
/// moved — it has been re-found by a correlation over a frame the game is still
/// drawing — and a reopen re-shows. Beyond it the window was dragged or the UI
/// was rescaled, and every ROI this read placed is somewhere else, so it reads.
///
/// Two things are left outside both halves.
///
/// **The kill, mid-incursion.** The architect dies between the START line and
/// the END line, so it lands inside one epoch: a plate changes name or tier and
/// both offers are replaced, and none of it is something the frame can see —
/// `current`, `doors` and `uncertain` are untouched and the panel has not moved.
/// A sheet reopened without walking anywhere therefore re-shows the PRE-kill
/// board until the END line bumps the epoch. The answer is Re-arm, which is half
/// the key and exists for exactly the cases neither half can see. This is the
/// same shape as the missed-Alva-line orphan above and is bounded the same way:
/// one incursion, ended by a line the loop is already watching for.
///
/// **An origin that moves.** A frame outside the origin or scale band is a new
/// board position and is read from the newly resolved anchor. It receives the
/// ordinary read/retry rules; there is no separate geometry-only cap.
///
/// A MAP-side reopen is not sighted, and that is the design rather than a
/// defect: the cycle completed when the sheet closed and the loop stood down, so
/// nothing is looking. Re-arm is the way back and it forces a READ — it is half
/// the key — so it produces the `layout panel found` line and never the `back`
/// one. See docs/TEMPLE-LIFECYCLE.md's residual, and OVERLAY-GUIDE.md's smoke
/// item, which is written against the temple run for this reason.
///
/// [`slice::merge_reads`] tests the SAME frame with the SAME predicate, so a
/// retry this gate lets through as one board is one the merge will fold rather
/// than discard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardRead {
    /// `(temple_epoch, temple_rearm)` when this read was taken.
    pub key: (u64, u64),
    /// The frame this read was taken at — the half of the identity the key
    /// cannot see. See the type's note.
    pub frame: slice::BoardFrame,
    /// The status [`slice::project`] wrote — [`TempleStatus::Read`], or
    /// [`TempleStatus::NoCurrentRoom`] for a sheet opened between rooms. It is
    /// what [`TickOutcome::Reshown`] puts back on a reopen, so the badge a
    /// reopened sheet shows is the one its own read produced rather than a
    /// guess.
    pub status: TempleStatus,
    /// Whether some region of that read did not come out clean
    /// ([`slice::unclean`]).
    pub unclean: bool,
    /// Reading rounds still owed to an unclean board.
    ///
    /// Spent by a RETRY — a read of the same board in the same place — and by
    /// nothing else. A geometry-only move carries it across unchanged; see
    /// [`LoopState::note_read`]. What a retry round actually OCRs is
    /// [`slice::plan_read`]'s answer and is usually a fraction of a full read;
    /// this budget bounds the ROUNDS, not the calls.
    pub retries_left: u8,
}

/// Extra reading rounds an UNCLEAN board is worth, on top of the first.
///
/// Two, owner-decided — docs/TEMPLE-LIFECYCLE.md row 2, and restated
/// 2026-09-07: *"We do up to 2 more rounds of temple reading, but only for the
/// parts that was previously not clear, so we in fact do 3 reading rounds
/// total."* The failures a retry recovers are the ones a redraw fixes — OCR
/// over a half-drawn panel, a plate the game was still fading in — and those
/// are gone by the second look. Past that the cause is the read itself (a plate
/// name outside the vocabulary, a diamond the selection frame covers) and
/// paying for it every 650 ms for the rest of the incursion buys nothing.
///
/// The second half of the quote is [`slice::plan_read`]: rounds 2 and 3 re-read
/// only the unclean regions, so this budget costs three FULL reads only on a
/// board where every region failed.
pub const RETRIES: u8 = 2;

/// What the OCR gate decided about one sighting — [`LoopState::gate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateAnswer {
    /// Pay for the 28 OCR calls.
    Read,
    /// Re-show `status`: the same board, in the same place, with nothing owed.
    Reshow(TempleStatus),
}

/// What one pixel tick did to the panel state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectOutcome {
    /// A panel was found where there was none.
    Found,
    /// A panel that was already live anchored again.
    Held,
    /// Nothing anchored, and nothing was live — the loop searching.
    Missed,
    /// Nothing anchored over a LIVE panel that has not yet missed
    /// [`RETIRE_AFTER`] times in a row (POE-275, owner 2026-09-11). The panel
    /// stays live and [`miss`] publishes nothing ([`miss_publish`]), so the
    /// sheet-bound overlays and the arm gate hold, and [`cycle_complete`] does
    /// not fire.
    HeldMiss,
    /// The live panel just retired after [`RETIRE_AFTER`] misses.
    Retired,
    /// The tick could not LOOK — the screen grab failed, so this tick is not
    /// evidence about the panel either way ([`LoopState::on_blind_tick`]).
    Blind,
}

impl LoopState {
    /// Fold one anchor result into the state.
    ///
    /// `seen` is whether this tick found the layout panel. A bool again since
    /// WI-1 (2026-09-07): it carried a `now_ms` stamp only to feed POE-246's
    /// 120 s panel clock, and the gate now reads [`Self::live`] — "the last tick
    /// saw it" — which this method already maintains.
    ///
    /// Called by every tick that LOOKED — the anchored path and a clean miss —
    /// which is what makes it the place those ticks spend the start-up probe.
    /// A tick whose grab failed goes through [`Self::on_blind_tick`] instead;
    /// between the two, every tick spends the probe exactly once.
    pub fn on_detect(&mut self, seen: bool) -> DetectOutcome {
        self.probe_spent = true;
        if seen {
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
                DetectOutcome::HeldMiss
            }
        }
    }

    /// A tick whose screen GRAB failed: spend the start-up probe, and touch
    /// nothing else.
    ///
    /// The rule, stated once: **an errored tick says nothing about the sheet.**
    /// [`Self::live`] and [`Self::misses`] are left exactly as the last tick
    /// that could see the screen left them, so a transient capture failure
    /// cannot retire a panel that is in front of the player. Since WI-1 `live`
    /// is the whole panel branch of the arm gate
    /// ([`trigger::arm_source`]), so folding this tick through
    /// [`Self::on_detect`] with `seen = false` would count a failed grab toward
    /// [`RETIRE_AFTER`]. At `1` (until 2026-09-11) that stood the capture down on
    /// ONE failed grab whenever the panel is the only thing holding the gate (a
    /// hideout read, a Re-arm whose grace has run out under an open sheet),
    /// recoverable only by Re-arm; at `2` two failed grabs, or one beside a real
    /// miss, would do the same.
    ///
    /// The probe IS spent, and that is the one thing this tick does prove: it
    /// ran. A machine whose capture never succeeds must not hold the gate open
    /// for the session (POE-246), so the debt a starting loop owes itself is
    /// settled by a tick that failed as much as by one that looked.
    ///
    /// [`DetectOutcome::Blind`] rather than `Missed`: `Missed` is a claim about
    /// the screen, and this tick did not see one. It is also what keeps
    /// [`cycle_complete`] honest through the outcome alone — a blind tick can no
    /// longer reach the `Retired` branch that rule keys on.
    pub fn on_blind_tick(&mut self) -> DetectOutcome {
        self.probe_spent = true;
        DetectOutcome::Blind
    }

    /// Whether the loop still owes itself the one detect it runs before it may
    /// stand down (POE-246). [`trigger::arm_source`]'s third input.
    pub fn probe_pending(&self) -> bool {
        !self.probe_spent
    }

    /// Whether this loop has a completed read of the board `key` names (WI-1).
    ///
    /// [`cycle_complete`]'s second input, and the KEY half of the identity
    /// alone: the frame is deliberately not asked about. A sheet that closed is
    /// not on screen to compare a frame against, and what the cycle rule needs
    /// to know is whether this incursion has been read at all — the board the
    /// player walked away from and the board they would see if they reopened are
    /// the same board while the key holds.
    ///
    /// The key IS asked about, and that is what makes Re-arm work: pressing it
    /// bumps `temple_rearm`, so the read in hand is a read of another key and
    /// the loop probes for a new one instead of standing down on the old one.
    /// An Alva line or a zone change moves the epoch the same way, though those
    /// have already disarmed the gate by their own rule.
    ///
    /// What is NOT asked is whether that read came out UNCLEAN with rounds
    /// still owed — the question [`Self::gate`] does ask — so a sheet closed
    /// mid-budget completes its cycle and loses the rest of [`RETRIES`]. That
    /// is an accepted residual with Re-arm as its answer; it is named in
    /// `docs/TEMPLE-LIFECYCLE.md` with the rest of them.
    pub fn has_read(&self, key: (u64, u64)) -> bool {
        matches!(&self.board, Some(board) if board.key == key)
    }

    /// The OCR gate (POE-249): what this sighting of `(key, frame)` gets.
    ///
    /// The whole rule, in one place, because the two answers are two branches of
    /// one question and a caller that asked them separately could see them
    /// disagree. [`Self::reshow`] and [`Self::wants_read`] are views over it.
    ///
    /// The order is the order of the three thirds:
    ///
    /// 1. a different key, or a different `semantic`, is a different BOARD —
    ///    read, and the budget starts over;
    /// 2. the same board whose frame MOVED past the band is the same OCR
    ///    content in a different place — the ROIs are stale, so read;
    /// 3. the same board in the same place re-shows, unless it read unclean and
    ///    is still owed a retry.
    pub fn gate(&self, key: (u64, u64), frame: &slice::BoardFrame) -> GateAnswer {
        let Some(board) = &self.board else {
            return GateAnswer::Read;
        };
        if board.key != key || !board.frame.same_content(frame) {
            return GateAnswer::Read;
        }
        if !board.frame.matches(frame) {
            return GateAnswer::Read;
        }
        if board.unclean && board.retries_left > 0 {
            return GateAnswer::Read;
        }
        GateAnswer::Reshow(board.status)
    }

    /// Whether the board this loop last read is the one `(key, frame)`
    /// describes, in the same place.
    ///
    /// The identity rule as a bool — see [`slice::BoardFrame`] for what each
    /// third catches. Two callers ask it and they must not drift apart:
    /// [`Self::note_read`] (is this a retry, or something else?) and
    /// [`kept_for`] (may the kept reading be merged into, or must it be dropped
    /// first?). [`Self::gate`] asks the same two questions in its own order
    /// because it has a third answer to give.
    ///
    /// It is NOT the read decision: a moved frame is not the same board.
    pub fn same_board(&self, key: (u64, u64), frame: &slice::BoardFrame) -> bool {
        matches!(&self.board, Some(board) if board.key == key && board.frame.matches(frame))
    }

    /// The status a sighting of `(key, frame)` can be answered with, or `None`
    /// when it has to pay for a read. [`Self::gate`]'s answer, narrowed.
    ///
    /// Spelled as "what can I answer with?" rather than as a bare bool so the
    /// caller has no unreachable branch to write: the status a reopen re-shows
    /// and the decision to re-show at all are one answer.
    /// [`Self::wants_read`] is its negation, and is what the tests name.
    pub fn reshow(&self, key: (u64, u64), frame: &slice::BoardFrame) -> Option<TempleStatus> {
        match self.gate(key, frame) {
            GateAnswer::Read => None,
            GateAnswer::Reshow(status) => Some(status),
        }
    }

    /// Whether a sighting of `(key, frame)` pays for a full read — the negation
    /// of [`Self::reshow`].
    ///
    /// Both exist because the gate answers two questions at once and the tick
    /// needs them together: it re-shows the status of the board it skipped. This
    /// is the same rule stated as the bool the module doc and
    /// docs/TEMPLE-LIFECYCLE.md row 2 describe, and it is what the tests name —
    /// asserting on `Option<TempleStatus>` would tie every one of them to the
    /// status a fixture happened to record.
    #[allow(dead_code)] // The rule, named. The loop calls `gate`, which is wider.
    pub fn wants_read(&self, key: (u64, u64), frame: &slice::BoardFrame) -> bool {
        self.reshow(key, frame).is_none()
    }

    /// Record what a completed read of `(key, frame)` produced.
    ///
    /// # Three thirds, two kinds of change
    ///
    /// The identity has three parts and they do not all mean the same thing to a
    /// retry BUDGET, which is a budget for OCR:
    ///
    /// - **the key or the content moved** — a different board is behind the
    ///   sheet, so this is a first look at it and it gets the whole [`RETRIES`]
    ///   budget;
    /// - **only the geometry moved** — the same board was re-placed. The ROIs
    ///   were stale, which is why it had to be read, but the OCR budget carries
    ///   across unchanged;
    /// - **nothing moved** — a retry, and the only thing that spends one.
    ///
    /// The decrement is keyed on [`slice::BoardFrame::matches`] rather than on a
    /// flag the caller passes: a retry is by definition a second read of the
    /// same board in the same place, so the two cannot drift apart.
    pub fn note_read(
        &mut self,
        key: (u64, u64),
        frame: &slice::BoardFrame,
        status: TempleStatus,
        unclean: bool,
    ) {
        let retries_left = match &self.board {
            Some(board) if board.key == key && board.frame.same_content(frame) => {
                if board.frame.matches(frame) {
                    board.retries_left.saturating_sub(1)
                } else {
                    board.retries_left
                }
            }
            _ => RETRIES,
        };
        self.board = Some(BoardRead {
            key,
            frame: *frame,
            status,
            unclean,
            retries_left,
        });
    }
}

// ------------------------------------------------------ the measurements --

/// What one detect tick spent, stage by stage, in wall time.
///
/// Written by [`tick`] as each stage finishes (write-through, so a tick that
/// returns early still leaves the stages it ran) and read twice: by
/// [`slow_tick_line`] for a cheap tick that overran the cadence, and by
/// [`read_timings_line`] for the read line, which carries the OCR half as well.
/// `Duration::ZERO` is a stage this tick did not run.
///
/// Every number here is a MEASUREMENT of the machine and the build that took it
/// (the laptop's debug build grabbed the screen in 225 ms, the PC's release
/// build in 52 ms — same code), which is why the lines print the numbers and
/// assert no budget. The budget is the owner's — verdict on screen about 1 s
/// after the panel opens (docs/TEMPLE-LIFECYCLE.md) — and these lines are how
/// it is checked against a real session rather than assumed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TickStages {
    /// `capture::capture_screen` — the monitor grab.
    pub capture: Duration,
    /// `anchor::detect_cheap` — the presence probe.
    pub cheap: Duration,
    /// Resolving the anchor into a `TempleLayout` (doors included), on the
    /// ticks that did.
    pub anchor: Duration,
}

/// What the full read spent after the anchor, stage by stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadStages {
    /// The panel and budget-line OCR (`panel_text`).
    pub text_ocr: Duration,
    /// The plate OCR calls (`panel::read_slots`): up to 26, two per PLANNED
    /// plate, so 26 on a full round and two per still-unread plate on a retry.
    pub plate_ocr: Duration,
    /// The door markers (`read_markers`).
    pub markers: Duration,
    /// Getting the 25 x 3 valuation table — a LOOKUP when nothing it depends
    /// on has moved (`ssot::temple_valuation_now`), the build when something
    /// has. Split off `advise` by POE-257 WI-3 (2026-09-07) because the two
    /// were one number and the owner's question — is the table cached? — could
    /// not be answered from the log: the pair measured 193-245 ms on the PC's
    /// release build while the advisor's own ranking is 13-22 ms of it
    /// (`advisor/mod.rs`, `the_conditional_ranking_cost_on_case_eight`).
    ///
    /// **It measures the lookup or the build, never a lock wait.** The accessor
    /// holds `AppState.temple_valuation` for a `ValuationKey` comparison and
    /// drops it before building, so this number cannot be inflated by another
    /// thread's in-flight compute. A `cached` reading here is therefore a claim
    /// about the work this read did and not about what it waited for — which is
    /// what makes a two-digit `(cached)` a real finding rather than contention.
    pub valuation: Duration,
    /// Whether [`Self::valuation`] measured a lookup or a build. The read line
    /// prints the word, so a session that is paying for the compute every read
    /// says so instead of being inferred from the milliseconds.
    pub valuation_source: crate::temple::preset::ValuationSource,
    /// The advisor's ranking alone (`slice::advise_read`).
    pub advise: Duration,
    /// Projecting the slice and emitting it.
    pub publish: Duration,
}

fn ms(d: Duration) -> u128 {
    d.as_millis()
}

/// The line a cheap tick writes when it overran [`SLOW_TICK`], or `None`.
///
/// `said` is when this loop last wrote one; the line is rate-limited to one per
/// [`SLOW_TICK_LOG_EVERY`] so a machine that is slow on every tick does not
/// fill the log with the same fact. The stage breakdown is the point of the
/// line: a stall in `capture` (the game saturating the GPU as a fight starts)
/// and a slow `cheap detect` (a debug build, a huge screen) are different
/// problems with the same total.
///
/// Cheap ticks only — a tick that resolved the anchor writes
/// [`read_timings_line`] instead when it read, and its anchor cost is a price
/// the loop chose to pay.
pub fn slow_tick_line(
    said: &mut Option<Instant>,
    now: Instant,
    took: Duration,
    stages: &TickStages,
) -> Option<String> {
    if took <= SLOW_TICK {
        return None;
    }
    if let Some(last) = *said {
        if now.duration_since(last) < SLOW_TICK_LOG_EVERY {
            return None;
        }
    }
    *said = Some(now);
    Some(format!(
        "Temple: slow detect tick — {} ms (capture {} ms, cheap detect {} ms, anchor {} ms); the cadence is {} ms",
        ms(took),
        ms(stages.capture),
        ms(stages.cheap),
        ms(stages.anchor),
        ms(DETECT_INTERVAL),
    ))
}

/// The line every full read writes: each stage from the grab to the publish,
/// the total, the read's own `last_read_at` stamp so the overlay's
/// `[temple-overlay] board read at …` line can be matched to it, WHICH ROUND
/// this was and what it re-read, and whether the read was clean or is buying
/// another round.
///
/// # The valuation field (WI-3)
///
/// `valuation N ms (cached | computed)` sits where the single `advise` field
/// used to, and `advise N ms` follows it: the pair was one number until
/// POE-257 WI-3, and the owner's question — is the table cached? — could not be
/// answered from a log that added them together. The WORD carries the half the
/// milliseconds cannot: a hit and a fast machine's rebuild both round to a
/// small number, and only `computed` on every read says the cache is missing.
///
/// # The round (WI-2)
///
/// `round N of {RETRIES + 1}: full | re-read 3 plates, panel` — what the round
/// did, then what it found. It is the only place the partial-round cost is
/// visible: with the stage timings beside it, `text ocr` and `plates` on a
/// round that named neither are the measurement of what skipping them bought,
/// which is why docs/TEMPLE-LIFECYCLE.md quotes no figure of its own.
///
/// `N` is derived from `retries_left` AFTER [`LoopState::note_read`] has spent
/// this read's share, so it counts what actually happened rather than what the
/// plan intended: a first look leaves the whole [`RETRIES`] budget and is round
/// 1, and each retry spends one. A geometry-only re-read carries the budget
/// rather than spending it and therefore repeats its round number — which is
/// the honest answer, because `kept_for` dropped the kept reading and it read
/// everything again.
pub fn read_timings_line(
    tick: &TickStages,
    read: &ReadStages,
    total: Duration,
    read_at: u64,
    unclean: bool,
    retries_left: Option<u8>,
    plan: &slice::ReadPlan,
) -> String {
    let verdict = match (unclean, retries_left) {
        (false, _) => "clean".to_string(),
        (true, Some(n)) => format!("unclean, {n} retries left"),
        (true, None) => "unclean".to_string(),
    };
    let round = RETRIES + 1 - retries_left.unwrap_or(RETRIES).min(RETRIES);
    format!(
        "Temple: read timings — capture {} ms, cheap detect {} ms, anchor {} ms, text ocr {} ms, plates {} ms, markers {} ms, valuation {} ms ({}), advise {} ms, publish {} ms — {} ms from grab to publish, read at {}; round {} of {}: {}; {}",
        ms(tick.capture),
        ms(tick.cheap),
        ms(tick.anchor),
        ms(read.text_ocr),
        ms(read.plate_ocr),
        ms(read.markers),
        ms(read.valuation),
        read.valuation_source.word(),
        ms(read.advise),
        ms(read.publish),
        ms(total),
        read_at,
        round,
        RETRIES + 1,
        plan.describe(),
        verdict,
    )
}

/// Whether this tick finished the incursion's cycle: the sheet was READ, and it
/// has now gone (WI-1, owner 2026-09-07).
///
/// The rule the loop hands to [`trigger::ArmState::complete_cycle`], as a pure
/// function of the three facts the tick has — what the panel state machine just
/// answered, whether a read of the CURRENT board key is in hand
/// ([`LoopState::has_read`]), and whether this tick could look at the screen at
/// all. It lives here beside [`loop_step`] for the same reason: the loop's gates
/// belong in the tested surface, not in an `if` inside a function that needs a
/// screen.
///
/// [`DetectOutcome::Retired`] and not `Missed`: `Retired` is the transition —
/// the sheet was live and has now missed [`RETIRE_AFTER`] ticks in a row — while
/// `Missed` is the loop's resting state over an empty screen, and completing a
/// cycle on that would stand the loop down on the very tick that armed it, before
/// the player ever opened the sheet. Nor [`DetectOutcome::HeldMiss`]: one miss
/// over a live sheet is not a close (owner, 2026-09-11, POE-275), and this rule
/// needed no change for that beyond the constant.
///
/// `read_in_hand` is the guard the owner kept: a sheet closed BEFORE any read
/// completed does not end anything, because the incursion this arm was bought
/// for has not been read yet. That is the player who opened the sheet on a frame
/// the anchor missed, or closed it again before the read landed, and the answer
/// for them is that the loop keeps probing. A read that COMPLETED unclean is
/// still a read here, so a sheet closed while retry rounds are owed ends the
/// cycle and loses them — accepted, with Re-arm as the answer, and named as a
/// residual in `docs/TEMPLE-LIFECYCLE.md`.
///
/// `looked` is `false` for a tick that FAILED rather than missed ([`miss`]'s
/// `errored`): a screen grab that returned an error says nothing about whether
/// the sheet is still there, and a stand-down on a transient capture failure
/// would cost the rest of the incursion. It is a parameter rather than an `if`
/// at the call site so the failing-grab case is decided in the same tested
/// function as the other two — `miss` needs an `AppHandle` and is not reachable
/// from a unit test.
///
/// Since the WI-1 fix round it is the SECOND of two guards over the same fact
/// and no longer the only one: an errored tick folds through
/// [`LoopState::on_blind_tick`], which answers [`DetectOutcome::Blind`] and
/// cannot reach the `Retired` branch above. That change was made for `live`
/// rather than for this rule — one failed grab used to retire a panel that was
/// on screen — and this parameter is kept because the rule reads true on its
/// own: a tick that could not look completes nothing, whatever outcome it is
/// handed.
///
/// What is NOT decided here is whether the key this tick observed is still the
/// current one. That guard belongs to the writer, because the answer can change
/// between this call and the lock — see `trigger::complete_cycle`.
pub fn cycle_complete(outcome: DetectOutcome, read_in_hand: bool, looked: bool) -> bool {
    looked && matches!(outcome, DetectOutcome::Retired) && read_in_hand
}

/// What a CLEAN miss publishes, by what the panel state machine answered —
/// `None` is "publish nothing" (POE-275, owner 2026-09-11).
///
/// [`TickOutcome::NoPanel`] for [`DetectOutcome::Missed`], the loop searching
/// with nothing live, and for [`DetectOutcome::Retired`], the sheet gone —
/// `panel_not_visible` is not in the webview's `OVERLAY_VISIBLE_STATUSES`, so
/// that publish is what takes the sheet-bound overlays down. Nothing for
/// [`DetectOutcome::HeldMiss`]: one miss over a live sheet must not hide them,
/// so the status the last sighting wrote stays on the slice. A failed tick's
/// `error` and its message stand through a held miss the same way, for that
/// one tick; the sighting or the retire after it clears them.
///
/// The other three are not a clean miss's answer and publish nothing here: a
/// sighting publishes through [`tick`]'s own path, and a failed grab
/// ([`DetectOutcome::Blind`]) has already published through [`fail`].
///
/// A pure function for the reason [`cycle_complete`] is one: [`miss`] needs an
/// `AppHandle` and is not reachable from a unit test.
pub fn miss_publish(outcome: DetectOutcome) -> Option<TickOutcome> {
    match outcome {
        DetectOutcome::Missed | DetectOutcome::Retired => Some(TickOutcome::NoPanel),
        DetectOutcome::HeldMiss
        | DetectOutcome::Found
        | DetectOutcome::Held
        | DetectOutcome::Blind => None,
    }
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
/// does not turn into one publish and one log line per tick.
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
/// has started (or stopped) looking — and it covers the three transitions no
/// Client.txt line announces at all: [`super::trigger::MANUAL_ARM_GRACE_MS`]
/// running out, (POE-246) the layout panel going off screen, and (WI-1) the
/// cycle completing.
///
/// # Why the source and not the publish
///
/// Keyed on [`trigger::ArmSource`] rather than on the publish
/// [`gate_announcement`] asks for, which POE-246 changed for two reasons. A gate
/// that stays open while the REASON changes hands says so — a Re-arm's grace
/// expiring under a panel that is still on screen is that shape, and it is
/// invisible in an armed-ness bit. And the re-assertion that corrects a foreign
/// status write (POE-171 finding 15) stops putting a second `stood down` line in
/// `app.log` for one stand-down: the publish is the correction, the line never
/// was.
///
/// # The stand-down names its CAUSE (WI-1, 2026-09-07)
///
/// Four things shut the gate now, and they are four different things for a smoke
/// run to check, so `stood_down` rides on the line. The vocabulary is
/// [`trigger::StandDown`]'s, which is also what writes it — this only prints —
/// and the resting case keeps POE-242's exact wording because that is the string
/// `docs/OVERLAY-GUIDE.md` smoke item 12 tells the runner to look for.
///
/// It is read on the `None` arm only. An armed gate names its SOURCE, and what
/// last shut the gate says nothing about why it is open.
///
/// `said` starts `None`, which is the same claim this line's `None` arm makes —
/// a loop that has said nothing has not started looking — so the first source it
/// does find is always announced.
fn gate_line(
    said: &mut Option<trigger::ArmSource>,
    source: Option<trigger::ArmSource>,
    stood_down: trigger::StandDown,
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
        None => format!(
            "Temple: capture stood down — {} (Re-arm forces a read)",
            stood_down.label()
        ),
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
    /// The tick ran clean, a panel anchored, and it is a board this loop has
    /// already read (POE-249, docs/TEMPLE-LIFECYCLE.md row 3).
    ///
    /// The sheet was closed and reopened inside one incursion, so the board
    /// behind it cannot have changed. The payload is the status the read's own
    /// projection wrote ([`BoardRead::status`]) and it is the ONLY thing this
    /// puts back — the layout, the panel, the advice and the timestamps on the
    /// slice are the ones that read published and are still current.
    ///
    /// The distinction from [`Self::Anchored`] is what it costs: `Anchored`
    /// announces a read that is about to run and is overwritten by it a second
    /// later, this one is the whole tick.
    Reshown(TempleStatus),
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

/// The status one loop event leaves behind, and what else it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusUpdate {
    pub status: TempleStatus,
    /// `true` when the event means the last error is over.
    pub clear_error: bool,
    /// `true` when the event means the module is no longer waiting for the
    /// temple sheet ([`slice::TempleSlice::waiting_for_panel`], POE-249).
    ///
    /// Two different reasons, both ending in the same write. A sighting is the
    /// wait being ANSWERED — the sheet is on screen — and BOTH sightings count:
    /// [`TickOutcome::Anchored`] is one whose read publishes the board, and
    /// [`TickOutcome::Reshown`] is one whose board was already published. What
    /// takes the notice down is the sheet being there, not the reading of it.
    /// A stand-down or a shutdown ([`TickOutcome::Disarmed`],
    /// [`TickOutcome::Stopping`]) is the loop no longer looking at all, and a
    /// notice that says "waiting for the temple panel" over a loop that is not
    /// looking for one is a lie on screen.
    ///
    /// The three that leave it alone are the ones where the wait is still the
    /// truth: [`TickOutcome::NoPanel`] is a tick that looked and saw nothing,
    /// [`TickOutcome::Failed`] is a tick that could not look this once, and
    /// [`TickOutcome::Armed`] is the gate opening — which is what a START line
    /// buys, and clearing there would take the notice down in the same second
    /// it went up.
    pub clear_waiting: bool,
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
///
/// The early return carries `clear_waiting: false` for the same reason it
/// carries `clear_error: false`: an unavailable slice is not moved by a tick
/// result at all. The wait such a slice may still be carrying is not left
/// standing, though — [`unavailable`] ends the cycle itself, in the same
/// publish that writes the status, because that publish is the last one its
/// thread makes.
pub fn next_status(prev: TempleStatus, outcome: TickOutcome) -> StatusUpdate {
    if prev == TempleStatus::Unavailable {
        return StatusUpdate {
            status: TempleStatus::Unavailable,
            clear_error: false,
            clear_waiting: false,
        };
    }
    let (status, clear_error) = match outcome {
        TickOutcome::Failed => (TempleStatus::Error, false),
        TickOutcome::NoPanel => (TempleStatus::PanelNotVisible, true),
        TickOutcome::Anchored => (TempleStatus::Reading, true),
        // The status its own read wrote, put back unchanged. `Reading` would be
        // a lie — nothing is being read — and re-deriving `Read` here would
        // publish `read` over a board that was projected as `no_current_room`.
        TickOutcome::Reshown(status) => (status, true),
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
        clear_waiting: matches!(
            outcome,
            TickOutcome::Anchored
                | TickOutcome::Reshown(_)
                | TickOutcome::Disarmed
                | TickOutcome::Stopping
        ),
    }
}

/// Fold one loop event into the slice — the only writer of
/// [`TempleSlice::status`] outside [`slice::project`].
pub fn apply_status(slice: &mut TempleSlice, outcome: TickOutcome) {
    let update = next_status(slice.status, outcome);
    slice.status = update.status;
    // No outcome is a retry round in flight; [`apply_anchored`] sets it after
    // this for the one that can be (POE-276).
    slice.read_retry = false;
    if update.clear_error {
        slice.last_error = None;
    }
    if update.clear_waiting {
        slice::end_cycle(slice);
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
        slice::RoiView {
            kind: slice::PANEL_REGION.to_string(),
            of: None,
            rect: panel_rect(origin, scale),
        },
        slice::RoiView { kind: "diamond".to_string(), of: None, rect: diamond_rect(origin, scale) },
        slice::RoiView {
            kind: slice::REMAINING_REGION.to_string(),
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

/// The scale hint this capture's screen slice supplies (POE-234 WI-2).
///
/// The shared slice is the app's one store of the screen's UI scale. This
/// converts it into the temple unit through [`anchor::scale_for_ui_scale`]. The
/// placed origin is added separately by [`cheap_hint_from_screen`]; this helper
/// remains the scale-only geometry validation seam used by commands and tests.
///
/// # Four sources can be behind the number, and one of them drifts
///
/// `None` — no scale hint, and the caller is uncalibrated — in these cases:
///
/// - nothing has measured a screen (fresh install, or the tick right after
///   `ssot::geometry_recalibrate`);
/// - the stored measurement is not of THIS capture. The rule is
///   `ssot::screen_matches`', reused rather than restated so the temple cannot
///   grow its own opinion of what "the same screen" means — in particular the
///   POE-237 one about `monitor_id == 0` being UNKNOWN and never compared as an
///   identity. In the capture loop this branch is nearly unreachable, because
///   `ssot::drop_if_mismatched` runs first on the same pixels and empties the
///   slot; `super::commands::temple_debug_capture` is the caller that can be
///   handed an image file of any size;
/// - the stored `ui_scale` cannot describe a screen. `settings::ScreenScaleSetting::is_sane`
///   refuses those at load and both writers measure rather than invent, so this
///   is the conversion being a total function rather than a claim that a zero
///   is reachable — the cost of being sure is one comparison, and the cost of
///   being wrong is a zero-size template.
pub fn hint_for_capture(
    screen: Option<&crate::ssot::ScreenSlice>,
    capture: (u32, u32),
    monitor_id: u32,
    client: [i32; 4],
) -> Option<anchor::AnchorCalibration> {
    let screen = screen?;
    if !crate::ssot::screen_matches(&Some(*screen), capture, monitor_id, client) {
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

/// Build the cheap recheck hint from the current screen slice.
///
/// `ssot::placements` owns the Entrance origin, including any origin the
/// fallback has remembered. The scale crosses units only through
/// [`anchor::scale_for_ui_scale`]. The value is rebuilt for each capture and is
/// never taken from the prior read.
fn cheap_hint_from_screen(
    screen: Option<&crate::ssot::ScreenSlice>,
    capture: (u32, u32),
    monitor_id: u32,
    client: [i32; 4],
) -> Option<CheapHint> {
    let screen = screen?;
    let calibration = hint_for_capture(Some(screen), capture, monitor_id, client)?;
    let origin = crate::ssot::placements(screen).temple?.entrance_origin;
    Some(CheapHint { calibration, origin })
}

/// The hint the loop should use for this capture, plus whether a screen slice
/// existed, read under the slice's own lock and dropped before any image work.
///
/// Lock-then-drop, like every other reader of an `AppState` mutex on this
/// thread: the anchor search that follows takes seconds, and holding the screen
/// slot across it would block `ssot::publish_screen` on the merc thread for all
/// of them.
fn hint_from_slice(
    app: &AppHandle,
    capture: (u32, u32),
    monitor_id: u32,
    client: [i32; 4],
) -> (
    Option<CheapHint>,
    Option<crate::ssot::ScreenScaleSource>,
    bool,
    Option<[i32; 2]>,
) {
    let screen = {
        let state = app.state::<AppState>();
        let slot = state.screen.lock().unwrap_or_else(|e| e.into_inner());
        *slot
    };
    let hint = cheap_hint_from_screen(screen.as_ref(), capture, monitor_id, client);
    // The ANCHORED origin, which is a different question from the hint's
    // origin: `ssot::placements` answers the latter with a SEED for any
    // non-null slice, so the hint alone cannot tell "a plate was found here"
    // from "arithmetic put the centre here". `cold_sweep_reason` needs the
    // first — see the note on its `placed_origin`.
    let anchored = screen
        .and_then(|screen| screen.anchors)
        .and_then(|anchors| anchors.temple_entrance);
    (hint, screen.map(|s| s.source), screen.is_some(), anchored)
}

/// Why one explicit cold fallback is allowed for a failed placed recheck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColdSweepReason {
    /// No screen scale or anchored placement exists yet.
    NullSlice,
    /// Re-arm announced this board and its placed recheck fell below the floor.
    PlacedMiss,
}

/// A cold sweep [`cold_sweep_reason`] has just started: why, and — for a null
/// slice — which of the key's [`NULL_SWEEP_CAP`] attempts it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SweepStart {
    reason: ColdSweepReason,
    /// `Some(k)`, 1-based, for [`ColdSweepReason::NullSlice`]; `None` for the
    /// placed miss, whose budget is one per key and needs no count.
    attempt: Option<u8>,
}

/// Every per-key cold-sweep budget the loop keeps (POE-269, POE-275 WI-2).
///
/// One `Copy` value so [`cold_sweep_reason`] can decide AND record a start in
/// one call the tests reach without an `AppHandle`: a sweep is charged when it
/// STARTS, whatever it later finds, so a start and its charge cannot drift
/// apart.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct SweepBudget {
    /// The board key for which the one placed-miss cold fallback was attempted.
    fallback_sweep_key: Option<(u64, u64)>,
    /// The key the two null-slice counters below belong to. A different key
    /// resets both ([`Self::null_miss`]).
    null_key: Option<(u64, u64)>,
    /// Consecutive clean misses on a null slice since the last sweep started, a
    /// sighting, or the key began — toward [`NULL_SWEEP_EVERY`].
    null_misses: u8,
    /// Null-slice sweeps started under `null_key` — toward [`NULL_SWEEP_CAP`].
    null_started: u8,
    /// The key whose null sweeps POE-269's withheld rule has ENDED
    /// ([`null_sweep_key_after_publish`]): a second found-but-withheld sweep, or
    /// one that filled the slice. Nothing but a key change reopens it.
    null_ended: Option<(u64, u64)>,
    /// The null-slice key that already spent its one withheld-anchor retry.
    null_sweep_released: Option<(u64, u64)>,
}

impl SweepBudget {
    /// One clean miss on a null or unplaced slice, with no sweep in flight:
    /// `Some(k)` when it starts the key's `k`-th null sweep, `None` otherwise.
    ///
    /// The count restarts at the start it answers, so the next sweep needs
    /// [`NULL_SWEEP_EVERY`] misses after this one — and since the caller asks
    /// only while nothing is in flight, after this one ENDS.
    fn null_miss(&mut self, key: (u64, u64)) -> Option<u8> {
        if self.null_key != Some(key) {
            self.null_key = Some(key);
            self.null_misses = 0;
            self.null_started = 0;
        }
        if self.null_ended == Some(key) || self.null_started >= NULL_SWEEP_CAP {
            return None;
        }
        self.null_misses = self.null_misses.saturating_add(1);
        if self.null_misses < NULL_SWEEP_EVERY {
            return None;
        }
        self.null_misses = 0;
        self.null_started += 1;
        Some(self.null_started)
    }

    /// A tick that anchored: the misses are no longer consecutive.
    fn on_sighting(&mut self) {
        self.null_misses = 0;
    }

    /// A found null-slice sweep of `key` has been confirmed and gone to
    /// [`publish_anchor_scale`], which answered `screen_filled` — POE-269's
    /// withheld rule, [`null_sweep_key_after_publish`].
    fn after_null_publish(&mut self, key: (u64, u64), screen_filled: bool) {
        (self.null_ended, self.null_sweep_released) =
            null_sweep_key_after_publish(key, self.null_sweep_released, screen_filled);
    }

    /// A placed recheck anchored under `key`. On a screen with no ANCHORED
    /// origin (`anchored` false) that ends the key's null sweeps (POE-275 WI-2
    /// fix round); on an anchored screen the null path is not in play and
    /// nothing changes. The seed it rechecked is where
    /// the sheet is, so a sweep under this key could only find what the recheck
    /// found — the same reasoning as the filled-slice rule in
    /// [`null_sweep_key_after_publish`]. Without it the cadence restarted after
    /// every close in the same key: up to [`NULL_SWEEP_CAP`] sheet-less sweeps
    /// across a Temple of Atzoatl run.
    fn on_recheck(&mut self, key: (u64, u64), anchored: bool) {
        if !anchored {
            self.null_ended = Some(key);
        }
    }
}

/// Decide whether this tick's miss starts a cold sweep, and charge the budget
/// for it when it does (POE-275 WI-2 — the start is what is charged, so a sweep
/// that finds nothing costs the same as one that finds the panel).
///
/// Only on [`DetectOutcome::Missed`] — the loop searching with nothing live —
/// and only while no sweep is in flight (`in_flight`): one sweep at a time.
/// **No sweep starts over a live panel.** A [`DetectOutcome::HeldMiss`] is a
/// tooltip or a misread over a sheet the recheck just verified, and the retire
/// after it is the sheet closing; neither is a reason to search the screen, and
/// neither counts toward [`NULL_SWEEP_EVERY`]. A failed grab
/// ([`DetectOutcome::Blind`]) never reaches here and would be refused if it
/// did.
///
/// A null or unplaced slice has no origin to verify, so any arm source may
/// sweep it — the start-up probe included, whose one tick is a first miss and
/// therefore buys none: a module switched on with the sheet already open stands
/// down unswept when there is no screen slice, or a seed that misses the sheet,
/// and Re-arm is the answer (docs/TEMPLE-LIFECYCLE.md's residual) — a right
/// seed is rechecked on the probe tick and reads the sheet. It sweeps on every
/// [`NULL_SWEEP_EVERY`]-th consecutive clean miss, up to [`NULL_SWEEP_CAP`] per
/// key, unless POE-269's withheld rule has ended the key
/// ([`SweepBudget::null_ended`]). Until 2026-09-11 it swept on the first miss,
/// once per key.
///
/// A placed slice gets one fallback per key, only under the Manual arm (Re-arm).
///
/// # `placed_origin` is the ANCHORED origin, never the hint's (POE-278)
///
/// "Placed" here means a plate was FOUND at this origin — `anchors.temple_entrance`
/// — and not merely that arithmetic proposed one. The two used to coincide by
/// accident: the only way to reach this with a non-null slice was for a module
/// to have measured the screen, and Recalibrate emptied the slot, so a press
/// reliably produced the `NullSlice` arm.
///
/// Recalibrate now leaves a capture-derived slice standing, and
/// `ssot::placements` answers `entrance_origin` with a SEED for any non-null
/// slice. Feeding that seed in here would read "a screen exists" as "the
/// placement has been verified" and drop the press straight into the
/// `PlacedMiss` arm, which fires only under a Manual arm — so a player who
/// pressed the button because their placement was wrong would get NO sweep on
/// an `AlvaStart` or `TempleArea` arm, which is the one case the button exists
/// to fix.
///
/// # Why AlvaStart and TempleArea buy no sweep (owner decision, 2026-09-09)
///
/// The first tick after Alva's start line runs before the player has opened
/// the sheet, so its placed recheck misses. Until 2026-09-09 that miss spent
/// the fallback: the pyramid sweep ran on a frame grabbed before the sheet
/// opened, no tick ran while it did, and the sheet was read only after it
/// returned. app.log 2026-09-09, debug build: armed 02:41:47, `sweep found no
/// layout panel` 02:42:18, `layout panel found` 02:42:19 — three incursions
/// that session, 30–34 s each. The placed origin is trusted instead: the game
/// draws the sheet in one place, so a miss under an incursion arm means "not
/// open yet", and the 650 ms recheck sees the sheet when it opens. Re-arm
/// keeps the sweep as the explicit "look again" for a placement that is wrong.
/// The rule stands now that the sweep no longer blocks the loop (owner,
/// 2026-09-11: *"keep that, no placed-miss sweep under AlvaStart/TempleArea"*).
fn cold_sweep_reason(
    screen_present: bool,
    placed_origin: Option<(i32, i32)>,
    outcome: DetectOutcome,
    in_flight: bool,
    source: Option<trigger::ArmSource>,
    key: (u64, u64),
    budget: &mut SweepBudget,
) -> Option<SweepStart> {
    if outcome != DetectOutcome::Missed || in_flight {
        return None;
    }
    if !screen_present || placed_origin.is_none() {
        return budget.null_miss(key).map(|attempt| SweepStart {
            reason: ColdSweepReason::NullSlice,
            attempt: Some(attempt),
        });
    }
    let manual = matches!(
        source,
        Some(trigger::ArmSource::Trigger(trigger::ArmReason::Manual))
    );
    if !manual || budget.fallback_sweep_key == Some(key) {
        return None;
    }
    budget.fallback_sweep_key = Some(key);
    Some(SweepStart { reason: ColdSweepReason::PlacedMiss, attempt: None })
}

/// The origin a successful fallback should remember, or `None` when the sweep
/// agrees with the placed origin inside the frame band.
///
/// A null slice has no competing placement, so the swept origin is still the
/// corroborated answer and must reach `remember_anchor` after a successful read.
fn placed_origin_contradiction(
    placed_origin: Option<(i32, i32)>,
    swept_origin: (i32, i32),
) -> Option<(i32, i32)> {
    match placed_origin {
        None => Some(swept_origin),
        Some(placed_origin)
            if placed_origin.0.abs_diff(swept_origin.0)
                > slice::FRAME_ORIGIN_TOLERANCE as u32
                || placed_origin.1.abs_diff(swept_origin.1)
                    > slice::FRAME_ORIGIN_TOLERANCE as u32 => Some(swept_origin),
        Some(_) => None,
    }
}

/// The contradiction line for a sweep that found the panel outside the placed
/// Entrance origin's tolerance band.
fn placed_origin_contradiction_line(
    placed_origin: Option<(i32, i32)>,
    swept_origin: (i32, i32),
) -> Option<String> {
    let placed_origin = placed_origin?;
    placed_origin_contradiction(Some(placed_origin), swept_origin).map(|_| {
        format!(
            "temple: placed origin ({},{}) contradicted by sweep ({},{})",
            placed_origin.0, placed_origin.1, swept_origin.0, swept_origin.1
        )
    })
}

/// What a FOUND null-slice sweep of `key` leaves of that key's null sweeps, once
/// its anchor has gone to [`publish_anchor_scale`]: `(ended, released)`, the
/// new [`SweepBudget::null_ended`] and [`SweepBudget::null_sweep_released`].
///
/// POE-269's rule, kept by POE-275 WI-2 on top of the [`NULL_SWEEP_EVERY`]
/// cadence. A found anchor whose proposed screen slice was withheld releases the
/// key for one retry — `ended` stays `None` and the cadence goes on — and
/// `released_key` records that the retry is used, so a second withheld result
/// ENDS the key's null sweeps until the `(temple_epoch, temple_rearm)` key
/// changes. A found anchor that filled the slice ends them too, as it always
/// did: until 2026-09-11 every null sweep spent its key and only this release
/// gave it back.
///
/// Asked only for a sweep that found the panel and was confirmed on a later
/// capture ([`Sighting::Swept`]); a sweep that found nothing is the cadence's
/// business alone. Until 2026-09-11 it was asked on every sighting with two
/// flags saying whether this was one.
fn null_sweep_key_after_publish(
    key: (u64, u64),
    released_key: Option<(u64, u64)>,
    screen_filled: bool,
) -> (Option<(u64, u64)>, Option<(u64, u64)>) {
    if !screen_filled && released_key != Some(key) {
        (None, Some(key))
    } else {
        (Some(key), released_key)
    }
}

/// The single placeholder for the future geometry notice task.
const GEOMETRY_NOTICE_LINE: &str =
    "Temple: geometry notice pending — placed origin contradicted by sweep (POE-271)";

/// Invoke anchor memory only at the end of a successful full read.
fn remember_fallback_anchor(
    fallback_origin: Option<(i32, i32)>,
    remember: impl FnOnce([i32; 2]),
) {
    if let Some((x, y)) = fallback_origin {
        remember([x, y]);
    }
}

// ------------------------------------------------ the sweep, off the loop --

/// Why a running sweep was let go before its answer was used (POE-275 WI-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SweepCancel {
    /// A placed recheck anchored while it searched; the recheck's origin wins.
    Recheck,
    /// `(temple_epoch, temple_rearm)` moved: a sweep belongs to the key it
    /// started under.
    KeyChange,
    /// The arm gate shut.
    StandDown,
    /// The module is stopping.
    Stop,
}

impl SweepCancel {
    /// The words [`sweep_line`] prints after `cancelled by`.
    fn label(self) -> &'static str {
        match self {
            SweepCancel::Recheck => "recheck",
            SweepCancel::KeyChange => "key change",
            SweepCancel::StandDown => "stand-down",
            SweepCancel::Stop => "stop",
        }
    }
}

/// What became of a sweep that FOUND the panel, judged on the capture after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FoundFate {
    /// A recheck at the swept origin and scale anchored on the current capture,
    /// and the tick reads there.
    Confirmed,
    /// It did not — the sheet closed, or the capture changed size, while the
    /// sweep searched. Discarded; the tick is a miss.
    Unconfirmed,
    /// The placed recheck anchored on the same tick. Its origin wins and this
    /// result is discarded, as a late one would be.
    Superseded,
}

impl FoundFate {
    /// The words [`sweep_line`] prints after the swept origin.
    fn label(self) -> &'static str {
        match self {
            FoundFate::Confirmed => "confirmed on the current frame",
            FoundFate::Unconfirmed => "not on the current frame, discarded",
            FoundFate::Superseded => "discarded, the placed recheck landed",
        }
    }
}

/// How one sweep ended — the outcome half of [`sweep_line`].
#[derive(Debug, Clone, Copy, PartialEq)]
enum SweepEnd {
    /// It found the panel at `origin` and `scale`, and `fate` is what the
    /// capture after it made of that.
    Found { origin: (i32, i32), scale: f32, fate: FoundFate },
    /// It searched the whole capture and found no layout panel.
    NoPanel,
    /// The loop let it go before using its answer.
    Cancelled(SweepCancel),
    /// Its thread ended without sending a result, which only a panic does.
    Lost,
}

/// Everything [`sweep_line`] prints about one sweep.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SweepReport {
    start: SweepStart,
    /// The size of the frame it searched.
    capture: (u32, u32),
    /// Measured on the sweep's own thread for a sweep that answered; from its
    /// launch to the moment the loop let it go for one that had not.
    took: Duration,
    end: SweepEnd,
}

/// What the sweep thread hands back: its anchor, if any, whether its stop
/// check had fired when the search returned, and how long the search took on
/// that thread.
struct SweepResult {
    found: Option<anchor::Anchor>,
    /// The search's own stop check, read after it returned.
    /// [`anchor::anchor_for_loop`] answers a stopped search with the same error
    /// as a miss, so without this a module stop would be logged as `found no
    /// layout panel`. [`SweepSlot::settle`] reports it as
    /// [`SweepCancel::Stop`], found or not.
    stopped: bool,
    took: Duration,
}

/// The one sweep in flight.
struct SweepFlight {
    start: SweepStart,
    /// The `(temple_epoch, temple_rearm)` key it started under.
    key: (u64, u64),
    /// The size of the frame it searches — the only capture size its origin
    /// means anything on ([`confirm_swept`]).
    capture: (u32, u32),
    launched: Instant,
    /// Set by the loop to cancel it. The search's stop closure reads it,
    /// together with the module's own `cancel`, between the pyramid's coarse
    /// correlations.
    stop: Arc<AtomicBool>,
    done: mpsc::Receiver<SweepResult>,
}

impl SweepFlight {
    fn report(&self, took: Duration, end: SweepEnd) -> SweepReport {
        SweepReport { start: self.start, capture: self.capture, took, end }
    }

    /// Set the stop flag and report the cancel, measured to now.
    fn cancelled(self, by: SweepCancel) -> SweepReport {
        self.stop.store(true, Ordering::SeqCst);
        // Its own duration when it had already answered: the answer is
        // discarded either way, and the measurement is still the sweep's.
        let took = match self.done.try_recv() {
            Ok(result) => result.took,
            Err(_) => self.launched.elapsed(),
        };
        self.report(took, SweepEnd::Cancelled(by))
    }
}

/// What this tick's placed recheck and the sweep slot, together, say it saw.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Sighting {
    /// The placed recheck anchored.
    Recheck(anchor::Anchor),
    /// A sweep found the panel and a recheck at its origin anchored on THIS
    /// tick's capture. The anchor is that recheck's, not the sweep's.
    Swept { anchor: anchor::Anchor, reason: ColdSweepReason },
    /// Neither: the tick is a miss.
    Nothing,
}

/// The cold sweep's one slot (POE-275 WI-2, owner 2026-09-11): at most one
/// sweep in flight, on a thread of its own, while the loop goes on ticking.
///
/// # Why the sweep may leave the loop's thread
///
/// The loop is a thread rather than a task because screen capture and
/// `Windows.Media.Ocr` are apartment-threaded (module doc).
/// [`anchor::anchor_for_loop`] is neither: it is image work over the frame it
/// is handed — a grayscale pyramid, template correlations, and the template
/// decoded once behind a `OnceLock` — with no capture, OCR or COM/WinRT call on
/// its path. So the sweep thread takes the frame by value and needs no
/// apartment.
///
/// # The loop never waits on it
///
/// Every method here returns at once. The loop polls the result once a tick
/// ([`Self::settle`]) and never joins the thread — not on a cancel and not on
/// module stop: a cancelled sweep stops at its next stop check, and any result
/// it still sends goes to a receiver that has been dropped. The checks are the
/// pyramid's, between its coarse correlations (see [`anchor::anchor_for_loop`]);
/// the stages before the pyramid — building the scene, the hint-scale search
/// and the three-scale table search — check nothing and run to completion, so a
/// cancel lands after them.
#[derive(Default)]
struct SweepSlot {
    flight: Option<SweepFlight>,
}

impl SweepSlot {
    fn in_flight(&self) -> bool {
        self.flight.is_some()
    }

    /// Run `search` on a thread of its own, handing it the per-sweep stop check.
    ///
    /// `search` is the whole sweep, injected so the tests control when it
    /// blocks and what it answers; the loop's is [`cold_sweep`]'s, which adds
    /// the module's `cancel` to the check. It answers `(found, stopped)`:
    /// `stopped` is its stop check read after it returned — see
    /// [`SweepResult::stopped`]. One flight at a time: the caller asks
    /// [`cold_sweep_reason`], which refuses while [`Self::in_flight`].
    fn launch<F>(
        &mut self,
        start: SweepStart,
        key: (u64, u64),
        capture: (u32, u32),
        search: F,
    ) -> std::io::Result<()>
    where
        F: FnOnce(&dyn Fn() -> bool) -> (Option<anchor::Anchor>, bool) + Send + 'static,
    {
        debug_assert!(self.flight.is_none(), "one sweep in flight at most");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let (sent, done) = mpsc::channel();
        std::thread::Builder::new().name("temple-sweep".to_string()).spawn(move || {
            let searching = Instant::now();
            let (found, stopped) = search(&|| flag.load(Ordering::SeqCst));
            // A failed send is a sweep the loop has let go of — the discard, not
            // an error.
            let _ = sent.send(SweepResult { found, stopped, took: searching.elapsed() });
        })?;
        self.flight = Some(SweepFlight {
            start,
            key,
            capture,
            launched: Instant::now(),
            stop,
            done,
        });
        Ok(())
    }

    /// Cancel the sweep in flight, if there is one, and report it.
    fn cancel(&mut self, by: SweepCancel) -> Option<SweepReport> {
        self.flight.take().map(|flight| flight.cancelled(by))
    }

    /// Cancel the sweep in flight when `key` is no longer the key it started
    /// under — its answer would be about a board the loop is no longer reading.
    fn keep_only(&mut self, key: (u64, u64)) -> Option<SweepReport> {
        match &self.flight {
            Some(flight) if flight.key != key => self.cancel(SweepCancel::KeyChange),
            _ => None,
        }
    }

    /// Fold this tick's placed recheck together with what the sweep in flight
    /// has produced, and report the sweep if this is where it ends.
    ///
    /// The recheck wins everything it takes part in: a sweep still searching is
    /// cancelled, and one that answered on this very tick is discarded. A sweep
    /// that FOUND the panel with no recheck beside it is handed to `confirm`
    /// with the capture size it searched, and only what `confirm` finds on THIS
    /// tick's frame is ever read — the loop passes [`confirm_swept`]. A sweep
    /// still searching with no recheck leaves the tick a miss and stays in
    /// flight.
    fn settle(
        &mut self,
        cheap: &anchor::CheapDetect,
        confirm: impl FnOnce(&anchor::Anchor, (u32, u32)) -> Option<anchor::Anchor>,
    ) -> (Sighting, Option<SweepReport>) {
        let recheck = match cheap {
            anchor::CheapDetect::Anchored(found) => Some(*found),
            anchor::CheapDetect::Nothing { .. } => None,
        };
        let seen = recheck.map_or(Sighting::Nothing, Sighting::Recheck);
        let Some(flight) = self.flight.take() else {
            return (seen, None);
        };
        let result = match flight.done.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                if recheck.is_some() {
                    return (seen, Some(flight.cancelled(SweepCancel::Recheck)));
                }
                self.flight = Some(flight);
                return (seen, None);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let took = flight.launched.elapsed();
                return (seen, Some(flight.report(took, SweepEnd::Lost)));
            }
        };
        // A search whose stop fired is a stopped search whatever it returned: a
        // miss from it is not evidence of an empty screen, and a find is not
        // used.
        if result.stopped {
            let end = SweepEnd::Cancelled(SweepCancel::Stop);
            return (seen, Some(flight.report(result.took, end)));
        }
        let Some(swept) = result.found else {
            return (seen, Some(flight.report(result.took, SweepEnd::NoPanel)));
        };
        let (sighting, fate) = match recheck {
            Some(found) => (Sighting::Recheck(found), FoundFate::Superseded),
            None => match confirm(&swept, flight.capture) {
                Some(anchor) => (
                    Sighting::Swept { anchor, reason: flight.start.reason },
                    FoundFate::Confirmed,
                ),
                None => (Sighting::Nothing, FoundFate::Unconfirmed),
            },
        };
        let end = SweepEnd::Found { origin: swept.origin, scale: swept.scale, fate };
        (sighting, Some(flight.report(result.took, end)))
    }
}

/// Re-find a swept anchor on a LATER capture: one windowed recheck at the swept
/// origin and scale, which is the cheap tick's own presence test
/// ([`anchor::detect_cheap`]). `None` when the sheet is no longer there.
///
/// The sweep searched a frame grabbed up to a whole sweep ago, and the sheet
/// can close in that time, so nothing is read at its origin until this frame
/// agrees (POE-275 WI-2). `capture` is the size of the frame the sweep
/// searched: a capture of another size is another screen, where the swept
/// origin means nothing, and `detect_cheap` refuses a hint whose size differs
/// from the image. The anchor returned is this frame's, re-centred inside the
/// recheck window, so the read is placed from the pixels it will crop.
fn confirm_swept(
    img: &DynamicImage,
    swept: &anchor::Anchor,
    capture: (u32, u32),
) -> Option<anchor::Anchor> {
    let hint = CheapHint {
        calibration: anchor::AnchorCalibration {
            screen_w: capture.0,
            screen_h: capture.1,
            scale: swept.scale,
        },
        origin: swept.origin,
    };
    match anchor::detect_cheap(img, Some(&hint)) {
        anchor::CheapDetect::Anchored(found) => Some(found),
        anchor::CheapDetect::Nothing { .. } => None,
    }
}

/// The build word [`sweep_line`] carries, from the caller's
/// `cfg!(debug_assertions)`: the same sweep is 5.3 s on a release build and
/// ~30 s on the PC's debug one, so a duration without it cannot be compared.
fn build_profile(debug_assertions: bool) -> &'static str {
    if debug_assertions {
        "debug"
    } else {
        "release"
    }
}

/// The one `app.log` line every cold sweep writes when it ends (POE-275 WI-2):
/// why it ran, which null-slice attempt of [`NULL_SWEEP_CAP`] it was, the
/// capture it searched, how long it took, how it ended, and the build.
///
/// A MEASUREMENT, never a switch — it is what [`NULL_SWEEP_EVERY`] and
/// [`NULL_SWEEP_CAP`] are to be corrected from, and the release-build duration
/// on the PC has never been recorded. It replaces the
/// `Temple: sweep found no layout panel at WxH — waiting for the panel` line,
/// whose case is the `found no layout panel` outcome here.
///
/// Not routed through [`ErrorLog`]: a sweep that finds nothing is not an error —
/// a screen with no layout panel on it is the state the loop lives in — and
/// [`cold_sweep_reason`]'s budget already bounds the lines: [`NULL_SWEEP_CAP`]
/// null sweeps plus one placed sweep per key.
fn sweep_line(report: &SweepReport, profile: &str) -> String {
    let attempt = match report.start.attempt {
        Some(k) => format!(", attempt {k} of {NULL_SWEEP_CAP}"),
        None => String::new(),
    };
    let end = match report.end {
        SweepEnd::Found { origin, scale, fate } => format!(
            "found at ({},{}) scale {scale:.3} — {}",
            origin.0,
            origin.1,
            fate.label()
        ),
        SweepEnd::NoPanel => "found no layout panel".to_string(),
        SweepEnd::Cancelled(by) => format!("cancelled by {}", by.label()),
        SweepEnd::Lost => "ended without a result".to_string(),
    };
    format!(
        "Temple: cold sweep ({:?}{attempt}) at {}x{} — {} ms, {end}; {profile} build",
        report.start.reason,
        report.capture.0,
        report.capture.1,
        ms(report.took),
    )
}

/// Write [`sweep_line`] for a sweep that just ended.
fn log_sweep(app: &AppHandle, report: &SweepReport) {
    crate::app_log(app, sweep_line(report, build_profile(cfg!(debug_assertions))));
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
/// at either gate and an unchanged value all stop here. Returns whether the
/// anchor produced a publishable screen slice; a withheld `k` measurement
/// returns false so a null-slice fallback can release its key for one retry. A
/// second withheld result keeps that key spent until the board key changes.
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
    client: [i32; 4],
) -> bool {
    let next = match screen_from_anchor(layout.scale, hint, capture, monitor_id, origin, client, now_ms())
    {
        Ok(next) => next,
        Err(line) => {
            // Once per distinct line, not once per tick: the condition holds for
            // as long as the panel is on screen at that scale, and this loop
            // ticks every `DETECT_INTERVAL`.
            if session.k_said.as_deref() != Some(line.as_str()) {
                session.k_said = Some(line.clone());
                crate::app_log(app, line);
            }
            return false;
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
    // not, and the deadband inside `changed` is what keeps a 650 ms loop off it.
    if crate::ssot::should_remember_screen(record.changed, next.source) {
        crate::persist_settings(app);
    }
    true
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
    client: [i32; 4],
    measured_at_ms: u64,
) -> Result<crate::ssot::ScreenSlice, String> {
    let withheld = match hint {
        Some(hint) => hint_disagreement_line(scale, hint),
        None => unit_ratio_line(scale, capture.0, capture.1),
    };
    match withheld {
        Some(line) => Err(line),
        None => Ok(anchored_screen(scale, capture, monitor_id, origin, client, measured_at_ms)),
    }
}

/// The line to log when an anchor disagrees with the hint it was searched
/// against, or `None` when the two corroborate each other.
///
/// One [`anchor::SCALE_STEP`] is the finest disagreement this module's scale grid
/// can express. Inside it the anchor confirms the standing measurement; outside
/// it, the anchor came from elsewhere — the table row or an explicit fallback —
/// and the temple does not overrule a measurement of this screen with a board it
/// read (see [`screen_from_anchor`]'s second section).
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
    client: [i32; 4],
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
        client,
        anchors: None,
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

/// The board key as of NOW — `(temple_epoch, temple_rearm)`, in that order.
///
/// One place the pair is built, so the two readers cannot disagree about which
/// counter is which half: [`tick`] takes it once at the top for the whole
/// iteration, and `super::trigger::complete_cycle` takes it again when the
/// stand-down actually lands, to check that the tick's key is still the current
/// one. See [`BoardRead`] for what the pair means.
pub(super) fn board_key(app: &AppHandle) -> (u64, u64) {
    (temple_epoch(app), rearm_counter(app))
}

/// The incursion cycle this tick belongs to (POE-249 WI-1).
///
/// Bumped by `trigger::on_client_line` on an Alva line or a non-temple area
/// line — the two events that end a cycle. Read alongside [`rearm_counter`] to
/// form the board key: see [`BoardRead`] for why those two together are what
/// "is this a board I have already read?" means.
fn temple_epoch(app: &AppHandle) -> u64 {
    let state = app.state::<AppState>();
    let epoch = state.temple_epoch.load(std::sync::atomic::Ordering::SeqCst);
    epoch
}

/// The loop's "have I already logged this?" filter.
///
/// A failure path re-runs on every tick and an error carrying a varying number
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
    /// What the current tick has spent so far — see [`TickStages`].
    tick_stages: TickStages,
    /// When [`slow_tick_line`] last wrote, for its rate limit.
    slow_tick_said: Option<Instant>,
    gate: slice::RearmGate,
    errors: ErrorLog,
    /// The last completed read of the board [`LoopState::board`] is keeping, so
    /// a retry can be merged into it rather than replacing it (POE-249).
    ///
    /// Here rather than on [`LoopState`] because [`LoopState`] is `Eq` cheap
    /// bookkeeping and this is three OCR results and a door set. Dropped by
    /// [`kept_for`] the moment the board moves — two reads of two different
    /// boards must never reach [`slice::merge_reads`].
    kept: Option<slice::KeptRead>,
    /// What each board key may still spend on cold sweeps — the placed-miss
    /// fallback, the null-slice cadence and cap, and POE-269's withheld rule.
    /// See [`SweepBudget`]; [`cold_sweep_reason`] charges it.
    sweep_budget: SweepBudget,
    /// The one sweep in flight, off this thread (POE-275 WI-2) — [`SweepSlot`].
    sweeps: SweepSlot,
    /// Whether the first successful placed-origin recheck has been measured and
    /// logged.
    placed_recheck_said: bool,
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
    /// than one per tick. See [`hint_line`], which owns the rule.
    hint_said: Option<anchor::AnchorCalibration>,
    /// The last `k` disagreement announced, for the same reason: the condition
    /// holds for as long as the panel is on screen at that scale, and
    /// [`publish_anchor_scale`] is reached on every anchored tick.
    k_said: Option<String>,
    /// The last [`rois_line`] announced, for the same reason: `app_log` keeps
    /// 50 entries and the rects are a function of `(origin, scale)` alone, so a
    /// line per READ would repeat one value up to [`RETRIES`] + 1 times per
    /// board and once more for every incursion of every map, while saying
    /// nothing a reader did not already have.
    rois_said: Option<String>,
    /// The outside-set [`clipped_roi_announcement`] last announced, `None`
    /// before the loop has looked. Its own memory rather than a message in
    /// [`ErrorLog`] — see that function for why a rect-keyed, session-capped
    /// seam is the wrong shape for a condition a mouse drag re-states every
    /// tick.
    clipped_said: Option<Vec<&'static str>>,
}

fn run_loop(app: AppHandle, cancel: watch::Receiver<bool>) {
    crate::app_log(&app, "Temple: capture loop started".to_string());
    crate::report_ocr_engine(&app);

    // The user's settings belong on the slice from the first frame — the page
    // and the overlay render their own controls from them, and a derive-default
    // flag would read as a setting the user chose rather than "not loaded yet".
    let settings = settings_snapshot(&app);
    publish(&app, |slice| {
        slice.status = TempleStatus::Idle;
        slice.config = settings.config.clone();
        slice.profile = settings.profile.clone();
        slice.last_error = None;
    });

    if let Err(e) = crate::ocr::engine_ready() {
        return unavailable(&app, &cancel, e);
    }

    let mut session = Session {
        state: LoopState::default(),
        tick_stages: TickStages::default(),
        slow_tick_said: None,
        gate: slice::RearmGate::default(),
        errors: ErrorLog::default(),
        kept: None,
        sweep_budget: SweepBudget::default(),
        sweeps: SweepSlot::default(),
        placed_recheck_said: false,
        gate_said: None,
        source_said: None,
        hint_said: None,
        k_said: None,
        rois_said: None,
        clipped_said: None,
    };
    // Backdated so the first iteration ticks immediately rather than after a
    // full cadence of doing nothing.
    let mut last_detect = Instant::now() - DETECT_INTERVAL;

    loop {
        if *cancel.borrow() {
            break;
        }

        // No capture while alt-tabbed: the layout panel is not on screen, and a
        // full-screen anchor match every 650 ms would be pure heat. The arm
        // gate (POE-242, POE-246) is asked only behind it, for two reasons: the
        // loop publishes nothing at all while the game is not in front (see
        // `TempleStatus::Idle`), and an unfocused iteration does the same thing
        // either way. There is no panel clock for the wait to outlast since WI-1
        // (2026-09-07): `LoopState::live` is a bool the ticks write, so an
        // alt-tab of any length — a second or an hour — leaves the gate open for
        // `RETIRE_AFTER` ticks on the way back (two since 2026-09-11, POE-275),
        // and the last of them is the one that retires the sheet.
        let focused = game_focused(&app);
        // One read of the arm for both answers: the SOURCE that keeps the gate
        // open, and the word for it if it is shut. Two reads could disagree —
        // the watcher writes this lock from another thread — and the line would
        // then name a cause the gate did not close for.
        let (source, stood_down) = if focused {
            let arm = trigger::arm_state(&app);
            (
                trigger::arm_source(
                    arm,
                    session.state.live,
                    session.state.probe_pending(),
                    now_ms(),
                ),
                arm.stood_down,
            )
        } else {
            (None, trigger::StandDown::default())
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
            if let Some(line) = gate_line(&mut session.source_said, source, stood_down) {
                crate::app_log(&app, line);
            }
            // A shut gate cancels a sweep still searching (POE-275 WI-2): the
            // loop stops looking, so nothing would read its answer. AFTER the
            // stand-down line, so `app.log` names the cause first. Focused only:
            // an alt-tab is not a stand-down, and a sweep that finishes while the
            // game is behind is confirmed on the first capture after it.
            if !armed {
                if let Some(report) = session.sweeps.cancel(SweepCancel::StandDown) {
                    log_sweep(&app, &report);
                }
            }
        }

        let step = loop_step(
            focused,
            armed,
            last_detect.elapsed() >= DETECT_INTERVAL,
        );
        // A `match` and not an `if`, so a fifth [`LoopStep`] cannot be added
        // without deciding here whether it captures.
        match step {
            LoopStep::Detect => {
                let started = Instant::now();
                let promoted = tick(&app, &mut session, &cancel, source);
                last_detect = Instant::now();
                // Measured, never acted on — see `SLOW_TICK`. A promoted tick
                // that read wrote its own `read_timings_line` inside `tick`.
                if !promoted {
                    if let Some(line) = slow_tick_line(
                        &mut session.slow_tick_said,
                        last_detect,
                        started.elapsed(),
                        &session.tick_stages,
                    ) {
                        crate::app_log(&app, line);
                    }
                }
            }
            // Nothing to do but wait: `step.nap()` below is the whole of it.
            LoopStep::UnfocusedNap | LoopStep::DisarmedNap | LoopStep::Quantum => {}
        }

        if !nap(&cancel, step.nap()) {
            break;
        }
    }

    // A sweep still searching is let go, never joined (POE-275 WI-2): its stop
    // check reads the module's `cancel` as well as its own flag, so its thread
    // ends at its next stop check whatever this loop does — after the
    // hint/table stage, then between the pyramid's coarse correlations.
    if let Some(report) = session.sweeps.cancel(SweepCancel::Stop) {
        log_sweep(&app, &report);
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
        // This is the LAST publish this thread makes, so a wait a START line
        // put up stands forever unless it comes down here. On a host with no
        // capture the notice would otherwise sit under the unavailable message
        // for the rest of the session, claiming the module is looking for a
        // panel it cannot see.
        slice::end_cycle(slice);
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
/// anything is there, resolve the anchor when it says so, and read the board
/// when that board has not been read yet (POE-249's two gates — see the module
/// note and docs/TEMPLE-LIFECYCLE.md rows 2-3).
///
/// Returns whether this tick paid to resolve the anchor — the caller writes
/// [`slow_tick_line`] only for the ticks that did not, because a promoted tick
/// is not evidence about what the cheap half costs, and a promoted tick that
/// READ writes [`read_timings_line`] of its own. A tick that resolved an anchor
/// and then re-showed an already-read board is a promoted tick by that measure:
/// it paid for the placed-origin check, or for confirming a swept origin, either
/// way. A cold sweep is not part of any tick since 2026-09-11 (POE-275 WI-2):
/// it runs on its own thread ([`SweepSlot`]), and a tick only starts one,
/// polls it, or confirms what it found.
fn tick(
    app: &AppHandle,
    session: &mut Session,
    cancel: &watch::Receiver<bool>,
    source: Option<trigger::ArmSource>,
) -> bool {
    // The measurements start at the grab (`TickStages`); every stage below
    // writes through to the session as it finishes.
    let grabbed_at = Instant::now();
    session.tick_stages = TickStages::default();
    // The board key (POE-249). Read once, at the top, so all THREE consumers see
    // the same reading of the same two counters: the anchor gate's `rearm`, the
    // OCR gate's key, and — since WI-1 — the cycle rule in `miss`, which is
    // reached from the failing-grab arm below and so cannot wait for the cheap
    // half. A bump seen by one and not another would force a read the next gate
    // then skipped, or stand the loop down on a board the user had just asked to
    // be re-read.
    //
    // What ONE reading cannot do is stay current: a START line or a Re-arm that
    // lands while this tick runs arms a fresh cycle. Since 2026-09-11 (POE-275
    // WI-2) the cold sweep that made a tick last 5.3 s — ~30 s on the PC's debug
    // build — runs off this thread, but a tick still spans its grab and its
    // detect before it reaches `miss`: the grab measured 225 ms on the laptop's
    // debug build and the detect 1042 ms on the PC's (docs/TEMPLE-LIFECYCLE.md,
    // "Cadences and budgets"). `trigger::complete_cycle` therefore re-reads the pair and
    // refuses a stand-down whose key has moved on — see `miss`.
    let key = board_key(app);
    let rearm = key.1;
    // A sweep belongs to the key it started under (POE-275 WI-2): after a Re-arm
    // or a new epoch its answer is about a board the loop is no longer reading.
    if let Some(report) = session.sweeps.keep_only(key) {
        log_sweep(app, &report);
    }
    let grab = match crate::capture::capture_screen(app) {
        Ok(grab) => grab,
        Err(e) => {
            // The failing grab is what the tick SPENT, and a slow-tick line that
            // attributes 0 ms to `capture` after a 650 ms stall points the reader
            // at the wrong stage.
            session.tick_stages.capture = grabbed_at.elapsed();
            fail(app, session, format!("Temple: screen capture failed — {e}"));
            // Through `miss`, which spends the start-up probe like any other
            // tick (POE-246): a machine whose capture never succeeds must not
            // hold the gate open for the session.
            miss(app, session, true, key);
            return false;
        }
    };
    session.tick_stages.capture = grabbed_at.elapsed();
    let monitor_id = grab.monitor_id;
    let origin = grab.origin;
    let client = grab.client;
    let img = grab.image;
    let capture = (img.width(), img.height());
    // Before ANY remembered geometry is read (POE-227): a screen scale measured
    // on another monitor or at another resolution is dropped from the shared
    // slice on the first capture that disagrees with it. The placed hint below is
    // derived only after this prune.
    crate::ssot::drop_if_mismatched(app, capture, monitor_id, client);

    let settings = settings_snapshot(app);
    // The screen slice is the sole source for both halves of the cheap hint:
    // placements owns the Entrance origin and the scale crosses units through
    // anchor::scale_for_ui_scale. The value is local to this tick; no prior
    // read can supply an origin.
    let (cheap_hint, hint_source, screen_present, anchored_origin) =
        hint_from_slice(app, capture, monitor_id, client);
    let hint = cheap_hint.map(|hint| hint.calibration);
    if let Some(line) = hint_line(&mut session.hint_said, hint, hint_source) {
        crate::app_log(app, line);
    }

    // One full-resolution windowed NCC at the placed origin is both the panel
    // presence test and the steady-state detect path.
    let probing = Instant::now();
    let cheap = anchor::detect_cheap(&img, cheap_hint.as_ref());
    session.tick_stages.cheap = probing.elapsed();
    if !session.placed_recheck_said
        && matches!(cheap, anchor::CheapDetect::Anchored(_))
    {
        session.placed_recheck_said = true;
        crate::app_log(
            app,
            format!(
                "Temple: placed-origin recheck — NCC {:.3}, {} ms",
                cheap.ncc(),
                ms(session.tick_stages.cheap)
            ),
        );
    }

    // The recheck and the sweep in flight, folded (POE-275 WI-2): a recheck
    // that anchored wins and cancels the sweep; a sweep that FOUND the panel is
    // used only as re-found on THIS frame (`confirm_swept`), which is always a
    // later capture than the one it searched.
    //
    // The confirmation is a windowed NCC of its own, so its cost is booked to
    // the `anchor` stage — the stage a slow-tick line reads it from.
    let mut confirming = Duration::ZERO;
    let (sighting, ended) = session.sweeps.settle(&cheap, |swept, searched| {
        let started = Instant::now();
        let confirmed = confirm_swept(&img, swept, searched);
        confirming = started.elapsed();
        confirmed
    });
    session.tick_stages.anchor = confirming;
    if let Some(report) = ended {
        log_sweep(app, &report);
    }

    let (found, swept_by) = match sighting {
        Sighting::Recheck(found) => {
            // The seed was right: a screen with no anchored origin whose placed
            // recheck anchored has nothing left for a null sweep to find in this
            // key (`SweepBudget::on_recheck`).
            session.sweep_budget.on_recheck(key, anchored_origin.is_some());
            (found, None)
        }
        Sighting::Swept { anchor, reason } => (anchor, Some(reason)),
        Sighting::Nothing => {
            let outcome = miss(app, session, false, key);
            // A miss may START a sweep, never wait for one: the loop keeps
            // ticking while it searches. A null/unplaced slice sweeps every
            // `NULL_SWEEP_EVERY` misses up to `NULL_SWEEP_CAP` per key; a placed
            // miss once per key under the Manual arm only — see
            // `cold_sweep_reason`.
            //
            // The ANCHORED origin, not the hint's: the hint carries a seed on any
            // non-null slice, and passing that would read every screen that
            // merely EXISTS as one whose placement has been verified (POE-278).
            let start = cold_sweep_reason(
                screen_present,
                anchored_origin.map(|[x, y]| (x, y)),
                outcome,
                session.sweeps.in_flight(),
                source,
                key,
                &mut session.sweep_budget,
            );
            if let Some(start) = start {
                cold_sweep(app, &mut session.sweeps, img, hint, cancel, start, key);
            }
            return false;
        }
    };

    let placed_origin = cheap_hint.as_ref().map(|hint| hint.origin);
    let fallback_origin = swept_by.and_then(|_| {
        let fallback_origin = placed_origin_contradiction(placed_origin, found.origin)?;
        if let Some(line) = placed_origin_contradiction_line(placed_origin, found.origin) {
            crate::app_log(app, line);
            crate::app_log(app, GEOMETRY_NOTICE_LINE.to_string());
        }
        Some(fallback_origin)
    });

    let anchoring = Instant::now();
    let layout = reader::read_layout_at(&img, found);
    session.tick_stages.anchor += anchoring.elapsed();

    // The sighting the arm gate reads (POE-246, WI-1): every anchored tick sets
    // `live`, so the gate stays open for as long as the sheet is in front of the
    // player and shuts on the tick that retires it (`RETIRE_AFTER` misses).
    //
    // BEFORE the OCR gate below, and that order is load-bearing (POE-249): a
    // sighting that re-shows an already-read board is still a sighting, and it
    // is what keeps the loop armed while the player reads the sheet. Gating this
    // on the read would stand the module down after the READ rather than after
    // the sheet closed.
    let detected = session.state.on_detect(true);
    session.sweep_budget.on_sighting();
    // The temple's WRITE of the shared slice (POE-234 WI-2). Here rather than in
    // `full_read`, so all three anchor paths — the cheap tick's verified hint,
    // the cold sweep, and the promoted read — publish, including the ticks whose
    // board looked unchanged and bought no read. `ssot::accepts` is what makes
    // that affordable on a 650 ms loop.
    let screen_filled =
        publish_anchor_scale(app, session, &layout, hint, capture, monitor_id, origin, client);
    if swept_by == Some(ColdSweepReason::NullSlice) {
        session.sweep_budget.after_null_publish(key, screen_filled);
    }

    // The OCR gate (POE-249, docs/TEMPLE-LIFECYCLE.md rows 2-3). Everything
    // above this line runs on every sighting; the OCR below it runs once per
    // board as the full 28 calls, plus at most `RETRIES` further rounds while
    // some region is unclean — and those re-read only the unclean regions
    // (`slice::plan_read`), not the 28.
    //
    // The frame costs nothing here: it is read off the layout the anchor above
    // already resolved, and it is what stops a reopen answering with the
    // previous ROOM's board inside one epoch — see `BoardRead`.
    let frame = slice::BoardFrame::of(&layout);
    let rearmed = session.gate.rearm_pending(rearm);
    let answer = session.state.gate(key, &frame);
    if matches!(answer, GateAnswer::Read) && rearmed {
        session.gate.note_rearm(rearm);
    }
    let reshown = match answer {
        GateAnswer::Read => None,
        GateAnswer::Reshow(status) => Some(status),
    };

    // ONE line per reopen, and it says which way the gate went (POE-249).
    // `Found` and not `Held`: `Held` is the steady state of a player reading the
    // sheet, and one line per 650 ms would empty `app_log`'s 50-entry buffer in
    // half a minute. Both branches were logged before, so every reopen that
    // re-showed said "found …" and "back …" one after the other and the pair
    // read as two events.
    if detected == DetectOutcome::Found {
        crate::app_log(
            app,
            match reshown {
                Some(_) => "Temple: layout panel back — same board, no read".to_string(),
                None => format!(
                    "Temple: layout panel found (scale {:.3}, NCC {:.3})",
                    layout.scale, layout.ncc
                ),
            },
        );
    }

    let Some(status) = reshown else {
        full_read(
            app,
            session,
            cancel,
            &img,
            layout,
            &settings,
            key,
            frame,
            grabbed_at,
            fallback_origin,
        );
        return true;
    };

    // Nothing to read: the sheet was closed and reopened inside one incursion
    // onto a board whose pixels have not moved, so the board on the slice is the
    // board on screen. Only the STATUS is put back — a retired panel published
    // `panel_not_visible` and the sheet-bound overlays went with it.
    publish(app, |slice| apply_status(slice, TickOutcome::Reshown(status)));
    true
}

/// Start one cold sweep of this capture on a thread of its own, and return at
/// once — [`SweepSlot::launch`] over [`anchor::anchor_for_loop`].
///
/// `start` is what [`cold_sweep_reason`] has already charged: a null or
/// unplaced slice gets this path every [`NULL_SWEEP_EVERY`] consecutive clean
/// misses, up to [`NULL_SWEEP_CAP`] per `(temple_epoch, temple_rearm)` key; a
/// placed miss once per key under the Manual arm only. Nothing is logged here:
/// the sweep's one line is written when it ends, found or not ([`sweep_line`]).
/// A thread that cannot be spawned says so and leaves the slot empty; the start
/// is charged already, so a failing spawn cannot be retried on every tick.
///
/// # Off the loop (POE-275 WI-2, owner 2026-09-11)
///
/// The sweep is 5.3 s on a 1920x1080 capture in the Linux container (release)
/// and ~30 s on the PC's debug build (app.log 2026-09-08/09). Until 2026-09-11
/// this was the loop's longest single call, the tick waited for it, and no
/// placed recheck ran while it searched. It now takes the frame by VALUE — the
/// tick that starts it is a miss and has no further use for it — and the loop
/// goes on ticking at [`DETECT_INTERVAL`]. The search's stop check reads the
/// slot's per-sweep flag and the module's `cancel` together. It is polled only
/// inside the pyramid — building the scene, the hint-scale search and the
/// three-scale table search run to completion first — so a cancel of either
/// kind lands after that stage, then between the pyramid's coarse correlations
/// (see [`anchor::anchor_for_loop`]).
fn cold_sweep(
    app: &AppHandle,
    sweeps: &mut SweepSlot,
    img: DynamicImage,
    hint: Option<anchor::AnchorCalibration>,
    cancel: &watch::Receiver<bool>,
    start: SweepStart,
    key: (u64, u64),
) {
    let capture = (img.width(), img.height());
    let module_stop = cancel.clone();
    // The placed recheck searched a small window. This explicit fallback searches
    // the whole capture with the pyramid path, trying the placed scale first.
    let launched = sweeps.launch(start, key, capture, move |stop| {
        let halt = || stop() || *module_stop.borrow();
        let found = anchor::anchor_for_loop(&img, hint.as_ref(), &halt).ok();
        (found, halt())
    });
    if let Err(e) = launched {
        crate::app_log(
            app,
            format!("Temple: cold sweep ({:?}) could not start — {e}", start.reason),
        );
    }
}

/// A tick that produced no layout — nothing on screen, or the grab failed.
///
/// The two reach the page the same way — no board this tick — and they are
/// DIFFERENT events to the panel state machine, which is the whole reason this
/// function branches on `errored` at all. A clean miss is the loop looking and
/// finding the sheet gone: it runs the rules below. A failed grab is the loop
/// not looking at all, and since the WI-1 fix round it does one thing and
/// returns — [`LoopState::on_blind_tick`], which spends the start-up probe and
/// leaves [`LoopState::live`] where the last tick that could see the screen left
/// it. [`fail`] has already put the status and the message on the page by then,
/// so there is nothing here to publish.
///
/// The residual that buys, named in docs/TEMPLE-LIFECYCLE.md: while `live` is
/// held by a capture that keeps failing, the panel branch of
/// [`trigger::arm_source`] is ORed ABOVE Client.txt, so no Alva line and no zone
/// change shuts the gate — the first grab that SUCCEEDS does, by finding no
/// panel. Accepted against what it replaced, which was one TRANSIENT failure
/// standing the capture down under a sheet the player was reading.
///
/// `errored` is what keeps [`fail`]'s message on the page: a clean miss — the
/// loop looked and there was no panel — means the last error is over, and
/// clearing it there is what stops a one-off capture failure sitting on the
/// page for the rest of the session. [`next_status`] moves the status in the
/// same publish, because clearing the message while leaving `error` standing
/// leaves the page red with nothing under it.
///
/// # A retire ends NOTHING but `live` (POE-244, POE-248, POE-249)
///
/// It used to drop `advice` and `mode`, on the reasoning that a recommendation
/// is a move the player could still act on while the board is only a record.
/// That reasoning had the case backwards: the panel closes the moment the
/// player steps through the door, so a retire is what the whole INCURSION looks
/// like, and dropping the advice there left POE-244's door widget with no
/// purple seal, no `open <edge>` line and no architect name at exactly the point
/// those are the only things still on screen.
///
/// What ends the advice is not here and was never here: `trigger::advice_end`
/// decides — an Alva line stamped after the read, or a zone change away from the
/// temple — and `slice::clear_advice` / `slice::force_off` write. Since POE-248
/// the STAND-DOWN does not end it either ([`apply_gate`] is the status alone),
/// because the live session that reported the bug stood the loop down
/// mid-incursion with the player still in the room the widget described.
///
/// What this function does end is [`LoopState::live`], on the retire after
/// [`RETIRE_AFTER`] consecutive clean misses — the second since 2026-09-11
/// (POE-275, owner), the first from POE-249 until then. The sheet-bound overlays
/// come down on that same miss: [`miss_publish`] answers
/// [`TickOutcome::NoPanel`] for the retire and for a miss with nothing live, and
/// nothing for the held miss before a retire, so the status the last sighting
/// wrote stays on the slice through one miss. Until 2026-09-11 every clean miss
/// published `NoPanel`, which is why the constant alone did not move the hide.
/// It does NOT invalidate the board:
/// a sheet reopened inside the same incursion re-shows it ([`LoopState::board`],
/// docs/TEMPLE-LIFECYCLE.md row 3) — while the loop is still armed to see the
/// reopen, which is what the paragraph below changed.
///
/// # And, since WI-1, the CYCLE (2026-09-07)
///
/// A retire over a board this loop has already read is the sheet closing on a
/// finished cycle, so it stands the capture down for good: [`cycle_complete`]
/// decides and `trigger::complete_cycle` writes, and the loop's next iteration
/// finds the gate shut and says so. The clean/errored split is one guard — a
/// tick that could not LOOK has learned nothing about the sheet, and since the
/// fix round it cannot even reach `Retired` — the read-in-hand half of the rule
/// is the second, so a sheet closed before its read landed leaves the loop
/// probing, and [`trigger::ArmState::complete_cycle`] holds the third: a Temple
/// of Atzoatl run is not ended by one of its rooms' sheets closing.
///
/// The `key` is passed on to `trigger::complete_cycle` and is the fourth guard.
/// This tick's key was read at the top of [`tick`], before its grab and its
/// detect — over a second on a debug build (1042 ms of cheap detect measured on
/// the PC's) — so a START line or a Re-arm in between may already have armed a
/// FRESH cycle by the time this runs, and disarming that on the strength of the
/// old key would leave the next board unread until Re-arm. Until 2026-09-11 the
/// gap also held the cold sweep, 5.3 s in the release container; the sweep runs
/// off the loop since then (POE-275 WI-2), and the guard stays for the gap
/// that is left.
///
/// The stand-down is not logged here. [`gate_line`] owns that line and prints it
/// on the next iteration, which is what keeps it one line per stand-down however
/// the gate came to be shut.
///
/// Returns what the panel state machine answered, which [`tick`] hands to
/// [`cold_sweep_reason`]: only a [`DetectOutcome::Missed`] may start a sweep.
fn miss(app: &AppHandle, session: &mut Session, errored: bool, key: (u64, u64)) -> DetectOutcome {
    if errored {
        // A tick that could not LOOK, and the whole of what it does: spend the
        // start-up probe and leave the panel state where the last tick that
        // COULD look left it ([`LoopState::on_blind_tick`], whose residual is
        // named in docs/TEMPLE-LIFECYCLE.md). It retires nothing, so it says
        // nothing about the sheet, completes no cycle and takes no overlay
        // down; and [`fail`] has already published both the status and the
        // message, so publishing here would say the same thing twice.
        return session.state.on_blind_tick();
    }
    let outcome = session.state.on_detect(false);
    if outcome == DetectOutcome::Retired {
        crate::app_log(app, "Temple: layout panel gone".to_string());
    }
    // AFTER the `layout panel gone` line and before the publish, so `app.log`
    // reads in the order the events happened: the sheet went, then the gate
    // shut.
    if cycle_complete(outcome, session.state.has_read(key), !errored) {
        trigger::complete_cycle(app, key);
    }
    if let Some(status) = miss_publish(outcome) {
        publish(app, |slice| apply_status(slice, status));
    }
    outcome
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
/// [`clipped_roi_announcement`] compares on. They are declared next to
/// [`slice::unclean`], which is the fourth consumer and the one that has to
/// agree with this list letter for letter: it reads the same names to decide
/// which read failures a clipped crop already explains.
pub fn text_regions(layout: &TempleLayout) -> [(&'static str, [i32; 4]); 2] {
    [
        (slice::PANEL_REGION, panel_rect(layout.origin, layout.scale)),
        (
            slice::REMAINING_REGION,
            remaining_rect(layout.origin, layout.scale),
        ),
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
///
/// `wanted` is the region names this round is reading — [`slice::ReadPlan`]'s
/// `text`, which for round 1 is both of them and for a retry round is only the
/// ones the kept panel still owes (WI-2). A region left out is not cropped and
/// not OCR'd, so it contributes no lines and [`panel::read_panel`] answers
/// `Unknown`/empty/`None` for it — which is exactly what
/// [`slice::merge_reads`] falls back to the kept panel on. An EMPTY `wanted`
/// therefore returns `Some(vec![])` rather than `None`: a round that reads no
/// text is a round, not a failure.
fn panel_text(
    app: &AppHandle,
    session: &mut Session,
    img: &DynamicImage,
    layout: &TempleLayout,
    wanted: &[&'static str],
) -> Option<Vec<crate::mercenary::geometry::OcrLineBox>> {
    let mut lines = Vec::new();
    for (name, rect) in text_regions(layout) {
        if !wanted.contains(&name) {
            continue;
        }
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
/// while the player stands still and the game window does not move, while
/// [`full_read`] runs once per board plus its retries and 1–3 times per map on
/// top of that — so an unconditional line here repeats one value into a 50-entry
/// buffer and pushes other diagnostics out of it, which is the opposite of what
/// a diagnostic is for.
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
/// on the message.** A windowed client being DRAGGED moves both rects while the
/// fact they report ("the panel crop is off the capture") does not change, so a
/// rect-keyed memory would say it again for every board read during and after
/// the drag. The names are also the whole of what a reader needs repeated; the rects
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

/// The kept reading a fresh read of `(key, frame)` may be merged into, or
/// `None` when there is nothing it may legally touch.
///
/// [`slice::merge_reads`] may only ever be handed two readings of the SAME
/// board: it folds region by region, so two boards would file the old room's
/// corridors under the new room's name and fill a plate the player has already
/// walked past into one they are looking at. Its own precondition catches a
/// FRAME that moved; it cannot catch a board that changed behind an unmoved
/// frame, which is exactly what the epoch sees and it does not.
///
/// Through [`LoopState::same_board`], the same predicate the gate upstream used
/// to send this tick here — the drop and the gate cannot disagree about what
/// "the same board" is, and the retry the gate let through is the one case a
/// kept reading survives into.
///
/// A function rather than an `if` inside [`full_read`] so the rule has a seam:
/// [`full_read`] needs an `AppHandle`, a capture and an OCR engine, and this
/// needs none of them. It was an `if` in there, and deleting it passed every
/// test.
///
/// # Why it is generic over the reading (WI-2)
///
/// [`full_read`] asks this TWICE and must get the same answer both times: once
/// on a BORROW before any OCR, to plan which regions this round re-reads
/// ([`slice::plan_read`]), and once on the OWNED reading at the merge. The
/// predicate is the ownership rule, not the ownership: making the reading the
/// type parameter is what keeps both calls on the one rule instead of leaving
/// the planning half to a hand-written copy of it. A board this loop has not
/// read still reaches neither the plan nor [`slice::merge_reads`].
fn kept_for<T>(
    kept: Option<T>,
    state: &LoopState,
    key: (u64, u64),
    frame: &slice::BoardFrame,
) -> Option<T> {
    kept.filter(|_| state.same_board(key, frame))
}

/// Whether the read the gate just let through is a RETRY ROUND (POE-276, owner
/// 2026-09-11): a re-read of the board this loop already read and published,
/// in the same place, under the same key.
///
/// [`LoopState::same_board`], the rule [`kept_for`] and [`LoopState::note_read`]
/// already share, asked once more. [`full_read`] is entered only on a
/// [`GateAnswer::Read`], and the gate answers `Read` for the SAME board only
/// while it is unclean and owed a round — so on that path this is true exactly
/// for a retry. A first read, a Re-arm or a settings change (the rearm count
/// moved), an Alva line or a zone change (the epoch moved) and a walked or
/// dragged sheet (the frame moved) are all false.
pub fn read_is_retry(state: &LoopState, key: (u64, u64), frame: &slice::BoardFrame) -> bool {
    state.same_board(key, frame)
}

/// The `Anchored` publish: `reading`, carrying whether the read is a retry
/// round ([`TempleSlice::read_retry`], POE-276).
///
/// [`apply_status`] clears the flag for every outcome, this one included, and
/// the flag is then set only beside the `Reading` it describes — an
/// `Unavailable` slice, which no tick moves, never carries it.
pub fn apply_anchored(slice: &mut TempleSlice, retry: bool) {
    apply_status(slice, TickOutcome::Anchored);
    slice.read_retry = retry && slice.status == TempleStatus::Reading;
}

/// The expensive half of one tick: the side panel, the 13 plates, the door
/// diamond, the advisor, and one publish.
///
/// # Three rounds per board at most: one full, then at most [`RETRIES`] partial
///
/// The caller has already decided this board is worth reading —
/// [`LoopState::reshow`] answered `None`, which is the form the loop calls
/// ([`LoopState::wants_read`] is the same rule stated as the bool the tests
/// name). What happens here is the read itself and the bookkeeping that bounds
/// it: the fresh reading is MERGED into whatever this loop kept for the same
/// board ([`slice::merge_reads`]), the merged read is what the advisor ranks and
/// [`slice::project`] publishes, and [`LoopState::note_read`] records whether it
/// is still unclean and spends one round.
///
/// `key` and `frame` are the caller's — the same pair the gate decided on,
/// passed down rather than recomputed, so the read is recorded under the
/// identity it was let through as.
///
/// # What this round OCRs, decided before it OCRs anything (WI-2)
///
/// [`slice::plan_read`] is asked FIRST, from the kept reading
/// [`kept_for`] allows and the clipped-region list this capture produced: a
/// board with nothing kept is read whole, and a retry round reads only the
/// plates still unnamed, the text regions the kept panel still owes, and the
/// diamond if it failed. Every OCR call below is behind that plan.
///
/// The ORDER is the point. The kept reading is only borrowed for the plan;
/// `session.kept` is still `take`n at the merge, so a round that bails out
/// between the two — a cancelled thread, a dead OCR engine — leaves it standing
/// and the next round plans from it again rather than reading the board whole.
///
/// The merge is what makes a retry safe. Two reads of one board are two OCR
/// passes over two frames, so the second can be WORSE than the first — a plate
/// that read on the first attempt and not on the second, a panel caught
/// mid-redraw. Replacing the kept read wholesale would let that regress a board
/// the player is looking at; merging region by region cannot. That same merge is
/// what a SKIPPED region rides out on: it arrives as the placeholder an unread
/// region produces, and the kept value wins — see [`slice::merge_reads`]'s note
/// on the region nobody looked at, and the diamond arm it names.
///
/// A read that bails out (a cancelled thread, a failed OCR engine) records
/// nothing, so the next tick reads the same board from scratch.
///
/// `fallback_origin` is populated only when the explicit cold fallback found a
/// panel away from the placed origin. It is remembered after this read has
/// published successfully, so a failed or cancelled read cannot change the
/// next tick's placement.
fn full_read(
    app: &AppHandle,
    session: &mut Session,
    cancel: &watch::Receiver<bool>,
    img: &DynamicImage,
    layout: TempleLayout,
    settings: &TempleSettings,
    key: (u64, u64),
    frame: slice::BoardFrame,
    since_grab: Instant,
    fallback_origin: Option<(i32, i32)>,
) {
    let mut stages = ReadStages::default();
    // BEFORE `note_read` below, and that order is the flag: `note_read` records
    // this read as the board, after which `same_board` is true for every read.
    // Not covered by a test — the only seam that would cover it needs an
    // `AppHandle`.
    let retry = read_is_retry(&session.state, key, &frame);
    publish(app, |slice| apply_anchored(slice, retry));
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

    // WHAT THIS ROUND READS, decided before a single OCR call (WI-2). The kept
    // reading is only BORROWED here — `kept_for` owns which reading this round
    // may look at, and `full_read` still `take`s it at the merge, so a round
    // that bails out below leaves it standing for the next one.
    //
    // The names, not the rects: `slice::plan_read` asks only WHICH regions fell
    // off the capture, because a region that is not on screen cannot read
    // better on a retry — it must neither buy a round nor be re-read by one.
    let clipped_names: Vec<&'static str> = clipped.iter().map(|(name, _)| *name).collect();
    let plan = slice::plan_read(
        kept_for(session.kept.as_ref(), &session.state, key, &frame),
        &clipped_names,
    );

    let reading_text = Instant::now();
    let panel = match panel_text(app, session, img, &layout, &plan.text) {
        Some(lines) => panel::read_panel(&lines),
        None => return,
    };
    stages.text_ocr = reading_text.elapsed();

    // Up to 26 more OCR calls follow — two per PLANNED plate, so 26 on round 1
    // and two per still-unread plate on a retry. A stop that arrived during the
    // text OCR must not buy them: this is the loop's longest blocking stretch
    // and a detached thread cannot be aborted out of it. The check is passed
    // INTO `read_slots` as well, so a stop lands between two plate crops rather
    // than after all 26. Bailing records no board, so the next start reads this
    // one from scratch.
    if *cancel.borrow() {
        return;
    }
    let lattice = Lattice::new(layout.origin, layout.scale);
    let stop = || *cancel.borrow();
    let reading_plates = Instant::now();
    let rooms = panel::read_slots(&SystemOcr, img, &lattice, &plan.plates, &stop);
    stages.plate_ocr = reading_plates.elapsed();
    if *cancel.borrow() {
        return;
    }

    let reading_markers = Instant::now();
    // A round that did not plan the diamond leaves BOTH halves empty, which is
    // `slice::merge_reads`' "not read" state and merges to the kept door set.
    // It is not `Ok(empty set)` — that would publish a room with no corridors.
    let (settled, marker_error) = match plan.markers {
        false => (None, None),
        true => match read_markers(img, &layout) {
            Ok(set) => (Some(set), None),
            Err(e) => (None, Some(e)),
        },
    };
    stages.markers = reading_markers.elapsed();

    // The merge (POE-249). `kept_for` owns the drop — see it for why a board
    // this loop has not read must not reach `slice::merge_reads` at all.
    let fresh = slice::KeptRead {
        layout,
        rooms,
        panel,
        settled,
        marker_error,
    };
    let read = match kept_for(session.kept.take(), &session.state, key, &frame) {
        Some(kept) => slice::merge_reads(&kept, fresh),
        None => fresh,
    };

    // Both name-sources for the room the player is standing in read, and they
    // disagree: the advice carries a warning for the overlay, and this puts the
    // same fact in the app log, which is what a user can send back. `log::` is
    // not that — it goes nowhere under `windows_subsystem = "windows"`.
    //
    // Decided on the MERGED read, and it has to be (WI-2): since a retry round
    // OCRs only what is still unclean, this round's own `rooms`/`panel` can each
    // be the placeholder an unread region produces, so a round that re-read only
    // the side panel would compare a fresh title against thirteen `Unknown`
    // plates and never fire. `advisor/state.rs` raises the overlay warning from
    // the merged read, so reading anything else here would put a DIFFERENT fact
    // in the log than the one on screen — which is the opposite of what this
    // block is for. `slice::merge_reads`' own tests pin the seam (a kept plate
    // under a freshly read title disagrees only after the merge); the ordering
    // of these two statements is not itself covered, because the only seam that
    // would cover it needs an `AppHandle`.
    if let Some((title, plate)) = slice::current_identity(
        read.layout.current,
        &slice::identities(&read.rooms),
        &read.panel,
    )
    .disagreement
    {
        crate::app_log(
            app,
            format!(
                "Temple: side panel says {title:?} but the current plate says {plate:?}; using the plate"
            ),
        );
    }
    // ONE valuation for this read, handed to the ranking AND to the offer
    // boxes below (POE-257 D6). Two calls would let the number on screen come
    // from a market read the recommendation never saw.
    //
    // LOOKED UP, not built (POE-257 WI-3): `ssot::temple_valuation_now` hands
    // back the table already computed for the preset, the Custom rates and this
    // market read, and only builds one when one of those has moved — the
    // owner's "computed on update, and the module uses already computed
    // weightings each time". It is the same call `commands::temple_value_table`
    // makes for the preset in force, so the page and this board are one object.
    //
    // The market arrives as STATE, never as a fetch: `crate::ssot` polls it
    // every `ssot::TEMPLE_MARKET_POLL` and this reads what it stored, because
    // `docs/TEMPLE-LIFECYCLE.md` forbids network work on this tick. It comes
    // back FROM the accessor rather than being read again here, so the table
    // and the `market_view` under the offer boxes can never be about two
    // different reads. The staleness judgement is made against THIS clock
    // (POE-258 D2) — before the first poll answers, and whenever the last read
    // has aged out, it is `MarketInput::none()` in effect and every room falls
    // back to its grade ladder value rather than to zero, which is epic lock
    // L4.
    let valuing = Instant::now();
    // THIS TICK'S settings snapshot, not the live ones: the table has to be
    // about the same preset and Custom table the projection echoes below and
    // `advise_read` ranks with, or a setter landing mid-read would publish one
    // preset's numbers under the other preset's name for a tick.
    let (valuation, market, valuation_source) = crate::ssot::temple_valuation_now(app, settings);
    stages.valuation = valuing.elapsed();
    stages.valuation_source = valuation_source;
    let advising = Instant::now();
    let advice = slice::advise_read(
        &read.layout,
        &read.rooms,
        &read.panel,
        read.settled.as_ref(),
        settings,
        &valuation,
    );
    stages.advise = advising.elapsed();
    let read_at = now_ms();

    let projected = slice::project(
        &slice::ReadResult {
            layout: &read.layout,
            rooms: &read.rooms,
            panel: &read.panel,
            settled: read.settled.as_ref(),
            marker_error: read.marker_error.clone(),
            read_notice,
            advice: advice.as_ref(),
            // The settings THIS tick started with. A setter that lands
            // mid-read echoes its new value onto the slice and then loses it
            // again for one tick when this projection overwrites it — the
            // setters' own `rearm` forces the next read, which restores it.
            //
            // The VALUATION above is from this same snapshot (POE-257 WI-3
            // fix round, 2026-09-07): `ssot::temple_valuation_now` is handed
            // `settings` rather than re-reading the live mutex, so the numbers
            // and the echo beside them are always the same preset's.
            config: settings.config.clone(),
            profile: settings.profile.clone(),
            preset: settings.preset,
            custom: settings.custom.clone(),
            valuation: &valuation,
            // The SAME read the valuation above was computed from, so the
            // price-age line and the numbers on the offer boxes can never be
            // about different markets.
            market: slice::market_view(&market),
            read_at,
        },
        // The calibration THIS capture measured, which is what the page's
        // "anchor calibration" row means: the scale the board in front of the
        // user was actually read at, not a remembered one. Since POE-234 WI-2
        // there is no remembered one to confuse it with — the module's only
        // store is the shared screen slice, and this is the read's own answer.
        //
        // From the MERGED read, whose layout is always the fresh anchor: every
        // ROI the slice publishes is placed from it, so it has to describe the
        // frame the player is looking at.
        Some(read.layout.calibration),
    );
    // The same `clipped_names` the plan above was built from — `slice::unclean`
    // and `slice::plan_read` are one function asked twice, and asking them with
    // two different exemption lists inside one read would let a round be bought
    // for a region the next round then refuses to look at.
    let unclean = slice::unclean(&read, &clipped_names);
    session.state.note_read(key, &frame, projected.status, unclean);
    session.kept = Some(read);
    let publishing = Instant::now();
    publish(app, |slice| *slice = projected);
    stages.publish = publishing.elapsed();
    remember_fallback_anchor(fallback_origin, |origin| {
        crate::ssot::remember_anchor(
            app,
            crate::ssot::AnchorModule::TempleEntrance,
            origin,
        );
    });
    // One line per read, always: reads are once per board plus at most
    // `RETRIES`, and this line is how the owner's ~1 s panel-open → verdict
    // budget is checked against a real session (docs/TEMPLE-LIFECYCLE.md,
    // "Cadences and budgets"). The overlay writes the other half —
    // `[temple-overlay] board read at <read_at> …` — against the same stamp.
    crate::app_log(
        app,
        read_timings_line(
            &session.tick_stages,
            &stages,
            since_grab.elapsed(),
            read_at,
            unclean,
            session.state.board.as_ref().map(|board| board.retries_left),
            &plan,
        ),
    );
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

    /// A moment in the shape [`now_ms`] returns (2025-09-02 08:00 UTC), for the
    /// arm-gate tests that need a clock. Nothing reads its value — the
    /// assertions are all on differences from it.
    const SEEN: u64 = 1_756_800_000_000;

    /// One missed anchor over a live panel is a HELD miss, and the panel stays
    /// live (owner, 2026-09-11, POE-275: *"one missed probe is easy to get (a
    /// misread, a tooltip over the plate)"*).
    ///
    /// Fails if `RETIRE_AFTER` is put back to one: the first miss would answer
    /// `Retired` and clear `live`.
    #[test]
    fn a_live_panel_holds_through_its_first_missed_anchor() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        assert_eq!(state.on_detect(false), DetectOutcome::HeldMiss);
        assert!(state.live, "a held miss leaves the panel live");
    }

    /// The SECOND consecutive missed anchor retires it — the outcome
    /// `cycle_complete` keys on and the `layout panel gone` line.
    ///
    /// Fails if `RETIRE_AFTER` is raised past two, or if the count is not
    /// carried from the first miss to the second: either way the second miss
    /// answers `HeldMiss` and a closed sheet never retires.
    #[test]
    fn a_live_panel_retires_on_its_second_consecutive_missed_anchor() {
        let mut state = LoopState { live: true, ..LoopState::default() };
        assert_eq!(state.on_detect(false), DetectOutcome::HeldMiss, "precondition");

        assert_eq!(state.on_detect(false), DetectOutcome::Retired);
    }

    /// Two misses count only when they are CONSECUTIVE: a sighting between them
    /// starts the count again, so the next miss is held rather than retiring.
    ///
    /// Fails if a sighting does not zero `misses` — a sheet that flickers once
    /// per incursion would then retire on its second flicker, however far apart
    /// the two were.
    #[test]
    fn a_sighting_between_two_misses_starts_the_count_again() {
        let mut state = LoopState { live: true, ..LoopState::default() };
        assert_eq!(state.on_detect(false), DetectOutcome::HeldMiss, "precondition");
        assert_eq!(state.on_detect(true), DetectOutcome::Held, "precondition");

        assert_eq!(state.on_detect(false), DetectOutcome::HeldMiss);
    }

    /// A sighting after a retire is `Found` again, which is the `layout panel
    /// found` log line and, on a board already read, the `layout panel back`
    /// one. Fails if a retire leaves `live` set — the reopen would report `Held`
    /// and neither line would ever be said twice in a session.
    #[test]
    fn a_sighting_after_a_retire_is_found_again() {
        let mut state = LoopState { live: true, ..LoopState::default() };
        state.on_detect(false);
        assert_eq!(state.on_detect(false), DetectOutcome::Retired, "precondition");

        assert_eq!(state.on_detect(true), DetectOutcome::Found);
    }

    // ----------------------------------------------- what a miss publishes --

    /// The held miss publishes nothing, so the status the last sighting wrote
    /// stays on the slice and the sheet-bound overlays with it (POE-275).
    ///
    /// Composed over the state machine, as `miss` composes it. Fails if the
    /// held miss is answered with `NoPanel` — `panel_not_visible` is not an
    /// overlay-visible status, so the offer boxes would come down on one
    /// misread — or if `RETIRE_AFTER` is put back to one.
    #[test]
    fn a_held_miss_publishes_nothing() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        assert_eq!(miss_publish(state.on_detect(false)), None);
    }

    /// The retiring miss publishes `NoPanel`, which is what takes the
    /// sheet-bound overlays down on the second consecutive miss.
    ///
    /// Fails if the retire publishes nothing: a closed sheet's offer boxes
    /// would stand over the game until the next sighting or stand-down.
    #[test]
    fn the_retiring_miss_publishes_no_panel() {
        let mut state = LoopState { live: true, ..LoopState::default() };
        state.on_detect(false);

        assert_eq!(miss_publish(state.on_detect(false)), Some(TickOutcome::NoPanel));
    }

    /// A miss with nothing live is the loop searching, and it still says so:
    /// `panel_not_visible`, with a stale error cleared (`next_status`).
    ///
    /// Fails if the held-miss rule is widened to every miss that retires
    /// nothing — a one-off capture error would then sit on the page, with its
    /// `error` status, for as long as the sheet stays shut.
    #[test]
    fn a_miss_with_nothing_live_publishes_no_panel() {
        let mut state = LoopState::default();

        assert_eq!(miss_publish(state.on_detect(false)), Some(TickOutcome::NoPanel));
    }

    // ------------------------------------------------------- the OCR gate --

    /// Two board keys, differing in the epoch — an Alva line or a zone change
    /// between them.
    const BOARD: (u64, u64) = (4, 0);
    const NEXT_BOARD: (u64, u64) = (5, 0);
    /// The pixel half of the identity: the frame a sheet was read at. Built
    /// through `slice::BoardFrame::of` from the shared fixture layout, so what
    /// these tests pin is what the GATE does with two real frames rather than
    /// what it does with a hand-written struct.
    fn frame(
        current: Option<lattice::Slot>,
        doors: &[(lattice::Slot, lattice::Slot)],
    ) -> slice::BoardFrame {
        slice::BoardFrame::of(&slice::fixture_layout(current, doors, &[]))
    }

    /// The fixture frame: standing in B0, one corridor open.
    fn same_frame() -> slice::BoardFrame {
        frame(Some(lattice::Slot::B0), &[(lattice::Slot::B0, lattice::Slot::C1)])
    }

    /// …and the same sheet after the player walked to the next room, which is
    /// the change inside one epoch the key cannot see.
    fn walked_frame() -> slice::BoardFrame {
        frame(Some(lattice::Slot::C1), &[(lattice::Slot::B0, lattice::Slot::C1)])
    }

    /// [`same_frame`] shifted `dx` px and rescaled `milli` thousandths — the two
    /// BANDED fields, moved on purpose.
    fn nudged(dx: i32, milli: i32) -> slice::BoardFrame {
        let base = same_frame();
        slice::BoardFrame {
            origin: (base.origin.0 + dx, base.origin.1),
            scale_milli: (base.scale_milli as i32 + milli) as u32,
            ..base
        }
    }

    /// A loop that has read nothing reads the first board it sees. Fails if
    /// `wants_read` treats a missing board as "already read", which would mean
    /// the module never OCRs anything.
    #[test]
    fn a_loop_that_has_read_no_board_reads_the_first_one_it_sees() {
        assert!(LoopState::default().wants_read(BOARD, &same_frame()));
    }

    /// The gate's whole purpose (docs/TEMPLE-LIFECYCLE.md row 3): a sheet closed
    /// and reopened inside one incursion, onto a board whose pixels have not
    /// moved, is the same board — and re-showing it costs no OCR. Fails if the
    /// identity is ignored — the loop would pay 28 OCR calls every 650 ms for as
    /// long as the player kept the sheet open.
    #[test]
    fn a_board_already_read_clean_is_not_read_again() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert!(!state.wants_read(BOARD, &same_frame()));
    }

    /// An Alva line or a zone change moves the epoch, which is the one way the
    /// board's CONTENTS can change. Fails if the key is not compared —
    /// the next incursion would be advised off the last one's board.
    #[test]
    fn a_board_read_under_another_key_is_read_again() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert!(state.wants_read(NEXT_BOARD, &same_frame()));
    }

    /// The other half of the identity, and the trigger POE-249 restored: the
    /// key cannot see the player walk to the next room, a corridor open or the
    /// window move, and all three happen INSIDE one epoch on a Temple of
    /// Atzoatl run. This is the old `layout_wants_read` gate, kept as the frame
    /// half of `same_board`.
    ///
    /// Fails if `same_board` compares the key alone — a reopen would answer
    /// `Reshown` with the previous room's outline, seals, advice and
    /// never-cover set over the frame in front of the player (ADR-019).
    #[test]
    fn a_board_whose_frame_moved_is_read_again() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert_eq!(state.reshow(BOARD, &walked_frame()), None);
    }

    /// The Re-arm button is the third part of the identity, so pressing it
    /// forces a re-read of the board it was pressed over. Fails if only the
    /// epoch is compared — the button would cost an anchor attempt and change
    /// nothing on screen, which is the whole complaint it exists to answer.
    #[test]
    fn a_rearm_bump_forces_the_same_board_to_be_read_again() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert!(state.wants_read((BOARD.0, BOARD.1 + 1), &same_frame()));
    }

    /// An unclean board is re-read, and exactly `RETRIES` more times.
    ///
    /// Fails at both ends: no retry at all leaves a half-read board standing for
    /// the incursion, and an unbounded one pays the full read on every tick for
    /// a region that is never going to resolve (a plate outside the vocabulary,
    /// a diamond under the selection frame).
    #[test]
    fn an_unclean_board_is_re_read_exactly_twice_more() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);

        let mut reads = 0;
        while state.wants_read(BOARD, &same_frame()) {
            reads += 1;
            assert!(reads <= 10, "the retry budget never ran out");
            state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);
        }

        assert_eq!(reads, usize::from(RETRIES));
    }

    /// A retry that came back clean stops the budget early — there is nothing
    /// left to improve. Fails if `unclean` is not consulted on each read, which
    /// would spend the whole budget on every board.
    #[test]
    fn a_retry_that_reads_clean_stops_the_budget() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);
        assert!(state.wants_read(BOARD, &same_frame()), "precondition: one retry is owed");

        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert!(!state.wants_read(BOARD, &same_frame()));
    }

    /// A NEW board gets a fresh budget, however much of the last one was spent.
    /// Fails if `note_read` decrements unconditionally: the second incursion of
    /// a map would get fewer retries than the first, and the fourth none.
    #[test]
    fn a_new_board_starts_with_a_whole_retry_budget() {
        let mut state = LoopState::default();
        for _ in 0..=RETRIES {
            state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);
        }
        assert!(!state.wants_read(BOARD, &same_frame()), "precondition: the budget is spent");

        state.note_read(NEXT_BOARD, &same_frame(), TempleStatus::Read, true);

        assert_eq!(
            state.board.expect("a board was just recorded").retries_left,
            RETRIES,
        );
    }

    /// …and so does the board the player WALKED into under an unchanged key,
    /// which is the same rule reached through the frame's exact third. That
    /// board is a first look, not the third attempt at the room behind it.
    ///
    /// Fails if `note_read`'s restore is keyed on the key alone: a spent board
    /// would hand its exhausted budget to the next room, and an unclean read
    /// there would stand for the rest of the epoch with no retry. It is also the
    /// second symptom of a key-only gate — a same-key read whose merge the frame
    /// rejected spent a retry it never got the benefit of.
    #[test]
    fn a_walk_to_the_next_room_restores_the_retry_budget() {
        let mut state = LoopState::default();
        for _ in 0..=RETRIES {
            state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);
        }
        assert!(!state.wants_read(BOARD, &same_frame()), "precondition: the budget is spent");

        assert!(state.wants_read(BOARD, &walked_frame()), "the moved board is read");
        state.note_read(BOARD, &walked_frame(), TempleStatus::Read, true);

        assert_eq!(
            state.board.expect("a board was just recorded").retries_left,
            RETRIES,
        );
    }

    /// `full_read` drops its kept reading on exactly this predicate before
    /// `slice::merge_reads` is handed anything, so the retry — the one case a
    /// kept reading must survive into — is the one case it answers `true`.
    ///
    /// Fails if `same_board` stops being the shared rule: a drop keyed on
    /// something looser than the gate would merge two readings of two boards,
    /// which files the old room's corridors under the new room's name.
    #[test]
    fn a_retry_of_the_recorded_board_is_the_same_board() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);

        assert!(state.same_board(BOARD, &same_frame()));
    }

    // ------------------------------------------------------- the frame band --

    /// The band's upper edge: an anchor origin that landed
    /// `slice::FRAME_ORIGIN_TOLERANCE` px away is the SAME sheet, re-found by a
    /// correlation over a frame the game was still drawing.
    ///
    /// Fails if the origins are compared exactly — which is what the first cut
    /// of this gate did, and it is the unbounded case: a new board restores the
    /// retry budget, so a per-frame jitter would have re-read an unclean board
    /// at the full 650 ms cadence for as long as the sheet was open.
    #[test]
    fn an_origin_inside_the_tolerance_is_the_same_board() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert!(state.same_board(BOARD, &nudged(slice::FRAME_ORIGIN_TOLERANCE, 0)));
    }

    /// …and one px past it is not. The window was dragged, and every ROI the
    /// kept read placed is somewhere else.
    ///
    /// Fails if the band is applied with `<` instead of `>` on the wrong side,
    /// or widened: a dragged panel would re-show the outline, seals and
    /// never-cover set at the old coordinates (ADR-019).
    #[test]
    fn an_origin_past_the_tolerance_is_a_new_board() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert!(!state.same_board(BOARD, &nudged(slice::FRAME_ORIGIN_TOLERANCE + 1, 0)));
    }

    /// Half a per cent of scale drift is inside the band — it is finer than the
    /// step `anchor::SCALE_STEP` searches at, so it is not a scale the anchor
    /// could have chosen between.
    ///
    /// Fails if the scale is compared exactly.
    #[test]
    fn a_scale_inside_the_tolerance_is_the_same_board() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert!(state.same_board(BOARD, &nudged(0, 5)));
    }

    /// Two per cent is not: that is two search steps, which is the in-game UI
    /// scale slider having moved. Fails if the band is widened past one step —
    /// the plate crops would be placed at the old pitch.
    #[test]
    fn a_scale_past_the_tolerance_is_a_new_board() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert!(!state.same_board(BOARD, &nudged(0, 20)));
    }

    /// The exact third of the frame, and the trigger POE-249 restored: the key
    /// cannot see the player walk to the next room, and that happens INSIDE one
    /// epoch on every Temple of Atzoatl run. This is the old `layout_wants_read`
    /// gate, kept as `BoardFrame::semantic`.
    ///
    /// Fails if `current` is dropped from `slice::layout_signature`, or if the
    /// semantic third is banded like the other two: a reopen would answer
    /// `Reshown` with the previous room's outline, seals, advice and never-cover
    /// set over the frame in front of the player (ADR-019).
    #[test]
    fn a_new_current_room_is_a_new_board() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert!(!state.same_board(BOARD, &walked_frame()));
    }

    /// …and so is a corridor that opened under an unmoved player, which is the
    /// other half of the same third. Fails if `doors` is dropped from
    /// `slice::layout_signature`: the room widget would keep drawing the sealed
    /// door the player just opened.
    #[test]
    fn an_opened_corridor_is_a_new_board() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        let opened = frame(
            Some(lattice::Slot::B0),
            &[
                (lattice::Slot::B0, lattice::Slot::C1),
                (lattice::Slot::B0, lattice::Slot::C0),
            ],
        );

        assert!(!state.same_board(BOARD, &opened));
    }

    // ------------------------------------------------------ the kept read --

    /// The retry, which is the ONE case a kept reading survives into: the fresh
    /// read is about to be folded into it region by region.
    ///
    /// Fails if `kept_for` ignores the predicate in the permissive direction's
    /// mirror — returning `None` unconditionally would make every retry a
    /// wholesale replacement, which is what `slice::merge_reads` exists to stop.
    #[test]
    fn a_retry_of_the_recorded_board_keeps_its_reading() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);
        let kept = slice::fixture_read(slice::fixture_layout(Some(lattice::Slot::B0), &[], &[]));

        assert_eq!(
            kept_for(Some(kept.clone()), &state, BOARD, &same_frame()),
            Some(kept),
        );
    }

    /// A board read under another KEY drops it. Fails if `kept_for` ignores the
    /// predicate: `slice::merge_reads` would be handed the last incursion's
    /// reading and would fill this board's unread plates from it.
    #[test]
    fn a_read_under_another_key_drops_the_kept_reading() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);
        let kept = slice::fixture_read(slice::fixture_layout(Some(lattice::Slot::B0), &[], &[]));

        assert_eq!(kept_for(Some(kept), &state, NEXT_BOARD, &same_frame()), None);
    }

    /// …and so does a board whose FRAME moved past the band, which is the half
    /// the key cannot see. Fails for the same reason, on the path a Temple of
    /// Atzoatl run actually takes: the player walks and the epoch does not move.
    #[test]
    fn a_read_under_a_moved_frame_drops_the_kept_reading() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);
        let kept = slice::fixture_read(slice::fixture_layout(Some(lattice::Slot::B0), &[], &[]));

        assert_eq!(kept_for(Some(kept), &state, BOARD, &walked_frame()), None);
    }

    // ------------------------------------------------ the retry flag (POE-276) --

    /// What the `Anchored` publish at the top of `full_read` leaves on a slice
    /// that was showing `before`, for a read of `(key, frame)` the gate let
    /// through — the two pure halves of that one line, composed as it composes
    /// them.
    fn anchored(
        state: &LoopState,
        key: (u64, u64),
        frame: &slice::BoardFrame,
        before: TempleStatus,
    ) -> TempleSlice {
        let mut slice = TempleSlice { status: before, ..TempleSlice::default() };
        apply_anchored(&mut slice, read_is_retry(state, key, frame));
        slice
    }

    /// A retry round of an unclean board publishes `reading` AS a retry, which
    /// is what keeps the room widget's `reading…` line down under the verdict
    /// the round is refining (owner, 2026-09-11). Fails if the flag is never
    /// set, which brings the line back ~650 ms after every unclean verdict.
    #[test]
    fn a_retry_round_publishes_reading_as_a_retry() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);
        assert!(state.wants_read(BOARD, &same_frame()), "precondition: a round is owed");

        let slice = anchored(&state, BOARD, &same_frame(), TempleStatus::Read);

        assert_eq!(slice.status, TempleStatus::Reading);
        assert!(slice.read_retry);
    }

    /// The first read of a loop is not a retry — there is no verdict on screen
    /// for it to refine, and its line is the whole point of POE-276. Fails if
    /// every `Anchored` is flagged.
    #[test]
    fn a_first_read_publishes_reading_without_the_retry_flag() {
        let slice = anchored(&LoopState::default(), BOARD, &same_frame(), TempleStatus::Idle);

        assert!(!slice.read_retry);
    }

    /// A read under a new EPOCH — an Alva line or a zone change, including a
    /// START heard with the sheet still open — is a new board, even over the
    /// same pixels and straight after the last verdict. Fails if the flag is
    /// keyed on the frame alone.
    #[test]
    fn a_read_under_a_new_epoch_is_not_a_retry() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);

        let slice = anchored(&state, NEXT_BOARD, &same_frame(), TempleStatus::Read);

        assert!(!slice.read_retry);
    }

    /// A Re-arm (or a settings change, which bumps the same count) forces a
    /// read the player asked for, and it shows its line (owner, 2026-09-11).
    /// Fails if the rearm half of the key is ignored.
    #[test]
    fn a_rearm_forced_read_is_not_a_retry() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);

        let slice = anchored(&state, (BOARD.0, BOARD.1 + 1), &same_frame(), TempleStatus::Read);

        assert!(!slice.read_retry);
    }

    /// The sheet the player walked to the next room with is a new board under
    /// the same key. Fails if the flag is keyed on the key alone.
    #[test]
    fn a_read_of_a_walked_sheet_is_not_a_retry() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, true);

        let slice = anchored(&state, BOARD, &walked_frame(), TempleStatus::Read);

        assert!(!slice.read_retry);
    }

    /// Every loop event ends a retry in flight — the round failed, the sheet
    /// went, the loop stood down or stopped, or the next `Anchored` arrived and
    /// will say for itself. Fails if `apply_status` leaves a standing `true`
    /// for the next first read to inherit.
    #[test]
    fn every_status_outcome_ends_a_retry_in_flight() {
        for outcome in [
            TickOutcome::Failed,
            TickOutcome::NoPanel,
            TickOutcome::Anchored,
            TickOutcome::Reshown(TempleStatus::Read),
            TickOutcome::Disarmed,
            TickOutcome::Armed,
            TickOutcome::Stopping,
        ] {
            let mut slice = TempleSlice {
                status: TempleStatus::Reading,
                read_retry: true,
                ..TempleSlice::default()
            };

            apply_status(&mut slice, outcome);

            assert!(!slice.read_retry, "{outcome:?}");
        }
    }

    /// The flag describes a `reading` status and nothing else. Fails if
    /// `apply_anchored` sets it over a slice no tick can move — an
    /// `Unavailable` one would then carry a retry it is not running.
    #[test]
    fn an_unavailable_slice_never_carries_the_retry_flag() {
        let mut slice = TempleSlice { status: TempleStatus::Unavailable, ..TempleSlice::default() };

        apply_anchored(&mut slice, true);

        assert!(!slice.read_retry);
    }

    /// The status a reopen re-shows is the one that board's own projection
    /// wrote. Fails if `note_read` records a constant, or if `reshow` derives
    /// the status instead of returning it: a sheet opened between rooms
    /// published `no_current_room`, and coming back as `read` would put the
    /// sheet-bound overlays over a board with no advice.
    #[test]
    fn a_reshown_board_carries_the_status_its_own_read_published() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::NoCurrentRoom, false);

        assert_eq!(state.reshow(BOARD, &same_frame()), Some(TempleStatus::NoCurrentRoom));
    }

    // --------------------------------------------------- reshown, on screen --

    /// A reopened sheet puts its own board's status back, whichever of the two
    /// a read can produce. Fails if `Reshown` maps to a fixed status —
    /// `Reading` would claim a read that is not running, and `Read` would claim
    /// advice a `no_current_room` board does not have.
    #[test]
    fn a_reshown_board_publishes_the_status_it_carries() {
        for status in [TempleStatus::Read, TempleStatus::NoCurrentRoom] {
            let update = next_status(TempleStatus::PanelNotVisible, TickOutcome::Reshown(status));

            assert_eq!(update.status, status);
        }
    }

    /// …and it is a sighting, so it clears the waiting notice and the last
    /// error like the read it is standing in for. Fails if `Reshown` is left out
    /// of either clear: the notice would sit over a sheet that is on screen.
    #[test]
    fn a_reshown_board_is_a_sighting_for_the_notice_and_the_error() {
        let update = next_status(
            TempleStatus::PanelNotVisible,
            TickOutcome::Reshown(TempleStatus::Read),
        );

        assert!(update.clear_waiting);
        assert!(update.clear_error);
    }

    /// The first anchor after nothing is `Found`, which is the log line.
    #[test]
    fn the_first_anchor_reports_found_and_later_ones_do_not() {
        let mut state = LoopState::default();

        assert_eq!(state.on_detect(true), DetectOutcome::Found);
        assert_eq!(state.on_detect(true), DetectOutcome::Held);
    }

    /// The write POE-246's stand-down rule rests on, in the form WI-1 left it:
    /// a tick that anchors leaves the panel LIVE, which is the arm gate's whole
    /// view of the screen.
    ///
    /// Fails if `live` is set only on the `Found` transition — a `Held` tick
    /// would then clear it and the loop would stand down under a sheet the
    /// player is reading, which is the 14:37:00 bug.
    #[test]
    fn every_anchored_tick_leaves_the_panel_live() {
        let mut state = LoopState::default();

        state.on_detect(true);
        state.on_detect(true);

        assert!(state.live);
    }

    /// Two consecutive clean misses over a live panel are the sheet closing, and
    /// `live` goes with them on the second ([`RETIRE_AFTER`] = 2 since
    /// 2026-09-11).
    ///
    /// Fails if the retire leaves `live` set: the gate would then stay open on a
    /// panel that is gone for the rest of the session, which is the
    /// free-running capture POE-242 removed.
    #[test]
    fn the_second_tick_that_found_nothing_takes_the_live_panel_with_it() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        state.on_detect(false);
        state.on_detect(false);

        assert!(!state.live);
    }

    /// The held miss keeps the panel holding the arm gate: with nothing in
    /// Client.txt, the gate is still open on `PanelOnScreen` after one miss
    /// (POE-275 — one miss must not end anything).
    ///
    /// Fails if the held miss clears `live`: a hideout read, or a Re-arm whose
    /// grace has run out under an open sheet, would stand the capture down on
    /// one misread.
    #[test]
    fn a_held_miss_keeps_the_panel_holding_the_arm_gate() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        state.on_detect(false);

        assert_eq!(
            trigger::arm_source(
                trigger::ArmState::default(),
                state.live,
                state.probe_pending(),
                SEEN,
            ),
            Some(trigger::ArmSource::PanelOnScreen),
        );
    }

    /// The start-up probe is a debt one tick settles, whatever that tick found.
    ///
    /// Fails if the probe is spent only on a sighting: a loop started over an
    /// empty screen would then hold the gate open for the rest of the session,
    /// which is the free-running capture POE-242 removed.
    #[test]
    fn the_first_tick_spends_the_start_up_probe_even_when_it_finds_nothing() {
        let mut state = LoopState::default();
        assert!(state.probe_pending(), "a loop that has not looked owes one look");

        state.on_detect(false);

        assert!(!state.probe_pending());
    }

    /// And a tick whose screen GRAB failed settles it too, because it also ran.
    ///
    /// Fails if [`LoopState::on_blind_tick`] leaves the debt standing: a machine
    /// whose capture never succeeds would hold the gate open for the whole
    /// session, which is POE-242's free-running capture with an error message on
    /// top of it.
    #[test]
    fn a_failed_grab_still_spends_the_start_up_probe() {
        let mut state = LoopState::default();
        assert!(state.probe_pending(), "a loop that has not looked owes one look");

        state.on_blind_tick();

        assert!(!state.probe_pending());
    }

    /// The fix round's rule: an errored tick says nothing about the sheet, so it
    /// leaves the panel exactly where the last tick that could SEE the screen
    /// left it — and with it the arm gate, whose whole view of the screen is
    /// [`LoopState::live`] since WI-1.
    ///
    /// The arm here is `default` — nothing in Client.txt — which is the case the
    /// panel is the only thing holding the gate: a hideout read, or a Re-arm
    /// whose sixty seconds have run out under an open sheet.
    ///
    /// Fails if the errored tick is folded through `on_detect(false)` like a
    /// clean miss: the outcome is then not `Blind`. At [`RETIRE_AFTER`] `= 1`
    /// (until 2026-09-11) that fold retired on the spot, so ONE transient capture
    /// failure stood the capture down with the sheet in front of the player,
    /// recoverable only by Re-arm; at `2` it counts toward the retire, which the
    /// next test pins.
    #[test]
    fn a_failed_grab_does_not_stand_the_capture_down() {
        let mut state = LoopState { live: true, ..LoopState::default() };

        let outcome = state.on_blind_tick();

        assert_eq!(outcome, DetectOutcome::Blind);
        assert!(state.live, "the last tick that looked saw the sheet");
        assert_eq!(
            trigger::arm_source(
                trigger::ArmState::default(),
                state.live,
                state.probe_pending(),
                SEEN,
            ),
            Some(trigger::ArmSource::PanelOnScreen),
        );
    }

    /// A failed grab between two clean misses is invisible to the count: the
    /// second miss retires exactly as if the failed tick had not run (POE-275,
    /// the count `RETIRE_AFTER` = 2 made live).
    ///
    /// Fails if `on_blind_tick` zeroes `misses` (the second miss would be held
    /// again, and a capture failing every other tick would keep a closed sheet
    /// live), and fails if it counts as a miss (the failed tick would retire the
    /// panel itself and the second clean miss would answer `Missed`).
    #[test]
    fn a_failed_grab_between_two_misses_leaves_the_count_where_it_was() {
        let mut state = LoopState { live: true, ..LoopState::default() };
        assert_eq!(state.on_detect(false), DetectOutcome::HeldMiss, "precondition");

        state.on_blind_tick();

        assert_eq!(state.on_detect(false), DetectOutcome::Retired);
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
            trigger::arm_source(arm, state.live, state.probe_pending(), SEEN),
            Some(trigger::ArmSource::StartupProbe),
            "the probe tick is allowed to run",
        );

        state.on_detect(true);

        assert_eq!(
            trigger::arm_source(arm, state.live, state.probe_pending(), SEEN + 1_000),
            Some(trigger::ArmSource::PanelOnScreen),
        );
    }

    /// The other half: a probe that finds nothing stands the loop down on the
    /// next iteration, which is POE-242's behaviour for a screen with no panel
    /// on it. Fails if the probe survives its own tick.
    #[test]
    fn a_probe_tick_that_finds_nothing_stands_the_loop_down() {
        let mut state = LoopState::default();

        state.on_detect(false);

        assert_eq!(
            trigger::arm_source(
                trigger::ArmState::default(),
                state.live,
                state.probe_pending(),
                SEEN,
            ),
            None,
        );
    }

    // ------------------------------------------------ the completed cycle --

    /// WI-1's rule (owner, 2026-09-07): the sheet was READ and it has now gone,
    /// so this incursion's cycle is over and the loop stops capturing.
    ///
    /// Fails if the retire is not consulted at all — the loop would then stand
    /// down on the tick after the read rather than on the retire after the sheet
    /// closed, taking the offer boxes off a sheet still in front of the player.
    #[test]
    fn a_retire_over_a_board_already_read_completes_the_cycle() {
        assert!(cycle_complete(DetectOutcome::Retired, true, true));
    }

    /// The guard the owner kept: a sheet that closed before any read landed ends
    /// nothing, because the incursion this arm was bought for has not been read.
    ///
    /// Fails if the read half is dropped — the first miss of an armed incursion
    /// is the state the loop lives in while it waits for the player to open the
    /// sheet, so the module would stand down seconds after every START line and
    /// never read anything again without Re-arm.
    #[test]
    fn a_retire_with_no_read_in_hand_completes_nothing() {
        assert!(!cycle_complete(DetectOutcome::Retired, false, true));
    }

    /// A tick whose screen grab FAILED learned nothing about the sheet, so it
    /// completes nothing either — however much the panel state machine, fed the
    /// same failure, reports a retire.
    ///
    /// Fails if the failing-grab case is dropped: one transient capture error
    /// over a read board would stand the module down for the rest of the
    /// incursion, and the recovery is a button the player has no reason to press
    /// because nothing on screen changed.
    #[test]
    fn a_tick_that_could_not_look_completes_nothing() {
        assert!(!cycle_complete(DetectOutcome::Retired, true, false));
    }

    /// Only the RETIRE completes it: the other four outcomes are not a sheet
    /// leaving the screen.
    ///
    /// `Missed` is the one that matters — it is what every tick over an empty
    /// screen answers, so a rule that took it would end the cycle on the tick
    /// after a read no matter what the sheet was doing. `Blind` is the fix
    /// round's: a failed grab reaches this rule through the same call and must
    /// not end anything. `HeldMiss` is POE-275's: one miss over a live sheet
    /// must not end the cycle (owner, 2026-09-11). Fails if the match is widened
    /// to "anything that is not a sighting".
    #[test]
    fn no_other_detect_outcome_completes_the_cycle() {
        for outcome in [
            DetectOutcome::Found,
            DetectOutcome::Held,
            DetectOutcome::Missed,
            DetectOutcome::HeldMiss,
            DetectOutcome::Blind,
        ] {
            assert!(!cycle_complete(outcome, true, true), "{outcome:?}");
        }
    }

    /// The cycle of a board already read ends on the SECOND consecutive miss
    /// after it, composed over the state machine as `miss` composes it
    /// (POE-275: two consecutive misses end the cycle, one does not).
    ///
    /// Fails if `RETIRE_AFTER` is put back to one (the first miss completes it)
    /// or raised past two (the second does not).
    #[test]
    fn a_read_board_completes_its_cycle_on_the_second_consecutive_miss() {
        let mut state = LoopState { live: true, ..LoopState::default() };
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        let first = state.on_detect(false);
        assert!(!cycle_complete(first, state.has_read(BOARD), true), "one miss: {first:?}");

        let second = state.on_detect(false);
        assert!(cycle_complete(second, state.has_read(BOARD), true), "two misses: {second:?}");
    }

    /// The read half is keyed on the CURRENT key, which is what makes Re-arm the
    /// way back: pressing it bumps `temple_rearm`, so the read in hand is a read
    /// of another key and the cycle is not complete.
    ///
    /// Fails if `has_read` answers "any read at all" — Re-arm would then arm the
    /// loop and the very next miss would stand it straight back down, which is
    /// the button doing nothing.
    #[test]
    fn a_read_of_another_key_is_not_a_read_of_this_one() {
        let mut state = LoopState::default();
        state.note_read(BOARD, &same_frame(), TempleStatus::Read, false);

        assert!(state.has_read(BOARD));
        assert!(!state.has_read(NEXT_BOARD));
    }

    /// And a loop that has read nothing at all has no read for any key. Fails if
    /// the `None` board answers `true`, which would stand a fresh loop down on
    /// its first miss.
    #[test]
    fn a_loop_that_has_read_nothing_has_no_read_for_this_key() {
        assert!(!LoopState::default().has_read(BOARD));
    }

    /// The slow-tick line is a measurement with a rate limit, not a switch:
    /// nothing at or inside the cadence, one line per `SLOW_TICK_LOG_EVERY`
    /// past it, and the stage breakdown on the line. Fails if a tick inside
    /// the cadence is reported, if two slow ticks inside one window both
    /// write, or if the line drops the stages that say WHERE the time went.
    #[test]
    fn a_slow_tick_is_logged_once_per_window_with_its_stages() {
        let mut said = None;
        let t0 = Instant::now();
        let stages = TickStages {
            capture: Duration::from_millis(1200),
            cheap: Duration::from_millis(300),
            anchor: Duration::ZERO,
        };

        assert_eq!(
            slow_tick_line(&mut said, t0, SLOW_TICK, &stages),
            None,
            "a tick at the cadence is not slow",
        );

        let line = slow_tick_line(&mut said, t0, Duration::from_millis(1519), &stages)
            .expect("a tick past the cadence writes");
        assert!(line.contains("1519 ms"), "{line}");
        assert!(line.contains("capture 1200 ms"), "{line}");
        assert!(line.contains("cheap detect 300 ms"), "{line}");

        assert_eq!(
            slow_tick_line(
                &mut said,
                t0 + SLOW_TICK_LOG_EVERY / 2,
                Duration::from_millis(1600),
                &stages
            ),
            None,
            "inside the window the fact is already on the log",
        );
        assert!(
            slow_tick_line(
                &mut said,
                t0 + SLOW_TICK_LOG_EVERY,
                Duration::from_millis(1600),
                &stages
            )
            .is_some(),
            "the window has passed",
        );
    }

    /// The stage timings a read line is built from, distinct per stage so a
    /// line that printed one under another's label is visible.
    fn read_stages() -> (TickStages, ReadStages) {
        (
            TickStages {
                capture: Duration::from_millis(35),
                cheap: Duration::from_millis(64),
                anchor: Duration::from_millis(96),
            },
            ReadStages {
                text_ocr: Duration::from_millis(165),
                plate_ocr: Duration::from_millis(102),
                markers: Duration::from_millis(6),
                // The 2026-09-06 release-build read's 193 ms `advise`, split
                // the way POE-257 WI-3 splits it: the valuation is nearly all
                // of it and the advisor's own ranking is the 13-22 ms
                // `advisor/mod.rs::the_conditional_ranking_cost_on_case_eight`
                // measures.
                valuation: Duration::from_millis(180),
                valuation_source: crate::temple::preset::ValuationSource::Cached,
                advise: Duration::from_millis(13),
                publish: Duration::from_millis(4),
            },
        )
    }

    /// The first look at a board is round 1 of 3 and reads everything — and the
    /// line still carries every stage measurement, which is what the owner's
    /// ~1 s budget is checked against.
    ///
    /// `retries_left` is what `note_read` leaves after a first look: the whole
    /// `RETRIES` budget. Fails on the mutation `round = RETRIES + 1 -
    /// retries_left` becoming a constant `1`, which the next test catches from
    /// the other side, and on `plan.describe()` being dropped from the line.
    #[test]
    fn the_read_line_names_a_first_look_as_round_one_of_three_full() {
        let (tick, read) = read_stages();

        let line = read_timings_line(
            &tick,
            &read,
            Duration::from_millis(666),
            1788567663863,
            false,
            Some(RETRIES),
            &slice::ReadPlan::full(),
        );

        assert!(line.contains("round 1 of 3: full;"), "{line}");
        assert!(line.contains("text ocr 165 ms, plates 102 ms, markers 6 ms"), "{line}");
        assert!(line.contains("666 ms from grab to publish"), "{line}");
        assert!(line.ends_with("clean"), "{line}");
    }

    /// The valuation is its OWN field beside the advisor's ranking, in the
    /// slot the single `advise` field used to occupy (POE-257 WI-3).
    ///
    /// Why the run of four stages is asserted as one literal rather than
    /// each in isolation: the numbers in the fixture are distinct per stage,
    /// so a line that printed the advisor's milliseconds under `valuation` —
    /// or that folded the two back into one number — reads exactly like the
    /// line this WI was opened to correct, and only the ORDER catches it.
    ///
    /// Fails on the mutation swapping `ms(read.valuation)` and
    /// `ms(read.advise)` in `read_timings_line`'s argument list: the line
    /// would say `valuation 13 ms (cached), advise 180 ms`.
    #[test]
    fn the_read_line_measures_the_valuation_apart_from_the_ranking() {
        let (tick, read) = read_stages();

        let line = read_timings_line(
            &tick,
            &read,
            Duration::from_millis(666),
            1788567663863,
            false,
            Some(RETRIES),
            &slice::ReadPlan::full(),
        );

        assert!(
            line.contains(
                "markers 6 ms, valuation 180 ms (cached), advise 13 ms, publish 4 ms"
            ),
            "{line}",
        );
    }

    /// A read that had to BUILD the table says so, and the word is the whole
    /// point of the field: the milliseconds alone cannot distinguish a cache
    /// that is working from one that is missing on every read on a fast
    /// machine, which is the regression this WI can otherwise only be
    /// suspected of.
    ///
    /// Fails on the mutation making `preset::ValuationSource::word` answer
    /// `"cached"` for both variants.
    #[test]
    fn the_read_line_says_computed_when_the_read_built_the_valuation() {
        let (tick, mut read) = read_stages();
        read.valuation_source = crate::temple::preset::ValuationSource::Computed;

        let line = read_timings_line(
            &tick,
            &read,
            Duration::from_millis(666),
            1788567663863,
            false,
            Some(RETRIES),
            &slice::ReadPlan::full(),
        );

        assert!(line.contains("valuation 180 ms (computed),"), "{line}");
        assert!(!line.contains("(cached)"), "{line}");
    }

    /// A retry round names its number AND the regions it re-read, which is the
    /// only place the partial round's cost is visible: `plates 12 ms` beside
    /// `re-read 2 plates` is the measurement of what skipping the other eleven
    /// bought.
    ///
    /// The round is derived from the retry budget AFTER `note_read` spent this
    /// read's share, so one retry left is round 2. Fails on a constant round
    /// number, on the arithmetic being inverted (`RETRIES + 1 - 1` would print
    /// round 2 for the first look as well), and on the plan's regions being
    /// left off the line.
    #[test]
    fn the_read_line_names_a_retry_round_and_the_regions_it_re_read() {
        let (tick, read) = read_stages();
        let plan = slice::ReadPlan {
            plates: vec![lattice::Slot::C1, lattice::Slot::D2],
            text: vec![slice::PANEL_REGION],
            markers: false,
        };

        let line = read_timings_line(
            &tick,
            &read,
            Duration::from_millis(210),
            1788567664551,
            true,
            Some(1),
            &plan,
        );

        assert!(line.contains("round 2 of 3: re-read 2 plates, panel;"), "{line}");
        assert!(line.ends_with("unclean, 1 retries left"), "{line}");
    }

    /// The last round the budget allows says so, and a plan of the diamond
    /// alone names the diamond. Fails if `describe` prints a region the plan
    /// did not carry, or if the round stops counting at 2.
    #[test]
    fn the_read_line_names_the_last_round_and_a_markers_only_plan() {
        let (tick, read) = read_stages();
        let plan = slice::ReadPlan {
            plates: Vec::new(),
            text: Vec::new(),
            markers: true,
        };

        let line = read_timings_line(
            &tick,
            &read,
            Duration::from_millis(120),
            1788567665204,
            true,
            Some(0),
            &plan,
        );

        assert!(line.contains("round 3 of 3: re-read markers;"), "{line}");
    }

    /// One plate is `re-read 1 plate`, not `1 plates` — the literal
    /// docs/OVERLAY-GUIDE.md's smoke check reads off `app.log` for the round a
    /// single covered plate buys, so the singular is part of the check and not
    /// a nicety.
    ///
    /// Fails on the mutation dropping `slice::ReadPlan::describe`'s `1 =>` arm:
    /// the count would fall through to the plural `{n} plates`.
    #[test]
    fn the_read_line_puts_a_single_re_read_plate_in_the_singular() {
        let (tick, read) = read_stages();
        let plan = slice::ReadPlan {
            plates: vec![lattice::Slot::C1],
            text: Vec::new(),
            markers: false,
        };

        let line = read_timings_line(
            &tick,
            &read,
            Duration::from_millis(180),
            1788567664551,
            true,
            Some(1),
            &plan,
        );

        assert!(line.contains("round 2 of 3: re-read 1 plate;"), "{line}");
    }

    /// A round bought by a region whose crop then fell off the capture plans
    /// nothing, and the line says `nothing left to re-read` — the other literal
    /// the smoke check names, and the one that tells a reader the round cost an
    /// anchor resolve and no OCR rather than that the line lost its regions.
    ///
    /// Fails on the mutation dropping `slice::ReadPlan::describe`'s
    /// `is_empty()` arm: the empty plan would fall through to the join and
    /// print a bare `re-read `.
    #[test]
    fn the_read_line_names_a_round_with_nothing_left_to_re_read() {
        let (tick, read) = read_stages();
        let plan = slice::ReadPlan {
            plates: Vec::new(),
            text: Vec::new(),
            markers: false,
        };

        let line = read_timings_line(
            &tick,
            &read,
            Duration::from_millis(101),
            1788567665204,
            false,
            Some(1),
            &plan,
        );

        assert!(line.contains("round 2 of 3: nothing left to re-read;"), "{line}");
    }

    // ------------------------------------------------ the cheap detect gate --

    /// A cheap outcome that saw nothing, for the gate tests.
    fn saw_nothing() -> anchor::CheapDetect {
        anchor::CheapDetect::Nothing { best_ncc: 0.2 }
    }

    /// Every source the arm gate can name, for the tests that must hold under
    /// all of them.
    const ALL_SOURCES: [Option<trigger::ArmSource>; 6] = [
        Some(trigger::ArmSource::Trigger(trigger::ArmReason::AlvaStart)),
        Some(trigger::ArmSource::Trigger(trigger::ArmReason::TempleArea)),
        Some(trigger::ArmSource::Trigger(trigger::ArmReason::Manual)),
        Some(trigger::ArmSource::PanelOnScreen),
        Some(trigger::ArmSource::StartupProbe),
        None,
    ];
    const ALVA: Option<trigger::ArmSource> =
        Some(trigger::ArmSource::Trigger(trigger::ArmReason::AlvaStart));
    const MANUAL: Option<trigger::ArmSource> =
        Some(trigger::ArmSource::Trigger(trigger::ArmReason::Manual));

    /// One clean miss with nothing live on a NULL slice, under `key`, with
    /// nothing in flight — the only tick that counts toward a null sweep.
    fn null_miss(budget: &mut SweepBudget, key: (u64, u64)) -> Option<SweepStart> {
        cold_sweep_reason(false, None, DetectOutcome::Missed, false, ALVA, key, budget)
    }

    /// The null-slice start the cadence answers for the `k`-th sweep of a key.
    fn null_start(k: u8) -> Option<SweepStart> {
        Some(SweepStart { reason: ColdSweepReason::NullSlice, attempt: Some(k) })
    }

    /// Three misses: the ticks that start one null sweep from a count of zero.
    fn spend_one_null_sweep(budget: &mut SweepBudget, key: (u64, u64)) -> Option<SweepStart> {
        null_miss(budget, key);
        null_miss(budget, key);
        null_miss(budget, key)
    }

    /// The placed path, unchanged by POE-275 (owner: *"keep that"*): Re-arm
    /// buys one sweep per key, and no other source buys one. It runs off the
    /// loop like any sweep, and like any sweep it starts only on a miss with
    /// nothing live and nothing in flight.
    ///
    /// Fails if an incursion arm buys the sweep again (the 2026-09-09 30–34 s
    /// wait), if Re-arm's budget is not per key, or if a held miss or a sweep
    /// already in flight is let through.
    #[test]
    fn a_below_floor_placed_miss_spends_one_fallback_under_re_arm_only() {
        let placed = Some((960, 713));
        let placed_miss = Some(SweepStart { reason: ColdSweepReason::PlacedMiss, attempt: None });
        let mut budget = SweepBudget::default();
        let missed = DetectOutcome::Missed;

        assert_eq!(
            cold_sweep_reason(true, placed, missed, false, MANUAL, BOARD, &mut budget),
            placed_miss,
            "Re-arm qualifies a placed miss",
        );
        assert_eq!(
            cold_sweep_reason(true, placed, missed, false, MANUAL, BOARD, &mut budget),
            None,
            "Re-arm spends only once per key",
        );
        assert_eq!(
            cold_sweep_reason(true, placed, missed, false, MANUAL, NEXT_BOARD, &mut budget),
            placed_miss,
            "a new key has its own fallback",
        );

        // An incursion arm's first miss is the sheet not being open yet
        // (2026-09-09), and the rest are not Re-arm's "look again".
        for source in ALL_SOURCES.into_iter().filter(|source| *source != MANUAL) {
            let mut budget = SweepBudget::default();
            assert_eq!(
                cold_sweep_reason(true, placed, missed, false, source, BOARD, &mut budget),
                None,
                "{source:?} does not qualify a placed fallback",
            );
        }

        let mut budget = SweepBudget::default();
        assert_eq!(
            cold_sweep_reason(
                true,
                placed,
                DetectOutcome::HeldMiss,
                false,
                MANUAL,
                BOARD,
                &mut budget,
            ),
            None,
            "a held miss is a live panel, not a wrong placement",
        );
        assert_eq!(
            cold_sweep_reason(true, placed, missed, true, MANUAL, BOARD, &mut budget),
            None,
            "one sweep in flight at most",
        );
        assert_eq!(
            cold_sweep_reason(true, placed, missed, false, MANUAL, BOARD, &mut budget),
            placed_miss,
            "neither refusal spent Re-arm's one sweep",
        );
    }

    /// The fix (owner, 2026-09-11): on a null slice the first miss after an arm
    /// is the sheet not being open yet, so misses one and two start nothing and
    /// the third starts exactly one sweep — under every arm source, the
    /// start-up probe's single tick included.
    ///
    /// Fails if the sweep is spent on the first miss again (the friend's first
    /// run), if `NULL_SWEEP_EVERY` moves, or if the null path is gated on the arm.
    #[test]
    fn a_null_slice_starts_its_first_sweep_on_the_third_consecutive_miss_under_every_arm() {
        for source in ALL_SOURCES {
            let mut budget = SweepBudget::default();
            let missed = DetectOutcome::Missed;
            let mut miss =
                || cold_sweep_reason(false, None, missed, false, source, BOARD, &mut budget);

            assert_eq!(miss(), None, "{source:?}: miss 1");
            assert_eq!(miss(), None, "{source:?}: miss 2");
            assert_eq!(miss(), null_start(1), "{source:?}: miss 3");
        }
    }

    /// A screen with a seeded but never-ANCHORED origin is a null slice for this
    /// rule too (POE-278: a seed is not a placement), and it waits the same
    /// three misses.
    ///
    /// Fails if `screen_present` alone is read as placed — the miss would go to
    /// the placed arm, which buys nothing under AlvaStart.
    #[test]
    fn an_unanchored_screen_waits_the_same_three_misses() {
        let mut budget = SweepBudget::default();
        let mut miss = || {
            cold_sweep_reason(true, None, DetectOutcome::Missed, false, ALVA, BOARD, &mut budget)
        };

        assert_eq!((miss(), miss(), miss()), (None, None, null_start(1)));
    }

    /// The next null sweep needs `NULL_SWEEP_EVERY` misses after the previous
    /// one ENDED: the count restarts when a sweep starts, and misses that land
    /// while it is still in flight do not advance it.
    ///
    /// Fails if the count is not reset at the start (the first miss after the
    /// sweep would start the next one) or if in-flight misses count (a 5.3 s
    /// sweep spans eight ticks, so the loop would sweep back-to-back).
    #[test]
    fn the_next_null_sweep_needs_three_misses_after_the_last_one_ended() {
        let mut budget = SweepBudget::default();
        assert_eq!(spend_one_null_sweep(&mut budget, BOARD), null_start(1), "precondition");
        let missed = DetectOutcome::Missed;
        for _ in 0..8 {
            assert_eq!(
                cold_sweep_reason(false, None, missed, true, ALVA, BOARD, &mut budget),
                None,
                "a miss during the sweep",
            );
        }

        assert_eq!(null_miss(&mut budget, BOARD), None, "miss 1 after it ended");
        assert_eq!(null_miss(&mut budget, BOARD), None, "miss 2 after it ended");
        assert_eq!(null_miss(&mut budget, BOARD), null_start(2), "miss 3 after it ended");
    }

    /// `NULL_SWEEP_CAP` sweeps per key, and not one more however long the loop
    /// goes on missing.
    ///
    /// Fails if the cap is dropped (an unplaceable screen sweeps for the whole
    /// portal wait, which has been measured at 22 min) or is off by one either
    /// way — the tenth must start.
    #[test]
    fn no_null_sweep_starts_past_the_cap() {
        let mut budget = SweepBudget::default();
        for k in 1..=NULL_SWEEP_CAP {
            assert_eq!(spend_one_null_sweep(&mut budget, BOARD), null_start(k), "sweep {k}");
        }

        for miss in 0..3 * u32::from(NULL_SWEEP_EVERY) {
            assert_eq!(null_miss(&mut budget, BOARD), None, "miss {miss} past the cap");
        }
    }

    /// A key change — Re-arm, a new epoch — gives the new key a whole count and
    /// a whole cap.
    ///
    /// Fails if either is carried across: two misses left over under the old
    /// key would start the new key's sweep on its FIRST miss, and a spent cap
    /// would leave the new key unable to sweep at all, which is the bug the
    /// owner's friend hit, one key later.
    #[test]
    fn a_key_change_resets_the_null_count_and_cap() {
        let mut budget = SweepBudget::default();
        for _ in 0..NULL_SWEEP_CAP {
            spend_one_null_sweep(&mut budget, BOARD);
        }
        null_miss(&mut budget, BOARD);
        null_miss(&mut budget, BOARD);

        assert_eq!(null_miss(&mut budget, NEXT_BOARD), None, "miss 1 under the new key");
        assert_eq!(null_miss(&mut budget, NEXT_BOARD), None, "miss 2 under the new key");
        assert_eq!(null_miss(&mut budget, NEXT_BOARD), null_start(1), "miss 3 under the new key");
    }

    /// Only the loop searching with nothing live counts. A held miss is a live
    /// panel (a tooltip, a misread), a retire is the sheet closing, a sighting is
    /// the sheet, and a blind tick saw nothing at all: none starts a sweep and
    /// none advances the count.
    ///
    /// Arranged two misses in, so an outcome that advanced the count would start
    /// the sweep itself. Fails if the start is widened to "any tick that did not
    /// anchor", or if the count is advanced before the outcome is checked.
    #[test]
    fn only_a_miss_with_nothing_live_counts_toward_a_null_sweep() {
        for outcome in [
            DetectOutcome::HeldMiss,
            DetectOutcome::Retired,
            DetectOutcome::Blind,
            DetectOutcome::Found,
            DetectOutcome::Held,
        ] {
            let mut budget = SweepBudget::default();
            null_miss(&mut budget, BOARD);
            null_miss(&mut budget, BOARD);

            assert_eq!(
                cold_sweep_reason(false, None, outcome, false, ALVA, BOARD, &mut budget),
                None,
                "{outcome:?} starts nothing",
            );
            assert_eq!(
                null_miss(&mut budget, BOARD),
                null_start(1),
                "{outcome:?} left the count at two",
            );
        }
    }

    /// The misses have to be CONSECUTIVE: a sighting between them starts the
    /// count again.
    ///
    /// Fails if `on_sighting` leaves the count standing — two misses either side
    /// of a sheet the loop was reading would start a sweep on the first miss
    /// after it.
    #[test]
    fn a_sighting_starts_the_null_count_again() {
        let mut budget = SweepBudget::default();
        null_miss(&mut budget, BOARD);
        null_miss(&mut budget, BOARD);

        budget.on_sighting();

        assert_eq!(null_miss(&mut budget, BOARD), None, "miss 1 after the sighting");
        assert_eq!(null_miss(&mut budget, BOARD), None, "miss 2 after the sighting");
        assert_eq!(null_miss(&mut budget, BOARD), null_start(1), "miss 3 after the sighting");
    }

    // ------------------------------------------- the sweep, off the loop --

    /// How long a test waits on a sweep thread before failing, so a broken
    /// cancel fails the test rather than hanging it.
    const PATIENCE: Duration = Duration::from_secs(5);

    fn placed_start() -> SweepStart {
        SweepStart { reason: ColdSweepReason::PlacedMiss, attempt: None }
    }

    /// An anchor at `origin`, as a sweep or a recheck reports one.
    fn anchor_at(origin: (i32, i32)) -> anchor::Anchor {
        anchor::Anchor { origin, scale: 1.0, ncc: 0.95 }
    }

    /// Launch a sweep that searches until its stop check fires — the pyramid
    /// polls it between coarse correlations — says whether it saw the stop,
    /// then waits for the test's word and answers `late`.
    ///
    /// Returns the receiver that hears whether the stop fired, and the sender
    /// that lets the sweep answer; dropping the sender lets the thread end.
    fn searching_sweep(
        slot: &mut SweepSlot,
        key: (u64, u64),
        late: Option<anchor::Anchor>,
    ) -> (mpsc::Receiver<bool>, mpsc::Sender<()>) {
        let (saw_stop, heard) = mpsc::channel();
        let (answer, answered) = mpsc::channel::<()>();
        slot.launch(placed_start(), key, (1920, 1080), move |stop| {
            let give_up = Instant::now() + PATIENCE;
            while !stop() && Instant::now() < give_up {
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = saw_stop.send(stop());
            let _ = answered.recv_timeout(PATIENCE);
            (late, stop())
        })
        .expect("the sweep thread starts");
        (heard, answer)
    }

    /// Settle ticks with no recheck until the sweep in flight has answered.
    fn settle_when_answered(
        slot: &mut SweepSlot,
        confirm: impl Fn(&anchor::Anchor, (u32, u32)) -> Option<anchor::Anchor>,
    ) -> (Sighting, Option<SweepReport>) {
        let give_up = Instant::now() + PATIENCE;
        loop {
            let settled = slot.settle(&saw_nothing(), &confirm);
            if settled.1.is_some() || Instant::now() >= give_up {
                return settled;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Requirement 4c (POE-275): a placed recheck that anchors while a sweep
    /// is searching cancels it, and the tick uses the RECHECK's origin.
    ///
    /// A real thread whose search blocks until its stop check fires, so what is
    /// pinned is the contract across the thread: the flag the loop sets is the
    /// one the search reads. Fails if the recheck does not win, if the sweep is
    /// not reported cancelled by it, or if the stop never reaches the search —
    /// the sweep would then run its full 5.3 s (~30 s debug) for nothing.
    #[test]
    fn a_recheck_landing_mid_sweep_wins_and_cancels_the_sweep() {
        let mut slot = SweepSlot::default();
        let (heard, _answer) = searching_sweep(&mut slot, BOARD, Some(anchor_at((745, 561))));
        let recheck = anchor_at((960, 713));

        let (sighting, ended) = slot.settle(&anchor::CheapDetect::Anchored(recheck), |_, _| {
            panic!("a landed recheck confirms no sweep")
        });

        assert_eq!(sighting, Sighting::Recheck(recheck));
        assert_eq!(ended.map(|report| report.end), Some(SweepEnd::Cancelled(SweepCancel::Recheck)));
        assert_eq!(heard.recv_timeout(PATIENCE), Ok(true), "the search saw its stop check fire");
    }

    /// …and the sweep's LATE answer, sent after the recheck landed, is never
    /// used: the tick after it is a plain miss with nothing to report.
    ///
    /// Fails if a cancelled flight is kept in the slot — the late origin would
    /// be confirmed and read over the recheck's.
    #[test]
    fn a_sweep_answer_arriving_after_the_recheck_landed_is_discarded() {
        let mut slot = SweepSlot::default();
        let (heard, answer) = searching_sweep(&mut slot, BOARD, Some(anchor_at((745, 561))));
        slot.settle(&anchor::CheapDetect::Anchored(anchor_at((960, 713))), |_, _| None);
        heard.recv_timeout(PATIENCE).expect("precondition: the search saw its stop");
        answer.send(()).expect("precondition: the sweep is still there to answer");
        // Time for the late answer to be sent where a kept flight would read it.
        std::thread::sleep(Duration::from_millis(100));

        let (sighting, ended) = slot.settle(&saw_nothing(), |swept, _| Some(*swept));

        assert_eq!((sighting, ended), (Sighting::Nothing, None));
    }

    /// A sweep still searching leaves the tick a miss and stays in flight: the
    /// loop goes on ticking, and the recheck goes on running, while it
    /// searches.
    ///
    /// Fails if a tick with no recheck cancels the sweep or waits for it.
    #[test]
    fn a_sweep_still_searching_leaves_the_tick_a_miss_and_stays_in_flight() {
        let mut slot = SweepSlot::default();
        let (_heard, _answer) = searching_sweep(&mut slot, BOARD, None);

        let (sighting, ended) = slot.settle(&saw_nothing(), |_, _| None);

        assert_eq!((sighting, ended), (Sighting::Nothing, None));
        assert!(slot.in_flight());
    }

    /// A sweep belongs to the key it started under: a Re-arm or a new epoch
    /// cancels it, and the stop reaches the search.
    ///
    /// Fails if the key is not compared (a sweep of the last board would be
    /// read into the next) or compared the wrong way round (every tick would
    /// cancel the sweep it started).
    #[test]
    fn a_key_change_cancels_a_running_sweep() {
        let mut slot = SweepSlot::default();
        let (heard, _answer) = searching_sweep(&mut slot, BOARD, None);
        assert_eq!(slot.keep_only(BOARD), None, "precondition: its own key keeps it");

        let ended = slot.keep_only(NEXT_BOARD);

        assert_eq!(
            ended.map(|report| report.end),
            Some(SweepEnd::Cancelled(SweepCancel::KeyChange)),
        );
        assert_eq!(heard.recv_timeout(PATIENCE), Ok(true), "the search saw its stop check fire");
    }

    /// A stand-down cancels the sweep: nothing is looking any more, so nothing
    /// would read its answer. Fails if the cancel does not reach the search.
    #[test]
    fn a_stand_down_cancels_a_running_sweep() {
        let mut slot = SweepSlot::default();
        let (heard, _answer) = searching_sweep(&mut slot, BOARD, None);

        let ended = slot.cancel(SweepCancel::StandDown);

        assert_eq!(
            ended.map(|report| report.end),
            Some(SweepEnd::Cancelled(SweepCancel::StandDown)),
        );
        assert_eq!(heard.recv_timeout(PATIENCE), Ok(true), "the search saw its stop check fire");
    }

    /// A sweep that FOUND the panel is not read when the capture after it does
    /// not confirm the origin — the sheet closed while it searched. The tick is
    /// a miss, and the line says the find was discarded.
    ///
    /// Fails if the swept origin is read without asking the current frame.
    #[test]
    fn a_found_sweep_the_current_frame_does_not_confirm_is_not_read() {
        let mut slot = SweepSlot::default();
        let swept = anchor_at((745, 561));
        slot.launch(placed_start(), BOARD, (1920, 1080), move |_| (Some(swept), false))
            .expect("the sweep thread starts");

        let (sighting, ended) = settle_when_answered(&mut slot, |_, _| None);

        assert_eq!(sighting, Sighting::Nothing);
        assert_eq!(
            ended.map(|report| report.end),
            Some(SweepEnd::Found { origin: (745, 561), scale: 1.0, fate: FoundFate::Unconfirmed }),
        );
    }

    /// A confirmed sweep is read at the origin the CURRENT frame re-found, and
    /// the confirmation was asked about the capture size the sweep searched.
    ///
    /// Fails if the tick reads the sweep's own origin (a frame a whole sweep
    /// old) rather than the confirmation's, or asks about the wrong capture.
    #[test]
    fn a_confirmed_sweep_is_read_at_the_origin_the_current_frame_confirms() {
        let mut slot = SweepSlot::default();
        let swept = anchor_at((745, 561));
        slot.launch(placed_start(), BOARD, (2560, 1440), move |_| (Some(swept), false))
            .expect("the sweep thread starts");
        let asked = std::cell::Cell::new(None);
        let refound = anchor_at((746, 563));

        let (sighting, _) = settle_when_answered(&mut slot, |origin, capture| {
            asked.set(Some((origin.origin, capture)));
            Some(refound)
        });

        assert_eq!(
            sighting,
            Sighting::Swept { anchor: refound, reason: ColdSweepReason::PlacedMiss },
        );
        assert_eq!(asked.get(), Some(((745, 561), (2560, 1440))));
    }

    /// A search that returns because its stop fired — the module stopping,
    /// which the slot's own flag does not see — is reported as stopped, not as
    /// a screen with no panel on it. `anchor_for_loop` answers both with the
    /// same error; `stopped` is what tells them apart.
    ///
    /// Fails if `settle` reads a stopped search's `None` as `found no layout
    /// panel`.
    #[test]
    fn a_search_that_returns_after_its_stop_fired_is_reported_as_stopped() {
        let mut slot = SweepSlot::default();
        slot.launch(placed_start(), BOARD, (1920, 1080), |_| (None, true))
            .expect("the sweep thread starts");

        let (sighting, ended) = settle_when_answered(&mut slot, |_, _| None);

        assert_eq!(
            (sighting, ended.map(|report| report.end)),
            (Sighting::Nothing, Some(SweepEnd::Cancelled(SweepCancel::Stop))),
        );
    }

    /// An anchored recheck with nothing in flight is the tick's sighting and
    /// buys no sweep: nothing is reported and nothing starts searching.
    ///
    /// Fails if a recheck sighting is folded as anything but itself.
    #[test]
    fn an_anchored_recheck_buys_no_sweep() {
        let mut slot = SweepSlot::default();
        let found = anchor_at((960, 713));

        let settled = slot.settle(&anchor::CheapDetect::Anchored(found), |_, _| {
            panic!("nothing is in flight to confirm")
        });

        assert_eq!(settled, (Sighting::Recheck(found), None));
        assert!(!slot.in_flight());
    }

    /// A sweep that searched the whole capture and found no layout panel ends
    /// its flight with the no-panel line, so the slot is free for the next
    /// sweep the cadence starts.
    ///
    /// Fails if the flight is put back after a no-panel answer — the slot would
    /// stay full and no sweep could ever start again in the session.
    #[test]
    fn a_sweep_that_found_nothing_ends_the_flight() {
        let mut slot = SweepSlot::default();
        slot.launch(placed_start(), BOARD, (1920, 1080), |_| (None, false))
            .expect("the sweep thread starts");

        let (_, ended) = settle_when_answered(&mut slot, |_, _| None);

        assert_eq!(ended.map(|report| report.end), Some(SweepEnd::NoPanel));
        assert!(!slot.in_flight());
    }

    /// A sweep that answered FOUND on the very tick the placed recheck landed
    /// loses to the recheck: the tick reads the recheck's origin and the line
    /// says the find was superseded.
    ///
    /// Fails if a confirmed sweep beats the recheck it arrived beside.
    #[test]
    fn a_found_answer_on_the_recheck_tick_is_superseded() {
        let mut slot = SweepSlot::default();
        let swept = anchor_at((745, 561));
        let (returning, returned) = mpsc::channel();
        slot.launch(placed_start(), BOARD, (1920, 1080), move |_| {
            let _ = returning.send(());
            (Some(swept), false)
        })
        .expect("the sweep thread starts");
        returned.recv_timeout(PATIENCE).expect("precondition: the sweep is answering");
        // Time for the answer to land in the channel the tick polls.
        std::thread::sleep(Duration::from_millis(100));
        let recheck = anchor_at((960, 713));

        let (sighting, ended) =
            slot.settle(&anchor::CheapDetect::Anchored(recheck), |swept, _| Some(*swept));

        assert_eq!(sighting, Sighting::Recheck(recheck));
        assert_eq!(
            ended.map(|report| report.end),
            Some(SweepEnd::Found { origin: (745, 561), scale: 1.0, fate: FoundFate::Superseded }),
        );
    }

    /// A sweep thread that dies without answering ends the flight and says so,
    /// rather than holding the one slot for the rest of the key.
    ///
    /// Fails if a disconnected channel is read as "still searching".
    #[test]
    fn a_sweep_thread_that_dies_ends_the_flight_and_says_so() {
        let mut slot = SweepSlot::default();
        slot.launch(placed_start(), BOARD, (1920, 1080), |_| panic!("the sweep thread dies"))
            .expect("the sweep thread starts");

        let (sighting, ended) = settle_when_answered(&mut slot, |_, _| None);

        assert_eq!(
            (sighting, ended.map(|report| report.end)),
            (Sighting::Nothing, Some(SweepEnd::Lost)),
        );
        assert!(!slot.in_flight());
    }

    /// The confirmation on real pixels: the committed 1920x1080 frame still
    /// shows the sheet at the recorded anchor, so a sweep that found it there is
    /// confirmed, at an origin inside the recheck window.
    ///
    /// Fails if the confirmation asks anything but the one windowed recheck at
    /// the swept origin and scale.
    #[test]
    fn a_swept_origin_is_confirmed_on_a_frame_that_still_shows_the_sheet() {
        let img = live_capture();
        let swept =
            anchor::Anchor { origin: LIVE_CAPTURE_ORIGIN, scale: LIVE_CAPTURE_SCALE, ncc: 0.99 };

        let confirmed =
            confirm_swept(&img, &swept, (1920, 1080)).expect("the sheet is on this frame");

        assert!(
            confirmed.origin.0.abs_diff(LIVE_CAPTURE_ORIGIN.0) <= 3
                && confirmed.origin.1.abs_diff(LIVE_CAPTURE_ORIGIN.1) <= 3,
            "confirmed at {:?}",
            confirmed.origin,
        );
        assert!(confirmed.ncc >= anchor::NCC_FLOOR);
    }

    /// A frame where the swept origin shows no Entrance plate — the sheet
    /// closed, or never was where the sweep saw it — confirms nothing.
    ///
    /// Fails if the confirmation trusts the swept anchor instead of the frame.
    #[test]
    fn a_swept_origin_is_not_confirmed_where_the_frame_shows_no_sheet() {
        let img = live_capture();
        let swept = anchor::Anchor { origin: (300, 300), scale: LIVE_CAPTURE_SCALE, ncc: 0.99 };

        assert_eq!(confirm_swept(&img, &swept, (1920, 1080)), None);
    }

    /// A swept origin from a capture of another size is another screen's
    /// origin, and is not confirmed even where this frame does show the sheet.
    ///
    /// Fails if the confirmation takes the size from the current frame instead
    /// of from the capture the sweep searched.
    #[test]
    fn a_swept_origin_from_another_capture_size_is_not_confirmed() {
        let img = live_capture();
        let swept =
            anchor::Anchor { origin: LIVE_CAPTURE_ORIGIN, scale: LIVE_CAPTURE_SCALE, ncc: 0.99 };

        assert_eq!(confirm_swept(&img, &swept, (2560, 1440)), None);
    }

    // --------------------------------------------- the one line per sweep --

    /// The line carries every field the measurement needs, in one fixed shape:
    /// the reason, the attempt of the cap, the capture, the duration, the
    /// outcome and the build.
    ///
    /// Fails if a field is dropped or crossed with another — each is a number a
    /// reader of `app.log` corrects `NULL_SWEEP_EVERY` or `NULL_SWEEP_CAP` from.
    #[test]
    fn the_sweep_line_carries_reason_attempt_capture_duration_outcome_and_build() {
        let report = SweepReport {
            start: null_start(3).expect("a null start"),
            capture: (1920, 1080),
            took: Duration::from_millis(5312),
            end: SweepEnd::Found { origin: (960, 713), scale: 1.0, fate: FoundFate::Confirmed },
        };

        assert_eq!(
            sweep_line(&report, "release"),
            "Temple: cold sweep (NullSlice, attempt 3 of 10) at 1920x1080 — 5312 ms, found at \
             (960,713) scale 1.000 — confirmed on the current frame; release build",
        );
    }

    /// The placed sweep has no cap to count against, so its line names no
    /// attempt; and a sweep that found nothing says so in the words the retired
    /// `sweep found no layout panel` line used.
    ///
    /// Fails if the placed line invents an attempt, or the no-panel outcome is
    /// worded as a find.
    #[test]
    fn a_placed_sweep_that_found_nothing_names_no_attempt() {
        let report = SweepReport {
            start: placed_start(),
            capture: (2560, 1440),
            took: Duration::from_millis(30_112),
            end: SweepEnd::NoPanel,
        };

        assert_eq!(
            sweep_line(&report, "debug"),
            "Temple: cold sweep (PlacedMiss) at 2560x1440 — 30112 ms, found no layout panel; \
             debug build",
        );
    }

    /// Every way a sweep ends that is not a confirmed find is named apart, so a
    /// log can tell a recheck that won from a sheet that closed.
    ///
    /// Fails if two endings share words.
    #[test]
    fn the_sweep_line_names_every_other_ending_apart() {
        let endings = [
            (SweepEnd::Cancelled(SweepCancel::Recheck), "cancelled by recheck;"),
            (SweepEnd::Cancelled(SweepCancel::KeyChange), "cancelled by key change;"),
            (SweepEnd::Cancelled(SweepCancel::StandDown), "cancelled by stand-down;"),
            (SweepEnd::Cancelled(SweepCancel::Stop), "cancelled by stop;"),
            (SweepEnd::Lost, "ended without a result;"),
            (
                SweepEnd::Found { origin: (1, 2), scale: 1.0, fate: FoundFate::Unconfirmed },
                "not on the current frame, discarded;",
            ),
            (
                SweepEnd::Found { origin: (1, 2), scale: 1.0, fate: FoundFate::Superseded },
                "discarded, the placed recheck landed;",
            ),
        ];

        for (end, words) in endings {
            let report = SweepReport {
                start: placed_start(),
                capture: (1920, 1080),
                took: Duration::from_millis(1),
                end,
            };
            let line = sweep_line(&report, "release");
            assert!(line.contains(words), "{end:?}: {line}");
        }
    }

    /// The build word is `debug` exactly when debug assertions are on — which
    /// is what `log_sweep` hands it — because the same sweep is 5.3 s on one
    /// and ~30 s on the other. Fails if the two words are swapped.
    #[test]
    fn the_build_word_follows_debug_assertions() {
        assert_eq!((build_profile(true), build_profile(false)), ("debug", "release"));
    }

    #[test]
    fn a_sweep_elsewhere_logs_the_contradiction() {
        let line = placed_origin_contradiction_line(Some((960, 713)), (745, 561));

        assert_eq!(
            line.as_deref(),
            Some("temple: placed origin (960,713) contradicted by sweep (745,561)"),
        );
    }

    #[test]
    fn a_sweep_elsewhere_remembers_after_successful_read() {
        let placed = (960, 713);
        let swept = (745, 561);
        let remembered = std::cell::Cell::new(None);

        remember_fallback_anchor(placed_origin_contradiction(Some(placed), swept), |origin| {
            remembered.set(Some(origin));
        });

        assert_eq!(remembered.get(), Some([745, 561]));
    }

    #[test]
    fn a_null_sweep_origin_is_remembered_after_successful_read() {
        let swept = (745, 561);
        let remembered = std::cell::Cell::new(None);

        assert_eq!(placed_origin_contradiction(None, swept), Some(swept));
        remember_fallback_anchor(placed_origin_contradiction(None, swept), |origin| {
            remembered.set(Some(origin));
        });

        assert_eq!(remembered.get(), Some([745, 561]));
    }

    #[test]
    fn a_placed_origin_inside_the_tolerance_is_not_a_contradiction() {
        let placed = (960, 713);

        assert_eq!(placed_origin_contradiction(Some(placed), placed), None);
        assert_eq!(
            placed_origin_contradiction(Some(placed), (placed.0 + 1, placed.1)),
            None,
        );
        assert_eq!(
            placed_origin_contradiction_line(Some(placed), (placed.0 + 1, placed.1)),
            None,
        );
    }

    /// POE-269's withheld rule, kept on top of the cadence (POE-275 WI-2): a
    /// found null sweep whose slice was withheld leaves the key's null sweeps
    /// going once; a second one ENDS them — for that key only.
    ///
    /// Fails if a withheld result ends the key at once (the one retry is gone),
    /// if the second one does not end it (an anchor the screen never
    /// corroborates is swept for up to the whole cap), or if the release is
    /// remembered across keys (the next key's first withheld result would end
    /// it).
    #[test]
    fn a_second_withheld_null_sweep_ends_that_keys_null_sweeps() {
        let withheld = screen_from_anchor(
            1.25,
            None,
            (1920, 1080),
            7,
            (745, 561),
            [0, 0, 1920, 1080],
            1_700_000_000_002,
        );
        assert!(withheld.is_err(), "the height check withholds this anchor");
        let mut budget = SweepBudget::default();
        spend_one_null_sweep(&mut budget, BOARD);

        budget.after_null_publish(BOARD, false);
        assert_eq!(
            spend_one_null_sweep(&mut budget, BOARD),
            null_start(2),
            "the first withheld result leaves the cadence going",
        );

        budget.after_null_publish(BOARD, false);
        for miss in 0..3 * u32::from(NULL_SWEEP_EVERY) {
            assert_eq!(null_miss(&mut budget, BOARD), None, "miss {miss} after the second");
        }

        spend_one_null_sweep(&mut budget, NEXT_BOARD);
        budget.after_null_publish(NEXT_BOARD, false);
        assert_eq!(
            spend_one_null_sweep(&mut budget, NEXT_BOARD),
            null_start(2),
            "a new key has its own one withheld retry",
        );
    }

    /// A found null sweep that FILLED the slice ends the key's null sweeps, as
    /// every null sweep did until 2026-09-11: the slice is no longer null, and
    /// the successful read remembers the origin that makes it placed.
    ///
    /// Fails if `screen_filled` is ignored — the filled key would be released
    /// like a withheld one and go on sweeping on the cadence.
    #[test]
    fn a_null_sweep_that_filled_the_slice_ends_that_keys_null_sweeps() {
        let mut budget = SweepBudget::default();
        spend_one_null_sweep(&mut budget, BOARD);

        budget.after_null_publish(BOARD, true);

        for miss in 0..3 * u32::from(NULL_SWEEP_EVERY) {
            assert_eq!(null_miss(&mut budget, BOARD), None, "miss {miss} after the fill");
        }
    }

    /// A placed recheck that anchored on a screen with no anchored origin ends
    /// the key's null sweeps (fix round): the seed was right, so a sweep under
    /// this key could only find what the recheck found. A new key sweeps again.
    ///
    /// Fails if the end is dropped or does not reach the cadence — after the
    /// sheet closes under a TempleArea arm the cadence would restart in the same
    /// key, up to the whole cap of sheet-less sweeps across the run — or if the
    /// end outlives its key.
    #[test]
    fn a_recheck_sighting_on_an_unanchored_screen_ends_that_keys_null_sweeps() {
        let mut budget = SweepBudget::default();

        budget.on_recheck(BOARD, false);

        for miss in 0..3 * u32::from(NULL_SWEEP_EVERY) {
            assert_eq!(null_miss(&mut budget, BOARD), None, "miss {miss} after the recheck");
        }
        assert_eq!(spend_one_null_sweep(&mut budget, NEXT_BOARD), null_start(1), "a new key");
    }

    /// On a screen whose origin IS anchored the recheck is the ordinary placed
    /// path and ends nothing — the flag is what the rule keys on.
    ///
    /// Fails if `on_recheck` ignores `anchored`.
    #[test]
    fn a_recheck_sighting_on_an_anchored_screen_ends_nothing() {
        let mut budget = SweepBudget::default();

        budget.on_recheck(BOARD, true);

        assert_eq!(spend_one_null_sweep(&mut budget, BOARD), null_start(1));
    }

    /// Recalibrate re-arms the key the null-slice budget belongs to, so the next
    /// Temple tick sweeps and can publish a fresh corroborated measurement.
    /// **Amended 2026-09-11 (POE-275 WI-2):** not the next tick — the third
    /// consecutive clean miss after the press starts the sweep, on the cadence. This
    /// restores the deleted end-to-end decision seam without reintroducing the
    /// retired cadence gate.
    ///
    /// The state it builds is the one the command actually LEAVES since POE-278:
    /// a capture-derived slice standing, `screen_present == true`, a seeded
    /// Entrance origin in the hint — and NO anchor. Before POE-278 this test
    /// built `cheap_hint_from_screen(None, ..)` and passed `screen_present =
    /// false`, which the command can no longer produce; it stayed green while
    /// the press had stopped granting the sweep at all.
    #[test]
    fn recalibrate_leaves_the_temple_sweeping_and_republishing() {
        let capture = (1920, 1080);
        let client = [0, 0, capture.0 as i32, capture.1 as i32];
        let measured = crate::ssot::screen_from_geometry(
            capture.0,
            capture.1,
            7,
            LIVE_CAPTURE_ORIGIN,
            client,
            1_700_000_000_001,
        );
        assert_eq!(measured.anchors, None, "the press leaves no anchor to verify");
        let hint = cheap_hint_from_screen(Some(&measured), capture, 7, client);
        assert!(
            hint.is_some(),
            "the press leaves a SEEDED hint standing — the state that used to be null",
        );

        // What `cold_sweep_reason` is handed: the ANCHORED origin, which the
        // press left empty. Handing it the hint's seeded origin instead is the
        // POE-278 regression, and flips every assertion below to `None`.
        let placed_origin = measured
            .anchors
            .and_then(|anchors| anchors.temple_entrance)
            .map(|[x, y]| (x, y));
        assert_eq!(placed_origin, None);

        // Since POE-275 WI-2 the null slice sweeps on the cadence, up to the cap
        // per key, rather than once per key on the first miss.
        let mut budget = SweepBudget::default();
        let mut miss = |key| {
            let missed = DetectOutcome::Missed;
            cold_sweep_reason(true, placed_origin, missed, false, None, key, &mut budget)
        };
        let before = (4, 0);
        assert_eq!(
            (miss(before), miss(before), miss(before)),
            (None, None, null_start(1)),
            "the null-slice cadence permits the cold-start sweep",
        );
        for _ in 0..3 * u32::from(NULL_SWEEP_EVERY) * u32::from(NULL_SWEEP_CAP) {
            miss(before);
        }
        assert_eq!(miss(before), None, "the key's null-slice cap is spent");
        let after_recalibrate = (4, 1);
        assert_eq!(
            (miss(after_recalibrate), miss(after_recalibrate), miss(after_recalibrate)),
            (None, None, null_start(1)),
            "Recalibrate's rearm creates a fresh key with a whole cap",
        );

        // The sweep runs against the slice the press LEFT, not an empty one, and
        // its anchor converts to the same 0.90 the height implies at 1080p —
        // i.e. inside `OCR_DRIFT_BAND`. It must still be published: a cue that
        // read the art has to be able to overwrite the derivation, or pressing
        // Recalibrate would make `temple-anchor` unreachable on this machine.
        let swept = screen_from_anchor(
            LIVE_CAPTURE_SCALE,
            // The hint the tick derives FROM the standing slice — which since
            // POE-278 exists after a press, so the anchor is corroborated
            // against `hint_disagreement_line` rather than the capture height.
            hint.as_ref().map(|hint| hint.calibration),
            capture,
            7,
            LIVE_CAPTURE_ORIGIN,
            client,
            1_700_000_000_002,
        )
        .expect("the corroborated anchor fits the standing slice's capture geometry");
        assert!(
            (swept.ui_scale - measured.ui_scale).abs() <= 0.01,
            "the anchor agrees with the derived value to within the drift band, which is \
             what makes the publish below a real test of the override",
        );
        let mut slot = Some(measured);
        let record = crate::ssot::record_screen(&mut slot, swept);
        assert!(record.accepted && record.changed, "the corroborated sweep is published");
        let published = slot.expect("the published sweep replaces the derived slice");
        assert_eq!(
            published.source,
            crate::ssot::ScreenScaleSource::TempleAnchor,
            "the verifying cue takes the slice back from the derivation",
        );
        assert!(published.verified_this_session);
        assert_eq!((published.width, published.height), capture);
        assert_eq!(published.ui_scale, swept.ui_scale);
        assert_eq!(published.source, swept.source);
        assert_eq!(published.origin, swept.origin);
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
    /// publish and a log line on every tick. `Unavailable` is that status: it
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
            gate_line(
                &mut said,
                Some(trigger::ArmSource::Trigger(trigger::ArmReason::AlvaStart)),
                trigger::StandDown::Waiting,
            ),
            Some(
                "Temple: capture armed by Alva's start line — looking for the layout panel"
                    .to_string()
            ),
        );
        assert_eq!(
            gate_line(&mut said, None, trigger::StandDown::Waiting),
            Some("Temple: capture stood down — waiting for Alva (Re-arm forces a read)".to_string()),
        );
    }

    /// WI-1's line: the stand-down says WHY, so a smoke run can tell the cycle
    /// completing from Alva ending it, from the player walking out, from a
    /// Re-arm nobody used.
    ///
    /// Fails if the cause is dropped and the line goes back to one string — the
    /// four stand-downs are four different checks, and `app.log` would not
    /// separate them.
    #[test]
    fn the_stand_down_line_names_the_cause() {
        for (cause, expected) in [
            (
                trigger::StandDown::CycleComplete,
                "Temple: capture stood down — the sheet was read and closed (Re-arm forces a read)",
            ),
            (
                trigger::StandDown::AlvaLine,
                "Temple: capture stood down — Alva's line (Re-arm forces a read)",
            ),
            (
                trigger::StandDown::LeftArea,
                "Temple: capture stood down — the zone changed (Re-arm forces a read)",
            ),
            (
                trigger::StandDown::GraceOver,
                "Temple: capture stood down — Re-arm's grace is over (Re-arm forces a read)",
            ),
        ] {
            let mut said = Some(trigger::ArmSource::PanelOnScreen);

            assert_eq!(
                gate_line(&mut said, None, cause),
                Some(expected.to_string()),
                "{cause:?}",
            );
        }
    }

    /// And an ARMED gate names its source, never the cause it last shut for.
    /// Fails if the two are folded into one line — `capture armed by the sheet
    /// was read and closed` is not a sentence, and the smoke item reads the
    /// source.
    #[test]
    fn an_armed_gate_ignores_the_stand_down_cause() {
        let mut said = None;

        assert_eq!(
            gate_line(
                &mut said,
                Some(trigger::ArmSource::Trigger(trigger::ArmReason::Manual)),
                trigger::StandDown::CycleComplete,
            ),
            Some("Temple: capture armed by Re-arm — looking for the layout panel".to_string()),
        );
    }

    /// POE-246's own line: the gate stays open while the reason changes hands,
    /// and the log says which one is holding it. Fails if the source vocabulary
    /// stops at `ArmReason` — a smoke run then cannot tell a loop kept alive by
    /// the panel on screen from one Client.txt is still arming.
    #[test]
    fn the_gate_line_names_the_panel_that_is_holding_the_gate_open() {
        let mut said = Some(trigger::ArmSource::Trigger(trigger::ArmReason::Manual));

        assert_eq!(
            gate_line(
                &mut said,
                Some(trigger::ArmSource::PanelOnScreen),
                trigger::StandDown::GraceOver,
            ),
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

        assert!(gate_line(&mut said, source, trigger::StandDown::Waiting).is_some());

        assert_eq!(
            gate_line(&mut said, source, trigger::StandDown::Waiting),
            None,
            "and not again",
        );
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
                convenience: None,
                recommended_exit: None,
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
                convenience: None,
                recommended_exit: None,
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
                convenience: None,
                recommended_exit: None,
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

    /// The four loop events that end the wait for the temple sheet (POE-249).
    ///
    /// Two different reasons, one write. `Anchored` and `Reshown` are the wait
    /// ANSWERED — the sheet is on screen, and which of the two it is only says
    /// whether the board had to be read again — and `Disarmed`/`Stopping` are
    /// the loop no longer looking, over which a notice saying "waiting for the
    /// temple panel" is a lie. Fails if the clear is keyed on the resulting
    /// STATUS rather than on the outcome: these four land on four different
    /// statuses.
    #[test]
    fn the_outcomes_that_end_the_wait_for_the_panel_clear_it() {
        for outcome in [
            TickOutcome::Anchored,
            TickOutcome::Reshown(TempleStatus::Read),
            TickOutcome::Disarmed,
            TickOutcome::Stopping,
        ] {
            let mut slice = TempleSlice {
                waiting_for_panel: true,
                ..TempleSlice::default()
            };

            apply_status(&mut slice, outcome);

            assert!(!slice.waiting_for_panel, "{outcome:?}");
        }
    }

    /// And the three that leave it standing, because the wait is still the
    /// truth: a tick that looked and saw nothing, a tick that could not look
    /// this once, and the gate OPENING — which is what Alva's start line buys,
    /// so clearing there would take the notice down in the same second it went
    /// up.
    #[test]
    fn the_outcomes_that_are_still_a_wait_leave_it_standing() {
        for outcome in [
            TickOutcome::NoPanel,
            TickOutcome::Failed,
            TickOutcome::Armed,
        ] {
            let mut slice = TempleSlice {
                waiting_for_panel: true,
                ..TempleSlice::default()
            };

            apply_status(&mut slice, outcome);

            assert!(slice.waiting_for_panel, "{outcome:?}");
        }
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
            TickOutcome::Reshown(TempleStatus::Read),
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

    /// A failure path that re-runs on every tick says each thing once. Fails if
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
            client: [0, 0, width as i32, height as i32],
            anchors: None,
        }
    }

    #[test]
    fn the_placed_origin_hint_uses_the_ssot_placement_on_the_reference_screen() {
        let screen = remembered(ScreenScaleSource::MercFrame, 1920, 1080, 0.90, 7);
        let placements = crate::ssot::placements(&screen);
        let hint = cheap_hint_from_screen(
            Some(&screen),
            (1920, 1080),
            7,
            [0, 0, 1920, 1080],
        )
        .expect("the reference slice supplies a placed temple origin");

        assert_eq!(
            hint.origin,
            placements.temple.expect("the screen has temple placements").entrance_origin,
        );
        assert_eq!(hint.origin, (960, 713));
        assert!(
            (hint.calibration.scale - 0.99999).abs() < 1e-6,
            "the reference screen's measured temple scale is 0.99999, got {}",
            hint.calibration.scale,
        );
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

        let hint = hint_for_capture(Some(&screen), (1920, 1080), 7, [0, 0, 1920, 1080])
            .expect("this screen has one");

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

        assert_eq!(hint_for_capture(Some(&screen), (1920, 1080), 9, [0, 0, 1920, 1080]), None);
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
            hint_for_capture(Some(&no_id), (1920, 1080), 7, [0, 0, 1920, 1080]).is_some(),
            "a pre-POE-237 stored scale still hints on a capture of its size",
        );
        assert!(
            hint_for_capture(Some(&known), (1920, 1080), 0, [0, 0, 1920, 1080]).is_some(),
            "a capture with no display id still gets the hint its size earns",
        );
        assert_eq!(
            hint_for_capture(Some(&no_id), (2560, 1440), 7, [0, 0, 2560, 1440]),
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

        assert_eq!(hint_for_capture(Some(&screen), (1920, 1080), 7, [0, 0, 1920, 1080]), None);
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
                hint_for_capture(Some(&screen), (1920, 1080), 7, [0, 0, 1920, 1080]),
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
    /// `app_log` keeps 50 entries and `full_read` runs on every board of every
    /// incursion, so a line said unconditionally here evicts other diagnostics
    /// from the buffer while repeating one value — the failure this rule
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
    /// every read taken during the drag produces a different message about the
    /// same fact. Keying on the message — which is what `ErrorLog` does — would
    /// say it once per read AND spend the session-wide
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
        let refused = screen_from_anchor(
            2.05,
            None,
            (1920, 1080),
            7,
            (0, 0),
            [0, 0, 1920, 1080],
            1_700_000_000_000,
        )
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
            [0, 0, 1920, 1080],
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
            screen_from_anchor(
                slider.scale,
                Some(slider),
                (1920, 1080),
                7,
                (0, 0),
                [0, 0, 1920, 1080],
                1_700_000_000_000,
            )
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
            [0, 0, 1920, 1080],
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

        let next = anchored_screen(
            measured,
            (1920, 1080),
            7,
            (-1920, 0),
            [0, 0, 1920, 1080],
            1_700_000_000_000,
        );

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
            [0, 0, 1920, 1080],
            1_700_000_000_001,
        );
        let elsewhere = anchored_screen(
            anchor::scale_for_ui_scale(0.905),
            (1920, 1080),
            9,
            (-1920, 0),
            [0, 0, 1920, 1080],
            1_700_000_000_001,
        );
        let disagreeing = anchored_screen(
            anchor::scale_for_ui_scale(0.94),
            (1920, 1080),
            7,
            (0, 0),
            [0, 0, 1920, 1080],
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
            [0, 0, 1920, 1080],
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
            [0, 0, 1920, 1080],
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
    /// One line per tick for the life of a session is not a log, and a line
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
