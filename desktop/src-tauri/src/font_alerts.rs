//! Divine Font craft alerts (POE-328): the exceptional-support price mirror and
//! the projection of the font OCR's current frames onto the two alert crafts.
//!
//! The mirror is the shape `GET /api/analysis/exceptional-supports` serves
//! (plan "The wire", homes H5); it is polled the way the temple market is
//! (`temple::market::spawn_temple_market_poll`).

use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::Notify;

use crate::font_parser::CraftOption;
use crate::AppState;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionalPrices {
    pub source: String,
    pub variant: String,
    pub as_of: Option<DateTime<Utc>>,
    pub divine_rate: f64,
    pub gems: Vec<ExceptionalGem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionalGem {
    pub name: String,
    pub chaos: Option<f64>,
}

/// The stored read plus the server generation it belongs to. A server change
/// bumps the generation, so an answer requested before it is rejected.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PriceMirror {
    generation: u64,
    read: Option<ExceptionalPrices>,
}

/// The server snapshots hourly; the 2 h stale line is judged in the view.
pub const EXCEPTIONAL_PRICES_POLL: Duration = Duration::from_secs(5 * 60);

static PRICES_NOTIFY: LazyLock<Notify> = LazyLock::new(Notify::new);

/// A failed fetch or a cold server (`asOf: null`) keeps the stored read.
pub(crate) fn next_stored(
    stored: Option<ExceptionalPrices>,
    fetched: Option<ExceptionalPrices>,
) -> Option<ExceptionalPrices> {
    match fetched {
        Some(prices) if prices.as_of.is_some() => Some(prices),
        _ => stored,
    }
}

pub(crate) fn supersede(m: &mut PriceMirror) {
    m.generation += 1;
    m.read = None;
}

/// Stores `fetched` unless it was requested under an older generation; true
/// when the stored read changed.
pub(crate) fn accept(
    m: &mut PriceMirror,
    request_generation: u64,
    fetched: Option<ExceptionalPrices>,
) -> bool {
    if request_generation != m.generation {
        return false;
    }
    let next = next_stored(m.read.clone(), fetched);
    let changed = next != m.read;
    m.read = next;
    changed
}

async fn fetch_exceptional_prices_once(app: &AppHandle) -> (u64, Option<ExceptionalPrices>) {
    let state = app.state::<AppState>();
    // Generation first, url second: `set_server_url` writes the url before it
    // calls `on_server_url_changed`, so an old url can only pair with an old
    // generation, which `accept` rejects.
    let generation = state
        .exceptional_prices
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .generation;
    let server_url = crate::settings::server_base(&state);
    let http = state.server_http.clone();
    let url = format!("{}/api/analysis/exceptional-supports", server_url);

    let resp = match http.get(&url).timeout(Duration::from_secs(15)).send().await {
        Ok(r) => r,
        Err(e) => {
            log::debug!("exceptional prices fetch: request to {} failed: {}", url, e);
            return (generation, None);
        }
    };
    if !resp.status().is_success() {
        log::debug!("exceptional prices fetch: {} returned {}", url, resp.status());
        return (generation, None);
    }
    match resp.json::<ExceptionalPrices>().await {
        Ok(prices) => (generation, Some(prices)),
        Err(e) => {
            log::debug!("exceptional prices fetch: bad JSON from {}: {}", url, e);
            (generation, None)
        }
    }
}

/// Polls every [`EXCEPTIONAL_PRICES_POLL`], and at once after a server change.
/// Unconditional, like the temple market poll.
pub fn spawn_exceptional_price_poll(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut was_priced: Option<bool> = None;
        loop {
            let (generation, fetched) = fetch_exceptional_prices_once(&app).await;
            let priced = {
                let state = app.state::<AppState>();
                let mut mirror = state
                    .exceptional_prices
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                accept(&mut mirror, generation, fetched);
                mirror.read.is_some()
            };
            if was_priced != Some(priced) {
                if priced {
                    log::info!("exceptional prices: priced read stored");
                } else {
                    log::info!("exceptional prices: no priced read yet");
                }
                was_priced = Some(priced);
            }
            tokio::select! {
                _ = tokio::time::sleep(EXCEPTIONAL_PRICES_POLL) => {}
                _ = PRICES_NOTIFY.notified() => {}
            }
        }
    });
}

/// Drops the old server's read (the overlay shows "pricing…" at once) and asks
/// the new server immediately.
pub fn on_server_url_changed(app: &AppHandle) {
    {
        let state = app.state::<AppState>();
        supersede(&mut state.exceptional_prices.lock().unwrap_or_else(|e| e.into_inner()));
    }
    PRICES_NOTIFY.notify_one();
}

/// Fixed display order, exceptional above transfigure (Q2 interim; homes H2
/// names the panel line-index alternative).
pub const ALERT_CRAFT_TYPES: [&str; 2] = ["exchange_exceptional", "transform_direct"];

/// The alert crafts the latest current font read listed (POE-391 H2). Each
/// press replaces it, so an alert stays up between presses for as long as the
/// panel showed it at the last one.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AlertSeen {
    exceptional: bool,
    transfigure: bool,
}

