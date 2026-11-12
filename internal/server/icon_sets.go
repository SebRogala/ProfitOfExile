package server

import (
	"log/slog"

	"profitofexile/internal/exchange"
	"profitofexile/internal/icons"
)

// DefaultIconCacheDir is the icon cache root BuildIconSets uses when it is
// given an empty root. In production it is a persistent volume (ICON_CACHE_DIR in
// cmd/server); an ephemeral one there is not a slow first request but a
// permanent 502, because poewiki 403s the VPS and the cache can only be
// pre-seeded — see docs/ICONS.md and ADR-012.
const DefaultIconCacheDir = "./data/icons-cache"

// BuildIconSets builds every icon set under one cache root. Each embedded source
// map is one typed cache under that root. A malformed category is allowed to
// degrade independently; NewSets returns the valid partial registry and logs the
// construction error, so the typed route can still serve its peers.
func BuildIconSets(root string) *icons.Sets {
	if root == "" {
		root = DefaultIconCacheDir
	}

	iconSets, iconSetsErr := icons.NewSets(root)
	if iconSetsErr != nil {
		slog.Error("icon set initialization degraded", "root", root, "error", iconSetsErr)
	}
	if iconSets != nil {
		if addErr := iconSets.Add("currency-exchange", exchange.IconURLs(), root); addErr != nil {
			slog.Error("currency-exchange icon set initialization degraded", "root", root, "error", addErr)
		}
	}
	return iconSets
}
