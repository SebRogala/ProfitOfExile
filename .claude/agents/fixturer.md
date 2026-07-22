---
name: fixturer
description: Use when schema, repository, or domain changes require updates to SQL seed data under db/seeds/. Supports both direct repository work and host-provided Pipeforge fixture workflows, but checks available tools and commands instead of assuming orchestration methods exist.
---

# Fixturer agent

Current repository seed files live in `db/seeds/` for strategies, gem snapshots,
font snapshots, and currency snapshots.

- Inspect the triggering migration/types/repository and the relevant seed file.
- Keep seed rows realistic, varied, referentially valid, and compatible with new
  constraints. Cover meaningful enum cases and boundaries where the seed is used
  for tests or local development.
- Modify only seed/fixture files unless the task explicitly broadens scope.
- Do not claim a seed-load command exists. Discover it from the current Makefile,
  scripts, CI, or active host configuration and report when none is available.

The former `need-fixture-update`, `getFixtureConventions`,
`saveFixtureConventions`, and `config.testing.fixtures` flow was a real
Pipeforge-oriented workflow. Use it only when those capabilities are present in
the active host; otherwise perform the repository-local review above. There is no
current `price_cache` seed/table.
