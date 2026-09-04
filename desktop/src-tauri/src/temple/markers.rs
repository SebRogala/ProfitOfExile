//! The side panel's door markers (POE-169).
//!
//! The diamond next to the room name is **not a diagram** — Sebastian: *"that
//! diamond is just a bit rotated room"*. It is an isometric view of the room
//! the player is standing in, and the coloured seals on its walls are that
//! room's corridors: **green = open, red = closed**, one seal per lattice
//! neighbour. That makes it authoritative where beam sampling is not: the
//! current room's ornate gold selection frame overhangs ~15 px into a 21 px
//! plate gap and *completely covers* its own diagonal corridor midpoints,
//! which is why POE-168's reader hands those edges back in
//! [`super::reader::TempleLayout::uncertain`] instead of guessing.
//!
//! Pixels only, no OCR, no Tauri — the whole module runs in the Linux test
//! container against committed crops.
//!
//! # The position → neighbour mapping is DERIVED, not assumed
//!
//! A seal's direction from the diamond's centre is a **linear** function of
//! its neighbour's direction on the board. Fitted on the reference board and
//! then checked against every other measured board:
//!
//! | board | slot | degree |
//! |---|---|---|
//! | `2026-08-02_22-22-38` | D3 | 3 |
//! | `2026-08-03_22-54-58` | B0 | 4 |
//! | `2026-08-02_16-41-11` | B1 | 4 |
//! | `2026-08-03_11-42-02` | C0 | 4 |
//! | `2026-08-03_11-58-28` | D1 | 6 |
//! | `2026-08-03_22-43-37` | B1 | 4 |
//! | `2026-08-07_19-28-36` | B0 | 4 |
//! | `2026-08-07_19-01-13` | D3 | 3 |
//!
//! 8 boards, 32 seals, 5 slot shapes, 2 UI scales: **worst angular residual
//! 0.32°** against a per-board least-squares centre. End to end — the shipped
//! detector, measuring from the *rect's* centre rather than a fitted one — the
//! five committed crops land within **1.28°**. The corridor directions are
//! **55.7° apart at their closest**, so that residual has well over an order
//! of magnitude of headroom. The colours the mapping produces agree
//! with the hand-encoded door sets of TEMPLE-CORE-RULES cases 1, 3, 4, 5, 6
//! and 7 — including Case 3 (`Chasm` B0, open → Breach Containment Chamber at
//! C1) and Case 4 (`Chasm` B1, open → Apex), which is the independent
//! confirmation: those two boards put the green seal in *different* positions,
//! and the model predicts both.

// POE-171 is that caller: `temple::run` and `temple::slice` reach this module
// on every read, so the file-level `#![allow(dead_code)]` is gone. What is
// still uncalled carries its own attribute, which is now the inventory of what
// only the tests reach rather than a blanket over the whole file.

use std::collections::BTreeSet;

use image::DynamicImage;

use super::lattice::{self, Edge, Lattice, Slot};
use super::reader::TempleLayout;

// ------------------------------------------------------------ ink gates --

/// Saturation a pixel needs to count as marker ink.
///
/// Measured over 25×25 probes at 10 seals on 6 boards: seal ink runs
/// 0.51–1.00.
pub const INK_SAT: f32 = 0.50;

/// Value (brightness) a pixel needs to count as marker ink.
///
/// **This is the gate that separates the seals from everything else in the
/// panel**, of which there are two kinds:
///
/// - the two small room icons drawn *inside* the diamond, which are red and
///   green too and at saturation alone are indistinguishable — but top out at
///   value **0.48**;
/// - the faint red schematic linework behind the panel, which survives a 0.55
///   gate in patches of 26–75 px.
///
/// Every seal core reaches 0.88–0.98. Swept over 0.55/0.62/0.68/0.72/0.76/0.80
/// on the five committed crops: at **0.72** each seal keeps 13–59 px in a
/// 17–23 px tall blob while every surviving background patch is 9 px tall or
/// less, which is the separation [`MIN_BLOB_HEIGHT`] then acts on. Below 0.72
/// the linework merges into the seals; above 0.76 the dimmest green seal is
/// down to 8 px and the read stops surviving a smaller UI scale.
pub const INK_VALUE: f32 = 0.72;

/// Hue half-width, in degrees, of the red band around 0°. Measured seal ink:
/// −24°..+25°.
pub const RED_HUE: f32 = 30.0;
/// Hue band of the green seals, degrees. Measured seal ink: 80°..120°; the
/// upper bound is loose because nothing else in the panel is cyan.
pub const GREEN_HUE: (f32, f32) = (80.0, 175.0);

/// Minimum ink a blob needs, as a fraction of the diamond rect's area. A
/// noise floor only — the green seal, the scarcest of the two because its
/// ring is thinner, keeps 13 px of a 240×200 rect (0.00027) at
/// [`INK_VALUE`].
pub const MIN_INK_FRACTION: f64 = 0.000_16;

/// Minimum blob height, as a fraction of the rect's shorter side.
///
/// **The discriminator.** A seal is a disc; the panel's background is thin,
/// largely horizontal schematic linework, so what survives [`INK_VALUE`] there
/// is wide and flat. Measured over the five committed crops, after the
/// same-colour merge and at [`INK_VALUE`]:
///
/// | population | height |
/// |---|---|
/// | seals | 11–23 px (11 is the dim green one on `diamond-ref-d1-1376.png`; 17–20 is typical; 23 on the 272×220 live crop) |
/// | background blobs | ≤ 9 px (tallest: 9 px on the live crop, 8 px on `diamond-ref-d3-1374.png`) |
///
/// The two populations are therefore **2 px apart**, and the threshold has to
/// land inside that gap on both crop sizes at once — it is a fraction of the
/// shorter side, which is 200 px on four of the crops and 220 px on the fifth.
/// 0.048 gives 10 px and 11 px respectively, both between the populations.
///
/// Swept end to end over the five fixtures at
/// 0.030/0.035/0.038/0.040/0.048/0.055/0.057/0.060: **[0.040, 0.057] passes**,
/// and the edges fail for the two opposite reasons — 0.038 is 8 px on the
/// live crop and admits its 8 px background blob, 0.060 is 12 px on the
/// 240×200 crops and drops the 11 px green seal. 0.048 is the middle of that
/// window. The previous 0.055 was 11 px on the 240×200 crops — exactly the
/// height of the smallest seal, i.e. sitting *on* a population rather than
/// between them.
pub const MIN_BLOB_HEIGHT: f64 = 0.048;

