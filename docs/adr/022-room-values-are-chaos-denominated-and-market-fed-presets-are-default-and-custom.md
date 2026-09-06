---
uid: 7489a81c-d659-4141-94d4-cc33c080dd95
---

# ADR-022: Room Values Are Chaos-Denominated and Market-Fed; Presets Are Default and Custom

**The rule in full**: every temple room-tier is worth a number of chaos built
from one market read — sale plus drops plus bonus — the letter grade prices only
what nothing else priced, the same object is shown and ranked, and everything a
player might disagree with is a settings field split across exactly two presets.

## Status

Accepted (POE-257, commits `cd5627c` (WI-1) + `f722d49` (WI-2), 2026-09-06),
under epic POE-124. Supersedes nothing.

Two things it decided were **not yet reachable by a user** when it was accepted.
One of them has since landed: POE-258 wired the poll, so `run.rs` now prices a
board against a real market read (`ssot::temple_market_now`) and takes the
cold-ladder branch only when there is nothing to price with — see the amended
Consequences bullet. The other stands: POE-257 shipped no preset setters, so the
preset is reachable only by editing `settings.json` (POE-259 adds
`temple_set_preset` / `temple_set_custom`).

Related: [ADR-013](013-ui-picks-persist-in-a-schema-less-prefs-map.md) — the two
new settings blocks are TYPED, against that ADR's default, because Rust reads
them. [ADR-017](017-no-default-engine-floor-may-hide-a-live-market.md) and
[ADR-018](018-flags-mark-they-never-order.md) — thin-market and window-priced
provenance rides on each term as a flag and never gates a room out of the
ranking.

## Context

The temple builder advisor (POE-167…POE-171) ranked boards on
`StrategyProfile::room_values`: Sebastian's 1–10 outcome ranking, four lines
priced, twenty-one at nothing. `StrategyProfile::locus_doryani_rush`
(`temple/strategy.rs`) is that table — Corruption `[0, 0, 9]`, Gem `[0, 0, 7]`,
the pair as a `Combination` worth 10, `apex_score` 2.0 — and it works only for
the strategy it is named after. On any other board twenty-one lines and every
tier-1 and tier-2 room score zero, so the kill is a free choice and the door
chain decides it.

Three inputs arrived at once and changed what was possible:

- **POE-254 / POE-255** — the collector pulls the poe.ninja temple, vial and
  unique feeds, and the server serves one aggregated, league-keyed object at
  `GET /api/analysis/temple-market`: 86 room-tier lines with a sale delta each,
  the 31 recipe members' prices, and the recipe table
  (`internal/temple/market.go`, `internal/temple/recipes.go`).
- **POE-256** — `temple/drops.rs`, the per-line, per-tier drop table: the
  tier-3 chest unique, the vial the architect rolls for, the architect's
  signature rare, and poedb's quantity / rarity / pack-size percentages. Every
  number carries an `Estimate` with a `Basis` of `Measured` or `Guess`.
- **Vertolka's letter grade** (`rooms::Grade`, D…A++), already imported and
  read only for display.

Epic POE-124 fixed six locks (L1–L6) before the work started; four of them bind
this ADR. **L1**: every room value in this crate is chaos, and the letter is the
fallback for a room nothing has priced — never a second currency running
alongside the first. **L2**: value is per `(line, tier)`, with tiers 1 and 2 at
a fraction of tier 3, editable, and it is a BUILDING-time value. **L3**: two
presets, Default and Custom, no Basic, and the rusher's table survives as a test
fixture. **L4**: a missing or stale price falls back to the preset's base value,
never to zero — and the value shown and the value ranked come from the same
read. **L5** is the lifecycle lock this work had to stay inside rather than
decide, and §2 records how: no HTTP on the 650 ms tick.

## Decision

### 1. Chaos everywhere: sale + drops + bonus

`temple/valuation.rs` is the one place a room-tier's worth is computed. At tier
3, for one line (`valuation::value_tier3`):

```text
sale   = the feed's price above the room-line floor        (market::MarketInput::sale_delta)
drops  = drops_weight × Σ( expected count × unit price )   (drops.rs × market.rs)
bonus  = quantity% × c_per_quantity + rarity% × c_per_rarity
total  = sale + drops + bonus
```

