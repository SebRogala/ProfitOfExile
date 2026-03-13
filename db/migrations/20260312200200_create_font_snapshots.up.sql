CREATE TABLE font_snapshots (
    time     TIMESTAMPTZ NOT NULL,
    color    TEXT        NOT NULL,
    variant  TEXT        NOT NULL,
    pool     INTEGER,
    ev       NUMERIC(10,2),
    min_val  NUMERIC(10,2),
    max_val  NUMERIC(10,2),
    PRIMARY KEY (time, color, variant)
);

SELECT create_hypertable('font_snapshots', 'time');
