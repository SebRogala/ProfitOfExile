# Trade and Market Data Lifecycles

> **Status: Public architecture guide.** Current behavior and proposed reliability targets are labeled separately. Last code-grounded review: 2026-07-21; server gate hardening reviewed 2026-10-23 (POE-297).

This document explains how ProfitOfExile obtains market data, performs optional live trade lookups, shares desktop-contributed results, and uses Mercure notifications. It is intentionally safe for a public repository: it documents trust boundaries and behavior without deployment credentials or private infrastructure identifiers.

## Why the distinction matters

ProfitOfExile has several workflows that involve prices or gems but do not use the same upstream API, network identity, or rate-limit pool. Treating them as one “trade system” would create incorrect architecture and could accidentally move user-scoped requests onto a shared server address.

The main planes are:

1. poe.ninja market collection.
2. Desktop-native GGG trade lookup.
3. Desktop contribution to the shared server pool.
4. Optional shared server/web Trade Gate.
5. Desktop-to-web pairing discovery.

The collector also emits optional cadence ticks for the shared server Trade Gate. Those ticks are scheduling hints, not market results.

## System map

```text
                                 ┌─────────────────┐
                                 │    poe.ninja    │
                                 └────────┬────────┘
                                          │ market snapshots
                                          v
┌──────────────┐                   ┌───────────────┐
│ Path of Exile│                   │   Collector   │
│  Trade API   │                   │ gems/currency │
└──────┬───────┘                   │ /fragments    │
       │                           └───────┬───────┘
       │                                   │ DB writes + invalidation
       │ user IP                           v
       │                           ┌────────────────┐
       ├──────────────────────────>│     Server     │
       │ Desktop native lookup     │ analysis/cache │
       │          │                └───────┬────────┘
       │          └─ contribution HTTP ────┘
       │
       │ shared server IP, only when explicitly enabled
       └────────────────────────── optional Server Trade Gate

Desktop OCR ──pairing message──> Server ──Mercure pair topic──> Browser
```

## 1. poe.ninja market collection

The collector periodically requests league-specific datasets from poe.ninja:

- Skill gems
- Currency
- Fragments

It stores snapshots in PostgreSQL/TimescaleDB. Successful stores publish Mercure invalidation events so the server can recompute analysis.

This path does not call the Path of Exile trade search/fetch API. It does not consume a desktop user's trade allowance or the optional server Trade Gate allowance.

Mercure delivery is not the authority for these prices. PostgreSQL snapshots are authoritative, and the server must reconcile snapshot watermarks after subscriber reconnect.

## 2. Desktop-native GGG trade lookup

**Current behavior:** Implemented. League propagation is being corrected by the proposed league SSOT work.

The Tauri desktop application can query the Path of Exile trade API directly.

```text
User action or opted-in auto refresh
    -> Tauri trade_lookup
    -> local Rust queue and limiter
    -> GGG search/fetch from the user's network address
    -> local result and queue progress events
```

Properties:

- Requests originate from the individual user's machine.
- Each desktop instance owns its limiter and serialized queue.
- Progress uses local Tauri events, not Mercure.
- Completion returns directly through the Tauri command.
- The lookup remains usable when server-side live trading and Mercure are unavailable.
- League comes from the server-authoritative league status under the league SSOT design.

### Manual lookup

The user explicitly refreshes trade information for a gem. The desktop performs one queued native lookup and displays the result locally.

### Trade price definition

