# ADR-005: gem_snapshots Unified Row Model — Base and Transfigured Prices Side-by-Side

## Status

Accepted

## Context

POE-19 introduces the TimescaleDB schema for gem price history. The `gem_snapshots` hypertable must store prices for both base gems and their transfigured counterparts collected from poe.ninja every 15 minutes.

The Font of Divine Skill strategy (the first implemented analysis in `/var/www/poe/scripts/font-analysis.mjs`) requires computing ROI by comparing a base gem's cost to the expected value of its transfigured outcome. This comparison is the hot path for profitability queries.

Key considerations:

1. poe.ninja returns base and transfigured gems as separate items in its API response, but they share a canonical gem name (e.g., "Cleave" and "Cleave of Tenuity" share the base name "Cleave").
2. Profitability queries join base gem cost with transfigured gem value. In a separate-row model this requires a self-join on `(name, variant, time)` — expensive on a hypertable with millions of rows.
3. TimescaleDB continuous aggregates work most efficiently on a single hypertable. A self-join query cannot be materialized as a continuous aggregate without significant workarounds.
4. Non-transfigured gems still need to be stored (base cost is always needed); transfigured-only entries are valid when no base gem exists.

## Decision

Store base gem and transfigured gem prices in a single row, with transfigured columns nullable:

```sql
CREATE TABLE gem_snapshots (
    time             TIMESTAMPTZ NOT NULL,
    name             TEXT        NOT NULL,
    variant          TEXT        NOT NULL,     -- "1", "20", "1/20", "20/20"
    base_chaos       NUMERIC(10,2),
    base_listings    INTEGER,
    trans_chaos      NUMERIC(10,2),
    trans_listings   INTEGER,
    is_transfigured  BOOLEAN     NOT NULL DEFAULT false,
    gem_color        TEXT,                     -- RED/GREEN/BLUE/WHITE, NULL if unknown
    PRIMARY KEY (time, name, variant)
);
```

The collector (POE-18) is responsible for pairing base and transfigured prices at write time by matching on `(name, variant)`. When only a base gem exists, `trans_chaos` and `trans_listings` are NULL. When only a transfigured gem exists (no known base), `base_chaos` and `base_listings` are NULL.

ROI queries read a single row — no join required. Continuous aggregates (`gem_snapshots_hourly`, `gem_snapshots_daily`) aggregate both price columns in one materialization pass.

## Consequences

### Positive

- ROI calculation requires no join — single-row read eliminates the most expensive query pattern.
- Continuous aggregates can materialize base and transfigured averages together, enabling efficient hourly/daily rollups.
- Schema is self-documenting: a row with both columns populated is a paired gem; NULL `trans_*` means no transfigured variant was observed.

### Negative

- The collector must pair base and transfigured entries at ingestion time rather than at query time. If the pairing logic has bugs, correcting historical data requires a backfill.
- Rows where only `trans_*` is populated (no known base) are valid but semantically awkward — the `is_transfigured` flag distinguishes them.
- Adding a second transfigured variant per base (unlikely in PoE1 but possible in future patches) would require a schema change.

## Alternatives Considered

### Separate rows for base and transfigured gems

Store each poe.ninja item as its own row. Use a discriminator column or rely on the gem name suffix to distinguish base from transfigured. ROI queries join on `(name, variant, time)`.

**Rejected because**: A self-join on a TimescaleDB hypertable at query time is expensive and cannot be materialized as a continuous aggregate without a custom view layer. The Font of Divine Skill analysis runs this join for every gem in every color bucket — the query cost scales with data volume and would degrade over time.

### Separate tables for base gems and transfigured gems

Two hypertables: `base_gem_snapshots` and `trans_gem_snapshots`. ROI queries join across tables.

**Rejected because**: Cross-hypertable joins in TimescaleDB are not supported by continuous aggregates. Compression and retention policies would also need to be maintained separately for two tables with the same lifecycle. This doubles operational overhead without meaningful benefit over a single unified table.

### JSONB column for all variants

Store all price data for a gem as a JSONB blob keyed by variant.

**Rejected because**: JSONB columns cannot be aggregated or indexed efficiently by TimescaleDB. Continuous aggregates cannot operate on JSONB fields, eliminating the primary benefit of using TimescaleDB.

## References

- [ADR-003](003-no-orm-direct-pgx-queries.md) — direct pgx queries; this schema is designed for efficient raw SQL without ORM abstraction
- [ADR-004](004-database-migration-strategy.md) — migration tooling; this table is created via golang-migrate timestamp-named files
- [POE-19](https://softsolution.youtrack.cloud/issue/POE-19) — TimescaleDB schema design task that prompted this decision
- [POE-18](https://softsolution.youtrack.cloud/issue/POE-18) — price collector that will write to this table
