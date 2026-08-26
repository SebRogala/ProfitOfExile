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
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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

/// The `Err` a lookup returns when its source was cancelled while it waited.
///
/// A named constant because it is a control signal two callers branch on
/// (`lib.rs`'s gem command, `mercenary::search`'s lookup task) to tell a user
/// action apart from a failure — a typo on either side turns a cancel into an
/// error toast.
pub const CANCELLED: &str = "cancelled";

/// The HTTP status a lookup error carries, when it carries one.
///
/// This whole path is `Result<_, String>` — the queue, the gem command, the
/// merc lookup task and `CANCELLED` all trade in messages — so a status travels
/// inside the message, as the `(NNN)` marker the four failure sites below write
/// (`Trade search failed (400): …`, `Trade fetch failed (503): …`, and the two
/// `Trade search/fetch rate limited by GGG (429). …` returns). This is the only
/// reader of that marker and it sits in the same file as every writer, with
/// tests that drive a real 400, 429 and 503 through both ends.
///
/// `None` for everything else: a transport failure, a parse failure and a
/// cancel have no status, and none of them is a decision the caller can make
/// about GGG's answer. The parse is what rejects them — reqwest's message
/// parenthesises the URL it failed on, and a URL is not a number.
pub fn error_status(error: &str) -> Option<u16> {
    let open = error.find('(')?;
    let rest = &error[open + 1..];
    let close = rest.find(')')?;
    rest[..close].parse().ok()
}

