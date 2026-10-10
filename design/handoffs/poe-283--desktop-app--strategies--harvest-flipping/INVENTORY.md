# INVENTORY.md — Harvest Flipping (POE-283): screen × state × affordance

The completeness contract of `AGENT.md` §2. "Done" is measured against this list. Tick each box
in lockstep with the build; the adversarial completeness pass (`AGENT.md` §4 step 5) audits the
delivery against it and every reference PNG.

- **Source** names the reference PNG (`reference_screens/desktop/`) and the section the row comes
  from: `CL` = `CHECKLIST.md`, `RM` = `README.md`.
- **WI** names the work item(s) of plan `2026-10-10-01-poe-283-harvest-flipping` that build the
  row, taken from the plan's "CHECKLIST box → WI" and "Reference PNG → WI" tables. Rows that are
  in neither table name the WI by the same plan's work-item scope and are marked *(by scope)*.
- Pixel pass waived by the Operator 2026-10-10: rows are verified from code, fixtures and
  component tests against the references, not from a rendered diff.
- Ticked by WI-11 (2026-10-10) after tracing each row to the code that draws it and the test that
  asserts it; the trace and its residual questions are in the lane's WI-11 return and `NOTES.md`
  ("Designer items"). The adversarial completeness pass (`AGENT.md` §4 step 5) round 1
  (`.claude/pipeforge/fleet/completeness/findings-1.md`, C1–C18) was built in WI-12; rows it
  touched carry a *WI-12 (…)* note naming the finding.

## A. CHECKLIST boxes (one row per box — 31)

### MUST NOT DROP

- [x] A1 — No-data families (Astrolabes, Oils, Catalysts) stay visible as tabs with the "no data" mark and open the "reroll weights not logged yet" panel; never hidden. · ref 06, 08 §9 · CL MUST NOT DROP · **WI-5, WI-6, WI-8, WI-11**
- [x] A2 — Not worth it / nothing to flip show no divine-scale line; not worth it reads "No — not at these prices", EV red with U+2212, both columns listed. · ref 08 §6 · CL MUST NOT DROP · **WI-1, WI-6, WI-11**
- [x] A3 — Nothing to flip: "No — nothing to flip", the reason line, "No keepers: nothing to stop on." / "No feeders: every tier is kept." in place of the empty column; no crash, no blank column. · ref 08 §7 · CL MUST NOT DROP · **WI-1, WI-6, WI-11**
- [x] A4 — Unpriced type: always a keeper, price "—" with an "unpriced" mark, counts 0c as an outcome, EV panel says the EV is a floor and what it reads at the last seen price. · ref 08 §5 · CL MUST NOT DROP · **WI-1, WI-5, WI-6, WI-7** · *WI-12 (C14/C15: floor note under "Loop EV per feeder"; no move on an all-unpriced tier)*
- [x] A5 — Split tier: a tier with types on both sides appears in both columns with the "split" mark. · ref 05 (and 01, MID) · CL MUST NOT DROP · **WI-6, WI-7, WI-11**
- [x] A6 — Reset to computed is dimmed and inert with no picks; it clears the active family only. · ref 01 (dimmed), 05 (enabled) · CL MUST NOT DROP · **WI-6, WI-8 (controller test)**
- [x] A7 — Picks persist across an app restart, per family, and survive a price refresh. · RM § Interactions & state · CL MUST NOT DROP · **WI-6, WI-8 (controller tests: restored prefs, refetch, failed refresh)**
- [x] A8 — Regex over 250 characters: the count turns amber. · ref 08 §8 · CL MUST NOT DROP · **WI-2, WI-6, WI-7**
- [x] A9 — Loading, cold server, stale and error states as CX does them; stale keeps every number on screen. · ref 08 §1–4 · CL MUST NOT DROP · **WI-6, WI-8, WI-11**
- [x] A10 — Corrupted Essences use the Primal colour and say "assumed uniform" in the provenance; no logged sample. · ref 04 · CL MUST NOT DROP · **WI-5, WI-6**
- [x] A11 — Essences cost Primal (blue), not Vivid. · ref 03, 04 · CL MUST NOT DROP · **WI-5, WI-1**
- [x] A12 — Obscured Delirium Orb is absent. · ref 02 · CL MUST NOT DROP · **WI-5**
- [x] A13 — Fine Delirium Orb is `CurrencyAfflictionOrbCurrency`; never look an item up by name. · ref 02 · CL MUST NOT DROP · **WI-5**
- [x] A14 — Sidebar entry only on `BETA_FEATURE` devices, no badge; sidebar and top bar otherwise byte-identical. · ref 09 · CL MUST NOT DROP · **WI-10**
- [x] A15 — WCAG AA contrast; tabs, chips, tier buttons, Copy, Reset and the horizon control keyboard-reachable with a visible focus ring. · RM § Constraints · CL MUST NOT DROP · **WI-7, WI-8, WI-11**
- [x] A16 — Tokens only: zero raw hex outside `tokens.css`; derived fills are `color-mix()` of a token. · RM § Tokens · CL MUST NOT DROP · **WI-7, WI-8, WI-11**
- [x] A17 — All copy matches the references exactly, incl. U+2212, " · " and "→". · ref 01–09 · CL MUST NOT DROP · **WI-6, WI-11** · *WI-12 (C11/C13: U+2019 "player’s"; refs 01–04 values pinned in view.test `reference values (C13)`)*

