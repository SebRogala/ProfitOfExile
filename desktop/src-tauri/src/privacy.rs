//! The Data & Privacy notice acceptance (POE-395, ADR-033): which revision is
//! current, whether it is accepted, and the commands the notice modal calls.
//! The gate itself is [`crate::http::NoticeGate`]; the server client refuses
//! every connection while it is closed.

use crate::http::NoticeGate;
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager};

const NOTICE_JSON: &str = include_str!("../../src/lib/privacy/notice.json");

#[derive(serde::Deserialize)]
struct NoticeRevision {
    revision: u32,
}

fn parse_revision(json: &str) -> Result<u32, serde_json::Error> {
    serde_json::from_str::<NoticeRevision>(json).map(|n| n.revision)
}

/// The revision of the notice bundled with this build.
pub(crate) fn current_revision() -> u32 {
    parse_revision(NOTICE_JSON).expect("the bundled notice.json has a numeric revision")
}

/// Whether a stored acceptance covers the `current` revision.
pub(crate) fn accepted(stored: Option<u32>, current: u32) -> bool {
    stored.is_some_and(|stored| stored >= current)
}

/// Whether the notice is accepted this session.
pub(crate) fn is_accepted(state: &crate::AppState) -> bool {
    state.privacy_gate.is_open()
}

#[derive(Debug, PartialEq)]
pub(crate) struct AcceptOutcome {
    pub persisted: bool,
}

/// Accept `revision` for this session and record it in the settings file at
/// `path`. Only the current revision is accepted. The gate opens even when
/// the write fails; only what reached the file counts at the next start.
pub(crate) fn accept_at(path: Option<&Path>, gate: &NoticeGate, revision: u32) -> Result<AcceptOutcome, String> {
    let current = current_revision();
    if revision != current {
        return Err(format!("Notice revision {revision} is not the current revision {current}"));
    }
    let persisted = match path {
        None => {
            log::error!("No settings path: the notice acceptance holds for this session only");
            false
        }
        Some(path) => match crate::settings::update_at(path, |loaded| loaded.unwrap_or_default(), |s| {
            crate::settings::Settings { privacy_accepted_revision: Some(revision), ..s.clone() }
        }) {
            Ok(()) => true,
            Err(e) => {
                log::error!("Notice acceptance not saved, it holds for this session only: {e}");
                false
            }
        },
    };
    gate.open();
    Ok(AcceptOutcome { persisted })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyState {
    current_revision: u32,
    accepted_revision: Option<u32>,
    accepted: bool,
    persisted: bool,
}

#[tauri::command]
pub fn get_privacy_state(app: AppHandle) -> PrivacyState {
    let stored = crate::settings::privacy_accepted_revision_at(crate::settings::settings_path_pub(&app).as_deref());
    let current = current_revision();
    PrivacyState {
        current_revision: current,
        accepted_revision: stored,
        accepted: is_accepted(&app.state::<crate::AppState>()),
        persisted: accepted(stored, current),
    }
}

#[derive(serde::Serialize)]
pub struct AcceptResult {
    persisted: bool,
}

#[tauri::command]
pub fn accept_privacy_revision(app: AppHandle, revision: u32) -> Result<AcceptResult, String> {
    let path = crate::settings::settings_path_pub(&app);
    let outcome = accept_at(path.as_deref(), &app.state::<crate::AppState>().privacy_gate, revision)?;
    if !outcome.persisted {
        crate::app_log(&app, "Data & privacy acceptance could not be saved; the notice shows again next start".to_string());
    }
    if let Err(e) = app.emit("privacy-accepted", ()) {
        log::warn!("emit privacy-accepted failed: {e}");
    }
    // Wake what failed closed at the gate instead of waiting for its backoff.
    crate::ssot::rearm_league(app.clone());
    crate::font_alerts::on_server_url_changed(&app);
    crate::mercenary::sync::spawn_repull(&app);
    Ok(AcceptResult { persisted: outcome.persisted })
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// A settings file holding `contents` in a fresh per-test directory.
    fn settings_file(name: &str, contents: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("poe_test_privacy_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, contents).unwrap();
        (dir, path)
    }

    #[test]
    fn parse_revision_reads_the_revision_from_a_notice() {
        let notice = r#"{"date": "2026-01-01", "revision": 7, "changes": [{"revision": 3}]}"#;
        assert_eq!(parse_revision(notice).unwrap(), 7);
    }

    #[test]
    fn current_revision_is_the_bundled_notice_revision() {
        let on_disk = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/privacy/notice.json")).unwrap();
        assert_eq!(current_revision(), parse_revision(&on_disk).unwrap());
    }

    #[test]
    fn accepted_only_when_stored_is_at_least_current() {
        assert!(!accepted(None, 5));
        assert!(!accepted(Some(4), 5));
        assert!(accepted(Some(5), 5));
        assert!(accepted(Some(6), 5));
    }

    #[test]
    fn accept_rejects_a_revision_other_than_current() {
        let gate = NoticeGate::default();
        let result = accept_at(None, &gate, current_revision() + 1);
        assert!(result.is_err(), "{result:?}");
        assert!(!gate.is_open());
    }

    #[test]
    fn accept_at_with_a_writable_settings_file_persists_the_revision() {
        let (dir, path) = settings_file("writable", r#"{"sidebar_open": false}"#);
        let gate = NoticeGate::default();
        let outcome = accept_at(Some(&path), &gate, current_revision());
        let read_back = crate::settings::privacy_accepted_revision_at(Some(&path));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(outcome, Ok(AcceptOutcome { persisted: true }));
        assert!(gate.is_open());
        assert_eq!(read_back, Some(current_revision()));
    }

    #[test]
    fn accept_at_with_a_settings_file_that_is_not_json_opens_the_gate_but_reports_not_persisted() {
        let (dir, path) = settings_file("not_json", r#"{"sidebar_open": fal"#);
        let gate = NoticeGate::default();
        let outcome = accept_at(Some(&path), &gate, current_revision());
        let read_back = crate::settings::privacy_accepted_revision_at(Some(&path));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(outcome, Ok(AcceptOutcome { persisted: false }));
        assert!(gate.is_open());
        assert_eq!(read_back, None);
    }

    #[test]
    fn accept_at_with_no_settings_path_opens_the_gate_but_reports_not_persisted() {
        let gate = NoticeGate::default();
        assert_eq!(accept_at(None, &gate, current_revision()), Ok(AcceptOutcome { persisted: false }));
        assert!(gate.is_open());
    }
}
