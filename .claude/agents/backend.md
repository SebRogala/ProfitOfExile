# Backend Agent

Server-side implementation principles. Extends the general agent with Go architecture and API design conventions.

## Architecture Patterns

- Follow the project's package structure: `cmd/server/` for entrypoint, `internal/` for all business logic.
- Domain invariants belong in domain types, not in handlers. A type should never be in an invalid state after construction.
- Services orchestrate operations across domain types and repositories. Keep them focused on a single use case.
- Use interfaces at package boundaries. Concrete implementations live in infrastructure packages.

## Go Conventions

- **Go 1.22+**: Use standard library HTTP routing (`http.NewServeMux` with method patterns) or chi router.
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
└── server/          # main.go entrypoint
internal/
├── domain/          # Business types, interfaces (no external dependencies)
│   ├── strategy/    # Strategy tree, simulation
│   ├── inventory/   # Shared inventory, set converter
│   ├── price/       # Price types, source interfaces
│   └── market/      # Market source abstractions
├── handler/         # HTTP handlers (thin — validate, call service, respond)
├── service/         # Use case orchestration
├── repository/      # pgx implementations of domain interfaces
├── pricefeed/       # poe.ninja + TFT API clients
└── config/          # Configuration loading
```

### Domain Layer Rules

- Domain types are plain Go structs with constructor functions enforcing invariants.
- Domain interfaces define what infrastructure the domain needs — domain never imports infrastructure.
- Value objects are small immutable types (ItemName, Source, Price).

### Handler Layer Rules

- Handlers are thin — parse request, call service, encode response.
- Use middleware for cross-cutting concerns (logging, auth, CORS).
- Static file serving via Go `embed` package for the compiled SvelteKit frontend.

## Price Data Sources

- **poe.ninja**: REST API with `chaosEquivalent` for currency/fragments, `chaosValue` for items. 60-min cache TTL.
- **TFT**: Static JSON from GitHub. Lifeforce entries have `ratio: 1000` — must divide chaos by ratio.
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
