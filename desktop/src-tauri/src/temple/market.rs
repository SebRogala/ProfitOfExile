//! The market read a room valuation is computed against (POE-257).
//!
//! POE-255 serves one aggregated, league-keyed object at
//! `GET /api/analysis/temple-market`: every room-tier's sale delta and every
//! room unique's price. This module is the desktop's mirror of that payload, plus the poll that fetches
//! it and publishes it onto the temple slice.
//!
//! The poll ([`spawn_temple_market_poll`], POE-258) lives here as its own tokio
//! task, never on the 650 ms tick (`docs/TEMPLE-LIFECYCLE.md` forbids network
//! work there); the payload mirror stays pure, so it runs in the Linux test
//! container.
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
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use tauri::{AppHandle, Manager};

use super::slice::{MarketView, TempleSlice};
use super::strategy::Tier;
use super::valuation::warm_temple_valuation;
use crate::AppState;

// -------------------------------------------------------------- the quotes --

/// One room-tier line as the server priced it.
///
/// `sale_delta` is the whole point of the room half: what the room-tier sells
/// for above the floor when the server reads it as above the floor, and zero
/// otherwise. The server owns that rule (`internal/temple/market.go`: the
/// trailing-24h median against a multiple of the floor) and this side only
/// reads its answer. On a
/// typical snapshot 84 of the 86 rooms read zero, and the two that do not are
/// the rooms worth selling.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomQuote {
    /// The feed's price for this room-tier, in chaos.
    pub chaos: f64,
    /// The server's sale price above the floor (its 24h median, not `chaos`)
    /// when above the floor, `0.0` otherwise.
    pub sale_delta: f64,
    /// Whether the server read this room-tier as carrying a sellable outcome.
    pub above_floor: bool,
    /// The feed's listing count.
    pub listings: i64,
    /// POE-131's thin-market flag: below 40% of its own market's median depth.
    /// A flag, never a filter (ADR-015/017/018).
    pub low_confidence: bool,
}

/// One room unique's price.
#[derive(Debug, Clone, PartialEq)]
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

// --------------------------------------------------------------- the input --

/// One market snapshot, as the valuation reads it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "WireMarket")]
pub struct MarketInput {
    /// The league the server priced against, as it named it — `"Allflame"` on
    /// the committed capture. Prices are league-local: a Standard read and a
    /// challenge-league read of the same unique are two different numbers, so the
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
    /// Priced uniques, keyed by poe.ninja's own `lines[].name`.
    pub items: BTreeMap<String, ItemQuote>,
    /// Uniques the feed carries no usable line for. Distinct from
    /// absent-from-the-payload, and never a zero price.
    pub unpriced: BTreeSet<String>,
}

/// How old an observation may be before the read stops pricing anything, in
/// milliseconds: two hours (POE-258 D2).
///
/// Four poe.ninja collection cycles. The server recomputes the temple market
/// hourly, so one missed cycle is normal and must not blank the board; four
/// missed cycles is a feed that has stopped, and a room valued from prices that
/// old is a number the player cannot act on. Crossing it does not blank the
/// board either — [`MarketInput::prices_anything`] falls the whole read back to
/// the preset's base values, which is epic lock L4.
pub const STALE_AFTER_MS: i64 = 2 * 60 * 60 * 1000;

impl MarketInput {
    /// The empty read: no observation, no rooms, no prices.
    ///
    /// This is what a valuation gets before the first poll answers, and it is
    /// what makes every room fall back to its grade ladder value rather than to
    /// zero.
    pub fn none() -> MarketInput {
        MarketInput {
            league: String::new(),
            as_of: None,
            stale: false,
            floor: 0.0,
            rooms: BTreeMap::new(),
            items: BTreeMap::new(),
            unpriced: BTreeSet::new(),
        }
    }

    /// [`Self::as_of`] as epoch milliseconds — the form the slice's staleness
    /// clock and the overlay's price-age line work in.
    pub fn as_of_ms(&self) -> Option<i64> {
        self.as_of.map(|at| at.timestamp_millis())
    }

    /// This read with [`Self::stale`] judged against `now_ms`.
    ///
    /// **Judged at USE time, never at poll time** (POE-258 D2). The poll stores
    /// the payload as it arrived and every reader asks for the judgement it
    /// needs by calling this with the current clock, so a server that stops
    /// recomputing goes stale on its own — with a poll cadence of
    /// `market::TEMPLE_MARKET_POLL` a flag stamped at fetch time would say "live"
    /// for as long as the fetches keep succeeding against a frozen cache.
    ///
    /// A read with no observation ([`Self::none`], and the cold answer that
    /// becomes it) is NOT stale at any age: `stale` is the claim that a real
    /// observation is too old, and the empty read has none to be old. The
    /// distinction is the one [`Self::none`]'s own test pins.
    ///
    /// Consuming rather than borrowing, so the judgement cannot be forgotten:
    /// a caller holding a [`MarketInput`] has either asked the clock or is
    /// holding the poll's own untouched copy, and there is no third state where
    /// `stale` is a value somebody set by hand.
    pub fn aged_at(mut self, now_ms: i64) -> MarketInput {
        self.stale = match self.as_of_ms() {
            Some(at) => now_ms.saturating_sub(at) > STALE_AFTER_MS,
            None => false,
        };
        self
    }

