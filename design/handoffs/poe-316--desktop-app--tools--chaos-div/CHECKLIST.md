# CHECKLIST.md — acceptance contract for Tools → Chaos ⇄ Div (POE-316)

## ⛔ MUST NOT DROP
- [ ] Every `fixtures.json` → `vectors` case passes as a unit test against the pure calculation module.
- [ ] Ratio 1 hides the undercut table and labels the amount field "Amount".
- [ ] The "1" swap flips the side and the maths (`1 : 6.12` selling ≠ `6.12 : 1` selling).
- [ ] Buying produces **outbids** (price up), selling produces **undercuts** (price down).
- [ ] Rows only show ratios whose displayed value differs from the typed ratio's.
- [ ] Copy on chaos total, divine total, and on **both** I want and I have of every row; Copy puts the bare number.
- [ ] Reset restores Selling, Chaos, 1, ratio 1 with the 1 on the right.
- [ ] The rate comes from the existing divine-rate store; no rate line on the page; no new fetch.
- [ ] No rate yet: rate-dependent figures show "—", never "0", "NaN" or "Infinity".
- [ ] Only one new sidebar entry; `TopBar.svelte` untouched. Tokens only; copy exact (including "⇄", "−").

## Page
- [ ] Header with Reset on the right.
- [ ] Input bar: Mode, Currency, Amount stepper (min 1), Ratio with the fixed 1, the swap button and the per-item line.
- [ ] Totals cards: lead word Total / Worth / Cost; whole split line.
- [ ] Table title, column heads and the last column switch with the mode; first row highlighted; at most 6 rows.
- [ ] Inputs wrap at narrow widths; the table scrolls horizontally inside its box.
