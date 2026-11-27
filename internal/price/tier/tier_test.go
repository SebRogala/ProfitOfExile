package tier

import (
	"reflect"
	"strconv"
	"testing"
)

func item(group, key string, price float64, depth int) Item {
	return Item{Group: group, Key: key, Price: price, Depth: depth}
}

func k(group, key string) Key { return Key{Group: group, Key: key} }

func TestLowConfidence_BasicThreshold(t *testing.T) {
	items := []Item{
		item("20/20", "Normal Gem", 500, 60),
		item("20/20", "Thin Gem", 3000, 3),
		item("20/20", "Another Normal", 200, 50),
		item("20/20", "Moderate Gem", 100, 40),
	}

	lc := LowConfidence(items, Policy{})

	if lc[k("20/20", "Normal Gem")] {
		t.Error("Normal Gem should not be low confidence")
	}
	if !lc[k("20/20", "Thin Gem")] {
		t.Error("Thin Gem (3 listings vs median ~45) should be low confidence")
	}
}

func TestLowConfidence_PerVariant(t *testing.T) {
	items := []Item{
		// Group "20/20": median ~50 listings
		item("20/20", "Gem A", 100, 50),
		item("20/20", "Gem B", 100, 60),
		item("20/20", "Gem C", 100, 10), // 10/50 = 0.2 < 0.4
		// Group "1": median ~10 listings (different market)
		item("1", "Gem D", 10, 10),
		item("1", "Gem E", 10, 12),
		item("1", "Gem F", 10, 3), // 3/10 = 0.3 < 0.4
	}

	lc := LowConfidence(items, Policy{})

	if !lc[k("20/20", "Gem C")] {
		t.Error("Gem C (10 listings, group median 50) should be low confidence")
	}
	if !lc[k("1", "Gem F")] {
		t.Error("Gem F (3 listings, group median 10) should be low confidence")
	}
	if lc[k("20/20", "Gem A")] {
		t.Error("Gem A should not be low confidence")
	}
	if lc[k("1", "Gem D")] {
		t.Error("Gem D (at median) should not be low confidence")
	}
}

func TestDetectTops_GapDetection(t *testing.T) {
	items := []Item{
		// Clear gap: 1300, 1200 → gap 800 → 400 (well above 3x avg gap)
		item("20/20", "Mega Gem", 1300, 50),
		item("20/20", "Big Gem", 1200, 40),
		item("20/20", "High Gem", 400, 30),
		item("20/20", "Mid Gem", 350, 60),
		item("20/20", "Low Gem", 300, 80),
		item("20/20", "Floor A", 100, 100),
		item("20/20", "Floor B", 80, 100),
		item("20/20", "Floor C", 50, 120),
	}

	tops, _ := detectTops(items, map[Key]bool{}, Policy{})

	if !tops[k("20/20", "Mega Gem")] {
		t.Error("Mega Gem should be TOP")
	}
	if !tops[k("20/20", "Big Gem")] {
		t.Error("Big Gem should be TOP")
	}
	if tops[k("20/20", "High Gem")] {
		t.Error("High Gem should NOT be TOP")
	}
	if tops[k("20/20", "Mid Gem")] {
		t.Error("Mid Gem should NOT be TOP")
	}
}

func TestDetectTops_ExcludesLowConfidence(t *testing.T) {
	items := []Item{
		item("20/20", "Spike Gem", 5000, 3),
		item("20/20", "Normal A", 500, 50),
		item("20/20", "Normal B", 400, 60),
		item("20/20", "Normal C", 300, 70),
		item("20/20", "Normal D", 200, 80),
		item("20/20", "Normal E", 100, 90),
	}
	lowConf := map[Key]bool{k("20/20", "Spike Gem"): true}

	tops, _ := detectTops(items, lowConf, Policy{})

	// Spike Gem is excluded from the pool, so it shouldn't be TOP.
	if tops[k("20/20", "Spike Gem")] {
		t.Error("Low-confidence Spike Gem should not be TOP")
	}
}

