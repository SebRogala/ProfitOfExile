# ProfitOfExile — BACKBONE

> Design document and source of truth. Full detail, no simplifications.
> This document captures the complete vision, architecture, and rationale for the project.

---

## 1. What Is This

ProfitOfExile is a **Path of Exile 1 profit simulation platform**. It models farming strategies as composable trees of activities, fetches live prices from multiple market sources, simulates inventory flows with automatic intermediate-to-set conversions, and calculates profitability per strategy — including optimal buy/sell source selection across markets.

It is NOT a simple price checker or a static spreadsheet. It is a **supply chain simulator** where:
- Outputs of one activity become inputs to another
- Intermediate products auto-convert (4 fragments → 1 set, 10 splinters → 1 writ)
- The system determines whether to sell intermediates or chain them into the next step
- Prices are live and source-aware (trade vs bulk vs TFT)

### Origin

The project started as a PHP/Symfony + Vue 3 application (original codebase preserved in git history). The core concepts — inventory-driven cascade, recursive tree composition, asymmetric buy/sell sources — were architecturally sound. The rewrite ports these proven patterns to Go + SvelteKit, finishes unimplemented features, and adds capabilities that were always planned.

---

## 2. Core Concepts

### 2.1 Strategy Tree (Composable Activities)

Every farming activity is a **Strategy** — an atomic unit that:
- Consumes specific items (required components)
- Produces specific items (rewards) with probability
- Takes a known average time

Strategies are composed into **trees**. A parent node wraps children and executes them N times (the `series` count) before running itself. This models real PoE farming chains:

```
Root (series: 1)
├── Run Shaper Guardian Map (series: 4)    → produces 4 ShaperGuardianFragments
├── Run The Formed (series: 1)             → consumes TheFormed + 15 Scouring, produces 6 MavenSplinters + fragments
└── Run Shaper (series: 1)                 → consumes 1 ShaperSet (auto-converted from 4 fragments), produces drops
```

The **Runner** executes this tree recursively: for each node, run all children `series` times, then run the node itself. Siblings execute left-to-right.

A **Wrapper** is a no-op strategy used purely as a grouping node — it has no inputs/outputs/time but allows nesting children under a single repeatable parent.

### 2.2 Inventory-Driven Cascade

All strategies share a single **Inventory** object during simulation. This is the mechanism that enables cascading without strategies knowing about each other:

1. `RunShaperGuardianMap` adds a `ShaperGuardianFragment` to inventory
2. On every `add()`, the inventory's **SetConverter** checks conversion rules
3. After 4 fragments accumulate, they auto-convert to 1 `ShaperSet`
4. Later, `RunShaper` requires a `ShaperSet` — it's already there, no purchase needed
5. If inventory is short on any required item, **auto-buy** triggers immediately

Conversion rules are data-driven, not hardcoded per item:

```
4x ShaperGuardianFragment   → 1 ShaperSet
4x ElderGuardianFragment     → 1 ElderSet
10x MavenSplinter            → 1 MavensWrit
2x UberElderShaperFragment + 2x UberElderElderFragment → 1 UberElderSet
10x CrescentSplinter         → 1 MavensWrit (alternative path)
```

New conversion rules can be added without code changes — they are configuration.

### 2.3 Multi-Source Price Engine

The platform fetches prices from multiple market sources and maintains a **price book** where each item has a price per source:

| Item | poe.ninja (trade) | TFT (bulk) | Faustus (future) |
|------|-------------------|------------|-------------------|
| ShaperGuardianFragment | 14.2c | — | — |
| ShaperSet | 56.8c (sum of 4 frags) | 38.0c | — |
| Yellow Lifeforce | 0.082c | 0.067c/unit | — |

**Source characteristics:**
- **poe.ninja**: Individual trade prices. Best for buying singles. Uses `chaosEquivalent` for currency/fragments, `chaosValue` for items. Fragment set prices computed as sum of individual fragments.
- **TFT**: Bulk trade prices from `The-Forbidden-Trove/tft-data-prices` GitHub repo. Static JSON files per league (`/lsc/` = league softcore). Better rates for bulk operations. Lifeforce priced per 1000 units (must divide). Categories: `bulk-sets.json`, `bulk-invitation.json`, `bulk-maps.json`, `bulk-lifeforce.json`.
- **Faustus** (future): Primarily scarab/compass pricing. Not a general market API currently. Parked for when/if they open up broader access.

### 2.4 Asymmetric Buy/Sell Optimization

The real PoE economy has different optimal sources for buying vs selling:

- **Buying singles** → trade (poe.ninja prices) is usually cheaper
- **Selling bulk** → TFT bulk rates are usually better
- **Some items** only exist on one source