**Current behavior (POE-302, verified 2026-10-23):** The normative definition is
[ADR-027, amendment N = 3](adr/027-trade-is-the-primary-gem-price-poe-ninja-is-the-fallback.md#amendment--n--3-2026-10-23).
The wire field is `TradeLookupResult.medianTop3`, separate from the existing
`medianTop10`. Rust `trade::signals::build_result` and Go `trade.BuildResult`
normalize seller amounts to chaos before ordering and computing it. Each
implementation names its sample size once; both read the same expected outcomes
from `internal/trade/testdata/trade-price-vectors.json`, following the existing
cross-language fixture convention under `internal/*/testdata`.

An empty lookup carries zero in this field but prices nothing. The Lab comparator
widget selects the built quote only when listings exist, otherwise retaining
the ninja price (or `—` for NO_DATA). It does no median calculation. This changes
the row's quote only; tier and Font EV integration remains POE-299.

The widget receives native Rust results and server-cached normal compare results
through the main Comparator's `tradeData` → `set_comparator_data` JSON payload →
`get_comparator_data` 500 ms poll. A new lookup changes `fetchedAt`, which causes
the widget to replace its trade map and recompute the displayed quote. Go cache
admission derives the field from normalized listings too, so database-warmed and
older contributed results need no persisted field or schema change.

Normal Rust and Go gem searches exclude corrupted items. Native Dedication
searches omit that filter and request level ≥21 with exact variant quality;
Dedication compare responses do not attach server trade cache entries. No builder
combines these listing sets. Submitted results accept listing corruption flags
as supplied, however, so the shared cache does not guarantee homogeneous flags.
POE-302 adds no corruption filter.

### Opt-in automatic stale refresh

**Current behavior:** Implemented in the native desktop comparator.

Automatic lookup is disabled by default. When enabled, the desktop comparator examines the age of server-cached or previously loaded trade data.

- Missing data is eligible for refresh.
- Data older than the configured automatic refresh age is eligible.
- The user controls that threshold in desktop settings.
- The current default is 900 seconds (15 minutes).
- Automatic and manual lookups use the same native queue and limiter.

This setting controls when the user's desktop performs native GGG lookup requests. It is separate from contribution consent, server cache retention, and analysis trade-data freshness weighting.

### Mercenary capture auto-search

**Current behavior:** Implemented (POE-202). Desktop-only; the server knows nothing about mercenary listings.

This is the second automatic path through the native lookup, and it is not governed by the stale-refresh setting above. When the mercenary module has fully read a recruit window, the desktop builds a trade query for that exact mercenary and searches for it without the user asking.

- It has its own toggle, `merc_trade_auto`, default **on**. It is independent of the comparator's automatic stale refresh (default off) and of contribution consent.
- The query the app **posts** is one `and` group holding every cell that names exactly one support, plus one `count` group of `value.min = 1` per cell that names several (an icon read the app could not narrow, or a support loosened down to the tier floor). It carries **no** `mercenary` group and so no row scoping: a mercenary with the same skills and links arranged across different rows also matches, which is an accepted false-positive class. GGG's complexity budget is what forces it — a `mercenary` group costs 21 of a budget of 35, so two of them never fit and a captured panel is four or five rows. Against that budget an `and` filter costs 1 and a `count` group costs 3 plus 2 per filter; over it, the app drops its own tier loosening first and then support cells from the last row inward. Only the second is reported to the user as a looser query than the capture asked for — a query that lost just the loosening still asks for exactly what the panel showed.
- The `trade ↗` **link** carries a different query (since 2026-09-06). It opens in the user's browser under the user's own login, where GGG's budget is the larger logged-in one ("Logging in will increase this limit", the API's own words, verified 2026-09-05), so it takes the guides' shape: one `and` group of the rolled skills — a recognised class's two fixed primaries left out, since every listing of the class has them (`docs/GAME-FACTS.md`, "Mercenary class primaries") — and one row-scoped `mercenary` group per linked row asking for the skill plus its links at any tier the floor admits, one link short on a row with two or more (`value.min` = 1 + cells − 1, `LINK_TOLERANCE`; measured 2026-09-06 — every link on every row found no listing, one short per row found several). It is built from the rows before the budget walk, so it keeps the loosening the posted body may have dropped. The logged-in budget is measured only as a lower bound, so a wide panel at a low floor may still be refused in the browser; that is an open experiment, not a guarantee. The posted body was kept flat because the app has no login.
- A second shape of the same link, **anchor rows**, is published beside it (`urlAnchors`, since 2026-09-06, on trial): only the linked rows whose skill is a combat skill — a primary, a rolled secondary, or one the class table does not know — each as a `mercenary` group with the same one-link slack, and an `and` group of just those anchor skills. Utility rows (movement, auras, curses) and rows without links are left out, because a mercenary is priced by a desired combat skill and the links on that row (`docs/GAME-FACTS.md`, "Mercenary classes and their skill pools"). When nothing anchors it is the every-row link. The page shows one button per shape so the two can be judged on the same capture, plus a link to a third party's warrant price check, which takes a pasted warrant and nothing from the capture.
- A trade session opens at the capture's first complete edge, not at the first detect: a half-read panel builds a query for a mercenary nobody has. Retiring the recruit window closes the session and cancels a lookup that is still queued or in flight; a search that had already answered keeps its result on the slice, because the retired capture's verdict stays on screen and the listings are part of it.
- The policy is re-evaluated on the capture loop's own cadence — every 100 ms with the game focused, every second without — so the bounds are what keep it from searching on every tick:
  - at most three searches per capture session; once the budget is spent the app stops asking GGG and hands the user the trade-site link instead;
  - a changed query hash must hold still for two seconds before it is worth a search, because hover-confirms keep correcting cells after the capture settles;
  - results are cached by query hash for fifteen minutes, so retiring and re-detecting the same window is answered from the cache rather than out of the new session's budget.
