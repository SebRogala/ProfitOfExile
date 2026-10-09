# AGENT.md — autonomous build protocol for ProfitOfExile — top-bar divine rate (POE-284)

You are implementing ProfitOfExile — top-bar divine rate (POE-284) **as the real, shipped application** in the target codebase —
**the final product, with data mocked as the only difference.**

**Themes in scope:** dark only. Every theme line below refers to these.

Read `README.md` first (full spec), then **§0b below** (what in this package is *not* the product),
then follow THIS protocol.

## 0. Deliverable shape (read this first — it defines "done")
Ship the **final application**, indistinguishable from the shipped product except that data is
mocked. Concretely that means:
- **Real route/URL** the view will actually live at (not a `/design/` or preview path).
- **Real module placement** in the codebase, **real app layout/shell** (the actual nav, header,
  chrome), **real navigation** in and out of the view.
- **Real app components & conventions** — the existing Button, Input, Select, Modal, etc. Never
  re-style or hand-roll what the app already has a component for.
- **Data mocked at the real seam** — pass mock values through the *same* boundary the view will read
  real data from, so going live = **swap the data source only**, no re-layout. Do NOT build a
  backend, auth, or real persistence.
- **Scope boundary — STOP at the seam.** The mock is a fixture (JSON/array/in-memory stub) that
  feeds the view through the boundary real data will use. Build nothing that makes the data real:
  no schema or migrations, no persistence or writes, no queries against real storage, no real
  integrations or endpoints, no auth. Backend work is a **separate task** — if a screen seems to
  need it, stub the value and note it in `NOTES.md`.
  **The seam takes whatever shape the codebase requires.** Sort each piece by one test — *does
  going live keep it?* Kept (the view, its components, the route, and any typed boundary the
  codebase demands: read models, DTOs, a port interface) → it is the seam; build it. Replaced (the
  fixture and the adapter that reads it) → it is the mock; build it, one per data owner (§6).
  Added only when going live (the real query, the write path) → it is the backend; stub and note
  it. This package names no code layers; the codebase's own rules and gates decide them.
- **Restyle only:** edit `desktop/src/lib/components/TopBar.svelte` in place. Add the chip and nothing else. Keep existing behaviour and tests.
- The **reference PNGs define visual fidelity, they are NOT the delivery format.**

**Anti-patterns (do NOT do these):** a preview/gallery page; a device-bezel/phone-frame mockup;
light+dark shown side-by-side *in the app*; a standalone demo page instead of the real route;
bespoke re-styling where a real app component exists; a token/swatch panel, an option badge, or any
other design-document scaffolding rendered as product (§0b). Those belong to the design prototype,
not your implementation.

## 0b. ⛔ What in this package is NOT the product
The package mixes **the thing to build** with **the reasoning about it**. Build only the first.

| File / element | Status |
|---|---|
| `design/canvas/desktop-app--strategies--harvest-flipping/project/TopBar-prototype.dc.html` | **THE SCREEN.** The only file describing what to build. |
| `reference_screens/` PNGs | **Visual ground truth.** Match these. Not a delivery format. |
| `fixtures.json` | **Data contract** — every field, value, owner and format. Consume it all; map names onto existing codebase types per §6. |
| `CHECKLIST.md` | **Acceptance contract.** Every box is a requirement. |
| `README.md` prose | **Spec + reasoning, mixed.** See the rule below. |
| `README.md § Spec changes vs ticket` | **Decided requirements that supersede the ticket.** Build Y, not X; write them back (below). |
| `README.md § Open decisions` | **Questions, not requirements.** Don't invent answers — where one blocks the build, stub it and log it in `NOTES.md`. |
| `INVENTORY.md`, `NOTES.md` | **Your output**, not input. |
| Every other board on the canvas | **Provenance only.** Never build from it. |

