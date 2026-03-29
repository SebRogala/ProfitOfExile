# Unified Tier System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 3-4 different tier systems with a single per-variant pipeline: CASCADE detection → TOP detection → gap-based tiers. One computation, used everywhere. Clean up all dead code.

**Architecture:** A new `ComputeGemClassification` function runs as step 0 of the v2 pipeline, producing per-gem regime and tier assignments in a `GemClassificationMap`. All 4 variants (1, 1/20, 20, 20/20) are computed independently — no cross-variant mixing. `ComputeMarketContext`, `ComputeGemFeatures`, `AnalyzeFont`, and `AnalyzeTrends` all consume this single map. `GlobalTier` is retired — `Tier` is the only tier field.

**Tech Stack:** Go (internal/lab package), PostgreSQL (gem_snapshots, gem_features)

---

## Current State (What's Broken)

Four independent tier systems exist:
1. `DetectTierBoundaries` in `ComputeMarketContext` — recursive gap detection, global + per-variant + "all"
2. `DetectTierBoundariesSimplified` in `AnalyzeFont` — per-color, 3-tier only (HIGH/MID/FLOOR)
3. `DetectTierBoundaries` in `AnalyzeTrends` — recomputed per snapshot from current data
4. `classifyTierForVariant` / `classifyTierGlobal` in `ComputeGemFeatures` — reads from market context

Problems:
- CASCADE gems (3 listings, 3204c spike) distort tier boundaries for everyone
- Per-color simplified tiers produce different TOP than global, confusing Pool Overview vs Jackpot
- RED gets 66% FLOOR because boundaries are computed from all colors (BLUE/GREEN push thresholds up)
- `GlobalTier` vs `Tier` distinction causes confusion — both exist but mean different things

## Target State

```
Per variant (independently for each of: 1, 1/20, 20, 20/20):

  Step 1: Low Confidence Detection  (input: current snapshot listings)
      → which gems are thin market (depth < 0.4 of variant median listings)
      → these are NOT labeled CASCADE/META — we can't tell in real-time
      → flagged as "low confidence" for the user to interpret

  Step 2: TOP Detection             (input: pool minus low-confidence gems)
      → which gems are TOP (gap detection on clean pool)

  Step 3: Tier Boundaries           (input: pool minus low-confidence and TOP gems)
      → gap-detected: HIGH, MID-HIGH, MID, LOW, FLOOR

  Step 4: Per-Gem Classification    (combine steps 1-3)
      → Low-confidence gems: tier="LOW_CONFIDENCE", excluded from Font EV
      → TOP gems: tier="TOP"
      → Everyone else: tier from step 3

Output: GemClassificationMap[name+variant] → {Tier, LowConfidence bool}
    Used by: MarketContext, GemFeatures, Font EV, Trends, Signals, BestPlays

Font EV behavior:
  - Low-confidence gems: in pool count (Font draws from ALL gems), but
    EV contribution = 0 (not counted as winners, not in avg win prices)
  - Pool Overview: shown as separate "Low confidence" row with tooltip
    showing gem name, listings, price, and note that system can't determine
    if this is a meta shift or manipulation

BestPlays / By Variant behavior:
  - Low-confidence gems shown with visual indicator (dimmed/badge)
  - User can toggle visibility (like poe.ninja "show low confidence")
```

## What Gets Deleted

| Item | File | Why |
|------|------|-----|
| `DetectTierBoundariesSimplified` | tiers.go | Replaced by unified pipeline |
| `classifyTierForVariant` | gem_features.go | Direct lookup from classification map |
| `classifyTierGlobal` | gem_features.go | `GlobalTier` retired, only `Tier` exists |
| `GemFeature.GlobalTier` field | v2types.go | Unified — `Tier` is the only tier |
| `GlobalTier` column references | repository.go | DB column stays but always equals `Tier` |
| `computeVariantBaselines` tier computation | market_context.go | Tiers come from classification, not MC |
| `mc.TierBoundaries` usage for tiers | market_context.go | Boundaries stored from classification for backward compat |
| All `DetectTierBoundaries` calls in MC | market_context.go | Replaced |
| `DetectTierBoundaries` call in trends.go | trends.go | Reads from classification map |
| `cascadeCappedPrice` | font.go | Low-confidence gems excluded from EV entirely |
| `computePoolP75` | font.go | No longer needed |

