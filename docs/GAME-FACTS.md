# Game facts the code depends on

Status: current reference, every entry dated and sourced. This file records
facts about Path of Exile itself, as opposed to facts about this codebase.
Each one was either measured on a live game, stated by the owner from play, or
taken from the wiki, and the code cites them as invariants. When the game
changes, update the entry and its date; do not let the code drift from it
silently.

"Owner-stated" means Sebastian, from play. "Measured" names the date and the
method. Anything not yet validated is in the last section, kept separate so it
is not mistaken for a fact.

## Labyrinth

- **At most one golden door per run.** One in the whole run, never one per
  section; no layout has two. Owner-stated, 2026-07-26. This is why
  `routeWithGoldenDoor` in `desktop/src/lib/compass/navigation.ts` takes the
  first key room and the first door room and hardcodes a single two-phase
  route: that is complete, not a simplification awaiting generalisation.
  LabCompass solves N doors generically; that generality is unreachable in the
  game. Audit findings that call the single-door assumption a defect are
  invalid.
- **Every lab room is its own area load**, and Client.txt logs each load. That
  is what the path strip and lab OCR triggers key on, and it is the reason the
  temple below cannot be tracked the same way.

## Temple of Atzoatl

- **Moving between temple rooms writes nothing to Client.txt.** Measured
  2026-07-29 in a live temple, twice: a byte-diff of the log across a real
  room transition appended one unrelated `[WINDOW] Lost focus` line, and a
  grep of the full log history for room names (Sacrificial Chamber, Hall of
  Mettle, Doryani's Institute, Temple Nexus, Corruption Chamber, Pools of
  Restoration, Apex of Atzoatl) returned nothing, ever. The whole temple is one
  area, so there is no load and no line.
- **No on-screen cue names the current room** during play. Only the layout
  panel does, with a gold border on the current room. Owner-stated, same day.
- **Entry and exit do log.** Entry is `Generating level N area
  "Incursion_Temple8"` followed by `: You have entered The Temple of Atzoatl.`;
  the next `You have entered` line is the exit. Those two are what arms and
  disarms the temple module (`desktop/src-tauri/src/temple/trigger.rs`).
- Consequence: temple position is read from the layout panel when the player
  opens it, and is "last known as of the last panel open". Live tracking is
  not a missing feature; it has no signal to build on
  ([TEMPLE-LIFECYCLE.md](TEMPLE-LIFECYCLE.md)).
- **An architect offer's kind is where its target sits.** The side panel
  prints two architects per room. `Kill to upgrade to <X>` names the current
  room's own line one tier up; `Kill to change to <Y>` names the other
  architect's line. A tier-0 filler has no resident architect, so both print
  `change`, and no room prints two `upgrade`s. Seen on every transcribed
  panel: Torture Cages 2026-09-05 (upgrade to Sadist's Den, change to Shrine
  of Empowerment), Armourer's Workshop 2026-09-03 (upgrade to Armoury, change
  to Shrine of Empowerment), Tombs 2026-08-02 (two changes). The panel parser
  (`desktop/src-tauri/src/temple/panel.rs`, `decide_kind`) settles the kind
  from the title by this rule when OCR loses the verb, which Windows OCR did
  on a legible crop on 2026-09-05 (`(KILL TO TO SHRINE OF`).
- **A room is a rectangle, and its doors sit on fixed walls.** The diamond on
  the side panel is an isometric view of the current room, with one seal per
  board neighbour: green = open, red = closed. The two long walls carry two
  doors each, the board's two upward diagonals on one and the two downward
  diagonals on the other. The two short walls carry one same-row door each, at
  the wall's midpoint. Owner-stated 2026-09-04; measured on 8 boards, 32 seals
  and 2 UI scales, with an outline aspect of 1.147
  (`desktop/src-tauri/src/temple/markers.rs`, `seal_position`). Re-checked
  2026-10-03 against four more panels (Gemcutter's Workshop D0, Halls E0, Pits
  C1, Banquet Hall B1).
