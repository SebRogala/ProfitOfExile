---
uid: 49e34882-2cb2-4b18-ba12-8f41984ecf45
---

# ADR-030: Paginated Lists Page, Sort and Filter in the Backend

## Status

Accepted 2026-11-12 (POE-336). Last verified 2026-11-12.

## Enforcement

Tier: required

Scope: Every list the desktop app (`desktop/src`) or the web app (`frontend/src`) shows with a paginator, and the server endpoint that feeds it.

Summary: The backend filters, sorts, counts and pages in SQL; the frontend sends page, sort and filter as request parameters and renders the result. Only a dataset a spec, ADR or task requires to work offline is cached and sorted/filtered on the client.

## Context

The Operator ruled on 2026-11-12 for all projects: "if there is a paginator,
we have to use backend paginator and sorting, do not burden the frontend,
unless that's something that needs to be used offline, then we cache it".
The shared coding rule 7 ("Lists page in the database") states it normatively;
this ADR applies it to this repository and records the current state.

## Decision

A backend list's read side (CQRS query) "filters, sorts, counts and pages in
SQL, set-based, never per row and never in application memory; a page loads
with a fixed number of queries whatever its size". "For a list shown with a
paginator, the frontend sends page, sort and filter as request parameters and
renders what comes back; it does not fetch the whole list to page, sort or
filter it in the client."

Scope: this governs lists shown with a paginator only. A list rendered whole,
without a paginator (for example the Currency Exchange route filters of
ADR-015), is outside this decision.

Offline exception: "a dataset that a spec, ADR or task requires to work
without a network connection is cached on the client, and the client sorts
and filters that cached copy; name that requirement in the change
description. Faster feel, fewer requests, a small dataset today, or simpler
frontend code are not offline requirements." A change claiming the exception
names its source (spec, ADR or tracker task).

## Consequences

Exceptions in this repository: none. No spec, ADR or task found requires a
list in `desktop/src` or `frontend/src` to work without a network connection
(Observed: no offline-list requirement under `docs/`; the "offline" hits are
trade-seller status and icon-migration tooling).

Paginated lists found (Observed, 2026-11-12):

- Boss loot Runs (desktop) — compliant. The page controller requests one page
  (`desktop/src/lib/boss-loot/page-controller.svelte.ts:172`) through the
  `boss_loot_runs` command with `page`/`pageSize`
  (`desktop/src/lib/boss-loot/native-adapters.ts:81`,
  `desktop/src-tauri/src/loot_api.rs:133`). The handler parses and validates
  them (`internal/server/handlers/boss_loot.go:126-140`); the repository
  counts and pages in SQL with `ORDER BY ... LIMIT $5 OFFSET $6`
  (`internal/bossloot/repository.go:193`, `:197`), page size capped at 200
  (`internal/bossloot/model.go:132`). Sort order is fixed server-side; no
  client sort or filter applies.
- The fixture adapter slices in memory
  (`desktop/src/lib/boss-loot/fixture-adapters.ts:135`); it is a test/fixture
  port, not a server-data path, so it is not a violation (Inferred from its
  name and role).

Violations: 0. No other paginator component or state (`totalPages`,
`pageSize`, `Pagination`, `currentPage`, `perPage`) exists in either
frontend (Observed by search); `frontend/src` has none. Client-side sorting
and filtering of whole lists without a paginator remain outside this ADR.

A new paginated list adds page, sort and filter parameters to its endpoint
and a SQL count plus page query; adding a client-side pager over a full
fetch violates this ADR.
