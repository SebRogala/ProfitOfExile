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
Both have since landed.

**Amended 2026-09-06 (POE-258):** the market poll is wired, so `run.rs` prices a
board against a real market read (`ssot::temple_market_now`) and takes the
cold-ladder branch only when there is nothing to price with — see the amended
Consequences bullet.

**Amended 2026-09-06 (POE-262 WI-1):** §3's recorded owner fork is CLOSED, in
Vertolka's favour. On a live read a room that summed nothing is anchored on the
LOWEST tier-3 total among the rooms that summed something — 2.65 on the
committed capture when WI-1 landed, 1.857 after WI-2 moved the anchor room's
own vial term — where it used to be the top sale delta capped at the lowest
sale-priced room — so a letter can no longer outrank a measurement. That
anchor is over the formula sums of the 23 non-instrumental lines with Custom
overrides ignored. §4's two instrumental lines keep the old anchor-and-cap
unchanged, on their own board-7 evidence. See the rewritten §3 and the new
Alternatives entries.

**Amended 2026-09-06 (POE-262 WI-2):** the vial rate is no longer a flat 0.1
carried on six of the nine vial lines. It is DERIVED for all nine, at every
tier, as `vials_per_run × vial_chance_raw / 1689` — poedb's own per-line ratio
against the tier-3 integer the three standard vial rooms share — and
`vials_per_run` is the sixth knob. §1's knob list, its "What is a guess, and
whose" entry and its "deliberately absent" list are amended below. Glittering
Halls, which named a vial but no unique and so carried no rate at all, goes
from 15 c to 86.33 c on the committed capture and rises above Factory and
Defense Research Lab, which is what Vertolka's 2026-09-06 review asked for.

**Amended 2026-09-06 (POE-259):** the presets are reachable from the UI. The
Temple page carries a Default / Custom picker and an editor over the 25 lines by
3 tiers, served by three commands — `temple_set_preset` (the choice alone, so
switching away and back never touches the table), `temple_set_custom` (validated
by the same per-entry rules the loader salvages by) and `temple_value_table` (the
75 rows on demand, keyed on preset, market age and the Custom table, so the 3 s
SSOT snapshot carries no room values). Editing `settings.json` by hand is no
longer the only way in.

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

The **six** knobs are `valuation::Knobs`, and `preset.rs` re-exports rather
than redefines them: `tier_fraction` 0.8, `c_per_quantity` 0.5, `c_per_rarity`
0.25, `vials_per_run` 0.1 (**added 2026-09-06, POE-262 WI-2**), `drops_weight`
1.0, `combo_premium` 0.0.

**The vial rate is an anchor, not a per-line number** (POE-262 WI-2,
2026-09-06). `drops::VIAL_RATE_ANCHOR_RAW` is 1689 — the tier-3 `chance to drop
<tag> vial` integer poedb prints on Conduit of Lightning, Crucible of Flame and
Sanctum of Immortality, three of the six lines Vertolka's *"price of vial
divided by 10"* was stated for, and the value most of them share (the other
three print 804, 1005 and 201). The convention this fixes is exactly **"raw
1689 means `vials_per_run` per run"**; the SCALE of the integer is still
unknown and this does not settle it. `TierDrops::vials_per_run(anchor)` derives
every line and tier from it, so at the default anchor the nine vial lines read,
at tier 3:

| line | raw | vials/run |
|---|---|---|
| Glittering Halls | 2815 | 0.1667 |
| Conduit / Crucible / Sanctum | 1689 | 0.1 |
| Hybridisation Chamber | 1005 | 0.0595 |
| Defense Research Lab | 804 | 0.0476 |
| Toxic Grove | 201 | 0.0119 |
| Locus of Corruption / Throne of Atziri | 20 | 0.00118 |

Locus and Throne's 0.00118 is Vertolka's "small chance" as a number. Two things
deliberately do NOT scale: `uniques_per_run` (his flat 0.25 — no page prints a
per-tier chest-unique chance to scale it by) and `mod_item_chance_raw` (no
anchor has been stated for it, so it stays raw and unmultiplied).

**What is a guess, and whose.** Three sources, and they are not
interchangeable.

