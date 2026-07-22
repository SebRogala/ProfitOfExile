# Codex Audit — ProfitOfExile

> **Status: Historical audit snapshot.** Findings reflect 2026-04-26 and some have since been resolved or changed, including CI coverage. Re-verify every finding against current code before acting on it.

**Date:** 2026-04-26
**Scope:** Engineering audit + AI-native portfolio/readiness assessment
**Context:** Public project used as evidence for AI-native product engineering. Built through Claude Code + Pipeforge by an owner who did not previously know Go/Tauri.

---

## Executive Summary

ProfitOfExile is strong evidence for an AI-native Product Engineer role because it is not a toy. It has a real product surface, public release artifacts, production deployment, a Go backend, SvelteKit web app, Tauri desktop app, PostgreSQL/TimescaleDB, Mercure events, CI/CD, and substantial domain logic.

The backend is the strongest part of the project. It has meaningful package separation, direct SQL, domain-heavy analysis modules, good test density, race-enabled test coverage, and production-minded operational patterns.

The weakest part is not product ambition or backend engineering. The weakest part is validation coverage across the frontend/desktop surfaces and some signs of agent-generated accumulation: large components, duplicated web/desktop UI, stale or incomplete JS tooling, broad Tauri permissions/CSP, and docs/stats that need refreshing.

As portfolio evidence, the project is credible, but it should be packaged more deliberately:

- show what is live
- show what users do with it
- show the AI-native delivery workflow
- show the verification gates
- show what risks were found and fixed

The repo currently proves "AI can build a complex product." To be stronger for hiring, it should also prove "I know where the risks are, how I verify them, and how I harden them."

## Verification Performed

Commands run:

```bash
npm run build                         # frontend
docker compose exec app go test ./...
docker compose exec app go test -race ./...
docker compose run --rm -w /app/desktop desktop npm run check
docker compose run --rm -w /app/desktop desktop npm test
docker compose run --rm -w /app/desktop/src-tauri desktop cargo test
```

Results:

- Frontend production build: **passes**, with Svelte accessibility/CSS warnings.
- Go tests: **pass**.
- Go tests with race detector: **pass**.
- Desktop Rust tests: **pass**: 58 tests.
- Desktop Svelte check: **fails** with 6 errors and 2 warnings.
- Desktop JS tests: **fail** because `vitest` is not installed/declared.

Local environment note:

- Local `go` binary is not installed; project correctly relies on Docker for Go workflows.
- Docker daemon access required elevated permission from Codex sandbox.

## What Looks Strong

### 1. Real Product Surface

This is a full product, not a code sample:

- public web landing/dashboard
- desktop installer/release flow
- Discord/user-support angle
- live market-data ingestion
- real-time analysis
- OCR/overlay desktop workflow
- price/risk modeling for an actual niche user problem

That matters for the Appliscale-style role. The project shows product thinking, not only engineering machinery.

### 2. Backend Test and Domain Coverage

The Go backend has broad package-level tests across:

- collector scheduling/fetching/storage
- DB migration behavior
- lab analysis models
- trend/velocity/risk scoring
- trade cache/gate/rate limiter
- server routing/handlers/middleware
- Mercure publishing

Both normal and race-enabled Go tests pass. This is a strong quality signal.

### 3. Architecture Has Real Boundaries

The backend has identifiable modules:

- `internal/collector`
- `internal/lab`
- `internal/trade`
- `internal/server`
- `internal/db`
- `internal/device`
- `internal/price/gemcolor`

The server router is explicit and dependency-injected via `RouterConfig`, which makes tests and optional features easier to reason about.

### 4. Operational Thinking Is Visible

Examples:

- server auto-migrates and fails fast
- collector does not own migrations
- Mercure decouples collector/server/browser/desktop events
- admin recompute was removed from HTTP and moved to CLI/Mercure
- trade API has gate/cache/rate-limiter logic
- desktop releases are tagged and bundled through GitHub Actions

This reads like an engineer who thinks about production, not only implementation.

### 5. AI-Native Evidence Is Strong

This repo plus Pipeforge tells a coherent story:

- large unfamiliar stack
- built through AI orchestration
- real users/public surface
- tests and heavy-review process
- production deployment
- domain-specific agents and prompts
- multi-worktree workflow

This is directly relevant to AI-native Product Engineer / DevEx / Forward Deployed roles.

## High-Priority Issues

### P0 — Desktop JS Validation Is Broken

`docker compose run --rm -w /app/desktop desktop npm run check` fails.

Errors observed:

- missing `@tauri-apps/plugin-updater` type/module resolution
- missing `@tauri-apps/plugin-process` type/module resolution
- missing `vitest` type/module resolution in `navigation.test.ts`
- `RunHistoryPage.svelte` references `store.status?.app_version`, but `AppStatus` does not expose `app_version`
- implicit `any` on updater progress callback

`docker compose run --rm -w /app/desktop desktop npm test` also fails:

```text
sh: 1: vitest: not found
```

Action:

1. Add `vitest` to `desktop/devDependencies`.
2. Ensure desktop Docker/node volume installs current `desktop/package-lock.json`.
3. Fix `AppStatus` type to include `app_version` or stop reading it.
4. Fix updater/process module resolution.
5. Add desktop `npm run check` and `npm test` to CI.

Why it matters:

The desktop app is the most impressive product surface, but its TypeScript/Svelte validation currently cannot be trusted.

### P0 — CI Does Not Cover the Full Product

Current GitHub server workflow runs only:

```bash
go test ./...
```

It does not run:

- `go test -race ./...`
- frontend `npm run build`
- desktop `npm run check`
- desktop `npm test`
- desktop `cargo test` except indirectly during tag build flow
- Docker image build verification for server/collector on PR-like changes

The desktop workflow runs on `v-desktop-*` tags, which is late. It builds a release, but does not protect normal commits from desktop regressions.

Action:

Add a non-release CI workflow:

```text
backend:
  go test -race ./...

frontend:
  cd frontend
  npm ci
  npm run build

desktop-web:
  cd desktop
  npm ci
  npm run check
  npm test

desktop-rust:
  cd desktop/src-tauri
  cargo test
```

Why it matters:

For "AI-native but high quality", CI must prove the generated code is continuously validated across every runtime.

### P1 — Desktop Tauri Security Surface Needs Hardening

`desktop/src-tauri/tauri.conf.json` has:

```json
"security": {
  "csp": null
}
```

Capabilities also grant broad window/webview permissions across many windows, plus shell open and restart.

This may be acceptable during rapid iteration, but as a public desktop app it should be reviewed.

Action:

1. Add a real CSP suitable for local app assets and required external requests.
2. Split Tauri capabilities by window where possible.
3. Keep overlay windows on the minimum permissions they need.
4. Document why each capability exists.
5. Add a desktop security audit checklist before each release.

Why it matters:

Public desktop apps are held to a higher bar than websites. The "I did not read generated code" thesis needs compensating security gates.

### P1 — Frontend Build Has Accessibility Warnings

Frontend build passes, but warns about:

- clickable `<img>` elements without keyboard handlers
- clickable lightbox `<div>` without keyboard handler
- `InfoTooltip.svelte` role/button mismatch
- unused CSS selector in Comparator

Action:

Convert clickable images/lightbox controls to accessible buttons or add keyboard handlers and labels. Clean unused CSS.

Why it matters:

These are not deep bugs, but they are visible evidence of polish and quality standards. They are good low-cost fixes.

### P1 — Web/Desktop UI Duplication Is High

The web and desktop apps duplicate many lab components:

- `BestPlays.svelte`
- `ByVariant.svelte`
- `Comparator.svelte`
- `FontEV.svelte`
- `FontEVCompare.svelte`
- `GemIcon.svelte`
- `MarketOverview.svelte`
- `SessionQueue.svelte`
- `SignalBadge.svelte`
- `Sparkline.svelte`
- tooltip/trade/api utilities

This is understandable because the desktop app has different runtime constraints, but duplicated components will drift.

Action:

1. Extract shared pure utilities first: trade URL/query helpers, tooltip dictionaries, formatting helpers.
2. Then extract shared visual components only where runtime differences are small.
3. For large components like `Comparator.svelte`, split domain logic from rendering before trying to share.

Why it matters:

Duplication is likely to become the biggest maintenance tax as the app matures.

## Medium-Priority Issues

### P2 — Large Components and Files Need Seams

Large files:

- `desktop/src-tauri/src/lib.rs`: ~2,553 lines
- `frontend/src/routes/+page.svelte`: ~1,140 lines
- `frontend/src/routes/lab/components/Comparator.svelte`: ~1,434 lines
- `desktop/src/routes/(app)/components/Comparator.svelte`: ~1,582 lines
- `cmd/backtest/main.go`: ~908 lines
- `internal/server/handlers/analysis.go`: ~1,587 lines

These are not automatically bad, but they make review, chunking, and regression detection harder.

Action:

