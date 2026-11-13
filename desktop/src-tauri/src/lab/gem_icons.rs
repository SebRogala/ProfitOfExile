//! Icon matching for the Divine Font's three gem-choice slots (POE-397): each
//! slot is cropped from the press frame, resized to the canonical icon size and
//! compared with every reference icon of both gem sets. Pure: no I/O, no state.
//!
//! Method (the POE-397 spike's `simB.classify`): sRGB → CIE Lab, a reference's
//! pixels are those with alpha > 0.9, the reference is placed on the slot's
//! foreground centroid and searched ±`SHIFT_PX`, the score is the RMSE of the
//! Lab distance. A coarse pass on a subsample of each reference picks the best
//! `FINE_TOP` per set, which a full pass then ranks.

use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};

/// Reference icons and slot crops are compared at this size: the icon size the
/// slots measured on the 1080p frame (`crate::GEM_SLOT_ICON_REF`).
pub(crate) const CANONICAL_PX: u32 = 48;
/// Context kept around the slot square, at canonical size, so the shift search
/// compares real frame pixels instead of clamped edges.
const MARGIN_PX: u32 = 12;
const CROP_PX: u32 = CANONICAL_PX + 2 * MARGIN_PX;
const SHIFT_PX: i32 = 4;
/// Points per reference in the coarse pass.
const COARSE_POINTS: usize = 350;
/// References per set the fine pass ranks.
const FINE_TOP: usize = 25;
/// Candidates an uncertain slot reports.
const TOP_CANDIDATES: usize = 3;
/// A slot is certain when its second-best reference (in the chosen set) scores
/// at least this many times its best (C2).
pub(crate) const UNCERTAIN_MARGIN: f32 = 1.1;

/// The presence gate: above this sum of the three slots' best RMSE (chosen
/// set, 48 px) the frame holds no gem panel and the reader writes nothing.
///
/// **PROVISIONAL**, measured 2026-11-13 on the POE-397 corpus (frames
/// 0000-0012; the corpus has no options-screen frame, so that negative is
/// still the open in-game check):
///
/// | Frame / region | Joint best RMSE |
/// |---|---|
/// | 0012 gem screen (3 correct, transfigured) | 53.9 spike / 54.1 this code (4K-derived 50.9, 16:10-derived 57.4) |
/// | 0000-0011 gameplay at the slot positions | 128.1-146.2 |
/// | 0012 panel chrome above the slots | 146.9 spike / 148.3 this code |
/// | 0012 CONFIRM / CRAFT buttons | 131.5 |
/// | 0000 quest-tracker text on dark | 145.6 |
///
/// 85 is close to the geometric mean of 53.9 and 128.1.
pub(crate) const GEM_PANEL_JOINT_RMSE_MAX: f32 = 85.0;

/// Which half of the gem dictionary a Font frame shows. A frame never mixes
/// them (docs/GAME-FACTS.md § Divine Font).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GemSet {
    Normal,
    Transfigured,
}

/// One reference icon at canonical size, Lab-converted.
pub(crate) struct IconRef {
    pub name: String,
    /// (y, x, Lab) of every pixel with alpha > 0.9.
    points: Vec<(i32, i32, [f32; 3])>,
    cy: f32,
    cx: f32,
}

impl IconRef {
    /// `None` when no pixel is opaque enough to compare.
    pub(crate) fn new(name: &str, art: &RgbaImage) -> Option<Self> {
        let icon = image::imageops::resize(art, CANONICAL_PX, CANONICAL_PX, FilterType::Lanczos3);
        let points: Vec<_> = icon
            .enumerate_pixels()
            .filter(|(_, _, p)| p[3] as f32 / 255.0 > 0.9)
            .map(|(x, y, p)| (y as i32, x as i32, lab([p[0], p[1], p[2]])))
            .collect();
        if points.is_empty() {
            return None;
        }
        let n = points.len() as f32;
        let cy = points.iter().map(|p| p.0 as f32).sum::<f32>() / n;
        let cx = points.iter().map(|p| p.1 as f32).sum::<f32>() / n;
        Some(Self { name: name.to_string(), points, cy, cx })
    }
}

