# ProfitOfExile — Epics for YouTrack

> Seed document for creating YT epics. Each epic is described with full context
> so pipeforge breakdown can generate detailed tasks from them.

---

## Epic 1: Project Foundation & Go Scaffolding

**Summary:** Set up the Go project structure, module initialization, development tooling, and deployment pipeline.

**Scope:**
- Go module init with proper directory structure (`cmd/server`, `internal/` packages)
- PostgreSQL schema design and migrations (strategies, price_cache, conversion_rules tables)
- Database connection layer using `pgx` (no ORM, direct queries)
- Multi-stage Dockerfile: Node build stage (SvelteKit) → Go build stage → minimal runtime image
- CapRover deployment configuration (captain-definition, environment variables for Postgres connection)
- Basic health check endpoint
- `.gitignore`, `Makefile` with common targets (build, run, test, migrate)
- CLAUDE.md for the project with Go conventions, project structure, testing approach

**Dependencies:** None — this is the starting point.

**Acceptance:** `make build` produces a single binary, `make run` starts the server, Docker image builds and deploys to CapRover, connects to Postgres.

---

## Epic 2: Multi-Source Price Engine

**Summary:** Build the data fetching, normalization, and caching layer that pulls prices from multiple market sources and produces a unified price book.

**Scope:**
- `Datasource` interface: `Fetch(league string) → []RawPrice` — each source implements this
- **poe.ninja client:** fetch Currency, Fragment, Invitation, SkillGem endpoints. Parse `chaosEquivalent` (currency/fragments) and `chaosValue` (items). Handle `detailsId` lookups. Gem-specific fields: `variant`, `corrupted`, `tradeFilter.query.type.discriminator` for transfigured detection, icon base64 `gd` field for gem color.
- **TFT client:** fetch static JSONs from `The-Forbidden-Trove/tft-data-prices` GitHub repo. Handle league code mapping (`lsc`, `lhc`). Parse all four files: `bulk-sets.json`, `bulk-invitation.json`, `bulk-maps.json`, `bulk-lifeforce.json`. Handle lifeforce `ratio` field (divide chaos by ratio, typically 1000).
- **Price normalization:** merge all sources into unified price book. Each item maps to `{source → {buy, sell}}`. Compute aggregate items (ShaperGuardianFragment = avg of 4 individual fragments, ShaperSet ninja price = sum of 4 fragments). Track `bestBuy` and `bestSell` per item across sources.
- **Best-deal matrix:** for any item, return all source prices side by side with optimal buy/sell highlighted.
- **Caching in Postgres:** store fetched prices with timestamps. TTL-based refresh (configurable, default 60 min). Force-refresh endpoint. Price history retained (not just latest — enables future charting).
- REST endpoints: `GET /api/prices?league=X` (full price book), `GET /api/prices/refresh` (force fetch)

**Dependencies:** Epic 1 (project structure, DB connection)

**Acceptance:** API returns normalized prices from both sources merged, with best-deal flags. Prices cached in Postgres with TTL. Second request within TTL returns cached data. Force refresh works.

---

## Epic 3: Simulation Engine

**Summary:** Port and improve the core simulation engine — strategy interface, inventory with auto-convert cascade, recursive tree runner, and results calculation.

**Scope:**

### Strategy Interface
- Go interface: `Name()`, `RequiredItems() []ItemStack`, `Rewards() []Reward`, `AverageTime() time.Duration`
- `Reward` struct: item, quantity, probability (0-100)
- `occurrenceProbability` on strategy itself (e.g., Harvest 90% spawn chance)
- Strategy registry: map of strategy keys to constructors

### Inventory
- Shared mutable inventory tracking item quantities (float64 for fractional simulation)
- `Add(item, qty)` — adds items AND triggers conversion check
- `Remove(item, qty)` — removes items, returns deficit if insufficient
- Auto-buy integration: when `Remove` has deficit, trigger buy from price book using optimal source
- Buy log: track every auto-purchase (item, qty, source, unit price, total cost)

### SetConverter (Cascade)
- Data-driven conversion rules (not hardcoded per item type)
- Rules stored in config/DB: `{inputs: [{item, qty}], output: {item, qty}}`
- Greedy conversion on every `Add()` — convert as many as possible
- Multi-input conversions supported (UberElderSet needs 2 shaper + 2 elder fragments)
- Initial rules: ShaperSet, ElderSet, MavensWrit, UberElderSet (from original codebase)

### Runner
- Recursive tree executor: takes strategy tree (JSON) + inventory + price book
- For each node: execute children `series` times, then execute self
- Wrapper/group nodes: no-op strategy, purely for nesting
- Collect per-strategy timing, expense, and reward data

### Results
- Per-strategy breakdown: time spent, items consumed (with source + price), items produced (with value)
- Inventory summary: all remaining items valued at best sell price per source
- Auto-buy log: everything purchased, from which source, at what price
- Totals: total time, total expenses, total revenue, profit, chaos/hour, divine/hour
- All data returned in API response (unlike original where UI ignored half the data)

