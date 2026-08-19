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
//! `N Incursions Remaining` is printed at the bottom of the *layout* panel,
//! not the side panel, which is why [`read_panel`] takes every line of the
//! window rather than a side-panel crop.

// POE-169's surface is reached only by its own tests until POE-170 (the
// advisor) and POE-171 (the overlay) call in — see `rooms` for the same note.
#![allow(dead_code)]

use image::DynamicImage;
use strsim::jaro_winkler;

use super::lattice::{Lattice, Slot};
use super::rooms::{self, Match, OfferKind, RoomIdentity};

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
const MAX_BLOCK_LINES: usize = 3;

/// One run of OCR lines that opened on an `Architect` word.
struct Block {
    /// Line indices the run covers, inclusive.
    start: usize,
    end: usize,
    /// Whether the run ended on its own closing bracket, rather than at the
    /// next `Architect` line or at the end of the input.
    closed: bool,
    /// The run's lines, joined back into one string.
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
        self.offer.is_some() || (self.closed && self.end - self.start < MAX_BLOCK_LINES)
    }
}

/// Regroup OCR lines into whole architect blocks, keeping each block's span.
///
/// The panel wraps each offer over two or three lines and the two offers sit
/// on opposite sides of the diamond, so they never interleave in reading
/// order. A block runs from an `Architect` line to the line that closes the
/// parenthesis, or to the next `Architect` line if the bracket was never read.
fn architect_blocks<S: AsRef<str>>(lines: &[S]) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut current: Option<(usize, usize, String)> = None;
    let mut close = |current: &mut Option<(usize, usize, String)>, closed: bool| {
        if let Some((start, end, text)) = current.take() {
            out.push(Block {
                start,
                end,
                closed,
                offer: parse_architect_block(&text),
                text,
            });
        }
    };
    for (i, line) in lines.iter().enumerate() {
        let line = line.as_ref();
        if starts_architect(line) {
            close(&mut current, false);
            current = Some((i, i, line.trim().to_string()));
        } else if let Some((_, end, text)) = current.as_mut() {
            *end = i;
            text.push(' ');
            text.push_str(line.trim());
        } else {
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
pub fn group_architect_blocks<S: AsRef<str>>(lines: &[S]) -> Vec<String> {
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
    })
}

