# PoE Official Trade API — Research (2026-03-16)

## Overview

The Path of Exile developer platform exposes **three distinct APIs** for price/listing data:

1. **Trade Search API** — Individual item lookups (same backend as pathofexile.com/trade)
2. **Public Stash Tab API** — Continuous stream ("river") of all public stash tab changes
3. **Currency Exchange API** — Aggregate hourly trade history for currency pairs

**Server endpoint:** `https://api.pathofexile.com` (OAuth API)
**Legacy endpoint:** `https://www.pathofexile.com/api/` (session-based, being deprecated)

---

## 1. Trade Search API (pathofexile.com/trade)

**NOT part of the official developer API.** The trade site uses internal website APIs. GGG's developer docs explicitly state:

> "We can only support resources defined in our API Reference or listed in our Data Exports. Requests for access to any other internal website APIs will be denied. It is against our Terms of Use (section 7i) to reverse-engineer endpoints outside of this documentation."

The trade search endpoints (`/api/trade/search`, `/api/trade/fetch`) are **website-internal APIs** — they work (for now) but are not officially supported for third-party use through the developer platform.

### How It Works (for reference)

Two-stage pattern:

**Stage 1 — Search** `POST /api/trade/search/{league}`

```json
{
  "query": {
    "name": "Kinetic Blast",
    "type": "Skill Gem",
    "status": { "option": "any" },
    "stats": [{ "type": "and", "filters": [] }]
  },
  "sort": { "price": "asc" }
}
```

Response:
```json
{
  "result": ["hash1", "hash2", ...],  // up to 10,000 IDs
  "id": "yYJLQXOcR",
  "total": 5000,
  "inexact": false
}
```

**Stage 2 — Fetch** `GET /api/trade/fetch/{comma-separated-ids}?query={queryId}`

- **Max 10 item IDs per request** (hard API limit)
- Returns full listing: seller, price, item properties, mods, sockets, quality, level, corrupted status, etc.

### Search Result Limits

| Aspect | Limit |
|--------|-------|
| Max IDs returned from search | **10,000** |
| Max IDs per fetch request | **10** |
| Max query filters | **35** |
| `total` field | True match count (can exceed 10k) |
| `inexact` field | `true` when total is approximate |

When `total` <= 10,000: the `result` array contains **ALL** matching item IDs.
When `total` > 10,000: truncated to 10k, no server-side pagination beyond that.

**Key insight for our use case:** A search for "Kinetic Blast" gem returns ALL listings (base, transfigured, corrupted, every quality/level variant) in one search response as IDs. We then selectively fetch the ones we care about in batches of 10.

### Verified Query Field Behavior (discovered via testing 2026-03-16)

These are NOT documented by GGG — discovered by testing against live API:

| Field | Correct Usage | Gotcha |
|-------|--------------|--------|
| `query.type` | Exact gem name match | `query.name` is for uniques only (returns "Unknown item name"). `query.term` is fuzzy and pulls in transfigured variants (e.g., "Kinetic Blast" also matches "Kinetic Blast of Fragmentation") |
| `status.option: "securable"` | **Instant buyout only** — matches trade site "Buyout" toggle | NOT `sale_type: "priced"` which includes ~price (negotiable) listings. `"any"` = all including offline, `"online"` = currently online only |
| `sale_type: "priced"` | Has any price tag (buyout OR negotiable) | Does NOT mean "instant buyout". Includes `~price` listings which are negotiable, not instant-buy |
| `sale_type: "unpriced"` | No price tag at all | |
| `collapse: "true"` | One listing per seller account | Dedup for manipulation detection. May not change count if most sellers have single listings |
| `misc_filters.quality` | `{min, max}` range | Our variant "0" quality means "0-19%" (not quality-gemmed), "20" means exact 20% |
| `misc_filters.gem_level` | `{min, max}` for exact level | |
| `misc_filters.corrupted` | `"false"` to exclude corrupted | |
| `type_filters.category: "gem"` | Restrict to skill/support gems | |

**Listing price types** (from `listing.price.type` in fetch response):
- `~b/o` = buyout (instant purchase) — shown with `status: "securable"`
- `~price` = negotiable — only shown with `status: "any"`

