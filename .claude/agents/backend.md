# Backend Agent

Server-side implementation principles. Extends the general agent with Go architecture and API design conventions.

## Architecture Patterns

ARCHITECTURE.md is the single source of truth for structural decisions (directory layout, module boundaries, tech stack choices).

- **Hexagonal + CQRS + UseCase**: The project follows ports-and-adapters architecture with separated write (UseCase) and read (Query/Handler) paths.
- Follow the project's package structure: `cmd/server/` for entrypoint, `internal/` for vertical slice modules.
- Each module (`internal/{module}/`) contains three layers: `domain/`, `application/`, `infrastructure/`.
- Domain invariants belong in domain types, not in handlers. A type should never be in an invalid state after construction.
- **UseCases** (in `application/`) orchestrate write operations — one struct per operation with `Execute(req) (resp, error)`. Query handlers in `application/` handle reads.
- **Infrastructure** (in `infrastructure/`) contains adapters: HTTP clients, PostgreSQL repositories, external service integrations. Domain never imports infrastructure.
- Use interfaces (ports) defined in `domain/` at module boundaries. Concrete implementations live in `infrastructure/`.
- **Cross-module hooks**: Use simple observer pattern for inter-module communication (e.g., price update triggers lab recomputation). No event bus.

## Go Conventions

- **Go 1.22+**: Use chi router for HTTP routing (`github.com/go-chi/chi/v5`). See `go.mod` and `internal/server/server.go`.
- **Error handling**: Return errors, don't panic. Wrap with `fmt.Errorf("context: %w", err)` for chain preservation.
- **Naming**: Follow Go conventions — short receiver names, unexported by default, exported only at package API boundaries.
- **Constructors**: Use `New*` functions that validate invariants and return `(*T, error)` when construction can fail.
- **Enums**: Use typed string/int constants with `iota` where appropriate. Provide `String()` and `Valid()` methods.

## Database (PostgreSQL via pgx)

- Direct SQL queries via `pgx` — no ORM.
- Use parameterized queries (`$1`, `$2`) for all user/external input. Never string-concatenate SQL.
- JSONB columns for strategy trees and price data. Use `pgtype.JSONB` or custom `Scan`/`Value` implementations.
- New columns on existing tables should be nullable with sensible defaults for zero-downtime migrations.
- Migration files should be self-contained with literal values — don't reference Go constants.

## API Design

- Validate input at the handler level. Domain code receives clean, typed data.
- Use domain-specific error types, not generic HTTP errors, in service layers. Map domain errors to HTTP responses in handlers.
- Design write operations for idempotency where possible.
- Return consistent JSON error response structures with enough context for the caller.

## Data Integrity

- Prefer database constraints (unique, foreign key, check) over application-level validation for critical invariants.
- Application validation provides user feedback; database constraints prevent corruption.
- When deleting or archiving records, consider referential integrity. Soft deletes are safer than hard deletes for entities with relationships.

## Package Structure

```
cmd/
└── server/                    # main.go entrypoint
internal/
├── price/                     # Vertical slice: Multi-Source Price Engine
│   ├── domain/                # PriceBook, SourcePrice, interfaces (ports)
│   ├── application/           # UseCases: FetchPrices, RefreshPrices; Queries: GetPrices
│   └── infrastructure/        # Adapters: NinjaClient, TftClient, PostgresRepo
├── lab/                       # Vertical slice: Lab Farming Dashboard
│   ├── domain/                # Analysis, Gem, GemColor, GemPool types
│   ├── application/           # UseCases: AnalyzeFont, AnalyzeTransfigure; Queries
│   └── infrastructure/        # Adapters: GemColorResolver, PostgresRepo
├── simulation/                # Vertical slice: Simulation Engine (future)
│   ├── domain/                # Strategy, Inventory, SetConverter, Runner
│   ├── application/           # UseCases: RunSimulation
│   └── infrastructure/        # Adapters: PostgresRepo
└── server/                    # HTTP layer (thin)
    ├── server.go              # chi router, middleware, startup
    ├── handlers/              # HTTP handlers per module
    └── scheduler/             # Background tick + hook registry
```

### Domain Layer Rules

- Domain types are plain Go structs with constructor functions enforcing invariants.
- Domain interfaces (ports) define what infrastructure the domain needs — domain never imports infrastructure.
- Value objects are small immutable types (ItemName, Source, Price).

### Application Layer Rules

- One UseCase struct per write operation with `Execute(req) (resp, error)`.
- Query handlers for reads may skip domain and go directly to repository.
- Application code depends on domain ports, never on infrastructure directly.

### Infrastructure Layer Rules

- Contains adapters that implement domain ports: HTTP clients, database repositories, external APIs.
- Each adapter lives in the module it serves, not in a shared package.

### Handler Layer Rules

- Handlers are thin — parse request, call application layer, encode response.
- Use middleware for cross-cutting concerns (logging, auth, CORS).
- Static file serving via Go `embed` package for the compiled SvelteKit frontend.

## Price Data Sources

- **poe.ninja**: REST API with `chaosEquivalent` for currency/fragments, `chaosValue` for items. 60-min cache TTL.
- Cache prices in PostgreSQL `price_cache` table with TTL-based invalidation.
- **Listing count** is as important as price — it's the leading indicator for saturation and crash risk. Always store and expose listing counts alongside prices.
- **Confidence filtering**: items with <5 listings have unreliable prices. The system should support a minimum listing threshold for strategy calculations.

## Strategy Domain Complexity

Strategies are not just linear "buy input → get output". Real strategies involve:

- **Probabilistic branching**: Font of Divine Skill offers 3 random same-color gems, pick 1. EV depends on pool size and value distribution.
- **Decision optimization**: Lab runs show 4 enchant options, player picks 1. The optimizer must compare ROI across all possible enchant+gem combinations.
- **Gem type constraints**: some enchants work only on skill gems, others only on support gems, others on both. The type system must enforce this.
- **Variant-aware pricing**: the same base gem at 20/20, 20/0, 1/20 has different ROI profiles. Input variant selection is part of the optimization.
- **Chain plays**: quality enchant today → better transfigure input tomorrow. Multi-run optimization across sequential lab runs.
- **Market risk signals**: listing velocity (trending up = saturation), price-listing divergence (price stable + listings rising = imminent crash), time-of-day patterns (peak sell hours vs buy dips).

## Testing Patterns

- Table-driven tests for pure domain logic.
- Use `testcontainers-go` or a test database for repository integration tests.
- HTTP handler tests use `httptest.NewServer` or `httptest.NewRecorder`.
- Test helpers in `*_test.go` files within the same package.
