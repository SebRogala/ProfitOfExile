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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use image::{DynamicImage, RgbaImage};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::sync::watch;

use super::icons::{SeedInstall, TemplateStore, SHIFT_MAX};
use super::{icons::CellSig, MercGeometry, Thresholds};
use crate::AppState;
use tauri::Manager as _;

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
/// # Lock order — normative, and stated only here
///
/// **`icons::writing_icons_dir` → `AppState::merc_templates` → this lock.**
/// Every seed path takes a PREFIX or a SUFFIX of that chain and never inverts
/// it; the callers cite this doc rather than arguing the order again:
///
/// - `sync::apply_corpus` — directory, then store, then this one after both
///   are released;
/// - `debug::merc_forget_template` — directory, then store, then this one
///   while the store is still held;
/// - `debug::merc_forget_seed` — store, then this one while it is still held;
/// - `run.rs`'s hover tick — store, released, then this one; blocking after
///   the release is safe because the confirmation it evicted for is IN the
///   store, so a pass racing it yields to that sample rather than re-seeding
///   (see [`install_all`]);
/// - [`store_art`] and [`clear_seed_state`] — this lock alone.
///
/// [`SeedBlocklist::load`] takes NO lock: [`SeedBlocklist::save`] renames a
/// finished temporary over the file, so a reader sees one whole version or the
/// other and can be called from inside the store mutex.
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
    // BEFORE the deletion, inside the lock: a fetch pass that captured the old
    // epoch and is holding bytes it has not written yet must lose the race, and
    // it decides that by comparing epochs under this same lock. Bumping after
    // the deletion would leave a window in which a write is still accepted for
    // a reset that has already emptied the directory. See [`store_art`].
    SEED_EPOCH.fetch_add(1, Ordering::SeqCst);
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

// ---------------------------------------------------------------------------
// The fetch (L9) — one request per uncached family, serial, fail-soft
// ---------------------------------------------------------------------------

/// Counts resets of the seed cache.
///
/// [`clear_seed_state`] deletes the art directory; a fetch pass that started
/// before it may still be holding bytes for a file inside that directory, and
/// writing them afterwards would leave exactly the art the reset was pressed to
/// remove. The lock alone cannot decide that — it makes the two operations
/// atomic, not ordered — so the write carries the epoch it started under and
/// refuses when the number has moved.
static SEED_EPOCH: AtomicU64 = AtomicU64::new(0);

/// The reset counter a fetch pass should carry into its writes.
pub fn seed_epoch() -> u64 {
    SEED_EPOCH.load(Ordering::SeqCst)
}

/// Request timeout for one gem-art call.
///
/// The same 20 s [`super::sync`] gives a pool call, and for the same reason —
/// the shared HTTP client sets none. It has to clear the server's own 10 s
/// poewiki timeout, which is what a DEV server spends on the first request for
/// an icon its cache volume does not hold yet; prod answers from that volume.
const ART_TIMEOUT: Duration = Duration::from_secs(20);

/// How long the load seam waits for the fetch's cache pass.
///
/// Mirrors [`super::sync`]'s `STARTUP_WAIT`, and for the same reason: what is
/// waited for is a directory scan with no network in it, so the steady state
/// costs microseconds and the bound is only there so a wedged task cannot hold
/// the loop's first detect.
const INSTALL_WAIT: Duration = Duration::from_millis(1200);

/// Poll step while waiting out [`INSTALL_WAIT`].
const INSTALL_POLL: Duration = Duration::from_millis(50);

/// The scale seeds are first derived at — the reference geometry, window 34.
///
/// The live panel is whatever the game is running (0.974 on Sebastian's
/// 1920×1200), and the first detect that reports another window re-derives
/// through [`rederive_for_window`]. Deriving at 1.0 up front rather than
/// waiting for a detect is what makes a warm cache seed the FIRST capture.
const INSTALL_SCALE: f32 = 1.0;

/// What the cache and the blocklist say about the map, before any request.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FetchPlan {
    /// Rows whose art is already on disk — installable at once, no request.
    pub cached: Vec<SeedEntry>,
    /// Rows that need one request each, in map order.
    pub fetch: Vec<SeedEntry>,
    /// Enabled rows this device has blocklisted — counted, not fetched.
    pub blocklisted: usize,
}

/// Split the map into "already have it", "ask for it" and "never again".
///
/// `is_cached` is injected rather than reading the directory here so the split
/// is testable without a filesystem, and because the fetch loop re-asks the
/// same question per row just before it spends a request.
///
/// A cached file is NEVER re-fetched. The route serves
/// `Cache-Control: immutable` (ADR-012) and the art is a released game asset;
/// re-validating it every start would spend 18 requests to learn nothing.
pub fn plan_fetch(
    entries: &[SeedEntry],
    blocked: &SeedBlocklist,
    mut is_cached: impl FnMut(&SeedEntry) -> bool,
) -> FetchPlan {
    let eligible = installable(entries, blocked);
    // Through `installable`, so "who does this device seed" has ONE answer and
    // a blocklisted family cannot be re-fetched by a rule that forgot to ask.
    let blocklisted = entries.iter().filter(|e| e.enabled).count() - eligible.len();
    let mut plan = FetchPlan {
        blocklisted,
        ..Default::default()
    };
    for entry in eligible {
        if is_cached(&entry) {
            plan.cached.push(entry);
        } else {
            plan.fetch.push(entry);
        }
    }
    plan
}

/// What one gem-art request came back with, before it is judged.
///
/// Two variants rather than a `Result<Response>` because the transport failure
/// and the server's answer are different EVIDENCE even though they get the same
/// verdict — the log line names which one happened.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtReply {
    /// No response at all — offline, DNS, TLS, a timeout.
    Unreachable(String),
    /// An HTTP status and whatever body came with it.
    Response(u16, Vec<u8>),
}

/// Accept one reply as cacheable art, or say why the family stays unseeded.
///
/// **Everything that is not a 200 carrying a decodable PNG is unavailable, and
/// unavailable means nothing is written.** That absence IS the retry: the next
/// module start's [`plan_fetch`] finds no cached file and asks again. 404 (the
/// gem is not in the server's embedded map) and 502 (the server holds the name
/// but could not produce bytes) deliberately collapse into the same answer —
/// both are the server's, both can be temporary on a dev server that fetches
/// the wiki live, and neither is something this device can act on differently.
///
/// The format is pinned to PNG rather than sniffed: the route serves PNG, and
/// a proxy's HTML error page decodes as nothing, but a captive-portal login
/// screen served as a JPEG would decode as an image and cache as gem art.
pub fn accept_art(reply: ArtReply) -> Result<Vec<u8>, String> {
    let (status, body) = match reply {
        ArtReply::Unreachable(why) => return Err(format!("unreachable — {why}")),
        ArtReply::Response(status, body) => (status, body),
    };
    if status != 200 {
        return Err(format!("server returned {status}"));
    }
    match image::load_from_memory_with_format(&body, image::ImageFormat::Png) {
        Ok(_) => Ok(body),
        Err(e) => Err(format!("not a readable PNG — {e}")),
    }
}

