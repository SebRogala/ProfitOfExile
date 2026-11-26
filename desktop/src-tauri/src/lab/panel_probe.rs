//! Pixel-only gem-choice recognition, measured on the real lab fixture.
use std::sync::OnceLock;
use image::RgbImage;
use crate::{capture::Capture, ssot::LabPlacements};

#[derive(Debug, Clone, Copy)]
pub(crate) struct GemPanelEvidence;

fn assets() -> &'static (RgbImage, RgbImage) {
    static ASSETS: OnceLock<(RgbImage, RgbImage)> = OnceLock::new();
    ASSETS.get_or_init(|| (
        image::load_from_memory(include_bytes!("assets/divine-font-title.png")).expect("embedded title").to_rgb8(),
        image::load_from_memory(include_bytes!("assets/divine-font-confirm-lit.png")).expect("embedded confirm").to_rgb8(),
    ))
}

fn matches(image: &RgbImage, template: &RgbImage, x: i32, y: i32) -> bool {
    let w = template.width() as i32;
    let h = template.height() as i32;
    if x < 0 || y < 0 || w < 8 || h < 4 || x + w > image.width() as i32 || y + h > image.height() as i32 { return false; }
    let mut error = 0u64;
    let mut n = 0u64;
    for ty in (0..template.height()).step_by(2) {
        for tx in (0..template.width()).step_by(2) {
            let actual = image.get_pixel((x + tx as i32) as u32, (y + ty as i32) as u32);
            let expected = template.get_pixel(tx, ty);
            for c in 0..3 { error += actual[c].abs_diff(expected[c]) as u64; n += 1; }
        }
    }
    // Source pixels match exactly. This tolerance admits resize interpolation;
    // masked and darkened patches are rejected by the real-fixture tests.
    error < n * 12
}

