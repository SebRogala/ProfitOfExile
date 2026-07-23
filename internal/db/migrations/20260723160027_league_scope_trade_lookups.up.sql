SELECT remove_compression_policy('trade_lookups', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'trade_lookups' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE trade_lookups ADD COLUMN league TEXT;
UPDATE trade_lookups SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM trade_lookups lookups LEFT JOIN leagues ON leagues.id = lookups.league WHERE lookups.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'trade_lookups contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE trade_lookups DROP CONSTRAINT trade_lookups_pkey;
ALTER TABLE trade_lookups ADD PRIMARY KEY (league, time, gem, variant);
DROP INDEX IF EXISTS idx_trade_lookups_gem_variant_time;
CREATE INDEX idx_trade_lookups_league_gem_variant_time ON trade_lookups (league, gem, variant, time DESC);
ALTER TABLE trade_lookups SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, gem, variant', timescaledb.compress_orderby = 'time DESC');
SELECT add_compression_policy('trade_lookups', INTERVAL '7 days');
ALTER TABLE trade_lookups ALTER COLUMN league SET NOT NULL;
