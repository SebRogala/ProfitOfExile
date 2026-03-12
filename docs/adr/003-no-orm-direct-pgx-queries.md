# ADR-003: No ORM — Direct pgx Queries

## Status

Accepted

## Context

ProfitOfExile stores price snapshots, lab analysis results, and strategy trees in PostgreSQL. The data access layer must be chosen before any repository implementations are written, because it affects the structure of every `infrastructure/postgres_repo.go` file across all modules.

The project uses PostgreSQL 16 with JSONB columns for structured data (price books, strategy trees). Several queries involve aggregations over snapshot history and time-series analysis that benefit from direct SQL control.

Key considerations:

1. The domain model is relatively small — a handful of types per module. The mapping problem that ORMs solve (complex object graphs → relational tables) is minimal here
2. JSONB columns require PostgreSQL-specific operators (`->`, `->>`, `@>`) that most ORMs abstract poorly or not at all
3. Direct SQL queries are explicit: the developer sees exactly what hits the database, making performance reasoning straightforward
4. pgx v5 is the idiomatic high-performance PostgreSQL driver for Go, with first-class support for PostgreSQL types including JSONB, UUID, arrays, and LISTEN/NOTIFY

## Decision

Use **pgx v5** directly with raw SQL queries. No ORM layer.

- All repository implementations in `internal/{module}/infrastructure/postgres_repo.go` use `pgxpool.Pool` for connection pooling
- Queries are written as SQL strings, executed via `pool.Query()`, `pool.QueryRow()`, or `pool.Exec()`
- Row scanning is done manually or via `github.com/georgysavva/scany/v2/pgxscan` for struct scanning where convenient
- Migrations are managed by `golang-migrate` with timestamped SQL files in `db/migrations/`
- Transactions are managed directly via `pool.BeginTx(ctx, pgx.TxOptions{})`

## Consequences

### Positive

- Full SQL control — queries are explicit, debuggable, and optimizable without ORM translation layers
- PostgreSQL-specific features (JSONB operators, CTEs, window functions, LISTEN/NOTIFY) are first-class
- No ORM magic to debug when queries behave unexpectedly
- pgx v5 is a high-performance Go PostgreSQL driver with native type support

### Negative

- More boilerplate per query compared to an ORM (explicit scan calls, manual column mapping)
- Schema changes require updating both migration SQL and scan code — no auto-sync
- No query builder means complex dynamic queries (e.g., filtering by multiple optional criteria) require string construction or a lightweight query builder

## Alternatives Considered

### GORM

The most popular Go ORM. Provides struct-tag-based schema mapping, auto-migrations, and chainable query building.

**Rejected because**: GORM adds a significant abstraction layer over pgx that obscures what SQL is being generated. JSONB column handling requires workarounds. GORM's "magic" (hooks, associations) creates implicit behavior that conflicts with the explicit, no-magic philosophy of this codebase. Performance overhead is non-trivial for high-frequency price snapshot writes.

### Ent (Facebook)

A code-generation-based ORM with a schema DSL and graph traversal API.

**Rejected because**: The schema DSL and generated code add a heavy compile-time layer. Ent works best for complex relational graphs with many associations — ProfitOfExile's data model is simple enough that Ent's benefits do not justify its complexity. JSONB columns require custom extensions.

### sqlx

A lightweight extension to `database/sql` that adds struct scanning via reflection.

**Rejected because**: sqlx uses `database/sql` under the hood, which means a pgx connection is proxied through the standard driver interface — losing pgx's native type support (JSONB, UUID, arrays). pgx v5 with `scany/pgxscan` provides equivalent struct scanning convenience without sacrificing native type handling.

## References

- [ADR-002](002-internal-architecture-hexagonal-cqrs-vertical-slice.md) — Hexagonal architecture that defines repository ports (interfaces) implemented by pgx adapters
- [ARCHITECTURE.md](../../ARCHITECTURE.md) — Tech stack table specifying PostgreSQL 16 + pgx
- [POE-13](https://softsolution.youtrack.cloud/issue/POE-13) — Go module init + project scaffold (foundational task)
- [POE-15](https://softsolution.youtrack.cloud/issue/POE-15) — DB migrations + connection wiring (first actual pgx usage)
