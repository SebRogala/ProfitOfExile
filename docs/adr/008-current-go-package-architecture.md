---
uid: 723f68df-5208-4743-ae8f-e6de4013e7ff
---

# ADR-008: Current Go Package Architecture

## Status

Accepted; supersedes ADR-002.

## Enforcement

Tier: required

Scope: internal/ Go feature packages, HTTP handlers and dependency boundaries.

Summary: Keep behavior in its owning flat feature package and HTTP transport in internal/server; add abstractions for concrete boundaries.

## Context

ADR-002 proposed vertical modules split into `domain`, `application`, and
`infrastructure` subpackages. The implementation evolved into flat feature
packages: `collector`, `db`, `device`, `lab`, `mercure`, `price`, `server`, and
`trade` under `internal/`.

The proposed `internal/simulation` package and most three-layer module trees
were never implemented. Describing that layout as current causes contributors
and agents to create abstractions and paths that do not match the repository.

## Decision

The existing feature packages are the current package architecture.

- Add behavior to the package that owns the feature or lifecycle.
- Keep HTTP transport under `internal/server` and reusable behavior in the
  owning feature package.
- Introduce an interface or subpackage when a concrete dependency boundary or
  package-size problem requires it, not to reproduce the superseded template.
- Preserve direct pgx repositories and the other decisions recorded by their
  respective ADRs.
- Give a future simulation engine its own design decision based on current
  requirements rather than inheriting ADR-002 automatically.

## Consequences

- Documentation and agent guidance match the code contributors modify.
- Package boundaries remain simpler, but discipline is required to avoid
  unnecessary coupling between HTTP, persistence, and domain calculations.
- ADR-002 remains decision history and may inform a targeted refactor, but it is
  not the default structure for new work.

## Evidence

- Current directories under `internal/`.
- Composition in `cmd/server/main.go` and `cmd/collector/main.go`.
- Router and handlers under `internal/server`.
- [Historical rewrite baseline](../history/architecture-rewrite-2026-03-12.md).

## References

- [ADR-026](026-price-tiering-is-one-leaf-package-callers-own-eligibility-and-cutoff.md) — the package list gains `internal/price/tier`, justified by the exchange import boundary
