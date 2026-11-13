SELECT remove_compression_policy('trade_lookups', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'trade_lookups' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE trade_lookups ADD COLUMN unique_accounts INTEGER;
-- The count matches the former Go rule: a missing, null or "" account is one seller "".
UPDATE trade_lookups SET
    unique_accounts = CASE WHEN jsonb_typeof(listings) = 'array'
        THEN (SELECT count(DISTINCT COALESCE(e->>'account', '')) FROM jsonb_array_elements(listings) e)
        ELSE 0 END,
    listings = CASE WHEN jsonb_typeof(listings) = 'array'
        THEN (SELECT COALESCE(jsonb_agg(CASE WHEN jsonb_typeof(e) = 'object' THEN e - 'account' ELSE e END ORDER BY ord), '[]'::jsonb)
              FROM jsonb_array_elements(listings) WITH ORDINALITY AS t(e, ord))
        ELSE listings END;
ALTER TABLE trade_lookups ALTER COLUMN unique_accounts SET DEFAULT 0, ALTER COLUMN unique_accounts SET NOT NULL;
ALTER TABLE trade_lookups SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, gem, variant', timescaledb.compress_orderby = 'time DESC');
SELECT add_compression_policy('trade_lookups', INTERVAL '7 days');
