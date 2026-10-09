# Reference screens — index

Ground-truth renders at true frame width, full content height, no crop and no margin. Match these in
layout, spacing, colour (tokens) and copy. Dark only.

- **Desktop screens:** 1024 px wide (the app's default window) × natural height.
- **Captured:** 2026-10-10. The session rendered them with headless Chrome from the canvas source and the
  canvas runtime. Recapture if `Overview-prototype.dc.html` changes after this date.
- **Fixture state shown:** league Allflame, horizon Day 24h, prices up to 2026-09-09 23:00 UTC, no picks
  (except 05), active family per row below.

## `desktop/`
| File | Screen / state | What to match |
|---|---|---|
| `01-harvest--fossils.png` | Fossils, default | Full page in the real shell; verdict +25.7c Lucent Fossil; MID/LOW feeders, TOP/HIGH/MID keepers |
| `02-harvest--delirium-orbs.png` | Delirium Orbs tab | Keepers Diviner's + Skittering; Primal cost |
| `03-harvest--deafening-essences.png` | Deafening Essences tab | Primal 1.5c; feed Torment +6.3c; six keepers |
| `04-harvest--corrupted-essences.png` | Corrupted Essences tab | Closed pool of four; "assumed uniform" provenance |
| `05-harvest--fossils-split-tier.png` | Fossils, Dense clicked | MID "split" in both columns; dashed amber Dense chip; "1 type moved by you"; Reset enabled |
| `06-harvest--astrolabes-no-data.png` | Astrolabes tab | No-data panel; tab still listed |
| `07-harvest--copy-pressed.png` | Fossils, Copy pressed | Button reads "Copied" |
| `08-harvest--states.png` | States sheet (page area) | Loading, cold, stale, error, unpriced, not worth it, nothing to flip, regex > 250, no data |
| `09-nav-icon.png` | Sidebar entry | Icon at 64px and nav size, inactive and active (Wild is the chosen one; the other three were candidates) |

## Inspecting beyond the PNGs
Open `Overview-prototype.dc.html` on the canvas (https://claude.ai/artifact/PTogG15PdG1WJYjbDqNHJf, page
"Handoff · build sources") and click through tabs, chips, tier buttons, Reset and Copy.
