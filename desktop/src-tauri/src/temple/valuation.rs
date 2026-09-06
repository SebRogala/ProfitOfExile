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
//! # Fallback is never zero, and a cold read is not a half-priced one
//!
//! Epic lock L4: a missing or stale market falls back to the preset's base
//! value. There are two shapes of that, and which one applies turns on
//! [`MarketInput::prices_anything`] — live AND a usable floor AND at least one
//! room or item — which is the predicate `compute_with` and `rung` actually
//! branch on. Not [`MarketInput::is_live`] alone: that is one of its three
//! terms, and a payload with a zero floor is live and still prices nothing.
//!
//! **A read that prices nothing** — [`MarketInput::none`] before the first poll
//! answers, a stale payload, or one with an unusable floor — prices EVERY room
//! at [`Grade::fallback_chaos`](super::rooms::Grade::fallback_chaos), the cold
//! ladder, and nothing else. Those ten rungs ARE the preset's base-value table
//! that L4 names. Mixing them with the terms that survive a cold market — a
//! poedb quantity bonus, `drops.rs`'s manually priced temple-mod item — is
//! what this module used to do, and it produced a board where Sanctum of
//! Immortality (grade A) sat at 6 c UNDER Sadist's Den (grade C) at 10 c. Half
//! a formula and half a ladder is not a ranking in either unit. A Custom
//! override still wins over the rung: the player stating a number outright is
//! not a missing price.
//!
//! **A live read** sums the formula. The ladder stands in only for a room
//! where **nothing at all was summed** — no sale, no priced drop, no bonus —
//! and there it is
//! [`fallback_chaos_scaled`](super::rooms::Grade::fallback_chaos_scaled)
//! anchored on [`lowest_summed_room_total`]: the SMALLEST tier-3 total among
//! the rooms that summed something on this read. The set is the FORMULA sums
//! of the 23 non-instrumental lines, overrides ignored — a player's stated
//! number is not a sum, so it never enters the set and never moves it, while
//! the line's own formula sum always does. On the committed capture the
//! anchor is Toxic Grove's 2.65, so A++ reads 2.65, B− reads 0.0994 and D
//! reads 0.0066.
//!
//! That is Vertolka's rule (POE-262, 2026-09-06): *"if temple does not have
//! APEX, value of room itself is just zero and should be counted only based on
//! what you can drop from it"*. An unpriced room is worth less than every room
//! something priced, and the letter only orders the unpriced rooms among
//! themselves. It CLOSES the fork ADR-022 §3 left open — the shipped rule used
//! to anchor on the read's best sale delta and then cap at the lowest
//! SALE-PRICED room, which let Apex of Ascension (B−, nothing summed) stand at
//! 31.7 c over Chamber of Iron's measured 6 c. A guess no longer outranks a
//! measurement.
//!
//! Two properties the anchor buys, and both are why it is the LOWEST summed
//! total rather than anything else:
//!
//! - **A letter never beats a price.** Every rung is at most the anchor
//!   ([`Grade::APlusPlus`](super::rooms::Grade::APlusPlus) equals it, every
//!   lower grade is a fixed fraction of it) and the anchor is by construction
//!   at or below every summed room. A++ equalling the anchor is stated rather
//!   than hidden: the only A++ line is Locus of Corruption, which is a
//!   fallback room only on a read that prices neither it nor anything it
//!   drops.
//! - **A room's rung depends on its own letter and on nothing else that
//!   happened to fall back.** Clipping each rung at the anchor instead would
//!   tie B− and C together at it, and anchoring the board's top FALLBACK
//!   letter on it would move a C room's value the moment a B− room lost its
//!   price.
//!
//! The two [`INSTRUMENTAL_LINES`] do NOT take this anchor — see below; they
//! keep §4's own two-step, and [`lowest_sale_priced_room_total`] is the cap in
//! it.
//!
//! Where the ladder stands in, [`RoomValue::priced`] says [`Priced::Fallback`]
//! and the rung is the only driver carrying chaos. A room's total is never
//! `NaN` and never negative.
//!
//! # Two lines are priced at their letter even when the market is live
//!
//! [`INSTRUMENTAL_LINES`] — the upgrade line and the explosives line — take
//! the same rung by a different route, and say so with
//! [`Priced::Instrumental`] rather than [`Priced::Fallback`]: no price is
//! missing, summing is simply the wrong question for them. Their worth is the
//! tiers they lift and the rooms they clear, which the advisor's rollout
//! models; their own drops are a 6 c quantity bonus, which would price the
//! shrine that lifts a 846 c line below a junk C-grade room. See
//! [`instrumental`] for the board-7 evidence and the double-pay trade-off. A
//! Custom override replaces the rung like any other room's value.
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
//! chaos rests on somebody's estimate. Two knobs are estimates by construction
//! — [`Knobs::c_per_quantity`] and [`Knobs::c_per_rarity`] — so a room whose
//! value comes from a quantity bonus is flagged even though the underlying
//! percentage is poedb's measured number. That is the honest reading: the
//! *percentage* is measured, the *rate* that turns it into chaos is not.
//!
//! The grade ladder is flagged on ONE of its two paths, and the asymmetry is
//! deliberate. The LIVE rung is an inference about this market — the cold
//! number re-anchored on what this read actually priced — so it reads
//! `guessed: true`. The COLD rung is not an inference about anything: it is
//! the preset's stated base value for that room, the same standing as a Custom
//! override, which also reads `guessed: false`. [`Priced::Fallback`] is what
//! tells the player no price was involved either way.

use std::collections::BTreeMap;

use super::drops::Estimate;
use super::market::{ItemQuote, MarketInput};
use super::rooms::{Grade, RoomLine, LINES};
use super::strategy::{Tier, INSTRUMENTAL_LINES};

// ---------------------------------------------------------------- the knobs --

/// The per-user rates the formula is parameterised by (POE-257 D4).
///
/// Every one of these is a number a player may reasonably disagree with, which
/// is why it is a field rather than a constant — the module's recorded decision
/// is "one base strategy, per-user configurables" (`temple/mod.rs`). POE-259
/// persists them per preset; nothing here reads or writes settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Knobs {
    /// What a tier-1 or tier-2 room is worth as a fraction of its line's tier-3
    /// total. Epic lock L2's 80 %: line presence is what the double-tier node
    /// and the Timelines scarab act on. Clamped to `0.0..=1.0` — a tier-1 room
    /// worth more than its own tier-3 would invert the upgrade advice — and a
    /// non-finite or negative value falls back to this default rather than to
    /// zero (see [`tier_fraction`]).
    pub tier_fraction: f64,
    /// Chaos per point of `increased Quantity of Items found in this Area`.
    /// **Vertolka's proposed rate** (1 % quantity = 0.5 c, 1 % rarity = 0.25 c,
    /// 2026-09-06 message on POE-124: *"Maybe there we can setup something like
    /// 1% quant = 0,5c, 1% rarity = 0,25c"*), unmeasured — a guess he flagged
    /// as such.
    pub c_per_quantity: f64,
    /// Chaos per point of `increased Rarity of Items found in this Area`.
    /// **Vertolka's proposed rate**, same message and same standing as
    /// [`Self::c_per_quantity`]: unmeasured, a guess he flagged as such.
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
    /// One of the two [`INSTRUMENTAL_LINES`], priced at its letter rather than
    /// at what it drops.
    ///
    /// The upgrade line's worth is the TIERS IT LIFTS and the explosives
    /// line's is the ROOMS IT CLEARS, and both of those reach the score
    /// through the rollout (`advisor::rollout::UPGRADE_TARGETS` and
    /// `CHARGES`), not through a price. What the room itself drops — Temple
    /// Nexus's 6 c quantity bonus — is not what a player is buying when they
    /// take it, and summing it would put the shrine below a junk C-grade room.
    /// The letter is the honest stand-in: Vertolka graded the line for what it
    /// does, not for what falls out of it.
    Instrumental,
    /// The player's own number for this room-tier, from the Custom preset's
    /// table (POE-257 D4). It REPLACES the formula rather than adding to it,
    /// which is why it is the only driver such a room carries.
    CustomOverride,
}

