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

/// The fetch envelope. Entries stay as raw JSON so one lookup path can serve
/// consumers that shape them differently (gem listings, mercenary listings).
#[derive(Debug, Deserialize)]
struct GggFetchResponse {
    result: Vec<serde_json::Value>,
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
// Consumers of the shared trade queue
// ---------------------------------------------------------------------------

/// Who a lookup belongs to.
///
/// One `TradeApiClient` (one IP, one rate-limit budget) serves every consumer,
/// so the queue itself stays shared — but cancellation and event routing must
/// not be. Without this discriminator a mercenary retire cancels the
/// Comparator's gem queue and every window renders every other window's queue
/// progress (POE-202).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TradeSource {
    Gem,
    Mercenary,
}

impl TradeSource {
    /// Index into the per-source flag/counter arrays.
    fn index(self) -> usize {
        match self {
            TradeSource::Gem => 0,
            TradeSource::Mercenary => 1,
        }
    }
}

/// Raw two-phase lookup output: what GGG returned, before any consumer has
/// shaped it. `items` are the untouched fetch entries — the gem path parses
/// them into `TradeListingDetail`, the mercenary path into its own listing
/// type (POE-202).
#[derive(Debug, Clone)]
pub struct RawSearch {
    pub query_id: String,
    pub total: u32,
    pub items: Vec<serde_json::Value>,
    pub league: String,
}

// ---------------------------------------------------------------------------
// Trade queue events (emitted to frontend via Tauri)
// ---------------------------------------------------------------------------

