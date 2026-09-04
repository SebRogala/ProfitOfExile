//! The temple's closed room vocabulary (POE-169).
//!
//! 25 room *lines* of three tiers each, 10 tier-0 fillers, the Entrance and the
//! Apex of Atzoatl — **87 names, and the game can print no other**. Everything
//! here is text: no pixels, no OCR, no Tauri, so the whole module runs in the
//! Linux test container.
//!
//! # Where the table comes from
//!
//! RePoE's `dat-export … IncursionRooms.csv` (2026-08-18). The T1→T2→T3 chains
//! are the game's own `RoomUpgrade` keys rather than anyone's memory of them,
//! and the tier-3 grades are Vertolka's sheet. Three of his spellings differ
//! from game data (`Ascencion`, `House of the Other`, `Sadist Den`); they are
//! carried as explicit [`ALIASES`] rather than left to the fuzzy matcher, so a
//! sheet import resolves deterministically instead of depending on a threshold.
//!
//! # Match on FULL names, never on a word
//!
//! "Shrine of Unmaking" is the *explosives* line and "Shrine of Empowerment"
//! the upgrade line; "Hall of Locks" and "Hall of Lords" are different lines
//! one character apart. Every lookup in this module is against a whole
//! normalised name, and [`match_room_name`] additionally refuses a read that
//! does not clear its runner-up — see [`LEAD`].
//!
//! # The tier-1-name gotcha
//!
//! With Contested Development taken (POE-167 assumes it), the room the panel
//! *names* is not the room that gets built. [`resolve_offer`] is the only
//! function that should ever produce user-facing text for an architect offer.

// POE-171 is that caller: `temple::run` and `temple::slice` reach this module
// on every read, so the file-level `#![allow(dead_code)]` is gone. What is
// still uncalled carries its own attribute, which is now the inventory of what
// only the tests reach rather than a blanket over the whole file.

use strsim::jaro_winkler;

use super::strategy::{Line, Tier};

// ------------------------------------------------------------------ grades --

/// Vertolka's tier-3 ranking for a line, imported now and unused by scoring
/// until POE-170 decides what weight (if any) a third-party grade carries.
///
/// Declared **worst first**, so the derived [`Ord`] reads "greater is better".
/// Every one of the 25 lines is graded on his sheet, which is why this is not
/// an `Option`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    D,
    CMinus,
    C,
    CPlus,
    BMinus,
    B,
    BPlus,
    A,
    APlus,
    APlusPlus,
}

impl Grade {
    /// The grade as it is written on the sheet — the wire form of this enum
    /// (`slice::OfferView::grade`, POE-249), because `Grade` itself is a
    /// reasoning type with a derived worst-first `Ord` and no `Serialize`.
    pub fn as_str(self) -> &'static str {
        match self {
            Grade::D => "D",
            Grade::CMinus => "C-",
            Grade::C => "C",
            Grade::CPlus => "C+",
            Grade::BMinus => "B-",
            Grade::B => "B",
            Grade::BPlus => "B+",
            Grade::A => "A",
            Grade::APlus => "A+",
            Grade::APlusPlus => "A++",
        }
    }
}

// ------------------------------------------------------------------- lines --

/// One three-tier room family.
///
/// The tier-1 name is the line's root but not its identity: the player cares
/// about `tiers[2]`, which is also what Vertolka grades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomLine {
    key: &'static str,
    tiers: [&'static str; 3],
    grade: Grade,
}

impl RoomLine {
    /// Canonical key. For the four mechanically relevant lines this is
    /// POE-167's key, so [`RoomLine::mechanical_line`] round-trips through
    /// [`Line::named`]; for the other 21 it is a snake_case slug of the tier-3
    /// name, which is the name the strategy layer talks about.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn key(self) -> &'static str {
        self.key
    }

    /// The three room names, tier 1 first.
    #[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
    pub fn tiers(self) -> [&'static str; 3] {
        self.tiers
    }

    /// This line's room at `tier`, or `None` for [`Tier::T0`] — tier 0 is
    /// filler and belongs to no line.
    pub fn name(self, tier: Tier) -> Option<&'static str> {
        match tier.get() {
            1..=3 => Some(self.tiers[tier.get() as usize - 1]),
            _ => None,
        }
    }

    /// Vertolka's grade for this line's tier-3 room. Published per offer since
    /// POE-249 (`slice::offer_view`), paired with the tier-3 name so the letter
    /// is never read as a grade of the room the kill actually builds.
    pub fn grade(self) -> Grade {
        self.grade
    }

    /// The POE-167 [`Line`] this family scores as.
    ///
    /// Built through [`Line::named`] rather than by matching variants here, so
    /// the four mechanical keys can never drift out of sync with `strategy`'s
    /// own constants: a typo in [`LINES`] would surface as an unscored
    /// `Line::Other("corrupton")` in POE-170 instead of silently comparing
    /// unequal.
    pub fn mechanical_line(self) -> Line {
        Line::named(self.key)
    }
}

/// The Entrance plate's name — a fixed slot, never a room.
pub const ENTRANCE_NAME: &str = "Entrance";
/// The Apex plate's name. Never a drop room, never an architect target.
pub const APEX_NAME: &str = "Apex of Atzoatl";

/// The ten tier-0 fillers. They carry no line, upgrade to nothing, and an
/// upgrade-room's random pick landing on one is simply wasted.
pub const FILLERS: [&str; 10] = [
    "Chasm",
    "Passageways",
    "Halls",
    "Tunnels",
    "Pits",
    "Banquet Hall",
    "Tombs",
    "Antechamber",
    "Cellar",
    "Cloister",
];