/// `{server}/api/gem-icon/{gem}`, with the gem percent-encoded as one path
/// segment.
///
/// `server_url` carries no `/api` suffix — the JS `apiBase` does, and a URL
/// built by pasting the two together 404s. The segment goes through the URL
/// parser rather than a `format!` because every gem name has spaces in it and
/// four have none of the other characters that would make a hand-rolled
/// encoder look wrong until the map grows one.
///
/// `None` when `server` is not a parseable absolute URL, which is a
/// misconfigured device, not a failed fetch — the caller skips the pass.
pub fn art_url(server: &str, gem: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(server).ok()?;
    url.path_segments_mut().ok()?.pop_if_empty().extend(["api", "gem-icon", gem]);
    Some(url.to_string())
}

/// Cache one gem's art, unless a reset landed since `epoch`.
///
/// Under [`SEED_BLOCKLIST_LOCK`] — the same lock [`clear_seed_state`] holds
/// while it deletes the directory — so the write and the reset cannot
/// interleave, and the epoch decides which of them wins when they are
/// concurrent. `Ok(false)` means the reset won and nothing was written.
///
/// Temp-then-rename like every other file this module owns: a half-written PNG
/// would read as cached and never be fetched again.
pub fn store_art(icons_dir: &Path, gem: &str, bytes: &[u8], epoch: u64) -> Result<bool, String> {
    let _guard = SEED_BLOCKLIST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if seed_epoch() != epoch {
        return Ok(false);
    }
    let dir = art_dir(icons_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = art_path(icons_dir, gem);
    let tmp = path.with_extension("png.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(true)
}

/// What one fetch pass did, for the one line it logs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FetchSummary {
    pub fetched: usize,
    pub cached: usize,
    pub unavailable: usize,
    pub blocklisted: usize,
}

impl FetchSummary {
    /// Families with art on disk when the pass ended — what the headline
    /// number means, and NOT what the store installed: a seed can still yield
    /// to a learned sample of another family.
    pub fn seeded(&self) -> usize {
        self.fetched + self.cached
    }
}

/// The fetch's one summary line.
pub fn fetch_line(s: &FetchSummary) -> String {
    format!(
        "Merc: seeded {} families ({} fetched, {} cached, {} unavailable, {} blocklisted)",
        s.seeded(),
        s.fetched,
        s.cached,
        s.unavailable,
        s.blocklisted,
    )
}

// ---------------------------------------------------------------------------
// Install (memory only) and the per-window re-derivation
// ---------------------------------------------------------------------------

/// What one install pass did to the store.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InstallTally {
    pub installed: usize,
    /// Yielded to a stored sample of ANOTHER family — the incumbent wins and
    /// nothing is blocklisted (see [`TemplateStore::install_seed`]).
    pub yielded: usize,
    /// The `(family, tier)` key was already at its sample ceiling.
    pub key_full: usize,
    /// Blocklisted between the derivation and the install — the ✕ or a forget
    /// landed in the gap.
    pub blocked: usize,
}

/// Decode one cached art file.
///
/// A file that is present but does not decode is DELETED, not just skipped: it
/// reads as cached forever otherwise, so the family would never be seeded and
/// never re-fetched either. A truncated download and a half-written file are
/// the two ways to get one, and both want the same answer — ask again next
/// start.
fn read_art(icons_dir: &Path, gem: &str) -> Option<RgbaImage> {
    let path = art_path(icons_dir, gem);
    let bytes = std::fs::read(&path).ok()?;
    match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) {
        Ok(img) => Some(img.to_rgba8()),
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

/// One signature per eligible family whose art is cached, at this scale.
///
/// The blocklist is applied HERE, through [`installable`], and not only at the
/// fetch: the art of an evicted family stays in the cache (a reset is the only
/// thing that removes it), so a pass that skipped this filter would re-install
/// exactly the seed an eviction threw out.
///
/// `memo` holds the signatures already derived AT THIS WINDOW, so a window the
/// panel oscillates back to costs no derivation. Missing entries are filled in,
/// which is what makes a partly-filled memo — one late fetch installed on its
/// own — heal itself rather than lock a family out.
pub fn derive_installable(
    icons_dir: &Path,
    entries: &[SeedEntry],
    blocked: &SeedBlocklist,
    g: &MercGeometry,
    scale: f32,
    memo: &mut BTreeMap<String, CellSig>,
) -> Vec<(String, u8, CellSig)> {
    let mut out = Vec::new();
    for entry in installable(entries, blocked) {
        let sig = match memo.get(&entry.family) {
            Some(sig) => sig.clone(),
            None => {
                let Some(sig) = read_art(icons_dir, &entry.gem).and_then(|art| derive(&art, g, scale))
                else {
                    continue;
                };
                memo.insert(entry.family.clone(), sig.clone());
                sig
            }
        };
        out.push((entry.family, entry.tier, sig));
    }
    out
}

/// Put `seeds` into the store, one per family.
///
/// **Forget-then-install, always**, and that is what makes both callers safe.
/// [`TemplateStore::install_seed`] refuses a sample of ANOTHER family and a key
/// already at its ceiling, but it has no opinion about a SECOND seed of its own
/// family — so the start-up pass and a late fetch that both name one family
/// would otherwise file two. It is also how the re-derivation replaces: the new
/// window's signature takes the old one's place instead of joining it.
///
/// `blocked` is re-read by the caller UNDER the store mutex and applied here,
/// not at the derivation: the two forget doors write the blocklist while
/// holding that same mutex, so a ✕ pressed after this pass derived its
/// signatures is still seen. The two doors that block AFTER releasing the mutex
/// — the hover tick's confirm and the pull's merge — can still land in the gap.
/// Normally that costs nothing: both evicted BECAUSE another family's sample
/// took that art, and [`TemplateStore::install_seed`] yields to it. The merge
/// has two paths where the served sample is evicted-for but not installed
/// (refused after a later collision, or a full key), and NCC does not
/// transit, so a seed can come back for ONE session there; the blocklist
/// entry still lands and the next start does not re-derive it.
///
/// Memory only. The caller holds the store mutex and NOTHING else — a seed is
/// never written to `index.json`, so the directory lock this would otherwise
/// need is not owed, and taking it would put the detect tick behind a PNG
/// write. Lock order: see [`SEED_BLOCKLIST_LOCK`].
pub fn install_all(
    store: &mut TemplateStore,
    seeds: &[(String, u8, CellSig)],
    blocked: &SeedBlocklist,
    t: &Thresholds,
) -> InstallTally {
    let mut tally = InstallTally::default();
    for (family, tier, sig) in seeds {
        if blocked.blocks(family) {
            tally.blocked += 1;
            continue;
        }
        store.forget_seed(family);
        match store.install_seed(family, *tier, sig.clone(), t) {
            SeedInstall::Installed => tally.installed += 1,
            SeedInstall::YieldedTo { .. } => tally.yielded += 1,
            SeedInstall::KeyFull => tally.key_full += 1,
        }
    }
    tally
}

/// What a detect at some window owes the installed seeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowPlan {
    /// The store's seeds are already derived at this window — nothing to do.
    Ready,
    /// Signatures for this window are memoised: re-install them, quietly.
    Reinstall,
    /// Nothing is memoised at this window — derive, and say so once.
    Derive,
}

