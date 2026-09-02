//! Overlay click-through for Windows/WebView2 — a REGISTRY of hooked windows.
//!
//! Problem: WebView2 creates child HWNDs (Chrome_WidgetWin_0/1, Intermediate
//! D3D Window) that handle hit-testing independently. Subclassing the parent
//! with WM_NCHITTEST → HTTRANSPARENT does NOT work because WebView2's child
//! windows intercept mouse input before the parent sees it. WebView2 also
//! strips WS_EX_TRANSPARENT when creating/updating child windows.
//!
//! Solution:
//!   1. Set WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE on every
//!      overlay window — always fully click-through, game never sees them.
//!   2. Install one global WH_MOUSE_LL hook, shared by every registered window.
//!   3. The hook re-applies WS_EX_TRANSPARENT on mouse events over any
//!      registered window (the WebView2 fix).
//!   4. When a click lands in a window's declared HOT RECT, buffer the
//!      window-relative coordinates and consume the click (game doesn't see it).
//!   5. The message loop drains the buffer and emits `overlay-click` to that
//!      window with `emit_to`.
//!   6. The page uses elementFromPoint + data-action attributes to map clicks.
//!   7. `has_content` gates interception per window — an empty overlay passes
//!      clicks through.
//!
//! Why a registry and not the singleton it replaces: the singleton tracked ONE
//! HWND, so a second interactive window silently took the first one's place
//! (`install_hook` returned `None` and the earlier window stopped being
//! interactive), and the WS_EX_TRANSPARENT repair — the only thing that undoes
//! WebView2's stripping — reached that one window alone. Every overlay now
//! registers, so every overlay is repaired.
//!
//! The hot rects replace the singleton's right-edge "interactive width": a
//! window declares the physical, window-relative rectangles that its own
//! buttons occupy, and withdraws them when the buttons unmount. Outside config
//! mode a window's WS_EX_TRANSPARENT is never cleared.
//!
//! One behavioural delta from the singleton, deliberate: the BUTTON-UP is now
//! consumed wherever the cursor is, iff the matching press was claimed. The
//! singleton consumed an up only while the cursor was still inside the
//! interactive zone, so it both ate releases it had never claimed (a drag that
//! started on the game and ended over the overlay lost its button-up, leaving
//! the game holding the button) and leaked releases it had (a press on an
//! overlay button, dragged off it, delivered a stray up to the game). Pairing
//! the up to the down fixes both directions: a release we never claimed always
//! passes through, and a release we did claim is always eaten.

use serde::Deserialize;

/// A window-relative rectangle, in PHYSICAL pixels, whose clicks the overlay
/// claims.
///
/// Physical because that is what the hook has: `GetWindowRect` and the
/// `MSLLHOOKSTRUCT` cursor are both physical, and the page converts its own
/// `getBoundingClientRect()` with the window's Tauri `scaleFactor()`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub struct HotRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl HotRect {
    /// Whether a window-relative point falls inside this rect.
    ///
    /// Half-open on both axes, so a zero-sized rect — what an unmounted or
    /// display-none button measures — claims nothing. Widened to `i64` because
    /// `x + w` is an `i32` plus a `u32` and the page is the one supplying both.
    #[cfg(any(windows, test))]
    fn contains(&self, lx: i32, ly: i32) -> bool {
        let (lx, ly) = (lx as i64, ly as i64);
        let (x, y) = (self.x as i64, self.y as i64);
        lx >= x && lx < x + self.w as i64 && ly >= y && ly < y + self.h as i64
    }
}

/// Cached overlay geometry for the click-through hook, and the rule for when
/// that cache may be trusted.
///
/// A type rather than four loose statics so the trust rule is testable off
/// Windows. The rule exists entirely for the case a test cannot provoke — a
/// failed `GetWindowRect` — but every consequence of that failure lives here,
/// so this is the seam. Compiled on every platform under `test`; only Windows
/// builds instantiate it (POE-148).
#[cfg(any(windows, test))]
// Off Windows this compiles for tests only, with no hook to call `invalidate`.
#[cfg_attr(not(windows), allow(dead_code))]
mod rect_cache {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Mutex as StdMutex;

    pub struct RectCache {
        /// (left, top, right, bottom) in physical pixels.
        rect: StdMutex<(i32, i32, i32, i32)>,
        dirty: AtomicBool,
        valid: AtomicBool,
        read_failures: AtomicU32,
    }

    impl RectCache {
        pub const fn new() -> Self {
            Self {
                rect: StdMutex::new((0, 0, 0, 0)),
                // Starts dirty and untrusted: (0,0,0,0) describes no window.
                dirty: AtomicBool::new(true),
                valid: AtomicBool::new(false),
                read_failures: AtomicU32::new(0),
            }
        }

        /// Mark the cache stale so the next attempt re-reads it.
        pub fn invalidate(&self) {
            self.dirty.store(true, Ordering::Relaxed);
        }

        /// Whether a refresh is due, clearing the flag in the same operation.
        pub fn take_dirty(&self) -> bool {
            self.dirty.swap(false, Ordering::Relaxed)
        }