/// The 25 lines, in the source table's order.
pub const LINES: [RoomLine; 25] = [
    RoomLine {
        key: "apex_of_ascension",
        tiers: [
            "Sacrificial Chamber",
            "Hall of Offerings",
            "Apex of Ascension",
        ],
        grade: Grade::BMinus,
    },
    RoomLine {
        key: "atlas_of_worlds",
        tiers: [
            "Surveyor's Study",
            "Office of Cartography",
            "Atlas of Worlds",
        ],
        grade: Grade::D,
    },
    RoomLine {
        key: "chamber_of_iron",
        tiers: ["Armourer's Workshop", "Armoury", "Chamber of Iron"],
        grade: Grade::C,
    },
    RoomLine {
        key: "conduit_of_lightning",
        tiers: [
            "Lightning Workshop",
            "Omnitect Reactor Plant",
            "Conduit of Lightning",
        ],
        grade: Grade::C,
    },
    RoomLine {
        key: "court_of_sealed_death",
        tiers: [
            "Strongbox Chamber",
            "Hall of Locks",
            "Court of Sealed Death",
        ],
        grade: Grade::D,
    },
    RoomLine {
        key: "crucible_of_flame",
        tiers: ["Flame Workshop", "Omnitect Forge", "Crucible of Flame"],
        grade: Grade::APlus,
    },
    RoomLine {
        key: "defense_research_lab",
        tiers: [
            "Trap Workshop",
            "Temple Defense Workshop",
            "Defense Research Lab",
        ],
        grade: Grade::C,
    },
    RoomLine {
        key: "gem",
        tiers: [
            "Gemcutter's Workshop",
            "Department of Thaumaturgy",
            "Doryani's Institute",
        ],
        grade: Grade::APlus,
    },
    RoomLine {
        key: "factory",
        tiers: ["Workshop", "Engineering Department", "Factory"],
        grade: Grade::B,
    },
    RoomLine {
        key: "glittering_halls",
        tiers: ["Jeweller's Workshop", "Jewellery Forge", "Glittering Halls"],
        grade: Grade::B,
    },
    RoomLine {
        key: "hall_of_champions",
        tiers: ["Sparring Room", "Arena of Valour", "Hall of Champions"],
        grade: Grade::C,
    },
    RoomLine {
        key: "hall_of_legends",
        tiers: ["Hall of Mettle", "Hall of Heroes", "Hall of Legends"],
        grade: Grade::D,
    },
    RoomLine {
        key: "hall_of_war",
        tiers: ["Guardhouse", "Barracks", "Hall of War"],
        grade: Grade::C,
    },
    RoomLine {
        key: "house_of_the_others",
        tiers: [
            "Anomaly Research Lab",
            "Breach Containment Chamber",
            "House of the Others",
        ],
        grade: Grade::D,
    },
    RoomLine {
        key: "hybridisation_chamber",
        tiers: ["Hatchery", "Automaton Lab", "Hybridisation Chamber"],
        grade: Grade::C,
    },
    RoomLine {
        key: "corruption",
        tiers: [
            "Corruption Chamber",
            "Catalyst of Corruption",
            "Locus of Corruption",
        ],
        grade: Grade::APlusPlus,
    },
    RoomLine {
        key: "museum_of_artefacts",
        tiers: ["Storage Room", "Warehouses", "Museum of Artefacts"],
        grade: Grade::D,
    },
    RoomLine {
        key: "sadists_den",
        tiers: ["Torment Cells", "Torture Cages", "Sadist's Den"],
        grade: Grade::C,
    },
    RoomLine {
        key: "sanctum_of_immortality",
        tiers: [
            "Pools of Restoration",
            "Sanctum of Vitality",
            "Sanctum of Immortality",
        ],
        grade: Grade::A,
    },
    RoomLine {
        key: "explosive",
        tiers: ["Explosives Room", "Demolition Lab", "Shrine of Unmaking"],
        grade: Grade::D,
    },
    RoomLine {
        key: "storm_of_corruption",
        tiers: [
            "Tempest Generator",
            "Hurricane Engine",
            "Storm of Corruption",
        ],
        grade: Grade::CMinus,
    },
    RoomLine {
        key: "upgrade",
        tiers: ["Shrine of Empowerment", "Sanctum of Unity", "Temple Nexus"],
        grade: Grade::BPlus,
    },
    RoomLine {
        key: "throne_of_atziri",
        tiers: ["Royal Meeting Room", "Hall of Lords", "Throne of Atziri"],
        grade: Grade::CMinus,
    },
    RoomLine {
        key: "toxic_grove",
        tiers: ["Poison Garden", "Cultivar Chamber", "Toxic Grove"],
        grade: Grade::CPlus,
    },
    RoomLine {
        key: "wealth_of_the_vaal",
        tiers: ["Vault", "Treasury", "Wealth of the Vaal"],
        grade: Grade::D,
    },
];

/// Spellings that are not the game's but must resolve anyway: Vertolka's sheet
/// writes these three differently. `(printed, game name)`.
pub const ALIASES: [(&str, &str); 3] = [
    ("Apex of Ascencion", "Apex of Ascension"),
    ("House of the Other", "House of the Others"),
    ("Sadist Den", "Sadist's Den"),
];

// ---------------------------------------------------------------- identity --

