# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ProfitOfExile is a **Path of Exile 1 profit simulation platform** being rewritten from PHP/Symfony + Vue 3 to **Go + SvelteKit**. It models farming strategies as composable trees, fetches live prices from multiple market sources, simulates inventory flows with automatic set conversions, and calculates profitability per strategy.

The original PHP codebase exists only in git history (commit `537e37e` and earlier) as architectural reference — do not restore or modify it. The current repo contains `BACKBONE.md` as the authoritative design reference. Epics and tasks are tracked in YouTrack (project POE).

## Early Prototypes (`/var/www/poe`)

Working Node.js scripts and strategy notes live in a separate workspace at `/var/www/poe`. These are the first real implementations and serve as reference for porting into the Go platform:

- `scripts/font-analysis.mjs` — Font of Divine Skill profitability analysis (the first probabilistic strategy). Fetches poe.ninja SkillGem data, classifies transfigured gems by color (RED/GREEN/BLUE), calculates win probability via hypergeometric formula (`pWin3Picks`), computes EV per color. Supports `--variant`, `--compare`, `--threshold`, `--min-listings`.
- `scripts/lab-transfigure-analysis.mjs` — Merciless Lab "change gem to transfigured" ROI analysis. Compares base gem cost vs transfigured value.
- `scripts/fetch-gem-colors.mjs` — Builds `data/gem-colors.json` from RePoE + poe.ninja. Resolves transfigured gem colors by progressively stripping " of X" suffixes.
- `scripts/lab-quality-analysis.mjs`, `scripts/lab-loadout.mjs`, `scripts/gem-exp-check.mjs`, `scripts/inspect-gems.mjs` — Additional lab/gem helper scripts.
- `data/gem-colors.json` — Gem color map (RePoE-derived + manual overrides).
- `poe1-knowledge.md` — PoE1 knowledge base covering patches 3.25–3.28 (current league: 3.28 Mirage).
- `strategies/3.28-mirage/` — Current league strategy notes (lab farming, build notes).

## Architecture (Target)

### Backend: Go
- Standard library HTTP server (Go 1.22+ routing) or chi router
- PostgreSQL via `pgx` (no ORM — direct SQL queries)
- Static file serving via Go `embed` package (bundles compiled frontend)
- Target structure: `cmd/server/`, `internal/` packages

### Frontend: SvelteKit
- SvelteKit with `adapter-static` (no Node runtime in production)
- Tailwind CSS v4 (CSS-first config — `@theme` in `app.css`, no `tailwind.config.js`)
- Svelte 5 with runes (`$props()`, `{@render children()}`)
- Built to static files, served by Go backend

### Database: PostgreSQL + TimescaleDB
- Application tables: `strategies`, `profiles`, `conversion_rules`
- TimescaleDB hypertables for time-series price data (gems, currency, fragments, cards, uniques)
- JSONB columns for strategy trees

### Deployment: Docker on Coolify
- Multi-stage Dockerfile: Node build (SvelteKit) → Go build → minimal runtime image
- Separate container for price collector service (24/7 data collection)

## Dev Environment

- `make up` → `docker compose up -d` — Go (air) + SvelteKit (Vite) both hot-reload in Docker
- Single domain: `profitofexile.localhost` — Traefik routes `/api` → Go, everything else → Vite
- No local Go/Node tooling needed — everything runs in containers, use `docker compose exec` for CLI
- Infra stack at `/var/www/infra`: Traefik, Postgres, Redis, Mailpit (shared `infra` network)
- Global HTTP→HTTPS redirect configured on Traefik entrypoint level

## Gotchas