The original implementation used hardcoded priority (buy from ninja first, sell on TFT first). The new version must implement a **best-deal matrix**:

- For each item, show ALL source prices side by side
- Highlight the optimal buy source (lowest price) and optimal sell source (highest price)
- The simulation should use optimal sources by default but allow manual override ("I only trade on ninja" or "I have a TFT voucher for this")

The final results table must display:
- What was bought, from which source, at what price
- What was sold (or remains in inventory), valued at which source
- Per-item profit contribution
- Total profit with source breakdown

### 2.5 Market Intelligence — Price History, Saturation & Liquidity (NEW)

Raw price-based ROI is misleading without market context. A gem showing +263c profit is a trap if every farmer targets it and the price crashes before you sell. The system must track and surface:

**Price History & Snapshots:**
- Store timestamped price snapshots in PostgreSQL (gem prices, listing counts, computed ROIs)
- Snapshot frequency: at least every 2-4 hours during active league (configurable)
- poe.ninja provides daily historical data but that's too coarse — intraday resolution catches crashes that happen in hours
- Show price trend sparklines alongside ROI in strategy results

**Saturation Detection:**
- Track listing count changes over time per item
- **Saturation signal:** listing count rising + price falling = too many farmers, avoid
- **Scarcity signal:** listing count falling + price rising = genuine demand, safe target
- Flag items crossing saturation thresholds in strategy output (e.g., "⚠ Molten Strike: listings +300%, price -60% in 12h")

**Liquidity Scoring:**
- High listing count ≠ high liquidity. It can mean high **supply** (many sellers, slow to move)
- True liquidity = items that sell quickly at listed price
- Proxy metric: listing count stability (not growing = items are actually selling) + price stability
- Classify items: HIGH liquidity (stable listings, stable price), MEDIUM (growing listings, stable price), LOW/TRAP (growing listings, falling price)

**Competition Risk per Strategy:**
- EV-based strategies (Font, Emp/Enl/Enh exchange) have **diversified output** — no single item tanks the EV
- Specific transfigure strategies have **concentrated output** — one gem, one price, high competition risk
- The system should weight EV by competition risk: concentrated output with thin listings gets a penalty
- Input cost matters: cheap input (1-5c) = low downside if price crashes. Expensive input (50c+) = can lose money

**Risk-Adjusted ROI:**
- `adjustedROI = rawROI × liquidityScore × (1 - saturationPenalty)`
- The UI should show both raw and adjusted ROI, with clear visual distinction
- Default sort by adjusted ROI, option to sort by raw

### 2.6 Breakpoint Analysis (NEW — not in original)

Given a strategy tree, the system should automatically analyze decision points:

> "Is it more profitable to run 4 guardian maps and sell the fragments, or chain them into a Shaper run?"

This is computed by:
1. Running the simulation at each tree depth (stop after step 1, stop after step 2, etc.)
2. At each stop point, valuing remaining inventory at best sell price
3. Comparing total profit across all stop points
4. Presenting the optimal chain length and all alternatives

This answers the question the original version could never answer: **where in the chain should I stop?**

---

## 3. Data Sources — Technical Details

### 3.1 poe.ninja

**Endpoints used:**
- `https://poe.ninja/poe1/api/economy/stash/current/item/overview?league={league}&type=Currency`
- `https://poe.ninja/poe1/api/economy/stash/current/item/overview?league={league}&type=Fragment`
- `https://poe.ninja/poe1/api/economy/stash/current/item/overview?league={league}&type=Invitation`
- `https://poe.ninja/poe1/api/economy/stash/current/item/overview?league={league}&type=SkillGem`

**Key fields:**
- Currency/Fragment: `currencyTypeName`, `chaosEquivalent`, `receive.value`
- Items/Invitations/Gems: `name`, `chaosValue`, `listingCount`, `variant`, `corrupted`
- Gems: `tradeFilter.query.type.discriminator` (starts with `alt_` for transfigured)
- Item lookup: by `detailsId` (lowercase slug, e.g., `"fragment-of-the-hydra"`)
- Icon parsing: base64 segment in URL path contains JSON with `gd` field for gem color

**Caching:** TTL-based. Minimum 60 minutes between fetches. Force-refresh available.

### 3.2 TFT (The Forbidden Trove)

**Source:** Static JSON files on GitHub: `https://raw.githubusercontent.com/The-Forbidden-Trove/tft-data-prices/master/{league_code}/`

**League codes:** `lsc` (league softcore), `lhc` (league hardcore)

**Files:**
- `bulk-sets.json` — ShaperSet, ElderSet, UberElderSet, MavenWrit, SirusSet, 5-Way Set
- `bulk-invitation.json` — TheFormed, TheTwisted, etc.
- `bulk-maps.json` — ShaperMaps, ElderMaps
- `bulk-lifeforce.json` — Yellow/Blue/Purple lifeforce

