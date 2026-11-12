package main

import (
	"context"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/bossloot"
	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/mercenary"
	"profitofexile/internal/mercure"
	"profitofexile/internal/ratelimit"
	"profitofexile/internal/server"
	"profitofexile/internal/server/handlers"
	"profitofexile/internal/trade"
)

// routerDeps holds the router dependencies cmd/server builds from the pool.
// Every one is built unconditionally: NewRouter registers a route family only
// when its dependency is non-nil.
type routerDeps struct {
	BossLoot         *handlers.BossLoot
	LabRunRepo       *lab.LabRunRepository
	FontSessionRepo  *lab.FontSessionRepository
	MercTemplateRepo *mercenary.Repository
	MercLimiter      *ratelimit.RateLimiter
	MercCorpus       *handlers.MercCorpusCache
	LayoutPublisher  mercure.Publisher
	KnownGem         func(context.Context, league.Scope, string) (bool, error)
}

func newRouterDeps(pool *pgxpool.Pool, scope league.Scope, mercureURL, mercureSecret string, tradeRepo *trade.Repository) routerDeps {
	// Boss loot has its own device budget, separate from the merc limiter.
	bossLootLimiter := ratelimit.NewRateLimiter(
		handlers.DefaultBossLootWriteBudget,
		handlers.DefaultBossLootWriteWindow,
		handlers.DefaultBossLootRateLimitDevices,
	)
	return routerDeps{
		BossLoot:         handlers.NewBossLoot(bossloot.NewRepository(pool), scope, bossLootLimiter),
		LabRunRepo:       lab.NewLabRunRepository(pool),
		FontSessionRepo:  lab.NewFontSessionRepository(pool),
		MercTemplateRepo: mercenary.NewRepository(pool),
		// One limiter for the whole process, shared by both merc writes: a
		// device that has spent its budget spamming uploads must not get free
		// tombstones with what is left.
		MercLimiter: ratelimit.NewRateLimiter(
			mercenary.DefaultUploadBudget,
			mercenary.DefaultUploadWindow,
			mercenary.DefaultRateLimitDevices,
		),
		// One cache shared by all three merc routes: the writes are what
		// invalidate the read, so they have to be looking at the same entry.
		MercCorpus:      handlers.NewMercCorpusCache(handlers.DefaultMercCorpusTTL),
		LayoutPublisher: layoutPublisher(mercureURL, mercureSecret),
		KnownGem:        knownGemFunc(tradeRepo),
	}
}

// layoutPublisher returns an untyped nil unless the hub is configured: a nil
// *HubPublisher stored in the interface would read as non-nil to the layout
// handlers.
func layoutPublisher(mercureURL, mercureSecret string) mercure.Publisher {
	if mercureURL != "" && mercureSecret != "" {
		return &mercure.HubPublisher{URL: mercureURL, Secret: mercureSecret}
	}
	return nil
}

// knownGemFunc is the trade-name membership check shared by the trade routes
// and the trade-tick handler; nil when trade persistence is absent.
func knownGemFunc(repo *trade.Repository) func(context.Context, league.Scope, string) (bool, error) {
	if repo == nil {
		return nil
	}
	return repo.KnownGem
}

func (d routerDeps) applyTo(cfg *server.RouterConfig) {
	cfg.BossLoot = d.BossLoot
	cfg.LabRunRepo = d.LabRunRepo
	cfg.FontSessionRepo = d.FontSessionRepo
	cfg.MercTemplateRepo = d.MercTemplateRepo
	cfg.MercLimiter = d.MercLimiter
	cfg.MercCorpus = d.MercCorpus
	cfg.LayoutPublisher = d.LayoutPublisher
	cfg.KnownGem = d.KnownGem
}