- **Vertolka's**, all of it quoted on POE-124 from his 2026-09-06 message and
  flagged by him as proposals: the two c-per-percent rates (*"Maybe there we can
  setup something like 1% quant = 0,5c, 1% rarity = 0,25c"*), the two drop
  counts — 0.25 uniques and 0.1 vials per run at tier 3, stated as *"price of
  unique divided by 4 + price of vial divided by 10"* — and the one manual price
  in the table, *"Crucible of Flames is giving on average 2 temple gloves per
  run and their base price is usually around 30c"* (poe.ninja publishes no line
  for a mod-rolled rare). The unique count and the glove price are
  `Basis::Guess` in `drops.rs`; the c-per-percent rates and (since POE-262
  WI-2) the vial rate are `Knobs` defaults.
- **Vertolka's and poedb's TOGETHER**, which is the vial rate and nothing else
  (POE-262 WI-2). Its anchor is his estimate and its per-line ratio is poedb's
  measurement, so it is a `Basis::Guess` sourced to `VIAL_RATE_DERIVED`, which
  names both parents rather than crediting either alone. Half a measurement is
  not a measurement: every vial driver reads `guessed: true`, on all nine
  lines. That is why `LineDrops::has_guess` is now true for the nine vial lines
  rather than the six chest ones, and why Locus of Corruption — whose 846 c
  sale the feed printed — no longer reports an unguessed total.
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
listed against a rate), poedb's raw `mod_item_chance_raw` integer (no printed
scale and no stated anchor — see [GAME-FACTS.md](../GAME-FACTS.md)), and any
term with no count or no price, which is still listed as a `Driver` with
`chaos: None`.

**Amended 2026-09-06 (POE-262 WI-2):** that list used to name `vial_chance_raw`
beside `mod_item_chance_raw`. It no longer does. The integer is still never
multiplied by a price, but it IS read — as the ratio that turns
`Knobs::vials_per_run` into one line's expected count, which needs no scale
because the anchor and the line are the same stat.

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
- **Live** (**amended 2026-09-06, POE-262 WI-1**): the formula is summed. The
  ladder stands in only for a room where **nothing at all was summed** — no
  sale, no priced drop, no bonus — and there it is
  `Grade::fallback_chaos_scaled(L)`, where **L is the lowest tier-3 total among
  the rooms that SUMMED something on this read** (`Priced::Market` or
  `Priced::Partial` — `valuation::lowest_summed_room_total`). The set is **the
  formula sums of the 23 non-instrumental lines, overrides ignored**: a
  player's stated number is not a sum, so it never enters the set and never
  moves it, while the line's own formula sum always does. The two instrumental
  lines are out because their totals are letters rather than sums (§4). A++
  lands on L exactly and every lower grade on a fixed fraction of it. On the
  committed capture L is Toxic Grove's formula sum: 2.65 when WI-1 landed and
  **1.857 since WI-2** (its vial went from a flat 0.1 × 9 c to 0.1 × 201/1689 ×
  9 c), so B− reads 0.0696 and D 0.0046 where they read 0.0994 and 0.0066.
  L moves with the drop table and the knobs by design — it is a property of the
  read, not a constant.

  As accepted this bullet read `Grade::fallback_chaos_scaled(top_sale_delta)`
  capped at the lowest sale-priced room. That two-step survives for the §4
  lines and for nothing else.

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

**Why the cap — history, and now §4's rule only.** On the committed capture the
only two above-floor rooms are Locus of Corruption tier 3 (856 c, delta 846) and
Doryani's Institute tier 3 (400 c, delta 390) against a floor of 10. Anchored on
846 the A+ rung scales to 423, which still outranks the 390 the feed actually
printed. The cap pulls it to 390. Rescaling alone does not keep a letter under
the feed; the cap does, and only against the LOWEST sale-priced room. That
reasoning still stands — it is why `valuation::lowest_sale_priced_room_total`
exists — but since POE-262 it holds up the §4 instrumental lines and nothing
else.

