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
    /// The game window's client rectangle in this capture's physical pixels:
    /// x, y, w, h. Built only by [`client_in_capture`] from the live client
    /// rect the focus poller last read; the whole image while that is unknown
    /// or has no intersection with this capture (another display).
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

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn refresh_changed_client(
    previous: Option<[i32; 4]>,
    next: [i32; 4],
    resolve_monitor: impl FnOnce(),
    accept_client: impl FnOnce(),
) {
    if previous == Some(next) {
        return;
    }
    resolve_monitor();
    accept_client();
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
/// A plain read of the owner. Focus entry and changed client geometry refresh
/// it, so this command does not enumerate displays on every Settings poll.
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

/// The game's client rect made relative to a capture of the monitor at
/// `origin` sized `size`. `game_client` is virtual-desktop physical px
/// `[x, y, w, h]`. Keep its nonempty intersection with the capture; an
/// unknown rect or empty intersection yields the whole capture so OCR always
/// crops within the captured pixels.
#[cfg_attr(not(windows), allow(dead_code))]
fn client_in_capture(game_client: Option<[i32; 4]>, origin: (i32, i32), size: (u32, u32)) -> [i32; 4] {
    #[cfg(windows)]
    return platform::selected_client(game_client, origin, size);
    #[cfg(not(windows))]
    untrimmed_client_in_capture(game_client, origin, size)
}

fn untrimmed_client_in_capture(game_client: Option<[i32; 4]>, origin: (i32, i32), size: (u32, u32)) -> [i32; 4] {
    let (cw, ch) = (size.0 as i64, size.1 as i64);
    let whole = [0, 0, size.0 as i32, size.1 as i32];
    let Some([x, y, w, h]) = game_client else { return whole };
    let (rx, ry) = (x as i64 - origin.0 as i64, y as i64 - origin.1 as i64);
    let (w64, h64) = (w as i64, h as i64);
    let (left, top) = (rx.max(0), ry.max(0));
    let (right, bottom) = ((rx + w64).min(cw), (ry + h64).min(ch));
    if w64 > 0 && h64 > 0 && right > left && bottom > top {
        [left as i32, top as i32, (right - left) as i32, (bottom - top) as i32]
    } else {
        whole
    }
}

#[derive(Default)]
#[cfg_attr(not(windows), allow(dead_code))]
struct ClientDiagnostics {
    reported: std::sync::Mutex<std::collections::HashSet<([i32; 4], (i32, i32), (u32, u32))>>,
}

#[cfg_attr(not(windows), allow(dead_code))]
fn select_client_with_diagnostic(
    diagnostics: &ClientDiagnostics,
    raw: Option<[i32; 4]>,
    origin: (i32, i32),
    size: (u32, u32),
    select: impl FnOnce() -> [i32; 4],
    log: impl FnOnce(String),
) -> [i32; 4] {
    let client = select();
    if let Some(rect) = raw {
        if !client_fits_capture(rect, origin, size) {
            let first = diagnostics.reported.lock().unwrap_or_else(|e| e.into_inner())
                .insert((rect, origin, size));
            if first {
                log(format!(
                    "capture: game client {rect:?} (desktop px), monitor origin {origin:?} size {}x{} -> client used {client:?} (client outside the capture)",
                    size.0, size.1,
                ));
            }
        }
    }
    client
}

fn client_fits_capture([x, y, w, h]: [i32; 4], origin: (i32, i32), size: (u32, u32)) -> bool {
    w > 0 && h > 0 && untrimmed_client_in_capture(Some([x, y, w, h]), origin, size).map(i64::from)
        == [i64::from(x) - i64::from(origin.0), i64::from(y) - i64::from(origin.1), i64::from(w), i64::from(h)]
}

#[cfg_attr(not(windows), allow(dead_code))]
fn rendered_area(image: &image::RgbaImage) -> Option<[i32; 4]> {
    let (width, height) = image.dimensions();
    if width < 1024 || height < 768 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return None;
    }
    let dark = |x, y| image.get_pixel(x, y).0[..3].iter().all(|&v| v <= 12);
    let column = |x| (0..32).all(|n| dark(x, (u64::from(height - 1) * n / 31) as u32));
    let row = |y| (0..32).all(|n| dark((u64::from(width - 1) * n / 31) as u32, y));
    let left = (0..width).find(|&x| !column(x))?;
    let right = (0..width).find(|&x| !column(width - 1 - x))?;
    let top = (0..height).find(|&y| !row(y))?;
    let bottom = (0..height).find(|&y| !row(height - 1 - y))?;
    let w = width.checked_sub(left + right)?;
    let h = height.checked_sub(top + bottom)?;
    if w < 1024 || h < 768 {
        return None;
    }
    let (w64, h64) = (u64::from(w), u64::from(h));
    let supported = match (left > 0 || right > 0, top > 0 || bottom > 0) {
        (true, false) => left.abs_diff(right) <= 2
            && u64::from(left.min(right)) * 100 >= u64::from(width)
            && w64 * 3 >= h64 * 4 && w64 * u64::from(height) <= h64 * u64::from(width),
        (false, true) => top.abs_diff(bottom) <= 2
            && u64::from(top.min(bottom)) * 100 >= u64::from(height)
            && w64 * u64::from(height) >= h64 * u64::from(width) && w64 * 9 <= h64 * 32,
        _ => false,
    };
    supported.then_some([left as i32, top as i32, w as i32, h as i32])
}

