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
- JSON collections (a map or slice sent over HTTP/Mercure, into JSONB, or into a JSON cache): emit
  `[]`/`{}` via `make(...)` or a normalizer (`nonNilSparkline`, `nonNil`); nil marshals to `null`,
  which clients read as absent, a cache as never-stored, and NOT NULL JSONB accepts as a scalar.

## Connections and advisory locks

- Pool budget (new goroutines or long-lived pgxpool holders): give fixed sets a test-pinned share
  (`trade_submit_pool_share_test.go`) of `db.DefaultMaxConns` (6) minus the fence; `POE_DB_MAX_CONNS`
  can cut the pool to `cmd/server/main.go`'s `>= 3` floor, so raise that floor when a holder nests.
- Advisory locks (any code taking one): fence via `league.AcquireProcessLock` (session-mode pooling
  only; health polls `CheckHeld`); cap read-then-insert with `pg_advisory_xact_lock` in the writing tx
  (not a pooled session lock: unlock hits another conn, idle reaping frees it, tx pooling zombies it).

## Hypertable migrations

- Hypertable index (a migration indexing an existing hypertable): build it `WITH
  (timescaledb.transaction_per_chunk)` alone in its up.sql, like `20260804100000` (not `CONCURRENTLY`,
  rejected on hypertables; a second statement, even `SET`, wraps the file in a refusing transaction).
- Compressed hypertables (migrations changing a CHECK or deleting rows): `DROP CONSTRAINT IF EXISTS`
  (prod may lack it), then add it `NOT VALID` (no ACCESS EXCLUSIVE full scan; new writes still
  checked); keep non-segmentby-keyed `DELETE`s out of down.sql (each decompresses every chunk).
- Row identity (new row-distinguishing column on an `ON CONFLICT DO NOTHING` table): put it in the
  PK (else `DO NOTHING` drops its second value); on a compressed hypertable follow `20260729120000`
  (decompress, backfill, swap PK and segmentby, re-add the policy; down.sql `RAISE EXCEPTION`).

## Device identity

- Device attribution (a handler storing or scoping submitted data by device): key rows by
  `dev.Fingerprint` from `middleware.DeviceFromContext(r.Context())`, 401 on nil before the handler's
  own DB work, as `lab_runs.go` does (not a body or query `device_id`, which any client can forge).
- Sensitive routes (operator or trusted-caller actions): ship them as container CLIs (the server
  mounts no admin HTTP); an HTTP route gates on a header secret compared with `crypto/subtle`, 404 on
  mismatch, like `mountPprof` (not `X-Device-ID`: any client mints one and gets auto-registered).
