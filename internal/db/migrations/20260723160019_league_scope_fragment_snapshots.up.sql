SELECT remove_compression_policy('fragment_snapshots', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'fragment_snapshots' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE fragment_snapshots ADD COLUMN league TEXT;
UPDATE fragment_snapshots SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM fragment_snapshots snapshots LEFT JOIN leagues ON leagues.id = snapshots.league WHERE snapshots.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'fragment_snapshots contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE fragment_snapshots DROP CONSTRAINT fragment_snapshots_pkey;
ALTER TABLE fragment_snapshots ADD PRIMARY KEY (league, time, fragment_id);
DROP INDEX IF EXISTS idx_fragment_snapshots_id_time;
CREATE INDEX idx_fragment_snapshots_league_id_time ON fragment_snapshots (league, fragment_id, time DESC);
ALTER TABLE fragment_snapshots SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, fragment_id', timescaledb.compress_orderby = 'time DESC');
SELECT add_compression_policy('fragment_snapshots', INTERVAL '7 days');
ALTER TABLE fragment_snapshots ALTER COLUMN league SET NOT NULL;
ALTER TABLE fragment_snapshots ADD CONSTRAINT fragment_snapshots_league_fkey
    FOREIGN KEY (league) REFERENCES leagues(id);
