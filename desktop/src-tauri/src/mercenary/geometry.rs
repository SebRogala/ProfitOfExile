//! Recruit-window panel geometry (POE-165 D2) — pure over OCR line rects.
//!
//! ADR-024 / POE-270 makes the screen-slice placed recruit rect the normal input:
//! the shipped placement supplies the crop and the row grid, while the first
//! read verifies that the recruit chrome is inside it. The full-screen OCR path
//! is the explicit one-shot fallback locate. [`detect_reason`] remains the pure
//! row-seeding parser used by that fallback and by the geometry regression suite;
//! the live placed path uses [`placed_layout`] so rows cannot disappear merely
//! because pass-1 OCR mangled a skill name.
//!
//! The contract is that [`detect_reason`] is PURE — not that the module is.
//! [`occupied`] and [`stddev`] both touch pixels, and the frame-anchored fit
//! that runs after a detect ([`super::cellfit`]) touches many more; it lives in
//! its own module precisely so `detect_reason` and its 95 tests never need an
//! image.

use image::GenericImageView;
use serde::{Deserialize, Serialize};

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
/// A detect frame is not always the whole screen: the placed panel rect is
/// padded and cropped before OCR, and Windows OCR reports line boxes in the
/// pixels it was handed — CROP-relative. Every rule downstream of the OCR is
/// screen-absolute: [`panel_bounds`] and [`header_guard_bounds`] feed `run.rs`'s
/// cursor tests, and the cell rects end up in `MercCapture` where the hover tick
/// hit-tests them against the real cursor. Mixing the two spaces would not fail
/// loudly: it would read as "the panel moved", every frame, for ever.
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

/// Which of the two grabs a [`Frame`] describes.
///
/// A LABEL for the log and the one bit `to_screen` branches on, in one field
/// rather than two booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// The whole screen.
    Full,
    /// A crop of a KNOWN panel, re-read on the live cadence.
    Crop,
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

    /// The whole screen's size in px.
    pub fn screen(&self) -> [u32; 2] {
        self.screen
    }

    /// `full` or `crop`, for the log.
    pub fn describe(&self) -> &'static str {
        match self.kind {
            FrameKind::Full => "full",
            FrameKind::Crop => "crop",
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
    /// The left skill-icon column, immediately before the name column. The
    /// reader uses this rect only as the independent on-screen row sensor.
    pub skill_icon: [i32; 4],
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
    /// Runtime scale, and the ONE number every derived rect is measured in.
    ///
    /// [`placed_layout`] receives the SSOT placement scale; the explicit
    /// full-screen fallback [`detect_reason`] derives it from the observed row
    /// pitch ÷ [`MercGeometry::row_pitch`]. [`super::cellfit::refine`] then
    /// REPLACES either estimate with the support grid's frame-measured scale
    /// when it can find the frame (POE-214 D1), which is why
    /// [`Self::scale_source`] sits beside it — the cues disagree by ~3 %, and a
    /// reader of a log or a debug report has no other way to tell which one a
    /// capture was read at.
    pub scale: f32,
    /// Which cue produced [`Self::scale`]. `Ocr` until the frame fit lands.
    pub scale_source: super::ScaleSource,
    /// The skill-name column's left edge — the ONE x every cell is measured
    /// from. Never a single row's own x: a leading glyph's side bearing
    /// shifts one row by a couple of px and would skew that row's cells.
    pub column_x0: i32,
    /// The pitch the rows were laid out from — which of the two detect paths
    /// wrote it decides what that means. `detect_reason` writes the OBSERVED
    /// OCR line pitch, before the scale division, and its rows sit on the line
    /// centres that measured it. `placed_layout` writes the ENUMERATION pitch
    /// (a held fit's, else `row_pitch · scale`); there matched OCR lines
    /// replace individual centres, so the rows' final spacing is NOT this
    /// number.
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

/// [`detect_reason`] as an `Option`, for the tests that only ask whether a
/// frame detects. Production reads the reason.
#[cfg(test)]
pub fn detect(
    lines: &[OcrLineBox],
    g: &MercGeometry,
    vocab: &MercVocab,
    known_panel: Option<[i32; 4]>,
) -> Option<MercLayout> {
    detect_reason(lines, g, vocab, known_panel).ok()
}

/// Detect a recruit window in a screen's OCR lines.
///
/// Returns `Err` — never a partial guess — when any of the D2 preconditions
/// fails: fewer than [`MercGeometry::min_skill_candidates`] skill-name
/// candidates, or no panel anchor. The anchor is the discriminator against
/// every other PoE surface that lists skill names (a gem tooltip, the
/// character panel): those have skill text but no wager, no recruit verdict
/// and no recruit buttons. Any of THREE chrome lines anchors — see
/// [`AnchorKind`]: "Wager" above row 1 ([`is_wager_line`]), the recruit
/// verdict above row 1 ([`is_recruit_verdict_line`]), or a
/// "TAKE ITEM" / "REMATCH" button below the last row ([`is_button_line`]).
///
/// Each of the three has been measured missing on its own, which is why there
/// are three:
///
/// - 2026-08-24, 1920×1200: Windows OCR returned NO line for `Wager: 8 831`
///   (small gold text on the dark panel) while both buttons read cleanly;
/// - 2026-08-27, 1920×1080 (POE-217): the buttons were HIDDEN under the
///   player's skill bar and the wager read `Waggr: 6 231`, while
///   `Should Recruit` read clean.
///
/// `known_panel` is the rect the LAST detect of the capture still on screen
/// produced ([`panel_bounds`]), and it is a fourth anchor — see
/// [`panel_anchor`]. `None` means there is no live capture, and
/// then a frame anchors on its own chrome or not at all.
///
/// The `Err` carries the reason a miss missed.
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
    //    within `wager_search_pitches` of it reading "Wager" or the recruit
    //    verdict, or a button line below the last row within the same reach —
    //    unless the rows are sitting in a panel we already found, which is an
    //    anchor in its own right.
    let first_centre = centres[0];
    let last_centre = centres[centres.len() - 1];
    let panel = panel_anchor(known_panel, &centres, column_x0, g, scale);
    if panel != PanelAnchor::Anchored {
        let reach = if row_pitch > 0.0 {
            g.wager_search_pitches * row_pitch
        } else {
            g.wager_search_pitches * g.row_pitch * scale
        };
        let anchor = lines
            .iter()
            .find_map(|l| text_anchor_at(l, first_centre, last_centre, reach, g));
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
                skill_icon: skill_icon_rect(column_x0, centre, cell_size),
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
        // The OCR line pitch is the only cue this function has; the frame fit
        // that overwrites it needs pixels and runs after (`super::cellfit`).
        scale_source: super::ScaleSource::Ocr,
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
    /// recruit-verdict line, no button line, and `panel` says why the
    /// known-panel anchor abstained. This is the shape a tooltip over the
    /// footer produces.
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
                write!(
                    f,
                    "{rows} row(s), no wager, recruit-verdict or button line, known panel: {panel}"
                )
            }
        }
    }
}

