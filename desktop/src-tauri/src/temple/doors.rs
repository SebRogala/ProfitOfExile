//! Corridor and current-room reading from panel pixels (POE-168).
//!
//! Two measurements, both gold-pixel fractions over a small patch placed by
//! [`super::lattice`]:
//!
//! - **corridors** — a `(2 × 14 × scale)²` patch at each of the 26 edge
//!   midpoints. Open corridors measured 0.24–0.50, closed ones ≤0.077 across 8
//!   boards, so the two classes are split by Otsu rather than by a constant:
//!   the panel's absolute brightness moves with the game's own settings, the
//!   *separation* does not. See "Why the two corridor families are calibrated
//!   apart" below — the split runs per [`Corridor`] family, not over all 26.
//! - **current room** — the ornate gold frame ring drawn OUTSIDE the plate
//!   border of the room the player is standing in. This one IS a fixed
//!   threshold ([`CURRENT_ROOM_GOLD`]) rather than a relative split, because
//!   the ring is a single population with nothing to split it against; it was
//!   validated separately on 8 measured boards plus the 2 committed fixtures.
//!
//! # Why the two corridor families are calibrated apart
//!
//! The OPEN class is bimodal by geometry. A horizontal corridor spans the 39
//! reference-px gap between two same-row plates and its beam art fills that
//! short gap densely; a diagonal one spans a longer slanted gap. Measured on
//! the two committed fixtures:
//!
//! | family | open | closed (clean) |
//! |---|---|---|
//! | horizontal | 0.447–0.490 | 0.000 |
//! | diagonal | 0.269–0.330 | 0.000–0.077 |
//!
//! A single Otsu over all 26 lands in the *closed↔diagonal* gap only while the
//! board still has closed corridors to anchor the low cluster. Near the end of
//! a temple — boards 7–11 of the measured set are fully connected, which is the
//! normal terminal state and not an edge case — one Otsu instead splits
//! horizontal from diagonal and calls every diagonal corridor closed, with a
//! 0.13–0.15 gap that sails past the [`MIN_CLUSTER_GAP`] guard and reports
//! `High`. Calibrating each family against itself removes that failure mode:
//! within a family the only remaining separation is open vs closed.
//!
//! # Why the current room is excluded from calibration
//!
//! That selection frame overhangs ~15 px into a 21 px plate gap, so it covers
//! the midpoints of every corridor touching the current room and fakes gold
//! there (0.28 and 0.17 measured on corridors that are actually closed).
//! Including those values in the Otsu split drags the threshold up and starts
//! losing real doors elsewhere on the board. They are therefore excluded from
//! the calibration and returned in [`DoorRead::uncertain`] — still classified,
//! with an extra mean-luminance gate that separates real corridors (80–94)
//! from frame artefacts (43–58), but flagged so POE-169 can overrule them from
//! the side panel's door markers.
//!
//! An edge can appear in BOTH `doors` and `uncertain`: `uncertain` says "this
//! reading came from a poisoned patch", not "this is closed".

// POE-171 is that caller: `temple::run` and `temple::slice` reach this module
// on every read, so the file-level `#![allow(dead_code)]` is gone. What is
// still uncalled carries its own attribute, which is now the inventory of what
// only the tests reach rather than a blanket over the whole file.

use std::collections::BTreeSet;

use image::RgbImage;

use super::lattice::{Corridor, Edge, Lattice, Slot, PATCH_HALF};

/// Gold fraction above which a plate is judged to carry the selection frame.
///
/// Unlike the corridor split this is an absolute threshold, because the ring is
/// one population with no second cluster to calibrate against. The prototype's
/// 0.30 was correct on all 8 measured boards. On the two committed fixtures the
/// framed plate rings score 0.44 and 0.36 while the brightest unframed plate
/// scores 0.15 and 0.20 — every plate carries some gold, so the threshold has
/// to sit in that gap and cannot be "any gold at all".
pub const CURRENT_ROOM_GOLD: f32 = 0.30;

/// Mean luminance an *uncertain* corridor must also clear. Real corridors read
/// 80–94, selection-frame artefacts 43–58.
///
/// The margin is thin at one measured point: `C2-D3` on `board-ref-1374.png` is
/// a real open corridor reading 65.7, only 0.7 above this floor, and it is the
/// value `reader::reference_board_reads_like_the_prototype` pins by expecting
/// `C2-D3` in that board's `doors`. Raising this constant past 65.7 drops a
/// true door; that test is what says so.
const UNCERTAIN_MIN_LUM: f32 = 65.0;

/// Smallest gap the Otsu split may straddle and still be believed.
///
/// The gap is `min(open cluster) − max(closed cluster)` within one corridor
/// family. Measured per family on the two committed fixtures: 0.4467 and 0.4778
/// horizontal, 0.2012 and 0.2022 diagonal — 0.08 is comfortably under the worst
/// of those.
///
/// It is a *gap* test, not a spread test. A family that is all-open or all-low
/// is one cluster spread over a band, and its largest internal spacing stays
/// well under 0.08 (0.006–0.03 measured); so does an occluded panel's. Falling
/// under this floor therefore says "one class here", not which class —
/// [`single_class`] then asks the absolute band which one it is.
///
/// Clearing it is also the strongest evidence a family can give that the panel
/// is lit at all, which is what [`combine`] spends it on.
const MIN_CLUSTER_GAP: f32 = 0.08;

