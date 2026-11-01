package bossloot

import (
	"encoding/json"
	"testing"
)

func TestItemsRejectInvalidTransport(t *testing.T) {
	for _, raw := range []string{
		`{"quantities":{"Eye":0},"counted":["Eye"]}`,
		`{"quantities":{"Eye":-1},"counted":["Eye"]}`,
		`{"quantities":{"Eye":1.5},"counted":["Eye"]}`,
		`{"quantities":{"Eye":1},"counted":[]}`,
		`{"quantities":{},"counted":["Eye","Eye"]}`,
		`{"quantities":{},"counted":[" "]}`,
		`{"quantities":null,"counted":[]}`,
		`{"quantities":{},"counted":null}`,
		`{"quantities":{},"counted":["bad\nkey"]}`,
	} {
		t.Run(raw, func(t *testing.T) {
			var items Items
			err := json.Unmarshal([]byte(raw), &items)
			if err == nil {
				err = items.Validate()
			}
			if err == nil {
				t.Fatalf("accepted invalid items %s", raw)
			}
		})
	}
}
func TestItemsAcceptOpaqueKeys(t *testing.T) {
	items := Items{Quantities: map[string]int64{"Unrecognized future item": 3}, Counted: []string{"Unrecognized future item", "category:test:2:rare"}}
	if err := items.Validate(); err != nil {
		t.Fatal(err)
	}
}
func TestSubmitRequiresVersionedMergeProvenance(t *testing.T) {
	valid := Submit{Profile: Profile{Boss: "catarina", ProfileKey: "Normal", ProfileVersion: 1}, Mode: "merge", Seed: ptr("seed"), Target: &Target{ID: "1", ExpectedVersion: 1}, IncomingPositiveKeys: []string{"Eye"}, Items: Items{Quantities: map[string]int64{}, Counted: []string{}}}
	if err := valid.Validate(); err != nil {
		t.Fatal(err)
	}
	valid.IncomingPositiveKeys = nil
	if err := valid.Validate(); err == nil {
		t.Fatal("merge accepted missing provenance")
	}
}
func ptr(s string) *string { return &s }