        /// Record a successful read.
        ///
        /// Poison-tolerant, like every reader: taking the lock with `if let Ok`
        /// dropped the update on a poisoned mutex *after* the dirty flag had
        /// already been cleared, so a single panic while holding the lock froze
        /// the rect for the rest of the session with no signal.
        pub fn store(&self, rect: (i32, i32, i32, i32)) {
            *self.rect.lock().unwrap_or_else(|e| e.into_inner()) = rect;
            self.valid.store(true, Ordering::Relaxed);
        }

        /// Record a failed read.
        ///
        /// The stored tuple still holds the *previous* geometry, so trust is
        /// withdrawn rather than the value replaced — consumers must decline,
        /// not compute against another window's rect. Staying dirty lets the
        /// cache self-heal on the next attempt that can read; the accepted
        /// trade-off is that a persistently unreadable window is retried on
        /// every mouse event, which the failure count makes visible.
        pub fn record_failure(&self) {
            self.dirty.store(true, Ordering::Relaxed);
            self.valid.store(false, Ordering::Relaxed);
            self.read_failures.fetch_add(1, Ordering::Relaxed);
        }

        /// Forget which window the cache describes, without counting a failure.
        pub fn reset_for_new_window(&self) {
            self.dirty.store(true, Ordering::Relaxed);
            self.valid.store(false, Ordering::Relaxed);
        }

        /// Whether the stored rect came from a successful read of the current
        /// window. False means every consumer must decline.
        pub fn is_valid(&self) -> bool {
            self.valid.load(Ordering::Relaxed)
        }

        pub fn get(&self) -> (i32, i32, i32, i32) {
            *self.rect.lock().unwrap_or_else(|e| e.into_inner())
        }

        /// Failures since the last drain. The hook cannot log — it must return
        /// inside `LowLevelHooksTimeout` — so its message loop drains this.
        pub fn take_failures(&self) -> u32 {
            self.read_failures.swap(0, Ordering::Relaxed)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::RectCache;

        #[test]
        fn a_fresh_cache_is_untrusted() {
            // (0,0,0,0) would hit-test as a degenerate zone at the screen
            // origin, so the cache must not be trusted before its first read.
            assert!(!RectCache::new().is_valid());
        }

        #[test]
        fn a_failed_read_withdraws_trust_while_leaving_the_stale_rect_in_place() {
            // The exact POE-148 regression: before this, a failed GetWindowRect
            // only re-set the dirty flag, and the hook went straight on to
            // hit-test and translate coordinates against the tuple below.
            let cache = RectCache::new();
            cache.store((100, 200, 400, 500));

            cache.record_failure();

            assert!(!cache.is_valid());
            assert_eq!(
                cache.get(),
                (100, 200, 400, 500),
                "the stale rect is still readable — withdrawing trust is what stops it being used"
            );
        }

        #[test]
        fn a_failed_read_leaves_the_cache_dirty_so_it_can_self_heal() {
            // Mirrors the refresh sequence: the dirty flag is consumed before
            // the read, so a successful read leaves the cache clean.
            let cache = RectCache::new();
            assert!(cache.take_dirty(), "a fresh cache is due for its first read");
            cache.store((1, 2, 3, 4));
            assert!(!cache.take_dirty(), "a successful read leaves nothing to refresh");

            cache.record_failure();

            assert!(cache.take_dirty());
        }

        #[test]
        fn a_later_successful_read_restores_trust() {
            let cache = RectCache::new();
            cache.record_failure();

            cache.store((10, 20, 30, 40));

            assert!(cache.is_valid());
            assert_eq!(cache.get(), (10, 20, 30, 40));
        }

        #[test]
        fn failed_reads_are_counted_and_drained_by_the_reporter() {
            let cache = RectCache::new();
            cache.record_failure();
            cache.record_failure();

            assert_eq!(cache.take_failures(), 2);
            assert_eq!(cache.take_failures(), 0, "the drain must not re-report");
        }

        #[test]
        fn adopting_a_new_window_withdraws_trust_without_counting_a_failure() {
            let cache = RectCache::new();
            cache.store((100, 200, 400, 500));

            cache.reset_for_new_window();

            assert!(!cache.is_valid());
            assert_eq!(cache.take_failures(), 0);
        }

        #[test]
        fn a_poisoned_rect_lock_does_not_freeze_the_cache() {
            // Every reader already recovers from poisoning with `into_inner`, so
            // a writer that gives up on `Err` leaves them serving the old tuple
            // for the rest of the session. One panic while holding this lock
            // used to be enough.
            let cache = RectCache::new();
            cache.store((1, 2, 3, 4));
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = cache.rect.lock().unwrap();
                panic!("poison the rect lock");
            }));
            assert!(cache.rect.is_poisoned(), "the arrange step must have poisoned the lock");

            cache.store((50, 60, 70, 80));

            assert_eq!(cache.get(), (50, 60, 70, 80));
            assert!(cache.is_valid());
        }
    }
}

#[cfg(any(windows, test))]
use rect_cache::RectCache;