/// Gold fraction a corridor must reach to be called open with no closed
/// corridor in its family to compare against.
///
/// Used **only** when the Otsu split is degenerate. Open corridors measured
/// ≥0.24 across 8 boards on two machines (≥0.269 on the two committed
/// fixtures); 0.22 sits under that with margin.
const OPEN_GOLD_FLOOR: f32 = 0.22;

/// …and the fraction it must stay under to be called low on the same terms.
///
/// Closed corridors measured ≤0.10 across the same 8 boards (≤0.077 on the
/// fixtures); 0.15 sits above that with margin. The 0.15–0.22 span between the
/// two constants is deliberately dead: a degenerate family landing inside it,
/// or straddling it, is [`FamilyRead::Ambiguous`] rather than guessed at.
///
/// "Low", not "closed": a family reading entirely under this ceiling is a bare
/// family on a lit panel and an unlit panel alike, and only the other family
/// can say which — see [`combine`].
const CLOSED_GOLD_CEILING: f32 = 0.15;

/// How much of the board reading to trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Confidence {
    /// Every corridor family landed on one side of the measured band, and at
    /// least one of them proved the panel is lit while doing so. See
    /// [`combine`] — the judgement is over the panel, not per family.
    High,
    /// Either some family's fractions sit in, or straddle, the dead span
    /// between [`CLOSED_GOLD_CEILING`] and [`OPEN_GOLD_FLOOR`] — an occluded or
    /// mid-animation panel — or no family found gold anywhere, which is a
    /// dimmed or faded panel rather than a board. The door sets are still
    /// returned, because a caller may want to show them, but nothing downstream
    /// should act on them.
    Low,
}

/// The gold-fraction threshold each corridor family calibrated.
///
/// Two numbers rather than one because the families are separate populations —
/// see the module header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub horizontal: f32,
    pub diagonal: f32,
}

impl Thresholds {
    /// The threshold a corridor of this family is judged against.
    pub fn of(self, kind: Corridor) -> f32 {
        match kind {
            Corridor::Horizontal => self.horizontal,
            Corridor::Diagonal => self.diagonal,
        }
    }
}

/// Mean luminance and gold fraction of one sampled patch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatchStats {
    pub lum: f32,
    pub gold: f32,
}

/// The corridor reading for one board.
#[derive(Debug, Clone, PartialEq)]
pub struct DoorRead {
    /// Corridors judged open.
    pub doors: BTreeSet<Edge>,
    /// Corridors touching the current room, whose patch the selection frame
    /// covers. A subset of the 26 edges, overlapping `doors` freely.
    pub uncertain: BTreeSet<Edge>,
    /// What the clean edges of each family calibrated.
    pub thresholds: Thresholds,
    pub confidence: Confidence,
}

/// Mean luminance and gold fraction over the `2·hw × 2·hh` box centred on
/// `(cx, cy)`, clipped to the image.
///
/// "Gold" is bright, red-dominant and not green-dominant — the palette the
/// corridor beams and the selection frame are drawn in. An empty or fully
/// off-image box reads as black rather than panicking: a panel captured
/// half off-screen must produce a low-confidence read, not a crash.
pub fn patch_stats(rgb: &RgbImage, cx: i32, cy: i32, hw: i32, hh: i32) -> PatchStats {
    let (w, h) = (rgb.width() as i32, rgb.height() as i32);
    let x0 = (cx - hw).max(0);
    let y0 = (cy - hh).max(0);
    let x1 = (cx + hw).min(w);
    let y1 = (cy + hh).min(h);
    if x1 <= x0 || y1 <= y0 {
        return PatchStats {
            lum: 0.0,
            gold: 0.0,
        };
    }
    let mut lum_sum = 0.0f64;
    let mut gold = 0u32;
    let mut n = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = rgb.get_pixel(x as u32, y as u32);
            let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
            let lum = 0.299 * r + 0.587 * g + 0.114 * b;
            lum_sum += lum as f64;
            if lum > 90.0 && r > b + 30.0 && r >= g - 10.0 {
                gold += 1;
            }
            n += 1;
        }
    }
    PatchStats {
        lum: (lum_sum / n as f64) as f32,
        gold: gold as f32 / n as f32,
    }
}

/// Gold fraction of the frame ring drawn just above `slot`'s plate.
pub fn frame_gold(rgb: &RgbImage, lattice: &Lattice, slot: Slot) -> f32 {
    let (hw, hh) = lattice.plate_half();
    let pad = (11.0 * lattice.scale as f64) as i32;
    let ring_hh = (pad / 2).max(2);
    let (cx, cy) = lattice.centre(slot);
    patch_stats(rgb, cx, cy - hh - pad / 2, hw, ring_hh).gold
}

