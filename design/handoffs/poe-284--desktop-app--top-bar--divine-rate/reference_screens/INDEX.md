# Reference screens — index

- **Desktop:** 1024 × 200 (the top bar plus room for its tooltip, which is drawn open).
- **Captured:** 2026-10-10 with headless Chrome from `TopBar-prototype.dc.html` (`state` = ready / cold / stale).
- **Fixture state:** rate 360.07, hour 2026-09-09 23:00 UTC, updated 6 min ago.

## `desktop/`
| File | State | What to match |
|---|---|---|
| `01-top-bar--ready.png` | ready | Chip at the right before the window controls; tooltip lines |
| `02-top-bar--no-rate-yet.png` | cold | "—" muted; "No divine rate yet" tooltip |
| `03-top-bar--stale.png` | stale | Value and "STALE" in warning colour; stale tooltip |

## Inspecting beyond the PNGs
Open `TopBar-prototype.dc.html` on the canvas (page "Handoff · build sources") and switch its `state` tweak.
