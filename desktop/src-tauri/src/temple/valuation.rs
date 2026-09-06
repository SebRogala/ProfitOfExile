//! What one room-tier is worth, in chaos (POE-257).
//!
//! Epic lock L1 (POE-124): **every room value in this crate is chaos**, and
//! Vertolka's letter grade is the fallback for a room nothing has priced — never
//! a second currency running alongside the first. This module is the one place
//! the number is computed, so the value the advisor ranks on and the value the
//! overlay shows are the same object (POE-260, D6).
//!
//! # The formula
//!
//! At tier 3, for one line:
//!
//! ```text
//! sale   = the feed price above the room-line floor          (market.rs)
//! drops  = drops_weight x SUM( expected count x unit price )  (drops.rs x market.rs)
//! bonus  = quantity% x c_per_quantity + rarity% x c_per_rarity
//! total  = sale + drops + bonus
//! ```
//!
//! Tiers 1 and 2 are [`Knobs::tier_fraction`] (80 % by default) of their line's
//! tier-3 total, drivers included — epic lock L2. That is deliberately not
//! "their own quantity bonus": what a tier-1 room is worth at *building* time is
//! the line being present at all, because the 50 % double-tier node and the
//! Timelines scarab act on the line, not on the tier.
//!
//! # Three terms have no price, and say so rather than reading zero
//!
//! - **Pack size.** [`drops::TierDrops::pack_size_pct`](super::drops::TierDrops::pack_size_pct)
//!   carries poedb's measured percentage, and nobody has stated what a point of
//!   it is worth. There is no `c_per_pack_size` knob and no driver: inventing a
//!   rate would put a fabricated number in the total, and pricing it at zero
//!   would claim pack size is worthless. It is simply not in the sum, and it is
//!   the one stated exception to the third bullet below: a term left out is
//!   still listed, but a driver has to be listed against a rate, and pack size
//!   has none.
//! - **The raw poedb chance integers.**
//!   [`vial_chance_raw`](super::drops::TierDrops::vial_chance_raw) and
//!   [`mod_item_chance_raw`](super::drops::TierDrops::mod_item_chance_raw) run 7
//!   to 2815 with no scale printed anywhere. They are never multiplied here.
//! - **A term with no count or no price** contributes nothing and is still
//!   listed as a [`Driver`] with `chaos: None`, so the explanation box shows
//!   what was left out instead of quietly showing a smaller total.
//!
//! # Fallback is never zero
//!
//! Epic lock L4: a missing or stale market falls back to the preset's base
//! value. The ladder stands in only where **nothing at all was summed** — no
//! sale, no priced drop, and no bonus. A bonus is a summed term like any
//! other: its percentage is poedb's measured number and the rate that prices
//! it is a knob the player sets, so a room whose only priced term is its
//! quantity bonus is worth that bonus and not its letter. Letting the letter
//! win there would put a third-party grade above the feed, which is the one
//! thing [`Grade::fallback_chaos`](super::rooms::Grade::fallback_chaos) says it
//! must never do — visibly so under a rusher's `drops_weight = 0`, where an A+
//! rung would otherwise outrank Doryani's Institute's real 390 c. Where the
//! ladder does stand in, [`RoomValue::priced`] says [`Priced::Fallback`] and
//! the rung is the only driver left carrying chaos. A room's total is never
//! `NaN` and never negative.
//!
//! # The Mirage vial spike is a drop-table question, not a formula one
//!
//! At Vertolka's 0.1 vials per run an 808 c Vial of Summoning adds 74.8 c to
//! Sanctum of Immortality tier 3 (60.75 -> 135.55 on the committed capture),
//! which cannot rival Doryani's Institute at 390. The epic's "a high-chance
//! vial room rivals Doryani" case needs about 0.41 vials per run, or a scale
//! for the poedb chance integers to derive one from — a number the drop table
//! owes, not a change to this formula.
//!
//! # What is a guess, and what is measured
//!
//! [`RoomValue::guessed`] is true when any driver that actually contributed
//! chaos rests on somebody's estimate. Three knobs are estimates by
//! construction — [`Knobs::c_per_quantity`], [`Knobs::c_per_rarity`] and the
//! grade ladder — so a room whose value comes from a quantity bonus or from the
//! ladder is flagged even though the underlying percentage is poedb's measured
//! number. That is the honest reading: the *percentage* is measured, the *rate*
//! that turns it into chaos is not.

use std::collections::BTreeMap;

use super::drops::Estimate;
use super::market::{ItemQuote, MarketInput};
use super::rooms::{RoomLine, LINES};
use super::strategy::Tier;

// ---------------------------------------------------------------- the knobs --

