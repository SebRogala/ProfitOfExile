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

## File map
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

## Open questions / to confirm
- 2026-10-10: POE-283 got a "Lifeforce colour correction" section (Primal for essences).
- Brief Q11 (weight updates: merge vs replace, last-updated date) — still open.
- Price sides (input / outcome / lifeforce) under the CX convention — not pinned.
- Data: `items.json` names both `CurrencyAfflictionOrbHarbinger` and `…Prophecies` "Fine Delirium Orb" (icons say Foreboding / Portentous); Fine = `CurrencyAfflictionOrbCurrency`. Name lookups would collide.
- Token gap: CX hard-codes `#6b7280` / `#4b5563` greys that fail AA on `--color-lab-surface`; this design uses `--color-lab-text-secondary` / `--color-lab-text-muted` instead, no new token.
