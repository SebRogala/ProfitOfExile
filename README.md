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
| Database | PostgreSQL + TimescaleDB |
| Events | Mercure (SSE hub) |
| Deployment | Docker, Coolify, GitHub Actions CI/CD |

## Project Stats (as of: 05.05.2026)

| Metric | Count |
|--------|-------|
| Source lines | ~52k LOC (Go 20.9k, Svelte 19.4k, Rust 6.0k, TS 3.8k, SQL 1.7k, CSS 0.2k — excluding tests, generated assets, lockfiles) |
| Go packages | 19 |
| Go tests | 943 passing across 11 test packages |
| REST API endpoints | 30 |
| DB migrations | 41 |
| TimescaleDB hypertables | 14 |
| Desktop releases | 15 (v-desktop-0.1.0 → v-desktop-0.6.1) |
| Commits | 887 |

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

## Documentation

- [Documentation Index](docs/README.md) — canonical guides, accepted ADRs, proposed specifications, dated research, and historical plans.
- [Trade and Market Data Lifecycles](docs/TRADE-LIFECYCLE.md) — public-safe overview of collection, desktop-native trade, shared contributions, optional server trading, pairing, caching, and Mercure boundaries.
- [Overlay Guide](docs/OVERLAY-GUIDE.md) — Tauri overlay architecture and interaction conventions.

Proposed and not yet implemented, specified in the tracker: `POE-118` (Mercure lifecycle reliability), `POE-117` (league SSOT and rollover), and `POE-88` (LabCompass fidelity and overlay SSOT).

## License

MIT — see [LICENSE](LICENSE).
