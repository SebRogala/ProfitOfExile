# Currency Exchange row — the arithmetic invariant

Status: CURRENT. Normative for the desktop Currency Exchange table.

Last verified: 2026-09-05 against `poe-252@725dbe3` — `desktop/src/lib/exchange/view.ts`,
`filters.ts`, `desktop/src/lib/pages/CurrencyExchangePage.svelte`,
`desktop/src/lib/components/ExchangeRoute.svelte`, `desktop/src/lib/tooltips.ts`,
`internal/exchange/plays.go`, `internal/exchange/direct.go`. EVERY `file:line`
reference below was re-checked against that commit and re-pointed where it had
drifted — the earlier `poe-252@6f76efd` re-stamp moved the date without moving
the offsets, which is the failure this line now forecloses. Each reference names
the SYMBOL beside its offset, so the next drift is found by grepping the name.
The divine trash-price knob that the 2026-08-23
stamp carried as uncommitted landed in `c43e76f`; POE-220 (`c175941`) then edited
§4's sort ruling.

This re-stamp covers **POE-252**: a market whose scored hour traded under
`Config.ThinHourVolume` units prices its legs from a trailing CLOCK WINDOW instead
of from that hour, and the row ships three fields saying so — `windowPriced`,
`windowHours`, `windowVolume`, on the play and on each leg. That moves WHICH HOUR
a price belongs to and nothing else: no equation in §3 changes, no rendering rule
in §4 changes, and the scale rule of §1 reads the same field it always did. What
it touched here is §2 (the symbols' hours), §4's closing note on `fair`, the new
§6.6, and §7's enforcement.

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
cross-hour simulation; displayed prices stay single-hour — and, since its
2026-09-04 amendment, the trailing window a thin pair prices from), ADR-018
(flags mark, they never order), ADR-017 (no default
engine floor may hide a live market).

---

## 1. The three rules

**SCALE.** Every money figure on a row counts the SAME number of exchanges.
That number is `displayScale(play).units` (`view.ts`), and ONE RULE chooses it
on every row: ONE MINIMAL POSTING of the market the play buys on — the buy leg's
own `priceItemQty` when that pair is usable, and ONE item when it is not
(version skew, `hasQuantityPair`). The entry currency does not enter into it.

Whichever branch is taken, every emitter takes it at the same moment, because
they all read the one function. There is no per-emitter fallback.

