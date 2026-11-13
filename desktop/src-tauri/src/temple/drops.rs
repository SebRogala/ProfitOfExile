//! What each of the 25 room lines is expected to DROP, per tier (POE-256).
//!
//! [`rooms`](super::rooms) owns the closed name vocabulary and Vertolka's letter
//! grade. This module owns the numbers behind that letter: the tier-3 chest
//! unique and the rarity / pack-size percentages the room adds.
//!
//! Epic lock L1 (POE-124) is what this table feeds: room value = sale price +
//! drops (expected count x LIVE price) + bonus (rarity x chaos-per-%).
//! **No price lives here.** Prices are always fetched; a name in this table is a
//! join key, not a value.
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
//! Four sources for the NUMBERS, and only four:
//!
//! - [`POEDB`] — the room's own page on poedb.tw, read 2026-09-06. This is the
//!   game's data: the rarity / pack-size lines, the item-chance stat
//!   and the unique listed on the room's `Unique` tab. Measured.
//! - [`VERTOLKA_SHEET`] — Vertolka's room sheet, read 2026-09-06. Used for
//!   Vertolka's commentary ([`LineDrops::note`]); every number it states is
//!   also on poedb, and poedb is cited for those.
//! - [`VERTOLKA_MSG`] — Vertolka's 2026-09-06 message quoted on POE-124. Every
//!   number from it is a [`Basis::Guess`], because he said so: "price of unique
//!   divided by 4" (hence 0.25 per run at tier 3).
//! - Vertolka's 3.30 sheet export, 2026-11-13 (POE-369) — the rows cited by
//!   [`SHEET_LOCUS_330`], [`SHEET_CRADLE_330`] and [`SHEET_GLITTERING_330`]:
//!   the corruption rooms' rarity and pack size, the Cradle of Creation line's
//!   magic pack size and Glittering Halls' rarity. Measured.
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
//! # No item quantity (POE-369)
//!
//! 3.30 removed item quantity, so no line carries a quantity percentage and no
//! room value has a quantity term.
//!
//! [`TierDrops::uniques_per_run`] does NOT scale: no stat on any page states a
//! per-tier chest-unique chance, so his 0.25 stays a flat tier-3 number on the
//! six lines whose sheet row names a unique.
//!
//! No line carries the architect's temple-mod items: 3.30 removed every
//! source of them (POE-367).
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
//! The schema is numeric — counts per run and percentages. Five further statements on those pages and on the sheet are
//! real drops or real bonuses with no number and no field here. They are
//! listed rather than passed over in silence, because a reader of this table
//! would otherwise conclude the rooms state nothing.
//!
//! poedb states, and this table does not carry:
//!
//! - `apex_of_ascension`, tiers 1-3 — "Ahuana drops an additional Unique Item".
//!   An extra unique per run, on the one line whose own value is the sacrifice.
//!   3.30: the altar returns multiple uniques proportional to the one
//!   sacrificed.
//! - `atlas_of_worlds` tier 3 — "Temple Architects drop 1 additional Scarabs".
//! - `sadists_den`, all tiers — "Area is haunted by 5 additional Tormented
//!   Spirits".
//!
//! [`VERTOLKA_SHEET`]'s `global temple modifier` column states two more with
//! no percentage in them at all: Sadist Den "from T2 is Omnitect possesed" and
//! Storm of Corruption "adding corrupting/radiating tempest - high rarity
//! buff".
//!
//! Removed in 3.30, and so no longer listed: `upgrade` "Monsters drop items 1
//! Levels higher" and Temple Nexus "increases item level of items". Throne of
//! Atziri's "increase magic monster packs" is now carried as
//! [`TierDrops::magic_pack_size_pct`].
//!
//! All five are **accepted as outside POE-256's numeric scope**. Each would
//! need either a unit no field here has (a monster rarity, a
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
/// Vertolka's room sheet, read 2026-09-06 — the source of [`LineDrops::note`].
#[allow(dead_code)] // Named by the docs only since POE-367 removed the temple-mod rows that cited it.
pub const VERTOLKA_SHEET: &str = "Vertolka's sheet, 2026-09-06";
/// The 3.30 corruption-room bonus (POE-369, Operator ruling T27 2026-11-13
/// 00:14: "4% from rarity ... and yeah pack size 1% per tier").
pub const SHEET_LOCUS_330: &str = "Vertolka's sheet export 2026-11-13: gid0.csv row 10 (Locus of Corruption 4-6% rarity, 1% pack size per tier) and g744679169.csv row 24 (1% pack size per tier where the room gives 4% IIR), read at the standard eight's 4/8/12 and 1/2/3; not yet confirmed in game";
/// The 3.30 Cradle of Creation bonus (POE-369, Operator ruling T28 2026-11-13
/// 00:20 confirmed 10 % per tier).
pub const SHEET_CRADLE_330: &str = "Vertolka's sheet export 2026-11-13: gid0.csv row 8 (Cradle of Creations 10% to magic monster pack size) and g744679169.csv row 24 (Cradle of Creation 10% to magic pack size), read per tier like Hall of War's 10/20/30; per tier confirmed by the Operator 2026-11-13 00:20";
/// The 3.30 Glittering Halls rarity (POE-369).
pub const SHEET_GLITTERING_330: &str = "Vertolka's sheet export 2026-11-13: gid0.csv row 13 (Glittering Halls 12% rarity) and g744679169.csv row 27 (12% IIR per tier, exact)";
/// Vertolka's 2026-09-06 message, quoted on epic POE-124. Everything sourced
/// here is a [`Basis::Guess`]; he offered the numbers as his own estimates.
pub const VERTOLKA_MSG: &str = "Vertolka, 2026-09-06 message (POE-124)";

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