- Split Tauri `lib.rs` by command/overlay/tray/window domain.
- Split Comparator into query state, result transformation, UI panels, and overlay bridge.
- Split landing page into sections/components.
- Treat `cmd/backtest/main.go` as an internal analysis tool, but isolate reusable backtest logic if it keeps growing.

### P2 — Rust Warnings Should Be Cleaned

`cargo test` passes but emits warnings:

- unused imports
- unused variables
- unused functions
- unnecessary `mut`

Action:

Run `cargo fix` where safe, and make future CI fail on warnings for desktop Rust once current warnings are cleaned.

### P2 — Public Evidence Needs a Case Study Page

The README is good technically, but the hiring story needs a concise case study.

Add:

```text
docs/AI-NATIVE-CASE-STUDY.md
```

Include:

- product problem
- user workflow
- architecture diagram
- AI-native delivery workflow
- verification gates
- what failed and how it was hardened
- metrics: tests, releases, commits, LOC, users/support channel
- screenshots

Why it matters:

Hiring reviewers will not reverse-engineer the story from the repo. Give them the story.

### P2 — README Stats Need a Refresh Contract

README says:

- ~107k source LOC
- 886 tests
- 39 migrations
- 11 desktop releases

Current rough scan found:

- ~70k lines across Go/Svelte/TS/Rust in `cmd`, `internal`, `frontend/src`, `desktop/src`, `desktop/src-tauri/src`
- 82 SQL migration files including up/down pairs and migration tests
- Rust desktop test count: 58
- Go test command passes, but the command output does not expose a total test count

The README numbers may still be valid depending on counting method, but they should be generated or documented.

Action:

Add a stats script:

```bash
scripts/project-stats.sh
```

It should print the same metrics the README claims.

## Portfolio Assessment Against AI-Native Product Engineer Role

The Appliscale role asks for someone who:

- ships AI-native software features end-to-end
- uses AI tools daily
- works from specs and plans
- improves prompt systems, workspace rules, and AI workflows
- selects the right AI tool per stage
- maintains quality through tests/reviews/monitoring
- collaborates across product/business/engineering
- has MCP and multi-agent familiarity

ProfitOfExile + Pipeforge is unusually aligned with that.

Strong evidence:

- The product exists and is public.
- It spans backend, frontend, database, deployment, and desktop.
- You used AI to cross into unfamiliar stacks.
- Pipeforge shows the workflow was systematic, not random prompting.
- The backend/race tests passing gives a quality anchor.
- The desktop overlay/OCR workflow is product-specific and non-trivial.

Weak evidence / likely interviewer concerns:

- Desktop JS validation currently fails.
- CI does not yet prove the full stack.
- Some files look like typical AI-generated accumulation: large components, duplicated UI, stale tooling.
- "I do not read generated code" will sound risky unless paired with hard verification gates.
- Public repo has no single "AI-native case study" that explains the accomplishment quickly.

Best framing:

```text
I used AI to build a production-grade product in stacks I did not previously know,
but I did not rely on trust. I built an orchestration and verification system around it:
planned chunks, specialized implementers, heavy review, tests, race checks, CI, and
post-failure audit tooling. ProfitOfExile is the product proof; Pipeforge is the
delivery-system proof.
```

Do not frame it as "AI wrote everything and I do not look at code" without immediately explaining the verification model. The stronger frame is:

```text
I moved human judgment from line-by-line authorship to intent, architecture, product fit,
and verification design.
```

## Recommended Next Steps

### Immediate

1. Fix desktop `npm run check`.
2. Add `vitest` and make `desktop npm test` pass.
3. Add CI for frontend build, desktop check/test, desktop Rust tests.
4. Fix frontend accessibility warnings from production build.
5. Add `docs/AI-NATIVE-CASE-STUDY.md`.

### Next

6. Add a stats script and refresh README metrics from it.
7. Split the largest desktop/web files where review boundaries are obvious.
8. Add Tauri CSP and capability review.
9. Extract shared web/desktop utilities.
10. Add screenshots/GIFs of the product workflow to README or docs.

### Later

11. Add Playwright smoke tests for the web dashboard.
12. Add desktop smoke/release checklist.
13. Add observability docs: what is logged, monitored, and manually checked in production.
14. Add a public "Architecture + AI workflow" write-up for job applications.

## Final Rating

As a product artifact: **7.5/10**.

As AI-native evidence: **8.5/10**.

As a public hiring artifact today: **7/10**, mostly because the story is not packaged and the desktop JS/CI gaps are easy for a skeptical reviewer to criticize.

With the immediate fixes and a short case study, this becomes very strong evidence for an AI-native Product Engineer application.
