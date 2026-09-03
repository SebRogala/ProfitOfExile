/// Screen capture for reading PoE gem tooltips, the merc recruit window and
/// the temple layout panel.
/// Windows implementation uses xcap for screen capture.
/// Other platforms get a stub that returns an error.

/// One screen grab and the display it came off (POE-237).
///
/// The identity travels WITH the pixels, rather than being asked for again
/// later, because the two questions have different answers the moment the
/// player alt-tabs: a caller that grabbed a frame and then looked up "which
/// monitor is the game on" could stamp a measurement taken on one display with
/// the id of another. Every consumer that publishes geometry
/// (`ssot::ScreenSlice`) or prunes against it (`ssot::drop_if_mismatched`)
/// reads these two fields off the same value it read `image` from.
pub struct Capture {
    /// The whole monitor, as grabbed. Callers crop it themselves.
    pub image: image::DynamicImage,
    /// The display's id in xcap's space (the Windows `HMONITOR` truncated to 32
    /// bits, `xcap::Monitor::id`) — the SAME space
    /// [`GameMonitor::id`] is in, because the focus poller reads its handle the
    /// same way.
    ///
    /// `0` means UNKNOWN and is never compared as an identity: it is what a
    /// pre-POE-237 persisted slice loads as, and — in theory — what a handle
    /// whose low 32 bits are all zero would truncate to. Consumers treat it as
    /// "no opinion" and fall back to comparing dimensions
    /// (`ssot::different_monitor`).
    pub monitor_id: u32,
    /// The display's top-left corner in virtual-desktop PHYSICAL px, so a rect
    /// measured inside this image can be turned into a screen-absolute one.
    /// `(0, 0)` for the primary monitor, and also the unknown value — the two
    /// coincide, which is why `monitor_id` and not this is the identity.
    ///
    /// This is xcap's `Monitor::x()`/`y()`, which reads Windows'
    /// `DEVMODEW.dmPosition`; [`GameMonitor::x`]/[`GameMonitor::y`] are the
    /// same corner read from `GetMonitorInfoW`'s `rcMonitor` instead. OBSERVED
    /// to agree under the app's per-monitor-v2 DPI awareness, which is what
    /// lets `capture_screen` look one display up by the other's corner. If a
    /// future build drops PMv2 the two APIs report different (virtualised)
    /// spaces and that lookup is the thing that breaks first.
    pub origin: (i32, i32),
}

/// The display the GAME is drawn on, as the focus poller last resolved it
/// (POE-237) — the owner is `AppState.game_monitor`.
///
/// `None` there means nothing has seen the game window yet (and is what the
/// non-Windows build always holds), which [`capture_screen`] reads as "grab the
/// primary monitor", the pre-POE-237 behaviour.
///
/// PHYSICAL px throughout, and `x`/`y` are `GetMonitorInfoW`'s `rcMonitor`
/// corner — the same corner [`Capture::origin`] carries out of xcap, by a
/// different API (see that field).
///
/// Carries NO scale factor. Both webview readers size themselves from the
/// TAURI monitor they matched this one to by position, and it is THAT
/// `scaleFactor` their logical-vs-physical maths has to agree with — a second
/// number from a third API could only disagree with it (`overlay/monitor-choice.ts`).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMonitor {
    /// xcap's id space — see [`Capture::monitor_id`].
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Which display the game is on, for the webview (POE-237).
///
/// The layout builds a module's fullscreen widget window on this monitor
/// instead of the primary one, and Settings converts a widget's shipped CSS
/// defaults into coordinates inside that same canvas — see
/// `overlay/monitor-choice.ts`, which owns the matching rule. Both go through
/// the TAURI monitor they match this one to by position, so what they read a
/// scale factor off is that one and never this value.
///
/// `None` while nothing has seen the game window, which every caller must read
/// as "use the primary monitor" rather than as a failure: it is the honest
/// answer before PoE has ever been in the foreground, and the permanent one off
/// Windows.
///
/// A plain read of the owner. The lookup that fills it happens once per focus
/// transition, in the poller, precisely so this command is not a display
/// enumeration on every Settings poll.
#[tauri::command]
pub fn get_game_monitor(state: tauri::State<crate::AppState>) -> Option<GameMonitor> {
    *state.game_monitor.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(windows)]
