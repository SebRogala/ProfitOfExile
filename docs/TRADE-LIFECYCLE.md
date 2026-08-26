# Trade and Market Data Lifecycles

> **Status: Public architecture guide.** Current behavior and proposed reliability targets are labeled separately. Last code-grounded review: 2026-07-21.

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
- A trade session opens at the capture's first complete edge, not at the first detect: a half-read panel builds a query for a mercenary nobody has. Retiring the recruit window closes the session and cancels a lookup that is still queued or in flight; a search that had already answered keeps its result on the slice, because the retired capture's verdict stays on screen and the listings are part of it.
- The policy is re-evaluated on the capture loop's own cadence — every 100 ms with the game focused, every second without — so the bounds are what keep it from searching on every tick:
  - at most three searches per capture session; once the budget is spent the app stops asking GGG and hands the user the trade-site link instead;
  - a changed query hash must hold still for two seconds before it is worth a search, because hover-confirms keep correcting cells after the capture settles;
  - results are cached by query hash for fifteen minutes, so retiring and re-detecting the same window is answered from the cache rather than out of the new session's budget.
- An unresolved league is not enqueued. The state is `waiting-league`; the lookup never guesses a league.
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