/// What a room name resolves to.
///
/// `Filler` carries the name because R4 (deliberately maxing junk out of the
/// drop pool) still has to tell one filler from another when it reports what
/// the board looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomIdentity {
    /// Slot E1. Not a room: it has no line, no tier and never drops.
    Entrance,
    /// Slot A0. Not a room either, and never an architect target.
    Apex,
    /// A tier-0 filler, by its name.
    Filler(&'static str),
    /// A real room: which line, and which of its three tiers.
    Room { line: &'static RoomLine, tier: Tier },
}

impl RoomIdentity {
    /// The game's own name for this identity.
    pub fn display_name(self) -> &'static str {
        match self {
            RoomIdentity::Entrance => ENTRANCE_NAME,
            RoomIdentity::Apex => APEX_NAME,
            RoomIdentity::Filler(name) => name,
            RoomIdentity::Room { line, tier } => line
                .name(tier)
                .expect("a Room identity is only ever built at tier 1..=3"),
        }
    }

    /// The tier the *board* should score this slot at. Entrance, Apex and
    /// fillers are all [`Tier::T0`] — none of them has a line to advance.
    pub fn tier(self) -> Tier {
        match self {
            RoomIdentity::Room { tier, .. } => tier,
            _ => Tier::T0,
        }
    }

    /// The line this identity belongs to, or `None` for the three
    /// line-less kinds.
    pub fn line(self) -> Option<&'static RoomLine> {
        match self {
            RoomIdentity::Room { line, .. } => Some(line),
            _ => None,
        }
    }
}

/// Every name the game can print, with what it means. 87 entries plus the
/// three [`ALIASES`].
fn vocabulary() -> impl Iterator<Item = (&'static str, RoomIdentity)> {
    let fixed = [
        (ENTRANCE_NAME, RoomIdentity::Entrance),
        (APEX_NAME, RoomIdentity::Apex),
    ]
    .into_iter();
    let fillers = FILLERS.into_iter().map(|n| (n, RoomIdentity::Filler(n)));
    let rooms = LINES.iter().flat_map(|line| {
        [1u8, 2, 3].into_iter().map(move |t| {
            let tier = Tier::new(t).expect("1..=3 are tiers");
            (
                line.tiers[t as usize - 1],
                RoomIdentity::Room { line, tier },
            )
        })
    });
    let aliases = ALIASES.into_iter().map(|(printed, game)| {
        (
            printed,
            resolve_exact(game).expect("every alias points at a game name"),
        )
    });
    fixed.chain(fillers).chain(rooms).chain(aliases)
}

/// Fold a printed or OCR'd name onto its comparison key.
///
/// Lower-cases, deletes apostrophes of every shape (so `Sadists Den` and
/// `Sadist's Den` are the same key), turns every other non-alphanumeric into a
/// space, and collapses runs of whitespace. Digits survive: `0mnitect` has to
/// stay a near-miss of `Omnitect` rather than becoming a different length.
pub fn normalise(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_space = false;
    for ch in name.chars() {
        match ch {
            '\'' | '\u{2018}' | '\u{2019}' | '`' | '\u{00B4}' => continue,
            c if c.is_ascii_alphanumeric() => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push(c.to_ascii_lowercase());
            }
            c if c.is_alphanumeric() => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.extend(c.to_lowercase());
            }
            _ => pending_space = true,
        }
    }
    out
}

/// Exact (post-[`normalise`]) lookup. `None` for anything outside the closed
/// vocabulary — this never guesses.
#[allow(dead_code)] // Only the tests reach this; comes off with its first production caller.
pub fn resolve_name(name: &str) -> Option<RoomIdentity> {
    let key = normalise(name);
    if key.is_empty() {
        return None;
    }
    vocabulary()
        .find(|(label, _)| normalise(label) == key)
        .map(|(_, id)| id)
}

/// [`resolve_name`] restricted to the 87 game names, used while building the
/// alias table (an alias cannot resolve through another alias).
fn resolve_exact(name: &str) -> Option<RoomIdentity> {
    let key = normalise(name);
    if let Some(name) = FILLERS.into_iter().find(|f| normalise(f) == key) {
        return Some(RoomIdentity::Filler(name));
    }
    if key == normalise(ENTRANCE_NAME) {
        return Some(RoomIdentity::Entrance);
    }
    if key == normalise(APEX_NAME) {
        return Some(RoomIdentity::Apex);
    }
    for line in LINES.iter() {
        for (i, tier_name) in line.tiers.iter().enumerate() {
            if normalise(tier_name) == key {
                return Some(RoomIdentity::Room {
                    line,
                    tier: Tier::new(i as u8 + 1).expect("1..=3 are tiers"),
                });
            }
        }
    }
    None
}

// ------------------------------------------------------------ fuzzy match --

/// Jaro-Winkler score a fuzzy read must reach.
///
/// **PROVISIONAL — measured off synthetic OCR noise, not an error
/// distribution**, exactly like `mercenary::Thresholds`. Two numbers bracket
/// it, both measured over this vocabulary:
///
/// - the loosest read that must still land: `Ternple Nexus` (the `rn`→`m`
///   confusion, the commonest OCR failure on this UI font) scores **0.8883**
///   against `Temple Nexus`;
/// - the tightest same-length foreign text that must be rejected:
///   `Molten Strike` scores **0.6838** against `Torment Cells`.
///
/// 0.88 sits 0.0083 under the first. That margin is thin and this is the first
/// number a Windows OCR dump should re-derive. It is deliberately *below*
/// `mercenary`'s 0.92: that vocabulary is 534 entries with heavy word reuse,
/// this one is 87, and the entries that actually collide here are separated by
/// [`LEAD`], not by an absolute score — the worst in-vocabulary pair
/// (`Sanctum of Vitality` / `Sanctum of Immortality`) already scores 0.9531,
/// so no absolute threshold can tell them apart.
pub const MATCH: f64 = 0.88;

/// Lead over the best *different* identity a fuzzy read must hold.
///
/// Required **always** — there is no `no_lead` escape hatch of the kind
/// `mercenary::Thresholds::name_no_lead` provides, and that is the whole point:
/// `Hall of Lorks` scores **0.9692 against BOTH `Hall of Lords` and
/// `Hall of Locks`**, so a 0.97 escape hatch would have accepted a coin flip.
/// Measured boundary: a read that drops one character (`Hall of Lo ds`) leads
/// by 0.0308 and is genuinely ambiguous; a read with one substituted character
/// (`Sanctum of Vitalitv`) leads by 0.0455 and is not. 0.04 splits them.
pub const LEAD: f64 = 0.04;