mod platform {
    use super::{Capture, GameMonitor};
    use image::DynamicImage;
    use tauri::{AppHandle, Emitter, Manager};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use xcap::Monitor;

    /// Capture the display the game is on — or the primary one until something
    /// has said where the game is.
    ///
    /// Before POE-237 this was unconditionally the primary monitor, which is
    /// the bug: with PoE fullscreen on a second display every OCR read the
    /// wrong screen while focus detection still said the game was in front.
    /// The target comes from `AppState.game_monitor`, written by the focus
    /// poller from the game window's own HWND.
    ///
    /// The lookup is `from_point(x + 1, y + 1)` rather than the corner itself:
    /// a monitor rect is half-open at its far edges, and the exact top-left of
    /// a display whose neighbour ends there is ambiguous. One pixel in is
    /// unambiguously inside, on every arrangement.
    ///
    /// A game monitor that cannot be found is a display that is GONE —
    /// unplugged, or rearranged out from under the stored rect — and the
    /// remembered answer is then wrong rather than merely unluckily timed. So
    /// the stored value is CLEARED and the capture falls through to the
    /// primary, which is the pre-POE-237 behaviour and the same answer this
    /// function gives before anything has seen the game window.
    ///
    /// Erroring instead is what shipped first, and it is worse: nothing
    /// re-resolves the display except a focus TRANSITION, and a player whose
    /// second monitor died while PoE stayed in the foreground never makes one —
    /// so every capture for the rest of the session failed, and OCR simply
    /// stopped. Clearing is self-healing in the other direction too: the next
    /// transition into the game writes a fresh value, and because the slot is
    /// now `None` it counts as a change, so it logs and emits again.
    ///
    /// Cleared, and therefore logged, ONCE: the following captures read `None`
    /// and take the primary without a word.
    pub fn capture_screen(app: &AppHandle) -> Result<Capture, String> {
        let target = *app
            .state::<crate::AppState>()
            .game_monitor
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let monitor = match target {
            Some(game) => match Monitor::from_point(game.x + 1, game.y + 1) {
                Ok(monitor) => monitor,
                Err(e) => {
                    *app.state::<crate::AppState>()
                        .game_monitor
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = None;
                    crate::app_log(
                        app,
                        format!(
                            "the game's monitor at {},{} is gone ({}) — capturing the primary until the next alt-tab",
                            game.x, game.y, e
                        ),
                    );
                    primary_monitor()?
                }
            },
            None => primary_monitor()?,
        };

        let monitor_id = monitor.id();
        let origin = (monitor.x(), monitor.y());
        let img = monitor
            .capture_image()
            .map_err(|e| format!("Screen capture failed: {}", e))?;

        Ok(Capture { image: DynamicImage::ImageRgba8(img), monitor_id, origin })
    }

    /// The primary display, or any display at all — the pre-POE-237 target,
    /// kept verbatim as the no-game-window fallback.
    fn primary_monitor() -> Result<Monitor, String> {
        let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;
        monitors
            .into_iter()
            .find(|m| m.is_primary())
            .or_else(|| Monitor::all().ok()?.into_iter().next())
            .ok_or_else(|| "No monitor found".to_string())
    }

