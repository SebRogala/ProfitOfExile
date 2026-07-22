# Gem Market Findings — March 2026

> **Status: Dated, non-reproducible research record.** These observations were
> added in commit `f736797` after a stated seven-day research session. The raw
> queries, dataset snapshot, and full report are not present in this repository,
> so the figures preserve historical project knowledge but cannot be independently
> reproduced here. Do not use them as current market truth without rerunning the
> analysis against a named league and time window.

## Recorded observations

- 64% of gems stayed within ±2% over two hours.
- The recorded analysis described price-change autocorrelation as zero and the
  RISING signal as 29% directionally accurate. Those conclusions apply only to
  the sampled dataset; “zero” and “fundamentally unreliable” should not be
  generalized beyond it.
- Listing count was reported as more useful for sellability than price direction.
- Gems above 300 chaos were reported to decay around 1% per four hours during
  the sampled mid-league period.
- A listing increase above 30% in one hour was followed by a median 2.1% price
  decrease over the next four hours.
- A listing decrease above 20% over three hours was followed by an average 17.8%
  price increase in the recorded sample.
- TOP-tier gems were reported to move more meaningfully at a four-hour horizon:
  70% were non-flat, compared with weaker two-hour movement.
- LOW-tier gems below roughly 30 chaos were reported as 77% flat.
- 05:00–12:00 UTC was recorded as the most bearish interval, with direction bias
  from -0.075 to -0.104. Activity and listing velocity were highest from
  14:00–19:00 UTC.
- Weekend volatility was reported as higher than weekday volatility; the older
  Friday/Saturday price-rise narrative remained explicitly unvalidated.

## Recorded tier and pool descriptions

The original profiles described TOP as 1–4 gems above about 600 chaos, HIGH as
5–15 gems around 300–600 chaos, MID as roughly 30–300 chaos, and LOW below 30
chaos. These were dataset descriptions, not stable tier constants; current code
derives tier boundaries from market context.

Historical transfigured-gem pool estimates were approximately 35 red, 75 green,
and 87 blue gems. Current code derives unique pools dynamically, so these counts
must not be hard-coded.

## Flame Golem case

The profiles record a user listing Flame Golem of Hordes at 350 chaos when only
two listings were visible. Supply reportedly grew to 28 listings within hours
and the observed price fell to 60 chaos; the estimated quick-sale value at the
start was around 180 chaos. Preserve this as a thin-market case study, not as a
general pricing formula.

## Historical planning assumptions

Earlier evaluator profiles used assumptions such as Gift costing three Divine
Orbs, fixed Font uses by lab tier, fixed quality caps, fixed run-time bands,
quick-sale discounts of 15–20%, and simple sell-probability/stability brackets.
The repository does not establish these as current game facts, and current risk
and undercut code uses different piecewise calculations. Verify them externally
before scenario analysis.
