# Plan: Live Lab Farming Dashboard

**Goal:** Replace `/var/www/poe` CLI scripts with a self-updating web dashboard at `profitofexile.localhost` (and eventually production). Open browser → see live market state.

**YouTrack epics:** POE-17 (Market Intelligence), POE-3 (Price Engine), POE-11 (Lab Dashboard)

---

## Phase 1 — Infrastructure ✅ COMPLETE

### 1.1 VPS Migration: CapRover → Coolify ✅ DONE
- **What:** Migrated VPS hosting platform to Coolify with TimescaleDB
- Coolify handles Docker Compose natively, DNS/SSL configured

### 1.2 TimescaleDB Schema (POE-19) ✅ DONE — PR #8, merged 2026-03-13
- **What:** PostgreSQL + TimescaleDB extension (local infra + VPS)
- **Infra:** Shared Postgres image swapped to `timescale/timescaledb:latest-pg16`
- **Schema implemented:**
  - `gem_snapshots` hypertable — separate-row model (one row per gem per variant per snapshot, `chaos`/`listings`/`is_transfigured`/`gem_color`)
  - `font_snapshots` hypertable — per-color per-variant pool stats
  - `exchange_snapshots` hypertable — empower/enlighten/enhance prices
  - `gcp_snapshots` hypertable — GCP chaos price
  - `gem_colors` lookup table — ~750 entries seeded from RePoE data
- **Policies:** Compression (7d), retention (90d), continuous aggregates (hourly + daily rollups)
- **Gem color resolver:** `internal/price/gemcolor/` — in-memory resolver with Vaal/Greater prefix stripping, progressive " of " suffix stripping, dynamic discovery + upsert
- **Tests:** Unit tests for resolver, integration tests for migrations + resolver against real TimescaleDB
- **Scale:** ~800 gems × 96 snapshots/day = ~77k rows/day, ~7M rows/league

### 1.3 Mercure SSE Hub (POE-12) ✅ DONE — PR #9, merged 2026-03-13
- **What:** Added Mercure hub to shared infra for real-time SSE updates
- **Infra:** `dunglas/mercure` service in `/var/www/infra/docker-compose.yml`
- **Routing:** Traefik `websecure` entrypoint at `mercure.localhost` with TLS
- **App:** `MERCURE_URL` + `MERCURE_JWT_SECRET` env vars in ProfitOfExile `docker-compose.yml`
- **Dev mode:** Anonymous subscriptions + permissive CORS

---

## Phase 2 — Price Collector Service (POE-18) ✅ COMPLETE

### 2.1 Standalone Go Binary ✅ DONE — PR #10, merged 2026-03-13
- **What:** Lightweight Go service running 24/7 on VPS, separate from the main app
- **Data flow:**
  ```
  poe.ninja SkillGem API → Parse gems (incl. corrupted) → gem_snapshots hypertable
  poe.ninja Exchange API  → Parse currency (104 items)   → currency_snapshots hypertable
  ```
- **Endpoints (health/debug only):**
  - `GET /health` — last snapshot time, row count, uptime
  - `GET /snapshots/latest` — most recent snapshot (debug)
- **Resilience:** Log + skip on poe.ninja failure, retry next cycle. No crash on transient errors.
- **Deployment:** Single Docker container on Coolify, connects to TimescaleDB

### 2.2 Smart Polling (POE-22) ✅ DONE — PR #11, merged 2026-03-13
- **What:** Replaced fixed 15-min interval with cache-aware smart polling
- **Architecture:** Goroutine-per-endpoint scheduler, each endpoint runs independent fetch→sleep→fetch loop
- **Cache-aware sleep:** Reads `age` header, calculates `sleep = max-age - age + 5s buffer` (~30min cycles aligned to poe.ninja refresh)
- **ETag/If-None-Match:** Conditional requests — 304 = cheap no-op instead of 7MB download
- **304 retry limit:** 5 consecutive, then fall back to configurable interval
- **Per-endpoint config:** `EndpointConfig` with `FetchFunc`/`StoreFunc`/`StalenessFunc` function fields, env var overrides (`NINJA_FALLBACK_INTERVAL`, `NINJA_MAX_RETRIES`, `NINJA_MIN_SLEEP`)
- **Startup jitter:** Random 2-7s delay per goroutine to avoid thundering herd

### 2.3 Hotfixes (post-merge, 2026-03-13)
- **Corrupted gems:** Removed collector-level filter — corrupted gems are needed for Enriched Eternal Lab strategies (Divine Font transform options). Added `is_corrupted` column to `gem_snapshots` with updated PK/index.
- **Currency endpoint:** Fixed URL from `stash/current/item` to `exchange/current` — different API with different response structure (`id`/`primaryValue` vs `currencyTypeName`/`chaosEquivalent`).
- **Infra:** Fixed TimescaleDB image name (`timescale/timescaledb:latest-pg16`), replaced fragile `\gexec` in init script with `createdb`, added `CREATEDB` privilege to roles.