**Schema (all files):**
```json
{
  "timestamp": "...",
  "data": [
    {
      "name": "Shaper Set",
      "divine": 0.15,
      "chaos": 38.0,
      "lowConfidence": false,
      "ratio": 1
    }
  ]
}
```

**Critical:** Lifeforce entries have `ratio: 1000` — the chaos price is per 1000 units. Must divide by ratio.

**Lookup:** by exact `name` match (e.g., `"Shaper Set"`, `"Vivid (Yellow)"`, `"The Formed"`).

### 3.3 Price Normalization

All sources are normalized into a unified price book. Each item maps to:

```
Item → {
  sourceA: { buy: float, sell: float },
  sourceB: { buy: float, sell: float },
  bestBuy: { source, price },
  bestSell: { source, price }
}
```

Special cases:
- **Aggregate items**: ShaperGuardianFragment price = average of Hydra/Phoenix/Minotaur/Chimera fragments ÷ 4
- **Set prices from singles**: ShaperSet ninja price = sum of all 4 individual fragment prices (what it costs to buy the set piece by piece on trade)
- **Set prices from bulk**: ShaperSet TFT price = direct bulk price (one transaction)

---

## 4. Strategy Types

### 4.1 Deterministic Strategies (Fixed Input/Output)

Standard boss runs, map completions, crafting recipes with known outcomes.

- Input: specific items consumed
- Output: specific items produced, each with a probability (0-100%)
- An `occurrenceProbability` on the strategy itself (e.g., Harvest has 90% chance of spawning)
- Quantities are fractional in simulation (0.9 fragments per run on average)

### 4.2 Probabilistic / EV-Based Strategies (NEW)

Font of Divine Skill, gambling, divination card turn-ins — outcomes drawn from a pool.

- Input: specific items consumed
- Output: random selection from a pool of possible items
- Each pool item has a probability (uniform for Font: 3 random from pool, pick 1)
- EV calculated across the full pool
- "Win rate" = probability of getting an item above threshold value

The Font farming strategy is the first of this type:
- Input: 1 gem (specific variant: 1/20, 20/20, etc.)
- Mechanic: Font offers 3 random same-color transfigured gems, player picks 1
- Color pools: RED, GREEN, BLUE (each with known set of transfigured gems)
- Analysis: P(at least 1 winner in 3 picks) × average winner value = EV
- The "winner threshold" is configurable (default: 3× input cost)

### 4.3 Currency Flip Strategies (FUTURE)

Buy currency/items at one price, sell at another. No transformation, pure arbitrage.
- Needs real-time or near-real-time price feeds
- Profit margins are thin — listing fees, time cost matter
- May need Faustus data for scarab flips

---

## 5. Sharing & Content Creator Model

### 5.1 Strategy Persistence

Strategies are stored in PostgreSQL as:
- JSON tree (the full strategy composition)
- Metadata: name, description, author (optional), league, created/updated timestamps
- Short ID for URL sharing (e.g., `abc123`)
- Edit token (secret, given to creator on save)

### 5.2 Sharing Flow

1. User builds strategy in the UI (drag/drop tree composer)
2. Clicks "Save & Share" → strategy saved to DB → gets two URLs:
   - **View URL**: `/s/abc123` — anyone can open, sees strategy with LIVE prices
   - **Edit URL**: `/s/abc123?edit=secret-token` — creator can modify
3. Creator shares the view URL (YouTube description, Discord, etc.)
4. Viewers open it days/weeks later → prices auto-update → they see current profitability
5. Creator can update the tree → same URL, new version

### 5.3 Content Creator Value Proposition

Content creators currently use static Excel screenshots in videos. ProfitOfExile replaces that with:
- A **live link** that updates prices automatically
- Viewers see **current league profitability**, not recording-day prices
- The strategy tree is interactive — viewers can tweak parameters (series count, input costs)
- Creators can maintain a portfolio of strategies across leagues

### 5.4 No Accounts (v1)

No user accounts in v1. Strategy ownership is via edit token only. If a creator loses their token, they can create a new copy. Account system can come later if there's demand.

---

## 6. User Profile (Local/Private)

A personal profile stored in the database (NOT in git) that helps the AI assistant brainstorm farming strategies and builds tailored to the user. Contains:

- Preferred playstyle (softcore/hardcore, trade/SSF)
- Current league and character info
- Budget range and risk tolerance
- Preferred content (bossing, mapping, farming, flipping)
- Build archetype preferences
- Historical strategy performance notes

This data is only used locally for AI-assisted brainstorming. It is never shared with other users or included in shared strategies.

---

## 7. Tech Stack