/// Every architect offer the OCR lines contain.
pub fn parse_architects<S: AsRef<str>>(lines: &[S]) -> Vec<ArchitectOffer> {
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
pub fn parse_incursions_remaining<S: AsRef<str>>(lines: &[S]) -> Option<u8> {
    for line in lines {
        let words = words(line.as_ref());
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
    /// Both architects, in reading order.
    pub architects: Vec<ArchitectOffer>,
    /// The remaining budget, when the layout panel's footer was legible.
    pub incursions_remaining: Option<u8>,
}

impl PanelReading {
    /// The title's game name, when it was legible.
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
pub fn read_panel<S: AsRef<str>>(lines: &[S]) -> PanelReading {
    let blocks = architect_blocks(lines);
    let spans: Vec<(usize, usize)> = blocks
        .iter()
        .filter(|block| block.is_offer_text())
        .map(|block| (block.start, block.end))
        .collect();
    let title_at = |i: usize| {
        if spans.iter().any(|(start, end)| (*start..=*end).contains(&i)) {
            return Match::Unknown;
        }
        title_match(lines[i].as_ref())
    };

    let above = blocks
        .iter()
        .find(|block| block.offer.is_some())
        .map_or(0, |block| block.start);
    let room = (0..above)
        .rev()
        .map(title_at)
        .find(|m| *m != Match::Unknown)
        .or_else(|| (0..lines.len()).map(title_at).find(|m| *m != Match::Unknown))
        .unwrap_or(Match::Unknown);

    PanelReading {
        room,
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
    /// Recognised lines, top to bottom.
    fn recognize(&self, img: &DynamicImage) -> Result<Vec<String>, String>;
}

/// The production recogniser: the crate's Windows.Media.Ocr binding, through
/// the same [`crate::capture::preprocess_for_ocr`] contrast-and-upscale pass
/// every other capture in the app uses (POE-164).
///
/// No `#[cfg(windows)]` here on purpose — `crate::ocr::recognize_lines` already
/// has a non-Windows arm that returns `Err(UNAVAILABLE)`, so duplicating the
/// gate would only add a second place for the two arms to diverge.
pub struct SystemOcr;

impl TextRecognizer for SystemOcr {
    fn recognize(&self, img: &DynamicImage) -> Result<Vec<String>, String> {
        let prepared = crate::capture::preprocess_for_ocr(img);
        Ok(crate::ocr::recognize_lines(&prepared)?
            .into_iter()
            .map(|line| line.text)
            .collect())
    }
}

/// Read every plate's name off a board.
///
/// A slot whose crop falls outside the capture, or whose OCR call fails, comes
/// back as [`Match::Unknown`] rather than being dropped: the board always has
/// 13 slots and a missing one would silently change the graph POE-170 scores.
pub fn read_board(
    recognizer: &dyn TextRecognizer,
    img: &DynamicImage,
    lattice: &Lattice,
) -> Vec<RoomReading> {
    Slot::ALL
        .into_iter()
        .map(|slot| RoomReading {
            slot,
            identity: read_plate(recognizer, img, lattice, slot),
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
    let joined = lines.join(" ");
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
    lines.iter().find_map(|line| rooms::parse_numeral(line))
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

    /// A recogniser that answers with fixed text, so the board walk can be
    /// exercised without an OCR engine. The numeral crop is far narrower than
    /// the name band, which is how the double tells the two calls apart.
    struct Canned {
        name: Vec<String>,
        numeral: Vec<String>,
    }

    impl Canned {
        fn name(text: &str) -> Canned {
            Canned {
                name: vec![text.to_string()],
                numeral: Vec::new(),
            }
        }
    }

    impl TextRecognizer for Canned {
        fn recognize(&self, img: &DynamicImage) -> Result<Vec<String>, String> {
            Ok(if img.width() < 60 {
                self.numeral.clone()
            } else {
                self.name.clone()
            })
        }
    }

    struct Broken;

    impl TextRecognizer for Broken {
        fn recognize(&self, _img: &DynamicImage) -> Result<Vec<String>, String> {
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
            name: vec!["Gemcutter's".into(), "Workshop".into()],
            numeral: Vec::new(),
        };
        let board = read_board(&recognizer, &img, &lattice);
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

    // A failed OCR call is Unknown, never a dropped slot: the board always has
    // 13 slots and a missing one would silently change the graph POE-170
    // scores.
    #[test]
    fn a_plate_whose_ocr_call_fails_reads_unknown() {
        let img = blank(1374, 862);
        let lattice = Lattice::new((673, 682), 0.99);
        let broken = read_board(&Broken, &img, &lattice);
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
        let clipped = read_board(&Canned::name("Tombs"), &img, &offscreen);
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

    // The numeral cross-check runs on the real crop path: a plate whose name
    // and numeral disagree is demoted.
    #[test]
    fn a_plate_whose_numeral_contradicts_its_name_reads_unknown() {
        let img = blank(1374, 862);
        let lattice = Lattice::new((673, 682), 0.99);
        // The numeral crop answers "III" on every plate, so a tier-1 name is
        // contradicted while a tier-3 one is confirmed.
        let iii = |name: &str| Canned {
            name: vec![name.to_string()],
            numeral: vec!["III".to_string()],
        };
        let contradicted = read_board(&iii("Corruption Chamber"), &img, &lattice);
        assert!(contradicted.iter().all(|r| r.identity == Match::Unknown));
        let agreed = read_board(&iii("Locus of Corruption"), &img, &lattice);
        assert!(agreed
            .iter()
            .all(|r| r.identity.identity().map(|id| id.display_name())
                == Some("Locus of Corruption")));
    }
}

