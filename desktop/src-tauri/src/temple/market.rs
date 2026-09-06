//! The market read a room valuation is computed against (POE-257).
//!
//! POE-255 serves one aggregated, league-keyed object at
//! `GET /api/analysis/temple-market`: every room-tier's sale delta, every
//! recipe member's price, and the recipe table that ties them together. This
//! module is the desktop's mirror of that payload and nothing else.
//!
//! **It does no HTTP.** POE-258 owns the poll and hands the parsed
//! [`MarketInput`] to the temple slice, because `docs/TEMPLE-LIFECYCLE.md`
//! forbids network work on the 650 ms tick. Everything here is pure, so it runs
//! in the Linux test container.
//!
//! # The wire is mirrored, not re-derived
//!
//! The private `Wire*` structs below are a field-for-field copy of
//! `internal/temple/market.go`'s served shape (camelCase, unknown fields
//! ignored, `price: null` meaning unpriced). [`MarketInput`] is the same data
//! re-keyed for lookup: rooms by `(poe.ninja room name, tier)`, items by name.
//! Nothing is filtered on the way in and nothing is defaulted to zero — an item
//! the feed could not price lands in [`MarketInput::unpriced`], never in
//! [`MarketInput::items`] with a `0.0`, because zero is a real price and
//! "nobody is selling this" is not one.
//!
//! # Room names are poe.ninja's
//!
//! `rooms` is keyed on the name the feed publishes — `"Locus of Corruption"`,
//! `"Anomaly Research Lab"` — which is the room's own name at that tier, not the
//! family name. Callers resolve a [`rooms::RoomLine`](super::rooms::RoomLine)
//! key to that string through [`RoomLine::name`](super::rooms::RoomLine::name),
//! so there is exactly one name table in the crate and no second hand-written
//! map to drift.
//!
//! # Stale is answered here, once
//!
//! Epic lock L4 (POE-124) says a stale market falls back to the preset's base
//! value rather than to zero. [`MarketInput::stale`] is the client's judgement
//! (POE-258 sets it from [`MarketInput::as_of`]); this module enforces its
//! consequence in ONE place, by having every reading accessor answer as if the
//! market were absent while the flag is set. A caller cannot forget the rule
//! because it never gets to see the numbers.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::strategy::Tier;

// -------------------------------------------------------------- the quotes --

/// One room-tier line as the server priced it.
///
/// `sale_delta` is the whole point of the room half: the feed price minus the
/// floor when the room-tier is above the floor, and zero otherwise. On a
/// typical snapshot 84 of the 86 rooms read zero, and the two that do not are
/// the rooms worth selling.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub struct RoomQuote {
    /// The feed's price for this room-tier, in chaos.
    pub chaos: f64,
    /// `chaos - floor` when above the floor, `0.0` otherwise.
    pub sale_delta: f64,
    /// Whether the server read this room-tier as carrying a sellable outcome.
    pub above_floor: bool,
    /// The feed's listing count.
    pub listings: i64,
    /// POE-131's thin-market flag: below 40% of its own market's median depth.
    /// A flag, never a filter (ADR-015/017/018).
    pub low_confidence: bool,
}

/// One recipe member's price — a vial, a base unique or an upgraded unique.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub struct ItemQuote {
    /// Chaos price. Zero is legal and means the feed prices it at zero; an item
    /// nobody is selling is in [`MarketInput::unpriced`] instead.
    pub chaos: f64,
    /// The feed's listing count.
    pub listings: i64,
    /// POE-131's thin-market flag.
    pub low_confidence: bool,
    /// POE-252: this price is the median over a trailing window rather than the
    /// newest print. A flag for the explanation box, never a filter.
    pub window_priced: bool,
}

/// `vial` + `base` -> `upgraded`. Names only; the prices live in
/// [`MarketInput::items`] so a member appearing in two recipes is priced once.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub struct Recipe {
    /// The vial that performs the upgrade.
    pub vial: String,
    /// The unique the vial is used on.
    pub base: String,
    /// What it becomes.
    pub upgraded: String,
}

// --------------------------------------------------------------- the input --

