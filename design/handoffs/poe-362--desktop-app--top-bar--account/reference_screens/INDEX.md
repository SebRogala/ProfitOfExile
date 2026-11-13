# Reference screens — index

- **Desktop:** 1024 × 110 (the top bar plus room for its tooltip, drawn open where the look is hover or focus).
- **Captured:** 2026-11-13 with headless Chrome from `TopBar-prototype.dc.html` (one copy per `state` × `look`), same day as the prototype's last change.
- **Fixture state:** `deviceMe.registered` (alias "Gaming PC", role beta) for the registered refs.

## `desktop/`
| File | state · look | What to match |
|---|---|---|
| `01-top-bar--registered.png` | registered · hover | Filled disc "G" after the rate chip; tooltip two lines |
| `02-top-bar--unregistered.png` | unregistered · hover | Hollow ring, person icon; tooltip |
| `03-top-bar--unknown.png` | unknown · hover | Dashed ring "?"; tooltip |
| `04-top-bar--checking.png` | checking · hover | Dashed ring "…"; one-line tooltip |
| `05-top-bar--registered-rest.png` | registered · rest | Disc at rest, no tooltip |
| `06-top-bar--unregistered-focus.png` | unregistered · focus | Blue focus ring, tooltip open on focus |

## Inspecting beyond the PNGs
Open `TopBar-prototype.dc.html` on the canvas (page "Handoff · build sources") and switch its `state` and `look` tweaks. The `look` tweak and its `.is-hover` / `.is-focus` classes are capture scaffolding: build real `:hover` / `:focus-visible`.
