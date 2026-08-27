//! Frame-anchored support-cell fit (POE-214 Part A).
//!
//! [`super::geometry::detect_reason`] derives the whole support grid from the
//! OCR line centres: the observed row pitch divided by
//! [`super::MercGeometry::row_pitch`] is a scale, and every cell rect is that
//! scale times a reference offset. Measured on the two committed fixtures that
//! path lands 6-12 px LEFT of the gold frame the icons are actually drawn in —
//! on `tests/fixtures/merc-skills-panel.png` the frame's dark lines sit at
//! x 312 / 361 / 409 / 458 / 507 / 556 while the OCR path predicts
//! 306 / 355 / 404 / 453 / 501 / 550. Every stored template and every corpus
//! crop carries that mis-registration back (POE-208's `SEED_ART_OFFSET_FRAC`
//! IS the offset, measured and pushed into the seed renderer).
//!
//! So this module stops deriving the grid and MEASURES it: the frame is a
//! 1-px dark line with a 2-px light line inside it and flat panel outside, and
//! that signature is sharp enough to locate on a global `(X0, pitch)` grid.
//! [`refine`] rewrites the layout onto the frame it found and reports what it
//! moved; a frame it cannot find leaves the layout exactly as the OCR built it.
//! A caller that has fitted BEFORE does better than that last part: it holds
//! the registration it settled on ([`FittedScale`]) and writes it back with
//! [`apply_held`], so one dark tick does not put the whole capture back on the
//! drift this module exists to remove.
//!
//! # Why here and not in `geometry`
//!
//! `geometry`'s contract is that [`super::geometry::detect_reason`] is PURE
//! over OCR line rects — 95 tests rest on being able to exercise the whole
//! detect without an image. This fit needs pixels, so it lives beside its
//! consumers (`run`, `debug`) rather than dragging `DynamicImage` into that
//! producer. It is not in `read` either: `read` runs layout → capture, and
//! this runs before a capture exists.

use std::cmp::Ordering;
use std::collections::HashMap;

use image::{DynamicImage, GenericImageView};

use super::geometry::{Frame, MercLayout};
use super::{MercGeometry, ScaleSource};

/// The reference fixture's MEASURED slot pitch, in px at `ui_scale` 1.0.
///
/// The gold frame's dark lines on `tests/fixtures/merc-skills-panel.png` sit at
/// x 312 / 361 / 409-410 / 458 / 507 / 556, which is a pitch of 48.67 — not the
/// 49.0 [`super::MercGeometry::cell_pitch`] carries (0.7 % high) and not the
/// 49.3 its `row_pitch` carries. This module owns the true unit so the fit does
/// not inherit that error; correcting the two `MercGeometry` constants is a
/// separate change with a wider blast radius (POE-214 D7 → POE-216).
pub const REF_PITCH: f32 = 48.67;

/// The reference fixture's MEASURED dark square side, in px at `ui_scale` 1.0.
///
/// The frame's dark line runs 43 px on each side of a cell on the reference
/// fixture (the outer cell rect the detect emits is 44 — see
/// [`super::MercGeometry::cell_size`] — because the rect includes the pixel the
/// line ends on). Paired with [`REF_PITCH`] this is the whole unit: a fit's
/// scale is `pitch / REF_PITCH`, and its dark square is
/// `round(REF_DARK_SIDE · pitch / REF_PITCH)` — 43 at 1.0, 39 at the PC's 0.90.
pub const REF_DARK_SIDE: i32 = 43;

/// The ring score a cell must reach to be counted as a located frame.
///
/// Set on the LOSSLESS reference fixture, where the twelve occupied cells score
/// 8.13-22.02 at truth (the two darkest arts 8.13 and 9.25) and every one of
/// the twenty-four empty cells scores exactly 0.00 — so 5.0 sits at 1.6× the
/// weakest occupied cell with no empty cell anywhere near it.
///
/// It is deliberately LOSSY on the JPEG-derived PC fixture, whose fifteen
/// occupied cells span 0.63-15.06: three fall under it and twelve are accepted,
/// the weakest at 5.23. Losing those three costs nothing — X0 and the pitch are
/// exact from the other twelve (measured) — but the converse does not hold:
/// **raising `T` may not be done without re-measuring the PC fixture**, which
/// would start dropping cells that the grid needs.
pub const T: f32 = 5.0;

/// How far either side of the OCR's predicted slot-0 x the search looks, in px.
///
/// The OCR path's own error is 6 px at the laptop and ~10 px at the PC, and a
/// systematic ±5 px error in the OCR line boxes on top of that is absorbed
/// rather than followed (measured: a ±5 px shift of the skill column returns
/// the SAME fit). 14 covers both with margin and still cannot reach the
/// neighbouring slot, which is 43-49 px away.
pub const X_SPAN: f32 = 14.0;

/// The step of the X0 grid, in px. Half a pixel: the cell rects are integers,
/// so a finer step buys nothing, and a whole-pixel step would quantise the
/// pitch's lever arm (X0 and pitch are fitted together).
pub const X_STEP: f32 = 0.5;

/// How far either side of the OCR's predicted pitch the search looks, as a
/// fraction.
///
/// 4 % covers the 1.8 % error in `row_pitch` (which is what the OCR scale is
/// divided by), the 0.7 % error in `cell_pitch`, and the rounding of an OCR
/// pitch measured from six line centres — the fit lands on the frame from
/// either bootstrap, measured on both fixtures.
pub const P_FRAC: f32 = 0.04;

/// The step of the pitch grid, in px. 0.15 px over five slots is 0.75 px at
/// slot 5 — under the rounding of the cell rect it produces.
pub const P_STEP: f32 = 0.15;

/// How far either side of the OCR's predicted cell top the search looks, in px.
///
/// The OCR centre anchor is already within 1 px on both fixtures (the measured
/// per-row residuals are `[0, -1, 0, -1, 0]` and `[0, 0, 0, 0, 0]`), so this is
/// not a search for the row — it is the tolerance that keeps a row whose OCR
/// centre is a couple of px out from dropping its cells. It is also the guard
/// that makes a systematic y error DECLINE rather than mis-fit: past ±3 the
/// cells stop clearing [`T`] and the lever-arm rule rejects the remains.
pub const Y_SPAN: i32 = 3;

/// The band the frame-measured scale must sit in, as a ratio of the OCR's.
///
/// The two scales are measurements of the same thing through different cues, so
/// a large disagreement means one of them is wrong and the OCR's — which the
/// whole rest of the layout rests on — is the one to keep. Measured: 1.027 at
/// the laptop, 1.034 at the PC.
///
/// The two paths reach it differently, and only one of them can trip it.
///
/// On the GRID path, with the shipped [`super::MercGeometry`] constants, it
/// cannot fire: the pitch grid is struck around `cell_pitch · s_ocr`, so the
/// ratio is confined to `49.0 · (1 ± P_FRAC) / REF_PITCH` = [0.9665, 1.0470]
/// by construction. The ONE-CELL fallback is reachable, because its scale is
/// `W / REF_DARK_SIDE` over `W ∈ round(REF_DARK_SIDE · s_ocr) ± 2` — the ±0.5
/// of that rounding rides on top of the ±2, so the ratio it can report spans
/// `[nominal − 2, nominal + 2] / (43 · s_ocr)`, which leaves the band as soon
/// as `s_ocr < 0.969`. At the PC fixture's 0.872 the span is [0.933, 1.040]
/// and the low end is outside: a fallback that measures the square two px
/// small IS refused, and that is the gate working rather than a miss. The
/// fallback is the degraded path (`cells_used == 1`), and the OCR scale a
/// refusal falls back to is the one the rest of the layout already rests on.
///
/// The other thing it defends is `merc-geometry.json`, which is per-field: a
/// hand-edited `rowPitch` moves `s_ocr` without moving `cell_pitch`, and this
/// is what stops the fit from rescaling a whole capture onto that mistake.
pub const SCALE_BAND: (f32, f32) = (0.94, 1.06);

/// What [`refine`] hands back: the layout to use, and what it did to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Refined {
    /// The layout, rewritten onto the frame when the fit landed and returned
    /// UNCHANGED when it did not.
    pub layout: MercLayout,
    /// The measurement, when the fit landed.
    pub fit: Option<CellFit>,
    /// Why it did not, when it did not. Exactly one of the two is `Some`.
    pub declined: Option<FitDecline>,
}

/// One frame-anchored measurement of the support grid.
#[derive(Debug, Clone, PartialEq)]
pub struct CellFit {
    /// Slot 0's left edge in SCREEN px — the frame's dark line, not the OCR's
    /// prediction of it.
    pub x0: f32,
    /// The measured slot pitch in screen px.
    pub pitch: f32,
    /// `pitch / REF_PITCH` — the UI scale in units of the reference fixture.
    pub scale: f32,
    /// The dark square's side, in px. The frame signature this fit matched.
    pub dark_side: i32,
    /// The outer cell size written into the rects — `dark_side + 1` on both
    /// fixtures, but derived rather than assumed (see [`fit_cell_size`]).
    pub cell_size: i32,
    /// How many cells cleared [`T`] at the winning grid point.
    pub cells_used: u8,
    /// `max_slot - min_slot` over those cells — the lever arm the pitch was
    /// measured on. `0` from the one-cell fallback.
    pub slot_span: u8,
    /// The largest `|x_fit - x_ocr|` over the slots — how far the rewrite moved
    /// the grid. THE number the smoke check reads.
    pub moved_px: i32,
    /// Per row that carried an accepted cell, the measured top line minus the
    /// OCR centre anchor's prediction of it. Evidence for POE-216: a row of
    /// zeros says the centre anchor is right.
    pub row_dy: Vec<i32>,
    /// Measured row pitch minus `g.row_pitch · scale` — POE-216's evidence that
    /// [`super::MercGeometry::row_pitch`] is 1.8 % high. `0.0` when fewer than
    /// two rows carried a cell, since there is then no gap to measure.
    pub residual_row_pitch: f32,
}

