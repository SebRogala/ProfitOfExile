-- Enable compression on all hypertables
ALTER TABLE gem_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'name, variant',
    timescaledb.compress_orderby = 'time DESC'
);

ALTER TABLE font_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'color, variant',
    timescaledb.compress_orderby = 'time DESC'
);

ALTER TABLE exchange_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'name',
    timescaledb.compress_orderby = 'time DESC'
);

ALTER TABLE gcp_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_orderby = 'time DESC'
);

-- Compression policies: compress chunks older than 7 days
SELECT add_compression_policy('gem_snapshots', INTERVAL '7 days');
SELECT add_compression_policy('font_snapshots', INTERVAL '7 days');
SELECT add_compression_policy('exchange_snapshots', INTERVAL '7 days');
SELECT add_compression_policy('gcp_snapshots', INTERVAL '7 days');

-- Retention policies: drop raw data older than 90 days
SELECT add_retention_policy('gem_snapshots', INTERVAL '90 days');
SELECT add_retention_policy('font_snapshots', INTERVAL '90 days');
SELECT add_retention_policy('exchange_snapshots', INTERVAL '90 days');
SELECT add_retention_policy('gcp_snapshots', INTERVAL '90 days');

-- Continuous aggregate: hourly gem snapshot rollup
CREATE MATERIALIZED VIEW gem_snapshots_hourly
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT
    time_bucket('1 hour', time) AS bucket,
    name,
    variant,
    avg(chaos)              AS avg_chaos,
    avg(listings::numeric)  AS avg_listings,
    last(gem_color, time)   AS gem_color,
    last(is_transfigured, time) AS is_transfigured
FROM gem_snapshots
GROUP BY bucket, name, variant
WITH NO DATA;

SELECT add_continuous_aggregate_policy('gem_snapshots_hourly',
    start_offset  => INTERVAL '3 hours',
    end_offset    => INTERVAL '1 hour',
    schedule_interval => INTERVAL '1 hour'
);

-- Continuous aggregate: daily gem snapshot rollup (cascaded from hourly)
CREATE MATERIALIZED VIEW gem_snapshots_daily
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT
    time_bucket('1 day', bucket) AS bucket,
    name,
    variant,
    avg(avg_chaos)          AS avg_chaos,
    avg(avg_listings)       AS avg_listings,
    last(gem_color, bucket) AS gem_color,
    last(is_transfigured, bucket) AS is_transfigured
FROM gem_snapshots_hourly
GROUP BY time_bucket('1 day', bucket), name, variant
WITH NO DATA;

SELECT add_continuous_aggregate_policy('gem_snapshots_daily',
    start_offset  => INTERVAL '3 days',
    end_offset    => INTERVAL '1 day',
    schedule_interval => INTERVAL '1 day'
);
