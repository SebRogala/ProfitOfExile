# AI-Native Case Study: ProfitOfExile

ProfitOfExile is a Path of Exile 1 lab-farming companion built with an AI-native delivery workflow. It started as a personal tool, then became useful to friends, then reached the wider community after content creators shared it.

The project is useful as a public artifact because it shows both sides of the work:

- the product users can inspect and run
- the delivery system used to build it across unfamiliar stacks

## Product Origin

The first version was built for my own play. The problem was practical: Path of Exile lab farming has short-lived market opportunities, and deciding which Divine Font craft is profitable requires price, liquidity, variant, and timing context.

After friends started using it, one of them, a YouTube content creator, asked if he could make a video about the app because he considered it useful enough for the community. Later, an unrelated Portuguese creator included it in a "new tools for PoE" video and spoke positively about it.

That path matters: this was not created as a portfolio demo. It was pulled outward by users because it solved a real niche workflow.

## What It Does

ProfitOfExile combines a public web dashboard with a desktop overlay:

- collects gem, currency, and fragment prices from poe.ninja
- stores time-series market snapshots in PostgreSQL/TimescaleDB
- computes lab-farming profitability signals ahead of user requests
- classifies opportunities by ROI, liquidity, market trend, and confidence
- provides a Tauri desktop app for in-game overlay use
- reads Path of Exile screen regions via OCR to identify craft options
- shows live profit signals without sending input to the game

## Architecture

The system spans several runtimes:

- Go backend and collector
- SvelteKit web frontend
- Tauri desktop app with Rust backend and Svelte frontend
- PostgreSQL/TimescaleDB for time-series data
- Mercure SSE for collector/server/frontend/desktop events
- GitHub Actions and Coolify for deployment

The backend and collector are separate processes. The collector ingests market data and publishes update events. The server recomputes analysis and serves API/frontend traffic. The desktop app consumes server APIs and local game context.

## AI-Native Delivery Workflow

The project was built through Claude Code using Pipeforge, my orchestration layer for AI-assisted software delivery.

The workflow is phase-driven:

1. discuss task intent and constraints
2. explore relevant code through agents
3. write and review an implementation plan
4. split larger plans into typed chunks
5. dispatch specialized implementation agents
6. commit each chunk separately
7. run test and fixture phases
8. run heavy review with multiple reviewer agents
9. fix findings through typed agents
10. merge, update route state, and capture learnings

This changed my role from line-by-line code author to product/architecture/verifier:

- define the outcome
- shape the plan
- decide acceptable trade-offs
- review delivered behavior
- improve the orchestration when failure modes appear

## Verification Gates

The project uses several forms of verification:

- Go test suite across collector, database, domain analysis, server, trade, and device packages
- race-enabled Go test runs
- desktop Rust tests for OCR parsing, lab navigation, state transitions, settings, and trade query generation
- frontend and desktop build/check workflows
- AI heavy-review agents for code quality and hidden failure modes
- Pipeforge review findings stored for later pattern integration

The strongest current backend signal:

```bash
docker compose exec app go test -race ./...
```

The desktop app also has Rust test coverage:

```bash
docker compose run --rm -w /app/desktop/src-tauri desktop cargo test
```

## Failure Modes Found

The project exposed the same lesson as production AI-assisted work generally: generated code can be fast, but verification has to be structural.

Examples of risks found and addressed:

- admin HTTP endpoints were removed in favor of CLI-triggered Mercure events
- Tauri overlay positioning needed physical/logical pixel handling to avoid multi-monitor DPI bugs
- desktop settings persistence needed careful ordering so overlay/window state did not overwrite each other
- trade API access required rate limiting, caching, and queueing
- backend tests and race checks became a central trust boundary

The current hardening roadmap is explicit:

- broaden CI across frontend and desktop
- keep desktop TypeScript/Svelte validation green
- review Tauri CSP and capabilities
- reduce duplication between web and desktop UI components
- split large files where review boundaries are too broad

## Why This Matters

ProfitOfExile demonstrates that AI-native engineering is not just faster typing. The important part is building a delivery system around AI:

- persistent task and plan state
- bounded agent responsibilities
- repeatable review phases
- tests and race checks
- artifact capture
- prompt evolution from recurring findings

The product proves the workflow can ship real software. Pipeforge proves the workflow is systematic rather than accidental.
