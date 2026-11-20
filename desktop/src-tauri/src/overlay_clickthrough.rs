//! Overlay click-through setup (POE-443): the `set_overlay_clickthrough`
//! command, its Windows arming half and the outcome mapping its tests drive.

use tauri::AppHandle;
#[cfg(windows)]
use tauri::Manager;

#[cfg(windows)]
use crate::{overlay_hook, AppState};

/// What the delayed click-through setup ended up observing.
///
/// The setup itself is Windows-only and needs a real WebView2 HWND, so the
/// MAPPING from what it saw to what the command answers is split out below
/// (`clickthrough_outcome`): that half is what decides whether the caller keeps
/// the window or tears it down, and it is the half a test can drive.
///
/// Every variant but `Armed` is constructed on Windows only — off Windows there
/// is no extended style to fail at — so a `cargo check` on Linux would call the
/// other four unconstructed. The mapping below runs on BOTH platforms and its
/// tests build all five, which is what the allow is scoped to.
/// `make desktop-check-windows` type-checks the Windows half.
///
/// The variants are NOT equally likely. `WindowGone`, `HwndUnavailable` and
/// `NotTransparent` are the live signals; `IgnoreCursorFailed` is close to
/// unreachable, because tao's `set_ignore_cursor_events` returns `Ok`
/// unconditionally on Windows and Tauri only errors when the event loop is
/// already gone. It is kept because "close to unreachable" is not "cannot
/// happen" and the alternative is swallowing it, which is the defect this
/// command was fixed for.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ClickthroughSetup {
    /// Click-through installed — or deliberately NOT re-armed because the user
    /// is already arranging widgets in this window — and the window registered
    /// with the mouse hook. The only success.
    Armed,
    /// The label was gone by the time the delay elapsed; there was nothing to
    /// arm. Ordinary during a fast module off→on, and still a failed creation:
    /// the caller must not report a window it no longer has. Its message
    /// carries [`CLICKTHROUGH_WINDOW_GONE`] so the caller can tell this
    /// ordinary case from a window that is live and eating clicks.
    WindowGone,
    /// `set_ignore_cursor_events(true)` refused. Near-unreachable in practice
    /// (see the type comment): the Windows implementation cannot fail, so this
    /// really means the event loop has gone away underneath us.
    IgnoreCursorFailed(String),
    /// It was applied, and `WS_EX_TRANSPARENT` did not read back set. The
    /// window is opaque to the mouse, and the hook only repairs a window the
    /// cursor is already over — so nothing will fix this on its own.
    NotTransparent,
    /// The HWND never became available, so the window was never registered and
    /// the hook cannot repair its style later either.
    HwndUnavailable,
}

/// The marker `WindowGone`'s message starts with.
///
/// A machine-readable prefix rather than a structured error, because the whole
/// contract is one string across the Tauri boundary and exactly ONE of these
/// failures is ordinary: a window destroyed inside the command's 1 s wait is
/// what an overlay toggled off mid-creation looks like, and reporting it as
/// "this window may be eating your clicks" would cry wolf on every fast toggle.
/// The caller matching it is `clickthroughReport` in
/// `src/lib/overlay/clickthrough-report.ts`, which keeps the same literal; the
/// test below is what stops this end of the pair drifting.
pub(super) const CLICKTHROUGH_WINDOW_GONE: &str = "window-gone";

#[cfg(any(windows, test))]
pub(super) fn clickthrough_failure_log_level(message: &str) -> log::Level {
    if message.starts_with(CLICKTHROUGH_WINDOW_GONE) {
        log::Level::Info
    } else {
        log::Level::Error
    }
}

/// Turn what the setup observed into the answer `set_overlay_clickthrough`
/// gives its caller.
///
/// Only `Armed` is success. Every other variant leaves a transparent,
/// always-on-top window that takes clicks meant for the game — and for a
/// widget-engine window, which is the size of the monitor, that is EVERY click
/// on the screen until the window is destroyed. So the failure is returned
/// rather than logged: the module-coupled callers destroy the half-built window
/// and let `module-lifecycle.ts` retry it, and the rest at least say so in the
/// app log.
pub(super) fn clickthrough_outcome(label: &str, setup: ClickthroughSetup) -> Result<(), String> {
    match setup {
        ClickthroughSetup::Armed => Ok(()),
        ClickthroughSetup::WindowGone => Err(format!(
            "{}: Overlay '{}' not found after the setup delay",
            CLICKTHROUGH_WINDOW_GONE, label
        )),
        ClickthroughSetup::IgnoreCursorFailed(e) => Err(format!(
            "set_ignore_cursor_events failed for '{}': {}",
            label, e
        )),
        ClickthroughSetup::NotTransparent => Err(format!(
            "Overlay '{}' is not click-through after setup: WS_EX_TRANSPARENT did not read back",
            label
        )),
        ClickthroughSetup::HwndUnavailable => Err(format!(
            "Overlay '{}' HWND not available after the setup delay — not registered with the hook",
            label
        )),
    }
}