impl DriverKind {
    /// The wire form — `snake_case` variants, the convention
    /// `desktop/src/lib/README.md` records for this app's enums.
    ///
    /// Hand-written rather than derived because [`DriverKind`] is a reasoning
    /// type the projection reads, exactly like
    /// [`Grade::as_str`](super::rooms::Grade::as_str).
    pub fn as_str(self) -> &'static str {
        match self {
            DriverKind::Sale => "sale",
            DriverKind::UniqueDrop => "unique_drop",
            DriverKind::VialDrop => "vial_drop",
            DriverKind::ModItem => "mod_item",
            DriverKind::QuantityBonus => "quantity_bonus",
            DriverKind::RarityBonus => "rarity_bonus",
            DriverKind::TierFraction => "tier_fraction",
            DriverKind::GradeFallback => "grade_fallback",
            DriverKind::Instrumental => "instrumental",
            DriverKind::CustomOverride => "custom_override",
        }
    }
}

/// One line of the explanation box: a term of the sum, and everything the
/// player needs to judge it.
///
/// A driver with `chaos: None` contributed nothing — either nobody has stated
/// the count, or the feed carries no price for the item. It is still listed,
/// because a term silently dropped from a total is indistinguishable from a
/// term worth zero.
#[derive(Debug, Clone, PartialEq)]
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
pub enum Priced {
    /// Every term this room names turned into chaos.
    Market,
    /// At least one named term did not, and at least one did.
    Partial,
    /// Nothing did. The total is the grade ladder's value (epic lock L4).
    Fallback,
    /// Nothing was summed because summing was the wrong question: this is one
    /// of the two [`INSTRUMENTAL_LINES`], priced at its letter. Distinct from
    /// [`Self::Fallback`] because no price is MISSING — see
    /// [`DriverKind::Instrumental`].
    Instrumental,
    /// Nothing was summed at all: the player stated this room-tier's value
    /// outright in the Custom table, and their number replaced the formula.
    Override,
}

impl Priced {
    /// The wire form — `snake_case`, like [`DriverKind::as_str`].
    pub fn as_str(self) -> &'static str {
        match self {
            Priced::Market => "market",
            Priced::Partial => "partial",
            Priced::Fallback => "fallback",
            Priced::Instrumental => "instrumental",
            Priced::Override => "override",
        }
    }
}

/// What one room-tier is worth, and what that number is made of.
#[derive(Debug, Clone, PartialEq)]
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

// -------------------------------------------------------------- the recipe --

/// One member of a vial recipe, priced (POE-260).
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeItem {
    /// poe.ninja's own name for the item — the join key the price came from,
    /// and the name the icon endpoint is asked for.
    pub name: String,
    /// Chaos, or `None` where this read priced nothing for it: the feed carries
    /// no usable line, the payload is cold, or the read is stale. Never `0.0`
    /// standing in for a missing price.
    pub chaos: Option<f64>,
}

/// A line's vial upgrade: base unique + vial -> upgraded unique, priced.
///
/// The recipe is a property of the LINE and the market read, not of a tier's
/// sum — no term of [`RoomValue`] comes from it. It is published because the
/// three prices together are the one thing that says whether the unique a room
/// drops is worth *keeping* or worth *upgrading*, which is a decision the box
/// otherwise leaves the player to make off the drop price alone.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeValue {
    /// The unique the line's tier-3 chest drops — [`LineDrops::unique`].
    pub base: RecipeItem,
    /// The vial that transforms it. **Not** necessarily the vial the line's
    /// own architect rolls for: on Locus of Corruption those are different
    /// items, which is why this is looked up by BASE and never by vial.
    pub vial: RecipeItem,
    /// What the two become.
    pub upgraded: RecipeItem,
}

/// The whole 25 x 3 table for one market read and one set of knobs.
///
/// One table per read is the point (POE-257 D6): the advisor ranks on it and
/// the overlay shows from it, so the number the player sees is the number the
/// recommendation used.
#[derive(Debug, Clone, PartialEq)]
pub struct Valued {
    lines: BTreeMap<&'static str, [RoomValue; 3]>,
    recipes: BTreeMap<&'static str, RecipeValue>,
    knobs: Knobs,
    league: String,
    as_of_ms: Option<i64>,
    top_sale_delta: Option<f64>,
}

/// One line's three per-tier overrides, tier 1 first. `None` means "use the
/// formula"; `Some` is the player's own chaos number for that room-tier.
pub type RoomOverrides = BTreeMap<String, [Option<f64>; 3]>;

impl Valued {
    /// Value every one of the 25 lines at all three tiers.
    pub fn compute(market: &MarketInput, knobs: &Knobs) -> Valued {
        Valued::compute_with(market, knobs, &RoomOverrides::new())
    }

    /// [`Self::compute`] with the Custom preset's per-room overrides applied
    /// (POE-257 D4).
    ///
    /// An override REPLACES the formula for exactly the room-tier it names: a
    /// tier-3 override also moves that line's tier-1 and tier-2 rows, because
    /// those are [`Knobs::tier_fraction`] of whatever tier 3 is worth (epic
    /// lock L2), while a tier-1 override moves nothing but itself.
    pub fn compute_with(market: &MarketInput, knobs: &Knobs, overrides: &RoomOverrides) -> Valued {
        // All three computed once for the whole table and BEFORE any room is
        // valued: what the ladder is rescaled on is a property of the READ,
        // not of a room, so a per-room re-derivation would be 25 answers to
        // one question. There are three rather than one because the two
        // rung-priced paths take DIFFERENT ladders — the fallback rooms hang
        // off `measured_floor` (POE-262) and the two instrumental lines keep
        // ADR-022 §4's own anchor-and-cap, which has its own board evidence.
        let top_sale_delta = top_tier3_sale_delta(market);
        let instrumental_cap = lowest_sale_priced_room_total(market, knobs);
        let measured_floor = lowest_summed_room_total(market, knobs);
        // The one branch the whole fallback rule turns on (epic lock L4). A
        // read that prices nothing — cold before the first poll, or stale —
        // still has terms that would survive it: a poedb quantity bonus and
        // `drops.rs`'s manually priced temple-mod item owe the feed nothing.
        // Summing those and letting the ladder stand in for the rest ranks half
        // the board in one unit and half in another, which is how a grade-A
        // room ended up under a grade-C one at 6 c against 10 c.
        let live = market.prices_anything();

        let mut lines = BTreeMap::new();
        for line in LINES.iter() {
            let row = overrides.get(line.key());
            let at = |tier: usize| row.and_then(|values| values[tier]);

            let t3 = match at(2) {
                Some(chaos) => overridden(line, Tier::T3, chaos),
                None if INSTRUMENTAL_LINES.contains(&line.mechanical_line()) => {
                    instrumental(line, live, top_sale_delta, instrumental_cap)
                }
                None if !live => cold_fallback(line),
                None => value_tier3(line, market, knobs, measured_floor),
            };
            let scaled = |tier: usize| match at(tier) {
                Some(chaos) => overridden(line, tier_of(tier), chaos),
                None => scale_from_tier3(line, &t3, knobs),
            };
            let t1 = scaled(0);
            let t2 = scaled(1);
            lines.insert(line.key(), [t1, t2, t3]);
        }

        Valued {
            lines,
            // Computed from the SAME read, so a box cannot show a recipe
            // priced off one snapshot beside a total priced off another.
            recipes: recipe_table(market),
            knobs: *knobs,
            league: market.league.clone(),
            // WITHHELD on a stale read, which is not the same claim
            // `MarketInput::as_of_ms` makes. That one answers "when did the
            // server last observe anything"; this one is the age of the prices
            // BEHIND these numbers, and a stale read contributed none of them
            // — every room on it fell back to the grade ladder. Publishing its
            // timestamp would date a valuation to a market it did not use.
            as_of_ms: market.is_live().then(|| market.as_of_ms()).flatten(),
            top_sale_delta,
        }
    }

    /// One room-tier's value. `None` for an unknown key and for
    /// [`Tier::T0`] — tier 0 is filler and belongs to no line.
    pub fn get(&self, key: &str, tier: Tier) -> Option<&RoomValue> {
        let tiers = self.lines.get(key)?;
        match tier.get() {
            1..=3 => Some(&tiers[tier.get() as usize - 1]),
            _ => None,
        }
    }

    /// The vial upgrade for one line's unique, or `None` (POE-260).
    ///
    /// `None` is the ordinary answer and covers three different facts, all of
    /// which the surface renders the same way — by printing no recipe line:
    /// the line drops no unique of its own (eighteen of the twenty-five), its
    /// unique is not the BASE of any recipe (Locus of Corruption drops
    /// Shadowstitch and rolls an amulet vial that upgrades somebody else's
    /// item), or the payload carried no recipe table at all, which is what a
    /// read that has never reached the server looks like.
    pub fn recipe(&self, key: &str) -> Option<&RecipeValue> {
        self.recipes.get(key)
    }

