# The In-Memory Analysis Cache

**Status:** Current  
**Last verified:** 2026-08-04  
**Canonical for:** How `lab.Cache` is populated, served, and safely written, including the sparkline series cache.

`internal/lab.Cache` holds pre-computed analysis results in memory so the HTTP
handlers can serve without querying Postgres on the request path. This document
records the process topology, the tick chain that fills the cache, the
concurrency contract every field must honour, and the specifics of the sparkline
series cache.

## Process topology

The collector and the server are separate binaries: `cmd/collector` and
`cmd/server`, run as separate containers against the same database
(`docker-compose.yml`). The collector never imports `internal/lab` and never
touches `lab.Cache`. Nothing the collector does writes to the cache directly.

Anything described below as "populated on the tick" happens inside the **server**
process, reacting to a Mercure notification the collector published.

## The tick chain

1. The collector fetches poe.ninja, inserts rows via `InsertGemSnapshots`, and
   publishes a Mercure event on `poe/collector/gems`
   (`internal/collector/scheduler.go`).
2. The server's Mercure subscriber callback (`cmd/server/main.go`) validates the
   event's league stamp and revision through `LeagueEventGuard.AcceptRaw`
   (`internal/server/league_events.go`). A mismatched event is dropped.
3. On the `ninja_gems` endpoint the callback starts `RunTransfigure` and
   `RunQuality` in their own goroutines, and — in a third goroutine — runs
   `RunV2`, then `RunFont`, then `RunDedication` **serially**. The order is
   load-bearing: font and dedication read `GemFeatures` from the cache, so
   running them concurrently with `RunV2` makes them read the previous cycle's
   tier classification.
4. Each `Run*` queries the database itself, computes, persists to Postgres, and
   only then — guarded by `if a.cache != nil` — calls
   `a.cache.For(scope).SetX(...)`.

Each analysis kind has its own mutex on `Analyzer` (`muV2`, `muFont`, …), so
overlapping ticks queue rather than pile up.

### `RunV2` runs twice per snapshot

After every `ninja_gems` event the server also schedules a **T+15-minute delayed
recompute** that calls `RunV2` again on the same snapshot, to pick up trade data
accumulated since. A new gem event stops and reschedules the pending timer, so
there is at most one outstanding timer.

Any cache population wired into `RunV2` must therefore be **idempotent**, not
merely monotonic: a second pass over an unchanged snapshot has to be a no-op,
not a duplicate append.

### `league.DataLockKey` skips, it does not serialize

The delayed recompute takes `league.DataLockKey` via
`league.AcquireProcessLock` (`internal/league/advisory.go`), which is
**non-blocking**. On contention — another server instance recomputing the same
dataset — the callback logs a skip and returns. It does not wait for the lock.
Do not assume a queued second run will eventually happen; assume the tick was
dropped. (The immediate `RunV2` paths are not yet under this lock; see the
`TODO(POE-118)` in `cmd/server/main.go`.)

## Cache tenancy

One `Cache` instance serves exactly one league for the whole process lifetime.
`scope` is set at construction in `NewCache(scope)` from the process-active
league and never changes.

`For(scope)` is **not a lookup — it is a tenancy assertion.** It compares
`scope.ID()` against the bound league and **panics** on a mismatch, then returns
the same receiver. This is deliberate: handlers read cache-first and only fall
through to the league-scoped repository on a miss, so a cache silently serving
one league's rows under another league's scope would be an undetectable data
leak. See ADR-009.

Every read and write goes through `For`. There is no unscoped accessor.

## The immutability contract

A single `sync.RWMutex` guards every field. There is no per-field locking.

Readers take the **slice or map header** under `RLock` and release the lock
before scanning it. Stored data is therefore treated as **immutable once
assigned** — a reader may still be walking a slice long after the lock is gone.

Consequences, all of them mandatory for new fields:

- Never mutate a stored slice or map in place. Not `copy(s, s[n:])`, not
  `delete(m, k)`, not element assignment. The textbook in-place trim idiom is a
  genuine `-race` failure here.
- Never store a reslice of a larger array when only the tail is wanted; the
  reslice pins the whole backing array. Allocate and copy.
- Replace wholesale. Every `Set*` is a plain assignment under the write lock;
  the old backing array becomes garbage on the next write.

## The write discipline

1. Compute the replacement value **outside** the lock — including sorting, map
   building, and any allocation. `BuildDedicationCorpus` builds its maps before
   `SetDedication` takes the lock; the sparkline population merges everything
   before assigning.