**Dependencies:** Epic 2 (price book for auto-buy and valuation)

**Acceptance:** Can define a Shaper rotation as a strategy tree, run simulation, get full breakdown with correct cascade (fragments → set), auto-buy from cheapest source, accurate profit calculation matching manual spreadsheet verification.

---

## Epic 4: Concrete Strategies — Boss Rotations

**Summary:** Implement the first set of concrete strategies covering the Shaper/Elder/Maven boss rotation chain from the original ProfitOfExile.

**Scope:**
- `RunShaperGuardianMap` — consumes 1 ShaperGuardianMap, produces 1 ShaperGuardianFragment, 150s
- `RunElderGuardianMap` — consumes 1 ElderGuardianMap, produces 1 ElderGuardianFragment, 150s
- `RunTheFormed` — consumes 1 TheFormed + 15 OrbOfScouring, produces 6 MavenSplinter + 1 ShaperGuardianFragment + 1 ShaperGuardianMap, 140s
- `RunTheTwisted` — consumes 1 TheTwisted + 15 OrbOfScouring, produces 6 MavenSplinter + 2 ElderGuardianFragment, 140s
- `RunShaper` — consumes 1 ShaperSet, produces 1 UberElderShaperFragment, 480s
- `RunSimpleHarvest` — consumes nothing, produces 250 Yellow + 150 Blue + 150 Purple Lifeforce, 120s, 90% occurrence probability
- `Wrapper` — no-op grouping node
- Item definitions for all items referenced above (fragments, sets, currency, lifeforce)
- Reward probabilities for boss-specific drops (unique items, etc.)
- Pre-built strategy tree templates: "Full Shaper Rotation", "Guardian Farm Only", "Maven Witness Chain"

**Dependencies:** Epic 3 (simulation engine)

**Acceptance:** All strategies from original codebase ported. Pre-built templates produce correct results. Can compose custom trees from these building blocks via API.

---

## Epic 5: Font Farming Strategy (Probabilistic Type)

**Summary:** Port the font analysis script into the simulation engine as a new probabilistic/EV-based strategy type, extending the engine to handle pool-based random outcomes.

**Scope:**
- New strategy type: `ProbabilisticStrategy` — instead of fixed rewards, draws from a pool
- Font-specific implementation:
  - Input: 1 gem (variant-aware: 1/20, 20/20, etc.) with configurable input cost
  - Mechanic: 3 random gems drawn from same-color pool, player picks best
  - Color pools: RED, GREEN, BLUE — populated from poe.ninja transfigured gem data
  - Pool detection: gem color from icon base64 `gd` field (5/6=RED, 9/10=GREEN, 13/14=BLUE), fallback to manual color map, fallback to RePoE-derived map
  - Win probability: `P(at least 1 winner in 3 picks from pool)` using hypergeometric formula
  - EV: `P(win) × average winner value`
  - Configurable win threshold (default: 3× input cost)
  - Min-listings filter for confidence
- Port gem color data: `gem-colors.json` (RePoE-derived) + `gem-colors-manual.json` (manual overrides for 3.25-3.28 new gems)
- Color comparison mode: show RED vs GREEN vs BLUE side by side with EV, win rate, profit
- Variant comparison mode: show 1/20 vs 20/20 side by side
- Integration with simulation engine: Font can be a node in a strategy tree (e.g., farm maps → collect gems → run Font)

**Dependencies:** Epic 3 (simulation engine), Epic 2 (gem prices from poe.ninja)

**Acceptance:** Font strategy produces same results as current CLI script. Can be composed into larger strategy trees. Color and variant comparison available via API.

---

## Epic 6: SvelteKit Frontend

**Summary:** Build the web frontend using SvelteKit + Tailwind CSS. Strategy tree composer, results display, league selector, and price explorer.

**Scope:**

### Core Layout
- App shell: navigation, league selector (dropdown, persisted), dark/light theme
- Responsive design (usable on mobile for viewing shared strategies, desktop for composing)

### Strategy Composer
- Recursive tree UI for building strategy chains (port concept from original Vue `ManageStrategy.vue`)
- Drag/drop or add/remove nodes
- Each node: strategy type selector, series count input, collapse/expand children
- Wrapper nodes for grouping
- Parameter overrides per node (input cost, probability adjustments)
- "Run Simulation" button → calls API → displays results

### Results Display
- Summary bar: total time, total cost, total revenue, profit, c/hr, div/hr
- Per-strategy breakdown table: time, expenses (item + source + price), rewards (item + value)
- Auto-buy log: what was purchased, from where, at what cost
- Inventory table: remaining items, valued at each source, best-sell highlighted
- Best-deal matrix view: for any item, all source prices side by side