    /// Every line's three tiers, tier 1 first, in key order.
    pub fn lines(&self) -> impl Iterator<Item = (&'static str, &[RoomValue; 3])> + '_ {
        self.lines.iter().map(|(key, tiers)| (*key, tiers))
    }

    /// The rates this table was computed with.
    ///
    /// Carried rather than re-fetched because the table IS the market read and
    /// these knobs together: the profile bridge reads
    /// [`Knobs::combo_premium`] off it (POE-257 D5), and the explanation box
    /// needs the two bonus rates to say which of its lines are a guess.
    pub fn knobs(&self) -> &Knobs {
        &self.knobs
    }

    /// The league the market read priced against, empty on a cold read.
    /// Published beside every value (POE-260): a price is league-local, so a
    /// number shown without one is a number nobody can check.
    pub fn league(&self) -> &str {
        &self.league
    }

    /// When the market read behind this table was last observed, in epoch ms.
    /// `None` on a cold read — and on a stale one, which
    /// [`MarketInput::as_of_ms`] still answers but which this table was built
    /// from as if it carried nothing.
    pub fn as_of_ms(&self) -> Option<i64> {
        self.as_of_ms
    }

    /// The best tier-3 sale delta this read carries, or `None` on a cold or
    /// stale read (where the ladder is the absolute one). Published so the
    /// audit trail can say which ladder ran.
    ///
    /// It anchors the two [`INSTRUMENTAL_LINES`] and nothing else since
    /// POE-262. A room that summed nothing hangs off
    /// [`lowest_summed_room_total`] instead, which is not published: the
    /// number a player can check is the total on each row, and a second
    /// anchor on the wire would need a second explanation beside it.
    pub fn top_sale_delta(&self) -> Option<f64> {
        self.top_sale_delta
    }
}

/// A tier index `0..=2` as a [`Tier`]. Private, and total by construction —
/// the only caller iterates the three slots of a `[_; 3]`.
fn tier_of(index: usize) -> Tier {
    Tier::new(index as u8 + 1).expect("a [_; 3] index is 0..=2")
}

/// Every line's vial upgrade, priced off this read (POE-260).
///
/// Keyed by BASE and never by vial, because the two are not the same question:
/// [`LineDrops::vial`] is the vial the line's architect ROLLS FOR, and the
/// recipe is about the vial that TRANSFORMS the unique this line's chest drops.
/// They coincide on the six chest lines and diverge on Locus of Corruption,
/// which is the case that proves the rule (`drops.rs`'s module header).
///
/// A line whose unique the recipe table does not name gets no entry at all
/// rather than an entry with three empty prices — the recipe does not exist,
/// which is a different fact from a recipe nothing priced.
fn recipe_table(market: &MarketInput) -> BTreeMap<&'static str, RecipeValue> {
    let mut out = BTreeMap::new();
    for line in LINES.iter() {
        let Some(unique) = line.drops().unique() else {
            continue;
        };
        let Some(recipe) = market.recipes.iter().find(|r| r.base == unique) else {
            continue;
        };
        out.insert(
            line.key(),
            RecipeValue {
                base: recipe_item(market, &recipe.base),
                vial: recipe_item(market, &recipe.vial),
                upgraded: recipe_item(market, &recipe.upgraded),
            },
        );
    }
    out
}

/// One recipe member with whatever this read prices it at.
///
/// `MarketInput::price` is the single staleness gate (its module header), so a
/// stale read answers `None` here for every member without a second check.
fn recipe_item(market: &MarketInput, name: &str) -> RecipeItem {
    RecipeItem {
        name: name.to_string(),
        chaos: market.price(name).map(|quote| quote.chaos),
    }
}

/// The best tier-3 sale delta this read carries, or `None`.
///
/// `None` says "do not re-anchor the grade ladder", and one expression answers
/// it for every unusable market: a cold read carries no rooms, a live read
/// where everything sits at the floor carries no positive delta, and a STALE
/// read answers zero to every accessor by construction
/// ([`MarketInput`]'s module header: staleness is enforced in one place, and a
/// second check here would be a second answer to it).
fn top_tier3_sale_delta(market: &MarketInput) -> Option<f64> {
    if !market.prices_anything() {
        return None;
    }
    let top = LINES
        .iter()
        .filter_map(|line| line.name(Tier::T3))
        .map(|room| market.sale_delta(room, Tier::T3))
        .fold(0.0, f64::max);
    (top > 0.0).then_some(top)
}

/// The lowest tier-3 total among the rooms whose SALE is above the floor, or
/// `None` for a read with no such room.
///
/// 390 on the committed capture, where the two such rooms are Locus of
/// Corruption (846) and Doryani's Institute (390). This is ADR-022 §4's cap
/// and it is now read by [`instrumental`] alone: those two lines are priced at
/// their letter on the read's best sale delta, and without the cap a scaled B+
/// rung could outrank the cheapest price the feed actually printed, which is
/// the letter beating the market epic lock L1 forbids.
///
/// **Not the anchor a room that summed nothing takes** — that is
/// [`lowest_summed_room_total`], a strictly wider set and a much lower number
/// (2.65 against 390 on the capture). The two are deliberately separate
/// rather than folded into one: §4's departure from L1 rests on its own
/// board-7 evidence, and holding the upgrade line at 0.0033 of the cheapest
/// summed room would reproduce the miss that evidence is about.
///
/// `None` where there is no above-floor room (any read
/// [`MarketInput::prices_anything`] calls cold — no poll yet, stale, no usable
/// floor — or a board entirely at the floor), and there the scaled rung stands
/// uncapped, because there is no printed price for it to be beating.
///
/// An above-floor room always sums its own sale, so valuing one here can never
/// re-enter the fallback branch this result feeds — the recursion the missing
/// argument would otherwise imply cannot happen.
fn lowest_sale_priced_room_total(market: &MarketInput, knobs: &Knobs) -> Option<f64> {
    if !market.prices_anything() {
        return None;
    }
    LINES
        .iter()
        .filter(|line| {
            let Some(room) = line.name(Tier::T3) else {
                return false;
            };
            market
                .room(room, Tier::T3)
                .is_some_and(|quote| quote.above_floor)
                && market.sale_delta(room, Tier::T3) > 0.0
        })
        .map(|line| value_tier3(line, market, knobs, None).total)
        .fold(None, |lowest: Option<f64>, total| {
            Some(lowest.map_or(total, |low| low.min(total)))
        })
}

/// The lowest tier-3 total among the rooms that SUMMED something on this read
/// — the anchor a room that summed nothing hangs its letter off (POE-262).
///
/// Vertolka's rule: a room nothing priced is worth less than every room
/// something priced, and the letter only orders such rooms among themselves.
/// [`Grade::fallback_chaos_scaled`] maps A++ onto this number exactly and
/// every lower grade onto a fixed fraction of it, so both halves of the rule
/// hold at once — the whole ladder sits at or under the cheapest measured
/// room, and its ten rungs keep the order Vertolka graded them in.
///
/// **The set is what makes it that number**, and it is the FORMULA sums of the
/// 23 non-instrumental lines, overrides ignored:
///
/// - a room is IN when its own sum produced chaos, [`Priced::Market`] or
///   [`Priced::Partial`] alike — a partial sum is still a price somebody
///   printed, and Toxic Grove's partial 2.65 is exactly the number the capture
///   turns on;
/// - the two [`INSTRUMENTAL_LINES`] are OUT, because their totals are letters
///   rather than sums (ADR-022 §4) and anchoring the ladder on a rung of
///   itself would make the ladder define its own scale;
/// - a Custom override neither adds to the set nor removes from it. The
///   player's stated number is not a sum, so it never enters; the line's own
///   formula sum is a sum, so it always does. That keeps the anchor a property
///   of the READ alone — one Custom edit cannot move the other eleven unpriced
///   rooms, which excluding the overridden line would do (overriding the
///   capture's anchor room would raise every rung by 6.00/2.65, and overriding
///   the last summed room would drop the whole board onto the cold ladder,
///   from 0.03 c to 30 c in one edit).
///
/// `None` on a read that prices nothing, and on a live read where no room
/// summed anything at all — every room at the floor with every rate and weight
/// at zero. There is no third ladder for that case: [`fallback_rung`] answers
/// with the COLD rung, which is the same number the whole board would take one
/// branch earlier.
///
/// Valuing a candidate with no anchor is what keeps this non-recursive: a room
/// that summed something never reads a rung, and a room that did is filtered
/// out by its [`Priced::Fallback`] verdict before its (cold) total is looked
/// at.
fn lowest_summed_room_total(market: &MarketInput, knobs: &Knobs) -> Option<f64> {
    if !market.prices_anything() {
        return None;
    }
    LINES
        .iter()
        .filter(|line| !INSTRUMENTAL_LINES.contains(&line.mechanical_line()))
        .map(|line| value_tier3(line, market, knobs, None))
        .filter(|value| matches!(value.priced, Priced::Market | Priced::Partial))
        .fold(None, |lowest: Option<f64>, value| {
            Some(lowest.map_or(value.total, |low| low.min(value.total)))
        })
}

