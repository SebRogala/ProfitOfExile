#[cfg(windows)]
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};
#[cfg(windows)]
use tauri::Emitter;

#[cfg(windows)]
use crate::{capture, emit_status, overlay_hook, FocusState};
use crate::AppState;

#[cfg(windows)]
static EDITABLE_FOCUS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<isize, bool>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

#[tauri::command]
pub(crate) fn report_read_editable_focus(window: tauri::WebviewWindow, editable: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        EDITABLE_FOCUS.lock().unwrap_or_else(|e| e.into_inner()).insert(hwnd.0 as isize, editable);
    }
    #[cfg(not(windows))]
    let _ = (window, editable);
    Ok(())
}

#[cfg(windows)]
pub(crate) fn read_focus() -> (FocusState, bool) {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, GetClassNameW};
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        if hwnd.0.is_null() || GetWindowThreadProcessId(hwnd, Some(&mut pid)) == 0 {
            return (FocusState::Other, true);
        }
        if pid == std::process::id() {
            let editable = EDITABLE_FOCUS.lock().unwrap_or_else(|e| e.into_inner())
                .get(&(hwnd.0 as isize)).copied().unwrap_or(true);
            return (FocusState::OwnWindow, editable);
        }
        let mut class = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut class);
        let kind = if len > 0 && String::from_utf16_lossy(&class[..len as usize]) == "POEWindowClass" {
            FocusState::Game
        } else {
            FocusState::Other
        };
        (kind, false)
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn refresh_focus_geometry(
    entering: bool,
    resolve_monitor: impl FnOnce(),
    refresh_client: impl FnOnce(),
) {
    if entering {
        resolve_monitor();
    }
    refresh_client();
}

/// What the focus poller should do to ONE overlay window on a focus transition.
///
/// `Some(true)` show, `Some(false)` hide, `None` leave it exactly as it is.
///
/// `gate_met` is that overlay's own condition — game focus for the
/// temple/merc/lab group.
/// The two suppressors are why this is a function rather than an `if` in the
/// loop: both make a HIDE wrong while leaving a SHOW right, and both are
/// invisible when they are missing.
///
/// `debug` is the long-standing one (Ctrl+Shift+F12 force-shows every overlay,
/// and an alt-tab must not undo it). `config_mode` is POE-226's: while the user
/// is arranging widgets, that window is interactive and carries the only Save
/// and Cancel there are. The poller runs on transitions, so an `Other` window
/// taking the foreground for a moment mid-session would hide it once and leave
/// it hidden — config mode still on, the game still eating nothing, and no way
/// out but a second Configure press.
// Its only non-test caller is `apply_overlay_focus`, which is Windows-only.
// The decision lives out here anyway so it can be driven on Linux, which is
// where this suite runs.
#[cfg_attr(not(windows), allow(dead_code))]
fn overlay_focus_action(gate_met: bool, debug: bool, config_mode: bool) -> Option<bool> {
    if gate_met {
        return Some(true);
    }
    if debug || config_mode {
        return None;
    }
    Some(false)
}

/// Apply [`overlay_focus_action`] to a named window, if it exists.
#[cfg(windows)]
fn apply_overlay_focus(app: &AppHandle, overlay_name: &str, gate_met: bool, debug: bool) {
    let Some(win) = app.get_webview_window(overlay_name) else {
        return;
    };
    match overlay_focus_action(gate_met, debug, overlay_hook::config_mode(overlay_name)) {
        Some(true) => {
            if let Err(e) = win.show() {
                log::warn!("Failed to show {} overlay: {}", overlay_name, e);
            }
        }
        Some(false) => {
            if let Err(e) = win.hide() {
                log::warn!("Failed to hide {} overlay: {}", overlay_name, e);
            }
        }
        None => {}
    }
}

/// The focus poller's cadence; also the client-rect refresh cadence while the
/// game is in front. Reversible interim pending the Operator's Windows cost
/// measurement (POE-272).
const FOCUS_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1000);

