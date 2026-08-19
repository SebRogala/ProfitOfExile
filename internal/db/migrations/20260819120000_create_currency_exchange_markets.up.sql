-- POE-173: storage for GGG's public currency exchange feed.
--
-- `time` is the feed hour itself, not the moment of collection: the feed
-- publishes one immutable snapshot per unix hour and `time` is that hour in UTC,
-- truncated. Re-ingesting an hour is therefore idempotent against the primary
-- key, which is what makes the cursor walk crash-safe.
--
-- No price columns. The feed gives paired quantities and consumers derive the
-- two prices with `exchange.Ratio` (`internal/exchange/normalize.go`); storing a
-- derived float would create a second source of truth for the same number.
--
-- Compression only, no retention policy: a retention policy drops chunks by time
-- alone and would delete a league's start window
-- (see docs/adr/010-archived-league-history-is-retained-indefinitely.md).
--
-- Both tables are league-scoped and both join the wipe set truncated at league
-- rollover (see docs/adr/011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md
-- and docs/LEAGUE-SCHEMA-MIGRATION-RUNBOOK.md).

CREATE TABLE currency_exchange_markets (
    league           TEXT        NOT NULL REFERENCES leagues(id),
    time             TIMESTAMPTZ NOT NULL,
    market_id        TEXT        NOT NULL,
    item_a           TEXT        NOT NULL,
    item_b           TEXT        NOT NULL,
    volume_a         BIGINT      NOT NULL,
    volume_b         BIGINT      NOT NULL,
    lowest_stock_a   BIGINT      NOT NULL,
    lowest_stock_b   BIGINT      NOT NULL,
    highest_stock_a  BIGINT      NOT NULL,
    highest_stock_b  BIGINT      NOT NULL,
    lowest_ratio_a   BIGINT      NOT NULL,
    lowest_ratio_b   BIGINT      NOT NULL,
    highest_ratio_a  BIGINT      NOT NULL,
    highest_ratio_b  BIGINT      NOT NULL,
    price_valid      BOOLEAN     NOT NULL,
    invalid_reason   TEXT        NOT NULL DEFAULT '',
    PRIMARY KEY (league, time, market_id)
);

SELECT create_hypertable('currency_exchange_markets', 'time');

CREATE INDEX idx_currency_exchange_markets_league_market_time
    ON currency_exchange_markets (league, market_id, time DESC);

ALTER TABLE currency_exchange_markets SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'league, market_id',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('currency_exchange_markets', INTERVAL '7 days');

-- The ingest cursor is a dedicated row, not MAX(time): an hour whose markets all
-- belong to other leagues stores zero rows, and MAX(time) would make the walk
-- re-fetch that hour forever. `next_hour` is the unix hour the runner fetches
-- next, advanced in the same transaction as the hour's inserts.
CREATE TABLE currency_exchange_cursor (
    league     TEXT        PRIMARY KEY REFERENCES leagues(id),
    next_hour  BIGINT      NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
