package main

import (
	"context"
	"embed"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"syscall"
	"time"

	"profitofexile/internal/db"
	"profitofexile/internal/lab"
	"profitofexile/internal/server"
)

//go:embed all:frontend_build
var frontendEmbed embed.FS

func main() {
	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}

	if p, err := strconv.Atoi(port); err != nil || p < 1 || p > 65535 {
		slog.Error("invalid PORT value", "port", port)
		fmt.Fprintf(os.Stderr, "PORT must be a number between 1 and 65535, got %q\n", port)
		os.Exit(1)
	}

	databaseURL := os.Getenv("DATABASE_URL")
	if databaseURL == "" {
		slog.Error("DATABASE_URL is required")
		fmt.Fprintln(os.Stderr, "DATABASE_URL environment variable must be set")
		os.Exit(1)
	}

	ctx, ctxCancel := context.WithCancel(context.Background())
	defer ctxCancel()

	pool, err := db.NewPool(ctx, databaseURL)
	if err != nil {
		slog.Error("failed to connect to database", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to connect to database. Check DATABASE_URL and ensure PostgreSQL is running.")
		os.Exit(1)
	}
	defer pool.Close()
	slog.Info("database connected")

	// Auto-migrate: apply pending migrations before binding the server.
	// Fail-fast on error per ADR-004.
	if err := db.MigrateUp(db.MigrationsFS, databaseURL); err != nil {
		slog.Error("auto-migrate failed", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to apply database migrations. Check migration files and database state.")
		os.Exit(1)
	}

	frontendFS, err := fs.Sub(frontendEmbed, "frontend_build")
	if err != nil {
		slog.Error("failed to load embedded frontend", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to load embedded frontend assets.")
		os.Exit(1)
	}

	mercureURL := os.Getenv("MERCURE_URL")
	mercureSecret := os.Getenv("MERCURE_JWT_SECRET")
	devMode := os.Getenv("APP_ENV") == "dev"

	if mercureURL != "" && mercureSecret == "" {
		slog.Warn("MERCURE_URL is set but MERCURE_JWT_SECRET is empty — publish operations will be skipped")
	}

	labRepo := lab.NewRepository(pool)
	labCache := lab.NewCache()
	throttler := lab.NewThrottler(mercureURL, mercureSecret, 2*time.Second)
	analyzer := lab.NewAnalyzer(labRepo, throttler, labCache)

	router := server.NewRouter(pool, frontendFS, server.RouterConfig{
		MercureURL:    mercureURL,
		MercureSecret: mercureSecret,
		DevMode:       devMode,
		Pool:          pool,
		LabRepo:              labRepo,
		LabCache:             labCache,
		MercureSubscriberKey: os.Getenv("MERCURE_SUBSCRIBER_KEY"),
		MercurePublicURL:     os.Getenv("MERCURE_PUBLIC_URL"),
	})

	// Run initial analyses on startup (uses existing data).
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("transfigure analysis panicked on startup", "recover", r)
			}
		}()
		if err := analyzer.RunTransfigure(ctx); err != nil {
			slog.Warn("startup transfigure analysis failed (non-fatal)", "error", err)
		}
	}()
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("font analysis panicked on startup", "recover", r)
			}
		}()
		if err := analyzer.RunFont(ctx); err != nil {
			slog.Warn("startup font analysis failed (non-fatal)", "error", err)
		}
	}()
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("quality analysis panicked on startup", "recover", r)
			}
		}()
		if err := analyzer.RunQuality(ctx); err != nil {
			slog.Warn("startup quality analysis failed (non-fatal)", "error", err)
		}
	}()
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("trend analysis panicked on startup", "recover", r)
			}
		}()
		if err := analyzer.RunTrends(ctx); err != nil {
			slog.Warn("startup trend analysis failed (non-fatal)", "error", err)
		}
	}()

	// Start Mercure subscriber in background if configured.
	if mercureURL != "" {
		subCtx, subCancel := context.WithCancel(ctx)
		defer subCancel()

		topics := []string{"poe/collector/gems", "poe/collector/currency", "poe/collector/fragments"}
		mercureSubKey := os.Getenv("MERCURE_SUBSCRIBER_KEY")
		sub := server.NewMercureSubscriber(mercureURL, topics, mercureSubKey, func(ev server.MercureEvent) {
			var payload map[string]any
			if err := json.Unmarshal([]byte(ev.Data), &payload); err != nil {
				slog.Warn("mercure: invalid event payload", "error", err)
				return
			}
			slog.Info("mercure event received",
				"topic", ev.Topic,
				"endpoint", payload["endpoint"],
				"inserted", payload["inserted"],
			)

			// Trigger analysis on new gem data.
			endpoint, ok := payload["endpoint"].(string)
			if !ok {
				slog.Warn("mercure: missing or non-string 'endpoint' in payload", "payload", payload)
				return
			}
			if endpoint == "ninja_gems" || endpoint == "ninja-gems" {
				go func() {
					defer func() {
						if r := recover(); r != nil {
							slog.Error("transfigure analysis panicked", "recover", r)
						}
					}()
					if err := analyzer.RunTransfigure(subCtx); err != nil {
						slog.Warn("transfigure analysis failed", "error", err)
					}
				}()
				go func() {
					defer func() {
						if r := recover(); r != nil {
							slog.Error("font analysis panicked", "recover", r)
						}
					}()
					if err := analyzer.RunFont(subCtx); err != nil {
						slog.Warn("font analysis failed", "error", err)
					}
				}()
				go func() {
					defer func() {
						if r := recover(); r != nil {
							slog.Error("quality analysis panicked", "recover", r)
						}
					}()
					if err := analyzer.RunQuality(subCtx); err != nil {
						slog.Warn("quality analysis failed", "error", err)
					}
				}()
				go func() {
					defer func() {
						if r := recover(); r != nil {
							slog.Error("trend analysis panicked", "recover", r)
						}
					}()
					if err := analyzer.RunTrends(subCtx); err != nil {
						slog.Warn("trend analysis failed", "error", err)
					}
				}()
			}
		})
		go sub.Run(subCtx)
		slog.Info("mercure subscriber started", "topics", topics)
	}

	srv := &http.Server{
		Addr:         ":" + port,
		Handler:      router,
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 30 * time.Second,
		IdleTimeout:  120 * time.Second,
	}

	// Start server in a goroutine, sending errors back to the main goroutine
	// so deferred cleanup can run before exit.
	errCh := make(chan error, 1)
	go func() {
		slog.Info("server starting", "addr", srv.Addr)
		errCh <- srv.ListenAndServe()
	}()

	// Wait for SIGINT/SIGTERM or a server startup error.
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)

	select {
	case err := <-errCh:
		if !errors.Is(err, http.ErrServerClosed) {
			slog.Error("server failed to start", "error", err)
			os.Exit(1)
		}
	case sig := <-quit:
		slog.Info("shutting down server", "signal", sig.String())
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := srv.Shutdown(ctx); err != nil {
		slog.Error("server forced to shutdown", "error", err)
		os.Exit(1)
	}

	slog.Info("server stopped")
}
