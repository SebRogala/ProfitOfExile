//! The Divine Font options reader (POE-391): one read of the `lab.font`
//! region per read key press while a font context is armed.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use super::font_context::{self, FontReadRecord};
use super::report_engine_once;
use crate::capture::Capture;
use crate::font_parser::FontPanelState;
use crate::lab_scan::LabScanState;
use crate::read_hotkey::{PendingRead, Press, ScreenReader};
use crate::{app_log, AppState};

pub struct LabFontReader;

/// The token a press captures, or `None` when no font context is armed (the
/// press takes no grab for this reader).
pub(crate) fn armed_token(lab: &LabScanState) -> Option<u64> {
    Some(lab.font_context.load(Ordering::SeqCst)).filter(|token| *token != 0)
}

impl ScreenReader<AppHandle> for LabFontReader {
    fn id(&self) -> crate::read_hotkey::ReaderId { crate::read_hotkey::ReaderId::Font }
    fn prepare(&self, app: &AppHandle, _press: &Press) -> Option<Box<dyn PendingRead<AppHandle>>> {
        let state = app.state::<AppState>();
        if !*state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) { return None; }
        armed_token(&state.lab_scan).map(|token| Box::new(PendingFontRead {
            token,
            prompt: crate::read_prompt::armed_id_in(&state, crate::read_prompt::Mechanic::DivineFont),
            lines: Vec::new(),
        }) as _)
    }
}

struct PendingFontRead {
    token: u64,
    prompt: Option<u64>,
    lines: Vec<String>,
}

impl PendingRead<AppHandle> for PendingFontRead {
    fn probe(&mut self, frame: &Arc<Capture>, app: &AppHandle) -> Result<bool, String> {
        let state = app.state::<AppState>();
        if armed_token(&state.lab_scan) != Some(self.token)
            || !*state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) { return Ok(false); }
        let screen = *state.screen.lock().unwrap_or_else(|e| e.into_inner());
        let [x, y, w, h] = crate::ssot::placements_for(screen.as_ref()).lab.font;
        if x < 0 || y < 0 || w <= 0 || h <= 0 || x as u32 + w as u32 > frame.image.width() || y as u32 + h as u32 > frame.image.height() { return Ok(false); }
        let crop = frame.image.crop_imm(x as u32, y as u32, w as u32, h as u32);
        self.lines = crate::ocr::recognize_text(&crate::capture::preprocess_for_ocr(&crop))?;
        Ok(crate::font_parser::parse_font_panel(&self.lines).font_active)
    }
    fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, app: &AppHandle) {
        if frame.is_err() { return; }
        report_engine_once(app);
        let state = app.state::<AppState>();
        if !*state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) { return; }
        let read = read_font_lines(&state, self.token, &self.lines);
        publish_font_read(app, &self.lines, read, self.prompt);
    }
}

/// One press's OCR lines applied to the state: the parsed panel, and what the
/// session recorded (`None` when the panel was inactive or the context ended).
pub(crate) struct FontLinesRead {
    pub panel: FontPanelState,
    pub record: Option<FontReadRecord>,
}

/// The pure step of a font read: replace the alert set under its own lock
/// (re-checking the token there), then record an active panel in the session.
pub(crate) fn read_font_lines(state: &AppState, captured: u64, lines: &[String]) -> FontLinesRead {
    let panel = crate::font_parser::parse_font_panel(lines);
    if !panel.font_active || captured == 0 || state.lab_scan.font_context.load(Ordering::SeqCst) != captured {
        return FontLinesRead { panel, record: None };
    }
    crate::font_alerts::record_current_alert_read(
        &state.lab_scan.font_alert_seen,
        || state.lab_scan.font_context.load(Ordering::SeqCst) == captured,
        panel.font_active,
        &panel.options,
    );
    let record = if panel.font_active {
        let event_seq = state.lab_scan.font_opened_seq.load(Ordering::SeqCst);
        font_context::record_font_read(state, captured, &panel, event_seq)
    } else {
        None
    };
    FontLinesRead { panel, record }
}