/// Set up an overlay window for click-through and REGISTER it with the hook.
/// Call from JS after the window is created. Delays 1s for HWND availability.
///
/// Every overlay registers, not only the interactive one: the hook is the only
/// thing that repairs the `WS_EX_TRANSPARENT` WebView2 strips off when it
/// rebuilds child windows, and it repairs what it tracks. Which clicks a window
/// then CLAIMS is a separate declaration — `set_overlay_hot_rects`, sent by the
/// window's own page — so this command no longer takes an interactive width.
///
/// AWAITED, and the outcome is the caller's to act on. The 1 s wait stays (the
/// WebView2 HWND is not available sooner — see the guide's runtime-earned
/// observations), but it is now spent inside the command rather than on a
/// detached thread the caller cannot observe. It used to return immediately and
/// only LOG a failed `set_ignore_cursor_events` or a missing HWND, which meant
/// a window that never became click-through was indistinguishable from one that
/// did: for the monitor-sized widget window that is an invisible, always-on-top
/// rectangle eating every click on the screen until it is destroyed.
///
/// The 500 ms `set_noactivate` repair below stays fire-and-forget on its own
/// thread. It is a REPAIR of a style WebView2 may strip while it builds its
/// children, not a gate — the window is already click-through without it, and
/// making creation wait another half-second on it would buy nothing.
#[tauri::command]
pub(crate) async fn set_overlay_clickthrough(label: String, app: AppHandle) -> Result<(), String> {
    apply_overlay_clickthrough(label, app).await
}

/// Nothing to arm where there is no Win32 window style to arm it with. Reported
/// as success so a Linux dev build's creation paths behave like the shipped
/// ones instead of tearing every overlay down on startup.
#[cfg(not(windows))]
async fn apply_overlay_clickthrough(label: String, app: AppHandle) -> Result<(), String> {
    let _ = app;
    clickthrough_outcome(&label, ClickthroughSetup::Armed)
}

#[cfg(windows)]
async fn apply_overlay_clickthrough(label: String, app: AppHandle) -> Result<(), String> {
    let for_thread = label.clone();
    // `spawn_blocking`, not `spawn`: the body sleeps a second and then makes
    // blocking Tauri and Win32 calls. Awaiting the join handle is what makes
    // the failure reach the caller.
    let setup = tauri::async_runtime::spawn_blocking(move || clickthrough_setup(for_thread, app))
        .await
        .map_err(|e| format!("click-through setup for '{}' did not run: {}", label, e))?;

    let outcome = clickthrough_outcome(&label, setup);
    match &outcome {
        Ok(()) => log::info!(
            "Overlay clickthrough setup complete for '{}' (registered with the mouse hook)",
            label
        ),
        // Logged here as well as returned: the caller reports through the app
        // log too, but a Rust-side line is what pairs the failure with the
        // Win32 detail that produced it.
        Err(msg) => log::log!(clickthrough_failure_log_level(msg), "{}", msg),
    }
    outcome
}

