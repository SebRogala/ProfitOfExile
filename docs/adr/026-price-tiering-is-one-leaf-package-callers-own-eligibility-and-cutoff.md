---
uid: d2525ce4-c6c3-4b1e-b1a1-bcf86eb48f2b
---

# ADR-026: Price Tiering Is One Leaf Package; Callers Own Eligibility and Cutoff

## Status

Accepted (2026-10-10, Operator rescope on the Harvest tiers lane: "we need to
make it SSOT and universal"). Shipped on branch `harvest-tiers` (`a39b733c`,
"tiers: move the tier algorithm to internal/price/tier").

## Enforcement

Tier: required

Scope: internal/price/tier and its Lab, Exchange and Harvest callers.

Summary: Keep tiering in one stdlib-only leaf package; callers own eligibility and cutoff, groups remain separate markets and labels remain flags.

## Context

The unified tier system (TOP, HIGH, MID-HIGH, MID, LOW, FLOOR, CHAOTIC) lived in
`internal/lab/classification.go` as two near-copies — a Font pipeline and a
filtered pipeline used by Dedication and double-corrupt — and both applied a
hard-coded `g.Chaos <= 5` cutoff at eight sites. Harvest flipping (POE-283)
needs the same tiers for Oils, Catalysts and other families, and the Operator
ruled that Harvest has no cutoff: "fow those flips, there should be no cutoff -
that's the place that the cheapest ones would be taken as a feed part" (P49).
With the 5c cutoff, 14 sub-5c Harvest items had no tier and could not be fed.

