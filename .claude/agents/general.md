# General Agent

Universal coding principles for all implementation and fix agents. This is the base context — every specialized agent extends it.

## Coding Principles

- Follow existing patterns in the codebase. When adding new code, match the style, naming, and structure of surrounding code.
- Don't over-engineer. Implement what's needed for the current task, not speculative future requirements.
- Preserve intent and nuance. When modifying existing code, understand why it was written that way before changing it. Don't simplify away meaning.
- Read the project's CLAUDE.md before starting work. It contains project-specific conventions, anti-patterns, and constraints that override general principles.

## MCP Context Loading

Before implementation, load context from MCP to understand what's already known:

- `getTaskContext(taskId)` — returns discoveries (task + parent), session findings, and chunk artifacts in one call
- `getChunkArtifacts(taskId)` — results from previously completed chunks (avoid conflicts, build on their work)

Use these to orient yourself before exploring code. Don't re-discover what's already documented.

## Project Commands

Read `## Key Commands` from the project's CLAUDE.md for allowed build/test commands. Never invoke build tools directly — use the project's configured commands.

## File Operations

- **Read files**: use the Read tool, not cat/head/tail
- **Edit files**: use the Edit tool for targeted changes, not sed/awk
- **Write files**: use the Write tool for new files, not echo/cat with redirects
- **Search content**: use Grep, not grep/rg bash commands
- **Find files**: use Glob, not find/ls bash commands

These tools provide better output formatting and avoid permission/encoding issues.

## Testing

- Run tests using the project's configured test commands (from config or Makefile), not raw test runner commands
- Test commands may have required environment setup, database state, or wrapper logic that raw commands skip
- When tests fail, read the full error output before attempting fixes. Understand the failure before acting.

## Quality Rules

- Every change must leave the codebase in a working state. Don't commit code that breaks existing tests.
- When modifying shared code (utilities, base packages, interfaces), check all callers for impact.
- Prefer explicit over implicit. Named constants over magic numbers, clear variable names over abbreviations.
- Error handling should be specific. Return typed errors with context, not generic error strings.

## Recurring Review Patterns

These patterns are repeatedly flagged in code reviews. Internalize them to avoid rework.

### Error wrapping must include context

Always wrap errors with `fmt.Errorf` and `%w` to preserve the chain. Include the operation and relevant entity identifiers:

```go
// BAD — loses context
return err

// GOOD — preserves chain with context
return fmt.Errorf("fetch price for item %q from %s: %w", itemName, source, err)
```

### Logger context must include entity IDs

Error/warning logs must include relevant entity identifiers (strategyID, itemName, source), not just the error message. This makes production debugging possible.

### Response DTO completeness

When adding a new field to an entity or domain type, update **all** API response types that represent it. Search for the type name in handler/response files. A new field without a matching response field means the API silently drops data.

## Domain Context

This project models Path of Exile 1 farming strategies. Key concepts:

- **Strategy Tree**: Composable tree of farming activities with series counts, item inputs/outputs, and probabilities. Strategies can be nested (e.g., a Lab Run contains sub-strategies for each enchant type).
- **Inventory-Driven Cascade**: Shared inventory with automatic set conversion (e.g., 4 fragments -> 1 set) and auto-buy from cheapest source.
- **Multi-Source Pricing**: Prices from poe.ninja (individual) and TFT (bulk). Optimal buy/sell source selection. Listing count is a first-class metric alongside price.
- **Breakpoint Analysis**: Simulate at each tree depth to find optimal sell points.
- **Decision Optimization**: Some strategies present choices (e.g., Lab enchant selection from 4 options). The optimizer picks the highest-ROI option given available inputs and their type constraints.
- **Variant-Aware Inputs**: The same item at different quality/level variants (20/20, 20/0, 1/20) has different ROI. Input variant selection is part of optimization.
- **Market Risk Signals**: Listing velocity (saturation indicator), price-listing divergence (crash predictor), time-of-day patterns. These inform strategy recommendations.

### Reference Material

Working prototypes and strategy analysis live in `/var/www/poe/`:
- `scripts/` — Node.js analysis scripts (font EV, transfigure ROI, price trends)
- `strategies/3.28-mirage/` — Current league strategy notes with real market data
- `poe1-knowledge.md` — PoE1 knowledge base covering patches 3.25-3.28

## Commit Discipline

- Stage only files relevant to the current task. Don't include unrelated changes.
- Never commit files that contain secrets, credentials, or environment-specific configuration.
