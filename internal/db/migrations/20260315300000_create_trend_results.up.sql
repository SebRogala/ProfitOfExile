CREATE TABLE trend_results (
    time              TIMESTAMPTZ NOT NULL,
    name              TEXT        NOT NULL,
    variant           TEXT        NOT NULL,
    gem_color         TEXT        NOT NULL DEFAULT '',
    current_price     NUMERIC(10,2) NOT NULL DEFAULT 0,
    current_listings  INTEGER     NOT NULL DEFAULT 0,
    price_velocity    NUMERIC(10,4) NOT NULL DEFAULT 0,
    listing_velocity  NUMERIC(10,4) NOT NULL DEFAULT 0,
    cv                NUMERIC(10,2) NOT NULL DEFAULT 0,
    signal            TEXT        NOT NULL DEFAULT 'STABLE',
    hist_position     NUMERIC(5,2) NOT NULL DEFAULT 0,
    price_high_7d     NUMERIC(10,2) NOT NULL DEFAULT 0,
    price_low_7d      NUMERIC(10,2) NOT NULL DEFAULT 0,
    PRIMARY KEY (time, name, variant)
);

SELECT create_hypertable('trend_results', 'time');

CREATE INDEX idx_trend_results_signal ON trend_results (time DESC, signal);
CREATE INDEX idx_trend_results_variant ON trend_results (variant, time DESC);

ALTER TABLE trend_results SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'variant',
    timescaledb.compress_orderby = 'time DESC, name'
);

SELECT add_compression_policy('trend_results', INTERVAL '7 days');
SELECT add_retention_policy('trend_results', INTERVAL '120 days');
