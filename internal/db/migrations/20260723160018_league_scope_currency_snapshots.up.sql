SELECT remove_compression_policy('currency_snapshots', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'currency_snapshots' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE currency_snapshots ADD COLUMN league TEXT;
UPDATE currency_snapshots SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM currency_snapshots snapshots LEFT JOIN leagues ON leagues.id = snapshots.league WHERE snapshots.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'currency_snapshots contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE currency_snapshots DROP CONSTRAINT currency_snapshots_pkey;
ALTER TABLE currency_snapshots ADD PRIMARY KEY (league, time, currency_id);
DROP INDEX IF EXISTS idx_currency_snapshots_id_time;
CREATE INDEX idx_currency_snapshots_league_id_time ON currency_snapshots (league, currency_id, time DESC);
ALTER TABLE currency_snapshots SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, currency_id', timescaledb.compress_orderby = 'time DESC');
SELECT add_compression_policy('currency_snapshots', INTERVAL '7 days');
ALTER TABLE currency_snapshots ALTER COLUMN league SET NOT NULL;
