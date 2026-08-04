package server

import (
	"io/fs"
	"log/slog"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/go-chi/cors"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/device"
	"profitofexile/internal/gemicon"
	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/mercure"
	"profitofexile/internal/server/handlers"
	devmw "profitofexile/internal/server/middleware"
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
	// TradeRepo is the trade lookup persistence repository. May be nil if trade is disabled.
	TradeRepo *trade.Repository
	// TradeSyncTimeout is the max time the handler blocks waiting for a fast-path response.
	TradeSyncTimeout time.Duration
	// League is the resolved scope for the process-active league. Fixed at boot;
	// every scoped handler reads and writes under it (per-request selection is POE-121).
	League league.Scope
	// Analyzer is the lab analysis engine for admin recalculation endpoint.
	Analyzer *lab.Analyzer
	// LayoutRepo is the repository for daily lab layout data.
	LayoutRepo *lab.LayoutRepository
	// AllowedOrigins for CORS (desktop app needs cross-origin access).
	// Example: ["http://localhost:1420", "tauri://localhost"]
	AllowedOrigins []string
	// DeviceRepo is the device repository for fingerprint-based identity.
	// May be nil — device middleware is skipped when nil.
	DeviceRepo *device.Repository
	// FenceChecker reports the liveness of the server's process fence (the held
	// advisory-lock connection). May be nil — health then reflects the DB ping
	// alone. The fence's actual guarantee is boot-time exclusion: a second server
	// cannot acquire ServerLockKey and refuses readiness, so only one writer boots.
	// When set, a mid-life fence loss is DETECTED here (CheckHeld) and degrades
	// /api/health to 503 — but that only sheds HTTP traffic. It does NOT stop the
	// writers, which run off Mercure events and timers (recompute) and GGG trade
	// calls, not off HTTP requests; those keep writing until the process exits.
	// Quiescing behind a peer that grabbed the lock therefore depends on an
	// external liveness probe restarting this process on the sustained 503.
	// TODO(POE-118): cancel the writer lifecycle on fence loss so a demoted server
	// stops writing without waiting for an external restart.
	FenceChecker handlers.LivenessChecker
	// GemIconCacheDir is the persistent directory where fetched gem icons are
	// cached. Empty falls back to gemicon.DefaultCacheDir.
	GemIconCacheDir string
}

