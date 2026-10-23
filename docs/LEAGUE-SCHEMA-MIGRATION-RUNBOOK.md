# League Schema Migration Runbook

**Status:** Current runbook for league rollover with the POE-127 CLI, plus the
POE-119 migration gate requiring the matching POE-120/POE-121 application revision.

**Last verified:** 2026-10-23 against the league CLI, collector startup order,
scoped-table registry, and desktop league refresh. Migration rehearsal
measurements retain their original dates below.

**Canonical for:** rehearsal and production execution of the POE-119
league-scoping schema migration and normal rollover with `/league`. The tracker
tasks remain canonical for the application implementation.

## Gate

POE-119 cannot deploy independently. Its migrations make `league` mandatory
on the twelve pre-existing data tables, while the pre-POE-120/POE-121 writers do not supply it
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
  `NOT NULL` on those twelve tables while the still-deployed pre-POE-120 writers omit
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

## Sixteen-table count manifest

Capture a `COUNT(*)` for each relation before and after the migration. The
manifest is complete only with all sixteen rows. The last four rows postdate the
POE-119 migration (two added 2026-08-19 by POE-173, one 2026-08-24 by POE-125, one
2026-09-06 by POE-254):
for a POE-119 rehearsal their pre/post-migration and null-`league` columns are
`n/a`; for a league rollover they count like every other row. A rehearsal
restored from a backup older than a table's migration records that row as
absent rather than zero.

`merc_icon_templates` (2026-08-25, POE-200) is deliberately NOT in this manifest
and NOT in the `TRUNCATE` below: mercenary icon art is league-invariant, so the
table is excluded from the rollover wipe by decision, not by oversight — see the
2026-08-25 amendment in
[ADR-011](adr/011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md).

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
| `item_snapshots` |  |  |  |

## Production execution (wipe-first — initial schema migration)