### Font Farming View
- Dedicated view for Font analysis (simpler than full tree composer)
- Color comparison table (RED/GREEN/BLUE)
- Variant toggle (1/20 vs 20/20)
- Top gems per color list with value, listings, win status
- Parameter controls: input cost, threshold, min listings

### Price Explorer
- Browse current prices from all sources
- Search/filter by item name
- Source comparison per item

### Go Integration
- SvelteKit builds to static files
- Go backend serves them via `embed` package
- API calls to Go backend for all data
- SvelteKit adapter-static (no Node runtime needed in production)

**Dependencies:** Epic 3 (API endpoints for simulation), Epic 2 (API endpoints for prices), Epic 5 (Font strategy API)

**Acceptance:** Full strategy composition, simulation, and results display working in browser. Font farming analysis functional. Deployed via same Docker image as Go backend.

---

## Epic 7: Strategy Sharing & Persistence

**Summary:** Save strategies to PostgreSQL and share via short URLs with live-updating prices.

**Scope:**
- DB schema: `strategies` table with JSONB tree, metadata (name, description, league, timestamps), short ID, edit token
- Short ID generation: URL-safe, collision-resistant (nanoid or similar)
- Edit token: generated on creation, returned to creator, required for modifications
- REST endpoints:
  - `POST /api/strategies` → create, returns `{id, editToken, viewUrl, editUrl}`
  - `GET /api/strategies/:id` → return strategy tree + metadata (public)
  - `PUT /api/strategies/:id` → update (requires edit token in header)
  - `DELETE /api/strategies/:id` → delete (requires edit token)
- View page: `/s/:id` — loads strategy, fetches LIVE prices, runs simulation, shows results
- Edit mode: `/s/:id?edit=token` — same as view but with composer UI enabled
- Strategy metadata editing: name, description, notes
- No user accounts — ownership via edit token only
- Copy/fork: anyone can duplicate a shared strategy as their own (gets new ID + token)

**Dependencies:** Epic 6 (frontend for composition and display), Epic 1 (Postgres)

**Acceptance:** Create strategy → get shareable URL → open in new browser → see live results. Edit with token works. Fork works. Opening a shared link days later shows updated prices.

---

## Epic 8: Breakpoint Analyzer

**Summary:** Automatically analyze strategy trees to find optimal stop points — where in the chain should you sell intermediates vs continue to the next step.

**Scope:**
- Given a strategy tree, simulate at each depth level:
  - Stop after step 1: value remaining inventory
  - Stop after step 2: value remaining inventory
  - ... continue for all steps
- At each stop point, compute: total cost so far, inventory value (at best sell price), profit, c/hr
- Compare all stop points: find the optimal chain length
- Handle branching: when a tree has parallel branches, analyze combinations
- Present results as a table: step → cost → inventory value → profit → c/hr → verdict
- Highlight the most profitable stop point
- Show "chain premium" — how much extra profit (or loss) each additional step adds
- API endpoint: `POST /api/analyze/breakpoints` with strategy tree input
- Frontend: breakpoint analysis tab in results view, visual chart of profit curve by step

**Dependencies:** Epic 3 (simulation engine), Epic 2 (price book)

**Acceptance:** Shaper rotation breakpoint analysis correctly identifies whether selling fragments is more profitable than running Shaper. Results match manual calculation.

---

## Epic 9: User Profile (Local/AI-Assisted)

**Summary:** Database-backed personal profile for AI-assisted strategy brainstorming. Private, not shared, not in git.

**Scope:**
- DB schema: `profiles` table (single row for now — no multi-user)
- Profile fields:
  - Playstyle: softcore/hardcore, trade/SSF
  - Current league and character info (class, build, level, budget)
  - Preferred content: bossing, mapping, farming, flipping, lab running
  - Risk tolerance: conservative / moderate / aggressive
  - Build archetype preferences
  - Notes: freeform text for AI context
  - Historical performance notes per strategy
- REST endpoints: `GET /api/profile`, `PUT /api/profile`
- Frontend: profile settings page
- Integration point: when AI is used for brainstorming (future), profile is included as context
- Privacy: profile data never included in shared strategies, never sent to external APIs

**Dependencies:** Epic 1 (Postgres)

**Acceptance:** Profile can be created, edited, persisted across sessions. Data stays in DB only.

---

## Epic Dependency Graph

```
Epic 1 (Foundation)
├── Epic 2 (Price Engine)
│   ├── Epic 3 (Simulation Engine)
│   │   ├── Epic 4 (Boss Strategies)
│   │   ├── Epic 5 (Font Strategy)
│   │   └── Epic 8 (Breakpoint Analyzer)
│   └── Epic 6 (Frontend)
│       └── Epic 7 (Sharing)
└── Epic 9 (Profile)
```

**Critical path:** 1 → 2 → 3 → 4 (minimum viable simulation)
**Frontend can start in parallel** with Epic 3 using mocked API responses.

---

*This document feeds into YouTrack via pipeforge breakdown. Each epic should become a YT epic with subtasks generated from the scope details above.*
