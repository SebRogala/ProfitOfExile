# PROJECT.md — living project brain for ProfitOfExile (design/)

> Durable memory of what is being designed and every decision made. `design/CLAUDE.md` points at
> the playbook; THIS file holds project truth.

## What we're designing
- **Surface(s):** Desktop app (Tauri 2 + SvelteKit) main window → Strategies section of the
  sidebar.
- **Audience & goal:** PoE players farming Harvest lifeforce; decide which reroll flips pay and
  when to stop rerolling (POE-283).
- **Platforms / themes / locale:** Windows desktop, 1024×768 default, resizable; dark only;
  English, game item names verbatim.

## Design context (source of truth)
- **Design system / tokens:** `desktop/src/tokens.css`. Two palettes live there: the legacy
  `--bg/--surface/--border/--text/--accent` set (sidebar, Button, Toggle) and the
  `--color-lab-*` set (every Strategies page, SegmentedButtons, Select). Strategy pages use
  `--color-lab-*`.
- **Component vocabulary:** `desktop/src/lib/components/` — SegmentedButtons, Button, Toggle,
  Tooltip, Select, ItemIcon, RangeSlider, ExchangeItemPicker, ExchangeCategoryPills,
  ExchangeFilterBar. Closest page: `desktop/src/lib/pages/CurrencyExchangePage.svelte` (page-head
  bar, status line, sticky-header table in a bordered surface, Marks legend strip).
- **Reference material:** `design/tasks/poe-283--desktop-app--strategies--harvest-flipping.intake.md`.
- **Rule:** use the token/component layer; never raw hex. Flag any new token candidate.

## Design system decisions (the committed system)
- **Type:** system UI stack; numbers in `'Consolas','Monaco',monospace` weight 600 (CX `.mono`);
  page title 1rem/700; header labels 0.6875rem uppercase, letter-spacing 0.5px; body 0.8125rem.
- **Colour:** ground `--color-lab-bg`, panels `--color-lab-surface` + 1px `--color-lab-border`;
  gain `--color-lab-green`, loss `--color-lab-red`, caveat `--color-lab-yellow`, focus/links
  `--color-lab-blue`; segmented active fill rgba(99,102,241,.2) (inherited from the component).
- **Token candidates:** none. CX hard-codes `#6b7280` / `#4b5563`, which fail AA as text on
  `--color-lab-surface`; this design uses `--color-lab-text-secondary` / `--color-lab-text-muted`.
- **Shared components built:** —

## Screen inventory (the plan + status)
| # | Page path (breadcrumb) | Status | File / frame id | Notes |
|---|---|---|---|---|
| 1 | Desktop app → Strategies → Harvest Flipping (variant a: family-first table) | reference only | `Main.dc.html` | POE-283; interactive: tabs, sort, overrides |
| 4 | Desktop app → Strategies → Harvest Flipping (variant c: family overview — tiers, regex) | agreed | `Overview.dc.html` (page round 2) | POE-283; owner's sketch 2026-10-10; B to be dropped |
| 5 | Desktop app → Top bar — divine rate | in review | `TopBar.dc.html` (page round 2) | POE-284; ready / cold / stale + tooltip |
| 3 | Desktop app → Strategies → Harvest Flipping — states | in review | `States.dc.html` (page round 2) | C layout: loading, cold, stale, error, unpriced, not worth it, nothing to flip, regex too long, no data |
| 6 | Desktop app → Overlay → Boss loot confirmation (A icon grid / B list) | in review | `Main.dc.html`, `OverlayList.dc.html` (boss-loot canvas, round 1) | POE-317; interactive: pick, toggles, countdown, Own/Collective |
| 7 | Desktop app → Overlay → Boss loot capture — frame + states | in review | `OverlayStates.dc.html` (boss-loot, round 1) | preview, capture, refused, submitting, saved, unreachable, merged, 2 bases, nothing matched, fragment, low n, auto-save off |
| 8 | Desktop app → Boss loot (Results · Runs · Settings) + sidebar entry and Modules row | superseded by #10 | `Page.dc.html` (boss-loot, round 1) | POE-317; module settings live on the page |
| 9 | Desktop app → Sidebar → icons: Boss loot, Chaos ⇄ Div | rejected | `Icons.dc.html` (boss-loot, round 1) | 3 candidates each |
| 10 | Desktop app → Boss loot r2 (sort by name/rate, runs pagination, no Runs column; position row moved out) | in review | `Page-r2.dc.html` (boss-loot, round 2) | operator feedback r1 |
| 11 | Desktop app → Settings → Overlay Positions → Boss loot group | in review | `SettingsOverlays.dc.html` (boss-loot, round 2) | position lives in global Settings |
| 12 | Desktop app → Sidebar → icons r2: generic boss, Faustus helper | in review | `Icons-r2.dc.html` (boss-loot, round 2) | r1 candidates all rejected |
| 13 | Desktop app → Overlay → Boss loot confirmation · general shape (Catarina, A grid / B list) | in review | `R3Catarina.dc.html` + component `LootOverlay.dc.html` (boss-loot, round 3) | 0..k guaranteed groups, category tiles, quantities, which-one picker, profile key |
| 14 | Desktop app → Overlay → general shape · stress (Maven, Incarnation of Fear, Black Star) | in review | `R3Stress.dc.html` (boss-loot, round 3) | proves a second boss needs no redesign |
| 15 | Handoff · build sources: BossLootOverlay / BossLootCapture / BossLootPage / SettingsOverlays prototypes | handed off | `*-prototype.dc.html` (boss-loot, page Handoff · build sources) | POE-317 handoff 1 |

