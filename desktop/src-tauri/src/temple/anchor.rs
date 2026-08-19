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

/// Screen width the template was cut at. `image_width / this` is the opening
/// guess for the scale sweep.
pub const REFERENCE_SCREEN_WIDTH: u32 = 1374;

/// Fine-score NCC below which an anchor is rejected outright.
///
/// Measured: correct anchors scored 0.942–1.000 across 8 boards; the two wrong
/// anchors that invented an Apex corridor scored 0.809 and 0.829. 0.88 is the
/// middle of that gap.
pub const NCC_FLOOR: f32 = 0.88;

/// Half-width of the seeded scale band around `image_width / reference_width`.
const SEED_TOLERANCE: f32 = 0.15;
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
/// Full-resolution search radius around a nominee, in image px. A coarse
/// position is accurate to ±[`COARSE_DIVISOR`] px by construction; the rest is
/// slack for the resampling difference between the two resolutions.
const FINE_RADIUS: i32 = COARSE_DIVISOR as i32 + 8;

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

/// Anchor, trying a remembered scale first.
///
/// Three attempts, each stopping the moment one clears [`NCC_FLOOR`]:
///
/// 1. the hint's scale alone, when it was measured at this capture size;
/// 2. the [`seed_band`] — `image_width / REFERENCE_SCREEN_WIDTH` ± 15%;
/// 3. the [`full_sweep`].
///
/// Both derive from the capture's WIDTH, so `img` must be a full game-window
/// capture; see [`super::reader::read_layout`].
///
/// A stale hint therefore costs one extra single-scale match and is never
/// *believed* — the guard against proceeding on a low-confidence anchor is the
/// floor, not the provenance of the scale.
pub fn anchor_with_hint(
    img: &DynamicImage,
    hint: Option<&AnchorCalibration>,
) -> Result<Anchor, ReadError> {
    let scene = Scene::of(img);
    let mut best: Option<Anchor> = None;

    let attempt = |grid: Vec<f32>, best: &mut Option<Anchor>| -> bool {
        if let Some(found) = scene.search(&grid) {
            if best.map_or(true, |b| found.ncc > b.ncc) {
                *best = Some(found);
            }
            return found.ncc >= NCC_FLOOR;
        }
        false
    };

    if let Some(h) = hint.filter(|h| h.applies_to(img)) {
        if attempt(vec![h.scale], &mut best) {
            return Ok(best.expect("a cleared attempt recorded a best"));
        }
    }
    if attempt(seed_band(img.width()), &mut best) {
        return Ok(best.expect("a cleared attempt recorded a best"));
    }
    if attempt(full_sweep(img.width()), &mut best) {
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
/// MISS it runs all three attempts — hint, [`seed_band`], [`full_sweep`] —
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
/// 2. **One nominating correlation** at `image_width / REFERENCE_SCREEN_WIDTH`,
///    against [`COARSE_CANDIDATE_FLOOR`]. One scale, not the whole
///    [`seed_band`], because the ÷4 pass is famously scale-insensitive here
///    (it ranked 1.09 above the true 1.13 at 0.968 vs 0.961) — the property
///    that makes it useless as a *winner* is what makes one of its scales
///    enough as a *detector*. The measured seeds sit within 3% of the true
///    scale on all five boards.
///
/// Step 2 runs even when a hint applied and missed, which is what bounds the
/// recovery of a panel that moved or rescaled to one tick: without it a stale
/// origin would hide the panel until the caller's periodic full read.
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
    let (x, y, score) = locate(&level, &scaled, Some(fine_window(top_left)))?;
    (score >= NCC_FLOOR).then_some(Anchor {
        origin: (x + tw as i32 / 2, y + th as i32 / 2),
        scale,
        ncc: score,
    })
}

/// The best ÷4 score at the width-derived scale alone, over the whole capture.
///
/// `None` when that scale produces a template the coarse level cannot hold.
fn coarse_candidate(img: &DynamicImage) -> Option<f32> {
    let level = coarse_level(img);
    let tmpl = template();
    let scale = seed_scale(img.width());
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

/// The scale the capture's own width implies, and the centre of the seeded
/// band.
fn seed_scale(width: u32) -> f32 {
    width as f32 / REFERENCE_SCREEN_WIDTH as f32
}

/// The seeded band for a capture this wide: the width-derived seed
/// ± [`SEED_TOLERANCE`], on the [`SCALE_STEP`] grid.
fn seed_band(width: u32) -> Vec<f32> {
    let seed = seed_scale(width);
    grid(seed * (1.0 - SEED_TOLERANCE), seed * (1.0 + SEED_TOLERANCE))
}

/// The fallback sweep for a capture this wide, used only when [`seed_band`]
/// fails to clear [`NCC_FLOOR`].
///
/// [`FULL_SWEEP`]'s fixed 0.80–1.60 covers every window size measured so far,
/// but it is anchored on a 1374 px reference: a 2560 px capture seeds at 1.86
/// and a 3840 px one at 2.79, both above 1.60. A fixed band could therefore
/// never reach the true scale on exactly the captures whose seeded band it
/// exists to rescue. The ceiling therefore rises to twice the seed.
///
/// Only the ceiling. Widening downward would answer a case that does not
/// exist: a capture narrow enough to seed below 1.60 is already inside the
/// fixed band, and 0.80 is below the smallest scale the game's own UI produces.
fn full_sweep(width: u32) -> Vec<f32> {
    grid(FULL_SWEEP.0, FULL_SWEEP.1.max(seed_scale(width) * 2.0))
}

/// Scale candidates from `lo` to `hi` inclusive, on the [`SCALE_STEP`] grid.
fn grid(lo: f32, hi: f32) -> Vec<f32> {
    let steps = ((hi - lo) / SCALE_STEP).round().max(0.0) as usize;
    (0..=steps)
        .map(|i| (lo + i as f32 * SCALE_STEP).max(0.01))
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
    fn nominate(&self, scales: &[f32]) -> Vec<Nominee> {
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
        nominees
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
        let tmpl = template();
        let nominees = self.nominate(scales);
        let mut groups: Vec<(u32, u32)> = Vec::new();
        let mut best: Option<Anchor> = None;
        for nominee in &nominees {
            if !groups.contains(&nominee.size) {
                if groups.len() == TOP_K_GROUPS {
                    break;
                }
                groups.push(nominee.size);
            }
            let (tw, th) = (
                (tmpl.width() as f32 * nominee.scale) as u32,
                (tmpl.height() as f32 * nominee.scale) as u32,
            );
            if tw as usize >= self.full.gray.w || th as usize >= self.full.gray.h {
                continue;
            }
            let scaled =
                Gray::from_rgb(&image::imageops::resize(tmpl, tw, th, FilterType::Triangle));
            let origin = (
                nominee.at.0 * COARSE_DIVISOR as i32,
                nominee.at.1 * COARSE_DIVISOR as i32,
            );
            let Some((x, y, score)) = locate(&self.full, &scaled, Some(fine_window(origin)))
            else {
                continue;
            };
            if best.map_or(true, |b| score > b.ncc) {
                best = Some(Anchor {
                    origin: (x + tw as i32 / 2, y + th as i32 / 2),
                    scale: nominee.scale,
                    ncc: score,
                });
            }
        }
        best
    }
}

/// The full-resolution search box around a coarse nominee's position,
/// `(x0, y0, x1, y1)` inclusive — `(2 · FINE_RADIUS + 1)²` positions.
///
/// This bound is what keeps the fine pass affordable: without it each nominee
/// would be re-correlated over the whole capture at full resolution.
fn fine_window(origin: (i32, i32)) -> (i32, i32, i32, i32) {
    (
        origin.0 - FINE_RADIUS,
        origin.1 - FINE_RADIUS,
        origin.0 + FINE_RADIUS,
        origin.1 + FINE_RADIUS,
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

    /// The seeded band is centred on the width-derived scale and reaches
    /// [`SEED_TOLERANCE`] either side of it — 1539 px implies 1.12, and the
    /// board's true 1.13 has to be inside.
    #[test]
    fn the_seeded_band_spans_the_tolerance_around_the_width_derived_scale() {
        let band = seed_band(1539);
        let seed = 1539.0 / REFERENCE_SCREEN_WIDTH as f32;
        let (lo, hi) = (*band.first().unwrap(), *band.last().unwrap());
        assert!(
            (lo - seed * (1.0 - SEED_TOLERANCE)).abs() <= SCALE_STEP,
            "band starts at {lo}, not {} below the {seed} seed",
            SEED_TOLERANCE
        );
        assert!(
            (hi - seed * (1.0 + SEED_TOLERANCE)).abs() <= SCALE_STEP,
            "band ends at {hi}, not {} above the {seed} seed",
            SEED_TOLERANCE
        );
        assert!(lo < 1.13 && hi > 1.13, "1.13 is not inside {lo}..{hi}");
    }

    /// …on a grid of [`SCALE_STEP`]. Pinned as a count, so widening the step
    /// cannot pass by keeping the ends where they are.
    #[test]
    fn the_seeded_band_is_stepped_at_the_scale_grid() {
        let band = seed_band(1539);
        // (1.12009 * 1.15 - 1.12009 * 0.85) / 0.01 = 33.6 steps, so 35 scales.
        assert_eq!(band.len(), 35, "band {band:?}");
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
    /// in a 260 px-wide capture seeds a band of 0.16–0.22 that cannot contain
    /// it, and the sweep still finds it.
    #[test]
    fn the_full_sweep_anchors_a_scale_the_seeded_band_cannot_reach() {
        let scale = 1.30f32;
        let t = template();
        let big = image::imageops::resize(
            t,
            (t.width() as f32 * scale) as u32,
            (t.height() as f32 * scale) as u32,
            FilterType::Triangle,
        );
        let mut img = noise(260, 140);
        assert!(
            !seed_band(img.width()).iter().any(|&s| (s - scale).abs() < 0.05),
            "the seeded band for {} px must not reach {scale}",
            img.width()
        );
        image::imageops::replace(&mut img, &big, 10, 10);
        let found = anchor(&DynamicImage::ImageRgb8(img)).expect("the full sweep finds it");
        assert!(
            (found.scale - scale).abs() <= 0.03,
            "recovered scale {}",
            found.scale
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
            "the fine pass correlated {positions} positions; 0 means it searched \
             unwindowed, more means the radius grew"
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
    /// Fails if the cheap tick ever reaches [`seed_band`] or [`full_sweep`].
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
        .nominate(scales)
        .into_iter()
        .map(|n| (n.score, n.scale))
        .collect()
}

/// The fine pass's search box for a nominee at `origin`, as production builds it.
#[cfg(test)]
pub fn fine_window_for_test(origin: (i32, i32)) -> (i32, i32, i32, i32) {
    fine_window(origin)
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
