use chrono::Utc;
use std::collections::HashSet;

use super::types::{CheapestStaleness, SellerConcentration, TradeListingDetail, TradeLookupResult, TradeSignals};

// Trade price definition: docs/TRADE-LIFECYCLE.md#trade-price-definition.
const TRADE_PRICE_LISTING_COUNT: usize = 3;

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

    listings.sort_by(|a, b| a.chaos_price.total_cmp(&b.chaos_price));

    let trade_url = if !query_id.is_empty() && !league.is_empty() {
        format!(
            "https://www.pathofexile.com/trade/search/{}/{}",
            league, query_id
        )
    } else {
        String::new()
    };

    let (price_floor, price_ceiling, price_spread, median) = if !listings.is_empty() {
        let chaos: Vec<f64> = listings.iter().map(|l| l.chaos_price).collect();
        let floor = floor_chaos_price(&chaos);
        let ceiling = chaos.iter().copied().fold(0.0f64, f64::max);
        (floor, ceiling, ceiling - floor, median_chaos_price(&chaos))
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    let cheapest: Vec<f64> = listings.iter()
        .take(TRADE_PRICE_LISTING_COUNT)
        .map(|l| l.chaos_price)
        .collect();
    let median_top3 = median_chaos_price(&cheapest);
    let signals = compute_signals(&listings);

    TradeLookupResult {
        gem: gem.to_string(),
        variant: variant.to_string(),
        total,
        price_floor,
        price_ceiling,
        price_spread,
        median_top10: median,
        median_top3,
        listings,
        signals,
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
        u if u >= 8 => SellerConcentration::Normal,
        u if u >= 5 => SellerConcentration::Concentrated,
        _ => SellerConcentration::Monopoly,
    };

    let age = Utc::now() - listings[0].indexed_at;
    let staleness = match age.num_hours() {
        h if h < 1 => CheapestStaleness::Fresh,
        h if h < 6 => CheapestStaleness::Aging,
        _ => CheapestStaleness::Stale,
    };

    let chaos: Vec<f64> = listings.iter().map(|l| l.chaos_price).collect();
    let median = median_chaos_price(&chaos);
    let outlier = listings[0].chaos_price < median * 0.5;

    TradeSignals {
        seller_concentration: concentration,
        cheapest_staleness: staleness,
        price_outlier: outlier,
        unique_accounts: unique,
    }
}

/// Median of a set of chaos prices. Takes bare prices rather than listings so
/// the mercenary result path can reuse it without a gem-shaped listing type
/// (POE-202).
pub(crate) fn median_chaos_price(prices: &[f64]) -> f64 {
    if prices.is_empty() {
        return 0.0;
    }
    let mut prices: Vec<f64> = prices.to_vec();
    prices.sort_by(|a, b| a.total_cmp(b));
    let n = prices.len();
    if n % 2 == 0 {
        ((prices[n / 2 - 1] + prices[n / 2]) / 2.0 * 100.0).round() / 100.0
    } else {
        prices[n / 2]
    }
}

/// Cheapest of a set of chaos prices, or 0.0 when there are none.
///
/// Ordered with `total_cmp`, the same total order the listing sort in
/// `build_result` uses, so the floor is the first row of the rendered table
/// whether the caller pre-sorted or not.
pub(crate) fn floor_chaos_price(prices: &[f64]) -> f64 {
    prices
        .iter()
        .copied()
        .min_by(|a, b| a.total_cmp(b))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PriceVector {
        name: String,
        divine_rate: f64,
        listings: Vec<RawPrice>,
        median_top3: f64,
        median_top10: f64,
    }

    #[derive(Deserialize)]
    struct RawPrice {
        price: f64,
        currency: String,
    }

    fn vectors() -> Vec<PriceVector> {
        // Shared with TestBuildResult_TradePriceVectors in internal/trade/types_test.go.
        serde_json::from_str(include_str!(
            "../../../../internal/trade/testdata/trade-price-vectors.json"
        )).unwrap()
    }

    fn result(vector: &PriceVector) -> TradeLookupResult {
        let listings = vector.listings.iter().map(|raw| TradeListingDetail {
            price: raw.price,
            currency: raw.currency.clone(),
            chaos_price: 0.0,
            account: "seller".to_string(),
            indexed_at: Utc::now(),
            gem_level: 20,
            gem_quality: 20,
            corrupted: false,
        }).collect();
        build_result("Gem", "20/20", "Test League", "query", 10, listings, vector.divine_rate)
    }

    #[test]
    fn built_trade_price_matches_shared_vectors() {
        for vector in vectors() {
            assert_eq!(result(&vector).median_top3, vector.median_top3, "{}", vector.name);
        }
    }

    #[test]
    fn existing_median_still_uses_all_fetched_listings() {
        for vector in vectors() {
            assert_eq!(result(&vector).median_top10, vector.median_top10, "{}", vector.name);
        }
    }

    #[test]
    fn built_trade_price_serializes_for_the_widget() {
        for vector in vectors() {
            let json = serde_json::to_value(result(&vector)).unwrap();
            assert_eq!(json["medianTop3"], vector.median_top3, "{}", vector.name);
        }
    }
}
