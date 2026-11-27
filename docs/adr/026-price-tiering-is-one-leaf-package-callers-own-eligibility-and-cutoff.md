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

## Amendment — 2026-11-16 (POE-408)

Status: accepted; current implementation.

The unused legacy Lab tier algorithm and its tests in `internal/lab/tiers.go`
and `internal/lab/tiers_test.go` were removed after confirming they had no
production callers. The unused Lab tier-name and classification wrappers were
removed with them. The original account above is preserved as historical
context; its statements about retained legacy code and wrappers no longer
describe the current implementation.

Live classification remains in `internal/price/tier`, including its independent
fallback. Lab's live percentile helper, eligibility and cutoff policy, and
`TierBoundaries = tier.Boundaries` alias remain unchanged, as do the
classification fixtures. With no production callers of the removed code,
production behavior is inferred unchanged.

## Amendment — 2026-11-25 (POE-404)

Status: accepted; current tier implementation.

Decision 3 also permits callers to retain an eligible row while withholding its
price from TOP detection and tier boundaries, through `tier.Item.Gated`.
`LowConfidence` includes caller-gated items unless the policy's cutoff excludes
them. This widens Decision 2: `DepthUnknown` skips only relative-depth detection
and still honours the caller's gate. Gated items remain in the median-depth
input for the relative rule.

`Classify` retains each gated item above the cutoff in `Result.Items` with
`LowConfidence: true`. It uses the existing tier boundaries, including CHAOTIC
above the highest boundary, or FLOOR when the group has no boundaries. An
all-gated group has neither boundaries nor a TOP boundary. With no caller gates,
classification is unchanged. Callers continue to own the evidence policy that
sets the gate; the tier package introduces no evidence floor.

Caller-held items (`tier.Item.Held`) are exempt from relative-depth detection as defined in [ADR-032](032-one-price-engine-key-per-market-resolver-and-book.md#evidence-gate).

## Amendment — 2026-11-27 (POE-406)

Status: accepted; current tier implementation.

Callers may set `tier.Policy.MinConfident` to require a minimum number of
confident, cutoff-passing items in each group. Below a positive minimum,
`Classify` returns `tier.Unrated` (`UNRATED`) for every cutoff-passing member,
preserves each member's `LowConfidence` flag, and omits the group's tier
boundaries and TOP boundary. `Unrated` is outside `tier.Order`.

This supersedes the POE-404 amendment's FLOOR fallback for undersized groups
only when the caller enables the minimum. The zero policy retains the existing
no-boundary FLOOR and percentile fallback behavior. Harvest and Lab currently
leave the minimum disabled; Lab adoption remains proposed under POE-406.

### Ratio ladder — 2026-11-27 (POE-406)

Status: accepted; current opt-in tier implementation.

`tier.Policy.RatioLadder` selects a descending ratio ladder anchored to the
existing `computeTop5Avg` over confident, cutoff-passing, non-TOP prices in each
group. HIGH, MID-HIGH and MID use 0.70, 0.35 and 0.17 of that average; FLOOR
uses the greater of 0.08 of the average and 1c. Intermediate boundaries at or
below FLOOR are omitted together with their names. The ratio path bypasses gap
snapping and the small-group percentile fallback. The zero policy preserves
the gap ladder; Harvest and Lab leave the ratio policy disabled at this step.
`tier.MaxHighStep` records the accepted 0.25 movement limit; enforcing it
against previous boundaries remains proposed under POE-406 WI-4.