- **Walking length is not hop count.** Vertolka (the temple collaborator),
  2026-10-03: the routes Doryani's Institute → Pits → Conduit of Lightning and
  Doryani's Institute → Tempest Generator → Conduit of Lightning are both two
  hops, but the first is a straight walk and the second zig-zags. That follows
  from the door placement above: the straight route enters and leaves each room
  on the same side, and the zig-zag crosses each room diagonally. How much
  longer the zig-zag is in play has not been measured. The builder advisor
  counts hops (`desktop/src-tauri/src/temple/advisor/convenience.rs`).
- **The layout sheet does not change during an Alva encounter.** The player's
  position on it is frozen in the building phase. Opening a door with a Stone
  of Passage mid-run does not update the board either. Owner-stated,
  2026-10-09 (POE-282). A kill mid-incursion does not change the sheet
  either (Operator ruling, 2026-10-10). Consequence: the read the temple
  module takes is the board for the whole encounter
  (`desktop/src-tauri/src/temple/run.rs`, `BoardRead::locked`).
- **A map holds 3 incursions, 4 with Artefacts of the Vaal.** Confirmed by
  the Operator, 2026-11-12. Still true in 3.30, where incursions no longer spawn
  in maps at random (03 Endgame Changes: "The following league mechanics no
  longer have a random chance to spawn in Map areas: Abyss, Bestiary, Betrayal,
  Breach, Blight, Expedition, Incursion, Legion, Ritual, and Ultimatum.") and
  come from Incursion Threads. The advisor reads it from
  `TempleConfig::entrances_per_map` (`desktop/src-tauri/src/temple/strategy.rs`).

- **3.30.0 renamed three rooms and an architect.** Patch notes
  (https://www.pathofexile.com/forum/view-thread/4013680, 06 League Changes):
  Throne of Atziri → Cradle of Creation, Omnitect Forge → Royal Forge,
  Omnitect Reactor Plant → Grand Reactor, and "Xipocado, Royal Architect is now
  known as Xipocado, Architect to the Queen." The pre-3.30 names were removed in
  3.30.0 from the room vocabulary (`desktop/src-tauri/src/temple/rooms.rs`,
  `ALIASES`; temp league only). The persisted line keys stay
  (`throne_of_atziri`, `crucible_of_flame`, `conduit_of_lightning`; rooms.rs
  `RoomLine::key`). Also Vertolka's sheet export 2026-11-13, g744679169.csv
  rows 17-21.
- **A temple with a Cradle of Creation is area level 84.** Patch notes: "While
  the Temple of Atzoatl contains the Cradle of Creation room, it will be area
  level 84 and contain Uber Atziri." Carried as the `throne_of_atziri` line's
  content "Uber Atziri, area level 84" (`drops.rs`); sheet gid0.csv row 8,
  g744679169.csv row 32.

### Vial recipes (removed in 3.30.0)

Vials were removed in 3.30.0 (POE-366). Patch notes: "The following Vials no
longer drop: … Existing items will remain, but have no functionality." The
table below is history. The eleven Base → Upgraded pairs survive without the
vial column; their normative home is `internal/temple/upgrades.go`
(`uniqueUpgrades`).

Before 3.30.0, a vial was consumed at the Altar of Sacrifice together with a
**base** unique and transformed it into an **upgraded** unique. The vial's own
currency text named the item you SACRIFICE, not the item you receive. Verified
2026-09-06 against poedb.tw's per-vial pages (currency text, icon file name,
prose blurb and Recipe block agreeing on every row).

| Vial | Base | Upgraded |
|---|---|---|
| Vial of Awakening | Apep's Slumber | Apep's Supremacy |
| Vial of Consequence | Coward's Chains | Coward's Legacy |
| Vial of Dominance | Architect's Hand | Slavedriver's Hand |
| Vial of Fate | Story of the Vaal | Fate of the Vaal |
| Vial of Sacrifice | Sacrificial Heart | Zerphi's Heart |
| Vial of Summoning | Mask of the Spirit Drinker | Mask of the Stitched Demon |
| Vial of Transcendence | Tempered Flesh | Transcendent Flesh |
| Vial of Transcendence | Tempered Mind | Transcendent Mind |
| Vial of Transcendence | Tempered Spirit | Transcendent Spirit |
| Vial of the Ghost | Soul Catcher | Soul Ripper |
| Vial of the Ritual | Dance of the Offered | Omeyocan |

- **The room → vial pairs were removed in 3.30.0** with the vials (POE-366).
  In 3.30 eight rooms carry a chest unique, base → upgraded
  (`desktop/src-tauri/src/temple/drops.rs`, `ChestUnique::new`):
  - Conduit of Lightning: Dance of the Offered → Omeyocan
  - Crucible of Flame: Story of the Vaal → Fate of the Vaal
  - Defense Research Lab: Architect's Hand → Slavedriver's Hand
  - Glittering Halls: always has its chest and drops one of Sacrificial Heart /
    Tempered Flesh / Tempered Mind / Tempered Spirit (Operator ruling
    2026-11-13 00:20; sheet g744679169.csv row 28, gid0.csv row 13)
  - Hybridisation Chamber: Coward's Chains → Coward's Legacy
  - Museum of Artefacts: Soul Catcher → Soul Ripper (g744679169.csv row 29,
    gid0.csv row 25)
  - Sanctum of Immortality: Mask of the Spirit Drinker → Mask of the Stitched
    Demon
  - Toxic Grove: Apep's Slumber → Apep's Supremacy

  Locus of Corruption has no chest unique. Storm of Corruption "can no longer
  contain a special chest" (patch notes; POE-370). The Cradle of Creation has
  no chest of its own.
- **Temple-mod items were removed in 3.30.0** (POE-367). Patch notes: "Removed
  all sources of Incursion Architect Modifiers on Magic and Rare items." The
  pre-3.30 30 c Crucible of Flame glove guess went with them. The rooms' own
  area modifiers and descriptions stay (`LineDrops::content`, drops.rs).

### Temple room bonuses per tier (3.30, 2026-11-13)

Carried in `desktop/src-tauri/src/temple/drops.rs`. Percentages are `increased
Rarity of Items` / `increased Pack Size` / `increased Magic Pack Size` in this
area, tier 1 / tier 2 / tier 3. Twelve of the 25 lines print no such line at
all and are `None` rather than zero. Item quantity was removed in 3.30.0
(POE-369; sheet g744679169.csv row 8 "remove IIQ bunuses").

| Line (room key) | Rarity % | Pack size % | Magic pack size % |
|---|---|---|---|
| the standard eight (see below) | 4 / 8 / 12 | 1 / 2 / 3 | — |
| Factory | 4 / 8 / 12 | 1 / 2 / 3 | — |
| corruption (Locus of Corruption line) | 4 / 8 / 12 | 1 / 2 / 3 | — |
| Glittering Halls | 12 / 24 / 36 | — | — |
| Hall of War | — | 10 / 20 / 30 | — |
| Cradle of Creation (`throne_of_atziri`) | — | — | 10 / 20 / 30 |

Sources: the standard eight and Factory, poedb.tw room pages (2026-09-06).
Corruption: Vertolka sheet (`SHEET_LOCUS_330`; gid0.csv row 10,
g744679169.csv row 24), author confirmation relayed by the Operator 2026-11-13
00:14. Glittering Halls: `SHEET_GLITTERING_330` and the patch notes "up to 36%
for the Glittering Halls (previously 60%)". Cradle of Creation:
`SHEET_CRADLE_330`, per tier confirmed by the Operator 2026-11-13 00:20.

The standard eight are `chamber_of_iron`, `conduit_of_lightning`,
`crucible_of_flame`, `defense_research_lab`, `hall_of_champions`,
`hybridisation_chamber`, `sanctum_of_immortality` and `upgrade` (Temple Nexus).

Two places where Vertolka's sheet claims a bonus and poedb prints no percentage:
Toxic Grove ("increase quantity/rarity") and Storm of Corruption ("high rarity
buff"). Both are recorded as absent rather than as a figure nobody wrote down.

- **The "chance" integers were removed in 3.30.0.** The vial stat and the
  architect's-rare stat went with vials and temple-mod items (POE-366,
  POE-367); the code no longer carries `vial_chance_raw` /
  `mod_item_chance_raw`, and the POE-262 convention (raw 1689 = the
  configured vial rate) went with them.
- **Uniques drop at tier 3 only.** Vertolka-stated 2026-09-06: "only tier 3
  rooms can drop unique, but T1 and T2 adding chance to drop vial and provide
  smaller quant/rarity bonuses" (drops.rs). The vial half was removed in 3.30.0
  (POE-366). Patch notes: "Unique items that required Vials to obtain instead
  now drop from various places in Temples of Atzoatl that contain a Cradle of
  Creation." and "Certain Tier 3 Temple Rooms that have special chest no longer
  always contain a special chest. When they do, they will now always contain
  the Unique item specific to that room." How a unique is valued is under
  [Not validated](#not-validated).
- **What a point of rarity is worth has never been measured.** The quantity
  half was removed in 3.30.0 (POE-369). The default rarity rate is
  `C_PER_RARITY_BASE` (`desktop/src-tauri/src/temple/valuation.rs`), a
  starting base to tune; its value is under [Not validated](#not-validated).
  **Pack size and magic pack size have no rate** and are deliberately not
  priced. Both are flagged wherever they reach a number
  ([ADR-022](adr/022-room-values-are-chaos-denominated-and-market-fed-presets-are-default-and-custom.md)).

## Divine Font

Source: poewiki.net/wiki/Divine_Font plus community data, as of the Mirage
league. The desktop parser (`desktop/src-tauri/src/font_parser.rs`) does not
reconstruct sentences; it keys on the anchor phrase in the third column, so a
wording change in the game shows up there first.

### Crafts per labyrinth

| Lab | Area level | Crafts | Options per craft | With Twice Blessed |
|---|---|---|---|---|
| Normal | 33 | 1 | 2 | 2 crafts |
| Cruel | 55 | 1 | 3 | 2 crafts |
| Merciless | 68 | 1 | 4 | 2 crafts |
| Eternal (Uber) | 75 | 2 | 4 | 3 crafts |
| Potential (Dedication to the Goddess) | 83 | 2 | 4 | 3 crafts |
| Fortune (Tribute to the Goddess) | 83 | 2 | 4 | 3 crafts |
| Opportunity (Gift to the Goddess) | 83 | 8 | 4 | 9 crafts |

Source: the poewiki Labyrinth table (owner screenshot, 2026-10-11) for area
level, crafts and options; Dedication adds the corrupted-gem options, Tribute
enriches lab rewards. An earlier version of this table gave Tribute 8 crafts
with no source. The Twice Blessed column is the +1 rule, not checked per lab.

The panel hides its "Crafts Remaining" line on the last craft; the parser
treats an absent line as "one left" and a garbled one as "unknown", which are
different states (see the type comment in `font_parser.rs`).

### Options

Always present, always first: *"Transform a Skill Gem to be a random
Transfigured Gem of the same colour"*, which offers three random same-colour
transfigured gems to pick from. Parser anchor: `random transfigured gem`.

The random pool:

| Wording (game) | Anchor in the parser | Notes |
|---|---|---|
| Transform a non-Transfigured Skill Gem to a Transfigured version | `nontransfigured` (separators squashed) | The jackpot. Since Mirage the player chooses which transfigured version; before, it was random. |
| Exchange a Support Gem for a random Empower Support, Enlighten Support, or Enhance Support | `empower support` | Eternal and above only. |
| Add +X% quality to a Gem | `quality` with add/gem | Tiers from +2–8% (Normal) to +8–20% (Eternal). |
| Add X experience to a Gem | `experience`, not Facetor | Tiers from 3–5m (Cruel) to 30–150m (Eternal). |
| Sacrifice a Gem to gain X% of its experience as a Facetor's Lens | `facetor` (also `faction`, a recurring OCR misread) | 20/40% Merciless, 30/60% Eternal. |
| Sacrifice a Gem for Treasure Keys | `treasure keys` | Works on corrupted gems. |
| Sacrifice a Gem for Currency Items | `currency items` | |
| Sacrifice a Gem to gain X% of its experience as your own experience | `your own experience` | |

Dedication to the Goddess only:

| Wording (game) | Anchor |
|---|---|
| Transform a Corrupted Transfigured Skill Gem to be a random Corrupted Transfigured Skill Gem of the same colour | `corrupted transfigured` |
| Transform a Corrupted Skill Gem to be a random Corrupted Skill Gem of the same colour | `corrupted skill gem`, not transfigured |

Removed in Mirage: *"Exchange a Support Gem for its Awakened version"*.

Appearance rates quoted in the community (about 6% for the jackpot, about 8%
for the 60% lens) come from small samples. They are not measured here and must
not be presented as such; collecting real rates across users is one reason the
app captures the panel at all.

## Dedication craft rules

Established 2026-07-30, Allflame league. There is no object called "Font of
Divine Skill"; the Dedication crafts are on the Divine Font, and skill gems and
transfigured gems are two disjoint reroll pools (the two wordings above).

- **Vaal gems are a legal input and never an output.** Owner-stated. They
  price into the input cost and stay out of the outcome pool, the tiers, and
  the rankings. `internal/lab/dedication.go` prices the feed over what may be
  fed and the pool over what may come out; the two differ by exactly the Vaal
  gems.
- **Colourless gems are neither.** poe.ninja's skill-gem feed carries gems
  with no attribute requirement (the Allflame Pacts, Portal, Detonate Mines,
  Convocation). "Of the same colour" excludes them; the colour resolver leaves
  `gem_color` empty for them, and that empty colour is what the code keys on.
- **`is_drop_restricted` is not an exclusion key.** Every transfigured gem
  carries it, and transfigured gems are the font's main output.
- **Best of three** on the reroll is confirmed by the owner.
- **Not verified**, and either would change the expected-value formula rather
  than its inputs: whether the corrupted reroll preserves level and quality
  (the 21/20 in, 21/20 out assumption is unsourced; the wiki states
  preservation only for the transfigure craft), and whether the pool is
  uniformly weighted.

## Currency Exchange sidebar

The in-game exchange's "I want" column lists sixteen categories, as served
by `GAME_CATEGORIES` (`scripts/generate-currency-exchange-items.py`): Currency,
Essences, Delve, Threads, Divination Cards, Delirium, Legion, Fragments, Oils, Catalysts, Omens, Tattoos,
Expedition, Harvest, Runegrafts, Allflame, plus Favourites and All. Categories
carry sub-headers inside (Eldritch Currency under Currency).
Before 3.30.0 the Threads slot was Scarabs (screenshot, Allflame league,
2026-08-19); Scarabs were removed in 3.30.0.

3.30 (patch notes, inferred, unverified in game): Scarabs are removed and
Threads take their sidebar slot ("A new Threads section has been added to the
Fragment Stash Tab"); Divine Vessels and Allflame Embers are now Currency
items. The served list carries Threads in the Scarabs position until a 3.30
screenshot confirms the order.

No upstream feed exposes this taxonomy. RePoE's `item_class` does not match it
(oils, catalysts, essences and fossils are all StackableCurrency), and neither
does the metadata path (oils, catalysts, omens, tattoos and runegrafts all sit
under `Metadata/Items/Currency/`). The curated mapping is `CATEGORY_RULES` in
`scripts/generate-currency-exchange-items.py`, emitted into
`internal/exchange/itemdata/items.json`; a rule naming a category outside the
sixteen fails the run ([ICONS.md](ICONS.md), "Currency Exchange
items"). One id prefix, `Currency/Ancestral`, holds both Omens and Tattoos and
is split by name.

## Trade site

The search API's "Query is too complex" is a per-query budget, not a filter
count. The measured cost model (anonymous budget 35; `and` 1 per filter,
`count` 3 + 2 per filter, `mercenary` 21 + 2 per filter, so two `mercenary`
groups never fit) is in
[RESEARCH-poe-trade-api.md](RESEARCH-poe-trade-api.md) and enforced by
`complexity` in `desktop/src-tauri/src/mercenary/search.rs`. Logged in, the
budget is at least 116: on 2026-08-26 saved searches with two, three and four
`mercenary` groups of four filters each all loaded in a logged-in browser.
That is why the mercenary guides can carry searches the app itself could never
send. The API says so itself — measured 2026-09-05 by posting the Path of
Evening cheap Manyshot search (`8r8JqonVIV`, cost 96 under the model)
anonymously: `400 {"error":{"code":2,"message":"Query is too complex. Please
reduce the amount of filters used.\nLogging in will increase this limit."}}`.
This is why the mercenary module's `trade ↗` link (opened in the user's
browser, logged in) and the query the app posts (anonymous) are two different
shapes.

## Mercenaries

- **Mercenary classes and their skill pools.** Thirty-five classes plus the
  identical "Warpriest of the Ruckus". Every mercenary of a class carries the
  class's fixed primary skills — two for most classes, THREE for some
  (Bloodletter: Bloodthirst, Blood Mortar, Lacerate) — and rolls one or two
  secondaries and one or two utilities from the class's two pools. The same
  skill can be a primary in one class and a utility in another (Leap Slam).
  Wiki-sourced 2026-09-06
  ([List of mercenary classes](https://www.poewiki.net/wiki/List_of_mercenary_classes),
  "Always has these skills"), joined to GGG's stat vocabulary by display text,
  and committed as `desktop/src-tauri/src/mercenary/class-pools.json`, every
  id pinned against the vocabulary by test. The current class name is
  `Kineticist`; the guides' `Kinetist` is the app's archetype key, not the
  header's word. The recruit window prints one rank prefix before the class,
  `Infamous` (better links and gear; no source says higher support tiers),
  and no other; the 3.26 Renown ranks were player ranks, not mercenary ones.
  Not in the table and unverified: the tier-roll odds per class or per rank.
  The trade link leaves a recognised class's primaries out of its `and` group
  because a filter every listing satisfies adds nothing, and its anchor-rows
  shape leaves utility rows out because only support-mercenary buyers price
  them (community consensus, same date: a mercenary is priced by a desired
  combat skill and the links on that row; a rolled secondary is NOT
  interchangeable — Vaal Ice Shot against Icicle Rain decides a Manyshot).

## Not validated

Recorded so the status is explicit, not so they can be built on.

- **3.30 Currency Exchange ids.** Hand-added before the 3.30 feed was
  observed: `Metadata/Items/Threads/{Abyss,Bestiary,Betrayal,Blight,Breach,
  Expedition,Incursion,Legion,Ritual,Ultimatum}Thread` and
  `Metadata/Items/Threads/FrayedThread` (Frayed Thread filed under Threads is
  inferred), listed as `PROVISIONAL_IDS` in
  `scripts/generate-currency-exchange-items.py`. The Threaded Delirium Orb keeps
  the Skittering id `Metadata/Items/Currency/CurrencyAfflictionOrbScarabs`;
  whether the rename changed the id is unverified. Regenerate from the 3.30
  feed.

- **3.30 Harvest Flipping weights and costs.** The Delirium Orb and
  Astrolabe weights in `internal/harvest/familydata/families.json` are pre-3.30
  HarvestForge logs with the removed rows dropped (Fragmented Delirium Orb;
  Deceptive and Fruiting Astrolabes) and are provisional: 3.30 makes Delirium
  Orbs reforge rare Maps (patch notes 03 Endgame, line 72), and the Threaded
  Delirium Orb keeps the Skittering weight although Fragmented stock converts
  to it (line 78). Every family's `rerollCost` and `lifeforce` are unverified
  for 3.30: the notes give no reroll cost (06 League Changes, lines 72-80).
  Re-measure both in 3.30.

- **League start and the weekly cycle.** Owner's account: leagues start Friday
  evening CET; the first hours are a dump as veterans clear their first lab;
  the market is usable for farming decisions from roughly hour twelve; and
  Sunday-evening prices run well above Wednesday-morning prices for all gems
  at once. The dated research
  ([research/market-findings-2026-03.md](research/market-findings-2026-03.md))
  found weekend volatility higher than weekday and left the Friday/Saturday
  price-rise narrative explicitly unvalidated. Until a measurement settles it,
  temporal normalisation is an accuracy enhancer, not a prerequisite, and
  nothing may judge system accuracy on hour 0–6 data.

- **3.30 double-corruption destroy chance.** The double-corruption EV
  (`internal/lab/doublecorrupt.go`) destroys the gem with
  `dcDestroyChanceProvisional` = 0.125 per corruption roll (two rolls, so
  15/64 ≈ 23% of attempts end destroyed and count as worth 0). Source: none — a
  guess. The 3.30 notes (06 League Changes) say only that "The Lapidary Lens now
  has a chance to destroy the item"; the community temple sheet's "chance for
  poof" at Doryani's Institute gives no rate. The model also assumes,
  unverified, that Doryani's Institute's gem device is the double-corruption
  device those notes describe and that it still applies two independent rolls
  to an uncorrupted gem; the notes say the devices now "Corrupt an Equipment
  Item or Gem a second time", which may mean one roll on an already-corrupted
  gem. Results built on this guess carry the model marker
  `estimated-provisional-destroy`, and the desktop Comparator says the EV
  depends on an unverified rate. Measure the destroy rate (and the roll count)
  in 3.30.

- **3.30 temple values not yet confirmed in game.** Each is carried by the
  code as a guess or starting base:
  - The exact 3.30 patch-note line that removes item quantity is not quoted;
    the sources are the POE-369 sweep and sheet g744679169.csv row 8.
  - Unique value = non-upgraded price × 25 % (g744679169.csv row 13;
    `SHEET_UNIQUES_330`; Operator ruling 2026-11-13 00:20, a guess).
  - Equal-weight pool over the room's base uniques (the sheet author's guess,
    same ruling). Glittering Halls counts 0.25 × the mean of its four.
  - Rarity default 1 c per 1 % (`C_PER_RARITY_BASE`), a starting base to tune
    (Operator ruling 2026-11-13 00:08 on Vertolka's note "Also probably change
    base added value per IIR to 1%=1c? for start"). It supersedes the sheet's
    0.5 c (g744679169.csv row 12) and Vertolka's 2026-09-06 proposal "1% quant
    = 0,5c, 1% rarity = 0,25c".
  - Cradle bracket: the upgraded unique from the same chest, only with a Cradle
    of Creation, shown as 25 % of the upgraded price (g744679169.csv row 13;
    the cell prints the contribution).
  - Operator 2026-11-13 00:14, verbatim: "4-6% per tier it should be 4% from
    rarity and 1-2% from redirected from quantity and yeah pack size 1% per
    tier". The extra 1-2 %/tier redirected from quantity is unmeasured and has
    no value term; the sheet's 6 % upper bound is not adopted.
  - Sheet "?" rows not adopted: Factory rarity "4%-24%?" (gid0.csv rows 11-12);
    Sadist Den "Atziri is possesed?" / "from T2 architect and Atziri? are
    possesed" (gid0.csv row 24); Atlas device uses "1/3/5?" (gid0.csv row 23).
  - The chest-absence chance ("no longer always contain a special chest") is
    read as covered by the flat 0.25 (drops.rs, inferred).