**The rule for prose:** anything phrased as a *reason* ("because…", "deliberate, for the older
user", "tried and rejected", "would compete with…") explains **why** a requirement exists. Use it
as context for your judgement where the spec is thin; never render it. Descriptions, imperatives
and tables describe the product — reasons never do.

**Ticket vs this package:** the ticket is the spec as written *before* design. Decisions made while
designing are listed in `README.md § Spec changes vs ticket`, and they win. Before building, write
each one back into the ticket; without tracker access, list them in `NOTES.md` under **Ticket
updates needed** for a human. Any other conflict between the ticket and this package is
unconfirmed: build what the package shows and flag it in `NOTES.md` under the same heading.

**Specifically DO NOT build:**
- Any token/swatch/spine panel. Color and type values are a **token layer**, not a page (the §4
  step-1 swatch check is throwaway).
- Any turn/option badge (e.g. `TURA 2`, `2A`, `1A`), version caption, or commentary about the
  design direction. Those live only in the design document — if you see them anywhere, they are
  scaffolding.
- Any side-by-side comparison of variants. One design shipped; the alternatives were discarded.
- Any "design notes" affordance in the UI.

## 1. Non-negotiables
- **1:1 with `reference_screens/`** (grouped per app/surface) for *visual fidelity* — match
  layout, spacing, color, copy, every theme in scope. When unsure, open `TopBar-prototype.dc.html`
  and look — never the other boards (§0b).
  **"1:1" means visual fidelity ONLY — it is NOT a licence to copy the prototype's DOM,
  inline styles, device frame, or dual-theme scaffolding.** Reproduce what the screen *looks
  like* using the real app's shell, components, and tokens; never transcribe the prototype's
  markup. Verbatim structural copy is the drift this protocol exists to prevent.
- **Use the design tokens, never raw colors.** `desktop/src/tokens.css` is the only declaration site.
- **All UI copy in English**, exactly as in the reference.
- **Use the codebase's existing patterns/components.** If greenfield, pick one stack, stay in it.
- **Do NOT drop CHECKLIST.md → "MUST NOT DROP".** Those edge cases are required.
- **Do NOT add features** beyond the listed screens. Log ideas in `NOTES.md`; don't build them.
- Any affordance the reference implies (password show/hide, clear button, validation states) must
  use a **real app icon/component** — never an improvised or missing asset. If unspecified, use
  the app's standard and note it; don't invent a broken one.

> _Origin anecdote (the second flip): told to implement "1:1", an agent shipped the real PWA route
> with the prototype's two theme chromes side by side inside it — the design canvas transcribed into
> production. First flip: a preview instead of the app. Second flip: the app wearing the preview.
> §0 and the first rule above exist so neither recurs._

## 2. Enumerate before building (the completeness contract)
Before writing any screen, produce `INVENTORY.md`: a countable checklist of every
**screen × state × affordance** the delivery must contain, mined from `CHECKLIST.md`
(incl. MUST-NOT-DROP) and *every* `reference_screens/` PNG — not just the happy-path
screens. Enumerate the states explicitly: empty, loading, error, validation, success,
disabled, and each interactive affordance (show/hide, clear, expand, select, toggle).
"Done" is measured against this inventory, **not** against a visual glance — a screen that
renders but omits its error / empty / validation states is **not** done. Those omitted
states are exactly where behavior gets silently dropped. Keep the inventory ticked in
lockstep with the build loop.

> _Origin anecdote: the first uninstructed redesign attempt (HTML in, "let's redesign"
> out, no brief) silently dropped ~30% of the functionality. This contract exists so that
> can't recur — the number is the baseline it prevents, not a rate to expect under it._

## 3. The build loop (per screen, in order)
```
for each screen in IMPLEMENTATION ORDER:
  1. Read this screen's CHECKLIST.md section.
  2. Study reference_screens/desktop/<NN>-<state>.png.
  3. Build it at its REAL route, in the REAL app shell, with REAL components; mock data via
     the real seam (fixtures.json).
  4. Render in the running app and SCREENSHOT (every theme in scope).
  5. COMPARE to the reference; list diffs; fix; re-render until it matches.
  6. Tick every CHECKLIST box for this screen.
  7. Commit. Then next screen.
```
May not declare a screen done with any box unticked or any visible diff. Unresolvable →
`NOTES.md`, never silently skipped.

## 4. Implementation order (smallest blast radius first)
1. Tokens (+ theme switch when more than one theme is in scope) — verify on a throwaway swatch
   page in every theme in scope; delete it before committing, it is not product (§0b)
2. Shared components
3. Screens: (a) the single desktop rate store every conversion reads; (b) the chip in `TopBar.svelte` (references 01–03).
4. Full pass — run the whole CHECKLIST end to end in every theme in scope.
5. **Adversarial completeness pass** — a reviewer that did NOT build the screens audits the
   delivery against `INVENTORY.md` + every reference PNG, asking only *"what is missing?"*
   (unbuilt state, dropped affordance, unmatched reference, unticked box). Its findings are
   the next build round. The builder never signs off its own coverage — self-review is where
   dropped behavior hides.

## 5. Reference index
See `reference_screens/INDEX.md` for file → screen/state mapping and how to reach interactive states.

## 6. Data rules
Consume every field and value in `fixtures.json`. Do not invent or widen fields. Honor `_note`
keys. Pin formats (`fixtures.json` → `_formats`). Label/status/category sets the real system reads from
data come through the fixture too — never as constants in a template or component.

**One fixture source per data owner.** `fixtures.json` → `_owners` says which part of the system
produces each field. Put each owner's fields behind that owner's own boundary with its own
fixture adapter, so going live swaps one owner's source at a time. A single adapter reading the
whole file makes "swap the data source only" false wherever a screen joins several owners.

**The fixture fixes *what* each value is — name, meaning, type, format, owner — not the
codebase's types.** Where the codebase already has a type for an entity (an existing read model,
status or label type), use that type and its names, and record the mapping *fixture field →
codebase field* in `NOTES.md`. Where it has none, keep the fixture's names. Nesting and grouping
follow the codebase's boundary types and size rules.

