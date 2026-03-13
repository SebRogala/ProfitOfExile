SELECT remove_retention_policy('currency_snapshots', if_exists => true);
SELECT remove_compression_policy('currency_snapshots', if_exists => true);

-- Decompress any compressed chunks before disabling compression
DO $$
DECLARE
    _chunk REGCLASS;
BEGIN
    FOR _chunk IN
        SELECT format('%I.%I', chunk_schema, chunk_name)::regclass
        FROM timescaledb_information.chunks
        WHERE hypertable_name = 'currency_snapshots' AND is_compressed
    LOOP
        PERFORM decompress_chunk(_chunk);
    END LOOP;
END;
$$;

DROP TABLE IF EXISTS currency_snapshots CASCADE;
