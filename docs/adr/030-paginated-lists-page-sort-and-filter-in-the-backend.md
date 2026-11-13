---
uid: 49e34882-2cb2-4b18-ba12-8f41984ecf45
---

# ADR-030: Paginated Lists Page, Sort and Filter in the Backend

## Status

Accepted 2026-11-12 (POE-336). Last verified 2026-11-13. The three
violations found at acceptance are fixed; see Consequences.

## Enforcement

Tier: required

Scope: Every list the desktop app (`desktop/src`) or the web app (`frontend/src`) shows with a paginator — numbered/next/previous navigation or a row-cap control (Show N / Show all) over server data — and the server endpoint that feeds it.

Summary: The backend filters, sorts, counts and pages in SQL with a fixed number of queries; the frontend sends page, sort and filter as request parameters and renders the result. Only a dataset a spec, ADR or task requires to work offline is cached and sorted/filtered on the client.

## Context

The Operator ruled on 2026-11-12 for all projects: "if there is a paginator,
we have to use backend paginator and sorting, do not burden the frontend,
unless that's something that needs to be used offline, then we cache it".
The shared coding rule 7 ("Lists page in the database") states it normatively;
this ADR applies it to this repository and records the current state.

## Decision

"A backend list's read side (CQRS query) filters, sorts, counts and pages in
SQL, set-based, never per row and never in application memory; a page loads
with a fixed number of queries whatever its size (no N+1: batch the lookups
for the page's rows), and the page query is run against production-sized data
before it ships." "For a list shown with a paginator, the frontend sends page,
sort and filter as request parameters and renders what comes back; it does
not fetch the whole list to page, sort or filter it in the client."

Scope: this governs lists shown with a paginator only. A paginator is either
numbered/next/previous page navigation or a row-cap control (Show N / Show
all) over server data; both decide which slice of a server list the user
sees. Client sorting or filtering of a list with neither control is outside
this decision.

Offline exception: "a dataset that a spec, ADR or task requires to work
without a network connection is cached on the client, and the client sorts
and filters that cached copy; name that requirement in the change
description. Faster feel, fewer requests, a small dataset today, or simpler
frontend code are not offline requirements." A change claiming the exception
names its source (spec, ADR or tracker task).

## Consequences

Exceptions: none documented. Searched `docs/` (including ADRs and
`docs/superpowers/specs/`) and `AGENTS.md`; no requirement for a desktop or
web list to work without a network connection was found (Observed). The
tracker was not searched, so a task-level offline requirement is not ruled
out.

Compliant example (Observed, 2026-11-12) — desktop Boss loot Runs: the page
controller requests one page
(`desktop/src/lib/boss-loot/page-controller.svelte.ts:172`) through
`boss_loot_runs` with `page`/`pageSize`
(`desktop/src/lib/boss-loot/native-adapters.ts:81`,
`desktop/src-tauri/src/loot_api.rs:133`); the handler validates them
(`internal/server/handlers/boss_loot.go:126-140`); the repository counts and
pages in SQL (`internal/bossloot/repository.go:193`, `:197`), page size
capped at 200 (`internal/bossloot/model.go:132`). Running the page query
against production-sized data was not verified. The in-memory slice in
`desktop/src/lib/boss-loot/fixture-adapters.ts:135` is a fixture port, not
the production wiring (Observed: `desktop/src/routes/(app)/+layout.svelte:56`
wires the native ports).

Violations: 3 found at acceptance (Observed, 2026-11-12), all fixed (Observed,
2026-11-13: each commit below resolves on `dev`):

1. Desktop Currency Exchange — row cap, filters and sort ran in the client
   over an unbounded `/currency-exchange/plays`. Fixed by 2d6377e5, 0c895fce,
   138d9ea7 (WI-6 groundwork), a2f51ccb, 38df2415 (WI-7 SQL page query and
   paged route) and 8e1190e6, 9ddf5877 (WI-8 client).
2. Web gem rankings, Normal and Dedication — client sort-then-slice over 100
   rows per variant, ranked in application memory on the server. Fixed by
   7e994f87, 3c108a32 (WI-1 Normal in SQL), 928eae82 (WI-2 Dedication in SQL)
   and a3f6f3b8, 348f14ff (WI-4 web client).
3. Desktop gem rankings, Normal and Dedication — the same mechanism. Fixed by
   the same server commits and 184b5eb2, a17694c7, a3573dd9 (WI-5 desktop
   client).

Production-sized runs (lane DB, synthetic data at documented prod scale, not
production measurements): gem queries at 27,648,000 gem_snapshots (1,440
hourly ticks) — "the slowest single query is the 1000-row search page at
34.3 ms"; currency-exchange `PlaysPage` at "2 horizons x 2,000 plays
(`MaxPlays` x horizons, the production bound)" — "the slowest is the search's
stage count at 35.9 ms" (the slowest single run; its median of 3 warm runs
is 35.4 ms). Measured 2026-11-12/13 with `EXPLAIN (ANALYZE, BUFFERS)`; the
gem figure is a median of 3 warm runs; no index added.

Legacy requests from released desktop builds (Observed): a request without
`pageSize` keeps working. The exchange handler serves it the pre-ADR cache
body unchanged (`internal/server/handlers/currency_exchange.go:217`,
`internal/exchange/plays_page.go:76-77`). The collective handlers keep the
legacy params, bounds and ordering (`internal/server/handlers/collective.go:120`,
`:344`), now ranked in SQL, and their envelope gains an additive `total`
field (`:150-158`, `:393-401`). The legacy path stays until a minimum
desktop version retires it (Inferred from the plan; not yet scheduled).

A new paginated list adds page, sort and filter parameters to its endpoint
and a SQL count plus page query; adding a client-side pager or row cap over a
full fetch violates this ADR.
