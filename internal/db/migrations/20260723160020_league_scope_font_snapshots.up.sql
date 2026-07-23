SELECT remove_compression_policy('font_snapshots', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'font_snapshots' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE font_snapshots ADD COLUMN league TEXT;
UPDATE font_snapshots SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM font_snapshots snapshots LEFT JOIN leagues ON leagues.id = snapshots.league WHERE snapshots.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'font_snapshots contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE font_snapshots DROP CONSTRAINT font_snapshots_pkey;
ALTER TABLE font_snapshots ADD PRIMARY KEY (league, time, color, variant, mode);
DROP INDEX IF EXISTS idx_font_snapshots_color_variant;
CREATE INDEX idx_font_snapshots_league_color_variant ON font_snapshots (league, color, variant, time DESC);
ALTER TABLE font_snapshots SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, color, variant', timescaledb.compress_orderby = 'time DESC');
SELECT add_compression_policy('font_snapshots', INTERVAL '7 days');
ALTER TABLE font_snapshots ALTER COLUMN league SET NOT NULL;
