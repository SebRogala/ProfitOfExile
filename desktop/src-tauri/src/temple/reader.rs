//! The layout reader's one entry point (POE-168).
//!
//! [`read_layout`] is pure: a screenshot in, a board out, no Tauri, no state,
//! no Windows API. POE-143's capture layer and POE-169's room-identity pass
//! are the intended callers.

// POE-168's whole surface is reached only by its own tests until POE-143
// (capture) and POE-169 (room identity) call `reader::read_layout`. Unlike
// `strategy`, which carries a per-item `#[allow(dead_code)]` because its items
// are claimed piecemeal by two different consumers, this is one unit with one
// root: marking the root alone silences nothing, because rustc walks
// reachability from live code. The allow is therefore per file, and it comes
// off in one edit when the first caller lands.
#![allow(dead_code)]

use std::collections::BTreeSet;

use image::DynamicImage;

use super::anchor::{self, Anchor, AnchorCalibration};
use super::doors::{self, Confidence, Thresholds};
use super::lattice::{Edge, Lattice, Slot};
use super::ReadError;

/// Everything one screenshot of the layout panel yields.
///
/// `doors` and `uncertain` overlap on purpose — see [`super::doors`]. A
/// consumer that wants only corridors it can act on takes
/// `doors.difference(&uncertain)`; POE-169 resolves the rest from the side
/// panel's door markers.
#[derive(Debug, Clone, PartialEq)]
pub struct TempleLayout {
    /// Entrance plate centre in image px — the origin every other coordinate
    /// here is derived from.
    pub origin: (i32, i32),
    /// Image px per reference px.
    pub scale: f32,
    /// The anchor's full-resolution NCC score. Always ≥
    /// [`anchor::NCC_FLOOR`], or there is no layout.
    pub ncc: f32,
    /// Whether the corridor fractions split into two clusters. [`Confidence::Low`]
    /// means the door sets are a best effort over an unreadable panel.
    pub confidence: Confidence,
    /// The room the player is standing in, or `None` when the panel was opened
    /// between rooms.
    pub current: Option<Slot>,
    /// Corridors judged open.
    pub doors: BTreeSet<Edge>,
    /// Corridors whose patch the current room's selection frame covers.
    pub uncertain: BTreeSet<Edge>,
    /// Plate centres in image px, indexed by [`Slot::index`].
    pub slots: [(i32, i32); 13],
    /// What each corridor family's clean edges calibrated.
    pub thresholds: Thresholds,
    /// The scale, keyed on this capture's dimensions, for POE-171 to persist.
    pub calibration: AnchorCalibration,
}

/// Read a board from a capture of the **whole game window** with the temple's
/// layout panel open.
///
/// Not a crop of the panel: the anchor's first scale guess is
/// `img.width() / anchor::REFERENCE_SCREEN_WIDTH`, so the capture's width has
/// to be the game window's width for that seed to mean anything. A panel-sized
/// crop seeds far too low, falls through to the (much slower) full sweep, and
/// on a narrow enough crop finds nothing at all. POE-143's capture layer is the
/// intended caller and captures the window.
pub fn read_layout(img: &DynamicImage) -> Result<TempleLayout, ReadError> {
    read_layout_with_hint(img, None)
}

