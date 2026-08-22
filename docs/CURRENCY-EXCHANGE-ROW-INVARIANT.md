# Currency Exchange row — the arithmetic invariant

Status: CURRENT. Normative for the desktop Currency Exchange table.

Last verified: 2026-08-22 against `main@50ec6ff` — `desktop/src/lib/exchange/view.ts`,
`filters.ts`, `internal/exchange/plays.go`, `desktop/src/lib/tooltips.ts`.

This document is the single normative statement of what the numbers on one
Currency Exchange row mean and how they must agree. It is normative because the
invariant spans five emitters and one wire contract — `desktop/src/lib/exchange/view.ts`,
`desktop/src/lib/exchange/filters.ts`, `desktop/src/lib/pages/CurrencyExchangePage.svelte`,
`desktop/src/lib/components/ExchangeRoute.svelte`, `desktop/src/lib/tooltips.ts`, and the
field docs in `internal/exchange/plays.go` — and no one of those files can hold
it without the other five drifting away from it silently. That is the whole
history this document exists to end: three rounds of "the numbers on one row
disagree", each fixed in one emitter.

Behaviour remains authoritative in code (AGENTS.md). This document does not
compete with that: it states the equations, and the CLOSURE TESTS in
`desktop/src/lib/exchange/view.test.ts` assert them on emitted output. If the
code and this document disagree, one of them is a bug and the closure tests say
which.

Referenced from: `desktop/src/lib/exchange/view.ts` header comment (the only
emitter of the mechanical numbers), and `docs/README.md`.

Related: ADR-015 (quality gates live client-side), ADR-016 (Expected ROI is a
cross-hour simulation; displayed prices stay single-hour), ADR-017 (no default
engine floor may hide a live market).

---

## 1. The three rules

**SCALE.** Every money figure on a row counts the SAME number of exchanges.
That number is `worthwhileScale(play).flips` — the repeat count at which the
play's measured expectation clears `SCALE_TARGET_CHAOS` (100c). A play with no
positive expectation has no such count; the row then counts ONE exchange, and it
does so in every emitter at once. There is no third scale and no per-emitter
fallback.

**BASIS.** Every MECHANICAL number on a row is priced at the UNDERCUT FILL
PRICES — the price an order that actually gets taken is posted at:
`Price*(1+Tick)` on a buy leg, `Price*(1-Tick)` on a sell leg
(`internal/exchange/plays.go:47-84`, computed at `plays.go:963-967`). The raw
hourly extremes appear on the row in exactly one place, the step hovers, worded
as what the market PRINTED rather than as what to post.

**CLOSURE.** The row's printed numbers satisfy the equations in §3 by
CONSTRUCTION — one variable feeding two emitters — not by two calculations that
happen to agree. Where construction cannot reach (a value that must cross a
currency conversion or a different formatter), the closure tests assert the
equation on the emitted output.

---

## 2. Symbols

For one play, with `divineChaosRate` from the same response:

| Symbol | Definition | Source |
| --- | --- | --- |
| `N` | flips: `worthwhileScale(play).flips`, or `1` when there is no worthwhile scale | `view.ts:519-530` |
| `r` | chaos per unit of the ENTRY quote: `1` for chaos, `divineChaosRate` for divine | `chaosPerQuote`, `view.ts:768-772` |
| `u0` | undercut buy price of leg 1, in entry-quote units per item: `legs[0].price * (1 + legs[0].tick)` | wire |
| `u1` | undercut sell price of leg 2, in leg 2's own quote per item: `legs[1].price * (1 - legs[1].tick)` | wire |
| `u2` | undercut price of leg 3 (1-hop only), entry-quote per unit of the intermediate: `legs[2].price * (1 - legs[2].tick)` | wire |
| `I` | `moneyColumns(play).investment` — chaos the run ties up | `view.ts:578-588` |
| `R` | `moneyColumns(play).roi` — chaos the run gains at the hour's BEST-CASE prices | `view.ts:578-588` |
| `X` | `moneyColumns(play).expectedRoi` — chaos the run is MEASURED to pay | `view.ts:578-588` |