/// One room-tier on a read that priced nothing: its cold grade rung, alone.
///
/// Epic lock L4's "base value" — the ten rungs of
/// [`Grade::fallback_chaos`](super::rooms::Grade::fallback_chaos) are the
/// preset's stated table for a board with no market, and the whole board is
/// read off it so that every room is ranked in the same unit. No other driver
/// is listed: nothing was tried, because there was nothing to try it against.
/// `guessed` is false for the same reason a Custom override's is — this is a
/// stated number, not an inference about a market.
/// One grade's rung for a room that summed NOTHING on a live read (POE-262).
///
/// `measured_floor` is [`lowest_summed_room_total`], and one multiplication is
/// the whole rule: A++ lands on that floor and every lower grade on a fixed
/// fraction of it, so no letter reaches a room something priced and the
/// letters still order the unpriced rooms among themselves.
///
/// `None` is the read where nothing summed at all, and the answer there is the
/// COLD rung rather than a third ladder — with no summed room there is nothing
/// for a letter to be beating, and the ten rungs are the preset's stated table
/// for exactly that board. On a live read it also means no SALE-priced room
/// exists, since a sale is a sum. It is unreachable from the shipped knobs on
/// any read that prices anything: it takes every room at the floor with both
/// bonus rates and the drops weight zeroed.
fn fallback_rung(grade: Grade, measured_floor: Option<f64>) -> f64 {
    match measured_floor {
        Some(floor) => grade.fallback_chaos_scaled(floor),
        None => grade.fallback_chaos(),
    }
}

/// One grade's rung for an [`INSTRUMENTAL_LINES`] line — ADR-022 §4's own
/// two-step, and deliberately NOT [`fallback_rung`].
///
/// The cold rung on a read that prices nothing, and on a live read the rung
/// re-anchored on today's best sale delta and then held under the cheapest
/// sale-priced room ([`lowest_sale_priced_room_total`]). §4 departs from epic
/// lock L1 on board-7 evidence — the upgrade line is worth the tiers it lifts,
/// which is why it is not summed and not held under what it drops — and
/// POE-262 changed the fallback ladder without touching that departure or its
/// evidence.
fn instrumental_rung(grade: Grade, live: bool, ladder_top: Option<f64>, cap: Option<f64>) -> f64 {
    if !live {
        return grade.fallback_chaos();
    }
    let rung = match ladder_top {
        Some(top) => grade.fallback_chaos_scaled(top),
        None => grade.fallback_chaos(),
    };
    match cap {
        Some(cap) => rung.min(cap),
        None => rung,
    }
}

/// One of the two [`INSTRUMENTAL_LINES`], priced at its letter.
///
/// Temple Nexus is B+ and Shrine of Unmaking is D, and those two rungs are
/// what the lines are worth — 100 and 2 cold, 105.75 and 2.115 on the
/// committed capture. NOT what they drop. Temple Nexus is the room the claim
/// turns on: it carries a 6 c area bonus (6 % quantity, 12 % rarity at tier
/// 3), and summing that would price the room that lifts an 846 c line at 6 c,
/// below a junk C-grade room. Shrine of Unmaking drops NOTHING — the
/// explosive line is `None` at every tier of `drops.rs` — so summing gives it
/// zero and the ordinary fallback would hand it the same D rung; what this
/// path changes for the shrine is [`Priced::Instrumental`] rather than
/// [`Priced::Fallback`], which is the honest label for a room whose price is
/// not missing. What both lines are actually worth is the tiers they lift and
/// the rooms they clear, and that reaches the score through
/// `advisor::rollout`, which this does not touch.
///
/// The double-pay this deliberately accepts: the rollout ALREADY credits both
/// effects, so the rung is paid on top of them. The evidence that the
/// mechanical credit alone is too little is retrospective board 7, where the
/// app recommended *upgrade to Armoury* over Sebastian's *change to Sanctum of
/// Unity* — a 6 c junk room beating the shrine — under every priced profile
/// until the rung was added
/// (`advisor::tests::the_live_priced_profile_kills_this_on_every_walked_board`).
/// A Custom override replaces the rung like any other room's value, and it
/// reaches the ranking as well as the box.
fn instrumental(
    line: &RoomLine,
    live: bool,
    ladder_top: Option<f64>,
    cap: Option<f64>,
) -> RoomValue {
    let grade = line.grade();
    let chaos = instrumental_rung(grade, live, ladder_top, cap);
    RoomValue {
        sale: 0.0,
        drops: 0.0,
        bonus: 0.0,
        total: chaos,
        drivers: vec![Driver {
            kind: DriverKind::Instrumental,
            // The letter, exactly as `GradeFallback` names it: the rung IS the
            // grade, and the box's own wording for what that means belongs to
            // the surface (POE-260), not to a data field.
            name: grade.as_str().to_string(),
            count: None,
            unit_price: None,
            chaos: Some(chaos),
            // The same rule as the ladder: a live rung is an inference about
            // this market, a cold one is the preset's stated base value.
            guessed: live,
            low_confidence: false,
            window_priced: false,
        }],
        guessed: live,
        priced: Priced::Instrumental,
    }
}

fn cold_fallback(line: &RoomLine) -> RoomValue {
    let grade = line.grade();
    let chaos = grade.fallback_chaos();
    RoomValue {
        sale: 0.0,
        drops: 0.0,
        bonus: 0.0,
        total: chaos,
        drivers: vec![Driver {
            kind: DriverKind::GradeFallback,
            name: grade.as_str().to_string(),
            count: None,
            unit_price: None,
            chaos: Some(chaos),
            guessed: false,
            low_confidence: false,
            window_priced: false,
        }],
        guessed: false,
        priced: Priced::Fallback,
    }
}

/// One room-tier priced by the player rather than by the formula.
///
/// The single [`DriverKind::CustomOverride`] driver IS the whole accounting:
/// the drivers a consumer prints must sum to the total it ranks on, and a
/// stated number has no terms behind it to show.
fn overridden(line: &RoomLine, tier: Tier, chaos: f64) -> RoomValue {
    let total = rate(chaos);
    RoomValue {
        sale: 0.0,
        drops: 0.0,
        bonus: 0.0,
        total,
        drivers: vec![Driver {
            kind: DriverKind::CustomOverride,
            name: line
                .name(tier)
                .expect("an override names a tier-1..3 room")
                .to_string(),
            count: None,
            unit_price: None,
            chaos: Some(total),
            guessed: false,
            low_confidence: false,
            window_priced: false,
        }],
        guessed: false,
        priced: Priced::Override,
    }
}

// -------------------------------------------------------------- the formula --

