---
name: desktop
description: Use for ProfitOfExile's Tauri 2 desktop application, including Rust commands/state, Svelte 5 UI, OCR, Client.txt, trade integration, settings, and Windows overlays. Requires the desktop component registry and the maintained overlay guide before touching those areas.
---

# Desktop agent

Read `AGENTS.md` first. Then read:

- `desktop/src/lib/README.md` for components, stores, navigation, OCR, and desktop
  conventions;
- `docs/OVERLAY-GUIDE.md` before any overlay, positioning, click-through,
  settings, focus, or multi-window change;
- the affected Rust/Svelte code and tests, which remain authoritative.

Use Svelte 5 runes and existing CSS custom properties. Main views remain mounted
and switch through the navigation store; overlay routes are separate windows.

Check the component registry before adding UI, and register genuinely reusable
components. One-off markup is allowed when extraction would not create reuse or
clarity.

Desktop state uses several mechanisms: Rust events, commands, standard/async
mutexes, atomics, filesystem notifications, and polling/reconciliation loops.
Follow the existing owner of the specific state path; do not impose a universal
“events only” or “all state behind Mutex” rule. Persist and emit only when the
command's contract requires persistence or notification.

For overlays, apply the guide's capability, physical-coordinate, move-not-
recreate, settings-survival, click-through-type, and error-visibility guards.
Treat its runtime-earned WebView2/Win32 observations as regression constraints
until they are explicitly superseded with evidence.

## Game-client boundary

- **Read the screen and Client.txt only.** Design any in-game action around the player doing it
  (never move the cursor or send clicks, keys or other input to the PoE client: GGG's ToS bans it).
  (Fires when: a change touches the game window via capture, OCR, overlays or Win32 input APIs.)

## Webview and Svelte traps

- **Read store-backed state through `$derived`** (`const apiBase = $derived(getApiBase())`): pages
  mount once, before the Rust status carrying `server_url`, so a script-init `const` pins every URL
  to the fallback base all session. (Fires when: a component's top-level `const` reads a store.)
- **Call a mount `$effect`'s loader as `untrack(() => loadAll())`** and re-fetch from the change
  handler: a reactive read inside the loader (`isDedication` in `LabPage.svelte`) re-runs the whole
  load on every toggle. (Fires when: an `$effect` calls a function that reads reactive state.)
- **Key `ssot`-driven effects on a primitive** (a joined string or scalar of shape, not moving text,
  as `valueTableKey`): each 3 s poll or `ssot-changed` nudge replaces `ssot.temple`/`.mercenary`
  whole, so derived objects are new. (Fires when: an effect, `{#each}` key or guard reads them.)
- **Delay Mercure reloads by a debounce plus jitter re-rolled per fire** (`refetchDelay()`): all
  clients get a publish at once, so a fixed offset aligns the herd. (Fires when: code reloads on a
  Mercure publish.)
- **Change `LabPage.svelte`'s `MERCURE_*` constants together with
  `frontend/src/routes/lab/+page.svelte`'s**: the web lab page mirrors the desktop reload timing.
  (Fires when: a change edits either file's `MERCURE_*` constants.)
- **Write every chaos amount through `formatPrice`** (`$lib/price.svelte`; change its `frontend/`
  twin with it): pass `NaN` when unknown (NO_DATA/NO_BASE) so it prints `—`, never `0`/`0c`; trade
  listings keep the seller's currency. (Fires when: UI renders a chaos amount, price or ROI.)
- **Overlays call `setDivineRate` themselves** from their polled payload: each is its own WebView
  sharing no main-window module state, and a rate left at 0 means unknown and renders chaos.
  (Fires when: an overlay renders a chaos amount.)
- **Overlay routes set their own font**: `routes/overlay/+layout.svelte` imports only `tokens.css`,
  so text falls back to WebView2's serif; put `app.css`'s `font-family` on the root panel (as the
  mercenary overlay does). (Fires when: a route or component under `routes/overlay/` renders text.)
- **Normalize API variants before comparing them with UI ones**: `displayVariant()` (`lib/api.ts`)
  restores the `/0` the server drops (`1` → `1/0`); strip Dedication's corrupted `c` (`21/20c` →
  `21/20`). (Fires when: client code filters or compares a variant string from an API response.)
- **Sort comparators compare, never subtract**: `Infinity - Infinity` is `NaN`, which leaves the
  WHOLE order unspecified (V8 masks it as 0); sort a missing key last and a printed `Infinity` as
  the largest (`sortPlays`). (Fires when: a sort key can be `null`, `NaN` or `±Infinity`.)
- **Look up foreign string keys in a `Map` or `Object.keys(TABLE).includes(key)`** (not `key in
  TABLE` or an `=== undefined` guard): an OCR, file, payload or pref key can be `constructor` (see
  `parseView`). (Fires when: code indexes an object literal with a string it did not write itself.)
- **Pair `overflow-x: auto` with `overflow-y: hidden`** on a horizontal scroller that must not
  scroll vertically: alone it computes `overflow-y` to `auto`, and Windows classic scrollbars
  then add a vertical bar only Windows shows. (Fires when: CSS sets `overflow-x: auto`/`scroll`.)

## Rust crate traps

- **Save state files by temp + rename**: write `path.with_extension("json.tmp")`, then `fs::rename`
  (as `settings::save`); a bare `fs::write` truncates first, so a crash leaves a file `load` drops.
  (Fires when: Rust saves a state, settings or index file with `std::fs`.)
- **Serialize a shared file's load-mutate-save cycles** under one process-wide `static Mutex<()>`
  (as `mercenary::sync::SYNC_FILE_LOCK`), or a later save silently drops an earlier mutation.
  (Fires when: several threads load, mutate and save one file.)
- **End every `AppState` guard's scope before `emit_status`** (a block, or a statement temporary):
  `build_status` locks the status mutexes (`font_session` among them) and `std::sync::Mutex` is not
  re-entrant, so a live guard deadlocks. (Fires when: `emit_status` runs while one is in scope.)
- **Measure OCR geometry on the real engine**: `Windows.Media.Ocr` line boxes are cap-height, so
  take pitch, gap and height-ratio thresholds from a Debug capture's `ocr-lines.json` and pin them
  with a real-engine fixture as in `temple/panel.rs`. (Fires when: tuning an OCR-box threshold.)

## Verification and tooling

Run desktop Svelte checks/unit tests and focused Rust tests. Use `make
desktop-check` and `make desktop-test` for broad Rust verification.

Format TS/Svelte by hand to match the surrounding code (tabs, single quotes, no
trailing commas) and verify formatting with `npm run check` plus reading the
surrounding code. Leave prettier, and any formatter not in desktop/package.json,
off this repo: no prettier dependency or config exists, and an
`npx prettier --write` will install itself and reflow whole files.

- **Judge latency on a release build** (`make desktop-release-windows`): `tauri dev` compiles the
  crate at opt-level 0, so its pixel loops (anchor NCC, OCR crop prep) run several times slower;
  check which build wrote an `app.log` first. (Fires when: tuning a timeout, cadence or budget.)
- **Run `make deps-check` after any npm dependency change** (in `make qa`; reinstall the desktop
  `node_modules` volume if it lags `package.json`): a peer-range violation can bundle Svelte's
  server runtime, making every `untrack` a no-op with tests green. (Fires when: npm deps change.)
- **Pin expected test values as literals** (never a re-call of a helper production also calls, e.g.
  `applyNumericFilters` against the Investment cell's function): such an assertion compares
  production with itself and cannot fail. (Fires when: a test's expected value comes from a helper.)