2. Take the write lock once and assign.
3. **Never hold the lock across a query or any other slow work.** Read what is
   needed out under `RLock`, release, do the work, then take the write lock for
   the assignment.

Existing precedent for building fresh rather than compacting in place:
`purgeExpired` in `internal/trade/ratelimiter.go`.

### Coherence

Fields written by the same run — `marketContext`, `gemFeatures`, `gemSignals`,
and the sparkline maps — each take the lock separately, so a reader can observe
one updated and another not yet. This millisecond-wide window against a
minutes-long tick interval is a pre-existing, accepted trade-off. New fields
inherit it; no additional mitigation is expected.

## The cold-start window

The startup warm-up goroutines in `cmd/server/main.go` (seed `nextFetch` and
`divineRate`, then `RecomputeLatestV2` → `RunFont` → `RunDedication`, plus
`RunTransfigure` and `RunQuality`) **do not block `ListenAndServe`**. After a
deploy the server accepts HTTP traffic against a cold cache and keeps serving
while the warm-up runs.

Every handler must therefore treat an empty cache as normal, not exceptional.

## The cache-first, database-fallback pattern

Handlers read the cache, ask it whether it is warm, and fall through to the
repository only when it is not.

**The cache-state contract is normative in code, at the top of
`internal/lab/cache.go`** — COLD versus WARM-AND-EMPTY, the rule that a handler
never infers warmth from a value it filtered, which accessor shape each
addressing mode takes, and the checklist for adding a field. Read it there
before adding or reading a cache field; this document deliberately does not
restate it, because two normative homes is how the rule drifted in the first
place (POE-158).

## The sparkline cache

Files: `internal/lab/sparkline_cache.go` (merge and population),
`internal/lab/cache.go` (fields and accessors), `internal/lab/repository.go`
(`SparklineWindow`), `internal/server/handlers/collective.go` and
`internal/server/handlers/analysis.go` (read paths).

Four fields: `sparklines` and `sparklinesCorrupted`, both
`map[sparklineKey][]SparklinePoint` keyed by gem name plus variant,
`sparklineHighWater`, and `sparklinesSet`. Prices for different variants are
different markets and are never merged into one series.

One `SparklineWindow` read fills both maps, so `sparklinesSet` is the COLD/WARM
discriminator for the pair and `HasSparklines` is the single predicate over it —
including per variant, because the read's own filter is the variant allowlist
the handlers admit. The cache-state rule that predicate answers to lives at the
top of `internal/lab/cache.go`.

### Window and tail

- `SparklineWindowHours = 12` — the rolling window served to consumers.
- `SparklineTailPoints = 4` — the minimum a series keeps. A gem that stops
  appearing in snapshots would otherwise decay to an empty sparkline inside the
  window; the tail leaves its last known shape visible.
- `sparklineMaxLookbackHours = 168` — how far back the tail may reach. Past that
  age, showing nothing beats showing a flat line from last week. The 168-hour
  trend consumer is served from the same cache, and each consumer trims at
  request time.

These three travel together as `SparklineBounds`, built by `sparklineCacheBounds()`
and passed to the read. Binding them into one struct is what keeps the rows
fetched matched to the rows retained — see the cold read below.

### Population and the high-water mark

`populateSparklineCache` runs at the end of `RunV2`, after `SetGemSignals`,
guarded by `a.cache != nil`. A failure is **logged, not fatal**: handlers fall
back to `gem_snapshots`, and the analysis output is already persisted.

The read is incremental. `SparklineWindow(ctx, scope, since, bounds)` fetches only
rows newer than the cached high-water mark. Series that received no incoming
points are still re-merged, so points aging out of the rolling window are
trimmed.

The read shape follows the mark and nothing else. A zero mark — no row has ever
been observed — passes a zero `since` and takes the cold read. Any other mark
takes the incremental read, **including when both maps are empty**: a series is
dropped only once its every point is older than the lookback, so empty maps imply
a mark at least that old, and the incremental read is bounded by that same
lookback. It returns everything a cold read would keep, so a decayed series is
rebuilt as soon as its gem is priced again.

Deciding from map contents instead is the warmth-from-contents defect on the
writer side, and it made a retained-nothing tick re-run the cold union forever
(POE-161).

The cold read does **not** read the full 168-hour lookback — doing so would
materialise roughly fourteen times the rows the merge retains, at process start,
exactly when `RunV2` is already holding its own history load. Instead it runs a
bounded union: the full 12-hour window, plus a lateral tail query taking the last
`SparklineTailPoints` rows per series within the lookback. Both halves share one
row predicate, so the variant allowlist and the corruption split cannot drift
apart between them.

