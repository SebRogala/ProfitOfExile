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

use std::collections::HashMap;
use std::path::Path;

use image::{DynamicImage, GenericImageView, GrayImage, RgbaImage};
use serde::{Deserialize, Serialize};

use super::geometry::{inner_rect, luma, occupied};
use super::{BadgeGeometry, MercGeometry, ReadState, Thresholds};

/// Signature side length. 24×24 keeps the icon's silhouette and colour
/// gradient while discarding the per-pixel noise a 44 px crop carries.
pub const SIG_DIM: u32 = 24;

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
/// batch is placed, and the forget/reset commands. The in-memory store's mutex
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

/// A normalized cell signature: 24×24 grayscale, zero-mean and unit-stddev
/// over its unmasked cells, with the badge corner zeroed.
#[derive(Debug, Clone, PartialEq)]
pub struct CellSig {
    /// The pre-normalization grayscale, kept so a template round-trips
    /// through a PNG on disk without storing floats. Masked positions are
    /// zeroed here too, so the saved template shows exactly the pixels that
    /// take part in the correlation and a reload reproduces the signature
    /// byte for byte.
    gray: Vec<u8>,
    /// Zero-mean unit-stddev values; exactly 0.0 at masked positions, which
    /// makes them contribute nothing to the correlation.
    norm: Vec<f32>,
    /// How many positions are unmasked — the correlation's divisor.
    active: usize,
}

/// Whether a signature cell is inside the masked badge corner.
fn masked(x: u32, y: u32) -> bool {
    let mask_x0 = SIG_DIM - (SIG_DIM as f32 * MASK_W_FRAC).round() as u32;
    let mask_y0 = SIG_DIM - (SIG_DIM as f32 * MASK_H_FRAC).round() as u32;
    x >= mask_x0 && y >= mask_y0
}