/// Why a frame fit did not land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FitDecline {
    /// NO cell's frame cleared [`T`] — an empty panel, a fully occluded one, a
    /// capture whose art is too dark for the ring to separate, or a band that
    /// fell off the image. The count is not carried because it is zero at
    /// every site that can reach this: a panel where cells WERE found but
    /// could not be measured declines as [`Self::OneCellUnmeasured`] or
    /// [`Self::NoLeverArm`] instead.
    TooFewCells,
    /// One cell's frame cleared [`T`] on the grid, but re-measuring its own
    /// square — which is how the fallback gets a scale — found nothing over
    /// [`T`] in the `round(REF_DARK_SIDE · s_ocr) ± 2` sizes at
    /// `x_pred + slot · p_pred ± X_SPAN`. The two windows are not the grid's:
    /// the grid may have located that cell at a pitch that puts it outside
    /// them, which is exactly the disagreement worth declining on.
    OneCellUnmeasured { slot: u8 },
    /// Cells were found, but they do not span two slots and there is more than
    /// one of them — so the pitch rests on adjacent (or coincident) evidence
    /// and would be free to be wrong by px at slot 5. One cell alone takes the
    /// fallback instead; two that disagree take nothing.
    NoLeverArm { span: u8 },
    /// The frame-measured scale disagrees with the OCR's by more than
    /// [`SCALE_BAND`]. One of the two measurements is wrong and the OCR's is
    /// the one the rest of the layout already rests on.
    OutOfBand { ratio: f32 },
}

impl std::fmt::Display for FitDecline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewCells => write!(
                f,
                "no cell frame cleared the ring score; the panel reads as empty"
            ),
            Self::OneCellUnmeasured { slot } => write!(
                f,
                "slot {slot}'s frame cleared the ring score but its own square did not re-measure"
            ),
            Self::NoLeverArm { span } => write!(
                f,
                "the located cells span {span} slot(s) — two apart is the least that pins a pitch"
            ),
            Self::OutOfBand { ratio } => write!(
                f,
                "the frame scale is {:.3}× the OCR's, outside [{:.2}, {:.2}]",
                ratio, SCALE_BAND.0, SCALE_BAND.1
            ),
        }
    }
}

/// Measure the support grid's gold frame and rewrite `layout` onto it.
///
/// `frame` is the detect frame the OCR ran on, and every pixel read goes
/// through [`Frame::local`] exactly as `read::build_capture` does — so the
/// function is identical on a full grab and on the loop's cropped re-detect,
/// and every rect that comes back out is screen-absolute.
///
/// A decline is not an error: the layout comes back untouched with the reason
/// beside it, and the loop carries on at the OCR scale it already had.
pub fn refine(
    img: &DynamicImage,
    frame: Frame,
    mut layout: MercLayout,
    g: &MercGeometry,
) -> Refined {
    let s_ocr = layout.scale;
    if layout.rows.is_empty() || !s_ocr.is_finite() || s_ocr <= 0.0 {
        return declined(layout, FitDecline::TooFewCells);
    }

    let centres: Vec<f32> = layout.rows.iter().map(|r| r.centre_y).collect();
    let slots = g.max_slots.max(1);
    let x_pred = layout.column_x0 as f32 + g.cell_offset_x * s_ocr;
    let p_pred = g.cell_pitch * s_ocr;
    let p_lo = p_pred * (1.0 - P_FRAC);
    let p_hi = p_pred * (1.0 + P_FRAC);
    let w_max = dark_side_for(p_hi);
    if w_max < 8 {
        return declined(layout, FitDecline::TooFewCells);
    }

    let Some(band) = LumaBand::cut(img, frame, &centres, x_pred, p_hi, slots, w_max) else {
        return declined(layout, FitDecline::TooFewCells);
    };

    // The whole search in one memo: 90 % of the `(x, y, W)` keys repeat across
    // the X0 grid, because two X0 half a pixel apart round to the same integer
    // cell x at most slots. 373k nominal ring evaluations collapse to 38k
    // distinct ones, and the whole call — integral image included — measures
    // 9 ms on the reference fixture in a release build (115 ms in a debug one).
    // The detect tick's own budget is 50 ms and it already spent an OCR.
    let mut memo: HashMap<(i32, i32, i32), f32> = HashMap::new();

    let n_p = ((p_hi - p_lo) / P_STEP) as usize + 1;
    let n_x = (2.0 * X_SPAN / X_STEP) as usize + 1;
    let mut best: Option<GridPoint> = None;
    for pi in 0..n_p {
        let pitch = p_lo + pi as f32 * P_STEP;
        let w = dark_side_for(pitch);
        if w < 8 {
            continue;
        }
        for xi in 0..n_x {
            let x0 = x_pred - X_SPAN + xi as f32 * X_STEP;
            let point = score_grid(&band, frame, &centres, slots, x0, pitch, w, &mut memo);
            if point.cells_used > 0 && best.as_ref().is_none_or(|held| point.beats(held, s_ocr)) {
                best = Some(point);
            }
        }
    }

    let Some(best) = best else {
        return declined(layout, FitDecline::TooFewCells);
    };
    let accepted = accepted_cells(
        &band,
        frame,
        &centres,
        slots,
        best.x0,
        best.pitch,
        best.dark_side,
        &mut memo,
    );

    // Two adjacent slots pin the pitch only to ±0.5 px, which is ±2.5 px by
    // slot 5 — worse than the error the fit exists to remove. One cell alone
    // carries no pitch claim at all, so it takes the fallback (which measures
    // the scale off that cell's OWN square) instead of pretending to one.
    let measured = if best.slot_span >= 2 {
        Measured {
            x0: best.x0,
            pitch: best.pitch,
            scale: best.pitch / REF_PITCH,
            dark_side: best.dark_side,
            cells: accepted,
            slot_span: best.slot_span,
        }
    } else if accepted.len() == 1 {
        let cell = accepted[0];
        let slot_pred = x_pred + cell.slot as f32 * p_pred;
        match one_cell_fallback(&band, frame, &centres, slot_pred, s_ocr, &cell, &mut memo) {
            Some(m) => m,
            None => {
                return declined(layout, FitDecline::OneCellUnmeasured { slot: cell.slot });
            }
        }
    } else {
        return declined(layout, FitDecline::NoLeverArm { span: best.slot_span });
    };

    let ratio = measured.scale / s_ocr;
    if ratio < SCALE_BAND.0 || ratio > SCALE_BAND.1 {
        return declined(layout, FitDecline::OutOfBand { ratio });
    }

    let fit = rewrite(&mut layout, &measured, g);
    Refined { layout, fit: Some(fit), declined: None }
}

/// The dark square's side at a pitch — the frame signature's one free
/// parameter, tied to the pitch by the reference unit rather than searched.
fn dark_side_for(pitch: f32) -> i32 {
    (REF_DARK_SIDE as f32 * pitch / REF_PITCH).round() as i32
}

/// The outer cell size a measured scale produces.
///
/// `round(g.cell_size · scale)` — the SAME expression
/// [`super::geometry::detect_reason`] and [`super::seed::cell_px`] use, so a
/// seed is rendered into the cell the fit will read. It equals `dark_side + 1`
/// on both committed fixtures, but that is a coincidence of those two scales,
/// not an invariant: `round(44·s) != round(43·s) + 1` on 9.4 % of
/// `s ∈ [0.76, 1.10]`, the nearest such band starting 0.0008 below the PC's own
/// 0.8985. Asserting the identity here would panic the detect thread on a live
/// capture inside one of those bands (POE-214 A1).
fn fit_cell_size(scale: f32, g: &MercGeometry) -> i32 {
    (g.cell_size * scale).round().max(1.0) as i32
}

fn declined(layout: MercLayout, why: FitDecline) -> Refined {
    Refined { layout, fit: None, declined: Some(why) }
}

/// One accepted cell at the winning grid point.
#[derive(Debug, Clone, Copy, PartialEq)]
struct AcceptedCell {
    row: usize,
    slot: u8,
    /// Top edge in SCREEN px — the row residual [`rewrite`] reports is measured
    /// off this. There is no `x` beside it on purpose: the fitted x of every
    /// slot is `X0 + slot · pitch`, and a per-cell copy of it would be a second
    /// source for the one number the grid exists to produce.
    top: i32,
    score: f32,
}

/// A grid point's summary — everything the tie-break needs, without the cells.
///
/// The accepted cells are recomputed for the winner alone: every evaluation is
/// memoised, so the second pass is free, and carrying a `Vec` through 1 482
/// grid points is not.
#[derive(Debug, Clone, Copy)]
struct GridPoint {
    total: f32,
    cells_used: u8,
    slot_span: u8,
    x0: f32,
    pitch: f32,
    dark_side: i32,
}

impl GridPoint {
    /// The tie-break, in order: total score, then the longer lever arm, then
    /// the scale closer to the OCR's, then the lower X0.
    ///
    /// Ties are real at the SHIPPED [`T`] of 5: the PC fixture's two best grid
    /// points both total 130.943 on twelve cells at X0 955.586 and side 39,
    /// differing only in pitch — 43.729 and 43.879, one `P_STEP` apart. The
    /// scale rule is what separates them (43.729 is 0.8985, the closer of the
    /// two to the OCR's 0.872), and without it the fit would depend on how the
    /// grid happens to be walked.
    fn beats(&self, other: &Self, s_ocr: f32) -> bool {
        match self.total.total_cmp(&other.total) {
            Ordering::Greater => return true,
            Ordering::Less => return false,
            Ordering::Equal => {}
        }
        match self.slot_span.cmp(&other.slot_span) {
            Ordering::Greater => return true,
            Ordering::Less => return false,
            Ordering::Equal => {}
        }
        let mine = (self.pitch / REF_PITCH - s_ocr).abs();
        let theirs = (other.pitch / REF_PITCH - s_ocr).abs();
        match mine.total_cmp(&theirs) {
            Ordering::Less => return true,
            Ordering::Greater => return false,
            Ordering::Equal => {}
        }
        self.x0 < other.x0
    }
}

/// A measured grid, from either path, before it is written into the layout.
struct Measured {
    x0: f32,
    pitch: f32,
    scale: f32,
    dark_side: i32,
    cells: Vec<AcceptedCell>,
    slot_span: u8,
}

/// Sum the ring score over every cell of one `(X0, pitch)` grid point.
fn score_grid(
    band: &LumaBand,
    frame: Frame,
    centres: &[f32],
    slots: u8,
    x0: f32,
    pitch: f32,
    w: i32,
    memo: &mut HashMap<(i32, i32, i32), f32>,
) -> GridPoint {
    let mut total = 0.0;
    let mut used = 0u8;
    let mut min_slot = u8::MAX;
    let mut max_slot = 0u8;
    for &centre in centres {
        let top = (centre - w as f32 / 2.0).round() as i32;
        for slot in 0..slots {
            let x = (x0 + slot as f32 * pitch).round() as i32;
            let (score, _) = best_over_y(band, frame, x, top, w, memo);
            if score >= T {
                total += score;
                used = used.saturating_add(1);
                min_slot = min_slot.min(slot);
                max_slot = max_slot.max(slot);
            }
        }
    }
    GridPoint {
        total,
        cells_used: used,
        slot_span: if used == 0 { 0 } else { max_slot - min_slot },
        x0,
        pitch,
        dark_side: w,
    }
}

