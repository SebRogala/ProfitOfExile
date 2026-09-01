# ProfitOfExile Documentation

This is the documentation entry point. Documents are classified so historical plans and proposed designs are not mistaken for current behavior.

## Start here

- [Project README](../README.md) — product overview, stack, and development commands.
- [Product vision](product-vision.md) — historical strategy-simulation domain and future scope; not current architecture.
- [Trade and Market Data Lifecycles](TRADE-LIFECYCLE.md) — current workflows plus clearly labeled reliability targets for collection, native trade, contributions, optional server trading, pairing, and Mercure.
- [Overlay Guide](OVERLAY-GUIDE.md) — maintained Windows/Tauri overlay mechanics and regression guards.
- [Collector Endpoint Guide](COLLECTOR-ENDPOINTS.md) — current cross-layer recipe for adding a market-data source.
- [Gem and Item Icons](GEM-ICONS.md) — current procedure for adding or changing an icon, and why seeding precedes deploy.
- [Deployment](DEPLOY.md) — how main reaches production, why the deploy is path-filtered, and what a green pipeline does not tell you, desktop release channels (stable / beta by device role), the public-repo rules for beta testers, and the one-off POE-215 merc registration-reset runbook.
- [League Schema Migration Runbook](LEAGUE-SCHEMA-MIGRATION-RUNBOOK.md) — current production gate and rehearsal procedure for POE-119; requires the matching POE-120/POE-121 application revision.
- [Currency Exchange row invariant](CURRENCY-EXCHANGE-ROW-INVARIANT.md) — current normative spec for what one exchange row's numbers mean: the one scale every figure counts, the one price basis they are quoted at, and the single labeled deviation from it.
- [Architecture decisions](adr/) — accepted and superseded architecture decisions.

## Proposed specifications

Tracker task and epic descriptions are canonical for active specifications. They
are not mirrored into this repository. Three proposed, unimplemented
specifications currently describe planned behavior:

- `POE-117` — League SSOT, historical isolation, and safe rollover.
- `POE-118` — Mercure lifecycle reliability and event coordination.
- `POE-88` — LabCompass fidelity restoration and overlay SSOT.

The three are interdependent: league identity scopes events and data, Mercure defines notification and coordination behavior, and the Compass specification defines Rust-owned desktop lab state. `POE-118` depends on both `POE-117` and `POE-88`.

## Accepted architecture decisions

- [ADR-001: Go module path](adr/001-go-module-path.md)
- [ADR-003: Direct pgx, no ORM](adr/003-no-orm-direct-pgx-queries.md)
- [ADR-004: Database migration strategy](adr/004-database-migration-strategy.md)
- [ADR-005: Gem snapshot row model](adr/005-gem-snapshots-unified-row-model.md)
- [ADR-006: Database-backed gem colors](adr/006-gem-colors-db-backed-upsert-table.md)
- [ADR-007: Unified analysis pipeline](adr/007-v3-hybrid-analysis-unified-pipeline.md)
- [ADR-008: Current Go package architecture](adr/008-current-go-package-architecture.md)
- [ADR-009: League-scoped repository convention](adr/009-league-scoped-repository-convention.md)
- [ADR-010: Archived league history is retained indefinitely](adr/010-archived-league-history-is-retained-indefinitely.md)
- [ADR-011: Wipe the outgoing league at rollover; preserve it as a dump](adr/011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md)
- [ADR-012: Icons are pre-seeded from an allowed IP and content-addressed](adr/012-icons-are-pre-seeded-from-an-allowed-ip-and-cached-by-content-address.md)
- [ADR-013: UI picks persist in a schema-less prefs map](adr/013-ui-picks-persist-in-a-schema-less-prefs-map.md)
- [ADR-014: Desktop features are modules with a work toggle and a view page](adr/014-desktop-features-are-modules-with-a-work-toggle-and-a-view-page.md)
- [ADR-015: Exchange quality gates live client-side; the server serves everything sane](adr/015-exchange-quality-gates-live-client-side-the-server-serves-everything-sane.md)
- [ADR-016: Expected ROI is a cross-hour simulation; displayed prices stay single-hour](adr/016-expected-roi-is-a-cross-hour-simulation-displayed-prices-stay-single-hour.md)
- [ADR-017: No default engine floor may hide a live market](adr/017-no-default-engine-floor-may-hide-a-live-market.md)
- [ADR-018: Flags mark; they never order](adr/018-flags-mark-they-never-order.md)

Superseded:

- [ADR-002: Hexagonal/CQRS vertical-slice proposal](adr/002-internal-architecture-hexagonal-cqrs-vertical-slice.md) — superseded by ADR-008 after implementation evolved into flat feature packages.

ADRs record decisions at a point in time. If implementation later supersedes a decision, add a new ADR and update the earlier status rather than silently rewriting history.

## Current guides and narratives

- [Trade and Market Data Lifecycles](TRADE-LIFECYCLE.md) — mixed current/target guide with per-section labels.
- [Overlay Guide](OVERLAY-GUIDE.md) — current click-through, positioning, lifecycle distinctions, and Windows regression guards.
- [Collector Endpoint Guide](COLLECTOR-ENDPOINTS.md) — current endpoint extension procedure, verified against the fragments implementation.
- [Gem and Item Icons](GEM-ICONS.md) — current icon map, cache-seeding order, and the puller/repopulate steps.
- [Deployment](DEPLOY.md) — current deploy workflow, filter derivation, manual-dispatch cases, and the accepted verification gap.
- [Analysis Cache Guide](ANALYSIS-CACHE.md) — current `lab.Cache` topology, tick chain, tenancy and concurrency contract, cold start, and the sparkline series cache.
- [Currency Exchange row invariant](CURRENCY-EXCHANGE-ROW-INVARIANT.md) — current normative spec for the exchange row: the invariant equations, the rendering rules, the re-affirmed exemptions, and the closure-test enforcement tiers.
- [Historical overlay debugging notes](history/overlay-debugging-notes.md) — preserved runtime discoveries and obsolete implementation generations; not a current recipe.
- [AI-Native Case Study](AI-NATIVE-CASE-STUDY.md) — public project/portfolio narrative, not an implementation contract.

## Codebase state

- [Codebase State Report — 2026-07-22](STATE-REPORT-2026-07-22.md) — measured gate inventory, size/complexity distribution, module import graph, table-ownership map, and `project-seed` principle conformance. Dated measurement; re-run the cited commands rather than trusting the numbers after code changes.

## Dated research

- [PoE Trade API Research — 2026-03-16](RESEARCH-poe-trade-api.md) — historical research notes. Verify unstable API/policy facts against current official documentation before implementation.
- [Gem Market Findings — March 2026](research/market-findings-2026-03.md) — preserved seven-day research observations without a reproducible query artifact; rerun before using as current truth.

## Historical architecture, designs, and plans

These are retained as implementation history. They may contain obsolete paths, APIs, topology, or incomplete work and must not override current code, accepted ADRs, or active proposed specifications.

- [Architecture rewrite baseline — 2026-03-12](history/architecture-rewrite-2026-03-12.md)
- [Live dashboard plan](PLAN-live-dashboard.md)
- [Frontend design](FRONTEND-DESIGN.md)
- [Codex audit — 2026-04-26](CODEX-AUDIT-2026-04-26.md)
- [Session tracker design](superpowers/specs/2026-03-16-session-tracker-design.md)
- [Trade API integration design](superpowers/specs/2026-03-16-trade-api-integration-design.md)
- [Desktop screen-reader proof of concept](superpowers/specs/2026-03-27-desktop-screen-reader-poc-design.md)
- [Desktop app-shell design](superpowers/specs/2026-03-28-desktop-app-shell-design.md)
- [Unified tier implementation plan](superpowers/plans/2026-03-25-unified-tier-system.md)
- [Comparator overlay plan](superpowers/plans/2026-03-28-comparator-overlay.md)
- [Desktop app-shell implementation plan](superpowers/plans/2026-03-28-desktop-app-shell.md)
- [Desktop dashboard migration plan](superpowers/plans/2026-03-28-desktop-dashboard-migration.md)

## Third-party documentation

Do not vendor copies of third-party documentation into this repository. Verbatim HTML snapshots of the official Path of Exile developer documentation were removed on 2026-07-22: they carried no provenance metadata, redistributed copyrighted content from a public repository, and went stale. Read upstream documentation at its source and record the distilled, dated findings in a research document such as [the PoE trade API research](RESEARCH-poe-trade-api.md).

## Documentation maintenance rules

Every new architecture/design document should state:

- Status: current, accepted, proposed/unimplemented, historical, superseded, or dated research.
- Last verified date where factual behavior is described.
- What the document is canonical for.
- Which document supersedes it, when applicable.

Documentation changes should run a relative-link check in CI. Public documentation must not contain credentials, real private endpoints, production identifiers, pairing capabilities, device fingerprints, or backup locations.
