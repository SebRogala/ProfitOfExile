package exchange

import (
	"bytes"
	"encoding/json"
	"log/slog"
	"sort"
	"strings"
	"sync"
	"testing"
)

// delirumOrbID is a real asset entry whose icon is null: poewiki's cargo table
// lists the item but carries no inventory image for it. It is the boundary
// between "the asset does not know this id" and "the asset knows it and there is
// no artwork", which IconPath must answer the same way and UnknownItems must
// not.
const delirumOrbID = "Metadata/Items/Currency/CurrencyAfflictionOrbGeneric"

// missingID is shaped like a feed id but is not in the asset — a stand-in for an
// item GGG adds after the last regeneration.
const missingID = "Metadata/Items/Currency/CurrencyNotInTheAssetYet"

// The icon-coverage floor is a share of the entries, not an absolute count: the
// asset grows every league, so a fixed number drifts out of meaning while the
// share does not. 70% sits well under the 85.3% the asset currently carries, so
// ordinary poewiki churn (a handful of items losing their inventory image)
// leaves it alone, while a join that dropped a whole category — all 471
// divination cards, say — takes coverage under it.
const (
	minIconCoverageNumerator   = 7
	minIconCoverageDenominator = 10
)

// wikiImagePrefix is where every icon URL in the asset must point. The cache
// fetches these URLs verbatim, so a relative path or a wiki *page* URL sneaking
// into the asset would serve HTML as an icon.
const wikiImagePrefix = "https://www.poewiki.net/images/"

// unknownItemIDs returns the "id" attribute of every unknown-item warning the
// capture holds, in the order logged.
func unknownItemIDs(t *testing.T, capture *logCapture) []string {
	t.Helper()
	var ids []string
	for _, rec := range capture.records() {
		if rec.Message != "currency-exchange: unknown item id" {
			continue
		}
		var (
			id    string
			found bool
		)
		rec.Attrs(func(a slog.Attr) bool {
			if a.Key != "id" {
				return true
			}
			id, found = a.Value.String(), true
			return false
		})
		if !found {
			t.Fatalf("unknown-item record carries no %q attribute: %v", "id", rec)
		}
		ids = append(ids, id)
	}
	return ids
}

// assetEntries decodes the embedded asset with its key order preserved, which
// the map the package parses into cannot report.
func assetEntries(t *testing.T) (ids []string, entries map[string]assetEntry) {
	t.Helper()
	if err := json.Unmarshal(itemDataJSON, &entries); err != nil {
		t.Fatalf("unmarshal embedded asset: %v", err)
	}
	decoder := json.NewDecoder(bytes.NewReader(itemDataJSON))
	if _, err := decoder.Token(); err != nil { // opening brace
		t.Fatalf("read asset opening token: %v", err)
	}
	for decoder.More() {
		key, err := decoder.Token()
		if err != nil {
			t.Fatalf("read asset key: %v", err)
		}
		ids = append(ids, key.(string))
		var skip json.RawMessage
		if err := decoder.Decode(&skip); err != nil {
			t.Fatalf("read asset value for %q: %v", key, err)
		}
	}
	return ids, entries
}

func TestLookupItem_knownID_returnsTheInGameNameAndItsWikiIconURL(t *testing.T) {
	tests := []struct {
		name        string
		id          string
		wantName    string
		wantIconURL string
	}{
		{
			name:        "chaos orb",
			id:          ChaosID,
			wantName:    "Chaos Orb",
			wantIconURL: "https://www.poewiki.net/images/9/9c/Chaos_Orb_inventory_icon.png",
		},
		{
			name:        "divine orb",
			id:          DivineID,
			wantName:    "Divine Orb",
			wantIconURL: "https://www.poewiki.net/images/5/58/Divine_Orb_inventory_icon.png",
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			item, ok := LookupItem(tc.id)

			if !ok {
				t.Fatalf("LookupItem(%q) reported a miss, want the asset entry", tc.id)
			}
			if item.Name != tc.wantName {
				t.Errorf("name = %q, want %q", item.Name, tc.wantName)
			}
			if item.IconURL != tc.wantIconURL {
				t.Errorf("iconURL = %q, want %q", item.IconURL, tc.wantIconURL)
			}
		})
	}
}

