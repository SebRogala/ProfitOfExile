//! Recruit-window panel geometry (POE-165 D2) — pure over OCR line rects.
//!
//! There is no fixed capture region: the recruit window can sit anywhere, at
//! any UI scale. [`detect`] takes the OCR lines of a whole screen and answers
//! "is a recruit window on screen, and where are its rows and support cells?"
//! using nothing but the lines' text and rects, so every rule in it is unit
//! testable on this Linux host — the Windows half (screen grab, OCR call,
//! tick loop) contributes no logic.
//!
//! The one image-touching function here is [`occupied`], which answers whether
//! a support slot holds an icon at all.

use image::GenericImageView;

use super::vocab::MercVocab;
use super::{MercGeometry, MercHeader, ReadState};

/// One OCR line with its bounding rect, in screen px.
///
/// The Windows OCR path builds these by merging an `OcrLine`'s words'
/// `BoundingRect()`s (WI-3); nothing in this module cares where they came
/// from.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrLineBox {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl OcrLineBox {
    pub fn centre_y(&self) -> f32 {
        self.y as f32 + self.h as f32 / 2.0
    }
}

/// One detected skill row.
#[derive(Debug, Clone, PartialEq)]
pub struct MercLayoutRow {
    pub index: u8,
    /// Mean of the member lines' vertical centres — a wrapped two-line name
    /// contributes both, so the row centre lands between them.
    pub centre_y: f32,
    /// `[x, y, w, h]` covering the name text, for the pass-2 re-OCR crop.
    pub name_rect: [i32; 4],
    /// The pass-1 text, member lines joined with a space.
    pub text: String,
    /// Candidate support-cell rects, `[x, y, w, h]`, slot 0 first. ALL slots
    /// are emitted; the caller walks them and stops at the first cell
    /// [`occupied`] rejects (it owns the pixels, this function does not).
    pub cells: Vec<[i32; 4]>,
}

/// A detected recruit window.
#[derive(Debug, Clone, PartialEq)]
pub struct MercLayout {
    /// Runtime scale: observed row pitch ÷ [`MercGeometry::row_pitch`].
    pub scale: f32,
    /// The skill-name column's left edge — the ONE x every cell is measured
    /// from. Never a single row's own x: a leading glyph's side bearing
    /// shifts one row by a couple of px and would skew that row's cells.
    pub column_x0: i32,
    /// Observed row pitch in screen px, before the scale division.
    pub row_pitch: f32,
    pub rows: Vec<MercLayoutRow>,
    pub header: MercHeader,
}

/// Median of a slice, by the mid element after sorting (mean of the middle two
/// when even). Used for the column x and the row pitch: both need the robust
/// centre, since one outlier line is exactly the case they defend against.
fn median(values: &mut [f32]) -> f32 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
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

