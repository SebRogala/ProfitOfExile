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
    /// `0` means UNKNOWN: it is what a pre-POE-237 persisted slice loads as.
    /// It is carried for the log and the Settings card; it is NOT the screen's
    /// identity, because Windows hands one display a new handle
    /// between sessions — `origin` is (`ssot::different_display`).
    pub monitor_id: u32,
    /// The display's top-left corner in virtual-desktop PHYSICAL px, so a rect
    /// measured inside this image can be turned into a screen-absolute one.
    /// `(0, 0)` for the primary monitor, and also the unknown value. This is
    /// the screen's identity (`ssot::different_display`): two displays of one
    /// desktop cannot share a top-left, and an unknown reads as the primary.
    ///
    /// This is xcap's `Monitor::x()`/`y()`, which reads Windows'
    /// `DEVMODEW.dmPosition`; [`GameMonitor::x`]/[`GameMonitor::y`] are the
    /// same corner read from `GetMonitorInfoW`'s `rcMonitor` instead. OBSERVED
    /// to agree under the app's per-monitor-v2 DPI awareness, which is what
    /// lets `capture_screen` look one display up by the other's corner. If a
    /// future build drops PMv2 the two APIs report different (virtualised)
    /// spaces and that lookup is the thing that breaks first.
    pub origin: (i32, i32),
    /// The capture rectangle in this capture's physical pixels: x, y, w, h.
    /// POE-268 treats the capture as fullscreen, so this is always the whole
    /// image. POE-272 will add the live client offset for windowed support.
    pub client: [i32; 4],
    /// Where this grab's time went, for callers that log their own stages.
    pub timing: GrabTiming,
}

/// One `capture_screen` call split into its phases, in ms.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GrabTiming {
    /// HWND lookups (an event-loop round trip each) plus setting
    /// `WDA_EXCLUDEFROMCAPTURE` on the listed overlays.
    pub exclude_ms: u32,
    /// The xcap copy of the whole monitor.
    pub grab_ms: u32,
    /// Clearing the affinity again.
    pub restore_ms: u32,
    /// The whole call, monitor lookup included.
    pub call_ms: u32,
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

/// Per-window count of overlapping grabs holding the exclude-from-capture
/// affinity, keyed by the raw window handle. Grabs run on several threads (lab
/// scan, loot hotkey, merc and temple loops, debug commands); without the count
/// one grab finishing cleared the flag while another was still copying the
/// screen, and the overlay landed in that frame (desktop M1, 2026-11-12).
///
/// The platform call runs under the lock, so set and clear happen in the order
/// the count changes. Callers resolve the handle before calling in and never
/// hold the ledger across the grab itself.
#[cfg(any(windows, test))]
#[derive(Default)]
struct AffinityLedger {
    counts: std::sync::Mutex<std::collections::HashMap<isize, usize>>,
}

#[cfg(any(windows, test))]
impl AffinityLedger {
    /// Holds `hwnd` excluded. Only the first holder calls `set`; a failed
    /// `set` returns false and leaves no count.
    fn acquire(&self, hwnd: isize, set: impl FnOnce() -> bool) -> bool {
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = counts.get_mut(&hwnd) {
            *n += 1;
            return true;
        }
        if !set() {
            return false;
        }
        counts.insert(hwnd, 1);
        true
    }