`r` is the client's mirror of the server's `entryRate` and is bit-identical to
it: `Result.DivineChaosRate` is the newest hour's divine/chaos VWAP
(`plays.go:776`) and every served play cleared in that hour
(`plays.go:681-686`), so `chaosPerQuote(legs[0].quote, divineChaosRate)` is the
same float the server valued `Investment` at.

Legs are read BY POSITION, never by `action` — leg 3 is a `sell` on the wire and
a *convert* on screen. `u2` uses the sell form for that reason and states it.

---

## 3. The invariant equations

Stated in CHAOS, which is the one unit every figure can be valued in. Each end
and each step is rendered in the currency named in §4; the rendering is a
division by `r` and never a second derivation.

```
E1  I  =  N · u0 · r                     (the run's cost at the undercut entry)
E2  buyStepTotal   ·  r  =  I           (buy step total = Spend = Investment)
E3  chainEnd       ·  r  =  I + R       (the mechanical end of the row)
E4  chainEnd·r − buyStepTotal·r  =  R   (the ROI column, by subtraction)
E5  get            ·  r  =  I + X       (Get = Spend + Exp. ROI)
E6  keep/lose line  =  |X|,  X  =  Exp. ROI column  =  Scale column's "→ +Xc"
E7  direct:  sellStepTotal = chainEnd
    1-hop:   sellStepTotal = chainEnd / u2   and   convert step prints
             sellStepTotal → chainEnd
```

`chainEnd` is the last mechanical total the row emits: the sell step's total on
a direct play, the convert step's right-hand amount on a 1-hop. E4 is Sebastian's
"sell-step total − buy-step total = ROI" in the form that also holds for a
triangle, where the sell total is denominated in the intermediate currency and
cannot be subtracted from the buy total at all.

**Why E7 derives the sell total backwards from `chainEnd` rather than forwards
from `legs[1].price`.** Because `roiPct` is the WIRE'S ANSWER to what the round
trip returns, and the client must not be able to disagree with it. `roiPct` is
served (`plays.go:1012`, computed at `plays.go:963-1006` from the same undercut
prices the legs carry), `R` is built from it, and `chainEnd` is built from `R`.
A forward derivation (`N · u1`) recomputes the served answer from the served
inputs and then prints its own result beside it — so the moment the server's
formula and the client's reading of the legs diverge for any reason (a leg
served with a price the server did not use for that hour, a mode the client
infers by leg count and the server decided by `Mode`, a future change to
`roiPctOf`), the row prints two answers to one question and nothing on screen
says which is the served one. Deriving backwards makes that class of divergence
unrepresentable: there is one mechanical chain, and its every total is the
wire's `roiPct` rendered.

Forward derivation is also the same number only in real arithmetic — `roiPct` is
`u1/u0 − 1` for a direct play and `u1·u2/u0 − 1` for a 1-hop
(`plays.go:1083-1092`), so `I·(1+roiPct) = N·u1·r` exactly on paper, while in
float `200·19·1.01` and `19.19·200` differ in the last ulp. That is a real
effect and it is why the CROSS-CHECKS in §7 carry a tolerance, but it is about
one ulp and is invisible to the reader; it is a supporting detail, not the
reason for the rule.

**The one permitted forward derivation.** On a 1-hop whose convert leg carries
no usable price (`legs[2].price` non-finite or `≤ 0` — version skew), `u2` does
not exist and `chainEnd / u2` is not a reading. The sell step's total then falls
FORWARD to `N · u1`, and that is the only place in the row where a mechanical
total is computed forwards. It is permitted because the alternative is worse in
the specific way this document exists to prevent: the fallbacks that do not use
it print the market's LOT quantity on a step whose two ends count a run
(§4.7). `chainEnd` itself is unaffected — it is `I + R` and never touched `u2` —
so E2, E3, E4 and E5 all still hold on such a row; only E7's 1-hop identity is
replaced, and the convert step drops its total for the market ratio line and
says why in its hover.

