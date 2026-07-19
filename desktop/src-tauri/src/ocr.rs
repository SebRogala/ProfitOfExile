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
    use windows::Globalization::Language;
    use windows::core::HSTRING;

    use std::cell::RefCell;

    thread_local! {
        static OCR_ENGINE: RefCell<Option<OcrEngine>> = RefCell::new(None);
        /// Human-readable status of which recognizer was selected on this thread.
        /// Recorded by get_or_create_engine, surfaced to the LOGS panel via
        /// engine_report() (log::warn!/info! only reach stderr, which release
        /// builds and the in-app LOGS panel never see).
        static OCR_STATUS: RefCell<Option<String>> = RefCell::new(None);
    }

    /// Build an OCR recognizer pinned to en-US. The PoE client renders its UI
    /// in English regardless of the Windows profile locale, so a profile-language
    /// recognizer (e.g. a CJK one on a Korean-locale box) mangles the game text.
    /// Distinguishes "pack not installed" (Ok(false)) from an OCR-runtime failure
    /// of the support check (Err) so the caller reports the real reason instead of
    /// collapsing both into "not installed".
    fn create_english_engine() -> Result<OcrEngine, String> {
        let lang = Language::CreateLanguage(&HSTRING::from("en-US"))
            .map_err(|e| format!("en-US language: {}", e))?;
        match OcrEngine::IsLanguageSupported(&lang) {
            Ok(true) => {}
            Ok(false) => return Err("en-US OCR language pack not installed".into()),
            Err(e) => return Err(format!("en-US support check failed (OCR runtime error): {e}")),
        }
        OcrEngine::TryCreateFromLanguage(&lang).map_err(|e| format!("en-US engine: {}", e))
    }

    fn get_or_create_engine() -> Result<OcrEngine, String> {
        OCR_ENGINE.with(|cell| {
            let mut opt = cell.borrow_mut();
            if let Some(ref engine) = *opt {
                return Ok(engine.clone());
            }
            // Prefer an English recognizer; fall back to the profile-language path
            // only when en-US OCR isn't installed. Record a human-readable status
            // either way so the active recognizer (and any fallback warning)
            // surfaces in the LOGS panel via engine_report().
            let (engine, status) = match create_english_engine() {
                Ok(engine) => {
                    log::info!("OCR: using en-US recognizer");
                    (engine, "OCR: using English (en-US) recognizer".to_string())
                }
                Err(en_err) => {
                    log::warn!("OCR: en-US unavailable ({en_err}), falling back to profile languages");
                    // Keep BOTH errors if the fallback also fails — the en-US
                    // reason is the actionable one, the profile error is the proximate.
                    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
                        .map_err(|pe| format!("Failed to create OCR engine — en-US: {en_err}; profile: {pe}"))?;
                    let status = format!(
                        "OCR: en-US unavailable ({en_err}) — fell back to the Windows profile \
                         language; text may be misread. Install the English (US) OCR language \
                         pack for reliable detection."
                    );
                    log::info!("{status}");
                    (engine, status)
                }
            };
            OCR_STATUS.with(|s| *s.borrow_mut() = Some(status));
            *opt = Some(engine.clone());
            Ok(engine)
        })
    }

    /// Ensure the engine exists on this thread and return the recorded recognizer
    /// status (or the creation error). Call once at the top of each scan thread so
    /// the active recognizer — and any en-US fallback warning — lands in the LOGS panel.
    pub fn engine_report() -> String {
        match get_or_create_engine() {
            Ok(_) => OCR_STATUS.with(|s| {
                s.borrow()
                    .clone()
                    .unwrap_or_else(|| "OCR: recognizer status unavailable".to_string())
            }),
            Err(e) => format!("OCR: engine unavailable — {e}"),
        }
    }

    /// Recognize text in an image using Windows.Media.Ocr.
    /// Returns all recognized text lines. Reuses the OCR engine per thread.
    pub fn recognize_text(img: &DynamicImage) -> Result<Vec<String>, String> {
        let engine = get_or_create_engine()?;

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

    pub fn engine_report() -> String {
        "OCR not available on this platform".to_string()
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
                && l.len() > 2 // "Arc" (3 chars) is the shortest gem name
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
#[allow(dead_code)]
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