/// One market snapshot, as the valuation reads it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "WireMarket")]
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub struct MarketInput {
    /// The league the server priced against, as it named it — `"Allflame"` on
    /// the committed capture. Prices are league-local: a Standard read and a
    /// challenge-league read of the same vial are two different numbers, so the
    /// value the overlay shows has to be able to say which league it is from
    /// (POE-260), and POE-258 has to notice when the server switches league.
    /// Empty when the payload carried none.
    pub league: String,
    /// When the server last had an observation. `None` on a cold cache.
    pub as_of: Option<DateTime<Utc>>,
    /// The client's judgement that [`Self::as_of`] is too old to price with.
    /// Never served — POE-258 sets it. While it is set every accessor here
    /// answers as if the market were absent, which is epic lock L4.
    pub stale: bool,
    /// The room-line floor the server's sale deltas are measured from.
    pub floor: f64,
    /// Room-tiers keyed by `(poe.ninja room name, tier)`.
    pub rooms: BTreeMap<(String, u8), RoomQuote>,
    /// Priced recipe members, keyed by poe.ninja's own `lines[].name`.
    pub items: BTreeMap<String, ItemQuote>,
    /// Recipe members the feed carries no usable line for. Distinct from
    /// absent-from-the-payload, and never a zero price.
    pub unpriced: BTreeSet<String>,
    /// The static vial -> unique recipe table.
    pub recipes: Vec<Recipe>,
}

impl MarketInput {
    /// The empty read: no observation, no rooms, no prices.
    ///
    /// This is what a valuation gets before the first poll answers, and it is
    /// what makes every room fall back to its grade ladder value rather than to
    /// zero.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn none() -> MarketInput {
        MarketInput {
            league: String::new(),
            as_of: None,
            stale: false,
            floor: 0.0,
            rooms: BTreeMap::new(),
            items: BTreeMap::new(),
            unpriced: BTreeSet::new(),
            recipes: Vec::new(),
        }
    }

    /// [`Self::as_of`] as epoch milliseconds — the form the slice's staleness
    /// clock and the overlay's price-age line work in.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn as_of_ms(&self) -> Option<i64> {
        self.as_of.map(|at| at.timestamp_millis())
    }

    /// Whether this read may be priced against at all — a stale read may not.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn is_live(&self) -> bool {
        !self.stale
    }

    /// The quote for one room-tier, by the name poe.ninja publishes for it.
    ///
    /// `None` while [`Self::stale`] is set, so a stale read is indistinguishable
    /// from no read at every call site.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn room(&self, name: &str, tier: Tier) -> Option<&RoomQuote> {
        if self.stale {
            return None;
        }
        self.rooms.get(&(name.to_string(), tier.get()))
    }

    /// The sale value of one room-tier: what the feed pays above the floor.
    ///
    /// `0.0` when the room is at the floor, when the read carries no such room,
    /// and when the read is stale. Negative and non-finite deltas are read as
    /// `0.0` — a room cannot be worth less than not selling it, and the
    /// valuation's "never NaN, never negative" invariant starts here.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn sale_delta(&self, name: &str, tier: Tier) -> f64 {
        match self.room(name, tier) {
            Some(quote) if quote.sale_delta.is_finite() && quote.sale_delta > 0.0 => {
                quote.sale_delta
            }
            _ => 0.0,
        }
    }

    /// The price of one recipe member, by poe.ninja's own name for it.
    ///
    /// `None` for an unpriced item and for every item while the read is stale.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn price(&self, item: &str) -> Option<&ItemQuote> {
        if self.stale {
            return None;
        }
        self.items.get(item)
    }

    /// Whether the payload carried this item and could not price it — the
    /// reason a drop term contributes nothing, as opposed to the item simply
    /// not being in the recipe table.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn is_unpriced(&self, item: &str) -> bool {
        self.unpriced.contains(item)
    }
}

// ---------------------------------------------------------------- the wire --