### 2.4 What It Replaces
- `price-snapshot.mjs` — the cron-based JSONL writer
- `fetch-gem-colors.mjs` — gem color classification (baked into collector)
- The manual "run script, read terminal" workflow

---

## Phase 3 — Event-Driven Analysis Pipeline (POE-23)

**Architecture:** NOT request-time computation. Event-driven pipeline where analysis results are pre-computed on every collector update and stored in DB. Tracked as POE-23 under POE-3.

```
Collector container (exists)     Go app container                    Browser
────────────────────────────     ────────────────                    ───────
save to DB                       subscribe Mercure
  → publish Mercure         →      → parallel goroutines:
    topic: collector/gems            - Transfigure ROI analyzer
    topic: collector/currency        - Font EV analyzer
                                     - Quality ROI analyzer
                                     - Trends / velocity analyzer
                                   each: compute → save to DB
                                   → throttle/debounce 1-2s
                                   → publish Mercure            →   subscribe Mercure
                                     topic: lab/analysis-updated      → fetch fresh data
                                                                      → re-render
```

### 3.1 Collector: Mercure Publish
- **What:** Add Mercure publish after each save in existing collector code
- **Topics:** `collector/gems`, `collector/currency` (one per data type)
- **Payload:** Minimal — just snapshot timestamp + row count
- **Size:** Tiny change to existing `cmd/collector/`

### 3.2 App: Mercure Subscriber
- **What:** Go app subscribes to `collector/*` topics via SSE client
- **On event:** Dispatches to registered analyzers based on topic
- **Package:** `internal/analysis/subscriber.go`

### 3.3 Analysis Engine — Parallel Goroutines
- **What:** One goroutine per analysis type (NOT per lab variant), all fire in parallel
- **Analyzers are per-type, shared across lab variants:**

**Transfigure ROI** (replaces `lab-transfigure-analysis.mjs`)
- For each transfigured gem, find base gem (strip " of XXX" suffix), compute ROI = transfigured - base price
- Confidence flag: <5 listings = LOW
- Stores: per-gem ROI, base price, transfigured price, listings, confidence

**Font EV** (replaces `font-analysis.mjs`)
- Group transfigured gems by color → per-color pool stats
- Pool size, winner count (above threshold), P(win) via hypergeometric formula
- EV = P(win) × avg_winner_price, Profit = EV - input_cost
- Stores: per-color stats with top gems listed

**Quality ROI** (replaces `lab-quality-analysis.mjs`)
- Per gem, compute ROI at +4%, +6%, +10%, +15%, +20% quality rolls
- Value at N% = (20% price) - (20-N) × GCP_cost
- Stores: per-gem per-roll ROI breakdown

**Trends / Velocity** (replaces `price-trends.mjs`)
- Diff current vs previous snapshot: price delta, listings delta
- Movers: biggest ROI changes over time window
- Saturation: rising listings + falling price (leading indicator)
- Stores: per-gem deltas, top movers, saturation flags

**Computation is trivial:** ~800 gems × simple math = microseconds. DB read/write (~50ms each) is the bottleneck.

### 3.4 Throttler / Debouncer
- **What:** Collects "analysis done" signals from goroutines, waits 1-2s to deduplicate, publishes single Mercure event
- **Topic:** `lab/analysis-updated`
- **Purpose:** Prevents frontend from getting 4 separate "refresh" signals in quick succession

### 3.5 Thin API — Serve Pre-Computed Results
- `GET /api/lab/analysis` — all pre-computed analysis data, variant applied as query param
- **Lab variants are a lens, NOT separate computation:**

| Variant | Entry Cost | Font Uses | Max Quality | Special Enchants |
|---------|-----------|-----------|-------------|-----------------|
| Merciless Lab | Standard | 1 | 15% | — |
| Uber Lab | Fragment | 2 | 20% | — |
| Gift | TBD | TBD | TBD | TBD |
| Dedication | TBD | TBD | TBD | Corrupted gem exchange |
| Tribute | TBD | TBD | TBD | TBD |

99% of computation is shared. Variant just adjusts entry cost, font uses, quality caps, available enchant types.

### 3.6 End-to-End Latency
```
Collector save → ~1ms Mercure publish → ~50ms DB read → ~1ms math → ~50ms DB write → 1-2s throttle → Mercure push
```
~2s total including deliberate debounce.

---

## Phase 4 — SvelteKit Dashboard (POE-11 subtasks)

Frontend subscribes to Mercure and renders pre-computed data with variant lens.

