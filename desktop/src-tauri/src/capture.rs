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

    pub fn capture_region(_x: u32, _y: u32, _w: u32, _h: u32) -> Result<DynamicImage, String> {
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

    // Only upscale small captures — large images are already readable
    if w <= 800 && h <= 400 {
        let upscaled = image::imageops::resize(&contrasted, w * 2, h * 2, FilterType::Lanczos3);
        image::DynamicImage::ImageLuma8(upscaled)
    } else {
        image::DynamicImage::ImageLuma8(contrasted)
    }
}