Tiers 1 and 2 are `Knobs::tier_fraction` — **0.8** by default — of their line's
tier-3 total (`valuation::scale_from_tier3`). Deliberately not their own
area bonus: what a tier-1 room is worth at BUILDING time is the line being
present at all, because the 50 % double-tier node and the Timelines scarab act
on the line, not on the tier.

The five knobs are `valuation::Knobs`, and `preset.rs` re-exports rather than
redefines them: `tier_fraction` 0.8, `c_per_quantity` 0.5, `c_per_rarity` 0.25,
`drops_weight` 1.0, `combo_premium` 0.0.

**What is a guess, and whose.** Three sources, and they are not
interchangeable.

- **Vertolka's**, all of it quoted on POE-124 from his 2026-09-06 message and
  flagged by him as proposals: the two c-per-percent rates (*"Maybe there we can
  setup something like 1% quant = 0,5c, 1% rarity = 0,25c"*), the two drop
  counts — 0.25 uniques and 0.1 vials per run at tier 3, stated as *"price of
  unique divided by 4 + price of vial divided by 10"* — and the one manual price
  in the table, *"Crucible of Flames is giving on average 2 temple gloves per
  run and their base price is usually around 30c"* (poe.ninja publishes no line
  for a mod-rolled rare). The counts and the glove price are `Basis::Guess` in
  `drops.rs`; the two rates are `Knobs` defaults.
- **poedb's**, measured and dated 2026-09-06: the quantity / rarity / pack-size
  percentages, and the raw chance integers. So a bonus term is half measured and
  half guessed — the *percentage* is poedb's number, the *rate* that turns it
  into chaos is Vertolka's — which is why every bonus driver reads
  `guessed: true` (`valuation::push_bonus`).
- **The orchestrator's**: the ten grade-ladder rungs. They are neither
  Vertolka's nor poedb's — he graded the lines with letters and never priced a
  letter. The ten numbers are a geometric ladder calibrated on 2026-09-06,
  during POE-257's own autonomous run, against the committed capture: 800 and
  400 were chosen so Locus (A++, delta 846) and Doryani (A+, delta 390) keep the
  feed's order at roughly the feed's ratio, and the rest halves down from there.
  Nothing below A+ is calibrated against anything — no feed prices a C+ room
  above the floor — so those rungs are a ranking, not a valuation.

`RoomValue::guessed` is true when any driver that actually CONTRIBUTED chaos
rests on one of these estimates, and the live grade rung is one of them (§3).

Three terms are deliberately absent from the sum rather than priced at zero:
pack size (no rate exists, and there is no driver because a driver has to be
listed against a rate), poedb's raw `vial_chance_raw` / `mod_item_chance_raw`
integers (no printed scale — see [GAME-FACTS.md](../GAME-FACTS.md)), and any
term with no count or no price, which is still listed as a `Driver` with
`chaos: None`.

### 2. ONE read, shown and ranked

`slice::value_read(settings, market)` builds the whole 25 × 3 table
(`valuation::Valued`) for one `MarketInput` and one preset. `run.rs` calls it
**once per read** (`run.rs`, `full_read`) and hands the same object to
`slice::advise_read` — which bridges it into the profile the advisor ranks on —
and to `slice::project`, which publishes it as `OfferView.value:
Option<RoomValueView>`. There is no second computation on the view path, so the
number in the offer box is the number the recommendation used, for all 25 lines
including the two instrumental ones.

`RoomValueView` carries `total`, `priced`, `guessed`, `league`, `asOf`,
`scaledFromTier3` and the full `drivers` list. **A tier-1 or tier-2 row's
drivers must not be summed**: they are the tier-3 row's terms copied UNSCALED
plus one `tier_fraction` driver whose `chaos` IS the total, so summing a Locus
tier-1 row would print 846 + 676.80. `scaledFromTier3` is the branch a consumer
takes; it is `null` on a tier-3 row.