/// The slot the player is standing in, or `None` when the panel was opened
/// between rooms — which is guaranteed to happen and is not an error.
pub fn current_room(rgb: &RgbImage, lattice: &Lattice) -> Option<Slot> {
    let (slot, score) = Slot::ALL
        .into_iter()
        .map(|slot| (slot, frame_gold(rgb, lattice, slot)))
        .fold((Slot::A0, 0.0f32), |best, next| {
            if next.1 > best.1 {
                next
            } else {
                best
            }
        });
    (score > CURRENT_ROOM_GOLD).then_some(slot)
}

/// Read all 26 corridors.
///
/// Each [`Corridor`] family is calibrated against its own clean edges, and the
/// board's confidence is the weaker of the two — a board is only readable when
/// every family is.
pub fn read_doors(rgb: &RgbImage, lattice: &Lattice, current: Option<Slot>) -> DoorRead {
    let hw = (PATCH_HALF * lattice.scale as f64) as i32;
    let measured: Vec<(Edge, Corridor, PatchStats)> = super::lattice::edges()
        .into_iter()
        .map(|edge| {
            let kind = edge
                .kind()
                .expect("lattice::edges() yields only real corridors");
            let (mx, my) = lattice.edge_midpoint(edge);
            (edge, kind, patch_stats(rgb, mx, my, hw, hw))
        })
        .collect();

    let touches_current = |edge: &Edge| current.is_some_and(|c| edge.touches(c));
    let clean_of = |family: Corridor| -> Vec<f32> {
        measured
            .iter()
            .filter(|(edge, kind, _)| *kind == family && !touches_current(edge))
            .map(|(_, _, s)| s.gold)
            .collect()
    };
    let horizontal = calibrate(&clean_of(Corridor::Horizontal));
    let diagonal = calibrate(&clean_of(Corridor::Diagonal));
    let thresholds = Thresholds {
        horizontal: horizontal.threshold(),
        diagonal: diagonal.threshold(),
    };
    let confidence = combine(horizontal, diagonal);

    let mut doors = BTreeSet::new();
    let mut uncertain = BTreeSet::new();
    for (edge, kind, stats) in &measured {
        let threshold = thresholds.of(*kind);
        if touches_current(edge) {
            uncertain.insert(*edge);
            if stats.gold > threshold && stats.lum > UNCERTAIN_MIN_LUM {
                doors.insert(*edge);
            }
        } else if stats.gold > threshold {
            doors.insert(*edge);
        }
    }
    DoorRead {
        doors,
        uncertain,
        thresholds,
        confidence,
    }
}

/// What one corridor family's clean gold fractions say about that family.
///
/// Deliberately NOT a confidence: three of these four variants are decisive on
/// their own, and the fourth is decisive only next to the other family's. See
/// [`combine`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FamilyRead {
    /// Two clusters straddling a gap of at least [`MIN_CLUSTER_GAP`]. That gap
    /// *is* the open/closed boundary, and the threshold is its midpoint.
    Split(f32),
    /// One cluster, entirely at or above [`OPEN_GOLD_FLOOR`]: every corridor of
    /// the family is built. The terminal state of every finished temple.
    AllOpen,
    /// One cluster, entirely at or below [`CLOSED_GOLD_CEILING`]: no gold on
    /// any of this family's midpoints.
    ///
    /// The only variant that cannot be read on its own. A bare family on a lit
    /// panel and a family on an unlit panel produce the same measurement, and
    /// nothing inside this family separates them — [`combine`] asks the other
    /// family.
    AllLow,
    /// One cluster that is neither: values inside, or straddling, the dead span
    /// between [`CLOSED_GOLD_CEILING`] and [`OPEN_GOLD_FLOOR`]. What an
    /// occluded or mid-animation panel produces, and never readable.
    Ambiguous(f32),
}

impl FamilyRead {
    /// The gold fraction this family's corridors are judged against.
    ///
    /// For the two single-class reads it is the far constant of the band — the
    /// one that classifies the whole family the way it read, and that also
    /// judges the edges excluded from calibration (those the selection frame
    /// poisons) against a number that means something.
    pub fn threshold(self) -> f32 {
        match self {
            FamilyRead::Split(threshold) | FamilyRead::Ambiguous(threshold) => threshold,
            FamilyRead::AllOpen => CLOSED_GOLD_CEILING,
            FamilyRead::AllLow => OPEN_GOLD_FLOOR,
        }
    }

    /// Whether this family is by itself proof the panel is lit: it found gold,
    /// and enough of it to place a boundary.
    fn is_lit(self) -> bool {
        matches!(self, FamilyRead::Split(_) | FamilyRead::AllOpen)
    }
}

