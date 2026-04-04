# ADR-007: v3 Hybrid Analysis — Unified Pipeline with Trade-Enriched Features

## Status

Accepted

## Context

The analysis system accumulated three parallel, competing subsystems:

1. **v1 (`TrendResult`)**: served all user-facing endpoints (collective, compare, trends) but used hardcoded `marketDepth=1.0`, 2-hour velocity windows, and lacked MID-HIGH/FLOOR tier handling
2. **v2 (`GemSignal`/`GemFeature`)**: correct pipeline with 6-hour velocity, per-variant market depth, and proper 6-tier classification — but only served by unused `/v2/*` endpoints
3. **Trade data**: collected from GGG trade API every ~45-90s for MID-HIGH+ gems, stored in DB and cache, but never fed into signal computation or sellability scoring

This caused:
- Inaccurate sellability scores (hardcoded depth means market depth bonus never fires)
- Three competing `SellConfidence` implementations (v2 `classifySellConfidence`, inline `deriveSellConfidence` in handler, v2 `GemSignal.SellConfidence`)
- Race condition between `RunTrends` and `RunV2` causing font pool and best plays to show different TOP gem counts
- Trade signals (MONOPOLY, staleness, outliers) computed but never consumed

Key considerations:

1. All three systems touch the same data (gem prices, signals, tiers) but produce inconsistent results
2. Trade data provides signals (seller concentration, listing staleness, price outliers) that ninja cannot — these directly affect sellability and confidence
3. The v2 pipeline is already correct but isolated — promoting it eliminates the v1/v2 divergence
4. User-submitted trade data (via desktop app) can arrive for any gem, not just background-refreshed tiers

## Decision

Consolidate into a single v3 pipeline by promoting v2 and integrating trade data at the feature computation layer.

### Kill v1

Delete `RunTrends`, `AnalyzeTrends`, `TrendResult` cache fields, `deriveSellConfidence`, and all v1-specific repository methods. All endpoints (`/api/analysis/collective`, `/compare`, `/trends`) serve from `GemSignal`/`GemFeature` exclusively. The `sellability()` function is retained (called by v2 path in `ComputeGemSignals`).

### Trade data in the analysis layer, not handlers

Trade data is wired into `ComputeGemFeatures` via a nil-safe `*trade.TradeCache` parameter. Seven trade fields are added to `GemFeature`. The analysis layer owns data source selection — HTTP handlers never decide which price source to use.

This was chosen over handler-level enrichment (the prior pattern where `CompareAnalysis` bolt-on attached trade data to the response) because:
- Signal computation needs trade inputs (MONOPOLY affects sellability scoring)
- Keeping source selection in handlers would duplicate the freshness/availability logic across every endpoint
- The analysis layer already runs as a background pipeline — adding trade cache reads is a natural fit

### Freshness-based consumption, not tier-gated

Trade data is consumed for any gem that has fresh data, regardless of tier. Freshness degrades in tiers: <5min full weight, 5-30min 75%, 30-90min 50%, >90min ignored. The MID+ tier threshold only applies to background refresh scheduling — user-submitted trade data from any tier is used when fresh.

### Delayed recomputation

A single additional `RunV2` fires 15 minutes after each ninja snapshot event, picking up trade data accumulated since the snapshot. Next ninja event cancels any pending timer. This gives two recomputations per ~30-minute ninja cycle.

## Consequences

### Positive

- Single source of truth for all signal, sellability, and confidence data — eliminates v1/v2 divergence
- Trade signals (MONOPOLY, staleness, outliers) directly improve sellability and confidence accuracy
- Tier consistency guaranteed — font pool and best plays read from the same classification (race condition eliminated)
- Foundation for future trade-primary pricing as user base grows and more trade data flows in

### Negative

- `BaseListings` and `BaseVelocity` fields lost in the migration (v1 computed these from separate base gem queries; v2 pipeline does not) — requires future work to restore
- Big-bang delivery means all endpoints change simultaneously — no gradual rollout possible
- Trade data freshness thresholds (300s/1800s/5400s) are magic numbers repeated across multiple files — risk of divergence when tuning

## Alternatives Considered

### A) Phased rollout with feature flag

Serve both v1 and v3 simultaneously, switch per-endpoint via feature flag.

**Rejected because**: the core problem is three competing systems — running v1 and v3 in parallel adds a fourth. The v1/v2 race condition persists until v1 is fully removed. The overhead of maintaining both paths during rollout outweighs the safety benefit for a system with no external API consumers (only our own frontend).

### B) Bottom-up: enrich v2 with trade first, then swap endpoints

Wire trade data into v2 while v1 still serves endpoints. Validate via `/v2/*` endpoints. Then swap.

**Rejected because**: longer period of v1/v2 coexistence, and the tier consistency bug persists until the final swap. The v2 endpoints had no real users, so validation would be synthetic rather than practical.

### C) Handler-level trade enrichment (extend existing pattern)

Keep trade data integration in HTTP handlers (like the existing compare endpoint bolt-on). Each handler fetches from trade cache and enriches the response.

**Rejected because**: signal computation needs trade inputs at the analysis layer (MONOPOLY must affect sellability scoring, not just be displayed). Handler-level enrichment would duplicate freshness logic across every endpoint and prevent trade signals from influencing the core ranking algorithm.

## References

- [POE-101](https://softsolution.youtrack.cloud/issue/POE-101) — epic that prompted this decision
- [ADR-002](002-internal-architecture-hexagonal-cqrs-vertical-slice.md) — hexagonal architecture that this pipeline follows
