# INVENTORY.md — top-bar divine rate (POE-284)

Every screen × state × affordance this delivery must contain, mined from `CHECKLIST.md` (incl. MUST NOT DROP)
and the three reference PNGs. One screen: the top bar, on every page of the main window. Dark theme only.
The pixel pass is waived by the Operator 2026-10-10; visual rows are verified by reading code + fixtures
against refs 01–03 (`NOTES.md` §7).

## States (each supplied by the fixture adapter `readDivineRateFixture(now, state)`)

- [x] **Ready** (ref 01): divine icon, "1", "=", "360", chaos icon; tooltip "1 divine = 360.1 chaos" /
      "Currency Exchange, hour of 09 Sep 23:00 · updated 6 min ago" / "Every chaos ↔ divine figure in the app uses this rate."
      — `divine-rate.test.ts` "ready view"; `TopBar.svelte` chip markup.
- [x] **No rate yet / cold** (ref 02): divine icon, "1", "=", "—" in `--text-muted`, chaos icon; tooltip
      "No divine rate yet" / "Waiting for the first Currency Exchange hour." / line 3. Never "0" or "NaN".
      Loading and "no exchange hour" are this one state (`NOTES.md` §1).
      — `divine-rate.test.ts` "cold view"; `.cold .rate-n` CSS.
- [x] **Stale** (ref 03): last value "360" in `--warning`, followed by the "STALE" tag (`--warning`, 0.625rem uppercase);
      tooltip "1 divine = 360.1 chaos (stale)" / "Last known rate from 21:00; server unreachable since 21:04." / line 3.
      — `divine-rate.test.ts` "stale view"; store test keeps the value; `.stale .rate-n`, `.rate-tag` CSS.
- [x] **Error (read rejected)**: no fourth drawn state — a rejected read leaves the last snapshot on screen
      (`divine-rate.svelte.test.ts` "a rejected read after a good one…"). Liveness-derived stale is a Known gap (`NOTES.md` §8).

## Affordances

- [x] Hover opens the tooltip — `Tooltip.svelte` `onmouseenter={show}` (unchanged).
- [x] Keyboard focus opens the tooltip — `Tooltip.svelte` `onfocusin={show}` / `onfocusout={hide}`; the chip is a `<button>`, natively focusable.
- [x] Popup is announced — popup `role="tooltip"` + unique `id`; the focused chip gets `aria-describedby`.
- [x] Focus ring — `.rate-chip:focus-visible` outline `1px solid var(--color-lab-blue)`.
- [x] Hover background — `.rate-chip:hover` `rgba(255, 255, 255, 0.06)` on a 4px radius (README value).
- [x] Tooltip live update while open — `Tooltip.svelte` `$effect` re-applies `text` while visible ("updated N min ago" and a refreshed rate).
- [x] Drag exclusion — the chip is a `<button>`, so `startDrag`'s `closest('button, a, .status-dot')` skips it; chip CSS `-webkit-app-region: no-drag`.
- [x] Drag still works on the bar background — `startDrag` and `.topbar` unchanged; `.right` is a plain `div`.
- [x] Refresh on `poe/currency-exchange/updated` without reload — store listens to the existing Tauri re-emit
      `currency-exchange-updated` with one debounced `refetchDelay()` read (`divine-rate.svelte.test.ts`).
- [x] Age advances while the app is open — 30 s tick on `divineRate.now` (`divine-rate.svelte.test.ts`).

## Placement

- [x] Chip right of the centre group, immediately left of the window controls, 12px gap — `.right` group in `TopBar.svelte`.
- [x] Nothing else in `TopBar.svelte` or `Sidebar.svelte` changes — diff limited to imports, two `$derived`, `.right`, chip markup + CSS.

## CHECKLIST mapping

| CHECKLIST box | Ticked | Evidence |
|---|---|---|
| No rate yet shows "—" in `--text-muted`, never 0/NaN | [x] | `divineRateView` cold tests; `.cold .rate-n` CSS read against ref 02; pixel pass waived by the Operator 2026-10-10 |
| Stale keeps last value in `--warning` with "STALE" | [x] | store stale test keeps the value; `.stale` CSS + tag markup read against ref 03; pixel pass waived |
| Chip right of centre group, left of window controls; drag area still works | [x] | `.right` group read against ref 01; `<button>` skipped by `startDrag`; pixel pass waived |
| Tooltip opens on hover and on keyboard focus | [x] | `Tooltip.svelte` diff: mouse handlers unchanged, `onfocusin={show}` / `onfocusout={hide}`, popup `id` + `role="tooltip"`, `aria-describedby` |
| Every conversion reads the same store | [x] partial | Met for chip + store only — Lab / overlay / CX / Harvest deferred, see `NOTES.md` §4 |
| Nothing else in TopBar / Sidebar changes | [x] | `TopBar.svelte` diff; `Sidebar.svelte` untouched |
| Tokens only; copy exact incl. "↔" | [x] | `tooltipHtml` equality tests (U+2194); CSS `var(--…)` except the README hover rgba |
| Ready: icon, "1 = 360", icon; three-line tooltip | [x] | ready `divineRateView` test; markup order read against ref 01 |
| Refresh on `poe/currency-exchange/updated` without reload | [x] | store event test; the topic → event bridge is existing `LabPage.svelte` code |
