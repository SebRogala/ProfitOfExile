//! Multi-scale anchoring of the temple layout panel (POE-168).
//!
//! The Entrance plate's art is pixel-identical in every screenshot, so a
//! normalised cross-correlation of one embedded crop of it recovers the whole
//! board's `(origin, scale)`. [`Slot::ENTRANCE`](super::lattice::Slot::ENTRANCE)
//! is the origin; everything else in [`super::lattice`] is a fixed multiple of
//! it.
//!
//! # Why the search is coarse-to-fine, and why the winner is a FINE score
//!
//! Scale is not free: the same panel measured 0.99–1.00 on 1352–1385 px
//! windows, 1.12 on 1494 px and 1.13 on 1539 px, and a fixed-scale match fell
//! to NCC 0.57 on the 1494 px board.
//!
//! Searching every scale at full resolution is too slow, so a ÷4 pass narrows
//! the field — but **only to nominate**. On the 1539 px board the ÷4 pass
//! ranked scale 1.09 *above* the true 1.13 (0.968 vs 0.961); trusting it
//! anchored at NCC 0.829 and turned the closed Apex corridor into an open one.
//! So each nominee is re-matched at full resolution and the winner is chosen on
//! that fine score. Reverting to a coarse-only winner is a correctness
//! regression, not a speed trade.
//!
//! # Why there is a floor
//!
//! Measured anchors: 0.942–1.000 when correct, 0.809 and 0.829 on the two
//! boards that produced a phantom Apex corridor. [`NCC_FLOOR`] sits in that
//! gap, and a match below it is an error rather than a guess — a
//! low-confidence anchor puts a wrong board in front of the player, which is
//! worse than no board.

// POE-171 is that caller: `temple::run` and `temple::slice` reach this module
// on every read, so the file-level `#![allow(dead_code)]` is gone. What is
// still uncalled carries its own attribute, which is now the inventory of what
// only the tests reach rather than a blanket over the whole file.

use std::sync::OnceLock;

use image::{imageops::FilterType, DynamicImage, RgbImage};

use super::ReadError;

/// The Entrance plate crop the whole module anchors on: box
/// `(595, 648, 752, 716)` of `tmp/alva-screenshots/2026-08-02_22-22-38.png`,
/// 157×68 px. Embedded because it is production input, not test data — the
/// reader cannot run without it.
const ENTRANCE_PLATE_PNG: &[u8] = include_bytes!("assets/entrance-plate.png");

/// Screen width the template was cut at.
///
/// **No longer a seed.** `image_width / this` was the opening guess for every
/// cold start until 2026-09-03, when a 1920x1080 laptop capture anchored at
/// scale 1.000 (NCC 0.99999) against a width seed of 1.397 — a 40% miss, far
/// outside the ±15% band it was searched in. The game's UI scales with screen
/// HEIGHT, so a
/// width-derived seed answers the wrong question; [`MEASURED_SCALES`] and
/// [`Scene::pyramid_sweep`] replace it. What it still does is set the ceiling
/// of [`full_sweep`], the exhaustive last resort — a coverage rule rather than
/// a guess about where the scale is.
pub const REFERENCE_SCREEN_WIDTH: u32 = 1374;

/// Temple scale ÷ shared `ui_scale` — the coefficient between this module's
/// scale unit and [`crate::ssot::ScreenSlice`]'s.
///
/// # Exactly what was measured, and what was not
///
/// **Measured** (2026-09-03, laptop debug dump `temple-debug/1788438639673`):
/// one half of it. A 1920x1080 capture anchors at temple scale **1.000**, NCC
/// 0.99999.
///
/// **Nominal**, not measured: the other half. `k = 1.000 / (1080 / 1200) =
/// 1.1111` divides by the shared unit's DEFINITION on a 1080 px screen, not by
/// a `ui_scale` the merc module measured on that machine — no merc reading from
/// that session was collected. The slice's own writer accepts a reading within
/// 0.01 of the standing one ("the documented 6-12 px OCR drift over the 1200-px
/// reference height"), and 0.01 on a 0.90 denominator is **~1% on k**.
///
/// So this is one anchored temple scale over a nominal denominator, and it is
/// enough only under the assumption both units are linear in the capture height
/// — which is what each is documented to be, and which no second measurement
/// yet contradicts. **The commit that makes the temple a slice writer must
/// recompute this against the slice's actual reading** once a session has both
/// numbers on the same machine, and should not treat 1.1111 as better than ±1%.
///
/// Nothing in this module consumes it: the conversion belongs to that commit.
/// It is defined here because this is where the temple's own unit is defined.
#[allow(dead_code)] // Consumed by the slice writer, which is not this commit.
pub const TEMPLE_SCALE_PER_UI_SCALE: f32 = 1.1111;

/// Fine-score NCC below which an anchor is rejected outright.
///
/// Measured: correct anchors scored 0.942–1.000 across 8 boards; the two wrong
/// anchors that invented an Apex corridor scored 0.809 and 0.829. 0.88 is the
/// middle of that gap.
pub const NCC_FLOOR: f32 = 0.88;

/// Scale grid step, both in the seeded band and in the full sweep.
const SCALE_STEP: f32 = 0.01;
/// Narrowest scale the fallback sweep considers, and its widest before the
/// seed-relative raise of the ceiling in [`full_sweep`]. The floor is fixed.
const FULL_SWEEP: (f32, f32) = (0.80, 1.60);
/// Downscale factor of the nominating pass.
///
/// 4, as in the prototype, and not further: at ÷6 the 1374 px reference board
/// stops nominating its own scale at all and the read fails outright
/// (measured 2026-08-18). The nominating pass is where essentially all the
/// search time goes, so this is the one constant worth re-measuring if that
/// ever needs to change.
const COARSE_DIVISOR: u32 = 4;
/// How many distinct coarse nominations get re-matched at full resolution.
const TOP_K_GROUPS: usize = 4;
/// Narrowest scale [`Scene::pyramid_sweep`] considers. [`FULL_SWEEP`]'s floor,
/// unchanged and for its stated reason.
const SWEEP_FLOOR: f32 = FULL_SWEEP.0;
/// Screen height the one full-screen measurement was taken at: 1080 px anchors
/// at temple scale 1.000 (see [`TEMPLE_SCALE_PER_UI_SCALE`]). The temple scale
/// tracks the capture HEIGHT, so `height / this` is what the sweep's ceiling is
/// built from.
const SWEEP_REFERENCE_HEIGHT: f32 = 1080.0;

/// The scale range [`Scene::pyramid_sweep`] covers on a capture this tall.
///
/// The ceiling is the capture's own height in units of
/// [`SWEEP_REFERENCE_HEIGHT`], floored at 2.00 so nothing narrows below what a
/// 2160 px screen needs.
///
/// # Why the ceiling has to follow the height rather than sit at 2.00
///
/// It is not about reaching a taller display — it is about what happens when
/// the sweep does NOT reach one. The ceiling is soft: the fine pass refines one
/// [`SWEEP_NOMINATE_STEP`] past the top nominee, so a capture whose true scale
/// is above the ceiling does not fail, it anchors APPROXIMATELY. Measured
/// 2026-09-03 on a synthetic plate at scale 2.10 against a fixed 2.00 ceiling:
/// the sweep answered 2.05 at NCC 0.9390, above [`NCC_FLOOR`].
///
/// `super::run` would then persist that as the screen's calibration, and a
/// calibration is what closes `super::run::SweepGate` — so a 2.5%-wrong scale
/// would be remembered, the gate would shut, and the loop would build every
/// lattice on it. A ceiling that follows the capture puts the true scale inside
/// the grid at the game's DEFAULT UI scale, whatever the display — which is the
/// case the soft edge would otherwise be reached by. The slider is a separate
/// matter and is not covered here; see [`anchor_for_loop`]'s note on what the
/// loop gives up.
fn sweep_range(height: u32) -> (f32, f32) {
    (
        SWEEP_FLOOR,
        (height as f32 / SWEEP_REFERENCE_HEIGHT).max(2.00),
    )
}
/// Scale step of the sweep's NOMINATING pass — coarser than [`SCALE_STEP`],
/// which is what makes the sweep affordable.
///
/// Measured 2026-09-03 in the Linux container, release, on
/// `screen-live-1920x1080.png`: nominating 0.80-2.79 at [`SCALE_STEP`] (what
/// [`full_sweep`] does) took 27.7 s of the whole search's 28.4 s. Nominating
/// 0.80-2.00 at 0.05 takes 4.8 s and puts the true 1.00 top of the ranking at
/// 0.9688, ahead of 1.05 (0.9646) and 0.95 (0.9220).
///
/// 0.05 and not wider because that is inside the ÷4 pass's demonstrated scale
/// tolerance: on the 1539 px board it ranked 1.09 above the true 1.13, i.e. it
/// still scores near its peak 0.04 away from the truth. Every nominee's
/// neighbourhood is then re-expanded at [`SCALE_STEP`], so the coarse step
/// costs nothing in the ANSWER's resolution — only in which neighbourhoods get
/// looked at.
const SWEEP_NOMINATE_STEP: f32 = 0.05;
/// How many distinct nominated SCALES have their neighbourhood refined at full
/// resolution.
///
/// Three, not [`TOP_K_GROUPS`]'s four: the nominating grid is coarse enough
/// that neighbouring entries no longer collapse into one coarse template size,
/// so these are three genuinely different scales rather than three tie groups.
/// The measured margin on the fixture is 0.9688 / 0.9646 / 0.9220 for the true
/// scale and its two neighbours — the truth is inside the first, and the two
/// after it are what covers the 1539 px board's failure mode, where the coarse
/// ranking put the wrong scale first.
const SWEEP_TOP_K: usize = 3;
/// Full-resolution search radius around a SWEEP nominee, in image px.
///
/// Wider than [`FINE_RADIUS`] by the position error a coarse scale step buys: a
/// template [`SWEEP_NOMINATE_STEP`] away from the truth centres its best match
/// up to `template_width * step / 2` px off, which at 157 px and 0.05 is 4 px.
/// Measured on the fixture, the nominee at 1.00 sits 2 px from the true
/// top-left and the whole 0.85-1.15 family spans 28 px.
const SWEEP_FINE_RADIUS: i32 = FINE_RADIUS + 4;
/// Full-resolution search radius around a nominee, in image px. A coarse
/// position is accurate to ±[`COARSE_DIVISOR`] px by construction; the rest is
/// slack for the resampling difference between the two resolutions.
const FINE_RADIUS: i32 = COARSE_DIVISOR as i32 + 8;