- An unresolved league is not enqueued. The state is `waiting-league`; the lookup never guesses a league.
- A lookup GGG **rejects** — any HTTP 4xx, 429 included — is not retried for that query. The rejection is a verdict on the request body, identical every time it is asked, and 4xx responses count toward GGG's separate invalid-request ban (`docs/RESEARCH-poe-trade-api.md`, "Invalid Request Threshold"). A 5xx or a transport failure keeps the debounced retry. The refusal is scoped to the rejected query hash, so a corrected capture is searched for normally.
- The path is local by construction: it calls `TradeApiClient::lookup_query(TradeSource::Mercenary)` directly rather than the `trade_lookup` command, so no branch in it can reach `/api/trade/submit`. Mercenary results are never contributed to the shared pool, and no server feature flag governs them.
- Mercenary and gem lookups share the one native queue and limiter. Queue events carry a `source` (`gem` or `mercenary`) and each surface ignores the other's. The queue's `Done` event means the fetch succeeded — not that a caller accepted or displayed the result.
- Prices come back as the sellers' own amounts in the sellers' own currencies. There is no divine-to-chaos rate on the Rust side, so `chaos_price` is the raw seller number and the row order is left as GGG returned it under `sort.price=asc` — the only value ordering in this path computed by a party that knows the rates.

**Deferred:** the native lookup path answers a GGG 429 with an error message and does not read the response's `Retry-After` header; the only wait it observes is its own limiter's. That was deferred deliberately under POE-202 and is tracked as a follow-up.

## 3. Desktop contribution to the shared pool

**Current behavior:** Successful native lookups make a best-effort, fire-and-forget server submission.  
**Target behavior:** Durable, idempotent, authenticated contribution as described below.

Contribution is a separate persisted opt-in, disabled by default; it is not implied by enabling automatic stale refresh. When the user enables it, every successful native lookup should be contributed to the server so other users and analysis can benefit from a fresher shared pool.

```text
Successful desktop result
    -> durable pending contribution
    -> authenticated POST /api/trade/submit
    -> server validation
    -> league-scoped shared cache
    -> trade history persistence
    -> acknowledgement
```

The local result does not depend on contribution success. A user should still receive the lookup they performed if the server is temporarily unavailable.

The reliable target contract is:

- A stable contribution ID makes retries idempotent.
- Pending contributions survive desktop restart.
- Submission contains league/revision, source, fetched time, and bounded result data.
- Server accepts only authenticated, valid, correctly scoped contributions.
- Older data never replaces a newer cache entry.
- Duplicate and stale-but-valid acknowledgements are terminal and safe.
- Transient errors retry with bounded backoff and jitter.
- Disabling contribution stops network submission and deletes unsent pending contribution payloads.
- Desktop credentials use an enrolled device signing key held in the OS credential store; an observed or caller-supplied device ID is not authentication.
- Public responses do not expose raw device fingerprints.

Shared trade history records provenance such as `desktop-native` versus `server-gate`. This is important because the sources use different network identities and operational policies.

Mercure may notify clients about a changed shared cache entry, but HTTP acknowledgement and persistence provide correctness.

## 4. Optional shared server/web Trade Gate

**Current behavior:** Implemented but disabled by default; capability negotiation and request-scoped result delivery are proposed reliability work.

The server can optionally perform GGG trade lookups for web requests and background cache refresh. This feature is disabled by default because all requests originate from one shared server network address and therefore share one rate-limit pool.

When disabled:

- The full server lookup route is unavailable.
- Web UI should advertise cache-only/unavailable live lookup capability.
- Cached desktop-contributed results can still enrich analysis and comparisons.
- Desktop-native lookup remains unaffected.

When enabled:

- Server status advertises the capability.
- Interactive web lookup can return immediately or queue asynchronously.
- Asynchronous progress uses a request-capability-scoped Mercure topic; result data is retrieved through capability-validated HTTP, with bounded polling/status reconciliation.
- Server owns one shared search/fetch limiter.
- Results persist with `source=server-gate`.

### Current lookup and contribution input validation (POE-297)

`POST /api/trade/lookup` and `POST /api/trade/submit` use the same
`validateTradeInput` helper, dictionary and membership checker. Lookup validates
before cache access or gate submission, including `force=true` and non-forced
cache hits. Contribution validates before cache mutation or persistence enqueue.
Unknown names and malformed variants return the existing JSON
`{ "error": "..." }` shape with HTTP 400: these are invalid request fields rather
than missing HTTP resources.

The league-bound `lab.Cache.GemDictionary` is a positive name cache, not a complete
allowlist. Its base and transfigured OCR pools union `gem_colors` with market
snapshot names, including corrupted market identities, but deliberately exclude
Heist-only names and base support gems. On a warm, nonempty dictionary miss,
`trade.Repository.KnownGem` checks exact membership in the unfiltered shared
`gem_colors` registry or this league's `gem_snapshots`. This preserves known
support, base, transfigured and corrupted names without applying Font eligibility
to trade lookups. Neither name source stores allowed variants: the snapshot's
variant is separate from its name. The handler imports lab and trade; trade does
not import lab, preserving the ADR-008 package direction.

Variant syntax is decimal level `1`–`99`, optionally followed by `/` and quality
`0`–`99`, without leading zeros. Level-only aliases such as `1`, `4` and `20`
remain valid alongside `1/0`, `1/20`, `20/0` and `20/20`. The `c` suffix used by
corrupted snapshot variants (`21/20c`, for example) is not accepted: the current
server query parser does not implement it. Name membership does not change the
query builder's existing corruption behavior.

A cold, empty or missing dictionary, or a missing membership checker, permits
names through so startup does not disable all live lookups. The limiter and
flood breaker remain the fallback in that state. Dictionary warmth is read on
every request. Once warm, a membership query error returns JSON 503 before
submission, cache mutation or persistence; cancellation and database failure
cannot be used as a validation bypass. The same startup allowance applies to
contributions, including when the server-side lookup gate is disabled.