`MarketInput` (`temple/market.rs`) is a name-faithful SUBSET of the served
payload — the wire structs declare only the fields the valuation reads and serde
ignores the rest (`name`, `icon`, `divine`, `floorRule`, `categoriesSeen`,
`windowHours`, `windowSamples`), because declaring a field nothing reads is a
dead-code warning that says nothing — and it does no HTTP — POE-258 owns the poll, because
[TEMPLE-LIFECYCLE.md](../TEMPLE-LIFECYCLE.md) forbids network work on the 650 ms
tick. Staleness is enforced in one place: while `stale` is set every reading
accessor answers as if the market were absent, so no caller can forget the rule.

### 3. The fallback rule: a cold board is read in ONE unit

The branch turns on `MarketInput::prices_anything()` alone — live means not
stale, a usable (non-zero, finite) floor, and at least one room or item quote.

- **Not live** (no poll yet, stale, or no usable floor): EVERY room is valued at
  its cold rung, `Grade::fallback_chaos` — A++ 800, A+ 400, A 200, B+ 100, B 50,
  B− 30, C+ 20, C 10, C− 5, D 2 chaos. Those ten rungs ARE the preset's
  base-value table that L4 names.
- **Live**: the formula is summed. The ladder stands in only for a room where
  **nothing at all was summed** — no sale, no priced drop, no bonus — and there
  it is `Grade::fallback_chaos_scaled(top)`, every rung multiplied by
  `top_sale_delta / 800`, then **capped at the lowest tier-3 total among the
  rooms whose sale is above the floor**.

**`guessed` is asymmetric across those two branches, deliberately**
(`valuation.rs`, module header): a COLD rung reads `guessed: false`, because it
is the preset's stated base value for that room and stands exactly where a
Custom override stands; a LIVE rung reads `guessed: true`, because it is an
inference about THIS market — the orchestrator's number re-anchored on today's
top delta and then capped. So the live ladder is one of the estimates
`RoomValue::guessed` enumerates, alongside Vertolka's rates and counts.
`Priced::Fallback` is what tells the player no price was involved either way.

Mixing the two — summing the terms that survive a cold market (a poedb bonus,
the manually priced glove) and reading the rest off the letters — is what the
first cut did, and it produced a board where a grade-A room sat at 6 c under a
grade-C room at 10 c. Half a formula and half a ladder is not a ranking in
either unit.

**Why the cap.** On the committed capture the only two above-floor rooms are
Locus of Corruption tier 3 (856 c, delta 846) and Doryani's Institute tier 3
(400 c, delta 390) against a floor of 10. Anchored on 846 the A+ rung scales to
423, which still outranks the 390 the feed actually printed. The cap pulls it
to 390. Rescaling alone does not keep a letter under the feed; the cap does, and
only against the LOWEST sale-priced room.

**The residual, recorded and not claimed away.** A letter can still outrank a
room whose own MEASURED value is small: Apex of Ascension is B−, sums nothing
and stands in at 31.725, while Chamber of Iron summed a real 6.00 c area bonus
(6 % quantity x 0.5 + 12 % rarity x 0.25) and stays at 6.00. And the cap is a function of which rooms the feed
prices: on a read where Locus is the only sale-priced room the cap is 846, so an
A+ line that summed nothing reads its full 423. **Owner's open fork**: cap at
the lowest MEASURED total instead, which on this capture would pull every letter
under 6 c and make the ladder nearly inert for the ~15 rooms nobody prices at
all. Neither rule is measured; the shipped one keeps the ladder useful.

The whole ladder is **PROVISIONAL, calibrated 2026-09-06** on that capture, and
its ten absolute numbers are exactly the shape `AGENTS.md` warns about — which
is why they are now the COLD branch only and the live branch rescales.

### 4. Two lines are priced at their letter even when the market is live

`strategy::INSTRUMENTAL_LINES` — the upgrade line (Temple Nexus, B+) and the
explosives line (Shrine of Unmaking, D) — take the same rung by a different
route and say so with `Priced::Instrumental` rather than `Priced::Fallback`: no
price is missing, summing is simply the wrong question. Their worth is the tiers
they lift and the rooms they clear, which `advisor::rollout`'s `UPGRADE_TARGETS`
and `CHARGES` already model. Cold they read 100 and 2; on the capture 105.75 and
2.115.

