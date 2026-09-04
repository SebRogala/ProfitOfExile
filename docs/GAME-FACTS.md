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
send.

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