This is also why the convert step's divisor changes from the RAW price to `u2`.
The old rationale (`view.ts:1219-1222`) — bare `price`, because the undercut is
already inside `expectedRoi` — was correct only while the convert line's
numerator was Get. Under E3 the numerator is `chainEnd`, which is built from
`R`, which is built from `roiPct`, which already contains `u2`. Recovering the
proceeds from `chainEnd` therefore requires dividing by `u2`; dividing by the
raw price would print a proceeds figure the wire's own `roiPct` contradicts.

**The single scale rule, restated as code.** `N` and the three money roots come
from `moneyColumns(play)` and nowhere else. `moneyColumns` has two branches — the
run and the single exchange — and every emitter takes the SAME branch at the
same time because they all read the same function. The route ends, the three
money columns, the Scale column, the Run cost bounds
(`filters.ts:663-679`) and the step totals are one set of numbers.

**The one exempt branch: an entry currency this response cannot value in chaos.**
`chaosPerQuote` answers `null` when the entry quote is divine and the response's
`divineChaosRate` is 0 (`view.ts:759-772`). There is then no `r`, so E1–E4 and
E7 have no entry-currency rendering to be stated in and the row cannot carry a
mechanical chain at all. That branch renders both ends in CHAOS from
`moneyColumns` and prints the markets' own ratios on the steps. E5 survives
there and is the reason the branch is still a closed row: the ends are
`I` and `I + X` in chaos, so Get is still Spend plus the Exp. ROI column. E2,
E3, E4 and E7 are suspended, and the suspension is the branch's whole content.
It is unreachable on a served body — no divine-quoted play is served in an hour
that carried no divine/chaos trade (`plays.go:681-686`, `plays.go:776`) — and it
is guarded anyway, because the alternative is a division by zero printed as an
amount.

---

## 4. Rendering rules

These are part of the invariant, because a number that closes and then prints
through two different formatters does not close for the reader.

1. **Ends are in the ENTRY currency** — the currency the reader actually pays
   with — with a chaos sub-line when the entry is not chaos. This holds in BOTH
   scale branches. Chaos prints whole orbs (`formatChaos`), anything else prints
   to the hundredth (`formatFractionalOrbs`); the choice is `withOrbUnit`.
2. **A step total prints through the SAME formatter as the end it must equal.**
   Buy total and Spend are one variable through `withOrbUnit` with the entry
   unit, so they are the same STRING and not merely the same number. Likewise the
   1-hop sell total and the convert line's left amount.
3. **A step total carries `≈ `** — space included — because it is not a postable
   order: `buy 200 for ≈ 3,838c`. The item count before it is exact and carries
   no `≈`. The convert line keeps its single leading `≈`, which governs both of
   its amounts: `≈ 2.97 div → 615c`.
   The space after `≈` is deliberate and matches the two strings already shipped
   (`keep ≈ 102c`, `≈ 2.52 div → 526c`). Do not close it up.
4. **The ends carry no `≈`.** They never claimed to be orders. The
   approximation on the Get side is already carried by its profit line.
5. **The profit line's VERB follows the sign of `X`, and its amount is a
   magnitude.** `keep ≈ 102c` when the run is measured to gain, `lose ≈ 3c`
   when it is measured to lose, and `keep ≈ 0c` at exactly zero. The amount is
   `formatChaos(Math.abs(X))`, never the signed rendering: "keep ≈ -100c" asks
   the reader to hold two negations at once, and on a divine entry it doubles
   up into "keep ≈ -100c (≈ -0.50 div)". The sign lives in the word, which is
   the only place on the row where a word carries a sign, and that is why the
   word rather than the number is where it belongs.
