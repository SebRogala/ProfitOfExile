//! Read exchange (POE-477): one press reads the Faustus Currency Exchange's
//! top Competing Trades row into the Currency Exchange Companion.
pub mod parse;
pub mod reader;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::read_hotkey::{DispatchOutcome, Shortcut};
use parse::ExchangeSelection;
use reader::ExchangeReadResult;

/// The "Read exchange" binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExchangeReadSettings {
    pub shortcut: Shortcut,
}

impl Default for ExchangeReadSettings {
    fn default() -> Self {
        Self { shortcut: Shortcut { key: 2, shift: false, ctrl: false, alt: true } }
    }
}

impl ExchangeReadSettings {
    pub fn validate(&self) -> Result<(), String> {
        self.shortcut.validate()?;
        if matches!(self.shortcut.key, 1 | 2)
            && !self.shortcut.shift && !self.shortcut.ctrl && !self.shortcut.alt
        {
            return Err("Plain left or right click cannot be the read exchange shortcut".into());
        }
        Ok(())
    }

    pub fn apart_from(&self, read: &Shortcut, loot: &crate::loot_capture::LootSettings) -> Result<(), String> {
        if self.shortcut == *read
            || self.shortcut == loot.capture_shortcut
            || self.shortcut == loot.preview_shortcut
        {
            return Err("Read exchange shortcut must differ from the read screen and boss loot shortcuts".into());
        }
        Ok(())
    }
}

#[tauri::command]
pub fn exchange_read_set_settings(app: tauri::AppHandle, settings: ExchangeReadSettings) -> Result<(), String> {
    store_settings(&app.state::<crate::AppState>(), settings)?;
    crate::persist_settings(&app);
    crate::ssot::emit_ssot(&app);
    Ok(())
}

/// The owner write behind the Settings row; refuses before touching state.
pub(crate) fn store_settings(state: &crate::AppState, settings: ExchangeReadSettings) -> Result<(), String> {
    settings.validate()?;
    let read = state.read_hotkey.lock().unwrap_or_else(|e| e.into_inner()).shortcut.clone();
    let loot = state.loot.settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
    settings.apart_from(&read, &loot)?;
    *state.exchange_read.lock().unwrap_or_else(|e| e.into_inner()) = settings;
    Ok(())
}

/// What stands in for a refused or clashing exchange binding at load.
pub fn fallback_apart_from(
    read: &Shortcut,
    loot: &crate::loot_capture::LootSettings,
) -> ExchangeReadSettings {
    let default = ExchangeReadSettings::default();
    [2, 4, 5, 6].into_iter()
        .map(|key| ExchangeReadSettings { shortcut: Shortcut { key, ..default.shortcut.clone() } })
        .find(|candidate| candidate.apart_from(read, loot).is_ok())
        .unwrap_or_default()
}

/// Every code an exchange press can fail with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExchangeReadFailure {
    PanelNotFound,
    RatioUnreadable,
    NamesUnreadable,
    NoCurrencySide,
    GrabFailed,
    PrivacyRefused,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExchangeReadOutcome {
    Selection(ExchangeSelection),
    Error(ExchangeReadFailure),
}

/// The `exchange-read` payload: the press time and what it read.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeReadEvent {
    pub pressed_at_ms: u64,
    #[serde(flatten)]
    pub outcome: ExchangeReadOutcome,
}

