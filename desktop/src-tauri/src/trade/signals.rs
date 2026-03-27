use chrono::Utc;
use std::collections::HashSet;

use super::types::{TradeListingDetail, TradeLookupResult, TradeSignals};

/// Normalize a listing price to chaos using the divine exchange rate.
pub fn normalize_to_chaos(price: f64, currency: &str, divine_rate: f64) -> f64 {
    if currency == "divine" && divine_rate > 0.0 {
        (price * divine_rate * 10.0).round() / 10.0
    } else {
        price
    }
}

/// Assemble a TradeLookupResult from raw search + fetch data.
/// Mirrors Go's BuildResult in types.go.
pub fn build_result(
    gem: &str,
    variant: &str,
    league: &str,
    query_id: &str,
    total: i32,
    mut listings: Vec<TradeListingDetail>,
    divine_rate: f64,
) -> TradeLookupResult {
    for l in &mut listings {
        l.chaos_price = normalize_to_chaos(l.price, &l.currency, divine_rate);
    }

    listings.sort_by(|a, b| a.chaos_price.partial_cmp(&b.chaos_price).unwrap());

    let trade_url = if !query_id.is_empty() && !league.is_empty() {
        format!(
            "https://www.pathofexile.com/trade/search/{}/{}",
            league, query_id
        )
    } else {
        String::new()
    };

    let (price_floor, price_ceiling, price_spread, median) = if !listings.is_empty() {
        let floor = listings[0].chaos_price;
        let ceiling = listings
            .iter()
            .map(|l| l.chaos_price)
            .fold(0.0f64, f64::max);
        (floor, ceiling, ceiling - floor, median_chaos_price(&listings))
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    TradeLookupResult {
        gem: gem.to_string(),
        variant: variant.to_string(),
        total,
        price_floor,
        price_ceiling,
        price_spread,
        median_top10: median,
        listings,
        signals: TradeSignals::default(),
        divine_price: divine_rate,
        trade_url,
        fetched_at: Utc::now(),
    }
}

/// Compute market health signals from the top listings.
/// Mirrors Go's ComputeSignals in types.go.
pub fn compute_signals(listings: &[TradeListingDetail]) -> TradeSignals {
    if listings.is_empty() {
        return TradeSignals::default();
    }

    let mut seen = HashSet::new();
    for l in listings {
        seen.insert(l.account.as_str());
    }
    let unique = seen.len() as i32;

    let concentration = match unique {
        u if u >= 8 => "NORMAL",
        u if u >= 5 => "CONCENTRATED",
        _ => "MONOPOLY",
    }
    .to_string();

    let age = Utc::now() - listings[0].indexed_at;
    let staleness = match age.num_hours() {
        h if h < 1 => "FRESH",
        h if h < 6 => "AGING",
        _ => "STALE",
    }
    .to_string();

    let median = median_chaos_price(listings);
    let outlier = listings[0].chaos_price < median * 0.5;

    TradeSignals {
        seller_concentration: concentration,
        cheapest_staleness: staleness,
        price_outlier: outlier,
        unique_accounts: unique,
    }
}

fn median_chaos_price(listings: &[TradeListingDetail]) -> f64 {
    if listings.is_empty() {
        return 0.0;
    }
    let mut prices: Vec<f64> = listings.iter().map(|l| l.chaos_price).collect();
    prices.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = prices.len();
    if n % 2 == 0 {
        ((prices[n / 2 - 1] + prices[n / 2]) / 2.0 * 100.0).round() / 100.0
    } else {
        prices[n / 2]
    }
}

impl Default for TradeSignals {
    fn default() -> Self {
        Self {
            seller_concentration: "NORMAL".to_string(),
            cheapest_staleness: "FRESH".to_string(),
            price_outlier: false,
            unique_accounts: 0,
        }
    }
}