6. **The exactly-postable pair lives in the step's hover**, worded as the
   market's printed extreme — "This market printed 4 for 1 div" — and never as
   an instruction. A leg served without a usable pair (version skew) simply
   drops that sentence; a step that prints a TOTAL does not depend on the pair
   at all any more, so there is no decimal fallback left on that line. (The
   ratio line of rule 7 still has one, because a ratio line is nothing but the
   pair.)

   The hover also carries the market's LOT, because deleting the snap deleted
   the row's only disclosure of it: a line reading `sell 12 for ≈ 2.97 div` on a
   market that posts four at a time asserts a quantity no single order can move.
   So the hover adds one clause whenever the run is not a whole number of the
   market's lots — "this market posts in multiples of 4, so the run of 12 is 3
   whole orders" — with the unsold-residue sentence when the division leaves a
   remainder on a SELL step ("2 of the 12 bought stay unsold"), and the
   smaller-than-one-lot sentence when the run is under a single lot. That is the
   same pair of facts the snap's two hovers carried (`view.ts:957-965` and
   `view.ts:972-975`), moved off the line and onto the hover with the pair.

7. **A step with no total prints the market's ratio, and says so.** Three places
   have no total to print: the exempt branch of §3, a 1-hop convert step whose
   leg carries no usable price, and the sell step of a 1-hop whose sale could be
   priced from neither the convert leg nor its own. All three fall back to one
   shared helper that renders the leg's own posted pair (`buy 16 for 1 div`,
   `convert 1 div for 209c`), or the decimal rate when even the pair is unusable
   (`convert @ 0.00 c`). A fourth caller of that helper prints no line at all —
   the convert step's run-total hover quotes the market's posted pair inside its
   own sentence, from the same helper, so the hover and the fallback line cannot
   word one market's order two ways. Such a line prints the MARKET'S LOT
   QUANTITY and not the run, which is the mixed-quantity reading the rest of
   this document exists to end — so it carries a hover that says the row's ends
   count the run while this line counts one order of this market, and line
   emission is confined to those three places.

   The hover FOLLOWS THE BRANCH the line took. A line that fell all the way
   through to the decimal rate posted no pair, so its caveat names the per-unit
   rate rather than a quantity pair that is nowhere on screen; the run-versus-lot
   half of the sentence is unchanged, because the ends beside it count the run
   either way.

---

## 5. The one permitted deviation: the expectation

`chainEnd` (E3) and `get` (E5) are different numbers. Their difference is
`X − R` — the measured expectation minus the hour's best case — and that gap is
the reason this table exists (ADR-016).

Measured across 960 top-20 play-hours that difference is negative and large: the
best case overstates the measurement by four to eight times (`tooltips.ts:89`).
Nothing in the arithmetic guarantees the SIGN, though, and no emitter, no
formatter and no closure test may assume `chainEnd > get`. A row whose measured
expectation beats its hour's best case is a legal row, and the matrix carries
one so that assumption cannot be made by accident.

The deviation is permitted, bounded and labelled:

- It is ONE number, `X`, appearing in three homes, all reading
  `moneyColumns(play).expectedRoi` (`= worthwhileScale(play).gain` whenever a
  run exists): the **Exp. ROI cell**, the **Scale column's `→ +Xc`**, and the
  **Get slot's `keep ≈ Xc` / `lose ≈ Xc`** line (§4.5 — the verb carries the
  sign, the amount is the magnitude). Three renderings of one variable, never
  three calculations.
- Nothing else on the row deviates. Every other printed figure is on the
  mechanical chain.
- The reader can close the gap from the columns: the last step's total is
  `Investment + ROI` and Get is `Investment + Exp. ROI`, both of which are
  printed two and three cells to the right.

**This rule applies in the no-run branch too, and that is a change.** Before
this document, a play with no positive expectation rendered `Get = Spend + roi`
— the best case — while its Exp. ROI cell showed the measured loss. That row
did not close: `Get − Spend` was not the Exp. ROI column. Under E5 it does: the
fallback row's Get is `Spend + X` with `X ≤ 0`, its `lose ≈` line prints the
magnitude of the loss, and `positive` is `X > 0`. A Get below its Spend on such
a row is the measurement, not broken arithmetic — and the best case is still on
the row, as the last step's total.

