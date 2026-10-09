# Desktop app → Strategies → Harvest Flipping

**Handoff 1 — 2026-10-10** · ticket POE-283

## What's in this package
- **Build source:** `Overview-prototype.dc.html`. It is not copied here (playbook `LOCAL.md`). The file is
  `design/canvas/desktop-app--strategies--harvest-flipping/project/Overview-prototype.dc.html`, rendered on the
  canvas https://claude.ai/artifact/PTogG15PdG1WJYjbDqNHJf, page "Handoff · build sources".
- **Provenance only, do not build from it:** every other board on that canvas (`Main.dc.html`, `Overview.dc.html`,
  `States.dc.html`, `TopBar.dc.html`, `NavIcon.dc.html`).
- `README.md` — the spec (this file).
- `CHECKLIST.md` — the acceptance contract.
- `fixtures.json` — the data.
- `reference_screens/` — ground truth, 9 PNGs (index: `reference_screens/INDEX.md`).
- `assets/harvest-flipping-icon.png` — the nav icon, 64×64 with a transparent background.
- `AGENT.md` — the implementer's protocol. Read it first.

Give the agent: _"Read `AGENT.md` and follow it exactly. Implement the frontend 1:1 against
`reference_screens/` using `fixtures.json`. Do not stop until every `CHECKLIST.md` box passes; log
anything you can't resolve in `NOTES.md`."_

