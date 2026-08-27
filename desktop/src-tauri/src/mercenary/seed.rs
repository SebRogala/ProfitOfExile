//! Seed icon templates from player support-gem art (POE-208 L9/L10).
//!
//! The recruit window's support icons ARE the player support gems' inventory
//! art (measured 2026-08-27: poewiki's 78×78 `*_Support_inventory_icon.png`
//! correlated 0.89-0.95 against live crops, next-best ≤ 0.84). The server
//! already serves that art over `/api/gem-icon/{name}` (ADR-012), so a device
//! can recognise a family it has never hovered — the hover stays the fallback
//! for the ~100 families no player gem is named after.
//!
//! # Three parts, one per file it owns
//!
//! - **[`seed-map.json`](SEED_MAP_JSON)** — which player gem's art seeds which
//!   merc family, generated from two name rules and hand-extended. Loaded
//!   here, never by the vocabulary: the installer runs before the loop reads
//!   the vocabulary, which is why the map carries the family's lowest tier
//!   itself.
//! - **The derivation** — [`derive`] renders one 78×78 RGBA art into a
//!   synthetic cell of the geometry IN FORCE and hands it to
//!   [`super::icons::normalize_cell`]. Three constants say how, and all three
//!   are FRACTIONS of the alignment window rather than pixel counts — see
//!   [`SEED_ART`].
//! - **[`seed-blocklist.json`](BLOCKLIST_FILE)** — the only seed state that
//!   survives a restart. Signatures are re-derived from the cached art on
//!   every start and are never written into `index.json`, so a family whose
//!   seed was thrown out has to be remembered somewhere or the next start
//!   would put it straight back.
//!
//! # Why nothing here is saved as a template
//!
//! A seed is memory-only ([`super::icons::Origin::Seed`]). Persisting it would
//! buy a downgrade hazard for nothing: an older build reading `"origin":
//! "seed"` in `index.json` fails `deny_unknown_fields`-adjacent expectations
//! and purges the whole store, and the art is on disk anyway, one resize away.

// The map loader and the derivation have no PRODUCTION caller yet: the fetch,
// the install seam and the per-window re-derivation are POE-208 WI-B, and this
// file is WI-A. Everything here is reached by the tests below, so the choice is
// between this one attribute and eighteen of them.
//
// **WI-B deletes this line.** Once `run.rs` installs seeds, a `dead_code`
// warning in this module means something really is unreachable, and the whole
// desktop crate is otherwise warning-clean.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::{DynamicImage, RgbaImage};
use serde::{Deserialize, Serialize};

use super::icons::SHIFT_MAX;
use super::{icons::CellSig, MercGeometry};

// ---------------------------------------------------------------------------
// The family → gem map
// ---------------------------------------------------------------------------

/// The committed map, compiled in — the installer needs it before any file the
/// app data directory holds is readable.
pub const SEED_MAP_JSON: &str = include_str!("seed-map.json");

/// How much is known about one map row's art.
///
/// Three grades, because they buy different things. `Corpus` is the acceptance
/// test's own verdict — the seed matched a clean crop of that family through
/// the real matcher. `Visual` is a human comparison of the art against a live
/// crop, which is what the families whose corpus crops are POISONED have.
/// `Name` is the two naming rules and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verified {
    /// A clean corpus crop of this family matches the seed through
    /// `match_family` (the acceptance test writes this grade).
    Corpus,
    /// The art was compared against a live crop by eye on 2026-08-27.
    Visual,
    /// Only the name rule says these two are the same picture.
    Name,
}

/// One row of [`SEED_MAP_JSON`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SeedEntry {
    /// The merc support FAMILY — `vocab::MercStat::family`, the same string
    /// the template store keys on.
    pub family: String,
    /// The player gem whose inventory art seeds it — a key of
    /// `internal/gemicon/gem-icon-urls.json`, and the path segment
    /// `/api/gem-icon/{gem}` takes.
    pub gem: String,
    /// The family's LOWEST vocabulary tier, written by the generator.
    ///
    /// Carried in the map rather than looked up so the install seam needs no
    /// vocabulary: the loop parses the vocabulary after the store is
    /// installed, and one seed has to land under one key that every
    /// `(family, tier)` code path already understands.
    pub tier: u8,
    pub verified: Verified,
    /// Whether this row is fetched, derived and installed at all.
    ///
    /// **`name`-only rows ship disabled** (ruling 2026-08-27, orchestrator,
    /// under the module's standing preference to fail towards
    /// `LowConfidence` and never towards a wrong family). A wrong seed is not
    /// self-correcting the way a wrong pooled sample is: it produces a
    /// confident wrong `Matched`, which provokes no hover, so the eviction
    /// rule that un-poisons everything else is never reached. The follow-up
    /// curation ticket flips rows on as verification lands.
    pub enabled: bool,
}

/// The map's header comment, carried as a JSON key because JSON has none.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SeedMapFile {
    /// What the rules are and what the grades mean — for whoever opens the
    /// fixture, not for the code.
    #[serde(rename = "_comment")]
    _comment: Vec<String>,
    entries: Vec<SeedEntry>,
}

/// Parse the compiled-in map.
///
/// `Err` only if the committed fixture stops being this shape, which a
/// contract test below rules out — the loop can surface it rather than dying.
pub fn load_map() -> Result<Vec<SeedEntry>, String> {
    serde_json::from_str::<SeedMapFile>(SEED_MAP_JSON)
        .map(|f| f.entries)
        .map_err(|e| format!("seed-map.json did not parse: {e}"))
}

/// Which rows this device fetches, derives and installs: enabled, and not
/// blocklisted.
///
/// One place, because three callers ask the same question at three moments —
/// the fetch (do not spend a request), the start-up install, and a late fetch
/// that lands after the install seam. A blocklisted family answered "yes" at
/// any one of them would put back exactly the seed an eviction threw out.
pub fn installable(entries: &[SeedEntry], blocked: &SeedBlocklist) -> Vec<SeedEntry> {
    entries
        .iter()
        .filter(|e| e.enabled && !blocked.blocks(&e.family))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Derivation (L10) — at the live window, in window fractions
// ---------------------------------------------------------------------------

/// How a 78×78 gem art is mounted in a support cell, as FRACTIONS of the
/// alignment window.
///
/// # Why fractions and not pixels
///
/// The panel is captured at whatever scale the game is running (0.974 on
/// Sebastian's 1920×1200, 1.0 on the reference fixture), and the alignment
/// window — the inner crop minus [`SHIFT_MAX`] px per side — is 33 px at the
/// first and 34 px at the second. The art fills a FIXED FRACTION of the cell
/// on screen, so the number that is scale-invariant is `art px / window px`,
/// not `art px`. The ±3 px alignment search cannot stand in for getting this
/// right: it slides the probe, it does not resample it, so a fraction error
/// shows up as a resampling mismatch the search has no lever on.
///
/// A struct rather than three loose constants because the calibration sweep
/// varies them together and the shipped values are one measured point in that
/// space, not three independent tunables.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeedArt {
    /// Art side length ÷ window side length.
    pub frac: f32,
    /// The art's top-left corner relative to the window's, in window widths —
    /// negative means the art starts outside the window and is cropped by it.
    pub offset_frac: [f32; 2],
    /// The cell background the art's transparent pixels sit on.
    pub bg: [u8; 3],
}

/// Art side ÷ window side — 38 px at the reference window 34, 37 px at the
/// live 33.
///
/// The art is rendered LARGER than the window and cropped by it, because the
/// recruit cell shows the gem art bled out to its frame while the alignment
/// window has already given up [`SHIFT_MAX`] px per side to the shift search.
///
/// Measured by [`tests::sweep_the_calibration_against_the_corpus`] over
/// 13 fractions × 11 × 13 offsets × 5 backgrounds, scored through the real
/// 49-alignment search against the 29 clean corpus crops of mapped families.
/// At this point 26 of the 29 reach `icon_match` 0.88, worst 0.689, mean
/// 0.946; the next four rows of the ranked table are this same fraction and
/// offset at the four other backgrounds, and only then does another geometry
/// appear (1.175 at offset 3, -3, worst 0.658). A plateau around one point,
/// not a lucky cell.
pub const SEED_ART_FRAC: f32 = 1.125;