/// The per-user rates the formula is parameterised by (POE-257 D4).
///
/// Every one of these is a number a player may reasonably disagree with, which
/// is why it is a field rather than a constant — the module's recorded decision
/// is "one base strategy, per-user configurables" (`temple/mod.rs`). POE-259
/// persists them per preset; nothing here reads or writes settings.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub struct Knobs {
    /// What a tier-1 or tier-2 room is worth as a fraction of its line's tier-3
    /// total. Epic lock L2's 80 %: line presence is what the double-tier node
    /// and the Timelines scarab act on. Clamped to `0.0..=1.0` — a tier-1 room
    /// worth more than its own tier-3 would invert the upgrade advice — and a
    /// non-finite or negative value falls back to this default rather than to
    /// zero (see [`tier_fraction`]).
    pub tier_fraction: f64,
    /// Chaos per point of `increased Quantity of Items found in this Area`.
    /// **A guess** — Vertolka stated no rate, and nobody has measured one.
    pub c_per_quantity: f64,
    /// Chaos per point of `increased Rarity of Items found in this Area`.
    /// **A guess**, same standing as [`Self::c_per_quantity`].
    pub c_per_rarity: f64,
    /// A global multiplier on the whole drops term. A rusher who never opens a
    /// chest sets it to `0.0`, which reduces the ranking to sale value alone.
    pub drops_weight: f64,
    /// What the Locus + Doryani pair is worth ON TOP of the sum of the two
    /// rooms' own values. Zero by default: the orchestrator settled that the
    /// combination premium is the sum of the two above-floor deltas and no
    /// more. Read by the profile bridge (POE-257 D5), not by this module.
    pub combo_premium: f64,
}

impl Default for Knobs {
    fn default() -> Knobs {
        Knobs {
            tier_fraction: 0.8,
            c_per_quantity: 0.5,
            c_per_rarity: 0.25,
            drops_weight: 1.0,
            combo_premium: 0.0,
        }
    }
}

/// A rate that cannot poison a total: anything non-finite or negative reads as
/// zero. A hand-edited settings file is the expected source of such a value.
///
/// Zero rather than the [`Knobs::default`] value on purpose. Zero is a
/// supported setting for the knobs this governs — a rusher really does set
/// [`Knobs::drops_weight`] to it, and a player who thinks quantity is worthless
/// really does set [`Knobs::c_per_quantity`] to it — so a malformed value lands
/// somewhere the player can already reach, and silently substituting a default
/// would hide the malformed entry instead of surfacing it. POE-259 owns the
/// warning that names the offending key when the table loads, and owns
/// rejecting the value at the UI before it is ever written.
///
/// [`Knobs::tier_fraction`] is the one knob this does not govern; see
/// [`tier_fraction`].
fn rate(value: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
}

/// [`Knobs::tier_fraction`], made safe — and the one knob whose malformed
/// value is not read as zero.
///
/// Zero is kept, because it is a position a player can hold: a tier-1 room is
/// worth nothing to somebody who only ever builds tier 3. A non-finite or
/// negative fraction is nobody's position, and reading it as zero would price
/// every tier-1 and tier-2 room in the game at nothing off one malformed
/// character — a whole-table wipe wearing the face of an opinion. Those fall
/// back to [`Knobs::default`]'s 80 %. POE-259 owns rejecting the value at the
/// UI so it never reaches here.
fn tier_fraction(value: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        value.min(1.0)
    } else {
        Knobs::default().tier_fraction
    }
}

// -------------------------------------------------------------- the drivers --

/// Which term of the formula a [`Driver`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub enum DriverKind {
    /// The room-tier's own feed price above the floor.
    Sale,
    /// The tier-3 chest unique.
    UniqueDrop,
    /// The vial the line's architect rolls for.
    VialDrop,
    /// The architect's signature rare, priced from its manual base price
    /// because poe.ninja prices no rare.
    ModItem,
    /// `increased Quantity of Items` x [`Knobs::c_per_quantity`].
    QuantityBonus,
    /// `increased Rarity of Items` x [`Knobs::c_per_rarity`].
    RarityBonus,
    /// Tier 1 / tier 2 as a fraction of the line's tier-3 total.
    TierFraction,
    /// Nothing was priced, so the letter grade's ladder value stood in.
    GradeFallback,
}

/// One line of the explanation box: a term of the sum, and everything the
/// player needs to judge it.
///
/// A driver with `chaos: None` contributed nothing — either nobody has stated
/// the count, or the feed carries no price for the item. It is still listed,
/// because a term silently dropped from a total is indistinguishable from a
/// term worth zero.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub struct Driver {
    /// Which term of the formula this is.
    pub kind: DriverKind,
    /// What is being priced, as the source spells it: the poe.ninja room or
    /// item name, the game's own bonus wording, or the grade letter.
    pub name: String,
    /// Expected count per run, the percentage, or the tier fraction. `None`
    /// where the term has no count (a sale) or nobody stated one.
    pub count: Option<f64>,
    /// Chaos per unit of [`Self::count`]. `None` where nothing priced it.
    pub unit_price: Option<f64>,
    /// What this term added to the total. `None` when it added nothing because
    /// a count or a price was missing.
    pub chaos: Option<f64>,
    /// Whether this term rests on somebody's estimate rather than on measured
    /// data — a [`Basis::Guess`](super::drops::Basis::Guess) count, one of the
    /// two unmeasured bonus rates, or the grade ladder.
    pub guessed: bool,
    /// POE-131's thin-market flag on the price behind this term.
    pub low_confidence: bool,
    /// POE-252: the price behind this term is a trailing-window median rather
    /// than the newest print.
    pub window_priced: bool,
}

// ---------------------------------------------------------------- the value --

/// How complete the sum behind a [`RoomValue`] is.
///
/// This is about the SUM, not only about prices: a term whose count nobody has
/// stated is as absent from the total as a term the feed cannot price, and the
/// player is owed the same warning either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub enum Priced {
    /// Every term this room names turned into chaos.
    Market,
    /// At least one named term did not, and at least one did.
    Partial,
    /// Nothing did. The total is the grade ladder's value (epic lock L4).
    Fallback,
}

