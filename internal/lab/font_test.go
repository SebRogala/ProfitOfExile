package lab

import (
	"math"
	"testing"
	"time"
)

func TestPWin3Picks_Basic(t *testing.T) {
	// 5 winners out of 50: P(at least one win in 3 picks)
	p := pWin3Picks(5, 50)
	if p < 0.27 || p > 0.29 {
		t.Errorf("pWin3Picks(5, 50) = %f, want ~0.28", p)
	}
}

func TestPWin3Picks_AllWinners(t *testing.T) {
	p := pWin3Picks(10, 10)
	if p != 1.0 {
		t.Errorf("pWin3Picks(10, 10) = %f, want 1.0", p)
	}
}

func TestPWin3Picks_NoWinners(t *testing.T) {
	p := pWin3Picks(0, 50)
	if p != 0.0 {
		t.Errorf("pWin3Picks(0, 50) = %f, want 0.0", p)
	}
}

func TestPWin3Picks_TotalLessThan3_WithWinners(t *testing.T) {
	p := pWin3Picks(1, 2)
	if p != 1.0 {
		t.Errorf("pWin3Picks(1, 2) = %f, want 1.0", p)
	}
}

func TestPWin3Picks_TotalLessThan3_NoWinners(t *testing.T) {
	p := pWin3Picks(0, 2)
	if p != 0.0 {
		t.Errorf("pWin3Picks(0, 2) = %f, want 0.0", p)
	}
}

func TestPWin3Picks_OneWinnerInThree(t *testing.T) {
	// 1 winner in 3: P = 1 - (2/3)*(1/2)*(0/1) = 1 - 0 = 1.0
	p := pWin3Picks(1, 3)
	if p != 1.0 {
		t.Errorf("pWin3Picks(1, 3) = %f, want 1.0", p)
	}
}

func TestPWin3Picks_ZeroTotal(t *testing.T) {
	p := pWin3Picks(0, 0)
	if p != 0.0 {
		t.Errorf("pWin3Picks(0, 0) = %f, want 0.0", p)
	}
}

func TestAnalyzeFont_BasicEV(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		// RED transfigured gems (3 unique names)
		{Name: "Cleave of Rage", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "RED"},
		{Name: "Slam of Force", Variant: "20/20", Chaos: 50, Listings: 10, IsTransfigured: true, GemColor: "RED"},
		{Name: "Strike of Fear", Variant: "20/20", Chaos: 2, Listings: 10, IsTransfigured: true, GemColor: "RED"},
	}

	results := AnalyzeFont(now, gems)

	// Should have results for RED color across all 4 variants
	var found *FontResult
	for i := range results {
		if results[i].Color == "RED" && results[i].Variant == "20/20" {
			found = &results[i]
			break
		}
	}
	if found == nil {
		t.Fatal("expected RED/20/20 result")
	}

	if found.Pool != 3 {
		t.Errorf("Pool = %d, want 3", found.Pool)
	}
	// threshold for 20/20 = max(ceil(3.5*3), 5) = max(11, 5) = 11
	// winners: 100c and 50c are >= 11, so 2 winners
	if found.Winners != 2 {
		t.Errorf("Winners = %d, want 2", found.Winners)
	}
	// avgWin = (100+50)/2 = 75
	if math.Abs(found.AvgWin-75) > 0.01 {
		t.Errorf("AvgWin = %f, want 75", found.AvgWin)
	}
}

func TestAnalyzeFont_PoolIsUniqueNamesAcrossVariants(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Cleave of Rage", Variant: "1", Chaos: 10, Listings: 10, IsTransfigured: true, GemColor: "RED"},
		{Name: "Cleave of Rage", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "RED"},
		{Name: "Slam of Force", Variant: "20/20", Chaos: 50, Listings: 10, IsTransfigured: true, GemColor: "RED"},
	}

	results := AnalyzeFont(now, gems)

	// Pool should be 2 unique names (Cleave of Rage + Slam of Force), not 3 rows
	for _, r := range results {
		if r.Color == "RED" {
			if r.Pool != 2 {
				t.Errorf("Pool = %d, want 2 (unique names across variants)", r.Pool)
			}
			break
		}
	}
}

func TestAnalyzeFont_ExcludesCorruptedAndTrarthus(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Cleave of Rage", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "RED"},
		{Name: "Corrupted Gem of Rage", Variant: "20/20", Chaos: 500, Listings: 10, IsTransfigured: true, IsCorrupted: true, GemColor: "RED"},
		{Name: "Wave of Conviction of Trarthus", Variant: "20/20", Chaos: 500, Listings: 10, IsTransfigured: true, GemColor: "RED"},
	}

	results := AnalyzeFont(now, gems)
	for _, r := range results {
		if r.Color == "RED" && r.Variant == "20/20" {
			if r.Pool != 1 {
				t.Errorf("Pool = %d, want 1 (corrupted and Trarthus excluded)", r.Pool)
			}
			return
		}
	}
}

func TestAnalyzeFont_EmptyInput(t *testing.T) {
	results := AnalyzeFont(time.Now(), nil)
	if len(results) != 0 {
		t.Errorf("got %d results, want 0", len(results))
	}

	results = AnalyzeFont(time.Now(), []GemPrice{})
	if len(results) != 0 {
		t.Errorf("got %d results, want 0", len(results))
	}
}

func TestAnalyzeFont_LowListingsNotWinners(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Cleave of Rage", Variant: "20/20", Chaos: 100, Listings: 2, IsTransfigured: true, GemColor: "RED"},
		{Name: "Slam of Force", Variant: "20/20", Chaos: 50, Listings: 10, IsTransfigured: true, GemColor: "RED"},
	}

	results := AnalyzeFont(now, gems)
	for _, r := range results {
		if r.Color == "RED" && r.Variant == "20/20" {
			// Cleave has < 5 listings, so not a winner
			if r.Winners != 1 {
				t.Errorf("Winners = %d, want 1 (low listings excluded)", r.Winners)
			}
			return
		}
	}
}