/// Turns one exchange press's dispatch outcome and its reader's result into
/// its one `exchange-read` event.
pub fn finish(
    outcome: DispatchOutcome,
    read: Option<ExchangeReadResult>,
    pressed_at_ms: u64,
    emit: &mut dyn FnMut(ExchangeReadEvent),
) {
    use parse::ExchangeReadError;
    use ExchangeReadFailure::*;
    let outcome = match outcome {
        DispatchOutcome::Selected(crate::read_hotkey::ReaderId::Exchange) => match read {
            Some(Ok(selection)) => ExchangeReadOutcome::Selection(selection),
            result => ExchangeReadOutcome::Error(match result {
                Some(Err(ExchangeReadError::NamesUnreadable)) => NamesUnreadable,
                Some(Err(ExchangeReadError::NoCurrencySide)) => NoCurrencySide,
                _ => RatioUnreadable,
            }),
        },
        DispatchOutcome::NoMatch => ExchangeReadOutcome::Error(PanelNotFound),
        DispatchOutcome::Failed(_) => ExchangeReadOutcome::Error(GrabFailed),
        DispatchOutcome::Suppressed => ExchangeReadOutcome::Error(PrivacyRefused),
        DispatchOutcome::Selected(_) => ExchangeReadOutcome::Error(RatioUnreadable),
    };
    emit(ExchangeReadEvent { pressed_at_ms, outcome });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loot_capture::LootSettings;
    use crate::read_hotkey::{ReadHotkeySettings, ReaderId};
    use crate::settings::tests::test_app_state;

    fn shortcut(key: u16, shift: bool, ctrl: bool, alt: bool) -> Shortcut {
        Shortcut { key, shift, ctrl, alt }
    }
    fn alt(key: u16) -> Shortcut {
        shortcut(key, false, false, true)
    }
    fn exchange(shortcut: Shortcut) -> ExchangeReadSettings {
        ExchangeReadSettings { shortcut }
    }
    fn exchange_of(state: &crate::AppState) -> ExchangeReadSettings {
        state.exchange_read.lock().unwrap().clone()
    }

    fn ember_for_chaos() -> ExchangeSelection {
        use crate::exchange_read::parse::{Currency, Mode, OneSide};
        ExchangeSelection {
            mode: Mode::Sell,
            currency: Currency::Chaos,
            ratio: 130.0,
            one_side: OneSide::Right,
            left: 130.0,
            right: 1.0,
            stock: 1,
            want_name: "Chaos Orb".into(),
            have_name: "Allflame Ember of the Ethereal".into(),
        }
    }

    /// Every event `finish` emits for one outcome, as the webview receives it.
    fn emitted(outcome: DispatchOutcome, read: Option<ExchangeReadResult>) -> Vec<serde_json::Value> {
        let mut events = Vec::new();
        finish(outcome, read, 1_700_000_000_123, &mut |event| events.push(serde_json::to_value(event).unwrap()));
        events
    }

    mod keep_happy {
        use super::*;

        // keep:happy — AC-3: Settings saves a different Read exchange shortcut.
        #[test]
        fn a_read_exchange_shortcut_apart_from_every_other_binding_is_stored() {
            let accepted = [
                alt(4),
                alt(6),
                shortcut(2, false, true, false),
                shortcut(1, false, false, true),
                shortcut(70, true, false, false),
            ];
            for candidate in accepted {
                let state = test_app_state();
                assert_eq!(store_settings(&state, exchange(candidate.clone())), Ok(()), "{candidate:?}");
                assert_eq!(exchange_of(&state), exchange(candidate.clone()), "{candidate:?}");
            }
        }
    }

    mod keep_critical {
        use super::*;

        const CLASH: &str = "Read exchange shortcut must differ from the read screen and boss loot shortcuts";
        const PLAIN_CLICK: &str = "Plain left or right click cannot be the read exchange shortcut";

        // keep:critical — silent: an exchange binding on the read key or a
        // boss-loot key makes one press start two reads, so the exchange read
        // grabs a frame the player pressed for something else; a plain left or
        // right click would capture on every ordinary game click.
        #[test]
        fn a_clashing_or_plain_click_read_exchange_shortcut_is_refused_and_the_stored_one_kept() {
            let loot = LootSettings::default();
            let refused = [
                (ReadHotkeySettings::default().shortcut, CLASH),
                (loot.capture_shortcut.clone(), CLASH),
                (loot.preview_shortcut.clone(), CLASH),
                (shortcut(1, false, false, false), PLAIN_CLICK),
                (shortcut(2, false, false, false), PLAIN_CLICK),
            ];
            for (candidate, message) in refused {
                let state = test_app_state();
                let before = exchange_of(&state);
                assert_eq!(store_settings(&state, exchange(candidate.clone())), Err(message.to_string()), "{candidate:?}");
                assert_eq!(exchange_of(&state), before, "{candidate:?}");
            }
        }

        // keep:critical — irreversible: a load fallback that panics, or one that
        // lands on a key already taken, either kills the app at start or binds
        // one press to two readers.
        #[test]
        fn the_load_fallback_takes_the_first_of_the_four_candidates_left_free() {
            let ctrl_a = ReadHotkeySettings::default().shortcut;
            let default_loot = LootSettings::default();
            let loot = |capture: Shortcut, preview: Shortcut| LootSettings { capture_shortcut: capture, preview_shortcut: preview, ..Default::default() };
            let cases = [
                (ctrl_a.clone(), default_loot.clone(), alt(2)),
                (alt(2), default_loot.clone(), alt(4)),
                (alt(2), loot(alt(4), default_loot.preview_shortcut.clone()), alt(5)),
                (alt(2), loot(alt(4), alt(5)), alt(6)),
                (alt(6), loot(alt(4), alt(5)), alt(2)),
            ];
            for (read, loot, expected) in cases {
                assert_eq!(fallback_apart_from(&read, &loot), exchange(expected.clone()), "read {read:?}, loot {:?}/{:?}", loot.capture_shortcut, loot.preview_shortcut);
            }
        }

        // keep:critical — silent: a press mapped to the wrong event code tells
        // the player the panel or the grab failed when something else did, or
        // a missing/duplicate event leaves a stale status or fills twice.
        #[test]
        fn each_exchange_press_outcome_emits_its_one_event_with_the_press_time() {
            let at = 1_700_000_000_123_u64;
            let cases = [
                (
                    DispatchOutcome::Selected(ReaderId::Exchange),
                    Some(Ok(ember_for_chaos())),
                    serde_json::json!({
                        "pressedAtMs": at,
                        "selection": {
                            "mode": "sell", "currency": "chaos", "ratio": 130.0, "oneSide": "right",
                            "left": 130.0, "right": 1.0, "stock": 1,
                            "wantName": "Chaos Orb", "haveName": "Allflame Ember of the Ethereal"
                        }
                    }),
                ),
                (
                    DispatchOutcome::Selected(ReaderId::Exchange),
                    Some(Err(parse::ExchangeReadError::RatioUnreadable)),
                    serde_json::json!({"pressedAtMs": at, "error": "ratio-unreadable"}),
                ),
                (
                    DispatchOutcome::Selected(ReaderId::Exchange),
                    Some(Err(parse::ExchangeReadError::NamesUnreadable)),
                    serde_json::json!({"pressedAtMs": at, "error": "names-unreadable"}),
                ),
                (
                    DispatchOutcome::Selected(ReaderId::Exchange),
                    Some(Err(parse::ExchangeReadError::NoCurrencySide)),
                    serde_json::json!({"pressedAtMs": at, "error": "no-currency-side"}),
                ),
                (DispatchOutcome::NoMatch, None, serde_json::json!({"pressedAtMs": at, "error": "panel-not-found"})),
                (
                    DispatchOutcome::Failed("no monitor".into()),
                    None,
                    serde_json::json!({"pressedAtMs": at, "error": "grab-failed"}),
                ),
                (DispatchOutcome::Suppressed, None, serde_json::json!({"pressedAtMs": at, "error": "privacy-refused"})),
            ];
            for (outcome, read, expected) in cases {
                let label = format!("{outcome:?} / {read:?}");
                assert_eq!(emitted(outcome, read), vec![expected], "{label}");
            }
        }
    }
}