    /// Whether this read may be priced against at all — a stale read may not.
    pub fn is_live(&self) -> bool {
        !self.stale
    }

    /// Whether this read's floor is a number a sale delta can be measured
    /// against.
    ///
    /// The server computes the floor as the median of all 86 room-tier lines
    /// (POE-255), so zero is not a market state anybody can reach by trading —
    /// it is a broken or empty feed. Left unguarded it turns every room's raw
    /// price into its "sale delta", which would anchor the grade ladder on the
    /// most expensive room in the game and cap the rungs against the cheapest.
    /// [`Self::prices_anything`] therefore reads it as no market at all.
    pub fn has_usable_floor(&self) -> bool {
        self.floor.is_finite() && self.floor > 0.0
    }

    /// Whether anything in this read can put chaos on a term.
    ///
    /// A read that answers `false` is COLD in the sense epic lock L4 means it:
    /// the valuation prices no room from it and reads the whole board off the
    /// preset's base-value ladder instead
    /// (`valuation::Valued::compute_with`). Three ways to be cold, and a
    /// caller must not have to tell them apart — [`Self::none`] before the
    /// first poll answers, a payload the client has judged [`Self::stale`],
    /// which every accessor here already answers as absent, and a payload with
    /// no usable floor.
    ///
    /// The floor belongs in this answer rather than only in the two derived
    /// numbers that read it. Guarding only those leaves a third state: the
    /// board would still SUM the payload's own `saleDelta`s while every
    /// unpriced and instrumental room took the cold, UNCAPPED rung — Temple
    /// Nexus at 100 over a room the feed prices at 3 — which is a board ranked
    /// in two units at once and is exactly what the cap exists to prevent.
    ///
    /// Rooms OR items: a payload that carries no room quotes can still price a
    /// unique, and a valuation that ignored it would throw away the only number
    /// it had.
    pub fn prices_anything(&self) -> bool {
        self.is_live()
            && self.has_usable_floor()
            && !(self.rooms.is_empty() && self.items.is_empty())
    }

    /// The quote for one room-tier, by the name poe.ninja publishes for it.
    ///
    /// `None` while [`Self::stale`] is set, so a stale read is indistinguishable
    /// from no read at every call site.
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
    pub fn sale_delta(&self, name: &str, tier: Tier) -> f64 {
        match self.room(name, tier) {
            Some(quote) if quote.sale_delta.is_finite() && quote.sale_delta > 0.0 => {
                quote.sale_delta
            }
            _ => 0.0,
        }
    }

    /// The price of one unique, by poe.ninja's own name for it.
    ///
    /// `None` for an unpriced item and for every item while the read is stale.
    pub fn price(&self, item: &str) -> Option<&ItemQuote> {
        if self.stale {
            return None;
        }
        self.items.get(item)
    }

    /// Whether the payload carried this item and could not price it — the
    /// reason a drop term contributes nothing, as opposed to the item simply
    /// not being in the served item set.
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
        }
    }
}

// ------------------------------------------- the temple market poll (POE-258) --

/// How often the temple market is re-read from the server.
///
/// Five minutes, and the cadence is the poll's own rather than a ride on some
/// other tick because there is no other tick to ride: the league service in `ssot.rs`
/// waits indefinitely after success, and the only periodic
/// server read the app already had is the webview's 30-minute entitlement +
/// update check in `stores/status.svelte.ts` — six times too slow for a price
/// the overlay states the age of.
///
/// Five minutes against a market the server recomputes hourly is deliberately
/// far inside the source's own cadence: what the extra polls buy is not
/// fresher prices but a bounded lag on the two transitions the player sees —
/// a read ageing past [`market::STALE_AFTER_MS`](crate::temple::market::STALE_AFTER_MS),
/// and a server that has come back. Both are re-judged on every tick of this
/// loop whether or not the fetch succeeds.
pub const TEMPLE_MARKET_POLL: Duration = Duration::from_secs(5 * 60);

/// What one poll of the temple market decided.
///
/// A value rather than a branch inside the loop, so the rule that decides
/// whether a payload may be shown is one testable function
/// ([`judge_market`]) instead of a chain of `if`s wrapped in HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketPoll {
    /// The payload prices the league the app is on: it replaces the stored
    /// read.
    Accepted,
    /// The payload prices a DIFFERENT league. Discarded — a Standard price and
    /// a challenge-league price for the same unique are two different numbers,
    /// and showing the old server's as current is the failure the DEBUG/PROD
    /// toggle makes reachable in one click.
    WrongLeague,
    /// The server answered from a cold cache (`asOf: null`): reachable, with
    /// nothing observed yet. **The last good read stands** and ages on its own,
    /// the same as [`MarketPoll::Failed`] — a server that has not recomputed
    /// yet says nothing about the prices already in hand, and a restarted
    /// server would otherwise take a board off prices that are still minutes
    /// old. With nothing stored the read is already [`MarketInput::none`], so
    /// the board falls back to base values and the overlay says unavailable.
    Cold,
    /// The request failed. **The last good read stands** and ages on its own —
    /// blanking a market because one poll timed out would take the board off
    /// prices that are still minutes old.
    Failed,
    /// No league is resolved yet, so nothing can be keyed and no request was
    /// made. The stored read stands.
    Unresolved,
}