Contributed `fetchedAt` must be later than receipt time minus five minutes and
no later than receipt time. Missing, future, or at-least-five-minute-old stamps
are reset to the server's receipt time in both the cache and persisted row;
recent timestamps are retained. The five-minute window matches the current
collector trade-tick stale age. Resetting an ancient stamp to receipt, rather
than merely to the stale cutoff, prevents it immediately becoming the oldest
refresh candidate. Future stamps cannot keep a contributed key fresh indefinitely.

Collector ticks choose existing cache keys, not a gem name from the tick
payload. New contributions pass the same input guard before entering that pool.
On refresh selection, `HandleTradeTick` calls the same `KnownTradeGem` helper
used by `validateTradeInput`. With a warm dictionary, unknown stale keys are
skipped for that tick and the next known stale key can be refreshed, including
when the unknown key was restored from persisted history. Name checks run
outside the cache lock; a membership query error abandons the tick without
sending. Exclusions are per tick so later dictionary growth can admit a name.
Cold/empty dictionaries or an absent checker skip no names, preserving startup
behavior. Cache entries and rows are retained unchanged.

The live callback in `cmd/server` passes `trade.Repository.KnownGem` to the tick
handler as it does to the HTTP routes. `Refresher` has no production constructor
call and remains unchanged. Native desktop lookups still use the user's own
limiter/IP; only their server contribution passes the server input guard.

Known residuals: names present in `gem_colors` or `gem_snapshots` can still be
rejected by GGG and contribute to the breaker. Each warm OCR dictionary miss
costs one database membership probe when that checker is available. The cold allowance lasts until a v2 run
successfully populates the dictionary; failed seed reads can prolong it.
A genuinely old contribution is still stamped as receipt time by the clamp;
the owner reports native desktop submissions arrive within seconds. Once name
validation is warm, new-league names absent from both name sources are rejected
on submit. Dictionary-query 503 responses have no `Retry-After` header.

### Current server gate restrictions and empty results (POE-297)

Search waits only for search capacity; fetch waits only when listings need
fetching. Before each send, the gate rechecks capacity at intervals of at most
250 ms, waking sooner at the endpoint's next allowed time or the queue deadline.
Each queued caller has an independent deadline (30 seconds by default), including
limiter waiting before search starts. A later deduplicated caller retains its
own deadline. Timers expire queued callers even while another lookup is in HTTP
or waiting for fetch capacity; expired queue items make no upstream request.
The queue deadline stops when search starts, so it does not time out a running lookup.
Cancellation during either wait delivers an error and releases the in-flight key.

Every attempted send consumes a local slot, including failed requests. Every
response's headers are observed, including errors. `Retry-After` (seconds or an
HTTP date) and every `X-Rate-Limit-*-State` restriction extend one limiter-wide
blocked-until timestamp. Both endpoints stop until that time and resume without
operator action. HTTP 401, 403 and 429 without a usable restriction duration
trigger a precautionary fallback of at least one minute, raised to the known
endpoint window/penalty and capped at 30 minutes. Other individual 4xx responses
(such as 400, 404 and 422) reject only that request and key; they do not pause
unrelated lookups unless a restriction header supplies an active deadline.