/// One overlay window the hook watches.
///
/// `hwnd` is `0` until [`register`] runs: a page may declare its content or its
/// hot rects during the ~1 s `set_overlay_clickthrough` spends waiting for the
/// WebView2 HWND, and dropping those declarations would leave the window inert
/// until the page happened to re-send them. A zero HWND simply never produces a
/// readable rect, so such an entry declines every click until it is filled in.
#[cfg(any(windows, test))]
// Off Windows this compiles for tests only, with no hook to read the fields.
#[cfg_attr(not(windows), allow(dead_code))]
pub struct HookedWindow {
    pub label: String,
    pub hwnd: isize,
    /// Cached window geometry. See [`rect_cache::RectCache`] for the trust rule.
    pub rect: RectCache,
    /// Window-relative rectangles this window claims clicks in.
    pub hot: Vec<HotRect>,
    /// Whether the window is drawing anything. False → clicks pass through.
    pub has_content: bool,
    /// True while the user is arranging widgets in this window. The hook then
    /// neither re-applies WS_EX_TRANSPARENT (the window is deliberately
    /// interactive) nor intercepts (the webview is handling the clicks itself).
    pub config_mode: bool,
}

#[cfg(any(windows, test))]
impl HookedWindow {
    /// A registry entry with nothing declared about it yet.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            hwnd: 0,
            rect: RectCache::new(),
            hot: Vec::new(),
            has_content: false,
            config_mode: false,
        }
    }
}

/// The entry for `label`, created if the page spoke before the window
/// registered. See [`HookedWindow`] for why that entry is kept.
///
/// Pure and platform-neutral so the registry rules are testable off Windows;
/// the Windows half only supplies the lock around the `Vec`.
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
pub fn upsert<'a>(windows: &'a mut Vec<HookedWindow>, label: &str) -> &'a mut HookedWindow {
    if let Some(i) = windows.iter().position(|w| w.label == label) {
        return &mut windows[i];
    }
    windows.push(HookedWindow::new(label));
    windows.last_mut().expect("just pushed")
}

/// Point `label` at `hwnd`, merging with whatever the page already declared.
///
/// Declarations made under this label survive a re-register of the SAME HWND —
/// the page may have declared its hot rects, its content or its config mode
/// during the ~1 s `set_overlay_clickthrough` spends waiting for the WebView2
/// HWND, and the setup that arrives afterwards must not undo them. They are
/// dropped only when the HWND CHANGED, because they then describe a page that
/// is gone.
///
/// `config_mode` is the one that hurts most if this is got wrong: a window
/// re-registered while the user is arranging its widgets would be handed back
/// to the hook while it is still `set_ignore_cursor_events(false)`, ending up
/// neither interactive nor click-through.
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
pub fn register_in(windows: &mut Vec<HookedWindow>, label: &str, hwnd: isize) {
    let w = upsert(windows, label);
    if w.hwnd != 0 && w.hwnd != hwnd {
        w.hot.clear();
        w.has_content = false;
        w.config_mode = false;
    }
    w.hwnd = hwnd;
    // The cached rect describes the previous window under this label until the
    // first successful read of this one.
    w.rect.reset_for_new_window();
}

/// Drop `label`. True when that removal emptied the registry, i.e. when the
/// caller should tear the hook down.
///
/// False for a label that was not there: removing nothing cannot be the reason
/// the registry is empty, and a stray unregister must not tear down a hook the
/// remaining windows are relying on.
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
pub fn unregister_in(windows: &mut Vec<HookedWindow>, label: &str) -> bool {
    let before = windows.len();
    windows.retain(|w| w.label != label);
    before != windows.len() && windows.is_empty()
}

/// The window a click at physical `(cx, cy)` belongs to, and that click's
/// window-relative coordinates.
///
/// Pure, and the whole interception decision: the hook does the Win32 work
/// (refreshing rects, re-applying styles) and then asks this. Order is the
/// registration order and the FIRST match wins — two overlays can overlap, and
/// a stable answer is worth more than a z-order the hook cannot see anyway.
///
/// A window is skipped when it is in config mode (the webview is taking the
/// clicks itself), when it has no content, or when its cached rect is not
/// trusted — declining passes the click to the game, which is strictly better
/// than translating coordinates against geometry we could not read (POE-148).
#[cfg(any(windows, test))]
pub fn hit_test(windows: &[HookedWindow], cx: i32, cy: i32) -> Option<(usize, i32, i32)> {
    for (i, w) in windows.iter().enumerate() {
        if w.config_mode || !w.has_content || !w.rect.is_valid() {
            continue;
        }
        let (left, top, right, bottom) = w.rect.get();
        if !(cx >= left && cx < right && cy >= top && cy < bottom) {
            continue;
        }
        let (lx, ly) = (cx - left, cy - top);
        if w.hot.iter().any(|h| h.contains(lx, ly)) {
            return Some((i, lx, ly));
        }
    }
    None
}

/// Which window, if any, consumed the last WM_LBUTTONDOWN.
///
/// `0` means none; otherwise the registry index plus one, so the whole latch is
/// one atomic. It exists to PAIR the up with the down: the singleton this
/// replaces ate every button-up inside the interactive zone, so a drag that
/// started on the game and ended over the overlay lost its release and left the
/// game holding the button.
///
/// A type rather than a loose static so the pairing is testable off Windows
/// without two tests racing over one process-wide value.
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct PressLatch(std::sync::atomic::AtomicUsize);