/// Whether a fetched payload may replace the stored read, and why not.
///
/// Pure, so the league rule is unit-testable without a server. The order of the
/// checks is the order of the questions: an unresolvable payload is not a
/// failed fetch, a payload for another league is discarded whether or not it
/// carried observations, and only then does a cold answer read as cold.
pub fn judge_market(fetched: Option<&MarketInput>, league: Option<&str>) -> MarketPoll {
    let Some(league) = league else {
        return MarketPoll::Unresolved;
    };
    let Some(market) = fetched else {
        return MarketPoll::Failed;
    };
    if market.league.trim() != league.trim() {
        return MarketPoll::WrongLeague;
    }
    if market.as_of.is_none() {
        return MarketPoll::Cold;
    }
    MarketPoll::Accepted
}

/// The stored market read as of NOW: the payload the poll last accepted, with
/// [`MarketInput::stale`] judged against the current clock.
///
/// **The one reader every consumer uses**, `temple::run`'s tick included. The
/// judgement is made here and not at poll time so a server that stops
/// recomputing goes stale without a new poll (POE-258 D2), and taking it
/// through one function is what stops a caller from reading the raw field and
/// pricing a board against a two-day-old market.
pub fn temple_market_now(app: &AppHandle) -> MarketInput {
    let stored = {
        let state = app.state::<AppState>();
        let market = state.temple_market.lock().unwrap_or_else(|e| e.into_inner());
        market.clone()
    };
    stored.aged_at(chrono::Utc::now().timestamp_millis())
}

/// Publish the stored read's age and availability onto the temple slice.
///
/// **`TempleSlice::poll` and NOTHING else.** `TempleSlice::market` belongs to
/// the read that produced the board on screen and is written by
/// `temple::slice::project` alone; a poll writing it would put this market's
/// line over that read's numbers — "prices 3 min old" over a board on the cold
/// grade ladder, or "prices unavailable" over a priced board one DEBUG/PROD
/// switch later. The two fields are the answer to two different questions and
/// this function may only answer one of them.
///
/// Through `temple::run::publish`, which is the same path the `temple_set_*`
/// commands write their echoes through: it writes under the temple mutex,
/// compares, and emits `ssot-changed` only when something actually moved — so a
/// poll that changes nothing costs every overlay's poll nothing.
fn publish_market_view(app: &AppHandle) {
    let view = super::slice::market_view(&temple_market_now(app));
    super::run::publish(app, |slice| apply_market_view(slice, view.clone()));
}

/// Where a poll's view lands on the slice, as a function of the slice alone.
///
/// Extracted from [`publish_market_view`] for the reason [`store_market`] and
/// [`drop_market`] are extracted: the rule is one line, it is the whole
/// contract, and behind an `AppHandle` there is nothing to assert it against.
fn apply_market_view(slice: &mut TempleSlice, view: MarketView) {
    slice.poll = view;
}

/// Store what a poll decided. Returns whether the stored read actually changed.
///
/// [`MarketPoll::Failed`], [`MarketPoll::Unresolved`] and [`MarketPoll::Cold`]
/// write nothing: none of the three is evidence ABOUT the prices in hand, so
/// the last good read stands and ages on its own. Only
/// [`MarketPoll::WrongLeague`] blanks — a payload for another league is the app
/// knowing that what it holds prices the wrong market — and
/// [`MarketPoll::Accepted`] replaces.
///
/// Takes the mutex rather than the `AppHandle` so the rule is unit-testable
/// without a running app, as with `write_league`.
fn store_market(
    stored: &std::sync::Mutex<MarketInput>,
    verdict: MarketPoll,
    fetched: Option<MarketInput>,
) -> bool {
    let incoming = match verdict {
        MarketPoll::Accepted => match fetched {
            Some(market) => market,
            // Unreachable: `judge_market` answers `Accepted` only for a payload
            // it was handed. Treated as "we learned nothing" rather than
            // unwrapped, so a future edit to the judge cannot panic the poller.
            None => return false,
        },
        MarketPoll::WrongLeague => MarketInput::none(),
        MarketPoll::Failed | MarketPoll::Unresolved | MarketPoll::Cold => return false,
    };
    let mut guard = stored.lock().unwrap_or_else(|e| e.into_inner());
    if *guard == incoming {
        return false;
    }
    *guard = incoming;
    true
}

/// Drop the stored read outright. Returns whether there was one to drop.
///
/// The server-switch path, and NOT a poll verdict: the app has not learned
/// anything about a market here, it has learned that the market it was holding
/// belongs to a server it is no longer talking to. Extracted for the same
/// testability reason as [`store_market`].
fn drop_market(stored: &std::sync::Mutex<MarketInput>) -> bool {
    let mut guard = stored.lock().unwrap_or_else(|e| e.into_inner());
    if *guard == MarketInput::none() {
        return false;
    }
    *guard = MarketInput::none();
    true
}

/// One poll's verdict, with the two league names the wrong-league log has to
/// name — the payload's and the app's. Carried out of the poll rather than
/// re-read at the log site, because by then the payload is gone.
struct PollOutcome {
    verdict: MarketPoll,
    /// The league the payload priced. `None` when there was no payload.
    payload_league: Option<String>,
    /// The league the app is on. `None` before it resolves.
    app_league: Option<String>,
}

