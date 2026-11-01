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

## Enforcement

Tier: required

Scope: League rollover, league-dependent tables and schema migration operations.

Summary: Preserve outgoing league data as a dedicated dump before deliberate rollover wipes; league-dependent tables join the wipe set except for the accepted retained continuous aggregates and boss observations.

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

**Amended 2026-08-24 (POE-125):** `double_corrupt_snapshots` joins the wipe set
(now fifteen scoped tables). Every table added to the league-scoped schema joins
it at creation.

**Amended 2026-08-25 (POE-200):** `merc_icon_templates` is the first table
deliberately **excluded** from the wipe set, and the set stays at fifteen. It
carries no `league` column because mercenary support-icon art is
league-invariant — the same support gem draws the same picture in the next
league — so wiping it would discard a still-correct corpus and make every
device relearn 264 keys by hovering. The exclusion is stated in the migration
header as well, so a rollover edit that walks the schema finds the reason
without reading this ADR. The rule it does not change: a table carrying
league-dependent observations joins the wipe set at creation, and an exclusion
has to argue league-invariance the way this one does.

**Amended 2026-09-06 (POE-254):** `item_snapshots` joins the wipe set (now
sixteen scoped tables). It carries poe.ninja item-overview observations —
IncursionTemple, Vial and the five unique slots — for one league, so the
league-dependence test the POE-200 exclusion had to argue against is met
outright. Every table added to the league-scoped schema joins it at creation.

At each league rollover, deliberately **wipe** the outgoing league's sixteen scoped
tables (`TRUNCATE`) rather than keeping them live, and **preserve** that league's
data as a dedicated dump stored outside the nightly rotation. Complete past-league
raw history requires restoring its dump to a scratch database; the dated
POE-317 and POE-324 amendments below qualify live historical access.

On the first stack deploy the wipe happens **before** the schema migration, so the
migration runs on empty tables (see the runbook's wipe-first Production execution).

This uses ADR-010's own escape hatch — deletion is a deliberate operator action,
not a background age policy — so ADR-010's *mechanism* (no retention policies) is
unchanged. Only its *consequence* "`league.Historical` can reach a league's full
lifetime" is superseded: that reach now requires a dump restore.

## Consequences

- The stack migration and every subsequent rollover are memory-safe on the
  constrained shared VPS; the OOM / dirty-migration hazard is removed.
- Live access to complete past-league raw history requires restoring that
  league's preserved dump to a scratch DB — a proven, disposable workflow
  (rehearsed 2026-07-24). Retained aggregate buckets and boss observations remain
  live under the POE-324 and POE-317 amendments below.
- The backup-shrink expectation applies only to wiped raw tables. Retained
  aggregates continue to contribute disk and dump cost (POE-324 below); backups
  are not limited to the active league's data.
- `league.Historical` cannot reach complete wiped raw history live. Explicit
  league-keyed reads can still reach retained aggregates (POE-324 below); this
  exception does not adopt general retention of all computed data.
- ADR-010's "derived tables are recomputable / first pruning candidate" note
  does not recover wiped data without a dump restore; the retained aggregates
  remain a separate exception under POE-324 below.

## Evidence

- `docs/LEAGUE-SCHEMA-MIGRATION-RUNBOOK.md` — "Production execution (wipe-first)":
  ordering (truncate before deploy), the sixteen-table `TRUNCATE`, and the
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

## Amended 2026-11-01 (POE-317): retain versioned boss observations

Status: accepted. The final Operator retention decision explicitly excludes
`boss_loot_runs` from `league.ScopedTables()` and rollover wipes. Unlike market
snapshots, these are device-owned empirical observations used cumulatively by
Boss loot. Keep their league FK and profile version: retained observations do
not establish permanent league invariance, and changed profile pools must not
silently combine. Historical owned edit/delete follows ADR-009's amendment.

This exception supersedes the blanket statements above that every league-FK
table joins the wipe set and that live queries can reach only the active league.
Other wipe-set tables retain their existing rollover behavior, subject to the
separate POE-324 aggregate exception below. The new
migration header records this same explicit exclusion for schema readers.

## Amended 2026-11-01 (POE-324): retained continuous aggregates

Status: accepted; records current rollover behavior. The wipe deliberately
retains older-league buckets in `gem_snapshots_hourly` and
`gem_snapshots_daily`. Explicit historical queries can reach those buckets by
league key without mixing them with the active league; their presence does not
promise complete live past-league raw history, which still requires a dump
restore. Retained buckets continue to consume disk and contribute to logical
dump size, so the active-only live-access and backup-shrink claims above are
qualified by this exception. The runbook's
[Continuous-aggregate data retained](../LEAGUE-SCHEMA-MIGRATION-RUNBOOK.md#continuous-aggregate-data-retained)
section is normative for the procedure and measured disk/dump cost, including
the deferral of aggregate deletion pending measurement on a restored dump.
This exception is separate from POE-317's retained boss observations and does
not change their ownership, versioning or historical mutation rules.
