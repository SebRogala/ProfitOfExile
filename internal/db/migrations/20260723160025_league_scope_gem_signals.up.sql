SELECT remove_compression_policy('gem_signals', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'gem_signals' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE gem_signals ADD COLUMN league TEXT;
UPDATE gem_signals SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM gem_signals signals LEFT JOIN leagues ON leagues.id = signals.league WHERE signals.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'gem_signals contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE gem_signals DROP CONSTRAINT gem_signals_pkey;
ALTER TABLE gem_signals ADD PRIMARY KEY (league, time, name, variant);
DROP INDEX IF EXISTS idx_gem_signals_confidence;
DROP INDEX IF EXISTS idx_gem_signals_variant;
DROP INDEX IF EXISTS idx_gem_signals_tier;
CREATE INDEX idx_gem_signals_league_confidence ON gem_signals (league, time DESC, confidence DESC);
CREATE INDEX idx_gem_signals_league_variant ON gem_signals (league, variant, time DESC);
CREATE INDEX idx_gem_signals_league_tier ON gem_signals (league, tier, time DESC);
ALTER TABLE gem_signals SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, variant', timescaledb.compress_orderby = 'time DESC');
SELECT add_compression_policy('gem_signals', INTERVAL '7 days');
ALTER TABLE gem_signals ALTER COLUMN league SET NOT NULL;
ALTER TABLE gem_signals ADD CONSTRAINT gem_signals_league_fkey
    FOREIGN KEY (league) REFERENCES leagues(id);
