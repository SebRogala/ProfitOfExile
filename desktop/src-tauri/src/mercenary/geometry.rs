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

/// Where a grabbed frame sits on the screen.
///
/// A detect frame is not always the whole screen: once the panel's rect is
/// known the loop OCRs a crop of it ([`crop_around`]), and Windows OCR
/// reports line boxes in the pixels it was handed — CROP-relative. Every rule
/// downstream of the OCR is screen-absolute: [`panel_anchor`]
/// weighs rows against the last known panel rect, [`panel_bounds`] and
/// [`header_guard_bounds`] feed `run.rs`'s cursor tests, and the cell rects
/// end up in `MercCapture` where the hover tick hit-tests them against the
/// real cursor. Mixing the two spaces would not fail loudly: it would read as
/// "the panel moved", every frame, for ever.
///
/// So this type is the ONE seam between them. [`Self::to_screen`] moves an
/// OCR box out of the frame the moment it leaves the OCR call, and
/// [`Self::local`] moves a screen rect back in for the one thing that still
/// indexes the image — the pixel reads in `read::build_capture`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    /// The grabbed image's top-left corner in screen px. `(0, 0)` for a full
    /// grab, which is what makes the full path arithmetically identical to the
    /// one that existed before crops.
    origin: (i32, i32),
    /// The WHOLE screen's size, never the image's. `read::build_capture`
    /// publishes it as `MercCapture::screen`, and `run::hover_region` clamps
    /// the tooltip crop to it — a crop's own dimensions there would clamp the
    /// hover to the panel.
    screen: [u32; 2],
    kind: FrameKind,
}

/// Which of the three grabs a [`Frame`] describes.
///
/// A LABEL for the log and the one bit `to_screen` branches on, in one field
/// rather than two booleans: the probe band (POE-204 WI-C) is a crop like the
/// re-detect's, so a `cropped: bool` would have made the two indistinguishable
/// in the log at the exact moment the reader needs to tell a 500 ms anchor look
/// from a re-read of a known panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// The whole screen.
    Full,
    /// A crop of a KNOWN panel, re-read on the live cadence.
    Crop,
    /// The voice-line gate's anchor band — see [`probe_band_bounds`].
    Probe,
}

impl Frame {
    /// A frame that IS the screen.
    pub fn full(screen: [u32; 2]) -> Self {
        Self { origin: (0, 0), screen, kind: FrameKind::Full }
    }

    /// A frame cropped out of the screen at `origin`.
    pub fn cropped(origin: (i32, i32), screen: [u32; 2]) -> Self {
        Self { origin, screen, kind: FrameKind::Crop }
    }

    /// The gate's anchor band, cut out of the screen at `origin`.
    pub fn probe(origin: (i32, i32), screen: [u32; 2]) -> Self {
        Self { origin, screen, kind: FrameKind::Probe }
    }

    /// The whole screen's size in px.
    pub fn screen(&self) -> [u32; 2] {
        self.screen
    }

    /// `full`, `crop` or `probe`, for the log.
    pub fn describe(&self) -> &'static str {
        match self.kind {
            FrameKind::Full => "full",
            FrameKind::Crop => "crop",
            FrameKind::Probe => "probe",
        }
    }

    /// OCR boxes as the engine returned them (frame-local) moved into screen
    /// coordinates. Called on the line vector the instant it comes back, so
    /// nothing downstream ever sees a frame-local box.
    pub fn to_screen(&self, mut lines: Vec<OcrLineBox>) -> Vec<OcrLineBox> {
        if self.kind == FrameKind::Full {
            return lines;
        }
        for line in &mut lines {
            line.x += self.origin.0;
            line.y += self.origin.1;
        }
        lines
    }

    /// A screen rect in this frame's own pixels — the inverse of
    /// [`Self::to_screen`], for the pixel reads that still index the image.
    pub fn local(&self, rect: [i32; 4]) -> [i32; 4] {
        [rect[0] - self.origin.0, rect[1] - self.origin.1, rect[2], rect[3]]
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
///
/// `known_panel` is the rect the LAST detect of the capture still on screen
/// produced ([`panel_bounds`]), and it is a third anchor — see
/// [`panel_anchor`]. `None` means there is no live capture, and
/// then a frame anchors on its own chrome or not at all.
pub fn detect(
    lines: &[OcrLineBox],
    g: &MercGeometry,
    vocab: &MercVocab,
    known_panel: Option<[i32; 4]>,
) -> Option<MercLayout> {
    detect_reason(lines, g, vocab, known_panel).ok()
}

/// [`detect`], with the reason a miss missed.
///
/// A miss is the failure mode that costs a capture: two of them retire the
/// window, and the log line the loop prints for one ("looked, no recruit
/// window") says only how many lines and candidates were read. That was not
/// enough to tell a panel that had closed from a partial read that lost its
/// anchor (app.log 2026-08-26 16:08:25 → 16:08:28, the window still open at
/// 16:08:37), so the stage that returned `None` is reported instead of
/// discarded. `run.rs` prints it under debug mode; nothing branches on it.
pub fn detect_reason(
    lines: &[OcrLineBox],
    g: &MercGeometry,
    vocab: &MercVocab,
    known_panel: Option<[i32; 4]>,
) -> Result<MercLayout, DetectMiss> {
    // 1. Skill-name candidates seed the column.
    let candidates: Vec<&OcrLineBox> = lines
        .iter()
        .filter(|l| {
            let read = vocab.match_skill(&l.text, &g.thresholds);
            read.state == ReadState::Matched || read.state == ReadState::LowConfidence
        })
        .collect();
    if candidates.len() < g.min_skill_candidates {
        return Err(DetectMiss {
            candidates: candidates.len(),
            column_x0: None,
            stage: DetectStage::TooFewCandidates { needed: g.min_skill_candidates },
        });
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
        return Err(DetectMiss {
            candidates: candidates.len(),
            column_x0: Some(column_x0),
            stage: DetectStage::NoRowClusters,
        });
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
        return Err(DetectMiss {
            candidates: candidates.len(),
            column_x0: Some(column_x0),
            stage: DetectStage::BadScale { scale },
        });
    }

    // 4. The panel anchor, checked once the pitch is known: a line above row 1
    //    within `wager_search_pitches` of it reading "Wager", or a button line
    //    below the last row within the same reach — unless the rows are sitting
    //    in a panel we already found, which is an anchor in its own right.
    let first_centre = centres[0];
    let last_centre = centres[centres.len() - 1];
    let panel = panel_anchor(known_panel, &centres, column_x0, g, scale);
    if panel != PanelAnchor::Anchored {
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
        if anchor.is_none() {
            return Err(DetectMiss {
                candidates: candidates.len(),
                column_x0: Some(column_x0),
                stage: DetectStage::NoAnchor { rows: centres.len(), panel },
            });
        }
    }

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

    Ok(MercLayout {
        scale,
        column_x0: column_x0.round() as i32,
        row_pitch,
        header: parse_header(lines, first_centre, &rows, column_x0, row_pitch.max(g.row_pitch * scale)),
        rows,
    })
}

/// Why [`detect_reason`] found no layout on a frame.
///
/// Diagnostic only — every field is here to be printed. The three numbers are
/// the ones that separate the failure modes the smoke log could not tell
/// apart: how many skill names the frame read at all, where their column was,
/// and which step threw the frame away.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectMiss {
    /// Skill-name candidates the frame produced — the seed set of step 1.
    pub candidates: usize,
    /// The candidates' median left edge, once there were enough to take one.
    pub column_x0: Option<f32>,
    /// The step that returned.
    pub stage: DetectStage,
}

/// The step of [`detect_reason`] that gave up.
#[derive(Debug, Clone, PartialEq)]
pub enum DetectStage {
    /// Step 1: fewer skill names than `min_skill_candidates`. The ordinary
    /// shape of "no recruit window on screen".
    TooFewCandidates { needed: usize },
    /// Step 2: the column clustered to nothing, which the candidate filter
    /// makes unreachable in practice.
    NoRowClusters,
    /// Step 3: the pitch or the line height gave a scale that is not a
    /// positive finite number.
    BadScale { scale: f32 },
    /// Step 4: rows were read and nothing anchored them — no wager line, no
    /// button line, and `panel` says why the known-panel anchor abstained.
    /// This is the shape a tooltip over the footer produces.
    NoAnchor { rows: usize, panel: PanelAnchor },
}

impl std::fmt::Display for DetectMiss {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} candidate(s)", self.candidates)?;
        match self.column_x0 {
            Some(x) => write!(f, " at column x0 {x:.0}")?,
            None => write!(f, ", no column")?,
        }
        write!(f, " — ")?;
        match &self.stage {
            DetectStage::TooFewCandidates { needed } => write!(f, "fewer than the {needed} needed"),
            DetectStage::NoRowClusters => write!(f, "the column clustered to no rows"),
            DetectStage::BadScale { scale } => write!(f, "unusable scale {scale}"),
            DetectStage::NoAnchor { rows, panel } => {
                write!(f, "{rows} row(s), no wager or button line, known panel: {panel}")
            }
        }
    }
}

/// What the known-panel anchor made of this frame's rows — [`panel_anchor`]'s
/// answer, and the sub-predicate that said no when it said no.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelAnchor {
    /// Every row centre is inside the known rect and the column is where that
    /// rect's column was. The frame is anchored.
    Anchored,
    /// No live capture, so no rect to weigh anything against.
    NoKnownRect,
    /// A rect, but no rows to place in it.
    NoRows,
    /// The skill column is not where the known rect's column was, by more than
    /// half a cell — a different panel, or the same one moved.
    ColumnMoved { column_x: i32, expected_x: i32, tolerance: i32 },
    /// A row centre falls outside the known rect. The all-quantifier: one is
    /// enough, and this is the one.
    RowOutside { centre: i32 },
}

impl std::fmt::Display for PanelAnchor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PanelAnchor::Anchored => write!(f, "anchored"),
            PanelAnchor::NoKnownRect => write!(f, "none known"),
            PanelAnchor::NoRows => write!(f, "no rows"),
            PanelAnchor::ColumnMoved { column_x, expected_x, tolerance } => write!(
                f,
                "column at {column_x}, expected {expected_x} ±{tolerance}"
            ),
            PanelAnchor::RowOutside { centre } => write!(f, "row centre {centre} outside it"),
        }
    }
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

/// Whether this frame's clustered rows sit inside the panel rect the last
/// detects produced ([`union_rect`]) — the third anchor, and the only one
/// available on a frame the game has drawn a tooltip over — and, when they do
/// not, which of the two sub-predicates said no.
///
/// MEASURED 2026-08-26 (app.log 09:14:51, 09:41:52): with the recruit window
/// plainly on screen, `detect` returned `None` on frames carrying 12 and 6
/// skill candidates. The rows were read; the ANCHOR was not. The anchor is one
/// line — the wager (which OCR drops outright, see above) or an exact-equality
/// button label — so a tooltip drawn over the footer, or one glyph error in
/// `TAKE ITEM`, deletes it and the whole capture retires two ticks later.
///
/// A tooltip can delete a text line. It CANNOT move the skill rows: they are
/// where the panel is, and the panel does not slide while it is open. So rows
/// landing in the rect the panel was last measured at is positive evidence of
/// the same panel, of exactly the kind the missing chrome line was standing in
/// for.
///
/// The rect is only ever `Some` while a capture is LIVE — `run.rs`'s retire
/// clears `session.panel`, and so does a REPLACED panel — so this cannot
/// resurrect a window that closed: a closed window's rows are not on screen to
/// land anywhere.
///
/// Two things must hold, and they constrain different axes.
///
/// EVERY row centre must be inside the rect, not merely one: a genuinely new
/// panel that overlaps the old one's footprint by a row is not the old panel.
///
/// The all-quantifier does NOT, on its own, catch a window that MOVED — the
/// claim this doc used to make. [`contains`] tests the column x against the
/// rect's whole width, and the rect is as wide as the grid (~570 px at the
/// reference scale), so a panel dragged 200 px sideways keeps every row
/// "inside" and would inherit the old rect's identity. What the all-quantifier
/// actually pins is the VERTICAL span. The horizontal one needs its own test:
/// the skill column has to be where the old panel's column was, within half a
/// cell.
///
/// `rect[0] + margin` reconstructs the `column_x0` the rect was built from (see
/// [`panel_bounds`]) — exactly, except for a panel far enough left that the
/// rect's `.max(0)` clamp bit, which is worth at most [`PANEL_MARGIN_CELLS`] of
/// error against a tolerance of half a cell.
pub(super) fn panel_anchor(
    rect: Option<[i32; 4]>,
    centres: &[f32],
    column_x0: f32,
    g: &MercGeometry,
    scale: f32,
) -> PanelAnchor {
    let Some(rect) = rect else {
        return PanelAnchor::NoKnownRect;
    };
    if centres.is_empty() {
        return PanelAnchor::NoRows;
    }
    let x = column_x0.round() as i32;
    let margin = (g.cell_size * scale * PANEL_MARGIN_CELLS).round() as i32;
    let tolerance = column_tolerance(g, scale);
    let expected_x = rect[0] + margin;
    if (x - expected_x).abs() > tolerance {
        return PanelAnchor::ColumnMoved { column_x: x, expected_x, tolerance };
    }
    match centres
        .iter()
        .find(|&&centre| !contains(rect, (x, centre.round() as i32)))
    {
        Some(&centre) => PanelAnchor::RowOutside { centre: centre.round() as i32 },
        None => PanelAnchor::Anchored,
    }
}