impl CellSig {
    /// Build a signature from a `SIG_DIM × SIG_DIM` grayscale buffer.
    ///
    /// `None` when the unmasked region is flat: an empty slot has no gradient
    /// to normalize, and dividing by its zero stddev would make every
    /// comparison NaN.
    pub fn from_gray(gray: Vec<u8>) -> Option<Self> {
        if gray.len() != (SIG_DIM * SIG_DIM) as usize {
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
                let v = gray[(y * SIG_DIM + x) as usize] as f64;
                sum += v;
                sum_sq += v * v;
                active += 1;
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
        let mut gray = gray;
        let mut norm = vec![0.0f32; gray.len()];
        for y in 0..SIG_DIM {
            for x in 0..SIG_DIM {
                let i = (y * SIG_DIM + x) as usize;
                if masked(x, y) {
                    gray[i] = 0;
                    continue;
                }
                norm[i] = ((gray[i] as f64 - mean) / sd) as f32;
            }
        }
        Some(Self { gray, norm, active })
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

    /// The stored grayscale — masked positions zeroed, exactly the bytes a
    /// reload reproduces the signature from.
    ///
    /// This is also the ONLY thing that goes on the wire to the shared pool
    /// (POE-201): the upload payload is built from these bytes in memory, so
    /// the colour crop `save` writes next to a template never leaves the
    /// device and no code path has to walk the store directory to publish.
    pub fn gray(&self) -> &[u8] {
        &self.gray
    }

    /// The stored grayscale as an image, for `save` and for eyeballing.
    pub fn to_image(&self) -> GrayImage {
        GrayImage::from_raw(SIG_DIM, SIG_DIM, self.gray.clone())
            .expect("a signature always holds SIG_DIM² samples")
    }
}

/// Reduce a support cell to a signature.
///
/// Takes the cell's OUTER rect (what [`super::geometry::detect`] emits) and
/// reads its inner region, so the cell frame — identical on every cell — never
/// reaches the correlation.
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
    let [x, y, w, h] = inner_rect(rect, g);
    if x < 0 || y < 0 || w <= 0 || h <= 0 {
        return None;
    }
    let (iw, ih) = img.dimensions();
    if (x + w) as u32 > iw || (y + h) as u32 > ih {
        return None;
    }
    let crop = img.crop_imm(x as u32, y as u32, w as u32, h as u32).to_luma8();
    let resized = image::imageops::resize(
        &crop,
        SIG_DIM,
        SIG_DIM,
        image::imageops::FilterType::Triangle,
    );
    CellSig::from_gray(resized.into_raw())
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

/// One learned sample.
#[derive(Debug, Clone)]
pub struct Template {
    pub family: String,
    pub tier: u8,
    pub sig: CellSig,
    /// The colour crop the sample was learned from, kept so the debug dump can
    /// show what the store actually holds. Never used in matching, and never
    /// uploaded — see [`CellSig::gray`].
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

#[derive(Serialize, Deserialize)]
struct IndexEntry {
    family: String,
    tier: u8,
    file: String,
    /// Both POE-201 fields default, so an `index.json` written before the
    /// shared pool existed still loads: every sample already on disk was
    /// hover-learned (`Local`) and was never offered to a pool that did not
    /// exist (`uploaded: false`), which is exactly what the defaults say and
    /// exactly what the first module start after the upgrade should act on.
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
    /// The corpus declared a format version this build cannot read — nothing
    /// was merged.
    pub foreign_version: bool,
}

impl MergeOutcome {
    /// Whether the store actually moved, and therefore whether a save and a
    /// generation bump are owed.
    pub fn changed(&self) -> bool {
        self.added > 0 || self.replaced > 0
    }
}

/// The learned icon templates, keyed by `(family, tier)`.
///
/// `Clone` so the off-tick writer can take a SNAPSHOT under the mutex and do
/// its disk I/O outside it. [`Self::save`] writes one PNG per sample plus the
/// index, and holding the store's lock across that would put every
/// [`Self::match_family`] call on the detect tick behind the filesystem — the
/// stall the off-tick save exists to remove, moved rather than fixed. The copy
/// is cheap next to the write it replaces: a full store is
/// [`Self::MAX_SAMPLES_PER_KEY`] × 24×24 grayscale plus the 44×44 raw crops.
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
    /// Returns `false` and changes nothing when a sample already stored under
    /// `(family, tier)` MATCHES this one (at `t.icon_match`), or when the key
    /// already holds [`Self::MAX_SAMPLES_PER_KEY`] samples. A sample that no
    /// stored one reaches is ADDED, never overwriting: the existing sample may
    /// be good art from another session, and this may be the mistimed hover —
    /// or the reverse. Matching searches every sample, so either way the cell
    /// is recognised next time. The un-poison path is [`Self::forget`], which
    /// drops the whole key.
    pub fn learn(
        &mut self,
        family: &str,
        tier: u8,
        sig: CellSig,
        raw: Option<RgbaImage>,
        t: &Thresholds,
    ) -> bool {
        let same_key: Vec<&Template> = self
            .templates
            .iter()
            .filter(|s| s.family == family && s.tier == tier)
            .collect();
        if same_key.len() >= Self::MAX_SAMPLES_PER_KEY
            || same_key.iter().any(|s| s.sig.ncc(&sig) >= t.icon_match)
        {
            return false;
        }
        self.push_sample(family, tier, sig, raw, Origin::Local, false);
        true
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
            .map(|t| (t.family.clone(), t.tier, t.sig.gray().to_vec()))
            .collect()
    }

    /// Record that the pool has seen this exact sample. `true` when something
    /// was marked, so the caller knows whether a save is owed.
    ///
    /// Matched on the signature bytes rather than on the key: a key can hold
    /// three samples offered in three different requests, and marking the key
    /// would tell the next module start that samples the pool never saw are
    /// already published.
    pub fn mark_uploaded(&mut self, family: &str, tier: u8, gray: &[u8]) -> bool {
        let mut marked = false;
        for sample in &mut self.templates {
            if sample.family == family
                && sample.tier == tier
                && sample.origin == Origin::Local
                && !sample.uploaded
                && sample.sig.gray() == gray
            {
                sample.uploaded = true;
                marked = true;
            }
        }
        marked
    }

    /// Fold a pulled corpus into the store (POE-201).
    ///
    /// Three rules, in this order:
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
    /// 3. **Everything else is a union**, deduped by the same NCC the local
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

        for sample in &corpus.samples {
            if held(&sample.family, sample.tier) {
                out.suppressed += 1;
                continue;
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

    /// Best family for a signature, across every stored sample.
    ///
    /// `Matched` needs `icon_match` with an `icon_lead` lead over the best
    /// sample of a DIFFERENT family — two tiers of the same family competing
    /// with each other is agreement, not ambiguity. `LowConfidence` at
    /// `icon_low`. Anything else is `Unknown`, which the page renders as
    /// "unknown — hover to confirm".
    pub fn match_family(&self, sig: &CellSig, t: &Thresholds) -> IconMatch {
        let mut best: Option<(&Template, f32)> = None;
        for template in &self.templates {
            let score = template.sig.ncc(sig);
            if best.is_none_or(|(_, b)| score > b) {
                best = Some((template, score));
            }
        }
        let Some((winner, score)) = best else {
            return IconMatch::unknown();
        };
        let runner_up = self
            .templates
            .iter()
            .filter(|t| t.family != winner.family)
            .map(|t| t.sig.ncc(sig))
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

    /// Write the store to `dir`: one 24×24 PNG per sample, the raw crop
    /// alongside when there is one, and an index naming them (family names
    /// carry spaces, so the file names are slugs and the index is what maps
    /// them back).
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
        let json = serde_json::to_string_pretty(&index).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(INDEX_FILE), json).map_err(|e| e.to_string())
    }

    /// Read a store back. A missing directory is an empty store (the normal
    /// first-run case); an entry whose PNG will not load is SKIPPED and
    /// reported, so one corrupt file cannot take the whole store down.
    pub fn load(dir: &Path) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let raw = match std::fs::read_to_string(dir.join(INDEX_FILE)) {
            Ok(raw) => raw,
            Err(_) => return (Self::new(), problems),
        };
        let index: Vec<IndexEntry> = match serde_json::from_str(&raw) {
            Ok(i) => i,
            Err(e) => {
                problems.push(format!("{INDEX_FILE} did not parse: {e}"));
                return (Self::new(), problems);
            }
        };
        let mut store = Self::new();
        for entry in index {
            match image::open(dir.join(&entry.file)) {
                Ok(img) => {
                    let gray = img.to_luma8();
                    match CellSig::from_gray(gray.into_raw()) {
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
    /// genuinely different icons must still sit below it. Measured 0.8552,
    /// row 3 slot 0 (a plain silver gem) against row 4 slot 2 (a silver gem
    /// under crossed golden shafts): two supports, one palette. D4's
    /// provisional 0.80 would have called them one family, and with only one
    /// of the two learned the read would have been confidently WRONG rather
    /// than merely unknown.
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
    /// Measured 0.9034, row 3 slot 1 against row 4 slot 1 (one blue
    /// double-orb, a px of rect misalignment apart).
    ///
    /// With the test above this brackets `icon_match` into the measured
    /// 0.8552..0.9034 band — moving it below 0.8552 or above 0.9034 fails one
    /// of the two, and the band is what the first Windows dump should
    /// re-derive on a bigger sample.
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

        let m = store.match_family(&sig_of(&img, 1, 0), &t);

        assert_eq!(m.state, ReadState::Matched, "match was {m:?}");
        assert_eq!(m.family.as_deref(), Some("Chain"));
        assert_eq!(m.learned_tier, Some(2));
    }

    /// A cell whose family was never confirmed stays unknown. Handing it the
    /// nearest learned family would name a support the player does not have.
    #[test]
    fn an_unlearned_icon_does_not_borrow_the_nearest_family() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 2, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);

        let m = store.match_family(&sig_of(&img, 2, 2), &t);

        assert_eq!(m.state, ReadState::Unknown, "match was {m:?}");
        assert!(m.family.is_none());
    }

    /// An empty store answers nothing — the first-run state, before any
    /// hover-confirm.
    #[test]
    fn an_empty_store_matches_nothing() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;

        let m = TemplateStore::new().match_family(&sig_of(&img, 0, 0), &t);

        assert_eq!(m.state, ReadState::Unknown);
        assert_eq!(m.score, 0.0);
    }

    /// Two tiers of ONE family are not competition: the lead rule measures
    /// against a different family, so a family learned at both tiers must
    /// still match, not fall to low-confidence.
    #[test]
    fn two_tiers_of_the_same_family_do_not_cancel_each_others_lead() {
        let img = fixture();
        let t = MercGeometry::default().thresholds;
        let mut store = TemplateStore::new();
        store.learn("Chain", 1, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);
        store.learn("Chain", 3, sig_of(&img, 1, 0), None, &MercGeometry::default().thresholds);

        let m = store.match_family(&sig_of(&img, 1, 0), &t);

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
        assert!(!same, "the same art again is refused");
        assert_eq!(store.len(), 1);

        let accepted = store.learn("Chain", 2, sig_of(&img, 2, 2), None, &t);
        assert!(accepted, "unreached art is added as a second sample");
        assert_eq!(store.len(), 2);
        assert_eq!(store.learned_keys(), ["Chain--2"], "one key, two samples");
        assert_eq!(
            store.match_family(&sig_of(&img, 2, 2), &t).family.as_deref(),
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
        let m = loaded.match_family(&sig_of(&img, 2, 2), &t);
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
            store.match_family(&sig_of(&img, 1, 0), &t).family.as_deref(),
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
            store.match_family(&sig_of(&img, 2, 2), &t).family.as_deref(),
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
        let foreign = PooledCorpus {
            format_version: super::super::sync::FORMAT_VERSION + 1,
            samples: vec![pooled("Pierce", 1, sig_of(&img, 2, 2))],
            tombstones: vec![("Chain".to_string(), 2)],
        };

        let out = store.merge_pulled(&foreign, &[], &t);

        assert!(out.foreign_version);
        assert_eq!(out.added, 0);
        assert_eq!(out.replaced, 0, "a foreign tombstone must not delete either");
        assert_eq!(store.learned_keys(), ["Chain--2"]);
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
        assert_eq!(pending[0].2.len(), 576, "the wire payload is the signature");
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
        let first = sig_of(&img, 1, 0).gray().to_vec();

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

        assert!(!store.mark_uploaded("Chain", 2, &[0u8; 576]));
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
        store.mark_uploaded("Chain", 2, &sig_of(&img, 1, 0).gray().to_vec());
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
    fn an_index_without_the_pool_fields_loads_as_an_unpublished_local_sample() {
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
        // Exactly the shape POE-165 shipped: three fields, no provenance.
        std::fs::write(
            dir.join("index.json"),
            r#"[{"family":"Chain","tier":2,"file":"chain--t2.png"}]"#,
        )
        .expect("rewrite the index in the old shape");

        let (loaded, problems) = TemplateStore::load(&dir);

        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(loaded.learned_keys(), ["Chain--2"]);
        assert!(loaded.pooled_keys().is_empty(), "an old sample is the user's own");
        assert_eq!(loaded.pending_uploads().len(), 1, "and is owed to the pool");
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
}