/// Temple anchor scales MEASURED on a real capture, by capture size.
///
/// This is DATA, not a model: each row is one screenshot somebody anchored and
/// read the scale off, with the dump it came from. No row is interpolated and
/// none is derived from a formula — the formula is what
/// [`REFERENCE_SCREEN_WIDTH`] used to be, and it was 40% wrong on the only
/// full-screen capture ever checked against it.
///
/// | capture | scale | NCC | provenance |
/// |---|---|---|---|
/// | 1920x1080 | 1.00 | 0.99999 | 2026-09-03, laptop dump `temple-debug/1788438639673`; committed as `tests/fixtures/temple/screen-live-1920x1080.png` |
///
/// That row is also the measurement that retired the width seed: `1920 / 1374`
/// is 1.397 against a true 1.000, and the ±15% band it was searched in
/// (1.19-1.61) does not contain the answer.
///
/// A hit is a HINT and not a promise: [`table_band`] matches it plus or minus
/// one [`SCALE_STEP`], and the result still has to clear [`NCC_FLOOR`] like any
/// other anchor. A miss falls to [`Scene::pyramid_sweep`], which is what a
/// resolution nobody has measured yet gets.
const MEASURED_SCALES: &[((u32, u32), f32)] = &[((1920, 1080), 1.00)];

/// The measured scale for a capture of exactly this size, if one exists.
///
/// Exact match on BOTH dimensions: the scale is a property of the render
/// resolution, and 1920x1080 says nothing about 1920x1200.
pub fn table_scale(width: u32, height: u32) -> Option<f32> {
    MEASURED_SCALES
        .iter()
        .find(|((w, h), _)| *w == width && *h == height)
        .map(|(_, scale)| *scale)
}

/// The scales to try for a capture size [`MEASURED_SCALES`] knows: the measured
/// one and one [`SCALE_STEP`] either side.
///
/// One step of slack, not a band: the measurement is of THIS screen size and
/// the neighbours exist only to absorb the resampling difference between the
/// dump the scale was read off and a live capture of the same screen. Widening
/// it would be re-introducing a guess under a measurement's name.
fn table_band(width: u32, height: u32) -> Option<Vec<f32>> {
    let scale = table_scale(width, height)?;
    Some(vec![scale - SCALE_STEP, scale, scale + SCALE_STEP])
}

/// A successful anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    /// Entrance plate centre in image px.
    pub origin: (i32, i32),
    /// Image px per reference px.
    pub scale: f32,
    /// The winning **fine** (full-resolution) NCC score.
    pub ncc: f32,
}

/// A remembered scale for one capture size.
///
/// POE-171 persists this in `settings.json`; this module only produces and
/// consumes it. It is keyed on the capture dimensions because the scale is a
/// property of the window size, so a different capture size invalidates it
/// outright rather than merely making it a worse guess.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnchorCalibration {
    pub screen_w: u32,
    pub screen_h: u32,
    pub scale: f32,
}

impl AnchorCalibration {
    /// The calibration measured on this anchor of an image of this size.
    pub fn of(img: &DynamicImage, anchor: &Anchor) -> AnchorCalibration {
        AnchorCalibration {
            screen_w: img.width(),
            screen_h: img.height(),
            scale: anchor.scale,
        }
    }

    fn applies_to(&self, img: &DynamicImage) -> bool {
        self.screen_w == img.width() && self.screen_h == img.height()
    }
}

/// Anchor with no prior knowledge: seeded band first, full sweep as fallback.
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub fn anchor(img: &DynamicImage) -> Result<Anchor, ReadError> {
    anchor_with_hint(img, None)
}

/// Anchor with everything there is, exhaustive last resort included.
///
/// **The capture loop never calls this.** Its step 3 is [`full_sweep`], which
/// measured 28.4 s in the Linux container and 347.8 s on the laptop that
/// reported POE-234 — a price a 1 Hz loop may not pay on a screen with nothing
/// on it. `super::run` calls [`anchor_for_loop`], which is this chain with that
/// step replaced by [`Scene::pyramid_sweep`]. What still reaches here is
/// `super::commands::temple_debug_capture`, where the user pressed a button and
/// is waiting, and `super::reader::read_layout`, which the tests and the
/// fixtures go through.
///
/// Three attempts, each stopping the moment one clears [`NCC_FLOOR`]:
///
/// 1. the hint's scale alone, when it was measured at this capture size;
/// 2. the [`table_band`] — the MEASURED scale for this capture size, ± one
///    [`SCALE_STEP`] — when [`MEASURED_SCALES`] has a row for it;
/// 3. the [`full_sweep`], exhaustive and slow, as the last resort.
///
/// Steps 2-3 are properties of the whole capture, so `img` must be a full
/// game-window capture; see [`super::reader::read_layout`].
///
/// A stale hint therefore costs one extra single-scale match and is never
/// *believed* — the guard against proceeding on a low-confidence anchor is the
/// floor, not the provenance of the scale.
///
/// **What used to be step 2 was `image_width / REFERENCE_SCREEN_WIDTH` ± 15%.**
/// It is gone from the decision path: measured 2026-09-03, a 1920x1080 capture
/// anchors at 1.000 and that seed says 1.397, so the band it produced could not
/// contain the answer on the one full screen ever checked against it.
///
/// Dropping it left `board-ref-1374.png` unchanged and moved
/// `board-live-1539.png` from **1.1320742 to 1.13**: the retired band was
/// stepped from `seed * 0.85`, so its scales were irrational offsets, while
/// step 3 is stepped from 0.80 and lands on the round value that fixture has
/// always called its true scale. Same plate, same origin, same door sets; the
/// one derived statistic that moved is the calibrated diagonal threshold, and
/// it is re-recorded in `super::reader::tests`'s `LIVE`.
///
/// # Why [`Scene::pyramid_sweep`] is not one of these attempts
///
/// It disagrees with step 3, and it is the more accurate of the two. Measured
/// 2026-09-03 by correlating every scale over the WHOLE board at full
/// resolution: `board-ref-1374.png` peaks at scale **1.00, NCC 0.9936**, and
/// this function returns 0.99 at 0.9603 — [`TOP_K_GROUPS`] admits four coarse
/// template SIZES, the [`SCALE_STEP`] grid packs thirteen of them into the band,
/// and the true peak's group falls outside the budget. [`Scene::pyramid_sweep`]
/// nominates [`SWEEP_NOMINATE_STEP`] apart, so its three neighbourhoods span
/// the same range with room to spare, and it finds the peak.
///
/// Being right about the PLATE is not yet a reason to change what this
/// function returns, because everything downstream was pinned against the
/// answer it gives: `super::reader`'s two board tests assert the prototype's
/// corridor set, and re-anchoring `board-ref-1374.png` at 1.00 closes the
/// `C2-D3` corridor the prototype reads as open. Which of the two lattices is
/// right is a question about the BOARD, answerable only against the source
/// screenshots, and it is not a question a cold-start latency change gets to
/// settle in passing. So the sweep is confined to [`anchor_for_loop`], which
/// is reached only where there was no answer at all.
pub fn anchor_with_hint(
    img: &DynamicImage,
    hint: Option<&AnchorCalibration>,
) -> Result<Anchor, ReadError> {
    let scene = Scene::of(img);
    let mut best: Option<Anchor> = None;

    let take = |found: Option<Anchor>, best: &mut Option<Anchor>| -> bool {
        if let Some(found) = found {
            if best.map_or(true, |b| found.ncc > b.ncc) {
                *best = Some(found);
            }
            return found.ncc >= NCC_FLOOR;
        }
        false
    };

    if let Some(h) = hint.filter(|h| h.applies_to(img)) {
        if take(scene.search(&[h.scale]), &mut best) {
            return Ok(best.expect("a cleared attempt recorded a best"));
        }
    }
    if let Some(band) = table_band(img.width(), img.height()) {
        if take(scene.search(&band), &mut best) {
            return Ok(best.expect("a cleared attempt recorded a best"));
        }
    }
    if take(scene.search(&full_sweep(img.width())), &mut best) {
        return Ok(best.expect("a cleared attempt recorded a best"));
    }

    Err(ReadError::AnchorNotFound {
        best_ncc: best.map_or(f32::NEG_INFINITY, |b| b.ncc),
    })
}

/// Anchor with everything a CAPTURE LOOP may pay for, and nothing more.
///
/// [`anchor_with_hint`]'s chain with its last resort swapped: hint, then
/// [`table_band`], then [`Scene::pyramid_sweep`] — never [`full_sweep`]. That is
/// the whole difference, and it is a cost decision, measured on the POE-234
/// capture in the Linux container (release): 5.3 s against 28.4 s, for the
/// identical answer of scale 1.000 at (960, 713).
///
/// # What the loop gives up by not reaching [`full_sweep`]
///
/// Scales past [`sweep_range`]'s ceiling — which follows the capture's own
/// height, precisely so a real screen's true scale is never past it. What is
/// left out is a scale the game's UI-scale slider put far above what the
/// capture's height implies, and the reason that matters is the SOFT edge: the
/// fine pass refines one [`SWEEP_NOMINATE_STEP`] beyond the top nominee, so a
/// scale above the ceiling is answered APPROXIMATELY rather than refused
/// (measured 2026-09-03 against a fixed 2.00 ceiling: a plate at 2.10 answered
/// 2.05 at NCC 0.9390, above [`NCC_FLOOR`]). `super::run` would persist that as
/// the screen's calibration and shut its own sweep gate on it.
/// `temple_debug_capture` still reaches the exhaustive sweep, and a
/// [`MEASURED_SCALES`] row makes it unnecessary.
///
/// # And where the two can disagree
///
/// [`Scene::pyramid_sweep`] nominates [`SWEEP_NOMINATE_STEP`] apart and refines
/// three neighbourhoods; [`Scene::search`] nominates [`SCALE_STEP`] apart and refines
/// [`TOP_K_GROUPS`] tie groups, which on a dense grid can miss the true fine
/// peak — measured on `board-ref-1374.png`, where the peak is 1.00 at NCC
/// 0.9936 and the exhaustive chain returns 0.99 at 0.9603 (see
/// [`anchor_with_hint`]'s note). So an uncalibrated screen the loop anchors
/// here can land on a scale the Debug button would not. Which is right is a
/// question about the BOARD, open at the time of writing; what is settled is
/// that the loop takes this one, because the other costs it a minute.
///
/// # `may_sweep` is the caller's budget, not a preference
///
/// The last step costs seconds and the two before it cost two correlations, so
/// only the caller knows whether this frame may pay — `super::run` asks its own
/// `SweepGate` once per tick and hands the answer to every path that could
/// reach the sweep. `false` means "hint and table only": the chain reports
/// `AnchorNotFound` rather than blocking, which is the right answer for a
/// promoted tick that arrived between cadences.
///
/// `stop` is polled between the sweep's coarse correlations — one per distinct
/// coarse template size, ~23 of them over [`sweep_range`] — so a module being
/// switched off mid-sweep stops within roughly a twenty-third of it. A stopped
/// sweep reports `AnchorNotFound`: it found nothing, which is true, and the
/// caller's next tick asks again.
pub fn anchor_for_loop(
    img: &DynamicImage,
    hint: Option<&AnchorCalibration>,
    may_sweep: bool,
    stop: &dyn Fn() -> bool,
) -> Result<Anchor, ReadError> {
    let scene = Scene::of(img);
    let mut best: Option<Anchor> = None;

    let take = |found: Option<Anchor>, best: &mut Option<Anchor>| -> bool {
        if let Some(found) = found {
            if best.map_or(true, |b| found.ncc > b.ncc) {
                *best = Some(found);
            }
            return found.ncc >= NCC_FLOOR;
        }
        false
    };

    if let Some(h) = hint.filter(|h| h.applies_to(img)) {
        if take(scene.search(&[h.scale]), &mut best) {
            return Ok(best.expect("a cleared attempt recorded a best"));
        }
    }
    if let Some(band) = table_band(img.width(), img.height()) {
        if take(scene.search(&band), &mut best) {
            return Ok(best.expect("a cleared attempt recorded a best"));
        }
    }
    if may_sweep && take(scene.pyramid_sweep(stop), &mut best) {
        return Ok(best.expect("a cleared attempt recorded a best"));
    }

    Err(ReadError::AnchorNotFound {
        best_ncc: best.map_or(f32::NEG_INFINITY, |b| b.ncc),
    })
}

