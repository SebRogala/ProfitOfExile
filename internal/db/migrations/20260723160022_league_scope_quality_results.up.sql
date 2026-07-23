SELECT remove_compression_policy('quality_results', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'quality_results' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE quality_results ADD COLUMN league TEXT;
UPDATE quality_results SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM quality_results results LEFT JOIN leagues ON leagues.id = results.league WHERE results.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'quality_results contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE quality_results DROP CONSTRAINT quality_results_pkey;
ALTER TABLE quality_results ADD PRIMARY KEY (league, time, name, level);
DROP INDEX IF EXISTS idx_quality_results_roi;
DROP INDEX IF EXISTS idx_quality_results_level;
CREATE INDEX idx_quality_results_league_roi ON quality_results (league, time DESC, avg_roi DESC);
CREATE INDEX idx_quality_results_league_level ON quality_results (league, level, time DESC);
ALTER TABLE quality_results SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, level', timescaledb.compress_orderby = 'time DESC, avg_roi DESC');
SELECT add_compression_policy('quality_results', INTERVAL '7 days');
ALTER TABLE quality_results ALTER COLUMN league SET NOT NULL;
ALTER TABLE quality_results ADD CONSTRAINT quality_results_league_fkey
    FOREIGN KEY (league) REFERENCES leagues(id);