**Considered and rejected: `Get = chainEnd` on every row.** The alternative
closes the row the other way round — the route stays wholly mechanical, both
ends and every step on the one best-case basis, and `keep ≈ X` moves off the
Get slot onto the Exp. ROI cell's existing sub-line, which already renders `n=`
and `low` (`CurrencyExchangePage.svelte:677-702`). It is a genuinely smaller
change: no red Get, no verb over a loss, no `positive` flip, no rendering change
to the fallback rows at all.

It is rejected because it moves the expectation OFF the route, and the route is
where the reader decides. The row's five slots are the one place the play is
read as a sequence of actions — spend this, buy that, sell it, get this back —
and "what you get back" answering with the hour's best case puts the number the
table is ranked on two columns away from the sentence that ends in it. The
whole reason ADR-016 exists is that the best case overstates the measured
outcome by four to eight times; ending the route on it and footnoting the
measurement inverts which of the two the row asserts. The cost of the choice
made is real and is paid here: the deviation of §5 exists at all, the fallback
branch's Get can print below its Spend, and the shipped
`keep ≈ 102c (≈ 0.51 div)` string — the one the width contract is sized around
(`ExchangeRoute.svelte:220-232`) — has to stay on the row and be sized for.

---

## 6. Re-affirmed exceptions

Each of these is OUTSIDE one of the three rules on purpose. Each states which
rule it is outside of and why.

### 6.1 The per-exchange gate knobs (outside the SCALE rule)

`filters.ts:565-590` — `minItemPrice` vs `play.investment`, `minRoiChaos` vs
`play.roi`, `minTurnover`, `maxTickPct`, `minEdgeTickRatio`, `minRoiPct` — all
read the wire's PER-EXCHANGE fields, never the run.

*Why exempt:* the armed levels are calibrated numbers, not free parameters. Each
tooltip names the level worth typing (`Min profit` "type 3", `Min turnover`
"type 10000", `Max price step` "type 10", `Edge vs step` "type 5", `Min return`
"type 2"), and every one of those is the floor the SERVER applied per exchange
before POE-191 handed the judgement to the reader. Retargeting them at the run
would multiply each level by a per-row flip count: a "3c profit" floor would
mean 3c-per-run on a ×1 row and 3c-per-exchange-times-34 on a ×34 row, so the
number in the box would no longer be a level at all. The calibration is the
reason, and it cannot survive a per-row multiplier.

The deeper reason is that a gate asks a different question. A gate asks whether
this MARKET is worth trading; the columns ask how far the play has to be
repeated to pay. A market has no run. So the split is not a seam to be closed
but two questions with two right answers, and the tooltips for `Min item price`
(`tooltips.ts:128-129`) and `Min profit` (`tooltips.ts:130-131`) already say the
uncomfortable half out loud: *no column prints the figure this compares
against*. That sentence is load-bearing and must not be edited away.

**The exception's own exception:** the filter bar's **Run cost** bounds
(`filters.ts:663-679`) DO read the run, because a bankroll ceiling is a run-sized
quantity — the reader is asking what they can afford to have tied up, not what
one exchange costs. That bound reads `moneyColumns(play).investment`, the exact
figure the Investment column prints and the Scale column's "N c in" sub-line
repeats, so it is INSIDE the scale rule rather than exempt from it.

### 6.2 ROI% (outside the SCALE rule and outside the BASIS rule)

`play.roiPct` / `play.roiPctRaw`, rendered at `CurrencyExchangePage.svelte:704-718`.

*Why exempt from SCALE:* it is a ratio. The run multiplies numerator and
denominator by the same `N`, so the per-exchange percentage and the per-run
percentage are the same number. It is not on one scale; it is on all of them at
once, which is the literal meaning of scale-free. Tagging it with a scale would
imply a distinction that does not exist.

*Why exempt from BASIS:* it is the one figure on the row whose JOB is to compare
the two price bases. NET is the undercut round trip; RAW is the same round trip
at the hour's raw extremes; the gap between them is exactly what the fill steps
cost, and on a coarse market it is the whole spread. A single-basis ROI% would
delete the only reading that prices the undercut.