/// What the cheap detect tick found — see [`detect_cheap`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CheapDetect {
    /// The remembered plate re-matched where it was last seen, at or above
    /// [`NCC_FLOOR`]. A real anchor, verified at full resolution.
    Anchored(Anchor),
    /// Nothing was verified, but the nominating pass found something
    /// plate-shaped at the width-derived scale. A *candidate*, never an
    /// anchor: coarse scores are the ones the whole module refuses to trust
    /// (see the file header), so this only means "worth the full read".
    Candidate { coarse_ncc: f32 },
    /// Neither. The tick can be skipped.
    Nothing { best_ncc: f32 },
}

impl CheapDetect {
    /// Whether this outcome is worth paying [`super::reader::read_layout_with_hint`] for.
    pub fn worth_reading(&self) -> bool {
        !matches!(self, CheapDetect::Nothing { .. })
    }
}

/// Where the cheap tick last saw the plate, so it can look there first.
///
/// Held in memory by the capture loop rather than persisted beside
/// [`AnchorCalibration`]: the scale is a property of the capture SIZE, which is
/// what that type is keyed on, while the origin is a property of the game
/// window's POSITION, which the same capture size does not pin. Storing the two
/// under one key would let a moved window write a stale origin to disk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheapHint {
    /// Screen size and scale, reusing [`AnchorCalibration::applies_to`].
    pub calibration: AnchorCalibration,
    /// Entrance plate centre in image px, as [`Anchor::origin`] reports it.
    pub origin: (i32, i32),
}

// No constructor from an `Anchor`: the capture loop's own source is a
// `reader::TempleLayout`, which already carries both fields, and a second way
// to build a two-field struct would only be a second thing to keep in step.

/// Coarse score above which the unverified nominating pass calls a candidate.
///
/// Measured 2026-08-19 at the width-derived scale alone, over the whole
/// capture: the two committed boards nominate at **0.939** and **0.938**,
/// deterministic noise of the same two sizes at **0.204** and **0.203**.
///
/// 0.70 sits in that gap, deliberately nearer the noise end. The two errors are
/// not symmetric and neither is unbounded: a false candidate costs exactly one
/// of the full reads the loop used to run on *every* tick, while a missed
/// candidate hides the panel until the caller's periodic full read. Buying
/// detection latency with an occasional tick of the old cost is the right side
/// of that trade — but it is the reason this floor is not tuned any closer to
/// 0.938 on a two-board sample.
pub const COARSE_CANDIDATE_FLOOR: f32 = 0.70;

/// The detect tick's cheap half: "is there anything here worth a full read?"
///
/// # Why this exists
///
/// [`anchor_with_hint`] is coarse-to-fine over a *band* of scales, and on a
/// MISS it runs every attempt it has — hint, [`table_band`], [`full_sweep`] —
/// because a miss is what "the band did not contain it" looks like. A closed
/// layout panel misses, and a closed panel is the state the capture loop lives
/// in, so the loop's steady state was the most expensive path in this module.
///
/// Measured 2026-08-19, release build, on deterministic noise the size of each
/// committed board fixture — i.e. a focused game with no layout panel, which is
/// where the loop spends its life:
///
/// | capture | [`super::reader::read_layout_with_hint`] | [`detect_cheap`] |
/// |---|---|---|
/// | 1374×542 | 92 correlations, 2 586 408 positions, 2.51 s | 2 correlations, 36 920 positions, 32 ms |
/// | 1539×613 | 105 correlations, 3 860 177 positions, 3.91 s | 2 correlations, 46 795 positions, 47 ms |
///
/// **~1/80 of the cost**, on both units and on both boards. (For scale, the
/// same hinted full read over a capture that *does* hold the panel is 2
/// correlations — the expense is entirely the two fallbacks a miss runs
/// through.)
///
/// This does at most two correlations instead, in this order:
///
/// 1. **The hint**, when one applies to this capture size: a single windowed
///    match at the remembered scale, in the same [`FINE_RADIUS`] box the fine
///    pass uses, around the remembered origin. Verified against [`NCC_FLOOR`]
///    like any other anchor, so a stale hint is never *believed* — it is just
///    the first place to look.
/// 2. **One nominating correlation** at [`coarse_candidate`]'s scale, against
///    [`COARSE_CANDIDATE_FLOOR`]. ONE scale, not a band, because the ÷4 pass is
///    famously scale-insensitive here (it ranked 1.09 above the true 1.13 at
///    0.968 vs 0.961) — the property that makes it useless as a *winner* is
///    what makes one of its scales enough as a *detector*.
///
/// Step 2 runs even when a hint applied and missed, which is what bounds the
/// recovery of a panel that moved or rescaled to one tick: without it a stale
/// origin would hide the panel until the caller's periodic full read.
///
/// **Step 2 is only as good as its one scale**, and on a capture size
/// [`MEASURED_SCALES`] does not know, that scale is the width seed — which
/// measured 1.397 against a true 1.000 on a 1920x1080 laptop and scored 0.66,
/// below the floor, on a panel that was open (2026-09-03). The recovery from
/// that is not here: it is [`anchor_for_loop`]'s cold-start sweep, which
/// `super::run` runs on a cadence while nothing is remembered.
pub fn detect_cheap(img: &DynamicImage, hint: Option<&CheapHint>) -> CheapDetect {
    if let Some(h) = hint.filter(|h| h.calibration.applies_to(img)) {
        if let Some(found) = recheck(img, h) {
            return CheapDetect::Anchored(found);
        }
    }
    match coarse_candidate(img) {
        Some(score) if score >= COARSE_CANDIDATE_FLOOR => CheapDetect::Candidate { coarse_ncc: score },
        Some(score) => CheapDetect::Nothing { best_ncc: score },
        None => CheapDetect::Nothing {
            best_ncc: f32::NEG_INFINITY,
        },
    }
}

/// One full-resolution windowed match at the hint's scale and origin.
///
/// `None` when the template does not fit, when the window is flat, or when the
/// score is below [`NCC_FLOOR`] — the same refusal [`anchor_with_hint`] makes,
/// for the same reason.
fn recheck(img: &DynamicImage, hint: &CheapHint) -> Option<Anchor> {
    let tmpl = template();
    let scale = hint.calibration.scale;
    let (tw, th) = (
        (tmpl.width() as f32 * scale) as u32,
        (tmpl.height() as f32 * scale) as u32,
    );
    // Before `full_level`, not after: a hint whose template cannot fit must not
    // buy a full-resolution integral pass to find that out.
    if tw == 0 || th == 0 || tw >= img.width() || th >= img.height() {
        return None;
    }
    let level = full_level(img);
    let scaled = Gray::from_rgb(&image::imageops::resize(tmpl, tw, th, FilterType::Triangle));
    // `hint.origin` is the plate CENTRE, as `Anchor::origin` reports it, and
    // `locate` works in top-left coordinates.
    let top_left = (
        hint.origin.0 - tw as i32 / 2,
        hint.origin.1 - th as i32 / 2,
    );
    let (x, y, score) = locate(&level, &scaled, Some(box_around(top_left, FINE_RADIUS)))?;
    (score >= NCC_FLOOR).then_some(Anchor {
        origin: (x + tw as i32 / 2, y + th as i32 / 2),
        scale,
        ncc: score,
    })
}

/// The best ÷4 score at ONE nominating scale, over the whole capture.
///
/// The scale is [`table_scale`]'s when this capture size has been measured, and
/// the width seed otherwise. That order is the whole of the fix here: at
/// 1920x1080 the width seed is 1.397 against a true 1.000 and this pass scored
/// 0.66 — under [`COARSE_CANDIDATE_FLOOR`], so the loop never promoted and
/// never found a panel that was on screen the whole time (measured 2026-09-03).
///
/// The width seed stays as the fallback because an unmeasured capture size has
/// nothing better, and the ÷4 pass's scale-insensitivity means it is right
/// often enough to be worth one correlation. When it is wrong, the capture
/// loop's cold-start sweep ([`anchor_for_loop`]) is what recovers — not this.
///
/// `None` when that scale produces a template the coarse level cannot hold.
fn coarse_candidate(img: &DynamicImage) -> Option<f32> {
    let level = coarse_level(img);
    let tmpl = template();
    let scale = table_scale(img.width(), img.height()).unwrap_or_else(|| seed_scale(img.width()));
    let size = (
        (tmpl.width() as f32 * scale / COARSE_DIVISOR as f32) as u32,
        (tmpl.height() as f32 * scale / COARSE_DIVISOR as f32) as u32,
    );
    if size.0 < 4
        || size.1 < 4
        || size.0 as usize >= level.gray.w
        || size.1 as usize >= level.gray.h
    {
        return None;
    }
    let scaled = Gray::from_rgb(&image::imageops::resize(
        tmpl,
        size.0,
        size.1,
        FilterType::Triangle,
    ));
    locate(&level, &scaled, None).map(|(_, _, score)| score)
}

/// The scale the capture's own width implies.
///
/// Not a seed any more — see [`REFERENCE_SCREEN_WIDTH`]. Two callers remain:
/// [`full_sweep`]'s ceiling, and [`coarse_candidate`]'s nominating scale on a
/// capture size nobody has measured.
fn seed_scale(width: u32) -> f32 {
    width as f32 / REFERENCE_SCREEN_WIDTH as f32
}

/// The exhaustive sweep for a capture this wide, the last resort behind
/// [`Scene::pyramid_sweep`].
///
/// [`FULL_SWEEP`]'s fixed 0.80–1.60 covers every window size measured so far,
/// but it is anchored on a 1374 px reference: a 2560 px capture seeds at 1.86
/// and a 3840 px one at 2.79, both above 1.60. A fixed band could therefore
/// never reach the true scale on exactly the captures the fast paths are most
/// likely to miss. The ceiling therefore rises to twice the width seed —
/// which, now that the seed is not trusted to point AT the scale, is doing the
/// only thing it is still good for: guaranteeing the exhaustive sweep reaches
/// past anything a wide capture could plausibly hold.
///
/// Only the ceiling. Widening downward would answer a case that does not
/// exist: a capture narrow enough to seed below 1.60 is already inside the
/// fixed band, and 0.80 is below the smallest scale the game's own UI produces.
fn full_sweep(width: u32) -> Vec<f32> {
    grid(FULL_SWEEP.0, FULL_SWEEP.1.max(seed_scale(width) * 2.0))
}

