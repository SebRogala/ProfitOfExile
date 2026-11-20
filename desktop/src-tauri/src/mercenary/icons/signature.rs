use image::{DynamicImage, GenericImageView, RgbImage};

use super::super::geometry::{inner_rect, occupied};
use super::super::MercGeometry;


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
/// The cell rects `geometry::detect_reason` emits land 1-3 px off per cell (the
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
pub(super) fn masked(x: u32, y: u32) -> bool {
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
    #[cfg(test)]
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
/// Takes the cell's OUTER rect (what [`super::geometry::detect_reason`] emits), reads
/// its inner region and then gives up the margin [`window_offset`] leaves —
/// [`SHIFT_MAX`] px per side up to a 48 px cell, proportionally more above it —
/// the alignment window. What is left of the cell frame after the inset is outside
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

/// How far the UNSHIFTED signature window sits inside a `side`-px cell's outer
/// edge — THE framing rule, which [`shift_window`] cuts live cells with and
/// `seed::render_cell` places seed art by.
///
/// At the reference cell ([`MercGeometry::cell_size`], 44) the window is the
/// inner crop minus [`SHIFT_MAX`] per side: `cell_inset + SHIFT_MAX` = 5 px
/// in, a 34 px window over the art. A signature correlates across screens only
/// when every screen frames the art that way, so a LARGER cell keeps the
/// fraction rather than the px: 9 px in at the 79 px cell of a 3840×2160
/// screen. Cut 5 px in there, the window spans 69 of 79 px and the art comes
/// out ~12 % smaller than in a sample cut at 1080p. Measured on a 4K capture
/// (`tests/fixtures/merc-recruit-4k.png`) at its fitted rects, against a store
/// learned from the reference and PC fixtures' labelled cells: the same art
/// scored 0.66-0.92 cut 5 px in, and 0.95-0.995 cut this way.
///
/// A SMALLER cell keeps the 5 px, because the window must leave [`SHIFT_MAX`]
/// px of alignment room inside the inset. The proportional term rounds to 5 up
/// to a 48 px cell, so the cells the committed fixtures and corpus were cut
/// from — 1080p's 40, 1200p's 43-44 — are cut exactly where they were.
pub fn window_offset(side: i32, g: &MercGeometry) -> i32 {
    let floor = g.cell_inset.round() as i32 + SHIFT_MAX;
    let proportional = (side as f32 * floor as f32 / g.cell_size).round() as i32;
    floor.max(proportional)
}

/// The UNSHIFTED signature window of a cell, and whether shifting it is safe.
///
/// The window sits [`window_offset`] px inside the cell — the inner crop
/// shrunk by [`SHIFT_MAX`] screen px per side up to a 48 px cell, which is
/// 33×33 at the live scale 0.974 and 34×34 on the 1:1 reference fixture. The
/// margin it gives up is what the matcher slides over, so the alignment room
/// is bought here rather than by growing the rect (which would pull the
/// neighbouring cell's art in at the extremes).
///
/// A `cellInset` override can make the inner crop too small to shrink. That is
/// the user's geometry, not a bug, so the signature falls back to the whole
/// inner crop and the matcher runs unaligned — the version-1 behaviour, which
/// worked badly rather than not at all. Warned once, because the loop would
/// otherwise say it on every cell of every tick.
pub(super) fn shift_window(rect: [i32; 4], g: &MercGeometry) -> ([i32; 4], bool) {
    let [ix, iy, iw, ih] = inner_rect(rect, g);
    let margin = window_offset(rect[2].min(rect[3]), g) - g.cell_inset.round() as i32;
    let (ww, wh) = (iw - 2 * margin, ih - 2 * margin);
    if ww < SIG_DIM as i32 || wh < SIG_DIM as i32 {
        NARROW_WINDOW.call_once(|| {
            log::warn!(
                "Merc: cell inner crop {iw}×{ih} leaves no room for the ±{SHIFT_MAX} px \
                 alignment window — matching unaligned (check cellInset/cellSize)"
            );
        });
        return ([ix, iy, iw, ih], false);
    }
    ([ix + margin, iy + margin, ww, wh], true)
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
/// The rects `geometry::detect_reason` emits are 1-3 px off per cell, so a template
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
    /// The `(dx, dy)` each entry of [`Self::shifted`] was built at.
    ///
    /// Carried rather than recomputed from the index, because the loop below
    /// SKIPS any shift whose window does not normalise. Off-image is not how
    /// that happens today — the occupancy gate has already put the whole inner
    /// rect inside the image, and ±[`SHIFT_MAX`] reaches exactly that rect's
    /// edges — but a flat window still returns `None`, and an inset override
    /// could move the arithmetic. One skip and `i` stops being
    /// `(i % SHIFT_SPAN, i / SHIFT_SPAN)`; a caller that re-derived it that way
    /// would then report the wrong offset with the right score, which no
    /// scoring assertion can catch. Two vectors built in one loop cannot drift.
    #[cfg(test)]
    shifts: Vec<(i32, i32)>,
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
            #[cfg(test)]
            shifts: vec![(0, 0)],
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

    /// Every alignment with the `(dx, dy)` it was taken at.
    ///
    /// The only supported way to ask "which shift won": [`Self::all`] gives no
    /// answer, and reconstructing one from the index is wrong the moment a
    /// shift is dropped (see [`Self::shifts`]). The POE-208 calibration tests
    /// read this — a mis-mapped shift there would move the measured optimum
    /// without failing anything.
    #[cfg(test)]
    pub fn aligned(&self) -> impl Iterator<Item = (&CellSig, (i32, i32))> {
        self.shifted.iter().zip(self.shifts.iter().copied())
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
    #[cfg(test)]
    let mut shifts = Vec::with_capacity((SHIFT_SPAN * SHIFT_SPAN) as usize);
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
            #[cfg(test)]
            shifts.push((dx, dy));
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
        #[cfg(test)]
        shifts,
        coarse,
        centre,
    })
}