#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
impl PressLatch {
    pub const fn new() -> Self {
        Self(std::sync::atomic::AtomicUsize::new(0))
    }

    /// Record that the window at `index` consumed a button-down.
    pub fn press(&self, index: usize) {
        self.0
            .store(index + 1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Forget any unreleased press.
    ///
    /// Called for a button-down we DECLINED: that press belongs to the game, so
    /// whatever the latch was still holding can never see its own release any
    /// more. Left set, it would eat the game's next button-up instead — one
    /// stranded latch is one lost release.
    pub fn clear(&self) {
        self.0.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// The window that consumed the matching down, clearing the latch.
    ///
    /// `None` means the button went down somewhere we did not claim, so its
    /// release belongs to whoever got the press. The index is the registry
    /// position AT PRESS TIME and is reported, not re-validated — the caller
    /// only needs to know whether the release is its own.
    pub fn release(&self) -> Option<usize> {
        match self.0.swap(0, std::sync::atomic::Ordering::Relaxed) {
            0 => None,
            n => Some(n - 1),
        }
    }
}

// ---------------------------------------------------------------------------
// Windows implementation
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod win {
    use super::{hit_test, register_in, unregister_in, upsert, HookedWindow, HotRect, PressLatch};
    use std::sync::Mutex as StdMutex;
    use std::sync::RwLock;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::*;

    struct SendHook(HHOOK);
    unsafe impl Send for SendHook {}

    static HOOK_HANDLE: StdMutex<Option<SendHook>> = StdMutex::new(None);
    /// Whether an install is claimed — set BEFORE the thread that actually
    /// installs, cleared when that thread exits.
    ///
    /// Separate from `HOOK_HANDLE` because the handle only exists once the
    /// spawned thread has called `SetWindowsHookExW`, and every overlay now
    /// races to be the first to register: the lab windows are created in one
    /// burst and their 1 s setup timers expire together, so two of them could
    /// both find no handle and both install. The loser's hook would never be
    /// unhooked — `HOOK_HANDLE` can only hold one.
    static HOOK_CLAIMED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    /// Every overlay window the hook watches.
    ///
    /// A `std::sync::RwLock` on purpose: the hook proc reads it on every mouse
    /// event and must return inside `LowLevelHooksTimeout`, while the writers
    /// (a page declaring rects, a window registering) are rare and hold the
    /// lock for a field assignment or a `Vec` swap.
    static HOOKED: RwLock<Vec<HookedWindow>> = RwLock::new(Vec::new());
    static PRESSED: PressLatch = PressLatch::new();
    /// Click buffer — the hook pushes `(label, window-relative x, y)`, the
    /// message loop drains it and emits.
    static CLICK_BUFFER: StdMutex<Vec<(String, i32, i32)>> = StdMutex::new(Vec::new());

    fn hooked() -> std::sync::RwLockReadGuard<'static, Vec<HookedWindow>> {
        HOOKED.read().unwrap_or_else(|e| e.into_inner())
    }

    fn hooked_mut() -> std::sync::RwLockWriteGuard<'static, Vec<HookedWindow>> {
        HOOKED.write().unwrap_or_else(|e| e.into_inner())
    }

    /// Re-read one window's rect when it is marked dirty.
    ///
    /// Deliberately cached rather than read per event: this runs inside a
    /// `WH_MOUSE_LL` hook, which Windows silently unhooks when it exceeds
    /// `LowLevelHooksTimeout`, and the hook sees every mouse event system-wide.
    fn refresh_rect_if_dirty(w: &HookedWindow) {
        if w.hwnd == 0 {
            return;
        }
        if !w.rect.take_dirty() {
            return;
        }

        unsafe {
            let hwnd = HWND(w.hwnd as *mut _);
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                w.rect.store((rect.left, rect.top, rect.right, rect.bottom));
            } else {
                w.rect.record_failure();
            }
        }
    }

