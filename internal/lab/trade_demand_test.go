package lab

import (
	"fmt"
	"math"
	"reflect"
	"testing"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/resolve"
	"profitofexile/internal/price/tier"
)

var demandSnapshot = time.Date(2026, 11, 16, 12, 0, 0, 0, time.UTC)

func demandGem(name, variant string, price float64) GemPrice {
	return GemPrice{Name: name, Variant: variant, Chaos: price, IsTransfigured: true, GemColor: "GREEN", Listings: 20}
}

func TestBuildTradeDemand_FourSnapshotMarketsIncludeCheapGem(t *testing.T) {
	scope := league.Historical("Temp-demand")
	gems := []GemPrice{
		demandGem("Siege Ballista of Splintering", "1", 1),
		demandGem("Siege Ballista of Splintering", "1/20", 20),
		demandGem("Siege Ballista of Splintering", "20", 40),
		demandGem("Siege Ballista of Splintering", "20/20", 100),
	}
	got := BuildTradeDemand(scope, demandSnapshot, gems)
	if len(got) != 4 {
		t.Fatalf("demand = %+v, want four actual markets", got)
	}
	for i, variant := range []string{"1", "1/20", "20", "20/20"} {
		key := resolve.GemKey(scope.ID(), gems[i].Name, variant)
		want := resolve.Observation{Key: key, Source: resolve.SourceNinja, At: demandSnapshot, Prices: []resolve.Money{{Amount: gems[i].Chaos, Currency: "chaos"}}}
		if got[i].Key != key || !got[i].SnapshotTime.Equal(demandSnapshot) || !reflect.DeepEqual(got[i].Ninja, want) {
			t.Errorf("market %s = %+v, want key/stamp/raw observation %+v", variant, got[i], want)
		}
		if got[i].PickProbability != 1 || got[i].PotentialContribution != gems[i].Chaos {
			t.Errorf("single-name exposure = %+v, want probability 1, contribution %g", got[i], gems[i].Chaos)
		}
	}
}

func TestBuildTradeDemand_IneligibleRowsExcluded(t *testing.T) {
	for _, scenario := range []string{"corrupted", "corruptedSuffix", "support", "Heist", "base", "white", "variant"} {
		t.Run(scenario, func(t *testing.T) {
			g := demandGem("Excluded", "20/20", 100)
			switch scenario {
			case "corrupted":
				g.IsCorrupted = true
			case "corruptedSuffix":
				g.Variant = "20/20c"
			case "support":
				g.Name = "Excluded Support"
			case "Heist":
				g.Name = "Gem of Trarthus"
			case "base":
				g.IsTransfigured = false
			case "white":
				g.GemColor = "WHITE"
			case "variant":
				g.Variant = "21/20"
			}
			if got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, []GemPrice{g}); len(got) != 0 {
				t.Fatalf("ineligible %s demand = %+v", scenario, got)
			}
		})
	}
}

func TestBuildTradeDemand_RankedExposure(t *testing.T) {
	prices := []float64{100, 80, 60, 40, 20}
	gems := make([]GemPrice, len(prices))
	for i, name := range []string{"A", "B", "C", "D", "E"} {
		gems[i] = demandGem(name, "20/20", prices[i])
	}
	got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, gems)
	wantProb := []float64{0.6, 0.3, 0.1, 0, 0}
	wantContribution := []float64{60, 24, 6, 0, 0}
	if len(got) != 5 {
		t.Fatalf("demand length = %d, want 5", len(got))
	}
	for i, d := range got {
		if d.PickProbability != wantProb[i] || d.PotentialContribution != wantContribution[i] {
			t.Errorf("%s exposure = %g/%g, want %g/%g", d.Key.Name, d.PickProbability, d.PotentialContribution, wantProb[i], wantContribution[i])
		}
	}
}

