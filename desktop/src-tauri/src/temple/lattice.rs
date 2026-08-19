//! The temple board's rigid 13-slot lattice (POE-168).
//!
//! Every coordinate on the layout panel is a fixed multiple of
//! `(entrance centre, scale)` — see `docs` note in [`super`]. This module owns
//! that arithmetic and nothing else: it never touches pixels, so all of it is
//! exercised by fixture-free unit tests.
//!
//! # Measured constants
//!
//! Taken from 8 boards on 2 machines (2026-08-05..07) and re-checked by the
//! `temple.py` prototype on 11 live boards:
//!
//! | quantity | reference px |
//! |---|---|
//! | column pitch | 212 |
//! | row pitch | 105 |
//! | plate (incl. border) | 173 × 84 |
//! | Entrance drop below its row-E siblings | 19 |
//!
//! Row x offsets from the Entrance centre: A `0` · B `±106` · C `−212, 0, +212`
//! · D `−318, −106, +106, +318` · E `−212, 0, +212`.

// POE-171 is that caller: `temple::run` and `temple::slice` reach this module
// on every read, so the file-level `#![allow(dead_code)]` is gone. What is
// still uncalled carries its own attribute, which is now the inventory of what
// only the tests reach rather than a blanket over the whole file.

/// Horizontal distance between two slots in the same row, reference px.
pub const COL_PITCH: f64 = 212.0;
/// Vertical distance between two rows, reference px.
pub const ROW_PITCH: f64 = 105.0;
/// Plate width including its border, reference px.
pub const PLATE_W: f64 = 173.0;
/// Plate height including its border, reference px.
pub const PLATE_H: f64 = 84.0;
/// The Entrance plate sits this much lower than the other two row-E plates.
pub const ENTRANCE_DROP: f64 = 19.0;
/// Half-size of the square patch sampled at a corridor midpoint, reference px.
pub const PATCH_HALF: f64 = 14.0;

/// One of the 13 board positions.
///
/// The discriminant order is the prototype's key order (`A0`, `B0`, `B1`, …),
/// which is also alphabetical, so the derived [`Ord`] makes a
/// `BTreeSet<Edge>` print in the same order the Python reader does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    A0,
    B0,
    B1,
    C0,
    C1,
    C2,
    D0,
    D1,
    D2,
    D3,
    E0,
    E1,
    E2,
}

impl Slot {
    /// Every slot, in [`Ord`] order. Index into this is the slot's index into
    /// [`Lattice::centres`].
    pub const ALL: [Slot; 13] = [
        Slot::A0,
        Slot::B0,
        Slot::B1,
        Slot::C0,
        Slot::C1,
        Slot::C2,
        Slot::D0,
        Slot::D1,
        Slot::D2,
        Slot::D3,
        Slot::E0,
        Slot::E1,
        Slot::E2,
    ];

    /// The Apex of Atzoatl — the single row-A slot.
    pub const APEX: Slot = Slot::A0;
    /// The Entrance — the middle row-E slot, and the anchor of the geometry.
    pub const ENTRANCE: Slot = Slot::E1;