// NewRouter creates a chi router with middleware and mounted routes.
// The pinger must not be nil. The frontendFS parameter provides the embedded
// SvelteKit build output; if nil, no static file serving is configured.
func NewRouter(pinger handlers.Pinger, frontendFS fs.FS, cfg RouterConfig) http.Handler {
	r := chi.NewRouter()

	r.Use(middleware.RequestID)
	r.Use(middleware.Logger)
	r.Use(handlers.SlogRecoverer)

	if len(cfg.AllowedOrigins) > 0 {
		r.Use(cors.Handler(cors.Options{
			AllowedOrigins:   cfg.AllowedOrigins,
			AllowedMethods:   []string{"GET", "POST", "PATCH", "OPTIONS"},
			AllowedHeaders:   []string{"Content-Type", "Authorization", "X-Device-ID", "X-App-Version"},
			AllowCredentials: false,
			MaxAge:           300,
		}))
	}

	if cfg.DeviceRepo != nil {
		r.Use(devmw.DeviceMiddleware(cfg.DeviceRepo))
	}

	r.Get("/api/health", handlers.Health(pinger, cfg.FenceChecker))

	// Gem icons: public, static, no league scope or auth. Serves both clients
	// from a persistent on-disk cache of poewiki images.
	if gemIcons, err := gemicon.New(cfg.GemIconCacheDir); err != nil {
		slog.Error("gem icon cache init failed; /api/gem-icon disabled", "error", err)
	} else {
		r.Get("/api/gem-icon/{name}", gemIcons.Handler())
	}

	// The whole /api/snapshots/* family is gone. /stats went first (POE-150):
	// three unbounded full-relation aggregates, 14.7 s of DB time and 168 MB of
	// temp spill per unauthenticated request, with no consumer. Its /gems,
	// /currency and /fragments siblings followed (POE-157): also unauthenticated
	// raw-hypertable reads whose 24h window was only a default, so
	// ?from=1970-01-01&limit=10000 bypassed it. They existed so an agent could
	// pull prod data without SSH; the real prod->local path is the SSH pipe,
	// which authenticates with a key and can COPY a full dump. Do not re-add an
	// HTTP export — write a CLI around SSH + psql instead. The market overview
	// /stats once backed is served from cache by /api/analysis/market-overview.

	if cfg.LabRepo != nil {
		r.Get("/api/analysis/transfigure", handlers.TransfigureAnalysis(cfg.LabRepo, cfg.LabCache, cfg.League))
		r.Get("/api/analysis/font", handlers.FontAnalysis(cfg.LabRepo, cfg.LabCache, cfg.League))
		r.Get("/api/analysis/dedication", handlers.DedicationAnalysis(cfg.LabRepo, cfg.LabCache, cfg.League))
		r.Get("/api/analysis/quality", handlers.QualityAnalysis(cfg.LabRepo, cfg.LabCache, cfg.League))
		r.Get("/api/analysis/trends", handlers.TrendAnalysis(cfg.LabRepo, cfg.LabCache, cfg.League))
		r.Get("/api/analysis/collective", handlers.CollectiveAnalysis(cfg.LabRepo, cfg.LabCache, cfg.League))
		r.Get("/api/analysis/compare", handlers.CompareAnalysis(cfg.LabRepo, cfg.LabCache, cfg.TradeCache, cfg.League))
		r.Get("/api/analysis/gems/names", handlers.GemNamesAutocomplete(cfg.LabRepo, cfg.LabCache, cfg.League))
		r.Get("/api/analysis/gems/dictionary", handlers.GemDictionary(cfg.LabRepo, cfg.League))
		r.Get("/api/analysis/status", handlers.AnalysisStatus(cfg.LabCache, cfg.Pool, cfg.League))
		r.Get("/api/analysis/history", handlers.SignalHistory(cfg.LabRepo, cfg.League))

		r.Get("/api/analysis/market-overview", handlers.MarketOverview(cfg.LabCache, cfg.Pool, cfg.League))

		// V2 pre-computed analysis endpoints
		r.Get("/api/analysis/market-context", handlers.MarketContextAnalysis(cfg.LabRepo, cfg.LabCache, cfg.League))
		r.Get("/api/analysis/gem-features", handlers.GemFeaturesAnalysis(cfg.LabRepo, cfg.LabCache, cfg.League))
		r.Get("/api/analysis/gem-signals", handlers.GemSignalsAnalysis(cfg.LabRepo, cfg.LabCache, cfg.League))

		// Admin operations (recalculation, etc.) are intentionally NOT exposed
		// over HTTP. Operator triggers run as CLI binaries inside the server
		// container (e.g. `docker exec server /recalculate`), which publish
		// Mercure events the running subscriber consumes.
	}

	if cfg.DeviceRepo != nil {
		r.Post("/api/device/identify", handlers.DeviceIdentify(cfg.DeviceRepo))
	}

	if cfg.TradeGate != nil {
		r.Post("/api/trade/lookup", handlers.TradeLookup(cfg.TradeGate, cfg.TradeCache, cfg.League, cfg.TradeSyncTimeout))
		// Trade refresh ticks arrive over Mercure (poe/collector/trade-tick),
		// not HTTP. See cmd/server subscriber + internal/server/trade_tick.go.
	}

	// Trade submit: available whenever trade cache exists (desktop can submit
	// even when the server-side gate is disabled).
	if cfg.TradeCache != nil {
		r.Post("/api/trade/submit", handlers.TradeSubmit(cfg.TradeCache, cfg.TradeRepo, cfg.League))
	}

	r.Get("/api/mercure/token", handlers.MercureToken(cfg.MercureSubscriberKey, cfg.MercurePublicURL))

	if cfg.MercureURL != "" && cfg.MercureSecret != "" {
		r.Post("/api/desktop/gems", handlers.DesktopGems(cfg.MercureURL, cfg.MercureSecret))
	}

	if cfg.Pool != nil {
		r.Post("/api/desktop/font-session", handlers.FontSession(cfg.Pool))
		r.Post("/api/lab/runs", handlers.StoreLabRun(cfg.Pool))
		r.Get("/api/lab/runs", handlers.ListLabRuns(cfg.Pool))
	}

	if cfg.LayoutRepo != nil {
		var layoutPub mercure.Publisher
		if cfg.MercureURL != "" && cfg.MercureSecret != "" {
			layoutPub = &mercure.HubPublisher{URL: cfg.MercureURL, Secret: cfg.MercureSecret}
		}
		r.Get("/api/lab/layout/{difficulty}", handlers.GetLayout(cfg.LayoutRepo))
		r.Post("/api/lab/layout/{difficulty}", handlers.UploadLayout(cfg.LayoutRepo, layoutPub))
		r.Patch("/api/lab/layout/{difficulty}/room/{roomId}", handlers.PatchRoom(cfg.LayoutRepo, layoutPub))
	}

	if cfg.DevMode {
		r.Post("/debug/trigger", handlers.DebugTrigger(cfg.MercureURL, cfg.MercureSecret, cfg.League))
	}

	// Serve static frontend files with SPA fallback. The wildcard pattern never
	// shadows explicit API routes because chi's radix tree prefers exact matches.
	if frontendFS != nil {
		r.Handle("/*", StaticHandler(frontendFS))
	}

	return r
}