/// Decide what a detect reporting `window` costs.
///
/// The memo is keyed on the WINDOW IN PIXELS rather than on the scale: two
/// scales that round to the same window need one derivation, and a scale step
/// too small to move the window changes nothing the matcher can see. The
/// panel's scale is measured per detect from the observed row pitch, so it
/// jitters — a plan that only remembered "this window was done once" would
/// freeze the store at whichever of 33 and 34 the loop saw first and never
/// return. Remembering the SIGNATURES instead makes the return trip free and
/// still derives at most once per window per session.
pub fn window_plan(current: Option<i32>, memoised: impl Fn(i32) -> bool, window: i32) -> WindowPlan {
    if current == Some(window) {
        WindowPlan::Ready
    } else if memoised(window) {
        WindowPlan::Reinstall
    } else {
        WindowPlan::Derive
    }
}

/// Everything the fetch task and the detect path need after the load seam has
/// installed the store.
#[derive(Debug, Clone)]
struct InstallContext {
    icons_dir: PathBuf,
    geometry: MercGeometry,
    /// The scale the seeds are currently derived at — 1.0 until a detect
    /// reports another window.
    scale: f32,
}

/// Session state for the seeding: the single-flight claim, the seam gate, and
/// the per-window memo.
#[derive(Default)]
struct SeedState {
    /// A fetch pass is running.
    fetching: bool,
    /// The running pass has finished its no-network look at the cache, so
    /// [`wait_for_install`] may stop waiting.
    cache_scanned: bool,
    /// `Some` once the load seam has installed the store. Until then a fetch
    /// that lands must NOT install: the seam's whole-store assignment would
    /// erase it, which is the writer race POE-207 removed from the pull.
    install: Option<InstallContext>,
    /// The window the store's seeds are derived at.
    window: Option<i32>,
    /// Window px → the signatures derived at it.
    derived: BTreeMap<i32, BTreeMap<String, CellSig>>,
    /// [`SEED_MAP_JSON`] parsed once.
    ///
    /// The bytes are `include_str!`, so they cannot change while the process
    /// runs and re-parsing 51 rows on every window flip buys nothing. NOT
    /// cleared by [`forget_session`]: a reset throws out this device's state,
    /// and the map is the build's.
    map: Option<Vec<SeedEntry>>,
}

/// A process-wide `static` on the [`SEED_BLOCKLIST_LOCK`] model: the state
/// belongs to the module's session, not to a window, and three threads reach it
/// — the capture loop, the fetch task, and whichever tick re-derives.
///
/// **Never held across the store mutex.** Every function below takes it, reads
/// or writes plain data, and drops it before touching `merc_templates`.
static SEED_STATE: Mutex<SeedState> = Mutex::new(SeedState {
    fetching: false,
    cache_scanned: false,
    install: None,
    window: None,
    derived: BTreeMap::new(),
    map: None,
});

fn with_seed_state<T>(f: impl FnOnce(&mut SeedState) -> T) -> T {
    let mut state = SEED_STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut state)
}

/// [`load_map`], parsed once per process.
fn map_once() -> Result<Vec<SeedEntry>, String> {
    if let Some(map) = with_seed_state(|s| s.map.clone()) {
        return Ok(map);
    }
    let map = load_map()?;
    with_seed_state(|s| s.map = Some(map.clone()));
    Ok(map)
}

/// Throw away everything this session learned about the seeds.
///
/// **Called by `merc_reset_templates`**, and it is the half a reset cannot do
/// by deleting files. The art directory and the blocklist go on disk, but the
/// DERIVED signatures live here: without this, the next detect at a different
/// window finds them memoised, re-installs every family from a memo that never
/// reads the deleted art, passes the (now empty) blocklist, and the store fills
/// back up while the page's seed group stays empty — no chip, and so no ✕ to
/// press again.
///
/// `install` goes with them, so nothing re-seeds until the next module start
/// has fetched and re-derived; the fetch claim does not, because a pass in
/// flight still owns its own release.
pub fn forget_session() {
    with_seed_state(|s| {
        s.install = None;
        s.window = None;
        s.derived.clear();
    });
}

/// Claim the fetch for this module start. `true` when this caller owns the pass.
///
/// Single-flight over the REQUESTS: a module toggled repeatedly gets one pass,
/// not one per toggle, and the second caller's [`wait_for_install`] waits on
/// the first's cache scan because the art it is fetching is the same art.
///
/// Pure over [`SeedState`] so the claim/release cycle is testable without an
/// app: a claim that leaked would silence every later session's seeding, and
/// the only symptom would be families that stop being seeded after a toggle.
fn claim_fetch(s: &mut SeedState) -> bool {
    if s.fetching {
        return false;
    }
    s.fetching = true;
    // The new pass has not looked at the cache yet, so the seam must wait for
    // it rather than read the last pass's answer.
    s.cache_scanned = false;
    true
}

/// Release the claim, opening the seam whether or not the pass got that far.
fn release_fetch(s: &mut SeedState) {
    s.cache_scanned = true;
    s.fetching = false;
}

/// Start the module-start fetch, beside the pool pull.
///
/// Single-flight: a module toggled repeatedly gets one pass, not one per
/// toggle. A pass already in flight keeps its claim and the new loop's
/// [`wait_for_install`] waits on it — the art it is fetching is the same art.
///
/// The caller stays on its own thread; this returns immediately.
pub fn spawn_fetch(app: &AppHandle) {
    // UNCONDITIONAL, and outside the claim below. The store this loop is about
    // to install carries no seeds (they are never saved), so the window and the
    // signatures the last session derived say nothing about it — and a rapid
    // off→on toggle is exactly the case where the claim is refused, which would
    // otherwise leave session 2 running on session 1's install context and
    // memo.
    forget_session();
    if !with_seed_state(claim_fetch) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { fetch_pass(app).await });
}