    unsafe extern "system" fn mouse_hook_proc(
        n_code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if n_code >= 0 {
            let mouse = &*(l_param.0 as *const MSLLHOOKSTRUCT);
            let cx = mouse.pt.x;
            let cy = mouse.pt.y;
            let msg_id = w_param.0 as u32;

            let windows = hooked();

            // A click is the one event rare enough to afford a fresh
            // GetWindowRect per window (clicks are ~3 orders of magnitude rarer
            // than moves, and the move path must stay inside
            // LowLevelHooksTimeout). A successful read bounds a missed
            // move/resize/DPI event to a single click instead of misplacing the
            // hot rects and every emitted coordinate for the rest of the
            // session (POE-148). A failed read gives no such bound — it
            // invalidates the cache instead, and the consumers below decline
            // until a read succeeds.
            if msg_id == WM_LBUTTONDOWN {
                for w in windows.iter() {
                    w.rect.invalidate();
                }
            }
            // Clean caches cost one atomic swap each, so this stays cheap on
            // the move path; only a dirty entry reaches Win32.
            for w in windows.iter() {
                refresh_rect_if_dirty(w);
            }

            // WebView2 may strip WS_EX_TRANSPARENT when creating/updating child
            // windows. Re-apply when it's missing, for every registered window
            // the cursor is currently over — the singleton this replaces
            // repaired only the comparator, which left the merc strip able to
            // go opaque to the mouse after a content-driven resize.
            //
            // Bounded by how many overlays overlap under one cursor position:
            // the check is skipped entirely unless the cursor is inside a
            // window's rect, so the usual cost on the move path is zero Win32
            // calls and the worst case is one per overlapping overlay.
            for w in windows.iter() {
                if w.config_mode || !w.rect.is_valid() {
                    continue;
                }
                let (left, top, right, bottom) = w.rect.get();
                if !(cx >= left && cx < right && cy >= top && cy < bottom) {
                    continue;
                }
                let hwnd = HWND(w.hwnd as *mut _);
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
                // ex == 0 can mean failure OR "no styles" — only re-apply if we
                // got a nonzero result that's missing WS_EX_TRANSPARENT.
                if ex != 0 && ex & WS_EX_TRANSPARENT.0 as i32 == 0 {
                    SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_TRANSPARENT.0 as i32);
                }
            }

            match msg_id {
                WM_LBUTTONDOWN => match hit_test(&windows, cx, cy) {
                    Some((i, lx, ly)) => {
                        CLICK_BUFFER
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push((windows[i].label.clone(), lx, ly));
                        PRESSED.press(i);
                        // Consume — don't pass to the game or CallNextHookEx.
                        return LRESULT(1);
                    }
                    // This press is the game's, so any press we are still
                    // holding will never see its own release — a hot rect can
                    // be withdrawn between the down and the up (POE-148 rect
                    // failure, `has_content` going false, the button
                    // unmounting). Drop the latch here or it eats the release
                    // of THIS press instead.
                    None => PRESSED.clear(),
                },
                WM_LBUTTONUP => {
                    // Only the release of a press WE took. A press that started
                    // on the game must keep its release, or the game is left
                    // holding the button.
                    if PRESSED.release().is_some() {
                        return LRESULT(1);
                    }
                }
                _ => {}
            }
        }
        CallNextHookEx(None, n_code, w_param, l_param)
    }

    /// Add or refresh `label`'s registry entry. Merge rule in [`register_in`].
    pub fn register(label: &str, hwnd: HWND) {
        register_in(&mut hooked_mut(), label, hwnd.0 as isize);
    }

    /// Drop `label`. True when that emptied the registry, i.e. when the caller
    /// should tear the hook down. Rule in [`unregister_in`].
    pub fn unregister(label: &str) -> bool {
        unregister_in(&mut hooked_mut(), label)
    }

    pub fn set_hot_rects(label: &str, rects: Vec<HotRect>) {
        let mut windows = hooked_mut();
        upsert(&mut windows, label).hot = rects;
    }

    pub fn set_has_content(label: &str, has_content: bool) {
        let mut windows = hooked_mut();
        upsert(&mut windows, label).has_content = has_content;
    }

    pub fn set_config_mode(label: &str, on: bool) {
        let mut windows = hooked_mut();
        upsert(&mut windows, label).config_mode = on;
    }

    /// Whether `label` is currently being arranged by the user.
    ///
    /// Read by every path that would otherwise re-assert click-through
    /// (`set_overlay_clickthrough`'s delayed setup, `fit_overlay_height`'s
    /// post-resize re-arm): those calls are correct for a hooked window and
    /// wrong for one the user is dragging widgets in — they would leave it
    /// `set_ignore_cursor_events(true)` while `config_mode` still tells the
    /// hook to keep its hands off, i.e. neither interactive nor hooked.
    ///
    /// A label nobody registered is not in config mode.
    pub fn config_mode(label: &str) -> bool {
        hooked().iter().any(|w| w.label == label && w.config_mode)
    }

    /// Mark `label`'s cached rect stale. A no-op for a label nobody registered,
    /// so every caller that moves a window may call it unconditionally.
    pub fn invalidate_label(label: &str) {
        for w in hooked().iter() {
            if w.label == label {
                w.rect.invalidate();
            }
        }
    }

    /// Install the global mouse hook. The message loop drains click events and
    /// emits Tauri `overlay-click` events. Returns a stop-signal sender, or
    /// `None` when a hook is already running.
    ///
    /// Upholds one invariant: **a non-empty registry means a hook is installed
    /// or is being installed.** The thread re-checks the registry after it
    /// releases the claim and re-installs if anyone registered during teardown
    /// (see the tail of the loop below).
    pub fn install_hook(app: tauri::AppHandle) -> Option<std::sync::mpsc::Sender<()>> {
        use std::sync::atomic::Ordering;
        if HOOK_CLAIMED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            log::info!("Overlay mouse hook already installed — reusing existing hook");
            return None;
        }
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            unsafe {
                let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) {
                    Ok(h) => h,
                    Err(e) => {
                        log::error!("Mouse hook install failed: {}", e);
                        // Release the claim so the next overlay to register can
                        // try again — otherwise one failure leaves every overlay
                        // unrepaired for the rest of the session.
                        HOOK_CLAIMED.store(false, Ordering::SeqCst);
                        return;
                    }
                };
                *HOOK_HANDLE.lock().unwrap_or_else(|e| e.into_inner()) = Some(SendHook(hook));
                log::info!("Overlay mouse hook installed (fully click-through mode)");

                let mut msg = MSG::default();
                // Throttled: a persistently unreadable rect stays dirty, so the
                // hook retries on every mouse event and the counter can climb by
                // thousands a second. One line per second reports the volume
                // without flooding the 50-entry LOGS buffer.
                let mut last_failure_report = std::time::Instant::now();
                loop {
                    if stop_rx.try_recv().is_ok() { break; }

                    if last_failure_report.elapsed() >= std::time::Duration::from_secs(1) {
                        last_failure_report = std::time::Instant::now();
                        let failures: u32 = hooked()
                            .iter()
                            .fold(0u32, |acc, w| acc.saturating_add(w.rect.take_failures()));
                        if failures > 0 {
                            crate::app_log(&app, format!(
                                "Overlay rect read failed {} time(s) in the last second — hot-rect clicks passed through to the game until a read succeeds",
                                failures,
                            ));
                        }
                    }

                    // Drain click buffer → emit Tauri events to the window that
                    // was clicked. `emit_to` and not `emit`: the coordinates are
                    // window-relative, so they are meaningful only to the window
                    // that owns them — a broadcast would hand them to every
                    // other overlay's `elementFromPoint` as well. Only the
                    // comparator listens today; scoping the emit is what keeps
                    // that true as more windows grow buttons. The listener must
                    // be webview-scoped to match (`getCurrentWebviewWindow()
                    // .listen`, not a bare `listen`) — see OVERLAY-GUIDE.
                    {
                        let mut buf = CLICK_BUFFER.lock().unwrap_or_else(|e| e.into_inner());
                        for (label, x, y) in buf.drain(..) {
                            use tauri::Emitter;
                            let payload = serde_json::json!({ "label": label, "x": x, "y": y });
                            if let Err(e) = app.emit_to(label.as_str(), "overlay-click", payload) {
                                log::warn!("emit overlay-click to '{}' failed: {}", label, e);
                            }
                        }
                    }

                    if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    } else {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }

                if let Some(SendHook(h)) = HOOK_HANDLE.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    if let Err(e) = UnhookWindowsHookEx(h) {
                        log::error!("Failed to unhook mouse hook: {} — hook may leak", e);
                    }
                }
                HOOK_CLAIMED.store(false, Ordering::SeqCst);
                log::info!("Overlay mouse hook removed");

                // INVARIANT: a non-empty registry means a hook is installed or
                // is being installed. Teardown is the one window where that can
                // break: `unregister` returns true and the stop signal is sent,
                // but the claim is only released here, several milliseconds
                // later. A window registering in between calls `install_hook`,
                // loses the compare_exchange, is told "already installed —
                // reusing", and ends up with no hook at all for the rest of the
                // session — no WS_EX_TRANSPARENT repair, no hot-rect clicks.
                // Re-checking after the claim is released closes it: whoever
                // registered during the gap is visible in the registry by now,
                // and if the same check has already been won by a fresh
                // `install_hook` this one simply gets `None`.
                if !hooked().is_empty() {
                    if let Some(tx) = install_hook(app.clone()) {
                        use tauri::Manager;
                        *app.state::<crate::AppState>()
                            .overlay_hook_stop
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()) = Some(tx);
                        log::info!(
                            "Overlay mouse hook re-installed — a window registered while the previous hook was tearing down"
                        );
                    }
                }
            }
        });
        Some(stop_tx)
    }

    pub unsafe fn set_noactivate(hwnd: HWND) {
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as i32);

        unsafe extern "system" fn enum_child(child: HWND, _: LPARAM) -> BOOL {
            let ex = GetWindowLongW(child, GWL_EXSTYLE);
            SetWindowLongW(child, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as i32);
            BOOL(1)
        }
        let _ = EnumChildWindows(hwnd, Some(enum_child), LPARAM(0));
    }
}

