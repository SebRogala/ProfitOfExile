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
            recognition: None,
        }) as _)
    }
}

struct PendingFontRead {
    token: u64,
    prompt: Option<u64>,
    recognition: Option<FontRecognition>,
}

struct FontRecognition {
    lines: Vec<String>,
    panel: FontPanelState,
}

fn probe_font_frame(frame: &Capture, region: [i32; 4],
    recognize: impl FnOnce(&image::DynamicImage) -> Result<Vec<String>, String>) -> Result<Option<FontRecognition>, String> {
    let [x,y,w,h] = region;
    if x < 0 || y < 0 || w <= 0 || h <= 0 || x as u32 + w as u32 > frame.image.width() || y as u32 + h as u32 > frame.image.height() { return Ok(None); }
    let crop = frame.image.crop_imm(x as u32, y as u32, w as u32, h as u32);
    let lines = recognize(&crate::capture::preprocess_for_ocr(&crop))?;
    let panel = crate::font_parser::parse_font_panel(&lines);
    Ok((panel.font_active && !panel.options.is_empty()).then_some(FontRecognition { lines, panel }))
}

impl PendingFontRead {
    fn probe_in(&mut self, frame: &Capture, state: &AppState,
        recognize: impl FnOnce(&image::DynamicImage) -> Result<Vec<String>, String>) -> Result<bool, String> {
        if armed_token(&state.lab_scan) != Some(self.token)
            || !*state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) { return Ok(false); }
        let screen = *state.screen.lock().unwrap_or_else(|e| e.into_inner());
        self.recognition = probe_font_frame(frame, crate::ssot::placements_for(screen.as_ref()).lab.font, recognize)?;
        Ok(self.recognition.is_some())
    }
}

impl PendingRead<AppHandle> for PendingFontRead {
    fn probe(&mut self, frame: &Arc<Capture>, app: &AppHandle) -> Result<bool, String> {
        self.probe_in(frame, &app.state::<AppState>(), crate::ocr::recognize_text)
    }
    fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, app: &AppHandle) {
        if frame.is_err() { return; }
        report_engine_once(app);
        let state = app.state::<AppState>();
        if !*state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner()) { return; }
        let Some(recognition) = self.recognition else { return; };
        let read = read_font_panel(&state, self.token, recognition.panel);
        publish_font_read(app, self.token, &recognition.lines, read, self.prompt);
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
#[cfg(test)]
pub(crate) fn read_font_lines(state: &AppState, captured: u64, lines: &[String]) -> FontLinesRead {
    read_font_panel(state, captured, crate::font_parser::parse_font_panel(lines))
}