*Why the posting, and only the posting* (owner rulings, Sebastian, 2026-08-22 —
two of them, a day's work apart). A run-scaled row deceives: a 30c card whose
worthwhile run is sixteen exchanges rendered "buy 16 for ≈ 384c", which reads at
a glance as a 384c blockbuster and fakes the ranking's first glance — while
buying sixteen of anything on such a market is slow and, at the depths these
markets carry, unrealistic. Markets post either X-items-for-1c (trash) or
Xc-per-item, and the row must read at that minimal posting.

The FIRST ruling took the run off the chaos entries and left the divine ones
alone, on the reasoning that a divine Spend is a fraction of an orb whatever
count it carries and so fakes no size at a glance. The SECOND ruling struck that
exemption out. In the owner's words: *noone will buy 159 for ≈ 2.72 div — that
is not measurable price*. A run counted in divine is exactly the same deception
as a run counted in chaos — an unplaceable quantity beside a price nobody can
post, taking the top of the ranking on a number no order backs. The fraction of
an orb was never what made it honest.

So the divine exemption is GONE and there is one rule. Every row renders the buy
market's own minimal posting; the run lives in the Scale column and nowhere
else. This document is the spec FOLLOWING both decisions, not preceding them.

**N > F is a legitimate reading, and is assumed rather than guarded.** The
posting can count MORE exchanges than the worthwhile run: a market that posts a
thousand at a time on a play whose run is 167 exchanges gives a row counting
1,000 beside a Scale column reading ×167. Nothing clamps that, and the reason is
that the posting is the MINIMAL EXECUTABLE TRADE — there is no smaller order on
that market, so a row shrunk to the run would print a quantity nobody can place,
which is the one thing §4.3's exactness claim forbids. The row prints the
posting whole and the BUY step's hover carries the overshoot ("this market posts
1,000 at a time, more than the ×167 run the Scale column sizes"), which is the
same disclosure route the lot clauses take. The consequence the reader sees is
that the Exp. ROI column can exceed the Scale column's own gain, and that is the
arithmetic being honest: one order of that market really does clear the target
several times over.

**The ordering consequence, stated rather than fixed.** The `'roi'` sort orders
by `moneyColumns(play).roi` (`view.ts`, `sortPlays`), which is a POSTING-sized
gain on every row. So the order no longer mixes two SIZE RULES across rows — but
it does still compare postings of different sizes, because a market's lot is a
fact about that market: a market posting a thousand at a time ranks above one
posting a single item at ten times the price. That is the same reading the chaos
rows already carried before the second ruling, it is disclosed where it is
caused — on each buy step's hover, which says what that market posts — and the
rule that the table is ordered by the number it SHOWS is what holds the sort
where it is. Re-pointing it at `play.roi` would order the table by a figure
printed on no row. Since POE-220 the `'expected'` sort reads
`moneyColumns(play).expectedRoi` under the same rule — it used to hand back the
served order — so the posting-size reading above is now the whole table's and
not just the ROI column's, and no flag partitions either order.

*What did not move.* `worthwhileScale` itself is untouched: the run is still
derived, still what the Scale column prints (×N, its cost, its hours), still
what the Fastest sort orders by. The Scale column is now the ONE place the run
appears on the surface, on every row. The BASIS rule below is untouched too —
only the scale moved, so the `≈` and the undercut fill prices stay exactly as
they were.

**BASIS.** Every MECHANICAL number on a row is priced at the UNDERCUT FILL
PRICES — the price an order that actually gets taken is posted at:
`Price*(1+Tick)` on a buy leg, `Price*(1-Tick)` on a sell leg
(`Leg`, `internal/exchange/plays.go:38-41`; computed in `evaluate`,
`plays.go:1254-1256`). The raw
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
| `N` | `displayScale(play).units`: the buy market's own posting, or `1` where that market posted no usable pair | `displayScale`, `view.ts:680-685` |
| `F` | `worthwhileScale(play).flips` — the RUN, which the Scale column prints on every row and which no rule makes equal to `N` any more; where the two coincide it is a market's lot happening to land on a flip count | `worthwhileScale`, `view.ts:572-583` |
| `r` | chaos per unit of the ENTRY quote: `1` for chaos, `divineChaosRate` for divine | `chaosPerQuote`, `view.ts:1266-1270` |
| `u0` | undercut buy price of leg 1, in entry-quote units per item: `legs[0].price * (1 + legs[0].tick)` | wire |
| `u1` | undercut sell price of leg 2, in leg 2's own quote per item: `legs[1].price * (1 - legs[1].tick)` | wire |
| `u2` | undercut price of leg 3 (1-hop only), entry-quote per unit of the intermediate: `legs[2].price * (1 - legs[2].tick)` | wire |
| `I` | `moneyColumns(play).investment` = `play.investment · N` — chaos the row ties up | `moneyColumns`, `view.ts:731-738` |
| `R` | `moneyColumns(play).roi` = `play.roi · N` — chaos the row gains at the hour's BEST-CASE prices | `moneyColumns`, `view.ts:731-738` |
| `X` | `moneyColumns(play).expectedRoi` = `play.expectedRoi · N` — chaos the row is MEASURED to pay | `moneyColumns`, `view.ts:731-738` |
| `I_run` | `runInvestment(play)` = `worthwhileScale(play).investment`, or `play.investment` with no run — what the RUN ties up, sized by `F` on every row and the only money figure outside the Scale column that still is | `runInvestment`, `view.ts:605-607` |

`r` is the client's mirror of the server's `entryRate` and is bit-identical to
it: `Result.DivineChaosRate` is the newest hour's divine/chaos VWAP
(`result.DivineChaosRate = hourRate`, `plays.go:1026`) and every served play
cleared in that hour (the newest-hour cut, `plays.go:1128-1133`), so `chaosPerQuote(legs[0].quote, divineChaosRate)` is the
same float the server valued `Investment` at.

Legs are read BY POSITION, never by `action` — leg 3 is a `sell` on the wire and
a *convert* on screen. `u2` uses the sell form for that reason and states it.

**On a window-priced row `u0` and `u1` can be built from a different hour than the
`legs[i].tick` beside them, and the equations are unchanged.** Since POE-252 a leg
whose scored hour traded under `Config.ThinHourVolume` units carries one of the
extremes the market REALIZED over the trailing clock window, with the
`priceItemQty`/`priceQuoteQty` pair the row that printed it posted it at — a whole
posting from one row, never an average across the window's rows. `windowPriced`
says so and `windowHours` is the span. Which hour `tick` reads then depends on
which kind of window-priced leg it is:

- **hour-live** (the hour traded, just too little to print two sides): `tick`,
  `volume` and `stock` stay the SCORED hour's, deliberately — the step is what the
  market can express NOW, and that is the half of the row the window does not
  touch;
- **window-rescued** (no row that hour, or one under `Config.MinVolumePerHour`):
  there is no such hour, so `tick`, `volume` and `stock` all come from the newest
  CONTRIBUTING window row — the same row family as the price, and no older than
  it. The leg's item/quote ORIENTATION is not among them and never was: `orient`
  reads `span[0].Row` for `ItemA`/`ItemB` alone, and those two are the same on
  every row a market id carries, so any row of the span answers identically
  (`direct.go:241-249`).

`u0`, `u1` and `u2` are built from `legs[i].price` and `legs[i].tick` AS EMITTED,
so E1–E8 close whichever hour each of those came from. There is no suspension to
look for here, and a reader who goes looking should find the absence stated rather
than have to prove it: the window moves the hour a price belongs to, and that is
the whole of it.

One identity a reader may reach for is not one, and never was:
`legs[i].tick = 1 / max(priceItemQty, priceQuoteQty)` is **NOT a wire invariant on
any leg**. `tickOf` (`pricing.go`) takes the COARSER of the source row's TWO
quantity pairs — `max(1/max(lowestA, lowestB), 1/max(highestA, highestB))` — while
the pair beside a price is the pair THAT price was posted at, so a buy leg whose
market posted its high more coarsely than its low already disagreed with the
identity before POE-252. The window widens the ways the two can differ; it did not
open the gap. Nothing in §3 recomputes `tick` from the pair, which is why the gap
costs the reader nothing.

---

## 3. The invariant equations

Stated in CHAOS, which is the one unit every figure can be valued in. Each end
and each step is rendered in the currency named in §4; the rendering is a
division by `r` and never a second derivation.

```
E1  I  =  N · u0 · r                     (the row's cost at the undercut entry)
E2  buyStepTotal   ·  r  =  I           (buy step total = Spend = Investment)
E3  chainEnd       ·  r  =  I + R       (the mechanical end of the row)
E4  chainEnd·r − buyStepTotal·r  =  R   (the ROI column, by subtraction)
E5  get            ·  r  =  I + X       (Get = Spend + Exp. ROI)
E6  keep/lose line  =  |X|,  X  =  Exp. ROI column
E7  direct:  sellStepTotal = chainEnd
    1-hop:   sellStepTotal = chainEnd / u2   and   convert step prints
             sellStepTotal → chainEnd
E8  Scale column  =  ×F  →  +(play.expectedRoi · F)c,  "I_run c in"
    and  E8 ≠ E6's X  as a rule; they coincide only where a market's lot
    happens to equal F, which no rule arranges and nothing may rely on
```

**`N` on a window-priced row is still one postable order, just not the scored
hour's.** E1 counts `N = displayScale(play).units`, which reads the BUY leg's own
`priceItemQty` (§2) and goes on reading exactly that after POE-252. On a
window-priced buy leg that pair is the one the market posted the window's low at,
taken whole off the single row that printed it, so the row still names a quantity
somebody could place and §4.3's exactness claim survives — the posting is simply
not the scored hour's. How stale it may be is `windowHours`, on the row, beside
the mark.

**E6 and E8 are two different questions, and on every row they have two
different answers.** The Exp. ROI column and the Get slot's `keep ≈` line are
what ONE POSTING is measured to pay; the Scale column is what the WHOLE RUN
would pay and what it would tie up. Before the display scale moved they were one
number in three homes; now they are one number in two homes (the column and the
line) beside a third that answers the run. The Scale column is the row's only
disclosure of the run, which is why it is stated as an equation of its own
rather than left implicit.

`chainEnd` is the last mechanical total the row emits: the sell step's total on
a direct play, the convert step's right-hand amount on a 1-hop. E4 is Sebastian's
"sell-step total − buy-step total = ROI" in the form that also holds for a
triangle, where the sell total is denominated in the intermediate currency and
cannot be subtracted from the buy total at all.

**Why E7 derives the sell total backwards from `chainEnd` rather than forwards
from `legs[1].price`.** Because `roiPct` is the WIRE'S ANSWER to what the round
trip returns, and the client must not be able to disagree with it. `roiPct` is
served (`RoiPct: roiPct`, `plays.go:1360`), computed at
`roiPctOf(c.mode, undercut)` (`plays.go:1316`) from the undercut prices
`evaluate`'s leg loop builds (`plays.go:1248-1311`), `R` is built from it, and `chainEnd` is built from `R`.
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
(`roiPctOf`, `plays.go:1390-1399`), so `I·(1+roiPct) = N·u1·r` exactly on paper, while in
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
The old rationale (a `convertStep` comment this change deleted; the reasoning
now sits in `convertStep`'s doc, `view.ts:1867-1874`) — bare `price`, because the undercut is
already inside `expectedRoi` — was correct only while
the convert line's numerator was Get. Under E3 the numerator is `chainEnd`,
which is built from `R`, which is built from `roiPct`, which already contains
`u2`. Recovering the proceeds from `chainEnd` therefore requires dividing by
`u2`; dividing by the raw price would print a proceeds figure the wire's own
`roiPct` contradicts.

**The single scale rule, restated as code.** `N` comes from `displayScale(play)`
and the three money roots from `moneyColumns(play)`, which multiplies the wire's
per-exchange fields by that same `N`. Both are read, never recomputed: the route
ends, the step totals and the three money columns are one set of numbers because
they are one call.

Two figures on the surface are deliberately NOT that call, and each says so
where it lives:

- the **Scale column** (`CurrencyExchangePage.svelte`), which reads
  `worthwhileScale(play)` and is `F`-sized on every row (E8);
- the **Run cost bounds** (`applyNumericFilters`, `filters.ts:842-859`), which
  read `runInvestment(play)` — `I_run`, the Scale column's own "N c in" figure —
  because a bankroll ceiling is a run-sized question whatever the row prints
  (§6.1).

That second seam is the one this document most needs stated out loud, because it
is invisible in the code unless the bound names the run: `filters.ts` used to
read `moneyColumns(play).investment` and would have become a per-POSTING ceiling
the moment the display scale moved, silently, with every divine-row test still
green. It reads `runInvestment` instead, and the Run cost tooltip tells the
reader which cell carries the figure it compares against.

**The one exempt branch: an entry currency this response cannot value in chaos.**
`chaosPerQuote` answers `null` when the entry quote is divine and the response's
`divineChaosRate` is 0 (`chaosPerQuote`, `view.ts:1266-1270`). There is then no `r`, so E1–E4 and
E7 have no entry-currency rendering to be stated in and the row cannot carry a
mechanical chain at all. That branch renders both ends in CHAOS from
`moneyColumns` and prints the markets' own ratios on the steps. E5 survives
there and is the reason the branch is still a closed row: the ends are
`I` and `I + X` in chaos, so Get is still Spend plus the Exp. ROI column. E2,
E3, E4 and E7 are suspended, and the suspension is the branch's whole content.
It is unreachable on a served body — no divine-quoted play is served in an hour
that carried no divine/chaos trade (the newest-hour cut, `plays.go:1128-1133`;
`result.DivineChaosRate = hourRate`, `plays.go:1026`) — and it
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
3. **A step total carries `≈ `** — space included — because the PRICE is the
   undercut fill price and not the extreme the market printed: `buy 16 for ≈ 1.01
   div`, `buy 4 for ≈ 1c`. The item count before it is exact and carries no `≈`.
   Where the buy leg posted a usable pair that count is exactly one postable
   order; where it posted NONE (version skew, basis `single`) it is ONE item —
   the smallest claim the row can make that is still true, and not a quantity any
   market vouched for. The
   convert line keeps its single leading `≈`, which governs both of its amounts:
   `≈ 222c → 1.10 div`.
   The space after `≈` is deliberate and matches the two strings already shipped
   (`keep ≈ 102c`, `≈ 2.52 div → 526c`). Do not close it up.
4. **The ends carry no `≈`.** They never claimed to be orders. The
   approximation on the Get side is already carried by its profit line.
5. **The profit line's VERB follows the sign of `X`, and its amount is a
   magnitude.** `keep ≈ 102c` when the row is measured to gain, `lose ≈ 3c`
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
   the row's only disclosure of it: a line reading `sell 12 for ≈ 5c` on a market
   that posts five at a time asserts a quantity no single order can move. So the
   hover adds one clause whenever `N` is not a whole number of the market's lots
   — "this market posts in multiples of 5, so the 12 this row counts is 2 whole
   orders" — with the unsold-residue sentence when the division leaves a
   remainder on a SELL step ("2 of the 12 bought stay unsold"), and the
   smaller-than-one-lot sentence when `N` is under a single lot. That is the same
   pair of facts the snap's two hovers carried (both deleted with the snap),
   moved off the line and onto the hover with the pair — where they now live, in
   `marketPair` (`view.ts:1440-1481`).

   The clauses say "the N this row counts" and never "the run". `N` is the BUY
   market's posting and this same hover words the SELL market beside it, whose
   lot has no reason to match; calling that count a run would name a quantity no
   row prints. It is also why the buy step's hover never carries a LOT clause at
   all: `N` IS that market's lot, so the division is exact by construction and
   the smaller-than-one-lot and multiples clauses are reachable from the SELL
   step alone. They are kept on both, because a clause dropped by a future scale
   rule is a fact the reader loses silently.

   The BUY step's hover carries one clause the sell step's cannot, and it is the
   `N > F` disclosure of §1: when the posting counts past the worthwhile run, the
   hover says so — "this market posts 1,000 at a time, more than the ×167 run the
   Scale column sizes — one order is the smallest trade it accepts, so the row
   counts it whole". It belongs to the buy step alone because the sentence names
   the ENTRY order's own lot against the run, and the sell market's lot has no
   claim on that comparison. `marketPair` takes the run for that step only and
   `null` for the other, so the sentence cannot appear where it would be false.

   The clause is CURRENCY-AGNOSTIC and needs no divine wording of its own: it
   compares two counts and neither carries a unit. It became reachable on a
   divine row with the second ruling — while such a row counted its run, `N` and
   `F` were the same number and the sentence could never fire on one.

7. **A step with no total prints the market's ratio, and says so.** Three places
   have no total to print: the exempt branch of §3, a 1-hop convert step whose
   leg carries no usable price, and the sell step of a 1-hop whose sale could be
   priced from neither the convert leg nor its own. All three fall back to one
   shared helper that renders the leg's own posted pair (`buy 16 for 1 div`,
   `convert 1 div for 209c`), or the decimal rate when even the pair is unusable
   (`convert @ 0.00 c`). A fourth caller of that helper prints no line at all —
   the convert step's total hover quotes the market's posted pair inside its
   own sentence, from the same helper, so the hover and the fallback line cannot
   word one market's order two ways. Such a line prints the MARKET'S LOT
   QUANTITY and not `N`, which is the mixed-quantity reading the rest of
   this document exists to end — so it carries a hover that says the row's ends
   count the exchanges the row is priced for while this line counts one order of
   this market, and line emission is confined to those three places.

   The hover FOLLOWS THE BRANCH the line took. A line that fell all the way
   through to the decimal rate posted no pair, so its caveat names the per-unit
   rate rather than a quantity pair that is nowhere on screen; the
   count-versus-lot half of the sentence is unchanged, because the ends beside it
   count `N` either way.

**One wire field the row never renders, and one flag it does.** `legs[i].fair` —
the leg's volume-weighted anchor — reaches no emitter here: nothing in `view.ts`
or `ExchangeRoute.svelte` reads it, and no equation above is stated in it. It is
recorded in this section all the same, because it is what the engine computes
`suspect` against and `suspect` IS drawn on the row. On a window-priced leg `fair`
is not the scored hour's VWAP: it becomes the POOLED volume-weighted price of the
very rows that printed the window's extremes (`windowVwapOf`, read in `gatedLeg`),
so the band judges a window spread against that window's own traded mass rather
than against one hour's.

The consequence is visible and is not a defect: **a window-priced row can also
carry `suspect`, and does so more readily than an hour-priced one** — six hours of
extremes bracket more than one hour's do, so the very width that made the window
worth showing is what crosses the bands. The measured 17:00Z Apocalypse case pools to a fair near
737 chaos with a window spread of 486/1148, so both the buy leg (under
`fair·0.67`) and the sell leg (over `fair·1.5`) trip the bands. An hour-priced row
printing that same spread against that same fair is flagged identically, which is
the point — the two pricing paths read the same way, and a 2.4× spread genuinely
is wide. Both marks order nothing (ADR-018) and hide nothing (ADR-017).

---

## 5. The one permitted deviation: the expectation

`chainEnd` (E3) and `get` (E5) are different numbers. Their difference is
`X − R` — the measured expectation minus the hour's best case — and that gap is
the reason this table exists (ADR-016).

Measured across 960 top-20 play-hours that difference is negative and large: the
best case overstates the measurement by four to eight times
(`EXCHANGE_TOOLTIPS['Exp. ROI']`, `tooltips.ts:89`).
Nothing in the arithmetic guarantees the SIGN, though, and no emitter, no
formatter and no closure test may assume `chainEnd > get`. A row whose measured
expectation beats its hour's best case is a legal row, and the matrix carries
one so that assumption cannot be made by accident.

The deviation is permitted, bounded and labelled:

- It is ONE number, `X`, appearing in two homes, both reading
  `moneyColumns(play).expectedRoi`: the **Exp. ROI cell** and the **Get slot's
  `keep ≈ Xc` / `lose ≈ Xc`** line (§4.5 — the verb carries the sign, the amount
  is the magnitude). Two renderings of one variable, never two calculations.

  The **Scale column's `→ +Xc`** used to be a third home and is now a separate
  figure, `play.expectedRoi · F` (E8). No rule makes it equal `X` any more — it
  would take a market whose lot happened to be exactly `F` — and it is the row's
  only disclosure of what the run would pay. Reading the two as one number is
  the mistake this bullet now exists to prevent, and the closure suite asserts
  the DIVERGENCE on every case that has a run at all, rather than skipping the
  assertion.
- Nothing else on the row deviates. Every other printed figure is on the
  mechanical chain, at `N`.
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
and `low` (the Exp. ROI cell, `CurrencyExchangePage.svelte:784-809`). It is a genuinely smaller
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
(`.slot-get`, `ExchangeRoute.svelte:258-267`) — has to stay on the row and be sized for.

---

## 6. Re-affirmed exceptions

Each of these is OUTSIDE one of the three rules on purpose. Each states which
rule it is outside of and why.

### 6.1 The per-exchange gate knobs (outside the SCALE rule)

`filters.ts`, `applyGates` — `minItemPrice` vs `play.investment`,
`minItemPriceDiv` vs `play.investment / divineChaosRate`, `minRoiChaos` vs
`play.roi`, `minTurnover`, `maxTickPct`, `minEdgeTickRatio`, `minRoiPct` — all
read the wire's PER-EXCHANGE fields, never `N` and never `F`.

*Why exempt:* the armed levels are calibrated numbers, not free parameters. Each
tooltip names the level worth typing (`Min profit` "type 3", `Min turnover`
"type 10000", `Max price step` "type 10", `Edge vs step` "type 5", `Min return`
"type 2"), and every one of those is the floor the SERVER applied per exchange
before POE-191 handed the judgement to the reader. Retargeting them at `N`
would multiply each level by a per-row count: a "3c profit" floor would mean
3c-per-order on a market that posts one at a time and twelve times that on a
market that posts twelve, so the number in the box would no longer be a level at
all. The calibration is the reason, and it cannot survive a per-row multiplier.

The deeper reason is that a gate asks a different question. A gate asks whether
this MARKET is worth trading; the columns ask what one posting or one run of it
returns. So the split is not a seam to be closed but two questions with two
right answers, and the tooltips for `Min item price` and `Min profit`
(`tooltips.ts`) already say the uncomfortable half out loud: *no column prints
the figure this compares against*. That sentence is load-bearing and must not be
edited away — and the display-scale change narrowed rather than removed its
scope, on BOTH of those gates and by one rule. A row whose display scale is ONE
item prints the gates' own per-exchange figures: `Min item price` reads
`play.investment` and the Investment column is `play.investment · 1`, `Min
profit` reads `play.roi` and the ROI column is `play.roi · 1`. Two shapes land
there — a market that posts one item at a time, and a buy leg served without a
usable pair. On every other row the count is not 1 and neither figure is
anywhere on screen, which is the state the sentence was written for.

**`Min item price (div)` is the same shape with a different cell** (owner ruling,
2026-08-23). It compares `play.investment / divineChaosRate` — the same
per-exchange entry cost as its chaos twin, un-converted by the response's own
rate — on plays whose BUY LEG quotes in divine, and nothing else. Where it
lands on a `N = 1` row it is the ROUTE's **Spend** slot that prints it, not the
Investment column: `runLedger.spend = moneyColumns(play).investment / entryRate`
and the Investment column is always chaos (`{formatChaos(money.investment)}c`).
So the narrowing sentence holds for this knob too, and points at a different
cell than the two above it.

*Why the figure is `investment` and not the buy leg's own divine-denominated
`price`:* `price` is the hour's printed EXTREME and `investment` is the UNDERCUT
entry the row is priced at, one tick apart. The two knobs are one line drawn in
two currencies, so a pair judging two different price bases would answer a
boundary differently on the same row for no reader-visible reason. Rebuilding
the undercut from the leg (`price · (1 + tick)`) would avoid the rate but would
re-derive a market number, which `filters.ts` does not do.

**The exception's own exception:** the filter bar's **Run cost** bounds
(`applyNumericFilters`, `filters.ts:842-859`) DO read the run, because a bankroll
ceiling is a run-sized quantity — the reader is asking what they can afford to have tied up, not what
one order costs. That bound reads `runInvestment(play)` — `I_run`, the exact
figure the Scale column's "N c in" sub-line prints — and NOT
`moneyColumns(play).investment`, which since the display-scale change is what the
row DISPLAYS and is one posting on every row.

**That seam is explicit on purpose, and it is the one this section exists to
name.** The Investment column prints what one order costs while the bound judges
what the whole run ties up, so the figure the bound compares against lives in the
SCALE column and not in the column beside it — the same shape the per-exchange
gates above already carry, and the Run cost tooltip says which cell holds it.
Left reading `moneyColumns`, the bound would have become a per-posting ceiling
silently: the code compiles, nothing on screen changes, and a reader typing 500
to mean 500c of bankroll would be filtering on what one trash-market order
costs. The first ruling left divine rows agreeing with the column, so every
divine-row test stayed green through it; the second removed even that cover,
which is why the seam is now pinned on a divine fixture as well as a chaos one.

### 6.2 ROI% (outside the SCALE rule and outside the BASIS rule)

`play.roiPct` / `play.roiPctRaw`, rendered by the ROI% cell
(`CurrencyExchangePage.svelte:811-825`).

*Why exempt from SCALE:* it is a ratio. Whatever the row is sized at multiplies
numerator and denominator by the same `N`, so the per-exchange percentage, the
per-posting percentage and the per-run percentage are one number. It is not on
one scale; it is on all of them at once, which is the literal meaning of
scale-free. Tagging it with a scale would imply a distinction that does not
exist.

*Why exempt from BASIS:* it is the one figure on the row whose JOB is to compare
the two price bases. NET is the undercut round trip; RAW is the same round trip
at the hour's raw extremes; the gap between them is exactly what the fill steps
cost, and on a coarse market it is the whole spread. A single-basis ROI% would
delete the only reading that prices the undercut.

Neither number is what the table is ranked by (that is `ExpectedRoi`), and NET
is what the Gates row judges — both already stated at
`EXCHANGE_TOOLTIPS['ROI%']` (`tooltips.ts:86-87`).

### 6.3 Depth (outside SCALE and BASIS)

`play.depth`, rendered by the Depth cell (`CurrencyExchangePage.svelte:845-849`).

*Why exempt:* it is a MARKET reading, not a figure of the play — units per hour
that changed hands on the play's thinnest leg. It is the whole book's volume and
not the reader's share, it is a count of items rather than money, and there is no
undercut price of a volume. Scaling it by `N` would claim the market gets deeper
the more the reader wants to trade.

### 6.4 `Scale.hours` (outside the BASIS rule)

`worthwhileScale().hours = ceil(flips / depth)` (`worthwhileScale`, `view.ts:581`).

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
hours are **UNCONTESTED**. The retired word appears nowhere on the row surface:
`tooltips.ts`, `view.ts` and its tests, `filters.ts` and its tests, the page
caption, `ExchangeRoute.svelte` and `ExchangeFilterBar.svelte` are all swept to
zero hits. The one recorded exclusion is `api.ts`'s wire-type docs, which
mirror the Go wire docs and say so in place.

### 6.5 A basis note that is not an exception

`moneyColumns(play).roi = play.roi * units` (`view.ts:731-738`) has been read as
mixing bases — while `units` could be a flip count, a fill-simulation-derived
number multiplying a best-case per-exchange figure. It did not, and the reading
is now moot besides: `units` is a market's posted pair on every row. **A count
is a SCALE, not a BASIS.** `units` is a dimensionless count, whether it came
from a market's posted pair or from a flip count; the only price basis inside
`roi` is `roiPct`,
which is the undercut round trip and nothing else. The ROI and Exp. ROI columns
are on the SAME scale and on their own declared bases, which is exactly what §1
asks for. `WorthwhileScale` carries no best-case total and should not: it exists
to answer how far the MEASURED expectation has to be repeated.

### 6.6 The window-priced row (inside SCALE, inside BASIS, outside the single-hour reading)

`play.windowPriced` / `windowHours` / `windowVolume`, and the same three on each
leg (`internal/exchange/plays.go`), rendered as a sub-line of the DEPTH cell,
under the `low liquidity` sub-line and labelled with its span: `window 6h`. The
two marks share that cell; there is no flags cell on this table (POE-252,
2026-09-04).

*What deviates:* the HOUR a leg's price came from, and nothing else on the row. An
hour that traded under `Config.ThinHourVolume` (2) units cannot print a spread —
one trade collapses the low and the high onto the same number, and the row then
reads −0% however long two sides have stood in the game's book — so such a leg is
priced from the extremes the market REALIZED over the last
`Config.WindowPriceHours` (6) CLOCK hours, needing `Config.MinWindowVolume` (2)
units summed across the rows that priced. Each extreme is one row's whole realized
print with its own posted pair. **It is not a blend**: no average, no median and
no clamp is taken over the window, because a blended price is a trade nobody could
have made — the reason the single-hour doctrine exists at all (Mawr Blaidd/Chaos,
POE-188).

Since the same task the window also carries LIVENESS. A market that published no
row in the scored hour, or one that traded under `Config.MinVolumePerHour`, is
still enumerated and still served, priced window-only and marked (window-RESCUED).
The measured pair published no row at all in nine of twenty-six clock hours, and
ten of the SEVENTEEN shifts the acceptance test scores land on an hour the market
either did not publish or published untraded — before this, every one of those
ten served nothing. That is the flicker the task was filed on
(`TestCorpus_apocalypseWindow_isServedInAllSeventeenShifts`,
`internal/exchange/corpus_test.go:1300`).

*Why it belongs here rather than in §5:* §5's deviation is a cross-hour
STATISTIC that is never a price. This one IS a price — it just belongs to a
different hour than the row was scored in. The doctrine it bends is the one
ADR-016 wrote down and owns, and that ADR's 2026-09-04 amendment is where the
decision and its bounds are recorded. This section states only what it means for
THIS row.

*What still holds:*

- **SCALE.** `N` is still `displayScale(play).units`, still the buy market's own
  posted pair, still one order (§3's note under E1). Every money figure on the row
  still counts the same `N`.
- **BASIS.** Every mechanical number is still priced at the UNDERCUT fill prices
  built from the leg's emitted `price` and `tick`. Window pricing changes the
  input, never the basis.
- **CLOSURE.** E1–E8 hold unchanged and are asserted on such a row by the desktop
  corpus incident block named in §7. A window price must close like any other.
- **`tick`, `lastHour` and the chaos rate are the SCORED hour's** on every leg that
  had a live hour of its own, so what the market can express now is read from now.
  On a RESCUED leg they come from the newest contributing window row instead (§2),
  which is also what keeps `depth` and `turnover` positive on such a row rather
  than reporting no depth on exactly the rows this exists to surface. A `depth` of
  0 would cost the Scale column its `hours` sub-line (§6.4 — the division has no
  denominator), and a `turnover` of 0 would put the row under any armed
  `minTurnover` gate.
- **`hoursSeen` still counts hours the recipe was priced on its OWN prices.** A
  rescued hour never counts, and it records no fill-simulation entry either, so
  `expectedRoi` and `hoursSeen` on every pre-POE-252 row are untouched.
- **It marks; it does not order and it does not hide.** `windowPriced` is in
  neither comparator — not the server's ranking and not `sortPlays` (ADR-018) — and
  no default gate in `filters.ts` reads it (ADR-017, ADR-015). Such a row's ROI and
  Exp. ROI sit wherever their own numbers put them.

*What the reader gives up:* freshness, bounded and disclosed. A spread that closed
five hours ago can still be shown. `windowHours` is the span behind the price and
`windowVolume` the mass its extremes were drawn from; both are on the row, which
is the whole of how the reader knows.

---

## 7. Enforcement

The equations of §3 and the rendering rules of §4 are asserted by a **closure
suite** in `desktop/src/lib/exchange/view.test.ts`. **Eleven cases**, ten of them
running the shared equation battery and one (F7) asserting a suspension instead:

- **F1–F4** — all four of `{direct, 1-hop} × {chaos entry, divine entry}` with a
  worthwhile run. The two divine cases post SIXTEEN for a divine, so they price
  that posting and carry a 50-flip run in the Scale column alone.
- **F5, F6** — two of those four repeated with NO run, one of each shape and one
  of each entry currency; the remaining two vary nothing these do not. Their
  COUNTS are unchanged from F1 and F4, which is the point: a market's lot does
  not depend on the measurement, so losing the run empties the Scale column and
  moves nothing else. F6 is the divine-entry LOSS — a `lose ≈ 80c (≈ 0.40 div)`
  profit line and a Get printing below its Spend in divine, neither of which a
  chaos case can reach.
- **F7** — the exempt branch of §3, which asserts the SUSPENSION of E2/E3/E4/E7
  and the survival of E5 rather than the equations. It is described on its own
  and does not run the battery.
- **F8** — the posting that is neither one item nor the run: a market that posts
  TWELVE whose sell market posts five, so the row carries a count no other case
  can produce and a lot that cannot divide it.
- **F9** — the `N > F` row of §1: a market that posts a THOUSAND at a time on a
  play whose worthwhile run is 167 exchanges. It is the only case in the matrix
  where the posting counts past the run, so it is the only one that can pin the
  buy step's overshoot clause and the Exp. ROI column standing ABOVE the Scale
  column's own gain. (The clause's reachability on a DIVINE entry is pinned
  outside the matrix, in the `routeSlots` suite, because the clause is
  currency-agnostic and needs no second equation battery to say so.)
- **F10** — the `single` basis: a buy leg served with no usable pair, so the row
  counts ONE item without a market having vouched for it (§4.3). Its equations
  run at `N = 1`, which no other case in the matrix reaches now that the divine
  no-run row keeps its posting.
- **F11** — the trash tier, whose money columns ROUND AWAY: a sub-chaos market
  posting four for a chaos, where the ROI and Exp. ROI columns both print `0`
  while the row is still drawn as a gain. That rendering is specified, not a bug —
  the columns count whole orbs (POE-189) and the measurement is positive; the
  shipped 0.5c `minItemPrice` floor is what keeps most of the tier off the table
  by default.

The suite parses the EMITTED strings and values — not intermediate state —
because the reader compares printed numbers.

Every case carries `units` and `basis` as LITERALS and pins them before it reads
anything else, so a case cannot assert its own arithmetic at whatever size the
code happened to choose. The T3 cross-checks multiply that literal and never
`ledger.units`, for the same reason.

Four assertion tiers, with the tolerance rule for each:

- **T1 — string identity (exact).** Two emitted strings that must show the same
  number are produced from one variable through one formatter, so they are
  compared with `toBe` on the whole string, with no tolerance. Covers:
  - the buy step's total against the Spend amount. The step line carries a unit
    and the end does not (`RouteEnd.amount` is bare, its unit word being a
    separate span — the `end` snippet, `ExchangeRoute.svelte:177-180`), so the pin is two
    assertions and not one: the line ENDS WITH `for ≈ ${withOrbUnit(spend,
    entryUnitShort)}`, and the numeric head of that tail is character-identical
    to `route.spend.amount`. Either one alone can pass while the two numbers
    differ.
  - 1-hop only: the sell step's total against the convert line's left amount,
    same two-part shape.
  - the Get slot's profit line against the Exp. ROI cell's value — including its
    VERB, which is `keep` for `X > 0` and `lose` for `X < 0` (§4.5), so the pin
    is sign-aware and runs on the losing fixtures too.
  - the Scale column's `→ +Xc` against the Exp. ROI cell, asserted as a
    DIVERGENCE and never as an identity: the Scale column prints the run's gain,
    the Exp. ROI column prints one posting's, and the two must not be equal.
    That is the pin that dies if the money columns are ever re-multiplied by the
    flip count, and it is stated in the ruling's own words — ROI is per single
    trade, not per batch. It runs on every case that HAS a run (the divine
    entries included, which is the second ruling's whole content) and is skipped
    where `worthwhileScale` answers `null`, since the expression has nothing to
    read there. A CASE-level guard asserts the two literals differ, so a future
    edit that set them equal fails rather than passing on a fixture that pins
    nothing.
- **T2 — ledger identity (exact).** `chainEndChaos === I + R` and
  `getChaos === I + X` are single float additions of the ledger's own roots and
  are compared with `toBe`. This is what "closure by construction" means
  mechanically: there is no second expression to drift. `I` is pinned by
  RE-DERIVING it — `investmentChaos === play.investment * N`, with `N` the
  case's literal — and not by comparing it to `moneyColumns(play).investment`,
  which is the expression the field is assigned from and would assert nothing.
- **T3 — cross-check against the wire and against hand-worked arithmetic
  (relative tolerance 1e-9).** The entry-currency cost `spend ≈ N·u0`, the
  §5 deviation `chainEndChaos − getChaos ≈ R − X`, and the forward derivations
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
(`roiPctOf`, `plays.go:1390-1399`), and `roi` = `investment · roiPct`. A fixture that
contradicts those identities can make a closure test pass or fail for a reason
that has nothing to do with the code. Fixture INPUTS may be derived from the leg
prices in the test file, with the hand-worked value in a comment; EXPECTED
values are always literals.

The rule got easier to keep, and one deliberate violation was retired with the
display-scale change. The suite used to carry a fixture whose sell leg
contradicted its own money fields, because holding a count of 12 against a lot
of 5 required it: both the count and the lot were derived from the same
expectation. Under the display scale the count comes from the BUY market's pair
and the lot from the SELL market's, so the two are independent by construction —
F8 carries that case wire-consistently and the contradiction is gone.

The rule binds the `routeSlots` fixtures too, and not only the closure matrix's
own. Those fixtures pin EMITTED STRINGS, and once every string on the row is
read out of `investment`, `roi` and the legs together, a fixture whose
`investment` contradicts its buy leg's price pins a row no server could send.
There is one fixture set, wire-consistent, and the closure matrix is built from
it by overriding `expectedRoi`.

A case that needs a different POSTING has to override the legs, and the rule
then binds the override: since `N` is now read off the buy leg's pair, changing
that pair changes the row's whole arithmetic, so `investment`, `roi` and
`roiPct` are re-derived with it and the hand-worked derivation goes in the
case's comment. Four cases in the `routeSlots` and `runLedger` suites do that —
the hundredths-above-a-hundred end, the thousands grouping, the read-not-recompute
ulp check, and the divine overshoot — and each one states its `u0`, `u1` and
back-solved `roiPct` above the fixture. An override that changed only the pair
would pin a row no server could send, and would pin it at the very size the case
is about.

**The window fields (§6.6) are enforced on both tiers, and are named here so the
enforcement is findable from this document rather than only from the code.**

- **The wire type.** `windowPriced`, `windowHours` and `windowVolume` are declared
  NON-OPTIONAL on `WireLeg` and `WirePlay` in
  `desktop/src/lib/exchange/corpus.test.ts`, so a field disappearing from the
  engine is a type error on the cross-layer tier rather than a silently absent
  mark.
- **The desktop incident block** `incident — Apocalypse card, thin newest hour
  priced from the window (POE-252)` (`corpus.test.ts`) reads the market out of the
  regenerated `recent.json` golden and asserts the mark, the six-hour span, the
  rendered buy and sell prices against the window's own low and high, `suspect`
  (§4's closing note, the accepted reading), and the full §3 equation battery — the
  pin that a window price closes like any other row. The same file's visibility
  predicate lists `windowPriced` among the flags no default gate may hide.
- **The Go tier**, in `internal/exchange`:
  `TestCorpus_apocalypseThinNewestHour_isPricedFromTheWindow` (the incident),
  `TestCorpus_apocalypseThickNewestHour_isPricedFromTheHourAndUnmarked` and
  `TestCorpus_thickMarkets_areNeverWindowPriced` (the POE-220 backtest: no
  pre-existing key window-prices),
  `TestCorpus_apocalypseWindow_isServedInAllSeventeenShifts` with
  `TestCorpus_apocalypseWindowShifts_markTheThinHoursAndOnlyThose` (the no-flicker
  acceptance, shift by shift),
  `TestCorpus_deadWindow_isNotServedInEitherHorizon` (the window is not a
  resurrection machine),
  `TestCorpus_windowRescuedHours_recordNoSimulationEntry` and
  `TestCorpus_livenessRelaxation_movesNoPreExistingValue` (the two counters that
  keep `expectedRoi` and `hoursSeen` still), and
  `TestBestPlays_windowPricedMark_doesNotOrderTheServedList` (ADR-018).

---

## 8. Out of scope for this document

Gate semantics and levels; the server's pricing and simulation arithmetic; the
Exp. ROI column's non-monotonicity in the served order (`sortPlays`'s doc,
`view.ts:1008-1016`, a seam by choice); the Gold column; the gem-flip ROI domain, which shares the
words and means percentage points (`METRIC_TOOLTIPS`, `tooltips.ts:50-53`).