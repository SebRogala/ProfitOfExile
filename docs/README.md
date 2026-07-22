# ProfitOfExile Documentation

This is the documentation entry point. Documents are classified so historical plans and proposed designs are not mistaken for current behavior.

## Start here

- [Project README](../README.md) — product overview, stack, and development commands.
- [Product vision](product-vision.md) — historical strategy-simulation domain and future scope; not current architecture.
- [Trade and Market Data Lifecycles](TRADE-LIFECYCLE.md) — current workflows plus clearly labeled reliability targets for collection, native trade, contributions, optional server trading, pairing, and Mercure.
- [Overlay Guide](OVERLAY-GUIDE.md) — maintained Windows/Tauri overlay mechanics and regression guards.
- [Collector Endpoint Guide](COLLECTOR-ENDPOINTS.md) — current cross-layer recipe for adding a market-data source.
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

Superseded:

- [ADR-002: Hexagonal/CQRS vertical-slice proposal](adr/002-internal-architecture-hexagonal-cqrs-vertical-slice.md) — superseded by ADR-008 after implementation evolved into flat feature packages.

ADRs record decisions at a point in time. If implementation later supersedes a decision, add a new ADR and update the earlier status rather than silently rewriting history.

## Current guides and narratives

- [Trade and Market Data Lifecycles](TRADE-LIFECYCLE.md) — mixed current/target guide with per-section labels.
- [Overlay Guide](OVERLAY-GUIDE.md) — current click-through, positioning, lifecycle distinctions, and Windows regression guards.
- [Collector Endpoint Guide](COLLECTOR-ENDPOINTS.md) — current endpoint extension procedure, verified against the fragments implementation.
- [Historical overlay debugging notes](history/overlay-debugging-notes.md) — preserved runtime discoveries and obsolete implementation generations; not a current recipe.
- [AI-Native Case Study](AI-NATIVE-CASE-STUDY.md) — public project/portfolio narrative, not an implementation contract.

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