Neither number is what the table is ranked by (that is `ExpectedRoi`), and NET
is what the Gates row judges — both already stated at `tooltips.ts:86-87`.

### 6.3 Depth (outside SCALE and BASIS)

`play.depth`, rendered at `CurrencyExchangePage.svelte:738-750`.

*Why exempt:* it is a MARKET reading, not a figure of the play — units per hour
that changed hands on the play's thinnest leg. It is the whole book's volume and
not the reader's share, it is a count of items rather than money, and there is no
undercut price of a volume. Scaling it by the flip count would claim the market
gets deeper the more the reader wants to trade.

### 6.4 `Scale.hours` (outside the BASIS rule)

`worthwhileScale().hours = ceil(flips / depth)` (`view.ts:528`).

*Why exempt:* it is a TIME, and a time has no price basis. It inherits the run
scale correctly (its numerator is `flips`) but its denominator is §6.3's
whole-book volume, so the answer assumes the reader takes every unit the market
trades and nobody competes for it. That assumption is a FLOOR on the wait, not
the wait, and the Scale tooltip states it in those words. It is not corrected,
because there is no served figure for the reader's own share of a book; the
honest move is to label the assumption, which is what is done.

**Terminology.** This assumption used to be called "optimistic", the same word
the row used for the best-case price basis. Two different senses of one word on
one row is how numbers come to disagree in a reader's head, so the word is
retired from this surface. The price basis is **BEST CASE**. The whole-book
hours are **UNCONTESTED**. Neither word is used for the other meaning anywhere
in `tooltips.ts`, `view.ts` doc comments, or the page caption.

### 6.5 A basis note that is not an exception

`moneyColumns(play).roi = play.roi * flips` (`view.ts:585`) has been read as
mixing bases — a fill-simulation-derived flip count multiplying a best-case
per-exchange figure. It does not. **A flip count is a SCALE, not a BASIS.**
`flips` is a dimensionless repeat count; the only price basis inside `roi` is
`roiPct`, which is the undercut round trip and nothing else. The ROI and
Exp. ROI columns are on the SAME scale and on their own declared bases, which
is exactly what §1 asks for. `WorthwhileScale` carries no best-case total and
should not: it exists to answer how far the MEASURED expectation has to be
repeated.

---

## 7. Enforcement

The equations of §3 and the rendering rules of §4 are asserted by a **closure
suite** in `desktop/src/lib/exchange/view.test.ts`. The matrix is all four of
`{direct, 1-hop} × {chaos entry, divine entry}` on the scaled branch, two of
those four repeated on the no-run fallback (one of each shape, one of each entry
currency — the remaining two vary nothing the first two do not), and the exempt
branch of §3 as a seventh case that asserts the SUSPENSION rather than the
equations. The suite parses the EMITTED strings and values — not intermediate
state — because the reader compares printed numbers.

Four assertion tiers, with the tolerance rule for each:

- **T1 — string identity (exact).** Two emitted strings that must show the same
  number are produced from one variable through one formatter, so they are
  compared with `toBe` on the whole string, with no tolerance. Covers:
  - the buy step's total against the Spend amount. The step line carries a unit
    and the end does not (`RouteEnd.amount` is bare, its unit word being a
    separate span — `ExchangeRoute.svelte:143-144`), so the pin is two
    assertions and not one: the line ENDS WITH `for ≈ ${withOrbUnit(spend,
    entryUnitShort)}`, and the numeric head of that tail is character-identical
    to `route.spend.amount`. Either one alone can pass while the two numbers
    differ.
  - 1-hop only: the sell step's total against the convert line's left amount,
    same two-part shape.
  - the Get slot's profit line against the Exp. ROI cell's value — including its
    VERB, which is `keep` for `X > 0` and `lose` for `X < 0` (§4.5), so the pin
    is sign-aware and runs on the losing fixtures too.
  - the Scale column's `→ +Xc`. This one is scoped to the SCALED fixtures:
    `worthwhileScale` answers `null` on a play with no positive expectation, so
    a `formatGain(worthwhileScale(play)!.gain)` written across the whole matrix
    throws on the fallback rows rather than asserting anything.