## What Stays

| Item | Why |
|------|-----|
| `DetectTierBoundaries` / `DetectTierBoundariesRecursive` function | The gap detection ALGORITHM is reused — just called differently |
| `classifyTier(price, boundaries)` function | Still needed to classify individual prices against boundaries |
| `TierBoundaries` type | Still the output of gap detection |
| `mc.TierBoundaries` / `mc.VariantStats[*].Tiers` DB fields | Populated from classification for backward compat + optimizer |

## File Changes

| File | Action | What Changes |
|------|--------|-------------|
| `internal/lab/v2types.go` | Modify | Add `GemClassification`, `GemClassificationKey`, `GemClassificationMap`. Remove `GlobalTier` from `GemFeature`. |
| `internal/lab/classification.go` | **Create** | `ComputeGemClassification` — the unified pipeline (steps 1-4) |
| `internal/lab/classification_test.go` | **Create** | Tests for CASCADE, TOP, boundaries, full pipeline |
| `internal/lab/tiers.go` | Modify | Delete `DetectTierBoundariesSimplified`. Add `DetectTierBoundariesNoTop` (gap detection without TOP step). Keep `DetectTierBoundaries` unchanged. |
| `internal/lab/market_context.go` | Modify | Accept classification map. Remove internal tier detection. Populate `TierBoundaries` from classification for backward compat. |
| `internal/lab/gem_features.go` | Modify | Accept classification map. Direct lookup instead of `classifyTierForVariant`/`classifyTierGlobal`. Delete both functions. Remove `GlobalTier` assignment. |
| `internal/lab/analyzer.go` | Modify | Run classification as step 0, pass map to all downstream. |
| `internal/lab/font.go` | Modify | Remove `DetectTierBoundariesSimplified`, use `feat.Tier` for everything. Pool breakdown uses `feat.Tier`. Safe/Premium/Jackpot use `feat.Tier`. |
| `internal/lab/trends.go` | Modify | Accept classification map. Remove `DetectTierBoundaries` call. Look up `priceTier` from map. |
| `internal/lab/gem_signals.go` | Modify | Replace `feat.GlobalTier` references with `feat.Tier`. |
| `internal/lab/collective.go` | Modify | Remove `GlobalTier` field from result structs. |
| `internal/lab/repository.go` | Modify | `GlobalTier` column: write same value as `Tier` for backward compat. |
| `internal/server/handlers/analysis.go` | Modify | Remove `GlobalTier` from API response rows. |
| `internal/server/handlers/collective.go` | Modify | Remove `GlobalTier` enrichment from cached features. |
| `frontend/src/lib/api.ts` | Modify | Remove `globalTier` field from types. |
| `frontend/src/routes/lab/components/BestPlays.svelte` | Modify | Use `priceTier` instead of `globalTier`. |
| Tests: `*_test.go` | Modify | Update all `GlobalTier` references, remove `DetectTierBoundariesSimplified` tests. |

---

### Task 1: Define GemClassification Type + Remove GlobalTier (backend, standalone)

**Files:**
- Modify: `internal/lab/v2types.go`

- [ ] **Step 1: Add GemClassification types**

```go
// GemClassification holds the pre-computed tier and confidence for a single gem.
type GemClassification struct {
    Tier          string // "TOP", "HIGH", "MID-HIGH", "MID", "LOW", "FLOOR"
    LowConfidence bool   // true = thin market, excluded from EV calculations
}

type GemClassificationKey struct {
    Name    string
    Variant string
}

type GemClassificationMap map[GemClassificationKey]GemClassification
```

- [ ] **Step 2: Add `LowConfidence bool` to GemFeature struct**

Do NOT remove `GlobalTier` yet — Tasks 2-5 need a compiling package to run tests. `GlobalTier` removal happens in Task 7 when all callers are updated together.

- [ ] **Step 3: Create DB migration for `low_confidence` column**

```bash
make migration name=add_low_confidence_to_gem_features
```

Up:
```sql
ALTER TABLE gem_features ADD COLUMN IF NOT EXISTS low_confidence BOOLEAN NOT NULL DEFAULT false;
```

