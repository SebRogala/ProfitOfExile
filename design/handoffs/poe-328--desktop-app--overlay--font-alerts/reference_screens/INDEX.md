# Reference screens — index

Renders of `FontAlerts-prototype.dc.html` (canvas page **Handoff · build sources**,
https://claude.ai/artifact/Dij6S2j91a9x2HXCVx5cxp), captured with the playbook's
`tools/capture-canvas.mjs` in headless Chromium.

- **Size:** 300 px wide, transparent background; the stack is 260 px plus its shadow, drawn from the top-left. Heights are the content height.
- **Captured:** 2026-11-02, after the last prototype change on the same day.
- **Known capture artefacts (not design):** gem icons are dashed placeholders (use `GemIcon`); the panel is drawn on transparency, in game it sits over the scene with `blur(8px)`.
- **How to reach a state:** the prototype's props — `crafts` (both / exceptional / transfigure) and `state` (priced / missing / loading / stale).

## `desktop/`
| File | Screen / state | What to match |
|---|---|---|
| `01-alert--two-crafts-stacked.png` | crafts=both, state=priced | exceptional panel above transfigure panel, 6 px gap |
| `02-alert--exceptional-priced.png` | crafts=exceptional, priced | title row, sub line, three rows with amber prices, footer + age |
| `03-alert--exceptional-one-price-missing.png` | missing | Enlighten row "no price", other two priced |
| `04-alert--exceptional-loading.png` | loading | "pricing…" in all three, footer without age |
| `05-alert--exceptional-stale.png` | stale | yellow "3h ago" badge in the footer |
| `06-alert--transfigure-only.png` | crafts=transfigure | transfigure panel alone |
