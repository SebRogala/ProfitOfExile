# Reference screens — index

Ground-truth renders of the stripped prototypes on the canvas page **Handoff · build sources**
(https://claude.ai/artifact/Wgm3i9gPEpT3dwL8HB7BPg), captured with the playbook's
`tools/capture-canvas.mjs` in headless Chromium.

- **Overlay PNGs:** 420 px wide; the panel is 400 px (grid) or 420 px (list) wide, drawn from the top-left; the rest is transparent-page background.
- **Capture frame PNGs:** 940 × 790; the frame is the real 900 × 750 capture area at 1080p, drawn over a striped stand-in for the game.
- **Page PNGs:** 1024 × 900 (the app's default window is 1024 × 768; the page scrolls).
- **Settings PNGs:** 1024 wide, Overlay Positions section only.
- **Captured:** 2026-11-01, after the last prototype change on the same day.
- **Fixture state shown:** Catarina profile; capture run 79 (Urn picked, Zorath's Eye + Veiled Exalted Orb + Allflame Ember ticked, Spidersilk Robe unmatched); 78 seed runs, page size 10; Own scope; auto-save 10 s; Communion Support not counted; layout Grid unless the row says List.
- **Known capture artefacts (not design):** the countdown shows whatever second the capture landed on (7 s); the 👹 glyph of the Boss loot nav entry and Modules row renders as a box because headless Chromium has no emoji font (WebView2 on Windows draws it); the sidebar/top bar are stand-ins for the real shell.

## `desktop/`
| File | Screen / state | What to match |
|---|---|---|
| `01-overlay--catarina-grid.png` | Overlay, Catarina, Grid, live countdown | header (name · run · profile-key badge, zone + level), countdown badge + bar, Own/Collective, unique group tiles with `read` tag + check, additional tiles, not-counted Communion, category tile with − 0 +, Not in table line, Discard / Save now |
| `02-overlay--catarina-list.png` | Same, List layout | radio/checkbox rows, own % + wiki column, category row stepper |
| `03-overlay--catarina-no-unique-paused.png` | Urn deselected: guaranteed group below min | "pick 1" (yellow), "Paused: check guaranteed", bar empty |
| `04-overlay--merged-same-seed.png` | Second capture in the same seed | "Same seed as run 79: added to it." + "Save as new run" link |
| `05-overlay--submitting.png` | Saving | "Saving" badge with spinner, no Discard/Save buttons |
| `06-overlay--saved.png` | Saved | "Saved" badge + "Saved as run 79." line, no buttons |
| `07-overlay--server-unreachable.png` | Submit failed | red panel edge, "Not saved" badge, "Server unreachable. This run was not recorded." + Close |
| `08-overlay--maven-which-one-grid.png` | Test profile: three uniques on one base | yellow prompt box, dashed yellow candidates tagged "Cloth Belt?", nothing pre-picked, "which one?", paused; no runs → "wiki N%" in place of rates; quantity stepper on Orb of Conflict (2) |
| `09-overlay--maven-which-one-list.png` | Same in List | dashed yellow rows, quantity stepper at the row end |
| `10-overlay--fear-two-guaranteed-groups.png` | Test profile: two guaranteed groups | two "Guaranteed · … · exactly 1" sections, each with its own status; Memory Strand category tile with count 2 |
| `11-overlay--blackstar-no-guaranteed-quantities.png` | Test profile: no guaranteed group | no guaranteed section at all; quantity steppers (1, 3); "5 items" counts quantities |
| `12-capture--preview.png` | Capture frame, preview (Shift+F8) | cyan corner brackets, chip "Boss loot preview Shift + F8 hides", cursor 2/3 down |
| `13-capture--reading.png` | Capture taken | solid cyan frame, chip with spinner "Reading loot · Catarina · run 79" |
| `14-capture--refused-no-profile.png` | No profile for the zone | yellow brackets, "Not logged · no boss profile for this zone", zone + level + "Boss loot tracks: Catarina." |
| `15-page--results-by-rate.png` | Page, Results, sorted by Rate | sidebar entry + Modules row, page head, boss pill, tabs, Rate ↓ header, groups kept, bars, wiki column, not counted |
| `16-page--results-by-name.png` | Results sorted by Item | Item ↑, A–Z ignoring "The", within each group |
| `17-page--runs-page-1.png` | Runs, page 1 | runs 78–69, "no date", unique + named additional chips, Edit/Delete, pager 1–8 |
| `18-page--runs-page-8-editing.png` | Runs page 8, run 8 in edit | inline editor: unique chips (pick one), additional chips, Save run 8 / Cancel; run 6 shows 4 additional chips wrapping |
| `19-page--settings.png` | Module settings | module toggle, Auto-save segmented, Capture + Preview shortcut buttons, Not counted checklist, Where runs go |
| `20-page--settings-capture-listening.png` | Rebinding the capture shortcut | "Press a key or mouse button…", "Esc cancels" |
| `21-page--collective.png` | Results in Collective | "78 runs · 1 device" |
| `22-settings--overlay-positions-grid.png` | Settings → Overlay Positions, Boss loot group | Loot confirmation Show + geometry, Loot layout Grid/List (Grid) + note, Configure widgets |
| `23-settings--overlay-positions-list.png` | Same with List picked | note "One row per item, wiki rate column" |

## Inspecting beyond the PNGs
Open the canvas page **Handoff · build sources** and press Play on a prototype. The overlay
prototype's Tweaks set `profile`, `layout`, `state` and `timeout`; the capture prototype's set `mode`.
The source files are under `design/canvas/desktop-app--boss-loot/project/` (see `../AGENT.md` §0b).
