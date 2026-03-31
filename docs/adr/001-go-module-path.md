# ADR-001: Go Module Path: Short Local Identifier

## Status

Accepted

## Context

When initializing the Go module for ProfitOfExile, the module path must be chosen. In Go, the module path appears in every import statement throughout the codebase and is embedded in `go.mod`. Changing it later requires a mass rename of all import paths across all source files.

Go's conventions offer two broad approaches:

1. Use a fully-qualified VCS path (`github.com/owner/repo`) so the module can be fetched by others via `go get`
2. Use a short local name with no VCS prefix (e.g., `profitofexile`) for applications that are never published as libraries

ProfitOfExile is a single-tenant, self-hosted web application. It will never be imported by external Go code and will never be resolved via `go get`. The deployment artifact is a Docker image, not a Go package.

Key considerations:

1. Module paths with `github.com/` prefixes imply the module is a distributable library — this sends the wrong signal for an application codebase
2. Short names are more readable in import statements throughout the codebase (`import "profitofexile/internal/price/domain"` vs `import "github.com/owner/profitofexile/internal/price/domain"`)
3. The repository may be private or self-hosted, making the `github.com/` prefix misleading
4. Go tooling does not require a VCS-prefixed path for application binaries built locally or in CI

## Decision

Use `profitofexile` as the Go module path in `go.mod`:

```
module profitofexile

go 1.22.0
```

All internal packages are imported as `profitofexile/internal/{module}/...`.

## Consequences

### Positive

- Import paths are short and readable throughout the codebase
- No false implication that the module is a publishable library
- No dependency on the GitHub org/repo name, which could change

### Negative

- Deviates from Go community convention for modules hosted on VCS — developers expecting `github.com/...` may find this surprising
- If the project were ever extracted into a shared library (unlikely), all import paths would need to be renamed

## Alternatives Considered

### github.com/owner/profitofexile

The conventional Go module path for open-source or distributable projects.

**Rejected because**: ProfitOfExile is a self-contained application binary, not a library. Using a VCS-prefixed path implies `go get` installability that is intentionally not supported. The added verbosity in every import statement provides no benefit for an application-only codebase.

## References

- [POE-13](POE-13) — Go module init + project scaffold (task that prompted this decision)