/// What one room-tier is worth, and what that number is made of.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub struct RoomValue {
    /// The feed price above the room-line floor. Zero on
    /// [`Priced::Fallback`].
    pub sale: f64,
    /// Expected drop value per run, after [`Knobs::drops_weight`]. Zero on
    /// [`Priced::Fallback`].
    pub drops: f64,
    /// The quantity / rarity bonus in chaos. Zero on [`Priced::Fallback`] by
    /// construction: a room that priced a bonus at anything is not on that
    /// path, because a summed term outranks the ladder.
    pub bonus: f64,
    /// `sale + drops + bonus`, or the grade ladder's value on
    /// [`Priced::Fallback`]. Never `NaN`, never negative.
    pub total: f64,
    /// Every term of the sum, including the ones that contributed nothing.
    pub drivers: Vec<Driver>,
    /// Whether any term that actually contributed chaos rests on an estimate.
    pub guessed: bool,
    /// How complete the sum is.
    pub priced: Priced,
}

/// The whole 25 x 3 table for one market read and one set of knobs.
///
/// One table per read is the point (POE-257 D6): the advisor ranks on it and
/// the overlay shows from it, so the number the player sees is the number the
/// recommendation used.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub struct Valued {
    lines: BTreeMap<&'static str, [RoomValue; 3]>,
}

impl Valued {
    /// Value every one of the 25 lines at all three tiers.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn compute(market: &MarketInput, knobs: &Knobs) -> Valued {
        let mut lines = BTreeMap::new();
        for line in LINES.iter() {
            let t3 = value_tier3(line, market, knobs);
            let t1 = scale_from_tier3(line, &t3, knobs);
            let t2 = scale_from_tier3(line, &t3, knobs);
            lines.insert(line.key(), [t1, t2, t3]);
        }
        Valued { lines }
    }

    /// One room-tier's value. `None` for an unknown key and for
    /// [`Tier::T0`] — tier 0 is filler and belongs to no line.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn get(&self, key: &str, tier: Tier) -> Option<&RoomValue> {
        let tiers = self.lines.get(key)?;
        match tier.get() {
            1..=3 => Some(&tiers[tier.get() as usize - 1]),
            _ => None,
        }
    }

    /// Every line's three tiers, tier 1 first, in key order.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn lines(&self) -> impl Iterator<Item = (&'static str, &[RoomValue; 3])> + '_ {
        self.lines.iter().map(|(key, tiers)| (*key, tiers))
    }
}

// -------------------------------------------------------------- the formula --

fn value_tier3(line: &RoomLine, market: &MarketInput, knobs: &Knobs) -> RoomValue {
    let room = line
        .name(Tier::T3)
        .expect("every line has a tier-3 room name");
    let drops_row = line.drops();
    let tier = drops_row
        .tier(Tier::T3)
        .expect("every drops row has a tier-3 entry");

    let mut drivers = Vec::new();

    // --- sale ---
    let quote = market.room(room, Tier::T3);
    let sale = market.sale_delta(room, Tier::T3);
    drivers.push(Driver {
        kind: DriverKind::Sale,
        name: room.to_string(),
        count: None,
        unit_price: quote.map(|q| q.chaos),
        chaos: quote.map(|_| sale),
        guessed: false,
        low_confidence: quote.is_some_and(|q| q.low_confidence),
        window_priced: false,
    });

    // --- drops ---
    let mut drops = 0.0;
    let weight = rate(knobs.drops_weight);

    if let Some(unique) = drops_row.unique() {
        drops += push_drop(
            &mut drivers,
            DriverKind::UniqueDrop,
            unique,
            tier.uniques_per_run(),
            market.price(unique).map(UnitPrice::from_feed),
            weight,
        );
    }
    if let Some(vial) = drops_row.vial() {
        drops += push_drop(
            &mut drivers,
            DriverKind::VialDrop,
            vial,
            tier.vials_per_run(),
            market.price(vial).map(UnitPrice::from_feed),
            weight,
        );
    }
    if let Some(temple_mod) = drops_row.temple_mod() {
        // The one price that does not come from the feed: poe.ninja publishes
        // no line for a mod-rolled rare, so drops.rs carries a manual base
        // price and it is Vertolka's guess.
        let price = temple_mod.base_price_chaos().map(|base| UnitPrice {
            chaos: base.value(),
            guessed: base.is_guess(),
            low_confidence: false,
            window_priced: false,
        });
        drops += push_drop(
            &mut drivers,
            DriverKind::ModItem,
            temple_mod.item_hint(),
            temple_mod.per_run(),
            price,
            weight,
        );
    }

    // --- bonus ---
    let mut bonus = 0.0;
    bonus += push_bonus(
        &mut drivers,
        DriverKind::QuantityBonus,
        "increased Quantity of Items found in this Area",
        tier.quantity_pct(),
        rate(knobs.c_per_quantity),
    );
    bonus += push_bonus(
        &mut drivers,
        DriverKind::RarityBonus,
        "increased Rarity of Items found in this Area",
        tier.rarity_pct(),
        rate(knobs.c_per_rarity),
    );

    // --- fallback (epic lock L4) ---
    //
    // Only where NOTHING was summed. Every term above is non-negative by
    // construction (`rate` floors each factor at zero), so this reads exactly
    // "sale + drops + bonus == 0" without a float equality. A room that
    // produced any chaos at all keeps it, its bonus included — see the module
    // header for why the letter may not win there.
    let summed = sale + drops + bonus;
    if summed <= 0.0 {
        let grade = line.grade();
        let ladder = grade.fallback_chaos();
        // The rung is the whole total, so it must be the whole accounting: a
        // term still carrying `Some(0.0)` would make the drivers the box
        // prints disagree with the number the advisor ranked on. Counts and
        // unit prices stay — what was tried is still worth showing.
        for driver in drivers.iter_mut() {
            driver.chaos = None;
        }
        drivers.push(Driver {
            kind: DriverKind::GradeFallback,
            name: grade.as_str().to_string(),
            count: None,
            unit_price: None,
            chaos: Some(ladder),
            guessed: true,
            low_confidence: false,
            window_priced: false,
        });
        return RoomValue {
            sale: 0.0,
            drops: 0.0,
            bonus: 0.0,
            total: ladder,
            guessed: true,
            priced: Priced::Fallback,
            drivers,
        };
    }

    let priced = if drivers
        .iter()
        .filter(|d| is_sum_term(d.kind))
        .all(|d| d.chaos.is_some())
    {
        Priced::Market
    } else {
        Priced::Partial
    };

    RoomValue {
        sale,
        drops,
        bonus,
        total: summed,
        guessed: contributed_a_guess(&drivers),
        priced,
        drivers,
    }
}