#[derive(Default)]
#[cfg_attr(not(windows), allow(dead_code))]
struct RenderedClientLatch {
    key: Option<((i32, i32), Option<[i32; 4]>)>,
    accepted: Option<([i32; 4], (u32, u32))>,
    pending: Option<([i32; 4], (u32, u32))>,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl RenderedClientLatch {
    fn select(&self, raw: Option<[i32; 4]>, origin: (i32, i32), size: (u32, u32)) -> [i32; 4] {
        let untrimmed = untrimmed_client_in_capture(raw, origin, size);
        if self.key == Some((origin, raw)) && untrimmed == [0, 0, size.0 as i32, size.1 as i32] {
            if let Some((rect, captured_size)) = self.accepted {
                if captured_size == size {
                    return rect;
                }
            }
        }
        untrimmed
    }

    fn observe(
        &mut self,
        raw: Option<[i32; 4]>,
        origin: (i32, i32),
        size: (u32, u32),
        probe: impl FnOnce() -> Option<[i32; 4]>,
    ) -> [i32; 4] {
        if self.key != Some((origin, raw)) {
            *self = Self { key: Some((origin, raw)), ..Self::default() };
        }
        if untrimmed_client_in_capture(raw, origin, size) == [0, 0, size.0 as i32, size.1 as i32] {
            let candidate = probe().map(|rect| (rect, size));
            if candidate.is_some() && candidate == self.pending {
                self.accepted = candidate;
            }
            self.pending = candidate;
        } else {
            self.pending = None;
        }
        self.select(raw, origin, size)
    }
}

/// The Recalibrate log line naming the raw game client read, the captured
/// monitor's origin and size, and the client the measurement actually used.
pub(crate) fn recalibrate_client_diagnostic(
    raw: Option<[i32; 4]>,
    origin: (i32, i32),
    size: (u32, u32),
    client: [i32; 4],
) -> String {
    let selected = untrimmed_client_in_capture(raw, origin, size);
    let why = match raw {
        None => "no client read",
        Some(rect) => {
            if client_fits_capture(rect, origin, size) {
                "client inside the capture"
            } else {
                "client outside the capture"
            }
        }
    };
    let trim = if selected == [0, 0, size.0 as i32, size.1 as i32] && client != selected {
        "; rendered-area trim"
    } else {
        ""
    };
    format!(
        "Recalibrate: game client {raw:?} (desktop px), monitor origin {origin:?} size {}x{} -> client used {client:?} ({why}{trim})",
        size.0, size.1
    )
}

#[cfg_attr(not(windows), allow(dead_code))]
fn capture_exclusion_labels() -> &'static [&'static str] {
    &[]
}

#[cfg_attr(not(windows), allow(dead_code))]
fn capture_with_exclusions<T>(
    _windows: &[isize],
    _acquire: impl FnMut(isize) -> bool,
    _release: impl FnMut(isize),
    _grab: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    Err(String::new())
}

#[cfg(windows)]
mod platform {
    use super::{client_in_capture, refresh_changed_client, rendered_area, select_client_with_diagnostic, untrimmed_client_in_capture, AffinityLedger, Capture, ClientDiagnostics, GameMonitor, GrabTiming, RenderedClientLatch};
    use std::sync::Mutex;
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
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
    ///
    /// The read prompt (`prompt`, POE-388) is up exactly when a read key press
    /// is expected, so it is listed rather than placed clear of every module's
    /// read regions (ADR-023).
    const EXCLUDED_WHILE_GRABBING: &[&str] = &["mercenary", "overlay-preview", "overlay-region-editor", "loot", "prompt"];

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

    /// The game window's client rect, virtual-desktop physical px
    /// `[x, y, w, h]`, as [`remember_game_client`] last read it. Read and
    /// written only in this module.
    // The revision rejects a grab spanning a raw-client or display change,
    // including a change away and back to the same rectangle.
    static GAME_CLIENT: Mutex<(Option<[i32; 4]>, u64)> = Mutex::new((None, 0));
    static RENDERED_CLIENT: LazyLock<Mutex<RenderedClientLatch>> =
        LazyLock::new(|| Mutex::new(RenderedClientLatch::default()));
    static CLIENT_DIAGNOSTICS: LazyLock<ClientDiagnostics> = LazyLock::new(ClientDiagnostics::default);

    pub(super) fn selected_client(raw: Option<[i32; 4]>, origin: (i32, i32), size: (u32, u32)) -> [i32; 4] {
        RENDERED_CLIENT.lock().unwrap_or_else(|e| e.into_inner()).select(raw, origin, size)
    }

    pub(crate) fn raw_game_client() -> Option<[i32; 4]> {
        GAME_CLIENT.lock().unwrap_or_else(|e| e.into_inner()).0
    }

    pub(crate) fn refresh_game_client(app: &AppHandle) {
        use windows::core::w;
        use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

        if let Ok(hwnd) = unsafe { FindWindowW(w!("POEWindowClass"), None) } {
            remember_game_client(app, hwnd);
        }
    }

    static CLIENT_READ_FAILURE_REPORTED: AtomicBool = AtomicBool::new(false);
    static CLIENT_READ_TIMED: AtomicBool = AtomicBool::new(false);

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
        let (game_client, revision) = *GAME_CLIENT.lock().unwrap_or_else(|e| e.into_inner());
        let phase = Instant::now();
        let grabbed = monitor.capture_image();
        timing.grab_ms = ms(phase);
        let phase = Instant::now();
        drop(excluded);
        timing.restore_ms = ms(phase);
        let img = grabbed.map_err(|e| format!("Screen capture failed: {}", e))?;

        let size = img.dimensions();
        // Lock order is monitor, raw client, rendered client. No lock spans an
        // overlay event-loop round trip in ExcludedFromCapture.
        let client = select_client_with_diagnostic(
            &CLIENT_DIAGNOSTICS, game_client, origin, size,
            || {
                let state = app.state::<crate::AppState>();
                let current_monitor = state.game_monitor.lock().unwrap_or_else(|e| e.into_inner());
                let current_client = GAME_CLIENT.lock().unwrap_or_else(|e| e.into_inner());
                if *current_monitor == target && *current_client == (game_client, revision) {
                    RENDERED_CLIENT.lock().unwrap_or_else(|e| e.into_inner())
                        .observe(game_client, origin, size, || rendered_area(&img))
                } else {
                    untrimmed_client_in_capture(game_client, origin, size)
                }
            },
            |msg| crate::app_log(app, msg),
        );
        timing.call_ms = ms(call_started);
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
    /// (POE-237). Called on focus entry and when the client rectangle changes.
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
                if current.map(|mon| (mon.x, mon.y)) != Some((next.x, next.y)) {
                    let mut client = GAME_CLIENT.lock().unwrap_or_else(|e| e.into_inner());
                    client.1 = client.1.wrapping_add(1);
                    *RENDERED_CLIENT.lock().unwrap_or_else(|e| e.into_inner()) = RenderedClientLatch::default();
                }
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

