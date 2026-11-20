const TRADE_SEARCH_BASE: &str = "https://www.pathofexile.com/trade/search/";

/// Trade-site search URL prefix for a league, escaped like Go's `url.PathEscape`.
pub fn trade_search_url(league: &str) -> String {
    let mut out = String::from(TRADE_SEARCH_BASE);
    for &b in league.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~$&+:=@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}
