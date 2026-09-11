//! Turning a detected layout into a capture (POE-165 D2 pass 2, D4).
//!
//! [`build_planned`] is the ONE place a `MercLayout` plus a screen image
//! becomes a [`MercCapture`]: the debug command goes through its full-read form
//! [`build_capture`], and the capture loop through the same function with the
//! round's [`ReadPlan`] (POE-278), so a dump can never disagree with what the
//! page was shown. [`carry_capture`] is the round that reads nothing.
//!
//! It is pure — image in, capture out, no OCR call, no clock, no lock — which
//! is what makes the cell walk (occupancy → signature → badge → vocabulary)
//! testable on this Linux host. The one thing it cannot do is the pass-2 re-OCR
//! of a row's name band, because that IS an OCR call; the caller does that and
//! hands the text in ([`pass2_texts`]).

use std::collections::{HashMap, HashSet};

use image::{DynamicImage, GenericImageView, RgbaImage};
use serde::Serialize;

use super::geometry::{
    column_tolerance, inner_rect, name_crop_left, occupied, stddev, Frame, MercLayout,
    MercLayoutRow, NAME_CROP_PAD,
};
use super::icons::{cell_candidates, read_tier, CellSig, TemplateStore};
use super::vocab::{classify_resolution, MercVocab};
use super::{
    MercCapture, MercGeometry, MercRow, MercSkillRead, MercSupportRead, ReadState,
};

/// Everything [`build_capture`] learned about one support slot, including the
/// slots it rejected. The capture keeps only the reads; the debug dump keeps
/// these, which is where "why is slot 3 missing?" gets answered.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellDebug {
    pub row: u8,
    pub slot: u8,
    pub rect: [i32; 4],
    /// Grayscale stddev of the inner region — `None` when the rect is
    /// off-image, which is itself the answer for a half-off-screen window.
    pub stddev: Option<f32>,
    pub occupied: bool,
    pub tier: Option<u8>,
    pub family: Option<String>,
    pub icon_score: f32,
    pub icon_runner_up: f32,
    pub state: ReadState,
}

/// A capture plus what the debug dump needs and the hover-confirm remembers.
pub struct ReadResult {
    pub capture: MercCapture,
    pub cells: Vec<CellDebug>,
    /// Geometry rows whose left skill icon is occupied. Occupancy is the
    /// independent icon-count sensor: it comes from the same stddev gate that
    /// decides which support cells enter the capture.
    pub rows_on_screen: usize,
    /// Geometry rows whose skill OCR resolved to anything other than Unknown.
    pub rows_read: usize,
    /// The PRE-HOVER crop of every occupied cell, keyed `(row key, slot)`.
    ///
    /// D5's rule in data form: the template a hover-confirm learns comes from
    /// the crop taken at DETECT time, never from a fresh grab — by the time the
    /// tooltip is up, the cell underneath may be drawn highlighted, and the
    /// store would learn the highlight.
    ///
    /// Keyed by [`row_key`], not by `row.index` (POE-207): the index is
    /// sequential over the rows THIS read found, so a skill line the OCR
    /// dropped renumbers every row below it and the crop cached under `(2, 1)`
    /// becomes another cell's art. A hover-confirm keys on `(row_key, slot)`,
    /// so the cache has to as well or the two disagree about which cell they
    /// are talking about — the second, code-verified mislabel path behind the
    /// 21 poisoned templates of 2026-08-26.
    pub sigs: HashMap<(String, u8), (CellSig, Option<RgbaImage>)>,
    /// The `(row key, slot)` of every cell this read COPIED from the kept
    /// capture instead of matching it (POE-278). A copied cell cut no crop, so
    /// it has nothing in [`Self::sigs`]; its entry in the loop's crop cache is
    /// the one an earlier read cut, and `run::merge_sigs` carries it across
    /// this read rather than dropping it. Filtered by the same trailing-row trim
    /// and the same collision rule as [`Self::sigs`].
    pub carried: HashSet<(String, u8)>,
}

/// The key a confirmation — and the pre-hover crop it learns from — is
/// remembered under: the row's skill, plus the slot.
///
/// D5: confirmations survive re-detection of the SAME window. The row index is
/// not stable enough for that on its own — a wrapped name or a missed line
/// renumbers the rows — so the row's identity is its skill id, falling back to
/// its raw text when the skill did not resolve.
///
/// The key is NOT unique by construction, and the two ways it can collide pull
/// in opposite directions:
///
/// - CHURN — an unresolved row whose raw OCR text differs between detects
///   produces a new key, so the hovered cell finds no cached crop and the
///   confirm reports `NoCrop`. That fails safe on its own;
/// - COLLISION — two rows of one panel whose skills resolve to the SAME id (a
///   fuzzy match that lands on one skill twice, or two unresolved rows the OCR
///   read the same way) claim one key, and the later row's crop would overwrite
///   the earlier one's. That does NOT fail safe: a confirm on the first row
///   would learn the SECOND row's art under the tooltip's family, and the
///   two-read hover guard cannot catch it because the guard corroborates the
///   TOOLTIP, not the crop. [`build_capture`] therefore drops the crops of every
///   colliding row outright — see the retain at the end of it.
///
/// Learning nothing is the honest outcome in both; learning another cell's art
/// is the bug.
pub fn row_key(skill: &MercSkillRead) -> String {
    match skill.ids.first() {
        Some(id) => id.clone(),
        None => skill.raw.trim().to_lowercase(),
    }
}

/// Read a detected layout into a capture — the FULL read: pass-2 text for every
/// row, the icon walk for every row. [`build_planned`] with no plan.
///
/// `row_texts` is per row, in layout order: the pass-2 text where the re-OCR
/// produced one, the pass-1 text otherwise (see [`pass2_texts`]). Passing the
/// pass-1 text for every row is legal and is what the Linux tests do.
///
/// Slot scanning stops at the first UNOCCUPIED slot (D2 step 4): the cells to
/// the right of an empty one are empty by construction, and scanning past it
/// would sign whatever UI sits beyond the panel's right edge.
pub fn build_capture(
    img: &DynamicImage,
    frame: Frame,
    layout: &MercLayout,
    row_texts: &[String],
    captured_at_ms: u64,
    g: &MercGeometry,
    vocab: &MercVocab,
    store: &TemplateStore,
) -> ReadResult {
    build_planned(img, frame, layout, row_texts, captured_at_ms, g, vocab, store, None)
}

/// Read a detected layout into a capture, re-reading only what `planned` asks
/// for (POE-278, ADR-025 clause 2).
///
/// `planned` is the round's per-layout-row [`RowPlan`] (see [`plan_read`])
/// and the KEPT capture it was decided from; `None` is the full read
/// [`build_capture`] names. Per layout row, matched to the kept row by `index`:
///
/// - [`RowPlan::Read`] — the row's `row_texts` entry through the vocabulary
///   and the whole icon walk, exactly as a full read. A kept row, or a missing
///   one, is never consulted: its skill can resolve to another `row_key`, and
///   the crop cache and the confirmations key on it.
/// - [`RowPlan::Cells`] and [`RowPlan::Unseen`] — the kept skill verbatim (its
///   `row_texts` entry is ignored) and the occupancy walk, which is pixels
///   only. A slot whose kept cell is confident is COPIED — no
///   `cell_candidates`, no badge read, no `match_family` — and every other
///   occupied slot is matched: an unknown kept cell, or a slot past the row's
///   last kept cell, which no read has seen yet. The two differ only in what
///   the plan expects to find ([`plan_read`]).
///
/// An empty slot stops the WALK, not the row. A panel's cells are a prefix, so
/// a CONFIDENT kept cell at or past that slot proves the dark read is this
/// frame's — a tooltip's shadow, a redraw — and the kept cells up to and
/// including the last confident one ride verbatim: one frame must not remove
/// cells an earlier round read, confirmed ones included, or the next round
/// would plan over the truncated row. Kept cells past the last confident one
/// were verified by nothing, and this frame says the slot is empty, so they
/// are dropped as a full read would drop them — an occlusion phantom must not
/// outlive its tooltip.
///
/// A copied cell takes its `rect` from THIS layout where it has that slot (the
/// kept rect otherwise): the geometry is the layout's, the identity and read
/// state are the kept read's. It lands in [`ReadResult::carried`], not in
/// [`ReadResult::sigs`]. A row the plan wants walked but the kept capture does
/// not have is read.
///
/// The skill-icon sensor and the collision rule run over every layout row
/// whatever the plan says: they are pixel reads and key arithmetic, not OCR.
/// The trailing-row trim does too, but never removes a row taken from the kept
/// read — that row was on screen when it was read, and a dark icon on this
/// frame is not evidence it left.
#[allow(clippy::too_many_arguments)]
pub fn build_planned(
    img: &DynamicImage,
    frame: Frame,
    layout: &MercLayout,
    row_texts: &[String],
    captured_at_ms: u64,
    g: &MercGeometry,
    vocab: &MercVocab,
    store: &TemplateStore,
    planned: Option<(&[RowPlan], &MercCapture)>,
) -> ReadResult {
    let mut cells_debug = Vec::new();
    let mut sigs = HashMap::new();
    let mut carried = HashSet::new();
    // Every row's key, in layout order — the collision count at the end needs
    // the keys of rows that cached nothing too, because a row with no occupied
    // cell still claims its key.
    let mut row_keys: Vec<String> = Vec::with_capacity(layout.rows.len());
    let mut rows = Vec::with_capacity(layout.rows.len());
    let (row_icons, rows_on_screen) = icon_sensor(img, frame, layout, g);
    // One past the last row taken from the kept read — the trim's floor.
    let mut kept_len = 0;

    for (i, row) in layout.rows.iter().enumerate() {
        // What this round does with the row, and the kept row it copies from.
        // `None` is a read: no plan, a `Read` row, or no kept row to copy.
        let kept = planned.and_then(|(plan, kept)| {
            let action = plan.get(i).copied().unwrap_or(RowPlan::Read);
            if action == RowPlan::Read {
                return None;
            }
            kept.rows.iter().find(|k| k.index == row.index).map(|k| (action, k))
        });
        let skill = match kept {
            Some((_, known)) => known.skill.clone(),
            None => {
                let raw = row_texts.get(i).cloned().unwrap_or_else(|| row.text.clone());
                let name_read = vocab.match_skill(&raw, &g.thresholds);
                MercSkillRead {
                    raw,
                    ids: name_read.ids,
                    name: name_read.name,
                    score: name_read.score,
                    state: name_read.state,
                }
            }
        };
        // The identity the crop cache and the hover-confirm SHARE. Computed
        // once per row, from the skill the row just resolved to.
        let key = row_key(&skill);
        row_keys.push(key.clone());

        let kept_row = kept.map(|(_, known)| known);
        if kept_row.is_some() {
            kept_len = i + 1;
        }

        let mut supports = Vec::new();
        for (slot, rect) in row.cells.iter().enumerate() {
            // The layout is screen-absolute; the IMAGE may be a crop of the
            // screen. `local` is the only place the two spaces meet — the rect
            // that goes into the capture, the debug dump and the signature
            // cache stays absolute, because that is what the hover tick
            // hit-tests the real cursor against.
            let px = frame.local(*rect);
            let sd = stddev(img, inner_rect(px, g));
            let is_occupied = occupied(img, px, g);
            if !is_occupied {
                cells_debug.push(CellDebug {
                    row: row.index,
                    slot: slot as u8,
                    rect: *rect,
                    stddev: sd,
                    occupied: false,
                    tier: None,
                    family: None,
                    icon_score: 0.0,
                    icon_runner_up: 0.0,
                    state: ReadState::Unknown,
                });
                // The walk stops here. A panel's cells are a prefix, so a
                // CONFIDENT kept cell at or past this slot proves the empty read
                // is this frame's (a tooltip's shadow, a redraw): the kept cells
                // up to it ride verbatim. Past the last confident one nothing
                // verified them and this frame says the slot is empty — they
                // go, as a full read would drop them (an occlusion phantom must
                // not outlive its tooltip).
                if let Some(known) = kept_row {
                    let last_confident = known
                        .supports
                        .iter()
                        .filter(|cell| cell.slot as usize >= slot && confident(cell.state))
                        .map(|cell| cell.slot)
                        .max();
                    let rest = known.supports.iter().filter(|cell| {
                        cell.slot as usize >= slot && last_confident.is_some_and(|l| cell.slot <= l)
                    });
                    for cell in rest {
                        supports.push(MercSupportRead {
                            rect: row.cells.get(cell.slot as usize).copied().unwrap_or(cell.rect),
                            ..cell.clone()
                        });
                        carried.insert((key.clone(), cell.slot));
                    }
                }
                break;
            }

            // A cell the kept read is confident about is COPIED, never matched
            // again: a confident read is not replaced by a later round
            // (ADR-025 clause 2). Occupied is all this round asks of it.
            if let Some(known) = kept_row.and_then(|k| confident_cell(k, slot as u8)) {
                supports.push(MercSupportRead {
                    rect: *rect,
                    ..known.clone()
                });
                cells_debug.push(CellDebug {
                    row: row.index,
                    slot: slot as u8,
                    rect: *rect,
                    stddev: sd,
                    occupied: true,
                    tier: known.tier,
                    family: known.family.clone(),
                    icon_score: known.score,
                    icon_runner_up: 0.0,
                    state: known.state,
                });
                carried.insert((key.clone(), slot as u8));
                continue;
            }

            // Built ONCE per occupied cell and matched against every template
            // (POE-207): the 49 aligned signatures are the expensive half, and
            // rebuilding them per template is what would put the aligned
            // search out of reach at the pool's ceiling.
            let aligned = cell_candidates(img, px, g);
            let tier = read_tier(img, px, g);
            let icon = match &aligned {
                Some(c) => store.match_family(c, &g.thresholds),
                // Unreachable while `occupied` and `cell_candidates` share the
                // same gate, but a rect that passes one and not the other must
                // still produce a read rather than a panic.
                None => super::icons::IconMatch::unknown(),
            };

            let (family, ids, name, state, candidates) = resolve_cell(&icon, tier, vocab);
            supports.push(MercSupportRead {
                slot: slot as u8,
                rect: *rect,
                family: family.clone(),
                tier,
                ids,
                name,
                score: icon.score,
                state,
                candidates,
            });
            cells_debug.push(CellDebug {
                row: row.index,
                slot: slot as u8,
                rect: *rect,
                stddev: sd,
                occupied: true,
                tier,
                family,
                icon_score: icon.score,
                icon_runner_up: icon.runner_up,
                state,
            });

            // The UNSHIFTED signature is what a hover-confirm learns: an
            // aligned one carries this capture's rect jitter, and the next
            // capture's rect does not reproduce it.
            if let Some(c) = aligned {
                sigs.insert((key.clone(), slot as u8), (c.into_centre(), crop_rgba(img, px, g)));
            }
        }

        rows.push(MercRow {
            index: row.index,
            skill,
            supports,
        });
    }

    // A placed panel's fixed geometry can extend beyond a mercenary who has
    // fewer rows than the seed allows. Keep every row when no icon is visible
    // (that preserves an OCR-only read and its honest unknown rows), but once
    // the icon sensor sees a row, do not publish trailing geometry whose icon
    // cell is dark. `row_icons` is the same gate as support-cell occupancy
    // ([`icon_sensor`]), so this trim cannot turn an occupied support into
    // "nothing". A row a partial round took from the kept read stays: it was
    // on screen when it was read (`kept_len`).
    if let Some(last_occupied) = row_icons.iter().rposition(|&occupied| occupied) {
        let retained_len = (last_occupied + 1).max(kept_len);
        if retained_len < rows.len() {
            let removed_keys = row_keys[retained_len..]
                .iter()
                .cloned()
                .collect::<HashSet<_>>();
            sigs.retain(|(key, _): &(String, u8), _| !removed_keys.contains(key));
            carried.retain(|(key, _)| !removed_keys.contains(key));
            rows.truncate(retained_len);
            row_keys.truncate(retained_len);
            let last_index = rows.last().map_or(0, |row| row.index);
            cells_debug.retain(|cell| cell.row <= last_index);
        }
    }

    let rows_read = rows
        .iter()
        .filter(|row| row.skill.state != ReadState::Unknown)
        .count();

    // COLLIDING ROWS CACHE NOTHING. `row_key` is a resolved skill id or a
    // lowercased raw OCR line, and neither is unique across the rows of one
    // panel: a fuzzy match can land on the same skill twice, and two unread
    // rows can OCR to the same text. Those rows share one cache key, so the
    // later one's crop silently overwrites the earlier one's and a confirm on
    // the earlier row would learn the LATER row's art under the tooltip's
    // family. The two-read hover guard cannot see it — it corroborates the
    // tooltip, and both reads of the wrong crop agree.
    //
    // Dropping the crops of every colliding row leaves those cells reporting
    // `NoCrop`: the confirmation still names them, nothing is learned. The next
    // read that reads the skill lines apart caches them again for a cell it
    // MATCHES; a cell it copies (POE-278) cut no crop and stays `NoCrop` for
    // the rest of the capture.
    //
    // A carried crop is a crop too: a copied row that collides carries none.
    let counts = key_counts(&row_keys);
    sigs.retain(|(key, _): &(String, u8), _| counts.get(key.as_str()) == Some(&1));
    carried.retain(|(key, _)| counts.get(key.as_str()) == Some(&1));

    ReadResult {
        capture: MercCapture {
            captured_at_ms,
            live: true,
            scale: layout.scale,
            // The SCREEN, not the image: a cropped detect frame is smaller
            // than the desktop, and `run::hover_region` clamps the tooltip
            // crop to this — clamping it to the panel would cut every tooltip
            // that opens outside the grid.
            screen: frame.screen(),
            panel: None,
            header: layout.header.clone(),
            rows,
            rows_on_screen,
            rows_read,
            partial: false,
        },
        cells: cells_debug,
        rows_on_screen,
        rows_read,
        sigs,
        carried,
    }
}

