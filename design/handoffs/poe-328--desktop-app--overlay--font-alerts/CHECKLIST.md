# CHECKLIST.md — acceptance contract for POE-328 Font alerts

Tick a box ONLY when it's true in your implementation AND matches the reference PNG (dark only).
Unticked = not done.

## ⛔ MUST NOT DROP
- [ ] A missing price keeps its row with "no price"; the other prices still show (`03`).
- [ ] Loading shows "pricing…" in every price slot, never "0c" or a blank (`04`).
- [ ] Stale price: the age turns into a yellow tinted badge (`05`).
- [ ] Two alert crafts in one panel read → both panels, stacked, 6 px apart (`01`).
- [ ] Transfigure alone → only its panel; no empty exceptional panel, no gap (`06`).
- [ ] No alert craft in the read → nothing rendered, no empty frame.
- [ ] The stack never overlaps `placements.lab.font` at its default position (1080p and 1920×1200).
- [ ] Rows keep the order Enhance, Enlighten, Empower whatever the prices.
- [ ] Display-only: no hot rects; clicks over the stack reach the game.
- [ ] Tokens only through `tokens.css` (`overlay-tokens.test.ts` passes); copy exactly as the references.

## Widget
- [ ] Registered in `widget-registry.ts` (module `lab`); Settings → Overlay Positions shows its Show + Configure row.
- [ ] Default position = right of the font region, top-aligned, 12 px·scale gap; (976, 220) at 1920×1080.
- [ ] A Configure placement overrides the default and persists.
- [ ] Hides when the font session ends or a read no longer lists the craft.

## Exceptional support panel (`02`)
- [ ] Title, "Font option", "Gives one of these at random", three rows with GemIcon, name, amber mono price, footer "poe.ninja · level 1, uncorrupted" + age.
- [ ] Prices through `formatPrice` (chaos below 2 div).

## Transfigure panel (`06`)
- [ ] Title "Transfigure gem", "Font option", option text, "You choose the version"; nothing below.