/// The two rects a live capture's panel has been measured at, as one.
///
/// GROW-ONLY, and that is the whole point (app.log 2026-08-26 16:08:25). A
/// tooltip over the lower rows leaves a two-row layout, whose
/// [`panel_bounds`] is two rows tall. Writing that over the six-row rect the
/// same window produced a tick earlier makes [`panel_anchor`]'s
/// all-quantifier reject the NEXT full read — six centres, a rect that holds
/// two — and with the chrome still hidden the frame has no anchor left at all.
/// Both the crop and the full retake come back empty and the capture retires
/// with the window plainly on screen.
///
/// The union cannot creep across windows: `run.rs` clears the rect on retire
/// and on a REPLACED panel, so everything unioned here is one window's own
/// measurements. Within one window the union grows almost entirely downward —
/// the left edge is `column_x0 - margin` and the column does not move while
/// the panel is open, which is what keeps [`panel_anchor`]'s reconstruction of
/// `column_x0` from the rect exact.
pub fn union_rect(a: Option<[i32; 4]>, b: Option<[i32; 4]>) -> Option<[i32; 4]> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let x = a[0].min(b[0]);
            let y = a[1].min(b[1]);
            let right = (a[0] + a[2]).max(b[0] + b[2]);
            let bottom = (a[1] + a[3]).max(b[1] + b[3]);
            Some([x, y, right - x, bottom - y])
        }
        (some, None) | (None, some) => some,
    }
}

/// Half a cell at this capture's scale: how far the skill column may sit from
/// where a remembered rect says it should be and still be the same panel.
///
/// ONE definition, because two consumers ask the same question of the same
/// number and a drift between them would be silent. [`panel_anchor`] uses it to
/// decide whether a frame's rows belong to the remembered rect; [`next_panel`]
/// uses it to decide whether the remembered rect may be GROWN by this frame's
/// measurement at all. A tolerance that admitted a frame for the first and
/// rejected it for the second would leave the loop anchoring on a rect it
/// refuses to update.
pub(super) fn column_tolerance(g: &MercGeometry, scale: f32) -> i32 {
    (g.cell_size * scale / 2.0).round().max(1.0) as i32
}

/// The panel rect the loop holds after a detect: what it remembered and what
/// this frame measured, folded per the rule the two of them are evidence for.
///
/// Three outcomes, and the middle one is why the call site is not [`union_rect`]
/// on its own:
///
/// - **`replaced`** — the header fold says a DIFFERENT mercenary is behind a
///   panel that looks the same (a REMATCH). The grown rect belongs to the
///   window that is gone, and carrying a retired mercenary's footprint onto the
///   one that replaced it is the same inheritance the confirmations are dropped
///   for. This frame's measurement, alone.
/// - **the column MOVED** — the two rects disagree about the skill column's x
///   by more than [`column_tolerance`], so the panel was dragged. A union here
///   would be actively harmful rather than merely stale: the hull's left edge
///   would stay the OLD column's, and [`panel_anchor`] reconstructs its
///   `expected_x` from `rect[0]` — so every later frame of the moved panel
///   would be measured against the position the panel left, for the whole life
///   of the capture, since nothing else ever narrows the rect. This frame's
///   measurement, alone.
/// - **otherwise** — [`union_rect`], the grow-only rule a partial read under a
///   tooltip needs.
///
/// `remembered[0]` and `fresh[0]` are both `column_x0 - margin` at the same
/// scale (see [`bounds`]), so their difference IS the column's, except for a
/// panel far enough left that the `.max(0)` clamp bit.
pub fn next_panel(
    remembered: Option<[i32; 4]>,
    fresh: Option<[i32; 4]>,
    replaced: bool,
    column_tolerance: i32,
) -> Option<[i32; 4]> {
    if replaced {
        return fresh;
    }
    match (remembered, fresh) {
        (Some(held), Some(measured)) if (measured[0] - held[0]).abs() > column_tolerance => {
            Some(measured)
        }
        (held, measured) => union_rect(held, measured),
    }
}

/// How far past the skill column and the last cell the panel rect reaches, in
/// cell widths. Half a cell at the reference scale is ~22 px — enough to cover
/// the panel's frame either side of the grid without claiming screen the panel
/// does not own.
const PANEL_MARGIN_CELLS: f32 = 0.5;

/// How far below the last row's cells the panel rect reaches, in row pitches.
///
/// The recruit window's footer — TAKE ITEM and REMATCH — sits under the last
/// row, and it is the ONE part of the panel the player is guaranteed to put
/// the cursor on: it is what closes the window. One pitch (the old value)
/// stopped short of the buttons on the reference panel, so a cursor on TAKE
/// ITEM read as OUTSIDE the panel, and a detect that lost its anchor to the
/// button's own tooltip counted as a MISS instead of an occlusion — two of
/// those retire the capture with the window still open (app.log 2026-08-26
/// 09:14:51 → 09:14:54).
///
/// Three pitches clears the footer on the 2026-08-24 Windows dump (last row
/// centre 926, button baseline 985, pitch ~48.6) with room for the frame under
/// it. The over-reach it buys — a band of dead screen below a window that
/// really did close — costs at most `run.rs`'s `OCCLUDED_MAX` of held capture,
/// which is the cap that exists for exactly this trade.
const PANEL_FOOTER_PITCHES: f32 = 3.0;

/// Whether `p` lies inside `rect` (`[x, y, w, h]`), right/bottom exclusive.
pub fn contains(rect: [i32; 4], p: (i32, i32)) -> bool {
    let [x, y, w, h] = rect;
    p.0 >= x && p.0 < x + w && p.1 >= y && p.1 < y + h
}

/// The screen rect the recruit panel occupies, from a detected layout.
///
/// Its one consumer is the occlusion rule (`run.rs`'s `miss_kind`): a detect
/// that found nothing while the cursor was inside this rect is a tooltip drawn
/// OVER the panel, not a window that closed. `None` for a layout with no rows,
/// which [`detect`] never produces.
///
/// This is NOT the rect the header-withholding rule keys on — see
/// [`header_guard_bounds`] for why the two questions need different bottoms.
///
/// Horizontally the rect spans the skill column's left edge to the rightmost
/// candidate cell — ALL slots, occupied or not, because the panel is as wide as
/// its grid whether or not the mercenary filled it — plus
/// [`PANEL_MARGIN_CELLS`] either side, scaled with the capture.
///
/// Vertically it runs one row pitch above the first row to
/// [`PANEL_FOOTER_PITCHES`] below the last. It still UNDER-reaches upward — the
/// wager line can sit up to `wager_search_pitches` (12) above row 1 — and the
/// asymmetry is deliberate. This rect is evidence that the cursor is over the
/// panel, and the two errors cost differently: under-reaching costs a tolerated
/// miss on a cursor parked in the chrome, while over-reaching holds a dead
/// capture alive for a cursor resting in the band where the panel used to be.
/// `run.rs`'s `OCCLUDED_MAX` (15 s) is what bounds the over-reach downward, and
/// the footer is where the cursor demonstrably IS — see
/// [`PANEL_FOOTER_PITCHES`].
pub fn panel_bounds(layout: &MercLayout, g: &MercGeometry) -> Option<[i32; 4]> {
    bounds(layout, g, PANEL_FOOTER_PITCHES)
}

/// How far below the last row's cells the HEADER-guard rect reaches, in row
/// pitches — the one-pitch bottom [`panel_bounds`] had before the footer
/// extension, kept here because the header rule never wanted the footer.
const HEADER_GUARD_FOOTER_PITCHES: f32 = 1.0;

/// The rect the header-withholding rule keys on (`run.rs`'s
/// `publishable_header`): the grid with one row pitch of chrome above and
/// below, and NOT the footer.
///
/// The occlusion rect and this one answer different questions, and giving both
/// to [`panel_bounds`] would silently answer the second with the first's shape:
///
/// - Occlusion asks *could the game have drawn something over the panel?* That
///   has to include the footer. TAKE ITEM and REMATCH are where the cursor
///   demonstrably is, and the tooltip they open is what costs the frame its
///   anchor — the whole reason [`PANEL_FOOTER_PITCHES`] is 3.
/// - Withholding asks *could a tooltip have put lines in the HEADER BAND,
///   above row 0, where `parse_header` looks?* The game draws a tooltip at the
///   cursor. A cursor three pitches below the LAST row is most of a panel's
///   height away from the header band, and nothing drawn there reaches it.
///
/// Keying the header rule on the footer-extended rect would therefore throw
/// away every clean header read taken while the player's cursor rests on TAKE
/// ITEM — which is precisely when the name is wanted, because that click is
/// what ends the window. One pitch below the last row is the band inside which
/// a tooltip is close enough to the header to be a plausible source of its
/// lines.
///
/// `None` for a layout with no rows, exactly as [`panel_bounds`].
pub fn header_guard_bounds(layout: &MercLayout, g: &MercGeometry) -> Option<[i32; 4]> {
    bounds(layout, g, HEADER_GUARD_FOOTER_PITCHES)
}

/// How far ABOVE the panel rect a cropped re-detect reaches, in row pitches.
///
/// The header band is far taller than the panel rect's own one-pitch top
/// margin, and a crop that clips it would silently stop the loop ever reading
/// a name — the capture would never complete, the cadence would never settle,
/// and no trade session would open. MEASURED on `scratchpad/recruit-cai.png`,
/// the reference every geometry test is built from: the title `Cai, the Lout`
/// is centred at y 30, the class/level line at 73, the wager at 173 and row 1
/// at 620, with a pitch of 48. The title is 12.3 pitches above row 1 and
/// [`bounds`] already spends one of them, so 13 covers the whole band with a
/// pitch of slack.
const CROP_HEADER_PITCHES: f32 = 13.0;

/// How far to either SIDE of the panel rect a cropped re-detect reaches, in
/// row pitches.
///
/// Not a guess: it is [`parse_header`]'s own x-window. That filter keeps a
/// header line whose CENTRE is within four pitches of the grid — the wager on
/// the reference panel starts at x 80, left of the grid's own left edge — so a
/// narrower crop would hand `parse_header` a smaller candidate set than the
/// full-screen path had, which is a behaviour change hiding inside an
/// optimisation. The residue is a line admissible by its centre that extends
/// more than four pitches past the grid; nothing on the reference panel does.
const CROP_SIDE_PITCHES: f32 = 4.0;