A non-lab consumer cannot import `internal/lab`: `internal/exchange`'s
`boundary_test.go` forbids it, and a future Harvest server read model may sit
beside exchange. A second copy of the algorithm for Harvest would drift. The
cutoff itself is the thing AGENTS.md § Modeling rules ("Absolute cutoffs on
price or listing count are suspect", POE-134) and
[ADR-017](017-no-default-engine-floor-may-hide-a-live-market.md) warn about, so
it cannot be the shared default.

## Decision

1. **One home, a stdlib-only leaf.** The tier algorithm lives in
   `internal/price/tier` (package `tier`). It imports the standard library only
   (`math`, `sort`) and nothing from `profitofexile/...`; this is enforced by
   `internal/price/tier/boundary_test.go`. `internal/lab`, `cmd/harvest-tiers`,
   `internal/exchange` and a future Harvest server package may import it. The
   `internal/price/` umbrella separates price tiers from temple room tiers and
   merc support tiers, which also use the word "tier".

2. **Callers own the policy; the zero `Policy` is no cutoff.**
   `tier.Policy{CutoffChaos float64; HasCutoff bool; DepthUnknown bool}`. With
   `HasCutoff`, an item priced `<= CutoffChaos` takes no part in any step. The
   zero value applies no cutoff, because a sentinel `CutoffChaos = 0` would make
   a legal zero-price cutoff ambiguous. `DepthUnknown` skips low-confidence
   detection, so no item is low-confidence or CHAOTIC; Harvest uses it because
   it carries no listing depth. Lab's 5c is Lab's own parameter,
   `labTierCutoffChaos = 5` in `internal/lab/classification.go`, applied through
   `labPolicy` (POE-134).

3. **Callers own eligibility; groups are markets.** `tier.Classify(items []Item,
   p Policy) Result` takes pre-filtered items. `Item.Group` is the market — a gem
   variant, a Harvest family — and is required; each group is classified on its
   own and pools never mix. Lab pre-filters with its own predicates (Font
   outcomes, Dedication outcomes, corrupted, double-corrupt inputs) and maps
   `GemPrice` to `Item{Group: Variant, Key: Name, Price: Chaos, Depth: Listings}`.
   The Font and filtered pipelines are one pipeline; Lab keeps thin adapters.

4. **`lab.TierBoundaries` is an alias, and its JSON is unchanged.**
   `type TierBoundaries = tier.Boundaries`, with the same tags
   (`json:"boundaries"`, `json:"names,omitempty"`). The shape stored in market
   context rows (`internal/lab/repository.go`) and served by
   `internal/server/handlers/analysis.go` is byte-identical. `lab.TierNames` is
   `tier.Order` and `lab.ClassifyTier` forwards to `tier.Of`.

5. **Exchange may import it; tier labels there are flags.** `internal/exchange`
   still never imports `internal/lab` or `internal/collector`, but it may import
   `internal/price/tier`. Any tier label it attaches is a mark, never a sort key
   or a hiding filter ([ADR-018](018-flags-mark-they-never-order.md): flags mark; they never order).

6. **Lab output is unchanged by the move.** A characterisation golden
   (`internal/lab/classification_characterisation_test.go`, corpus under
   `internal/lab/testdata/classification/`) was committed before the move and
   is byte-unchanged after it. Font, Dedication and market context compare
   exactly. Double-corrupt compares its probability-derived floats (outcome
   probability, EV, raw EV, profit, priced and unpriced mass) within 1e-12
   relative, using |got−want| ≤ 1e-12 × max(1, |want|), and every other
   field exactly, because
   `doubleCorruptCellDistribution` (`internal/lab/doublecorrupt.go`) sums float
   probabilities over `range` of maps, in Go map order, so the last bits can
   differ between runs. That nondeterminism predates this ADR; removing it
   (summing over a sorted state slice) would change Lab output and is a
   separate change.

Not changed: `lab.ClassificationResult.TopBoundary` stays an empty map, and the
legacy `DetectTierBoundaries*` functions in `internal/lab/tiers.go` keep their
own copy of `tierFallback`.

## Consequences

- One algorithm serves Lab and Harvest; a change to tiering is made once, in
  `internal/price/tier`, and reaches every market.
- A new market picks its policy explicitly. Leaving it zero means no cutoff,
  which follows ADR-017; a cutoff is a visible, owned parameter of the caller.
- Harvest's fixture tiers are regenerated by `go run ./cmd/harvest-tiers`
  through this package, so the 14 sub-5c Harvest items land in FLOOR.
- Reversal is costly: the persisted and served `TierBoundaries` JSON and every
  caller bind to the package's API.
- Duplicated tier-name vocabulary elsewhere (server trade tick, backtest, font
  predicates, Svelte components) is not moved here; those copies should
  reference `tier.Order` and the name constants when their files are next
  touched.

## Alternatives Considered

### (a) A cutoff parameter inside `internal/lab/classification.go`

**Rejected because**: a non-lab consumer cannot import lab — exchange's boundary
test forbids it, and the Harvest server read model may sit beside exchange.

### (b) Harvest-only fixture regeneration with a scratch harness

**Rejected because**: the tiers would come from code outside the repository, so
they could not be reproduced or checked for drift.

### (c) A Harvest copy of the algorithm

**Rejected because**: a second copy would create another implementation to
maintain alongside the shared algorithm.

### (d) One shared package — chosen

The Operator rescope asked for one SSOT used by more than two modules. A bare
`internal/tier` was not chosen because "tier" already names temple room tiers
and merc support tiers; `internal/shared` was not chosen because tiering
encodes business rules.

## References

- POE-283 — Harvest flipping, the lane that prompted this decision
- POE-134 — the absolute-cutoff modelling rule behind Lab's explicit 5c
- [ADR-007](007-v3-hybrid-analysis-unified-pipeline-with-trade-enriched-features.md) — the analysis pipeline still classifies once per snapshot
- [ADR-008](008-current-go-package-architecture.md) — the new package exists because a concrete dependency boundary requires it
- [ADR-017](017-no-default-engine-floor-may-hide-a-live-market.md) — the zero `Policy` applies no default floor
- [ADR-018](018-flags-mark-they-never-order.md) — tier labels are flags, not sort keys
