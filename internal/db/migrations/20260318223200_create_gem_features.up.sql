CREATE TABLE gem_features (
    time                TIMESTAMPTZ      NOT NULL,
    name                TEXT             NOT NULL,
    variant             TEXT             NOT NULL,
    chaos               DOUBLE PRECISION NOT NULL DEFAULT 0,
    listings            INTEGER          NOT NULL DEFAULT 0,
    tier                TEXT             NOT NULL DEFAULT 'LOW',
    vel_short_price     DOUBLE PRECISION NOT NULL DEFAULT 0,
    vel_short_listing   DOUBLE PRECISION NOT NULL DEFAULT 0,
    vel_med_price       DOUBLE PRECISION NOT NULL DEFAULT 0,
    vel_med_listing     DOUBLE PRECISION NOT NULL DEFAULT 0,
    vel_long_price      DOUBLE PRECISION NOT NULL DEFAULT 0,
    vel_long_listing    DOUBLE PRECISION NOT NULL DEFAULT 0,
    cv                  DOUBLE PRECISION NOT NULL DEFAULT 0,
    hist_position       DOUBLE PRECISION NOT NULL DEFAULT 50,
    high_7d             DOUBLE PRECISION NOT NULL DEFAULT 0,
    low_7d              DOUBLE PRECISION NOT NULL DEFAULT 0,
    flood_count         INTEGER          NOT NULL DEFAULT 0,
    crash_count         INTEGER          NOT NULL DEFAULT 0,
    listing_elasticity  DOUBLE PRECISION NOT NULL DEFAULT 0,
    relative_price      DOUBLE PRECISION NOT NULL DEFAULT 0,
    relative_listings   DOUBLE PRECISION NOT NULL DEFAULT 0,
    PRIMARY KEY (time, name, variant)
);

SELECT create_hypertable('gem_features', 'time');

CREATE INDEX idx_gem_features_tier ON gem_features (time DESC, tier);
CREATE INDEX idx_gem_features_variant ON gem_features (variant, time DESC);

ALTER TABLE gem_features SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'variant',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('gem_features', INTERVAL '7 days');
SELECT add_retention_policy('gem_features', INTERVAL '120 days');