/// Scale candidates from `lo` to `hi` inclusive, on the [`SCALE_STEP`] grid.
fn grid(lo: f32, hi: f32) -> Vec<f32> {
    stepped(lo, hi, SCALE_STEP)
}

/// Scale candidates from `lo` to `hi` inclusive, `step` apart.
fn stepped(lo: f32, hi: f32, step: f32) -> Vec<f32> {
    let steps = ((hi - lo) / step).round().max(0.0) as usize;
    (0..=steps)
        .map(|i| (lo + i as f32 * step).max(0.01))
        .collect()
}

/// The Entrance template, decoded once per process.
fn template() -> &'static RgbImage {
    static TEMPLATE: OnceLock<RgbImage> = OnceLock::new();
    TEMPLATE.get_or_init(|| {
        image::load_from_memory(ENTRANCE_PLATE_PNG)
            .expect("the embedded Entrance plate decodes")
            .to_rgb8()
    })
}

/// A single-channel image plus the integral images the NCC denominator needs.
struct Gray {
    w: usize,
    h: usize,
    px: Vec<f32>,
}

impl Gray {
    fn from_rgb(img: &RgbImage) -> Gray {
        let px = img
            .pixels()
            .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
            .collect();
        Gray {
            w: img.width() as usize,
            h: img.height() as usize,
            px,
        }
    }
}

/// A gray image with the prefix sums the NCC denominator needs.
///
/// `sum` and `sum_sq` are `(w+1) × (h+1)`, so a window's sum and sum of squares
/// are O(1) and the only per-position work proportional to the template is the
/// correlation itself. Both resolutions carry them: the nominating pass runs
/// ~34 scales over the whole image and is where nearly all the time goes.
struct Level {
    gray: Gray,
    sum: Vec<f64>,
    sum_sq: Vec<f64>,
}

impl Level {
    fn of(gray: Gray) -> Level {
        let (sum, sum_sq) = integrals(&gray);
        Level { gray, sum, sum_sq }
    }
}

/// The scene at both resolutions the search needs.
struct Scene {
    full: Level,
    coarse: Level,
}

/// The capture at full resolution, with its prefix tables.
///
/// Built apart from [`coarse_level`] so a caller can pay for one resolution
/// instead of both. [`detect_cheap`] builds this one only when a hint applies,
/// and the coarse one only when it has to nominate — which on a hinted MISS is
/// both of them, because that path falls through from one to the other.
fn full_level(img: &DynamicImage) -> Level {
    Level::of(Gray::from_rgb(&img.to_rgb8()))
}

/// The capture at [`COARSE_DIVISOR`], with its prefix tables — see
/// [`full_level`].
fn coarse_level(img: &DynamicImage) -> Level {
    let (cw, ch) = (
        (img.width() / COARSE_DIVISOR).max(1),
        (img.height() / COARSE_DIVISOR).max(1),
    );
    Level::of(Gray::from_rgb(
        &img.resize_exact(cw, ch, FilterType::Triangle).to_rgb8(),
    ))
}

impl Scene {
    fn of(img: &DynamicImage) -> Scene {
        Scene {
            full: full_level(img),
            coarse: coarse_level(img),
        }
    }

    /// The nominating pass on its own, best coarse score first.
    ///
    /// Neighbouring scales collapse to the same integer template size once
    /// divided by [`COARSE_DIVISOR`] — at ÷4, scales 0.97/0.98/0.99 all give a
    /// 38×16 template and therefore the identical score. Each distinct size is
    /// correlated once and its result handed to every scale that produced it,
    /// which is exact (the inputs are the same image) and cuts the nominating
    /// pass, where nearly all the time goes, by the size of those groups.
    /// `None` when `stop` fired part-way: a partial ranking is not a worse
    /// answer, it is a DIFFERENT one, and returning it would let a cancelled
    /// sweep hand back a scale the full ranking would have beaten.
    fn nominate(&self, scales: &[f32], stop: &dyn Fn() -> bool) -> Option<Vec<Nominee>> {
        let tmpl = template();
        let mut computed: Vec<((u32, u32), Option<(i32, i32, f32)>)> = Vec::new();
        let mut nominees: Vec<Nominee> = Vec::new();
        for &scale in scales {
            let size = (
                (tmpl.width() as f32 * scale / COARSE_DIVISOR as f32) as u32,
                (tmpl.height() as f32 * scale / COARSE_DIVISOR as f32) as u32,
            );
            if size.0 < 4
                || size.1 < 4
                || size.0 as usize >= self.coarse.gray.w
                || size.1 as usize >= self.coarse.gray.h
            {
                continue;
            }
            let found = match computed.iter().find(|(k, _)| *k == size) {
                Some((_, found)) => *found,
                None => {
                    // Polled HERE and not per scale: this arm is the only one
                    // that costs anything (one correlation over the whole
                    // coarse level), so it is both the finest granularity that
                    // exists and the only one worth checking.
                    if stop() {
                        return None;
                    }
                    let scaled = Gray::from_rgb(&image::imageops::resize(
                        tmpl,
                        size.0,
                        size.1,
                        FilterType::Triangle,
                    ));
                    let found = locate(&self.coarse, &scaled, None);
                    computed.push((size, found));
                    found
                }
            };
            if let Some((x, y, score)) = found {
                nominees.push(Nominee {
                    score,
                    scale,
                    size,
                    at: (x, y),
                });
            }
        }
        // Stable, so scales tied at coarse resolution keep ascending order.
        nominees.sort_by(|a, b| b.score.total_cmp(&a.score));
        Some(nominees)
    }

    /// One nominee re-matched at FULL resolution, in a box of `radius` image px
    /// around the position the coarse pass put it at.
    ///
    /// The scale is the caller's, not the nominee's: [`Self::pyramid_sweep`]
    /// re-matches a whole [`SCALE_STEP`] neighbourhood around one coarse
    /// position, which is the point of nominating on a coarser grid than the
    /// answer is given on.
    fn refine(&self, scale: f32, coarse_at: (i32, i32), radius: i32) -> Option<Anchor> {
        let tmpl = template();
        let (tw, th) = (
            (tmpl.width() as f32 * scale) as u32,
            (tmpl.height() as f32 * scale) as u32,
        );
        if tw as usize >= self.full.gray.w || th as usize >= self.full.gray.h {
            return None;
        }
        let scaled = Gray::from_rgb(&image::imageops::resize(tmpl, tw, th, FilterType::Triangle));
        let origin = (
            coarse_at.0 * COARSE_DIVISOR as i32,
            coarse_at.1 * COARSE_DIVISOR as i32,
        );
        let (x, y, ncc) = locate(&self.full, &scaled, Some(box_around(origin, radius)))?;
        Some(Anchor {
            origin: (x + tw as i32 / 2, y + th as i32 / 2),
            scale,
            ncc,
        })
    }

    /// The cold-start sweep: nominate the WHOLE scale range on the ÷4 level,
    /// then refine only the best few neighbourhoods at full resolution.
    ///
    /// # Why this is not [`Self::search`] over [`full_sweep`]
    ///
    /// Both are coarse-to-fine and both pick their winner on the fine score.
    /// The difference is what the nominating pass is asked to do: `search`
    /// nominates every [`SCALE_STEP`] of the range, and measured 2026-09-03 in
    /// the Linux container (release) on `screen-live-1920x1080.png`, that is
    /// 27.7 s of a 28.4 s search — the same sweep took 347.8 s on the laptop
    /// that reported this. Nominating [`SWEEP_NOMINATE_STEP`] apart over
    /// [`sweep_range`] costs 4.8 s for the same answer (scale 1.00, origin
    /// (960, 713), NCC 0.99999).
    ///
    /// It cannot replace `search` outright: it looks at
    /// [`SWEEP_TOP_K`] neighbourhoods rather than [`TOP_K_GROUPS`] tie groups,
    /// and it stops at [`sweep_range`]'s ceiling. So `search` over
    /// [`full_sweep`] stays behind it in [`anchor_with_hint`] as the exhaustive
    /// last resort — reached by a caller that can afford it, and by no capture
    /// loop.
    ///
    /// Returns the best FINE-scored anchor found, which the caller checks
    /// against [`NCC_FLOOR`]; `None` when nothing fit or `stop` fired.
    fn pyramid_sweep(&self, stop: &dyn Fn() -> bool) -> Option<Anchor> {
        // The capture's OWN height, so the ceiling covers whatever screen this
        // is rather than whatever screen a constant was written for — see
        // `sweep_range`.
        let (lo, hi) = sweep_range(self.full.gray.h as u32);
        let coarse = stepped(lo, hi, SWEEP_NOMINATE_STEP);
        let nominees = self.nominate(&coarse, stop)?;
        let mut best: Option<Anchor> = None;
        for nominee in nominees.iter().take(SWEEP_TOP_K) {
            // One nomination step either side, on the answer's own grid: the
            // nominating pass only says WHICH neighbourhood, and the fine pass
            // is what says which scale in it.
            for scale in grid(
                nominee.scale - SWEEP_NOMINATE_STEP,
                nominee.scale + SWEEP_NOMINATE_STEP,
            ) {
                let Some(found) = self.refine(scale, nominee.at, SWEEP_FINE_RADIUS) else {
                    continue;
                };
                if best.map_or(true, |b| found.ncc > b.ncc) {
                    best = Some(found);
                }
            }
        }
        best
    }

    /// Coarse-nominate over `scales`, fine-verify the top groups, return the
    /// fine winner. `None` when no scale produced a template that fits.
    ///
    /// The cut-off counts distinct coarse **groups**, not scales: scales that
    /// share a coarse template are one nomination, and every scale in an
    /// admitted group is fine-matched. Counting scales instead would let a
    /// single group's ties eat the whole budget and hide the runner-up — which
    /// is the group the true scale sat in on the 1539 px board.
    fn search(&self, scales: &[f32]) -> Option<Anchor> {
        let nominees = self.nominate(scales, &|| false)?;
        let mut groups: Vec<(u32, u32)> = Vec::new();
        let mut best: Option<Anchor> = None;
        for nominee in &nominees {
            if !groups.contains(&nominee.size) {
                if groups.len() == TOP_K_GROUPS {
                    break;
                }
                groups.push(nominee.size);
            }
            let Some(found) = self.refine(nominee.scale, nominee.at, FINE_RADIUS) else {
                continue;
            };
            if best.map_or(true, |b| found.ncc > b.ncc) {
                best = Some(found);
            }
        }
        best
    }
}

/// The full-resolution search box around a coarse nominee's position,
/// `(x0, y0, x1, y1)` inclusive — `(2 · radius + 1)²` positions.
///
/// This bound is what keeps the fine pass affordable: without it each nominee
/// would be re-correlated over the whole capture at full resolution.
fn box_around(origin: (i32, i32), radius: i32) -> (i32, i32, i32, i32) {
    (
        origin.0 - radius,
        origin.1 - radius,
        origin.0 + radius,
        origin.1 + radius,
    )
}