## Spec changes vs ticket
Compared against: POE-283 (description as of 2026-10-10) + `design/tasks/poe-283--desktop-app--strategies--harvest-flipping.intake.md`.
- Layout: the ticket asked for two variants, (a) a family-first table and (b) an input-first calculator → **build the family overview**: a verdict, Feeders and Keepers as tier cards, two stash regexes, cost per reroll and the EV of the selection. Why: the owner wants to see the whole situation at once, know right away whether the gamble is worth doing, and pick out keepers and feeders easily. A calculator for one held item answered a question players don't ask.
- Keep-set override: per type → **per tier (a whole tier moves between Feeders and Keepers) or per single type**, defaulting to the computed keep set. Why: the owner wants to select several tiers as keepers at once.
- Lifeforce for essences: "Vivid for Essences" → **30 Primal (blue) per reroll, for Deafening and corrupted essences alike**. Why: the requester confirmed "the blue ones", and poedb agrees. The ticket carries a "Lifeforce colour correction" section.
- Corrupted essences (open in the ticket) → **their own tab, a closed pool of four (Hysteria, Insanity, Horror, Delirium), weights assumed uniform, no logged sample**. Why: they can only come from rerolling a corrupted essence (owner).
- Obscured Delirium Orb → **left out of the pool and the page**. Why: it is no longer in the game (owner).
- Essence tiers (open) → **Deafening only**. Why: owner; the logs cover Deafening only.
- Batch quantity (open) → **per item only, no stack-size input** (owner).
- Gating (open) → **behind the `beta` grant (`BETA_FEATURE`); no "beta" badge in the menu**. Why: the menu never shows badges (owner).
- Surface (open) → **desktop only**. Why: the web frontend is deprecated except Lab (owner).
- Override persistence (open) → **picks persist across sessions, per family** (owner).
- Where EV is computed (open) → **tiers on the server, cached so every client gets them warm; keep set, EV and regex on the desktop**. Why: tiers depend only on prices and are the same for everyone; the rest depends on the player's picks and must react instantly (owner, same split as ADR-022).
- New, not in the ticket: **stash-search regexes for feeders and keepers**, and **"1 div → N lifeforce → M rerolls"** under the cost per reroll. Why: the owner asked for both, since players buy lifeforce with divines and search their stash for feeders and keepers.
- Unpriced type (the ticket's "thin markets" constraint) → **always a keeper, never fed; counts 0c as an outcome so the EV is a floor; the summary says how much its price would move the EV**. Why: the page must never recommend rerolling an item it cannot price.
- Div vs chaos per keeper: the owner chose a "Sell in" mark per keeper row during design, but the final layout has no place for it. **Not built.** It is listed under Open decisions.

## Tokens
All colours come from `desktop/src/tokens.css`. No new tokens.
- Page: `--color-lab-*`, the same as `CurrencyExchangePage.svelte`.
- Chrome: `--bg`, `--surface`, `--border`, `--text`, `--text-muted`, `--accent`, `--success`, `--warning`.
- Derived fills are `color-mix()` of an existing token, never a new hex:
  - keeper chip: `--color-lab-green` at 10% fill and 40% border
  - forced (your pick) chip: dashed `--color-lab-yellow` border
  - "split" and "unpriced" marks: `--color-lab-yellow` at 45% border
- The segmented control's active fill (`rgba(99,102,241,.2)`) is `SegmentedButtons.svelte`'s own, so use the component.

Rendered values the prototype uses:
- Panels: 1px border, no radius.
- Chips and buttons: 3–4px radius.
- Tier name: 0.6875rem / 700, letter-spacing 0.06em.
- Verdict headline: 1.25rem / 700.
- Headline EV: 1.375rem mono.
- Section captions: 0.625rem uppercase, letter-spacing 0.06em.
- Numbers: `'Consolas','Monaco',monospace` / 600, the CX `.mono`.

## Screens
### Harvest Flipping page (route `/harvest-flipping`, sidebar → Strategies)
Top to bottom, inside the existing app shell:
1. **Page head**, as on CX: title "Harvest Flipping", the league label, and on the right "Prices" with
   `SegmentedButtons` [Recent 6h | Day 24h] (CX's `HORIZON_OPTIONS`).
2. **Family tabs**: Delirium Orbs · Deafening Essences · Corrupted Essences · Fossils · Astrolabes · Oils ·
   Catalysts. The last three carry a "no data" mark and open the no-data panel (reference 06). Tabs fit on
   one line at 1024px.
3. **Status line**, as on CX: "updated N min ago · prices: Currency Exchange, <horizon> · 1 div = 360c ·
   weights: <provenance>". Provenance per family is in `fixtures.json` → `families[].weights.sample`.
4. **Verdict panel**: "Is it worth it?" with one of three headlines: "Yes — feed the cheap tiers",
   "No — not at these prices" or "No — nothing to flip". Under it a one-line reason naming the cheapest
   feeder. On the right, the loop EV of the cheapest feeder, large, with "per <item>".
5. **Keep / feed split row**: "computed from prices" or "N type(s) moved by you" (amber), and a
   "Reset to computed" button that is dimmed and inert when nothing was moved.
6. **Two columns, Feeders | Keepers**. One card per tier present in that column, ordered TOP → FLOOR:
   - Card header: tier name, a "split" mark when the tier has types on both sides, "<n> types · <min>–<max>c",
     and a move button ("← Keep tier" on a feeder card, "Feed tier →" on a keeper card).
   - Card body: one chip per type, priced high to low, with the item icon, the short name and the price.
     Feeders also show their loop EV (green or red). Keepers show their weight share.
7. **Right column**:
   - **Stash search**: "Feeders regex" and "Keepers regex", each a read-only input, a Copy button
     (turns "Copied") and "<n> / 250 characters", amber above 250.
   - **Cost per reroll**: "0.9c = 30 × [icon] Wild lifeforce (purple)", then "1 div → 9,905 lifeforce → 330
     rerolls", then a caption naming both markets.
   - **EV of your selection**: cheapest feeder, loop EV per feeder, keeper hit per roll, rerolls per keeper,
     lifeforce per keeper, and after a rule: "1 div of lifeforce yields ~N keepers" and "…worth after inputs".

### Sidebar entry
"Harvest Flipping" under Strategies, after Currency Exchange, shown only when `hasFeature(BETA_FEATURE)`.
- Icon: `assets/harvest-flipping-icon.png` (Wild lifeforce with a green recycle badge filling the
  bottom-right quadrant), sized like `lab-icon.png` (18px expanded; collapsed rail uses the same image).
- No badge.
- **Nothing else in the sidebar or the top bar changes.** The divine rate in the top bar is POE-284,
  with its own package.

## Interactions & state
- **Tab** switches family. Each family keeps its own picks.
- **Chip click** moves that one type to the other column. It becomes a pick, with a dashed amber border
  and a title that says so.
- **Tier move button** moves every type of that tier to the other column.
- **Reset to computed** clears the active family's picks only.
- Everything recomputes immediately on the desktop: verdict, columns, regexes, EV and the split label.
- Picks persist across sessions (prefs map, ADR-013).
- A price refresh (`poe/currency-exchange/updated` over Mercure) refetches without a reload. Picks survive
  it. The computed default moves with the prices.
- **Copy** writes the regex to the clipboard, and the button reads "Copied" until the next change.
- **Horizon** switches the price window. Its persisted pref follows CX's pattern.
- States are on reference 08: loading, cold server, stale, error, unpriced type, not worth it, nothing to
  flip, regex over 250, and no data.

## Engine (desktop; built in this task — it stays when going live)
For one family with types `i`, chaos prices `p(i)`, weights `w(i)` (logged rolls, normalised; uniform
where `weights.sample.uniform`) and reroll cost `c = rerollCost × lifeforce.chaos`:
- `P(j | i) = w(j) / (1 − w(i))` for `j ≠ i`, and 0 for `j = i`.
- Value iteration to a fixed point, change below 1e-9, at most 400 passes. `R(i) = Σ_j P(j|i)·V(j) − c`.
  `keep(i) = pick(i) ?? (p(i) ≥ R(i))`. `V(i) = keep(i) ? p(i) : R(i)`.
- Loop EV of feeder `i` = `R(i) − p(i)`.
- Rerolls per keeper `N(i) = 1 + Σ_{j not kept} P(j|i)·N(j)`, solved iteratively.
- Keeper hit per roll = `Σ_{k kept} P(k|i)`.
- Lifeforce per keeper = `30 × N(i)` and `c × N(i)` chaos.
- 1 div of lifeforce yields `perDivine / 30 / N(i)` keepers, worth `that × loop EV`.
- The headline uses the **cheapest feeder** by price.
- An unpriced type counts `p = 0` as an outcome and is always kept.
- **Regex**: for each chosen type's full name, the shortest lowercase run of ≥3 letters (no spaces or
  apostrophes) that appears in no other full name of the same family. Join with `|` and wrap in double
  quotes. Count length including the quotes, against PoE 1's 250-character stash-search cap.
- Expected results on the fixture: `fixtures.json` → `derived`.

## Data shapes
See `fixtures.json`. Owners:
- Currency Exchange: prices, lifeforce, the divine rate.
- New Harvest server data: weights, reroll cost and colour, tiers (cached).
- Desktop prefs: picks.
- Desktop engine: everything under `derived`.

Item icons come from the existing icon path (`ItemIcon` + `iconSrc` in `$lib/exchange/view.ts`) keyed
by `itemId`, never from poewiki URLs.

## Assets
- `assets/harvest-flipping-icon.png` → `desktop/static/`.
- The sidebar's existing emoji and `lab-icon.png` stay as they are.
- No new fonts: system UI stack and Consolas/Monaco, as the app already uses.

## Glossary
- **Feeder**: a type you reroll (put in).
- **Keeper**: a type you stop on and sell.
- **Loop**: reroll a feeder, keep rerolling feeders until a keeper comes out.
- **Tier**: TOP, HIGH, MID-HIGH, MID, LOW or FLOOR, from the unified tier system on the family's prices.
- **Pick**: the player's override of the computed keep set.

## Constraints
- Dark only; English; item names exactly as in-game.
- WCAG AA contrast; every control keyboard-reachable (tabs, chips, move buttons, Copy, Reset).
- Visibility is the default (ADR-017/018): nothing with a price is hidden; a family that doesn't pay shows
  its negative numbers.
- Page only, no work toggle (ADR-014).
- Out of scope:
  - overlays, OCR and Client.txt
  - trade-site lookups and poe.ninja
  - families without weights, beyond the no-data panel
  - the top-bar rate (POE-284)
  - the server endpoint, the tier cache and weight storage: stub them at the seam, as `AGENT.md` §0 says

## Open decisions — questions, not requirements; do not invent answers
- **Div vs chaos per keeper**: the owner picked a per-keeper "Sell in" mark early on; the final layout has
  no place for it. Interim: not shown. Ask the owner where it goes (chip tooltip, chip mark, or the EV panel).
- **Price sides**: "follow the Currency Exchange convention" (owner), but which side prices an input, an
  outcome and lifeforce is not pinned. Interim: one chaos price per item (the fixture's).
- **Weight updates** (ticket Q11): merge or replace when a new HarvestForge log arrives, and whether to show
  a weights-last-updated date. Interim: the status line shows the log's date.
- **Regex over 250 characters**: the UI shows the amber count only. Whether to split it into two searches is
  undecided.
