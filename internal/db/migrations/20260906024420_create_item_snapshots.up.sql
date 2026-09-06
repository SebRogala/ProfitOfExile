-- POE-254: per-tick storage for poe.ninja's item-overview categories.
--
-- One table for all seven categories the collector polls — IncursionTemple,
-- Vial, and the five unique slots (UniqueArmour, UniqueAccessory, UniqueWeapon,
-- UniqueJewel, UniqueFlask). They are one payload shape from one endpoint
-- (economy/stash/current/item/overview), so they get one DDL and one repository
-- path. `category` holds poe.ninja's own type name verbatim; the collector does
-- not invent a taxonomy over it.
--
-- Identity: `ninja_id` is poe.ninja's numeric line id, and it is the key.
-- Measured on the live 2026-09-06 payload for league Allflame: every category
-- has as many distinct ids as lines (IncursionTemple 86/86, Vial 9/9,
-- UniqueArmour 977/977, UniqueAccessory 366/366, UniqueWeapon 643/643,
-- UniqueJewel 166/166, UniqueFlask 39/39), while (name, variant, links) is NOT
-- unique — "Precursor's Emblem" appears five times in UniqueAccessory with no
-- variant and no links, once per ring base (ids 7747, 7765, 7599, 7602, 7762,
-- priced 424.3 / 152 / 109.6 / 100 / 50 chaos). Keying on the natural columns
-- would collapse four of those five rows into an ON CONFLICT DO NOTHING drop.
-- `details_id` is equally unique but is a slug poe.ninja can re-derive; the
-- numeric id is the stable handle.
--
-- `category` is part of the primary key even though no id collided across the
-- seven categories in that payload: one snapshot is not a guarantee, and the
-- key must not depend on poe.ninja allocating ids from one global sequence.
-- It also makes the per-category staleness read (MAX(time) for one category)
-- an index-only prefix scan.
--
-- Numeric columns are DOUBLE PRECISION, following double_corrupt_snapshots
-- (20260824120000), the nearest current hypertable, rather than the older
-- NUMERIC of gem_snapshots/fragment_snapshots: the Go side is float64 either
-- way and unique prices span mirror-tier magnitudes.
--
-- The table takes every scalar the item-overview line carries, because the
-- collector stores what upstream serves and does not decide downstream's
-- filters. That includes the three price axes (`chaos`, `divine`, `exalted` —
-- `exaltedValue` is populated on 166/166 UniqueJewel lines where `divineValue`
-- is null on 36 of them), `item_type` (162/166 UniqueJewel, absent on
-- IncursionTemple), `level_required` (8/166 UniqueJewel) and `stack_size` (9/9
-- Vial). Absent keys land as the Go zero value and store as the column default,
-- the same way `variant` and `links` do.
--
-- `flavourText` is the one scalar deliberately left out: it is flavour prose on
-- the item's art, not a market datum, and nothing that reads this table prices
-- or ranks on it. Nested arrays (explicitModifiers, mutatedModifiers,
-- tradeInfo) are out for the same reason the sparkline is reduced to its
-- totalChange — this is a price table, not a mirror of poe.ninja.
--
-- `listings` is poe.ninja's `listingCount` (total listings), matching
-- gem_snapshots' column name. `sample_count` is poe.ninja's `count` (the number
-- of listings it sampled to derive the price) — named apart from `listings` and
-- away from the SQL COUNT function so a query reading it cannot be misread.
--
-- Compression policy, no retention policy: retention drops chunks by time alone
-- and would delete an archived league's start window
-- (docs/adr/010-archived-league-history-is-retained-indefinitely.md, and
-- 20260724090000_retain_league_history, which removed the 90-day policies the
-- older snapshot tables shipped with).
--
-- League-scoped, so it joins the rollover wipe set at creation
-- (docs/adr/011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md
-- and docs/LEAGUE-SCHEMA-MIGRATION-RUNBOOK.md) and is read and written only
-- through internal/collector's scope-taking repository (ADR-009).

CREATE TABLE item_snapshots (
    league            TEXT             NOT NULL REFERENCES leagues(id),
    time              TIMESTAMPTZ      NOT NULL,
    category          TEXT             NOT NULL,
    ninja_id          BIGINT           NOT NULL,
    details_id        TEXT             NOT NULL DEFAULT '',
    name              TEXT             NOT NULL DEFAULT '',
    variant           TEXT             NOT NULL DEFAULT '',
    links             INTEGER          NOT NULL DEFAULT 0,
    chaos             DOUBLE PRECISION NOT NULL DEFAULT 0,
    divine            DOUBLE PRECISION NOT NULL DEFAULT 0,
    exalted           DOUBLE PRECISION NOT NULL DEFAULT 0,
    listings          INTEGER          NOT NULL DEFAULT 0,
    sample_count      INTEGER          NOT NULL DEFAULT 0,
    stack_size        INTEGER          NOT NULL DEFAULT 0,
    icon              TEXT             NOT NULL DEFAULT '',
    item_class        INTEGER          NOT NULL DEFAULT 0,
    item_type         TEXT             NOT NULL DEFAULT '',
    base_type         TEXT             NOT NULL DEFAULT '',
    level_required    INTEGER          NOT NULL DEFAULT 0,
    sparkline_change  DOUBLE PRECISION NOT NULL DEFAULT 0,
    PRIMARY KEY (league, time, category, ninja_id)
);

SELECT create_hypertable('item_snapshots', 'time');

-- Both reads this table has are "one league, one category, newest first": the
-- collector's startup staleness check and the latest-tick read a consumer runs.
CREATE INDEX idx_item_snapshots_league_category_time
    ON item_snapshots (league, category, time DESC);

-- Segment by (league, category) — seven categories per league, which groups a
-- chunk into a handful of large segments. Segmenting by ninja_id instead would
-- shred every chunk into ~2,300 tiny segments; TimescaleDB's
-- `column "ninja_id" should be used for segmenting or ordering` notice is an
-- accepted trade-off here, the same one gem_features and
-- double_corrupt_snapshots make for `name`.
ALTER TABLE item_snapshots SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'league, category',
    timescaledb.compress_orderby = 'time DESC'
);

SELECT add_compression_policy('item_snapshots', INTERVAL '7 days');