/// Narrowest `len(query) / len(candidate)` a fuzzy read may have, on
/// normalised text.
///
/// Jaro-Winkler is prefix-weighted and nearly blind to an unmatched tail, so
/// without this a *fragment* scores like a read. `Chamber` is the measured
/// boundary case: it hits **0.8933** against `Chamber of Iron` with a 0.0400
/// lead, clearing both other gates, and is rejected solely by its 0.467 ratio.
/// `Shrine` (0.333 against `Shrine of Unmaking`) is the same failure one word
/// further along. The loosest read that must survive is a dropped trailing
/// word — `Breach Containment` sits at 0.692 — so the gate is set between the
/// two rather than at either.
pub const RATIO_MIN: f64 = 0.60;

/// Widest `len(query) / len(candidate)` a fuzzy read may have.
///
/// The mirror of [`RATIO_MIN`], and the *only* gate that rejects two names
/// OCR'd as one: `Sacrificial Chamber Hall of Offerings` scores **0.9027**
/// against `Sacrificial Chamber` — comfortably over [`MATCH`], with a 0.2365
/// lead — and is caught solely by its 1.947 length ratio.
pub const RATIO_MAX: f64 = 1.45;

/// The result of matching OCR text against the closed vocabulary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Match {
    /// The normalised text *is* a vocabulary name (or one of the [`ALIASES`]).
    Exact(RoomIdentity),
    /// Close enough, with the Jaro-Winkler score that got it there.
    Fuzzy(RoomIdentity, f64),
    /// Nothing cleared the gates. **Never downgraded to a guess** — POE-171
    /// shows this to the user as an unread plate.
    Unknown,
}

impl Match {
    /// The identity, for the two variants that have one.
    pub fn identity(self) -> Option<RoomIdentity> {
        match self {
            Match::Exact(id) => Some(id),
            Match::Fuzzy(id, _) => Some(id),
            Match::Unknown => None,
        }
    }

    /// Whether this read may be acted on.
    pub fn is_known(self) -> bool {
        self.identity().is_some()
    }
}

/// Match one OCR'd room name against the closed vocabulary.
///
/// Three gates, all of them measured on this vocabulary: [`MATCH`] rejects
/// foreign text, [`RATIO_MIN`]/[`RATIO_MAX`] reject fragments and run-together
/// pairs, [`LEAD`] rejects a read that two names fit equally well.
pub fn match_room_name(ocr_text: &str) -> Match {
    let query = normalise(ocr_text);
    if query.is_empty() {
        return Match::Unknown;
    }

    let mut scored: Vec<(f64, String, RoomIdentity)> = Vec::with_capacity(90);
    for (label, identity) in vocabulary() {
        let candidate = normalise(label);
        if candidate == query {
            return Match::Exact(identity);
        }
        let score = jaro_winkler(&query, &candidate);
        scored.push((score, candidate, identity));
    }

    // `max_by` keeps the LAST of equal elements, so a tie goes to the later
    // vocabulary entry. Which one it goes to cannot change the answer: two
    // entries tied at the top have zero lead over each other, so a tie between
    // two *different* identities is rejected by [`LEAD`] below, and a tie
    // between two spellings of the SAME identity (a name and its alias)
    // returns that identity either way.
    let best = scored
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i);
    let Some(best) = best else {
        return Match::Unknown;
    };
    let (score, ref candidate, identity) = scored[best];

    // A different SPELLING of the same identity is not a rival: `Apex of
    // Ascencion` must not veto `Apex of Ascension`.
    let runner_up = scored
        .iter()
        .filter(|(_, _, other)| *other != identity)
        .map(|(s, _, _)| *s)
        .fold(0.0_f64, f64::max);

    let ratio = query.chars().count() as f64 / candidate.chars().count() as f64;
    if !(RATIO_MIN..=RATIO_MAX).contains(&ratio) {
        return Match::Unknown;
    }
    if score >= MATCH && score - runner_up >= LEAD {
        Match::Fuzzy(identity, score)
    } else {
        Match::Unknown
    }
}

/// Cross-check a read against the plate's `I`/`II`/`III` numeral when one was
/// legible.
///
/// The numeral is free confirmation, never the primary read: room names are
/// unique per tier, so name→tier is already a function. A disagreement
/// demotes the read to [`Match::Unknown`] rather than trusting either half.
pub fn cross_check_numeral(read: Match, numeral: Option<Tier>) -> Match {
    let (Some(id), Some(tier)) = (read.identity(), numeral) else {
        return read;
    };
    if id.tier() == tier {
        read
    } else {
        Match::Unknown
    }
}

/// Parse a plate's tier numeral. `None` for a tier-0 plate, which prints none.
pub fn parse_numeral(text: &str) -> Option<Tier> {
    match normalise(text).as_str() {
        "i" | "l" | "1" => Tier::new(1),
        "ii" | "ll" | "11" | "il" | "li" => Tier::new(2),
        "iii" | "lll" | "111" => Tier::new(3),
        _ => None,
    }
}

// -------------------------------------------------------- architect offers --

/// Which architect an offer belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferKind {
    /// Kill the **resident** architect. Base game: swaps to exactly the named
    /// room. With Contested Development: `currentTier + 1` of the named
    /// room's LINE — see [`resolve_offer`].
    Change,
    /// Kill the **non-resident** architect: same line, `currentTier + 1`.
    Upgrade,
}