fn publish_font_read(app: &AppHandle, lines: &[String], read: FontLinesRead, prompt: Option<u64>) {
    let FontLinesRead { panel, record } = read;
    if !panel.font_active {
        if !lines.is_empty() {
            app_log(app, format!("Font OCR raw (inactive, {} lines): {}", lines.len(), lines.join(" | ")));
        }
        return;
    }
    let Some(record) = record else {
        app_log(app, "Font read skipped (its font context ended before the read)".to_string());
        return;
    };
    crate::read_prompt::read_done(app, crate::read_prompt::Mechanic::DivineFont, prompt);
    if let Some(detection) = record.detection {
        crate::publish_lab_detection(app, detection);
    }
    font_context::spawn_font_idle_timer(app, record.idle_seq);
    let Some(outcome) = record.outcome else { return };
    if let Some(sealed) = &outcome.sealed {
        font_context::log_sealed_round(app, sealed);
        crate::emit_status(app);
    }
    if outcome.buffer_grew {
        app_log(app, format!("Font OCR raw ({} lines): {}", lines.len(), lines.join(" | ")));
        app_log(
            app,
            format!(
                "Font options captured: {} options{}{}",
                outcome.buffer.len(),
                if panel.jackpot_detected { " *** JACKPOT! ***" } else { "" },
                outcome.crafts_remaining.map_or(" (last craft)".to_string(), |n| format!(" (remaining: {})", n)),
            ),
        );
        for opt in &outcome.buffer {
            app_log(
                app,
                format!("  - {} {}", opt.option_type, opt.value.map(|v| format!("({})", v)).unwrap_or_default()),
            );
        }
        if panel.jackpot_detected {
            if let Err(e) = app.emit("font-jackpot", true) {
                log::warn!("emit font-jackpot failed: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::tests::test_app_state;

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|l| l.to_string()).collect()
    }

    /// An options panel as Windows OCR returns it: 2 crafts left, three options.
    fn options_panel() -> Vec<String> {
        lines(&[
            "Transform a Skill Gem to be a random",
            "Transfigured Gem of the same colour",
            "Add +20% quality to a Gem",
            "Sacrifice a Gem for Treasure Keys",
            "Crafts Remaining: 2",
        ])
    }

    #[test]
    fn no_context_armed_means_no_token_and_no_grab() {
        let state = test_app_state();

        assert_eq!(armed_token(&state.lab_scan), None);
        assert_eq!(super::super::gem_reader::armed_token(&state.lab_scan), None);
    }

    #[test]
    fn an_armed_context_hands_its_token_to_the_press() {
        let state = test_app_state();
        let arm = font_context::arm_font_context(&state);

        assert_eq!(armed_token(&state.lab_scan), Some(arm.token));
    }

    #[test]
    fn a_press_with_the_options_panel_open_records_that_rounds_options() {
        let state = test_app_state();
        let arm = font_context::arm_font_context(&state);

        let read = read_font_lines(&state, arm.token, &options_panel());

        let record = read.record.expect("a current read of an active panel is recorded");
        assert!(record.idle_seq > arm.idle_seq, "the adapter's timer must outlive the arm's");
        let types: Vec<String> = state
            .lab_scan
            .font_session
            .lock()
            .unwrap()
            .current_options
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|o| o.option_type)
            .collect();
        assert_eq!(types, vec!["transform_random", "quality", "sacrifice_keys"]);
        assert_eq!(state.lab_scan.font_session.lock().unwrap().ledger.current(), Some(2));
    }

    #[test]
    fn a_press_without_the_panel_records_nothing() {
        let state = test_app_state();
        let arm = font_context::arm_font_context(&state);

        let read = read_font_lines(&state, arm.token, &lines(&["Aspirants' Plaza", "Izaro"]));

        assert!(read.record.is_none());
        assert!(state.lab_scan.font_session.lock().unwrap().current_options.is_none());
    }

    #[test]
    fn inactive_font_probe_preserves_the_last_alerts_and_timer() {
        let state = test_app_state();
        let arm = font_context::arm_font_context(&state);
        read_font_lines(&state, arm.token, &lines(&[
            "Transform a non-Transfigured Skill Gem to a Transfigured version",
        ]));
        let alerts = *state.lab_scan.font_alert_seen.lock().unwrap();
        let idle = state.lab_scan.font_idle_seq.load(Ordering::SeqCst);
        let options = state.lab_scan.font_session.lock().unwrap().current_options.clone();
        let read = read_font_lines(&state, arm.token, &lines(&["Inventory"]));
        assert!(!read.panel.font_active);
        assert_eq!(*state.lab_scan.font_alert_seen.lock().unwrap(), alerts);
        assert_eq!(state.lab_scan.font_idle_seq.load(Ordering::SeqCst), idle);
        assert_eq!(state.lab_scan.font_session.lock().unwrap().current_options, options);
    }

    #[test]
    fn the_next_rounds_press_after_a_craft_click_seals_the_first_round() {
        let state = test_app_state();
        let arm = font_context::arm_font_context(&state);
        read_font_lines(&state, arm.token, &options_panel());
        state.lab_scan.font_opened_seq.fetch_add(1, Ordering::SeqCst);

        let read = read_font_lines(
            &state,
            arm.token,
            &lines(&["Transform a Skill Gem to be a random", "Transfigured Gem of the same colour"]),
        );

        let sealed = read.record.unwrap().outcome.unwrap().sealed.expect("one press of the new panel seals");
        assert_eq!(sealed.round.crafts_remaining, Some(2));
        assert_eq!(sealed.round.options.len(), 3);
    }
}