/// Read a board, trying a remembered scale before sweeping.
///
/// The hint is a speed-up only: it is verified against
/// [`anchor::NCC_FLOOR`] like any other candidate, so a stale one costs a
/// single extra match and never a wrong board.
pub fn read_layout_with_hint(
    img: &DynamicImage,
    hint: Option<&AnchorCalibration>,
) -> Result<TempleLayout, ReadError> {
    let found = anchor::anchor_with_hint(img, hint)?;
    let Anchor { origin, scale, ncc } = found;
    let lattice = Lattice::new(origin, scale);
    let rgb = img.to_rgb8();
    let current = doors::current_room(&rgb, &lattice);
    let read = doors::read_doors(&rgb, &lattice, current);
    Ok(TempleLayout {
        origin,
        scale,
        ncc,
        confidence: read.confidence,
        current,
        doors: read.doors,
        uncertain: read.uncertain,
        slots: lattice.centres,
        thresholds: read.thresholds,
        calibration: AnchorCalibration::of(img, &found),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::anchor::NCC_FLOOR;
    use crate::temple::doors::{frame_gold, CURRENT_ROOM_GOLD};
    use image::{Rgb, RgbImage};

    /// A committed board, plus everything the Python prototype
    /// (`temple.py`, session e821f349) read off that exact file.
    ///
    /// Both fixtures are **vertical bands** of their source screenshot, cut at
    /// full width on purpose: the scale sweep is seeded from `image_width /
    /// REFERENCE_SCREEN_WIDTH`, so narrowing a fixture horizontally would
    /// silently move it onto the widened-sweep fallback and stop testing the
    /// path the app actually runs. Both were requantised to a 256-colour
    /// palette to fit in the repository; the prototype reads identical scale,
    /// origin, current room, doors and uncertain sets before and after that
    /// step (only the NCC moves, 0.968 → 0.960 and 0.942 → 0.936).
    struct Fixture {
        file: &'static str,
        /// Source screenshot and the crop box `(x0, y0, x1, y1)` taken from it.
        source: &'static str,
        crop: (u32, u32, u32, u32),
        size: (u32, u32),
        scale: f32,
        ncc: f32,
        origin: (i32, i32),
        current: Slot,
        doors: &'static [&'static str],
        uncertain: &'static [&'static str],
        /// What each corridor family's clean edges calibrate on this board.
        /// Pinned per family, and apart, because a reader that judged both
        /// families against one number would return these same door sets: the
        /// two thresholds are the only place the per-family split is visible
        /// on a board whose corridors all sit clear of both.
        thresholds: Thresholds,
    }

    /// The reference scale family: 1374 px window, scale 0.99, standing in
    /// Tombs (D3). This is the board `TEMPLE-CORE-RULES.md` Case 1 was
    /// hand-encoded from; the reader's only difference from that hand encoding
    /// is `C2-D3`, which it self-flags as uncertain.
    const REF: Fixture = Fixture {
        file: "board-ref-1374.png",
        source: "tmp/alva-screenshots/2026-08-02_22-22-38.png",
        crop: (0, 188, 1374, 730),
        size: (1374, 542),
        scale: 0.99,
        ncc: 0.9603,
        origin: (673, 494),
        current: Slot::D3,
        doors: &[
            "A0-B0", "B0-B1", "B1-C1", "C0-C1", "C0-D0", "C0-D1", "C2-D3", "D1-E1", "D2-E2",
            "E1-E2",
        ],
        uncertain: &["C2-D3", "D2-D3", "D3-E2"],
        thresholds: Thresholds {
            horizontal: 0.2389,
            diagonal: 0.1775,
        },
    };

    /// The board that produced the anchor bug: 1539 px, true scale 1.13, where
    /// a coarse-only winner picked 1.09 and opened the closed Apex corridor.
    /// `A0-B0` must come back flagged and NOT in `doors`.
    const LIVE: Fixture = Fixture {
        file: "board-live-1539.png",
        source: "Screenshots/2026-08-07_19-28-36.png",
        crop: (0, 207, 1539, 820),
        size: (1539, 613),
        scale: 1.13,
        ncc: 0.9355,
        origin: (745, 561),
        current: Slot::B0,
        doors: &[
            "B0-C0", "B1-C2", "C0-C1", "C0-D1", "C1-D2", "C2-D2", "C2-D3", "D0-D1", "D0-E0",
            "D1-D2", "D1-E0", "D3-E2", "E0-E1",
        ],
        uncertain: &["A0-B0", "B0-B1", "B0-C0", "B0-C1"],
        thresholds: Thresholds {
            horizontal: 0.2233,
            diagonal: 0.1600,
        },
    };

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

    fn names(edges: &BTreeSet<Edge>) -> Vec<String> {
        edges.iter().map(|e| e.to_string()).collect()
    }

    /// Both corridor sets on both scale families. `uncertain` is asserted
    /// alongside `doors` deliberately: a regression that stops flagging the
    /// current room's edges leaves `doors` intact on these boards and would
    /// otherwise pass.
    fn assert_reads_like_the_prototype(f: &Fixture) {
        let layout = read_layout(&load(f)).expect("the fixture anchors");
        assert!(
            (layout.scale - f.scale).abs() <= 0.02,
            "{}: recovered scale {} is not within 0.02 of {}",
            f.file,
            layout.scale,
            f.scale
        );
        assert!(
            layout.ncc >= NCC_FLOOR,
            "{}: NCC {} fell below the floor {NCC_FLOOR} (prototype read {})",
            f.file,
            layout.ncc,
            f.ncc
        );
        assert!(
            (layout.origin.0 - f.origin.0).abs() <= 3
                && (layout.origin.1 - f.origin.1).abs() <= 3,
            "{}: origin {:?} is more than 3 px from {:?}",
            f.file,
            layout.origin,
            f.origin
        );
        assert_eq!(layout.current, Some(f.current), "{}: current room", f.file);
        assert_eq!(names(&layout.doors), f.doors, "{}: open corridors", f.file);
        assert_eq!(
            names(&layout.uncertain),
            f.uncertain,
            "{}: corridors the selection frame covers",
            f.file
        );
        assert_eq!(
            layout.confidence,
            Confidence::High,
            "{}: a real board separates into two clusters",
            f.file
        );
        let (got, want) = (&layout.thresholds, &f.thresholds);
        for (family, got, want) in [
            ("horizontal", got.horizontal, want.horizontal),
            ("diagonal", got.diagonal, want.diagonal),
        ] {
            assert!(
                (got - want).abs() <= 0.005,
                "{}: the {family} family calibrated {got}, not {want}",
                f.file
            );
        }
        assert!(
            f.thresholds.horizontal - f.thresholds.diagonal > 0.05,
            "{}: the two families are pinned too close together to prove they \
             calibrated apart",
            f.file
        );
    }

    #[test]
    fn reference_board_reads_like_the_prototype() {
        assert_reads_like_the_prototype(&REF);
    }

    #[test]
    fn live_board_reads_like_the_prototype() {
        assert_reads_like_the_prototype(&LIVE);
    }

    /// The regression the coarse-to-fine split exists for, asserted on both
    /// halves: the ÷4 pass really does prefer 1.09 here (0.9655 vs 0.9581),
    /// and the reader really does return 1.13 anyway. Only 1.13 places the
    /// geometry well enough to keep the Apex corridor closed, which is what
    /// the coarse-only reader got wrong on this board.
    #[test]
    fn the_fine_pass_overrides_the_coarse_ranking_on_the_1539px_board() {
        let img = load(&LIVE);
        // Two scales is enough to reproduce the mis-ranking and keeps the
        // extra sweep cheap; coarse scores do not depend on the rest of the
        // grid.
        let ranking = crate::temple::anchor::coarse_ranking_for_test(&img, &[1.09, 1.13]);
        assert_eq!(
            ranking.first().map(|&(_, scale)| scale),
            Some(1.09),
            "the coarse pass is expected to mis-rank this board; ranking {ranking:?}"
        );

        let layout = read_layout(&img).expect("the fixture anchors");
        assert!(
            (layout.scale - 1.13).abs() <= 0.02,
            "recovered scale {} — the coarse pass's own pick is 1.09",
            layout.scale
        );
        let apex = Edge::new(Slot::APEX, Slot::B0);
        assert!(
            !layout.doors.contains(&apex),
            "the Apex corridor is closed on this board"
        );
        assert!(
            layout.uncertain.contains(&apex),
            "it touches the current room, so it must be flagged"
        );
    }

    /// A remembered scale that is wrong for this capture is discarded, not
    /// believed: the reader falls back and still recovers the true scale.
    #[test]
    fn a_stale_calibration_is_discarded_and_the_true_scale_recovered() {
        let img = load(&LIVE);
        let stale = AnchorCalibration {
            screen_w: LIVE.size.0,
            screen_h: LIVE.size.1,
            scale: 1.00,
        };
        let layout = read_layout_with_hint(&img, Some(&stale)).expect("the fixture anchors");
        assert!(
            (layout.scale - 1.13).abs() <= 0.02,
            "recovered scale {} after a stale 1.00 hint",
            layout.scale
        );
        assert_eq!(layout.current, Some(Slot::B0));
        assert_eq!(layout.calibration.scale, layout.scale);
    }

    /// A correct calibration reproduces the sweep's own answer.
    #[test]
    fn a_matching_calibration_reproduces_the_swept_result() {
        let img = load(&LIVE);
        let swept = read_layout(&img).expect("the fixture anchors");
        let hinted = read_layout_with_hint(&img, Some(&swept.calibration)).expect("anchors");
        assert_eq!(hinted, swept);
    }

    /// The selection frame is picked out by a margin, not by being the only
    /// gold on the panel: every plate's ring carries some. Anchoring is
    /// skipped here — the lattice is built straight from the recorded
    /// ground truth — so the assertion is about the ring test alone.
    #[test]
    fn the_selection_frame_outscores_every_other_plate_by_a_margin() {
        for f in [&REF, &LIVE] {
            let rgb = load(f).to_rgb8();
            let lattice = Lattice::new(f.origin, f.scale);
            let mut scores: Vec<(Slot, f32)> = Slot::ALL
                .into_iter()
                .map(|s| (s, frame_gold(&rgb, &lattice, s)))
                .collect();
            scores.sort_by(|a, b| b.1.total_cmp(&a.1));
            assert_eq!(scores[0].0, f.current, "{}: brightest ring", f.file);
            assert!(
                scores[0].1 > CURRENT_ROOM_GOLD,
                "{}: the framed plate scored {}",
                f.file,
                scores[0].1
            );
            assert!(
                scores[1].1 > 0.0 && scores[1].1 < CURRENT_ROOM_GOLD,
                "{}: the brightest unframed plate scored {}, which must be \
                 nonzero (so a zero threshold is wrong) and below the \
                 threshold (so this one is right)",
                f.file,
                scores[1].1
            );
        }
    }

    /// No Entrance plate anywhere: an error, never a coordinate. A reader that
    /// returns its best guess here puts an invented board on screen.
    #[test]
    fn an_image_without_an_entrance_plate_is_an_error() {
        // Deterministic noise — flat colour would be rejected by the zero-
        // variance guard alone and would not exercise the floor.
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut img = RgbImage::new(480, 360);
        for p in img.pixels_mut() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let b = (seed >> 33) as u8;
            *p = Rgb([b, b.wrapping_add(40), b.wrapping_sub(30)]);
        }
        match read_layout(&DynamicImage::ImageRgb8(img)) {
            Err(ReadError::AnchorNotFound { best_ncc }) => assert!(
                best_ncc < NCC_FLOOR,
                "reported {best_ncc}, which is not below the floor"
            ),
            other => panic!("expected AnchorNotFound, got {other:?}"),
        }
    }

    /// The occlusion guard end to end: a panel whose corridors all read into
    /// the dead span between the two band constants anchors fine but says
    /// nothing about which corridors are open, so the layout comes back
    /// low-confidence rather than confidently wrong.
    ///
    /// The diagonal weave is what a half-faded or overdrawn panel looks like to
    /// the gold test: every patch lands near 0.18 whatever the recovered scale
    /// rounds the patch size to.
    #[test]
    fn a_panel_whose_corridors_are_all_ambiguous_reads_low_confidence() {
        let mut img = RgbImage::from_pixel(1374, 560, Rgb([30, 26, 20]));
        for (x, y, p) in img.enumerate_pixels_mut() {
            if (x + y) % 11 < 2 {
                *p = Rgb([214, 176, 66]);
            }
        }
        let plate = crate::temple::anchor::template_for_test();
        // Pasted last, so the anchor has undamaged art to match on.
        image::imageops::replace(&mut img, plate, 595, 460);
        let layout = read_layout(&DynamicImage::ImageRgb8(img)).expect("the plate anchors");
        assert!((layout.scale - 1.0).abs() <= 0.02, "scale {}", layout.scale);
        assert_eq!(layout.confidence, Confidence::Low);
        assert_eq!(layout.current, None);
    }

    /// A uniformly dark panel is refused too, and this is the end-to-end shape
    /// of it: the NCC match is mean-subtracted and variance-normalised, so a
    /// dimmed or faded panel anchors as well as a bright one while reading no
    /// gold at all. This is the ONLY arrangement in which an all-low family is
    /// refused — no gold anywhere on the panel, so neither family can vouch for
    /// the other. A bare family beside a lit one reads as a board; see
    /// `doors::a_bare_family_beside_a_split_one_reads_closed_and_the_board_reads`.
    #[test]
    fn a_panel_with_no_corridor_gold_is_refused_rather_than_read_as_closed() {
        let mut img = RgbImage::from_pixel(1374, 560, Rgb([30, 26, 20]));
        let plate = crate::temple::anchor::template_for_test();
        image::imageops::replace(&mut img, plate, 595, 460);
        let layout = read_layout(&DynamicImage::ImageRgb8(img)).expect("the plate anchors");
        assert!(layout.ncc >= NCC_FLOOR, "the dimmed panel still anchors");
        assert!(layout.doors.is_empty(), "open corridors {:?}", names(&layout.doors));
        assert_eq!(layout.confidence, Confidence::Low);
        assert_eq!(layout.current, None);
    }
}
