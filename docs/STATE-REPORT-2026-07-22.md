# Codebase State Report — 2026-07-22

**Status:** Dated measurement. Canonical for *what was measured on 2026-07-22*, not for current behavior thereafter.
**Last verified:** 2026-07-22, branch `main` at `664751e`.
**Canonical for:** the gate inventory, size/complexity distribution, module import graph, table-ownership map, and conformance against `/var/www/project-seed` principles, as of the date above.
**Not canonical for:** any of these facts after code changes. Re-run the commands cited inline rather than trusting the numbers.

## Why this document exists

`project-seed`'s brownfield protocol (`INHERIT.md`, Phase 0) requires a written state report as a durable artifact: *"the map is a deliverable, not an assumption."* It exists so future work cites a measured baseline instead of re-deriving one per task.

The protocol's usual premise does not apply here. ProfitOfExile is AI-born, so its owner watched every module appear and Phase 0 should have been free. It was not, because the project outgrew the window in which one person could hold all of it. The diagnosis is not *inherited* code but **outgrown** code. The consequence is that this report must be regenerable — a script plus a document — not a one-time reading.

Produced by three read-only audits. Every number below came from a tool run or a grep, not an estimate. Where something could not be executed, it is marked unverified.

---

## 1. Gate inventory — what actually guards production

| Gate | `make qa` | `deploy.yml` (push→main→prod) | `quality.yml` | `desktop.yml` (tag) |
|---|---|---|---|---|
| `go test -race ./...` | yes | yes | yes | — |
| integration-tagged Go tests | **no** | **no** | **no** | — |
| web `npm run build` | no | yes (Dockerfile stage) | yes | — |
| web `svelte-check` / lint / tests | no | no | no | — |
| desktop `svelte-check` | no | no | yes | no |
| desktop `vitest` | no | no | yes | no |
| desktop `cargo test` | separate target | no | yes | no |
| `gofmt` / `go vet` / any linter | **no** | **no** | **no** | **no** |
| any baseline / ratchet | **no** | **no** | **no** | **no** |

**The load-bearing finding.** `main` is not branch-protected, and `deploy.yml` and `quality.yml` are independent workflows with no `needs:` relation. A push to `main` that breaks `svelte-check`, `vitest`, or `cargo test` **still deploys to production**, provided `go test -race` passes. On the production path, the Go test suite is the only mechanically enforced gate. `quality.yml` on `main` is a post-hoc notification.

**Orphaned gate.** 21 tests carry `//go:build integration` (in `internal/db`, `internal/collector`, `internal/price/gemcolor`, `cmd/migrate`, `internal/db/migrations`). No Makefile target and no CI job passes `-tags integration`, so they have executed zero times. `.claude/agents/backend-test.md:18` documents the tag as though it runs. This is silent gate death, already realized. Whether they still pass is **unverified** — assume some rot.

## 2. Size and complexity

Largest production files (`find internal cmd -name '*.go' ! -name '*_test.go' | xargs wc -l | sort -rn`):

```
1587  internal/server/handlers/analysis.go
1349  internal/lab/repository.go
1010  internal/lab/optimizer.go
 908  cmd/backtest/main.go
 788  internal/server/handlers/collective.go
 658  cmd/optimize/main.go
 656  internal/lab/collective.go
 633  internal/lab/classification.go
 613  internal/lab/trends.go
 581  cmd/server/main.go
```

Outside Go: `desktop/src-tauri/src/lib.rs` at **2602 lines** (41% of the Rust surface), `Comparator.svelte` at 1582 (desktop) and 1430 (web).

Complexity outliers, top 10 of 44 over gocyclo 15 / 48 over gocognit 20:

| Function | Cyclomatic | Cognitive | Location |
|---|---:|---:|---|
| `main` | 77 | 190 | `cmd/server/main.go:91` |
| `AnalyzeDedication` | 46 | 140 | `internal/lab/dedication.go:69` |
| `TrendAnalysis` | 46 | 127 | `internal/server/handlers/analysis.go:361` |
| `CollectiveAnalysis` | 39 | 92 | `internal/server/handlers/collective.go:60` |
| `AnalyzeFont` | 38 | 114 | `internal/lab/font.go:182` |
| `computeTemporalCoefficients` | 35 | 65 | `internal/lab/temporal_normalization.go:52` |
| `main` (collector) | 34 | 45 | `cmd/collector/main.go:20` |
| `BuildCompareResults` | 34 | 61 | `internal/lab/collective.go:258` |
| `ValidateSellability` | 31 | 56 | `internal/lab/optimizer.go:738` |
| `computeOfferingTiming` | 31 | 44 | `internal/server/handlers/analysis.go:1397` |

