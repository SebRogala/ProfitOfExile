package lab

import (
	"math"
	"strings"
	"testing"
	"time"
)

// The expected values in this file are exact evaluations of the outcome model
// stated in doublecorrupt.go (two independent rolls with replacement over
// {no-change .25, level+1 .125 cap 21, level-1 .125, quality-reroll .25
// uniform 0..23, Vaal .25 when a Vaal version exists, folded into no-change
// otherwise}), aggregated into market cells (quality 23 → "/23c", 20-22 →
// "/20c", <20 → level-only "c"). Key exact cell masses, derived by enumerating
// the 5×5 roll pairs:
//
//	with a Vaal version:     20/20c = 15/128   VAAL 20/20c = 13/64
//	                         21/20c = 11/128   VAAL 20c    = 5/48
//	                         21/23c = 1/384
//	without a Vaal version:  20/20c = 41/128   21/20c = 19/128
//	                         20/23c = 5/384    21/23c = 1/384
//
// A weight or cap mutation in the model moves these masses and every EV
// constant below.

func makeDCInput(name, color string, chaos float64, listings int) GemPrice {
	return GemPrice{
		Name:           name,
		Variant:        "20/20",
		Chaos:          chaos,
		Listings:       listings,
		IsTransfigured: strings.Contains(name, " of "),
		IsCorrupted:    false,
		GemColor:       color,
	}
}

// makeDCBaseInput is makeDCInput for a base (non-transfigured) skill gem, whose
// own name may itself contain " of " — "Rain of Arrows" is a base gem, not a
// transfigured one.
func makeDCBaseInput(name, color string, chaos float64, listings int) GemPrice {
	g := makeDCInput(name, color, chaos, listings)
	g.IsTransfigured = false
	return g
}

func makeDCCorrupted(name, variant string, chaos float64, listings int) GemPrice {
	return GemPrice{
		Name:           name,
		Variant:        variant,
		Chaos:          chaos,
		Listings:       listings,
		IsTransfigured: strings.Contains(name, " of "),
		IsCorrupted:    true,
		GemColor:       "BLUE",
	}
}

// fullyPricedArcSnapshot prices every cell the model can reach from a 20/20
// input for "Arc of Surging" (Vaal version present), all at 50+ listings so
// risk adjustment is a no-op (sell probability 1.0, no stability discount).
//
// EVRaw over these prices = 10395/64 = 162.421875 exactly.
func fullyPricedArcSnapshot() []GemPrice {
	vaal := "Vaal Arc (Arc of Surging)"
	return []GemPrice{
		makeDCInput("Arc of Surging", "BLUE", 100, 30),
		makeDCCorrupted("Arc of Surging", "20/20c", 100, 50),
		makeDCCorrupted("Arc of Surging", "21/20c", 300, 50),
		makeDCCorrupted("Arc of Surging", "20/23c", 500, 50),
		makeDCCorrupted("Arc of Surging", "21/23c", 2000, 50),
		makeDCCorrupted("Arc of Surging", "20c", 40, 50),
		makeDCCorrupted("Arc of Surging", "21c", 60, 50),
		makeDCCorrupted("Arc of Surging", "19c", 10, 50),
		makeDCCorrupted("Arc of Surging", "19/20c", 20, 50),
		makeDCCorrupted("Arc of Surging", "19/23c", 80, 50),
		makeDCCorrupted("Arc of Surging", "18/20c", 5, 50),
		makeDCCorrupted(vaal, "20/20c", 150, 50),
		makeDCCorrupted(vaal, "21/20c", 1000, 50),
		makeDCCorrupted(vaal, "20/23c", 800, 50),
		makeDCCorrupted(vaal, "20c", 50, 50),
		makeDCCorrupted(vaal, "19/20c", 30, 50),
	}
}

func singleDCResult(t *testing.T, results []DoubleCorruptResult, name string) DoubleCorruptResult {
	t.Helper()
	var found *DoubleCorruptResult
	for i := range results {
		if results[i].Name == name {
			if found != nil {
				t.Fatalf("more than one result for %q", name)
			}
			found = &results[i]
		}
	}
	if found == nil {
		t.Fatalf("no result for %q (got %d results)", name, len(results))
	}
	return *found
}

