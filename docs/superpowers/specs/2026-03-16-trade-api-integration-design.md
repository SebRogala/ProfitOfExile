# Trade API Integration — Design Spec

## Problem

Our analysis relies entirely on poe.ninja aggregate prices — averages that hide real market dynamics. We have no visibility into:
- Actual listing counts per variant (real liquidity)
- Price floors vs averages (real cost to buy)
- Price spread across listings (market depth)
- Seller concentration (manipulation / dumping detection)
- How stale the cheapest listings are

## Solution

Integrate the PoE trade search API into the server to provide real listing data as a signal layer on top of poe.ninja aggregates. Start with the font comparator (interactive, per-gem lookups), then extend to periodic background scans.

## Constraints

- **CORS blocks browser-side requests** — all trade API calls must go through our Go server
- **Shared VPS IP** — all users share one rate limit budget, practical ceiling of 2-3 concurrent users
- **Rate limits are dynamic** — must parse `X-Rate-Limit-*` headers on every response, never hardcode
- **Multiple overlapping rate windows** — GGG enforces tiers like `8:10s, 15:60s, 30:300s` simultaneously
- **4xx errors count toward bans** — must prevent 429s proactively, not just react to them
- **Trade search is an internal website API** — not officially supported in GGG developer docs, but widely used by community tools
- **Future: browser extension** will offload interactive requests to user IPs (Task #4)

## Architecture

### Package Layout

```
internal/trade/
  client.go       — HTTP client, header parsing, request execution
  ratelimiter.go  — sliding window rate limiter with server state sync
  gate.go         — priority queue + rate limit gating + Mercure events
  cache.go        — in-memory LRU result cache
  types.go        — request/response types

internal/server/handlers/
  trade.go        — HTTP handler for /api/trade/lookup (follows existing handler convention)
```

### Request Flow

```
Frontend (Comparator)
  POST /api/trade/lookup { gem: "Spark of Nova", variant: "20/20" }

Go Server (handlers/trade.go)
  1. Check cache -> if hit, return 200 with data immediately
  2. Check Gate for in-flight dedup -> if same gem already in progress, attach to existing
  3. Submit to Gate with per-request result channel
  4. Block with 500ms sync timeout:
     - If result arrives within 500ms -> return 200 { data }
     - If timeout -> return 202 { requestId }, Gate will deliver via Mercure

Gate (single goroutine)
  1. Dequeue next request (HIGH priority first)
  2. Re-check EstimateWait across both search AND fetch pools
  3. If wait needed -> publish Mercure wait event -> sleep
  4. Execute two-phase lookup (search + fetch as atomic unit)
  5. Parse rate limit headers on both responses, sync state
  6. Cache result
  7. Deliver via result channel (fast path) AND Mercure (wait path)
```

### Fast Path vs Wait Path

**Cache hit:**
```
POST /api/trade/lookup -> cache hit
  -> returns 200 { data } immediately
  -> no Gate involvement, no Mercure, no GGG API call
```

**Fast path (budget available, GGG responds within 500ms):**
```
POST /api/trade/lookup -> cache miss -> Gate has budget
  -> handler blocks on result channel (500ms deadline)
  -> Gate fires search+fetch -> result arrives in ~200-400ms
  -> handler returns 200 { data }
  -> no Mercure events needed
```

**Wait path (rate limited or slow):**
```
POST /api/trade/lookup -> cache miss -> Gate must wait (or 500ms deadline expires)
  -> handler returns 202 { requestId }
  -> frontend subscribes to Mercure topic, shows spinner
  -> Gate publishes Mercure: { type: "waiting", gem, waitSeconds: 4 }
  -> card shows spinner + "waiting 4s"
  -> Gate fires request when budget available
  -> Gate publishes Mercure: { type: "ready", requestId, data }
  -> card renders trade data
```

### Handler ↔ Gate Communication

Each request submitted to the Gate carries a result channel:

```go
type GateRequest struct {
    Gem       string
    Variant   string
    RequestID string
    Priority  Priority  // HIGH or LOW
    Result    chan *GateResponse
}

type GateResponse struct {
    Data  *TradeLookupResult
    Error error
}
```

The handler creates the channel, submits to Gate, then does a `select` with 500ms timeout:

```go
select {
case res := <-req.Result:
    // fast path: return 200
case <-time.After(500 * time.Millisecond):
    // wait path: return 202, Gate will deliver via Mercure
}
```

The Gate always sends to the result channel AND publishes Mercure (for the wait path case where the handler has already disconnected from the channel).

### Request Deduplication

The Gate maintains an in-flight map: `map[string][]*GateRequest` keyed by `"gem|variant"`.

When a new request arrives:
1. If the same key is already in-flight, append the new request's result channel to the fan-out list
2. When the in-flight request completes, deliver to ALL attached result channels
3. Remove from in-flight map

This prevents wasting rate limit budget when multiple users (or rapid selections) request the same gem simultaneously.

## Rate Limiter

Adapted from Awakened PoE Trade's pattern for server-side use.

### Sliding Window with Multi-Tier Support

```go
type RateLimiter struct {
    mu       sync.Mutex
    pools    map[string]*Pool  // "search", "fetch" — separate GGG policies
}

type Pool struct {
    tiers    []Tier
    padding  time.Duration  // latency compensation (DESYNC_FIX)
    ceiling  float64        // self-imposed ceiling, e.g. 0.65 = use 65% of budget
}

type Tier struct {
    maxHits  int
    window   time.Duration
    penalty  time.Duration
    slots    []time.Time     // sliding window of request timestamps
}
```

### Server State Sync

On every GGG response:
1. Parse `X-Rate-Limit-Rules`, `X-Rate-Limit-{Rule}`, `X-Rate-Limit-{Rule}-State`
2. For each tier: compare server's `currentHits` with our tracked slots
3. If server shows MORE hits than us (other consumers on same IP) — inject phantom slots to match
4. If server reports new/changed tiers — create/update tier definitions
5. Apply `ceiling` factor — if tier says max 10, we treat it as max 6-7

### EstimateWait

Calculates when the next request can safely fire:

1. Purge expired slots: remove any `slot` where `slot.Add(tier.window) < time.Now()`
2. For each tier: if `len(activeSlots) >= int(maxHits * ceiling)`:
   - `wait = slots[0].Add(tier.window + padding) - time.Now()` (when oldest active slot exits the window)
3. Return `max(wait)` across all tiers in the pool
4. **Must be called twice**: once at dequeue time (to publish wait event with accurate seconds), and again immediately before firing (to account for intervening requests from other sources)

### Conservative Defaults

On startup (before any GGG response):
- Search pool: 1 request per 5 seconds
- Fetch pool: 1 request per 2 seconds
- After first successful response, real tiers replace defaults

## Priority Gate

```go
type Gate struct {
    high      chan *GateRequest  // interactive lookups (buffered: 10)
    low       chan *GateRequest  // background scans (buffered: 50)
    limiter   *RateLimiter
    mercure   *MercurePublisher
    inflight  map[string][]*GateRequest  // dedup: "gem|variant" -> fan-out
    mu        sync.Mutex
    maxWait   time.Duration  // 30s — max time a request can wait in queue
}
```

Single goroutine loop:
```go
for {
    select {
    case req := <-g.high:
        g.process(req)
    default:
        select {
        case req := <-g.high:
            g.process(req)
        case req := <-g.low:
            g.process(req)
        }
    }
}
```

HIGH always drains before LOW. The nested select with default ensures HIGH pre-empts LOW.

### Two-Phase Atomic Lookup

`gate.process()` executes search and fetch as a single atomic unit:

```go
func (g *Gate) process(req *GateRequest) {
    // 1. Check queue age — if request has been waiting > maxWait, send error
    if time.Since(req.SubmittedAt) > g.maxWait {
        g.publishError(req, "Trade API temporarily unavailable")
        return
    }

    // 2. EstimateWait for BOTH pools (search + fetch)
    searchWait := g.limiter.EstimateWait("search")
    fetchWait := g.limiter.EstimateWait("fetch")
    totalWait := max(searchWait, fetchWait)

    // 3. If wait needed, publish Mercure event and sleep
    if totalWait > 0 {
        g.publishWait(req, totalWait)
        time.Sleep(totalWait)
    }

    // 4. Re-check before firing (budget may have changed)
    if recheck := g.limiter.EstimateWait("search"); recheck > 0 {
        g.publishWait(req, recheck)
        time.Sleep(recheck)
    }

    // 5. Fire search
    searchResult, err := g.client.Search(req.Gem, req.Variant)
    // ... handle error, sync rate limiter from response headers

    // 6. Check fetch pool before firing fetch
    if fetchWait := g.limiter.EstimateWait("fetch"); fetchWait > 0 {
        time.Sleep(fetchWait)
    }

    // 7. Fire fetch (top 10 IDs)
    listings, err := g.client.Fetch(searchResult.QueryID, searchResult.IDs[:10])
    // ... handle error, sync rate limiter from response headers

    // 8. Build result, cache it, deliver to all attached channels + Mercure
    result := buildResult(req, searchResult, listings)
    g.cache.Set(req.cacheKey(), result)
    g.deliverResult(req, result)
}
```

### Queue Timeout + Error Events

If a request has been in the queue longer than `maxWait` (30s default):
- Publish Mercure error event: `{ "type": "error", "requestId": "abc123", "message": "Trade API temporarily unavailable" }`
- Send error on result channel
- Remove from in-flight map

This prevents indefinite spinner states.

## Trade API Queries

### Per-Gem Search

```json
POST https://www.pathofexile.com/api/trade/search/Mirage
{
  "query": {
    "type": "Skill Gem",
    "term": "Spark of Nova",
    "stats": [{ "type": "and", "filters": [] }],
    "filters": {
      "trade_filters": {
        "filters": {
          "sale_type": { "option": "priced" },
          "collapse": { "option": "true" }
        }
      }
    },
    "status": { "option": "any" }
  },
  "sort": { "price": "asc" }
}
```

Notes:
- `sale_type: "priced"` = **instant buyout only** — excludes negotiable listings, giving us clean price signals.
- `collapse: true` = **one listing per account** — eliminates spam/manipulation noise. Top 10 results = 10 different sellers by definition. Seller concentration signals on collapsed data reflect genuine market thinness, not one person flooding.
- `status: "any"` includes offline sellers intentionally — we want all listings for price discovery, not just currently-buyable ones. The price floor from offline sellers is still a valid market signal.

Response gives us:
- `total` — listing count (liquidity signal)
- `result[]` — up to 10,000 hash IDs sorted by price

### Fetch Top Listings

```
GET https://www.pathofexile.com/api/trade/fetch/{first-10-ids}?query={queryId}
```

From each listing we extract:
- `listing.price` — exact price in chaos/divine
- `listing.account.name` — seller account (manipulation detection)
- `listing.indexed` — when it was listed (staleness)
- `item.properties` — gem level, quality
- `item.corrupted` — corruption status
- `item.typeLine` — exact gem name/variant

### Requests Per Gem Variant: 2

1. `POST /api/trade/search` — get total + IDs (uses search pool)
2. `GET /api/trade/fetch` — get top 10 listing details (uses fetch pool)

## Data Extracted Per Variant

```go
type TradeLookupResult struct {
    Gem           string
    Variant       string
    Total         int              // total listing count
    PriceFloor    float64          // cheapest listing
    PriceCeiling  float64          // 10th cheapest listing
    PriceSpread   float64          // ceiling - floor
    MedianTop10   float64          // median of top 10
    Listings      []TradeListingDetail
    Signals       TradeSignals
    FetchedAt     time.Time
}

type TradeListingDetail struct {
    Price       float64
    Currency    string            // "chaos" or "divine"
    Account     string            // seller account name
    IndexedAt   time.Time         // when listed
    GemLevel    int
    GemQuality  int
    Corrupted   bool
}

type TradeSignals struct {
    SellerConcentration string   // "NORMAL", "CONCENTRATED", "MONOPOLY"
    CheapestStaleness   string   // "FRESH" (<1h), "AGING" (1-6h), "STALE" (>6h)
    PriceOutlier        bool     // cheapest < 50% of median top 10
    UniqueAccounts      int      // distinct sellers in top 10
}
```

### Derived Signals

From the raw data we compute:
- **Liquidity** — `Total` listing count, much more granular than poe.ninja's single number
- **Real price floor** — what it actually costs to buy right now
- **Spread** — tight spread = stable price, wide spread = volatile/thin market
- **Seller concentration**:
  - `UniqueAccounts >= 8` → "NORMAL"
  - `UniqueAccounts >= 5` → "CONCENTRATED" (worth noting)
  - `UniqueAccounts < 5` → "MONOPOLY" (strong manipulation signal)
- **Staleness**:
  - Cheapest listing indexed < 1h ago → "FRESH"
  - 1-6h → "AGING"
  - \> 6h → "STALE" (price may be outdated)
- **Price outlier** — cheapest listing price < 50% of median top 10 → possible bait / price fix

## Cache

### LRU with Max Entries

```go
type TradeCache struct {
    mu       sync.RWMutex
    entries  map[string]*CacheEntry  // key: "gem_name|variant"
    order    []string                // LRU eviction order
    maxSize  int                     // default: 200 entries
}

type CacheEntry struct {
    Result    *TradeLookupResult
    CreatedAt time.Time
}
```

- **Max 200 entries** — evicts oldest on overflow (LRU)
- Compare endpoint reads from cache to score all selected gems
- Frontend "refresh prices" button triggers re-fetch (bypasses cache)
- Cache hit = immediate 200, no Gate involvement

### Cache-First Flow

1. Handler receives lookup request
2. Check cache — if hit, return 200 immediately
3. If miss, check Gate for in-flight dedup — if same gem in progress, attach to result
4. If truly new, submit to Gate
5. On result, store in cache + return/deliver

## Mercure Events

### Stable Topic (consistent with existing patterns)

All trade lookup events use a single stable topic:

```
Topic: poe/trade/results
```

The frontend subscribes once and filters by `requestId` in the payload. This is consistent with how `poe/analysis/updated` works — one topic, payload-based filtering.

### Event Types

**Wait event** (rate limited, request queued):
```json
{
  "type": "waiting",
  "requestId": "abc123",
  "gem": "Spark of Nova",
  "waitSeconds": 4
}
```

**Result event** (data ready, wait path only):
```json
{
  "type": "ready",
  "requestId": "abc123",
  "data": { ... TradeLookupResult as JSON ... }
}
```

**Error event** (queue timeout or API failure):
```json
{
  "type": "error",
  "requestId": "abc123",
  "gem": "Spark of Nova",
  "message": "Trade API temporarily unavailable"
}
```

## Frontend Integration

### Comparator Changes

Current flow:
```
Select gem -> fetchCompare(gems, variant) -> render cards
```

New flow:
```
Select gem -> POST /api/trade/lookup { gem, variant }
           -> if 200: render trade data on card immediately
           -> if 202: show spinner, listen on Mercure poe/trade/results
              -> filter events by requestId
              -> on "waiting": spinner + "waiting Xs" countdown
              -> on "ready": render trade data on card
              -> on "error": show error state on card
           -> fetchCompare(gems, variant) still runs for poe.ninja-based analysis
```

### Card Trade Data Section

Each comparator card gains a new section showing:
- Real price floor (from trade API) vs poe.ninja average
- Total listings count
- Top 10 listing details (expandable): price, seller, listed time
- Manipulation flags if detected (CONCENTRATED, MONOPOLY, OUTLIER)

### 2-Gem Comparison

The existing `BuildCompareResults` accepts `names []string` with no minimum. If 2 gems are selected, compare just those 2 — recommendation logic (BEST/OK/AVOID) works the same way.

### Recommendation Enrichment

The compare endpoint incorporates trade data into scoring when available:
```
score = ROI * signal_weight * (sellability / 100) * trade_confidence

trade_confidence = clamp(
    base_confidence
    * liquidity_factor       // total/200, clamped [0.3, 1.0]
    * spread_factor          // 1.0 if spread < 20% of median, 0.7 if > 50%
    * concentration_factor   // 1.0 NORMAL, 0.7 CONCENTRATED, 0.4 MONOPOLY
    * staleness_factor       // 1.0 FRESH, 0.8 AGING, 0.5 STALE
    * outlier_penalty        // 0.6 if price outlier detected, 1.0 otherwise
, 0.1, 1.0)
```

If no trade data is cached for a gem, `trade_confidence` defaults to 1.0 (no penalty, no boost — behaves same as today).

## API Endpoint

### POST /api/trade/lookup

**Request:**
```json
{
  "gem": "Spark of Nova",
  "variant": "20/20"
}
```

**Response (fast path — 200):**
```json
{
  "gem": "Spark of Nova",
  "variant": "20/20",
  "total": 847,
  "priceFloor": 12.0,
  "priceCeiling": 18.5,
  "priceSpread": 6.5,
  "medianTop10": 14.2,
  "listings": [
    {
      "price": 12.0,
      "currency": "chaos",
      "account": "PlayerOne",
      "indexedAt": "2026-03-16T10:30:00Z",
      "gemLevel": 20,
      "gemQuality": 20,
      "corrupted": false
    }
  ],
  "signals": {
    "sellerConcentration": "NORMAL",
    "cheapestStaleness": "FRESH",
    "priceOutlier": false,
    "uniqueAccounts": 9
  },
  "fetchedAt": "2026-03-16T12:00:00Z"
}
```

**Response (wait path — 202):**
```json
{
  "requestId": "abc123"
}
```

## User-Agent

```
profitofexile/0.1.0 (contact: <project-email>)
```

No `OAuth` prefix since we're not using OAuth — just a good-citizen identifier.

## Configuration

```go
type TradeConfig struct {
    Enabled          bool          // kill switch
    CeilingFactor    float64       // 0.65 — use 65% of reported limits
    LatencyPadding   time.Duration // 1s — added to rate limit windows
    DefaultSearchRate int          // 1 req/5s before real headers arrive
    DefaultFetchRate  int          // 1 req/2s before real headers arrive
    MaxQueueWait     time.Duration // 30s — max time before error event
    CacheMaxEntries  int           // 200
    UserAgent        string
    SyncWaitBudget   time.Duration // 500ms — max time handler blocks for fast path
}
```

League name is read from the existing application config (same source as collector/analysis) to avoid configuration drift.

## Known Limitations (v1)

- **2-3 concurrent user ceiling** — shared VPS IP rate limits. Task #4 (browser extension) addresses this.
- **In-memory cache lost on restart** — rate limiter falls back to conservative defaults, cache is cold. Acceptable for v1.
- **No persistent storage of trade data** — Task #2 addresses schema changes for mixed-source snapshots.
- **Offline seller listings included** — intentional for price discovery, but cheapest listing may not be immediately buyable.

## Future Extensions (separate tasks)

- **Task #2**: Rethink snapshot windowing when trade data mixes with poe.ninja snapshots
- **Task #3**: Lab run session tracker (font pick queue with "next" button)
- **Task #4**: Browser extension for client-side requests (removes 2-3 user ceiling)
- **Background periodic scans**: Top 15 gems every 5 min on LOW priority
- **Currency Exchange API**: Register OAuth app for `service:cxapi` scope — official hourly trade volumes