    /// Drops one hold on `hwnd`; only the last holder calls `clear`.
    fn release(&self, hwnd: isize, clear: impl FnOnce()) {
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        let Some(n) = counts.get_mut(&hwnd) else { return };
        *n -= 1;
        if *n == 0 {
            counts.remove(&hwnd);
            clear();
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::{AffinityLedger, Capture, GameMonitor, GrabTiming};
    use std::time::Instant;
    use image::DynamicImage;
    use tauri::{AppHandle, Emitter, Manager};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::LazyLock;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    };
    use xcap::Monitor;

    /// Overlay windows kept out of a screen grab WHILE it is taken.
    ///
    /// The merc strip draws over the recruit panel and the reader grabs the
    /// whole monitor, so whatever the strip paints is otherwise in the frame
    /// the icons are matched on. The preview can sit over any module's OCR
    /// region for the same reason. Windows' exclude-from-capture affinity
    /// removes a window from captures without changing what is on screen —
    /// measured 2026-09-07 through Tauri's `contentProtected`, which sets the
    /// same flag: the strip vanished from the reader's dump AND from the
    /// player's own screenshots. The second half is the cost, so the flag is
    /// not held permanently: [`ExcludedFromCapture`] sets it just before a
    /// grab and clears it right after; per window handle, the first of any
    /// overlapping grabs sets it and the last one clears it
    /// ([`AffinityLedger`]). Between grabs
    /// — at the reader's 2 s live / 10 s paused cadence, nearly always — a
    /// screenshot tool sees the listed window.
    ///
    /// Labels, not handles: a window that is not built right now is simply
    /// skipped, and one rebuilt later is picked up by the next grab.
    const EXCLUDED_WHILE_GRABBING: &[&str] = &["mercenary", "overlay-preview", "loot"];

    /// The affinity held for one grab. `Drop` releases this guard's hold and
    /// the last holder clears it, so an error between `hold` and the end of
    /// the grab cannot leave a listed window out of screenshots.
    struct ExcludedFromCapture {
        hwnds: Vec<HWND>,
    }

    /// Reported once per process, not once per grab: the reader grabs every
    /// few seconds, and a failure here is a configuration fact, not news.
    static AFFINITY_FAILURE_REPORTED: AtomicBool = AtomicBool::new(false);

    /// The one line that says the mechanism is active in this session — the
    /// first successful hold. A troubled session with no `capture:` line at
    /// all never had a listed window to exclude (module off, preview not built).
    static AFFINITY_ARMED_REPORTED: AtomicBool = AtomicBool::new(false);

    static LEDGER: LazyLock<AffinityLedger> = LazyLock::new(AffinityLedger::default);

    impl ExcludedFromCapture {
        fn hold(app: &AppHandle) -> Self {
            let mut hwnds = Vec::new();
            for label in EXCLUDED_WHILE_GRABBING {
                let Some(window) = app.get_webview_window(label) else { continue };
                // `hwnd()` is a blocking round-trip to the event loop (see the
                // click-through setup in `lib.rs`); one per grab is a few
                // hundred microseconds against a full-monitor copy.
                let hwnd = match window.hwnd() {
                    Ok(hwnd) => HWND(hwnd.0 as *mut _),
                    Err(e) => {
                        report_once(app, format!("overlay '{label}' HWND lookup failed: {e}"));
                        continue;
                    }
                };
                // Only the first holder calls the platform; its result is
                // reported after the ledger lock is released.
                let mut set_result = None;
                let held = LEDGER.acquire(hwnd.0 as isize, || {
                    let result = unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) };
                    let ok = result.is_ok();
                    set_result = Some(result);
                    ok
                });
                match set_result {
                    Some(Ok(())) => {
                        if !AFFINITY_ARMED_REPORTED.swap(true, Ordering::Relaxed) {
                            crate::app_log(
                                app,
                                format!("capture: overlay '{label}' is excluded from each grab while it is taken"),
                            );
                        }
                    }
                    Some(Err(e)) => report_once(
                        app,
                        format!("overlay '{label}' could not be excluded from capture: {e}"),
                    ),
                    None => {}
                }
                if held {
                    hwnds.push(hwnd)
                }
            }
            Self { hwnds }
        }
    }

    impl Drop for ExcludedFromCapture {
        fn drop(&mut self) {
            for hwnd in &self.hwnds {
                // A clear that fails leaves the window out of screenshots until
                // the next grab's clear; there is no `app` here to log with,
                // and the set half of the same call already reports.
                LEDGER.release(hwnd.0 as isize, || {
                    let _ = unsafe { SetWindowDisplayAffinity(*hwnd, WDA_NONE) };
                });
            }
        }
    }

    /// Guard 6 of the overlay guide, without a line per grab.
    fn report_once(app: &AppHandle, msg: String) {
        if AFFINITY_FAILURE_REPORTED.swap(true, Ordering::Relaxed) {
            return;
        }
        crate::app_log(app, format!("capture: {msg} — the listed overlay is IN every grab"));
    }

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
    pub(super) fn grab_screen(app: &AppHandle) -> Result<Capture, String> {
        let call_started = Instant::now();
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
        let ms = |since: Instant| since.elapsed().as_millis() as u32;
        let mut timing = GrabTiming::default();
        // Held for exactly this copy — see `EXCLUDED_WHILE_GRABBING`.
        let phase = Instant::now();
        let excluded = ExcludedFromCapture::hold(app);
        timing.exclude_ms = ms(phase);
        let phase = Instant::now();
        let grabbed = monitor.capture_image();
        timing.grab_ms = ms(phase);
        let phase = Instant::now();
        drop(excluded);
        timing.restore_ms = ms(phase);
        timing.call_ms = ms(call_started);
        let img = grabbed.map_err(|e| format!("Screen capture failed: {}", e))?;

        let client = [0, 0, img.width() as i32, img.height() as i32];
        Ok(Capture { image: DynamicImage::ImageRgba8(img), monitor_id, origin, client, timing })
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
    pub(super) fn grab_screen(_app: &AppHandle) -> Result<Capture, String> {
        Err("Screen capture not available on this platform".to_string())
    }
}