/// The round that reads nothing (POE-278, [`ReadPlan::Nothing`]): the kept
/// capture carried onto this frame.
///
/// Every kept row verbatim, in the kept order — a layout row the kept capture
/// lacks is not added, because nothing here reads it — with each cell's `rect`
/// taken from this layout's row of the same `index` where it has that slot.
/// The skill-icon sensor still runs over every layout row: it is the pixel
/// counter `rows_on_screen`, not a read. `rows_read` counts the carried rows.
/// The header is this layout's pass-1 header, for the caller to fold; every
/// carried cell is in [`ReadResult::carried`] unless its row key collides.
pub fn carry_capture(
    img: &DynamicImage,
    frame: Frame,
    layout: &MercLayout,
    kept: &MercCapture,
    captured_at_ms: u64,
    g: &MercGeometry,
) -> ReadResult {
    let (_, rows_on_screen) = icon_sensor(img, frame, layout, g);
    let rows: Vec<MercRow> = kept
        .rows
        .iter()
        .map(|known| MercRow {
            index: known.index,
            skill: known.skill.clone(),
            supports: match layout.rows.iter().find(|row| row.index == known.index) {
                Some(row) => copied_cells(known, row),
                None => known.supports.clone(),
            },
        })
        .collect();
    let rows_read = rows
        .iter()
        .filter(|row| row.skill.state != ReadState::Unknown)
        .count();
    let row_keys: Vec<String> = rows.iter().map(|row| row_key(&row.skill)).collect();
    let counts = key_counts(&row_keys);
    let carried = rows
        .iter()
        .zip(&row_keys)
        .filter(|(_, key)| counts.get(key.as_str()) == Some(&1))
        .flat_map(|(row, key)| row.supports.iter().map(move |cell| (key.clone(), cell.slot)))
        .collect();
    ReadResult {
        capture: MercCapture {
            captured_at_ms,
            live: true,
            scale: layout.scale,
            screen: frame.screen(),
            panel: None,
            header: layout.header.clone(),
            rows,
            rows_on_screen,
            rows_read,
            partial: false,
        },
        cells: Vec::new(),
        rows_on_screen,
        rows_read,
        sigs: HashMap::new(),
        carried,
    }
}

/// A kept row's cells at `row`'s rects: identity and read state from the kept
/// read, geometry from this layout. A slot this layout row does not have keeps
/// its kept rect.
fn copied_cells(known: &MercRow, row: &MercLayoutRow) -> Vec<MercSupportRead> {
    known
        .supports
        .iter()
        .map(|cell| MercSupportRead {
            rect: row.cells.get(cell.slot as usize).copied().unwrap_or(cell.rect),
            ..cell.clone()
        })
        .collect()
}

/// The kept cell at `slot`, when the kept read is confident about it.
fn confident_cell(known: &MercRow, slot: u8) -> Option<&MercSupportRead> {
    known
        .supports
        .iter()
        .find(|cell| cell.slot == slot && confident(cell.state))
}

/// How many rows claim each row key — the collision count both builders share.
fn key_counts(row_keys: &[String]) -> HashMap<&str, usize> {
    let mut counts: HashMap<&str, usize> = HashMap::with_capacity(row_keys.len());
    for key in row_keys {
        *counts.entry(key.as_str()).or_insert(0) += 1;
    }
    counts
}

/// The skill-icon sensor over every layout row, and the `rows_on_screen` count
/// it makes: the occupied icons plus one probe a pitch above the first row and
/// one a pitch below the last.
///
/// Pixel reads only, so every round runs it whatever its plan (POE-278). The
/// per-row half is also what [`build_planned`]'s trailing-row trim keys on.
fn icon_sensor(
    img: &DynamicImage,
    frame: Frame,
    layout: &MercLayout,
    g: &MercGeometry,
) -> (Vec<bool>, usize) {
    let row_icons: Vec<bool> = layout
        .rows
        .iter()
        .map(|row| occupied(img, frame.local(row.skill_icon), g))
        .collect();
    // The icon sensor also samples one pitch outside the enumerated geometry.
    // Those probes are counters only: there is no row geometry to publish for
    // them, but they catch a panel that extends just beyond the seed.
    let pitch = if layout.row_pitch.is_finite() && layout.row_pitch > 0.0 {
        layout.row_pitch
    } else {
        g.row_pitch * layout.scale
    };
    let mut rows_on_screen = row_icons.iter().filter(|&&occupied| occupied).count();
    if pitch.is_finite() && pitch > 0.0 {
        if let Some(first) = layout.rows.first() {
            let above = shifted_y(first.skill_icon, -(pitch.round() as i32));
            if occupied(img, frame.local(above), g) {
                rows_on_screen += 1;
            }
        }
        if let Some(last) = layout.rows.last() {
            let below = shifted_y(last.skill_icon, pitch.round() as i32);
            if occupied(img, frame.local(below), g) {
                rows_on_screen += 1;
            }
        }
    }
    (row_icons, rows_on_screen)
}

fn shifted_y(rect: [i32; 4], delta: i32) -> [i32; 4] {
    [rect[0], rect[1] + delta, rect[2], rect[3]]
}

// ---------------------------------------------------------------------------
// The read plan (POE-278, ADR-025 clauses 2 and 3)
// ---------------------------------------------------------------------------

/// What one round does with one layout row. See [`build_planned`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowPlan {
    /// Pass 2 for the name band and the full icon walk.
    Read,
    /// The kept skill verbatim and the occupancy walk: the kept read's
    /// confident cells copied, its unknown cells matched. The row has kept
    /// cells that are not confident.
    Cells,
    /// The kept skill verbatim and the occupancy walk, as [`Self::Cells`] —
    /// every kept cell is confident and copied, so the walk can only match a
    /// slot past the row's last kept cell that reads occupied: a slot no read
    /// has seen (its art was still drawing on round 1). Pixels only unless one
    /// turns up.
    Unseen,
}

impl RowPlan {
    /// Whether the round re-OCRs this row's name band (pass 2). Only a
    /// [`Self::Read`] row does: the other two keep the kept skill.
    pub fn reads_name(self) -> bool {
        self == RowPlan::Read
    }
}

/// A header field [`header_complete`] reads — the wager is not one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderField {
    /// Missing, or not name-shaped ([`super::geometry::is_name_shaped`]).
    Name,
    Class,
    Level,
}

impl HeaderField {
    pub fn label(self) -> &'static str {
        match self {
            HeaderField::Name => "name",
            HeaderField::Class => "class",
            HeaderField::Level => "level",
        }
    }
}

/// What one reading round of a live capture OCRs, decided from the KEPT
/// capture before any of the round's OCR is paid for (POE-278).
///
/// The owner's rule, ADR-025: one full read; while that read is incomplete, at
/// most `run::RETRIES` more rounds, each re-reading only what the kept read
/// leaves unknown; then only the presence check. Pass 1 on the placed crop is
/// that presence check and runs on every detect tick whatever the plan says —
/// which is also why the header, which pass 1 reads, is folded on every round
/// including [`Self::Nothing`] (see [`fold_unresolved_header`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadPlan {
    /// Pass 2 for every row, the icon walk for every row: round 1, and a kept
    /// capture that no longer lines up with the layout ([`plan_read`]).
    Full,
    /// Re-read only what the kept capture leaves unknown.
    Partial {
        /// One entry per layout row, in layout order.
        rows: Vec<RowPlan>,
        /// The kept cells that are not confident in the [`RowPlan::Cells`]
        /// rows — the cells this round is for.
        cells: usize,
        /// The kept header's unresolved fields, folded from pass 1.
        header: Vec<HeaderField>,
    },
    /// Nothing to re-read: the kept capture is complete, or the rounds are
    /// spent. The kept capture is carried ([`carry_capture`]).
    Nothing,
}

impl ReadPlan {
    /// Whether this round reads anything — every plan but [`Self::Nothing`],
    /// and the rounds `run::LoopState` counts.
    pub fn reads(&self) -> bool {
        *self != ReadPlan::Nothing
    }

    /// What the round did, for the read line: `full`, `nothing`, or
    /// `re-read R rows, C cells, header <fields>`.
    pub fn describe(&self) -> String {
        match self {
            ReadPlan::Full => "full".to_string(),
            ReadPlan::Nothing => "nothing".to_string(),
            ReadPlan::Partial { rows, cells, header } => format!(
                "re-read {} rows, {cells} cells, header {}",
                rows.iter().filter(|row| row.reads_name()).count(),
                header_fields_text(header),
            ),
        }
    }
}

/// `name/class`, or `none` — the header half of the read lines.
pub fn header_fields_text(fields: &[HeaderField]) -> String {
    if fields.is_empty() {
        return "none".to_string();
    }
    fields.iter().map(|field| field.label()).collect::<Vec<_>>().join("/")
}

/// The plan for the round about to run.
///
/// `kept` is the capture the loop holds, and `None` both for a first look and
/// for round 1 of a refilled budget — round 1 reads everything, whatever is
/// kept. `rounds_left` is how many rounds the budget still allows, this one
/// included; `panel` and `layout` are this tick's.
///
/// In order:
///
/// 1. no kept capture → [`ReadPlan::Full`];
/// 2. the kept capture complete, or no rounds left → [`ReadPlan::Nothing`] —
///    before the geometry test, so a jittery geometry cannot buy reads forever;
/// 3. the kept capture does not [line up](lines_up) with this layout →
///    [`ReadPlan::Full`], which the caller counts as a round like any other;
/// 4. otherwise [`ReadPlan::Partial`], per layout row, against the kept row of
///    the same `index`: no kept row, or a skill that is not [`confident`] →
///    [`RowPlan::Read`]; a confident skill with a cell that is not →
///    [`RowPlan::Cells`]; everything confident → [`RowPlan::Unseen`], which
///    still walks for a slot past the last kept cell (a slot never seen is
///    unknown too).
pub fn plan_read(
    kept: Option<&MercCapture>,
    rounds_left: u8,
    panel: Option<[i32; 4]>,
    layout: &MercLayout,
    g: &MercGeometry,
) -> ReadPlan {
    let Some(kept) = kept else {
        return ReadPlan::Full;
    };
    if rounds_left == 0 || capture_complete(kept) {
        return ReadPlan::Nothing;
    }
    if !lines_up(kept, panel, layout, g) {
        return ReadPlan::Full;
    }
    let mut cells = 0;
    let rows = layout
        .rows
        .iter()
        .map(|row| {
            let Some(known) = kept.rows.iter().find(|k| k.index == row.index) else {
                return RowPlan::Read;
            };
            if !confident(known.skill.state) {
                return RowPlan::Read;
            }
            let unread = known.supports.iter().filter(|cell| !confident(cell.state)).count();
            if unread == 0 {
                RowPlan::Unseen
            } else {
                cells += unread;
                RowPlan::Cells
            }
        })
        .collect();
    ReadPlan::Partial {
        rows,
        cells,
        header: unresolved_header_fields(&kept.header),
    }
}

/// Whether the kept capture is a read of THIS layout's geometry, so its rows
/// and cells may be matched to the layout's by `index` and `slot`.
///
/// All three, exactly:
///
/// - the kept `panel` equals this tick's panel rect — both are the loop's
///   settled rect (the SSOT placement on a placed tick), not a per-tick
///   measurement, so any difference is a moved placement;
/// - every kept row's `index` is a layout row's;
/// - every kept cell has a layout cell at the same `(index, slot)` of the same
///   width and height, whose origin lies inside the half-cell band
///   (`geometry::column_tolerance`, the band a placed panel's column and origin
///   are held to) on both axes. The size test is what catches an adopted cell
///   size; the band absorbs the per-tick jitter of the OCR-seeded row centres
///   and the frame fit, which is far less than the half cell that would put a
///   cell on its neighbour.
///
/// A layout row the kept capture has no row for does not break the match — it
/// is an unknown row, and [`plan_read`] reads it.
fn lines_up(
    kept: &MercCapture,
    panel: Option<[i32; 4]>,
    layout: &MercLayout,
    g: &MercGeometry,
) -> bool {
    if kept.panel != panel {
        return false;
    }
    let tolerance = column_tolerance(g, layout.scale);
    kept.rows.iter().all(|known| {
        let Some(row) = layout.rows.iter().find(|row| row.index == known.index) else {
            return false;
        };
        known.supports.iter().all(|cell| match row.cells.get(cell.slot as usize) {
            Some(rect) => {
                rect[2] == cell.rect[2]
                    && rect[3] == cell.rect[3]
                    && (rect[0] - cell.rect[0]).abs() <= tolerance
                    && (rect[1] - cell.rect[1]).abs() <= tolerance
            }
            None => false,
        })
    })
}

