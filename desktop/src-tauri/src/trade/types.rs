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
    pub seller_concentration: SellerConcentration,
    pub cheapest_staleness: CheapestStaleness,
    pub price_outlier: bool,
    pub unique_accounts: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SellerConcentration {
    Normal,
    Concentrated,
    Monopoly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheapestStaleness {
    Fresh,
    Aging,
    Stale,
}

impl Default for TradeSignals {
    fn default() -> Self {
        Self {
            seller_concentration: SellerConcentration::Normal,
            cheapest_staleness: CheapestStaleness::Fresh,
            price_outlier: false,
            unique_accounts: 0,
        }
    }
}

/// SearchResponse holds parsed GGG trade search results.
#[derive(Debug, Clone)]
pub struct SearchResponse {
    pub query_id: String,
    pub ids: Vec<String>,
    pub total: i32,
}
