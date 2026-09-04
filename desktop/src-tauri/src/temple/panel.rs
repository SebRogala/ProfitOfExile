//! The side panel and the plate name strips (POE-169).
//!
//! Two halves, deliberately split:
//!
//! - **text parsing** — [`parse_architect_block`], [`parse_incursions_remaining`],
//!   [`read_panel`]. Pure functions over strings, so every wording and every
//!   OCR confusion is a unit test that runs in the Linux container.
//! - **the OCR call** — [`TextRecognizer`], one method wide. [`SystemOcr`] is
//!   the production implementation and is the only thing here that touches an
//!   image; a test double replaces it wholesale.
//!
//! Both halves speak [`TextLine`] since POE-243: a line is its text plus, when
//! the caller has one, the box the engine read it at in CAPTURE px. That is
//! what lets the parsing stay pure — a `&str` is a `TextLine` with no box —
//! while the production read groups by geometry and publishes each block's
//! rect.
//!
//! # Why the file is not called `ocr.rs`
//!
//! The crate already has a root `ocr.rs` that owns the Windows.Media.Ocr
//! binding. A second `ocr` in scope would make every `use` here a coin toss
//! over which one is meant, and this module is not an OCR engine — it is the
//! *panel*, of which OCR is one input. The door markers, the panel's other
//! half, live in [`super::markers`] because they are pixels rather than text.
//!
//! # What the panel prints (MEASURED, 8 boards)
//!
//! - the current room's name, as the panel title;
//! - both architects, each as `<name>, <title> (Kill to change|upgrade to
//!   <ROOM>)`, wrapped over two or three OCR lines. The title is **not**
//!   always `Architect of <X>` — `Xipocado, Royal Architect` appears on two
//!   of the eight boards;
//! - a diamond with one door marker per lattice neighbour ([`super::markers`]).
//!
//! `N Incursions Remaining` is printed at the bottom of the *layout* panel, not
//! the side panel, which is why [`read_panel`] takes a line VECTOR rather than
//! one crop: it needs both regions' text in one list. POE-171's
//! [`super::run::panel_text`] concatenates the two bounded crops — side panel
//! first, budget line second — and hands the result here. Since POE-243 the
//! concatenation order is no longer load-bearing: the lines carry their boxes
//! in CAPTURE px, so [`reading_order`] puts them back in the order the game
//! drew them whichever crop they came out of. It is deliberately not "every line on screen": the parsing
//! below only needs the panel's own lines in reading order, and a whole-frame
//! OCR would both cost a 2× buffer of the monitor and feed the title rule every
//! plate name on the board.

// POE-171 is that caller: `temple::run` and `temple::slice` reach this module
// on every read, so the file-level `#![allow(dead_code)]` is gone. What is
// still uncalled carries its own attribute, which is now the inventory of what
// only the tests reach rather than a blanket over the whole file.

use image::DynamicImage;
use strsim::jaro_winkler;

use super::lattice::{Lattice, Slot};
use super::rooms::{self, Match, OfferKind, RoomIdentity};
use crate::mercenary::geometry::OcrLineBox;

/// Jaro-Winkler score a single **word** must reach to count as one of the
/// panel's fixed keywords (`kill`, `to`, `change`, `upgrade`, `incursions`,
/// `remaining`).
///
/// Looser than [`rooms::MATCH`] on purpose: these are 2-to-10-letter words from
/// a small vocabulary with no near neighbours (`change` vs `upgrade` scores
/// 0.44), and none of them is read on its own: each has to appear in the right
/// position of a multi-word clause (`kill to <verb> to`, `incursions
/// remaining`) before it counts. The risk here is the opposite one — a missed
/// keyword drops a whole architect offer.
///
/// `architect` is deliberately **not** on that list: it is the word that
/// decides whether a line is an architect block at all, and it has a near
/// neighbour in the room vocabulary. It gets [`ARCHITECT_KEYWORD`] instead.
pub const KEYWORD: f64 = 0.82;

/// Jaro-Winkler score the block-opening word `architect` must reach.
///
/// **MEASURED collision.** `artefacts` — of `Museum of Artefacts`, a tier-3
/// room the board can print anywhere — scores **0.8222** against `architect`
/// and so cleared [`KEYWORD`]. That opened an architect block on the plate
/// name, and because the line carries no closing bracket the block never
/// closed and swallowed the panel title for the rest of the read.
///
/// The word's own OCR slips are far closer than that: `Archrtect` 0.9556,
/// `Architeci` 0.9556, `Arcnitect` 0.9481. 0.90 sits between the two
/// populations with 0.048 of margin on the tight side, so the tolerance
/// [`KEYWORD`] exists for is kept.
pub const ARCHITECT_KEYWORD: f64 = 0.90;

fn word_is(word: &str, keyword: &str) -> bool {
    word == keyword || jaro_winkler(word, keyword) >= KEYWORD
}

fn word_is_architect(word: &str) -> bool {
    word == "architect" || jaro_winkler(word, "architect") >= ARCHITECT_KEYWORD
}

/// A word of the source text, with its normalised comparison key and the
/// original slice it came from.
struct Word<'a> {
    raw: &'a str,
    key: String,
}

/// Split on whitespace, keeping each word's original slice.
///
/// A token that starts with digits and continues into a **word** is split in
/// two: Windows OCR merges `9 Incursions` into `9Incursions` often enough that
/// treating it as one token would lose the count. The tail has to be at least
/// three characters for that to fire — `1O` is a two-character *number* with a
/// mis-read zero, and splitting it would report 1 remaining incursion instead
/// of 10.
fn words(text: &str) -> Vec<Word<'_>> {
    const MERGED_WORD_MIN: usize = 3;
    let mut out = Vec::new();
    for raw in text.split_whitespace() {
        let digits = raw
            .char_indices()
            .take_while(|(_, c)| c.is_ascii_digit())
            .count();
        if digits > 0 && raw.len() - digits >= MERGED_WORD_MIN {
            let (head, tail) = raw.split_at(digits);
            out.push(Word {
                raw: head,
                key: rooms::normalise(head),
            });
            out.push(Word {
                raw: tail,
                key: rooms::normalise(tail),
            });
        } else {
            out.push(Word {
                raw,
                key: rooms::normalise(raw),
            });
        }
    }
    out.retain(|w| !w.key.is_empty());
    out
}

// ----------------------------------------------------------- architects --

/// One architect's offer, as printed.
///
/// `printed_target` is kept verbatim so a debug dump can show what the game
/// actually said; **it is not what to show the user** — that is
/// [`rooms::resolve_offer`]'s job, because with Contested Development the
/// printed name is a lie about the tier.
#[derive(Debug, Clone, PartialEq)]
pub struct ArchitectOffer {
    /// Text before the first comma: `Ticaba`, `Juatalotli`, `Xipocado`.
    pub architect_name: String,
    /// Resident (`change`) or non-resident (`upgrade`).
    pub kind: OfferKind,
    /// The room name the panel printed, verbatim.
    pub printed_target: String,
    /// That name matched against the closed vocabulary.
    pub target: Match,
    /// `[x, y, w, h]` of the block on screen, in CAPTURE px — the union of the
    /// boxes of the OCR lines the block was built from (POE-243).
    ///
    /// `None` when the lines carried no boxes, which is every text-only caller
    /// (the tests, and [`parse_architect_block`], which is handed a string).
    /// Nothing in the advisor reads it: it exists so a surface can point at the
    /// block the advice is about without guessing where the panel drew it.
    ///
    /// Deliberately OUTSIDE [`super::slice::panel_signature`]: a box that
    /// wobbles by a pixel between two reads of the same panel is not a changed
    /// panel, and hashing it would re-read the board for a rounding difference.
    pub rect: Option<[i32; 4]>,
}

/// How many architect blocks the incursion side panel prints.
///
/// **Always two**, MEASURED on every one of the eight reference boards and on
/// the live captures since: the game offers a resident and a non-resident
/// architect and draws them on opposite sides of the diamond. It is what makes
/// a one-block read a *partial* read rather than a board with one architect —
/// see `super::advisor::Warning::PartialArchitects`.
pub const ARCHITECTS_PER_PANEL: usize = 2;

/// One line of panel text as the parsers below read it: the string, and the
/// box it was read at when the caller has one.
///
/// Two populations implement it, and the difference is the whole point:
///
/// - `str` / `String` — the text-only callers. Every parsing test in this file
///   is one, and so is anything that only has a transcript. `rect` is `None`,
///   and the grouping falls back to the order the lines were handed over in.
/// - [`OcrLineBox`] — the production read. `crate::ocr::recognize_lines`
///   reports a box per line, [`SystemOcr`] moves it out of the 2× preprocessed
///   space and [`crop_lines`] moves it into CAPTURE px, so by the time it
///   reaches here the box is where the game drew the text.
///
/// A blanket `impl<T: AsRef<str>>` is not possible alongside the
/// [`OcrLineBox`] one — the compiler cannot rule out a future
/// `impl AsRef<str> for OcrLineBox` — so the string cases are spelled out and
/// one blanket over references covers `&&str`, `&String` and `&OcrLineBox`.
pub trait TextLine {
    fn text(&self) -> &str;
    /// `[x, y, w, h]` in CAPTURE px, or `None` when nothing reported one.
    fn rect(&self) -> Option<[i32; 4]> {
        None
    }
}

impl TextLine for str {
    fn text(&self) -> &str {
        self
    }
}

impl TextLine for String {
    fn text(&self) -> &str {
        self
    }
}

impl<T: TextLine + ?Sized> TextLine for &T {
    fn text(&self) -> &str {
        (**self).text()
    }
    fn rect(&self) -> Option<[i32; 4]> {
        (**self).rect()
    }
}

impl TextLine for OcrLineBox {
    fn text(&self) -> &str {
        &self.text
    }
    fn rect(&self) -> Option<[i32; 4]> {
        Some([self.x, self.y, self.w, self.h])
    }
}

/// The smallest box containing both.
fn union(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    let right = (a[0] + a[2]).max(b[0] + b[2]);
    let bottom = (a[1] + a[3]).max(b[1] + b[3]);
    [x, y, right - x, bottom - y]
}

