package exchange

import (
	_ "embed"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/url"
	"sync"
)

// itemDataJSON is the generated name/icon asset, keyed by feed item id.
//
// The feed identifies every item by its metadata id and sends no display name
// and no icon, so both have to be carried in the binary. The asset is produced
// by scripts/generate-currency-exchange-items.py from the RePoE-fork base-item
// dump (names) joined against poewiki's items cargo table (icons), and is
// committed: it is the source of truth at build time, not something fetched at
// runtime. Regenerate it once per league, when GGG adds items.
//
//go:embed itemdata/items.json
var itemDataJSON []byte

// iconPathPrefix is the API-RELATIVE path of the currency-exchange icon route.
//
// It deliberately omits the "/api" the router mounts the route under, because
// every client already holds an API base that ends in "/api" (desktop:
// getApiBase in desktop/src/lib/api.ts). A client joins base + path; putting
// "/api" here too would produce "/api/api/...".
const iconPathPrefix = "/currency-exchange/icon/"

// Item is the display data for one feed item id.
//
// IconURL is the upstream poewiki image URL and is empty for an item that has
// no icon in the asset. It is NOT what a client fetches — clients go through
// IconPath, which points at this server's cache (docs/adr/012-*.md: poewiki
// 403s the production VPS, so the server serves its own copy). IconURL is here
// for the cache to fetch and for the pre-seed script's map.
type Item struct {
	Name    string
	IconURL string
}

// assetEntry is the on-disk shape of one asset record. icon is null for an item
// the generator found no wiki image for, which is why it is a pointer: a
// missing icon and an empty string must both land on Item.IconURL == "".
type assetEntry struct {
	Name string  `json:"name"`
	Icon *string `json:"icon"`
}

// itemsByID is parsed once, at package initialisation.
//
// The parse panics rather than returning an error, and it runs at init rather
// than lazily on first lookup, because the input is an embedded build artifact:
// a malformed asset is a defect in the committed file, identical in every
// process, and unrecoverable at runtime. Failing at process start turns that
// into an immediate, obvious crash. The alternatives are worse — an error
// return would put an impossible branch on every caller of DisplayName, and a
// silent empty map would quietly degrade every item on the page to its
// humanized id with nothing in the logs to say why.
var itemsByID = parseItemData(itemDataJSON)

func parseItemData(raw []byte) map[string]Item {
	var entries map[string]assetEntry
	if err := json.Unmarshal(raw, &entries); err != nil {
		panic(fmt.Sprintf("currency-exchange: parse embedded itemdata/items.json: %v", err))
	}
	items := make(map[string]Item, len(entries))
	for id, entry := range entries {
		item := Item{Name: entry.Name}
		if entry.Icon != nil {
			item.IconURL = *entry.Icon
		}
		items[id] = item
	}
	return items
}

// LookupItem returns the asset entry for a feed item id.
//
// A false result means the id is not in the asset at all — a new item GGG
// added since the last regeneration, or an id outside the eight categories the
// generator covers. Callers that render it should fall back through
// DisplayName and record it with UnknownItems.
func LookupItem(id string) (Item, bool) {
	item, ok := itemsByID[id]
	return item, ok
}

// DisplayName returns the in-game name for a feed item id, falling back to
// Humanize.
//
// Humanize is the fallback, not the mechanism: it splits the id's CamelCase
// tail and is readable but frequently wrong ("Reroll Rare" for a Chaos Orb).
// The asset is what makes names correct; this function exists so an id the
// asset does not cover still renders as something rather than as a raw
// metadata path.
func DisplayName(id string) string {
	// The empty-name guard is redundant against the generator, which excludes
	// unnamed ids, and against the asset test that asserts it. It is kept
	// because the failure it prevents — a blank chip in the UI with no other
	// symptom — is worse than the humanized fallback it costs.
	if item, ok := LookupItem(id); ok && item.Name != "" {
		return item.Name
	}
	return Humanize(id)
}

// IconURLs returns a fresh id → upstream poewiki URL map for every item that
// has an icon.
//
// The copy is deliberate: the caller is internal/gemicon's cache constructor,
// which keeps the map for the process's lifetime, and handing out the package's
// own map would make the embedded asset mutable through a shared reference.
func IconURLs() map[string]string {
	urls := make(map[string]string, len(itemsByID))
	for id, item := range itemsByID {
		if item.IconURL != "" {
			urls[id] = item.IconURL
		}
	}
	return urls
}

// IconPath returns the API-relative path a client uses to fetch an item's icon,
// and whether the item has one.
//
// The id is escaped as a single path segment, so its slashes become %2F —
// "Metadata/Items/Currency/CurrencyRerollRare" is one route parameter, not four
// path segments. The icon handler reverses this with url.PathUnescape.
//
// A false result is not an error: an item with no icon renders without one, and
// the client must not build a URL that would 404 on every render.
func IconPath(id string) (string, bool) {
	item, ok := LookupItem(id)
	if !ok || item.IconURL == "" {
		return "", false
	}
	return iconPathPrefix + url.PathEscape(id), true
}

// UnknownItems records feed item ids the asset does not cover, logging each one
// once.
//
// Once per id, not once per occurrence: an unknown id appears in every leg of
// every play that touches it, on every recompute, so an unconditional log line
// would bury the signal it is meant to raise. The set is bounded by the number
// of distinct ids the feed carries.
//
// The zero value is ready to use, and it is safe for concurrent use — the
// handler decorates responses from multiple requests at once.
type UnknownItems struct {
	seen sync.Map
}

// Note logs id as unknown the first time it is seen and does nothing after.
func (u *UnknownItems) Note(id string) {
	if _, loaded := u.seen.LoadOrStore(id, struct{}{}); loaded {
		return
	}
	slog.Warn("currency-exchange: unknown item id", "id", id)
}