func TestDetectTops_PerVariant(t *testing.T) {
	items := []Item{
		// Group 20/20: clear TOP at 1000 (gap of 800 vs next at 200)
		item("20/20", "Top 2020", 1000, 50),
		item("20/20", "Mid 2020", 200, 60),
		item("20/20", "Low 2020 A", 150, 70),
		item("20/20", "Low 2020 B", 100, 70),
		item("20/20", "Low 2020 C", 50, 70),
		// Group 1: evenly spaced, no significant gap
		item("1", "Gem 1A", 200, 100),
		item("1", "Gem 1B", 180, 100),
		item("1", "Gem 1C", 150, 100),
		item("1", "Gem 1D", 120, 100),
		item("1", "Gem 1E", 100, 100),
	}

	tops, _ := detectTops(items, map[Key]bool{}, Policy{})

	if !tops[k("20/20", "Top 2020")] {
		t.Error("Top 2020 should be TOP")
	}
	// Group 1 has no significant gap — no TOPs expected
	if tops[k("1", "Gem 1A")] {
		t.Error("Gem 1A in group 1 should not be TOP (no significant gap)")
	}
}

func TestCleanBoundaries_REDLikePool(t *testing.T) {
	// RED 20/20 pool without TOPs: 25 gems ranging 267c to 29c
	prices := []float64{267, 250, 229, 200, 175, 137, 120, 115, 105, 95, 89, 75, 74, 73, 66, 65, 63, 54, 50, 48, 42, 40, 35, 31, 29}
	depths := []int{30, 50, 70, 40, 30, 60, 80, 100, 50, 40, 60, 80, 70, 60, 90, 100, 80, 70, 60, 50, 40, 30, 90, 100, 80}
	var items []Item
	for i, p := range prices {
		items = append(items, item("20/20", "G"+string(rune('A'+i)), p, depths[i]))
	}

	boundaries := cleanBoundaries(items, map[Key]bool{}, map[Key]bool{}, Policy{})

	vb, ok := boundaries["20/20"]
	if !ok {
		t.Fatal("expected boundaries for 20/20")
	}
	if len(vb.Boundaries) < 2 {
		t.Errorf("expected >= 2 boundaries, got %d", len(vb.Boundaries))
	}

	// Key assertion: FLOOR should be < 50% of pool.
	floorCount := 0
	for _, it := range items {
		if Of(it.Price, vb) == Floor {
			floorCount++
		}
	}
	if floorCount > 12 {
		t.Errorf("too many FLOOR gems: %d/25 (want <= 12, was 66%% before refactor)", floorCount)
	}

	// No item should get "TOP" from these boundaries.
	for _, it := range items {
		if got := Of(it.Price, vb); got == Top {
			t.Errorf("item %s at %fc classified as TOP — clean boundaries never produce TOP", it.Key, it.Price)
		}
	}
}

func TestCleanBoundaries_ExcludesLowConfAndTops(t *testing.T) {
	items := []Item{
		item("20/20", "Top Gem", 1300, 50),
		item("20/20", "Spike", 5000, 3),
		item("20/20", "Normal A", 300, 60),
		item("20/20", "Normal B", 200, 70),
		item("20/20", "Normal C", 100, 80),
		item("20/20", "Normal D", 50, 90),
	}
	lowConf := map[Key]bool{k("20/20", "Spike"): true}
	tops := map[Key]bool{k("20/20", "Top Gem"): true}

	boundaries := cleanBoundaries(items, lowConf, tops, Policy{})

	// Computed from [300, 200, 100, 50] only: FLOOR = (300+200+100+50)/4 × 0.08
	// = 13; HIGH snaps to 300; MID-HIGH also snaps to 300 so it is dropped; MID
	// needs 4 prices below MID-HIGH and has 3. Spike or Top Gem in the pool
	// would raise both FLOOR and HIGH.
	want := Boundaries{Boundaries: []float64{300, 13}, Names: []string{High, Low, Floor}}
	if got := boundaries["20/20"]; !reflect.DeepEqual(got, want) {
		t.Errorf("boundaries[20/20] = %+v, want %+v", got, want)
	}
}

func TestOf_DynamicBoundaries(t *testing.T) {
	// 3 boundaries: TOP >= 500, HIGH >= 200, MID-HIGH >= 50
	tb := Boundaries{Boundaries: []float64{500, 200, 50}}

	tests := []struct {
		price float64
		want  string
	}{
		{600, "TOP"},
		{500, "TOP"},     // exactly at boundary[0] -> TOP (>= semantics)
		{499.99, "HIGH"}, // below boundary[0], above boundary[1]
		{200, "HIGH"},    // exactly at boundary[1] -> HIGH
		{199.99, "MID-HIGH"},
		{50, "MID-HIGH"}, // exactly at boundary[2] -> MID-HIGH
		{49.99, "MID"},   // below all boundaries, index 3 -> MID
		{0, "MID"},
	}

	for _, tt := range tests {
		got := Of(tt.price, tb)
		if got != tt.want {
			t.Errorf("Of(%v, {500, 200, 50}) = %s, want %s", tt.price, got, tt.want)
		}
	}
}

