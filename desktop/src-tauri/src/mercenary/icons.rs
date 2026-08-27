//! Support-link identity: icon family template × roman tier badge (POE-165 D4).
//!
//! A support cell shows an icon and a small roman numeral. The icon names the
//! FAMILY (`Chain`), the numeral names the TIER (1-3); together they resolve to
//! a vocabulary link (`Lesser Chain (Tier 1)`). Neither half is shipped with
//! the app: PoE's art is not ours to redistribute, so the template store
//! starts empty and bootstraps ONLY from hover-confirms (D5).
//!
//! # Why the store is keyed on `(family, tier)`
//!
//! 58 of the 154 families span more than one tier, and whether the art differs
//! between tiers is unknown. Keying on the family alone would let a tier-1
//! confirmation overwrite a tier-3 sample (or vice versa) with art that may
//! not be the same picture; keying on the pair means one confirmation
//! bootstraps a family and no confirmed sample is ever replaced.
//! [`TemplateStore::match_family`] still searches ALL samples and reports the
//! best one's family, so a family learned at one tier immediately recognises
//! its other tiers — the tier comes from the badge, not from the template.
//!
//! # The signature, format 2 (POE-207)
//!
//! A cell reduces to 24×24 RGB over a disc that excludes the shared gold
//! frame, with the badge corner masked, normalised jointly over the 657 kept
//! channels — [`normalize_cell`] is the one place that derivation lives.
//! Matching then slides the CELL, not the template: [`cell_candidates`] builds
//! the same signature at all 49 alignments in ±[`SHIFT_MAX`] px, because the
//! rects `geometry::detect` emits land 1-3 px off per cell and an unaligned
//! comparison of one art against itself scored 0.45-0.70.
//!
//! Format 1 was 24×24 luma of the whole inner crop with no disc and no
//! alignment. It is not read anywhere: [`purge_stale_store`] unlinks a
//! version-1 store on the first start of a version-2 build, and the pool keys
//! on the version so the two corpora never mix.

use std::collections::HashMap;
use std::path::Path;

use image::{DynamicImage, GenericImageView, RgbImage, RgbaImage};
use serde::{Deserialize, Serialize};

use super::geometry::{inner_rect, luma, occupied};
use super::{BadgeGeometry, MercGeometry, ReadState, Thresholds};

/// Signature side length. 24×24 keeps the icon's silhouette and its colour
/// gradient while discarding the per-pixel noise a 44 px crop carries.
pub const SIG_DIM: u32 = 24;

/// Channels per signature position — RGB, since format 2 (POE-207).
///
/// Grayscale was version 1 and it could not be tuned into working: the gold
/// frame every cell shares dominates the luma, so visibly different icons
/// correlated 0.97-0.99 across families and `Matched` was unreachable for half
/// the store (measured on Sebastian's 61-template store, 2026-08-26). Colour
/// plus the disc mask below drops the genuine cross-family maximum to 0.818.
pub const SIG_CHANNELS: usize = 3;

/// Bytes in one stored/wire signature.
pub const SIG_BYTES: usize = (SIG_DIM * SIG_DIM) as usize * SIG_CHANNELS;

/// Radius of the kept disc, as a fraction of [`SIG_DIM`].
///
/// The cell's gold frame is identical on every cell, so anything it reaches is
/// shared signal that inflates every correlation equally. 0.36 keeps the art
/// and excludes the frame: at `SIG_DIM` 24 the disc is 8.64 px of a 12 px
/// half-width, which leaves the corners — where the frame lives — out.
const DISC_R_FRAC: f32 = 0.36;

/// Alignment margin, in SCREEN pixels per side, never scaled.
///
/// The cell rects `geometry::detect` emits land 1-3 px off per cell (the
/// column origin and the pitch are both fractional), and the same art in two
/// cells scored 0.45-0.70 without alignment — under `icon_low`, so a cell the
/// store already knew still read as unknown. The jitter is measured in screen
/// px and does not shrink with the panel, so this margin does not scale with
/// it either: the signature is built from the inner crop shrunk by this much
/// per side, and the matcher slides that window over the whole ±3 px range.
pub const SHIFT_MAX: i32 = 3;

/// Shifts per axis — `-SHIFT_MAX ..= SHIFT_MAX`.
pub const SHIFT_SPAN: i32 = 2 * SHIFT_MAX + 1;

/// The per-axis shifts stage one of the match search scores.
///
/// Every second offset, so nine of the 49 alignments cover the whole ±3 px
/// range at a 2 px grid. A correlation surface over a 1-3 px misalignment is
/// broad enough that the coarse maximum is within a step of the true one; the
/// fine stage is what turns "within a step" into the exact number.
const COARSE_STEPS: [i32; 3] = [-2, 0, 2];

/// The badge corner masked out of a signature, as fractions of the cell.
///
/// The numeral is not part of the family's identity — the same art carries a
/// I, II or III — so leaving it in would make the tier-1 and tier-3 samples of
/// one family score as different families. Wider and taller than the badge's
/// own read box ([`BadgeGeometry`]) on purpose: the mask only has to cost a
/// little signal, whereas a numeral leaking into the signature costs identity.
const MASK_W_FRAC: f32 = 0.45;
const MASK_H_FRAC: f32 = 0.35;

/// Index file naming the templates in a store directory.
const INDEX_FILE: &str = "index.json";

/// Run one write of the template directory, serialised against every other
/// writer of it (POE-204 WI-B).
///
/// [`TemplateStore::save`] writes one PNG per sample and THEN `index.json`, and
/// four owners call it: the loop's off-tick `run::SaveQueue` worker,
/// `sync::apply_corpus` when the pool's art lands, `sync::mark_uploaded` when a
/// batch is placed, and the forget/reset commands. [`purge_stale_store`] is the
/// fifth writer, and the one that unlinks rather than overwrites. The in-memory store's mutex
/// does not serialise them — the worker drops it before writing on purpose, so
/// the PNG writes do not stall the detect tick — and two overlapping writes end
/// with one caller's `index.json` over the other's PNGs: pooled art on disk
/// that no index names, under an ETag that says the pool already served it, so
/// every later pull answers 304 and the art never comes back.
///
/// **Lock order: this lock FIRST, then `AppState::merc_templates`.** Every
/// writer takes it around the whole read-modify-write, which is also what stops
/// a snapshot going stale between being taken and being written. Taking the
/// store's mutex first anywhere would close a cycle with the callers that hold
/// this one across it.
pub fn writing_icons_dir<T>(lock: &std::sync::Mutex<()>, write: impl FnOnce() -> T) -> T {
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    write()
}

/// A normalized cell signature: 24×24 RGB, zero-mean and unit-stddev jointly
/// over its unmasked channels, with the badge corner and everything outside
/// the disc zeroed.
#[derive(Debug, Clone, PartialEq)]
pub struct CellSig {
    /// The pre-normalization RGB, kept so a template round-trips through a PNG
    /// on disk without storing floats. Masked positions are zeroed here too,
    /// so the saved template shows exactly the pixels that take part in the
    /// correlation and a reload reproduces the signature byte for byte.
    bytes: Vec<u8>,
    /// Zero-mean unit-stddev values; exactly 0.0 at masked positions, which
    /// makes them contribute nothing to the correlation.
    norm: Vec<f32>,
    /// How many CHANNELS are unmasked — the correlation's divisor. 657 under
    /// the default mask (219 kept positions × 3).
    active: usize,
}

/// Whether a signature position is masked out: inside the badge corner, or
/// outside the kept disc.
///
/// Two rules, one gate. The badge corner carries the tier numeral, which is
/// not part of the family's identity. Outside the disc is the cell frame,
/// which every cell shares — keeping it would make every pair of icons
/// correlate on the frame rather than on the art.
fn masked(x: u32, y: u32) -> bool {
    let mask_x0 = SIG_DIM - (SIG_DIM as f32 * MASK_W_FRAC).round() as u32;
    let mask_y0 = SIG_DIM - (SIG_DIM as f32 * MASK_H_FRAC).round() as u32;
    if x >= mask_x0 && y >= mask_y0 {
        return true;
    }
    // Pixel CENTRES against the signature's centre — the convention the
    // corpus band was measured with, and the one the server's mask repeats.
    let centre = SIG_DIM as f32 / 2.0;
    let dx = x as f32 + 0.5 - centre;
    let dy = y as f32 + 0.5 - centre;
    dx.hypot(dy) > DISC_R_FRAC * SIG_DIM as f32
}

impl CellSig {
    /// Build a signature from a `SIG_DIM × SIG_DIM` RGB buffer.
    ///
    /// The three channels normalise JOINTLY — one mean and one stddev over
    /// all [`Self::active`] kept channels, not one per channel. Per-channel
    /// normalisation would rescale each channel to unit variance and throw
    /// away the ratios between them, which is most of what separates two
    /// icons that share a silhouette.
    ///
    /// `None` when the unmasked region is flat: an empty slot has no gradient
    /// to normalize, and dividing by its zero stddev would make every
    /// comparison NaN.
    pub fn from_rgb(bytes: Vec<u8>) -> Option<Self> {
        if bytes.len() != SIG_BYTES {
            return None;
        }
        let mut sum = 0.0f64;
        let mut sum_sq = 0.0f64;
        let mut active = 0usize;
        for y in 0..SIG_DIM {
            for x in 0..SIG_DIM {
                if masked(x, y) {
                    continue;
                }
                let base = ((y * SIG_DIM + x) as usize) * SIG_CHANNELS;
                for c in 0..SIG_CHANNELS {
                    let v = bytes[base + c] as f64;
                    sum += v;
                    sum_sq += v * v;
                    active += 1;
                }
            }
        }
        if active == 0 {
            return None;
        }
        let mean = sum / active as f64;
        let var = (sum_sq / active as f64) - mean * mean;
        if var < 1.0 {
            // Under one grey level of variation: flat panel, not an icon.
            return None;
        }
        let sd = var.sqrt();
        let mut bytes = bytes;
        let mut norm = vec![0.0f32; bytes.len()];
        for y in 0..SIG_DIM {
            for x in 0..SIG_DIM {
                let base = ((y * SIG_DIM + x) as usize) * SIG_CHANNELS;
                if masked(x, y) {
                    for c in 0..SIG_CHANNELS {
                        bytes[base + c] = 0;
                    }
                    continue;
                }
                for c in 0..SIG_CHANNELS {
                    norm[base + c] = ((bytes[base + c] as f64 - mean) / sd) as f32;
                }
            }
        }
        Some(Self {
            bytes,
            norm,
            active,
        })
    }

    /// How many channels take part in the correlation. 657 under the default
    /// mask; two signatures with different counts never correlate.
    pub fn active(&self) -> usize {
        self.active
    }

    /// Normalized cross-correlation with another signature: 1.0 for identical
    /// art, 0.0 for unrelated, negative for inverted.
    pub fn ncc(&self, other: &CellSig) -> f32 {
        if self.active == 0 || self.active != other.active {
            return 0.0;
        }
        let dot: f32 = self
            .norm
            .iter()
            .zip(&other.norm)
            .map(|(a, b)| a * b)
            .sum();
        dot / self.active as f32
    }

    /// The stored RGB — masked positions zeroed, exactly the bytes a reload
    /// reproduces the signature from.
    ///
    /// This is also the ONLY thing that goes on the wire to the shared pool
    /// (POE-201): the upload payload is built from these bytes in memory, so
    /// the colour crop `save` writes next to a template never leaves the
    /// device and no code path has to walk the store directory to publish.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The stored RGB as an image, for `save` and for eyeballing.
    pub fn to_image(&self) -> RgbImage {
        RgbImage::from_raw(SIG_DIM, SIG_DIM, self.bytes.clone())
            .expect("a signature always holds SIG_BYTES samples")
    }
}

/// Reduce a support cell to its UNSHIFTED signature — THE derivation.
///
/// One named definition of "the signature of this cell", because three things
/// have to agree on it byte for byte: what `learn` stores, what goes on the
/// wire to the pool, and what the server derives from the same crop
/// (`internal/mercenary/signature.go`, checked by the parity golden). The
/// aligned search in [`cell_candidates`] is built on top of this, not beside
/// it.
///
/// Takes the cell's OUTER rect (what [`super::geometry::detect`] emits), reads
/// its inner region and then gives up [`SHIFT_MAX`] px per side of that — the
/// alignment window. What is left of the cell frame after the inset is outside
/// the disc mask, so the frame — identical on every cell — never reaches the
/// correlation.
///
/// `None` when the rect is off-image, or when the cell is not
/// [`occupied`]. The occupancy gate is here rather than only at the call sites
/// because an empty slot's signature is *nearly constant*, and two of them
/// correlate at ~1.0: learning one as a template would then "recognise" every
/// empty slot on screen as that family. One rule, one threshold
/// (`empty_cell_stddev`), enforced where the signature is built.
pub fn normalize_cell(img: &DynamicImage, rect: [i32; 4], g: &MercGeometry) -> Option<CellSig> {
    if !occupied(img, rect, g) {
        return None;
    }
    let (win, _) = shift_window(rect, g);
    window_sig(img, win, 0, 0)
}

/// Warned once per process when the geometry leaves no alignment room.
static NARROW_WINDOW: std::sync::Once = std::sync::Once::new();

/// The UNSHIFTED signature window of a cell, and whether shifting it is safe.
///
/// The window is the inner crop shrunk by [`SHIFT_MAX`] screen px per side —
/// 33×33 at the live scale 0.974, 34×34 on the 1:1 reference fixture. The
/// margin it gives up is what the matcher slides over, so the alignment room
/// is bought here rather than by growing the rect (which would pull the
/// neighbouring cell's art in at the extremes).
///
/// A `cellInset` override can make the inner crop too small to shrink. That is
/// the user's geometry, not a bug, so the signature falls back to the whole
/// inner crop and the matcher runs unaligned — the version-1 behaviour, which
/// worked badly rather than not at all. Warned once, because the loop would
/// otherwise say it on every cell of every tick.
fn shift_window(rect: [i32; 4], g: &MercGeometry) -> ([i32; 4], bool) {
    let [ix, iy, iw, ih] = inner_rect(rect, g);
    let (ww, wh) = (iw - 2 * SHIFT_MAX, ih - 2 * SHIFT_MAX);
    if ww < SIG_DIM as i32 || wh < SIG_DIM as i32 {
        NARROW_WINDOW.call_once(|| {
            log::warn!(
                "Merc: cell inner crop {iw}×{ih} leaves no room for the ±{SHIFT_MAX} px \
                 alignment window — matching unaligned (check cellInset/cellSize)"
            );
        });
        return ([ix, iy, iw, ih], false);
    }
    ([ix + SHIFT_MAX, iy + SHIFT_MAX, ww, wh], true)
}

/// One shifted window of a cell, resized to a signature.
///
/// `None` when the window falls outside the image, or when what it holds is
/// too flat to normalise.
fn window_sig(img: &DynamicImage, win: [i32; 4], dx: i32, dy: i32) -> Option<CellSig> {
    let [wx, wy, w, h] = win;
    let (x, y) = (wx + dx, wy + dy);
    if x < 0 || y < 0 || w <= 0 || h <= 0 {
        return None;
    }
    let (iw, ih) = img.dimensions();
    if (x + w) as u32 > iw || (y + h) as u32 > ih {
        return None;
    }
    let crop = img.crop_imm(x as u32, y as u32, w as u32, h as u32).to_rgb8();
    let resized = image::imageops::resize(
        &crop,
        SIG_DIM,
        SIG_DIM,
        image::imageops::FilterType::Triangle,
    );
    CellSig::from_rgb(resized.into_raw())
}

/// Every alignment of one cell, built once and matched many times (POE-207).
///
/// The rects `geometry::detect` emits are 1-3 px off per cell, so a template
/// learned in one cell scores 0.45-0.70 against the same art in another. The
/// fix is to let the CELL move, not the template: this holds the signature of
/// the cell's window at all 49 shifts in `-SHIFT_MAX..=SHIFT_MAX`², and
/// [`TemplateStore::match_family`] takes each template's best over them.
///
/// Built once per occupied cell per detect — 49 small crops and resizes — and
/// then reused for every template, which is what keeps the aligned search
/// affordable at the 792-sample pool ceiling.
#[derive(Debug, Clone)]
pub struct CellCandidates {
    /// Every shift whose window normalised.
    shifted: Vec<CellSig>,
    /// Indices into [`Self::shifted`] of the coarse subset, `dx,dy ∈ {-2,0,2}`.
    coarse: Vec<usize>,
    /// Index into [`Self::shifted`] of the unshifted `(0,0)` signature.
    centre: usize,
}

impl CellCandidates {
    /// A cell with exactly one alignment — what a `cellInset` override that
    /// leaves no margin produces, and what the tests use when the shift is
    /// not the thing under test.
    pub fn unaligned(sig: CellSig) -> Self {
        Self {
            shifted: vec![sig],
            coarse: vec![0],
            centre: 0,
        }
    }

    /// The unshifted signature — what `learn` stores and what the pool gets.
    ///
    /// Learning the aligned best would store a template built from a window
    /// the NEXT capture's rect does not reproduce, so the stored art would
    /// carry this capture's jitter into every later comparison. Equal, byte
    /// for byte, to what [`normalize_cell`] returns for the same rect — the
    /// derivation the wire and the server share.
    ///
    /// Consuming rather than borrowing on purpose: there is exactly one
    /// accessor for this alignment, so "which one did we learn" has one
    /// answer and one test. The other 48 exist only to be matched against.
    pub fn into_centre(mut self) -> CellSig {
        self.shifted.swap_remove(self.centre)
    }

    /// Every alignment, for the fine stage.
    pub fn all(&self) -> &[CellSig] {
        &self.shifted
    }

    /// The coarse alignments, for stage one.
    pub fn coarse(&self) -> impl Iterator<Item = &CellSig> {
        self.coarse.iter().map(|&i| &self.shifted[i])
    }
}

/// Every alignment of a support cell's signature.
///
/// Same gates as [`normalize_cell`] — occupancy first, then the window's own
/// bounds and flatness checks — so a cell either yields candidates whose
/// [`CellCandidates::into_centre`] is exactly what `normalize_cell` would
/// have returned, or yields nothing.
pub fn cell_candidates(
    img: &DynamicImage,
    rect: [i32; 4],
    g: &MercGeometry,
) -> Option<CellCandidates> {
    let centre_sig = normalize_cell(img, rect, g)?;
    let (win, shiftable) = shift_window(rect, g);
    if !shiftable {
        return Some(CellCandidates::unaligned(centre_sig));
    }

    let mut shifted = Vec::with_capacity((SHIFT_SPAN * SHIFT_SPAN) as usize);
    let mut coarse = Vec::with_capacity(9);
    let mut centre = 0usize;
    for dy in -SHIFT_MAX..=SHIFT_MAX {
        for dx in -SHIFT_MAX..=SHIFT_MAX {
            let sig = if dx == 0 && dy == 0 {
                Some(centre_sig.clone())
            } else {
                window_sig(img, win, dx, dy)
            };
            let Some(sig) = sig else {
                continue;
            };
            let i = shifted.len();
            shifted.push(sig);
            if dx == 0 && dy == 0 {
                centre = i;
            }
            if COARSE_STEPS.contains(&dx) && COARSE_STEPS.contains(&dy) {
                coarse.push(i);
            }
        }
    }
    Some(CellCandidates {
        shifted,
        coarse,
        centre,
    })
}

/// Where a stored sample came from (POE-201).
///
/// Matching does not care — a pooled sample recognises a cell exactly as well
/// as a hovered one. Two other things do: only `Local` samples are ever
/// uploaded (a pooled sample re-offered to the pool it came from is pure
/// traffic), and the page distinguishes the two so "I taught this" and "the
/// pool gave me this" are not the same chip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Learned on this device from a hover confirmation.
    #[default]
    Local,
    /// Merged in from the shared pool.
    Pooled,
}

impl Origin {
    /// How a log line names this provenance. Load-bearing in the refusal line
    /// [`TemplateStore::learn`] produces: "learned here" and "from the pool"
    /// point the player at two different fixes — their own wrong confirmation,
    /// or somebody else's arriving through the pool.
    pub fn describe(self) -> &'static str {
        match self {
            Origin::Local => "learned here",
            Origin::Pooled => "from the pool",
        }
    }
}