/// One poll: read the league, fetch, judge, store, publish.
///
/// The league is read BEFORE the request and a missing one skips it entirely —
/// there is nothing to check a payload against, so the request would be spent
/// on an answer that could only be discarded.
async fn poll_temple_market_once(app: &AppHandle) -> PollOutcome {
    let app_league = {
        let state = app.state::<AppState>();
        let ssot = state.ssot.lock().unwrap_or_else(|e| e.into_inner());
        ssot.league.name.clone()
    };
    let fetched = match app_league {
        Some(_) => fetch_temple_market_once(app).await,
        None => None,
    };
    let verdict = judge_market(fetched.as_ref(), app_league.as_deref());
    // Read back before the move: the payload is about to become the stored
    // read, and both log lines below are about what it said.
    let payload_league = fetched.as_ref().map(|m| m.league.clone());
    let priced = fetched.as_ref().map(|m| (m.as_of, m.rooms.len()));
    let changed = {
        let state = app.state::<AppState>();
        store_market(&state.temple_market, verdict, fetched)
    };
    // Published on EVERY poll and not only on a change, because the view
    // carries an age: a last-good read that has just crossed the stale line
    // moves `TempleSlice::poll` while the stored payload sits still. It does
    // NOT touch `TempleSlice::market` — that field belongs to the read whose
    // board is on screen, and a poll re-labelling it is the defect this split
    // exists to end.
    publish_market_view(app);
    // "Computed on update", for the one input that updates on a timer: the
    // valuation is rebuilt HERE and not on the read that needs it. Warmed on
    // every poll rather than only on `changed`, because the market's own
    // staleness verdict moves with the clock while the stored payload sits
    // still — this is what keeps the two-hourly stale flip off the read path
    // as well.
    warm_temple_valuation(app);
    if changed && verdict == MarketPoll::Accepted {
        if let Some((as_of, rooms)) = priced {
            log::info!(
                "temple market: {} rooms priced in {:?}, observed {:?}",
                rooms,
                payload_league,
                as_of
            );
        }
    }
    PollOutcome { verdict, payload_league, app_league }
}

/// One temple-market fetch attempt. `None` on any failure (offline, non-2xx,
/// bad JSON) so the caller keeps the last good read. The server-url guard and
/// the cloned HTTP client are scoped before the first `.await`, so no lock is
/// held across the network round-trip — the same shape as
/// `crate::ssot::fetch_league_once`.
async fn fetch_temple_market_once(app: &AppHandle) -> Option<MarketInput> {
    let (server_url, http) = {
        let state = app.state::<AppState>();
        let url = state
            .server_url
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        (url, state.server_http.clone())
    };
    let url = format!("{}/api/analysis/temple-market", server_url);

    // Per-request, because `server_http` carries no default timeout: without
    // one a connection that hangs open holds this poll forever and the loop
    // never reaches its next tick. The same guard the merc sync
    // (`mercenary/sync.rs`) and seed art fetch put on their own requests.
    let resp = match http.get(&url).timeout(Duration::from_secs(15)).send().await {
        Ok(r) => r,
        Err(e) => {
            log::debug!("temple market fetch: request to {} failed: {}", url, e);
            return None;
        }
    };
    if !resp.status().is_success() {
        log::debug!("temple market fetch: {} returned {}", url, resp.status());
        return None;
    }
    match resp.json::<MarketInput>().await {
        Ok(market) => Some(market),
        Err(e) => {
            log::debug!("temple market fetch: bad JSON from {}: {}", url, e);
            None
        }
    }
}

/// Spawn the temple market poller: one fetch every [`TEMPLE_MARKET_POLL`], and
/// one immediately whenever [`TEMPLE_MARKET_NOTIFY`](crate::ssot::TEMPLE_MARKET_NOTIFY) is woken.
///
/// It never returns and it never backs off. Unlike the league resolver there is
/// nothing to resolve ONCE — prices go out of date — and a failed poll is not a
/// state worth retrying faster than the cadence, because the last good read is
/// still standing and still says its own age.
///
/// It polls whether or not the temple module is switched on: the Temple page
/// stays browsable with the module off (ADR-014) and states the age of the
/// prices its board was valued at, so a poller gated on the toggle would leave
/// that page saying "unavailable" about a server it never asked.
pub fn spawn_temple_market_poll(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<MarketPoll> = None;
        loop {
            let outcome = poll_temple_market_once(&app).await;
            // On the TRANSITION only — the loop runs 288 times a day and a
            // steady state has nothing new to say. The wrong-league line names
            // BOTH leagues, which is what a DEBUG/PROD switch needs: which one
            // the server priced, and which one this app is on.
            if last != Some(outcome.verdict) {
                match outcome.verdict {
                    MarketPoll::WrongLeague => log::info!(
                        "temple market: the server prices {:?} but this app is on {:?}; \
                         discarded until the two agree",
                        outcome.payload_league,
                        outcome.app_league
                    ),
                    MarketPoll::Unresolved => {
                        log::info!("temple market: no league resolved yet, not polling")
                    }
                    MarketPoll::Failed => log::info!(
                        "temple market: unreachable; the last read stands and ages on its own"
                    ),
                    MarketPoll::Cold => {
                        log::info!("temple market: the server has no observations yet")
                    }
                    MarketPoll::Accepted => {}
                }
                last = Some(outcome.verdict);
            }
            tokio::select! {
                _ = tokio::time::sleep(TEMPLE_MARKET_POLL) => {}
                _ = crate::ssot::TEMPLE_MARKET_NOTIFY.notified() => {}
            }
        }
    });
}

