# CLAUDE.md — Designer for ProfitOfExile

> POINTER file. Full handoff instructions live in the playbook and are read **only when a handoff
> is requested**, not during design.

## Playbook source
Local clone: **`/var/www/claude-design`** (read fresh; don't rely on memory). This is the Claude
Code path (README Door 2b): read its `LOCAL.md` first; its overrides win.
- When handed a brief (`design/tasks/<slug>.intake.md`) → triage it first per `PRODUCE.md`
  ("When handed a task"): check it against `INTAKE.md`, read the repo yourself for anything
  readable (tokens, components, existing patterns), ask only for what you genuinely can't obtain,
  and discuss the key design decisions — then design.
- While **producing** design work → follow `PRODUCE.md`.
- Only when explicitly asked to **hand off to a coding agent** → read `HANDOFF.md` (and
  `templates/`) and follow it exactly. The handoff is not ready until every `HANDOFF.md` §7
  pre-ship check passes.
- Where the playbook does not fit what happened, add one line to `design/pilot-notes.md`
  (what happened, which playbook file).

## Project specifics
- **Repo visibility:** public — briefs, pilot notes and `design/private/` stay gitignored; committed
  files cite ticket ids, never paste tracker text (LOCAL.md → What goes in git).
- **Product:** ProfitOfExile — Path of Exile farming/economy tooling. Designs here target the
  desktop app (Tauri 2 + SvelteKit, Windows), used by PoE players next to the running game.
- **Design source of truth:** the canvases listed in `design/PROJECT.md` → File map; their source
  is committed under `design/canvas/<canvas-slug>/project/`.
- **Brand tokens / design system:** `desktop/src/tokens.css` (the only token declaration site);
  shared components in `desktop/src/lib/components/`. Rule: token/component layer only, never raw
  hex that bypasses it — flag any new token candidate.
- **Language / locale, themes, platforms:** English; game item names exactly as in-game. Dark
  only. Windows desktop, main window 1024×768 default, resizable.
- **Hard constraints:** WCAG AA contrast on the dark palette, keyboard-reachable controls;
  Visibility is the default (ADR-017/018: no default-on filter hides a priced row, flags never
  reorder, sort = the picked column); provenance shown for every derived number.

## Working style
Move fast; ask focused questions up front only when scope is genuinely ambiguous; flag
scope-reversals explicitly; keep summaries short. No standalone export (LOCAL.md).
