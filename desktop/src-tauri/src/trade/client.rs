//! GGG Path of Exile trade API client.
//!
//! Direct port of Go's internal/trade/client.go.
//!
//! Two-phase lookup:
//!   1. POST /api/trade/search/{league} → query ID + result IDs
//!   2. GET  /api/trade/fetch/{ids}?query={queryId} → listing details
//!
//! Uses a browser-like User-Agent (Awakened PoE Trade does the same).
//! No POESESSID needed for public listings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use super::rate_limiter::TradeRateLimiter;
use super::signals::build_result;
use super::types::{SearchResponse, TradeListingDetail, TradeLookupResult};

const TRADE_API_BASE_URL: &str = "https://www.pathofexile.com";
const TRADE_CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Browser-like User-Agent. GGG blocks non-browser UAs.
/// Awakened PoE Trade uses Electron's default Chromium UA — same idea.
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36";

// ---------------------------------------------------------------------------
// GGG API response shapes (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GggSearchResponse {
    id: String,
    result: Vec<String>,
    total: i32,
}

#[derive(Debug, Deserialize)]
struct GggFetchResponse {
    result: Vec<GggFetchEntry>,
}

#[derive(Debug, Deserialize)]
struct GggFetchEntry {
    listing: GggFetchListing,
    item: GggFetchItem,
}

#[derive(Debug, Deserialize)]
struct GggFetchListing {
    indexed: DateTime<Utc>,
    account: GggFetchAccount,
    price: GggFetchPrice,
}

#[derive(Debug, Deserialize)]
struct GggFetchAccount {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GggFetchPrice {
    amount: f64,
    currency: String,
}

#[derive(Debug, Deserialize)]
struct GggFetchItem {
    #[serde(default)]
    corrupted: bool,
    #[serde(default)]
    properties: Vec<GggItemProperty>,
}

#[derive(Debug, Deserialize)]
struct GggItemProperty {
    name: String,
    values: Vec<Vec<serde_json::Value>>,
}

// ---------------------------------------------------------------------------
// Trade queue events (emitted to frontend via Tauri)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TradeQueueEvent {
    Queued { gem: String, position: usize, total: usize },
    Waiting { gem: String, wait_secs: f64, position: usize, total: usize },
    Fetching { gem: String, position: usize, total: usize },
    Done { gem: String },
    Error { gem: String, error: String },
    Cancelled { remaining: usize },
}

// ---------------------------------------------------------------------------
// Trade API client
// ---------------------------------------------------------------------------

/// HTTP client for the GGG Path of Exile trade API.
///
/// Each desktop app instance has its own client = own IP = own rate limits.
/// The rate limiter maintains separate "search" and "fetch" pools with
/// multi-tier sliding windows, synced from GGG's X-Rate-Limit-* headers.
///
/// All lookups are serialized through `lookup_mutex` to prevent concurrent
/// requests from bypassing the rate limiter (TOCTOU race fix).
pub struct TradeApiClient {
    http_client: reqwest::Client,
    /// Resolved league name. `None` means **not yet resolved** — trade lookups
    /// fail closed (error out) rather than silently querying the wrong league.
    /// No hardcoded default; written only via `set_league` once resolved
    /// (POE-128). Guarded by a std Mutex used with lock-clone-drop discipline —
    /// the guard is never held across an `.await`.
    league: Mutex<Option<String>>,
    rate_limiter: TradeRateLimiter,
    /// Serializes all lookup_gem calls — one search+fetch pair at a time.
    lookup_mutex: tokio::sync::Mutex<()>,
    /// Number of lookups waiting to acquire the mutex + the one in flight.
    pending_count: AtomicUsize,
    /// Set by trade_cancel command; checked after acquiring mutex.
    cancel_flag: AtomicBool,
    /// Counter of enqueued lookups in the current batch. Reset when queue drains.
    enqueued: AtomicUsize,
    /// Counter of completed/cancelled lookups in the current batch. Reset when queue drains.
    completed: AtomicUsize,
}

impl TradeApiClient {
    pub fn new() -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(TRADE_CLIENT_TIMEOUT)
            .user_agent(BROWSER_USER_AGENT)
            .build()
            .expect("failed to build trade HTTP client");