/// Detect a recruit window in a screen's OCR lines.
///
/// Returns `None` — never a partial guess — when any of the D2 preconditions
/// fails: fewer than [`MercGeometry::min_skill_candidates`] skill-name
/// candidates, or no panel anchor. The anchor is the discriminator against
/// every other PoE surface that lists skill names (a gem tooltip, the
/// character panel): those have skill text but no wager and no recruit
/// buttons. Either chrome line anchors: "Wager" above row 1, or a
/// "TAKE ITEM" / "REMATCH" button below the last row. Measured 2026-08-24 on
/// a 1920×1200 screen: Windows OCR returned NO line for `Wager: 8 831` (small
/// gold text on the dark panel) while both buttons read cleanly, so the wager
/// alone is not a reliable anchor.
pub fn detect(lines: &[OcrLineBox], g: &MercGeometry, vocab: &MercVocab) -> Option<MercLayout> {
    // 1. Skill-name candidates seed the column.
    let candidates: Vec<&OcrLineBox> = lines
        .iter()
        .filter(|l| {
            let read = vocab.match_skill(&l.text, &g.thresholds);
            read.state == ReadState::Matched || read.state == ReadState::LowConfidence
        })
        .collect();
    if candidates.len() < g.min_skill_candidates {
        return None;
    }

    let mut xs: Vec<f32> = candidates.iter().map(|l| l.x as f32).collect();
    let column_x0 = median(&mut xs);
    let mut hs: Vec<f32> = candidates.iter().map(|l| l.h as f32).collect();
    let line_height = median(&mut hs);

    // The column is every line left-aligned with the seed — including the
    // continuation line of a wrapped name, which matches no skill on its own
    // ("Trap" from "Ball Lightning of Orbiting / Trap") and would otherwise
    // leave its row a line short.
    //
    // Bounded to the candidates' own vertical span (widened by one cluster gap
    // so a wrap at either end still joins), because "left-aligned with the
    // panel" is not on its own a panel line: a chat message or an inventory
    // label that happens to share the x would otherwise become a seventh row
    // and drag the pitch median with it.
    let tolerance = (g.column_x_tolerance_frac * line_height).max(1.0);
    let cluster_gap = g.row_cluster_factor * line_height;
    let span_top = candidates
        .iter()
        .map(|l| l.centre_y())
        .fold(f32::INFINITY, f32::min)
        - cluster_gap;
    let span_bottom = candidates
        .iter()
        .map(|l| l.centre_y())
        .fold(f32::NEG_INFINITY, f32::max)
        + cluster_gap;
    let mut column: Vec<&OcrLineBox> = lines
        .iter()
        .filter(|l| {
            (l.x as f32 - column_x0).abs() <= tolerance
                && l.centre_y() >= span_top
                && l.centre_y() <= span_bottom
        })
        .collect();
    column.sort_by(|a, b| {
        a.centre_y()
            .partial_cmp(&b.centre_y())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 2. Cluster the column into rows with an ABSOLUTE seed: consecutive lines
    //    closer than `row_cluster_factor` line heights are one wrapped name.
    //    The pitch cannot seed this — it is an OUTPUT of the clustering.
    let mut clusters: Vec<Vec<&OcrLineBox>> = Vec::new();
    for line in column {
        match clusters.last_mut() {
            Some(last)
                if line.centre_y()
                    - last
                        .last()
                        .expect("clusters never hold an empty group")
                        .centre_y()
                    <= cluster_gap =>
            {
                last.push(line)
            }
            _ => clusters.push(vec![line]),
        }
    }
    if clusters.is_empty() {
        return None;
    }

    // 3. Pitch and scale. With one row there is no inter-row gap to measure,
    //    so the line height is the only cue left.
    let centres: Vec<f32> = clusters
        .iter()
        .map(|c| c.iter().map(|l| l.centre_y()).sum::<f32>() / c.len() as f32)
        .collect();
    let (row_pitch, scale) = if centres.len() >= 2 {
        let mut gaps: Vec<f32> = centres.windows(2).map(|w| w[1] - w[0]).collect();
        let pitch = median(&mut gaps);
        (pitch, pitch / g.row_pitch)
    } else {
        (0.0, line_height / g.ref_line_height)
    };
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }

    // 4. The panel anchor, checked once the pitch is known: a line above row 1
    //    within `wager_search_pitches` of it reading "Wager", or a button line
    //    below the last row within the same reach.
    let first_centre = centres[0];
    let last_centre = centres[centres.len() - 1];
    let reach = if row_pitch > 0.0 {
        g.wager_search_pitches * row_pitch
    } else {
        g.wager_search_pitches * g.row_pitch * scale
    };
    let anchor = lines.iter().find(|l| {
        let c = l.centre_y();
        (c < first_centre && first_centre - c <= reach && is_wager_line(&l.text, g))
            || (c > last_centre && c - last_centre <= reach && is_button_line(&l.text, g))
    });
    anchor?;

    // 5. Rows with their cell rects.
    let cell_size = (g.cell_size * scale).round().max(1.0) as i32;
    let rows: Vec<MercLayoutRow> = clusters
        .iter()
        .zip(&centres)
        .enumerate()
        .map(|(i, (members, &centre))| {
            let x0 = members.iter().map(|l| l.x).min().unwrap_or(column_x0 as i32);
            let top = members.iter().map(|l| l.y).min().unwrap_or(0);
            let bottom = members.iter().map(|l| l.y + l.h).max().unwrap_or(0);
            let right = members.iter().map(|l| l.x + l.w).max().unwrap_or(x0);
            let cells = (0..g.max_slots)
                .map(|slot| {
                    let cx = column_x0
                        + g.cell_offset_x * scale
                        + slot as f32 * g.cell_pitch * scale;
                    [
                        cx.round() as i32,
                        (centre - cell_size as f32 / 2.0).round() as i32,
                        cell_size,
                        cell_size,
                    ]
                })
                .collect();
            MercLayoutRow {
                index: i as u8,
                centre_y: centre,
                name_rect: [x0, top, (right - x0).max(1), (bottom - top).max(1)],
                text: members
                    .iter()
                    .map(|l| l.text.trim())
                    .collect::<Vec<_>>()
                    .join(" "),
                cells,
            }
        })
        .collect();

    Some(MercLayout {
        scale,
        column_x0: column_x0.round() as i32,
        row_pitch,
        header: parse_header(lines, first_centre, &rows, column_x0, row_pitch.max(g.row_pitch * scale)),
        rows,
    })
}

/// Whether a line reads as the panel's "Wager" label.
///
/// D2 specifies a fuzzy match of the WHOLE line against "Wager" at 0.85. Two
/// measured problems with that, both fixed here:
///
/// - the line really reads `Wager: 1 028`, which scores only 0.883 whole —
///   barely over the bar, and OCR noise in the amount pushes it under. So the
///   LEADING WORD is what is scored, cut at the first non-alphanumeric
///   character so `Wager:` and `Wager:1` both reduce to `wager` (1.000);
/// - 0.85 is far too loose for a five-letter word. `Wagers` scores 0.967,
///   `Wage` 0.960 and `Wagner` 0.961 — and "Wagner has entered the area" is an
///   ordinary PoE chat line, which at 0.85 would anchor a capture. The bar is
///   `thresholds.wager_anchor` (0.98), which admits only a clean read.
pub fn is_wager_line(text: &str, g: &MercGeometry) -> bool {
    let lower = text.trim().to_lowercase();
    let head: String = lower
        .split_whitespace()
        .next()
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_alphanumeric())
        .collect();
    if head.is_empty() {
        return false;
    }
    strsim::jaro_winkler(&head, "wager") as f32 >= g.thresholds.wager_anchor
}

