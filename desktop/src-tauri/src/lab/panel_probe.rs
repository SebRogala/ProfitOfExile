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

fn matches(image: &RgbImage, template: &RgbImage, x: i32, y: i32, scale: f32) -> bool {
    let w = (template.width() as f32 * scale).round() as i32;
    let h = (template.height() as f32 * scale).round() as i32;
    if x < 0 || y < 0 || w < 8 || h < 4 || x + w > image.width() as i32 || y + h > image.height() as i32 { return false; }
    let mut error = 0u64;
    let mut n = 0u64;
    for ty in (0..template.height()).step_by(2) {
        for tx in (0..template.width()).step_by(2) {
            let actual = image.get_pixel((x + (tx as f32 * scale).round() as i32).min(x + w - 1) as u32,
                (y + (ty as f32 * scale).round() as i32).min(y + h - 1) as u32);
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
    let scale = slot[2] as f32 / 48.0;
    if !scale.is_finite() || !(0.4..=4.0).contains(&scale) { return None; }
    let x = slot[0] + slot[2] / 2 - (88.0 * scale).round() as i32;
    let y = slot[1] - (157.0 * scale).round() as i32;
    let image = frame.image.to_rgb8();
    let (title, confirm) = assets();
    let margin = (4.0 * scale).ceil() as i32;
    for dy in -margin..=margin {
        for dx in -margin..=margin {
            if matches(&image, title, x + dx, y + dy, scale)
                && matches(&image, confirm, x + dx + (2.0 * scale).round() as i32,
                    y + dy + (250.0 * scale).round() as i32, scale) { return Some(GemPanelEvidence); }
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
}
