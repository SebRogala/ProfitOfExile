-- Dedication analysis becomes per-variant: 21/23c and 21/20c are separate
-- markets with their own pools, tiers, input costs and EV, so a row is only
-- identified once the variant is part of its key. Without variant in the
-- primary key the second variant of every (time, color, gem_type, mode) would
-- be swallowed by the writer's ON CONFLICT DO NOTHING.
--
-- Existing rows were all computed over 21/23c, which is what the backfill says.
SELECT remove_compression_policy('dedication_snapshots', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'dedication_snapshots' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;

ALTER TABLE dedication_snapshots ADD COLUMN IF NOT EXISTS variant TEXT;
UPDATE dedication_snapshots SET variant = '21/23c' WHERE variant IS NULL;

ALTER TABLE dedication_snapshots DROP CONSTRAINT dedication_snapshots_pkey;
ALTER TABLE dedication_snapshots ADD PRIMARY KEY (league, time, variant, color, gem_type, mode);

DROP INDEX IF EXISTS idx_dedication_snapshots_league_color_gemtype_mode;
CREATE INDEX idx_dedication_snapshots_league_variant_color_gemtype_mode
    ON dedication_snapshots (league, variant, color, gem_type, mode, time DESC);

ALTER TABLE dedication_snapshots SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, variant, color, gem_type, mode');
SELECT add_compression_policy('dedication_snapshots', INTERVAL '7 days');

ALTER TABLE dedication_snapshots ALTER COLUMN variant SET NOT NULL;
