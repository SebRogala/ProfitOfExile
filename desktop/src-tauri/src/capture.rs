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

    // Upscale EVERY crop 2×, with no size gate (POE-164). Text size tracks the
    // glyph height inside the band, not the band's own dimensions, so a crop
    // being tall says nothing about whether its text is big enough for OCR.
    // The default font panel region (530×350) cleared the old `h <= 400` gate,
    // but a region widened for a higher-resolution client does not: a 683×641
    // font crop carries the same small UI text and was sent to OCR at native
    // size. The earlier `w <= 800` term (POE-116) had already been dropped for
    // the same reason on the width axis. Both axes scale by the same factor, so
    // the aspect ratio is preserved and OCR line rects stay proportional to the
    // source.
    //
    // Nothing hard-caps the input size: the live capture paths crop from the
    // primary monitor, so dimensions stay bounded. (The test_ocr_on_image debug
    // command can feed an arbitrary image — a 2× buffer of it is the accepted
    // cost of not special-casing a debug path.)
    let upscaled = image::imageops::resize(&contrasted, w * 2, h * 2, FilterType::Lanczos3);
    image::DynamicImage::ImageLuma8(upscaled)
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

    // Width-uncap: a full-4K-wide, short band still upscales 2× on both axes.
    // Fails if any `w <= cap` term (e.g. the old `w <= 800`) is re-added, since
    // 3840 would exceed it and skip upscaling.
    #[test]
    fn full_width_short_band_is_upscaled_2x() {
        let out = preprocess_for_ocr(&gray(3840, 96));
        assert_eq!((out.width(), out.height()), (7680, 192));
    }

    // The POE-164 bug case: a tall crop upscales. 683×641 is a font panel region
    // widened for a higher-resolution client — past the removed `h <= 400` gate,
    // so its small UI text used to reach OCR at native size.
    #[test]
    fn font_region_crop_683x641_is_upscaled_2x() {
        let out = preprocess_for_ocr(&gray(683, 641));
        assert_eq!((out.width(), out.height()), (1366, 1282));
    }

    // h == 400 was the last height the old gate upscaled; it still does. Kept
    // from the gated era so a reintroduced gate cannot pass this suite by
    // upscaling only the tall cases.
    #[test]
    fn height_400_boundary_is_upscaled() {
        let out = preprocess_for_ocr(&gray(100, 400));
        assert_eq!((out.width(), out.height()), (200, 800));
    }

    // Both axes scale by the same factor. A non-square input pins it: scaling
    // one axis only keeps the other dimension right and skews the text, which is
    // exactly the distortion OCR cannot recover from.
    #[test]
    fn upscaling_preserves_the_source_aspect_ratio() {
        let out = preprocess_for_ocr(&gray(300, 200));
        assert_eq!((out.width(), out.height()), (600, 400));
    }
}