/// What the known-panel anchor made of this frame's rows — [`panel_anchor`]'s
/// answer, and the sub-predicate that said no when it said no.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelAnchor {
    /// Every row centre and the sampled skill-column point are inside the known
    /// rect. The frame is anchored.
    Anchored,
    /// No live capture, so no rect to weigh anything against.
    NoKnownRect,
    /// A rect, but no rows to place in it.
    NoRows,
    /// The placed crop contains name lines, but their median x is outside the
    /// half-cell tolerance of the placed column. The caller must spend the
    /// full-screen fallback rather than publish the shifted geometry.
    ColumnMoved { column_x: i32, expected_x: i32, tolerance: i32 },
    /// A placed rect supplied the rows, but no wager, verdict or button line
    /// corroborated that the recruit window is open.
    NoChrome,
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
            PanelAnchor::NoChrome => write!(f, "no chrome"),
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
/// - the whole-line bar was far too loose for a five-letter word, so the score
///   is taken against the head alone and the bar is
///   `thresholds.wager_anchor`. That threshold's own doc carries the measured
///   scores and why it now sits at 0.90 rather than 0.98 (POE-217).
///
/// The head score is not the whole test. The label the panel draws is ALWAYS
/// followed by an amount, so what is left of the line after the head word must
/// carry at least one ASCII digit. That is what refuses the words the 0.90 bar
/// admits — `Wagner` (0.961), `Wagers` (0.972), `Wage` (0.960) — as they
/// appear in prose and in chat, and it refuses a bare `Wager` too: a wager
/// label with no amount is not the line this panel draws.
pub fn is_wager_line(text: &str, g: &MercGeometry) -> bool {
    match head_and_rest(text) {
        Some((head, rest)) => {
            strsim::jaro_winkler(&head, "wager") as f32 >= g.thresholds.wager_anchor
                && rest.chars().any(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

/// The line's head word — cut exactly as [`anchor_words`] cuts it — paired
/// with the rest of the line, starting the character after that word's
/// alphanumeric run.
///
/// `None` when the line holds no word at all. Split out from [`anchor_words`]
/// because [`is_wager_line`] asks two different questions of the two halves:
/// the head is fuzzy-matched against the label, the tail is searched for the
/// amount. The cut falls INSIDE the token, not at the whitespace after it, so
/// a `Wager:1028` OCR returned with no space still has its amount in the tail.
fn head_and_rest(text: &str) -> Option<(String, &str)> {
    let mut offset = 0;
    while offset < text.len() {
        let start = text.len() - text[offset..].trim_start().len();
        let token_end = text[start..]
            .find(char::is_whitespace)
            .map_or(text.len(), |i| start + i);
        let run: usize = text[start..token_end]
            .chars()
            .take_while(|c| c.is_alphanumeric())
            .map(char::len_utf8)
            .sum();
        if run > 0 {
            return Some((text[start..start + run].to_lowercase(), &text[start + run..]));
        }
        offset = token_end;
    }
    None
}

/// A line's words, lowercased and each cut at its first non-alphanumeric
/// character, with the empties dropped.
///
/// The cut is what makes `Wager:` and `Wager:1` both reduce to `wager`, and
/// what lets `Recruit.` still read as `recruit`. Dropping the empties is what
/// keeps a leading glyph OCR read as punctuation ("/ Infamous Earthshaker")
/// from becoming the head word.
fn anchor_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.to_lowercase()
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Jaro-Winkler bar the recruit verdict's two anchor words must each clear.
///
/// MEASURED 2026-08-27 (POE-217, 1920×1080): the line itself OCR'd clean as
/// `Should Recruit`, so the fuzz is not paying for an observed error — it is
/// slack for the SAME one-glyph substitution class the same dump produced on
/// the wager (`Wager` → `Waggr`). At 0.90 a one-glyph error in either word is
/// still admitted (`shou1d` 0.933, `recrult` 0.952) while the words this panel
/// actually sits among are nowhere near: `complete` scores 0.528 against
/// "should" and `incursions` 0.574 against "recruit".
const RECRUIT_VERDICT_FUZZ: f32 = 0.90;

/// Whether a line reads as the recruit panel's verdict, "Should Recruit" or
/// "Should Not Recruit".
///
/// The THIRD text anchor, added because the 2026-08-27 dump had neither of the
/// other two: the footer buttons were hidden under the player's skill bar and
/// the wager read `Waggr: 6 231`. This line was still clean, and it is chrome
/// the recruit window alone draws.
///
/// Scored on the FIRST and LAST words only, each against
/// [`RECRUIT_VERDICT_FUZZ`], so that the "Not" of the refusal wording — and
/// anything else the panel wraps between them — costs nothing. Two words are
/// required: "Recruit" on its own is an inventory verb and an ordinary chat
/// word, and scoring a single token as both ends would let it through.
pub fn is_recruit_verdict_line(text: &str, _g: &MercGeometry) -> bool {
    let words = anchor_words(text);
    if words.len() < 2 {
        return false;
    }
    let first = &words[0];
    let last = &words[words.len() - 1];
    strsim::jaro_winkler(first, "should") as f32 >= RECRUIT_VERDICT_FUZZ
        && strsim::jaro_winkler(last, "recruit") as f32 >= RECRUIT_VERDICT_FUZZ
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

/// Which of [`detect_reason`]'s three TEXT anchors a line answered as.
///
/// A label, for the debug report and the log line — nothing branches on it.
/// It exists because "no anchor" and "anchored" were the only two things a
/// dump could say, and the POE-217 incident turned on WHICH of the three had
/// survived the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnchorKind {
    /// [`is_wager_line`], above row 1.
    Wager,
    /// [`is_recruit_verdict_line`], above row 1.
    RecruitVerdict,
    /// [`is_button_line`], below the last row.
    Button,
}

impl std::fmt::Display for AnchorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnchorKind::Wager => write!(f, "wager"),
            AnchorKind::RecruitVerdict => write!(f, "recruit verdict"),
            AnchorKind::Button => write!(f, "button"),
        }
    }
}

/// Which text anchor a line reads as WITHOUT the positional test — the whole
/// of [`detect_reason`]'s step 4 rule except "is it near the rows".
///
/// Split out for the one caller that has no rows to measure against: the debug
/// dump reports the anchor a frame carried even when the detect went on to
/// reject it, and reporting "missing" for a line that was there but misplaced
/// sent POE-217's diagnosis at the wrong predicate.
pub fn text_anchor(text: &str, g: &MercGeometry) -> Option<AnchorKind> {
    if is_wager_line(text, g) {
        Some(AnchorKind::Wager)
    } else if is_recruit_verdict_line(text, g) {
        Some(AnchorKind::RecruitVerdict)
    } else if is_button_line(text, g) {
        Some(AnchorKind::Button)
    } else {
        None
    }
}

/// [`text_anchor`] plus [`detect_reason`]'s positional test: the wager and the
/// recruit verdict must sit ABOVE row 1 within `reach`, a button BELOW the
/// last row within the same reach.
///
/// The side matters. Both above-lines are panel chrome drawn over the header
/// band, and both buttons live under the grid; accepting either on the wrong
/// side would let a line from whatever the game drew below the panel stand in
/// for the panel's own.
fn text_anchor_at(
    l: &OcrLineBox,
    first_centre: f32,
    last_centre: f32,
    reach: f32,
    g: &MercGeometry,
) -> Option<AnchorKind> {
    let c = l.centre_y();
    let above = c < first_centre && first_centre - c <= reach;
    let below = c > last_centre && c - last_centre <= reach;
    match text_anchor(&l.text, g)? {
        AnchorKind::Wager if above => Some(AnchorKind::Wager),
        AnchorKind::RecruitVerdict if above => Some(AnchorKind::RecruitVerdict),
        AnchorKind::Button if below => Some(AnchorKind::Button),
        _ => None,
    }
}

/// Whether every detected row centre remains inside the last live panel rect.
///
/// This is retained for the detect-reason regression suite and the occlusion
/// path. The occlusion shape it exists for, measured on the 2026-08-26 smoke:
/// a tooltip drawn over the lower rows leaves the header, the wager line and
/// the top two of six skill rows, and the frame then detects as a two-row
/// layout for a six-row window. Normal placed reads use [`placed_layout`] and do not locate or grow a
/// session rectangle from OCR rows.
pub(super) fn panel_anchor(
    rect: Option<[i32; 4]>,
    centres: &[f32],
    column_x0: f32,
    _g: &MercGeometry,
    _scale: f32,
) -> PanelAnchor {
    let Some(rect) = rect else {
        return PanelAnchor::NoKnownRect;
    };
    if centres.is_empty() {
        return PanelAnchor::NoRows;
    }
    match centres
        .iter()
        .find(|&&centre| !contains(rect, (column_x0.round() as i32, centre.round() as i32)))
    {
        Some(&centre) => PanelAnchor::RowOutside { centre: centre.round() as i32 },
        None => PanelAnchor::Anchored,
    }
}

/// The placed-vs-located origin tolerance, in reference cell fractions.
///
/// This is the former panel-anchor column tolerance: half a cell at the
/// current fitted scale. The fallback applies the same band to x and y so a
/// located panel that is only OCR/frame jitter is not persisted as a move.
pub const PLACED_PANEL_TOLERANCE_CELLS: f32 = 0.5;

/// Half a cell at this capture's scale.
pub(super) fn column_tolerance(g: &MercGeometry, scale: f32) -> i32 {
    (g.cell_size * scale * PLACED_PANEL_TOLERANCE_CELLS).round().max(1.0) as i32
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

/// Padding around the SSOT panel placement, in the 1920×1200 reference unit.
///
/// The horizontal 197 px is four reference row pitches, matching the header
/// parser's x-window. The 615 px top reach is the seed's own y coordinate, so
/// the crop reaches y=0 on the reference screen and covers about 45% of the
/// screen, matching ADR-024's crop-size statement. The seed's footer already
/// includes the button band, so no bottom padding is added. This is crop
/// padding, not a second placement or a remembered rectangle.
pub const PLACED_PANEL_PADDING_REF: [i32; 4] = [197, 615, 197, 0];

/// Expand the SSOT panel placement into the OCR crop for this screen.
pub fn placed_panel_crop(panel: [i32; 4], scale: f32, screen: [u32; 2]) -> [i32; 4] {
    let [x, y, w, h] = panel;
    let [left, top, right, bottom] = PLACED_PANEL_PADDING_REF;
    let left = (left as f32 * scale).round().max(0.0) as i32;
    let top = (top as f32 * scale).round().max(0.0) as i32;
    let right = (right as f32 * scale).round().max(0.0) as i32;
    let bottom = (bottom as f32 * scale).round().max(0.0) as i32;
    let width = screen[0].max(1) as i32;
    let height = screen[1].max(1) as i32;
    let x0 = (x - left).clamp(0, width - 1);
    let y0 = (y - top).clamp(0, height - 1);
    let x1 = (x + w + right).clamp(x0 + 1, width);
    let y1 = (y + h + bottom).clamp(y0 + 1, height);
    [x0, y0, x1 - x0, y1 - y0]
}

/// Whether a full-screen locate is far enough from the placed origin to be a
/// real geometry contradiction. The band is the existing half-cell column
/// tolerance, applied to both axes; rect width and height are not compared
/// because the SSOT seed size is intentionally provisional.
pub fn placed_panel_contradicted(
    placed: [i32; 4],
    located: [i32; 4],
    g: &MercGeometry,
    scale: f32,
) -> bool {
    let tolerance = column_tolerance(g, scale);
    (placed[0] - located[0]).abs() > tolerance || (placed[1] - located[1]).abs() > tolerance
}

/// Geometry row centres for a placed panel.
///
/// The first centre is `panel.y + fitted_pitch + fitted_cell_size / 2`. The
/// last centre is bounded by both the placed footer (`3 * pitch + cell/2`)
/// and the first TAKE ITEM/REMATCH baseline when OCR supplied one. The count
/// is rounded from that interval and capped by `max_rows`; no pass-1 skill hit
/// can add or remove a row.
pub fn placed_row_centres(
    panel: [i32; 4],
    g: &MercGeometry,
    scale: f32,
    fitted_pitch: f32,
    button_y: Option<f32>,
) -> Vec<f32> {
    if !scale.is_finite() || scale <= 0.0 || !fitted_pitch.is_finite() || fitted_pitch <= 0.0 {
        return Vec::new();
    }
    let pitch = fitted_pitch;
    let cell = (g.cell_size * scale).max(1.0);
    let first = panel[1] as f32 + pitch + cell / 2.0;
    let footer_last = panel[1] as f32 + panel[3] as f32
        - PANEL_FOOTER_PITCHES * pitch
        - cell / 2.0;
    let button_last = button_y.map_or(f32::INFINITY, |y| y - pitch);
    let last = footer_last.min(button_last);
    if last < first {
        return Vec::new();
    }
    let count = (((last - first) / pitch).round() as usize + 1).min(g.max_rows as usize);
    (0..count).map(|i| first + i as f32 * pitch).collect()
}

struct PlacedRowGeometry {
    centre_y: f32,
    skill_icon: [i32; 4],
    name_rect: [i32; 4],
    cells: Vec<[i32; 4]>,
    band: [i32; 4],
}

fn placed_row_geometry(
    panel: [i32; 4],
    g: &MercGeometry,
    scale: f32,
    centres: &[f32],
) -> Vec<PlacedRowGeometry> {
    let cell_size = (g.cell_size * scale).round().max(1.0) as i32;
    let line_height = (g.ref_line_height * scale).round().max(1.0) as i32;
    let column_x0 = panel[0] as f32 + g.cell_size * scale * PANEL_MARGIN_CELLS;
    let cell_x0 = column_x0 + g.cell_offset_x * scale;
    let name_x0 = column_x0.round() as i32;
    let name_w = (cell_x0 - column_x0 - (4.0 * scale)).round().max(1.0) as i32;

    centres
        .iter()
        .map(|&centre| {
            let name_top = (centre - line_height as f32).round() as i32;
            let name_rect = [name_x0, name_top, name_w, line_height * 2];
            let cells = (0..g.max_slots)
                .map(|slot| {
                    let x = cell_x0 + slot as f32 * g.cell_pitch * scale;
                    [
                        x.round() as i32,
                        (centre - cell_size as f32 / 2.0).round() as i32,
                        cell_size,
                        cell_size,
                    ]
                })
                .collect::<Vec<_>>();
            let top = cells
                .first()
                .map_or(name_rect[1], |cell| name_rect[1].min(cell[1]));
            let right = cells.last().map_or(
                name_rect[0] + name_rect[2],
                |cell| (cell[0] + cell[2]).max(name_rect[0] + name_rect[2]),
            );
            let bottom = cells.last().map_or(
                name_rect[1] + name_rect[3],
                |cell| (cell[1] + cell[3]).max(name_rect[1] + name_rect[3]),
            );

            PlacedRowGeometry {
                centre_y: centre,
                skill_icon: skill_icon_rect(column_x0, centre, cell_size),
                name_rect,
                cells,
                band: [name_rect[0], top, right - name_rect[0], bottom - top],
            }
        })
        .collect()
}

/// Preview bands for the placed panel's fixed row geometry. This uses the same
/// cell/name projection as [`placed_layout`], but has no OCR lines to seed it.
pub fn placed_row_rects(
    panel: [i32; 4],
    g: &MercGeometry,
    scale: f32,
    fitted_pitch: f32,
    button_y: Option<f32>,
) -> Vec<[i32; 4]> {
    let centres = placed_row_centres(panel, g, scale, fitted_pitch, button_y);
    placed_row_geometry(panel, g, scale, &centres)
        .into_iter()
        .map(|row| row.band)
        .collect()
}

fn placed_skill_name_lines<'a>(
    lines: &'a [OcrLineBox],
    g: &MercGeometry,
    panel: [i32; 4],
    scale: f32,
    pitch: f32,
    centres: &[f32],
) -> Vec<&'a OcrLineBox> {
    let column_x0 = panel[0] as f32 + g.cell_size * scale * PANEL_MARGIN_CELLS;
    let line_height = (g.ref_line_height * scale).round().max(1.0) as f32;
    let cell_x0 = column_x0 + g.cell_offset_x * scale;
    lines
        .iter()
        .filter(|line| !matches!(text_anchor(&line.text, g), Some(_)))
        .filter(|line| {
            line.x as f32 >= column_x0 - line_height
                && line.x as f32 <= cell_x0
                && centres
                    .iter()
                    .any(|&centre| (line.centre_y() - centre).abs() <= pitch * 0.45)
        })
        .collect()
}

