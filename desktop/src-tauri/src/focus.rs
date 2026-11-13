#[cfg(windows)]
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};
#[cfg(windows)]
use tauri::Emitter;

#[cfg(windows)]
use crate::{capture, emit_status, overlay_hook, FocusState};
use crate::AppState;

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

/// Poll GetForegroundWindow to detect game focus changes.
/// More reliable than Client.txt log events (no latency, works if PoE crashes).
/// Runs every 1 second on a dedicated thread.
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
            std::thread::sleep(std::time::Duration::from_millis(1000));

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

                // On the transition INTO the game, and only there: resolve
                // which display the game window is on so every capture that
                // follows grabs THAT monitor (POE-237). The transition is the
                // right cadence — a player who moves the game to another screen
                // alt-tabs out and back to do it, and the per-second poll would
                // otherwise re-ask a question whose answer changes a handful of
                // times a session. The call logs and emits only when the answer
                // actually moved.
                //
                // BEFORE the `game_in_foreground` store, not inside the
                // transition block below it: that flag is what the merc capture
                // loop gates on, so a tick that read it between the store and
                // this lookup would grab the display the game had just left —
                // exactly the wrong-monitor frame POE-237 exists to stop. The
                // `!was_focused` term is what keeps this on the transition; the
                // block below owns the assignment to `was_focused`.
                if is_focused && !was_focused {
                    capture::remember_game_monitor(&app, game_hwnd);
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
