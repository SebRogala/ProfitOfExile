//! What each of the 25 room lines is expected to DROP, per tier (POE-256).
//!
//! [`rooms`](super::rooms) owns the closed name vocabulary and Vertolka's letter
//! grade. This module owns the numbers behind that letter: the tier-3 chest
//! unique, the architect's temple-mod items, and the quantity / rarity / pack-size percentages the room adds.
//!
//! Epic lock L1 (POE-124) is what this table feeds: room value = sale price +
//! drops (expected count x LIVE price) + bonus (quant/rarity x chaos-per-%).
//! **No price lives here.** Prices are always fetched; a name in this table is a
//! join key, not a value. The only exception is
//! [`TempleMod::base_price_chaos`], which exists precisely because poe.ninja
//! publishes no price for a mod-rolled rare, and it is a guess (see below).
//!
//! # Every number carries where it came from
//!
//! [`Estimate`] pairs a value with a [`Basis`] that is either
//! [`Basis::Measured`] or [`Basis::Guess`], and the basis names the source and
//! the date it was read. Nothing in this file is a bare `f64`, and **"none" is
//! `Option::None`, never `0.0`** — a room that drops no unique and a room whose
//! unique rate nobody has measured must not read the same to POE-257.
//! [`LineDrops::has_guess`] answers "is anything in this room's value guessed?"
//! so the overlay (POE-260) can mark it.
//!
//! Three sources for the NUMBERS, and only three:
//!
//! - [`POEDB`] — the room's own page on poedb.tw, read 2026-09-06. This is the
//!   game's data: the quantity / rarity / pack-size lines, the item-chance stat
//!   and the unique listed on the room's `Unique` tab. Measured.
//! - [`VERTOLKA_SHEET`] — Vertolka's room sheet, read 2026-09-06. Used for the
//!   architect mod-group names and the item hints; every number it states is
//!   also on poedb, and poedb is cited for those.
//! - [`VERTOLKA_MSG`] — Vertolka's 2026-09-06 message quoted on POE-124. Every
//!   number from it is a [`Basis::Guess`], because he said so: "price of unique
//!   divided by 4" (hence 0.25 per run at tier 3) and "Crucible of Flames is giving on average 2 temple gloves per
//!   run and their base price is usually around 30c".
//!
//! [`TempleMod::worth`] is the one field here that is a VERDICT rather than a
//! number, and it has its own source — Vertolka's 2026-09-10 Discord message.
//! See [`Worth`].
//!
//! # Uniques drop at tier 3 only
//!
//! Vertolka: "only tier 3 rooms can drop unique, but T1 and T2 ... provide
//! smaller quant/rarity bonuses". So
//! [`TierDrops::uniques_per_run`] is `None` at tiers 1 and 2 for every line, and
//! his 0.25 sits at tier 3 for the six lines whose sheet row names a unique.
//!
//! # No vials (POE-366)
//!
//! 3.30 removed vials from the temple, so no line carries a vial or a vial
//! chance and no room value has a vial term.
//!
//! [`TierDrops::uniques_per_run`] does NOT scale: no stat on any page states a
//! per-tier chest-unique chance, so his 0.25 stays a flat tier-3 number on the
//! six lines whose sheet row names a unique.
//!
//! The same pages print a per-tier item stat on four of those lines —
//! `map incursion boss chance to drop <tag> item % [33/66/100]`, on Conduit of
//! Lightning, Crucible of Flame, Hybridisation Chamber and Sanctum of
//! Immortality — and [`TierDrops::mod_item_chance_raw`] is that integer,
//! verbatim: raw, unscaled, never multiplied by a price. Nobody has stated an anchor for it — Vertolka
//! priced the gloves per RUN, not per point of this stat — so there is no
//! ratio to hang it off. It is the game's own per-tier handle on how often the
//! architect's signature rare ([`TempleMod`]) drops, which otherwise carries a
//! number only where Vertolka guessed one. Defense Research Lab and Toxic Grove
//! name a temple mod but their pages print no such stat, so both are `None`.
//!
//! # Where the two sources disagree
//!
//! Two places where Vertolka's sheet claims a bonus and poedb prints no
//! percentage: Toxic Grove ("increase quantity/rarity") and Storm of Corruption
//! ("high rarity buff"). Neither page states a number, so both are `None` rather
//! than a figure nobody wrote down.
//!
//! # What this table deliberately does not model
//!
//! The schema is numeric — counts per run, percentages, and the raw poedb
//! item-chance stat. Eight further statements on those pages and on the sheet are
//! real drops or real bonuses with no number and no field here. They are
//! listed rather than passed over in silence, because a reader of this table
//! would otherwise conclude the rooms state nothing.
//!
//! poedb states, and this table does not carry:
//!
//! - `apex_of_ascension`, tiers 1-3 — "Ahuana drops an additional Unique Item".
//!   An extra unique per run, on the one line whose own value is the sacrifice.
//! - `atlas_of_worlds` tier 3 — "Temple Architects drop 1 additional Scarabs".
//! - `sadists_den`, all tiers — "Area is haunted by 5 additional Tormented
//!   Spirits".
//! - `upgrade`, tiers 2 and 3 — "Monsters drop items 1 Levels higher".
//!
//! [`VERTOLKA_SHEET`]'s `global temple modifier` column states four more with
//! no percentage in them at all: Temple Nexus "increases item level of items",
//! Sadist Den "from T2 is Omnitect possesed", Storm of Corruption "adding
//! corrupting/radiating tempest - high rarity buff", Throne of Atziri "increase
//! magic monster packs".
//!
//! All eight are **accepted as outside POE-256's numeric scope**. Each would
//! need either a unit no field here has (an item level, a monster rarity, a
//! scarab) or a count nobody has stated (what a Tormented Spirit is worth), and
//! giving them a shape here would hand POE-257 a fabricated number. The prose
//! half of the sheet that IS carried is [`LineDrops::note`].

// POE-257 (the valuation formula) and POE-260 (the overlay) are this table's
// first callers; until one lands, most of this module is reached only by the
// tests. That is an inventory, not a blanket: each such item carries its own
// `#[allow(dead_code)]` and loses it with its first production caller, the way
// `rooms` records the same thing. A file-level allow would hide the next
// accessor somebody adds and forgets to wire up.

use super::rooms::RoomLine;
use super::strategy::Tier;

// ----------------------------------------------------------------- sources --

/// The room's own page on poedb.tw (`https://poedb.tw/us/<Room_Name>`), read
/// 2026-09-06 for all 75 tiered rooms.
pub const POEDB: &str = "poedb.tw room page, 2026-09-06";
/// Vertolka's room sheet, read 2026-09-06 — the architect mod-group names and
/// the item hints.
pub const VERTOLKA_SHEET: &str = "Vertolka's sheet, 2026-09-06";
/// Vertolka's 2026-09-06 message, quoted on epic POE-124. Everything sourced
/// here is a [`Basis::Guess`]; he offered the numbers as his own estimates.
pub const VERTOLKA_MSG: &str = "Vertolka, 2026-09-06 message (POE-124)";
/// Factory is the one room whose page prints the quantity stat twice: Jiquani's
/// own `20/40/60% increased Quantity of Items found in this Area` and the
/// shared room line's `2/4/6%`. Same stat, same scope, and `increased`
/// modifiers of one stat add, so [`TierDrops::quantity_pct`] holds their sum
/// (22 / 44 / 66) and this source names both halves.
pub const FACTORY_QUANTITY: &str =
    "poedb.tw Factory room page, 2026-09-06: the architect's 20/40/60% plus the room's 2/4/6% \
     increased Quantity of Items found in this Area, summed";

