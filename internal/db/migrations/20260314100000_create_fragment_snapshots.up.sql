CREATE TABLE fragment_snapshots (
    time              TIMESTAMPTZ    NOT NULL,
    fragment_id       TEXT           NOT NULL,
    chaos             NUMERIC(20,8),
    volume            NUMERIC(20,4),
    sparkline_change  NUMERIC(10,2),
    PRIMARY KEY (time, fragment_id)
);

SELECT create_hypertable('fragment_snapshots', 'time');

CREATE INDEX idx_fragment_snapshots_id_time ON fragment_snapshots (fragment_id, time DESC);

ALTER TABLE fragment_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'fragment_id',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('fragment_snapshots', INTERVAL '7 days');

SELECT add_retention_policy('fragment_snapshots', INTERVAL '90 days');