/// The reference icons of both sets.
pub(crate) struct GemIconRefs {
    pub normal: Vec<IconRef>,
    pub transfigured: Vec<IconRef>,
    /// Some eligible art failed to load (not a 404): the true gem may be the
    /// missing one, so no icon slot is reported certain.
    pub incomplete: bool,
}

impl GemIconRefs {
    fn set(&self, set: GemSet) -> &[IconRef] {
        match set {
            GemSet::Normal => &self.normal,
            GemSet::Transfigured => &self.transfigured,
        }
    }
}

/// One slot crop at canonical scale, Lab-converted, with its foreground
/// centroid.
pub(crate) struct SlotImage {
    lab: Vec<[f32; 3]>,
    cy: f32,
    cx: f32,
}

impl SlotImage {
    /// Crop `rect` (capture px) plus its margin from `frame` and resize it so
    /// the slot square is `CANONICAL_PX`.
    pub(crate) fn from_frame(frame: &DynamicImage, rect: [i32; 4]) -> Self {
        let margin = (rect[2] as f32 * MARGIN_PX as f32 / CANONICAL_PX as f32).round() as i32;
        let (x, y) = ((rect[0] - margin).max(0) as u32, (rect[1] - margin).max(0) as u32);
        let (w, h) = ((rect[2] + 2 * margin).max(1) as u32, (rect[3] + 2 * margin).max(1) as u32);
        let crop = frame.crop_imm(x, y, w, h).to_rgb8();
        let crop = image::imageops::resize(&crop, CROP_PX, CROP_PX, FilterType::Lanczos3);
        let lab: Vec<_> = crop.pixels().map(|p| lab(p.0)).collect();
        // Foreground: any channel above 0.28, inside the inner 44 px box (the
        // slot frame's dark interior), as the spike measured it.
        let (lo, hi) = (CROP_PX / 2 - 22, CROP_PX / 2 + 22);
        let (mut sy, mut sx, mut n) = (0.0f32, 0.0f32, 0u32);
        for (px, py, p) in crop.enumerate_pixels() {
            if (lo..hi).contains(&px) && (lo..hi).contains(&py) && p.0.iter().any(|c| *c as f32 / 255.0 > 0.28) {
                sy += py as f32;
                sx += px as f32;
                n += 1;
            }
        }
        let centre = CROP_PX as f32 / 2.0;
        let (cy, cx) = if n == 0 { (centre, centre) } else { (sy / n as f32, sx / n as f32) };
        Self { lab, cy, cx }
    }

    fn at(&self, y: i32, x: i32) -> [f32; 3] {
        let max = CROP_PX as i32 - 1;
        self.lab[(y.clamp(0, max) * CROP_PX as i32 + x.clamp(0, max)) as usize]
    }
}

/// sRGB (D65) to CIE Lab.
fn lab(rgb: [u8; 3]) -> [f32; 3] {
    let lin = rgb.map(|c| {
        let c = c as f32 / 255.0;
        if c > 0.04045 { ((c + 0.055) / 1.055).powf(2.4) } else { c / 12.92 }
    });
    let xyz = [
        (0.4124 * lin[0] + 0.3576 * lin[1] + 0.1805 * lin[2]) / 0.95047,
        0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2],
        (0.0193 * lin[0] + 0.1192 * lin[1] + 0.9505 * lin[2]) / 1.08883,
    ];
    let f = xyz.map(|t| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 });
    [116.0 * f[1] - 16.0, 500.0 * (f[0] - f[1]), 200.0 * (f[1] - f[2])]
}