pub use platform::*;

/// Set by the first refused grab; nothing re-arms it, so a session that never
/// accepts logs the refusal once, not once per tick.
static REFUSAL_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Grab the screen, the only grab path in the app (POE-395, ADR-033): while
/// the Data & privacy notice is not accepted it grabs nothing and returns
/// `Err`, which every caller already reads as "no frame".
pub fn capture_screen(app: &tauri::AppHandle) -> Result<Capture, String> {
    use tauri::Manager;
    let gate = &app.state::<crate::AppState>().privacy_gate;
    grab_if_accepted(gate, &REFUSAL_LOGGED, |msg| crate::app_log(app, msg), || platform::grab_screen(app))
}

/// Reads `gate` on every call, so opening it lets the next grab through.
fn grab_if_accepted<T>(
    gate: &crate::http::NoticeGate,
    refusal_logged: &std::sync::atomic::AtomicBool,
    log: impl FnOnce(String),
    grab: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if !gate.is_open() {
        if !refusal_logged.swap(true, std::sync::atomic::Ordering::SeqCst) {
            log("screen capture refused until the Data & privacy notice is accepted".to_string());
        }
        return Err(crate::http::NOTICE_NOT_ACCEPTED.to_string());
    }
    grab()
}

/// The factor [`preprocess_for_ocr`] and [`preprocess_for_ocr_fast`] upscale
/// a crop by, on both axes, whenever the result fits the OCR engine's
/// `MaxImageDimension`; [`ocr_upscale_factor`] lowers it for a crop that would
/// not (POE-361).
///
/// The factor actually applied can therefore differ from this constant, so a
/// caller that undoes the upscale measures it off the processed image's own
/// dimensions against the crop's: `temple::panel::SystemOcr` and
/// `mercenary::run::tooltip_lines` both do. `crate::ocr::recognize_lines`
/// reports each line's box in the pixels of the image it was handed, so a
/// caller that divided by this constant instead would draw at the wrong place
/// on a clamped crop rather than fail.
pub const OCR_UPSCALE: u32 = 2;

/// The upscale factor for a `w`×`h` crop when the OCR engine accepts at most
/// `limit` px on either axis.
///
/// [`OCR_UPSCALE`] while `max(w, h) · OCR_UPSCALE` fits; otherwise the factor
/// that puts the longer side exactly at `limit`, which is fractional, 1, or
/// below 1 only when the native crop already exceeds the limit. `None` means
/// no limit is known, and the crop gets the full factor.
pub fn ocr_upscale_factor(w: u32, h: u32, limit: Option<u32>) -> f32 {
    let longest = w.max(h);
    match limit {
        Some(limit) if longest > 0 && longest.saturating_mul(OCR_UPSCALE) > limit => {
            limit as f32 / longest as f32
        }
        _ => OCR_UPSCALE as f32,
    }
}

/// The output size of a `w`×`h` crop upscaled under [`ocr_upscale_factor`].
///
/// The unclamped case is the exact integer `w · 2`, `h · 2` it always was. A
/// clamped side is floored, so the longer side lands at `limit` and never one
/// past it, and kept at least 1 px.
pub fn ocr_upscaled_size(w: u32, h: u32, limit: Option<u32>) -> (u32, u32) {
    let factor = ocr_upscale_factor(w, h, limit);
    if factor == OCR_UPSCALE as f32 {
        return (w * OCR_UPSCALE, h * OCR_UPSCALE);
    }
    // Clamped only when a limit is known. Integer arithmetic, `v · limit /
    // longest`: the f32 factor times 6880 lands at 9999.99 and floors a pixel
    // short of the limit.
    let (limit, longest) = (u64::from(limit.unwrap_or(0)), u64::from(w.max(h)));
    let side = |v: u32| ((u64::from(v) * limit / longest) as u32).max(1);
    (side(w), side(h))
}