### Trade Data Endpoints (public, no auth, website-internal)

| Endpoint | Purpose |
|----------|---------|
| `GET /api/trade/data/leagues` | Available trade leagues |
| `GET /api/trade/data/items` | Searchable item categories/entries |
| `GET /api/trade/data/stats` | Stat modifiers for query filters |
| `GET /api/trade/data/static` | Static categories (currency, fragments, etc.) |

### Live Search (WebSocket)

```
ws://www.pathofexile.com/api/trade/live/{league}/{queryId}
```

- Requires `POESESSID` cookie + `Origin: https://www.pathofexile.com` header
- Max **20 simultaneous WebSocket connections**
- Pushes new matching items in real-time

---

## 2. Public Stash Tab API ("The River")

**Official developer API.** Scope: `service:psapi`

`GET /public-stash-tabs[/{realm}]?id={next_change_id}`

- Returns `next_change_id` (pagination token) + `stashes[]` array
- Each stash: `accountName`, `id`, `stashType`, `league`, `items[]`, `public`
- Items: full details including `note` (price tag like `~b/o 5 chaos`)
- **No historical data** — always returns current state
- **5-minute delay** on results (officially documented)
- If `stashes` array is empty → you've reached end of stream; poll again with same `next_change_id`
- Track stash re-appearances by comparing `PublicStashChange.id`; if unlisted, only `id` and `public` remain
- Latest change ID available from `https://poe.ninja/stats`

### Stash Types
NormalStash, PremiumStash, QuadStash, EssenceStash, CurrencyStash, MapStash, FragmentStash, DivinationCardStash

### frameType Values
0=normal, 1=magic, 2=rare, 3=unique, 4=gem, 5=currency, 6=divination card, 7=quest, 8=prophecy, 9=relic

---

## 3. Currency Exchange API (NEW — from official docs)

**Official developer API.** Scope: `service:cxapi`

`GET /currency-exchange[/{realm}][/{id}]`

- `realm`: `xbox`, `sony`, `poe2`, or omitted for PoE1 PC
- `id`: unix timestamp (truncated to hour); omit for first hour of history

### Returns aggregate hourly trade history:

```
next_change_id: uint (unix timestamp truncated to hour)
markets: [
  {
    league: "Mirage",
    market_id: "chaos|divine",        // currency pair
    volume_traded: { chaos: N },      // actual trade volume
    lowest_stock: { chaos: N },
    highest_stock: { chaos: N },
    lowest_ratio: { chaos: N },       // price range
    highest_ratio: { chaos: N }
  }
]
```

- **Purely historical** — no data from the current hour
- If `next_change_id` equals what you passed in → you're at the end; wait for next hourly boundary
- GGG may remove old history entries

**This is incredibly valuable for us** — official hourly currency exchange volumes and price ranges, directly from GGG. No scraping needed.

---

## 4. Authentication

### Official OAuth 2.1 (developer API)

**Registration:** Email `oauth@grindinggear.com` with:
1. PoE account name (with 4-digit discriminator)
2. Application name
3. Client type (confidential for web apps, public for desktop)
4. Required grant types
5. Required scopes and justification for each
6. Redirect URI

**WARNING from GGG:**
> "Due to the large volume of recent requests, we will immediately reject any low-effort or LLM-generated requests."

#### Grant Types
- **Authorization Code + PKCE** — user-facing flow
- **Client Credentials** — server-to-server (for `service:*` scopes)