    /// Read `hwnd`'s client rect (`GetClientRect` + `ClientToScreen` on its
    /// origin) and remember it as the game's. On a change, resolves the monitor
    /// before re-checking the published slice: a move re-places it, a resize
    /// drops it (`ssot::drop_if_mismatched`). A failed read or an empty
    /// rect keeps the last value. The first successful read's duration is
    /// logged once per session — the cost the per-tick refresh rests on.
    pub fn remember_game_client(app: &AppHandle, hwnd: HWND) {
        let started = Instant::now();
        let mut rect = RECT::default();
        let mut corner = POINT { x: 0, y: 0 };
        let ok = unsafe { GetClientRect(hwnd, &mut rect) }.is_ok()
            && unsafe { ClientToScreen(hwnd, &mut corner) }.as_bool();
        let elapsed = started.elapsed();
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        if !ok || w <= 0 || h <= 0 {
            if !CLIENT_READ_FAILURE_REPORTED.swap(true, Ordering::Relaxed) {
                crate::app_log(
                    app,
                    "capture: could not read the game window's client rect — keeping the last known one"
                        .to_string(),
                );
            }
            return;
        }
        if !CLIENT_READ_TIMED.swap(true, Ordering::Relaxed) {
            crate::app_log(app, format!("capture: game client read took {} µs", elapsed.as_micros()));
        }

        let abs = [corner.x, corner.y, w, h];
        let previous = raw_game_client();
        refresh_changed_client(previous, abs, || remember_game_monitor(app, hwnd), || {
            {
                let mut held = GAME_CLIENT.lock().unwrap_or_else(|e| e.into_inner());
                if held.0 == Some(abs) {
                    return;
                }
                *held = (Some(abs), held.1.wrapping_add(1));
                *RENDERED_CLIENT.lock().unwrap_or_else(|e| e.into_inner()) = RenderedClientLatch::default();
            }
            let monitor = *app
                .state::<crate::AppState>()
                .game_monitor
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(mon) = monitor {
                let origin = (mon.x, mon.y);
                let size = (mon.width, mon.height);
                let client = select_client_with_diagnostic(
                    &CLIENT_DIAGNOSTICS, Some(abs), origin, size,
                    || client_in_capture(Some(abs), origin, size),
                    |msg| crate::app_log(app, msg),
                );
                crate::ssot::drop_if_mismatched(app, size, origin, client);
            }
        });
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Capture;
    use tauri::AppHandle;

    pub(crate) fn raw_game_client() -> Option<[i32; 4]> {
        None
    }

    pub(crate) fn refresh_game_client(_app: &AppHandle) {}

    /// No capture off Windows, exactly as before POE-237 — the whole OCR half
    /// of the app is Windows-only and every caller already treats this `Err`
    /// as the normal answer on a dev machine.
    pub(super) fn grab_screen(_app: &AppHandle) -> Result<Capture, String> {
        Err("Screen capture not available on this platform".to_string())
    }
}

#[cfg(windows)]
pub use platform::*;

pub(crate) use platform::{raw_game_client, refresh_game_client};

/// Set by the first refused grab; nothing re-arms it, so a session that never
/// accepts logs the refusal once, not once per tick.
static REFUSAL_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Why [`capture_screen`] returned no frame.
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureError {
    /// The Data & privacy notice is not accepted; nothing was grabbed and the
    /// refusal is already logged (once per process).
    NotAccepted,
    /// The grab itself failed.
    Failed(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::NotAccepted => f.write_str(crate::http::NOTICE_NOT_ACCEPTED),
            CaptureError::Failed(e) => f.write_str(e),
        }
    }
}

impl From<CaptureError> for String {
    fn from(e: CaptureError) -> String {
        e.to_string()
    }
}

