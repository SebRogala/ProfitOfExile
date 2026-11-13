-- POE-399 (ADR-034): league-scoped market hypertables keep 21 days, superseding
-- the no-age-based-retention stance of ADR-010/ADR-011. With the default 7-day
-- chunks a 21-day policy keeps 21-28 days. Device-keyed tables are governed by
-- docs/DATA-RETENTION.md; currency_exchange_cursor/_plays/_play_sets are plain
-- tables and get no policy.
-- gem_snapshots_daily is built on gem_snapshots_hourly and neither has a reader,
-- so both go (daily first). trend_results has no reader or writer.
-- Every statement is safe to run twice: the retention block replaces any existing
-- policy so each table ends with exactly one 21-day job.
DO $$
BEGIN
    IF to_regclass('gem_snapshots_daily') IS NOT NULL THEN
        PERFORM remove_continuous_aggregate_policy('gem_snapshots_daily', if_exists => TRUE);
    END IF;
    IF to_regclass('gem_snapshots_hourly') IS NOT NULL THEN
        PERFORM remove_continuous_aggregate_policy('gem_snapshots_hourly', if_exists => TRUE);
    END IF;
END $$;
DROP MATERIALIZED VIEW IF EXISTS gem_snapshots_daily;
DROP MATERIALIZED VIEW IF EXISTS gem_snapshots_hourly;

DROP TABLE IF EXISTS trend_results;

SELECT remove_retention_policy('gem_snapshots', if_exists => TRUE);
SELECT add_retention_policy('gem_snapshots', INTERVAL '21 days');
SELECT remove_retention_policy('currency_snapshots', if_exists => TRUE);
SELECT add_retention_policy('currency_snapshots', INTERVAL '21 days');
SELECT remove_retention_policy('fragment_snapshots', if_exists => TRUE);
SELECT add_retention_policy('fragment_snapshots', INTERVAL '21 days');
SELECT remove_retention_policy('font_snapshots', if_exists => TRUE);
SELECT add_retention_policy('font_snapshots', INTERVAL '21 days');
SELECT remove_retention_policy('transfigure_results', if_exists => TRUE);
SELECT add_retention_policy('transfigure_results', INTERVAL '21 days');
SELECT remove_retention_policy('quality_results', if_exists => TRUE);
SELECT add_retention_policy('quality_results', INTERVAL '21 days');
SELECT remove_retention_policy('gem_features', if_exists => TRUE);
SELECT add_retention_policy('gem_features', INTERVAL '21 days');
SELECT remove_retention_policy('gem_signals', if_exists => TRUE);
SELECT add_retention_policy('gem_signals', INTERVAL '21 days');
SELECT remove_retention_policy('dedication_snapshots', if_exists => TRUE);
SELECT add_retention_policy('dedication_snapshots', INTERVAL '21 days');
SELECT remove_retention_policy('trade_lookups', if_exists => TRUE);
SELECT add_retention_policy('trade_lookups', INTERVAL '21 days');
SELECT remove_retention_policy('market_context', if_exists => TRUE);
SELECT add_retention_policy('market_context', INTERVAL '21 days');
SELECT remove_retention_policy('currency_exchange_markets', if_exists => TRUE);
SELECT add_retention_policy('currency_exchange_markets', INTERVAL '21 days');
SELECT remove_retention_policy('double_corrupt_snapshots', if_exists => TRUE);
SELECT add_retention_policy('double_corrupt_snapshots', INTERVAL '21 days');
SELECT remove_retention_policy('item_snapshots', if_exists => TRUE);
SELECT add_retention_policy('item_snapshots', INTERVAL '21 days');