Distribution: 2006 functions, 41.5k lines across `cmd/` + `internal/`. Function length p50/p90/p95/p99 = 22/59/79/164. 184 functions exceed 60 lines; 30 exceed 120; 10 exceed 200.

### `internal/lab` holds two unrelated domains

25 files, 207 functions, 52 types, **8955 lines — 43% of all non-test `internal/` code**, and the highest fan-in of any non-leaf package. It contains both:

- the actual Labyrinth-minigame data (`layout_repository.go` → `lab_layouts`), and
- the entire market-pricing and analysis engine (`repository.go`, `analyzer.go`, `optimizer.go`, `trends.go`, `classification.go`, `tiers.go`, `confidence.go`, `gem_features.go`, `gem_signals.go`, `gem_profiles.go`, `quality.go`, `transfigure.go`, `velocity.go`, `market_context.go`, `temporal*.go`, `collective.go`, `dedication.go`, `cache.go`, `throttler.go`).

`repository.go` is 1349 lines and exactly 28 methods on one receiver, writing 7 distinct tables. `cache.go` carries 30 methods. The package name describes neither half accurately, which makes every reference to "the lab package" ambiguous.

## 3. Module import graph

Built from `go list -f '{{.ImportPath}}|{{.Imports}}'`. **Zero cycles** — but Go's compiler forbids import cycles structurally, so this axis can never fire and is not evidence of design quality.

```
db, device, mercure, price/gemcolor  → (leaves)
trade                                 → mercure
lab                                   → mercure, trade
collector                             → mercure, price/gemcolor
server/middleware                     → device
server/handlers                       → collector, device, lab, mercure, middleware, trade
server                                → + handlers
cmd/*                                 → composition roots
```

Ten edges. No `Shared`/`Common`/`util` sink exists, so the mediation-hub failure mode is structurally absent. The concentration risk is different: `internal/lab` is simultaneously the largest package and the highest fan-in among non-leaf packages, with no published interface. It also imports `internal/trade` concrete types directly (`gem_features.go:17-250`, `analyzer.go:18,31`) — legal as a with-the-grain read dependency, recorded as observation.

## 4. Data boundaries

Every `INSERT`/`UPDATE`/`DELETE` across non-test Go, mapped to owning package:

| Owner | Tables |
|---|---|
| `internal/collector` | `gem_snapshots`, `currency_snapshots`, `fragment_snapshots` |
| `internal/lab` | `transfigure_results`, `quality_results`, `font_snapshots`, `dedication_snapshots`, `market_context`, `gem_features`, `gem_signals`, `lab_layouts` |
| `internal/device` | `devices` |
| `internal/trade` | `trade_lookups` |
| `internal/price/gemcolor` | `gem_colors` |
| **`internal/server/handlers`** | **`lab_runs`, `lab_run_rooms`, `font_sessions`, `font_rounds`** |
| *(none)* | `strategies`, `trend_results` — dead schema, no Go references |

**No table is written by two packages.** Zero multi-writer conflicts — this matters directly for league isolation.

**The gap:** four tables are owned by a delivery-layer package with no repository at all. `internal/server/handlers` contains **23 raw SQL call sites across 4 files** — more than `internal/lab` or `internal/collector`:

| File | Tables | Sites |
|---|---|---:|
| `lab_runs.go` | `lab_runs`, `lab_run_rooms` | 6 |
| `font_session.go` | `font_sessions`, `font_rounds` | 3 |
| `analysis.go` | `fragment_snapshots`, `currency_snapshots` | 6 |
| `snapshots.go` | `gem_snapshots`, `currency_snapshots`, `fragment_snapshots` | 8 |

`analysis.go` and `snapshots.go` hand-write SQL against tables `internal/collector` owns, while `collector.QueryGemSnapshots` sits available. Not because that method is wrong-shaped, but because no convention establishes "add a read method to the owning repository" over "inline it here." One query — `SELECT chaos FROM currency_snapshots WHERE currency_id = 'divine' ORDER BY time DESC LIMIT 1` — is copy-pasted at `cmd/server/main.go:230,301,440` and `internal/server/handlers/analysis.go:793`.