    /// Position in [`Slot::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// `"A0"`, `"B0"`, … — the prototype's key, and what POE-169/170 log.
    pub fn as_str(self) -> &'static str {
        match self {
            Slot::A0 => "A0",
            Slot::B0 => "B0",
            Slot::B1 => "B1",
            Slot::C0 => "C0",
            Slot::C1 => "C1",
            Slot::C2 => "C2",
            Slot::D0 => "D0",
            Slot::D1 => "D1",
            Slot::D2 => "D2",
            Slot::D3 => "D3",
            Slot::E0 => "E0",
            Slot::E1 => "E1",
            Slot::E2 => "E2",
        }
    }

    /// Row index, `0` for the Apex's row A down to `4` for the Entrance's row E.
    ///
    /// R1's "connect toward the top" is a **gradient, not a binary** — row C
    /// beats row D even when neither is row B — so the advisor needs the row as
    /// a number, not as a letter.
    pub fn row(self) -> u8 {
        match self {
            Slot::A0 => 0,
            Slot::B0 | Slot::B1 => 1,
            Slot::C0 | Slot::C1 | Slot::C2 => 2,
            Slot::D0 | Slot::D1 | Slot::D2 | Slot::D3 => 3,
            Slot::E0 | Slot::E1 | Slot::E2 => 4,
        }
    }

    /// Offset of this slot's centre from the Entrance centre, in reference px,
    /// before scaling. `+y` is down, matching image coordinates.
    fn offset(self) -> (f64, f64) {
        // Rows A..E are 4..0 row pitches above the row-E line, and the row-E
        // line itself is ENTRANCE_DROP above the Entrance centre.
        const fn row_dy(rows_above_e: f64) -> f64 {
            -(ENTRANCE_DROP + rows_above_e * ROW_PITCH)
        }
        match self {
            Slot::A0 => (0.0, row_dy(4.0)),
            Slot::B0 => (-106.0, row_dy(3.0)),
            Slot::B1 => (106.0, row_dy(3.0)),
            Slot::C0 => (-212.0, row_dy(2.0)),
            Slot::C1 => (0.0, row_dy(2.0)),
            Slot::C2 => (212.0, row_dy(2.0)),
            Slot::D0 => (-318.0, row_dy(1.0)),
            Slot::D1 => (-106.0, row_dy(1.0)),
            Slot::D2 => (106.0, row_dy(1.0)),
            Slot::D3 => (318.0, row_dy(1.0)),
            Slot::E0 => (-212.0, row_dy(0.0)),
            // The Entrance is the origin; its two siblings sit ENTRANCE_DROP
            // higher, which `row_dy(0.0)` encodes.
            Slot::E1 => (0.0, 0.0),
            Slot::E2 => (212.0, row_dy(0.0)),
        }
    }
}

/// Which of the two corridor geometries an edge is drawn as.
///
/// The distinction is not cosmetic: the plate gap a horizontal corridor spans
/// is `COL_PITCH − PLATE_W` = 39 reference px, while a diagonal one spans a
/// longer, slanted gap. The beam art fills the short gap far more densely, so
/// an open horizontal corridor reads a much higher gold fraction than an open
/// diagonal one and the two are calibrated as separate populations — see
/// [`super::doors::read_doors`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Corridor {
    /// Same row, `COL_PITCH` apart.
    Horizontal,
    /// Adjacent rows, `COL_PITCH / 2` apart.
    Diagonal,
}

/// An undirected corridor between two slots.
///
/// Constructed only through [`Edge::new`], which orders the pair, so
/// `Edge::new(a, b) == Edge::new(b, a)` and the derived [`Ord`] is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    lo: Slot,
    hi: Slot,
}

impl Edge {
    /// The corridor joining `a` and `b`.
    ///
    /// # Panics
    ///
    /// If `a == b` — a slot has no corridor to itself, and silently returning
    /// a degenerate edge would put one in the door set.
    pub fn new(a: Slot, b: Slot) -> Edge {
        assert!(a != b, "an edge needs two distinct slots, got {a:?} twice");
        if a < b {
            Edge { lo: a, hi: b }
        } else {
            Edge { lo: b, hi: a }
        }
    }

    /// The two endpoints, lower [`Ord`] first.
    pub fn ends(self) -> (Slot, Slot) {
        (self.lo, self.hi)
    }

    /// Whether `slot` is one of the endpoints.
    pub fn touches(self, slot: Slot) -> bool {
        self.lo == slot || self.hi == slot
    }

    /// Which corridor family this pair forms, or `None` when the two slots are
    /// not adjacent on the board and so no corridor can ever join them.
    ///
    /// This is the derivation rule [`edges`] enumerates with, so the family and
    /// the edge set can never disagree.
    pub fn kind(self) -> Option<Corridor> {
        let (ax, ay) = self.lo.offset();
        let (bx, by) = self.hi.offset();
        let (dx, dy) = ((ax - bx).abs(), (ay - by).abs());
        if dx == COL_PITCH && dy <= ENTRANCE_DROP {
            // The tolerance is what lets `E0-E1` and `E1-E2` survive the
            // Entrance drop.
            Some(Corridor::Horizontal)
        } else if dx == COL_PITCH / 2.0 && (dy - ROW_PITCH).abs() <= ENTRANCE_DROP {
            Some(Corridor::Diagonal)
        } else {
            None
        }
    }
}

