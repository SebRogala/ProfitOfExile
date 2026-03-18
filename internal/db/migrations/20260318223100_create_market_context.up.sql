CREATE TABLE market_context (
    time                TIMESTAMPTZ      NOT NULL,
    price_percentiles   JSONB            NOT NULL DEFAULT '{}',
    listing_percentiles JSONB            NOT NULL DEFAULT '{}',
    velocity_mean       DOUBLE PRECISION NOT NULL DEFAULT 0,
    velocity_sigma      DOUBLE PRECISION NOT NULL DEFAULT 0,
    listing_vel_mean    DOUBLE PRECISION NOT NULL DEFAULT 0,
    listing_vel_sigma   DOUBLE PRECISION NOT NULL DEFAULT 0,
    total_gems          INTEGER          NOT NULL DEFAULT 0,
    total_listings      INTEGER          NOT NULL DEFAULT 0,
    tier_boundaries     JSONB            NOT NULL DEFAULT '{}',
    hourly_bias         JSONB            NOT NULL DEFAULT '[]',
    weekday_bias        JSONB            NOT NULL DEFAULT '[]',
    PRIMARY KEY (time)
);

SELECT create_hypertable('market_context', 'time');

CREATE INDEX idx_market_context_time ON market_context (time DESC);

ALTER TABLE market_context SET (
    timescaledb.compress,
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('market_context', INTERVAL '7 days');
SELECT add_retention_policy('market_context', INTERVAL '120 days');
