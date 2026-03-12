package server

import (
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"

	"profitofexile/internal/server/handlers"
)

// NewRouter creates a chi router with middleware and mounted routes.
// The pinger must not be nil; use handlers.NopPinger in tests that don't
// require database access.
func NewRouter(pinger handlers.Pinger) http.Handler {
	r := chi.NewRouter()

	r.Use(middleware.RequestID)
	r.Use(middleware.Logger)
	r.Use(handlers.SlogRecoverer)

	r.Get("/api/health", handlers.Health(pinger))

	return r
}