/// The winner's accepted cells, in row-then-slot order.
fn accepted_cells(
    band: &LumaBand,
    frame: Frame,
    centres: &[f32],
    slots: u8,
    x0: f32,
    pitch: f32,
    w: i32,
    memo: &mut HashMap<(i32, i32, i32), f32>,
) -> Vec<AcceptedCell> {
    let mut out = Vec::new();
    for (row, &centre) in centres.iter().enumerate() {
        let predicted = (centre - w as f32 / 2.0).round() as i32;
        for slot in 0..slots {
            let x = (x0 + slot as f32 * pitch).round() as i32;
            let (score, top) = best_over_y(band, frame, x, predicted, w, memo);
            if score >= T {
                out.push(AcceptedCell { row, slot, top, score });
            }
        }
    }
    out
}

/// The best ring score over the `±Y_SPAN` band around a predicted top, and the
/// top it was found at.
fn best_over_y(
    band: &LumaBand,
    frame: Frame,
    x: i32,
    predicted_top: i32,
    w: i32,
    memo: &mut HashMap<(i32, i32, i32), f32>,
) -> (f32, i32) {
    let mut best = (0.0f32, predicted_top);
    for top in predicted_top - Y_SPAN..=predicted_top + Y_SPAN {
        let local = frame.local([x, top, w, w]);
        let key = (local[0], local[1], w);
        let score = match memo.get(&key) {
            Some(&s) => s,
            None => {
                let s = ring_score(band, local[0], local[1], w);
                memo.insert(key, s);
                s
            }
        };
        if score > best.0 {
            best = (score, top);
        }
    }
    best
}

/// The degraded path when exactly one cell's frame was located.
///
/// With one cell there is no pitch to measure, so the SCALE comes from the
/// cell's own dark square: the square's side is searched over
/// `round(REF_DARK_SIDE · s_ocr) ± 2` and the best-scoring side wins. Two
/// measured limits, both accepted rather than hidden:
///
/// 1. the scale is quantised to `1/43` ≈ 2.3 %, so X0 reconstructed back to
///    slot 0 inherits up to 2.8 px of error at slot 5;
/// 2. a single ambiguous cell can score higher one px off — measured at the PC
///    fixture, where row 3 slot 0 scores 16.90 at side 38 against a grid fit's
///    39, which reports `cell_px` 39 where the grid path reports 40.
///
/// So a fit from here is PROVISIONAL, and `run`'s hysteresis never lets one
/// replace a grid fit's session scale.
fn one_cell_fallback(
    band: &LumaBand,
    frame: Frame,
    centres: &[f32],
    slot_pred: f32,
    s_ocr: f32,
    cell: &AcceptedCell,
    memo: &mut HashMap<(i32, i32, i32), f32>,
) -> Option<Measured> {
    let centre = *centres.get(cell.row)?;
    let nominal = (REF_DARK_SIDE as f32 * s_ocr).round() as i32;
    let mut best: Option<(f32, i32, i32, i32)> = None;
    for w in (nominal - 2).max(8)..=nominal + 2 {
        let predicted_top = (centre - w as f32 / 2.0).round() as i32;
        let from = (slot_pred - X_SPAN).round() as i32;
        let to = (slot_pred + X_SPAN).round() as i32;
        for x in from..=to {
            for top in predicted_top - Y_SPAN..=predicted_top + Y_SPAN {
                let local = frame.local([x, top, w, w]);
                let key = (local[0], local[1], w);
                let score = match memo.get(&key) {
                    Some(&s) => s,
                    None => {
                        let s = ring_score(band, local[0], local[1], w);
                        memo.insert(key, s);
                        s
                    }
                };
                if best.is_none_or(|(held, ..)| score > held) {
                    best = Some((score, x, top, w));
                }
            }
        }
    }
    let (score, x, top, w) = best?;
    if score < T {
        return None;
    }
    let scale = w as f32 / REF_DARK_SIDE as f32;
    Some(Measured {
        x0: x as f32 - cell.slot as f32 * REF_PITCH * scale,
        pitch: REF_PITCH * scale,
        scale,
        dark_side: w,
        cells: vec![AcceptedCell { row: cell.row, slot: cell.slot, top, score }],
        slot_span: 0,
    })
}

/// Write the measured grid into the layout and report what changed.
///
/// `x = round(X0 + slot·pitch)` accumulated in float and rounded once, so slot
/// 5 does not carry five roundings. `y` stays the OCR CENTRE anchor
/// (`round(centre - size/2)`) — the fit measures each row's top line and
/// reports the residual instead of moving it, because the residual is already
/// within 1 px on both fixtures and the centre is the more robust cue.
/// `column_x0` and `row_pitch` are left alone: they are the OCR's own
/// measurements and other rules (the panel anchor, the column-moved test) are
/// calibrated against them.
///
/// The two anchors are therefore different in kind — x is on the FRAME, y on
/// the OCR centre — so the emitted rect can sit a px above the dark line it is
/// flush with on the left (`cell_size` is `dark_side + 1`, and a row whose
/// measured `row_dy` is −1 is a row whose top line the centre anchor puts one
/// px low). That is D4's sanctioned trade: the residual is reported rather
/// than corrected, because at ±1 px the centre is the more robust cue and
/// moving y would put the fit's own noise into every crop.
fn rewrite(layout: &mut MercLayout, m: &Measured, g: &MercGeometry) -> CellFit {
    let cell_size = fit_cell_size(m.scale, g);
    let mut moved_px = 0;
    for row in &mut layout.rows {
        let y = (row.centre_y - cell_size as f32 / 2.0).round() as i32;
        for (slot, rect) in row.cells.iter_mut().enumerate() {
            let x = (m.x0 + slot as f32 * m.pitch).round() as i32;
            moved_px = moved_px.max((x - rect[0]).abs());
            *rect = [x, y, cell_size, cell_size];
        }
    }
    layout.scale = m.scale;
    layout.scale_source = ScaleSource::Frame;

    // One residual per row that carried a cell, taken from that row's
    // best-scoring cell: the strongest evidence, and deterministic.
    let mut rows: Vec<(usize, i32)> = Vec::new();
    for cell in &m.cells {
        match rows.iter_mut().find(|(row, _)| *row == cell.row) {
            Some(_) => {}
            None => {
                let best = m
                    .cells
                    .iter()
                    .filter(|c| c.row == cell.row)
                    .max_by(|a, b| a.score.total_cmp(&b.score))
                    .expect("the row has at least this cell");
                rows.push((cell.row, best.top));
            }
        }
    }
    let row_dy: Vec<i32> = rows
        .iter()
        .map(|&(row, top)| {
            let centre = layout.rows[row].centre_y;
            top - (centre - m.dark_side as f32 / 2.0).round() as i32
        })
        .collect();
    let mut gaps: Vec<f32> = rows
        .windows(2)
        .map(|w| (w[1].1 - w[0].1) as f32 / (w[1].0 - w[0].0) as f32)
        .collect();
    let residual_row_pitch =
        if gaps.is_empty() { 0.0 } else { median(&mut gaps) - g.row_pitch * m.scale };

    CellFit {
        x0: m.x0,
        pitch: m.pitch,
        scale: m.scale,
        dark_side: m.dark_side,
        cell_size,
        cells_used: m.cells.len().min(u8::MAX as usize) as u8,
        slot_span: m.slot_span,
        moved_px,
        row_dy,
        residual_row_pitch,
    }
}

fn median(values: &mut [f32]) -> f32 {
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

// ---------------------------------------------------------------------------
// The registration a session settles on
// ---------------------------------------------------------------------------

/// Where a session has settled that the support grid IS — the registration
/// every rect downstream of a detect is cut at.
///
/// Not the raw per-tick fit: [`refine`] measures the frame afresh on every
/// detect, and the measurement wobbles. `run::next_fitted_scale` is the rule
/// that decides which wobbles the session adopts; this is what it holds, and
/// [`apply_held`] is what writes it back into a layout.
///
/// `x0_offset` is RELATIVE to the layout's own `column_x0` rather than an
/// absolute screen x, so a recruit window the player dragged re-registers for
/// free: the OCR measures the column on every tick, and the frame's offset from
/// that column does not move with the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FittedScale {
    /// The frame-measured UI scale, in [`REF_PITCH`] units.
    pub scale: f32,
    /// The outer cell side in px — `round(g.cell_size · scale)`, which is the
    /// number [`super::seed::cell_px`] memoises the seed store's window on and
    /// therefore the number a change of registration is judged by.
    pub cell_px: i32,
    /// Slot 0's fitted left edge minus the layout's `column_x0`.
    pub x0_offset: f32,
    /// The measured slot pitch in screen px.
    pub pitch: f32,
    /// Which of [`refine`]'s two paths measured it.
    pub source: FitSource,
    /// A [`Self::cell_px`] a fresh fit has proposed ONCE and the session has
    /// not adopted. The deadband: a second consecutive tick proposing the same
    /// size adopts it, and an oscillation across a rounding boundary never
    /// produces two in a row.
    pub pending: Option<i32>,
}

/// Which path of [`refine`] produced a registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitSource {
    /// The `(X0, pitch)` grid, on cells spanning two or more slots.
    Grid,
    /// The one-cell fallback, whose scale is quantised to `1/REF_DARK_SIDE`
    /// (≈ 2.3 %) and can land a px out on a single ambiguous cell — measured on
    /// the PC fixture. Good enough to register a capture that has nothing
    /// better; never good enough to overwrite a [`Self::Grid`] registration.
    OneCell,
}

impl FittedScale {
    /// The registration a fresh fit proposes, against the layout it was
    /// measured on (whose `column_x0` is what `x0_offset` is relative to).
    pub fn from_fit(fit: &CellFit, column_x0: i32) -> Self {
        Self {
            scale: fit.scale,
            cell_px: fit.cell_size,
            x0_offset: fit.x0 - column_x0 as f32,
            pitch: fit.pitch,
            source: if fit.cells_used <= 1 { FitSource::OneCell } else { FitSource::Grid },
            pending: None,
        }
    }
}