/// An active read replaces the set with the alert crafts it lists; an inactive
/// read (the panel is not on screen) clears it.
pub(crate) fn note_alert_read(seen: &mut AlertSeen, font_active: bool, options: &[CraftOption]) {
    let lists = |t: &str| font_active && options.iter().any(|o| o.option_type == t);
    *seen = AlertSeen {
        exceptional: lists(ALERT_CRAFT_TYPES[0]),
        transfigure: lists(ALERT_CRAFT_TYPES[1]),
    };
}

/// Records a read only if `is_current` still holds once the slot is locked:
/// every context end stores its token before it clears the slot, so a read
/// checked under the lock cannot write after that clear.
pub(crate) fn record_current_alert_read(
    slot: &Mutex<AlertSeen>,
    is_current: impl FnOnce() -> bool,
    font_active: bool,
    options: &[CraftOption],
) {
    let mut seen = slot.lock().unwrap_or_else(|e| e.into_inner());
    if is_current() {
        note_alert_read(&mut seen, font_active, options);
    }
}

/// The alert crafts in the set, in display order.
pub fn alert_crafts(seen: &AlertSeen) -> Vec<&'static str> {
    ALERT_CRAFT_TYPES
        .into_iter()
        .zip([seen.exceptional, seen.transfigure])
        .filter(|(_, listed)| *listed)
        .map(|(craft, _)| craft)
        .collect()
}

/// Called on `FontOpened` and wherever the font session data is reset, so a
/// new panel or lifecycle starts with no alert standing.
pub(crate) fn clear_alert_reads(slot: &Mutex<AlertSeen>) {
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = AlertSeen::default();
}

#[tauri::command]
pub fn get_font_alerts(state: tauri::State<AppState>) -> Vec<String> {
    let seen = *state
        .lab_scan
        .font_alert_seen
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    alert_crafts(&seen)
        .into_iter()
        .map(String::from)
        .collect()
}