- `golang-migrate` uses `lib/pq` (not `pgx`) — requires explicit `sslmode=disable` for local Postgres. Handled in `internal/db/migrate.go` but watch for it in test helpers too.
- `go:embed` requires `frontend/build/.gitkeep` so the embed directive works without running a Node build. Dev mode serves no frontend from Go — that's Vite's job via Traefik.
- Frontend `node_modules` uses a named Docker volume to prevent host bind mount from overwriting installed deps.
- **Migrations**: NEVER modify a deployed migration file — always create a new one. Use `date +%Y%m%d%H%M%S` for the timestamp (not manual `YYYYMMDD100000`). Run: `make migration name=add_foo_column` to generate both up/down files.
- **Desktop overlay positioning**: Never use `WebviewWindow` constructor `x`/`y` for overlay placement — Tauri's DPI conversion breaks on multi-monitor. Use Rust-side `move_overlay` command (`PhysicalPosition`) instead. Save `outerPosition()` directly, restore via `invoke('move_overlay')`. See `.claude/agents/desktop.md` for full details.

## Core Domain Concepts

**Strategy Tree**: Composable tree of farming activities. Each strategy consumes items, produces items with probability, and takes time. Parent nodes run children N times (`series` count) before executing themselves. A `Wrapper` is a no-op grouping node.

**Inventory-Driven Cascade**: All strategies share a single Inventory. On every `Add()`, the SetConverter checks conversion rules (e.g., 4 fragments → 1 set). When a strategy needs an item not in inventory, auto-buy triggers from the cheapest source.

**Multi-Source Pricing**: Prices fetched from poe.ninja (individual trade) and TFT (bulk trade). Each item has per-source buy/sell prices. The system computes optimal buy source (lowest) and sell source (highest).

**Breakpoint Analysis**: Simulates at each tree depth to determine where in a strategy chain it's most profitable to stop and sell intermediates vs. continue.

## Key Design Documents

- `BACKBONE.md` — Authoritative design reference. Full architecture, data source specs, domain model, API contracts, and feature roadmap. **When in doubt, refer to BACKBONE.**
- **YouTrack (project POE)** — Epics, tasks, and feature specs. **All design specs and task descriptions live in the tracker, not in files.** Do NOT create `docs/specs/`, `docs/designs/`, or similar spec files in the repo. When brainstorming a feature, write the spec directly into the YouTrack task description. Implementation plans live in MCP — use the `pf-local` MCP server's `savePlan` tool to persist them.

## Data Sources

- **poe.ninja**: REST API at `poe.ninja/poe1/api/economy/stash/current/item/overview`. Uses `chaosEquivalent` for currency/fragments, `chaosValue` for items. 60-min cache TTL.
- **TFT**: Static JSON from GitHub `The-Forbidden-Trove/tft-data-prices`. League codes: `lsc`/`lhc`. Lifeforce entries have `ratio: 1000` — must divide chaos by ratio.

## SECURITY: Public Repository

This repo is **public on GitHub**. Never commit:
- Passwords, API keys, tokens, secrets of any kind
- Internal hostnames, IP addresses, or infrastructure details
- Database connection strings or credentials
- Coolify webhook URLs or UUIDs
- Any `.env` file contents (`.env.example` with placeholders only)

All secrets live in Coolify env vars and GitHub Secrets — never in code or config files.

## Production Infrastructure

- **Server**: Go binary serving SvelteKit frontend + API (domain in Coolify env vars)
- **Collector**: 24/7 price data collection from poe.ninja (separate container)
- **Database**: TimescaleDB (managed via Coolify)
- **Deployment**: Coolify with Docker Build Stage Targets (`server` / `collector`), auto-deploy via GitHub Actions on merge to main
- **Mercure**: Event hub for collector → server SSE events (topics: `poe/collector/gems`, `poe/collector/currency`)

## Data Architecture & Analysis Patterns

### Time-Series Tables (TimescaleDB hypertables)

**`gem_snapshots`** — PK: `(time, name, variant, is_corrupted)`
- Columns: `time`, `name`, `variant`, `chaos`, `listings`, `is_transfigured`, `gem_color`, `is_corrupted`
- ~7,000 rows per snapshot (all gems × variants), snapshots every ~30min
- Indexed: `(time DESC)`, `(name, variant, is_corrupted, time DESC)`

**`currency_snapshots`** — similar structure for currency/fragment prices

### Querying Data for Analysis

Always query by time range — never SELECT * from hypertables:

```sql
-- Last hour of gem data
SELECT * FROM gem_snapshots WHERE time > NOW() - INTERVAL '1 hour';

-- Aggregated prices (30-min buckets, last 24h)
SELECT time_bucket('30 minutes', time) AS bucket, name, variant,
       AVG(chaos) AS avg_price, AVG(listings) AS avg_listings
FROM gem_snapshots
WHERE time > NOW() - INTERVAL '24 hours'
GROUP BY bucket, name, variant;

-- Price trend for a specific gem
SELECT time, chaos, listings FROM gem_snapshots
WHERE name = 'Empower Support' AND variant = '1'
  AND time > NOW() - INTERVAL '7 days'
ORDER BY time;
```

### Data API (server, not collector)

The **server** (`/api/snapshots`) is the data query API — supports time ranges, filtering, pagination. The **collector** only exposes simple status endpoints (`/health`, `/latest`). All analysis endpoints live on the server.

### Analysis Reference Data (`/var/www/poe/data/`)

- `price-history.jsonl` — 179 pre-computed analysis snapshots (Mar 11-14 2026) from Node.js prototype scripts. Contains transfigure ROI, font analysis, exchange prices, GCP data. Different schema from DB tables — use as reference for analysis algorithm design, not for DB import.

### Adding a New Data Source (Collector Endpoint Pattern)

To add a new poe.ninja endpoint (e.g., DivinationCards, Uniques):

1. **Migration**: Create `internal/db/migrations/{timestamp}_create_{type}_snapshots.up.sql` (and `.down.sql`). Copy the `currency_snapshots` pattern: hypertable, compression policy (7d), retention policy (90d), `(time, {id_col})` PK, `({id_col}, time DESC)` index.

2. **Snapshot type**: Add struct in `internal/collector/fetcher.go` (e.g., `FragmentSnapshot`). Fields: `Time`, ID field, `Chaos`, plus any type-specific fields.

3. **FetchResult field**: Add data slice to `FetchResult` in `endpoint.go`. Update `Validate()` to include the new slice in the mutual exclusivity check.

4. **Endpoint constant**: Add `EndpointNinja{Type}` to `endpoint.go` constants.

5. **Fetcher method**: Add `Fetch{Type}Endpoint` in `ninja.go`. If the poe.ninja response shape matches an existing type (e.g., Fragment uses same shape as Currency via `ninjaCurrencyLine`), reuse the response struct. Add a `convert{Type}Lines` function.

6. **Repository methods**: Add `Insert{Type}Snapshots` and `Last{Type}SnapshotTime` in `repository.go`. Follow the batch insert + `ON CONFLICT DO NOTHING` pattern.

7. **Mercure topic**: Add entry to `mercureTopicSuffix` map in `scheduler.go`.

8. **Wire in `cmd/collector/main.go`**: Clone the currency endpoint block, set `Name`, `FetchFunc`, `StoreFunc`, `StalenessFunc`. Add to the scheduler's endpoint slice.

9. **Server API**: Add query handler in `internal/server/handlers/snapshots.go`, route in `server.go`, and stats query in `SnapshotStats`.

10. **Server subscriber**: Add topic to the Mercure subscriber topics in `cmd/server/main.go`.

### Event Pipeline

Collector publishes Mercure events on each price update → Server subscribes via SSE → triggers analysis re-computation → pushes results to frontend. This is the pre-computed event-driven pipeline — analysis results are computed on data arrival, not on user request.

## Original Codebase Reference (git history only)

The PHP/Symfony codebase is read-only reference material in git history. Use `git show 537e37e:<path>` to inspect files when porting logic. Key files:
- `src/Domain/Strategy/Strategy.php` — core simulation loop
- `src/Infrastructure/Strategy/Runner.php` — recursive tree executor
- `src/Domain/Inventory/Inventory.php` — shared state + auto-buy
- `src/Domain/Inventory/SetConverter.php` — fragment→set conversion cascade
- `src/Infrastructure/Market/Buyer.php` — buy-side price source selection
- `src/Infrastructure/Pricer/Pricer.php` — sell-side pricing + c/hr calculation
- `src/Application/Command/PriceRegistry/UpdateRegistryHandler.php` — price normalization
- `src/Domain/Strategy/RunTheFormed.php` — best example of composite input/output