// ------------------------------------------------------------------- basis --

/// Where a number came from, and therefore how much weight it carries.
///
/// The distinction is the point of this module: POE-257 computes a chaos value
/// from these, and POE-260 shows the player which parts of it are somebody's
/// estimate rather than the game's own data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// Read off a page that states the number — the game's data.
    Measured {
        /// Who stated it and when.
        source: &'static str,
    },
    /// Somebody's estimate. Never treat as fact.
    Guess {
        /// Who guessed it and when.
        source: &'static str,
    },
}

impl Basis {
    /// Who stated or guessed the number, and the date it was read.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn source(self) -> &'static str {
        match self {
            Basis::Measured { source } | Basis::Guess { source } => source,
        }
    }

    /// Whether this number is somebody's estimate rather than the game's data.
    pub fn is_guess(self) -> bool {
        matches!(self, Basis::Guess { .. })
    }
}

/// A number that knows where it came from. There is no other kind here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    value: f64,
    basis: Basis,
}

impl Estimate {
    /// A number a source states outright.
    pub const fn measured(value: f64, source: &'static str) -> Estimate {
        Estimate {
            value,
            basis: Basis::Measured { source },
        }
    }

    /// A number somebody estimated.
    pub const fn guess(value: f64, source: &'static str) -> Estimate {
        Estimate {
            value,
            basis: Basis::Guess { source },
        }
    }

    /// The number itself. Units are the field's, not this type's.
    pub fn value(self) -> f64 {
        self.value
    }

    /// How the number was arrived at.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn basis(self) -> Basis {
        self.basis
    }

    /// Shorthand for [`Basis::is_guess`].
    pub fn is_guess(self) -> bool {
        self.basis.is_guess()
    }

    /// Shorthand for [`Basis::source`].
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn source(self) -> &'static str {
        self.basis.source()
    }
}

#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
fn any_guess(estimates: &[Option<Estimate>]) -> bool {
    estimates
        .iter()
        .any(|e| matches!(e, Some(estimate) if estimate.is_guess()))
}

// -------------------------------------------------------------- temple mod --

/// An item class an architect's mod family can roll on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Helmet,
    BodyArmour,
    Gloves,
    Boots,
    Amulet,
    Ring,
    Belt,
    Shield,
    Weapon,
}

impl Slot {
    /// The stable wire spelling used by the temple offer view.
    pub fn as_str(self) -> &'static str {
        match self {
            Slot::Helmet => "helmet",
            Slot::BodyArmour => "body_armour",
            Slot::Gloves => "gloves",
            Slot::Boots => "boots",
            Slot::Amulet => "amulet",
            Slot::Ring => "ring",
            Slot::Belt => "belt",
            Slot::Shield => "shield",
            Slot::Weapon => "weapon",
        }
    }
}

/// Whether a mod family is one to chase, one to ignore, or neither.
///
/// A VERDICT on the family itself and not on this read's prices — which is why
/// it lives beside the name here rather than being derived from
/// [`TempleMod::base_price_chaos`]. Only one mod in the table has a price at
/// all, so a price-derived rule would grade one row and shrug at six.
///
/// Source: Vertolka's verdict on the mod family itself, 2026-09-10 Discord:
/// *"make Puhuarte green and Matatl red"*. Everything he did not name is
/// [`Worth::Neutral`] — the absence of a verdict, not a middling one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Worth {
    /// Worth chasing.
    Good,
    /// Junk.
    Junk,
    /// Nobody has graded this family.
    Neutral,
}

impl Worth {
    /// The stable wire spelling used by the temple offer view.
    pub fn as_str(self) -> &'static str {
        match self {
            Worth::Good => "good",
            Worth::Junk => "junk",
            Worth::Neutral => "neutral",
        }
    }
}

/// The architect's signature rare — items that roll a mod group only this room
/// can produce.
///
/// poe.ninja prices no such item: it is a rare, and its worth is the mod, not
/// the base. That is why [`TempleMod::base_price_chaos`] exists at all, and why
/// it is the one price in this file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempleMod {
    architect: &'static str,
    item_hint: &'static str,
    /// Item classes this mod family can roll on, from the source named on the row.
    slots: &'static [Slot],
    /// Vertolka's verdict on the mod family itself (2026-09-10, see [`Worth`]).
    worth: Worth,
    source: &'static str,
    per_run: Option<Estimate>,
    base_price_chaos: Option<Estimate>,
}

impl TempleMod {
    /// The architect whose name the mod group is known by — "Puhuarte",
    /// "Guatelitzi". Spelled as the game spells the boss on the room's poedb
    /// page, which is also how [`VERTOLKA_SHEET`] writes it.
    pub fn architect(self) -> &'static str {
        self.architect
    }

    /// What the mod lands on, in Vertolka's words — "temple gloves", "jewellery
    /// with mana modifiers". Prose for the overlay, never a join key.
    pub fn item_hint(self) -> &'static str {
        self.item_hint
    }

    /// What item classes this mod family can roll on, from the wiki tables.
    pub fn slots(self) -> &'static [Slot] {
        self.slots
    }

    /// Vertolka's verdict on the mod family itself — what colours the offer
    /// box's mod name (POE-277). See [`Worth`] for the source and the rule.
    pub fn worth(self) -> Worth {
        self.worth
    }

    /// Expected items per finished-temple run, where anyone has said. Only
    /// Crucible of Flame has a number, and it is a guess.
    pub fn per_run(self) -> Option<Estimate> {
        self.per_run
    }

    /// What one such item is worth in chaos when poe.ninja cannot price it.
    pub fn base_price_chaos(self) -> Option<Estimate> {
        self.base_price_chaos
    }

    /// Who named the mod group and the item hint. Always [`VERTOLKA_SHEET`]:
    /// the sheet's `temple mod` column is the only place either is written
    /// down. The architect's spelling is normalised to the boss name on the
    /// room's poedb page ("Guatelitzi", not the sheet's "guatelitzi").
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn source(self) -> &'static str {
        self.source
    }

    /// Whether either of this mod's two numbers is a guess.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn has_guess(self) -> bool {
        any_guess(&[self.per_run, self.base_price_chaos])
    }
}

// --------------------------------------------------------------- per tier --

/// What one tier of one line is expected to drop.
///
/// Every field is `Option`: `None` means nobody has stated a number, which is a
/// different claim from zero and must stay distinguishable from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TierDrops {
    uniques_per_run: Option<Estimate>,
    mod_item_chance_raw: Option<u32>,
    quantity_pct: Option<Estimate>,
    rarity_pct: Option<Estimate>,
    pack_size_pct: Option<Estimate>,
}

impl TierDrops {
    /// A tier that drops nothing anybody has quantified — the explicit "none"
    /// for the lines with no chest and no area bonus.
    pub const NONE: TierDrops = TierDrops {
        uniques_per_run: None,
        mod_item_chance_raw: None,
        quantity_pct: None,
        rarity_pct: None,
        pack_size_pct: None,
    };

    /// Expected tier-3 chest uniques per run. Always `None` at tiers 1 and 2 —
    /// the game drops the chest unique at tier 3 only.
    pub fn uniques_per_run(self) -> Option<Estimate> {
        self.uniques_per_run
    }