/// One learned sample.
#[derive(Debug, Clone)]
pub struct Template {
    pub family: String,
    pub tier: u8,
    pub sig: CellSig,
    /// The colour crop the sample was learned from, kept so the debug dump can
    /// show what the store actually holds. Never used in matching, and never
    /// uploaded — see [`CellSig::bytes`].
    pub raw: Option<RgbaImage>,
    pub origin: Origin,
    /// Whether this sample has been offered to the shared pool.
    ///
    /// Persisted, and only ever meaningful for a `Local` sample: it is what
    /// makes the offer survive a restart. A batch the uploader could not place
    /// is left `false` on disk, so the next module start offers it again
    /// instead of the retry budget having to outlive the session.
    pub uploaded: bool,
}

/// What a template lookup concluded.
#[derive(Debug, Clone, PartialEq)]
pub struct IconMatch {
    pub family: Option<String>,
    /// The tier the winning SAMPLE was learned at — informational only. The
    /// capture's tier comes from the badge, never from here.
    pub learned_tier: Option<u8>,
    pub score: f32,
    /// Best score among samples of a DIFFERENT family.
    pub runner_up: f32,
    pub state: ReadState,
}

impl IconMatch {
    /// The read a cell gets when nothing in the store reaches it — also what
    /// `read::build_capture` falls back to when a rect passes the occupancy
    /// gate but not the signature's own bounds check.
    pub fn unknown() -> Self {
        Self {
            family: None,
            learned_tier: None,
            score: 0.0,
            runner_up: 0.0,
            state: ReadState::Unknown,
        }
    }
}

/// `index.json` as format 2 writes it (POE-207).
///
/// `deny_unknown_fields` is what makes the two shapes tell themselves apart:
/// without it a bare array would still fail to deserialise here (an array is
/// not an object), but a FUTURE index that grew a field would silently parse
/// as this one and be read with that field's meaning dropped. The version is
/// the contract; an unexpected key means the file is not this contract.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoreIndex {
    format_version: u16,
    entries: Vec<IndexEntry>,
}

#[derive(Serialize, Deserialize)]
struct IndexEntry {
    family: String,
    tier: u8,
    file: String,
    /// Both POE-201 fields default. Not for old indexes any more — a
    /// pre-pool index is a bare array, which is format 1 and is purged
    /// unread — but forwards: an entry written by a build that has not
    /// learned about provenance yet still loads, as the user's own
    /// unpublished sample, which is the safe reading of "we do not know".
    /// Dropping the defaults would make one missing key discard the index.
    #[serde(default)]
    origin: Origin,
    #[serde(default)]
    uploaded: bool,
}

/// One sample as the shared pool served it (POE-201).
#[derive(Debug, Clone, PartialEq)]
pub struct PooledSample {
    pub family: String,
    pub tier: u8,
    pub sig: CellSig,
}

/// What one pull brought back.
///
/// Decoded from the wire by [`super::sync`] and handed here as plain data, so
/// the merge rules can be exercised without a server: this type carries no
/// transport, no ETag and no device identity — the corpus deliberately names
/// nobody.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PooledCorpus {
    pub format_version: u16,
    pub samples: Vec<PooledSample>,
    /// Keys something was retired from. NOT "keys that are gone" — see
    /// [`TemplateStore::merge_pulled`].
    pub tombstones: Vec<(String, u8)>,
}

/// What one [`TemplateStore::merge_pulled`] did, for the log line and the
/// "does this need saving" question.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeOutcome {
    /// Served samples added to the store.
    pub added: usize,
    /// Local samples dropped because their key was tombstoned.
    pub replaced: usize,
    /// Served samples the store already had, or that would have gone past the
    /// per-key cap.
    pub skipped: usize,
    /// Served entries held back by a local forget the server has not
    /// acknowledged.
    pub suppressed: usize,
    /// Samples refused because one art turned up under two families — a
    /// mislabel from another device, refused for the same reason
    /// [`TemplateStore::learn`] refuses one locally. TWO per collision when
    /// both sides are pooled, because then both are refused; one when the
    /// incumbent is local and only the served sample yields.
    pub conflicting: usize,
    /// Already-stored POOLED samples removed because a served sample of
    /// another family carried the same art.
    ///
    /// Separate from `conflicting` because it answers a different question:
    /// `conflicting` counts refusals, this counts the ones that moved the
    /// store and therefore owe a save. A sample this same pull installed and
    /// then dropped is NOT counted — it un-counts itself from `added`, and the
    /// store is back where it started.
    pub dropped: usize,
    /// The corpus declared a format version this build cannot read — nothing
    /// was merged.
    pub foreign_version: bool,
}

impl MergeOutcome {
    /// Whether the store actually moved, and therefore whether a save and a
    /// generation bump are owed.
    pub fn changed(&self) -> bool {
        self.added > 0 || self.replaced > 0 || self.dropped > 0
    }
}

/// What one [`TemplateStore::learn`] did.
///
/// An enum rather than the `bool` it replaced because the third answer —
/// "identical art is already filed under someone else" — has to name WHO, and
/// a caller that only learns "not stored" cannot tell the player which of the
/// two families is now unmatchable.
#[derive(Debug, Clone, PartialEq)]
pub enum LearnOutcome {
    /// The sample was recorded.
    Stored,
    /// Nothing stored: a sample under this exact `(family, tier)` already
    /// matches the art, or the key is full.
    AlreadyKnown,
    /// Nothing stored: a stored sample of ANOTHER family reaches this art at
    /// `icon_match`. See [`TemplateStore::learn`] for why this is a refusal
    /// and not a second sample. `origin` says whether the incumbent is this
    /// player's own confirmation or one the pool taught them — two different
    /// fixes.
    ConflictsWith {
        family: String,
        tier: u8,
        origin: Origin,
        score: f32,
    },
}

impl LearnOutcome {
    /// Whether the store actually took the sample — and therefore whether a
    /// save and a pool offer are owed.
    pub fn stored(&self) -> bool {
        matches!(self, Self::Stored)
    }
}

/// A stored sample a candidate signature collides with — one art, two families.
///
/// Carries `origin` because the two doors act on it differently:
/// [`TemplateStore::learn`] only NAMES it (the incumbent wins either way), while
/// [`TemplateStore::merge_pulled`] evicts a pooled incumbent and leaves a local
/// one standing. `at` is the index the eviction needs.
#[derive(Debug, Clone)]
struct Collision {
    at: usize,
    family: String,
    tier: u8,
    origin: Origin,
    score: f32,
}

/// The learned icon templates, keyed by `(family, tier)`.
///
/// `Clone` so the off-tick writer can take a SNAPSHOT under the mutex and do
/// its disk I/O outside it. [`Self::save`] writes one PNG per sample plus the
/// index, and holding the store's lock across that would put every
/// [`Self::match_family`] call on the detect tick behind the filesystem — the
/// stall the off-tick save exists to remove, moved rather than fixed. The copy
/// is cheap next to the write it replaces: a full store is
/// [`Self::MAX_SAMPLES_PER_KEY`] × 24×24 RGB plus the 44×44 raw crops.
#[derive(Debug, Default, Clone)]
pub struct TemplateStore {
    templates: Vec<Template>,
}

impl TemplateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    pub fn templates(&self) -> &[Template] {
        &self.templates
    }

    /// `"<family>--<tier>"` for everything in the store, sorted.
    ///
    /// This exact shape is a CONTRACT with the page, not a display label: it
    /// goes into `MercenarySlice::learned_families`, and the page splits the
    /// trailing `--<digits>` back into the two arguments of
    /// `merc_forget_template(family, tier)`. The split is unambiguous because
    /// no family name in the vocabulary contains a hyphen or ends in a digit
    /// (154 families, checked by the test below), so the LAST `--` is always
    /// the separator.
    ///
    /// One producer on purpose: a second, prettier label would eventually be
    /// the one wired into the slice, and the page's parse would silently stop
    /// finding a tier.
    pub fn learned_keys(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .templates
            .iter()
            .map(|t| format!("{}--{}", t.family, t.tier))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Samples kept per `(family, tier)`. The first Windows session
    /// (2026-08-24) saved templates that later matched nothing, and with one
    /// slot per key every further confirm was refused as "already known"; a
    /// few extra samples let a confirm repair that, and the cap keeps a
    /// jittery hover from filling the store.
    pub const MAX_SAMPLES_PER_KEY: usize = 3;

    /// Record a confirmed sample.
    ///
    /// Stores nothing and returns [`LearnOutcome::AlreadyKnown`] when a sample
    /// already stored under `(family, tier)` MATCHES this one (at
    /// `t.icon_match`), or when the key already holds
    /// [`Self::MAX_SAMPLES_PER_KEY`] samples. A sample that no stored one
    /// reaches is ADDED, never overwriting: the existing sample may be good art
    /// from another session, and this may be the mistimed hover — or the
    /// reverse. Matching searches every sample, so either way the cell is
    /// recognised next time. The un-poison path is [`Self::forget`], which
    /// drops the whole key.
    ///
    /// **A mislabelled pair is REFUSED, not stored** (POE-207 AC3). Art the
    /// store already holds under a DIFFERENT family is the one thing a second
    /// sample cannot repair: `match_family` needs an `icon_lead` lead over the
    /// best sample of another family, so once one art sits under two families
    /// BOTH of them stop being `Matched` for good — and a `forget` of either
    /// one is a guess at which confirmation was the wrong one. This is the
    /// poison the 2026-08-26 purge found on 19 of 21 samples. The refusal
    /// names the family already holding the art, because that name is the whole
    /// of the diagnosis; the confirmation itself still stands and still applies
    /// to the cell — the player said what it is, and only the template is
    /// withheld.
    ///
    /// The cross-family check runs FIRST, before the same-key one, so a store
    /// already carrying the mislabel reports it on the next confirm rather than
    /// answering the innocent "already known". On a clean store the two orders
    /// are indistinguishable — nothing reaches `icon_match` across families.
    ///
    /// **The INCUMBENT wins, whichever side it is on.** A confirmation that
    /// collides with a pooled sample is refused rather than evicting it, even
    /// though the player is the better authority: eviction here and a re-merge
    /// on the next pull is a ping-pong that writes the store every session and
    /// never settles, because this device cannot tombstone somebody else's
    /// sample from the learn path. The player's escape is [`Self::forget`] on
    /// the named key, which DOES tombstone — which is why the refusal names the
    /// key and its provenance rather than just saying no.
    ///
    /// **What this gate does and does not catch.** It is one unshifted
    /// signature against another, no alignment search — sample-vs-sample, so a
    /// confirm stays cheap. That catches art that is BYTE-REGISTERED with the
    /// incumbent: the same cell rect read twice, which is the tooltip-lag
    /// mislabel and 19 of the 21 poisoned samples of 2026-08-26. It does NOT
    /// catch the same art cut from a DIFFERENT cell rect, on this device or
    /// another: `geometry::detect` lands its rects 1-3 px apart, and the same
    /// art in two cells scores 0.45-0.70 unaligned (see [`SHIFT_MAX`]) — under
    /// `icon_low`, let alone `icon_match`. Pooled samples are the same case by
    /// construction, since only the 1728-byte signature travels and this device
    /// never sees the crop it came from. The renumbering path — a dropped skill
    /// line filing one cell's art under another cell's family — is covered
    /// locally by keying the crop cache on `row_key` instead of the row index,
    /// not here.
    pub fn learn(
        &mut self,
        family: &str,
        tier: u8,
        sig: CellSig,
        raw: Option<RgbaImage>,
        t: &Thresholds,
    ) -> LearnOutcome {
        if let Some(other) = self.best_other_family(family, &sig, t) {
            return LearnOutcome::ConflictsWith {
                family: other.family,
                tier: other.tier,
                origin: other.origin,
                score: other.score,
            };
        }
        let same_key: Vec<&Template> = self
            .templates
            .iter()
            .filter(|s| s.family == family && s.tier == tier)
            .collect();
        if same_key.len() >= Self::MAX_SAMPLES_PER_KEY
            || same_key.iter().any(|s| s.sig.ncc(&sig) >= t.icon_match)
        {
            return LearnOutcome::AlreadyKnown;
        }
        self.push_sample(family, tier, sig, raw, Origin::Local, false);
        LearnOutcome::Stored
    }

    /// The stored sample of a family OTHER than `family` that `sig` reaches
    /// hardest, when it reaches one at `t.icon_match` at all.
    ///
    /// One derivation for both doors into the store — [`Self::learn`] and
    /// [`Self::merge_pulled`] — because a rule that only guarded the local door
    /// would let the same mislabel walk in from another device through the
    /// pool, which is the half of AC3 the hover guard does not cover. What the
    /// gate reaches and what it misses is written out on [`Self::learn`].
    ///
    /// Family, not `(family, tier)`: two tiers of ONE family sharing art is the
    /// normal case the matcher's lead rule already treats as agreement.
    ///
    /// Ties go to the FIRST stored sample, which is the oldest: when two
    /// families already hold the art the earlier confirmation is the one named.
    fn best_other_family(
        &self,
        family: &str,
        sig: &CellSig,
        t: &Thresholds,
    ) -> Option<Collision> {
        let mut best: Option<Collision> = None;
        for (at, stored) in self.templates.iter().enumerate() {
            if stored.family == family {
                continue;
            }
            let score = stored.sig.ncc(sig);
            if score < t.icon_match {
                continue;
            }
            if best.as_ref().map(|c| c.score).unwrap_or(f32::MIN) < score {
                best = Some(Collision {
                    at,
                    family: stored.family.clone(),
                    tier: stored.tier,
                    origin: stored.origin,
                    score,
                });
            }
        }
        best
    }

    /// Store a sample unconditionally — the load path, where every index
    /// entry is a sample that was accepted when it was learned.
    fn push_sample(
        &mut self,
        family: &str,
        tier: u8,
        sig: CellSig,
        raw: Option<RgbaImage>,
        origin: Origin,
        uploaded: bool,
    ) {
        self.templates.push(Template {
            family: family.to_string(),
            tier,
            sig,
            raw,
            origin,
            uploaded,
        });
    }

    /// `"<family>--<tier>"` for the keys holding NO locally-learned sample —
    /// the ones this device knows only because the pool taught it.
    ///
    /// A subset of [`Self::learned_keys`], in the same shape and for the same
    /// reason: the page renders one chip per learned key and marks the ones
    /// listed here. A key the user hovered stays off this list even after the
    /// pool adds samples to it, because the question the chip answers is "did
    /// I teach this", not "does the pool also have it".
    pub fn pooled_keys(&self) -> Vec<String> {
        let local: std::collections::HashSet<(&str, u8)> = self
            .templates
            .iter()
            .filter(|t| t.origin == Origin::Local)
            .map(|t| (t.family.as_str(), t.tier))
            .collect();
        let mut out: Vec<String> = self
            .templates
            .iter()
            .filter(|t| !local.contains(&(t.family.as_str(), t.tier)))
            .map(|t| format!("{}--{}", t.family, t.tier))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// How many samples came from the pool. The page shows it next to the
    /// learned count so "23 templates" can be read as "3 of mine, 20 shared".
    pub fn pooled_samples(&self) -> usize {
        self.templates
            .iter()
            .filter(|t| t.origin == Origin::Pooled)
            .count()
    }

    /// Every local sample the pool has not been offered yet, as
    /// `(family, tier, signature bytes)`.
    ///
    /// Built from the in-memory signatures, never from the directory: the
    /// colour crops sitting next to them on disk are GGG's art and must not
    /// leave the device (POE-201 L4).
    pub fn pending_uploads(&self) -> Vec<(String, u8, Vec<u8>)> {
        self.templates
            .iter()
            .filter(|t| t.origin == Origin::Local && !t.uploaded)
            .map(|t| (t.family.clone(), t.tier, t.sig.bytes().to_vec()))
            .collect()
    }

    /// Record that the pool has seen this exact sample. `true` when something
    /// was marked, so the caller knows whether a save is owed.
    ///
    /// Matched on the signature bytes rather than on the key: a key can hold
    /// three samples offered in three different requests, and marking the key
    /// would tell the next module start that samples the pool never saw are
    /// already published.
    pub fn mark_uploaded(&mut self, family: &str, tier: u8, bytes: &[u8]) -> bool {
        let mut marked = false;
        for sample in &mut self.templates {
            if sample.family == family
                && sample.tier == tier
                && sample.origin == Origin::Local
                && !sample.uploaded
                && sample.sig.bytes() == bytes
            {
                sample.uploaded = true;
                marked = true;
            }
        }
        marked
    }

    /// Fold a pulled corpus into the store (POE-201).
    ///
    /// Four rules, in this order:
    ///
    /// 1. **A foreign format version is never merged.** Signatures from two
    ///    versions are not comparable, so merging them would not "mostly work"
    ///    — it would put art into the matcher that correlates against nothing
    ///    and drag every score with it.
    /// 2. **A tombstoned key is REPLACED, not unioned.** The server listing a
    ///    key means somebody retired art from it; the local copies go and the
    ///    served ones take their place. This is what makes a forget durable:
    ///    without it, the device that still holds the bad sample would union it
    ///    straight back in on its next pull. A tombstoned key may still carry
    ///    live samples — retiring bad art does not close the key.
    /// 3. **A served sample whose art the store already holds under ANOTHER
    ///    family is refused**, and WHO ELSE goes with it depends on what the
    ///    incumbent is. The rule and the reason are [`Self::learn`]'s — one art
    ///    under two families makes both permanently unmatchable — reached by
    ///    the other door: the local hover guard cannot see a wrong confirmation
    ///    made on somebody else's machine, so the pool needs its own copy of
    ///    the check (POE-207 AC3). The same limit applies as there: this
    ///    compares one unshifted signature against another, which catches art
    ///    byte-registered with the incumbent and NOT the same art cut from a
    ///    different cell rect, which scores 0.45-0.70 unaligned. A pooled
    ///    sample is always that case — only the 1728-byte signature travels, so
    ///    this device never has the crop to re-align.
    ///    - **Against a LOCAL incumbent the served sample yields**, one
    ///      `conflicting`, and the incumbent stays: it is the art this player
    ///      confirmed on their own screen, and that is the only ground truth
    ///      this device has.
    ///    - **Against a POOLED incumbent BOTH go**, two `conflicting` and one
    ///      `dropped`, because nothing here says which of the two strangers is
    ///      the mislabel and keeping either is a coin flip that can read
    ///      `Matched` on the wrong family. The module's standing preference is
    ///      to fail towards `LowConfidence`, never towards a wrong family, so
    ///      the cell goes back to `?` and the player's own hover settles it.
    ///      The refused art is then remembered FOR THE REST OF THE PULL, so a
    ///      cluster empties whatever its size: without that, a third family
    ///      claiming the same art would meet the store the first two had just
    ///      emptied and install into it, and which family survived would be
    ///      the server's listing order. Two such three-family clusters are in
    ///      the committed corpus. What holds is therefore stronger than "the
    ///      same shape either way": for one art, the MERGED STORE ITSELF is
    ///      the same in every ordering — no family holds it.
    /// 4. **Everything else is a union**, deduped by the same NCC the local
    ///    `learn` uses and capped by the same [`Self::MAX_SAMPLES_PER_KEY`]:
    ///    the cap applies to the MERGED set, so a pull cannot push a key past
    ///    what the matcher was sized for.
    ///
    /// `suppressed` names keys this device forgot whose tombstone the server
    /// has not acknowledged yet. They are skipped entirely — local samples
    /// left alone, served ones not installed — because until the tombstone
    /// lands the corpus still carries the art the user just disowned, and
    /// installing it would undo the forget on every pull.
    pub fn merge_pulled(
        &mut self,
        corpus: &PooledCorpus,
        suppressed: &[(String, u8)],
        t: &Thresholds,
    ) -> MergeOutcome {
        let mut out = MergeOutcome::default();
        if corpus.format_version != super::sync::FORMAT_VERSION {
            out.foreign_version = true;
            return out;
        }
        let held = |family: &str, tier: u8| {
            suppressed
                .iter()
                .any(|(f, ti)| f.as_str() == family && *ti == tier)
        };

        for (family, tier) in &corpus.tombstones {
            if held(family, *tier) {
                out.suppressed += 1;
                continue;
            }
            let before = self.templates.len();
            self.templates
                .retain(|s| !(s.family == *family && s.tier == *tier));
            out.replaced += before - self.templates.len();
        }

        // Where this pull's own installs begin. Everything below it was in the
        // store before the loop, and only those owe a save when they are
        // dropped — see the pooled-incumbent arm.
        let mut installed_from = self.templates.len();
        // Art this pull has already thrown out. The store cannot carry this,
        // because emptying a collision is exactly what removes the evidence:
        // a cluster's members arrive one at a time, and once two of them have
        // knocked each other out the THIRD meets an empty store and installs —
        // surviving as a confident wrong `Matched` chosen by the server's
        // listing order. The fixture holds two real three-family clusters
        // (DoT Multiplier / Swift Affliction / Cooldown Recovery, and Area of
        // Effect / Increased Area of Effect / Curse Effect), so this is
        // observed shape, not a hypothetical.
        //
        // Keyed on the ART ALONE, with no family part: a disputed art is
        // disputed under every name, including the ones already in the
        // dispute. Excluding the refused sample's own family would let a
        // second sample of it back in and hand the art to exactly one family
        // again, which is the coin flip this rule exists to prevent.
        let mut disputed: Vec<CellSig> = Vec::new();
        for sample in &corpus.samples {
            if held(&sample.family, sample.tier) {
                out.suppressed += 1;
                continue;
            }
            // Rule 3, part one: art this pull has already emptied stays empty,
            // however many families claim it. Checked BEFORE the store,
            // because by now the collision that disputed it has left no trace
            // there.
            if disputed.iter().any(|d| d.ncc(&sample.sig) >= t.icon_match) {
                out.conflicting += 1;
                continue;
            }
            // Rule 3, part two, checked against the store AS IT STANDS —
            // local samples plus everything this same pull has already merged.
            match self.best_other_family(&sample.family, &sample.sig, t) {
                // The player confirmed the incumbent on their own screen. The
                // served sample yields to it.
                Some(c) if c.origin == Origin::Local => {
                    out.conflicting += 1;
                    continue;
                }
                // Two strangers, and nothing on this device says which is the
                // mislabel. Both go, so the cell reads `?` rather than a
                // coin-flip `Matched` on the wrong family — and both arts join
                // `disputed`, which is what carries the verdict past the pair
                // to the rest of the cluster. Together those two make the
                // merged store independent of the order the server listed a
                // cluster in, at any size: every family that claimed the art
                // ends the pull holding none of it.
                Some(c) => {
                    disputed.push(self.templates[c.at].sig.clone());
                    disputed.push(sample.sig.clone());
                    self.templates.remove(c.at);
                    if c.at >= installed_from {
                        // Installed by this very pull: un-count it rather than
                        // report a net change that did not happen.
                        out.added = out.added.saturating_sub(1);
                    } else {
                        // It was on disk before this pull, so the store really
                        // shrank and a save is owed. The boundary moves with
                        // the element that came out from under it.
                        installed_from -= 1;
                        out.dropped += 1;
                    }
                    out.conflicting += 2;
                    continue;
                }
                None => {}
            }
            let same_key = self
                .templates
                .iter()
                .filter(|s| s.family == sample.family && s.tier == sample.tier);
            let mut count = 0usize;
            let mut known = false;
            for existing in same_key {
                count += 1;
                if existing.sig.ncc(&sample.sig) >= t.icon_match {
                    known = true;
                }
            }
            if known || count >= Self::MAX_SAMPLES_PER_KEY {
                out.skipped += 1;
                continue;
            }
            self.push_sample(
                &sample.family,
                sample.tier,
                sample.sig.clone(),
                None,
                Origin::Pooled,
                false,
            );
            out.added += 1;
        }
        out
    }

    pub fn get(&self, family: &str, tier: u8) -> Option<&Template> {
        self.templates
            .iter()
            .find(|t| t.family == family && t.tier == tier)
    }

    /// Drop one sample. `true` when something was removed.
    pub fn forget(&mut self, family: &str, tier: u8) -> bool {
        let before = self.templates.len();
        self.templates
            .retain(|t| !(t.family == family && t.tier == tier));
        self.templates.len() != before
    }

    /// Drop everything.
    pub fn reset(&mut self) {
        self.templates.clear();
    }

    /// Templates rescored over all 49 alignments in stage two, per side.
    ///
    /// Measured on the 61-crop corpus (POE-207): with the 40 non-poisoned
    /// crops as the store and each of them as a probe, the two-stage result at
    /// 12 equals the full 49-shift result — family, score AND runner-up — on
    /// 40/40 probes, and the true best other-family template is inside the
    /// refined set every time. At 8 one probe differs, and it differs towards
    /// `LowConfidence`, never towards a wrong family.
    pub const REFINE_K: usize = 12;

    /// Best family for a cell, over every alignment of it.
    ///
    /// Two stages, because the plain search is 49 × templates × 657 MACs —
    /// 1.8 G at the 792-sample pool ceiling, which does not fit a 2 s detect
    /// tick. Stage one scores every template against the nine coarse
    /// alignments. Stage two rescores over all 49 both the top
    /// [`Self::REFINE_K`] templates and the top [`Self::REFINE_K`] among
    /// families OTHER than the stage-one winner's; every other template keeps
    /// its coarse score.
    ///
    /// The other-family half is not an optimisation — it is what keeps the
    /// verdict honest. A coarse score is a maximum over nine alignments and
    /// therefore UNDER-estimates the 49-alignment truth, so refining only the
    /// leaders would compare an exact winner against an under-estimated
    /// runner-up, inflate the lead, and promote a `LowConfidence` cell to a
    /// confidently wrong `Matched`.
    ///
    /// `Matched` needs `icon_match` with an `icon_lead` lead over the best
    /// sample of a DIFFERENT family — two tiers of the same family competing
    /// with each other is agreement, not ambiguity. `LowConfidence` at
    /// `icon_low`. Anything else is `Unknown`, which the page renders as
    /// "unknown — hover to confirm".
    pub fn match_family(&self, cell: &CellCandidates, t: &Thresholds) -> IconMatch {
        self.match_with_refinement(cell, t, Self::REFINE_K)
    }

    /// The unrefined search: every template against every alignment.
    ///
    /// The ground truth [`Self::match_family`] approximates, and the reason
    /// the two-stage search can be trusted at all. Test-only: at the 792-sample
    /// pool ceiling it is 1.84 G MACs per cell, which is what the two stages
    /// exist to avoid — shipping it would give a caller a way to reintroduce
    /// the cost the design rejected.
    #[cfg(test)]
    pub fn match_family_exhaustive(&self, cell: &CellCandidates, t: &Thresholds) -> IconMatch {
        let scores: Vec<f32> = self
            .templates
            .iter()
            .map(|tpl| best_over(&tpl.sig, cell.all()))
            .collect();
        self.verdict(scores, t)
    }

    fn match_with_refinement(
        &self,
        cell: &CellCandidates,
        t: &Thresholds,
        k: usize,
    ) -> IconMatch {
        if self.templates.is_empty() {
            return IconMatch::unknown();
        }
        let coarse: Vec<&CellSig> = cell.coarse().collect();
        let mut scores: Vec<f32> = self
            .templates
            .iter()
            .map(|tpl| best_over_refs(&tpl.sig, &coarse))
            .collect();

        let leader = argmax(&scores);
        let leader_family = self.templates[leader].family.clone();
        let mut refine = top_k(&scores, k, |_| true);
        refine.extend(top_k(&scores, k, |i| self.templates[i].family != leader_family));
        refine.sort_unstable();
        refine.dedup();
        for i in refine {
            scores[i] = best_over(&self.templates[i].sig, cell.all());
        }

        self.verdict(scores, t)
    }

    /// Turn per-template scores into the read, in the store's own order.
    fn verdict(&self, scores: Vec<f32>, t: &Thresholds) -> IconMatch {
        if self.templates.is_empty() {
            return IconMatch::unknown();
        }
        let best = argmax(&scores);
        let winner = &self.templates[best];
        let score = scores[best];
        let runner_up = self
            .templates
            .iter()
            .zip(&scores)
            .filter(|(tpl, _)| tpl.family != winner.family)
            .map(|(_, &s)| s)
            .fold(f32::NEG_INFINITY, f32::max);
        let runner_up = if runner_up.is_finite() { runner_up } else { 0.0 };

        let state = if score >= t.icon_match && score - runner_up >= t.icon_lead {
            ReadState::Matched
        } else if score >= t.icon_low {
            ReadState::LowConfidence
        } else {
            return IconMatch {
                score,
                runner_up,
                ..IconMatch::unknown()
            };
        };
        IconMatch {
            family: Some(winner.family.clone()),
            learned_tier: Some(winner.tier),
            score,
            runner_up,
            state,
        }
    }

    /// Write the store to `dir`: one 24×24 RGB PNG per sample, the raw crop
    /// alongside when there is one, and an index naming them (family names
    /// carry spaces, so the file names are slugs and the index is what maps
    /// them back).
    ///
    /// The index carries `formatVersion` (POE-207). Version 1 wrote a bare
    /// array, so the shape itself says which derivation the PNGs next to it
    /// hold — and [`purge_stale_store`], not this, is what acts on that.
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut index = Vec::with_capacity(self.templates.len());
        let mut seen: HashMap<(String, u8), usize> = HashMap::new();
        for t in &self.templates {
            // Samples of one key are numbered from the second on, so the first
            // keeps the file name earlier stores wrote.
            let n = seen.entry((t.family.clone(), t.tier)).or_insert(0);
            *n += 1;
            let suffix = if *n == 1 { String::new() } else { format!("-{n}") };
            let file = format!("{}--t{}{suffix}.png", slug(&t.family), t.tier);
            t.sig
                .to_image()
                .save(dir.join(&file))
                .map_err(|e| format!("{file}: {e}"))?;
            if let Some(raw) = &t.raw {
                let raw_file = format!("{}--t{}{suffix}-raw.png", slug(&t.family), t.tier);
                raw.save(dir.join(&raw_file))
                    .map_err(|e| format!("{raw_file}: {e}"))?;
            }
            index.push(IndexEntry {
                family: t.family.clone(),
                tier: t.tier,
                file,
                origin: t.origin,
                uploaded: t.uploaded,
            });
        }
        let json = serde_json::to_string_pretty(&StoreIndex {
            format_version: super::sync::FORMAT_VERSION,
            entries: index,
        })
        .map_err(|e| e.to_string())?;
        std::fs::write(dir.join(INDEX_FILE), json).map_err(|e| e.to_string())
    }

    /// Read a store back. A missing directory is an empty store (the normal
    /// first-run case); an entry whose PNG will not load is SKIPPED and
    /// reported, so one corrupt file cannot take the whole store down.
    ///
    /// **Read-only, always.** A store written by another format version is
    /// reported and yields nothing; it is NOT rewritten or cleaned here.
    /// [`purge_stale_store`] is the one writer that acts on the version, and
    /// it runs before the loop starts, inside the directory's write lock —
    /// putting the unlink here would make every reader a writer.
    pub fn load(dir: &Path) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let raw = match std::fs::read_to_string(dir.join(INDEX_FILE)) {
            Ok(raw) => raw,
            Err(_) => return (Self::new(), problems),
        };
        let index = match read_index(&raw) {
            Ok(index) => index,
            Err(problem) => {
                problems.push(problem);
                return (Self::new(), problems);
            }
        };
        if index.format_version != super::sync::FORMAT_VERSION {
            problems.push(format!(
                "{INDEX_FILE} is format {} — {} template(s) ignored, format {} is what this build reads",
                index.format_version,
                index.entries.len(),
                super::sync::FORMAT_VERSION,
            ));
            return (Self::new(), problems);
        }
        let mut store = Self::new();
        for entry in index.entries {
            match image::open(dir.join(&entry.file)) {
                Ok(img) => {
                    let rgb = img.to_rgb8();
                    match CellSig::from_rgb(rgb.into_raw()) {
                        Some(sig) => {
                            store.push_sample(
                                &entry.family,
                                entry.tier,
                                sig,
                                None,
                                entry.origin,
                                entry.uploaded,
                            );
                        }
                        None => problems.push(format!(
                            "{}: not a {SIG_DIM}×{SIG_DIM} template",
                            entry.file
                        )),
                    }
                }
                Err(e) => problems.push(format!("{}: {e}", entry.file)),
            }
        }
        (store, problems)
    }
}

