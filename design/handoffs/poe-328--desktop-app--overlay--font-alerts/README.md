# Desktop app → Overlay → Divine Font craft alerts (POE-328)

**Handoff 1 — 2026-11-02**

## What's in this package
- `design/canvas/desktop-app--lab--divine-font/project/FontAlerts-prototype.dc.html` (canvas: https://claude.ai/artifact/Dij6S2j91a9x2HXCVx5cxp, page Handoff · build sources) — build source. It mounts `ExceptionalAlert.dc.html` and `TransfigureAlert.dc.html` from the same folder; those two are part of the build source. Shared styles `theme.css`, sample data `data.js` beside them.
- `README.md` — spec (this file).
- `CHECKLIST.md` — acceptance contract.
- `fixtures.json` — data.
- `reference_screens/` — ground truth, 6 PNGs (index: `reference_screens/INDEX.md`).
- `AGENT.md` — implementer's protocol; read first.
- Sibling package: `../poe-329--desktop-app--lab-farming--font-ev--recipes/` (Font EV Recipes section in the main window). It shows the same three exceptional-support prices; this package owns how those prices reach the overlay, that one owns the main-window card.

Give the agent: _"Read `AGENT.md` and follow it exactly. Implement the frontend 1:1 against
`reference_screens/` using `fixtures.json`. Do not stop until every `CHECKLIST.md` box passes; log
anything you can't resolve in `NOTES.md`."_

## Spec changes vs ticket
Compared against: POE-328 as of 2026-11-02 + the Supervisor brief `font-crafts-design-brief.md` (records dir `poe-supervisor-2026-11-01`, Updates 1–3; gitignored, not in the repo).
- Ticket: one alert, exceptional support only → **build an alert family: exceptional support + direct transfigure ("Transform a non-Transfigured Skill Gem to a Transfigured version").** Why: Operator, Update 3 (2026-11-02).
- Ticket: silent on several alerts → **when the same Font panel offers more than one alert craft, show all of them stacked in one column, 6 px apart.** Why: Operator, Update 3.
- Transfigure alert: **no list of potential (TOP) gems now.** The panel ends after its two text lines; a later task adds the gem list under them. Why: Operator, Update 3 ("leave room for it later, do not draw it").

## Token and component changes
Token file: `desktop/src/tokens.css`. Tokens: None added or changed. The panel uses the Gems Compare widget's values (`desktop/src/lib/compass/LabComparatorWidget.svelte` `.table`: `rgba(15,17,23,0.92)` ground, `rgba(42,45,55,0.8)` border, 8 px radius, `blur(8px)`; price colour `#fbbf24` = `--warning`). That widget hard-codes them; reuse its values or its class, and any `var(--…)` used must be declared in `tokens.css` (`overlay-tokens.test.ts`).
Components: None changed. New widget `lab.fontalerts` (name is the implementer's; see Open decisions).
Design System artifact: none exists for this repo; no re-sync.

## Screens
### Font alert stack (in-game overlay widget, Lab window)
- **Widget:** one new entry in `desktop/src/lib/overlay/widgets/widget-registry.ts`, module `lab`, label "Font alerts", `resizable: false`, rendered by a branch in `desktop/src/routes/overlay/lab/+page.svelte`. It gets its Settings → Overlay Positions row (Show + Configure) from the registry like every Lab widget. Display-only: no buttons, no hot rects, fully click-through.
- **Default position:** right of the Divine Font panel's OCR read region, top edges aligned, 12 px gap (scaled by the UI scale): `x = font.x + font.w + 12·scale`, `y = font.y`, from `placements.lab.font` (`ssot::font_region`). At 1920×1080 the region is [298, 220, 666, 681], so the stack sits at (976, 220) and is 260 px wide; the inventory starts at x ≈ 1262, so it fits with room to spare. Use the WidgetHost `defaultsFor` hook (temple `doorDefaults` is the precedent); a user placement from Configure always wins.
- **Never covers the font OCR region** (ADR-019): the region is read every tick while the panel is open.
- **Stack:** a column, 260 px wide, gap 6 px. One panel per alert craft the current font read lists, in the order the Font panel lists them (top to bottom). No craft listed → the widget renders nothing.
- **Exceptional support panel** (`ExceptionalAlert`): title "Exceptional support" (14 px/700), right-aligned "Font option" (11 px, secondary); line "Gives one of these at random" (11 px, secondary); three rows, fixed order Enhance Support, Enlighten Support, Empower Support — 30 px tall, 22 px gem icon (`desktop/src/routes/(app)/components/GemIcon.svelte`), name 13 px/600, price right-aligned mono 14 px/700 in `--warning`; rows split by a 1 px `rgba(42,45,55,.4)` rule. Footer (11 px, secondary): "poe.ninja · level 1, uncorrupted" left, age right ("6m ago").
- **Transfigure panel** (`TransfigureAlert`): title "Transfigure gem", right "Font option"; the option text "Transform a non-Transfigured Skill Gem to a Transfigured version" (11 px, secondary, wraps); line "You choose the version" (12 px, text colour).
- Panel padding 8 px 10 px; prices through `formatPrice` (`desktop/src/lib/price.svelte.ts`): chaos under 2 divines, "N.Nd" from 2 divines up.

## Interactions & state
- **Shown** while the latest Divine Font read lists `exchange_exceptional` and/or `transform_direct` (`font_parser.rs` option types); **hidden** when a read no longer lists them or the font session ends. The overlay rebuilds this from polled state (cross-window events are unreliable, OVERLAY-GUIDE).
- **Loading:** each price reads "pricing…" (11 px italic, secondary); footer shows the source line without an age.
- **One price missing:** that row stays, price replaced by "no price" (italic, secondary); the other rows keep their prices. Rows never hide or reorder (ADR-017/018).
- **Stale:** the age becomes a yellow tinted badge ("3h ago"). Threshold: Open decisions.

## Data shapes
See `fixtures.json`. Prices are per gem, variant level 1, uncorrupted. Age is the price source's fetch time, shown as "Nm ago" / "Nh ago".

## Assets
No external fonts; system UI stack + `'Consolas','Monaco',monospace` for prices (app convention). Gem icons through the existing `GemIcon` component.

## Glossary
- **Alert craft** — a Divine Font option worth flagging in-game: today exceptional-support exchange and direct transfigure.
- **Exceptional support** — Enhance, Enlighten and Empower Support.

## Constraints
Dark only; WCAG AA (measured: secondary text 7.4:1, amber price 11.2:1, yellow badge text 6.7:1 on their grounds); English, gem names exactly as in-game; overlay rules in `docs/OVERLAY-GUIDE.md` (transparent, click-through, physical px, tokens.css only). Out of scope: TOP gems for transfigure; the Facetor's Lens craft as an overlay; the 30% lens craft.

## Open decisions — questions, not requirements; do not invent answers
- **Price source for the three gems** (ticket: "existing gem price pipeline vs trade queue"). Interim: read the existing poe.ninja gem prices (`gem_snapshots`, variant "1", uncorrupted) and label the footer "poe.ninja"; if the trade queue is picked, the footer names it instead.
- **Stale threshold.** The trade lookup thresholds (warn 120 s, critical 600 s) are too tight for a periodic price. Interim: stale at 2 h; log the choice in `NOTES.md`.
- **Narrow screens:** if the space right of the font region is narrower than the stack (not seen at 16:9 or 16:10), interim: place it left of the region, same top; log it.
- **Widget id and label:** interim `lab.fontalerts`, "Font alerts".
- **The exact OCR wording** of both options in a live read (ticket open item). The parser's types already exist; confirm from a font read.
- **Sort of the stack** follows Font panel order; if the implementer finds the read carries no stable order, interim: exceptional above transfigure.
