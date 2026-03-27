/// OCR engine for reading gem names from screenshots.
/// Windows implementation uses Windows.Media.Ocr (built-in, zero dependencies).
/// Other platforms get a stub.

#[cfg(windows)]
mod platform {
    use image::DynamicImage;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::{OcrEngine, OcrLine};
    use windows::Storage::Streams::DataWriter;
    use windows::Foundation::Collections::IVectorView;

    /// Recognize text in an image using Windows.Media.Ocr.
    /// Returns all recognized text lines.
    pub fn recognize_text(img: &DynamicImage) -> Result<Vec<String>, String> {
        // Create OCR engine from user's language profile
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()
            .map_err(|e| format!("Failed to create OCR engine: {}", e))?;

        // Convert image to RGBA bytes
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let pixels = rgba.into_raw();

        // Create SoftwareBitmap from pixel data
        let bitmap = SoftwareBitmap::Create(
            BitmapPixelFormat::Rgba8,
            width as i32,
            height as i32,
        )
        .map_err(|e| format!("Failed to create bitmap: {}", e))?;

        // Copy pixel data into the bitmap
        let buffer = create_buffer(&pixels)?;
        bitmap
            .CopyFromBuffer(&buffer)
            .map_err(|e| format!("Failed to copy pixels: {}", e))?;

        // Run OCR
        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| format!("OCR recognize failed: {}", e))?
            .get()
            .map_err(|e| format!("OCR result failed: {}", e))?;

        // Extract text lines
        let lines: IVectorView<OcrLine> = result
            .Lines()
            .map_err(|e| format!("Failed to get OCR lines: {}", e))?;

        let mut text_lines = Vec::new();
        for i in 0..lines.Size().unwrap_or(0) {
            if let Ok(line) = lines.GetAt(i) {
                if let Ok(text) = line.Text() {
                    let s: String = text.to_string_lossy();
                    if !s.trim().is_empty() {
                        text_lines.push(s.trim().to_string());
                    }
                }
            }
        }

        Ok(text_lines)
    }

    /// Create an IBuffer from a byte slice for SoftwareBitmap::CopyFromBuffer.
    fn create_buffer(data: &[u8]) -> Result<windows::Storage::Streams::IBuffer, String> {
        let writer = DataWriter::new()
            .map_err(|e| format!("Failed to create DataWriter: {}", e))?;
        writer
            .WriteBytes(data)
            .map_err(|e| format!("Failed to write bytes: {}", e))?;
        writer
            .DetachBuffer()
            .map_err(|e| format!("Failed to detach buffer: {}", e))
    }
}

#[cfg(not(windows))]
mod platform {
    use image::DynamicImage;

    pub fn recognize_text(_img: &DynamicImage) -> Result<Vec<String>, String> {
        Err("OCR not available on this platform".to_string())
    }
}

pub use platform::*;

/// Extract gem name candidates from OCR results.
/// Returns all non-empty lines that could be gem names (filters obvious non-names).
pub fn extract_gem_candidates(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|l| l.trim().to_string())
        .filter(|l| {
            !l.is_empty()
                && l.len() > 5
                && !l.starts_with("Level:")
                && !l.starts_with("Cost:")
                && !l.starts_with("Cooldown")
                && !l.starts_with("Cast Time")
                && !l.starts_with("Quality:")
                && !l.starts_with("Requires")
                && !l.starts_with("Place into")
        })
        .collect()
}

/// Legacy single-candidate extraction (used by tests).
pub fn extract_gem_name(lines: &[String]) -> Option<String> {
    extract_gem_candidates(lines).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_gem_name_from_tooltip_lines() {
        let lines = vec![
            "Summon Stone Golem of Safeguarding".to_string(),
            "Minion, Spell, Golem".to_string(),
            "Level: 20 (Max)".to_string(),
            "Cost: 54 Mana".to_string(),
        ];
        assert_eq!(
            extract_gem_name(&lines),
            Some("Summon Stone Golem of Safeguarding".to_string())
        );
    }

    #[test]
    fn extract_gem_name_skips_stat_lines() {
        let lines = vec![
            "Level: 20 (Max)".to_string(),
            "Cost: 54 Mana".to_string(),
            "Earthquake of Fragility".to_string(),
        ];
        assert_eq!(
            extract_gem_name(&lines),
            Some("Earthquake of Fragility".to_string())
        );
    }

    #[test]
    fn extract_gem_name_returns_none_for_empty() {
        let lines: Vec<String> = vec![];
        assert_eq!(extract_gem_name(&lines), None);
    }
}
