package lab

import (
	"testing"
	"time"

	"profitofexile/internal/price/resolve"
)

var gemObservationSnap = time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)

func TestGemObservations_carriesTheNinjaSampleCount(t *testing.T) {
	sample := 2
	gems := []GemPrice{{Name: "Siege Ballista of Splintering", Variant: "20/20", Chaos: 1000, SampleCount: &sample}}

	obs := GemObservations("Temp-POE307", gemObservationSnap, gems)

	if len(obs) != 1 {
		t.Fatalf("observations = %d, want 1", len(obs))
	}
	if obs[0].SampleCount == nil || *obs[0].SampleCount != 2 {
		t.Errorf("SampleCount = %v, want 2", obs[0].SampleCount)
	}
}

func TestGemObservations_lineIsOneNinjaChaosPriceAtTheSnapshotTime(t *testing.T) {
	gems := []GemPrice{{Name: "Siege Ballista of Splintering", Variant: "20/20", Chaos: 1000}}

	obs := GemObservations("Temp-POE307", gemObservationSnap, gems)

	if len(obs) != 1 {
		t.Fatalf("observations = %d, want 1", len(obs))
	}
	want := resolve.Key{League: "Temp-POE307", Kind: resolve.KindGem, Name: "Siege Ballista of Splintering", Variant: "20/20"}
	if obs[0].Key != want {
		t.Errorf("Key = %+v, want %+v", obs[0].Key, want)
	}
	if obs[0].Source != resolve.SourceNinja || !obs[0].At.Equal(gemObservationSnap) {
		t.Errorf("Source, At = %q, %v; want ninja, %v", obs[0].Source, obs[0].At, gemObservationSnap)
	}
	if len(obs[0].Prices) != 1 || obs[0].Prices[0] != (resolve.Money{Amount: 1000, Currency: "chaos"}) {
		t.Errorf("Prices = %+v, want [{1000 chaos}]", obs[0].Prices)
	}
}

func TestGemObservations_corruptedLineIsTheCorruptedMarket(t *testing.T) {
	gems := []GemPrice{{Name: "Siege Ballista of Splintering", Variant: "21/20", Chaos: 300, IsCorrupted: true}}

	obs := GemObservations("Temp-POE307", gemObservationSnap, gems)

	if len(obs) != 1 {
		t.Fatalf("observations = %d, want 1", len(obs))
	}
	if !obs[0].Key.Corrupted || obs[0].Key.Variant != "21/20" {
		t.Errorf("Key = %+v, want Corrupted true, Variant 21/20", obs[0].Key)
	}
}

func TestGemObservations_corruptedSuffixVariantNormalizesToTheBareVariant(t *testing.T) {
	gems := []GemPrice{{Name: "Siege Ballista of Splintering", Variant: "21/20c", Chaos: 300, IsCorrupted: true}}

	obs := GemObservations("Temp-POE307", gemObservationSnap, gems)

	if len(obs) != 1 {
		t.Fatalf("observations = %d, want 1", len(obs))
	}
	if obs[0].Key.Variant != "21/20" || !obs[0].Key.Corrupted {
		t.Errorf("Key = %+v, want Variant 21/20, Corrupted true", obs[0].Key)
	}
}

func TestGemObservations_zeroPricedLineIsNoObservation(t *testing.T) {
	gems := []GemPrice{
		{Name: "Siege Ballista of Splintering", Variant: "20/20", Chaos: 0},
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 40},
	}

	obs := GemObservations("Temp-POE307", gemObservationSnap, gems)

	if len(obs) != 1 || obs[0].Key.Name != "Spark of Nova" {
		t.Errorf("observations = %+v, want only Spark of Nova", obs)
	}
}
