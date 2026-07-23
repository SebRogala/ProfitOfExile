SELECT remove_continuous_aggregate_policy('gem_snapshots_daily', if_exists => TRUE);
SELECT remove_continuous_aggregate_policy('gem_snapshots_hourly', if_exists => TRUE);
DROP MATERIALIZED VIEW IF EXISTS gem_snapshots_daily;
DROP MATERIALIZED VIEW IF EXISTS gem_snapshots_hourly;

SELECT remove_compression_policy('gem_snapshots', if_exists => TRUE);

DO $$
DECLARE
    chunk REGCLASS;
BEGIN
    FOR chunk IN
        SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS
        FROM timescaledb_information.chunks
        WHERE hypertable_name = 'gem_snapshots' AND is_compressed
    LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;

ALTER TABLE gem_snapshots ADD COLUMN league TEXT;
UPDATE gem_snapshots SET league = 'Mirage' WHERE league IS NULL;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM gem_snapshots snapshots
        LEFT JOIN leagues ON leagues.id = snapshots.league
        WHERE snapshots.league IS NULL OR leagues.id IS NULL
    ) THEN
        RAISE EXCEPTION 'gem_snapshots contains a null or unknown league after backfill';
    END IF;
END
$$;

ALTER TABLE gem_snapshots DROP CONSTRAINT gem_snapshots_pkey;
ALTER TABLE gem_snapshots ADD PRIMARY KEY (league, time, name, variant, is_corrupted);

DROP INDEX IF EXISTS idx_gem_snapshots_name_variant;
CREATE INDEX idx_gem_snapshots_league_name_variant ON gem_snapshots (league, name, variant, time DESC);

ALTER TABLE gem_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'league, name, variant',
    timescaledb.compress_orderby = 'time DESC'
);
SELECT add_compression_policy('gem_snapshots', INTERVAL '7 days');

ALTER TABLE gem_snapshots ALTER COLUMN league SET NOT NULL;

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