/// Whether a line reads as one of the panel's footer buttons, "TAKE ITEM" or
/// "REMATCH". Exact after case and whitespace normalisation: these are
/// single-purpose labels that OCR returns clean or not at all, and
/// Jaro-Winkler's prefix bonus would pass "Take items" at any usable bar.
pub fn is_button_line(text: &str, _g: &MercGeometry) -> bool {
    let lower: String = text
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if lower.is_empty() {
        return false;
    }
    lower == "take item" || lower == "rematch"
}

/// Best-effort header parse (D2 step 5). Every field is independently
/// optional: a missing one is `None`, never inferred from a neighbour.
fn parse_header(
    lines: &[OcrLineBox],
    first_row_centre: f32,
    rows: &[MercLayoutRow],
    column_x0: f32,
    row_pitch: f32,
) -> MercHeader {
    // Header lines sit above row 1 AND within the panel's width: the quest
    // tracker to the right of the window carries tall text of its own, and
    // it was read as the name (measured 2026-08-24). The panel's width is
    // the span from the skill column to the rightmost support cell, widened
    // by a few pitches for the header's own margins.
    let right = rows
        .iter()
        .flat_map(|r| r.cells.iter().map(|c| c[0] + c[2]))
        .max()
        .unwrap_or(column_x0 as i32) as f32;
    let margin = 4.0 * row_pitch;
    let above: Vec<&OcrLineBox> = lines
        .iter()
        .filter(|l| l.centre_y() < first_row_centre)
        .filter(|l| {
            let cx = l.x as f32 + l.w as f32 / 2.0;
            cx >= column_x0 - margin && cx <= right + margin
        })
        .collect();

    // "Lvl 83" — OCR reads the small-caps "Lvl" as `LVI`, `Lvi` or `LvI`
    // (measured 2026-08-24: `LVI 83`), so the l/I confusion is folded away.
    let level_line = above.iter().find(|l| {
        let head: String = l
            .text
            .trim()
            .to_lowercase()
            .chars()
            .take(3)
            .map(|c| if c == 'i' { 'l' } else { c })
            .collect();
        head == "lvl"
    });
    let level = level_line.and_then(|l| parse_trailing_number(&l.text)).map(|n| n as u32);

    // The class sits to the LEFT of the level on the same header line.
    let class = level_line.and_then(|lvl| {
        above
            .iter()
            .filter(|l| l.x < lvl.x && vertically_overlaps(l, lvl))
            .min_by_key(|l| lvl.x - l.x)
            .map(|l| l.text.trim().to_string())
    });

    let wager = above
        .iter()
        .find(|l| l.text.trim().to_lowercase().starts_with("wager"))
        .and_then(|l| parse_trailing_number(&l.text));

    // The title is the tallest line above the panel — it is set in a bigger
    // face than every other header field.
    // The "Should Recruit" verdict sits on the wager line in a face as tall
    // as the title, and OCR folds its tick icon into the text ("Should
    // Recruit@") — measured 2026-08-24. It is excluded by its leading word,
    // and trailing non-letters are cut off the winner for the same reason.
    let name = above
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .filter(|l| !l.text.trim().to_lowercase().starts_with("should "))
        .max_by_key(|l| l.h)
        .map(|l| {
            l.text
                .trim()
                .trim_end_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        });

    MercHeader { name, class, level, wager }
}

fn vertically_overlaps(a: &OcrLineBox, b: &OcrLineBox) -> bool {
    a.y < b.y + b.h && b.y < a.y + a.h
}

/// Digits after the label, with the thin spaces PoE groups thousands by
/// removed: `Wager: 1 028` → 1028, `Lvl 70` → 70. `None` when there are no
/// digits, so a failed read stays `None` instead of becoming 0.
fn parse_trailing_number(text: &str) -> Option<u64> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Whether a support slot holds an icon.
///
/// An empty slot is flat dark panel; an icon is not. The rule is the inner
/// region's grayscale standard deviation against
/// `thresholds.empty_cell_stddev`. Measured on the reference panel's 36 slots
/// (`tests/fixtures/merc-skills-panel.png`): occupied 42.7-60.9, empty
/// 1.1-2.0 — the default 18.0 sits in the middle of a 20× gap, so this is the
/// least fragile of the provisional constants.
///
/// A rect that falls outside the image is NOT occupied: a partial read of a
/// half-off-screen window must not invent a support.
pub fn occupied(img: &image::DynamicImage, rect: [i32; 4], g: &MercGeometry) -> bool {
    stddev(img, inner_rect(rect, g)).is_some_and(|sd| sd > g.thresholds.empty_cell_stddev)
}

/// A cell's inner region — the outer rect minus its frame, which is drawn
/// identically whether the slot is filled or not and would raise an empty
/// slot's stddev.
pub fn inner_rect(rect: [i32; 4], g: &MercGeometry) -> [i32; 4] {
    let inset = g.cell_inset.round() as i32;
    [
        rect[0] + inset,
        rect[1] + inset,
        (rect[2] - 2 * inset).max(1),
        (rect[3] - 2 * inset).max(1),
    ]
}

/// Grayscale standard deviation over a rect. `None` when the rect does not lie
/// wholly inside the image.
pub fn stddev(img: &image::DynamicImage, rect: [i32; 4]) -> Option<f32> {
    let [x, y, w, h] = rect;
    if w <= 0 || h <= 0 || x < 0 || y < 0 {
        return None;
    }
    let (iw, ih) = img.dimensions();
    if (x + w) as u32 > iw || (y + h) as u32 > ih {
        return None;
    }
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let n = (w as f64) * (h as f64);
    for py in y..y + h {
        for px in x..x + w {
            let p = img.get_pixel(px as u32, py as u32);
            let v = luma(p.0[0], p.0[1], p.0[2]) as f64;
            sum += v;
            sum_sq += v * v;
        }
    }
    let mean = sum / n;
    Some(((sum_sq / n) - mean * mean).max(0.0).sqrt() as f32)
}

