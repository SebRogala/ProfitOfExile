-- POE-125: per-tick storage for the double-corruption (Doryani's Institute) EV
-- calculator.
--
-- One row per (gem, uncorrupted input variant) per tick, mirroring
-- gem_features' per-name-variant shape rather than dedication_snapshots'
-- per-pool shape: double corruption is priced for an individual gem you already
-- own, not for a pool you draw from.
--
-- `input_variant` is the UNCORRUPTED variant fed to the altar ("20/20"), never a
-- corrupted one. It is part of the primary key because each input variant is its
-- own market with its own outcome distribution and its own EV, and the two are
-- never merged (the per-variant rule).
--
-- `outcomes` carries the whole outcome-cell breakdown — probability mass, price,
-- risk-adjusted price, listings, priced/thin flags per cell — as JSONB rather
-- than as a second table. The breakdown is read whole with its parent row and
-- never queried across rows, the same reason dedication_snapshots stores
-- pool_breakdown and low_confidence_gems this way.
--
-- `model` records which outcome model produced the row. The probabilities are
-- sourced from community documentation, not from GGG, so a corrected model must
-- be distinguishable from the estimated one in stored history rather than
-- silently overwriting it. See the outcome-model block in
-- internal/lab/doublecorrupt.go for the weights and the open questions.
--
-- Compression policy, no retention policy: a retention policy drops chunks by
-- time alone and would delete a league's start window
-- (docs/adr/010-archived-league-history-is-retained-indefinitely.md).
--
-- League-scoped, so it joins the rollover wipe set at creation
-- (docs/adr/011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md
-- and docs/LEAGUE-SCHEMA-MIGRATION-RUNBOOK.md).

CREATE TABLE double_corrupt_snapshots (
    league                TEXT             NOT NULL REFERENCES leagues(id),
    time                  TIMESTAMPTZ      NOT NULL,
    name                  TEXT             NOT NULL,
    input_variant         TEXT             NOT NULL,
    color                 TEXT             NOT NULL DEFAULT '',
    input_cost            DOUBLE PRECISION NOT NULL DEFAULT 0,
    temple_overhead_chaos DOUBLE PRECISION NOT NULL DEFAULT 0,
    has_vaal_version      BOOLEAN          NOT NULL DEFAULT FALSE,
    ev                    DOUBLE PRECISION NOT NULL DEFAULT 0,
    ev_raw                DOUBLE PRECISION NOT NULL DEFAULT 0,
    profit                DOUBLE PRECISION NOT NULL DEFAULT 0,
    priced_probability    DOUBLE PRECISION NOT NULL DEFAULT 0,
    unpriced_probability  DOUBLE PRECISION NOT NULL DEFAULT 0,
    thin_outcome_cells    INTEGER          NOT NULL DEFAULT 0,
    liquidity_risk        TEXT             NOT NULL DEFAULT 'LOW',
    model                 TEXT             NOT NULL DEFAULT 'estimated',
    outcomes              JSONB            NOT NULL DEFAULT '[]'::jsonb,
    PRIMARY KEY (league, time, name, input_variant)
);

SELECT create_hypertable('double_corrupt_snapshots', 'time');

-- The ranking read ("most profitable gems to double-corrupt right now") selects
-- one league's latest tick at one input variant and orders by profit.
CREATE INDEX idx_double_corrupt_snapshots_league_variant_time
    ON double_corrupt_snapshots (league, input_variant, time DESC);

-- Segment by (league, input_variant) and order by time, the same shape
-- gem_features uses for the same (league, time, name, variant) key. TimescaleDB
-- emits `column "name" should be used for segmenting or ordering` here, as it
-- does for gem_features: segmenting by a ~2000-value gem name would shred every
-- chunk into tiny segments. Accepted trade-off, not an oversight.
ALTER TABLE double_corrupt_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'league, input_variant',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('double_corrupt_snapshots', INTERVAL '7 days');
