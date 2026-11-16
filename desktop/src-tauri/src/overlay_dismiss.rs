use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
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
    #[cfg(any(windows, test))]
    pub fn shortcuts(&self) -> Vec<Shortcut> {
        let mut shortcuts = vec![self.shortcut.clone()];
        for overlay in self.overlays.values() {
            let shortcut = overlay.shortcut.resolve(&self.shortcut);
            if !shortcuts.contains(shortcut) { shortcuts.push(shortcut.clone()); }
        }
        shortcuts
    }

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

pub(crate) fn with_settings(
    state: &crate::AppState,
    mut snapshot: crate::ssot::AppSsotSnapshot,
) -> crate::ssot::AppSsotSnapshot {
    snapshot.overlay_dismiss = DismissSnapshot {
        settings: state.overlay_dismiss.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        pending: state.dismiss_ledger.pending(),
    };
    snapshot
}

pub const PENDING_CAP: usize = 16;
pub const DISMISS_ACK_TIMEOUT_MS: u64 = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingDismiss {
    pub gen: u64,
    pub shortcut: Shortcut,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DismissSnapshot {
    #[serde(flatten)]
    pub settings: DismissSettings,
    pub pending: Vec<PendingDismiss>,
}

#[derive(Default)]
struct LedgerState {
    gen: u64,
    pending: VecDeque<PendingDismiss>,
    hosts: BTreeMap<String, BTreeSet<u64>>,
}

impl LedgerState {
    fn ready(&self, gen: u64, exists: &mut dyn FnMut(&str) -> bool) -> Option<bool> {
        if !self.pending.iter().any(|pending| pending.gen == gen) { return Some(false); }
        self.hosts.iter().all(|(host, acks)| !exists(host) || acks.contains(&gen)).then_some(true)
    }
}

#[derive(Default)]
pub struct DismissLedger {
    state: Mutex<LedgerState>,
    changed: Condvar,
}

impl DismissLedger {
    pub fn register(&self, window: String) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).hosts.insert(window, BTreeSet::new());
        self.changed.notify_all();
    }

    pub fn publish(&self, shortcut: &Shortcut) -> PendingDismiss {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.gen += 1;
        let pending = PendingDismiss { gen: state.gen, shortcut: shortcut.clone() };
        state.pending.push_back(pending.clone());
        if state.pending.len() > PENDING_CAP { state.pending.pop_front(); }
        let oldest = state.pending.front().unwrap().gen;
        for acks in state.hosts.values_mut() { acks.retain(|gen| *gen >= oldest); }
        self.changed.notify_all();
        pending
    }

    pub fn ack(&self, window: &str, gen: u64) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.pending.iter().any(|pending| pending.gen == gen) {
            if let Some(acks) = state.hosts.get_mut(window) { acks.insert(gen); }
        }
        self.changed.notify_all();
    }

    pub fn pending(&self) -> Vec<PendingDismiss> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).pending.iter().cloned().collect()
    }

    pub(crate) fn ready(&self, gen: u64, exists: &mut dyn FnMut(&str) -> bool) -> Option<bool> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).ready(gen, exists)
    }

    pub(crate) fn await_with(
        &self,
        gen: u64,
        exists: &mut dyn FnMut(&str) -> bool,
        elapsed: &mut dyn FnMut() -> Duration,
        wait: &mut dyn FnMut(Duration),
    ) -> bool {
        let bound = Duration::from_millis(DISMISS_ACK_TIMEOUT_MS);
        loop {
            let remaining = bound.saturating_sub(elapsed());
            if remaining.is_zero() { return false; }
            if let Some(ready) = self.ready(gen, exists) { return ready; }
            wait(remaining);
        }
    }

    fn await_hidden(&self, app: &tauri::AppHandle, gen: u64) -> bool {
        let start = Instant::now();
        let ready = self.await_with(gen, &mut |host| app.get_webview_window(host).is_some(), &mut || start.elapsed(), &mut |remaining| {
            let guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
            // Recheck under the wait lock so an acknowledgement between the
            // outer check and the condvar wait cannot lose its notification.
            if guard.ready(gen, &mut |host| app.get_webview_window(host).is_some()).is_none() {
                drop(self.changed.wait_timeout(guard, remaining).unwrap_or_else(|e| e.into_inner()));
            }
        });
        let hosts: Vec<_> = self.state.lock().unwrap_or_else(|e| e.into_inner()).hosts.keys()
            .filter(|host| app.get_webview_window(host).is_some()).cloned().collect();
        if ready {
            let state = app.state::<crate::AppState>();
            let debug = *state.debug_mode.lock().unwrap_or_else(|e| e.into_inner());
            if debug {
                crate::app_log(app, format!("read hotkey: overlay hide acknowledged after {} ms (hosts: {})", start.elapsed().as_millis(), hosts.join(", ")));
            }
        } else {
            crate::app_log(app, format!("read hotkey: press dropped — overlay hide not acknowledged in 500 ms (hosts: {})", hosts.join(", ")));
        }
        ready
    }
}

