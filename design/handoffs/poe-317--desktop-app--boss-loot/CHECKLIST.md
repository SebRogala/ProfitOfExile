# CHECKLIST.md — acceptance contract for ProfitOfExile — Boss loot (POE-317)

Tick a box ONLY when it is true in your implementation AND matches the reference PNG (dark theme).
Unticked = not done. Don't advance with open boxes on the current screen.

## ⛔ MUST NOT DROP — the edge cases that get silently skipped
- [ ] Everything new is **beta-only**: nav entry, rail entry and Modules row are absent (no gap) without the `beta` grant.
- [ ] The overlay is driven by the **profile**, not by Catarina constants: 0 guaranteed groups renders no guaranteed section (ref 11), 2 groups render two sections with independent status (ref 10). Covered by tests on `fixtures.json` → `testProfiles`.
- [ ] **Which one?** — a base shared by several profile items is never pre-filled: dashed yellow candidates, "<base>?" tags, prompt box, "which one?", countdown paused (refs 08, 09).
- [ ] Countdown **pauses** (empty bar, "Paused: …") while any group is outside min/max or a which-one is open (ref 03); **any click** on the overlay restarts it; Off = no countdown.
- [ ] Quantity stepper appears only on quantity items once ticked; category tiles always show − n +; "N items" counts quantities (ref 11).
- [ ] Not-counted items are disabled on the overlay, excluded from the Runs editor, shown "—" + "not counted" on Results; changing the setting moves an item in/out everywhere.
- [ ] Submit states: Saving (no buttons), Saved ("Saved as run N.", closes after 2 s), Server unreachable (red edge, "Not saved", Close; nothing queued) (refs 05–07).
- [ ] Same-seed capture merges and offers "Save as new run" (ref 04).
- [ ] Zone without a profile: capture refused, nothing logged, frame chip says so (ref 14).
- [ ] No runs for a boss: "No runs logged for this boss yet…" and "wiki X" in place of rates (ref 08).
- [ ] Seed runs with no date render "no date" (ref 17).
- [ ] Unmatched OCR lines show as "Not in table: …"; the line is absent when there are none (refs 08–11).
- [ ] Overlay clicks never reach the game; clicks outside the panel always do (hot rects, OVERLAY-GUIDE).
- [ ] Every number shows its provenance (drops/runs, run count, device count, wiki).
- [ ] Tokens only — zero raw hex outside `desktop/src/tokens.css` (rgba tints of token colours per README).
- [ ] Every control keyboard-reachable in the main window; WCAG AA on the pairs used.
- [ ] Copy in English exactly as in the references; item names exactly as in-game.

## Sidebar (refs 15, 17)
- [ ] "Boss loot" under Strategies after Harvest Flipping, replacing the DEV "Bosses (soon)" row; glyph 👹 (interim).
- [ ] Modules row "Boss loot" with on/off and the README title text; rail switch with status dot.
- [ ] Chaos ⇄ Div entry unchanged.

## Boss loot page — Results (refs 15, 16, 21)
- [ ] Page head: title, caption, Own | Collective, Export CSV.
- [ ] Boss pill "Catarina 78"; tabs Results | Runs | Settings.
- [ ] Line: full boss name, zone id (mono), level, "78 runs · this device" / "· 1 device".
- [ ] Columns Item · Drops · Rate · Wiki; no Runs column; Rate header shows "= drops / 78 runs".
- [ ] Sort by Rate (default, high first) or Item (A–Z ignoring "The"); within each group; arrow on the active header; `aria-sort` set.
- [ ] Group rows "Unique pool · one per kill" / "78 of 78 runs have exactly one" and "Additional drops" / "any number per kill".
- [ ] Rates equal `fixtures.json` → stats: Urn 31% 24, Spinehail 21% 16, Cane 18% 14, Diadem 14% 11, Queen's Hunger 9% 7, Bitterbind 8% 6, Eye 42% 33, Veiled Exalted 33% 26, Allflame 28% 22, Astrolabe 3% 2, Nook's 0%, Keeper's 0%, Rare veiled 0%.

## Boss loot page — Runs (refs 17, 18)
- [ ] Newest first, 10 per page, pager "Runs 78–69 of 78, newest first", ‹ › disabled at the ends.
- [ ] Additional drops as icon + name chips that wrap (run 6: 4 chips).
- [ ] Edit opens the inline editor (pick-one unique chips, additional chips without not-counted items, Save run N / Cancel); page change closes it.
- [ ] Delete per row (confirm behaviour per README Open decisions).

## Boss loot page — Settings (refs 19, 20)
- [ ] Module toggle mirrors the sidebar switch.
- [ ] Auto-save 5 s / 10 s / 20 s / 30 s / Off, default 10 s, persisted.
- [ ] Capture and Preview shortcut buttons; listening state "Press a key or mouse button…" + "Esc cancels"; defaults Shift + Right click, Shift + F8.
- [ ] Not counted checklist, Communion Support checked by default, persisted.
- [ ] "Where runs go" text with the device id.

## Settings → Overlay Positions (refs 22, 23)
- [ ] New "Boss loot" group after Temple: "Loot confirmation" row (Show toggle + geometry text), "Loot layout" Grid | List with its note, Configure widgets.
- [ ] Layout persisted; the next overlay uses it.

## Capture frame (refs 12–14)
- [ ] Preview: cyan corner brackets + chip; follows the cursor; Shift+F8 toggles.
- [ ] Capture: solid cyan frame + "Reading loot · <boss> · run N".
- [ ] Refused: yellow brackets + "Not logged" chip with zone, level and tracked bosses.

## Loot confirmation overlay (refs 01–11)
- [ ] Header: boss · run, profile-key badge, zone + level, countdown/pause/saved badge, Own | Collective, countdown bar.
- [ ] Grid layout (ref 01): 3-column guaranteed, 4-column additional tiles; read tags; checks; rates with drops/runs.
- [ ] List layout (ref 02): radio/checkbox rows, own %, wiki column, category stepper at row end.
- [ ] Group status texts: "1 picked" / "pick 1" / "N picked: max M" / "which one?".
- [ ] Discard closes without sending; Save now saves when valid.
- [ ] Placed as a widget (default right edge, top: fixtures → overlaySettings.defaultPlacement); movable with Configure widgets; hidden when Show is off.

## Tests
- [ ] Profile matching (area + level → profile, hideout/town skipped, no profile → refused).
- [ ] Group validation and which-one detection on `testProfiles` (maven, fear, blackstar).
- [ ] Rate formula and rounding against the Results numbers above.