/// The lines' indices in READING order — top to bottom, then left to right.
///
/// **The hardening POE-243 exists for.** Windows OCR emits lines in its own
/// order, and the panel wraps an offer over two or three of them; a
/// continuation emitted before the `Architect` line it belongs to is silently
/// dropped by the sequence-only grouping, and the offer is then either lost or
/// truncated at the wrap. Ordering by the boxes first removes the engine's
/// order from the answer entirely.
///
/// Reordering happens ONLY when every line carries a box. A partially-boxed
/// list has no total order to sort by — a boxless line has no position to place
/// the boxed ones around — and the production path never produces one:
/// `crate::ocr::recognize_lines` DROPS a line whose words all failed to report
/// a rect rather than emitting it at the origin. So a mixed list means a
/// caller mixed a transcript into a read, and engine order is the only order
/// that caller stated.
///
/// # Why the top alone is not the key
///
/// Two boxes on ONE visual row rarely share a top: they are glyph bounding
/// boxes, so a row whose left half has no ascender starts a pixel or two lower
/// than its right half. Sorting on the raw top then puts the right box first
/// and joins `Hall of` / `Champions` as `Champions Hall of`, which scores 0.60
/// against the vocabulary and reads as an unread plate.
///
/// So the sort is two-phase rather than one comparator: order by top, group the
/// result into ROWS — a line joins the open row while its box overlaps the
/// row's FIRST box vertically — then order each row left to right. Grouping
/// against the row's first box and not its running span is what stops one tall
/// line from chaining the whole panel into a single row.
///
/// A two-phase construction rather than a "same row ⇒ compare x, else compare
/// y" comparator on purpose: that relation is not transitive (three boxes can
/// overlap pairwise down a staircase), and `sort_by` on a non-total order is
/// documented to panic or return nonsense.
///
/// # The premise the banding rests on, and which way it fails
///
/// **Line PITCH exceeds glyph HEIGHT.** The band is one box's own height, so
/// two lines land in one row exactly when the second starts before the first
/// ends — which is what "the same visual row" means only while the game leaves
/// leading between lines. Every measurement to hand does: the laptop panel
/// capture reviewed for POE-243 reads h 12 against a 13 px pitch.
///
/// That is **1 px of margin**, so name the failure direction rather than trust
/// it. Where pitch ≤ height, a wrap bands into the row above it and the row is
/// then ordered by x — so a wrapped line whose box starts further left than the
/// line above would be emitted BEFORE it.
///
/// The two callers are not equally exposed:
///
/// - the **panel** path is safe either way. Both lines of a block share the
///   panel's left edge, and the within-row key is `(x, y)` — so a banded wrap
///   ties on x and falls back to y, which is the order it was already in. It
///   also does not matter to [`architect_blocks`], which reads the sequence
///   and not the row structure.
/// - the **plate strip** ([`read_plate`]) is where it would show. That crop is
///   a two-line name band and the whole point of the row rule there is to join
///   the halves in order; a wrapped second line indented LEFT of the first
///   would then join as `Workshop Gemcutter's`. Nothing measured does that —
///   the names are left-aligned — but it is the case to check first if a plate
///   starts reading Unknown on a client whose line spacing is tighter.
fn reading_order<L: TextLine>(lines: &[L]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..lines.len()).collect();
    let Some(rects) = lines.iter().map(|l| l.rect()).collect::<Option<Vec<_>>>() else {
        return order;
    };
    // A stable sort, so two lines with the same box keep the order they came in.
    order.sort_by_key(|&i| (rects[i][1], rects[i][0]));

    let mut out: Vec<usize> = Vec::with_capacity(order.len());
    let mut row: Vec<usize> = Vec::new();
    let flush = |row: &mut Vec<usize>, out: &mut Vec<usize>| {
        row.sort_by_key(|&i| (rects[i][0], rects[i][1]));
        out.append(row);
    };
    for i in order {
        let same_row = row
            .first()
            .is_some_and(|&first| rects[i][1] < rects[first][1] + rects[first][3]);
        if !same_row {
            flush(&mut row, &mut out);
        }
        row.push(i);
    }
    flush(&mut row, &mut out);
    out
}

/// How far below a block's last line a wrapped continuation may start, as a
/// multiple of THAT line's own glyph height, measured top to top.
///
/// **This number is the bound; the measurement is the band under it.** The
/// laptop panel capture reviewed for POE-243 puts a wrap at 1.03 to 1.15 of the
/// preceding line's height — glyph boxes, so the ratio moves with which letters
/// the line happens to carry — and 1.5 is the round number above that band.
/// What it has to stay under is the gap to the *next* block, which the game
/// draws on the other side of the diamond and a full offer lower.
///
/// Only that one capture is behind the band, so treat it as a floor rather than
/// as a population. A dump's `ocr-lines.json` (POE-243) carries every line's
/// box, which is what to re-measure against before moving this.
///
/// It multiplies the PREVIOUS line's height and not the taller of the two,
/// deliberately: the temple screen prints tall furniture — the `Enter
/// Incursion` button is nearly twice a text line — and letting the candidate's
/// own height widen the gate is exactly backwards. How far a wrap sits below
/// the line it wraps is a fact about the line it wraps.
const CONTINUATION_PITCH: f32 = 1.5;

/// Whether `next` is the wrapped continuation of a block whose last line is
/// `prev`: the same column, directly below.
///
/// Horizontal OVERLAP rather than a shared left edge, because the game indents
/// neither the wrap nor the `(Kill to …` line consistently and the boxes are
/// glyph bounding boxes, not text-field bounds. Two lines with no horizontal
/// overlap at all are two columns, which the panel does draw: the two offers
/// sit on opposite sides of the diamond.
fn continues(prev: [i32; 4], next: [i32; 4]) -> bool {
    let overlaps = next[0] < prev[0] + prev[2] && prev[0] < next[0] + next[2];
    let pitch = prev[3].max(1) as f32;
    let drop = (next[1] - prev[1]) as f32;
    // Slightly-negative tolerance: two lines of one wrap can differ by a few px
    // of ascender, and the horizontal overlap has already ruled out a
    // side-by-side pair.
    overlaps && drop > -pitch / 2.0 && drop <= CONTINUATION_PITCH * pitch
}

/// Whether a line opens an architect block.
///
/// The word `Architect` is the marker rather than the comma alone: two of the
/// eight measured boards print `Royal Architect` with no `of <X>` tail, so
/// keying on `Architect of` would drop them. Nothing else is required of the
/// line — a wrapped offer whose comma the OCR missed
/// (`Xipocado Royal Architect` / `(Kill to upgrade to` / `Omnitect Reactor
/// Plant)`) carries neither the comma nor the kill clause on its *first* line,
/// and demanding either here drops the whole offer.
///
/// That leaves the word doing the work alone, which is why
/// [`ARCHITECT_KEYWORD`] is set above [`KEYWORD`]: `artefacts` clears the
/// looser gate. The two consumers each add their own second half rather than
/// tightening this one:
///
/// - [`parse_architect_block`] requires a `Kill to <verb> to` clause, so a
///   pseudo-block never becomes an offer;
/// - [`read_panel`] skips only the blocks that parsed or closed, so a
///   pseudo-block never swallows the panel title.
pub fn starts_architect(line: &str) -> bool {
    words(line).iter().any(|w| word_is_architect(&w.key))
}

