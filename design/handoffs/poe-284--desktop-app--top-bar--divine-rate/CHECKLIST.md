# CHECKLIST.md — acceptance contract for the top-bar divine rate (POE-284)

## ⛔ MUST NOT DROP
- [ ] **No rate yet** shows "—" in `--text-muted`, never "0" or "NaN" (reference 02).
- [ ] **Stale** keeps the last value in `--warning` with "STALE" (reference 03).
- [ ] The chip sits **right of the centre group, immediately left of the window controls**, and the bar's drag area still works around it.
- [ ] The tooltip opens on hover and on keyboard focus.
- [ ] Every chaos/divine conversion in the app (Lab, the comparator overlay, Currency Exchange, Harvest Flipping) reads the **same** store as the chip.
- [ ] Nothing else in `TopBar.svelte` or `Sidebar.svelte` changes.
- [ ] Tokens only; copy exact (including "↔").

## Top bar
- [ ] Ready: divine icon, "1 = 360", chaos icon; tooltip with three lines (reference 01).
- [ ] Refresh on `poe/currency-exchange/updated` without a reload.
