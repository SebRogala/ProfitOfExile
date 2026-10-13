package harvest

import (
	"encoding/json"
	"os"
	"reflect"
	"testing"
)

const (
	oracleTiersPath  = "../../cmd/harvest-tiers/testdata/oracle-nocutoff.json"
	oraclePricesPath = "../../cmd/harvest-tiers/testdata/prices-allflame-2026-09-08.json"
)

func TestClassifyFamilies_reproducesTheOracle(t *testing.T) {
	rawPrices, err := os.ReadFile(oraclePricesPath)
	if err != nil {
		t.Fatalf("read the price file %s: %v", oraclePricesPath, err)
	}
	var prices map[string]map[string]float64
	if err := json.Unmarshal(rawPrices, &prices); err != nil {
		t.Fatalf("decode the price file: %v", err)
	}
	rawOracle, err := os.ReadFile(oracleTiersPath)
	if err != nil {
		t.Fatalf("read the oracle %s: %v", oracleTiersPath, err)
	}
	var oracle map[string]FamilyTiers
	if err := json.Unmarshal(rawOracle, &oracle); err != nil {
		t.Fatalf("decode the oracle: %v", err)
	}

	got := ClassifyFamilies(prices)

	for _, fam := range []string{"fossil", "delirium"} {
		want, ok := oracle[fam]
		if !ok {
			t.Fatalf("oracle has no family %s", fam)
		}
		if len(want.BoundariesChaos) == 0 {
			t.Fatalf("oracle %s has no boundaries; the test would not check them", fam)
		}
		if !reflect.DeepEqual(got[fam].ByItem, want.ByItem) {
			t.Errorf("family %s ByItem differs from the oracle", fam)
		}
		if !reflect.DeepEqual(got[fam].Names, want.Names) {
			t.Errorf("family %s Names = %v, want %v", fam, got[fam].Names, want.Names)
		}
		if !reflect.DeepEqual(got[fam].BoundariesChaos, want.BoundariesChaos) {
			t.Errorf("family %s BoundariesChaos = %v, want %v", fam, got[fam].BoundariesChaos, want.BoundariesChaos)
		}
	}
}

func TestClassifyFamilies_groupsEachFamilySeparately(t *testing.T) {
	both := map[string]map[string]float64{
		"a": {"x": 100, "y": 10, "z": 9},
		"b": {"p": 1, "q": 2},
	}
	alone := map[string]map[string]float64{
		"a": both["a"],
	}

	gotBoth := ClassifyFamilies(both)
	gotAlone := ClassifyFamilies(alone)

	if !reflect.DeepEqual(gotAlone["a"], gotBoth["a"]) {
		t.Fatalf("family a classified alone = %+v, together with b = %+v; families must not share a group",
			gotAlone["a"], gotBoth["a"])
	}
	bItems := make([]string, 0, len(gotBoth["b"].ByItem))
	for id := range gotBoth["b"].ByItem {
		bItems = append(bItems, id)
	}
	if len(bItems) != 2 || gotBoth["b"].ByItem["p"] == "" || gotBoth["b"].ByItem["q"] == "" {
		t.Fatalf("family b ByItem keys = %v, want exactly p and q", bItems)
	}
}

func TestClassifyFamilies_emptyFamilyIsAbsent(t *testing.T) {
	got := ClassifyFamilies(map[string]map[string]float64{"a": {}})
	if _, ok := got["a"]; ok {
		t.Fatalf("family a with no items is present in the result: %+v", got["a"])
	}
}
