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
- **Infra:** Shared Postgres image swapped to `timescaledb/timescaledb:2.17.2-pg16`
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

## Phase 2 — Price Collector Service (POE-18)

### 2.1 Standalone Go Binary
- **What:** Lightweight Go service running 24/7 on VPS, separate from the main app
- **Internal cron:** Every 15 minutes (poe.ninja cache is 30 min, 15 min is optimal ceiling)
- **Data flow:**
  ```
  poe.ninja API (SkillGem endpoint)
    → Parse: chaosValue, listingCount, variant, transfigured detection
    → Classify: gem color from icon gd field or gem_colors lookup
    → Write: INSERT INTO gem_snapshots
  ```
- **Endpoints (health/debug only):**
  - `GET /health` — last snapshot time, row count, uptime
  - `GET /snapshots/latest` — most recent snapshot (debug)
- **Resilience:** Log + skip on poe.ninja failure, retry next cycle. No crash on transient errors.
- **Deployment:** Single Docker container on Coolify, connects to TimescaleDB

### 2.2 What It Replaces
- `price-snapshot.mjs` — the cron-based JSONL writer
- `fetch-gem-colors.mjs` — gem color classification (baked into collector)
- The manual "run script, read terminal" workflow

---

## Phase 3 — Analysis API (POE-3 subtasks)

Go endpoints serving computed analysis from TimescaleDB data. The math is simple — the scripts prove it.

### 3.1 `GET /api/lab/transfigure`
- **Replaces:** `lab-transfigure-analysis.mjs`
- **Logic:** For each transfigured gem, find its base gem, compute ROI = transfigured_price - base_price
- **Query:** Join latest snapshot on gem name (strip " of XXX" suffix to find base)
- **Response:** Sorted by ROI desc, includes listings count, confidence flag (<5 listings = LOW)
- **Params:** `?variant=20/20&top=20`

### 3.2 `GET /api/lab/font`
- **Replaces:** `font-analysis.mjs`
- **Logic:** Group transfigured gems by color → compute per-color pool stats:
  - Pool size, winner count (above threshold), P(win) via hypergeometric formula
  - EV = P(win) × avg_winner_price
  - Profit = EV - input_cost
- **Response:** Per-color breakdown with top gems listed
- **Params:** `?variant=20/20&threshold=50&min_listings=5`

### 3.3 `GET /api/lab/quality`
- **Replaces:** `lab-quality-analysis.mjs`
- **Logic:** For each gem, compute ROI at +4%, +6%, +10%, +15% quality rolls
  - Value at N% quality = (20% price) - (20-N) × GCP_cost
  - ROI = value_at_quality - buy_price_at_0%
- **Response:** Sorted by avg ROI, per-roll breakdown
- **Params:** `?level=20&gcp_cost=4&top=20`

### 3.4 `GET /api/lab/loadout`
- **Replaces:** `lab-loadout.mjs`
- **Logic:** Combines all 3 analyses, returns unified ranking
  - Best gem for transfigure enchant
  - Best gem for quality enchant
  - Best color for font enchant
  - Budget-aware recommendations
- **Response:** Optimized loadout with tier annotations (Common vs Special/Heist)
- **Params:** `?budget=300&gcp_cost=4`

### 3.5 `GET /api/trends/{type}`
- **Replaces:** `price-trends.mjs`
- **Types:** `movers`, `saturation`, `timing`, `font-history`, `listings`
- **Logic:** TimescaleDB time_bucket queries over price history
  - **Movers:** Biggest ROI changes over time window
  - **Saturation:** Rising listings + falling price (leading indicator)
  - **Timing:** Peak/trough hours (CET) for ROI and Font EV
  - **Listings:** Supply velocity and hourly patterns
- **Params:** `?hours=24&variant=20/20&gem=Molten+Strike`

---

## Phase 4 — SvelteKit Dashboard (POE-11 subtasks)

### 4.1 Lab Loadout Page (`/lab`)
- **Primary view:** Unified table showing best gem per enchant type (transfigure, quality, font)
- **Columns:** Gem name, enchant type, ROI/EV, confidence, listings, tier
- **Budget filter:** Slider or input to filter by max gem cost
- **Auto-refresh:** Poll API every 60s or use Mercure SSE

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
- **Ranked table** with per-roll ROI columns (+4%, +6%, +10%, +15%)
- **Avg ROI column** for quick sorting
- **GCP cost input** (adjustable, defaults to current market price)

### 4.5 Trends Dashboard (`/trends`)
- **Sparklines** for top gem ROI over time (24h/48h/7d toggle)
- **Saturation alerts:** Highlighted rows when listings surge + price drops
- **Time-of-day heatmap:** Best sell/buy windows (CET hours)
- **Movers panel:** Biggest crashers and risers in selected time window

### 4.6 Auto-Refresh
- Option A: **Polling** — fetch API every 60s, simplest
- Option B: **Mercure SSE** — push on new snapshot arrival (POE-12, lower priority)
- Start with polling, upgrade to Mercure later

---

## What Gets Retired from `/var/www/poe`

| Script | Replaced By | Phase |
|--------|-------------|-------|
| `fetch-gem-colors.mjs` | Baked into collector + DB seed | 2 |
| `price-snapshot.mjs` | Price collector service (24/7) | 2 |
| `font-analysis.mjs` | `GET /api/lab/font` + Font EV page | 3+4 |
| `lab-transfigure-analysis.mjs` | `GET /api/lab/transfigure` + Transfigure page | 3+4 |
| `lab-quality-analysis.mjs` | `GET /api/lab/quality` + Quality page | 3+4 |
| `lab-loadout.mjs` | `GET /api/lab/loadout` + Loadout page | 3+4 |
| `price-trends.mjs` | `GET /api/trends/*` + Trends dashboard | 3+4 |
| `inspect-gems.mjs` | Not needed (debug utility) | — |
| `gem-exp-check.mjs` | Not needed (incomplete prototype) | — |

**Knowledge files** (`poe1-knowledge.md`, `strategies/`) stay as reference — no migration needed.

---

## Build Order (Dependency Chain)

```
Phase 1: VPS + Coolify + TimescaleDB + Mercure ✅ COMPLETE
    │
    ▼
Phase 2: Price collector (data starts accumulating)  ← YOU ARE HERE
    │
    ├──▶ Phase 3: Analysis API endpoints (can develop locally against collected data)
    │       │
    │       ▼
    │    Phase 4: SvelteKit dashboard pages
    │
    └──▶ Meanwhile: Node scripts keep working for current league
```

**Key insight:** Phase 2 is time-sensitive — every day without the collector is lost price history. Phases 3+4 can be built at any pace since the data is accumulating in the background.

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