#[cfg(windows)]
pub use win::{
    config_mode, install_hook, invalidate_label, register, set_config_mode, set_has_content,
    set_hot_rects, set_noactivate, unregister,
};

/// Whether `label` is currently being arranged by the user — always false where
/// there is no hook to leave alone.
///
/// Compiled off Windows so the callers that must not stomp config mode
/// (`set_overlay_clickthrough`, `fit_overlay_height`) read the same guard on
/// every platform instead of duplicating a `cfg` around each call site.
#[cfg(not(windows))]
pub fn config_mode(_label: &str) -> bool {
    false
}

#[cfg(test)]
mod hit_test_tests {
    use super::{hit_test, HookedWindow, HotRect, PressLatch};

    fn hot(x: i32, y: i32, w: u32, h: u32) -> HotRect {
        HotRect { x, y, w, h }
    }

    /// A registered window with a rect the hook has successfully read, content
    /// on screen, and the given hot rects — i.e. everything a click needs to be
    /// claimed, so each test can withdraw exactly one condition.
    fn window(label: &str, rect: (i32, i32, i32, i32), hot: Vec<HotRect>) -> HookedWindow {
        let w = HookedWindow::new(label);
        w.rect.store(rect);
        HookedWindow { hot, has_content: true, ..w }
    }