/// One coarse-pass candidate.
#[derive(Debug, Clone, Copy)]
struct Nominee {
    /// Coarse NCC. Never the basis for the final answer.
    score: f32,
    scale: f32,
    /// Coarse template size — the identity of the group this scale fell into.
    size: (u32, u32),
    /// Coarse top-left of the match.
    at: (i32, i32),
}

/// Prefix sums of a gray image and of its squares, both `(w+1) * (h+1)`.
fn integrals(g: &Gray) -> (Vec<f64>, Vec<f64>) {
    let (w, h) = (g.w, g.h);
    let mut sum = vec![0.0f64; (w + 1) * (h + 1)];
    let mut sum_sq = vec![0.0f64; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0.0f64;
        let mut row_sq = 0.0f64;
        for x in 0..w {
            let v = g.px[y * w + x] as f64;
            row += v;
            row_sq += v * v;
            sum[(y + 1) * (w + 1) + x + 1] = sum[y * (w + 1) + x + 1] + row;
            sum_sq[(y + 1) * (w + 1) + x + 1] = sum_sq[y * (w + 1) + x + 1] + row_sq;
        }
    }
    (sum, sum_sq)
}

#[inline]
fn window_sum(table: &[f64], stride: usize, x: usize, y: usize, w: usize, h: usize) -> f64 {
    table[(y + h) * stride + x + w] - table[y * stride + x + w] - table[(y + h) * stride + x]
        + table[y * stride + x]
}

/// Brute-force normalised cross-correlation of `tmpl` over `scene`.
///
/// Returns the best `(x, y, score)` top-left position, or `None` when the
/// template does not fit or every window in range is flat. `window` is
/// `(x0, y0, x1, y1)` in scene px and is clamped.
///
/// The template is mean-subtracted once, which makes the numerator plain
/// `Σ w·t̂` — the `−mean(w)·Σt̂` term is zero — so the only per-position work
/// proportional to the template is that single dot product.
fn locate(
    level: &Level,
    tmpl: &Gray,
    window: Option<(i32, i32, i32, i32)>,
) -> Option<(i32, i32, f32)> {
    let scene = &level.gray;
    if tmpl.w == 0 || tmpl.h == 0 || tmpl.w > scene.w || tmpl.h > scene.h {
        return None;
    }
    let n = (tmpl.w * tmpl.h) as f64;
    let mean = tmpl.px.iter().map(|&v| v as f64).sum::<f64>() / n;
    let centred: Vec<f64> = tmpl.px.iter().map(|&v| v as f64 - mean).collect();
    let tn = centred.iter().map(|v| v * v).sum::<f64>().sqrt();
    if tn == 0.0 {
        return None;
    }

    let (max_x, max_y) = (scene.w - tmpl.w, scene.h - tmpl.h);
    let (x0, y0, x1, y1) = match window {
        Some((x0, y0, x1, y1)) => (
            x0.clamp(0, max_x as i32) as usize,
            y0.clamp(0, max_y as i32) as usize,
            x1.clamp(0, max_x as i32) as usize,
            y1.clamp(0, max_y as i32) as usize,
        ),
        None => (0, 0, max_x, max_y),
    };

    let stride = scene.w + 1;
    let mut best: Option<(i32, i32, f32)> = None;
    let mut positions = 0usize;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let s = window_sum(&level.sum, stride, x, y, tmpl.w, tmpl.h);
            let ss = window_sum(&level.sum_sq, stride, x, y, tmpl.w, tmpl.h);
            let denom = (ss - s * s / n).max(0.0).sqrt();
            if denom <= 0.0 {
                continue;
            }
            let mut dot = 0.0f64;
            for r in 0..tmpl.h {
                let row = &scene.px[(y + r) * scene.w + x..][..tmpl.w];
                let trow = &centred[r * tmpl.w..][..tmpl.w];
                for i in 0..tmpl.w {
                    dot += row[i] as f64 * trow[i];
                }
            }
            let score = (dot / (denom * tn)) as f32;
            positions += 1;
            if best.map_or(true, |b| score > b.2) {
                best = Some((x as i32, y as i32, score));
            }
        }
    }
    note_search(positions, window.is_some());
    best
}

/// What the correlations under one call did, in the two units the tests bound
/// them in.
///
/// `calls` and `positions` count every [`locate`]; `windowed_high_water` is the
/// largest single *windowed* call, kept apart so the unbounded nominating pass
/// cannot drown out the number the fine pass's radius is pinned by — and so a
/// fine pass that stopped passing a window at all records nothing there rather
/// than recording a larger figure.
#[cfg(test)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SearchTally {
    pub calls: usize,
    pub positions: usize,
    pub windowed_high_water: usize,
}

// Thread-local: the harness gives each test its own thread and a search never
// leaves the one it started on.
#[cfg(test)]
thread_local! {
    static TALLY: std::cell::Cell<SearchTally> = const { std::cell::Cell::new(SearchTally {
        calls: 0,
        positions: 0,
        windowed_high_water: 0,
    }) };
}

#[cfg(test)]
fn note_search(positions: usize, windowed: bool) {
    TALLY.with(|c| {
        let mut t = c.get();
        t.calls += 1;
        t.positions += positions;
        if windowed {
            t.windowed_high_water = t.windowed_high_water.max(positions);
        }
        c.set(t);
    });
}