The mark advances **only to the newest timestamp actually observed among incoming
points**, never to `now`. An empty read leaves it where it was.

Population deliberately does **not** reuse the history `RunV2` already holds:
`GemPriceHistoryByVariant` filters `chaos > 5`, excludes Trarthus names, and
applies its own variant allowlist, none of which the previous sparkline path
applied. Reusing it would silently drop points the database path returned.
`SparklineWindow` filters only on the variant allowlist (`1`, `1/20`, `20`,
`20/20` for non-corrupted; the Dedication variants `21/23c` and `21/20c` for
corrupted) and `is_corrupted`, which
closes the key space without changing series content. POE-134 may change that
`chaos > 5` floor; the sparkline path is unaffected either way.

Sparklines always use raw prices. Never source them from normalized history —
temporal normalization was removed from this path for creating edge artifacts.

## The seed-once fields

Two fields cannot be filled from one tick's output alone, so their population
step reads the database on its **first pass only** and extends itself for free
afterwards. Both live at the end of `RunV2` beside `populateSparklineCache` and
follow its shape: warmth is checked first, and the read is skipped once warm.

- **`signalHistory`** (`internal/lab/signal_history_cache.go`) — a bounded
  per-`(name, variant)` ring of `SignalHistoryDepth` transitions, built from the
  `signals` and `features` the tick already holds. Appending one point per tick
  would take ~10 hours to fill a ring at the ~30-minute gem cadence, and a
  partially filled ring is worse than an empty one: it answers short instead of
  falling back. `SignalHistoryWindow` seeds it once, bounded to the newest
  `depth` snapshot times and to `signalHistorySeedMaxDays`.
- **`gemDictSkills` / `gemDictTransfigured`**
  (`internal/lab/gem_dictionary_cache.go`) — the OCR name dictionary. The tick
  sees only the current snapshot's names; the endpoint's answer is `gem_colors`
  unioned with every name the league has ever shown. `GemNameDictionary` seeds
  both halves once, then each tick unions its own names in. A failed seed leaves
  the cache COLD deliberately: a snapshot-only dictionary is missing every gem
  the market has not priced this league, the league-start blindness the union
  exists to prevent.

The Dedication corpus is not one of these. `BuildDedicationCorpus` derives the
rankings and this snapshot's corrupted gem prices from the `gems` slice
`RunDedication` already loaded, so it is complete after one tick like every
other field.

### Idempotency

`RunV2` runs twice per snapshot, so a repeated pass must change nothing. Three
mechanisms combine: the incremental `since` filter reads no rows, the merge
deduplicates on the `Time` string with the first occurrence winning, and the
high-water mark only advances to an observed timestamp.

### The three memory-safety rules

Each has a concrete failure mode; none is stylistic.

1. **Never store a reslice for the tail.** `points[len(points)-4:]` pins the
   entire history backing array. Because the gem is delisted, no further append
   ever occurs on that series, so the reallocation that would free the array
   never comes — one full history array retained per delisted gem for the rest
   of the league. `mergeSparklineSeries` always returns a freshly allocated
   slice.
2. **Never compact in place.** `copy(s, s[n:]); s = s[:len(s)-n]` mutates
   elements a concurrent reader is already holding, breaking the immutability
   contract above. It is a real `-race` failure, not a theoretical one. Build a
   fresh slice per series and assign.
3. **Never hold the lock across the query.** `sparklineSnapshot` reads both maps
   and the mark under a single `RLock` and returns; the query, the merge, and
   all allocation happen unlocked; `SetSparklines` then assigns both maps and
   the mark under one write lock.

Sizing, for context: roughly 56 bytes per point (32-byte struct plus the heap
allocation behind the RFC3339 time string), a few thousand series, on the order
of 4-7 MB total. `RunV2` already loads an order of magnitude more history
transiently, twice per tick.

## League rollover

`league.Scope` is resolved once at startup from `runtime_config` and is
immutable (`internal/league/league.go`). `LeagueEventGuard` only rejects
mismatched incoming Mercure events; it cannot switch a live cache's league.

Per ADR-011 the outgoing league's scoped tables are truncated at rollover, and
because scope is process-fixed, rollover requires a **server restart**, which
constructs a fresh `NewCache(scope)`. There is no stale-data leak path today and
no field needs explicit clearing.

The practical consequence for time-series fields: on a fresh post-rollover
process `gem_snapshots` is empty, so sparklines are empty or short for the first
hours of a new league. That is the same cold-start shape every other cache field
has.

If in-process league switching lands (POE-121), `internal/server/league_events.go`
and `Cache.For` are the places that change.
