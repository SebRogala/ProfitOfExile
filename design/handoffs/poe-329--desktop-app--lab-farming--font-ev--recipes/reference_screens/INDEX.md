# Reference screens — index

Renders of `FontEvRecipes-prototype.dc.html` (canvas page **Handoff · build sources**,
https://claude.ai/artifact/Dij6S2j91a9x2HXCVx5cxp), captured with the playbook's
`tools/capture-canvas.mjs` in headless Chromium.

- **Size:** 1024 × 330, the Recipes section alone with 16 px padding (the EV table above and Pool Overview below are unchanged and not drawn).
- **Captured:** 2026-11-02, after the last prototype change on the same day.
- **How to reach a state:** the prototype's props `exceptionalState` (priced / loading / missing / stale) and `lensState` (priced / loading / missing / stale / negative).

## `desktop/`
| File | Screen / state | What to match |
|---|---|---|
| `01-recipes--priced.png` | both priced | disclosure header, two cards, all rows, EV +317c, footers |
| `02-recipes--loading.png` | both loading | "pricing…" in every price and the EV, footers without age |
| `03-recipes--price-missing.png` | missing | Enlighten "no price"; lens "no price", EV "— needs the lens price" |
| `04-recipes--stale.png` | stale | yellow "updated 3h ago" badges |
| `05-recipes--lens-negative-ev.png` | exceptional priced, lens negative | lens 60c, EV −35c in red |
