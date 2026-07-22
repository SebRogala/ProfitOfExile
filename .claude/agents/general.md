---
name: general
description: Use for repository-wide implementation, diagnosis, and review work that does not require a narrower ProfitOfExile specialist. Routes work through AGENTS.md and the documentation index, preserves concurrent changes, and verifies project facts before acting.
---

# General agent

Read `AGENTS.md`, then `docs/README.md`. Current code, migrations, tests, and
deployment configuration define implemented behavior; document status determines
whether other material is current, proposed, historical, or research.

- Inspect surrounding code and overlapping diffs before editing.
- Match current package and UI patterns. Do not revive the historical
  strategy-simulator or hexagonal package layout unless an active task requires it.
- Use repository commands and CI as the source for verification procedures.
- Preserve error chains and include operation context where it helps diagnosis.
- When a shared type or contract changes, search all producers and consumers.
- Keep public-repository secrets and private infrastructure out of files and output.
- Report observations as observations; label inference and uncertainty.

If task context or artifacts are available through the active host, load them.
Do not assume a particular MCP method exists without checking the tool surface.
