use std::collections::BTreeSet;

use super::super::lattice::{Edge, Slot};
use super::super::panel::{ARCHITECTS_PER_PANEL, PanelReading, RoomReading};
use super::super::reader::TempleLayout;

// ----------------------------------------------- one board, read and kept --

/// The two text regions `super::run::text_regions` crops, by name.
///
/// Declared here rather than spelled out at each use because [`unclean`] is the
/// second consumer: it is handed the names `super::run::clipped_text_rois`
/// reported as OUTSIDE the capture, and has to know which read failures each of
/// them explains. Two string literals in two files is a drift the compiler
/// cannot see — a renamed region would silently stop exempting anything.
pub const PANEL_REGION: &str = "panel";
/// The budget line's region — see [`PANEL_REGION`].
pub const REMAINING_REGION: &str = "remaining";

/// How far the anchor origin may move before the sheet counts as MOVED, px.
///
/// # What two covers, in the terms this codebase actually quantifies
///
/// Not still-frame jitter — **nothing has measured the correlation peak's
/// per-frame stability on a still screen**, and that sentence is the honest
/// state of it. What IS quantified is the ROUTE the origin was found by, and the
/// two routes disagree by a known amount:
///
/// - The fast path, `super::anchor::detect_cheap`'s recheck, is a
///   full-resolution correlation over a fixed window at a fixed scale. Given the
///   same pixels it returns the same peak, so two frames of a still sheet that
///   both take this route differ by nothing. Two covers it exactly.
/// - The sweep path budgets more. `super::anchor`'s `SWEEP_FINE_RADIUS` is
///   `FINE_RADIUS + 4`, the four being the position error a coarse scale step
///   buys, and the measurement beside it is that the nominee at scale 1.00 sits
///   **2 px** from the true top-left. So two covers the measured case and half
///   the budgeted one.
///
/// A fallback can land outside the placed-origin band. That case is not free
/// or silent: the read uses the fallback origin, and a successful read remembers
/// it for later placements. The ordinary read/retry rules still apply.
///
/// What the band stops is the unbounded version of the same thing: at zero
/// tolerance every re-found origin is a new board with a fresh retry budget, so
/// an anchor that would not sit still paid 28 OCR calls every 650 ms.
pub const FRAME_ORIGIN_TOLERANCE: i32 = 2;

/// One completed read of one board, kept so a retry can fill in what it missed.
///
/// The five things a read produces that a LATER read of the same board could
/// improve on — everything [`ReadResult`] is built from except the settings
/// echo and the timestamps, which belong to the tick rather than to the board.
/// Owned rather than borrowed because it outlives the tick that read it: it
/// lives on `super::run::Session`, which is the loop's own state, and the
/// projection is rebuilt from it on every retry.
///
/// It is deliberately NOT on `super::run::LoopState`, which derives `Eq` and is
/// the loop's cheap bookkeeping. `super::run::BoardRead` is the bookkeeping half
/// — a key, a status, a retry budget — and this is the payload.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptRead {
    pub layout: TempleLayout,
    pub rooms: Vec<RoomReading>,
    pub panel: PanelReading,
    /// The door set the diamond settled, or `None` on the marker fallback.
    pub settled: Option<BTreeSet<Edge>>,
    /// Why the diamond read failed, when it did.
    pub marker_error: Option<String>,
}

/// What one round of reading a board will OCR — POE-249 WI-2.
///
/// # The rule it expresses (owner, 2026-09-07)
///
/// *"We do up to 2 more rounds of temple reading, but only for the parts that
/// was previously not clear, so we in fact do 3 reading rounds total."*
///
/// Round 1 of a board has no kept read to improve on and reads everything
/// ([`Self::full`]). Rounds 2 and 3 — the [`super::run::RETRIES`] budget — read
/// ONLY the regions the kept read still has unclean, and the decision is made
/// from the kept read BEFORE any OCR is paid for.
///
/// # Why it is the same function as [`unclean`]
///
/// The two questions are one question asked twice: "is this read worth another
/// round?" and "which parts of it is that round for?". Answering them from two
/// bodies of code is a drift the compiler cannot see — a region added to one
/// would buy rounds that never re-read it, or be re-read on a board nothing
/// said was dirty. So [`retry_plan`] is the single set of per-region
/// predicates, [`plan_read`] is its front door and [`unclean`] is
/// `!plan.is_empty()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPlan {
    /// The plate slots to OCR. Every other slot is filled with
    /// [`Match::Unknown`] by `super::panel::read_slots`.
    pub plates: Vec<Slot>,
    /// The text regions to crop, by name, in `super::run::text_regions`' own
    /// order. [`PANEL_REGION`] and [`REMAINING_REGION`] are the two names, and
    /// they are declared here for this reason: the plan, [`unclean`] and the
    /// region list must agree letter for letter.
    pub text: Vec<&'static str>,
    /// Whether to read the door diamond.
    pub markers: bool,
}

