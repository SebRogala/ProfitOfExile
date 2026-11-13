# CHECKLIST.md — acceptance contract for the top-bar account button (POE-362)

## ⛔ MUST NOT DROP
- [ ] **Only `registered` is filled.** `unknown` and `checking` are dashed hollow rings and never read as logged in (refs 03, 04).
- [ ] **Launch state:** the button shows `checking` until `/api/device/me` answers for this device id, then the real state — without the dialog ever being opened.
- [ ] **Failed `/me`** (network error, or a body without an `alias` key) shows `unknown`; a later good answer moves it to `registered` / `unregistered` without a restart.
- [ ] **After Identify succeeds** the button shows `registered` with the new alias's initial at once, taken from the identify response (not a `/me` re-read).
- [ ] **Server switch** returns the button to `checking`, then to the new server's answer.
- [ ] The button and the dialog's status row read **one** registration state and never disagree.
- [ ] **Ctrl+Shift+F11 no longer opens the dialog**; Ctrl+Shift+F12 still toggles debug.
- [ ] Placement: in `.right`, **after the rate chip, before the window controls**; the bar's drag area still works around it.
- [ ] Tooltip opens on hover **and** on keyboard focus; every state has its aria-label (README table).
- [ ] Nothing else in `TopBar.svelte`, `Sidebar.svelte` or the dialog's UI changes. Tokens only; copy exact (typographic quotes, "·", "…").

## Top bar — account button
- [ ] `registered`: filled `--text` disc, "G" in `--surface`; tooltip "Registered as “Gaming PC”" / "Role beta · click to open device identity" (ref 01, rest ref 05).
- [ ] `registered` with an empty role: line 2 reads "click to open device identity".
- [ ] `unregistered`: hollow `--text-muted` ring, person icon; tooltip "Not registered" / "This device has no alias on the server yet. Click to set one." (ref 02; focus ring ref 06).
- [ ] `unknown`: dashed ring, "?"; tooltip "Registration unknown" / "Server unreachable or outdated. Click to check again." (ref 03).
- [ ] `checking`: dashed ring, "…"; tooltip "Checking registration…" only (ref 04). No animation.
- [ ] Hover: `rgba(255,255,255,.06)` background, ring and glyph `--text` (refs 01–04).
- [ ] Focus: `1px solid var(--color-lab-blue)` ring, offset 1px (ref 06).
- [ ] Click / Enter / Space opens the existing Identify dialog.