// --------------------------------------------------------------- per tier --

/// What one tier of one line is expected to drop.
///
/// Every field is `Option`: `None` means nobody has stated a number, which is a
/// different claim from zero and must stay distinguishable from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TierDrops {
    uniques_per_run: Option<Estimate>,
    rarity_pct: Option<Estimate>,
    pack_size_pct: Option<Estimate>,
    magic_pack_size_pct: Option<Estimate>,
}

impl TierDrops {
    /// A tier that drops nothing anybody has quantified — the explicit "none"
    /// for the lines with no chest and no area bonus.
    pub const NONE: TierDrops = TierDrops {
        uniques_per_run: None,
        rarity_pct: None,
        pack_size_pct: None,
        magic_pack_size_pct: None,
    };

    /// Expected tier-3 chest uniques per run. Always `None` at tiers 1 and 2 —
    /// the game drops the chest unique at tier 3 only.
    pub fn uniques_per_run(self) -> Option<Estimate> {
        self.uniques_per_run
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

    /// `increased Magic Monster Pack size`, in percent. Carried, not priced and
    /// not on the wire, like pack size.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn magic_pack_size_pct(self) -> Option<Estimate> {
        self.magic_pack_size_pct
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
            self.rarity_pct,
            self.pack_size_pct,
            self.magic_pack_size_pct,
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

    /// Vertolka's own `Commentary` cell for this line, **verbatim** —
    /// [`VERTOLKA_SHEET`] is the source and its typos are his ("you should't
    /// need it", the stray closing bracket on Wealth of the Vaal). Seven of the
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
        self.unique.is_none() && self.tiers.iter().all(|t| t.is_empty())
    }

