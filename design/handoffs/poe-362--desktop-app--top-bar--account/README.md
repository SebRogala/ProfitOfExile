# Desktop app → Top bar → Account button

**Handoff 1 — 2026-11-13** · ticket POE-362 (top bar: profile avatar replaces the identity shortcut)

## What's in this package
- **Build source:** `TopBar-prototype.dc.html`. It is not copied here (playbook `LOCAL.md`). The file is
  `design/canvas/desktop-app--shell/project/TopBar-prototype.dc.html`, rendered on the canvas
  https://claude.ai/artifact/DZJGz7YwMzrEKq95ZqVbm5, page "Handoff · build sources". Its `state` tweak switches
  registered / unregistered / unknown / checking; its `look` tweak switches rest / hover / focus.
- **Provenance only, do not build from it:** every other board on that canvas (round 1: placement A, the
  alias-text option, the states sheet, `Bar.dc.html`).
- `README.md` — the spec (this file).
- `CHECKLIST.md` — the acceptance contract.
- `fixtures.json` — the data.
- `reference_screens/` — ground truth, 6 PNGs (index: `reference_screens/INDEX.md`).
- `AGENT.md` — the implementer's protocol. Read it first.

Give the agent: _"Read `AGENT.md` and follow it exactly. Implement the frontend 1:1 against
`reference_screens/` using `fixtures.json`. Do not stop until every `CHECKLIST.md` box passes; log
anything you can't resolve in `NOTES.md`."_

Related package: `poe-284--desktop-app--top-bar--divine-rate/` added the rate chip this button sits beside.

## Spec changes vs ticket
Compared against: POE-362 as of 2026-11-13 and the intake brief `design/tasks/poe-362--desktop-app--top-bar--account.intake.md` (gitignored).
- Placement was open (A: next to the Settings gear, B: between the divine rate and the window controls) → **B**. Why: owner's pick on round 1.
- "Show alias or initial" → **initial only**; the alias is in the tooltip and the accessible name. Why: owner's pick on round 1 (the alias-text option was drawn and not chosen).

## Tokens
All from `desktop/src/tokens.css`; this is chrome, so it uses the legacy set like the rest of `TopBar.svelte`:
- Registered disc: fill and ring `--text`, letter `--surface` (12.04:1).
- Unregistered / unknown / checking ring and glyph: `--text-muted` (4.48:1 on `--surface`; the glyph is an aria-hidden icon, so the 3:1 non-text bar applies).
- Hover: `rgba(255,255,255,.06)` background on the button (the rate chip's hover), ring and glyph go to `--text`.
- Focus ring: `1px solid var(--color-lab-blue)`, offset 1px (the rate chip's ring; offset outward because the button is round).
- Tooltip: the existing `Tooltip` component, `position="below"`; its first line bold, second line `--color-lab-text-secondary`.

## Token and component changes
- Tokens: none.
- Components: none changed. `IdentifyDialog.svelte` keeps its content and flow (see Data shapes for where its registration state comes from).

## Screens
### Top bar (every page)
- `TopBar.svelte` gains one element: the account button in `.right`, **after the rate chip and before
  `.window-controls`**, inside the existing 12px gap rhythm. Nothing else in the bar moves.
- Button: a 24px-tall round `<button>` (1px padding) holding a 22px disc, 1.5px ring, 11px / 700 glyph, centred.
- The four looks:

| State | Disc | Glyph | Tooltip line 1 (bold) | Tooltip line 2 | aria-label |
|---|---|---|---|---|---|
| `registered` | filled `--text` | first character of the alias, upper-cased, in `--surface` | Registered as "{alias}" | Role {role} · click to open device identity | Account: registered as {alias} |
| `unregistered` | hollow, solid ring | person icon (stroke) | Not registered | This device has no alias on the server yet. Click to set one. | Account: not registered |
| `unknown` | hollow, dashed ring | `?` | Registration unknown | Server unreachable or outdated. Click to check again. | Account: registration unknown |
| `checking` | hollow, dashed ring | `…` | Checking registration… | (none) | Account: checking registration |

  The quotes around the alias are typographic (“ ”). With no role, line 2 of `registered` reads "click to open device identity" with the "Role … · " part left out.
- Only `registered` is filled. That fill (a lightness difference, not a hue) is what tells a logged-in device from every other state; `unknown` and `checking` must never look registered.
- The person icon: the prototype's inline SVG (circle r 2.8 at 8,5.5 and the shoulder path, 16×16 viewBox, stroke 1.6). If the app already has a person icon, use that and note it.
- No animation (checking is static).

## Interactions & state
- **Click, Enter or Space** opens the existing `IdentifyDialog` (same instance as today). Nothing else in the dialog changes.
- **Hover and keyboard focus** open the tooltip (refs 01–04 hover, 06 focus).
- The button is `-webkit-app-region: no-drag` and a `<button>`, so `startDrag`'s `closest('button, …')` skips it.
- **Ctrl+Shift+F11 is removed**: delete the F11 branch and its comment in `routes/(app)/+layout.svelte`. Ctrl+Shift+F12 (debug) stays.
- State comes from `/api/device/me` (see Data shapes):
  - `checking` until the first answer for this device id (including while there is no device id yet), and again after a server switch.
  - `registered` when `alias` is a non-empty string; `unregistered` when it is `null` or `""`.
  - `unknown` when the request failed, or the body has no `alias` key (a server from before aliases). Retries continue; the next good answer moves the state on.
- **After a successful Identify** the button turns `registered` with the new alias at once, from the identify response, not a re-read of `/me` (the server's device cache can serve the old alias for its TTL — `internal/server/handlers/device.go`, `DeviceMe`).

## Data shapes
See `fixtures.json`. One owner: the device module (`internal/device`, served by `GET /api/device/me` and `POST /api/device/identify`). Both endpoints exist and already return `alias`; **no backend change**.

Today two readers parse `/me`: `stores/entitlements.svelte.ts` (role, channel, features; it **drops `alias`**) and `IdentifyDialog.svelte`'s own `loadRegistration()`. The top bar needs the registration state at launch, without the dialog open. Build one registration state (`checking | registered | unregistered | unknown` + `alias` + `role`) fed by the store's existing load/retry/reset chain, and make the dialog's status row read the same state, so the button and the dialog cannot disagree. Where it lives is the codebase's call.

## Assets
None new. The person icon is inline SVG (or an existing app icon).

## Constraints
- Nothing else in `TopBar.svelte`, `Sidebar.svelte` or `IdentifyDialog.svelte`'s UI changes.
- Dark only; WCAG AA; the button is keyboard-reachable with a visible focus ring.
- Out of scope: a profile page, avatar upload, web app changes, any change to the dialog's flow.

## Open decisions — questions, not requirements; do not invent answers
- **Alias whose first character is not a letter** (digit, symbol, emoji): interim — show that character as it is (upper-casing changes nothing); a whitespace-only alias cannot be saved (the dialog trims and rejects empty).
- **Tooltip anchor:** like the rate chip, the shared `Tooltip` anchors left and clamps 8px inside the window, so at 1024px the popup's right edge lands at x 1016 (refs 01–04, 06). Accepted for the rate chip (POE-284 NOTES §7); assumed the same here.