/// One serial pass over the map: scan the cache, then ask for what is missing.
///
/// SERIAL on purpose. A dev server fetches each icon from the wiki on first
/// request behind a 10 s timeout, so 30 parallel requests would be 30 parallel
/// wiki fetches; and the cache pass in front means the steady state issues no
/// request at all.
async fn fetch_pass(app: AppHandle) {
    // Released on EVERY exit, including the two below that never reach the
    // network: `wait_for_install` waits on `cache_scanned`, so a pass that gave
    // up without setting it would cost the loop the whole bounded window.
    fn done() {
        with_seed_state(release_fetch);
    }

    let Some(dir) = super::sync::icons_dir(&app) else {
        done();
        return;
    };
    let entries = match map_once() {
        Ok(entries) => entries,
        Err(e) => {
            crate::app_log(&app, format!("Merc: no seeds — {e}"));
            done();
            return;
        }
    };
    let blocked = SeedBlocklist::load(&dir);
    let plan = plan_fetch(&entries, &blocked, |e| art_path(&dir, &e.gem).exists());
    let mut summary = FetchSummary {
        cached: plan.cached.len(),
        blocklisted: plan.blocklisted,
        ..Default::default()
    };
    // The cache pass is over — everything below is network. `wait_for_install`
    // is released HERE, so a warm start installs without waiting on a request.
    with_seed_state(|s| s.cache_scanned = true);

    let epoch = seed_epoch();
    let (server, http) = super::sync::server_and_http(&app);
    for entry in &plan.fetch {
        // Re-asked per row rather than trusted from the plan: two rows may name
        // one gem, and the first of them wrote the file the second would spend
        // a second request on.
        if art_path(&dir, &entry.gem).exists() {
            summary.cached += 1;
            continue;
        }
        let Some(url) = art_url(&server, &entry.gem) else {
            crate::app_log(
                &app,
                format!("Merc: no seeds — {server} is not a usable server address"),
            );
            break;
        };
        let reply = match http.get(&url).timeout(ART_TIMEOUT).send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                match response.bytes().await {
                    Ok(body) => ArtReply::Response(status, body.to_vec()),
                    Err(e) => ArtReply::Unreachable(e.to_string()),
                }
            }
            Err(e) => ArtReply::Unreachable(e.to_string()),
        };
        match accept_art(reply).and_then(|bytes| store_art(&dir, &entry.gem, &bytes, epoch)) {
            Ok(true) => {
                summary.fetched += 1;
                // Through the same function the seam uses, so art that lands
                // late is installed exactly as art that was already cached.
                install_late(&app, &entry.family);
            }
            // The cache was reset under this pass. Everything still to fetch
            // belongs to a store the user just emptied; the next module start
            // asks again.
            Ok(false) => break,
            Err(why) => {
                summary.unavailable += 1;
                crate::app_log(
                    &app,
                    format!("Merc: {} will not be seeded this session — {why}", entry.family),
                );
            }
        }
    }
    crate::app_log(&app, fetch_line(&summary));
    done();
}


/// Install one family whose art has just landed, if the seam has opened.
///
/// Silent when it has not: the load seam's whole-store assignment is still
/// ahead, and anything installed before it would be erased by it.
fn install_late(app: &AppHandle, family: &str) {
    let Some(ctx) = with_seed_state(|s| s.install.clone()) else {
        return;
    };
    install_pass(app, &ctx, Some(family));
}

/// Derive and install every eligible family whose art is cached.
///
/// The ONE install path — the load seam, a late fetch and the re-derivation all
/// come through here. `only` narrows it to one family (a late fetch); `None` is
/// the whole map.
///
/// Derivation happens OUTSIDE the store mutex and the install inside one
/// acquisition of it, so a detect tick waits on the vector work and not on
/// decoding eighteen PNGs.
fn install_pass(app: &AppHandle, ctx: &InstallContext, only: Option<&str>) -> InstallTally {
    let entries = match map_once() {
        Ok(entries) => entries,
        Err(_) => return InstallTally::default(),
    };
    let entries: Vec<SeedEntry> = match only {
        Some(family) => entries.into_iter().filter(|e| e.family == family).collect(),
        None => entries,
    };
    // Read once here to keep the derivation off blocked families — the art of
    // an evicted family stays in the cache, so deriving it would be work spent
    // on a signature the install below throws away.
    let blocked = SeedBlocklist::load(&ctx.icons_dir);
    let window = window_px(&ctx.geometry, ctx.scale);

    let mut memo = with_seed_state(|s| s.derived.get(&window).cloned().unwrap_or_default());
    let seeds = derive_installable(
        &ctx.icons_dir,
        &entries,
        &blocked,
        &ctx.geometry,
        ctx.scale,
        &mut memo,
    );
    with_seed_state(|s| s.derived.entry(window).or_default().extend(memo));

    let state = app.state::<AppState>();
    let mut store = state
        .merc_templates
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // AND AGAIN under the store mutex, which is the read that decides. The
    // derivation above can take milliseconds, and both forget doors write the
    // blocklist while HOLDING this mutex — so a ✕ pressed in that gap is
    // invisible to the first read and seen by this one. See [`install_all`] for
    // why the two collision doors need no such cover.
    let blocked = SeedBlocklist::load(&ctx.icons_dir);
    install_all(&mut store, &seeds, &blocked, &ctx.geometry.thresholds)
}

/// Install the seeds whose art is already cached, then let later art in.
///
/// Sits between the loop's whole-store assignment and `sync::wait_for_pull`:
/// the store is in place, so this writes into the one the session will use, and
/// the pull's merge still runs after it under the same mutex.
///
/// Blocking, bounded by [`INSTALL_WAIT`] and by `cancel` — the same shape as
/// `sync::wait_for_pull`, and cheap for the same reason: what it waits for is a
/// directory scan, and the first detect only matters once a mercenary speaks.
pub fn wait_for_install(
    app: &AppHandle,
    cancel: &watch::Receiver<bool>,
    icons_dir: &Path,
    geometry: &MercGeometry,
) {
    let deadline = Instant::now() + INSTALL_WAIT;
    while !with_seed_state(|s| s.cache_scanned) {
        if Instant::now() >= deadline || *cancel.borrow() {
            break;
        }
        std::thread::sleep(INSTALL_POLL);
    }
    let ctx = InstallContext {
        icons_dir: icons_dir.to_path_buf(),
        geometry: geometry.clone(),
        scale: INSTALL_SCALE,
    };
    // OPENED FIRST, so art landing during the pass below installs itself rather
    // than waiting for a re-derivation. Both doors are forget-then-install, so
    // naming one family twice still leaves one seed.
    with_seed_state(|s| s.install = Some(ctx.clone()));
    let tally = install_pass(app, &ctx, None);
    with_seed_state(|s| s.window = Some(window_px(geometry, INSTALL_SCALE)));
    if tally.installed + tally.yielded + tally.key_full + tally.blocked > 0 {
        crate::app_log(
            app,
            format!(
                "Merc: {} seed template(s) installed at window {} ({} yielded to a learned sample, \
                 {} key(s) already full, {} blocklisted mid-pass)",
                tally.installed,
                window_px(geometry, INSTALL_SCALE),
                tally.yielded,
                tally.key_full,
                tally.blocked,
            ),
        );
    }
}