/// Grab the screen, the only grab path in the app (POE-395, ADR-033): while
/// the Data & privacy notice is not accepted it grabs nothing and returns
/// [`CaptureError::NotAccepted`].
pub fn capture_screen(app: &tauri::AppHandle) -> Result<Capture, CaptureError> {
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
) -> Result<T, CaptureError> {
    if !gate.is_open() {
        if !refusal_logged.swap(true, std::sync::atomic::Ordering::SeqCst) {
            log("screen capture refused until the Data & privacy notice is accepted".to_string());
        }
        return Err(CaptureError::NotAccepted);
    }
    grab().map_err(CaptureError::Failed)
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

/// The exchange-row preparation (POE-477): luma, contrast stretch, Lanczos3
/// 3x, inverted to dark text on white, framed by a 40 px white pad, clamped to
/// the OCR engine's size limit.
pub fn preprocess_for_ocr_rows(img: &image::DynamicImage) -> image::DynamicImage {
    preprocess_for_ocr_rows_within(img, crate::ocr::max_image_dimension())
}

/// [`preprocess_for_ocr_rows`] under an explicit engine limit, padding included.
pub fn preprocess_for_ocr_rows_within(img: &image::DynamicImage, limit: Option<u32>) -> image::DynamicImage {
    let contrasted = contrast_stretched(img);
    let (w, h) = contrasted.dimensions();
    let pad = limit.map_or(40, |limit| 40.min(limit.saturating_sub(1) / 2));
    let factor = limit.map_or(3.0, |limit| {
        ((limit.saturating_sub(2 * pad)) as f64 / w.max(h).max(1) as f64).min(3.0)
    });
    let out_w = (w as f64 * factor).floor().max(1.0) as u32;
    let out_h = (h as f64 * factor).floor().max(1.0) as u32;
    let mut scaled = image::imageops::resize(&contrasted, out_w, out_h, image::imageops::FilterType::Lanczos3);
    image::imageops::invert(&mut scaled);
    let mut padded = image::GrayImage::from_pixel(out_w + 2 * pad, out_h + 2 * pad, image::Luma([255]));
    image::imageops::replace(&mut padded, &scaled, pad.into(), pad.into());
    image::DynamicImage::ImageLuma8(padded)
}

fn contrast_stretched(img: &image::DynamicImage) -> image::GrayImage {
    let mut contrasted = img.to_luma8();
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

    contrasted
}

fn preprocess_with(
    img: &image::DynamicImage,
    filter: image::imageops::FilterType,
) -> image::DynamicImage {
    let contrasted = contrast_stretched(img);
    let (w, h) = contrasted.dimensions();

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

    fn bordered_frame(size: (u32, u32), bars: [u32; 4], dark: u8) -> image::RgbaImage {
        let [left, top, right, bottom] = bars;
        image::RgbaImage::from_fn(size.0, size.1, |x, y| {
            if x < left || x >= size.0 - right || y < top || y >= size.1 - bottom {
                image::Rgba([dark, dark, dark, 255])
            } else {
                image::Rgba([48, 96, 144, 255])
            }
        })
    }

    mod keep_happy {
        use super::super::{client_in_capture, recalibrate_client_diagnostic, rendered_area, RenderedClientLatch};
        use super::bordered_frame;

        #[test]
        fn a_pillarboxed_fullscreen_frame_trims_the_client_to_the_rendered_area() {
            let mut latch = RenderedClientLatch::default();
            let windowed = Some([2080, 90, 1600, 900]);
            assert_eq!(
                latch.observe(windowed, (1920, 0), (1920, 1080), || panic!("windowed clients must skip the pixel probe")),
                [160, 90, 1600, 900]
            );
            let frame = bordered_frame((3840, 1080), [640, 0, 640, 0], 0);
            let raw = Some([0, 0, 3840, 1080]);
            assert_eq!(latch.observe(raw, (0, 0), frame.dimensions(), || rendered_area(&frame)), [0, 0, 3840, 1080]);
            assert_eq!(latch.observe(raw, (0, 0), frame.dimensions(), || rendered_area(&frame)), [640, 0, 2560, 1080]);
        }

        #[test]
        fn a_letterboxed_frame_trims_rows() {
            let mut latch = RenderedClientLatch::default();
            let frame = bordered_frame((2560, 1440), [0, 180, 0, 180], 12);
            let raw = Some([0, 0, 2560, 1440]);
            assert_eq!(latch.observe(raw, (0, 0), frame.dimensions(), || rendered_area(&frame)), [0, 0, 2560, 1440]);
            assert_eq!(latch.observe(raw, (0, 0), frame.dimensions(), || rendered_area(&frame)), [0, 180, 2560, 1080]);
        }

        #[test]
        fn a_windowed_client_is_carried_capture_relative() {
            let client = client_in_capture(Some([2080, 90, 1600, 900]), (1920, 0), (1920, 1080));
            assert_eq!(client, [160, 90, 1600, 900]);
        }

        #[test]
        fn recalibrate_diagnostic_reports_an_inside_client_as_used() {
            let line = recalibrate_client_diagnostic(
                Some([0, 0, 2560, 1080]),
                (0, 0),
                (2560, 1440),
                [0, 0, 2560, 1080],
            );
            assert_eq!(
                line,
                "Recalibrate: game client Some([0, 0, 2560, 1080]) (desktop px), monitor origin (0, 0) size 2560x1440 -> client used [0, 0, 2560, 1080] (client inside the capture)"
            );
            let line = recalibrate_client_diagnostic(
                Some([0, 0, 3840, 1080]), (0, 0), (3840, 1080), [640, 0, 2560, 1080],
            );
            assert!(line.contains("client used [640, 0, 2560, 1080]"), "line: {line:?}");
            assert!(line.contains("trim"), "active trim must be named: {line:?}");
        }
    }

    mod keep_regression {
        use super::super::{
            rendered_area, select_client_with_diagnostic, untrimmed_client_in_capture,
            ClientDiagnostics, RenderedClientLatch,
        };

        #[test]
        fn windowed_client_fallbacks_report_each_distinct_rectangle_once() {
            let origin = (1920, 0);
            let size = (1920, 1080);
            let source = image::load_from_memory(include_bytes!(
                "../tests/fixtures/lab/font-gems-1080p.png"
            )).unwrap();
            let mut failures = Vec::new();
            for (w, h) in [(1024, 576), (1280, 720), (1366, 768)] {
                let game = source.resize_exact(w, h, image::imageops::FilterType::Lanczos3).to_rgba8();
                let inside = [2020, 100, w as i32, h as i32];
                let partial = [3500, 90, w as i32, h as i32];
                let outside = [100, 90, w as i32, h as i32];
                let clipped = [1580, 90, 340, h as i32];
                let whole = [0, 0, 1920, 1080];
                for path in ["grab observation", "selected client", "raced grab fallback"] {
                    let diagnostics = ClientDiagnostics::default();
                    let mut latch = RenderedClientLatch::default();
                    let mut lines = Vec::new();
                    for (raw, expected, log_count) in [
                        (inside, [100, 100, w as i32, h as i32], 0),
                        (partial, clipped, 1),
                        (partial, clipped, 1),
                        (outside, whole, 2),
                        (outside, whole, 2),
                        (partial, clipped, 2),
                    ] {
                        let mut frame = image::RgbaImage::from_pixel(
                            size.0, size.1, image::Rgba([90, 90, 90, 255]),
                        );
                        image::imageops::overlay(
                            &mut frame, &game,
                            i64::from(raw[0]) - i64::from(origin.0), i64::from(raw[1]),
                        );
                        let client = select_client_with_diagnostic(
                            &diagnostics, Some(raw), origin, frame.dimensions(),
                            || match path {
                                "grab observation" => latch.observe(
                                    Some(raw), origin, size, || rendered_area(&frame),
                                ),
                                "selected client" => latch.select(Some(raw), origin, size),
                                _ => untrimmed_client_in_capture(Some(raw), origin, size),
                            },
                            |line| lines.push(line),
                        );
                        if client != expected || lines.len() != log_count {
                            failures.push(format!(
                                "{w}x{h} {path} raw {raw:?}: client {client:?}, expected {expected:?}; diagnostics {}, expected {log_count}",
                                lines.len(),
                            ));
                        }
                    }
                    for (index, (raw, used)) in [(partial, clipped), (outside, whole)].into_iter().enumerate() {
                        let Some(line) = lines.get(index) else {
                            failures.push(format!("{w}x{h} {path}: missing diagnostic for {raw:?}"));
                            continue;
                        };
                        for value in [format!("{raw:?}"), format!("{origin:?}"), "1920x1080".to_string(), format!("{used:?}")] {
                            if !line.contains(&value) {
                                failures.push(format!("{path}: diagnostic {line:?} omits geometry {value}"));
                            }
                        }
                    }
                }
            }
            assert!(failures.is_empty(), "windowed client fallback contract:\n{}", failures.join("\n"));
        }
    }

    mod keep_critical {
        use super::super::{client_in_capture, recalibrate_client_diagnostic, rendered_area, RenderedClientLatch};
        use super::bordered_frame;

        struct PlacementFrame {
            name: &'static str,
            png: &'static [u8],
            size: (u32, u32),
            raw: [i32; 4],
            client: [i32; 4],
            panel_sides: [i32; 2],
            body_sides: [i32; 2],
            panel_top: i32,
            first_option: i32,
            lowest_text: i32,
            slots: [[i32; 4]; 3],
        }

        // Current fixture behavior and full derivation: tests/fixtures/ultrawide-placement/README.md.
        // Observed source rasters: 3742x1076 and 2556x1440. All coordinates below
        // were measured in those rasters, without scaling or translating pixels.
        // Pillarbox assumes a 3840x1076 monitor: restore 98 right-bar pixels,
        // since 3742+98=3840 and the measured right bar 526+98=624 (inferred crop).
        // Its synthetic game top row x=624..3215, y=0 is made non-dark, expressly
        // normalizing the source's sampled dark row; retain the 1076 height.
        // Both reconstructions remove all text/art and overlay-preview (excluded
        // from grabs). Removing the main app windows and letterbox taskbar is a
        // disclosed scenario choice: these windows are not excluded by capture.
        // Panel sides are the straight outer frame, body sides its inner column;
        // panel_top is the outer top; first_option is the gem-choice dialog top.
        // No Crafts Remaining is drawn: lowest_text is the CRAFT label bottom.
        // Slots measure the black icon squares inside their rounded metallic
        // bevels, not the surrounding 76px frames or the cyan preview boxes.
        // Tolerance 5px is fixed from the blurred 4-6px bevel/edge bands visible
        // on these source rasters, before evaluating the production resolver.
        // These oracles cover the drawn three-gem dialog, not six-option text.
        fn placement_frames() -> [PlacementFrame; 2] {
            [
                PlacementFrame {
                    name: "pillarbox",
                    png: include_bytes!("../tests/fixtures/ultrawide-placement/pillarbox.png"),
                    size: (3840, 1076),
                    raw: [0, 0, 3840, 1076],
                    client: [624, 0, 2592, 1076],
                    panel_sides: [1245, 1927],
                    body_sides: [1255, 1917],
                    panel_top: 161,
                    first_option: 338,
                    lowest_text: 642,
                    slots: [[1463, 371, 50, 50], [1564, 371, 50, 50], [1664, 371, 50, 50]],
                },
                PlacementFrame {
                    name: "letterbox",
                    png: include_bytes!("../tests/fixtures/ultrawide-placement/letterbox.png"),
                    size: (2556, 1440),
                    // Observed at unobscured x=100: game through row 1079,
                    // black from row 1080. Assume that inside raw client;
                    // the original player's actual window rect is unknown.
                    raw: [0, 0, 2556, 1080],
                    client: [0, 0, 2556, 1080],
                    panel_sides: [603, 1283],
                    body_sides: [613, 1273],
                    panel_top: 190,
                    first_option: 343,
                    lowest_text: 619,
                    slots: [[819, 374, 50, 50], [919, 374, 50, 50], [1019, 374, 50, 50]],
                },
            ]
        }

        fn captured_placement(case: &PlacementFrame) -> (RenderedClientLatch, crate::ssot::regions::Resolved) {
            let frame = image::load_from_memory(case.png).unwrap().to_rgba8();
            assert_eq!(frame.dimensions(), case.size, "{} capture raster", case.name);
            let mut latch = RenderedClientLatch::default();
            latch.observe(Some(case.raw), (0, 0), case.size, || rendered_area(&frame));
            let client = latch.observe(Some(case.raw), (0, 0), case.size, || rendered_area(&frame));
            (latch, auto_placement(case, client))
        }

        fn auto_placement(case: &PlacementFrame, client: [i32; 4]) -> crate::ssot::regions::Resolved {
            let slice = crate::ssot::screen_from_geometry(case.size.0, case.size.1, 7, (0, 0), client, 123);
            crate::ssot::regions::resolve(
                Some(&slice), &crate::ssot::regions::OcrRegionsSetting::default(), Some(case.size),
            )
        }

        /// Silent consequence: OCR reads the wrong pixels on ultrawide and letterboxed setups.
        #[test]
        fn measured_capture_frames_select_the_rendered_client() {
            for case in placement_frames() {
                let frame = image::load_from_memory(case.png).unwrap().to_rgba8();
                let mut latch = RenderedClientLatch::default();
                let probe = rendered_area(&frame);
                let expected_probe = (case.name == "pillarbox").then_some(case.client);
                assert_eq!(probe, expected_probe, "{} measured pixel trim", case.name);
                assert_eq!(latch.observe(Some(case.raw), (0, 0), case.size, || rendered_area(&frame)), case.raw,
                    "{} first observation", case.name);
                assert_eq!(latch.observe(Some(case.raw), (0, 0), case.size, || rendered_area(&frame)), case.client,
                    "{} second observation", case.name);
                if case.name == "letterbox" {
                    let whole = [0, 0, 2556, 1440];
                    let mut unsupported = RenderedClientLatch::default();
                    for _ in 0..2 {
                        assert_eq!(unsupported.observe(Some(whole), (0, 0), case.size, || rendered_area(&frame)), whole,
                            "top-aligned asymmetric bars cannot trim a whole-monitor client");
                    }
                }
            }
        }

        fn font_covers_content(case: &PlacementFrame, [x, y, w, h]: [i32; 4]) -> bool {
            let tolerance = 5;
            x >= case.panel_sides[0] - tolerance
                && x + w <= case.panel_sides[1] + tolerance
                && x <= case.body_sides[0] + tolerance
                && x + w >= case.body_sides[1] - tolerance
                && y > case.panel_top
                && y <= case.first_option + tolerance
                && y + h >= case.lowest_text - tolerance
        }

        /// Silent consequence: OCR reads the wrong pixels on ultrawide and letterboxed setups.
        #[test]
        fn measured_auto_font_covers_the_panel_body() {
            // Read-only known-good control, independently measured at native pixels:
            // straight outer frame x=287..968, top=214; body x=297..958;
            // gem dialog top=343; CRAFT bottom=592 (no Crafts Remaining).
            // Its title band ends around y=265, which is not an OCR top bound.
            let reference = PlacementFrame {
                name: "accepted-1080p",
                png: include_bytes!("../tests/fixtures/lab/font-gems-1080p.png"),
                size: (1920, 1080),
                raw: [0, 0, 1920, 1080],
                client: [0, 0, 1920, 1080],
                panel_sides: [287, 968],
                body_sides: [297, 958],
                panel_top: 214,
                first_option: 343,
                lowest_text: 592,
                // Only Font coverage is checked on the reference.
                slots: [[0; 4]; 3],
            };
            for case in placement_frames().into_iter().chain([reference]) {
                let (_, resolved) = captured_placement(&case);
                let font = resolved.placements.lab.font;
                // Reuse precisely the same containment oracle for real placements
                // and a crop whose top cuts the independently measured first content.
                let cut_top = case.first_option + 6;
                let cut = [font[0], cut_top, font[2], font[1] + font[3] - cut_top];
                for (label, rect, expected) in [("Auto", font, true), ("content cut", cut, false)] {
                    assert_eq!(font_covers_content(&case, rect), expected,
                        "PLACEMENT-BUG POE-501 {} {label} Font {rect:?}: expected coverage {expected}; panel {:?}, body {:?}, top {} < crop <= first content {} (+5px), bottom >= {} (-5px)",
                        case.name, case.panel_sides, case.body_sides, case.panel_top, case.first_option, case.lowest_text);
                }
            }
        }

        /// Silent consequence: OCR reads the wrong pixels on ultrawide and letterboxed setups.
        #[test]
        fn measured_auto_slots_cover_the_three_icon_squares() {
            for case in placement_frames() {
                let (_, resolved) = captured_placement(&case);
                for (index, (actual, measured)) in resolved.placements.lab.gem_slots.iter().zip(case.slots).enumerate() {
                    let edges = |[x, y, w, h]: [i32; 4]| [x, y, x + w, y + h];
                    for (edge, (actual_edge, measured_edge)) in edges(*actual).into_iter().zip(edges(measured)).enumerate() {
                        assert!((actual_edge - measured_edge).abs() <= 5,
                            "PLACEMENT-BUG POE-501 {} slot {index} edge {edge}: actual {actual:?}, measured {measured:?}, tolerance 5",
                            case.name);
                    }
                }
            }
        }

        /// Silent consequence: OCR reads the wrong pixels on ultrawide and letterboxed setups.
        #[test]
        fn recalibrate_preserves_measured_capture_placement() {
            for case in placement_frames() {
                let (latch, captured) = captured_placement(&case);
                let client = latch.select(Some(case.raw), (0, 0), case.size);
                assert_eq!(client, case.client, "{} Recalibrate selected client", case.name);
                let recalibrated = auto_placement(&case, client);
                assert_eq!(recalibrated.placements.lab.font, captured.placements.lab.font, "{} Font parity", case.name);
                assert_eq!(recalibrated.placements.lab.gem_slots, captured.placements.lab.gem_slots, "{} slot parity", case.name);
            }
        }

        /// Silent consequence: OCR reads the wrong pixels on ultrawide and letterboxed setups.
        #[test]
        fn recalibrate_diagnostic_identifies_the_measured_client() {
            for case in placement_frames() {
                let (latch, _) = captured_placement(&case);
                let client = latch.select(Some(case.raw), (0, 0), case.size);
                let line = recalibrate_client_diagnostic(Some(case.raw), (0, 0), case.size, client);
                let expected = if case.name == "pillarbox" {
                    "client used [624, 0, 2592, 1076] (client inside the capture; rendered-area trim)"
                } else {
                    "client used [0, 0, 2556, 1080] (client inside the capture)"
                };
                assert!(line.contains(expected), "{} diagnostic: {line}", case.name);
            }
        }

        /// Silent consequence: trusted crops move into black bars or reuse another display's geometry.
        #[test]
        fn an_all_black_frame_keeps_the_latched_client() {
            let mut latch = RenderedClientLatch::default();
            let size = (3840, 1080);
            let raw = Some([0, 0, 3840, 1080]);
            let frame = bordered_frame(size, [640, 0, 640, 0], 0);
            let black = image::RgbaImage::from_pixel(size.0, size.1, image::Rgba([0, 0, 0, 255]));
            assert_eq!(latch.observe(raw, (0, 0), size, || rendered_area(&frame)), [0, 0, 3840, 1080]);
            assert_eq!(latch.observe(raw, (0, 0), size, || rendered_area(&black)), [0, 0, 3840, 1080]);
            assert_eq!(latch.observe(raw, (0, 0), size, || rendered_area(&frame)), [0, 0, 3840, 1080]);
            assert_eq!(latch.observe(raw, (0, 0), size, || rendered_area(&frame)), [640, 0, 2560, 1080]);
            assert_eq!(latch.observe(raw, (0, 0), size, || rendered_area(&black)), [640, 0, 2560, 1080]);

            let other = bordered_frame(size, [960, 0, 960, 0], 0);
            assert_eq!(latch.observe(raw, (0, 0), size, || rendered_area(&other)), [640, 0, 2560, 1080]);
            assert_eq!(latch.observe(raw, (0, 0), size, || rendered_area(&black)), [640, 0, 2560, 1080]);
            assert_eq!(latch.observe(raw, (0, 0), size, || rendered_area(&other)), [640, 0, 2560, 1080]);
            assert_eq!(latch.observe(raw, (0, 0), size, || rendered_area(&other)), [960, 0, 1920, 1080]);

            let changed_raw = Some([0, 0, 4000, 1080]);
            assert_eq!(latch.observe(changed_raw, (0, 0), size, || rendered_area(&frame)), [0, 0, 3840, 1080]);
            assert_eq!(latch.observe(changed_raw, (0, 0), size, || rendered_area(&frame)), [640, 0, 2560, 1080]);
            assert_eq!(latch.observe(changed_raw, (-3840, 0), size, || rendered_area(&frame)), [0, 0, 3840, 1080]);
            assert_eq!(latch.observe(changed_raw, (-3840, 0), size, || rendered_area(&frame)), [640, 0, 2560, 1080]);
            assert_eq!(latch.select(changed_raw, (-3840, 0), (1920, 1080)), [0, 0, 1920, 1080]);
        }

        /// Silent consequence: a false trim supplies plausible wrong reader geometry.
        #[test]
        fn asymmetric_dark_edges_are_not_trimmed() {
            let cases = [
                ("asymmetric columns", (3840, 1080), [640, 0, 630, 0]),
                ("asymmetric rows", (2560, 1440), [0, 180, 0, 170]),
                ("tiny columns", (3840, 1080), [20, 0, 20, 0]),
                ("tiny rows", (2560, 1440), [0, 10, 0, 10]),
                ("pillar aspect", (3840, 1080), [1300, 0, 1300, 0]),
                ("letter aspect", (3840, 2160), [0, 600, 0, 600]),
                ("width floor", (1200, 768), [100, 0, 100, 0]),
                ("height floor", (1600, 900), [0, 100, 0, 100]),
                ("both axes", (3840, 1440), [640, 180, 640, 180]),
            ];
            for (name, size, bars) in cases {
                let frame = bordered_frame(size, bars, 0);
                let whole = [0, 0, size.0 as i32, size.1 as i32];
                let mut latch = RenderedClientLatch::default();
                let probe = rendered_area(&frame);
                assert_eq!(probe, None, "{name}: unsupported border must not produce a trim");
                assert_eq!(latch.observe(Some(whole), (0, 0), size, || rendered_area(&frame)), whole, "{name}: first frame");
                assert_eq!(latch.observe(Some(whole), (0, 0), size, || rendered_area(&frame)), whole, "{name}: second frame");
            }
        }

        /// Silent consequence: the recheck drops a matching screen slot and forgets its settings.
        #[test]
        fn the_client_change_recheck_uses_the_latched_trim() {
            let mut latch = RenderedClientLatch::default();
            let size = (3840, 1080);
            let origin = (1920, -1080);
            let raw = Some([1920, -1080, 3840, 1080]);
            let frame = bordered_frame(size, [640, 0, 640, 0], 0);
            latch.observe(raw, origin, size, || rendered_area(&frame));
            latch.observe(raw, origin, size, || rendered_area(&frame));
            let rechecked_client = latch.select(raw, origin, size);
            assert_eq!(rechecked_client, [640, 0, 2560, 1080]);
            let mut slot = Some(crate::ssot::screen_from_geometry(
                size.0, size.1, 7, origin, [640, 0, 2560, 1080], 123,
            ));
            let outcome = crate::ssot::reconcile_screen(&mut slot, size, origin, rechecked_client);
            assert!(matches!(outcome, crate::ssot::ScreenReconcile::Kept), "recheck: {outcome:?}");
            let kept = slot.expect("the matching screen slot must survive the client recheck");
            assert_eq!(kept.client, [640, 0, 2560, 1080]);
            assert_eq!(kept.measured_at_ms, 123);
        }

        const WHOLE: [i32; 4] = [0, 0, 1920, 1080];

        const RECAL_ORIGIN: (i32, i32) = (0, 0);
        const RECAL_SIZE: (u32, u32) = (2560, 1440);

        /// Silent consequence if this broke: reporting the whole-capture
        /// fallback as a valid client hides the oversized raw input that the
        /// Recalibrate diagnosis trusts.
        #[test]
        fn recalibrate_diagnostic_flags_an_oversized_client_as_outside_the_capture() {
            let raw = Some([0, 0, 2560, 1500]);
            let client = client_in_capture(raw, RECAL_ORIGIN, RECAL_SIZE);
            let line = recalibrate_client_diagnostic(raw, RECAL_ORIGIN, RECAL_SIZE, client);
            assert!(line.contains("client used [0, 0, 2560, 1440]"), "line: {line:?}");
            assert!(line.contains("(client outside the capture)"), "line: {line:?}");
        }

        /// Silent consequence if this broke: conflating a missing client read
        /// with an out-of-bounds one gives a plausible but wrong diagnosis.
        #[test]
        fn recalibrate_diagnostic_flags_a_missing_client_as_no_client_read() {
            let client = client_in_capture(None, RECAL_ORIGIN, RECAL_SIZE);
            let line = recalibrate_client_diagnostic(None, RECAL_ORIGIN, RECAL_SIZE, client);
            assert!(line.contains("client used [0, 0, 2560, 1440]"), "line: {line:?}");
            assert!(line.contains("(no client read)"), "line: {line:?}");
        }

        /// Silent consequence: replacing the visible client with the whole
        /// monitor supplies plausible wrong geometry to OCR.
        #[test]
        fn a_client_off_the_captured_monitor_falls_back_to_the_whole_capture() {
            let client = client_in_capture(Some([3500, 90, 1600, 900]), (1920, 0), (1920, 1080));
            assert_eq!(client, [1580, 90, 340, 900]);
        }

        /// Silent consequence if this broke: before the first client read,
        /// OCR would crop a made-up rect instead of the fullscreen capture.
        #[test]
        fn an_unknown_client_falls_back_to_the_whole_capture() {
            assert_eq!(client_in_capture(None, (1920, 0), (1920, 1080)), WHOLE);
        }

        /// Silent consequence if this broke: a client left of the captured
        /// monitor would yield a negative crop origin and read the wrong pixels.
        #[test]
        fn a_client_left_of_the_captured_monitor_falls_back_to_the_whole_capture() {
            let client = client_in_capture(Some([100, 90, 1600, 900]), (1920, 0), (1920, 1080));
            assert_eq!(client, WHOLE);
        }

        /// Exchange row prep (POE-477). Silent consequence if any of these broke:
        /// the measured inverted 3x padded prep is what made the top row and its
        /// stock readable; the shared 2x prep read only `1` off the same row, so a
        /// drifted prep hands OCR an image it misreads into a plausible wrong ratio.
        mod row_prep {
            use super::super::super::preprocess_for_ocr_rows_within;
            use image::{imageops::FilterType, DynamicImage, GrayImage, ImageBuffer, Luma};

            const PAD: u32 = 40;

            /// Light text (120) on the dark panel field (40): a centred block, so
            /// the crop has both a glyph interior and a background far from it.
            fn light_text_on_dark(w: u32, h: u32) -> DynamicImage {
                DynamicImage::ImageLuma8(ImageBuffer::from_fn(w, h, |x, y| {
                    let inside = x >= w / 4 && x < 3 * w / 4 && y >= h / 4 && y < 3 * h / 4;
                    Luma([if inside { 120 } else { 40 }])
                }))
            }

            // keep:critical
            #[test]
            fn row_prep_is_the_3x_crop_framed_by_a_40_px_white_border() {
                let out = preprocess_for_ocr_rows_within(&light_text_on_dark(30, 10), None).to_luma8();

                assert_eq!(out.dimensions(), (30 * 3 + 2 * PAD, 10 * 3 + 2 * PAD));
                let dark_border: Vec<(u32, u32, u8)> = out
                    .enumerate_pixels()
                    .filter(|(x, y, _)| *x < PAD || *y < PAD || *x >= PAD + 90 || *y >= PAD + 30)
                    .filter(|(_, _, p)| p.0[0] != 255)
                    .map(|(x, y, p)| (x, y, p.0[0]))
                    .take(5)
                    .collect();
                assert!(dark_border.is_empty(), "border pixels that are not white: {dark_border:?}");
            }

            // keep:critical
            #[test]
            fn row_prep_turns_light_text_on_dark_into_a_dark_lanczos_3x_glyph_on_white() {
                let (w, h) = (30, 10);
                // The measured order: stretch 40..120 to 0..255, Lanczos3 to 3x, invert.
                let source = light_text_on_dark(w, h).to_luma8();
                let stretched: GrayImage = ImageBuffer::from_fn(w, h, |x, y| {
                    Luma([(u32::from(source.get_pixel(x, y).0[0] - 40) * 255 / 80) as u8])
                });
                let mut expected = image::imageops::resize(&stretched, w * 3, h * 3, FilterType::Lanczos3);
                for p in expected.pixels_mut() {
                    p.0[0] = 255 - p.0[0];
                }

                let out = preprocess_for_ocr_rows_within(&light_text_on_dark(w, h), None).to_luma8();

                assert_eq!(out.dimensions(), (w * 3 + 2 * PAD, h * 3 + 2 * PAD));
                let off: Vec<(u32, u32, u8, u8)> = expected
                    .enumerate_pixels()
                    .map(|(x, y, e)| (x, y, e.0[0], out.get_pixel(x + PAD, y + PAD).0[0]))
                    .filter(|(_, _, e, got)| e.abs_diff(*got) > 1)
                    .take(5)
                    .collect();
                assert!(off.is_empty(), "interior (x, y, expected, got) off by more than 1: {off:?}");
            }

            // keep:critical
            #[test]
            fn row_prep_fits_its_padded_longer_side_within_the_ocr_limit() {
                let cases = [
                    // 3x + 80 = 12080 > 10000: the inner 3x clamps to 9920 / 4000 = 2.48,
                    // so 30 rows become 74 and the padded width lands on the limit.
                    (4000, 30, Some(10_000), (10_000, 74 + 2 * PAD)),
                    // 3x + 80 = 9998 fits: the full 3x stays.
                    (3306, 30, Some(10_000), (9_998, 90 + 2 * PAD)),
                    // No known limit: the full 3x.
                    (4000, 30, None, (12_080, 90 + 2 * PAD)),
                ];
                let mismatches: Vec<String> = cases
                    .iter()
                    .filter_map(|&(w, h, limit, want)| {
                        let got = preprocess_for_ocr_rows_within(&light_text_on_dark(w, h), limit).to_luma8().dimensions();
                        (got != want).then(|| format!("{w}x{h} under {limit:?}: expected {want:?}, got {got:?}"))
                    })
                    .collect();
                assert!(mismatches.is_empty(), "{mismatches:#?}");
            }
        }
    }

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
mod capture_exclusion_keep_critical {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn every_app_surface_is_excluded_to_prevent_silently_reading_our_ui() {
        let labels = capture_exclusion_labels();
        for label in ["main", "lab", "temple", "mercenary", "loot", "prompt",
            "overlay-preview", "overlay-region-editor"] {
            assert!(labels.contains(&label), "{label} may overlap the PoE read rectangle");
        }
    }

    #[test]
    fn exclusion_refusal_prevents_silent_ui_capture_and_restores_acquired_holds() {
        for refused in [1, 2] {
            let acquired = RefCell::new(Vec::new());
            let restored = RefCell::new(Vec::new());
            let result = capture_with_exclusions(&[1, 2], |hwnd| {
                if hwnd == refused { return false; }
                acquired.borrow_mut().push(hwnd);
                true
            }, |hwnd| restored.borrow_mut().push(hwnd), || {
                panic!("a refused exclusion must never expose app pixels to a reader")
            });
            let result: Result<(), String> = result;
            assert!(result.is_err(), "exclusion refusal must fail the read");
            assert!(!result.unwrap_err().is_empty(), "refusal needs a diagnostic");
            let expected = if refused == 1 { vec![] } else { vec![1] };
            assert_eq!(*acquired.borrow(), expected);
            assert_eq!(*restored.borrow(), expected, "earlier holds leaked after refusal");
        }
    }

    #[test]
    fn grab_completion_restores_all_holds_so_later_screenshots_are_not_silently_changed() {
        for failed in [false, true] {
            let held = RefCell::new(Vec::new());
            let acquired = RefCell::new(Vec::new());
            let restored = RefCell::new(Vec::new());
            let mut grabs = 0;
            let result = capture_with_exclusions(&[1, 2], |hwnd| {
                held.borrow_mut().push(hwnd);
                acquired.borrow_mut().push(hwnd);
                true
            }, |hwnd| {
                held.borrow_mut().retain(|h| *h != hwnd);
                restored.borrow_mut().push(hwnd);
            }, || {
                grabs += 1;
                assert_eq!(*held.borrow(), vec![1, 2], "capture began before exclusion");
                if failed { Err("grab failed".into()) } else { Ok(37) }
            });
            assert_eq!(result, if failed { Err("grab failed".into()) } else { Ok(37) });
            assert_eq!(grabs, 1, "keep the existing single game-monitor grab");
            assert_eq!(*acquired.borrow(), vec![1, 2]);
            let mut released = restored.into_inner();
            released.sort();
            assert_eq!(released, vec![1, 2], "success and failure must both restore holds");
            assert!(held.borrow().is_empty());
        }
    }
}

#[cfg(test)]
mod notice_gate_tests {
    use super::{grab_if_accepted, CaptureError};
    use crate::http::NoticeGate;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Runs one gated grab against a counting stand-in; returns the result.
    fn grab(gate: &NoticeGate, flag: &AtomicBool, grabs: &Cell<u32>, logs: &Cell<u32>) -> Result<&'static str, CaptureError> {
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
        assert_eq!(grab(&gate, &flag, &grabs, &logs), Err(CaptureError::NotAccepted));
        assert_eq!(grabs.get(), 0);
    }

    /// A failing grab once accepted is a `Failed`, not a refusal, so the read
    /// key still warns about it; callers that format the error see its text.
    #[test]
    fn a_failing_grab_once_accepted_is_failed_with_its_message() {
        let gate = NoticeGate::default();
        gate.open();
        let got = grab_if_accepted(&gate, &AtomicBool::new(false), |_| {}, || Err::<(), _>("no monitor".to_string()));
        assert_eq!(got, Err(CaptureError::Failed("no monitor".to_string())));
        assert_eq!(String::from(got.unwrap_err()), "no monitor");
        assert_eq!(String::from(CaptureError::NotAccepted), crate::http::NOTICE_NOT_ACCEPTED);
    }

    /// The accept command opens a clone of the gate the app holds; the next
    /// grab through the original must see it.
    #[test]
    fn capture_reaches_the_grab_once_the_same_gate_opens() {
        let (gate, flag, grabs, logs) = (NoticeGate::default(), AtomicBool::new(false), Cell::new(0), Cell::new(0));
        assert_eq!(grab(&gate, &flag, &grabs, &logs), Err(CaptureError::NotAccepted));
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
