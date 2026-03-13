-- Recreate exchange_snapshots
CREATE TABLE exchange_snapshots (
    time      TIMESTAMPTZ   NOT NULL,
    name      TEXT          NOT NULL,
    chaos     NUMERIC(10,2),
    listings  INTEGER,
    PRIMARY KEY (time, name)
);

SELECT create_hypertable('exchange_snapshots', 'time');

CREATE INDEX idx_exchange_snapshots_name ON exchange_snapshots (name, time DESC);

-- Recreate gcp_snapshots
CREATE TABLE gcp_snapshots (
    time  TIMESTAMPTZ NOT NULL PRIMARY KEY,
    chaos NUMERIC(10,2)
);

SELECT create_hypertable('gcp_snapshots', 'time');

-- Restore compression settings
ALTER TABLE exchange_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'name',
    timescaledb.compress_orderby = 'time DESC'
);

ALTER TABLE gcp_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('exchange_snapshots', INTERVAL '7 days');
SELECT add_compression_policy('gcp_snapshots', INTERVAL '7 days');

SELECT add_retention_policy('exchange_snapshots', INTERVAL '90 days');
SELECT add_retention_policy('gcp_snapshots', INTERVAL '90 days');
