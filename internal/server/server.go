package server

import (
	"io/fs"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
	"profitofexile/internal/server/handlers"
	"profitofexile/internal/trade"
)

// RouterConfig holds optional configuration for the server router.
type RouterConfig struct {
	// MercureURL is the Mercure hub URL for the debug trigger endpoint.
	MercureURL string
	// MercureSecret is the JWT signing secret for Mercure publish.
	MercureSecret string
	// DevMode enables dev-only endpoints like /debug/trigger.
	DevMode bool
	// Pool is the database connection pool for data query endpoints.
	Pool *pgxpool.Pool
	// LabRepo is the analysis repository for lab endpoints.
	LabRepo *lab.Repository
	// LabCache is the in-memory cache for pre-computed analysis results.
	// May be nil — handlers fall back to DB queries when cache is unavailable.
	LabCache *lab.Cache
	// MercureSubscriberKey is the JWT secret for generating frontend subscriber tokens.
	MercureSubscriberKey string
	// MercurePublicURL is the public Mercure hub URL for browser SSE connections.
	MercurePublicURL string
	// TradeGate is the priority gate for trade API lookups. May be nil if trade is disabled.
	TradeGate *trade.Gate
	// TradeCache is the LRU cache for trade lookup results. May be nil if trade is disabled.
	TradeCache *trade.TradeCache
	// TradeSyncTimeout is the max time the handler blocks waiting for a fast-path response.
	TradeSyncTimeout time.Duration
	// League is the current PoE league name (e.g. "Mirage").
	League string
}

// NewRouter creates a chi router with middleware and mounted routes.
// The pinger must not be nil. The frontendFS parameter provides the embedded
// SvelteKit build output; if nil, no static file serving is configured.
func NewRouter(pinger handlers.Pinger, frontendFS fs.FS, cfg RouterConfig) http.Handler {
	r := chi.NewRouter()

	r.Use(middleware.RequestID)
	r.Use(middleware.Logger)
	r.Use(handlers.SlogRecoverer)

	r.Get("/api/health", handlers.Health(pinger))

	if cfg.Pool != nil {
		r.Get("/api/snapshots/gems", handlers.GemSnapshots(cfg.Pool))
		r.Get("/api/snapshots/currency", handlers.CurrencySnapshots(cfg.Pool))
		r.Get("/api/snapshots/fragments", handlers.FragmentSnapshots(cfg.Pool))
		r.Get("/api/snapshots/stats", handlers.SnapshotStats(cfg.Pool))
	}

	if cfg.LabRepo != nil {
		r.Get("/api/analysis/transfigure", handlers.TransfigureAnalysis(cfg.LabRepo, cfg.LabCache))
		r.Get("/api/analysis/font", handlers.FontAnalysis(cfg.LabRepo, cfg.LabCache))
		r.Get("/api/analysis/quality", handlers.QualityAnalysis(cfg.LabRepo, cfg.LabCache))
		r.Get("/api/analysis/trends", handlers.TrendAnalysis(cfg.LabRepo, cfg.LabCache))
		r.Get("/api/analysis/collective", handlers.CollectiveAnalysis(cfg.LabRepo, cfg.LabCache))
		r.Get("/api/analysis/compare", handlers.CompareAnalysis(cfg.LabRepo, cfg.LabCache))
		r.Get("/api/analysis/gems/names", handlers.GemNamesAutocomplete(cfg.LabRepo, cfg.LabCache))
		r.Get("/api/analysis/status", handlers.AnalysisStatus(cfg.LabCache, cfg.Pool, cfg.League))
		r.Get("/api/analysis/history", handlers.SignalHistory(cfg.LabRepo))
	}

	if cfg.TradeGate != nil {
		r.Post("/api/trade/lookup", handlers.TradeLookup(cfg.TradeGate, cfg.TradeCache, cfg.TradeSyncTimeout))
	}

	r.Get("/api/mercure/token", handlers.MercureToken(cfg.MercureSubscriberKey, cfg.MercurePublicURL))

	if cfg.DevMode {
		r.Post("/debug/trigger", handlers.DebugTrigger(cfg.MercureURL, cfg.MercureSecret))
	}

	// Serve static frontend files with SPA fallback. The wildcard pattern never
	// shadows explicit API routes because chi's radix tree prefers exact matches.
	if frontendFS != nil {
		r.Handle("/*", StaticHandler(frontendFS))
	}

	return r
}
