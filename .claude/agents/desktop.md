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
- **Key `ssot`-driven effects on a primitive** (a joined string, as `valueTableKey`) unless they must
  run per delivery (the merc overlay's re-measure): every 3 s poll or nudge replaces `ssot.temple` and
  `.mercenary` whole, so derived objects are new. (Fires when: an effect, `{#each}` key or guard reads them.)
- **Delay Mercure reloads by a debounce plus jitter re-rolled per fire** (`refetchDelay()`): all
  clients get a publish at once, so a fixed offset aligns the herd. (Fires when: code reloads on a
  Mercure publish.)
- **Keep the Mercure reload windows in step**: `LabPage.svelte`'s `MERCURE_*`,
  `frontend/src/routes/lab/+page.svelte`'s `MERCURE_*` and `exchange/view.ts`'s `REFETCH_*` share one
  2–6 s window against the same publish. (Fires when: a change edits any of those constants.)
- **Format chaos with the surface's helper**: lab prices `formatPrice` (`$lib/price.svelte`, twin in
  `frontend/`; pass `NaN` when unknown so it prints `—`), exchange and temple their `formatChaos`
  (+ `formatGain`, `offerChaos`), listings `formatListingAmount`. (Fires when: UI renders chaos.)
- **An overlay using `formatPrice` calls `setDivineRate` itself** from its polled payload: each is
  its own WebView sharing no main-window module state, and a rate left at 0 means unknown and renders
  chaos. (Fires when: an overlay starts using `formatPrice`.)
- **Overlay routes set their own font**: `routes/overlay/+layout.svelte` imports only `tokens.css`,
  so text falls back to WebView2's serif; put `app.css`'s `font-family` on the root panel (as the
  mercenary overlay does). (Fires when: a route or component under `routes/overlay/` renders text.)
- **Normalize API variants before comparing them with UI ones**: `displayVariant()` (`lib/api.ts`)
  restores the `/0` the server drops (`1` → `1/0`); strip Dedication's corrupted `c` (`21/20c` →
  `21/20`). (Fires when: client code filters or compares a variant string from an API response.)
- **Sort comparators compare, never subtract**: `Infinity - Infinity` is `NaN`, which leaves the
  WHOLE order unspecified (V8 masks it as 0); sort a missing key last and a printed `Infinity` as
  the largest (`sortPlays`). (Fires when: a sort key can be `null`, `NaN` or `±Infinity`.)
- **Check foreign string keys as own keys** (`Map`, `Object.hasOwn(TABLE, key)` or `Object.keys`;
  not `key in TABLE` or `=== undefined`): an OCR, file, payload or pref key can be `constructor`
  (see `parseView`). (Fires when: code indexes an object literal with a string it did not write.)
- **Pair `overflow-x: auto` with `overflow-y: hidden`** on a horizontal scroller that must not
  scroll vertically: alone it computes `overflow-y` to `auto`, and Windows classic scrollbars
  then add a vertical bar only Windows shows. (Fires when: CSS sets `overflow-x: auto`/`scroll`.)

## Rust crate traps

- **Save a file whose last good contents must survive a crash by temp + rename**: write
  `path.with_extension("json.tmp")`, then `fs::rename` (as `settings::save`); a bare `fs::write`
  truncates first, so a crash leaves a file `load` drops. (Fires when: Rust persists state with `std::fs`.)
- **Serialize a shared file's load-mutate-save cycles under one lock**: an `AppState` mutex if every
  writer holds the app, else a `static Mutex<()>` (as `mercenary::sync::SYNC_FILE_LOCK`); unlocked,
  a later save drops an earlier mutation. (Fires when: several threads load, mutate and save one file.)
- **End any guard on a mutex `build_status` locks before `emit_status`** (a block, or a statement
  temporary): it relocks the status fields (`font_session` among them) and `std::sync::Mutex` is not
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

- **Judge latency on a release build** (`make desktop-release-windows`): `tauri dev` leaves the crate
  at opt-level 0 (pixel loops several times slower), and both builds write one unmarked `app.log`, so
  note which exe produced a timing. (Fires when: tuning a timeout, cadence or budget.)
- **After a desktop npm dependency change, `npm ci` into the `desktop_node_modules` volume, then run
  `make deps-check`** (`npm ls --all`; in `make qa`): a peer-range violation can bundle Svelte's server
  runtime, making every `untrack` a no-op with tests green. (Fires when: desktop npm deps change.)
- **Keep expected test values independent of the production path**: literals or a test-only oracle
  (not a re-call of a helper that path also calls, as `applyNumericFilters` calls `runInvestment`:
  the assertion then compares production with itself). (Fires when: a test computes an expected value.)