/// Largest bounding-box side a blob may have, as a fraction of the rect's
/// shorter side. Seals measure 11–25 px across in a 200 px unit and 17–24 in a
/// 220 px one, i.e. up to 0.125.
pub const MAX_BLOB_SIDE: f64 = 0.16;

// ------------------------------------------------- the projection model --

/// Screen direction of the board's `+x` (one half-column right), unit length.
///
/// Fitted to −41.70°: see the module note. `+y` is down, matching image
/// coordinates.
pub const AXIS_X: (f64, f64) = (0.746_70, -0.665_30);
/// Screen direction of the board's `+y` (one row down), **not** unit length —
/// the isometric projection stretches this axis by 1.88 relative to `+x`, and
/// that ratio is what makes the six directions land where they do.
pub const AXIS_Y: (f64, f64) = (1.410_81, 1.249_54);

/// Largest angular error, in degrees, an accepted marker→neighbour assignment
/// may carry.
///
/// The modelled directions are 55.7° apart at their closest, so anything under
/// 27.8° is unambiguous everywhere on the board. 22° keeps a margin and is
/// still ~17× the 1.28° the committed crops actually produce. Its real job is to reject a
/// *wrong diamond rect*: a centre far enough off to rotate the whole fan trips
/// this rather than producing a confident, wrong door set.
pub const MAX_RESIDUAL_DEG: f64 = 22.0;

/// `to`'s offset from `from` in **half-columns and ROW INDICES**, which is the
/// space [`AXIS_X`] and [`AXIS_Y`] project out of.
///
/// That the row half is an INDEX and not a pixel distance is measured, not
/// tidiness: the Entrance plate is drawn 19 px lower than its two row-E
/// siblings, and feeding that drop through the projection moves the predicted
/// seal for a D1→E1 corridor by 3.4° — away from where the seal on
/// `2026-08-03_11-58-28` actually sits. The diamond is a picture of the *room*,
/// so its doors follow the board's logical directions and ignore the layout
/// panel's drawing quirk.
///
/// The column half is read off the lattice rather than off a table so there is
/// one answer to where a plate is; the capture scale divides straight back out.
pub fn board_offset(lattice: &Lattice, from: Slot, to: Slot) -> (f64, f64) {
    let (fx, _) = lattice.centre(from);
    let (tx, _) = lattice.centre(to);
    let bx = (tx - fx) as f64 / (lattice::COL_PITCH / 2.0 * lattice.scale as f64);
    let by = row_index(to) - row_index(from);
    (bx, by)
}

/// A board offset through the isometric projection. Screen px units of the
/// panel's own drawing, `+y` down; not normalised.
fn project(offset: (f64, f64)) -> (f64, f64) {
    let (bx, by) = offset;
    (
        AXIS_X.0 * bx + AXIS_Y.0 * by,
        AXIS_X.1 * bx + AXIS_Y.1 * by,
    )
}

/// Which way `to`'s seal sits from the diamond's centre, as a unit vector.
///
/// The capture scale cancels in the normalisation, so this is the same vector
/// at any UI scale. See [`board_offset`] for the space it measures in.
pub fn neighbour_direction(lattice: &Lattice, from: Slot, to: Slot) -> (f64, f64) {
    let (x, y) = project(board_offset(lattice, from, to));
    let norm = (x * x + y * y).sqrt();
    (x / norm, y / norm)
}

/// The seal ring's radius, as a fraction of the diamond rect's **shorter side**.
///
/// The seals are drawn at one distance from the diamond's centre, not at a
/// distance that depends on which wall they are on. MEASURED through the
/// shipped detector on all five committed crops — 21 seals, 5 slot shapes,
/// 2 crop sizes:
///
/// | crop | short side | detected radii | ratio |
/// |---|---|---|---|
/// | `diamond-ref-d3-1374.png` | 200 | 72.1 – 79.1 | 1.10 |
/// | `diamond-ref-b0-1358.png` | 200 | 67.7 – 81.3 | 1.20 |
/// | `diamond-ref-b1-1352.png` | 200 | 71.2 – 82.5 | 1.16 |
/// | `diamond-ref-d1-1376.png` | 200 | 67.7 – 82.5 | 1.22 |
/// | `diamond-live-b0-1539.png` | 220 | 75.3 – 92.9 | 1.23 |
///
/// Swept 0.360 … 0.400 in 0.002 steps against the POSITIONAL residual
/// `|predicted − detected|`. **0.382 is the min-max optimum: rms 5.44 px, max
/// 8.94 px** (0.380 is the rms optimum at 5.43/9.38, and the max is what the
/// acceptance test spends, so the min-max point is the one taken).
/// `the_committed_crops_land_within_ten_px_of_the_seal_ring` is that test.
///
/// # What this replaced, and why the first model was wrong
///
/// POE-244 first placed a seal on the parallelogram spanned by [`AXIS_X`] and
/// [`AXIS_Y`], which puts a same-row corridor at HALF the radius of a diagonal
/// one — a 2.24 : 1 spread. The panel does not draw them that way: the six
/// radii on `diamond-ref-d1-1376.png` run 67.7 … 82.5 px, a spread of **1.22**.
/// Fitted end to end that model is rms 22.2 px, max 44.5 px — four times worse
/// than one constant. The mistake was reading [`AXIS_X`] / [`AXIS_Y`] as a
/// description of the SHAPE when they are a fit of DIRECTIONS only, which is
/// all [`neighbour_direction`] ever claimed and all the door reader ever used.
///
/// Only the test reaches this: nothing that SHIPS converts a seal to screen
/// pixels, because the widget draws the diamond in its own unit space where the
/// ring is 1 by construction. It is here because it is the measurement
/// [`DIAMOND_HALF_W`] and [`neighbour_direction`]'s unit length are both
/// expressed against — a number the code needs to be checkable against the
/// panel, per this module's per-item allow convention.
#[allow(dead_code)]
pub const SEAL_RING_FRACTION: f64 = 0.382;

/// Half-width of the room's outline, in units of the seal ring's radius.
///
/// The rhombus `|x| / A + |y| / B = 1` least-squares-fitted to the same 21
/// detected seals gives `a = 0.5565` and `b = 0.4407` of the shorter side —
/// `1.457` and `1.154` ring radii, an aspect of **1.263**, which is the
/// isometric squash the game draws the room at.
///
/// The seals are NOT on this outline by construction, and saying so is the
/// point: a ring fits the measurements better than the rhombus does
/// (rms 5.44 / max 8.94 against rms 5.56 / max 11.41), so the ring is what
/// places them and this is what draws the walls. At the six angles a corridor
/// can actually take, the outline's own radius is 1.125, 0.959 and 0.918 ring
/// radii — **averaging 1.000** — so the seals sit on the walls to the eye
/// while each one stays where the panel really puts it.
pub const DIAMOND_HALF_W: f64 = 1.457;
/// Half-height of the room's outline — see [`DIAMOND_HALF_W`].
pub const DIAMOND_HALF_H: f64 = 1.154;

