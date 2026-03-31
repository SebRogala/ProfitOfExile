# ADR-004: Database Migration Strategy — Auto-Migrate on Start with golang-migrate

## Status

Accepted

## Context

POE-15 introduces the first real database code: a pgx connection pool, the `strategies` table, and versioned schema migrations. Before writing any migration files, the project must decide how migrations are discovered, applied, and managed across dev and production environments.

ADR-003 chose pgx/golang-migrate for data access but did not define the operational model for migration execution. Two questions need answers:

1. Who triggers migrations — the app itself, or an explicit operator step?
2. What tooling manages migration state at the CLI level?

Key considerations:

1. The dev environment is Docker-based with hot-reload (`air`). Developers run `make up` and expect a working DB without extra steps.
2. Production deploys via Coolify with a single container. There is no init container or separate migration job in v1.
3. A broken schema must never result in a partially-booted app serving requests — fail-fast is safer than degraded operation.
4. Developers occasionally need to roll back or force a migration version during development (dirty state recovery).

## Decision

Use `golang-migrate/migrate/v4` with `file`-sourced SQL migrations in `db/migrations/`. Migrations run automatically on app start. A dedicated CLI binary provides manual control.

### Auto-migration on start

`cmd/server/main.go` calls `migrate.Up()` immediately after the pool is created, before the HTTP server binds:

- `migrate.ErrNoChange` is not an error — log at Info and continue.
- Any other error → `slog.Error("auto-migrate failed", "error", err)` + `os.Exit(1)`. The app does not start with a broken schema.

This applies to both dev and production. The migration source path is `file://db/migrations`, resolved relative to the working directory (`/app` in the container).

### Dedicated migration binary

`cmd/migrate/main.go` is a standalone binary for manual migration operations:

| Subcommand | Behaviour |
|---|---|
| `up` | Apply all pending migrations |
| `down [N]` | Roll back N steps (default 1) |
| `force VERSION` | Force version without running (dirty state recovery) |
| `version` | Print current version and dirty flag |

Makefile targets proxy to this binary via `docker compose exec app`:

```
make migrate           # up
make migrate-down      # down 1
make migrate-force VERSION=<n>   # force <n>
```

### Migration file naming

Timestamp-based: `YYYYMMDDHHmmSS_<description>.{up,down}.sql`. Example:

```
db/migrations/20260312100000_create_strategies.up.sql
db/migrations/20260312100000_create_strategies.down.sql
```

Timestamps avoid merge conflicts when two branches add migrations simultaneously (unlike sequential integers).

## Consequences

### Positive

- Zero manual steps for developers — `make up` produces a ready database.
- Production deploys are self-migrating; no separate migration job or init container needed in v1.
- Fail-fast on migration error prevents the app from serving requests against a broken schema.
- `cmd/migrate` binary gives full manual control during development and incident recovery without requiring `psql`.

### Negative

- Auto-migration on start means a bad migration will take down the app on every restart until fixed or rolled back. Operators need awareness of this behaviour.
- The `file://` source path couples the binary to its working directory (`/app`). Running outside Docker requires setting the CWD or adjusting the path.
- No migration locking across multiple replicas in v1 — golang-migrate uses an advisory lock, so concurrent starts are safe, but this is worth revisiting if horizontal scaling is introduced.

## Alternatives Considered

### Explicit migration step (init container or deploy script)

Run migrations as a pre-deploy step separate from the app binary.

**Rejected because**: Adds operational complexity with no benefit in v1. Coolify does not support init containers natively, and a separate deploy script requires coordinating timing with the app start. Auto-migration is simpler and safe with golang-migrate's advisory lock.

### goose

Alternative migration library with sequential or timestamp naming, Go-based migration support.

**Rejected because**: golang-migrate was already cited in ADR-003 as the chosen tool. goose offers similar functionality but switching would diverge from the established decision without clear gain. golang-migrate's separate `up`/`down` SQL files are a better fit for this project's SQL-first approach.

### Embed migrations in binary

Use Go `embed` to bundle migration files into the server binary, eliminating the `file://` path dependency.

**Rejected because**: Adds build-time complexity (embed directive, iofs source) for v1 when the container's working directory is deterministic. Can be revisited if the binary ever needs to run outside its expected container context.

### Manual-only migrations (no auto-migrate)

Require explicit `make migrate` before every deploy.

**Rejected because**: Increases the risk of human error — forgetting to migrate before starting the app. Auto-migration is safer in a single-container deployment where the migration and app lifecycle are tightly coupled.

## References

- [ADR-003](003-no-orm-direct-pgx-queries.md) — established golang-migrate as the migration tool; this ADR defines the operational model
- [POE-15](POE-15) — database layer + migration tooling task that prompted these decisions