    /// Resolve the display `hwnd` is on and remember it as the game's
    /// (POE-237). Called by the focus poller on the transition TO `Game`.
    ///
    /// `MONITOR_DEFAULTTONEAREST` rather than `…TONULL`: a window straddling
    /// two displays, or one being dragged, still has a monitor the player is
    /// looking at it on, and answering "none" there would silently send the
    /// next capture back to the primary.
    ///
    /// The id is the `HMONITOR` truncated the way `xcap::Monitor::id` truncates
    /// it, so [`Capture::monitor_id`] and [`GameMonitor::id`] are comparable —
    /// which is what lets a published measurement say WHICH display it was
    /// taken on.
    ///
    /// Logs and emits ONLY on a change. This runs on every alt-tab back into
    /// the game, and a line per alt-tab would drown the log the player reads.
    pub fn remember_game_monitor(app: &AppHandle, hwnd: HWND) {
        let hmonitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        let mut info =
            MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !unsafe { GetMonitorInfoW(hmonitor, &mut info) }.as_bool() {
            crate::app_log(
                app,
                "could not read the game window's monitor — capture stays on the last known display"
                    .to_string(),
            );
            return;
        }
        let rect = info.rcMonitor;
        let next = GameMonitor {
            id: hmonitor.0 as u32,
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left).max(0) as u32,
            height: (rect.bottom - rect.top).max(0) as u32,
        };

        let changed = {
            let state = app.state::<crate::AppState>();
            let mut current = state.game_monitor.lock().unwrap_or_else(|e| e.into_inner());
            if *current == Some(next) {
                false
            } else {
                *current = Some(next);
                true
            }
        };
        if !changed {
            return;
        }

        crate::app_log(
            app,
            format!(
                "game is on monitor {} at {},{} ({}x{})",
                next.id, next.x, next.y, next.width, next.height
            ),
        );
        // To the MAIN window only, and window-scoped: the layout is what rebuilds
        // a widget overlay onto the new display, and no overlay window acts on
        // this. A `getCurrentWebviewWindow().listen` is therefore what receives
        // it (`docs/OVERLAY-GUIDE.md`, runtime-earned observations).
        if let Err(e) = app.emit_to("main", "game-monitor-changed", next) {
            log::warn!("emit game-monitor-changed failed: {}", e);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Capture;
    use tauri::AppHandle;

    /// No capture off Windows, exactly as before POE-237 — the whole OCR half
    /// of the app is Windows-only and every caller already treats this `Err`
    /// as the normal answer on a dev machine.
    pub fn capture_screen(_app: &AppHandle) -> Result<Capture, String> {
        Err("Screen capture not available on this platform".to_string())
    }
}

pub use platform::*;

/// The factor [`preprocess_for_ocr`] and [`preprocess_for_ocr_fast`] upscale
/// every crop by, on both axes.
///
/// A constant rather than the two literals it replaced because a *caller* now
/// has to undo it: `crate::ocr::recognize_lines` reports each line's box in the
/// pixels of the image it was handed, so a caller that preprocesses first gets
/// boxes at twice the crop's coordinates and has to divide them back
/// (`temple::panel::descaled`, POE-243). One place multiplying and another
/// dividing by independently-written 2s is one edit away from boxes at half or
/// double the truth, which draws in the wrong place rather than failing.
///
/// The merc hover tick undoes the same upscale a DIFFERENT way — it measures
/// the ratio off the processed image's own dimensions
/// (`mercenary::run::tooltip_lines`) — and is deliberately left alone: it is
/// already independent of this constant, so pointing it here would trade a
/// working measurement for a shared assumption.
pub const OCR_UPSCALE: u32 = 2;

/// Pre-process a captured image for better OCR accuracy:
/// - Convert to grayscale
/// - Increase contrast
/// - Scale up 2x for small text
///
/// Lanczos3 on the upscale — the sharpest of the resamplers `image` offers,
/// and the right trade for its callers, which all read small UI text on a
/// cadence measured in seconds: the gem scan and the font scan (`lib.rs`), the
/// temple panel reads (`temple::panel`, `temple::commands`, `temple::run`), the
/// merc HEADER band (`mercenary::read`), and the `test_ocr_on_image` debug
/// command. A path that runs at the cursor's pace wants
/// [`preprocess_for_ocr_fast`] instead.
///
/// The merc DETECT path is on neither list: it hands `ocr::recognize_lines` the
/// frame (or its panel crop) exactly as grabbed and preprocesses nothing — see
/// `mercenary::run::detect_tick`.
pub fn preprocess_for_ocr(img: &image::DynamicImage) -> image::DynamicImage {
    preprocess_with(img, image::imageops::FilterType::Lanczos3)
}

/// [`preprocess_for_ocr`] with a Triangle (bilinear) upscale instead of
/// Lanczos3 — same grayscale, same contrast stretch, same 2× factor.
///
/// For the merc hover tick, which is on the player's critical path: it fires
/// every 400 ms while the cursor rests on a cell, and the tooltip it reads is
/// what the player is waiting to see confirmed. Lanczos3 is a separable 6-tap
/// kernel and Triangle a 2-tap one, so the resample costs a fraction as much
/// on the same crop. The tooltip text it feeds OCR is large, high-contrast UI
/// type at 2×, which is the case where the sharper kernel buys least.
///
/// Deliberately NOT the default: [`preprocess_for_ocr`]'s callers read small UI
/// text — gem and font panel crops, temple room names, the merc header band —
/// and nothing measured says Triangle is as accurate on type that size.
pub fn preprocess_for_ocr_fast(img: &image::DynamicImage) -> image::DynamicImage {
    preprocess_with(img, image::imageops::FilterType::Triangle)
}

fn preprocess_with(
    img: &image::DynamicImage,
    filter: image::imageops::FilterType,
) -> image::DynamicImage {
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
    // Nothing hard-caps the input size: the live capture paths crop from one
    // monitor (the game's since POE-237), so dimensions stay bounded. (The
    // test_ocr_on_image debug
    // command can feed an arbitrary image — a 2× buffer of it is the accepted
    // cost of not special-casing a debug path.)
    let upscaled = image::imageops::resize(
        &contrasted,
        w * OCR_UPSCALE,
        h * OCR_UPSCALE,
        filter,
    );
    image::DynamicImage::ImageLuma8(upscaled)
}

#[cfg(test)]
mod tests {
    use super::{preprocess_for_ocr, preprocess_for_ocr_fast};
    use image::{DynamicImage, ImageBuffer, Luma};

