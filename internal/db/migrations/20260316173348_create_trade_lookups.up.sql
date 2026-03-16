CREATE TABLE trade_lookups (
    time            TIMESTAMPTZ NOT NULL,
    gem             TEXT NOT NULL,
    variant         TEXT NOT NULL,
    total_listings  INTEGER,
    price_floor     NUMERIC(10,2),
    price_ceiling   NUMERIC(10,2),
    median_top10    NUMERIC(10,2),
    divine_rate     NUMERIC(10,2),
    source          TEXT NOT NULL DEFAULT 'user',
    listings        JSONB,
    PRIMARY KEY (time, gem, variant)
);

SELECT create_hypertable('trade_lookups', 'time');

ALTER TABLE trade_lookups SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'gem,variant',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('trade_lookups', INTERVAL '7 days');

CREATE INDEX idx_trade_lookups_gem_variant_time ON trade_lookups (gem, variant, time DESC);
