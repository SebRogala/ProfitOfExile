package temple

import (
	"encoding/json"
	"os/exec"
	"slices"
	"testing"

	"profitofexile/internal/collector"
)

// TestFeeds_matchCollectorItemEndpoints is why this package restates the seven
// categories instead of importing them: the two lists answer different questions
// (which to poll, which to read) and nothing under internal/server imports the
// collector, so the drift is closed here rather than by a production import.
//
// A category added to the collector and not here would be polled, stored, and
// silently never read; a topic that differs by one character would leave the
// server subscribed to a topic nobody publishes on.
func TestFeeds_matchCollectorItemEndpoints(t *testing.T) {
	if len(feeds) != len(collector.ItemEndpoints) {
		t.Fatalf("temple feeds = %d, collector.ItemEndpoints = %d", len(feeds), len(collector.ItemEndpoints))
	}

	mine := make(map[string]Feed, len(feeds))
	for _, f := range feeds {
		mine[f.Category] = f
	}

	for _, want := range collector.ItemEndpoints {
		got, ok := mine[want.Category]
		if !ok {
			t.Errorf("collector polls category %q, which internal/temple never reads", want.Category)
			continue
		}
		if got.Endpoint != want.Endpoint {
			t.Errorf("%s endpoint = %q, collector publishes %q", want.Category, got.Endpoint, want.Endpoint)
		}
		if wantTopic := "poe/collector/" + want.TopicSuffix; got.Topic != wantTopic {
			t.Errorf("%s topic = %q, collector publishes on %q", want.Category, got.Topic, wantTopic)
		}
	}
}

func TestTopics_areTheSevenItemTopicsTheServerSubscribesTo(t *testing.T) {
	got := Topics()
	if len(got) != 7 {
		t.Fatalf("Topics() = %d topics, want 7", len(got))
	}
	want := map[string]bool{
		"poe/collector/items/incursion-temple": true,
		"poe/collector/items/vial":             true,
		"poe/collector/items/unique-armour":    true,
		"poe/collector/items/unique-accessory": true,
		"poe/collector/items/unique-weapon":    true,
		"poe/collector/items/unique-jewel":     true,
		"poe/collector/items/unique-flask":     true,
	}
	for _, topic := range got {
		if !want[topic] {
			t.Errorf("Topics() carries %q, which is not an item topic", topic)
		}
		delete(want, topic)
	}
	for topic := range want {
		t.Errorf("Topics() is missing %q", topic)
	}
}

func TestCategories_areTheSevenItemSnapshotsCategories(t *testing.T) {
	got := Categories()
	if len(got) != 7 {
		t.Fatalf("Categories() = %d, want 7", len(got))
	}
	if got[0] != RoomCategory {
		t.Errorf("Categories()[0] = %q, want the room category %q first", got[0], RoomCategory)
	}
}

func TestCategories_returnsACopyTheCallerCannotWriteThrough(t *testing.T) {
	// The result is handed straight to a query parameter; aliasing package state
	// there would let one caller rewrite every later query's predicate.
	first := Categories()
	first[0] = "mutated by the caller"

	if second := Categories(); second[0] == "mutated by the caller" {
		t.Errorf("Categories()[0] = %q after a caller wrote to an earlier result", second[0])
	}
}

func TestIsFeedEndpoint_acceptsEveryItemEndpoint(t *testing.T) {
	for _, f := range feeds {
		if !IsFeedEndpoint(f.Endpoint) {
			t.Errorf("IsFeedEndpoint(%q) = false; the %s tick would never recompute the market", f.Endpoint, f.Category)
		}
	}
}

func TestIsFeedEndpoint_rejectsTheGemAndCurrencyEndpoints(t *testing.T) {
	// The server dispatches on this, and the gem branch below it runs the whole
	// v2/font/dedication chain. Returning true for a gem tick would run a temple
	// recompute and skip that chain entirely.
	for _, endpoint := range []string{"ninja-gems", "ninja-currency", "ninja-fragments", "", "items/vial"} {
		if IsFeedEndpoint(endpoint) {
			t.Errorf("IsFeedEndpoint(%q) = true, want false", endpoint)
		}
	}
}

// TestKeepCritical_generateTempleIconsScriptReadsTheFeedCategories pins that the
// icon generator reads the same categories, in order, as the server: a category
// the script missed would silently get no icons.
func TestKeepCritical_generateTempleIconsScriptReadsTheFeedCategories(t *testing.T) {
	python, err := exec.LookPath("python3")
	if err != nil {
		t.Fatalf("python3 is required to load scripts/generate-temple-icons.py: %v", err)
	}
	const loader = `import importlib.util, json
spec = importlib.util.spec_from_file_location("gen", "../../scripts/generate-temple-icons.py")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
print(json.dumps([mod.ROOM_CATEGORY] + mod.ITEM_CATEGORIES))`
	out, err := exec.Command(python, "-I", "-c", loader).Output()
	if err != nil {
		t.Fatalf("load script: %v", err)
	}
	var got []string
	if err := json.Unmarshal(out, &got); err != nil {
		t.Fatalf("decode script output %q: %v", out, err)
	}
	if want := Categories(); !slices.Equal(got, want) {
		t.Fatalf("script categories = %v, want %v", got, want)
	}
}