**This is a deliberate departure from L1**, which says the letter grade is
fallback ONLY — a room that summed something keeps what it summed. Temple Nexus
sums something: a 6 c area bonus (6 % quantity, 12 % rarity at tier 3), and
under L1 it would be worth that 6 c, which prices the room that lifts an 846 c
line below a junk C-grade room. The departure is taken on the board-7 evidence
below and nothing else. Shrine of Unmaking is not a departure at all in
arithmetic — the explosive line is `None` at every tier of `drops.rs`, so it
sums nothing and the ordinary fallback would hand it the same D rung; what this
rule changes for the shrine is the LABEL, `Priced::Instrumental` rather than
`Priced::Fallback`, because no price is missing. `guessed` follows §3's rule
unchanged — false on a cold rung, true on a live one — so on a live read both
these lines are flagged as resting on the orchestrator's ladder.

**The evidence** is retrospective board 7 (Armourer's Workshop, PC log). While
those two lines scored zero, the app recommended *upgrade to Armoury* — Chamber
of Iron, 6 c of area bonus — over Sebastian's *change to Sanctum of Unity*,
on the cold board and on the capture alike: the change onto the Nexus won on EV
but sat inside one noise band, where R4 preferred the upgrade. At its B+ rung
Sebastian's play returns on both paths and nothing moves on the other seven
boards (measured, `advisor::tests`).

**The accepted trade-off** is a double payment: the rollout already credits what
those lines DO, and the bridge now also copies their rung into `room_values`.
The visible cost is that Temple Nexus is THIRD on the capture, above two rooms
the feed prices. The better model — valuing a `change` onto an instrumental line
by its own downstream effect — needs the board's upgrade-reachable set at
scoring time, which `room_values` has no shape for. A Custom override replaces
the rung like any other room's value, and the bridge no longer zeroes these two
lines, so the box and the ranking use the same number for them.

### 5. Presets are Default and Custom, and the table is persisted separately

`preset::Preset` is `Default | Custom` and there is no Basic (L3). Default is
`drops.rs` plus the market read with `Knobs::default()`; Custom is the same
formula with the player's own rates plus a per-room-tier override table.
`preset::value_table` is the single entry point.

The preset CHOICE and the Custom TABLE are **two settings fields**:
`Settings::temple_preset: Salvaged<Preset>` and `Settings::temple_custom:
Salvaged<TempleCustomSettings>` (`settings.rs`), both typed (ADR-013's stated
exception: Rust reads them). `temple_custom` is camelCase inside, like the other
temple blocks; `temple_preset` has no inside — it is a bare `snake_case` enum
string, `"default"` or `"custom"`, which is this app's convention for enum
VARIANTS and a different rule from the one its sibling's FIELDS follow.
Separate because the whole point of the pair is that Default → Custom → Default
→ Custom returns the player's numbers unchanged; folding them into one "active
preset" blob would delete the table the moment the player looked at Default.
`Salvaged<T>` covers a present-but-wrong field: the block is replaced by its
default and REMEMBERS that it was, so `apply_to_state` can put a line in
`rejected` where the user sees it.

**A malformed entry costs its own room and nothing else.**
`TempleCustomSettings::overrides` refuses three things per entry and none of
them fatally: a key that is not one of the 25 room lines, an entry that does not
state exactly three values, and a value that is not a chaos amount (NaN,
infinite, negative) — which costs that SLOT while the entry's other tiers stand.
`rooms` is a `Vec<Option<f64>>` rather than a `[_; 3]` precisely so serde cannot
fail the whole field on one four-element entry. This replaces
`apply_to_state`'s whole-profile reject for this block only; the four
`temple_profile` scalars keep the all-or-nothing rule, because a negative
`path_cost` is a sign error and a scorer running on half a profile is worse than
one running on the default.

An override REPLACES the formula for the room-tier it names and carries a single
`CustomOverride` driver — the drivers a consumer prints must sum to the total it
ranks on, and a stated number has no terms behind it. A tier-3 override also
moves that line's tiers 1 and 2, because those are a fraction of whatever tier 3
is worth. Zero is accepted throughout: "this room is worth nothing to me" is a
position a player can hold, and `drops_weight = 0` is the rusher's — it reduces
the ranking to sale value alone.

### 6. The bridge, and what "relative" means

`TempleProfileSettings::to_profile(&Valued)` (`temple/slice.rs`) fills
`StrategyProfile::room_values` for all 25 lines at all three tiers in chaos,
keyed through `RoomLine::mechanical_line()` so the four mechanical keys resolve
to their own variants and the other 21 to `Line::Other(key)`.

`apex_score` and `path_cost` stay **RELATIVE settings**, stated in units where
the **top tier-3 room is worth 9** (`strategy::REFERENCE_TOP_ROOM_VALUE`), and
POE-259's controls must be labelled that way — a slider reading "2" beside a
board of three-figure chaos numbers is a slider nobody can set. The bridge
multiplies by `StrategyProfile::value_scale()` = top tier-3 value / 9. That
multiplier is applied to exactly five magnitudes, and the table in
`REFERENCE_TOP_ROOM_VALUE`'s doc is normative for the list: `apex_score`,
`apex_mixed_increment`, `room_baseline`, `path_cost`, and
`advisor::rules::NOISE_FLOOR` (scaled in `rules::noise_margin`).
`blast_discount` is not on it — it is a fraction of a score difference and is
already scale-free — and neither is `rollout::Valuation::lost_threshold`, which
is derived from `room_values` and moves with them by construction. A profile
whose rooms are all worth nothing answers `1.0` rather than `0`, because zeroing
the noise floor would make the priority chain decide every board on sampling
noise.

`combinations` is **empty** at `combo_premium = 0`, which is the default. The
orchestrator settled that Locus + Doryani is worth the sum of its two
above-floor deltas and no more, and the per-room sum already IS that sum. A
`Combination` REPLACES the per-room sum rather than adding to it, so emitting
one worth exactly the sum would be a no-op on a two-room board and a silent
deletion of every other line's value on a real one. Only a positive, finite
premium builds one; negative and non-finite are refused. **Owner call left
open**: whether an additive form of `Combination` should exist at all, so a
premium can be stated without taking the replacement rule with it.

`mode_rule` is untouched and stays `LinesConnected([Corruption, Gem])` whatever
the player prices those lines at. It decides when the build flips from chasing
to scarab-farming, and RV — the hard constraint that refuses to bank an
unreachable target — reads the same two lines. A Custom table that prices both
at zero stops them SCORING but does not stop the advisor protecting them, so
`overrides` warns about exactly that pair.

### 7. The reversal, and what is unchanged

`TempleProfileSettings`'s doc said of `room_values`, `combinations` and
`mode_rule`: *"Those arrive as a second shipped profile, not as settings JSON."*
**That is reversed for the first two.** `room_values` now comes from a market
read plus a settings-JSON override table, and `combinations` from a
settings-JSON premium. `mode_rule` is the one third of that sentence that still
holds.

What the reversal does NOT change is the decision the module header records
(`temple/mod.rs`, Sebastian, 2026-08-18): **one base strategy, per-user
configurables — a field of a profile, never a code branch.** POE-257 widened the
field set; it added no branch. `locus_doryani_rush()` is still the structural
base `to_profile` builds on, and its 10 / 9 / 7 / 2 table survives as the
fixture the pinned suites rank with: `value_scale()` reads exactly 1.0 on it, so
every constant expressed as a multiple of the reference is bit-identical to the
number it replaced there.

## Alternatives rejected

- **Ten absolute grade rungs, full stop** (the WI-1 shipped state). Reproduces
  an inversion L1 forbids: at A+ = 400 an A+ line that summed nothing outranks
  the 390 the feed printed for Doryani. That is structural, not a property of
  the capture — it happens not to fire there only because both A+ lines price
  something.
- **Rescaling without the cap.** A++ equals the top delta by construction, so A+
  scales to 423 and the inversion survives at a smaller margin. The guarantee
  has to be imposed a layer up.
- **Capping at the lowest MEASURED total** rather than the lowest sale-priced
  one. Not rejected on evidence — it is the recorded owner fork above. On this
  capture it caps everything at 6 c and makes the ladder inert for the rooms it
  exists for.
- **Valuing the two instrumental lines at what they drop.** That is the 6 c
  area bonus, and it is the number that produced the board-7 miss. The
  explosive line drops nothing at all, so for the Shrine of Unmaking this
  alternative and the shipped rule agree on the number and differ only on the
  label.
- **Zeroing the instrumental lines** (what `locus_doryani_rush` does, to avoid
  the double-pay). Reproduces the board-7 miss on the cold board and on the
  capture alike.
- **A `Combination` worth exactly the sum of its members.** A no-op on the
  two-room board and a silent deletion of every other line's value on a real
  one, because a combination replaces the sum.
- **One "active preset" settings blob.** Deletes the Custom table the moment the
  player switches to Default, which is precisely what L3 asks the pair to
  survive.
- **All-or-nothing rejection of the Custom table**, the rule `temple_profile`
  keeps. Wrong for a table of 25 independent numbers where the other 24 are
  still exactly what the player meant.
- **A `c_per_pack_size` knob.** Nobody has stated a rate. Inventing one would
  put a fabricated number in a total; pricing it at zero would claim pack size
  is worthless.
- **Multiplying poedb's raw chance integers by a price.** They run 7 to 2815
  with no scale printed anywhere.

## Consequences

- **Production runs on live prices since POE-258.** `run.rs` reads the market
  from `ssot::temple_market_now`, which `ssot::spawn_temple_market_poll` fills
  every five minutes and which judges staleness against the tick's own clock.
  The cold branch is still the whole fallback and is still what L4 asks for —
  before the first poll answers, on a server that has never priced anything, on
  a payload keyed to another league, and on a read older than two hours, every
  room is valued at its cold grade rung. A poll that fails and a server that
  answers cold both KEEP the last good read instead of blanking it, so a
  restarted server does not take a board off prices that are still minutes old;
  the read ages into stale on its own. What changed is that the player is now told
  which of the two is on screen (`view.ts::marketNote`), because a rung and a
  price print the same kind of number.
- **No preset is reachable from the UI.** POE-257 shipped no setters; the preset
  and the Custom table are settings-file-only until POE-259.
- **`room_baseline` is 4.44 c on a cold read** — 0.05 × (800 / 9) — which is
  more than a D-grade tier-3 room is worth (2 c). On a board of nothing but
  junk, "open one more room" outweighs "the room you opened". That is an
  accepted side effect of stating the constant as a fraction of the TOP rather
  than of the floor; the owner can lower it with no code change. **Open owner
  item.**
- **A sixth scaled magnitude added without a row in `REFERENCE_TOP_ROOM_VALUE`'s
  table is a constant that will quietly stop meaning what it meant.** That table
  is the checklist.
- **The Mirage vial-spike case is a drop-table question, not a formula one.** At
  Vertolka's 0.1 vials per run an 808 c Vial of Summoning adds 74.8 c to Sanctum
  of Immortality tier 3 (60.75 → 135.55 on the capture), which cannot rival
  Doryani's 390. Reaching the epic's "a high-chance vial room rivals Doryani"
  case needs about 0.41 vials per run, or a scale for the poedb chance integers
  to derive one from. **Open owner item**, for Vertolka; the shipped test
  asserts Sanctum crossing Crucible of Flame (67.35) instead.
- **The grade ladder is provisional and dated.** It is calibrated on one day's
  capture, and it is the only place a third-party LETTER (Vertolka's grade)
  enters arithmetic — through a rung this project chose, not one he stated.
- **POE-260 must branch on `scaledFromTier3` before rendering `drivers`.** The
  copied tier-3 terms are unscaled by design; the `tier_fraction` driver is the
  headline.
- **Every provenance flag is a flag.** `guessed`, `lowConfidence` and
  `windowPriced` ride on the driver that carries them and never remove a room
  from the ranking or reorder one (ADR-017, ADR-018).
- **The two settings blocks are typed, against ADR-013's default.** Rust reads
  them, and `prefs` is a webview-owned map.
