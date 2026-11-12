-- POE-336 (ADR-030): the served currency-exchange plays, persisted so the paged
-- GET /api/currency-exchange/plays filters, sorts, counts and pages them in SQL.
--
-- Written only by exchange.Repository.ReplacePlays, which replaces a league's
-- whole answer (every horizon) in one transaction per recompute; the legacy
-- request keeps reading the in-memory exchange.Cache.
--
-- Both tables are league-scoped and join the rollover wipe set
-- (internal/league/tables.go, ADR-009/011). There is no foreign key between
-- them: ReplacePlays writes the plays before their headers, and a page read
-- treats a horizon without a header as cold.

CREATE TABLE currency_exchange_play_sets (
    league            TEXT             NOT NULL REFERENCES leagues(id),
    horizon           TEXT             NOT NULL,
    hours             INTEGER          NOT NULL,
    divine_chaos_rate DOUBLE PRECISION NOT NULL,
    window_from       TIMESTAMPTZ      NOT NULL,
    window_to         TIMESTAMPTZ      NOT NULL,
    -- NULL when the set covers no feed hour (exchange.LastUpdated reports !ok).
    last_updated      TIMESTAMPTZ,
    computed_at       TIMESTAMPTZ      NOT NULL DEFAULT now(),
    PRIMARY KEY (league, horizon)
);

-- The filter and sort columns are copies of the play's own fields and of its
-- Go-computed display (exchange.Decorate); NULL stores a non-finite number, so
-- a comparison against it is unknown and the row is kept, as the desktop's NaN
-- comparisons keep it. side_* are the distinct leg sides in leg order:
-- side_items/side_categories drive the rules, side_names the search
-- (desktop matchesSearch reads display names, including ids the asset lacks).
CREATE TABLE currency_exchange_plays (
    league             TEXT             NOT NULL REFERENCES leagues(id),
    horizon            TEXT             NOT NULL,
    play_key           TEXT             NOT NULL CHECK (play_key <> ''),
    served_rank        INTEGER          NOT NULL,
    mode               TEXT             NOT NULL,
    leg_count          INTEGER          NOT NULL,
    side_items         TEXT[]           NOT NULL,
    side_categories    TEXT[]           NOT NULL,
    side_names         TEXT[]           NOT NULL,
    first_quote        TEXT             NOT NULL,
    investment         DOUBLE PRECISION,
    run_investment     DOUBLE PRECISION,
    roi                DOUBLE PRECISION,
    roi_pct            DOUBLE PRECISION,
    turnover           DOUBLE PRECISION,
    tick               DOUBLE PRECISION,
    sort_expected      DOUBLE PRECISION,
    sort_roi           DOUBLE PRECISION,
    sort_fastest_hours DOUBLE PRECISION,
    play               JSONB            NOT NULL,
    PRIMARY KEY (league, horizon, play_key)
);

CREATE INDEX idx_currency_exchange_plays_served_rank
    ON currency_exchange_plays (league, horizon, served_rank);
