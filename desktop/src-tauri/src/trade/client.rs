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
use serde::Deserialize;
use std::time::Duration;

use super::query::build_search_query;
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
// Trade API client
// ---------------------------------------------------------------------------

/// HTTP client for the GGG Path of Exile trade API.
///
/// Each desktop app instance has its own client = own IP = own rate limits.
/// The rate limiter maintains separate "search" and "fetch" pools with
/// multi-tier sliding windows, synced from GGG's X-Rate-Limit-* headers.
pub struct TradeApiClient {
    http_client: reqwest::Client,
    league_name: String,
    rate_limiter: TradeRateLimiter,
}

impl TradeApiClient {
    pub fn new(league_name: &str) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(TRADE_CLIENT_TIMEOUT)
            .user_agent(BROWSER_USER_AGENT)
            .build()
            .expect("failed to build trade HTTP client");

        Self {
            http_client,
            league_name: league_name.to_string(),
            rate_limiter: TradeRateLimiter::new(),
        }
    }

    /// Full trade lookup: rate-limit → search → rate-limit → fetch → build result.
    ///
    /// `divine_chaos_rate`: divine→chaos exchange rate for price normalization.
    /// Pass 0.0 to skip normalization (listings keep raw currency values).
    pub async fn lookup_gem(
        &self,
        gem_name: &str,
        variant: &str,
        divine_chaos_rate: f64,
    ) -> Result<TradeLookupResult, String> {
        // Phase 1: Search
        self.rate_limiter.wait_for_capacity("search").await;
        let search_response = self.execute_search(gem_name, variant).await?;

        if search_response.ids.is_empty() {
            return Ok(build_result(
                gem_name,
                variant,
                &self.league_name,
                &search_response.query_id,
                search_response.total,
                vec![],
                divine_chaos_rate,
            ));
        }

        // Phase 2: Fetch top 10
        self.rate_limiter.wait_for_capacity("fetch").await;
        let listings = self
            .fetch_listing_details(&search_response.query_id, &search_response.ids)
            .await?;

        Ok(build_result(
            gem_name,
            variant,
            &self.league_name,
            &search_response.query_id,
            search_response.total,
            listings,
            divine_chaos_rate,
        ))
    }

    /// POST /api/trade/search/{league}
    async fn execute_search(
        &self,
        gem_name: &str,
        variant: &str,
    ) -> Result<SearchResponse, String> {
        let query_body = build_search_query(gem_name, variant);
        let url = format!(
            "{}/api/trade/search/{}",
            TRADE_API_BASE_URL, self.league_name
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