/// The art's top-left relative to the window's, in window widths — 4 px right
/// and 2 px up at the reference window 34.
///
/// **Chosen so the MEDIAN best alignment is (0, 0)**, not so the score is
/// highest. Scoring alone picks `+1` in y, which scores better (27 of 29) and
/// is wrong: at that offset the search's best shift is pinned at the `dy = -3`
/// edge on 26 of the 29 crops, so the whole ±3 px budget is spent cancelling a
/// constant and a cell whose own jitter runs the same way falls off the end.
/// The search is for `geometry::detect`'s per-cell jitter; the calibration
/// owes it a zero mean.
pub const SEED_ART_OFFSET_FRAC: [f32; 2] = [4.0 / 34.0, -2.0 / 34.0];

/// The cell background behind transparent art pixels.
///
/// **Measured, and then overruled by the sweep — both numbers matter.** Over
/// the clean corpus crops of mapped families, the crop pixels underneath the
/// art's fully transparent pixels (11,200 of them at this calibration) have a
/// median of (18, 16, 14) sRGB, with the 5th-to-50th percentile spanning only
/// (14, 13, 11)..(18, 16, 14): the recruit window's empty cell is a near-black
/// warm grey, not the mid-brown the disc RING suggests — the ring is mostly
/// art.
///
/// Black scores better than that median at every offset the sweep tried
/// (0.946 mean against 0.944, and 0.689 worst against 0.666), so black is what
/// ships. The measurement is recorded because it is why the constant is dark
/// at all, and because it bounds how much this choice can matter: the two
/// values are four grey levels apart.
pub const SEED_ART_BG: [u8; 3] = [0, 0, 0];

/// The shipped calibration.
pub const SEED_ART: SeedArt = SeedArt {
    frac: SEED_ART_FRAC,
    offset_frac: SEED_ART_OFFSET_FRAC,
    bg: SEED_ART_BG,
};

/// The outer cell size the detect emits at this scale — `geometry::detect`'s
/// own expression, so a seed is rendered into the cell the loop will read.
pub fn cell_px(g: &MercGeometry, scale: f32) -> i32 {
    (g.cell_size * scale).round().max(1.0) as i32
}

/// The alignment window's side length at this scale: the inner crop minus
/// [`SHIFT_MAX`] px per side. 34 at scale 1.0, 33 at 0.974.
///
/// This is the number the seed signatures are MEMOISED on (WI-B): two scales
/// that round to the same window need one derivation, and a window that
/// changes needs a new one however small the scale step was.
pub fn window_px(g: &MercGeometry, scale: f32) -> i32 {
    let inset = g.cell_inset.round() as i32;
    cell_px(g, scale) - 2 * inset - 2 * SHIFT_MAX
}

/// Render one gem art into a synthetic support cell.
///
/// The cell is the OUTER rect [`super::geometry::detect`] would emit —
/// `cell_size · scale` square — so the art then travels through
/// [`super::icons::normalize_cell`] over exactly the door a live cell takes:
/// the occupancy gate, [`super::geometry::inner_rect`], the alignment window,
/// the Triangle resize to 24×24, the disc and the badge mask.
///
/// Transparent art pixels are composited over [`SeedArt::bg`] rather than left
/// alone, because a normalisation over the alpha-less RGB of a transparent
/// pixel reads whatever the PNG encoder happened to store there — usually
/// black, sometimes the edge colour smeared, and never the same across the
/// 51 files the map names.
pub fn render_cell(art: &RgbaImage, g: &MercGeometry, scale: f32, p: &SeedArt) -> DynamicImage {
    let outer = cell_px(g, scale).max(1) as u32;
    let inset = g.cell_inset.round() as i32;
    let window = window_px(g, scale).max(1);

    let art_px = (p.frac * window as f32).round().max(1.0) as u32;
    let scaled = image::imageops::resize(
        art,
        art_px,
        art_px,
        image::imageops::FilterType::Triangle,
    );

    let mut canvas = RgbaImage::from_pixel(
        outer,
        outer,
        image::Rgba([p.bg[0], p.bg[1], p.bg[2], 255]),
    );
    // The window's own origin inside the cell, then the art's offset from it.
    let win0 = inset + SHIFT_MAX;
    let x = win0 as i64 + (p.offset_frac[0] * window as f32).round() as i64;
    let y = win0 as i64 + (p.offset_frac[1] * window as f32).round() as i64;
    // `overlay`, not `replace`: it blends source-over, which is what makes the
    // background above show through the art's alpha.
    image::imageops::overlay(&mut canvas, &scaled, x, y);
    DynamicImage::ImageRgba8(canvas)
}

/// One seed signature at the geometry and scale in force.
///
/// `None` under exactly the conditions a live cell yields nothing: art so flat
/// the synthetic cell fails the occupancy gate, or a geometry whose cell is too
/// small to hold a signature.
pub fn derive(art: &RgbaImage, g: &MercGeometry, scale: f32) -> Option<CellSig> {
    derive_with(art, g, scale, &SEED_ART)
}

/// [`derive`] at an arbitrary calibration — the sweep's entry point, and the
/// reason the shipped constants are a measurement rather than a guess.
pub fn derive_with(
    art: &RgbaImage,
    g: &MercGeometry,
    scale: f32,
    p: &SeedArt,
) -> Option<CellSig> {
    let outer = cell_px(g, scale).max(1);
    let cell = render_cell(art, g, scale, p);
    super::icons::normalize_cell(&cell, [0, 0, outer, outer], g)
}

// ---------------------------------------------------------------------------
// The art cache and the blocklist
// ---------------------------------------------------------------------------

/// The seed art cache, inside the template directory.
///
/// GGG's art, so it lives beside the `-raw.png` colour crops the store already
/// keeps on the device and travels no further — the pool carries signatures
/// only.
pub const SEED_ART_DIR: &str = "seed";

/// `<icons_dir>/seed`.
pub fn art_dir(icons_dir: &Path) -> PathBuf {
    icons_dir.join(SEED_ART_DIR)
}

/// File-name-safe form of a gem name — the cache file's stem and the fixture's.
pub fn art_slug(gem: &str) -> String {
    let s: String = gem
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

/// `<icons_dir>/seed/<slug>.png` — where one gem's art is cached.
pub fn art_path(icons_dir: &Path, gem: &str) -> PathBuf {
    art_dir(icons_dir).join(format!("{}.png", art_slug(gem)))
}

/// The blocklist file, beside `index.json` and `pool-sync.json`.
pub const BLOCKLIST_FILE: &str = "seed-blocklist.json";

/// Serialises every read-modify-write of [`BLOCKLIST_FILE`].
///
/// Four writers on three threads reach it — the hover tick's eviction, the
/// pull task's eviction, `merc_forget_seed`, and `merc_reset_templates` — and
/// each is a load → mutate → save, so without this two of them can read the
/// same bytes and the later save drops the earlier family. A dropped family is
/// a seed that comes back on the next start, which is the ping-pong the "every
/// eviction blocklists" rule exists to stop.
///
/// A process-wide `static` on the [`super::sync`] `SYNC_FILE_LOCK` model,
/// because the resource is the FILE and two of the writers are deliberately
/// app-free so the evict → block → skip cycle stays testable.
///
/// **Lock order: the directory lock (`icons::writing_icons_dir`) FIRST, then
/// this one.** The merge path already holds the directory lock when it
/// blocklists.
static SEED_BLOCKLIST_LOCK: Mutex<()> = Mutex::new(());

/// `seed-blocklist.json` — families whose seed was thrown out here.
///
/// The ONE piece of seed state that survives a restart. Every eviction door —
/// a local confirm that collided, a pulled sample that collided, the page's ✕
/// — writes here, so the next start's re-derivation skips the family instead
/// of re-installing the art the user (or their own confirmation) just
/// contradicted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedBlocklist {
    #[serde(default)]
    pub families: Vec<String>,
}

