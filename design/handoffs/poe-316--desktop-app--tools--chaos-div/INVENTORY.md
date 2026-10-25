# POE-316 implementation inventory

Status: source implementation complete; checked items record implementation/source coverage, not runtime acceptance. Runtime gates belong to the orchestrator (NOTES.md).

One screen: Tools → Chaos ⇄ Div, mounted in the real desktop shell.

- [x] Release and dev navigation, expanded sidebar and collapsed rail; ungated.
- [x] Opening and Reset: Selling / Chaos / amount 1 / ratio 1 / fixed 1 right.
- [x] Header: exact title and Reset action with reset icon.
- [x] Mode: Selling and Buying; Currency: Chaos and Divine, using SegmentedButtons.
- [x] Amount: live numeric input, minimum 1, − / + with accessible names.
- [x] Ratio: live numeric input, fixed 1 either side, keyboard-accessible swap icon.
- [x] Labels: Amount at ratio 1; Items I have / Items I want otherwise.
- [x] Per-item formats: whole chaos from 10c, two decimals below; divine two/four decimals.
- [x] Totals: Total / Worth / Cost, grouped whole chaos, two-decimal divine, whole split.
- [x] Copy totals: bare values, successful Copied feedback for 1.2 seconds.
- [x] Selling table: Nearest undercuts / Undercut / Left over.
- [x] Buying table: Nearest outbids / Outbid / Short of target.
- [x] Table: Ratio, I want, I have; independent Copy actions on both order sides.
- [x] Table: first row tinted, at most six rows, displayed ratios differ from input.
- [x] Empty table hidden: ratio 1, no candidate, or invalid ratio.
- [x] Invalid/empty numeric input: no NaN or Infinity in output.
- [x] Cold rate: rate-dependent values unknown, corresponding total Copy disabled.
- [x] Cold Chaos currency: chaos figures and order rows remain usable.
- [x] Cold Divine currency: order rows remain usable independently of conversion rate.
- [x] Ready/stale rate: read existing store reactively, no fetch or extra rate line.
- [x] Copy failure: no false Copied feedback; warning logged.
- [x] Narrow layout: inputs wrap; table scrolls horizontally inside its surface.
- [x] Tokens, exact English copy, accessible labels and keyboard focus styles.
- [x] All six fixture vectors and calculation boundary tests pass.
- [x] Focused tests, full desktop tests, svelte-check pass without new touched-file diagnostics.
- [ ] Runtime rendering, screenshots, contrast/keyboard interaction and adversarial audit (orchestrator; see NOTES.md).

No reference PNGs were supplied. Main.dc.html is the visual reference; its demo shell is excluded.
