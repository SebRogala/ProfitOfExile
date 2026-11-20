use image::{DynamicImage, GenericImageView};

use super::super::geometry::{inner_rect, luma};
use super::super::{BadgeGeometry, MercGeometry};

/// One vertical stroke of a roman numeral.
struct Stroke {
    width: i32,
    /// Longest CONTIGUOUS ink run in the tallest column of the stroke.
    height: i32,
    top: i32,
    bottom: i32,
}

/// Read the roman tier badge from a support cell.
///
/// Takes the cell's OUTER rect. Returns 1, 2 or 3, or `None` when the badge
/// does not read cleanly — a wrong tier resolves to a different,
/// confidently-named support, so silence is the only safe failure.
///
/// # How it separates the numeral from the art
///
/// The numerals are GOLD (measured on the reference panel: core stroke
/// 255/215/142 sRGB), which excludes the blue-white and grey highlights most
/// icons carry, but NOT the gold frames and warm art the same corner holds —
/// icon bleed measured p99 219 luma, as bright as the badge. So brightness and
/// hue only build the ink mask; the accept rule is SHAPE:
///
/// 1. Only a scanline band in the middle of the badge box votes on columns, so
///    a serif above the glyph or art below it cannot add a stroke.
/// 2. A column is part of a stroke when it is ink for `column_fill` of that
///    band — a blob clipping one scanline does not count.
/// 3. The strokes must share a baseline and a top within a couple of px, have
///    comparable widths and heights, and fall inside absolute size caps. Roman
///    numerals are identical thin bars on one baseline; art is not. The caps
///    carry the whole judgement for tier I, whose single stroke has nothing to
///    be "comparable" to.
pub fn read_tier(img: &DynamicImage, rect: [i32; 4], g: &MercGeometry) -> Option<u8> {
    let [x, y, w, h] = inner_rect(rect, g);
    if x < 0 || y < 0 || w <= 0 || h <= 0 {
        return None;
    }
    let (iw, ih) = img.dimensions();
    if (x + w) as u32 > iw || (y + h) as u32 > ih {
        return None;
    }
    let b = &g.badge;
    let x1 = x + w;
    let y1 = y + h;
    let bx0 = x1 - (w as f32 * b.width_frac).round() as i32;
    let by0 = y1 - (h as f32 * b.top_frac).round() as i32;
    let by1 = y1 - (h as f32 * b.bottom_frac).round() as i32;
    if bx0 < x || by0 < y || by1 <= by0 {
        return None;
    }
    let band_h = by1 - by0;
    let m0 = by0 + (band_h as f32 * b.band_lo_frac).round() as i32;
    let m1 = by0 + (band_h as f32 * b.band_hi_frac).round() as i32;
    if m1 <= m0 {
        return None;
    }

    let is_ink = |px: i32, py: i32| -> bool {
        let p = img.get_pixel(px as u32, py as u32).0;
        luma(p[0], p[1], p[2]) >= b.ink_luma_min
            && p[0] >= p[1]
            && p[1] >= p[2]
            && (p[0] as i32 - p[2] as i32) >= b.ink_gold_delta
    };

    // 1-2. Which columns are stroke columns.
    let band_rows = (m1 - m0) as f32;
    let stroke_columns: Vec<bool> = (bx0..x1)
        .map(|px| {
            let hits = (m0..m1).filter(|&py| is_ink(px, py)).count() as f32;
            hits / band_rows >= b.column_fill
        })
        .collect();

    // Group consecutive stroke columns into runs.
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &on) in stroke_columns.iter().chain(std::iter::once(&false)).enumerate() {
        match (on, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                runs.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if runs.is_empty() || runs.len() > 3 {
        return None;
    }

    // 3. Shape checks over the FULL badge box.
    let mut strokes = Vec::with_capacity(runs.len());
    for (a, z) in &runs {
        let mut height = 0;
        let mut top = i32::MAX;
        let mut bottom = i32::MIN;
        for i in *a..*z {
            let px = bx0 + i as i32;
            if let Some((len, t, bo)) = longest_ink_run(by0, by1, |py| is_ink(px, py)) {
                height = height.max(len);
                top = top.min(t);
                bottom = bottom.max(bo);
            }
        }
        if height == 0 {
            return None;
        }
        strokes.push(Stroke {
            width: (*z - *a) as i32,
            height,
            top,
            bottom,
        });
    }
    accept_strokes(&strokes, w, h, b).then_some(strokes.len() as u8)
}

/// Longest contiguous run of ink in one column, and where it sits. A roman
/// stroke is a solid bar; measuring the LONGEST RUN rather than the ink extent
/// is what stops a serif dot or an art speck several px above the glyph from
/// inflating its height.
fn longest_ink_run(y0: i32, y1: i32, ink: impl Fn(i32) -> bool) -> Option<(i32, i32, i32)> {
    let mut best: Option<(i32, i32, i32)> = None;
    let mut run = 0;
    for y in y0..y1 {
        if ink(y) {
            run += 1;
            if best.is_none_or(|(len, _, _)| run > len) {
                best = Some((run, y - run + 1, y));
            }
        } else {
            run = 0;
        }
    }
    best
}

/// Whether a set of stroke candidates reads as a roman numeral.
fn accept_strokes(strokes: &[Stroke], cell_w: i32, cell_h: i32, b: &BadgeGeometry) -> bool {
    let widths: Vec<i32> = strokes.iter().map(|s| s.width).collect();
    let heights: Vec<i32> = strokes.iter().map(|s| s.height).collect();
    let bottoms: Vec<i32> = strokes.iter().map(|s| s.bottom).collect();
    let tops: Vec<i32> = strokes.iter().map(|s| s.top).collect();
    let span = |v: &[i32]| v.iter().max().copied().unwrap_or(0) - v.iter().min().copied().unwrap_or(0);

    if span(&bottoms) > b.baseline_tolerance || span(&tops) > b.top_tolerance {
        return false;
    }
    let (wmin, wmax) = (*widths.iter().min().unwrap(), *widths.iter().max().unwrap());
    if wmax as f32 > wmin as f32 * b.width_ratio_max {
        return false;
    }
    let (hmin, hmax) = (*heights.iter().min().unwrap(), *heights.iter().max().unwrap());
    if hmax as f32 > hmin as f32 * b.height_ratio_max {
        return false;
    }
    // Absolute caps. The three ratio rules above compare strokes to EACH
    // OTHER, so all three are vacuous for a one-stroke numeral: without these,
    // any single tall or fat bar of gold art in the badge corner reads as
    // tier I.
    hmin as f32 >= cell_h as f32 * b.min_height_frac
        && hmax as f32 <= cell_h as f32 * b.max_height_frac
        && wmax as f32 <= cell_w as f32 * b.max_width_frac
}