/// Re-derive the installed seeds when a detect reports a different window (L10).
///
/// Called on the detect path BEFORE the frame is matched, so the signatures the
/// match runs against belong to the panel in front of it. The ±3 px alignment
/// search cannot stand in for this: it slides the probe, it does not resample
/// it, and what changes with the scale is the resampling fraction.
///
/// Does nothing until the load seam has opened — before that there is no store
/// to write into that the seam will not overwrite.
pub fn rederive_for_window(app: &AppHandle, geometry: &MercGeometry, scale: f32) {
    let Some(mut ctx) = with_seed_state(|s| s.install.clone()) else {
        return;
    };
    let window = window_px(geometry, scale);
    let plan = with_seed_state(|s| {
        window_plan(s.window, |w| s.derived.contains_key(&w), window)
    });
    if plan == WindowPlan::Ready {
        return;
    }
    // The GEOMETRY the loop is reading with, not the one the seam stored: an
    // override can only change between sessions, but the seeds must be rendered
    // into the same cell the detect measured either way.
    ctx.geometry = geometry.clone();
    ctx.scale = scale;
    let tally = install_pass(app, &ctx, None);
    with_seed_state(|s| {
        s.window = Some(window);
        s.install = Some(ctx);
    });
    if plan == WindowPlan::Derive {
        crate::app_log(
            app,
            format!(
                "Merc: re-derived {} seed(s) for a {window} px window (panel scale {scale:.3})",
                tally.installed,
            ),
        );
    }
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

    /// Serialises the tests that move [`SEED_EPOCH`].
    ///
    /// The epoch is process-wide, as it must be — one app, one seed cache — so
    /// a reset in one test invalidates a write another test has already
    /// approved. Only the tests that RESET take this; the rest cache their art
    /// through [`write_art`], which does not read the epoch at all.
    static EPOCH_TESTS: Mutex<()> = Mutex::new(());

    fn epoch_guard() -> std::sync::MutexGuard<'static, ()> {
        EPOCH_TESTS.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Serialises the tests that write the process-wide [`SEED_STATE`].
    ///
    /// One app, one seeding session, so the state is a `static` — and a test
    /// that plants a memo in it must not have a neighbour clear it. The
    /// claim/release tests use a LOCAL [`SeedState`] and take nothing.
    static SESSION_TESTS: Mutex<()> = Mutex::new(());

    fn session_guard() -> std::sync::MutexGuard<'static, ()> {
        SESSION_TESTS.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("merc-seed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        dir
    }

    /// A blocked family survives a restart — the blocklist is the ONLY seed
    /// state that does, so an eviction that did not reach the file is an
    /// eviction the next start undoes.
    #[test]
    fn a_blocked_family_survives_a_restart() {
        let dir = tmp_dir("block");

        assert!(block_family(&dir, "Fork").expect("block"));

        let list = SeedBlocklist::load(&dir);
        assert!(list.blocks("Fork"));
        assert_eq!(list.families, vec!["Fork".to_string()]);
    }

    /// Blocking the same family twice reports the second as a no-op, so the
    /// caller's "will not be seeded again" line is said once.
    #[test]
    fn blocking_a_family_twice_is_a_no_op() {
        let dir = tmp_dir("block-twice");
        block_family(&dir, "Fork").expect("block");

        assert!(!block_family(&dir, "Fork").expect("block again"));

        assert_eq!(SeedBlocklist::load(&dir).families, vec!["Fork".to_string()]);
    }

    /// Blocking one family blocks only that one.
    #[test]
    fn blocking_a_family_leaves_the_others_seedable() {
        let dir = tmp_dir("block-one");
        block_family(&dir, "Fork").expect("block");

        assert!(!SeedBlocklist::load(&dir).blocks("Multistrike"));
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

    /// A reset drops the blocklist: nothing is un-seedable once there is no
    /// store left to explain why.
    #[test]
    fn clearing_seed_state_removes_the_blocklist() {
        let _serial = epoch_guard();
        let dir = tmp_dir("reset-blocklist");
        block_family(&dir, "Fork").expect("block");

        clear_seed_state(&dir).expect("clear");

        assert!(!dir.join(BLOCKLIST_FILE).exists(), "the blocklist file survived");
        assert!(SeedBlocklist::load(&dir).families.is_empty());
    }

    /// And the cached art with it — a reset says start over, and leaving the
    /// art would keep the very pictures the user is resetting away from.
    #[test]
    fn clearing_seed_state_removes_the_cached_art() {
        let _serial = epoch_guard();
        let dir = tmp_dir("reset-art");
        std::fs::create_dir_all(art_dir(&dir)).expect("art dir");
        let cached = art_path(&dir, "Fork Support");
        std::fs::write(&cached, b"png").expect("write art");

        clear_seed_state(&dir).expect("clear");

        assert!(!cached.exists(), "the cached art survived");
    }

    /// A reset on a device that never seeded is success, not an error.
    #[test]
    fn clearing_seed_state_on_an_untouched_device_is_not_an_error() {
        let _serial = epoch_guard();
        let dir = tmp_dir("reset-untouched");

        clear_seed_state(&dir).expect("clear an untouched device");
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

    // -- the fetch (WI-B) ---------------------------------------------------

    /// A map row, spelled out so each fetch test says what it is varying.
    fn row(family: &str, gem: &str, enabled: bool) -> SeedEntry {
        SeedEntry {
            family: family.into(),
            gem: gem.into(),
            tier: 3,
            verified: Verified::Corpus,
            enabled,
        }
    }

    /// The three rows every plan test starts from: two enabled, one disabled.
    fn plan_rows() -> Vec<SeedEntry> {
        vec![
            row("Fork", "Fork Support", true),
            row("Multistrike", "Multistrike Support", true),
            row("Chain", "Chain Support", false),
        ]
    }

    /// The bytes of a real gem art file — the only PNG in this suite that is
    /// certainly what the route serves.
    fn art_bytes(gem: &str) -> Vec<u8> {
        let path = std::path::Path::new(ART_DIR).join(format!("{}.png", art_slug(gem)));
        std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e} — {FETCH_HINT}", path.display()))
    }

    /// Art already on disk costs no request — the whole reason a warm start
    /// seeds without a network.
    #[test]
    fn a_cached_family_is_never_asked_for_again() {
        let plan = plan_fetch(&plan_rows(), &SeedBlocklist::default(), |e| {
            e.family == "Fork"
        });

        assert_eq!(
            plan.cached.iter().map(|e| e.family.as_str()).collect::<Vec<_>>(),
            vec!["Fork"],
        );
        assert!(
            !plan.fetch.iter().any(|e| e.family == "Fork"),
            "the cached family was queued for a request anyway",
        );
    }

    /// One request per uncached ENABLED family, and none for a disabled row —
    /// the requests are the plan's `fetch` list, in map order.
    #[test]
    fn every_uncached_enabled_family_is_asked_for_exactly_once() {
        let plan = plan_fetch(&plan_rows(), &SeedBlocklist::default(), |_| false);

        assert_eq!(
            plan.fetch.iter().map(|e| e.gem.as_str()).collect::<Vec<_>>(),
            vec!["Fork Support", "Multistrike Support"],
        );
        assert!(plan.cached.is_empty());
    }

    /// A blocklisted family is counted for the summary and asked for by
    /// nobody: art it has no cache entry for would otherwise be re-fetched
    /// every start for a seed that is never installed.
    #[test]
    fn a_blocklisted_family_is_counted_and_not_requested() {
        let blocked = SeedBlocklist {
            families: vec!["Fork".into()],
        };

        let plan = plan_fetch(&plan_rows(), &blocked, |_| false);

        assert_eq!(plan.blocklisted, 1);
        assert_eq!(
            plan.fetch.iter().map(|e| e.family.as_str()).collect::<Vec<_>>(),
            vec!["Multistrike"],
        );
        assert!(plan.cached.is_empty());
    }

    /// A server that cannot be reached at all leaves the family unseeded.
    #[test]
    fn an_unreachable_server_yields_no_art() {
        let why = accept_art(ArtReply::Unreachable("dns error".into()))
            .expect_err("an unreachable server produced art");

        assert!(why.contains("dns error"), "{why}");
    }

    /// A gem the server's map does not carry answers 404, and 404 is not art.
    #[test]
    fn a_404_yields_no_art() {
        let why = accept_art(ArtReply::Response(404, b"not found".to_vec()))
            .expect_err("a 404 produced art");

        assert!(why.contains("404"), "{why}");
    }

    /// A 502 — the server holds the name but could not produce bytes — is the
    /// same non-answer as a 404, and must not be cached as art.
    #[test]
    fn a_502_yields_no_art() {
        let why = accept_art(ArtReply::Response(502, art_bytes("Fork Support")))
            .expect_err("a 502 produced art");

        assert!(why.contains("502"), "{why}");
    }

    /// A 200 carrying something that is not a PNG is refused. A proxy's error
    /// page arrives exactly like this, and caching it would seed the family
    /// with a picture of an error.
    #[test]
    fn a_body_that_is_not_a_png_yields_no_art() {
        let why = accept_art(ArtReply::Response(200, b"<html>login</html>".to_vec()))
            .expect_err("an HTML body produced art");

        assert!(why.contains("PNG"), "{why}");
    }

    /// A 200 carrying a JPEG is refused too. This is the case the FORMAT PIN
    /// buys: a sniffing decoder would take it, and the family would be seeded
    /// with whatever a captive portal or a mis-configured proxy served.
    #[test]
    fn a_body_that_is_a_jpeg_yields_no_art() {
        let mut jpeg = Vec::new();
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 16, image::Rgba([90, 40, 20, 255])))
            .to_rgb8()
            .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .expect("encode a JPEG");

        let why = accept_art(ArtReply::Response(200, jpeg)).expect_err("a JPEG produced art");

        assert!(why.contains("PNG"), "{why}");
    }

    /// The accepted path hands back the bytes AS SERVED — the cache holds the
    /// file the route sent, not a re-encoding of it.
    #[test]
    fn a_200_carrying_a_png_is_accepted_byte_for_byte() {
        let bytes = art_bytes("Fork Support");

        let accepted = accept_art(ArtReply::Response(200, bytes.clone())).expect("accepted");

        assert_eq!(accepted, bytes);
    }

    /// Unavailable means NOTHING IS WRITTEN, and that absence is the retry:
    /// the next start plans the same request again. The contrast is the point
    /// — the family that succeeded is not asked for twice.
    #[test]
    fn an_unavailable_family_is_asked_for_again_next_start() {
        let dir = tmp_dir("retry");
        let rows = plan_rows();
        let cached = |e: &SeedEntry| art_path(&dir, &e.gem).exists();

        // First pass: Fork's reply is refused, Multistrike's is accepted.
        let first = plan_fetch(&rows, &SeedBlocklist::default(), cached);
        assert_eq!(first.fetch.len(), 2);
        assert!(accept_art(ArtReply::Response(502, Vec::new())).is_err());
        let art = accept_art(ArtReply::Response(200, art_bytes("Multistrike Support")))
            .expect("accepted");
        write_art(&dir, "Multistrike Support", &art);

        let second = plan_fetch(&rows, &SeedBlocklist::default(), cached);

        assert_eq!(
            second.fetch.iter().map(|e| e.family.as_str()).collect::<Vec<_>>(),
            vec!["Fork"],
            "the refused family was not asked for again",
        );
        assert_eq!(
            second.cached.iter().map(|e| e.family.as_str()).collect::<Vec<_>>(),
            vec!["Multistrike"],
        );
    }

    /// A reset that lands while a pass is holding bytes WINS: the art
    /// directory the user just emptied does not fill back up behind them.
    #[test]
    fn a_reset_under_a_running_fetch_refuses_the_write() {
        let _serial = epoch_guard();
        let dir = tmp_dir("reset-race");
        let epoch = seed_epoch();
        clear_seed_state(&dir).expect("reset");

        let wrote = store_art(&dir, "Fork Support", &art_bytes("Fork Support"), epoch)
            .expect("the write is not an error");

        assert!(!wrote, "the write went in over the reset");
        assert!(!art_path(&dir, "Fork Support").exists());
        // And the next pass, which carries the new epoch, writes normally.
        assert!(store_art(&dir, "Fork Support", &art_bytes("Fork Support"), seed_epoch())
            .expect("write"));
        assert!(art_path(&dir, "Fork Support").exists());
    }

    /// The summary line carries all four counts and the headline, which is
    /// what the Windows smoke reads to know a cold start worked.
    #[test]
    fn the_summary_line_reports_every_outcome() {
        let line = fetch_line(&FetchSummary {
            fetched: 4,
            cached: 11,
            unavailable: 2,
            blocklisted: 1,
        });

        assert_eq!(
            line,
            "Merc: seeded 15 families (4 fetched, 11 cached, 2 unavailable, 1 blocklisted)",
        );
    }

    /// The gem is one percent-encoded path segment under `/api/gem-icon`, and
    /// `server_url` carries no `/api` of its own.
    #[test]
    fn the_art_url_encodes_the_gem_under_the_api_prefix() {
        assert_eq!(
            art_url("https://profitofexile.top", "Added Chaos Damage Support").as_deref(),
            Some("https://profitofexile.top/api/gem-icon/Added%20Chaos%20Damage%20Support"),
        );
        // A trailing slash on the configured server must not double up.
        assert_eq!(
            art_url("https://profitofexile.top/", "Fork Support").as_deref(),
            Some("https://profitofexile.top/api/gem-icon/Fork%20Support"),
        );
    }

    // -- the live-window re-derivation (L10) --------------------------------

    /// A detect at the window the seeds are already derived at costs nothing.
    #[test]
    fn a_detect_at_the_current_window_asks_for_no_work() {
        assert_eq!(window_plan(Some(34), |_| true, 34), WindowPlan::Ready);
    }

    /// A window nothing is memoised at derives, and says so once.
    #[test]
    fn a_window_never_seen_this_session_derives() {
        assert_eq!(window_plan(Some(34), |_| false, 33), WindowPlan::Derive);
    }

    /// A window the panel oscillates BACK to re-installs from the memo — no
    /// second derivation, and no second log line. The scale is measured per
    /// detect from the observed row pitch, so 33 and 34 can alternate; a plan
    /// that only remembered "already done" would freeze the store at whichever
    /// came first.
    #[test]
    fn a_window_returned_to_reinstalls_without_deriving() {
        assert_eq!(window_plan(Some(33), |w| w == 34, 34), WindowPlan::Reinstall);
    }

    /// A store with no seeds yet is not "ready" at any window.
    #[test]
    fn an_unseeded_store_derives_at_its_first_window() {
        assert_eq!(window_plan(None, |_| false, 34), WindowPlan::Derive);
    }

    /// Put one gem's art in the cache without going through [`store_art`] —
    /// the tests that are not about the reset race must not depend on the
    /// process-wide epoch a concurrent test can move.
    fn write_art(dir: &Path, gem: &str, bytes: &[u8]) {
        std::fs::create_dir_all(art_dir(dir)).expect("art dir");
        std::fs::write(art_path(dir, gem), bytes).expect("write the art");
    }

    /// An icons directory holding the cached art for `gems`.
    fn cached_art_dir(name: &str, gems: &[&str]) -> PathBuf {
        let dir = tmp_dir(name);
        for gem in gems {
            write_art(&dir, gem, &art_bytes(gem));
        }
        dir
    }

    /// A different window yields DIFFERENT signatures — which is the whole
    /// reason the re-derivation exists. The ±3 px alignment search slides the
    /// probe; it does not resample it.
    #[test]
    fn a_new_window_yields_new_signatures() {
        let dir = cached_art_dir("window", &["Fork Support"]);
        let rows = vec![row("Fork", "Fork Support", true)];
        let g = MercGeometry::default();
        // 0.974 is Sebastian's measured panel scale: window 33, not 34.
        assert_ne!(window_px(&g, 1.0), window_px(&g, 0.974));

        let at_one = derive_installable(
            &dir,
            &rows,
            &SeedBlocklist::default(),
            &g,
            1.0,
            &mut BTreeMap::new(),
        );
        let at_live = derive_installable(
            &dir,
            &rows,
            &SeedBlocklist::default(),
            &g,
            0.974,
            &mut BTreeMap::new(),
        );

        assert_eq!(at_one.len(), 1);
        assert_eq!(at_live.len(), 1);
        assert_ne!(
            at_one[0].2.bytes(),
            at_live[0].2.bytes(),
            "the two windows produced the same signature",
        );
    }

    /// A re-derivation does not put back a family the blocklist holds. The art
    /// stays in the cache after an eviction — only a reset removes it — so a
    /// pass that skipped this filter would undo every eviction on the first
    /// scale change.
    #[test]
    fn a_blocklisted_family_is_not_re_derived() {
        let dir = cached_art_dir("blocked-rederive", &["Fork Support", "Multistrike Support"]);
        let rows = plan_rows();
        let blocked = SeedBlocklist {
            families: vec!["Fork".into()],
        };

        let seeds = derive_installable(
            &dir,
            &rows,
            &blocked,
            &MercGeometry::default(),
            0.974,
            &mut BTreeMap::new(),
        );

        assert_eq!(
            seeds.iter().map(|(f, _, _)| f.as_str()).collect::<Vec<_>>(),
            vec!["Multistrike"],
        );
    }

    /// Cached art that does not decode is DELETED, so the next start fetches
    /// it again. Left in place it would read as cached forever and the family
    /// would be neither seeded nor retried.
    #[test]
    fn a_corrupt_cache_file_is_dropped_so_the_next_start_refetches() {
        let dir = cached_art_dir("corrupt-art", &[]);
        std::fs::create_dir_all(art_dir(&dir)).expect("art dir");
        std::fs::write(art_path(&dir, "Fork Support"), b"truncated").expect("write");
        let rows = vec![row("Fork", "Fork Support", true)];

        let seeds = derive_installable(
            &dir,
            &rows,
            &SeedBlocklist::default(),
            &MercGeometry::default(),
            1.0,
            &mut BTreeMap::new(),
        );

        assert!(seeds.is_empty(), "a truncated file derived a signature");
        assert!(
            !art_path(&dir, "Fork Support").exists(),
            "the unreadable file was left to read as cached forever",
        );
    }

    /// A second install of the same family REPLACES its seed. The start-up
    /// pass and a late fetch can both name one family, and a re-derivation
    /// names every family that has one — `install_seed` refuses another
    /// family's art and a full key, but has no opinion about a second seed of
    /// its own family, so the replacement is the caller's rule.
    #[test]
    fn installing_again_replaces_the_seed_rather_than_adding_a_second() {
        let dir = cached_art_dir("reinstall", &["Fork Support"]);
        let rows = vec![row("Fork", "Fork Support", true)];
        let g = MercGeometry::default();
        let t = Thresholds::default();
        let first =
            derive_installable(&dir, &rows, &SeedBlocklist::default(), &g, 1.0, &mut BTreeMap::new());
        let second =
            derive_installable(&dir, &rows, &SeedBlocklist::default(), &g, 0.974, &mut BTreeMap::new());
        assert_ne!(first[0].2.bytes(), second[0].2.bytes(), "the two passes agreed");
        let mut store = TemplateStore::new();

        let open = SeedBlocklist::default();
        assert_eq!(install_all(&mut store, &first, &open, &t).installed, 1);
        assert_eq!(install_all(&mut store, &second, &open, &t).installed, 1);

        assert_eq!(store.seeded_families(), ["Fork"]);
        assert_eq!(store.len(), 1, "the re-derivation added a second sample");
    }

    /// A seed yields to a stored sample of another family, and the tally says
    /// so rather than reporting an install that did not happen.
    #[test]
    fn a_seed_that_yields_is_counted_as_yielded() {
        let dir = cached_art_dir("yield", &["Fork Support"]);
        let rows = vec![row("Fork", "Fork Support", true)];
        let g = MercGeometry::default();
        let t = Thresholds::default();
        let seeds = derive_installable(&dir, &rows, &SeedBlocklist::default(), &g, 1.0, &mut BTreeMap::new());
        let mut store = TemplateStore::new();
        // The same art, already confirmed under ANOTHER family.
        store.learn("Chain", 1, seeds[0].2.clone(), None, &t);

        let tally = install_all(&mut store, &seeds, &SeedBlocklist::default(), &t);

        assert_eq!(
            tally,
            InstallTally { installed: 0, yielded: 1, key_full: 0, blocked: 0 },
        );
        assert!(store.seeded_families().is_empty());
    }

    /// A signature already in the memo is REUSED, not re-derived. The proof is
    /// an art directory with nothing in it: a pass that ignored the memo would
    /// find no file and return nothing.
    #[test]
    fn a_memoised_signature_is_reused_without_reading_the_art() {
        let dir = tmp_dir("memo-hit");
        let rows = vec![row("Fork", "Fork Support", true)];
        // Deliberately ANOTHER family's art, so the assertion cannot pass by
        // the memo and the derivation agreeing.
        let sentinel =
            derive(&art("Multistrike Support"), &MercGeometry::default(), 1.0).expect("derive");
        let mut memo = BTreeMap::new();
        memo.insert("Fork".to_string(), sentinel.clone());

        let seeds = derive_installable(
            &dir,
            &rows,
            &SeedBlocklist::default(),
            &MercGeometry::default(),
            1.0,
            &mut memo,
        );

        assert_eq!(seeds.len(), 1, "the memo was ignored and the missing art won");
        assert_eq!(seeds[0].2.bytes(), sentinel.bytes());
    }

    /// A ✕ pressed between the derivation and the install is honoured. Both
    /// forget doors write the blocklist while holding the store mutex, and the
    /// install re-reads it there, so the gap the derivation opens is closed.
    #[test]
    fn a_family_blocked_after_the_derivation_is_not_installed() {
        let dir = cached_art_dir("blocked-midpass", &["Fork Support"]);
        let rows = vec![row("Fork", "Fork Support", true)];
        let g = MercGeometry::default();
        let t = Thresholds::default();
        let seeds =
            derive_installable(&dir, &rows, &SeedBlocklist::default(), &g, 1.0, &mut BTreeMap::new());
        assert_eq!(seeds.len(), 1, "the derivation had nothing to install");
        // The click lands here.
        let blocked = SeedBlocklist {
            families: vec!["Fork".into()],
        };
        let mut store = TemplateStore::new();

        let tally = install_all(&mut store, &seeds, &blocked, &t);

        assert_eq!(
            tally,
            InstallTally { installed: 0, yielded: 0, key_full: 0, blocked: 1 },
        );
        assert!(store.seeded_families().is_empty());
    }

    /// One fetch pass at a time: a module toggled repeatedly does not issue a
    /// second set of requests over the first.
    #[test]
    fn a_second_fetch_is_refused_while_one_is_running() {
        let mut state = SeedState::default();

        assert!(claim_fetch(&mut state), "the first pass was refused");
        assert!(!claim_fetch(&mut state), "a second pass claimed over the first");
    }

    /// And the claim is given back, or every later session's seeding is
    /// silently dead for the rest of the process.
    #[test]
    fn a_released_claim_lets_the_next_session_fetch() {
        let mut state = SeedState::default();
        claim_fetch(&mut state);
        release_fetch(&mut state);

        assert!(claim_fetch(&mut state));
    }

    /// A claim re-arms the seam: the new pass has not looked at the cache yet,
    /// so `wait_for_install` must wait for it rather than read the last pass's
    /// answer.
    #[test]
    fn a_claim_re_arms_the_install_seam() {
        let mut state = SeedState::default();
        release_fetch(&mut state);
        assert!(state.cache_scanned);

        claim_fetch(&mut state);

        assert!(!state.cache_scanned);
    }

    /// A reset throws out the DERIVED signatures, not just the files. Left in
    /// memory they re-install every family from a memo that never reads the
    /// deleted art — the store fills back up while the page's seed group stays
    /// empty, so there is no chip left to press.
    #[test]
    fn a_reset_drops_the_memoised_signatures() {
        let _serial = session_guard();
        let dir = tmp_dir("forget-session");
        let rows = vec![row("Fork", "Fork Support", true)];
        let g = MercGeometry::default();
        let window = window_px(&g, 1.0);
        // A session that has already seeded: art derived, memo warm.
        let warm = derive(&art("Fork Support"), &g, 1.0).expect("derive");
        with_seed_state(|s| {
            s.window = Some(window);
            s.derived
                .insert(window, BTreeMap::from([("Fork".to_string(), warm)]));
            s.install = Some(InstallContext {
                icons_dir: dir.clone(),
                geometry: g.clone(),
                scale: 1.0,
            });
        });

        forget_session();

        // `dir` holds no art — the reset deleted it — so the ONLY way a
        // signature comes back is the memo.
        let (window_verdict, install, mut memo) = with_seed_state(|s| {
            (
                window_plan(s.window, |w| s.derived.contains_key(&w), window),
                s.install.clone(),
                s.derived.get(&window).cloned().unwrap_or_default(),
            )
        });
        assert_eq!(window_verdict, WindowPlan::Derive);
        assert!(install.is_none(), "the install seam survived the reset");
        let seeds =
            derive_installable(&dir, &rows, &SeedBlocklist::default(), &g, 1.0, &mut memo);
        assert!(seeds.is_empty(), "a signature came back from the memo after a reset");
    }

}