/// The room's isometric outline, as four corners in ring order, `+y` down and
/// the centre at the origin, in units of the seal ring's radius.
///
/// Ring order is by screen angle — right, bottom, left, top — so a consumer can
/// draw it as a polygon without sorting. A seal goes at
/// [`neighbour_direction`], which is a unit vector, so it lands on the ring of
/// radius 1 inside this shape; see [`DIAMOND_HALF_W`] for how the two relate.
pub fn diamond_corners() -> [(f64, f64); 4] {
    [
        (DIAMOND_HALF_W, 0.0),
        (0.0, DIAMOND_HALF_H),
        (-DIAMOND_HALF_W, 0.0),
        (0.0, -DIAMOND_HALF_H),
    ]
}

/// Row of a slot as an index, `A` = 0 … `E` = 4.
fn row_index(slot: Slot) -> f64 {
    (slot.as_str().as_bytes()[0] - b'A') as f64
}

// ---------------------------------------------------------------- types --

/// One door marker on the diamond.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Marker {
    /// Blob centroid in the coordinates of the image passed to
    /// [`read_door_markers`].
    pub position: (i32, i32),
    /// Green seal. `false` is a red one — there is no third colour.
    pub open: bool,
}

/// What one diamond read yields.
#[derive(Debug, Clone, PartialEq)]
pub struct DoorMarkers {
    /// The diamond rect's centre, which the projection model measures angles
    /// from. Carried alongside the markers so the caller cannot pair a marker
    /// set with the wrong origin.
    pub centre: (i32, i32),
    /// The seals, in scan order.
    pub markers: Vec<Marker>,
}

/// Why a diamond could not be turned into door state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkerError {
    /// The rect is empty or falls outside the capture.
    RectOutsideImage,
    /// The seal count does not equal the slot's lattice degree. Never
    /// papered over: 3 seals on a 4-neighbour room means one was missed, and
    /// which one is unknowable.
    CountMismatch { found: usize, expected: usize },
    /// A seal sat further than [`MAX_RESIDUAL_DEG`] from every modelled
    /// direction — almost always a wrong diamond rect.
    Unmappable { worst_deg: f64 },
    /// The layout was read between rooms, so there is no current room for the
    /// markers to describe.
    NoCurrentRoom,
    /// A corridor the beam sampler read **confidently** — one it did not flag
    /// uncertain — disagrees with its seal. `beam_open` is what the sampler
    /// said. See [`apply_markers`].
    BeamDisagreement { edge: Edge, beam_open: bool },
}

impl std::fmt::Display for MarkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarkerError::RectOutsideImage => {
                write!(f, "the diamond rect is not inside the capture")
            }
            MarkerError::CountMismatch { found, expected } => write!(
                f,
                "read {found} door markers for a {expected}-neighbour room"
            ),
            MarkerError::Unmappable { worst_deg } => write!(
                f,
                "a door marker sat {worst_deg:.1}° from every corridor direction"
            ),
            MarkerError::NoCurrentRoom => write!(f, "the board has no current room"),
            MarkerError::BeamDisagreement { edge, beam_open } => write!(
                f,
                "corridor {edge} reads {} from the panel but {} from its seal",
                if *beam_open { "open" } else { "closed" },
                if *beam_open { "closed" } else { "open" }
            ),
        }
    }
}

impl std::error::Error for MarkerError {}

// ------------------------------------------------------------- the read --