func outcomeCell(t *testing.T, r DoubleCorruptResult, name, variant string) DoubleCorruptOutcome {
	t.Helper()
	for _, o := range r.Outcomes {
		if o.Name == name && o.Variant == variant {
			return o
		}
	}
	t.Fatalf("no outcome cell (%q, %q) in result for %q", name, variant, r.Name)
	return DoubleCorruptOutcome{}
}

const dcEps = 1e-9

func TestAnalyzeDoubleCorrupt_EVRawIsProbabilityWeightedSumOverPricedCells(t *testing.T) {
	results := AnalyzeDoubleCorrupt(time.Now(), fullyPricedArcSnapshot(), nil, DefaultTempleOverheadChaos)

	r := singleDCResult(t, results, "Arc of Surging")

	if math.Abs(r.EVRaw-162.421875) > dcEps {
		t.Errorf("EVRaw = %v, want 162.421875 (10395/64, the exact weighted sum)", r.EVRaw)
	}
	// All cells carry 50+ listings, so risk adjustment must be a no-op.
	if math.Abs(r.EV-r.EVRaw) > dcEps {
		t.Errorf("EV = %v, want EVRaw %v — 50+ listings must not be discounted", r.EV, r.EVRaw)
	}
	// Input cost is the gem's own uncorrupted 20/20 price; the room is sunk.
	// (EV and EVRaw coincide here, so the basis is pinned by
	// TestAnalyzeDoubleCorrupt_ProfitIsMeasuredOffTheRiskAdjustedEV instead.)
	if math.Abs(r.Profit-(162.421875-100)) > dcEps {
		t.Errorf("Profit = %v, want EV - 100 input cost", r.Profit)
	}
	if math.Abs(r.PricedProbability-1.0) > dcEps || r.UnpricedProbability > dcEps {
		t.Errorf("fully priced set: priced/unpriced mass = %v/%v, want 1/0",
			r.PricedProbability, r.UnpricedProbability)
	}
}

