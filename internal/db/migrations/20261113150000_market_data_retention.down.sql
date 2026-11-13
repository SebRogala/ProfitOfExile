-- POE-399 down: restores the pre-POE-399 schema only. Rows the retention jobs
-- dropped, trend_results rows and the aggregates' materialized buckets are NOT
-- restored; trend_results and both aggregates come back empty (WITH NO DATA).
SELECT remove_retention_policy('gem_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('currency_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('fragment_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('font_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('transfigure_results', if_exists => TRUE);
SELECT remove_retention_policy('quality_results', if_exists => TRUE);
SELECT remove_retention_policy('gem_features', if_exists => TRUE);
SELECT remove_retention_policy('gem_signals', if_exists => TRUE);
SELECT remove_retention_policy('dedication_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('trade_lookups', if_exists => TRUE);
SELECT remove_retention_policy('market_context', if_exists => TRUE);
SELECT remove_retention_policy('currency_exchange_markets', if_exists => TRUE);
SELECT remove_retention_policy('double_corrupt_snapshots', if_exists => TRUE);
SELECT remove_retention_policy('item_snapshots', if_exists => TRUE);

CREATE TABLE trend_results (
    time               TIMESTAMPTZ   NOT NULL,
    name               TEXT          NOT NULL,
    variant            TEXT          NOT NULL,
    gem_color          TEXT          NOT NULL DEFAULT '',
    current_price      NUMERIC(10,2) NOT NULL DEFAULT 0,
    current_listings   INTEGER       NOT NULL DEFAULT 0,
    price_velocity     NUMERIC(10,4) NOT NULL DEFAULT 0,
    listing_velocity   NUMERIC(10,4) NOT NULL DEFAULT 0,
    cv                 NUMERIC(10,2) NOT NULL DEFAULT 0,
    signal             TEXT          NOT NULL DEFAULT 'STABLE',
    hist_position      NUMERIC(5,2)  NOT NULL DEFAULT 0,
    price_high_7d      NUMERIC(10,2) NOT NULL DEFAULT 0,
    price_low_7d       NUMERIC(10,2) NOT NULL DEFAULT 0,
    base_listings      INTEGER       NOT NULL DEFAULT 0,
    base_velocity      NUMERIC(10,4) NOT NULL DEFAULT 0,
    relative_liquidity NUMERIC(6,4)  NOT NULL DEFAULT 0,
    liquidity_tier     TEXT          NOT NULL DEFAULT 'MED',
    window_score       NUMERIC(5,2)  NOT NULL DEFAULT 0,
    window_signal      TEXT          NOT NULL DEFAULT 'CLOSED',
    advanced_signal    TEXT          NOT NULL DEFAULT '',
    price_tier         TEXT          NOT NULL DEFAULT 'LOW',
    tier_action        TEXT          NOT NULL DEFAULT '',
    sell_urgency       TEXT          NOT NULL DEFAULT '',
    sell_reason        TEXT          NOT NULL DEFAULT '',
    sellability        INTEGER       NOT NULL DEFAULT 50,
    sellability_label  TEXT          NOT NULL DEFAULT 'MODERATE',
    league             TEXT          NOT NULL REFERENCES leagues(id),
    PRIMARY KEY (league, time, name, variant)
);
SELECT create_hypertable('trend_results', 'time');
CREATE INDEX idx_trend_results_league_signal ON trend_results (league, time DESC, signal);
CREATE INDEX idx_trend_results_league_variant ON trend_results (league, variant, time DESC);
CREATE INDEX idx_trend_results_window ON trend_results (time DESC, window_signal);
ALTER TABLE trend_results SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, variant', timescaledb.compress_orderby = 'time DESC, name');
SELECT add_compression_policy('trend_results', INTERVAL '7 days');

CREATE MATERIALIZED VIEW gem_snapshots_hourly
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT
    time_bucket('1 hour', time) AS bucket,
    league,
    name,
    variant,
    is_corrupted,
    avg(chaos) AS avg_chaos,
    avg(listings::numeric) AS avg_listings,
    last(gem_color, time) AS gem_color,
    last(is_transfigured, time) AS is_transfigured
FROM gem_snapshots
GROUP BY bucket, league, name, variant, is_corrupted
WITH NO DATA;

CREATE INDEX idx_gem_snapshots_hourly_league_name_variant
    ON gem_snapshots_hourly (league, name, variant, is_corrupted, bucket DESC);

SELECT add_continuous_aggregate_policy('gem_snapshots_hourly',
    start_offset => INTERVAL '3 hours',
    end_offset => INTERVAL '1 hour',
    schedule_interval => INTERVAL '1 hour'
);

CREATE MATERIALIZED VIEW gem_snapshots_daily
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT
    time_bucket('1 day', bucket) AS bucket,
    league,
    name,
    variant,
    is_corrupted,
    avg(avg_chaos) AS avg_chaos,
    avg(avg_listings) AS avg_listings,
    last(gem_color, bucket) AS gem_color,
    last(is_transfigured, bucket) AS is_transfigured
FROM gem_snapshots_hourly
GROUP BY time_bucket('1 day', bucket), league, name, variant, is_corrupted
WITH NO DATA;

CREATE INDEX idx_gem_snapshots_daily_league_name_variant
    ON gem_snapshots_daily (league, name, variant, is_corrupted, bucket DESC);

SELECT add_continuous_aggregate_policy('gem_snapshots_daily',
    start_offset => INTERVAL '3 days',
    end_offset => INTERVAL '1 day',
    schedule_interval => INTERVAL '1 day'
);