/// Parse an `index.json` of either shape.
///
/// A version-1 index is a bare ARRAY of entries — the shape `save` wrote
/// before `formatVersion` existed. Reading it as format 1 rather than
/// rejecting it as unparseable is what lets the purge count what it drops and
/// what lets `load` say how many templates it is ignoring.
fn read_index(raw: &str) -> Result<StoreIndex, String> {
    if let Ok(index) = serde_json::from_str::<StoreIndex>(raw) {
        return Ok(index);
    }
    match serde_json::from_str::<Vec<IndexEntry>>(raw) {
        Ok(entries) => Ok(StoreIndex {
            format_version: 1,
            entries,
        }),
        Err(e) => Err(format!("{INDEX_FILE} did not parse: {e}")),
    }
}

/// Drop a store this build cannot read, so the next `load` starts clean.
///
/// The signature derivation changed in format 2 (POE-207), and a version-1
/// template correlates against a version-2 cell at nothing meaningful — it
/// would not merely mismatch, it would sit in the store dragging every
/// runner-up around. There is no migration: the pixels a version-1 PNG holds
/// are 24×24 luma of the WHOLE inner crop, and no amount of arithmetic turns
/// that into the RGB disc of a 33 px window.
///
/// So the unlink is total: every `*.png` in the directory goes, the indexed
/// signatures AND the un-indexed `-raw.png` colour crops beside them, because
/// the raw crops are the GGG art the signatures were derived from and keeping
/// orphans of a format nothing reads is just a directory that never shrinks.
/// `pool-sync.json` is left alone — `sync::SyncFile::load` already discards a
/// file from another format version, which drops the version-1 pending
/// tombstones on purpose: a version-1 key names nothing in the version-2
/// keyspace.
///
/// Returns what was dropped, or `None` when there was nothing to purge (no
/// index at all, or one this build reads).
///
/// **Caller holds the directory write lock.** This is a writer, and it must
/// not interleave with [`TemplateStore::save`] — see [`writing_icons_dir`].
pub fn purge_stale_store(dir: &Path) -> Option<PurgedStore> {
    let raw = std::fs::read_to_string(dir.join(INDEX_FILE)).ok()?;
    let purged = match read_index(&raw) {
        Ok(index) if index.format_version == super::sync::FORMAT_VERSION => return None,
        Ok(index) => PurgedStore {
            version: Some(index.format_version),
            dropped: index.entries.len(),
        },
        // An index that parses as neither shape is still not a format-2 store,
        // and the PNGs beside it are still unreadable art. Purge, and say that
        // is what happened rather than reporting "0 format-1 templates" — the
        // two have different causes and only one of them is an upgrade.
        Err(_) => PurgedStore {
            version: None,
            dropped: 0,
        },
    };
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let empty = StoreIndex {
        format_version: super::sync::FORMAT_VERSION,
        entries: Vec::new(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&empty) {
        let _ = std::fs::write(dir.join(INDEX_FILE), json);
    }
    Some(purged)
}

/// What one [`purge_stale_store`] dropped, for the log line.
///
/// The version is carried rather than assumed: "format 1" is the case that
/// will happen on every current install, but a downgrade meets a format-3
/// index and an interrupted write meets one that parses as nothing, and a log
/// line that called all three "format-1 templates" would send the next reader
/// looking for the wrong cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurgedStore {
    /// The version the index declared, or `None` when it did not parse.
    pub version: Option<u16>,
    /// How many templates that index named.
    pub dropped: usize,
}

/// The best correlation of one template against a set of alignments.
fn best_over(template: &CellSig, cell: &[CellSig]) -> f32 {
    cell.iter()
        .map(|s| template.ncc(s))
        .fold(f32::NEG_INFINITY, f32::max)
}

/// [`best_over`] over borrowed alignments — the coarse subset's shape.
fn best_over_refs(template: &CellSig, cell: &[&CellSig]) -> f32 {
    cell.iter()
        .map(|s| template.ncc(s))
        .fold(f32::NEG_INFINITY, f32::max)
}

/// Index of the largest score. Ties go to the FIRST, which is store order —
/// the same rule the version-1 search used, so a tie is broken the same way
/// whichever stage produced the scores.
fn argmax(scores: &[f32]) -> usize {
    let mut best = 0usize;
    for (i, &s) in scores.iter().enumerate() {
        if s > scores[best] {
            best = i;
        }
    }
    best
}

/// Indices of the `k` highest scores among those the predicate admits.
///
/// Ties broken by index, so the refined set is a pure function of the scores
/// and does not depend on sort stability.
fn top_k(scores: &[f32], k: usize, admit: impl Fn(usize) -> bool) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..scores.len()).filter(|&i| admit(i)).collect();
    idx.sort_unstable_by(|&a, &b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    idx.truncate(k);
    idx
}

