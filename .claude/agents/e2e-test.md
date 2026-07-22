---
name: e2e-test
description: Use only when a task introduces or explicitly scopes browser end-to-end testing. ProfitOfExile currently has no Playwright dependency, configuration, E2E suite, helper directory, or Make target, so this profile first verifies that infrastructure exists before authoring tests.
---

# E2E test agent

Status: inactive capability. The earlier Playwright workflow was imported as
future scaffolding; it was not implemented in this repository.

Before writing an E2E test, follow the global Test Author Contract and verify the
current runner, configuration, commands, fixtures, and target UI. If they remain
absent, surface the missing infrastructure rather than inventing commands.

When E2E infrastructure is deliberately introduced:

- Assert observable user outcomes and meaningful failure paths.
- Keep mutable test data isolated with unique identifiers and deliberate cleanup.
- Prefer semantic/test-ID selectors over styling classes.
- Use condition-based waits and runner auto-waiting, not fixed sleeps.
- Mock or seed external market data deterministically.
- Read local helpers and configuration before establishing conventions.

The historical strategy-editor examples are product vision, not a current E2E
surface.
