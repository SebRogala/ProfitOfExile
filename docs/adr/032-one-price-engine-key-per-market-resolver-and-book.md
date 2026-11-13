---
uid: 2975e9d8-38b2-463e-85bf-a9a95531f6b4
---

# ADR-032: One Price Engine: a Key per Market, a Pure Resolver, and a Book

## Status

Accepted 2026-11-13 (POE-307; epic POE-295). Last verified 2026-11-13.

Supersedes [ADR-027](027-trade-is-the-primary-gem-price-poe-ninja-is-the-fallback.md)
Decision 1 (one shared loader) and Decision 3 (trade usable for 24 h), and the
freshness weights of [ADR-007](007-v3-hybrid-analysis-unified-pipeline-with-trade-enriched-features.md),
for every consumer that reads the book. Until POE-310 moves the gem analyses
onto the book, nothing reads it, today's pricing path is unchanged, and
ADR-027 stays Accepted; POE-310 marks it partially superseded.

## Enforcement

Tier: required

Scope: `internal/price/resolve`, `internal/price/book`, the observation
adapters (`internal/trade/observation.go`, `internal/lab/price_observation.go`,
`internal/lab/price_policy.go`), the divine rate (`internal/exchange/divine_rate.go`),
and every future reader of a resolved gem or currency price.

Summary: Every market has one key; one pure resolver turns native
observations into one observed price with source, age and reasons; one
per-league in-memory book holds the resolved prices and is read without I/O.
The divine rate is the exchange newest-hour VWAP, poe.ninja only while the
exchange has none.

## Context

Gem prices were resolved in several places: the analyses read the poe.ninja
snapshot, trade data adjusted sellability through freshness weights
(ADR-007), and ADR-027 planned a shared loader where trade expires after
24 h. Each consumer normalized variants, converted divine listings and
picked a source on its own, so two screens could price one gem differently,
and a stored chaos value carried the divine rate of the hour it was fetched.
POE-295 locks the rules (L-numbers below refer to its locks); POE-307 builds
the engine without a consumer, and POE-308 makes divine one of its keys.

## Decision

### One key per market

`resolve.Key{League, Kind, Name, Variant, Corrupted}` (`internal/price/resolve/key.go`)
is comparable and is the only market identity. `GemKey` strips a trailing
`c` into `Corrupted` and calls `NormalizeVariant` (`1/0` → `1`, `20/0` →
`20`; moved here from the handlers, which now call it). `CurrencyKey(league,
"divine")` is not a gem key. Leagues are different markets (L17).

### The resolver is a pure leaf

`internal/price/resolve` imports only the standard library, guarded by
`boundary_test.go` as in ADR-026. `Resolve(obs, asOf, rate, policy)`:

- considers only observations at or before `asOf`, newest per source;
- a trade observation with at least one convertible listing wins, however
  old (L10); its price is `MedianCheapest` of the cheapest
  `Policy.ListingCount` (3) listings, carrying ADR-027 Decision 2 and its
  2026-10-23 amendment;
- otherwise poe.ninja prices the key, with `never-checked` when no trade
  observation exists, or `no-divine-rate` when trade exists but none of its
  listings converts; `thin-sample` when the ninja sample is at or below
  `Policy.ThinSample`;
- `disagreement` flags a trade price whose max/min ratio to ninja exceeds
  k = 3×, or 2× when the ninja sample is thin (POE-310 decision 2026-10-25);
- `stale-evidence` flags trade older than `Policy.MaxRevisitAge` (0 = off
  until POE-309 sets it); the trade price still stands;
- the price is the observed price, never scaled by age or confidence (L9).

ADR-027 Decision 4 carries forward as amended by POE-295 L4: a
never-checked gem is priced from poe.ninja, keeps its confidence flags
(`never-checked`, and `thin-sample` when thin), and stays eligible for TOP
detection, tier boundaries and Font EV, so a league wipe does not empty them.
This is the rule for the consumer switch (POE-310); today's analyzers still
follow ADR-027 Decision 4 unchanged.

Observations carry native amounts only (`Money{Amount, Currency}`), never a
converted chaos value (L13). Divine converts at the `Rate` passed at resolve
time, rounded to one decimal like `trade.NormalizeToChaos`; divine without a
positive rate, and any other currency, is not convertible.

`internal/trade/testdata/trade-price-vectors.json` is the one median
definition: Go trade, the resolver and Rust `signals.rs` all test against it,
including the even-count rounding to two decimals.