// A field-for-field mirror of the served JSON. Only the fields the valuation
// reads are declared: serde ignores the rest (`name`, `icon`, `divine`,
// `floorRule`, `categoriesSeen`, `windowHours`, `windowSamples`), and declaring
// a field nothing reads would be a dead-code warning that says nothing.

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireMarket {
    #[serde(default)]
    league: String,
    #[serde(default)]
    as_of: Option<DateTime<Utc>>,
    #[serde(default)]
    floor: f64,
    #[serde(default)]
    rooms: Vec<WireRoom>,
    #[serde(default)]
    items: Vec<WireItem>,
    #[serde(default)]
    recipes: Vec<Recipe>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRoom {
    line: String,
    tier: u8,
    chaos: f64,
    sale_delta: f64,
    above_floor: bool,
    listings: i64,
    low_confidence: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireItem {
    name: String,
    price: Option<WirePrice>,
    unpriced: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WirePrice {
    chaos: f64,
    listings: i64,
    low_confidence: bool,
    window_priced: bool,
}

impl From<WireMarket> for MarketInput {
    fn from(wire: WireMarket) -> MarketInput {
        let mut rooms = BTreeMap::new();
        for room in wire.rooms {
            rooms.insert(
                (room.line, room.tier),
                RoomQuote {
                    chaos: room.chaos,
                    sale_delta: room.sale_delta,
                    above_floor: room.above_floor,
                    listings: room.listings,
                    low_confidence: room.low_confidence,
                },
            );
        }

        let mut items = BTreeMap::new();
        let mut unpriced = BTreeSet::new();
        for item in wire.items {
            // A negative or non-finite chaos figure is not a price. It joins
            // the unpriced set rather than poisoning a total with a NaN, and
            // the room that names it then reads as partially priced — which is
            // true — instead of silently valuing at zero.
            match item.price {
                Some(price) if !item.unpriced && price.chaos.is_finite() && price.chaos >= 0.0 => {
                    items.insert(
                        item.name,
                        ItemQuote {
                            chaos: price.chaos,
                            listings: price.listings,
                            low_confidence: price.low_confidence,
                            window_priced: price.window_priced,
                        },
                    );
                }
                _ => {
                    unpriced.insert(item.name);
                }
            }
        }

        MarketInput {
            league: wire.league,
            as_of: wire.as_of,
            stale: false,
            floor: if wire.floor.is_finite() && wire.floor >= 0.0 {
                wire.floor
            } else {
                0.0
            },
            rooms,
            items,
            unpriced,
            recipes: wire.recipes,
        }
    }
}

// -------------------------------------------------------------- the fixture --

/// A REAL capture of `GET /api/analysis/temple-market` against the local
/// server on 2026-09-06, league Allflame — byte-for-byte as it was served.
///
/// Nothing was trimmed: the endpoint's own answer is already exactly the 86
/// room-tier lines, the 31 recipe members and the 11 recipes, which is what
/// POE-257's brief asked the fixture to be scoped to. Two rooms are above the
/// floor of 10 c — Locus of Corruption tier 3 at 856 (delta 846) and Doryani's
/// Institute tier 3 at 400 (delta 390) — and those two numbers are what the
/// grade ladder in [`rooms::Grade::fallback_chaos`](super::rooms::Grade::fallback_chaos)
/// is calibrated against.
#[cfg(test)]
pub const ALLFLAME_CAPTURE: &str = include_str!("assets/temple-market-allflame-2026-09-06.json");

/// [`ALLFLAME_CAPTURE`] parsed. The shared arrange step of every valuation
/// test, so the formula is exercised against a market that actually happened
/// rather than one written to make it pass.
#[cfg(test)]
pub fn allflame() -> MarketInput {
    serde_json::from_str(ALLFLAME_CAPTURE).expect("the committed capture parses as a MarketInput")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(n: u8) -> Tier {
        Tier::new(n).expect("test tier is 0..=3")
    }

    #[test]
    fn the_fixture_parses_every_room_tier_the_payload_carries() {
        let market = allflame();

        assert_eq!(market.rooms.len(), 86, "86 room-tier lines in the capture");
        assert_eq!(market.league, "Allflame", "the league the capture priced");
        assert_eq!(market.floor, 10.0, "the capture's room-line floor");
        assert_eq!(
            market.as_of_ms(),
            Some(1_788_665_199_649),
            "asOf 2026-09-06T03:26:39.649783Z in epoch ms"
        );
    }

    #[test]
    fn a_room_tier_keeps_its_feed_price_and_sale_delta() {
        let market = allflame();

        let locus = market
            .room("Locus of Corruption", tier(3))
            .expect("the capture carries Locus of Corruption tier 3");

        assert_eq!(locus.chaos, 856.0);
        assert_eq!(locus.sale_delta, 846.0);
        assert!(locus.above_floor);
        assert_eq!(locus.listings, 2523);
        assert!(!locus.low_confidence);
    }

    #[test]
    fn a_room_at_the_floor_has_no_sale_value() {
        let market = allflame();

        // Anomaly Research Lab tier 1 is one of the 84 rooms sitting at the
        // floor price the floor IS.
        assert_eq!(market.sale_delta("Anomaly Research Lab", tier(1)), 0.0);
        assert_eq!(market.sale_delta("Locus of Corruption", tier(3)), 846.0);
    }

    #[test]
    fn a_room_the_payload_does_not_carry_reads_as_no_sale() {
        let market = allflame();

        assert!(market.room("Not A Temple Room", tier(3)).is_none());
        assert_eq!(market.sale_delta("Not A Temple Room", tier(3)), 0.0);
    }

    #[test]
    fn a_priced_item_carries_its_chaos_and_its_thin_market_flags() {
        let market = allflame();

        let vial = market
            .price("Vial of Summoning")
            .expect("the capture prices Vial of Summoning");

        assert_eq!(vial.chaos, 60.0);
        assert_eq!(vial.listings, 168);
        assert!(!vial.low_confidence);
        assert!(!vial.window_priced);
    }

    #[test]
    fn a_window_priced_item_carries_both_thin_market_flags() {
        // Vial of Sacrifice is the capture's one thin line: 11 listings, priced
        // off the 24-hour window rather than the newest print (POE-252).
        let market = allflame();

        let vial = market
            .price("Vial of Sacrifice")
            .expect("the capture prices Vial of Sacrifice");

        assert_eq!(vial.chaos, 428.0);
        assert!(vial.low_confidence);
        assert!(vial.window_priced);
    }

    #[test]
    fn an_item_the_feed_cannot_price_is_absent_rather_than_zero() {
        // Shadowstitch is the one unique in drops.rs poe.ninja publishes no
        // line for, so POE-255 never serves it at all.
        let market = allflame();

        assert!(market.price("Shadowstitch").is_none());
        assert!(!market.is_unpriced("Shadowstitch"));
    }

    #[test]
    fn a_negative_price_is_read_as_unpriced() {
        let market: MarketInput = serde_json::from_str(
            r#"{"floor":10,"rooms":[],"recipes":[],
                "items":[{"name":"Broken","price":{"chaos":-5,"listings":3,
                          "lowConfidence":false,"windowPriced":false},"unpriced":false}]}"#,
        )
        .expect("the wire mirror accepts a partial payload");

        assert!(market.price("Broken").is_none());
        assert!(market.is_unpriced("Broken"));
    }

    #[test]
    fn a_null_price_lands_in_the_unpriced_set() {
        let market: MarketInput = serde_json::from_str(
            r#"{"floor":10,"rooms":[],"recipes":[],
                "items":[{"name":"Nobody Sells This","price":null,"unpriced":true}]}"#,
        )
        .expect("the wire mirror accepts a partial payload");

        assert!(market.price("Nobody Sells This").is_none());
        assert!(market.is_unpriced("Nobody Sells This"));
        assert!(market.items.is_empty(), "never a zero price");
    }

    #[test]
    fn a_negative_sale_delta_is_read_as_no_sale() {
        let market: MarketInput = serde_json::from_str(
            r#"{"floor":10,"items":[],"recipes":[],
                "rooms":[{"line":"Locus of Corruption","tier":3,"chaos":1,"saleDelta":-9,
                          "aboveFloor":false,"listings":2,"lowConfidence":false}]}"#,
        )
        .expect("the wire mirror accepts a partial payload");

        assert_eq!(market.sale_delta("Locus of Corruption", tier(3)), 0.0);
    }

    #[test]
    fn the_recipe_table_survives_the_mirror() {
        let market = allflame();

        assert_eq!(market.recipes.len(), 11, "11 vial recipes in the capture");
        assert!(
            market.recipes.contains(&Recipe {
                vial: "Vial of Consequence".to_string(),
                base: "Coward's Chains".to_string(),
                upgraded: "Coward's Legacy".to_string(),
            }),
            "direction is base + vial -> upgraded"
        );
    }

    #[test]
    fn a_stale_read_answers_as_if_it_carried_nothing() {
        let mut market = allflame();
        market.stale = true;

        assert!(!market.is_live());
        assert!(market.room("Locus of Corruption", tier(3)).is_none());
        assert_eq!(market.sale_delta("Locus of Corruption", tier(3)), 0.0);
        assert!(market.price("Vial of Summoning").is_none());
    }

    #[test]
    fn the_empty_read_prices_nothing() {
        let market = MarketInput::none();

        assert!(market.is_live(), "empty is not the same claim as stale");
        assert_eq!(market.as_of_ms(), None);
        assert_eq!(market.sale_delta("Locus of Corruption", tier(3)), 0.0);
        assert!(market.price("Vial of Summoning").is_none());
    }
}
