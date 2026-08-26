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

/// One mercenary trade listing.
///
/// Deliberately NOT `TradeListingDetail`: that type mirrors Go's exactly (see
/// the invariant on `TradeLookupResult` above) and is gem-shaped down to
/// `gem_level`/`gem_quality`. A mercenary listing is desktop-only and prices a
/// mercenary, so it gets its own type rather than widening a mirrored one
/// (POE-202).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercTradeListing {
    /// Seller price normalized to chaos — EXCEPT on the mercenary path, which
    /// has no divine rate on the Rust side and so normalizes with a rate of 0,
    /// leaving this equal to [`Self::amount`]. Named for the field it mirrors
    /// on the gem listing; read [`Self::amount`] with [`Self::currency`] when
    /// the number has to mean a price.
    pub chaos_price: f64,
    /// Raw seller currency, kept because the Mercenaries page has no divine
    /// rate and shows what the seller actually asked for.
    pub currency: String,
    /// Raw seller amount in `currency`.
    pub amount: f64,
    pub account: String,
    /// ISO-8601 timestamp as GGG returned it.
    pub indexed_at: String,
}

/// The result of one auto-search for a captured mercenary (POE-202).
///
/// `query_hash` is the capture identity a late result is discarded by: a
/// result whose hash no longer matches the slice's is answering a question
/// the capture has already moved on from.
///
/// `PartialEq` because the merc SSOT slice carries it and `run::publish`
/// emits only on a real change — an equality that stopped being structural
/// would turn every liveness tick into an `ssot-changed` fan-out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercTradeResult {
    pub query_hash: String,
    pub league: String,
    pub total: u32,
    pub listings: Vec<MercTradeListing>,
    /// Cheapest and middle `chaos_price` of [`Self::listings`] — statistics
    /// over raw seller amounts, not a value floor and not a value median, for
    /// the reason given on [`MercTradeListing::chaos_price`].
    pub floor_chaos: f64,
    pub median_chaos: f64,
    pub fetched_at_ms: u64,
    /// Set when the 35-filter cap forced the query to drop tier loosening or
    /// support cells — the listings answer a LOOSER question than the capture.
    pub truncated: bool,
}

/// SearchResponse holds parsed GGG trade search results.
#[derive(Debug, Clone)]
pub struct SearchResponse {
    pub query_id: String,
    pub ids: Vec<String>,
    pub total: i32,
}
