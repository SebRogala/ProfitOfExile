-- THIS FILE MUST CONTAIN EXACTLY ONE STATEMENT.
--
-- timescaledb.transaction_per_chunk is refused inside a transaction block, and
-- Postgres wraps a multi-statement simple query in one implicit transaction. A
-- second statement here — even SET maintenance_work_mem — therefore fails the
-- migration. Comments are not statements and are safe.
--
-- If the index build fails partway (measured): the invalid parent index
-- survives, some chunks keep their index and the rest stay unindexed, and
-- schema_migrations is left dirty. golang-migrate checks the dirty flag before
-- it reads any migration (migrate.go Up(), the dirty check precedes readUp), so
-- a redeploy never re-executes this statement — it fails immediately with
-- "Dirty database version N. Fix and force version." and, because MigrateUp is
-- fail-fast (cmd/server/main.go, os.Exit(1)), the server crash-loops. Recovery:
--   1. DROP INDEX IF EXISTS idx_gem_snapshots_league_transfigured_name;
--   2. make migrate-force VERSION=<previous version>
--   3. redeploy
-- Step 1 before step 2: forcing the version without dropping the index makes the
-- retry fail with "relation ... already exists" instead.
--
-- Do NOT "fix" that by adding IF NOT EXISTS. It was tested: it makes a partial
-- failure record as clean while leaving chunks unindexed, which is strictly
-- worse — the same broken state, silently.
CREATE INDEX idx_gem_snapshots_league_transfigured_name
    ON gem_snapshots (league, is_transfigured, name)
    WITH (timescaledb.transaction_per_chunk);