    /// **Is anything in this room's value guessed?** POE-257 reads it to weight
    /// the number down; POE-260 reads it to mark the overlay.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn has_guess(&self) -> bool {
        self.tiers.iter().any(|t| t.has_guess())
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
        // 3.30 patch note: "can now generate multiple Unique Items, proportional
        // to the value of the Unique Item sacrificed".
        content: Some("Sacrifice a unique for uniques of its worth"),
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
        // 3.30 patch note: the device has "multiple uses in higher-tier rooms";
        // sheet export 2026-11-13 gid0.csv row 23 "corrupt maps for second time
        // (higher tier provide more uses)" (its 1/3/5? counts not adopted).
        content: Some("Map drops; re-corrupt maps (uses rise by tier)"),
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
        // 3.30 removed temple-mod items and this cell's 3.30 text is unverified (sheet not fetched).
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "conduit_of_lightning",
        unique: Some("Dance of the Offered"),
        content: None,
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "court_of_sealed_death",
        unique: None,
        content: Some("Arcanist & Diviner strongboxes"),
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
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "defense_research_lab",
        unique: Some("Architect's Hand"),
        content: None,
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "gem",
        unique: None,
        content: Some("Double-corrupt a gem"),
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
        // Sheet export 2026-11-13 gid0.csv row 11 "increase yield of items";
        // 3.30 removed item quantity.
        content: Some("Extra item drops"),
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "glittering_halls",
        unique: None,
        content: Some("Jewellery drops"),
        // The 3.30 sheet row (gid0.csv row 13) carries no fractured note.
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(12.0, SHEET_GLITTERING_330)),
                pack_size_pct: None,
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(24.0, SHEET_GLITTERING_330)),
                pack_size_pct: None,
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(36.0, SHEET_GLITTERING_330)),
                pack_size_pct: None,
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "hall_of_champions",
        unique: None,
        content: Some("Weapon drops"),
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "hall_of_legends",
        unique: None,
        content: Some("Legion encounter"),
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
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: None,
                pack_size_pct: Some(Estimate::measured(10.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: None,
                pack_size_pct: Some(Estimate::measured(20.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: None,
                pack_size_pct: Some(Estimate::measured(30.0, POEDB)),
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "house_of_the_others",
        unique: None,
        content: Some("Breach encounter"),
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
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
                magic_pack_size_pct: None,
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
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(4.0, SHEET_LOCUS_330)),
                pack_size_pct: Some(Estimate::measured(1.0, SHEET_LOCUS_330)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(8.0, SHEET_LOCUS_330)),
                pack_size_pct: Some(Estimate::measured(2.0, SHEET_LOCUS_330)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(12.0, SHEET_LOCUS_330)),
                pack_size_pct: Some(Estimate::measured(3.0, SHEET_LOCUS_330)),
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "museum_of_artefacts",
        unique: None,
        content: Some("Extra item drops"),
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
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "explosive",
        unique: None,
        content: Some("Destroy rooms / explosives"),
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
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(4.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(1.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(8.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(2.0, POEDB)),
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: Some(Estimate::measured(12.0, POEDB)),
                pack_size_pct: Some(Estimate::measured(3.0, POEDB)),
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "throne_of_atziri",
        unique: None,
        // 3.30 patch note: "area level 84 and contain Uber Atziri"; sheet export
        // 2026-11-13 gid0.csv row 8 "Area level 84, uber Atziri".
        content: Some("Uber Atziri, area level 84"),
        note: None,
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: None,
                pack_size_pct: None,
                magic_pack_size_pct: Some(Estimate::measured(10.0, SHEET_CRADLE_330)),
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: None,
                pack_size_pct: None,
                magic_pack_size_pct: Some(Estimate::measured(20.0, SHEET_CRADLE_330)),
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: None,
                pack_size_pct: None,
                magic_pack_size_pct: Some(Estimate::measured(30.0, SHEET_CRADLE_330)),
            },
        ],
    },
    LineDrops {
        key: "toxic_grove",
        unique: Some("Apep's Slumber"),
        content: None,
        note: Some("can be good with corrupting tempest for 6-link farming"),
        tiers: [
            TierDrops {
                uniques_per_run: None,
                rarity_pct: None,
                pack_size_pct: None,
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: None,
                rarity_pct: None,
                pack_size_pct: None,
                magic_pack_size_pct: None,
            },
            TierDrops {
                uniques_per_run: Some(Estimate::guess(0.25, VERTOLKA_MSG)),
                rarity_pct: None,
                pack_size_pct: None,
                magic_pack_size_pct: None,
            },
        ],
    },
    LineDrops {
        key: "wealth_of_the_vaal",
        unique: None,
        content: Some("Currency drops"),
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

    /// Seven of Vertolka's nine non-blank `Commentary` cells, transcribed from
    /// the CSV with his typos left in place ("you should't", the unmatched
    /// bracket on Wealth of the Vaal), in [`DROPS`] order. Reworded commentary
    /// is not his commentary, so this is a character-for-character copy.
    /// Chamber of Iron's and Glittering Halls' cells are dropped as stale in
    /// 3.30 pending a re-read.
    const SHEET_COMMENTARY: [(&str, &str); 7] = [
        (
            "court_of_sealed_death",
            "In T3 Arcanist and Diviner Stronboxes are quite common",
        ),
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


    // ------------------------------------------------------------ shape --





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

    // Every row's content prose, in DROPS order, pinned in one table. POE-369
    // updated four lines to the 3.30 notes (Apex, Atlas, Factory, Throne).
    // The box clamps content to two lines, so every string is at most 46
    // characters. Fails if any line's text is edited, blanked, moved or grown
    // past the clamp.
    #[test]
    fn every_rooms_content_text_is_this() {
        const EXPECTED: [(&str, Option<&str>); 25] = [
            ("apex_of_ascension", Some("Sacrifice a unique for uniques of its worth")),
            ("atlas_of_worlds", Some("Map drops; re-corrupt maps (uses rise by tier)")),
            ("chamber_of_iron", Some("Armour drops")),
            ("conduit_of_lightning", None),
            ("court_of_sealed_death", Some("Arcanist & Diviner strongboxes")),
            ("crucible_of_flame", None),
            ("defense_research_lab", None),
            ("gem", Some("Double-corrupt a gem")),
            ("factory", Some("Extra item drops")),
            ("glittering_halls", Some("Jewellery drops")),
            ("hall_of_champions", Some("Weapon drops")),
            ("hall_of_legends", Some("Legion encounter")),
            ("hall_of_war", Some("Rare monster packs")),
            ("house_of_the_others", Some("Breach encounter")),
            ("hybridisation_chamber", None),
            ("corruption", Some("Double-corrupt an item")),
            ("museum_of_artefacts", Some("Extra item drops")),
            ("sadists_den", Some("Extra monsters (dangerous)")),
            ("sanctum_of_immortality", None),
            ("explosive", Some("Destroy rooms / explosives")),
            ("storm_of_corruption", Some("Corrupting tempest")),
            ("upgrade", Some("Upgrade a room")),
            ("throne_of_atziri", Some("Uber Atziri, area level 84")),
            ("toxic_grove", None),
            ("wealth_of_the_vaal", Some("Currency drops")),
        ];

        let carried: Vec<(&str, Option<&str>)> =
            DROPS.iter().map(|row| (row.key(), row.content())).collect();
        assert_eq!(carried, EXPECTED);
        for (key, content) in carried {
            if let Some(content) = content {
                assert!(
                    content.chars().count() <= 46,
                    "{key} content {content:?} is over the two-line clamp"
                );
            }
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

    // poedb's numbers are the game's and must never be stored as a guess.
    // Fails if a poedb percentage is demoted to Guess.
    #[test]
    fn crucibles_percentages_are_measured_not_guessed() {
        let crucible = drops_for("crucible_of_flame");
        let t3 = crucible.tier(Tier::T3).expect("tier 3 exists");
        let rarity = t3.rarity_pct().expect("poedb states the percentage");
        assert_eq!(rarity.value(), 12.0);
        assert_eq!(rarity.basis(), Basis::Measured { source: POEDB });
        assert!(!rarity.is_guess());
    }

    // Factory's rarity and pack size are the plain room lines. Fails if
    // Factory's tier-3 rarity or its pack-size source drifts from poedb's.
    #[test]
    fn factorys_rarity_and_pack_size_are_the_plain_room_lines() {
        let factory = drops_for("factory");
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
        for key in [
            "museum_of_artefacts",
            "wealth_of_the_vaal",
            "sadists_den",
            "storm_of_corruption",
        ] {
            let row = drops_for(key);
            assert!(row.is_empty(), "{key} is not empty");
            assert!(!row.has_guess(), "{key} has a guess but no drops");
            assert_eq!(row.unique(), None, "{key} names a unique");
            for drops in row.tiers() {
                assert!(drops.is_empty(), "{key} has a non-empty tier");
            }
        }

        // The other side of the same contract: Hall of War names no unique, so only its
        // per-tier pack size keeps it off the empty list. A line that drops something is not empty.
        let hall_of_war = drops_for("hall_of_war");
        assert_eq!(hall_of_war.unique(), None);
        assert!(
            !hall_of_war.is_empty(),
            "Hall of War carries a pack-size bonus, so it is not empty"
        );
    }

    // The two rushed lines. Doryani's Institute drops nothing this table
    // prices; Locus of Corruption has no chest unique — its altar makes
    // Shadowstitch from the Sacrificial Garb the player brings. Fails if the
    // 0.25 is applied to Locus by pattern rather than because his sheet
    // states it.
    #[test]
    fn the_rushed_lines_name_no_unique_rate() {
        let doryani = drops_for("gem");
        assert!(doryani.is_empty(), "Doryani's Institute is not empty");

        let locus = drops_for("corruption");
        assert_eq!(locus.unique(), None);
        for (index, drops) in locus.tiers().iter().enumerate() {
            assert_eq!(
                drops.uniques_per_run(),
                None,
                "Locus tier {} claims a unique rate",
                index + 1
            );
        }
    }

    // POE-369, T24/T27: the 3.30 sheet gives the corruption rooms 4 % rarity
    // and 1 % pack size per tier, not yet confirmed in game. The extra 1-2 %
    // redirected from quantity has no value term, which the exact 4/8/12
    // pins. Fails if a tier's figure, its basis or the unverified marking
    // drifts, or if the room gains a unique rate.
    #[test]
    fn the_corruption_rooms_add_the_sheets_rarity_and_pack_size() {
        let locus = drops_for("corruption");
        let measured = Basis::Measured { source: SHEET_LOCUS_330 };
        let rarity: Vec<Option<(f64, Basis)>> = locus
            .tiers()
            .iter()
            .map(|t| t.rarity_pct().map(|e| (e.value(), e.basis())))
            .collect();
        assert_eq!(
            rarity,
            vec![Some((4.0, measured)), Some((8.0, measured)), Some((12.0, measured))]
        );
        let pack: Vec<Option<(f64, Basis)>> = locus
            .tiers()
            .iter()
            .map(|t| t.pack_size_pct().map(|e| (e.value(), e.basis())))
            .collect();
        assert_eq!(
            pack,
            vec![Some((1.0, measured)), Some((2.0, measured)), Some((3.0, measured))]
        );
        assert!(SHEET_LOCUS_330.contains("not yet confirmed in game"));
        assert_eq!(locus.unique(), None);
        assert!(locus.tiers().iter().all(|t| t.uniques_per_run().is_none()));
    }

    // POE-369, T24/T28: the 3.30 sheet gives the Cradle of Creation line 10 %
    // magic monster pack size per tier. Fails if a tier's figure or its basis
    // drifts, or if the bonus is filed as rarity or plain pack size.
    #[test]
    fn cradle_of_creation_adds_ten_percent_magic_pack_size_per_tier() {
        let cradle = drops_for("throne_of_atziri");
        let measured = Basis::Measured { source: SHEET_CRADLE_330 };
        let magic: Vec<Option<(f64, Basis)>> = cradle
            .tiers()
            .iter()
            .map(|t| t.magic_pack_size_pct().map(|e| (e.value(), e.basis())))
            .collect();
        assert_eq!(
            magic,
            vec![Some((10.0, measured)), Some((20.0, measured)), Some((30.0, measured))]
        );
        for t in cradle.tiers() {
            assert_eq!(t.rarity_pct(), None);
            assert_eq!(t.pack_size_pct(), None);
        }
    }

    // Magic pack size is the Cradle line's alone. Fails if the bonus is
    // copied onto any other row.
    #[test]
    fn no_line_but_the_cradle_line_carries_magic_pack_size() {
        for row in DROPS.iter().filter(|row| row.key() != "throne_of_atziri") {
            for (index, t) in row.tiers().iter().enumerate() {
                assert_eq!(
                    t.magic_pack_size_pct(),
                    None,
                    "{} tier {} carries magic pack size",
                    row.key(),
                    index + 1
                );
            }
        }
    }

    // POE-369, T24: the 3.30 sheet gives Glittering Halls 12 % rarity per
    // tier. Fails if a tier keeps poedb's old 20/40/60 or loses its source.
    #[test]
    fn glittering_halls_rarity_is_12_24_36_per_the_3_30_sheet() {
        let measured = Basis::Measured { source: SHEET_GLITTERING_330 };
        let rarity: Vec<Option<(f64, Basis)>> = drops_for("glittering_halls")
            .tiers()
            .iter()
            .map(|t| t.rarity_pct().map(|e| (e.value(), e.basis())))
            .collect();
        assert_eq!(
            rarity,
            vec![Some((12.0, measured)), Some((24.0, measured)), Some((36.0, measured))]
        );
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
    // stops walking the tiers.
    #[test]
    fn has_guess_marks_every_chest_line_and_no_other() {
        let guessed: Vec<&str> = DROPS
            .iter()
            .filter(|row| row.has_guess())
            .map(|row| row.key())
            .collect();
        assert_eq!(guessed, CHEST_LINES);

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
    // T1, three times T1, for rarity and pack size alike (4/8/12, 1/2/3,
    // 12/24/36, 10/20/30). That
    // shape is what makes a slipped digit visible at all — 6 for 60 reads as
    // perfectly plausible on its own. Fails on Glittering Halls tier-3 rarity
    // 36 -> 6, or on any tier-2 value transposed with its tier-3 neighbour.
    #[test]
    fn every_area_bonus_is_its_tier_one_figure_times_the_tier() {
        let bonuses: [(&str, fn(TierDrops) -> Option<Estimate>); 2] = [
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

}
