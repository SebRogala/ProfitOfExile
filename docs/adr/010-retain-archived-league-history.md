# ADR-010: Archived League History Is Retained Indefinitely

## Status

Accepted

## Context

POE-119 scopes twelve data tables by league and introduces `league.Historical`
so research can select an archived league. The same tables carried TimescaleDB
retention policies from POE-15 onward: 90 days on the observation tables and
120 days on the derived tables.

A retention policy drops chunks by time alone. It cannot distinguish leagues,
because a chunk is a time range across every league in the hypertable. The
policies therefore delete each league's earliest observations first — the
league-start window, which carries the price dynamics the league module exists
to study. A four-month league such as Mirage outlives the 90-day window, so
even the active league loses its start.

Scoping the schema without changing retention would give POE-117 isolation
over data that the background jobs are already deleting.

## Decision

Remove the retention policies from the eleven scoped hypertables that carried
them. Compression policies stay unchanged; compressed history is the mechanism
that makes indefinite retention affordable.

Deleting an archived league is a deliberate operation against that league's
rows, not a background job that runs on age.

## Consequences

- `league.Historical` can reach a league's full lifetime, including its start.
- Storage grows without bound in proportion to leagues collected. Disk becomes
  an operator-monitored constraint rather than a self-limiting one.
- The derived tables (`gem_features`, `gem_signals`, `trend_results`,
  `transfigure_results`, `quality_results`, `market_context`) are recomputable
  from the observation tables. They are the first candidates for reinstated
  pruning if disk pressure forces a change.
- Rollback of `20260724090000_retain_league_history` is refused: restoring the
  policies deletes history that was kept because of this decision.

## Evidence

- `internal/db/migrations/20260724090000_retain_league_history.up.sql` — policy
  removal.
- `internal/db/migrations/migrations_integration_test.go` —
  `TestLeagueMigrationsPreserveLegacySchemaContract` asserts compression and
  continuous-aggregate refresh survive the POE-119 migration while no retention
  policy remains.
- [ADR-009](009-league-scope-repository-convention.md) — the scope convention
  whose historical access this decision makes meaningful.
