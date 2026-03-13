CREATE TABLE currency_snapshots (
    time              TIMESTAMPTZ    NOT NULL,
    currency_id       TEXT           NOT NULL,
    chaos             NUMERIC(20,8),
    volume            NUMERIC,
    sparkline_change  NUMERIC(10,2),
    PRIMARY KEY (time, currency_id)
);

SELECT create_hypertable('currency_snapshots', 'time');

CREATE INDEX idx_currency_snapshots_id_time ON currency_snapshots (currency_id, time DESC);

-- Compression policy matching existing hypertables
ALTER TABLE currency_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'currency_id',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('currency_snapshots', INTERVAL '7 days');

-- Retention policy matching existing hypertables
SELECT add_retention_policy('currency_snapshots', INTERVAL '90 days');
