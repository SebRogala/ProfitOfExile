SELECT remove_compression_policy('dedication_snapshots', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'dedication_snapshots' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE dedication_snapshots ADD COLUMN league TEXT;
UPDATE dedication_snapshots SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM dedication_snapshots snapshots LEFT JOIN leagues ON leagues.id = snapshots.league WHERE snapshots.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'dedication_snapshots contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE dedication_snapshots DROP CONSTRAINT dedication_snapshots_pkey;
ALTER TABLE dedication_snapshots ADD PRIMARY KEY (league, time, color, gem_type, mode);
DROP INDEX IF EXISTS idx_dedication_snapshots_color_gemtype_mode;
CREATE INDEX idx_dedication_snapshots_league_color_gemtype_mode ON dedication_snapshots (league, color, gem_type, mode, time DESC);
ALTER TABLE dedication_snapshots SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, color, gem_type, mode');
SELECT add_compression_policy('dedication_snapshots', INTERVAL '7 days');
ALTER TABLE dedication_snapshots ALTER COLUMN league SET NOT NULL;
ALTER TABLE dedication_snapshots ADD CONSTRAINT dedication_snapshots_league_fkey
    FOREIGN KEY (league) REFERENCES leagues(id);