/// `(family, tier)` → the vocabulary link(s) it names (D4's resolution table).
///
/// Both halves must be confident: a matched template with NO tier has named
/// one of the 153 FAMILIES and nothing narrower — which of that family's
/// up-to-3 tier links it is stays open — so it is `Unknown` with the family
/// recorded, never a guess at tier 1.
fn resolve_cell(
    icon: &super::icons::IconMatch,
    tier: Option<u8>,
    vocab: &MercVocab,
) -> (Option<String>, Vec<String>, Option<String>, ReadState, Vec<String>) {
    match (icon.state, icon.family.clone(), tier) {
        (ReadState::Matched, Some(family), Some(t)) => {
            let matches = vocab.resolve(&family, t);
            let (ids, name, state, candidates) = classify_resolution(&matches);
            (Some(family), ids, name, state, candidates)
        }
        (ReadState::Matched, Some(family), None) => {
            (Some(family), Vec::new(), None, ReadState::Unknown, Vec::new())
        }
        (ReadState::LowConfidence, Some(family), t) => {
            let candidates = t
                .map(|t| {
                    vocab
                        .resolve(&family, t)
                        .iter()
                        .map(|s| s.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            (
                Some(family),
                Vec::new(),
                None,
                ReadState::LowConfidence,
                candidates,
            )
        }
        _ => (None, Vec::new(), None, ReadState::Unknown, Vec::new()),
    }
}

// ---------------------------------------------------------------------------
// The sticky header (2026-08-25 smoke)
// ---------------------------------------------------------------------------

/// Fold a re-read header into the one already on screen.
///
/// MEASURED on the 2026-08-25 Windows smoke: with the re-detect running every
/// 2 s, the strip's header BLINKED between `Fennik, of Unshakeable Faith ·
/// class not read · lvl 83` and `@ Fallen Reverend · @ Fallen Reverend · lvl
/// 83`. Nothing on screen changed between those ticks — the OCR simply read
/// the same pixels differently, and the loop published the newest read
/// whatever it said. A header that rewrites itself twice a minute is unusable
/// for the one thing it exists for: telling the player who this is.
///
/// So a live capture's header only ever gets BETTER:
///
/// - a field that was read once is never un-read — `None` never overwrites
///   `Some`, because "not read this tick" is not evidence that the panel stopped
///   showing it;
/// - a field that was read is replaced only by a STRICTLY better read, which is
///   [`better_read`]'s two-part rule: no leading glyph beats a leading glyph,
///   and at equal cleanliness more alphanumeric content beats less.
///
/// The numbers are on a different footing and take the newer read whenever
/// there is one: a level has no "content quality" to compare, so the only
/// sticky rule that applies to it is the first one. A mercenary's level does
/// not change while the window is open, so a differing re-read is OCR noise
/// either way — but "prefer the older" and "prefer the newer" are equally
/// arbitrary there, and preferring the newer keeps the rule to one sentence.
///
/// Pure so the rule is testable without a screen: the loop hands it the header
/// it published last and the header this detect produced.
pub fn merge_header(prev: &super::MercHeader, next: &super::MercHeader) -> super::MercHeader {
    let class = merge_text(prev.class.as_deref(), next.class.as_deref());
    let name = merge_text(prev.name.as_deref(), next.name.as_deref());
    super::MercHeader {
        // A merged name that equals the merged class is the smoke's
        // `@ Fallen Reverend · @ Fallen Reverend` arriving by a second route:
        // the parse rejects a title that IS the class line, but a name kept
        // from an earlier tick can collide with a class read for the first
        // time this one. The previous name is tried, and a field with nothing
        // uncollided to show goes back to `None` — "not read" is a true
        // statement, "the class" is not.
        name: match (&name, &class) {
            (Some(name), Some(class)) if same_field(name, class) => {
                prev.name.clone().filter(|kept| !same_field(kept, class))
            }
            _ => name,
        },
        class,
        // FIRST wins for the numbers, the opposite of the text rule. A level
        // does not change while one window is open, so a differing re-read is
        // noise — and keeping the first reading means the strip's level stops
        // moving once it has one, which is the whole point of the sticky
        // header. Taking the newer would have left `lvl 83` flicking to `88`
        // on a bad tick. Safe because a panel SWAP no longer merges at all
        // (see [`fold_header`]), so first-wins cannot outlive its window.
        level: prev.level.or(next.level),
        wager: prev.wager.or(next.wager),
    }
}

/// The header a round that did not read the panel afresh publishes (POE-278):
/// a field the kept header has RESOLVED stays verbatim, and an unresolved one
/// ([`unresolved_header_fields`]) is folded from this frame's pass-1 header by
/// [`merge_header`]'s rules. The wager follows [`merge_header`].
///
/// [`merge_header`] alone would let a strictly better read replace a resolved
/// field; a partial round, and a liveness tick, read nothing to replace it
/// with. Pass 1 is the presence check and runs on every detect tick anyway, so
/// the fold costs no OCR and goes on for as long as a field is unresolved —
/// round-spent liveness ticks included.
pub fn fold_unresolved_header(
    kept: &super::MercHeader,
    pass1: &super::MercHeader,
) -> super::MercHeader {
    let merged = merge_header(kept, pass1);
    let open = unresolved_header_fields(kept);
    let open_field = |field| open.contains(&field);
    super::MercHeader {
        name: if open_field(HeaderField::Name) { merged.name } else { kept.name.clone() },
        class: if open_field(HeaderField::Class) { merged.class } else { kept.class.clone() },
        level: if open_field(HeaderField::Level) { merged.level } else { kept.level },
        wager: merged.wager,
    }
}

/// Whether two header fields are the same reading, for the name/class clash.
///
/// Cleaned and case-folded, because the two fields come off different OCR
/// lines: `@ Fallen Reverend` and `fallen reverend` are the same claim, and a
/// byte comparison would let the collision through.
fn same_field(a: &str, b: &str) -> bool {
    super::geometry::clean_header_text(a).to_lowercase()
        == super::geometry::clean_header_text(b).to_lowercase()
}

/// The header for a re-read, plus whether the panel is a DIFFERENT window.
///
/// The sticky merge above is only ever correct for ONE recruit window. A
/// REMATCH swaps the mercenary behind an identical-looking panel, and since
/// the liveness pause the loop can take ~20 s to notice a window that closed —
/// so "a capture exists" is not evidence that the capture is of the same
/// mercenary. Without this gate the new mercenary would inherit the old one's
/// name, class and (first-wins) level: a confident, wrong header on the surface
/// the player pays from.
///
/// So identity is checked first ([`same_panel`]) and only a match merges. A
/// different panel returns the fresh header VERBATIM and `true`, which is the
/// loop's signal to drop everything it remembered about the old window.
pub fn fold_header(previous: Option<&MercCapture>, next: &MercCapture) -> (super::MercHeader, bool) {
    match previous {
        Some(prev) if panel_replaced(prev, next) => (next.header.clone(), true),
        Some(prev) => (merge_header(&prev.header, &next.header), false),
        None => (next.header.clone(), false),
    }
}

/// How many rows each read must have NAMED before their skill sets are allowed
/// to argue about identity.
///
/// One named row is not enough on either side: a single misread name would then
/// be a "different mercenary", and the panel's whole memory would be thrown away
/// on the sort of tick this module sees constantly.
const REPLACEMENT_ROW_EVIDENCE: usize = 2;

/// Whether `next` is a read of a DIFFERENT recruit window than `prev`.
///
/// **Positive evidence only, and the asymmetry is the whole design.** The two
/// answers cost very different things:
///
/// - saying "same window" when it is not inherits the last mercenary's name,
///   class and level — a confident, wrong header;
/// - saying "different window" when it is not throws away the remembered
///   confirmations, including the AMBIGUOUS resolutions that only live in this
///   session (the template store cannot hold "which of these two names"), and
///   lets one bad tick's header through verbatim.
///
/// Both are real, so the rule refuses to decide on absence: an unreadable tick
/// ABSTAINS and everything is kept. Only two facts are evidence of a swap:
///
/// - **two levels that disagree.** Both must be read; a level of `None` proves
///   nothing, since the header line is missed often;
/// - **two skill sets that are DISJOINT**, with at least
///   [`REPLACEMENT_ROW_EVIDENCE`] named rows on each side. Sets, not positions:
///   a dropped or misread row shifts every later index, and an index-wise
///   comparison read that as a different mercenary — the failure this rule
///   replaces. Sharing even one skill is enough to keep the window, because a
///   rematch rolls a whole new skill list.
///
/// A REMATCH clears both bars easily (different level, or six different
/// skills). A bad tick clears neither.
pub fn panel_replaced(prev: &MercCapture, next: &MercCapture) -> bool {
    if let (Some(before), Some(now)) = (prev.header.level, next.header.level) {
        if before != now {
            return true;
        }
    }
    let before = named_skills(prev);
    let now = named_skills(next);
    before.len() >= REPLACEMENT_ROW_EVIDENCE
        && now.len() >= REPLACEMENT_ROW_EVIDENCE
        && before.is_disjoint(&now)
}

/// The skills a capture actually named, as a set. Unread rows are not in it —
/// they are the absence this rule refuses to reason from.
fn named_skills(capture: &MercCapture) -> std::collections::HashSet<&str> {
    capture
        .rows
        .iter()
        .filter_map(|row| row.skill.name.as_deref())
        .collect()
}

/// Whether `next` is POSITIVELY a read of the SAME recruit window as `retired`.
///
/// The companion to [`panel_replaced`], and deliberately NOT its negation.
/// That rule ABSTAINS on absence — an unreadable tick keeps the window — which
/// is right for a LIVE capture: the alternative is throwing a session's
/// confirmations away on one bad tick, and the next tick two seconds later can
/// put them back either way.
///
/// It is wrong across a RETIRE. There the gap is a window that left the screen
/// and up to a minute of wall clock, so abstention is no longer a cheap bet: a
/// single shared skill (Flame Dash sits on more than one reference panel) plus
/// a level neither read named would abstain its way into writing one
/// mercenary's supports onto another's rows as `Confirmed` — and a confirmed
/// cell is never re-read, so no hover can correct it.
///
/// The burden therefore flips. Two conditions, both required:
///
/// - nothing may CONTRADICT sameness — [`panel_replaced`] owns that half, so
///   the two rules cannot drift apart and the live path keeps its abstention;
/// - and something must POSITIVELY say it is the same panel: two levels that
///   were both read and agree, or skill sets overlapping on at least HALF of
///   what the new read named, with [`REPLACEMENT_ROW_EVIDENCE`] named rows on
///   each side.
///
/// The level disjunct is what lets the ordinary case through: the first tick
/// after a re-detect often names no skill at all but does read the header line,
/// and requiring both facts would drop every such restore.
///
/// But a level is only allowed to speak for rows nobody read. Once BOTH sides
/// have named [`REPLACEMENT_ROW_EVIDENCE`] rows, a sub-half overlap is a
/// present, positive DISAGREEMENT — and two mercenaries sharing a level is
/// ordinary, so the skills outvote it. Without that the level disjunct would
/// restore across `Flame Dash` plus a level collision, which is the exact
/// mis-restore this rule exists to prevent.
pub fn same_panel_positive(retired: &MercCapture, next: &MercCapture) -> bool {
    if panel_replaced(retired, next) {
        return false;
    }
    let levels_agree = matches!(
        (retired.header.level, next.header.level),
        (Some(before), Some(now)) if before == now
    );
    let before = named_skills(retired);
    let now = named_skills(next);
    let shared = before.intersection(&now).count();
    let both_read_enough_rows =
        before.len() >= REPLACEMENT_ROW_EVIDENCE && now.len() >= REPLACEMENT_ROW_EVIDENCE;
    let skills_agree = both_read_enough_rows && shared * 2 >= now.len();
    // Not merely "skills_agree is false": absence still abstains, so this is
    // true only when both reads named enough rows to be arguing about the
    // same thing and the overlap came out short.
    let skills_contradict = both_read_enough_rows && shared * 2 < now.len();
    !skills_contradict && (levels_agree || skills_agree)
}

/// One text field's sticky rule. See [`merge_header`].
fn merge_text(prev: Option<&str>, next: Option<&str>) -> Option<String> {
    match (prev, next) {
        (Some(prev), Some(next)) if better_read(prev, next) => Some(next.to_string()),
        (Some(prev), _) => Some(prev.to_string()),
        (None, next) => next.map(str::to_string),
    }
}

/// Whether `next` is a strictly better read of the same header field than
/// `prev`.
///
/// Two signals, in order, both measured on the smoke screenshots:
///
/// 1. **A leading non-alphanumeric is a bad read.** `@ Fallen Reverend` is the
///    class ICON read as a glyph. [`super::geometry::clean_header_text`] strips
///    it at parse time, so this is the backstop for a producer that does not —
///    a clean read beats a glyphed one whatever their lengths.
/// 2. **More content is a better read.** At equal cleanliness, the read with
///    more alphanumeric characters won: OCR drops characters off a bad read far
///    more often than it invents them, so `Fennik, of Unshakeable Faith` beats
///    `Fennik, of Unshak`. Ties lose — a different read of the same length is
///    not evidence of anything, and swapping on it is the blink itself.
fn better_read(prev: &str, next: &str) -> bool {
    let clean_prev = starts_clean(prev);
    let clean_next = starts_clean(next);
    if clean_prev != clean_next {
        return clean_next;
    }
    alnum_len(next) > alnum_len(prev)
}

fn starts_clean(text: &str) -> bool {
    text.trim()
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric())
}

fn alnum_len(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphanumeric()).count()
}

// ---------------------------------------------------------------------------
// When there is nothing left to read
// ---------------------------------------------------------------------------

/// Whether a capture has nothing left for another OCR pass to improve.
///
/// Every row's skill name confident, every support cell the panel SHOWS
/// confident, and the three header fields the strip prints all read. `wager` is
/// deliberately not part of it: it is absent from the OCR on real dumps (see
/// `geometry.rs`'s Windows-dump test) and nothing in the module reads it, so
/// requiring it would mean the module never stops reading.
///
/// A row with no support cells at all is complete — an empty `supports` is a
/// skill the panel shows without supports, not a row that failed (`build_capture`
/// stops at the first unoccupied slot, so a cell that IS in the list is a cell
/// that is on screen).
///
/// This is what stops the re-reading (ADR-025 clause 3): a complete kept
/// capture plans [`ReadPlan::Nothing`], and the loop drops to the liveness
/// cadence, where the detect OCRs the placed crop to know the window is still
/// there (and to notice a REMATCH) and re-reads nothing. The other stop is the
/// round budget running out ([`plan_read`]). The hover tick keeps running — a
/// tooltip can still contradict a confident wrong match, which no re-detect
/// ever would.
pub fn capture_complete(capture: &MercCapture) -> bool {
    header_complete(&capture.header)
        && capture.rows.iter().all(|row| {
            confident(row.skill.state) && row.supports.iter().all(|cell| confident(cell.state))
        })
}

/// The header fields the strip prints, all read. See [`capture_complete`].
///
/// The name must also have the SHAPE of a name
/// ([`super::geometry::is_name_shaped`]), not merely be `Some`. This is the
/// backstop, not the guard: the parse already refuses a tooltip-shaped
/// candidate, but a name can also arrive here from a retained capture restored
/// across a retire (`run.rs`'s `restore_retained`) or from an earlier tick the
/// sticky merge kept — both paths pre-date the parse guard and neither
/// re-checks. What this gate protects is the edge `capture_complete` fires:
/// completeness is what opens the trade session (POE-202) and sends the name to
/// GGG as the query's label, which is where `SUPPORTED SKILLS PENETRATE
/// 100/GlRE` actually went on 2026-08-26. A capture whose name fails the shape
/// test is not complete, so the loop keeps reading until a clean frame gives it
/// one.
pub fn header_complete(header: &super::MercHeader) -> bool {
    unresolved_header_fields(header).is_empty()
}

/// The fields [`header_complete`] does not accept, in print order: a name that
/// is missing or not name-shaped, a missing class, a missing level.
pub fn unresolved_header_fields(header: &super::MercHeader) -> Vec<HeaderField> {
    let mut open = Vec::new();
    if !header.name.as_deref().is_some_and(super::geometry::is_name_shaped) {
        open.push(HeaderField::Name);
    }
    if header.class.is_none() {
        open.push(HeaderField::Class);
    }
    if header.level.is_none() {
        open.push(HeaderField::Level);
    }
    open
}

/// The two states a hover cannot improve. The same pair the verdict engine
/// treats as confident (`verdict.ts`'s `CONFIDENT_STATES`).
pub(super) fn confident(state: ReadState) -> bool {
    matches!(state, ReadState::Matched | ReadState::Confirmed)
}

/// The colour crop a template is learned from and the dump shows. `None` when
/// the rect does not lie wholly inside the image.
pub fn crop_rgba(img: &DynamicImage, rect: [i32; 4], g: &MercGeometry) -> Option<RgbaImage> {
    let [x, y, w, h] = inner_rect(rect, g);
    if x < 0 || y < 0 || w <= 0 || h <= 0 {
        return None;
    }
    let (iw, ih) = img.dimensions();
    if (x + w) as u32 > iw || (y + h) as u32 > ih {
        return None;
    }
    Some(
        img.crop_imm(x as u32, y as u32, w as u32, h as u32)
            .to_rgba8(),
    )
}

/// Pass 2 (D2): re-OCR each row's name band on its own — every row, the full
/// read's form. The capture loop runs it on round 1 of a capture only; a later
/// round re-OCRs just the rows its plan reads ([`pass2_planned`]), and a round
/// that reads nothing runs no pass 2 at all (POE-278).
///
/// The placed-crop pass 1 reads the name at native size; a 44 px-tall band goes
/// through `preprocess_for_ocr`, which upscales it 2× and stretches its
/// contrast — measurably better on small text (POE-116). The debug command's
/// full-screen replay uses the same pass-2 path.
///
/// Every failure falls back to the pass-1 text rather than blanking the row:
/// an off-image band, an OCR error (which is EVERY call on non-Windows), an
/// empty result, and a row past `max_rows` all keep pass 1. The debug dump
/// keeps both texts so a pass-2 regression is visible rather than merely
/// suspected.
///
/// The `max_rows` bound is what keeps ONE tick's cost bounded: this is a
/// per-row OCR call, and a mis-clustered detect could otherwise produce
/// arbitrarily many rows inside a single tick.
pub fn pass2_texts(
    img: &DynamicImage,
    frame: Frame,
    layout: &MercLayout,
    g: &MercGeometry,
) -> Vec<String> {
    pass2_for(img, frame, layout, g, &[])
}

/// Pass 2 for the rows a partial round reads ([`RowPlan::reads_name`]) and
/// no others; every other row keeps its pass-1 text, which
/// [`build_planned`] does not read for a row it copies. A layout row past the
/// end of `rows` is read, as in [`pass2_texts`].
pub fn pass2_planned(
    img: &DynamicImage,
    frame: Frame,
    layout: &MercLayout,
    g: &MercGeometry,
    rows: &[RowPlan],
) -> Vec<String> {
    pass2_for(img, frame, layout, g, rows)
}

fn pass2_for(
    img: &DynamicImage,
    frame: Frame,
    layout: &MercLayout,
    g: &MercGeometry,
    rows: &[RowPlan],
) -> Vec<String> {
    let (iw, ih) = img.dimensions();
    let budget = pass2_row_budget(layout.rows.len(), g);
    layout
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            if i >= budget || !rows.get(i).map_or(true, |plan| plan.reads_name()) {
                return row.text.clone();
            }
            let [x, y, w, h] = frame.local(name_band(row, layout, g));
            if x < 0 || y < 0 || w <= 0 || h <= 0 || (x + w) as u32 > iw || (y + h) as u32 > ih {
                return row.text.clone();
            }
            let band = img.crop_imm(x as u32, y as u32, w as u32, h as u32);
            let processed = crate::capture::preprocess_for_ocr(&band);
            match crate::ocr::recognize_text(&processed) {
                Ok(lines) if !lines.is_empty() => lines.join(" ").trim().to_string(),
                _ => row.text.clone(),
            }
        })
        .collect()
}