Down:
```sql
ALTER TABLE gem_features DROP COLUMN IF EXISTS low_confidence;
```

- [ ] **Step 4: Update repository.go INSERT/SELECT for gem_features**

Add `low_confidence` to the INSERT column list and SELECT scan. Match positional parameter count.

- [ ] **Step 5: Commit**

---

### Task 2: Implement Low Confidence Detection (backend, standalone)

**Files:**
- Create: `internal/lab/classification.go`
- Create: `internal/lab/classification_test.go`

- [ ] **Step 1: Write failing test for low confidence detection**

Test: gems with varying listings, verify thin-market gems (depth < 0.4 of variant median) get flagged. Use the same filtering logic as `DetectTierBoundaries` (listing floor = 25% of variant median, min 2) so the median is computed from the same pool.

- [ ] **Step 2: Implement `detectLowConfidence`**

Computes per-variant median listings from variant-filtered subsets of the snapshot. **Do NOT call `collectAndSortPrices` directly** — that function operates cross-variant. Instead, replicate its filtering logic (chaos > 5, `isAnalyzableGem`) but apply it per-variant. The listing floor (25% of variant median, min 2) is computed per-variant from the filtered subset. Returns `map[string]bool` keyed by `"name|variant"` — true = low confidence.

- [ ] **Step 3: Test passes**
- [ ] **Step 4: Commit**

---

### Task 3: Implement TOP Detection (backend, after task 2)

**Files:**
- Modify: `internal/lab/classification.go`
- Modify: `internal/lab/classification_test.go`
- Modify: `internal/lab/tiers.go` (add `DetectTierBoundariesNoTop`)

- [ ] **Step 1: Write failing test for TOP detection**

Test: pool with clear gap at top, low-confidence gem excluded. Verify only gems above the gap are TOP.

- [ ] **Step 2: Implement `detectTops`**

Per-variant: from the full `gems` slice, build per-variant subsets excluding low-confidence gems. For each variant subset, run `DetectTierBoundaries` on that variant-only slice, extract ONLY the first boundary (TOP threshold). Mark gems >= threshold as TOP. **Input to `DetectTierBoundaries` must be the variant-filtered, low-confidence-excluded subset, not the full snapshot.**

**Important:** To prevent `DetectTierBoundaries` from finding a TOP within the already-TOP-free pool in Task 4, add a new function `DetectTierBoundariesNoTop(gems []GemPrice) TierBoundaries` that calls the recursive algorithm but skips the TOP gap search (step 1). Keep `DetectTierBoundaries` unchanged (used for TOP detection in step 2). This avoids changing the existing function signature and breaking callers.

- [ ] **Step 3: Test passes**
- [ ] **Step 4: Commit**

---

### Task 4: Implement Clean Tier Boundaries (backend, after task 3)

**Files:**
- Modify: `internal/lab/classification.go`
- Modify: `internal/lab/classification_test.go`

- [ ] **Step 1: Write failing test for tier boundaries without TOPs**

Test: RED-like pool of ~30 gems (no CASCADE, no TOPs). Verify FLOOR < 50% of pool. Verify no gem gets classified as "TOP" from these boundaries.

- [ ] **Step 2: Implement `computeCleanTierBoundaries`**

Per-variant: exclude low-confidence + TOP gems, call `DetectTierBoundariesNoTop` (created in Task 3). Returns `map[string]TierBoundaries`.

- [ ] **Step 3: Test passes — explicit assertion that no "TOP" tier appears from classifyTier with these boundaries**
- [ ] **Step 4: Commit**

---

### Task 5: Assemble ComputeGemClassification (backend, after task 4)

**Files:**
- Modify: `internal/lab/classification.go`
- Modify: `internal/lab/classification_test.go`

- [ ] **Step 1: Write integration test with realistic data**

Test: ~10 gems including 1 thin-market gem (3 listings), 2 TOPs, rest normal. Verify:
- Thin gem has `LowConfidence: true`, gets tier from boundaries (not TOP)
- TOP gems have `Tier: "TOP"`, `LowConfidence: false`
- Normal gems have appropriate non-TOP tiers
- No gem has empty Tier

- [ ] **Step 2: Implement `ComputeGemClassification`**

