const TRADE_SEARCH_BASE: &str = "https://www.pathofexile.com/trade/search/";

/// A league as one URL path segment, escaped like Go's `url.PathEscape`.
fn league_path_segment(league: &str) -> String {
    let mut out = String::with_capacity(league.len());
    for &b in league.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~$&+:=@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Trade-site search URL prefix for a league, escaped like Go's `url.PathEscape`.
pub fn trade_search_url(league: &str) -> String {
    format!("{}{}", TRADE_SEARCH_BASE, league_path_segment(league))
}

/// Trade API search endpoint for a league on the given API origin.
pub fn trade_api_search_url(base_url: &str, league: &str) -> String {
    format!("{}/api/trade/search/{}", base_url, league_path_segment(league))
}

#[cfg(test)]
mod keep_happy {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct UrlVector {
        league: String,
        api_search_path: String,
    }

    #[test]
    fn api_search_url_escapes_league_like_go() {
        // Shared with the Go API search URL test in internal/trade.
        let vectors: Vec<UrlVector> = serde_json::from_str(include_str!(
            "../../../../internal/trade/testdata/trade-url-vectors.json"
        ))
        .unwrap();
        assert!(!vectors.is_empty());
        for v in &vectors {
            assert_eq!(
                trade_api_search_url("http://stub", &v.league),
                format!("http://stub{}", v.api_search_path)
            );
        }
    }
}
