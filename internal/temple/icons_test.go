package temple

import (
	"encoding/json"
	"os"
	"testing"
)

// templeIconMap is the committed category file the icon cache embeds. The test
// reads it from disk rather than through internal/icons so a failure names the
// file an operator has to regenerate.
const templeIconMap = "../icons/urls/temple.json"

// TestIconMap_coversTheServedSetAndTheRoomArtwork pins the two halves that
// IconPath deliberately does not check at runtime.
//
// A served name with no map entry is a 404 the client renders as its "?"
// fallback; a map entry with no served name is dead weight an operator still has
// to seed into the production volume. Both are build-time defects, so they fail
// here rather than in a browser.
func TestIconMap_coversTheServedSetAndTheRoomArtwork(t *testing.T) {
	raw, err := os.ReadFile(templeIconMap)
	if err != nil {
		t.Fatalf("read %s (regenerate with scripts/generate-temple-icons.py): %v", templeIconMap, err)
	}
	var urls map[string]string
	if err := json.Unmarshal(raw, &urls); err != nil {
		t.Fatalf("parse %s: %v", templeIconMap, err)
	}

	want := append(ItemNames(), RoomIconName)
	for _, name := range want {
		url, ok := urls[name]
		if !ok {
			t.Errorf("%s has no entry for %q; its icon would 404", templeIconMap, name)
			continue
		}
		if url == "" {
			t.Errorf("%s maps %q to an empty URL", templeIconMap, name)
		}
		delete(urls, name)
	}
	for name := range urls {
		t.Errorf("%s carries %q, which nothing serves", templeIconMap, name)
	}
}

func TestIconPath_escapesTheNameAsOnePathSegment(t *testing.T) {
	// The route parameter is one segment. An apostrophe and a space have to
	// arrive percent-encoded for the handler's url.PathUnescape to hand the map
	// the name it was keyed on.
	got := IconPath("Coward's Chains")
	want := "/api/gem-icon/Coward%27s%20Chains"
	if got != want {
		t.Errorf("IconPath(%q) = %q, want %q", "Coward's Chains", got, want)
	}
}

func TestIconPath_roomArtworkIsOneSharedEntry(t *testing.T) {
	// All 86 room-tier lines are lines on one Chronicle of Atzoatl and share its
	// artwork, so they resolve to one cached file rather than 86.
	got := IconPath(RoomIconName)
	want := "/api/gem-icon/Chronicle%20of%20Atzoatl"
	if got != want {
		t.Errorf("IconPath(RoomIconName) = %q, want %q", got, want)
	}
}