/// The rect a re-detect of a KNOWN panel grabs and OCRs: the panel rect (with
/// its footer) plus the header band above and [`parse_header`]'s x-window
/// either side, clamped to the screen.
///
/// The point is cost. A full-screen OCR of a 1920×1200 desktop is the tick's
/// dominant expense (app.log 2026-08-26 09:40:06: 4504 ms for a tick built out
/// of two of them), and once the panel has been found the answer to "is it
/// still there, and what does it say" lives inside this rect. The frame is
/// still translated back to screen coordinates before any rule reads it — see
/// [`Frame`] — so the known-panel anchor, the column-x test and the cell rects
/// all keep meaning exactly what they meant on a full frame.
///
/// **Takes the RECT, not the layout, and that is the contract.** The rect the
/// loop holds is [`next_panel`]'s, which a partial read under a tooltip grows
/// rather than shrinks; a layout's own [`panel_bounds`] is only what THAT frame
/// could see. Built from the layout, a two-row read under a tooltip installed a
/// two-row crop, and the next full read was handed a frame cropped out of the
/// four rows it was expected to find. Built from the held rect, the crop
/// encloses it — and the header band above it — by construction, because every
/// reach below is outward.
pub fn crop_around(panel: [i32; 4], pitch: f32, screen: [u32; 2]) -> [i32; 4] {
    let [x, y, w, h] = panel;
    let side = (pitch * CROP_SIDE_PITCHES).round() as i32;
    let above = (pitch * CROP_HEADER_PITCHES).round() as i32;

    let x0 = (x - side).max(0);
    let y0 = (y - above).max(0);
    let x1 = (x + w + side).min(screen[0] as i32);
    let y1 = (y + h).min(screen[1] as i32);
    [x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)]
}

/// The pitch every rect construction measures its reaches in: the observed one,
/// or the reference pitch at this capture's scale when there is none.
///
/// [`detect`] reports a pitch of 0.0 for a single-row layout — there is no
/// inter-row gap to measure — and a reach of zero px would collapse whichever
/// rect used it.
pub fn effective_pitch(layout: &MercLayout, g: &MercGeometry) -> f32 {
    if layout.row_pitch > 0.0 {
        layout.row_pitch
    } else {
        g.row_pitch * layout.scale
    }
}

/// How far outside the panel rect the PROBE band reaches, in row pitches.
///
/// The band's job is to find the footer buttons again on a window the loop is
/// no longer tracking, so it is sized against the thing that can have moved:
/// nothing, in the ordinary case (the panel opens where it opened last), and a
/// few px of UI jitter otherwise. One pitch either way is slack for that
/// without turning the band back into a screen.
const PROBE_BAND_SLACK_PITCHES: f32 = 1.0;

/// The band a [`crate::mercenary::trigger`] probe OCRs when the panel's
/// geometry is known: the last row and the whole footer strip under it, the
/// panel's own width, one pitch of slack on every side, clamped to the screen.
///
/// This is NOT [`crop_around`]. That rect answers "re-read the panel I
/// am already holding" and reaches thirteen pitches UP for the header band,
/// because the answer it owes includes the mercenary's name. The probe owes one
/// bit — is the recruit window's chrome on screen at all — and the chrome that
/// answers it is [`is_button_line`]'s TAKE ITEM / REMATCH, which lives under
/// the grid. Everything above the last row is cost with no bearing on that bit.
///
/// MEASURED on the 2026-08-24 Windows dump (1920×1200, six rows, scale 0.974):
/// the panel rect is `[698, 615, 555, 477]`, the grid bottom is y 948, and both
/// buttons OCR at y 979. The band this produces is `[650, 900, 651, 240]` —
/// 6.8% of the screen's pixels, with the buttons 79 px inside its top edge and
/// 148 px inside its bottom one.
///
/// The skill column is inside it by construction: the band spans the panel
/// rect's width, and that rect starts [`PANEL_MARGIN_CELLS`] LEFT of
/// `column_x0`. A probe that saw the buttons but not the column would still
/// accept — the column is not part of the accept test — but the band carries it
/// so the full detect the probe hands its frame to has the same evidence.
///
/// `None` for a layout with no rows, exactly as [`panel_bounds`].
pub fn probe_band_bounds(
    layout: &MercLayout,
    g: &MercGeometry,
    screen: [u32; 2],
) -> Option<[i32; 4]> {
    let panel = panel_bounds(layout, g)?;
    // Footer reach 0: the same rect construction stopped at the grid's own
    // bottom edge, which is where the footer strip starts.
    let grid = bounds(layout, g, 0.0)?;
    let pitch = if layout.row_pitch > 0.0 {
        layout.row_pitch
    } else {
        g.row_pitch * layout.scale
    };
    let slack = (pitch * PROBE_BAND_SLACK_PITCHES).round() as i32;

    let x0 = (panel[0] - slack).max(0);
    let y0 = (grid[1] + grid[3] - slack).max(0);
    let x1 = (panel[0] + panel[2] + slack).min(screen[0] as i32);
    let y1 = (panel[1] + panel[3] + slack).min(screen[1] as i32);
    Some([x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)])
}

/// Where the DEFAULT probe band starts, as a fraction of the screen's height.
///
/// The fallback for the first recruit window of a session, when no panel has
/// ever been measured and there is no pitch to build a band out of.
///
/// MEASURED, once: the 2026-08-24 Windows dump, 1920×1200, one machine, one
/// resolution. Both footer buttons sit at y 979, which is 0.816 of the height.
/// That is the whole of the evidence.
///
/// **How the panel is ANCHORED is not measured.** Whether 0.816 holds at
/// another resolution, another aspect ratio or another UI scale is unknown, and
/// one dump cannot say. An earlier version of this comment claimed PoE centres
/// the recruit window and derived the fraction's stability from that; the same
/// dump contradicts it — [`panel_bounds`] of that layout is
/// `[698, 615, 555, 477]`, whose vertical centre is y 853 on a 1200-high
/// screen, which is not the middle of anything.
///
/// So the fraction is SLACK against an unknown rather than a derivation. 0.45
/// leaves 0.366 H — 439 px at 1200 — above the measured footer, and the whole
/// of the screen below it. It is only ever the FIRST look of a session: once a
/// panel has been seen, every probe uses the band measured off it
/// ([`probe_band_bounds`]), and the cost of this one being wrong is a single
/// stand-down on a window Scan now can still capture.
///
/// FULL width, deliberately. The horizontal position has the same one dump
/// behind it and the same unknown anchoring, and height is where the saving is:
/// a little over half the pixels of a full-screen OCR, for a look that only has
/// to answer whether the chrome is there.
const DEFAULT_PROBE_BAND_TOP_FRAC: f32 = 0.45;

/// The probe band for a session that has never seen a panel. See
/// [`DEFAULT_PROBE_BAND_TOP_FRAC`].
pub fn default_probe_band(screen: [u32; 2]) -> [i32; 4] {
    let h = screen[1] as f32;
    let y0 = (h * DEFAULT_PROBE_BAND_TOP_FRAC).round().max(0.0) as i32;
    [0, y0, (screen[0] as i32).max(1), (screen[1] as i32 - y0).max(1)]
}

/// Whether an anchor-band OCR saw the recruit window's chrome.
///
/// The probe's whole verdict, and deliberately a predicate [`detect`]'s own
/// anchor step also accepts — a probe that accepted on something the detect
/// does not would hand its frame to a detect that then found nothing, which is
/// a stand-down dressed up as a hit.
///
/// No positional test. [`detect`] pairs its anchor with the rows (above row 1,
/// or below the last row, within `wager_search_pitches`); the probe has no rows
/// to pair anything with — finding them is the full detect's job, and the band
/// is the position test. What is left is the text, and [`is_button_line`] is
/// already tight: exact equality after normalisation.
///
/// **[`detect`]'s OTHER anchor, the wager line, is deliberately not here.** It
/// sits ABOVE the grid, and both bands a probe can read start at or below it:
/// [`probe_band_bounds`] begins at the grid's bottom edge, and
/// [`default_probe_band`] begins at [`DEFAULT_PROBE_BAND_TOP_FRAC`] of the
/// height against a wager measured at 0.144 H. No probe frame can hold the
/// line, so testing for it is a predicate per OCR line that can only ever
/// answer false — and one a later reader would take as evidence that the band
/// reaches further up than it does.
pub fn probe_hit(lines: &[OcrLineBox], g: &MercGeometry) -> bool {
    lines.iter().any(|l| is_button_line(&l.text, g))
}

/// Whether `outer` fully contains `inner` (`[x, y, w, h]`).
pub fn encloses(outer: [i32; 4], inner: [i32; 4]) -> bool {
    inner[0] >= outer[0]
        && inner[1] >= outer[1]
        && inner[0] + inner[2] <= outer[0] + outer[2]
        && inner[1] + inner[3] <= outer[1] + outer[3]
}

/// Whether a cropped detect has to be re-taken on the full screen before its
/// result is believed.
///
/// `crop` is the rect the frame was cut from, `None` when the frame WAS the
/// screen. `found` is [`panel_bounds`] of whatever the frame detected, `None`
/// when it detected nothing.
///
/// Two ways a crop can lie, and neither is a closed window:
///
/// - it found nothing. The panel may have MOVED out of the rect the last
///   detect measured — the player dragged the window, or the UI scale changed
///   — and a crop cut around the old position cannot see the new one. Counting
///   that as a miss would retire an open window in two cadences, which is the
///   phantom retire WI-A exists to stop, walking back in through the crop;
/// - it found a panel that does not FIT. The cells past the crop edge are not
///   in the image, `occupied` reads them as empty slots and the row stops
///   there, so a partial panel would publish as a mercenary with fewer
///   supports than they have.
///
/// A full frame answers for itself: there is nowhere else on the screen to
/// look, so `None` crop is never a re-take.
///
/// The FIT test is on the panel's on-screen part, which is why `screen` is a
/// parameter. [`panel_bounds`] reaches [`PANEL_FOOTER_PITCHES`] below the last
/// row and [`PANEL_MARGIN_CELLS`] either side, and none of that is clamped —
/// it is a rect for cursor tests, where a bound past the screen edge costs
/// nothing. [`crop_around`] IS clamped, because a crop has to be a
/// region of a real image. So a recruit window opened near the bottom or the
/// side of the screen produces a panel rect the crop provably cannot contain,
/// on every single tick: without the clamp the loop would take the crop, find
/// the panel, decide it does not fit, and pay a second full-screen OCR for
/// ever — the exact cost the crop was added to remove. Pixels outside the
/// screen are in no frame, cropped or full, so they are not evidence that the
/// crop missed anything.
pub fn crop_needs_full_look(
    crop: Option<[i32; 4]>,
    found: Option<[i32; 4]>,
    screen: [u32; 2],
) -> bool {
    let Some(crop) = crop else {
        return false;
    };
    match found {
        None => true,
        Some(panel) => !encloses(crop, on_screen(panel, screen)),
    }
}

/// `rect` clipped to the screen: the part of it any grab could have seen.
fn on_screen(rect: [i32; 4], screen: [u32; 2]) -> [i32; 4] {
    let x0 = rect[0].max(0);
    let y0 = rect[1].max(0);
    let x1 = (rect[0] + rect[2]).min(screen[0] as i32);
    let y1 = (rect[1] + rect[3]).min(screen[1] as i32);
    [x0, y0, (x1 - x0).max(0), (y1 - y0).max(0)]
}