/// The terms whose completeness [`Priced`] is about. The bonus terms are not
/// among them: they are computed from a measured percentage and a knob, so they
/// are never "missing" in the sense a price is, and letting them decide would
/// make every room with no quantity line read as partially priced.
fn is_sum_term(kind: DriverKind) -> bool {
    matches!(
        kind,
        DriverKind::Sale | DriverKind::UniqueDrop | DriverKind::VialDrop | DriverKind::ModItem
    )
}

fn contributed_a_guess(drivers: &[Driver]) -> bool {
    drivers.iter().any(|d| d.guessed && d.chaos.is_some())
}

/// What one unit of a drop term costs, and everything the driver has to record
/// about where that figure came from.
#[derive(Debug, Clone, Copy)]
struct UnitPrice {
    chaos: f64,
    guessed: bool,
    low_confidence: bool,
    window_priced: bool,
}

impl UnitPrice {
    /// A live feed price — measured by definition.
    fn from_feed(quote: &ItemQuote) -> UnitPrice {
        UnitPrice {
            chaos: quote.chaos,
            guessed: false,
            low_confidence: quote.low_confidence,
            window_priced: quote.window_priced,
        }
    }
}

/// Push one drop term and return what it added to the total.
fn push_drop(
    drivers: &mut Vec<Driver>,
    kind: DriverKind,
    name: &str,
    count: Option<Estimate>,
    price: Option<UnitPrice>,
    weight: f64,
) -> f64 {
    let chaos = match (count, price) {
        (Some(count), Some(price)) => Some(weight * rate(count.value()) * rate(price.chaos)),
        _ => None,
    };
    drivers.push(Driver {
        kind,
        name: name.to_string(),
        count: count.map(Estimate::value),
        unit_price: price.map(|price| price.chaos),
        chaos,
        guessed: count.is_some_and(Estimate::is_guess) || price.is_some_and(|price| price.guessed),
        low_confidence: price.is_some_and(|price| price.low_confidence),
        window_priced: price.is_some_and(|price| price.window_priced),
    });
    chaos.unwrap_or(0.0)
}

/// Push one bonus term and return what it added.
///
/// Always `guessed`: the percentage is poedb's measured number, but the rate
/// that turns a percentage into chaos is nobody's measurement.
fn push_bonus(
    drivers: &mut Vec<Driver>,
    kind: DriverKind,
    name: &str,
    pct: Option<Estimate>,
    per_point: f64,
) -> f64 {
    let Some(pct) = pct else {
        return 0.0;
    };
    let chaos = rate(pct.value()) * per_point;
    drivers.push(Driver {
        kind,
        name: name.to_string(),
        count: Some(pct.value()),
        unit_price: Some(per_point),
        chaos: Some(chaos),
        guessed: true,
        low_confidence: false,
        window_priced: false,
    });
    chaos
}

