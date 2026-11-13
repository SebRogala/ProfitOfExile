//! The lab's share of the app's one Client.txt consumer (POE-342): the
//! catch-up replay and the per-line hooks `spawn_log_watcher` calls. The
//! watcher keeps the call order; this module keeps the lab state between lines.

use std::path::Path;
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager};

use super::commands::{publish_lab_detection, record_lab_detection, send_font_session_data, send_gems_to_server};
use crate::{app_log, client_log, emit_status, font_session, lab_navigation, lab_state, AppState};

/// Lab state the log watcher carries from one Client.txt line to the next.
pub(crate) struct LabWatchState {
    state_machine: lab_state::LabStateMachine,
    detected_gems: Vec<String>,
    last_trial_entered: std::time::Instant,
    font_opened_count: u32,
}

impl LabWatchState {
    pub(crate) fn new() -> Self {
        Self {
            state_machine: lab_state::LabStateMachine::new(),
            detected_gems: Vec::new(),
            last_trial_entered: std::time::Instant::now() - std::time::Duration::from_secs(10),
            font_opened_count: 0,
        }
    }
}

/// Catchup: replay recent Client.txt history to reconstruct lab state.
pub(crate) fn catch_up(app: &AppHandle, client_txt: &Path, tail: Option<&str>) {
    let (replay_events, was_in_lab) = lab_navigation::replay_recent_tail(client_txt, tail);
    if !replay_events.is_empty() {
        app_log(app, format!("Catchup: replaying {} events from Client.txt history", replay_events.len()));
        let state = app.state::<AppState>();
        state.in_lab.store(was_in_lab, std::sync::atomic::Ordering::SeqCst);
        // Emit all replay events to frontend so overlays reconstruct navigation
        for event in &replay_events {
            if let Err(e) = app.emit("lab-nav", event) {
                log::warn!("catchup emit failed: {}", e);
            }
        }
        app_log(app, format!("Catchup complete: in_lab={}", was_in_lab));
    }
}

/// Aspirant's Trial / Plaza tracking (outside state machine).
pub(crate) fn on_trial_line(app: &AppHandle, line: &str, watch: &mut LabWatchState) {
    if line.contains("You have entered") {
        let state = app.state::<AppState>();
        if line.contains("Aspirants' Plaza") || line.contains("Aspirant's Plaza") {
            state.aspirant_trial_count.store(0, Ordering::SeqCst);
            watch.font_opened_count = 0;
            app_log(app, "Aspirants' Plaza — trial counter reset".to_string());
        } else if line.contains("Aspirant's Trial") {
            // Dedup: same Trial zone within 0.5s is a log batch artifact.
            if watch.last_trial_entered.elapsed() >= std::time::Duration::from_millis(500) {
                watch.last_trial_entered = std::time::Instant::now();
                let count = state.aspirant_trial_count.fetch_add(1, Ordering::SeqCst) + 1;
                app_log(app, format!("Aspirant's Trial #{}", count));
                if count == 3 {
                    app_log(app, "3rd Aspirant's Trial — final Izaro fight".to_string());
                }
            }
        }
    }
}