This procedure is specific to the initial schema migration; normal rollover
uses [Changing the active league](#changing-the-active-league). Rehearsed
end-to-end on the real ~27M-row prod backup, 2026-07-24.

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
6. On the production database (old schema, pre-migration), truncate the sixteen
   league-scoped tables in one statement:
   `TRUNCATE gem_snapshots, currency_snapshots, fragment_snapshots, font_snapshots, transfigure_results, quality_results, trend_results, gem_features, gem_signals, dedication_snapshots, trade_lookups, market_context, currency_exchange_markets, currency_exchange_cursor, double_corrupt_snapshots, item_snapshots;`
   (Plain `TRUNCATE` works directly on the compressed hypertables — no decompress
   step; ~3.5 s for 27M rows in rehearsal. `currency_exchange_markets` and
   `currency_exchange_cursor` exist only from the POE-173 migration onward, and
   `double_corrupt_snapshots` from the POE-125 migration onward, and
   `item_snapshots` from the POE-254 migration onward; drop absent
   tables from the statement when running against an older schema.)
7. Confirm all sixteen tables report zero rows.
8. Deploy the staged 119–121 revision (the atomic merge). The server migrates on
   boot against the now-empty tables — expect a near-instant apply.
9. Verify migration state + server health: every table has a `NOT NULL` `league`
   column with its `*_league_fkey`, the control tables (`leagues`,
   `runtime_config`) exist, and the recreated continuous aggregates are present
   (empty).
10. The migration seeds `active_league = 'Mirage'`. Activate the new league via
    [Changing the active league](#changing-the-active-league), using
    `"Allflame"` as the new id and `"Mirage"` as the old id (exact upstream
    casing). That procedure requires a revision shipping `/league`; do not run
    it from an older image. Its Phase B includes the assertion, restarts, and
    health checks in steps 11–14 below. After Phase B: Allflame active and
    collecting (revision bumped), Mirage archived.
11. Set `EXPECTED_LEAGUE=Allflame` (or unset it) in the server/collector config.
12. Restart the server; wait for healthy.
13. Start the collector from the same revision; verify health.
14. Verify a scoped write/read lands under `Allflame` and the server rejects a
    lingering old-league event.
15. If any check fails, keep both services stopped and restore the pre-deploy
    backup. Do not attempt a generic down migration on production data.

## Changing the active league

Current normal-rollover procedure (POE-127). Use a server image that ships
`/league`. The active league lives in `runtime_config`; `LEAGUE` is no longer
read. `EXPECTED_LEAGUE` only asserts the selected id. Activation is
**restart-to-activate**: server and collector keep their boot-time league scope.

Run each action and check in order. Phase A can run hours or days ahead without
changing the running league. Phase B performs the flip at league start.
`"<NewLeagueId>"` and `"<OldLeagueId>"` stand for exact upstream identifiers,
including casing and spaces, as used by poe.ninja and trade requests. Compare
with poe.ninja's league id, not a display alias. Replace all placeholders before
running commands; `<server container>` names the current server container and
must be resolved again after its restart. Keep deployment handles private.

The CLI serializes administration with transaction advisory locks. Activation
and committed wipes also require the collector's runtime fence to be free.
Prepare and activate dry runs execute their control-row changes and roll back;
wipe dry runs only preview counts, mode, and SQL, executing no TRUNCATE/DELETE.

### Phase A — prepare (before start; no restart)

1. Preview registration, using the announced start in RFC3339 format:

   ```sh
   docker exec <server container> /league prepare "<NewLeagueId>" --display-name "<Display Name>" --starts-at "<RFC3339 start>" --dry-run
   ```

   Compare the quoted id in the output with poe.ninja's exact league id. The
   preview must end with `dry run: rolled back`. `--starts-at` is informational;
   omit it when the start is unknown (stored as NULL).
2. Register with the same checked arguments, without `--dry-run`:

   ```sh
   docker exec <server container> /league prepare "<NewLeagueId>" --display-name "<Display Name>" --starts-at "<RFC3339 start>"
   docker exec <server container> /league status --no-counts
   ```

   Confirm the new league is `prepared` and the active id and revision are
   unchanged. The display name defaults to the id if `--display-name` is omitted.

### Phase B — activate, wipe, and restart (at league start)

3. Preserve a dedicated outgoing-league dump outside the nightly rotation and
   verify its restore procedure using the private backup/restore guide outside
   this repository. Past-league analysis requires restoring that dump to a
   scratch database (ADR-011); do not record backup locations here.
4. Disable collector auto-restart, stop the collector, and confirm it remains
   stopped. Keep it stopped through the wipe, server restart, and stray cleanup.
   It resolves its league **before** acquiring its runtime fence, so a process
   started during the flip could retain the old scope even after the fence frees.
5. Preview activation, inspect the quoted before/after ids and revision, then
   commit the same checked id:

   ```sh
   docker exec <server container> /league activate "<NewLeagueId>" --dry-run
   docker exec <server container> /league activate "<NewLeagueId>"
   ```

6. **Before any wipe**, compare the quoted new id in the committed activation
   output against poe.ninja's exact league id. Confirm the outgoing id is
   `"<OldLeagueId>"`, now `archived`, and the new id is `collecting` with the
   revision incremented. If wrong, stop here and use the revert below. After a
   wipe, recovery requires the preserved dump restore.
7. Preview per-table outgoing counts and the selected mode, then wipe:

   ```sh
   docker exec <server container> /league wipe "<OldLeagueId>" --dry-run
   docker exec <server container> /league wipe "<OldLeagueId>"
   ```

   With only outgoing-league rows, this selects one TRUNCATE across the sixteen
   tables in `ScopedTables` (`internal/league/tables.go`). It runs while the old
   server still serves. ACCESS EXCLUSIVE locks block table access for the
   `EXISTS` probes plus TRUNCATE, without a full count under that lock. The lock
   wait is bounded to 10 seconds. The preview counts are advisory; execution
   checks again under the locks. Mode `none` means no outgoing rows remain.
   A refusal for other leagues' rows requires investigation; reserve
   `--allow-delete` for the checked stray cleanup below.
8. Set `EXPECTED_LEAGUE` to `"<NewLeagueId>"` or unset it in **both** server and
   collector deployment configuration. A stale assertion refuses startup.
9. Restart the server and wait until healthy. Do not start the collector yet.
10. Check counted status in the current server container:

    ```sh
    docker exec <server container> /league status
    ```

    Confirm the active id and revision. The counted form scans all scoped tables;
    use it here to find outgoing rows the old server wrote after the first wipe.
    If `"<OldLeagueId>"` has rows again, preview and clean them explicitly:

    ```sh
    docker exec <server container> /league wipe "<OldLeagueId>" --allow-delete --dry-run
    docker exec <server container> /league wipe "<OldLeagueId>" --allow-delete
    docker exec <server container> /league status
    ```

    With mixed leagues this deletes only outgoing rows, preserving the new
    league's rows; an exclusive outgoing-only set still selects TRUNCATE.
    Confirm no outgoing rows remain.
11. Re-enable collector auto-restart and start the collector, after server health
    and cleanup have passed.
12. Check `/health` and `/latest` report `"<NewLeagueId>"`. Confirm a scoped
    write/read lands under it and the server rejects a lingering old-league event.
13. Desktop users press **Settings → Refresh** or restart the app. Its league
    resolver has no background poller after a successful resolve
    (`refresh_league` in `desktop/src-tauri/src/ssot.rs`).

### Wrong committed activation — only before `wipe`

There is no undo CLI command, and an archived league cannot be activated through
`/league activate`. If step 6 catches a wrong id, keep the collector stopped with
auto-restart disabled, pause other administration, and run this recovery
transaction through the private database administration procedure. Substitute
`"<WrongLeagueId>"` with the committed wrong id and `"<OldLeagueId>"` with the
previous active id; escape SQL literals when substituting. Confirm those rows
and states before committing; roll back if they differ from the captured
activation output.

```sql
BEGIN;
LOCK TABLE runtime_config, leagues IN ACCESS EXCLUSIVE MODE;
SELECT active_league, revision FROM runtime_config WHERE singleton = TRUE;
SELECT id, collection_state FROM leagues
WHERE id IN ('<OldLeagueId>', '<WrongLeagueId>');
-- Restore the selection, but advance the revision rather than restoring it.
UPDATE runtime_config
SET active_league = '<OldLeagueId>', revision = revision + 1, updated_at = NOW()
WHERE singleton = TRUE AND active_league = '<WrongLeagueId>';
UPDATE leagues
SET collection_state = 'collecting', updated_at = NOW()
WHERE id = '<OldLeagueId>' AND collection_state = 'archived';
UPDATE leagues
SET collection_state = 'prepared', updated_at = NOW()
WHERE id = '<WrongLeagueId>' AND collection_state = 'collecting';
-- Check each UPDATE affected exactly one row before committing.
COMMIT;
```

This restores the selection and the two collection states; lifecycle timestamps
remain a record of the attempted activation. Check `/league status --no-counts`
in the server container, restore or unset `EXPECTED_LEAGUE` for both services,
and restart the server before starting the collector if aborting the rollover.
Prepare the correct id before retrying the normal procedure. **After `wipe`,
this SQL cannot restore the outgoing data: recover from the preserved dump.**

### Continuous-aggregate data retained

The wipe leaves old-league buckets in `gem_snapshots_hourly` and
`gem_snapshots_daily`. They remain league-keyed and do not mix with the new
league. Disk and dump costs remain: the hourly aggregate measured 2.1 GB /
20.5 M rows in July 2026 (POE-127 D10); ADR-011's backup-shrink expectation
therefore applies only to the raw tables. Removing these buckets is deferred
until a per-league aggregate DELETE is measured on a restored dump; it is not
part of this procedure.

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

Historical observations captured during the first wipe-first deploy. The current
[Changing the active league](#changing-the-active-league) procedure incorporates
the restart and cleanup order with the POE-127 CLI.

1. **The deploy auto-starts services on the SEEDED league, before the flip.**
   Pushing to main → Coolify builds → the new server + collector boot and resolve
   `active_league='Mirage'` (the migration's seed) and immediately begin
   collecting/computing. So stray outgoing-league rows appear *after* the wipe,
   before you activate the new league. Expect them; clean them (step 4).
2. **Running services hold their boot-time league scope.** The flip
   (`runtime_config` → Allflame) does NOT affect an already-running server or
   collector — they resolved at boot. **You must restart both** for the new
   league to take effect. Keep the collector stopped with auto-restart disabled;
   restart the server, clean strays, then start the collector as Phase B specifies.
3. **`EXPECTED_LEAGUE` must be set to the new league only AFTER the flip.** The
   migration seeds `Mirage`; if `EXPECTED_LEAGUE=Allflame` is set before the flip,
   the server fails its assertion on boot and crash-loops. Leave it unset for the
   initial deploy, set it after activating the new league, then restart.
4. **Clean stray outgoing-league rows after the server restart, before collector
   startup.** Use `/league status`, then `/league wipe "<OldLeagueId>"
   --allow-delete` (dry-run first), as Phase B specifies. The registry now covers
   sixteen tables; mixed-league cleanup preserves new-league rows. At the first
   rollover the strays were recent/uncompressed, so their DELETE was cheap.
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
   datacenter IP, so the icon routes cannot fetch at runtime — seed the cache
   volume from an allowed IP (`scripts/download-gem-icons.py` → repopulate the
   volume). New-league gems also need color seeding (a `gem_colors` migration)
   and their icon URLs added — see [ICONS.md](ICONS.md).
