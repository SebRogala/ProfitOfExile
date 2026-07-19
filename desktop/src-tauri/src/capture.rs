/// Screen capture for reading PoE gem tooltips.
/// Windows implementation uses xcap for screen capture.
/// Other platforms get a stub that returns an error.

#[cfg(windows)]
mod platform {
    use image::DynamicImage;
    use xcap::Monitor;

    /// Capture the primary monitor's full screen as an image.
    pub fn capture_screen() -> Result<DynamicImage, String> {
        let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;
        let monitor = monitors
            .into_iter()
            .find(|m| m.is_primary())
            .or_else(|| Monitor::all().ok()?.into_iter().next())
            .ok_or_else(|| "No monitor found".to_string())?;

        let img = monitor
            .capture_image()
            .map_err(|e| format!("Screen capture failed: {}", e))?;

        Ok(DynamicImage::ImageRgba8(img))
    }
}

#[cfg(not(windows))]
mod platform {
    use image::DynamicImage;

    pub fn capture_screen() -> Result<DynamicImage, String> {
        Err("Screen capture not available on this platform".to_string())
    }

}

pub use platform::*;

/// Pre-process a captured image for better OCR accuracy:
/// - Convert to grayscale
/// - Increase contrast
/// - Scale up 2x for small text
pub fn preprocess_for_ocr(img: &image::DynamicImage) -> image::DynamicImage {
    use image::imageops::FilterType;

    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();

    // Increase contrast: stretch histogram
    let mut contrasted = gray.clone();
    let (mut min_val, mut max_val) = (255u8, 0u8);
    for p in contrasted.pixels() {
        min_val = min_val.min(p.0[0]);
        max_val = max_val.max(p.0[0]);
    }
    if max_val > min_val {
        let range = (max_val - min_val) as f32;
        for p in contrasted.pixels_mut() {
            let normalized = (p.0[0] - min_val) as f32 / range;
            p.0[0] = (normalized * 255.0) as u8;
        }
    }

    // Upscale short captures — text size tracks band HEIGHT, not crop width, so
    // gate on height alone (POE-116). The old `w <= 800` term silently skipped
    // upscaling for wide-but-short bands (e.g. a 1432×96 gem-name strip), leaving
    // the name too small for OCR. Width is intentionally uncapped: a very wide
    // region upscales proportionally, which is fine in practice because capture is
    // bounded to the primary monitor.
    if h <= 400 {
        let upscaled = image::imageops::resize(&contrasted, w * 2, h * 2, FilterType::Lanczos3);
        image::DynamicImage::ImageLuma8(upscaled)
    } else {
        image::DynamicImage::ImageLuma8(contrasted)
    }
}

#[cfg(test)]
mod tests {
    use super::preprocess_for_ocr;
    use image::{DynamicImage, ImageBuffer, Luma};

    /// Build a solid-gray Luma8 image of the given size. Content is irrelevant to
    /// the upscale gate — these tests pin OUTPUT DIMENSIONS only.
    fn gray(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageLuma8(ImageBuffer::from_pixel(w, h, Luma([128u8])))
    }

    // The bug case (POE-116): a wide-but-short gem-name strip must upscale 2×.
    // Fails if the dropped `w <= 800` gate term is reintroduced (1432 > 800 would
    // skip upscaling) or if the 2× factor changes.
    #[test]
    fn wide_short_band_is_upscaled_2x() {
        let out = preprocess_for_ocr(&gray(1432, 96));
        assert_eq!((out.width(), out.height()), (2864, 192));
    }

    // Non-regression: a tall font-panel crop (h > 400) is left at native size.
    // Fails if the gate ever upscales tall images.
    #[test]
    fn tall_font_panel_is_left_unchanged() {
        let out = preprocess_for_ocr(&gray(753, 759));
        assert_eq!((out.width(), out.height()), (753, 759));
    }

    // Boundary: h == 400 is inside the gate (`<=`), so it upscales.
    // Fails if the operator tightens to `<`.
    #[test]
    fn height_400_boundary_is_upscaled() {
        let out = preprocess_for_ocr(&gray(100, 400));
        assert_eq!((out.width(), out.height()), (200, 800));
    }

    // Boundary: h == 401 is just past the gate, so it is left unchanged.
    // Fails if the operator loosens to include 401.
    #[test]
    fn height_401_boundary_is_left_unchanged() {
        let out = preprocess_for_ocr(&gray(100, 401));
        assert_eq!((out.width(), out.height()), (100, 401));
    }
}
