SELECT remove_compression_policy('trend_results', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'trend_results' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE trend_results ADD COLUMN league TEXT;
UPDATE trend_results SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM trend_results results LEFT JOIN leagues ON leagues.id = results.league WHERE results.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'trend_results contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE trend_results DROP CONSTRAINT trend_results_pkey;
ALTER TABLE trend_results ADD PRIMARY KEY (league, time, name, variant);
DROP INDEX IF EXISTS idx_trend_results_signal;
DROP INDEX IF EXISTS idx_trend_results_variant;
CREATE INDEX idx_trend_results_league_signal ON trend_results (league, time DESC, signal);
CREATE INDEX idx_trend_results_league_variant ON trend_results (league, variant, time DESC);
ALTER TABLE trend_results SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, variant', timescaledb.compress_orderby = 'time DESC, name');
SELECT add_compression_policy('trend_results', INTERVAL '7 days');
ALTER TABLE trend_results ALTER COLUMN league SET NOT NULL;
ALTER TABLE trend_results ADD CONSTRAINT trend_results_league_fkey
    FOREIGN KEY (league) REFERENCES leagues(id);