/// How much of a two-family reading to trust — a judgement over the PANEL, not
/// over either family.
///
/// [`FamilyRead::AllLow`] is the reason this is not a per-family question. Read
/// alone it is ambiguous between two very different things:
///
/// - a family with nothing built yet, on a panel that is plainly lit. A temple
///   opens with corridors already built — Case 1 of `TEMPLE-CORE-RULES.md`
///   still had a 10-room connected component with 9 rooms left — but there is
///   no rule that both families are represented among them, so one bare family
///   beside a built one is an ordinary board and has to read as one;
/// - a dimmed, faded or overdrawn panel, where the reading means nothing. The
///   anchor cannot catch this: its NCC is mean-subtracted and
///   variance-normalised, so a contrast drop costs it nothing and the panel
///   still matches at ~1.0 while every midpoint reads 0.0 gold.
///
/// The other family is what separates them. If it is lit — it split into two
/// clusters, or it sat entirely in the open band — the panel demonstrably draws
/// gold where gold belongs, so the bare family is bare and not dark. If BOTH
/// families are all-low there is no gold anywhere on the panel, which is not a
/// board state the game produces, and the read is refused.
///
/// [`FamilyRead::Ambiguous`] is refused whatever the other family did: values
/// in the dead span are not a class, and a lit neighbour says nothing about
/// which side of the band they belong on.
fn combine(a: FamilyRead, b: FamilyRead) -> Confidence {
    let readable = |family: FamilyRead, other: FamilyRead| match family {
        FamilyRead::Split(_) | FamilyRead::AllOpen => true,
        FamilyRead::AllLow => other.is_lit(),
        FamilyRead::Ambiguous(_) => false,
    };
    if readable(a, b) && readable(b, a) {
        Confidence::High
    } else {
        Confidence::Low
    }
}

/// Calibrate one corridor family's clean gold fractions.
///
/// Pure and per-family on purpose: it reports what this family measured and
/// stops there, leaving the panel-level judgement to [`combine`].
///
/// Otsu first: when the winning split straddles a real gap, that gap *is* the
/// open/closed boundary.
///
/// When it does not, the family holds one class rather than two — which for a
/// single family is the common case, not a failure: a fully connected temple
/// has no closed corridor left to anchor a low cluster with, and a bare family
/// has no open one. Only then does the measured absolute band get consulted,
/// and only to answer *which* class.
pub fn calibrate(clean: &[f32]) -> FamilyRead {
    let band = single_class(clean);
    match otsu_split(clean) {
        Some((threshold, gap)) if gap >= MIN_CLUSTER_GAP => FamilyRead::Split(threshold),
        Some((threshold, _)) => band.unwrap_or(FamilyRead::Ambiguous(threshold)),
        // Fewer than two samples: nothing to split at all.
        None => band.unwrap_or_else(|| {
            FamilyRead::Ambiguous(clean.iter().sum::<f32>() / clean.len().max(1) as f32)
        }),
    }
}

/// Otsu 2-class split of `v` as `(threshold, gap)`, or `None` when there are
/// fewer than two samples to split.
fn otsu_split(v: &[f32]) -> Option<(f32, f32)> {
    if v.len() < 2 {
        return None;
    }
    let mut v: Vec<f32> = v.to_vec();
    v.sort_by(f32::total_cmp);

    let total: f64 = v.iter().map(|&x| x as f64).sum();
    let mut lo_sum = 0.0f64;
    let mut best_variance = -1.0f64;
    let mut split_at = 1usize;
    for i in 1..v.len() {
        lo_sum += v[i - 1] as f64;
        let (n_lo, n_hi) = (i as f64, (v.len() - i) as f64);
        let delta = lo_sum / n_lo - (total - lo_sum) / n_hi;
        let variance = n_lo * n_hi * delta * delta;
        if variance > best_variance {
            best_variance = variance;
            split_at = i;
        }
    }
    Some((
        (v[split_at - 1] + v[split_at]) / 2.0,
        v[split_at] - v[split_at - 1],
    ))
}