    /// poedb's `map incursion boss chance to drop <tag> item % [N]` integer,
    /// verbatim, on the four lines whose page prints it (33 / 66 / 100 across the tiers). It is
    /// the game's own handle on how often the architect's signature rare
    /// ([`TempleMod`]) actually drops, and the same caveat applies: **raw, the
    /// page prints no scale, not an [`Estimate`]** — no anchor has been stated
    /// for it, so it stays raw and unmultiplied.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn mod_item_chance_raw(self) -> Option<u32> {
        self.mod_item_chance_raw
    }

    /// `increased Quantity of Items found in this Area`, in percent.
    pub fn quantity_pct(self) -> Option<Estimate> {
        self.quantity_pct
    }

    /// `increased Rarity of Items found in this Area`, in percent.
    pub fn rarity_pct(self) -> Option<Estimate> {
        self.rarity_pct
    }

    /// `increased Pack size`, in percent.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn pack_size_pct(self) -> Option<Estimate> {
        self.pack_size_pct
    }

    /// Whether nobody has stated anything at all about this tier.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn is_empty(self) -> bool {
        self == TierDrops::NONE
    }

    /// Whether any number on this tier is somebody's estimate.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn has_guess(self) -> bool {
        any_guess(&[
            self.uniques_per_run,
            self.quantity_pct,
            self.rarity_pct,
            self.pack_size_pct,
        ])
    }
}

// --------------------------------------------------------------- per line --

/// One room line's drops, tier 1 first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineDrops {
    key: &'static str,
    unique: Option<&'static str>,
    /// WI-6 draft wording (orchestrator, 2026-09-10) for the tier-3 room's use;
    /// UNCONFIRMED — the owner / Vertolka are to confirm each string. Prose for
    /// the box, never a join key.
    content: Option<&'static str>,
    temple_mod: Option<TempleMod>,
    note: Option<&'static str>,
    tiers: [TierDrops; 3],
}

/// Whether a line draws as a chest or as content prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    /// A line whose room has a tier-3 chest unique.
    Chest,
    /// A line whose room is described by content prose instead.
    Content,
}

impl Default for LineKind {
    fn default() -> Self {
        Self::Chest
    }
}

impl LineKind {
    /// The wire spelling used by the slice and its TypeScript mirror.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chest => "chest",
            Self::Content => "content",
        }
    }
}

impl LineDrops {
    /// The [`RoomLine::key`] this row belongs to.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn key(self) -> &'static str {
        self.key
    }

    /// The unique the tier-3 chest drops, as **poe.ninja spells it** — the join
    /// key POE-255/257 price against. `None` for the nineteen lines with no
    /// unique of their own.
    pub fn unique(self) -> Option<&'static str> {
        self.unique
    }

    /// The draft prose for a content line, or `None` for a chest line.
    pub fn content(self) -> Option<&'static str> {
        self.content
    }

    /// How this line is drawn. A unique-bearing row is a chest; the other rows
    /// carry content prose instead. The kind is derived so those facts cannot
    /// disagree.
    pub fn kind(self) -> LineKind {
        if self.unique.is_some() {
            LineKind::Chest
        } else {
            LineKind::Content
        }
    }

    /// The architect's signature rare, where the room produces one.
    pub fn temple_mod(self) -> Option<TempleMod> {
        self.temple_mod
    }

    /// Vertolka's own `Commentary` cell for this line, **verbatim** —
    /// [`VERTOLKA_SHEET`] is the source and its typos are his ("you should't
    /// need it", the stray closing bracket on Wealth of the Vaal). Nine of the
    /// twenty-five rows carry one; the rest are `None`.
    ///
    /// It is prose for the overlay, never a join key and never a number. In
    /// particular it is deliberately **not** part of [`LineDrops::is_empty`] or
    /// [`LineDrops::has_guess`]: "nothing worth much" is a remark about a room
    /// that drops nothing, not a drop, and Museum of Artefacts must keep
    /// reporting empty while carrying it.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn note(self) -> Option<&'static str> {
        self.note
    }

    /// This line's drops at `tier`, or `None` for [`Tier::T0`] — tier 0 is
    /// filler and belongs to no line.
    pub fn tier(&self, tier: Tier) -> Option<&TierDrops> {
        match tier.get() {
            1..=3 => Some(&self.tiers[tier.get() as usize - 1]),
            _ => None,
        }
    }

    /// All three tiers, tier 1 first.
    pub fn tiers(&self) -> &[TierDrops; 3] {
        &self.tiers
    }

    /// Whether this line drops nothing anybody has quantified or named.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn is_empty(&self) -> bool {
        self.unique.is_none()
            && self.temple_mod.is_none()
            && self.tiers.iter().all(|t| t.is_empty())
    }

    /// **Is anything in this room's value guessed?** POE-257 reads it to weight
    /// the number down; POE-260 reads it to mark the overlay.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn has_guess(&self) -> bool {
        self.temple_mod.is_some_and(TempleMod::has_guess)
            || self.tiers.iter().any(|t| t.has_guess())
    }
}

/// The drops row for a line key, or `None` if the key is not one of the 25.
pub fn for_key(key: &str) -> Option<&'static LineDrops> {
    DROPS.iter().find(|d| d.key == key)
}

impl RoomLine {
    /// This line's drops.
    ///
    /// Total by construction: [`DROPS`] is keyed 1:1 on [`LINES`], which
    /// `every_line_has_a_drops_row_in_the_same_order` pins.
    pub fn drops(self) -> &'static LineDrops {
        for_key(self.key()).expect("DROPS covers every key in LINES")
    }
}