/// Poll GetForegroundWindow to detect game focus changes.
/// More reliable than Client.txt log events (no latency, works if PoE crashes).
/// Runs every [`FOCUS_POLL_INTERVAL`] on a dedicated thread.
///
/// Three-state logic:
///   - Foreground is PoE → game focused, show overlay
///   - Foreground is our own process (overlay/main window) → neutral, keep current state
///   - Foreground is anything else → game not focused, hide overlay
pub(crate) fn spawn_focus_poller(app: AppHandle) {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    {
        let state = app.state::<AppState>();
        *state.focus_poller_stop.lock().unwrap_or_else(|e| {
            log::warn!("focus_poller_stop mutex poisoned, recovering");
            e.into_inner()
        }) = Some(stop_tx);
    }

    std::thread::spawn(move || {
        #[cfg(windows)]
        let mut was_focused = false;
        #[cfg(windows)]
        let our_pid = std::process::id();

        loop {
            std::thread::sleep(FOCUS_POLL_INTERVAL);

            // Check stop signal
            if stop_rx.try_recv().is_ok() {
                log::info!("Focus poller stopped");
                break;
            }

            #[cfg(windows)]
            {
                use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

                // The HWND comes out WITH the verdict (POE-237). The game's
                // window is the only thing that can say which display the game
                // is on, and asking again after the block would be asking about
                // whatever is in front by then — which, one alt-tab later, is a
                // different window on a different monitor.
                let (focus_state, game_hwnd) = unsafe {
                    let fg = GetForegroundWindow();
                    if fg.0.is_null() {
                        (FocusState::Other, fg) // no foreground window → treat as blur
                    } else {
                        // Check if this window belongs to our process (overlay, main window).
                        let mut fg_pid: u32 = 0;
                        let tid = GetWindowThreadProcessId(fg, Some(&mut fg_pid));
                        if tid == 0 {
                            // HWND invalidated between GetForegroundWindow and here (TOCTOU).
                            // Fall through to title-based detection.
                            (FocusState::Other, fg)
                        } else if fg_pid == our_pid {
                            (FocusState::OwnWindow, fg)
                        } else {
                            // Check window class (more reliable than title — avoids
                            // matching browser tabs like "Path of Exile Trade")
                            use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
                            let mut cls_buf = [0u16; 256];
                            let cls_len = GetClassNameW(fg, &mut cls_buf);
                            if cls_len > 0 {
                                let class_name = String::from_utf16_lossy(&cls_buf[..cls_len as usize]);
                                if class_name == "POEWindowClass" {
                                    (FocusState::Game, fg)
                                } else {
                                    (FocusState::Other, fg)
                                }
                            } else {
                                (FocusState::Other, fg)
                            }
                        }
                    }
                };

                let is_focused = matches!(focus_state, FocusState::Game);

                // Resolve the monitor before the client on focus entry, then
                // let changed client geometry refresh it during focused polls.
                // Both precede foreground readiness: the merc capture loop
                // gates on that flag and must see the refreshed display.
                if is_focused {
                    refresh_focus_geometry(
                        !was_focused,
                        || capture::remember_game_monitor(&app, game_hwnd),
                        || capture::remember_game_client(&app, game_hwnd),
                    );
                }

                app.state::<AppState>().game_in_foreground.store(is_focused, Ordering::SeqCst);
                // When foreground is our own window (overlay button click, main app),
                // don't change game_focused — preserve the last known state.
                if matches!(focus_state, FocusState::OwnWindow) {
                    continue;
                }

                if is_focused != was_focused {
                    was_focused = is_focused;
                    let state = app.state::<AppState>();
                    *state.game_focused.lock().unwrap_or_else(|e| {
                        log::warn!("game_focused mutex poisoned, recovering");
                        e.into_inner()
                    }) = is_focused;
                    // Reserved, not consumed: nothing has listened for this since
                    // `manager.ts`'s `initFocusListener` was deleted with its other
                    // uncalled window-manager functions (POE-225). Show/hide is
                    // done below, in Rust, and the frontend reads focus off
                    // `AppStatus`. Kept because it is the only push signal a future
                    // window would have, and because removing it is a contract
                    // change to any page that starts listening.
                    if let Err(e) = app.emit("game-focus-changed", is_focused) {
                        log::warn!("emit game-focus-changed failed: {}", e);
                    }
                    emit_status(&app);

                    // Hide/show overlay windows based on game focus.
                    // Comparator + Temple + Merc + Lab: show whenever game is focused.
                    // Skip hide in debug mode.
                    let debug = *state.debug_mode.lock().unwrap_or_else(|e| e.into_inner());

                    // Game focus only. The temple is here, and not gated on the lab
                    // lifecycle, because its board is read from the Atlas/map UI, which
                    // has nothing to do with the labyrinth lifecycle; its route
                    // still gates on the module's own status, so a shown window
                    // with no board on screen draws nothing. The merc verdict
                    // overlay (POE-199) joins them for the same reason — a
                    // recruit window has nothing to do with the lab either.
                    //
                    // This list runs on `game_focused`, the HELD read, while the
                    // merc capture loop gates on the raw `game_in_foreground`
                    // (see the two fields on `AppState`). The two are never
                    // unified: holding the flag over our own windows is what
                    // stops an overlay click from blanking every overlay, and
                    // NOT holding it is what stops the capture loop from
                    // photographing our own window instead of the game.
                    for overlay_name in &["temple", "mercenary", "lab", "loot", "prompt"] {
                        apply_overlay_focus(&app, overlay_name, is_focused, debug);
                    }
                }
            }

            #[cfg(not(windows))]
            {
                let _ = &app;
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::overlay_focus_action;

    mod keep_regression {
        use std::cell::RefCell;

        use crate::capture::{refresh_changed_client, Capture};
        use super::super::refresh_focus_geometry;

        #[test]
        fn windowed_geometry_refreshes_the_monitor_before_accepting_a_changed_client() {
            let source = image::load_from_memory(include_bytes!(
                "../tests/fixtures/lab/font-gems-1080p.png"
            )).unwrap();
            let mut failures = Vec::new();
            for (w, h) in [(1024, 576), (1280, 720), (1366, 768)] {
                let game = source.resize_exact(w, h, image::imageops::FilterType::Lanczos3).to_rgba8();
                let inside = [100, 100, w as i32, h as i32];
                let resized = [100, 100, w as i32 - 160, h as i32 - 90];
                let moved = [2020, 100, w as i32, h as i32];
                let partial = [3500, 90, w as i32, h as i32];
                for (name, origin, previous, raw, expected) in [
                    ("first client", (0, 0), None, inside, vec!["monitor", "client"]),
                    ("unchanged client", (0, 0), Some(inside), inside, vec![]),
                    ("resized client", (0, 0), Some(inside), resized, vec!["monitor", "client"]),
                    ("moved across monitors", (1920, 0), Some(inside), moved, vec!["monitor", "client"]),
                    ("unchanged on second monitor", (1920, 0), Some(moved), moved, vec![]),
                    ("partly outside", (1920, 0), Some(moved), partial, vec!["monitor", "client"]),
                    ("moved back", (0, 0), Some(moved), inside, vec!["monitor", "client"]),
                ] {
                    let mut canvas = image::RgbaImage::from_pixel(
                        1920, 1080, image::Rgba([90, 90, 90, 255]),
                    );
                    image::imageops::overlay(
                        &mut canvas, &game,
                        i64::from(raw[0]) - i64::from(origin.0), i64::from(raw[1]),
                    );
                    let frame = Capture {
                        image: image::DynamicImage::ImageRgba8(canvas),
                        monitor_id: 7, origin, client: raw, timing: Default::default(),
                    };
                    let actions = RefCell::new(Vec::new());
                    refresh_changed_client(
                        previous, frame.client,
                        || actions.borrow_mut().push("monitor"),
                        || actions.borrow_mut().push("client"),
                    );
                    let actual = actions.into_inner();
                    if actual != expected {
                        failures.push(format!(
                            "{w}x{h} {name}: {previous:?} -> {raw:?}: actions {actual:?}, expected {expected:?}",
                        ));
                    }
                }
                for (entering, expected) in [
                    (true, vec!["monitor", "client"]),
                    (false, vec!["client"]),
                ] {
                    let actions = RefCell::new(Vec::new());
                    refresh_focus_geometry(
                        entering,
                        || actions.borrow_mut().push("monitor"),
                        || actions.borrow_mut().push("client"),
                    );
                    let actual = actions.into_inner();
                    if actual != expected {
                        failures.push(format!(
                            "{w}x{h} focus entry={entering}: actions {actual:?}, expected {expected:?}",
                        ));
                    }
                }
            }
            assert!(failures.is_empty(), "windowed monitor refresh contract:\n{}", failures.join("\n"));
        }
    }

    #[test]
    fn focus_poller_shows_an_overlay_whose_gate_is_met() {
        assert_eq!(overlay_focus_action(true, false, false), Some(true));
    }

    #[test]
    fn focus_poller_hides_an_overlay_whose_gate_is_not_met() {
        assert_eq!(overlay_focus_action(false, false, false), Some(false));
    }

    /// Ctrl+Shift+F12 force-shows every overlay; an alt-tab must not undo it.
    #[test]
    fn focus_poller_leaves_an_overlay_alone_in_debug_mode() {
        assert_eq!(overlay_focus_action(false, true, false), None);
    }

    /// POE-226. The window being arranged carries the only Save and Cancel
    /// there are, and the poller runs on TRANSITIONS — one hide would leave it
    /// hidden with config mode still on.
    #[test]
    fn focus_poller_leaves_an_overlay_alone_while_its_widgets_are_being_arranged() {
        assert_eq!(overlay_focus_action(false, false, true), None);
    }

    /// The suppressors hold a hide back, never a show: a window whose gate is
    /// met belongs on screen whatever else is true, and refusing the show would
    /// leave a config session on a window the game is drawing over.
    #[test]
    fn focus_poller_still_shows_a_gated_overlay_that_is_being_arranged() {
        assert_eq!(overlay_focus_action(true, false, true), Some(true));
    }
}
