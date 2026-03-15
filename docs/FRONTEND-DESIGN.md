# Lab Farming Dashboard — Frontend Design Spec

## Tech Stack
- SvelteKit with adapter-static
- Tailwind CSS v4 (dark mode default, light mode later)
- Desktop-only for v1
- Svelte 5 runes ($props, {@render})

## Data Flow
```
Collector snapshot → Mercure event (poe/collector/gems) → Server analysis
  → Throttler batches → Mercure event (poe/analysis/updated)
  → Frontend SSE receives → Fetches /api/analysis/* → Svelte reactivity updates DOM
```
No full page reload. Reactive data updates only.

## Global Header

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  ⚗️ ProfitOfExile — Lab Farming Dashboard                                   │
│  Lab: [●Merciless] [Uber] [Gift] [Dedication] [Tribute]                     │
│                                                                              │
│  Last update: 14:15 UTC (2 min ago)  │  Next: ~14:45 UTC (28 min)          │
│  ● Live — connected to event stream                                         │
└──────────────────────────────────────────────────────────────────────────────┘
```

- Lab selector is global — changes all sections below
- Dedication swaps to completely different view (corrupted exchange only)
- Gift makes Font EV dominant (×8 uses)
- Connection indicator: green dot = SSE connected, red = disconnected
- "Next update" comes from collector's nextFetch in Mercure payload

## Section 1: Lab Options Comparator

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  🔍 Lab Options Comparator                              Variant: [20/20 ▼] │
│  [search gem 1         ] [search gem 2         ] [search gem 3         ]    │
│  ┌────────────────────────┬────────────────────────┬────────────────────────┐│
│  │ Gem Name               │ Gem Name               │ Gem Name              ││
│  │ ROI (ROI%)  🔵 COLOR    │ ROI (ROI%)  🔴 COLOR    │ ROI (ROI%)  🔴 COLOR  ││
│  │ Signal  CV: X%          │ Signal  CV: X%          │ Signal  CV: X%        ││
│  │ Trans: X lst ↑↓X/2h    │ Trans: X lst ↑↓X/2h    │ Trans: X lst ↑↓X/2h  ││
│  │ Base: X lst ↑↓X/2h Liq │ Base: X lst ↑↓X/2h Liq │ Base: X lst ↑↓X/2h  ││
│  │ Window: SIGNAL          │ Window: SIGNAL          │ Window: SIGNAL        ││
│  │ ████░░░ (2h sparkline)  │ ████░░░ (2h sparkline)  │ ████░░░ (sparkline)  ││
│  │ Signal history:         │ Signal history:          │ Signal history:       ││
│  │  HH:MM old→new (reason) │  HH:MM old→new (reason) │  HH:MM old→new      ││
│  │  HH:MM old→new (reason) │  HH:MM old→new (reason) │  HH:MM old→new      ││
│  │  HH:MM old→new (reason) │  HH:MM old→new (reason) │  HH:MM old→new      ││
│  │ ✅ BEST / 👀 OK / 🚫    │                          │                      ││
│  └────────────────────────┴────────────────────────┴────────────────────────┘│
└──────────────────────────────────────────────────────────────────────────────┘
```

- Searchable select inputs (autocomplete from /api/analysis/gems/names)
- Variant selector independent from "By Variant" section below
- Recommendation: BEST/OK/AVOID based on signal-weighted ROI comparison

## Section 2: Window Alerts