### Engine

- [x] A18 — Unit tests reproduce `fixtures.json` → `derived` for every family (Fossils: Lucent Fossil +25.7c; keepers Hollow, Faceted, Sanctified, Gilded, Glyphic, Fractured, Shuddering). · RM § Engine · CL Engine · **WI-1**
- [x] A19 — A pick overrides one type; every other type is re-solved around it. · ref 05 · CL Engine · **WI-1**
- [x] A20 — Regex generator yields `"shu|san|gil|fac|fra|hol|gly"` (29) for the default fossil keepers. · ref 01 · CL Engine · **WI-2**

### Page — Fossils, default (reference 01)

- [x] A21 — Page head, tabs on one line at 1024 px, status line with the provenance. · ref 01 · CL Page — Fossils · **WI-6, WI-8, WI-11** · *nowrap + horizontal scroll; fit at 1024 px not verified without a render (pixel pass waived by the Operator 2026-10-10)*
- [x] A22 — Verdict "Yes — feed the cheap tiers", reason naming Lucent Fossil (9.1c), "+25.7c per Lucent Fossil", divine line "1 div profit ≈ 15 feeders · ~378 rerolls · ~11,349 Wild". · ref 01 · CL Page — Fossils · **WI-1, WI-6, WI-11**
- [x] A23 — Feeders MID + LOW; Keepers TOP, HIGH, MID; MID split by default (Dense, Corroded fed; five kept); every card has tier name, range and move button. · ref 01 · CL Page — Fossils · **WI-6, WI-7, WI-11**
- [x] A24 — Every chip: item icon, short name, price; feeders add signed EV, keepers add weight share. · ref 01 · CL Page — Fossils · **WI-6, WI-7, WI-11**
- [x] A25 — Right column: both regexes with counts, "0.9c = 30 × [icon] Wild lifeforce (purple)", "1 div → 9,905 lifeforce → 330 rerolls", EV panel rows. · ref 01 · CL Page — Fossils · **WI-2, WI-6, WI-7, WI-11** · *WI-12 (C9/C10: only "1 div" and the numbers in mono; caption to three decimals)*

### Page — other families (references 02–04)

- [x] A26 — Delirium Orbs: keepers Diviner's and Skittering. · ref 02 · CL Page — other · **WI-1, WI-11**
- [x] A27 — Deafening Essences (Primal, 1.5c): feed Torment +6.3c. · ref 03 · CL Page — other · **WI-1, WI-11**
- [x] A28 — Corrupted Essences: keep Horror. · ref 04 · CL Page — other · **WI-1, WI-11**

