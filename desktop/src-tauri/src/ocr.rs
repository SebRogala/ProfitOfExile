/// OCR engine for reading gem names from screenshots.
/// Windows implementation uses Windows.Media.Ocr (built-in, zero dependencies).
/// Other platforms get a stub.

use std::sync::Mutex;

/// The "en-US pack missing, fell back to the profile language" warning, cached
/// for the whole process.
///
/// The engine itself is `thread_local!` — every scan thread resolves its own —
/// so the warning cannot live beside it: `build_status` runs on many threads
/// and must NOT call `engine_report()`, which would construct an extra engine
/// (and re-log the fallback) per thread. This is the one slot `build_status`
/// reads instead. Never cleared: a language pack cannot appear mid-session, and
/// clearing it would flicker the warning off in the UI.
///
/// **Never call `emit_status` (or anything that reaches `build_status`) while
/// holding this lock.** `build_status` reads the cache through
/// [`language_warning`], so emitting under the guard re-enters the same
/// non-reentrant `Mutex` and deadlocks the calling thread. Record first, drop
/// the guard, then emit — which is why [`record_language_warning`] returns
/// rather than notifying.
static OCR_LANGUAGE_WARNING: Mutex<Option<String>> = Mutex::new(None);

/// Store `warning` in `cache`, but only while the cache is still empty.
///
/// Returns true EXACTLY on the `None` -> `Some` transition — the compare-and-set
/// result, for a caller that needs to know it won the race. `None` records
/// nothing: an en-US resolution is not news and must not lock out a later
/// thread's real warning.
///
/// The UI notification deliberately does NOT hang off this return value.
/// Any engine resolution caches the warning, including one from a debug command
/// (`test_ocr_on_image`, the merc debug dump) that has no status to emit — so
/// the thread that wins the transition is not reliably a thread that reports it.
/// `report_ocr_engine` therefore keys its emit on the warning being PRESENT and
/// on its own thread not having reported yet, which is correct whoever recorded.
///
/// Takes the cache by reference so the transition is testable on any platform,
/// without the Windows-only engine. Unused on non-Windows because only the
/// Windows engine path can produce a warning to record.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn record_language_warning(cache: &Mutex<Option<String>>, warning: Option<String>) -> bool {
    let Some(warning) = warning else {
        return false;
    };
    let mut slot = cache.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_some() {
        return false;
    }
    *slot = Some(warning);
    true
}