Only shown when active windows exist (BREWING, OPENING, OPEN, CLOSING, EXHAUSTED).

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  ⚠ Window Alerts                                                            │
│  🟢 OPEN     Gem Name (variant)    ROI   Trans lst   Base lst ↓X/2h  Liq   │
│              Action text. History: HH:MM old→new → HH:MM old→new           │
│  🍺 BREWING  Gem Name (variant)    ROI   Trans lst   Base lst ↓X/2h  Liq   │
│              Action text. History: ...                                       │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Section 3: Best Plays Now (ALL variants)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  🏆 Best Plays Now (ALL variants)         [budget: ___]  [sort: ROI ▼|ROI%] │
│                                                                              │
│  Gem                      Var   ROI  ROI% Signal CV  Window Adv Trans Base  │
│  ────────────────────────────────────────────────────────────────────────── │
│  Gem Name                 20   940c 892% ▲RISE 23% CLOSED      97↑  469↓  │
│    ████░░ (2h) History: HH:MM→HH:MM→HH:MM signals with reasons             │
│  ...                                                                         │
└──────────────────────────────────────────────────────────────────────────────┘
```

- Crosses ALL variant groups — variant shown per gem
- Budget filter optional (empty = unlimited)
- Sort toggle: absolute ROI vs ROI%
- Budget <= 50c auto-sorts by ROI%
- TRAP signal gems excluded entirely

## Section 4: By Variant

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  📋 By Variant   [●ALL] [1/0] [1/20] [20/0] [20/20]                        │
│                                                                              │
│  (when ALL selected: all 4 variant blocks stacked vertically)               │
│                                                                              │
│  ┌─ 20/20 ────────────────────────────────────────────────────────────────┐ │
│  │  Best Plays (same columns as above)                                    │ │
│  │  ...                                                                   │ │
│  │                                                                        │ │
│  │  Font EV                                                               │ │
│  │  🔴 RED   EV:Xc  Pool:X  Winners:X  pWin:X%  Profit:Xc  Δ2h: EV ±Xc │ │
│  │  🟢 GREEN EV:Xc  Pool:X  Winners:X  pWin:X%  Profit:Xc  Δ2h: EV ±Xc │ │
│  │  🔵 BLUE  EV:Xc  Pool:X  Winners:X  pWin:X%  Profit:Xc  Δ2h: EV ±Xc │ │
│  │  vs Quality avg ROI: Xc → Font [color] wins by Xc                     │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  ┌─ 20/0 ─────────────────────────────────────────────────────────────────┐ │
│  │  ...same layout...                                                     │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│  ┌─ 1/20 ─────────────────────────────────────────────────────────────────┐ │
│  │  ...                                                                   │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│  ┌─ 1/0 ──────────────────────────────────────────────────────────────────┐ │
│  │  ...                                                                   │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

- [ALL] shows all 4 stacked — full market overview in one scroll
- Individual tabs show only that variant
- Font EV multiplied by lab variant uses (×1 Merci, ×2 Uber, ×8 Gift)
- Quality comparison shown below Font in each variant block

## Section 5: Market Overview

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  📊 Market Overview                                          Updated: 14:15 │
│                                                                              │
│  Market avg price (transfigured): 82c (↑3c/2h)    Active gems: 170         │
│  Market avg base listings: 127 (↓8/2h)             Weekend premium: ~30%    │
│  Gems with WINDOW signals: 2 (OPEN:1, BREWING:1)   Gems with TRAP: 8       │
│  Most volatile: 🔵 BLUE (avg CV: 45%)              Most stable: 🔴 RED 28% │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Section 6: Legend (collapsible, collapsed by default)

Users learn signals quickly — legend takes unnecessary space after first visit.
Collapsed shows just: `📖 Legend ▶` — click to expand full reference.
All signal badges have hover tooltips regardless of legend state.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  📖 Legend                                                                   │
│                                                                              │
│  Signals                         Window Lifecycle           Advanced Signals │
│  ▲ STABLE  steady price+lst      CLOSED   no opportunity    🔄 COMEBACK     │
│  ▲ RISING  price increasing      BREWING  forming (~2h)     💎 POTENTIAL    │
│  ▼ FALLING price decreasing      OPENING  base draining     ⚠ MAN manipul. │
│  ⚠ HERD   price+lst both up     OPEN     farm now!                         │
│  ⚠ DUMP   price↓ listings↑      CLOSING  herd arriving     Liquidity Tiers │
│  ↻ RECOV  price↓ listings↓      EXHAUSTED no bases          HIGH safe farm  │
│  ⚠ TRAP  CV>100% avoid                                     MED  oscillating│
│                                                              LOW  drain risk│
│  Metrics                                                                     │
│  ROI  — profit in chaos (transfigured - base price)                         │
│  ROI% — return on investment % (ROI / base × 100)                           │
│  CV   — coefficient of variation. <25% safe, >100% trap                     │
│  EV   — expected value from Font. pWin × avg winner price                   │
│  pWin — probability of winner from font pool (3 picks, hypergeometric)      │
│  Liq  — base gem liquidity vs market average. Predicts drain speed          │
│  Δ2h  — change over last 2 hours (4 snapshots)                              │
│                                                                              │
│  Data refreshes every ~30 min from poe.ninja.                               │
│  Last: 14:15 UTC │ Next: ~14:45 UTC │ Collector uptime: 18h 32m            │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Lab Variant: Dedication (Different View)

When Dedication selected, the entire dashboard swaps to corrupted gem exchange:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  🔮 Dedication Lab — Corrupted Gem Exchange                                 │
│                                                                              │
│  Input: corrupted gem → Output: random corrupted transfigured (same color)  │
│                                                                              │
│  🔴 RED    Pool: 33   EV: 333c   Cheapest input: 10c   Profit: 323c       │
│  🟢 GREEN  Pool: 45   EV: 220c   Cheapest input: 8c    Profit: 212c       │
│  🔵 BLUE   Pool: 52   EV: 180c   Cheapest input: 5c    Profit: 175c       │
│                                                                              │
│  ⚠ Low volume strategy — corrupted transfigured inputs are rare            │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Auto-Refresh Flow

1. Frontend connects to Mercure SSE at MERCURE_PUBLIC_URL
2. Subscribes to topic `poe/analysis/updated`
3. On event received: fetch all /api/analysis/* endpoints
4. Svelte reactivity updates DOM — no page reload
5. Update "Last update" timestamp and "Next expected" from event payload
6. If SSE disconnects: show red indicator, auto-reconnect

## Mercure Event Payload Enhancement

Collector adds `nextFetch` to publish payload:
```json
{"endpoint": "ninja_gems", "timestamp": "...", "inserted": 7117, "nextFetch": "18:45:16Z"}
```

Throttler aggregates and passes to frontend:
```json
{"type": "analysis-batch", "timestamp": "...", "nextAny": "18:45:16Z"}
```

## API Endpoints Used

| Endpoint | Section |
|----------|---------|
| GET /api/analysis/collective?variant=&budget=&sort= | Best Plays Now |
| GET /api/analysis/transfigure?variant=&limit= | By Variant best plays |
| GET /api/analysis/font?variant= | By Variant font EV |
| GET /api/analysis/quality?variant= | By Variant quality |
| GET /api/analysis/trends?variant=&signal=&window= | Window alerts, signals |
| GET /api/analysis/compare?gems=&variant= | Comparator |
| GET /api/analysis/gems/names?q= | Autocomplete |
| GET /api/analysis/status | Cache health, last update |
| GET /api/snapshots/stats | Market overview |

## Signal History

Endpoint: `GET /api/analysis/history?name=Spark+of+Nova&variant=20/20&limit=4`
Returns last N snapshots in DESC order with signal, window, advanced, priceVelocity, listingVelocity.

**Frontend computes transitions client-side:**
- Reverse array for chronological order
- Diff consecutive entries: `history[i].signal !== history[i+1].signal` = transition
- Derive "reason" text from velocity values using same threshold rules as legend
- Example: if priceVelocity > 5 → "(+Xc/h)", if listingVelocity > 10 → "(lst spike)"

## Audit Resolutions

### Fixed (backend updated):
- **Throttler nextAny** — throttler now includes `nextAny` from collector's `nextFetch`
- **Collective signals** — CollectiveResult now includes windowSignal, advancedSignal, liquidityTier
- **Quality roi20** — QualityResult now includes ROI20 for Uber/Gift 5th tier

### Frontend handles:
- **Signal history diffs** — client-side diff of consecutive snapshots, reasons derived from velocities
- **Market Overview** — aggregated client-side from trends data already in memory, no new endpoint
- **Δ2h display** — multiply velocity × 2 for 2-hour deltas
- **Token refresh** — on SSE disconnect, re-fetch /api/mercure/token before reconnecting
- **Signal names** — match backend strings exactly: DUMPING (not DUMP), PRICE_MANIPULATION, COMEBACK, POTENTIAL
- **Budget=0** — treat as "unlimited" (no filter), same as empty

### Deferred:
- **Dedication lab** — gate with "Coming soon" in v1. Separate task for corrupted gem analyzer.
- **Variant-filtered autocomplete** — nice-to-have, not blocking
- **Collector uptime** — derive from gemFirstSnapshot in stats endpoint
- **Font EV Δ2h** — client-side delta tracking between updates
- **WindowScore progress bar** — data available (0-100), add as visual enhancement
- **Twice Blessed tooltip** — show "with ~33% shrine chance: ×N+1 uses" on Font EV

### Performance strategy:
- Initial load: ~18 API calls + N signal history calls
- Signal history: load lazily on row expand (not all at once)
- Cache-first: all analysis endpoints serve from memory, no DB contention
- On Mercure event: re-fetch only changed endpoints, not all

## Tooltips — Detailed Signal Descriptions

Every signal badge, metric label, and liquidity tier shows a tooltip on hover with:
1. What it means (plain English)
2. What to do (actionable advice)
3. What triggered it (data — for advanced users)

All times shown in user's local timezone (browser locale), no UTC suffix.

### Signal Tooltips

| Signal | Tooltip |
|--------|---------|
| ▲ STABLE | "Price and listings are steady. Safe to farm — predictable returns. Triggered: price velocity < ±2c/h, listing velocity < ±3/h" |
| ▲ RISING | "Price is increasing. Good entry point if CV is low. Triggered: price velocity > 5c/h" |
| ▼ FALLING | "Price is decreasing. Wait for stabilization before farming. Triggered: price velocity < -5c/h" |
| ⚠ HERD | "Both price AND listings are rising. Multiple farmers flooding the market. Sell now if you have stock. Don't start farming — you're late. Triggered: price velocity > 5, listing velocity > 10" |
| ⚠ DUMP | "Price dropping while listings rise. Sellers undercutting each other. Avoid — will keep falling. Triggered: price velocity < -5, listing velocity > 5" |
| ↻ RECOVERY | "Price and listings both dropping. Supply drying up — potential comeback. Watch for COMEBACK signal. Triggered: price velocity < -5, listing velocity < -5" |
| ⚠ TRAP | "Extreme volatility — this gem's price swings wildly. Never farm regardless of current ROI. Triggered: CV > 100%" |

### Window Tooltips

| Window | Tooltip |
|--------|---------|
| CLOSED | "No farming opportunity detected. Base gems available but no special conditions." |
| BREWING | "Opportunity forming! Price rising + trans listings falling + bases still available. Window may open in ~2 hours. Start planning your lab run. Triggered: price velocity > 0, listing velocity < 0, bases > 10" |
| OPENING | "Base gems starting to drain. Window score is moderate. Prepare to act soon. Triggered: window score ≥ 50, base velocity < 0" |
| OPEN | "Farm NOW! High ROI, low trans listings, bases draining fast. This window lasts 1-2 hours typically. Triggered: window score ≥ 70, base velocity < -2" |
| CLOSING | "Herd arriving — other farmers' transfigured gems hitting the market. Sell immediately if you have stock. Triggered: trans listing velocity > 3" |
| EXHAUSTED | "No base gems available on market. Unfarmable until bases reappear. Triggered: base listings ≤ 2" |

### Advanced Signal Tooltips

| Signal | Tooltip |
|--------|---------|
| 🔄 COMEBACK | "Was in the top gems previously, crashed, now showing recovery. Lower herd risk since it's no longer on poe.ninja's front page. Good for experienced farmers. Triggered: hist position < 30%, price rising, listings dropping" |
| 💎 POTENTIAL | "Rising ROI that hasn't been widely noticed yet. Low competition, moderate price, rising trend. Best opportunity for experienced players who want low-herd-risk plays. Triggered: price 30-200c, < 40 listings, price rising, below historical midpoint" |
| ⚠ MANIPULATION | "Suspicious pricing. Very few listings at high price with no movement. Likely someone trying to set a fake price floor. Avoid. Triggered: ≤ 3 listings, price > 200c, no velocity, high CV" |

### Metric Tooltips

| Metric | Tooltip |
|--------|---------|
| ROI | "Absolute profit in chaos orbs. Transfigured gem price minus base gem price. Higher = more profit per transfigure." |
| ROI% | "Return on investment as percentage. ROI divided by base price × 100. Better for comparing across price tiers. A 20c gem with 200% ROI is better for small budgets than a 200c gem with 50% ROI." |
| CV | "Coefficient of Variation — how predictable the price is. Lower = more stable. Under 25% is safe, 25-50% is moderate, over 100% is a trap. Calculated from price standard deviation over 7 days." |
| EV | "Expected Value from using Font of Divine Skill. Probability of hitting a profitable gem × average winner price. Higher EV = better font usage." |
| pWin | "Probability of getting at least one winner when the font picks 3 random gems from the color pool. Uses hypergeometric distribution. Higher = better odds." |
| Pool | "Number of unique transfigured gems of this color. Smaller pool = better odds of hitting a specific winner. RED typically has smallest pool." |
| Liq | "Base gem liquidity relative to market average. HIGH (≥80% of avg) = herd gets absorbed, safe. MED (30-80%) = windows open and close. LOW (<30%) = bases drain instantly, short windows. Auto-adjusts for weekend/weekday and league phase." |
| Δ2h | "Change over the last 2 hours (4 data snapshots at ~30min intervals). Shows recent momentum. ↑ = increasing, ↓ = decreasing." |

## Design Tokens (Dark Mode)

- Background: #0f1117 (near-black)
- Surface: #1a1d27 (card background)
- Border: #2a2d37
- Text primary: #e4e4e7
- Text secondary: #9ca3af
- Accent green (STABLE/RISING): #22c55e
- Accent red (DUMP/TRAP): #ef4444
- Accent yellow (HERD/CAUTION): #eab308
- Accent blue (BREWING): #3b82f6
- Accent purple (RECOVERY): #a855f7
