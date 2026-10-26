# POE-316 implementation notes

Status: current implementation, uncommitted; host gates recorded below. Runtime acceptance remains with the orchestrator.

## Interim decisions

The implementation brief explicitly settles the handoff's three open questions for this delivery:

- Tools appears in release builds with Chaos ⇄ Div as its only entry. Dev Tools and placeholders remain dev-only, in both sidebar densities.
- No access grant; the mounted page is ungated.
- With no rate, conversion-dependent values display `—` and their total Copy buttons are disabled. Chaos totals in Chaos currency and rate-independent order rows remain available.

## Integration and fixture mapping

- `/chaos-div` is a navigation-store path, matching existing mounted main views. It adds no SvelteKit page route. The app's existing `navView` preference still remembers the selected page; calculator inputs themselves never persist.
- `fixtures.rate` (the vectors' fixed test value) → `currentDivineRate()` inside `$derived` in production. The page owns no rate fetch or adapter; the existing rate store is already live. A stale held rate remains usable under that store's API.
- `defaults.mode`, `currency`, `amount`, `ratio`, `oneSide` → same-named page-local state and `CHAOS_DIV_DEFAULTS`; no new backend shape is required.
- `vectors[].input` → `ChaosDivInput`; `vectors[].expect` → same-named result fields. `perItemChaos` is the displayed rounded number; totals use the unrounded per-item price, as `renderVals` does. `perItemC`/`perItemD` carry formatted page strings.
- The handoff fixture is copied verbatim to `desktop/src/lib/tools/__fixtures__/chaos-div.json` for tests, since the desktop Docker mount contains only `desktop/`. It never supplies a production rate.
- The referenced README has no separate “Store API” heading. Its `stores/divine-rate.svelte.ts` registry row and the store source define the API used here.

## Reference behavior and local choices

- The canvas groups by displayed ratio, retains each bucket's first candidate price for sorting/percentage, then updates its order quantities to the largest item count. The calculator preserves this behavior because the supplied vectors pin it; percentages follow that retained price.
- The canvas bounds effective amount to 1–100,000 and floors fractions. The page uses the same bound for input/steppers and normalizes the visible amount on blur. Invalid/empty/nonfinite ratios show unknown values and hide the table, avoiding fabricated zero prices.
- Candidate orders whose display equals the typed display are explicitly excluded, including a floating-point boundary not covered by the six supplied vectors.
- `SegmentedButtons` and `Button` are the existing components; page-scoped sizing and palette rules adapt them to the reference. Numeric inputs/steppers have no existing general-purpose registry component to reuse. Reset/swap use the canvas's inline stroke SVG convention; the sidebar uses the title's `⇄` glyph.
- Copy reports `Copied` only after clipboard success, clears after 1.2 seconds, and invalidates pending success on Reset/input changes. Failure logs a warning without false success feedback.
- Main.dc.html uses legacy shell colors. The page maps them to the specified lab palette and uses its existing yellow/green tokens for deltas/first-row tint. No tokens, TopBar, shared controls or dependencies changed.

## Ticket updates needed

The handoff lists no spec changes versus POE-316. Tracker access is unavailable, as stated in the brief. Record the three interim decisions above in the ticket if they are not already recorded there. No other ticket conflict was found.

## Verification

- `npx vitest run src/lib/tools/chaos-div.test.ts src/lib/stores/navigation.svelte.test.ts`: 140 passed across 2 files (96 calculator tests, eight calculation vectors and three mode transitions).
- `npm test`: 4,069 passed across 79 files.
- `npm run check`: 497 files, 0 errors and 0 warnings.
- `git diff --check`, verbatim fixture comparison and untouched TopBar check passed.
- Mutation check: in an allocated scratch copy, changing Divine's chaos conversion factor to `1` made six calculation tests fail. Shared production files were never mutated. A second scratch mutation restoring the incomplete overflow guard failed the new extreme-ratio percentage regression.

## Unresolved / delegated gates

Per brief §4, Docker and Tauri were not run and no sandbox escalation was requested. AGENT.md §3 steps 4–5 (render/screenshots/comparison) and §4 step 5 (independent adversarial pass) are deferred to the orchestrator. Runtime narrow-window behavior, keyboard interaction, clipboard feedback and visual/contrast acceptance therefore remain unverified. The requested throwaway runtime swatch check is also unavailable; tokens were inspected in source.

No reference PNGs were supplied. Visual implementation follows the local Main.dc.html source; visual fidelity is not claimed as verified. Inventory implementation checks distinguish source coverage from these pending runtime gates. All changes remain uncommitted, overriding AGENT.md's per-screen commit instruction as required by the implementation brief.

- Fix round 1b (supersedes the first-candidate note above): corrected both fixture copies’ delta strings to the shown orders’ currency/items prices; the prototype now follows the shown order instead of retaining the first candidate price.

- Fix round 3: owner-approved mode/item-side flip, minimum whole-order cutoff/message, and four-decimal per-item chaos below 0.1c are implemented in the page and prototype; two calculation vectors and three mode transitions were generated from the updated prototype, with the six previous vectors verified unchanged. Regression checks reproduce the prior behavior in scratch.

## Fix round 3 prevention record

Incident — buying mode retained the selling ratio layout · severity: low.
- What: observed in the owner’s in-game report, entering 906 while buying displayed the reciprocal price and unreachable 1c outbids.
- Root cause (Observed): the old mode handler changed only `mode`, leaving the fixed item-side 1 on the previous side; the old candidate loop started at one item regardless of the typed price.
- Caught by: owner runtime testing, then the focused regression tests.
- Prevention applied: `switchChaosDivMode` preserves the price orientation; the candidate minimum and precision rules now have regression tests in `chaos-div.test.ts` and accepted behavior in the handoff README/CHECKLIST. No proposed prevention remains unrouted.

What worked (the net held): the owner found the layout defect; red tests reproduced it; scratch probes broke all six targeted regression checks; prototype-generated vectors verified the six existing vectors unchanged.

Upstream fixes (not mine): none. Ticket updates needed: record the owner-approved fix-round-3 design changes in POE-316 when tracker access is available.

Carry-forward checklist:
1. Run the calculation and mode-transition vectors after changing price orientation or order eligibility (`chaos-div.test.ts`).

Prevention homes: the calculation module, its tests, and the handoff README/CHECKLIST; runtime visual acceptance remains with the orchestrator.