/// File-name-safe form of a family name.
fn slug(family: &str) -> String {
    let s: String = family
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    if s.is_empty() {
        "unnamed".to_string()
    } else {
        s
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mercenary::geometry::occupied;
    use image::Rgba;

    /// The committed reference panel: the (60,585) crop of Sebastian's
    /// `recruit-cai.png`, 600×310, six skill rows and twelve support cells.
    /// This is the ONLY real-pixel ground truth the module has.
    fn fixture() -> DynamicImage {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/merc-skills-panel.png"
        );
        image::open(path).expect("the committed reference panel loads")
    }

    /// Row centres and the cell origin measured on the fixture, in FIXTURE px
    /// (the full-image values of D1/D2 minus the crop origin 60,585).
    ///
    /// - row centres 620/669/717/766/814/862 − 585 → 35/84/132/181/229/277
    ///   (row 4 is the wrapped name's two lines averaged);
    /// - the skill-name column starts at x 134 − 60 = 74, and D1 puts slot 0
    ///   at name_x + 238 → 312, pitch 49, size 44.
    ///
    /// The fixture is a 1:1 crop, so scale = 1 and the rects need no scaling.
    const ROW_CENTRES: [i32; 6] = [35, 84, 132, 181, 229, 277];
    const COLUMN_X0: i32 = 74;

    /// The OUTER cell rect for a row/slot of the fixture, built from the
    /// reference constants rather than typed in.
    fn cell(row: usize, slot: u8) -> [i32; 4] {
        let g = MercGeometry::default();
        let size = g.cell_size as i32;
        let x = COLUMN_X0 + g.cell_offset_x as i32 + slot as i32 * g.cell_pitch as i32;
        [x, ROW_CENTRES[row] - size / 2, size, size]
    }

    /// Every occupied cell of the fixture, with the tier its badge shows.
    /// Read off a 4× contact sheet of the twelve cells; all three tiers are
    /// represented.
    const OCCUPIED_CELLS: [(usize, u8, u8); 12] = [
        (0, 0, 3),
        (0, 1, 1),
        (1, 0, 2),
        (1, 1, 2),
        (2, 0, 2),
        (2, 1, 2),
        (2, 2, 3),
        (3, 0, 2),
        (3, 1, 2),
        (3, 2, 2),
        (3, 3, 2),
        (5, 0, 2),
    ];

    /// Occupancy on real pixels, every slot of every row. The panel holds
    /// 2/2/3/4/0/1 supports; the other 24 slots are empty panel. Measured
    /// stddevs with these rects: occupied 42.7-60.9, empty 1.1-2.0, so the
    /// default threshold 18.0 sits in a 20× gap — this test is what keeps that
    /// claim honest if the rect derivation moves.
    #[test]
    fn occupancy_on_the_reference_panel_finds_exactly_the_twelve_real_cells() {
        let img = fixture();
        let g = MercGeometry::default();
        let expected: Vec<(usize, u8)> = OCCUPIED_CELLS.iter().map(|&(r, s, _)| (r, s)).collect();

        let mut found = Vec::new();
        for row in 0..ROW_CENTRES.len() {
            for slot in 0..g.max_slots {
                if occupied(&img, cell(row, slot), &g) {
                    found.push((row, slot));
                }
            }
        }

        assert_eq!(found, expected);
    }

    /// Occupancy stops at the first empty slot (D2 step 4), so the run must be
    /// CONTIGUOUS from slot 0 — a gap would silently truncate a row's supports
    /// in the live loop.
    #[test]
    fn every_occupied_run_on_the_reference_panel_starts_at_slot_zero() {
        let img = fixture();
        let g = MercGeometry::default();

        for row in 0..ROW_CENTRES.len() {
            let occ: Vec<bool> = (0..g.max_slots)
                .map(|slot| occupied(&img, cell(row, slot), &g))
                .collect();
            let first_empty = occ.iter().position(|&o| !o).unwrap_or(occ.len());
            assert!(
                occ[first_empty..].iter().all(|&o| !o),
                "row {row} has an occupied slot after an empty one: {occ:?}",
            );
        }
    }

    /// The badge reader on all twelve real cells, tier by tier. Three II
    /// badges on art bright enough to bleed into the corner (rows 3-4) are in
    /// here on purpose — that is the case the shape rule exists for.
    #[test]
    fn read_tier_reads_every_badge_on_the_reference_panel() {
        let img = fixture();
        let g = MercGeometry::default();

        let read: Vec<(usize, u8, Option<u8>)> = OCCUPIED_CELLS
            .iter()
            .map(|&(row, slot, _)| (row, slot, read_tier(&img, cell(row, slot), &g)))
            .collect();

        let expected: Vec<(usize, u8, Option<u8>)> = OCCUPIED_CELLS
            .iter()
            .map(|&(row, slot, tier)| (row, slot, Some(tier)))
            .collect();
        assert_eq!(read, expected);
    }

    /// All three tiers really are covered — otherwise the test above could
    /// pass while the reader only ever answers II.
    #[test]
    fn the_reference_panel_covers_all_three_tiers() {
        let img = fixture();
        let g = MercGeometry::default();

        let mut tiers: Vec<u8> = OCCUPIED_CELLS
            .iter()
            .filter_map(|&(row, slot, _)| read_tier(&img, cell(row, slot), &g))
            .collect();
        tiers.sort_unstable();
        tiers.dedup();

        assert_eq!(tiers, [1, 2, 3]);
    }

    /// An empty slot has no badge. Reading one would put a tier on a support
    /// that is not there.
    #[test]
    fn read_tier_returns_none_for_an_empty_slot() {
        let img = fixture();
        let g = MercGeometry::default();

        assert_eq!(read_tier(&img, cell(4, 0), &g), None, "the Skitterbots row");
        assert_eq!(read_tier(&img, cell(0, 3), &g), None, "an empty slot in row 1");
    }

    /// Paint gold bars into a dark cell — the shape the reader accepts — so
    /// the negative tests below can perturb one property at a time.
    fn synthetic_badge(bars: &[(i32, i32)], top: i32, bottom: i32) -> DynamicImage {
        let mut img = RgbaImage::from_pixel(64, 64, Rgba([20, 18, 16, 255]));
        for &(x0, w) in bars {
            for x in x0..x0 + w {
                for y in top..bottom {
                    img.put_pixel(x as u32, y as u32, Rgba([255, 215, 142, 255]));
                }
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    /// The synthetic control: three gold bars on one baseline inside a 44 px
    /// cell read as III. Without it, the negative tests below could pass by
    /// accident (a reader that always says None passes every negative).
    #[test]
    fn three_aligned_gold_bars_read_as_tier_three() {
        let g = MercGeometry::default();
        // Cell [0,0,44,44] → inner 2..42; badge box x 20..42, y 25..39;
        // bars 8 px tall on a common baseline inside it.
        let img = synthetic_badge(&[(24, 1), (29, 1), (34, 1)], 29, 37);

        assert_eq!(read_tier(&img, [0, 0, 44, 44], &g), Some(3));
    }

    /// Art bleeding into the badge corner must not become a fourth stroke.
    /// A bar that does not reach the numerals' baseline is not one of them.
    #[test]
    fn a_bar_off_the_common_baseline_is_rejected() {
        let g = MercGeometry::default();
        let img = synthetic_badge(&[(24, 1), (29, 1), (34, 1)], 29, 37);
        assert_eq!(read_tier(&img, [0, 0, 44, 44], &g), Some(3), "precondition");

        // Same three bars, one lifted 4 px off the baseline.
        let mut lifted = synthetic_badge(&[(24, 1), (29, 1)], 29, 37);
        let third = synthetic_badge(&[(34, 1)], 25, 33);
        for y in 0..64u32 {
            for x in 0..64u32 {
                if third.get_pixel(x, y).0[0] > 200 {
                    let p = third.get_pixel(x, y);
                    lifted.as_mut_rgba8().unwrap().put_pixel(x, y, p);
                }
            }
        }

        assert_eq!(read_tier(&lifted, [0, 0, 44, 44], &g), None);
    }

    /// A wide art blob is not a numeral: the widest stroke may be at most
    /// twice the narrowest.
    #[test]
    fn a_blob_much_wider_than_its_neighbour_is_rejected() {
        let g = MercGeometry::default();
        let img = synthetic_badge(&[(24, 1), (29, 8)], 29, 37);

        assert_eq!(read_tier(&img, [0, 0, 44, 44], &g), None);
    }

    /// A stroke shorter than `min_height_frac` of the cell is noise, not a
    /// numeral. The bars here are 5 px — long enough to fill the scanline band
    /// (so the column test passes them and the HEIGHT floor is what has to
    /// reject them) but under the 6 px floor a 44 px cell sets.
    #[test]
    fn strokes_too_short_to_be_numerals_are_rejected() {
        let g = MercGeometry::default();
        let img = synthetic_badge(&[(24, 1), (29, 1)], 30, 35);

        assert_eq!(read_tier(&img, [0, 0, 44, 44], &g), None);
    }

    /// …and the floor is not so high that a real numeral trips it: the same
    /// bars one px taller read as II. Together these bracket the floor, so
    /// deleting the check OR raising it past a real glyph fails one of them.
    #[test]
    fn strokes_just_over_the_height_floor_still_read() {
        let g = MercGeometry::default();
        let img = synthetic_badge(&[(24, 1), (29, 1)], 30, 36);

        assert_eq!(read_tier(&img, [0, 0, 44, 44], &g), Some(2));
    }

    /// Art debris DETACHED above a stroke must not count toward its height.
    /// The reference panel's tier-III cell has exactly this — serif specks and
    /// icon bleed sitting 1-3 px above two of the three strokes — so a stroke
    /// measured by its ink EXTENT rather than by its longest contiguous run
    /// reports mismatched heights and throws the whole badge away.
    #[test]
    fn a_detached_speck_above_a_stroke_does_not_inflate_its_height() {
        let g = MercGeometry::default();
        let mut img = synthetic_badge(&[(24, 1), (29, 1), (34, 1)], 31, 37).to_rgba8();
        // One px of gold, 5 rows clear of the middle bar's top.
        img.put_pixel(29, 25, Rgba([255, 215, 142, 255]));

        assert_eq!(read_tier(&DynamicImage::ImageRgba8(img), [0, 0, 44, 44], &g), Some(3));
    }

    /// A single tall bar of gold art is not a tier I badge. The ratio rules
    /// cannot say so — there is no second stroke to compare against — so this
    /// is entirely on `max_height_frac`. Without it the reader answers Some(1)
    /// for any bright gold vertical in the corner, and tier I resolves to a
    /// real, confidently-named support.
    #[test]
    fn one_tall_gold_bar_is_not_a_tier_one_badge() {
        let g = MercGeometry::default();
        // Spans the whole badge box (25..39 for a 44 px cell): 14 px against a
        // real numeral's 8.
        let img = synthetic_badge(&[(29, 1)], 25, 39);

        assert_eq!(read_tier(&img, [0, 0, 44, 44], &g), None);
    }

    /// …and a single FAT bar is not one either — the other half of the n = 1
    /// judgement, on `max_width_frac`.
    #[test]
    fn one_wide_gold_bar_is_not_a_tier_one_badge() {
        let g = MercGeometry::default();
        let img = synthetic_badge(&[(24, 8)], 29, 37);

        assert_eq!(read_tier(&img, [0, 0, 44, 44], &g), None);
    }

    /// The caps must not reject the real thing: the reference panel's genuine
    /// tier I (row 1 slot 1) still reads. Without this, setting either cap
    /// tight enough to reject everything would pass the two tests above.
    #[test]
    fn the_real_tier_one_badge_still_reads_under_the_absolute_caps() {
        let img = fixture();
        let g = MercGeometry::default();

        assert_eq!(read_tier(&img, cell(0, 1), &g), Some(1));
    }

    /// Four or more strokes is not a roman tier — the vocabulary stops at III.
    #[test]
    fn four_strokes_are_rejected_rather_than_clamped_to_three() {
        let g = MercGeometry::default();
        let img = synthetic_badge(&[(20, 1), (25, 1), (30, 1), (35, 1)], 29, 37);

        assert_eq!(read_tier(&img, [0, 0, 44, 44], &g), None);
    }

    /// The mask is gold-specific: a blue-white icon highlight of the same
    /// brightness and the same shape is not a badge.
    #[test]
    fn white_bars_in_the_badge_corner_are_not_ink() {
        let g = MercGeometry::default();
        let mut img = RgbaImage::from_pixel(64, 64, Rgba([20, 18, 16, 255]));
        for &(x0, w) in &[(24, 1), (29, 1), (34, 1)] {
            for x in x0..x0 + w {
                for y in 29..37 {
                    img.put_pixel(x as u32, y as u32, Rgba([250, 250, 255, 255]));
                }
            }
        }

        assert_eq!(read_tier(&DynamicImage::ImageRgba8(img), [0, 0, 44, 44], &g), None);
    }

    /// A cell rect running off the image reads nothing rather than panicking
    /// on an out-of-bounds pixel.
    #[test]
    fn read_tier_on_an_off_image_rect_is_none() {
        let g = MercGeometry::default();
        let img = synthetic_badge(&[(24, 1)], 29, 37);

        assert_eq!(read_tier(&img, [40, 40, 44, 44], &g), None);
        assert_eq!(read_tier(&img, [-4, 0, 44, 44], &g), None);
    }

    // -- signatures and the template store ---------------------------------

    fn sig_of(img: &DynamicImage, row: usize, slot: u8) -> CellSig {
        normalize_cell(img, cell(row, slot), &MercGeometry::default())
            .unwrap_or_else(|| panic!("row {row} slot {slot} normalizes"))
    }

    /// Every alignment of a fixture cell — what the matcher takes.
    fn cands_of(img: &DynamicImage, row: usize, slot: u8) -> CellCandidates {
        cell_candidates(img, cell(row, slot), &MercGeometry::default())
            .unwrap_or_else(|| panic!("row {row} slot {slot} normalizes"))
    }

    /// A signature correlates perfectly with itself — the identity the whole
    /// matcher rests on. Not a tautology: it exercises the normalization (a
    /// wrong divisor or an unmasked cell mismatch drops it off 1.0).
    #[test]
    fn a_signature_correlates_at_one_with_itself() {
        let img = fixture();
        let sig = sig_of(&img, 0, 0);

        assert!((sig.ncc(&sig) - 1.0).abs() < 1e-4, "self NCC was {}", sig.ncc(&sig));
    }

    /// The fixture's twelve cells, grouped by the art they actually show —
    /// verified by eye at 6× on a labelled contact sheet. Three pairs repeat:
    /// a silver gem at tier III and tier II (rows 1/3 slot 0), a red gem on
    /// two rows (rows 2/6 slot 0), and a blue double-orb on two rows (rows
    /// 3/4 slot 1). Everything else is its own art.
    const SAME_ART_GROUPS: [&[(usize, u8)]; 3] =
        [&[(0, 0), (2, 0)], &[(1, 0), (5, 0)], &[(2, 1), (3, 1)]];

    fn same_art(a: (usize, u8), b: (usize, u8)) -> bool {
        SAME_ART_GROUPS
            .iter()
            .any(|g| g.contains(&a) && g.contains(&b))
    }

    /// Score every one of the 66 cell pairs, returning
    /// `(worst different-art score, its pair, best same-art score, its pair)`.
    fn ncc_extremes(img: &DynamicImage) -> (f32, ((usize, u8), (usize, u8)), f32, ((usize, u8), (usize, u8))) {
        let cells: Vec<(usize, u8)> = OCCUPIED_CELLS.iter().map(|&(r, s, _)| (r, s)).collect();
        let (mut cross_max, mut cross_pair) = (f32::NEG_INFINITY, ((0, 0), (0, 0)));
        let (mut within_min, mut within_pair) = (f32::INFINITY, ((0, 0), (0, 0)));
        for (i, &a) in cells.iter().enumerate() {
            for &b in &cells[i + 1..] {
                let score = sig_of(img, a.0, a.1).ncc(&sig_of(img, b.0, b.1));
                if same_art(a, b) {
                    if score < within_min {
                        within_min = score;
                        within_pair = (a, b);
                    }
                } else if score > cross_max {
                    cross_max = score;
                    cross_pair = (a, b);
                }
            }
        }
        (cross_max, cross_pair, within_min, within_pair)
    }

    /// MATCH's floor: the WORST case over all 66 pairs — the closest pair of
    /// genuinely different icons must still sit below it. Measured 0.5746
    /// under format 2, row 3 slot 0 (a plain silver gem) against row 4 slot 2
    /// (a silver gem under crossed golden shafts): two supports, one palette.
    ///
    /// Version 1 scored that same pair at 0.8552 — under the threshold, but
    /// only just, and on the whole 61-crop corpus its equivalent went over.
    /// The disc mask is what moved it: the silver both icons share is mostly
    /// frame, and the frame is now outside the correlation.
    #[test]
    fn the_closest_pair_of_different_icons_stays_below_the_match_threshold() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;

        let (cross_max, pair, _, _) = ncc_extremes(&img);

        assert!(
            cross_max < t.icon_match,
            "different icons {pair:?} correlated at {cross_max}, at or above MATCH {}",
            t.icon_match,
        );
    }

    /// MATCH's ceiling: the WORST case again, from the other side — the
    /// loosest pair of cells showing the SAME art must still clear it.
    /// Measured 0.9442 under format 2, row 3 slot 1 against row 4 slot 1 (one
    /// blue double-orb, a px of rect misalignment apart).
    ///
    /// With the test above this brackets `icon_match` into the measured
    /// 0.5746..0.9442 band — moving it below 0.5746 or above 0.9442 fails one
    /// of the two. The band is the REFERENCE PANEL's, at scale 1.0 and a 34 px
    /// alignment window; the corpus module's band is the live panel's, at
    /// 0.974 and 33 px, and the two are deliberately separate constants
    /// measured on separate art.
    #[test]
    fn the_loosest_pair_of_identical_icons_clears_the_match_threshold() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;

        let (_, _, within_min, pair) = ncc_extremes(&img);

        assert!(
            within_min >= t.icon_match,
            "the same art {pair:?} correlated at only {within_min}, under MATCH {}",
            t.icon_match,
        );
    }

    /// Art is shared across tiers for at least one family: rows 1 and 3 slot 0
    /// hold the same silver gem at tier III and tier II and correlate at 0.99.
    /// This is the measurement behind "the tier comes from the badge, not from
    /// the template" — a family learned at one tier recognises the other.
    #[test]
    fn one_familys_art_is_the_same_at_two_different_tiers() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let g = MercGeometry::default();
        assert_eq!(read_tier(&img, cell(0, 0), &g), Some(3), "precondition: tier III");
        assert_eq!(read_tier(&img, cell(2, 0), &g), Some(2), "precondition: tier II");

        let score = sig_of(&img, 0, 0).ncc(&sig_of(&img, 2, 0));

        assert!(score >= t.icon_match, "same art at two tiers correlated at only {score}");
    }

    /// A learned template recognises its own cell, and reports the family —
    /// the confirm→recognise loop D5 bootstraps.
    #[test]
    fn a_learned_template_matches_the_cell_it_was_learned_from() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);

        let m = store.match_family(&cands_of(&img, 1, 0), &t);

        assert_eq!(m.state, ReadState::Matched, "match was {m:?}");
        assert_eq!(m.family.as_deref(), Some("Chain"));
        assert_eq!(m.learned_tier, Some(2));
    }

    /// The alignment tests below all learn ONE template from row 1 slot 0 and
    /// then probe a cell rect nudged off it. The nudge stands in for what the
    /// live loop actually produces: `geometry::detect` builds the rect from a
    /// fractional column origin and pitch, so the same art lands 1-3 px
    /// differently in different cells of one panel.
    fn store_of_one(img: &DynamicImage) -> TemplateStore {
        let mut store = TemplateStore::new();
        store.learn(
            "Chain",
            2,
            sig_of(img, 1, 0),
            None,
            &MercGeometry::default().thresholds,
        );
        store
    }

    /// The cell rect of row 1 slot 0, nudged `dx` px right.
    fn nudged(dx: i32) -> [i32; 4] {
        let mut rect = cell(1, 0);
        rect[0] += dx;
        rect
    }

    fn match_nudged(img: &DynamicImage, store: &TemplateStore, dx: i32) -> IconMatch {
        let g = MercGeometry::default();
        let cands = cell_candidates(img, nudged(dx), &g)
            .unwrap_or_else(|| panic!("the cell nudged {dx} px still normalizes"));
        store.match_family(&cands, &g.thresholds)
    }

    /// THE learn-path invariant: the alignment a confirmation stores is the
    /// UNSHIFTED one.
    ///
    /// `read.rs` caches `cell_candidates(..).into_centre()` and `run.rs` hands
    /// that to `store.learn`, so this equality is what makes a learned
    /// template the same derivation the pool, the server and
    /// [`normalize_cell`] all agree on. Store an ALIGNED best instead and the
    /// template carries this capture's rect jitter: it would match the cell it
    /// came from and drift against every other sample of the same art, and
    /// nothing downstream could tell.
    #[test]
    fn the_alignment_a_confirmation_learns_is_the_unshifted_one() {
        let img = fixture();

        let learned = cands_of(&img, 1, 0).into_centre();

        assert_eq!(learned, sig_of(&img, 1, 0));
    }

    /// A cell carries every one of the 49 alignments. Nine of them are the
    /// coarse stage's; the other forty are what the fine stage is for, and a
    /// build loop that stopped early would silently shrink the search range
    /// the whole design rests on.
    #[test]
    fn a_cell_carries_all_forty_nine_alignments() {
        let img = fixture();

        let cands = cands_of(&img, 1, 0);

        assert_eq!(cands.all().len(), (SHIFT_SPAN * SHIFT_SPAN) as usize);
    }

    /// And they are genuinely different windows, not 49 copies.
    ///
    /// The negative half of the test above: a builder that ignored `dx`/`dy`
    /// would still produce 49 entries, and every alignment test in this file
    /// would keep passing while the matcher had stopped aligning.
    #[test]
    fn the_shifted_alignments_are_not_copies_of_the_unshifted_one() {
        let img = fixture();
        let centre = sig_of(&img, 1, 0);

        let cands = cands_of(&img, 1, 0);

        let distinct = cands.all().iter().filter(|s| **s != centre).count();
        assert_eq!(
            distinct,
            cands.all().len() - 1,
            "only {distinct} of {} alignments differ from the unshifted one",
            cands.all().len(),
        );
    }

    /// Two pixels of rect jitter — the common case — must not cost the match.
    /// Under version 1 this scored 0.45-0.70 and the cell read as unknown,
    /// which is why a confirmed support still had to be hovered in every other
    /// row it appeared in.
    #[test]
    fn a_learned_template_matches_its_cell_shifted_two_pixels() {
        let img = fixture();
        let store = store_of_one(&img);

        let m = match_nudged(&img, &store, 2);

        assert_eq!(m.state, ReadState::Matched, "match was {m:?}");
        assert_eq!(m.family.as_deref(), Some("Chain"));
    }

    /// Three pixels is the edge of the measured jitter and the edge of
    /// [`SHIFT_MAX`] — the alignment window is sized so the worst observed
    /// offset is still inside it, not merely most of them.
    #[test]
    fn a_learned_template_matches_its_cell_shifted_three_pixels() {
        let img = fixture();
        let store = store_of_one(&img);

        let m = match_nudged(&img, &store, 3);

        assert_eq!(m.state, ReadState::Matched, "match was {m:?}");
        assert_eq!(m.family.as_deref(), Some("Chain"));
    }

    /// Five pixels is past the window, and the search must NOT reach it.
    ///
    /// The negative half of the alignment contract. A search that slid far
    /// enough to recover this would also be sliding onto the NEIGHBOURING
    /// cell's art at the panel's pitch of 49 px against a cell of 44 — so
    /// "recovering" a 5 px miss is how a matcher starts reporting the support
    /// in the next slot. Measured 0.8247: `LowConfidence`, which asks for a
    /// hover, rather than `Matched`.
    #[test]
    fn a_learned_template_does_not_match_its_cell_shifted_five_pixels() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let store = store_of_one(&img);

        let m = match_nudged(&img, &store, 5);

        assert_ne!(m.state, ReadState::Matched, "match was {m:?}");
        assert!(m.score < t.icon_match, "scored {} at 5 px", m.score);
    }

    /// A cell whose family was never confirmed stays unknown. Handing it the
    /// nearest learned family would name a support the player does not have.
    #[test]
    fn an_unlearned_icon_does_not_borrow_the_nearest_family() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);

        let m = store.match_family(&cands_of(&img, 2, 2), &t);

        assert_eq!(m.state, ReadState::Unknown, "match was {m:?}");
        assert!(m.family.is_none());
    }

    /// An empty store answers nothing — the first-run state, before any
    /// hover-confirm.
    #[test]
    fn an_empty_store_matches_nothing() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;

        let m = TemplateStore::new().match_family(&cands_of(&img, 0, 0), &t);

        assert_eq!(m.state, ReadState::Unknown);
        assert_eq!(m.score, 0.0);
    }

    /// Two tiers of ONE family are not competition: the lead rule measures
    /// against a different family, so a family learned at both tiers must
    /// still match, not fall to low-confidence.
    ///
    /// The same art at two tiers of one family is the NORMAL case — a support
    /// keeps its icon across tiers — so the mislabelled-pair refusal
    /// (`best_other_family`) has to exclude on the family alone. A refusal keyed
    /// on the whole `(family, tier)` would make the second tier unlearnable and
    /// leave this match unreachable, which is why the second `learn` is checked
    /// here and not only the match it feeds.
    #[test]
    fn two_tiers_of_the_same_family_do_not_cancel_each_others_lead() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 1, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);

        let second = store.learn("Chain", 3, sig_of(&img, 1, 0), None, &t);

        assert_eq!(second, LearnOutcome::Stored, "a second tier of one family is not a collision");
        assert_eq!(store.len(), 2);
        let m = store.match_family(&cands_of(&img, 1, 0), &t);
        assert_eq!(m.state, ReadState::Matched, "match was {m:?}");
        assert_eq!(m.family.as_deref(), Some("Chain"));
    }

    /// The store is keyed on the PAIR: learning a second tier of a family adds
    /// an entry rather than replacing the first.
    #[test]
    fn learning_a_second_tier_of_a_family_adds_a_second_template() {
        let img = fixture();
        let mut store = TemplateStore::new();

        store.learn("Chain", 1, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);
        store.learn("Chain", 3, sig_of(&img, 2, 2), None, &MercGeometry::default().thresholds);

        assert_eq!(store.len(), 2);
        assert_eq!(store.learned_keys(), ["Chain--1", "Chain--3"]);
    }

    /// A confirmed sample is never overwritten. A second confirm of the same
    /// art is refused; art the stored sample does not reach is ADDED as a
    /// further sample, and the first stays — so a mistimed hover cannot
    /// replace good art, and good art can repair a mistimed first sample.
    #[test]
    fn relearning_a_known_key_keeps_the_first_sample() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);
        let first = store.get("Chain", 2).unwrap().sig.clone();

        let same = store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);
        assert_eq!(same, LearnOutcome::AlreadyKnown, "the same art again is refused");
        assert_eq!(store.len(), 1);

        let accepted = store.learn("Chain", 2, sig_of(&img, 2, 2), None, &t);
        assert_eq!(
            accepted,
            LearnOutcome::Stored,
            "unreached art is added as a second sample",
        );
        assert_eq!(store.len(), 2);
        assert_eq!(store.learned_keys(), ["Chain--2"], "one key, two samples");
        assert_eq!(
            store.match_family(&cands_of(&img, 2, 2), &t).family.as_deref(),
            Some("Chain"),
            "the second sample is matched"
        );

        for _ in 0..3 {
            store.learn("Chain", 2, sig_of(&img, 0, 0), None, &t);
        }
        assert_eq!(store.len(), TemplateStore::MAX_SAMPLES_PER_KEY, "capped per key");
        assert_eq!(store.get("Chain", 2).unwrap().sig, first, "the first sample stays");
    }

    /// Forget is the un-poison path — it must remove the named key and only
    /// that key, so a wrong tier-1 confirmation does not cost the tier-3 one.
    #[test]
    fn forget_removes_only_the_named_key() {
        let img = fixture();
        let mut store = TemplateStore::new();
        store.learn("Chain", 1, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);
        store.learn("Chain", 3, sig_of(&img, 2, 2), None, &MercGeometry::default().thresholds);

        assert!(store.forget("Chain", 1));

        assert_eq!(store.learned_keys(), ["Chain--3"]);
    }

    /// Forgetting something absent is a no-op that reports it, so the command
    /// can tell the page nothing happened.
    #[test]
    fn forgetting_an_unknown_key_reports_false() {
        let mut store = TemplateStore::new();

        assert!(!store.forget("Chain", 1));
    }

    /// Reset clears everything — the "start over" the page offers when the
    /// store has gone wrong in more than one place.
    #[test]
    fn reset_empties_the_store() {
        let img = fixture();
        let mut store = TemplateStore::new();
        store.learn("Chain", 1, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);
        store.learn("Pierce", 3, sig_of(&img, 2, 2), None, &MercGeometry::default().thresholds);

        store.reset();

        assert!(store.is_empty());
        assert!(store.learned_keys().is_empty());
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "poe-merc-icons-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// The store survives a restart: a saved template still matches the cell
    /// it was learned from after a round-trip through disk. Family names carry
    /// spaces, so the slug/index mapping is part of what this pins.
    #[test]
    fn a_saved_store_round_trips_and_still_matches() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let dir = temp_dir("roundtrip");
        let mut store = TemplateStore::new();
        store.learn("Caustic Conversion", 3, sig_of(&img, 2, 2), None, &MercGeometry::default().thresholds);
        store.save(&dir).expect("save");

        let (loaded, problems) = TemplateStore::load(&dir);

        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(loaded.learned_keys(), ["Caustic Conversion--3"]);
        let m = loaded.match_family(&cands_of(&img, 2, 2), &t);
        assert_eq!(m.state, ReadState::Matched, "match was {m:?}");
        assert_eq!(m.family.as_deref(), Some("Caustic Conversion"));
    }

    /// A missing store directory is an empty store, not an error: that is
    /// every first run.
    #[test]
    fn loading_a_missing_store_yields_an_empty_one() {
        let dir = temp_dir("absent").join("never-created");

        let (store, problems) = TemplateStore::load(&dir);

        assert!(store.is_empty());
        assert!(problems.is_empty());
    }

    /// One corrupt template must not cost the others: the bad entry is
    /// reported and skipped, the good one loads.
    #[test]
    fn a_corrupt_template_file_is_reported_and_skipped() {
        let img = fixture();
        let dir = temp_dir("corrupt");
        let mut store = TemplateStore::new();
        store.learn("Chain", 1, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);
        store.learn("Pierce", 3, sig_of(&img, 2, 2), None, &MercGeometry::default().thresholds);
        store.save(&dir).expect("save");
        std::fs::write(dir.join("chain--t1.png"), b"not a png").expect("corrupt one file");

        let (loaded, problems) = TemplateStore::load(&dir);

        assert_eq!(loaded.learned_keys(), ["Pierce--3"]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("chain--t1.png"), "{problems:?}");
    }

    /// Signatures come from the icon, not from the badge: the badge corner is
    /// masked, so painting a different numeral into a cell must not move its
    /// signature.
    #[test]
    fn the_masked_badge_corner_does_not_reach_the_signature() {
        let img = fixture();
        let rect = cell(1, 0);
        let base = sig_of(&img, 1, 0);

        let mut repainted = img.to_rgba8();
        let inner = inner_rect(rect, &MercGeometry::default());
        // Strictly inside the masked corner: the 40→24 resample blends
        // neighbours, so a repaint flush against the mask edge would bleed one
        // row/column outside it.
        for y in inner[1] + (inner[3] * 3) / 4..inner[1] + inner[3] {
            for x in inner[0] + (inner[2] * 2) / 3..inner[0] + inner[2] {
                repainted.put_pixel(x as u32, y as u32, Rgba([255, 215, 142, 255]));
            }
        }
        let repainted = DynamicImage::ImageRgba8(repainted);

        let after = normalize_cell(&repainted, rect, &MercGeometry::default()).expect("normalizes");
        assert_eq!(after, base);
    }

    /// An empty slot has no signature: it must not become a template that then
    /// "recognises" every other empty slot. Both fixture empties are real
    /// panel pixels, not a synthetic constant — their stddev is 1.1-2.0, well
    /// under the 18.0 gate but well over zero, so a naive
    /// "reject only a perfectly flat crop" guard would let them through.
    #[test]
    fn an_empty_slot_produces_no_signature() {
        let img = fixture();
        let g = MercGeometry::default();

        assert!(
            normalize_cell(&img, cell(4, 0), &g).is_none(),
            "the empty Skitterbots row must not normalize",
        );
        assert!(
            normalize_cell(&img, cell(0, 3), &g).is_none(),
            "an empty slot in an occupied row must not normalize either",
        );
    }

    /// The occupancy gate is a threshold, not a hard-coded shape: lowering it
    /// under an empty slot's own stddev lets that slot through. This is what
    /// makes the JSON override able to recalibrate the gate — and what would
    /// fail if the gate were re-hard-coded.
    #[test]
    fn lowering_the_occupancy_gate_lets_an_empty_slot_normalize() {
        let img = fixture();
        let mut g = MercGeometry::default();
        assert!(normalize_cell(&img, cell(4, 0), &g).is_none(), "precondition");

        g.thresholds.empty_cell_stddev = 0.5;

        assert!(normalize_cell(&img, cell(4, 0), &g).is_some());
    }

    /// An off-image rect produces no signature rather than panicking.
    #[test]
    fn an_off_image_rect_produces_no_signature() {
        let img = fixture();

        assert!(normalize_cell(&img, [580, 290, 44, 44], &MercGeometry::default()).is_none());
        assert!(normalize_cell(&img, [-10, 10, 44, 44], &MercGeometry::default()).is_none());
    }

    /// The `learned_families` wire format, pinned character for character:
    /// the page splits on the LAST `--` and parses the tail as the tier, so
    /// this shape is what makes `merc_forget_template(family, tier)`
    /// reachable from the UI. A family with a space in it is the case that
    /// rules out a plain space or single-dash separator.
    #[test]
    fn learned_keys_are_family_double_dash_tier_sorted() {
        let img = fixture();
        let mut store = TemplateStore::new();
        store.learn("Return", 3, sig_of(&img, 2, 2), None, &MercGeometry::default().thresholds);
        store.learn("Caustic Conversion", 1, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);

        let keys = store.learned_keys();

        assert_eq!(keys, ["Caustic Conversion--1", "Return--3"]);
        // The page's half of the contract: split on the last `--`, parse the
        // tail, and the two pieces must round-trip to the store's own key.
        for key in &keys {
            let (family, tier) = key.rsplit_once("--").expect("every key carries a separator");
            let tier: u8 = tier.parse().expect("the tail parses as a tier");
            assert!(
                store.get(family, tier).is_some(),
                "{key:?} did not round-trip back to a stored template",
            );
        }
    }

    /// The separator is only unambiguous while no family name contains a
    /// hyphen or ends in a digit. Checked against the whole shipped
    /// vocabulary, so a future re-fetch that introduces one fails HERE rather
    /// than by silently mis-parsing a forget request.
    #[test]
    fn no_vocabulary_family_can_collide_with_the_key_separator() {
        use crate::mercenary::vocab::{MercRole, MercVocab};
        let v = MercVocab::load().expect("vocabulary parses");

        let offenders: Vec<&str> = v
            .by_role(MercRole::Support)
            .filter(|s| {
                s.family.contains('-') || s.family.ends_with(|c: char| c.is_ascii_digit())
            })
            .map(|s| s.family.as_str())
            .collect();

        assert!(offenders.is_empty(), "families that break the parse: {offenders:?}");
    }

    /// Family names become file names; the slug must survive punctuation and
    /// never collapse to nothing.
    #[test]
    fn slugs_are_file_safe() {
        assert_eq!(slug("Caustic Conversion"), "caustic-conversion");
        assert_eq!(slug("Exposure on Hit"), "exposure-on-hit");
        assert_eq!(slug(""), "unnamed");
    }

    // -----------------------------------------------------------------------
    // POE-201 — the shared pool merge
    // -----------------------------------------------------------------------

    fn pooled(family: &str, tier: u8, sig: CellSig) -> PooledSample {
        PooledSample {
            family: family.to_string(),
            tier,
            sig,
        }
    }

    fn corpus(samples: Vec<PooledSample>, tombstones: Vec<(&str, u8)>) -> PooledCorpus {
        PooledCorpus {
            format_version: super::super::sync::FORMAT_VERSION,
            samples,
            tombstones: tombstones
                .into_iter()
                .map(|(f, t)| (f.to_string(), t))
                .collect(),
        }
    }

    /// The pool teaches a key this device never hovered — the whole point of
    /// the feature, and the acceptance criterion "only new icons need a hover".
    #[test]
    fn merging_adds_art_the_store_has_never_seen() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();

        let out = store.merge_pulled(
            &corpus(vec![pooled("Chain", 2, sig_of(&img, 1, 0))], vec![]),
            &[],
            &t,
        );

        assert_eq!(out.added, 1);
        assert_eq!(store.learned_keys(), ["Chain--2"]);
        assert_eq!(
            store.match_family(&cands_of(&img, 1, 0), &t).family.as_deref(),
            Some("Chain"),
            "a pooled sample matches exactly like a hovered one"
        );
    }

    /// A pooled sample the store already holds is not stored twice. Deduped by
    /// the SAME correlation `learn` uses, so a device does not accumulate three
    /// copies of its own upload coming back.
    #[test]
    fn merging_skips_art_the_store_already_holds() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);

        let out = store.merge_pulled(
            &corpus(vec![pooled("Chain", 2, sig_of(&img, 1, 0))], vec![]),
            &[],
            &t,
        );

        assert_eq!(out.skipped, 1);
        assert_eq!(out.added, 0);
        assert_eq!(store.len(), 1);
    }

    /// The cap applies to the MERGED set, not to what the pull brought: a key
    /// already at three samples takes none, whatever the corpus offers.
    #[test]
    fn merging_respects_the_per_key_cap() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);
        store.learn("Chain", 2, sig_of(&img, 2, 2), None, &t);
        store.learn("Chain", 2, sig_of(&img, 0, 0), None, &t);
        assert_eq!(store.len(), TemplateStore::MAX_SAMPLES_PER_KEY);

        let out = store.merge_pulled(
            &corpus(vec![pooled("Chain", 2, sig_of(&img, 3, 0))], vec![]),
            &[],
            &t,
        );

        assert_eq!(out.added, 0);
        assert_eq!(out.skipped, 1);
        assert_eq!(store.len(), TemplateStore::MAX_SAMPLES_PER_KEY);
    }

    /// A tombstoned key is REPLACED: the local sample goes and the served one
    /// takes its place. This is the edge case "device A tombstones key K,
    /// device B learned K offline" — B loses the sample on its next pull.
    #[test]
    fn a_tombstoned_key_replaces_the_local_samples_with_the_served_ones() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);

        let out = store.merge_pulled(
            &corpus(
                vec![pooled("Chain", 2, sig_of(&img, 2, 2))],
                vec![("Chain", 2)],
            ),
            &[],
            &t,
        );

        assert_eq!(out.replaced, 1, "the local sample was dropped");
        assert_eq!(out.added, 1, "the served one took its place");
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.match_family(&cands_of(&img, 2, 2), &t).family.as_deref(),
            Some("Chain"),
            "the served art is what matches now"
        );
    }

    /// A tombstone with nothing left to serve empties the key — the forget
    /// reaching a second device.
    #[test]
    fn a_tombstoned_key_with_no_served_samples_is_emptied() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);

        store.merge_pulled(&corpus(vec![], vec![("Chain", 2)]), &[], &t);

        assert!(store.learned_keys().is_empty());
    }

    /// A tombstone names ONE key. The neighbouring tier is a different market
    /// of art and must survive.
    #[test]
    fn a_tombstone_leaves_the_other_tiers_of_the_family_alone() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);
        store.learn("Chain", 3, sig_of(&img, 2, 2), None, &t);

        store.merge_pulled(&corpus(vec![], vec![("Chain", 2)]), &[], &t);

        assert_eq!(store.learned_keys(), ["Chain--3"]);
    }

    /// A key this device forgot whose tombstone has not been acknowledged is
    /// skipped entirely — otherwise the corpus, which still serves the
    /// disowned art, would undo the forget on every module start.
    #[test]
    fn a_key_awaiting_its_tombstone_takes_nothing_from_the_pool() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();

        let out = store.merge_pulled(
            &corpus(vec![pooled("Chain", 2, sig_of(&img, 1, 0))], vec![]),
            &[("Chain".to_string(), 2)],
            &t,
        );

        assert_eq!(out.suppressed, 1);
        assert_eq!(out.added, 0);
        assert!(store.is_empty());
    }

    /// A suppressed key must not have its LOCAL samples cleared either: the
    /// user forgot one key, and a pull is not licence to touch what they kept.
    #[test]
    fn a_key_awaiting_its_tombstone_keeps_its_local_samples() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);

        store.merge_pulled(
            &corpus(vec![], vec![("Chain", 2)]),
            &[("Chain".to_string(), 2)],
            &t,
        );

        assert_eq!(store.learned_keys(), ["Chain--2"]);
    }

    /// Signatures from two format versions do not correlate, so a foreign
    /// corpus must reach the matcher as nothing at all — not "mostly".
    #[test]
    fn a_corpus_from_another_format_version_is_never_merged() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);
        // BOTH directions. A newer server is the case that will happen next;
        // an OLDER one is the case that is happening now — format-1 rows are
        // still in the pool, and a client that merged them would be pulling
        // luma signatures into an RGB matcher.
        for version in [
            super::super::sync::FORMAT_VERSION - 1,
            super::super::sync::FORMAT_VERSION + 1,
        ] {
            let mut store = store.clone();
            let foreign = PooledCorpus {
                format_version: version,
                samples: vec![pooled("Pierce", 1, sig_of(&img, 2, 2))],
                tombstones: vec![("Chain".to_string(), 2)],
            };

            let out = store.merge_pulled(&foreign, &[], &t);

            assert!(out.foreign_version, "format {version} was merged");
            assert_eq!(out.added, 0, "format {version}");
            assert_eq!(out.replaced, 0, "format {version}: a foreign tombstone must not delete either");
            assert_eq!(store.learned_keys(), ["Chain--2"], "format {version}");
        }
    }

    /// Provenance is what tells the page "you taught this" from "the pool did",
    /// and what keeps a pooled sample from being offered back to the pool it
    /// came from.
    #[test]
    fn a_merged_sample_is_marked_pooled_and_is_never_offered_back() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();

        store.merge_pulled(
            &corpus(vec![pooled("Chain", 2, sig_of(&img, 1, 0))], vec![]),
            &[],
            &t,
        );

        assert_eq!(store.pooled_keys(), ["Chain--2"]);
        assert_eq!(store.pooled_samples(), 1);
        assert!(
            store.pending_uploads().is_empty(),
            "a pooled sample is not this device's to publish"
        );
    }

    /// A key the user hovered is theirs even after the pool adds a second
    /// sample to it — the chip answers "did I teach this", not "does the pool
    /// have it too".
    #[test]
    fn a_key_with_a_local_sample_is_not_listed_as_pooled() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);

        store.merge_pulled(
            &corpus(vec![pooled("Chain", 2, sig_of(&img, 2, 2))], vec![]),
            &[],
            &t,
        );

        assert_eq!(store.len(), 2, "the pool added a second sample");
        assert!(store.pooled_keys().is_empty());
        assert_eq!(store.pooled_samples(), 1);
    }

    /// A hover-learned sample is owed to the pool until it is placed.
    #[test]
    fn a_freshly_learned_sample_is_owed_to_the_pool() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);

        let pending = store.pending_uploads();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, "Chain");
        assert_eq!(pending[0].1, 2);
        assert_eq!(pending[0].2.len(), SIG_BYTES, "the wire payload is the signature");
    }

    /// Marking is per SAMPLE, not per key: three samples offered in three
    /// requests must not be closed out by the first acknowledgement.
    #[test]
    fn marking_one_sample_uploaded_leaves_the_keys_other_samples_owed() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);
        store.learn("Chain", 2, sig_of(&img, 2, 2), None, &t);
        let first = sig_of(&img, 1, 0).bytes().to_vec();

        assert!(store.mark_uploaded("Chain", 2, &first));

        let owed = store.pending_uploads();
        assert_eq!(owed.len(), 1, "the second sample is still owed");
        assert_ne!(owed[0].2, first);
    }

    /// Acknowledging something the store does not hold changes nothing and
    /// says so, so the uploader does not write the index for a no-op.
    #[test]
    fn marking_a_sample_the_store_does_not_hold_reports_false() {
        let img = fixture();
        let mut store = TemplateStore::new();
        store.learn(
            "Chain",
            2,
            sig_of(&img, 1, 0),
            None,
            &MercGeometry::default().thresholds,
        );

        assert!(!store.mark_uploaded("Chain", 2, &[0u8; SIG_BYTES]));
    }

    /// Provenance and the upload flag survive a restart. Without this an app
    /// restart re-offers the whole store to the pool and marks every pooled
    /// sample as the user's own work.
    #[test]
    fn provenance_and_the_upload_flag_survive_a_save_and_load() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let dir = temp_dir("provenance");
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &t);
        store.merge_pulled(
            &corpus(vec![pooled("Pierce", 1, sig_of(&img, 2, 2))], vec![]),
            &[],
            &t,
        );
        store.mark_uploaded("Chain", 2, &sig_of(&img, 1, 0).bytes().to_vec());
        store.save(&dir).expect("save");

        let (loaded, problems) = TemplateStore::load(&dir);

        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(loaded.pooled_keys(), ["Pierce--1"]);
        assert!(
            loaded.pending_uploads().is_empty(),
            "an acknowledged sample is not re-offered after a restart"
        );
    }

    /// An unplaced sample stays owed across a restart — that is the durable
    /// retry, and it is why the uploader may drop a batch it could not place.
    #[test]
    fn an_unplaced_sample_is_still_owed_after_a_restart() {
        let img = fixture();
        let dir = temp_dir("still-owed");
        let mut store = TemplateStore::new();
        store.learn(
            "Chain",
            2,
            sig_of(&img, 1, 0),
            None,
            &MercGeometry::default().thresholds,
        );
        store.save(&dir).expect("save");

        let (loaded, _) = TemplateStore::load(&dir);

        assert_eq!(loaded.pending_uploads().len(), 1);
    }

    /// An `index.json` written before the shared pool existed must load as
    /// "mine, and the pool has never seen it" — the state that makes the first
    /// module start after the upgrade publish the store instead of disowning
    /// it.
    #[test]
    fn a_bare_array_index_is_read_as_format_one_and_loads_nothing() {
        let img = fixture();
        let dir = temp_dir("legacy-index");
        let mut store = TemplateStore::new();
        store.learn(
            "Chain",
            2,
            sig_of(&img, 1, 0),
            None,
            &MercGeometry::default().thresholds,
        );
        store.save(&dir).expect("save");
        // Exactly the shape POE-165 shipped: a bare array, no version. The PNG
        // beside it is a REAL format-2 signature, so the emptiness this test
        // asserts comes from the version and not from an unreadable file.
        std::fs::write(
            dir.join("index.json"),
            r#"[{"family":"Chain","tier":2,"file":"chain--t2.png"}]"#,
        )
        .expect("rewrite the index in the old shape");

        let (loaded, problems) = TemplateStore::load(&dir);

        assert!(loaded.is_empty(), "loaded {:?}", loaded.learned_keys());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].contains("format 1") && problems[0].contains("1 template(s)"),
            "the problem must name the version and the count: {problems:?}",
        );
    }

    /// The other half of the version gate: an index the CURRENT build wrote
    /// loads its samples. Without this the test above passes just as well
    /// against a `load` that reads nothing at all.
    #[test]
    fn an_index_of_this_format_version_loads_its_samples() {
        let img = fixture();
        let dir = temp_dir("v2-index");
        let mut store = TemplateStore::new();
        store.learn(
            "Chain",
            2,
            sig_of(&img, 1, 0),
            None,
            &MercGeometry::default().thresholds,
        );
        store.save(&dir).expect("save");

        let (loaded, problems) = TemplateStore::load(&dir);

        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(loaded.learned_keys(), ["Chain--2"]);
        assert!(loaded.pooled_keys().is_empty(), "a hovered sample is the user's own");
        assert_eq!(loaded.pending_uploads().len(), 1, "and is owed to the pool");
    }

    /// An index from a version this build does not read is reported, not
    /// silently half-loaded — the same rule as the bare array, reached by the
    /// other door (a FUTURE version, where the shape parses and the number
    /// does not match).
    #[test]
    fn an_index_from_a_later_format_version_loads_nothing() {
        let img = fixture();
        let dir = temp_dir("future-index");
        let mut store = TemplateStore::new();
        store.learn(
            "Chain",
            2,
            sig_of(&img, 1, 0),
            None,
            &MercGeometry::default().thresholds,
        );
        store.save(&dir).expect("save");
        let raw = std::fs::read_to_string(dir.join("index.json")).expect("read");
        std::fs::write(
            dir.join("index.json"),
            raw.replace(
                &format!("\"formatVersion\": {}", super::super::sync::FORMAT_VERSION),
                &format!("\"formatVersion\": {}", super::super::sync::FORMAT_VERSION + 1),
            ),
        )
        .expect("rewrite the version");

        let (loaded, problems) = TemplateStore::load(&dir);

        assert!(loaded.is_empty(), "loaded {:?}", loaded.learned_keys());
        assert_eq!(problems.len(), 1, "{problems:?}");
    }

    // -- the directory write lock -------------------------------------------

    /// The two writers of one directory — the loop's off-tick `SaveQueue`
    /// worker and `sync`'s corpus merge — must not be inside `save` at the same
    /// time. The store's own mutex does not answer this: the worker drops it
    /// before writing. Composed as a handshake rather than with sleeps, so the
    /// overlap window is real and the test is not timing-dependent: the sync
    /// write is asked for while the worker is provably still inside its own.
    #[test]
    fn a_pool_merge_cannot_write_the_directory_while_the_off_tick_worker_is_in_it() {
        use std::sync::mpsc::channel;
        use std::sync::{Arc, Mutex};
        use std::time::Duration;
        let lock = Arc::new(Mutex::new(()));
        let order = Arc::new(Mutex::new(Vec::new()));
        let (worker_in_tx, worker_in_rx) = channel::<()>();
        let (release_tx, release_rx) = channel::<()>();
        let (merge_done_tx, merge_done_rx) = channel::<()>();

        let worker = {
            let (lock, order) = (lock.clone(), order.clone());
            std::thread::spawn(move || {
                writing_icons_dir(&lock, || {
                    worker_in_tx.send(()).ok();
                    release_rx.recv().ok();
                    order.lock().unwrap().push("worker");
                })
            })
        };
        worker_in_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the worker is inside its write");
        let merge = {
            let (lock, order) = (lock.clone(), order.clone());
            std::thread::spawn(move || {
                writing_icons_dir(&lock, || {
                    order.lock().unwrap().push("merge");
                    merge_done_tx.send(()).ok();
                })
            })
        };

        assert!(
            merge_done_rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "the merge wrote the directory while the worker was still in it",
        );
        release_tx.send(()).ok();
        merge_done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the merge writes once the worker is out");
        worker.join().expect("worker");
        merge.join().expect("merge");
        assert_eq!(*order.lock().unwrap(), ["worker", "merge"]);
    }

    // -- the format-2 mask and the store's version gate -----------------------

    /// The mask's kept-position count, derived from the FORMULA rather than
    /// from the implementation.
    ///
    /// 219 positions (657 channels) is a wire contract, not an implementation
    /// detail: [`CellSig::ncc`] returns 0.0 — a silent non-match, not an error
    /// — when two signatures disagree about it, so a desktop and a server that
    /// count differently would produce a pool where nothing ever matches and
    /// nothing ever complains. The server pins the same number from its own
    /// independent derivation (`internal/mercenary/signature_test.go`).
    #[test]
    fn the_format_two_mask_keeps_two_hundred_and_nineteen_positions() {
        let radius = 0.36 * SIG_DIM as f32;
        let centre = SIG_DIM as f32 / 2.0;
        let badge_x0 = SIG_DIM - (SIG_DIM as f32 * 0.45).round() as u32;
        let badge_y0 = SIG_DIM - (SIG_DIM as f32 * 0.35).round() as u32;

        let kept = (0..SIG_DIM)
            .flat_map(|y| (0..SIG_DIM).map(move |x| (x, y)))
            .filter(|&(x, y)| !(x >= badge_x0 && y >= badge_y0))
            .filter(|&(x, y)| {
                let (dx, dy) = (x as f32 + 0.5 - centre, y as f32 + 0.5 - centre);
                dx.hypot(dy) <= radius
            })
            .count();

        assert_eq!(kept, 219, "the formula keeps {kept} positions");
        let img = fixture();
        assert_eq!(sig_of(&img, 1, 0).active(), kept * SIG_CHANNELS);
    }

    /// A signature is refused unless it is exactly [`SIG_BYTES`] long — the
    /// version-1 length included.
    ///
    /// This is the boundary the format bump rests on: a 576-byte payload is a
    /// version-1 signature, and decoding it as a short version-2 one would
    /// build a signature over a disc that covers most of the buffer and
    /// correlate it against real ones at whatever the arithmetic happened to
    /// produce.
    #[test]
    fn a_signature_of_the_wrong_length_is_refused() {
        for len in [576usize, SIG_BYTES - 1, SIG_BYTES + 1] {
            let bytes: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();

            assert!(
                CellSig::from_rgb(bytes).is_none(),
                "{len} bytes was accepted as a signature",
            );
        }
    }

    /// A round trip must reproduce the NORMALISED values, not just the PNG.
    ///
    /// The bytes are what a reload rebuilds from, but the correlation runs on
    /// the floats, and equality of the floats is what says the rebuild
    /// produced the same signature rather than a same-looking one. Compared
    /// through `ncc` at exactly 1.0 with the divisor asserted too, because two
    /// signatures with different `active` counts correlate at 0.0 and would
    /// otherwise fail this test for an unrelated reason.
    #[test]
    fn a_reloaded_template_reproduces_the_signature_it_was_saved_from() {
        let img = fixture();
        let dir = temp_dir("roundtrip-values");
        let original = sig_of(&img, 2, 2);
        let mut store = TemplateStore::new();
        store.learn(
            "Caustic Conversion",
            3,
            original.clone(),
            None,
            &MercGeometry::default().thresholds,
        );
        store.save(&dir).expect("save");

        let (loaded, problems) = TemplateStore::load(&dir);

        assert!(problems.is_empty(), "{problems:?}");
        let reloaded = &loaded.templates()[0].sig;
        assert_eq!(reloaded.active(), original.active());
        assert_eq!(reloaded.bytes(), original.bytes());
        assert!(
            (reloaded.ncc(&original) - 1.0).abs() < 1e-6,
            "reloaded correlated with the original at {}",
            reloaded.ncc(&original),
        );
    }

    // -- the format-1 purge ---------------------------------------------------

    /// A store from format 1 is unlinked whole — index, signature PNGs and the
    /// `-raw.png` colour crops beside them.
    ///
    /// The raw crops are in scope on purpose: they are not indexed, so nothing
    /// else would ever remove them, and they are the art the dead signatures
    /// were derived from.
    #[test]
    fn purging_a_format_one_store_removes_every_png() {
        let dir = temp_dir("purge-v1");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(
            dir.join("index.json"),
            r#"[{"family":"Chain","tier":2,"file":"chain--t2.png"},
                {"family":"Pierce","tier":1,"file":"pierce--t1.png"}]"#,
        )
        .expect("write a version-1 index");
        for file in ["chain--t2.png", "pierce--t1.png", "chain--t2-raw.png"] {
            image::GrayImage::new(24, 24).save(dir.join(file)).expect("write a png");
        }
        std::fs::write(dir.join("pool-sync.json"), "{}").expect("write the sync file");

        let dropped = purge_stale_store(&dir);

        assert_eq!(
            dropped,
            Some(PurgedStore { version: Some(1), dropped: 2 }),
            "the log line names the version it read and what it dropped",
        );
        let left: Vec<String> = std::fs::read_dir(&dir)
            .expect("read the dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".png"))
            .collect();
        assert!(left.is_empty(), "png left behind: {left:?}");
        assert!(dir.join("pool-sync.json").exists(), "the sync file is not ours to delete");
        let (store, problems) = TemplateStore::load(&dir);
        assert!(store.is_empty());
        assert!(problems.is_empty(), "the purged index reads clean: {problems:?}");
    }

    /// A store this build wrote is left alone — every file, and the index
    /// unchanged.
    ///
    /// The purge runs on EVERY module start, so "recognises its own store" is
    /// the property that keeps it from being a wipe-on-launch. `merc_reset_
    /// templates` leans on the same thing: it writes an empty format-2 index,
    /// and the PNGs its doc promises are still on disk survive because of this.
    #[test]
    fn purging_leaves_a_store_of_this_format_version_untouched() {
        let img = fixture();
        let dir = temp_dir("purge-v2");
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);
        store.save(&dir).expect("save");
        let before = std::fs::read_to_string(dir.join("index.json")).expect("read");

        let dropped = purge_stale_store(&dir);

        assert_eq!(dropped, None);
        assert!(dir.join("chain--t2.png").exists(), "the signature was unlinked");
        assert_eq!(
            std::fs::read_to_string(dir.join("index.json")).expect("read"),
            before,
        );
    }

    /// An empty format-2 index — exactly what `merc_reset_templates` writes —
    /// is not a stale store, so a reset does not turn into a delete on the
    /// next module start.
    #[test]
    fn purging_leaves_a_reset_store_untouched() {
        let dir = temp_dir("purge-reset");
        std::fs::create_dir_all(&dir).expect("mkdir");
        TemplateStore::new().save(&dir).expect("save an empty store");
        image::GrayImage::new(24, 24)
            .save(dir.join("chain--t2.png"))
            .expect("a forgotten sample's png stays on disk");

        let dropped = purge_stale_store(&dir);

        assert_eq!(dropped, None);
        assert!(dir.join("chain--t2.png").exists(), "a reset must not delete the pngs");
    }

    /// A directory with no index at all is a first run, not a stale store.
    #[test]
    fn purging_a_store_that_was_never_written_does_nothing() {
        let dir = temp_dir("purge-absent");
        std::fs::create_dir_all(&dir).expect("mkdir");

        assert_eq!(purge_stale_store(&dir), None);
    }

    /// The parity golden: the ONE artefact both sides of the format-2
    /// signature are checked against (POE-207).
    ///
    /// `internal/mercenary/testdata/merc-sig-v2/crop.png` is a real GGG cell
    /// crop — the Multistrike support at tier 3, the 39×39 inner rect the
    /// desktop cuts at `cell_inset` 2 and the live scale 0.974.
    /// `signature.bin` is the 1728 bytes it must reduce to. Go derives it in
    /// `parity_golden_test.go`; this derives it through the production
    /// [`normalize_cell`]. Neither side generates the file for the other in a
    /// normal run — both compare against the same committed bytes, so a port
    /// that drifts fails on the side that drifted.
    ///
    /// Regenerate deliberately, never to make this pass:
    ///
    /// ```text
    /// MERC_SIG_UPDATE=1 docker compose run --rm -w /app/desktop/src-tauri \
    ///     desktop cargo test --lib parity
    /// ```
    mod parity {
        use super::*;

        const DIR: &str = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../internal/mercenary/testdata/merc-sig-v2"
        );
        const UPDATE_ENV: &str = "MERC_SIG_UPDATE";

        /// Whether this run is regenerating the golden rather than checking
        /// it. Both tests ask, because they share the file.
        fn updating() -> bool {
            std::env::var(UPDATE_ENV).as_deref() == Ok("1")
        }

        /// Replace the golden atomically: write a sibling temp file, then
        /// rename over it.
        ///
        /// `fs::write` truncates and then fills, so a reader that arrives in
        /// between — the sibling test, the Go suite, an interrupted run —
        /// sees a short or empty file and reports a format error for a golden
        /// that is fine. A rename inside one directory is atomic on every
        /// platform this repo builds for.
        fn write_golden(path: &std::path::Path, bytes: &[u8]) {
            std::fs::create_dir_all(DIR).expect("create the golden directory");
            let tmp = path.with_extension("bin.tmp");
            std::fs::write(&tmp, bytes).expect("write the golden");
            std::fs::rename(&tmp, path).expect("swap the golden in");
        }

        /// The committed crop, run through the whole production derivation:
        /// mounted as a cell, inner rect, alignment window, resize, mask,
        /// normalise.
        fn derived() -> CellSig {
            let path = std::path::Path::new(DIR).join("crop.png");
            let crop = image::open(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
                .to_rgba8();
            assert_eq!(
                crop.dimensions(),
                (39, 39),
                "the parity crop is not a 39×39 inner rect",
            );
            let mut canvas = RgbaImage::from_pixel(43, 43, Rgba([0, 0, 0, 255]));
            image::imageops::replace(&mut canvas, &crop, 2, 2);
            normalize_cell(
                &DynamicImage::ImageRgba8(canvas),
                [0, 0, 43, 43],
                &MercGeometry::default(),
            )
            .expect("the parity crop normalizes")
        }

        #[test]
        fn the_committed_crop_reduces_to_the_committed_signature() {
            let got = derived().bytes().to_vec();
            assert_eq!(got.len(), SIG_BYTES);
            let golden_path = std::path::Path::new(DIR).join("signature.bin");

            if updating() {
                write_golden(&golden_path, &got);
                panic!(
                    "{UPDATE_ENV}=1: rewrote {} ({} bytes). Re-run without the flag to verify, \
                     and re-run the Go parity test before committing.",
                    golden_path.display(),
                    got.len(),
                );
            }

            let want = std::fs::read(&golden_path).unwrap_or_else(|e| {
                panic!(
                    "read {} (generate it with {UPDATE_ENV}=1): {e}",
                    golden_path.display()
                )
            });
            assert_eq!(
                want.len(),
                SIG_BYTES,
                "{} is not a format-2 signature",
                golden_path.display(),
            );
            if got != want {
                let first = got
                    .iter()
                    .zip(&want)
                    .position(|(a, b)| a != b)
                    .expect("lengths already agree");
                let (position, channel) = (first / SIG_CHANNELS, first % SIG_CHANNELS);
                let (x, y) = (position as u32 % SIG_DIM, position as u32 / SIG_DIM);
                let differing = got.iter().zip(&want).filter(|(a, b)| a != b).count();
                panic!(
                    "derived signature differs from {} in {differing}/{SIG_BYTES} bytes; first at \
                     {first} (position ({x},{y}), channel {channel}, masked={}): got {}, want {}",
                    golden_path.display(),
                    masked(x, y),
                    got[first],
                    want[first],
                );
            }
        }

        /// The golden is a signature, not a blob: it decodes, it carries the
        /// format's divisor, and it correlates with a fresh derivation at
        /// exactly 1.0. A golden that byte-matched but could not be decoded
        /// would still be useless to every device that pulled it.
        #[test]
        fn the_golden_signature_decodes_and_self_correlates() {
            if updating() {
                // The sibling test is mid-rewrite of this very file. Reading
                // it now races that write, and the answer would be about a
                // half-written golden either way.
                return;
            }
            let raw = match std::fs::read(std::path::Path::new(DIR).join("signature.bin")) {
                Ok(raw) => raw,
                Err(e) => panic!("read the golden (generate it with {UPDATE_ENV}=1): {e}"),
            };
            let golden = CellSig::from_rgb(raw).expect("the golden decodes as a signature");

            assert_eq!(golden.active(), 657);
            assert!(
                (golden.ncc(&derived()) - 1.0).abs() < 1e-6,
                "the golden correlated with a fresh derivation at {}",
                golden.ncc(&derived()),
            );
        }
    }

    // -----------------------------------------------------------------------
    // The 61-crop corpus (POE-207)
    // -----------------------------------------------------------------------

    /// Sebastian's whole `merc-icons` store as it stood on 2026-08-27: the 61
    /// raw 39×39 colour crops the samples were learned from, with a manifest
    /// naming each one's family, its tier, and whether the confirmation that
    /// produced it was WRONG.
    ///
    /// This is the only many-icon ground truth the module has — the committed
    /// reference panel carries twelve cells and three repeated arts, which is
    /// not enough to say anything about cross-family separation. Every number
    /// the format-2 derivation was chosen on was measured here.
    mod corpus {
        use super::*;

        const DIR: &str = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/merc-icon-crops"
        );

        #[derive(serde::Deserialize)]
        struct Manifest {
            crops: Vec<Crop>,
        }

        #[derive(serde::Deserialize)]
        struct Crop {
            file: String,
            family: String,
            /// The tier the confirmation was filed under. Carried into the
            /// store the equivalence tests build, so the corpus exercises the
            /// real `(family, tier)` keyspace rather than a flattened one.
            tier: u8,
            #[serde(default)]
            poisoned: bool,
            #[serde(default)]
            why: Option<String>,
        }

        fn manifest() -> Manifest {
            let raw = std::fs::read_to_string(std::path::Path::new(DIR).join("manifest.json"))
                .expect("the corpus manifest is committed next to the crops");
            serde_json::from_str(&raw).expect("the corpus manifest parses")
        }

        /// The crops a matcher may learn from — the mislabels excluded.
        fn clean() -> Vec<Crop> {
            manifest().crops.into_iter().filter(|c| !c.poisoned).collect()
        }

        /// Mount a raw crop in a cell the production geometry reads.
        ///
        /// The crops ARE inner rects: 39×39, which is `cell_size` 43 at the
        /// live scale 0.974 minus 2 px of `cell_inset` per side. Pasting one
        /// into a 43×43 canvas and handing `[0, 0, 43, 43]` to the production
        /// entry points is what makes these numbers the numbers the loop gets
        /// — occupancy gate, inner rect, alignment window and all — rather
        /// than numbers from a private derivation that only the test knows.
        fn mounted(file: &str) -> DynamicImage {
            mount(crop_of(file))
        }

        /// One committed 39×39 inner crop, as pixels.
        fn crop_of(file: &str) -> RgbaImage {
            let crop = image::open(std::path::Path::new(DIR).join(file))
                .unwrap_or_else(|e| panic!("{file}: {e}"))
                .to_rgba8();
            assert_eq!(
                crop.dimensions(),
                (39, 39),
                "{file} is not a 39×39 inner crop",
            );
            crop
        }

        /// Paste an inner crop into the 43×43 cell canvas [`mounted`]
        /// describes. Split out so a SYNTHESISED crop reaches the production
        /// entry points through exactly the same door a committed one does.
        fn mount(crop: RgbaImage) -> DynamicImage {
            let mut canvas = RgbaImage::from_pixel(43, 43, Rgba([0, 0, 0, 255]));
            image::imageops::replace(&mut canvas, &crop, 2, 2);
            DynamicImage::ImageRgba8(canvas)
        }

        /// `weight` of `a`'s pixels over `(1 − weight)` of `b`'s.
        ///
        /// The corpus holds no clean cross-family pair inside
        /// `[icon_low, icon_match)` — the whole point of the format-2
        /// derivation is that real different-family art tops out at 0.759
        /// unshifted — so the near-miss the refusal must not swallow has to be
        /// SYNTHESISED. A pixel blend is the honest way to make one: it moves a
        /// real icon towards another real icon along a continuum, so the
        /// resulting score is a real correlation between two real arts rather
        /// than a number written into a fixture.
        fn blended(a: &str, b: &str, weight: f32) -> DynamicImage {
            let (pa, pb) = (crop_of(a), crop_of(b));
            let mut out = pa.clone();
            for (x, y, px) in out.enumerate_pixels_mut() {
                let other = pb.get_pixel(x, y);
                for c in 0..3 {
                    px[c] = (px[c] as f32 * weight + other[c] as f32 * (1.0 - weight))
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
            }
            mount(out)
        }

        const OUTER: [i32; 4] = [0, 0, 43, 43];

        /// Every alignment of one crop — a probe.
        fn probe(file: &str) -> CellCandidates {
            cell_candidates(&mounted(file), OUTER, &MercGeometry::default())
                .unwrap_or_else(|| panic!("{file} normalizes"))
        }

        /// The unshifted signature of one crop — a stored template.
        fn template(file: &str) -> CellSig {
            normalize_cell(&mounted(file), OUTER, &MercGeometry::default())
                .unwrap_or_else(|| panic!("{file} normalizes"))
        }

        /// Derived once: the crops, their signatures, and the full
        /// probe × template score matrix. 40 crops × 49 alignments is the
        /// expensive part and every test below reads the same numbers out of
        /// it, so it is paid once per test binary rather than once per test.
        struct Corpus {
            crops: Vec<Crop>,
            probes: Vec<CellCandidates>,
            templates: Vec<CellSig>,
            /// `scores[i][j]`: crop `i` as a cell, crop `j` as a template.
            scores: Vec<Vec<f32>>,
        }

        impl Corpus {
            /// One number per unordered pair — the better of the two
            /// directions. Either crop could be the learned one, so a pair is
            /// only as bad as its better direction.
            fn pair(&self, i: usize, j: usize) -> f32 {
                self.scores[i][j].max(self.scores[j][i])
            }

            /// Indices of the same-family pairs, `i < j`.
            fn same_family_pairs(&self) -> Vec<(usize, usize)> {
                let mut out = Vec::new();
                for i in 0..self.crops.len() {
                    for j in i + 1..self.crops.len() {
                        if self.crops[i].family == self.crops[j].family {
                            out.push((i, j));
                        }
                    }
                }
                out
            }

            /// For each crop, the best score it reaches against a template of
            /// any OTHER family, and that template's index.
            fn best_other_family(&self) -> Vec<(usize, usize, f32)> {
                (0..self.crops.len())
                    .map(|i| {
                        let (mut at, mut best) = (i, f32::NEG_INFINITY);
                        for j in 0..self.crops.len() {
                            if self.crops[i].family == self.crops[j].family {
                                continue;
                            }
                            if self.scores[i][j] > best {
                                best = self.scores[i][j];
                                at = j;
                            }
                        }
                        (i, at, best)
                    })
                    .collect()
            }

            /// A store holding every clean crop EXCEPT `probe` — what the
            /// device would hold when it meets that cell for the first time.
            fn store_without(&self, probe: usize) -> TemplateStore {
                let mut store = TemplateStore::new();
                for (i, crop) in self.crops.iter().enumerate() {
                    if i == probe {
                        continue;
                    }
                    // `push_sample`, not `learn`: `learn` refuses a sample a
                    // stored one already matches, and the corpus deliberately
                    // holds near-duplicate samples of one family.
                    store.push_sample(
                        &crop.family,
                        crop.tier,
                        self.templates[i].clone(),
                        None,
                        Origin::Local,
                        false,
                    );
                }
                store
            }
        }

        fn corpus() -> &'static Corpus {
            static CORPUS: std::sync::OnceLock<Corpus> = std::sync::OnceLock::new();
            CORPUS.get_or_init(|| {
                let crops = clean();
                let probes: Vec<CellCandidates> = crops.iter().map(|c| probe(&c.file)).collect();
                let templates: Vec<CellSig> =
                    crops.iter().map(|c| template(&c.file)).collect();
                let scores = probes
                    .iter()
                    .map(|p| templates.iter().map(|t| best_over(t, p.all())).collect())
                    .collect();
                Corpus {
                    crops,
                    probes,
                    templates,
                    scores,
                }
            })
        }

        /// The manifest names every file in the directory, and the directory
        /// holds nothing the manifest does not name.
        ///
        /// Every band in this module is measured over "the crops the manifest
        /// calls clean". A stray PNG dropped into the fixture would be
        /// invisible to all of them — it would join no pair, raise no
        /// cross-family score, and change no count — while the directory
        /// quietly stopped being the store it claims to be a copy of.
        #[test]
        fn the_manifest_and_the_fixture_directory_name_the_same_crops() {
            let mut named: Vec<String> =
                manifest().crops.into_iter().map(|c| c.file).collect();
            named.sort();

            let mut on_disk: Vec<String> = std::fs::read_dir(DIR)
                .expect("the fixture directory is committed")
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".png"))
                .collect();
            on_disk.sort();

            assert_eq!(named.len(), 61, "the corpus is Sebastian's 61-template store");
            assert_eq!(named, on_disk);
        }

        // -- the narrow-window fallback -------------------------------------

        /// The boundary `cellInset` crosses, from the safe side.
        ///
        /// At the live 43 px cell an inset of 6 leaves a 31 px inner crop and
        /// a 25 px window — over `SIG_DIM`, so all 49 alignments are still
        /// built. This is the half that says the guard is a real boundary and
        /// not a blanket "any override drops alignment".
        #[test]
        fn a_cell_inset_that_still_leaves_a_window_keeps_every_alignment() {
            let mut g = MercGeometry::default();
            g.cell_inset = 6.0;

            let cands = cell_candidates(&mounted("multistrike--t3-raw.png"), OUTER, &g)
                .expect("normalizes");

            assert_eq!(cands.all().len(), (SHIFT_SPAN * SHIFT_SPAN) as usize);
        }

        /// And from the other side: an inset of 7 leaves a 29 px inner crop
        /// and a 23 px window, under `SIG_DIM`.
        ///
        /// The matcher must FALL BACK to the unaligned inner crop, not refuse
        /// the cell and not silently resize a window smaller than the
        /// signature. That is the user's geometry, and version-1 behaviour —
        /// working badly — beats a module that reads nothing at all.
        #[test]
        fn a_cell_inset_that_leaves_no_window_falls_back_to_one_alignment() {
            let mut g = MercGeometry::default();
            g.cell_inset = 7.0;

            let cands = cell_candidates(&mounted("multistrike--t3-raw.png"), OUTER, &g)
                .expect("normalizes");

            assert_eq!(cands.all().len(), 1);
        }

        /// The fallback window is the WHOLE inner crop — same origin, same
        /// size — not a shifted sub-rect of it.
        ///
        /// Derived here from the documented rule (`inner = outer − 2·inset`)
        /// rather than read back out of `shift_window`, because the failure
        /// this guards is self-concealing: a fallback that moved its origin
        /// moves it for the template and the probe alike, so every match test
        /// still passes while the signature this device stores stops being the
        /// signature the pool and the server derive from the same pixels.
        #[test]
        fn the_unaligned_fallback_reads_the_whole_inner_crop() {
            let mut g = MercGeometry::default();
            g.cell_inset = 7.0;
            let img = mounted("multistrike--t3-raw.png");
            let inset = g.cell_inset.round() as i32;
            let side = OUTER[2] - 2 * inset;
            let crop = img
                .crop_imm(inset as u32, inset as u32, side as u32, side as u32)
                .to_rgb8();
            let resized = image::imageops::resize(
                &crop,
                SIG_DIM,
                SIG_DIM,
                image::imageops::FilterType::Triangle,
            );
            let expected = CellSig::from_rgb(resized.into_raw()).expect("normalizes");

            let got = normalize_cell(&img, OUTER, &g).expect("normalizes");

            assert_eq!(got, expected);
        }

        /// The fallback is still a working matcher: the one alignment it keeps
        /// recognises the cell it was learned from.
        ///
        /// Without this, the test above is satisfied by a fallback that
        /// produces one USELESS signature — a window of the wrong size, or the
        /// shifted origin applied to an unshiftable crop.
        #[test]
        fn the_unaligned_fallback_still_recognises_its_own_cell() {
            let mut g = MercGeometry::default();
            g.cell_inset = 7.0;
            let img = mounted("multistrike--t3-raw.png");
            let sig = normalize_cell(&img, OUTER, &g).expect("normalizes");
            let mut store = TemplateStore::new();
            store.push_sample("Multistrike", 3, sig, None, Origin::Local, false);

            let m = store.match_family(
                &cell_candidates(&img, OUTER, &g).expect("normalizes"),
                &Thresholds::default(),
            );

            assert_eq!(m.state, ReadState::Matched, "match was {m:?}");
            assert_eq!(m.family.as_deref(), Some("Multistrike"));
        }

        /// The 21 wrong templates, by name.
        ///
        /// Pinned rather than counted: the corpus is only ground truth while
        /// the manifest agrees with what was actually diagnosed on 2026-08-26
        /// (2 tooltip-text crops and 19 same-art-two-families mislabels from a
        /// cursor sweep the tooltip lagged). A count would let a later edit
        /// swap a genuinely poisoned crop for a clean one and keep passing,
        /// and every band below is measured over the complement of this list.
        const POISONED: [&str; 21] = [
            "ailment-damage--t2-3-raw.png",
            "area-of-effect--t3-2-raw.png",
            "area-of-effect--t3-raw.png",
            "brittle-chance--t3-raw.png",
            "cooldown-recovery--t2-2-raw.png",
            "cooldown-recovery--t2-3-raw.png",
            "cooldown-recovery--t2-raw.png",
            "curse-effect--t3-raw.png",
            "dot-multiplier--t2-raw.png",
            "dot-multiplier--t3-raw.png",
            "faster-attacks--t2-2-raw.png",
            "faster-attacks--t2-raw.png",
            "fork--t3-raw.png",
            "ignite-chance--t3-raw.png",
            "increased-area-of-effect--t2-2-raw.png",
            "increased-area-of-effect--t2-raw.png",
            "increased-area-of-effect--t3-raw.png",
            "less-duration--t2-raw.png",
            "multiple-projectiles--t3-raw.png",
            "physical-as-extra-chaos--t3-raw.png",
            "swift-affliction--t3-2-raw.png",
        ];

        #[test]
        fn the_manifest_marks_exactly_the_twenty_one_diagnosed_mislabels() {
            let mut marked: Vec<String> = manifest()
                .crops
                .into_iter()
                .filter(|c| c.poisoned)
                .map(|c| c.file)
                .collect();
            marked.sort();

            assert_eq!(marked, POISONED);
        }

        /// Every poisoned crop must say WHY it is poisoned. The reason is what
        /// a later reader needs to decide whether a re-diagnosis moved it, and
        /// an unexplained exclusion is indistinguishable from a crop somebody
        /// dropped to make a band pass.
        #[test]
        fn every_mislabelled_crop_carries_its_diagnosis() {
            let unexplained: Vec<String> = manifest()
                .crops
                .into_iter()
                .filter(|c| c.poisoned && c.why.as_deref().unwrap_or("").trim().is_empty())
                .map(|c| c.file)
                .collect();

            assert!(unexplained.is_empty(), "no reason given for {unexplained:?}");
        }

        /// The clean corpus's shape: 40 crops carrying 13 pairs of same-family
        /// art. Both halves of the band below are measured over exactly these,
        /// so a manifest edit that quietly drops a hard pair — the honest way
        /// to make a threshold test pass — fails here instead.
        #[test]
        fn the_clean_corpus_holds_forty_crops_in_thirteen_same_family_pairs() {
            let c = corpus();

            assert_eq!(c.crops.len(), 40);
            assert_eq!(c.same_family_pairs().len(), 13);
        }

        /// The one same-family pair format 2 does NOT resolve, named.
        ///
        /// Two "Ailment Damage" tier-2 confirms of visibly the same skull at
        /// 0.833: over `icon_low`, under `icon_match`, so the cell reads `?`
        /// and asks for a hover instead of guessing. Pinned by name and
        /// bracketed on both sides — a derivation that pushed it to `Matched`
        /// would be claiming a confidence this art does not support, and one
        /// that pushed it under `icon_low` would throw the cell away.
        #[test]
        fn the_hardest_pair_of_same_family_art_lands_in_the_low_confidence_band() {
            let c = corpus();
            let t = Thresholds::default();
            let i = index_of(c, "ailment-damage--t2-raw.png");
            let j = index_of(c, "ailment-damage--t2-2-raw.png");

            let score = c.pair(i, j);

            assert!(
                score >= t.icon_low && score < t.icon_match,
                "the Ailment Damage pair scored {score}, outside [{}, {})",
                t.icon_low,
                t.icon_match,
            );
        }

        /// Every OTHER same-family pair clears `icon_match` on its own — the
        /// half that says one hover teaches the rest of the panel. Measured
        /// weakest 0.924 (the two Fire Penetration crops).
        #[test]
        fn every_other_pair_of_same_family_art_clears_the_match_threshold() {
            let c = corpus();
            let t = Thresholds::default();
            let hardest = (
                index_of(c, "ailment-damage--t2-raw.png"),
                index_of(c, "ailment-damage--t2-2-raw.png"),
            );

            let failures: Vec<(String, String, f32)> = c
                .same_family_pairs()
                .into_iter()
                .filter(|&p| p != hardest)
                .map(|(i, j)| (c.crops[i].file.clone(), c.crops[j].file.clone(), c.pair(i, j)))
                .filter(|(_, _, s)| *s < t.icon_match)
                .collect();

            assert!(failures.is_empty(), "under MATCH {}: {failures:?}", t.icon_match);
        }

        /// The same-family half from the other side: the MEDIAN pair sits well
        /// clear of the threshold, not just over it. Measured 0.9885.
        ///
        /// This is the half a smaller disc breaks first. Masking more away
        /// raises cross-family separation (the test below keeps passing) while
        /// draining the signal that makes one family's two samples agree —
        /// without this bound, a derivation that scored every pair at 0.88 and
        /// every non-pair at 0.10 would look like an improvement.
        #[test]
        fn the_median_pair_of_same_family_art_stays_well_clear_of_the_threshold() {
            let c = corpus();
            let mut scores: Vec<f32> = c
                .same_family_pairs()
                .into_iter()
                .map(|(i, j)| c.pair(i, j))
                .collect();
            scores.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let median = scores[scores.len() / 2];

            assert!(median >= 0.95, "median same-family pair was {median}");
        }

        /// The cross-family half: no crop reaches another family close enough
        /// for the matcher to call it. `icon_match − icon_lead` is the real
        /// ceiling — a template that scored above it could be the runner-up
        /// that denies a correct winner its lead. Measured worst 0.818 against
        /// a ceiling of 0.83.
        #[test]
        fn no_crop_comes_within_the_matchers_lead_of_another_family() {
            let c = corpus();
            let t = Thresholds::default();
            let ceiling = t.icon_match - t.icon_lead;

            let over: Vec<(String, String, f32)> = c
                .best_other_family()
                .into_iter()
                .filter(|&(_, _, s)| s >= ceiling)
                .map(|(i, j, s)| (c.crops[i].file.clone(), c.crops[j].file.clone(), s))
                .collect();

            assert!(over.is_empty(), "at or over {ceiling}: {over:?}");
        }

        /// And the cross-family half from the other side: the closest pair of
        /// DIFFERENT families still correlates at 0.70+.
        ///
        /// Support icons share a palette, a frame and a lighting direction, so
        /// a derivation that put the closest unrelated pair near zero has
        /// stopped measuring the art and started measuring noise — which is
        /// exactly what a disc small enough to keep the same-family half honest
        /// would do. Measured 0.818.
        #[test]
        fn the_closest_pair_of_different_families_is_not_trivially_far_apart() {
            let c = corpus();

            let closest = c
                .best_other_family()
                .into_iter()
                .map(|(_, _, s)| s)
                .fold(f32::NEG_INFINITY, f32::max);

            assert!(closest >= 0.70, "the closest different families scored {closest}");
        }

        /// The two-stage search IS the full search, on every crop.
        ///
        /// `match_family` refines only `REFINE_K` templates per side; the
        /// other 27 keep a coarse score that under-estimates their true best.
        /// This compares it against `match_family_exhaustive` — the same
        /// verdict computed over all 49 alignments of every template — for
        /// every clean crop probed against a store of the other 39. Family,
        /// score AND runner-up, because a K too small shows up first as a
        /// runner-up that is too low, which inflates the lead before it ever
        /// changes the winner.
        #[test]
        fn the_two_stage_search_returns_the_full_searchs_verdict_for_every_crop() {
            let c = corpus();
            let t = Thresholds::default();

            let mut disagreements = Vec::new();
            for i in 0..c.crops.len() {
                let store = c.store_without(i);
                let fast = store.match_family(&c.probes[i], &t);
                let full = store.match_family_exhaustive(&c.probes[i], &t);
                if fast.family != full.family
                    || (fast.score - full.score).abs() > 1e-6
                    || (fast.runner_up - full.runner_up).abs() > 1e-6
                    || fast.state != full.state
                {
                    disagreements.push((c.crops[i].file.clone(), fast, full));
                }
            }

            assert!(disagreements.is_empty(), "{disagreements:#?}");
        }

        /// The OTHER-FAMILY half of the refinement set, isolated.
        ///
        /// The equivalence test above does not reach it: with 39 templates and
        /// `REFINE_K` 12, the plain top-12 already holds the best other-family
        /// template for every crop of this corpus, so removing the
        /// other-family pass leaves that test green. That is a property of the
        /// corpus, not of the search — one family can hold a dozen samples
        /// (three per key, and a family spans tiers, and the pool merges other
        /// devices' samples), and then the true runner-up sits outside the
        /// top-K and ONLY the other-family pass refines it.
        ///
        /// So the case is constructed: the twelve templates stage one ranks
        /// highest are relabelled into one family, which pushes the best rival
        /// past rank K in the ranking STAGE ONE ACTUALLY USES — the coarse-9
        /// maximum, not the 49-shift one. Asserted as a precondition, because
        /// ranking by the wrong score is exactly how this test would stop
        /// covering the thing it names.
        ///
        /// Then both halves of the effect: the rival's refined score is
        /// strictly higher than its coarse one (so the refinement moved
        /// something), and the two-stage verdict equals the full search's.
        /// Without the other-family pass the runner-up keeps the coarse
        /// under-estimate, the lead comes out too wide, and that is how a
        /// `LowConfidence` cell becomes a confidently wrong `Matched`.
        #[test]
        fn the_runner_up_is_refined_even_when_it_ranks_below_the_top_k() {
            let c = corpus();
            let t = Thresholds::default();
            let probe = index_of(c, "multistrike--t3-raw.png");
            // Stage one's own ranking: the coarse-9 maximum per template.
            let coarse: Vec<&CellSig> = c.probes[probe].coarse().collect();
            let mut ranked: Vec<usize> = (0..c.crops.len()).filter(|&i| i != probe).collect();
            ranked.sort_by(|&a, &b| {
                let (sa, sb) = (
                    best_over_refs(&c.templates[a], &coarse),
                    best_over_refs(&c.templates[b], &coarse),
                );
                sb.partial_cmp(&sa).unwrap().then(a.cmp(&b))
            });
            let mut store = TemplateStore::new();
            for (rank, &i) in ranked.iter().enumerate() {
                let family = if rank < TemplateStore::REFINE_K {
                    "Crowded Family".to_string()
                } else {
                    c.crops[i].family.clone()
                };
                store.push_sample(&family, 2, c.templates[i].clone(), None, Origin::Local, false);
            }

            let fast = store.match_family(&c.probes[probe], &t);
            let coarse_only = store.match_with_refinement(&c.probes[probe], &t, 0);
            let full = store.match_family_exhaustive(&c.probes[probe], &t);

            assert_eq!(
                fast.family.as_deref(),
                Some("Crowded Family"),
                "precondition: stage one's leader is the crowded family, so every \
                 rival sits outside the plain top-{}",
                TemplateStore::REFINE_K,
            );
            assert_eq!(
                coarse_only.family.as_deref(),
                Some("Crowded Family"),
                "precondition: the two runner-ups below are taken over the same family set",
            );
            assert!(
                coarse_only.runner_up < fast.runner_up - 1e-6,
                "the other-family pass changed nothing: coarse runner-up {} vs refined {}",
                coarse_only.runner_up,
                fast.runner_up,
            );
            assert_eq!(fast, full);
        }

        /// The refinement is what buys that agreement — not the coarse stage
        /// on its own. Without it the equivalence test above would pass
        /// against a `REFINE_K` of zero and prove nothing about the constant.
        ///
        /// Measured: with no refinement at all, the nine coarse alignments
        /// under-estimate at least one crop's verdict.
        #[test]
        fn the_coarse_stage_alone_does_not_reproduce_the_full_search() {
            let c = corpus();
            let t = Thresholds::default();

            let differing = (0..c.crops.len())
                .filter(|&i| {
                    let store = c.store_without(i);
                    let coarse = store.match_with_refinement(&c.probes[i], &t, 0);
                    let full = store.match_family_exhaustive(&c.probes[i], &t);
                    coarse != full
                })
                .count();

            assert!(differing > 0, "the coarse stage alone matched the full search everywhere");
        }

        /// THE PERF GATE (POE-207). Ignored by default; a RELEASE-mode run:
        ///
        /// ```text
        /// docker compose run --rm -w /app/desktop/src-tauri desktop \
        ///     cargo test --release --lib the_full_panel_match -- --ignored --nocapture
        /// ```
        ///
        /// Ignored rather than dropped because a wall-clock assertion in the
        /// ordinary suite is a flake generator — a debug build is ~20× slower
        /// and CI machines are shared — but "is the aligned search affordable"
        /// is the question the whole two-stage design answers, and an
        /// unreproducible one-off measurement in a commit body cannot be
        /// re-asked when `REFINE_K` or the pool ceiling moves.
        ///
        /// 792 templates is the pool ceiling (264 `(family, tier)` keys × 3
        /// samples per key); 12 cells is a full recruit panel. 250 ms is the
        /// budget: the detect tick runs at 2 s, and the match has to leave
        /// room for the OCR that precedes it.
        ///
        /// Measured 2026-08-27 in docker, release: build 10.8 ms + match
        /// 104.7 ms = 115.0 ms. At half the budget, `REFINE_K` stays 12; if a
        /// later change pushes this over, the documented fallback is K = 8
        /// (measured: one probe lands `LowConfidence` instead of `Matched`,
        /// never a wrong family).
        #[test]
        #[ignore = "wall-clock; release-mode only — see the doc comment"]
        fn the_full_panel_match_stays_inside_the_detect_tick_budget() {
            const BUDGET: std::time::Duration = std::time::Duration::from_millis(250);
            const POOL_CEILING: usize = 792;
            let c = corpus();
            let g = MercGeometry::default();
            let t = Thresholds::default();

            // A synthetic store at the ceiling: the corpus art, re-keyed into
            // as many families as it takes. Distinct families on purpose —
            // the runner-up scan and the other-family refinement both walk
            // them, so collapsing them would measure a cheaper search.
            let mut store = TemplateStore::new();
            while store.len() < POOL_CEILING {
                let batch = store.len() / c.crops.len();
                for (i, crop) in c.crops.iter().enumerate() {
                    if store.len() == POOL_CEILING {
                        break;
                    }
                    store.push_sample(
                        &format!("{} {batch}", crop.family),
                        crop.tier,
                        c.templates[i].clone(),
                        None,
                        Origin::Local,
                        false,
                    );
                }
            }
            assert_eq!(store.len(), POOL_CEILING);

            let img = super::fixture();
            let cells: Vec<[i32; 4]> = super::OCCUPIED_CELLS
                .iter()
                .map(|&(row, slot, _)| super::cell(row, slot))
                .collect();
            // Warm the caches the way a second detect tick would find them.
            for rect in &cells {
                let cands = cell_candidates(&img, *rect, &g).expect("normalizes");
                let _ = store.match_family(&cands, &t);
            }

            let start = std::time::Instant::now();
            for rect in &cells {
                let cands = cell_candidates(&img, *rect, &g).expect("normalizes");
                let _ = store.match_family(&cands, &t);
            }
            let elapsed = start.elapsed();

            println!(
                "PERF: {} cells × {} templates = {elapsed:?} (budget {BUDGET:?})",
                cells.len(),
                store.len(),
            );
            assert!(
                elapsed < BUDGET,
                "a full panel took {elapsed:?}, over the {BUDGET:?} budget — \
                 drop REFINE_K to 8 and re-measure the corpus tests",
            );
        }

        fn index_of(c: &Corpus, file: &str) -> usize {
            c.crops
                .iter()
                .position(|crop| crop.file == file)
                .unwrap_or_else(|| panic!("{file} is not in the clean corpus"))
        }

        // -- the mislabelled-pair refusal (POE-207 AC3) -----------------------

        /// The two crops the 2026-08-26 investigation found carrying the SAME
        /// art under two different families — the shape of 19 of the 21
        /// mislabels the purge removed.
        const SAME_ART: (&str, &str) = (
            "physical-as-extra-chaos--t3-raw.png",
            "brittle-chance--t3-raw.png",
        );

        /// The synthetic near-miss: 60% of Faster Projectiles' pixels over 40%
        /// of Concentrated Effect's, which correlates with unblended Faster
        /// Projectiles at 0.838 unshifted (measured 2026-08-27) — inside
        /// `[icon_low, icon_match)` with margin at both ends.
        ///
        /// Synthetic on purpose, and BOTH parents are clean corpus crops. No
        /// real cross-family pair in the corpus sits in that band: the
        /// format-2 derivation exists precisely to keep different families
        /// apart, and the worst clean pair reaches only 0.759 unshifted. So the
        /// near-miss the refusal must NOT swallow cannot be quoted from the
        /// fixture and has to be built, by walking one real icon towards
        /// another real icon until the correlation lands in the band.
        const NEAR_MISS: (&str, &str, f32) = (
            "faster-projectiles--t3-raw.png",
            "concentrated-effect--t3-raw.png",
            0.60,
        );

        /// A pull carrying these samples. `super::corpus` because this module
        /// has its own `corpus()` — the score matrix — and the outer one is the
        /// `PooledCorpus` builder.
        fn pull(samples: Vec<PooledSample>) -> PooledCorpus {
            super::corpus(samples, vec![])
        }

        /// A store holding one sample under one name, by the back door: `learn`
        /// is the thing under test.
        fn holding(family: &str, tier: u8, sig: CellSig) -> TemplateStore {
            let mut store = TemplateStore::new();
            store.push_sample(family, tier, sig, None, Origin::Local, false);
            store
        }

        /// THE thing POE-207 AC3 asks for. Art already filed under another
        /// family is refused outright, and the refusal names that family —
        /// storing it would leave BOTH families under the matcher's lead rule
        /// and therefore permanently unmatchable, and a later `forget` would be
        /// a guess at which of the two confirmations was the wrong one.
        #[test]
        fn art_a_different_family_already_holds_is_refused_by_name() {
            let t = Thresholds::default();
            let stored = template(SAME_ART.0);
            let confirmed = template(SAME_ART.1);
            assert!(
                stored.ncc(&confirmed) >= t.icon_match,
                "precondition: the fixture pair is the same art, scored {}",
                stored.ncc(&confirmed),
            );
            let mut store = holding("Physical as Extra Chaos", 3, stored);

            let outcome = store.learn("Brittle Chance", 3, confirmed, None, &t);

            let LearnOutcome::ConflictsWith {
                family,
                tier,
                origin,
                score,
            } = outcome
            else {
                panic!("expected a refusal, got {outcome:?}");
            };
            assert_eq!(family, "Physical as Extra Chaos");
            assert_eq!(tier, 3);
            assert_eq!(origin, Origin::Local, "the incumbent was hovered on this device");
            assert!(score >= t.icon_match, "refused on a score of only {score}");
            assert_eq!(
                store.learned_keys(),
                ["Physical as Extra Chaos--3"],
                "the mislabel must not reach the store",
            );
        }

        /// The other side of the threshold, on a SYNTHETIC near-miss (see
        /// [`NEAR_MISS`]): art that comes within `icon_low` of another family's
        /// sample but does not reach `icon_match` is still learned.
        ///
        /// This is the reason the refusal is bound to `icon_match` and not to
        /// anything looser. Support icons share a palette, a frame and a
        /// lighting direction, so neighbouring art correlates high on its own;
        /// a gate that fired in the `LowConfidence` band would start refusing
        /// honest confirmations of genuinely different supports, and the player
        /// would have no way to teach the one the module keeps calling `?`.
        #[test]
        fn a_synthetic_near_miss_of_a_different_family_is_still_learned() {
            let t = Thresholds::default();
            let (base, other, weight) = NEAR_MISS;
            let stored = template(base);
            let confirmed = normalize_cell(
                &blended(base, other, weight),
                OUTER,
                &MercGeometry::default(),
            )
            .expect("the blend still reads as an occupied cell");
            let score = stored.ncc(&confirmed);
            assert!(
                score >= t.icon_low && score < t.icon_match,
                "precondition: the blend scored {score}, outside [{}, {})",
                t.icon_low,
                t.icon_match,
            );
            let mut store = holding("Faster Projectiles", 3, stored);

            let outcome = store.learn("Concentrated Effect", 3, confirmed, None, &t);

            assert_eq!(outcome, LearnOutcome::Stored, "the blend scored {score}");
            assert_eq!(
                store.learned_keys(),
                ["Concentrated Effect--3", "Faster Projectiles--3"],
            );
        }

        /// The half the hover guard cannot reach: a wrong confirmation made on
        /// SOMEBODY ELSE'S machine, arriving through the pool. Local samples are
        /// what this player confirmed on their own screen, so the served one
        /// yields.
        #[test]
        fn a_served_sample_colliding_with_a_local_one_of_another_family_is_refused() {
            let t = Thresholds::default();
            let mut store = holding("Physical as Extra Chaos", 3, template(SAME_ART.0));

            let out = store.merge_pulled(
                &pull(vec![pooled("Brittle Chance", 3, template(SAME_ART.1))]),
                &[],
                &t,
            );

            assert_eq!(out.conflicting, 1, "the collision was not counted: {out:?}");
            assert_eq!(out.added, 0, "the served mislabel was installed: {out:?}");
            assert!(!out.changed(), "and nothing is owed to disk");
            assert_eq!(store.learned_keys(), ["Physical as Extra Chaos--3"]);
        }

        /// Two colliding samples inside ONE pull and no local sample to
        /// arbitrate: NEITHER is kept.
        ///
        /// Nothing on this device says which of the two strangers is the
        /// mislabel, so keeping the first served would make the surviving
        /// family a function of the order the server happened to list them in —
        /// a coin flip that can read `Matched` on the wrong family. The
        /// module's standing preference is to fail towards `LowConfidence`, so
        /// the cell goes back to `?` and the player's own hover settles it.
        #[test]
        fn two_colliding_served_samples_leave_neither_family_holding_the_art() {
            let t = Thresholds::default();
            let first = pooled("Physical as Extra Chaos", 3, template(SAME_ART.0));
            let second = pooled("Brittle Chance", 3, template(SAME_ART.1));

            let mut forwards = TemplateStore::new();
            let a = forwards.merge_pulled(&pull(vec![first.clone(), second.clone()]), &[], &t);
            let mut backwards = TemplateStore::new();
            let b = backwards.merge_pulled(&pull(vec![second, first]), &[], &t);

            assert!(
                forwards.is_empty(),
                "kept {:?} — the surviving family would be the server's listing order",
                forwards.learned_keys(),
            );
            assert!(
                backwards.is_empty(),
                "kept {:?} the other way round",
                backwards.learned_keys(),
            );
            assert_eq!(
                (a.added, a.conflicting),
                (0, 2),
                "(added, conflicting) served in this order: {a:?}",
            );
            assert_eq!(
                (b.added, b.conflicting),
                (0, 2),
                "(added, conflicting) served the other way round: {b:?}",
            );
            assert!(
                !a.changed(),
                "the store started empty and ended empty, so no save is owed: {a:?}",
            );
        }

        /// One art, THREE families, in every order the server could list them
        /// in — and none of the three ends up holding it.
        ///
        /// The pair rule alone does not reach this: the first two knock each
        /// other out and the third meets the store they just emptied, installs
        /// into it, and survives as a confident wrong `Matched` picked by
        /// listing order. `DoT Multiplier` / `Swift Affliction` /
        /// `Cooldown Recovery` is one of two such clusters in the committed
        /// corpus, so the shape is observed rather than imagined.
        ///
        /// `conflicting` is 3 — one per family that claimed the art: two from
        /// the collision that empties the pair, one from the third refused
        /// against the remembered art.
        #[test]
        fn a_three_way_cluster_of_served_samples_leaves_no_family_holding_the_art() {
            let t = Thresholds::default();
            let members = [
                pooled("DoT Multiplier", 2, template("dot-multiplier--t2-raw.png")),
                pooled("Swift Affliction", 3, template("swift-affliction--t3-2-raw.png")),
                pooled("Cooldown Recovery", 2, template("cooldown-recovery--t2-2-raw.png")),
            ];
            for (i, j) in [(0, 1), (0, 2), (1, 0), (1, 2), (2, 0), (2, 1)] {
                let k = 3 - i - j;
                let order = [members[i].clone(), members[j].clone(), members[k].clone()];
                let names = [&order[0].family, &order[1].family, &order[2].family];

                let mut store = TemplateStore::new();
                let out = store.merge_pulled(&pull(order.to_vec()), &[], &t);

                assert!(
                    store.is_empty(),
                    "served {names:?} and kept {:?} — the survivor is the listing order",
                    store.learned_keys(),
                );
                assert_eq!(
                    (out.added, out.conflicting),
                    (0, 3),
                    "(added, conflicting) for {names:?}: {out:?}",
                );
            }
        }

        /// A precondition for the test above, stated so its `is_empty` cannot
        /// be satisfied by three arts that never collided in the first place.
        #[test]
        fn the_three_way_cluster_really_is_one_art_under_three_families() {
            let t = Thresholds::default();
            let sigs = [
                template("dot-multiplier--t2-raw.png"),
                template("swift-affliction--t3-2-raw.png"),
                template("cooldown-recovery--t2-2-raw.png"),
            ];

            for (i, j) in [(0, 1), (0, 2), (1, 2)] {
                let score = sigs[i].ncc(&sigs[j]);
                assert!(
                    score >= t.icon_match,
                    "cluster members {i} and {j} correlate at only {score}",
                );
            }
        }

        /// The same collision, but the incumbent is one this pull did not
        /// install: it was already on disk. Both still go — and now the store
        /// really did shrink, so a save IS owed or the evicted sample comes
        /// straight back on the next start.
        #[test]
        fn evicting_a_previously_pooled_incumbent_owes_a_save() {
            let t = Thresholds::default();
            let mut store = TemplateStore::new();
            store.push_sample(
                "Physical as Extra Chaos",
                3,
                template(SAME_ART.0),
                None,
                Origin::Pooled,
                false,
            );

            let out = store.merge_pulled(
                &pull(vec![pooled("Brittle Chance", 3, template(SAME_ART.1))]),
                &[],
                &t,
            );

            assert!(store.is_empty(), "kept {:?}", store.learned_keys());
            assert_eq!(out.dropped, 1, "{out:?}");
            assert!(out.changed(), "an evicted sample that is never saved comes back: {out:?}");
        }

        /// Two evictions in ONE pull, one of a sample that was already on disk
        /// and one of a sample this pull had just installed, must be counted
        /// apart — and the boundary between "was here" and "arrived just now"
        /// slides when the first eviction removes an element from under it.
        ///
        /// Get that wrong and both numbers lie in the same breath: `dropped`
        /// claims two samples left the disk when only one did, so a save is
        /// owed for a store that is already correct, and `added` still claims
        /// an install that was undone three lines later. The two arts here are
        /// two independent collisions — one from each of the corpus's
        /// same-art pairs — so the second eviction cannot be explained by the
        /// first.
        #[test]
        fn a_pull_that_evicts_before_and_after_its_own_install_counts_them_apart() {
            let t = Thresholds::default();
            let mut store = TemplateStore::new();
            store.push_sample(
                "Physical as Extra Chaos",
                3,
                template(SAME_ART.0),
                None,
                Origin::Pooled,
                false,
            );

            let out = store.merge_pulled(
                &pull(vec![
                    // Installed: nothing in the store reaches it.
                    pooled("Area of Effect", 3, template("area-of-effect--t3-2-raw.png")),
                    // Evicts the sample that was ALREADY on disk.
                    pooled("Brittle Chance", 3, template(SAME_ART.1)),
                    // Evicts the one installed by this same pull.
                    pooled("Curse Effect", 3, template("curse-effect--t3-raw.png")),
                ]),
                &[],
                &t,
            );

            assert!(store.is_empty(), "kept {:?}", store.learned_keys());
            assert_eq!(
                (out.added, out.dropped),
                (0, 1),
                "(added, dropped) — one sample left the disk, the other never settled on it: {out:?}",
            );
            assert_eq!(out.conflicting, 4, "two collisions, two families each: {out:?}");
        }
    }
}