### Backend: Go
- Standard library HTTP server (Go 1.22+ routing) or chi router
- PostgreSQL via `pgx` (no ORM — direct queries)
- Strategy engine: pure Go structs and interfaces
- Price fetching: Go HTTP client with TTL caching in Postgres
- Static file serving: Go `embed` package bundles compiled frontend

### Frontend: SvelteKit
- SvelteKit for routing, SSR, component model
- Tailwind CSS for styling
- Strategy tree composer (recursive drag/drop component)
- Results tables with source comparison
- League selector dropdown

### Database: PostgreSQL
- Already running on CapRover
- Tables: strategies, price_cache, profiles, conversion_rules
- JSONB columns for strategy trees and price data
- No ORM — plain SQL queries

### Deployment: Docker on CapRover
- Multi-stage Dockerfile: Node build (SvelteKit) → Go build → minimal final image
- Single container, connects to existing Postgres
- Go binary serves everything

---

## 8. Feature Roadmap

### v1 — Foundation (launch-worthy)
- [ ] Go project scaffolding (cmd/server, internal packages)
- [ ] Datasource interface + poe.ninja client
- [ ] Datasource interface + TFT client
- [ ] Price normalization and unified price book
- [ ] Best-deal matrix (all source prices, optimal buy/sell highlighted)
- [ ] Strategy interface + concrete strategies (Shaper rotation as first)
- [ ] Inventory with auto-convert (SetConverter port)
- [ ] Recursive tree runner (simulation engine)
- [ ] Results output: per-strategy breakdown, auto-buy log, profit summary
- [ ] PostgreSQL schema + price caching
- [ ] REST API endpoints
- [ ] SvelteKit frontend: league selector, strategy composer, results display
- [ ] Strategy save/share (DB + short URL + edit token)
- [ ] Docker multi-stage build + CapRover deploy
- [ ] Port font farming strategy (probabilistic type)

### v2 — Analysis & Intelligence
- [ ] Breakpoint analyzer (sell-vs-chain optimization)
- [ ] Price history tracking and charts
- [ ] Market intelligence: saturation detection, liquidity scoring, competition risk (see §2.6)
- [ ] Strategy versioning (history of changes)
- [ ] User profile (DB-backed, AI brainstorming context)

### v3 — Expansion
- [ ] Currency flip strategies
- [ ] Additional datasources (Faustus if available)
- [ ] Community strategy gallery (browse others' shared strats)
- [ ] Account system (optional, for managing multiple strategies)
- [ ] Strategy templates (pre-built common rotations)
- [ ] Mobile-responsive UI

---

## 9. Original Architecture Reference

The original PHP/Symfony implementation is preserved in git history (commit `537e37e` and earlier). Key files for reference:

```
src/Domain/Strategy/Strategy.php              — core simulation loop (run method, lines 23-45)
src/Infrastructure/Strategy/Runner.php         — recursive tree executor
src/Infrastructure/Strategy/Factory.php        — strategy registry
src/Domain/Inventory/Inventory.php             — shared state + auto-buy
src/Domain/Inventory/SetConverter.php           — implicit cascade (fragment→set conversion)
src/Infrastructure/Market/Buyer.php            — buy-side price source selection
src/Infrastructure/Pricer/Pricer.php           — sell-side pricing + c/hr calculation
src/Application/Command/PriceRegistry/UpdateRegistryHandler.php — price normalization logic
src/Infrastructure/Http/PoeNinjaHttpClient.php — poe.ninja data fetch
src/Infrastructure/Http/TftHttpClient.php      — TFT data fetch (raw GitHub)
src/Domain/Strategy/RunTheFormed.php           — best example of composite input/output
assets/views/Compose/ManageStrategy.vue        — recursive UI tree structure
assets/views/Compose/Results.vue               — final output shape (only rendered summary, not full data)
```

### Gaps in original that must be addressed:
- UI never rendered per-strategy breakdown or auto-buy log (data existed in API response)
- No breakpoint analysis
- Hardcoded buy/sell source priority instead of computed optimum
- No strategy sharing/persistence (localStorage only)
- `Results.vue` only showed top-level summary, not the full detail
- `GrandStrategy` classes were dead code (replaced by dynamic Runner but never removed)
- `FullShaperInvitation.vue` was an empty stub

---

## 10. Naming & Identity

**Repository:** `ProfitOfExile` (existing GitHub repo: `SebRogala/ProfitOfExile`)
**Display name:** Profit of Exile (or ProfitOfExile)
**Domain:** TBD (can use CapRover subdomain initially)

---

*This document is the authoritative reference for the project's design. Implementation plans, task breakdowns, and technical decisions must align with what is described here. When in doubt, refer to BACKBONE.*