#[cfg(not(test))]
fn note_search(_positions: usize, _windowed: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic noise. A flat field would be rejected by the zero-variance
    /// guard alone and would never exercise anything downstream of it.
    fn noise(w: u32, h: u32) -> RgbImage {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut img = RgbImage::new(w, h);
        for p in img.pixels_mut() {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let b = (seed >> 33) as u8;
            *p = image::Rgb([b, b.wrapping_add(40), b.wrapping_sub(30)]);
        }
        img
    }

    /// FNV-1a over the template's RGB bytes — an identity, not a property.
    fn fingerprint(img: &RgbImage) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in img.as_raw() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    /// The embedded asset is the exact crop every measured constant in this
    /// module was taken against, down to its pixels. Dimensions alone would let
    /// a re-cut or re-encoded 157×68 plate through, and every scale, origin and
    /// NCC recorded in the fixture tests would then be measuring a different
    /// image.
    #[test]
    fn embedded_template_is_the_measured_crop() {
        let t = template();
        assert_eq!((t.width(), t.height()), (157, 68));
        assert_eq!(*t.get_pixel(78, 34), image::Rgb([83, 63, 29]), "plate centre");
        assert_eq!(*t.get_pixel(0, 0), image::Rgb([17, 12, 7]), "top-left corner");
        assert_eq!(fingerprint(t), 0xa569_90da_6194_0533);
    }

    /// The measured 1920x1080 row comes back, and its band is the measurement
    /// plus and minus exactly one [`SCALE_STEP`].
    ///
    /// The width the same capture implies is 1.397 — the number this table
    /// exists to stop being consulted — so a band built from the seed instead
    /// would fail the containment assertion, and a band widened to
    /// the retired seed's ±15% would fail the length one.
    #[test]
    fn the_measured_1080p_row_bands_one_step_either_side_of_its_measurement() {
        assert_eq!(table_scale(1920, 1080), Some(1.00));

        let band = table_band(1920, 1080).expect("a measured size has a band");

        assert_eq!(band.len(), 3, "band {band:?} is not the measurement ± one step");
        assert!(
            (band[0] - 0.99).abs() < 1e-5 && (band[2] - 1.01).abs() < 1e-5,
            "band {band:?} does not straddle the measured 1.00 by {SCALE_STEP}"
        );
    }

    /// A capture size nobody has measured has no row and no band — it must fall
    /// to the sweep rather than borrow a neighbour's number.
    ///
    /// 1920x1200 is the case that matters: it shares a WIDTH with the measured
    /// row, and the temple scale tracks the capture height, so a lookup that
    /// matched on width alone would hand a 1200 px screen a 1080 px screen's
    /// scale.
    #[test]
    fn an_unmeasured_capture_size_has_no_table_entry() {
        assert_eq!(table_scale(1920, 1200), None, "same width, different height");
        assert_eq!(table_scale(2560, 1440), None);
        assert!(table_band(1920, 1200).is_none());
    }

    /// The fallback sweep must be able to reach the scale a wide capture
    /// implies. 2560 px seeds at 1.86 and 3840 px at 2.79, both above the fixed
    /// 1.60 ceiling — a sweep that stopped there could never rescue the two
    /// capture sizes whose seeded band most needs rescuing.
    #[test]
    fn the_fallback_sweep_reaches_the_scale_a_wide_capture_implies() {
        for width in [2560u32, 3840] {
            let seed = width as f32 / REFERENCE_SCREEN_WIDTH as f32;
            let sweep = full_sweep(width);
            let (lo, hi) = (*sweep.first().unwrap(), *sweep.last().unwrap());
            assert!(
                lo <= seed && hi >= seed,
                "{width} px seeds at {seed}, outside the sweep {lo}..{hi}"
            );
        }
    }

    /// …without giving up the measured band on the capture sizes that already
    /// worked: a narrow capture still sweeps the whole of 0.80–1.60.
    #[test]
    fn the_fallback_sweep_still_covers_the_measured_band() {
        let sweep = full_sweep(1374);
        let (lo, hi) = (*sweep.first().unwrap(), *sweep.last().unwrap());
        assert!(
            lo <= FULL_SWEEP.0 && hi >= FULL_SWEEP.1,
            "sweep {lo}..{hi} does not cover {FULL_SWEEP:?}"
        );
    }

    /// A plate that IS there but matches poorly is the dangerous case — it is
    /// the shape the two live boards that invented an Apex corridor had (NCC
    /// 0.809 and 0.829). Blending the template 75% into noise reproduces it at
    /// 0.85, above every "obviously not a plate" score and still below the
    /// floor, and the reader must refuse it rather than return coordinates.
    #[test]
    fn a_plate_matching_between_the_wrong_anchors_and_the_floor_is_rejected() {
        let mut img = noise(300, 220);
        for (x, y, tp) in template().enumerate_pixels() {
            let sp = *img.get_pixel(x + 70, y + 70);
            let mix = |t: u8, s: u8| ((t as u32 * 75 + s as u32 * 25) / 100) as u8;
            let blended = image::Rgb([
                mix(tp[0], sp[0]),
                mix(tp[1], sp[1]),
                mix(tp[2], sp[2]),
            ]);
            img.put_pixel(x + 70, y + 70, blended);
        }
        match anchor(&DynamicImage::ImageRgb8(img)) {
            Err(super::super::ReadError::AnchorNotFound { best_ncc }) => assert!(
                (0.80..NCC_FLOOR).contains(&best_ncc),
                "expected a near-miss inside 0.80..{NCC_FLOOR}, got {best_ncc} — \
                 the construction has drifted and no longer probes the floor"
            ),
            other => panic!("expected AnchorNotFound, got {other:?}"),
        }
    }

    /// The full sweep is a real fallback, not dead code: a plate at scale 1.30
    /// in a 260 px-wide capture is nowhere near what the retired width seed
    /// implied (`260 / 1374` is 0.19), has no [`MEASURED_SCALES`] row, and the
    /// sweep still finds it.
    #[test]
    fn the_full_sweep_anchors_a_scale_no_earlier_attempt_can_reach() {
        let scale = 1.30f32;
        let t = template();
        let big = image::imageops::resize(
            t,
            (t.width() as f32 * scale) as u32,
            (t.height() as f32 * scale) as u32,
            FilterType::Triangle,
        );
        let mut img = noise(260, 140);
        assert_eq!(
            table_scale(img.width(), img.height()),
            None,
            "the fixture must not be a measured size, or this tests the table"
        );
        image::imageops::replace(&mut img, &big, 10, 10);

        let found = anchor(&DynamicImage::ImageRgb8(img)).expect("the full sweep finds it");

        assert!(
            (found.scale - scale).abs() <= 0.03,
            "recovered scale {}, not {scale}",
            found.scale
        );
    }

    /// …and the cold-start sweep finds the same plate on the same capture, which
    /// is what makes it a usable substitute for the caller that cannot afford
    /// the full sweep.
    #[test]
    fn the_cold_start_sweep_anchors_the_same_plate_the_full_sweep_does() {
        let scale = 1.30f32;
        let t = template();
        let big = image::imageops::resize(
            t,
            (t.width() as f32 * scale) as u32,
            (t.height() as f32 * scale) as u32,
            FilterType::Triangle,
        );
        let mut img = noise(260, 140);
        image::imageops::replace(&mut img, &big, 10, 10);
        let img = DynamicImage::ImageRgb8(img);
        let exhaustive = anchor(&img).expect("the full sweep finds it");

        let fast = Scene::of(&img)
            .pyramid_sweep(&|| false)
            .expect("and so does the sweep");

        assert!(
            (fast.scale - exhaustive.scale).abs() <= 1.5 * SCALE_STEP,
            "the sweep recovered {} where the full sweep recovered {}",
            fast.scale,
            exhaustive.scale
        );
        assert!(
            (fast.origin.0 - exhaustive.origin.0).abs() <= 2
                && (fast.origin.1 - exhaustive.origin.1).abs() <= 2,
            "the sweep put the plate at {:?}, the full sweep at {:?}",
            fast.origin,
            exhaustive.origin
        );
    }

    /// The fine pass is bounded to [`FINE_RADIUS`] around each nominee. Without
    /// that bound every nominee is re-correlated over the whole capture at full
    /// resolution, which is the cost the coarse pass exists to avoid.
    #[test]
    fn the_fine_pass_searches_only_the_radius_around_each_nominee() {
        let side = (2 * FINE_RADIUS_FOR_TEST + 1) as usize;
        let (x0, y0, x1, y1) = fine_window_for_test((200, 150));
        assert_eq!(
            ((x1 - x0 + 1) as usize) * ((y1 - y0 + 1) as usize),
            side * side,
            "fine window {:?}",
            (x0, y0, x1, y1)
        );

        let mut img = noise(200, 110);
        image::imageops::replace(&mut img, template(), 21, 21);
        let (found, positions) =
            windowed_search_high_water(|| anchor(&DynamicImage::ImageRgb8(img)));
        found.expect("the pasted plate anchors");
        assert_eq!(
            positions,
            side * side,
            "the fine pass correlated {positions} positions in its largest \
             windowed call; the bound is {FINE_RADIUS_FOR_TEST} px, and 0 means \
             it verified unwindowed"
        );
    }

    /// The SWEEP's fine pass verifies in a WINDOW, and the window is
    /// [`SWEEP_FINE_RADIUS`] wide.
    ///
    /// The load-bearing half is that it is windowed at all: 0 here means the
    /// sweep re-correlated a nominee over the whole capture at full resolution,
    /// which is the cost the entire pyramid exists to avoid. The exact radius
    /// rides along because the tally cannot report one without the other — a
    /// changed radius is a deliberate edit to a documented constant, not the
    /// regression this is watching for.
    #[test]
    fn the_sweeps_fine_pass_searches_only_its_own_radius_around_each_nominee() {
        let side = (2 * SWEEP_FINE_RADIUS_FOR_TEST + 1) as usize;

        let mut img = noise(200, 110);
        image::imageops::replace(&mut img, template(), 21, 21);
        let (found, positions) = windowed_search_high_water(|| {
            Scene::of(&DynamicImage::ImageRgb8(img)).pyramid_sweep(&|| false)
        });

        found.expect("the pasted plate anchors");
        assert_eq!(
            positions,
            side * side,
            "the sweep's fine pass correlated {positions} positions in its largest \
             windowed call; the bound is {SWEEP_FINE_RADIUS_FOR_TEST} px"
        );
    }

    /// NCC of a patch against itself is 1, and against its own negative is −1 —
    /// the two ends the whole search is scored on.
    #[test]
    fn ncc_is_one_on_an_exact_match_and_minus_one_when_inverted() {
        let px: Vec<f32> = (0..48).map(|i| ((i * 37) % 251) as f32).collect();
        let tmpl = Gray {
            w: 8,
            h: 6,
            px: px.clone(),
        };
        let same = Gray {
            w: 8,
            h: 6,
            px: px.clone(),
        };
        let (x, y, s) = locate(&Level::of(same), &tmpl, None).expect("fits");
        assert_eq!((x, y), (0, 0));
        assert!((s - 1.0).abs() < 1e-4, "exact match scored {s}");

        let inverted = Gray {
            w: 8,
            h: 6,
            px: px.iter().map(|v| 255.0 - v).collect(),
        };
        let (_, _, s) = locate(&Level::of(inverted), &tmpl, None).expect("fits");
        assert!((s + 1.0).abs() < 1e-4, "inverted match scored {s}");
    }

    /// The whole search's speed rests on reading a window's sum and sum of
    /// squares off the prefix tables instead of walking it. They must agree
    /// with the walk for every window, not just the one at the origin.
    #[test]
    fn prefix_tables_reproduce_a_walked_window() {
        let g = Gray {
            w: 20,
            h: 16,
            px: (0..320).map(|i| ((i * 101) % 199) as f32).collect(),
        };
        let (sum, sum_sq) = integrals(&g);
        for (x, y, w, h) in [(0, 0, 20, 16), (3, 2, 5, 4), (15, 12, 5, 4), (7, 0, 1, 1)] {
            let mut walked = 0.0f64;
            let mut walked_sq = 0.0f64;
            for r in y..y + h {
                for c in x..x + w {
                    let v = g.px[r * g.w + c] as f64;
                    walked += v;
                    walked_sq += v * v;
                }
            }
            assert!(
                (window_sum(&sum, g.w + 1, x, y, w, h) - walked).abs() < 1e-6,
                "sum over ({x},{y},{w},{h})"
            );
            assert!(
                (window_sum(&sum_sq, g.w + 1, x, y, w, h) - walked_sq).abs() < 1e-3,
                "sum of squares over ({x},{y},{w},{h})"
            );
        }
    }

    /// A flat window has zero variance and no defined correlation; it must be
    /// skipped rather than scored, or a blank screen anchors at 0/0.
    #[test]
    fn flat_scene_yields_no_match() {
        let scene = Gray {
            w: 12,
            h: 12,
            px: vec![80.0; 144],
        };
        let tmpl = Gray {
            w: 4,
            h: 4,
            px: (0..16).map(|i| i as f32).collect(),
        };
        assert!(locate(&Level::of(scene), &tmpl, None).is_none());
    }

    // ------------------------------------------------ the cheap detect tick --

    /// A committed board fixture, with the `(scale, origin)` its
    /// [`super::super::reader`] test pins.
    struct Board {
        file: &'static str,
        scale: f32,
        origin: (i32, i32),
    }

    const BOARDS: [Board; 2] = [
        Board {
            file: "board-ref-1374.png",
            scale: 0.99,
            origin: (673, 494),
        },
        Board {
            file: "board-live-1539.png",
            scale: 1.13,
            origin: (745, 561),
        },
    ];

    impl Board {
        fn load(&self) -> DynamicImage {
            let path = format!(
                "{}/tests/fixtures/temple/{}",
                env!("CARGO_MANIFEST_DIR"),
                self.file
            );
            image::open(&path).unwrap_or_else(|e| panic!("{path} loads: {e}"))
        }

        fn hint(&self, img: &DynamicImage) -> CheapHint {
            CheapHint {
                calibration: AnchorCalibration {
                    screen_w: img.width(),
                    screen_h: img.height(),
                    scale: self.scale,
                },
                origin: self.origin,
            }
        }
    }

    /// The hinted path re-anchors a panel it has already seen in ONE windowed
    /// match, on both scale families.
    ///
    /// The position count is half the assertion: a `recheck` that dropped the
    /// window, or that mistook the hint's plate CENTRE for a top-left, would
    /// either search the whole capture (the cost this whole path exists to
    /// avoid) or search 78 px away from the plate and find nothing.
    #[test]
    fn a_hinted_cheap_detect_re_anchors_both_boards_in_one_windowed_match() {
        for board in BOARDS {
            let img = board.load();
            let (found, tally) = search_tally(|| detect_cheap(&img, Some(&board.hint(&img))));

            let CheapDetect::Anchored(anchor) = found else {
                panic!("{}: expected an anchor, got {found:?}", board.file)
            };
            assert_eq!(anchor.scale, board.scale, "{}: the hint's scale", board.file);
            assert!(
                (anchor.origin.0 - board.origin.0).abs() <= 3
                    && (anchor.origin.1 - board.origin.1).abs() <= 3,
                "{}: origin {:?} is more than 3 px from {:?}",
                board.file,
                anchor.origin,
                board.origin
            );
            assert!(
                anchor.ncc >= NCC_FLOOR,
                "{}: NCC {} below the floor",
                board.file,
                anchor.ncc
            );
            assert_eq!(
                (tally.calls, tally.positions),
                (1, ((2 * FINE_RADIUS + 1) as usize).pow(2)),
                "{}: the hinted path is one windowed match and nothing else",
                board.file
            );
        }
    }

    /// With no hint the tick nominates rather than anchors: one *unwindowed*
    /// ÷4 correlation at the width-derived scale, above
    /// [`COARSE_CANDIDATE_FLOOR`], on both boards.
    ///
    /// One correlation, not the seeded band's fourteen: fails if the unhinted
    /// path is widened back into a band, and fails if it starts verifying at
    /// full resolution (which would register a windowed call).
    #[test]
    fn an_unhinted_cheap_detect_nominates_both_boards_in_one_coarse_pass() {
        for board in BOARDS {
            let img = board.load();
            let (found, tally) = search_tally(|| detect_cheap(&img, None));

            let CheapDetect::Candidate { coarse_ncc } = found else {
                panic!("{}: expected a candidate, got {found:?}", board.file)
            };
            // The measured margin, not just the ordering: pinning this against
            // `COARSE_CANDIDATE_FLOOR` alone would let the pass degrade all the
            // way to 0.70 — into the band where a busy game screen lives —
            // while still passing. Measured 0.939 and 0.938.
            assert!(
                coarse_ncc >= 0.90,
                "{}: nominated at {coarse_ncc}; the measured boards score 0.938+, \
                 and a pass that has drifted to the floor is not a detector",
                board.file
            );
            assert_eq!(tally.calls, 1, "{}: one correlation", board.file);
            assert_eq!(
                tally.windowed_high_water, 0,
                "{}: the unhinted path must not verify at full resolution",
                board.file
            );
        }
    }

    /// A screen with no plate on it is `Nothing`, hint or no hint — otherwise
    /// the loop promotes to a full read on every tick and the cheap tick buys
    /// nothing.
    #[test]
    fn a_capture_with_no_plate_is_nothing_with_or_without_a_hint() {
        for board in BOARDS {
            let img = board.load();
            let empty = DynamicImage::ImageRgb8(noise(img.width(), img.height()));

            for hint in [None, Some(board.hint(&img))] {
                let found = detect_cheap(&empty, hint.as_ref());
                let CheapDetect::Nothing { best_ncc } = found else {
                    panic!("{}: expected nothing, got {found:?}", board.file)
                };
                // Measured 0.204 and 0.203. Pinned as a margin for the same
                // reason the boards are: a pass that crept up to 0.69 on an
                // empty screen still clears this assertion against the floor
                // alone, and has no headroom left for a real game background.
                assert!(
                    best_ncc <= 0.35,
                    "{}: scored {best_ncc} on an empty screen; the measured \
                     noise scores 0.21, and {COARSE_CANDIDATE_FLOOR} is the \
                     floor this has to stay clear of",
                    board.file
                );
            }
        }
    }

    /// A panel that moved — the hint's scale still applies, its origin does
    /// not — is found again on the SAME tick, as a candidate.
    ///
    /// This is what bounds the recovery of a moved or rescaled panel to one
    /// tick. Fails if the hinted path returns early on its own miss, which
    /// would hide the panel until the caller's periodic full read.
    #[test]
    fn a_hint_pointing_at_the_wrong_place_still_nominates_the_panel_it_moved_from() {
        for board in BOARDS {
            let img = board.load();
            let mut stale = board.hint(&img);
            stale.origin = (board.origin.0 - 200, board.origin.1 - 120);

            let found = detect_cheap(&img, Some(&stale));

            assert!(
                matches!(found, CheapDetect::Candidate { .. }),
                "{}: a stale origin must fall through to the nominating pass, got {found:?}",
                board.file
            );
        }
    }

    /// The point of the whole tick, as a ratio against the path it replaces:
    /// on a capture with no panel — the state the capture loop lives in — the
    /// cheap tick costs a bounded couple of correlations where
    /// [`super::super::reader::read_layout_with_hint`] runs its hint, the
    /// seeded band AND the full sweep.
    ///
    /// Measured on the real capture sizes (release, 2026-08-19): 2 correlations
    /// and 46 795 positions against 105 and 3 860 177 — see [`detect_cheap`].
    /// Asserted here on a small frame so the full path is affordable in a unit
    /// test, and as a ratio against that path rather than as a pinned number,
    /// so it cannot rot when the sweep's constants move.
    ///
    /// Fails if the cheap tick ever reaches [`table_band`] or [`full_sweep`].
    #[test]
    fn the_cheap_tick_costs_an_order_of_magnitude_less_than_the_read_it_gates() {
        let empty = DynamicImage::ImageRgb8(noise(480, 360));
        let hint = CheapHint {
            calibration: AnchorCalibration {
                screen_w: 480,
                screen_h: 360,
                scale: 1.0,
            },
            origin: (240, 180),
        };
        let calibration = hint.calibration;

        let (found, cheap) = search_tally(|| detect_cheap(&empty, Some(&hint)));
        assert!(matches!(found, CheapDetect::Nothing { .. }), "got {found:?}");
        assert_eq!(
            cheap.calls, 2,
            "the cheap tick is one windowed match plus one nominating pass",
        );

        let (_, full) = search_tally(|| {
            super::super::reader::read_layout_with_hint(&empty, Some(&calibration))
        });

        assert!(
            full.calls >= 10 * cheap.calls,
            "the read this gates ran {} correlations to the cheap tick's {}",
            full.calls,
            cheap.calls,
        );
        assert!(
            full.positions >= 10 * cheap.positions,
            "the read this gates scored {} positions to the cheap tick's {}",
            full.positions,
            cheap.positions,
        );
    }

    // ------------------------------------------------- the cold-start sweep --

    /// The bug POE-234 was opened on, as a fixture: a whole 1920x1080 game
    /// screen with the layout panel open, which nothing in this module could
    /// anchor without the exhaustive sweep.
    ///
    /// The board fixtures are panel CROPS, so they cannot exercise a path whose
    /// input is the capture's own size. This one is the frame the laptop dump
    /// carried, at its own resolution.
    fn full_screen_1080p() -> DynamicImage {
        let path = format!(
            "{}/tests/fixtures/temple/screen-live-1920x1080.png",
            env!("CARGO_MANIFEST_DIR")
        );
        image::open(&path).unwrap_or_else(|e| panic!("{path} loads: {e}"))
    }

    /// The measurement the whole batch rests on: this capture anchors at scale
    /// 1.000, Entrance centre (960, 713), NCC 0.99999 (laptop dump
    /// `temple-debug/1788438639673`, 2026-09-03).
    ///
    /// Asserted on [`Scene::pyramid_sweep`] directly, not through
    /// [`anchor_for_loop`]: 1920x1080 has a [`MEASURED_SCALES`] row, so the
    /// chain would answer from the table and this would stop being a test of
    /// the sweep at all. A sweep that nominated the wrong neighbourhood, or
    /// that refined it at the coarse grid instead of [`SCALE_STEP`], lands
    /// outside the tolerances below.
    #[test]
    fn the_cold_start_sweep_anchors_the_1080p_capture_at_its_measured_scale() {
        let img = full_screen_1080p();

        let found = Scene::of(&img)
            .pyramid_sweep(&|| false)
            .expect("the sweep anchors the capture");

        assert!(
            (found.scale - 1.00).abs() <= SCALE_STEP,
            "recovered scale {}, measured 1.000",
            found.scale
        );
        assert!(
            (found.origin.0 - 960).abs() <= 2 && (found.origin.1 - 713).abs() <= 2,
            "recovered origin {:?}, measured (960, 713)",
            found.origin
        );
        assert!(
            found.ncc >= 0.99,
            "NCC {}; the measured anchor is 0.99999 and anything near \
             {NCC_FLOOR} means a different plate was found",
            found.ncc
        );
    }

    /// A cancelled sweep answers "nothing", not "the best of what I got to".
    ///
    /// A partial nominating pass ranks a different set of scales, so returning
    /// its winner would let a module being switched off persist a scale the
    /// full ranking would have beaten — and that scale is then the hint every
    /// later session starts from.
    #[test]
    fn a_sweep_that_is_stopped_part_way_finds_nothing_rather_than_its_best_so_far() {
        // Stopped AFTER some correlations, not before any: a sweep that never
        // started has no partial ranking to be tempted by, so it cannot show
        // that the partial one is discarded. Three is enough to be inside the
        // nominating pass and far short of the ~23 it would run.
        const BEFORE_STOP: usize = 3;
        let img = full_screen_1080p();
        let seen = std::cell::Cell::new(0usize);
        let stop = || {
            let n = seen.get();
            seen.set(n + 1);
            n >= BEFORE_STOP
        };

        let (stopped, tally) = search_tally(|| Scene::of(&img).pyramid_sweep(&stop));

        assert!(
            stopped.is_none(),
            "a sweep stopped after {BEFORE_STOP} correlations answered {stopped:?}; \
             a partial nominating pass ranks a different set of scales, so its \
             winner is a different answer rather than a worse one"
        );
        assert_eq!(
            tally.calls, BEFORE_STOP,
            "the sweep ran {} correlations before honouring a stop that fires on \
             the {BEFORE_STOP}th poll",
            tally.calls
        );
    }

    /// The sweep is the fast path, and "fast" here is a claim about WHICH
    /// correlations its nominating pass runs: one unwindowed ÷4 pass per
    /// distinct coarse template size, [`SWEEP_NOMINATE_STEP`] apart instead of
    /// [`SCALE_STEP`] apart, over the same range.
    ///
    /// Measured on the real capture 2026-09-03 (Linux container, release):
    /// 5.3 s and 2 678 912 scored positions against [`full_sweep`]'s 28.4 s and
    /// 9 731 220, for the identical answer. Asserted here on a synthetic frame,
    /// as a RATIO — the same shape, and for the same reason, as
    /// `the_cheap_tick_costs_an_order_of_magnitude_less_than_the_read_it_gates`:
    /// the property is a property of the grid, not of the fixture, and running
    /// the dense pass over a 1920x1080 frame costs the debug build four
    /// minutes.
    ///
    /// Fails if [`SWEEP_NOMINATE_STEP`] collapses back to [`SCALE_STEP`], which
    /// is the one way the sweep stops being the fast path at all.
    #[test]
    fn the_cold_start_sweep_nominates_a_fraction_of_the_dense_grids_correlations() {
        // Big enough that the coarse level still holds the template at this
        // capture's ceiling (78x34 at ÷4), so both grids nominate over the whole
        // range and neither is silently truncated.
        let img = DynamicImage::ImageRgb8(noise(640, 400));
        let scene = Scene::of(&img);
        let (lo, hi) = sweep_range(img.height());
        let sparse = stepped(lo, hi, SWEEP_NOMINATE_STEP);
        let dense = grid(lo, hi);

        let (_, fast) = search_tally(|| scene.nominate(&sparse, &|| false));
        let (_, slow) = search_tally(|| scene.nominate(&dense, &|| false));

        assert!(
            fast.calls * 2 <= slow.calls,
            "the sweep nominated in {} correlations against the dense grid's {}",
            fast.calls,
            slow.calls
        );
        assert!(
            fast.positions * 2 <= slow.positions,
            "the sweep scored {} positions against the dense grid's {}",
            fast.positions,
            slow.positions
        );
    }

    /// …and the other half of the saving is that it stops at [`sweep_range`]'s
    /// ceiling, where [`full_sweep`] keeps going.
    ///
    /// On the capture this batch is about — 1920x1080 — the sweep runs to 2.00
    /// and the exhaustive one to 2.79, the width seed doubled. Those extra
    /// scales are the most expensive it has, because a coarse correlation's
    /// cost grows with the template's area. Fails if [`sweep_range`] is widened
    /// to meet it, in which case the sweep inherits the cost this exists to
    /// avoid.
    #[test]
    fn the_cold_start_sweeps_grid_is_a_fraction_of_the_exhaustive_sweeps() {
        let (lo, hi) = sweep_range(1080);
        let sparse = stepped(lo, hi, SWEEP_NOMINATE_STEP);
        let exhaustive = full_sweep(1920);

        assert!(
            sparse.len() * 4 <= exhaustive.len(),
            "the sweep tries {} scales against the exhaustive sweep's {}",
            sparse.len(),
            exhaustive.len()
        );
        assert!(
            *exhaustive.last().unwrap() > hi,
            "the exhaustive sweep ends at {}, which no longer reaches past the \
             sweep's {hi} ceiling — the two have converged and one of them is \
             redundant",
            exhaustive.last().unwrap(),
        );
    }

    /// The loop's chain does not run the exhaustive sweep, asserted as the cost
    /// it does not pay.
    ///
    /// On a capture with no plate on it — the state the capture loop lives in,
    /// and the one where both chains run every attempt they have —
    /// [`anchor_for_loop`] correlates a fraction of what [`anchor_with_hint`]
    /// does, because the difference between them is one whole [`SCALE_STEP`]
    /// nominating pass over [`full_sweep`]. Measured on the real capture
    /// (Linux container, release): 5.3 s against 28.4 s.
    ///
    /// A ratio and not a pinned count, on a frame small enough for the debug
    /// build, for the same reason
    /// `the_cheap_tick_costs_an_order_of_magnitude_less_than_the_read_it_gates`
    /// is written that way. The frame is deliberately one where [`full_sweep`]'s
    /// width-derived ceiling has already caught up with [`sweep_range`]'s, so
    /// what this measures is the nominating STEP alone: 237 933 positions
    /// against 507 905, a share of 0.47. On the real 1920x1080 capture the
    /// range cut applies as well and the share is 0.28.
    ///
    /// The bound is loose (0.70) on purpose. The regression it exists to catch
    /// puts the share at or above 1.00 — [`full_sweep`] running behind the
    /// loop's chain — and a tight bound here would instead fail on
    /// [`SWEEP_TOP_K`], whose windowed matches move this number by a few per
    /// cent and are nobody's regression.
    #[test]
    fn the_loops_chain_does_not_pay_for_the_exhaustive_sweep() {
        // Wide enough that `full_sweep`'s width-derived ceiling (2.04 here)
        // reaches `sweep_range`'s, so what is left between the two chains is
        // the nominating STEP rather than the range; short enough that the
        // exhaustive pass is affordable in a unit test.
        let empty = DynamicImage::ImageRgb8(noise(1400, 200));

        let (bounded, cheap) = search_tally(|| anchor_for_loop(&empty, None, true, &|| false));
        let (exhaustive, dear) = search_tally(|| anchor_with_hint(&empty, None));

        assert!(
            bounded.is_err() && exhaustive.is_err(),
            "a frame with no plate must anchor on neither chain: {bounded:?} / {exhaustive:?}"
        );
        let share = cheap.positions as f64 / dear.positions as f64;
        assert!(
            share <= 0.70,
            "the loop's chain scored {} positions to the exhaustive chain's {} \
             ({share:.2} of it); measured 0.47, and anything at or over 1.00 \
             means the exhaustive sweep is being run behind it after all",
            cheap.positions,
            dear.positions
        );
    }

    /// The sweep's ceiling reaches the scale each capture height implies, which
    /// is the whole reason it is not a constant.
    ///
    /// The temple scale tracks the capture HEIGHT ([`SWEEP_REFERENCE_HEIGHT`]:
    /// 1080 px anchors at 1.000), and the ceiling is SOFT — a scale past it is
    /// answered approximately rather than refused, and `super::run` persists
    /// that approximate answer and shuts its sweep gate on it. So a capture
    /// whose own height implies a scale outside the grid is the one case that
    /// must not exist.
    ///
    /// Fails if the ceiling goes back to a fixed 2.00, which a 2880 px screen
    /// is already past.
    #[test]
    fn the_sweep_ceiling_reaches_the_scale_each_capture_height_implies() {
        for height in [1080u32, 1200, 1440, 2160, 2880] {
            let implied = height as f32 / SWEEP_REFERENCE_HEIGHT;
            let (lo, hi) = sweep_range(height);
            // The GRID the nominating pass walks, not the range's endpoint: it
            // steps from `lo`, so `hi` is a bound rather than a member and the
            // last nominee can sit up to one step under it. `+ step` is the
            // neighbourhood that nominee's refinement covers, which is where
            // the answer would actually be found.
            let last = *stepped(lo, hi, SWEEP_NOMINATE_STEP).last().unwrap();

            assert!(
                last + SWEEP_NOMINATE_STEP >= implied,
                "a {height} px capture implies scale {implied}; the sweep's grid \
                 over {lo}..{hi} ends at {last} and refines to \
                 {}, which does not reach it",
                last + SWEEP_NOMINATE_STEP
            );
        }
    }

    /// …and it never narrows below what a 2160 px screen needs, however short
    /// the capture is. A windowed client is not a small screen, and a ceiling
    /// that tracked its height down would stop reaching the scale the game's
    /// own UI is drawn at.
    #[test]
    fn the_sweep_ceiling_does_not_narrow_on_a_short_capture() {
        let (_, hi) = sweep_range(600);

        assert!(hi >= 2.00, "a 600 px capture narrowed the ceiling to {hi}");
    }

    /// A refused sweep budget stops the chain at the table, and the loop's
    /// promoted read is what that protects.
    ///
    /// The case: a screen whose background nominates above
    /// [`COARSE_CANDIDATE_FLOOR`] promotes on EVERY tick, and a promoted read
    /// reaches this chain with no calibration and no table row. With the sweep
    /// ungated that is 5.3 s of correlation per tick, for ever. With
    /// `may_sweep = false` it is the two cheap attempts and a miss.
    ///
    /// Asserted as the cost, not as the outcome: both calls answer
    /// `AnchorNotFound` on a frame with no plate, so an assertion on the RESULT
    /// alone would pass with the flag ignored entirely.
    #[test]
    fn a_refused_sweep_budget_stops_the_chain_before_the_sweep() {
        let empty = DynamicImage::ImageRgb8(noise(600, 400));

        let (refused, cheap) = search_tally(|| anchor_for_loop(&empty, None, false, &|| false));
        let (allowed, dear) = search_tally(|| anchor_for_loop(&empty, None, true, &|| false));

        assert!(
            refused.is_err() && allowed.is_err(),
            "a frame with no plate anchors either way: {refused:?} / {allowed:?}"
        );
        assert_eq!(
            cheap.calls, 0,
            "a refused budget still ran {} correlations; with no hint and no \
             table row for this size there is nothing left but the sweep",
            cheap.calls
        );
        assert!(
            dear.calls > 0,
            "the allowed budget ran no correlations either, so this test is not \
             comparing the two"
        );
    }

    /// The two SEARCHES agree on the fixture: [`Scene::pyramid_sweep`] and
    /// [`Scene::search`] over [`full_sweep`] return the same scale and origin.
    ///
    /// Asserted on the two searches and not on [`anchor_for_loop`] against
    /// [`anchor_with_hint`], which is what this test did until 2026-09-03 and
    /// which proved nothing: 1920x1080 has a [`MEASURED_SCALES`] row, so
    /// [`table_band`] answers in both chains and neither sweep runs at all.
    ///
    /// What it buys is the guard on the disagreement
    /// [`anchor_for_loop`]'s note describes. The loop persists whatever the
    /// pyramid finds; the Debug button reports whatever the exhaustive sweep
    /// finds; on `board-ref-1374.png` those differ (1.00 at 0.9936 against 0.99
    /// at 0.9603). On the one full SCREEN in the corpus they must not, or the
    /// loop is remembering a calibration the button contradicts.
    ///
    /// `#[ignore]` because the exhaustive half is ~30 s in release and four
    /// minutes in the debug build the repo's gate uses. Run it on the release
    /// lane: `cargo test --release -- --ignored
    /// the_two_searches_agree_on_the_1080p_capture`.
    #[test]
    #[ignore = "release-lane: ~30 s exhaustive sweep"]
    fn the_two_searches_agree_on_the_1080p_capture() {
        let img = full_screen_1080p();
        let scene = Scene::of(&img);

        let fast = scene.pyramid_sweep(&|| false).expect("the sweep anchors");
        let exhaustive = scene
            .search(&full_sweep(img.width()))
            .expect("and so does the exhaustive search");

        assert_eq!(
            (fast.scale, fast.origin),
            (exhaustive.scale, exhaustive.origin),
            "the loop would persist scale {} at {:?} while the Debug button \
             reports {} at {:?}",
            fast.scale,
            fast.origin,
            exhaustive.scale,
            exhaustive.origin
        );
    }

    /// A calibration is only applicable at the capture size it was taken at.
    #[test]
    fn calibration_is_scoped_to_its_capture_size() {
        let img = DynamicImage::new_rgb8(1539, 613);
        let cal = AnchorCalibration {
            screen_w: 1539,
            screen_h: 613,
            scale: 1.13,
        };
        assert!(cal.applies_to(&img));
        assert!(!AnchorCalibration {
            screen_w: 1494,
            ..cal
        }
        .applies_to(&img));
        assert!(!AnchorCalibration {
            screen_h: 940,
            ..cal
        }
        .applies_to(&img));
    }
}

