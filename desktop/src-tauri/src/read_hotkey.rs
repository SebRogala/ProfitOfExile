//! The app-wide "Read screen" shortcut (POE-387): which key a press of grabs
//! one frame, and the fixed delay between that press and the grab.
use serde::{Deserialize, Serialize};
use tauri::Manager;

/// Longest press-to-grab delay a setting may hold, in milliseconds.
pub const MAX_DELAY_MS: u32 = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub key: u16,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}
impl Shortcut {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !(1..=254).contains(&self.key) || matches!(self.key, 16..=18 | 27 | 160..=165) {
            return Err("Choose a key or mouse button other than a modifier or Esc".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReadHotkeySettings {
    pub shortcut: Shortcut,
    pub delay_ms: u32,
}
impl Default for ReadHotkeySettings {
    fn default() -> Self {
        Self {
            shortcut: Shortcut {
                key: 2,
                shift: true,
                ctrl: false,
                alt: false,
            },
            delay_ms: 0,
        }
    }
}
impl ReadHotkeySettings {
    pub fn validate(&self) -> Result<(), String> {
        self.shortcut.validate()?;
        if self.delay_ms > MAX_DELAY_MS {
            return Err(format!("Delay must be at most {MAX_DELAY_MS} ms"));
        }
        Ok(())
    }
}

#[tauri::command]
pub fn read_hotkey_set_settings(
    app: tauri::AppHandle,
    settings: ReadHotkeySettings,
) -> Result<(), String> {
    settings.validate()?;
    *app.state::<crate::AppState>()
        .read_hotkey
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = settings;
    crate::persist_settings(&app);
    crate::ssot::emit_ssot(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: u16) -> ReadHotkeySettings {
        ReadHotkeySettings {
            shortcut: Shortcut { key, shift: false, ctrl: true, alt: false },
            delay_ms: 0,
        }
    }

    #[test]
    fn read_hotkey_accepts_the_longest_delay() {
        let settings = ReadHotkeySettings { delay_ms: MAX_DELAY_MS, ..key(65) };
        assert_eq!(settings.validate(), Ok(()));
    }
    #[test]
    fn read_hotkey_rejects_a_delay_past_the_longest() {
        let settings = ReadHotkeySettings { delay_ms: MAX_DELAY_MS + 1, ..key(65) };
        assert!(settings.validate().unwrap_err().contains("Delay"));
    }
    #[test]
    fn read_hotkey_rejects_a_modifier_or_escape_as_the_key() {
        for bad in [0, 16, 17, 18, 27, 160, 165, 255] {
            assert!(key(bad).validate().is_err(), "key {bad} must be rejected");
        }
    }
    #[test]
    fn read_hotkey_accepts_a_mouse_button_as_the_key() {
        assert_eq!(key(4).validate(), Ok(()));
    }
}