The seam's **field shape is a contract** for the later backend task: "swap the data source only"
holds *only* if the real source delivers the same shape. Record the shape you built per screen in
`NOTES.md` as a requirement that task must honor — never silently pick a convenient shape the
template would then have to be re-plumbed away from.

## 7. Definition of done
- [ ] Every screen implemented **at its real route, in the real app shell, with real components**,
      and committed. No preview/gallery/bezel page anywhere.
- [ ] Going live = **swap the data source only** — no re-layout needed (mock data sits at
      the real seam).
- [ ] **No backend built.** Nothing that makes the data real — no schema, persistence, writes,
      queries against real storage, or real integrations (§0's "does going live keep it?" test).
      Backend is a separate task; unmet needs are stubbed + noted in NOTES.md, not implemented.
- [ ] One fixture source per data owner (§6); fixture → codebase name mappings in NOTES.md.
- [ ] Every CHECKLIST box ticked (or blockers logged in NOTES.md).
- [ ] `INVENTORY.md` fully ticked, and an **adversarial completeness pass** (by a non-builder)
      found nothing missing vs the inventory + reference PNGs.
- [ ] Mock field shapes recorded in NOTES.md as the seam contract for the backend task.
- [ ] Dark theme verified; WCAG AA met; chip focusable with tooltip on focus.
- [ ] Tokens only; copy in English; no invented features; no widened data.
- [ ] `git diff TopBar.svelte` shows only the chip and its group; `Sidebar.svelte` untouched.
- [ ] No improvised/broken icons or assets — every affordance uses a real app component.
- [ ] Every spec change written back into the ticket (or listed under **Ticket updates needed** in
      NOTES.md), and every unlisted ticket↔package conflict flagged there.
- [ ] NOTES.md summarizes decisions made + anything to flag to a human.

## 8. ProfitOfExile specifics
- Read the repo's `AGENTS.md` and `docs/README.md` first; desktop work follows the `desktop` agent profile.
- Tests: read `/root/repos/pipeforge/plugin/lib/test-quality-contract.md` and `test-author-contract.md` before writing any.
- Never push; every push to `main` deploys.