/// Tier 1 and tier 2 from their line's tier-3 value (epic lock L2).
///
/// The drivers are copied rather than recomputed, and a [`DriverKind::TierFraction`]
/// driver records the scaling — so the box explains a tier-1 room as "80 % of
/// what Locus of Corruption is worth", which is what the number means.
fn scale_from_tier3(line: &RoomLine, tier3: &RoomValue, knobs: &Knobs) -> RoomValue {
    let fraction = tier_fraction(knobs.tier_fraction);
    let mut drivers = tier3.drivers.clone();
    drivers.push(Driver {
        kind: DriverKind::TierFraction,
        name: line
            .name(Tier::T3)
            .expect("every line has a tier-3 room name")
            .to_string(),
        count: Some(fraction),
        unit_price: Some(tier3.total),
        chaos: Some(fraction * tier3.total),
        guessed: false,
        low_confidence: false,
        window_priced: false,
    });

    RoomValue {
        sale: tier3.sale * fraction,
        drops: tier3.drops * fraction,
        bonus: tier3.bonus * fraction,
        total: tier3.total * fraction,
        guessed: tier3.guessed,
        priced: tier3.priced,
        drivers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::market::allflame;
    use crate::temple::rooms::Grade;

    fn tier(n: u8) -> Tier {
        Tier::new(n).expect("test tier is 0..=3")
    }

    fn value_of(valued: &Valued, key: &str, n: u8) -> RoomValue {
        valued
            .get(key, tier(n))
            .unwrap_or_else(|| panic!("no value for {key} tier {n}"))
            .clone()
    }

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }

    fn driver<'a>(value: &'a RoomValue, kind: DriverKind) -> &'a Driver {
        value
            .drivers
            .iter()
            .find(|d| d.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?} driver among {:?}", value.drivers))
    }

    /// The Allflame capture with one item repriced — the only way to test a
    /// price move without inventing a whole market.
    fn allflame_with(item: &str, chaos: f64) -> MarketInput {
        let mut market = allflame();
        let quote = market
            .items
            .get_mut(item)
            .unwrap_or_else(|| panic!("the capture prices {item}"));
        quote.chaos = chaos;
        market
    }

    #[test]
    fn a_tier_three_room_sums_its_sale_its_drops_and_its_bonus() {
        // Sanctum of Immortality tier 3 on the 2026-09-06 capture: at the floor
        // (no sale), 0.25 x Mask of the Spirit Drinker at 195 c, 0.1 x Vial of
        // Summoning at 60 c, +6% quantity and +12% rarity.
        let value = value_of(
            &Valued::compute(&allflame(), &Knobs::default()),
            "sanctum_of_immortality",
            3,
        );

        assert_eq!(value.sale, 0.0);
        assert!(close(value.drops, 54.75), "drops was {}", value.drops);
        assert!(close(value.bonus, 6.0), "bonus was {}", value.bonus);
        assert!(close(value.total, 60.75), "total was {}", value.total);
    }

    #[test]
    fn a_priced_drop_driver_names_the_item_its_count_and_its_unit_price() {
        let value = value_of(
            &Valued::compute(&allflame(), &Knobs::default()),
            "sanctum_of_immortality",
            3,
        );

        let unique = driver(&value, DriverKind::UniqueDrop);
        assert_eq!(unique.name, "Mask of the Spirit Drinker");
        assert_eq!(unique.count, Some(0.25));
        assert_eq!(unique.unit_price, Some(195.0));
        assert_eq!(unique.chaos, Some(48.75));
        assert!(unique.guessed, "Vertolka's 0.25 per run is his estimate");
    }

    #[test]
    fn the_manual_base_price_of_an_architects_rare_is_the_mod_item_term() {
        // Crucible of Flame is the one line anybody priced a temple-mod item
        // on: 2 temple gloves per run at 30 c, both Vertolka's guesses.
        let value = value_of(
            &Valued::compute(&allflame(), &Knobs::default()),
            "crucible_of_flame",
            3,
        );

        let mod_item = driver(&value, DriverKind::ModItem);
        assert_eq!(mod_item.count, Some(2.0));
        assert_eq!(mod_item.unit_price, Some(30.0));
        assert_eq!(mod_item.chaos, Some(60.0));
        // 0.25 x Story of the Vaal 5 c + 0.1 x Vial of Fate 1 c + 60.
        assert!(close(value.drops, 61.35), "drops was {}", value.drops);
    }

    #[test]
    fn a_term_with_no_price_contributes_nothing_and_is_still_listed() {
        // Locus of Corruption drops Shadowstitch, which poe.ninja publishes no
        // line for, so POE-255 serves no price for it at all.
        let value = value_of(
            &Valued::compute(&allflame(), &Knobs::default()),
            "corruption",
            3,
        );

        let unique = driver(&value, DriverKind::UniqueDrop);
        assert_eq!(unique.name, "Shadowstitch");
        assert_eq!(unique.unit_price, None);
        assert_eq!(unique.chaos, None);
        assert_eq!(value.drops, 0.0);
        assert_eq!(value.priced, Priced::Partial);
    }

    #[test]
    fn a_term_with_a_price_but_no_stated_count_contributes_nothing() {
        // Locus rolls for Vial of Sacrifice, priced at 428 c on the capture,
        // and nobody has stated how often it drops.
        let value = value_of(
            &Valued::compute(&allflame(), &Knobs::default()),
            "corruption",
            3,
        );

        let vial = driver(&value, DriverKind::VialDrop);
        assert_eq!(vial.name, "Vial of Sacrifice");
        assert_eq!(vial.unit_price, Some(428.0));
        assert_eq!(vial.count, None);
        assert_eq!(vial.chaos, None);
        assert!(vial.low_confidence, "11 listings, POE-131 thin");
        assert!(vial.window_priced, "priced off the 24-hour window, POE-252");
    }

    #[test]
    fn an_above_floor_room_is_worth_its_sale_delta() {
        // Locus of Corruption tier 3: 856 c against a floor of 10.
        let value = value_of(
            &Valued::compute(&allflame(), &Knobs::default()),
            "corruption",
            3,
        );

        assert_eq!(value.sale, 846.0);
        assert_eq!(value.total, 846.0);
        assert_eq!(driver(&value, DriverKind::Sale).unit_price, Some(856.0));
    }

    #[test]
    fn a_room_whose_only_value_is_its_sale_is_fully_priced() {
        // Doryani's Institute names no unique, no vial and no bonus, so its
        // whole value is the 390 c the feed pays above the floor.
        let value = value_of(&Valued::compute(&allflame(), &Knobs::default()), "gem", 3);

        assert_eq!(value.total, 390.0);
        assert_eq!(value.priced, Priced::Market);
        assert!(!value.guessed, "a live sale price is nobody's estimate");
    }

    #[test]
    fn tier_one_and_tier_two_are_the_tier_fraction_of_tier_three() {
        let valued = Valued::compute(&allflame(), &Knobs::default());

        let t3 = value_of(&valued, "corruption", 3);
        let t2 = value_of(&valued, "corruption", 2);
        let t1 = value_of(&valued, "corruption", 1);

        assert!(close(t1.total, 0.8 * t3.total), "t1 was {}", t1.total);
        assert!(close(t2.total, 0.8 * t3.total), "t2 was {}", t2.total);
        assert!(close(t1.sale, 0.8 * t3.sale), "t1 sale was {}", t1.sale);
    }

    #[test]
    fn a_lower_tier_records_the_fraction_it_was_scaled_by() {
        let valued = Valued::compute(&allflame(), &Knobs::default());
        let t1 = value_of(&valued, "corruption", 1);

        let fraction = driver(&t1, DriverKind::TierFraction);
        assert_eq!(fraction.name, "Locus of Corruption");
        assert_eq!(fraction.count, Some(0.8));
        assert_eq!(fraction.unit_price, Some(846.0));
        assert!(
            t1.drivers.iter().any(|d| d.kind == DriverKind::UniqueDrop),
            "the tier-3 drivers are carried down, not recomputed"
        );
    }

    #[test]
    fn a_tier_fraction_above_one_cannot_make_a_lower_tier_beat_its_own_tier_three() {
        let knobs = Knobs {
            tier_fraction: 4.0,
            ..Knobs::default()
        };
        let valued = Valued::compute(&allflame(), &knobs);

        assert_eq!(
            value_of(&valued, "corruption", 1).total,
            value_of(&valued, "corruption", 3).total
        );
    }

    #[test]
    fn a_non_finite_tier_fraction_falls_back_to_the_default_fraction() {
        // A hand-edited settings file is the expected source of one. Reading
        // it as zero would price every tier-1 and tier-2 room in the game at
        // nothing, which reads like a deliberate setting rather than a fault.
        let knobs = Knobs {
            tier_fraction: f64::NAN,
            ..Knobs::default()
        };
        let valued = Valued::compute(&allflame(), &knobs);

        let t1 = value_of(&valued, "corruption", 1).total;
        let t3 = value_of(&valued, "corruption", 3).total;
        assert!(close(t1, 0.8 * t3), "{t1} is not the default 80 % of {t3}");
    }

    #[test]
    fn a_zero_tier_fraction_is_kept_rather_than_defaulted() {
        // Zero is a position a player can hold — a tier-1 room is worth
        // nothing to somebody who only ever builds tier 3 — so it is the one
        // falsy fraction the guard must not replace.
        let knobs = Knobs {
            tier_fraction: 0.0,
            ..Knobs::default()
        };
        let valued = Valued::compute(&allflame(), &knobs);

        assert_eq!(value_of(&valued, "corruption", 1).total, 0.0);
        assert_eq!(value_of(&valued, "corruption", 3).total, 846.0);
    }

    #[test]
    fn a_zero_drops_weight_removes_the_whole_drops_term() {
        // The rusher's setting: he never opens a chest, so the drop value of a
        // room is worth nothing to him.
        let knobs = Knobs {
            drops_weight: 0.0,
            ..Knobs::default()
        };

        let priced = value_of(
            &Valued::compute(&allflame(), &Knobs::default()),
            "crucible_of_flame",
            3,
        );
        let valued = Valued::compute(&allflame(), &knobs);
        let rushed = value_of(&valued, "crucible_of_flame", 3);

        assert!(close(priced.drops, 61.35), "drops was {}", priced.drops);
        assert_eq!(rushed.drops, 0.0);
        // And what is left is still the market's ranking: Crucible keeps its
        // 6 c quantity bonus instead of falling to the 400 c its A+ rung would
        // have stood in for, which would have put it above the 390 c the feed
        // actually pays for Doryani's Institute.
        assert!(close(rushed.total, 6.0), "total was {}", rushed.total);
        assert_eq!(value_of(&valued, "gem", 3).total, 390.0, "Doryani's sale");
        assert_eq!(
            value_of(&valued, "corruption", 3).total,
            846.0,
            "Locus's sale"
        );
        assert!(rushed.total < value_of(&valued, "gem", 3).total);
    }

    #[test]
    fn a_room_with_nothing_priced_falls_back_to_its_grade() {
        // Museum of Artefacts is the emptiest line on the sheet: it sits at
        // the floor, names no unique, no vial and no temple-mod item, and
        // poedb measures no area bonus on it. Nothing about it is priced, so
        // the letter is all there is.
        let value = value_of(
            &Valued::compute(&allflame(), &Knobs::default()),
            "museum_of_artefacts",
            3,
        );

        assert_eq!(value.priced, Priced::Fallback);
        assert_eq!(value.total, 2.0);
        assert_eq!(value.sale, 0.0);
        assert_eq!(value.drops, 0.0);
        assert_eq!(value.bonus, 0.0);
        assert_eq!(driver(&value, DriverKind::GradeFallback).name, "D");
    }

    #[test]
    fn a_bonus_only_room_is_worth_its_bonus_not_its_letter() {
        // Factory tier 3 sells at the floor and drops nothing anybody has
        // quantified, but poedb measures +66 % quantity and +12 % rarity on
        // it. At the default rates that is 36 c, and the room is worth that
        // rather than the 50 c its B rung would have stood in for: a term that
        // was summed always beats the ladder.
        let value = value_of(
            &Valued::compute(&allflame(), &Knobs::default()),
            "factory",
            3,
        );

        assert!(close(value.total, 36.0), "total was {}", value.total);
        assert!(close(value.bonus, 36.0), "66 x 0.5 + 12 x 0.25");
        assert_ne!(value.total, Grade::B.fallback_chaos(), "not the B rung");
        assert!(
            !value
                .drivers
                .iter()
                .any(|d| d.kind == DriverKind::GradeFallback),
            "a room that summed something carries no grade driver"
        );
        // The feed priced the room-tier (at the floor) and the room names no
        // drop at all, so every term it has turned into chaos — by way of two
        // rates that are nobody's measurement.
        assert_eq!(value.priced, Priced::Market);
        assert!(value.guessed, "the c-per-% rates are Vertolka's guesses");
    }

    #[test]
    fn a_fallback_total_is_carried_by_the_grade_driver_alone() {
        // Zero both bonus rates and Chamber of Iron has nothing left: it sits
        // at the floor, drops nothing, and its two percentages now price at 0,
        // so its C rung stands in. The terms stay listed — the percentages are
        // real — but one still carrying `Some(0)` would make the lines the box
        // prints disagree with the 10 c the advisor ranked on.
        let knobs = Knobs {
            c_per_quantity: 0.0,
            c_per_rarity: 0.0,
            ..Knobs::default()
        };
        let value = value_of(&Valued::compute(&allflame(), &knobs), "chamber_of_iron", 3);

        assert_eq!(value.priced, Priced::Fallback);
        assert_eq!(value.total, 10.0);
        assert!(
            value
                .drivers
                .iter()
                .any(|d| d.kind == DriverKind::QuantityBonus),
            "the quantity term is still listed"
        );
        let carried: Vec<(DriverKind, Option<f64>)> = value
            .drivers
            .iter()
            .filter(|d| d.chaos.is_some())
            .map(|d| (d.kind, d.chaos))
            .collect();
        assert_eq!(carried, vec![(DriverKind::GradeFallback, Some(10.0))]);
    }

    #[test]
    fn with_no_market_a_feed_priced_room_falls_back_to_its_grade() {
        let valued = Valued::compute(&MarketInput::none(), &Knobs::default());

        assert_eq!(value_of(&valued, "corruption", 3).total, 800.0, "A++");
        assert_eq!(value_of(&valued, "gem", 3).total, 400.0, "A+");
        assert_eq!(value_of(&valued, "museum_of_artefacts", 3).total, 2.0, "D");

        let still_priced: Vec<&str> = valued
            .lines()
            .filter(|(_, tiers)| tiers.iter().any(|v| v.priced != Priced::Fallback))
            .map(|(key, _)| key)
            .collect();
        assert_eq!(
            still_priced,
            vec![
                "chamber_of_iron",
                "conduit_of_lightning",
                "crucible_of_flame",
                "defense_research_lab",
                "factory",
                "glittering_halls",
                "hall_of_champions",
                "hybridisation_chamber",
                "sanctum_of_immortality",
                "upgrade",
            ],
            "exactly the lines whose value never came from the feed: poedb's              area bonuses, and Crucible's manually priced temple-mod item"
        );
    }

    #[test]
    fn a_manually_priced_term_survives_a_cold_market() {
        // Crucible of Flame's temple-mod item is the one drop in the table
        // priced from drops.rs's own base price rather than from poe.ninja,
        // which prices no mod-rolled rare. Two gloves a run at 30 c stand
        // whether or not the feed has spoken.
        let value = value_of(
            &Valued::compute(&MarketInput::none(), &Knobs::default()),
            "crucible_of_flame",
            3,
        );

        assert_eq!(value.priced, Priced::Partial);
        assert_eq!(driver(&value, DriverKind::ModItem).chaos, Some(60.0));
        assert!(close(value.total, 66.0), "60 drops + 6 bonus, no sale");
    }

    #[test]
    fn a_stale_market_falls_back_even_though_it_carries_prices() {
        let mut market = allflame();
        market.stale = true;

        let value = value_of(
            &Valued::compute(&market, &Knobs::default()),
            "corruption",
            3,
        );

        assert_eq!(value.priced, Priced::Fallback);
        assert_eq!(value.total, 800.0, "the A++ rung, not the stale 846");
    }

    #[test]
    fn the_grade_ladder_keeps_the_two_above_floor_rooms_in_the_feeds_order() {
        // The ladder is only defensible if it does not reorder the two rooms
        // the market actually prices. Live: Locus 846 > Doryani 390.
        let cold = Valued::compute(&MarketInput::none(), &Knobs::default());

        assert!(
            value_of(&cold, "corruption", 3).total > value_of(&cold, "gem", 3).total,
            "A++ must outrank A+ with no market, as the feed does with one"
        );
    }

    #[test]
    fn a_thin_price_that_is_not_a_window_median_sets_one_flag_and_not_the_other() {
        // POE-131's depth flag and POE-252's window flag are independent, and
        // a driver that crossed them would tell the box the price was a
        // trailing median when it was the newest print of a thin market.
        let market: MarketInput = serde_json::from_str(
            r#"{"league":"Allflame","floor":10,"rooms":[],"recipes":[],
                "items":[{"name":"Vial of Summoning","price":{"chaos":7,"listings":2,
                          "lowConfidence":true,"windowPriced":false},"unpriced":false}]}"#,
        )
        .expect("the wire mirror accepts a partial payload");

        let quote = market
            .price("Vial of Summoning")
            .expect("the inline payload prices the vial");
        assert!(quote.low_confidence, "two listings");
        assert!(
            !quote.window_priced,
            "the newest print, not a window median"
        );

        let value = value_of(
            &Valued::compute(&market, &Knobs::default()),
            "sanctum_of_immortality",
            3,
        );
        let vial = driver(&value, DriverKind::VialDrop);
        assert_eq!(vial.unit_price, Some(7.0));
        assert!(vial.low_confidence, "the depth flag reaches the driver");
        assert!(!vial.window_priced, "and the window flag stays off");
    }

    #[test]
    fn a_vial_price_spike_raises_the_room_that_rolls_for_it() {
        // Mirage's Vial of Summoning spike, 60 c -> 808 c, on the Sanctum of
        // Immortality line that rolls for it.
        let knobs = Knobs::default();
        let before = value_of(
            &Valued::compute(&allflame(), &knobs),
            "sanctum_of_immortality",
            3,
        );
        let after = value_of(
            &Valued::compute(&allflame_with("Vial of Summoning", 808.0), &knobs),
            "sanctum_of_immortality",
            3,
        );

        assert!(
            close(after.total - before.total, 0.1 * (808.0 - 60.0)),
            "0.1 vials per run x the price move; was {} -> {}",
            before.total,
            after.total
        );
        let vial = driver(&after, DriverKind::VialDrop);
        assert_eq!(vial.name, "Vial of Summoning");
        assert_eq!(vial.unit_price, Some(808.0));
    }

    #[test]
    fn a_vial_price_spike_reorders_the_two_rooms_that_roll_for_vials() {
        // The flip the spike is worth showing: Sanctum of Immortality sits
        // below Crucible of Flame at 60 c and above it at 808 c.
        let knobs = Knobs::default();
        let live = Valued::compute(&allflame(), &knobs);
        let spiked = Valued::compute(&allflame_with("Vial of Summoning", 808.0), &knobs);

        assert!(
            value_of(&live, "sanctum_of_immortality", 3).total
                < value_of(&live, "crucible_of_flame", 3).total
        );
        assert!(
            value_of(&spiked, "sanctum_of_immortality", 3).total
                > value_of(&spiked, "crucible_of_flame", 3).total
        );
    }

    #[test]
    fn every_line_is_valued_at_every_tier_with_a_finite_positive_total() {
        let valued = Valued::compute(&allflame(), &Knobs::default());

        assert_eq!(valued.lines().count(), 25);
        for (key, tiers) in valued.lines() {
            for (index, value) in tiers.iter().enumerate() {
                assert!(
                    value.total.is_finite() && value.total > 0.0,
                    "{key} tier {} valued {}",
                    index + 1,
                    value.total
                );
            }
        }
    }

    #[test]
    fn no_total_carries_chaos_its_drivers_do_not_account_for() {
        // POE-260's box prints the drivers and the advisor ranks on the total,
        // so a term inside the total that no driver names — or a driver
        // carrying a number the total does not — shows the player a sum that
        // is not the one that was used. Three reads to cover the three paths:
        // the live capture, a cold market, and rates that zero every bonus,
        // which is what pushes a bonus-only room onto the fallback path with
        // its terms still listed.
        let cases = [
            ("live", allflame(), Knobs::default()),
            ("cold", MarketInput::none(), Knobs::default()),
            (
                "no bonus rates",
                allflame(),
                Knobs {
                    c_per_quantity: 0.0,
                    c_per_rarity: 0.0,
                    ..Knobs::default()
                },
            ),
        ];

        for (label, market, knobs) in cases {
            let valued = Valued::compute(&market, &knobs);
            for (key, tiers) in valued.lines() {
                let carried: f64 = tiers[2].drivers.iter().filter_map(|d| d.chaos).sum();
                assert!(
                    close(carried, tiers[2].total),
                    "{label} {key} tier 3 totals {} over drivers carrying {carried}",
                    tiers[2].total
                );
                // Tiers 1 and 2 carry their line's tier-3 drivers unscaled —
                // epic lock L2 values the LINE, and a tier-1 room drops no
                // chest unique of its own — so the tier-fraction line is where
                // their own total is accounted for.
                for (index, lower) in tiers[..2].iter().enumerate() {
                    assert_eq!(
                        driver(lower, DriverKind::TierFraction).chaos,
                        Some(lower.total),
                        "{label} {key} tier {}",
                        index + 1
                    );
                }
            }
        }
    }

    #[test]
    fn only_the_two_rooms_the_feed_prices_outright_are_free_of_estimates() {
        // Everything else rests on Vertolka's per-run counts, the two unmeasured
        // bonus rates, or the grade ladder — and says so.
        let valued = Valued::compute(&allflame(), &Knobs::default());

        let measured: Vec<&str> = valued
            .lines()
            .filter(|(_, tiers)| !tiers[2].guessed)
            .map(|(key, _)| key)
            .collect();

        assert_eq!(measured, vec!["corruption", "gem"]);
    }
}
