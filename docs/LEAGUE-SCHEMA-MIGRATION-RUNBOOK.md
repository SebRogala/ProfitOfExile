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
on the twelve tables it scopes, while the pre-POE-120/POE-121 writers do not supply it
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
  `NOT NULL` on the twelve tables it scopes while the still-deployed pre-POE-120
  writers omit it, so production writes begin failing on the next collector tick.
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
   continuous-aggregate policies, aggregate indexes, and the league-scoped table
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
   (see [ADR-010](adr/010-archived-league-history-is-retained-indefinitely.md)). Record the disk
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

## Fifteen-table count manifest

Capture a `COUNT(*)` for each relation before and after the migration. The
manifest is complete only with all fifteen rows. The last three rows postdate the
POE-119 migration (two added 2026-08-19 by POE-173, one 2026-08-24 by POE-125):
for a POE-119 rehearsal their pre/post-migration and null-`league` columns are
`n/a`; for a league rollover they count like every other row.

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
| `currency_exchange_markets` |  |  |  |
| `currency_exchange_cursor` |  |  |  |
| `double_corrupt_snapshots` |  |  |  |

## Production execution (wipe-first — the chosen rollover)

Rehearsed end-to-end on the real ~27M-row prod backup, 2026-07-24.

**Why wipe-first:** running the migration in place on ~27M `gem_snapshots` rows
materializes two continuous aggregates and builds primary keys — locally it peaked
at ~4.7 GB, more than the whole prod box (3.7 GiB RAM, shared; `shared_buffers`
384 MB), and continuous-aggregate creation cannot run in a transaction, so a
mid-migration OOM/failure leaves a **dirty, half-applied forward-only** state
(recover only by restoring the pre-migration backup). Truncating the observation
data first makes the migration run on **empty** tables — instant, no aggregate
build, no memory pressure. Prior-league data is preserved as a separate dump
(supersedes ADR-010's retain-history-live intent — see ADR-011).

**Ordering is load-bearing:** the server applies migrations **on boot**, so the
`TRUNCATE` MUST happen *before* the new revision deploys — deploy first and the
migration runs against the full 27M rows, which is the exact hazard above. Do the
steps in order; each line is one action or one check.

1. Reconfirm the gate, the staged server/collector revision, and the intended
   start time.
2. Take a dedicated final backup of the outgoing league and store it in a
   non-rotating archive on the backup store (separate from the nightly rotation so
   it can't be overwritten) — this is the only way the old data stays reachable
   after the wipe (restore it to a scratch DB for analysis). Follow the private
   backup/restore guide kept outside this repository; do not record concrete
   backup locations here (public repo).
3. Stop the collector.
4. Stop the current (old) server revision.
5. Confirm both are stopped and nothing is writing.
6. On the production database (old schema, pre-migration), truncate the fifteen
   league-scoped tables in one statement:
   `TRUNCATE gem_snapshots, currency_snapshots, fragment_snapshots, font_snapshots, transfigure_results, quality_results, trend_results, gem_features, gem_signals, dedication_snapshots, trade_lookups, market_context, currency_exchange_markets, currency_exchange_cursor, double_corrupt_snapshots;`
   (Plain `TRUNCATE` works directly on the compressed hypertables — no decompress
   step; ~3.5 s for 27M rows in rehearsal. `currency_exchange_markets` and
   `currency_exchange_cursor` exist only from the POE-173 migration onward, and
   `double_corrupt_snapshots` from the POE-125 migration onward; drop absent
   tables from the statement when running against an older schema.)
7. Confirm all fifteen tables report zero rows.
8. Deploy the staged 119–121 revision (the atomic merge). The server migrates on
   boot against the now-empty tables — expect a near-instant apply.
9. Verify migration state + server health: every table has a `NOT NULL` `league`
   column with its `*_league_fkey`, the control tables (`leagues`,
   `runtime_config`) exist, and the recreated continuous aggregates are present
   (empty).
10. The migration seeds `active_league = 'Mirage'`. Activate the new league via
    the Phase A→B procedure below with `<NewLeagueId> = Allflame` (exact GGG
    casing). After Phase B: `active_league = Allflame` (revision bumped), Mirage
    archived, Allflame collecting.
11. Set `EXPECTED_LEAGUE=Allflame` (or unset it) in the server/collector config.
12. Restart the server; wait for healthy.
13. Start the collector from the same revision; verify health.
14. Verify a scoped write/read lands under `Allflame` and the server rejects a
    lingering old-league event.
15. If any check fails, keep both services stopped and restore the pre-deploy
    backup. Do not attempt a generic down migration on production data.

## Interim: changing the active league (until POE-127)

The active league lives in `runtime_config` in the database — there is no
`LEAGUE` env var anymore (POE-121 removed it; `EXPECTED_LEAGUE` only *asserts*,
it does not select). Until POE-127 delivers the rollover CLI, change leagues with
the two-phase checklist below. Activation is **restart-to-activate**: the
collector and server hold their resolved league for the process lifetime, so the
DB change takes effect only after a restart.

The split matters because you rarely get a long calm window right before league
start. **Phase A (pre-register)** is safe to run hours or days ahead and does not
touch the running league; **Phase B (activate)** is the short, restart-bearing
flip you run at go-time. Do the steps in order, one at a time — each line is a
single action or a single check.

`<NewLeagueId>` / `<OldLeagueId>` are the exact upstream GGG league identifiers
(the same string used in poe.ninja / trade requests), not display aliases.

### Phase A — pre-register the new league (any time before start; no restart)

The new league's exact GGG id is known once GGG announces it, so do this ahead of
time. It inserts an inert `'prepared'` row; the runtime keeps serving the current
league until Phase B flips `runtime_config`.

1. Run this single statement against the production database, substituting the
   placeholders (`starts_at` is informational — set the announced start or `NULL`):

   ```sql
   INSERT INTO leagues (id, display_name, collection_state, prepared_at, starts_at)
   VALUES ('<NewLeagueId>', '<Display Name>', 'prepared', NOW(), '<StartsAt or NULL>');
   ```

2. Confirm the row exists with `collection_state = 'prepared'` and that
   `runtime_config.active_league` is still the old league (unchanged).

### Phase B — activate at league start (fast; restart-to-activate)

3. Stop the collector (no new writes under the old league; frees the runtime
   advisory lock before the flip).
4. Confirm the collector process is stopped.
5. Run this single transaction against the production database (if you skipped
   Phase A, replace the first `UPDATE` with the `INSERT` from step 1 but with
   `collection_state = 'collecting'` and `activated_at = NOW()`):

   ```sql
   BEGIN;
   -- Promote the pre-registered league to active.
   UPDATE leagues
   SET collection_state = 'collecting', activated_at = NOW()
   WHERE id = '<NewLeagueId>';
   -- Point the runtime at it and bump the revision (the bump is what lets a
   -- running process detect the change; do not skip it).
   UPDATE runtime_config
   SET active_league = '<NewLeagueId>', revision = revision + 1, updated_at = NOW()
   WHERE singleton = TRUE;
   -- Archive the outgoing league (state flip, not a delete; data stays queryable
   -- per ADR-010).
   UPDATE leagues
   SET collection_state = 'archived', archived_at = NOW()
   WHERE id = '<OldLeagueId>' AND collection_state <> 'archived';
   COMMIT;
   ```

6. If `EXPECTED_LEAGUE` is set in the server/collector deploy config, set it to
   `<NewLeagueId>` (or unset it) — a stale value makes both services refuse
   readiness. Skip only if it is not set.
7. Restart the server.
8. Wait for the server to report healthy (it applies migrations and binds before
   it is ready).
9. Restart the collector — only after step 8 (it must not start before the
   server is healthy).
10. Check that `/health` reports `<NewLeagueId>`.
11. Check that `/latest` reports `<NewLeagueId>`.
12. Confirm a scoped write/read lands under `<NewLeagueId>`.
13. Confirm the server rejects a lingering old-league event.

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

## Operational lessons from the first rollover (Mirage → Allflame, 2026-07-24)

Captured live during the first wipe-first deploy. **POE-127's rollover CLI must
encode these**; until it exists, follow them manually.

1. **The deploy auto-starts services on the SEEDED league, before the flip.**
   Pushing to main → Coolify builds → the new server + collector boot and resolve
   `active_league='Mirage'` (the migration's seed) and immediately begin
   collecting/computing. So stray outgoing-league rows appear *after* the wipe,
   before you activate the new league. Expect them; clean them (step 4).
2. **Running services hold their boot-time league scope.** The flip
   (`runtime_config` → Allflame) does NOT affect an already-running server or
   collector — they resolved at boot. **You must restart both** for the new
   league to take effect. Order doesn't matter (distinct advisory locks); restart
   both before cleaning strays.
3. **`EXPECTED_LEAGUE` must be set to the new league only AFTER the flip.** The
   migration seeds `Mirage`; if `EXPECTED_LEAGUE=Allflame` is set before the flip,
   the server fails its assertion on boot and crash-loops. Leave it unset for the
   initial deploy, set it after activating the new league, then restart.
4. **Clean the stray outgoing-league rows** once services are on the new league
   (so no new strays land): `DELETE FROM <each of the 14 league-scoped tables>
   WHERE league='<OldLeagueId>';` (they are recent/uncompressed, so DELETE is
   cheap).
5. **Collector boot fence can crash-loop on restart.** If a previous collector
   instance's DB connection is still holding `RuntimeLockKey`, the new one refuses
   to start ("another collector still holds the runtime fence after 15s"). It
   self-heals once the old connection closes (~seconds); force it by fully
   stopping the collector, waiting ~30s, then starting one. `pg_terminate_backend`
   on the lock holder is the guaranteed release.
6. **Pre-launch "empty data" warnings are expected.** Before the new league opens,
   poe.ninja returns 200-with-empty; the collector correctly refuses to store it
   ("empty data … possible transient API issue") and the server logs "no rows".
   Not errors — they clear once real data flows.
7. **Gem icons need pre-population, not runtime fetch.** poewiki 403s the VPS
   datacenter IP, so `/api/gem-icon` cannot fetch at runtime — seed the cache
   volume from an allowed IP (`scripts/download-gem-icons.py` → repopulate the
   volume). New-league gems also need color seeding (a `gem_colors` migration)
   and their icon URLs added — see docs/KNOWN-MISSING-GEM-ICONS.md.