/// The best RMSE of `icon` on `slot` over the shift search; `stride` > 1 is the
/// coarse pass.
fn score(icon: &IconRef, slot: &SlotImage, stride: usize) -> f32 {
    let oy = (slot.cy - icon.cy).round() as i32;
    let ox = (slot.cx - icon.cx).round() as i32;
    let mut best = f32::INFINITY;
    for dy in -SHIFT_PX..=SHIFT_PX {
        for dx in -SHIFT_PX..=SHIFT_PX {
            let (mut sum, mut n) = (0.0f32, 0u32);
            for &(y, x, l) in icon.points.iter().step_by(stride) {
                let p = slot.at(y + oy + dy, x + ox + dx);
                sum += (p[0] - l[0]).powi(2) + (p[1] - l[1]).powi(2) + (p[2] - l[2]).powi(2);
                n += 1;
            }
            best = best.min((sum / n as f32).sqrt());
        }
    }
    best
}

/// A reference's name and score.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Candidate {
    pub name: String,
    pub rmse: f32,
}

/// `refs` ranked best-first on `slot`, fine-scored for the coarse top.
fn rank(refs: &[IconRef], slot: &SlotImage) -> Vec<Candidate> {
    let mut coarse: Vec<(f32, usize)> = refs
        .iter()
        .enumerate()
        .map(|(i, icon)| (score(icon, slot, (icon.points.len() / COARSE_POINTS).max(1)), i))
        .collect();
    coarse.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut fine: Vec<Candidate> = coarse
        .iter()
        .take(FINE_TOP)
        .map(|&(_, i)| Candidate { name: refs[i].name.clone(), rmse: score(&refs[i], slot, 1) })
        .collect();
    fine.sort_by(|a, b| a.rmse.total_cmp(&b.rmse));
    fine
}

/// One slot's icon verdict.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum IconSlot {
    Certain { name: String, margin: f32 },
    Uncertain { candidates: Vec<Candidate> },
}

/// The icon verdict on a frame's three slots.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum IconRead {
    /// The chosen set's joint score is above the presence gate, judged on a
    /// complete reference population.
    NoPanel { joint: f32 },
    /// The references cannot support the gate: none loaded, or the population
    /// is incomplete and nothing matched (the true gems may be the missing
    /// ones). Neither a panel nor its absence; no slot is guessed.
    Unavailable { joint: f32 },
    Panel {
        set: GemSet,
        /// The chosen set's sum of the three best RMSE.
        joint: f32,
        /// The other set's, when it has references.
        other_joint: Option<f32>,
        slots: [IconSlot; 3],
    },
}