/// Pre-process a captured image for better OCR accuracy:
/// - Convert to grayscale
/// - Increase contrast
/// - Scale up 2x for small text, clamped to the OCR engine's size limit
///
/// Lanczos3 on the upscale — the sharpest of the resamplers `image` offers,
/// and the right trade for its callers, which all read small UI text on a
/// cadence measured in seconds: the gem scan and the font scan (`lab/commands.rs`), the
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
    // The OCR engine rejects an image over `MaxImageDimension` on either axis
    // (POE-361): a gem band spans the full client width, and a 6880-wide client
    // at 2× is 13760 px. The factor is clamped to the limit instead
    // (`ocr_upscale_factor`); a client that wide renders its UI at ~2.67×, so
    // its glyphs are already past the size the 2× exists for. The other large
    // crops (font panel, temple panel, merc header) are sub-regions of the
    // same frame and take the same clamp here. The merc detect path skips this
    // function and hands OCR the frame unscaled, so the clamp never reaches it.
    let (out_w, out_h) = ocr_upscaled_size(w, h, crate::ocr::max_image_dimension());
    let upscaled = image::imageops::resize(&contrasted, out_w, out_h, filter);
    image::DynamicImage::ImageLuma8(upscaled)
}

#[cfg(test)]
mod tests {
    use super::{ocr_upscale_factor, ocr_upscaled_size, preprocess_for_ocr, preprocess_for_ocr_fast};
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

    // POE-361: the 6880-wide gem band at 2× is 13760 px, past the engine's
    // limit. The limit is a parameter here; 10000 is the value the report
    // names, not one production reads. The clamped band's longer side lands AT
    // the limit, never past it, and its height scales by the same factor.
    //
    // Fails if the clamp is dropped (13760 wide) or overshoots by a rounding
    // pixel (10001).
    #[test]
    fn a_6880_wide_gem_band_is_clamped_to_the_ocr_limit() {
        let (w, h) = ocr_upscaled_size(6880, 96, Some(10_000));

        assert_eq!(w, 10_000);
        assert_eq!(h, 96 * 10_000 / 6880);
    }

    // At 1080p, 1440p and 4K every production crop keeps the exact 2× it
    // always had: gem bands of 1920/2560/3840 and font, temple and merc header
    // sized crops, under the same 10000 limit.
    //
    // Fails if the clamp engages below the limit (e.g. a `>=` gate or a
    // limit compared against the native size times something other than 2).
    #[test]
    fn crops_at_1080p_1440p_and_4k_keep_the_exact_2x() {
        for (w, h) in [(1920, 96), (2560, 128), (3840, 192), (683, 641), (1100, 900), (1600, 300)] {
            assert_eq!(ocr_upscale_factor(w, h, Some(10_000)), 2.0, "{w}x{h}");
            assert_eq!(ocr_upscaled_size(w, h, Some(10_000)), (w * 2, h * 2), "{w}x{h}");
        }
    }

    // The boundary: exactly `limit` at 2× still fits; one px more clamps.
    #[test]
    fn a_crop_whose_2x_lands_exactly_on_the_limit_is_not_clamped() {
        assert_eq!(ocr_upscaled_size(5000, 50, Some(10_000)), (10_000, 100));
        assert!(ocr_upscale_factor(5001, 50, Some(10_000)) < 2.0);
    }

    // Native size fits but 2× does not: the factor sits between 1 and 2, so
    // the crop is never shrunk below the size it was captured at.
    #[test]
    fn a_crop_between_half_the_limit_and_the_limit_is_never_downscaled() {
        let factor = ocr_upscale_factor(9000, 100, Some(10_000));
        assert!(factor >= 1.0 && factor < 2.0, "factor {factor}");
    }

    // Only a crop already over the limit natively is downscaled, to fit.
    #[test]
    fn a_crop_over_the_limit_natively_is_downscaled_to_fit() {
        let (w, h) = ocr_upscaled_size(12_000, 200, Some(10_000));
        assert_eq!(w, 10_000);
        assert!(h < 200, "height {h}");
    }

    // The tall axis counts too: the limit applies to max(w, h).
    #[test]
    fn the_longer_side_drives_the_clamp_on_a_tall_crop() {
        assert_eq!(ocr_upscaled_size(100, 6000, Some(10_000)).1, 10_000);
    }

    // No known limit (the non-Windows arm, or a failed read) leaves the 2×.
    #[test]
    fn an_unknown_limit_leaves_the_full_2x() {
        assert_eq!(ocr_upscaled_size(6880, 96, None), (13_760, 192));
    }
}

#[cfg(test)]
mod affinity_ledger_tests {
    use super::AffinityLedger;
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;