    #[test]
    fn an_empty_registry_claims_nothing() {
        assert_eq!(hit_test(&[], 100, 100), None);
    }

    #[test]
    fn a_click_in_a_hot_rect_is_claimed_with_window_relative_coordinates() {
        // The window sits at (100, 200); the hot rect starts 40 px into it.
        // A click at (150, 230) is 10 px into the hot rect, and the page is
        // handed (50, 30) — its own coordinates, not the screen's.
        let windows = [window("comparator", (100, 200, 400, 500), vec![hot(40, 20, 60, 30)])];

        assert_eq!(hit_test(&windows, 150, 230), Some((0, 50, 30)));
    }

    #[test]
    fn a_click_inside_the_window_but_outside_every_hot_rect_is_not_claimed() {
        // The comparator's table is 500 px of panel the game must stay clickable
        // through; only the button column is claimed.
        let windows = [window("comparator", (100, 200, 400, 500), vec![hot(300, 0, 60, 300)])];

        assert_eq!(hit_test(&windows, 150, 230), None);
    }

    #[test]
    fn the_far_edges_of_a_hot_rect_are_outside_it() {
        // Half-open: a 60x30 rect at (40, 20) ends at (100, 50) exclusive, so
        // the pixel column at lx=100 belongs to the game.
        let windows = [window("comparator", (0, 0, 400, 500), vec![hot(40, 20, 60, 30)])];

        assert_eq!(hit_test(&windows, 99, 49), Some((0, 99, 49)));
        assert_eq!(hit_test(&windows, 100, 49), None);
        assert_eq!(hit_test(&windows, 99, 50), None);
    }

    #[test]
    fn a_zero_sized_hot_rect_claims_nothing() {
        // What a button measures while it is unmounted or display:none. It must
        // not swallow the click at its own origin.
        let windows = [window("comparator", (0, 0, 400, 500), vec![hot(40, 20, 0, 0)])];

        assert_eq!(hit_test(&windows, 40, 20), None);
    }

    #[test]
    fn a_click_past_the_window_edge_is_not_claimed() {
        // The window rect is checked before the offset is computed. Without
        // that check a click 10 px to the right of a 300-wide window would be
        // translated to lx=310 and tested against the hot rects anyway.
        let windows = [window("comparator", (100, 200, 400, 500), vec![hot(300, 0, 60, 30)])];

        assert_eq!(hit_test(&windows, 410, 210), None);
    }

    #[test]
    fn a_window_with_no_content_does_not_claim_its_hot_rect() {
        // An empty comparator declares the same column its buttons will occupy;
        // until there is something to click, the game gets the click.
        let mut w = window("comparator", (100, 200, 400, 500), vec![hot(40, 20, 60, 30)]);
        w.has_content = false;

        assert_eq!(hit_test(&[w], 150, 230), None);
    }

    #[test]
    fn a_window_whose_rect_could_not_be_read_does_not_claim_its_hot_rect() {
        // POE-148: the stale rect is still readable, so nothing but withdrawn
        // trust stops the hook translating this click against another window's
        // geometry.
        let w = window("comparator", (100, 200, 400, 500), vec![hot(40, 20, 60, 30)]);
        w.rect.record_failure();

        assert_eq!(hit_test(&[w], 150, 230), None);
    }

    #[test]
    fn a_window_in_config_mode_does_not_claim_its_hot_rect() {
        // In config mode the window is genuinely interactive — the webview is
        // handling drags and Save/Cancel itself, so the hook must not eat the
        // click before it gets there.
        let mut w = window("temple", (100, 200, 400, 500), vec![hot(40, 20, 60, 30)]);
        w.config_mode = true;

        assert_eq!(hit_test(&[w], 150, 230), None);
    }

    #[test]
    fn a_config_mode_window_does_not_shadow_the_window_behind_it() {
        // Skipped, not blocking: the overlay being arranged must not stop the
        // comparator underneath it from taking its own button clicks.
        let mut front = window("temple", (0, 0, 400, 500), vec![hot(40, 20, 60, 30)]);
        front.config_mode = true;
        let back = window("comparator", (0, 0, 400, 500), vec![hot(40, 20, 60, 30)]);

        assert_eq!(hit_test(&[front, back], 50, 30), Some((1, 50, 30)));
    }

    #[test]
    fn the_first_registered_window_wins_when_two_claim_the_same_click() {
        // The hook cannot see z-order, so registration order is the tiebreak and
        // the answer has to be stable — a click must not alternate between two
        // overlapping overlays.
        let first = window("comparator", (0, 0, 400, 500), vec![hot(40, 20, 60, 30)]);
        let second = window("temple", (0, 0, 400, 500), vec![hot(0, 0, 400, 500)]);

        assert_eq!(hit_test(&[first, second], 50, 30), Some((0, 50, 30)));
    }

