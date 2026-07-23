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
