CREATE TABLE quality_results (
    time         TIMESTAMPTZ    NOT NULL,
    name         TEXT           NOT NULL,
    level        INTEGER        NOT NULL,
    buy_price    NUMERIC(10,2)  NOT NULL DEFAULT 0,
    price_q20    NUMERIC(10,2)  NOT NULL DEFAULT 0,
    roi_4        NUMERIC(10,2)  NOT NULL DEFAULT 0,
    roi_6        NUMERIC(10,2)  NOT NULL DEFAULT 0,
    roi_10       NUMERIC(10,2)  NOT NULL DEFAULT 0,
    roi_15       NUMERIC(10,2)  NOT NULL DEFAULT 0,
    avg_roi      NUMERIC(10,2)  NOT NULL DEFAULT 0,
    gcp_price    NUMERIC(10,2)  NOT NULL DEFAULT 0,
    listings_0   INTEGER        NOT NULL DEFAULT 0,
    listings_20  INTEGER        NOT NULL DEFAULT 0,
    gem_color    TEXT           NOT NULL DEFAULT '',
    confidence   TEXT           NOT NULL DEFAULT 'LOW' CHECK (confidence IN ('OK', 'LOW')),
    PRIMARY KEY (time, name, level)
);

SELECT create_hypertable('quality_results', 'time');

CREATE INDEX idx_quality_results_roi ON quality_results (time DESC, avg_roi DESC);
CREATE INDEX idx_quality_results_level ON quality_results (level, time DESC);

ALTER TABLE quality_results SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'level',
    timescaledb.compress_orderby = 'time DESC, avg_roi DESC'
);

SELECT add_compression_policy('quality_results', INTERVAL '7 days');
SELECT add_retention_policy('quality_results', INTERVAL '120 days');
