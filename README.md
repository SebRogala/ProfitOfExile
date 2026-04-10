# ProfitOfExile

Real-time profit analysis platform for Path of Exile 1 lab farming. Fetches live market prices, analyzes gem transfiguration profitability, and provides actionable signals — including a desktop overlay that reads your in-game crafting options via OCR.

## What It Does

- **Live Price Collection** — Ingests gem, currency, and fragment prices from poe.ninja into TimescaleDB hypertables (~7,000 gem rows per snapshot, every 30 minutes)
- **Lab Farming Analysis** — Computes transfiguration ROI, font of divine skill profitability (hypergeometric model), and quality gem value — all pre-computed on data arrival, not on request
- **Market Signals** — Tracks listing velocity, price trends, and saturation risk. Classifies gems into confidence tiers (TOP/HIGH/MID/LOW/FLOOR) per variant
- **Desktop Overlay** — Tauri app that sits on top of Path of Exile, reads crafting bench options via OCR, and shows real-time profit signals as a transparent overlay
- **Event-Driven Pipeline** — Collector publishes via Mercure SSE, server recomputes analysis, frontend updates live

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Backend | Go 1.23 (chi router, standard library HTTP) |
| Frontend | SvelteKit + Svelte 5 (runes), Tailwind CSS v4, adapter-static |
| Desktop | Tauri 2.0 (Rust) + SvelteKit, OCR via Tesseract |
| Database | PostgreSQL + TimescaleDB (12 hypertables) |
| Events | Mercure (SSE hub) |
| Deployment | Docker, Coolify, GitHub Actions CI/CD |

## Project Stats

| Metric | Count |
|--------|-------|
| Source lines | ~107k (68k Go, 31k Svelte/TS, 5k Rust, 4k SQL) |
| Go packages | 33 |
| Test suites | 10 (886 tests, all passing) |
| REST API endpoints | 33 |
| DB migrations | 39 |
| TimescaleDB hypertables | 12 |
| Desktop releases | 11 |
| Commits | 800+ |

## Architecture

```
                    ┌─────────────┐
                    │  poe.ninja  │
                    └──────┬──────┘
                           │
                    ┌──────▼──────┐      Mercure SSE
                    │  Collector  │─────────────────┐
                    │  (Go, 24/7) │                  │
                    └─────────────┘                  │
                                              ┌──────▼──────┐
                                              │   Server    │
                                              │  (Go API)   │
                                              └──────┬──────┘
                                                     │
                                    ┌────────────────┼────────────────┐
                                    │                │                │
                             ┌──────▼──────┐  ┌──────▼──────┐ ┌──────▼──────┐
                             │  SvelteKit  │  │   Desktop   │ │  Trade API  │
                             │  (Web UI)   │  │  (Tauri)    │ │  (PoE GGG)  │
                             └─────────────┘  └─────────────┘ └─────────────┘
```

## Development

Everything runs in Docker — no local Go/Node tooling needed.

```bash
make up          # Start all services (Go + SvelteKit with hot reload)
make test        # Run all Go tests
make migration name=add_foo   # Generate new migration pair
```

Single domain via Traefik: `/api` routes to Go, everything else to Vite dev server.

## License

All rights reserved.