        Self {
            http_client,
            league: Mutex::new(None),
            rate_limiter: TradeRateLimiter::new(),
            lookup_mutex: tokio::sync::Mutex::new(()),
            pending_count: AtomicUsize::new(0),
            cancel_flag: AtomicBool::new(false),
            enqueued: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
        }
    }

    /// Set the resolved league. Lock, set `Some(name)`, drop the guard.
    /// Unused in chunk 2 — the fetch task that calls this lands in chunk 3.
    #[allow(dead_code)]
    pub fn set_league(&self, name: String) {
        let mut guard = self.league.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(name);
    }

    /// Clone-helper for the resolved league name (lock → clone → drop guard).
    ///
    /// Returns `Err` when the league is unset (`None`) so callers **fail
    /// closed** instead of querying the wrong league. The guard is dropped
    /// before returning, so it is never held across an `.await` at any call
    /// site — preserving the `Send` bound on the async lookup path.
    pub(crate) fn league(&self) -> Result<String, String> {
        let value = {
            let guard = self.league.lock().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };
        value.ok_or_else(|| {
            "Trade lookup unavailable: league not resolved yet".to_string()
        })
    }

    /// Cancel all pending trade lookups. In-flight request completes but
    /// queued lookups bail out with Err("cancelled") without making GGG requests.
    pub fn cancel(&self) -> usize {
        self.cancel_flag.store(true, Ordering::SeqCst);
        let remaining = self.pending_count.load(Ordering::SeqCst);
        log::info!("Trade queue: cancel requested ({} pending)", remaining);
        remaining
    }

    /// Number of lookups currently pending (queued + in-flight).
    pub fn pending(&self) -> usize {
        self.pending_count.load(Ordering::Relaxed)
    }

    /// Full trade lookup: serialize → rate-limit → search → rate-limit → fetch → build result.
    ///
    /// `divine_chaos_rate`: divine→chaos exchange rate for price normalization.
    /// Pass 0.0 to skip normalization (listings keep raw currency values).
    /// `emit`: callback to emit TradeQueueEvent to the frontend.
    pub async fn lookup_gem(
        &self,
        gem_name: &str,
        variant: &str,
        divine_chaos_rate: f64,
        emit: impl Fn(TradeQueueEvent),
    ) -> Result<TradeLookupResult, String> {
        self.lookup_gem_with_mode(gem_name, variant, divine_chaos_rate, false, emit).await
    }

    pub async fn lookup_gem_with_mode(
        &self,
        gem_name: &str,
        variant: &str,
        divine_chaos_rate: f64,
        dedication: bool,
        emit: impl Fn(TradeQueueEvent),
    ) -> Result<TradeLookupResult, String> {
        // Fail closed on an unresolved league — do this before touching queue
        // counters or awaiting anything, so an unknown league never reaches the
        // GGG API and no bookkeeping needs unwinding. `league()` locks, clones
        // and drops its guard, so nothing is held across the awaits below.
        let league = match self.league() {
            Ok(l) => l,
            Err(e) => {
                emit(TradeQueueEvent::Error {
                    gem: gem_name.to_string(),
                    error: e.clone(),
                });
                return Err(e);
            }
        };

        let pending = self.pending_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.enqueued.fetch_add(1, Ordering::SeqCst);
        let position = pending - self.completed.load(Ordering::SeqCst);

        emit(TradeQueueEvent::Queued {
            gem: gem_name.to_string(),
            position,
            total: pending,
        });

        // Serialize: wait for previous lookup to finish.
        let _guard = self.lookup_mutex.lock().await;

        // Check cancel flag after acquiring mutex.
        if self.cancel_flag.load(Ordering::SeqCst) {
            let remaining = self.pending_count.fetch_sub(1, Ordering::SeqCst) - 1;
            self.completed.fetch_add(1, Ordering::SeqCst);
            // Last cancelled lookup resets the flag and counters.
            if remaining == 0 {
                self.cancel_flag.store(false, Ordering::SeqCst);
                self.enqueued.store(0, Ordering::SeqCst);
                self.completed.store(0, Ordering::SeqCst);
            }
            return Err("cancelled".to_string());
        }

        let current_pending = self.pending_count.load(Ordering::SeqCst);
        let current_pos = self.completed.load(Ordering::SeqCst) + 1;

        // Phase 1: Search (rate-limit → request)
        let search_wait = self.rate_limiter.estimate_wait("search");
        if !search_wait.is_zero() {
            emit(TradeQueueEvent::Waiting {
                gem: gem_name.to_string(),
                wait_secs: search_wait.as_secs_f64(),
                position: current_pos,
                total: current_pending,
            });
            log::info!("Rate limiter: waiting {:?} for search pool capacity", search_wait);
            tokio::time::sleep(search_wait).await;
        }

        emit(TradeQueueEvent::Fetching {
            gem: gem_name.to_string(),
            position: current_pos,
            total: current_pending,
        });

        let search_result = self
            .execute_search_with_mode(gem_name, variant, dedication, &league)
            .await;
        let search_response = match search_result {
            Ok(r) => r,
            Err(e) => {
                self.pending_count.fetch_sub(1, Ordering::SeqCst);
                self.completed.fetch_add(1, Ordering::SeqCst);
                self.maybe_reset_counters();
                emit(TradeQueueEvent::Error {
                    gem: gem_name.to_string(),
                    error: e.clone(),
                });
                return Err(e);
            }
        };

        if search_response.ids.is_empty() {
            self.pending_count.fetch_sub(1, Ordering::SeqCst);
            self.completed.fetch_add(1, Ordering::SeqCst);
            self.maybe_reset_counters();
            emit(TradeQueueEvent::Done { gem: gem_name.to_string() });
            return Ok(build_result(
                gem_name,
                variant,
                &league,
                &search_response.query_id,
                search_response.total,
                vec![],
                divine_chaos_rate,
            ));
        }

        // Check cancel between search and fetch — no point fetching if cancelled.
        if self.cancel_flag.load(Ordering::SeqCst) {
            self.pending_count.fetch_sub(1, Ordering::SeqCst);
            self.completed.fetch_add(1, Ordering::SeqCst);
            self.maybe_reset_counters();
            return Err("cancelled".to_string());
        }

        // Phase 2: Fetch top 10 (rate-limit → request)
        let fetch_wait = self.rate_limiter.estimate_wait("fetch");
        if !fetch_wait.is_zero() {
            emit(TradeQueueEvent::Waiting {
                gem: gem_name.to_string(),
                wait_secs: fetch_wait.as_secs_f64(),
                position: current_pos,
                total: current_pending,
            });
            log::info!("Rate limiter: waiting {:?} for fetch pool capacity", fetch_wait);
            tokio::time::sleep(fetch_wait).await;
        }

        let listings_result = self
            .fetch_listing_details(&search_response.query_id, &search_response.ids)
            .await;

        self.pending_count.fetch_sub(1, Ordering::SeqCst);
        self.completed.fetch_add(1, Ordering::SeqCst);
        self.maybe_reset_counters();

        match listings_result {
            Ok(listings) => {
                emit(TradeQueueEvent::Done { gem: gem_name.to_string() });
                Ok(build_result(
                    gem_name,
                    variant,
                    &league,
                    &search_response.query_id,
                    search_response.total,
                    listings,
                    divine_chaos_rate,
                ))
            }
            Err(e) => {
                emit(TradeQueueEvent::Error {
                    gem: gem_name.to_string(),
                    error: e.clone(),
                });
                Err(e)
            }
        }
    }

    /// Reset counters and cancel flag when queue fully drains.
    fn maybe_reset_counters(&self) {
        if self.pending_count.load(Ordering::SeqCst) == 0 {
            self.cancel_flag.store(false, Ordering::SeqCst);
            self.enqueued.store(0, Ordering::SeqCst);
            self.completed.store(0, Ordering::SeqCst);
        }
    }

    /// POST /api/trade/search/{league}
    async fn execute_search_with_mode(
        &self,
        gem_name: &str,
        variant: &str,
        dedication: bool,
        league: &str,
    ) -> Result<SearchResponse, String> {
        let query_body = super::query::build_search_query_with_mode(gem_name, variant, dedication);
        let url = format!(
            "{}/api/trade/search/{}",
            TRADE_API_BASE_URL, league
        );

        log::info!("Trade search: {} ({}) → {}", gem_name, variant, url);
        log::info!("Trade query body: {}", serde_json::to_string(&query_body).unwrap_or_default());

        let response = self
            .http_client
            .post(&url)
            .header("accept", "application/json")
            .json(&query_body)
            .send()
            .await
            .map_err(|e| format!("Trade search request failed: {}", e))?;

        let response_headers = response.headers().clone();
        // Sync rate limits from headers (GGG sends these even on 429)
        self.rate_limiter
            .sync_from_response_headers("search", &response_headers);

        let status_code = response.status().as_u16();
        if status_code == 429 {
            return Err("Rate limited by GGG (429). Try again in a moment.".to_string());
        }

        // Only record successful requests toward rate limit budget
        self.rate_limiter.record("search");

        let body_text = response.text().await
            .map_err(|e| format!("Failed to read trade search response: {}", e))?;

        if status_code != 200 {
            return Err(format!(
                "Trade search failed ({}): {}",
                status_code,
                &body_text[..body_text.len().min(300)]
            ));
        }

        let parsed: GggSearchResponse = serde_json::from_str(&body_text)
            .map_err(|e| format!("Trade search parse failed: {}", e))?;

        log::info!(
            "Trade search OK: {} total results, {} IDs returned",
            parsed.total,
            parsed.result.len()
        );

        Ok(SearchResponse {
            query_id: parsed.id,
            ids: parsed.result,
            total: parsed.total,
        })
    }

    /// GET /api/trade/fetch/{ids}?query={queryId}
    async fn fetch_listing_details(
        &self,
        query_id: &str,
        result_ids: &[String],
    ) -> Result<Vec<TradeListingDetail>, String> {
        let ids_to_fetch: Vec<&str> = result_ids.iter().take(10).map(|s| s.as_str()).collect();
        let url = format!(
            "{}/api/trade/fetch/{}?query={}",
            TRADE_API_BASE_URL,
            ids_to_fetch.join(","),
            query_id
        );

        let response = self
            .http_client
            .get(&url)
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("Trade fetch request failed: {}", e))?;

        let response_headers = response.headers().clone();
        self.rate_limiter
            .sync_from_response_headers("fetch", &response_headers);

        let status_code = response.status().as_u16();
        if status_code == 429 {
            return Err("Rate limited by GGG (429). Try again in a moment.".to_string());
        }

        self.rate_limiter.record("fetch");

        if status_code != 200 {
            let body = response.text().await
                .map_err(|e| format!("Failed to read trade fetch response: {}", e))?;
            return Err(format!(
                "Trade fetch failed ({}): {}",
                status_code,
                &body[..body.len().min(300)]
            ));
        }

        let parsed: GggFetchResponse = response
            .json()
            .await
            .map_err(|e| format!("Trade fetch parse failed: {}", e))?;

        log::info!("Trade fetch OK: {} listings", parsed.result.len());

        let listings = parsed
            .result
            .into_iter()
            .map(parse_listing_entry)
            .collect();

        Ok(listings)
    }
}