func TestAnalyzeDoubleCorrupt_OutcomeMassesMatchTheModel(t *testing.T) {
	results := AnalyzeDoubleCorrupt(time.Now(), fullyPricedArcSnapshot(), nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Arc of Surging")

	vaal := "Vaal Arc (Arc of Surging)"
	wantMass := []struct {
		name    string
		variant string
		mass    float64
	}{
		{"Arc of Surging", "20/20c", 15.0 / 128},
		{vaal, "20/20c", 13.0 / 64},
		{"Arc of Surging", "21/20c", 11.0 / 128},
		{vaal, "20c", 5.0 / 48},
		{"Arc of Surging", "21/23c", 1.0 / 384},
	}
	for _, w := range wantMass {
		got := outcomeCell(t, r, w.name, w.variant).Probability
		if math.Abs(got-w.mass) > dcEps {
			t.Errorf("P(%s %s) = %v, want %v", w.name, w.variant, got, w.mass)
		}
	}

	var total float64
	for _, o := range r.Outcomes {
		total += o.Probability
	}
	if math.Abs(total-1.0) > dcEps {
		t.Errorf("outcome masses sum to %v, want 1", total)
	}
}

func TestAnalyzeDoubleCorrupt_LevelCapsAt21(t *testing.T) {
	results := AnalyzeDoubleCorrupt(time.Now(), fullyPricedArcSnapshot(), nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Arc of Surging")

	for _, o := range r.Outcomes {
		if strings.HasPrefix(o.Variant, "22") {
			t.Errorf("outcome cell %q exists — two +1 rolls must cap at level 21", o.Variant)
		}
	}
	// The capped double-up pair lands on 21/20c: without the cap its mass
	// would be 10/128, not 11/128.
	got := outcomeCell(t, r, "Arc of Surging", "21/20c").Probability
	if math.Abs(got-11.0/128) > dcEps {
		t.Errorf("P(21/20c) = %v, want 11/128 (includes the capped +1/+1 pair)", got)
	}
}

func TestAnalyzeDoubleCorrupt_NeverProducesAVaalGemAt2123(t *testing.T) {
	// Vaal + level up + quality 23 is three corruption outcomes; two rolls
	// cannot reach it (the same rule isDedicationFeed records). A poisoned
	// price for that impossible cell must not leak into the EV.
	gems := append(fullyPricedArcSnapshot(),
		makeDCCorrupted("Vaal Arc (Arc of Surging)", "21/23c", 99999, 50))

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Arc of Surging")

	for _, o := range r.Outcomes {
		if o.Name == "Vaal Arc (Arc of Surging)" && o.Variant == "21/23c" {
			t.Fatalf("model produced a Vaal 21/23c cell with mass %v", o.Probability)
		}
	}
	if math.Abs(r.EVRaw-162.421875) > dcEps {
		t.Errorf("EVRaw = %v, want 162.421875 — the impossible cell's price leaked in", r.EVRaw)
	}
}

func TestAnalyzeDoubleCorrupt_NoVaalVersionFoldsTransformWeightIntoNoChange(t *testing.T) {
	gems := []GemPrice{
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
		makeDCCorrupted("Rolling Magma of Nothing", "20/20c", 100, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rolling Magma of Nothing")

	if r.HasVaalVersion {
		t.Error("HasVaalVersion = true for a gem with no Vaal rows in the snapshot")
	}
	for _, o := range r.Outcomes {
		if strings.HasPrefix(o.Name, "Vaal ") {
			t.Errorf("outcome %q exists for a gem with no Vaal version", o.Name)
		}
	}
	// With the transform's 25% folded into no-change, the unchanged cell's
	// mass rises from 15/128 to 41/128.
	got := outcomeCell(t, r, "Rolling Magma of Nothing", "20/20c").Probability
	if math.Abs(got-41.0/128) > dcEps {
		t.Errorf("P(20/20c) = %v, want 41/128 under the folded model", got)
	}
}

func TestAnalyzeDoubleCorrupt_UnpricedCellsReportTheirMassInsteadOfSilentlyZeroing(t *testing.T) {
	// No Vaal version; only the four headline cells priced. The level-only and
	// level-down cells (mass 33/64 together) have no market row.
	gems := []GemPrice{
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
		makeDCCorrupted("Rolling Magma of Nothing", "20/20c", 100, 50),
		makeDCCorrupted("Rolling Magma of Nothing", "21/20c", 300, 50),
		makeDCCorrupted("Rolling Magma of Nothing", "20/23c", 500, 50),
		makeDCCorrupted("Rolling Magma of Nothing", "21/23c", 2000, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rolling Magma of Nothing")

	// 41/128·100 + 19/128·300 + 5/384·500 + 1/384·2000 = 2825/32.
	if math.Abs(r.EVRaw-88.28125) > dcEps {
		t.Errorf("EVRaw = %v, want 88.28125 over the priced cells only", r.EVRaw)
	}
	if math.Abs(r.UnpricedProbability-33.0/64) > dcEps {
		t.Errorf("UnpricedProbability = %v, want 33/64", r.UnpricedProbability)
	}
	if math.Abs(r.PricedProbability-31.0/64) > dcEps {
		t.Errorf("PricedProbability = %v, want 31/64", r.PricedProbability)
	}

	unpriced := outcomeCell(t, r, "Rolling Magma of Nothing", "20c")
	if unpriced.Priced || unpriced.Chaos != 0 {
		t.Errorf("unpriced cell reported priced=%v chaos=%v, want unpriced with 0 chaos",
			unpriced.Priced, unpriced.Chaos)
	}
	if unpriced.Probability <= 0 {
		t.Error("unpriced cell dropped from the outcome breakdown — its mass must stay visible")
	}
}

func TestAnalyzeDoubleCorrupt_ThinOutcomeCellIsDownWeighted(t *testing.T) {
	gems := []GemPrice{
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
		makeDCCorrupted("Rolling Magma of Nothing", "20/20c", 100, 2),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rolling Magma of Nothing")

	// EVRaw takes the listed price at face value; EV applies the thin-market
	// sell-probability floor (0.3 at 2 listings, no feature data).
	if math.Abs(r.EVRaw-32.03125) > dcEps {
		t.Errorf("EVRaw = %v, want 41/128·100 = 32.03125", r.EVRaw)
	}
	if math.Abs(r.EV-9.609375) > dcEps {
		t.Errorf("EV = %v, want 0.3 × EVRaw = 9.609375", r.EV)
	}
	cell := outcomeCell(t, r, "Rolling Magma of Nothing", "20/20c")
	if !cell.Thin {
		t.Error("a 2-listing cell must be flagged thin")
	}
	if r.ThinOutcomeCells != 1 {
		t.Errorf("ThinOutcomeCells = %d, want 1", r.ThinOutcomeCells)
	}
	if r.LiquidityRisk != "HIGH" {
		t.Errorf("LiquidityRisk = %q, want HIGH when every priced cell is thin", r.LiquidityRisk)
	}
}

func TestAnalyzeDoubleCorrupt_ProfitIsMeasuredOffTheRiskAdjustedEV(t *testing.T) {
	// The same snapshot as the thin-cell test, chosen because EV (9.609375)
	// and EVRaw (32.03125) differ there: profit off EVRaw would read -17.97
	// and turn a losing craft into a smaller-looking loss — and on richer gems
	// into a headline profit larger than the estimate printed beside it.
	gems := []GemPrice{
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
		makeDCCorrupted("Rolling Magma of Nothing", "20/20c", 100, 2),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rolling Magma of Nothing")

	if math.Abs(r.Profit-(9.609375-50)) > dcEps {
		t.Errorf("Profit = %v, want EV - input cost = -40.390625 (not EVRaw - input cost = -17.96875)", r.Profit)
	}
}

func TestAnalyzeDoubleCorrupt_FiveListingCellIsNotThin(t *testing.T) {
	// The thin flag's boundary: 5 listings is the first non-thin count, the
	// same floor Font and Dedication use.
	gems := []GemPrice{
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
		makeDCCorrupted("Rolling Magma of Nothing", "20/20c", 100, 5),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rolling Magma of Nothing")

	if cell := outcomeCell(t, r, "Rolling Magma of Nothing", "20/20c"); cell.Thin {
		t.Error("a 5-listing cell must not be flagged thin")
	}
	if r.ThinOutcomeCells != 0 {
		t.Errorf("ThinOutcomeCells = %d, want 0", r.ThinOutcomeCells)
	}
	if r.LiquidityRisk != "LOW" {
		t.Errorf("LiquidityRisk = %q, want LOW with no thin cells", r.LiquidityRisk)
	}
}

// deepCorrupted21x23 returns filler rows that set the 21/23c pool's median
// listing depth to 50, so a test row's depth is judged against a real cohort
// rather than against itself (a lone row is always its own median, depth 1.0).
func deepCorrupted21x23() []GemPrice {
	return []GemPrice{
		makeDCCorrupted("Filler One of Depth", "21/23c", 400, 40),
		makeDCCorrupted("Filler Two of Depth", "21/23c", 400, 50),
		makeDCCorrupted("Filler Three of Depth", "21/23c", 400, 60),
	}
}

func TestAnalyzeDoubleCorrupt_LowConfidenceOutcomeCellIsExcludedRatherThanDiscounted(t *testing.T) {
	// The failure this gate exists for: one 21/23c row at 36,960c standing on 2
	// listings against a 50-listing median. Its mass is only 1/384, but the
	// price is large enough that any discount short of exclusion still lifts the
	// gem to the top of the profit ranking.
	gems := append(deepCorrupted21x23(),
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
		makeDCCorrupted("Rolling Magma of Nothing", "20/20c", 100, 50),
		makeDCCorrupted("Rolling Magma of Nothing", "21/23c", 36960, 2),
	)

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rolling Magma of Nothing")

	outlier := outcomeCell(t, r, "Rolling Magma of Nothing", "21/23c")
	if outlier.Priced || outlier.Chaos != 0 {
		t.Errorf("outlier cell priced=%v chaos=%v, want excluded with 0 chaos", outlier.Priced, outlier.Chaos)
	}
	if !outlier.LowConfidence {
		t.Error("LowConfidence = false — the breakdown must say the row was refused, not that none exists")
	}
	if outlier.Listings != 2 {
		t.Errorf("Listings = %d, want the refused row's 2 kept for the explanation", outlier.Listings)
	}
	// Only the 20/20c cell survives: 41/128 · 100. Pricing the outlier at face
	// value would add 1/384 · 36960 = 96.25 and nearly quadruple the EV.
	if math.Abs(r.EVRaw-32.03125) > dcEps {
		t.Errorf("EVRaw = %v, want 32.03125 over the confident cells only", r.EVRaw)
	}
	if math.Abs(r.PricedProbability-41.0/128) > dcEps {
		t.Errorf("PricedProbability = %v, want 41/128 — the refused mass belongs to unpriced", r.PricedProbability)
	}
	if math.Abs(r.PricedProbability+r.UnpricedProbability-1.0) > dcEps {
		t.Errorf("priced + unpriced = %v, want 1 — refused mass must be accounted, not dropped",
			r.PricedProbability+r.UnpricedProbability)
	}
}

func TestAnalyzeDoubleCorrupt_OutcomeCellAtExactlyFortyPercentOfMedianDepthIsStillPriced(t *testing.T) {
	// The gate's boundary: depth < 0.4 of the variant's median is refused, so 20
	// listings against a median of 50 is the first depth that survives.
	gems := append(deepCorrupted21x23(),
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
		makeDCCorrupted("Rolling Magma of Nothing", "21/23c", 3840, 20),
	)

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rolling Magma of Nothing")

	cell := outcomeCell(t, r, "Rolling Magma of Nothing", "21/23c")
	if cell.LowConfidence || !cell.Priced {
		t.Errorf("cell lowConfidence=%v priced=%v, want priced at exactly 0.4 depth",
			cell.LowConfidence, cell.Priced)
	}
	// 1/384 · 3840 = 10.
	if math.Abs(r.EVRaw-10) > dcEps {
		t.Errorf("EVRaw = %v, want 10", r.EVRaw)
	}
}

func TestAnalyzeDoubleCorrupt_LowConfidenceInputProducesNoResult(t *testing.T) {
	// InputCost is subtracted from EV, so an input price standing on 2 listings
	// against a 40-listing median makes the profit as untrustworthy as the price.
	// There is no half-answer to serve, so the gem is dropped.
	gems := []GemPrice{
		makeDCInput("Deep One of Alpha", "BLUE", 100, 30),
		makeDCInput("Deep Two of Beta", "BLUE", 100, 40),
		makeDCInput("Deep Three of Gamma", "BLUE", 100, 50),
		makeDCInput("Thin One of Delta", "BLUE", 100, 2),
		makeDCCorrupted("Thin One of Delta", "20/20c", 5000, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)

	for _, r := range results {
		if r.Name == "Thin One of Delta" {
			t.Errorf("a 2-listing input produced a result at profit %v", r.Profit)
		}
	}
	// The three confident inputs still answer — the gate excludes a gem, not the
	// variant it belongs to.
	if len(results) != 3 {
		t.Errorf("got %d results, want the 3 confidently priced inputs", len(results))
	}
}

func TestAnalyzeDoubleCorrupt_NoResultWithoutARowAtTheInputVariant(t *testing.T) {
	// Priced at 1 and 20 but not at the analyzed 20/20 input — a different
	// market, which must not stand in for the 20/20 one (per-variant rule).
	gems := []GemPrice{
		{Name: "Arc of Surging", Variant: "1", Chaos: 5, Listings: 30, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Arc of Surging", Variant: "20", Chaos: 20, Listings: 30, IsTransfigured: true, GemColor: "BLUE"},
		makeDCCorrupted("Arc of Surging", "21/20c", 300, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	if len(results) != 0 {
		t.Fatalf("got %d results for a gem with no 20/20 row, want 0", len(results))
	}
}

func TestAnalyzeDoubleCorrupt_AnotherGemsCorruptedMarketDoesNotLeak(t *testing.T) {
	gems := []GemPrice{
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
		makeDCCorrupted("Rolling Magma of Nothing", "20/20c", 100, 50),
		makeDCInput("Spark of the Storm", "BLUE", 50, 30),
		makeDCCorrupted("Spark of the Storm", "20/20c", 99999, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rolling Magma of Nothing")

	if math.Abs(r.EVRaw-32.03125) > dcEps {
		t.Errorf("EVRaw = %v, want 32.03125 from the gem's own cells only", r.EVRaw)
	}
	for _, o := range r.Outcomes {
		if strings.HasPrefix(o.Name, "Spark") {
			t.Errorf("outcome %q belongs to another gem", o.Name)
		}
	}
}

func TestAnalyzeDoubleCorrupt_ResultsSortedByProfitDescending(t *testing.T) {
	gems := []GemPrice{
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
		makeDCCorrupted("Rolling Magma of Nothing", "20/20c", 100, 50),
		makeDCInput("Spark of the Storm", "BLUE", 50, 30),
		makeDCCorrupted("Spark of the Storm", "20/20c", 5000, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	if len(results) != 2 {
		t.Fatalf("got %d results, want 2", len(results))
	}
	if results[0].Name != "Spark of the Storm" {
		t.Errorf("results[0] = %q, want the higher-profit gem first", results[0].Name)
	}
}

func TestAnalyzeDoubleCorrupt_TransfiguredVaalIdentityIsTheParenthesizedMarketName(t *testing.T) {
	gems := []GemPrice{
		makeDCInput("Absolution of Inspiring", "RED", 100, 30),
		makeDCCorrupted("Vaal Absolution (Absolution of Inspiring)", "20/20c", 400, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Absolution of Inspiring")

	if !r.HasVaalVersion {
		t.Fatal("HasVaalVersion = false with the Vaal market identity present in the snapshot")
	}
	cell := outcomeCell(t, r, "Vaal Absolution (Absolution of Inspiring)", "20/20c")
	if !cell.Priced || cell.Chaos != 400 {
		t.Errorf("Vaal cell priced=%v chaos=%v, want priced at 400", cell.Priced, cell.Chaos)
	}
}

func TestAnalyzeDoubleCorrupt_BaseVaalGemDoesNotCountAsTransfiguredVaalVersion(t *testing.T) {
	// "Vaal Absolution" is the base gem's Vaal identity; corrupting the
	// transfigured gem yields "Vaal Absolution (Absolution of Inspiring)".
	gems := []GemPrice{
		makeDCInput("Absolution of Inspiring", "RED", 100, 30),
		makeDCCorrupted("Vaal Absolution", "20/20c", 400, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Absolution of Inspiring")

	if r.HasVaalVersion {
		t.Error("HasVaalVersion = true from the base gem's Vaal identity")
	}
}

func TestAnalyzeDoubleCorrupt_BaseGemWhoseOwnNameContainsOfKeepsItsWholeNameInTheVaalIdentity(t *testing.T) {
	// "Rain of Arrows" is a base gem: its Vaal identity is "Vaal Rain of
	// Arrows", not "Vaal Rain". Cutting the name at " of " loses the Vaal
	// branch of the distribution silently.
	gems := []GemPrice{
		makeDCBaseInput("Rain of Arrows", "GREEN", 20, 30),
		makeDCCorrupted("Rain of Arrows", "20/20c", 30, 50),
		makeDCCorrupted("Vaal Rain of Arrows", "20/20c", 900, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rain of Arrows")

	if !r.HasVaalVersion {
		t.Fatal("HasVaalVersion = false — \"Vaal Rain of Arrows\" is in the snapshot")
	}
	cell := outcomeCell(t, r, "Vaal Rain of Arrows", "20/20c")
	if !cell.Priced || cell.Chaos != 900 {
		t.Errorf("Vaal cell priced=%v chaos=%v, want priced at 900", cell.Priced, cell.Chaos)
	}
}

func TestAnalyzeDoubleCorrupt_TransfiguredGemOfAnOfNamedBaseResolvesItsVaalIdentity(t *testing.T) {
	// "Rain of Arrows of Saturation" carries two " of " separators; only the
	// last one is the transfigured suffix, so the Vaal identity is
	// "Vaal Rain of Arrows (Rain of Arrows of Saturation)".
	gems := []GemPrice{
		makeDCInput("Rain of Arrows of Saturation", "GREEN", 20, 30),
		makeDCCorrupted("Vaal Rain of Arrows (Rain of Arrows of Saturation)", "21/20c", 4000, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rain of Arrows of Saturation")

	cell := outcomeCell(t, r, "Vaal Rain of Arrows (Rain of Arrows of Saturation)", "21/20c")
	if !cell.Priced || cell.Chaos != 4000 {
		t.Errorf("Vaal cell priced=%v chaos=%v, want priced at 4000", cell.Priced, cell.Chaos)
	}
}

func TestAnalyzeDoubleCorrupt_VaalIdentityThatTheGemNameCannotProduceIsReadFromTheSnapshot(t *testing.T) {
	// Measured 2026-08-24: "Dominating Blow of Inspiring" has the Vaal identity
	// "Vaal Domination" — the one pair of the 46 in gem_snapshots that no
	// name-derivation rule reaches. The snapshot row itself is the source.
	gems := []GemPrice{
		makeDCInput("Dominating Blow of Inspiring", "RED", 60, 30),
		makeDCCorrupted("Vaal Domination (Dominating Blow of Inspiring)", "20/20c", 700, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Dominating Blow of Inspiring")

	if !r.HasVaalVersion {
		t.Fatal("HasVaalVersion = false for an irregularly named Vaal identity present in the snapshot")
	}
	cell := outcomeCell(t, r, "Vaal Domination (Dominating Blow of Inspiring)", "20/20c")
	if !cell.Priced || cell.Chaos != 700 {
		t.Errorf("Vaal cell priced=%v chaos=%v, want priced at 700", cell.Priced, cell.Chaos)
	}
}

func TestAnalyzeDoubleCorrupt_BaseGemInheritsTheVaalIdentityNamedByItsTransfiguredRow(t *testing.T) {
	// The base gem "Dominating Blow" has no "Vaal Dominating Blow" row to read;
	// its Vaal identity is recoverable only from the transfigured row's
	// parenthesised pairing.
	gems := []GemPrice{
		makeDCInput("Dominating Blow", "RED", 5, 30),
		makeDCCorrupted("Vaal Domination (Dominating Blow of Inspiring)", "20/20c", 700, 50),
		makeDCCorrupted("Vaal Domination", "21/20c", 250, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Dominating Blow")

	if !r.HasVaalVersion {
		t.Fatal("HasVaalVersion = false — the transfigured row pairs \"Dominating Blow\" with \"Vaal Domination\"")
	}
	cell := outcomeCell(t, r, "Vaal Domination", "21/20c")
	if !cell.Priced || cell.Chaos != 250 {
		t.Errorf("Vaal cell priced=%v chaos=%v, want priced at 250", cell.Priced, cell.Chaos)
	}
}

func TestBuildVaalIdentityIndex_InheritedBaseIdentityStripsOnlyTheTransfiguredSuffix(t *testing.T) {
	// Recovering the base gem "Rain of Arrows" from the parenthesised row
	// "Vaal Rain of Arrows (Rain of Arrows of Saturation)" means dropping only
	// the LAST " of " segment; cutting at the first one files the identity under
	// a gem called "Rain" and the base gem loses its Vaal branch entirely.
	//
	// The index is exercised directly because the presence gate below makes this
	// path unreachable from a snapshot that also carries a bare "Vaal Rain of
	// Arrows" row — that row would answer for the base gem through the direct
	// branch and hide a wrong suffix cut.
	gems := []GemPrice{
		makeDCCorrupted("Vaal Rain of Arrows (Rain of Arrows of Saturation)", "20/20c", 900, 50),
	}
	listed := map[string]bool{
		"Vaal Rain of Arrows (Rain of Arrows of Saturation)": true,
		"Vaal Rain of Arrows":                                true,
	}

	index := buildVaalIdentityIndex(gems, listed)

	if got := index["Rain of Arrows"]; got != "Vaal Rain of Arrows" {
		t.Errorf("index[\"Rain of Arrows\"] = %q, want \"Vaal Rain of Arrows\"", got)
	}
	if got, ok := index["Rain"]; ok {
		t.Errorf("index[\"Rain\"] = %q — the cut took the first \" of \", not the last", got)
	}
}

func TestAnalyzeDoubleCorrupt_InheritedVaalIdentityTheMarketNeverListsIsNotAVaalVersion(t *testing.T) {
	// "Vaal Rain of Arrows" here is cut out of a parenthesised row by string
	// surgery — no corrupted row carries that name. Market presence is the
	// calculator's proxy for existence, so an identity nobody lists must not set
	// HasVaalVersion and must not open a Vaal branch the EV can never price.
	gems := []GemPrice{
		makeDCBaseInput("Rain of Arrows", "GREEN", 20, 30),
		makeDCCorrupted("Vaal Rain of Arrows (Rain of Arrows of Saturation)", "20/20c", 900, 50),
	}

	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rain of Arrows")

	if r.HasVaalVersion {
		t.Error("HasVaalVersion = true for a Vaal identity no row in the snapshot carries")
	}
	for _, o := range r.Outcomes {
		if strings.HasPrefix(o.Name, "Vaal ") {
			t.Errorf("outcome %q exists for an unlisted Vaal identity", o.Name)
		}
	}
}

func TestAnalyzeDoubleCorrupt_TempleOverheadReducesProfitFlat(t *testing.T) {
	gems := []GemPrice{
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
		makeDCCorrupted("Rolling Magma of Nothing", "20/20c", 100, 50),
	}

	base := AnalyzeDoubleCorrupt(time.Now(), gems, nil, 0)
	withOverhead := AnalyzeDoubleCorrupt(time.Now(), gems, nil, 5)

	b := singleDCResult(t, base, "Rolling Magma of Nothing")
	o := singleDCResult(t, withOverhead, "Rolling Magma of Nothing")

	if math.Abs((b.Profit-o.Profit)-5) > dcEps {
		t.Errorf("overhead 5 changed profit by %v, want exactly 5", b.Profit-o.Profit)
	}
	if o.TempleOverheadChaos != 5 {
		t.Errorf("TempleOverheadChaos = %v, want 5 recorded on the result", o.TempleOverheadChaos)
	}
}

func TestAnalyzeDoubleCorrupt_ExcludesIneligibleInputs(t *testing.T) {
	tests := []struct {
		name string
		gem  GemPrice
	}{
		// Variant deliberately "20/20": only the corruption flag excludes it.
		{"corrupted row is not an input", GemPrice{
			Name: "Arc of Surging", Variant: "20/20", Chaos: 100, Listings: 50,
			IsTransfigured: true, IsCorrupted: true, GemColor: "BLUE",
		}},
		{"Heist-only gem", makeDCInput("Arc of Trarthus", "BLUE", 100, 30)},
		{"Vaal market identity", makeDCInput("Vaal Arc (Arc of Surging)", "BLUE", 100, 30)},
		{"unpriced input", makeDCInput("Arc of Surging", "BLUE", 0, 30)},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			results := AnalyzeDoubleCorrupt(time.Now(), []GemPrice{tt.gem}, nil, DefaultTempleOverheadChaos)
			if len(results) != 0 {
				t.Errorf("got %d results, want 0", len(results))
			}
		})
	}
}

func TestAnalyzeDoubleCorrupt_SupportGemsAreLegitimateInputs(t *testing.T) {
	// Same stance as the quality roll: the altar transforms what you feed it,
	// and the corrupted support market is real — a 21/20 support is the outcome
	// the craft is fed one for.
	//
	// The input is a normal support, which is the shape poe.ninja publishes at
	// 20/20. Empower/Enlighten/Enhance are the exception (they list at levels
	// 1-4 and never reach 20), so using one here would assert eligibility over a
	// row that cannot exist.
	gems := []GemPrice{
		makeDCInput("Increased Critical Damage Support", "RED", 20, 30),
		makeDCCorrupted("Increased Critical Damage Support", "21/20c", 90, 50),
	}
	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	if len(results) != 1 {
		t.Fatalf("got %d results for a support input, want 1", len(results))
	}
}

func TestAnalyzeDoubleCorrupt_ResultCarriesEstimatedModelMarker(t *testing.T) {
	gems := []GemPrice{
		makeDCInput("Rolling Magma of Nothing", "BLUE", 50, 30),
	}
	results := AnalyzeDoubleCorrupt(time.Now(), gems, nil, DefaultTempleOverheadChaos)
	r := singleDCResult(t, results, "Rolling Magma of Nothing")
	if r.Model != DoubleCorruptModelEstimated {
		t.Errorf("Model = %q, want %q — the UI badges EVs as estimated off this field",
			r.Model, DoubleCorruptModelEstimated)
	}
}
