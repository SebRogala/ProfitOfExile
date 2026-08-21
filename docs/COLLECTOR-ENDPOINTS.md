# Adding a Collector Endpoint

**Status:** Current  
**Last verified:** 2026-08-19  
**Canonical for:** Cross-layer procedure for adding a collector-backed market-data endpoint.

Use the existing fragments endpoint as the working reference. Exact fields,
constraints, retention, and compression policy depend on the source; copy the
project's integration pattern, not its schema blindly.

1. Define the source identity and storage schema. Create a new timestamped
   migration pair with `make migration name=descriptive_name`; never edit a
   migration that may have been deployed. Follow the nearest current hypertable
   for keys, indexes, compression, and retention, then justify any differences.
2. Add the typed snapshot model in `internal/collector/fetcher.go`.
3. Extend `FetchResult` in `internal/collector/endpoint.go` with the new typed
   slice and update `Validate` so only one data variant can be populated.
4. Add the canonical endpoint-name constant in `internal/collector/endpoint.go`.
5. Implement the upstream response conversion and fetch method in
   `internal/collector/ninja.go`, or in a source-specific client when the data
   does not come from poe.ninja.
6. Extend the repository interface and implement the endpoint's latest-snapshot
   and parameterized batch-insert methods in `internal/collector/repository.go`.
7. Add the endpoint-to-topic mapping in `mercureTopicSuffix` in
   `internal/collector/scheduler.go`.
8. Wire `FetchFunc`, `StoreFunc`, and `StalenessFunc` in
   `cmd/collector/main.go`, then add the configuration to the scheduler's
   endpoint list. Preserve `FetchResult.Validate` checks at construction and
   consumption boundaries.
9. Add snapshot handlers and routes under `internal/server`; update aggregate
   snapshot/status responses when the endpoint belongs there.
10. If the server reacts to this endpoint's Mercure notification, subscribe to
    its topic in `cmd/server/main.go` and handle the canonical endpoint name.
11. Add focused tests for conversion, result validation, repository behavior,
    scheduling/topic publication, and HTTP output as applicable. Read the
    repository's test-author contract before creating or modifying tests.

The current fragments path spans `internal/collector/{fetcher,endpoint,ninja,
repository,scheduler}.go`, `cmd/collector/main.go`, `internal/server`, and
`cmd/server/main.go`. Search by `EndpointNinjaFragments`, `FragmentData`, and
`poe/collector/fragments` to inspect the complete implementation before editing.

## Currency Exchange feed (not an endpoint)

The currency exchange feed does not follow the procedure above and is not part of
`internal/collector`'s endpoint list. It reads GGG's CDN directly rather than
poe.ninja, is addressed by unix hour rather than polled for freshness, and returns
every league in one payload, so the scheduler's staleness/`FetchResult` model does
not fit it. It lives in `internal/exchange` and runs in `cmd/collector` as an
`exchange.Runner` sibling goroutine next to `runTradeRefresher` and
`runLayoutResetTicker`: each tick advances a database cursor hour by hour, filters
the payload to the resolved league, stores the hour, and publishes one Mercure
event per stored hour on topic `poe/collector/currency-exchange`. Env:
`EXCHANGE_INGEST_ENABLED` (default on; `false` disables), `EXCHANGE_TICK`
(default `5m`) and `EXCHANGE_PER_HOUR_DELAY` (default `250ms`), which paces the
hours inside one catch-up pass so a long backlog does not pull 48 payloads of
~1.7 MB from the CDN back to back; `0` disables the pacing.

