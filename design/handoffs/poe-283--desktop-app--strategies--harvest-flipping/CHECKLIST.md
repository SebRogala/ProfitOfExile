# CHECKLIST.md — acceptance contract for Harvest Flipping (POE-283)

Tick a box ONLY when it's true in your implementation AND matches the reference PNG (dark theme, the
only one). Unticked = not done. Don't advance with open boxes on the current screen.

## ⛔ MUST NOT DROP — the edge cases that get silently skipped
- [ ] The **no-data families** (Astrolabes, Oils, Catalysts) stay visible as tabs with the "no data" mark, and open the "reroll weights not logged yet" panel (reference 06). They are never hidden.
- [ ] **Not worth it** and **nothing to flip** show **no** divine-scale line. **Not worth it**: when even the cheapest feeder loses, the verdict reads "No — not at these prices", the EV prints red with U+2212, and both columns stay listed (reference 08 §6).
- [ ] **Nothing to flip**: an empty keep set (or empty feed set) gives "No — nothing to flip", the reason line, and "No keepers: nothing to stop on." / "No feeders: every tier is kept." in place of the empty column (reference 08 §7). Never a crash, and never a blank column with no text.
- [ ] **Unpriced type**: it is always a keeper, its price shows "—" with an "unpriced" mark, it counts 0c as an outcome, and the EV panel says the EV is a floor and what it would read at the last seen price (reference 08 §5).
- [ ] **Split tier**: a tier with types on both sides appears in both columns with the "split" mark (reference 05).
- [ ] **Reset to computed** is dimmed and does nothing when there are no picks; it clears the active family only.
- [ ] **Picks persist** across an app restart, per family, and survive a price refresh.
- [ ] **Regex over 250 characters**: the count turns amber (reference 08 §8).
- [ ] **Loading, cold server, stale and error** states, as CX does them: the stale state keeps every number on screen (reference 08 §1–4).
- [ ] **Corrupted Essences** use the Primal colour and say "assumed uniform" in the provenance; there is no logged sample.
- [ ] **Essences cost Primal (blue)**, not Vivid.
- [ ] **Obscured Delirium Orb** is absent.
- [ ] **Fine Delirium Orb** is `CurrencyAfflictionOrbCurrency`. `items.json` names two other ids "Fine Delirium Orb", so never look an item up by name.
- [ ] The sidebar entry shows only on `BETA_FEATURE` devices, with **no badge**. Sidebar and top bar are otherwise byte-identical.
- [ ] WCAG AA contrast. Tabs, chips, tier buttons, Copy, Reset and the horizon control are all keyboard-reachable, with a visible focus ring.
- [ ] Tokens only: zero raw hex outside `tokens.css` (derived fills are `color-mix()` of a token).
- [ ] All copy matches the references exactly, including signs (U+2212), separators (" · ") and "→".

## Engine
- [ ] Unit tests reproduce `fixtures.json` → `derived` for every family. Fossils: cheapest feeder Lucent Fossil, +25.7c; keepers Hollow, Faceted, Sanctified, Gilded, Glyphic, Fractured, Shuddering.
- [ ] A pick overrides the computed decision for that type only; every other type is re-solved around it.
- [ ] The regex generator yields `"shu|san|gil|fac|fra|hol|gly"` (29 characters) for the default fossil keepers.

## Page — Fossils, default (reference 01)
- [ ] Page head, tabs on one line at 1024px, status line with the provenance.
- [ ] Verdict "Yes — feed the cheap tiers", reason naming Lucent Fossil (9.1c), headline "+25.7c per Lucent Fossil", divine line "1 div profit ≈ 15 feeders · ~378 rerolls · ~11,349 Wild".
- [ ] Feeders: MID and LOW cards; Keepers: TOP, HIGH and MID. MID is already **split** by default: Dense and Corroded are fed, the other five are kept. Every card has its tier name, range and move button.
- [ ] Every chip shows the item icon, short name and price; feeders add a signed EV, keepers add their weight share.
- [ ] Right column: both regexes with counts, cost per reroll "0.9c = 30 × [icon] Wild lifeforce (purple)", "1 div → 9,905 lifeforce → 330 rerolls", and the EV panel rows.

## Page — other families (references 02–04)
- [ ] Delirium Orbs: keepers Diviner's and Skittering.
- [ ] Deafening Essences (Primal, 1.5c): feed Torment +6.3c.
- [ ] Corrupted Essences: keep Horror.

## Page — interactions (references 05, 07)
- [ ] Clicking Dense moves it to Keepers: MID stays "split" with only Corroded left as a feeder, Dense shows the dashed amber pick border, "1 type moved by you" in amber, Reset enabled, verdict now Lucent Fossil +24.3c.
- [ ] Copy turns into "Copied".

## Sidebar (reference 09)
- [ ] "Harvest Flipping" after Currency Exchange, with `harvest-flipping-icon.png` at nav size, in active and inactive states.
