# Desktop app → Boss loot (module): page, capture overlay + confirmation, sidebar entry and module switch, module settings, Settings → Overlay Positions

**Handoff 1 — 2026-11-01** · ticket POE-317

## What's in this package
- **Build sources** are not copied here. They live on the canvas page **Handoff · build sources**
  (https://claude.ai/artifact/Wgm3i9gPEpT3dwL8HB7BPg). Their files are under
  `design/canvas/desktop-app--boss-loot/project/`:
  - `BossLootOverlay-prototype.dc.html`: the loot confirmation overlay. Props: `profile`, `layout` (grid | list), `state`, `timeout`.
  - `BossLootCapture-prototype.dc.html`: the capture frame. Prop: `mode` (preview | capture | refused).
  - `BossLootPage-prototype.dc.html`: the Boss loot page (Results · Runs · Settings).
  - `SettingsOverlays-prototype.dc.html`: the new Boss loot group under Settings → Overlay Positions.
  - Shared by all four: `theme.css` (token copy plus shared classes), `data.js` and `profiles.js`. These are the prototype's data; `fixtures.json` is the contract.
- `README.md` — the spec (this file).
- `CHECKLIST.md` — the acceptance contract.
- `fixtures.json` — the data, its owners and formats.
- `reference_screens/` — ground truth, 23 PNGs (index: `reference_screens/INDEX.md`).
- `AGENT.md` — the implementer's protocol. Read it first.
- Every other board on the canvas (rounds 1–3) is **provenance only — do not build from it**.

Give the agent: _"Read `AGENT.md` and follow it exactly. Implement the frontend 1:1 against
`reference_screens/` using `fixtures.json`. Do not stop until every `CHECKLIST.md` box passes; log
anything you can't resolve in `NOTES.md`."_

## Spec changes vs ticket
Compared against POE-317 as of 2026-11-01. The intake brief is
`design/tasks/poe-317--desktop-app--boss-loot.intake.md`, which is gitignored and holds the ticket text verbatim.
- **Readout:** the ticket's POC flashes a dashed frame and a text readout for 1.5 s → **build a clickable confirmation overlay** that shows the boss's whole loot table with item icons, pre-filled from OCR, with an auto-save countdown. Why: owner direction 2026-11-01 and Next #5 (confirm/edit step).
- **Storage:** the ticket writes one local CSV per boss → **runs are submitted to the ProfitOfExile server, with the device id.** Nothing is kept locally. When the server is unreachable, the run is not saved and the overlay says so. Why: owner 2026-11-01.
- **Loot table:** the ticket's hardcoded Catarina sheet columns (Flask, Staff, Body armour, …) → **built-in boss profiles in a general shape.**
  - A profile has zero to k guaranteed groups, each with a min/max, plus additional drops. Additional drops are named items (with an optional quantity) and category tiles.
  - The UI shows real unique names with icons; the base is secondary.
  - Matching is by exact base only. The Cane of Kulemak variants are one entry.
  - Why: owner 2026-11-01, plus operator round 3, so that a second boss needs no redesign.
- **Profiles:** the ticket leaves "editable JSON vs built-in" open → **built-in**, with no profiles UI. Why: owner 2026-11-01; collective rates need one table across devices.
- **Seed dedup:** the ticket says "merge instead of a second run" → **merge automatically and show "Same seed as run N: added to it." with a "Save as new run" link.** Why: owner 2026-11-01.
- **No profile for the zone:** the ticket says "say so, log nothing" → shown in the capture frame chip (reference 14). Why: not recorded beyond the ticket.
- **Editable hotkey "in settings"** → **in the module page's Settings tab**, together with the auto-save timeout and the not-counted list. Why: owner 2026-11-01.
- **Overlay position** → **global Settings → Overlay Positions, as a new "Boss loot" group.** It is the existing widget placement, with a Show toggle and Configure widgets. Why: operator feedback on round 1.
- **New: the "Loot layout" setting (Grid | List), next to the placement in the same group.** Why: the operator asked for both layouts with a user choice (2026-11-01). It sits beside position and Show because it is an overlay-appearance setting. The module page keeps behaviour settings only.
- **New: "Not counted" per item, saved** in the module Settings tab. Default: Communion Support. A not-counted item cannot be ticked, and its rate leaves out this device's runs. Why: owner 2026-11-01 (the loot filter hides it).
- **New: an Own / Collective switch** for the chances shown on the overlay and the page. Why: owner 2026-11-01.
- **New: the page.**
  - Results show drops and rate per item within each group. There is no Runs column; the run count appears once, in the Rate header.
  - Results sort by Item (A–Z, ignoring a leading "The") or Rate (high first). Groups keep their order.
  - Runs are listed newest first, 10 per page, with inline Edit and with Delete.
  - Export CSV.
  - Why: owner 2026-11-01 plus operator feedback on round 1.
- **Sidebar:**
  - A "Boss loot" nav entry under Strategies, in the slot of today's DEV-only "Bosses (soon)" placeholder, which it replaces.
  - A "Boss loot" row in the bottom Modules block.
  - Both are visible only with the `beta` grant.
  - Why: owner 2026-11-01.

## Token and component changes
Token file: `desktop/src/tokens.css`, the only declaration site. **Tokens: none added or changed.**
Selection and state tints are `rgba()` of existing token colours, the same pattern `SegmentedButtons.svelte` uses
for its active fill:
- blue 0.12–0.16 for selected tiles and rows and the countdown badge;
- yellow 0.10–0.16 for the which-one prompt and tags;
- red 0.22 for the "Not saved" badge;
- green 0.14 for the "Saved" badge.

**Token candidates for review:** `--color-lab-blue-tint` (selected item) and `--color-lab-yellow-tint` (needs a decision). Until they are approved, write the rgba values once, in the overlay's component styles.

**Components:**
- Reuse `SegmentedButtons` (Own/Collective, Results/Runs/Settings, Auto-save, Loot layout), `Toggle`, `Button`, `ItemIcon` and `Tooltip`.
- **New, for this module only:**
  - an item tile (grid) and an item row (list) with the states `on`, `off`, `ambiguous`, `not counted` and `category`;
  - a quantity stepper (− n +);
  - a shortcut-capture button (shows the binding; on click it shows "Press a key or mouse button…" and "Esc cancels").

  These live in the module's folder unless a second consumer appears.

The Design System re-sync from code does not apply: this project has no Design System artifact yet (`design/PROJECT.md`).

## Screens
### 1. Sidebar (existing UI touched)
- **Strategies:** "Boss loot" after Harvest Flipping. It replaces the DEV-only "Bosses (soon)" row, and both the expanded and the collapsed rail use today's 👹 glyph until the icon is decided (Open decisions). Clicking it goes to `/boss-loot`.
- **Modules block:** a "Boss loot" row in the Merc OCR / Temple reader pattern (`MODULES`, `MODULE_FEATURES` → `BETA_FEATURE`, `MODULE_TITLES`). The title is "Boss loot: the capture shortcut AND the confirmation overlay. The Boss loot page stays browsable either way."
- The collapsed rail gets the same entry and switch.
- The Chaos ⇄ Div label and glyph stay as they are (Open decisions).

### 2. Boss loot page (`/boss-loot`, a new view in `VIEW_PATHS`)
- **Page head** (the Chaos ⇄ Div / Harvest pattern): "Boss loot", caption "Drop rates per boss from logged kills.", then Own | Collective and Export CSV.
- **Boss pills:** one pill per profile with its run count ("Catarina 78"). Tabs: Results | Runs | Settings.
- **Results:**
  - Line: "Catarina, Master of Undeath", zone `BetrayalMastermindFight` · level 83, then "78 runs · this device" (Own) or "· 1 device" (Collective).
  - Table columns: Item (sortable) · Drops · Rate (sortable; header suffix "= drops / 78 runs") · Wiki.
  - Group header rows: "Unique pool · one per kill" with "78 of 78 runs have exactly one", then "Additional drops" with "any number per kill".
  - Rate cell: a bar plus the percentage. A not-counted item shows "—" drops and a "not counted" badge.
  - Default sort: Rate, high first. Ties break by name.
- **Runs:**
  - Columns: Run · Date ("no date" for seed runs) · Unique (icon + name) · Additional (icon + name chips that wrap) · Edit / Delete.
  - The pager reads "Runs 78–69 of 78, newest first", with ‹ 1…8 ›.
  - Edit expands the row inline: the unique chips (pick one), the additional chips (toggle; not-counted items are left out), then "Save run N" and Cancel.
- **Settings:**
  - Boss loot module toggle (mirrors the sidebar switch).
  - Auto-save after 5 s / 10 s / 20 s / 30 s / Off.
  - Capture shortcut (default Shift + Right click).
  - Preview frame (default Shift + F8).
  - Not counted: a checkbox per additional item.
  - Where runs go: "To the ProfitOfExile server, with this device's id `<id>`. Nothing is kept on this PC; a run made while the server is down is not saved."

### 3. Capture frame (overlay; references 12–14)
- The 900 × 750 capture area at 1080p, scaled by screen height, with the cursor 2/3 down. It is the POC geometry.
- **Preview:** cyan (`--color-lab-cyan`) corner brackets. Chip: **Boss loot** · preview · [Shift] + [F8] · hides.
- **Capture:** a solid cyan frame. Chip: spinner + "Reading loot · Catarina · run 79". The frame then gives way to the confirmation overlay.
- **Refused:** yellow brackets. Chip: "**Not logged** · no boss profile for this zone" and "Last zone `<area>`, level N. Boss loot tracks: Catarina."

### 4. Loot confirmation overlay (a widget; references 01–11)
- **Header:** "<Boss> · run N" with the profile-key badge (Normal / Normal pool / Uber pool / invitation name), then "Zone `<area>`, level N". On the right: the countdown badge ("Saving in N s") or the pause badge ("Paused: check guaranteed" / "Paused: which <base>?"), then Own | Collective. Under the header: the countdown bar.
- **Line:** "Chances from N runs" (+ " · 1 device" in Collective), or "No runs logged for this boss yet: rates show the wiki estimate."
- **Guaranteed sections**, one per group: "Guaranteed · <label> · exactly N" (or "min–max") with a status on the right:
  - "1 picked" (green);
  - "pick 1" (yellow);
  - "N picked: max M" (red);
  - "which one?" (yellow).
  
  A profile with no group renders no section.
- **Additional drops:** "N items" counts quantities.
  - Named items toggle on and off.
  - A quantity item shows − n + once it is on.
  - Category tiles have a dashed border, an "any" placeholder icon and a "category" tag; they always show − n +.
- **Footer:** "Not in table: <unmatched lines>" when there are any; Discard and Save now.
- **Grid layout:** guaranteed tiles in 3 columns, additional tiles in 4. Each tile shows the icon, the name and "<rate> <drops/runs>" (or "wiki X" with no runs). A "read" tag marks OCR-filled items; a blue check marks selected ones. The wiki rate is in the tile's title.
- **List layout:** one row per item: radio (guaranteed) or checkbox (additional), icon, name over base (+ " · read"), own %, then the wiki column.

## Interactions & state
- **Countdown:** it starts at the auto-save setting (default 10 s) and counts down whole seconds. **Any click on the overlay restarts it.** It saves at 0. It pauses, with an empty bar, while any guaranteed group is outside its min/max or a which-one choice is open. Off: no countdown, and the overlay waits for Save. (Reason: an auto-save must never write a run the profile says is impossible.)
- **Which one?** When OCR reads a base that several items in the profile share (Maven: 3 uniques on Cloth Belt), **nothing is pre-filled**:
  - The candidates get a dashed yellow ring and a "<base>?" tag, and a yellow prompt reads "**<base>** was read. Several uniques share this base: which one dropped?".
  - Picking one resolves it.
  - Reason: a silent guess corrupts the cumulative rates (operator round 3).
- **Guaranteed group with max 1:** picking another item moves the selection, and clicking the picked item clears it.
- **Not counted:** the tile is disabled ("not counted" badge). Its title is "Not counted: change on the Boss loot page".
- **Merged:** a capture whose Client.txt seed equals the previous run's merges into that run. The line offers "Save as new run".
- **Save:**
  - The badge becomes "Saving" (spinner) and the buttons go.
  - Success: "Saved" and "Saved as run N.", then the panel closes after 2 s.
  - Server unreachable: red panel edge, "Not saved", "Server unreachable. This run was not recorded." and Close. No retry and no queue.
- **Discard** closes the panel; nothing is sent.
- **Overlay focus:** the panel is a widget in the existing WidgetHost/hot-rect system (`docs/OVERLAY-GUIDE.md`). Its whole rect is hot, so clicks on it are consumed and never reach the game; everything else still goes to the game. It is placed with Configure widgets.
- **Shortcuts:** they are polled, not consumed, and active only while PoE is in the foreground (POC behaviour, unchanged). Rebinding: click the button, then press a key or mouse button with modifiers; Esc cancels.
- **Own / Collective:** the overlay and the page each have the switch. It changes only the stats source; an item with 0 collective drops shows "—".
- **Loot layout:** Grid or List, global, saved. It applies to the next overlay shown.

## Data shapes
See `fixtures.json`. It covers owners, formats, the Catarina profile, test profiles, capture, stats, runs, submit outcomes, module settings, overlay settings and the device. Runs and stats come from the server; the endpoints are not designed, so stub them at the seam.

## Assets
- **Item art:** the GGG CDN (via poe.ninja / RePoE), as uploaded to the canvas. The source files and their URLs are in `design/PROJECT.md` → File map. How the app delivers them is an open decision.
- No fonts are loaded: the system UI stack and `'Consolas','Monaco',monospace` for numbers, as in the rest of the app.

## Glossary
- **Profile** — a built-in boss loot table plus its attribution rule (area id, level, profile key).
- **Profile key** — which pool of the boss applies (Normal, Uber, invitation).
- **Guaranteed group** — items of which exactly min..max drop per kill.
- **Category tile** — an open set that cannot be listed as icons, counted by hand.
- **Not counted** — a per-item setting for items the player's loot filter hides.
- **Seed** — the Client.txt instance seed; one seed is one run.

## Constraints
- Dark theme only. WCAG AA, with the pairs measured in the design round. Every control is keyboard-reachable in the main window.
- Overlay text is at least 11 px at 1080p; the corner tags ("read", "<base>?", "category") are 10 px. The overlay scales with the widget host across 1080p and 4K.
- Visibility is the default (ADR-017/018): no default-on filter hides a row; sort follows the picked column.
- Provenance is shown for every number (drops/runs, n, wiki).
- **Out of scope:** the boss-profile engine beyond the Catarina profile, other bosses' tables (the test profiles exercise the shape only), OCR internals, the server endpoints.

## Open decisions — questions, not requirements; do not invent answers
- **Icons:** the Boss loot nav, rail and module icon and the Chaos ⇄ Div / "Faustus helper" icon are undecided. None of the round 1–2 candidates was accepted. Interim: keep today's glyphs (👹 for Boss loot, ⇄ for Chaos ⇄ Div).
- **"Faustus helper"** as the Chaos ⇄ Div page name. Interim: keep "Chaos ⇄ Div".
- **Item icon delivery:** bundle the loot-table art in `desktop/static/`, or add it to the server's `/api/icon/items` map (`docs/ICONS.md`). Interim: the implementer's choice, noted in NOTES.md.
- **Server endpoints** (submit a run, read per-boss stats for Own/Collective, list/edit/delete runs): not designed. Interim: a fixture adapter per owner (AGENT.md §6).
- **Dedup across multi-phase loot** (several captures that are not one seed, e.g. Venarius phases, the Feared): a data rule. Interim: seed merge only.
- **Quest-item ignore list** (first-kill Voidstones): a data rule; it does not affect Catarina. Interim: none.
- **Rates scaled by item quantity** (Black Star, normal Exarch/Eater): a data rule. Interim: plain drops/runs.
- **Uber vs normal attribution:** whether the uber fights share an area id. Interim: area id + level as in the ticket.
- **Rarity:** a rare or magic item on a pool base (e.g. a rare Titanium Spirit Shield) reads like the unique unless OCR uses the label colour. Interim: exact-base text match, and the player corrects it on the overlay.
- **Export:** CSV columns (sheet order vs item names), clipboard, Excel. Interim: CSV, one row per run, one column per item in profile order, using item names.
- **Delete a run:** confirm or not. Interim: the app's existing confirm pattern if one exists; otherwise none, noted in NOTES.md.
- **A paused overlay when the next capture arrives:** interim: the newer capture replaces it, and the unsaved one is discarded.
- **Existing contrast debt,** not introduced here: Settings `--text-muted` on `--surface` is 4.48:1, and the sidebar active `--accent` on `--border` is 3.26:1.
- **Page loading / stats unreachable** on the Boss loot page is not drawn. Interim: the Currency Exchange page's status-line pattern for loading and unreachable, with the table empty.
- **Token candidates** `--color-lab-blue-tint` / `--color-lab-yellow-tint` (Token and component changes).
