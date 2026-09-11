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

## Go language traps

- Sorting (`sort.Slice`/`slices.SortFunc` before ranking, truncating, or serving): end comparators
  on a unique key (name, then variant) or `sort.SliceStable` fixed-order input (not a single key:
  ties keep input order, random from a map or unordered query, so a top-N cut serves other rows).
- Float sums (analysis code accumulating floats from a Go map): copy the entries to a slice, sort it
  by a unique key, then add (not `+=` over the map: random order and non-associative addition give
  identical data last-bit-different sums that flip cached rankings and change detection).
- JSON collections (marshalling a map or slice to JSONB, HTTP/Mercure, or a cache): initialise with
  `make(map[K]V)`/`make([]T, 0)` (nil marshals to `null`: clients get `null` not `[]`, a cached
  empty answer reads as never-stored, a NOT NULL JSONB column lets the `null` scalar through).

## Connections and advisory locks

- Pool budget (new goroutines or long-lived pgxpool holders): give fixed sets a test-pinned share
  (`trade_submit_pool_share_test.go`) of `db.DefaultMaxConns` (6; pgbouncer exhaustion hangs 120 s)
  minus the fence; raise `cmd/server/main.go`'s `>= 3` floor if a lock holder also uses the pool.
- Advisory locks (any code taking one): fence via `league.AcquireProcessLock` (health polls
  `CheckHeld`); a read-then-insert cap takes `pg_advisory_xact_lock` in the writing tx (not a pooled
  session lock: unlock/`CheckHeld` hit other conns, idle reaping frees it, tx pooling zombies it).

## Hypertable migrations

- Hypertable index (any migration creating one): build it `WITH (timescaledb.transaction_per_chunk)`
  alone in its up.sql, like migration `20260804100000` (not `CONCURRENTLY`, rejected on hypertables;
  a second statement, even `SET`, makes the file an implicit transaction, which refuses it).
- Compressed hypertables (migrations changing a CHECK or deleting rows): `DROP CONSTRAINT IF EXISTS`
  (prod may lack it), then add it `NOT VALID` (no ACCESS EXCLUSIVE full scan; new writes still
  checked); keep non-segmentby-keyed `DELETE`s out of down.sql (each decompresses every chunk).
- Row identity (new row-distinguishing column on an `ON CONFLICT DO NOTHING` table): put it in the
  PK (else `DO NOTHING` drops its second value); on a compressed hypertable follow `20260729120000`
  (decompress, backfill, swap PK and segmentby, re-add the policy; down.sql `RAISE EXCEPTION`).

## Device identity

- Device attribution (a handler storing or scoping submitted data by device): key rows by
  `dev.Fingerprint` from `middleware.DeviceFromContext(r.Context())`, 401 on nil before any DB
  access, as `lab_runs.go` does (not a body or query `device_id`, which any client can forge).
- Sensitive routes (any route restricted to operators or trusted callers): gate on a header secret
  compared with `crypto/subtle`, 404 on mismatch, as `mountPprof` does with `POE_PPROF_TOKEN` (not
  `X-Device-ID`: any client mints a fingerprint and `DeviceMiddleware` auto-registers it).