/// Read the diamond's door markers.
///
/// `diamond` is `[x, y, w, h]` of the diamond graphic in `img`; its centre is
/// what the projection model measures from. `degree` is the current slot's
/// lattice degree — 3, 4 or 6 — and the read fails rather than returning a
/// partial fan if the seal count disagrees.
pub fn read_door_markers(
    img: &DynamicImage,
    diamond: [i32; 4],
    degree: usize,
) -> Result<DoorMarkers, MarkerError> {
    let [x0, y0, w, h] = diamond;
    if w <= 0 || h <= 0 || x0 < 0 || y0 < 0 {
        return Err(MarkerError::RectOutsideImage);
    }
    let (x1, y1) = (x0 + w, y0 + h);
    if x1 as u32 > img.width() || y1 as u32 > img.height() {
        return Err(MarkerError::RectOutsideImage);
    }

    let rgb = img.to_rgb8();
    let unit = w.min(h) as f64;
    let min_ink = ((w * h) as f64 * MIN_INK_FRACTION).max(6.0) as usize;
    let merge = ((w as f64 / 16.0).round() as i32).max(6);
    let min_height = (unit * MIN_BLOB_HEIGHT).round() as i32;
    let max_side = (unit * MAX_BLOB_SIDE).round() as i32;

    // Connected components over the ink mask, 8-connected. A seal is drawn as
    // a RING, so a row-major "is this pixel near a running centroid" pass
    // splits it into two arcs; flood fill does not.
    let (w_u, h_u) = (w as usize, h as usize);
    let mut mask: Vec<Option<bool>> = Vec::with_capacity(w_u * h_u);
    for y in y0..y1 {
        for x in x0..x1 {
            mask.push(classify(rgb.get_pixel(x as u32, y as u32).0));
        }
    }
    let mut seen = vec![false; w_u * h_u];
    let mut blobs: Vec<Blob> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..mask.len() {
        let Some(open) = mask[start] else { continue };
        if seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut blob = Blob::new((start % w_u) as i32 + x0, (start / w_u) as i32 + y0, open);
        blob.n = 0;
        blob.sx = 0;
        blob.sy = 0;
        while let Some(i) = stack.pop() {
            let (px, py) = ((i % w_u) as i32, (i / w_u) as i32);
            blob.add(px + x0, py + y0);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (px + dx, py + dy);
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        continue;
                    }
                    let j = ny as usize * w_u + nx as usize;
                    if !seen[j] && mask[j] == Some(open) {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        blobs.push(blob);
    }

    // A ring whose anti-aliased edge breaks the 8-connection still has to come
    // back as one seal, so same-coloured components closer than `merge` are
    // folded together.
    let mut merged: Vec<Blob> = Vec::new();
    blobs.sort_by(|a, b| b.n.cmp(&a.n));
    for blob in blobs {
        match merged.iter_mut().find(|m| {
            m.open == blob.open && (m.cx - blob.cx).abs() < merge && (m.cy - blob.cy).abs() < merge
        }) {
            Some(host) => host.absorb(&blob),
            None => merged.push(blob),
        }
    }
    let blobs = merged;

    let mut markers: Vec<Marker> = blobs
        .into_iter()
        .filter(|b| {
            let (bw, bh) = (b.x1 - b.x0 + 1, b.y1 - b.y0 + 1);
            b.n >= min_ink && bh >= min_height && bw <= max_side && bh <= max_side
        })
        .map(|b| Marker {
            position: ((b.sx / b.n as i64) as i32, (b.sy / b.n as i64) as i32),
            open: b.open,
        })
        .collect();
    markers.sort_by_key(|m| (m.position.0, m.position.1));

    if markers.len() != degree {
        return Err(MarkerError::CountMismatch {
            found: markers.len(),
            expected: degree,
        });
    }
    Ok(DoorMarkers {
        centre: (x0 + w / 2, y0 + h / 2),
        markers,
    })
}

/// Hue/saturation/value classification of one pixel: `Some(true)` green,
/// `Some(false)` red, `None` not marker ink.
fn classify(px: [u8; 3]) -> Option<bool> {
    let (r, g, b) = (px[0] as f32, px[1] as f32, px[2] as f32);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    if mx <= 0.0 {
        return None;
    }
    let value = mx / 255.0;
    let sat = (mx - mn) / mx;
    if sat < INK_SAT || value < INK_VALUE {
        return None;
    }
    let d = mx - mn;
    let hue = if d <= 0.0 {
        0.0
    } else if mx == r {
        (60.0 * ((g - b) / d) + 360.0) % 360.0
    } else if mx == g {
        60.0 * ((b - r) / d) + 120.0
    } else {
        60.0 * ((r - g) / d) + 240.0
    };
    if hue <= RED_HUE || hue >= 360.0 - RED_HUE {
        Some(false)
    } else if (GREEN_HUE.0..=GREEN_HUE.1).contains(&hue) {
        Some(true)
    } else {
        None
    }
}

/// A growing run of same-coloured ink.
struct Blob {
    n: usize,
    sx: i64,
    sy: i64,
    cx: i32,
    cy: i32,
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
    open: bool,
}

impl Blob {
    fn new(x: i32, y: i32, open: bool) -> Blob {
        Blob {
            n: 1,
            sx: x as i64,
            sy: y as i64,
            cx: x,
            cy: y,
            x0: x,
            x1: x,
            y0: y,
            y1: y,
            open,
        }
    }

    fn absorb(&mut self, other: &Blob) {
        self.n += other.n;
        self.sx += other.sx;
        self.sy += other.sy;
        self.cx = (self.sx / self.n as i64) as i32;
        self.cy = (self.sy / self.n as i64) as i32;
        self.x0 = self.x0.min(other.x0);
        self.x1 = self.x1.max(other.x1);
        self.y0 = self.y0.min(other.y0);
        self.y1 = self.y1.max(other.y1);
    }

    fn add(&mut self, x: i32, y: i32) {
        self.n += 1;
        self.sx += x as i64;
        self.sy += y as i64;
        self.cx = (self.sx / self.n as i64) as i32;
        self.cy = (self.sy / self.n as i64) as i32;
        self.x0 = self.x0.min(x);
        self.x1 = self.x1.max(x);
        self.y0 = self.y0.min(y);
        self.y1 = self.y1.max(y);
    }
}

// ------------------------------------------------------------ the mapping --

/// Pair each seal with the neighbour it belongs to.
///
/// Both fans are sorted by angle and matched in **cyclic order**, then the
/// best of the `degree` possible rotations is taken. Nearest-angle matching
/// would do on a correctly placed rect (worst residual measured: 0.32°); the
/// cyclic form is what keeps a slightly-off centre — which rotates every seal
/// the same way — from producing a mixed, half-wrong assignment.
pub fn assign_markers(
    lattice: &Lattice,
    current: Slot,
    markers: &DoorMarkers,
) -> Result<Vec<(Slot, Marker)>, MarkerError> {
    let neighbours = lattice::neighbours(current);
    if markers.markers.len() != neighbours.len() {
        return Err(MarkerError::CountMismatch {
            found: markers.markers.len(),
            expected: neighbours.len(),
        });
    }

    let mut modelled: Vec<(f64, Slot)> = neighbours
        .into_iter()
        .map(|slot| {
            let (dx, dy) = neighbour_direction(lattice, current, slot);
            (dy.atan2(dx).to_degrees(), slot)
        })
        .collect();
    modelled.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("angles are finite"));

    let mut seen: Vec<(f64, Marker)> = markers
        .markers
        .iter()
        .map(|m| {
            let dx = (m.position.0 - markers.centre.0) as f64;
            let dy = (m.position.1 - markers.centre.1) as f64;
            (dy.atan2(dx).to_degrees(), *m)
        })
        .collect();
    seen.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("angles are finite"));

    let n = modelled.len();
    let mut best: Option<(f64, usize)> = None;
    for shift in 0..n {
        let worst = (0..n)
            .map(|i| angular_delta(seen[(i + shift) % n].0, modelled[i].0).abs())
            .fold(0.0_f64, f64::max);
        if best.is_none_or(|(b, _)| worst < b) {
            best = Some((worst, shift));
        }
    }
    // `n` is the slot's lattice degree, which is 3, 4 or 6 — never 0 — so the
    // rotation search above always ran at least once.
    let (worst, shift) = best.expect("every slot has neighbours, so a rotation was scored");
    if worst > MAX_RESIDUAL_DEG {
        return Err(MarkerError::Unmappable { worst_deg: worst });
    }
    Ok((0..n)
        .map(|i| (modelled[i].1, seen[(i + shift) % n].1))
        .collect())
}

/// Signed difference between two bearings, wrapped to (−180, 180].
fn angular_delta(a: f64, b: f64) -> f64 {
    let mut d = (a - b) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d <= -180.0 {
        d += 360.0;
    }
    d
}

