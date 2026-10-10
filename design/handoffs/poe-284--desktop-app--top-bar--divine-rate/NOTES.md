# NOTES.md — top-bar divine rate (POE-284)

Implementer notes for handoff 1. Ticket text is referenced by id, never pasted (public repo).

## 1. Ticket updates needed (POE-284)

Spec changes from `README.md § Spec changes vs ticket`, to write back into POE-284:

- **Placement:** the chip moved from the centre group (before the status dots) to the right side of the top bar,
  immediately left of the window controls, 12px gap.
- **Chip text:** the ticket's "1 div = 360c" form became divine icon, "1 = 360", chaos icon.

Unlisted ticket ↔ package conflicts, built as the package shows:

- **"Every surface converts with this store"** is delivered for the chip + store only. Lab, the comparator overlay,
  Currency Exchange and Harvest Flipping are deferred (§4).
- **Loading and cold** are one rendered state ("—"): the ticket lists them separately, the package draws one (ref 02).
- **The ticket's open "Refresh" question** is answered: the store refreshes on the existing
  `currency-exchange-updated` Tauri re-emit of `poe/currency-exchange/updated` (LabPage's Mercure connection), with
  the same debounced, jittered `refetchDelay()` the Currency Exchange page uses. No second EventSource.
- **WCAG AA vs `--text-muted`:** README Constraints require AA contrast; CHECKLIST requires the cold dash in
  `--text-muted`. Measured `--text-muted #888` on `--surface #16213e` = **4.48:1**, below AA 4.5:1 for 12px text.
  Built per CHECKLIST; an unresolved AA owner item (Supervisor ruling 2, ANSWER (2)). For reference, measured on the
  same surface: `--warning #fbbf24` 9.52:1, `--text #e0e0e0` 12.04:1; tooltip secondary `#9ca3af` on `#1a1d27` 6.62:1.

## 2. Fixture → codebase mapping

`fixtures.json` has one owner, `internal/exchange`; one adapter, `desktop/src/lib/stores/divine-rate-fixture.ts`,
behind one port, `fetchDivineRate()` in `desktop/src/lib/api.ts`.

| Fixture field | Codebase | Note |
|---|---|---|
| `rate.divineChaosRate` 360.07 | `DivineRate.divineChaosRate` | ready and stale; `null` when cold |
| `rate.priceHour` `2026-09-09T23:00:00Z` | `DivineRate.priceHour` | ready; `null` when cold |
| `rate.updatedMinutesAgo` 6 | `DivineRate.updatedAt` = `now − 6 min` (ISO) | the port carries a timestamp, the age is formatted against the store's clock |
| `rate.state` `ready` | `DivineRate.state` | the product path shows the fixture's state; `readDivineRateFixture(now, state)` reaches cold and stale |
| `rate._note` stale example "lastHour 21:00, unreachable since 21:04" | stale: `priceHour` `2026-09-09T21:00:00Z`, `unreachableSince` `2026-09-09T21:04:00Z` | **interpretation:** the note gives clock times only; the date is taken from `rate.priceHour` |
| `items.divine` `Metadata/Items/Currency/CurrencyModValues` | `DIVINE_ID` (`$lib/exchange/view`) | imported, not redefined |
| `items.chaos` `Metadata/Items/Currency/CurrencyRerollRare` | `CHAOS_ID` (`$lib/exchange/view`) | imported, not redefined |
| `_formats.chip` / `tooltip` / `hour` / `age` | `formatChipRate` / `formatTooltipRate` / `formatPriceHour` / `formatAge` (`stores/divine-rate.ts`) | see §5 for the ≥1,000 grouping interpretation |

## 3. Seam contract for the backend task

The port the real source must fill, unchanged, so going live swaps the data source only:

```ts
interface DivineRate {
	divineChaosRate: number | null;
	priceHour: string | null;   // ISO hour the rate was measured in
	updatedAt: string;          // ISO time the source last updated it
	state: 'ready' | 'cold' | 'stale';
	unreachableSince: string | null;
}
```

- **Ungated**, served by `internal/exchange`: every device reads it.
- The rate is a finite number > 0 or `null`. `null` = no exchange hour yet, or no divine/chaos trade in the hour —
  the owner's `0` for that hour maps to `null`; the store also normalizes a stray `0`, negative or non-finite value to `null`.
- `state` comes from the source; the store derives none of its own (§8).
- Refresh relies on the `poe/currency-exchange/updated` Mercure topic, re-emitted by LabPage as the Tauri event
  `currency-exchange-updated`.
- **Going live** = replace `fetchDivineRate()`'s body in `desktop/src/lib/api.ts` and delete
  `desktop/src/lib/stores/divine-rate-fixture.ts`. The store, its pure half and the top bar do not change.

## 4. Deferrals with reasons

- **Lab** (`formatPrice`, the Rust `trade_lookup` normalization, the queue snapshot rate) and **the comparator
  overlay** stay on `status.divinePrice`. The store is fixture-fed, and the three Lab paths must move together or the
  queue's price delta picks up rate drift as a price move (homes audit High 3, High 4, Medium 1; Supervisor ruling 1, Q1).
  The README's open "Lab conversions" decision moves to the backend task.
- **Currency Exchange** keeps its response's `divineChaosRate` for un-converting entry prices: the server valued
  each play at that hour's rate, so a store read at another moment would mis-convert (homes audit High 1; ruling 1, Q2).
- **Lab header and MarketOverview** keep their poe.ninja rate readouts (ruling 1, Q4) — a follow-up, since they can
  now disagree with the chip.
- **Harvest Flipping** wires to this store after this merge (ruling 1, Q5).

## 5. Open decisions with interims

- **Cold server fallback** (poe.ninja stand-in vs "—"): interim "—" (README, ref 02).
- **Lab conversions:** moved to the backend task (§4).
- **`--text-muted` contrast** 4.48:1 on the cold dash: unresolved AA owner item (§1).
- **Tooltip grouping at ≥1,000:** `_formats.tooltip` says only "one decimal"; the tooltip groups like the chip
  (`1,234.5`) for consistency — an interpretation.
- **Shared `Tooltip` follow-ups, not built:** its hardcoded hex colours (not tokens) and no Escape-to-dismiss.

## 6. Exchange-grant observation

README Constraints say the Currency Exchange endpoint needs the `exchange` grant. In the code,
`/api/currency-exchange/plays` is registered with no server-side feature check (`internal/server/server.go`, route
registration); `FeatureExchange` appears only in `internal/device/entitlements.go`, and the desktop entitlements
store describes its gates as "Hiding, not securing". A separate light endpoint for the rate is still justified — by
payload size (the plays endpoint returns the full plays list) — not by access control (homes audit Medium 4).

## 7. Verification record

The 1:1 pixel pass (AGENT §3 steps 4–5) was **waived by the Operator 2026-10-10**, now and after merge: no render,
no screenshot. Instead, the chip markup + CSS and the `divineRateView` test strings were read against refs 01–03 and
`fixtures.json`. Ref colours were sampled from the PNGs' pixels (chip row, y 8–28).

- **Ref 01 (ready).**
  - Element order divine icon, "1", "=", "360", chaos icon: `desktop/src/lib/components/TopBar.svelte:99-103`.
  - Number tokens: "1" and "360" sampled `#dedede`–`#dfdfdf` ≈ `--text #e0e0e0`; `.rate-chip` `color: var(--text)`
    (`TopBar.svelte:242-253`); numbers in `'Consolas', 'Monaco', monospace` 600 (`.rate-n`, `TopBar.svelte:264`).
  - 12px gap to the window controls: `.right` `gap: 12px` (`TopBar.svelte:236-240`); `.window-controls` is the second
    child of `.right` (`TopBar.svelte:109`); in the ref the chip ends ≈ x 866–870 and the controls start at x 886.
  - Tooltip lines: `divine-rate.test.ts` "words the ready state as reference 01" asserts the three lines exactly, in UTC.
- **Ref 02 (no rate yet).**
  - The chip still draws icon, "1", "=", "—", icon (`TopBar.svelte:99-103`; ref 02 shows "1 = —").
  - "1" and "—" sampled `#858585`–`#868686` ≈ `--text-muted #888`; "=" sampled `#dedede`. `.cold .rate-n` colours both
    `.rate-n` spans (`TopBar.svelte:274`), which is what the ref shows.
  - Tooltip: `divine-rate.test.ts` "words the cold state as reference 02"; "prints neither 0 nor NaN anywhere in the cold state".
- **Ref 03 (stale).**
  - "1" and "360" sampled `#f7bc23`–`#fabe23` ≈ `--warning #fbbf24`; "=" sampled `#dedede`; `.stale .rate-n` colours
    both `.rate-n` spans (`TopBar.svelte:269-272`), as the ref shows.
  - "STALE" after the chaos icon: `<span class="rate-tag">stale</span>` rendered uppercase by `.rate-tag`
    `text-transform: uppercase` 0.625rem, `--warning` (`TopBar.svelte:104-106`, `:269-282`).
  - Tooltip: `divine-rate.test.ts` "words the stale state as reference 03".
- **Observed differences, not built:**
  - **Tooltip horizontal anchor.** In all three refs the popup's right edge sits near the chip's right edge (ref 01
    popup x ≈ 514–875). The shared `Tooltip.svelte` anchors the popup's LEFT edge to the wrapper and clamps it to the
    window, so at 1024px wide it lands ≈ x 656–1016. Changing the shared component's placement was not in scope (D10).
  - **Hour zone.** The refs print the fixture hour in UTC (`09 Sep 23:00`); the app prints it in the machine's local zone
    (`_formats.hour` "local"), so a CEST machine shows `10 Sep 01:00`.
- **No unit seam (surfaced finding).** `.svelte` files have no unit harness in this repo, so the `+layout.svelte` init
  (`desktop/src/routes/(app)/+layout.svelte:616`), the `Tooltip.svelte` focus open / `role="tooltip"` / `aria-describedby`
  / live text update (`Tooltip.svelte:13-19`, `:59`, `:100-105`, `:120-121`) and the chip's drag exclusion (a `<button>`
  matched by `startDrag`'s `closest('button, a, .status-dot')`, `TopBar.svelte:66-70`) are verified by diff reading
  only. The smallest seam — extracting them into a pure `.ts` — would move the shared component's DOM code out of the
  component for one test; the observable that would unblock them is a component test harness (none exists).

## 8. Known gaps

- **F1 — first-connect / reconnect refresh and client-side stale detection.** Handed to the backend task, which builds the real adapter behind `fetchDivineRate()`. That task decides how the source fills `state` and `unreachableSince` (connection liveness, failed reads) and how a read that fails before the server is known, or during a launch-time outage, recovers when the server first answers. Until then the store refreshes only at init and on `currency-exchange-updated`, and stale shows only when the source reports it. Sources: `lanes/POE-284/plan-review-r1.md` F1; `lanes/POE-284/ruling-2.md` SCOPE RULING. Mirrored in NOTES.md §8.
