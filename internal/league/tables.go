package league

// ScopedTables returns the league-scoped tables in rollover wipe order (ADR-011).
// Each call returns a fresh slice so callers cannot change the registry.
func ScopedTables() []string {
	return []string{
		"gem_snapshots",
		"currency_snapshots",
		"fragment_snapshots",
		"font_snapshots",
		"transfigure_results",
		"quality_results",
		"trend_results",
		"gem_features",
		"gem_signals",
		"dedication_snapshots",
		"trade_lookups",
		"market_context",
		"currency_exchange_markets",
		"currency_exchange_cursor",
		"double_corrupt_snapshots",
		"item_snapshots",
	}
}