/// What killing an architect actually builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedOffer {
    /// The line the printed name belongs to.
    pub line: &'static RoomLine,
    /// The tier that will actually exist afterwards.
    pub built_tier: Tier,
    /// The room name to show the user. **Not** the printed name.
    pub display_name: &'static str,
}

/// Resolve an architect's printed target into the room that will be built.
///
/// # Why the printed name is not the answer
///
/// Confirmed in the wild (TEMPLE-CORE-RULES §6d): the panel offered *"Kill to
/// change to **Sacrificial Chamber**"* on a **tier-2** room and built **Apex of
/// Ascension III**. Contested Development — which POE-167's
/// [`super::strategy::DOUBLE_TIER_CHANCE`] documents as assumed taken — turns
/// `change` into "`currentTier + 1` of the named line". Echoing the game's
/// wording is therefore a correctness bug, not a cosmetic one.
///
/// # One formula for both kinds
///
/// `upgrade` prints tier `current + 1` of the room's own line, so
/// `min(3, current + 1)` of the printed name's line is already the right
/// answer; `change` needs the same arithmetic for a different reason. Both
/// kinds go through here, which is why [`OfferKind`] is not a parameter.
///
/// # What this deliberately does NOT model
///
/// The 50% double-tier roll on `upgrade`
/// ([`super::strategy::DOUBLE_TIER_CHANCE`]) is POE-170's: it is a
/// *distribution*, and this function returns the deterministic floor that both
/// kinds guarantee. A caller showing an upgrade offer must add "or tier 3 at
/// 50%" itself.
///
/// # Boundaries
///
/// - A printed name that resolves to the Entrance, the Apex or a filler has no
///   line, so this returns `None`. The game does not offer them.
/// - `current_tier` 3 caps at 3. Tier-3 rooms leave the drop pool and cannot be
///   stood in, so this is a boundary rather than a reachable case.
/// - `current_tier` 0 yields tier 1: Contested Development's flat +1 still
///   applies, and `0 → 2` never happens.
pub fn resolve_offer(printed_target: &str, current_tier: Tier) -> Option<ResolvedOffer> {
    let line = match_room_name(printed_target).identity()?.line()?;
    let built = Tier::new((current_tier.get() + 1).min(Tier::MAX_VALUE))?;
    Some(ResolvedOffer {
        line,
        built_tier: built,
        display_name: line.name(built)?,
    })
}

/// What [`resolve_offer_for`] could say about one architect block.
///
/// Three outcomes rather than an `Option`, because the two failures are
/// different facts about the read and the overlay must not print the same line
/// for both: an unreadable name is an OCR problem, an unknown current tier is a
/// board the reader has not finished establishing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferResolution {
    /// What the kill builds.
    Built(ResolvedOffer),
    /// The printed target is not one of the 75 tiered rooms.
    UnknownName,
    /// The name is in the vocabulary, but the arithmetic needs a current tier
    /// this read never established.
    UnknownCurrentTier,
}

