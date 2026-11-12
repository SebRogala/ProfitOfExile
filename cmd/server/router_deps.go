package main

import (
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/bossloot"
	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/ratelimit"
	"profitofexile/internal/server"
	"profitofexile/internal/server/handlers"
)

// routerDeps holds the router dependencies cmd/server builds from the pool.
// Every one is built unconditionally: NewRouter registers a route family only
// when its dependency is non-nil.
type routerDeps struct {
	BossLoot        *handlers.BossLoot
	LabRunRepo      *lab.LabRunRepository
	FontSessionRepo *lab.FontSessionRepository
}

func newRouterDeps(pool *pgxpool.Pool, scope league.Scope) routerDeps {
	// Boss loot has its own device budget, separate from the merc limiter.
	bossLootLimiter := ratelimit.NewRateLimiter(
		handlers.DefaultBossLootWriteBudget,
		handlers.DefaultBossLootWriteWindow,
		handlers.DefaultBossLootRateLimitDevices,
	)
	return routerDeps{
		BossLoot:        handlers.NewBossLoot(bossloot.NewRepository(pool), scope, bossLootLimiter),
		LabRunRepo:      lab.NewLabRunRepository(pool),
		FontSessionRepo: lab.NewFontSessionRepository(pool),
	}
}

func (d routerDeps) applyTo(cfg *server.RouterConfig) {
	cfg.BossLoot = d.BossLoot
	cfg.LabRunRepo = d.LabRunRepo
	cfg.FontSessionRepo = d.FontSessionRepo
}