### Page — interactions (references 05, 07)

- [x] A29 — Clicking Dense moves it to Keepers: MID stays split with only Corroded fed, Dense dashed amber, "1 type moved by you" amber, Reset enabled, Lucent Fossil +24.3c. · ref 05 · CL Interactions · **WI-1, WI-6, WI-7, WI-8 (controller actions test), WI-11** · *WI-12 (C1: counts changed picks only; chip click always moves)*
- [x] A30 — Copy turns into "Copied". · ref 07 · CL Interactions · **WI-7, WI-8 (controller Copy test), WI-11**

### Sidebar (reference 09)

- [x] A31 — "Harvest Flipping" after Currency Exchange, `harvest-flipping-icon.png` at nav size, active and inactive. · ref 09 · CL Sidebar · **WI-4, WI-10, WI-11**

## B. Screens × states (one row per reference-PNG state)

### Harvest Flipping page — populated families

- [x] B1 — Fossils, default: full page in the real shell. · ref 01 · RM § Screens 1–7 · **WI-1, WI-2, WI-6, WI-7, WI-8, WI-11**
- [x] B2 — Delirium Orbs tab: keepers Diviner's + Skittering, Primal cost, "+22.4c per Jeweller's Delirium Orb". · ref 02 · RM § Screens · **WI-1, WI-2, WI-11**
- [x] B3 — Deafening Essences tab: Primal 1.5c, feed Torment +6.3c, six keepers, provenance "uniform ~5%". · ref 03 · RM § Screens · **WI-1, WI-11**
- [x] B4 — Corrupted Essences tab: closed pool of four, "assumed uniform, closed pool of four" provenance, MID-HIGH tier name. · ref 04 · RM § Screens · **WI-1, WI-2, WI-6, WI-11**
- [x] B5 — Fossils split tier after Dense clicked (pick state). · ref 05 · RM § Interactions & state · **WI-1, WI-6, WI-7, WI-8, WI-11**
- [x] B6 — Astrolabes tab, no data: no-data panel, status line "weights: no log yet", tab still listed. · ref 06 · RM § Screens 2 · **WI-6, WI-8, WI-11** · *WI-12 (C2/C7: page test `family without weights`; panel capped at 620 px)*
- [x] B7 — Fossils, Copy pressed: button reads "Copied". · ref 07 · RM § Interactions & state · **WI-7, WI-8, WI-11**

### States sheet (reference 08)