/// Write a settled registration into `layout`, in place of whatever the OCR or
/// this tick's own fit put there.
///
/// The same expressions [`rewrite`] uses, so re-applying a registration that
/// was just adopted is idempotent — which is what lets the caller run this on
/// EVERY tick and keep one path for "the fit landed and was adopted", "the fit
/// landed and the deadband refused it" and "the fit declined".
///
/// It is the reason a decline no longer drops the loop back to the OCR's
/// 6-12 px drift: the session knows where the frame is, and the offset it knows
/// it in survives the panel being dragged. `source` is the caller's, because
/// the same rects mean [`super::ScaleSource::Frame`] on a tick that measured
/// them and [`super::ScaleSource::Held`] on a tick that did not.
pub fn apply_held(layout: &mut MercLayout, held: &FittedScale, source: ScaleSource) {
    let x0 = layout.column_x0 as f32 + held.x0_offset;
    for row in &mut layout.rows {
        let y = (row.centre_y - held.cell_px as f32 / 2.0).round() as i32;
        for (slot, rect) in row.cells.iter_mut().enumerate() {
            let x = (x0 + slot as f32 * held.pitch).round() as i32;
            *rect = [x, y, held.cell_px, held.cell_px];
        }
    }
    layout.scale = held.scale;
    layout.scale_source = source;
}

// ---------------------------------------------------------------------------
// The ring score
// ---------------------------------------------------------------------------

/// How strongly a `w × w` square at `(x, y)` looks like a support cell's gold
/// frame, in image-LOCAL px. `0.0` means "not a frame".
///
/// The frame is three concentric bands, MEASURED on both fixtures: a 1-px dark
/// line (luma ≈ 10 on the reference, 12-14 on the PC's JPEG), a 2-px light line
/// immediately inside it (≈ 47 / 45-52), and flat panel immediately outside
/// (≈ 21 / 18, stddev < 5). So:
///
/// ```text
/// score = max(0, (light - dark) + (out - dark) - 2·std(out) - std(dark))
/// ```
///
/// The two contrast terms find the line; the two spread terms are what makes an
/// EMPTY slot score exactly zero rather than weakly — flat panel has `light ≈
/// dark ≈ out`, so the contrasts vanish while the spreads do not. The `out`
/// spread is also what keeps the score from drifting onto art: a square laid
/// over an icon's interior has a busy outside ring.
///
/// Measured on the reference fixture at truth: twelve occupied cells 8.13-22.02,
/// twenty-four empty cells 0.00, and every occupied cell displaced by ±2 px or
/// more also 0.00 (only +1 px leaks, at 9.55).
fn ring_score(band: &LumaBand, x: i32, y: i32, w: i32) -> f32 {
    let (dark, dark_sd, dn) = band.annulus(x, y, x + w, y + w, x + 1, y + 1, x + w - 1, y + w - 1);
    let (light, _, ln) =
        band.annulus(x + 1, y + 1, x + w - 1, y + w - 1, x + 3, y + 3, x + w - 3, y + w - 3);
    let (out, out_sd, on) = band.annulus(x - 1, y - 1, x + w + 1, y + w + 1, x, y, x + w, y + w);
    if dn == 0 || ln == 0 || on == 0 {
        return 0.0;
    }
    (((light - dark) + (out - dark) - 2.0 * out_sd - dark_sd) as f32).max(0.0)
}

/// Integral images of luma and luma² over the panel band.
///
/// The band, not the screen: the search touches a few hundred px either side of
/// the grid, and a 1920×1080 pair of `f64` planes is 33 MB to build on a 1 Hz
/// detect tick. Cutting it to the band makes the whole fit a sub-frame cost.
struct LumaBand {
    /// The band's top-left in image-LOCAL px.
    x0: i32,
    y0: i32,
    w: usize,
    h: usize,
    /// `(w + 1) × (h + 1)` row-major prefix sums.
    sum: Vec<f64>,
    sum_sq: Vec<f64>,
}

impl LumaBand {
    /// Cut the band the search will touch out of `img`, clamped to it.
    ///
    /// `None` when nothing of the band is on the image — a recruit window
    /// dragged fully off screen, which the fit declines rather than guesses at.
    fn cut(
        img: &DynamicImage,
        frame: Frame,
        centres: &[f32],
        x_pred: f32,
        p_hi: f32,
        slots: u8,
        w_max: i32,
    ) -> Option<Self> {
        let last = (slots.max(1) - 1) as f32;
        let top = centres.iter().copied().fold(f32::INFINITY, f32::min);
        let bottom = centres.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        // Two px of margin past the outside ring on every edge, so nothing the
        // score reads is clipped by the band that is not also clipped by the
        // image.
        let screen = [
            (x_pred - X_SPAN).floor() as i32 - 2,
            (top - w_max as f32 / 2.0).floor() as i32 - Y_SPAN - 2,
            0,
            0,
        ];
        let far_x = (x_pred + X_SPAN + last * p_hi).ceil() as i32 + w_max + 2;
        let far_y = (bottom + w_max as f32 / 2.0).ceil() as i32 + Y_SPAN + 2;
        let local = frame.local([screen[0], screen[1], 0, 0]);
        let far = frame.local([far_x, far_y, 0, 0]);

        let (iw, ih) = img.dimensions();
        let x0 = local[0].max(0);
        let y0 = local[1].max(0);
        let x1 = far[0].min(iw as i32);
        let y1 = far[1].min(ih as i32);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let w = (x1 - x0) as usize;
        let h = (y1 - y0) as usize;

        let mut sum = vec![0.0f64; (w + 1) * (h + 1)];
        let mut sum_sq = vec![0.0f64; (w + 1) * (h + 1)];
        for row in 0..h {
            let mut run = 0.0f64;
            let mut run_sq = 0.0f64;
            for col in 0..w {
                let p = img.get_pixel((x0 as u32) + col as u32, (y0 as u32) + row as u32);
                let v = luma_f64(p.0[0], p.0[1], p.0[2]);
                run += v;
                run_sq += v * v;
                sum[(row + 1) * (w + 1) + col + 1] = sum[row * (w + 1) + col + 1] + run;
                sum_sq[(row + 1) * (w + 1) + col + 1] = sum_sq[row * (w + 1) + col + 1] + run_sq;
            }
        }
        Some(Self { x0, y0, w, h, sum, sum_sq })
    }

