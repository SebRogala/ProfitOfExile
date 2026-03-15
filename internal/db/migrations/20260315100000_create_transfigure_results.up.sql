CREATE TABLE transfigure_results (
    time                  TIMESTAMPTZ    NOT NULL,
    base_name             TEXT           NOT NULL,
    transfigured_name     TEXT           NOT NULL,
    variant               TEXT           NOT NULL,
    base_price            NUMERIC(10,2),
    transfigured_price    NUMERIC(10,2),
    roi                   NUMERIC(10,2),
    roi_pct               NUMERIC(10,2),
    base_listings         INTEGER,
    transfigured_listings INTEGER,
    gem_color             TEXT,
    confidence            TEXT           NOT NULL DEFAULT 'LOW',
    PRIMARY KEY (time, transfigured_name, variant)
);

SELECT create_hypertable('transfigure_results', 'time');

CREATE INDEX idx_transfigure_results_roi ON transfigure_results (time DESC, roi DESC);
CREATE INDEX idx_transfigure_results_variant ON transfigure_results (variant, time DESC);

ALTER TABLE transfigure_results SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'variant',
    timescaledb.compress_orderby = 'time DESC, roi DESC'
);

SELECT add_compression_policy('transfigure_results', INTERVAL '7 days');
SELECT add_retention_policy('transfigure_results', INTERVAL '120 days');