/// Position of `kill to <verb> to` in `words`, with the verb it names.
///
/// The trailing `to` is what pins the room's first word; without it a title
/// such as `Architect of the Hoard` could swallow the verb position.
fn kill_clause(words: &[Word<'_>]) -> Option<(usize, OfferKind)> {
    for i in 0..words.len().saturating_sub(3) {
        if !word_is(&words[i].key, "kill") || !word_is(&words[i + 1].key, "to") {
            continue;
        }
        let kind = if word_is(&words[i + 2].key, "change") {
            OfferKind::Change
        } else if word_is(&words[i + 2].key, "upgrade") {
            OfferKind::Upgrade
        } else {
            continue;
        };
        if !word_is(&words[i + 3].key, "to") {
            continue;
        }
        return Some((i, kind));
    }
    None
}

/// Lines an architect block may run to and still count as bounded.
///
/// MEASURED over the six reference captures: the panel breaks an offer over
/// one, two or three lines and prints the closing bracket on the last of them.
/// A "block" that reaches a fourth line has not been read as an offer — it is
/// an `Architect`-scoring line that never closed.
///
/// # The four-line fixture is synthetic, and what it costs
///
/// `laptop_panel_with_map_fragment` in the tests builds a FOUR-line Hayoxi
/// block: the architect's name wrapped (`Hayoxi, Architect of` /
/// `Destruction`) **and** the clause wrapped (`(Kill to upgrade to Omnitect` /
/// `Reactor Plant)`). No capture shows both wraps at once — each half is
/// measured, the combination is a worst case constructed to put the map-info
/// fragment inside an offer with room on either side of it.
///
/// It costs nothing there because [`Block::is_offer_text`] takes the PARSE as
/// its strong evidence and only falls back to `closed && attached <=
/// MAX_BLOCK_LINES` when there is none. So a genuine four-line offer is still
/// recognised as block text — unless its room name ALSO failed the vocabulary,
/// which is the one state this bound would reject. That state is doubly
/// unmeasured, and the trade is the deliberate one: the bound exists to stop a
/// mis-scored line latching onto a bracket several lines away and swallowing
/// the panel title, which is a live failure with a live incident behind it.
const MAX_BLOCK_LINES: usize = 3;

/// One run of OCR lines that opened on an `Architect` word.
struct Block {
    /// Positions in READING order (see [`reading_order`]) the run SPANS,
    /// inclusive — NOT indices into the caller's array, and not the same thing
    /// as the lines it took: a foreign line skipped inside the run (see
    /// [`architect_blocks`] rule 2) falls in this range without being part of
    /// the block.
    ///
    /// The range is what [`read_panel`] refuses to read a title from, and
    /// covering a skipped line there is the conservative direction — a line
    /// sitting between two lines of an architect block is not where the game
    /// draws the panel title, which it prints above both blocks.
    start: usize,
    end: usize,
    /// How many lines the run actually TOOK, the opener included. The length
    /// bound below counts these and not the span, so a skipped line cannot
    /// push a two-line offer past [`MAX_BLOCK_LINES`].
    attached: usize,
    /// Whether the run ended on its own closing bracket, rather than at the
    /// next `Architect` line or at the end of the input.
    closed: bool,
    /// The run's lines, joined back into one string.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    text: String,
    /// The offer, when the run is one.
    offer: Option<ArchitectOffer>,
}

impl Block {
    /// Whether the run is one the panel really printed, rather than a line
    /// that merely scored like the keyword.
    ///
    /// Parsing is the strong evidence. A bracket closed inside
    /// [`MAX_BLOCK_LINES`] is the weaker one, kept because an offer whose room
    /// name OCR mangled past the vocabulary is still an offer and its lines
    /// still must not be read as the panel title.
    fn is_offer_text(&self) -> bool {
        self.offer.is_some() || (self.closed && self.attached <= MAX_BLOCK_LINES)
    }
}

/// Regroup OCR lines into whole architect blocks, keeping each block's span
/// and the box it covers.
///
/// The panel wraps each offer over two or three lines, and the two offers sit
/// on opposite sides of the diamond — so no line of one offer ever falls
/// between two lines of the OTHER. That is the whole of what the geometry
/// guarantees. It says nothing about FOREIGN text: the panel title, and
/// whatever else the crop caught (see the note below), can and does land
/// between an offer's lines in reading order, which is what rule 2 skips.
///
/// Two rules build a block out of that, and since POE-243 the first one is
/// geometric wherever the lines carry boxes:
///
/// 1. **Order.** The lines are walked in [`reading_order`] — top, then left —
///    rather than in the order the OCR engine emitted them. A boxless list
///    keeps the engine's order, which is all it states.
/// 2. **Attachment.** A non-`Architect` line joins the open block only when
///    [`continues`] says it is in the same column and directly below the
///    block's last line. A line that fails that test is SKIPPED — left as
///    ordinary text, with the block still open behind it.
///
/// A line with no box attaches by sequence alone, which is the pre-POE-243
/// rule and the one every text-only transcript is written against.
///
/// A block still runs to the line that closes the parenthesis, or to the next
/// `Architect` line if the bracket was never read.
///
/// # Why a refused line is skipped and not a block boundary
///
/// The sequence-only rule read the "offers do not interleave" guarantee above
/// as the stronger claim that NOTHING lands between an offer's lines. That is
/// false: [`super::run::panel_rect`]'s right margin (POE-230) admits the map's
/// own info block at the panel's edge, and a fragment of it sorts by its top
/// into the middle of an offer. Closing the block there would drop every line
/// of the offer that follows — including the `(Kill to …` clause, so the whole
/// offer — which is the same lost-architect failure POE-243 exists to remove,
/// arriving through the fix for it.
///
/// So the refusal is narrower than a boundary: it says "this line is not part
/// of this block", which is exactly what was measured, and says nothing about
/// what comes after it. The block still ends where it always ended — on its
/// bracket, on the next `Architect` line, or at the end of the input.
fn architect_blocks<L: TextLine>(lines: &[L]) -> Vec<Block> {
    /// The run being built. `last` is the newest ATTACHED line's own box, which
    /// is what [`continues`] measures the next line against — `rect`, the
    /// running union, has already grown past it, and a skipped line never
    /// touches either.
    struct Open {
        start: usize,
        end: usize,
        attached: usize,
        text: String,
        rect: Option<[i32; 4]>,
        last: Option<[i32; 4]>,
    }

    let mut out: Vec<Block> = Vec::new();
    let mut current: Option<Open> = None;
    let mut close = |current: &mut Option<Open>, closed: bool| {
        if let Some(run) = current.take() {
            let mut offer = parse_architect_block(&run.text);
            if let Some(offer) = offer.as_mut() {
                offer.rect = run.rect;
            }
            out.push(Block {
                start: run.start,
                end: run.end,
                attached: run.attached,
                closed,
                text: run.text,
                offer,
            });
        }
    };
    for (at, &i) in reading_order(lines).iter().enumerate() {
        let line = lines[i].text();
        let rect = lines[i].rect();
        if starts_architect(line) {
            close(&mut current, false);
            current = Some(Open {
                start: at,
                end: at,
                attached: 1,
                text: line.trim().to_string(),
                rect,
                last: rect,
            });
        } else {
            // Geometry decides attachment when BOTH lines placed themselves;
            // otherwise the caller's sequence is the only evidence there is.
            let attaches = match (current.as_ref().and_then(|run| run.last), rect) {
                (Some(prev), Some(next)) => continues(prev, next),
                _ => true,
            };
            let Some(run) = current.as_mut().filter(|_| attaches) else {
                // Either nothing is open, or the line is not this block's —
                // ordinary text either way, and the block (if any) stays open
                // behind it. See the note above on why this is not a boundary.
                continue;
            };
            run.end = at;
            run.attached += 1;
            run.text.push(' ');
            run.text.push_str(line.trim());
            run.rect = match (run.rect, rect) {
                (Some(a), Some(b)) => Some(union(a, b)),
                (a, b) => a.or(b),
            };
            run.last = rect;
            // Only an ATTACHED line can close the run. A skipped line's
            // bracket belongs to whatever else printed it.
            if line.trim_end().ends_with(')') {
                close(&mut current, true);
            }
            continue;
        }
        if line.trim_end().ends_with(')') {
            close(&mut current, true);
        }
    }
    close(&mut current, false);
    out
}

/// The architect blocks as text, in reading order.
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub fn group_architect_blocks<L: TextLine>(lines: &[L]) -> Vec<String> {
    architect_blocks(lines)
        .into_iter()
        .map(|block| block.text)
        .collect()
}

/// Parse one whole architect block.
///
/// Requires **both** halves — an `Architect` word and a `Kill to <verb> to`
/// clause — so an ordinary line of the panel returns `None` instead of a
/// half-filled offer.
pub fn parse_architect_block(block: &str) -> Option<ArchitectOffer> {
    let words = words(block);
    let architect_at = words.iter().position(|w| word_is_architect(&w.key))?;
    let (kill_at, kind) = kill_clause(&words)?;
    if kill_at <= architect_at {
        return None;
    }

    let printed_target = words[kill_at + 4..]
        .iter()
        .map(|w| w.raw.trim_end_matches(')'))
        .filter(|raw| !raw.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if printed_target.is_empty() {
        return None;
    }

    // The name is everything before the first comma, and failing that the
    // words before `Architect`. The comma is what separates the name from the
    // title, so only the first form isolates it: `Xipocado, Royal Architect`
    // yields `Xipocado`, and a comma-less read of the same line yields
    // `Xipocado Royal`. Keeping the title in is the deliberate trade — the
    // alternative, guessing that the last word before `Architect` is part of
    // the title, would eat a two-word architect name.
    let name = match block.find(',') {
        Some(comma) => block[..comma].trim().to_string(),
        None => words[..architect_at]
            .iter()
            .map(|w| w.raw)
            .collect::<Vec<_>>()
            .join(" "),
    };
    if name.is_empty() {
        return None;
    }

    Some(ArchitectOffer {
        architect_name: name,
        kind,
        target: rooms::match_room_name(&printed_target),
        printed_target,
        // The block's own lines are what carry boxes, and this function is
        // handed one joined string. `architect_blocks` fills it in.
        rect: None,
    })
}

/// Every architect offer the OCR lines contain.
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub fn parse_architects<L: TextLine>(lines: &[L]) -> Vec<ArchitectOffer> {
    architect_blocks(lines)
        .into_iter()
        .filter_map(|block| block.offer)
        .collect()
}

// ------------------------------------------------- incursions remaining --

/// Parse `N Incursions Remaining` — the budget for the whole temple.
///
/// Both observed orderings are accepted (`9 Incursions Remaining` and
/// `Incursions Remaining: 9`), as is the singular at 1. The count is only
/// looked for on a line that already carries the label, which is what makes
/// the digit-confusion folding (`O`→0, `l`/`I`→1, `S`→5, `B`→8) safe: outside
/// that line, `so` would fold to `50`.
///
/// **Known limitation:** a count read with *no* surviving ASCII digit
/// (`l Incursion Remaining`) is rejected rather than guessed at 1.
pub fn parse_incursions_remaining<L: TextLine>(lines: &[L]) -> Option<u8> {
    for line in lines {
        let words = words(line.text());
        let labelled = (0..words.len().saturating_sub(1)).any(|i| {
            (word_is(&words[i].key, "incursions") || word_is(&words[i].key, "incursion"))
                && word_is(&words[i + 1].key, "remaining")
        });
        if !labelled {
            continue;
        }
        for word in &words {
            if let Some(count) = fold_count(&word.key) {
                return Some(count);
            }
        }
    }
    None
}

/// A 1-to-3 character token as a number, folding the digit shapes OCR
/// confuses. At least one character must already be an ASCII digit.
fn fold_count(key: &str) -> Option<u8> {
    if key.is_empty() || key.len() > 3 || !key.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut folded = String::with_capacity(key.len());
    for c in key.chars() {
        let digit = match c {
            '0'..='9' => c,
            'o' => '0',
            'l' | 'i' => '1',
            's' => '5',
            'b' => '8',
            _ => return None,
        };
        folded.push(digit);
    }
    folded.parse().ok()
}

// -------------------------------------------------------- panel reading --

/// Everything one OCR pass over the open temple screen yields, minus the door
/// markers.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelReading {
    /// The panel title — the room the player is standing in.
    pub room: Match,
    /// `[x, y, w, h]` of the title line on screen, in CAPTURE px (POE-243).
    ///
    /// The box of the ONE line the title rule picked, not of every line that
    /// scored: it is where the game printed the current room's name, which is
    /// what a surface has to avoid covering. `None` when the title was unread,
    /// or when the lines carried no boxes.
    pub room_rect: Option<[i32; 4]>,
    /// Both architects, in reading order.
    pub architects: Vec<ArchitectOffer>,
    /// The remaining budget, when the layout panel's footer was legible.
    pub incursions_remaining: Option<u8>,
}

impl PanelReading {
    /// The title's game name, when it was legible.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn identity_name(&self) -> Option<&'static str> {
        self.room.identity().map(|id| id.display_name())
    }
}

/// Constant screen furniture the temple screen prints, which is not a room
/// name and must never be matched as one.
///
/// `Temple of Atzoatl` is the layout panel's own window header. It is printed
/// on **100% of the captures** — six reference screenshots, six of six — above
/// the side panel's title, so OCR hands it to [`read_panel`] *before* the real
/// title on every read. It scores 0.8515 against `Apex of Atzoatl`, which is
/// under [`rooms::MATCH`], but a single OCR edit is enough to clear the gate:
/// `tample of atzoatl` reaches 0.9190, `aemple of atzoatl` 0.9056,
/// `tmple of atzoatl` 0.8931 — each a confident, wrong `Apex`.
pub const SCREEN_FURNITURE: [&str; 1] = ["Temple of Atzoatl"];

/// Whether a line is one of the constant strings in [`SCREEN_FURNITURE`].
///
/// Matched with [`rooms::MATCH`], the same gate the room vocabulary uses.
///
/// **MEASURED**, over all 1188 single-edit slips of `temple of atzoatl`
/// (every substitution, deletion and insertion of `[a-z0-9]`, post-normalise):
/// the slips run from **0.8358** to 1.0, so the gate at 0.88 does **not** clear
/// the whole population — three slips fall under it, `oemple of atzoatl`
/// 0.8358, `otemple of atzoatl` 0.8442 and `toemple of atzoatl` 0.8775. All
/// three lose the header's prefix, which is what Jaro-Winkler weights most.
///
/// Those three are not a hole, because the header only *matters* when it can
/// be mistaken for the Apex, and losing the prefix costs them that too: they
/// score 0.7746, 0.7091 and 0.7604 against `Apex of Atzoatl`, far under the
/// same gate, so [`rooms::match_room_name`] returns Unknown and the title is
/// simply unread. Of the 1188 slips, **11 do reach a fuzzy `Apex of Atzoatl`
/// — and all 11 are caught here**, over 0.88 against the header.
///
/// The backstop for anything that clears both is [`title_match`]'s fixed-slot
/// rule, which takes `Apex of Atzoatl` and `Entrance` on an **exact** read
/// only. Nothing in the measured population needs it today; it is what keeps a
/// two-edit slip, or a second string added to [`SCREEN_FURNITURE`], from
/// turning into a confident, wrong Apex.
///
/// The nearest real room name, `Apex of Atzoatl`, is 0.8515 from the header
/// and so is not swallowed by this gate.
pub fn is_screen_furniture(line: &str) -> bool {
    let key = rooms::normalise(line);
    if key.is_empty() {
        return false;
    }
    SCREEN_FURNITURE
        .iter()
        .any(|f| jaro_winkler(&key, &rooms::normalise(f)) >= rooms::MATCH)
}

/// Read the panel from OCR lines covering the whole temple screen.
///
/// # Which line is the title
///
/// **Positional, not first-match**: the title is the vocabulary line
/// immediately *above* the first architect block that parsed. That is where
/// the game draws it — the side panel prints the current room's name and then
/// its two architects — and it is the only rule that survives the layout
/// panel, whose plate names OCR interleaves with the side panel's lines. A
/// first-match rule hands back whichever plate name happened to be read first.
///
/// When there is no parsed block above which to look — no architect was read,
/// or the block is the very first line — the rule falls back to the first
/// vocabulary line outside a block. Taking the *first* rather than the
/// best-scoring line still matters there: on a board such as Case 1 (`Tombs`,
/// offering `Storage Room`) the offer's target is the longer, cleaner
/// vocabulary hit.
///
/// # Which lines belong to a block
///
/// Only the blocks that **parsed**, or that closed their bracket inside
/// [`MAX_BLOCK_LINES`], are skipped. A run that neither parses nor closes is
/// left as ordinary text.
///
/// [`starts_architect`] is a single fuzzy word, so a run it opens is a
/// *hypothesis* about the lines that follow, not a fact about them. Treating
/// every run as a block makes that hypothesis unfalsifiable, and the cost is
/// paid by whatever the panel printed next: the [`ARCHITECT_KEYWORD`] incident
/// is the shape of it, where one mis-scored line opened a run that reached the
/// next closing bracket and took the title with it. Requiring a parse or a
/// close is what lets the run be wrong without costing anything.
///
/// Two things a plain vocabulary match would accept are refused here:
/// [`SCREEN_FURNITURE`], and a *fuzzy* read of one of the two fixed-slot names
/// — see [`title_match`].
pub fn read_panel<L: TextLine>(lines: &[L]) -> PanelReading {
    // Every index below is a position in READING order, which is also the
    // order the blocks' spans are recorded in — so `order[at]` is the one
    // place a position becomes a line again.
    let order = reading_order(lines);
    let blocks = architect_blocks(lines);
    let spans: Vec<(usize, usize)> = blocks
        .iter()
        .filter(|block| block.is_offer_text())
        .map(|block| (block.start, block.end))
        .collect();
    // The title AND the box it was read at, so a surface can point at the line
    // rather than re-derive where the panel drew it.
    let title_at = |at: usize| -> Option<(Match, Option<[i32; 4]>)> {
        if spans.iter().any(|(start, end)| (*start..=*end).contains(&at)) {
            return None;
        }
        let line = &lines[order[at]];
        match title_match(line.text()) {
            Match::Unknown => None,
            found => Some((found, line.rect())),
        }
    };

    let above = blocks
        .iter()
        .find(|block| block.offer.is_some())
        .map_or(0, |block| block.start);
    let (room, room_rect) = (0..above)
        .rev()
        .find_map(title_at)
        .or_else(|| (0..order.len()).find_map(title_at))
        .unwrap_or((Match::Unknown, None));

    PanelReading {
        room,
        room_rect,
        architects: blocks.into_iter().filter_map(|block| block.offer).collect(),
        incursions_remaining: parse_incursions_remaining(lines),
    }
}

