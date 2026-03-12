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

## Core Domain Concepts

**Strategy Tree**: Composable tree of farming activities. Each strategy consumes items, produces items with probability, and takes time. Parent nodes run children N times (`series` count) before executing themselves. A `Wrapper` is a no-op grouping node.

**Inventory-Driven Cascade**: All strategies share a single Inventory. On every `Add()`, the SetConverter checks conversion rules (e.g., 4 fragments → 1 set). When a strategy needs an item not in inventory, auto-buy triggers from the cheapest source.

**Multi-Source Pricing**: Prices fetched from poe.ninja (individual trade) and TFT (bulk trade). Each item has per-source buy/sell prices. The system computes optimal buy source (lowest) and sell source (highest).

**Breakpoint Analysis**: Simulates at each tree depth to determine where in a strategy chain it's most profitable to stop and sell intermediates vs. continue.

## Key Design Documents

- `BACKBONE.md` — Authoritative design reference. Full architecture, data source specs, domain model, API contracts, and feature roadmap. **When in doubt, refer to BACKBONE.**
- **YouTrack (project POE)** — Epics and task breakdown. All implementation tracking lives in the tracker, not in files.

## Data Sources

- **poe.ninja**: REST API at `poe.ninja/poe1/api/economy/stash/current/item/overview`. Uses `chaosEquivalent` for currency/fragments, `chaosValue` for items. 60-min cache TTL.
- **TFT**: Static JSON from GitHub `The-Forbidden-Trove/tft-data-prices`. League codes: `lsc`/`lhc`. Lifeforce entries have `ratio: 1000` — must divide chaos by ratio.

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