impl SeedBlocklist {
    /// Read the file, or an empty list.
    ///
    /// A file that does not parse reads as EMPTY rather than as a failure: the
    /// worst case is one session of seeds the user had thrown out, which the
    /// next eviction blocks again, whereas refusing to start the seeding over
    /// a corrupt list would cost every family.
    pub fn load(dir: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(dir.join(BLOCKLIST_FILE)) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    /// Write the file whole, through a temporary — the shape
    /// [`super::sync::SyncFile::save`] uses, and for the same reason: a plain
    /// write truncates first, so a process that dies mid-write leaves a list
    /// that parses as empty and every blocked family comes back.
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let path = dir.join(BLOCKLIST_FILE);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn blocks(&self, family: &str) -> bool {
        self.families.iter().any(|f| f == family)
    }
}

/// Block one family's seed, durably. `true` when it was not already blocked.
///
/// The whole read-modify-write is under [`SEED_BLOCKLIST_LOCK`].
pub fn block_family(dir: &Path, family: &str) -> Result<bool, String> {
    let _guard = SEED_BLOCKLIST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut list = SeedBlocklist::load(dir);
    if list.blocks(family) {
        return Ok(false);
    }
    list.families.push(family.to_string());
    list.families.sort();
    list.save(dir)?;
    Ok(true)
}

/// Drop the blocklist and the cached art — what a reset means.
///
/// Both, because a reset says "start my store over": leaving the art would
/// keep the very pictures the user is resetting away from, and leaving the
/// blocklist would keep families un-seedable with no store left to explain it.
/// A missing directory or file is success, not an error.
pub fn clear_seed_state(icons_dir: &Path) -> Result<(), String> {
    let _guard = SEED_BLOCKLIST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let list = icons_dir.join(BLOCKLIST_FILE);
    if let Err(e) = std::fs::remove_file(&list) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(format!("{}: {e}", list.display()));
        }
    }
    let art = art_dir(icons_dir);
    if let Err(e) = std::fs::remove_dir_all(&art) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(format!("{}: {e}", art.display()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mercenary::icons::{cell_candidates, CellCandidates, TemplateStore};
    use crate::mercenary::vocab::{MercRole, MercVocab};
    use crate::mercenary::Thresholds;

    /// Committed 78×78 gem art, one file per mapped gem, fetched over the
    /// production route (`/api/gem-icon/{gem}`) so the bytes the tests reason
    /// about are the bytes the app will cache.
    const ART_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/merc-seed-art");

    /// The corpus of live crops POE-207 measured the descriptor on.
    const CROP_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/merc-icon-crops");

    /// The server's embedded name → URL map, read from the repo rather than
    /// mirrored: a `gem` this file does not carry 404s at runtime, and a copy
    /// here would only tell us that the copy agreed with itself.
    const GEM_ICON_URLS: &str =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../internal/gemicon/gem-icon-urls.json");

    /// Rewrites `seed-map.json` instead of checking it.
    const UPDATE_ENV: &str = "MERC_SEED_MAP_UPDATE";

    /// The seven families whose art was compared against a live crop BY EYE on
    /// 2026-08-27 (POE-165 discoveries § "Seed source").
    ///
    /// Two of them — Faster Attacks and Fork — have no clean corpus crop (both
    /// confirmations in Sebastian's store were mislabels), so the eye is the
    /// only evidence they will ever have here and `visual` is their ceiling.
    /// The other five are re-graded to `corpus` by the generator when they
    /// pass, and the constant is what keeps them enabled if a future corpus
    /// ever loses their crop.
    const VISUAL_FAMILIES: [&str; 7] = [
        "Added Chaos",
        "Faster Attacks",
        "Faster Projectiles",
        "Fork",
        "Multistrike",
        "Second Wind",
        "Swift Affliction",
    ];

    fn updating() -> bool {
        std::env::var(UPDATE_ENV).as_deref() == Ok("1")
    }

    // -- the vocabulary and the gem route -----------------------------------

    /// Every support family, with its lowest vocabulary tier.
    fn families() -> Vec<(String, u8)> {
        let vocab = MercVocab::load().expect("the committed vocabulary parses");
        let mut lowest: std::collections::BTreeMap<String, u8> = std::collections::BTreeMap::new();
        for stat in vocab.by_role(MercRole::Support) {
            let tier = stat.tier.expect("every support entry carries a tier");
            let slot = lowest.entry(stat.family.clone()).or_insert(tier);
            *slot = (*slot).min(tier);
        }
        lowest.into_iter().collect()
    }

    /// The names `/api/gem-icon/{name}` will serve art for.
    fn gem_icon_keys() -> std::collections::BTreeSet<String> {
        let raw = std::fs::read_to_string(GEM_ICON_URLS)
            .expect("internal/gemicon/gem-icon-urls.json is in this repo");
        serde_json::from_str::<std::collections::BTreeMap<String, String>>(&raw)
            .expect("the gem icon map parses")
            .into_keys()
            .collect()
    }

    /// The two naming rules, in order (POE-208 spec).
    ///
    /// Rule 2 exists because four merc families drop the word the player gem
    /// keeps: the merc link is `Added Chaos`, the gem is `Added Chaos Damage
    /// Support`. It is tried second so a family that satisfies both — none
    /// today — would take the exact name.
    fn mapped_gem(family: &str, keys: &std::collections::BTreeSet<String>) -> Option<String> {
        let exact = format!("{family} Support");
        if keys.contains(&exact) {
            return Some(exact);
        }
        let damage = format!("{family} Damage Support");
        keys.contains(&damage).then_some(damage)
    }

    // -- the corpus ---------------------------------------------------------

    #[derive(serde::Deserialize)]
    struct Manifest {
        crops: Vec<Crop>,
    }

    #[derive(serde::Deserialize)]
    struct Crop {
        file: String,
        family: String,
        #[serde(default)]
        poisoned: bool,
    }

    /// The crops a matcher may be judged on — the mislabels excluded.
    fn clean_crops() -> Vec<Crop> {
        let raw = std::fs::read_to_string(std::path::Path::new(CROP_DIR).join("manifest.json"))
            .expect("the corpus manifest is committed next to the crops");
        serde_json::from_str::<Manifest>(&raw)
            .expect("the corpus manifest parses")
            .crops
            .into_iter()
            .filter(|c| !c.poisoned)
            .collect()
    }

    /// The outer cell rect a corpus crop is read through — 39×39 inner plus
    /// 2 px of `cell_inset` per side, the live 0.974 geometry. Its alignment
    /// window is 33 px, and the SEEDS below are derived at 34: the cross is
    /// the point, since the fraction form is what has to make both work.
    const CROP_OUTER: [i32; 4] = [0, 0, 43, 43];

    /// Every alignment of one corpus crop, through the production entry point.
    fn probe(file: &str) -> CellCandidates {
        let crop = image::open(std::path::Path::new(CROP_DIR).join(file))
            .unwrap_or_else(|e| panic!("{file}: {e}"))
            .to_rgba8();
        assert_eq!(crop.dimensions(), (39, 39), "{file} is not a 39×39 inner crop");
        let mut canvas = RgbaImage::from_pixel(43, 43, image::Rgba([0, 0, 0, 255]));
        image::imageops::replace(&mut canvas, &crop, 2, 2);
        let img = DynamicImage::ImageRgba8(canvas);
        cell_candidates(&img, CROP_OUTER, &MercGeometry::default())
            .unwrap_or_else(|| panic!("{file} normalizes"))
    }

    // -- seeds --------------------------------------------------------------

    /// One gem art from the fetched fixture.
    ///
    /// Shape-checked at the door the way [`probe`] checks a corpus crop, and
    /// for the same reason: everything downstream resizes, so a wrong-shaped
    /// source would not fail — it would quietly change the measurement the
    /// constants are read off. SQUARE is the requirement, not 78×78: the
    /// derivation resizes to a fraction of the window, so the pixel count only
    /// has to be square and large enough to carry the icon. `Sacred Wisps
    /// Support` really is 80×80 on the route today, and `Trap and Mine Damage
    /// Support` really has no alpha channel (`to_rgba8` gives it an opaque
    /// one) — both are `name`-graded, so neither reaches a shipped seed, but
    /// pinning the rule rather than the number is what keeps them from
    /// silently becoming a third case.
    fn art(gem: &str) -> RgbaImage {
        let path = std::path::Path::new(ART_DIR).join(format!("{}.png", art_slug(gem)));
        let img = image::open(&path)
            .unwrap_or_else(|e| panic!("{}: {e} — {FETCH_HINT}", path.display()))
            .to_rgba8();
        let (w, h) = img.dimensions();
        assert_eq!(w, h, "{}: gem art must be square, got {w}×{h}", path.display());
        assert!(
            (64..=128).contains(&w),
            "{}: {w}×{h} is outside the inventory-icon range this fixture is measured on",
            path.display(),
        );
        img
    }

    /// How to get the art, named in every failure that misses it.
    const FETCH_HINT: &str = "the seed art is not committed (GPL-3 repo, \
        CC-BY-NC-SA icons — see the fixture README); fetch it with \
        `make merc-seed-art`";

    /// Fail unless every gem the map names has its art on disk.
    ///
    /// Loud, never a skip. A skip here would leave the three calibration
    /// constants and the map's whole `corpus` grade unmeasured while the suite
    /// still reported green — which is the failure mode that costs most,
    /// because the numbers those tests defend are the ones nobody can eyeball.
    fn require_art_fixture(rows: &[(String, String, u8)]) {
        let missing: Vec<&str> = rows
            .iter()
            .filter(|(_, gem, _)| {
                !std::path::Path::new(ART_DIR)
                    .join(format!("{}.png", art_slug(gem)))
                    .exists()
            })
            .map(|(_, gem, _)| gem.as_str())
            .collect();
        assert!(
            missing.is_empty(),
            "{} of {} gem art file(s) missing from the fixture (first: {}) — {FETCH_HINT}",
            missing.len(),
            rows.len(),
            missing[0],
        );
    }

    /// The geometry the seeds are derived at: the shipped reference, scale
    /// 1.0, window 34.
    fn seed_scale() -> (MercGeometry, f32) {
        (MercGeometry::default(), 1.0)
    }

    /// A store holding one seed per row, at `p`.
    fn seeded_store(rows: &[(String, String, u8)], p: &SeedArt) -> TemplateStore {
        let (g, scale) = seed_scale();
        let t = Thresholds::default();
        let mut store = TemplateStore::new();
        for (family, gem, tier) in rows {
            let sig = derive_with(&art(gem), &g, scale, p)
                .unwrap_or_else(|| panic!("{gem} derives a signature"));
            store.install_seed(family, *tier, sig, &t);
        }
        store
    }

    /// Every `(family, gem, tier)` the two name rules produce and art exists
    /// for — the candidate set the grading runs over.
    fn candidate_rows() -> Vec<(String, String, u8)> {
        let keys = gem_icon_keys();
        let rows: Vec<(String, String, u8)> = families()
            .into_iter()
            .filter_map(|(family, tier)| {
                mapped_gem(&family, &keys).map(|gem| (family, gem, tier))
            })
            .collect();
        // Checked HERE, once, rather than left to whichever `art()` call
        // happened to come first: an absent fixture used to filter rows OUT
        // silently, which turned "no art" into "no corpus grade" and let the
        // generator write a map of nothing but `name`.
        require_art_fixture(&rows);
        rows
    }

    /// The best correlation between one seed and one crop, over the 49
    /// alignments, together with the shift it was found at.
    fn best_alignment(seed: &CellSig, cell: &CellCandidates) -> (f32, i32, i32) {
        let mut best = (f32::MIN, 0i32, 0i32);
        // `aligned()`, not an index walk over `all()`: a shift whose window
        // falls off the image is dropped, so reconstructing `(dx, dy)` from the
        // position would mis-map every alignment after the gap — and silently,
        // since the SCORES would still be right.
        for (sig, (dx, dy)) in cell.aligned() {
            let score = seed.ncc(sig);
            if score > best.0 {
                best = (score, dx, dy);
            }
        }
        best
    }

    /// The rows that could plausibly be enabled: the ones whose own seed
    /// reaches every clean crop of their family on its own, plus the
    /// hand-graded [`VISUAL_FAMILIES`].
    ///
    /// This — not all 51 candidates — is the store the grading runs in,
    /// because it is a superset of the SHIPPED store and nothing else is.
    /// Grading against all 51 puts `name`-only seeds that will never be
    /// installed into the competition, and one of them really does steal a
    /// family: `Minion Damage`'s art reaches the `Minion Life` crop at 0.982,
    /// so the shipped store would recognise Minion Life and the all-candidates
    /// store would not. Since this set is a superset of the enabled one and
    /// removing templates can only lower a runner-up, a family graded here is
    /// still graded correctly when the acceptance test re-checks it in the
    /// smaller store.
    fn gradable_rows() -> Vec<(String, String, u8)> {
        let t = Thresholds::default();
        let (g, scale) = seed_scale();
        let clean = clean_crops();
        candidate_rows()
            .into_iter()
            .filter(|(family, gem, _)| {
                if VISUAL_FAMILIES.contains(&family.as_str()) {
                    return true;
                }
                let Some(seed) = derive(&art(gem), &g, scale) else {
                    return false;
                };
                let mut crops = clean.iter().filter(|c| c.family == *family).peekable();
                crops.peek().is_some()
                    && crops.all(|c| best_alignment(&seed, &probe(&c.file)).0 >= t.icon_match)
            })
            .collect()
    }

    /// THE corpus predicate, shared by the generator (which writes the grade)
    /// and by the acceptance test (which re-checks it against the committed
    /// file). Two copies of this rule would let the map claim a grade the
    /// acceptance test does not enforce.
    ///
    /// The whole matcher, not a bare correlation: `match_family` also has to
    /// pick this family over every other seed in the store and clear
    /// `icon_lead`, which is what "the cell will read `Matched`" actually
    /// means.
    ///
    /// # The plan's unshifted clause, measured
    ///
    /// The plan asked for a second clause here — `into_centre()` alone
    /// clearing `icon_match` — so that a ≤3 px calibration error could not
    /// hide inside the alignment search. It is not a property of the
    /// calibration alone, because the corpus crops carry
    /// `geometry::detect`'s own per-cell jitter (POE-207 measured the same
    /// art in two cells at 0.45-0.70 unaligned): a crop cut 3 px off centre
    /// scores badly unshifted however right the constants are.
    ///
    /// Measured at the shipped calibration, over the 24 clean crops of
    /// corpus-graded families: **12 clear `icon_match` unshifted**, 8 of them
    /// because their own best alignment IS (0, 0); the rest fall away with
    /// their jitter, down to `infused-channelling--t3-raw.png` at 0.225
    /// unshifted and 0.958 at its best shift of (-3, 0). So the clause holds
    /// for half the corpus and cannot hold for all of it.
    ///
    /// Both halves are therefore asserted in
    /// [`the_shipped_calibration_matches_the_corpus_and_is_offset_sensitive`]:
    /// the plan's clause as a FLOOR on how many crops clear it unshifted, and
    /// the systematic component as a median best shift of exactly (0, 0),
    /// which per-cell jitter cannot move and a constant error cannot survive.
    fn corpus_verifies(family: &str, store: &TemplateStore, t: &Thresholds) -> bool {
        let crops: Vec<Crop> = clean_crops().into_iter().filter(|c| c.family == family).collect();
        if crops.is_empty() {
            return false;
        }
        crops.iter().all(|c| {
            let read = store.match_family(&probe(&c.file), t);
            read.family.as_deref() == Some(family)
                && read.state == crate::mercenary::ReadState::Matched
                && read.score >= t.icon_match
        })
    }

    /// The map exactly as the rules, the fixtures and the corpus produce it,
    /// with the families the pairwise contract excludes.
    ///
    /// Three steps, in order: grade every row, enable everything graded, then
    /// take back the enablement of any PAIR of enabled seeds whose signatures
    /// land inside the lead band. The third step is not a nicety — two seeds
    /// within `icon_match - icon_lead` of each other make BOTH their families
    /// permanently un-`Matched`, and disabling only one is worse still: the
    /// survivor then answers the other family's cells with a confident wrong
    /// name, which is the one outcome this module refuses. So both go, the
    /// same rule `merge_pulled` applies to one art claimed by two families.
    fn generated_map() -> (Vec<SeedEntry>, Vec<String>) {
        let rows = gradable_rows();
        let store = seeded_store(&rows, &SEED_ART);
        let t = Thresholds::default();
        let keys = gem_icon_keys();
        let (g, scale) = seed_scale();
        let mut out: Vec<SeedEntry> = families()
            .into_iter()
            .filter_map(|(family, tier)| mapped_gem(&family, &keys).map(|gem| (family, gem, tier)))
            .map(|(family, gem, tier)| {
                let verified = if rows.iter().any(|(f, _, _)| *f == family)
                    && corpus_verifies(&family, &store, &t)
                {
                    Verified::Corpus
                } else if VISUAL_FAMILIES.contains(&family.as_str()) {
                    Verified::Visual
                } else {
                    Verified::Name
                };
                SeedEntry {
                    family,
                    gem,
                    tier,
                    verified,
                    enabled: verified != Verified::Name,
                }
            })
            .collect();
        out.sort_by(|a, b| a.family.cmp(&b.family));

        let ceiling = t.icon_match - t.icon_lead;
        let sigs: Vec<(String, CellSig)> = out
            .iter()
            .filter(|e| e.enabled)
            .map(|e| {
                (
                    e.family.clone(),
                    derive(&art(&e.gem), &g, scale).expect("an enabled seed derives"),
                )
            })
            .collect();
        let mut excluded: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for i in 0..sigs.len() {
            for j in i + 1..sigs.len() {
                if sigs[i].1.ncc(&sigs[j].1) >= ceiling {
                    excluded.insert(sigs[i].0.clone());
                    excluded.insert(sigs[j].0.clone());
                }
            }
        }
        for entry in &mut out {
            if excluded.contains(&entry.family) {
                entry.enabled = false;
            }
        }
        (out, excluded.into_iter().collect())
    }

    fn header(excluded: &[String]) -> Vec<String> {
        let mut lines: Vec<String> = [
            "GENERATED by mercenary::seed::tests::the_seed_map_is_what_the_rules_and_the_corpus_produce.",
            "Regenerate with MERC_SEED_MAP_UPDATE=1 cargo test --lib mercenary::seed; never hand-edit.",
            "family/gem: rule 1 '<family> Support' (47 hits), then rule 2 '<family> Damage Support' (4:",
            "Added Chaos/Cold/Fire/Lightning). family = merc display text minus '(Tier N)' minus a leading",
            "Lesser/Greater/Gilded. gem is a key of internal/gemicon/gem-icon-urls.json.",
            "tier: the family's LOWEST vocabulary tier — one seed per family, under one key.",
            "verified: 'corpus' = every clean corpus crop of this family resolves to it through the real",
            "matcher (the acceptance test re-checks every row that claims it); 'visual' = the art was",
            "compared against a live crop by eye on 2026-08-27; 'name' = only the name rule says so.",
            "enabled: RULING 2026-08-27 (orchestrator) — 'name'-only rows ship DISABLED. A wrong seed is a",
            "confident wrong Matched that provokes no hover, so the eviction rule never reaches it; the",
            "module's standing preference is to fail towards LowConfidence, never towards a wrong family.",
            "A follow-up curation ticket flips rows on as verification lands.",
            "enabled is ALSO withdrawn from both halves of any pair of seeds whose signatures correlate at",
            "or above icon_match - icon_lead (0.83): inside that band neither family can ever read Matched,",
            "and keeping just one would answer the other family's cells with the wrong name.",
        ]
        .map(str::to_string)
        .to_vec();
        lines.push(if excluded.is_empty() {
            "No pair is excluded at this calibration.".to_string()
        } else {
            format!("Excluded by that pairwise rule: {}.", excluded.join(", "))
        });
        lines
    }

    // -- the generator ------------------------------------------------------

    /// The map is a golden: the committed file is exactly what the two name
    /// rules, the committed art and the corpus produce.
    ///
    /// A checked-in generated file rather than a runtime derivation because
    /// the installer runs BEFORE the vocabulary is parsed and offline, and
    /// because a hand-curated grade has to survive the next regeneration —
    /// which it does by being computed, not remembered.
    #[test]
    fn the_seed_map_is_what_the_rules_and_the_corpus_produce() {
        let (entries, excluded) = generated_map();
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/mercenary/seed-map.json"
        ));
        let json = serde_json::to_string_pretty(&SeedMapFile {
            _comment: header(&excluded),
            entries: entries.clone(),
        })
        .expect("the map serialises")
            + "\n";
        if updating() {
            std::fs::write(&path, &json).expect("rewrite seed-map.json");
            panic!(
                "{UPDATE_ENV}=1: rewrote {} ({} entries). Re-run without the flag to verify.",
                path.display(),
                entries.len(),
            );
        }
        // BYTES, not parsed entries: the `_comment` header states the rules,
        // the ruling and the pairwise exclusions, and a hand-edit there is
        // exactly the drift this golden exists to catch — a parsed comparison
        // would let the file describe rules the generator does not apply.
        let committed = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {} : {e}", path.display()));
        if committed != json {
            let mismatch = committed
                .lines()
                .zip(json.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map(|(n, (a, b))| format!("line {}: committed {a:?}, generated {b:?}", n + 1))
                .unwrap_or_else(|| {
                    format!(
                        "committed {} lines, generated {}",
                        committed.lines().count(),
                        json.lines().count(),
                    )
                });
            panic!(
                "seed-map.json is not what the generator produces — {mismatch}. \
                 Regenerate with {UPDATE_ENV}=1",
            );
        }
        // And the bytes really are this contract, not a file that merely
        // round-trips: parsing them back has to give the same entries.
        assert_eq!(load_map().expect("the committed map parses"), entries);
    }

    // -- map contract -------------------------------------------------------

    /// Every `gem` has to be a name `/api/gem-icon/{name}` will answer for.
    /// A name absent from the server's embedded map is a permanent 404 — the
    /// family would silently never seed, and the summary line would report it
    /// as "unavailable" forever.
    #[test]
    fn every_mapped_gem_is_a_gem_icon_route_key() {
        let keys = gem_icon_keys();
        let missing: Vec<String> = load_map()
            .expect("the map parses")
            .into_iter()
            .filter(|e| !keys.contains(&e.gem))
            .map(|e| e.gem)
            .collect();
        assert!(missing.is_empty(), "not served by /api/gem-icon: {missing:?}");
    }

    /// Every `family` has to be a vocabulary family, and `tier` its LOWEST
    /// vocabulary tier.
    ///
    /// The tier is the whole reason the map carries one: the installer runs
    /// before the vocabulary is parsed, so a wrong number here files the seed
    /// under a key the badge reader never asks about, and the family reads
    /// unknown with a seed sitting right there.
    #[test]
    fn every_family_is_a_vocabulary_family_at_its_lowest_tier() {
        let lowest: std::collections::BTreeMap<String, u8> = families().into_iter().collect();
        for entry in load_map().expect("the map parses") {
            match lowest.get(&entry.family) {
                Some(&tier) => assert_eq!(
                    entry.tier, tier,
                    "{} is filed at tier {} but its lowest vocabulary tier is {tier}",
                    entry.family, entry.tier,
                ),
                None => panic!("{} is not a support family", entry.family),
            }
        }
    }

    /// One seed per family. Two rows would install two samples under one key
    /// and spend the per-key budget a confirmation needs.
    #[test]
    fn no_family_appears_twice() {
        let map = load_map().expect("the map parses");
        let mut seen = std::collections::BTreeSet::new();
        for entry in &map {
            assert!(seen.insert(entry.family.clone()), "{} twice", entry.family);
        }
        assert_eq!(seen.len(), map.len());
    }

    /// The two name rules produce 47 + 4 hits, and every one of the seven
    /// hand-graded families is among them.
    ///
    /// Pinned because the counts are the spec's own measurement: a vocabulary
    /// update or a gem rename that moves them is a fact worth failing on, not
    /// a map that quietly seeds fewer families.
    #[test]
    fn the_name_rules_hit_forty_seven_plus_four_families() {
        let keys = gem_icon_keys();
        let mut exact = 0usize;
        let mut damage = 0usize;
        for (family, _) in families() {
            if keys.contains(&format!("{family} Support")) {
                exact += 1;
            } else if keys.contains(&format!("{family} Damage Support")) {
                damage += 1;
            }
        }
        assert_eq!((exact, damage), (47, 4));
        let map = load_map().expect("the map parses");
        assert_eq!(map.len(), exact + damage);
        for family in VISUAL_FAMILIES {
            assert!(
                map.iter().any(|e| e.family == family),
                "{family} is hand-graded but the name rules did not map it",
            );
        }
    }

    /// The measured floor. An empty or collapsed corpus grade must fail here
    /// rather than ship a map that seeds nothing and says nothing.
    ///
    /// **Seventeen, not the plan's twenty.** Twenty is the count of mapped
    /// families with a clean corpus crop; three of them do not verify, each
    /// for its own reason, all measured 2026-08-27 at the shipped calibration:
    ///
    /// - `Chance to Poison` — the player gem's art is a different picture
    ///   (0.680 against its own crops, where everything that verifies is
    ///   ≥ 0.92). A naming hit, not an art hit; the name rules cannot tell.
    /// - `Minion Life` — 0.854, just under `icon_match`, and its crop is
    ///   reached by `Minion Damage`'s art at 0.982. Two gems, one picture.
    /// - `Faster Projectiles` — reaches 0.889 but `Faster Attacks`'s seed
    ///   reaches the same crop at 0.874, so the read is `LowConfidence` for
    ///   want of `icon_lead`. That pair is then excluded from `enabled`
    ///   altogether by the pairwise rule.
    ///
    /// Raising this number is the follow-up curation ticket's job. Lowering it
    /// means something regressed.
    #[test]
    fn at_least_seventeen_entries_are_corpus_verified() {
        let n = load_map()
            .expect("the map parses")
            .iter()
            .filter(|e| e.verified == Verified::Corpus)
            .count();
        assert!(n >= 17, "only {n} corpus-verified entries");
    }

    /// The ruling, as a check: everything enabled has evidence behind it, and
    /// no `name`-only row is enabled.
    ///
    /// Only that direction. The converse does NOT hold and must not be
    /// asserted: a graded row can still be disabled, by the pairwise rule
    /// below.
    #[test]
    fn only_corpus_or_visual_entries_are_enabled() {
        for entry in load_map().expect("the map parses") {
            if entry.enabled {
                assert_ne!(
                    entry.verified,
                    Verified::Name,
                    "{} is enabled on the name rule alone",
                    entry.family,
                );
            }
        }
    }

    /// Every row that is graded but NOT enabled is one the pairwise rule took
    /// back — there is no third reason, and a hand-edit that switched one off
    /// for a fourth reason has to fail here.
    #[test]
    fn a_graded_row_is_disabled_only_by_the_pairwise_rule() {
        let t = Thresholds::default();
        let (g, scale) = seed_scale();
        let ceiling = t.icon_match - t.icon_lead;
        let map = load_map().expect("the map parses");
        let sig = |e: &SeedEntry| derive(&art(&e.gem), &g, scale).expect("the seed derives");
        for entry in map.iter().filter(|e| !e.enabled && e.verified != Verified::Name) {
            let mine = sig(entry);
            let partner = map
                .iter()
                .filter(|o| o.family != entry.family && o.verified != Verified::Name)
                .find(|o| mine.ncc(&sig(o)) >= ceiling);
            assert!(
                partner.is_some(),
                "{} is graded {:?} and disabled, but no other graded seed is inside its lead band",
                entry.family,
                entry.verified,
            );
        }
    }

    // -- derivation ---------------------------------------------------------

    /// The window is what the constants are fractions OF, so it has to be the
    /// number the matcher's alignment window really is at each scale.
    #[test]
    fn the_window_is_the_inner_crop_less_the_shift_margin() {
        let g = MercGeometry::default();
        assert_eq!(cell_px(&g, 1.0), 44);
        assert_eq!(window_px(&g, 1.0), 34);
        assert_eq!(cell_px(&g, 0.974), 43);
        assert_eq!(window_px(&g, 0.974), 33);
    }

    /// The fraction form is the whole reason both scales work: the art has to
    /// grow with the window, not stay at one pixel count.
    #[test]
    fn the_art_grows_with_the_window() {
        let g = MercGeometry::default();
        let big = render_cell(&art("Multistrike Support"), &g, 1.0, &SEED_ART);
        let small = render_cell(&art("Multistrike Support"), &g, 0.974, &SEED_ART);
        assert_eq!(image::GenericImageView::dimensions(&big), (44, 44));
        assert_eq!(image::GenericImageView::dimensions(&small), (43, 43));
        // Same art, two windows, one signature space: the derivations agree
        // far more closely than `icon_match`, which is what lets a seed
        // derived at 34 recognise a cell read at 33.
        let a = derive(&art("Multistrike Support"), &g, 1.0).expect("derives at 1.0");
        let b = derive(&art("Multistrike Support"), &g, 0.974).expect("derives at 0.974");
        assert!(a.ncc(&b) > 0.98, "cross-window self-correlation {}", a.ncc(&b));
    }

    /// Transparent art pixels take the cell background, not whatever the PNG
    /// stored under a zero alpha — and opaque ones keep their own colour.
    ///
    /// Checked at a background the shipped constant is NOT, because
    /// [`SEED_ART_BG`] is black and so is the canvas an uncomposited render
    /// would leave: at the shipped value the assertion would hold with the
    /// alpha ignored entirely.
    #[test]
    fn transparent_art_pixels_take_the_cell_background() {
        let g = MercGeometry::default();
        let loud = SeedArt {
            bg: [200, 100, 50],
            ..SEED_ART
        };
        let mut art = RgbaImage::from_pixel(78, 78, image::Rgba([255, 0, 255, 0]));
        // One fully opaque quadrant, so both sides of the composite are read.
        for y in 0..39 {
            for x in 0..39 {
                art.put_pixel(x, y, image::Rgba([10, 220, 30, 255]));
            }
        }
        let cell = render_cell(&art, &g, 1.0, &loud);
        let clear = image::GenericImageView::get_pixel(&cell, 36, 36).0;
        assert_eq!([clear[0], clear[1], clear[2]], [200, 100, 50]);
        let opaque = image::GenericImageView::get_pixel(&cell, 12, 12).0;
        assert_eq!([opaque[0], opaque[1], opaque[2]], [10, 220, 30]);
    }

    // -- calibration --------------------------------------------------------

    /// The sweep that produced the three constants, and the claim that they
    /// ARE its answer.
    ///
    /// Ignored by default because it derives 51 seeds at each of 8,580 points
    /// and takes about a minute in release; run it when the art, the corpus or
    /// the derivation changes:
    ///
    /// `cargo test --release --lib mercenary::seed -- --ignored --nocapture sweep`
    ///
    /// It prints the ranked table the constants' docs quote, and then asserts
    /// the top row is the shipped calibration — so the docs cannot drift away
    /// from the measurement without this failing.
    #[test]
    #[ignore = "calibration sweep — derives 8,580 candidate calibrations"]
    fn sweep_the_calibration_against_the_corpus() {
        let rows = candidate_rows();
        let t = Thresholds::default();
        let (g, scale) = seed_scale();
        let window = window_px(&g, scale) as f32;
        let clean = clean_crops();
        // Probes are independent of the calibration, so they are built once.
        let probes: Vec<(String, CellCandidates)> = clean
            .iter()
            .filter(|c| rows.iter().any(|(f, _, _)| *f == c.family))
            .map(|c| (c.family.clone(), probe(&c.file)))
            .collect();

        let bgs: [[u8; 3]; 5] = [
            [0, 0, 0],
            [8, 7, 6],
            [18, 16, 14],
            [26, 22, 17],
            [40, 34, 26],
        ];
        struct Row {
            matched: usize,
            min: f32,
            mean: f32,
            med: (i32, i32),
            label: String,
            p: SeedArt,
        }
        let mut table: Vec<Row> = Vec::new();
        for step in 0..=12 {
            let frac = 1.00 + 0.025 * step as f32;
            for dx in -4..=6 {
                for dy in -8..=4 {
                    for bg in bgs {
                        let p = SeedArt {
                            frac,
                            offset_frac: [dx as f32 / window, dy as f32 / window],
                            bg,
                        };
                        let seeds: std::collections::BTreeMap<String, CellSig> = rows
                            .iter()
                            .filter_map(|(f, gem, _)| {
                                derive_with(&art(gem), &g, scale, &p).map(|s| (f.clone(), s))
                            })
                            .collect();
                        let mut scores = Vec::new();
                        let mut matched = 0usize;
                        let (mut sx, mut sy) = (Vec::new(), Vec::new());
                        for (family, cell) in &probes {
                            let Some(seed) = seeds.get(family) else { continue };
                            let (score, dx, dy) = best_alignment(seed, cell);
                            scores.push(score);
                            sx.push(dx);
                            sy.push(dy);
                            if score >= t.icon_match {
                                matched += 1;
                            }
                        }
                        sx.sort();
                        sy.sort();
                        table.push(Row {
                            matched,
                            min: scores.iter().copied().fold(f32::MAX, f32::min),
                            mean: scores.iter().sum::<f32>() / scores.len() as f32,
                            med: (sx[sx.len() / 2], sy[sy.len() / 2]),
                            label: format!("{frac:.3} {dx:>2} {dy:>2} {bg:?}"),
                            p,
                        });
                    }
                }
            }
        }
        // The systematic component of the offset has to be ZERO, which is what
        // a median best shift of (0, 0) says: the ±3 px search exists for the
        // per-cell jitter `geometry::detect` produces, and a calibration that
        // spends it on a constant error leaves nothing for the cell that
        // jitters the same way. Ranking by score alone picks exactly that
        // calibration — the first run of this sweep did, and its winner's best
        // shift was pinned at the dy = -3 EDGE on 26 of the 29 crops, three
        // pixels of budget already gone before the cell's own jitter was
        // counted.
        table.retain(|r| r.med == (0, 0));
        table.sort_by(|a, b| {
            // matched, then the WORST family, then the mean: the worst is what
            // decides whether one more family verifies, and the mean only
            // separates points the worst has already tied.
            b.matched
                .cmp(&a.matched)
                .then(b.min.partial_cmp(&a.min).unwrap())
                .then(b.mean.partial_cmp(&a.mean).unwrap())
        });
        println!(
            "frac dx dy bg              matched/{} min   mean  median shift",
            probes.len()
        );
        for r in table.iter().take(25) {
            println!(
                "{:<28} {:>2} {:.3} {:.3} {:?}",
                r.label, r.matched, r.min, r.mean, r.med
            );
        }
        let winner = table[0].p;
        println!("\nper-family at the winner {}:", table[0].label);
        let seeds: std::collections::BTreeMap<String, CellSig> = rows
            .iter()
            .filter_map(|(f, gem, _)| {
                derive_with(&art(gem), &g, scale, &winner).map(|s| (f.clone(), s))
            })
            .collect();
        let mut per: Vec<(f32, String)> = probes
            .iter()
            .filter_map(|(family, cell)| {
                seeds.get(family).map(|seed| {
                    (
                        cell.all().iter().map(|c| seed.ncc(c)).fold(f32::MIN, f32::max),
                        family.clone(),
                    )
                })
            })
            .collect();
        per.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for (score, family) in &per {
            println!("  {score:.3} {family}");
        }

        // Compared through a DERIVATION rather than on the struct: the sweep
        // builds its fraction by arithmetic (`1.00 + 0.025 * 5`) and the
        // constant is written as a ratio, so two runs could name the same
        // calibration without holding the same f32. What matters is whether
        // the winner produces the shipped SIGNATURE, and that is what this
        // asks. The tolerance is 1e-4 rather than 1e-6 because the correlation
        // sums 657 f32 products: a signature against itself lands a few ulps
        // either side of 1.0 (1.0000014 here), never exactly on it.
        let shipped = derive(&art("Multistrike Support"), &g, scale).expect("derives");
        let won = derive_with(&art("Multistrike Support"), &g, scale, &winner).expect("derives");
        assert!(
            (shipped.ncc(&won) - 1.0).abs() < 1e-4,
            "the sweep's best calibration is {} — the shipped constants are not its answer",
            table[0].label,
        );
    }

    /// How many of the 24 clean crops of corpus-graded families must clear
    /// `icon_match` against their seed with NO alignment shift.
    ///
    /// Twelve, measured at the shipped calibration — half the corpus, and 8 of
    /// those 12 because their own best shift is exactly (0, 0). A floor rather
    /// than an equality so a calibration that helps MORE crops is not a
    /// failure; dropping below it means the art has moved off the window.
    const UNSHIFTED_FLOOR: usize = 12;

    /// The shipped constants, restated as two checks the sweep's ranking is
    /// built on: the calibration MATCHES the corpus, and it is not free to
    /// slide.
    ///
    /// 1. Every corpus-graded family's clean crops reach `icon_match` against
    ///    their own seed, and the MEDIAN best alignment over all of them is
    ///    (0, 0) — the ±3 px search is spent on `geometry::detect`'s per-cell
    ///    jitter, not on a constant this file got wrong. A systematic error
    ///    shows up in the median; per-cell jitter cannot move it.
    /// 1b. At least [`UNSHIFTED_FLOOR`] of those crops clear `icon_match`
    ///    with NO shift at all — the plan's unshifted clause, kept as far as
    ///    the corpus's own jitter allows. See [`corpus_verifies`] for why it
    ///    is a floor and not a universal.
    /// 2. The same seeds derived `2 · SHIFT_MAX + 1` = 7 px off do NOT match.
    ///    Seven and not six, measured: the search cancels 3 px on its own and
    ///    a crop whose own jitter runs the same way can hide up to 3 more, so
    ///    6 px is the last offset the pair can still explain between them —
    ///    at +5 six crops still match, at +6 two do (`Elemental Damage with
    ///    Attacks` at 0.906), at +7 none do and the best is 0.806. One pixel
    ///    past what alignment can account for is where the claim becomes
    ///    about the calibration rather than about the search.
    ///
    /// One test, because either half alone passes for the wrong reason — a
    /// derivation that matched everything at every offset would clear the
    /// first, and one that matched nothing would clear the second.
    #[test]
    fn the_shipped_calibration_matches_the_corpus_and_is_offset_sensitive() {
        let t = Thresholds::default();
        let (g, scale) = seed_scale();
        let window = window_px(&g, scale) as f32;
        let map = load_map().expect("the map parses");
        let corpus: Vec<&SeedEntry> = map.iter().filter(|e| e.verified == Verified::Corpus).collect();
        assert!(!corpus.is_empty(), "the map claims no corpus verification");

        let error_px = (2 * SHIFT_MAX + 1) as f32;
        let shifted = SeedArt {
            offset_frac: [
                SEED_ART_OFFSET_FRAC[0] + error_px / window,
                SEED_ART_OFFSET_FRAC[1],
            ],
            ..SEED_ART
        };
        let clean = clean_crops();
        let (mut dxs, mut dys) = (Vec::new(), Vec::new());
        let mut unshifted_hits = 0usize;
        let mut worst_unshifted = (f32::MAX, String::new());
        for entry in corpus {
            let seed = derive(&art(&entry.gem), &g, scale).expect("the seed derives");
            let off = derive_with(&art(&entry.gem), &g, scale, &shifted)
                .expect("the shifted seed derives");
            for crop in clean.iter().filter(|c| c.family == entry.family) {
                let cell = probe(&crop.file);
                let (hit, dx, dy) = best_alignment(&seed, &cell);
                let (miss, _, _) = best_alignment(&off, &cell);
                assert!(
                    hit >= t.icon_match,
                    "{} vs {}: {hit:.3} < icon_match",
                    entry.family,
                    crop.file,
                );
                assert!(
                    miss < t.icon_match,
                    "{} vs {} at offset +{error_px} px: {miss:.3} still matches — the \
                     ±{SHIFT_MAX} px search is absorbing the calibration",
                    entry.family,
                    crop.file,
                );
                dxs.push(dx);
                dys.push(dy);
                // The plan's clause, counted rather than required of every
                // crop. `into_centre` is the exact signature `learn` stores and
                // the pool carries, so this is the same comparison a device
                // makes when its own rects happen to land true.
                let centre = probe(&crop.file).into_centre();
                let unshifted = seed.ncc(&centre);
                if unshifted >= t.icon_match {
                    unshifted_hits += 1;
                }
                if unshifted < worst_unshifted.0 {
                    worst_unshifted = (unshifted, crop.file.clone());
                }
            }
        }
        assert!(
            unshifted_hits >= UNSHIFTED_FLOOR,
            "only {unshifted_hits} of {} corpus crops clear icon_match unshifted (floor \
             {UNSHIFTED_FLOOR}); worst is {} at {:.3}",
            dxs.len(),
            worst_unshifted.1,
            worst_unshifted.0,
        );
        dxs.sort();
        dys.sort();
        let median = (dxs[dxs.len() / 2], dys[dys.len() / 2]);
        assert_eq!(
            median,
            (0, 0),
            "the median best alignment over {} corpus crops is {median:?} — the calibration \
             carries a systematic offset the shift search is paying for",
            dxs.len(),
        );
    }

    /// The acceptance test (POE-208 WI-A): a store holding every ENABLED seed
    /// recognises every corpus-verified family, aligned and unshifted, and
    /// recognises nothing it should not.
    #[test]
    fn every_corpus_entry_matches_through_the_real_matcher() {
        let t = Thresholds::default();
        let map = load_map().expect("the map parses");
        let rows: Vec<(String, String, u8)> = map
            .iter()
            .filter(|e| e.enabled)
            .map(|e| (e.family.clone(), e.gem.clone(), e.tier))
            .collect();
        let store = seeded_store(&rows, &SEED_ART);
        assert_eq!(store.len(), rows.len(), "a seed yielded during the install");

        let mut verified = 0usize;
        for entry in map.iter().filter(|e| e.verified == Verified::Corpus) {
            assert!(
                corpus_verifies(&entry.family, &store, &t),
                "{} claims corpus verification but does not match",
                entry.family,
            );
            verified += 1;
        }
        assert!(verified >= 17, "only {verified} corpus-verified families");

        // The failure that costs most is not a miss, it is a confident wrong
        // name: a cell that reads `Matched` on a family it is not provokes no
        // hover and feeds the verdict silently. Two clauses, because there are
        // two ways to get one.
        let ceiling = t.icon_match - t.icon_lead;
        let mapped: std::collections::BTreeSet<&str> =
            map.iter().map(|e| e.family.as_str()).collect();
        for crop in clean_crops() {
            let read = store.match_family(&probe(&crop.file), &t);
            // 1. NO clean crop may resolve to another family — including the
            //    crops of mapped families the map chose not to enable. This is
            //    what makes disabling BOTH halves of a lead-band pair
            //    load-bearing. Measured: enable `Faster Attacks` alone and the
            //    `Faster Projectiles` crop reads `Faster Attacks` at 0.895,
            //    state `Matched` — a confident wrong name, because the family
            //    that would have contested the lead is no longer in the store.
            if read.state == crate::mercenary::ReadState::Matched {
                assert_eq!(
                    read.family.as_deref(),
                    Some(crop.family.as_str()),
                    "{} ({}) reads as another family at {:.3}",
                    crop.family,
                    crop.file,
                    read.score,
                );
            }
            // 2. A family the map does not carry at all must not come CLOSE
            //    either — inside the lead band it can still take a cell off a
            //    family that is seeded.
            if !mapped.contains(crop.family.as_str()) {
                assert!(
                    read.score < ceiling,
                    "{} ({}) is unmapped but reaches {:?} at {:.3} ≥ {ceiling:.2}",
                    crop.family,
                    crop.file,
                    read.family,
                    read.score,
                );
            }
        }
    }

    /// Two seeds inside the lead band would make BOTH their families
    /// permanently un-`Matched`, and the install gate (`icon_match`) would not
    /// catch it — it refuses at 0.88, and the damage starts at 0.83.
    #[test]
    fn no_two_enabled_seeds_are_inside_the_lead_band() {
        let t = Thresholds::default();
        let (g, scale) = seed_scale();
        let ceiling = t.icon_match - t.icon_lead;
        let seeds: Vec<(String, CellSig)> = load_map()
            .expect("the map parses")
            .into_iter()
            .filter(|e| e.enabled)
            .map(|e| {
                (
                    e.family.clone(),
                    derive(&art(&e.gem), &g, scale).expect("the seed derives"),
                )
            })
            .collect();
        for i in 0..seeds.len() {
            for j in i + 1..seeds.len() {
                let score = seeds[i].1.ncc(&seeds[j].1);
                assert!(
                    score < ceiling,
                    "{} and {} correlate at {score:.3} ≥ {ceiling:.2}",
                    seeds[i].0,
                    seeds[j].0,
                );
            }
        }
    }

    // -- the blocklist ------------------------------------------------------

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("merc-seed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        dir
    }

    /// A blocked family survives a restart and is not blocked twice.
    #[test]
    fn blocking_a_family_is_durable_and_idempotent() {
        let dir = tmp_dir("block");
        assert!(block_family(&dir, "Fork").expect("block"));
        assert!(!block_family(&dir, "Fork").expect("block again"));
        let list = SeedBlocklist::load(&dir);
        assert_eq!(list.families, vec!["Fork".to_string()]);
        assert!(list.blocks("Fork"));
        assert!(!list.blocks("Multistrike"));
    }

    /// A blocklisted family is not installed, however enabled its row is.
    #[test]
    fn a_blocklisted_family_is_not_installable() {
        let entries = vec![
            SeedEntry {
                family: "Fork".into(),
                gem: "Fork Support".into(),
                tier: 3,
                verified: Verified::Visual,
                enabled: true,
            },
            SeedEntry {
                family: "Multistrike".into(),
                gem: "Multistrike Support".into(),
                tier: 3,
                verified: Verified::Corpus,
                enabled: true,
            },
            SeedEntry {
                family: "Chain".into(),
                gem: "Chain Support".into(),
                tier: 1,
                verified: Verified::Name,
                enabled: false,
            },
        ];
        let blocked = SeedBlocklist {
            families: vec!["Fork".into()],
        };
        let out = installable(&entries, &blocked);
        assert_eq!(
            out.iter().map(|e| e.family.as_str()).collect::<Vec<_>>(),
            vec!["Multistrike"],
        );
    }

    /// A blocklist that does not parse reads as empty rather than taking the
    /// seeding down with it.
    #[test]
    fn an_unparseable_blocklist_reads_as_empty() {
        let dir = tmp_dir("corrupt");
        std::fs::write(dir.join(BLOCKLIST_FILE), "{ not json").expect("write");
        assert_eq!(SeedBlocklist::load(&dir), SeedBlocklist::default());
    }

    /// A reset takes both: the blocklist AND the cached art.
    #[test]
    fn clearing_seed_state_removes_the_blocklist_and_the_art() {
        let dir = tmp_dir("reset");
        block_family(&dir, "Fork").expect("block");
        std::fs::create_dir_all(art_dir(&dir)).expect("art dir");
        let cached = art_path(&dir, "Fork Support");
        std::fs::write(&cached, b"png").expect("write art");

        clear_seed_state(&dir).expect("clear");

        assert!(!dir.join(BLOCKLIST_FILE).exists(), "the blocklist survived");
        assert!(!cached.exists(), "the cached art survived");
        assert!(SeedBlocklist::load(&dir).families.is_empty());
        // Twice, because a reset on a device that never seeded must not fail.
        clear_seed_state(&dir).expect("clear again");
    }

    /// The cache path is the slug of the GEM, not of the family: two families
    /// can map to one gem, and a family-named file would fetch the same art
    /// twice.
    #[test]
    fn art_is_cached_under_the_gems_slug() {
        assert_eq!(art_slug("Added Chaos Damage Support"), "added-chaos-damage-support");
        assert_eq!(
            art_path(Path::new("/x/merc-icons"), "Fork Support"),
            Path::new("/x/merc-icons/seed/fork-support.png"),
        );
    }
}