/// Match one line **as a panel title**, which is stricter than matching it as
/// a room name.
///
/// Two refusals on top of [`rooms::match_room_name`]:
///
/// - [`SCREEN_FURNITURE`] is not a room, however well it scores;
/// - `Apex of Atzoatl` and `Entrance` are accepted only as
///   [`Match::Exact`] reads. Both are fixed slots the game draws once per
///   board, and both sit at the bottom of a very shallow basin: ten
///   single-edit slips of the window header alone clear [`rooms::MATCH`] and
///   [`rooms::LEAD`] against `Apex of Atzoatl`. Their names are also short and
///   unmistakable when they *are* read, so requiring an exact (post-normalise)
///   read costs nothing and closes the basin.
///
/// This is a **title** rule, not a vocabulary rule: a plate strip
/// ([`read_plate`]) still takes a fuzzy Entrance or Apex, because there the
/// crop's position already says which slot is being read.
fn title_match(line: &str) -> Match {
    if is_screen_furniture(line) {
        return Match::Unknown;
    }
    match rooms::match_room_name(line) {
        Match::Fuzzy(RoomIdentity::Entrance | RoomIdentity::Apex, _) => Match::Unknown,
        other => other,
    }
}

// --------------------------------------------------- plates and the OCR --

/// One plate's identity, as read.
///
/// [`Match::Unknown`] is a first-class value here rather than a reason to drop
/// the slot: POE-171 draws the plate as unread so the player can see which
/// room the advisor is ignoring.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomReading {
    pub slot: Slot,
    pub identity: Match,
}

/// Fraction of a plate's height, measured from its top, at which the name
/// band starts.
///
/// Measured by cropping the reference board's plates: a single-line name
/// occupies the bottom quarter and a wrapped one (`Gemcutter's Workshop`)
/// starts at 0.55. 0.48 clears the wrapped case, stays below the room art —
/// whose bright edges cost OCR accuracy — and, deliberately, starts **below**
/// [`NUMERAL_BOTTOM`]: a numeral swept into the name crop turns
/// `Torment Cells` into `I Torment Cells`, which is a 1.15 length ratio and a
/// worse Jaro-Winkler score for no gain.
const NAME_TOP: f64 = 0.48;
/// Fraction of a plate's width the tier numeral's column occupies, from the
/// left border. The numeral is drawn against the plate's left border, ~0.15 of
/// its width wide.
const NUMERAL_W: f64 = 0.22;
/// Vertical band of the plate the tier numeral sits in. Measured on `II`,
/// `III` and `I` plates of the reference board: 0.15–0.33, so this band has
/// margin on both sides and still clears the name.
const NUMERAL_TOP: f64 = 0.08;
const NUMERAL_BOTTOM: f64 = 0.42;

/// `[x, y, w, h]` of a plate's name band, image px.
pub fn name_strip(lattice: &Lattice, slot: Slot) -> [i32; 4] {
    let (cx, cy) = lattice.centre(slot);
    let (hw, hh) = lattice.plate_half();
    let top = cy - hh + (2.0 * hh as f64 * NAME_TOP).round() as i32;
    [cx - hw, top, 2 * hw, cy + hh - top]
}

/// `[x, y, w, h]` of a plate's tier numeral, image px.
pub fn numeral_box(lattice: &Lattice, slot: Slot) -> [i32; 4] {
    let (cx, cy) = lattice.centre(slot);
    let (hw, hh) = lattice.plate_half();
    let top = cy - hh + (2.0 * hh as f64 * NUMERAL_TOP).round() as i32;
    let bottom = cy - hh + (2.0 * hh as f64 * NUMERAL_BOTTOM).round() as i32;
    [
        cx - hw,
        top,
        (2.0 * hw as f64 * NUMERAL_W).round() as i32,
        bottom - top,
    ]
}

/// The one OCR call this module makes, behind a trait so the parsers above can
/// be tested without an OCR engine.
pub trait TextRecognizer {
    /// Recognised lines, each with the box it was read at, in the pixels of
    /// the image passed in — and in the ENGINE's own order, which
    /// [`reading_order`] is what corrects.
    ///
    /// Boxes rather than bare strings since POE-243: a block's screen rect is
    /// the union of its lines' boxes, and the grouping needs them to tell a
    /// wrapped continuation from a line of some other column. Dropping them
    /// here — which is what this method used to do — is what made both
    /// unavailable.
    fn recognize(&self, img: &DynamicImage) -> Result<Vec<OcrLineBox>, String>;
}

/// Move an OCR box out of [`crate::capture::preprocess_for_ocr`]'s upscaled
/// space and back into the pixels of the image that was handed to it.
///
/// Rounded OUTWARD — near edge down, far edge up — so the box never excludes a
/// pixel the glyph covered. The consumer is a surface that must not cover the
/// text (POE-244), and a box one pixel too large costs nothing there while one
/// a pixel too small is the failure it is trying to avoid.
///
/// The size is derived from the FAR EDGE (`x + w`) rather than scaled on its
/// own, because rounding a size independently of where it starts loses the very
/// pixel this rounds outward for: at 2×, `x = 1, w = 2` covers source columns
/// 0 and 1, and a size-only ceiling gives `w = 1`, dropping column 1.
///
/// `upscale` is [`crate::capture::OCR_UPSCALE`] at every production call site;
/// it is a parameter so the arithmetic can be checked against a factor the
/// test states rather than against the constant the function already uses.
pub fn descaled(line: OcrLineBox, upscale: i32) -> OcrLineBox {
    let far = |v: i32| v.div_euclid(upscale) + i32::from(v.rem_euclid(upscale) != 0);
    let x = line.x.div_euclid(upscale);
    let y = line.y.div_euclid(upscale);
    OcrLineBox {
        x,
        y,
        w: (far(line.x + line.w) - x).max(1),
        h: (far(line.y + line.h) - y).max(1),
        text: line.text,
    }
}

/// Move an OCR box out of a crop's pixels and into the capture's, given where
/// the crop was taken from.
///
/// `origin` is the crop's top-left corner in capture px — the CLIPPED one that
/// [`super::run::crop_clipped`] reports, not the rect that was asked for: a ROI
/// hanging off the frame is cropped at 0 and a box placed against the
/// unclipped rect would sit off-screen by however much was cut.
pub fn translated(line: OcrLineBox, origin: (i32, i32)) -> OcrLineBox {
    OcrLineBox {
        x: line.x + origin.0,
        y: line.y + origin.1,
        w: line.w,
        h: line.h,
        text: line.text,
    }
}

/// The production recogniser: the crate's Windows.Media.Ocr binding, through
/// the same [`crate::capture::preprocess_for_ocr`] contrast-and-upscale pass
/// every other capture in the app uses (POE-164).
///
/// No `#[cfg(windows)]` here on purpose — `crate::ocr::recognize_lines` already
/// has a non-Windows arm that returns `Err(UNAVAILABLE)`, so duplicating the
/// gate would only add a second place for the two arms to diverge.
///
/// The 2× descale is HERE and nowhere else, because this is the only place
/// that knows the engine was handed a preprocessed image rather than the crop
/// itself. Its boxes are therefore in the CROP's pixels; [`crop_lines`] is
/// what takes them the last step, into the capture's.
pub struct SystemOcr;

impl TextRecognizer for SystemOcr {
    fn recognize(&self, img: &DynamicImage) -> Result<Vec<OcrLineBox>, String> {
        let prepared = crate::capture::preprocess_for_ocr(img);
        Ok(crate::ocr::recognize_lines(&prepared)?
            .into_iter()
            .map(|line| descaled(line, crate::capture::OCR_UPSCALE as i32))
            .collect())
    }
}

/// One bounded crop's OCR lines, in CAPTURE px.
///
/// The single seam both readers of the panel's text go through — the capture
/// loop (`super::run::panel_text`) and the debug dump
/// (`super::commands::debug_capture_blocking`) — so the two cannot disagree
/// about where a line is. `origin` is the crop's clipped top-left corner in
/// capture px; see [`translated`].
pub fn crop_lines(crop: &DynamicImage, origin: (i32, i32)) -> Result<Vec<OcrLineBox>, String> {
    Ok(SystemOcr
        .recognize(crop)?
        .into_iter()
        .map(|line| translated(line, origin))
        .collect())
}

/// Read every plate's name off a board.
///
/// A slot whose crop falls outside the capture, or whose OCR call fails, comes
/// back as [`Match::Unknown`] rather than being dropped: the board always has
/// 13 slots and a missing one would silently change the graph POE-170 scores.
///
/// # `should_stop`
///
/// This is 26 OCR calls — the longest blocking stretch in the capture loop, and
/// one a detached thread cannot be aborted out of (POE-171). `should_stop` is
/// polled **before each plate**, so a stop signal costs at most the two calls
/// already in flight rather than all 26. The slots not reached come back
/// [`Match::Unknown`], the same value an unreadable plate gets: the caller
/// discards a stopped read rather than publishing it, and 13 entries is the
/// invariant above.
///
/// Pass `&|| false` when there is nothing to stop for.
pub fn read_board(
    recognizer: &dyn TextRecognizer,
    img: &DynamicImage,
    lattice: &Lattice,
    should_stop: &dyn Fn() -> bool,
) -> Vec<RoomReading> {
    Slot::ALL
        .into_iter()
        .map(|slot| RoomReading {
            slot,
            identity: if should_stop() {
                Match::Unknown
            } else {
                read_plate(recognizer, img, lattice, slot)
            },
        })
        .collect()
}

