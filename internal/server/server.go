package server

import (
	"io/fs"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/server/handlers"
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