/// Everything that has to happen when the user points the app at a different
/// server (the DEBUG/PROD toggle, or a url typed in Settings).
///
/// Called from `set_server_url`, which is the single choke point for the
/// change. Drops the market, publishes the poll view and warms the
/// valuation; `lib.rs` bumps the league fetch generation before this and
/// re-arms the league after it.
///
/// **The market is dropped immediately.** Its prices came from the other
///    server's league and nothing from it may be priced as current, so the very
///    next read falls back to base values rather than waiting five minutes for
///    a poll to say so — the webview's own entitlement reset
///    (`stores/status.svelte.ts`, commit a4414ef) makes exactly this trade for
///    exactly this reason.
///
///    What it does NOT do is re-label the board already on screen.
///    `publish_market_view` moves `TempleSlice::poll`, so the page's reader row
///    says "prices unavailable" within the same click while the standing
///    board's own boxes keep the line they were priced under — which is the
///    truth about them until a read replaces them.
///
///    The valuation is rebuilt against the dropped market in the same step
///    ([`warm_temple_valuation`]), so the first read on the new server looks
///    up its grade-ladder table instead of building one.
pub fn on_server_url_changed(app: &AppHandle) {
    {
        let state = app.state::<AppState>();
        drop_market(&state.temple_market);
    }
    publish_market_view(app);
    // The dropped market is an input change like any other, and the very next
    // read is the one that falls back to the grade ladder — so it is built
    // here rather than paid for there.
    warm_temple_valuation(app);
}

// -------------------------------------------------------------- the fixture --

/// A REAL capture of `GET /api/analysis/temple-market` against the local
/// server on 2026-09-06, league Allflame — byte-for-byte as it was served.
///
/// Nothing was trimmed: the endpoint's own answer is already exactly the 86
/// room-tier lines, the 31 priced items and the 11 vial recipes (inert since
/// POE-366: the desktop no longer reads `recipes`), which is what
/// POE-257's brief asked the fixture to be scoped to. Two rooms are above the
/// floor of 10 c — Locus of Corruption tier 3 at 856 (delta 846) and Doryani's
/// Institute tier 3 at 400 (delta 390) — and those two numbers are what the
/// grade ladder in [`rooms::RoomRank::fallback_chaos`](super::rooms::RoomRank::fallback_chaos)
/// is calibrated against.
#[cfg(test)]
pub const ALLFLAME_CAPTURE: &str = include_str!("assets/temple-market-allflame-2026-09-06.json");