/// The cached OCR language warning, or `None` when the recognizer resolved to
/// en-US (or when no thread has resolved one yet). Read by `build_status`.
pub fn language_warning() -> Option<String> {
    OCR_LANGUAGE_WARNING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Test-only writer for the process-wide cache the production path uses.
///
/// Exists so the `lib.rs` status-field test can seed the same slot
/// `build_status` reads without making the static crate-visible. Exactly the
/// tests named in `ocr.rs` and `lib.rs` may call it, and they are written to be
/// order-independent: the cache is never cleared, so a second caller in the same
/// test binary records nothing.
#[cfg(test)]
pub fn record_language_warning_globally(warning: &str) -> bool {
    record_language_warning(&OCR_LANGUAGE_WARNING, Some(warning.to_string()))
}

#[cfg(windows)]
mod platform {
    use image::DynamicImage;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::{OcrEngine, OcrLine};
    use windows::Storage::Streams::DataWriter;
    use windows::Foundation::Collections::IVectorView;
    use windows::Globalization::Language;
    use windows::core::HSTRING;

    use crate::mercenary::geometry::OcrLineBox;

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

    /// Resolve (and thread-locally cache) the recognizer.
    ///
    /// Whatever the outcome, the process-wide language warning is cached before
    /// this returns — a fallback on the success path, the combined failure on
    /// the error path — so `build_status` can surface it without ever resolving
    /// an engine of its own.
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
            let (engine, status, warning) = match create_english_engine() {
                Ok(engine) => {
                    log::info!("OCR: using en-US recognizer");
                    (
                        engine,
                        "OCR: using English (en-US) recognizer".to_string(),
                        None,
                    )
                }
                Err(en_err) => {
                    log::warn!("OCR: en-US unavailable ({en_err}), falling back to profile languages");
                    // Keep BOTH errors if the fallback also fails — the en-US
                    // reason is the actionable one, the profile error is the proximate.
                    let engine = match OcrEngine::TryCreateFromUserProfileLanguages() {
                        Ok(engine) => engine,
                        Err(pe) => {
                            let err = format!(
                                "Failed to create OCR engine — en-US: {en_err}; profile: {pe}"
                            );
                            // OCR is dead on this thread, which is strictly worse
                            // than the fallback this arm exists to warn about.
                            // Cache it as the warning BEFORE propagating, or the
                            // one path where nothing can be read is also the one
                            // path where the UI says nothing.
                            super::record_language_warning(
                                &super::OCR_LANGUAGE_WARNING,
                                Some(err.clone()),
                            );
                            return Err(err);
                        }
                    };
                    let status = format!(
                        "OCR: en-US unavailable ({en_err}) — fell back to the Windows profile \
                         language; text may be misread. Install the English (US) OCR language \
                         pack for reliable detection."
                    );
                    log::info!("{status}");
                    (engine, status.clone(), Some(status))
                }
            };
            // Cache the warning process-wide BEFORE handing the engine back, so
            // `build_status` (which never resolves an engine itself) can read it.
            super::record_language_warning(&super::OCR_LANGUAGE_WARNING, warning);
            OCR_STATUS.with(|s| *s.borrow_mut() = Some(status));
            *opt = Some(engine.clone());
            Ok(engine)
        })
    }

    /// Ensure the engine exists on this thread and return the recorded recognizer
    /// status (or the creation error). Call once at the top of each scan thread so
    /// the active recognizer — and any en-US fallback warning — lands in the LOGS panel.
    /// Call it through `report_ocr_engine`, which also gets the warning to the UI.
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

    /// Ensure an OCR engine can be created on this thread.
    ///
    /// Separate from `engine_report` because the merc capture loop needs the
    /// *decision* (run, or publish `unavailable`), not the prose — and
    /// `engine_report` deliberately returns a String in both cases so the LOGS
    /// panel always gets a line.
    pub fn engine_ready() -> Result<(), String> {
        get_or_create_engine().map(|_| ())
    }

    /// Recognize text as LINES WITH RECTS (POE-165 D2).
    ///
    /// `recognize_text` returns strings only, which is enough for the gem and
    /// font loops (they OCR a known region). The merc detector has no fixed
    /// region: it finds the recruit window by where the text sits, so it needs
    /// each line's bounding box.
    ///
    /// `OcrLine` itself exposes no rect — only its words do — so the line box
    /// is the union of its words' `BoundingRect()`s. A line whose words all
    /// fail to report a rect is DROPPED rather than emitted at the origin: a
    /// (0,0) line would join the leftmost column and drag the geometry.
    ///
    /// Coordinates are in the pixel space of the image passed in. The loop
    /// therefore OCRs the screen grab at native resolution and never
    /// `preprocess_for_ocr`s it first — that upscales 2×, and every rect would
    /// come back at twice the screen coordinate.
    pub fn recognize_lines(img: &DynamicImage) -> Result<Vec<OcrLineBox>, String> {
        let engine = get_or_create_engine()?;
        let bitmap = to_software_bitmap(img)?;

        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| format!("OCR recognize failed: {}", e))?
            .get()
            .map_err(|e| format!("OCR result failed: {}", e))?;

        let lines: IVectorView<OcrLine> = result
            .Lines()
            .map_err(|e| format!("Failed to get OCR lines: {}", e))?;

        let mut out = Vec::new();
        for i in 0..lines.Size().unwrap_or(0) {
            let Ok(line) = lines.GetAt(i) else { continue };
            let text = match line.Text() {
                Ok(t) => t.to_string_lossy().trim().to_string(),
                Err(_) => continue,
            };
            if text.is_empty() {
                continue;
            }
            let Ok(words) = line.Words() else { continue };
            let (mut x0, mut y0) = (f32::MAX, f32::MAX);
            let (mut x1, mut y1) = (f32::MIN, f32::MIN);
            for w in 0..words.Size().unwrap_or(0) {
                let Ok(word) = words.GetAt(w) else { continue };
                let Ok(r) = word.BoundingRect() else { continue };
                x0 = x0.min(r.X);
                y0 = y0.min(r.Y);
                x1 = x1.max(r.X + r.Width);
                y1 = y1.max(r.Y + r.Height);
            }
            if !(x1 > x0 && y1 > y0) {
                continue;
            }
            out.push(OcrLineBox {
                text,
                x: x0.floor() as i32,
                y: y0.floor() as i32,
                w: (x1 - x0).ceil().max(1.0) as i32,
                h: (y1 - y0).ceil().max(1.0) as i32,
            });
        }
        Ok(out)
    }

    /// Copy an image into a `SoftwareBitmap` for the OCR engine.
    ///
    /// Deliberately NOT shared with `recognize_text`: this whole module is
    /// Windows-only and cannot be compile-checked on the Linux host this was
    /// written on, so the working gem/font path is left byte-identical rather
    /// than refactored blind. Fold the two together on a Windows box.
    fn to_software_bitmap(img: &DynamicImage) -> Result<SoftwareBitmap, String> {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let pixels = rgba.into_raw();

        let bitmap = SoftwareBitmap::Create(BitmapPixelFormat::Rgba8, width as i32, height as i32)
            .map_err(|e| format!("Failed to create bitmap: {}", e))?;
        let buffer = create_buffer(&pixels)?;
        bitmap
            .CopyFromBuffer(&buffer)
            .map_err(|e| format!("Failed to copy pixels: {}", e))?;
        Ok(bitmap)
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

    use crate::mercenary::geometry::OcrLineBox;

    /// The one message every non-Windows OCR entry point returns, so the merc
    /// loop's `unavailable` status and the debug command's error read the same.
    pub const UNAVAILABLE: &str = "OCR not available on this platform";

    pub fn recognize_text(_img: &DynamicImage) -> Result<Vec<String>, String> {
        Err(UNAVAILABLE.to_string())
    }

    pub fn recognize_lines(_img: &DynamicImage) -> Result<Vec<OcrLineBox>, String> {
        Err(UNAVAILABLE.to_string())
    }

    pub fn engine_ready() -> Result<(), String> {
        Err(UNAVAILABLE.to_string())
    }

    pub fn engine_report() -> String {
        UNAVAILABLE.to_string()
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

    /// A fresh cache, isolated from the process-wide one so the transition can
    /// be exercised more than once per test run.
    fn empty_cache() -> Mutex<Option<String>> {
        Mutex::new(None)
    }

    #[test]
    fn the_first_warning_reports_the_transition_and_is_cached() {
        let cache = empty_cache();
        assert!(record_language_warning(&cache, Some("fell back to ja-JP".into())));
        assert_eq!(
            cache.lock().unwrap().as_deref(),
            Some("fell back to ja-JP")
        );
    }

    #[test]
    fn a_second_warning_reports_no_transition_and_leaves_the_first_standing() {
        // The second scan thread resolves the same broken language setup. The
        // cache is first-write-wins: the text the UI is already showing must not
        // be rewritten under it, and the transition is reported once.
        let cache = empty_cache();
        record_language_warning(&cache, Some("fell back to ja-JP".into()));
        assert!(!record_language_warning(&cache, Some("fell back to ko-KR".into())));
        assert_eq!(
            cache.lock().unwrap().as_deref(),
            Some("fell back to ja-JP"),
            "the first warning is the one the UI already shows; a later thread must not rewrite it"
        );
    }

    #[test]
    fn a_healthy_en_us_resolution_records_nothing() {
        // An en-US engine has no warning to report, and must not occupy the slot
        // — a later thread that DOES fall back still has to get through.
        let cache = empty_cache();
        assert!(!record_language_warning(&cache, None));
        assert_eq!(*cache.lock().unwrap(), None);
        assert!(record_language_warning(&cache, Some("fell back to ja-JP".into())));
    }

    #[test]
    fn language_warning_reads_the_cache_the_recorder_writes() {
        // Wiring: `build_status` reads through `language_warning()` while the
        // engine path writes through `record_language_warning`. Pointing either
        // at a different cell would leave the UI permanently warning-free.
        //
        // Touches the process-wide static, which no test may assume it owns —
        // the cache is never cleared and another test in this binary may have
        // recorded first. So it asserts the reader agrees with the cell rather
        // than with a literal, which holds whichever write won.
        record_language_warning(
            &OCR_LANGUAGE_WARNING,
            Some("en-US unavailable — fell back to the Windows profile language".into()),
        );
        let stored = OCR_LANGUAGE_WARNING.lock().unwrap().clone();
        assert!(stored.is_some(), "recording must leave the cache populated");
        assert_eq!(language_warning(), stored);
    }

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
