package mercure

// Mercure topics published by the server, collector and tools. The
// subscriber-token claim list and every publisher read them from here.
const (
	LeagueActiveTopic    = "poe/league/active"
	AnalysisUpdatedTopic = "poe/analysis/updated"
	TradeResultsTopic    = "poe/trade/results"
	LabLayoutTopic       = "poe/lab/layout"
	DesktopTopicPrefix   = "poe/desktop/"
	DesktopTopicPattern  = DesktopTopicPrefix + "{pair}"
	CollectorTopicPrefix = "poe/collector/"
	TradeTickTopic       = CollectorTopicPrefix + "trade-tick"
	AdminRecomputeTopic  = "poe/admin/recompute"
)