## 5. Security

- **Security headers: none.** No CSP, HSTS, X-Content-Type-Options, Referrer-Policy, or X-Frame-Options anywhere in Go or config. Middleware stack (`internal/server/server.go:68-83`) is RequestID, Logger, SlogRecoverer, CORS, DeviceMiddleware. Desktop `tauri.conf.json:23-25` sets `"csp": null` — flagged P1 in the 2026-04-26 audit, still open.
- **Security audit log: none.** No `audit` references in Go; all logging goes to one `slog` channel with no retention distinction.
- **Object-scope authorization: built, applied inconsistently.** The device system is a genuine primitive — `DeviceMiddleware` validates fingerprint format, upserts, enforces bans, attaches to context. `lab_runs.go:181-247` uses it correctly: rejects nil device with 401, scopes queries by `dev.Fingerprint`. **`font_session.go` does not** — it declares `DeviceID` as a JSON body field (`:16`) and writes `coalesce(body.DeviceID, "unknown")` (`:89`). Device identity comes from the request body rather than the authenticated context, and the endpoint requires no device at all. Two neighbouring handlers, opposite postures.
- `DeviceMiddleware` fails open on DB error (`middleware/device.go:60-70`) — banned devices pass during an outage. Documented and reasoned; recorded as an accepted trade-off, not a finding.
- **25 reachable vulnerabilities** (`govulncheck`): mostly the stale go1.23.12 toolchain, plus `golang.org/x/text` 0.21.0→0.39.0 and `github.com/jackc/pgx/v5` 5.7.4→5.9.2. Same-day fix; do it *before* wiring a gate so the baseline starts at zero.

## 6. Tests

49 files, 691 top-level `Test` functions, 20,644 test lines against 20,900 production lines. Race-enabled and on the deploy path — this is the project's one strong gate. Distribution is the problem:

| Package | Tests | Prod lines |
|---|---:|---:|
| `internal/lab` | 455 | 8955 |
| `internal/collector` | 103 | 1424 |
| `internal/trade` | 43 | 1712 |
| `internal/server/handlers` | **28** | **4151** |
| `internal/device` | 4 | 295 |
| `cmd/*` (8 binaries) | **0 executing** | **3528** |

The package with the most SQL has the worst ratio. `cmd/` is untested entirely. Web frontend: **0 test files** for 7.6k lines. Desktop JS: 1 file, 215 lines, for 15.5k lines of Svelte/TS. Desktop Rust: 73 `#[test]` across 6334 lines. No mutation testing, no coverage floor.

## 7. Client duplication

`desktop/src/` (15,540 lines) and `frontend/src/` (7,607 lines) share 23 basenames. Roughly **78% of the web frontend's entire surface has a same-named, mostly-duplicated desktop counterpart** (~5949 desktop-side lines under a shared basename).

| Status | Count | Examples |
|---|---:|---|
| Byte-identical | 4 | `GemIcon`, `Legend`, `OfferingChart`, `Sparkline` |
| >90% identical | 6 | `FontEV`, `MarketOverview`, `Select`, `SessionQueue`, `SignalBadge`, `BestPlays` |
| Moderately diverged | 3 | `ByVariant` (~50%), `Header` (~61%), `FontEVCompare` (~83%) |
| Heavily diverged | 2 | `Comparator` (~57%; 1582 vs 1430 lines), `InfoTooltip` (34 vs 161) |

`trade-utils.ts` is logically 100% duplicated — character-identical code with two functions declared in swapped order. The April 2026 audit predicted this drift; 17 of 23 have since diverged. Whether a shared package pays for itself is a judgement call, not a mechanical one.

## 8. Conformance against `project-seed` principles

