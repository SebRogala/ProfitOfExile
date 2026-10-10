# NOTES.md — Harvest Flipping (POE-283): implementer notes

The implementer's notes, required by `AGENT.md` §0b, §6 and §7. They cover the seam contract for the backend task, fixture-to-codebase names, ticket updates for a human, open decisions at their interims, rulings, designer items and follow-ups outside this lane.

Sources: the lane plan `2026-10-10-01-poe-283-harvest-flipping` (Decisions D1–D13), the lane run record (CONFIRM rulings, SUPERVISOR rulings, dated), the homes audit (High 1–4, Medium 5–15), and the committed code (`desktop/src/lib/harvest/`).

Pixel pass waived by the Operator 2026-10-10. In WI-11, layout, states and copy will be verified from code, fixtures and component tests against references 01–09, with no rendered diff.

## Seam contract

The backend task must honour this shape. Going live means swapping the data source only (`AGENT.md` §0, §6). The source is `desktop/src/lib/harvest/seam.ts` as committed in 70c1cbd7. One port and one fixture adapter exist per data owner (`fixtures.json` → `_owners`). Picks are desktop prefs (`persisted()`, ADR-013), not a port.

### Owner: Currency Exchange — `LoadExchangePrices = (horizon: CurrencyExchangeHorizon) => Promise<ExchangePriceRead>`

`ExchangePriceRead`:
| Field | Type | Meaning |
|---|---|---|
| `league` | `string` | League name. |
| `horizon` | `CurrencyExchangeHorizon` (`'recent' \| 'day'`, re-exported by `exchange/view.ts`) | The price window the read answers. |
| `lastUpdated` | `string \| null` | ISO time of the newest price; `null` before the first exchange hour. |
| `priceHour` | `string` | ISO hour the prices were read from. The tiers built on them carry the same hour. |
| `warm` | `boolean` | `false` while the server has not read an exchange hour (the cold state, ref 08 §2). |
| `divineChaosRate` | `number` | Divine:chaos rate. |
| `prices` | `Record<itemId, ExchangeItemPrice>` | Every item of every family, keyed by itemId and read with `Object.hasOwn`. Not derived from CX plays, which are a filtered set (audit Medium 5). |
| `lifeforce` | `Record<'Primal' \| 'Vivid' \| 'Wild', LifeforcePrice>` | Every lifeforce colour. |

`ExchangeItemPrice` = `{ chaos: number | null; divine: number | null; lastSeenChaos: number | null }`. `chaos: null` marks an unpriced type. `lastSeenChaos` is the latest earlier chaos price, read only for an unpriced type (C1).

`LifeforcePrice` = `{ itemId: string; colour: string; chaos: number; perDivine: number }`. `colour` is the in-game colour name the page prints ("purple", "blue", "yellow"). `perDivine` is the lifeforce bought per 1 divine on the lifeforce/divine market.

### Owner: Harvest server data — `LoadHarvestFamilies = (horizon: CurrencyExchangeHorizon) => Promise<HarvestFamilyData>`

`HarvestFamilyData` = `{ warm: boolean; families: HarvestFamily[] }`, with families in display order (the tab order).

