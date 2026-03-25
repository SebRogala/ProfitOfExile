package lab

import "testing"

func TestDetectLowConfidence_BasicThreshold(t *testing.T) {
	gems := []GemPrice{
		{Name: "Normal Gem", Variant: "20/20", Chaos: 500, Listings: 60, IsTransfigured: true},
		{Name: "Thin Gem", Variant: "20/20", Chaos: 3000, Listings: 3, IsTransfigured: true},
		{Name: "Another Normal", Variant: "20/20", Chaos: 200, Listings: 50, IsTransfigured: true},
		{Name: "Moderate Gem", Variant: "20/20", Chaos: 100, Listings: 40, IsTransfigured: true},
	}

	lc := detectLowConfidence(gems)

	if lc["Normal Gem|20/20"] {
		t.Error("Normal Gem should not be low confidence")
	}
	if !lc["Thin Gem|20/20"] {
		t.Error("Thin Gem (3 listings vs median ~45) should be low confidence")
	}
}

func TestDetectLowConfidence_PerVariant(t *testing.T) {
	gems := []GemPrice{
		// Variant "20/20": median ~50 listings
		{Name: "Gem A", Variant: "20/20", Chaos: 100, Listings: 50, IsTransfigured: true},
		{Name: "Gem B", Variant: "20/20", Chaos: 100, Listings: 60, IsTransfigured: true},
		{Name: "Gem C", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true}, // 10/50 = 0.2 < 0.4
		// Variant "1": median ~10 listings (different market)
		{Name: "Gem D", Variant: "1", Chaos: 10, Listings: 10, IsTransfigured: true},
		{Name: "Gem E", Variant: "1", Chaos: 10, Listings: 12, IsTransfigured: true},
		{Name: "Gem F", Variant: "1", Chaos: 10, Listings: 3, IsTransfigured: true}, // 3/10 = 0.3 < 0.4
	}

	lc := detectLowConfidence(gems)

	if !lc["Gem C|20/20"] {
		t.Error("Gem C (10 listings, variant median 50) should be low confidence")
	}
	if !lc["Gem F|1"] {
		t.Error("Gem F (3 listings, variant median 10) should be low confidence")
	}
	if lc["Gem A|20/20"] {
		t.Error("Gem A should not be low confidence")
	}
	if lc["Gem D|1"] {
		t.Error("Gem D (at median) should not be low confidence")
	}
}

func TestDetectLowConfidence_SkipsNonAnalyzable(t *testing.T) {
	gems := []GemPrice{
		{Name: "Normal", Variant: "20/20", Chaos: 100, Listings: 50, IsTransfigured: true},
		{Name: "Corrupted", Variant: "20/20", Chaos: 100, Listings: 3, IsTransfigured: true, IsCorrupted: true},
		{Name: "Not Trans", Variant: "20/20", Chaos: 100, Listings: 3, IsTransfigured: false},
		{Name: "Too Cheap", Variant: "20/20", Chaos: 3, Listings: 3, IsTransfigured: true},
	}

	lc := detectLowConfidence(gems)

	// Non-analyzable gems should not appear in results at all.
	if _, exists := lc["Corrupted|20/20"]; exists {
		t.Error("Corrupted gem should not be in results")
	}
	if _, exists := lc["Not Trans|20/20"]; exists {
		t.Error("Non-transfigured gem should not be in results")
	}
}