On the server side, `cmd/server` recomputes the league's best plays from the
stored hours on startup and on every `poe/collector/currency-exchange` event,
serves the answer from memory at
`GET /api/currency-exchange/plays?mode=all|direct|1-hop&horizon=recent|day`
(never from the database), and publishes `poe/currency-exchange/updated` after
each recompute burst. Both horizons — `recent` (the default, a six-hour window)
and `day` (twenty-four hours) — come from the same recompute, so `?horizon=` is a
cache lookup; an unknown value of either parameter is a 400. Every price a play
shows is the LAST SNAPSHOT's — the window's newest feed hour, which is the hour
a served play must have been live in (every leg's market traded
`EXCHANGE_MIN_VOLUME_PER_HOUR` units — 1 by default, i.e. a trade happened —
with stock on both sides) and cleared the `EXCHANGE_MIN_EDGE` sanity floor of
+0.1% in; the window contributes `hoursSeen`, a count of every hour the play
cleared on that hour's own prices, and nothing else. Since POE-193 no default
floor hides a live market: liveness is "a trade happened", persistence is
reported through `hoursSeen` rather than demanded (`*_MIN_HOURS_SEEN` defaults
to 1 on both horizons), and thinness shows as `simEntries`/`lowCoverage` and
`suspect`. The old levels — 10 units an hour, 4-of-6 and 18-of-24 hours seen —
remain what the knobs are FOR. On the wire `roiPct` is the fractional return of
one round trip after undercutting each leg by one of its own ticks (`edge` is
its deprecated alias; `roiPctRaw` is the same trip at the raw extremes shown on
the legs) and `roi` is that return in chaos for one exchanged unit, so
`roi == roiPct × investment` by construction. A leg whose extreme sits too far
from that hour's VWAP is flagged `suspect` rather than replaced, and a suspect
play is served ranked after every clean one. The ranking knobs are overridable
with `EXCHANGE_MIN_VOLUME_PER_HOUR`, `EXCHANGE_MIN_EDGE`,
`EXCHANGE_MAX_PLAYS`, the four quality knobs
`EXCHANGE_MIN_TURNOVER_CHAOS`, `EXCHANGE_MAX_TICK`,
`EXCHANGE_MIN_EDGE_TICK_RATIO` and `EXCHANGE_MIN_ROI_CHAOS` (all four ship OFF
since POE-191 — the quality gates live client-side in the desktop's Gates row —
and each accepts a positive value only, so setting one here re-arms it for every
client and can only tighten the served set, never loosen it — a positive
value raises each floor and lowers the one ceiling, MAX_TICK), the junk-flag knobs
`EXCHANGE_SUSPECT_LOW_BAND`, `EXCHANGE_SUSPECT_HIGH_BAND` and
`EXCHANGE_HIDE_SUSPECT` (a bool; drops flagged plays instead of ranking them
last), and the per-horizon windows `EXCHANGE_RECENT_WINDOW_HOURS` /
`EXCHANGE_RECENT_MIN_HOURS_SEEN` and `EXCHANGE_DAY_WINDOW_HOURS` /
`EXCHANGE_DAY_MIN_HOURS_SEEN` (`EXCHANGE_WINDOW_HOURS`
and `EXCHANGE_MIN_HOURS_SEEN` still work and bind the recent horizon only), each
of which logs a WARN and keeps its default when the value is unusable.

Legs are served with display names and icon paths, not raw metadata ids: the
handler resolves each id through the committed asset in
`internal/exchange/itemdata/` (regenerate it once per league with
`scripts/generate-currency-exchange-items.py`) and serves the artwork from
`/api/currency-exchange/icon/{id}` (id `%2F`-escaped) out of
`CURRENCY_EXCHANGE_ICON_CACHE_DIR`,
which must be pre-seeded on production — see [Gem and Item Icons](GEM-ICONS.md).

Read `internal/exchange/doc.go` before changing it — the feed's semantics, the
cursor rule, and the event payload fields are documented there.

**Recovery from a stuck cursor.** If the feed's `next_change_id` is more than one
hour ahead of the stored cursor — the hour aged out of retention or was never
published — the walk stops on that hour and never advances on its own, logging
`WARN currency-exchange: feed moved past the cursor` with `gapHours` on every
tick. Read the position with `SELECT league, next_hour, to_timestamp(next_hour)
FROM currency_exchange_cursor;` and move it by hand:

```sql
UPDATE currency_exchange_cursor
SET next_hour = <unix hour to resume from>, updated_at = now()
WHERE league = '<league>';
```

Resume at the WARN's `nextChangeID` minus 3600 (the newest complete hour), or at
any earlier hour the feed still serves — the inserts are idempotent against the
primary key, so re-walking a stored range is safe. The collector needs no
restart; the next pass reads the cursor row again.