**The residual is CLOSED (POE-262 WI-1, 2026-09-06).** As accepted, a letter
could still outrank a room whose own MEASURED value was small: Apex of Ascension
is B−, summed nothing and stood in at 31.725, while Chamber of Iron summed a
real 6.00 c area bonus (6 % quantity × 0.5 + 12 % rarity × 0.25) and stayed at
6.00. And the cap was a function of which rooms the feed priced: on a read where
Locus is the only sale-priced room the cap was 846, so an A+ line that summed
nothing read its full 423. Vertolka's rule, from his 2026-09-06 review of the
value table, decides it: *"if temple does not have APEX, value of room itself is
just zero and should be counted only based on what you can drop from it"* — an
unpriced room is worth less than every room something priced, and the letter
only orders the unpriced rooms among themselves. Anchoring on L delivers both
halves in one multiplication, and it makes a room's rung a function of its own
letter and L alone rather than of which OTHER rooms happened to fall back. L is
over the formula sums of the 23 non-instrumental lines, **overrides ignored** —
a player's stated number never enters the set and never moves it — so the anchor
is a property of the READ, and one Custom edit cannot move the other ten
unpriced rooms. Apex now reads 0.0696 against Chamber of Iron's 6.00. The ladder is not inert for the
ten rooms nobody prices: it still ranks them against each other, which is
what a ranking of unpriced rooms can honestly be.

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
any control over them must be labelled that way (POE-259's are) — a slider
reading "2" beside a board of three-figure chaos numbers is a slider nobody can
set. The bridge multiplies by `StrategyProfile::value_scale()` = top tier-3
value / 9. That multiplier is applied to exactly five magnitudes, and the table
in `REFERENCE_TOP_ROOM_VALUE`'s doc is normative for the list: `apex_score`,
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
- **Anchoring the fallback ladder on the top sale delta and capping it at the
  lowest sale-priced room** (the rule as accepted for POE-257, superseded by
  POE-262 WI-1).
  A letter still outranked a small measured room — Apex of Ascension's 31.725
  over Chamber of Iron's 6.00 — and the cap moved with which rooms the feed
  happened to price, so a room's rung changed when a DIFFERENT room lost its
  price. Its reasoning survives for the §4 instrumental lines, which take that
  anchor and that cap unchanged.
- **Clipping every rung at L** rather than scaling the ladder onto it. Every
  grade above the clip collapses to L, so B− and C tie at L on the capture
  and the letters stop ordering the unpriced rooms — the half of Vertolka's
  rule the grades exist for.
- **Anchoring the board's top FALLBACK letter on L.** A C room's value would
  then move the moment a B− room lost its price, which is exactly the
  dependence on other rooms' fortunes that the old cap was rejected for.
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

- **Amended 2026-09-06 (POE-258): production runs on live prices.** As accepted,
  this bullet read that `run.rs` always took the cold-ladder branch because
  nothing filled the market. It does not any more: `run.rs` reads the market
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
- **No preset was reachable from the UI.** POE-257 shipped no setters; the
  preset and the Custom table were settings-file-only. **Amended 2026-09-06
  (POE-259):** the Temple page's picker and value editor close this — see the
  amended Status. The consequence that survives is the one that made it worth
  writing down: what a player can reach, they can also get wrong, so
  `temple_set_custom` validates per entry and per knob and returns advisory
  lines (both target lines at zero) without refusing the write.
- **`room_baseline` is 4.44 c on a cold read** — 0.05 × (800 / 9) — which is
  more than a D-grade tier-3 room is worth (2 c). On a board of nothing but
  junk, "open one more room" outweighs "the room you opened". That is an
  accepted side effect of stating the constant as a fraction of the TOP rather
  than of the floor; the owner can lower it with no code change. **Open owner
  item.**
- **A sixth scaled magnitude added without a row in `REFERENCE_TOP_ROOM_VALUE`'s
  table is a constant that will quietly stop meaning what it meant.** That table
  is the checklist.
- **Amended 2026-09-06 (POE-262 WI-2): the Mirage vial-spike case is now a KNOB
  value, not a missing scale.** At the default anchor an 808 c Vial of Summoning
  adds 74.8 c to Sanctum of Immortality tier 3 (60.75 → 135.55 on the capture),
  which still cannot rival Doryani's 390; reaching the epic's "a high-chance
  vial room rivals Doryani" case needs about 0.41 vials per run on the anchor
  lines. That is no longer blocked on deriving a scale for the poedb integers —
  `vials_per_run = 0.41` reaches it, and Vertolka's own proposed 0.2 is halfway
  there. What remains is his call on what the shipped Default should be: **open
  owner item**, for Vertolka. The shipped test asserts Sanctum crossing Crucible
  of Flame (67.35) instead. Glittering Halls, whose raw 2815 is 1.67× the
  anchor, is the line that reaches highest for a given anchor. A second Default
  is open beside it: Vertolka 2026-09-06 proposed 0.4 c per rarity % as a
  possible Default; today it is a Custom edit; owner call open.
- **The grade ladder is provisional and dated.** It is calibrated on one day's
  capture, and it is the only place a third-party LETTER (Vertolka's grade)
  enters arithmetic — through a rung this project chose, not one he stated.
- **On a live read the letters read as FRACTIONS OF A CHAOS, and that is the
  point** (POE-262). Anchored on the cheapest summed room, a C rung is 0.0232 c
  and a D rung 0.0046 c on the committed capture (0.0331 and 0.0066 before WI-2
  moved the anchor room's own vial term). A number that small is not the
  claim that the room is nearly worthless in the game — it is the claim that
  nothing on this read priced it, which is what the `F` mark beside it says.
  `values.ts::formatChaos` keeps two significant digits under 1 c for the same
  reason: `0.00` would read as zero, and epic lock L4 forbids a fallback from
  saying zero. Two bands are carved out of that rule for the CELL rather than
  for the number — `[0.995, 1)` keeps the two-decimal form so 0.999 reads
  `1.00` rather than introducing a third format, and anything under 0.0001
  prints the literal `<0.0001` rather than the exponential `1.0e-7`. The
  per-run COUNTS moved the same way and for the same reason: a derived rate is
  `0.1 × 2815/1689`, so `values.ts::formatCount` prints two significant digits
  with trailing zeros trimmed (`×0.17`, `×0.012`) where the raw float would
  put seventeen digits in an overlay cell and `toFixed(2)` would print the
  smallest rates as `×0.00`.
- **`advisor::rollout`'s `lost_threshold` moves with the anchor** (POE-262).
  It is the minimum POSITIVE tier-3 value among the lines the mode rule names
  as TARGETS (the corruption and gem lines for the shipped profiles). Both are
  sale-priced on the committed capture, so it reads 390 and nothing about it
  changes. On a read where a target line loses its price, its tier-3 value is
  now a rung — a fraction of a chaos — and the risk band collapses with it.
  Accepted rather than guarded: a target line the market does not price is a
  board with no target, and the band is measuring nothing that read.
  `rules::noise_margin` is unaffected — it scales on
  `StrategyProfile::value_scale`, which reads the MAXIMUM tier-3 value, and the
  top of the board is a summed room on every read that prices anything.
- **POE-260 must branch on `scaledFromTier3` before rendering `drivers`.** The
  copied tier-3 terms are unscaled by design; the `tier_fraction` driver is the
  headline.
- **Amended 2026-09-06 (POE-262 WI-2): the `guessed` mark now says almost
  nothing, and a materiality threshold is an open owner item.** Deriving the
  vial rate gave every line poedb prints a vial chance on a guessed count, so
  on the committed capture `RoomValue::guessed` is true for 24 of the 25 tier-3
  rooms — `only_doryanis_institute_is_free_of_estimates` is the pin, and
  Doryani's Institute is estimate-free only because it names no unique, no vial
  and no bonus. Locus of Corruption is the case that shows the cost: it earns
  its `G` on a vial term of 0.51 c inside a total of 846.51, a term three
  orders of magnitude below the figure the mark sits beside. So the mark has
  stopped separating soft numbers from firm ones; it now reports that SOME term
  rests on somebody's estimate, which is nearly always. A threshold — mark only
  when the guessed terms are a material share of the total — would restore the
  separation, and picking the share is the owner's call: **open owner item**.
  Not urgent, because of the bullet below: `guessed` is a flag under ADR-018,
  so it orders nothing either way, and what a bad threshold costs is a legend
  players learn to ignore rather than a mis-ranked board.
- **Every provenance flag is a flag.** `guessed`, `lowConfidence` and
  `windowPriced` ride on the driver that carries them and never remove a room
  from the ranking or reorder one (ADR-017, ADR-018).
- **The two settings blocks are typed, against ADR-013's default.** Rust reads
  them, and `prefs` is a webview-owned map.
