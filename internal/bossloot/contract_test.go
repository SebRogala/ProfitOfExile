package bossloot

// Writer of desktop/contracts/boss-loot-{submit,runs,stats}.json; desktop/src-tauri/src/loot_api.rs reads them.
// Marshals the model types the way handlers.bossRespond does (json.NewEncoder), DB-free.
// Regenerate: docker compose exec -T app go test ./internal/bossloot/ -run TestBossLoot_contractGolden -update

import (
	"bytes"
	"encoding/json"
	"flag"
	"os"
	"testing"
	"time"
)

var update = flag.Bool("update", false, "rewrite the desktop contract goldens")

const bossRegenerate = "docker compose exec -T app go test ./internal/bossloot/ -run TestBossLoot_contractGolden -update"

func contractRun() Run {
	date := time.Date(2026, 11, 1, 12, 0, 0, 0, time.UTC)
	seed := "seed-42"
	return Run{
		Profile: Profile{Boss: "Sirus", ProfileKey: "sirus-uber", ProfileVersion: 3},
		ID:      "run-1", League: "Allflame", Run: 7, Date: &date, Seed: &seed, Version: 2,
		Items: Items{Quantities: map[string]int64{"awakeners-orb": 1}, Counted: []string{"awakeners-orb"}},
	}
}

func TestBossLoot_contractGolden(t *testing.T) {
	cases := []struct {
		path string
		body any
	}{
		{"../../desktop/contracts/boss-loot-submit.json", SubmitOutcome{State: "created", Run: contractRun(), DroppedPositiveKeys: []string{"old-key"}}},
		{"../../desktop/contracts/boss-loot-runs.json", Page{Total: 41, Page: 2, PageSize: 20, Items: []Run{contractRun()}}},
		{"../../desktop/contracts/boss-loot-stats.json", Stats{Runs: 41, Devices: 3, NextRun: 42, Items: map[string]ItemStats{"awakeners-orb": {Drops: 5, EligibleRuns: 40}}}},
	}
	for _, c := range cases {
		t.Run(c.path, func(t *testing.T) {
			var encoded bytes.Buffer
			if err := json.NewEncoder(&encoded).Encode(c.body); err != nil {
				t.Fatalf("encode: %v", err)
			}
			var pretty bytes.Buffer
			if err := json.Indent(&pretty, bytes.TrimSpace(encoded.Bytes()), "", "  "); err != nil {
				t.Fatalf("indent: %v", err)
			}
			pretty.WriteByte('\n')
			if *update {
				if err := os.WriteFile(c.path, pretty.Bytes(), 0o644); err != nil {
					t.Fatalf("write golden: %v", err)
				}
				return
			}
			committed, err := os.ReadFile(c.path)
			if err != nil {
				t.Fatalf("read golden %s: %v (regenerate: %s)", c.path, err, bossRegenerate)
			}
			if !bytes.Equal(committed, pretty.Bytes()) {
				t.Errorf("%s differs from the encoded model; if the change is intended, regenerate: %s\n got:\n%s", c.path, bossRegenerate, pretty.String())
			}
		})
	}
}