Wires steps 1-3. Also stores per-variant `TierBoundaries` in the result for `MarketContext` backward compatibility:

```go
type ClassificationResult struct {
    Gems            GemClassificationMap
    Boundaries      map[string]TierBoundaries // per-variant, for MC backward compat
    TopBoundary     map[string]float64        // per-variant TOP threshold
}
```

Note: `LowConfidence` is already encoded per-gem in `GemClassificationMap` — no need for a separate field.

- [ ] **Step 3: Test passes**
- [ ] **Step 4: Commit**

---

### Task 6: Wire into RunV2 + Migrate MarketContext (backend, after task 5)

**Files:**
- Modify: `internal/lab/analyzer.go`
- Modify: `internal/lab/market_context.go`
- Modify: `internal/lab/market_context_test.go` (if exists)

- [ ] **Step 1: Add classification as step 0 in RunV2**

```go
classification := ComputeGemClassification(gems)
```

Pass `classification` to `ComputeMarketContext`.

- [ ] **Step 2: Modify ComputeMarketContext signature**

```go
func ComputeMarketContext(snapTime time.Time, gems []GemPrice, history []GemPriceHistory, cls ClassificationResult) MarketContext
```

Remove ALL internal `DetectTierBoundaries` calls (lines 94, 243, 250). Instead:
- `mc.TierBoundaries = cls.Boundaries["20/20"]` — use 20/20 as the "default" variant (most relevant for display). No more cross-variant "all" boundary.
- `mc.VariantStats[v].Tiers = cls.Boundaries[v]` for each variant
- `mc.VariantStats["all"]` — omit or set to 20/20 boundaries for backward compat

- [ ] **Step 3: Remove `computeVariantBaselines` tier computation**

Keep median listings computation (needed for other things), delete the `DetectTierBoundaries(variantGems)` calls within it.

- [ ] **Step 4: Update ALL callers of ComputeMarketContext**

Known call sites (verify with `grep -rn "ComputeMarketContext" internal/`):
- `analyzer.go` RunV2 — primary, pass classification
- `temporal_normalization_test.go` — test helper, pass empty/mock classification
- `optimizer.go` — if it calls MC directly, update signature
- Any other test files

- [ ] **Step 5: Fix compile errors, run tests**
- [ ] **Step 6: Commit**

---

### Task 7: Migrate GemFeatures (backend, after task 6)

**Files:**
- Modify: `internal/lab/gem_features.go`
- Modify: `internal/lab/gem_features_test.go`
- Modify: `internal/lab/risk_scoring_test.go` (calls `ComputeGemFeatures` directly)

- [ ] **Step 1: Change ComputeGemFeatures signature to accept classification**

```go
func ComputeGemFeatures(snapTime time.Time, gems []GemPrice, history []GemPriceHistory, mc MarketContext, cls GemClassificationMap) []GemFeature
```

- [ ] **Step 2: Replace tier assignment with direct lookup**

Remove all CALL SITES for `classifyTierForVariant`, `classifyTierGlobal`. Leave the function bodies in place — they are deleted as a group in Task 10. Replace:

```go
if c, ok := cls[GemClassificationKey{g.Name, g.Variant}]; ok {
    f.Tier = c.Tier
    f.LowConfidence = c.LowConfidence
    f.MarketDepth = computeMarketDepthForGem(g.Listings, g.Variant, mc, avgListings)
    // MarketRegime stays as CASCADE/TEMPORAL based on depth — NOT overridden
    // by LowConfidence. LowConfidence is a user-facing flag; MarketRegime is
    // internal (temporal normalization gating, advanced signal detection).
    // A gem can be both LowConfidence AND CASCADE — those are independent axes.
    if f.MarketDepth < 0.4 {
        f.MarketRegime = "CASCADE"
    } else {
        f.MarketRegime = "TEMPORAL"
    }
}
```

Note: `LowConfidence` and `MarketRegime` are independent. LowConfidence = user-facing (excludes from EV, shows badge). MarketRegime = internal (temporal normalization, advanced signals). A thin gem gets both `LowConfidence=true` AND `MarketRegime="CASCADE"`.

- [ ] **Step 3: Update repository.go — write Tier into both `tier` and `global_tier` columns**

