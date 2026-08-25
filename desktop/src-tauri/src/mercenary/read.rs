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
/// This is what lets the DETECT stop: at complete there is no better read
/// available from another full-screen pass, so the 2 s re-detect is pure heat
/// over the game and only the liveness check (is the window still there?) has
/// anything left to find out. The hover tick keeps running — a tooltip can
/// still contradict a confident wrong match, which no re-detect ever would.
pub fn capture_complete(capture: &MercCapture) -> bool {
    header_complete(&capture.header)
        && capture.rows.iter().all(|row| {
            confident(row.skill.state) && row.supports.iter().all(|cell| confident(cell.state))
        })
}

/// The header fields the strip prints, all read. See [`capture_complete`].
pub fn header_complete(header: &super::MercHeader) -> bool {
    header.name.is_some() && header.class.is_some() && header.level.is_some()
}

/// The two states a hover cannot improve. The same pair the verdict engine
/// treats as confident (`verdict.ts`'s `CONFIDENT_STATES`).
fn confident(state: ReadState) -> bool {
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
            header: header_of,
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, name)| named_row(i as u8, name))
                .collect(),
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
            header,
            rows,
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
}