    /// Sum, sum of squares and pixel count over the half-open box
    /// `[x0, x1) × [y0, y1)` in image-local px, clipped to the band.
    fn box_stats(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> (f64, f64, u32) {
        let bx0 = (x0 - self.x0).clamp(0, self.w as i32) as usize;
        let by0 = (y0 - self.y0).clamp(0, self.h as i32) as usize;
        let bx1 = (x1 - self.x0).clamp(0, self.w as i32) as usize;
        let by1 = (y1 - self.y0).clamp(0, self.h as i32) as usize;
        if bx1 <= bx0 || by1 <= by0 {
            return (0.0, 0.0, 0);
        }
        let stride = self.w + 1;
        let at = |t: &Vec<f64>, r: usize, c: usize| t[r * stride + c];
        let s = at(&self.sum, by1, bx1) - at(&self.sum, by0, bx1) - at(&self.sum, by1, bx0)
            + at(&self.sum, by0, bx0);
        let s2 = at(&self.sum_sq, by1, bx1) - at(&self.sum_sq, by0, bx1)
            - at(&self.sum_sq, by1, bx0)
            + at(&self.sum_sq, by0, bx0);
        (s, s2, ((bx1 - bx0) * (by1 - by0)) as u32)
    }

    /// Mean, standard deviation and pixel count over an annulus — an outer box
    /// minus an inner one.
    #[allow(clippy::too_many_arguments)]
    fn annulus(
        &self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        ix0: i32,
        iy0: i32,
        ix1: i32,
        iy1: i32,
    ) -> (f64, f64, u32) {
        let (so, so2, no) = self.box_stats(x0, y0, x1, y1);
        let (si, si2, ni) = self.box_stats(ix0, iy0, ix1, iy1);
        let n = no.saturating_sub(ni);
        if n == 0 {
            return (0.0, 0.0, 0);
        }
        let mean = (so - si) / n as f64;
        let var = ((so2 - si2) / n as f64 - mean * mean).max(0.0);
        (mean, var.sqrt(), n)
    }
}

/// ITU-R BT.601 luma in full precision.
///
/// The same weighting as [`super::geometry::luma`], which every other measured
/// number in this module tree was taken against, but NOT quantised to `u8`:
/// the integral image is `f64` regardless, and rounding each pixel down first
/// costs the PC fixture one accepted cell (11 instead of 12 at `T = 5`) for no
/// gain. Measured both ways — the fitted `(X0, pitch, dark_side)` is identical.
fn luma_f64(r: u8, g: u8, b: u8) -> f64 {
    0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64
}

/// The origin `tests/fixtures/merc-recruit-pc-1080p.png` was cropped at, and
/// the screen it came off: a 1920×1080 PC. JPEG-derived, so its per-cell ring
/// scores run about 0.6× the lossless reference's.
#[cfg(test)]
pub(super) const PC_ORIGIN: (i32, i32) = (700, 585);
/// See [`PC_ORIGIN`].
#[cfg(test)]
pub(super) const PC_SCREEN: [u32; 2] = [1920, 1080];

/// The PC panel's OCR lines, hand-authored in SCREEN px from the capture: six
/// rows at y-centres 616/659/703/746/790/833 in a skill column at x 743, and
/// the wager line above row 1. All six names are in `mercenary-stats.json`, so
/// [`super::geometry::detect`] seeds its column off them exactly as it would
/// off the real OCR's.
///
/// `pub(super)` for the same reason [`super::geometry::reference_lines`] is:
/// the fixture and these lines are ONE ground truth — the SECOND machine's,
/// at UI scale 0.90 — and `icons`' corpus readers need both halves of it to
/// cut a cell at a scale no committed crop was harvested at (POE-214 Part C).
#[cfg(test)]
pub(super) fn pc_lines() -> Vec<super::geometry::OcrLineBox> {
    use super::geometry::test_line;
    vec![
        test_line("Wager: 8 831", 700, 580),
        test_line("Withering Step", 743, 616),
        test_line("Chaotic Burst", 743, 659),
        test_line("Chaotic Shot", 743, 703),
        test_line("Caustic Arrow", 743, 746),
        test_line("Trarthan Agility", 743, 790),
        test_line("Grace", 743, 833),
    ]
}

// -- AC1/AC2 (Part C): the gold frame's dark|light step ---------------------
//
// `pub(super)` and at module level for the same reason [`pc_lines`] is: the
// step statistic below is ONE definition of what the frame looks like in a
// column mean, and `icons`' corpus reader runs it over the committed crops
// (POE-214 Part C) while the tests here run it over the two fixtures. A
// second copy would drift the moment a threshold moved.

/// The gold frame's dark line, as a ceiling on a column mean.
///
/// A1 measures the line itself at luma ~10 (reference) / 12-14 (PC JPEG).
/// This ceiling is read against two column means of a whole cell, both
/// measured over the 27 occupied cells of the two fixtures: the dark
/// column inside the OCR crop's left band (6.4-11.6) and the back-probe
/// two px left of the fitted crop (6.4-11.9). 15 clears the top of both
/// by three levels.
///
/// It does NOT separate the line from the panel on its own — the columns
/// where the frame fades into panel read 11-18 — which is why
/// [`frame_step_columns`] pairs it with [`LIGHT_LINE`] on the NEXT column
/// rather than using it alone.
#[cfg(test)]
pub(super) const DARK_LINE: f32 = 15.0;

/// The 2-px light line drawn immediately inside [`DARK_LINE`], as a floor
/// on a column mean. A1 measures it at ~47 / 45-52; averaged down a cell,
/// the fitted crop's first column — which IS that line — reads 40.5-41.2
/// (reference) / 37.6-41.5 (PC).
///
/// The value the same floor has to stay ABOVE is the OCR crop's first
/// column, 5.2-23.1: flat panel at the near slots, and at the far ones
/// the PREVIOUS cell's dark frame line, which the accumulated drift has
/// walked the crop's left edge onto (the reference's row 2 slot 2 reads
/// 6.1 and its row 3 slot 3 reads 5.2). So 30 sits in the 23.1-37.6 gap
/// those two leave, with seven levels of margin on either side.
#[cfg(test)]
pub(super) const LIGHT_LINE: f32 = 30.0;

/// How far in from an edge the frame is looked for: columns `0..EDGE_BAND`
/// are tested, so the light-line read reaches index `EDGE_BAND`.
///
/// The OCR path's drift grows with the slot — the reference's step sits at
/// column 4 on slot 0 and 7 on slot 3, the PC's at 3 on slot 0 and 7 on
/// slot 4. 7 is the largest column measured, so the right-hand headroom is
/// ZERO. A capture with a further occupied slot would push the step past
/// the band and fail loudly in
/// `the_fit_moves_the_cell_crop_off_the_gold_frame_it_was_cutting_through`
/// ("exactly one of its left 8 columns") rather than pass quietly;
/// widening the band is the fix at that point, and it still stops well
/// short of the art's own centre.
#[cfg(test)]
pub(super) const EDGE_BAND: usize = 8;

/// A crop's column means, left to right, in BT.601 luma — the same
/// weighting every other number in this module was measured with.
#[cfg(test)]
pub(super) fn column_means(crop: &image::RgbaImage) -> Vec<f32> {
    let (w, h) = crop.dimensions();
    (0..w)
        .map(|x| {
            (0..h)
                .map(|y| {
                    let p = crop.get_pixel(x, y).0;
                    crate::mercenary::geometry::luma(p[0], p[1], p[2]) as f32
                })
                .sum::<f32>()
                / h as f32
        })
        .collect()
}

/// The columns of the left [`EDGE_BAND`] where a dark line is immediately
/// followed by a light one — the gold frame's signature, read the way A1
/// describes it rather than by correlating a template.
#[cfg(test)]
pub(super) fn frame_step_columns(means: &[f32]) -> Vec<usize> {
    (0..EDGE_BAND)
        .filter(|&x| means[x] <= DARK_LINE && means[x + 1] >= LIGHT_LINE)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mercenary::geometry::{detect, reference_lines, OcrLineBox};
    use crate::mercenary::seed;
    use crate::mercenary::vocab::MercVocab;
    use image::Rgba;

    fn vocab() -> MercVocab {
        MercVocab::load().expect("vocabulary parses")
    }

    fn fixture(name: &str) -> DynamicImage {
        image::open(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("the committed fixture {name} loads: {e}"))
    }

    // -- the reference fixture ------------------------------------------------

    /// `tests/fixtures/merc-skills-panel.png` is the (60,585) crop of the
    /// laptop's 1920×1200 `recruit-cai.png`, and [`reference_lines`] is the OCR
    /// half of that same screen — so the two are one ground truth and the crop
    /// origin is what joins them.
    const REF_ORIGIN: (i32, i32) = (60, 585);
    const REF_SCREEN: [u32; 2] = [1920, 1200];

    /// The gold frame's dark lines, MEASURED in fixture px: 312 / 361 / 409-410
    /// / 458 / 507 / 556, i.e. a slot pitch of 48.67. The OCR path predicts
    /// 306 / 355 / 404 / 453 / 501 / 550 — 6 px out at slot 0, 11 at slot 5.
    const REF_FRAME_X0: f32 = 312.0;
    /// The dark square's side on the reference fixture, measured.
    const REF_SIDE: i32 = 43;

    /// Which slots of the reference panel hold art, per row (from the twelve
    /// cells `icons.rs` reads tiers off).
    const REF_OCCUPIED: [&[u8]; 6] = [&[0, 1], &[0, 1], &[0, 1, 2], &[0, 1, 2, 3], &[], &[0]];

    // -- the PC fixture -------------------------------------------------------

    /// The PC panel's frame columns, MEASURED in SCREEN px for slots 0-4 (slot
    /// 5 is empty on every row of this capture).
    const PC_FRAME_COLUMNS: [i32; 5] = [956, 999, 1043, 1087, 1131];
    /// The dark square's side at the PC's 0.90 UI scale, measured. `round(43 ·
    /// 43.8 / 48.67)`; at 38 the interior slots drop out.
    const PC_SIDE: i32 = 39;

    // -- helpers --------------------------------------------------------------

    /// The measured grid of a fixture, in FIXTURE px — what the fit is supposed
    /// to find, used here only to paint cells out.
    struct Truth {
        x0: f32,
        pitch: f32,
        side: i32,
        centres: [f32; 6],
        /// The panel's own flat background, measured on the fixture.
        flat: [u8; 3],
    }

    const REF_TRUTH: Truth = Truth {
        x0: 312.0,
        pitch: 48.6667,
        side: 43,
        centres: [35.0, 84.0, 132.0, 181.0, 229.0, 277.0],
        flat: [21, 21, 21],
    };

    const PC_TRUTH: Truth = Truth {
        x0: 255.5,
        pitch: 43.75,
        side: 39,
        centres: [31.0, 74.0, 118.0, 161.0, 205.0, 248.0],
        flat: [18, 18, 18],
    };

    /// Paint the fixture's grid over with flat panel, keeping only `keep`.
    ///
    /// The cell and its frame are 43 (39) px square; painting three px past
    /// every edge removes the frame AND its outside ring, so a painted cell
    /// scores exactly zero and the fit sees only the cells left behind. It is
    /// the occlusion case (a tooltip over most of the panel) and the
    /// nearly-empty-mercenary case in one.
    fn only_cells(img: &DynamicImage, truth: &Truth, keep: &[(usize, u8)]) -> DynamicImage {
        let mut out = img.to_rgba8();
        let (w, h) = (out.width() as i32, out.height() as i32);
        for (row, &centre) in truth.centres.iter().enumerate() {
            for slot in 0..6u8 {
                if keep.contains(&(row, slot)) {
                    continue;
                }
                let x = (truth.x0 + slot as f32 * truth.pitch).round() as i32;
                let y = (centre - truth.side as f32 / 2.0).round() as i32;
                for py in (y - 3).max(0)..(y + truth.side + 4).min(h) {
                    for px in (x - 3).max(0)..(x + truth.side + 4).min(w) {
                        out.put_pixel(
                            px as u32,
                            py as u32,
                            Rgba([truth.flat[0], truth.flat[1], truth.flat[2], 255]),
                        );
                    }
                }
            }
        }
        DynamicImage::ImageRgba8(out)
    }

    /// The ring score of one square, in fixture px.
    ///
    /// The band is cut around that square alone, which is arithmetically the
    /// same as the search's: the score reads a 1-px ring outside the square and
    /// the band reaches 16 px further than that on every side.
    fn score_at(img: &DynamicImage, x: i32, y: i32, w: i32) -> f32 {
        let (iw, ih) = img.dimensions();
        let frame = Frame::full([iw, ih]);
        let centre = y as f32 + w as f32 / 2.0;
        let band = LumaBand::cut(img, frame, &[centre], x as f32, 1.0, 1, w)
            .expect("the square lies on the fixture");
        ring_score(&band, x, y, w)
    }

    /// The reference fixture's cells at truth, as `(row, slot, x, y)` in
    /// fixture px.
    fn ref_cells_at_truth() -> Vec<(usize, u8, i32, i32)> {
        let mut out = Vec::new();
        for (row, &centre) in REF_TRUTH.centres.iter().enumerate() {
            for slot in 0..6u8 {
                let x = (REF_TRUTH.x0 + slot as f32 * REF_TRUTH.pitch).round() as i32;
                let y = (centre - REF_SIDE as f32 / 2.0).round() as i32;
                out.push((row, slot, x, y));
            }
        }
        out
    }

    /// The reference panel's OCR lines with every SKILL row moved `dy` px down.
    /// The skill column is the one at x 134; the header and wager lines sit
    /// elsewhere and stay where they are, so this is a shift of the rows the
    /// grid is derived from rather than of the whole screen.
    fn rows_shifted(dy: i32) -> Vec<OcrLineBox> {
        reference_lines()
            .into_iter()
            .map(|mut l| {
                if l.x == 134 {
                    l.y += dy;
                }
                l
            })
            .collect()
    }

    /// The best score over the `±Y_SPAN` band, which is what the search reads.
    fn best_score_at(img: &DynamicImage, x: i32, y: i32, w: i32) -> f32 {
        (y - Y_SPAN..=y + Y_SPAN).map(|ty| score_at(img, x, ty, w)).fold(0.0, f32::max)
    }

    fn ref_refined() -> Refined {
        let g = MercGeometry::default();
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        assert!(
            (layout.scale - 48.0 / g.row_pitch).abs() < 1e-6,
            "the OCR path must reach the fit at its own 48/49.3, not something else: {}",
            layout.scale,
        );
        refine(&fixture("merc-skills-panel.png"), Frame::cropped(REF_ORIGIN, REF_SCREEN), layout, &g)
    }

    fn pc_refined() -> Refined {
        let g = MercGeometry::default();
        let layout = detect(&pc_lines(), &g, &vocab(), None).expect("the PC panel");
        assert!(
            (layout.scale - 0.872).abs() <= 0.002,
            "the hand-authored PC lines must reproduce the capture's 0.872 OCR scale, not {}",
            layout.scale,
        );
        refine(
            &fixture("merc-recruit-pc-1080p.png"),
            Frame::cropped(PC_ORIGIN, PC_SCREEN),
            layout,
            &g,
        )
    }

    // -- (1) the reference fixture -------------------------------------------

    /// The whole fit on the ONE lossless ground truth: the OCR path puts slot 0
    /// at fixture x 306 and the gold frame is at 312, and the fit has to move
    /// the grid onto the frame.
    ///
    /// `pitch` is documentation rather than the invariant — the two top-scoring
    /// pitches (48.65 and 48.50) are 0.4 % apart in total score and produce the
    /// same x0, dark side and cell size, which are the numbers everything
    /// downstream reads.
    #[test]
    fn the_reference_panel_fit_lands_on_the_measured_frame() {
        let out = ref_refined();

        let fit = out.fit.expect("the reference panel's frame is found");
        assert_eq!(out.declined, None);
        assert!(
            (fit.x0 - (REF_ORIGIN.0 as f32 + REF_FRAME_X0)).abs() <= 0.5,
            "slot 0 must land on the frame at fixture x {REF_FRAME_X0}, not at {}",
            fit.x0 - REF_ORIGIN.0 as f32,
        );
        assert_eq!(fit.dark_side, REF_SIDE);
        assert_eq!(fit.cell_size, 44);
        assert_eq!(fit.cells_used, 12, "all twelve occupied cells clear the ring score");
        assert!(
            (fit.pitch - 48.67).abs() <= 0.30,
            "the measured pitch is 48.67, not {}",
            fit.pitch,
        );
        // 6 px at slot 0 and 11 at slot 5, both measured. Correcting
        // `MercGeometry`'s pitch constants (POE-216) would narrow this to 2-6.
        assert!(
            (6..=13).contains(&fit.moved_px),
            "the rewrite moved the grid {} px; the measured drift is 6-13",
            fit.moved_px,
        );
        assert_eq!(out.layout.scale_source, ScaleSource::Frame);
        assert_eq!(out.layout.scale, fit.scale);
        // POE-216's evidence, measured here rather than described in a doc: the
        // OCR centre anchor puts two of the five occupied rows' tops one px
        // high (D4 reports that residual instead of moving y), and the measured
        // row pitch runs 1.03 px SHORT of `row_pitch · scale` — which is the
        // 1.8 % error in the 49.3 constant, in px.
        assert_eq!(fit.row_dy, vec![0, -1, 0, -1, 0], "the five rows that carry a cell");
        assert!(
            (fit.residual_row_pitch + 1.03).abs() <= 0.1,
            "the measured row pitch is 1.03 px under g.row_pitch · scale, not {}",
            fit.residual_row_pitch,
        );
    }

    // -- (2) the PC fixture ---------------------------------------------------

    /// The same fit at a second UI scale, from a second machine, through a
    /// CROPPED frame — so every screen rect the fit emits is proof that
    /// `Frame::local` is the only place the two coordinate spaces meet.
    #[test]
    fn the_pc_panel_fit_lands_on_the_measured_frame() {
        let out = pc_refined();

        let fit = out.fit.expect("the PC panel's frame is found");
        assert_eq!(out.declined, None);
        assert!(
            (fit.x0 - 955.6).abs() <= 1.0,
            "slot 0 must land on the frame at screen x 955.6, not at {}",
            fit.x0,
        );
        assert!((fit.pitch - 43.8).abs() <= 0.2, "the measured pitch is 43.75, not {}", fit.pitch);
        assert_eq!(fit.dark_side, PC_SIDE);
        assert_eq!(fit.cell_size, 40);
        assert!(
            (fit.scale - 0.90).abs() <= 0.01,
            "1080/1200 of the reference scale is 0.90, not {}",
            fit.scale,
        );
        let row3: Vec<i32> = out.layout.rows[3].cells.iter().take(5).map(|c| c[0]).collect();
        assert_eq!(
            row3, PC_FRAME_COLUMNS,
            "the five cells of the panel's fullest row must sit on the measured frame columns",
        );
        // The other half of POE-216's evidence, on the second machine: here the
        // centre anchor is exact on every row, and the row pitch runs 0.79 px
        // short — the same 1.8 % error as the reference, scaled by 0.90.
        assert_eq!(fit.row_dy, vec![0, 0, 0, 0, 0], "the OCR centre anchor is exact at the PC");
        assert!(
            (fit.residual_row_pitch + 0.79).abs() <= 0.1,
            "the measured row pitch is 0.79 px under g.row_pitch · scale, not {}",
            fit.residual_row_pitch,
        );
    }

    /// The fixture's own precondition, DERIVED from its pixels instead of
    /// computed from the constants it is supposed to check: sweep the ring
    /// score across the whole grid region of the panel's fullest row, at every
    /// column, and the peaks that come back must BE the frame columns the fit
    /// is pinned against.
    ///
    /// The sweep assumes no column and no pitch — only a row band, the same
    /// `±Y_SPAN` the search itself allows. So this fails if the fixture is ever
    /// replaced by a capture whose panel sits somewhere else, which is exactly
    /// what a precondition is for.
    #[test]
    fn the_pc_fixtures_frame_columns_are_where_the_pixels_say() {
        let img = fixture("merc-recruit-pc-1080p.png");
        let row_top = (PC_TRUTH.centres[3] - PC_SIDE as f32 / 2.0).round() as i32;

        let scores: Vec<(i32, f32)> = (200..=470)
            .map(|x| {
                let best = (row_top - Y_SPAN..=row_top + Y_SPAN)
                    .map(|y| score_at(&img, x, y, PC_SIDE))
                    .fold(0.0f32, f32::max);
                (x, best)
            })
            .collect();
        // A peak is a column over the threshold that beats everything within
        // half a slot pitch of it, so one frame contributes one column; ties go
        // to the left, which keeps the sweep deterministic.
        let peaks: Vec<(i32, f32)> = scores
            .iter()
            .enumerate()
            .filter(|(i, (x, s))| {
                *s > T
                    && scores.iter().enumerate().all(|(j, (xj, sj))| {
                        (xj - x).abs() > 20 || *sj < *s || (*sj == *s && j >= *i)
                    })
            })
            .map(|(_, &peak)| peak)
            .collect();

        let found: Vec<i32> = peaks.iter().map(|&(x, _)| x + PC_ORIGIN.0).collect();
        assert_eq!(
            found, PC_FRAME_COLUMNS,
            "the sweep found its frames at {found:?}, not at the columns this file pins",
        );
        let weakest = peaks.iter().map(|&(_, s)| s).fold(f32::INFINITY, f32::min);
        assert!(
            weakest > T,
            "the weakest frame column scores {weakest}, under the threshold {T}; measured 11.21",
        );
        for pair in found.windows(2) {
            let gap = (pair[1] - pair[0]) as f32;
            assert!((gap - 43.75).abs() <= 0.75, "column gap {gap} is not the measured 43.75");
        }
    }

    // -- (3) the ring score ---------------------------------------------------

    /// The one discrimination the whole fit rests on, in three numbers: art
    /// separates from empty panel by a factor, and a square one cell-width off
    /// scores nothing at all — so the grid search cannot be pulled off the
    /// frame by a bright icon.
    #[test]
    fn the_ring_score_discriminates_frame_from_panel_and_from_a_near_miss() {
        let img = fixture("merc-skills-panel.png");
        let cells = ref_cells_at_truth();

        let occupied: Vec<f32> = cells
            .iter()
            .filter(|(row, slot, ..)| REF_OCCUPIED[*row].contains(slot))
            .map(|&(_, _, x, y)| best_score_at(&img, x, y, REF_SIDE))
            .collect();
        let empty: Vec<f32> = cells
            .iter()
            .filter(|(row, slot, ..)| !REF_OCCUPIED[*row].contains(slot))
            .map(|&(_, _, x, y)| best_score_at(&img, x, y, REF_SIDE))
            .collect();
        let displaced: Vec<f32> = cells
            .iter()
            .filter(|(row, slot, ..)| REF_OCCUPIED[*row].contains(slot))
            .flat_map(|&(_, _, x, y)| {
                [-3i32, 3].map(|dx| best_score_at(&img, x + dx, y, REF_SIDE))
            })
            .collect();

        let min_occupied = occupied.iter().copied().fold(f32::INFINITY, f32::min);
        assert!(
            min_occupied >= 7.5,
            "the weakest of the twelve occupied cells scores {min_occupied}; measured 8.13",
        );
        assert_eq!(
            empty.iter().copied().fold(0.0, f32::max),
            0.0,
            "every one of the twenty-four empty slots must score exactly zero",
        );
        assert_eq!(
            displaced.iter().copied().fold(0.0, f32::max),
            0.0,
            "an occupied cell three px off its frame must score exactly zero",
        );
    }

    // -- (4) the one-cell fallback -------------------------------------------

    /// A mercenary with one support, or a tooltip over all but one cell. There
    /// is no pitch to measure, so the scale comes from that cell's own dark
    /// square — and on the lossless fixture it recovers the truth exactly.
    #[test]
    fn one_occupied_cell_still_fits_from_its_own_frame() {
        let g = MercGeometry::default();
        let img = only_cells(&fixture("merc-skills-panel.png"), &REF_TRUTH, &[(5, 0)]);
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");

        let out = refine(&img, Frame::cropped(REF_ORIGIN, REF_SCREEN), layout, &g);

        let fit = out.fit.expect("one cell is enough for the fallback");
        assert_eq!(fit.cells_used, 1);
        assert_eq!(fit.slot_span, 0, "one cell carries no lever arm and must not claim one");
        assert_eq!(fit.dark_side, REF_SIDE);
        assert_eq!(fit.cell_size, 44);
        assert!(
            (fit.x0 - (REF_ORIGIN.0 as f32 + REF_FRAME_X0)).abs() <= 1.0,
            "the surviving cell is in slot 0, so x0 is its own x: {}",
            fit.x0 - REF_ORIGIN.0 as f32,
        );
        assert_eq!(
            FittedScale::from_fit(&fit, out.layout.column_x0).source,
            FitSource::OneCell,
            "and the registration it proposes says which path measured it, which is what \
             stops it overwriting a grid fit",
        );
    }

    /// The fallback's cost, pinned at the value it actually produces rather
    /// than the one we would like. On the PC's JPEG the single cell of row 3
    /// scores higher at a 38 px square than at the 39 the grid path measures,
    /// so the fallback reports a 39 px cell where the grid reports 40 — a whole
    /// px of window. `run::next_fitted_scale` is what keeps this out of a
    /// session that already has a grid fit.
    #[test]
    fn the_one_cell_fallback_is_scale_quantised_at_the_pc() {
        let g = MercGeometry::default();
        let img = only_cells(&fixture("merc-recruit-pc-1080p.png"), &PC_TRUTH, &[(3, 0)]);
        let layout = detect(&pc_lines(), &g, &vocab(), None).expect("the PC panel");

        let out = refine(&img, Frame::cropped(PC_ORIGIN, PC_SCREEN), layout, &g);

        let fit = out.fit.expect("one cell is enough for the fallback");
        assert_eq!(fit.cells_used, 1);
        assert_eq!(fit.dark_side, 38, "the degraded measurement, not the grid path's 39");
        assert_eq!(fit.cell_size, 39, "…which is a 39 px cell where the grid reports 40");
        assert_eq!(
            seed::cell_px(&g, fit.scale),
            39,
            "and the seed store would be re-derived at that cell, one px small",
        );
    }

    // -- (5) the lever arm ----------------------------------------------------

    /// Two cells in ADJACENT slots pin the pitch to about ±0.5 px, which is
    /// ±2.5 px by slot 5 — worse than the drift the fit exists to remove. So
    /// the fit declines and the layout comes back exactly as the OCR built it.
    #[test]
    fn two_adjacent_cells_have_no_lever_arm() {
        let g = MercGeometry::default();
        let img = only_cells(&fixture("merc-skills-panel.png"), &REF_TRUTH, &[(0, 0), (0, 1)]);
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let before = layout.clone();

        let out = refine(&img, Frame::cropped(REF_ORIGIN, REF_SCREEN), layout, &g);

        assert_eq!(out.declined, Some(FitDecline::NoLeverArm { span: 1 }));
        assert_eq!(out.fit, None);
        assert_eq!(out.layout, before, "a decline must leave the layout untouched");
        assert_eq!(out.layout.scale_source, ScaleSource::Ocr);
    }

    // -- (6) systematic OCR error --------------------------------------------

    /// The OCR reads the skill column five px off — a wider leading glyph, a
    /// different font hinting. `X_SPAN` is 14, so the search still reaches the
    /// frame and comes back with the SAME fit: the OCR's x is a starting point,
    /// never a constraint.
    #[test]
    fn an_x_shifted_ocr_column_is_absorbed_and_lands_on_the_same_frame() {
        let g = MercGeometry::default();
        let img = fixture("merc-skills-panel.png");
        let truth = ref_refined().fit.expect("the unshifted fit");

        for dx in [-5, 5] {
            let lines: Vec<OcrLineBox> = reference_lines()
                .into_iter()
                .map(|mut l| {
                    if l.x == 134 {
                        l.x += dx;
                    }
                    l
                })
                .collect();
            let layout = detect(&lines, &g, &vocab(), None).expect("the shifted panel");
            let out = refine(&img, Frame::cropped(REF_ORIGIN, REF_SCREEN), layout, &g);

            let fit = out.fit.unwrap_or_else(|| panic!("a {dx} px column shift must still fit"));
            assert!(
                (fit.x0 - truth.x0).abs() <= 0.5,
                "a {dx} px column shift moved the fit to {}, not {}",
                fit.x0,
                truth.x0,
            );
            assert_eq!(fit.cells_used, 12, "and it must not cost a cell");
        }
    }

    /// The same error in Y is NOT absorbed, and that is the point: the cells
    /// stop clearing the ring score at their own frames, two survive at a WRONG
    /// pitch (48.20 five px down, 47.45 five px up — both measured), and the
    /// lever-arm rule throws them away. The failure mode the fit must never
    /// have is a confident fit on the wrong grid.
    #[test]
    fn a_five_px_row_shift_is_declined_rather_than_fitted() {
        let g = MercGeometry::default();
        let img = fixture("merc-skills-panel.png");

        for dy in [-5, 5] {
            let layout = detect(&rows_shifted(dy), &g, &vocab(), None).expect("the shifted panel");
            let before = layout.clone();
            let out = refine(&img, Frame::cropped(REF_ORIGIN, REF_SCREEN), layout, &g);

            assert_eq!(
                out.declined,
                Some(FitDecline::NoLeverArm { span: 0 }),
                "a {dy} px row shift leaves two cells in ONE slot, which pins nothing",
            );
            assert_eq!(out.layout, before, "a {dy} px row shift must change nothing");
        }
    }

    /// …and the cliff itself, so the band is a measurement rather than a guess:
    /// inside `Y_SPAN` the fit still lands on the same grid, on MEASURED fewer
    /// cells — seven at -4 and nine at +3. Asymmetric because the row tops sit
    /// half a px below their OCR centres, which is also why the far edge is at
    /// -4 on one side and +3 on the other.
    #[test]
    fn a_row_shift_inside_the_y_search_still_lands_on_the_measured_frame() {
        let g = MercGeometry::default();
        let img = fixture("merc-skills-panel.png");
        let truth = ref_refined().fit.expect("the unshifted fit");

        for (dy, cells) in [(-4, 7), (3, 9)] {
            let layout = detect(&rows_shifted(dy), &g, &vocab(), None).expect("the shifted panel");
            let out = refine(&img, Frame::cropped(REF_ORIGIN, REF_SCREEN), layout, &g);

            let fit = out.fit.unwrap_or_else(|| panic!("a {dy} px row shift is inside the search"));
            assert!(
                (fit.x0 - truth.x0).abs() <= 0.5 && (fit.pitch - truth.pitch).abs() <= 0.01,
                "a {dy} px row shift must land on the same grid, not {:?}",
                (fit.x0, fit.pitch),
            );
            assert_eq!(
                fit.cells_used, cells,
                "a {dy} px row shift costs cells but keeps the lever arm",
            );
        }

        // One px past the reach on the near side, the cliff is a DECLINE rather
        // than a worse fit: the two cells that still clear the score sit in one
        // slot, and the lever-arm rule throws them away.
        let layout = detect(&rows_shifted(4), &g, &vocab(), None).expect("the shifted panel");
        let out = refine(&img, Frame::cropped(REF_ORIGIN, REF_SCREEN), layout, &g);
        assert_eq!(
            out.declined,
            Some(FitDecline::NoLeverArm { span: 0 }),
            "4 px is past the reach",
        );
    }

    // -- (7) the seed store's own expression ---------------------------------

    /// `seed::cell_px` and the fit must be one expression of one input, or a
    /// seed is rendered into a cell the loop does not read. Their agreement
    /// with `dark_side + 1` is asserted on the two fixtures ONLY: it holds at
    /// their two scales and fails on 9.4 % of the scales in between, which is
    /// why `cellfit` carries it as a test and not as a `debug_assert!`.
    #[test]
    fn seed_cell_px_is_the_fitted_cell_size_and_dark_side_plus_one() {
        let g = MercGeometry::default();

        for (name, fit) in [
            ("reference", ref_refined().fit.expect("the reference fit")),
            ("pc", pc_refined().fit.expect("the PC fit")),
        ] {
            assert_eq!(
                seed::cell_px(&g, fit.scale),
                fit.cell_size,
                "{name}: the seed store must render into the cell the fit reads",
            );
            assert_eq!(
                fit.cell_size,
                fit.dark_side + 1,
                "{name}: the outer rect is the dark square plus the pixel its line ends on",
            );
        }
    }

    // -- the settled registration --------------------------------------------

    /// What a tick whose fit DECLINED hands downstream: the registration the
    /// session settled on, re-applied to that tick's own OCR layout. The offset
    /// is relative to `column_x0`, so a panel the player dragged carries the
    /// frame with it — which is what lets a decline keep the fitted rects
    /// instead of falling back to the 6-12 px drift.
    #[test]
    fn a_held_registration_follows_the_column_the_ocr_measured() {
        let g = MercGeometry::default();
        let fitted = ref_refined();
        let fit = fitted.fit.expect("the reference fit");
        let held = FittedScale::from_fit(&fit, fitted.layout.column_x0);
        assert_eq!(held.source, FitSource::Grid, "twelve cells over four slots is the grid path");
        assert_eq!(held.cell_px, fit.cell_size);

        // The next tick: the same panel, seven px to the right, and no fit.
        let mut moved = detect(&reference_lines(), &g, &vocab(), None).expect("the panel");
        moved.column_x0 += 7;
        apply_held(&mut moved, &held, ScaleSource::Held);

        assert_eq!(moved.scale, fit.scale);
        assert_eq!(moved.scale_source, ScaleSource::Held);
        let xs: Vec<i32> = moved.rows[3].cells.iter().take(4).map(|c| c[0]).collect();
        let want: Vec<i32> =
            fitted.layout.rows[3].cells.iter().take(4).map(|c| c[0] + 7).collect();
        assert_eq!(xs, want, "the held rects move with the column, not with the OCR's guess");
        assert_eq!(
            moved.rows[3].cells[0][2],
            fit.cell_size,
            "and they keep the fitted cell size, which is what the seed store is built at",
        );
    }

    // -- (10) the wrapped name row -------------------------------------------

    /// Row 4 of the reference panel is a name the game wrapped onto two lines,
    /// so its centre is the mean of two OCR centres rather than one line's. It
    /// is also the panel's fullest row. Both facts land in one place: its four
    /// cells must sit on the measured frame columns.
    #[test]
    fn the_wrapped_name_rows_cells_land_on_the_frame_columns() {
        let out = ref_refined();

        let wrapped = &out.layout.rows[3];
        assert_eq!(wrapped.text, "Ball Lightning of Orbiting Trap", "row 4 is the wrapped name");
        assert_eq!(wrapped.centre_y, 766.0, "its centre is the mean of its two lines");
        let xs: Vec<i32> = wrapped.cells.iter().take(4).map(|c| c[0] - REF_ORIGIN.0).collect();
        assert_eq!(
            xs,
            vec![312, 361, 410, 458],
            "the four occupied cells sit on the frame at fixture x 312 / 361 / 410 / 458",
        );
        assert_eq!(wrapped.cells[0][1] - REF_ORIGIN.1, 159, "and on the measured row top");
    }

    // -- declines -------------------------------------------------------------

    /// An empty panel has no frame anywhere, so there is nothing to fit and
    /// nothing is claimed. The layout survives untouched at the OCR scale.
    #[test]
    fn a_panel_with_no_frame_anywhere_declines() {
        let g = MercGeometry::default();
        let img = only_cells(&fixture("merc-skills-panel.png"), &REF_TRUTH, &[]);
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let before = layout.clone();

        let out = refine(&img, Frame::cropped(REF_ORIGIN, REF_SCREEN), layout, &g);

        assert_eq!(out.declined, Some(FitDecline::TooFewCells));
        assert_eq!(out.layout, before);
    }

    /// The sanity gate, reached the one way it can be: through
    /// `merc-geometry.json`.
    ///
    /// The search bands are struck around `cell_pitch · s_ocr`, so on the
    /// SHIPPED constants the fitted scale can never be more than 4.7 % from the
    /// OCR's and this gate cannot fire — it is there for the override file,
    /// which is per-field and lets `row_pitch` be corrected on its own. A
    /// `row_pitch` of 44 makes the OCR report a scale 9 % high while the panel
    /// in front of it has not moved; the fit finds the real frame, sees the two
    /// cues disagree by more than [`SCALE_BAND`], and hands the layout back at
    /// the OCR's scale rather than silently rescaling the whole capture.
    #[test]
    fn a_frame_scale_far_from_the_ocrs_is_refused() {
        let g = MercGeometry {
            // Wrong, and the reason the OCR scale is inflated.
            row_pitch: 44.0,
            // Kept consistent with the panel, so the search still reaches the
            // real frame and the gate is what refuses it — not a miss.
            cell_pitch: 48.67 / (48.0 / 44.0),
            cell_offset_x: 238.0 / (48.0 / 44.0),
            ..MercGeometry::default()
        };
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let before = layout.clone();

        let out = refine(
            &fixture("merc-skills-panel.png"),
            Frame::cropped(REF_ORIGIN, REF_SCREEN),
            layout,
            &g,
        );

        match out.declined {
            Some(FitDecline::OutOfBand { ratio }) => assert!(
                (ratio - 0.916).abs() < 0.01,
                "the frame reads 0.916× the OCR's inflated scale, not {ratio}",
            ),
            other => panic!("expected the sanity gate to refuse, got {other:?}"),
        }
        assert_eq!(out.fit, None);
        assert_eq!(out.layout, before, "and the layout keeps the scale it came in with");
    }

    /// Every decline has to say WHY in the log line the smoke check reads —
    /// three shapes, three sentences, none of them the debug formatting.
    #[test]
    fn a_decline_reads_as_a_sentence() {
        assert!(FitDecline::TooFewCells.to_string().contains("no cell frame cleared"));
        assert!(FitDecline::OneCellUnmeasured { slot: 2 }
            .to_string()
            .contains("slot 2's frame cleared"));
        assert!(FitDecline::NoLeverArm { span: 1 }.to_string().contains("span 1 slot(s)"));
        assert!(FitDecline::OutOfBand { ratio: 1.4 }.to_string().contains("1.400×"));
    }

    // -- AC1/AC2 (Part C): what the corpus was cut through --------------------
    //
    // The step statistic itself — [`DARK_LINE`], [`LIGHT_LINE`], [`EDGE_BAND`],
    // [`column_means`], [`frame_step_columns`] — lives at module level so
    // `icons`' corpus reader runs the same one.

    /// One fixture read twice: the layout the OCR path registers, and the
    /// [`Refined`] the fit re-registers it to. Both come off the SAME `detect`
    /// call the pinned fits above use, so the crops below are the crops the
    /// loop would cut on that machine.
    fn both_registrations(
        file: &str,
        origin: (i32, i32),
        screen: [u32; 2],
        lines: Vec<OcrLineBox>,
    ) -> (DynamicImage, Frame, MercLayout, Refined) {
        let g = MercGeometry::default();
        let img = fixture(file);
        let frame = Frame::cropped(origin, screen);
        let ocr = detect(&lines, &g, &vocab(), None).expect("the panel");
        let refined = refine(&img, frame, ocr.clone(), &g);
        (img, frame, ocr, refined)
    }

    /// The reference and the PC, named, each read both ways.
    fn both_fixtures() -> Vec<(&'static str, DynamicImage, Frame, MercLayout, Refined)> {
        let (ri, rf, ro, rr) =
            both_registrations("merc-skills-panel.png", REF_ORIGIN, REF_SCREEN, reference_lines());
        let (pi, pf, po, pr) = both_registrations(
            "merc-recruit-pc-1080p.png",
            PC_ORIGIN,
            PC_SCREEN,
            pc_lines(),
        );
        vec![("reference", ri, rf, ro, rr), ("pc", pi, pf, po, pr)]
    }

    /// Every occupied cell of a fitted panel as `(row, slot, ocr rect, fitted
    /// rect)`, in FIXTURE px. Occupancy is decided on the FITTED rect, which
    /// is the rect `build_capture` decides it on.
    fn occupied_pairs(
        img: &DynamicImage,
        frame: Frame,
        ocr: &MercLayout,
        refined: &Refined,
    ) -> Vec<(usize, usize, [i32; 4], [i32; 4])> {
        let g = MercGeometry::default();
        let mut out = Vec::new();
        for (r, row) in refined.layout.rows.iter().enumerate() {
            for (s, &cell) in row.cells.iter().enumerate() {
                let fitted = frame.local(cell);
                if crate::mercenary::geometry::occupied(img, fitted, &g) {
                    out.push((r, s, frame.local(ocr.rows[r].cells[s]), fitted));
                }
            }
        }
        out
    }

    /// AC1 on pixels: the cell crop the loop cuts today runs INTO the gold
    /// frame, and the one the fit cuts starts ON it.
    ///
    /// Measured over all 27 occupied cells of the two committed fixtures, so
    /// this is the corpus's own defect rather than a description of it. The
    /// OCR crop carries the frame's dark|light step at exactly ONE column of
    /// its left eight — column 4 at the reference's slot 0 walking right to 7
    /// by slot 3, and column 3 at the PC's slot 0 walking to 7 by slot 4,
    /// because the 0.7 % pitch error ACCUMULATES with the slot. The fitted
    /// crop carries it at none of its eight, and its first column IS the light
    /// line: 37.6-41.5 against the OCR crop's 5.2-23.1.
    ///
    /// Only the LEFT edge is a control, and that is a measurement rather than
    /// an omission: the fit moves x and (D4) leaves y where the OCR centres
    /// put it, so the top and bottom edges read the same both ways (the step
    /// appears on 3 of 27 OCR crops and 2 of 27 fitted ones). The right edge
    /// is one rounding px from the frame's OTHER dark line — `cell_size` 44
    /// wraps a 43 px square — and catches it on 2 of the 27 fitted crops. The
    /// registration the corpus is cut at is a LEFT-edge fact.
    ///
    /// This test is sharp one px RIGHT and tolerant one px left — the light
    /// line is two px wide. [`the_frames_dark_line_sits_two_px_left_of_the_fitted_crop`]
    /// closes the other side.
    #[test]
    fn the_fit_moves_the_cell_crop_off_the_gold_frame_it_was_cutting_through() {
        let g = MercGeometry::default();
        let mut cells = 0;

        for (name, img, frame, ocr, refined) in both_fixtures() {
            assert!(refined.fit.is_some(), "{name}: precondition, the frame is found");
            for (r, s, ocr_rect, fitted_rect) in occupied_pairs(&img, frame, &ocr, &refined) {
                cells += 1;
                let at = format!("{name} row {r} slot {s}");
                let today = crate::mercenary::read::crop_rgba(&img, ocr_rect, &g)
                    .unwrap_or_else(|| panic!("{at}: the OCR crop lies on the fixture"));
                let fitted = crate::mercenary::read::crop_rgba(&img, fitted_rect, &g)
                    .unwrap_or_else(|| panic!("{at}: the fitted crop lies on the fixture"));

                let (today_means, fitted_means) = (column_means(&today), column_means(&fitted));
                let steps = frame_step_columns(&today_means);
                assert_eq!(
                    steps.len(),
                    1,
                    "{at}: the OCR crop must carry the frame at exactly one of its left \
                     {EDGE_BAND} columns, not at {steps:?} — profile {:?}",
                    &today_means[..EDGE_BAND],
                );
                assert!(
                    (3..=7).contains(&steps[0]),
                    "{at}: the measured drift puts the frame at column 3-7, not {}",
                    steps[0],
                );
                assert_eq!(
                    frame_step_columns(&fitted_means),
                    Vec::<usize>::new(),
                    "{at}: the fitted crop still cuts through the frame — profile {:?}",
                    &fitted_means[..EDGE_BAND],
                );
                assert!(
                    fitted_means[0] >= LIGHT_LINE,
                    "{at}: the fitted crop must START on the light line, not at {}",
                    fitted_means[0],
                );
                assert!(
                    today_means[0] < LIGHT_LINE,
                    "{at}: the OCR crop must start short of the light line — on panel at \
                     the near slots, on the previous cell's dark line at the far ones — \
                     not at {}",
                    today_means[0],
                );
            }
        }

        assert_eq!(cells, 27, "the two fixtures carry 12 + 15 occupied cells");
    }

    /// AC1's other half, and the sharpness of the registration: the frame's
    /// dark line is EXACTLY two px left of the fitted crop's first column.
    ///
    /// Two px is `cell_inset`, so this is the statement that the fitted OUTER
    /// rect's own left edge IS the dark line. Measured: a fit reporting `x0`
    /// one px right reads the light line here (33.7) and one px left reads
    /// flat panel (17.8), against the 6.4-11.9 the dark line gives. It earns
    /// its place beside the test above, which a one-px-LEFT error still passes
    /// (the light line is 2 px wide, so the crop still starts on it) — this is
    /// the half that pins the registration to a single pixel in BOTH
    /// directions.
    #[test]
    fn the_frames_dark_line_sits_two_px_left_of_the_fitted_crop() {
        let g = MercGeometry::default();
        let mut cells = 0;

        for (name, img, frame, ocr, refined) in both_fixtures() {
            for (r, s, _, fitted_rect) in occupied_pairs(&img, frame, &ocr, &refined) {
                cells += 1;
                let back = [fitted_rect[0] - 2, fitted_rect[1], fitted_rect[2], fitted_rect[3]];
                let crop = crate::mercenary::read::crop_rgba(&img, back, &g)
                    .unwrap_or_else(|| panic!("{name} row {r} slot {s}: the probe is on-image"));
                let first = column_means(&crop)[0];
                assert!(
                    first <= DARK_LINE,
                    "{name} row {r} slot {s}: two px left of the fitted crop must be the \
                     frame's dark line (measured 6.4-11.9), not {first}",
                );
            }
        }

        assert_eq!(cells, 27, "the two fixtures carry 12 + 15 occupied cells");
    }
}
