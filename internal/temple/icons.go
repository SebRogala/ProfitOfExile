package temple

import "net/url"

// ICONS
//
// Temple artwork is served through the typed icon cache route —
// GET /api/icon/temple/{name} — over internal/icons. The extension point is the
// category file at internal/icons/urls/temple.json, generated from the live feed
// by scripts/generate-temple-icons.py (see docs/ICONS.md).
//
// Production cannot fetch upstream, so the icon volume must be pre-seeded BEFORE
// the deploy that carries the map, or every temple icon is a permanent 502
// (ADR-012).

// iconRoute is the prefix of the path a client fetches an icon from. It matches
// the typed route registered in internal/server and the paths the web and
// desktop clients build in their shared icons helpers.
const iconRoute = "/api/icon/temple/"

// RoomIconName is the icon-map key every room-tier's icon path is built from.
//
// All 86 room lines carry the same poe.ninja artwork — the temple map image —
// because a room-tier is a line on one Chronicle of Atzoatl, not an item with
// art of its own. One key, one cached file, 86 rooms pointing at it.
//
// scripts/generate-temple-icons.py writes this key and asserts the feed really
// does serve one shared artwork across the category.
const RoomIconName = "Chronicle of Atzoatl"

// IconPath returns the API-relative path a client fetches name's artwork from.
//
// The name is escaped as a single path segment, the same way the web and desktop
// clients escape a name (encodeURIComponent in their icons helpers); the
// icon handler reverses it with url.PathUnescape before the map lookup.
//
// It does not consult the map. Membership is a build-time property here — the
// served set and the committed map are pinned to each other by
// TestIconMap_coversTheServedSetAndTheRoomArtwork — so a runtime lookup would
// only add a second, weaker place for the two to disagree, and a name that
// somehow escaped the test is better served as a path that 404s (the clients
// render their "?" fallback) than as an empty string the client cannot tell
// from "no artwork exists".
func IconPath(name string) string {
	return iconRoute + url.PathEscape(name)
}
