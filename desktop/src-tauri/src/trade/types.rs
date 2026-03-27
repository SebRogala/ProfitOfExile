use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// TradeLookupResult — mirrors Go's trade.TradeLookupResult exactly
/// so the frontend gets identical JSON shape from both server and desktop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeLookupResult {
    pub gem: String,
    pub variant: String,
    pub total: i32,
    pub price_floor: f64,
    pub price_ceiling: f64,
    pub price_spread: f64,
    pub median_top10: f64,
    pub listings: Vec<TradeListingDetail>,
    pub signals: TradeSignals,
    pub divine_price: f64,
    pub trade_url: String,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeListingDetail {
    pub price: f64,
    pub currency: String,
    pub chaos_price: f64,
    pub account: String,
    pub indexed_at: DateTime<Utc>,
    pub gem_level: i32,
    pub gem_quality: i32,
    pub corrupted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeSignals {
    pub seller_concentration: String,
    pub cheapest_staleness: String,
    pub price_outlier: bool,
    pub unique_accounts: i32,
}

/// SearchResponse holds parsed GGG trade search results.
pub struct SearchResponse {
    pub query_id: String,
    pub ids: Vec<String>,
    pub total: i32,
}
