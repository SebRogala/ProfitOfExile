package resolve

import "testing"

const keyLeague = "Temp-POE307"

func TestGemKey_levelOneWithZeroQualityIsTheLevelOneMarket(t *testing.T) {
	got := GemKey(keyLeague, "Siege Ballista", "1/0")
	want := Key{League: keyLeague, Kind: KindGem, Name: "Siege Ballista", Variant: "1"}
	if got != want {
		t.Fatalf("GemKey(1/0) = %+v, want %+v", got, want)
	}
}

func TestGemKey_levelTwentyWithZeroQualityIsTheLevelTwentyMarket(t *testing.T) {
	got := GemKey(keyLeague, "Siege Ballista", "20/0")
	want := Key{League: keyLeague, Kind: KindGem, Name: "Siege Ballista", Variant: "20"}
	if got != want {
		t.Fatalf("GemKey(20/0) = %+v, want %+v", got, want)
	}
}

func TestGemKey_twentyTwentyKeepsItsQuality(t *testing.T) {
	got := GemKey(keyLeague, "Siege Ballista", "20/20")
	if got.Variant != "20/20" {
		t.Fatalf("GemKey(20/20).Variant = %q, want %q", got.Variant, "20/20")
	}
}

func TestGemKey_corruptedSuffixIsADifferentMarket(t *testing.T) {
	corrupted := GemKey(keyLeague, "Siege Ballista", "21/20c")
	clean := GemKey(keyLeague, "Siege Ballista", "21/20")
	if corrupted == clean {
		t.Fatalf("GemKey(21/20c) == GemKey(21/20) = %+v, want different markets", clean)
	}
}

func TestGemKey_corruptedSuffixSetsTheCorruptedState(t *testing.T) {
	got := GemKey(keyLeague, "Siege Ballista", "21/20c")
	want := Key{League: keyLeague, Kind: KindGem, Name: "Siege Ballista", Variant: "21/20", Corrupted: true}
	if got != want {
		t.Fatalf("GemKey(21/20c) = %+v, want %+v", got, want)
	}
}

func TestGemKey_leaguesAreDifferentMarkets(t *testing.T) {
	a := GemKey("Temp-A", "Siege Ballista", "20/20")
	b := GemKey("Temp-B", "Siege Ballista", "20/20")
	if a == b {
		t.Fatalf("GemKey in Temp-A == GemKey in Temp-B = %+v, want different markets", a)
	}
}

func TestCurrencyKey_divineIsNotAGem(t *testing.T) {
	currency := CurrencyKey(keyLeague, "divine")
	gem := GemKey(keyLeague, "divine", "")
	if currency == gem {
		t.Fatalf("CurrencyKey(divine) == GemKey(divine) = %+v, want different markets", gem)
	}
}
