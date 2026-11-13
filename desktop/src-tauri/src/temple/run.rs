//! The temple read worker (POE-171, POE-389) — the module's Windows glue.
//!
//! `modules.rs::spawn_temple` delegates here. Like the merc loop, this is a
//! [`ModuleJoin::Thread`](crate::modules::ModuleJoin::Thread) rather than a
//! task: `Windows.Media.Ocr` is apartment-threaded and deadlocks on the async
//! runtime (see `spawn_gem_scan` in lab/commands.rs). Threads cannot be
//! aborted, so the worker waits in [`CANCEL_POLL`] slices — two orders under the
//! registry's 5 s `MODULE_THREAD_POLL_CEILING`.
//!
//! # One press, one read (POE-389)
//!
//! The module takes no screen grab of its own. The app-wide read key
//! (`crate::read_hotkey`) grabs one frame per press and hands it to every
//! armed reader; [`super::press::TEMPLE_READER`] arms only under Client.txt
//! temple context and sends the frame down the channel this thread installs in
//! `TempleState.press`. [`worker_loop`] reads each received frame exactly once
//! ([`read_press`]); between presses nothing is captured. The policy is
//! normative in docs/OVERLAY-GUIDE.md § "Screen reading (GGG policy)".
//!
//! # What one press reads
//!
//! One full-resolution windowed NCC at the Entrance origin supplied by
//! [`crate::ssot::placements`] ([`anchor::detect_cheap`]) is the presence test.
//! When it misses, [`locate_on_miss`] decides whether the pressed frame is
//! searched with [`anchor::anchor_for_loop`]'s pyramid: only for a null or
//! unplaced slice. A placed miss is the sheet not being open, and
//! Recalibrate is the manual fix for a wrong placement. Nothing is searched on
//! any frame but the pressed one.
//!
//! A hit reads the whole board once — two bounded text crops and two crops per
//! plate, 28 OCR calls ([`read_board`]) — with no merge and no retry. A press
//! that finds no panel publishes [`TickOutcome::NoPanel`], which takes the
//! sheet-bound overlays down.
//!
//! # No `#[cfg(windows)]`, on purpose
//!
//! The platform difference arrives as an `Err` from `ocr::recognize_lines` and
//! `ocr::engine_ready`, each of which already has a non-Windows arm, so the
//! Linux container compiles and tests the code it is meant to protect.
//!
//! # Every OCR crop is bounded
//!
//! No path here hands a whole frame to [`crate::capture::preprocess_for_ocr`],
//! which upscales 2× unconditionally. All three ROIs — [`panel_rect`],
//! [`remaining_rect`] and [`diamond_rect`] — are placed from the anchor's origin
//! and scale, so each stays a fixed size in reference px whatever the monitor
//! is. [`read_press`] prints all three once per distinct value
//! (`Temple: rois …`).
//!
//! # Read-only, always
//!
//! This module reads the screen. It never moves the cursor and never sends
//! input — injecting input into the PoE client is against GGG's ToS.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use image::DynamicImage;
use tauri::{AppHandle, Manager};
use tokio::sync::watch;

use crate::modules::ModuleJoin;
use crate::AppState;

use super::anchor::{self, CheapHint};
use super::lattice::{self, Lattice};
use super::markers;
use super::panel::{self, SystemOcr, TextRecognizer};
use super::press::PressedFrame;
use super::reader::{self, TempleLayout};
use super::slice::{self, TempleSettings, TempleSlice, TempleStatus};

/// How often a waiting worker checks its stop signal, and the slice every
/// other wait is built from.
const CANCEL_POLL: Duration = Duration::from_millis(100);
/// How long an `unavailable` module idles between stop checks.
const UNFOCUSED_NAP: Duration = Duration::from_millis(1000);
/// Distinct error messages logged before the worker stops repeating itself. An
/// error carrying a varying number is a different string every time, so
/// without a cap one session could fill the 50-entry LOGS buffer on its own.
const MAX_DISTINCT_ERRORS: usize = 12;

/// Spawn the read worker. Called through `MODULES` — see `modules.rs`.
pub fn spawn(app: AppHandle, cancel: watch::Receiver<bool>) -> ModuleJoin {
    ModuleJoin::Thread(std::thread::spawn(move || run_worker(app, cancel)))
}

// ------------------------------------------------------ the measurements --

/// What one press spent before the OCR, stage by stage, in wall time.
///
/// Written by [`read_press`] and read by [`read_timings_line`], which carries
/// the OCR half as well. `capture` is the grab's own `call_ms`, measured by the
/// read key's dispatcher. `Duration::ZERO` is a stage this press did not run.
///
/// Every number here is a MEASUREMENT of the machine and the build that took it
/// (the laptop's debug build grabbed the screen in 225 ms, the PC's release
/// build in 52 ms — same code), which is why the lines print the numbers and
/// assert no budget. The budget is the owner's — verdict on screen about 1 s
/// after the panel opens (docs/TEMPLE-LIFECYCLE.md) — and these lines are how
/// it is checked against a real session rather than assumed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TickStages {
    /// The read key's monitor grab.
    pub capture: Duration,
    /// `anchor::detect_cheap` — the presence probe.
    pub cheap: Duration,
    /// Locating the panel on a miss (when [`locate_on_miss`] allowed it) plus
    /// resolving the anchor into a `TempleLayout` (doors included).
    pub anchor: Duration,
}