/// The blocking half: wait for the HWND, arm the window, register it, and
/// report what happened. Runs off the main thread.
#[cfg(windows)]
fn clickthrough_setup(label: String, app: AppHandle) -> ClickthroughSetup {
    use windows::Win32::Foundation::HWND;

    // WebView2 HWND not available immediately — wait for init
    std::thread::sleep(std::time::Duration::from_millis(1000));

    let window = match app.get_webview_window(&label) {
        Some(w) => w,
        None => return ClickthroughSetup::WindowGone,
    };

    // This setup runs a full second after the window was created, and
    // the user can have opened widget configuration inside that second.
    // Re-asserting click-through then would leave the window neither
    // interactive (cursor events ignored again) nor hooked (the hook
    // still sees `config_mode` and keeps its hands off it), with
    // nothing to undo it until config mode is closed. Registration
    // itself is unconditional — it is what the hook needs to repair
    // WS_EX_TRANSPARENT once config mode ends, and `register` keeps the
    // flag for a same-HWND re-register.
    let in_config = overlay_hook::config_mode(&label);
    if in_config {
        log::info!(
            "Overlay '{}' is in widget-configuration mode — registering without re-arming click-through",
            label
        );
    } else if let Err(e) = window.set_ignore_cursor_events(true) {
        // The call that makes the whole window click-through. Its refusal used
        // to be logged and swallowed; it is now the command's answer.
        return ClickthroughSetup::IgnoreCursorFailed(e.to_string());
    }

    // ALSO A BARRIER, not just a lookup. `set_ignore_cursor_events` above posts
    // its work to the event loop and returns; `hwnd()` is a blocking round-trip
    // to that same loop, so by the time it answers the style call has been
    // serviced. That is what makes the belt at the foot of this function a
    // read-back rather than a race. Moving the belt above this call, or making
    // this lookup non-blocking, breaks that ordering silently.
    let hwnd = match window.hwnd() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            // The variant carries no payload — the Win32 reason is worth a line
            // of its own rather than a wider enum only this log would read.
            log::warn!("Overlay '{}' HWND lookup failed: {}", label, e);
            return ClickthroughSetup::HwndUnavailable;
        }
    };
    let h = HWND(hwnd.0 as *mut _);
    if !in_config {
        unsafe { overlay_hook::set_noactivate(h); }
    }

    overlay_hook::register(&label, h);

    // Idempotent: the first overlay to get here installs the hook,
    // every later one reuses it and gets `None`.
    if let Some(tx) = overlay_hook::install_hook(app.clone()) {
        let state = app.state::<AppState>();
        *state.overlay_hook_stop.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    }

    // Re-apply WS_EX_NOACTIVATE after WebView2 children are created.
    // Re-checked, not inherited from `in_config`: config mode can be
    // entered during these 500 ms too, and WS_EX_NOACTIVATE on a
    // window the user is dragging widgets in stops it taking the
    // focus its own drag handles need.
    //
    // It resolves the LABEL again rather than reusing the handle captured here,
    // and that is the point of the re-lookup: this command can now report a
    // failure, and its callers answer one by DESTROYING the window. Win32
    // recycles HWND values, so a raw handle half a second old can name somebody
    // else's window by the time this runs. No window, no repair — and a window
    // rebuilt under the same label in the meantime is a window that wants
    // WS_EX_NOACTIVATE anyway.
    let label3 = label.clone();
    let app_for_repair = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let Some(window) = app_for_repair.get_webview_window(&label3) else {
            log::info!(
                "Overlay '{}' is gone — WS_EX_NOACTIVATE not re-applied",
                label3
            );
            return;
        };
        if overlay_hook::config_mode(&label3) {
            log::info!(
                "Overlay '{}' entered widget-configuration mode — WS_EX_NOACTIVATE not re-applied",
                label3
            );
            return;
        }
        match window.hwnd() {
            Ok(h) => unsafe { overlay_hook::set_noactivate(HWND(h.0 as *mut _)) },
            Err(e) => log::warn!(
                "Overlay '{}' HWND unavailable — WS_EX_NOACTIVATE not re-applied: {}",
                label3, e
            ),
        }
    });

    // The belt: ask the window back what it now carries. `set_ignore_cursor_events`
    // answering `Ok` is not proof the extended style took, and the one thing that
    // would otherwise repair it — the hook — only acts on a window the cursor is
    // already over. Read AFTER registration rather than instead of it: the
    // registration is wanted even on the path that reports a failure, because a
    // caller that keeps the window still wants it repairable.
    if !in_config && !clickthrough_belt_passes(h) {
        return ClickthroughSetup::NotTransparent;
    }

    ClickthroughSetup::Armed
}

/// How many times the belt reads the style back before calling it missing, and
/// how long it waits between reads.
///
/// More than one because a verdict that destroys the user's overlay must not
/// rest on a single sample: the `hwnd()` barrier above orders the style call
/// ahead of this read, but WebView2 is still building children underneath it,
/// and it is that child-building which strips `WS_EX_TRANSPARENT` in the first
/// place. Three reads 20 ms apart cost 40 ms on the failing path and nothing at
/// all on the passing one, which is the first read.
#[cfg(windows)]
const CLICKTHROUGH_BELT_READS: u32 = 3;
#[cfg(windows)]
const CLICKTHROUGH_BELT_GAP_MS: u64 = 20;

/// Whether the window reads back as click-through.
///
/// Only a style that is READABLE AND MISSING on every attempt fails. An
/// unreadable style is UNKNOWN, not missing (see `overlay_hook::is_transparent`),
/// and tearing down a window we could not measure would cost working overlays on
/// a Win32 hiccup.
#[cfg(windows)]
fn clickthrough_belt_passes(h: windows::Win32::Foundation::HWND) -> bool {
    for attempt in 1..=CLICKTHROUGH_BELT_READS {
        match unsafe { overlay_hook::is_transparent(h) } {
            Some(false) => {
                if attempt == CLICKTHROUGH_BELT_READS {
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(CLICKTHROUGH_BELT_GAP_MS));
            }
            _ => return true,
        }
    }
    true
}