func TestLookupItem_idTheAssetDoesNotCover_reportsAMiss(t *testing.T) {
	// The miss is the signal the caller acts on: it is what separates a name the
	// asset supplied from one Humanize invented.
	item, ok := LookupItem(missingID)

	if ok {
		t.Fatalf("LookupItem(%q) = %+v, true; want a miss", missingID, item)
	}
	if item != (Item{}) {
		t.Errorf("missed lookup returned %+v, want the zero Item", item)
	}
}

func TestLookupItem_itemWithoutArtwork_isFoundWithAnEmptyIconURL(t *testing.T) {
	// A null icon in the asset must land on "found, no artwork", not on a miss:
	// the item still has a correct name to render.
	item, ok := LookupItem(delirumOrbID)

	if !ok {
		t.Fatalf("LookupItem(%q) reported a miss, want the asset entry", delirumOrbID)
	}
	if item.Name != "Delirium Orb" {
		t.Errorf("name = %q, want %q", item.Name, "Delirium Orb")
	}
	if item.IconURL != "" {
		t.Errorf("iconURL = %q, want empty — the asset carries null for this item", item.IconURL)
	}
}

func TestDisplayName_knownID_prefersTheAssetNameOverTheHumanizedID(t *testing.T) {
	// Humanize alone produces "Reroll Rare" for a Chaos Orb: readable, and wrong.
	// This is the whole reason the asset exists.
	got := DisplayName(ChaosID)

	if got != "Chaos Orb" {
		t.Fatalf("DisplayName(%q) = %q, want %q", ChaosID, got, "Chaos Orb")
	}
	if humanized := Humanize(ChaosID); got == humanized {
		t.Errorf("DisplayName returned the humanized id %q — the asset name was not used", humanized)
	}
}

func TestDisplayName_idTheAssetDoesNotCover_fallsBackToHumanize(t *testing.T) {
	got := DisplayName(missingID)

	if want := Humanize(missingID); got != want {
		t.Fatalf("DisplayName(%q) = %q, want the humanized fallback %q", missingID, got, want)
	}
	if got == missingID {
		t.Errorf("DisplayName(%q) returned the raw metadata id, want it split into words", missingID)
	}
}

func TestIconURLs_returnsACopyCallersCannotReachTheAssetThrough(t *testing.T) {
	// The caller (internal/gemicon's constructor) keeps the map for the process's
	// lifetime, so handing out the package's own map would make the embedded
	// asset mutable through a shared reference.
	first := IconURLs()
	delete(first, ChaosID)
	first[missingID] = "https://example.invalid/injected.png"

	second := IconURLs()

	if got := second[ChaosID]; got == "" {
		t.Error("a deletion from a returned map removed the chaos orb from the asset")
	}
	if got, injected := second[missingID]; injected {
		t.Errorf("a write to a returned map added %q = %q to the asset", missingID, got)
	}
}

func TestIconURLs_omitsEveryItemWithoutArtwork(t *testing.T) {
	// An empty URL in this map would make the cache fetch "" on first request and
	// serve a 502 forever for an item that simply has no icon.
	urls := IconURLs()

	if _, present := urls[delirumOrbID]; present {
		t.Errorf("%q has a null icon in the asset but appears in IconURLs()", delirumOrbID)
	}
	for id, url := range urls {
		if url == "" {
			t.Fatalf("IconURLs()[%q] = \"\", want only items that have an icon", id)
		}
	}
	if _, present := urls[ChaosID]; !present {
		t.Errorf("IconURLs() has no entry for %q, want the icon it carries in the asset", ChaosID)
	}
}

func TestIconPath_itemWithAnIcon_escapesTheIDIntoOneRouteSegment(t *testing.T) {
	// The slashes must become %2F: the id is a single {name} route parameter, not
	// four path segments, and the icon handler unescapes it back.
	path, ok := IconPath(ChaosID)

	if !ok {
		t.Fatalf("IconPath(%q) reported no icon, want a path", ChaosID)
	}
	want := "/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare"
	if path != want {
		t.Errorf("IconPath(%q) = %q, want %q", ChaosID, path, want)
	}
	if strings.HasPrefix(path, "/api/") {
		t.Errorf("IconPath returned %q — the path is API-relative, clients join it onto a base ending in /api", path)
	}
}