/// Which single class a one-cluster family holds, read off the measured
/// absolute band, or `None` when it is decisively on neither side of it.
///
/// Both ends are named here, symmetrically — the asymmetry lives one level up,
/// in what [`combine`] is willing to believe about each.
fn single_class(v: &[f32]) -> Option<FamilyRead> {
    let min = v.iter().copied().reduce(f32::min)?;
    let max = v.iter().copied().reduce(f32::max)?;
    if min >= OPEN_GOLD_FLOOR {
        Some(FamilyRead::AllOpen)
    } else if max <= CLOSED_GOLD_CEILING {
        Some(FamilyRead::AllLow)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(edges: &BTreeSet<Edge>) -> Vec<String> {
        edges.iter().map(|e| e.to_string()).collect()
    }

    /// Two well-separated clusters: the threshold lands in the gap.
    #[test]
    fn calibrate_splits_two_clusters_in_the_gap() {
        let clean = [0.03, 0.00, 0.07, 0.30, 0.45, 0.28, 0.02];
        match calibrate(&clean) {
            FamilyRead::Split(threshold) => assert!(
                threshold > 0.07 && threshold < 0.28,
                "threshold {threshold} did not land in the 0.07..0.28 gap"
            ),
            other => panic!("expected a split, got {other:?}"),
        }
    }

    /// The threshold is the midpoint of the straddled gap, not a constant —
    /// scaling every fraction scales it too.
    #[test]
    fn calibrate_threshold_follows_the_data() {
        let dim = calibrate(&[0.01, 0.02, 0.20, 0.22]).threshold();
        let bright = calibrate(&[0.02, 0.04, 0.40, 0.44]).threshold();
        assert!((dim - 0.11).abs() < 1e-5, "dim board threshold {dim}");
        assert!((bright - 0.22).abs() < 1e-5, "bright board threshold {bright}");
    }

    /// The occlusion guard: fractions in one narrow band inside the dead span
    /// have no two clusters to find and no side of the band to fall on.
    #[test]
    fn calibrate_reports_ambiguous_without_a_gap() {
        let read = calibrate(&[0.20, 0.21, 0.22, 0.23, 0.24, 0.25]);
        assert!(matches!(read, FamilyRead::Ambiguous(_)), "{read:?}");
    }

    /// The guard is a gap test, not a spread test: a wide but gapless ramp is
    /// still a single cluster, and one that spans the whole band at that.
    #[test]
    fn calibrate_reports_ambiguous_on_a_gapless_ramp() {
        let ramp: Vec<f32> = (0..20).map(|i| i as f32 * 0.025).collect();
        let read = calibrate(&ramp);
        assert!(matches!(read, FamilyRead::Ambiguous(_)), "{read:?}");
    }

    /// A gap just over the floor is a split, one just under is not. Both
    /// samples straddle the dead span so the absolute band cannot rescue the
    /// narrow one — this is the gap floor on its own.
    #[test]
    fn calibrate_splits_only_on_a_gap_at_or_over_the_measured_floor() {
        let just_over = calibrate(&[0.10, 0.11, 0.20, 0.21]);
        assert!(
            matches!(just_over, FamilyRead::Split(_)),
            "gap 0.09 should be believed, got {just_over:?}"
        );
        let just_under = calibrate(&[0.10, 0.11, 0.18, 0.19]);
        assert!(
            matches!(just_under, FamilyRead::Ambiguous(_)),
            "gap 0.07 should not be, got {just_under:?}"
        );
    }

    /// One sample has no split, so the absolute band decides — and it can: a
    /// family reduced to a single clean edge by the selection frame still lands
    /// on a side of the band.
    #[test]
    fn calibrate_falls_back_to_the_band_for_a_single_open_sample() {
        let read = calibrate(&[0.42]);
        assert_eq!(read, FamilyRead::AllOpen);
        assert!(0.42 > read.threshold(), "threshold {}", read.threshold());
    }

    /// …and symmetrically at the other end. What this single low sample does
    /// NOT settle is whether the panel is lit; that is [`combine`]'s question,
    /// not this one's.
    #[test]
    fn calibrate_falls_back_to_the_band_for_a_single_low_sample() {
        let read = calibrate(&[0.03]);
        assert_eq!(read, FamilyRead::AllLow);
        assert!(0.03 <= read.threshold(), "threshold {}", read.threshold());
    }

    /// …but not when that one sample lands between the two band constants.
    #[test]
    fn calibrate_cannot_classify_a_single_sample_inside_the_dead_span() {
        let read = calibrate(&[0.18]);
        assert!(matches!(read, FamilyRead::Ambiguous(_)), "{read:?}");
    }

    /// The REF board's open horizontal and open diagonal corridors, each fed to
    /// `calibrate` with no closed corridor beside them — the end-of-run state
    /// where every corridor in the family is built. Each family must still read
    /// every one of its edges open.
    #[test]
    fn a_family_of_only_open_corridors_reads_all_open() {
        for (family, values) in [
            ("horizontal", &[0.4778, 0.4837, 0.4896][..]),
            (
                "diagonal",
                &[0.2781, 0.2988, 0.2988, 0.3047, 0.3136, 0.3299][..],
            ),
        ] {
            let read = calibrate(values);
            assert_eq!(read, FamilyRead::AllOpen, "{family}");
            for &v in values {
                assert!(
                    v > read.threshold(),
                    "{family}: {v} read closed against {}",
                    read.threshold()
                );
            }
        }
    }

    /// The mirror case, at family level: nothing built yet reads all-low, and
    /// its threshold closes every one of them. Whether that means "bare" or
    /// "the panel is dark" is not decided here. The values are the two
    /// fixtures' measured closed corridors.
    #[test]
    fn a_family_of_only_low_corridors_reads_all_low() {
        for (family, values) in [
            ("horizontal", &[0.0, 0.0, 0.0, 0.0][..]),
            (
                "diagonal",
                &[0.0, 0.0, 0.0385, 0.0385, 0.0385, 0.0769, 0.0769][..],
            ),
        ] {
            let read = calibrate(values);
            assert_eq!(read, FamilyRead::AllLow, "{family}");
            for &v in values {
                assert!(
                    v <= read.threshold(),
                    "{family}: {v} read open against {}",
                    read.threshold()
                );
            }
        }
    }

    /// Which band constant a single-class family hands back is not cosmetic: it
    /// is what resolves the edges EXCLUDED from calibration. The selection
    /// frame drags a real corridor's patch down into the dead span (0.17 and
    /// 0.28 measured on corridors it covers), and on a family that read
    /// all-open such an edge belongs to the open class its family established.
    #[test]
    fn an_all_open_family_resolves_a_dead_span_edge_as_open() {
        let read = calibrate(&[0.2781, 0.2988, 0.3047, 0.3136, 0.3299]);
        assert_eq!(read, FamilyRead::AllOpen);
        assert!(
            0.18 > read.threshold(),
            "a poisoned patch at 0.18 read closed against {}",
            read.threshold()
        );
    }

    /// …and the mirror: on a family that read all-low the same 0.18 belongs to
    /// the closed class. The two constants are the two answers, and swapping
    /// them changes every excluded edge on both kinds of board.
    #[test]
    fn an_all_low_family_resolves_a_dead_span_edge_as_closed() {
        let read = calibrate(&[0.0, 0.0, 0.0385, 0.0385, 0.0769]);
        assert_eq!(read, FamilyRead::AllLow);
        assert!(
            0.18 <= read.threshold(),
            "a poisoned patch at 0.18 read open against {}",
            read.threshold()
        );
    }

    /// A one-cluster family sitting in the dead span is what an occluded or
    /// mid-animation panel produces, and the band must refuse to name it rather
    /// than pick the nearer side.
    #[test]
    fn a_single_cluster_inside_the_dead_span_is_ambiguous() {
        let read = calibrate(&[0.16, 0.17, 0.18, 0.19, 0.20, 0.21]);
        assert!(matches!(read, FamilyRead::Ambiguous(_)), "{read:?}");
    }

    /// Straddling the span is equally unreadable: some edges are decisively
    /// closed, some decisively open, but the family has no gap between them.
    #[test]
    fn a_single_cluster_straddling_the_dead_span_is_ambiguous() {
        let read = calibrate(&[0.13, 0.16, 0.19, 0.21, 0.24]);
        assert!(matches!(read, FamilyRead::Ambiguous(_)), "{read:?}");
    }

    /// Gold is bright, red-dominant, not green-dominant. A patch of the
    /// corridor palette reads as all gold; the dark panel behind it as none.
    #[test]
    fn patch_stats_counts_gold_pixels_only() {
        let gold = RgbImage::from_pixel(8, 8, image::Rgb([214, 176, 66]));
        assert!((patch_stats(&gold, 4, 4, 4, 4).gold - 1.0).abs() < 1e-6);

        let panel = RgbImage::from_pixel(8, 8, image::Rgb([30, 26, 20]));
        assert_eq!(patch_stats(&panel, 4, 4, 4, 4).gold, 0.0);

        // Bright, but blue — the corridor test must reject it.
        let blue = RgbImage::from_pixel(8, 8, image::Rgb([80, 120, 220]));
        assert_eq!(patch_stats(&blue, 4, 4, 4, 4).gold, 0.0);

        // Bright and not green-dominant, but not red-dominant either: pale UI
        // chrome and white text sit all over this panel and must not read as
        // corridor.
        let pale = RgbImage::from_pixel(8, 8, image::Rgb([200, 200, 200]));
        assert_eq!(patch_stats(&pale, 4, 4, 4, 4).gold, 0.0);
    }

    /// Red-dominant and not green-dominant, but too dark: the corridor beams
    /// glow, and the panel's own dark red trim does not. Only the luminance
    /// clause rejects this one.
    #[test]
    fn patch_stats_rejects_a_dark_red_pixel() {
        let dark_red = RgbImage::from_pixel(8, 8, image::Rgb([60, 20, 10]));
        assert_eq!(patch_stats(&dark_red, 4, 4, 4, 4).gold, 0.0);
    }

    /// Bright and red-above-blue, but green-dominant: the temple panel draws
    /// its connected-room highlights in green. Only the `r >= g - 10` clause
    /// rejects this one.
    #[test]
    fn patch_stats_rejects_a_green_dominant_pixel() {
        let green = RgbImage::from_pixel(8, 8, image::Rgb([100, 200, 50]));
        assert_eq!(patch_stats(&green, 4, 4, 4, 4).gold, 0.0);
    }

    /// A patch that runs off the image is clipped, and one entirely outside it
    /// reads as black — a half-captured panel must not panic.
    #[test]
    fn patch_stats_clips_to_the_image() {
        let mut img = RgbImage::from_pixel(8, 8, image::Rgb([0, 0, 0]));
        for x in 0..4 {
            for y in 0..8 {
                img.put_pixel(x, y, image::Rgb([214, 176, 66]));
            }
        }
        // Centred on x=0: the left half of the box is off-image, so only the
        // gold columns 0..4 are counted.
        assert!((patch_stats(&img, 0, 4, 4, 4).gold - 1.0).abs() < 1e-6);
        assert_eq!(
            patch_stats(&img, -50, 4, 4, 4),
            PatchStats {
                lum: 0.0,
                gold: 0.0
            }
        );
    }

    /// With no current room every edge is calibration input and nothing is
    /// flagged — the between-rooms case must not silently drop 26 edges into
    /// `uncertain`. Both families are all-low here, which is the one shape that
    /// refuses the panel outright, so the test also pins that the flagging is
    /// driven by `current` and not by the confidence.
    #[test]
    fn no_current_room_leaves_every_edge_clean() {
        let img = RgbImage::from_pixel(1000, 700, image::Rgb([30, 26, 20]));
        let read = read_doors(&img, &Lattice::new((500, 600), 1.0), None);
        assert!(read.uncertain.is_empty());
        assert!(read.doors.is_empty());
        assert_eq!(read.confidence, Confidence::Low);
    }

    /// A board whose corridors are ALL open — the state every finished temple
    /// ends in. Both families are then single-cluster, and a single Otsu over
    /// all 26 would split the horizontal family from the diagonal one and call
    /// all 18 diagonals closed. Per-family calibration must return all 26.
    #[test]
    fn a_fully_connected_board_reads_every_corridor_open() {
        let lattice = Lattice::new((500, 600), 1.0);
        let mut img = RgbImage::from_pixel(1000, 700, image::Rgb([30, 26, 20]));
        for edge in super::super::lattice::edges() {
            // The measured densities of the two families: an open horizontal
            // corridor fills its short gap far more densely than a diagonal.
            let density = match edge.kind() {
                Some(Corridor::Horizontal) => 0.47,
                _ => 0.30,
            };
            paint_corridor_for_test(&mut img, &lattice, edge, density);
        }
        let read = read_doors(&img, &lattice, None);
        assert_eq!(read.doors.len(), 26, "open corridors: {:?}", names(&read.doors));
        assert_eq!(read.confidence, Confidence::High);
    }

    /// A board with every horizontal built and no diagonal built. The diagonal
    /// family alone cannot tell "bare" from "dark", but the horizontal one sat
    /// entirely in the open band, so the panel is demonstrably drawing gold —
    /// the diagonals are bare, and the board reads.
    ///
    /// The all-open neighbour is the weaker of the two lit forms: it has no
    /// closed corridor of its own, so nothing but the absolute band vouches for
    /// it. It still has to count.
    #[test]
    fn a_bare_family_beside_an_all_open_one_reads_closed_and_the_board_reads() {
        let lattice = Lattice::new((500, 600), 1.0);
        let mut img = RgbImage::from_pixel(1000, 700, image::Rgb([30, 26, 20]));
        for edge in super::super::lattice::edges() {
            if edge.kind() == Some(Corridor::Horizontal) {
                paint_corridor_for_test(&mut img, &lattice, edge, 0.47);
            }
        }
        let read = read_doors(&img, &lattice, None);
        let open: Vec<String> = names(&read.doors);
        assert_eq!(
            open,
            vec!["B0-B1", "C0-C1", "C1-C2", "D0-D1", "D1-D2", "D2-D3", "E0-E1", "E1-E2"]
        );
        assert_eq!(read.confidence, Confidence::High);
    }

    /// The same arrangement with the lit family split rather than all-open —
    /// an ordinary mid-temple board, where some horizontals are built and no
    /// diagonal is yet. The split is the strongest evidence a family can give
    /// that the panel is lit, so the bare diagonals read closed and the board
    /// reads at full confidence. Refusing this board is the bug this rule
    /// exists to prevent.
    #[test]
    fn a_bare_family_beside_a_split_one_reads_closed_and_the_board_reads() {
        let lattice = Lattice::new((500, 600), 1.0);
        let mut img = RgbImage::from_pixel(1000, 700, image::Rgb([30, 26, 20]));
        let mut built: Vec<String> = Vec::new();
        for edge in super::super::lattice::edges() {
            if edge.kind() != Some(Corridor::Horizontal) {
                continue;
            }
            // Every other horizontal built, at the two measured extremes, so
            // this family splits on a real gap.
            let open = built.len().is_multiple_of(2);
            paint_corridor_for_test(&mut img, &lattice, edge, if open { 0.478 } else { 0.077 });
            if open {
                built.push(edge.to_string());
            }
        }
        let read = read_doors(&img, &lattice, None);
        assert_eq!(names(&read.doors), built);
        assert_eq!(read.confidence, Confidence::High);
    }

    /// …and the case the cross-family rule must NOT rescue: an ambiguous
    /// family beside a split one. A lit neighbour says the panel draws gold; it
    /// says nothing about which side of the dead span these fractions belong
    /// on, so the board is still refused.
    #[test]
    fn an_ambiguous_family_beside_a_split_one_still_refuses_the_board() {
        let lattice = Lattice::new((500, 600), 1.0);
        let mut img = RgbImage::from_pixel(1000, 700, image::Rgb([30, 26, 20]));
        let mut horizontals = 0usize;
        for edge in super::super::lattice::edges() {
            let horizontal = edge.kind() == Some(Corridor::Horizontal);
            if horizontal {
                horizontals += 1;
            }
            // The horizontals split on a real gap; every diagonal lands inside
            // the 0.15–0.22 dead span, all at the same value so that family has
            // no gap of its own either.
            let density = match (horizontal, horizontals.is_multiple_of(2)) {
                (true, false) => 0.478,
                (true, true) => 0.077,
                (false, _) => 0.18,
            };
            paint_corridor_for_test(&mut img, &lattice, edge, density);
        }
        let read = read_doors(&img, &lattice, None);
        assert_eq!(read.confidence, Confidence::Low);
    }

    /// Every corridor is judged against ITS family's threshold, not the
    /// board's.
    ///
    /// Painted at the measured extremes that make the difference visible: the
    /// horizontal family's closed corridors at the top of their measured range
    /// (0.077) against open ones at 0.478 calibrate horizontal at ~0.30, which
    /// is ABOVE every open diagonal (0.269 at the bottom of that family's
    /// measured range). Judging the diagonals against the horizontal threshold
    /// therefore closes every open one, and judging the horizontals against the
    /// diagonal threshold changes nothing — so the door set alone says which
    /// threshold each family got.
    #[test]
    fn each_corridor_is_judged_against_its_own_family_threshold() {
        let lattice = Lattice::new((500, 600), 1.0);
        let mut img = RgbImage::from_pixel(1000, 700, image::Rgb([30, 26, 20]));
        let mut expected: Vec<String> = Vec::new();
        let (mut seen_h, mut seen_d) = (0usize, 0usize);
        for edge in super::super::lattice::edges() {
            let horizontal = edge.kind() == Some(Corridor::Horizontal);
            // Every other corridor of each family built, so both calibrate off
            // two clusters of their own.
            let seen = if horizontal {
                seen_h += 1;
                seen_h
            } else {
                seen_d += 1;
                seen_d
            };
            let built = seen % 2 == 1;
            let density = match (horizontal, built) {
                (true, true) => 0.478,
                (true, false) => 0.077,
                (false, true) => 0.269,
                (false, false) => 0.0,
            };
            paint_corridor_for_test(&mut img, &lattice, edge, density);
            if built {
                expected.push(edge.to_string());
            }
        }
        expected.sort();
        let read = read_doors(&img, &lattice, None);
        assert_eq!(read.confidence, Confidence::High);
        assert!(
            (read.thresholds.horizontal - 0.3036).abs() < 0.005,
            "horizontal threshold {}",
            read.thresholds.horizontal
        );
        assert!(
            (read.thresholds.diagonal - 0.1429).abs() < 0.005,
            "diagonal threshold {}",
            read.thresholds.diagonal
        );
        assert_eq!(names(&read.doors), expected);
    }

    /// Every corridor touching the current room is flagged, and only those.
    #[test]
    fn edges_touching_the_current_room_are_flagged_uncertain() {
        let img = RgbImage::from_pixel(1000, 700, image::Rgb([30, 26, 20]));
        let read = read_doors(&img, &Lattice::new((500, 600), 1.0), Some(Slot::C1));
        let flagged: Vec<String> = read.uncertain.iter().map(|e| e.to_string()).collect();
        assert_eq!(
            flagged,
            vec!["B0-C1", "B1-C1", "C0-C1", "C1-C2", "C1-D1", "C1-D2"]
        );
    }

    /// A plate with no frame ring is not the current room.
    #[test]
    fn current_room_is_none_when_no_plate_is_framed() {
        let img = RgbImage::from_pixel(1000, 700, image::Rgb([30, 26, 20]));
        assert_eq!(current_room(&img, &Lattice::new((500, 600), 1.0)), None);
    }
}

/// Paint `edge`'s sample patch so it reads a gold fraction of about `density`,
/// for tests that synthesise a board. The gold rows are laid from the top of
/// the patch, so the fraction is `rows / (2 · hw)` rounded up.
#[cfg(test)]
pub fn paint_corridor_for_test(
    img: &mut RgbImage,
    lattice: &Lattice,
    edge: Edge,
    density: f32,
) {
    let hw = (PATCH_HALF * lattice.scale as f64) as i32;
    let (mx, my) = lattice.edge_midpoint(edge);
    let rows = (density * (2 * hw) as f32).ceil() as i32;
    for dy in 0..rows {
        for dx in 0..2 * hw {
            let (x, y) = (mx - hw + dx, my - hw + dy);
            if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
                img.put_pixel(x as u32, y as u32, image::Rgb([214, 176, 66]));
            }
        }
    }
}
