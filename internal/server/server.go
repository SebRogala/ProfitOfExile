package server

import (
	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"

	"profitofexile/internal/server/handlers"
)

// NewRouter creates a chi router with middleware and mounted routes.
func NewRouter() *chi.Mux {
	r := chi.NewRouter()

	r.Use(middleware.RequestID)
	r.Use(middleware.Logger)
	r.Use(middleware.Recoverer)

	r.Get("/api/health", handlers.Health())

	return r
}
