SELECT remove_compression_policy('market_context', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'market_context' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE market_context ADD COLUMN league TEXT;
UPDATE market_context SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM market_context context_rows LEFT JOIN leagues ON leagues.id = context_rows.league WHERE context_rows.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'market_context contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE market_context DROP CONSTRAINT market_context_pkey;
ALTER TABLE market_context ADD PRIMARY KEY (league, time);
DROP INDEX IF EXISTS idx_market_context_time;
CREATE INDEX idx_market_context_league_time ON market_context (league, time DESC);
ALTER TABLE market_context SET (timescaledb.compress, timescaledb.compress_segmentby = 'league', timescaledb.compress_orderby = 'time DESC');
SELECT add_compression_policy('market_context', INTERVAL '7 days');
ALTER TABLE market_context ALTER COLUMN league SET NOT NULL;