/// Whether GGG rejected the REQUEST rather than failing to serve it.
///
/// 400–428 and 430–499 are verdicts on the body: the same request earns the same
/// answer however many times it is asked, so a caller that retries one spends
/// its budget for nothing.
///
/// **429 is the deliberate odd one out.** It is a verdict on TIMING, not on the
/// body — the same request would succeed later — and this function still calls
/// it terminal. Two reasons, in this order. First, every 4xx counts toward GGG's
/// separate invalid-request ban, 429 explicitly included
/// (`docs/RESEARCH-poe-trade-api.md:269-274`, quoting GGG: "Invalid requests
/// include any response codes in the HTTP 4xx range. This includes 401, 403, and
/// 429"), so answering one with a retry is not merely wasted but actively
/// harmful. Second, `rate_limiter` exists precisely so this client never
/// produces a 429; one that arrives anyway means the limiter's model of GGG's
/// budget is wrong, and retrying against a wrong model is how the ban is earned.
/// Honouring `Retry-After` is the principled alternative and is deliberately NOT
/// implemented — it would be a second, contradictory scheduler beside the
/// limiter.
///
/// "Terminal" is scoped to the rejected BODY, not to the caller: the merc path
/// records the rejected hash on its slice (`mercenary::search::accept_error`),
/// so a corrected capture is searched for normally while the rejected body stays
/// refused — by the session that earned the rejection and by the fresh one a
/// re-detect opens alike.
///
/// 5xx and transport failures are the transient case and stay retryable.
pub fn is_client_error(error: &str) -> bool {
    matches!(error_status(error), Some(400..=499))
}

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
    /// Origin every request is built against. Always [`TRADE_API_BASE_URL`] in
    /// a shipped build — a field rather than the constant only so the unit
    /// tests can point one client at a local stub server and exercise the
    /// two-phase path (queue events, cancel checkpoints, result shaping)
    /// without reaching GGG.
    base_url: String,
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
    /// Same count, split per source — what [`Self::cancel`] reports back as
    /// "this many were stopped".
    pending_by_source: [AtomicUsize; 2],
    /// One monotonic cancel counter per source. [`Self::cancel`] bumps its
    /// source's; a lookup snapshots it when it joins the queue and treats
    /// itself as cancelled only if the value MOVED since. Cancelling one
    /// consumer leaves the other consumer's queued lookups running.
    ///
    /// An epoch rather than a flag because a flag has no way to say WHICH
    /// lookups a cancel was about: it latched over an empty queue and stopped
    /// the next unrelated lookup, and every rule for clearing it again raced
    /// the next enqueue in the other direction. An epoch answers the question
    /// the checkpoint actually asks — "was I cancelled?" — with no clearing
    /// step to race.
    cancel_epochs: [AtomicU64; 2],
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
            base_url: TRADE_API_BASE_URL.to_string(),
            league: Mutex::new(None),
            rate_limiter: TradeRateLimiter::new(),
            lookup_mutex: tokio::sync::Mutex::new(()),
            pending_count: AtomicUsize::new(0),
            pending_by_source: [AtomicUsize::new(0), AtomicUsize::new(0)],
            cancel_epochs: [AtomicU64::new(0), AtomicU64::new(0)],
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
    /// but that source's queued lookups bail out with [`CANCELLED`] without
    /// making GGG requests. Other sources are untouched.
    ///
    /// Bumping the epoch is unconditional and cannot latch: it stops exactly
    /// the lookups that snapshotted an earlier value — every lookup already in
    /// the queue — and nothing that enqueues afterwards. Calling this with
    /// nothing pending is therefore a no-op rather than a trap for the next
    /// lookup.
    ///
    /// Returns how many lookups of `source` were pending.
    pub fn cancel(&self, source: TradeSource) -> usize {
        let remaining = self.pending_by_source[source.index()].load(Ordering::SeqCst);
        self.cancel_epochs[source.index()].fetch_add(1, Ordering::SeqCst);
        log::info!("Trade queue: cancel requested for {:?} ({} pending)", source, remaining);
        remaining
    }

    /// Whether `source` was cancelled since a lookup snapshotted `since`.
    fn cancelled_since(&self, source: TradeSource, since: u64) -> bool {
        self.cancel_epochs[source.index()].load(Ordering::SeqCst) != since
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

        // The cancel epoch AS THIS LOOKUP JOINS THE QUEUE. Every checkpoint
        // below asks whether it moved since, so a cancel that fired before this
        // lookup existed cannot be about this lookup, and one that fires after
        // always is. Snapshotted before the counters so no cancel can slip into
        // the gap unnoticed.
        let cancel_epoch = self.cancel_epochs[source.index()].load(Ordering::SeqCst);

        let pending = self.pending_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.pending_by_source[source.index()].fetch_add(1, Ordering::SeqCst);
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

        // Check for a cancel of this source after acquiring the mutex.
        if self.cancelled_since(source, cancel_epoch) {
            self.drain_one(source);
            return Err(CANCELLED.to_string());
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
        if self.cancelled_since(source, cancel_epoch) {
            self.drain_one(source);
            return Err(CANCELLED.to_string());
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
        self.maybe_reset_counters();
    }

    /// Reset the shared batch counters once the whole queue has drained.
    ///
    /// Shared, not per source: the counters describe one queue — "position 2 of
    /// 5" is a fact about every consumer's wait — so they may only reset when
    /// nothing at all is pending. Cancellation has nothing to reset here; a
    /// cancel epoch is never cleared, only compared (see [`Self::cancel`]).
    fn maybe_reset_counters(&self) {
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
            self.base_url, league
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
            // Says what happened, and does not promise what does not happen:
            // this lookup is dropped, not retried. Retrying is what earns the
            // invalid-request ban (see [`is_client_error`]), and the limiter —
            // not a retry here — is what is supposed to keep 429s from
            // happening at all.
            return Err(
                "Trade search rate limited by GGG (429). Dropped, not retried.".to_string(),
            );
        }

        // Only record successful requests toward rate limit budget
        self.rate_limiter.record("search");

        let body_text = response.text().await
            .map_err(|e| format!("Failed to read trade search response: {}", e))?;

        if status_code != 200 {
            return Err(format!(
                "Trade search failed ({}): {}",
                status_code,
                // By CHARS, not bytes: GGG's error bodies are UTF-8 and slicing
                // one at byte 300 panics whenever that lands mid-codepoint.
                body_text.chars().take(300).collect::<String>()
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
            self.base_url,
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
            // Same honesty rule as the search leg: dropped, not retried.
            return Err(
                "Trade fetch rate limited by GGG (429). Dropped, not retried.".to_string(),
            );
        }

        self.rate_limiter.record("fetch");

        if status_code != 200 {
            let body = response.text().await
                .map_err(|e| format!("Failed to read trade fetch response: {}", e))?;
            return Err(format!(
                "Trade fetch failed ({}): {}",
                status_code,
                // By CHARS, not bytes — see the search leg.
                body.chars().take(300).collect::<String>()
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

    // -----------------------------------------------------------------------
    // Two-phase lookup against a loopback stub (POE-202)
    // -----------------------------------------------------------------------
    //
    // `base_url` is a field for exactly this: the queue's observable behaviour
    // — the event sequence, both cancel checkpoints, the shaped result — only
    // exists on the far side of two HTTP calls, and a test that reached
    // pathofexile.com would be neither deterministic nor allowed.

    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A loopback HTTP/1.1 server answering the two trade endpoints with canned
    /// JSON, and recording every request it was sent.
    struct StubApi {
        base_url: String,
        requests: Arc<Mutex<Vec<String>>>,
    }

    impl StubApi {
        /// Every request received so far, head and body, oldest first.
        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }
    }

    /// Serve `search` to `/api/trade/search/...` and `fetch` to
    /// `/api/trade/fetch/...`, for as many requests as arrive.
    async fn stub_api(search: serde_json::Value, fetch: serde_json::Value) -> StubApi {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback bind");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let request = read_request(&mut sock).await;
                let body = if request.contains("/api/trade/search/") {
                    search.clone()
                } else {
                    fetch.clone()
                };
                seen.lock().unwrap_or_else(|e| e.into_inner()).push(request);
                let payload = serde_json::to_string(&body).unwrap();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    payload.len(),
                    payload,
                );
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.shutdown().await;
            }
        });
        StubApi { base_url, requests }
    }

    /// Read one request: headers, then as many body bytes as `content-length`
    /// announced. Answering before the body is in would race the client's
    /// write.
    async fn read_request(sock: &mut tokio::net::TcpStream) -> String {
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 2048];
        loop {
            let Ok(n) = sock.read(&mut chunk).await else {
                break;
            };
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf).to_string();
            let Some(head_end) = text.find("\r\n\r\n") else {
                continue;
            };
            let len = text[..head_end]
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("content-length:"))
                .and_then(|line| line.split(':').nth(1))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if buf.len() >= head_end + 4 + len {
                break;
            }
        }
        String::from_utf8_lossy(&buf).to_string()
    }

    /// A loopback server answering every request with one canned status line
    /// and body — the failing counterpart to [`stub_api`], which always
    /// answers 200.
    async fn stub_api_status(status: u16, reason: &str, body: impl Into<String>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback bind");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let head = format!("HTTP/1.1 {status} {reason}");
        let body = body.into();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                read_request(&mut sock).await;
                let response = format!(
                    "{head}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body,
                );
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.shutdown().await;
            }
        });
        base_url
    }

    /// The whole point of [`is_client_error`]: a rejection GGG actually sent,
    /// read back through the message the client actually formats. The two ends
    /// are a `format!` and a parser, so only a test that drives a real 400
    /// through both can say they still agree.
    #[tokio::test]
    async fn a_search_ggg_rejects_reads_back_as_a_client_error() {
        let base_url =
            stub_api_status(400, "Bad Request", r#"{"error":{"message":"Query is too complex"}}"#)
                .await;
        let mut client = TradeApiClient::new();
        client.base_url = base_url;
        client.set_league("Allflame".to_string());

        let error = client
            .lookup_query(TradeSource::Mercenary, "merc", serde_json::json!({}), |_| {})
            .await
            .expect_err("the stub rejects the search");

        assert!(error.contains("Query is too complex"), "got {error}");
        assert!(
            is_client_error(&error),
            "a 400 the caller cannot recognise is a 400 it will retry: {error}",
        );
    }

    /// The error body is cut to 300 CHARACTERS, not 300 bytes. GGG's bodies are
    /// UTF-8, and `&body[..300]` panics whenever byte 300 lands inside a
    /// codepoint — taking down the lookup task rather than returning the error
    /// it was formatting. The stub puts a two-byte `é` across exactly that
    /// boundary.
    #[tokio::test]
    async fn a_rejection_body_is_cut_on_a_character_boundary() {
        let body = format!("{}é tail", "x".repeat(299));
        let base_url = stub_api_status(400, "Bad Request", body).await;
        let mut client = TradeApiClient::new();
        client.base_url = base_url;
        client.set_league("Allflame".to_string());

        let error = client
            .lookup_query(TradeSource::Mercenary, "merc", serde_json::json!({}), |_| {})
            .await
            .expect_err("the stub rejects the search");

        assert!(error.ends_with(&format!("{}é", "x".repeat(299))), "got {error}");
        assert!(!error.contains("tail"), "the body is still cut at 300: {error}");
    }

    /// A 5xx is the server failing, not the request being wrong, so it stays
    /// retryable — the discriminator callers branch on. Driven through the stub
    /// for the same reason the 400 is: a hand-written message proves the parser
    /// and nothing about what this client emits.
    #[tokio::test]
    async fn a_search_the_server_failed_to_serve_reads_back_as_retryable() {
        let base_url = stub_api_status(503, "Service Unavailable", "upstream unavailable").await;
        let mut client = TradeApiClient::new();
        client.base_url = base_url;
        client.set_league("Allflame".to_string());

        let error = client
            .lookup_query(TradeSource::Mercenary, "merc", serde_json::json!({}), |_| {})
            .await
            .expect_err("the stub fails the search");

        assert_eq!(error_status(&error), Some(503), "got {error}");
        assert!(
            !is_client_error(&error),
            "a 503 read as a rejection is a retry the caller never makes: {error}",
        );
    }

    /// 429 is deliberately counted with the rest of the 4xx range: GGG's docs
    /// put it inside the invalid-request threshold
    /// (`docs/RESEARCH-poe-trade-api.md:269-274`), so answering one with an
    /// immediate retry is the behaviour that earns a ban. Driven through the
    /// stub so the message this client actually writes is the one parsed.
    #[tokio::test]
    async fn a_rate_limited_search_reads_back_as_a_client_error() {
        let base_url = stub_api_status(429, "Too Many Requests", "").await;
        let mut client = TradeApiClient::new();
        client.base_url = base_url;
        client.set_league("Allflame".to_string());

        let error = client
            .lookup_query(TradeSource::Mercenary, "merc", serde_json::json!({}), |_| {})
            .await
            .expect_err("the stub rate-limits the search");

        assert!(
            is_client_error(&error),
            "a 429 read as transient is the retry that earns the ban: {error}",
        );
        assert!(
            !error.contains("Try again"),
            "the message must not promise a retry the client does not make: {error}",
        );
    }

    /// A transport failure has no status, and reqwest's message carries a
    /// parenthesised URL — the shape most likely to fool a marker reader into
    /// inventing one.
    #[test]
    fn a_transport_failure_carries_no_status() {
        let error = "Trade search request failed: error sending request for url \
             (http://127.0.0.1:9/api/trade/search/Allflame)";
        assert_eq!(error_status(error), None);
        assert_eq!(error_status(CANCELLED), None);
    }

    /// A client pointed at the stub, with the league already resolved.
    fn stub_client(stub: &StubApi) -> Arc<TradeApiClient> {
        let mut client = TradeApiClient::new();
        client.base_url = stub.base_url.clone();
        client.set_league("Allflame".to_string());
        Arc::new(client)
    }

    fn search_ok() -> serde_json::Value {
        serde_json::json!({"id": "qid-1", "result": ["r1"], "total": 42})
    }

    fn fetch_ok() -> serde_json::Value {
        serde_json::json!({"result": [{
            "listing": {
                "indexed": "2026-08-26T10:00:00Z",
                "account": {"name": "Seller"},
                "price": {"amount": 12.5, "currency": "chaos"},
            },
            "item": {
                "corrupted": true,
                "properties": [
                    {"name": "Level", "values": [["20", 0]]},
                    {"name": "Quality", "values": [["+23%", 0]]},
                ],
            },
        }]})
    }

    /// The `kind` of every event, in order — the wire tag the two webview
    /// consumers switch on.
    fn kinds(events: &[TradeQueueEvent]) -> Vec<String> {
        events
            .iter()
            .map(|e| serde_json::to_value(e).unwrap()["kind"].as_str().unwrap().to_string())
            .collect()
    }

    /// Wait until `want` lookups have joined the queue, so a cancel lands on
    /// parked lookups rather than on an empty queue.
    async fn park_until(client: &TradeApiClient, want: usize) {
        for _ in 0..10_000 {
            if client.pending() >= want {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("only {} of {want} lookups reached the queue", client.pending());
    }

    /// Gem-path regression (POE-202 moved the queue body into `lookup_query`):
    /// the sequence a Comparator renders its progress from is still
    /// queued → fetching → done, all tagged `gem`. A `waiting` may not appear
    /// here — the rate limiter of a fresh client owes no wait.
    #[tokio::test]
    async fn a_gem_lookup_still_reports_queued_then_fetching_then_done() {
        let stub = stub_api(search_ok(), fetch_ok()).await;
        let client = stub_client(&stub);
        let events = Mutex::new(Vec::new());

        client
            .lookup_gem_with_mode("Empower Support", "20/20", 0.0, false, |e| {
                events.lock().unwrap().push(e)
            })
            .await
            .expect("the stub answers both phases");

        let events = events.into_inner().unwrap();
        assert_eq!(kinds(&events), ["queued", "fetching", "done"]);
        assert!(
            events
                .iter()
                .all(|e| serde_json::to_value(e).unwrap()["source"] == "gem"),
            "every gem-lookup event must be tagged gem: {events:?}",
        );
    }

    /// Gem-path regression: `lookup_query` returns raw JSON entries and the gem
    /// path parses them, so the level/quality/corrupted decoding must survive
    /// the refactor along with the identity fields `build_result` stamps on.
    #[tokio::test]
    async fn a_gem_lookup_still_returns_the_parsed_listing_and_its_identity() {
        let stub = stub_api(search_ok(), fetch_ok()).await;
        let client = stub_client(&stub);

        let result = client
            .lookup_gem_with_mode("Empower Support", "20/20", 0.0, false, |_| {})
            .await
            .expect("the stub answers both phases");

        assert_eq!(result.gem, "Empower Support");
        assert_eq!(result.variant, "20/20");
        assert_eq!(result.total, 42);
        assert_eq!(result.trade_url, "https://www.pathofexile.com/trade/search/Allflame/qid-1");
        assert_eq!(result.listings.len(), 1);
        let listing = &result.listings[0];
        assert_eq!(listing.price, 12.5);
        assert_eq!(listing.currency, "chaos");
        assert_eq!(listing.account, "Seller");
        assert_eq!(listing.gem_level, 20);
        assert_eq!(listing.gem_quality, 23);
        assert!(listing.corrupted);
    }

    /// The gem query still reaches GGG: the body the gem builder produced is
    /// POSTed to the resolved league's search endpoint. Guards the refactor's
    /// one silent failure mode — a `lookup_query` that runs the queue but
    /// forwards the wrong body.
    #[tokio::test]
    async fn a_gem_lookup_posts_its_built_query_to_the_resolved_leagues_endpoint() {
        let stub = stub_api(search_ok(), fetch_ok()).await;
        let client = stub_client(&stub);

        client
            .lookup_gem_with_mode("Empower Support", "20/20", 0.0, false, |_| {})
            .await
            .expect("the stub answers both phases");

        let requests = stub.requests();
        assert_eq!(requests.len(), 2, "one search and one fetch: {requests:?}");
        assert!(
            requests[0].starts_with("POST /api/trade/search/Allflame "),
            "search must be POSTed to the resolved league: {}",
            requests[0],
        );
        assert!(
            requests[0].contains("Empower Support"),
            "the gem's own query body must be the one sent: {}",
            requests[0],
        );
        assert!(
            requests[1].starts_with("GET /api/trade/fetch/r1?query=qid-1 "),
            "the fetch must carry the search's ids and query id: {}",
            requests[1],
        );
    }

    /// Per-consumer cancel (POE-202): a mercenary retire cancels the merc
    /// queue. A gem lookup parked behind it must still run — before the
    /// per-source split, one `cancel` emptied the Comparator's queue too.
    #[tokio::test]
    async fn cancelling_the_mercenary_queue_leaves_a_queued_gem_lookup_running() {
        let stub = stub_api(search_ok(), fetch_ok()).await;
        let client = stub_client(&stub);
        let held = client.lookup_mutex.lock().await;

        let merc_client = client.clone();
        let merc = tokio::spawn(async move {
            merc_client
                .lookup_query(TradeSource::Mercenary, "Kaom", serde_json::json!({}), |_| {})
                .await
        });
        let gem_client = client.clone();
        let gem = tokio::spawn(async move {
            gem_client
                .lookup_query(TradeSource::Gem, "Spark", serde_json::json!({}), |_| {})
                .await
        });
        park_until(&client, 2).await;

        client.cancel(TradeSource::Mercenary);
        drop(held);

        assert_eq!(
            merc.await.unwrap().unwrap_err(),
            CANCELLED,
            "the cancelled source must bail out",
        );
        let gem = gem.await.unwrap().expect("the gem lookup must survive a mercenary cancel");
        assert_eq!(gem.total, 42);
    }

    /// The mirror, and the drain case with it: the mercenary lookup is queued
    /// BEHIND the gem one, so the gem lookup runs to completion and drains the
    /// queue in between the cancel and the mercenary's checkpoint. The cancel
    /// must still be waiting for it there.
    #[tokio::test]
    async fn a_mercenary_cancel_outlives_a_gem_lookup_draining_the_queue() {
        let stub = stub_api(search_ok(), fetch_ok()).await;
        let client = stub_client(&stub);
        let held = client.lookup_mutex.lock().await;

        let gem_client = client.clone();
        let gem = tokio::spawn(async move {
            gem_client
                .lookup_query(TradeSource::Gem, "Spark", serde_json::json!({}), |_| {})
                .await
        });
        let merc_client = client.clone();
        let merc = tokio::spawn(async move {
            merc_client
                .lookup_query(TradeSource::Mercenary, "Kaom", serde_json::json!({}), |_| {})
                .await
        });
        park_until(&client, 2).await;

        client.cancel(TradeSource::Mercenary);
        drop(held);

        let gem = gem.await.unwrap().expect("the gem lookup must not see the mercenary cancel");
        assert_eq!(gem.total, 42);
        assert_eq!(
            merc.await.unwrap().unwrap_err(),
            CANCELLED,
            "a completed gem lookup must not un-cancel the mercenary queue",
        );
    }

    /// `cancel` reports how many of ITS OWN source's lookups were pending —
    /// the number `trade_cancel` shows the user. The shared `pending_count`
    /// would say two here.
    #[tokio::test]
    async fn cancel_reports_only_its_own_sources_pending_lookups() {
        let stub = stub_api(search_ok(), fetch_ok()).await;
        let client = stub_client(&stub);
        let held = client.lookup_mutex.lock().await;

        let merc_client = client.clone();
        let merc = tokio::spawn(async move {
            merc_client
                .lookup_query(TradeSource::Mercenary, "Kaom", serde_json::json!({}), |_| {})
                .await
        });
        let gem_client = client.clone();
        let gem = tokio::spawn(async move {
            gem_client
                .lookup_query(TradeSource::Gem, "Spark", serde_json::json!({}), |_| {})
                .await
        });
        park_until(&client, 2).await;

        assert_eq!(client.cancel(TradeSource::Mercenary), 1);
        assert_eq!(client.cancel(TradeSource::Gem), 1);

        drop(held);
        let _ = merc.await;
        let _ = gem.await;
    }

    /// The cancel epoch must not latch: a cancel over an empty queue is about
    /// lookups that no longer exist, and the next lookup — a re-detect of the
    /// same recruit window seconds later — has to run.
    #[tokio::test]
    async fn a_lookup_enqueued_after_its_source_was_cancelled_still_runs() {
        let stub = stub_api(search_ok(), fetch_ok()).await;
        let client = stub_client(&stub);

        assert_eq!(client.cancel(TradeSource::Mercenary), 0, "nothing is pending yet");

        let result = client
            .lookup_query(TradeSource::Mercenary, "Kaom", serde_json::json!({}), |_| {})
            .await
            .expect("a cancel with nothing pending must not stop the next lookup");

        assert_eq!(result.total, 42);
    }

    /// The mercenary path fails closed on an unresolved league exactly like the
    /// gem path, and its Error event is tagged `mercenary` — a merc failure
    /// rendered in the Comparator is the bug the discriminator exists to stop.
    #[tokio::test]
    async fn lookup_query_fails_closed_on_an_unset_league_without_reaching_the_api() {
        let stub = stub_api(search_ok(), fetch_ok()).await;
        let mut client = TradeApiClient::new();
        client.base_url = stub.base_url.clone();
        let events = Mutex::new(Vec::new());

        let result = client
            .lookup_query(TradeSource::Mercenary, "Kaom", serde_json::json!({}), |e| {
                events.lock().unwrap().push(e)
            })
            .await;

        assert!(result.is_err(), "an unresolved league must not be guessed at");
        assert!(stub.requests().is_empty(), "nothing may be sent: {:?}", stub.requests());
        let events = events.into_inner().unwrap();
        assert_eq!(kinds(&events), ["error"]);
        assert_eq!(serde_json::to_value(&events[0]).unwrap()["source"], "mercenary");
    }
}