#[tauri::command]
pub fn overlay_dismiss_host(app: tauri::AppHandle, window: String) {
    app.state::<crate::AppState>().dismiss_ledger.register(window);
}

#[tauri::command]
pub fn overlay_dismiss_ack(app: tauri::AppHandle, window: String, gen: u64) {
    app.state::<crate::AppState>().dismiss_ledger.ack(&window, gen);
}

pub(crate) fn publish(app: &tauri::AppHandle, shortcut: &Shortcut) -> u64 {
    let pending = app.state::<crate::AppState>().dismiss_ledger.publish(shortcut);
    if let Err(error) = app.emit("overlay-dismiss-key", &pending) {
        crate::app_log(app, format!("overlay dismissal: failed to emit key: {error}"));
    }
    crate::ssot::emit_ssot(app);
    pending.gen
}

pub(crate) fn await_hidden(app: &tauri::AppHandle, gen: u64) -> bool {
    app.state::<crate::AppState>().dismiss_ledger.await_hidden(app, gen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escape() -> Shortcut {
        DismissSettings::default().shortcut
    }

    #[test]
    fn only_the_exact_generation_releases_all_existing_hosts() {
        let ledger = DismissLedger::default();
        ledger.register("temple".into());
        ledger.register("lab".into());
        let older = ledger.publish(&escape()).gen;
        let gen = ledger.publish(&escape()).gen;
        let later = ledger.publish(&escape()).gen;
        ledger.ack("temple", older);
        ledger.ack("temple", later);
        assert_eq!(ledger.ready(gen, &mut |_| true), None);
        ledger.ack("temple", gen);
        assert_eq!(ledger.ready(gen, &mut |_| true), None);
        ledger.ack("lab", gen);
        assert_eq!(ledger.ready(gen, &mut |_| true), Some(true));
    }

    #[test]
    fn evicted_generation_is_lost_even_after_a_late_ack() {
        let ledger = DismissLedger::default();
        ledger.register("temple".into());
        let gen = ledger.publish(&escape()).gen;
        for _ in 0..PENDING_CAP { ledger.publish(&escape()); }
        ledger.ack("temple", gen);
        assert_eq!(ledger.ready(gen, &mut |_| true), Some(false));
        assert_eq!(ledger.pending().len(), PENDING_CAP);
    }

    #[test]
    fn absent_hosts_return_without_waiting() {
        let ledger = DismissLedger::default();
        let gen = ledger.publish(&escape()).gen;
        assert!(ledger.await_with(gen, &mut |_| true, &mut || std::time::Duration::ZERO, &mut |_| panic!("no hosts")));
        ledger.register("temple".into());
        assert!(ledger.await_with(gen, &mut |_| false, &mut || std::time::Duration::ZERO, &mut |_| panic!("window gone")));
    }

    #[test]
    fn unacknowledged_generation_expires_at_the_bound() {
        use std::cell::Cell;
        use std::time::Duration;
        let ledger = DismissLedger::default();
        ledger.register("temple".into());
        let gen = ledger.publish(&escape()).gen;
        let elapsed = Cell::new(Duration::ZERO);
        assert!(!ledger.await_with(gen, &mut |_| true, &mut || elapsed.get(), &mut |remaining| {
            assert_eq!(remaining, Duration::from_millis(500));
            elapsed.set(remaining);
        }));
        assert_eq!(elapsed.get(), Duration::from_millis(500));
    }

    #[test]
    fn an_ack_arriving_at_the_deadline_cannot_revive_the_press() {
        use std::cell::Cell;
        use std::time::Duration;
        let ledger = DismissLedger::default();
        ledger.register("temple".into());
        let gen = ledger.publish(&escape()).gen;
        let elapsed = Cell::new(Duration::ZERO);
        assert!(!ledger.await_with(gen, &mut |_| true, &mut || elapsed.get(), &mut |remaining| {
            elapsed.set(remaining);
            ledger.ack("temple", gen);
        }));
    }

    #[test]
    fn ssot_preserves_pending_order_and_each_shortcut() {
        let state = crate::settings::tests::test_app_state();
        let f = Shortcut { key: 70, ..escape() };
        state.dismiss_ledger.publish(&f);
        state.dismiss_ledger.publish(&escape());
        let snapshot = crate::ssot::build_snapshot(&state);
        assert_eq!(serde_json::to_value(snapshot).unwrap()["overlayDismiss"]["pending"], serde_json::json!([
            {"gen":1,"shortcut":{"key":70,"shift":false,"ctrl":false,"alt":false}},
            {"gen":2,"shortcut":{"key":27,"shift":false,"ctrl":false,"alt":false}}
        ]));
    }

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