fn read_font_panel(state: &AppState, captured: u64, panel: FontPanelState) -> FontLinesRead {
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

fn publish_font_read(app: &AppHandle, captured: u64, lines: &[String], read: FontLinesRead, prompt: Option<u64>) {
    let state = app.state::<AppState>();
    let current = {
        let _session = state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner());
        captured != 0 && state.lab_scan.font_context.load(Ordering::SeqCst) == captured
            && *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner())
    };
    if !current { return; }
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

    fn probe_capture() -> Capture {
        Capture { image: image::DynamicImage::new_rgb8(4,4), monitor_id: 7, origin:(0,0), client:[0,0,4,4], timing:Default::default() }
    }

    #[test]
    fn selected_options_record_cached_recognition_with_one_ocr() {
        let state = test_app_state();
        let token = font_context::arm_font_context(&state).token;
        let mut ocrs = 0;
        let recognition = probe_font_frame(&probe_capture(), [0,0,4,4], |_| { ocrs += 1; Ok(options_panel()) }).unwrap().unwrap();
        assert!(state.lab_scan.font_session.lock().unwrap().current_options.is_none(), "recognition has no recording effects");
        let read = read_font_panel(&state, token, recognition.panel);
        let outcome = read.record.unwrap().outcome.unwrap();
        assert_eq!(outcome.buffer.iter().map(|o| o.option_type.as_str()).collect::<Vec<_>>(),
            vec!["transform_random", "quality", "sacrifice_keys"]);
        assert_eq!(ocrs, 1);
    }

    #[test]
    fn inactive_recognition_cannot_clear_a_previous_alert() {
        let state = test_app_state();
        let token = font_context::arm_font_context(&state).token;
        read_font_lines(&state, token, &lines(&["Transform a non-Transfigured Skill Gem to a Transfigured version"]));
        let before = *state.lab_scan.font_alert_seen.lock().unwrap();
        let recognition = probe_font_frame(&probe_capture(), [0,0,4,4], |_| Ok(lines(&["Inventory"]))).unwrap();
        assert!(recognition.is_none());
        assert_eq!(*state.lab_scan.font_alert_seen.lock().unwrap(), before);
    }

    #[test]
    fn remaining_count_without_options_does_not_select_font() {
        let recognition = probe_font_frame(&probe_capture(), [0,0,4,4], |_| Ok(lines(&["Crafts Remaining: 2"]))).unwrap();
        assert!(recognition.is_none());
    }

    #[test]
    fn expired_recognition_leaves_the_replacement_context_and_prompt_untouched() {
        let state = test_app_state();
        let old = font_context::arm_font_context(&state).token;
        let recognition = probe_font_frame(&probe_capture(), [0,0,4,4], |_| Ok(options_panel())).unwrap().unwrap();
        font_context::disarm_font_context(&state);
        let current = font_context::arm_font_context(&state);
        crate::read_prompt::apply_in(&state, crate::read_prompt::Change::Arm(crate::read_prompt::Mechanic::DivineFont));
        let prompt = state.read_prompt.lock().unwrap().snapshot();
        assert!(read_font_panel(&state, old, recognition.panel).record.is_none());
        assert_eq!(state.read_prompt.lock().unwrap().snapshot(), prompt);
        assert_eq!(state.lab_scan.font_idle_seq.load(Ordering::SeqCst), current.idle_seq);
        assert!(state.lab_scan.font_session.lock().unwrap().current_options.is_none());
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

    mod manual_regions {
        use super::*;
        use crate::ssot::{screen_from_geometry, ManualRegions, OcrRegionsMode, OcrRegionsSetting, ScreenRef};

        const SET_ON_3440: ScreenRef = ScreenRef { width: 3440, height: 1440, origin: [0, 0], at_ms: 1_732_400_000_000 };

        /// A Manual set on 3440x1440 holding only `lab_font`.
        fn manual_font(lab_font: Option<[i32; 4]>) -> OcrRegionsSetting {
            OcrRegionsSetting {
                mode: OcrRegionsMode::Manual,
                manual: Some(ManualRegions {
                    set_on: SET_ON_3440,
                    edited: true,
                    lab_gem: None,
                    lab_font,
                    lab_gem_slots: None,
                    temple_panel: None,
                    merc_panel: None,
                }),
            }
        }

        /// A test `AppState` on a measured 3440x1440 slice holding `setting`,
        /// with a font context armed; returns the state and the press token.
        fn armed_state_3440(setting: OcrRegionsSetting) -> (AppState, u64) {
            let state = test_app_state();
            *state.screen.lock().unwrap() = Some(screen_from_geometry(3440, 1440, 1, (0, 0), [0, 0, 3440, 1440], 0));
            *state.ocr_regions.lock().unwrap() = setting;
            let token = font_context::arm_font_context(&state).token;
            (state, token)
        }

        fn pending(token: u64) -> PendingFontRead {
            PendingFontRead { token, prompt: None, recognition: None }
        }

        /// A black 3440x1440 capture with the right half of `rect` white, so a
        /// crop of any other rect of the same size differs from a crop of `rect`.
        fn capture_marking(rect: [i32; 4]) -> Capture {
            let mut image = image::RgbImage::new(3440, 1440);
            let [x, y, w, h] = rect.map(|v| v as u32);
            for py in y..y + h {
                for px in x + w / 2..x + w {
                    image.put_pixel(px, py, image::Rgb([255, 255, 255]));
                }
            }
            Capture { image: image::DynamicImage::ImageRgb8(image), monitor_id: 1, origin: (0, 0), client: [0, 0, 3440, 1440], timing: Default::default() }
        }

        mod keep_happy {
            use super::*;

            // keep:happy — AC-1: the controller layout's centre-right Font
            // panel is read from the placed Manual rect, not Auto's
            // [837, 293, 888, 908] on the same slice.
            #[test]
            fn a_press_on_a_3440x1440_capture_ocrs_the_manual_font_rect() {
                let font = [1820, 293, 888, 908];
                let (state, token) = armed_state_3440(manual_font(Some(font)));
                let frame = capture_marking(font);
                let mut seen: Option<image::DynamicImage> = None;

                let read = pending(token).probe_in(&frame, &state, |crop| { seen = Some(crop.clone()); Ok(options_panel()) });

                let expected = crate::capture::preprocess_for_ocr(&frame.image.crop_imm(1820, 293, 888, 908));
                assert_eq!(read, Ok(true), "the options panel read from the Manual Font is a recognition");
                assert!(
                    seen.as_ref() == Some(&expected),
                    "OCR must receive the preprocessed [1820, 293, 888, 908] crop; got {:?}",
                    seen.as_ref().map(|crop| (crop.width(), crop.height())),
                );
            }
        }

        mod keep_critical {
            use super::*;

            /// Silent consequence: a Manual Font that is unset or wholly past
            /// the capture keeps Auto's display-only rect in the placements;
            /// OCR of that rect reads whatever UI lies there on every press and
            /// records it as the Font's options with nothing to say so (AC-5:
            /// unset or unreadable regions are not read).
            #[test]
            fn an_unread_manual_font_runs_no_ocr_at_the_auto_fallback_rect() {
                let cases = [
                    ("Font unset", None),
                    ("Font wholly past x = 3440", Some([3500, 293, 888, 908])),
                ];

                for (case, lab_font) in cases {
                    let (state, token) = armed_state_3440(manual_font(lab_font));
                    let frame = capture_marking([837, 293, 888, 908]);
                    let mut ocrs = 0;

                    let read = pending(token).probe_in(&frame, &state, |_| { ocrs += 1; Ok(options_panel()) });

                    assert_eq!((read, ocrs), (Ok(false), 0), "{case}: an unread Font takes no OCR and selects no frame");
                }
            }
        }
    }
}