- [x] B8 — §1 Loading — first fetch: "Loading…" + skeleton; tabs and weights render at once; verdict, tiers, regex and EV wait for prices. · ref 08 §1 · **WI-6, WI-8, WI-11** (08 row: WI-1, WI-2, WI-6, WI-7, WI-8, WI-11)
- [x] B9 — §2 Cold server: "Waiting for the first Currency Exchange hour…", verdict panel "No prices yet" + "The server has not read an hour of the exchange. This fills in on its own." · ref 08 §2 · **WI-6, WI-8, WI-11** · *WI-12 (C2: page test `cold server (reference 08 §2)`)*
- [x] B10 — §3 Stale: "stale since HH:MM — server unreachable" (amber), "prices from HH:MM (N h ago)", rest of the status line; every number stays, nothing dimmed; next Mercure update clears the line. · ref 08 §3 · **WI-6, WI-8, WI-11** · *WI-12 (C2: page test `stale (reference 08 §3)`)*
- [x] B11 — §4 Error: "Couldn't reach the server" (amber), never reached the server. · ref 08 §4 · **WI-6, WI-8, WI-11** · *WI-12 (C2: page test `never reached the server (reference 08 §4)`)*
- [x] B12 — §5 Unpriced type: chip "—" + UNPRICED mark inside its tier ("1 type · —"); EV panel floor note "N unpriced type counted as 0c, so this EV is a floor. At its last seen Xc it would read +Yc." · ref 08 §5 · **WI-1, WI-5, WI-6, WI-7** · *WI-12 (C2/C15: page test floor note directly under its row)*
- [x] B13 — §6 Not worth it: "No — not at these prices", "Even the cheapest feeder sells for more than the loop returns.", red negative EV with U+2212 and "per <item>" (the PNG illustrates −0.3c; the plan's WI-1/WI-6 test input asserts the negative sign, not that literal), no divine line, columns still listed. · ref 08 §6 · **WI-1, WI-6, WI-11**
- [x] B14 — §7 Nothing to flip: "No — nothing to flip", "The keep set is empty: pick a tier to keep.", column text "No keepers: nothing to stop on." (and the feed-set twin "No feeders: every tier is kept."). · ref 08 §7 · **WI-1, WI-6, WI-11** · *WI-12 (C2/C3 + Q2 superseded: closest-type reason on an empty feed set, "—" headline over "no feeder", EV panel kept with "—")*
- [x] B15 — §8 Regex over 250, Copy pressed: "263 / 250 characters: too long for one stash search" in amber, button "Copied". · ref 08 §8 · **WI-2, WI-6, WI-7**
- [x] B16 — §9 No data (Astrolabes, Oils, Catalysts): "<Family>: reroll weights not logged yet" panel with the HarvestForge-log body text. · ref 08 §9 · **WI-6, WI-8, WI-11** · *WI-12 (C2/C7: page test, max-width 620 px)*
- [x] B17 — Oils tab, no data (same panel, family name "Oils"). · ref 08 §9 (not drawn on its own) · **WI-6, WI-8, WI-11**
- [x] B18 — Catalysts tab, no data (same panel, family name "Catalysts"). · ref 08 §9 (not drawn on its own) · **WI-6, WI-8, WI-11**

### Sidebar entry (reference 09)

- [x] B19 — Inactive entry: Wild icon at nav size + "Harvest Flipping". · ref 09 · RM § Sidebar entry · **WI-4, WI-10, WI-11**
- [x] B20 — Active entry (selected fill, accent text). · ref 09, 01 · RM § Sidebar entry · **WI-4, WI-10, WI-11**
- [x] B21 — Collapsed rail uses the same image. · RM § Sidebar entry · **WI-4, WI-10, WI-11** *(by scope: D6 both Sidebar densities)*
- [x] B22 — Entry absent on a device without `BETA_FEATURE`. · RM § Sidebar entry · **WI-10, WI-11** · *WI-12 (C18: no Sidebar test; follow-up in NOTES)*

## C. Affordances (each with its keyboard focus ring)

- [x] C1 — Family tab switches family; each family keeps its own picks; active tab persisted (`harvestFlippingFamily`). · ref 01–06 · RM § Interactions & state · **WI-6, WI-8, WI-11** *(by scope: D10 prefs)*
- [x] C2 — Family tab focus ring (keyboard). · RM § Constraints · **WI-7, WI-8, WI-11**
- [x] C3 — Chip click moves one type to the other column as a pick: dashed amber border + a title saying so. · ref 05 · RM § Interactions & state · **WI-1, WI-6, WI-7, WI-8, WI-11** · *WI-12 (C1/C5/Q4: drawn titles "Kept (engine) — click to feed just <name>" / "(your pick)" on every priced chip)*
- [x] C4 — Chip click skips an unpriced type (C2 ruling: unpriced wins over a pick). · ref 08 §5 · **WI-6, WI-8** *(by scope: D4 C2)*
- [x] C5 — Chip focus ring (keyboard). · RM § Constraints · **WI-7, WI-8, WI-11**
- [x] C6 — Tier move "← Keep tier" on a feeder card moves the whole tier to Keepers. · ref 01 · RM § Screens 6 · **WI-6, WI-7, WI-8, WI-11** · *WI-12 (C1: moved count = picks that differ from the engine)*
- [x] C7 — Tier move "Feed tier →" on a keeper card moves the whole tier to Feeders (unpriced members skipped). · ref 01 · RM § Screens 6 · **WI-6, WI-7, WI-8, WI-11** · *WI-12 (C14: no move on a card with no priced chip)*
- [x] C8 — Tier move button focus ring (both directions). · RM § Constraints · **WI-7, WI-8, WI-11**
- [x] C9 — Reset to computed, enabled (picks present): clears the active family's picks only. · ref 05 · RM § Screens 5 · **WI-6, WI-8 (controller test)**
- [x] C10 — Reset to computed, inert (no picks): dimmed, click does nothing. · ref 01 · RM § Screens 5 · **WI-6, WI-8 (controller test)**
- [x] C11 — Reset focus ring. · RM § Constraints · **WI-7, WI-8, WI-11**
- [x] C12 — Copy (feeders regex) writes to the clipboard → "Copied" until the next change. · ref 07 · RM § Interactions & state · **WI-7, WI-8 (controller Copy test), WI-11**
- [x] C13 — Copy (keepers regex) writes to the clipboard → "Copied" until the next change. · ref 07 · RM § Interactions & state · **WI-7, WI-8 (controller Copy test), WI-11**
- [x] C14 — Copy focus ring (both buttons). · RM § Constraints · **WI-7, WI-8, WI-11**
- [x] C15 — Horizon switch [Recent 6h | Day 24h] via `SegmentedButtons`: refetches; pref `harvestFlippingHorizon`, default Day 24h. · ref 01 · RM § Interactions & state · **WI-6, WI-8, WI-11** *(by scope: D4 C3, D10)*
- [x] C16 — Horizon control focus ring. · RM § Constraints · **WI-7, WI-8, WI-11**
- [x] C17 — Mercure price refresh (`poe/currency-exchange/updated`) refetches without a reload; picks survive; computed default moves with prices. · RM § Interactions & state · **WI-8 (controller tests)** *(by scope)*
- [x] C18 — Every pick, tier move and reset recomputes immediately: verdict, columns, regexes, EV, split label. · ref 05 · RM § Interactions & state · **WI-1, WI-6, WI-8** · *WI-12 (C1: split label counts changes against the engine)*
- [ ] C19 — Sidebar entry navigates to `/harvest-flipping`; entry focus ring. · ref 09 · RM § Sidebar entry · **WI-10, WI-11** · *pre-existing app-wide gap: no sidebar item has a focus ring (Sidebar.svelte all: unset, no :focus-visible)*

## D. Supplemental page elements (README § Screens 6–7, not a CHECKLIST box of their own)

- [x] D1 — Both regex fields (Feeders regex, Keepers regex) are read-only inputs displaying the generated text. · ref 01–05 · RM § Screens 7 · **WI-7, WI-11** *(by scope: HarvestRegexBox)*
- [x] D2 — Stash caption: "Paste into the stash search. Shortest fragment unique within this family; follows your tier picks." · ref 01–05 · RM § Screens 7 · **WI-6, WI-8, WI-11** *(by scope)*
- [x] D3 — Cost-per-reroll caption names both markets; Fossils reads "chaos side 0.031c each; divine side from the lifeforce/divine market". · ref 01–05 · RM § Screens 7 · **WI-6, WI-8, WI-11** *(by scope)*
- [x] D4 — EV panel rows: cheapest feeder with its price, loop EV per feeder, keeper hit per roll, rerolls per keeper, lifeforce per keeper in units and chaos; after a rule, "1 div of lifeforce yields" and "…worth after inputs". · ref 01–05 · RM § Screens 7 · **WI-1, WI-6, WI-8, WI-11** *(by scope)* · *WI-12 (C3: kept with "—" values when nothing flips)*
- [x] D5 — Cards ordered TOP, then the data's tier-name order toward FLOOR, absent tiers omitted; chips within a card sorted by price high to low. · ref 01–05 · RM § Screens 6 · **WI-6, WI-7, WI-11** *(by scope: D11, WI-6 step 7)*
