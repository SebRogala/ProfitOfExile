-- THIS FILE MUST CONTAIN EXACTLY ONE STATEMENT.
--
-- timescaledb.transaction_per_chunk is refused inside a transaction block, and
-- Postgres wraps a multi-statement simple query in one implicit transaction. A
-- second statement here — even SET maintenance_work_mem — therefore fails the
-- migration. Comments are not statements and are safe.
--
-- If the index build fails partway (measured): the invalid parent index
-- survives, schema_migrations is left dirty, re-running fails with
-- "relation ... already exists", and because MigrateUp is fail-fast
-- (cmd/server/main.go, os.Exit(1)) the server crash-loops on deploy. Recovery:
--   1. DROP INDEX IF EXISTS idx_gem_snapshots_league_transfigured_name;
--   2. reset schema_migrations to the previous version (clear the dirty flag)
--   3. redeploy
--
-- Do NOT "fix" that by adding IF NOT EXISTS. It was tested: it makes a partial
-- failure record as clean while leaving chunks unindexed, which is strictly
-- worse — the same broken state, silently.
CREATE INDEX idx_gem_snapshots_league_transfigured_name
    ON gem_snapshots (league, is_transfigured, name)
    WITH (timescaledb.transaction_per_chunk);