`HarvestFamily`:
| Field | Type | Meaning |
|---|---|---|
| `id` | `string` | Family id (pref key; `delirium`, `essence`, `corrupt`, `fossil`, `astrolabe`, `oil`, `catalyst`). |
| `label` | `string` | Tab label. |
| `lifeforce` | `'Primal' \| 'Vivid' \| 'Wild' \| null` | The reroll colour; `null` for a family without logged weights. |
| `rerollCost` | `number \| null` | Lifeforce per reroll (30 for every logged family); `null` without weights. |
| `weights` | `HarvestWeights \| null` | `null` when no HarvestForge log exists yet. The tab then shows "no data" and is never hidden. |
| `tiers` | `HarvestTiers \| null` | `null` without weights. |
| `note` | `string \| null` | Free text from the source (the fixture's `_note`). |

`HarvestWeights` = `{ sample: { rolls: number | null; lifeforceSpent: number | null; date: string | null; source: string; uniform?: boolean }; types: { itemId: string; name: string; shortName: string; loggedRolls: number | null }[] }`. `uniform: true` means every type weighs the same and `loggedRolls` is ignored. `rolls: null` means no logged sample.

`HarvestTiers` = `{ boundariesChaos: number[]; names: string[]; topBoundaryChaos: number | null; byItem: Record<itemId, string>; priceHour: string }`. `names` omits TOP; TOP is present when `byItem` maps an item to `"TOP"`. `topBoundaryChaos` is `null` for Delirium Orbs and Corrupted Essences in the fixture.

### Requirements on the live source

- **Tiers per horizon, stamped with `priceHour`** (audit Medium 7). Tiers are derived from prices, so the server must serve them per horizon and carry the `priceHour` they were computed on. Otherwise Recent 6h chips render under Day 24h tiers, and a card's `<min>–<max>c` contradicts its chips. A Mercure tick could also refresh prices and tiers at different times.
- **The mock echoes the horizon** (WI-5 doubt 1, accepted in review). Both fixture adapters return the same data for both horizons, and the exchange adapter echoes the requested horizon in `horizon`. The live source must actually key prices and tiers by horizon + `priceHour`.
- **Adapter choices the live source must reproduce or replace deliberately** (WI-5 doubt 2, accepted in review). The exchange fixture has no `warm` field, so the adapter sets `warm: true`; the Harvest adapter also sets `warm: true`. No-data families (astrolabe, oil, catalyst) omit `lifeforce` and `rerollCost` in the fixture, so the adapter returns `null` for both and carries `_note` as `note`.
- **Both ports are async** (audit Medium 8). A synchronous source would never render `loading`.
- **Two owners, two statuses** (audit Medium 6, D8). The page combines them through `deriveState`, whose input is widened to `{warm; lastUpdated}`: any owner errored → stale or unreachable; any owner cold → warming.

## Fixture → codebase names

| `fixtures.json` | Codebase |
|---|---|
| `exchange` (block) | `ExchangePriceRead` (`harvest/seam.ts`), served by `harvest/sources/exchange-fixture.ts` from `harvest/__fixtures__/exchange.json` (D1) |
| `exchange.updatedMinutesAgo` | `ExchangePriceRead.lastUpdated` = now − N minutes, an ISO string (audit Medium 5). Reuses CX's age formatting. |
| `exchange.horizon` | `ExchangePriceRead.horizon` (echoes the request under the mock) |
| `exchange.priceHour` | `ExchangePriceRead.priceHour`, and `HarvestTiers.priceHour` on every logged family |
| `exchange.divineChaosRate` | `ExchangePriceRead.divineChaosRate` |
| `exchange.lifeforce.<Colour>` | `ExchangePriceRead.lifeforce.<Colour>` (`LifeforcePrice`) |
| `families[].prices` | `ExchangePriceRead.prices`: every family's `prices` map merged into one map keyed by itemId (owner: Currency Exchange, D1) |
| `families[].prices[id].chaos` / `.divine` | `ExchangeItemPrice.chaos` / `.divine`; `lastSeenChaos: null` is added (C1 widening) |
| `families[]` minus `prices` | `HarvestFamilyData.families` (`HarvestFamily`), served by `harvest/sources/harvest-fixture.ts` from `harvest/__fixtures__/harvest.json` |
| `families[]._note` | `HarvestFamily.note` |
| `families[].weights.types[].loggedRolls` | `FamilyType.weight` in `harvest/engine.ts` (`familyTypes`): `loggedRolls`, or 1 each when `sample.uniform` |
| `derived` | `harvest/__fixtures__/derived.json`, imported by tests only. Its values are what `solveFamily` and `regex.ts` compute (D1). |
| `picks` | The `persisted()` pref `harvestFlippingPicks`, JSON `{family: {itemId: 'keep' \| 'reroll'}}`, parsed by `parsePicks` (`harvest/view.ts`) (D10) |
| `horizon` default | Pref `harvestFlippingHorizon`, default `'day'`, parsed by `parseHarvestHorizon` (`harvest/view.ts`) (C3, D10) |
| `activeFamily` | Pref `harvestFlippingFamily`, default `'fossil'` (D10) |
| `_formats` | `formatHarvestChaos`, `formatHarvestGain`, `formatHarvestPercent`, `formatHarvestRolls`, `formatHarvestCount` (`harvest/view.ts`) (D9) |
| tier labels | Data strings from `tiers.names` + `"TOP"`. Not `api.ts` `PriceTier`, which lacks MID-HIGH and FLOOR (D11, audit Medium 15). |

## Ticket updates needed

Write these into POE-283. Without tracker access, a human does it (`AGENT.md` §0b).

### README § Spec changes vs ticket (decided; they supersede the ticket)

1. Layout: build the family overview (verdict, Feeders/Keepers tier cards, two stash regexes, cost per reroll, EV of the selection), not the ticket's two variants (family-first table / input-first calculator).
2. Keep-set override: per tier or per single type, defaulting to the computed keep set (ticket: per type).
3. Lifeforce for essences: 30 Primal (blue) per reroll for Deafening and corrupted essences alike (ticket: Vivid for Essences).
4. Corrupted essences: their own tab, a closed pool of four (Hysteria, Insanity, Horror, Delirium), weights assumed uniform, no logged sample.
5. Obscured Delirium Orb: left out of the pool and the page.
6. Essence tiers: Deafening only.
7. Batch quantity: per item only, no stack-size input.
8. Gating: behind the `beta` grant (`BETA_FEATURE`); no "beta" badge in the menu.
9. Surface: desktop only.
10. Override persistence: picks persist across sessions, per family.
11. Where EV is computed: tiers on the server, cached; keep set, EV and regex on the desktop.
12. New: stash-search regexes for feeders and keepers, and "1 div → N lifeforce → M rerolls" under the cost per reroll.
13. Unpriced type: always a keeper, never fed; counts 0c as an outcome, so the EV is a floor; the summary says how much its price would move the EV.
14. Div vs chaos: a divine-scale line in the verdict panel ("1 div profit ≈ 15 feeders · ~378 rerolls · ~11,349 Wild"), not a per-keeper row.

### Found while building

1. **C1 seam widening.** The CX read model needs `chaos: number | null` plus `lastSeenChaos: number | null` per item. Reference 08 §5 draws "last seen 402c", and no CX boundary carries a last-seen price (audit High 2, Medium 5). The default fixture stays fully priced; the unpriced path is tested with test-local data. Source: run record, CONFIRM C1, agreed by the Supervisor 2026-10-09T23:27:24Z.
2. **C4 divine-scale rounding.** The engine computes `rerolls = round(feeders × N)` and `lifeforce = round(feeders × N × 30)` from the unrounded `N`. This matches CHECKLIST and `derived.fossil.divLine` (~11,349 Wild for Fossils). The README wording "rerolls = feeders × N(i) (rounded), lifeforce = rerolls × 30" gives 378 × 30 = 11,340 and should be reworded. Source: run record, CONFIRM C4; audit Medium 14.
3. **Reference 04, Corrupted Essences, Hysteria +19.3c.** This was drawn from the unconverged engine. The exact fixed-policy solve gives 19.35c, which half-up one-decimal rounding prints as +19.4c (the ticket's figure). The page prints +19.4c. Source: run record, SUPERVISOR rounding RULING (after WI-1 FIX-1, 08:56), which supersedes the earlier "reference wins".
4. **Reference 08 §5, unpriced floor +23.8c, is an illustration error.** Under the rule (an unpriced type counts 0c and is always kept), the Fossils floor with Hollow unpriced is 23.743c, printed +23.7c. The drawn +23.8c matches 23.789c, which is Hollow fed. The rule wins, and the page prints +23.7c; the designer should recapture §5. Source: run record, SUPERVISOR RULING after WI-1 DONE (08:45).
5. **Weight-share denominator.** A keeper's weight share is `loggedRolls / Σ loggedRolls` over the family's types (`harvest/engine.ts` `weightShare = w / Σw`). For Fossils that is Σ loggedRolls = 4,720, not `sample.rolls` = 5,271. The status line still prints the sample's 5,271 rolls. Measured from `harvest/__fixtures__/harvest.json`: Delirium Σ = 5,122 = `sample.rolls`; Fossils Σ = 4,720 ≠ 5,271. The ticket should state which number a share and the "N rolls" provenance mean.
6. **Reference 04, Hysteria vs ticket.** The plan's reviewer question 4 resolved "reference over ticket" before the rounding ruling. Item 3 above supersedes it: the ticket's +19.4c is the correct figure.
7. **Seam fields carried but not read (WI-12, completeness C12).** The engine now reads `HarvestFamily.rerollCost` as the lifeforce per reroll (it hard-coded 30 before). These seam fields are carried and drawn nowhere, so nothing reads them: `ExchangeItemPrice.divine`, `HarvestWeightSample.lifeforceSpent`, `HarvestTiers.boundariesChaos`, `HarvestTiers.topBoundaryChaos`, and `HarvestTiers.priceHour` (never compared with `ExchangePriceRead.priceHour`, so a tier/price hour mismatch is not detected). The backend task should keep or drop each deliberately.
8. **Weights provenance label (WI-12, completeness C11).** The status line prints "weights: HarvestForge log · … · one player’s sample" as constants, as references 01–03 draw it. The fixture's `sample.source` reads "requester's HarvestForge log" (and "none — assumed uniform (owner, 2026-10-10)" for Corrupted Essences), which is not the drawn copy, so the page does not print it. The ticket should say whether provenance comes from `source` or stays fixed copy.

## Open decisions

These are questions, not requirements (README § Open decisions). Each is built at its stated interim only (D12).

1. **Price sides.** Which side prices an input, an outcome and lifeforce is not pinned ("follow the Currency Exchange convention"). Interim: one chaos price per item, the fixture's.
2. **Weight updates** (ticket Q11). Merge or replace when a new HarvestForge log arrives, and whether to show a weights-last-updated date. Interim: the status line shows the log's date.
3. **Regex over 250 characters.** Whether to split it into two searches is undecided. Interim: the amber count only, no split.
4. **Orphaned README fragment, for the designer.** README § Open decisions opens with a line that has lost its first half: "no place for it. Interim: not shown. Ask the owner where it goes (chip tooltip, chip mark, or the EV panel)." Its first line was removed in design commit `b89d27dd`, so the subject is unknown. Nothing is built for it. Designer: restore the subject or delete the line. Source: run record 2026-10-09T23:15:22Z.

## CONFIRM rulings

Orchestrator rulings from written authorities, agreed by the Supervisor ("C1-C4 agreed as stated", run record 2026-10-09T23:27:24Z):

- **C1 — unpriced tier slot and seam widening.** Reference 08 §5 shows Hollow UNPRICED inside a TOP card ("1 type · —") that "keeps its tier slot". So `tiers.byItem` keeps the entry, and a card with no priced member prints "—" as its range. The CX read model gets `chaos: number | null` + `lastSeenChaos: number | null`. Sources: ref 08 §5; README § Spec changes (unpriced type).
- **C2 — pick vs unpriced.** The unpriced rule wins over a pick: "always a keeper, never fed" (README) and "never feed what can't be priced" (ref 08 §5). The chip click and the tier move skip an unpriced type.
- **C3 — default horizon.** `'day'`, under its own pref `harvestFlippingHorizon`. The parser falls back to `'day'`, not to CX's `'recent'`. Sources: `fixtures.json` `"horizon": "day"`; references 01 and 08 show Day 24h; audit Medium 9.
- **C4 — divine-scale rounding.** `rerolls = round(feeders × N)` and `lifeforce = round(feeders × N × rerollCost)` (30 for every logged family) from the unrounded `N`. Sources: CHECKLIST (acceptance contract) and `derived.fossil.divLine` (~11,349); the README wording is under Ticket updates needed (item 2).

### D5 — one divine rate (the shared store)

One current divine rate feeds both the status line ("1 div = 360c") and the divine-scale calculation. The controller takes a dependency `divineRate: () => number | null` and uses `divineRate() ?? exchange.divineChaosRate`. Since WI-10, production `divineRate` is `currentDivineRate()` from POE-284's shared store (`$lib/stores/divine-rate.svelte`), read-only — the rate the top-bar chip shows — so the top bar and Harvest never show two different rates. The exchange seam's `ExchangePriceRead.divineChaosRate` (fixture 360.07) stands in only while that store is cold (`null`). A store rate that moves (400 → 300) moves both lines without a reload (`controller.svelte.test.ts`, "the shared divine rate"). Sources: plan D5 and R2-1; run record SUPERVISOR STATUS 2026-10-09T23:59:54Z and GO (store API path).

## Designer items

Accepted by the Supervisor (WI-6 answer, run record after 09:26:43Z), recorded for the designer:

- **Q2 — empty feed set (superseded, WI-12).** The prototype draws it (`Overview-prototype.dc.html` lines 454–455), so the page builds it: "Every type sells for more than a reroll returns. Closest: <full name> <signed EV>.", with the headline EV "—" (muted) over "no feeder". The closest is the priced type with the highest loop EV when every type is kept; an unpriced type is never named. With no priced type at all there is nothing to name and the reason line is left out (undrawn). The earlier ruling (no reason line) is superseded by the Supervisor (run record after 10:46:37Z).
- **Nothing to flip keeps the EV panel (WI-12).** As drawn (prototype lines 451–456), the "EV of your selection" panel stays with every value "—", and both nothing-to-flip states show the "—" headline EV over "no feeder".
- **Unpriced chip has no title (WI-12).** The drawn chip titles (prototype line 416) say "click to feed just <name>", which an unpriced chip cannot do (C2). The prototype draws no unpriced type, so the unpriced chip has no title; a drawn title is a designer item.
- **"N types moved by you" counts changes only (WI-12).** As the prototype does (lines 463–471), a pick counts only when its side differs from the engine's own decision (`price ≥ reroll value`) under the current picks, and Reset is inert when none differs. A chip whose pick matches the engine still shows the dashed "your pick" style, as in the prototype. When the picks leave nothing to flip, the side the engine takes without picks stands in for the comparison (the prototype's own count is undefined there).
- **Q5 — plural unpriced note.** With two or more unpriced types, the EV-panel floor note uses a mechanical plural of the drawn singular sentence ("1 unpriced type counted as 0c, so this EV is a floor…"). It is not a new state.
- **Q6 — untiered chips.** A type with no `tiers.byItem` entry is listed plainly in its column (same chip, no new card style, no invented header), never hidden (ADR-017 visibility).
- **Clipboard write failure has no drawn state (WI-8).** When the clipboard write is refused, the app logs a warning (`console.warn`) and the button stays "Copy"; nothing else is shown (Supervisor ruling). A drawn error state is a designer item if wanted.
- **Before any port answers, no tabs (WI-8).** The tabs come from the Harvest server read, so before either owner answers the page shows the drawn "Loading…" line and the skeleton panel without tabs; once the Harvest read lands, tabs show while prices load (ref 08 §1). Reference 08 §1 assumes tabs ship with the app; a drawn pre-tab loading state, or tab labels shipped with the app, is a designer item.
- **Horizon control has no tooltip (WI-8).** The undrawn `title` on the Prices segmented control was dropped (Supervisor ruling); only the "Prices" label and the two options are shown.
- **Designer intake — "prices unavailable" no-data variant (Q7).** The no-data panel is drawn only for families without weights (ref 06, ref 08 §9). Engine results whose numbers are unusable reuse its text (see PENDING OPERATOR). A drawn "prices unavailable" variant of the panel is requested.
- **Stale status line keeps the weights segment (WI-11).** Reference 08 §3 draws the stale line as "stale since 21:04 — server unreachable · prices from 21:00 (2 h ago) · prices: Currency Exchange, Day 24h · 1 div = 360c" and stops there; reference 01's ready line ends with the weights provenance. The page keeps the weights segment in the stale state too, since every derived number shows its provenance. Designer: confirm, or redraw §3 if the stale line should drop it.
- **Unpriced chip shows no weight share (WI-11).** Reference 08 §5 draws an unpriced keeper as "Hollow — UNPRICED" with no share; the page now matches (priced keepers keep their share).
- **Sidebar entry focus ring (WI-11).** Pre-existing app-wide gap: no sidebar item had a focus ring (Sidebar.svelte all: unset, no :focus-visible). The Harvest entry inherited it; no Sidebar.svelte change in this lane (Supervisor ruling). Fixed for every sidebar item by commit 422bc7a4 (`fix(desktop): sidebar: keyboard focus ring on every item`): a `--color-lab-blue` `:focus-visible` outline on `.nav-item` and `.collapsed-item`.
- **Headline EV contrast (WI-11).** `--color-lab-red` on `--color-lab-surface` measures 4.47:1, below 4.5:1 for normal text. `.ev-big` is now 1.375rem/700, matching the prototype's `.gain` weight (700), so it qualifies as large bold text (3:1). Chip loss EVs sit on `--color-lab-bg` at 5.01:1.
- **Tabs on one line at 1024 px (WI-11).** Nowrap + horizontal scroll; fit at 1024 px not verified without a render (pixel pass waived by the Operator 2026-10-10).

## PENDING OPERATOR

Copy the design does not draw. Each string is one named constant in `desktop/src/lib/harvest/view.ts`, recorded for the Operator as P45 (coordinator-18; run record, SUPERVISOR ANSWER after 09:26:43Z):

Q4 (picked-chip titles) is answered by the design (WI-12): every chip carries the prototype's title (line 416), "Kept (engine) — click to feed just <full name>", "Kept (your pick) — …", "Fed (engine) — click to keep just <full name>", "Fed (your pick) — …" (`chipTitle`, `view.ts`). Q7 is answered by the Operator 2026-10-10 (P45, "Accept now, Designer later"): `NO_DATA_TEXT` ships as is; the Designer-intake line for a "prices unavailable" variant stays at § Designer items. Nothing is pending in this section.

- **Q7 — no-data text for unusable-number engine results (answered: ships as is).**
  - `NO_DATA_TEXT` (`view.ts:644`): "The EV needs how often each type comes out of a reroll. Nobody has sent a HarvestForge log for this family yet, so there is nothing to compute. The tab fills in when one arrives."

## Follow-ups outside this lane

- **Sidebar entry hidden without `BETA_FEATURE` has no test (WI-12, completeness C18).** INVENTORY B22 rests on reading `Sidebar.svelte`; no Sidebar or layout test covers any feature-gated entry (a pre-existing pattern, not Harvest's). Follow-up task: a Sidebar test that the gated entries (Harvest Flipping among them) are absent without their feature and present with it. Outside this lane's files.

- **Lifeforce colour facts for `docs/GAME-FACTS.md`** (another owner's file; noted here only). Primal = blue, Vivid = yellow, Wild = purple. A reroll costs 30 lifeforce of the family's colour. Deafening and corrupted essences cost Primal (blue), Fossils cost Wild (purple), and Delirium Orbs cost Primal. Sources: `fixtures.json` `exchange.lifeforce` and `families[].lifeforce`; README § Spec changes ("the blue ones", poedb). `docs/GAME-FACTS.md` has no lifeforce entry at `d82d311a`.
- **`items.json` name defect.** `internal/exchange/itemdata/items.json:847` names `Metadata/Items/Currency/CurrencyAfflictionOrbHarbinger` "Fine Delirium Orb", while its icon URL (`icon-urls.json:169`) is the Foreboding Delirium Orb. `…Prophecies` (`items.json:882`, icon Portentous) carries the same name; `design/PROJECT.md:76` records both. The real Fine Delirium Orb is `CurrencyAfflictionOrbCurrency`. Harvest looks items up by itemId only, but any name lookup would collide. Fix the item names in the item data.
