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
- Keep deterministic market data in unit tests; do not call live APIs.
- Use `httptest` for HTTP behavior and assert response content as well as status.
- Put local helpers in `*_test.go`, call `t.Helper()`, and match existing mock style.
- Integration tests use the `integration` build tag and a real PostgreSQL database
  supplied through repository environment configuration. They may skip clearly
  when required infrastructure is absent.
- Give each database test isolated data and reliable cleanup; match the affected
  package's established cleanup pattern.

Test current code: collector scheduling/cache behavior, pgx repositories, API
handlers, Font expected-value calculations, variants, analysis signals, trade,
and desktop-facing contracts. Strategy-tree execution, set conversion, auto-buy,
and breakpoint simulation remain product vision rather than implemented test
targets.

Run the narrow package test first, perform the contract's mutation check, then
use `make test` when the change warrants repository-wide verification.
