---
uid: da1f0040-85f9-4dac-8925-ef0ac6cc76f6
---

# ADR-034: Market Data Is Retained for 21 Days

## Status

Accepted 2026-11-13 (POE-399).

Supersedes the retention parts of
[ADR-010](010-archived-league-history-is-retained-indefinitely.md) (its
no-age-based-retention decision and the POE-173 / POE-125 amendments that bind
new hypertables to it) and of
[ADR-011](011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md)
("ADR-010's removal of age-based retention stands" and the POE-324 retained
continuous aggregates amendment). ADR-011's rollover wipe and dump stay in force.

## Enforcement

Tier: required

Scope: Every league-scoped market hypertable in `league.ScopedTables()`
(`internal/league/tables.go`) and every new one.

Summary: Each league-scoped market hypertable carries exactly one TimescaleDB
retention policy of 21 days, added at creation. Device-keyed tables follow
[DATA-RETENTION.md](../DATA-RETENTION.md) instead.

## Context

ADR-010 removed age-based retention so a league's start window stayed
queryable. In practice no reader looks back further than 14 days: the longest
windows are signal history and offering timing (14 days), then 168 hours for
gem and item snapshots; the result tables are read latest-only. Long history
cost disk without a reader. The `gem_snapshots_hourly` continuous aggregate
alone held about 2.1 GB / 20.5 M rows, and no Go code reads it.

## Decision

- The 14 league-scoped market hypertables (`gem_snapshots`,
  `currency_snapshots`, `fragment_snapshots`, `font_snapshots`,
  `transfigure_results`, `quality_results`, `gem_features`, `gem_signals`,
  `dedication_snapshots`, `trade_lookups`, `market_context`,
  `currency_exchange_markets`, `double_corrupt_snapshots`, `item_snapshots`)
  each carry one `add_retention_policy(..., INTERVAL '21 days')`. With the
  default 7-day chunks, 21 to 28 days stay on disk.
- A new league-scoped market hypertable gets the same policy in its creating
  migration.
- `currency_exchange_cursor`, `currency_exchange_plays` and
  `currency_exchange_play_sets` are plain tables, not hypertables, and get no
  policy; the plays tables are replaced whole on every recompute.
- `gem_snapshots_hourly` and `gem_snapshots_daily` are dropped. Neither has a
  reader, and the daily aggregate is built on the hourly one, so keeping it
  would mean recreating it over raw data, empty, for no reader.
- `trend_results` is dropped: it has no writer and no reader. It leaves
  `league.ScopedTables()` and the rollover wipe set.
- The down migration restores the schema (no policies, `trend_results`, both
  aggregates `WITH NO DATA`) but not the data.

## Consequences

- The league-start window is not kept live. The dump taken at rollover
  (ADR-011) is the only long history.
- Accepted losses beyond 21 days: `cmd/backtest --since`, `cmd/run-v2 --hours`
  (rows it backfills at old times are dropped again on the next job run),
  `cmd/optimize --hours`, and the `trade_lookups` history backtest reads.
- `collector.GetCollectionStats` reports the retention edge, not the league
  start, as the "first snapshot".
- Readers that serve the latest row with no time bound (for example gem name
  autocomplete) return nothing after a collector stall longer than 21 to 28
  days instead of stale data.
- Storage per league is bounded; disk no longer grows with league length.

## Evidence

- `internal/db/migrations/20261113150000_market_data_retention.up.sql` — the
  policies and the drops.
- `internal/league/tables_integration_test.go` —
  `TestScopedHypertablesCarryTheMarketRetentionPolicy` fails when a scoped
  hypertable lacks the 21-day job or another table has one.
- `internal/db/migrations/migrations_integration_test.go` —
  `TestMarketDataRetentionMigration` covers replacement of an older policy,
  idempotency and the down/up round trip.
- Optional pre-deploy check on production:
  `SELECT job_id, proc_name, hypertable_name, config FROM timescaledb_information.jobs ORDER BY 2, 3;`
