-- Seller account names removed by the up migration are not restored.
SELECT remove_compression_policy('trade_lookups', if_exists => TRUE);
DO $$
DECLARE chunk REGCLASS;
BEGIN
    FOR chunk IN SELECT format('%I.%I', chunk_schema, chunk_name)::REGCLASS FROM timescaledb_information.chunks WHERE hypertable_name = 'trade_lookups' AND is_compressed LOOP
        PERFORM decompress_chunk(chunk);
    END LOOP;
END
$$;
ALTER TABLE trade_lookups DROP COLUMN unique_accounts;
ALTER TABLE trade_lookups SET (timescaledb.compress, timescaledb.compress_segmentby = 'league, gem, variant', timescaledb.compress_orderby = 'time DESC');
SELECT add_compression_policy('trade_lookups', INTERVAL '7 days');
