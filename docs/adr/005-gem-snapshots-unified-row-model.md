# ADR-005: gem_snapshots Separate Row Model — One Row Per Gem Observation

## Status

Accepted

## Context

POE-19 introduces the TimescaleDB schema for gem price history. The `gem_snapshots` hypertable must store prices for both base gems and their transfigured counterparts collected from poe.ninja every 15 minutes.

The Font of Divine Skill strategy (the first implemented analysis in `/var/www/poe/scripts/font-analysis.mjs`) requires computing ROI by comparing a base gem's cost to the expected value of its transfigured outcome. This comparison is the hot path for profitability queries.

Key considerations:

1. poe.ninja returns base and transfigured gems as separate items in its API response, but they share a canonical gem name (e.g., "Cleave" and "Cleave of Tenuity" share the base name "Cleave").
2. Profitability queries join base gem cost with transfigured gem value. In a separate-row model this requires a self-join on `(name, variant, time)`.
3. TimescaleDB continuous aggregates work most efficiently on a single hypertable.
4. Non-transfigured gems still need to be stored (base cost is always needed); transfigured-only entries are valid when no base gem exists.

## Decision

Store each gem observation as its own row, with `is_transfigured` and `gem_color` columns to distinguish base from transfigured entries:

```sql
CREATE TABLE gem_snapshots (
    time             TIMESTAMPTZ   NOT NULL,
    name             TEXT          NOT NULL,
    variant          TEXT          NOT NULL,     -- "1", "20", "1/20", "20/20"
    chaos            NUMERIC(10,2),
    listings         INTEGER,
    is_transfigured  BOOLEAN       NOT NULL DEFAULT false,
    gem_color        TEXT,                       -- RED/GREEN/BLUE/WHITE, NULL if unknown
    PRIMARY KEY (time, name, variant)
);
```

The collector (POE-18) writes one row per gem per snapshot interval. Base and transfigured gems are stored as separate rows. ROI queries that need to compare base and transfigured prices use a self-join on the base gem name (derived by stripping the transfigured suffix).

Continuous aggregates (`gem_snapshots_hourly`, `gem_snapshots_daily`) aggregate per row, and the `gem_color` column enables grouping by color without joining the `gem_colors` lookup table.

## Consequences

### Positive

- Simple, flat schema that maps 1:1 to the poe.ninja API response — no pairing logic needed at ingestion time.
- The collector writes each gem independently, reducing write-path complexity and eliminating pairing bugs.
- Continuous aggregates can materialize averages per gem, and the `gem_color` column enables color-based grouping in queries.
- Schema naturally supports gems that exist only as base or only as transfigured without NULL column semantics.

### Negative

- ROI calculation requires a self-join on `(base_name, variant, time)` to pair base and transfigured prices. This is more expensive than a single-row read but is mitigated by the `idx_gem_snapshots_name_variant` index.
- The self-join cannot be directly materialized as a TimescaleDB continuous aggregate. ROI rollups require a custom view or application-level computation on top of the hourly/daily aggregates.
- `gem_color` is denormalized from the `gem_colors` table into each snapshot row. If a gem's color is corrected, historical rows retain the old value (acceptable — color assignments are stable across a league).

## Alternatives Considered

### Unified row model (base + transfigured prices side-by-side)

Store base and transfigured prices in a single row with `base_chaos`, `base_listings`, `trans_chaos`, `trans_listings` columns. ROI queries read a single row — no join required.

**Rejected because**: The collector must pair base and transfigured entries at write time, adding complexity and fragility. If pairing logic has bugs, correcting historical data requires a backfill. The simpler separate-row model pushes pairing to query time where it can be corrected without data migration.

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