#### OAuth Endpoints (on `api.pathofexile.com`)
| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/oauth/authorize` | GET | Start authorization flow |
| `/oauth/token` | POST | Exchange code / refresh / client credentials |
| `/oauth/token/revoke` | POST | Revoke token |
| `/oauth/token/introspect` | POST | Inspect token validity |

#### Official Scopes (from developer docs)

**Service scopes (Client Credentials — server-to-server):**
- `service:leagues` — League list
- `service:leagues:ladder` — League ladders
- `service:pvp_matches` — PvP matches
- `service:pvp_matches:ladder` — PvP ladders
- `service:psapi` — **Public Stash Tab API**
- `service:cxapi` — **Currency Exchange API**

**Account scopes (Authorization Code — user grants access):**
- `account:profile` — Basic account info
- `account:leagues` — League participation
- `account:stashes` — Personal stash tabs
- `account:characters` — Character data
- `account:item_filter` — Item filter management
- `account:league_accounts` — League account info
- `account:guild:stashes` — Guild stash access (special request)

#### User-Agent Format (required)
```
OAuth {clientId}/{version} (contact: {email})
```

### Legacy: POESESSID (deprecated)
- Session cookie from logged-in browser
- Used by website-internal APIs (trade search, etc.)
- **Being phased out** by GGG

---

## 5. Rate Limits

### Header-Based Dynamic System

From the official docs: rate limits are **dynamic and can change at any time**. Applications must parse response headers:

```
HTTP/1.1 200 OK
X-Rate-Limit-Policy: ladder-view
X-Rate-Limit-Rules: client
X-Rate-Limit-Client: 10:5:10
X-Rate-Limit-Client-State: 1:5:0
```

When exceeded:
```
HTTP/1.1 429 Too Many Requests
X-Rate-Limit-Policy: ladder-view
X-Rate-Limit-Rules: client
X-Rate-Limit-Client: 10:5:10
X-Rate-Limit-Client-State: 11:5:10
Retry-After: 10
```

### Header Breakdown

| Header | Description |
|--------|-------------|
| `X-Rate-Limit-Policy` | Policy name. Same policy across endpoints = shared rate limit |
| `X-Rate-Limit-Rules` | Comma-delimited applicable rules: `ip`, `account`, `client` |
| `X-Rate-Limit-{Rule}` | `hits:period:timeout` — max hits, within N seconds, ban duration if exceeded |
| `X-Rate-Limit-{Rule}-State` | `current_hits:period:active_restriction` (0 if not limited) |
| `Retry-After` | Seconds to wait (on 429) |

### Enforcement Levels
- **`ip`** — applies regardless of auth (most restrictive)
- **`account`** — applies when OAuth-authenticated (more generous)
- **`client`** — general rate limiting

### Invalid Request Threshold (critical!)

From official docs:
> "Applications (and users) that make too many invalid requests in a short period of time will be restricted from further access. Invalid requests include any response codes in the HTTP 4xx range. This includes 401, 403, and 429."

This means: even 429s count toward a **separate** invalid request ban. You must proactively avoid hitting 429s, not just react to them.

### Error Codes
| Code | Message |
|------|---------|
| 0 | Accepted |
| 1 | Resource not found |
| 2 | Invalid query |
| 3 | Rate limit exceeded |
| 4 | Internal error |
| 5 | Unexpected content type |
| 6 | Forbidden |
| 7 | Temporarily Unavailable |
| 8 | Unauthorized |
| 9 | Method not allowed |
| 10 | Unprocessable Entity |

---

## 6. How poe.ninja Works

poe.ninja consumes the **Public Stash Tab river**, not the trade search API:
1. Continuously polls `/api/public-stash-tabs?id={next_change_id}`
2. Parses price notes (`~b/o 5 chaos`, `~price 1 divine`) from stash tab `note` fields
3. Aggregates across thousands of listings for average/median prices
4. Almost certainly GGG-whitelisted (has `service:psapi` scope with elevated limits)

poe.ninja then exposes its own REST API (what we already consume) with processed price data.

---

## 7. Official API Endpoints Summary (api.pathofexile.com)

### Service Endpoints (Client Credentials)
| Endpoint | Scope | Purpose |
|----------|-------|---------|
| `GET /league` | `service:leagues` | List leagues |
| `GET /league/{id}` | `service:leagues` | Get league details |
| `GET /league/{id}/ladder` | `service:leagues:ladder` | League ladder (PoE1) |
| `GET /league/{id}/event-ladder` | `service:leagues:ladder` | Event ladder (PoE1) |
| `GET /pvp-match` | `service:pvp_matches` | List PvP matches |
| `GET /pvp-match/{id}` | `service:pvp_matches` | Get PvP match |
| `GET /pvp-match/{id}/ladder` | `service:pvp_matches:ladder` | PvP ladder |
| `GET /public-stash-tabs[/{realm}]` | `service:psapi` | Public stash river |
| `GET /currency-exchange[/{realm}][/{id}]` | `service:cxapi` | Currency exchange history |

### Account Endpoints (Authorization Code)
| Endpoint | Scope | Purpose |
|----------|-------|---------|
| `GET /profile` | `account:profile` | Account profile |
| `GET /character` | `account:characters` | List characters |
| `GET /character/{name}` | `account:characters` | Character details |
| `GET /stash/{league}` | `account:stashes` | List stash tabs |
| `GET /stash/{league}/{id}` | `account:stashes` | Stash contents |
| `GET /league-account[/{realm}]` | `account:league_accounts` | League accounts |
| `GET /item-filter` | `account:item_filter` | Item filters |

---

## 8. Existing Go Libraries

| Library | Features |
|---------|----------|
| [willroberts/poeapi](https://github.com/willroberts/poeapi) | 100% stdlib, rate limiting (4 req/s), caching, thread-safe |
| [t73liu/poe-arbitrage](https://github.com/t73liu/poe-arbitrage) | Currency arbitrage CLI using exchange API |
| [cptpingu/poe-stash](https://github.com/cptpingu/poe-stash) | Stash indexer with rate limit manager |

---

## 9. Integration Strategy for ProfitOfExile

### What we already have
- poe.ninja consumption → aggregate prices (avg/median for items over time)
- TFT bulk trade prices

### Three integration paths (by official support level)

#### Path A: Currency Exchange API (official, `service:cxapi`)
- **Hourly** aggregate trade data for currency pairs
- Volume, price range, stock levels — directly from GGG
- Perfect for divine/chaos exchange rate tracking
- Requires OAuth registration with `service:cxapi` scope

#### Path B: Public Stash Tab API (official, `service:psapi`)
- Raw listing stream — we could build our own price index for gems
- 5-minute delay, but we get individual listings with full item details
- Could filter for just gems → granular quality/level/corruption price data
- Heavy operational burden (parsing, deduplication, aggregation)
- Requires OAuth registration with `service:psapi` scope

#### Path C: Trade Search API (unofficial website API)
- Search "Kinetic Blast" → get ALL listings (up to 10k IDs) → fetch details in batches of 10
- One search per gem, every 4-5 minutes for top 15 gems + font comparisons
- Gives us real listing data with exact prices, quality, level, corruption status
- **Risk**: these are internal website APIs, not officially supported for third-party tools
- Uses POESESSID or just IP-based rate limits (no OAuth scope exists for this)

### Recommended Approach

**Phase 1 (immediate, no OAuth needed):** Use the trade search website API cautiously:
- 15 gem searches × 1 search + ~5 fetch calls each = ~90 requests per cycle
- At ~10 req/5s limit, one full cycle takes ~45 seconds
- Run every 5 minutes → very low request rate, well within limits
- Parse all variants from each search (base, transfigured, corrupted, quality tiers)
- This gives us the SIGNAL data immediately

**Phase 2 (register OAuth app):** Add Currency Exchange API:
- Email oauth@grindinggear.com requesting `service:cxapi` scope
- Get official hourly currency exchange volumes + price ranges
- Perfect complement to poe.ninja divine/chaos rates

**Phase 3 (optional):** Consider Public Stash Tab API:
- If we want our own real-time gem price index independent of poe.ninja
- Significant engineering effort but gives us full control
- Would need `service:psapi` scope

### Request Budget (Phase 1 — Trade Search)

For 15 gems every 5 minutes:
```
Per gem:
  1 search request → get all IDs (total maybe 200-2000 listings)
  We only need top ~50 listings for price signal → 5 fetch requests (10 IDs each)
  = 6 requests per gem

Per cycle (15 gems):
  15 × 6 = 90 requests
  At conservative ~2 req/s = 45 seconds per cycle

Per 5-minute window:
  90 requests / 300 seconds = 0.3 req/s average
  Well within any reasonable rate limit
```

### Policy Requirements
- User-Agent: `OAuth {clientId}/{version} (contact: {email})`
- Include disclaimer: "Not affiliated with or endorsed by Grinding Gear Games"
- No CORS — all requests must be server-side (our Go backend)
- Keep credentials safe, never in code
- One product per registered application
