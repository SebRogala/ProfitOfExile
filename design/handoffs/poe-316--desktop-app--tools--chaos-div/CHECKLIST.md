# CHECKLIST.md — acceptance contract for Tools → Chaos ⇄ Div (POE-316)

Source implementation coverage is checked below. Runtime rendering, visual comparison, keyboard/clipboard interaction and adversarial acceptance are deferred by the implementation brief; see `NOTES.md`.

## ⛔ MUST NOT DROP
- [x] Every `fixtures.json` → `vectors` case passes as a unit test against the pure calculation module.
- [x] Ratio 1 hides the undercut table and labels the amount field "Amount".
- [x] The "1" swap flips the side and the maths (`1 : 6.12` selling ≠ `6.12 : 1` selling).
- [x] Buying produces **outbids** (price up), selling produces **undercuts** (price down).
- [x] Rows only show ratios whose displayed value differs from the typed ratio's.
- [x] Copy on chaos total, divine total, and on **both** I want and I have of every row; Copy puts the bare number.
- [x] Reset restores Selling, Chaos, 1, ratio 1 with the 1 on the right.
- [x] The rate comes from the existing divine-rate store; no rate line on the page; no new fetch.
- [x] No rate yet: rate-dependent figures show "—", never "0", "NaN" or "Infinity".
- [x] Only one new sidebar entry; `TopBar.svelte` untouched. Tokens only; copy exact (including "⇄", "−").

## Page
- [x] Header with Reset on the right.
- [x] Input bar: Mode, Currency, Amount stepper (min 1), Ratio with the fixed 1, the swap button and the per-item line.
- [x] Totals cards: lead word Total / Worth / Cost; whole split line.
- [x] Table title, column heads and the last column switch with the mode; first row highlighted; at most 6 rows.
- [x] Inputs wrap at narrow widths; the table scrolls horizontally inside its box.