/// The shared rect construction: the grid plus [`PANEL_MARGIN_CELLS`] either
/// side, one pitch above the first row and `footer_pitches` below the last.
fn bounds(layout: &MercLayout, g: &MercGeometry, footer_pitches: f32) -> Option<[i32; 4]> {
    if layout.rows.is_empty() {
        return None;
    }
    // The observed pitch is 0.0 for a single-row layout — `detect` has no
    // inter-row gap to measure there — so fall back to the reference pitch at
    // this capture's scale, the same substitution the anchor search makes.
    let pitch = if layout.row_pitch > 0.0 {
        layout.row_pitch
    } else {
        g.row_pitch * layout.scale
    };
    let margin = (g.cell_size * layout.scale * PANEL_MARGIN_CELLS).round() as i32;

    let mut top = i32::MAX;
    let mut bottom = i32::MIN;
    let mut right = layout.column_x0;
    for row in &layout.rows {
        top = top.min(row.name_rect[1]);
        bottom = bottom.max(row.name_rect[1] + row.name_rect[3]);
        for cell in &row.cells {
            top = top.min(cell[1]);
            bottom = bottom.max(cell[1] + cell[3]);
            right = right.max(cell[0] + cell[2]);
        }
    }

    let x0 = (layout.column_x0 - margin).max(0);
    let y0 = ((top as f32 - pitch).round() as i32).max(0);
    let x1 = right + margin;
    let y1 = (bottom as f32 + pitch * footer_pitches).round() as i32;
    Some([x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)])
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
    let class = level_line
        .and_then(|lvl| {
            above
                .iter()
                .filter(|l| l.x < lvl.x && vertically_overlaps(l, lvl))
                .min_by_key(|l| lvl.x - l.x)
                .map(|l| clean_header_text(&l.text))
        })
        .filter(|text| !text.is_empty());

    let wager = above
        .iter()
        .find(|l| l.text.trim().to_lowercase().starts_with("wager"))
        .and_then(|l| parse_trailing_number(&l.text));

    // The title is the tallest line above the panel — it is set in a bigger
    // face than every other header field.
    // The "Should Recruit" verdict sits on the wager line in a face as tall
    // as the title, and OCR folds its tick icon into the text ("Should
    // Recruit@") — measured 2026-08-24. It is excluded by its leading word,
    // and glyphs are cut off both ends of the winner for the same reason.
    //
    // **The name is never the class.** Measured on the 2026-08-25 Windows
    // smoke: the header blinked between `Fennik, of Unshakeable Faith · class
    // not read` and `@ Fallen Reverend · @ Fallen Reverend` across re-detects.
    // On the ticks where the title read badly, the tallest line above the
    // panel WAS the class line, so the same string was published as both
    // fields — a claim the recruit window never makes. The tallest line that
    // is not the class wins instead, which is the next candidate down the same
    // ordering rather than a second rule about where a name lives.
    let class_key = class.as_deref().map(str::to_lowercase);
    let mut candidates: Vec<(usize, &&OcrLineBox)> = above
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.text.trim().to_lowercase().starts_with("should "))
        .collect();
    // Tallest first. Ties keep the LAST line of the OCR order, which is what
    // `max_by_key` did before this became a ranking.
    candidates.sort_by(|a, b| b.1.h.cmp(&a.1.h).then(b.0.cmp(&a.0)));
    let name = candidates
        .into_iter()
        .map(|(_, l)| clean_header_text(&l.text))
        .filter(|text| is_name_shaped(text))
        .find(|text| class_key.as_deref() != Some(text.to_lowercase().as_str()));

    MercHeader { name, class, level, wager }
}

/// The longest a mercenary name is allowed to be, in characters.
///
/// `Fennik, of Unshakeable Faith` is 28 and is the longest real name measured
/// so far; 40 leaves the epithet room to grow without admitting a sentence.
const NAME_MAX_CHARS: usize = 40;

/// The most whitespace-separated words a mercenary name may have.
///
/// The generated shape is `Given, the Epithet` / `Given, of the Epithet` — four
/// words at the outside. Five is one word of slack for an OCR split.
const NAME_MAX_WORDS: usize = 5;

/// Whether a string has the SHAPE of a mercenary's name.
///
/// MEASURED 2026-08-26 (app.log 09:41:09): the module sent GGG a trade search
/// labelled `SUPPORTED SKILLS PENETRATE 100/GlRE`. A support-gem tooltip was
/// open over the panel; its lines are inside the header band and taller than
/// the title, so [`parse_header`]'s tallest-line rule picked one. From there
/// the corruption is permanent — `read::better_read` scores 31 alphanumerics
/// over `Arith, the Quickshot`'s 18, and the sticky header keeps the winner.
///
/// The rule is a shape test, not a vocabulary: names are generated, so there is
/// no list to check against. What the panel's title always is, and a gem
/// tooltip line never is:
///
/// - short — at most [`NAME_MAX_CHARS`] characters;
/// - a few words — at most [`NAME_MAX_WORDS`];
/// - free of digits. A name has none. Inside the header band the two lines that
///   legitimately carry digits are the level and the wager, both of which
///   [`parse_header`] identifies by their own rules, and a tooltip's numbers
///   (`100/GlRE`, `+25%`, `Tier 3`) are what mark it as not the title.
///
/// The digit rule has a cost, and it is accepted deliberately. Windows OCR
/// confuses `I`→`1` and `O`→`0` on the panel's small gold-on-dark title, so a
/// real name can come back with a digit in it (`Ar1th, the Quickshot`) and be
/// rejected. That read STALLS: the name stays `None`, `header_complete` keeps
/// the capture incomplete, and the loop reads again until a frame OCRs the
/// title cleanly. The trade is a stall against a poisoned label — and the
/// poisoned label is the worse half, because it does not stay in the app: a
/// complete header opens the trade session (POE-202) and the name goes to GGG
/// as the query's label, which is where `SUPPORTED SKILLS PENETRATE 100/GlRE`
/// went on 2026-08-26. A stall costs ticks; a poisoned label costs a wrong
/// search and a wrong string on the strip, and the sticky merge makes it
/// permanent.
///
/// Rejecting is cheap and accepting is not: a rejected name is `None`, which
/// [`super::read::merge_header`] reads as "not read this tick" and the next
/// clean frame supplies, while an accepted one becomes the label on a GGG
/// query and on the strip over the game.
pub fn is_name_shaped(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && text.chars().count() <= NAME_MAX_CHARS
        && text.split_whitespace().count() <= NAME_MAX_WORDS
        && !text.chars().any(|c| c.is_ascii_digit())
}

