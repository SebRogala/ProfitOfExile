//! Turning a detected layout into a capture (POE-165 D2 pass 2, D4).
//!
//! [`build_capture`] is the ONE place a `MercLayout` plus a screen image
//! becomes a [`MercCapture`]: the capture loop and the debug command both go
//! through it, so a dump can never disagree with what the page was shown.
//!
//! It is pure — image in, capture out, no OCR call, no clock, no lock — which
//! is what makes the cell walk (occupancy → signature → badge → vocabulary)
//! testable on this Linux host. The one thing it cannot do is the pass-2 re-OCR
//! of a row's name band, because that IS an OCR call; the caller does that and
//! hands the text in ([`pass2_texts`]).

use std::collections::HashMap;

use image::{DynamicImage, GenericImageView, RgbaImage};
use serde::Serialize;

use super::geometry::{inner_rect, occupied, stddev, MercLayout};
use super::icons::{normalize_cell, read_tier, CellSig, TemplateStore};
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
    /// The PRE-HOVER crop of every occupied cell, keyed `(row, slot)`.
    ///
    /// D5's rule in data form: the template a hover-confirm learns comes from
    /// the crop taken at DETECT time, never from a fresh grab — by the time the
    /// tooltip is up, the cell underneath may be drawn highlighted, and the
    /// store would learn the highlight.
    pub sigs: HashMap<(u8, u8), (CellSig, Option<RgbaImage>)>,
}

/// Read a detected layout into a capture.
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
    layout: &MercLayout,
    row_texts: &[String],
    captured_at_ms: u64,
    g: &MercGeometry,
    vocab: &MercVocab,
    store: &TemplateStore,
) -> ReadResult {
    let (iw, ih) = img.dimensions();
    let mut cells_debug = Vec::new();
    let mut sigs = HashMap::new();
    let mut rows = Vec::with_capacity(layout.rows.len());

    for (i, row) in layout.rows.iter().enumerate() {
        let raw = row_texts.get(i).cloned().unwrap_or_else(|| row.text.clone());
        let name_read = vocab.match_skill(&raw, &g.thresholds);
        let skill = MercSkillRead {
            raw,
            ids: name_read.ids,
            name: name_read.name,
            score: name_read.score,
            state: name_read.state,
        };

        let mut supports = Vec::new();
        for (slot, rect) in row.cells.iter().enumerate() {
            let sd = stddev(img, inner_rect(*rect, g));
            let is_occupied = occupied(img, *rect, g);
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
                break;
            }

            let sig = normalize_cell(img, *rect, g);
            let tier = read_tier(img, *rect, g);
            let icon = match &sig {
                Some(s) => store.match_family(s, &g.thresholds),
                // Unreachable while `occupied` and `normalize_cell` share the
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

            if let Some(s) = sig {
                sigs.insert((row.index, slot as u8), (s, crop_rgba(img, *rect, g)));
            }
        }

        rows.push(MercRow {
            index: row.index,
            skill,
            supports,
        });
    }

    ReadResult {
        capture: MercCapture {
            captured_at_ms,
            live: true,
            scale: layout.scale,
            screen: [iw, ih],
            header: layout.header.clone(),
            rows,
        },
        cells: cells_debug,
        sigs,
    }
}

/// `(family, tier)` → the vocabulary link(s) it names (D4's resolution table).
///
/// Both halves must be confident: a matched template with NO tier names 154
/// possible links, so it is `Unknown` with the family recorded, never a guess
/// at tier 1.
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

/// Pass 2 (D2): re-OCR each row's name band on its own.
///
/// The whole-screen pass 1 reads the name at native size; a 44 px-tall band
/// goes through `preprocess_for_ocr`, which upscales it 2× and stretches its
/// contrast — measurably better on small text (POE-116).
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
pub fn pass2_texts(img: &DynamicImage, layout: &MercLayout, g: &MercGeometry) -> Vec<String> {
    let (iw, ih) = img.dimensions();
    let budget = pass2_row_budget(layout.rows.len(), g);
    layout
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            if i >= budget {
                return row.text.clone();
            }
            let [x, y, w, h] = name_band(row.name_rect, layout, g);
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
fn name_band(name_rect: [i32; 4], layout: &MercLayout, g: &MercGeometry) -> [i32; 4] {
    let pad = (4.0 * layout.scale).round() as i32;
    let x = name_rect[0] - pad;
    let cells_x0 = layout.column_x0 + (g.cell_offset_x * layout.scale).round() as i32;
    let right = (cells_x0 - pad).max(x + 1);
    [x, name_rect[1] - pad, right - x, name_rect[3] + 2 * pad]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mercenary::geometry::{detect, OcrLineBox};
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

    fn layout_of() -> MercLayout {
        detect(&reference_lines(), &MercGeometry::default(), &vocab())
            .expect("the reference lines detect as a panel")
    }

    /// The skill column is what the capture is FOR: the row's text has to reach
    /// the vocabulary and come back as an identified skill, per row.
    #[test]
    fn each_row_carries_its_matched_skill_read() {
        let img = flat_screen(900, 300);
        let layout = layout_of();
        let g = MercGeometry::default();

        let out = build_capture(&img, &layout, &[], 1_700_000_000_000, &g, &vocab(), &TemplateStore::new());

        assert_eq!(out.capture.rows.len(), 2);
        assert_eq!(out.capture.rows[0].skill.name.as_deref(), Some("Ice Shot"));
        assert_eq!(out.capture.rows[0].skill.state, ReadState::Matched);
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

        let out = build_capture(&img, &layout, &[], 0, &g, &vocab(), &TemplateStore::new());

        assert!(out.capture.rows.iter().all(|r| r.supports.is_empty()));
        assert!(out.sigs.is_empty(), "no signature is cached for an empty slot");
        assert!(
            out.cells.iter().all(|c| !c.occupied),
            "the debug cells still record the rejected slots and why",
        );
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

        let out = build_capture(&img, &layout, &[], 0, &g, &vocab(), &TemplateStore::new());

        let supports = &out.capture.rows[0].supports;
        assert_eq!(supports.len(), 1, "scanning stops at the empty second slot");
        assert_eq!(supports[0].slot, 0);
        assert_eq!(supports[0].state, ReadState::Unknown);
        assert!(supports[0].ids.is_empty());
        assert!(
            out.sigs.contains_key(&(0, 0)),
            "the pre-hover crop must be cached so a later confirm can learn it",
        );
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

    /// The pass-2 band is bounded by the CELL column, not by the pass-1 text's
    /// own right edge — a short pass-1 read must not crop away the characters
    /// pass 2 exists to recover.
    #[test]
    fn the_pass_two_band_reaches_the_support_column() {
        let g = MercGeometry::default();
        let layout = layout_of();
        // A deliberately SHORT name rect: 20 px of a name that runs much wider.
        let band = name_band([100, 84, 20, 16], &layout, &g);

        let cells_x0 = layout.column_x0 + (g.cell_offset_x * layout.scale).round() as i32;
        assert!(
            band[0] < 100 && band[0] + band[2] >= cells_x0 - 8,
            "band {band:?} must span from before the name to the cell column at {cells_x0}",
        );
        assert!(band[1] < 84, "the band pads above the text as well");
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

        let texts = pass2_texts(&img, &layout, &MercGeometry::default());

        assert_eq!(texts, vec!["Ice Shot".to_string(), "Conductivity".to_string()]);
    }
}