/// How many rows pass 2 will re-OCR, given how many the detect produced.
///
/// One OCR call per row, all inside a single tick, so this is the number that
/// bounds the tick — which is why it is a named decision rather than a `min`
/// buried in a loop. Every row past it keeps its pass-1 text.
pub fn pass2_row_budget(rows: usize, g: &MercGeometry) -> usize {
    rows.min(g.max_rows as usize)
}

/// The crop pass 2 re-reads: the row's name text, widened to the first support
/// cell and padded by 4 scaled px so no glyph is clipped at the edge.
///
/// The right edge comes from the CELL column, not from the name's own width: a
/// pass-1 read that stopped short (the reason we are re-reading at all) would
/// otherwise crop the very characters it missed.
///
/// It takes the ROW, not just its `name_rect`, so that edge is the row's own
/// fitted slot-0 rect rather than a second derivation of it (POE-214 A7). Once
/// `layout.scale` is the frame-measured one the two differ — measured 0.22 px
/// on the reference fixture and 1.4 px at the PC — and the rect is the one the
/// crop must stop short of, since it is where the icon actually starts.
/// `g` remains the fallback for a layout with no cells at all (`max_slots` 0
/// through an override).
fn name_band(row: &MercLayoutRow, layout: &MercLayout, g: &MercGeometry) -> [i32; 4] {
    let pad = (NAME_CROP_PAD * layout.scale).round() as i32;
    let name_rect = row.name_rect;
    let x = name_crop_left(name_rect[0], layout.scale);
    let cells_x0 = row
        .cells
        .first()
        .map(|cell| cell[0])
        .unwrap_or_else(|| layout.column_x0 + (g.cell_offset_x * layout.scale).round() as i32);
    let right = (cells_x0 - pad).max(x + 1);
    [x, name_rect[1] - pad, right - x, name_rect[3] + 2 * pad]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mercenary::geometry::{detect, OcrLineBox, NAME_CROP_LEAD};
    use image::{Rgba, RgbaImage};

    fn vocab() -> MercVocab {
        MercVocab::load().expect("vocabulary parses")
    }

    /// A screen with the reference panel's line layout drawn as OCR boxes: the
    /// Wager anchor plus two skill names one row pitch apart, at scale 1.
    fn reference_lines() -> Vec<OcrLineBox> {
        vec![
            OcrLineBox { text: "Wager: 1 028".into(), x: 100, y: 40, w: 90, h: 16 },
            OcrLineBox { text: "Ice Shot".into(), x: 100, y: 92, w: 64, h: 16 },
            OcrLineBox { text: "Conductivity".into(), x: 100, y: 141, w: 96, h: 16 },
        ]
    }

    /// A flat dark screen — every support slot reads as empty on it.
    fn flat_screen(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, Rgba([12, 12, 14, 255])))
    }

    /// Paint noise into a rect so its stddev clears `empty_cell_stddev`. The
    /// pattern is deterministic and high-contrast — what matters is that the
    /// occupancy rule sees a filled cell, not what the "icon" depicts.
    fn fill_noise(img: &mut RgbaImage, rect: [i32; 4]) {
        for dy in 0..rect[3] {
            for dx in 0..rect[2] {
                let v = if (dx / 3 + dy / 3) % 2 == 0 { 20 } else { 220 };
                img.put_pixel(
                    (rect[0] + dx) as u32,
                    (rect[1] + dy) as u32,
                    Rgba([v, v, v, 255]),
                );
            }
        }
    }

    /// The test images ARE the screen — every read.rs test predates crops and
    /// must keep meaning what it meant.
    fn whole(img: &DynamicImage) -> Frame {
        let (w, h) = img.dimensions();
        Frame::full([w, h])
    }

    fn layout_of() -> MercLayout {
        detect(&reference_lines(), &MercGeometry::default(), &vocab(), None)
            .expect("the reference lines detect as a panel")
    }

    /// The skill column is what the capture is FOR: the row's text has to reach
    /// the vocabulary and come back as an identified skill, per row.
    #[test]
    fn each_row_carries_its_matched_skill_read() {
        let img = flat_screen(900, 300);
        let layout = layout_of();
        let g = MercGeometry::default();

        let out = build_capture(&img, whole(&img), &layout, &[], 1_700_000_000_000, &g, &vocab(), &TemplateStore::new());

        assert_eq!(out.capture.rows.len(), 2);
        assert_eq!(out.capture.rows[0].skill.name.as_deref(), Some("Ice Shot"));
        assert_eq!(out.capture.rows[0].skill.state, ReadState::Matched);
        assert_eq!(out.rows_on_screen, 0);
        assert_eq!(out.rows_read, 2);
        assert!(
            !out.capture.rows[0].skill.ids.is_empty(),
            "a matched skill must carry the vocabulary id the verdict engine keys on",
        );
        assert_eq!(out.capture.rows[1].skill.name.as_deref(), Some("Conductivity"));
    }

    /// `row_texts` is the pass-2 seam: what the caller re-read wins over the
    /// pass-1 text the layout carries. Without this the re-OCR is decoration.
    #[test]
    fn a_pass_two_text_replaces_the_pass_one_text_for_matching() {
        let img = flat_screen(900, 300);
        let layout = layout_of();
        let g = MercGeometry::default();

        let out = build_capture(
            &img,
            whole(&img),
            &layout,
            &["Frostbolt".to_string(), "Conductivity".to_string()],
            0,
            &g,
            &vocab(),
            &TemplateStore::new(),
        );

        assert_eq!(out.capture.rows[0].skill.raw, "Frostbolt");
        assert_eq!(
            out.capture.rows[0].skill.name.as_deref(),
            Some("Frostbolt"),
            "the pass-1 text said Ice Shot; the row must report what pass 2 read",
        );
    }

    /// A panel whose slots are all flat dark yields NO supports — the occupancy
    /// gate is what stops the reader inventing six cells per row out of empty
    /// panel, and an empty slot's near-constant signature would poison the
    /// template store if it were ever learned.
    #[test]
    fn empty_slots_produce_no_support_reads() {
        let img = flat_screen(900, 300);
        let layout = layout_of();
        let g = MercGeometry::default();

        let out = build_capture(&img, whole(&img), &layout, &[], 0, &g, &vocab(), &TemplateStore::new());

        assert!(out.capture.rows.iter().all(|r| r.supports.is_empty()));
        assert!(out.sigs.is_empty(), "no signature is cached for an empty slot");
        assert!(
            out.cells.iter().all(|c| !c.occupied),
            "the debug cells still record the rejected slots and why",
        );
        assert_eq!(out.rows_on_screen, 0);
        assert_eq!(out.rows_read, 2);
    }

    /// An occupied cell with an EMPTY template store is `unknown` — the page's
    /// "hover to confirm" state. The store ships empty, so this is the first
    /// thing every real capture does.
    #[test]
    fn an_occupied_cell_with_no_learned_template_reads_unknown() {
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        let g = MercGeometry::default();
        let layout = layout_of();
        let first = layout.rows[0].cells[0];
        fill_noise(&mut raw, first);
        let img = DynamicImage::ImageRgba8(raw);

        let out = build_capture(&img, whole(&img), &layout, &[], 0, &g, &vocab(), &TemplateStore::new());

        let supports = &out.capture.rows[0].supports;
        assert_eq!(supports.len(), 1, "scanning stops at the empty second slot");
        assert_eq!(supports[0].slot, 0);
        assert_eq!(supports[0].state, ReadState::Unknown);
        assert!(supports[0].ids.is_empty());
        assert_eq!(out.rows_on_screen, 0);
        assert_eq!(out.rows_read, 2);
        assert!(
            out.sigs.contains_key(&(row_key(&out.capture.rows[0].skill), 0)),
            "the pre-hover crop must be cached so a later confirm can learn it",
        );
    }

    /// The crop cache is keyed by the row's IDENTITY, not by where the row
    /// happened to land in this read (POE-207). `row.index` is sequential over
    /// the rows a detect found, so a skill line the OCR drops renumbers every
    /// row below it and the crop the hover-confirm looks up under the cell it
    /// is standing on is the cell BELOW it — which is how art was learned under
    /// another family's name. The expected key is the skill's vocabulary id,
    /// read off the capture rather than through `row_key`, so a production
    /// change to what the key is made of fails here.
    #[test]
    fn the_crop_cache_is_keyed_by_the_rows_skill_not_its_position() {
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        let layout = layout_of();
        fill_noise(&mut raw, layout.rows[0].cells[0]);
        fill_noise(&mut raw, layout.rows[1].cells[0]);
        let img = DynamicImage::ImageRgba8(raw);

        let out = build_capture(
            &img,
            whole(&img),
            &layout,
            &[],
            0,
            &MercGeometry::default(),
            &vocab(),
            &TemplateStore::new(),
        );

        let second = &out.capture.rows[1];
        assert_eq!(second.index, 1, "arrange: the second row is at position 1");
        let id = second.skill.ids.first().expect("Conductivity resolves to a skill id");
        assert!(
            out.sigs.contains_key(&(id.clone(), 0)),
            "the second row's crop must be cached under its skill id, not its index — \
             keys were {:?}",
            out.sigs.keys().collect::<Vec<_>>(),
        );
        assert!(
            !out.sigs.contains_key(&(second.index.to_string(), 0)),
            "and never under a stringified row position",
        );
    }

    /// TWO ROWS, ONE KEY (POE-207 review, HIGH 1). `row_key` is a resolved
    /// skill id or a lowercased raw OCR line, and a panel can produce the same
    /// one twice — here both rows are re-OCR'd as "Ice Shot", which is exactly
    /// what a fuzzy pass-2 match landing on one skill twice looks like. The two
    /// rows then share a cache key, and the later row's crop would overwrite the
    /// earlier one's: a confirm on row 0 would learn ROW 1's art under whatever
    /// the tooltip named, and the two-read guard cannot catch it because both
    /// reads corroborate the tooltip, not the crop.
    ///
    /// So a colliding row caches nothing at all. Its cells report `NoCrop`, the
    /// confirmation still names them, and nothing wrong is learned.
    #[test]
    fn two_rows_that_resolve_to_one_key_cache_no_crop_at_all() {
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        let layout = layout_of();
        fill_noise(&mut raw, layout.rows[0].cells[0]);
        fill_noise(&mut raw, layout.rows[1].cells[0]);
        let img = DynamicImage::ImageRgba8(raw);

        let out = build_capture(
            &img,
            whole(&img),
            &layout,
            &["Ice Shot".to_string(), "Ice Shot".to_string()],
            0,
            &MercGeometry::default(),
            &vocab(),
            &TemplateStore::new(),
        );

        assert_eq!(
            row_key(&out.capture.rows[0].skill),
            row_key(&out.capture.rows[1].skill),
            "arrange: both rows must resolve to the same key for this to be a collision",
        );
        assert_eq!(
            out.capture.rows[0].supports.len(),
            1,
            "arrange: both rows still READ their painted cell",
        );
        assert!(
            out.sigs.is_empty(),
            "a colliding row caches nothing — one of these crops is the other \
             row's art. Cached: {:?}",
            out.sigs.keys().collect::<Vec<_>>(),
        );
    }

    /// The other side of it: rows that resolve APART keep their crops. Dropping
    /// on any repeated read rather than on a genuine collision would make the
    /// cache empty on every ordinary panel.
    #[test]
    fn rows_that_resolve_to_different_keys_both_keep_their_crops() {
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        let layout = layout_of();
        fill_noise(&mut raw, layout.rows[0].cells[0]);
        fill_noise(&mut raw, layout.rows[1].cells[0]);
        let img = DynamicImage::ImageRgba8(raw);

        let out = build_capture(
            &img,
            whole(&img),
            &layout,
            &["Ice Shot".to_string(), "Conductivity".to_string()],
            0,
            &MercGeometry::default(),
            &vocab(),
            &TemplateStore::new(),
        );

        assert_eq!(out.sigs.len(), 2, "both distinct rows cached their cell");
    }

    /// The whole point of [`Frame`], end to end. The same screen read twice —
    /// once whole, once as a crop of itself — must produce the SAME capture:
    /// the cell rects the hover tick hit-tests the real cursor against are
    /// screen-absolute, and the screen size the tooltip crop is clamped to is
    /// the desktop's, not the crop's. Reading the crop's pixels at the screen's
    /// coordinates would land the occupancy test tens of px off the icon and
    /// the cell would read empty.
    #[test]
    fn a_crop_of_the_screen_reads_the_same_capture_as_the_whole_screen() {
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        let g = MercGeometry::default();
        let layout = layout_of();
        fill_noise(&mut raw, layout.rows[0].cells[0]);
        fill_noise(&mut raw, layout.rows[1].cells[0]);
        let img = DynamicImage::ImageRgba8(raw);
        let origin = (50, 30);
        let cropped = img.crop_imm(origin.0 as u32, origin.1 as u32, 850, 270);
        let frame = Frame::cropped(origin, [900, 300]);

        let whole_read =
            build_capture(&img, whole(&img), &layout, &[], 7, &g, &vocab(), &TemplateStore::new());
        let crop_read =
            build_capture(&cropped, frame, &layout, &[], 7, &g, &vocab(), &TemplateStore::new());

        assert_eq!(
            whole_read.capture.rows[0].supports.len(),
            1,
            "arrange: the whole-screen read finds the painted cell",
        );
        assert_eq!(crop_read.capture, whole_read.capture);
        assert_eq!(crop_read.capture.screen, [900, 300], "the SCREEN, not the crop");
        assert_eq!(
            crop_read.capture.rows[0].supports[0].rect,
            layout.rows[0].cells[0],
            "the published rect is where the cell is on the SCREEN",
        );
        let sorted = |r: &ReadResult| {
            let mut keys: Vec<(String, u8)> = r.sigs.keys().cloned().collect();
            keys.sort();
            keys
        };
        assert_eq!(sorted(&crop_read), sorted(&whole_read));
        let mut expected: Vec<(String, u8)> = whole_read
            .capture
            .rows
            .iter()
            .map(|row| (row.skill.ids[0].clone(), 0u8))
            .collect();
        expected.sort();
        assert_eq!(sorted(&crop_read), expected, "both painted cells cached a crop");
    }

    /// The reader stops at the FIRST empty slot: a filled slot 1 behind an
    /// empty slot 0 must not be read, because "the panel ends here" is what an
    /// empty slot means.
    #[test]
    fn slot_scanning_stops_at_the_first_empty_slot() {
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        let layout = layout_of();
        fill_noise(&mut raw, layout.rows[0].cells[1]);
        let img = DynamicImage::ImageRgba8(raw);

        let out = build_capture(
            &img,
            whole(&img),
            &layout,
            &[],
            0,
            &MercGeometry::default(),
            &vocab(),
            &TemplateStore::new(),
        );

        assert!(
            out.capture.rows[0].supports.is_empty(),
            "slot 1 is behind an empty slot 0 and must not be read",
        );
    }

    /// A matched template with NO readable badge stays `unknown`: the family
    /// alone names up to three different links, and guessing tier 1 would put a
    /// confident wrong id in front of the verdict engine.
    #[test]
    fn a_known_family_without_a_tier_stays_unknown() {
        let icon = super::super::icons::IconMatch {
            family: Some("Pierce".into()),
            learned_tier: Some(2),
            score: 0.97,
            runner_up: 0.1,
            state: ReadState::Matched,
        };

        let (family, ids, name, state, _) = resolve_cell(&icon, None, &vocab());

        assert_eq!(family.as_deref(), Some("Pierce"));
        assert_eq!(state, ReadState::Unknown);
        assert!(ids.is_empty());
        assert!(name.is_none());
    }

    /// The resolution that matters: a confident family plus a badge tier
    /// becomes the vocabulary id the rulesets are written against.
    #[test]
    fn a_known_family_with_a_tier_resolves_to_vocabulary_ids() {
        let icon = super::super::icons::IconMatch {
            family: Some("Pierce".into()),
            learned_tier: Some(1),
            score: 0.97,
            runner_up: 0.1,
            state: ReadState::Matched,
        };

        let (_, ids, _, state, candidates) = resolve_cell(&icon, Some(3), &vocab());

        // ('Pierce', 3) is the one rule-relevant collision in the vocabulary —
        // Greater and Gilded share it — so it resolves ambiguous, with both
        // names offered and both ids kept.
        assert_eq!(state, ReadState::Ambiguous);
        assert_eq!(ids.len(), 2);
        assert_eq!(candidates.len(), 2);
    }

    /// A low-confidence template must not resolve to ids, but must still say
    /// what it nearly matched — that list is the page's "low confidence" cell
    /// and the operator's clue about which threshold to move.
    #[test]
    fn a_low_confidence_template_offers_candidates_without_ids() {
        let icon = super::super::icons::IconMatch {
            family: Some("Pierce".into()),
            learned_tier: Some(1),
            score: 0.80,
            runner_up: 0.1,
            state: ReadState::LowConfidence,
        };

        let (family, ids, name, state, candidates) = resolve_cell(&icon, Some(3), &vocab());

        assert_eq!(state, ReadState::LowConfidence);
        assert_eq!(family.as_deref(), Some("Pierce"));
        assert!(ids.is_empty(), "an unconfident read must not reach the verdict engine");
        assert!(name.is_none());
        assert_eq!(candidates.len(), 2);
    }

    /// The pass-2 band is bounded by the row's own slot-0 RECT, not by the
    /// pass-1 text's right edge and not by a second derivation of the cell
    /// column: a short pass-1 read must not crop away the characters pass 2
    /// exists to recover, and the band must stop before the icon that would
    /// otherwise be OCR'd as a glyph (POE-214 A7).
    #[test]
    fn the_pass_two_band_stops_just_short_of_the_rows_first_cell() {
        let g = MercGeometry::default();
        let mut layout = layout_of();
        // A deliberately SHORT name rect: 20 px of a name that runs much wider.
        layout.rows[0].name_rect = [100, 84, 20, 16];
        // The fit's rewrite, in miniature: slot 0 moved 6 px right of where the
        // OCR scale put it. The band has to follow the RECT.
        let derived = layout.column_x0 + (g.cell_offset_x * layout.scale).round() as i32;
        let fitted = derived + 6;
        for cell in &mut layout.rows[0].cells {
            cell[0] += 6;
        }

        let band = name_band(&layout.rows[0], &layout, &g);

        let pad = (4.0 * layout.scale).round() as i32;
        let lead = (NAME_CROP_LEAD * layout.scale).round() as i32;
        assert_eq!(band[0], 100 - pad - lead, "the band pads AND leads left of the name text");
        assert_eq!(
            band[0] + band[2],
            fitted - pad,
            "the band's right edge is the row's own slot-0 rect at {fitted}, not the \
             scale-derived column at {derived}",
        );
        assert_eq!(band[1], 84 - pad, "the band pads above the text as well");
    }

    /// The fallback branch of that rule, which only an override can reach: a
    /// layout whose rows carry NO cells (`maxSlots` 0 in `merc-geometry.json`)
    /// has no fitted rect to stop short of, so the right edge comes from the
    /// scale-derived column instead. Without it the band would be empty and
    /// pass 2 would silently stop re-reading names.
    #[test]
    fn a_row_with_no_cells_falls_back_to_the_scale_derived_column() {
        let g = MercGeometry::default();
        let mut layout = layout_of();
        layout.rows[0].name_rect = [100, 84, 20, 16];
        layout.rows[0].cells.clear();

        let band = name_band(&layout.rows[0], &layout, &g);

        let pad = (4.0 * layout.scale).round() as i32;
        let derived = layout.column_x0 + (g.cell_offset_x * layout.scale).round() as i32;
        assert_eq!(
            band[0] + band[2],
            derived - pad,
            "with no rect to measure against, the band stops at the derived column",
        );
        assert!(band[2] > 0, "and it is still a crop the OCR can be handed");
    }

    /// A mis-clustered detect can produce arbitrarily many "rows"; pass 2 is an
    /// OCR call each, inside ONE tick. The bound is what keeps a bad detect from
    /// blowing the loop's poll budget.
    #[test]
    fn pass_two_reads_at_most_max_rows_rows() {
        let mut g = MercGeometry::default();
        assert_eq!(g.max_rows, 8, "the shipped bound the default panel fits under");

        assert_eq!(pass2_row_budget(20, &g), 8, "a 20-row detect reads 8");
        assert_eq!(pass2_row_budget(6, &g), 6, "the reference panel is read whole");
        g.max_rows = 2;
        assert_eq!(pass2_row_budget(6, &g), 2, "the bound is the override's, not a literal");
    }

    /// Non-Windows has no OCR, so pass 2 cannot produce anything — and the
    /// contract is that it falls back to pass 1 rather than blanking the row.
    /// (On Windows the same branch covers an OCR error and an empty read.)
    #[test]
    fn pass_two_falls_back_to_the_pass_one_text_when_the_re_ocr_fails() {
        let img = flat_screen(900, 300);
        let layout = layout_of();

        let texts = pass2_texts(&img, whole(&img), &layout, &MercGeometry::default());

        assert_eq!(texts, vec!["Ice Shot".to_string(), "Conductivity".to_string()]);
    }

    // -- the sticky header -------------------------------------------------

    use crate::mercenary::MercHeader;

    fn header(name: Option<&str>, class: Option<&str>, level: Option<u32>) -> MercHeader {
        MercHeader {
            name: name.map(str::to_string),
            class: class.map(str::to_string),
            level,
            wager: None,
        }
    }

    /// The blink, in one assertion. A tick that read nothing is not evidence
    /// that the panel stopped showing a name — and on the 2026-08-25 smoke
    /// that tick was every other one.
    #[test]
    fn a_field_that_was_not_read_this_tick_keeps_the_read_before_it() {
        let prev = header(Some("Fennik, of Unshakeable Faith"), Some("Fallen Reverend"), Some(83));

        let merged = merge_header(&prev, &header(None, None, None));

        assert_eq!(merged.name.as_deref(), Some("Fennik, of Unshakeable Faith"));
        assert_eq!(merged.class.as_deref(), Some("Fallen Reverend"));
        assert_eq!(merged.level, Some(83));
    }

    /// The other half of the blink: a SHORTER read of the same field is the
    /// OCR dropping characters, not the panel changing.
    #[test]
    fn a_read_with_less_content_does_not_replace_the_one_on_screen() {
        let prev = header(Some("Fennik, of Unshakeable Faith"), None, None);

        let merged = merge_header(&prev, &header(Some("Fennik, of Unshak"), None, None));

        assert_eq!(merged.name.as_deref(), Some("Fennik, of Unshakeable Faith"));
    }

    /// Sticky is not frozen: a read that recovered characters the last one
    /// missed is the better read and wins.
    #[test]
    fn a_read_with_more_content_replaces_the_one_on_screen() {
        let prev = header(Some("Fennik, of Unshak"), None, None);

        let merged = merge_header(&prev, &header(Some("Fennik, of Unshakeable Faith"), None, None));

        assert_eq!(merged.name.as_deref(), Some("Fennik, of Unshakeable Faith"));
    }

    /// The class-icon glyph, at the merge seam. It is longer than the clean
    /// read by one character, so a rule that only counted length would let the
    /// glyphed read win — which is the `@ Fallen Reverend` the smoke showed.
    #[test]
    fn a_read_carrying_a_leading_glyph_never_beats_a_clean_one() {
        let prev = header(None, Some("Fallen Reverend"), None);

        let merged = merge_header(&prev, &header(None, Some("@ Fallen Reverend"), None));

        assert_eq!(merged.class.as_deref(), Some("Fallen Reverend"));
    }

    /// And the same rule the other way: a clean read replaces a glyphed one
    /// even when it is no longer, because the glyph is what makes it worse.
    #[test]
    fn a_clean_read_replaces_a_glyphed_one_of_the_same_content() {
        let prev = header(None, Some("@ Fallen Reverend"), None);

        let merged = merge_header(&prev, &header(None, Some("Fallen Reverend"), None));

        assert_eq!(merged.class.as_deref(), Some("Fallen Reverend"));
    }

    /// A different read of the SAME length is not evidence of anything, and
    /// swapping on it is the blink itself: two readings of equal length would
    /// alternate every tick forever.
    #[test]
    fn a_read_of_the_same_length_does_not_replace_the_one_on_screen() {
        let prev = header(Some("Fennik, of Unshakeable Faith"), None, None);

        let merged = merge_header(&prev, &header(Some("Fennlk, of Unshakeable Falth"), None, None));

        assert_eq!(merged.name.as_deref(), Some("Fennik, of Unshakeable Faith"));
    }

    /// FIRST wins for the level, the opposite of the text rule: the number does
    /// not change while one window is open, so a re-read that disagrees is OCR
    /// noise — and taking the newer would leave `lvl 83` flicking to `88`.
    #[test]
    fn a_level_that_was_already_read_is_not_replaced_by_a_re_read() {
        let prev = header(None, None, Some(83));

        let merged = merge_header(&prev, &header(None, None, Some(88)));

        assert_eq!(merged.level, Some(83));
    }

    /// The name/class collision arriving by the second route: the parse rejects
    /// a title that IS the class line, but a name KEPT from an earlier tick can
    /// collide with a class read for the first time on this one.
    #[test]
    fn a_merged_name_that_equals_the_merged_class_falls_back_to_the_previous_name() {
        // The earlier tick got a SHORT name — short enough that the class line
        // would win the length rule and land in both fields. Without the clash
        // check the strip would print `Fallen Reverend · Fallen Reverend`,
        // which is what the smoke screenshots showed.
        let prev = header(Some("Fennik"), None, None);

        // This tick read the class for the first time, and read the title as
        // that same class line.
        let merged = merge_header(&prev, &header(Some("Fallen Reverend"), Some("Fallen Reverend"), None));

        assert_eq!(merged.class.as_deref(), Some("Fallen Reverend"));
        assert_eq!(
            merged.name.as_deref(),
            Some("Fennik"),
            "a short true name beats the class standing in for one",
        );
    }

    /// …and with nothing uncollided to fall back to, the name goes back to
    /// unread. "Not read" is a true statement; "the class" is not.
    #[test]
    fn a_name_that_can_only_be_the_class_goes_back_to_unread() {
        let prev = header(Some("@ Fallen Reverend"), None, None);

        let merged = merge_header(&prev, &header(Some("Fallen Reverend"), Some("Fallen Reverend"), None));

        assert_eq!(merged.name, None);
        assert_eq!(merged.class.as_deref(), Some("Fallen Reverend"));
    }

    /// A field nobody has read yet takes whatever the new tick found — the
    /// stickiness is about not LOSING a read, not about refusing new ones.
    #[test]
    fn a_field_read_for_the_first_time_is_taken() {
        let merged = merge_header(&header(Some("Fennik"), None, None), &header(None, Some("Fallen Reverend"), Some(83)));

        assert_eq!(merged.class.as_deref(), Some("Fallen Reverend"));
        assert_eq!(merged.level, Some(83));
    }

    // -- panel identity ----------------------------------------------------

    fn named_row(index: u8, name: &str) -> MercRow {
        MercRow {
            index,
            skill: MercSkillRead {
                raw: name.into(),
                ids: vec![format!("mercenary.skill_{name}")],
                name: Some(name.to_string()),
                score: 0.99,
                state: ReadState::Matched,
            },
            supports: Vec::new(),
        }
    }

    fn panel(rows: &[&str], header_of: MercHeader) -> MercCapture {
        MercCapture {
            captured_at_ms: 1_700_000_000_000,
            live: true,
            scale: 1.0,
            screen: [2560, 1440],
            panel: None,
            header: header_of,
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, name)| named_row(i as u8, name))
                .collect(),
            rows_on_screen: rows.len(),
            rows_read: rows.len(),
            partial: false,
        }
    }

    /// THE REMATCH. The panel stays on screen and the mercenary behind it
    /// changes — and since the liveness pause the loop can take ~20 s to notice
    /// a window that closed, so "a capture exists" is not evidence that it is
    /// the same one. Inheriting here would put a confident, wrong name, class
    /// and level on the surface the player pays from.
    #[test]
    fn a_rematch_to_a_different_mercenary_replaces_the_whole_header() {
        let before = panel(
            &["Ice Shot", "Conductivity", "Frostbolt"],
            header(Some("Fennik, of Unshakeable Faith"), Some("Fallen Reverend"), Some(83)),
        );
        let after = panel(
            &["Cyclone", "Enfeeble", "Flame Dash"],
            header(Some("Cai, the Lout"), Some("Shock Ambusher"), Some(68)),
        );

        let (folded, replaced) = fold_header(Some(&before), &after);

        assert!(replaced, "a disjoint skill list is a different window");
        assert_eq!(folded.name.as_deref(), Some("Cai, the Lout"));
        assert_eq!(folded.class.as_deref(), Some("Shock Ambusher"));
        assert_eq!(folded.level, Some(68));
    }

    /// A rematch that rolled the SAME level is still a rematch: the skill sets
    /// carry it on their own.
    #[test]
    fn a_rematch_at_the_same_level_is_caught_by_the_skill_sets() {
        let before = panel(&["Ice Shot", "Conductivity"], header(Some("Fennik"), None, Some(83)));
        let after = panel(&["Cyclone", "Enfeeble"], header(Some("Cai, the Lout"), None, Some(83)));

        let (folded, replaced) = fold_header(Some(&before), &after);

        assert!(replaced);
        assert_eq!(folded.name.as_deref(), Some("Cai, the Lout"));
    }

    /// …and a rematch whose skills happen to overlap is carried by the LEVEL,
    /// which is the other half of the evidence.
    #[test]
    fn a_rematch_at_a_different_level_is_caught_by_the_level() {
        let before = panel(&["Ice Shot", "Conductivity"], header(Some("Fennik"), None, Some(83)));
        let after = panel(&["Ice Shot", "Conductivity"], header(Some("Cai, the Lout"), None, Some(68)));

        let (folded, replaced) = fold_header(Some(&before), &after);

        assert!(replaced);
        assert_eq!(folded.name.as_deref(), Some("Cai, the Lout"));
        assert_eq!(folded.level, Some(68));
    }

    /// The case the POSITIONAL rule got wrong: OCR dropped the first row, so
    /// every later row shifted up an index. Nothing about the window changed —
    /// and calling this a replacement would throw away the remembered
    /// confirmations, including ambiguous resolutions that only live in the
    /// session.
    #[test]
    fn a_read_that_dropped_a_row_still_merges_into_the_same_window() {
        let before = panel(
            &["Ice Shot", "Conductivity", "Frostbolt"],
            header(Some("Fennik, of Unshakeable Faith"), Some("Fallen Reverend"), Some(83)),
        );
        let after = panel(&["Conductivity", "Frostbolt"], header(None, None, None));

        let (folded, replaced) = fold_header(Some(&before), &after);

        assert!(!replaced, "a shifted row list is the same skills, not a new mercenary");
        assert_eq!(folded.name.as_deref(), Some("Fennik, of Unshakeable Faith"));
        assert_eq!(folded.class.as_deref(), Some("Fallen Reverend"));
        assert_eq!(folded.level, Some(83));
    }

    /// The abstention rule, at its sharpest: a tick that named NOTHING is not
    /// evidence of anything. It must merge — and because it merges, the loop
    /// keeps the confirmations it made on this window.
    #[test]
    fn a_tick_that_read_nothing_keeps_the_window_it_had() {
        let before = panel(
            &["Ice Shot", "Conductivity"],
            header(Some("Fennik, of Unshakeable Faith"), Some("Fallen Reverend"), Some(83)),
        );
        let mut after = panel(&["Ice Shot", "Conductivity"], header(None, None, None));
        for row in &mut after.rows {
            row.skill.name = None;
            row.skill.state = ReadState::Unknown;
        }

        let (folded, replaced) = fold_header(Some(&before), &after);

        assert!(!replaced);
        assert_eq!(folded.name.as_deref(), Some("Fennik, of Unshakeable Faith"));
        assert_eq!(folded.level, Some(83));
    }

    /// One named row on a side is below the evidence bar: a single misread name
    /// would otherwise be enough to declare a different mercenary and wipe the
    /// session's confirmations.
    #[test]
    fn one_named_row_is_not_enough_evidence_to_replace_a_window() {
        let before = panel(&["Ice Shot", "Conductivity"], header(None, None, None));
        let mut after = panel(&["Cyclone", "Enfeeble"], header(None, None, None));
        after.rows[1].skill.name = None;

        assert!(!panel_replaced(&before, &after));
    }

    /// Sharing even ONE skill keeps the window. A rematch rolls a whole new
    /// list, so an overlap of one is a misread, not a new mercenary.
    #[test]
    fn skill_sets_that_share_one_name_are_the_same_window() {
        let before = panel(&["Ice Shot", "Conductivity", "Frostbolt"], header(None, None, None));
        let after = panel(&["Ice Shot", "Enfeeble", "Cyclone"], header(None, None, None));

        assert!(!panel_replaced(&before, &after));
    }

    /// A level nobody read proves nothing — the header line is missed often
    /// enough that holding it against the panel would replace the window on
    /// every tick that lost it.
    #[test]
    fn a_level_this_tick_did_not_read_does_not_replace_the_window() {
        let before = panel(&["Ice Shot", "Conductivity"], header(None, None, Some(83)));
        let after = panel(&["Ice Shot", "Conductivity"], header(None, None, None));

        assert!(!panel_replaced(&before, &after));
    }

    /// A capture that has never been folded against anything keeps its own
    /// header and is not a replacement — nothing was there to replace.
    #[test]
    fn the_first_capture_of_a_window_is_taken_as_read() {
        let first = panel(&["Ice Shot"], header(Some("Cai, the Lout"), None, Some(68)));

        let (folded, replaced) = fold_header(None, &first);

        assert!(!replaced);
        assert_eq!(folded.name.as_deref(), Some("Cai, the Lout"));
    }

    // -- when there is nothing left to read --------------------------------

    fn read_at(state: ReadState) -> MercSupportRead {
        MercSupportRead {
            slot: 0,
            rect: [0, 0, 44, 44],
            family: Some("Pierce".into()),
            tier: Some(3),
            ids: vec!["mercenary.support_1".into()],
            name: Some("Greater Pierce (Tier 3)".into()),
            score: 0.95,
            state,
            candidates: Vec::new(),
        }
    }

    fn capture_of(rows: Vec<MercRow>, header: MercHeader) -> MercCapture {
        MercCapture {
            captured_at_ms: 1_700_000_000_000,
            live: true,
            scale: 1.0,
            screen: [2560, 1440],
            panel: None,
            header,
            rows_on_screen: rows.len(),
            rows_read: rows.len(),
            rows,
            partial: false,
        }
    }

    fn skill(state: ReadState) -> MercSkillRead {
        MercSkillRead {
            raw: "Ice Shot".into(),
            ids: vec!["mercenary.skill_11495".into()],
            name: Some("Ice Shot".into()),
            score: 0.99,
            state,
        }
    }

    #[test]
    fn a_capture_with_every_read_confident_and_a_full_header_is_complete() {
        let capture = capture_of(
            vec![MercRow {
                index: 0,
                skill: skill(ReadState::Matched),
                supports: vec![read_at(ReadState::Matched), read_at(ReadState::Confirmed)],
            }],
            header(Some("Fennik"), Some("Fallen Reverend"), Some(83)),
        );

        assert!(capture_complete(&capture));
    }

    /// The cell the player would hover. While one is unread there IS something
    /// another pass can find, so the loop must not pause.
    #[test]
    fn one_unread_cell_keeps_the_capture_incomplete() {
        let capture = capture_of(
            vec![MercRow {
                index: 0,
                skill: skill(ReadState::Matched),
                supports: vec![read_at(ReadState::Matched), read_at(ReadState::Unknown)],
            }],
            header(Some("Fennik"), Some("Fallen Reverend"), Some(83)),
        );

        assert!(!capture_complete(&capture));
    }

    /// An ambiguous cell has a family and a tier but two possible names — the
    /// hover is what settles it, so it is not read.
    #[test]
    fn an_ambiguous_cell_keeps_the_capture_incomplete() {
        let capture = capture_of(
            vec![MercRow {
                index: 0,
                skill: skill(ReadState::Matched),
                supports: vec![read_at(ReadState::Ambiguous)],
            }],
            header(Some("Fennik"), Some("Fallen Reverend"), Some(83)),
        );

        assert!(!capture_complete(&capture));
    }

    #[test]
    fn an_unread_skill_name_keeps_the_capture_incomplete() {
        let capture = capture_of(
            vec![MercRow {
                index: 0,
                skill: skill(ReadState::LowConfidence),
                supports: vec![read_at(ReadState::Matched)],
            }],
            header(Some("Fennik"), Some("Fallen Reverend"), Some(83)),
        );

        assert!(!capture_complete(&capture));
    }

    /// The header is on the strip, so a missing class is a field another pass
    /// could still fill in.
    #[test]
    fn a_header_field_nobody_read_keeps_the_capture_incomplete() {
        let rows = vec![MercRow {
            index: 0,
            skill: skill(ReadState::Matched),
            supports: vec![read_at(ReadState::Matched)],
        }];

        assert!(!capture_complete(&capture_of(rows.clone(), header(Some("Fennik"), None, Some(83)))));
        assert!(!capture_complete(&capture_of(rows.clone(), header(None, Some("Fallen Reverend"), Some(83)))));
        assert!(!capture_complete(&capture_of(rows, header(Some("Fennik"), Some("Fallen Reverend"), None))));
    }

    /// A name that is not name-SHAPED is not a read name. Completeness is what
    /// opens the trade session (POE-202) and hands GGG the label, so this is
    /// the last gate the corruption of 2026-08-26 had to pass — and it did:
    /// `SUPPORTED SKILLS PENETRATE 100/GlRE` was `Some`, and three `is_some()`
    /// calls had nothing else to say about it.
    #[test]
    fn a_tooltip_shaped_name_keeps_the_capture_incomplete() {
        let rows = vec![MercRow {
            index: 0,
            skill: skill(ReadState::Matched),
            supports: vec![read_at(ReadState::Matched)],
        }];
        let corrupted = header(
            Some(crate::mercenary::geometry::TOOLTIP_NAME),
            Some("Fallen Reverend"),
            Some(83),
        );

        assert!(!capture_complete(&capture_of(rows.clone(), corrupted)));
        assert!(
            capture_complete(&capture_of(
                rows,
                header(Some("Arith, the Quickshot"), Some("Fallen Reverend"), Some(83)),
            )),
            "a real name still completes the capture",
        );
    }

    /// The wager is absent from real OCR dumps and nothing reads it. Requiring
    /// it would mean the module never stops reading.
    #[test]
    fn a_missing_wager_does_not_hold_a_capture_open() {
        let capture = capture_of(
            vec![MercRow {
                index: 0,
                skill: skill(ReadState::Matched),
                supports: vec![read_at(ReadState::Matched)],
            }],
            header(Some("Fennik"), Some("Fallen Reverend"), Some(83)),
        );

        assert_eq!(capture.header.wager, None);
        assert!(capture_complete(&capture));
    }

    /// A skill the panel shows without supports is read, not broken —
    /// `build_capture` only lists cells that are actually on screen.
    #[test]
    fn a_row_the_panel_shows_with_no_supports_is_complete() {
        let capture = capture_of(
            vec![MercRow { index: 0, skill: skill(ReadState::Matched), supports: Vec::new() }],
            header(Some("Fennik"), Some("Fallen Reverend"), Some(83)),
        );

        assert!(capture_complete(&capture));
    }
    // -- positive sameness, for the retained slot --------------------------

    /// The divergence from the live rule, stated as one assertion pair: a tick
    /// that named nothing and read no level ABSTAINS for the live capture (it
    /// keeps the window) and FAILS for the retained slot. Across a retire there
    /// is no cheap next tick to correct a wrong restore — the cells land
    /// `Confirmed`, which nothing re-reads.
    #[test]
    fn a_tick_that_read_nothing_is_not_positive_evidence_of_the_same_panel() {
        let retired = panel(&["Ice Shot", "Conductivity"], header(None, None, Some(83)));
        let mut next = panel(&["Ice Shot", "Conductivity"], header(None, None, None));
        for row in &mut next.rows {
            row.skill.name = None;
            row.skill.state = ReadState::Unknown;
        }

        assert!(!panel_replaced(&retired, &next), "the live rule still abstains");
        assert!(!same_panel_positive(&retired, &next));
    }

    /// THE FLAME DASH CASE. Two different mercenaries can share one skill, and
    /// with no level on either side the live rule abstains its way into calling
    /// them the same window. One name out of three is not evidence.
    #[test]
    fn one_shared_skill_out_of_three_is_not_enough_overlap() {
        let retired = panel(
            &["Flame Dash", "Ice Shot", "Conductivity"],
            header(None, None, None),
        );
        let next = panel(&["Flame Dash", "Cyclone", "Enfeeble"], header(None, None, None));

        assert!(!panel_replaced(&retired, &next), "the live rule abstains on the overlap");
        assert!(!same_panel_positive(&retired, &next));
    }

    /// The veto keeps the SAME evidence bar the replacement rule has: ONE named
    /// row on the new side is a misread, not a disagreement. Without the bar a
    /// single garbled name would outvote a level both sides read and drop a
    /// restore that should stand.
    #[test]
    fn one_garbled_name_on_the_new_side_does_not_veto_an_agreed_level() {
        let retired = panel(&["Ice Shot", "Conductivity"], header(None, None, Some(83)));
        let mut next = panel(&["Ice Shot", "Conductivity"], header(None, None, Some(83)));
        next.rows[0].skill.name = Some("Bal1 Lightning".into());
        next.rows[1].skill.name = None;
        next.rows[1].skill.state = ReadState::Unknown;

        assert!(same_panel_positive(&retired, &next));
    }

    /// The level is only allowed to speak for rows nobody read. Here both reads
    /// named three and share one, which is a present DISAGREEMENT — and two
    /// mercenaries sharing a level is ordinary, so the skills outvote it.
    /// Letting the level win would restore one merc's supports onto another's
    /// rows as `Confirmed`, which no hover can correct.
    #[test]
    fn a_shortfall_in_the_overlap_outvotes_two_levels_that_agree() {
        let retired = panel(
            &["Flame Dash", "Ice Shot", "Conductivity"],
            header(None, None, Some(83)),
        );
        let next = panel(
            &["Flame Dash", "Cyclone", "Enfeeble"],
            header(None, None, Some(83)),
        );

        assert!(!panel_replaced(&retired, &next), "the live rule still abstains");
        assert!(!same_panel_positive(&retired, &next));
    }

    /// …and two out of three carries it: a rematch rolls a whole new list, so
    /// a majority overlap is the same mercenary read twice.
    #[test]
    fn two_shared_skills_out_of_three_carry_the_panel() {
        let retired = panel(
            &["Flame Dash", "Ice Shot", "Conductivity"],
            header(None, None, None),
        );
        let next = panel(&["Flame Dash", "Ice Shot", "Enfeeble"], header(None, None, None));

        assert!(same_panel_positive(&retired, &next));
    }

    /// The other disjunct, and the reason it exists: the first tick after a
    /// re-detect often names no skill at all but does read the header line.
    /// Requiring both facts would drop every restore that matters.
    #[test]
    fn two_levels_that_were_both_read_and_agree_carry_the_panel() {
        let retired = panel(&["Ice Shot", "Conductivity"], header(None, None, Some(83)));
        let mut next = panel(&["Ice Shot", "Conductivity"], header(None, None, Some(83)));
        for row in &mut next.rows {
            row.skill.name = None;
            row.skill.state = ReadState::Unknown;
        }

        assert!(same_panel_positive(&retired, &next));
    }

    /// A level that DISAGREES is a contradiction, and a contradiction outranks
    /// any amount of positive evidence — otherwise a rematch that kept most of
    /// its skill list would restore the previous mercenary's supports.
    #[test]
    fn a_level_that_disagrees_beats_a_matching_skill_list() {
        let retired = panel(
            &["Ice Shot", "Conductivity", "Frostbolt"],
            header(None, None, Some(83)),
        );
        let next = panel(
            &["Ice Shot", "Conductivity", "Frostbolt"],
            header(None, None, Some(68)),
        );

        assert!(!same_panel_positive(&retired, &next));
    }

    /// A rematch at the SAME level with a whole new list must not ride the
    /// level disjunct in: the disjuncts are `or`, but the contradiction gate
    /// runs first.
    #[test]
    fn a_same_level_rematch_with_a_new_skill_list_is_not_the_same_panel() {
        let retired = panel(&["Ice Shot", "Conductivity"], header(None, None, Some(83)));
        let next = panel(&["Cyclone", "Enfeeble"], header(None, None, Some(83)));

        assert!(!same_panel_positive(&retired, &next));
    }

    // -- the read plan (POE-278) --------------------------------------------

    /// The loop's settled panel rect the kept captures below were read at.
    const PANEL: [i32; 4] = [80, 20, 520, 260];

    fn full_header() -> MercHeader {
        header(Some("Arith, the Quickshot"), Some("Fallen Reverend"), Some(83))
    }

    /// The kept capture a round is planned from: one row per entry of `cells`,
    /// matched to the layout row at the same position and named by that row's
    /// own pass-1 text, carrying a Pierce read per state at slots 0.. at the
    /// layout's own rects, under `header_of` and [`PANEL`].
    fn kept_of(layout: &MercLayout, cells: &[&[ReadState]], header_of: MercHeader) -> MercCapture {
        let g = MercGeometry::default();
        let v = vocab();
        let rows = layout
            .rows
            .iter()
            .zip(cells)
            .map(|(row, states)| {
                let read = v.match_skill(&row.text, &g.thresholds);
                MercRow {
                    index: row.index,
                    skill: MercSkillRead {
                        raw: row.text.clone(),
                        ids: read.ids,
                        name: read.name,
                        score: read.score,
                        state: read.state,
                    },
                    supports: states
                        .iter()
                        .enumerate()
                        .map(|(slot, state)| MercSupportRead {
                            slot: slot as u8,
                            rect: row.cells[slot],
                            ..read_at(*state)
                        })
                        .collect(),
                }
            })
            .collect();
        MercCapture { panel: Some(PANEL), ..capture_of(rows, header_of) }
    }

    fn partial_rows(plan: &ReadPlan) -> &[RowPlan] {
        match plan {
            ReadPlan::Partial { rows, .. } => rows,
            other => panic!("expected a partial round, got {other:?}"),
        }
    }

    #[test]
    fn a_first_look_plans_the_full_read() {
        let layout = layout_of();

        assert_eq!(
            plan_read(None, 3, Some(PANEL), &layout, &MercGeometry::default()),
            ReadPlan::Full,
        );
    }

    /// Criterion 3: once fully read, nothing more is read.
    #[test]
    fn a_complete_kept_capture_plans_nothing() {
        let layout = layout_of();
        let kept =
            kept_of(&layout, &[&[ReadState::Matched], &[ReadState::Confirmed]], full_header());
        assert!(capture_complete(&kept), "arrange: the kept capture is complete");

        assert_eq!(
            plan_read(Some(&kept), 2, Some(PANEL), &layout, &MercGeometry::default()),
            ReadPlan::Nothing,
        );
    }

    /// After three rounds an incomplete capture stays incomplete (ADR-025's
    /// accepted cost): the hover, or a Scan now, is the answer.
    #[test]
    fn no_rounds_left_plans_nothing_over_an_incomplete_capture() {
        let layout = layout_of();
        let kept = kept_of(&layout, &[&[ReadState::Unknown], &[]], full_header());

        assert_eq!(
            plan_read(Some(&kept), 0, Some(PANEL), &layout, &MercGeometry::default()),
            ReadPlan::Nothing,
        );
    }

    /// The order that bounds a jittery geometry: with no rounds left, a kept
    /// capture that no longer lines up still buys nothing.
    #[test]
    fn no_rounds_left_plans_nothing_even_when_the_geometry_moved() {
        let layout = layout_of();
        let kept = kept_of(&layout, &[&[ReadState::Unknown], &[]], full_header());
        let moved = [PANEL[0] + 200, PANEL[1], PANEL[2], PANEL[3]];

        assert_eq!(
            plan_read(Some(&kept), 0, Some(moved), &layout, &MercGeometry::default()),
            ReadPlan::Nothing,
        );
    }

    /// Criterion 2 for a cell: its row's walk re-matches it, the fully read row
    /// is walked only for a slot no read has seen — no pass 2 anywhere.
    #[test]
    fn an_unknown_cell_plans_its_rows_cell_walk() {
        let layout = layout_of();
        let kept = kept_of(
            &layout,
            &[&[ReadState::Matched, ReadState::Unknown], &[ReadState::Matched]],
            full_header(),
        );

        let plan = plan_read(Some(&kept), 2, Some(PANEL), &layout, &MercGeometry::default());

        assert_eq!(
            plan,
            ReadPlan::Partial {
                rows: vec![RowPlan::Cells, RowPlan::Unseen],
                cells: 1,
                header: Vec::new(),
            },
        );
    }

    /// An unread skill re-reads its whole row: the key its cells are cached and
    /// confirmed under can change when the skill resolves.
    #[test]
    fn an_unknown_skill_plans_its_whole_row() {
        let layout = layout_of();
        let mut kept =
            kept_of(&layout, &[&[ReadState::Matched], &[ReadState::Matched]], full_header());
        kept.rows[0].skill.state = ReadState::LowConfidence;

        let plan = plan_read(Some(&kept), 2, Some(PANEL), &layout, &MercGeometry::default());

        assert_eq!(partial_rows(&plan), &[RowPlan::Read, RowPlan::Unseen]);
    }

    #[test]
    fn a_layout_row_the_kept_capture_lacks_is_read() {
        let layout = layout_of();
        let kept =
            kept_of(&layout, &[&[ReadState::Matched]], header(Some("Arith"), None, Some(83)));

        let plan = plan_read(Some(&kept), 2, Some(PANEL), &layout, &MercGeometry::default());

        assert_eq!(partial_rows(&plan), &[RowPlan::Unseen, RowPlan::Read]);
    }

    /// A header field is unknown too, and folding it costs no OCR — but it is
    /// what keeps the capture incomplete, so the round is for it alone.
    #[test]
    fn an_unresolved_header_field_is_a_rounds_only_work() {
        let layout = layout_of();
        let kept = kept_of(
            &layout,
            &[&[ReadState::Matched], &[ReadState::Matched]],
            header(Some("Arith, the Quickshot"), None, None),
        );

        let plan = plan_read(Some(&kept), 2, Some(PANEL), &layout, &MercGeometry::default());

        assert_eq!(
            plan,
            ReadPlan::Partial {
                rows: vec![RowPlan::Unseen, RowPlan::Unseen],
                cells: 0,
                header: vec![HeaderField::Class, HeaderField::Level],
            },
        );
    }

    #[test]
    fn a_kept_capture_that_no_longer_lines_up_plans_the_full_read() {
        let layout = layout_of();
        let kept = kept_of(&layout, &[&[ReadState::Unknown], &[]], full_header());
        let moved = [PANEL[0] + 200, PANEL[1], PANEL[2], PANEL[3]];

        assert_eq!(
            plan_read(Some(&kept), 2, Some(moved), &layout, &MercGeometry::default()),
            ReadPlan::Full,
        );
    }

    // -- when the kept capture lines up -------------------------------------

    fn shifted(kept: &mut MercCapture, dx: i32, dy: i32) {
        for cell in kept.rows.iter_mut().flat_map(|row| row.supports.iter_mut()) {
            cell.rect[0] += dx;
            cell.rect[1] += dy;
        }
    }

    fn lined_up_kept(layout: &MercLayout) -> MercCapture {
        kept_of(
            layout,
            &[&[ReadState::Matched, ReadState::Unknown], &[ReadState::Matched]],
            full_header(),
        )
    }

    #[test]
    fn a_kept_capture_at_this_layouts_own_rects_lines_up() {
        let layout = layout_of();

        assert!(lines_up(&lined_up_kept(&layout), Some(PANEL), &layout, &MercGeometry::default()));
    }

    /// The per-tick jitter of the OCR-seeded row centres and the frame fit is
    /// inside the half-cell band, edge included.
    #[test]
    fn a_cell_moved_by_the_whole_half_cell_band_still_lines_up() {
        let g = MercGeometry::default();
        let layout = layout_of();
        let band = column_tolerance(&g, layout.scale);
        let mut kept = lined_up_kept(&layout);
        shifted(&mut kept, band, -band);

        assert!(lines_up(&kept, Some(PANEL), &layout, &g));
    }

    #[test]
    fn a_cell_moved_past_the_band_across_does_not_line_up() {
        let g = MercGeometry::default();
        let layout = layout_of();
        let mut kept = lined_up_kept(&layout);
        shifted(&mut kept, column_tolerance(&g, layout.scale) + 1, 0);

        assert!(!lines_up(&kept, Some(PANEL), &layout, &g));
    }

    #[test]
    fn a_cell_moved_past_the_band_down_does_not_line_up() {
        let g = MercGeometry::default();
        let layout = layout_of();
        let mut kept = lined_up_kept(&layout);
        shifted(&mut kept, 0, column_tolerance(&g, layout.scale) + 1);

        assert!(!lines_up(&kept, Some(PANEL), &layout, &g));
    }

    /// An adopted cell size re-registers every crop: the kept cells are not
    /// this layout's cells any more, however close their origins.
    #[test]
    fn a_cell_of_another_size_does_not_line_up() {
        let layout = layout_of();
        let mut kept = lined_up_kept(&layout);
        kept.rows[0].supports[0].rect[2] += 1;

        assert!(!lines_up(&kept, Some(PANEL), &layout, &MercGeometry::default()));
    }

    #[test]
    fn a_kept_row_the_layout_no_longer_has_does_not_line_up() {
        let layout = layout_of();
        let mut kept = lined_up_kept(&layout);
        kept.rows[1].index = 7;

        assert!(!lines_up(&kept, Some(PANEL), &layout, &MercGeometry::default()));
    }

    #[test]
    fn a_kept_cell_past_the_layout_rows_last_slot_does_not_line_up() {
        let layout = layout_of();
        let mut kept = lined_up_kept(&layout);
        kept.rows[0].supports[1].slot = layout.rows[0].cells.len() as u8;

        assert!(!lines_up(&kept, Some(PANEL), &layout, &MercGeometry::default()));
    }

    #[test]
    fn a_moved_panel_does_not_line_up() {
        let layout = layout_of();
        let moved = [PANEL[0], PANEL[1] + 1, PANEL[2], PANEL[3]];

        assert!(!lines_up(&lined_up_kept(&layout), Some(moved), &layout, &MercGeometry::default()));
    }

    // -- the header a round that read nothing new publishes -----------------

    /// `merge_header` alone would take the longer read; a resolved name is not
    /// re-litigated by a round that re-reads nothing.
    #[test]
    fn a_resolved_name_stays_verbatim_over_a_longer_pass_one_read() {
        let kept = header(Some("Fennik, of Unshak"), Some("Fallen Reverend"), Some(83));
        let pass1 = header(Some("Fennik, of Unshakeable Faith"), None, None);

        let folded = fold_unresolved_header(&kept, &pass1);

        assert_eq!(folded.name.as_deref(), Some("Fennik, of Unshak"));
    }

    #[test]
    fn a_resolved_class_stays_verbatim_over_a_longer_pass_one_read() {
        let kept = header(Some("Arith"), Some("Fallen Rev"), Some(83));
        let pass1 = header(None, Some("Fallen Reverend"), None);

        let folded = fold_unresolved_header(&kept, &pass1);

        assert_eq!(folded.class.as_deref(), Some("Fallen Rev"));
    }

    #[test]
    fn an_unresolved_class_is_folded_from_pass_one() {
        let kept = header(Some("Arith"), None, Some(83));

        let folded = fold_unresolved_header(&kept, &header(None, Some("Fallen Reverend"), None));

        assert_eq!(folded.class.as_deref(), Some("Fallen Reverend"));
    }

    #[test]
    fn an_unresolved_level_is_folded_from_pass_one() {
        let kept = header(Some("Arith"), Some("Fallen Reverend"), None);

        let folded = fold_unresolved_header(&kept, &header(None, None, Some(83)));

        assert_eq!(folded.level, Some(83));
    }

    /// A name that is `Some` but not name-shaped is unresolved, and folds.
    #[test]
    fn an_unshaped_name_is_folded_from_pass_one() {
        let kept = header(Some("Lvl 83"), Some("Fallen Reverend"), Some(83));

        let pass1 = header(Some("Arith, the Quickshot"), None, None);

        let folded = fold_unresolved_header(&kept, &pass1);

        assert_eq!(folded.name.as_deref(), Some("Arith, the Quickshot"));
    }

    // -- a planned build ----------------------------------------------------

    /// Row 0's slots 0 and 1 painted: occupied, and a FRESH match against the
    /// empty store reads them `Unknown` with no family — so a `Pierce` cell in
    /// the result can only have been copied.
    fn two_painted_cells(layout: &MercLayout) -> DynamicImage {
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        fill_noise(&mut raw, layout.rows[0].cells[0]);
        fill_noise(&mut raw, layout.rows[0].cells[1]);
        DynamicImage::ImageRgba8(raw)
    }

    fn planned_read(
        img: &DynamicImage,
        layout: &MercLayout,
        texts: &[String],
        rows: &[RowPlan],
        kept: &MercCapture,
    ) -> ReadResult {
        build_planned(
            img,
            whole(img),
            layout,
            texts,
            0,
            &MercGeometry::default(),
            &vocab(),
            &TemplateStore::new(),
            Some((rows, kept)),
        )
    }

    #[test]
    fn a_confident_kept_cell_is_copied_rather_than_matched() {
        let layout = layout_of();
        let img = two_painted_cells(&layout);
        let kept = kept_of(
            &layout,
            &[&[ReadState::Matched, ReadState::Unknown], &[]],
            full_header(),
        );

        let out = planned_read(&img, &layout, &[], &[RowPlan::Cells, RowPlan::Unseen], &kept);

        let cells = &out.capture.rows[0].supports;
        assert_eq!(
            cells[1].state,
            ReadState::Unknown,
            "arrange: a fresh match of a painted cell against the empty store reads Unknown",
        );
        assert_eq!(cells[0].state, ReadState::Matched, "the confident kept cell was re-matched");
        assert_eq!(cells[0].family.as_deref(), Some("Pierce"));
    }

    #[test]
    fn a_copied_cell_carries_its_cached_crop_instead_of_cutting_one() {
        let layout = layout_of();
        let img = two_painted_cells(&layout);
        let kept = kept_of(
            &layout,
            &[&[ReadState::Matched, ReadState::Unknown], &[]],
            full_header(),
        );
        let key = row_key(&kept.rows[0].skill);

        let out = planned_read(&img, &layout, &[], &[RowPlan::Cells, RowPlan::Unseen], &kept);

        assert!(out.carried.contains(&(key.clone(), 0)), "carried: {:?}", out.carried);
        assert!(!out.sigs.contains_key(&(key.clone(), 0)), "a copied cell cut a crop");
        assert!(out.sigs.contains_key(&(key, 1)), "the matched cell cut its crop");
    }

    /// The kept skill verbatim: pass 2 did not run for the row, and whatever
    /// text the caller holds for it is not a read of it.
    #[test]
    fn a_row_whose_cells_are_walked_keeps_its_kept_skill() {
        let layout = layout_of();
        let img = flat_screen(900, 300);
        let kept = kept_of(&layout, &[&[ReadState::Unknown], &[]], full_header());
        let texts = ["Frostbolt".to_string(), "Conductivity".to_string()];

        let out = planned_read(&img, &layout, &texts, &[RowPlan::Cells, RowPlan::Unseen], &kept);

        assert_eq!(out.capture.rows[0].skill.name.as_deref(), Some("Ice Shot"));
    }

    /// The key-change rule: a row re-read for its skill copies none of its
    /// kept cells, confident or not.
    #[test]
    fn a_re_read_row_copies_nothing_from_its_kept_row() {
        let layout = layout_of();
        let img = two_painted_cells(&layout);
        let mut kept = kept_of(
            &layout,
            &[&[ReadState::Matched, ReadState::Matched], &[]],
            full_header(),
        );
        kept.rows[0].skill.state = ReadState::LowConfidence;

        let out = planned_read(&img, &layout, &[], &[RowPlan::Read, RowPlan::Unseen], &kept);

        assert_eq!(out.capture.rows[0].supports[0].state, ReadState::Unknown);
        assert_eq!(out.capture.rows[0].supports[0].family, None);
        assert!(out.carried.is_empty(), "carried: {:?}", out.carried);
    }

    /// Geometry is the layout's: the hover tick hit-tests these rects.
    #[test]
    fn a_copied_rows_cells_take_this_layouts_rects() {
        let layout = layout_of();
        let img = flat_screen(900, 300);
        let mut kept =
            kept_of(&layout, &[&[ReadState::Unknown], &[ReadState::Matched]], full_header());
        shifted(&mut kept, 3, -2);

        let out = planned_read(&img, &layout, &[], &[RowPlan::Cells, RowPlan::Unseen], &kept);

        assert_eq!(out.capture.rows[1].supports[0].rect, layout.rows[1].cells[0]);
    }

    /// The collision rule reaches a carried crop too: two rows claiming one
    /// key would hand one row's cached art to the other's confirm.
    #[test]
    fn copied_rows_that_share_a_row_key_carry_no_crop() {
        let layout = layout_of();
        let img = flat_screen(900, 300);
        let mut kept =
            kept_of(&layout, &[&[ReadState::Matched], &[ReadState::Matched]], full_header());
        kept.rows[1].skill = kept.rows[0].skill.clone();

        let out = planned_read(&img, &layout, &[], &[RowPlan::Unseen, RowPlan::Unseen], &kept);

        assert_eq!(out.capture.rows[1].supports.len(), 1, "arrange: both rows kept their cell");
        assert!(out.carried.is_empty(), "carried: {:?}", out.carried);
    }

    /// An empty slot stops the walk, not the row: slot 1 reading dark on this
    /// frame must not take the confident slot 2 (or slot 1's kept read) away.
    #[test]
    fn a_slot_that_reads_empty_keeps_the_kept_cells_after_it() {
        let layout = layout_of();
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        fill_noise(&mut raw, layout.rows[0].cells[0]);
        fill_noise(&mut raw, layout.rows[0].cells[2]);
        let img = DynamicImage::ImageRgba8(raw);
        let kept = kept_of(
            &layout,
            &[&[ReadState::Matched, ReadState::Unknown, ReadState::Matched], &[]],
            full_header(),
        );

        let out = planned_read(&img, &layout, &[], &[RowPlan::Cells, RowPlan::Unseen], &kept);

        let slots: Vec<(u8, ReadState)> = out.capture.rows[0]
            .supports
            .iter()
            .map(|cell| (cell.slot, cell.state))
            .collect();
        assert_eq!(
            slots,
            vec![(0, ReadState::Matched), (1, ReadState::Unknown), (2, ReadState::Matched)],
        );
    }

    /// Nothing confident at or past the dark slot vouches for the kept slot-1
    /// read, so the empty frame stands — a tooltip's phantom does not ride on.
    #[test]
    fn an_unverified_kept_cell_on_a_slot_that_reads_empty_is_dropped() {
        let layout = layout_of();
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        fill_noise(&mut raw, layout.rows[0].cells[0]);
        let img = DynamicImage::ImageRgba8(raw);
        let kept =
            kept_of(&layout, &[&[ReadState::Matched, ReadState::Unknown], &[]], full_header());

        let out = planned_read(&img, &layout, &[], &[RowPlan::Cells, RowPlan::Unseen], &kept);

        let slots: Vec<u8> = out.capture.rows[0].supports.iter().map(|cell| cell.slot).collect();
        assert_eq!(slots, vec![0]);
    }

    /// A row taken from the kept read was on screen when it was read; a dark
    /// skill icon on this frame does not trim it away.
    #[test]
    fn a_kept_row_behind_a_dark_icon_is_not_trimmed() {
        let layout = layout_of();
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        fill_noise(&mut raw, layout.rows[0].skill_icon);
        let img = DynamicImage::ImageRgba8(raw);
        let kept = kept_of(&layout, &[&[ReadState::Unknown], &[ReadState::Matched]], full_header());

        let out = planned_read(&img, &layout, &[], &[RowPlan::Cells, RowPlan::Unseen], &kept);

        assert_eq!(out.capture.rows.len(), 2);
    }

    /// A slot past a row's last kept cell has never been seen — round 1 can read
    /// it empty while its art is still drawing — so a partial round matches it
    /// when it turns up, and still copies the confident cell before it.
    #[test]
    fn a_slot_past_the_last_kept_cell_that_reads_occupied_is_matched() {
        let layout = layout_of();
        let img = two_painted_cells(&layout);
        let kept = kept_of(&layout, &[&[ReadState::Matched], &[]], full_header());
        let key = row_key(&kept.rows[0].skill);

        let out = planned_read(&img, &layout, &[], &[RowPlan::Unseen, RowPlan::Unseen], &kept);

        let cells = &out.capture.rows[0].supports;
        assert_eq!(cells.len(), 2, "the newly occupied slot 1 was not walked");
        assert_eq!(cells[0].family.as_deref(), Some("Pierce"), "slot 0 was not copied");
        assert!(out.sigs.contains_key(&(key, 1)), "slot 1 was not matched (no crop cut)");
    }

    #[test]
    fn only_a_read_row_re_ocrs_its_name() {
        assert!(RowPlan::Read.reads_name());
        assert!(!RowPlan::Cells.reads_name());
        assert!(!RowPlan::Unseen.reads_name());
    }

    // -- the round that reads nothing ---------------------------------------

    #[test]
    fn a_round_that_reads_nothing_carries_every_kept_row_at_this_layouts_rects() {
        let layout = layout_of();
        let img = flat_screen(900, 300);
        let mut kept =
            kept_of(&layout, &[&[ReadState::Matched], &[ReadState::Confirmed]], full_header());
        shifted(&mut kept, 3, -2);

        let out = carry_capture(&img, whole(&img), &layout, &kept, 0, &MercGeometry::default());

        let states: Vec<ReadState> = out
            .capture
            .rows
            .iter()
            .flat_map(|row| row.supports.iter().map(|cell| cell.state))
            .collect();
        assert_eq!(states, vec![ReadState::Matched, ReadState::Confirmed]);
        assert_eq!(out.capture.rows[0].supports[0].rect, layout.rows[0].cells[0]);
        assert_eq!(out.capture.rows[1].supports[0].rect, layout.rows[1].cells[0]);
    }

    #[test]
    fn a_round_that_reads_nothing_carries_every_kept_cells_crop() {
        let layout = layout_of();
        let img = flat_screen(900, 300);
        let kept =
            kept_of(&layout, &[&[ReadState::Matched], &[ReadState::Confirmed]], full_header());
        let keys: HashSet<(String, u8)> =
            kept.rows.iter().map(|row| (row_key(&row.skill), 0)).collect();

        let out = carry_capture(&img, whole(&img), &layout, &kept, 0, &MercGeometry::default());

        assert_eq!(out.carried, keys);
        assert!(out.sigs.is_empty());
    }

    /// The sensor is pixels, not a read, so it counts THIS frame's icons.
    #[test]
    fn a_round_that_reads_nothing_still_counts_this_frames_icons() {
        let layout = layout_of();
        let mut raw = RgbaImage::from_pixel(900, 300, Rgba([12, 12, 14, 255]));
        fill_noise(&mut raw, layout.rows[0].skill_icon);
        let img = DynamicImage::ImageRgba8(raw);
        let mut kept = kept_of(&layout, &[&[ReadState::Matched], &[]], full_header());
        kept.rows_on_screen = 5;

        let out = carry_capture(&img, whole(&img), &layout, &kept, 0, &MercGeometry::default());

        assert_eq!(out.rows_on_screen, 1);
    }
}