For backward compat, `global_tier` column gets the same value as `tier`:
```go
f.Tier, f.Tier, // tier column, global_tier column
```

- [ ] **Step 4: Fix all test references to GlobalTier, update assertions**
- [ ] **Step 5: Run all tests, fix failures**
- [ ] **Step 6: Commit**

---

### Task 8: Migrate Font EV (backend, after task 7)

**Files:**
- Modify: `internal/lab/font.go`
- Modify: `internal/lab/font_test.go`

**Ordering dependency:** `RunFont` reads `features` from cache (populated by `RunV2`). Since `RunV2` now populates `LowConfidence` on features, `RunFont` must run AFTER `RunV2`. In `cmd/server/main.go`, the gem event handler fires both in parallel goroutines — `RunFont` must wait for `RunV2`. Either: (a) make `RunFont` sequential after `RunV2` in the event handler, or (b) have `RunFont` read `LowConfidence` from the cached classification map directly. Option (a) is simpler — change the event handler to run `RunV2` first, then `RunFont` after.

- [ ] **Step 1: Remove all per-color tier detection from AnalyzeFont**

Delete:
- `colorGems` construction
- `DetectTierBoundariesSimplified(colorGems)` call
- `colorTier := classifyTier(effectivePrice, colorTiers)`

Replace with: `tier := feat.Tier` (from unified classification)

- [ ] **Step 2: Simplify winner counting**

All three modes use `feat.Tier`, but low-confidence gems are EXCLUDED from winners:
- Safe: `feat.Tier != "FLOOR" && !feat.LowConfidence`
- Premium: `isPremiumTierWinner(feat.Tier) && !feat.LowConfidence`
- Jackpot: `feat.Tier == "TOP" && !feat.LowConfidence`

Low-confidence gems still count toward pool size (the Font draws from ALL gems — you CAN hit a thin-market gem) but their EV contribution is 0. This prevents inflated prices from distorting the EV.

- [ ] **Step 3: Pool breakdown uses feat.Tier directly**

No more `breakdownTier` overrides. Just `tierStats[feat.Tier]`.
Low-confidence gems get a separate "Low confidence" row in the breakdown.

- [ ] **Step 4: Add LowConfidenceGems to FontResult + pool overview tooltip**

Add to `FontResult`:
```go
LowConfidenceGems []LowConfidenceGemInfo `json:"lowConfidenceGems,omitempty"`
```

```go
type LowConfidenceGemInfo struct {
    Name     string  `json:"name"`
    Chaos    float64 `json:"chaos"`
    Listings int     `json:"listings"`
}
```

During AnalyzeFont iteration, collect low-confidence gems into this list instead of into tier stats / winner counts.

Add `FontEVCompare.svelte` to Task 10 Step 6 to render the "Low confidence" row in Pool Overview with per-gem tooltip:
- Tooltip: "{name} at {price}c with {listings} listings — significantly fewer listings than normal. Could be a meta shift or price manipulation."

- [ ] **Step 5: Remove cascadeCappedPrice and computePoolP75 from font.go**

No longer needed — low-confidence gems are excluded from EV entirely. Non-low-confidence gems use raw ninja price (no cap) — if they have enough listings to pass the confidence threshold, their price is trustworthy.

- [ ] **Step 6: Update font tests**

Update `makeFeature` helper: drop `GlobalTier` param, add `LowConfidence bool` param (default false).
Add explicit test: a low-confidence gem at 3000c contributes 0 to EV but is counted in pool size.

- [ ] **Step 7: Run all tests**
- [ ] **Step 8: Commit**

---

### Task 9: Migrate Trends + Signals (backend, after task 7)

**Files:**
- Modify: `internal/lab/trends.go`
- Modify: `internal/lab/trends_test.go`
- Modify: `internal/lab/trends_backtest_test.go`
- Modify: `internal/lab/gem_signals.go`
- Modify: `internal/lab/analyzer.go`

- [ ] **Step 1: Change AnalyzeTrends signature**

```go
func AnalyzeTrends(snapTime time.Time, current []GemPrice, history []GemPriceHistory,
    baseHistory map[string][]PricePoint, marketAvgBaseLst float64,
    cls GemClassificationMap) []TrendResult
```