func TestOf_TwoBoundaries(t *testing.T) {
	// 2 boundaries: TOP >= 500, HIGH >= 100
	tb := Boundaries{Boundaries: []float64{500, 100}}

	tests := []struct {
		price float64
		want  string
	}{
		{600, "TOP"},
		{500, "TOP"},
		{499, "HIGH"},
		{100, "HIGH"},
		{99, "MID-HIGH"}, // below all 2 boundaries -> Order[2] = "MID-HIGH"
		{0, "MID-HIGH"},
	}

	for _, tt := range tests {
		got := Of(tt.price, tb)
		if got != tt.want {
			t.Errorf("Of(%v, {500, 100}) = %s, want %s", tt.price, got, tt.want)
		}
	}
}

func TestOf_FourBoundaries(t *testing.T) {
	// 4 boundaries: TOP >= 500, HIGH >= 200, MID-HIGH >= 100, MID >= 30
	tb := Boundaries{Boundaries: []float64{500, 200, 100, 30}}

	tests := []struct {
		price float64
		want  string
	}{
		{600, "TOP"},
		{300, "HIGH"},
		{150, "MID-HIGH"},
		{50, "MID"},
		{29, "LOW"}, // below all 4 boundaries -> Order[4] = "LOW"
		{0, "LOW"},
	}

	for _, tt := range tests {
		got := Of(tt.price, tb)
		if got != tt.want {
			t.Errorf("Of(%v, ...) = %s, want %s", tt.price, got, tt.want)
		}
	}
}

func TestOf_NoBoundaries(t *testing.T) {
	// No boundaries -> everything is the first tier name.
	got := Of(100, Boundaries{})
	if got != "TOP" {
		t.Errorf("Of(100, empty) = %s, want TOP", got)
	}
}

// evenPool is a five-item market with no TOP gap: its clean boundaries are
// [90, 80, 6.4] named HIGH, MID-HIGH, LOW, FLOOR (FLOOR = top-5 average 80 ×
// 0.08), so any extra item priced below 6.4 lands in FLOOR.
func evenPool(group string, depth int) []Item {
	return []Item{
		item(group, "p100", 100, depth),
		item(group, "p90", 90, depth),
		item(group, "p80", 80, depth),
		item(group, "p70", 70, depth),
		item(group, "p60", 60, depth),
	}
}

func TestClassify_ZeroPolicyTiersASubChaosItem(t *testing.T) {
	items := append(evenPool("oil", 50), item("oil", "cheap", 0.09, 50))

	got, ok := Classify(items, Policy{}).Items[k("oil", "cheap")]

	if !ok || got.Tier != Floor {
		t.Errorf("0.09c item under the zero Policy = %+v (present %v), want tier %s", got, ok, Floor)
	}
}

func TestClassify_CutoffLeavesAnItemPricedAtTheCutoffOut(t *testing.T) {
	items := append(evenPool("20/20", 50), item("20/20", "at-cutoff", 5.00, 50))

	res := Classify(items, Policy{CutoffChaos: 5, HasCutoff: true})

	if got, ok := res.Items[k("20/20", "at-cutoff")]; ok {
		t.Errorf("5.00c item with cutoff 5 classified as %+v, want it left out", got)
	}
}

func TestClassify_CutoffTiersAnItemPricedJustAboveIt(t *testing.T) {
	items := append(evenPool("20/20", 50), item("20/20", "above-cutoff", 5.01, 50))

	got, ok := Classify(items, Policy{CutoffChaos: 5, HasCutoff: true}).Items[k("20/20", "above-cutoff")]

	if !ok || got.Tier != Floor {
		t.Errorf("5.01c item with cutoff 5 = %+v (present %v), want tier %s", got, ok, Floor)
	}
}

// thinSpikePool is evenPool at 100 listings plus one 500c item with a single
// listing: depth 1/100 is far under the 0.4 low-confidence threshold, and 500c
// sits above the top boundary 90.
func thinSpikePool() []Item {
	return append(evenPool("20/20", 100), item("20/20", "spike", 500, 1))
}