    type Log = Arc<Mutex<Vec<(isize, &'static str)>>>;

    fn acquire(ledger: &AffinityLedger, log: &Log, hwnd: isize, ok: bool) -> bool {
        ledger.acquire(hwnd, || {
            log.lock().unwrap().push((hwnd, "set"));
            ok
        })
    }

    fn release(ledger: &AffinityLedger, log: &Log, hwnd: isize) {
        ledger.release(hwnd, || log.lock().unwrap().push((hwnd, "clear")));
    }

    fn entries(log: &Log) -> Vec<(isize, &'static str)> {
        log.lock().unwrap().clone()
    }

    #[test]
    fn an_overlapping_grab_keeps_the_window_excluded_when_the_first_releases() {
        let (ledger, log) = (AffinityLedger::default(), Log::default());
        acquire(&ledger, &log, 1, true);
        acquire(&ledger, &log, 1, true);
        release(&ledger, &log, 1);
        assert_eq!(entries(&log), vec![(1, "set")], "cleared while a second grab still held the window");
    }

    #[test]
    fn the_last_release_clears_the_window_once() {
        let (ledger, log) = (AffinityLedger::default(), Log::default());
        acquire(&ledger, &log, 1, true);
        acquire(&ledger, &log, 1, true);
        release(&ledger, &log, 1);
        release(&ledger, &log, 1);
        assert_eq!(entries(&log), vec![(1, "set"), (1, "clear")]);
    }

    #[test]
    fn window_handles_are_counted_independently() {
        let (ledger, log) = (AffinityLedger::default(), Log::default());
        acquire(&ledger, &log, 1, true);
        acquire(&ledger, &log, 2, true);
        release(&ledger, &log, 1);
        assert_eq!(entries(&log), vec![(1, "set"), (2, "set"), (1, "clear")]);
    }

    #[test]
    fn a_failed_set_is_refused() {
        let (ledger, log) = (AffinityLedger::default(), Log::default());
        assert!(!acquire(&ledger, &log, 1, false));
    }

    #[test]
    fn a_failed_set_leaves_no_count_behind() {
        let (ledger, log) = (AffinityLedger::default(), Log::default());
        acquire(&ledger, &log, 1, false);
        acquire(&ledger, &log, 1, true);
        release(&ledger, &log, 1);
        assert_eq!(entries(&log), vec![(1, "set"), (1, "set"), (1, "clear")]);
    }

    /// A acquires; B acquires; A releases and is joined; B is parked on the
    /// third barrier. Returns the log as seen while B was parked and the final
    /// log after B released and was joined.
    fn two_threads_overlapping() -> (Vec<(isize, &'static str)>, Vec<(isize, &'static str)>) {
        let ledger = Arc::new(AffinityLedger::default());
        let log = Log::default();
        let barriers: Vec<_> = (0..3).map(|_| Arc::new(Barrier::new(2))).collect();
        let a = {
            let (ledger, log, b1, b2) = (ledger.clone(), log.clone(), barriers[0].clone(), barriers[1].clone());
            thread::spawn(move || {
                acquire(&ledger, &log, 1, true);
                b1.wait();
                b2.wait();
                release(&ledger, &log, 1);
            })
        };
        let b = {
            let (ledger, log, b1, b2, b3) =
                (ledger.clone(), log.clone(), barriers[0].clone(), barriers[1].clone(), barriers[2].clone());
            thread::spawn(move || {
                b1.wait();
                acquire(&ledger, &log, 1, true);
                b2.wait();
                b3.wait();
                release(&ledger, &log, 1);
            })
        };
        a.join().unwrap();
        let while_b_holds = entries(&log);
        barriers[2].wait();
        b.join().unwrap();
        (while_b_holds, entries(&log))
    }

    #[test]
    fn a_grab_on_another_thread_keeps_the_window_excluded_when_the_first_releases() {
        let (while_b_holds, _) = two_threads_overlapping();
        assert!(
            !while_b_holds.contains(&(1, "clear")),
            "cleared while thread B still held the window: {while_b_holds:?}"
        );
    }

    #[test]
    fn the_last_thread_to_release_clears_the_window_once() {
        let (_, after) = two_threads_overlapping();
        assert_eq!(after.iter().filter(|e| **e == (1, "clear")).count(), 1, "{after:?}");
        assert_eq!(after.last(), Some(&(1, "clear")), "{after:?}");
    }
}

#[cfg(test)]
mod notice_gate_tests {
    use super::grab_if_accepted;
    use crate::http::NoticeGate;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Runs one gated grab against a counting stand-in; returns the result.
    fn grab(gate: &NoticeGate, flag: &AtomicBool, grabs: &Cell<u32>, logs: &Cell<u32>) -> Result<&'static str, String> {
        grab_if_accepted(
            gate,
            flag,
            |_| logs.set(logs.get() + 1),
            || {
                grabs.set(grabs.get() + 1);
                Ok("frame")
            },
        )
    }