/// ITU-R BT.601 luma (299/587/114).
///
/// NOT the same weighting as `image`'s `to_luma8`, which uses Rec.709
/// (2126/7152/722) — the two disagree by several levels on saturated colour,
/// which is most of a PoE icon. Every number measured against this function
/// (the occupancy stddevs, the badge ink floor) was measured with BT.601, so
/// switching to `to_luma8` here means re-deriving them, not just swapping a
/// call. `icons::normalize_cell` deliberately goes through `to_luma8` instead:
/// its output is normalized to zero mean and unit stddev, so the weighting
/// cancels out of the correlation.
pub fn luma(r: u8, gch: u8, b: u8) -> u8 {
    ((r as u32 * 299 + gch as u32 * 587 + b as u32 * 114) / 1000).min(255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mercenary::vocab::MercVocab;
    use image::{DynamicImage, Rgba, RgbaImage};

    fn vocab() -> MercVocab {
        MercVocab::load().expect("vocabulary parses")
    }

    fn line(text: &str, x: i32, centre_y: i32) -> OcrLineBox {
        // Height 16 and width 8·len are the reference panel's cap height and
        // rough advance — the tests care about the RECTS the geometry derives,
        // not about glyph metrics.
        OcrLineBox {
            text: text.to_string(),
            x,
            y: centre_y - 8,
            w: text.len() as i32 * 8,
            h: 16,
        }
    }

    /// The reference panel as the OCR would report it: the measured line
    /// centres of `scratchpad/recruit-cai.png` (full-image px), the wrapped
    /// fourth name as the two lines it really is, and the header lines above.
    /// Every geometry assertion below is derived from THESE inputs, never
    /// echoed from `MercGeometry`'s constants.
    fn reference_lines() -> Vec<OcrLineBox> {
        vec![
            line("Cai, the Lout", 285, 30),
            line("Shock Ambusher", 200, 73),
            line("Lvl 70", 385, 73),
            line("Dex / Int", 530, 73),
            line("Wager: 1 028", 80, 173),
            line("Conductivity", 134, 620),
            line("Vaal Lightning Trap", 134, 669),
            line("Lightning Spire Trap", 134, 717),
            line("Ball Lightning of Orbiting", 134, 757),
            line("Trap", 134, 775),
            line("Summon Skitterbots", 134, 814),
            line("Flame Dash", 134, 862),
        ]
    }

    fn tall(text: &str, x: i32, centre_y: i32, h: i32) -> OcrLineBox {
        OcrLineBox {
            text: text.to_string(),
            x,
            y: centre_y - h / 2,
            w: text.len() as i32 * 8,
            h,
        }
    }

    /// The whole of D2 steps 1-3 on the reference panel: seven lines in the
    /// column collapse to SIX rows because the wrapped name's two lines sit
    /// 18 px apart (inside 1.5 × 16), and the pitch is the median of the five
    /// inter-row gaps those six centres produce — 49, 48, 49, 48, 48 → 48.
    #[test]
    fn the_reference_panel_detects_six_rows_at_the_pitch_its_centres_imply() {
        let lines = reference_lines();
        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("panel detected");

        assert_eq!(layout.rows.len(), 6);
        assert_eq!(layout.row_pitch, 48.0);
        // scale = 48 / 49.3
        assert!(
            (layout.scale - 48.0 / MercGeometry::default().row_pitch).abs() < 1e-6,
            "scale was {}",
            layout.scale,
        );
        assert_eq!(layout.column_x0, 134);
    }

    /// The wrapped row is ONE row whose centre is the mean of its two lines
    /// (757, 775 → 766), and whose name rect spans both. Treating the
    /// continuation as its own row would shift every later row's cells by half
    /// a pitch and produce a seventh, cell-less row.
    #[test]
    fn a_wrapped_name_is_one_row_centred_between_its_two_lines() {
        let layout =
            detect(&reference_lines(), &MercGeometry::default(), &vocab()).expect("detected");

        let wrapped = &layout.rows[3];
        assert_eq!(wrapped.text, "Ball Lightning of Orbiting Trap");
        assert_eq!(wrapped.centre_y, 766.0);
        assert_eq!(wrapped.name_rect[1], 749, "top of the first line");
        assert_eq!(wrapped.name_rect[1] + wrapped.name_rect[3], 783, "bottom of the second");
    }

    /// Cells are measured from the COLUMN's x, not from the row's own x. A row
    /// whose first glyph carries a wider side bearing reports a larger x, and
    /// keying off it would walk that row's cells off the icons.
    #[test]
    fn a_single_row_with_a_shifted_x_still_gets_the_columns_cells() {
        let mut lines = reference_lines();
        // Nudge one row 2 px right — inside the column tolerance (0.15 × 16).
        for l in lines.iter_mut() {
            if l.text == "Flame Dash" {
                l.x += 2;
            }
        }

        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("detected");

        assert_eq!(layout.rows.len(), 6, "the nudged row must stay in the column");
        assert_eq!(
            layout.rows[5].cells[0][0], layout.rows[0].cells[0][0],
            "every row's slot 0 shares one x",
        );
    }

    /// Cell rects follow the D1 offsets scaled by the DERIVED scale, not by 1:
    /// slot 0 at column_x0 + 238·s, then one 49·s pitch per slot, sized 44·s,
    /// centred on the row. With s = 48/49.3 that is x0 = 134 + 232 = 366.
    #[test]
    fn cell_rects_are_the_reference_offsets_scaled_by_the_derived_scale() {
        let g = MercGeometry::default();
        let layout = detect(&reference_lines(), &g, &vocab()).expect("detected");
        let s = layout.scale;

        let row0 = &layout.rows[0];
        assert_eq!(row0.cells.len(), g.max_slots as usize);
        let expected_x0 = (134.0 + g.cell_offset_x * s).round() as i32;
        let expected_size = (g.cell_size * s).round() as i32;
        assert_eq!(row0.cells[0][0], expected_x0);
        assert_eq!(row0.cells[0][2], expected_size);
        assert_eq!(row0.cells[0][3], expected_size);
        // Slot spacing, asserted as a spacing rather than as a second
        // absolute: the origins accumulate in float and round once, so a
        // per-slot `round(pitch·s)` would differ by a px without either being
        // wrong. Within a px of 49·s is the real contract.
        for pair in row0.cells.windows(2) {
            let step = (pair[1][0] - pair[0][0]) as f32;
            assert!(
                (step - g.cell_pitch * s).abs() <= 1.0,
                "slot step {step} is not one 49·s pitch ({})",
                g.cell_pitch * s,
            );
        }
        assert_eq!(
            row0.cells[0][1] + expected_size / 2,
            row0.centre_y.round() as i32,
            "the cell is centred on the row",
        );
    }

    /// A line that merely shares the column's x is not part of the panel. This
    /// one sits 300 px below the last skill — far outside the row grid — and
    /// must neither become a seventh row nor drag the pitch median (which its
    /// 338 px gap would move from 48 to 48.5).
    #[test]
    fn a_left_aligned_line_far_below_the_panel_does_not_join_the_column() {
        let mut lines = reference_lines();
        lines.push(line("has entered the area", 134, 1200));

        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("detected");

        assert_eq!(layout.rows.len(), 6);
        assert_eq!(layout.row_pitch, 48.0);
    }

    /// …and one just above the first skill is out too. The header strip sits
    /// there, and an aligned header line becoming row 0 would shift every
    /// row's cells off its icons.
    #[test]
    fn a_left_aligned_line_far_above_the_panel_does_not_join_the_column() {
        let mut lines = reference_lines();
        lines.push(line("Inventory", 134, 400));

        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("detected");

        assert_eq!(layout.rows.len(), 6);
        assert_eq!(layout.rows[0].text, "Conductivity");
    }

    /// The bound must not cost the wrap it exists alongside: a continuation
    /// line BELOW the last skill name (the bottom row wrapping) is within one
    /// cluster gap of it and still joins its row.
    #[test]
    fn a_wrap_on_the_bottom_row_still_joins_its_row() {
        let mut lines = reference_lines();
        for l in lines.iter_mut() {
            if l.text == "Flame Dash" {
                l.text = "Ball Lightning of Orbiting".to_string();
            }
        }
        lines.push(line("Trap", 134, 880));

        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("detected");

        assert_eq!(layout.rows.len(), 6);
        assert_eq!(layout.rows[5].text, "Ball Lightning of Orbiting Trap");
    }

    /// The anchor is the discriminator. A gem tooltip or a character panel
    /// shows skill names in a column too; without a wager line the module must
    /// report nothing rather than capture the wrong window.
    #[test]
    fn a_skill_column_without_a_wager_line_is_not_a_recruit_window() {
        let lines: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Wager"))
            .collect();

        assert!(detect(&lines, &MercGeometry::default(), &vocab()).is_none());
    }

    /// Without a readable wager line, a recruit button under the rows anchors
    /// the panel instead — the 2026-08-24 Windows case, where OCR dropped the
    /// wager line entirely and read both buttons.
    #[test]
    fn a_recruit_button_below_the_rows_anchors_when_the_wager_line_is_missing() {
        let mut lines: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Wager"))
            .collect();
        // Last row centre is ~814 + 7; the buttons sit about one pitch below.
        lines.push(line("TAKE ITEM", 250, 880));
        lines.push(line("REMATCH", 420, 880));

        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("anchored by button");
        assert_eq!(layout.header.wager, None, "no wager line was read");
        assert!(layout.rows.len() >= 3);
    }

    /// A button line far below the panel (past the reach) is not an anchor,
    /// and a near-miss word is not a button.
    #[test]
    fn a_far_or_near_miss_button_line_is_not_an_anchor() {
        let g = MercGeometry::default();
        let mut lines: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Wager"))
            .collect();
        lines.push(line("REMATCH", 420, 814 + (20.0 * 48.0) as i32));
        assert!(detect(&lines, &g, &vocab()).is_none(), "out of reach");

        assert!(!is_button_line("Take items", &g));
        assert!(!is_button_line("Rematches", &g));
        assert!(is_button_line("take  item", &g));
        assert!(is_button_line("REMATCH", &g));
    }

    /// The 2026-08-24 Windows dump (1920×1200, merc-debug/1787604709231):
    /// the wager line is absent from OCR, both buttons are present, six rows.
    #[test]
    fn the_first_windows_dump_detects_by_the_recruit_buttons() {
        let lines = vec![
            OcrLineBox { text: "Nytra, the Cyaxan Loner".into(), x: 813, y: 84, w: 273, h: 26 },
            OcrLineBox { text: "Infamous Frosthand".into(), x: 775, y: 129, w: 164, h: 15 },
            OcrLineBox { text: "LVI 83".into(), x: 980, y: 129, w: 44, h: 16 },
            OcrLineBox { text: "NOCTURNAL HIDEOUT".into(), x: 306, y: 134, w: 192, h: 15 },
            OcrLineBox { text: "22:51".into(), x: 368, y: 1049, w: 37, h: 15 },
            OcrLineBox { text: "MENU".into(), x: 265, y: 1155, w: 55, h: 18 },
            OcrLineBox { text: "Int".into(), x: 1150, y: 131, w: 23, h: 13 },
            OcrLineBox { text: "Life".into(), x: 53, y: 901, w: 31, h: 17 },
            OcrLineBox { text: "2 5031?".into(), x: 118, y: 902, w: 60, h: 26 },
            OcrLineBox { text: "Shield 2229120229".into(), x: 52, y: 925, w: 159, h: 29 },
            OcrLineBox { text: "It wasn't people Nytra Cyaxan loathed, but the frailties they wore so".into(), x: 769, y: 161, w: 467, h: 17 },
            OcrLineBox { text: "proudly: need, artifice, expectation.".into(), x: 881, y: 180, w: 242, h: 17 },
            OcrLineBox { text: "Should Recruit".into(), x: 1073, y: 228, w: 135, h: 17 },
            OcrLineBox { text: "FROST BOMB".into(), x: 719, y: 678, w: 87, h: 13 },
            OcrLineBox { text: "FROSTBITE".into(), x: 719, y: 726, w: 69, h: 13 },
            OcrLineBox { text: "VORTEX".into(), x: 718, y: 775, w: 52, h: 13 },
            OcrLineBox { text: "EYE OF WINTER".into(), x: 719, y: 823, w: 103, h: 13 },
            OcrLineBox { text: "FLAME DASH".into(), x: 719, y: 871, w: 86, h: 13 },
            OcrLineBox { text: "DISCIPLINE".into(), x: 719, y: 920, w: 74, h: 13 },
            OcrLineBox { text: "28".into(), x: 1771, y: 73, w: 15, h: 13 },
            OcrLineBox { text: "0:03".into(), x: 1602, y: 80, w: 28, h: 12 },
            OcrLineBox { text: "0:05".into(), x: 1683, y: 84, w: 27, h: 12 },
            OcrLineBox { text: "KINGSÜARCH.PQOSPECTlNd(OPTlONAL)".into(), x: 1491, y: 329, w: 344, h: 24 },
            OcrLineBox { text: "for a reward".into(), x: 1647, y: 355, w: 100, h: 15 },
            OcrLineBox { text: "HREADS Of THE ORIGINATOR".into(), x: 1502, y: 379, w: 246, h: 17 },
            OcrLineBox { text: "Explore Memory Vaults in differentAtlas".into(), x: 1518, y: 400, w: 331, h: 19 },
            OcrLineBox { text: "Quadrapts (214)".into(), x: 1518, y: 421, w: 123, h: 21 },
            OcrLineBox { text: "9218010".into(), x: 1814, y: 902, w: 59, h: 21 },
            OcrLineBox { text: "Mana".into(), x: 1712, y: 903, w: 49, h: 15 },
            OcrLineBox { text: "Reserved".into(), x: 1712, y: 925, w: 80, h: 17 },
            OcrLineBox { text: "709".into(), x: 1837, y: 928, w: 31, h: 15 },
            OcrLineBox { text: "TAKE ITEM".into(), x: 830, y: 979, w: 87, h: 13 },
            OcrLineBox { text: "REMATCH".into(), x: 989, y: 979, w: 81, h: 13 },
        ];
        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("detected");
        assert_eq!(layout.rows.len(), 6);
        assert_eq!(layout.header.name.as_deref(), Some("Nytra, the Cyaxan Loner"));
        assert_eq!(layout.header.level, Some(83));
        assert_eq!(layout.header.class.as_deref(), Some("Infamous Frosthand"));
        // The quest tracker's "Speak to Johan for a reward" (x 1647) is
        // outside the panel and must not win the name.
        let mut lines = lines;
        lines.push(OcrLineBox { text: "SpeakrgVohÅn for a reward".into(), x: 1520, y: 350, w: 300, h: 30 });
        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("detected");
        assert_eq!(layout.header.name.as_deref(), Some("Nytra, the Cyaxan Loner"));
        assert!((layout.scale - 1.0).abs() < 0.05, "scale {}", layout.scale);
    }

    /// The verdict line is never the name, however tall OCR boxes it, and the
    /// tick icon OCR glues onto it must not survive as a trailing glyph.
    #[test]
    fn the_verdict_line_is_not_the_name_and_icon_glyphs_are_trimmed() {
        let mut lines = reference_lines();
        lines.push(OcrLineBox { text: "Should Recruit@".into(), x: 500, y: 165, w: 140, h: 40 });
        for l in lines.iter_mut() {
            if l.text.starts_with("Cai") {
                l.text = "Cai, the Lout@".into();
                l.h = 30;
            }
        }
        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("detected");
        assert_eq!(layout.header.name.as_deref(), Some("Cai, the Lout"));
    }

    /// The anchor must be ABOVE the rows and NEAR them. A wager line far up
    /// the screen (past 12 row pitches) belongs to some other surface.
    #[test]
    fn a_wager_line_out_of_reach_above_the_panel_is_not_an_anchor() {
        let mut lines = reference_lines();
        for l in lines.iter_mut() {
            if l.text.starts_with("Wager") {
                // 12 pitches above row 1 (620) is y = 620 - 576 = 44; put it
                // one pitch further still.
                l.y = 620 - (13.0 * 48.0) as i32;
            }
        }

        assert!(detect(&lines, &MercGeometry::default(), &vocab()).is_none());
    }

    /// A near-miss word is NOT the anchor. "Wagner has entered the area" is an
    /// ordinary PoE chat line whose first word scores 0.961 against "wager" —
    /// over D2's 0.85 bar, under the 0.98 this uses. Anchoring on it would
    /// hand the module a capture of whatever window happened to be open.
    #[test]
    fn a_chat_line_starting_with_a_near_miss_word_is_not_an_anchor() {
        let mut lines = reference_lines();
        for l in lines.iter_mut() {
            if l.text.starts_with("Wager") {
                l.text = "Wagner has entered the area".to_string();
            }
        }

        assert!(detect(&lines, &MercGeometry::default(), &vocab()).is_none());
    }

    /// The same for the plural and for the shorter root — 0.967 and 0.960,
    /// both over 0.85, both rejected here. Listed one by one so the failure
    /// message names the word that got through.
    #[test]
    fn other_near_miss_words_are_not_anchors() {
        for word in ["Wagers", "Wage", "Water", "Manager"] {
            let mut lines = reference_lines();
            for l in lines.iter_mut() {
                if l.text.starts_with("Wager") {
                    l.text = format!("{word}: 1 028");
                }
            }

            assert!(
                detect(&lines, &MercGeometry::default(), &vocab()).is_none(),
                "{word:?} must not anchor a capture",
            );
        }
    }

    /// …and the real label still anchors, whether or not the colon is read and
    /// whether or not a space separates it from the amount. Without this, any
    /// tight-enough threshold would pass the two tests above.
    #[test]
    fn the_real_wager_label_anchors_in_every_spelling_ocr_returns() {
        for label in ["Wager: 1 028", "Wager 1028", "Wager:1028", "WAGER: 1 028"] {
            let mut lines = reference_lines();
            for l in lines.iter_mut() {
                if l.text.starts_with("Wager") {
                    l.text = label.to_string();
                }
            }

            assert!(
                detect(&lines, &MercGeometry::default(), &vocab()).is_some(),
                "{label:?} must anchor a capture",
            );
        }
    }

    /// A wager line BELOW the first row is not the panel's header label.
    #[test]
    fn a_wager_line_below_the_first_row_is_not_an_anchor() {
        let mut lines = reference_lines();
        for l in lines.iter_mut() {
            if l.text.starts_with("Wager") {
                l.y = 900;
            }
        }

        assert!(detect(&lines, &MercGeometry::default(), &vocab()).is_none());
    }

    /// One skill name is not a panel — D2 needs two, so a stray gem name in a
    /// chat window cannot start a capture.
    #[test]
    fn a_single_skill_name_is_not_enough_to_detect_a_panel() {
        let lines = vec![line("Wager: 1 028", 80, 173), line("Conductivity", 134, 620)];

        assert!(detect(&lines, &MercGeometry::default(), &vocab()).is_none());
    }

    /// No skill names at all: the detector must not fall back to "any column".
    #[test]
    fn a_screen_with_no_skill_names_detects_nothing() {
        let lines = vec![
            line("Wager: 1 028", 80, 173),
            line("Inventory", 134, 620),
            line("Stash", 134, 669),
        ];

        assert!(detect(&lines, &MercGeometry::default(), &vocab()).is_none());
    }

    /// The single-row fallback: with one cluster there is no inter-row gap, so
    /// the scale comes from the line height instead. A 24 px line against the
    /// 16 px reference is a 1.5× UI.
    #[test]
    fn a_single_row_panel_falls_back_to_the_line_height_scale() {
        let lines = vec![
            tall("Wager: 1 028", 80, 200, 24),
            // Both lines of one wrapped name: two candidates, one cluster.
            tall("Ball Lightning of Orbiting", 134, 600, 24),
            tall("Ball Lightning of Orbiting Trap", 134, 624, 24),
        ];

        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("detected");

        assert_eq!(layout.rows.len(), 1);
        assert_eq!(layout.row_pitch, 0.0, "no pitch is measurable from one row");
        assert_eq!(layout.scale, 24.0 / MercGeometry::default().ref_line_height);
    }

    /// A UI at a different scale must be measured, not assumed: doubling every
    /// coordinate must double the reported scale and the cell size.
    #[test]
    fn a_2x_panel_reports_double_the_scale_and_double_the_cells() {
        let doubled: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .map(|l| OcrLineBox { x: l.x * 2, y: l.y * 2, w: l.w * 2, h: l.h * 2, ..l })
            .collect();
        let base = detect(&reference_lines(), &MercGeometry::default(), &vocab()).unwrap();

        let layout = detect(&doubled, &MercGeometry::default(), &vocab()).expect("detected");

        assert!((layout.scale - base.scale * 2.0).abs() < 1e-4, "scale {}", layout.scale);
        assert_eq!(layout.rows[0].cells[0][2], base.rows[0].cells[0][2] * 2);
    }

    /// The header is read from the lines above the panel, each field
    /// independently. `Wager: 1 028` carries a thousands space that must not
    /// truncate the number to 1.
    #[test]
    fn the_header_reads_name_class_level_and_a_spaced_wager() {
        let layout =
            detect(&reference_lines(), &MercGeometry::default(), &vocab()).expect("detected");

        assert_eq!(layout.header.level, Some(70));
        assert_eq!(layout.header.wager, Some(1028));
        assert_eq!(layout.header.class.as_deref(), Some("Shock Ambusher"));
    }

    /// The title is the TALLEST line above the panel — it is set in a larger
    /// face than the class/level strip, which is the only thing separating
    /// them.
    #[test]
    fn the_header_name_is_the_tallest_line_above_the_panel() {
        let mut lines = reference_lines();
        for l in lines.iter_mut() {
            if l.text == "Cai, the Lout" {
                l.h = 26;
                l.y = 30 - 13;
            }
        }

        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("detected");

        assert_eq!(layout.header.name.as_deref(), Some("Cai, the Lout"));
    }

    /// Header fields the panel does not show stay `None`. Guessing a level
    /// would put a number on the page that the game never displayed.
    #[test]
    fn missing_header_fields_stay_none() {
        let lines: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Lvl"))
            .collect();

        let layout = detect(&lines, &MercGeometry::default(), &vocab()).expect("detected");

        assert_eq!(layout.header.level, None);
        assert_eq!(layout.header.class, None, "the class is located BY the level line");
        assert_eq!(layout.header.wager, Some(1028), "other fields are unaffected");
    }

    // -- occupancy ---------------------------------------------------------

    /// Paint a rect with a checkerboard so its stddev is high, on a flat dark
    /// background. Mirrors what the panel really looks like: flat empty slots,
    /// busy icons.
    fn img_with_icon_at(rect: [i32; 4]) -> DynamicImage {
        let mut img = RgbaImage::from_pixel(200, 200, Rgba([17, 17, 17, 255]));
        for y in rect[1]..rect[1] + rect[3] {
            for x in rect[0]..rect[0] + rect[2] {
                let v = if (x + y) % 2 == 0 { 240 } else { 10 };
                img.put_pixel(x as u32, y as u32, Rgba([v, v, v, 255]));
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    /// A busy region is occupied; the identical rect over flat panel is not.
    /// One image, two rects — so the assertion is about the PIXELS, not about
    /// two differently built fixtures.
    #[test]
    fn a_busy_slot_is_occupied_and_a_flat_one_is_not() {
        let g = MercGeometry::default();
        let img = img_with_icon_at([10, 10, 44, 44]);

        assert!(occupied(&img, [10, 10, 44, 44], &g));
        assert!(!occupied(&img, [100, 100, 44, 44], &g));
    }

    /// The threshold is a field, not a literal: raising it past the icon's own
    /// stddev must flip the verdict, which is what makes the JSON override a
    /// real recalibration knob.
    #[test]
    fn raising_the_stddev_threshold_flips_an_occupied_slot_to_empty() {
        let mut g = MercGeometry::default();
        let img = img_with_icon_at([10, 10, 44, 44]);
        assert!(occupied(&img, [10, 10, 44, 44], &g), "precondition");

        g.thresholds.empty_cell_stddev = 200.0;

        assert!(!occupied(&img, [10, 10, 44, 44], &g));
    }

    /// A rect that runs off the image is not occupied. A half-off-screen
    /// recruit window must not invent supports out of missing pixels.
    #[test]
    fn a_rect_outside_the_image_is_not_occupied() {
        let g = MercGeometry::default();
        let img = img_with_icon_at([10, 10, 44, 44]);

        assert!(!occupied(&img, [180, 180, 44, 44], &g));
        assert!(!occupied(&img, [-5, 10, 44, 44], &g));
        assert!(stddev(&img, [180, 180, 44, 44]).is_none());
    }

    /// Occupancy reads the INNER region: the cell frame is drawn identically
    /// whether the slot is filled or not, so an empty slot with a bright frame
    /// must still read empty.
    #[test]
    fn a_bright_frame_around_a_flat_slot_does_not_make_it_occupied() {
        let g = MercGeometry::default();
        let mut img = RgbaImage::from_pixel(200, 200, Rgba([17, 17, 17, 255]));
        let rect = [10, 10, 44, 44];
        for y in rect[1]..rect[1] + rect[3] {
            for x in rect[0]..rect[0] + rect[2] {
                let edge = x < rect[0] + 2
                    || x >= rect[0] + rect[2] - 2
                    || y < rect[1] + 2
                    || y >= rect[1] + rect[3] - 2;
                if edge {
                    img.put_pixel(x as u32, y as u32, Rgba([255, 215, 120, 255]));
                }
            }
        }
        let img = DynamicImage::ImageRgba8(img);

        assert!(!occupied(&img, rect, &g), "the frame must be inset away");
    }
}