impl ReadPlan {
    /// Everything: 13 plates, both text regions, the diamond. What round 1 of
    /// every board reads, and what a board whose kept reading was dropped
    /// (another key, a moved frame — `super::run::kept_for`) reads again.
    pub fn full() -> Self {
        Self {
            plates: Slot::ALL.to_vec(),
            text: vec![PANEL_REGION, REMAINING_REGION],
            markers: true,
        }
    }

    /// Nothing left to re-read — the read this plan was derived from is clean.
    pub fn is_empty(&self) -> bool {
        self.plates.is_empty() && self.text.is_empty() && !self.markers
    }

    /// What the round did, for the read line
    /// (`super::run::read_timings_line`): `full`, or the regions it re-read.
    ///
    /// `full` is decided by comparing against [`Self::full`] rather than by the
    /// round number, so a round 2 that happens to owe every region says `full`
    /// and means it.
    pub fn describe(&self) -> String {
        if *self == Self::full() {
            return "full".to_string();
        }
        if self.is_empty() {
            return "nothing left to re-read".to_string();
        }
        let mut parts = Vec::new();
        match self.plates.len() {
            0 => {}
            1 => parts.push("1 plate".to_string()),
            n => parts.push(format!("{n} plates")),
        }
        parts.extend(self.text.iter().map(|name| (*name).to_string()));
        if self.markers {
            parts.push("markers".to_string());
        }
        format!("re-read {}", parts.join(", "))
    }
}

/// The regions a RETRY of `read` would be for — POE-249 row 2's four causes,
/// minus the ones a clipped crop explains, each as the region it names rather
/// than as a bool.
///
/// `clipped` is the region names `super::run::clipped_text_rois` reported as
/// falling entirely OUTSIDE the capture this read was taken from. A region that
/// is not on screen cannot read better next time, so the failures it explains
/// are neither counted nor re-read: with [`REMAINING_REGION`] outside, the
/// budget is missing because the crop was empty, and with [`PANEL_REGION`]
/// outside, so are the architect blocks. Without this a windowed client whose
/// panel crop has walked off the monitor would pay three rounds per board
/// forever, re-crop the same empty region in each of them, and publish the same
/// empty panel every time.
///
/// The plates and the door diamond have no such exemption: they are placed off
/// the anchor inside the board itself, so a plate that did not read is a plate
/// OCR could not name, which is exactly what a retry is for.
fn retry_plan(read: &KeptRead, clipped: &[&'static str]) -> ReadPlan {
    let plates = read
        .rooms
        .iter()
        .filter(|reading| !reading.identity.is_known())
        .map(|reading| reading.slot)
        .collect();
    // In `super::run::text_regions`' order, which is what makes an all-regions
    // plan compare equal to `ReadPlan::full`.
    let mut text = Vec::new();
    if !clipped.contains(&PANEL_REGION)
        && (read.panel.architects.len() < ARCHITECTS_PER_PANEL
            || read
                .panel
                .architects
                .iter()
                .any(|offer| offer.target.identity().is_none()))
    {
        text.push(PANEL_REGION);
    }
    if !clipped.contains(&REMAINING_REGION) && read.panel.incursions_remaining.is_none() {
        text.push(REMAINING_REGION);
    }
    ReadPlan {
        plates,
        text,
        markers: read.marker_error.is_some(),
    }
}

/// What the round about to run should OCR, decided from the kept read alone.
///
/// `kept` is `super::run::kept_for`'s answer: the reading this loop may legally
/// fold into, which is `None` for the first look at a board and for a board
/// whose key or frame moved. No kept reading means nothing to improve on, so
/// there is nothing to be partial about and the round is [`ReadPlan::full`].
///
/// Called BEFORE any OCR of the round it plans — that is the whole point, and
/// it is why this takes a borrow: `super::run::full_read` still `take`s the
/// kept reading at the merge, so a round that bails out (a stop signal, a dead
/// OCR engine) leaves it standing for the next one.
pub fn plan_read(kept: Option<&KeptRead>, clipped: &[&'static str]) -> ReadPlan {
    match kept {
        None => ReadPlan::full(),
        Some(read) => retry_plan(read, clipped),
    }
}

/// Whether this read is worth spending a retry on — [`retry_plan`] asked as a
/// bool.
///
/// The causes, the clipped exemptions and their reasons are all
/// [`retry_plan`]'s: a read is unclean exactly when a retry would have
/// something to re-read.
pub fn unclean(read: &KeptRead, clipped: &[&'static str]) -> bool {
    !retry_plan(read, clipped).is_empty()
}
