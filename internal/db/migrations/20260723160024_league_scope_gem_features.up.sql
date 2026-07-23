SELECT remove_compression_policy('gem_features', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'gem_features' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE gem_features ADD COLUMN league TEXT;
UPDATE gem_features SET league = 'Mirage' WHERE league IS NULL;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM gem_features features LEFT JOIN leagues ON leagues.id = features.league WHERE features.league IS NULL OR leagues.id IS NULL) THEN
        RAISE EXCEPTION 'gem_features contains a null or unknown league after backfill';
    END IF;
END $$;
ALTER TABLE gem_features DROP CONSTRAINT gem_features_pkey;
ALTER TABLE gem_features ADD PRIMARY KEY (league, time, name, variant);
DROP INDEX IF EXISTS idx_gem_features_tier;
DROP INDEX IF EXISTS idx_gem_features_variant;
CREATE INDEX idx_gem_features_league_tier ON gem_features (league, time DESC, tier);
CREATE INDEX idx_gem_features_league_variant ON gem_features (league, variant, time DESC);
ALTER TABLE gem_features SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, variant', timescaledb.compress_orderby = 'time DESC');
SELECT add_compression_policy('gem_features', INTERVAL '7 days');
ALTER TABLE gem_features ALTER COLUMN league SET NOT NULL;
