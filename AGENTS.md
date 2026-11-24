# ProfitOfExile agent guidance

This is the shared entry point for coding agents. Keep it tool-neutral and hold
it to links, working rules, and cross-cutting invariants; detailed project
knowledge belongs in maintained documentation or specialized agent profiles.

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
- [Analysis cache guide](docs/ANALYSIS-CACHE.md) — how `lab.Cache` is populated
  and served, and the concurrency rules any new cache field must follow.
- [Game facts](docs/GAME-FACTS.md) — dated, sourced Path of Exile facts the
  code treats as invariants; read before touching lab, font, temple, or
  exchange logic.

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

- Branches: `dev` is the integration branch. Fork each task branch from `dev`
  and fast-forward merge it back into `dev`; gates run on `dev`. `main` is
  production. Fast-forward `main` to `dev` only when the owner
  says deploy, because a push to `main` deploys; [docs/DEPLOY.md](docs/DEPLOY.md)
  holds the deploy mechanics. Push `dev` without asking; a `dev` push deploys
  nothing (owner, 2026-11-12 and 2026-11-19).

## Durable rules

- Follow [scripts/size-gate.sh](scripts/size-gate.sh)'s plan-time duty for a split WI or reviewed intrinsic marker when touching already-over files.
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
- When a change edits a file that has a same-named copy across `frontend/src/lib/` ↔ `desktop/src/lib/`
  or `frontend/src/routes/lab/components/` ↔ `desktop/src/routes/(app)/components/`, make each edit to
  code both copies share in that copy too, in the same change: the copies are hand-maintained, so a
  one-sided fix ships the bug. `desktop/src/lib/mirror-identity.test.ts` is the mechanical half of
  this rule: a change that makes a pair identical or divergent updates that file's lists in the same
  change.
- When a code comment cross-references another function or file, name the symbol (`mirrors
  BuildCompareResults in collective.go`), not a line (`collective.go:311` rots on the next edit).
  Keep the why and its evidence, including a past behaviour that explains a current constraint.
- Limit labels, notes and empty states to data and update times.
  Write a plain tooltip as one defining sentence, optionally followed by one pointer sentence to a related column (e.g. "see Scale").
  Keep a plain tooltip to at most 25 words and 2 sentences in total, without HTML.
  Use the structured form only when the tooltip must distinguish two or more states, steps or symbols the column renders.
  "It needs context", "it is important" or "it was hard to shorten" does not qualify; cut to the plain limit instead.
  Model structured tooltips on the `col-signal` header in `desktop/src/routes/(app)/components/BestPlays.svelte`.
  Use a bold title, bold group headings and blank lines between groups.
  Give each group heading a scope clause.
  Put each state on one line: state name — gloss of at most eight words.
  Reuse each state's existing colour; leave names uncoloured where no state colour exists.
  Allow Room valuation tooltips three compact labelled sections (Effect, Drops, Source) that may wrap.

## Modeling rules

These keep resurfacing in review. Each was learned once at a cost.

- **Per market, never unified.** Every threshold, sigmoid, discount, or
  confidence parameter is calibrated per gem variant (`1`, `1/20`, `20`,
  `20/20`, and the corrupted Dedication variants). `20/20` and `1/0` are
  different markets: listing counts and price dispersion differ several-fold,
  so one curve centred for one is wrong for the other. Compute variant
  baselines and pass them through.
- **Absolute cutoffs on price or listing count are suspect.** They encode a
  league-age assumption: 5c is a real price on day 2 and trash on day 60.
  Prefer a percentile of the current distribution; a league-phase gate is the
  fallback (POE-134).
- **The collector stores everything upstream serves.** Filtering,
  classification, and subsetting happen in the analysis layer that consumes
  the data. Corrupted gems were once dropped at the collector, and the
  Dedication analysis then had nothing to price.
- **Collector scheduling seeds from the database on start**, never from
  in-memory state alone: the last stored snapshot time decides whether to
  fetch, so a redeploy does not write a duplicate snapshot.
- **A prediction near the majority-class baseline is not a prediction.**
  Compare accuracy with "always predict the majority class"; within a few
  points of it, show the raw data as uncertain, never as a recommendation.
  Directional signals at 29–35% accuracy once shipped with confidence scores;
  POE-66 replaced them with risk-adjusted value scoring.
- **Labels never borrow a colour name.** Gem colours are RED/GREEN/BLUE, so
  sellability is SAFE/FAIR/RISKY, not traffic-light colours. Check a new
  categorical name against the existing vocabularies: gem colour, signal type,
  tier name.
- Visibility and ordering are accepted decisions, not judgment calls: served
  plays are visible by default and quality is ranking plus flags (ADR-015,
  ADR-017); a sort orders by its column alone and a flag never partitions
  (ADR-018).

Keep subsystem recipes and volatile facts out of this file. Add them to the
appropriate maintained document or specialized profile instead.
