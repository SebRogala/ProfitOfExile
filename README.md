# ProfitOfExile

Free, open-source Windows companion app for Path of Exile 1. It reads
`Client.txt` to know when a game panel is relevant, reads that panel with
Windows OCR, checks the market, and shows the verdict in click-through in-game
overlays.

[Website](https://profitofexile.top/) ·
[Windows installer](https://github.com/SebRogala/ProfitOfExile/releases/latest/download/ProfitOfExile-setup.exe) ·
[Portable build](https://github.com/SebRogala/ProfitOfExile/releases/latest/download/ProfitOfExile-standalone.exe) ·
[Discord](https://discord.gg/QX53hrv5GP)

**Requirements:** Windows 10 or 11 and Path of Exile 1. On non-English
Windows installations, the app may also need Microsoft's English (United
States) OCR language pack. If OCR produces garbled gem names or Settings shows
the language-pack warning, follow the
[English OCR pack instructions](https://profitofexile.top/#ocr-language-pack).

## What It Does

- **Lab Farming** — Prices and ranks Divine Font gem choices, tracks Font
  crafts, and provides Labyrinth route and compass overlays.
- **Temple of Atzoatl (beta)** — Reads the temple sheet and ranks both
  architect choices, including room value, reasoning, and gamble risk.
- **Mercenaries (beta)** — Reads a recruit window and shows a verdict per
  community guide ruleset, with optional live trade checks.
- **Currency Exchange (beta)** — Ranks arbitrage routes from GGG's public
  currency-exchange feed and derives a worthwhile trade size.
- **Live dashboard** — Shows gem rankings, market signals, and Divine Font
  expected value from periodically refreshed market data.

Beta modules are enabled per device. Ask in
[Discord](https://discord.gg/QX53hrv5GP) if you want to test one.

## How It Works

1. Events in Path of Exile's `Client.txt` arm the relevant module.
2. When needed, the app captures only the configured panel region and reads it
   with Windows' built-in OCR.
3. Server-side analysis and market data price or rank the available choices.
4. Click-through overlays place the result over the game without blocking it.

ProfitOfExile does not read game memory or send clicks, keystrokes, or chat
commands to the game. OCR happens locally. See the
[Transparency & Terms of Service](https://profitofexile.top/#transparency)
section for the complete data-flow and compliance description.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Backend | Go 1.23 (chi router, standard library HTTP) |
| Frontend | SvelteKit + Svelte 5 (runes), Tailwind CSS v4, adapter-static |
| Desktop | Tauri 2.0 (Rust) + SvelteKit, Windows OCR |
| Database | PostgreSQL + TimescaleDB |
| Events | Mercure (SSE hub) |
| Deployment | Docker, Coolify, GitHub Actions CI/CD |

## Engineering Highlights

- A Go API and independent collector use direct, parameterized `pgx` queries
  against PostgreSQL/TimescaleDB; analysis is computed when market data arrives,
  not rebuilt for every request.
- The SvelteKit web app and Tauri desktop app consume the same server models,
  while Rust owns Windows capture, native OCR, and overlay-window behavior.
- Mercure SSE invalidates client caches after collector updates, with server and
  client debounce paths to avoid synchronized reload bursts.
- Production embeds the prerendered SvelteKit build in the Go server binary;
  Docker multi-stage builds and GitHub Actions cover server, frontend, and
  desktop quality gates.
- Cross-language fixtures keep Go, Rust, and TypeScript implementations aligned
  where wire formats and image-derived signatures cross process boundaries.

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

Fresh machine (WSL prerequisites, shared infra, Windows toolchain, `desktop/` sync, `npx tauri dev`): [docs/DEV-SETUP.md](docs/DEV-SETUP.md).

## Documentation

- [Documentation Index](docs/README.md) — canonical guides, accepted ADRs, proposed specifications, dated research, and historical plans.
- [Trade and Market Data Lifecycles](docs/TRADE-LIFECYCLE.md) — public-safe overview of collection, desktop-native trade, shared contributions, optional server trading, pairing, caching, and Mercure boundaries.
- [Overlay Guide](docs/OVERLAY-GUIDE.md) — Tauri overlay architecture and interaction conventions.
- [Game Facts](docs/GAME-FACTS.md) — dated, sourced Path of Exile facts the implementation relies on.
- [AI-native case study](docs/AI-NATIVE-CASE-STUDY.md) — how the project is delivered and what the workflow changed.

## License

GPL-3.0 — see [LICENSE](LICENSE).

This project was MIT-licensed until July 2026. It is GPL-3.0 because it
incorporates material from [yznpku/LabCompass](https://github.com/yznpku/LabCompass),
itself GPL-3.0, and the GPL requires derivative works to carry the same license.

### Third-party attribution

From [yznpku/LabCompass](https://github.com/yznpku/LabCompass) (GPL-3.0):

- `desktop/static/compass/presets/*.svg` — the 54 labyrinth room-preset drawings,
  copied verbatim from `app/resources/images/room-preset/`.
- `desktop/src/lib/compass/room-presets.ts` — room preset door-layout data,
  ported from the same project.
- `desktop/src-tauri/src/lab_navigation.rs` — Izaro voiceline strings used to
  detect lab events from `Client.txt`.
- `desktop/src/lib/compass/__fixtures__/poelab-*.json` — two real poelab layout
  exports, used as test fixtures.

The route-cost model in `desktop/src/lib/compass/navigation.ts` reimplements
LabCompass's room-cost weighting; the surrounding routing engine is our own.
