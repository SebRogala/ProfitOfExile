CREATE TABLE dedication_snapshots (
    time               TIMESTAMPTZ NOT NULL,
    color              TEXT        NOT NULL,
    gem_type           TEXT        NOT NULL,  -- 'skill' or 'transfigured'
    pool               INTEGER,
    winners            INTEGER,
    p_win              DOUBLE PRECISION,
    avg_win_raw        DOUBLE PRECISION,
    ev_raw             DOUBLE PRECISION,
    input_cost         DOUBLE PRECISION,
    profit             DOUBLE PRECISION,
    fonts_to_hit       DOUBLE PRECISION,
    mode               TEXT        NOT NULL DEFAULT 'safe',
    thin_pool_gems     INTEGER     NOT NULL DEFAULT 0,
    liquidity_risk     TEXT        NOT NULL DEFAULT 'LOW',
    pool_breakdown     JSONB,
    low_confidence_gems JSONB,
    PRIMARY KEY (time, color, gem_type, mode)
);

SELECT create_hypertable('dedication_snapshots', 'time');

ALTER TABLE dedication_snapshots
    SET (timescaledb.compress,
         timescaledb.compress_segmentby = 'color, gem_type, mode');

SELECT add_compression_policy('dedication_snapshots', INTERVAL '7 days');
SELECT add_retention_policy('dedication_snapshots', INTERVAL '90 days');

CREATE INDEX idx_dedication_snapshots_color_gemtype_mode
    ON dedication_snapshots (color, gem_type, mode, time DESC);