func TestBuildTradeDemand_SparseVariantUsesWholeColorPool(t *testing.T) {
	gems := []GemPrice{demandGem("A", "20/20", 100)}
	for _, name := range []string{"B", "C", "D", "E"} {
		gems = append(gems, demandGem(name, "1", 1))
	}
	got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, gems)
	if len(got) != 5 {
		t.Fatalf("demand length = %d, want 5 actual snapshot keys", len(got))
	}
	count := 0
	for _, d := range got {
		if d.Key.Variant == "20/20" {
			count++
			if d.Key.Name != "A" || d.PickProbability != 0.6 || d.PotentialContribution != 60 {
				t.Errorf("sparse exposure = %+v, want A at 0.6/60", d)
			}
		}
	}
	if count != 1 {
		t.Errorf("20/20 demand count = %d, want only actual A row", count)
	}
}

func TestBuildTradeDemand_EqualPricesUseCanonicalTies(t *testing.T) {
	gems := []GemPrice{}
	for _, name := range []string{"E", "D", "C", "B", "A"} {
		gems = append(gems, demandGem(name, "20/20", 100))
	}
	got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, gems)
	for i, want := range []float64{0.6, 0.3, 0.1, 0, 0} {
		if got[i].Key.Name != string(rune('A'+i)) || got[i].PickProbability != want {
			t.Errorf("tie rank %d = %+v, want canonical rank at probability %g", i, got[i], want)
		}
	}
}

func TestBuildTradeDemand_UnknownColorRetainsZeroExposure(t *testing.T) {
	g := demandGem("Unknown", "1", 100)
	g.GemColor = ""
	got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, []GemPrice{g})
	if len(got) != 1 || got[0].PickProbability != 0 || got[0].PotentialContribution != 0 {
		t.Fatalf("unresolved demand = %+v, want retained with zero exposure", got)
	}
}

func TestBuildTradeDemand_InvalidPriceRetainsFiniteExposure(t *testing.T) {
	for name, price := range map[string]float64{"zero": 0, "negative": -1, "NaN": math.NaN(), "positiveInfinity": math.Inf(1), "negativeInfinity": math.Inf(-1)} {
		t.Run(name, func(t *testing.T) {
			got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, []GemPrice{demandGem("A", "1", price), demandGem("B", "1", 100), demandGem("C", "1", 50), demandGem("D", "1", 10)})
			if len(got) != 4 || got[0].Key.Name != "A" || got[0].PotentialContribution != 0 || got[1].PotentialContribution != 75 {
				t.Fatalf("invalid-price pool = %+v, want retained A at zero and B at 75", got)
			}
			amount := got[0].Ninja.Prices[0].Amount
			if amount != price && !(math.IsNaN(amount) && math.IsNaN(price)) {
				t.Errorf("raw observation amount = %g, want original %g", amount, price)
			}
			for _, d := range got {
				if math.IsNaN(d.PickProbability) || math.IsInf(d.PickProbability, 0) || d.PickProbability < 0 {
					t.Errorf("invalid probability: %+v", d)
				}
			}
		})
	}
}

func TestBuildTradeDemand_ThinChaoticGemRetainsRawExposure(t *testing.T) {
	gems := []GemPrice{demandGem("Spike", "20/20", 5000)}
	gems[0].Listings = 1
	one := 1
	gems[0].SampleCount = &one
	for i, name := range []string{"A", "B", "C", "D", "E", "F", "G", "H", "I"} {
		g := demandGem(name, "20/20", float64(200-i*15))
		g.SampleCount = solidSample()
		gems = append(gems, g)
	}
	classification := ComputeGemClassification(gems)
	spike := classification.Gems[GemClassificationKey{Name: "Spike", Variant: "20/20"}]
	if spike.Tier != "CHAOTIC" || !spike.LowConfidence {
		t.Fatalf("fixture classification = %+v, want low-confidence CHAOTIC", spike)
	}
	got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, gems)
	for _, d := range got {
		if d.Key.Name == "Spike" {
			if d.PickProbability != 0.3 || d.PotentialContribution != 1500 || d.Ninja.SampleCount == nil || *d.Ninja.SampleCount != 1 {
				t.Fatalf("CHAOTIC exposure = %+v, want 0.3/1500 with thin sample", d)
			}
			return
		}
	}
	t.Fatal("CHAOTIC market missing from demand")
}

func TestBuildTradeDemand_EmptySnapshotHasNoTargets(t *testing.T) {
	if got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, nil); len(got) != 0 {
		t.Fatalf("empty snapshot demand = %+v", got)
	}
}