/// Candidate lines for the placed-column contradiction detector. This window
/// stays crop-wide because a moved column is the case where the geometry's
/// narrow skill-name band excludes the evidence we need to reject it.
fn placed_contradiction_lines<'a>(
    lines: &'a [OcrLineBox],
    g: &MercGeometry,
    panel: [i32; 4],
    scale: f32,
    pitch: f32,
    centres: &[f32],
) -> Vec<&'a OcrLineBox> {
    let horizontal_pad = (PLACED_PANEL_PADDING_REF[0] as f32 * scale).round();
    let crop_left = panel[0] as f32 - horizontal_pad;
    let crop_right = panel[0] as f32 + panel[2] as f32 + horizontal_pad;
    lines
        .iter()
        .filter(|line| !matches!(text_anchor(&line.text, g), Some(_)))
        .filter(|line| {
            line.x as f32 >= crop_left
                && line.x as f32 <= crop_right
                && centres
                    .iter()
                    .any(|&centre| (line.centre_y() - centre).abs() <= pitch * 0.45)
        })
        .collect()
}

/// Build the live layout from a placed panel and its fitted row pitch.
///
/// Chrome is still required as the open-window proof, but skill-name OCR is
/// only text assigned to already-known geometry bands. The geometry supplies
/// the row count; each band's centre is replaced by the mean of its matched
/// skill-name line centres when one exists, while an unread row keeps its
/// geometry centre. A mangled or absent pass-1 name therefore produces an
/// unread row rather than deleting a row.
pub fn placed_layout(
    lines: &[OcrLineBox],
    panel: [i32; 4],
    g: &MercGeometry,
    scale: f32,
    fitted_pitch: f32,
) -> Result<MercLayout, DetectMiss> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(DetectMiss {
            candidates: 0,
            column_x0: None,
            stage: DetectStage::BadScale { scale },
        });
    }
    let pitch = fitted_pitch;
    let seed_centres = placed_row_centres(panel, g, scale, pitch, None);
    let button_y = lines
        .iter()
        .filter(|line| is_button_line(&line.text, g))
        .map(OcrLineBox::centre_y)
        .filter(|&y| seed_centres.last().is_some_and(|last| y > *last))
        .min_by(|a, b| a.total_cmp(b));
    let centres = placed_row_centres(panel, g, scale, pitch, button_y);
    if centres.is_empty() {
        return Err(DetectMiss {
            candidates: 0,
            column_x0: None,
            stage: DetectStage::NoRowClusters,
        });
    }

    let first = centres[0];
    let last = *centres.last().expect("placed rows are non-empty");
    let reach = g.wager_search_pitches * pitch;
    if lines
        .iter()
        .find_map(|line| text_anchor_at(line, first, last, reach, g))
        .is_none()
    {
        return Err(DetectMiss {
            candidates: 0,
            column_x0: Some(panel[0] as f32),
            stage: DetectStage::NoAnchor { rows: centres.len(), panel: PanelAnchor::NoChrome },
        });
    }

    let column_x0 = panel[0] as f32 + g.cell_size * scale * PANEL_MARGIN_CELLS;
    let mut observed_name_xs =
        placed_contradiction_lines(lines, g, panel, scale, pitch, &centres)
        .into_iter()
        .map(|line| line.x as f32)
        .collect::<Vec<_>>();
    if !observed_name_xs.is_empty() {
        let observed_column_x = median(&mut observed_name_xs);
        let tolerance = column_tolerance(g, scale);
        let expected_x = column_x0.round() as i32;
        let observed_x = observed_column_x.round() as i32;
        if (observed_x - expected_x).abs() > tolerance {
            return Err(DetectMiss {
                candidates: observed_name_xs.len(),
                column_x0: Some(observed_column_x),
                stage: DetectStage::NoAnchor {
                    rows: centres.len(),
                    panel: PanelAnchor::ColumnMoved {
                        column_x: observed_x,
                        expected_x,
                        tolerance,
                    },
                },
            });
        }
    }
    let seeded_centres = centres
        .iter()
        .map(|&geometry_centre| {
            let matched = placed_skill_name_lines(
                lines,
                g,
                panel,
                scale,
                pitch,
                &[geometry_centre],
            );
            if matched.is_empty() {
                geometry_centre
            } else {
                matched.iter().map(|line| line.centre_y()).sum::<f32>() / matched.len() as f32
            }
        })
        .collect::<Vec<_>>();
    let row_geometry = placed_row_geometry(panel, g, scale, &seeded_centres);
    let rows = row_geometry
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let text = lines
                .iter()
                .filter(|line| {
                    !matches!(text_anchor(&line.text, g), Some(_))
                        && (line.centre_y() - row.centre_y).abs() <= pitch * 0.45
                        && line.x as f32 >= column_x0 - (g.ref_line_height * scale).round().max(1.0)
                        && line.x as f32 <= column_x0 + g.cell_offset_x * scale
                })
                .map(|line| line.text.trim())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            MercLayoutRow {
                index: i as u8,
                centre_y: row.centre_y,
                skill_icon: row.skill_icon,
                name_rect: row.name_rect,
                text,
                cells: row.cells.clone(),
            }
        })
        .collect::<Vec<_>>();

    Ok(MercLayout {
        scale,
        scale_source: super::ScaleSource::Ocr,
        column_x0: column_x0.round() as i32,
        row_pitch: pitch,
        header: parse_header(lines, first, &rows, column_x0, pitch),
        rows,
    })
}

