---
uid: 8e39f612-f8e4-4aeb-b33a-702359ebbcf3
---

# ADR-009: League-Scoped Repository Convention

## Status

Accepted.

**Amended 2026-08-19 (POE-173):** the convention binds every league-scoped table,
not only the twelve POE-119 retrofitted. The set is now fourteen —
`currency_exchange_markets` and `currency_exchange_cursor` were created with
`league TEXT NOT NULL REFERENCES leagues(id)` and are read and written only
through `internal/exchange`'s scope-taking repository.

**Amended 2026-08-24 (POE-125):** `double_corrupt_snapshots` joins the set (now
fifteen), created with `league TEXT NOT NULL REFERENCES leagues(id)` and read
and written only through `internal/lab`'s scope-taking repository.

**Amended 2026-09-06 (POE-254):** `item_snapshots` joins the set (now sixteen),
created with `league TEXT NOT NULL REFERENCES leagues(id)` and written only
through `internal/collector`'s scope-taking repository.

**Amended 2026-09-06 (POE-255):** `item_snapshots` is also read through
`internal/temple`'s scope-taking repository, which serves the temple market
endpoint. Writer and reader in separate packages is the same arrangement
`gem_snapshots` already has (written by `internal/collector`, read by
`internal/lab`); what the convention binds is that every repository touching the
table takes `league.Scope`, not that one package owns it.

## Enforcement

Tier: required

Scope: All repositories and workflows accessing league-scoped tables.

Summary: Resolve league.Scope once at the lifecycle boundary and carry it through every scoped read and write; admit trade contributions by exact league name before side effects; use one desktop league holder and generation subscription service; historical access is explicit.

## Context

POE-119 adds a league column to its twelve historical data tables and introduces
`runtime_config` as the database-backed selection of the active league. A
repository query without a league predicate can mix observations and analysis
from distinct leagues. A string parameter at individual call sites does not
make the selection's configuration revision visible or establish one convention
for historical access.

`internal/league.Scope` identifies the selected league and carries the runtime
configuration revision returned by `league.Resolve`. `league.Historical`
explicitly creates a revision-neutral scope for historical research.

## Decision

Repositories that read or write league-scoped data must accept
`league.Scope`. They use `scope.ID()` in all predicates and inserted rows; they
do not resolve the active league internally and do not accept a bare league ID
for normal scoped operations.

The caller resolves the scope once at the lifecycle boundary, then passes the
same value through every repository and analysis operation in that workflow.
`Scope.Revision()` is carried with the selection so callers can detect a
runtime-configuration change before committing work that depends on it.

Historical reads must be explicit through `league.Historical`; they do not
silently substitute the active league.

## Consequences

- Repository signatures reveal the required data-isolation boundary.
- A workflow can retain one league identity and configuration revision across
  multiple repository calls.
- POE-120 and POE-121 must apply this convention to existing writers, readers,
  and analysis paths before the POE-119 schema migration is deployed.
- Unscoped lookup/control repositories remain unscoped only when they cannot
  access league-scoped data.

## Evidence

- `internal/league/league.go` — `Scope`, runtime resolution, and historical
  selection.
- `internal/db/migrations/20260723160016_create_league_control_tables.up.sql`
  — `leagues` and `runtime_config`.
- POE-119 — schema foundation that establishes the storage boundary.

## Amended 2026-11-01 (POE-317): retained boss observations

Status: accepted. `boss_loot_runs` retains league identity while serving cumulative
observations within one boss/profile/version. `ListCumulative` and
`StatsCumulative` name this cross-league read explicitly; they are exceptions to
the ordinary scope-taking read convention, not implicit active-league reads.

New submissions originally resolved `league.Scope` at the HTTP boundary; the
POE-117 amendment below supersedes that resolution boundary. Edit/delete first
look up the row by device ownership and id,
then construct `league.Historical` from that owned row's stored league. Every
mutation predicate includes device, id, stored league and expected row version.
Client league never grants mutation authority. Active-league changes therefore
neither retag nor prevent edits to an owned historical observation. Counted sets
remain historical, and profile versions keep changed pools separate.

## Amended 2026-11-01 (POE-117): admission and desktop league ownership

Status: accepted and implemented. Repository implementation verified 2026-11-01
at `66ea8fc7`: POE-117 WI-1 delivered trade contribution admission (POE-319),
WI-3 delivered desktop ownership and reconciliation, and WI-6 delivered
boss-loot boot-scope injection. Active tracking remains in POE-117 and its linked
tasks.