/// Parse a single GGG fetch entry into our TradeListingDetail.
fn parse_listing_entry(entry: GggFetchEntry) -> TradeListingDetail {
    let mut gem_level = 0i32;
    let mut gem_quality = 0i32;

    for property in &entry.item.properties {
        let value = extract_numeric_property_value(property);
        match property.name.as_str() {
            "Level" => gem_level = value,
            "Quality" => gem_quality = value,
            _ => {}
        }
    }

    TradeListingDetail {
        price: entry.listing.price.amount,
        currency: entry.listing.price.currency,
        chaos_price: 0.0, // normalized later in build_result
        account: entry.listing.account.name,
        indexed_at: entry.listing.indexed,
        gem_level,
        gem_quality,
        corrupted: entry.item.corrupted,
    }
}

/// Extract a numeric value from a GGG item property.
/// Properties come as `[["20", 0]]` — first element is display string
/// (may include "+" prefix or "%" suffix).
fn extract_numeric_property_value(property: &GggItemProperty) -> i32 {
    if property.values.is_empty() || property.values[0].is_empty() {
        return 0;
    }
    match property.values[0][0].as_str() {
        Some(raw) => raw
            .trim_start_matches('+')
            .trim_end_matches('%')
            .parse()
            .unwrap_or(0),
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A freshly constructed client has no resolved league, so the clone-helper
    /// must fail rather than hand back a default — this is the fail-closed
    /// contract (POE-128). Mutation check: if `league()` returned `Ok` for the
    /// `None` state, this assertion fails.
    #[test]
    fn league_returns_err_when_unset() {
        let client = TradeApiClient::new();
        assert!(client.league().is_err());
    }

    /// Once resolved via `set_league`, the clone-helper returns that exact name.
    /// Guards against a helper that drops or mangles the stored value.
    #[test]
    fn league_returns_name_after_set() {
        let client = TradeApiClient::new();
        client.set_league("Mirage".to_string());
        assert_eq!(client.league().unwrap(), "Mirage");
    }

    /// The full lookup read path fails closed when the league is unset: it
    /// surfaces a `TradeQueueEvent::Error` and returns `Err` **without** ever
    /// reaching the GGG API (no `Fetching`/`Queued` progress is emitted because
    /// the guard trips before any network work). Asserting the emitted Error +
    /// the Err return is the observable outcome, not a status flag.
    #[tokio::test]
    async fn lookup_fails_closed_when_league_unset() {
        let client = TradeApiClient::new();
        let events = std::sync::Mutex::new(Vec::new());

        let result = client
            .lookup_gem_with_mode("Empower Support", "20/20", 0.0, false, |e| {
                events.lock().unwrap().push(e);
            })
            .await;

        assert!(result.is_err(), "unresolved league must error, not query");

        let events = events.into_inner().unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TradeQueueEvent::Error { .. })),
            "an Error event must be surfaced on the fail-closed path, got: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, TradeQueueEvent::Fetching { .. })),
            "must not reach the network (no Fetching) when league is unset"
        );
    }
}