pub(crate) fn probe(frame: &Capture, lab: &LabPlacements) -> Option<GemPanelEvidence> {
    let slot = lab.gem_slots[1];
    let centre = |r: [i32;4]| r[0] + r[2] / 2;
    let scale = (centre(lab.gem_slots[2]) - centre(lab.gem_slots[0])) as f32 / 198.0;
    if !scale.is_finite() || !(0.4..=4.0).contains(&scale) { return None; }
    let x = slot[0] + slot[2] / 2 - (88.0 * scale).round() as i32;
    let y = slot[1] - (157.0 * scale).round() as i32;
    let (title, confirm) = assets();
    let resize = |template: &RgbImage| image::imageops::resize(template,
        (template.width() as f32 * scale).round() as u32,
        (template.height() as f32 * scale).round() as u32, image::imageops::FilterType::Lanczos3);
    let (title, confirm) = (resize(title), resize(confirm));
    let margin = (4.0 * scale).ceil() as i32;
    let (cx, cy) = ((2.0 * scale).round() as i32, (250.0 * scale).round() as i32);
    let left = (x - margin).min(x + cx - margin).max(0);
    let top = (y - margin).max(0);
    let right = (x + margin + title.width() as i32).max(x + cx + margin + confirm.width() as i32)
        .min(frame.image.width() as i32);
    let bottom = (y + margin + title.height() as i32).max(y + cy + margin + confirm.height() as i32)
        .min(frame.image.height() as i32);
    if right <= left || bottom <= top { return None; }
    let image = frame.image.crop_imm(left as u32, top as u32, (right - left) as u32, (bottom - top) as u32).to_rgb8();
    let (x, y) = (x - left, y - top);
    for dy in -margin..=margin {
        for dx in -margin..=margin {
            if matches(&image, &title, x + dx, y + dy)
                && matches(&image, &confirm, x + dx + cx, y + dy + cy) { return Some(GemPanelEvidence); }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;
    fn frame() -> Capture {
        Capture { image: image::load_from_memory(include_bytes!("../../tests/fixtures/lab/font-gems-1080p.png")).unwrap(),
            monitor_id: 1, origin: (0,0), client: [0,0,1920,1080], timing: Default::default() }
    }
    #[test]
    fn real_gem_choice_matches() { assert!(probe(&frame(), &crate::ssot::placements_for(None).lab).is_some()); }
    #[test]
    fn title_mask_rejects() {
        let mut frame = frame();
        image::imageops::replace(&mut frame.image, &DynamicImage::new_rgb8(176,40), 540,216);
        assert!(probe(&frame, &crate::ssot::placements_for(None).lab).is_none());
    }
    #[test]
    fn confirm_mask_rejects() {
        let mut frame = frame();
        image::imageops::replace(&mut frame.image, &DynamicImage::new_rgb8(170,36), 542,466);
        assert!(probe(&frame, &crate::ssot::placements_for(None).lab).is_none());
    }
    #[test]
    fn dark_confirm_rejects() {
        let mut frame = frame();
        let dark = frame.image.crop_imm(542,466,170,36).brighten(-80);
        image::imageops::replace(&mut frame.image, &dark, 542,466);
        assert!(probe(&frame, &crate::ssot::placements_for(None).lab).is_none());
    }
    #[test]
    fn tiny_frame_rejects() {
        let mut frame = frame(); frame.image = DynamicImage::new_rgb8(1,1);
        assert!(probe(&frame, &crate::ssot::placements_for(None).lab).is_none());
    }

    #[test]
    fn derived_4k_frame_matches_projected_slots() {
        let mut frame = frame();
        frame.image = frame.image.resize_exact(3840, 2160, image::imageops::FilterType::Lanczos3);
        let screen = crate::ssot::screen_from_geometry(3840, 2160, 1, (0,0), [0,0,3840,2160], 0);
        assert!(probe(&frame, &crate::ssot::placements(&screen).lab).is_some());
    }

    #[test]
    fn derived_16_10_panel_matches_projected_slots() {
        let source = frame();
        let old = crate::ssot::placements_for(None).lab.gem_slots[1];
        let screen = crate::ssot::screen_from_geometry(1920, 1200, 1, (0,0), [0,0,1920,1200], 0);
        let lab = crate::ssot::placements(&screen).lab;
        // Resize the entire real frame: lab.font's OCR crop clips four rows of
        // the title template, so that crop cannot stand in for the whole panel.
        let panel = source.image.resize_exact(2133,1200,image::imageops::FilterType::Lanczos3);
        let slot = lab.gem_slots[1];
        let x = slot[0] + slot[2]/2 - ((old[0]+old[2]/2) as f32 * 1200.0/1080.0).round() as i32;
        let y = slot[1] + slot[3]/2 - ((old[1]+old[3]/2) as f32 * 1200.0/1080.0).round() as i32;
        let mut target = source;
        target.image = DynamicImage::new_rgb8(1920, 1200);
        image::imageops::overlay(&mut target.image, &panel, x as i64, y as i64);
        assert!(probe(&target, &lab).is_some());
    }

    #[test]
    fn flat_frame_rejects() {
        let mut frame = frame();
        frame.image = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(1920,1080,image::Rgb([40,40,40])));
        assert!(probe(&frame, &crate::ssot::placements_for(None).lab).is_none());
    }

    #[test]
    fn clipped_confirm_rejects() {
        let mut frame = frame();
        frame.image = frame.image.crop_imm(0,0,1920,480);
        assert!(probe(&frame, &crate::ssot::placements_for(None).lab).is_none());
    }

    #[test]
    fn unrelated_real_mechanic_frames_reject() {
        for bytes in [include_bytes!("../../tests/fixtures/temple/screen-live-1920x1080.png").as_slice(),
            include_bytes!("../../tests/fixtures/merc-recruit-pc-1080p.png").as_slice()] {
            let mut frame = frame();
            frame.image = image::load_from_memory(bytes).unwrap();
            assert!(probe(&frame, &crate::ssot::placements_for(None).lab).is_none());
        }
    }

    #[test]
    fn embedded_assets_are_lossless_source_crops() {
        let frame = frame();
        let (title, confirm) = assets();
        assert_eq!(*title, frame.image.crop_imm(540,216,176,40).to_rgb8());
        assert_eq!(*confirm, frame.image.crop_imm(542,466,170,36).to_rgb8());
    }

    mod keep_regression {
        use super::*;

        // keep:regression (POE-503, Operator-reported): in a Cruel lab the Font window
        // behind the gem pick is taller, so its title sits 26 px higher than in Normal
        // while the 3-gem popup and CONFIRM stay within 1 px, and the pick went undetected.
        // Fixture: the Operator's 1920x1080 JPEG capture (2026-11-26, Cruel lab), rect
        // (x 282, y 138, w 690, h 536) decoded by the `image` crate and saved as PNG
        // without re-encode; pasted back at that offset on a black 1080p canvas.
        #[test]
        fn cruel_font_window_gem_pick_matches() {
            let crop = image::load_from_memory(
                include_bytes!("../../tests/fixtures/lab/font-gems-cruel-1080p-crop.png")).unwrap();
            let mut frame = frame();
            frame.image = DynamicImage::new_rgb8(1920, 1080);
            image::imageops::overlay(&mut frame.image, &crop, 282, 138);
            assert!(probe(&frame, &crate::ssot::placements_for(None).lab).is_some(),
                "the Cruel-height Font window's gem pick must be recognised");
        }
    }
}