- **T2 — ledger identity (exact).** `chainEndChaos === I + R` and
  `getChaos === I + X` are single float additions of the ledger's own roots and
  are compared with `toBe`. This is what "closure by construction" means
  mechanically: there is no second expression to drift. `I` is pinned by
  RE-DERIVING it — `investmentChaos === play.investment * flips` — and not by
  comparing it to `moneyColumns(play).investment`, which is the expression the
  field is assigned from and would assert nothing.
- **T3 — cross-check against the wire and against hand-worked arithmetic
  (relative tolerance 1e-9).** `R === play.roi * N`, and the forward derivations
  `chainEnd ≈ N·u1` (direct), `sellStepTotal ≈ N·u1` (1-hop) and
  `chainEnd ≈ N·u1·u2` (1-hop) cross the wire's `roiPct` multiplication and
  therefore reassociate: `19*1.01*200` and `19.19*200` differ in the last ulp.
  Asserted with `|a − b| <= 1e-9 * max(1, |b|)`.

  A worked example of why the tolerance is needed, not a convenience:
  `chaosScarab` has `price 19`, `tick 0.01`, `investment 19.19`. `200 * 19 *
  1.01` and `19.19 * 200` are different floats. That is ambiguity 3 from the
  scout inventory, and the fix is to compute the figure ONCE (T2) and to
  tolerate the reassociation only where a cross-check deliberately recomputes
  it (T3).
- **T4 — the reader's own check, across independently rounded printed values.**
  The reader subtracts the printed Spend from the printed chain end and expects
  the printed ROI column; likewise Get minus Spend against Exp. ROI. Two rules
  make that assertable:

  - **Unit.** The two ends print in the ENTRY currency and the two columns print
    in CHAOS. The assertion converts — it multiplies both parsed ends by `r` and
    compares in chaos — because chaos is the unit both sides can be stated in
    and the columns are the side with no choice. Comparing `4.95 − 3.16 = 1.79`
    against `+359` is not a weaker test, it is a different quantity.
  - **Slack.** Three independently rounded values take part. Each printed end
    carries up to half a printed unit of rounding and the column carries half a
    chaos, so the residual is up to `1 ulp + 0.5c/r`, which for a divine entry
    at a realistic rate is about one and a half printed units — 0.0125 div at
    `r = 200`. The bound is therefore **2 printed units: 1c for a chaos entry,
    0.02 div otherwise** (`0.02 · r` chaos once converted). One unit is NOT
    enough on a non-chaos entry: it fails on healthy code as soon as the
    residual passes 0.01, and a fixture that happens to land at 0.005 hides
    that. For a chaos entry all three values are integers, so the residual is an
    integer and the bound collapses to 1 by integrality.

  This slack never hides a bug, because the same fixture also pins each side's
  exact string under T1.

**Fixture rule.** A closure fixture must be WIRE-CONSISTENT: `investment` =
`u0 · r`, `roiPct` = the server's own formula over the legs' undercut prices
(`plays.go:1083-1092`), and `roi` = `investment · roiPct`. A fixture that
contradicts those identities can make a closure test pass or fail for a reason
that has nothing to do with the code. Fixture INPUTS may be derived from the leg
prices in the test file, with the hand-worked value in a comment; EXPECTED
values are always literals.

The rule binds the `routeSlots` fixtures too, and not only the closure matrix's
own. Those fixtures pin EMITTED STRINGS, and once every string on the row is
read out of `investment`, `roi` and the legs together, a fixture whose
`investment` contradicts its buy leg's price pins a row no server could send.
There is one fixture set, wire-consistent, and the closure matrix is built from
it by overriding `expectedRoi`.

---

## 8. Out of scope for this document

Gate semantics and levels; the server's pricing and simulation arithmetic; the
Exp. ROI column's non-monotonicity in the served order (`view.ts:604-613`, a
seam by choice); the Gold column; the gem-flip ROI domain, which shares the
words and means percentage points (`tooltips.ts:70-83`).