The server resolves `runtime_config` through `league.Resolve` once at boot and
injects that process scope through `server.RouterConfig.League`.
`AnalysisStatus` remains the public league read. New boss observations use this
injected scope through `NewBossLoot`, superseding POE-317's HTTP-boundary
resolution above. Client boss-loot league stays informational; this decision
adds no league-mismatch 409 there. Historical owned edit/delete predicates,
row-version/merge-conflict 409s, and explicit cumulative reads remain as accepted
in POE-317. CLI lifecycle resolution, delayed recompute guard reads and
`LeagueEventGuard` retain their existing responsibilities.

Trade contributions must pass `league.Scope.Admit` by exact league name
immediately after JSON decode, before dictionary validation, cache mutation,
enqueue or persistence. Prefer a nonempty explicit contribution `league`;
otherwise recover it from the legacy trade URL's league path segment, decoded
once with `url.PathUnescape` (including existing raw-space URLs). Malformed or
missing fallback identity is absent, with no default league. Absent identity
returns HTTP 400; a different name returns HTTP 409 with
`{"error":"league_mismatch","league":"<active>"}`. An old zero-result
contribution without a query URL is therefore rejected as absent. Admission
compares names only; revision on status, the desktop slice and this comparison
remains deferred. Existing server revision stamps on Mercure events remain.

Desktop `LeagueSlice` in Rust `ssot.rs` is the single holder; the shared trade
client receives a captured league before queueing rather than owning another
copy. Mercenary search and its league-keyed cache read the same slice. Preserve
the shared GGG client, rate limiter and source cancellation behavior. Generation
advances on first resolution and real name changes, not identical names. A real
change emits `ssot-changed` before one `league-changed {league,generation}` event;
first resolution establishes the baseline without a consumer change event.
The webview SSOT store owns `leagueGeneration` and `onLeagueChange`
subscriptions: `applySnapshot` notifies once per advanced generation after a
resolved baseline. Consumers subscribe there rather than listening directly to
the Tauri event or comparing league names independently.

League resolution is event-driven: startup, a 409 from a write, a Mercure league
hint, Settings Refresh and server-URL change trigger the shared Rust resolver
lifecycle. There is no periodic status poll. `note_server_league` re-resolves
when a hint differs or is unknown; status remains authoritative over hints,
and successful resolutions apply through `apply_league`. Triggered failures
retain bounded backoff and wakeable immediate reconciliation. The server
publishes one `poe/league/active` hint after acquiring its boot fence, reusing the
market publisher and scope stamp (`topic`, `league`, `leagueRevision`). The
existing SSE connection carries that topic and forwards it to Rust; it creates
no second connection. This event-driven convention supersedes earlier periodic
polling proposals; missed-event retention remains deferred.

Implementation evidence:

- `internal/server/handlers/trade.go` (`tradeSubmitHandler`),
  `internal/league/league.go` (`Scope.Admit`) and
  `internal/trade/tradeurl.go` (`LeagueFromTradeURL`) implement admission.
- `desktop/src-tauri/src/ssot.rs` (`write_league`, `apply_league`,
  `run_league_resolver`) and `desktop/src/lib/stores/ssot.svelte.ts`
  (`applySnapshot`, `onLeagueChange`) implement ownership and reconciliation.
- `cmd/server/router_deps.go` (`newRouterDeps`) builds `handlers.NewBossLoot`
  with the boot scope, `internal/server/server.go` (`NewRouter`) registers it,
  and `internal/server/handlers/boss_loot.go` (`Submit`) passes that scope to
  the repository. (Evidence amended 2026-11-12, POE-344; decision unchanged.)

## Amended 2026-11-12 (POE-118): fence-loss contract (interim, pending owner)

Status: interim pending owner decision (exit vs stay-up-and-stop-writers).

The server supervises its fence connection (the `ServerLockKey` session) every
`fenceCheckInterval` (5 s) with a `fenceCheckTimeout` (2 s) per check
(`cmd/server/main.go`). A failed or timed-out check is fence loss. On loss the
server cancels its service context (recompute runtime, trade gate, publishers),
shuts the HTTP server down (10 s bound) and exits 1. There is no re-acquire;
recovery is a fresh boot through `AcquireFencedScope`, and reads are down until
the container restarts. `GET /api/health` still answers 503 on a failed fence
check. The collector fence is unchanged.

Two residual windows are accepted. Loss → detection lasts up to
`fenceCheckInterval + fenceCheckTimeout` (about 7 s); a per-write fencing token
would close it and is deferred. Detection → exit is the graceful HTTP drain
(up to 10 s): requests already in flight run on their own request context and
can still commit, for example a boss-loot submit; an HTTP write gate would close
it and is deferred. Writes stop for certain only when the process exits.

**Operator precondition for deploy: verify the prod server container restarts after a non-zero exit.**
Evidence: the repository `docker-compose.yml` app service sets
`restart: unless-stopped`; the prod restart policy is unverified.