/// Lab navigation events, then the lab state machine, in that order.
pub(crate) fn on_client_line(app: &AppHandle, line: &str, watch: &mut LabWatchState) {
    // --- Lab navigation events (outside state machine) ---
    {
        let state = app.state::<AppState>();
        let in_lab = state.in_lab.load(Ordering::SeqCst);
        let mut nav_emitted = false;
        if let Some(nav_event) = lab_navigation::parse_nav_event(line, in_lab) {
            match &nav_event {
                lab_navigation::NavEvent::PlazaEntered => {
                    state.lab_scan.detection.lock().unwrap_or_else(|e| e.into_inner()).reset();
                    state.in_lab.store(true, Ordering::SeqCst);
                    app_log(app, "Lab nav: Plaza entered".to_string());
                }
                lab_navigation::NavEvent::LabStarted => {
                    app_log(app, "Lab nav: Izaro started".to_string());
                }
                lab_navigation::NavEvent::RoomChanged { name } => {
                    app_log(app, format!("Lab nav: room {}", name));
                }
                lab_navigation::NavEvent::LabExited => {
                    state.in_lab.store(false, Ordering::SeqCst);
                    app_log(app, "Lab nav: exited lab".to_string());
                    // The state machine only emits `ZoneChanged`
                    // from FontReady/PickingGems, so a scan
                    // spawned at LabFinished for a player who
                    // never opened the font has no other stop.
                    state.lab_scan.font_scan_generation.fetch_add(1, Ordering::SeqCst);
                    let was_live = font_session::font_scan_is_live(
                        state.lab_scan.font_scan_live_gen.swap(0, Ordering::SeqCst),
                    );
                    if was_live {
                        app_log(app, "Font scan stopped (lab exited)".to_string());
                    }
                    // Emit the navigation event so frontend state resets on LabExited.
                    if let Err(e) = app.emit("lab-nav", &nav_event) {
                        log::warn!("emit lab-nav (LabExited) failed: {}", e);
                    }
                    nav_emitted = true;
                }
                lab_navigation::NavEvent::LabFinished => {
                    app_log(app, "Lab nav: Izaro defeated! Starting font panel OCR".to_string());
                    watch.font_opened_count = 0;
                    super::commands::spawn_font_scan(app);
                }
                lab_navigation::NavEvent::SectionFinished => {
                    app_log(app, "Lab nav: section finished".to_string());
                }
                lab_navigation::NavEvent::IzaroBattleStarted => {
                    app_log(app, "Lab nav: Izaro battle started".to_string());
                }
                lab_navigation::NavEvent::PortalSpawned => {
                    app_log(app, "Lab nav: portal spawned".to_string());
                }
                lab_navigation::NavEvent::DarkshrineActivated => {
                    app_log(app, "Lab nav: darkshrine activated".to_string());
                }
            }
            if !nav_emitted {
                if let Err(e) = app.emit("lab-nav", &nav_event) {
                    log::warn!("emit lab-nav failed: {}", e);
                }
            }
        }
    }

    if let Some(event) = watch.state_machine.process_line(line) {
        let state = app.state::<AppState>();
        match &event {
            lab_state::LabEvent::FontOpened => {
                // Publish the event before anything else: the
                // craft ledger reads this counter per OCR frame
                // and a frame captured after the click must see
                // the new value, or the count change it carries
                // reads as a misread.
                state.lab_scan.font_opened_seq.fetch_add(1, Ordering::SeqCst);
                // The event cannot seal a round — it fires on
                // font open as well as on CRAFT, an unbounded
                // number of times per craft. Only the panel's
                // craft count delimits rounds (`font_ledger`).
                watch.font_opened_count += 1;
                // A portal trip out of the lab stops the font
                // scan for good (`LabFinished` does not fire
                // again on return), so the liveness token is
                // what decides whether this event has to bring
                // the panel OCR back.
                let font_scan_live = font_session::font_scan_is_live(
                    state.lab_scan.font_scan_live_gen.load(Ordering::SeqCst),
                );
                for effect in font_session::font_opened_effects(watch.font_opened_count, font_scan_live) {
                    match effect {
                        font_session::FontOpenedEffect::StartGemScan => {
                            // Odd: CRAFT click → start gem scan
                            watch.detected_gems.clear();
                            super::commands::spawn_gem_scan(app, "font");
                            app_log(app, format!("FontOpened #{} — CRAFT, gem scan started", watch.font_opened_count));
                        }
                        font_session::FontOpenedEffect::StopGemScan => {
                            // Even: CONFIRM click → stop scanning, clear comparator
                            state.lab_scan.gem_scan_generation.fetch_add(1, Ordering::SeqCst);
                            *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) =
                                lab_state::LabState::Idle;
                            watch.detected_gems.clear();
                            *state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner()) = Vec::new();
                            if let Err(e) = app.emit("gems-cleared", ()) { log::warn!("emit gems-cleared failed: {}", e); }
                            app_log(app, format!("FontOpened #{} — CONFIRM, gem scan stopped", watch.font_opened_count));
                        }
                        font_session::FontOpenedEffect::RearmFontScan => {
                            // `ZoneChanged` owns the session
                            // reset, so this starts a fresh
                            // segment rather than resuming one.
                            app_log(app, "Font scan re-armed".to_string());
                            super::commands::spawn_font_scan(app);
                        }
                    }
                }
            }
            lab_state::LabEvent::ZoneChanged { area } => {
                app_log(app, format!("Zone changed: {} — stopping", area));

                // Stop both gem and font scans + reset state.
                state.lab_scan.gem_scan_generation.fetch_add(1, Ordering::SeqCst);
                state.lab_scan.font_scan_generation.fetch_add(1, Ordering::SeqCst);
                // Synchronously, not by waiting for the loop to
                // notice: a `FontOpened` arriving in that window
                // must still see a dead scan and re-arm it.
                state.lab_scan.font_scan_live_gen.store(0, Ordering::SeqCst);
                watch.font_opened_count = 0;
                *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) =
                    lab_state::LabState::Idle;

                if !watch.detected_gems.is_empty() {
                    let gems = watch.detected_gems.clone();
                    let app_clone = app.clone();
                    tauri::async_runtime::spawn(async move {
                        send_gems_to_server(&app_clone, gems).await;
                    });
                    watch.detected_gems.clear();
                }

                // Send accumulated font session data to server.
                send_font_session_data(app);

                // Clear frontend comparator — player left the area.
                *state.lab_scan.detected_gems.lock().unwrap_or_else(|e| e.into_inner()) =
                    Vec::new();
                if let Err(e) = app.emit("gems-cleared", ()) { log::warn!("emit gems-cleared failed: {}", e); }
                emit_status(app);
            }
            lab_state::LabEvent::FontClosed => {
                app_log(app, "Font closed".to_string());
                *state.lab_state.lock().unwrap_or_else(|e| e.into_inner()) =
                    lab_state::LabState::Idle;
                emit_status(app);
            }
            // GameFocused/GameBlurred handled by the focus poller
            // (GetForegroundWindow — more reliable than Client.txt)
            lab_state::LabEvent::GameFocused | lab_state::LabEvent::GameBlurred => {}
        }
    }
}

/// The lab half of an area change: the detection observes the new area.
pub(crate) fn on_area_change(app: &AppHandle, change: &client_log::AreaChange<'_>) {
    let state = app.state::<AppState>();
    let update = record_lab_detection(&state, |detection| detection.observe_area(change.level, change.area));
    publish_lab_detection(app, update);
}
