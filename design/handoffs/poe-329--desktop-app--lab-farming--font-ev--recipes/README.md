# Desktop app → Lab Farming → Font EV → Recipes (POE-329)

**Handoff 1 — 2026-11-02**

## What's in this package
- `design/canvas/desktop-app--lab--divine-font/project/FontEvRecipes-prototype.dc.html` (canvas: https://claude.ai/artifact/Dij6S2j91a9x2HXCVx5cxp, page Handoff · build sources) — build source. It mounts `RecipeCard.dc.html` from the same folder, which is part of the build source; shared styles `theme.css`, sample data `data.js` beside them.
- `README.md` — spec (this file).
- `CHECKLIST.md` — acceptance contract.
- `fixtures.json` — data.
- `reference_screens/` — ground truth, 5 PNGs (index: `reference_screens/INDEX.md`).
- `AGENT.md` — implementer's protocol; read first.
- Sibling package: `../poe-328--desktop-app--overlay--font-alerts/` (in-game alert overlay). The exceptional-support card here shows the same three prices; POE-328 owns where they come from.

Give the agent: _"Read `AGENT.md` and follow it exactly. Implement the frontend 1:1 against
`reference_screens/` using `fixtures.json`. Do not stop until every `CHECKLIST.md` box passes; log
anything you can't resolve in `NOTES.md`."_

## Spec changes vs ticket
Compared against: POE-329 as of 2026-11-02 + the Supervisor brief `font-crafts-design-brief.md` (records dir `poe-supervisor-2026-11-01`, Updates 1–3; gitignored, not in the repo).
- Ticket: "Font EV section: include the Lens craft EV" → **build a "Recipes" section of cards below the per-variant EV table and above Pool Overview, one card per craft recipe, a grid that takes more recipes without layout change.** Why: Operator, Update 1 (2026-11-02).
- Ticket: Lens only → **the section also carries an "Exceptional support" card (Enhance / Enlighten / Empower, level 1 uncorrupted prices).** Why: Operator, Update 1 ("2 crafting options now").
- Ticket: "a button that opens the support-gem buy search" → **no buy button.** Why: Operator, Update 2: the gem cannot be bought while in the lab. (The Operator called the card "the in-lab overlay"; the card lives in the main window, and the Supervisor ruled the button goes from it.)
- Ticket: feeder = support gem search (req. level ≥ 76, gem level ≥ 3) → **the cost line reads exactly "Exceptional Support Gem lv 3+", no sub-line.** Why: Operator, Update 2. The server search itself is unchanged (POE-329 backend plan).

## Token and component changes
Token file: `desktop/src/tokens.css`. Tokens: None added or changed (`--color-lab-surface`, `--color-lab-border`, `--color-lab-text`, `--color-lab-text-secondary`, `--color-lab-red`, `--color-lab-yellow`). Components: None changed; the disclosure header matches the existing "Pool Overview" toggle in `FontEVCompare.svelte`. Design System artifact: none exists for this repo; no re-sync.

## Screens
### Font EV → Recipes section
- **Placement:** in `FontEVCompare.svelte`, directly after the per-variant EV table (1/20, 20/0, 20/20) and before the "Pool Overview" disclosure. Normal mode only (not Dedication mode).
- **Header:** a disclosure toggle "Recipes", built exactly like the existing "Pool Overview" toggle (its ▼ / ▶ glyph and style; the prototype draws an SVG chevron as a stand-in); open by default.
- **Cards:** CSS grid `repeat(auto-fill, minmax(320px, 1fr))`, gap 12 px, cards top-aligned (heights differ). Card: `--color-lab-surface`, 1 px `--color-lab-border`, radius 4 px, padding 10 px 12 px, gap 6 px.
- **Card anatomy (shared by every recipe):** title (14 px/700); the font option text (12 px, secondary); a "Gives" group (11 px uppercase label, 0.5 px letter-spacing, 1 px rule under it) with one row per outcome — name left, price right (mono 700); optional "Costs" group, same row style; optional "EV per craft" line (label 600 + "lens − support" 11 px secondary; value mono 700 1.25rem) above a 1 px rule; footer (11 px, secondary): price source left, age right, no wrap.
- **Exceptional support card:** title "Exceptional support"; option "Exchange a Support Gem for a random Empower, Enlighten, or Enhance Support"; group label "Gives one at random"; rows Enhance Support, Enlighten Support, Empower Support (fixed order); no Costs, no EV; footer "poe.ninja · level 1, uncorrupted" + age.
- **Facetor's Lens card:** title "Facetor's Lens · 60%"; option "Sacrifice a Gem to gain 60% of its experience as a Facetor's Lens"; Gives: "Facetor's Lens" with sub-line "stored experience ≥ 999,627,000" (11 px secondary); Costs: "Exceptional Support Gem lv 3+" (no sub-line); EV per craft = lens − support, "+317c" in text colour, negative "−35c" in `--color-lab-red`; footer "trade · median of the 3 lowest" + age.

## Interactions & state
- **Loading:** every price and the EV read "pricing…" (12 px italic, secondary); footer without age.
- **Price unknown:** that row's price reads "no price"; a missing lens price makes the EV "— needs the lens price". Rows never hide (ADR-017/018). A missing support price, interim: EV "— needs the support price" (not drawn; same treatment).
- **Stale:** the age becomes a yellow tinted badge "updated 3h ago".
- **Negative EV:** red value with the real minus sign. Red on the card ground measures 4.47:1, so it passes only as large text — keep it at 1.25rem bold.
- The only interactive element is the disclosure header (keyboard reachable, `aria-expanded`).

## Data shapes
See `fixtures.json`. Lens and support prices come from the POE-329 backend (server trade queue, median of the 3 lowest listings) through the endpoint `FontEVCompare` already reads; the EV is computed where that plan puts it.

## Assets
None external. Prices through `formatPrice` / `formatPriceSigned` (`desktop/src/lib/price.svelte.ts`).

## Glossary
- **Recipe** — a Divine Font craft with known inputs and outputs, priced as a card.
- **Feeder** — the gem sacrificed in the craft (here: the exceptional support at level 3+).

## Constraints
Dark only; WCAG AA (measured on `--color-lab-surface`: text 13.3:1, secondary 6.6:1, yellow badge 6.7:1, red 4.47:1 large-text only); English, item names exactly as in-game; desktop window 1024×768 default. Out of scope: the 30% lens craft; any buy button; the overlay (POE-328).

## Open decisions — questions, not requirements; do not invent answers
- **Exceptional card EV:** none is shown (odds of the three and the feeder cost are not defined). Interim: prices only.
- **Price source for the exceptional card** — decided in POE-328. Interim: poe.ninja gem prices, footer "poe.ninja".
- **Stale threshold** for both cards. Interim: 2 h; log it in `NOTES.md`.
- **Lens and support prices on the references are illustrative** (412c / 95c, negative case 60c / 95c); no trade-queue price exists yet.
