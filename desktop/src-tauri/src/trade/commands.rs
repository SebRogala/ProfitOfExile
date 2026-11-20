//! The trade command group (POE-443): GGG trade lookups and their cancel, the
//! overlay's trade refresh, and the submit retry policy `submit` reads.

use tauri::{AppHandle, Emitter, Manager};

use crate::{app_log, settings, trade, AppState};

/// Cancel the pending GEM trade lookups. In-flight request completes but
/// queued lookups bail out without making GGG requests.
///
/// Scoped to `TradeSource::Gem`: this is the Comparator's cancel button, and
/// before POE-202 it also killed every other consumer's queued lookups.
#[tauri::command]
pub(crate) fn trade_cancel(app: AppHandle) {
    let state = app.state::<AppState>();
    let source = trade::TradeSource::Gem;
    let remaining = state.trade_client.cancel(source);
    use tauri::Emitter;
    if let Err(e) = app.emit("trade-queue", trade::TradeQueueEvent::Cancelled { source, remaining }) {
        log::warn!("emit trade-queue Cancelled failed: {}", e);
    }
    app_log(&app, format!("Trade queue cancelled ({} pending)", remaining));
}

/// Total attempts for a trade submit that the server sheds with 503 — the
/// first send plus two retries.
///
/// The insert behind the endpoint is idempotent (`ON CONFLICT DO NOTHING` on
/// `(league, time, gem, variant)`) and the submit is already detached, so a
/// retry can duplicate work but never rows. The bound is the point: the server
/// grew a queue and a 503 precisely to shed load, and an unbounded client-side
/// retry would hand that load straight back.
pub(crate) const TRADE_SUBMIT_ATTEMPTS: u32 = 3;
/// Wait used when a 503 carries no parseable `Retry-After`.
const TRADE_SUBMIT_RETRY_FALLBACK: std::time::Duration = std::time::Duration::from_secs(1);
/// Ceiling on an honoured `Retry-After`. The row is a cache-enrichment nicety;
/// holding a task for minutes because a header said so is not worth it.
const TRADE_SUBMIT_RETRY_MAX: std::time::Duration = std::time::Duration::from_secs(5);

/// Leading `max` bytes of a response body for a log line.
///
/// Cuts on a character boundary: `&body[..body.len().min(max)]` panics when the
/// truncation point lands mid-codepoint, and an error body is exactly the kind
/// of response that can carry a non-ASCII byte (a proxy's HTML error page, a
/// server message quoting user text). Panicking while logging a rejection would
/// lose the rejection.
pub(crate) fn body_excerpt(body: &str, max: usize) -> &str {
    if body.len() <= max {
        return body;
    }
    let mut end = max;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    &body[..end]
}

/// Delay to wait before retrying a shed submit.
///
/// Honours the delta-seconds form of `Retry-After` (what the server sends),
/// clamped to `TRADE_SUBMIT_RETRY_MAX`. The HTTP-date form is not parsed — it
/// falls back like any other unusable value.
pub(crate) fn retry_after_delay(header: Option<&str>) -> std::time::Duration {
    header
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
        .unwrap_or(TRADE_SUBMIT_RETRY_FALLBACK)
        .min(TRADE_SUBMIT_RETRY_MAX)
}

/// Direct trade API lookup against GGG from the desktop app.
/// Each user has their own IP → own rate limits (no shared server bottleneck).
/// Accepts optional divine rate for chaos normalization of divine-priced listings.
/// When mode is "dedication", the query targets corrupted 21/23 skill gems.
#[tauri::command]
pub(crate) async fn trade_lookup(
    gem: String,
    variant: String,
    divine_rate: Option<f64>,
    mode: Option<String>,
    app: AppHandle,
) -> Result<trade::TradeLookupResult, String> {
    let state = app.state::<AppState>();
    let rate = divine_rate.unwrap_or(0.0);
    let dedication = mode.as_deref() == Some("dedication");
    if rate <= 0.0 {
        log::warn!("Trade lookup: divine_rate is 0 — divine-priced listings will NOT be normalized to chaos");
    }
    app_log(&app, format!("Trade lookup: {} ({}) divine_rate={:.0} dedication={}", gem, variant, rate, dedication));

    let app_for_emit = app.clone();
    let league = state.ssot.lock().unwrap_or_else(|e| e.into_inner()).league.name.clone();
    let result = state.trade_client.lookup_gem_with_mode(league, &gem, &variant, rate, dedication, |event| {
        use tauri::Emitter;
        if let Err(e) = app_for_emit.emit("trade-queue", &event) {
            log::warn!("emit trade-queue failed: {}", e);
        }
    }).await
        .map_err(|e| {
            if e == trade::CANCELLED {
                return e; // Don't log cancellations as errors
            }
            app_log(&app, format!("Trade error: {}", e));
            e
        })?;

    app_log(
        &app,
        format!(
            "Trade result: {} total, {} listings, floor={:.1} {}",
            result.total,
            result.listings.len(),
            if result.listings.is_empty() { 0.0 } else { result.listings[0].price },
            if result.listings.is_empty() { "" } else { &result.listings[0].currency },
        ),
    );

    // Fire-and-forget: submit trade result to server for cache enrichment
    {
        let server_url = settings::server_base(&state);
        let http = state.server_http.clone();
        let submit_result = result.clone();
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            let url = format!("{}/api/trade/submit", server_url);
            trade::submit::submit_trade_result(&http, &url, &submit_result, app_clone.clone(),
                |line| app_log(&app_clone, line),
            ).await;
        });
    }

    Ok(result)
}

#[tauri::command]
pub(crate) fn request_trade_refresh(gem: String, variant: String, app: AppHandle) {
    if let Err(e) = app.emit("overlay-trade-refresh", serde_json::json!({ "name": gem, "variant": variant })) {
        log::warn!("emit overlay-trade-refresh failed: {}", e);
    }
}