/// Classify the three slots: choose the set by the joint score, apply the
/// presence gate, then call each slot certain or uncertain.
pub(crate) fn classify_slots(refs: &GemIconRefs, slots: &[SlotImage; 3]) -> IconRead {
    let ranked: Vec<(GemSet, [Vec<Candidate>; 3], f32)> = [GemSet::Normal, GemSet::Transfigured]
        .into_iter()
        .filter(|set| !refs.set(*set).is_empty())
        .map(|set| {
            let lists = [0, 1, 2].map(|i| rank(refs.set(set), &slots[i]));
            let joint = lists.iter().map(|l| l[0].rmse).sum::<f32>();
            (set, lists, joint)
        })
        .collect();
    let Some(chosen) = ranked.iter().min_by(|a, b| a.2.total_cmp(&b.2)) else {
        return IconRead::Unavailable { joint: f32::INFINITY };
    };
    let (set, lists, joint) = chosen;
    if *joint > GEM_PANEL_JOINT_RMSE_MAX {
        return if refs.incomplete {
            IconRead::Unavailable { joint: *joint }
        } else {
            IconRead::NoPanel { joint: *joint }
        };
    }
    let other_joint = ranked.iter().find(|r| r.0 != *set).map(|r| r.2);
    let slots = lists.clone().map(|list| {
        let margin = list.get(1).map_or(f32::INFINITY, |second| second.rmse / list[0].rmse);
        if margin >= UNCERTAIN_MARGIN && !refs.incomplete {
            IconSlot::Certain { name: list[0].name.clone(), margin }
        } else {
            IconSlot::Uncertain { candidates: list.into_iter().take(TOP_CANDIDATES).collect() }
        }
    });
    IconRead::Panel { set: *set, joint: *joint, other_joint, slots }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::lab::gem_icon_refs::tests::fixture_refs;
    use crate::ssot::{placements_for, ScreenSlice};

    pub(crate) const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lab/font-gems-1080p.png");
    pub(crate) const TRUTH: [&str; 3] = ["Blade Vortex of the Scythe", "Reave of Refraction", "Blade Flurry of Incision"];

    pub(crate) fn fixture_frame() -> DynamicImage {
        image::open(FIXTURE).unwrap_or_else(|e| panic!("{FIXTURE}: {e}"))
    }

    pub(crate) fn screen(width: u32, height: u32, ui_scale: f32) -> ScreenSlice {
        ScreenSlice {
            width,
            height,
            ui_scale,
            source: crate::ssot::ScreenScaleSource::MercFrame,
            measured_at_ms: 0,
            verified_this_session: true,
            monitor_id: 0,
            origin: (0, 0),
            client: [0, 0, width as i32, height as i32],
            anchors: None,
        }
    }

    pub(crate) fn slots_of(frame: &DynamicImage, rects: [[i32; 4]; 3]) -> [SlotImage; 3] {
        rects.map(|rect| SlotImage::from_frame(frame, rect))
    }

    fn read_at(frame: &DynamicImage, slice: &ScreenSlice) -> IconRead {
        let rects = placements_for(Some(slice)).lab.gem_slots;
        classify_slots(fixture_refs(), &slots_of(frame, rects))
    }

    /// The three names, the set, and whether every slot is certain.
    fn verdict(read: &IconRead) -> Option<(GemSet, Vec<String>, bool)> {
        let IconRead::Panel { set, slots, .. } = read else { return None };
        let names = slots
            .iter()
            .map(|s| match s {
                IconSlot::Certain { name, .. } => name.clone(),
                IconSlot::Uncertain { candidates } => format!("uncertain {:?}", candidates[0].name),
            })
            .collect();
        Some((*set, names, slots.iter().all(|s| matches!(s, IconSlot::Certain { .. }))))
    }

    fn truth() -> Option<(GemSet, Vec<String>, bool)> {
        Some((GemSet::Transfigured, TRUTH.map(String::from).to_vec(), true))
    }

    #[test]
    fn the_real_1080p_frame_reads_its_three_transfigured_gems() {
        let read = read_at(&fixture_frame(), &screen(1920, 1080, 0.9));
        eprintln!("1080p: {read:?}");
        assert_eq!(verdict(&read), truth(), "{read:?}");
    }

    #[test]
    fn the_frame_upscaled_to_4k_reads_the_same_gems_from_the_scaled_slots() {
        let frame = fixture_frame().resize_exact(3840, 2160, FilterType::Lanczos3);
        let read = read_at(&frame, &screen(3840, 2160, 1.8));
        eprintln!("4K: {read:?}");
        assert_eq!(verdict(&read), truth(), "{read:?}");
    }

    #[test]
    fn the_font_panel_pasted_at_its_16_10_place_reads_the_same_gems() {
        let src = fixture_frame();
        let at_1080 = placements_for(Some(&screen(1920, 1080, 0.9))).lab.font;
        let slice = screen(1920, 1200, 1.0);
        let at_1200 = placements_for(Some(&slice)).lab.font;
        let panel = src
            .crop_imm(at_1080[0] as u32, at_1080[1] as u32, at_1080[2] as u32, at_1080[3] as u32)
            .resize_exact(at_1200[2] as u32, at_1200[3] as u32, FilterType::Lanczos3);
        let mut canvas = DynamicImage::new_rgb8(1920, 1200);
        image::imageops::overlay(&mut canvas, &panel, at_1200[0] as i64, at_1200[1] as i64);

        let read = read_at(&canvas, &slice);
        eprintln!("16:10: {read:?}");
        assert_eq!(verdict(&read), truth(), "{read:?}");
    }

    /// A 50/50 blend of two sibling icons, composited on the slot's dark
    /// interior, centred in a frame-sized canvas at the 1080p slot rects.
    pub(crate) fn blended_frame(a: &str, b: &str) -> (DynamicImage, [[i32; 4]; 3]) {
        let dir = crate::lab::gem_icon_refs::tests::ART_DIR;
        let [a, b] = [a, b].map(|g| crate::gem_art::read_art(std::path::Path::new(dir), g).expect(g));
        let mut blend = a.clone();
        for (p, q) in blend.pixels_mut().zip(b.pixels()) {
            for c in 0..4 {
                p[c] = ((p[c] as u16 + q[c] as u16) / 2) as u8;
            }
        }
        let blend = image::imageops::resize(&blend, CANONICAL_PX, CANONICAL_PX, FilterType::Lanczos3);
        let rects = placements_for(Some(&screen(1920, 1080, 0.9))).lab.gem_slots;
        let mut canvas = RgbaImage::from_pixel(1920, 1080, image::Rgba([12, 10, 9, 255]));
        for rect in rects {
            image::imageops::overlay(&mut canvas, &blend, rect[0] as i64, rect[1] as i64);
        }
        (DynamicImage::ImageRgba8(canvas), rects)
    }

    #[test]
    fn a_blend_of_two_siblings_is_uncertain_with_both_among_the_candidates() {
        let (frame, rects) = blended_frame("Dark Pact", "Dark Bargain");

        let read = classify_slots(fixture_refs(), &slots_of(&frame, rects));

        eprintln!("blend: {read:?}");
        let IconRead::Panel { slots, .. } = &read else { panic!("the blend failed the presence gate: {read:?}") };
        for slot in slots {
            let IconSlot::Uncertain { candidates } = slot else { panic!("a blend was read as certain: {slot:?}") };
            let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
            assert!(names.contains(&"Dark Pact") && names.contains(&"Dark Bargain"), "{names:?}");
        }
    }

    #[test]
    fn panel_chrome_above_the_slots_is_no_gem_panel() {
        let rects = [530, 628, 728].map(|x| [x - 24, 300 - 24, 48, 48]);

        let read = classify_slots(fixture_refs(), &slots_of(&fixture_frame(), rects));

        eprintln!("chrome: {read:?}");
        assert!(matches!(read, IconRead::NoPanel { .. }), "{read:?}");
    }

    #[test]
    fn an_empty_failed_population_is_unavailable_not_no_panel() {
        let refs = GemIconRefs { normal: Vec::new(), transfigured: Vec::new(), incomplete: true };
        let rects = placements_for(Some(&screen(1920, 1080, 0.9))).lab.gem_slots;

        let read = classify_slots(&refs, &slots_of(&fixture_frame(), rects));

        assert!(matches!(read, IconRead::Unavailable { .. }), "{read:?}");
    }

    #[test]
    fn an_incomplete_population_with_no_match_is_unavailable_not_no_panel() {
        let refs = crate::lab::gem_icon_refs::tests::fixture_refs_marked_incomplete();
        let rects = [530, 628, 728].map(|x| [x - 24, 300 - 24, 48, 48]);

        let read = classify_slots(&refs, &slots_of(&fixture_frame(), rects));

        assert!(matches!(read, IconRead::Unavailable { .. }), "{read:?}");
    }

    #[test]
    fn incomplete_references_never_call_a_slot_certain() {
        let refs = crate::lab::gem_icon_refs::tests::fixture_refs_marked_incomplete();
        let rects = placements_for(Some(&screen(1920, 1080, 0.9))).lab.gem_slots;

        let read = classify_slots(&refs, &slots_of(&fixture_frame(), rects));

        let IconRead::Panel { slots, .. } = &read else { panic!("{read:?}") };
        assert!(slots.iter().all(|s| matches!(s, IconSlot::Uncertain { .. })), "{read:?}");
    }
}
