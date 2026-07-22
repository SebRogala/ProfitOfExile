# ProfitOfExile agent guidance

This is the shared entry point for coding agents. Keep it short and
tool-neutral; detailed project knowledge belongs in maintained documentation or
specialized agent profiles.

## Start here

- [README.md](README.md) — product overview, stack, and development entry point.
- [Documentation index](docs/README.md) — the registry of current guides,
  accepted ADRs, proposed specifications, research, and historical documents.
- [Product vision](docs/product-vision.md) — historical strategy-simulation
  domain and future scope; not current implementation guidance.
- [Architecture decisions](docs/adr/) — accepted technical decisions.
- [Trade and market lifecycles](docs/TRADE-LIFECYCLE.md) — current and proposed
  data-flow behavior, explicitly labelled.
- [Overlay guide](docs/OVERLAY-GUIDE.md) — specialized Tauri/Windows knowledge;
  observe its status caveat.

Current code, migrations, tests, and deployment configuration are authoritative
for implemented behavior. Document status matters: do not treat product vision,
historical plans, research, or proposed specifications as current behavior.

## How to work

Prefer specialized sub-agents for substantial, bounded work when the agent host
supports delegation. The main agent should retain ownership of scope,
coordination, integration, and final verification. Run independent investigations
in parallel where useful; keep tightly coupled edits with one owner. Do not
delegate trivial work merely to create more agents.

Specialized profiles live in [.claude/agents/](.claude/agents/). Select the
smallest relevant set, read the profile before delegating, and give each agent a
clear deliverable and file scope. Profiles provide domain context, not authority:
verify their commands, paths, and factual claims against the current repository
and the documentation index before acting on them.

## Durable rules

- Preserve unrelated and uncommitted user changes. Inspect overlapping diffs
  before editing.
- PostgreSQL access uses direct, parameterized `pgx` queries; do not add an ORM.
- Never modify a migration that may have been deployed. Create a new timestamped
  pair with `make migration name=descriptive_name`.
- Development is Docker-first. Use repository commands and CI configuration as
  the source for build and test procedures; run verification proportional to the
  change and report checks that could not run.
- When documentation describes behavior, label it current, accepted, proposed,
  historical, superseded, or dated research as appropriate.
- Tracker task and epic descriptions are canonical for active specifications and
  implementation tracking. Do not mirror a specification into a repository file;
  reference the tracker ID instead. Repository documents describe implemented
  behavior, accepted decisions, and history.
- This is a public repository. Never commit or reproduce credentials, tokens,
  private endpoints, production identifiers, database connection strings,
  `.env` contents, device fingerprints, pairing capabilities, or backup
  locations. Local ignored files may contain secrets.

Keep subsystem recipes and volatile facts out of this file. Add them to the
appropriate maintained document or specialized profile instead.
