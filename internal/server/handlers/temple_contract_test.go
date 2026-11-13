package handlers

// Writer of desktop/contracts/temple-market.json; desktop/src-tauri/src/temple/market.rs reads it.
// Regenerate: docker compose exec -T app go test ./internal/server/handlers/ -run TestTempleMarket_contractGolden -update

import (
	"bytes"
	"encoding/json"
	"flag"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"
	"time"

	"profitofexile/internal/temple"
)

var update = flag.Bool("update", false, "rewrite the desktop contract goldens")

const templeGoldenPath = "../../../desktop/contracts/temple-market.json"

func TestTempleMarket_contractGolden(t *testing.T) {
	at := time.Date(2026, 11, 1, 12, 0, 0, 0, time.UTC)
	cache := temple.NewCache(templeScope)
	cache.Set(temple.Build("Allflame", []temple.Line{
		{Category: temple.RoomCategory, Time: at, Name: "A (Tier 1)", Chaos: 10, Listings: 700},
		{Category: temple.RoomCategory, Time: at, Name: "B (Tier 1)", Chaos: 10, Listings: 700},
		{Category: temple.RoomCategory, Time: at, Name: "Locus of Corruption (Tier 3)", Chaos: 853.6, Listings: 2489},
		{Category: "UniqueAccessory", Time: at, Name: "Coward's Chains", Chaos: 1, Divine: 0.002, Listings: 343},
	}, nil, nil))

	rec := httptest.NewRecorder()
	TempleMarket(cache, templeScope)(rec, httptest.NewRequest(http.MethodGet, "/api/analysis/temple-market", nil))
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}

	var top map[string]json.RawMessage
	if err := json.Unmarshal(rec.Body.Bytes(), &top); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	for _, key := range []string{"league", "asOf", "floor", "floorRule", "categoriesSeen", "rooms", "items"} {
		switch string(top[key]) {
		case "", "null", `""`, "0", "[]":
			t.Errorf("golden field %q is zero (%s); the fixture must set every top-level field", key, top[key])
		}
	}

	checkContractGolden(t, templeGoldenPath, rec.Body.Bytes(),
		"docker compose exec -T app go test ./internal/server/handlers/ -run TestTempleMarket_contractGolden -update")
}

func checkContractGolden(t *testing.T, path string, body []byte, regenerate string) {
	t.Helper()
	var pretty bytes.Buffer
	if err := json.Indent(&pretty, bytes.TrimSpace(body), "", "  "); err != nil {
		t.Fatalf("indent response: %v", err)
	}
	pretty.WriteByte('\n')
	if *update {
		if err := os.WriteFile(path, pretty.Bytes(), 0o644); err != nil {
			t.Fatalf("write golden: %v", err)
		}
		return
	}
	committed, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read golden %s: %v (regenerate: %s)", path, err, regenerate)
	}
	if !bytes.Equal(committed, pretty.Bytes()) {
		t.Errorf("%s differs from the served response; if the change is intended, regenerate: %s\n got:\n%s", path, regenerate, pretty.String())
	}
}