/// [`resolve_offer`] for a read that may not know the current tier.
///
/// # Why the kind matters here and nowhere else
///
/// With a known `current_tier` both kinds are the same `min(3, current + 1)`
/// arithmetic and this delegates verbatim — [`OfferKind`] changes nothing.
///
/// Without one the two kinds part company, because only one of them prints the
/// answer:
///
/// - [`OfferKind::Upgrade`] prints tier `current + 1` of the room's OWN line,
///   so the printed name IS the built room. Its own tier is the built tier and
///   the current tier is not needed at all. This is POE-229: standing in
///   **Office of Cartography** with the plate unread, *"upgrade to Atlas of
///   Worlds"* builds Atlas of Worlds III, not the tier-1 Surveyor's Study that
///   `current_tier = 0` produced.
/// - [`OfferKind::Change`] names a DIFFERENT line, so the built tier is a fact
///   about the room the player is standing in and nothing in the printed text
///   carries it. There is no answer to give, and assuming tier 1 prints a room
///   the kill will not build.
pub fn resolve_offer_for(
    printed_target: &str,
    kind: OfferKind,
    current_tier: Option<Tier>,
) -> OfferResolution {
    let Some(identity) = match_room_name(printed_target).identity() else {
        return OfferResolution::UnknownName;
    };
    let Some(line) = identity.line() else {
        return OfferResolution::UnknownName;
    };
    if let Some(current_tier) = current_tier {
        return match resolve_offer(printed_target, current_tier) {
            Some(resolved) => OfferResolution::Built(resolved),
            None => OfferResolution::UnknownName,
        };
    }
    match kind {
        OfferKind::Change => OfferResolution::UnknownCurrentTier,
        OfferKind::Upgrade => {
            let built = identity.tier();
            match line.name(built) {
                Some(display_name) => OfferResolution::Built(ResolvedOffer {
                    line,
                    built_tier: built,
                    display_name,
                }),
                None => OfferResolution::UnknownName,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_by_key(key: &str) -> &'static RoomLine {
        LINES
            .iter()
            .find(|l| l.key == key)
            .unwrap_or_else(|| panic!("no line keyed {key}"))
    }

    fn tier(n: u8) -> Tier {
        Tier::new(n).expect("test tier is 0..=3")
    }

    // ------------------------------------------------------ vocabulary --

    // The whole point of the module: every one of the 75 tiered names is a
    // key into exactly one (line, tier). Fails if any T1/T2/T3 cell of LINES
    // is duplicated, misspelled against its own row, or transposed between
    // rows.
    #[test]
    fn every_tiered_name_resolves_to_its_own_line_and_tier() {
        let mut checked = 0;
        for line in LINES.iter() {
            for (i, name) in line.tiers.iter().enumerate() {
                let want = tier(i as u8 + 1);
                match resolve_name(name) {
                    Some(RoomIdentity::Room { line: got, tier: t }) => {
                        assert_eq!(got.key(), line.key(), "{name} resolved to the wrong line");
                        assert_eq!(t, want, "{name} resolved to the wrong tier");
                        assert_eq!(
                            RoomIdentity::Room { line: got, tier: t }.display_name(),
                            *name,
                            "{name} does not round-trip through display_name"
                        );
                    }
                    other => panic!("{name} resolved to {other:?}, not a Room"),
                }
                checked += 1;
            }
        }
        assert_eq!(checked, 75, "the table no longer holds 25 lines of 3");
    }

    // The three line-less kinds must not be swept into Room{}: the Entrance
    // and the Apex never drop and never upgrade, and a filler has no line for
    // an architect to advance. Fails if any of them is added to LINES.
    #[test]
    fn the_fixed_slots_and_the_ten_fillers_resolve_to_their_own_kinds() {
        assert_eq!(resolve_name("Entrance"), Some(RoomIdentity::Entrance));
        assert_eq!(resolve_name("Apex of Atzoatl"), Some(RoomIdentity::Apex));
        for filler in FILLERS {
            assert_eq!(
                resolve_name(filler),
                Some(RoomIdentity::Filler(filler)),
                "{filler} is not a filler"
            );
            assert_eq!(
                resolve_name(filler).unwrap().tier(),
                Tier::T0,
                "{filler} is not tier 0"
            );
            assert!(
                resolve_name(filler).unwrap().line().is_none(),
                "{filler} must carry no line"
            );
        }
    }

    // resolve_name is exact, so an out-of-vocabulary string is None rather
    // than the nearest neighbour. Fails if resolve_name is ever routed
    // through the fuzzy matcher.
    #[test]
    fn a_name_the_game_cannot_print_resolves_to_nothing() {
        assert_eq!(resolve_name("Sanctum of Perpetuity"), None);
        assert_eq!(resolve_name("Molten Strike"), None);
        assert_eq!(resolve_name(""), None);
        assert_eq!(resolve_name("   "), None);
    }

    // The closed set is 25 x 3 + 10 + 2 = 87, and every name must be unique
    // or name -> (line, tier) stops being a function. Fails on any duplicate
    // introduced by a copy-paste into LINES or FILLERS.
    #[test]
    fn the_vocabulary_is_87_unique_names_over_25_lines() {
        assert_eq!(LINES.len(), 25);
        let names: Vec<&str> = vocabulary()
            .map(|(label, _)| label)
            .filter(|label| !ALIASES.iter().any(|(printed, _)| printed == label))
            .collect();
        assert_eq!(names.len(), 87, "the closed set is not 87 names");
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 87, "the closed set contains a duplicate name");
        // Normalised too: two names that differ only by punctuation would
        // collide in every lookup this module performs.
        let mut keys: Vec<String> = names.iter().map(|n| normalise(n)).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 87, "two names share a normalised key");
    }

    // Vertolka's sheet is the source of the grade column, and three of its
    // spellings are not the game's. They are aliases, not fuzzy hits, so this
    // holds no matter where MATCH sits. Fails if ALIASES is dropped.
    #[test]
    fn the_three_vertolka_spellings_resolve_to_the_game_names() {
        for (printed, game) in ALIASES {
            assert_eq!(
                resolve_name(printed),
                resolve_name(game),
                "{printed} does not fold onto {game}"
            );
            assert_eq!(
                resolve_name(printed)
                    .expect("alias resolves")
                    .display_name(),
                game,
                "{printed} must DISPLAY as {game}"
            );
        }
        assert_eq!(
            resolve_name("Apex of Ascencion")
                .expect("alias resolves")
                .display_name(),
            "Apex of Ascension"
        );
    }

    // Every line carries a grade, and the derived Ord reads best-last.
    // Fails if a Grade variant is reordered.
    #[test]
    fn grades_are_ordered_worst_first() {
        assert!(Grade::APlusPlus > Grade::APlus);
        assert!(Grade::APlus > Grade::A);
        assert!(Grade::A > Grade::BPlus);
        assert!(Grade::BMinus > Grade::D);
        assert_eq!(line_by_key("corruption").grade(), Grade::APlusPlus);
        assert_eq!(line_by_key("corruption").grade().as_str(), "A++");
        assert_eq!(line_by_key("gem").grade(), Grade::APlus);
        assert_eq!(line_by_key("explosive").grade(), Grade::D);
    }

    // ------------------------------------------------- bridge to POE-167 --

    // The four mechanically relevant lines must land on their Line variants,
    // not on Line::Other with the same text — POE-167 documents that an
    // Other holding a known key scores zero. Fails if a key in LINES is
    // misspelled against strategy.rs's KEY_* constants.
    #[test]
    fn the_four_mechanical_lines_map_to_their_strategy_variants() {
        assert_eq!(
            line_by_key("corruption").mechanical_line(),
            Line::Corruption
        );
        assert_eq!(line_by_key("gem").mechanical_line(), Line::Gem);
        assert_eq!(line_by_key("upgrade").mechanical_line(), Line::Upgrade);
        assert_eq!(line_by_key("explosive").mechanical_line(), Line::Explosive);
        // …and they are the lines the strategy doc names, by their tier-3 room.
        assert_eq!(line_by_key("corruption").tiers()[2], "Locus of Corruption");
        assert_eq!(line_by_key("gem").tiers()[2], "Doryani's Institute");
        assert_eq!(line_by_key("upgrade").tiers()[2], "Temple Nexus");
        assert_eq!(line_by_key("explosive").tiers()[2], "Shrine of Unmaking");
    }

    // The other 21 lines are addressable junk: Line::Other by key, never one
    // of the four. Fails if a tail key is spelled "corruption"/"gem"/etc.
    #[test]
    fn the_other_twenty_one_lines_are_addressable_but_not_mechanical() {
        let mut tail = 0;
        for line in LINES.iter() {
            match line.mechanical_line() {
                Line::Other(key) => {
                    assert_eq!(key, line.key(), "Other must carry the line's own key");
                    tail += 1;
                }
                mechanical => assert!(
                    matches!(
                        mechanical,
                        Line::Corruption | Line::Gem | Line::Upgrade | Line::Explosive
                    ),
                    "unexpected variant for {}",
                    line.key()
                ),
            }
        }
        assert_eq!(tail, 21, "exactly four lines are mechanically relevant");
    }

    // ------------------------------------------ the tier-1-name gotcha --

    // The live case from TEMPLE-CORE-RULES §6d: the panel printed
    // "Kill to change to Sacrificial Chamber" on a TIER-2 room and the game
    // built Apex of Ascension III. Fails the moment resolve_offer echoes the
    // printed name or forgets the +1.
    #[test]
    fn sacrificial_chamber_offered_on_a_tier_2_room_builds_apex_of_ascension_iii() {
        let got = resolve_offer("Sacrificial Chamber", tier(2)).expect("a real offer");
        assert_eq!(got.display_name, "Apex of Ascension");
        assert_eq!(got.built_tier, Tier::T3);
        assert_eq!(got.line.key(), "apex_of_ascension");
        assert_ne!(
            got.display_name, "Sacrificial Chamber",
            "the advisor must never echo the game's wording"
        );
    }

    // Case 5's play: "change to Shrine of Empowerment" standing in a tier-1
    // Poison Garden yields Sanctum of Unity II, which is what makes the
    // double-neighbour upgrade certain. Fails if the +1 is dropped.
    #[test]
    fn shrine_of_empowerment_offered_on_a_tier_1_room_builds_sanctum_of_unity_ii() {
        let got = resolve_offer("Shrine of Empowerment", tier(1)).expect("a real offer");
        assert_eq!(got.display_name, "Sanctum of Unity");
        assert_eq!(got.built_tier, Tier::T2);
        assert_eq!(got.line.mechanical_line(), Line::Upgrade);
    }

    // Boundary, not a reachable case: tier-3 rooms leave the drop pool, so
    // the player cannot be standing in one. Fails if the min(3, …) clamp goes.
    #[test]
    fn an_offer_on_a_tier_3_room_caps_at_tier_3() {
        let got = resolve_offer("Corruption Chamber", tier(3)).expect("a real offer");
        assert_eq!(got.built_tier, Tier::T3);
        assert_eq!(got.display_name, "Locus of Corruption");
    }

    // Contested Development's flat +1 applies from tier 0 too, and 0 -> 2
    // never happens. Fails if the arithmetic is changed to +2 or to "the
    // printed tier".
    #[test]
    fn an_offer_on_a_tier_0_room_builds_tier_1_of_the_printed_line() {
        let got = resolve_offer("Catalyst of Corruption", Tier::T0).expect("a real offer");
        assert_eq!(got.built_tier, Tier::T1);
        assert_eq!(
            got.display_name, "Corruption Chamber",
            "the LINE is looked up, not the printed tier"
        );
    }

    // The printed target is looked up to its LINE whatever tier it names, so
    // a tier-3 name on a tier-1 room still resolves through the line.
    #[test]
    fn a_tier_3_name_on_the_panel_still_resolves_through_its_line() {
        let got = resolve_offer("Locus of Corruption", tier(1)).expect("a real offer");
        assert_eq!(got.built_tier, Tier::T2);
        assert_eq!(got.display_name, "Catalyst of Corruption");
    }

    // Line-less names have nothing to build. Fails if resolve_offer starts
    // defaulting a missing line instead of returning None.
    #[test]
    fn an_offer_naming_a_line_less_room_resolves_to_nothing() {
        assert_eq!(resolve_offer("Tombs", tier(1)), None);
        assert_eq!(resolve_offer("Entrance", tier(1)), None);
        assert_eq!(resolve_offer("Apex of Atzoatl", tier(1)), None);
        assert_eq!(resolve_offer("Fictional Room", tier(1)), None);
    }

    // The panel text is OCR'd, so the offer target has to survive noise.
    #[test]
    fn an_offer_target_read_with_noise_still_resolves() {
        let got = resolve_offer("SacrificiaI Chamber", tier(2)).expect("a fuzzy offer");
        assert_eq!(got.display_name, "Apex of Ascension");
    }

    // ------------------------------------------------------ fuzzy match --

    // Case, apostrophes and run-together whitespace are folded by normalise,
    // so these are EXACT, not fuzzy — the matcher never has to spend its
    // threshold on them. Fails if normalise stops folding any of the three.
    #[test]
    fn case_apostrophes_and_whitespace_are_absorbed_before_scoring() {
        let want = resolve_name("Doryani's Institute").expect("in vocabulary");
        for spelling in [
            "doryani's institute",
            "DORYANI'S INSTITUTE",
            "Doryanis Institute",
            "Doryani\u{2019}s Institute",
            "  Doryani's   Institute  ",
        ] {
            assert_eq!(
                match_room_name(spelling),
                Match::Exact(want),
                "{spelling} should be an exact read"
            );
        }
    }

    // A substituted character is what actually needs the threshold.
    #[test]
    fn a_substituted_character_is_a_fuzzy_read_of_the_right_room() {
        let want = resolve_name("Locus of Corruption").expect("in vocabulary");
        match match_room_name("Locus of Corruptlon") {
            Match::Fuzzy(got, score) => {
                assert_eq!(got, want);
                assert!(score >= MATCH, "score {score} under MATCH");
            }
            other => panic!("expected a fuzzy read, got {other:?}"),
        }
    }

    // The out-of-vocabulary policy, in one test. Nothing here may produce an
    // identity: a bare word, a name from another game, an empty read and two
    // room names OCR'd as one line.
    #[test]
    fn out_of_vocabulary_text_is_unknown_and_never_guessed() {
        for junk in [
            "",
            "   ",
            "Shrine",
            "Room",
            "Molten Strike",
            "Enter Incursion",
            "Sacrificial Chamber Hall of Offerings",
        ] {
            assert_eq!(
                match_room_name(junk),
                Match::Unknown,
                "{junk:?} must not resolve"
            );
        }
    }

    // MATCH boundary, pinned from both sides by measured scores.
    //
    // Above: `Ternple Nexus` (the rn->m confusion, 0.8883) lands, so raising
    // MATCH past 0.89 breaks this. Below: `F1ame W0rkshop` (l->1 and o->0,
    // both canonical digit confusions, 0.8643 with a 0.0726 lead and a 1.0
    // length ratio, so MATCH is the only gate it fails) must not, so lowering
    // MATCH to 0.86 breaks it too.
    #[test]
    fn the_match_threshold_is_pinned_from_both_sides() {
        let nexus = resolve_name("Temple Nexus").expect("in vocabulary");
        match match_room_name("Ternple Nexus") {
            Match::Fuzzy(got, score) => {
                assert_eq!(got, nexus);
                assert!(
                    (0.88..0.89).contains(&score),
                    "calibration moved: score is {score}"
                );
            }
            other => panic!("expected a fuzzy read, got {other:?}"),
        }
        assert_eq!(match_room_name("F1ame W0rkshop"), Match::Unknown);
        assert_eq!(match_room_name("Ternp1e Nexus"), Match::Unknown);
    }

    // LEAD boundary inside the Sanctum cluster, whose two worst names score
    // 0.9531 against each other. `Sanctum of Vitalitv` leads by 0.0455 and
    // lands; `Sanctum of ltality` leads by 0.0244 and does not. Fails if LEAD
    // moves, or if a no-lead escape hatch is reintroduced.
    #[test]
    fn the_lead_threshold_separates_a_decided_read_from_an_ambiguous_one() {
        let vitality = resolve_name("Sanctum of Vitality").expect("in vocabulary");
        assert!(
            matches!(match_room_name("Sanctum of Vitalitv"), Match::Fuzzy(got, _) if got == vitality)
        );
        assert_eq!(match_room_name("Sanctum of ltality"), Match::Unknown);
    }

    // The case a no-lead escape hatch would get wrong: `Hall of Lorks` scores
    // 0.9692 against BOTH `Hall of Lords` and `Hall of Locks`. A coin flip
    // must read as Unknown.
    #[test]
    fn a_read_two_names_fit_equally_well_is_unknown() {
        assert_eq!(match_room_name("Hall of Lorks"), Match::Unknown);
    }

    // Length-ratio boundary. `Chamber` clears MATCH (0.8933) and LEAD
    // (0.0400) and is rejected by RATIO_MIN alone; a dropped trailing word
    // (`Breach Containment`, ratio 0.692) still lands. Fails if RATIO_MIN
    // moves far enough to admit a bare word.
    #[test]
    fn a_bare_word_is_rejected_by_length_while_a_dropped_word_survives() {
        assert_eq!(match_room_name("Chamber"), Match::Unknown);
        let breach = resolve_name("Breach Containment Chamber").expect("in vocabulary");
        assert!(matches!(
            match_room_name("Breach Containment"),
            Match::Fuzzy(got, _) if got == breach
        ));
    }

    // An alias must not veto the name it aliases: both spellings are the same
    // identity, so the runner-up filter has to compare identities, not labels.
    #[test]
    fn an_alias_does_not_veto_the_game_spelling_it_aliases() {
        let apex = resolve_name("Apex of Ascension").expect("in vocabulary");
        assert!(matches!(
            match_room_name("Apex of Ascensiom"),
            Match::Fuzzy(got, _) if got == apex
        ));
    }

    // ----------------------------------------------------- tier numeral --

    #[test]
    fn the_plate_numeral_parses_with_its_common_ocr_confusions() {
        assert_eq!(parse_numeral("I"), Some(Tier::T1));
        assert_eq!(parse_numeral("l"), Some(Tier::T1));
        assert_eq!(parse_numeral("II"), Some(Tier::T2));
        assert_eq!(parse_numeral("11"), Some(Tier::T2));
        assert_eq!(parse_numeral("III"), Some(Tier::T3));
        assert_eq!(parse_numeral("lll"), Some(Tier::T3));
        assert_eq!(parse_numeral(""), None);
        assert_eq!(parse_numeral("IV"), None);
        assert_eq!(parse_numeral("Tombs"), None);
    }

    // The numeral is a cross-check, so agreement is a no-op and disagreement
    // demotes rather than overrides. Fails if either half is trusted over the
    // other.
    #[test]
    fn a_numeral_that_contradicts_the_name_demotes_the_read_to_unknown() {
        let read = match_room_name("Catalyst of Corruption");
        assert_eq!(cross_check_numeral(read, Some(Tier::T2)), read);
        assert_eq!(cross_check_numeral(read, Some(Tier::T3)), Match::Unknown);
        assert_eq!(cross_check_numeral(read, None), read);
        // A tier-0 filler prints no numeral, so None must not be read as a
        // contradiction.
        let filler = match_room_name("Tombs");
        assert_eq!(cross_check_numeral(filler, None), filler);
        assert_eq!(cross_check_numeral(filler, Some(Tier::T1)), Match::Unknown);
    }
}