fn read_plate(
    recognizer: &dyn TextRecognizer,
    img: &DynamicImage,
    lattice: &Lattice,
    slot: Slot,
) -> Match {
    let [x, y, w, h] = name_strip(lattice, slot);
    if x < 0 || y < 0 || w <= 0 || h <= 0 {
        return Match::Unknown;
    }
    let (x, y, w, h) = (x as u32, y as u32, w as u32, h as u32);
    if x + w > img.width() || y + h > img.height() {
        return Match::Unknown;
    }
    let crop = img.crop_imm(x, y, w, h);
    let Ok(lines) = recognizer.recognize(&crop) else {
        return Match::Unknown;
    };
    // A wrapped name arrives as two lines; the plate holds one name, so
    // joining is right and matching each line separately would fail both.
    //
    // Joined in READING order rather than in the engine's (POE-243). A plate
    // name wraps within one narrow strip, so top-to-bottom is the order the
    // game printed it in — `Gemcutter's` then `Workshop`. The engine emitting
    // the tail first would otherwise produce `Workshop Gemcutter's`, which is
    // 0.60 against the vocabulary entry and reads as an unread plate.
    let joined = reading_order(&lines)
        .into_iter()
        .map(|i| lines[i].text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let numeral = read_numeral(recognizer, img, lattice, slot);
    rooms::cross_check_numeral(rooms::match_room_name(&joined), numeral)
}

fn read_numeral(
    recognizer: &dyn TextRecognizer,
    img: &DynamicImage,
    lattice: &Lattice,
    slot: Slot,
) -> Option<super::strategy::Tier> {
    let [x, y, w, h] = numeral_box(lattice, slot);
    if x < 0 || y < 0 || w <= 0 || h <= 0 {
        return None;
    }
    let (x, y, w, h) = (x as u32, y as u32, w as u32, h as u32);
    if x + w > img.width() || y + h > img.height() {
        return None;
    }
    let crop = img.crop_imm(x, y, w, h);
    let lines = recognizer.recognize(&crop).ok()?;
    lines.iter().find_map(|line| rooms::parse_numeral(&line.text))
}

#[cfg(test)]
mod tests {
    use super::super::rooms::{resolve_name, RoomIdentity};
    use super::super::strategy::Tier;
    use super::*;

    /// The side panel of `2026-08-02_22-22-38` (TEMPLE-CORE-RULES §5 Case 1),
    /// transcribed line by line as the panel wraps it: Tombs at D3, two junk
    /// `change` offers.
    const CASE_1: [&str; 6] = [
        "Tombs",
        "Ticaba, Architect of the Arena",
        "(Kill to change to Storage",
        "Room)",
        "Juatalotli, Architect of the Hoard",
        "(Kill to change to Sparring Room)",
    ];

    /// The live board `2026-08-07_19-28-36`: Lightning Workshop, and the one
    /// measured `upgrade` offer, from an architect whose title carries no
    /// `of <X>` tail.
    const LIVE: [&str; 5] = [
        "Lightning Workshop",
        "Xipocado, Royal Architect",
        "(Kill to upgrade to",
        "Omnitect Reactor Plant)",
        "Xopec, Architect of Power (Kill to change to Royal Meeting Room)",
    ];

    fn offer(lines: &[&str]) -> ArchitectOffer {
        let found = parse_architects(lines);
        assert_eq!(found.len(), 1, "expected one offer in {lines:?}");
        found.into_iter().next().expect("one offer")
    }

    /// Glyph height of one panel line in the fixtures below, capture px.
    const LINE_H: i32 = 20;
    /// Top-to-top distance the panel wraps at — 1.15 line heights, which is
    /// what the reference captures measure and what [`CONTINUATION_PITCH`] is
    /// sized to clear.
    const LINE_PITCH: i32 = 23;

    /// One OCR line with a box, in capture px. Width tracks the text so two
    /// lines of different lengths still overlap horizontally, which is what
    /// [`continues`] tests.
    fn boxed(text: &str, x: i32, y: i32) -> OcrLineBox {
        OcrLineBox {
            text: text.to_string(),
            x,
            y,
            w: 8 * text.len() as i32,
            h: LINE_H,
        }
    }

    /// The PC side panel of `2026-09-03_13-56-40` — Armourer's Workshop at C2,
    /// Quipolatl offering an upgrade and Atmohua a `change` whose target wraps
    /// onto a second line. Boxes are the panel's own column: one x, one line
    /// per [`LINE_PITCH`], the two offers a block apart.
    ///
    /// Returned in READING order; the tests that need the engine's wrong order
    /// permute it.
    fn pc_panel() -> Vec<OcrLineBox> {
        const X: i32 = 1300;
        vec![
            boxed("Armourer's Workshop", X, 100),
            boxed("Quipolatl, Architect of the Armoury", X, 140),
            boxed("(Kill to upgrade to Armoury)", X, 140 + LINE_PITCH),
            boxed("Atmohua, Architect of Iron", X, 210),
            boxed("(Kill to change to Shrine of", X, 210 + LINE_PITCH),
            boxed("Empowerment)", X, 210 + 2 * LINE_PITCH),
        ]
    }

    /// [`pc_panel`] as the OCR engine emitted it on the machine that produced
    /// the bad advice: the wrapped continuation `Empowerment)` BEFORE the
    /// architect line it belongs to.
    fn pc_panel_engine_order() -> Vec<OcrLineBox> {
        let lines = pc_panel();
        vec![
            lines[0].clone(),
            lines[1].clone(),
            lines[2].clone(),
            lines[5].clone(),
            lines[3].clone(),
            lines[4].clone(),
        ]
    }

    /// A panel whose crop caught a fragment of the map's own info block at its
    /// right edge — the shape [`super::super::run::panel_rect`]'s deliberately
    /// tight right margin (POE-230) admits.
    ///
    /// The fragment's box is in a different COLUMN (x 1659 against the panel's
    /// 1480) but its top, 130, sits between the architect line's wrap at 128
    /// and the kill clause at 141 — so reading order puts it inside the offer.
    fn laptop_panel_with_map_fragment() -> Vec<OcrLineBox> {
        const X: i32 = 1480;
        const H: i32 = 12;
        let line = |text: &str, x: i32, y: i32| OcrLineBox { h: H, ..boxed(text, x, y) };
        vec![
            line("Tombs", X, 90),
            line("Hayoxi, Architect of", X, 115),
            line("Destruction", X, 128),
            line("Area Level: 68", 1659, 130),
            line("(Kill to upgrade to Omnitect", X, 141),
            line("Reactor Plant)", X, 154),
            line("Xopec, Architect of Power (Kill to change to Royal Meeting Room)", X, 200),
        ]
    }

    /// The same six lines with their boxes thrown away — what the parsers saw
    /// before POE-243, and what a transcript-only caller still sees.
    fn texts(lines: &[OcrLineBox]) -> Vec<String> {
        lines.iter().map(|l| l.text.clone()).collect()
    }

    // ------------------------------------------------------- architects --

    // The `change` wording, wrapped over three lines exactly as the panel
    // breaks it. Fails if block grouping stops joining wrapped lines.
    #[test]
    fn a_change_offer_wrapped_over_three_lines_parses_whole() {
        let got = offer(&CASE_1[1..4]);
        assert_eq!(got.architect_name, "Ticaba");
        assert_eq!(got.kind, OfferKind::Change);
        assert_eq!(got.printed_target, "Storage Room");
        assert_eq!(
            got.target,
            Match::Exact(resolve_name("Storage Room").expect("in vocabulary"))
        );
    }

    // The `upgrade` wording, and the title with no `of <X>` tail — two of the
    // eight measured boards print `Royal Architect`, so keying the parse on
    // the comma rather than the word `Architect` would drop them.
    #[test]
    fn an_upgrade_offer_from_a_titleless_architect_parses() {
        let got = offer(&LIVE[1..4]);
        assert_eq!(got.architect_name, "Xipocado");
        assert_eq!(got.kind, OfferKind::Upgrade);
        assert_eq!(got.printed_target, "Omnitect Reactor Plant");
        assert!(got.target.is_known());
    }

    // One offer, one line, and the block still closes on its bracket.
    #[test]
    fn a_single_line_offer_parses() {
        let got = offer(&LIVE[4..5]);
        assert_eq!(got.architect_name, "Xopec");
        assert_eq!(got.kind, OfferKind::Change);
        assert_eq!(got.printed_target, "Royal Meeting Room");
    }

    // OCR noise in the keywords AND in the room name. `Archrtect`, `Klll` and
    // `chanqe` all have to survive KEYWORD, and `Storaqe Room` has to reach
    // the vocabulary through the fuzzy matcher.
    #[test]
    fn a_noisily_read_offer_still_parses_and_resolves() {
        let got = offer(&["Ticaba, Archrtect of the Arena (Klll to chanqe to Storaqe Room)"]);
        assert_eq!(got.architect_name, "Ticaba");
        assert_eq!(got.kind, OfferKind::Change);
        assert_eq!(got.printed_target, "Storaqe Room");
        match got.target {
            Match::Fuzzy(id, _) => assert_eq!(id.display_name(), "Storage Room"),
            other => panic!("expected a fuzzy target, got {other:?}"),
        }
    }

    // The measured keyword collision, at the level the block is opened: the
    // board can print `Museum of Artefacts` anywhere, and `artefacts` scores
    // 0.8222 against `architect` — over the old 0.82 gate. Fails if
    // ARCHITECT_KEYWORD is dropped back to KEYWORD.
    #[test]
    fn a_room_name_that_scores_like_the_keyword_supplies_no_architect() {
        assert_eq!(
            parse_architect_block("Museum of Artefacts (Kill to change to Storage Room)"),
            None
        );
    }

    // The `Royal Architect` offer with its comma lost to OCR: the first line
    // then carries neither the comma nor the kill clause, so `starts_architect`
    // has only the word to go on and must open the block on that alone. Fails
    // if a comma-or-kill-clause guard is put back on `starts_architect` — the
    // block never opens and the whole offer is dropped.
    #[test]
    fn a_wrapped_offer_whose_comma_was_missed_still_parses() {
        let got = offer(&[
            "Xipocado Royal Architect",
            "(Kill to upgrade to",
            "Omnitect Reactor Plant)",
        ]);
        assert_eq!(got.architect_name, "Xipocado Royal");
        assert_eq!(got.kind, OfferKind::Upgrade);
        assert_eq!(got.printed_target, "Omnitect Reactor Plant");
    }

    // An architect line whose offer never arrived — the OCR pass cut the panel
    // short, or the wrap was lost — is ordinary text, not a block. Nothing
    // closed it, so skipping its lines would lose whatever the panel printed
    // next. Fails if an unparsed, unclosed run is treated as block text.
    #[test]
    fn an_architect_line_whose_offer_was_never_read_leaves_the_title_alone() {
        assert_eq!(parse_architects(&["Xopec, Architect of Power"]).len(), 0);
        assert_eq!(
            read_panel(&["Xopec, Architect of Power", "Tombs"]).identity_name(),
            Some("Tombs")
        );
    }

    // The other half: a run that DID close its bracket is block text even
    // though it failed to parse — here the offer's middle line was dropped, so
    // there is no kill clause to parse but `Omnitect Reactor Plant` is still
    // the offer's target and must not be read as the room the player is in.
    // Fails if only parsed blocks are skipped.
    #[test]
    fn a_closed_block_that_failed_to_parse_still_hides_its_target() {
        let lines = [
            "Xipocado, Royal Architect",
            "Omnitect Reactor Plant)",
            "Lightning Workshop",
        ];
        assert_eq!(parse_architects(&lines).len(), 0, "no kill clause to parse");
        assert_eq!(read_panel(&lines).identity_name(), Some("Lightning Workshop"));
    }

    // …and the bracket only counts while the run is still offer-shaped. A
    // measured offer wraps over at most three lines (MAX_BLOCK_LINES), so a run
    // that reaches a fourth has lost its own close and latched onto a later
    // one — the runaway that swallows every line between. Fails if the length
    // bound is dropped: the run then closes on `Storage Room)` and takes the
    // title with it.
    #[test]
    fn a_run_that_latches_onto_a_later_bracket_does_not_swallow_the_title() {
        let lines = [
            "Xopec, Architect of Power",
            "Tombs",
            "9 Incursions Remaining",
            "Enter Incursion",
            "Storage Room)",
        ];
        assert_eq!(read_panel(&lines).identity_name(), Some("Tombs"));
    }

    // The layout panel's plates are OCR'd into the same line list as the side
    // panel, so a plate name reaches `read_panel` above the real title. The
    // room is `Tombs` because the title is the vocabulary line above the first
    // parsed architect block — position, not score, and not reading order.
    //
    // Fails if the title reverts to the first vocabulary line: the plate name
    // is first and wins. (`Museum of Artefacts` opens no block of its own —
    // ARCHITECT_KEYWORD is what stops that, covered above.)
    #[test]
    fn a_plate_name_before_the_title_does_not_become_the_room() {
        let mut lines = vec!["Museum of Artefacts"];
        lines.extend(CASE_1);
        assert_eq!(read_panel(&lines).identity_name(), Some("Tombs"));
    }

    // Both halves are required: a line that names an architect but no kill
    // clause, and a kill clause with no architect, are each a `None` rather
    // than a half-filled offer. Fails if either gate is dropped.
    #[test]
    fn a_line_that_is_not_a_whole_architect_offer_parses_to_nothing() {
        for line in [
            "Tombs",
            "9 Incursions Remaining",
            "Enter Incursion",
            "Ticaba, Architect of the Arena",
            "(Kill to change to Storage Room)",
            "Architect",
            "",
        ] {
            assert_eq!(
                parse_architect_block(line),
                None,
                "{line:?} is not an offer"
            );
        }
    }

    // Both architects come back, in reading order, from the whole panel — the
    // second block must not be swallowed by the first.
    #[test]
    fn a_whole_panel_yields_both_architects_in_reading_order() {
        let got = parse_architects(&CASE_1);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].architect_name, "Ticaba");
        assert_eq!(got[0].printed_target, "Storage Room");
        assert_eq!(got[1].architect_name, "Juatalotli");
        assert_eq!(got[1].printed_target, "Sparring Room");
        assert!(got.iter().all(|o| o.kind == OfferKind::Change));
    }

    // The gotcha, end to end from panel text: Case 5's kill on a TIER-1
    // Poison Garden. The panel says "Shrine of Empowerment"; the player gets
    // Sanctum of Unity II, which is what makes Doryani certain there.
    #[test]
    fn a_parsed_offer_resolves_to_the_room_that_is_actually_built() {
        let got = offer(&["Tacati, Architect of Toxins (Kill to change to Shrine of Empowerment)"]);
        assert_eq!(got.printed_target, "Shrine of Empowerment");
        let built = super::super::rooms::resolve_offer(&got.printed_target, Tier::T1)
            .expect("the upgrade line");
        assert_eq!(built.display_name, "Sanctum of Unity");
        assert_eq!(built.built_tier, Tier::T2);
    }

    // -------------------------------------------- grouping by geometry --

    // The 2026-09-03 PC board, with the wrap emitted out of order — the shape
    // that produced advice for a board with one architect on it. Ordering by
    // the boxes puts `Empowerment)` back under its own `(Kill to change to
    // Shrine of` line, so the second offer names the room the panel printed.
    //
    // Fails if `architect_blocks` walks the caller's order instead of
    // `reading_order`: the continuation is then read before the block that
    // needs it, dropped, and the target truncates at the wrap.
    #[test]
    fn a_wrap_emitted_before_its_own_architect_line_is_grouped_by_its_box() {
        let got = parse_architects(&pc_panel_engine_order());

        assert_eq!(got.len(), 2, "the panel prints two offers: {got:?}");
        assert_eq!(got[0].architect_name, "Quipolatl");
        assert_eq!(got[0].printed_target, "Armoury");
        assert_eq!(got[1].architect_name, "Atmohua");
        assert_eq!(
            got[1].printed_target, "Shrine of Empowerment",
            "the wrapped tail belongs to Atmohua's offer",
        );
    }

    // The same six lines with no boxes: the fallback is the caller's order,
    // and this is what that costs on this input. `Empowerment)` arrives while
    // no block is open, so it is dropped, and Atmohua's offer ends at the wrap
    // — a target of `Shrine of`, which resolves to nothing.
    //
    // Documented rather than fixed: without a box there is no evidence the
    // stray line belongs to the block that follows it. Fails if reordering is
    // applied to a boxless list, which would be a guess dressed as geometry.
    #[test]
    fn the_same_lines_without_boxes_keep_the_engine_order_and_truncate_the_wrap() {
        let got = parse_architects(&texts(&pc_panel_engine_order()));

        assert_eq!(got.len(), 2);
        assert_eq!(got[1].architect_name, "Atmohua");
        assert_eq!(
            got[1].printed_target, "Shrine of",
            "the offer ends at the wrap when nothing places the tail",
        );
        assert_eq!(
            got[1].target,
            Match::Unknown,
            "and a truncated target resolves to no room at all",
        );
    }

    // A line that is nowhere near the open block does not join it, even when
    // it arrives directly after it. This is the other half of the geometry
    // rule: sequence alone would sweep the budget line into Atmohua's offer
    // and take `9 Incursions Remaining` for the target's second half.
    //
    // Fails if `continues` is dropped, or if its vertical bound is widened
    // past the gap the panel leaves between blocks.
    #[test]
    fn a_line_far_below_the_open_block_is_not_swept_into_it() {
        let mut lines = pc_panel();
        // The budget line the layout panel prints, a long way under the side
        // panel and to the left of it — a different column and a different
        // block.
        lines.remove(5);
        lines.push(boxed("9 Incursions Remaining", 900, 700));

        let got = parse_architects(&lines);

        assert_eq!(got.len(), 2);
        assert_eq!(
            got[1].printed_target, "Shrine of",
            "the offer is truncated because its own wrap is missing, not extended by the budget line",
        );
        assert_eq!(
            parse_incursions_remaining(&lines),
            Some(9),
            "and the budget line is still ordinary text, still read",
        );
    }

    // The failure the FIRST cut of this grouping introduced, and the reason a
    // refused line is skipped rather than treated as a block boundary.
    //
    // POE-230's right margin on the panel ROI (20 ref px, deliberately tight)
    // admits the map's own info block at the panel's right edge. In reading
    // order that fragment sorts by its top INTO the middle of an offer — after
    // the architect line, before the `(Kill to …` clause. Closing the block
    // there drops the clause, so the offer never parses and the architect is
    // lost: the exact failure POE-243 exists to remove, arriving through the
    // fix for it.
    //
    // Fails if a refused line closes the run instead of being stepped over.
    #[test]
    fn a_foreign_line_between_two_lines_of_an_offer_is_skipped_not_a_boundary() {
        let lines = laptop_panel_with_map_fragment();

        let got = parse_architects(&lines);

        assert_eq!(got.len(), 2, "the fragment costs no offer: {got:?}");
        assert_eq!(got[0].architect_name, "Hayoxi");
        assert_eq!(
            got[0].printed_target, "Omnitect Reactor Plant",
            "the clause below the fragment still belongs to Hayoxi's block",
        );
        assert_eq!(got[1].architect_name, "Xopec");
        assert_eq!(got[1].printed_target, "Royal Meeting Room");
    }

    // The bracket half of the same rule: a `)` on a REFUSED line does not close
    // the block.
    //
    // The sibling test above uses a fragment with no bracket in it, so it
    // exercises only the "not a boundary" half — the block survives because
    // nothing asked it to end. The map's info block prints brackets of its own
    // (`Area Level: 68 (Merciless)` is one line of it), and a `)` read off THAT
    // is a claim about the map panel, not about the offer. Closing on it drops
    // the `(Kill to …` clause below, so Hayoxi's block never parses and the
    // architect is lost — the same POE-243 failure by the other door.
    //
    // Fails if the geometric `attaches` filter is dropped — which is the change
    // this guards, because it is the one a reader would call a simplification.
    // Without it the fragment ATTACHES, its `)` closes Hayoxi's run at
    // `Hayoxi, Architect of Destruction Area Level: 68 (Merciless)`, that text
    // parses to no offer, the `(Kill to …` clause below is left with nothing
    // open to join, and `got.len() == 1` with only `Xopec` in it.
    //
    // (Moving the close check out of the attached branch fails this too, by the
    // same arithmetic. It is named second because nobody writes that edit by
    // accident — the branch carries a comment saying why it is there.)
    #[test]
    fn a_bracket_on_a_refused_line_does_not_close_the_block() {
        let mut lines = laptop_panel_with_map_fragment();
        // The same fragment, at the same place, now ending in a bracket.
        lines[3] = OcrLineBox { h: 12, ..boxed("Area Level: 68 (Merciless)", 1659, 130) };

        let got = parse_architects(&lines);

        assert_eq!(got.len(), 2, "the fragment's bracket costs no offer: {got:?}");
        assert_eq!(got[0].architect_name, "Hayoxi");
        assert_eq!(
            got[0].printed_target, "Omnitect Reactor Plant",
            "the clause below the bracket still belongs to Hayoxi's block",
        );
    }

    // …and the skipped line is not in the block either: the rect stops at the
    // panel column, 1704, and does not reach the fragment's far edge at 1771.
    // A surface pointing at this block would otherwise cover a strip of the
    // map's info block as well.
    //
    // Fails if a refused line is swept into the union — which is what the
    // sequence-only path does, and is the one thing geometry buys over it on
    // this input.
    #[test]
    fn a_skipped_line_is_left_out_of_the_blocks_rect() {
        let lines = laptop_panel_with_map_fragment();

        let got = parse_architects(&lines);

        // The four attached lines: tops 115..154, the widest right edge 1704.
        assert_eq!(got[0].rect, Some([1480, 115, 224, 51]));
    }

    // Two boxes on ONE visual row are ordered left to right even when their
    // tops disagree. Glyph boxes: a half-row with no ascender starts a pixel
    // lower than its neighbour, and ordering on the raw top then reads the
    // plate as `Champions Hall of` — 0.60 against the vocabulary, which is an
    // unread plate and junk to the advisor.
    //
    // Fails if `reading_order` sorts on the top alone.
    #[test]
    fn two_boxes_on_one_row_are_ordered_left_to_right_despite_a_jittered_top() {
        let img = blank(1374, 862);
        let lattice = Lattice::new((673, 682), 0.99);
        // The right half sits ONE px higher, which is enough to invert a
        // top-only sort and is well inside one line height.
        let jittered = Canned {
            name: vec![boxed("Champions", 200, 99), boxed("Hall of", 100, 100)],
            numeral: Vec::new(),
        };

        let board = read_board(&jittered, &img, &lattice, &|| false);

        assert!(board
            .iter()
            .all(|r| r.identity.identity().map(|id| id.display_name())
                == Some("Hall of Champions")));
        assert_eq!(
            super::super::rooms::match_room_name("Champions Hall of"),
            Match::Unknown,
            "the other order really is unreadable",
        );
    }

    // The temple screen prints furniture much taller than a text line — the
    // `Enter Incursion` button is nearly twice one — and a tall candidate must
    // not widen the gap it is allowed to sit at. 43 px below a 20 px architect
    // line is 2.15 of that line's height: over the 30 px `CONTINUATION_PITCH`
    // allows, and under the 52.5 px it would allow if the BUTTON's own 35 px
    // set the pitch.
    //
    // Fails if `continues` takes the taller of the two heights: the button's
    // text is then appended to an open block, where it can be read as the
    // target the `(Kill to …` clause never delivered.
    #[test]
    fn a_tall_line_below_an_open_block_does_not_widen_the_gap_it_may_sit_at() {
        let lines = vec![
            boxed("Xopec, Architect of Power", 1480, 310),
            OcrLineBox { h: 35, ..boxed("Enter Incursion", 1480, 353) },
        ];

        assert_eq!(
            group_architect_blocks(&lines),
            vec!["Xopec, Architect of Power".to_string()],
            "the button is screen furniture, not this offer's second line",
        );
    }

    // ------------------------------------------------------ block rects --

    // The rect a block publishes is the union of the boxes of the lines it was
    // built from — computed here from the fixture's own numbers rather than
    // from the function. Atmohua's block spans three lines: x from the widest,
    // y from the first line's top to the last line's bottom.
    //
    // Fails if the union takes the first line's box, the last line's box, or
    // the intersection.
    #[test]
    fn a_blocks_rect_is_the_union_of_its_lines_boxes() {
        let lines = pc_panel();
        let got = parse_architects(&lines);

        let top = lines[3].y;
        let bottom = lines[5].y + lines[5].h;
        let widest = lines[3].w.max(lines[4].w).max(lines[5].w);
        assert_eq!(
            got[1].rect,
            Some([lines[3].x, top, widest, bottom - top]),
            "Atmohua's block covers its three lines",
        );

        // A single-line block is its own line's box, which is the boundary the
        // union arithmetic has to get right too.
        assert_eq!(
            got[0].rect,
            Some([
                lines[1].x,
                lines[1].y,
                lines[1].w.max(lines[2].w),
                lines[2].y + lines[2].h - lines[1].y,
            ]),
            "Quipolatl's block covers its two",
        );
    }

    // A text-only read publishes no rect rather than a made-up one: there is
    // nothing on a `&str` that says where the panel drew it. Fails if the rect
    // defaults to a zero box, which a surface would draw at the screen origin.
    #[test]
    fn a_block_read_from_text_alone_publishes_no_rect() {
        assert!(parse_architects(&CASE_1)
            .iter()
            .all(|offer| offer.rect.is_none()));
    }

    // The title's own box, for the surface that must not cover it (POE-244).
    // It is the box of the ONE line the title rule picked — the room name —
    // and not of the architect block above or below it.
    //
    // Fails if `read_panel` publishes the first boxed line's rect, or the
    // union of everything it looked at.
    #[test]
    fn the_panel_title_carries_the_box_of_the_line_it_was_read_from() {
        let lines = pc_panel();

        let got = read_panel(&lines);

        assert_eq!(got.identity_name(), Some("Armourer's Workshop"));
        assert_eq!(
            got.room_rect,
            Some([lines[0].x, lines[0].y, lines[0].w, lines[0].h]),
        );
    }

    // An unread title has no box to publish. Fails if `room_rect` is filled in
    // from whichever line was looked at last.
    #[test]
    fn an_unread_title_publishes_no_box() {
        let got = read_panel(&[boxed("Enter Incursion", 1300, 100)]);

        assert_eq!(got.room, Match::Unknown);
        assert_eq!(got.room_rect, None);
    }

    // ------------------------------------------- the coordinate spaces --

    // `preprocess_for_ocr` upscales 2×, so every box `recognize_lines` reports
    // is at twice the crop's coordinates. The expected numbers here are the
    // definition — halve the origin, halve the size — not a second call to the
    // function.
    //
    // Fails if the descale is dropped (boxes at 2× the truth), applied twice,
    // or applied to the origin only.
    #[test]
    fn an_ocr_box_is_halved_out_of_the_2x_preprocessed_space() {
        let got = descaled(
            OcrLineBox { text: "Tombs".to_string(), x: 240, y: 500, w: 160, h: 40 },
            2,
        );

        assert_eq!((got.x, got.y, got.w, got.h), (120, 250, 80, 20));
        assert_eq!(got.text, "Tombs");
    }

    // The rounding direction, at the boundary: an odd box must come back
    // covering the pixel it started on and the pixel it ended on, so the
    // origin rounds DOWN and the size rounds UP. A surface that must not cover
    // OCR text can afford a box a pixel too big and cannot afford one a pixel
    // too small.
    //
    // Fails if the size truncates: 41/2 would be 20, and the box would end one
    // px short of the glyphs.
    #[test]
    fn an_odd_ocr_box_rounds_outward_rather_than_truncating() {
        let got = descaled(
            OcrLineBox { text: "Tombs".to_string(), x: 241, y: 501, w: 161, h: 41 },
            2,
        );

        assert_eq!((got.x, got.y), (120, 250), "the origin rounds down");
        assert_eq!((got.w, got.h), (81, 21), "the size rounds up");
    }

    // The case a size-only ceiling gets wrong: an ODD origin with an EVEN size.
    // `x = 1, w = 2` covers columns 1 and 2 of the 2× image, which is columns 0
    // and 1 of the source — so the answer is `x = 0, w = 2`. Scaling the size
    // on its own gives `w = 1` and drops the far column, which is the pixel the
    // outward rounding exists to keep.
    //
    // Fails if the size is derived from `w` rather than from the far edge
    // `x + w`.
    #[test]
    fn an_odd_origin_with_an_even_size_keeps_its_far_edge() {
        let got = descaled(
            OcrLineBox { text: "I".to_string(), x: 1, y: 3, w: 2, h: 2 },
            2,
        );

        assert_eq!((got.x, got.y), (0, 1));
        assert_eq!(
            (got.w, got.h),
            (2, 2),
            "the box must still reach the source pixel its far edge covered",
        );
    }

    // The second step: a crop's own pixels are not the capture's. The origin
    // added here is the crop's CLIPPED top-left corner, so a box read at (12,
    // 8) inside a panel crop taken at (1288, 92) sits at (1300, 100) on screen
    // — which is where `pc_panel` says the title is.
    //
    // Fails if the translate is dropped (every rect at the crop's origin, i.e.
    // the top-left of the screen for a full grab), or if it subtracts.
    #[test]
    fn a_crop_relative_box_moves_to_capture_px_by_the_crops_origin() {
        let got = translated(
            OcrLineBox { text: "Armourer's Workshop".to_string(), x: 12, y: 8, w: 152, h: 20 },
            (1288, 92),
        );

        assert_eq!((got.x, got.y), (1300, 100));
        assert_eq!((got.w, got.h), (152, 20), "a translate does not resize");
    }

    // --------------------------------------------- incursions remaining --

    // Both observed orderings, the singular, and the merged token Windows OCR
    // produces when the digit touches the word.
    #[test]
    fn the_incursion_budget_parses_in_every_observed_shape() {
        assert_eq!(
            parse_incursions_remaining(&["9 Incursions Remaining"]),
            Some(9)
        );
        assert_eq!(
            parse_incursions_remaining(&["Incursions Remaining: 9"]),
            Some(9)
        );
        assert_eq!(
            parse_incursions_remaining(&["1 Incursion Remaining"]),
            Some(1)
        );
        assert_eq!(
            parse_incursions_remaining(&["12 Incursions Remaining"]),
            Some(12)
        );
        assert_eq!(
            parse_incursions_remaining(&["9Incursions Remaining"]),
            Some(9)
        );
        assert_eq!(
            parse_incursions_remaining(
                &CASE_1
                    .iter()
                    .chain(["9 Incursions Remaining"].iter())
                    .collect::<Vec<_>>()
            ),
            Some(9)
        );
    }

    // OCR confusions in the label and in the digit. `lncursions` is the
    // canonical capital-I failure on this font; `1O` is the zero one.
    #[test]
    fn the_incursion_budget_survives_the_common_ocr_confusions() {
        assert_eq!(
            parse_incursions_remaining(&["9 lncursions Remaining"]),
            Some(9)
        );
        assert_eq!(
            parse_incursions_remaining(&["9 Incursions Remaininq"]),
            Some(9)
        );
        assert_eq!(
            parse_incursions_remaining(&["1O Incursions Remaining"]),
            Some(10)
        );
    }

    // No label, no number, and the deliberate limitation: a count with no
    // surviving ASCII digit is rejected rather than guessed.
    #[test]
    fn a_line_without_both_a_label_and_a_count_yields_nothing() {
        assert_eq!(parse_incursions_remaining(&CASE_1), None);
        assert_eq!(parse_incursions_remaining(&["Incursions Remaining"]), None);
        assert_eq!(parse_incursions_remaining(&["9 Incursions"]), None);
        assert_eq!(parse_incursions_remaining(&["Enter Incursion"]), None);
        assert_eq!(parse_incursions_remaining(&["l Incursion Remaining"]), None);
        assert_eq!(parse_incursions_remaining::<&str>(&[]), None);
    }

    // The digit folding is aggressive enough that `so` would fold to 50, so a
    // token only counts as a number if it still carries a real ASCII digit.
    // Fails if that guard is dropped: `so` precedes the count here.
    #[test]
    fn a_word_that_folds_to_digits_is_not_read_as_the_count() {
        assert_eq!(
            parse_incursions_remaining(&["so 9 Incursions Remaining"]),
            Some(9)
        );
        assert_eq!(parse_incursions_remaining(&["so many rooms"]), None);
    }

    // ---------------------------------------------------- panel reading --

    // The title is the room the player is standing in — NOT the best-scoring
    // room name on the panel. Case 1 is the discriminating board: it prints
    // `Tombs`, `Storage Room` and `Sparring Room`, and the last two are
    // longer, cleaner vocabulary hits.
    #[test]
    fn the_panel_title_is_the_current_room_not_an_architect_target() {
        let got = read_panel(&CASE_1);
        assert_eq!(
            got.room.identity(),
            Some(RoomIdentity::Filler("Tombs")),
            "the title must be Tombs"
        );
        assert_eq!(got.architects.len(), 2);
        assert_eq!(got.incursions_remaining, None);

        // The same panel with the title read last — an architect block's
        // wrapped tail is a bare vocabulary name on its own line, and taking
        // the first match without skipping blocks would return it.
        let reordered = [
            "Xipocado, Royal Architect",
            "(Kill to upgrade to",
            "Omnitect Reactor Plant)",
            "Tombs",
        ];
        assert_eq!(
            read_panel(&reordered).identity_name(),
            Some("Tombs"),
            "an architect's target must not be mistaken for the panel title"
        );
    }

    #[test]
    fn a_whole_screen_read_yields_room_architects_and_budget_together() {
        let mut lines: Vec<&str> = LIVE.to_vec();
        lines.push("9 Incursions Remaining");
        let got = read_panel(&lines);
        assert_eq!(
            got.room.identity().map(|id| id.display_name()),
            Some("Lightning Workshop")
        );
        assert_eq!(got.room.identity().map(|id| id.tier()), Some(Tier::T1));
        assert_eq!(got.architects.len(), 2);
        assert_eq!(got.architects[0].kind, OfferKind::Upgrade);
        assert_eq!(got.architects[1].kind, OfferKind::Change);
        assert_eq!(got.incursions_remaining, Some(9));
    }

    // ------------------------------------------------ screen furniture --

    /// The window header as OCR mangles it. Each is one edit from the real
    /// string and each scored over both `rooms::MATCH` and `rooms::LEAD`
    /// against `Apex of Atzoatl` — 0.9190, 0.9056 and 0.8931 — so each was a
    /// confident, wrong Apex before the stop-list.
    const HEADER_SLIPS: [&str; 3] = [
        "tample of atzoatl",
        "aemple of atzoatl",
        "tmple of atzoatl",
    ];

    // The header is furniture in its clean form and through the slips that
    // reach the Apex, and the nearest real room name is NOT. Fails if
    // SCREEN_FURNITURE is emptied, or if its gate is loosened far enough to
    // swallow `Apex of Atzoatl` (0.8515 away).
    #[test]
    fn the_window_header_is_screen_furniture_and_the_apex_is_not() {
        assert!(is_screen_furniture("Temple of Atzoatl"));
        for slip in HEADER_SLIPS {
            assert!(is_screen_furniture(slip), "{slip:?} is the header");
        }
        for room in ["Apex of Atzoatl", "Temple Nexus", "Tombs", ""] {
            assert!(!is_screen_furniture(room), "{room:?} is not furniture");
        }
    }

    // The regression, end to end: the header is printed above the side panel
    // on every capture, so it reaches `read_panel` before the real title.
    #[test]
    fn the_window_header_never_becomes_the_panel_title() {
        for header in ["Temple of Atzoatl"].iter().chain(HEADER_SLIPS.iter()) {
            let mut lines = vec![*header];
            lines.extend(CASE_1);
            assert_eq!(
                read_panel(&lines).identity_name(),
                Some("Tombs"),
                "{header:?} was read as the title"
            );
        }
    }

    // Standing in the Apex is a real board state, and its title reads
    // exactly. Fails if the fixed-slot rule refuses more than fuzzy reads.
    #[test]
    fn an_exactly_read_apex_title_is_accepted() {
        assert_eq!(
            read_panel(&["Apex of Atzoatl"]).room,
            Match::Exact(resolve_name("Apex of Atzoatl").expect("in vocabulary"))
        );
    }

    // …but a fuzzy one is not. The basin around `Apex of Atzoatl` is deep
    // enough that a single mis-read character still clears both vocabulary
    // gates, so the title takes these two names on an exact read only. Fails
    // if the fixed-slot rule is dropped: `Apex of Atzoat1` scores 0.9733.
    #[test]
    fn a_fuzzily_read_apex_title_is_refused() {
        assert!(matches!(
            rooms::match_room_name("Apex of Atzoat1"),
            Match::Fuzzy(RoomIdentity::Apex, _)
        ));
        assert_eq!(read_panel(&["Apex of Atzoat1"]).room, Match::Unknown);
    }

    // The same rule on the other fixed slot.
    #[test]
    fn a_fuzzily_read_entrance_title_is_refused() {
        assert!(matches!(
            rooms::match_room_name("Entrence"),
            Match::Fuzzy(RoomIdentity::Entrance, _)
        ));
        assert_eq!(read_panel(&["Entrence"]).room, Match::Unknown);
    }

    // Nothing legible is Unknown, not a guess.
    #[test]
    fn a_panel_of_junk_reads_as_unknown() {
        let got = read_panel(&["", "Enter Incursion", "Vaal Outpost"]);
        assert_eq!(got.room, Match::Unknown);
        assert!(got.architects.is_empty());
        assert_eq!(got.incursions_remaining, None);
    }

    // -------------------------------------------------- plate geometry --

    /// The reference board, anchored where [`super::super::reader`]'s own
    /// fixture records: `board-ref-1374.png`, origin `(673, 494)`, scale 0.99.
    /// That makes the plates 170 × 82 px.
    fn reference_board() -> (DynamicImage, Lattice) {
        let path = format!(
            "{}/tests/fixtures/temple/board-ref-1374.png",
            env!("CARGO_MANIFEST_DIR")
        );
        let img = image::open(&path).unwrap_or_else(|e| panic!("{path} loads: {e}"));
        (img, Lattice::new((673, 494), 0.99))
    }

    /// Runs of consecutive rows of `band` that carry glyphs, as inclusive
    /// `(first, last)` pairs of row indices **within the band**.
    ///
    /// A row counts as text when between 6% and 60% of its pixels are brighter
    /// than 90 on `max(r, g, b)`. Both bounds are measured on this board: a
    /// name row covers 10–24% of the plate width, the art between the name
    /// lines covers 0.6%, and the plate's own bottom border — which is inside
    /// the band and is not text — covers 98–100%.
    fn text_rows(img: &DynamicImage, band: [i32; 4]) -> Vec<(usize, usize)> {
        let [x, y, w, h] = band;
        let rgb = img.to_rgb8();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for row in 0..h as usize {
            let bright = (0..w)
                .filter(|dx| {
                    let px = rgb.get_pixel((x + dx) as u32, (y + row as i32) as u32).0;
                    px[0].max(px[1]).max(px[2]) > 90
                })
                .count() as f64
                / w as f64;
            let is_text = (0.06..0.60).contains(&bright);
            match runs.last_mut() {
                Some(last) if is_text && last.1 + 1 == row => last.1 = row,
                _ if is_text => runs.push((row, row)),
                _ => {}
            }
        }
        runs
    }

    // Where the name band actually lands on a real board, rather than that it
    // lands somewhere inside the plate.
    //
    // Hand-measured on `board-ref-1374.png` at origin (673, 494) / scale 0.99,
    // as **plate-relative** rows of the 82 px plate:
    //
    // | slot | plate | name rows | quiet |
    // |---|---|---|---|
    // | E0 | `Gemcutter's Workshop`, wrapped | 46–53, 63–70 | 38–43 under the numeral, 54–60 between the lines |
    // | C2 | `Chamber of Iron`, one line | 62–69 | 39–59 |
    //
    // `NAME_TOP` = 0.48 puts the band's first row at plate row 39, seven rows
    // clear of the wrapped name's first line. Fails on any drift that clips a
    // name line or starts the band mid-glyph — 0.62, for instance, opens the
    // band at plate row 51 and cuts `Gemcutter's` in half.
    #[test]
    fn the_name_band_opens_above_a_wrapped_name_and_holds_both_its_lines() {
        let (img, lattice) = reference_board();
        assert_eq!(
            text_rows(&img, name_strip(&lattice, Slot::E0)),
            vec![(7, 14), (24, 31)],
            "Gemcutter's Workshop wraps over two lines, 7 and 24 rows into the band"
        );
        assert_eq!(
            text_rows(&img, name_strip(&lattice, Slot::C2)),
            vec![(23, 30)],
            "Chamber of Iron is a single line, 23 rows into the band"
        );
    }

    // The two crops must sit inside the plate and must not overlap: a numeral
    // swept into the name band changes the string the matcher sees.
    #[test]
    fn the_name_and_numeral_crops_sit_inside_the_plate_and_do_not_overlap() {
        let lattice = Lattice::new((673, 682), 0.99);
        for slot in Slot::ALL {
            let (cx, cy) = lattice.centre(slot);
            let (hw, hh) = lattice.plate_half();
            let [nx, ny, nw, nh] = name_strip(&lattice, slot);
            let [qx, qy, qw, qh] = numeral_box(&lattice, slot);
            assert!(nx >= cx - hw && nx + nw <= cx + hw, "{slot:?} name x");
            assert!(ny >= cy - hh && ny + nh <= cy + hh, "{slot:?} name y");
            assert!(qx >= cx - hw && qx + qw <= cx + hw, "{slot:?} numeral x");
            assert!(qy >= cy - hh && qy + qh <= cy + hh, "{slot:?} numeral y");
            assert!(
                qy + qh <= ny,
                "{slot:?}: the numeral box reaches into the name band"
            );
            assert!(nw > 0 && nh > 0 && qw > 0 && qh > 0, "{slot:?} empty crop");
        }
    }

    // -------------------------------------------------------- the seam --

    /// A recogniser that answers with fixed lines, so the board walk can be
    /// exercised without an OCR engine. The numeral crop is far narrower than
    /// the name band, which is how the double tells the two calls apart.
    struct Canned {
        name: Vec<OcrLineBox>,
        numeral: Vec<OcrLineBox>,
    }

    impl Canned {
        fn name(text: &str) -> Canned {
            Canned {
                name: vec![boxed(text, 0, 0)],
                numeral: Vec::new(),
            }
        }
    }

    impl TextRecognizer for Canned {
        fn recognize(&self, img: &DynamicImage) -> Result<Vec<OcrLineBox>, String> {
            Ok(if img.width() < 60 {
                self.numeral.clone()
            } else {
                self.name.clone()
            })
        }
    }

    struct Broken;

    impl TextRecognizer for Broken {
        fn recognize(&self, _img: &DynamicImage) -> Result<Vec<OcrLineBox>, String> {
            Err("no OCR engine".to_string())
        }
    }

    fn blank(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::new(w, h))
    }

    // A wrapped name arrives as two OCR lines and has to be joined before
    // matching: neither half is a vocabulary name on its own.
    #[test]
    fn a_wrapped_plate_name_is_joined_before_matching() {
        let img = blank(1374, 862);
        let lattice = Lattice::new((673, 682), 0.99);
        let recognizer = Canned {
            name: vec![boxed("Gemcutter's", 0, 0), boxed("Workshop", 0, LINE_H)],
            numeral: Vec::new(),
        };
        let board = read_board(&recognizer, &img, &lattice, &|| false);
        assert_eq!(board.len(), 13, "the board always has 13 slots");
        assert!(board
            .iter()
            .all(|r| r.identity.identity().map(|id| id.display_name())
                == Some("Gemcutter's Workshop")));
        // …and neither line alone would have got there.
        assert_eq!(
            super::super::rooms::match_room_name("Gemcutter's"),
            Match::Unknown
        );
    }

    // …and joined in the order the plate DREW them, not the order the engine
    // emitted them (POE-243). `Workshop Gemcutter's` scores 0.60 against the
    // vocabulary entry — under `rooms::MATCH` — so an engine that returns the
    // tail first turns a read plate into an unread one, and an unread plate is
    // junk to the advisor.
    //
    // Fails if `read_plate` joins `lines` as handed over.
    #[test]
    fn a_wrapped_plate_name_is_joined_top_line_first_whatever_order_ocr_returned() {
        let img = blank(1374, 862);
        let lattice = Lattice::new((673, 682), 0.99);
        let reversed = Canned {
            name: vec![boxed("Workshop", 0, LINE_PITCH), boxed("Gemcutter's", 0, 0)],
            numeral: Vec::new(),
        };

        let board = read_board(&reversed, &img, &lattice, &|| false);

        assert!(board
            .iter()
            .all(|r| r.identity.identity().map(|id| id.display_name())
                == Some("Gemcutter's Workshop")));
        // …and the other join really would have missed.
        assert_eq!(
            super::super::rooms::match_room_name("Workshop Gemcutter's"),
            Match::Unknown,
        );
    }

    // A failed OCR call is Unknown, never a dropped slot: the board always has
    // 13 slots and a missing one would silently change the graph POE-170
    // scores.
    #[test]
    fn a_plate_whose_ocr_call_fails_reads_unknown() {
        let img = blank(1374, 862);
        let lattice = Lattice::new((673, 682), 0.99);
        let broken = read_board(&Broken, &img, &lattice, &|| false);
        assert_eq!(
            broken.iter().map(|r| r.slot).collect::<Vec<_>>(),
            Slot::ALL.to_vec()
        );
        assert!(broken.iter().all(|r| r.identity == Match::Unknown));
    }

    // Same for a plate whose crop falls outside the capture — an origin near
    // the left edge pushes the row-D plates off-image.
    #[test]
    fn a_plate_cropped_outside_the_capture_reads_unknown() {
        let img = blank(1374, 862);
        let offscreen = Lattice::new((60, 682), 0.99);
        let clipped = read_board(&Canned::name("Tombs"), &img, &offscreen, &|| false);
        assert_eq!(
            clipped.iter().map(|r| r.slot).collect::<Vec<_>>(),
            Slot::ALL.to_vec(),
            "every slot is still reported"
        );
        assert!(
            clipped.iter().any(|r| r.identity == Match::Unknown),
            "a plate outside the capture must read Unknown"
        );
    }

    // A stop signal lands BETWEEN plate crops, not after all 26 OCR calls.
    // The recogniser counts its calls, so this fails if `should_stop` is only
    // polled once before the walk (13 plates read) or never (26 calls).
    #[test]
    fn a_stop_signal_ends_the_board_walk_at_the_next_plate() {
        use std::cell::Cell;

        struct Counting {
            calls: Cell<usize>,
        }
        impl TextRecognizer for Counting {
            fn recognize(&self, _img: &DynamicImage) -> Result<Vec<OcrLineBox>, String> {
                self.calls.set(self.calls.get() + 1);
                Ok(vec![boxed("Tombs", 0, 0)])
            }
        }

        let img = blank(1374, 862);
        let lattice = Lattice::new((673, 682), 0.99);
        let recognizer = Counting { calls: Cell::new(0) };
        // Two OCR calls per plate, so this stops after the third plate.
        let stop_after = 6;
        let board = read_board(&recognizer, &img, &lattice, &|| {
            recognizer.calls.get() >= stop_after
        });

        assert_eq!(
            recognizer.calls.get(),
            stop_after,
            "the walk must stop at the next plate boundary, not run to 26 calls",
        );
        assert_eq!(board.len(), 13, "a stopped read still reports 13 slots");
        assert_eq!(
            board.iter().filter(|r| r.identity.is_known()).count(),
            3,
            "exactly the plates read before the stop carry an identity",
        );
        assert!(
            board[3..].iter().all(|r| r.identity == Match::Unknown),
            "the slots the walk never reached are Unknown, never guessed",
        );
    }

    // The numeral cross-check runs on the real crop path: a plate whose name
    // and numeral disagree is demoted.
    #[test]
    fn a_plate_whose_numeral_contradicts_its_name_reads_unknown() {
        let img = blank(1374, 862);
        let lattice = Lattice::new((673, 682), 0.99);
        // The numeral crop answers "III" on every plate, so a tier-1 name is
        // contradicted while a tier-3 one is confirmed.
        let iii = |name: &str| Canned {
            name: vec![boxed(name, 0, 0)],
            numeral: vec![boxed("III", 0, 0)],
        };
        let contradicted = read_board(&iii("Corruption Chamber"), &img, &lattice, &|| false);
        assert!(contradicted.iter().all(|r| r.identity == Match::Unknown));
        let agreed = read_board(&iii("Locus of Corruption"), &img, &lattice, &|| false);
        assert!(agreed
            .iter()
            .all(|r| r.identity.identity().map(|id| id.display_name())
                == Some("Locus of Corruption")));
    }
}

