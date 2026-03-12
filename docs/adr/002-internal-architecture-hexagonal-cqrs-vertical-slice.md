# ADR-002: Internal Architecture: Hexagonal + CQRS + Vertical Slice

## Status

Accepted

## Context

The Go rewrite of ProfitOfExile requires a clear internal package structure that can accommodate multiple feature domains (price engine, lab farming, simulation engine) while keeping each domain's code cohesive and its boundaries explicit. The structure must also be comprehensible to developers familiar with idiomatic Go, which tends to favor flat packages.

The original PHP/Symfony codebase used a domain-driven structure with Symfony's DI container, Doctrine ORM, and event dispatching. The Go rewrite must achieve similar separation of concerns without those framework affordances.

Key considerations:

1. Each feature domain (price, lab, simulation) needs to own its types, business logic, storage contracts, and external-system adapters independently — leaking these across domains creates coupling that makes the codebase harder to reason about
2. The read path (serving cached results to the frontend) is significantly simpler than the write path (fetching, normalizing, and storing prices) — conflating them into a single "service" layer obscures this asymmetry
3. Domain logic must be testable without a database or HTTP client — this requires explicit interface boundaries (ports) between business logic and infrastructure
4. The project is inspired by the [cresco](https://github.com/SebRogala/cresco) PHP framework, whose patterns are being adapted to Go idioms

## Decision

Adopt a **Hexagonal Architecture + CQRS + Vertical Slice** structure:

### Module layout

Each feature domain is a vertical slice under `internal/`:

```
internal/
├── {module}/
│   ├── domain/          # Types, domain logic, port interfaces
│   ├── application/     # UseCases (writes) + Query handlers (reads)
│   └── infrastructure/  # Adapters: HTTP clients, postgres repos
└── server/              # Thin HTTP layer: routing, handlers, scheduler
```

### CQRS separation within each module

- **Write path**: `application/{use_case}.go` — one struct per write operation with `Execute(Req) (Resp, error)`
- **Read path**: `application/{query}.go` — query handlers that read directly from the repository, bypassing domain model construction

### Ports & Adapters

- Domain defines interfaces (ports): `domain/repository.go`, `domain/datasource.go`
- Infrastructure implements them: `infrastructure/postgres_repo.go`, `infrastructure/ninja_client.go`
- Domain layer has zero imports from `infrastructure/`

### Cross-module communication

No event bus. Modules communicate via simple hook registration in the scheduler:

```go
scheduler.OnPriceUpdate(func(book *price.PriceBook) {
    labService.RecomputeAnalysis(book)
})
```

### Manual dependency injection

No DI container. Dependencies are wired explicitly in `cmd/server/main.go`.

## Consequences

### Positive

- Domain logic is fully isolated and testable without infrastructure (database, HTTP)
- Read and write paths are explicitly separated, making the performance and complexity asymmetry visible
- Each vertical slice is independently deployable as a unit — adding a new feature domain means adding a new `internal/{module}/` directory with no changes to existing modules
- Familiar pattern for developers who know hexagonal architecture; differences from cresco are documented in ARCHITECTURE.md

### Negative

- More directories and files upfront compared to a flat package approach — the structure pays off as the codebase grows, but adds overhead for the initial scaffold
- Manual DI wiring in `main.go` grows as modules are added; no auto-wiring

## Alternatives Considered

### Flat package layout

```
internal/
├── domain/
├── handler/
├── service/
├── repository/
└── pricefeed/
```

**Rejected because**: All domains share the same `service/` and `repository/` layers, creating implicit coupling. Adding the simulation module would require touching existing layers. The flat layout also obscures the read/write asymmetry that CQRS makes explicit.

### Standard layered n-tier (Controller → Service → Repository)

A classic three-layer architecture where all features share layers.

**Rejected because**: The service layer becomes a catch-all that grows unbounded. Repository interfaces end up aggregating all persistence operations across features. This was the weakness of early versions of the PHP codebase that cresco was designed to correct.

### Go-idiomatic flat packages (one package per concept)

Following Go standard library style: small, focused packages like `price`, `lab`, `gem`, etc., with no internal layer distinction.

**Rejected because**: Without explicit layer boundaries, infrastructure concerns (SQL queries, HTTP clients) bleed into domain types. This makes unit testing the domain logic harder and makes the dependency direction implicit rather than enforced.

## References

- [ARCHITECTURE.md](../../ARCHITECTURE.md) — Full structural specification, module layout, request flow diagrams
- [ADR-001](001-go-module-path.md) — Module path decision (foundational to all import paths)
- [POE-13](https://softsolution.youtrack.cloud/issue/POE-13) — Go module init + project scaffold
- [POE-2](https://softsolution.youtrack.cloud/issue/POE-2) — Foundation epic that this architecture serves