## File map
- **Canvas `desktop-app--boss-loot`:** https://claude.ai/artifact/Wgm3i9gPEpT3dwL8HB7BPg
  — source in `design/canvas/desktop-app--boss-loot/project/` (shared `theme.css`, fixture `data.js`).
  Item art: GGG CDN via poe.ninja/RePoE; nav-icon art: minimap sheet `art/2dart/minimap/player.dds`.
  Handoff: `design/handoffs/poe-317--desktop-app--boss-loot/` (build sources on page "Handoff · build sources").
- **Canvas `desktop-app--strategies--harvest-flipping`:** https://claude.ai/artifact/PTogG15PdG1WJYjbDqNHJf
  — source in `design/canvas/desktop-app--strategies--harvest-flipping/project/`.
- **Handoffs:** `design/handoffs/poe-283--desktop-app--strategies--harvest-flipping/` (build source `Overview-prototype.dc.html`) and `design/handoffs/poe-284--desktop-app--top-bar--divine-rate/` (build source `TopBar-prototype.dc.html`); both prototypes on the canvas page "Handoff · build sources". Index: `design/handoffs/README.md`.

## Decisions log (append-only — date + decision + why)
- 2026-10-09 — design/ set up per playbook LOCAL.md (Door 2b); canvas created for POE-283.
- 2026-10-09 — Per item only, no stack-size input (owner).
- 2026-10-09 — Obscured Delirium Orb dropped: no longer in the game (owner).
- 2026-10-09 — Corrupted essences (Hysteria, Insanity, Horror, Delirium) are their own closed pool, reachable only by rerolling a corrupted essence; own tab, weights assumed uniform, no logged sample (owner).
- 2026-10-09 — Essences v1 = Deafening only (owner).
- 2026-10-09 — Div-vs-chaos is per keeper row ("Sell in"), POE-123 maths; a divine market under 200/day is marked thin (owner picked placement; threshold is Designer's default).
- 2026-10-09 — Defaults taken by Designer, open to change: one price per item (fixture = 24h VWAP; buy/sell side still open, brief Q4); horizon control mirrors CX (Recent 6h / Day 24h); Astrolabes/Oils/Catalysts as "no data" tabs; overrides marked "forced" with per-row and Reset-all clear.
- 2026-10-09 — Unpriced type: counts 0c as an outcome (EV is a floor), is kept as an input (never recommend rerolling what can't be priced), and the summary says how much its price would move the best loop.
- 2026-10-09 — Lifeforce shown as icon + name, no underline. The colour word is in plain text only on the cost line: B "30 × [icon] Wild (purple)", A "30 × [icon] Wild lifeforce (purple)". Never "Crystallised". Primal blue, Vivid yellow, Wild purple (owner).
- 2026-10-09 — Tables are CSS grid + ARIA roles, not `<table>`: the canvas's `<sc-for>` cannot live inside `<tbody>`. The Svelte build should use a real `<table>` like CX.
- 2026-10-10 — Variant C from the owner's sketch: verdict, then Feeders and Keepers as tier cards (unified tier system, `internal/lab/classification.go`, ported without the gem-only >5c filter). Tiers are multi-selectable (move a whole tier, or one type), default = engine's keep set, so a tier can be split. Headline EV = loop EV of the cheapest feeder.
- 2026-10-10 — Stash regex per set: shortest ≥3-letter fragment unique within the family, `"a|b|c"`, counted against PoE 1's 250-character cap (raised in 3.26 per a GGG forum report; not seen in official patch notes).
- 2026-10-10 — Cost per reroll gains a divine line: "1 div → N lifeforce → M rerolls", from the lifeforce/divine market (Wild 9,905, Primal 7,445, Vivid 1,917 per div, Allflame 24h).
- 2026-10-10 — Hollow Fossil stays in the pool: the requester's log has 1 in 5,271, and poedb lists no exclusions for the fossil change.
- 2026-10-10 — Essence rerolls (Deafening and corrupted) cost 30 Primal (blue), not Vivid: requester confirmed "blue ones" via the owner, matching poedb. POE-283's "Vivid for Essences" line (and the brief copied from it) is wrong. At Primal prices essences pay: keep Scorn/Envy/Misery/Zeal/Loathing/Rage, feed Torment +6.3c; corrupted: keep Horror, feed Hysteria +19.4c.
- 2026-10-10 — App-wide divine:chaos rate goes in the top bar, one source for every page: the Currency Exchange rate (owner). Filed as POE-284 (relates to POE-283); top-bar mock lives on this canvas until the shell gets its own canvas.
- 2026-10-10 — Owner answers to the brief's open questions: tiers computed on the server and cached so every user gets them warm, keep set/EV/regex on the desktop (Q3); price sides follow the Currency Exchange convention (Q4); keep/feed picks persist across sessions via the prefs map (Q6); page hidden behind the `beta` grant (Q2); desktop only, the web frontend is deprecated except Lab (Q1); handoff wanted (Q13).
- 2026-10-10 — Variant C is the design; B removed from the canvas, A kept as reference only. App chrome (sidebar, top bar) on the boards is a 1:1 recreation of `Sidebar.svelte` / `TopBar.svelte`: the handoff adds only the Harvest nav entry (shown only to `beta`-granted devices, no badge — the menu never shows one) and, under POE-284, the rate chip — nothing else in the chrome changes.
- 2026-10-10 — Harvest Flipping nav icon: Wild Crystallised Lifeforce art with a green recycle badge filling its bottom-right quadrant, shipped as an image like `lab-icon.png` (owner). Rate chip sits at the top bar's right, left of the window controls (owner).
- 2026-10-10 — Handoff 1 for POE-283 and POE-284 as two packages (one implementer session each). Reference PNGs captured by the Designer session with headless Chrome from the canvas source + the type's runtime, at the owner's request. POE-283's prototype has the chrome exactly as today plus the nav entry; the rate chip lives only in POE-284.
- 2026-10-10 — Div vs chaos per keeper replaced by a divine-scale line in the verdict panel: "1 div profit ≈ N feeders · ~R rerolls · ~L lifeforce" (owner).
- 2026-11-01 — POE-317 (Boss loot) round 1. Owner answers: boss profiles built-in (no profiles UI; collective rates need one table); "not counted" is per item and saved, set on the module page (the item's rate leaves out this device's runs); a capture in the same seed auto-merges into the previous run with "Save as new run" to undo; the nav entry takes the DEV "Bosses (soon)" slot under Strategies, named "Boss loot".
- 2026-11-01 — POE-317 fixture = the owner's 78-run sheet (every run has exactly one unique-pool base). Base → unique mapping confirmed against poe.ninja: Silver Flask → Cinderswallow Urn, Vile Arrow Quiver → Spinehail, Serpentine Staff → Cane of Kulemak, Necromancer Circlet → The Devouring Diadem, Titanium Spirit Shield → Bitterbind Point, Vaal Regalia → The Queen's Hunger. Nook's Crown and Keeper's Corruption are divination cards.
- 2026-11-01 — POE-317 Designer defaults, open to change: auto-save 10 s (5/10/20/30 s/Off); countdown pauses while the unique pool has 0 or 2+ picks; "Rare, two veiled modifiers" is manual-only (OCR cannot see mods); capture frame drawn in `--color-lab-cyan` corner brackets (yellow when refused); confirmation overlay = a widget in the existing WidgetHost/hot-rect system, placed with the overlay layout mode ("Move overlay").
- 2026-11-01 — POE-317 round 2 (operator feedback on r1): overlay position moves to global Settings → Overlay Positions as a "Boss loot" group (Show toggle + Configure widgets), not the module page; Results drops the Runs column (n stated once in the Rate header) and sorts by Item (A–Z, ignoring "The") or Rate (high first), within each group; Runs lists all 78 runs, 10 per page, newest first, with additional drops as named chips that wrap; r1 icons all rejected: generic boss candidates D–G and Faustus-helper candidates D–F on `Icons-r2`.
- 2026-11-01 — POE-317 round 3 (operator GO + scout report of 17 boss profiles): the overlay is one component over a boss profile = 0..k guaranteed groups (each with min/max and its own check; Black Star has none, Incarnations have unique + Astrolabe) + additional drops (named items, dashed category tiles with a count for open sets such as Catarina's veiled rare or Memory Strand items, and a quantity stepper on stackable drops). A base shared by several uniques is never pre-filled: the candidates get a dashed yellow ring and a "which one?" prompt, and the countdown pauses until one is picked. The profile key (Normal / Normal pool / Uber pool / invitation) is a grey badge beside the boss name. A boss with no runs shows the wiki estimate in place of a rate. Handoff open items (data rules, not design): dedup across multi-phase loot, a quest-Voidstone ignore list, rate scaling by item quantity.
- 2026-11-01 — POE-317 handoff 1 (operator): both overlay layouts ship, Grid and List, chosen by a "Loot layout" setting in global Settings → Overlay Positions → Boss loot group (next to the widget's Show toggle and placement; the module page keeps behaviour settings: auto-save, shortcuts, not counted). Icons and the "Faustus helper" name stay open: current glyphs (👹, ⇄) and labels kept. "Not saved" badge text moved to `--color-lab-text` on a stronger red tint (red on the red tint measured 4.41:1). Default overlay placement (1480, 120) at 1080p is the Designer's.

## Open questions / to confirm
- 2026-10-10: POE-283 got a "Lifeforce colour correction" section (Primal for essences).
- Brief Q11 (weight updates: merge vs replace, last-updated date) — still open.
- Price sides (input / outcome / lifeforce) under the CX convention — not pinned.
- Data: `items.json` names both `CurrencyAfflictionOrbHarbinger` and `…Prophecies` "Fine Delirium Orb" (icons say Foreboding / Portentous); Fine = `CurrencyAfflictionOrbCurrency`. Name lookups would collide.
- Token gap: CX hard-codes `#6b7280` / `#4b5563` greys that fail AA on `--color-lab-surface`; this design uses `--color-lab-text-secondary` / `--color-lab-text-muted` instead, no new token.
- POE-317: owner to pick overlay variant (A grid / B list), nav icons, and whether "server unreachable" offers a retry (owner said nothing more; drawn without one). Wiki rate shows in B as a column, in A only on hover.
- POE-317 r2: "many additional items in a single run" — layout handles any count; open whether one item can drop more than once per run (needs a quantity). Is "Faustus helper" a rename of the Chaos ⇄ Div page or only the icon concept?