A limiter-wide circuit breaker also triggers the bounded fallback after five
headerless non-429 4xx responses within a sliding ten-minute window. It combines
search and fetch failures, including 401/403. Events exactly ten minutes old
are excluded; successful requests do not reset the window. Responses with a
parseable active restriction and 429 responses do not increment this counter.
A threshold of five failures and a ten-minute window are local policy constants
(`invalidRequestThreshold`, `invalidRequestWindow`), not published GGG limits:
the research document gives no numeric invalid-request threshold. Explicit
upstream restriction durations are never capped.
Restriction extensions log once; queued requests do not each log the restriction.
These are defensive server choices, not claimed upstream defaults. The upstream
header semantics were checked against [GGG's rate-limit documentation](https://www.pathofexile.com/developer/docs#ratelimits)
on 2026-10-23.

A successful search with no IDs skips fetch and completes normally: a fresh
empty `listings: []` result is cached, persisted and delivered. Its `FetchedAt`
stamps the check so the stale picker does not select it again immediately;
it becomes eligible again at the normal refresh age. Empty listings mean no
quote, not a 0c price. Analysis falls back to ninja, and comparison clients show
no listings. A restart warms the empty result instead of reviving an older quote.

A failed upstream search or fetch postpones only that cached key's stale
selection. Request-scoped 4xx rejections (all 4xx except 401/403/429) receive six
hours; retrying an unchanged query every five minutes can repeat a rejection
such as an unknown gem name. Transport, decode and 5xx failures retain five
minutes, as do 401/403/429 responses. These are local retry policies, not upstream
limits. An explicit lookup can still succeed and clear the delay.

If a queued caller times out and a global restriction overlapped its queue
lifetime, its key is also deferred until five minutes after restriction expiry
or timeout processing, whichever is later. Expired restriction evidence is
retained so delayed timer callbacks and dequeue processing still defer the key.
This records
scheduling metadata on the timeout path instead of coupling the stale picker
to the limiter, and prevents an expired queued key immediately leading the
picker on resume. An existing longer delay (such as the six-hour rejection
delay) is never shortened. Ordinary queue expiry without a global restriction
and shutdown cancellation do not record a market failure.

The last successful result and its `FetchedAt` stay unchanged; no price row or
zero-listing check is created. All per-key delays are process-local, cleared on
successful replacement or eviction, and lost on restart. Other stale keys
remain selectable during the delay.

New gate history rows use `server-gate`, including interactive web requests and
background refreshes. Historical `user` rows remain unchanged. Cache warming
and backtest reads have no source predicate, accepting legacy `user`, new
`server-gate` and desktop rows equally. No schema migration is needed.

The browser must subscribe to its request-capability-scoped result topic only when this capability is enabled and in use. A shared `poe/trade/results` subscription is not an acceptable target design.

## 5. Collector cadence for the optional Trade Gate

**Current behavior:** Implemented at the cadence and selection rules below. Explicit suppression while the server gate is disabled is proposed.

The collector currently provides timing, while the server chooses and performs the optional lookup.

At the current default cadence of 45 seconds, ticks alternate between:

1. The oldest sufficiently stale cached transfigured `20/20` gem whose tier is MID or higher.
2. The oldest sufficiently stale cached transfigured `20/20` gem from the full cached pool.

The current minimum age is five minutes. The filtered selection uses `>= MID`, so “MID+” means MID, MID-HIGH, HIGH, or TOP.

Important boundaries:

- A tick contains no trade result.
- The collector does not call GGG for this workflow.
- The server chooses at most one candidate from its existing cache.
- If the cache has no eligible entry, no lookup is made.
- If the optional server Trade Gate is disabled, no GGG request is made.
- Tick publication should itself be disabled when the gate is intentionally unavailable to avoid unnecessary hub traffic.

The poe.ninja collector remains active regardless of this optional cadence feature.

## 6. Desktop pairing discovery

Desktop pairing currently connects OCR discovery to the browser comparator:

```text
Desktop detects gem names
    -> server pairing endpoint
    -> Mercure topic scoped by pairing code
    -> paired browser populates comparator inputs
```

**Current behavior:** The short pairing code scopes the Mercure topic used for discovery messages.

**Target behavior:** Treat the short code only as a rendezvous value. Browser entry creates a pending challenge but grants no subscription. The authenticated Desktop explicitly approves that challenge using a private rendezvous handle; the server then consumes the one-time code and issues high-entropy, expiring credentials for one exact session topic. Codes have expiry and attempt limits, and replay, cancellation, replacement, and approval races fail closed.

This workflow:

- Sends gem names and variant information.
- Does not itself call GGG.
- Does not consume desktop or server Trade Gate capacity.
- Does not currently provide a reverse remote-command channel to the native desktop trade client.

The native desktop's own opt-in automatic stale refresh is a separate workflow. If browser-to-desktop remote lookup is added later, it requires explicit consent, authentication, command identity, result acknowledgement, and rate-limit visibility.

## 7. Shared cache lifecycle

The target shared-cache key is:

```text
(league, gem, variant)
```

Cache entries carry their upstream `fetchedAt` timestamp and provenance. New data replaces existing data only when it is newer and valid.

Consumers include:

- Web comparison cache-only display
- Desktop comparison before optional native refresh
- Analysis feature enrichment
- Optional server Trade Gate stale-entry selection
- Server restart cache warming from league-scoped persistence

Cache freshness is not one universal threshold:

- Desktop warning/critical colors are user-facing settings.
- Desktop automatic refresh age controls user opt-in requests.
- Optional server cadence minimum age controls shared-server refresh eligibility.
- Analysis may weight or ignore trade evidence at its own documented ages.

These values must be named separately to prevent one setting from silently changing another policy.

## 8. Mercure lifecycle

Mercure topics used by these workflows include:

| Topic | Purpose |
|-------|---------|
| `poe/collector/gems` | Notify server that authoritative gem snapshots changed |
| `poe/collector/currency` | Currency snapshot invalidation |
| `poe/collector/fragments` | Fragment snapshot invalidation |
| `poe/collector/trade-tick` | Optional lossy server Trade Gate cadence |
| `poe/analysis/updated` | Tell clients to reconcile analysis revision |
| `poe/trade/results/{requestCapability}` | Target: opaque optional Trade Gate progress notifications; result bodies are fetched over capability-validated HTTP |
| `poe/desktop/{pair}` | Current: short-code-scoped discovery; target: rendezvous only, followed by a high-entropy session capability |

Notifications can be lost or duplicated. Reconnect must reconcile authoritative database/API revisions. Administrative work and analysis scheduling must be idempotent and coalesced.

See tracker item `POE-118` (Mercure lifecycle reliability and event coordination) for the implementation contract.

## 9. League rollover behavior

All shared market and trade data is league-scoped.

During rollover:

- Desktop obtains the new league from server status before enabling lookup actions.
- Native desktop contributions identify the exact league/revision.
- Server rejects contributions or events from a different active league.
- Cache warming selects only the active league.
- Collector poe.ninja requests use the database-authoritative active league.
- Optional server Trade Gate and cadence use the same active league.

See tracker item `POE-117` (League SSOT, historical isolation, and safe rollover CLI).

## 10. Failure behavior

| Failure | Expected behavior |
|---------|-------------------|
| Mercure unavailable | Authoritative HTTP/DB reconciliation continues; desktop-native lookup still works |
| Server unavailable during native lookup | Local result succeeds; contribution remains pending |
| Duplicate contribution | Server acknowledges idempotently |
| Older contribution | Retain newer cache entry; acknowledge without retry loop |
| Optional server gate disabled | Web uses cache-only state; collector market collection remains active |
| Collector trade tick lost | No correctness loss; a later tick may refresh another stale entry |
| Collector market invalidation lost | Server reconnect/database watermark reconciliation catches up |
| League mismatch | Reject event/contribution and do not mutate active cache |

## 11. Public repository safety

This project is public. Documentation and examples deliberately use generic placeholders and omit:

- Real credentials or signing secrets
- Private service addresses
- Production network/container identifiers
- Real pairing codes or device fingerprints
- Backup locations and access details

Contributors should document trust boundaries and configuration behavior clearly while keeping secrets in deployment-managed configuration.

## Related documents

- Tracker `POE-118` — Mercure lifecycle implementation specification.
- Tracker `POE-117` — League SSOT and rollover specification.
- Tracker `POE-88` — LabCompass fidelity and overlay SSOT specification.
- [Overlay guide](OVERLAY-GUIDE.md)