/// What the full read spent after the anchor, stage by stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadStages {
    /// The panel and budget-line OCR ([`read_board`]'s text regions).
    pub text_ocr: Duration,
    /// The plate OCR calls (`panel::read_slots`): 26, two per plate.
    pub plate_ocr: Duration,
    /// The door markers (`read_markers`).
    pub markers: Duration,
    /// Getting the 25 x 3 valuation table — a LOOKUP when nothing it depends
    /// on has moved (`valuation::temple_valuation_now`), the build when something
    /// has. Split off `advise` by POE-257 WI-3 (2026-09-07) because the two
    /// were one number and the owner's question — is the table cached? — could
    /// not be answered from the log: the pair measured 193-245 ms on the PC's
    /// release build while the advisor's own ranking is 13-22 ms of it
    /// (`advisor/mod.rs`, `the_conditional_ranking_cost_on_case_eight`).
    ///
    /// **It measures the lookup or the build, never a lock wait.** The accessor
    /// holds `AppState.temple_state.valuation` for a `ValuationKey` comparison and
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

/// The line every successful press read writes: each stage from the grab to
/// the publish, the total, the read's own `last_read_at` stamp so the overlay's
/// `[temple-overlay] board read at …` line can be matched to it, what the read
/// covered, and whether it was clean.
///
/// `valuation N ms (cached | computed)` sits beside `advise N ms` (POE-257
/// WI-3): the WORD says whether the table was looked up or built, which the
/// milliseconds alone cannot.
pub fn read_timings_line(
    tick: &TickStages,
    read: &ReadStages,
    total: Duration,
    read_at: u64,
    unclean: bool,
    plan: &slice::ReadPlan,
) -> String {
    format!(
        "Temple: read timings — capture {} ms, cheap detect {} ms, anchor {} ms, text ocr {} ms, plates {} ms, markers {} ms, valuation {} ms ({}), advise {} ms, publish {} ms — {} ms from grab to publish, read at {}; {}; {}",
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
        plan.describe(),
        if unclean { "unclean" } else { "clean" },
    )
}

// --------------------------------------------------- the status machine --

/// What one press (or the worker stopping) says about the module's state —
/// the half the page sees, separated from the thread so the rules are testable
/// without a screen or a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickOutcome {
    /// The press failed before it could tell whether a panel is there — the
    /// grab or the OCR engine returned an error.
    Failed,
    /// The press read clean and no panel anchored.
    NoPanel,
    /// The press anchored a panel. The read follows, which publishes its own
    /// status through [`slice::project`].
    Anchored,
    /// The worker is shutting down.
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
    /// Two different reasons, both ending in the same write.
    /// [`TickOutcome::Anchored`] is the wait being ANSWERED — the sheet is on
    /// screen. [`TickOutcome::Stopping`] is the worker no longer reading at
    /// all, and a notice that says "waiting for the temple panel" over it is a
    /// lie on screen.
    ///
    /// The two that leave it alone are the ones where the wait is still the
    /// truth: [`TickOutcome::NoPanel`] is a press that looked and saw nothing,
    /// and [`TickOutcome::Failed`] is a press that could not look this once.
    pub clear_waiting: bool,
}

