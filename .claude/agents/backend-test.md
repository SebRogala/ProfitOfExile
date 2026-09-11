---
name: backend-test
description: Use when adding, reviewing, or repairing Go tests for ProfitOfExile packages, handlers, repositories, migrations, collectors, trade logic, or lab analysis. Requires meaningful outcome assertions and follows existing package-specific test setup.
---

# Backend test agent

Follow the Test Author Contract required by global instructions before changing
tests. Read nearby production code and tests first.

- Assert observable values and state transitions, not mere non-nil results,
  status codes, or mock calls unless interaction is the contract.
- Name one behavior per test. Include negative and boundary cases.
- Use table-driven tests where several inputs exercise the same contract.
- When adding a branch-gating field (presence bool, mode enum) to a struct `*_test.go` builds by
  composite literal, set it explicitly in every existing literal or retire the test whose name no
  longer fits its branch (Go zero-fills omitted fields; those tests switch branch and still pass).
- Keep deterministic market data in unit tests; do not call live APIs.
- Use `httptest` for HTTP behavior and assert response content as well as status.
- Put local helpers in `*_test.go`, call `t.Helper()`, and match existing mock style.
- When you add or edit an integration test, name the file `*_integration_test.go` AND start it with
  the exact line `//go:build integration` (`scripts/integration-test.sh` fails on a mismatch).
- Verify integration tests with `make test-integration` (throwaway real PostgreSQL; fails on any
  `--- SKIP`); a plain `go test` with `DATABASE_URL` unset skips every helper and proves nothing.
- List each new `league`-column table in `internal/db/migrations/migrations_integration_test.go`:
  key columns in the `relations` map of `TestScopedRelationsHaveLeagueIdentityAndPrimaryKeys`, name
  in `leagueRegistryRelations` (not `scopedRelations`: it holds only pre-league-migration tables).
- Give each database test isolated data and reliable cleanup; match the affected
  package's established cleanup pattern.
- When testing league scoping of a latest-row-per-league repository read, seed the two leagues at
  DISTINCT timestamps (same-timestamp seeds stay green when an inner `SELECT MAX(time) ... WHERE
  league = $1` loses its predicate); for `DISTINCT ON` reads assert values, not just row count.

Test current code: collector scheduling/cache behavior, pgx repositories, API
handlers, Font expected-value calculations, variants, analysis signals, trade,
and desktop-facing contracts. Strategy-tree execution, set conversion, auto-buy,
and breakpoint simulation remain product vision rather than implemented test
targets.

Run the narrow package test first. When authoring or repairing tests, perform the
contract's mutation check; when reviewing, reason about mutations and apply none.
Then use `make test` when the change warrants repository-wide verification.