/// `gem` is the lookup's display label, not necessarily a gem: for
/// `TradeSource::Mercenary` it carries the captured mercenary's label. The
/// field name is kept because the payload is a wire contract with two webview
/// consumers (Comparator, overlay comparator). Every consumer must filter on
/// `source` first.
///
/// `rename_all` on the enum renames variants only, so `Waiting` carries its own
/// field-level `rename_all` to keep `waitSecs` matching `tradeApi.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TradeQueueEvent {
    Queued { source: TradeSource, gem: String, position: usize, total: usize },
    #[serde(rename_all = "camelCase")]
    Waiting { source: TradeSource, gem: String, wait_secs: f64, position: usize, total: usize },
    Fetching { source: TradeSource, gem: String, position: usize, total: usize },
    Done { source: TradeSource, gem: String },
    Error { source: TradeSource, gem: String, error: String },
    Cancelled { source: TradeSource, remaining: usize },
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
    /// Number of lookups waiting to acquire the mutex + the one in flight,
    /// across every source. Drives the shared queue position/total the UI shows.
    pending_count: AtomicUsize,
    /// Same count, split per source — the granularity a per-source cancel
    /// needs to know when its own flag may be cleared.
    pending_by_source: [AtomicUsize; 2],
    /// One cancel flag per source; checked after acquiring the mutex against
    /// the in-flight lookup's own source. Cancelling one consumer must leave
    /// the other consumer's queued lookups running.
    cancel_flags: [AtomicBool; 2],
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
            pending_by_source: [AtomicUsize::new(0), AtomicUsize::new(0)],
            cancel_flags: [AtomicBool::new(false), AtomicBool::new(false)],
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

    /// Cancel `source`'s pending trade lookups. An in-flight request completes
    /// but that source's queued lookups bail out with Err("cancelled") without
    /// making GGG requests. Other sources are untouched.
    ///
    /// Returns how many lookups of `source` were pending.
    pub fn cancel(&self, source: TradeSource) -> usize {
        let remaining = self.pending_by_source[source.index()].load(Ordering::SeqCst);
        if remaining > 0 {
            // Read-then-set is not atomic: a lookup enqueued between this load
            // and the store is cancelled with the batch, and one enqueued right
            // after a zero read is not cancelled at all. Tolerated — the flag is
            // cleared again at the top of the next `lookup_query` for a source
            // with nothing else pending, so neither case can latch.
            self.cancel_flags[source.index()].store(true, Ordering::SeqCst);
        }
        log::info!("Trade queue: cancel requested for {:?} ({} pending)", source, remaining);
        remaining
    }

    /// Number of lookups currently pending (queued + in-flight), all sources.
    #[allow(dead_code)]
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
        let query_body = super::query::build_search_query_with_mode(gem_name, variant, dedication);

        // `lookup_query` logs the shared `label` only; the variant is gem-path
        // detail, so it is logged here rather than widening `label` (which is
        // the `gem` key on every emitted event).
        log::info!("Trade gem lookup: {} ({}), dedication={}", gem_name, variant, dedication);

        let raw = self
            .lookup_query(TradeSource::Gem, gem_name, query_body, &emit)
            .await?;

        let mut listings = Vec::with_capacity(raw.items.len());
        for item in raw.items {
            match serde_json::from_value::<GggFetchEntry>(item) {
                Ok(entry) => listings.push(parse_listing_entry(entry)),
                Err(e) => {
                    let error = format!("Trade fetch parse failed: {}", e);
                    emit(TradeQueueEvent::Error {
                        source: TradeSource::Gem,
                        gem: gem_name.to_string(),
                        error: error.clone(),
                    });
                    return Err(error);
                }
            }
        }

        Ok(build_result(
            gem_name,
            variant,
            &raw.league,
            &raw.query_id,
            raw.total as i32,
            listings,
            divine_chaos_rate,
        ))
    }

    /// Run one two-phase lookup for `source` against a caller-supplied search
    /// body: serialize -> rate-limit -> search -> rate-limit -> fetch.
    ///
    /// This owns everything that makes the queue a queue — the serializing
    /// mutex, the fail-closed league read, both rate-limit waits, the cancel
    /// checks and the shared counters — so that every consumer shares one rate
    /// limit budget (a second client means a second IP-less budget and the ban
    /// that came with it, commit e359be7). Consumers only shape `RawSearch`.
    ///
    /// `label` is the display name carried on every emitted queue event.
    ///
    /// The emitted `Done` event means **the fetch succeeded**, not that the
    /// lookup succeeded: consumers that parse `RawSearch::items` can still fail
    /// afterwards. Derive terminal state from the returned `RawSearch` (or the
    /// `Err`), never from the event stream.
    pub async fn lookup_query(
        &self,
        source: TradeSource,
        label: &str,
        query_body: serde_json::Value,
        emit: impl Fn(TradeQueueEvent),
    ) -> Result<RawSearch, String> {
        // Fail closed on an unresolved league — do this before touching queue
        // counters or awaiting anything, so an unknown league never reaches the
        // GGG API and no bookkeeping needs unwinding. `league()` locks, clones
        // and drops its guard, so nothing is held across the awaits below.
        let league = match self.league() {
            Ok(l) => l,
            Err(e) => {
                emit(TradeQueueEvent::Error {
                    source,
                    gem: label.to_string(),
                    error: e.clone(),
                });
                return Err(e);
            }
        };

        let pending = self.pending_count.fetch_add(1, Ordering::SeqCst) + 1;
        // Clear a cancel that never had anything to cancel. `cancel` can be
        // called for a source with nothing pending, and nothing would drain to
        // clear the flag — the next lookup of that source would then bail out.
        // Only this lookup being the source's first makes that safe: with
        // others still pending the flag belongs to a live batch.
        if self.pending_by_source[source.index()].fetch_add(1, Ordering::SeqCst) == 0 {
            self.cancel_flags[source.index()].store(false, Ordering::SeqCst);
        }
        self.enqueued.fetch_add(1, Ordering::SeqCst);
        // `completed` can already have overtaken `pending` when a batch drains
        // concurrently; saturate rather than underflow-panic in dev builds.
        let position = pending.saturating_sub(self.completed.load(Ordering::SeqCst));

        emit(TradeQueueEvent::Queued {
            source,
            gem: label.to_string(),
            position,
            total: pending,
        });

        // Serialize: wait for previous lookup to finish.
        let _guard = self.lookup_mutex.lock().await;

        // Check this source's cancel flag after acquiring mutex.
        if self.cancel_flags[source.index()].load(Ordering::SeqCst) {
            self.drain_one(source);
            return Err("cancelled".to_string());
        }

        let current_pending = self.pending_count.load(Ordering::SeqCst);
        let current_pos = self.completed.load(Ordering::SeqCst) + 1;

        // Phase 1: Search (rate-limit → request)
        let search_wait = self.rate_limiter.estimate_wait("search");
        if !search_wait.is_zero() {
            emit(TradeQueueEvent::Waiting {
                source,
                gem: label.to_string(),
                wait_secs: search_wait.as_secs_f64(),
                position: current_pos,
                total: current_pending,
            });
            log::info!("Rate limiter: waiting {:?} for search pool capacity", search_wait);
            tokio::time::sleep(search_wait).await;
        }

        emit(TradeQueueEvent::Fetching {
            source,
            gem: label.to_string(),
            position: current_pos,
            total: current_pending,
        });

        let search_result = self.execute_search(label, &query_body, &league).await;
        let search_response = match search_result {
            Ok(r) => r,
            Err(e) => {
                self.drain_one(source);
                emit(TradeQueueEvent::Error {
                    source,
                    gem: label.to_string(),
                    error: e.clone(),
                });
                return Err(e);
            }
        };

        if search_response.ids.is_empty() {
            self.drain_one(source);
            emit(TradeQueueEvent::Done {
                source,
                gem: label.to_string(),
            });
            return Ok(RawSearch {
                query_id: search_response.query_id,
                total: search_response.total as u32,
                items: vec![],
                league,
            });
        }

        // Check cancel between search and fetch — no point fetching if cancelled.
        if self.cancel_flags[source.index()].load(Ordering::SeqCst) {
            self.drain_one(source);
            return Err("cancelled".to_string());
        }

        // Phase 2: Fetch top 10 (rate-limit → request)
        let fetch_wait = self.rate_limiter.estimate_wait("fetch");
        if !fetch_wait.is_zero() {
            emit(TradeQueueEvent::Waiting {
                source,
                gem: label.to_string(),
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

        self.drain_one(source);

        match listings_result {
            Ok(items) => {
                emit(TradeQueueEvent::Done {
                    source,
                    gem: label.to_string(),
                });
                Ok(RawSearch {
                    query_id: search_response.query_id,
                    total: search_response.total as u32,
                    items,
                    league,
                })
            }
            Err(e) => {
                emit(TradeQueueEvent::Error {
                    source,
                    gem: label.to_string(),
                    error: e.clone(),
                });
                Err(e)
            }
        }
    }

    /// One lookup leaves the queue: shared counters first, then this source's
    /// own counter, then the reset check.
    fn drain_one(&self, source: TradeSource) {
        self.pending_count.fetch_sub(1, Ordering::SeqCst);
        self.pending_by_source[source.index()].fetch_sub(1, Ordering::SeqCst);
        self.completed.fetch_add(1, Ordering::SeqCst);
        self.maybe_reset_counters(source);
    }

    /// Clear `source`'s cancel flag once that source has drained, and reset the
    /// shared batch counters once the whole queue has.
    ///
    /// The two conditions are deliberately different. The counters describe one
    /// shared queue — "position 2 of 5" is a fact about every consumer's wait —
    /// so they may only reset when nothing at all is pending. A cancel flag
    /// describes one consumer, so a draining gem batch must not clear a
    /// mercenary cancel that still has queued lookups to stop.
    fn maybe_reset_counters(&self, source: TradeSource) {
        if self.pending_by_source[source.index()].load(Ordering::SeqCst) == 0 {
            self.cancel_flags[source.index()].store(false, Ordering::SeqCst);
        }
        if self.pending_count.load(Ordering::SeqCst) == 0 {
            self.enqueued.store(0, Ordering::SeqCst);
            self.completed.store(0, Ordering::SeqCst);
        }
    }

    /// POST /api/trade/search/{league}
    async fn execute_search(
        &self,
        label: &str,
        query_body: &serde_json::Value,
        league: &str,
    ) -> Result<SearchResponse, String> {
        let url = format!(
            "{}/api/trade/search/{}",
            TRADE_API_BASE_URL, league
        );

        log::info!("Trade search: {} → {}", label, url);
        log::info!("Trade query body: {}", serde_json::to_string(query_body).unwrap_or_default());

        let response = self
            .http_client
            .post(&url)
            .header("accept", "application/json")
            .json(query_body)
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
    ) -> Result<Vec<serde_json::Value>, String> {
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

        log::info!("Trade fetch OK: {} entries fetched", parsed.result.len());

        Ok(parsed.result)
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

    /// The `Waiting` payload is a wire contract with `tradeApi.ts`, which reads
    /// `waitSecs`. `rename_all` on the enum only renames variants, so without a
    /// field-level `rename_all` this serializes as `wait_secs` and every
    /// consumer silently reads `undefined`. Mutation check: dropping the
    /// variant's `#[serde(rename_all)]` fails both assertions.
    #[test]
    fn waiting_event_serializes_fields_in_camel_case() {
        let json = serde_json::to_value(TradeQueueEvent::Waiting {
            source: TradeSource::Gem,
            gem: "Spark".to_string(),
            wait_secs: 2.5,
            position: 1,
            total: 3,
        })
        .unwrap();
        assert_eq!(json["kind"], "waiting");
        assert_eq!(json["source"], "gem");
        assert_eq!(json["waitSecs"], 2.5);
        assert!(json.get("wait_secs").is_none(), "snake_case key must not be present: {}", json);
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