/// Settle a board's corridors with the diamond's seals.
///
/// Returns **the whole door set** the caller should go on to use: every
/// corridor [`super::reader::read_layout`] judged open that does not touch the
/// current room, plus the current room's corridors exactly as its seals give
/// them. The seals overrule the beam sampler on that room — the selection
/// frame covers its diagonal corridor midpoints outright — so those edges are
/// replaced, never merged.
///
/// # Why this returns the settled set and not the open edges
///
/// An "open edges touching the current room" result reads as something to add
/// to what the reader already had, and `layout.doors ∪ resolved` is the
/// obvious way to add it. That union is wrong in exactly the case this
/// function exists for: it puts every beam false positive on the current room
/// straight back, which on the reference board is `C2-D3` — a corridor the
/// sampler reads as gold through the selection frame and the seals say is
/// shut. Handing back the settled set makes that mistake unrepresentable.
///
/// # Errors
///
/// [`MarkerError::BeamDisagreement`] when the sampler read an incident
/// corridor **confidently** — not in
/// [`super::reader::TempleLayout::uncertain`] — and its seal says the
/// opposite. The seals' licence is over the edges the frame hides; a clash on
/// a clean read means one of the two reads is of the wrong board, and quietly
/// preferring either would hand POE-170 a confident, wrong graph.
///
/// **This cannot fire on a layout from [`super::reader::read_layout`] today**:
/// [`super::doors::read_doors`] puts *every* edge incident to the current room
/// into `uncertain`, unconditionally, so the check above skips all of them and
/// the seals always win. It is kept as defence against a future narrowing of
/// `uncertain` — the moment the sampler starts trusting some incident corridor
/// (a tighter frame model, a sampling offset that clears the selection ring),
/// the clash becomes reachable and must be an error rather than a silent
/// preference. Its test synthesises the narrowed layout directly.
pub fn apply_markers(
    layout: &TempleLayout,
    markers: &DoorMarkers,
) -> Result<BTreeSet<Edge>, MarkerError> {
    let current = layout.current.ok_or(MarkerError::NoCurrentRoom)?;
    let lattice = Lattice::new(layout.origin, layout.scale);
    let sealed: BTreeSet<Edge> = assign_markers(&lattice, current, markers)?
        .into_iter()
        .filter(|(_, marker)| marker.open)
        .map(|(slot, _)| Edge::new(current, slot))
        .collect();

    for slot in lattice::neighbours(current) {
        let edge = Edge::new(current, slot);
        if layout.uncertain.contains(&edge) {
            continue;
        }
        let beam_open = layout.doors.contains(&edge);
        if beam_open != sealed.contains(&edge) {
            return Err(MarkerError::BeamDisagreement { edge, beam_open });
        }
    }

    let mut settled: BTreeSet<Edge> = layout
        .doors
        .iter()
        .copied()
        .filter(|edge| !edge.touches(current))
        .collect();
    settled.extend(sealed);
    Ok(settled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    /// A committed diamond crop, plus the board state
    /// `TEMPLE-CORE-RULES.md` records for that exact screenshot.
    ///
    /// The crops are cut **centred on the diamond**, so the whole image is the
    /// `diamond` rect and its centre is the fitted centre to within 0.5 px;
    /// each is a lossless RGB cut of its source, deliberately *not*
    /// requantised, because every gate in this module is a hue/value gate.
    struct Fixture {
        file: &'static str,
        /// Source screenshot and the crop box `(x0, y0, x1, y1)` taken from it.
        source: &'static str,
        crop: (u32, u32, u32, u32),
        size: (u32, u32),
        /// The room the panel belongs to.
        current: Slot,
        /// Neighbours whose seal is GREEN, by slot key. Everything else in
        /// `lattice::neighbours(current)` is closed.
        open: &'static [&'static str],
        /// Where the case is written down.
        case: &'static str,
    }

    /// Case 1 — Tombs (D3), 3 doors, all closed, fully isolated.
    const TOMBS: Fixture = Fixture {
        file: "diamond-ref-d3-1374.png",
        source: "tmp/alva-screenshots/2026-08-02_22-22-38.png",
        crop: (1006, 86, 1246, 286),
        size: (240, 200),
        current: Slot::D3,
        open: &[],
        case: "TEMPLE-CORE-RULES §5 Case 1",
    };

    /// Case 3 — Chasm (B0), one door open (→ Breach Containment Chamber at
    /// C1), three closed (Apex A0, Locus B1, Hall of Mettle C0).
    const CHASM_B0: Fixture = Fixture {
        file: "diamond-ref-b0-1358.png",
        source: "tmp/alva-screenshots/2026-08-03_22-54-58.png",
        crop: (1002, 83, 1242, 283),
        size: (240, 200),
        current: Slot::B0,
        open: &["C1"],
        case: "TEMPLE-CORE-RULES §5 Case 3",
    };

    /// Case 4 — Chasm (B1), one door open (→ Apex), three closed. The
    /// counterpart to Case 3: same degree, same colours, a *different* seal is
    /// green.
    const CHASM_B1: Fixture = Fixture {
        file: "diamond-ref-b1-1352.png",
        source: "tmp/alva-screenshots/2026-08-02_16-41-11.png",
        crop: (1008, 60, 1248, 260),
        size: (240, 200),
        current: Slot::B1,
        open: &["A0"],
        case: "TEMPLE-CORE-RULES §5 Case 4",
    };

    /// Case 6 — Cloister (D1), the six-neighbour shape. Its component is
    /// `{Cloister, Antechamber, Omnitect Forge}`, so exactly two doors are
    /// open; Cellar (C1) is the one the advisor recommended opening, so it
    /// must read closed.
    const CLOISTER: Fixture = Fixture {
        file: "diamond-ref-d1-1376.png",
        source: "tmp/alva-screenshots/2026-08-03_11-58-28.png",
        crop: (1006, 76, 1246, 276),
        size: (240, 200),
        current: Slot::D1,
        open: &["C0", "D0"],
        case: "TEMPLE-CORE-RULES §6 Case 6",
    };

    /// The live-scale board: 1539 px window, scale 1.13. Same slot shape as
    /// Case 3 with the green seal in a different place, which is what pins the
    /// model rather than a per-board fudge.
    const LIVE_B0: Fixture = Fixture {
        file: "diamond-live-b0-1539.png",
        source: "Screenshots/2026-08-07_19-28-36.png",
        crop: (1113, 108, 1385, 328),
        size: (272, 220),
        current: Slot::B0,
        open: &["C0"],
        case: "TEMPLE-CORE-RULES §6b, the anchor-bug board",
    };

    const ALL: [&Fixture; 5] = [&TOMBS, &CHASM_B0, &CHASM_B1, &CLOISTER, &LIVE_B0];

    // All five crops are 240x200 or 272x220, the sizes they were hand-cut at.
    // Production has not been that size since POE-230 sized `run::DIAMOND_W_REF`
    // against the anchor rather than against the capture's edge: it is 200x200
    // ref px. Every threshold in this file was therefore swept at FIXTURE sizes,
    // and `run`'s `screen-live-1920x1080.png` is the one witness at the size the
    // loop actually crops. What moves with the size is `merge` (`w / 16`),
    // `min_ink`, `min_height` and `max_side` — all four are fractions of the
    // rect, so a re-cut of these crops is what would retire the gap.

    fn load(f: &Fixture) -> DynamicImage {
        let path = format!(
            "{}/tests/fixtures/temple/{}",
            env!("CARGO_MANIFEST_DIR"),
            f.file
        );
        let img = image::open(&path).unwrap_or_else(|e| panic!("{path} loads: {e}"));
        assert_eq!(
            (img.width(), img.height()),
            f.size,
            "{} is not the committed crop of {} {:?}",
            f.file,
            f.source,
            f.crop
        );
        img
    }

    fn whole(img: &DynamicImage) -> [i32; 4] {
        [0, 0, img.width() as i32, img.height() as i32]
    }

    /// Directions and angles are scale- and origin-free — only the ratio of
    /// the lattice pitches survives the normalisation — so the marker mapping
    /// needs no anchor.
    fn lattice() -> Lattice {
        Lattice::new((0, 0), 1.0)
    }

    /// Read a fixture's seals at its slot's lattice degree.
    ///
    /// [`read_door_markers`] fails unless it finds exactly `degree` seals, so
    /// every test that goes through here has "one marker per lattice
    /// neighbour" as a precondition of getting a value at all — asserting the
    /// length afterwards would assert nothing.
    fn read(f: &Fixture) -> DoorMarkers {
        let img = load(f);
        let degree = lattice::neighbours(f.current).len();
        read_door_markers(&img, whole(&img), degree)
            .unwrap_or_else(|e| panic!("{} ({}): {e}", f.file, f.case))
    }

    fn open_slots(f: &Fixture) -> Vec<String> {
        let mut got: Vec<String> = assign_markers(&lattice(), f.current, &read(f))
            .unwrap_or_else(|e| panic!("{} ({}): {e}", f.file, f.case))
            .into_iter()
            .filter(|(_, m)| m.open)
            .map(|(slot, _)| slot.as_str().to_string())
            .collect();
        got.sort();
        got
    }

    // The green/red census on all five boards and all three degrees. This is
    // also the room-icon test: the two icons drawn INSIDE every diamond are
    // red and green, and dropping INK_VALUE to admit them turns each of these
    // counts into a mismatch.
    #[test]
    fn every_board_reads_the_seal_colours_its_case_records() {
        for f in ALL {
            let green = read(f).markers.iter().filter(|m| m.open).count();
            assert_eq!(
                green,
                f.open.len(),
                "{} ({}) should show {} green seal(s)",
                f.file,
                f.case,
                f.open.len()
            );
        }
        // The three degrees the lattice can produce are all covered above.
        let degrees: BTreeSet<usize> = ALL
            .iter()
            .map(|f| lattice::neighbours(f.current).len())
            .collect();
        assert_eq!(degrees, BTreeSet::from([3, 4, 6]));
    }

    // Case 1: three seals, all red, so the room is isolated — and the beam
    // sampler's C2-D3 (which it self-flagged as uncertain) was wrong.
    #[test]
    fn case_1_tombs_reads_three_closed_doors() {
        assert!(open_slots(&TOMBS).is_empty());
    }

    // Case 3 and Case 4 are the pair that proves the mapping: the same
    // 4-neighbour shape, one green seal each, in different positions, and the
    // model has to put them on different neighbours.
    #[test]
    fn case_3_chasm_b0_opens_toward_c1() {
        assert_eq!(open_slots(&CHASM_B0), vec!["C1".to_string()]);
    }

    #[test]
    fn case_4_chasm_b1_opens_toward_the_apex() {
        assert_eq!(open_slots(&CHASM_B1), vec!["A0".to_string()]);
    }

    // Case 6: the six-neighbour shape, and the two open doors its three-room
    // component implies. Cellar (C1) is the door the advisor recommended
    // OPENING, so its absence here is the point.
    #[test]
    fn case_6_the_six_neighbour_shape_opens_only_toward_its_component() {
        assert_eq!(
            open_slots(&CLOISTER),
            vec!["C0".to_string(), "D0".to_string()],
            "the component is {{Cloister, Antechamber, Omnitect Forge}}; Cellar is still shut"
        );
    }

    // Scale invariance: a 1539 px capture at scale 1.13 reads the same
    // geometry as the 1358 px reference ones.
    #[test]
    fn the_live_scale_board_maps_with_the_same_constants() {
        assert_eq!(open_slots(&LIVE_B0), vec!["C0".to_string()]);
    }

    // The rect the caller supplies will not be perfect. A 10 px centre error
    // rotates the whole fan, and the cyclic assignment absorbs it.
    #[test]
    fn a_diamond_rect_off_by_ten_pixels_still_assigns_every_seal() {
        let img = load(&CHASM_B0);
        let degree = lattice::neighbours(CHASM_B0.current).len();
        for (dx, dy) in [(10, 0), (-10, 0), (0, 10), (0, -10), (8, 8)] {
            let rect = [
                10 + dx,
                10 + dy,
                img.width() as i32 - 20,
                img.height() as i32 - 20,
            ];
            let markers = read_door_markers(&img, rect, degree)
                .unwrap_or_else(|e| panic!("offset {dx},{dy}: {e}"));
            let open: Vec<&str> = assign_markers(&lattice(), CHASM_B0.current, &markers)
                .unwrap_or_else(|e| panic!("offset {dx},{dy}: {e}"))
                .into_iter()
                .filter(|(_, m)| m.open)
                .map(|(slot, _)| slot.as_str())
                .collect();
            assert_eq!(open, vec!["C1"], "offset {dx},{dy} moved the open door");
        }
    }

    // …but a centre wrong enough to rotate the fan past half the 55.7° gap
    // must fail loudly instead of naming the wrong neighbour.
    #[test]
    fn a_centre_far_enough_to_rotate_the_fan_is_rejected() {
        let markers = read(&CHASM_B0);
        let shoved = DoorMarkers {
            centre: (markers.centre.0 - 90, markers.centre.1),
            markers: markers.markers.clone(),
        };
        match assign_markers(&lattice(), CHASM_B0.current, &shoved) {
            Err(MarkerError::Unmappable { worst_deg }) => {
                assert!(worst_deg > MAX_RESIDUAL_DEG)
            }
            other => panic!("expected Unmappable, got {other:?}"),
        }
    }

    // A count that disagrees with the degree is an error, never a partial
    // read: three seals on a four-neighbour room leaves the missing one
    // unknowable.
    #[test]
    fn a_seal_count_that_disagrees_with_the_degree_is_an_error() {
        let img = load(&TOMBS);
        assert_eq!(
            read_door_markers(&img, whole(&img), 4),
            Err(MarkerError::CountMismatch {
                found: 3,
                expected: 4
            })
        );
        let markers = read(&TOMBS);
        assert_eq!(
            assign_markers(&lattice(), Slot::B0, &markers),
            Err(MarkerError::CountMismatch {
                found: 3,
                expected: 4
            })
        );
    }

    #[test]
    fn a_rect_outside_the_capture_is_an_error() {
        let img = load(&TOMBS);
        for rect in [
            [0, 0, img.width() as i32 + 1, img.height() as i32],
            [-1, 0, 10, 10],
            [0, 0, 0, 10],
        ] {
            assert_eq!(
                read_door_markers(&img, rect, 3),
                Err(MarkerError::RectOutsideImage),
                "{rect:?}"
            );
        }
    }

    // The modelled directions have to stay far enough apart, on EVERY slot,
    // for the assignment to be decidable at all — and MAX_RESIDUAL_DEG has to
    // stay inside half the tightest fan. Fails if AXIS_X/AXIS_Y are edited
    // into a flatter projection, or if the tolerance is widened past what the
    // geometry supports.
    #[test]
    fn no_two_corridor_directions_come_within_the_assignment_tolerance() {
        let lattice = lattice();
        let mut tightest = (f64::INFINITY, Slot::A0);
        for slot in Slot::ALL {
            let mut angles: Vec<f64> = lattice::neighbours(slot)
                .into_iter()
                .map(|to| {
                    let (dx, dy) = neighbour_direction(&lattice, slot, to);
                    dy.atan2(dx).to_degrees()
                })
                .collect();
            angles.sort_by(|a, b| a.partial_cmp(b).expect("angles are finite"));
            let n = angles.len();
            let min = (0..n)
                .map(|i| (angles[(i + 1) % n] - angles[i]).rem_euclid(360.0))
                .fold(f64::INFINITY, f64::min);
            if min < tightest.0 {
                tightest = (min, slot);
            }
        }
        let (min, _slot) = tightest;
        assert!(
            (55.5..56.0).contains(&min),
            "tightest corridor pair is {min:.2}°, measured 55.72°"
        );
        assert!(
            MAX_RESIDUAL_DEG < min / 2.0,
            "the tolerance can reach a neighbouring corridor"
        );
    }

    /// The seal model against the SHIPPED DETECTOR on every committed crop
    /// (POE-244).
    ///
    /// The test the first version of this geometry did not have and needed: the
    /// one it shipped with asserted the model against its own algebra, which a
    /// model that is wrong about the panel passes perfectly. This one predicts
    /// `centre + SEAL_RING_FRACTION × short_side × neighbour_direction` and
    /// compares it to where `read_door_markers` actually found the ink — 21
    /// seals, 5 slot shapes, 2 crop sizes, both scale families.
    ///
    /// 10 px on a 200 px rect is 5 % of the shape, which is inside a seal's own
    /// 11–25 px diameter: a prediction this close is one the eye reads as being
    /// on the same wall. Measured worst case is 8.94 px, so the bound has about
    /// a pixel of headroom and a re-fit that loses more than that fails here.
    #[test]
    fn the_committed_crops_land_within_ten_px_of_the_seal_ring() {
        let mut worst = (0.0_f64, "");
        for f in ALL {
            let read = read(f);
            let assigned = assign_markers(&lattice(), f.current, &read)
                .unwrap_or_else(|e| panic!("{} ({}): {e}", f.file, f.case));
            let img = load(f);
            let short = img.width().min(img.height()) as f64;
            let radius = SEAL_RING_FRACTION * short;
            for (slot, marker) in assigned {
                let (ux, uy) = neighbour_direction(&lattice(), f.current, slot);
                let px = read.centre.0 as f64 + ux * radius;
                let py = read.centre.1 as f64 + uy * radius;
                let err = (px - marker.position.0 as f64).hypot(py - marker.position.1 as f64);
                assert!(
                    err < 10.0,
                    "{}: {} -> {} predicted ({px:.1}, {py:.1}), detected {:?}, off by {err:.2} px",
                    f.file,
                    f.current.as_str(),
                    slot.as_str(),
                    marker.position,
                );
                if err > worst.0 {
                    worst = (err, f.file);
                }
            }
        }
        // Pinned as a floor as well as a ceiling: a change that quietly made
        // every seal land at zero error would mean the prediction had stopped
        // being independent of the detection.
        assert!(
            worst.0 > 1.0,
            "every seal predicted to under a pixel ({:.2} on {}) — the two sides are no longer independent",
            worst.0,
            worst.1,
        );
    }

    /// A seal is on the ring, which is the whole placement rule now that the
    /// outline no longer defines it.
    #[test]
    fn every_modelled_seal_sits_at_one_ring_radius() {
        let lattice = lattice();
        for slot in Slot::ALL {
            for to in lattice::neighbours(slot) {
                let (x, y) = neighbour_direction(&lattice, slot, to);
                assert!(
                    (x.hypot(y) - 1.0).abs() < 1e-12,
                    "{} -> {} is at radius {}",
                    slot.as_str(),
                    to.as_str(),
                    x.hypot(y)
                );
            }
        }
    }

    /// The outline is a real convex quadrilateral in ring order — the property
    /// a consumer drawing it as an SVG `polygon` relies on, and the one a
    /// re-ordering of [`diamond_corners`] would break silently (an hourglass
    /// renders, it just renders wrong).
    #[test]
    fn the_diamond_corners_wind_one_way_around_a_convex_shape() {
        let corners = diamond_corners();
        let cross = |i: usize| {
            let (ax, ay) = corners[i];
            let (bx, by) = corners[(i + 1) % 4];
            let (cx, cy) = corners[(i + 2) % 4];
            (bx - ax) * (cy - by) - (by - ay) * (cx - bx)
        };
        let first = cross(0);
        assert!(first.abs() > 0.1, "degenerate outline: {corners:?}");
        for i in 1..4 {
            assert!(
                cross(i) * first > 0.0,
                "corner {i} turns the other way: {corners:?}"
            );
        }
    }


    // Calibration regression: AXIS_X/AXIS_Y were fitted on one board, and this
    // is what they are worth on the other four, through the shipped detector
    // and the rect's own centre. Fails on any drift in the projection or in
    // the ink gates that moves a centroid.
    #[test]
    fn every_committed_board_maps_within_two_degrees() {
        let lattice = lattice();
        let mut worst = 0.0_f64;
        for f in ALL {
            let markers = read(f);
            for (slot, marker) in assign_markers(&lattice, f.current, &markers)
                .unwrap_or_else(|e| panic!("{} ({}): {e}", f.file, f.case))
            {
                let (mx, my) = neighbour_direction(&lattice, f.current, slot);
                let modelled = my.atan2(mx).to_degrees();
                let seen = ((marker.position.1 - markers.centre.1) as f64)
                    .atan2((marker.position.0 - markers.centre.0) as f64)
                    .to_degrees();
                worst = worst.max(angular_delta(seen, modelled).abs());
            }
        }
        assert!(
            worst < 2.0,
            "worst residual is now {worst:.2}°, measured 1.28°"
        );
    }

    // ------------------------------------------------- the ink gates --

    /// A canvas the size of four of the five committed crops, so the derived
    /// gates a synthetic blob has to clear — `min_ink` 7 px, `min_height`
    /// 10 px, `max_side` 32 px — are the ones the real boards run under.
    fn canvas() -> RgbImage {
        RgbImage::from_pixel(240, 200, Rgb([0, 0, 0]))
    }

    /// Where the one legitimate seal is drawn on every synthetic canvas.
    const SEAL: (i32, i32) = (180, 100);

    /// A filled disc 13 px across — a seal's shape and size. Drawn
    /// symmetrically, so its centroid is exactly `centre`.
    fn disc(img: &mut RgbImage, centre: (i32, i32), colour: [u8; 3]) {
        const R: i32 = 6;
        for dy in -R..=R {
            for dx in -R..=R {
                if dx * dx + dy * dy <= R * R {
                    img.put_pixel(
                        (centre.0 + dx) as u32,
                        (centre.1 + dy) as u32,
                        Rgb(colour),
                    );
                }
            }
        }
    }

    /// The one marker a synthetic canvas is expected to yield, by position.
    fn only_marker(img: &RgbImage) -> (i32, i32) {
        let img = DynamicImage::ImageRgb8(img.clone());
        read_door_markers(&img, whole(&img), 1)
            .expect("exactly one blob is marker ink")
            .markers[0]
            .position
    }

    // A red that is bright but washed out is panel art, not seal ink. Fails
    // if INK_SAT is dropped: [230, 140, 140] is 0.39 saturated, clears
    // INK_VALUE at 0.90, and reads as a second red seal.
    #[test]
    fn a_desaturated_red_blob_is_not_seal_ink() {
        let mut img = canvas();
        disc(&mut img, SEAL, [230, 20, 20]);
        disc(&mut img, (60, 100), [230, 140, 140]);
        assert_eq!(only_marker(&img), SEAL);
    }

    // Scattered speckle that the merge pass folds into one tall, near-empty
    // blob is noise, not a seal. Fails if MIN_INK_FRACTION is dropped: six
    // pixels over eleven rows clear the height gate and the 6 px floor, and
    // only the 7 px this crop's area buys rejects them.
    #[test]
    fn a_speckle_with_too_little_ink_is_not_a_seal() {
        let mut img = canvas();
        disc(&mut img, SEAL, [230, 20, 20]);
        for i in 0..6 {
            img.put_pixel(60, (90 + 2 * i) as u32, Rgb([230, 20, 20]));
        }
        assert_eq!(only_marker(&img), SEAL);
    }

    // A blob far bigger than a seal is a panel fill, not a door marker. Fails
    // if MAX_BLOB_SIDE is dropped: 60 px is nearly twice the 32 px a seal may
    // measure on this crop, and everything else about it reads as red ink.
    #[test]
    fn a_blob_far_larger_than_a_seal_is_not_a_seal() {
        let mut img = canvas();
        disc(&mut img, SEAL, [230, 20, 20]);
        for y in 70..130 {
            for x in 20..80 {
                img.put_pixel(x, y, Rgb([230, 20, 20]));
            }
        }
        assert_eq!(only_marker(&img), SEAL);
    }

    // ------------------------------------------------- settling a board --

    /// The reference board as POE-168 reads it: Tombs at D3, with `C2-D3` in
    /// `doors` and all three of Tombs' corridors flagged uncertain.
    fn reference_layout() -> TempleLayout {
        let path = format!(
            "{}/tests/fixtures/temple/board-ref-1374.png",
            env!("CARGO_MANIFEST_DIR")
        );
        let board = image::open(&path).unwrap_or_else(|e| panic!("{path} loads: {e}"));
        super::super::reader::read_layout(&board).expect("the reference board reads")
    }

    // End to end against POE-168: the beam sampler reads C2-D3 as open through
    // the selection frame, and the seals say Tombs is isolated. The settled
    // set must have dropped it — `layout.doors` unioned with the open seals
    // would keep it.
    #[test]
    fn the_seals_overrule_a_beam_false_positive_on_the_current_room() {
        let layout = reference_layout();
        let c2_d3 = Edge::new(Slot::C2, Slot::D3);
        assert_eq!(layout.current, Some(Slot::D3));
        assert!(
            layout.doors.contains(&c2_d3) && layout.uncertain.contains(&c2_d3),
            "the beam sampler reads C2-D3 as open, and flags it uncertain"
        );

        let settled = apply_markers(&layout, &read(&TOMBS)).expect("the seals map");
        assert!(
            !settled.iter().any(|e| e.touches(Slot::D3)),
            "Tombs is isolated; every corridor touching it is shut, got {settled:?}"
        );
    }

    // …and the rest of the board is carried through untouched: the seals speak
    // only for the room the diamond is a picture of.
    #[test]
    fn corridors_away_from_the_current_room_survive_the_settling() {
        let layout = reference_layout();
        let settled = apply_markers(&layout, &read(&TOMBS)).expect("the seals map");
        let expected: BTreeSet<Edge> = layout
            .doors
            .iter()
            .copied()
            .filter(|e| !e.touches(Slot::D3))
            .collect();
        assert_eq!(settled, expected);
        assert!(settled.contains(&Edge::new(Slot::A0, Slot::B0)));
    }

    // A corridor the sampler read CONFIDENTLY — one it did not flag uncertain
    // — that its seal contradicts is surfaced, not discarded. Here C2-D3 is
    // taken out of `uncertain`, leaving a clean read of open against a red
    // seal.
    #[test]
    fn a_clean_beam_read_that_contradicts_its_seal_is_an_error() {
        let mut layout = reference_layout();
        let c2_d3 = Edge::new(Slot::C2, Slot::D3);
        assert!(layout.uncertain.remove(&c2_d3));
        assert_eq!(
            apply_markers(&layout, &read(&TOMBS)),
            Err(MarkerError::BeamDisagreement {
                edge: c2_d3,
                beam_open: true
            })
        );
    }

    // A layout read between rooms has no room for the diamond to describe.
    #[test]
    fn a_layout_with_no_current_room_cannot_be_settled() {
        let mut layout = reference_layout();
        layout.current = None;
        assert_eq!(
            apply_markers(&layout, &read(&TOMBS)),
            Err(MarkerError::NoCurrentRoom)
        );
    }
}
