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
                key: 120,
                shift: true,
                ctrl: false,
                alt: false,
            },
            delay_ms: 0,
        }
    }
}

/// The read key reads the whole screen and the boss-loot keys read near the
/// mouse, so one key may not do both.
pub fn apart_from_boss_loot(
    read: &Shortcut,
    loot: &crate::loot_capture::LootSettings,
) -> Result<(), String> {
    if *read == loot.capture_shortcut || *read == loot.preview_shortcut {
        return Err("The read screen shortcut must differ from the boss loot shortcuts".into());
    }
    Ok(())
}

/// What stands in for a refused read hotkey: the default, or the first of
/// Shift+F10 / Shift+F11 when the boss-loot keys already hold it. Two boss-loot
/// keys can block at most two of the three.
pub fn fallback_apart_from(loot: &crate::loot_capture::LootSettings) -> ReadHotkeySettings {
    let default = ReadHotkeySettings::default();
    [120, 121, 122]
        .into_iter()
        .map(|key| ReadHotkeySettings {
            shortcut: Shortcut { key, ..default.shortcut.clone() },
            ..default.clone()
        })
        .find(|candidate| apart_from_boss_loot(&candidate.shortcut, loot).is_ok())
        .expect("two boss-loot keys cannot block three candidates")
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
    store_settings(&app.state::<crate::AppState>(), settings)?;
    crate::persist_settings(&app);
    crate::ssot::emit_ssot(&app);
    Ok(())
}
/// The owner write behind `read_hotkey_set_settings`; refuses before touching state.
pub(crate) fn store_settings(state: &crate::AppState, settings: ReadHotkeySettings) -> Result<(), String> {
    settings.validate()?;
    let loot = state.loot.settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
    apart_from_boss_loot(&settings.shortcut, &loot)?;
    *state.read_hotkey.lock().unwrap_or_else(|e| e.into_inner()) = settings;
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
    fn read_hotkey_default_is_apart_from_the_default_boss_loot_keys() {
        let loot = crate::loot_capture::LootSettings::default();
        assert_eq!(apart_from_boss_loot(&ReadHotkeySettings::default().shortcut, &loot), Ok(()));
    }
    use crate::loot_capture::LootSettings;
    use crate::settings::tests::test_app_state;

    fn read_of(state: &crate::AppState) -> ReadHotkeySettings {
        state.read_hotkey.lock().unwrap().clone()
    }
    fn loot_of(state: &crate::AppState) -> LootSettings {
        state.loot.settings.lock().unwrap().clone()
    }
    #[test]
    fn storing_a_read_hotkey_on_the_boss_loot_capture_key_is_refused_and_keeps_the_old_one() {
        let state = test_app_state();
        let capture = loot_of(&state).capture_shortcut;
        let refused = ReadHotkeySettings { shortcut: capture, delay_ms: 100 };
        assert!(store_settings(&state, refused).is_err());
        assert_eq!(read_of(&state), ReadHotkeySettings::default());
    }
    #[test]
    fn storing_a_read_hotkey_on_the_boss_loot_preview_key_is_refused_and_keeps_the_old_one() {
        let state = test_app_state();
        let preview = loot_of(&state).preview_shortcut;
        let refused = ReadHotkeySettings { shortcut: preview, delay_ms: 100 };
        assert!(store_settings(&state, refused).is_err());
        assert_eq!(read_of(&state), ReadHotkeySettings::default());
    }
    #[test]
    fn storing_a_read_hotkey_apart_from_the_boss_loot_keys_is_kept() {
        let state = test_app_state();
        assert_eq!(store_settings(&state, key(65)), Ok(()));
        assert_eq!(read_of(&state), key(65));
    }
    #[test]
    fn storing_boss_loot_capture_on_the_read_key_is_refused_and_keeps_the_old_settings() {
        let state = test_app_state();
        let refused = LootSettings {
            capture_shortcut: read_of(&state).shortcut,
            auto_save_seconds: 30,
            ..Default::default()
        };
        assert!(crate::loot_capture::store_settings(&state, refused).is_err());
        assert_eq!(loot_of(&state), LootSettings::default());
    }
    #[test]
    fn storing_boss_loot_preview_on_the_read_key_is_refused_and_keeps_the_old_settings() {
        let state = test_app_state();
        let refused = LootSettings {
            preview_shortcut: read_of(&state).shortcut,
            auto_save_seconds: 30,
            ..Default::default()
        };
        assert!(crate::loot_capture::store_settings(&state, refused).is_err());
        assert_eq!(loot_of(&state), LootSettings::default());
    }
    #[test]
    fn storing_boss_loot_keys_apart_from_the_read_key_is_kept() {
        let state = test_app_state();
        let kept = LootSettings { auto_save_seconds: 30, ..Default::default() };
        assert_eq!(crate::loot_capture::store_settings(&state, kept.clone()), Ok(()));
        assert_eq!(loot_of(&state), kept);
    }
    #[test]
    fn fallback_skips_a_default_held_by_boss_loot_capture() {
        let loot = LootSettings { capture_shortcut: ReadHotkeySettings::default().shortcut, ..Default::default() };
        let fallback = fallback_apart_from(&loot);
        assert_eq!(fallback.shortcut.key, 121);
        assert_eq!(apart_from_boss_loot(&fallback.shortcut, &loot), Ok(()));
    }
    #[test]
    fn fallback_skips_a_default_held_by_boss_loot_preview() {
        let loot = LootSettings { preview_shortcut: ReadHotkeySettings::default().shortcut, ..Default::default() };
        assert_eq!(fallback_apart_from(&loot).shortcut.key, 121);
    }
    #[test]
    fn fallback_skips_two_candidates_held_by_both_boss_loot_keys() {
        let default = ReadHotkeySettings::default().shortcut;
        let loot = LootSettings {
            capture_shortcut: default.clone(),
            preview_shortcut: Shortcut { key: 121, ..default },
            ..Default::default()
        };
        assert_eq!(fallback_apart_from(&loot).shortcut.key, 122);
    }
    #[test]
    fn fallback_is_the_default_when_boss_loot_leaves_it_free() {
        assert_eq!(fallback_apart_from(&LootSettings::default()), ReadHotkeySettings::default());
    }
    #[test]
    fn read_hotkey_with_other_modifiers_than_a_boss_loot_key_is_accepted() {
        let loot = crate::loot_capture::LootSettings::default();
        let read = Shortcut { ctrl: true, ..loot.preview_shortcut.clone() };
        assert_eq!(apart_from_boss_loot(&read, &loot), Ok(()));
    }
    #[test]
    fn read_hotkey_accepts_a_mouse_button_as_the_key() {
        assert_eq!(key(4).validate(), Ok(()));
    }
}
