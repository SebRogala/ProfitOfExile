# Desktop app → Tools → Chaos ⇄ Div

**Handoff 1 — 2026-10-26** · ticket POE-316 (relates to POE-284)

## What's in this package
- **Build source:** `Main.dc.html`. It is not copied here. The file is
  `design/canvas/desktop-app--tools--chaos-div/project/Main.dc.html`, rendered on the canvas
  https://claude.ai/artifact/MtdUEJxNHjaLHDPB1NX1ss (Play to interact; the Tweaks panel sets the demo rate).
  Its top bar and sidebar are stand-ins for the real shell. Its `renderVals()` holds the reference
  algorithm that produced `fixtures.json` → `vectors`.
- `README.md` — the spec (this file).
- `CHECKLIST.md` — the acceptance contract.
- `fixtures.json` — defaults, formats and calculation test vectors.
- `reference_screens/` — **no PNGs in handoff 1** (see its `INDEX.md`).
- `AGENT.md` — the implementer's protocol. Read it first.

Give the agent: _"Read `AGENT.md` and follow it exactly. Implement the page 1:1 against the canvas
artboard using `fixtures.json`. Do not stop until every `CHECKLIST.md` box passes; log anything you
can't resolve in `NOTES.md`."_

## Spec changes vs ticket
Compared against: POE-316 (description as of 2026-10-26). The ticket was filed from the design session; no changes.

## Tokens
All from `desktop/src/tokens.css`. The page is a Tools page, so it uses the `--color-lab-*` set like the
Strategies pages (`design/PROJECT.md` → Design system decisions). The canvas draws with the legacy
`--bg/--surface/--border` hexes; map them to `--color-lab-bg/-surface/-border`.
- Numbers: `'Consolas','Monaco',monospace` / 600.
- Undercut/outbid percentage: `--color-lab-yellow`. First row highlight: a faint `--color-lab-green` tint.
- Mode and Currency use the existing `SegmentedButtons`. Reset and Copy use the existing `Button`.

## Screens
### Tools → Chaos ⇄ Div (one page)
1. **Header:** title "Chaos ⇄ Div" with a **Reset** button on the right.
2. **Input bar** (one bordered surface, wraps on narrow windows):
   - **Mode:** Selling / Buying.
   - **Currency:** Chaos / Divine.
   - **Amount field** with − / + stepper buttons, minimum 1. Its label is **"Amount"** while the ratio is 1, otherwise
     **"Items I have"** (selling) or **"Items I want"** (buying).
   - **Ratio:** a number input with a fixed "1" beside it: `[x] : 1` or `1 : [x]`. A swap button (two-arrow icon,
     `aria-label="Swap which side is 1"`) moves the "1". Below it: "Per item `<c>c · <d>d`".
3. **Totals:** two cards side by side. "Total / Worth / Cost in chaos" shows a whole-chaos figure with a Copy button. "… in divine" shows two decimals with a Copy button, and under it "Whole split `Nd + Mc`". The lead word is
   "Total" at ratio 1, "Worth" when selling, "Cost" when buying.
4. **Undercut table** ("Nearest undercuts" / "Nearest outbids"): columns Ratio, Undercut/Outbid, I want, I have,
   Left over / Short of target. Up to 6 rows. **I want** and **I have** each have their own Copy button. The first
   row is highlighted. The table is **hidden** while the ratio is 1, or when no row exists.
5. There is no divine-rate line on the page; the top-bar chip already shows the rate.

## Interactions & state
- **Calculation** (the algorithm lives in the canvas `renderVals()`; `fixtures.json` → `vectors` pin it):
  - `r` = currency per item. The typed value is `v`. Selling reads the game's `want(currency) : have(item)`;
    buying reads `want(item) : have(currency)`. So `v` is `r` when (selling and the 1 is on the right) or
    (buying and the 1 is on the left), and `1/v` otherwise.
  - Totals: chaos = round(r × chaos-per-currency × amount), where chaos-per-currency is the rate for Divine and 1 for Chaos.
    Divine = chaos / rate.
  - Rows: the typed ratio's displayed bucket is its value ± 0.005. For each item count n from 1 to amount, take one
    candidate: when selling, the largest whole currency p with p/n below the bucket; when buying, the smallest p with
    p/n above it. Group candidates by their displayed ratio (2 decimals, whole numbers bare), keep the largest n per
    group, sort nearest first, and take 6. Partial fills exist in game (owner, 2026-10-26), so the largest order is
    the useful one.
- **Copy:** writes the bare value (integer, or the divine 2-decimal string) via the app's clipboard pattern
  (`navigator.clipboard.writeText`, as `harvest/controller.svelte.ts`). The button reads "Copied" for ~1.2 s.
- **Reset** restores `fixtures.json` → `defaults` (Selling, Chaos, 1, ratio 1, the 1 on the right). The page opens in
  the same state. The canvas opens on the Eye demo only to show the table.
- Recalculation is live on every input. Nothing persists.

## Data shapes
See `fixtures.json`. One external value: the divine rate, read from the existing store
(`$lib/stores/divine-rate.svelte`, POE-284). Everything else is page-local input. Put the calculation in a pure
module (no Svelte) so the vectors run as unit tests.

## Assets
Swap and Reset icons: inline stroke SVGs in the canvas. Use the app's icon convention if one exists; otherwise
inline SVG with `aria-hidden`. No item icons.

## Constraints
- Add one sidebar entry under **Tools**. Nothing else in the sidebar or the top bar changes.
- WCAG AA contrast; every control keyboard-reachable; the swap and stepper buttons have `aria-label`s.
- Out of scope: any server work, saving inputs, live competing ratios from the exchange data.

## Open decisions — questions, not requirements; do not invent answers
- **Visibility:** the Tools section renders only in `import.meta.env.DEV` today. Interim: show the section in release
  builds with only this entry. Dev Tools and the placeholder items stay dev-only.
- **Gating:** proposal is no grant (client-side maths over the public rate). Interim: ungated.
- **No rate yet** (store `cold`): not drawn. Interim: every figure that needs the rate shows "—" and its Copy is
  disabled. Chaos totals in Chaos currency still work.