#[tauri::command]
pub fn get_exceptional_support_prices(state: tauri::State<AppState>) -> Option<ExceptionalPrices> {
    state
        .exceptional_prices
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .read
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const PRICED: &str = include_str!("../../src/lib/compass/__fixtures__/exceptional-supports/priced.json");
    const MISSING: &str = include_str!("../../src/lib/compass/__fixtures__/exceptional-supports/missing.json");

    fn opt(t: &str) -> CraftOption {
        CraftOption { option_type: t.into(), text: String::new(), value: None }
    }

    fn priced() -> ExceptionalPrices {
        serde_json::from_str(PRICED).unwrap()
    }

    fn missing() -> ExceptionalPrices {
        serde_json::from_str(MISSING).unwrap()
    }

    fn cold() -> ExceptionalPrices {
        ExceptionalPrices { as_of: None, ..priced() }
    }

    fn read_once(options: &[CraftOption]) -> Vec<&'static str> {
        let mut seen = AlertSeen::default();
        note_alert_read(&mut seen, true, options);
        alert_crafts(&seen)
    }

    #[test]
    fn both_alert_crafts_list_exceptional_before_transfigure() {
        assert_eq!(
            read_once(&[opt("transform_direct"), opt("exchange_exceptional")]),
            vec!["exchange_exceptional", "transform_direct"]
        );
    }

    #[test]
    fn transfigure_alone_lists_only_transfigure() {
        assert_eq!(
            read_once(&[opt("transform_random"), opt("transform_direct")]),
            vec!["transform_direct"]
        );
    }

    #[test]
    fn exceptional_alone_lists_only_exceptional() {
        assert_eq!(
            read_once(&[opt("add_quality"), opt("exchange_exceptional")]),
            vec!["exchange_exceptional"]
        );
    }

    #[test]
    fn a_read_without_alert_crafts_lists_nothing() {
        assert!(read_once(&[opt("transform_random"), opt("add_quality")]).is_empty());
    }

    #[test]
    fn an_alert_stays_listed_until_the_next_read_however_long_the_gap() {
        // No clock is involved: nothing but a read or a clear changes the set.
        let slot = Mutex::new(AlertSeen::default());
        record_current_alert_read(&slot, || true, true, &[opt("exchange_exceptional")]);
        std::thread::sleep(Duration::from_millis(1100));
        assert_eq!(alert_crafts(&slot.lock().unwrap()), vec!["exchange_exceptional"]);
    }

    #[test]
    fn a_later_read_without_an_alert_drops_it() {
        let mut seen = AlertSeen::default();
        note_alert_read(&mut seen, true, &[opt("exchange_exceptional"), opt("transform_direct")]);
        note_alert_read(&mut seen, true, &[opt("transform_direct")]);
        assert_eq!(alert_crafts(&seen), vec!["transform_direct"]);
    }

    #[test]
    fn an_inactive_read_clears_the_alerts() {
        let mut seen = AlertSeen::default();
        note_alert_read(&mut seen, true, &[opt("exchange_exceptional")]);
        note_alert_read(&mut seen, false, &[opt("exchange_exceptional")]);
        assert!(alert_crafts(&seen).is_empty());
    }

    #[test]
    fn nothing_read_lists_nothing() {
        assert!(alert_crafts(&AlertSeen::default()).is_empty());
    }

    #[test]
    fn a_clear_drops_alerts_until_a_later_read() {
        let slot = Mutex::new(AlertSeen::default());
        note_alert_read(&mut slot.lock().unwrap(), true, &[opt("exchange_exceptional"), opt("transform_direct")]);
        clear_alert_reads(&slot);
        assert!(alert_crafts(&slot.lock().unwrap()).is_empty());

        note_alert_read(&mut slot.lock().unwrap(), true, &[opt("exchange_exceptional")]);
        assert_eq!(alert_crafts(&slot.lock().unwrap()), vec!["exchange_exceptional"]);
    }

    #[test]
    fn a_read_for_an_ended_context_writes_nothing() {
        let slot = Mutex::new(AlertSeen::default());
        record_current_alert_read(&slot, || false, true, &[opt("exchange_exceptional")]);
        assert!(alert_crafts(&slot.lock().unwrap()).is_empty());
    }

    #[test]
    fn a_read_checked_before_a_context_end_cannot_write_after_its_clear() {
        let slot = Mutex::new(AlertSeen::default());
        let ended = std::cell::Cell::new(false);
        // The context end (token store, then clear) runs once: during the
        // check when the slot is free then, otherwise after the read's write.
        let check = || {
            if let Ok(mut seen) = slot.try_lock() {
                *seen = AlertSeen::default();
                ended.set(true);
                return true; // the token read before the end
            }
            true
        };
        record_current_alert_read(&slot, check, true, &[opt("exchange_exceptional")]);
        if !ended.get() {
            clear_alert_reads(&slot);
        }
        assert!(alert_crafts(&slot.lock().unwrap()).is_empty());
    }

    #[test]
    fn the_missing_price_payload_mirrors_a_null_chaos() {
        assert_eq!(
            missing().gems[1],
            ExceptionalGem { name: "Enlighten Support".into(), chaos: None }
        );
    }

    #[test]
    fn the_priced_payload_mirrors_its_snapshot_time() {
        assert_eq!(priced().as_of, Some(Utc.with_ymd_and_hms(2026, 9, 9, 0, 33, 0).unwrap()));
    }

    #[test]
    fn the_mirror_serialises_back_to_the_wire_shape() {
        // Keys and the timestamp, not whole values: the golden's `9` parses as
        // an integer and the mirror's f64 serialises as `9.0`, which
        // serde_json compares unequal.
        fn keys(v: &serde_json::Value) -> Vec<&str> {
            v.as_object().unwrap().keys().map(String::as_str).collect()
        }
        let golden: serde_json::Value = serde_json::from_str(PRICED).unwrap();
        let sent = serde_json::to_value(priced()).unwrap();
        assert_eq!(keys(&sent), keys(&golden));
        assert_eq!(keys(&sent["gems"][1]), keys(&golden["gems"][1]));
        assert_eq!(sent["asOf"], golden["asOf"]);
    }

    #[test]
    fn a_failed_poll_keeps_the_stored_read() {
        assert_eq!(next_stored(Some(priced()), None), Some(priced()));
    }

    #[test]
    fn a_cold_answer_keeps_the_stored_read() {
        assert_eq!(next_stored(Some(priced()), Some(cold())), Some(priced()));
    }

    #[test]
    fn a_cold_answer_with_nothing_stored_stays_unpriced() {
        assert_eq!(next_stored(None, Some(cold())), None);
    }

    #[test]
    fn a_priced_answer_replaces_the_stored_read() {
        assert_eq!(next_stored(Some(priced()), Some(missing())), Some(missing()));
    }

    #[test]
    fn a_response_requested_before_a_server_change_cannot_restore_prices() {
        let mut m = PriceMirror { generation: 0, read: Some(priced()) };
        let g = m.generation;
        supersede(&mut m);
        accept(&mut m, g, Some(priced()));
        assert_eq!(m.read, None);
    }

    #[test]
    fn a_response_requested_before_a_switch_away_and_back_cannot_restore_prices() {
        let mut m = PriceMirror { generation: 0, read: Some(priced()) };
        let g = m.generation;
        supersede(&mut m);
        supersede(&mut m);
        accept(&mut m, g, Some(priced()));
        assert_eq!(m.read, None);
    }

    #[test]
    fn a_response_requested_after_the_server_change_is_stored() {
        let mut m = PriceMirror::default();
        supersede(&mut m);
        let g = m.generation;
        accept(&mut m, g, Some(missing()));
        assert_eq!(m.read, Some(missing()));
    }

    #[test]
    fn a_server_change_drops_the_stored_read() {
        let mut m = PriceMirror { generation: 0, read: Some(priced()) };
        supersede(&mut m);
        assert_eq!(m.read, None);
        assert_eq!(m.generation, 1);
    }
}
