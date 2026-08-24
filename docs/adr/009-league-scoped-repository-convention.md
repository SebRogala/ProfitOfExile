---
uid: 8e39f612-f8e4-4aeb-b33a-702359ebbcf3
---

# ADR-009: League-Scoped Repository Convention

## Status

Accepted.

**Amended 2026-08-19 (POE-173):** the convention binds every league-scoped table,
not only the twelve POE-119 retrofitted. The set is now fourteen —
`currency_exchange_markets` and `currency_exchange_cursor` were created with
`league TEXT NOT NULL REFERENCES leagues(id)` and are read and written only
through `internal/exchange`'s scope-taking repository.

**Amended 2026-08-24 (POE-125):** `double_corrupt_snapshots` joins the set (now
fifteen), created with `league TEXT NOT NULL REFERENCES leagues(id)` and read
and written only through `internal/lab`'s scope-taking repository.

## Context

POE-119 adds a league column to its twelve historical data tables and introduces
`runtime_config` as the database-backed selection of the active league. A
repository query without a league predicate can mix observations and analysis
from distinct leagues. A string parameter at individual call sites does not
make the selection's configuration revision visible or establish one convention
for historical access.

`internal/league.Scope` identifies the selected league and carries the runtime
configuration revision returned by `league.Resolve`. `league.Historical`
explicitly creates a revision-neutral scope for historical research.

## Decision

Repositories that read or write league-scoped data must accept
`league.Scope`. They use `scope.ID()` in all predicates and inserted rows; they
do not resolve the active league internally and do not accept a bare league ID
for normal scoped operations.

The caller resolves the scope once at the lifecycle boundary, then passes the
same value through every repository and analysis operation in that workflow.
`Scope.Revision()` is carried with the selection so callers can detect a
runtime-configuration change before committing work that depends on it.

Historical reads must be explicit through `league.Historical`; they do not
silently substitute the active league.

## Consequences

- Repository signatures reveal the required data-isolation boundary.
- A workflow can retain one league identity and configuration revision across
  multiple repository calls.
- POE-120 and POE-121 must apply this convention to existing writers, readers,
  and analysis paths before the POE-119 schema migration is deployed.
- Unscoped lookup/control repositories remain unscoped only when they cannot
  access league-scoped data.

## Evidence

- `internal/league/league.go` — `Scope`, runtime resolution, and historical
  selection.
- `internal/db/migrations/20260723160016_create_league_control_tables.up.sql`
  — `leagues` and `runtime_config`.
- POE-119 — schema foundation that establishes the storage boundary.