    #[test]
    fn a_release_without_a_press_is_not_ours() {
        // The regression this pairing exists for: a drag that started on the
        // game and ended over an overlay used to lose its button-up.
        let latch = PressLatch::new();

        assert_eq!(latch.release(), None);
    }

    #[test]
    fn a_release_names_the_window_that_took_the_press() {
        let latch = PressLatch::new();
        latch.press(2);

        assert_eq!(latch.release(), Some(2));
    }

    #[test]
    fn only_the_first_release_after_a_press_is_ours() {
        // The latch clears as it reports, so the NEXT button-up — which may well
        // be a click on the game — is not eaten too.
        let latch = PressLatch::new();
        latch.press(0);

        assert_eq!(latch.release(), Some(0));
        assert_eq!(latch.release(), None);
    }

    #[test]
    fn a_declined_press_strands_no_latch_for_the_game_to_lose_a_release_to() {
        // A press we claimed whose hot rect then went away (rect read failed,
        // content cleared, button unmounted) leaves the latch set with no
        // release of ours ever coming. The next press belongs to the game, and
        // the hook clears the latch on it — otherwise that press's own release
        // is eaten and the game is left holding the button.
        let latch = PressLatch::new();
        latch.press(1);

        latch.clear();

        assert_eq!(latch.release(), None);
    }
}

#[cfg(test)]
mod registry_tests {
    use super::{register_in, unregister_in, HookedWindow, HotRect};

    /// A registered window that has declared everything a page can declare, so
    /// a re-register either preserves all three or drops all three.
    fn declared(label: &str, hwnd: isize) -> HookedWindow {
        HookedWindow {
            hwnd,
            hot: vec![HotRect { x: 10, y: 20, w: 30, h: 40 }],
            has_content: true,
            config_mode: true,
            ..HookedWindow::new(label)
        }
    }

    #[test]
    fn re_registering_the_same_hwnd_keeps_what_the_page_declared() {
        // The 1 s `set_overlay_clickthrough` delay is long enough for the page
        // to declare its rects, its content and — via Settings → Configure —
        // its config mode. Dropping those here would leave the window inert,
        // and a window still in config mode neither interactive nor hooked.
        let mut windows = vec![declared("comparator", 0x1234)];

        register_in(&mut windows, "comparator", 0x1234);

        assert_eq!(windows[0].hot.len(), 1);
        assert!(windows[0].has_content);
        assert!(windows[0].config_mode);
    }

    #[test]
    fn registering_a_different_hwnd_drops_what_the_previous_page_declared() {
        // A new HWND under the same label is a NEW window: the old rects
        // describe a page that is gone, and its config mode ended with it.
        let mut windows = vec![declared("comparator", 0x1234)];

        register_in(&mut windows, "comparator", 0x5678);

        assert!(windows[0].hot.is_empty());
        assert!(!windows[0].has_content);
        assert!(!windows[0].config_mode);
        assert_eq!(windows[0].hwnd, 0x5678);
    }

    #[test]
    fn registering_a_label_the_page_already_spoke_for_adopts_its_declarations() {
        // hwnd 0 is the entry `set_overlay_hot_rects` creates when the page
        // beats the setup call. It is not a previous window, so nothing is
        // dropped.
        let mut windows = vec![declared("comparator", 0)];

        register_in(&mut windows, "comparator", 0x1234);

        assert_eq!(windows[0].hwnd, 0x1234);
        assert_eq!(windows[0].hot.len(), 1);
        assert!(windows[0].has_content);
    }

    #[test]
    fn registering_an_unknown_label_appends_an_entry() {
        let mut windows = vec![declared("comparator", 0x1234)];

        register_in(&mut windows, "temple", 0x9999);

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[1].label, "temple");
        assert_eq!(windows[1].hwnd, 0x9999);
    }

    #[test]
    fn unregistering_the_last_window_reports_the_registry_empty() {
        // True is the tear-the-hook-down signal.
        let mut windows = vec![declared("comparator", 0x1234)];

        assert!(unregister_in(&mut windows, "comparator"));
        assert!(windows.is_empty());
    }

    #[test]
    fn unregistering_one_of_several_windows_does_not_report_the_registry_empty() {
        // The hook has to outlive the first overlay to close — the others still
        // need their WS_EX_TRANSPARENT repaired.
        let mut windows = vec![declared("comparator", 0x1234), declared("temple", 0x5678)];

        assert!(!unregister_in(&mut windows, "comparator"));
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "temple");
    }

    #[test]
    fn unregistering_an_unknown_label_changes_nothing_and_reports_nothing() {
        // An empty registry plus a removal that removed nothing is not a
        // teardown signal: no hook was ever running for the caller to stop.
        let mut windows: Vec<HookedWindow> = Vec::new();

        assert!(!unregister_in(&mut windows, "comparator"));
        assert!(windows.is_empty());
    }

    #[test]
    fn unregistering_an_unknown_label_leaves_the_other_windows_in_place() {
        let mut windows = vec![declared("comparator", 0x1234)];

        assert!(!unregister_in(&mut windows, "temple"));
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "comparator");
    }
}