func TestIconPath_noArtwork_returnsNoPathSoClientsBuildNoURL(t *testing.T) {
	tests := []struct {
		name string
		id   string
	}{
		{"an asset entry whose icon is null", delirumOrbID},
		{"an id the asset does not cover", missingID},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			path, ok := IconPath(tc.id)

			if ok {
				t.Fatalf("IconPath(%q) = %q, true; want no path", tc.id, path)
			}
			if path != "" {
				t.Errorf("IconPath(%q) returned %q alongside false, want an empty path", tc.id, path)
			}
		})
	}
}

func TestUnknownItems_Note_logsOneIDOnceHoweverOftenItRecurs(t *testing.T) {
	// The id appears in every leg of every play that touches it, on every
	// recompute, so an unconditional line would bury the signal it raises.
	capture := captureLogs(t)
	unknown := &UnknownItems{}

	unknown.Note(missingID)
	unknown.Note(missingID)
	unknown.Note(missingID)

	if got := unknownItemIDs(t, capture); len(got) != 1 || got[0] != missingID {
		t.Fatalf("logged ids = %v, want exactly one %q", got, missingID)
	}
}

func TestUnknownItems_Note_logsEveryDistinctIDItSees(t *testing.T) {
	// "Once per id" must not collapse into "once, ever": a second unnamed item is
	// a second thing the asset is missing.
	capture := captureLogs(t)
	unknown := &UnknownItems{}
	second := missingID + "Two"

	unknown.Note(missingID)
	unknown.Note(second)
	unknown.Note(missingID)
	unknown.Note(second)

	got := unknownItemIDs(t, capture)
	sort.Strings(got)
	want := []string{missingID, second}
	sort.Strings(want)
	if len(got) != len(want) || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("logged ids = %v, want one line per distinct id %v", got, want)
	}
}

func TestUnknownItems_Note_logsOnceUnderConcurrentCallers(t *testing.T) {
	// The handler decorates responses from several requests at once, so the
	// once-per-id set is reached concurrently. Run with -race.
	capture := captureLogs(t)
	unknown := &UnknownItems{}

	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			unknown.Note(missingID)
		}()
	}
	wg.Wait()

	if got := unknownItemIDs(t, capture); len(got) != 1 {
		t.Fatalf("logged ids = %v, want exactly one line from 8 concurrent callers", got)
	}
}

func TestItemAsset_everyEntryCarriesANonEmptyName(t *testing.T) {
	// A blank name renders as a blank chip with nothing in the logs to say why;
	// the generator excludes unnamed ids and this is what holds it to that.
	_, entries := assetEntries(t)

	if len(entries) == 0 {
		t.Fatal("the embedded asset is empty")
	}
	for id, entry := range entries {
		if entry.Name == "" {
			t.Errorf("asset entry %q has an empty name", id)
		}
	}
}

func TestItemAsset_iconCoverageStaysAboveTheFloor(t *testing.T) {
	// A regeneration that lost its icon join would still produce a complete,
	// name-only asset and every icon on the page would silently disappear.
	_, entries := assetEntries(t)

	if len(entries) == 0 {
		t.Fatal("the embedded asset is empty")
	}
	icons := 0
	for _, entry := range entries {
		if entry.Icon != nil && *entry.Icon != "" {
			icons++
		}
	}
	if icons*minIconCoverageDenominator < len(entries)*minIconCoverageNumerator {
		t.Errorf("asset carries %d icons over %d entries (%.1f%%), want at least %d%%",
			icons, len(entries), 100*float64(icons)/float64(len(entries)),
			100*minIconCoverageNumerator/minIconCoverageDenominator)
	}
}

func TestItemAsset_everyIconURLPointsAtAPoewikiImageFile(t *testing.T) {
	_, entries := assetEntries(t)

	for id, entry := range entries {
		if entry.Icon == nil {
			continue
		}
		if !strings.HasPrefix(*entry.Icon, wikiImagePrefix) {
			t.Errorf("asset entry %q icon = %q, want a %s URL the cache can fetch verbatim", id, *entry.Icon, wikiImagePrefix)
		}
	}
}

func TestItemAsset_idsAreSortedSoRegenerationDiffsStayReadable(t *testing.T) {
	// The generator writes the file sorted by id. Unsorted output would turn each
	// league's regeneration into a whole-file diff and hide what actually changed.
	ids, _ := assetEntries(t)

	for i := 1; i < len(ids); i++ {
		if ids[i-1] >= ids[i] {
			t.Fatalf("asset id %d (%q) does not sort before id %d (%q)", i-1, ids[i-1], i, ids[i])
		}
	}
}
