# CHECKLIST.md — acceptance contract for POE-329 Font EV Recipes

Tick a box ONLY when it's true in your implementation AND matches the reference PNG (dark only).
Unticked = not done.

## ⛔ MUST NOT DROP
- [ ] Section sits between the per-variant EV table and "Pool Overview"; neither of those changes.
- [ ] No buy button anywhere in the section.
- [ ] Cost line reads exactly "Exceptional Support Gem lv 3+", no sub-line.
- [ ] Loading: every price and the EV show "pricing…", never "0c" (`02`).
- [ ] Unknown price keeps its row ("no price"); unknown lens → EV "— needs the lens price" (`03`).
- [ ] Stale: yellow "updated 3h ago" badge in the footer (`04`).
- [ ] Negative EV in `--color-lab-red`, real minus sign, 1.25rem bold (`05`).
- [ ] Exceptional rows keep the order Enhance, Enlighten, Empower.
- [ ] Footer never wraps; source shrinks with an ellipsis.
- [ ] Tokens only; copy exactly as the references.

## Section shell (`01`)
- [ ] "Recipes" toggle built like "Pool Overview" (▼ / ▶), open by default, keyboard reachable, `aria-expanded`.
- [ ] Card grid `auto-fill, minmax(320px, 1fr)`, gap 12 px.
- [ ] Adding a third recipe needs only a new card (one shared card component).

## Exceptional support card (`01`, `02`, `03`, `04`)
- [ ] Title, option text, "Gives one at random", three rows, footer "poe.ninja · level 1, uncorrupted" + age; no Costs, no EV.

## Facetor's Lens card (`01`–`05`)
- [ ] Title "Facetor's Lens · 60%", option text, Gives row with "stored experience ≥ 999,627,000", Costs row, EV per craft = lens − support (412 − 95 = +317c; 60 − 95 = −35c), footer "trade · median of the 3 lowest" + age.
