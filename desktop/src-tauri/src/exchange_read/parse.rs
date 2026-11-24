//! Turns the OCR text of the exchange's top Competing Trades row and its two
//! item-name boxes into one Companion selection, or one reader error.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Currency {
    Chaos,
    Div,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OneSide {
    Left,
    Right,
}

/// The Companion inputs one top row fills, plus the row as read for the status.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeSelection {
    pub mode: Mode,
    pub currency: Currency,
    pub ratio: f64,
    pub one_side: OneSide,
    pub left: f64,
    pub right: f64,
    pub stock: u64,
    pub want_name: String,
    pub have_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExchangeReadError {
    RatioUnreadable,
    NamesUnreadable,
    NoCurrencySide,
}

/// `want_lines` and `have_lines` are the OCR lines of the I want and I have
/// name boxes, top to bottom.
pub fn parse_top_row(
    _ratio: &str,
    _stock: &str,
    _want_lines: &[&str],
    _have_lines: &[&str],
) -> Result<ExchangeSelection, ExchangeReadError> {
    Err(ExchangeReadError::RatioUnreadable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Deserialize)]
    struct Fixture {
        rows: HashMap<String, Row>,
    }

    #[derive(Deserialize)]
    struct Row {
        ratio: String,
        stock: String,
        want: Vec<String>,
        have: Vec<String>,
    }

    fn parse_fixture_row(id: &str) -> Result<ExchangeSelection, ExchangeReadError> {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/exchange/rows.json")).unwrap();
        let row = fixture.rows.get(id).unwrap_or_else(|| panic!("rows.json has no row {id}"));
        let want: Vec<&str> = row.want.iter().map(String::as_str).collect();
        let have: Vec<&str> = row.have.iter().map(String::as_str).collect();
        parse_top_row(&row.ratio, &row.stock, &want, &have)
    }

    // keep:happy
    mod keep_happy {
        use super::*;

        #[test]
        fn an_ember_held_for_chaos_wanted_at_130_to_1_selects_selling_for_chaos_with_the_one_on_the_right() {
            let selection = parse_fixture_row("ember-for-chaos").expect("the measured top row parses");
            assert_eq!(
                serde_json::to_value(&selection).unwrap(),
                serde_json::json!({
                    "mode": "sell",
                    "currency": "chaos",
                    "ratio": 130.0,
                    "oneSide": "right",
                    "left": 130.0,
                    "right": 1.0,
                    "stock": 1,
                    "wantName": "Chaos Orb",
                    "haveName": "Allflame Ember of the Ethereal",
                })
            );
        }

        #[test]
        fn a_stock_read_as_two_words_is_one_thousands_grouped_number() {
            assert_eq!(parse_fixture_row("ember-for-chaos-split-stock").map(|s| s.stock), Ok(8125));
        }
    }

    mod keep_critical {
        use super::*;

        // keep:critical — silent: a `1 : b` row filled as `b : 1`, or mode taken
        // from the wrong side, prices the order upside down and the Companion trusts it.
        #[test]
        fn a_one_on_the_left_row_with_divine_held_selects_buying_with_divine_at_the_right_value() {
            assert_eq!(
                parse_fixture_row("divine-for-ember-one-on-left"),
                Ok(ExchangeSelection {
                    mode: Mode::Buy,
                    currency: Currency::Div,
                    ratio: 6.12,
                    one_side: OneSide::Left,
                    left: 1.0,
                    right: 6.12,
                    stock: 3,
                    want_name: "Allflame Ember of the Ethereal".into(),
                    have_name: "Divine Orb".into(),
                })
            );
        }

        // keep:critical — silent: a refused row that still yields a selection (a
        // stripped `<`/`>` prefix, a guessed ratio, a guessed side) fills a plausible
        // wrong trade into the Companion; each must yield its error code and nothing else.
        #[test]
        fn an_unreadable_or_one_sided_top_row_is_refused_with_its_event_code() {
            let cases = [
                ("outbid-prefixed-top-row", "ratio-unreadable"),
                ("undercut-prefixed-top-row", "ratio-unreadable"),
                ("ratio-missing-a-side", "ratio-unreadable"),
                ("ratio-misread-letters", "ratio-unreadable"),
                ("ratio-empty", "ratio-unreadable"),
                ("ratio-neither-side-one", "ratio-unreadable"),
                ("ratio-zero-side", "ratio-unreadable"),
                ("want-name-missing", "names-unreadable"),
                ("have-name-blank", "names-unreadable"),
                ("neither-side-currency", "no-currency-side"),
            ];
            let mismatches: Vec<String> = cases
                .iter()
                .filter_map(|(id, code)| {
                    let got = parse_fixture_row(id).map_err(|e| serde_json::to_value(e).unwrap());
                    (got != Err(serde_json::json!(code))).then(|| format!("{id}: expected Err({code}), got {got:?}"))
                })
                .collect();
            assert!(mismatches.is_empty(), "{mismatches:#?}");
        }
    }
}