### 4.1 Lab Dashboard (`/lab`)
- **Primary view:** Unified loadout showing best gem per enchant type (transfigure, quality, font)
- **Variant selector:** Merciless / Uber (applies entry cost, font uses, quality caps)
- **Auto-refresh:** Mercure SSE subscription — updates seconds after new data
- **Columns:** Gem name, enchant type, ROI/EV, confidence, listings, tier
- **Budget filter:** Slider or input to filter by max gem cost

### 4.2 Transfigure ROI Table (`/lab/transfigure`)
- **Full ranked table** of all transfigured gems with ROI
- **Columns:** Gem, base price, transfigured price, ROI, ROI%, listings, confidence
- **Sortable columns**, variant toggle (1/20 vs 20/20)
- **Visual:** Color-coded ROI (green = high, red = negative)

### 4.3 Font EV Analysis (`/lab/font`)
- **Per-color cards** (RED/GREEN/BLUE) showing pool stats, P(win), EV, profit
- **Expandable:** Top winners per color with price + listings
- **Variant comparison:** Side-by-side 1/20 vs 20/20

### 4.4 Quality ROI Table (`/lab/quality`)
- **Ranked table** with per-roll ROI columns (+4%, +6%, +10%, +15%, +20%)
- **Avg ROI column** for quick sorting
- **GCP cost input** (adjustable, defaults to current market price)

### 4.5 Trends Dashboard (`/trends`)
- **Sparklines** for top gem ROI over time (24h/48h/7d toggle)
- **Saturation alerts:** Highlighted rows when listings surge + price drops
- **Time-of-day heatmap:** Best sell/buy windows (CET hours)
- **Movers panel:** Biggest crashers and risers in selected time window

---

## What Gets Retired from `/var/www/poe`

| Script | Replaced By | Phase |
|--------|-------------|-------|
| `fetch-gem-colors.mjs` | Baked into collector + DB seed | 2 |
| `price-snapshot.mjs` | Price collector service (24/7) | 2 |
| `font-analysis.mjs` | Font EV analyzer (event-driven) + Font EV page | 3+4 |
| `lab-transfigure-analysis.mjs` | Transfigure ROI analyzer (event-driven) + Transfigure page | 3+4 |
| `lab-quality-analysis.mjs` | Quality ROI analyzer (event-driven) + Quality page | 3+4 |
| `lab-loadout.mjs` | Variant lens on shared analysis results + Loadout page | 3+4 |
| `price-trends.mjs` | Trends/velocity analyzer (event-driven) + Trends dashboard | 3+4 |
| `inspect-gems.mjs` | Not needed (debug utility) | — |
| `gem-exp-check.mjs` | Not needed (incomplete prototype) | — |

**Knowledge files** (`poe1-knowledge.md`, `strategies/`) stay as reference — no migration needed.

---

## Build Order (Dependency Chain)

```
Phase 1: VPS + Coolify + TimescaleDB + Mercure ✅ COMPLETE
    │
    ▼
Phase 2: Price collector (data accumulating) ✅ COMPLETE
    │
    ▼
Phase 3: Event-driven analysis pipeline (POE-23)
    │
    ├─ 3.1 Collector: Mercure publish after save (tiny change)
    ├─ 3.2 App: Mercure subscriber (listen to collector topics)
    ├─ 3.3 Analysis engine: parallel goroutines (transfigure, font, quality, trends)
    ├─ 3.4 Throttler/debouncer → Mercure push to frontend
    ├─ 3.5 Thin API: serve pre-computed results with variant lens
    │       │
    │       ▼
    │    Phase 4: SvelteKit dashboard (subscribes to Mercure, renders data)
    │
    └──▶ Meanwhile: Node scripts keep working for current league
```

**Key insight:** Phase 2 is time-sensitive — every day without the collector is lost price history. Phases 3+4 can be built at any pace since the data is accumulating in the background. Analysis results are pre-computed (event-driven), not calculated on request.

### Historical Data Migration
- **Source:** `/var/www/poe/data/price-history.jsonl` (~70 snapshots, 33 hours, 2.7 MB)
- **What:** One-shot Go or Node script that reads JSONL and INSERTs into `gem_snapshots`
- **When:** Run once after TimescaleDB schema is up, before collector starts
- **Mapping:** Each JSONL line has `timestamp` + per-gem `{ base, trans, roi, baseLst, transLst }` per variant → flattens into hypertable rows
- **Bonus:** Keep `price-snapshot.mjs` running until collector is deployed — migrate all accumulated JSONL at cutover

---

## Open Questions

1. **Quality analysis scope:** Port `lab-quality-analysis.mjs` as-is, or wait until simulation engine (POE-4) handles it generically?
2. **TFT integration:** POE-3 mentions TFT bulk prices. Include in collector from day 1, or add later?
3. **Auth:** Dashboard is public (no login) for v1? Or add edit_token-based views later?
4. **Mobile:** Any need for responsive tables on phone while farming?
