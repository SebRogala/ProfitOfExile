package server

import (
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/server/handlers"
)

// NewRouter creates a chi router with middleware and mounted routes.
// The pool parameter may be nil in tests that don't require database access.
func NewRouter(pool *pgxpool.Pool) http.Handler {
	r := chi.NewRouter()

	r.Use(middleware.RequestID)
	r.Use(middleware.Logger)
	r.Use(handlers.SlogRecoverer)

	r.Get("/api/health", handlers.Health(pool))

	return r
}