/// One room-tier on a LIVE read.
///
/// `measured_floor` anchors the grade ladder for a room that summed nothing —
/// [`lowest_summed_room_total`], a property of the whole read, and `None`
/// where the read carries no summed room to derive it from. A read that is not
/// live never reaches here — [`cold_fallback`] answers for it.
///
/// The two set helpers call this with `None` for exactly that reason: a
/// candidate for the anchor either summed something (and never reads the
/// anchor) or is filtered out on its [`Priced::Fallback`] verdict.
fn value_tier3(
    line: &RoomLine,
    market: &MarketInput,
    knobs: &Knobs,
    measured_floor: Option<f64>,
) -> RoomValue {
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
        // `value_tier3` only runs on a live read, so the rung is the rescaled
        // one — the cold rungs are `cold_fallback`'s, one branch earlier.
        let ladder = fallback_rung(grade, measured_floor);
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
    use crate::temple::market::{allflame, STALE_AFTER_MS};
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

    /// The carried-down drivers are the TIER-3 row's, unscaled.
    ///
    /// This is the contract POE-260 has to render against, and it is
    /// counter-intuitive on purpose: a tier-1 row's `tier_fraction` driver
    /// carries the WHOLE total, and every other driver on it carries the
    /// tier-3 number it was copied from. Scaling the clones would look tidier
    /// and would be wrong twice over — the copies would then sum to the total
    /// alongside the fraction driver that already is it, and the box would
    /// print a Locus tier-1 room as 676.80 + 676.80.
    ///
    /// Fails the moment `scale_from_tier3` multiplies the copies.
    #[test]
    fn a_lower_tiers_carried_drivers_keep_the_tier_three_numbers() {
        let valued = Valued::compute(&allflame(), &Knobs::default());
        let t3 = value_of(&valued, "corruption", 3);
        let t1 = value_of(&valued, "corruption", 1);

        let carried: Vec<&Driver> = t1
            .drivers
            .iter()
            .filter(|d| d.kind != DriverKind::TierFraction)
            .collect();
        assert_eq!(
            carried.len(),
            t3.drivers.len(),
            "every tier-3 driver is carried down, and nothing else is",
        );
        for (copy, original) in carried.iter().zip(t3.drivers.iter()) {
            assert_eq!(copy.kind, original.kind);
            assert_eq!(copy.chaos, original.chaos, "{:?} was rescaled", copy.kind);
            assert_eq!(copy.unit_price, original.unit_price);
            assert_eq!(copy.count, original.count);
        }
        // And the one driver that IS the answer.
        assert_eq!(
            driver(&t1, DriverKind::TierFraction).chaos,
            Some(t1.total),
            "the fraction driver holds the total, so the clones must not",
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
        // The D rung on the LIVE ladder, which POE-262 re-anchored: the
        // capture's cheapest SUMMED room is Toxic Grove at 2.65, so every rung
        // is 2.65/800 of its cold value and D is 2 x 2.65/800 = 0.006625.
        // 2.0 here would mean the ladder had stopped moving with the market;
        // 2.115 would mean it was still anchored on the top sale delta.
        assert_eq!(value.total, Grade::D.fallback_chaos_scaled(2.65));
        assert!(close(value.total, 0.006_625), "total was {}", value.total);
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
        assert_ne!(
            value.total,
            Grade::B.fallback_chaos_scaled(846.0),
            "not the B rung — and the LIVE B rung, which is the one this read \
             would have fallen back to",
        );
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
        // prints disagree with the 0.033125 c the advisor ranked on.
        let knobs = Knobs {
            c_per_quantity: 0.0,
            c_per_rarity: 0.0,
            ..Knobs::default()
        };
        let value = value_of(&Valued::compute(&allflame(), &knobs), "chamber_of_iron", 3);

        assert_eq!(value.priced, Priced::Fallback);
        // The C rung on the LIVE ladder, stated as a number as well as as an
        // expression: an assertion written only in terms of
        // `fallback_chaos_scaled` moves with any change to that function and
        // could not fail. Zeroing the two rates does not move the anchor —
        // Toxic Grove's 2.65 is a DROP, not a bonus — so C is
        // 10 x 2.65/800 = 0.033125.
        assert_eq!(value.total, Grade::C.fallback_chaos_scaled(2.65));
        assert!(close(value.total, 0.033_125), "total was {}", value.total);
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
        assert_eq!(
            carried,
            vec![(
                DriverKind::GradeFallback,
                Some(Grade::C.fallback_chaos_scaled(2.65))
            )],
        );
    }

    /// A read that prices nothing prices the WHOLE board off the cold ladder.
    ///
    /// Epic lock L4's "base value" is those ten rungs, and it is all of them:
    /// the terms that survive a cold market — poedb's area bonuses, Crucible of
    /// Flame's manually priced temple-mod item — are deliberately NOT summed
    /// alongside a letter. Mixing them is what this module used to do, and it
    /// put Sanctum of Immortality (grade A, 6 c of quantity bonus) under
    /// Sadist's Den (grade C, 10 c of letter). Half a formula and half a ladder
    /// is not a ranking in either unit.
    ///
    /// Fails the moment the bonus term is summed on a cold read again: Sanctum
    /// reads 6 instead of 200, and Crucible reads 66 instead of 400.
    #[test]
    fn a_read_that_prices_nothing_reads_the_whole_board_off_the_cold_ladder() {
        let valued = Valued::compute(&MarketInput::none(), &Knobs::default());

        assert_eq!(valued.lines().count(), 25);
        for (key, tiers) in valued.lines() {
            let line = LINES
                .iter()
                .find(|line| line.key() == key)
                .expect("every valued key is a room line");
            assert_eq!(
                tiers[2].total,
                line.grade().fallback_chaos(),
                "{key} is graded {}",
                line.grade().as_str(),
            );
            // The two instrumental lines reach the SAME rung by a different
            // route and say so: they are not priced at their letter because
            // nothing was priced, but because their worth is the tiers they
            // lift and the rooms they clear.
            let instrumental = INSTRUMENTAL_LINES.contains(&line.mechanical_line());
            assert_eq!(
                tiers[2].priced,
                if instrumental {
                    Priced::Instrumental
                } else {
                    Priced::Fallback
                },
                "{key}",
            );
            assert!(
                !tiers[2].guessed,
                "{key}: a stated base value is not an inference about a market",
            );
            assert_eq!(
                tiers[2].drivers.len(),
                1,
                "{key}: nothing was tried, so only the rung is listed",
            );
            assert_eq!(
                tiers[2].drivers[0].kind,
                if instrumental {
                    DriverKind::Instrumental
                } else {
                    DriverKind::GradeFallback
                },
                "{key}",
            );
        }

        // The order the ladder is FOR, spelled out on the rooms the epic
        // argues about.
        assert_eq!(value_of(&valued, "corruption", 3).total, 800.0, "Locus A++");
        assert_eq!(value_of(&valued, "gem", 3).total, 400.0, "Doryani A+");
        assert_eq!(
            value_of(&valued, "crucible_of_flame", 3).total,
            400.0,
            "Crucible A+, and NOT the 66 its mod item plus bonus would sum to",
        );
        assert_eq!(
            value_of(&valued, "sanctum_of_immortality", 3).total,
            200.0,
            "Sanctum A, and NOT the 6 its quantity bonus would sum to",
        );
        assert_eq!(value_of(&valued, "sadists_den", 3).total, 10.0, "C");
        assert_eq!(
            value_of(&valued, "upgrade", 3).total,
            100.0,
            "Temple Nexus B+, priced at its letter and NOT at its 6 c bonus",
        );
        assert!(
            value_of(&valued, "sanctum_of_immortality", 3).total
                > value_of(&valued, "sadists_den", 3).total,
            "the inversion the rule exists to end",
        );
    }

    /// A manually priced drop is not a market, and does not stand in for one.
    ///
    /// Crucible of Flame's temple-mod item is the one drop in the table priced
    /// from `drops.rs`'s own base price rather than from poe.ninja, which
    /// prices no mod-rolled rare. Two gloves a run at 30 c is a real number and
    /// the box still shows it once a read arrives — but on a COLD read it is
    /// the only number on the board, and 66 c of it would rank Crucible below
    /// eleven rooms whose letters say it outranks them. Fails if the cold path
    /// starts summing terms again.
    #[test]
    fn a_manually_priced_term_does_not_stand_in_for_a_cold_market() {
        let value = value_of(
            &Valued::compute(&MarketInput::none(), &Knobs::default()),
            "crucible_of_flame",
            3,
        );

        assert_eq!(value.priced, Priced::Fallback);
        assert_eq!(value.total, 400.0, "the A+ rung, not the 66 it can sum");
        assert!(
            !value.drivers.iter().any(|d| d.kind == DriverKind::ModItem),
            "and nothing was tried, so nothing is listed: {:?}",
            value.drivers,
        );
        // The same term, on a live read, still contributes — this is about the
        // cold path only.
        assert_eq!(
            driver(
                &value_of(
                    &Valued::compute(&allflame(), &Knobs::default()),
                    "crucible_of_flame",
                    3
                ),
                DriverKind::ModItem
            )
            .chaos,
            Some(60.0),
        );
    }

    /// The room Vertolka flagged outranks the two letters that beat it.
    ///
    /// The POE-262 report, from his 2026-09-06 review of the value table:
    /// Hybridisation Chamber tier 3 measured 8.7 c on that day's read and sat
    /// UNDER Sadist's Den and Hall of War, which summed nothing at all and
    /// took a C rung of 10.575 between them. His rule ends it — *"if temple
    /// does not have APEX, value of room itself is just zero and should be
    /// counted only based on what you can drop from it"*.
    ///
    /// Fails on the shipped-before state: anchor the fallback rung on
    /// `top_tier3_sale_delta` instead of on `lowest_summed_room_total` and the
    /// two letters read 10.575 again, over a room the market priced.
    #[test]
    fn the_room_vertolka_flagged_outranks_the_two_letters_that_beat_it() {
        let valued = Valued::compute(&allflame(), &Knobs::default());

        let measured = value_of(&valued, "hybridisation_chamber", 3);
        let sadists = value_of(&valued, "sadists_den", 3);
        let war = value_of(&valued, "hall_of_war", 3);

        assert!(close(measured.total, 8.7), "was {}", measured.total);
        assert_eq!(measured.priced, Priced::Partial, "it summed a real drop");
        assert_eq!(sadists.priced, Priced::Fallback, "C, and it sums nothing");
        assert_eq!(
            war.priced,
            Priced::Fallback,
            "C, and pack size prices at nothing"
        );
        // 10 x 2.65/800: the C rung on the cheapest SUMMED room.
        assert!(close(sadists.total, 0.033_125), "was {}", sadists.total);
        assert!(close(war.total, 0.033_125), "was {}", war.total);
        assert!(measured.total > sadists.total);
        assert!(measured.total > war.total);
    }

    /// On a live read a letter never reaches a room that summed something.
    ///
    /// The whole point of POE-262's anchor, stated over all 25 lines rather
    /// than over the pair that was reported: `fallback_chaos_scaled` maps A++
    /// onto `lowest_summed_room_total` exactly and every lower grade onto a
    /// fraction of it, so the ladder as a whole sits at or under the cheapest
    /// measured room. The `<=` is A++'s alone and is stated rather than papered
    /// over with a margin constant — the only A++ line is Locus of Corruption,
    /// which reaches this path only on a read that prices neither it nor
    /// anything it drops.
    ///
    /// It is the property that closes ADR-022 §3's recorded fork, and Apex of
    /// Ascension is the fork's own example: B-, summing nothing, it used to
    /// stand at 31.725 over Chamber of Iron's measured 6.
    ///
    /// Fails if the anchor widens back to the sale-priced set (Apex reads
    /// 31.725 against a floor of 2.65) and fails if any rung stops being a
    /// fraction of the anchor.
    #[test]
    fn no_letter_reaches_a_room_that_summed_something() {
        let valued = Valued::compute(&allflame(), &Knobs::default());

        let cheapest_summed = valued
            .lines()
            .filter(|(_, tiers)| matches!(tiers[2].priced, Priced::Market | Priced::Partial))
            .map(|(_, tiers)| tiers[2].total)
            .fold(f64::INFINITY, f64::min);
        assert!(close(cheapest_summed, 2.65), "Toxic Grove, a partial sum");

        let mut fallbacks = 0;
        for (key, tiers) in valued.lines() {
            if tiers[2].priced != Priced::Fallback {
                continue;
            }
            fallbacks += 1;
            let line = LINES
                .iter()
                .find(|line| line.key() == key)
                .expect("every valued key is a room line");
            if line.grade() == Grade::APlusPlus {
                assert!(
                    tiers[2].total <= cheapest_summed,
                    "{key} is A++, which EQUALS the anchor, and read {}",
                    tiers[2].total,
                );
            } else {
                assert!(
                    tiers[2].total < cheapest_summed,
                    "{key} stands in at {} over a room the market priced at {cheapest_summed}",
                    tiers[2].total,
                );
            }
        }
        assert_eq!(fallbacks, 11, "the capture's unpriced rooms");
    }

    /// On the rusher's board the anchor collapses onto the two rooms the feed
    /// prices.
    ///
    /// `drops_weight = 0` is the rusher's setting (ADR-022 §5) and zeroing both
    /// bonus rates with it leaves a board where the ONLY summed rooms are Locus
    /// (846) and Doryani (390). The anchor is the cheaper of them, so Crucible
    /// of Flame — an A+ line sitting at the floor, which those knobs push onto
    /// the fallback path — reads 400 x 390/800 = 195 and stays under the 390
    /// the feed printed.
    ///
    /// The set is what moves here, not the rule: the same expression that gives
    /// 2.65 at the shipped knobs gives 390 at these. Fails if the anchor is
    /// hard-coded to the capture's 2.65, and fails if it goes back to the top
    /// sale delta (Crucible would read 423, over Doryani).
    #[test]
    fn the_rushers_knobs_leave_the_two_sale_priced_rooms_as_the_whole_anchor_set() {
        let knobs = Knobs {
            drops_weight: 0.0,
            c_per_quantity: 0.0,
            c_per_rarity: 0.0,
            ..Knobs::default()
        };
        let valued = Valued::compute(&allflame(), &knobs);

        let crucible = value_of(&valued, "crucible_of_flame", 3);
        assert_eq!(crucible.priced, Priced::Fallback, "precondition");
        assert!(close(crucible.total, 195.0), "A+, was {}", crucible.total);
        assert_eq!(
            crucible.total,
            Grade::APlus.fallback_chaos_scaled(390.0),
            "anchored on Doryani, the cheaper of the two summed rooms",
        );
        assert!(
            crucible.total < value_of(&valued, "gem", 3).total,
            "and under the price the feed actually printed",
        );
    }

    /// Among the rooms nothing priced, the letters still order them.
    ///
    /// The half of Vertolka's rule that is easy to lose: an unpriced room is
    /// worth less than every priced one, and the ten rungs are what says which
    /// unpriced room is worth more than which other. Every rung being a fixed
    /// fraction of one anchor is what keeps both halves at once.
    ///
    /// Fails if the rungs are CLIPPED at the anchor rather than scaled onto it
    /// — B- and C would tie at 2.65 on this capture — and fails if the ladder
    /// is flattened to a constant.
    #[test]
    fn the_letters_still_order_the_rooms_nothing_priced() {
        let valued = Valued::compute(&allflame(), &Knobs::default());

        let apex = value_of(&valued, "apex_of_ascension", 3);
        let sadists = value_of(&valued, "sadists_den", 3);
        let storm = value_of(&valued, "storm_of_corruption", 3);
        let atlas = value_of(&valued, "atlas_of_worlds", 3);

        for value in [&apex, &sadists, &storm, &atlas] {
            assert_eq!(value.priced, Priced::Fallback, "precondition");
        }
        // B- 30, C 10, C- 5, D 2, each x 2.65/800.
        assert!(close(apex.total, 0.099_375), "B-, was {}", apex.total);
        assert!(close(sadists.total, 0.033_125), "C, was {}", sadists.total);
        assert!(close(storm.total, 0.016_562_5), "C-, was {}", storm.total);
        assert!(close(atlas.total, 0.006_625), "D, was {}", atlas.total);
        assert!(apex.total > sadists.total);
        assert!(sadists.total > storm.total);
        assert!(storm.total > atlas.total);
    }

    /// An override on the anchor room moves no other room's value.
    ///
    /// The anchor is over the FORMULA sums of the 23 non-instrumental lines,
    /// and a Custom override changes neither membership nor value: the player's
    /// number is not a sum so it never enters the set, and the line's own
    /// formula sum is one so it never leaves. Toxic Grove is the room to prove
    /// it on because it IS the capture's anchor at 2.65, and the two override
    /// values bracket it from both sides — 0 below, 500 above.
    ///
    /// Excluding an overridden line instead is the tempting reading, and it is
    /// the one this pins against: it would make one Custom edit move eleven
    /// other rooms (the anchor would jump to Chamber of Iron's 6.00 and every
    /// rung would rise by 6.00/2.65), and overriding the LAST summed room would
    /// drop the whole board onto the cold ladder — a C room going from 0.033 c
    /// to 10 c because of an edit made somewhere else.
    ///
    /// Fails if the override filter is re-added to `lowest_summed_room_total`.
    #[test]
    fn an_override_on_the_anchor_room_moves_no_other_rooms_value() {
        let market = allflame();
        let untouched = Valued::compute(&market, &Knobs::default());
        assert!(
            close(value_of(&untouched, "toxic_grove", 3).total, 2.65),
            "precondition: Toxic Grove is the capture's anchor",
        );

        for stated in [0.0, 500.0] {
            let overrides =
                RoomOverrides::from([("toxic_grove".to_string(), [None, None, Some(stated)])]);

            let valued = Valued::compute_with(&market, &Knobs::default(), &overrides);

            assert_eq!(
                value_of(&valued, "toxic_grove", 3).total,
                stated,
                "the player's number is on the row",
            );
            for (key, tiers) in valued.lines() {
                if key == "toxic_grove" || tiers[2].priced != Priced::Fallback {
                    continue;
                }
                assert_eq!(
                    tiers[2].total,
                    untouched.get(key, Tier::T3).expect("valued").total,
                    "{key} moved because the anchor room was overridden to {stated}",
                );
            }
        }
    }

    /// A number the player states never becomes the anchor.
    ///
    /// The other half of the same rule, and the one a player would hit first:
    /// writing 0.5 c against the room that IS the anchor must not drag all
    /// eleven unpriced rooms down by a factor of five. Toxic Grove is the room
    /// because its stated 0.5 sits below its own formula sum of 2.65, so the
    /// two answers are distinguishable on the row and on every rung.
    ///
    /// Fails if the set is ever fed the PUBLISHED totals — the row as the
    /// player sees it — rather than the formula value of each line: Sadist's
    /// Den would read 10 x 0.5/800 = 0.00625 instead of 10 x 2.65/800.
    #[test]
    fn a_stated_number_never_becomes_the_anchor() {
        let market = allflame();
        let overrides = RoomOverrides::from([("toxic_grove".to_string(), [None, None, Some(0.5)])]);

        let valued = Valued::compute_with(&market, &Knobs::default(), &overrides);

        assert_eq!(
            value_of(&valued, "toxic_grove", 3).total,
            0.5,
            "the player's number is on the row",
        );
        let sadists = value_of(&valued, "sadists_den", 3);
        assert_eq!(sadists.priced, Priced::Fallback);
        // 10 x 2.65/800, the formula sum of the overridden line — not 10 x
        // 0.5/800, the number the player wrote over it.
        assert!(close(sadists.total, 0.033_125), "C, was {}", sadists.total);
    }

    /// The two instrumental lines keep ADR-022 §4's own ladder.
    ///
    /// POE-262 re-anchored the FALLBACK rooms and deliberately left §4 alone:
    /// Temple Nexus and Shrine of Unmaking are priced at their letter on the
    /// read's best sale delta, capped at the cheapest sale-priced room, and
    /// what they are worth is the tiers they lift and the rooms they clear —
    /// not a fraction of the cheapest room anybody prices. Board 7 is the
    /// evidence and it is `advisor::tests`'s, not this module's.
    ///
    /// Fails if the two lines are routed through `fallback_rung`: at the
    /// capture's 2.65 anchor Temple Nexus would read 0.33125 and the shrine
    /// 0.006625, which is the shape that produced the board-7 miss.
    #[test]
    fn the_instrumental_lines_are_not_re_anchored_on_the_cheapest_summed_room() {
        let valued = Valued::compute(&allflame(), &Knobs::default());

        let nexus = value_of(&valued, "upgrade", 3);
        let shrine = value_of(&valued, "explosive", 3);

        assert_eq!(nexus.priced, Priced::Instrumental);
        assert_eq!(shrine.priced, Priced::Instrumental);
        // 100 and 2 x 846/800, the top sale delta, capped at Doryani's 390.
        assert!(close(nexus.total, 105.75), "B+, was {}", nexus.total);
        assert!(close(shrine.total, 2.115), "D, was {}", shrine.total);
        // And they really do stand above the rooms the same read prices,
        // which is the departure from L1 §4 takes on its own evidence.
        assert!(nexus.total > value_of(&valued, "chamber_of_iron", 3).total);
    }

    /// The §4 cap really does hold an instrumental line under the cheapest
    /// price the feed printed.
    ///
    /// On the committed capture the cap never bites — the B+ rung is 105.75
    /// and the cheapest sale-priced room is Doryani's Institute at 390 — so
    /// the capture alone cannot tell `rung.min(cap)` apart from `rung`. This
    /// read is the capture with Doryani's tier-3 sale delta taken down to 60,
    /// which leaves it sale-priced (still above the 10 c floor, still
    /// `above_floor`) and makes it the cheapest such room. Its sale is its
    /// whole value, so its total is 60 exactly.
    ///
    /// Temple Nexus is then B+ at 100 x 846/800 = 105.75 before the cap and
    /// 60 after it — the price the feed actually printed, never over it, which
    /// is the one thing §4's departure from epic lock L1 still owes L1. The
    /// Shrine of Unmaking's D rung is 2 x 846/800 = 2.115, under the cap, and
    /// stays there.
    ///
    /// Fails if `instrumental_rung`'s `Some(cap) => rung.min(cap)` becomes
    /// `Some(_cap) => rung` (Temple Nexus reads 105.75, over the feed), and
    /// fails if it becomes `Some(cap) => cap` (the shrine reads 60, an
    /// explosives line priced like Doryani's Institute).
    #[test]
    fn an_instrumental_line_is_held_under_the_cheapest_sale_priced_room() {
        let mut market = allflame();
        let quote = market
            .rooms
            .get_mut(&("Doryani's Institute".to_string(), 3))
            .expect("the capture prices Doryani's Institute at tier 3");
        assert!(quote.above_floor, "precondition: it is a sale-priced room");
        quote.sale_delta = 60.0;

        let valued = Valued::compute(&market, &Knobs::default());

        assert!(
            close(value_of(&valued, "gem", 3).total, 60.0),
            "precondition: Doryani's is now the cheapest sale-priced room",
        );
        let nexus = value_of(&valued, "upgrade", 3);
        assert_eq!(nexus.priced, Priced::Instrumental);
        assert!(close(nexus.total, 60.0), "B+ capped, was {}", nexus.total);
        let shrine = value_of(&valued, "explosive", 3);
        assert!(
            close(shrine.total, 2.115),
            "D, under the cap, was {}",
            shrine.total,
        );
    }

    /// A room losing its price takes its own letter and moves nothing else.
    ///
    /// Take Doryani's Institute down to the floor and the gem line, whose sale
    /// was its whole value, falls to its A+ rung. What must NOT happen is the
    /// rest of the board moving with it: the anchor is the cheapest room that
    /// SUMMED something, and Doryani at 390 was never that room, so every
    /// other unpriced room reads exactly what it read before.
    ///
    /// The old rule failed this by construction — the cap was a function of
    /// which rooms the feed priced, so removing Doryani raised it from 390 to
    /// 846 and lifted the gem line to 423, above every measured room but
    /// Locus. Fails if the fallback rung goes back to reading a set that
    /// depends on which rooms fell back.
    #[test]
    fn a_room_losing_its_price_takes_its_letter_and_moves_no_other_room() {
        let untouched = Valued::compute(&allflame(), &Knobs::default());
        let mut market = allflame();
        let quote = market
            .rooms
            .get_mut(&("Doryani's Institute".to_string(), 3))
            .expect("the capture prices Doryani's Institute at tier 3");
        quote.above_floor = false;
        quote.sale_delta = 0.0;

        let valued = Valued::compute(&market, &Knobs::default());

        let gem = value_of(&valued, "gem", 3);
        assert_eq!(gem.priced, Priced::Fallback, "its sale was its whole value");
        // 400 x 2.65/800: the A+ rung on the unchanged anchor, not the 423 the
        // capped ladder used to hand it.
        assert!(
            close(gem.total, Grade::APlus.fallback_chaos_scaled(2.65)),
            "was {}",
            gem.total,
        );
        assert!(close(gem.total, 1.325), "was {}", gem.total);
        for (key, tiers) in valued.lines() {
            if key == "gem" || tiers[2].priced != Priced::Fallback {
                continue;
            }
            assert_eq!(
                tiers[2].total,
                untouched.get(key, Tier::T3).expect("valued").total,
                "{key} moved because ANOTHER room lost its price",
            );
        }
    }

    /// A live read where nothing summed at all answers the COLD rungs.
    ///
    /// The one shape `lowest_summed_room_total` has no answer for, and there is
    /// no third ladder for it: with no summed room there is nothing for a
    /// letter to be beating, so the ten absolute rungs — the preset's stated
    /// table — stand. It takes a live read with a usable floor, no room quote
    /// at all and every rate and weight zeroed, which is what this builds.
    ///
    /// It is still a LIVE read and says so: `guessed` is true, because a rung
    /// standing in on a market that answered is an inference about that market,
    /// where a cold rung is a stated base value.
    ///
    /// Fails if the `None` arm answers zero — every room in the game would be
    /// worth nothing the moment a market went quiet — or if it answers a rung
    /// scaled on something else.
    #[test]
    fn a_live_read_that_summed_nothing_anywhere_answers_the_cold_rungs() {
        let market: MarketInput = serde_json::from_str(
            r#"{"league":"Allflame","floor":10,"rooms":[],"recipes":[],
                "items":[{"name":"Vial of Summoning","price":{"chaos":7,"listings":40,
                          "lowConfidence":false,"windowPriced":false},"unpriced":false}]}"#,
        )
        .expect("the wire mirror accepts a partial payload");
        assert!(market.prices_anything(), "precondition: this read is LIVE");
        let knobs = Knobs {
            drops_weight: 0.0,
            c_per_quantity: 0.0,
            c_per_rarity: 0.0,
            ..Knobs::default()
        };

        let valued = Valued::compute(&market, &knobs);

        assert_eq!(valued.lines().count(), 25);
        for (key, tiers) in valued.lines() {
            let line = LINES
                .iter()
                .find(|line| line.key() == key)
                .expect("every valued key is a room line");
            assert_eq!(
                tiers[2].total,
                line.grade().fallback_chaos(),
                "{key} is graded {}",
                line.grade().as_str(),
            );
            assert!(
                tiers[2].guessed,
                "{key}: a rung on a live market is an inference about it",
            );
        }
    }

    /// A read with a broken floor is a cold read, on the WHOLE board.
    ///
    /// The floor is the median of all 86 room-tier lines, so zero is not a
    /// market anybody can trade into — it is an empty or broken feed. The
    /// dangerous shape is not the floor being ignored outright but the board
    /// being ranked in two units at once: with the floor guarded only inside
    /// the ladder anchor and the cap, a payload like this would still SUM its
    /// own `saleDelta`s while every unpriced room took the cold, UNCAPPED rung
    /// — Temple Nexus standing at 100 over rooms the feed prices at 3.
    ///
    /// Fails if the floor term is dropped from `MarketInput::prices_anything`:
    /// Locus reads its wire delta instead of its 800 rung, and Temple Nexus
    /// stops being capped.
    #[test]
    fn a_read_with_no_usable_floor_reads_the_same_cold_ladder_as_no_market() {
        for floor in [0.0, -1.0, f64::NAN] {
            let mut market = allflame();
            market.floor = floor;
            assert!(market.is_live(), "precondition: nothing else is wrong");
            assert!(!market.rooms.is_empty(), "precondition: it carries rooms");

            let broken = Valued::compute(&market, &Knobs::default());
            let cold = Valued::compute(&MarketInput::none(), &Knobs::default());

            for (key, tiers) in broken.lines() {
                let same = cold.get(key, Tier::T3).expect("valued");
                assert_eq!(tiers[2].total, same.total, "floor {floor}: {key}");
                assert_eq!(tiers[2].priced, same.priced, "floor {floor}: {key}");
            }
            assert_eq!(broken.top_sale_delta(), None, "floor {floor}: no anchor");
        }
    }

    /// A stale read is a cold read: the same whole-board cold ladder, not the
    /// stale prices and not the scaled rungs.
    ///
    /// Epic lock L4 names stale and missing in one breath, and every accessor
    /// on `MarketInput` already answers as absent while `stale` is set. Fails
    /// if a stale read reaches the live path — Locus would read its stale 846,
    /// or the rungs would be re-anchored on it.
    #[test]
    fn a_stale_market_reads_the_same_cold_ladder_as_no_market_at_all() {
        let mut market = allflame();
        market.stale = true;

        let stale = Valued::compute(&market, &Knobs::default());
        let cold = Valued::compute(&MarketInput::none(), &Knobs::default());

        assert_eq!(value_of(&stale, "corruption", 3).total, 800.0, "A++");
        for (key, tiers) in stale.lines() {
            let same = cold.get(key, Tier::T3).expect("valued");
            assert_eq!(tiers[2].total, same.total, "{key}");
            assert_eq!(tiers[2].priced, same.priced, "{key}");
        }
    }

    #[test]
    fn a_stale_read_publishes_no_price_age_and_no_ladder_anchor() {
        // A stale read contributed no price to any total — every room on it
        // fell back to the grade ladder. Publishing its `asOf` anyway would
        // date the valuation to a market it did not use, and the overlay's
        // "priced N minutes ago" line would be about numbers that came from a
        // letter grade.
        let mut market = allflame();
        market.stale = true;
        assert_eq!(
            market.as_of_ms(),
            Some(1_788_665_199_649),
            "precondition: the read itself still knows when it was taken",
        );

        let valued = Valued::compute(&market, &Knobs::default());

        assert_eq!(valued.as_of_ms(), None);
        assert_eq!(valued.top_sale_delta(), None, "and nothing to anchor on");
        assert_eq!(valued.league(), "Allflame", "the league is still a fact");
    }

    #[test]
    fn a_live_read_publishes_its_age_and_the_delta_the_ladder_was_anchored_on() {
        let valued = Valued::compute(&allflame(), &Knobs::default());

        assert_eq!(valued.as_of_ms(), Some(1_788_665_199_649));
        assert_eq!(valued.top_sale_delta(), Some(846.0), "Locus of Corruption");
    }

    #[test]
    fn a_cold_read_has_no_anchor_and_no_age() {
        let valued = Valued::compute(&MarketInput::none(), &Knobs::default());

        assert_eq!(valued.as_of_ms(), None);
        assert_eq!(valued.top_sale_delta(), None);
        assert_eq!(valued.league(), "");
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

    // ------------------------------------------------------- the recipe --

    #[test]
    fn a_lines_recipe_names_its_unique_the_vial_that_transforms_it_and_the_result() {
        // Crucible of Flame on the 2026-09-06 capture: the chest drops Story of
        // the Vaal, Vial of Fate turns it into Fate of the Vaal, and all three
        // are priced. The prices are the capture's own, so a projection that
        // read the wrong member — or dropped the price and kept the name —
        // fails on the number rather than on the shape.
        let valued = Valued::compute(&allflame(), &Knobs::default());

        let recipe = valued
            .recipe("crucible_of_flame")
            .expect("Story of the Vaal is a recipe base");

        assert_eq!(recipe.base.name, "Story of the Vaal");
        assert_eq!(recipe.base.chaos, Some(5.0));
        assert_eq!(recipe.vial.name, "Vial of Fate");
        assert_eq!(recipe.vial.chaos, Some(1.0));
        assert_eq!(recipe.upgraded.name, "Fate of the Vaal");
        assert_eq!(recipe.upgraded.chaos, Some(39.2));
    }

    #[test]
    fn a_line_whose_own_unique_no_vial_upgrades_has_no_recipe() {
        // Locus of Corruption is the case that proves the lookup is by BASE.
        // It drops Shadowstitch, which no recipe transforms, while the vial its
        // architect rolls for — Vial of Sacrifice — IS a recipe vial, of
        // Sacrificial Heart. A lookup keyed on the vial would hand this line
        // somebody else's upgrade and print two items it never drops.
        let valued = Valued::compute(&allflame(), &Knobs::default());

        assert_eq!(valued.recipe("corruption"), None);
    }

    #[test]
    fn a_line_that_drops_no_unique_has_no_recipe() {
        // Eighteen of the twenty-five, and Chamber of Iron is one: no chest
        // unique, so there is nothing for a vial to transform.
        let valued = Valued::compute(&allflame(), &Knobs::default());

        assert_eq!(valued.recipe("chamber_of_iron"), None);
    }

    #[test]
    fn a_stale_read_keeps_the_recipes_names_and_prices_none_of_its_members() {
        // Epic lock L4 on the recipe line: a stale snapshot prices nothing, and
        // the box draws an em dash rather than yesterday's number. The RECIPE
        // itself survives, because which vial upgrades which unique is a fact
        // of the game and not of the market.
        let stale = allflame().aged_at(1_788_665_199_649 + STALE_AFTER_MS + 1);
        let valued = Valued::compute(&stale, &Knobs::default());

        let recipe = valued
            .recipe("crucible_of_flame")
            .expect("the recipe table survives a stale read");

        assert_eq!(recipe.base.name, "Story of the Vaal");
        assert_eq!(
            (recipe.base.chaos, recipe.vial.chaos, recipe.upgraded.chaos),
            (None, None, None),
            "a stale read prices no recipe member"
        );
    }

    #[test]
    fn a_read_that_has_never_reached_the_server_has_no_recipe_table_at_all() {
        // `MarketInput::none()` carries no recipes, which is a different fact
        // from a recipe nothing priced: the app does not know the table yet, so
        // it states nothing rather than printing three em dashes.
        let valued = Valued::compute(&MarketInput::none(), &Knobs::default());

        assert_eq!(valued.recipe("crucible_of_flame"), None);
    }
}