impl std::fmt::Display for Edge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.lo.as_str(), self.hi.as_str())
    }
}

/// The 26 geometrically possible corridors.
///
/// Derived from the slot offsets by [`Edge::kind`]'s rule rather than typed in,
/// so a wrong offset shows up as a wrong edge set instead of hiding behind a
/// hand-maintained list.
pub fn edges() -> Vec<Edge> {
    let mut out = Vec::new();
    for (i, &a) in Slot::ALL.iter().enumerate() {
        for &b in &Slot::ALL[i + 1..] {
            let edge = Edge::new(a, b);
            if edge.kind().is_some() {
                out.push(edge);
            }
        }
    }
    out.sort_unstable();
    out
}

/// The slots reachable from `slot` by one corridor.
pub fn neighbours(slot: Slot) -> Vec<Slot> {
    edges()
        .into_iter()
        .filter_map(|e| match e.ends() {
            (a, b) if a == slot => Some(b),
            (a, b) if b == slot => Some(a),
            _ => None,
        })
        .collect()
}

/// The board's 13 plate centres in image pixels, plus the scale they were
/// built at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lattice {
    /// Entrance plate centre in image px — the origin the whole board hangs off.
    pub origin: (i32, i32),
    /// Image px per reference px.
    pub scale: f32,
    /// Plate centres, indexed by [`Slot::index`].
    pub centres: [(i32, i32); 13],
}

impl Lattice {
    /// Build the lattice from an anchored Entrance centre and a scale.
    pub fn new(origin: (i32, i32), scale: f32) -> Lattice {
        let s = scale as f64;
        let (ex, ey) = (origin.0 as f64, origin.1 as f64);
        let mut centres = [(0, 0); 13];
        for &slot in &Slot::ALL {
            let (dx, dy) = slot.offset();
            centres[slot.index()] = ((ex + dx * s).round() as i32, (ey + dy * s).round() as i32);
        }
        Lattice {
            origin,
            scale,
            centres,
        }
    }

    /// Centre of `slot`'s plate, image px.
    pub fn centre(&self, slot: Slot) -> (i32, i32) {
        self.centres[slot.index()]
    }

    /// Midpoint of a corridor — where the door patch is sampled.
    pub fn edge_midpoint(&self, edge: Edge) -> (i32, i32) {
        let (a, b) = edge.ends();
        let (ax, ay) = self.centre(a);
        let (bx, by) = self.centre(b);
        // Integer halving, matching the prototype's `(ax + bx) // 2`.
        (
            (ax + bx).div_euclid(2),
            (ay + by).div_euclid(2),
        )
    }

