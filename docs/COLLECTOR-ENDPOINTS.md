# Adding a Collector Endpoint

**Status:** Current  
**Last verified:** 2026-07-22  
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