- [ ] **Step 2: Remove DetectTierBoundaries call (line 305)**

Replace `priceTier := classifyTier(g.Chaos, tb)` with:

```go
priceTier := "FLOOR"
if c, ok := cls[GemClassificationKey{g.Name, g.Variant}]; ok {
    priceTier = c.Tier
}
```

- [ ] **Step 3: Update all AnalyzeTrends callers**

- `analyzer.go` RunTrends: pass classification map
- `cmd/server/main.go` gem event handler: pass classification map (or restructure to not need it — AnalyzeTrends is called from event handler too)

Note: The event handler calls `analyzer.RunTrends(subCtx)` which internally calls `AnalyzeTrends`. Add these fields to `Analyzer` struct in `analyzer.go`:

```go
muClassification    sync.RWMutex
latestClassification GemClassificationMap
```

- `RunV2` writes with `muClassification.Lock()` after computing classification
- `RunTrends` reads with `muClassification.RLock()`
- `RunV2` writes with write lock after computing classification
- `RunTrends` reads with read lock
- `RunTrends` must nil-guard: if classifications is nil (first run before RunV2), fall back to empty map (all gems get "FLOOR" tier — safe default, will correct on next snapshot)
- **Startup ordering:** `cmd/server/main.go` line 265 fires `RunTrends` in a startup goroutine that may run before `RecomputeLatestV2` completes. This is acceptable — the nil fallback handles it, and the next gem event (within ~30min) will provide correct tiers. Add a test that verifies `RunTrends` handles nil classification gracefully.

- [ ] **Step 4: Verify gem_signals.go has no GlobalTier references**

`gem_signals.go` uses `f.Tier` not `f.GlobalTier` — no changes needed. Confirm with grep.

- [ ] **Step 5: Run all tests**
- [ ] **Step 6: Commit**

---

### Task 10: Clean Up Dead Code (backend, after tasks 8-9)

**Files:**
- Modify: `internal/lab/tiers.go`
- Modify: `internal/lab/tiers_test.go`
- Modify: `internal/lab/collective.go`
- Modify: `internal/server/handlers/analysis.go`
- Modify: `internal/server/handlers/collective.go`
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/routes/lab/components/BestPlays.svelte`

- [ ] **Step 1: Delete DetectTierBoundariesSimplified from tiers.go + its tests**
- [ ] **Step 2: Delete classifyTierForVariant, classifyTierGlobal from gem_features.go**
- [ ] **Step 3: Remove GlobalTier from CollectiveResult struct (CompareResult has no GlobalTier)**
- [ ] **Step 4: Remove GlobalTier from API response rows in handlers**
- [ ] **Step 5: Remove GlobalTier enrichment from collective handler (cache.GemFeatures lookup)**
- [ ] **Step 6: Frontend: remove globalTier, add lowConfidence toggle**

- `api.ts`: remove `globalTier` from types, add `lowConfidence: boolean` to GemPlay
- `BestPlays.svelte`: add "Show low confidence" toggle (default off). Filter `sorted` to exclude `lowConfidence` gems unless toggled on. Low-confidence gems shown with dimmed row + dedicated `tier-low-confidence` CSS class.
- `ByVariant.svelte`: no changes needed — it passes `allPlays` to `BestPlays` which handles filtering internally.
- `FontEVCompare.svelte`: render "Low confidence" row in Pool Overview with count + per-gem tooltip (data from `FontResult.LowConfidenceGems`). Use `InfoTooltip` component for the tooltip.
- [ ] **Step 7: Run ALL tests (Go + frontend svelte-check)**
- [ ] **Step 8: Commit**

---

### Task 11: Verify Locally with Prod Data (general, after task 10)

- [ ] **Step 1: Restart local app, trigger recalculation**
- [ ] **Step 2: Check Pool Overview for RED 20/20** — FLOOR should be < 50%, TOPs shown, numbers match Font EV
- [ ] **Step 3: Check Font EV** — Safe/Premium/Jackpot consistent with pool overview
- [ ] **Step 4: Check BestPlays** — tier badges use unified tiers
- [ ] **Step 5: Check CASCADE gem** — Lightning Strike should not distort anything
- [ ] **Step 6: Screenshot comparison with current prod**
