---
uid: b6757498-fa1b-4213-939c-cd41c22fb626
---

# ADR-011: Wipe the Outgoing League at Rollover; Preserve It as a Dump

## Status

Accepted. Supersedes the retain-history-**live** intent of
[ADR-010](010-archived-league-history-is-retained-indefinitely.md); ADR-010's
removal of age-based retention policies stands.

**Amended 2026-08-19 (POE-173):** the wipe set is now fourteen tables — the
original twelve plus `currency_exchange_markets` and `currency_exchange_cursor`.
Every table added to the league-scoped schema joins this set at creation.

## Context

ADR-010 removed age-based retention so archived leagues stay live and queryable,
accepting unbounded storage growth as an operator-monitored constraint, and named
the derived tables as the first candidates for pruning under disk pressure.

Two facts surfaced at the first real rollover (Mirage → Allflame), verified
2026-07-24 on a restored ~27M-row production backup:

- Running the POE-119→121 migration **in place** on ~27M `gem_snapshots` rows
  materializes two continuous aggregates and builds primary keys. In rehearsal it
  peaked at ~4.7 GB — more than the whole prod box (3.7 GiB RAM, shared;
  `shared_buffers` 384 MB) — and continuous-aggregate creation cannot run in a
  transaction, so a mid-migration OOM leaves a **dirty, half-applied forward-only**
  state (recover only by restoring the pre-migration backup). Running the same
  migration on **empty** tables is instant and safe.
- Each nightly backup is a full logical dump (~900 MB, roughly flat day-to-day
  because it re-dumps all accumulated history). Retaining every league live
  compounds this every league.

Prod-shaped sizing (measured): raw observation data ≈ 532 MB (`gem_snapshots`
469 MB dominates); derived/computed data ≈ 824 MB (`gem_features` 404 MB
dominates). Neither half is small, and the computed half is the larger.

## Decision

At each league rollover, deliberately **wipe** the outgoing league's fourteen scoped
tables (`TRUNCATE`) rather than keeping them live, and **preserve** that league's
data as a dedicated dump stored outside the nightly rotation. Analysis of a past
league is served by restoring its dump to a scratch database, not by a live query.

On the first stack deploy the wipe happens **before** the schema migration, so the
migration runs on empty tables (see the runbook's wipe-first Production execution).

This uses ADR-010's own escape hatch — deletion is a deliberate operator action,
not a background age policy — so ADR-010's *mechanism* (no retention policies) is
unchanged. Only its *consequence* "`league.Historical` can reach a league's full
lifetime" is superseded: that reach now requires a dump restore.

## Consequences

- The stack migration and every subsequent rollover are memory-safe on the
  constrained shared VPS; the OOM / dirty-migration hazard is removed.
- Live queries reach only the active league. Past-league analysis requires
  restoring that league's preserved dump to a scratch DB — a proven, disposable
  workflow (rehearsed 2026-07-24).
- Backups shrink to the active league's data, which starts small each league and
  grows over the league instead of accumulating across all leagues.
- `league.Historical`'s live cross-league reach is lost. A "keep computed, drop
  raw" middle path was measured (~824 MB computed) but **not** adopted here; it
  remains candidate future work if live past-league analysis becomes needed.
- ADR-010's "derived tables are recomputable / first pruning candidate" note is
  moot for a wiped league — its entire dataset lives only in the preserved dump.

## Evidence

- `docs/LEAGUE-SCHEMA-MIGRATION-RUNBOOK.md` — "Production execution (wipe-first)":
  ordering (truncate before deploy), the fourteen-table `TRUNCATE`, and the
  preserve-dump step.
- Rehearsal 2026-07-24: restored the `profitofexile` nightly (~27M `gem_snapshots`)
  to a disposable scratch DB; `TRUNCATE` 27M → 0 in ~3.5 s directly on the
  compressed hypertables (no decompress); migrate-on-empty near-instant; the
  Allflame Phase A→B switch and DB-level FK fail-closed (writes to an unregistered
  league rejected) verified.
- [ADR-010](010-archived-league-history-is-retained-indefinitely.md) — the
  retain-history decision this supersedes in part.
- `internal/db/migrations/20260819120000_create_currency_exchange_markets.up.sql`
  — the two POE-173 tables, created with compression and no retention policy.
