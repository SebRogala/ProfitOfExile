package main

import (
	"bytes"
	"encoding/json"
	"os"
	"reflect"
	"testing"
)

const (
	pricesPath        = "testdata/prices-allflame-2026-09-08.json"
	oraclePath        = "testdata/oracle-nocutoff.json"
	committedTierPath = "../../desktop/src/lib/harvest/__fixtures__/harvest-tiers.json"
)

func readPrices(t *testing.T) map[string]map[string]float64 {
	t.Helper()
	raw, err := os.ReadFile(pricesPath)
	if err != nil {
		t.Fatalf("read the price file: %v", err)
	}
	var prices map[string]map[string]float64
	if err := json.Unmarshal(raw, &prices); err != nil {
		t.Fatalf("decode the price file: %v", err)
	}
	return prices
}

// The oracle was computed outside the repo by the pre-move Lab algorithm with
// its 5c cutoff removed; a regression in internal/price/tier changes the
// generated tiers and fails here even when the committed fixture is
// regenerated to match.
func TestGenerate_reproducesTheNoCutoffOracle(t *testing.T) {
	raw, err := os.ReadFile(oraclePath)
	if err != nil {
		t.Fatalf("read the oracle: %v", err)
	}
	var want map[string]familyTiers
	if err := json.Unmarshal(raw, &want); err != nil {
		t.Fatalf("decode the oracle: %v", err)
	}

	got := generate(readPrices(t)).Families

	if !reflect.DeepEqual(got, want) {
		for fam := range want {
			if !reflect.DeepEqual(got[fam], want[fam]) {
				t.Errorf("family %s:\n got  %+v\n want %+v", fam, got[fam], want[fam])
			}
		}
		for fam := range got {
			if _, ok := want[fam]; !ok {
				t.Errorf("family %s generated but absent from the oracle", fam)
			}
		}
	}
}

func TestCommittedFixture_matchesAFreshRun(t *testing.T) {
	committed, err := os.ReadFile(committedTierPath)
	if err != nil {
		t.Fatalf("read %s: %v\nregenerate it: %s", committedTierPath, err, regenerateCommand)
	}

	fresh := encode(generate(readPrices(t)))

	if !bytes.Equal(committed, fresh) {
		t.Fatalf("%s differs from a fresh generator run\nregenerate it: %s", committedTierPath, regenerateCommand)
	}
}
