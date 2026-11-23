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
///
/// Rust calibration (2026-11-23, `gem_icon_pair_rmse_report`): 361 rows,
/// including 1080p/4K/16:10, the reported frame with and without its true
/// reference, and 13 sibling pairs in both directions under offsets,
/// scales and gold/white washes. The 53 wrong nearest matches have margins
/// 1.001-1.371; the maximum is Massacre's 50% gold wash (RMSE 18.766).
/// The 12 clean real slots have margins >=1.428. Choose 1.4 between these
/// extrema: an RMSE cut alone would reject clean slots before that sibling.
/// Known limit: 1.4 leaves only ~2% headroom under the closest clean real
/// slot (1.428), so a slightly softer real capture can read uncertain.
/// The missing Chain Reaction art reads Scavenging at margin 1.249. The
/// margin also leaves 109 of the 308 correct nearest matches in this
/// degraded sample uncertain; under complete references the reader holds
/// each uncertain slot as its flagged best candidate.
pub(crate) const UNCERTAIN_MARGIN: f32 = 1.4;

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
    classify_ranked(refs, &ranked)
}

fn classify_ranked(refs: &GemIconRefs, ranked: &[(GemSet, [Vec<Candidate>; 3], f32)]) -> IconRead {
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

    /// What one slot of a frame is made to suffer before it is read.
    #[derive(Debug, Clone, Copy)]
    enum Degrade {
        Clean,
        /// Every pixel of the slot's crop moved `amount` of the way to the
        /// colour, as a selection or hover glow would.
        Wash([u8; 3], f32),
        /// The slot rect read `dx, dy` capture px off the icon.
        Offset(i32, i32),
        /// The icon drawn at this factor of the slot size.
        Scale(f32),
    }

    const GOLD: [u8; 3] = [230, 180, 60];
    const WHITE: [u8; 3] = [255, 255, 255];
    /// Slots 1 and 2 of a synthetic frame: clean, so the frame passes the
    /// presence gate on its transfigured set.
    const FILLER: [&str; 2] = ["Blade Vortex of the Scythe", "Reave of Refraction"];

    /// The sibling pairs whose reference icons are closest (Lab RMSE of the
    /// art composited on the slot interior, 13.5-18.7), Detonate Dead being
    /// the reported one.
    const PAIRS: [[&str; 2]; 13] = [
        ["Detonate Dead of Chain Reaction", "Detonate Dead of Scavenging"],
        ["Boneshatter of Carnage", "Boneshatter of Complex Trauma"],
        ["Artillery Ballista of Cross Strafe", "Artillery Ballista of Focus Fire"],
        ["Earthshatter of Fragility", "Earthshatter of Prominence"],
        ["Perforate of Bloodshed", "Perforate of Duality"],
        ["Lacerate of Butchering", "Lacerate of Haemorrhage"],
        ["Summon Flame Golem of Hordes", "Summon Flame Golem of the Meteor"],
        ["Summon Stone Golem of Hordes", "Summon Stone Golem of Safeguarding"],
        ["Volatile Dead of Confinement", "Volatile Dead of Seething"],
        ["Blade Trap of Greatswords", "Blade Trap of Laceration"],
        ["Toxic Rain of Sporeburst", "Toxic Rain of Withering"],
        ["Rain of Arrows of Artillery", "Rain of Arrows of Saturation"],
        ["Ethereal Knives of Lingering Blades", "Ethereal Knives of the Massacre"],
    ];

    /// The reported frame's slot row (`font-slots-detonate-dead-chain-reaction.png`,
    /// see the fixture README), its slot rects and what the game's tooltips named.
    const REPORTED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lab/font-slots-detonate-dead-chain-reaction.png");
    pub(crate) const REPORTED_RECTS: [[i32; 4]; 3] = [[58, 42, 49, 49], [158, 42, 49, 49], [259, 42, 49, 49]];
    pub(crate) const REPORTED_TRUTH: [&str; 3] = ["Toxic Rain of Sporeburst", "Fire Trap of Blasting", "Detonate Dead of Chain Reaction"];

    pub(crate) fn reported_frame() -> RgbaImage {
        image::open(REPORTED).unwrap_or_else(|e| panic!("{REPORTED}: {e}")).to_rgba8()
    }

    /// Move every pixel of `rect`'s crop (the rect plus `SlotImage`'s margin)
    /// `amount` of the way to `colour`.
    fn wash(frame: &mut RgbaImage, rect: [i32; 4], colour: [u8; 3], amount: f32) {
        let m = (rect[2] as f32 * MARGIN_PX as f32 / CANONICAL_PX as f32).round() as i32;
        let (w, h) = (frame.width() as i32, frame.height() as i32);
        for y in (rect[1] - m).max(0)..(rect[1] + rect[3] + m).min(h) {
            for x in (rect[0] - m).max(0)..(rect[0] + rect[2] + m).min(w) {
                let p = frame.get_pixel_mut(x as u32, y as u32);
                for c in 0..3 {
                    p.0[c] = (p.0[c] as f32 * (1.0 - amount) + colour[c] as f32 * amount).round() as u8;
                }
            }
        }
    }

    /// `rects` with slot 3's rect moved or its crop washed by `degrade`
    /// (`Scale` is drawn, not applied here).
    fn degrade_slot_3(frame: &mut RgbaImage, rects: [[i32; 4]; 3], degrade: Degrade) -> [[i32; 4]; 3] {
        let mut out = rects;
        match degrade {
            Degrade::Wash(colour, amount) => wash(frame, rects[2], colour, amount),
            Degrade::Offset(dx, dy) => out[2] = [rects[2][0] + dx, rects[2][1] + dy, rects[2][2], rects[2][3]],
            Degrade::Clean | Degrade::Scale(_) => {}
        }
        out
    }

    /// A 1080p frame of the reference art: `FILLER` clean in slots 1-2 and
    /// `gem` in slot 3 under `degrade`, with the rects to read.
    fn synthetic_frame(gem: &str, degrade: Degrade) -> (DynamicImage, [[i32; 4]; 3]) {
        let dir = std::path::Path::new(crate::lab::gem_icon_refs::tests::ART_DIR);
        let rects = placements_for(Some(&screen(1920, 1080, 0.9))).lab.gem_slots;
        let mut canvas = RgbaImage::from_pixel(1920, 1080, image::Rgba([12, 10, 9, 255]));
        for (i, name) in [FILLER[0], FILLER[1], gem].into_iter().enumerate() {
            let art = crate::gem_art::read_art(dir, name).unwrap_or_else(|| panic!("no art for {name} in {dir:?}"));
            let rect = rects[i];
            let size = match degrade {
                Degrade::Scale(f) if i == 2 => (rect[2] as f32 * f).round() as i32,
                _ => rect[2],
            };
            let icon = image::imageops::resize(&art, size as u32, size as u32, FilterType::Lanczos3);
            let (x, y) = (rect[0] + (rect[2] - size) / 2, rect[1] + (rect[3] - size) / 2);
            image::imageops::overlay(&mut canvas, &icon, x as i64, y as i64);
        }
        let rects = degrade_slot_3(&mut canvas, rects, degrade);
        (DynamicImage::ImageRgba8(canvas), rects)
    }

    /// The reported frame with slot 3 under `degrade` (`Scale` unsupported).
    fn reported_degraded(degrade: Degrade) -> (DynamicImage, [[i32; 4]; 3]) {
        let mut frame = reported_frame();
        let rects = degrade_slot_3(&mut frame, REPORTED_RECTS, degrade);
        (DynamicImage::ImageRgba8(frame), rects)
    }

    /// The gem slot `i` is called certain as, when that is not `truth`.
    fn wrong_certain(read: &IconRead, i: usize, truth: &str) -> Option<String> {
        let IconRead::Panel { slots, .. } = read else { return None };
        match &slots[i] {
            IconSlot::Certain { name, .. } if name != truth => Some(name.clone()),
            _ => None,
        }
    }

    const WASHES: [Degrade; 4] =
        [Degrade::Wash(GOLD, 0.4), Degrade::Wash(GOLD, 0.5), Degrade::Wash(WHITE, 0.4), Degrade::Wash(WHITE, 0.5)];

    mod keep_regression {
        use super::*;
        use crate::lab::gem_icon_refs::tests::fixture_refs_transfigured;

        #[test]
        fn the_reported_frame_reads_detonate_dead_of_chain_reaction_in_slot_3() {
            let (frame, rects) = reported_degraded(Degrade::Clean);

            let read = classify_slots(fixture_refs_transfigured(), &slots_of(&frame, rects));

            eprintln!("reported: {read:?}");
            assert_eq!(verdict(&read), Some((GemSet::Transfigured, REPORTED_TRUTH.map(String::from).to_vec(), true)), "{read:?}");
        }

    }

    mod keep_critical {
        use super::*;
        use crate::lab::gem_icon_refs::tests::fixture_refs_transfigured;

        /// Silent consequence without it: a gem the references lack (a new
        /// league's) is held as its sibling and priced as that gem.
        #[test]
        fn a_slot_whose_gem_has_no_reference_is_not_read_as_its_sibling_certain() {
            let refs = crate::lab::gem_icon_refs::tests::fixture_refs_transfigured_without(REPORTED_TRUTH[2]);
            let (frame, rects) = reported_degraded(Degrade::Clean);

            let read = classify_slots(&refs, &slots_of(&frame, rects));

            let IconRead::Panel { slots, .. } = &read else { panic!("{read:?}") };
            assert!(matches!(&slots[2], IconSlot::Uncertain { .. }), "{:?}", slots[2]);
        }

        /// Silent consequence without it: across the closest sibling pairs, a
        /// washed icon is held as the other variant and priced as that gem.
        #[test]
        fn a_washed_sibling_icon_is_its_own_gem_or_uncertain_never_another_gem_certain() {
            let mut wrong = Vec::new();
            for amount in [0.3, 0.35, 0.4, 0.45, 0.5] {
                for colour in [GOLD, WHITE] {
                    let (frame, rects) = reported_degraded(Degrade::Wash(colour, amount));
                    let read = classify_slots(fixture_refs_transfigured(), &slots_of(&frame, rects));
                    assert!(matches!(read, IconRead::Panel { .. }), "reported {colour:?} {amount}: {read:?}");
                    if let Some(name) = wrong_certain(&read, 2, REPORTED_TRUTH[2]) {
                        wrong.push(format!("reported {colour:?} {amount}: certain {name}"));
                    }
                }
            }
            for gem in PAIRS.into_iter().flatten() {
                for degrade in WASHES {
                    let (frame, rects) = synthetic_frame(gem, degrade);
                    let read = classify_slots(fixture_refs_transfigured(), &slots_of(&frame, rects));
                    assert!(matches!(read, IconRead::Panel { .. }), "{gem} {degrade:?}: {read:?}");
                    if let Some(name) = wrong_certain(&read, 2, gem) {
                        wrong.push(format!("{gem} {degrade:?}: certain {name}"));
                    }
                }
            }
            assert!(wrong.is_empty(), "a wrong gem read as certain: {wrong:#?}");
        }
    }

    /// Rust measurement sample behind the certainty constants; it asserts
    /// nothing. Run `cargo test --lib gem_icon_pair_rmse_report -- --ignored --nocapture`.
    #[test]
    #[ignore = "calibration report, minutes long"]
    fn gem_icon_pair_rmse_report() {
        let refs = crate::lab::gem_icon_refs::tests::fixture_refs_transfigured();
        // The report repeats clean filler crops across hundreds of rows. Reuse
        // their rankings without bypassing the production presence/certainty rules.
        let mut ranked_crops: Vec<(*const GemIconRefs, SlotImage, Vec<Candidate>)> = Vec::new();
        let mut row = |population: &GemIconRefs, label: &str, truth: &str, frame: &DynamicImage, rects: [[i32; 4]; 3], i: usize| {
            let slots = slots_of(frame, rects);
            let lists = slots.map(|slot| {
                if let Some((_, _, list)) = ranked_crops.iter().find(|(prior, crop, _)| {
                    std::ptr::eq(*prior, population) && crop.lab == slot.lab && crop.cy == slot.cy && crop.cx == slot.cx
                }) {
                    return list.clone();
                }
                let list = rank(&population.transfigured, &slot);
                ranked_crops.push((population, slot, list.clone()));
                list
            });
            let joint = lists.iter().map(|list| list[0].rmse).sum();
            let list = &lists[i];
            let read = classify_ranked(population, &[(GemSet::Transfigured, lists.clone(), joint)]);
            let verdict = match &read {
                IconRead::Panel { slots, .. } => match &slots[i] {
                    IconSlot::Certain { name, .. } if name == truth => "certain-right".to_string(),
                    IconSlot::Certain { name, .. } => format!("CERTAIN-WRONG({name})"),
                    IconSlot::Uncertain { .. } => "uncertain".to_string(),
                },
                IconRead::NoPanel { .. } => "no-panel".to_string(),
                IconRead::Unavailable { .. } => "unavailable".to_string(),
            };
            println!(
                "{label:<60} | {best:<38} | {b:6.3} | {r:6.3} | {m:5.3} | nearest-{nearest} | {verdict}",
                best = list[0].name,
                b = list[0].rmse,
                r = list[1].rmse,
                m = list[1].rmse / list[0].rmse,
                nearest = if list[0].name == truth { "correct" } else { "wrong" },
            );
        };
        println!("UNCERTAIN_MARGIN {UNCERTAIN_MARGIN}; best, second-best and margin measured below");
        println!("{:<60} | {:<38} | {:>6} | {:>6} | {:>5} | verdict", "case", "best", "best", "2nd", "margin");
        let fixture = fixture_frame();
        for (label, frame, slice) in [
            ("font-gems-1080p", fixture.clone(), screen(1920, 1080, 0.9)),
            ("font-gems 4K", fixture.resize_exact(3840, 2160, FilterType::Lanczos3), screen(3840, 2160, 1.8)),
        ] {
            let rects = placements_for(Some(&slice)).lab.gem_slots;
            for (i, truth) in TRUTH.into_iter().enumerate() {
                row(refs, format!("{label} slot {}", i + 1).as_str(), truth, &frame, rects, i);
            }
        }
        let from = placements_for(Some(&screen(1920, 1080, 0.9))).lab.font;
        let slice = screen(1920, 1200, 1.0);
        let to = placements_for(Some(&slice)).lab.font;
        let panel = fixture.crop_imm(from[0] as u32, from[1] as u32, from[2] as u32, from[3] as u32)
            .resize_exact(to[2] as u32, to[3] as u32, FilterType::Lanczos3);
        let mut canvas = DynamicImage::new_rgb8(1920, 1200);
        image::imageops::overlay(&mut canvas, &panel, to[0] as i64, to[1] as i64);
        for (i, truth) in TRUTH.into_iter().enumerate() {
            row(refs, &format!("font-gems 16:10 slot {}", i + 1), truth, &canvas, placements_for(Some(&slice)).lab.gem_slots, i);
        }
        let missing = crate::lab::gem_icon_refs::tests::fixture_refs_transfigured_without(REPORTED_TRUTH[2]);
        let (frame, rects) = reported_degraded(Degrade::Clean);
        row(&missing, "reported missing Chain Reaction", REPORTED_TRUTH[2], &frame, rects, 2);
        for degrade in [Degrade::Clean, Degrade::Offset(3, 3), Degrade::Offset(-3, -3)]
            .into_iter()
            .chain([0.2, 0.3, 0.4, 0.5].into_iter().flat_map(|a| [Degrade::Wash(GOLD, a), Degrade::Wash(WHITE, a)]))
        {
            let (frame, rects) = reported_degraded(degrade);
            if matches!(degrade, Degrade::Clean) {
                for i in 0..2 {
                    row(refs, format!("reported slot {}", i + 1).as_str(), REPORTED_TRUTH[i], &frame, rects, i);
                }
            }
            row(refs, format!("reported slot 3 {degrade:?}").as_str(), REPORTED_TRUTH[2], &frame, rects, 2);
        }
        let degrades = [Degrade::Clean, Degrade::Offset(3, 3), Degrade::Offset(-3, -3), Degrade::Scale(0.94), Degrade::Scale(1.06)]
            .into_iter()
            .chain([0.2, 0.3, 0.4, 0.5].into_iter().flat_map(|a| [Degrade::Wash(GOLD, a), Degrade::Wash(WHITE, a)]));
        for degrade in degrades {
            for gem in PAIRS.into_iter().flatten() {
                let (frame, rects) = synthetic_frame(gem, degrade);
                row(refs, format!("{gem} {degrade:?}").as_str(), gem, &frame, rects, 2);
            }
        }
    }
}
