CREATE TABLE exchange_snapshots (
    time      TIMESTAMPTZ   NOT NULL,
    name      TEXT          NOT NULL,
    chaos     NUMERIC(10,2),
    listings  INTEGER,
    PRIMARY KEY (time, name)
);

SELECT create_hypertable('exchange_snapshots', 'time');

CREATE INDEX idx_exchange_snapshots_name ON exchange_snapshots (name, time DESC);

CREATE TABLE gcp_snapshots (
    time  TIMESTAMPTZ NOT NULL PRIMARY KEY,
    chaos NUMERIC(10,2)
);

SELECT create_hypertable('gcp_snapshots', 'time');
