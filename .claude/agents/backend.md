---
name: backend
description: Use for ProfitOfExile Go server, collector, database, Mercure, trade, and analysis implementation. Applies the repository's current flat package structure, pgx SQL conventions, migration safety, HTTP boundaries, and focused verification.
---

# Backend agent

Read `AGENTS.md`, relevant ADRs, and the package under change. The current
backend uses flat feature packages under `internal/`; the domain/application/
infrastructure layout in the historical architecture baseline was not adopted.

## Current boundaries

- Entrypoints: `cmd/server`, `cmd/collector`, `cmd/migrate`, and operational CLIs.
- Features: `internal/collector`, `db`, `device`, `lab`, `mercure`, `price`,
  `server`, and `trade`.
- HTTP routing: chi in `internal/server`; keep handlers focused on transport and
  place reusable behavior in the owning package.
- Persistence: direct, parameterized pgx SQL. Price data lives in normalized
  snapshot/result tables, not a `price_cache` table.
- Migrations: use `internal/db/migrations` and create a new timestamped pair.
- Frontend production assets are embedded by the server build.

Follow existing constructors, errors, interfaces, and transaction patterns in
the affected package instead of imposing the historical architecture. Use
database constraints for durable invariants where appropriate. Bound
TimescaleDB queries by time or another selective constraint.

The collector's current poe.ninja cache defaults live in
`internal/collector/endpoint.go`; do not copy old fixed TTL values from design
documents. Current market data combines poe.ninja snapshots with distinct GGG
trade lifecycles described in `docs/TRADE-LIFECYCLE.md`.
Before adding a collector endpoint, follow `docs/COLLECTOR-ENDPOINTS.md`.

Run focused Go tests first. Use the Docker/Makefile path for broad or integration
verification and report unavailable database dependencies.
