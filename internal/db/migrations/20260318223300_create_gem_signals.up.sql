CREATE TABLE gem_signals (
    time                TIMESTAMPTZ      NOT NULL,
    name                TEXT             NOT NULL,
    variant             TEXT             NOT NULL,
    signal              TEXT             NOT NULL DEFAULT 'STABLE',
    confidence          INTEGER          NOT NULL DEFAULT 0,
    sell_urgency        TEXT             NOT NULL DEFAULT '',
    sell_reason         TEXT             NOT NULL DEFAULT '',
    sellability         INTEGER          NOT NULL DEFAULT 50,
    sellability_label   TEXT             NOT NULL DEFAULT 'MODERATE',
    window_signal       TEXT             NOT NULL DEFAULT 'CLOSED',
    advanced_signal     TEXT             NOT NULL DEFAULT '',
    phase_modifier      DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    recommendation      TEXT             NOT NULL DEFAULT '',
    tier                TEXT             NOT NULL DEFAULT 'LOW',
    PRIMARY KEY (time, name, variant)
);

SELECT create_hypertable('gem_signals', 'time');

CREATE INDEX idx_gem_signals_confidence ON gem_signals (time DESC, confidence DESC);
CREATE INDEX idx_gem_signals_variant ON gem_signals (variant, time DESC);
CREATE INDEX idx_gem_signals_tier ON gem_signals (tier, time DESC);

ALTER TABLE gem_signals SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'variant',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('gem_signals', INTERVAL '7 days');
SELECT add_retention_policy('gem_signals', INTERVAL '120 days');
