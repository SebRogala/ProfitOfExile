use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use crate::read_hotkey::Shortcut;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Inheritance<T> {
    #[default]
    Inherit,
    Override(T),
}

impl<T> Inheritance<T> {
    pub fn resolve<'a>(&'a self, global: &'a T) -> &'a T {
        match self {
            Self::Inherit => global,
            Self::Override(value) => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OverlayDismiss {
    pub timeout: Inheritance<u32>,
    pub shortcut: Inheritance<Shortcut>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DismissSettings {
    pub timeout_seconds: u32,
    pub shortcut: Shortcut,
    pub overlays: BTreeMap<String, OverlayDismiss>,
}

impl Default for DismissSettings {
    fn default() -> Self {
        Self {
            timeout_seconds: crate::loot_capture::LootSettings::default().auto_save_seconds,
            shortcut: Shortcut { key: 27, shift: false, ctrl: false, alt: false },
            overlays: BTreeMap::new(),
        }
    }
}

fn validate_timeout(seconds: u32) -> Result<(), String> {
    crate::loot_capture::LootSettings {
        auto_save_seconds: seconds,
        ..Default::default()
    }.validate()
}

fn validate_shortcut(shortcut: &Shortcut) -> Result<(), String> {
    if !(1..=254).contains(&shortcut.key) || matches!(shortcut.key, 16..=18 | 160..=165) {
        return Err("Choose a key or mouse button other than a modifier".into());
    }
    Ok(())
}

impl DismissSettings {
    pub fn validate(&self) -> Result<(), String> {
        validate_timeout(self.timeout_seconds)?;
        validate_shortcut(&self.shortcut)?;
        for overlay in self.overlays.values() {
            if let Inheritance::Override(seconds) = overlay.timeout {
                validate_timeout(seconds)?;
            }
            if let Inheritance::Override(shortcut) = &overlay.shortcut {
                validate_shortcut(shortcut)?;
            }
        }
        Ok(())
    }
}

#[tauri::command]
pub fn overlay_dismiss_set_settings(app: tauri::AppHandle, settings: DismissSettings) -> Result<(), String> {
    store_settings(&app.state::<crate::AppState>(), settings)?;
    crate::persist_settings(&app);
    crate::ssot::emit_ssot(&app);
    Ok(())
}

pub(crate) fn store_settings(state: &crate::AppState, settings: DismissSettings) -> Result<(), String> {
    settings.validate()?;
    *state.overlay_dismiss.lock().unwrap_or_else(|e| e.into_inner()) = settings;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_ten_seconds_and_escape() {
        let settings = DismissSettings::default();
        assert_eq!(settings.timeout_seconds, 10);
        assert_eq!(settings.shortcut, Shortcut { key: 27, shift: false, ctrl: false, alt: false });
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn escape_is_valid_but_modifier_and_out_of_range_keys_are_refused() {
        for key in [0, 16, 17, 18, 160, 161, 162, 163, 164, 165, 255, u16::MAX] {
            let mut settings = DismissSettings::default();
            settings.shortcut.key = key;
            assert!(settings.validate().is_err(), "accepted key {key}");
        }
        for key in [1, 27, 82, 254] {
            let mut settings = DismissSettings::default();
            settings.shortcut.key = key;
            assert!(settings.validate().is_ok(), "refused key {key}");
        }
    }

    #[test]
    fn only_the_boss_loot_timeout_options_are_accepted_globally_and_per_overlay() {
        for seconds in [0, 5, 10, 20, 30, 1, 7, 31, u32::MAX] {
            let mut settings = DismissSettings::default();
            settings.timeout_seconds = seconds;
            assert_eq!(settings.validate().is_ok(), [0, 5, 10, 20, 30].contains(&seconds));
            settings.timeout_seconds = 10;
            settings.overlays.insert("lab.fontalerts".into(), OverlayDismiss {
                timeout: Inheritance::Override(seconds), ..Default::default()
            });
            assert_eq!(settings.validate().is_ok(), [0, 5, 10, 20, 30].contains(&seconds));
        }
    }

    #[test]
    fn inheriting_overlays_follow_global_changes_and_overrides_stay_fixed() {
        let mut settings = DismissSettings::default();
        let inheriting = OverlayDismiss::default();
        let overridden = OverlayDismiss {
            timeout: Inheritance::Override(0),
            shortcut: Inheritance::Override(Shortcut { key: 82, shift: false, ctrl: true, alt: false }),
        };
        assert_eq!(inheriting.timeout.resolve(&settings.timeout_seconds), &10);
        settings.timeout_seconds = 30;
        settings.shortcut.key = 65;
        assert_eq!(inheriting.timeout.resolve(&settings.timeout_seconds), &30);
        assert_eq!(inheriting.shortcut.resolve(&settings.shortcut).key, 65);
        assert_eq!(overridden.timeout.resolve(&settings.timeout_seconds), &0);
        assert_eq!(overridden.shortcut.resolve(&settings.shortcut).key, 82);
    }

    #[test]
    fn invalid_override_is_rejected_before_changing_the_owner() {
        let state = crate::settings::tests::test_app_state();
        let mut settings = DismissSettings::default();
        settings.timeout_seconds = 20;
        store_settings(&state, settings.clone()).unwrap();
        let mut invalid = settings.clone();
        invalid.overlays.insert("mercenary.verdict".into(), OverlayDismiss {
            shortcut: Inheritance::Override(Shortcut { key: 16, shift: false, ctrl: false, alt: false }),
            ..Default::default()
        });
        assert!(store_settings(&state, invalid).is_err());
        assert_eq!(*state.overlay_dismiss.lock().unwrap(), settings);
    }
}
