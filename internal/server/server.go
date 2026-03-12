package server

import (
	"io/fs"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"

	"profitofexile/internal/server/handlers"
)

// NewRouter creates a chi router with middleware and mounted routes.
// The pinger must not be nil; use handlers.NopPinger in tests that don't
// require database access. The frontendFS parameter provides the embedded
// SvelteKit build output; if nil, no static file serving is configured.
func NewRouter(pinger handlers.Pinger, frontendFS fs.FS) http.Handler {
	r := chi.NewRouter()

	r.Use(middleware.RequestID)
	r.Use(middleware.Logger)
	r.Use(handlers.SlogRecoverer)

	r.Get("/api/health", handlers.Health(pinger))

	// Serve static frontend files with SPA fallback. Registered last so
	// API routes take precedence (chi matches more specific routes first).
	if frontendFS != nil {
		r.Handle("/*", StaticHandler(frontendFS))
	}

	return r
}
