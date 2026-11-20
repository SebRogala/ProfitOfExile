package mercure

// Mercure topics published by the server, collector and tools. The
// subscriber-token claim list and the publishers read them from here, except
// exchange.UpdatedTopic and harvest.UpdatedTopic, which stay in their pillar
// packages (internal/exchange, internal/harvest) for now. The server's
// recompute runtime (cmd/server/recompute_runtime.go) subscribes through the
// collector, trade-tick and admin-recompute topics below.
const (
	LeagueActiveTopic    = "poe/league/active"
	AnalysisUpdatedTopic = "poe/analysis/updated"
	TradeResultsTopic    = "poe/trade/results"
	LabLayoutTopic       = "poe/lab/layout"
	DesktopTopicPrefix   = "poe/desktop/"
	DesktopTopicPattern  = DesktopTopicPrefix + "{pair}"
	CollectorTopicPrefix = "poe/collector/"

	CollectorGemsSuffix      = "gems"
	CollectorCurrencySuffix  = "currency"
	CollectorFragmentsSuffix = "fragments"
	CollectorGemsTopic       = CollectorTopicPrefix + CollectorGemsSuffix
	CollectorCurrencyTopic   = CollectorTopicPrefix + CollectorCurrencySuffix
	CollectorFragmentsTopic  = CollectorTopicPrefix + CollectorFragmentsSuffix

	TradeTickTopic      = CollectorTopicPrefix + "trade-tick"
	AdminRecomputeTopic = "poe/admin/recompute"
)