/// The module's status machine.
///
/// # The rule this exists for
///
/// A transient failure — one grab error while the player is not on
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
        // A stopped worker is not reporting a live board, and the reason the last
        // error happened is no longer something the user can act on.
        TickOutcome::Stopping => (TempleStatus::Idle, true),
    };
    StatusUpdate {
        status,
        clear_error,
        clear_waiting: matches!(outcome, TickOutcome::Anchored | TickOutcome::Stopping),
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
/// maximised, and [`read_press`]'s `Temple: rois` line prints the rect it used so
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

/// Sleep in [`CANCEL_POLL`] slices, stopping early on cancel. `false` = cancelled.
fn nap(cancel: &watch::Receiver<bool>, total: Duration) -> bool {
    let mut left = total;
    while left > Duration::ZERO {
        if *cancel.borrow() {
            return false;
        }
        let step = left.min(CANCEL_POLL);
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
    let settings = state.temple_state.settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
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
///   grow its own opinion of what "the same screen" means — in particular that
///   the display is its virtual-desktop origin, never its monitor handle. In the capture loop this branch is nearly unreachable, because
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
    origin: (i32, i32),
    client: [i32; 4],
) -> Option<anchor::AnchorCalibration> {
    let screen = screen?;
    if !crate::ssot::screen_matches(&Some(*screen), capture, origin, client) {
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
    display_origin: (i32, i32),
    client: [i32; 4],
) -> Option<CheapHint> {
    let screen = screen?;
    let calibration = hint_for_capture(Some(screen), capture, display_origin, client)?;
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
    display_origin: (i32, i32),
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
    let hint = cheap_hint_from_screen(screen.as_ref(), capture, display_origin, client);
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

/// Whether a press whose placed recheck missed searches the pressed frame with
/// [`anchor::anchor_for_loop`] (POE-389, Coordinator ruling 2026-11-13).
///
/// True only for a null slice or one with no ANCHORED Entrance origin: there is
/// no placement to trust, so the frame is the only way to find the sheet. A
/// placed miss is the sheet not being open, and is `NoPanel` with no search;
/// Recalibrate is the manual fix for a placement that is wrong.
///
/// `anchored_origin` is the slice's anchored origin, never the hint's: the hint
/// carries a seed for any non-null slice (POE-278), and reading that as a
/// verified placement would never search a screen that merely exists.
fn locate_on_miss(screen_present: bool, anchored_origin: Option<[i32; 2]>) -> bool {
    !screen_present || anchored_origin.is_none()
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


/// What the worker remembers between presses: only the say-once memories of
/// its log lines. No board, no retry budget and no sweep survive a press.
#[derive(Default)]
struct Session {
    errors: ErrorLog,
    /// The slice-derived hint last announced — see [`hint_line`].
    hint_said: Option<anchor::AnchorCalibration>,
    /// The last `k` disagreement announced by [`publish_anchor_scale`].
    k_said: Option<String>,
    /// The last [`rois_line`] announced.
    rois_said: Option<String>,
    /// The outside-set [`clipped_roi_announcement`] last announced.
    clipped_said: Option<Vec<&'static str>>,
    /// Whether the first anchored placed-origin recheck has been logged.
    placed_recheck_said: bool,
}

/// Install (or clear) the worker's inbox in `TempleState.press`.
fn set_press_sender(app: &AppHandle, tx: Option<mpsc::Sender<PressedFrame>>) {
    let state = app.state::<AppState>();
    *state.temple_state.press.lock().unwrap_or_else(|e| e.into_inner()) = tx;
}

fn run_worker(app: AppHandle, cancel: watch::Receiver<bool>) {
    crate::app_log(&app, "Temple: read worker started".to_string());
    crate::report_ocr_engine(&app);

    // The user's settings belong on the slice from the first frame — the page
    // and the overlay render their own controls from them.
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

    let (tx, rx) = mpsc::channel();
    set_press_sender(&app, Some(tx));
    let mut session = Session::default();
    worker_loop(&rx, &|| *cancel.borrow(), &mut |pressed| {
        read_press(&app, &mut session, &cancel, pressed)
    });
    set_press_sender(&app, None);

    // Best-effort by contract: on app exit the process is gone before this
    // runs, which is why `status` — forced to `Off` by the SSOT composer once
    // the module is disabled — is what the page trusts.
    publish(&app, |slice| apply_status(slice, TickOutcome::Stopping));
    crate::app_log(&app, "Module temple: stopped".to_string());
}

/// The worker's receive path: one `read` per received frame, in order, until
/// `stop` answers true or every sender is gone. It owns no grab function, so it
/// cannot capture anything a press did not.
pub fn worker_loop(
    rx: &mpsc::Receiver<PressedFrame>,
    stop: &dyn Fn() -> bool,
    read: &mut dyn FnMut(PressedFrame),
) {
    loop {
        if stop() {
            return;
        }
        match rx.recv_timeout(CANCEL_POLL) {
            Ok(pressed) => read(pressed),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
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
/// [`read_press`] runs on every press — so an unconditional line here repeats one value into a 50-entry
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
/// working install: the read key grabs the whole monitor, so a read region
/// outside it means the game is not filling the monitor being grabbed.
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

/// One pressed frame read once (POE-389): the per-frame prefix the loop's tick
/// ran, then the first-read body, on THIS frame only.
///
/// A grab error is reported through [`fail`] and nothing else. A placed recheck
/// that misses searches the frame only when [`locate_on_miss`] says so;
/// otherwise, or when the search finds nothing, the press publishes
/// [`TickOutcome::NoPanel`]. A located anchor that fills or contradicts the
/// placement is remembered only after the read has published (ADR-025 cl. 4),
/// through [`finish`].
fn read_press(
    app: &AppHandle,
    session: &mut Session,
    cancel: &watch::Receiver<bool>,
    pressed: PressedFrame,
) {
    let started = Instant::now();
    let reason = pressed.reason.label();
    let said = |outcome: &str| {
        crate::app_log(app, format!("Temple: read on press ({reason}) — {outcome}"));
    };
    let grab = match pressed.frame {
        Ok(grab) => grab,
        Err(e) => {
            fail(app, session, format!("Temple: screen capture failed — {e}"));
            return said("capture failed");
        }
    };
    let mut tick_stages = TickStages {
        capture: Duration::from_millis(u64::from(grab.timing.call_ms)),
        ..TickStages::default()
    };
    let img = &grab.image;
    let capture = (img.width(), img.height());
    let (monitor_id, origin, client) = (grab.monitor_id, grab.origin, grab.client);
    // Before ANY remembered geometry is read (POE-227).
    crate::ssot::drop_if_mismatched(app, capture, origin, client);

    let settings = settings_snapshot(app);
    let (cheap_hint, hint_source, screen_present, anchored_origin) =
        hint_from_slice(app, capture, origin, client);
    let hint = cheap_hint.map(|hint| hint.calibration);
    if let Some(line) = hint_line(&mut session.hint_said, hint, hint_source) {
        crate::app_log(app, line);
    }

    let probing = Instant::now();
    let cheap = anchor::detect_cheap(img, cheap_hint.as_ref());
    tick_stages.cheap = probing.elapsed();
    if !session.placed_recheck_said && matches!(cheap, anchor::CheapDetect::Anchored(_)) {
        session.placed_recheck_said = true;
        crate::app_log(
            app,
            format!(
                "Temple: placed-origin recheck — NCC {:.3}, {} ms",
                cheap.ncc(),
                ms(tick_stages.cheap)
            ),
        );
    }
    let stop = || *cancel.borrow();
    let (found, located) = match cheap {
        anchor::CheapDetect::Anchored(found) => (found, false),
        anchor::CheapDetect::Nothing { .. } => {
            let searched = locate_on_miss(screen_present, anchored_origin).then(|| {
                let locating = Instant::now();
                let found = anchor::anchor_for_loop(img, hint.as_ref(), &stop);
                tick_stages.anchor = locating.elapsed();
                found
            });
            match searched {
                Some(Ok(found)) => (found, true),
                _ => {
                    publish(app, |slice| apply_status(slice, TickOutcome::NoPanel));
                    return said("no panel");
                }
            }
        }
    };

    // Only a LOCATED anchor may replace the placement (ADR-025 cl. 4), and only
    // after the read below has published — `finish` owns that.
    let placed_origin = cheap_hint.as_ref().map(|hint| hint.origin);
    let fallback_origin = located
        .then(|| {
            if let Some(line) = placed_origin_contradiction_line(placed_origin, found.origin) {
                crate::app_log(app, line);
                crate::app_log(app, GEOMETRY_NOTICE_LINE.to_string());
            }
            placed_origin_contradiction(placed_origin, found.origin)
        })
        .flatten();

    let anchoring = Instant::now();
    let layout = reader::read_layout_at(img, found);
    tick_stages.anchor += anchoring.elapsed();
    publish_anchor_scale(app, session, &layout, hint, capture, monitor_id, origin, client);
    crate::app_log(
        app,
        format!("Temple: layout panel found (scale {:.3}, NCC {:.3})", layout.scale, layout.ncc),
    );
    publish(app, |slice| apply_anchored(slice, false));

    // Before any crop, so a read that fails halfway still leaves its geometry.
    if let Some(line) = rois_line(&mut session.rois_said, layout.origin, layout.scale) {
        crate::app_log(app, line);
    }
    let clipped = clipped_text_rois(&text_regions(&layout), img.width(), img.height());
    if let Some(lines) = clipped_roi_announcement(&mut session.clipped_said, &clipped) {
        for line in lines {
            crate::app_log(app, line);
        }
    }
    let clipped_names: Vec<&'static str> = clipped.iter().map(|(name, _)| *name).collect();

    let mut stages = ReadStages::default();
    let mut unclean = false;
    let result = read_board(img, layout, &SystemOcr, &clipped_names, &stop, &mut stages).map(|read| {
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
        // THIS press's settings snapshot, so the table, the ranking and the
        // echoed settings are one preset's (POE-257 WI-3).
        let valuing = Instant::now();
        let (valuation, market, valuation_source) =
            crate::temple::valuation::temple_valuation_now(app, &settings);
        stages.valuation = valuing.elapsed();
        stages.valuation_source = valuation_source;
        let advising = Instant::now();
        let projected = verdict(&read, &clipped, &settings, &valuation, &market, now_ms());
        stages.advise = advising.elapsed();
        unclean = slice::unclean(&read, &clipped_names);
        projected
    });
    let cancelled = result.is_err() && stop();
    let done = finish(result, fallback_origin);
    let read_at = match done.publish {
        Ok(projected) => {
            let read_at = projected.last_read_at;
            let publishing = Instant::now();
            publish(app, |slice| *slice = projected);
            stages.publish = publishing.elapsed();
            read_at
        }
        Err(_) if cancelled => return said("cancelled"),
        Err(msg) => {
            fail(app, session, msg);
            return said("OCR failed");
        }
    };
    if let Some(origin) = done.remember {
        crate::ssot::remember_anchor(app, crate::ssot::AnchorModule::TempleEntrance, origin);
    }
    crate::app_log(
        app,
        read_timings_line(
            &tick_stages,
            &stages,
            started.elapsed(),
            read_at.unwrap_or(0),
            unclean,
            &slice::plan_read(None, &clipped_names),
        ),
    );
    said(if unclean { "read, unclean" } else { "read" });
}

/// The OCR body of one press: the whole board, no merge (POE-389 D4).
///
/// The two text regions are read first and the plates after, through
/// `recognizer`. An OCR error is `Err("Temple: OCR failed — …")` and buys no
/// plate calls; a `stop` that turns true is `Err("cancelled")`.
pub fn read_board(
    img: &DynamicImage,
    layout: TempleLayout,
    recognizer: &dyn TextRecognizer,
    clipped_names: &[&'static str],
    stop: &dyn Fn() -> bool,
    stages: &mut ReadStages,
) -> Result<slice::KeptRead, String> {
    let plan = slice::plan_read(None, clipped_names);

    let reading_text = Instant::now();
    let mut lines = Vec::new();
    for (name, rect) in text_regions(&layout) {
        if !plan.text.contains(&name) {
            continue;
        }
        // A crop outside the capture contributes nothing rather than failing
        // the read: losing the budget line costs a warning, not the board.
        let Some((crop, origin)) = crop_clipped(img, rect) else {
            continue;
        };
        let read = panel::crop_lines(recognizer, &crop, origin)
            .map_err(|e| format!("Temple: OCR failed — {e}"))?;
        lines.extend(read);
    }
    let panel_reading = panel::read_panel(&lines);
    stages.text_ocr = reading_text.elapsed();

    // Up to 26 more OCR calls follow; a stop must not buy them, and lands
    // between two plate crops inside `read_slots`.
    if stop() {
        return Err("cancelled".to_string());
    }
    let lattice = Lattice::new(layout.origin, layout.scale);
    let reading_plates = Instant::now();
    let rooms = panel::read_slots(recognizer, img, &lattice, &plan.plates, stop);
    stages.plate_ocr = reading_plates.elapsed();
    if stop() {
        return Err("cancelled".to_string());
    }

    let reading_markers = Instant::now();
    let (settled, marker_error) = match plan.markers {
        false => (None, None),
        true => match read_markers(img, &layout) {
            Ok(set) => (Some(set), None),
            Err(e) => (None, Some(e)),
        },
    };
    stages.markers = reading_markers.elapsed();
    Ok(slice::KeptRead { layout, rooms, panel: panel_reading, settled, marker_error })
}

/// The slice one read publishes: the advisor's ranking and the projection,
/// exactly as the loop's first read passed them.
pub fn verdict(
    read: &slice::KeptRead,
    clipped: &[(&'static str, [i32; 4])],
    settings: &TempleSettings,
    valuation: &crate::temple::valuation::Valued,
    market: &crate::temple::market::MarketInput,
    read_at: u64,
) -> TempleSlice {
    // The page shows a state, not a history: the notice is on every read it is
    // true of, while the log line is said once per outside-set.
    let read_notice = (!clipped.is_empty()).then(|| {
        clipped
            .iter()
            .map(|(name, rect)| clipped_roi_line(name, *rect))
            .collect::<Vec<_>>()
            .join("; ")
    });
    let advice = slice::advise_read(
        &read.layout,
        &read.rooms,
        &read.panel,
        read.settled.as_ref(),
        settings,
        valuation,
    );
    slice::project(
        &slice::ReadResult {
            layout: &read.layout,
            rooms: &read.rooms,
            panel: &read.panel,
            settled: read.settled.as_ref(),
            marker_error: read.marker_error.clone(),
            read_notice,
            advice: advice.as_ref(),
            config: settings.config.clone(),
            profile: settings.profile.clone(),
            preset: settings.preset,
            custom: settings.custom.clone(),
            valuation,
            market: slice::market_view(market),
            read_at,
        },
        // The calibration THIS capture measured: the scale the board in front
        // of the user was read at.
        Some(read.layout.calibration),
    )
}

/// What a finished read publishes, and the located origin it may remember.
#[derive(Debug)]
pub struct PressFinish {
    pub publish: Result<TempleSlice, String>,
    /// `Some` only for a successful read whose anchor was located away from
    /// (or without) a placement.
    pub remember: Option<[i32; 2]>,
}

/// Pair a read's result with what it may remember: a failed or cancelled read
/// never changes the next press's placement.
pub fn finish(result: Result<TempleSlice, String>, fallback_origin: Option<(i32, i32)>) -> PressFinish {
    let mut remember = None;
    if result.is_ok() {
        remember_fallback_anchor(fallback_origin, |origin| remember = Some(origin));
    }
    PressFinish { publish: result, remember }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::MODULE_THREAD_POLL_CEILING;

    /// The worker's stop discipline, as a number rather than as a comment: a
    /// detached thread can outlive its stop by at most one [`CANCEL_POLL`].
    /// Fails if the quantum is raised past the registry's ceiling.
    #[test]
    fn the_poll_quantum_stays_inside_the_registry_ceiling() {
        assert!(
            CANCEL_POLL <= MODULE_THREAD_POLL_CEILING,
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

    /// A press read covers the whole board and the line says so — and it still
    /// carries every stage measurement, which is what the owner's ~1 s budget
    /// is checked against. Fails if `plan.describe()` is dropped from the line.
    #[test]
    fn the_read_line_names_a_full_read_and_every_stage() {
        let (tick, read) = read_stages();

        let line = read_timings_line(
            &tick,
            &read,
            Duration::from_millis(666),
            1788567663863,
            false,
            &slice::ReadPlan::full(),
        );

        assert!(line.contains("read at 1788567663863; full; clean"), "{line}");
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
            &slice::ReadPlan::full(),
        );

        assert!(line.contains("valuation 180 ms (computed),"), "{line}");
        assert!(!line.contains("(cached)"), "{line}");
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

    /// POE-244's core fix. The panel leaving the screen is what an INCURSION
    /// looks like — the player stepped through the door and the layout panel
    /// closed behind them — and `PanelNotVisible` is what a press that finds no
    /// panel publishes. Dropping the advice there left the door widget with no
    /// purple seal, no `open <edge>` line and no architect name at exactly the
    /// point they are the only things still on screen. Fails if `NoPanel`
    /// clears the advice.
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

    /// The two events that end the wait for the temple sheet (POE-249).
    ///
    /// `Anchored` is the wait ANSWERED — the sheet is on screen — and
    /// `Stopping` is the worker no longer reading, over which a notice saying
    /// "waiting for the temple panel" is a lie. Fails if the clear is keyed on
    /// the resulting STATUS rather than on the outcome: these two land on two
    /// different statuses.
    #[test]
    fn the_outcomes_that_end_the_wait_for_the_panel_clear_it() {
        for outcome in [TickOutcome::Anchored, TickOutcome::Stopping] {
            let mut slice = TempleSlice {
                waiting_for_panel: true,
                ..TempleSlice::default()
            };

            apply_status(&mut slice, outcome);

            assert!(!slice.waiting_for_panel, "{outcome:?}");
        }
    }

    /// And the two that leave it standing, because the wait is still the
    /// truth: a press that looked and saw nothing, and a press that could not
    /// look this once.
    #[test]
    fn the_outcomes_that_are_still_a_wait_leave_it_standing() {
        for outcome in [TickOutcome::NoPanel, TickOutcome::Failed] {
            let mut slice = TempleSlice {
                waiting_for_panel: true,
                ..TempleSlice::default()
            };

            apply_status(&mut slice, outcome);

            assert!(slice.waiting_for_panel, "{outcome:?}");
        }
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
            (0, 0),
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

        let hint = hint_for_capture(Some(&screen), (1920, 1080), (0, 0), [0, 0, 1920, 1080])
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

        assert_eq!(hint_for_capture(Some(&screen), (1920, 1080), (1920, 0), [0, 0, 1920, 1080]), None);
    }

    /// A measurement of a different resolution is not a hint. The capture loop
    /// reaches this only through `temple_debug_capture`'s image-file path —
    /// `ssot::drop_if_mismatched` empties the slot first on a live tick — and
    /// the answer has to be the same either way.
    #[test]
    fn a_scale_measured_at_another_resolution_is_not_a_hint() {
        let screen = remembered(ScreenScaleSource::MercFrame, 2560, 1440, 1.20, 7);

        assert_eq!(hint_for_capture(Some(&screen), (1920, 1080), (0, 0), [0, 0, 1920, 1080]), None);
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
                hint_for_capture(Some(&screen), (1920, 1080), (0, 0), [0, 0, 1920, 1080]),
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
    /// `app_log` keeps 50 entries and `read_press` runs on every board of every
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

    // ---------------------------- first-read verdict on the live capture --

    /// Today's first read of a board, pinned on the live capture (POE-389
    /// WI-0): the oracle for "one press gives the same verdict as today on the
    /// same frame". The expected values were recorded by running the pre-change
    /// code once; they are literals so a refactor of the read cannot move them.
    mod first_read_verdict {
        use super::*;
        use crate::ocr::OcrLineBox;
        use crate::temple::lattice::Slot;
        use crate::temple::market::MarketInput;
        use crate::temple::preset::{self, Preset, TempleCustomSettings};
        use crate::temple::rooms::Match;
        use std::cell::RefCell;
        use std::collections::VecDeque;

        /// One expected OCR call: what it is for, the crop it must be handed,
        /// and what the engine answers.
        struct Call {
            label: String,
            crop: DynamicImage,
            response: Result<Vec<OcrLineBox>, String>,
        }

        /// An ordered response script. Each `recognize` pops the next call and
        /// refuses a crop that is not, pixel for pixel, the one production cuts
        /// for that call — so a reordered or re-cropped read fails here instead
        /// of answering with another plate's text.
        struct ScriptedOcr {
            calls: RefCell<VecDeque<Call>>,
            made: RefCell<usize>,
        }

        impl panel::TextRecognizer for ScriptedOcr {
            fn recognize(&self, img: &DynamicImage) -> Result<Vec<OcrLineBox>, String> {
                *self.made.borrow_mut() += 1;
                let call = self
                    .calls
                    .borrow_mut()
                    .pop_front()
                    .expect("an OCR call the script does not expect");
                assert!(
                    img.as_bytes() == call.crop.as_bytes()
                        && (img.width(), img.height()) == (call.crop.width(), call.crop.height()),
                    "{}: the crop handed to OCR is not the one production cuts for it",
                    call.label,
                );
                call.response
            }
        }

        impl ScriptedOcr {
            fn assert_consumed(&self) {
                let left: Vec<String> = self.calls.borrow().iter().map(|c| c.label.clone()).collect();
                assert!(left.is_empty(), "scripted OCR calls never made: {left:?}");
            }
        }

        /// The side panel as Windows.Media.Ocr boxed it on this capture
        /// (laptop dump, capture px) — the same lines `panel.rs` pins.
        const PANEL_LINES: [([i32; 4], &str); 9] = [
            ([1288, 80, 254, 19], "LIGHTNING WORKSHOP"),
            ([1505, 115, 113, 10], "HAYOXI, ARCHITECT OF"),
            ([1526, 129, 70, 9], "DESTRUCTION"),
            ([1479, 143, 163, 11], "(KILL TO UPGRADE TO OMNITECT"),
            ([1520, 157, 83, 11], "REACTOR PLANT)"),
            ([1194, 289, 146, 10], "XOPEC, ARCHITECT OF POWER"),
            ([1188, 303, 160, 11], "(KILL TO CHANGE TO EXPLOSIVES"),
            ([1251, 317, 34, 11], "ROOM)"),
            ([1342, 364, 146, 12], "ENTER INCURSION"),
        ];

        /// The budget line under the layout, capture px.
        const REMAINING_LINE: ([i32; 4], &str) = ([855, 786, 210, 16], "3 INCURSIONS REMAINING");

        /// What each plate prints on the capture, in [`Slot::ALL`] order: its
        /// name and its tier numeral (empty where the plate prints none).
        const PLATES: [(&str, &str); 13] = [
            ("APEX OF ATZOATL", ""),
            ("HOUSE OF THE OTHERS", "III"),
            ("VAULT", "I"),
            ("CHASM", ""),
            ("LIGHTNING WORKSHOP", "I"),
            ("SANCTUM OF UNITY", "II"),
            ("HALL OF CHAMPIONS", "III"),
            ("SACRIFICIAL CHAMBER", "I"),
            ("TEMPEST GENERATOR", "I"),
            ("DEPARTMENT OF THAUMATURGY", "II"),
            ("COURT OF SEALED DEATH", "III"),
            ("ENTRANCE", ""),
            ("HYBRIDISATION CHAMBER", "III"),
        ];

        /// The plates (indices into [`Slot::ALL`]) whose name the partial case
        /// reads as nothing.
        const UNREAD_PLATES: [usize; 2] = [1, 8];

        fn line_at(text: &str, [x, y, w, h]: [i32; 4], origin: (i32, i32)) -> OcrLineBox {
            OcrLineBox { text: text.to_string(), x: x - origin.0, y: y - origin.1, w, h }
        }

        fn crop(img: &DynamicImage, [x, y, w, h]: [i32; 4]) -> DynamicImage {
            img.crop_imm(x as u32, y as u32, w as u32, h as u32)
        }

        /// The script for one first read: the two text regions in
        /// [`text_regions`] order, then each plate's name and numeral.
        fn script(img: &DynamicImage, layout: &TempleLayout, unread: &[usize]) -> ScriptedOcr {
            let mut calls = VecDeque::new();
            for (name, rect) in text_regions(layout) {
                let (crop, origin) = crop_clipped(img, rect).expect("both text regions are on this capture");
                let response = match name {
                    slice::PANEL_REGION => PANEL_LINES.iter().map(|(r, t)| line_at(t, *r, origin)).collect(),
                    _ => vec![line_at(REMAINING_LINE.1, REMAINING_LINE.0, origin)],
                };
                calls.push_back(Call { label: format!("text region {name}"), crop, response: Ok(response) });
            }
            let lattice = Lattice::new(layout.origin, layout.scale);
            for (i, (slot, (name, numeral))) in Slot::ALL.into_iter().zip(PLATES).enumerate() {
                let name_lines = match unread.contains(&i) {
                    true => Vec::new(),
                    false => vec![OcrLineBox { text: name.to_string(), x: 0, y: 0, w: 100, h: 10 }],
                };
                calls.push_back(Call {
                    label: format!("plate {i} name"),
                    crop: crop(img, panel::name_strip(&lattice, slot)),
                    response: Ok(name_lines),
                });
                let numeral_lines = match numeral.is_empty() {
                    true => Vec::new(),
                    false => vec![OcrLineBox { text: numeral.to_string(), x: 0, y: 0, w: 10, h: 10 }],
                };
                calls.push_back(Call {
                    label: format!("plate {i} numeral"),
                    crop: crop(img, panel::numeral_box(&lattice, slot)),
                    response: Ok(numeral_lines),
                });
            }
            ScriptedOcr { calls: RefCell::new(calls), made: RefCell::new(0) }
        }

        /// What a first read publishes, as plain values.
        #[derive(Debug, PartialEq)]
        struct Verdict {
            status: TempleStatus,
            unread: usize,
            headline: Option<String>,
            architect_index: Option<usize>,
            doors: Vec<String>,
            secondary_door: Option<String>,
            map_action: Option<String>,
            warnings: Vec<String>,
            recommended_exit: Option<(String, String)>,
            unclean: bool,
        }

        /// One press's read through the production pieces — [`read_board`] →
        /// [`verdict`] → [`finish`] — without an `AppHandle`: the valuation is
        /// the Default preset with no market, `read_at` is 0, and no anchor was
        /// located.
        fn press_read(ocr: &ScriptedOcr, stop: &dyn Fn() -> bool) -> (Result<slice::KeptRead, String>, PressFinish) {
            let img = live_capture();
            let layout = live_layout(&img);
            let settings = TempleSettings::default();
            let clipped = clipped_text_rois(&text_regions(&layout), img.width(), img.height());
            let clipped_names: Vec<&'static str> = clipped.iter().map(|(name, _)| *name).collect();

            let read = read_board(&img, layout, ocr, &clipped_names, stop, &mut ReadStages::default());
            let valuation = preset::value_table(Preset::Default, &TempleCustomSettings::default(), &MarketInput::none());
            let projected = read
                .as_ref()
                .map(|read| verdict(read, &clipped, &settings, &valuation, &MarketInput::none(), 0))
                .map_err(Clone::clone);
            (read, finish(projected, None))
        }

        fn first_read(ocr: &ScriptedOcr) -> Verdict {
            let (read, done) = press_read(ocr, &|| false);
            let read = read.expect("a scripted read succeeds");
            let projected = done.publish.expect("a successful read publishes its slice");
            let unclean = slice::unclean(&read, &[]);

            let view = projected.advice.as_ref();
            let top = view.and_then(|v| v.recommendations.first());
            Verdict {
                status: projected.status,
                unread: read.rooms.iter().filter(|r| r.identity == Match::Unknown).count(),
                headline: top.map(|r| r.headline.clone()),
                architect_index: top.and_then(|r| r.architect_index),
                doors: top.map(|r| r.doors.clone()).unwrap_or_default(),
                secondary_door: view.and_then(|v| v.secondary_door.clone()),
                map_action: view.map(|v| v.map_action.clone()),
                warnings: view.map(|v| v.warnings.clone()).unwrap_or_default(),
                recommended_exit: view
                    .and_then(|v| v.recommended_exit.as_ref())
                    .map(|e| (e.door.clone(), e.name.clone())),
                unclean,
            }
        }

        /// Recorded on HEAD 326256a2. The board is `unclean` on its own: the
        /// engine's `OMNITECT REACTOR PLANT` is not in the room vocabulary, so
        /// Hayoxi's offer stays unresolved and the panel is owed a re-read.
        fn recorded_full_read() -> Verdict {
            Verdict {
                status: TempleStatus::Read,
                unread: 0,
                headline: Some("change → Demolition Lab".to_string()),
                architect_index: Some(1),
                doors: vec!["B0-C1".to_string()],
                secondary_door: Some("B1-C1".to_string()),
                map_action: Some("continue".to_string()),
                warnings: vec!["architect target \"OMNITECT REACTOR PLANT\" is not a known room".to_string()],
                recommended_exit: Some(("B0-C1".to_string(), "House of the Others".to_string())),
                unclean: true,
            }
        }

        /// 13 plates × (name + numeral) + the two text regions.
        const OCR_CALLS: usize = 28;

        #[test]
        fn first_read_verdict_on_the_live_capture_is_the_recorded_one() {
            let img = live_capture();
            let ocr = script(&img, &live_layout(&img), &[]);

            let got = first_read(&ocr);

            ocr.assert_consumed();
            assert_eq!(*ocr.made.borrow(), OCR_CALLS);
            assert_eq!(got, recorded_full_read());
        }

        /// Two plates read as nothing: they count as unread, the board is
        /// unclean, and the exit through House of the Others (plate 1) loses
        /// its name — a guess is worse than silence — while the ranked move,
        /// which needs neither plate, is unchanged.
        #[test]
        fn first_read_verdict_with_two_unread_plates_is_the_recorded_partial() {
            let img = live_capture();
            let ocr = script(&img, &live_layout(&img), &UNREAD_PLATES);

            let got = first_read(&ocr);

            ocr.assert_consumed();
            assert_eq!(*ocr.made.borrow(), OCR_CALLS);
            assert_eq!(
                got,
                Verdict { unread: 2, recommended_exit: None, unclean: true, ..recorded_full_read() },
            );
        }

        /// The panel's OCR fails: the read fails with HEAD's message, buys no
        /// plate call, and remembers nothing.
        #[test]
        fn a_failed_panel_ocr_fails_the_read_before_any_plate() {
            let img = live_capture();
            let ocr = script(&img, &live_layout(&img), &[]);
            ocr.calls.borrow_mut()[0].response = Err("engine down".to_string());

            let (_, done) = press_read(&ocr, &|| false);

            assert_eq!(*ocr.made.borrow(), 1, "no call after the failed one");
            assert_eq!(done.publish.unwrap_err(), "Temple: OCR failed — engine down");
            assert_eq!(done.remember, None);
        }

        /// A stop raised before the plates ends the read with no plate call
        /// and nothing published or remembered.
        #[test]
        fn a_stopped_read_is_cancelled_after_the_text_regions() {
            let img = live_capture();
            let ocr = script(&img, &live_layout(&img), &[]);

            let (_, done) = press_read(&ocr, &|| true);

            assert_eq!(*ocr.made.borrow(), 2, "the two text regions, no plate");
            assert_eq!(done.publish.unwrap_err(), "cancelled");
            assert_eq!(done.remember, None);
        }
    }

    // ------------------------------------------------- what a press may persist --

    fn a_read_slice() -> TempleSlice {
        TempleSlice { status: TempleStatus::Read, ..TempleSlice::default() }
    }

    #[test]
    fn a_successful_read_remembers_its_located_origin() {
        assert_eq!(finish(Ok(a_read_slice()), Some((745, 561))).remember, Some([745, 561]));
    }

    #[test]
    fn a_failed_read_remembers_nothing() {
        let done = finish(Err("Temple: OCR failed — engine down".to_string()), Some((745, 561)));
        assert_eq!(done.remember, None);
    }

    /// A recheck hit, or a located anchor inside the tolerance band, has no
    /// fallback origin to remember.
    #[test]
    fn a_successful_read_with_no_located_origin_remembers_nothing() {
        assert_eq!(finish(Ok(a_read_slice()), None).remember, None);
    }

    // -------------------------------------------------- the locate on a miss --

    #[test]
    fn a_null_slice_is_located_on_the_pressed_frame() {
        assert!(locate_on_miss(false, None));
    }

    /// A slice with a seeded but never ANCHORED origin is unplaced (POE-278).
    #[test]
    fn an_unanchored_slice_is_located_on_the_pressed_frame() {
        assert!(locate_on_miss(true, None));
    }

    /// A placed miss is the sheet not being open: no locate, `NoPanel`.
    #[test]
    fn a_placed_miss_is_not_located() {
        assert!(!locate_on_miss(true, Some([960, 713])));
    }

    /// The live frame with no placement: the locate that a null slice buys
    /// finds the sheet at the live origin, on that same frame.
    #[test]
    fn the_locate_finds_the_panel_on_the_live_frame() {
        let img = live_capture();
        let found = anchor::anchor_for_loop(&img, None, &|| false).expect("the live sheet is found");
        let tol = slice::FRAME_ORIGIN_TOLERANCE;
        assert!(
            (found.origin.0 - 960).abs() <= tol && (found.origin.1 - 713).abs() <= tol,
            "found {:?}",
            found.origin,
        );
    }
}