| Principle | Status |
|---|---|
| `gate-parity` | PARTIAL — three named layers ungated (Go static analysis, JS/TS lint, SQL) |
| `ratchet` | ABSENT — no baseline mechanism, no CODEOWNERS, `main` unprotected |
| `module-dag` | PARTIAL — graph clean at 10 edges, but nothing declares or guards it |
| `shared-admission` | HOLDS (accidentally — no sink exists to admit into) |
| `placement-and-naming` | ABSENT — no glossary; `strategies`/`trend_results` ambiguity unresolved |
| `publication` | ABSENT — `internal/lab` exports 100 top-level declarations, no contract |
| `size-gates` | ABSENT — 11 files >800 lines, 24 in a 400–800 band |
| `disposition-protocol` | ABSENT (vacuously — no gate config exists to protect) |
| `data-boundaries` | PARTIAL — write-side ownership clean; 4 tables unowned; reads unenumerable |
| `test-boundaries` | PARTIAL — boundaries fine, distribution badly skewed |
| `security-day1` | ABSENT — all three core gates missing |
| `irreversible-actions` | ABSENT — one raw `confirm()` in the entire codebase |
| `scar-tissue-gates` | PARTIAL — the strongest principle here; see below |
| `runtime-patterns` | ABSENT — 8 entry points, no ADR pinning shape |
| `operational` | PARTIAL — backups run; **no restore has ever been drilled** |
| `rot-catalog` | ABSENT as process; rots #7, #8, #9 observable today |

### Already satisfied without trying — do not spend here

Table single-ownership (zero violations across 22 tables); one-transaction policy (holds by construction); module acyclicity (compiler-enforced); test-tree boundaries (test imports are a strict subset of production imports); schema/code decoupling (no ORM, per ADR-003); migration discipline (timestamped up/down pairs, never edited in place); documentation status classification (this index is better than the gate story).

### Where `scar-tissue-gates` is unusually strong

Mechanized scars naming their incident already exist: `internal/server/mercure_test.go:13` ("locks in the POE-111 hotfix"), `internal/collector/scheduler_test.go:1715`, `internal/lab/tiers_test.go:446-451`, `desktop/src-tauri/src/settings.rs:252,295`, `font_parser.rs:523`, `capture.rs:109`. One of these is cited by name in `OVERLAY-GUIDE.md:29-31` as a regression guard — doc → named test → CI, the principle working.

The gap: `history/overlay-debugging-notes.md` catalogues 15 numbered gotchas, `OVERLAY-GUIDE.md:12-32` promotes 6 to "non-negotiable regression guards", and **exactly one of the six has a mechanical check**. Guards #1 (capability labels) and #6 (no silent catch) are trivially greppable. Separately, the GGG trade-API ban (1616s, fixed in desktop v0.3.0) has a well-tested limiter but **no test, comment, or ADR naming the incident** — a future agent refactoring it has no signal that this code cost a real ban.

### Backup and restore

Backups run nightly to off-host storage. Verification is existence-only — that a file appeared, not that it restores. **No restore drill has been recorded, ever.** No date of last successful restore exists.

This matters because POE-117 specifies `league archive` plus restore verification against a disposable instance with checksum manifests — i.e. it builds the project's first tested restore path as a subtask of a league rollover, under a deadline. Two consequences: (a) that verification should be scoped as the project-wide backup drill, same code and broader mandate at no extra cost; (b) it must start from the known TimescaleDB gotcha that `pg_dump -t <hypertable>` produces **zero rows**, forcing a `COPY (SELECT * FROM ...) TO STDOUT` wrapper — otherwise that failure gets rediscovered under time pressure.

## 9. Where the project sits on `INHERIT.md`

**Between Phase 0 and Phase 1, with Phase 1 uneven and Phase 2 not started.**

- **Phase 0 (map):** ~80% before this document; this closes most of the rest. Still open: the `strategies` / `trend_results` dead-schema question.
- **Phase 1 (characterization tests + CI):** half done, asymmetrically. Go analysis seams are over-covered; three seams are uncovered and all three sit on POE-117's path — `internal/server/handlers` (4151 lines, 28 tests, 23 SQL sites, owns the 4 unowned tables), `cmd/*` (3528 lines, 0 executing tests), and the 21 integration tests that exist but never run. CI exists but does not gate the deploy path.
- **Phase 2 (baseline walls):** not started.
- **Phase 3 (fix-in-context):** informally practiced; prescribed in `AGENTS.md` prose, visible in the `fix(review):` commit series, not enforced.

## 10. Recommended order

Sequencing matters more than content. Adopting a linter *before* a no-growth check produces a baseline nobody burns down — the configuration `ratchet.md` names as worse than no gate.

