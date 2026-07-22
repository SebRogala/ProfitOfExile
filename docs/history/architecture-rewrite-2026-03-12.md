# ProfitOfExile — Architecture

> **Status: Historical architecture baseline (2026-03-12), not current system
> documentation.** The collector, Mercure pipeline, routes, and deployment have
> materially evolved. Use [the documentation index](../README.md) for current
> documentation and accepted ADRs.

> Decided 2026-03-12. This document captures architectural decisions for the Go rewrite.
> For historical domain concepts and future scope, see
> [the product vision](../product-vision.md).

---

## 1. High-Level Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        Browser                                  │
│                  profitofexile.localhost                         │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────────────┐
│                      Traefik (shared infra)                      │
│               TLS termination · *.localhost routing               │
└──────────┬───────────────────────────────────┬──────────────────┘
           │                                   │
     ┌─────▼─────┐                      ┌──────▼──────┐
     │ SvelteKit │  HMR (dev)           │   Mercure   │
     │ Vite Dev  │  websocket           │   SSE Hub   │
     │  :5173    │                      │             │
     └─────┬─────┘                      └──────▲──────┘
           │ /api/* proxy                      │ publish
     ┌─────▼──────────────────────────────────────────┐
     │                Go Server (:8080)                │
     │                                                 │
     │  ┌─────────┐  ┌─────────┐  ┌────────────────┐  │
     │  │  price   │  │   lab   │  │  simulation    │  │
     │  │  module  │  │  module │  │  module (future)│ │
     │  └────┬─────┘  └────┬────┘  └────────────────┘  │
     │       │              │                           │
     │  ┌────▼──────────────▼───────────────────────┐  │
     │  │         Background Scheduler              │  │
     │  │  15-min tick: fetch → snapshot → analyze   │  │
     │  │  + manual /api/refresh trigger             │  │
     │  └───────────────────────────────────────────┘  │
     └──────────────────────┬──────────────────────────┘
                            │
                     ┌──────▼──────┐
                     │  PostgreSQL │
                     │  (shared    │
                     │   infra)    │
                     └─────────────┘
```

### Production

```
┌────────────────────────────────────────────┐
│              Coolify (VPS)                  │
│                                            │
│  ┌──────────────┐  ┌──────────┐            │
│  │ Go Binary    │  │ Postgres │            │
│  │ (embeds      │  │          │            │
│  │  static      │  └──────────┘            │
│  │  SvelteKit)  │                          │
│  └──────────────┘  ┌──────────┐            │
│                    │ Mercure  │            │
│                    └──────────┘            │
│  Traefik (Coolify-managed)                 │
└────────────────────────────────────────────┘
```

In production, the Go binary serves everything: API + embedded static frontend.
No Node runtime needed. Coolify injects `DATABASE_URL` and manages Traefik.

---

## 2. Architecture Pattern: Hexagonal + CQRS + UseCase

Ported from [cresco](https://github.com/SebRogala/cresco) (PHP/Symfony), adapted for Go idioms.

### Principles

- **Vertical modules** — each feature area is a self-contained module
- **Ports & Adapters** — domain defines interfaces (ports), infrastructure implements them
- **CQRS** — write path (UseCase) and read path (Query/Handler) are separated
- **UseCase per operation** — one struct per write operation with `Execute(req) (resp, error)`
- **No ORM** — direct SQL via pgx
- **No event bus** — simple observer pattern for cross-module hooks (e.g., price update triggers lab recomputation)

### Module Structure

```
internal/
├── price/                          # Epic: Multi-Source Price Engine
│   ├── domain/
│   │   ├── pricebook.go            # PriceBook, SourcePrice, BestDeal types
│   │   ├── datasource.go           # Datasource interface (port)
│   │   └── repository.go           # PriceRepository interface (port)
│   ├── application/
│   │   ├── fetch_prices.go         # UseCase: fetch from sources, normalize, cache
│   │   ├── refresh_prices.go       # UseCase: force refresh
│   │   └── get_prices.go           # Query handler: read cached price book
│   └── infrastructure/
│       ├── ninja_client.go         # poe.ninja HTTP adapter
│       ├── tft_client.go           # TFT GitHub JSON adapter
│       └── postgres_repo.go        # PriceRepository implementation
│
├── lab/                            # Epic: Lab Farming Dashboard + Strategies
│   ├── domain/
│   │   ├── analysis.go             # FontAnalysis, TransfigureAnalysis types
│   │   ├── gem.go                  # Gem, GemColor, GemPool types
│   │   └── repository.go           # LabRepository interface (port)
│   ├── application/
│   │   ├── analyze_font.go         # UseCase: compute font EV per color
│   │   ├── analyze_transfigure.go  # UseCase: compute transfigure ROI
│   │   ├── get_analysis.go         # Query: cached lab analysis results
│   │   └── get_trends.go           # Query: price trends, saturation, movers
│   └── infrastructure/
│       ├── gem_color_resolver.go   # Icon base64 parsing + fallback maps
│       └── postgres_repo.go
│
├── simulation/                     # Epic: Simulation Engine (future)
│   ├── domain/
│   │   ├── strategy.go             # Strategy interface
│   │   ├── inventory.go            # Inventory + SetConverter
│   │   ├── runner.go               # Recursive tree executor
│   │   └── result.go               # Simulation results
│   ├── application/
│   │   └── run_simulation.go       # UseCase: execute strategy tree
│   └── infrastructure/
│       └── postgres_repo.go
│
└── server/                         # HTTP layer (thin)
    ├── server.go                   # chi router, middleware, startup
    ├── handlers/
    │   ├── health.go               # GET /api/health
    │   ├── prices.go               # GET /api/prices, GET /api/refresh
    │   └── lab.go                  # GET /api/lab/font, GET /api/lab/transfigure
    └── scheduler/
        └── scheduler.go            # Background tick (15-min) + hook registry
```

### Request Flow

```
Write path (UseCase):

  HTTP Request
    → Handler (parse request, validate)
      → UseCase.Execute(Request)
        → Load from Domain Port (repository interface)
        → Apply domain logic
        → Save via Domain Port
        → Return Response
      → Handler (serialize response)
  HTTP Response

Read path (Query):

  HTTP Request
    → Handler (parse query params)
      → QueryHandler.Handle(Query)
        → Read directly from repository (skip domain model)
        → Return Result DTO
      → Handler (serialize result)
  HTTP Response
```

### Cross-Module Communication

```
Price module fetches new data
  → Scheduler calls registered hooks
    → Lab module recomputes font EV + transfigure ROI
    → Lab module recomputes trends
  → Scheduler publishes "prices-updated" via Mercure
  → Frontend auto-refreshes
```

No event bus or message queue. Hooks are simple function registrations:

```go
scheduler.OnPriceUpdate(func(book *price.PriceBook) {
    labService.RecomputeAnalysis(book)
    labService.RecomputeTrends(book)
})
```

---

## 3. Tech Stack

| Layer | Technology | Notes |
|-------|-----------|-------|
| HTTP Router | chi | Lightweight, idiomatic, middleware support |
| Database | PostgreSQL 16 + pgx | No ORM, direct queries |
| Migrations | golang-migrate | Timestamp-based SQL files in `db/migrations/` |
| Frontend | SvelteKit + Tailwind CSS | adapter-static, embedded in Go binary for prod |
| Dev Reload | air (Go), Vite HMR (SvelteKit) | Both run inside Docker |
| Real-time | Mercure | SSE hub, shared infra |
| Deployment | Coolify | Dockerfile is the contract |

---

## 4. Development Environment

```
make up    → docker compose up -d
             ├── Go container (air: watches .go files, auto-rebuilds)
             ├── Node container (vite dev: HMR for SvelteKit)
             └── joins shared infra network (postgres, redis, traefik, mercure)

Accessible at: https://profitofexile.localhost (via Traefik)
```

No local Go or Node installation needed. Everything runs in containers.

### Makefile Targets

| Target | Description |
|--------|-------------|
| `make up` | Start dev environment (docker compose up -d) |
| `make down` | Stop dev environment |
| `make build` | Build Go binary |
| `make test` | Run Go tests |
| `make migrate` | Apply database migrations |
| `make migrate-down` | Rollback last migration |

---

## 5. Data Flow: Price Snapshot Pipeline

```
Every 15 minutes (or GET /api/refresh):

  ┌──────────┐     ┌───────────┐     ┌──────────────┐
  │ poe.ninja │────▶│  Fetch &  │────▶│    Store     │
  │   API     │     │ Normalize │     │  Snapshot    │
  └──────────┘     └───────────┘     │  (history)   │
                                      └──────┬───────┘
  ┌──────────┐                               │
  │   TFT    │────▶ (same pipeline)          ▼
  │  GitHub  │                        ┌──────────────┐
  └──────────┘                        │  Recompute   │
                                      │  Lab Analysis│
                                      │  + Trends    │
                                      └──────┬───────┘
                                             │
                                             ▼
                                      ┌──────────────┐
                                      │   Publish    │
                                      │  via Mercure │
                                      └──────┬───────┘
                                             │
                                             ▼
                                      ┌──────────────┐
                                      │   Browser    │
                                      │  auto-refresh│
                                      └──────────────┘
```

---

## 6. Key Differences from Cresco

| Aspect | Cresco (PHP) | ProfitOfExile (Go) |
|--------|-------------|-------------------|
| Multi-tenancy | Doctrine SQL filter + TenantContext | Not needed (single tenant) |
| Event dispatch | Symfony EventDispatcher + #[AsEventListener] | Simple hook registration |
| Transaction | TransactionPort wrapping | pgx transactions directly |
| Base classes | AggregateRoot, DoctrineRepository inheritance | Composition, embedded structs |
| DI Container | Symfony autowiring | Manual construction in main.go |
| ORM | Doctrine | None (pgx direct SQL) |
| Immutability | readonly classes | Unexported fields + value receivers |
