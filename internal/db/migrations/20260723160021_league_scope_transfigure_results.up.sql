SELECT remove_compression_policy('transfigure_results', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'transfigure_results' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE transfigure_results ADD COLUMN league TEXT;
UPDATE transfigure_results SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM transfigure_results results LEFT JOIN leagues ON leagues.id = results.league WHERE results.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'transfigure_results contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE transfigure_results DROP CONSTRAINT transfigure_results_pkey;
ALTER TABLE transfigure_results ADD PRIMARY KEY (league, time, transfigured_name, variant);
DROP INDEX IF EXISTS idx_transfigure_results_roi;
DROP INDEX IF EXISTS idx_transfigure_results_variant;
CREATE INDEX idx_transfigure_results_league_roi ON transfigure_results (league, time DESC, roi DESC);
CREATE INDEX idx_transfigure_results_league_variant ON transfigure_results (league, variant, time DESC);
ALTER TABLE transfigure_results SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, variant', timescaledb.compress_orderby = 'time DESC, roi DESC');
SELECT add_compression_policy('transfigure_results', INTERVAL '7 days');
ALTER TABLE transfigure_results ALTER COLUMN league SET NOT NULL;
ALTER TABLE transfigure_results ADD CONSTRAINT transfigure_results_league_fkey
    FOREIGN KEY (league) REFERENCES leagues(id);