    /// A vertical step edge: the left half `dark`, the right half `light`.
    /// The one input that separates two resamplers — a flat field upscales
    /// identically under any filter.
    fn step_edge(w: u32, h: u32, dark: u8, light: u8) -> DynamicImage {
        DynamicImage::ImageLuma8(ImageBuffer::from_fn(w, h, |x, _| {
            Luma([if x < w / 2 { dark } else { light }])
        }))
    }

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

    // The merc hover path (POE-204 smoke) trades the resampler, and NOTHING
    // else: the 2× upscale is what makes small UI text readable, and a fast
    // path that skipped it would be a silent accuracy regression on the one
    // read the player is waiting for.
    #[test]
    fn the_fast_path_upscales_2x_like_the_quality_one() {
        let out = preprocess_for_ocr_fast(&gray(300, 200));
        assert_eq!((out.width(), out.height()), (600, 400));
    }

    // The other half of "nothing else": the contrast stretch. A tooltip read
    // off a dark panel arrives as a narrow band of greys, and OCR needs it
    // opened out to the full range — the same as every other crop.
    #[test]
    fn the_fast_path_stretches_contrast_like_the_quality_one() {
        let out = preprocess_for_ocr_fast(&step_edge(64, 8, 100, 110)).to_luma8();
        let min = out.pixels().map(|p| p.0[0]).min().expect("a non-empty image");
        let max = out.pixels().map(|p| p.0[0]).max().expect("a non-empty image");

        assert_eq!((min, max), (0, 255));
    }

    // The change itself. Triangle is a 2-tap kernel and Lanczos3 a 6-tap one,
    // so on a step edge they resolve the transition differently — which is the
    // only observable difference between the two functions, and the one thing
    // a revert to Lanczos3 on the hover path would erase.
    #[test]
    fn the_fast_path_does_not_resample_with_the_detect_paths_filter() {
        let img = step_edge(64, 8, 0, 255);

        let fast = preprocess_for_ocr_fast(&img).to_luma8();
        let quality = preprocess_for_ocr(&img).to_luma8();

        assert_eq!(fast.dimensions(), quality.dimensions(), "same crop, same size");
        assert_ne!(fast.into_raw(), quality.into_raw());
    }
}