/// One header field's OCR text with the glyph noise cut off both ends.
///
/// MEASURED 2026-08-25 (Windows smoke): the class icon left of the class name
/// is OCR'd as a character, so the header published `@ Fallen Reverend`. The
/// tick beside "Should Recruit" does the same at the other end (2026-08-24).
/// Neither is text the recruit window shows, and both survive into the strip
/// over the game and into the header's own stickiness rule, where a leading
/// glyph is exactly what marks a read as the WORSE one
/// ([`super::read::merge_header`]).
///
/// Inner punctuation is untouched: `Cai, the Lout` and `Al-Hezmin` are names.
pub fn clean_header_text(text: &str) -> String {
    text.trim()
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_end_matches(|c: char| !c.is_alphanumeric())
        .to_string()
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

/// The exact string the module published as a mercenary's name on 2026-08-26
/// (app.log 09:41:09) and sent to GGG as a trade query's label: the title line
/// of a support-gem tooltip drawn over the recruit panel.
///
/// 31 alphanumerics against `Arith, the Quickshot`'s 18, which is why
/// [`super::read::better_read`] preferred it once it had won the parse, and why
/// the sticky header then kept it for the rest of the window's life. Shared by
/// the header tests in this module, `read.rs` and `run.rs` so all three argue
/// about the SAME string the log caught.
#[cfg(test)]
pub(crate) const TOOLTIP_NAME: &str = "SUPPORTED SKILLS PENETRATE 100/GlRE";

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
    use std::time::Duration;

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
        let layout =
            detect(&lines, &MercGeometry::default(), &vocab(), None).expect("panel detected");

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
            detect(&reference_lines(), &MercGeometry::default(), &vocab(), None).expect("detected");

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

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

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
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("detected");
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

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

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

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

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

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

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

        assert!(detect(&lines, &MercGeometry::default(), &vocab(), None).is_none());
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

        let layout =
            detect(&lines, &MercGeometry::default(), &vocab(), None).expect("anchored by button");
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
        assert!(detect(&lines, &g, &vocab(), None).is_none(), "out of reach");

        assert!(!is_button_line("Take items", &g));
        assert!(!is_button_line("Rematches", &g));
        assert!(is_button_line("take  item", &g));
        assert!(is_button_line("REMATCH", &g));
    }

    /// The phantom retire, at its source. MEASURED 2026-08-26 (app.log
    /// 09:14:51, 09:41:52): the recruit window was on screen and 12 (then 6)
    /// skill candidates were read, but the ONE anchor line was not, so `detect`
    /// answered "no recruit window" and two of those retired the capture.
    ///
    /// A tooltip deletes a text line; it cannot move the rows. So rows landing
    /// in the rect the panel was last measured at anchor the frame themselves.
    #[test]
    fn rows_inside_the_last_known_panel_anchor_a_frame_whose_chrome_is_gone() {
        let g = MercGeometry::default();
        let rect = panel_bounds(
            &detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel"),
            &g,
        )
        .expect("the reference panel has bounds");
        let stripped: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Wager"))
            .collect();

        let layout = detect(&stripped, &g, &vocab(), Some(rect)).expect("the rect anchors it");

        assert_eq!(layout.rows.len(), 6);
    }

    /// The FIRST detect has no rect, and it still needs the chrome: the anchor
    /// is what separates a recruit window from a gem tooltip or the character
    /// panel, and nothing may capture one of those.
    #[test]
    fn a_frame_with_no_known_rect_still_needs_its_chrome_line() {
        let stripped: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Wager"))
            .collect();

        assert!(detect(&stripped, &MercGeometry::default(), &vocab(), None).is_none());
    }

    /// A rect is evidence only while the rows are IN it. A window the player
    /// dragged elsewhere — or a skill column on some other surface, with the
    /// last panel's rect still on the session — falls back to the chrome
    /// anchor rather than inheriting an identity from where a panel used to be.
    #[test]
    fn a_known_rect_the_rows_are_not_inside_does_not_anchor() {
        let stripped: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Wager"))
            .collect();
        // The reference rows run from y 620 to 862 at x 134.
        let elsewhere = [1000, 100, 400, 300];

        assert!(detect(&stripped, &MercGeometry::default(), &vocab(), Some(elsewhere)).is_none());
    }

    /// EVERY row must be inside, not merely one. A rect covering the top half
    /// of the column is a panel that moved or resized, and half its rows
    /// landing in the old footprint is not evidence that it is the same window.
    #[test]
    fn a_rect_that_holds_only_some_of_the_rows_does_not_anchor() {
        let g = MercGeometry::default();
        let stripped: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Wager"))
            .collect();
        // The full panel's rect, cut off below the third row. Its LEFT EDGE is
        // the real one — a rect at some other x is rejected by the column test
        // before the all-quantifier is ever reached, which is not what this
        // test is about.
        let full = panel_bounds(
            &detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel"),
            &g,
        )
        .expect("six rows have bounds");
        // Rows sit at 620, 669, 717, 766, 814, 862; this stops after the third.
        let half = [full[0], full[1], full[2], 740 - full[1]];
        assert_eq!(
            panel_anchor(
                Some(half),
                &[620.0, 669.0, 717.0, 766.0, 814.0, 862.0],
                134.0,
                &g,
                48.0 / g.row_pitch
            ),
            PanelAnchor::RowOutside { centre: 766 },
            "the rect must be rejected by the all-quantifier, not by the column test"
        );

        assert!(detect(&stripped, &g, &vocab(), Some(half)).is_none());
    }

    /// The reference panel as a TOOLTIP leaves it: the header and the wager
    /// line, and only the top two of its six skill rows. This is the shape the
    /// 2026-08-26 smoke produced — the game draws the tooltip over the lower
    /// rows, OCR reads what is left, and `detect` returns a two-row layout for
    /// a six-row window.
    fn partially_covered_reference_lines() -> Vec<OcrLineBox> {
        reference_lines()
            .into_iter()
            .filter(|l| l.x != 134 || l.centre_y() <= 700.0)
            .collect()
    }

    /// The reference panel with its wager line gone — what a tooltip over the
    /// chrome leaves, and the frame that has nothing but the known rect to
    /// anchor on.
    fn chromeless_reference_lines() -> Vec<OcrLineBox> {
        reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Wager"))
            .collect()
    }

    /// The rect the loop remembers must GROW, never shrink to whatever the
    /// last frame could see. A tooltip over the lower rows measures a panel
    /// two rows tall; writing that over the six-row rect loses the bottom of
    /// the window, and the rect is the anchor the NEXT frame needs.
    #[test]
    fn the_grown_rect_covers_every_row_the_full_read_measured() {
        let g = MercGeometry::default();
        let full = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let full_rect = panel_bounds(&full, &g).expect("six rows have bounds");
        let partial = detect(&partially_covered_reference_lines(), &g, &vocab(), None)
            .expect("two rows and the wager still detect");
        let partial_rect = panel_bounds(&partial, &g).expect("two rows have bounds");
        let bottom_row = full.rows.last().expect("six rows").centre_y.round() as i32;
        // The precondition the bug is made of. Without it the union below
        // would be covering rows the partial rect already held.
        assert!(
            !contains(partial_rect, (full.column_x0, bottom_row)),
            "the two-row rect {partial_rect:?} must stop short of row 6 at {bottom_row}"
        );

        let grown = union_rect(Some(full_rect), Some(partial_rect)).expect("two rects union");

        for row in &full.rows {
            assert!(
                contains(grown, (full.column_x0, row.centre_y.round() as i32)),
                "row {} at {} fell outside the grown rect {grown:?}",
                row.index,
                row.centre_y
            );
        }
    }

    /// The retire the smoke recorded, in one assertion pair: with the chrome
    /// hidden the rect is the only anchor left, the shrunken one rejects the
    /// full read, and the grown one carries it.
    #[test]
    fn a_full_read_anchors_on_the_grown_rect_the_shrunken_one_rejects() {
        let g = MercGeometry::default();
        let full_rect =
            panel_bounds(&detect(&reference_lines(), &g, &vocab(), None).expect("six rows"), &g)
                .expect("six rows have bounds");
        let partial_rect = panel_bounds(
            &detect(&partially_covered_reference_lines(), &g, &vocab(), None).expect("two rows"),
            &g,
        )
        .expect("two rows have bounds");
        let chromeless = chromeless_reference_lines();
        // The precondition: this is the frame that retired a window still on
        // screen (app.log 2026-08-26 16:08:25 → 16:08:28).
        assert!(
            detect(&chromeless, &g, &vocab(), Some(partial_rect)).is_none(),
            "the shrunken rect must not anchor six rows — otherwise this proves nothing"
        );

        let grown = union_rect(Some(full_rect), Some(partial_rect));

        assert_eq!(
            detect(&chromeless, &g, &vocab(), grown).map(|l| l.rows.len()),
            Some(6)
        );
    }

    /// The FIRST detect of a window has nothing remembered, and the rect it
    /// measures must survive the union unchanged — otherwise the grow-only
    /// rule would leave the loop permanently without an anchor.
    #[test]
    fn growing_from_nothing_keeps_the_rect_just_measured() {
        assert_eq!(union_rect(None, Some([100, 200, 300, 400])), Some([100, 200, 300, 400]));
    }

    /// A layout with no rows has no bounds, and a frame that produced none
    /// must not erase the rect the loop is holding.
    #[test]
    fn growing_onto_nothing_keeps_the_remembered_rect() {
        assert_eq!(union_rect(Some([100, 200, 300, 400]), None), Some([100, 200, 300, 400]));
    }

    // -- the fold the loop actually applies ([`next_panel`]) ---------------

    /// The grow-only rule, through the fold rather than the union directly:
    /// nothing about this frame says the panel changed, so a partial read under
    /// a tooltip must not shrink the rect the next full read anchors on.
    #[test]
    fn an_unchanged_panel_grows_the_remembered_rect() {
        let g = MercGeometry::default();
        let full = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let full_rect = panel_bounds(&full, &g).expect("six rows have bounds");
        let partial = detect(&partially_covered_reference_lines(), &g, &vocab(), None)
            .expect("two rows and the wager still detect");
        let partial_rect = panel_bounds(&partial, &g).expect("two rows have bounds");
        let bottom_row = full.rows.last().expect("six rows").centre_y.round() as i32;
        // The precondition the bug is made of: this frame's own rect does not
        // reach row 6, so a fold that took it alone would lose the row.
        assert!(
            !contains(partial_rect, (full.column_x0, bottom_row)),
            "the two-row rect {partial_rect:?} must stop short of row 6 at {bottom_row}"
        );

        let held = next_panel(
            Some(full_rect),
            Some(partial_rect),
            false,
            column_tolerance(&g, partial.scale),
        )
        .expect("two rects fold");

        for row in &full.rows {
            assert!(
                contains(held, (full.column_x0, row.centre_y.round() as i32)),
                "row {} at {} fell outside the held rect {held:?}",
                row.index,
                row.centre_y
            );
        }
    }

    /// The exception a union cannot express, and the one that would poison the
    /// anchor for the rest of the capture: a panel the player DRAGGED. Its rows
    /// still land inside a hull as wide as the grid, so the hull keeps the OLD
    /// column's left edge — and [`panel_anchor`] rebuilds its `expected_x` from
    /// that edge, pinning every later frame to a column the panel has left.
    #[test]
    fn a_panel_whose_column_moved_replaces_the_remembered_rect() {
        let g = MercGeometry::default();
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let remembered = panel_bounds(&layout, &g).expect("six rows have bounds");
        // Two cells right — the same displacement the moved-column anchor test
        // uses, and well past the half-cell tolerance.
        let shift = (g.cell_size * layout.scale * 2.0).round() as i32;
        let moved = [remembered[0] + shift, remembered[1], remembered[2], remembered[3]];

        let held = next_panel(
            Some(remembered),
            Some(moved),
            false,
            column_tolerance(&g, layout.scale),
        )
        .expect("two rects fold");

        assert_eq!(held, moved);
        // Named rather than implied: the hull is what this rule refuses, and it
        // is what the fold produced before the column test existed.
        assert_ne!(
            held,
            union_rect(Some(remembered), Some(moved)).expect("two rects union"),
            "the fold must not hand back the hull of the old position and the new"
        );
    }

    /// A REMATCH puts a DIFFERENT mercenary behind a panel that looks the same.
    /// The rect the loop grew belongs to the window that is gone, and a hull
    /// spanning both would carry a retired mercenary's footprint onto the one
    /// that replaced it — the same inheritance the confirmations are dropped
    /// for.
    #[test]
    fn a_replaced_panel_takes_this_frames_rect_alone() {
        let remembered = [100, 200, 300, 400];
        let fresh = [100, 260, 300, 150];

        assert_eq!(next_panel(Some(remembered), Some(fresh), true, 20), Some(fresh));
    }

    /// The FIRST detect of a window has nothing remembered, and the rect it
    /// just measured has to come through the fold unchanged — otherwise the
    /// loop would never acquire an anchor at all.
    #[test]
    fn a_first_detect_keeps_the_rect_it_just_measured() {
        assert_eq!(
            next_panel(None, Some([100, 200, 300, 400]), false, 20),
            Some([100, 200, 300, 400])
        );
    }

    /// The reason the log needs when the column test is what said no: a panel
    /// dragged sideways keeps every row inside a rect as wide as the grid, so
    /// this is the only sub-predicate that can catch it — and the line has to
    /// say so rather than blaming a row.
    #[test]
    fn the_anchor_reports_a_moved_column_with_both_positions() {
        let g = MercGeometry::default();
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let rect = panel_bounds(&layout, &g).expect("six rows have bounds");
        let centres: Vec<f32> = layout.rows.iter().map(|r| r.centre_y).collect();
        // Two cells right of where the rect was built — well past the
        // half-cell tolerance, and still inside the rect's own width.
        let moved = layout.column_x0 as f32 + g.cell_size * layout.scale * 2.0;

        let why = panel_anchor(Some(rect), &centres, moved, &g, layout.scale);

        match why {
            PanelAnchor::ColumnMoved { column_x, expected_x, tolerance } => {
                assert_eq!(column_x, moved.round() as i32);
                // The rect reconstructs the column it was built from.
                assert_eq!(expected_x, layout.column_x0);
                assert!(
                    (column_x - expected_x).abs() > tolerance,
                    "{column_x} vs {expected_x} must exceed the tolerance {tolerance} that \
                     rejected it"
                );
            }
            other => panic!("expected ColumnMoved, got {other:?}"),
        }
    }

    /// The other sub-predicate: the line must name a row centre that fell out,
    /// because that is what says the rect is too SHORT rather than in the wrong
    /// place — and it must name the FIRST of them, the top of what was lost,
    /// which is where the rect stops rather than where the panel ends.
    ///
    /// The rect a tooltip leaves cannot show that: [`PANEL_FOOTER_PITCHES`]
    /// makes a two-row rect reach past rows 3-5, so exactly one centre falls
    /// out of it and first, last and any are the same answer. A panel DRAGGED
    /// DOWN with its column unchanged is the case that separates them — the
    /// column test passes, and every centre past the remembered rect's bottom
    /// is outside it.
    #[test]
    fn the_anchor_reports_the_first_row_that_fell_outside() {
        let g = MercGeometry::default();
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let remembered = panel_bounds(&layout, &g).expect("six rows have bounds");
        // Far enough that row 4's centre lands one px past the remembered
        // rect's bottom edge, which puts rows 4, 5 and 6 outside it.
        let drop = (remembered[1] + remembered[3]) as f32 - layout.rows[3].centre_y + 1.0;
        let dropped: Vec<f32> = layout.rows.iter().map(|r| r.centre_y + drop).collect();
        let outside: Vec<i32> = dropped
            .iter()
            .filter(|&&c| !contains(remembered, (layout.column_x0, c.round() as i32)))
            .map(|c| c.round() as i32)
            .collect();
        // Without this the assertion below would hold for "the last row that
        // fell outside", or for any of them, and would prove nothing about
        // WHICH one the reason names.
        assert!(
            outside.len() > 1,
            "more than one row must fall outside {remembered:?} or `first` is not exercised, \
             dropped centres were {dropped:?}"
        );

        let why = panel_anchor(Some(remembered), &dropped, layout.column_x0 as f32, &g, layout.scale);

        assert_eq!(why, PanelAnchor::RowOutside { centre: outside[0] });
    }

    /// No live capture is not the same answer as a rect the rows missed, and
    /// the log has to tell them apart: one says the loop had nothing to weigh
    /// against, the other says it weighed and rejected.
    #[test]
    fn the_anchor_reports_a_missing_rect_rather_than_a_row() {
        let g = MercGeometry::default();

        let why = panel_anchor(None, &[620.0, 669.0], 134.0, &g, 1.0);

        assert_eq!(why, PanelAnchor::NoKnownRect);
    }

    /// A rect with nothing to place in it — the frame read no rows at all.
    #[test]
    fn the_anchor_reports_no_rows_when_the_frame_clustered_none() {
        let g = MercGeometry::default();

        let why = panel_anchor(Some([80, 560, 600, 400]), &[], 134.0, &g, 1.0);

        assert_eq!(why, PanelAnchor::NoRows);
    }

    /// The positive answer, which is what `detect` branches on.
    #[test]
    fn the_anchor_reports_anchored_when_the_rows_are_where_the_rect_is() {
        let g = MercGeometry::default();
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let rect = panel_bounds(&layout, &g).expect("six rows have bounds");
        let centres: Vec<f32> = layout.rows.iter().map(|r| r.centre_y).collect();

        let why = panel_anchor(Some(rect), &centres, layout.column_x0 as f32, &g, layout.scale);

        assert_eq!(why, PanelAnchor::Anchored);
    }

    /// The miss the smoke could not explain: rows read, chrome gone, nothing
    /// remembered. Every field on the report is one the log line prints.
    #[test]
    fn a_chromeless_frame_with_no_rect_misses_at_the_anchor_step() {
        let g = MercGeometry::default();

        let why = detect_reason(&chromeless_reference_lines(), &g, &vocab(), None)
            .expect_err("no anchor, no layout");

        assert_eq!(why.column_x0, Some(134.0));
        assert_eq!(
            why.stage,
            DetectStage::NoAnchor { rows: 6, panel: PanelAnchor::NoKnownRect }
        );
    }

    /// The ordinary miss — an empty screen — must report the candidate step,
    /// not the anchor step, so a log full of these is legible as "nothing on
    /// screen" rather than "the panel keeps losing its anchor".
    #[test]
    fn a_frame_with_no_skill_names_misses_for_want_of_candidates() {
        let g = MercGeometry::default();

        let why = detect_reason(&[line("Vika has entered the area", 10, 10)], &g, &vocab(), None)
            .expect_err("no candidates, no layout");

        assert_eq!(why.candidates, 0);
        assert_eq!(why.column_x0, None);
        assert_eq!(
            why.stage,
            DetectStage::TooFewCandidates { needed: g.min_skill_candidates }
        );
    }

    /// The all-quantifier pins the VERTICAL span and almost nothing else. The
    /// rect is as wide as the grid, so a panel dragged sideways keeps every row
    /// "inside" it and would inherit the old panel's identity on the strength
    /// of a band of screen. The column has to be where the old column was.
    #[test]
    fn a_rect_displaced_only_horizontally_does_not_anchor() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");
        let rect = panel_bounds(&layout, &g).expect("six rows have bounds");
        let moved = [rect[0] - 200, rect[1], rect[2], rect[3]];
        let stripped: Vec<OcrLineBox> = windows_dump_lines()
            .into_iter()
            .filter(|l| !is_button_line(&l.text, &g))
            .collect();
        assert!(
            layout.rows.iter().all(|r| {
                contains(moved, (layout.column_x0 as i32, r.name_rect[1] + r.name_rect[3] / 2))
            }),
            "arrange: every row still lands inside the displaced rect",
        );

        assert!(
            detect(&stripped, &g, &vocab(), None).is_none(),
            "arrange: with the buttons gone this frame has no chrome anchor left",
        );
        assert!(detect(&stripped, &g, &vocab(), Some(moved)).is_none());
        assert!(
            detect(&stripped, &g, &vocab(), Some(rect)).is_some(),
            "the same frame against the UNMOVED rect still anchors",
        );
    }

    /// Two rows inside the old footprint but in another column are not the old
    /// panel. This is the case the vertical test cannot see: the cluster is
    /// short enough to fit the band whatever it is, so the column is the only
    /// thing left that says which window it belongs to.
    #[test]
    fn a_two_row_cluster_in_another_column_does_not_anchor_inside_the_old_footprint() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");
        let rect = panel_bounds(&layout, &g).expect("six rows have bounds");
        let pair: Vec<OcrLineBox> = windows_dump_lines()
            .into_iter()
            .filter(|l| l.text == "FROST BOMB" || l.text == "FROSTBITE")
            .collect();
        let shifted: Vec<OcrLineBox> = pair
            .iter()
            .map(|l| OcrLineBox { x: l.x + 200, ..l.clone() })
            .collect();
        assert!(
            shifted
                .iter()
                .all(|l| contains(rect, (l.x, l.centre_y().round() as i32))),
            "arrange: the shifted pair is still inside the old rect",
        );

        assert!(detect(&shifted, &g, &vocab(), Some(rect)).is_none());
        assert!(
            detect(&pair, &g, &vocab(), Some(rect)).is_some(),
            "the same two rows in the panel's OWN column do anchor",
        );
    }

    /// The 2026-08-24 Windows dump (1920×1200, merc-debug/1787604709231) as
    /// OCR returned it: the wager line absent, both footer buttons present,
    /// six rows, and the quest tracker's own tall text off to the right.
    fn windows_dump_lines() -> Vec<OcrLineBox> {
        vec![
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
        ]
    }

    /// Six rows off a real screen, anchored by the buttons because the wager
    /// line never reached the OCR.
    #[test]
    fn the_first_windows_dump_detects_by_the_recruit_buttons() {
        let lines = windows_dump_lines();
        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");
        assert_eq!(layout.rows.len(), 6);
        assert_eq!(layout.header.name.as_deref(), Some("Nytra, the Cyaxan Loner"));
        assert_eq!(layout.header.level, Some(83));
        assert_eq!(layout.header.class.as_deref(), Some("Infamous Frosthand"));
        // The quest tracker's "Speak to Johan for a reward" (x 1647) is
        // outside the panel and must not win the name.
        let mut lines = lines;
        lines.push(OcrLineBox { text: "SpeakrgVohÅn for a reward".into(), x: 1520, y: 350, w: 300, h: 30 });
        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");
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
        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");
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

        assert!(detect(&lines, &MercGeometry::default(), &vocab(), None).is_none());
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

        assert!(detect(&lines, &MercGeometry::default(), &vocab(), None).is_none());
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
                detect(&lines, &MercGeometry::default(), &vocab(), None).is_none(),
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
                detect(&lines, &MercGeometry::default(), &vocab(), None).is_some(),
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

        assert!(detect(&lines, &MercGeometry::default(), &vocab(), None).is_none());
    }

    /// One skill name is not a panel — D2 needs two, so a stray gem name in a
    /// chat window cannot start a capture.
    #[test]
    fn a_single_skill_name_is_not_enough_to_detect_a_panel() {
        let lines = vec![line("Wager: 1 028", 80, 173), line("Conductivity", 134, 620)];

        assert!(detect(&lines, &MercGeometry::default(), &vocab(), None).is_none());
    }

    /// No skill names at all: the detector must not fall back to "any column".
    #[test]
    fn a_screen_with_no_skill_names_detects_nothing() {
        let lines = vec![
            line("Wager: 1 028", 80, 173),
            line("Inventory", 134, 620),
            line("Stash", 134, 669),
        ];

        assert!(detect(&lines, &MercGeometry::default(), &vocab(), None).is_none());
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

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

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
        let base = detect(&reference_lines(), &MercGeometry::default(), &vocab(), None).unwrap();

        let layout = detect(&doubled, &MercGeometry::default(), &vocab(), None).expect("detected");

        assert!((layout.scale - base.scale * 2.0).abs() < 1e-4, "scale {}", layout.scale);
        assert_eq!(layout.rows[0].cells[0][2], base.rows[0].cells[0][2] * 2);
    }

    /// The header is read from the lines above the panel, each field
    /// independently. `Wager: 1 028` carries a thousands space that must not
    /// truncate the number to 1.
    #[test]
    fn the_header_reads_name_class_level_and_a_spaced_wager() {
        let layout =
            detect(&reference_lines(), &MercGeometry::default(), &vocab(), None).expect("detected");

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

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

        assert_eq!(layout.header.name.as_deref(), Some("Cai, the Lout"));
    }

    /// MEASURED 2026-08-25 (Windows smoke): the strip published `@ Fallen
    /// Reverend` as the class. The `@` is the class ICON, OCR'd as a glyph —
    /// text the recruit window never shows.
    #[test]
    fn a_leading_icon_glyph_is_not_part_of_the_class() {
        let mut lines = reference_lines();
        for l in lines.iter_mut() {
            if l.text == "Shock Ambusher" {
                l.text = "@ Shock Ambusher".into();
            }
        }

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

        assert_eq!(layout.header.class.as_deref(), Some("Shock Ambusher"));
    }

    /// The same glyph on the title line. The name is what the strip's first
    /// field prints, so a leading `@` there is what the player reads.
    #[test]
    fn a_leading_icon_glyph_is_not_part_of_the_name() {
        let mut lines = reference_lines();
        for l in lines.iter_mut() {
            if l.text == "Cai, the Lout" {
                l.text = "@ Cai, the Lout".into();
                l.h = 30;
            }
        }

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

        assert_eq!(layout.header.name.as_deref(), Some("Cai, the Lout"));
    }

    /// MEASURED 2026-08-25: on the ticks where the title read badly, the
    /// TALLEST line above the panel was the class line, and the strip printed
    /// `@ Fallen Reverend · @ Fallen Reverend` — the same string as both
    /// fields. A mercenary is never its own class; the next candidate down the
    /// same tallest-first ordering is.
    #[test]
    fn the_name_is_never_the_class_line() {
        let mut lines = reference_lines();
        for l in lines.iter_mut() {
            match l.text.as_str() {
                // OCR boxed the class line taller than the title this tick.
                "Shock Ambusher" => l.h = 30,
                "Cai, the Lout" => l.h = 26,
                _ => {}
            }
        }

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

        assert_eq!(layout.header.class.as_deref(), Some("Shock Ambusher"));
        assert_eq!(layout.header.name.as_deref(), Some("Cai, the Lout"));
    }

    /// The header corruption of 2026-08-26, at its source. A support-gem
    /// tooltip drawn over the panel puts its own lines in the header band, and
    /// they are set taller than the title — so the tallest-line rule picked
    /// one, `merge_header` made it permanent, and it went to GGG as the label
    /// on a trade query (app.log 09:41:09).
    #[test]
    fn a_tooltip_line_in_the_header_band_does_not_become_the_name() {
        let mut lines = reference_lines();
        // The title is set larger than the rest of the header, as it is on
        // screen; the tooltip's line is larger still, which is the whole
        // problem — height alone ranks it first.
        for l in lines.iter_mut() {
            if l.text.starts_with("Cai") {
                l.h = 26;
            }
        }
        lines.push(OcrLineBox { text: TOOLTIP_NAME.into(), x: 300, y: 100, w: 280, h: 40 });

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

        assert_eq!(layout.header.name.as_deref(), Some("Cai, the Lout"));
    }

    /// The tooltip line is 35 characters and four words — inside both counting
    /// rules — so the digits are what reject it. Nothing in a mercenary's name
    /// is a digit; inside the header band the lines that carry them are the
    /// level and the wager, which this parse finds by their own labels.
    #[test]
    fn a_candidate_carrying_digits_is_not_a_name() {
        assert!(!is_name_shaped(TOOLTIP_NAME));
        assert!(is_name_shaped("Arith, the Quickshot"));
    }

    /// The length cap, at its boundary. A name is a title, not a sentence —
    /// the longest measured is `Fennik, of Unshakeable Faith` at 28. The
    /// lengths are spelled out rather than taken from `NAME_MAX_CHARS`, so
    /// moving the cap moves this test red instead of moving it along.
    #[test]
    fn a_candidate_longer_than_forty_characters_is_not_a_name() {
        assert!(is_name_shaped(&"a".repeat(40)));
        assert!(!is_name_shaped(&"a".repeat(41)));
    }

    /// The word cap, at its boundary. The generated shape is `Given, the
    /// Epithet` — four words at the outside, five here for an OCR split — and
    /// a body-text line that fits in forty characters does not have it.
    #[test]
    fn a_candidate_of_more_than_five_words_is_not_a_name() {
        assert!(is_name_shaped("Fennik, of the Unshakeable Faith"));
        // Six words, 25 characters — under the length cap, over the word cap.
        assert!(!is_name_shaped("and of the many that lost"));
    }

    /// Header fields the panel does not show stay `None`. Guessing a level
    /// would put a number on the page that the game never displayed.
    #[test]
    fn missing_header_fields_stay_none() {
        let lines: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Lvl"))
            .collect();

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

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

    // -- the panel rect the occlusion rule tests against -------------------

    /// A layout the way `detect` builds one: cells laid out from the column x
    /// at the reference offsets, all `max_slots` of them.
    fn layout_of(centres: &[f32], row_pitch: f32, column_x0: i32, g: &MercGeometry) -> MercLayout {
        let cell_size = g.cell_size as i32;
        MercLayout {
            scale: 1.0,
            column_x0,
            row_pitch,
            rows: centres
                .iter()
                .enumerate()
                .map(|(i, &centre)| MercLayoutRow {
                    index: i as u8,
                    centre_y: centre,
                    name_rect: [column_x0, centre as i32 - 8, 90, 16],
                    text: "Ice Shot".into(),
                    cells: (0..g.max_slots)
                        .map(|slot| {
                            [
                                column_x0 + g.cell_offset_x as i32 + slot as i32 * g.cell_pitch as i32,
                                centre as i32 - cell_size / 2,
                                cell_size,
                                cell_size,
                            ]
                        })
                        .collect(),
                })
                .collect(),
            header: MercHeader::default(),
        }
    }

    /// The rect, edge by edge: half a cell either side of the grid, one row
    /// pitch above the first row and `PANEL_FOOTER_PITCHES` below the last.
    /// Written out because every edge is a separate decision the occlusion rule
    /// depends on — a rect that stops at the skill text would call a cursor on
    /// a support cell "outside".
    #[test]
    fn the_panel_rect_wraps_the_grid_by_a_margin_a_pitch_and_the_footer() {
        let g = MercGeometry::default();
        let layout = layout_of(&[200.0, 249.0], 49.0, 100, &g);

        let rect = panel_bounds(&layout, &g).expect("a two-row layout has bounds");

        // column 100 − 22 margin; row-0 cell top 178 − 49 pitch.
        // last cell right 583 + 44 + 22; row-1 cell bottom 271 + 3 × 49.
        assert_eq!(rect, [78, 129, 571, 289]);
    }

    /// The consumer's question, asked directly: the cursor that provokes the
    /// tooltip is on a SUPPORT CELL, including the last slot — which the
    /// published capture drops when the slot is empty, and which is why the
    /// rect is measured off the layout's full grid.
    #[test]
    fn a_cursor_on_the_last_support_slot_is_inside_the_panel() {
        let g = MercGeometry::default();
        let layout = layout_of(&[200.0, 249.0], 49.0, 100, &g);
        let last = *layout.rows[1].cells.last().expect("six slots");

        let rect = panel_bounds(&layout, &g).expect("a two-row layout has bounds");

        assert!(contains(rect, (last[0] + last[2] / 2, last[1] + last[3] / 2)));
    }

    /// …and it does not swallow the screen beside the panel, which is what
    /// would keep a closed window's capture alive for any parked cursor.
    #[test]
    fn a_cursor_well_right_of_the_grid_is_outside_the_panel() {
        let g = MercGeometry::default();
        let layout = layout_of(&[200.0, 249.0], 49.0, 100, &g);
        let last = *layout.rows[1].cells.last().expect("six slots");

        let rect = panel_bounds(&layout, &g).expect("a two-row layout has bounds");

        assert!(!contains(rect, (last[0] + last[2] + 200, last[1])));
    }

    /// A ONE-row panel has no inter-row gap, so `detect` reports `row_pitch`
    /// 0.0 and the vertical band has to come from the reference pitch at this
    /// capture's scale. Without the fallback the rect would hug the row and a
    /// cursor in the header would read as "off the panel".
    #[test]
    fn a_single_row_layout_still_gets_a_vertical_band() {
        let g = MercGeometry::default();
        let layout = layout_of(&[200.0], 0.0, 100, &g);
        let cell_top = layout.rows[0].cells[0][1];

        let rect = panel_bounds(&layout, &g).expect("a one-row layout has bounds");

        assert!(contains(rect, (layout.column_x0, cell_top - g.row_pitch as i32 + 1)));
        assert!(!contains(rect, (layout.column_x0, cell_top - g.row_pitch as i32 - 5)));
    }

    /// The footer is the one part of the panel the player is CERTAIN to put the
    /// cursor on: TAKE ITEM is how the window is closed and REMATCH is how it
    /// is rerolled, and both open a tooltip that can cost the frame its anchor.
    /// A cursor there has to read as occlusion, or the two detects it costs
    /// retire a window that is still on screen (app.log 2026-08-26 09:14).
    ///
    /// Measured against the 2026-08-24 Windows dump, whose buttons OCR at
    /// y 979-992 — the label. The button's own box carries on below its text,
    /// which is where the old one-pitch rect stopped.
    #[test]
    fn a_cursor_on_the_footer_below_the_button_label_holds_the_capture() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");
        let rect = panel_bounds(&layout, &g).expect("six rows have bounds");
        // The TAKE ITEM label's centre, one row pitch further down the button.
        let cursor = (873, 985 + layout.row_pitch as i32);

        assert_eq!(
            crate::mercenary::run::miss_kind(true, contains(rect, cursor), Duration::ZERO),
            crate::mercenary::run::MissKind::Occluded,
        );
    }

    /// …and the band stops. Below the footer is the skill bar and the globes,
    /// where a cursor rests for minutes at a time — a rect reaching there would
    /// hold a closed window's verdict on screen for the whole of `OCCLUDED_MAX`
    /// every time the player parked the mouse.
    #[test]
    fn a_cursor_a_long_way_below_the_footer_is_outside_the_panel() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");
        let rect = panel_bounds(&layout, &g).expect("six rows have bounds");

        assert!(!contains(rect, (873, 985 + 4 * layout.row_pitch as i32)));
    }

    /// The two rects answer different questions and must not share a bottom.
    /// A cursor on TAKE ITEM is INSIDE the panel — that is the whole point of
    /// `PANEL_FOOTER_PITCHES`, and it is what holds the capture through the
    /// button's own tooltip. It is OUTSIDE the header guard, because a tooltip
    /// drawn three pitches below the last row cannot put lines in the header
    /// band, and withholding on it would blank the name at the exact moment the
    /// player is about to take the mercenary.
    #[test]
    fn a_cursor_on_the_footer_is_inside_the_panel_but_outside_the_header_guard() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");
        // The TAKE ITEM label's centre, one row pitch further down the button.
        let cursor = (873, 985 + layout.row_pitch as i32);

        assert!(contains(panel_bounds(&layout, &g).expect("six rows have bounds"), cursor));
        assert!(!contains(
            header_guard_bounds(&layout, &g).expect("six rows have bounds"),
            cursor
        ));
    }

    /// …and the guard is not a degenerate rect. It still wraps the grid and the
    /// chrome one pitch above row 0 — the band a tooltip has to be drawn in to
    /// reach `parse_header`'s candidates at all.
    #[test]
    fn the_header_guard_covers_the_grid_and_the_pitch_above_the_first_row() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");
        let first = layout.rows[0].cells[0];

        let guard = header_guard_bounds(&layout, &g).expect("six rows have bounds");

        assert!(contains(guard, (first[0] + first[2] / 2, first[1] + first[3] / 2)));
        assert!(contains(guard, (layout.column_x0 as i32, first[1] - layout.row_pitch as i32 + 1)));
    }

    #[test]
    fn a_layout_with_no_rows_has_no_header_guard_rect() {
        let g = MercGeometry::default();
        let layout = layout_of(&[], 0.0, 100, &g);

        assert_eq!(header_guard_bounds(&layout, &g), None);
    }

    #[test]
    fn a_layout_with_no_rows_has_no_panel_rect() {
        let g = MercGeometry::default();
        let layout = layout_of(&[], 0.0, 100, &g);

        assert_eq!(panel_bounds(&layout, &g), None);
    }

    // -- cropped detect frames ---------------------------------------------

    /// The reference panel's lines as a CROP would report them: the same screen
    /// re-expressed in the crop's own pixels, which is what Windows OCR hands
    /// back when it is given a cropped image.
    fn crop_relative(lines: Vec<OcrLineBox>, origin: (i32, i32)) -> Vec<OcrLineBox> {
        lines
            .into_iter()
            .map(|l| OcrLineBox { x: l.x - origin.0, y: l.y - origin.1, ..l })
            .collect()
    }

    /// The translation itself: a box the OCR reported inside a crop comes back
    /// out at the screen position it was cut from, size untouched.
    #[test]
    fn a_crops_ocr_boxes_come_back_at_the_screen_position_they_were_cut_from() {
        let origin = (112, 22);
        let screen = [1920, 1200];

        let out = Frame::cropped(origin, screen).to_screen(crop_relative(reference_lines(), origin));

        assert_eq!(out, reference_lines());
    }

    /// The inverse, for the one thing that still indexes the image: a screen
    /// rect in the crop's own pixels.
    #[test]
    fn a_screen_rect_maps_back_into_the_crops_own_pixels() {
        let frame = Frame::cropped((112, 22), [1920, 1200]);

        assert_eq!(frame.local([200, 100, 44, 44]), [88, 78, 44, 44]);
    }

    /// Why the translation is load-bearing rather than tidy. Untranslated, a
    /// crop's rows are reported hundreds of px left of where they are, and the
    /// known-panel anchor's column-x test — the ONE thing left pinning the
    /// horizontal axis on a frame whose chrome a tooltip deleted — reads them
    /// as a different window. The same lines translated anchor.
    #[test]
    fn a_crops_untranslated_rows_miss_the_known_panels_column() {
        let g = MercGeometry::default();
        let rect = panel_bounds(
            &detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel"),
            &g,
        )
        .expect("the reference panel has bounds");
        let origin = (112, 22);
        let stripped: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Wager"))
            .collect();
        let raw = crop_relative(stripped, origin);

        assert!(
            detect(&raw, &g, &vocab(), Some(rect)).is_none(),
            "the crop's own pixels are not the screen's, and the column test must say so",
        );
        assert!(
            detect(&Frame::cropped(origin, [1920, 1200]).to_screen(raw), &g, &vocab(), Some(rect))
                .is_some(),
            "translated, the same frame is the panel the session already knows",
        );
    }

    /// The crop the loop takes off a layout, as [`crop_around`]'s callers build
    /// it: the rect that layout measured, at that layout's own pitch.
    fn crop_of(layout: &MercLayout, g: &MercGeometry, screen: [u32; 2]) -> [i32; 4] {
        crop_around(
            panel_bounds(layout, g).expect("a layout with rows has bounds"),
            effective_pitch(layout, g),
            screen,
        )
    }

    /// The crop has to clear the HEADER band, which sits far above the panel
    /// rect's own one-pitch margin: on the reference panel the title is at
    /// y 30 and the wager at 173 while row 1 is at 620. A crop that cut them
    /// off would leave `parse_header` with no name, no class and no level —
    /// the capture would never complete and no trade session would ever open.
    #[test]
    fn the_re_detect_crop_covers_the_header_band_and_the_footer() {
        let g = MercGeometry::default();
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let panel = panel_bounds(&layout, &g).expect("six rows have bounds");

        let crop = crop_of(&layout, &g, [1920, 1200]);

        for header in reference_lines().iter().filter(|l| l.centre_y() < 600.0) {
            assert!(
                encloses(crop, [header.x, header.y, header.w, header.h]),
                "the crop must hold the whole of {:?}",
                header.text,
            );
        }
        assert!(encloses(crop, panel), "and the panel rect, footer included");
    }

    /// The crop follows the rect the LOOP HOLDS, not the layout of the frame
    /// that produced it — which is the whole reason [`crop_around`] takes a
    /// rect. A partial read under a tooltip measures a two-row panel; a crop
    /// built from that layout is two rows tall, and the next full read is then
    /// handed a frame cropped out of the rows it is expected to find, on a
    /// window plainly on screen.
    #[test]
    fn the_crop_after_a_partial_read_still_covers_every_row_the_full_read_measured() {
        let g = MercGeometry::default();
        let full = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");
        let partial = detect(&partially_covered_reference_lines(), &g, &vocab(), None)
            .expect("two rows and the wager still detect");
        let bottom_row = full.rows.last().expect("six rows").centre_y.round() as i32;
        // The precondition, and the bug: this frame's OWN crop loses row 6.
        let from_this_frame = crop_of(&partial, &g, SCREEN);
        assert!(
            !contains(from_this_frame, (full.column_x0, bottom_row)),
            "the two-row layout's own crop {from_this_frame:?} must stop short of row 6 at \
             {bottom_row}, or this test proves nothing"
        );
        // What the tick does: the held rect grows first, and the crop is taken
        // off THAT.
        let held = next_panel(
            panel_bounds(&full, &g),
            panel_bounds(&partial, &g),
            false,
            column_tolerance(&g, partial.scale),
        )
        .expect("two rects fold");

        let crop = crop_around(held, effective_pitch(&partial, &g), SCREEN);

        for row in &full.rows {
            assert!(
                contains(crop, (full.column_x0, row.centre_y.round() as i32)),
                "row {} at {} fell outside the crop {crop:?}",
                row.index,
                row.centre_y
            );
        }
    }

    /// It is a CROP, not the screen: a rect that reached the whole desktop
    /// would buy nothing, and the point of the whole exercise is the OCR cost.
    #[test]
    fn the_re_detect_crop_is_smaller_than_the_screen_it_came_from() {
        let g = MercGeometry::default();
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");

        let crop = crop_of(&layout, &g, [1920, 1200]);

        assert!(crop[2] * crop[3] * 2 < 1920 * 1200, "crop was {crop:?}");
    }

    /// Clamped, not merely computed: a panel near the screen edge would
    /// otherwise produce a rect starting at a negative x and `crop_imm` would
    /// panic on it.
    #[test]
    fn a_crop_never_reaches_outside_the_screen() {
        let g = MercGeometry::default();
        let layout = detect(&reference_lines(), &g, &vocab(), None).expect("the reference panel");

        let crop = crop_of(&layout, &g, [700, 900]);

        assert!(encloses([0, 0, 700, 900], crop), "crop was {crop:?}");
    }

    /// A screen big enough that none of the rects below touch its edges — the
    /// clamp is the subject of its own two tests further down.
    const SCREEN: [u32; 2] = [1920, 1200];

    /// A crop that found nothing is not evidence the window closed: it may have
    /// MOVED out of the rect the last detect measured. One full look first.
    #[test]
    fn a_cropped_frame_that_found_nothing_takes_a_full_look_first() {
        assert!(crop_needs_full_look(Some([100, 100, 400, 400]), None, SCREEN));
    }

    /// A FULL frame that found nothing has already looked everywhere. Making
    /// this true too would mean no detect could ever count as a miss, and no
    /// closed window would ever retire.
    #[test]
    fn a_full_frame_that_found_nothing_is_the_answer() {
        assert!(!crop_needs_full_look(None, None, SCREEN));
    }

    /// The ordinary hit: the panel is where it was, wholly inside the crop.
    #[test]
    fn a_cropped_frame_that_found_the_whole_panel_is_believed() {
        assert!(!crop_needs_full_look(
            Some([100, 100, 400, 400]),
            Some([150, 150, 200, 200]),
            SCREEN,
        ));
    }

    /// A panel hanging over the crop's edge is only PARTLY in the image: the
    /// cells past the edge are not there to be read, `occupied` rejects them
    /// and the row stops short, so the capture would claim a mercenary with
    /// fewer supports than they have.
    #[test]
    fn a_panel_hanging_out_of_the_crop_takes_a_full_look_first() {
        assert!(crop_needs_full_look(
            Some([100, 100, 400, 400]),
            Some([150, 150, 400, 200]),
            SCREEN,
        ));
    }

    /// The screen-edge case, and the reason the FIT test is on the panel's
    /// on-screen part. A recruit window opened low puts `panel_bounds`'
    /// three-pitch footer reach past the bottom of the screen, where
    /// `crop_around` is clamped and cannot follow — so an unclamped
    /// comparison fails on every tick and the loop pays a second full-screen
    /// OCR for ever.
    #[test]
    fn a_panel_whose_footer_reach_runs_off_the_screen_is_still_believed() {
        let screen = [1920, 1200];
        let crop = [100, 700, 800, 500];

        assert!(!crop_needs_full_look(Some(crop), Some([150, 750, 700, 600]), screen));
    }

    /// …and the clamp is only downward. A panel over the crop's edge INSIDE
    /// the screen is still a partial read, and still takes the full look.
    #[test]
    fn the_clamp_does_not_excuse_a_panel_that_overruns_the_crop_on_screen() {
        let screen = [1920, 1200];

        assert!(crop_needs_full_look(
            Some([100, 100, 400, 400]),
            Some([150, 150, 400, 200]),
            screen,
        ));
    }

    // -- the voice-line gate's probe band (POE-204 WI-C) --------------------

    /// The band's whole job: hold the chrome [`probe_hit`] accepts on. Both
    /// buttons of the MEASURED panel, and the skill column beside them, on the
    /// dump the rest of this module is calibrated against.
    #[test]
    fn the_probe_band_covers_the_footer_buttons_of_the_measured_panel() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");

        let band = probe_band_bounds(&layout, &g, [1920, 1200]).expect("six rows have a band");

        for button in windows_dump_lines()
            .iter()
            .filter(|l| is_button_line(&l.text, &g))
        {
            assert!(
                encloses(band, [button.x, button.y, button.w, button.h]),
                "the band must hold {:?}, band was {band:?}",
                button.text,
            );
        }
        assert!(
            contains(band, (layout.column_x0, layout.rows[5].centre_y.round() as i32)),
            "and the last row's column, band was {band:?}",
        );
    }

    /// The reason the band exists at all. A full-screen OCR is the tick's
    /// dominant cost and the probe runs twice per voice line, in an arena full
    /// of mercenaries — a band that read most of the screen would be the burst
    /// this replaced, wearing a different name.
    #[test]
    fn the_probe_band_reads_a_small_fraction_of_the_screen() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");

        let band = probe_band_bounds(&layout, &g, [1920, 1200]).expect("six rows have a band");

        assert!(band[2] * band[3] * 8 < 1920 * 1200, "band was {band:?}");
    }

    /// It is NOT the re-detect crop. That one reaches thirteen pitches up for
    /// the header band because the answer it owes includes the mercenary's
    /// name; the probe owes one bit and everything above the last row is cost
    /// with no bearing on it.
    #[test]
    fn the_probe_band_is_shorter_than_the_re_detect_crop() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");

        let band = probe_band_bounds(&layout, &g, [1920, 1200]).expect("a band");
        let crop = crop_of(&layout, &g, [1920, 1200]);

        assert!(band[3] < crop[3], "band {band:?} vs crop {crop:?}");
    }

    /// Clamped, not merely computed: `crop_imm` is handed these four numbers,
    /// and a panel near an edge would otherwise produce a negative origin or a
    /// width past the image.
    #[test]
    fn a_probe_band_never_reaches_outside_the_screen() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");

        let band = probe_band_bounds(&layout, &g, [800, 1000]).expect("a band");

        assert!(encloses([0, 0, 800, 1000], band), "band was {band:?}");
    }

    /// The fallback for the first recruit window of a session, when no panel
    /// has ever been measured. It has to hold the same buttons — this is the
    /// band a probe uses when it has nothing better, and a miss here is a
    /// stand-down on a window that is open.
    #[test]
    fn the_default_band_covers_the_measured_panels_footer_too() {
        let g = MercGeometry::default();

        let band = default_probe_band([1920, 1200]);

        for button in windows_dump_lines()
            .iter()
            .filter(|l| is_button_line(&l.text, &g))
        {
            assert!(
                encloses(band, [button.x, button.y, button.w, button.h]),
                "the default band must hold {:?}, band was {band:?}",
                button.text,
            );
        }
    }

    /// Still a saving, or the fallback would be the full-screen OCR the gate
    /// exists to avoid. At most three fifths of the height — 0.45 leaves 0.55,
    /// and a fraction pushed lower to buy more slack against the unmeasured
    /// anchoring would stop being a probe and start being the burst.
    #[test]
    fn the_default_band_reads_at_most_three_fifths_of_the_height() {
        let band = default_probe_band([1920, 1200]);

        assert!(band[3] * 5 <= 1200 * 3, "band was {band:?}");
        assert!(encloses([0, 0, 1920, 1200], band), "band was {band:?}");
    }

    /// The whole WIDTH, because the horizontal position has one dump behind it
    /// and no model of how the panel is anchored — see
    /// [`DEFAULT_PROBE_BAND_TOP_FRAC`].
    #[test]
    fn the_default_band_is_the_full_width() {
        let band = default_probe_band([1920, 1200]);

        assert_eq!((band[0], band[2]), (0, 1920));
    }

    /// The slack the fraction buys, stated as the thing it is slack AGAINST:
    /// the one measured footer, at 0.816 of the height. A fraction raised until
    /// it grazed that measurement would stand the gate down on the first
    /// recruit window of every session whose panel sits a little higher.
    #[test]
    fn the_default_band_starts_well_above_the_measured_footer() {
        let band = default_probe_band([1920, 1200]);

        assert!(band[1] < 979 - 400, "band was {band:?}, footer at y 979");
    }

    // -- what a probe accepts on -------------------------------------------

    /// The dump as OCR returned it, wager line and all: the probe accepts.
    #[test]
    fn a_band_holding_a_footer_button_is_a_hit() {
        assert!(probe_hit(&windows_dump_lines(), &MercGeometry::default()));
    }

    /// The 2026-08-24 measurement that shaped the whole anchor rule: Windows
    /// OCR returned NO line for the wager, and both buttons read clean. Either
    /// one alone has to be enough.
    #[test]
    fn either_button_alone_is_a_hit() {
        let g = MercGeometry::default();

        for text in ["TAKE ITEM", "REMATCH"] {
            let lines = vec![OcrLineBox { text: text.into(), x: 830, y: 979, w: 87, h: 13 }];
            assert!(probe_hit(&lines, &g), "{text} must accept");
        }
    }

    /// [`detect`]'s other anchor, and NOT the probe's. The wager line is above
    /// the grid; the remembered band starts at the grid's bottom edge and the
    /// default band at [`DEFAULT_PROBE_BAND_TOP_FRAC`] of the height, against a
    /// wager measured at y 173 of 1200. A probe frame cannot hold it, so a hit
    /// on it would be a hit on a line that is not in the image.
    #[test]
    fn the_wager_line_alone_is_not_a_hit() {
        let lines = vec![OcrLineBox { text: "Wager: 8 831".into(), x: 80, y: 173, w: 120, h: 17 }];

        assert!(is_wager_line(&lines[0].text, &MercGeometry::default()), "arrange: it IS a wager");
        assert!(!probe_hit(&lines, &MercGeometry::default()));
    }

    /// …and neither band reaches it, which is the reason the predicate is gone.
    /// Measured on the same dump the module is calibrated against.
    #[test]
    fn no_probe_band_reaches_the_wager_line() {
        let g = MercGeometry::default();
        let layout = detect(&windows_dump_lines(), &g, &vocab(), None).expect("the dump detects");
        let wager_y = 173;

        let remembered = probe_band_bounds(&layout, &g, [1920, 1200]).expect("a band");
        let default = default_probe_band([1920, 1200]);

        assert!(remembered[1] > wager_y, "remembered band was {remembered:?}");
        assert!(default[1] > wager_y, "default band was {default:?}");
    }

    /// The rejection that matters: skill names are what a gem tooltip and the
    /// character panel are full of, and the probe runs while the player is
    /// walking through an arena. Accepting on those would hand a full detect to
    /// every voice line, which is the burst back.
    #[test]
    fn a_band_holding_only_skill_names_is_not_a_hit() {
        let g = MercGeometry::default();
        let lines: Vec<OcrLineBox> = windows_dump_lines()
            .into_iter()
            .filter(|l| !is_button_line(&l.text, &g) && !is_wager_line(&l.text, &g))
            .collect();

        assert!(!lines.is_empty(), "arrange: the dump still has its skill rows");
        assert!(!probe_hit(&lines, &g));
    }

    #[test]
    fn an_empty_band_is_not_a_hit() {
        assert!(!probe_hit(&[], &MercGeometry::default()));
    }

    /// The probe's frame names itself, so the debug line that reports a band
    /// OCR is distinguishable from the detect's own crop in the log.
    #[test]
    fn a_probe_frame_names_itself() {
        assert_eq!(Frame::probe((650, 900), [1920, 1200]).describe(), "probe");
    }

    /// And it translates like the re-detect's crop — one seam out of OCR space,
    /// whichever grab produced the boxes.
    #[test]
    fn a_probe_frame_translates_like_a_crop() {
        let frame = Frame::probe((650, 900), [1920, 1200]);

        assert_eq!(
            frame.to_screen(vec![OcrLineBox { text: "TAKE ITEM".into(), x: 180, y: 79, w: 87, h: 13 }]),
            vec![OcrLineBox { text: "TAKE ITEM".into(), x: 830, y: 979, w: 87, h: 13 }],
        );
    }
}