func TestClassify_DepthUnknownFlagsNoItemLowConfidence(t *testing.T) {
	res := Classify(thinSpikePool(), Policy{DepthUnknown: true})

	for key, c := range res.Items {
		if c.LowConfidence || c.Tier == Chaotic {
			t.Errorf("%v = %+v under DepthUnknown, want no low-confidence and no CHAOTIC item", key, c)
		}
	}
	if len(res.Items) != 6 {
		t.Errorf("classified %d items, want 6", len(res.Items))
	}
}

func TestClassify_ThinItemAboveTheTopBoundaryIsChaotic(t *testing.T) {
	got := Classify(thinSpikePool(), Policy{}).Items[k("20/20", "spike")]

	want := Classification{Tier: Chaotic, LowConfidence: true}
	if got != want {
		t.Errorf("thin 500c item = %+v, want %+v", got, want)
	}
}

// gapPool has the TOP cluster {1300, 1200}: the 800c gap under 1200 is more
// than twice the average gap of the top three.
func gapPool(group string) []Item {
	return []Item{
		item(group, "Mega", 1300, 50),
		item(group, "Big", 1200, 40),
		item(group, "High", 400, 30),
		item(group, "Mid", 350, 60),
		item(group, "Low", 300, 80),
		item(group, "Floor A", 100, 100),
		item(group, "Floor B", 80, 100),
		item(group, "Floor C", 50, 120),
	}
}

func TestClassify_TopBoundaryIsTheLowestTopPrice(t *testing.T) {
	res := Classify(gapPool("20/20"), Policy{})

	if got, ok := res.TopBoundary["20/20"]; !ok || got != 1200 {
		t.Errorf("TopBoundary[20/20] = %v (present %v), want 1200", got, ok)
	}
}

func TestClassify_GroupWithoutTopHasNoTopBoundary(t *testing.T) {
	res := Classify(evenPool("20/20", 50), Policy{})

	if got, ok := res.TopBoundary["20/20"]; ok {
		t.Errorf("TopBoundary[20/20] = %v for a pool with no TOP gap, want no key", got)
	}
}

func TestClassify_GroupsSharingKeysClassifyIndependently(t *testing.T) {
	// Both groups use the same item keys; each must be tiered against its
	// own market only.
	a := gapPool("a")
	b := evenPool("b", 50)
	for i := range b {
		b[i].Key = a[i].Key
	}

	joint := Classify(append(append([]Item{}, a...), b...), Policy{})
	alone := map[string]Result{"a": Classify(a, Policy{}), "b": Classify(b, Policy{})}

	for group, want := range alone {
		if !reflect.DeepEqual(joint.Boundaries[group], want.Boundaries[group]) {
			t.Errorf("group %s boundaries joint = %+v, alone = %+v", group, joint.Boundaries[group], want.Boundaries[group])
		}
		for key, c := range want.Items {
			if joint.Items[key] != c {
				t.Errorf("%v joint = %+v, alone = %+v", key, joint.Items[key], c)
			}
		}
	}
	if len(joint.Items) != len(a)+len(b) {
		t.Errorf("classified %d items, want %d", len(joint.Items), len(a)+len(b))
	}
}

// labLikePolicy mirrors Lab's tier policy: a 5c cutoff.
var labLikePolicy = Policy{CutoffChaos: 5, HasCutoff: true}

func gatedItem(group, key string, price float64, depth int) Item {
	it := item(group, key, price, depth)
	it.Gated = true
	return it
}

// keep:happy — the ruled Covens-shaped 20/20 example.
func TestKeepHappy_ClassifyGatedCovensGroupKeepsEveryRowWithoutBoundariesOrTop(t *testing.T) {
	prices := []float64{527, 264, 170, 88, 64, 45, 33, 24, 17, 12, 8}
	items := make([]Item, len(prices))
	for i, price := range prices {
		items[i] = gatedItem("20/20", "covens-"+strconv.Itoa(i), price, 1+i%2)
	}

	res := Classify(items, labLikePolicy)

	if len(res.Items) != len(items) {
		t.Errorf("len(Items) = %d, want %d — every gated row must stay", len(res.Items), len(items))
	}
	for _, it := range items {
		got, ok := res.Items[k(it.Group, it.Key)]
		if !ok {
			t.Errorf("%s (%.0fc) missing from Items", it.Key, it.Price)
			continue
		}
		if !got.LowConfidence {
			t.Errorf("%s (%.0fc) LowConfidence = false, want true", it.Key, it.Price)
		}
	}
	if b, ok := res.Boundaries["20/20"]; ok {
		t.Errorf("Boundaries[20/20] = %+v, want no entry for an all-gated group", b)
	}
	if tb, ok := res.TopBoundary["20/20"]; ok {
		t.Errorf("TopBoundary[20/20] = %v, want no entry for an all-gated group", tb)
	}
}

