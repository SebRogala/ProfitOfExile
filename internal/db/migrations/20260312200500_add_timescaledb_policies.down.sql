-- Drop continuous aggregates first (they depend on base hypertables)
DROP MATERIALIZED VIEW IF EXISTS gem_snapshots_daily CASCADE;
DROP MATERIALIZED VIEW IF EXISTS gem_snapshots_hourly CASCADE;

-- Remove retention policies
SELECT remove_retention_policy('gem_snapshots', if_exists => true);
SELECT remove_retention_policy('font_snapshots', if_exists => true);
SELECT remove_retention_policy('exchange_snapshots', if_exists => true);
SELECT remove_retention_policy('gcp_snapshots', if_exists => true);

-- Remove compression policies
SELECT remove_compression_policy('gem_snapshots', if_exists => true);
SELECT remove_compression_policy('font_snapshots', if_exists => true);
SELECT remove_compression_policy('exchange_snapshots', if_exists => true);
SELECT remove_compression_policy('gcp_snapshots', if_exists => true);

-- Decompress any compressed chunks before disabling compression.
-- Without this, ALTER TABLE SET (timescaledb.compress = false) will fail
-- if any chunks have already been compressed.
DO $$
DECLARE
    _tbl TEXT;
    _chunk REGCLASS;
BEGIN
    FOREACH _tbl IN ARRAY ARRAY['gem_snapshots','font_snapshots','exchange_snapshots','gcp_snapshots']
    LOOP
        FOR _chunk IN
            SELECT format('%I.%I', chunk_schema, chunk_name)::regclass
            FROM timescaledb_information.chunks
            WHERE hypertable_name = _tbl AND is_compressed
        LOOP
            PERFORM decompress_chunk(_chunk);
        END LOOP;

        EXECUTE format('ALTER TABLE %I SET (timescaledb.compress = false)', _tbl);
    END LOOP;
END;
$$;