| # | Action | Cost | Enters baseline |
|---|---|---|---:|
| 1 | Protect `main`; make `quality.yml` gate `deploy.yml` | ~1h, no code | 0 |
| 2 | Fix the 25 vulnerabilities (toolchain + 2 module bumps) | same-day | 0 |
| 3 | Security headers middleware + a route test asserting them | ~40 lines | 0 |
| 4 | `gofmt -l` + `go vet ./...` in `make qa` and CI | ~10 lines | ~0 (unverified) |
| 5 | Fix `font_session.go` device sourcing | ~5 lines | 1 → 0 |
| 6 | Run the 21 integration tests; add a canary so they can't re-orphan | 1 CI job | unknown |
| 7 | Table-ownership manifest + write-side CI grep | ~1h | 0 |
| 8 | Layer-leakage grep gate (raw SQL in handlers) | ~1h | 4 files / 23 sites |
| 9 | `golangci-lint` with a frozen baseline | ~half day | see §11 |
| 10 | Incident-naming comments on rate-limiter tests; mechanize overlay guards #1 and #6 | ~3h | 0 |
| 11 | `scripts/project-stats.sh` — makes this report regenerable | ~1h | n/a |

Items 1–8 and 11 enter with a zero or near-zero baseline and are adoptable immediately. Line-count size gates are the only genuinely expensive item and should wait until the ratchet exists.

**Deliberately not recommended:** `go-arch-lint` layer/module configuration, or any hexagonal-shape rule. Per `INHERIT.md` Phase 2, imposing either now would baseline all 214 Go files as permanent noise and fight the existing flat structure instead of guarding it. Revisit only if a real module boundary gets drawn — the `internal/lab` dual-domain split would be the natural first candidate. God-files, complexity outliers, and client duplication are **fix-in-context** work per Phase 3, not dedicated paydown tickets.

## 11. Go toolchain — verified facts

All verified July 2026 against current documentation, not recalled.

- **Consolidates into one `.golangci.yml`:** `staticcheck` (which in golangci-lint v2 absorbed `gosimple` and `stylecheck`), `govet`, `errcheck`, `unused`, `ineffassign`, `gocyclo`, `gocognit`, `cyclop`, `funlen`, `maintidx`, `dupl`, `depguard`, `importas`, `nolintlint`, `gosec`.
- **Cannot consolidate:** `go-arch-lint` (standalone only; there is no architecture linter in golangci-lint's catalogue) and `govulncheck`. Three binaries total, not eight.
- **The critical gap: golangci-lint has no baseline file.** Issue #3356 has been open since 2022, labelled "no decision", and there is no config `include`/`extends`. Two workarounds, not equivalent:
  1. `--new-from-merge-base` — holds in CI (requires `fetch-depth: 0`) and guarantees a green day one, but is line-diff-derived rather than an artifact. Nothing to diff, nothing to count, no "baseline grew" check possible. It also grants silent amnesty: `funlen` reports at the declaration line, so a 484-line function can grow to 900 without tripping as long as line 1 is untouched.
  2. `linters.exclusions.rules` — a checked-in, diffable, per-entry, shrink-only artifact, exactly `ratchet.md`'s model, and CODEOWNERS-protectable. Must be hand-built from a first run.

  Ship (1) to go green, treat converting to (2) as the Phase 2 obligation. This is the largest parity gap versus PHPStan's baseline and is not currently stated in the seed.
- **`govulncheck` has no suppression at all** (documented; `go.dev/issue/61211`). The only workable baseline is `-format json` plus a checked-in OSV-ID allowlist.
- **`go-arch-lint` has no baseline either** — but that is *good* for the ratchet: the config **is** the baseline, so "fail on growth" becomes "diff the `deps:` block", which is precisely the edge-accretion counter `module-dag.md` wants.
- **Two corrections to the seed's Go column.** *Publication marker* is not "no clean marker": Go's nested `internal/` is compiler-enforced **at any depth**, so `internal/lab/internal/store` is a per-module published surface with zero tooling and no baseline — directory-granular rather than symbol-granular, but sufficient. *Mutation testing* is no longer "weak ecosystem": `gremlins` shipped v0.6.0 in December 2025 with commits through March 2026 and supports a floor-ratchet; thin, but not absent.
- `nolintlint` with `require-explanation` and `require-specific` mechanically enforces `size-gates.md`'s tier-2 escape rule, for free.

## 12. Unverified

- Whether the 21 integration tests still pass — no Go toolchain and no running stack on the audit host.
- Coverage numbers — `go test -cover` was not executed.
- `gofmt`/`go vet` violation counts — expected near zero, not measured.
- Whether a Go-1.25-built `go-arch-lint` correctly loads a `go 1.23` module.
