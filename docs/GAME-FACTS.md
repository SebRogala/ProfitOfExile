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

### Vial recipes: nine vials, eleven transformations

A vial is consumed at the Altar of Sacrifice together with a **base** unique and
transforms it into an **upgraded** unique. **The direction is the part that is
easy to get backwards**: the vial's own in-game currency text names the item you
SACRIFICE, not the item you receive — "Sacrifice this item on the Altar of
Sacrifice along with Coward's Chains to transform it" means Coward's Chains →
Coward's Legacy, not the reverse.

Verified 2026-09-06 against poedb.tw's per-vial pages, with four independent
signals agreeing on every row: the game's own currency text, the vial's icon
file name (`VialCowardsChains`, `VialSlumber`, …), poedb's prose blurb, and its
Recipe block ("Offer: `<UPGRADED>`; Your Offer: 1x `<BASE>`, 1x `<VIAL>`"). The
game text settles a disagreement, because it is the item's own description
rather than a site's rendering of it. Normative home:
`internal/temple/recipes.go`.

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

Nine vials, eleven recipes: Vial of Transcendence upgrades any of Tempered
Flesh / Mind / Spirit, so it appears three times.

- **The vial a room rolls for is not always the vial that upgrades that room's
  own unique.** It is on the six chest lines; it is not on Locus of Corruption
  (drops Shadowstitch, rolls Vial of Sacrifice), Glittering Halls (rolls Vial of
  Transcendence, drops no unique) or Throne of Atziri (rolls Vial of the Ghost,
  drops no unique). Source: poedb room pages plus Vertolka's sheet, both read
  2026-09-06, which agree independently on all nine room → vial pairs
  (`desktop/src-tauri/src/temple/drops.rs`).
- **poe.ninja prices neither Shadowstitch nor any temple-mod item.**
  Shadowstitch is the one unique in the drop table with no line in the live
  item-overview feed (checked for league Allflame, 2026-09-06), so the server
  never serves a price for it and Locus of Corruption's unique term contributes
  nothing. The architects' signature drops are mod-rolled RARES, which poe.ninja
  does not publish at all; the one number the code has for them is Vertolka's
  stated guess for Crucible of Flame, verbatim: *"Crucible of Flames is giving
  on average 2 temple gloves per run and their base price is usually around
  30c"* (2026-09-06) — **a guess**, carried as `TempleMod::base_price_chaos`
  and the only manual price in the table.

### Temple room bonuses per tier (poedb, 2026-09-06)

Read off each room's own poedb.tw page for all 75 tiered rooms and carried in
`desktop/src-tauri/src/temple/drops.rs`. Percentages are `increased Quantity of
Items` / `increased Rarity of Items` / `increased Pack Size` in this area, tier
1 / tier 2 / tier 3. Fourteen of the 25 lines print no such line at all and are
`None` rather than zero.

| Line (room key) | Quantity % | Rarity % | Pack size % |
|---|---|---|---|
| the standard eight (see below) | 2 / 4 / 6 | 4 / 8 / 12 | 1 / 2 / 3 |
| Factory | 22 / 44 / 66 | 4 / 8 / 12 | 1 / 2 / 3 |
| Glittering Halls | — | 20 / 40 / 60 | — |
| Hall of War | — | — | 10 / 20 / 30 |

The standard eight are `chamber_of_iron`, `conduit_of_lightning`,
`crucible_of_flame`, `defense_research_lab`, `hall_of_champions`,
`hybridisation_chamber`, `sanctum_of_immortality` and `upgrade` (Temple Nexus).
Factory differs on quantity alone; its rarity and pack-size lines are the
standard ones.