/// The row's left skill-icon sensor rect, derived from the name column and the
/// fitted cell size. It deliberately overlaps the icon's right side: the
/// screen can crop the panel's left edge, while the existing occupancy rule
/// correctly rejects a rect with no readable pixels.
fn skill_icon_rect(column_x0: f32, centre: f32, cell_size: i32) -> [i32; 4] {
    [
        column_x0.round() as i32 - cell_size,
        (centre - cell_size as f32 / 2.0).round() as i32,
        cell_size,
        cell_size,
    ]
}

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
/// which [`detect_reason`] never produces.
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
/// [`PANEL_FOOTER_PITCHES`] below the last. It still UNDER-reaches upward — an
/// above-the-rows text anchor (the wager line, the recruit verdict) can sit up
/// to `wager_search_pitches` (12) above row 1 — and the
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

/// Whether the placed recruit crop OCR saw any recruit-window chrome.
///
/// The crop is already anchored by the SSOT panel placement, so any of the
/// three text anchors is enough for the cheap voice-gate proof.
pub fn probe_hit(lines: &[OcrLineBox], g: &MercGeometry) -> bool {
    lines.iter().any(|l| text_anchor(&l.text, g).is_some())
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

/// One OCR line at a centre. Height 16 and width 8·len are the reference
/// panel's cap height and rough advance — the tests care about the RECTS the
/// geometry derives, not about glyph metrics.
#[cfg(test)]
pub(super) fn test_line(text: &str, x: i32, centre_y: i32) -> OcrLineBox {
    OcrLineBox {
        text: text.to_string(),
        x,
        y: centre_y - 8,
        w: text.len() as i32 * 8,
        h: 16,
    }
}

/// The reference panel as the OCR would report it: the measured line centres of
/// `scratchpad/recruit-cai.png` (full-image px), the wrapped fourth name as the
/// two lines it really is, and the header lines above. Every geometry assertion
/// below is derived from THESE inputs, never echoed from `MercGeometry`'s
/// constants.
///
/// `pub(super)` because the committed fixture
/// `tests/fixtures/merc-skills-panel.png` is the (60,585) crop of that same
/// screen, so these lines are the OCR half of the ONE real-pixel ground truth
/// the module has — `cellfit`'s reference test needs both halves.
#[cfg(test)]
pub(super) fn reference_lines() -> Vec<OcrLineBox> {
    vec![
        test_line("Cai, the Lout", 285, 30),
        test_line("Shock Ambusher", 200, 73),
        test_line("Lvl 70", 385, 73),
        test_line("Dex / Int", 530, 73),
        test_line("Wager: 1 028", 80, 173),
        test_line("Conductivity", 134, 620),
        test_line("Vaal Lightning Trap", 134, 669),
        test_line("Lightning Spire Trap", 134, 717),
        test_line("Ball Lightning of Orbiting", 134, 757),
        test_line("Trap", 134, 775),
        test_line("Summon Skitterbots", 134, 814),
        test_line("Flame Dash", 134, 862),
    ]
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

/// A cell's inner region — the outer rect minus the gold frame drawn around a
/// FILLED slot, whose bright line would otherwise raise an empty slot's stddev
/// past the occupancy threshold.
///
/// An empty slot carries no frame at all (measured on both committed fixtures:
/// the dark|light step this inset exists to remove is present only where an
/// icon is). The inset is applied unconditionally anyway, because occupancy is
/// decided AFTER it and the rule has to read the same region either way.
pub fn inner_rect(rect: [i32; 4], g: &MercGeometry) -> [i32; 4] {
    let inset = g.cell_inset.round() as i32;
    [
        rect[0] + inset,
        rect[1] + inset,
        (rect[2] - 2 * inset).max(1),
        (rect[3] - 2 * inset).max(1),
    ]
}

/// The OUTER cell rect an inner crop of `inner_side` px came out of, and the
/// inset that produced it — [`inner_rect`] run backwards.
///
/// The rule, stated once here and referenced everywhere else: **`outer =
/// inner + 2 · inset`**, where the inset is [`MercGeometry::cell_inset`]
/// rounded. A reader that holds a stored inner crop and wants to hand it to a
/// production entry point (which all take OUTER rects) rebuilds the cell that
/// way instead of hard-coding a pair of numbers, because the pair is different
/// on every machine: the laptop cuts 39 inside 43 and the PC 36 inside 40.
///
/// The alignment window the matcher then gets from that cell is
/// `window = inner − 2 · SHIFT_MAX` (`icons::shift_window`), so an
/// inner side under `SIG_DIM + 2 · SHIFT_MAX` leaves no window at all and the
/// matcher falls back to a single unaligned signature.
#[allow(dead_code)] // Only the corpus readers reach this; comes off with its first production caller.
pub(super) fn outer_rect_for_inner(inner_side: u32, g: &MercGeometry) -> (u32, i32) {
    let inset = g.cell_inset.round() as i32;
    ((inner_side as i32 + 2 * inset).max(1) as u32, inset)
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
/// call. `icons::normalize_cell` does not reduce to luma at ALL since format 2
/// (POE-207) — it keeps RGB, because the gold frame every cell shares dominates
/// any single-channel reduction and made visibly different icons correlate at
/// 0.97-0.99. `icons::read_tier` still calls this: the badge's ink mask is a
/// brightness threshold, and it was measured against THIS weighting.
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
    /// 18 px apart (inside 2 × 16), and the pitch is the median of the five
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
        lines.push(test_line("has entered the area", 134, 1200));

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
        lines.push(test_line("Inventory", 134, 400));

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
        lines.push(test_line("Trap", 134, 880));

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

        assert_eq!(layout.rows.len(), 6);
        assert_eq!(layout.rows[5].text, "Ball Lightning of Orbiting Trap");
    }

    /// The 1080p wrap measured on 2026-09-01 (app.log 11:10:23 and the 15:03
    /// debug capture, both `7 rows, scale 0.791`): rows 43-44 px apart, the two
    /// lines of a wrapped name 17 px apart, the small-caps names boxed at 11 px
    /// by the OCR. A merge window of 1.5 line heights is 16.5 px there, so the
    /// continuation became a seventh row and the pitch median fell from 43.5 to
    /// 39 - the exact `rowPitch` that capture reported.
    #[test]
    fn a_wrapped_name_at_1080p_line_heights_is_one_row() {
        let lines = vec![
            tall("Wager: 6 539", 100, 174, 11),
            tall("Conductivity", 170, 574, 11),
            tall("Vaal Lightning Trap", 170, 617, 11),
            tall("Ball Lightning of Orbiting", 170, 652, 11),
            tall("Trap", 170, 669, 11),
            tall("Summon Skitterbots", 170, 704, 11),
            tall("Flame Dash", 170, 748, 11),
            tall("Lightning Spire Trap", 170, 791, 11),
        ];

        let layout = detect(&lines, &MercGeometry::default(), &vocab(), None).expect("detected");

        assert_eq!(layout.rows.len(), 6, "the continuation line must join its row");
        assert_eq!(layout.rows[2].text, "Ball Lightning of Orbiting Trap");
        assert_eq!(layout.row_pitch, 43.5);
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
        lines.push(test_line("TAKE ITEM", 250, 880));
        lines.push(test_line("REMATCH", 420, 880));

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
        lines.push(test_line("REMATCH", 420, 814 + (20.0 * 48.0) as i32));
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

    /// The fallback's known-panel anchor survives a frame whose OCR lost every
    /// chrome line: rows inside the last panel rect are enough to identify it.
    #[test]
    fn detect_reason_anchors_rows_inside_the_known_panel_without_chrome() {
        let g = MercGeometry::default();
        let rect = panel_bounds(
            &detect_reason(&reference_lines(), &g, &vocab(), None).expect("the reference panel"),
            &g,
        )
        .expect("the reference panel has bounds");
        let chromeless: Vec<OcrLineBox> = reference_lines()
            .into_iter()
            .filter(|line| text_anchor(&line.text, &g).is_none())
            .collect();

        let layout = detect_reason(&chromeless, &g, &vocab(), Some(rect))
            .expect("the known panel rect anchors its rows");

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

    /// The reference panel with its wager line gone — what a tooltip over the
    /// chrome leaves, and the frame that has nothing but the known rect to
    /// anchor on.
    fn chromeless_reference_lines() -> Vec<OcrLineBox> {
        reference_lines()
            .into_iter()
            .filter(|l| !l.text.starts_with("Wager"))
            .collect()
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

        let why = detect_reason(&[test_line("Vika has entered the area", 10, 10)], &g, &vocab(), None)
            .expect_err("no candidates, no layout");

        assert_eq!(why.candidates, 0);
        assert_eq!(why.column_x0, None);
        assert_eq!(
            why.stage,
            DetectStage::TooFewCandidates { needed: g.min_skill_candidates }
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

    #[test]
    fn the_placed_crop_is_seeded_from_the_ssot_reference_rect() {
        let screen = crate::ssot::ScreenSlice {
            width: 1920,
            height: 1200,
            ui_scale: 1.0,
            source: crate::ssot::ScreenScaleSource::MercFrame,
            measured_at_ms: 0,
            verified_this_session: true,
            monitor_id: 1,
            origin: (0, 0),
            client: [0, 0, 1920, 1200],
            anchors: None,
        };
        let panel = crate::ssot::placements(&screen).merc.expect("merc placement").panel;

        assert_eq!(panel, crate::ssot::MERC_PANEL_REF);
        assert_eq!(placed_panel_crop(panel, 1.0, [1920, 1200]), [501, 0, 949, 1092]);
    }

    #[test]
    fn a_located_panel_inside_the_half_cell_band_is_not_a_contradiction() {
        let g = MercGeometry::default();
        let placed = crate::ssot::MERC_PANEL_REF;

        assert!(!placed_panel_contradicted(placed, [719, 616, 555, 477], &g, 1.0));
        assert!(placed_panel_contradicted(placed, [721, 615, 555, 477], &g, 1.0));
    }

    #[test]
    fn a_placed_crop_offset_by_100_px_rejects_its_name_column() {
        let g = MercGeometry::default();
        let lines = vec![
            test_line("Wager: 8 831", 700, 580),
            test_line("Withering Step", 743, 616),
            test_line("Chaotic Burst", 743, 659),
            test_line("Chaotic Shot", 743, 703),
            test_line("Caustic Arrow", 743, 746),
            test_line("Trarthan Agility", 743, 790),
            test_line("Grace", 743, 833),
        ];
        let scale = 43.0 / g.row_pitch;
        let miss = placed_layout(&lines, [624, 554, 500, 429], &g, scale, 43.0)
            .expect_err("a crop shifted by 100 px must spend the fallback");

        assert_eq!(
            miss.stage,
            DetectStage::NoAnchor {
                rows: 6,
                panel: PanelAnchor::ColumnMoved {
                    column_x: 743,
                    expected_x: 643,
                    tolerance: 19,
                },
            }
        );
    }

    #[test]
    fn a_placed_crop_shifted_right_rejects_its_name_column() {
        let g = MercGeometry::default();
        let lines = crate::mercenary::cellfit::pc_lines();
        let scale = 43.0 / g.row_pitch;
        let miss = placed_layout(&lines, [824, 554, 500, 430], &g, scale, 43.0)
            .expect_err("a crop shifted right by 100 px must spend the fallback");

        assert_eq!(
            miss.stage,
            DetectStage::NoAnchor {
                rows: 6,
                // At the sibling test's scale 43.0 / 49.3 = 0.8722 the expected
                // column is 824 + 44 · 0.8722 / 2 = 843.2 and the tolerance
                // round(44 · 0.8722 · 0.5) = 19 — the mirror of the 643 / 19 above.
                panel: PanelAnchor::ColumnMoved {
                    column_x: 743,
                    expected_x: 843,
                    tolerance: 19,
                },
            }
        );
    }

    #[test]
    fn placed_rows_survive_a_mangled_pass_one_name_as_an_unread_row() {
        let g = MercGeometry::default();
        let panel = [724, 554, 500, 429];
        let mut lines = vec![
            test_line("Wager: 8 831", 700, 580),
            test_line("Withering Step", 743, 616),
            test_line("Chaotic Burst", 743, 659),
            test_line("Chaotic Shot", 743, 703),
            test_line("Caustic Arrow", 743, 746),
            test_line("Trarthan Agility", 743, 790),
            test_line("Grace", 743, 833),
        ];
        let shot = lines.iter_mut().find(|line| line.text == "Chaotic Shot").expect("chaotic shot row");
        shot.text = "MANGLED".into();

        let scale = 43.0 / g.row_pitch;
        let layout = placed_layout(&lines, panel, &g, scale, 43.0).expect("placed panel layout");
        assert_eq!(layout.rows.len(), 6);
        assert!(layout.rows.iter().any(|row| row.text == "MANGLED"));

        let img = image::load_from_memory(include_bytes!(
            "../../tests/fixtures/merc-recruit-pc-1080p.png"
        ))
        .expect("the committed PC recruit fixture loads");
        let texts = layout.rows.iter().map(|row| row.text.clone()).collect::<Vec<_>>();
        let result = crate::mercenary::read::build_capture(
            &img,
            Frame::cropped((700, 585), [1920, 1080]),
            &layout,
            &texts,
            0,
            &g,
            &vocab(),
            &crate::mercenary::icons::TemplateStore::new(),
        );
        let mangled = result
            .capture
            .rows
            .iter()
            .find(|row| row.skill.raw == "MANGLED")
            .expect("the geometry row remains in the capture");
        assert_eq!(mangled.skill.state, ReadState::Unknown);
        assert_eq!(result.rows_on_screen, 6, "the left skill-icon sensor sees all six rows");
        assert_eq!(result.rows_read, 5, "the mangled pass-1 name is the one unread row");
    }

    #[test]
    fn placed_rows_follow_pc_skill_line_centres_without_changing_the_geometry_count() {
        let g = MercGeometry::default();
        let layout = placed_layout(
            &crate::mercenary::cellfit::pc_lines(),
            [724, 554, 500, 430],
            &g,
            0.90,
            g.row_pitch * 0.90,
        )
        .expect("the PC placed panel layout");
        let expected = [616.5, 659.5, 703.5, 746.5, 790.5, 833.5];

        assert_eq!(layout.rows.len(), 6);
        for (row, expected_centre) in layout.rows.iter().zip(expected) {
            assert!(
                (row.centre_y - expected_centre).abs() <= 1.0,
                "row {} centre {} drifted from {expected_centre}",
                row.index,
                row.centre_y,
            );
        }
    }

    #[test]
    fn an_absent_pc_name_keeps_its_geometry_centre_as_an_unread_row() {
        let g = MercGeometry::default();
        let mut lines = crate::mercenary::cellfit::pc_lines();
        lines.retain(|line| line.text != "Trarthan Agility");
        let layout = placed_layout(
            &lines,
            [724, 554, 500, 430],
            &g,
            0.90,
            g.row_pitch * 0.90,
        )
        .expect("the PC placed panel layout");

        for (row_index, expected_centre) in [
            (0, 616.5),
            (1, 659.5),
            (2, 703.5),
            (3, 746.5),
            (5, 833.5),
        ] {
            assert!(
                (layout.rows[row_index].centre_y - expected_centre).abs() <= 1.0,
                "row {row_index} centre {} drifted from {expected_centre}",
                layout.rows[row_index].centre_y,
            );
        }
        // 554 + 44.37 + 20 + 4·44.37; placed_row_centres uses the unrounded
        // 19.8 half-cell.
        let geometry_centre = 795.65;
        assert!(
            (layout.rows[4].centre_y - geometry_centre).abs() <= 0.01,
            "row 4 must keep its geometry centre {geometry_centre}, got {}",
            layout.rows[4].centre_y,
        );
        assert!(layout.rows[4].text.is_empty(), "the absent name remains unread");
    }

    #[test]
    fn placed_geometry_with_no_button_trims_dark_trailing_rows_before_publish() {
        let g = MercGeometry::default();
        let lines = vec![
            test_line("Wager: 8 831", 700, 580),
            test_line("Withering Step", 743, 616),
            test_line("Chaotic Burst", 743, 659),
            test_line("Chaotic Shot", 743, 703),
            test_line("Caustic Arrow", 743, 746),
            test_line("Trarthan Agility", 743, 790),
            test_line("Grace", 743, 833),
        ];
        let scale = 43.0 / g.row_pitch;
        let layout = placed_layout(&lines, [724, 554, 500, 429], &g, scale, 43.0)
            .expect("placed geometry supplies the seed rows");
        let mut raw = RgbaImage::from_pixel(1920, 1080, Rgba([12, 12, 14, 255]));
        for row in layout.rows.iter().take(4) {
            for dy in 0..row.skill_icon[3] {
                for dx in 0..row.skill_icon[2] {
                    let value = if (dx / 3 + dy / 3) % 2 == 0 { 20 } else { 220 };
                    raw.put_pixel(
                        (row.skill_icon[0] + dx) as u32,
                        (row.skill_icon[1] + dy) as u32,
                        Rgba([value, value, value, 255]),
                    );
                }
            }
        }
        let image = DynamicImage::ImageRgba8(raw);
        let texts = layout.rows.iter().map(|row| row.text.clone()).collect::<Vec<_>>();
        let result = crate::mercenary::read::build_capture(
            &image,
            Frame::full([1920, 1080]),
            &layout,
            &texts,
            0,
            &g,
            &vocab(),
            &crate::mercenary::icons::TemplateStore::new(),
        );

        assert_eq!(result.capture.rows.len(), 4);
        assert_eq!(result.rows_on_screen, 4);
        assert_eq!(result.rows_read, 4);
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

    /// The 2026-08-27 Windows dump (1920×1080, POE-217,
    /// merc-debug/1787863871932) as OCR returned it. The frame the incident
    /// was reported on: BOTH of the anchors that existed at v0.8.0 were gone —
    /// the footer buttons were hidden under the player's skill bar and the
    /// wager label read `Waggr: 6 231` (head score 0.907, under the 0.98 bar
    /// of the day) — while `Should Recruit` read clean. `detect` answered
    /// `NoAnchor` and the capture was lost.
    ///
    /// All 55 lines, unedited, including the quest tracker and the map mods
    /// down the right-hand side: the noise is the point, since it is what a
    /// widened anchor rule has to keep refusing.
    fn koparu_dump_lines() -> Vec<OcrLineBox> {
        vec![
            OcrLineBox { text: "Koparu, the Keitan Chainbreaker".into(), x: 788, y: 75, w: 326, h: 24 },
            OcrLineBox { text: "/ Infamous Earthshaker".into(), x: 768, y: 117, w: 172, h: 17 },
            OcrLineBox { text: "LVI 83".into(), x: 979, y: 117, w: 40, h: 15 },
            OcrLineBox { text: ",LTH BOOTS".into(), x: 0, y: 115, w: 106, h: 17 },
            OcrLineBox { text: "EN WAND".into(), x: 4, y: 149, w: 87, h: 15 },
            OcrLineBox { text: "'065/3065".into(), x: 102, y: 809, w: 90, h: 22 },
            OcrLineBox { text: "Life".into(), x: 46, y: 811, w: 27, h: 14 },
            OcrLineBox { text: "Shield 2 0112229".into(), x: 45, y: 832, w: 145, h: 25 },
            OcrLineBox { text: "Str".into(), x: 1131, y: 118, w: 20, h: 13 },
            OcrLineBox { text: "Fighting in chains taught Koparu everything he needed - including how to".into(), x: 766, y: 144, w: 464, h: 16 },
            OcrLineBox { text: "break chem.".into(), x: 959, y: 161, w: 75, h: 13 },
            OcrLineBox { text: "Waggr: 6 231".into(), x: 708, y: 209, w: 102, h: 19 },
            OcrLineBox { text: "INTIMIDATINC CRY".into(), x: 743, y: 611, w: 119, h: 11 },
            OcrLineBox { text: "LEAP SIAM".into(), x: 743, y: 654, w: 64, h: 11 },
            OcrLineBox { text: "EARTHQUAKE OF AMPLI FICATION".into(), x: 743, y: 698, w: 199, h: 13 },
            OcrLineBox { text: "TECTONIC CASCADE".into(), x: 742, y: 741, w: 120, h: 11 },
            OcrLineBox { text: "DETERMINATION".into(), x: 743, y: 785, w: 103, h: 11 },
            OcrLineBox { text: "VAAL VITALITY".into(), x: 742, y: 828, w: 89, h: 11 },
            OcrLineBox { text: "Should Recruit".into(), x: 1060, y: 204, w: 123, h: 17 },
            OcrLineBox { text: "0:56".into(), x: 1663, y: 74, w: 25, h: 12 },
            OcrLineBox { text: "Allflame League".into(), x: 1782, y: 98, w: 123, h: 17 },
            OcrLineBox { text: "Short Allocation".into(), x: 1781, y: 121, w: 124, h: 13 },
            OcrLineBox { text: "Frankfurt (ELI) Realm".into(), x: 1742, y: 145, w: 162, h: 17 },
            OcrLineBox { text: "„More than 50 monsters remain".into(), x: 1659, y: 169, w: 246, h: 16 },
            OcrLineBox { text: "AREA HAS INCREASED MONSTER VARIEft'".into(), x: 1658, y: 237, w: 256, h: 12 },
            OcrLineBox { text: "AREA CONTAINS MANY TOTEMS".into(), x: 1712, y: 255, w: 202, h: 12 },
            OcrLineBox { text: "38% MORE MONSTER LIFE".into(), x: 1748, y: 273, w: 167, h: 13 },
            OcrLineBox { text: "MONSTERS CANNOT BE STUNNED".into(), x: 1701, y: 291, w: 214, h: 12 },
            OcrLineBox { text: "MERCENARIES FOUND IN AREA ARE ACCOMPANIED BY TWO WILD MERCENARIES".into(), x: 1404, y: 309, w: 511, h: 12 },
            OcrLineBox { text: "MERCENARIES FOUND IN AREA ARE INFAMOUS".into(), x: 1618, y: 328, w: 297, h: 12 },
            OcrLineBox { text: "MONSTERS HAVE 64% INCREASED ACCURACY RATING".into(), x: 1574, y: 346, w: 340, h: 13 },
            OcrLineBox { text: "MONSTERS HAVE +76% CHANCE TO SUPPRESS SPELL DAMAGE".into(), x: 1522, y: 364, w: 393, h: 13 },
            OcrLineBox { text: "MONSTERS POWER, FRENZY AND ENDURANCE CHARGES ON HIT".into(), x: 1470, y: 382, w: 446, h: 13 },
            OcrLineBox { text: "PLAYERS HAVE 76% LESS RECOVERY RATE OF LIFE AND ENERGY SHIELD".into(), x: 1468, y: 401, w: 446, h: 13 },
            OcrLineBox { text: "window — lEt".into(), x: 1499, y: 430, w: 92, h: 8 },
            OcrLineBox { text: "PLAY".into(), x: 1422, y: 437, w: 30, h: 11 },
            OcrLineBox { text: "Zethara, the Bardiyan Rose • Infamous Kineticist • Ivl 83".into(), x: 1468, y: 446, w: 339, h: 13 },
            OcrLineBox { text: "unknown — 8 icons unread".into(), x: 1468, y: 465, w: 154, h: 9 },
            OcrLineBox { text: "8 — to".into(), x: 1468, y: 484, w: 117, h: 8 },
            OcrLineBox { text: "AREA IS CORRUPTED".into(), x: 1787, y: 510, w: 127, h: 12 },
            OcrLineBox { text: "DELIRIUM.REWARD TYPES IN YOUR MAPS GAIN +1 TO COUNT ON DEFEATING A UNIQUE DELIRIUM Boss".into(), x: 1250, y: 528, w: 665, h: 14 },
            OcrLineBox { text: "DELIRIUM ENCOUNTERS CONTAIN ALL UNIQUE DELIRIUM BOSSES".into(), x: 1491, y: 546, w: 423, h: 14 },
            OcrLineBox { text: "DELIRIUM ENCOUNTERS GENERATE 6 ADDITIONAL REWARD TYPES".into(), x: 1499, y: 565, w: 416, h: 12 },
            OcrLineBox { text: "ALVA, MASTER EXPLORER".into(), x: 1531, y: 607, w: 187, h: 17 },
            OcrLineBox { text: "Complete the temporal Incursiohs (2/3)".into(), x: 1557, y: 628, w: 287, h: 17 },
            OcrLineBox { text: "KiNCSMARCH PROSPECTING'(OPTIONAL)".into(), x: 1533, y: 649, w: 306, h: 18 },
            OcrLineBox { text: "Speak to Johan for a reward".into(), x: 1557, y: 670, w: 205, h: 17 },
            OcrLineBox { text: "THREADS OF THE ORIGINATOR".into(), x: 1531, y: 691, w: 228, h: 13 },
            OcrLineBox { text: "Explore Memory Vaults in different Atlas".into(), x: 1557, y: 710, w: 300, h: 18 },
            OcrLineBox { text: "drants (2/4)".into(), x: 1585, y: 730, w: 82, h: 17 },
            OcrLineBox { text: "Mana 4921801".into(), x: 1733, y: 811, w: 142, h: 19 },
            OcrLineBox { text: "Reserved ăŻ".into(), x: 1732, y: 828, w: 122, h: 27 },
            OcrLineBox { text: "09".into(), x: 1855, y: 835, w: 19, h: 14 },
            OcrLineBox { text: "2251".into(), x: 329, y: 944, w: 36, h: 14 },
            OcrLineBox { text: "MENU".into(), x: 241, y: 1041, w: 48, h: 14 },        ]
    }

    /// The dump's lines minus the one whose text is `text`, with an arrange
    /// guard that it was actually there — the tests below turn on which chrome
    /// line is absent, and a typo would silently make them assert nothing.
    fn without(lines: Vec<OcrLineBox>, text: &str) -> Vec<OcrLineBox> {
        let before = lines.len();
        let kept: Vec<OcrLineBox> = lines.into_iter().filter(|l| l.text != text).collect();
        assert!(
            kept.len() < before,
            "arrange: {text:?} must have been in the dump to be removed from it",
        );
        kept
    }

    /// The lines of `lines` that read as one of the three text anchors, in the
    /// order OCR returned them.
    fn anchor_lines<'a>(lines: &'a [OcrLineBox], g: &MercGeometry) -> Vec<&'a OcrLineBox> {
        lines.iter().filter(|l| text_anchor(&l.text, g).is_some()).collect()
    }

    /// The incident frame, whole, now detects — and the geometry it reports is
    /// the one its own line centres imply, not a constant echoed back. The six
    /// skill lines sit at x 742-743 with centres 616.5, 659.5, 704.5, 746.5,
    /// 790.5 and 833.5, whose five gaps (43, 45, 42, 44, 43) have median 43.
    #[test]
    fn the_koparu_dump_detects_six_rows_at_the_pitch_its_centres_imply() {
        let g = MercGeometry::default();

        let layout = detect(&koparu_dump_lines(), &g, &vocab(), None)
            .expect("the POE-217 frame must detect");

        assert_eq!(layout.rows.len(), 6);
        assert_eq!(layout.row_pitch, 43.0);
        assert!(
            (layout.scale - 43.0 / g.row_pitch).abs() < 1e-6,
            "scale must be 43 / 49.3 = 0.8722, was {}",
            layout.scale,
        );
        assert_eq!(layout.column_x0, 743);
    }

    /// The recruit verdict anchors on its own. With the mangled wager taken
    /// out, `Should Recruit` is the only chrome line the frame has left — the
    /// footer buttons never reached this OCR at all — so the detect that
    /// follows can only have come through the new rule.
    #[test]
    fn the_recruit_verdict_alone_anchors_a_frame_with_no_wager_and_no_buttons() {
        let g = MercGeometry::default();
        let lines = without(koparu_dump_lines(), "Waggr: 6 231");
        let anchors = anchor_lines(&lines, &g);
        assert_eq!(anchors.len(), 1, "arrange: one chrome line left, got {anchors:?}");
        assert_eq!(anchors[0].text, "Should Recruit");

        let layout = detect(&lines, &g, &vocab(), None)
            .expect("the recruit verdict must anchor the frame on its own");

        assert_eq!(layout.rows.len(), 6);
    }

    /// And the loosened wager bar anchors on its own. With `Should Recruit`
    /// taken out, `Waggr: 6 231` is the only chrome left; at the 0.98 bar it
    /// scored 0.907 and this is exactly the frame that was lost.
    #[test]
    fn the_mangled_wager_alone_anchors_a_frame_with_no_verdict_and_no_buttons() {
        let g = MercGeometry::default();
        let lines = without(koparu_dump_lines(), "Should Recruit");
        let anchors = anchor_lines(&lines, &g);
        assert_eq!(anchors.len(), 1, "arrange: one chrome line left, got {anchors:?}");
        assert_eq!(anchors[0].text, "Waggr: 6 231");

        let layout = detect(&lines, &g, &vocab(), None)
            .expect("the mangled wager must anchor the frame on its own");

        assert_eq!(layout.rows.len(), 6);
    }

    /// With both gone the frame carries no chrome at all, and it is still
    /// refused: POE-217 widened WHAT anchors, it did not drop the requirement.
    /// Six rows of real skill names and 49 lines of map mods and quest text
    /// are not a recruit window.
    #[test]
    fn the_koparu_dump_stripped_of_both_chrome_lines_misses_at_the_anchor_step() {
        let g = MercGeometry::default();
        let lines = without(without(koparu_dump_lines(), "Waggr: 6 231"), "Should Recruit");
        assert!(anchor_lines(&lines, &g).is_empty(), "arrange: no chrome left");

        let miss = detect_reason(&lines, &g, &vocab(), None)
            .expect_err("a frame with no chrome is not a recruit window");

        assert_eq!(
            miss.stage,
            DetectStage::NoAnchor { rows: 6, panel: PanelAnchor::NoKnownRect },
        );
    }

    /// The verdict anchors ABOVE the rows only. Below the last row the one
    /// piece of chrome the panel draws is a footer button, and accepting a
    /// verdict there would let whatever the game drew under the panel stand in
    /// for the panel's own.
    #[test]
    fn a_recruit_verdict_below_the_rows_is_not_an_anchor() {
        let g = MercGeometry::default();
        let mut lines = without(koparu_dump_lines(), "Waggr: 6 231");
        for l in lines.iter_mut() {
            if l.text == "Should Recruit" {
                // The last row's centre is 833.5; one pitch under it.
                l.y = 877;
            }
        }

        assert!(detect(&lines, &g, &vocab(), None).is_none());
    }

    /// …and NEAR them. Row 1's centre is 616.5 and the pitch 43, so the reach
    /// ends 516 px up at 100.5; a verdict-shaped line past that belongs to some
    /// other surface, exactly as a wager line does.
    #[test]
    fn a_recruit_verdict_out_of_reach_above_the_panel_is_not_an_anchor() {
        let g = MercGeometry::default();
        let mut lines = without(koparu_dump_lines(), "Waggr: 6 231");
        for l in lines.iter_mut() {
            if l.text == "Should Recruit" {
                // Thirteen pitches above row 1 rather than twelve.
                l.y = 616 - (13.0 * 43.0) as i32;
            }
        }

        assert!(detect(&lines, &g, &vocab(), None).is_none());
    }

    /// The verdict predicate reads both wordings the panel uses, in any case,
    /// and survives one glyph going wrong in either anchor word — `shou1d`
    /// scores 0.933 against "should" and `recrult` 0.952 against "recruit",
    /// both over the 0.90 bar. The refusal wording puts a word BETWEEN the
    /// two, which is why only the first and the last are scored.
    #[test]
    fn the_recruit_verdict_reads_both_wordings_and_a_one_glyph_error_in_either() {
        let g = MercGeometry::default();

        for text in ["Should Recruit", "SHOULD RECRUIT", "Should Not Recruit", "Shou1d Recrult"] {
            assert!(is_recruit_verdict_line(text, &g), "{text:?} must read as the verdict");
        }
    }

    /// Neither anchor word carries the line on its own, and the quest tracker
    /// that sits beside the panel on this very screen does not either:
    /// `complete` scores 0.528 against "should", `incursions` 0.574 against
    /// "recruit".
    #[test]
    fn a_single_word_or_unrelated_text_is_not_the_recruit_verdict() {
        let g = MercGeometry::default();

        for text in ["Recruit", "should", "Complete the temporal Incursions", "Should Not", ""] {
            assert!(!is_recruit_verdict_line(text, &g), "{text:?} must not read as the verdict");
        }
    }

    /// Where the wager bar sits since POE-217, by the two spellings that
    /// bracket it: the incident's own `Waggr` scores 0.907 on the head word and
    /// is admitted, a `vv` read for the `w` scores 0.822 and is not.
    #[test]
    fn the_wager_bar_admits_the_incidents_spelling_and_refuses_a_two_glyph_one() {
        let g = MercGeometry::default();

        assert!(is_wager_line("Waggr: 6 231", &g), "the POE-217 spelling must anchor");
        assert!(!is_wager_line("vvager: 6 231", &g), "a two-glyph error must not");
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
    /// over the 0.90 bar this uses since POE-217. Anchoring on it would hand
    /// the module a capture of whatever window happened to be open.
    ///
    /// The line is left exactly where the real label sits, above row 1 and
    /// inside the reach, so position cannot be what refuses it: the amount is.
    /// Chat prose carries no digits and the panel's label always does.
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

    /// The amount is half the predicate. Both spellings the panel has actually
    /// been read as — the clean label and the mangled one the POE-217 capture
    /// was lost on — carry theirs, and both anchor.
    #[test]
    fn a_wager_label_followed_by_its_amount_is_the_anchor() {
        let g = MercGeometry::default();

        for text in ["Wager: 1 028", "Waggr: 6 231"] {
            assert!(is_wager_line(text, &g), "{text:?} must read as the label");
        }
    }

    /// …and the words the 0.90 bar admits are refused for want of one. All
    /// three clear the head score — `Wagner` 0.961, `wagers` 0.972, `Wage`
    /// 0.960 — and all three are ordinary words that PoE draws in chat and in
    /// item text. Listed one by one so the failure message names the word that
    /// got through.
    #[test]
    fn a_near_miss_word_with_no_amount_after_it_is_not_the_wager_label() {
        let g = MercGeometry::default();

        for text in ["Wagner", "wagers", "Wage"] {
            assert!(!is_wager_line(text, &g), "{text:?} must not read as the label");
        }
    }

    /// Even the word itself, spelled perfectly, is not the label without an
    /// amount behind it. The panel has never been seen to draw one without the
    /// other, so a bare `Wager` is some other surface's text — and this is the
    /// case the head score cannot refuse, since it scores 1.000.
    #[test]
    fn a_bare_wager_word_with_no_amount_is_not_the_label() {
        assert!(!is_wager_line("Wager", &MercGeometry::default()));
    }

    /// The bar still has a floor: two glyphs wrong, or a word that only shares
    /// the shape, and the line is not the label wherever it sits. `Waqer`
    /// scores 0.893, `vvager` 0.822, `Water` 0.893 and `Manager` 0.791 —
    /// all under 0.90. Listed one by one so the failure message names the word
    /// that got through.
    #[test]
    fn other_near_miss_words_are_not_anchors() {
        for word in ["Waqer", "vvager", "Water", "Manager"] {
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
        let lines = vec![test_line("Wager: 1 028", 80, 173), test_line("Conductivity", 134, 620)];

        assert!(detect(&lines, &MercGeometry::default(), &vocab(), None).is_none());
    }

    /// No skill names at all: the detector must not fall back to "any column".
    #[test]
    fn a_screen_with_no_skill_names_detects_nothing() {
        let lines = vec![
            test_line("Wager: 1 028", 80, 173),
            test_line("Inventory", 134, 620),
            test_line("Stash", 134, 669),
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

    /// [`outer_rect_for_inner`] is [`inner_rect`] run backwards, at every UI
    /// scale and at an inset the player overrode.
    ///
    /// The rule is `outer = inner + 2 · inset`, and the corpus readers rebuild
    /// a cell from a stored inner crop through it (POE-214 D5). The four
    /// inputs are the geometries that exist: the laptop's fitted cell (44
    /// outer, 40 inner), the PC's at UI scale 0.90 (40/36), an integral
    /// `cellInset` override — which has to move the outer rect and leave the
    /// inner crop alone, so a helper that ignored `g` would pass the first two
    /// — and a FRACTIONAL one, which is the only input that separates the
    /// contract's `.round()` from a truncating `as i32`: at 2.5 the inset is
    /// 3, not 2, and both directions have to agree on that or a reader would
    /// paste a crop into a canvas one px off on every side.
    #[test]
    fn the_outer_rect_of_an_inner_crop_round_trips_back_through_inner_rect() {
        let g = MercGeometry::default();

        assert_eq!(outer_rect_for_inner(40, &g), (44, 2), "the laptop's fitted cell");
        assert_eq!(inner_rect([0, 0, 44, 44], &g), [2, 2, 40, 40]);

        assert_eq!(outer_rect_for_inner(36, &g), (40, 2), "the PC's cell at 0.90");
        assert_eq!(inner_rect([0, 0, 40, 40], &g), [2, 2, 36, 36]);

        let mut wide = MercGeometry::default();
        wide.cell_inset = 5.0;
        assert_eq!(outer_rect_for_inner(36, &wide), (46, 5));
        assert_eq!(inner_rect([0, 0, 46, 46], &wide), [5, 5, 36, 36]);

        let mut half = MercGeometry::default();
        half.cell_inset = 2.5;
        assert_eq!(
            outer_rect_for_inner(39, &half),
            (45, 3),
            "a 2.5 px inset ROUNDS to 3, so the cell is 39 + 2 · 3",
        );
        assert_eq!(inner_rect([0, 0, 45, 45], &half), [3, 3, 39, 39]);
    }

    // -- the panel rect the occlusion rule tests against -------------------

    /// A layout the way `detect` builds one: cells laid out from the column x
    /// at the reference offsets, all `max_slots` of them.
    fn layout_of(centres: &[f32], row_pitch: f32, column_x0: i32, g: &MercGeometry) -> MercLayout {
        let cell_size = g.cell_size as i32;
        MercLayout {
            scale: 1.0,
            scale_source: super::super::ScaleSource::Ocr,
            column_x0,
            row_pitch,
            rows: centres
                .iter()
                .enumerate()
                .map(|(i, &centre)| MercLayoutRow {
                    index: i as u8,
                    centre_y: centre,
                    skill_icon: skill_icon_rect(column_x0 as f32, centre, cell_size),
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

    // -- what a placed-crop probe accepts on -------------------------------

    /// The dump as OCR returned it, wager line and all: the probe accepts.
    #[test]
    fn a_crop_holding_a_footer_button_is_a_hit() {
        assert!(probe_hit(&windows_dump_lines(), &MercGeometry::default()));
    }

    /// The 2026-08-24 measurement that shaped the whole anchor rule: Windows
    /// OCR returned NO line for the wager, and both buttons read clean. Either
    /// one alone has to be enough.
    #[test]
    fn each_text_anchor_alone_is_a_hit() {
        let g = MercGeometry::default();

        for text in ["Wager: 8 831", "Should Recruit", "TAKE ITEM", "REMATCH"] {
            let lines = vec![OcrLineBox { text: text.into(), x: 830, y: 979, w: 87, h: 13 }];
            assert!(probe_hit(&lines, &g), "{text} must accept");
        }
    }

    /// Skill text alone is not recruit chrome.
    #[test]
    fn a_crop_holding_only_skill_names_is_not_a_hit() {
        let g = MercGeometry::default();
        let lines = vec![OcrLineBox { text: "FROST BOMB".into(), x: 719, y: 678, w: 87, h: 13 }];

        assert!(text_anchor(&lines[0].text, &g).is_none(), "arrange: it is not chrome");
        assert!(!probe_hit(&lines, &g));
    }

    /// The rejection that matters: skill names are what a gem tooltip and the
    /// character panel are full of, and the probe runs while the player is
    /// walking through an arena. Accepting on those would hand a full detect to
    /// every voice line, which is the burst back.
    #[test]
    fn a_crop_holding_only_non_chrome_lines_is_not_a_hit() {
        let g = MercGeometry::default();
        let lines: Vec<OcrLineBox> = windows_dump_lines()
            .into_iter()
            .filter(|l| text_anchor(&l.text, &g).is_none())
            .collect();

        assert!(!lines.is_empty(), "arrange: the dump still has its skill rows");
        assert!(!probe_hit(&lines, &g));
    }

    #[test]
    fn an_empty_crop_is_not_a_hit() {
        assert!(!probe_hit(&[], &MercGeometry::default()));
    }

}
