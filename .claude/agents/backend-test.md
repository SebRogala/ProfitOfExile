# Backend Test Agent

Knowledge for writing and fixing Go backend tests (unit, integration, and handler tests). This agent is loaded as context by test pipeline skills and injected into fix loops that diagnose and repair test failures.

## 1. Table-Driven Tests

Use table-driven tests for domain logic with multiple input/output scenarios:

```go
func TestStrategyProfit(t *testing.T) {
    tests := []struct {
        name     string
        strategy Strategy
        prices   PriceMap
        wantCPH  float64
    }{
        {
            name:     "simple single-step strategy",
            strategy: newTestStrategy(...),
            prices:   testPrices(...),
            wantCPH:  42.5,
        },
        // more cases...
    }
    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            got := tt.strategy.Simulate(tt.prices)
            if got.ChaosPerHour != tt.wantCPH {
                t.Errorf("ChaosPerHour = %v, want %v", got.ChaosPerHour, tt.wantCPH)
            }
        })
    }
}
```

## 2. Test Helpers

- Place test helpers in `*_test.go` files within the same package.
- Helper functions that create test data should start with `newTest*` or `test*`.
- Use `t.Helper()` in helper functions so failures report the caller's line number.
- For complex setup, use `testutil` package in `internal/testutil/`.

## 3. Database Integration Tests

- Use build tags (`//go:build integration`) to separate integration tests from unit tests.
- Each test should set up its own data and clean up after — don't depend on shared state between tests.
- Use `t.Cleanup()` for teardown, not `defer` (cleanup runs even if test is skipped).
- Test against a real PostgreSQL instance (via testcontainers or a test database).

```go
func TestPriceCacheRepository(t *testing.T) {
    if testing.Short() {
        t.Skip("skipping integration test")
    }
    db := setupTestDB(t)
    repo := repository.NewPriceCache(db)
    // test operations...
}
```

## 4. HTTP Handler Tests

Use `httptest` for handler testing. Test the full request/response cycle:

```go
func TestGetStrategyHandler(t *testing.T) {
    svc := &mockStrategyService{...}
    handler := handler.NewStrategy(svc)

    req := httptest.NewRequest("GET", "/api/strategies/123", nil)
    w := httptest.NewRecorder()

    handler.ServeHTTP(w, req)

    if w.Code != http.StatusOK {
        t.Errorf("status = %d, want %d", w.Code, http.StatusOK)
    }
    // decode and assert response body...
}
```

## 5. Mock Patterns

- Use interface-based mocks. Define mock structs implementing domain interfaces in test files.
- For simple mocks, inline the struct. For complex/reused mocks, extract to a shared test helper.
- Prefer function fields over method stubs for flexible per-test behavior:

```go
type mockPricer struct {
    GetPriceFn func(item string, source Source) (Price, error)
}

func (m *mockPricer) GetPrice(item string, source Source) (Price, error) {
    return m.GetPriceFn(item, source)
}
```

- After changing mock behavior, verify the mock still reflects realistic behavior. Stale mocks that return impossible combinations produce false confidence.

## 6. Assertion Quality

### Assert values, not just structure

When test data is known, assert actual values — not just that fields exist:

```go
// BAD — proves nothing about correctness
if result.Name == "" {
    t.Error("expected non-empty name")
}

// GOOD — verifies actual data
if result.Name != "Uber Elder" {
    t.Errorf("Name = %q, want %q", result.Name, "Uber Elder")
}
```

### Negative assertions alongside positive

When testing filtering or access control, also assert that excluded items are NOT present:

```go
names := extractNames(results)
assertContains(t, names, "Maven's Writ")
assertNotContains(t, names, "Exalted Orb") // shouldn't appear in fragment results
```

### Test boundary conditions

For any query/filter function: empty inputs, nil values, zero-length slices, and edge cases of the domain (e.g., zero-cost items, 100% probability strategies).

## 7. Price Data Testing

- Use deterministic test prices, not live API calls, in unit tests.
- Test multi-source price selection: verify cheapest buy source and highest sell source are chosen correctly.
- Test cache TTL behavior: stale prices should trigger refresh.
- Test listing count thresholds: items with <5 listings should be flagged as low-confidence.
- Test price + listing divergence detection: price stable but listings doubling = saturation signal.

## 8. Strategy Simulation Testing

- Test the recursive tree executor with known inputs and verify cumulative outputs.
- Test set conversion cascade: adding fragments should trigger automatic set assembly.
- Test auto-buy behavior: when inventory is missing an item, verify buy from cheapest source.
- Test breakpoint analysis: verify that stopping at different tree depths produces correct profitability.
- **Probabilistic output strategies**: test Font-style "pick best of N random from pool" — verify EV calculation against known pool sizes and value distributions.
- **Decision optimization**: test Lab-style "choose 1 from K options" — verify the optimizer picks highest ROI given gem type constraints (skill-only vs support-only enchants).
- **Variant-aware ROI**: test that 20/20, 20/0, and 1/20 inputs produce different ROI for the same strategy, and that the optimizer selects the best variant.
- **Chain play simulation**: test multi-run strategies where one run's output becomes the next run's input (quality enchant → future transfigure).

## 9. Debugging Test Failures

- Read the full error message and stack trace before attempting a fix. The root cause is often not where the test fails.
- When a test fails intermittently, suspect shared state, timing dependencies, or test ordering. Never fix flaky tests by adding delays.
- Replace silent test skips with explicit assertions. If a test can't run because data is missing, fail loudly with a descriptive message.