// keep:critical — silent: a gated ask re-anchoring the group would relabel
// every tier that later steps trust.
func TestKeepCritical_ClassifyGatedOutlierMovesNoBoundaryOrTop(t *testing.T) {
	prices := []float64{300, 280, 150, 120, 100, 90, 70, 50, 40, 30}
	depths := []int{30, 35, 40, 45, 50, 50, 55, 60, 65, 70} // median 50
	base := make([]Item, len(prices))
	for i := range prices {
		base[i] = item("deep", "gem-"+strconv.Itoa(i), prices[i], depths[i])
	}
	outlier := gatedItem("deep", "gated-outlier", 527, 50)
	withOutlier := append(append([]Item{}, base...), outlier)

	want := Classify(base, labLikePolicy)
	if _, ok := want.TopBoundary["deep"]; !ok {
		t.Fatalf("fixture: base group has no TopBoundary; the comparison would be vacuous")
	}
	got := Classify(withOutlier, labLikePolicy)

	if !reflect.DeepEqual(got.Boundaries["deep"], want.Boundaries["deep"]) {
		t.Errorf("Boundaries[deep] = %+v, want %+v (as without the gated item)", got.Boundaries["deep"], want.Boundaries["deep"])
	}
	gotTop, gotOK := got.TopBoundary["deep"]
	if wantTop := want.TopBoundary["deep"]; !gotOK || gotTop != wantTop {
		t.Errorf("TopBoundary[deep] = %v (present %v), want %v (as without the gated item)", gotTop, gotOK, wantTop)
	}
	wantClass := Classification{Tier: Chaotic, LowConfidence: true}
	if c := got.Items[k("deep", "gated-outlier")]; c != wantClass {
		t.Errorf("gated outlier = %+v, want %+v", c, wantClass)
	}
}

// keep:critical — silent (row a): Double-corrupt reads LowConfidence directly
// and would price gated outcomes.
func TestKeepCritical_LowConfidenceIncludesGatedItems(t *testing.T) {
	peers := []Item{
		item("20/20", "peer-a", 120, 40),
		item("20/20", "peer-b", 90, 40),
		item("20/20", "peer-c", 60, 40),
		item("20/20", "peer-d", 30, 40),
	}
	gated := gatedItem("20/20", "gated", 50, 40)
	items := append(append([]Item{}, peers...), gated)

	tests := []struct {
		name   string
		policy Policy
	}{
		{"lab-like cutoff policy", labLikePolicy},
		{"depth unknown", Policy{DepthUnknown: true}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := LowConfidence(items, tt.policy)
			want := map[Key]bool{k("20/20", "gated"): true}
			if !reflect.DeepEqual(got, want) {
				t.Errorf("LowConfidence = %v, want %v", got, want)
			}
		})
	}
}

// keep:critical — silent: held values vanish from EV, or caller-gated asks enter it.
func TestKeepCritical_HeldExemptsRelativeDepthOnly(t *testing.T) {
	peers := []Item{item("20/20", "a", 80, 40), item("20/20", "b", 60, 40), item("20/20", "c", 40, 40)}
	for _, tt := range []struct {
		name              string
		held, gated       bool
		price             float64
		wantFlag, wantRow bool
	}{
		{"held shallow", true, false, 100, false, true},
		{"unheld shallow", false, false, 100, true, true},
		{"caller gated held", true, true, 100, true, true},
		{"held below cutoff", true, false, 5, false, false},
	} {
		t.Run(tt.name, func(t *testing.T) {
			it := Item{Group: "20/20", Key: "thin", Price: tt.price, Depth: 1, Held: tt.held, Gated: tt.gated}
			items := append(append([]Item(nil), peers...), it)
			key := k("20/20", "thin")
			if flag := LowConfidence(items, labLikePolicy)[key]; flag != tt.wantFlag {
				t.Errorf("relative-depth flag = %v, want %v", flag, tt.wantFlag)
			}
			got := Classify(items, labLikePolicy)
			c, ok := got.Items[key]
			if ok != tt.wantRow || c.LowConfidence != tt.wantFlag {
				t.Errorf("classification = %+v (present %v), want present %v, flag %v", c, ok, tt.wantRow, tt.wantFlag)
			}
		})
	}
}
