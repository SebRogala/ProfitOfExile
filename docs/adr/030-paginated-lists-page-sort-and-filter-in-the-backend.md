---
uid: 49e34882-2cb2-4b18-ba12-8f41984ecf45
---

# ADR-030: Paginated Lists Page, Sort and Filter in the Backend

## Status

Accepted 2026-11-12 (POE-336). Last verified 2026-11-12. Current violations
are listed under Consequences and are not yet fixed.

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

Violations: 3, listed, not fixed (Observed, 2026-11-12):

1. Desktop Currency Exchange — row cap Show 25/50/100/200/All
   (`desktop/src/lib/pages/CurrencyExchangePage.svelte:452`,
   `desktop/src/lib/exchange/view.ts:185`), Show all at
   `CurrencyExchangePage.svelte:504`; the code calls this "pagination"
   (`desktop/src/lib/exchange/view.ts:1078`). Filters and sorting run in the
   client (`CurrencyExchangePage.svelte:252`) and the result is sliced there
   (`:280`, `view.ts:1103`). Endpoint unbounded: `/currency-exchange/plays`
   receives only mode/horizon (`desktop/src/lib/api.ts:1088`) and returns every
   mode-matching play from the cache snapshot
   (`internal/server/handlers/currency_exchange.go:139-169`).
2. Web gem rankings, Normal and Dedication — row cap Show 10/20/50
   (`frontend/src/routes/lab/components/ByVariant.svelte:327`); client
   filters (`:249-258`) and sort-then-slice
   (`frontend/src/routes/lab/components/BestPlays.svelte:58-73`). The page
   fetches 100 rows per variant (`frontend/src/routes/lab/+page.svelte:178-179`)
   from `/analysis/collective` (`frontend/src/lib/api.ts:394-411`); the
   user's cap, sort and filter picks are not sent as request parameters.
3. Desktop gem rankings, Normal and Dedication — the same mechanism:
   row cap at `desktop/src/routes/(app)/components/ByVariant.svelte:360`,
   sort-then-slice at `desktop/src/routes/(app)/components/BestPlays.svelte:72-82`,
   100 rows per variant (`desktop/src/lib/pages/LabPage.svelte:151-156`)
   from `/analysis/collective` (`desktop/src/lib/api.ts:804-821`).

For 2 and 3 the endpoint is bounded, not unbounded: `limit` is capped at 100
for Normal (`internal/server/handlers/collective.go:165-177`) and 500 for
Dedication (`:230-237`). The backend ranks in application memory, not SQL
(`internal/server/handlers/collective_query.go:94`,
`internal/lab/collective.go:277-285`), which rule 7 also forbids.

A new paginated list adds page, sort and filter parameters to its endpoint
and a SQL count plus page query; adding a client-side pager or row cap over a
full fetch violates this ADR.
