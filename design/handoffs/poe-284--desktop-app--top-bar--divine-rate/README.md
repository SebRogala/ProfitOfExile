# Desktop app → Top bar → Divine rate

**Handoff 1 — 2026-10-10** · ticket POE-284 (relates to POE-283)

## What's in this package
- **Build source:** `TopBar-prototype.dc.html`. It is not copied here (playbook `LOCAL.md`). The file is
  `design/canvas/desktop-app--strategies--harvest-flipping/project/TopBar-prototype.dc.html`, rendered on the
  canvas https://claude.ai/artifact/PTogG15PdG1WJYjbDqNHJf, page "Handoff · build sources". Its `state`
  tweak switches between ready, cold and stale.
- **Provenance only, do not build from it:** `TopBar.dc.html` and every other board on that canvas.
- `README.md` — the spec (this file).
- `CHECKLIST.md` — the acceptance contract.
- `fixtures.json` — the data.
- `reference_screens/` — ground truth, 3 PNGs (index: `reference_screens/INDEX.md`).
- `AGENT.md` — the implementer's protocol. Read it first.

Give the agent: _"Read `AGENT.md` and follow it exactly. Implement the frontend 1:1 against
`reference_screens/` using `fixtures.json`. Do not stop until every `CHECKLIST.md` box passes; log
anything you can't resolve in `NOTES.md`."_

## Spec changes vs ticket
Compared against: POE-284 (description as of 2026-10-10). There is no intake brief; the ticket was filed from the design session.
- Placement: "centre group, before the server/OCR status dots" → **right side of the top bar, immediately left of the window controls** (12px gap). Why: the owner finds it more natural to look top right than at the middle.
- The ticket's UI text "`1 div = 360c`" → **divine icon, "1 = 360", chaos icon**. Why: it matches the item icons used across the app (owner reviewed the mock).

## Tokens
All from `desktop/src/tokens.css`; this is chrome, so it uses the legacy set:
- `--text` for the numbers, in `'Consolas','Monaco',monospace` / 600 at 12px
- `--text-muted` for the cold dash
- `--warning` for the stale value and its "STALE" tag (0.625rem uppercase)
- Hover: `rgba(255,255,255,.06)` on a 4px radius, the same hover family as `TopBar.svelte`'s window buttons.
- The tooltip is the existing `Tooltip` component.

## Screens
### Top bar (every page)
- `TopBar.svelte` stays as it is. One element is added: the rate chip in a right-hand group, before
  `.window-controls`.
- Chip contents: Divine Orb icon (18px), "1", "=", the rate rounded to a whole chaos ("360"), Chaos Orb icon (18px).
- Hover or focus shows the tooltip with three lines:
  - **"1 divine = 360.1 chaos"**
  - "Currency Exchange, hour of 09 Sep 23:00 · updated 6 min ago"
  - "Every chaos ↔ divine figure in the app uses this rate."
- The chip is `-webkit-app-region: no-drag`, like every other control in the bar.

## Interactions & state
- **Ready** (reference 01): as above.
- **No rate yet** (reference 02): loading, or the server holds no exchange hour. The value is "—" in
  `--text-muted`, and the tooltip reads "No divine rate yet" / "Waiting for the first Currency Exchange hour."
- **Stale** (reference 03): the server is unreachable. The last value stays, in `--warning`, followed by "STALE".
  The tooltip reads "1 divine = 360.1 chaos (stale)" / "Last known rate from 21:00; server unreachable since 21:04."
- The value refreshes on the exchange's Mercure topic (`poe/currency-exchange/updated`) without a reload.

## Data shapes
See `fixtures.json`. One owner: the Currency Exchange module (`internal/exchange`). The desktop reads the
rate from **one store**, and every chaos/divine conversion in the app reads that same store. Today
`setDivineRate` in `price.svelte.ts` is fed by the Lab page and by the comparator overlay; both move to
this store.

## Assets
Divine Orb and Chaos Orb icons through the existing icon path (`ItemIcon` + `iconSrc`), with item ids
`Metadata/Items/Currency/CurrencyModValues` and `Metadata/Items/Currency/CurrencyRerollRare`.

## Constraints
- Nothing else in the top bar or the sidebar changes.
- The rate must reach every device. The CX endpoint needs the `exchange` grant, so this needs an ungated
  source (backend: separate task; stub at the seam).
- WCAG AA contrast; the chip is keyboard-focusable and its tooltip opens on focus.
- Out of scope: the server endpoint and retiring the poe.ninja fallback (backend task).

## Open decisions — questions, not requirements; do not invent answers
- **Cold server fallback**: should the poe.ninja rate stand in until the first exchange hour, or should the
  bar show "—"? Interim: "—" (reference 02).
- **Lab conversions**: the Lab page currently converts at the lab cache's `divinePrice`. Switching it to this
  store changes Lab's numbers slightly. Confirm with the owner before the backend task lands.
