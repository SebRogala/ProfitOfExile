# League Schema Migration Runbook

**Status:** Current operational gate for POE-119; execute only with the
matching POE-120/POE-121 application revision.

**Last verified:** 2026-07-23 against the POE-119 migration set, embedded
migration loader, and deployment workflow.

**Canonical for:** rehearsal and production execution of the POE-119
league-scoping schema migration. The tracker tasks remain canonical for the
application implementation.

## Gate

POE-119 cannot deploy independently. Its migrations make `league` mandatory
on twelve data tables, while the pre-POE-120/POE-121 writers do not supply it
and server analysis is not yet scoped. Before starting, an operator must
confirm all of the following in the change record:

1. A pre-deploy backup exists and has a documented restore procedure.
2. The server and collector are staged from the same immutable revision that
   contains the POE-120/POE-121 compatible writers, readers, and analysis
   paths.
3. The server and collector are both stopped by an operator; record the
   confirmation and time. Do not rely on a deploy trigger to stop either one.
4. A successful isolated rehearsal completed using the same migration files
   and intended service revision.

Do not use generic down migrations against restored production data. Recovery
is restore of the pre-deploy backup, then investigation and a new forward
migration; it is not `migrate down`.

## Merging the stack (POE-119 + POE-120 + POE-121)

POE-119, POE-120, and POE-121 are developed as **stacked branches**
(120 branches off 119, 121 off 120) and must reach `main` as **one atomic
deploy** — every push to `main` auto-deploys to production.

- **Do not merge POE-119 to `main` on its own.** Alone it makes `league`
  `NOT NULL` on twelve tables while the still-deployed pre-POE-120 writers omit
  it, so production writes begin failing on the next collector tick.
- Land them together. Recommended: collapse POE-120 and POE-121 onto the
  POE-119 branch (rebase each onto its parent so the branch carries all three),
  then open and merge **one** PR to `main`. That single merge is the single
  deploy the Gate above governs.
- The pre-existing POE-119 PR predates the session commits that added the
  foreign keys, retention removal (ADR-010), and `Scope.Validate` — rebase/refresh
  it before relying on it.
- Deploys must be **stop-then-start**: POE-121's boot fences (`RuntimeLockKey`
  for the collector, `ServerLockKey` for the server) refuse readiness while a
  previous instance still holds the lock. The bounded-retry fence waits ~15 s for
  a handoff; a start-before-stop deploy that exceeds that window crashloops the
  new instance until the old one releases. Confirm the orchestrator stops the old
  container before starting the new one.

## Rehearsal

Use an isolated, disposable database restored from a production backup. Never
run the rehearsal against the live database.

1. Restore a complete database backup, or export and import every hypertable
   with `COPY (SELECT * FROM <hypertable>)`. Do not use `pg_dump -t` for every
   hypertable as the rehearsal source.
2. Before migration, record the revision under test, PostgreSQL and TimescaleDB
   versions, `schema_migrations` version/dirty state, compression and
   continuous-aggregate policies, aggregate indexes, and the twelve-table
   count manifest below.
3. Compare the restored counts to the captured source manifest. Stop if any
   count differs; the rehearsal is invalid until the restore is explained.
4. Start only the staged server revision. Its startup migration must complete
   before the server binds; the collector must remain stopped.
5. Record the post-migration migration state and repeat the count manifest.
   Each table must retain its count, and every table must have zero null
   `league` values. Confirm that compression and continuous-aggregate refresh
   policies match the pre-migration capture, that every scoped table now has a
   foreign key to `leagues(id)`, and that no retention policy remains
   (see [ADR-010](adr/010-retain-archived-league-history.md)). Record the disk
   usage that indefinite retention will now grow.
6. Refresh a controlled, completed source window in dependency order: hourly
   aggregate first, then daily. Do not refresh daily before hourly.

   ```sql
   CALL refresh_continuous_aggregate(
       'gem_snapshots_hourly',
       TIMESTAMPTZ '<completed-window-start>',
       TIMESTAMPTZ '<completed-window-end>'
   );
   CALL refresh_continuous_aggregate(
       'gem_snapshots_daily',
       TIMESTAMPTZ '<completed-window-start>',
       TIMESTAMPTZ '<completed-window-end>'
   );
   ```

7. With a controlled fixture in the disposable restore, prove aggregate
   isolation: two valid league values with otherwise matching source dimensions
   produce separate hourly and daily aggregate rows. Discard the fixture with
   the restored database.