/// The 25 lines' drops, in [`LINES`] order.
pub const DROPS: [LineDrops; 25] = [
    LineDrops {
        key: "apex_of_ascension",
        unique: None,
        content: Some("Sacrifice a unique, get a random one"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
    LineDrops {
        key: "atlas_of_worlds",
        unique: None,
        content: Some("Map drops"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
    LineDrops {
        key: "chamber_of_iron",
        unique: None,
        content: Some("Armour drops"),
        temple_mod: None,
        note: Some("sometimes can drop temple mod item + fracture/veiled items"),
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(2.0, POEDB)),
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(4.0, POEDB)),
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(6.0, POEDB)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
            },
        ],
    },
    LineDrops {
        key: "conduit_of_lightning",
        unique: Some("Dance of the Offered"),
        content: None,
        temple_mod: Some(TempleMod {
            architect: "Xopec",
            item_hint: "jewellery with mana modifiers",
            // Source: poewiki modifier tables ("Xopec's" and "of Xopec"),
            // union of the Classes column, owner-pasted 2026-09-10; every
            // weapon class folds into Weapon.
            slots: &[Slot::Helmet, Slot::Gloves, Slot::Boots, Slot::Amulet, Slot::Ring],
            worth: Worth::Neutral,
            source: VERTOLKA_SHEET,
            per_run: None,
            base_price_chaos: None,
        }),
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: Some(33),
                quantity_pct: Some(Estimate::measured(2.0, POEDB)),
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: Some(66),
                quantity_pct: Some(Estimate::measured(4.0, POEDB)),
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                mod_item_chance_raw: Some(100),
                quantity_pct: Some(Estimate::measured(6.0, POEDB)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
            },
        ],
    },
    LineDrops {
        key: "court_of_sealed_death",
        unique: None,
        content: Some("Arcanist & Diviner strongboxes"),
        temple_mod: None,
        note: Some("In T3 Arcanist and Diviner Stronboxes are quite common"),
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
    LineDrops {
        key: "crucible_of_flame",
        unique: Some("Story of the Vaal"),
        content: None,
        temple_mod: Some(TempleMod {
            architect: "Puhuarte",
            item_hint: "temple gloves",
            // Source: poewiki modifier tables ("Puhuarte's" and "of Puhuarte"),
            // union of the Classes column, owner-pasted 2026-09-10; every
            // weapon class folds into Weapon.
            slots: &[Slot::Helmet, Slot::Gloves, Slot::Amulet],
            worth: Worth::Good,
            source: VERTOLKA_SHEET,
            per_run: Some(Estimate::guess(2.0, VERTOLKA_MSG)),
            base_price_chaos: Some(Estimate::guess(30.0, VERTOLKA_MSG)),
        }),
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: Some(33),
                quantity_pct: Some(Estimate::measured(2.0, POEDB)),
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: Some(66),
                quantity_pct: Some(Estimate::measured(4.0, POEDB)),
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                mod_item_chance_raw: Some(100),
                quantity_pct: Some(Estimate::measured(6.0, POEDB)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
            },
        ],
    },
    LineDrops {
        key: "defense_research_lab",
        unique: Some("Architect's Hand"),
        content: None,
        temple_mod: Some(TempleMod {
            architect: "Matatl",
            // The sheet's tier-3 column names two things, not one: "mine/trap
            // items + 30% movespeed boots". Dropping the boots would lose a
            // whole item class from the hint (VERTOLKA_SHEET).
            item_hint: "items with trap and mine modifiers, + 30% movespeed boots",
            // Source: poewiki modifier tables ("Matatl's" and "of Matatl"),
            // union of the Classes column, owner-pasted 2026-09-10; every
            // weapon class folds into Weapon.
            slots: &[Slot::Boots, Slot::Weapon],
            worth: Worth::Junk,
            source: VERTOLKA_SHEET,
            per_run: None,
            base_price_chaos: None,
        }),
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(2.0, POEDB)),
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(4.0, POEDB)),
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(6.0, POEDB)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
            },
        ],
    },
    LineDrops {
        key: "gem",
        unique: None,
        content: Some("Double-corrupt a gem"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
    LineDrops {
        key: "factory",
        unique: None,
        content: Some("Extra item quantity"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(22.0, FACTORY_QUANTITY)),
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(44.0, FACTORY_QUANTITY)),
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(66.0, FACTORY_QUANTITY)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
            },
        ],
    },
    LineDrops {
        key: "glittering_halls",
        unique: None,
        content: Some("Jewellery drops"),
        temple_mod: None,
        note: Some("sometimes can drop fractured item"),
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: Some(Estimate::measured(20.0, POEDB)),
                pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: Some(Estimate::measured(40.0, POEDB)),
                pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: Some(Estimate::measured(60.0, POEDB)),
                pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "hall_of_champions",
        unique: None,
        content: Some("Weapon drops"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(2.0, POEDB)),
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(4.0, POEDB)),
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(6.0, POEDB)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
            },
        ],
    },
    LineDrops {
        key: "hall_of_legends",
        unique: None,
        content: Some("Legion encounter"),
        temple_mod: None,
        note: Some("there are much better ways how to farm legion"),
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
    LineDrops {
        key: "hall_of_war",
        unique: None,
        content: Some("Rare monster packs"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: Some(Estimate::measured(10.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: Some(Estimate::measured(20.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: Some(Estimate::measured(30.0, POEDB)),
            },
        ],
    },
    LineDrops {
        key: "house_of_the_others",
        unique: None,
        content: Some("Breach encounter"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
    LineDrops {
        key: "hybridisation_chamber",
        // Coward's CHAINS is the room's drop, not Coward's Legacy: the vial's
        // own in-game text reads "Sacrifice this item on the Altar of Sacrifice
        // along with Coward's Chains to transform it", and the Recipe table on
        // the same page names Chains as the input (poedb.tw Vial of Consequence
        // page, 2026-09-06). Legacy is the upgrade, and belongs to POE-255.
        unique: Some("Coward's Chains"),
        content: None,
        temple_mod: Some(TempleMod {
            architect: "Citaqualotl",
            item_hint: "items with minion modifiers",
            // Source: poewiki modifier tables ("Citaqualotl's" and "of
            // Citaqualotl"), union of the Classes column, owner-pasted
            // 2026-09-10; every weapon class folds into Weapon.
            slots: &[Slot::Weapon],
            worth: Worth::Neutral,
            source: VERTOLKA_SHEET,
            per_run: None,
            base_price_chaos: None,
        }),
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: Some(33),
                quantity_pct: Some(Estimate::measured(2.0, POEDB)),
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: Some(66),
                quantity_pct: Some(Estimate::measured(4.0, POEDB)),
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                mod_item_chance_raw: Some(100),
                quantity_pct: Some(Estimate::measured(6.0, POEDB)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
            },
        ],
    },
    LineDrops {
        key: "corruption",
        // Locus of Corruption's altar MAKES Shadowstitch from the Sacrificial
        // Garb the player brings; it is not a drop. The earlier reading of the
        // poedb.tw `Unique /1` tab (2026-09-06) was the source of that mistake.
        // Vertolka's sheet correctly has no chest unique here: this room has no
        // chest.
        unique: None,
        content: Some("Double-corrupt an item"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "museum_of_artefacts",
        unique: None,
        content: Some("Extra item drops"),
        temple_mod: None,
        note: Some("nothing worth much"),
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
    LineDrops {
        key: "sadists_den",
        unique: None,
        content: Some("Extra monsters (dangerous)"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
    LineDrops {
        key: "sanctum_of_immortality",
        unique: Some("Mask of the Spirit Drinker"),
        content: None,
        temple_mod: Some(TempleMod {
            architect: "Guatelitzi",
            item_hint: "items with life and energy-shield modifiers",
            // Source: poewiki modifier tables ("Guatelitzi's" and "of
            // Guatelitzi"), union of the Classes column, owner-pasted
            // 2026-09-10; every weapon class folds into Weapon.
            slots: &[Slot::BodyArmour, Slot::Amulet, Slot::Ring, Slot::Belt],
            worth: Worth::Neutral,
            source: VERTOLKA_SHEET,
            per_run: None,
            base_price_chaos: None,
        }),
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: Some(33),
                quantity_pct: Some(Estimate::measured(2.0, POEDB)),
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: Some(66),
                quantity_pct: Some(Estimate::measured(4.0, POEDB)),
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                mod_item_chance_raw: Some(100),
                quantity_pct: Some(Estimate::measured(6.0, POEDB)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
            },
        ],
    },
    LineDrops {
        key: "explosive",
        unique: None,
        content: Some("Destroy rooms / explosives"),
        temple_mod: None,
        note: Some("usually you should't need it"),
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
    LineDrops {
        key: "storm_of_corruption",
        unique: None,
        content: Some("Corrupting tempest"),
        temple_mod: Some(TempleMod {
            architect: "Topotante",
            item_hint: "weapons with elemental offence modifiers, gloves with physical-to-elemental conversion",
            // Source: poewiki modifier tables ("Topotante's" and "of
            // Topotante"), union of the Classes column, owner-pasted
            // 2026-09-10; every weapon class folds into Weapon.
            slots: &[Slot::Gloves, Slot::Shield, Slot::Weapon],
            worth: Worth::Neutral,
            source: VERTOLKA_SHEET,
            per_run: None,
            base_price_chaos: None,
        }),
        note: Some("corrupting tempest can be good in early league for corrupted 6-links"),
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
    LineDrops {
        key: "upgrade",
        unique: None,
        content: Some("Upgrade a room"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(2.0, POEDB)),
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(4.0, POEDB)),
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: Some(Estimate::measured(6.0, POEDB)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
            },
        ],
    },
    LineDrops {
        key: "throne_of_atziri",
        unique: None,
        content: Some("Queen Atziri"),
        temple_mod: None,
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "toxic_grove",
        unique: Some("Apep's Slumber"),
        content: None,
        temple_mod: Some(TempleMod {
            architect: "Tacati",
            // Vertolka's own trailing caveat, kept verbatim: one of the mods he
            // lists is no longer in the drop pool and the wikis still print it,
            // so the hint has to carry the correction with the claim
            // (VERTOLKA_SHEET).
            item_hint: "weapons with attack- and cast-speed, physical offence or spell-trigger modifiers, body armour with chaos resistance, note: caster mod dont drop - legacy - wiki dont update this change",
            // Source: poewiki modifier tables ("Tacati's" and "of Tacati"),
            // union of the Classes column, owner-pasted 2026-09-10; every
            // weapon class folds into Weapon.
            slots: &[Slot::BodyArmour, Slot::Shield, Slot::Weapon],
            worth: Worth::Neutral,
            source: VERTOLKA_SHEET,
            per_run: None,
            base_price_chaos: None,
        }),
        note: Some("can be good with corrupting tempest for 6-link farming"),
        tiers: [
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                mod_item_chance_raw: None,
                quantity_pct: None,
                rarity_pct: None,
                pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "wealth_of_the_vaal",
        unique: None,
        content: Some("Currency drops"),
        temple_mod: None,
        note: Some("couple of currency usually max. 10c value - maybe worth in Early League)"),
        tiers: [
            TierDrops::NONE,
            TierDrops::NONE,
            TierDrops::NONE,
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::rooms::LINES;

    /// poe.ninja's own `lines[].name` strings, snapshotted 2026-09-06. The
    /// table joins to live prices by these, so this file is the typo net.
    const NINJA_NAMES: &str = include_str!("assets/poe-ninja-names-2026-09-06.txt");

    fn fixture(section: &str) -> Vec<&'static str> {
        let mut names = Vec::new();
        let mut inside = false;
        for line in NINJA_NAMES.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                inside = line == section;
            } else if inside && !line.is_empty() && !line.starts_with('#') {
                names.push(line);
            }
        }
        names
    }

    fn drops_for(key: &str) -> &'static LineDrops {
        for_key(key).unwrap_or_else(|| panic!("no drops row keyed {key}"))
    }

    fn tier(n: u8) -> Tier {
        Tier::new(n).expect("test tier is 0..=3")
    }

    /// The six lines whose sheet row names a tier-3 chest unique, and so the
    /// six that carry his 0.25 unique rate.
    const CHEST_LINES: [&str; 6] = [
        "conduit_of_lightning",
        "crucible_of_flame",
        "defense_research_lab",
        "hybridisation_chamber",
        "sanctum_of_immortality",
        "toxic_grove",
    ];

    /// Vertolka's nine non-blank `Commentary` cells, transcribed from the CSV
    /// with his typos left in place ("you should't", the unmatched bracket on
    /// Wealth of the Vaal), in [`DROPS`] order. Reworded commentary is not his
    /// commentary, so this is a character-for-character copy.
    const SHEET_COMMENTARY: [(&str, &str); 9] = [
        (
            "chamber_of_iron",
            "sometimes can drop temple mod item + fracture/veiled items",
        ),
        (
            "court_of_sealed_death",
            "In T3 Arcanist and Diviner Stronboxes are quite common",
        ),
        ("glittering_halls", "sometimes can drop fractured item"),
        (
            "hall_of_legends",
            "there are much better ways how to farm legion",
        ),
        ("museum_of_artefacts", "nothing worth much"),
        ("explosive", "usually you should't need it"),
        (
            "storm_of_corruption",
            "corrupting tempest can be good in early league for corrupted 6-links",
        ),
        (
            "toxic_grove",
            "can be good with corrupting tempest for 6-link farming",
        ),
        (
            "wealth_of_the_vaal",
            "couple of currency usually max. 10c value - maybe worth in Early League)",
        ),
    ];

    /// poedb's raw mod-item chance stat, read a **second** time out of the
    /// POE-256 evidence set (`poedb-pct.json`, the `map incursion boss chance
    /// to drop <tag> item % [N]` lines of the 75 fetched pages) rather than
    /// copied out of [`DROPS`]. Two independent transcriptions of the same
    /// pages: a slipped digit in either one shows up as a mismatch, which is
    /// the only way a plausible-looking wrong integer gets caught.
    ///
    /// Key, then the mod-item stat at tiers 1/2/3.
    const POEDB_RAW_CHANCES: [(&str, [u32; 3]); 4] = [
        ("conduit_of_lightning", [33, 66, 100]),
        ("crucible_of_flame", [33, 66, 100]),
        ("hybridisation_chamber", [33, 66, 100]),
        ("sanctum_of_immortality", [33, 66, 100]),
    ];

    // ------------------------------------------------------------ shape --

    #[test]
    fn every_slot_has_a_unique_snake_case_wire_name() {
        // Fails if a variant is added without a row here
        let slots = [
            (Slot::Helmet, "helmet"),
            (Slot::BodyArmour, "body_armour"),
            (Slot::Gloves, "gloves"),
            (Slot::Boots, "boots"),
            (Slot::Amulet, "amulet"),
            (Slot::Ring, "ring"),
            (Slot::Belt, "belt"),
            (Slot::Shield, "shield"),
            (Slot::Weapon, "weapon"),
        ];

        for (slot, expected) in slots {
            let actual = slot.as_str();
            assert_eq!(actual, expected, "{slot:?} wire name");
        }
    }

    #[test]
    fn every_worth_has_its_own_wire_spelling() {
        // These three strings are the whole contract with the overlay:
        // `LineView.mod_worth` carries one of them and `view.ts`'s `modWorth`
        // guard recognises `good` and `junk` by spelling, dropping anything
        // else to `neutral`. So a retyped or swapped arm here does not fail
        // anywhere downstream — it reaches the box as a mod nobody graded, or
        // as the opposite verdict, in a colour.
        let worths = [(Worth::Good, "good"), (Worth::Junk, "junk"), (Worth::Neutral, "neutral")];

        for (worth, expected) in worths {
            assert_eq!(worth.as_str(), expected, "{worth:?} wire name");
        }
    }

    // Fails if a slot is swapped, if a verdict moves to another family, or if a
    // temple-mod row is added or removed.
    //
    // The `worth` column joined this table rather than getting a test of its
    // own because it is the same fact as the other two — the row — and it is
    // Vertolka's 2026-09-10 message in full: *"make Puhuarte green and Matatl
    // red"*, and the five families he did not name are `Neutral`. The five
    // matter as much as the two: `Neutral` is the absence of a verdict, so a
    // family that quietly acquired one would colour a mod name on the overlay
    // off nobody's word.
    #[test]
    fn temple_mod_rows_carry_their_architect_slot_table_and_worth() {
        const EXPECTED: [(&str, &str, &[Slot], Worth); 7] = [
            (
                "conduit_of_lightning",
                "Xopec",
                &[Slot::Helmet, Slot::Gloves, Slot::Boots, Slot::Amulet, Slot::Ring],
                Worth::Neutral,
            ),
            (
                "crucible_of_flame",
                "Puhuarte",
                &[Slot::Helmet, Slot::Gloves, Slot::Amulet],
                Worth::Good,
            ),
            ("defense_research_lab", "Matatl", &[Slot::Boots, Slot::Weapon], Worth::Junk),
            ("hybridisation_chamber", "Citaqualotl", &[Slot::Weapon], Worth::Neutral),
            (
                "sanctum_of_immortality",
                "Guatelitzi",
                &[Slot::BodyArmour, Slot::Amulet, Slot::Ring, Slot::Belt],
                Worth::Neutral,
            ),
            (
                "storm_of_corruption",
                "Topotante",
                &[Slot::Gloves, Slot::Shield, Slot::Weapon],
                Worth::Neutral,
            ),
            (
                "toxic_grove",
                "Tacati",
                &[Slot::BodyArmour, Slot::Shield, Slot::Weapon],
                Worth::Neutral,
            ),
        ];

        let actual_keys: Vec<&str> = DROPS
            .iter()
            .filter_map(|row| row.temple_mod().map(|_| row.key()))
            .collect();
        let expected_keys: Vec<&str> = EXPECTED.iter().map(|(key, _, _, _)| *key).collect();
        assert_eq!(actual_keys, expected_keys, "temple mod rows changed");

        for (key, architect, slots, worth) in EXPECTED {
            let temple_mod = drops_for(key)
                .temple_mod()
                .unwrap_or_else(|| panic!("{key} must carry a temple mod"));
            assert_eq!(temple_mod.architect(), architect, "{key} architect");
            assert_eq!(temple_mod.slots(), slots, "{key} slots");
            assert_eq!(temple_mod.worth(), worth, "{key} worth");
        }
    }

    // The overlay draws the "Appears on:" glyphs straight off this array, so
    // the array's order IS the reading order: armour top-to-bottom, then
    // jewellery, then shield, then weapon. The wiki's Classes column is in a
    // different order, and a row transcribed from it lands here. Fails on a
    // row written in column order, and on a slot listed twice.
    #[test]
    fn every_temple_mod_lists_its_slots_in_display_order() {
        const DISPLAY_ORDER: [Slot; 9] = [
            Slot::Helmet,
            Slot::BodyArmour,
            Slot::Gloves,
            Slot::Boots,
            Slot::Amulet,
            Slot::Ring,
            Slot::Belt,
            Slot::Shield,
            Slot::Weapon,
        ];
        let rank = |slot: Slot| {
            DISPLAY_ORDER
                .iter()
                .position(|candidate| *candidate == slot)
                .expect("DISPLAY_ORDER names every variant")
        };

        let mut checked = 0;
        for row in DROPS.iter() {
            let Some(temple_mod) = row.temple_mod() else {
                continue;
            };
            let ranks: Vec<usize> = temple_mod.slots().iter().map(|slot| rank(*slot)).collect();
            let mut ascending = ranks.clone();
            ascending.sort_unstable();
            ascending.dedup();
            assert_eq!(
                ranks,
                ascending,
                "{} lists {:?}, not display order",
                row.key(),
                temple_mod.slots()
            );
            checked += 1;
        }
        assert_eq!(checked, 7, "the temple-mod rows moved out from under this");
    }

    // DROPS is keyed 1:1 on LINES and in the same order, which is what makes
    // RoomLine::drops total rather than a lookup that can miss. Fails if a row
    // is dropped, duplicated, renamed, or moved out of LINES order.
    #[test]
    fn every_line_has_a_drops_row_in_the_same_order() {
        assert_eq!(DROPS.len(), LINES.len(), "DROPS and LINES differ in length");
        for (drops, line) in DROPS.iter().zip(LINES.iter()) {
            assert_eq!(
                drops.key(),
                line.key(),
                "DROPS is out of step with LINES at {}",
                line.key()
            );
            assert_eq!(
                line.drops().key(),
                line.key(),
                "{} resolves wrong",
                line.key()
            );
        }
        assert_eq!(
            for_key("apex_of_atzoatl"),
            None,
            "a non-line resolved to a row"
        );
    }

    // Tier 0 is filler and belongs to no line, so a tier-0 lookup must not
    // silently hand back tier 1's row. Fails if `tier` indexes without the
    // 1..=3 guard.
    #[test]
    fn tier_zero_has_no_drops_row() {
        let line = drops_for("crucible_of_flame");
        assert_eq!(line.tier(Tier::T0), None);
        assert_eq!(line.tier(tier(1)), Some(&line.tiers()[0]));
        assert_eq!(line.tier(tier(3)), Some(&line.tiers()[2]));
    }

    // ------------------------------------------------------------ names --

    // Every unique in the table has to be a string poe.ninja actually
    // publishes, or POE-255/257 join to nothing and the room silently prices at
    // zero. Fails on any typo — "Mask of Spirit Drinker" for "Mask of the
    // Spirit Drinker".
    #[test]
    fn every_item_name_in_the_table_is_one_poe_ninja_publishes() {
        let uniques = fixture("[unique]");
        let absent = fixture("[absent]");

        for row in DROPS.iter() {
            if let Some(unique) = row.unique() {
                assert!(
                    uniques.contains(&unique) || absent.contains(&unique),
                    "{}: {unique:?} is neither published by poe.ninja nor listed as absent",
                    row.key()
                );
            }
        }

        // No name in the table is unpublished now. Pinned so that an
        // unpriceable name cannot be waved through by adding it to the fixture.
        assert!(absent.is_empty(), "the drops table has an unpriceable name: {absent:?}");
    }

    #[test]
    fn content_lines_are_exactly_the_non_chest_lines() {
        const CONTENT_LINES: [&str; 19] = [
            "apex_of_ascension",
            "atlas_of_worlds",
            "chamber_of_iron",
            "court_of_sealed_death",
            "gem",
            "factory",
            "glittering_halls",
            "hall_of_champions",
            "hall_of_legends",
            "hall_of_war",
            "house_of_the_others",
            "corruption",
            "museum_of_artefacts",
            "sadists_den",
            "explosive",
            "storm_of_corruption",
            "upgrade",
            "throne_of_atziri",
            "wealth_of_the_vaal",
        ];

        let content: Vec<&str> = DROPS
            .iter()
            .filter(|row| row.content().is_some())
            .map(|row| row.key())
            .collect();
        assert_eq!(content.len(), 19);
        assert_eq!(content, CONTENT_LINES);

        for row in DROPS.iter() {
            assert_eq!(
                row.content().is_some(),
                row.unique().is_none(),
                "{} kind fields disagree",
                row.key()
            );
        }
    }

    /// The kind is DERIVED rather than stored, and this is what that buys: the
    /// two facts cannot drift apart the way a second column would let them.
    /// A row with a chest unique draws the `A + B -> C` recipe and a row
    /// without draws prose, so a kind that disagreed with `unique` would draw
    /// the wrong one of those on the box.
    #[test]
    fn every_rows_kind_is_chest_exactly_when_the_line_drops_a_unique() {
        for row in DROPS.iter() {
            let expected = match row.unique() {
                Some(_) => LineKind::Chest,
                None => LineKind::Content,
            };
            assert_eq!(row.kind(), expected, "{} kind does not follow its unique", row.key());
        }

        // The six, stated so the loop above cannot pass vacuously on a table
        // that had drifted to one kind throughout.
        assert_eq!(DROPS.iter().filter(|row| row.kind() == LineKind::Chest).count(), 6);
    }

    // ----------------------------------------------------------- rates --

    // "only tier 3 rooms can drop unique" (Vertolka, 2026-09-06). Fails if a
    // unique rate is ever put on tier 1 or 2, or if a rate appears on a line
    // that names no unique to price it against.
    #[test]
    fn only_tier_three_carries_a_unique_rate() {
        for row in DROPS.iter() {
            for (index, drops) in row.tiers().iter().enumerate().take(2) {
                assert_eq!(
                    drops.uniques_per_run(),
                    None,
                    "{} tier {} claims a unique rate",
                    row.key(),
                    index + 1
                );
            }
            if row.tiers()[2].uniques_per_run().is_some() {
                assert!(
                    row.unique().is_some(),
                    "{} rates uniques but names none to price",
                    row.key()
                );
            }
        }
        let rated: Vec<&str> = DROPS
            .iter()
            .filter(|row| row.tiers()[2].uniques_per_run().is_some())
            .map(|row| row.key())
            .collect();
        assert_eq!(rated, CHEST_LINES);
    }

    // Vertolka's numbers are his estimates and poedb's are the game's; the two
    // must never be stored as the same kind of fact. Fails if Crucible's gloves
    // are promoted to Measured, or a poedb percentage demoted to Guess.
    #[test]
    fn crucible_gloves_are_guessed_while_its_percentages_are_measured() {
        let crucible = drops_for("crucible_of_flame");
        let temple_mod = crucible.temple_mod().expect("Crucible has a temple mod");
        assert_eq!(temple_mod.architect(), "Puhuarte");

        let per_run = temple_mod.per_run().expect("Vertolka gave a rate");
        assert_eq!(per_run.value(), 2.0);
        assert_eq!(
            per_run.basis(),
            Basis::Guess {
                source: VERTOLKA_MSG
            }
        );

        let price = temple_mod
            .base_price_chaos()
            .expect("Vertolka gave a base price");
        assert_eq!(price.value(), 30.0);
        assert!(price.is_guess(), "a base price nobody measured is a guess");

        let t3 = crucible.tier(Tier::T3).expect("tier 3 exists");
        let quantity = t3.quantity_pct().expect("poedb states the percentage");
        assert_eq!(quantity.value(), 6.0);
        assert_eq!(quantity.basis(), Basis::Measured { source: POEDB });
        assert!(!quantity.is_guess());
    }

    // Factory is the only room whose page prints the quantity stat twice, and
    // the stored figure has to be both of them. Fails if the architect's
    // 60% is stored alone (60.0), or the room's 6% alone.
    #[test]
    fn factory_quantity_sums_both_lines_poedb_prints() {
        let factory = drops_for("factory");
        let expected = [22.0, 44.0, 66.0];
        for (index, drops) in factory.tiers().iter().enumerate() {
            let quantity = drops.quantity_pct().expect("Factory states a quantity");
            assert_eq!(
                quantity.value(),
                expected[index],
                "Factory tier {} quantity",
                index + 1
            );
            assert_eq!(quantity.source(), FACTORY_QUANTITY);
        }
        // Its rarity and pack size are the plain room lines, unsummed.
        assert_eq!(
            factory.tiers()[2].rarity_pct().map(Estimate::value),
            Some(12.0)
        );
        assert_eq!(
            factory.tiers()[2].pack_size_pct().map(Estimate::source),
            Some(POEDB)
        );
    }

    // ------------------------------------------------------- provenance --

    // The AC's "explicit none": a line nobody has quantified reports nothing
    // rather than zero, and reports nothing guessed. Fails if any field of an
    // empty line is filled in, or if is_empty stops looking at a field.
    #[test]
    fn a_line_with_no_drops_reports_empty_and_nothing_guessed() {
        for key in ["museum_of_artefacts", "wealth_of_the_vaal", "sadists_den"] {
            let row = drops_for(key);
            assert!(row.is_empty(), "{key} is not empty");
            assert!(!row.has_guess(), "{key} has a guess but no drops");
            assert_eq!(row.unique(), None, "{key} names a unique");
            assert_eq!(row.temple_mod(), None, "{key} names a temple mod");
            for drops in row.tiers() {
                assert!(drops.is_empty(), "{key} has a non-empty tier");
                assert_eq!(drops.quantity_pct(), None);
            }
        }

        // The other side of the same contract: Hall of War names no unique and
        // no temple mod, so only its per-tier pack size keeps it off
        // the empty list. A line that drops something is not empty.
        let hall_of_war = drops_for("hall_of_war");
        assert_eq!(hall_of_war.unique(), None);
        assert_eq!(hall_of_war.temple_mod(), None);
        assert!(
            !hall_of_war.is_empty(),
            "Hall of War carries a pack-size bonus, so it is not empty"
        );

        // And the temple-mod half of the same clause. Storm of Corruption's
        // three tiers are all NONE and it names no unique, so the
        // architect's mod group is the only thing keeping it off the empty
        // list. Fails if is_empty stops looking at temple_mod.
        assert!(
            !drops_for("storm_of_corruption").is_empty(),
            "a named temple mod is not nothing"
        );
    }

    // The two rushed lines. Doryani's Institute drops nothing this table
    // prices; Locus of Corruption has no chest unique — its altar makes
    // Shadowstitch from the Sacrificial Garb the player brings. Fails if the
    // 0.25 is applied to Locus by pattern rather than because his sheet
    // states it, or if a temple mod or bonus is transcribed onto it.
    #[test]
    fn the_rushed_lines_name_no_unique_rate() {
        let doryani = drops_for("gem");
        assert!(doryani.is_empty(), "Doryani's Institute is not empty");

        let locus = drops_for("corruption");
        assert_eq!(locus.unique(), None);
        assert_eq!(locus.temple_mod(), None);
        for (index, drops) in locus.tiers().iter().enumerate() {
            assert_eq!(
                drops.uniques_per_run(),
                None,
                "Locus tier {} claims a unique rate",
                index + 1
            );
            assert_eq!(drops.quantity_pct(), None, "Locus tier {}", index + 1);
            assert_eq!(drops.rarity_pct(), None, "Locus tier {}", index + 1);
            assert_eq!(drops.pack_size_pct(), None, "Locus tier {}", index + 1);
        }
    }

    // POE-366: 3.30 removed vials, so Glittering Halls — whose only guessed
    // number was its derived vial rate (its rarity is poedb's) — is measured.
    #[test]
    fn a_line_whose_only_guess_was_its_vial_is_not_guessed() {
        assert!(
            !drops_for("glittering_halls").has_guess(),
            "Glittering Halls carries only poedb's rarity"
        );
    }

    // has_guess is what POE-257/260 read to mark a value as somebody's
    // estimate: the set it is true on is exactly the six chest lines carrying
    // Vertolka's 0.25. Fails if a Guess is stored as Measured or if has_guess
    // stops walking the tiers. (The temple-mod arm is pinned by
    // `a_guess_that_sits_only_in_the_temple_mod_still_marks_the_line`.)
    #[test]
    fn has_guess_marks_every_chest_line_and_no_other() {
        let guessed: Vec<&str> = DROPS
            .iter()
            .filter(|row| row.has_guess())
            .map(|row| row.key())
            .collect();
        assert_eq!(guessed, CHEST_LINES);

        // A temple mod is not guessed merely by existing: Crucible's carries
        // Vertolka's two numbers, Storm of Corruption's carries none. Fails if
        // TempleMod::has_guess stops reading either field, or starts reporting
        // true for a mod with no numbers at all.
        let crucible = drops_for("crucible_of_flame")
            .temple_mod()
            .expect("Crucible names a temple mod");
        assert!(
            crucible.has_guess(),
            "Crucible's gloves are Vertolka's guess"
        );
        let storm = drops_for("storm_of_corruption")
            .temple_mod()
            .expect("Storm of Corruption names a temple mod");
        assert!(
            !storm.has_guess(),
            "Topotante's mod group has no number to guess at"
        );

        // Every guessed unique rate is attributed to Vertolka's message.
        for row in DROPS.iter() {
            for drops in row.tiers() {
                if let Some(unique) = drops.uniques_per_run() {
                    assert_eq!(unique.source(), VERTOLKA_MSG, "{}", row.key());
                    assert!(unique.is_guess(), "{}", row.key());
                }
            }
        }
    }

    // Vertolka's unique ratio is a flat number on the six chest lines: 1/4 of
    // a unique per run at tier 3, and no page states a per-tier chance to
    // scale it by. Fails on an off-by-a-decimal (0.025) and on a rate leaking
    // onto a seventh line.
    #[test]
    fn tier_three_carries_vertolkas_quarter_unique_unscaled() {
        for key in CHEST_LINES {
            let t3 = drops_for(key).tier(Tier::T3).expect("tier 3 exists");
            assert_eq!(
                t3.uniques_per_run().map(Estimate::value),
                Some(0.25),
                "{key} unique rate"
            );
        }
    }

    // The arm of LineDrops::has_guess that the real table cannot exercise: a
    // line whose ONLY guessed number sits in its temple mod. Every DROPS row
    // with a guessed temple mod also has guessed tier rates, so deleting the
    // `temple_mod` arm changes no answer on the shipped data. Fails the moment
    // it is deleted.
    #[test]
    fn a_guess_that_sits_only_in_the_temple_mod_still_marks_the_line() {
        let mod_guessed_only = LineDrops {
            key: "not_a_real_line",
            unique: None,
            content: None,
            temple_mod: Some(TempleMod {
                architect: "Puhuarte",
                item_hint: "temple gloves",
                slots: &[Slot::Helmet, Slot::Gloves, Slot::Amulet],
                worth: Worth::Good,
                source: VERTOLKA_SHEET,
                per_run: Some(Estimate::guess(2.0, VERTOLKA_MSG)),
                base_price_chaos: None,
            }),
            note: None,
            tiers: [TierDrops::NONE, TierDrops::NONE, TierDrops::NONE],
        };
        assert!(
            mod_guessed_only.tiers().iter().all(|t| !t.has_guess()),
            "the guess must sit in the temple mod alone or this proves nothing"
        );
        assert!(mod_guessed_only.has_guess());
    }

    // ------------------------------------------------------- commentary --

    // Vertolka's Commentary column is the only prose in the sheet the table
    // keeps, and it is kept verbatim: the overlay quotes him, so a tidied-up
    // sentence would be words he did not write. Fails if a cell is blanked,
    // reworded, attached to the wrong line, or invented for a tenth row.
    #[test]
    fn vertolkas_commentary_is_carried_verbatim_on_the_rows_he_wrote_one_for() {
        for (key, text) in SHEET_COMMENTARY {
            assert_eq!(drops_for(key).note(), Some(text), "{key} commentary");
        }
        let carried: Vec<&str> = DROPS
            .iter()
            .filter(|row| row.note().is_some())
            .map(|row| row.key())
            .collect();
        let expected: Vec<&str> = SHEET_COMMENTARY.iter().map(|(key, _)| *key).collect();
        assert_eq!(
            carried, expected,
            "a row carries commentary he did not write"
        );

        // A remark about a room is not a drop, which is why `note` is outside
        // both is_empty and has_guess. Museum of Artefacts is the case that
        // proves it: Vertolka wrote "nothing worth much" precisely because the
        // room drops nothing this table prices.
        let museum = drops_for("museum_of_artefacts");
        assert_eq!(museum.note(), Some("nothing worth much"));
        assert!(museum.is_empty(), "commentary is not a drop");
        assert!(!museum.has_guess(), "commentary is not a number");
    }

    // ---------------------------------------------------------- numbers --

    // Every area bonus poedb prints is one figure and its multiples: T1, twice
    // T1, three times T1, for quantity, rarity and pack size alike (2/4/6,
    // 4/8/12, 1/2/3, 20/40/60, 10/20/30, and Factory's summed 22/44/66). That
    // shape is what makes a slipped digit visible at all — 6 for 60 reads as
    // perfectly plausible on its own. Fails on Glittering Halls tier-3 rarity
    // 60 -> 6, or on any tier-2 value transposed with its tier-3 neighbour.
    #[test]
    fn every_area_bonus_is_its_tier_one_figure_times_the_tier() {
        let bonuses: [(&str, fn(TierDrops) -> Option<Estimate>); 3] = [
            ("quantity", TierDrops::quantity_pct),
            ("rarity", TierDrops::rarity_pct),
            ("pack size", TierDrops::pack_size_pct),
        ];
        for row in DROPS.iter() {
            let tiers = row.tiers();
            for (name, read) in bonuses {
                let value = |index: usize| read(tiers[index]).map(Estimate::value);
                let Some(t1) = value(0) else {
                    // No tier-1 figure means the page states this bonus
                    // nowhere on the line, not that it starts at tier 2.
                    assert_eq!(value(1), None, "{} tier 2 {name}", row.key());
                    assert_eq!(value(2), None, "{} tier 3 {name}", row.key());
                    continue;
                };
                assert_eq!(value(1), Some(t1 * 2.0), "{} tier 2 {name}", row.key());
                assert_eq!(value(2), Some(t1 * 3.0), "{} tier 3 {name}", row.key());
            }
        }
    }

    // The raw poedb integers have no internal shape to check them against, so
    // the check is a second transcription of the same pages, made from the
    // evidence set rather than from this file. Fails on a single wrong digit
    // in the 12 mod-item values, and on the stat given to a fifth line.
    #[test]
    fn the_raw_poedb_chances_match_a_second_transcription_of_the_pages() {
        for (key, item) in POEDB_RAW_CHANCES {
            let stored_item: Vec<Option<u32>> = drops_for(key)
                .tiers()
                .iter()
                .map(|t| t.mod_item_chance_raw())
                .collect();
            assert_eq!(stored_item, item.map(Some).to_vec(), "{key} mod-item chance");
        }

        let transcribed: Vec<&str> = POEDB_RAW_CHANCES.iter().map(|(key, _)| *key).collect();
        for row in DROPS.iter().filter(|row| !transcribed.contains(&row.key())) {
            for drops in row.tiers() {
                assert_eq!(
                    drops.mod_item_chance_raw(),
                    None,
                    "{} carries an untranscribed mod-item chance",
                    row.key()
                );
            }
        }
    }
}