/// [`ALLFLAME_CAPTURE`] parsed. The shared arrange step of every valuation
/// test, so the formula is exercised against a market that actually happened
/// rather than one written to make it pass.
#[cfg(test)]
pub fn allflame() -> MarketInput {
    // The capture predates the 3.30 renames (POE-365); the asset stays
    // byte-for-byte and its old names are mapped onto the current vocabulary.
    let renamed = ALLFLAME_CAPTURE
        .replace("\"Throne of Atziri\"", "\"Cradle of Creation\"")
        .replace("\"Omnitect Forge\"", "\"Royal Forge\"")
        .replace("\"Omnitect Reactor Plant\"", "\"Grand Reactor\"");
    serde_json::from_str(&renamed).expect("the committed capture parses as a MarketInput")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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

    /// Every tier-3 room `rooms::LINES` names is a tier-3 line the server
    /// prices — the JOIN the sale half of the valuation is made of.
    ///
    /// `MarketInput` is keyed on the name poe.ninja publishes and `rooms.rs`
    /// holds the only copy of those names in the crate, so the two tables are
    /// joined by string and by nothing else. A rename on EITHER side — a typo
    /// fixed in `LINES`, a room the feed starts publishing under a new name —
    /// takes `sale_delta` to `0.0` for that line and moves it silently down the
    /// board: no panic, no warning, just a room the advisor stops valuing. Both
    /// counts are asserted so the check cannot pass by both tables shrinking.
    ///
    /// Tier 3 alone because it is the tier the feed and the grade ladder are
    /// both about (`RoomLine`'s own note), and because epic lock L2 prices the
    /// lower two as a fraction of this line rather than from their own quote.
    #[test]
    fn every_tier_three_room_name_is_a_line_the_capture_prices() {
        let market = allflame();

        let missing: Vec<&str> = super::super::rooms::LINES
            .iter()
            .map(|line| line.tiers()[2])
            .filter(|name| market.room(name, tier(3)).is_none())
            .collect();

        assert!(
            missing.is_empty(),
            "tier-3 names the capture does not price: {missing:?}"
        );
        assert_eq!(super::super::rooms::LINES.len(), 25, "the 25 room lines");
        assert_eq!(
            market.rooms.keys().filter(|(_, t)| *t == 3).count(),
            25,
            "and the 25 tier-3 lines the capture carries — no room priced under a name \
             `LINES` does not know either",
        );
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

        let unique = market
            .price("Architect's Hand")
            .expect("the capture prices Architect's Hand");

        assert_eq!(unique.chaos, 75.4);
        assert_eq!(unique.listings, 214);
        assert!(!unique.low_confidence);
        assert!(!unique.window_priced);
    }

    #[test]
    fn a_window_priced_item_carries_both_thin_market_flags() {
        // Zerphi's Heart is a thin line: 29 listings, priced off the 24-hour
        // window rather than the newest print (POE-252).
        let market = allflame();

        let unique = market
            .price("Zerphi's Heart")
            .expect("the capture prices Zerphi's Heart");

        assert_eq!(unique.chaos, 1280.0);
        assert!(unique.low_confidence);
        assert!(unique.window_priced);
    }

    #[test]
    fn an_item_the_feed_cannot_price_is_absent_rather_than_zero() {
        // Shadowstitch is a name the capture carries no line for. It left
        // drops.rs with POE-277 WI-6; it stays here because this test is about
        // the fixture's own answer — absent, not zero — and not about the table.
        let market = allflame();

        assert!(market.price("Shadowstitch").is_none());
        assert!(!market.is_unpriced("Shadowstitch"));
    }

    #[test]
    fn a_negative_price_is_read_as_unpriced() {
        let market: MarketInput = serde_json::from_str(
            r#"{"floor":10,"rooms":[],
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
            r#"{"floor":10,"rooms":[],
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
            r#"{"floor":10,"items":[],
                "rooms":[{"line":"Locus of Corruption","tier":3,"chaos":1,"saleDelta":-9,
                          "aboveFloor":false,"listings":2,"lowConfidence":false}]}"#,
        )
        .expect("the wire mirror accepts a partial payload");

        assert_eq!(market.sale_delta("Locus of Corruption", tier(3)), 0.0);
    }

    /// POE-366 (decisions T9): a server from before the vial removal still
    /// serves `recipes` objects carrying `vial`, and vial items. The desktop no
    /// longer reads either, and the payload must still decode — a decode
    /// error drops the board to the cold ladder with nothing on screen to say
    /// why. Fails if `WireMarket` starts refusing unknown fields.
    #[test]
    fn an_old_server_payload_with_vial_recipes_still_decodes() {
        let market: MarketInput = serde_json::from_str(
            r#"{"floor":10,"rooms":[],
                "recipes":[{"vial":"Vial of Fate","base":"Story of the Vaal",
                            "upgraded":"Fate of the Vaal"}],
                "items":[{"name":"Story of the Vaal","price":{"chaos":5,"listings":291,
                          "lowConfidence":false,"windowPriced":false},"unpriced":false},
                         {"name":"Vial of Fate","price":{"chaos":1,"listings":343,
                          "lowConfidence":false,"windowPriced":false},"unpriced":false}]}"#,
        )
        .expect("an old server's payload still decodes");

        assert_eq!(market.price("Story of the Vaal").map(|q| q.chaos), Some(5.0));
    }

    #[test]
    fn a_stale_read_answers_as_if_it_carried_nothing() {
        let mut market = allflame();
        market.stale = true;

        assert!(!market.is_live());
        assert!(market.room("Locus of Corruption", tier(3)).is_none());
        assert_eq!(market.sale_delta("Locus of Corruption", tier(3)), 0.0);
        assert!(market.price("Story of the Vaal").is_none());
    }

    /// The capture's own `asOf`, so the ages below are measured from a real
    /// observation rather than from a number invented for the assertion.
    const CAPTURE_AS_OF_MS: i64 = 1_788_665_199_649;
    const ONE_HOUR_MS: i64 = 60 * 60 * 1000;

    /// Age is measured from the payload's own `asOf` and NOT from when the poll
    /// fetched it — the arrange step is a read the poll just handed over, and
    /// it is stale all the same because the server's cache is what froze.
    #[test]
    fn a_read_three_hours_old_prices_nothing() {
        let market = allflame().aged_at(CAPTURE_AS_OF_MS + 3 * ONE_HOUR_MS);

        assert!(market.stale);
        assert!(!market.prices_anything());
        assert_eq!(market.sale_delta("Locus of Corruption", tier(3)), 0.0);
    }

    #[test]
    fn a_read_one_hour_old_still_prices_its_rooms() {
        let market = allflame().aged_at(CAPTURE_AS_OF_MS + ONE_HOUR_MS);

        assert!(!market.stale);
        assert!(market.prices_anything());
        assert_eq!(market.sale_delta("Locus of Corruption", tier(3)), 846.0);
    }

    /// The boundary itself, so the comparison cannot drift to `>=` unnoticed:
    /// a read exactly [`STALE_AFTER_MS`] old is the last live one, and one
    /// millisecond later is the first stale one.
    #[test]
    fn the_stale_boundary_is_exclusive_to_the_millisecond() {
        let at_the_line = allflame().aged_at(CAPTURE_AS_OF_MS + STALE_AFTER_MS);
        let one_past_it = allflame().aged_at(CAPTURE_AS_OF_MS + STALE_AFTER_MS + 1);

        assert!(!at_the_line.stale, "two hours old is still priceable");
        assert!(one_past_it.stale, "a millisecond past two hours is not");
    }

    /// The empty read has no observation, so it has nothing that can be too
    /// old — `stale` is a claim ABOUT an observation and `none()` is the
    /// absence of one. Fails if `aged_at` starts judging a `None` `as_of`.
    #[test]
    fn the_empty_read_is_never_stale_however_late_the_clock() {
        let market = MarketInput::none().aged_at(CAPTURE_AS_OF_MS + 10 * 365 * 24 * ONE_HOUR_MS);

        assert!(!market.stale);
        assert!(market.is_live(), "empty is not the same claim as stale");
    }

    #[test]
    fn the_empty_read_prices_nothing() {
        let market = MarketInput::none();

        assert!(market.is_live(), "empty is not the same claim as stale");
        assert_eq!(market.as_of_ms(), None);
        assert_eq!(market.sale_delta("Locus of Corruption", tier(3)), 0.0);
        assert!(market.price("Story of the Vaal").is_none());
    }

    // ----------------------------------- the temple market poll (POE-258) --

    /// The cadence, stated as the two constraints that chose it rather than as
    /// its own literal: it is a MINUTES-scale poll (never a chatty one beside
    /// the 650 ms detect tick), and it is short enough that a read ageing past
    /// the stale line reaches the screen well inside the window it just left.
    ///
    /// Fails if the poll is dropped to seconds, or raised until a stale market
    /// could sit on screen claiming to be live.
    #[test]
    fn the_market_poll_is_minutes_scale_and_well_inside_the_stale_window() {
        assert!(
            TEMPLE_MARKET_POLL >= Duration::from_secs(60),
            "a price the server recomputes hourly does not need a per-second poll",
        );
        assert!(
            (TEMPLE_MARKET_POLL.as_millis() as i64) * 4 <= crate::temple::market::STALE_AFTER_MS,
            "the stale transition must be published well inside the window it leaves",
        );
    }

    /// The happy path: the committed capture, against the league it priced, is
    /// the read the board is valued on.
    #[test]
    fn a_payload_for_the_league_in_force_is_accepted() {
        let capture = crate::temple::market::allflame();

        assert_eq!(
            judge_market(Some(&capture), Some("Allflame")),
            MarketPoll::Accepted
        );
    }

    /// The DEBUG/PROD edge case: the server prices a league this app is not on,
    /// so nothing in the payload may be shown as current.
    ///
    /// Fails if the league check is dropped — which is the failure that puts
    /// last league's prices on this league's board after one click of the
    /// server toggle.
    #[test]
    fn a_payload_for_another_league_is_discarded() {
        let capture = crate::temple::market::allflame();

        assert_eq!(
            judge_market(Some(&capture), Some("Mirage")),
            MarketPoll::WrongLeague
        );
    }

    /// The league is compared on its trimmed name, so whitespace either side of
    /// an otherwise identical name is not a league change.
    #[test]
    fn surrounding_whitespace_is_not_a_league_difference() {
        let capture = crate::temple::market::allflame();

        assert_eq!(
            judge_market(Some(&capture), Some("  Allflame ")),
            MarketPoll::Accepted
        );
    }

    /// The cold server: reachable, 200, and nothing observed yet.
    ///
    /// Told apart from a failed fetch by `asOf` alone, exactly as the endpoint's
    /// own contract says — the floor rule is served warm
    /// or cold, so the arrays cannot be the test.
    #[test]
    fn a_cold_server_answer_is_not_mistaken_for_a_priced_one() {
        let cold: MarketInput = serde_json::from_str(
            r#"{"league":"Allflame","asOf":null,"floor":0,"rooms":[],"items":[]}"#,
        )
        .expect("the cold answer parses");

        assert_eq!(judge_market(Some(&cold), Some("Allflame")), MarketPoll::Cold);
    }

    /// A failed request is its own verdict, and NOT a cold server: the two keep
    /// the stored read alike but say different things to the user, and the
    /// poller's transition line names which one is in force.
    #[test]
    fn a_failed_request_is_told_apart_from_a_cold_server() {
        assert_eq!(judge_market(None, Some("Allflame")), MarketPoll::Failed);
    }

    /// Before the league resolves there is nothing to key a payload on, so the
    /// poll reports that rather than accepting one unchecked.
    #[test]
    fn an_unresolved_league_leaves_the_payload_unjudged() {
        let capture = crate::temple::market::allflame();

        assert_eq!(judge_market(Some(&capture), None), MarketPoll::Unresolved);
    }

    /// An accepted payload becomes the stored read, whole.
    ///
    /// The 86 room-tier lines are the assertion because they are what the
    /// valuation consumes: a store that kept the envelope and dropped the rooms
    /// would leave every board on base values with nothing on screen to say so.
    #[test]
    fn an_accepted_payload_replaces_the_stored_read() {
        let stored = Mutex::new(MarketInput::none());

        let changed = store_market(
            &stored,
            MarketPoll::Accepted,
            Some(crate::temple::market::allflame()),
        );

        assert!(changed);
        let held = stored.lock().unwrap();
        assert_eq!(held.rooms.len(), 86);
        assert_eq!(held.league, "Allflame");
        assert_eq!(held.as_of_ms(), Some(1_788_665_199_649));
    }

    /// A failed poll keeps the last good read.
    ///
    /// The read then ages on its own through `MarketInput::aged_at`, which is
    /// the whole reason blanking it would be wrong: prices minutes old are
    /// still prices, and one timed-out request is not evidence about them.
    /// Fails if `store_market` starts writing on a failure.
    #[test]
    fn a_failed_poll_keeps_the_last_good_read() {
        let stored = Mutex::new(crate::temple::market::allflame());

        let changed = store_market(&stored, MarketPoll::Failed, None);

        assert!(!changed);
        assert_eq!(stored.lock().unwrap().rooms.len(), 86, "still the last good read");
    }

    /// A cold server keeps the last good read too.
    ///
    /// A server that has not recomputed yet — a restart, a cache eviction —
    /// says nothing about the prices already in hand, so they stand and age
    /// into stale on their own. With nothing stored the read is already
    /// `MarketInput::none()`, which is what makes the board fall back to base
    /// values and the overlay say unavailable. Fails if `store_market` starts
    /// writing `none()` on a cold answer.
    #[test]
    fn a_cold_server_keeps_the_last_good_read() {
        let stored = Mutex::new(crate::temple::market::allflame());

        let changed = store_market(&stored, MarketPoll::Cold, None);

        assert!(!changed);
        assert_eq!(
            stored.lock().unwrap().rooms.len(),
            86,
            "still the last good read"
        );
    }

    /// So does a wrong-league payload — the point of the check is that nothing
    /// from the old league survives it, not merely that the new payload is
    /// refused.
    #[test]
    fn a_wrong_league_payload_blanks_the_stored_read() {
        let stored = Mutex::new(crate::temple::market::allflame());

        let changed = store_market(
            &stored,
            MarketPoll::WrongLeague,
            Some(crate::temple::market::allflame()),
        );

        assert!(changed);
        assert_eq!(*stored.lock().unwrap(), MarketInput::none());
    }

    /// Re-storing the same read reports no change, so a poll that learned
    /// nothing new does not wake every overlay's poll.
    #[test]
    fn an_unchanged_read_is_not_republished() {
        let stored = Mutex::new(crate::temple::market::allflame());

        let changed = store_market(
            &stored,
            MarketPoll::Accepted,
            Some(crate::temple::market::allflame()),
        );

        assert!(!changed);
    }

    /// Switching servers drops the market outright, before any poll runs.
    ///
    /// The prices in hand were read against the OTHER server's league, so they
    /// must be gone in the same click rather than at the next poll five minutes
    /// later. Fails if the reset stops writing.
    #[test]
    fn switching_servers_drops_the_market_in_hand() {
        let stored = Mutex::new(crate::temple::market::allflame());

        let changed = drop_market(&stored);

        assert!(changed);
        assert_eq!(*stored.lock().unwrap(), MarketInput::none());
    }

    /// …and a second switch with nothing held reports no change, so the reset
    /// does not spend an emit on every url save.
    #[test]
    fn switching_servers_with_no_market_held_changes_nothing() {
        let stored = Mutex::new(MarketInput::none());

        assert!(!drop_market(&stored));
    }

    /// A poll publish moves `poll` and leaves the READ's market exactly where
    /// it was.
    ///
    /// The whole point of two fields. A poll that also wrote `market` would put
    /// its own line over a board it knows nothing about — "prices 3 min old"
    /// across a board on the cold grade ladder, and after a DEBUG/PROD switch
    /// "prices unavailable — base values" across a board of real chaos figures.
    /// Fails if `apply_market_view` writes both.
    #[test]
    fn a_poll_publish_moves_the_poll_view_and_not_the_read_it_found() {
        let read = crate::temple::slice::market_view(&crate::temple::market::allflame());
        let mut slice = TempleSlice { market: read.clone(), ..TempleSlice::default() };
        // What a server switch publishes: nothing priced, nothing observed.
        let after_switch = crate::temple::slice::market_view(&MarketInput::none());
        assert_ne!(after_switch, read, "precondition: the two views differ");

        apply_market_view(&mut slice, after_switch.clone());

        assert_eq!(slice.market, read, "the read's own market is untouched");
        assert_eq!(slice.poll, after_switch, "and the poll's view is what moved");
    }

    // The golden is written by internal/server/handlers/temple_contract_test.go; regenerate with
    // docker compose exec -T app go test ./internal/server/handlers/ -run TestTempleMarket_contractGolden -update
    fn temple_golden() -> String {
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../contracts/temple-market.json")).unwrap()
    }

    #[test]
    fn the_go_golden_fills_every_defaulted_market_field() {
        let market: MarketInput = serde_json::from_str(&temple_golden()).unwrap();

        assert!(!market.league.is_empty(), "league parsed empty: Go renamed `league`?");
        assert!(market.as_of.is_some(), "asOf parsed None: Go renamed `asOf`?");
        assert!(market.floor > 0.0, "floor parsed 0: Go renamed `floor`?");
        assert!(!market.rooms.is_empty(), "rooms parsed empty: Go renamed `rooms`?");
        assert!(!market.items.is_empty(),"items parsed empty: Go renamed `items`?");
    }

    #[test]
    fn the_go_golden_carries_every_key_the_wire_market_consumes() {
        let golden: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&temple_golden()).unwrap();

        for key in ["league", "asOf", "floor", "rooms", "items"] {
            assert!(golden.contains_key(key), "golden lacks `{key}`, which WireMarket reads");
        }
    }
}