func TestBuildTradeDemand_ColorPoolsStaySeparate(t *testing.T) {
	gems := []GemPrice{demandGem("Green", "1", 100)}
	for _, name := range []string{"A", "B", "C", "D"} {
		g := demandGem(name, "1", 200)
		g.GemColor = "RED"
		gems = append(gems, g)
	}
	got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, gems)
	for _, d := range got {
		if d.Key.Name == "Green" {
			if d.PickProbability != 1 || d.PotentialContribution != 100 {
				t.Fatalf("GREEN exposure = %+v, want 1/100 independent of RED", d)
			}
			return
		}
	}
	t.Fatal("GREEN market missing")
}

func TestBuildTradeDemand_VariantsRankIndependently(t *testing.T) {
	gems := []GemPrice{
		demandGem("A", "1/0", 100), demandGem("B", "1", 80),
		demandGem("A", "20/20", 80), demandGem("B", "20/20", 100),
	}
	got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, gems)
	if len(got) != 4 {
		t.Fatalf("demand = %+v, want four canonical keys", got)
	}
	for _, d := range got {
		want := 0.0
		if d.Key.Name == "A" && d.Key.Variant == "1" || d.Key.Name == "B" && d.Key.Variant == "20/20" {
			want = 100
		}
		if d.Key.Variant == "1/0" || d.PotentialContribution != want {
			t.Errorf("variant exposure = %+v, want contribution %g at canonical variant", d, want)
		}
	}
}

func demandFloorPool() []GemPrice {
	gems := []GemPrice{
		demandGem("A", "20/20", 1000), demandGem("B", "20/20", 800),
		demandGem("C", "20/20", 600), demandGem("D", "20/20", 400),
		demandGem("Floor", "20/20", 10),
	}
	for i := range gems {
		gems[i].SampleCount = solidSample()
	}
	return gems
}

func TestBuildTradeDemand_NamedFloorExcluded(t *testing.T) {
	gems := demandFloorPool()
	classification := ComputeGemClassification(gems)
	if got := classification.Gems[GemClassificationKey{Name: "Floor", Variant: "20/20"}].Tier; got != tier.Floor {
		t.Fatalf("10c fixture tier = %q, want named FLOOR above the absolute cutoff", got)
	}
	// The same name in a different market remains independently eligible.
	gems = append(gems, demandGem("Floor", "1", 100))
	got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, gems)
	want := []resolve.Key{
		resolve.GemKey("Temp-demand", "A", "20/20"),
		resolve.GemKey("Temp-demand", "B", "20/20"),
		resolve.GemKey("Temp-demand", "C", "20/20"),
		resolve.GemKey("Temp-demand", "D", "20/20"),
		resolve.GemKey("Temp-demand", "Floor", "1"),
	}
	keys := make([]resolve.Key, 0, len(got))
	for _, d := range got {
		keys = append(keys, d.Key)
	}
	if !reflect.DeepEqual(keys, want) {
		t.Fatalf("demand keys = %+v, want %+v (only named FLOOR market excluded)", keys, want)
	}
}

func TestBuildTradeDemand_FloorExclusionPreservesFullPickPool(t *testing.T) {
	got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, demandFloorPool())
	for _, d := range got {
		if d.Key.Name == "A" {
			if d.PickProbability != 0.6 || d.PotentialContribution != 600 {
				t.Fatalf("top exposure = %+v, want 0.6/600 from all five eligible names", d)
			}
			return
		}
	}
	t.Fatal("top market missing")
}

func TestBuildTradeDemand_UntieredCheapUnpricedRetained(t *testing.T) {
	for _, price := range []float64{0, 1, 5} {
		t.Run(fmt.Sprint(price), func(t *testing.T) {
			g := demandGem("Untiered", "20/20", price)
			if _, ok := ComputeGemClassification([]GemPrice{g}).Gems[GemClassificationKey{Name: g.Name, Variant: g.Variant}]; ok {
				t.Fatal("fixture unexpectedly has a named tier")
			}
			got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, []GemPrice{g})
			if len(got) != 1 || got[0].Key != resolve.GemKey("Temp-demand", g.Name, g.Variant) || got[0].PotentialContribution != price {
				t.Fatalf("untiered demand = %+v, want retained market at raw contribution %g", got, price)
			}
		})
	}
}