/// The embedded Entrance template, for tests that need to synthesise a board.
#[cfg(test)]
pub fn template_for_test() -> &'static RgbImage {
    template()
}

/// The nominating pass's ranking for `scales`, best coarse score first, as
/// `(coarse score, scale)`. Exposed so the regression test can watch the fine
/// pass disagree with it.
#[cfg(test)]
pub fn coarse_ranking_for_test(img: &DynamicImage, scales: &[f32]) -> Vec<(f32, f32)> {
    Scene::of(img)
        .nominate(scales, &|| false)
        .expect("an uncancellable nomination always answers")
        .into_iter()
        .map(|n| (n.score, n.scale))
        .collect()
}

/// The fine pass's search box for a nominee at `origin`, as production builds
/// it for [`Scene::search`].
#[cfg(test)]
pub fn fine_window_for_test(origin: (i32, i32)) -> (i32, i32, i32, i32) {
    box_around(origin, FINE_RADIUS)
}

/// Run `f` and report every correlation it performed.
#[cfg(test)]
pub fn search_tally<T>(f: impl FnOnce() -> T) -> (T, SearchTally) {
    TALLY.with(|c| c.set(SearchTally::default()));
    let out = f();
    (out, TALLY.with(|c| c.get()))
}

/// Run `f`, and report the largest windowed correlation it performed, in scored
/// positions. Zero means the fine pass never passed a window at all.
#[cfg(test)]
pub fn windowed_search_high_water<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let (out, tally) = search_tally(f);
    (out, tally.windowed_high_water)
}

/// [`FINE_RADIUS`], for the test that pins the fine pass's search bound.
#[cfg(test)]
pub const FINE_RADIUS_FOR_TEST: i32 = FINE_RADIUS;

/// [`SWEEP_FINE_RADIUS`], for the test that pins the sweep's search bound.
#[cfg(test)]
pub const SWEEP_FINE_RADIUS_FOR_TEST: i32 = SWEEP_FINE_RADIUS;