8. Record migration duration. Start the staged collector only after the server
   migration and health checks pass. Confirm server and collector startup
   health, then dispose of the rehearsal database.

## Twelve-table count manifest

Capture a `COUNT(*)` for each relation before and after the migration. The
manifest is complete only with all twelve rows:

| Relation | Pre-migration count | Post-migration count | Null `league` count |
| --- | ---: | ---: | ---: |
| `gem_snapshots` |  |  |  |
| `currency_snapshots` |  |  |  |
| `fragment_snapshots` |  |  |  |
| `font_snapshots` |  |  |  |
| `transfigure_results` |  |  |  |
| `quality_results` |  |  |  |
| `trend_results` |  |  |  |
| `gem_features` |  |  |  |
| `gem_signals` |  |  |  |
| `dedication_snapshots` |  |  |  |
| `trade_lookups` |  |  |  |
| `market_context` |  |  |  |

## Production execution

1. Reconfirm the gate, backup, server/collector revision, operator-confirmed
   stops, and successful rehearsal. Record the intended start time.
2. Keep the collector stopped. Start the staged server revision and wait for
   migration completion, migration state, and server health. Record duration.
3. Verify the twelve-table counts and zero-null-league result, the recreated
   continuous aggregates and indexes, and retain the completed-window
   league-isolation proof from rehearsal with the production change record.
4. Start the collector from the same revision only after those checks pass.
   Verify collector health and a scoped write/read path.
5. If any gate or check fails, keep both services stopped and restore the
   pre-deploy backup. Do not attempt a generic down migration on production
   data.

## Interim: changing the active league (until POE-127)

The active league lives in `runtime_config` in the database — there is no
`LEAGUE` env var anymore (POE-121 removed it; `EXPECTED_LEAGUE` only *asserts*,
it does not select). Until POE-127 delivers the rollover CLI, changing leagues is
the manual procedure below. Activation is **restart-to-activate**: the running
collector and server hold their resolved league for the process lifetime, so the
DB change takes effect only after a restart.

Run this while the **collector is stopped** (so no writes land under the old
league and the runtime advisory lock is free), then restart. All statements in
one transaction:

```sql
BEGIN;
-- 1. Register the new league (starts_at optional; set collection_state directly to 'collecting').
INSERT INTO leagues (id, display_name, collection_state, prepared_at, activated_at)
VALUES ('<NewLeagueId>', '<Display Name>', 'collecting', NOW(), NOW());

-- 2. Point the runtime at it and bump the revision (the revision bump is what
--    lets a running process detect the change; do not skip it).
UPDATE runtime_config
SET active_league = '<NewLeagueId>', revision = revision + 1, updated_at = NOW()
WHERE singleton = TRUE;

-- 3. Archive the outgoing league. Its data is retained and stays queryable
--    (retention was removed, ADR-010); archiving is a state flip, not a delete.
UPDATE leagues
SET collection_state = 'archived', archived_at = NOW()
WHERE id = '<OldLeagueId>' AND collection_state <> 'archived';
COMMIT;
```

Then:

1. If `EXPECTED_LEAGUE` is set in the server/collector deploy config, update it to
   `<NewLeagueId>` (or unset it) — a stale value makes both services refuse
   readiness.
2. Restart the server, then the collector (server applies migrations and binds
   first; the collector must not start before the server is healthy). Each
   resolves the new league at boot and re-acquires its fence.
3. Verify: `/health` and `/latest` report `<NewLeagueId>`; a scoped write/read
   lands under the new league; the server rejects any lingering old-league event.

`<NewLeagueId>` is the exact upstream league identifier (the GGG league name), the
same string used in the poe.ninja / trade requests — not a display alias.

## Evidence to retain

Retain the source and restored count manifests; version, migration-state,
policy, and aggregate-index captures; operator stop/start confirmations;
rehearsal and production durations; aggregate-isolation proof; and server and
collector health results. Keep them with the deployment record, not this public
repository.

## Verified implementation references

- `internal/db/migrate.go` embeds `internal/db/migrations/` and is used by
  both the server and `cmd/migrate`.
- `cmd/server/main.go` applies pending migrations before binding its HTTP
  listener; `cmd/collector/main.go` deliberately does not migrate.
- `internal/db/migrations/20260723160017_league_scope_gem_snapshots.up.sql`
  recreates the hourly aggregate before the daily aggregate and their indexes
  and policies.
- `.github/workflows/deploy.yml` can deploy server and collector separately;
  this runbook's same-revision gate prevents a mixed writer/schema deployment.
