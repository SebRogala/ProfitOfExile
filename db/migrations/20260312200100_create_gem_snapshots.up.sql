CREATE TABLE gem_snapshots (
    time             TIMESTAMPTZ   NOT NULL,
    name             TEXT          NOT NULL,
    variant          TEXT          NOT NULL,
    chaos            NUMERIC(10,2),
    listings         INTEGER,
    is_transfigured  BOOLEAN       NOT NULL DEFAULT false,
    gem_color        TEXT,
    PRIMARY KEY (time, name, variant)
);

SELECT create_hypertable('gem_snapshots', 'time');

CREATE INDEX idx_gem_snapshots_name_variant ON gem_snapshots (name, variant, time DESC);