    /// Half-width and half-height of a plate at this scale, image px.
    pub fn plate_half(&self) -> (i32, i32) {
        let s = self.scale as f64;
        (
            (PLATE_W * s / 2.0) as i32,
            (PLATE_H * s / 2.0) as i32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every slot the panel can show, once.
    #[test]
    fn lattice_has_thirteen_distinct_slots() {
        let unique: BTreeSet<Slot> = Slot::ALL.into_iter().collect();
        assert_eq!(unique.len(), 13);
        let lattice = Lattice::new((673, 682), 1.0);
        let centres: BTreeSet<(i32, i32)> = lattice.centres.into_iter().collect();
        assert_eq!(centres.len(), 13, "two slots landed on the same centre");
    }

    /// The 26 corridors, by name. Typed out here on purpose: this is the one
    /// place the derivation rule is checked against the measured board rather
    /// than against itself.
    #[test]
    fn edge_derivation_yields_the_twenty_six_measured_corridors() {
        let got: Vec<String> = edges().iter().map(|e| e.to_string()).collect();
        assert_eq!(
            got,
            vec![
                "A0-B0", "A0-B1", "B0-B1", "B0-C0", "B0-C1", "B1-C1", "B1-C2", "C0-C1", "C0-D0",
                "C0-D1", "C1-C2", "C1-D1", "C1-D2", "C2-D2", "C2-D3", "D0-D1", "D0-E0", "D1-D2",
                "D1-E0", "D1-E1", "D2-D3", "D2-E1", "D2-E2", "D3-E2", "E0-E1", "E1-E2",
            ]
        );
    }

    /// Per-slot degree — the scarcity input POE-170's rule RS ranks doors by,
    /// so a wrong degree is a wrong recommendation, not a cosmetic slip.
    #[test]
    fn slot_degrees_match_the_measured_board() {
        let expected = [
            (Slot::A0, 2),
            (Slot::B0, 4),
            (Slot::B1, 4),
            (Slot::C0, 4),
            (Slot::C1, 6),
            (Slot::C2, 4),
            (Slot::D0, 3),
            (Slot::D1, 6),
            (Slot::D2, 6),
            (Slot::D3, 3),
            (Slot::E0, 3),
            (Slot::E1, 4),
            (Slot::E2, 3),
        ];
        for (slot, degree) in expected {
            assert_eq!(
                neighbours(slot).len(),
                degree,
                "{} has the wrong number of corridors",
                slot.as_str()
            );
        }
    }

    /// The Entrance drop: E1 sits 19 reference px below E0 and E2.
    #[test]
    fn entrance_sits_nineteen_px_below_its_row_siblings() {
        let lattice = Lattice::new((673, 682), 1.0);
        let entrance_y = lattice.centre(Slot::ENTRANCE).1;
        assert_eq!(entrance_y - lattice.centre(Slot::E0).1, 19);
        assert_eq!(entrance_y - lattice.centre(Slot::E2).1, 19);
    }

    /// …and the drop scales with the rest of the board.
    #[test]
    fn entrance_drop_scales_with_the_board() {
        let lattice = Lattice::new((745, 768), 1.13);
        let entrance_y = lattice.centre(Slot::ENTRANCE).1;
        // 19 * 1.13 = 21.47 -> 21 px.
        assert_eq!(entrance_y - lattice.centre(Slot::E0).1, 21);
    }

    /// Scaling multiplies both pitches, measured between the extreme slots so
    /// a single-axis mistake cannot hide.
    #[test]
    fn scaling_multiplies_column_and_row_pitch() {
        let one = Lattice::new((673, 682), 1.0);
        let big = Lattice::new((673, 682), 2.0);
        let span = |l: &Lattice| {
            (
                l.centre(Slot::D3).0 - l.centre(Slot::D0).0,
                l.centre(Slot::E1).1 - l.centre(Slot::A0).1,
            )
        };
        assert_eq!(span(&one), (636, 439));
        assert_eq!(span(&big), (1272, 878));
    }

    /// The board is built relative to the anchor: move the anchor, every plate
    /// moves by the same vector.
    #[test]
    fn moving_the_origin_translates_every_slot() {
        let a = Lattice::new((673, 682), 1.0);
        let b = Lattice::new((700, 600), 1.0);
        for &slot in &Slot::ALL {
            let (ax, ay) = a.centre(slot);
            let (bx, by) = b.centre(slot);
            assert_eq!((bx - ax, by - ay), (27, -82), "{}", slot.as_str());
        }
    }

    /// A corridor's sample point is the midpoint of the two plate centres.
    #[test]
    fn edge_midpoint_sits_between_its_two_plates() {
        let lattice = Lattice::new((673, 682), 1.0);
        let (ax, ay) = lattice.centre(Slot::E0);
        let (bx, by) = lattice.centre(Slot::E1);
        assert_eq!(
            lattice.edge_midpoint(Edge::new(Slot::E1, Slot::E0)),
            ((ax + bx) / 2, (ay + by) / 2)
        );
    }

    /// Edges are undirected: the pair is ordered on construction.
    #[test]
    fn edge_is_undirected() {
        assert_eq!(Edge::new(Slot::E1, Slot::A0), Edge::new(Slot::A0, Slot::E1));
        assert_eq!(Edge::new(Slot::E1, Slot::A0).to_string(), "A0-E1");
    }

    /// A self-edge is a bug in the caller, not a corridor.
    #[test]
    #[should_panic(expected = "two distinct slots")]
    fn edge_rejects_a_self_loop() {
        Edge::new(Slot::C1, Slot::C1);
    }
}