// keep:happy — ruled acceptance: a low-confidence FLOOR-band gem in a deep
// variant stays trade-demand-eligible.
func TestKeepHappy_BuildTradeDemandKeepsLowConfidenceFloorKey(t *testing.T) {
	gems := []GemPrice{
		demandGem("A", "20/20", 1000), demandGem("B", "20/20", 800),
		demandGem("C", "20/20", 600), demandGem("D", "20/20", 400),
		demandGem("E", "20/20", 300), demandGem("F", "20/20", 200),
	}
	thin := demandGem("Thin", "20/20", 10)
	thin.Listings = 2 // under 0.4 × the variant median of 20
	gems = append(gems, thin)

	fixture := ComputeGemClassification(gems).Gems[GemClassificationKey{Name: "Thin", Variant: "20/20"}]
	if fixture.Tier != tier.Floor || !fixture.LowConfidence {
		t.Fatalf("fixture classification = %+v, want low-confidence FLOOR", fixture)
	}

	want := resolve.GemKey("Temp-demand", "Thin", "20/20")
	got := BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, gems)
	for _, d := range got {
		if d.Key == want {
			return
		}
	}
	keys := make([]resolve.Key, 0, len(got))
	for _, d := range got {
		keys = append(keys, d.Key)
	}
	t.Fatalf("demand keys = %+v, want %+v (low-confidence FLOOR key stays eligible)", keys, want)
}

// keep:happy — WI-4's Covens clause: every gated Covens 20/20 key stays in
// trade demand.
func TestKeepHappy_BuildTradeDemandKeepsEveryGatedCovens2020Key(t *testing.T) {
	gems := covens2020Gems()

	got := make(map[resolve.Key]bool)
	for _, d := range BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, gems) {
		got[d.Key] = true
	}

	for _, g := range gems {
		if key := resolve.GemKey("Temp-demand", g.Name, g.Variant); !got[key] {
			t.Errorf("%s (%.0fc) missing from trade demand", key, g.Chaos)
		}
	}
}

// keep:critical — silent: Font would count a gem while demand treats it as
// gated, or the reverse, and nothing would report it.
func TestKeepCritical_OneSnapshotGradesTheSameInFontAndDemand(t *testing.T) {
	asOf := time.Date(2026, 11, 16, 6, 44, 0, 0, time.UTC)
	one := 1
	gems := []GemPrice{
		{Name: "Smite of Divine Judgement", Variant: "20/20", Chaos: 111, Listings: 1, SampleCount: &one, IsTransfigured: true, GemColor: "GREEN"},
		{Name: "Storm Burst of Repulsion", Variant: "20/20", Chaos: 527, Listings: 1, SampleCount: &one, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Deep Gem of Test", Variant: "20/20", Chaos: 60, Listings: 20, SampleCount: solidSample(), IsTransfigured: true, GemColor: "GREEN"},
	}
	nine := 9
	points := []evidencePoint{{Name: "Smite of Divine Judgement", Variant: "20/20",
		Time: time.Date(2026, 11, 16, 4, 44, 0, 0, time.UTC), Chaos: 138, Listings: 9, SampleCount: &nine}}

	held := deriveEvidenceHolds(gems, points, asOf)
	cls := ComputeGemClassification(held)

	smite := cls.Gems[GemClassificationKey{"Smite of Divine Judgement", "20/20"}]
	if smite.LowConfidence {
		t.Errorf("Smite classification = %+v, want not gated (held by its 04:44Z SOLID point)", smite)
	}

	inDemand := make(map[resolve.Key]bool)
	for _, d := range BuildTradeDemand(league.Historical("Temp-demand"), demandSnapshot, held) {
		inDemand[d.Key] = true
	}
	for _, g := range held {
		c := cls.Gems[GemClassificationKey{g.Name, g.Variant}]
		wantDemand := !(c.Tier == tier.Floor && !c.LowConfidence)
		if key := resolve.GemKey("Temp-demand", g.Name, g.Variant); inDemand[key] != wantDemand {
			t.Errorf("%s in demand = %v, want %v for Font classification %+v", g.Name, inDemand[key], wantDemand, c)
		}
	}
}