### Adapters own their stores

- Trade (`internal/trade/observation.go`, `Repository.ObservationsAsOf`): the
  newest stored lookup per gem and variant at or before T, no age cap; native
  listings filtered by `ListingsInMarket`. The stored converted columns are
  not read.
- poe.ninja (`internal/lab/price_observation.go`,
  `Repository.GemObservationsAsOf`): the newest gem snapshot at or before T,
  one chaos price per line with its sample count; corruption from
  `IsCorrupted`; zero-priced lines are no observation.
- Server-gate and desktop-submitted lookups carry the same trust (ADR-027
  Decision 5; L7, L15). Everything is per variant (ADR-027 Decision 6).

`ListingsInMarket` mirrors `buildSearchQuery`'s market definition: the
corruption state must match, the level is pinned only at 20 and above, and
the quality only at exactly 20. A change to the search's market must change
the filter. This is the interim rule (POE-295 Q6); the final call comes with
the trade-link audit.

### The divine rate

The app-wide divine→chaos rate is the exchange newest-hour VWAP; poe.ninja
serves only while the exchange has none (Q1, Operator ruling 2026-11-13).
`DivineRateReader.Current` and `DivineRateAsOf` share one rule set,
`chooseDivineRate`: the newest hour's divine VWAP while the exchange is warm
and that hour traded divine, else the ninja snapshot, else no rate. An
hour without a divine trade serves ninja, not an older hour's VWAP.
`DivineRateAsOf` gives the answer `Current` would have given at T from rows
anchored on `Repository.NewestHourBefore`, with ninja from
`lab.Repository.NinjaDivinePriceAsOf`. The `currency_id = 'divine'` literal
stays at one site in `internal/lab/repository.go`. No production code calls
the as-of functions yet.

POE-308 (WI-8) makes divine an engine key: the book answers
`CurrencyKey(league, "divine")` with the reader's rate, and the rate is
pushed into the book after each refresh, guarded so that the reader is the
only divine→chaos source. Until it lands the book takes the rate only at
`Seed` and through `SetRate`, which has no production caller.

### The book is the service

`internal/price/book` imports only the standard library, `internal/league`
and the resolver, guarded by its `boundary_test.go`; it never imports lab,
server or cmd. A `Book` is bound to one league scope and one policy
(`lab.GemPricePolicy()`, which reuses the lab's thin-sample count). Its ports
are `TradeObservations`, `NinjaGemObservations` and `DivineRate`.

- `Seed(ctx, now)` reads each port once, resolves every key at the current
  rate and publishes an immutable snapshot; on error the previous snapshot
  stays.
- `Get(key)` is a pointer load and a map read; it never calls a port, for a
  seeded or a missing key (L16).
- `Observe(obs)` re-resolves that key only, copy-on-write; observations of
  another league are refused (L17). POE-311 is its first caller.
- `SetRate(rate)` re-resolves every key from held observations and publishes
  one snapshot carrying the rate, with no I/O.

`cmd/server` constructs one book per process (the server holds one fenced
league scope) and seeds it in `seedCaches` after the divine fallback
refresh; a seed failure is logged, never fatal. No handler, analyzer or
router receives the book.

### Fixed cutoffs are owner-locked

The listing count N = 3, the thin sample count ≤ 5 and k = 3× / 2× are
fixed values the owner locked. They are accepted exceptions to the AGENTS.md rule
"Absolute cutoffs on price or listing count are suspect" and live in caller-owned policy, not in
the resolver.

## Consequences

- Served output does not change until POE-310 switches the analyses to the
  book; the engine runs beside today's path.
- A trade price never expires; staleness is a reason, and revisiting old
  trade evidence is the scheduler's job (POE-309), not the resolver's.
- A divine-listed gem's chaos price moves with the rate without a new
  lookup.
- The market filter is coupled to the trade search's query builder.
- Desktop's Rust `signals.rs` still medians unfiltered listings until
  POE-313.

## Alternatives rejected

- The ADR-027 shared loader with a 24 h trade window: an old trade price is
  still better evidence than a thin ninja line, and a time cut-off flips the
  source without new evidence.
- Freshness or confidence weights on the price (ADR-007): they produce a
  price nobody observed.
- Storing converted chaos values: they freeze the divine rate of the fetch
  hour.
- A pooled-window divine VWAP: the code and the Operator ruling use the
  newest hour.
- Exact level and quality equality as the market filter: it would change
  every `1` and `20` price; the interim rule keeps today's output.