    #[test]
    fn capture_refuses_while_the_notice_is_not_accepted() {
        let (gate, flag, grabs, logs) = (NoticeGate::default(), AtomicBool::new(false), Cell::new(0), Cell::new(0));
        assert_eq!(grab(&gate, &flag, &grabs, &logs), Err(crate::http::NOTICE_NOT_ACCEPTED.to_string()));
        assert_eq!(grabs.get(), 0);
    }

    /// The accept command opens a clone of the gate the app holds; the next
    /// grab through the original must see it.
    #[test]
    fn capture_reaches_the_grab_once_the_same_gate_opens() {
        let (gate, flag, grabs, logs) = (NoticeGate::default(), AtomicBool::new(false), Cell::new(0), Cell::new(0));
        assert_eq!(grab(&gate, &flag, &grabs, &logs), Err(crate::http::NOTICE_NOT_ACCEPTED.to_string()));
        gate.clone().open();
        assert_eq!(grab(&gate, &flag, &grabs, &logs), Ok("frame"));
        assert_eq!(grabs.get(), 1);
    }

    #[test]
    fn refusal_logs_once() {
        let (gate, flag, grabs, logs) = (NoticeGate::default(), AtomicBool::new(false), Cell::new(0), Cell::new(0));
        let _ = grab(&gate, &flag, &grabs, &logs);
        let _ = grab(&gate, &flag, &grabs, &logs);
        assert_eq!(logs.get(), 1);
        assert!(flag.load(Ordering::SeqCst));
    }

    // --- Guard: the only grab path is `capture::capture_screen` ---

    const GRAB_APIS: [&str; 5] = ["xcap::", "BitBlt", "GetDIBits", "PrintWindow", "GraphicsCaptureItem"];

    fn without_comment_lines(source: &str) -> String {
        source.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n")
    }

    fn grab_apis_in(code: &str) -> Vec<&'static str> {
        GRAB_APIS.into_iter().filter(|api| code.contains(api)).collect()
    }

    /// `capture.rs` production code (before its first test module) with the
    /// `platform` modules cut out.
    fn capture_rs_outside_platform(source: &str) -> String {
        let production = &source[..source.find("#[cfg(test)]").unwrap_or(source.len())];
        let mut out = String::new();
        let mut rest = production;
        while let Some(at) = rest.find("mod platform {") {
            out.push_str(&rest[..at]);
            let body = &rest[at..];
            let mut depth = 0usize;
            let mut end = body.len();
            for (i, c) in body.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            rest = &body[end..];
        }
        out.push_str(rest);
        without_comment_lines(&out)
    }

    #[test]
    fn guard_flags_a_grab_outside_the_platform_module() {
        let source = "mod platform { use xcap::Monitor; fn g() { BitBlt(); } }\npub fn capture_screen() {}\n#[cfg(test)]\nmod t { const X: &str = \"xcap::\"; }\n";
        assert!(grab_apis_in(&capture_rs_outside_platform(source)).is_empty());
        let source = "mod platform { }\nfn sneaky() { xcap::Monitor::all(); }\n";
        assert_eq!(grab_apis_in(&capture_rs_outside_platform(source)), vec!["xcap::"]);
    }

    /// Walks the source tree at test time so a new file is covered without
    /// being listed here.
    #[test]
    fn no_source_grabs_the_screen_outside_capture_platform() {
        fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, files);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&src, &mut files);
        assert!(files.len() > 10, "source walk found only {} files", files.len());
        let offenders: Vec<String> = files
            .iter()
            .filter_map(|path| {
                let source = std::fs::read_to_string(path).unwrap();
                let code = if path.ends_with("capture.rs") {
                    capture_rs_outside_platform(&source)
                } else {
                    without_comment_lines(&source)
                };
                let found = grab_apis_in(&code);
                (!found.is_empty()).then(|| format!("{}: {:?}", path.display(), found))
            })
            .collect();
        assert!(offenders.is_empty(), "grab the screen only via capture::capture_screen: {offenders:#?}");
    }
}
