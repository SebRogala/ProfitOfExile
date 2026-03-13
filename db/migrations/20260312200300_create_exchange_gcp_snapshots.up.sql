CREATE TABLE exchange_snapshots (
    time      TIMESTAMPTZ NOT NULL,
    gem_name  TEXT        NOT NULL,
    chaos     NUMERIC(10,2),
    listings  INTEGER,
    PRIMARY KEY (time, gem_name)
);

SELECT create_hypertable('exchange_snapshots', 'time');

CREATE TABLE gcp_snapshots (
    time  TIMESTAMPTZ NOT NULL PRIMARY KEY,
    chaos NUMERIC(10,2)
);

SELECT create_hypertable('gcp_snapshots', 'time');
