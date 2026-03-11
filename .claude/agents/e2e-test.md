# E2E Test Agent

Knowledge for writing and fixing Playwright E2E tests. This agent is loaded as context by test pipeline skills and injected into sub-agents that write or fix tests.

## 1. Self-Contained Test Pattern (MANDATORY)

Every test must be fully self-contained. Tests that mutate data must create their own entities through UI interactions — never rely on pre-existing state for anything that will be modified.

### Rules

- **Create your own data**: Every test that modifies state must set up its own context first.
- **Use unique identifiers**: Generate collision-free names to avoid conflicts between parallel test runs.
- **Seed data is read-only**: Use it for reference data and context, but never mutate it.
- **No per-test teardown**: Unique identifiers mean no collisions. No cleanup needed.

### Example Structure

```javascript
import { test, expect } from '@playwright/test';

test.describe('POE-42: Strategy Profitability', () => {
    test('can create and simulate a strategy', async ({ page }) => {
        // Setup: create own data via UI
        // Action: perform the feature action
        // Assert: verify expected outcome
    });
});
```

## 2. Test Execution Rules

Always use the project's configured test commands (e.g., `make e2e-test`), not raw `npx playwright` commands.

| Action | Command |
|--------|---------|
| Run specific test | `make e2e-test ARGS="tests/e2e/specific-file.spec.js"` |
| Run all E2E tests | `make e2e-test` |

## 3. Helper Awareness

Before writing any test, read the project's test helpers directory to discover available utilities (login, entity creation, unique ID generation).

### Helper Creation Rules

- **When to create**: If two or more tests need the same multi-step entity creation, extract it.
- **Naming**: `create-{entity}.js` — e.g., `create-strategy.js`.
- **Export pattern**: `export async function create{Entity}(page, overrides = {}) { ... }`
- Helpers should verify success before returning. Don't let silent creation failures cascade.

## 4. Selector Conventions

Prefer data attributes over CSS class selectors for test stability:

```javascript
// Preferred — stable across styling changes
page.locator('[data-testid="strategy-tree"]')

// Acceptable — semantic selectors
page.locator('button:has-text("Simulate")')

// Avoid — brittle, breaks on style changes
page.locator('.flex.items-center.gap-4')
```

## 5. Framework Patterns

### Navigation with Action

Use `Promise.all` to avoid race conditions when an action triggers navigation:

```javascript
await Promise.all([
    page.waitForNavigation(),
    page.click('button[type="submit"]'),
]);
```

### Waiting

Never use `page.waitForTimeout()`. Always use explicit waits:

- `await expect(page.locator('.alert-success')).toBeVisible()` — preferred, auto-retries
- `page.waitForSelector('.result-table')` — wait for element
- `page.waitForNavigation()` — wait for navigation
- Playwright's built-in auto-waiting on actions (click, fill, etc.)

### Max Iteration Guard on Loops

Any loop that iterates DOM elements needs a cap to prevent infinite hangs:

```javascript
const MAX = 20;
let i = 0;
while (await page.locator('.loading').isVisible() && i < MAX) {
    await page.waitForTimeout(100);
    i++;
}
```

### Pagination-Safe List Lookups

Parallel workers create enough data to push items beyond page 1. Any test that navigates to a list page and looks for a specific row MUST use search/filter parameters:

```javascript
// BAD — row may be on page 2
await page.goto('/strategies');

// GOOD — filter server-side
await page.goto(`/strategies?search=${encodeURIComponent(strategyName)}`);
```

### Null Guard on getAttribute()

When selecting option values from dropdowns, guard against null returns with descriptive errors:

```javascript
const value = await page.locator('select#source option:not([value=""])').first().getAttribute('value');
if (!value) throw new Error('No options for #source. Is seed data loaded?');
```

## 6. Test File Conventions

### Naming

Test files named after the task slug: `{task-slug}.spec.js`

### Structure

```javascript
import { test, expect } from '@playwright/test';

test.describe('POE-42: Strategy Tree Editor', () => {
    test('can add a child node to strategy tree', async ({ page }) => {
        // Setup
        // Action
        // Assert
    });
});
```

### Guidelines

- `test.describe` block label format: `'{TASK_ID}: Feature Name'`
- Focus on the happy path — cover the primary user flow
- Keep tests focused on one user journey per `test()` block
- **Assert observable behavior, not internal state** — verify what the user sees (price display, profitability chart, strategy tree shape), not internal calculations

## 7. Domain-Specific Patterns

### Strategy Tree Interactions

- Tree nodes may be nested — use recursive selectors or level-specific data attributes.
- Adding/removing nodes should update the tree visualization immediately.
- Simulation results should appear after clicking "Simulate" without full page reload.

### Price Data

- Tests should not depend on live price data. Mock or seed deterministic prices.
- Verify price display shows both source and value.
- Test stale price indicators when cache TTL is exceeded.

## 8. Triage Categories

| Category | Criteria | Action |
|----------|----------|--------|
| **GENERATE** | UI feature with clear user flow: strategy editor, simulation, price display, settings | Write the test |
| **SKIP** | Backend-only: price fetching, database migrations, API-only endpoints, cron jobs | Skip |
| **COMPLEX** | Features needing external price APIs, real-time updates, complex tree manipulation | Flag for manual review |