Two places where Vertolka's sheet claims a bonus and poedb prints no percentage:
Toxic Grove ("increase quantity/rarity") and Storm of Corruption ("high rarity
buff"). Both are recorded as absent rather than as a figure nobody wrote down.

- **The "chance" integers have no published scale.** poedb prints a per-tier
  stat on all nine vial lines — `map incursion boss chance to drop <tag> vial %
  [N]` — and a second on four of them — `map incursion boss chance to drop
  <tag> item % [33/66/100]`. The vial values run **7 to 2815** (Locus of
  Corruption and Throne of Atziri share 7 / 13 / 20 at the bottom, Glittering
  Halls has 929 / 1886 / 2815 at the top) and **nothing on the page states what unit they are in**. They are
  carried verbatim as `u32` (`vial_chance_raw`, `mod_item_chance_raw`) and are
  never multiplied by a price. Anyone who finds the scale should record it here
  first.

  **The scale is still unknown; a CONVENTION was fixed on 2026-09-06 (POE-262)
  for the vial stat only.** Vertolka's "0.1 vials per run" was stated for six
  lines, three of which print **1689** at tier 3, so the app reads *raw 1689 =
  the configured rate per run* and scales every other line and tier by its own
  integer over 1689. At the shipped 0.1 that gives Glittering Halls 0.1667 per
  run, Hybridisation Chamber 0.0595, Defense Research Lab 0.0476, Toxic Grove
  0.0119, and Locus of Corruption and Throne of Atziri 0.00118 — which is his
  "small chance" as a number. **This is a ratio, not a unit**: it says one
  line's rate relative to another's, and it says nothing about what raw 1689
  means in the game. `mod_item_chance_raw` has no such anchor (nobody has stated
  a per-run rate for the architect's rare) and stays raw and unmultiplied.
- **Uniques drop at tier 3 only.** Vertolka-stated 2026-09-06: "only tier 3
  rooms can drop unique, but T1 and T2 adding chance to drop vial and provide
  smaller quant/rarity bonuses". His two drop rates — 0.25 uniques per run and
  0.1 vials per run at tier 3 — are **guesses**, stated as "price of unique
  divided by 4 + price of vial divided by 10". The unique rate is flat: no page
  prints a per-tier chest-unique chance to scale it by. The vial rate is the
  ANCHOR of the convention above and is a user setting; he proposed doubling it
  to 0.2 on 2026-09-06.
- **What a point of quantity or rarity is worth has never been measured.** The
  only figures anyone has proposed are Vertolka's, in the same 2026-09-06
  message: *"Maybe there we can setup something like 1% quant = 0,5c, 1% rarity
  = 0,25c"* — a proposal he flagged as such, and the default of the two knobs
  the code prices area bonuses with. **Pack size has no proposed rate at all**
  and is deliberately not priced. All three are flagged wherever they reach a
  number ([ADR-022](adr/022-room-values-are-chaos-denominated-and-market-fed-presets-are-default-and-custom.md)).
  Vertolka 2026-09-06 proposed 0.4 c per rarity % as a possible Default; today
  it is a Custom edit; owner call open.

## Divine Font

Source: poewiki.net/wiki/Divine_Font plus community data, as of the Mirage
league. The desktop parser (`desktop/src-tauri/src/font_parser.rs`) does not
reconstruct sentences; it keys on the anchor phrase in the third column, so a
wording change in the game shows up there first.

### Crafts per labyrinth

| Lab | Crafts | Options per craft | With Twice Blessed |
|---|---|---|---|
| Normal | 1 | 2 | 2 crafts |
| Cruel | 1 | 3 | 2 crafts |
| Merciless | 1 | 4 | 2 crafts |
| Eternal (Uber) | 2 | 4 | 3 crafts |
| Gift / Tribute to the Goddess | 8 | 4 | 9 crafts |
| Dedication to the Goddess | 2 | 4 | 3 crafts |

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

The in-game exchange's "I want" column lists sixteen categories (screenshot,
Allflame league, 2026-08-19): Currency, Essences, Delve, Scarabs, Divination
Cards, Delirium, Legion, Fragments, Oils, Catalysts, Omens, Tattoos,
Expedition, Harvest, Runegrafts, Allflame, plus Favourites and All. Categories
carry sub-headers inside (Eldritch Currency under Currency).

No upstream feed exposes this taxonomy. RePoE's `item_class` does not match it
(oils, catalysts, essences and fossils are all StackableCurrency), and neither
does the metadata path (oils, catalysts, omens, tattoos and runegrafts all sit
under `Metadata/Items/Currency/`). The curated mapping is `CATEGORY_RULES` in
`scripts/generate-currency-exchange-items.py`, emitted into
`internal/exchange/itemdata/items.json`; a rule naming a category outside the
sixteen fails the run ([GEM-ICONS.md](GEM-ICONS.md), "Currency Exchange
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
