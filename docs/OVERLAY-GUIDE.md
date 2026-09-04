# Tauri v2 Overlay Guide

> **Status: Current implementation guide.** Last verified against desktop code:
> 2026-09-02. Runtime observations that are valuable but not statically provable
> are labelled as such. Earlier debugging detail is preserved in
> [the historical overlay notes](history/overlay-debugging-notes.md).

Read `desktop/src/lib/README.md` for desktop components, stores, navigation, and
OCR lifecycle. The proposed Rust-owned navigation contract is specified in
tracker item `POE-88` (LabCompass fidelity restoration and overlay SSOT).

Switchable background modules are a separate mechanism with its own recipe: see
the module doc of `desktop/src-tauri/src/modules.rs`. The four overlay `enabled`
flags and `lab_overlays_enabled` documented here are NOT modules.

## Non-negotiable regression guards

1. **Capabilities:** every overlay and configuration-window label must appear in
   `desktop/src-tauri/capabilities/default.json`. Missing labels make Tauri APIs
   unavailable to that window.
2. **Physical persistence:** `outerPosition()` and `outerSize()` produce the
   physical coordinates persisted in settings. Restore existing windows with the
   Rust `move_overlay` command, which applies `PhysicalPosition` and
   `PhysicalSize`.
3. **Logical construction:** `WebviewWindow` constructor dimensions are logical.
   Convert persisted physical width/height with Tauri `scaleFactor()` for initial
   construction, then apply exact `PhysicalPosition`/`PhysicalSize` in
   `tauri://created`. Do not use `window.devicePixelRatio` for overlay geometry.
4. **Move instead of recreate:** reposition a live overlay with `move_overlay`.
   Avoid destroy/recreate cycles for position updates; Tauri label cleanup is
   asynchronous and has produced “already exists” failures.
5. **Settings survival:** overlay settings are not rebuilt from `AppState`.
   `persist_overlay_settings` must copy every persisted overlay field, and
   `test_overlay_settings_survive_persist_cycle` must cover additions. WIDGET
   placements are the exception and must stay out of that function:
   `Settings.widgets` IS owned by an `AppState` mutex, so it travels through
   `from_state`, and carrying the file's copy forward would undo the write
   `set_widget_geometry` just made. `settings.rs` has a test that fails if it is
   ever added there.
6. **Error visibility:** log failed invoke, window, and event operations. Do not
   silently swallow failures in overlay paths.

## Overlay types

EVERY overlay calls `set_overlay_clickthrough(label)`, and that call does two
things: it installs `WS_EX_TRANSPARENT`/`WS_EX_NOACTIVATE` behavior on the
window, and it REGISTERS the window with the shared `WH_MOUSE_LL` hook
(`desktop/src-tauri/src/overlay_hook.rs`). Registration is not about being
interactive — it is what lets the hook repair the `WS_EX_TRANSPARENT` WebView2
strips off when it rebuilds child windows. The hook is installed by the first
window to register and torn down when the last one is destroyed.

Which clicks a window CLAIMS is a separate, per-window declaration made by the
window's own page:

- `set_overlay_hot_rects(label, rects)` — window-relative rectangles in
  PHYSICAL pixels (`getBoundingClientRect()` × the window's Tauri
  `scaleFactor()`; the conversion and the change test are in
  `desktop/src/lib/overlay/hot-rects.ts`). A click inside one is consumed and
  re-emitted to that window as `overlay-click {label, x, y}`; everything else
  reaches the game. Declaring nothing — which is what compass, path-strip,
  timer and the merc strip do — makes a window display-only. Listen with
  `getCurrentWebviewWindow().listen('overlay-click', …)`; a bare `listen()` from
  `@tauri-apps/api/event` registers for the `Any` target and a labelled
  `emit_to` does not match it (tauri 2.10.3 `manager/mod.rs:602-628`).
- `set_overlay_has_content(label, has_content)` — a window that is drawing
  nothing claims nothing, whatever it declared.
- `set_overlay_config_mode(label, on)` — while on, the window is genuinely
  interactive (`set_ignore_cursor_events(false)`) and the hook leaves it
  entirely alone: it neither repairs `WS_EX_TRANSPARENT` nor intercepts.

Outside config mode a window's `WS_EX_TRANSPARENT` is never cleared, so a
button appearing and disappearing is a hot rect being declared and withdrawn,
never a transparency flip.

The comparator is the interactive one today: its page declares the button
column (and the trade-queue row while that shows) and maps the emitted
coordinates with `elementFromPoint` + `data-action`. Buttons must be inside a
declared rect and expose that `data-action` metadata; they no longer have to
align with the window's right edge, because the claim is the element's own
rectangle rather than a band measured from the edge.

The hook pairs button-up with button-down: it consumes a release only when it
consumed the matching press, so a drag that started on the game keeps its
release. Two overlapping windows are resolved by **the most recently shown
one** (POE-239) **(merged, not yet run on Windows — the POE-239 smoke bullet is
the acceptance)**: the hook cannot see z-order, so it uses the two show signals
Rust does receive — the window's registration, and the EDGE from empty to
drawing in `set_overlay_has_content(label, true)` — and the later of the two
wins. Only the false→true edge counts, never a repeat of `true`: the widget
host sends the flag when emptiness flips, but the comparator re-asserts it from
a `$effect` on every data change, so a repeat would hand the comparator a fresh
top-of-stack claim on every price tick and let it out-rank a window the user had
just opened. Registration order is the tiebreak and one
monotonic counter cannot produce a tie, so it is there only to keep the answer
total. The rule this replaces was first-registered-wins, which handed a shared
click to the window built FIRST, i.e. the one most likely to be underneath.
`set_overlay_hot_rects` logs one line per pair per registration when a window's
rects land on another registered window's, so a click going to the window the
user did not mean is something the log already named. Hot rects are
window-relative and the two windows that declare them never share an origin
(monitor-sized widget host at 0,0; a small comparator wherever the user put it),
so both sides are translated by their window's cached rect and compared in
SCREEN space. A pair is skipped, silently and without being marked reported,
while either window's rect is unknown — a page declares its rects during the
~1 s before its HWND resolves, and the next declaration after it does names the
pair. The command routes the returned lines through `app_log`, not `log::warn!`:
`env_logger` is initialised with no filter and `RUST_LOG` is unset, so anything
below Error never reaches `app.log`.

**Config mode is exclusive by construction, so no z rule applies to it.**
`hit_test` skips a config-mode window because the webview is taking those
clicks natively — the hook has nothing to award, not a lower priority to
assign. Do not invert that skip into "config mode wins": consuming the click
would re-emit it as an `overlay-click`, which is the one thing the window being
arranged is not listening for.

The capture/configuration overlay is deliberately interactive and has different
drag, resize, save, and cancel behavior. Treat it as a third type.

Temple (`temple`) and merc verdict (`mercenary`) are overlays COUPLED TO A
MODULE FLAG rather than to an overlay setting: the module toggle creates and
destroys the window, and `desktop/src/lib/overlay/module-lifecycle.ts` orders
those transitions so a fast off→on→off cannot strand a transparent
always-on-top window. The flag is not quite the whole desired state for a
window with WIDGETS: a live config session is ORed into it so the user can
arrange positions with the module off (the ordering contract below), which is
the one thing that raises such a window without any module work running. They
still appear in the Rust focus poller's game-focus show/hide list and in
`set_debug_mode`'s force-show branch. Persisted geometry is independent of the
coupling: the merc strip has `mercenary_overlay`, and the temple window has
none because it IS the game monitor (below).

The merc strip's **on-screen lifecycle** (owner decision, 2026-09-01) is: shown
for as long as a recruit window is being worked — `scanning` (the burst after
a voice line or Scan now), `live` (reading), `done` (read paused) — and for
`LINGER_MS` (4 s) after the module goes `idle`, whether that idle is "window
gone" over a retired capture or "waiting" over nothing; then the panel clears
and the whole overlay disappears. The gate is `overlayShown` in
`desktop/src/lib/mercenaries/overlay-view.ts`; the route owns only the clock.
The full contract is in `desktop/src/lib/mercenaries/README.md`.

The merc strip's **height follows content; width and position are persisted**
(owner decision, 2026-08-25). Its route observes its own panel with a
`ResizeObserver` and calls the Rust `fit_overlay_height` command, which converts
the measured CSS height with the WINDOW's own `scale_factor()`, clamps it to the
monitor work area so the strip can never grow over the taskbar, and re-applies
the position along with the size — a resize can disturb position, so the guard
belongs on every call and not just on startup. Width and position are never
touched by that path. `mercenary_overlay.height` is still written (Settings
saves the whole rect) and is then ignored by the real window, which refits on
first paint; the constructor seed in `desktop/src/lib/overlay/overlay-defaults.ts`
only decides what that first frame looks like. A shipped height was wrong twice
over: it clipped the last row, and it was reasoned in CSS pixels while being
applied as physical ones, so it clipped worse the more the display scaled.
Settings → Overlay Positions configures the merc row for **position and width
only**; its config window is given the live overlay's height when one is running
and the seed when the module is off, and that height is LOCKED with
`setSizeConstraints` (width stays draggable — Tauri's `resizable` flag has no
per-axis form, and the guide detail line ellipsises, so width is a real
setting).

`fit_overlay_height` re-asserts click-through after every resize —
`set_ignore_cursor_events(true)` plus `set_noactivate` on Windows, both
idempotent. This is not belt-and-braces: WebView2 strips `WS_EX_TRANSPARENT`
when it creates or updates child windows, and the only thing that repairs it is
the `WH_MOUSE_LL` hook. The hook now repairs every REGISTERED window, the merc
strip included — before the registry it tracked one HWND, the comparator's, and
the strip was simply unprotected. The re-assert stays anyway, because the repair
is driven by mouse events over the window: a resize that rebuilt WebView2's
children would otherwise leave the strip opaque to the mouse until the cursor
happened to cross it. While it is opaque, clicks stop reaching the game, and a
click landing on the strip takes focus, drops `game_in_foreground` and stops the
capture loop.

For the merc overlay, click-through is a correctness requirement, not a
preference. The capture loop reads the screen only while the game is the RAW
foreground window (`AppState.game_in_foreground`), while overlay visibility
follows `game_focused`, which is HELD over our own windows. The two reads are
deliberately never unified — so a click that focused the verdict overlay would
drop the raw flag and stop the loop producing the verdict on screen.

**Creation fails loudly if click-through cannot be applied.**
`set_overlay_clickthrough` still spends ~1 s waiting for the WebView2 HWND — it
is not available sooner — but it now AWAITS that work and returns
`Result<(), String>`. Four things can fail it: the label is gone by the time the
wait elapses (`window-gone`), the HWND never becomes available, the belt below
finds the style did not take, or `set_ignore_cursor_events(true)` refuses. The
first three are the live signals; the fourth is close to unreachable, because
tao's Windows implementation returns `Ok` unconditionally and Tauri only errors
when the event loop is already gone — it is kept because "close to unreachable"
is not "cannot happen", and swallowing it is the defect this command was fixed
for. Registration with the hook is still attempted on every path that has an
HWND, because a window the caller decides to keep must stay repairable.

`window-gone` carries a machine-readable prefix, and it is the ONE ordinary
failure: an overlay toggled off inside the 1 s wait looks exactly like this and
has nothing left to catch a click. The caller reports it as an info line rather
than a warning (`$lib/overlay/clickthrough-report.ts`); every other reason means
a live window that may be opaque to the mouse. The literal is pinned at both
ends, in `lib.rs`'s tests and in `clickthrough-report.test.ts`.

The belt is a Rust-side read-back: after applying, the setup asks the window
what `WS_EX_TRANSPARENT` it actually carries (`overlay_hook::is_transparent`)
and fails if it reads back missing. Two things make it a read-back rather than a
race, and both are load-bearing:

- **`window.hwnd()` is the barrier.** `set_ignore_cursor_events` posts its work
  to the event loop and returns; `hwnd()` is a blocking round-trip to that same
  loop, so by the time it answers the style call has been serviced. The belt
  sits after that call for this reason. Moving it earlier, or making the lookup
  non-blocking, breaks the ordering silently.
- **It reads three times, 20 ms apart.** WebView2 is still building children
  underneath, and child-building is what strips `WS_EX_TRANSPARENT` in the first
  place, so a verdict that destroys the user's overlay must not rest on one
  sample. The passing path costs one read; only the failing path pays 40 ms.

An unreadable style is UNKNOWN, not missing — `GetWindowLongW` answers 0 both
for "no extended styles" and for an error, the same rule the hook's own repair
uses — so a Win32 hiccup does not tear a working overlay down.

The 500 ms `set_noactivate` re-apply stays fire-and-forget on its own thread: it
is a repair of a style WebView2 may strip while it builds children, not a gate.
It resolves the LABEL again rather than reusing the handle it was given, because
callers now answer a failure by DESTROYING the window and Win32 recycles HWND
values — a raw handle half a second old can name someone else's window. A
destroyed overlay leaves the hook's registry through the `Destroyed` arm of
`on_window_event`, which also tears the hook down when it was the last one.

What the caller does with the failure differs by overlay type, and both answers
are in `routes/(app)/+layout.svelte`:

- The two MODULE-COUPLED windows (temple, merc) destroy the half-built window
  and report the creation failed, which `module-lifecycle.ts` retries with its
  bounded budget. Half-built is worse than absent here — the temple window is
  the size of the monitor, so one that never became click-through swallows
  every click on the screen, and a click on the merc strip also takes focus,
  drops `game_in_foreground` and stops the capture loop.
- The four LAB overlays (comparator, compass, path strip, timer) REPORT and
  keep the window: they are small, user-positioned rectangles the user just
  switched on, and destroying one would read as a toggle that does nothing.
  Note the split is by OWNER, not by size — the merc strip is small too and is
  in the group above, because it is coupled to a module flag and a click on it
  stops the capture loop. Their call is deliberately not
  awaited before the "hide if the game is not focused" step either — that hide
  must not wait a second on a window the user is not looking at — so the
  failure arrives on the promise's `catch`, in the app log as well as the
  console.

The window is still INTERACTIVE for that ~1 s, which no amount of reporting
closes: `focus: false` on the constructor stops the window activating itself,
but a click landing in it during that second still hits the panel. What changed
is that a setup which never completed is no longer indistinguishable from one
that did.

## Widget overlays

A module may instead open ONE fullscreen, click-through window over the game's
monitor and place small panels — WIDGETS — inside it. The temple is the first
(POE-225); the lab windows and the merc strip are not migrated.

- The window is the GAME monitor (POE-237). `routes/(app)/+layout.svelte` asks
  Rust's `get_game_monitor` — which the focus poller answers from the PoE
  window's own HWND — and matches it against an `availableMonitors()` entry by
  POSITION, because the two enumerations do not share an id space; the rule is
  `overlay/monitor-choice.ts` and it falls back to `primaryMonitor()` (then
  `currentMonitor()`) on every failing path, which is what shipped before
  POE-237. It constructs at the monitor's logical size and applies the exact
  `PhysicalPosition`/`PhysicalSize` in `tauri://created` — guard 3, with that
  monitor's own scale factor. It has no persisted rect, is not resizable, and is
  NOT in `RESIZABLE_OVERLAY_LABELS`: `fit_overlay_height` would shrink the canvas
  every widget's persisted coordinate is measured against. When the game moves to
  another display Rust emits `game-monitor-changed` to the main window and the
  layout REBUILDS the window there through the driver's own off/on **(merged, not
  yet run on Windows — smoke item 10 is the acceptance)** — guard 4's
  "move, not recreate" is about repositioning within one display, and a different
  display is a different canvas: different size, different scale factor,
  different coordinate space for every widget in it. Two cases do NOT rebuild.
  A notice naming the display the window was already BUILT on only teaches it
  the id, so the layout records the id and stops. The recorded id is the display
  the window WENT ON, not the one Rust named: a build that fell back to the
  primary — nothing had seen the game window yet, or the webview could not list
  the display Rust named — records `0`, because recording the game's id there
  would make the next notice's id guard return early and strand the overlay on
  the wrong screen. And a live widget-config session
  DEFERS the rebuild to its end: a rebuild is a destroy, and that window is the
  surface the user is dragging widgets on — taking it down mid-session drops
  them into a window that no longer exists and leaves Settings on
  `Configuring…` waiting on a destroyed host. The deferral leaves the recorded
  display untouched, so the next notice still rebuilds; and when the session was
  the only thing holding the window up, its end tears the window down and the
  next build asks `get_game_monitor` afresh.
  **The question is asked a SECOND time, once the build has settled** (POE-245,
  `reconcileTempleMonitor` over `monitor-choice.ts`'s `gameMonitorAfterBuild`
  and `builtOnStaleMonitor`). The first ask happens before the constructor and
  the answer can move while the window is still being built —
  `set_overlay_clickthrough` alone spends ~1 s waiting for the WebView2 HWND.
  Rust does emit a `game-monitor-changed` for that move, but the notice handler
  cannot act on one that arrives before the driver has settled the create
  (`built()` is false, there is no window to rebuild), and
  `remember_game_monitor` emits only on a CHANGE — so that notice is the only
  one there will ever be. It is therefore RECORDED rather than dropped, and the
  reconcile consumes it. Three details are load-bearing. The corner it compares
  against is a LOCAL captured where the build assigns it, not the module
  variable, because the notice handler writes that variable in between — reading
  it back is how the first version of this fix defeated itself. The reconcile
  runs on a `setTimeout(…, 0)` and not a microtask, because the rebuild it may
  ask for is the driver's own `setDesired(false)`/`setDesired(true)`, which is
  ignored while `pending` is still `'create'`. And a stale answer is NOT
  reported as a failed creation: the window is built and usable, and three stale
  answers would exhaust the create budget and leave the module with no overlay
  at all. On the non-stale path the reconcile re-arms both guards, so the
  "already the right canvas" comparison stays live. An unknown game monitor is
  never stale — that is the pre-POE-237 primary fallback — and neither is a
  build that KNEW where the game was and could not go there, which is POE-237's
  own soft failure and would rebuild onto the primary every time. The reconcile
  does NOT honour the config-session deferral: the deferral works for a notice
  because the next notice still rebuilds, and here there is no next notice.
  Nothing is being dragged that early, and a session opening across the rebuild
  waits it out inside `WIDGET_WINDOW_WAIT_MS`.
- The widgets are declared in
  `desktop/src/lib/overlay/widgets/widget-registry.ts`, keyed
  `"<module>.<widget>"`, with shipped defaults in CSS pixels; their placements
  are persisted in PHYSICAL, window-relative pixels in `Settings.widgets`. That
  unit is also capture pixels, because the window and every capture are the same
  monitor — the game's, by construction on both sides since POE-237 (the window
  is built on it, `capture::capture_screen` grabs it, and `ssot.screen` carries
  its id and origin) — so a user-placed widget and a game-anchored one need no
  conversion between them beyond the window's own scale factor.
- **The shipped widget list.** Three, all the temple's:
  `temple.advice` — the KILL CALLOUT (POE-244), `anchored`, a box carrying the
  architect's name and one reason, placed level with the block the advisor chose
  and just outside the game's own side panel; `temple.door` — the ROOM
  WIDGET, user-placed and persisted, the same isometric rectangle the side panel
  draws, with every corridor the read settled in the game's own colours (green
  open, red closed), the advisor's door bigger and purple, the door a SECOND
  Stone of Passage would buy in the same purple at half opacity and a radius
  between the two, and BOTH kills marked by a cyan glyph on their own architect
  icon spots inside the room — the block the advisor did NOT choose at a quarter
  opacity (POE-248); and `temple.waiting` — the WAITING NOTICE (POE-249), one
  line saying the module heard Alva's start phrase and the layout sheet is not
  up yet. It is PLACEABLE like the door — the ordinary pair of Settings
  controls, a Show checkbox and a persisted rectangle — and it ships top-centre,
  (830, 16) 260×40 on a 1920-wide host, which is measured clear of the panel's
  OCR crop rather than eye-level. It is the only surface that exists before
  anything has been read, and it is gone the moment there is a board.
  (NOT SHIPPED, tracked in POE-249: the kill callout is due to be replaced by
  per-offer boxes drawn on the architect blocks themselves. Until that lands
  this list is what the registry declares.)
  **Faint is the alternative** is one rule across that widget: the conditional
  door and the unchosen kill are both drawn and both dimmed, so everything at
  full strength is a thing to do now. It is the read that check items below
  test, not the opacity numbers.
  **What the temple overlay deliberately stopped showing in POE-244**, all of it
  still on the Temple page: the reader's status lines (`reading…`, `between
  rooms — layout only`), the top gamble and its risk %, the unread-plate badge,
  the marker-fallback notice in full, and the advisor's `warnings` list. The
  overlay is for seeing; the page is for reading. Two things did NOT move —
  `doorWarning()`, on the door widget, because it says do not act on the shape
  it sits under and that widget is the one still on screen while the player is
  acting; and the `leaveMap` banner, which is a decision about the map rather
  than a reading.
  **Retired again in POE-248**, after the first live session: the callout's
  ARROW (owner: no arrows anywhere — placement points, and the room widget's
  glyph is what survives the panel closing), the room widget's two text lines
  (`KILL <architect> → <room>` and `open <edge>`), and its grey UNSETTLED seals.
  A corridor the read could not settle is now drawn nowhere at all, and
  `doorWarning()` is what says so in words.
  The red CLOSED seals were retired with them and **came back the same day**,
  after the owner checked the build in game: hiding them was the wrong half of
  *"the seals add chaos"* — the grey was the chaos, and a room drawn with only
  its open walls is not the room the player is standing in. `sealVisible()`
  withholds one kind of corridor now, and one only.
  `temple.board` (the lattice redrawn over the game, POE-225) is RETIRED: the
  board is already on screen behind the window, and the copy cost space that has
  to be kept clear of the module's own OCR crops. Its persisted rectangle is left
  INERT rather than removed — Rust does not validate widget ids against the
  frontend registry (`set_widget_geometry` says why) and the host looks
  placements up by spec, so a row nothing declares is never read. **The same is
  true of `temple.advice`'s stored RECTANGLE**, which every machine that arranged
  the widgets before POE-244 also has: the widget still exists but is now
  anchored, so its `x`/`y`/`width`/`height` are never consulted again. Its
  `visible` flag is NOT inert — the Show checkbox still writes it and the host
  still honours it, which is the one thing a stored row for an anchored widget is
  still for.
- **Two kinds of widget, and the host draws them differently.** A PLACEABLE
  widget gets a positioned box from `placementFor`, a Settings row, a persisted
  rectangle and a config-mode frame. An `anchored` one gets none of those: the
  host renders it through a second `anchored` snippet into a layer the size of
  the whole window, and the MODULE positions its own content inside that. The
  host cannot place one — where a callout goes is a function of where the game
  drew the thing it points at — so what it contributes is the window, the frame
  (`HostFrame`: the scale factor and the host box, both already resolved here, so
  the module does not ask the window a second time), the same `data-hot` claim,
  and the exclusion from Settings' POSITION column and from placement
  persistence. Anchored widgets are NOT drawn in config mode: there is nothing to
  arrange, and a box beside the red frames over the dimmed host would say
  otherwise. They DO keep a Settings row and a Show checkbox — an anchored widget
  the user cannot switch off is the one overlay surface with no control at all —
  so `overlayGroups()` lists them with `placeable: false`, which is what tells
  the page to print `placed by the game` where a rectangle would go, and the host
  honours `visible` in the anchored branch as well as the placed one.
- **A module may override an unconfigured widget's shipped position**
  (`defaultsFor`, POE-244). The registry's numbers are fixed CSS px, which is
  right for a widget whose default has nothing to do with the game; the temple's
  door diamond is the other case — it ships "beside the game's own diamond, below
  the panel, and clear of every read region", which depends on a board the host
  knows nothing about. The hook answers with a replacement `defaults` and is used
  exactly where `spec.defaults` would have been, for an unstored widget's position
  and for what config mode seeds one at. A STORED placement outranks it: the user's
  placement is never overridden by a default, game-anchored or not. The
  consequence for a widget nobody has placed is that it FOLLOWS the panel as the
  panel moves, which is what "ships next to the game's diamond" has to mean.
  **The host REMEMBERS the last answer** (`lastDefaults`), and that is not a
  cache for speed. A widget-config session raises the window without starting the
  module's work (POE-241), so there is no board while the user is arranging and
  the hook answers null — config mode would seed the frame from the registry's
  fixed number while in play the widget sits beside the game's diamond, and Save
  would persist a position the widget has never been in. The remembered answer is
  the seed instead. When there has been no board at all this session the registry
  number is still the seed, and the config bar says so rather than pretending.
- **Nothing may cover a read region** (POE-244). The temple OCRs the side panel,
  the budget line and the panel's diamond, both boxes on every plate, and the beam
  patch at every corridor midpoint — 42 rectangles on a full board, published as
  `layout.rois` from `temple::run::read_rois`, which is the ONE builder over the
  six functions that own them (`run::panel_rect`, `run::diamond_rect`,
  `run::remaining_rect`, `panel::name_strip` + `panel::numeral_box` unioned per
  plate, and `Lattice::edge_midpoint` with `lattice::PATCH_HALF`). A surface drawn over one is OCR input the app
  wrote itself: a confident, wrong board with nothing anywhere reporting a
  failure. `overlay/widgets/widget-avoid.ts`'s `avoidRects` is the rule — the
  nearest position clear of every obstacle, or `null` — and `null` means the box
  is NOT drawn. Everything placed goes through it, the leave-the-map banner
  included: pinned top-centre it reached x 1200 on the committed 1920×1080 frame
  where the panel's crop starts at 1131. **There is no longer an exception.**
  POE-244 carved one out for the callout's arrow — a 3 px line may cross a crop
  where a filled box may not, with the arrowHEAD stopped `ARROW_STANDOFF_CSS`
  short of the block so the triangle did not land on its first glyphs — and
  POE-248 removed the arrow (owner: no arrows anywhere), and the exception with
  it: no PLACER output crosses a read region any more. That is the class this
  rule binds — a rectangle the user dragged is still outside it, which is why
  the Debug-capture item below tells you to blame a drag rather than a defect.
  The rects are published
  rather than recomputed in TypeScript for the usual reason — a copy of any of the
  five constants would drift with nothing to fail — and `neverCoverRects` returns
  an EMPTY list both when there is no layout and when the scale factor has not
  resolved, which the callers must read as "place nothing yet" and never as "the
  screen is free". **All three placers state that themselves rather than
  inheriting it**: `calloutPlacement`, `bannerPlacement` and
  `doorDefaultPlacement` each return null on an empty set, and `doorDefaults` in
  the route repeats it so no default is offered to the host at all. The banner is
  why the rule is stated per placer instead of once at the caller — its wanted
  position (top centre) is a function of the HOST alone, so an empty obstacle list
  did not withhold it, it PLACED it, straight over the panel crop the next tick
  reads. A `null` anchor covering the same case in the callout was a coincidence,
  not the rule. What the rule does NOT bind is a rectangle the USER owns — a
  stored placement, or the registry default `placementFor` falls back to. Those
  are visible, attributable and one drag from fixed; see ADR-019's carve-out.
- `WidgetHost.svelte` owns placement, hot rects and click routing. Any element a
  widget draws with `data-hot` is claimed; one that also carries `data-action`
  is dispatched through `elementFromPoint`. The window declares nothing of its
  own, so a widget overlay with no buttons on screen is display-only.
- **Hot rects and `has_content` are one declaration, not two.** `hit_test` skips
  a window whose `has_content` is false before it reads a single rect, and the
  flag starts false, so `use-hot-rects.ts` sets it from the rects' emptiness —
  armed when the first rect appears, cleared when the last goes away and on
  teardown. A host that only sent rects would have every button it draws
  swallowed by the game, silently. The five older overlays each set the flag
  from their own content rule; a widget window has no separate rule, because
  "drawing something clickable" and "claiming a rectangle" are the same
  statement out here.
- The host is mounted UNCONDITIONALLY by the module's route; the module's own
  content rule is applied inside the `content` snippet, which receives
  `(spec, configMode)`. A host behind an `{#if}` has no `widget-config`
  listener while the module is drawing nothing, so a window flipped into config
  mode then would be genuinely interactive with no Save and no Cancel on it. In
  config mode a widget with no content draws a placeholder carrying its name, so
  an empty frame is still identifiable and still draggable.
- **A placement written from outside the window is announced.**
  `set_widget_geometry` emits `widget-geometry-changed {module}` with
  `emit_to(module)`, and the host re-reads its map on it — outside config mode
  only, where the draft rectangles are the truth and a Save is committing one
  widget at a time. Settings' Show checkbox is the writer this exists for:
  without the notice, a widget switched off stayed on screen until the overlay
  was next rebuilt, and the checkbox looked like it had done nothing.
- A widget is CONTENT-SIZED until the user drags an edge: Save persists
  `width`/`height` of `0` unless that widget was resized in this config session
  or already had a non-zero stored size, and `placementFor` reads a zero size
  back as "let the content decide" while applying the registry's shipped width
  as a `max-width`. Persisting the measured size on every Save would pin every
  widget in the module the first time any one of them was moved.
- **A stored placement is REBASED first and clamped second** (POE-239). Every
  placement carries the host size it was made against (`host_width` /
  `host_height`, physical px, written on every Save), and `rebase()` scales the
  rectangle by the two axis ratios before `clampToHost()` sees it, so a widget
  placed two-thirds across a 3840×2160 monitor is two-thirds across a 1920×1080
  one. The clamp stays as the LAST-RESORT safety — an unknown host, an aspect
  change, a widget wider than the new screen — and nothing else: on its own it
  could only pin the widget to an edge, and the next Save wrote that edge back
  over the user's intent permanently. Run 4K → 1080p → 4K, `rebase()`'s OWN
  arithmetic returns the rectangle to within the two pixels its two roundings
  cost — a claim about that function and not about a real trip, which also goes
  through `cssRect()`, the clamp and `sizeToPersist()` and is not measured
  end to end. A widget that carries a size keeps at least `MIN_WIDGET_SIDE_CSS`
  of it through the rebase (converted with the window's scale factor, the same
  floor a live resize stops at), because a frame shrunk under its own grab zone
  has no interior to drag and no edge to pull, and config mode is the only way
  back; a content-sized `0 × 0` is not a size and stays `0 × 0`. A row with `0`
  for either host field is UNKNOWN — every row
  written before the field existed, and every row Settings' Show checkbox
  writes from a window that does not know the overlay's size — and is never
  rebased, so those behave exactly as they always did.

- **The config bar is placed against the WIDGETS, not against the window**
  (POE-245). The host is a whole monitor, so the bar's shipped position — 24 px
  above the bottom edge, centred on the screen — put the only controls that
  window has a thousand pixels from the widgets they act on, at 11 px of type;
  the owner's report was that it could not be found at all. `configBarAnchor()`
  (`overlay/widgets/widget-config-bar.ts`) anchors it to the bounding box of the
  widgets being arranged: above the cluster by preference, because the pointer
  is inside a widget for the length of a drag and a bar below it sits under the
  hand; below when there is no room above; the top of the host when neither
  fits. Everything is clamped into the host, so no answer can put Save off the
  edge of the monitor. Config mode also DIMS the whole host, which is the
  affordance a 3 px frame per widget was not: the previous session announced
  itself only by those frames and that strip, so pressing Configure and looking
  at the game told the user nothing. The bar's size is measured out of the DOM
  rather than assumed, because a refused save names itself in it and that copy
  is variable-length.

### Config-mode ordering contract

Config mode is IN-WINDOW, not a `/overlay?sync=` copy, and the order the three
steps happen in is what makes it recoverable. Settings asks for it — it emits
`widget-config-start {module}` and nothing else — and `routes/(app)/+layout.svelte`
does the steps, because that is the file that owns every overlay window's
creation. In this order:

1. Ensure the module's window EXISTS and is SHOWN — creating it force-shown if
   the module is off or the game is not focused, the `set_debug_mode` path for
   one label. A window that is not on screen cannot be arranged, and one that
   does not exist has nothing to receive the event. "Creating it" is the layout
   RAISING THE MODULE WINDOW'S DESIRED STATE for the session and waiting for the
   label: a module-coupled overlay's desired state is
   `(module flag && feature grant) || widgetConfigLive(label)`, and
   `module-lifecycle.ts`'s driver is the one builder either term goes through,
   so a second creator here would be two builders racing for one label. **NO
   MODULE WORK STARTS** (POE-241, owner decision): the window is raised, the
   Rust module loop stays spawned by the module flag alone (`modules.rs`
   reconcile), and arranging widget positions therefore runs no capture loop and
   no OCR. Ending the session drops the record, which lowers the desired state
   again and lets the driver tear the window down when the flag is off.
2. `set_overlay_config_mode(label, true)` — the Rust flag first, so the mouse
   hook is already leaving the window alone before it becomes interactive.
3. Emit `widget-config {module, on: true}`, **webview-scoped** to that label.

What step 1 forced — the visibility — is RECORDED and undone when the session
ends (`$lib/overlay/widgets/widget-config-session.ts`). Both wrong answers are
silent: forgetting the hide leaves an overlay standing over a game the user had
it hidden for, and hiding one the poller has since shown takes their overlay
away with no toggle touched. One qualifier on the restore: the hide is VETOED
when the game is focused by the time the session ends, because the poller has
already shown that window and wants it shown — restoring "hidden" is a restore
only while the reason for hiding still holds. The hide also runs BEFORE the
record is dropped, since dropping it is what lets the driver destroy the window.

**ADR-014 needs no exception for this flow.** Because the session raises the
window and never the module flag, the work toggle still governs only the
module's Rust background tasks and the view surface still costs nothing — the
work/view separation the ADR decides is intact, not carved out.

A start that never reaches a host — no window appeared, or the command failed —
restores the same way and emits `widget-config-end` itself, or Settings sits on
`Configuring…` waiting for a window that does not exist. A start emits
`widget-config-opening {module}` when it picks the request up and
`widget-config-open {module}` when config mode is actually on; the first buys
Settings' opening deadline one more period, the second stands it down. That
deadline bounds the OPENING only and never the arranging session, which ends
when the user presses Save or Cancel however long they take — and it must not
abandon a start still in flight, which would set config mode on a window it had
just torn down.
Waiting for a window means waiting for the driver's `built()` marker, not for
`getByLabel` to answer: the label exists the moment the constructor returns,
while `tauri://created` — and the self-hide at the end of it — is still running.

Three things then have to leave that window alone while it is being arranged,
and each of them used to hide it or re-arm click-through underneath the user:
the mouse hook (`config_mode` in the registry), the RUST FOCUS POLLER — whose
hide branch now consults `overlay_hook::config_mode(label)` as well as debug
mode, because it acts on TRANSITIONS and one `Other` window taking the
foreground mid-session would hide the window for good — and the window's own
creation path, which ends by hiding itself when the game is not focused and
must skip that when it was built FOR a config session (the user is in Settings,
so the game is never focused then). `get_overlay_config_mode` is hard-false off
Windows: the flag lives in the mouse-hook registry, which is a Windows
structure, so a Linux dev build never enters config mode through the catch-up
path and only the event drives it there.

The host ALSO queries `get_overlay_config_mode(label)` once on mount, chained
onto its `widget-config` listener so the listener is registered FIRST — `listen`
is itself async, and a query that ran beside it could answer false, then miss
the event that arrived before the listener existed. That query is the catch-up
path for step 1 creating the window: such a window has no listener when step 3
fires, and without the query it would sit interactive with no Save and no
Cancel. Because the flag is set before the emit, the query cannot miss it. The
two paths may therefore both fire, which is harmless: entering config mode is
idempotent, so a second Configure press — the user's way out of a window that
somehow missed both — re-emits safely and never reseeds a drag in progress.

The host owns the way out. Save writes every widget of the module through
`set_widget_geometry` — committing only the ids whose invoke RESOLVED, and
staying in config mode with the failure named in the Save/Cancel bar if any
rejected — and Cancel re-reads the persisted map rather than restoring a
snapshot taken on the way in (config mode can begin before the first read has
answered, and restoring an empty snapshot would wipe every placement).

**The exit is ordered the same way the entry is, and for the same reason:
nothing local changes until Rust confirms.** Both paths call
`set_overlay_config_mode(label, false)` FIRST and keep local config mode — the
widget frames, the draft rectangles and the Save/Cancel bar — until it resolves.
Only then are the draft dropped and `widget-config-end {module}` emitted, which
is what the layout restores the window's visibility and drops the session
record on, and what Settings clears its `Configuring…` button and re-reads the
placements on.

Clearing config mode first was the bug (POE-227): it removed the only controls a
monitor-sized overlay window has while the window was still interactive, and the
failure was logged rather than shown — an invisible, always-on-top rectangle
eating every click over the game with nothing on it to press. Emitting
`widget-config-end` on that path made it worse, ending the session everywhere
except on the window the user was stuck inside.

**A refusal RE-ASSERTS config mode before it shows the error, and that is not
belt-and-braces.** Read the Rust side's asymmetry first: on the way OUT,
`set_overlay_config_mode` clears its own registry flag and re-applies
`WS_EX_NOACTIVATE` even when `set_ignore_cursor_events(true)` failed, then
returns the error. Leaving the flag set would strand the window — the hook skips
a config-mode window and would never repair it. That is right for Rust and it
costs the host its retry: with the flag cleared the hook resumes repairing
`WS_EX_TRANSPARENT` on the next mouse move, so within one twitch of the cursor
the window is click-through again and the bar the host just decided to keep is
no longer clickable. So the host sends `set_overlay_config_mode(label, true)`
before rendering the failure, and the wording turns on whether THAT landed:
re-asserted means "press Save or Cancel again", and a re-assert that failed too
means the window cannot be recovered from the inside and the way back is
Configure in Settings. The decision is `configExitDecision` in
`$lib/overlay/widgets/widget-config-exit.ts`.

**The layout puts a floor under a session that never ends.** Because the host
can now decline to leave config mode, `widget-config-end` may never arrive —
and everyone outside the window is waiting on it: Settings on `Configuring…`,
the window force-shown, and a window that only the session is holding up. So
`routes/(app)/+layout.svelte` arms a deadline where Settings' opening deadline
stands down (on `widget-config-open`) and clears it in `endWidgetConfig`; if it
fires, it logs and runs `abandonWidgetConfig`. Ten minutes: far above real
arranging (the session ends when the user presses Save or Cancel, however long
they take) and far below "for the rest of the process", which is what a
monitor-sized interactive rectangle over the game would otherwise cost. A second
Configure press re-arms it, which is the honest reading of the user saying they
are still working.

**An abandon has to reach the WINDOW, not just Settings.** The host listens for
`widget-config` and nothing else, so restoring the visibility and the button
while saying nothing to the window would leave Rust's `config_mode` set — the
re-assert above puts it back on a refused exit — and with it a monitor-sized
interactive rectangle the mouse hook deliberately skips, now with no Settings
button left to explain it. `abandonWidgetConfig` therefore does the window
first, in three steps, and only then restores and emits the end:

1. `widget-config {on: false}`, webview-scoped. The HOST owns the Rust call —
   its handler runs `exitConfig` — so the ordinary path keeps one owner and the
   host clears its own frames, draft and bar.
2. `set_overlay_config_mode(label, false)` direct, as a belt. It covers a host
   that is unreachable (no listener, a window mid-teardown) and one whose own
   exit REFUSED, which has just re-asserted config mode and would otherwise
   leave the flag set behind us. On an abandon the layout wins: the session is
   coming down whether the window agrees or not.
3. The same event again. Free when the host has already exited — `exitConfig`
   returns immediately when it is not in config mode — and it closes the gap
   step 2 opens: a host that refused in step 1 is still drawing frames over a
   window that is click-through again, and this repeat is the retry that now
   succeeds, because step 2 has already put Rust where the host needs it.

The same three steps run for a start that never got as far as a host, where the
belt and the repeat are expected to find nothing. The five per-window config
flows above are untouched.

## Current data and lifecycle behavior

- Shared main-window status is event-driven through `status.svelte.ts`.
- The comparator overlay polls Rust-held comparator data every 500 ms.
- Compass/path-strip settings and layout paths include polling/reconciliation.
- Game focus is determined in Rust by a `GetForegroundWindow` poller; Client.txt
  focus events are not the authority.
- Client.txt uses filesystem notifications with a five-second polling fallback.
- Font panel scanning begins from the `LabFinished` navigation event. The third
  Aspirant's Trial is logged but is not the scan trigger.
- The merc verdict overlay does not render `mercenary.trade` yet (POE-202).
  The search state reaches every window on the slice regardless, and both
  pieces the Mercenaries page draws it with are already cross-window for that
  reason: `$lib/components/TradeListings.svelte` rather than a component
  beside the Comparator, and `$lib/mercenaries/trade-view.ts` rather than
  wording inside the page.

Do not summarize the desktop architecture as “no polling.” Use the mechanism
owned by the specific state path.

## Runtime-earned observations to preserve

The following were recorded during Windows/WebView2 debugging and remain
relevant constraints even though static code inspection cannot prove every
runtime failure mode:

- Cross-window JavaScript window operations and events have returned stale data
  or failed silently; prefer work performed in the owning window, Rust commands,
  or the established Rust-backed polling path.
- WebView2 child HWNDs defeat parent-only `WM_NCHITTEST` handling.
- HWND access immediately after creation can fail; the click-through command
  delays setup while WebView2 initializes. The delay is runtime-earned and
  stays; what the command no longer does is return before it has elapsed.
- `focusable: false`, `WS_EX_NOACTIVATE` alone, and Tauri cursor-ignore APIs did
  not provide selective interaction reliably.
- `onMount` was observed not to run reliably in overlay windows; established
  overlay initialization uses rune effects/listeners.
- Destroying a transparent always-on-top WebView2 window from its own click path
  has left Win32 mouse capture stuck. Configuration Save/Cancel is coordinated
  by the owning main-window flow.
- The WebView2 transparency resize workaround can disturb position, so exact
  physical position/size is applied after creation.
- Windows removes a `WH_MOUSE_LL` hook whose proc exceeds
  `LowLevelHooksTimeout` (300 ms by default) and reports nothing **(from
  training data; not yet reproduced on this app — smoke item 9 is the
  acceptance)**: the process keeps its `HHOOK`, `overlay_hook.rs`'s
  `HOOK_CLAIMED` stays set, and every symptom is silent — overlay buttons stop
  responding and the `WS_EX_TRANSPARENT` repair stops, with no log line. The
  message-loop thread therefore runs a watchdog every `WATCHDOG_PERIOD_MS`
  (3 s, a tunable guess): it samples `GetCursorPos` and the stamp the hook proc
  writes, and re-installs in place when **the cursor moved between two samples
  and no callback landed at or after the earlier one** —
  `overlay_hook::hook_is_dead` (POE-238). What it CANNOT detect: an idle cursor
  is not death, so a hook removed while the user's hand is off the mouse stays
  unnoticed until the next movement; a cursor `GetCursorPos` could not read is
  treated the same way (unreadable is `None`, never a stand-in coordinate) and
  is reported on its own first/every-tenth cadence; and a hook that is merely
  slow reads as healthy. Every watchdog line is gated by
  `overlay_hook::Cadence` — first, then every tenth of a consecutive run —
  because `app_log` is a synchronous write plus an emit on the hook thread, and
  successful re-installs need that gate as much as failures do: a proc that
  chronically blows the timeout is repaired and re-broken indefinitely. The
  claim is deliberately never released around the re-install, and the mouse
  path is deliberately limited to one `Relaxed` `AtomicU64` store — both are
  what keep the recovery from becoming the next timeout.

Do not delete these observations merely because a future code path appears
simpler; reproduce the Windows behavior first or supersede them with a dated
regression test/decision.

> The temple module's arming / detection / OCR / show-hide ORDER is normative in
> [TEMPLE-LIFECYCLE.md](TEMPLE-LIFECYCLE.md) (POE-249); the smoke items below test the rules it names.

## Windows smoke checks

### Replaying a saved capture (no game needed)

The Temple page's **Debug capture** takes an optional file path (POE-124, ad18695): paste the
full path of a dump's `screen.png` and the button reads **Debug read file** — the whole read
path (anchor, ROIs, marker read, panel OCR, advisor) runs on that image on the real WinRT OCR
engine and writes a new dump. The regression board is the 2026-09-03 laptop capture,
`%APPDATA%\profitofexile\temple-debug\1788438639673\screen.png` (1920×1080, Lightning
Workshop at C1). A pass (measured 2026-09-04 at 4b3e8dc): anchored at temple scale ≈ 1.0 with
no search (`anchoring on the remembered screen scale …` or the scale table), `panelRect`
≈ [1131,5,543,453], `diamondRect` ≈ [1312,118,200,200], `markerError` null (6/6 seals),
`panel — 2 architect block(s)` naming Hayoxi (upgrade → Omnitect Reactor Plant) and Xopec
(change → Explosives Room) with rects in `ocr-lines.json`, title Lightning Workshop.

To drive it without touching the UI (from WSL, while `npx tauri dev` runs): start a SECOND
instance of the debug exe with its own WebView2 browser process and a devtools port —

```
powershell.exe -NoProfile -Command "$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'; $env:WEBVIEW2_USER_DATA_FOLDER='C:\Users\<you>\AppData\Local\profitofexile-smoke'; Start-Process '<win repo>\src-tauri\target\debug\ProfitOfExile.exe' -WorkingDirectory '<win repo>\src-tauri' -PassThru"
```

(without the private user-data folder the argument is ignored — WebView2 shares one browser
process per folder), then run `desktop/scripts/smoke-cdp.mjs` ON WINDOWS
(`cmd.exe /c "cd /d <win repo> && node scripts\smoke-cdp.mjs <screen.png>"`): it finds the
`localhost:1420` page target and invokes `temple_debug_capture` with the path, printing the
report. **Stop the second instance before any Rust rebuild** — a running exe blocks
`tauri dev`'s relink. This is how POE-230/234/243 were accepted on real pixels after the
temple session was gone.

Before any Windows build, run `make desktop-check-windows`: it type-checks the
`cfg(windows)` half of the crate (overlay hook, click-through setup, capture)
against the `x86_64-pc-windows-gnu` target inside the desktop image, which the
Linux `cargo check`/`cargo test` gates never compile. It is a type-check only;
the build itself still happens on Windows CI.

Static gates cannot reach the checks below; run them on a Windows build after
touching the named path.

- **Widget overlay, click-through and the hot rect** (POE-225): with the temple
  module on and the game focused, click the game through an empty part of the
  fullscreen window AND through a widget — both must reach the game. Then start
  the app in debug mode and toggle the module off and on, so the window is built
  with `?debug` and the temple DOOR widget draws its `data-hot` probe: a click
  on the probe must log `hot-rect probe clicked`, and a click one pixel outside
  it must reach the game. A probe that does nothing means the declaration or the
  `overlay-click` listener is wrong; a click beside it that does NOT reach the
  game means the withdrawn/declared rect is too big. **This needs a live temple
  board on screen**, not just the module enabled: the probe sits inside the door
  widget's snippet, which the route draws only when there is a room read
  (`overlayShowsDoors` plus a published diamond). Outside a temple the window is
  up and the widget renders nothing, so there is no rect to click and the check
  proves nothing. Since POE-244 the probe is on the DOOR widget and not the
  advice one: the callout is anchored, so it is not placed by the user and is
  not drawn in config mode, and the probe belongs on a surface that behaves like
  every other widget. That moved its precondition too — the door widget needs a
  published `layout.diamond`, which means a read that settled a CURRENT ROOM, so
  **run this standing IN a room**. A board read between rooms
  (`no_current_room`) draws no diamond and therefore no probe.
- **Click-through actually applied, on every overlay** (POE-227): the command
  now awaits its own setup and returns a failure, but only Windows can produce
  one. Toggle each overlay on with the game focused and click the game through
  it. Then force the failure path once if you can (a label destroyed inside the
  ~1 s wait — toggle the temple module off and straight back on): the temple
  window must not be left standing, `module-lifecycle.ts` must log a retry, and
  the app log must carry the `set_overlay_clickthrough` reason. The Windows
  branch of this command is TYPE-CHECKED by `make desktop-check-windows`
  (`clickthrough_setup`, `clickthrough_belt_passes`,
  `overlay_hook::is_transparent`); only the behaviour is smoke.
- **Leaving widget-config mode** (POE-227): with the temple widgets being
  arranged, press Save and then, in a second session, Cancel. Both must close
  the session — the frames go, the bar goes, Settings' `Configuring…` clears,
  and the window visibility returns to what it was (the module flag was never
  touched: check the Modules row is exactly where you left it). This is
  the ORDERING check: `widget-config-exit.test.ts` pins only the decision, and
  the sequence that consumes it (invoke first, keep the bar, re-assert on a
  refusal, emit `widget-config-end` only on the Ok) is in `WidgetHost.svelte`,
  which has no unit-test harness.
- **A REFUSED exit from widget-config mode** (POE-227): if
  `set_overlay_config_mode(label, false)` ever rejects, the red frames and the
  Save/Cancel bar must REMAIN with the reason in the bar, and — this is the half
  that is easy to get wrong — the buttons must still respond after the cursor
  has moved across the window, which is what the re-assert buys. A bar that
  disappears is the original POE-227 regression; a bar that stays but stops
  responding means the re-assert is not being sent or is failing silently, and
  the wording must then be the "press Configure in Settings again" one.
- **A widget-config session that never ends** (POE-227): leave a session open
  past `WIDGET_CONFIG_SESSION_MAX_MS` (10 min) without pressing Save or Cancel.
  The layout must log that it is ending the session from its side and run
  `abandonWidgetConfig`: Settings' button leaves `Configuring…`, a window this
  flow raised for a module whose flag is off must go away rather than stay
  standing, and — the half that is invisible from Settings — the WINDOW must
  come out of config mode
  with it. Check the last part by clicking the game through where the widgets
  were: a click that does not reach the game means the abandon restored the
  outside and left the window interactive, which is the POE-227 N1 regression.
- **Comparator width, after an overlay-wide CSS change** (added after the
  POE-225 batch, where a `box-sizing: border-box` added to the shared overlay
  layout's reset narrowed the comparator table from 582 px to 560): with the
  comparator overlay open on a gem, check that the table is as wide as its saved
  window and that no column is clipped. `routes/overlay/+layout.svelte` is loaded
  by EVERY overlay window, and the five that predate the widget engine are laid
  out under the default `content-box` — a box-model declaration added there
  reflows all of them silently, with no gate that can see it. Anything the widget
  host needs belongs in `WidgetHost.svelte`, which is where `border-box` now is.
- **Merc strip, after a row-count change** (the content-driven resize path): let
  the strip redraw at a different height — open a recruit window with a
  different number of rows — then, with the read still running, sweep the mouse
  across the strip and left-click on it. The click must reach the game and the
  module status must stay `live`/`scanning`. A click that selects something in
  our window instead, or a status that drops to `idle`, means the resize lost
  `WS_EX_TRANSPARENT` and the re-assert in `fit_overlay_height` is not working.
  A retire no longer serves as the redraw here: it leaves the strip on screen
  for only the 4 s linger, and `idle` throughout it.
- **Merc strip, first paint**: starting the module must not flash a large empty
  panel. The constructor seed is one line tall and the content replaces it. The
  waiting line shows for 4 s after the start and then the strip clears; a strip
  still showing "waiting for a mercenary" a minute later means the idle linger
  (`overlayShown` / `lingerAdvance`) is not being advanced by the route.
- **Merc header, across re-detects** (added after the 2026-08-25 smoke, where
  the header blinked between the mercenary's name and its class every two
  seconds): keep one recruit window open and watch the header line for at least
  three re-detects. Fields may only be FILLED IN, never blanked or swapped for a
  shorter or glyph-prefixed reading, and the name must never equal the class. A
  header that changes back and forth means the sticky merge (`read.rs`'s
  `merge_header`, applied in `run.rs`'s detect tick) is not being applied to the
  published capture.
- **Merc strip, the done state**: with a recruit window open, hover every cell
  the strip marks `?` or `✕` until the status line reads `done · N rows · all
  icons read`. From then on the log says `capture complete — OCR paused` once,
  the strip must stay on screen with its verdict, and closing the window must
  still retire it (up to ~20 s later, two liveness checks): the strip then says
  `recruit window gone — last read` for 4 s and clears entirely. A strip that
  blanks at `done`, or a status that never reaches it on a fully-read window,
  means the on-screen status set (`live` + `done`) or `capture_complete`
  disagrees with what the reader produced; a strip still up more than a few
  seconds after the log says `window gone` means the linger is not running out.
  **Hover still corrects a wrong read while `done`**: park the cursor on a cell
  the module matched WRONG and confirm the tooltip replaces it — the detect is
  what paused, not the hover. The re-read of an already-matched cell is capped
  per cell (`HoverBudget`, 3 per capture), so the correction has to land within
  the first few ticks of the hover; moving off the cell and back does NOT refill
  it, and neither does a retire the module restored from — the budget rides
  along with the confirmations in the retained slot, so a spent cell stays spent
  across a retire and re-detect of the same panel. Only a genuinely new window
  refills it.
- **Merc hover, the occluded panel** (added after the 2026-08-25 smoke, where
  hovering a cell retired the capture two ticks later and the cell flipped back
  to `✕`): with a recruit window open, park the cursor on a support cell for at
  least 5 s so the game tooltip covers the panel, then move off it. The log must
  say `panel occluded (cursor over it) — holding the capture` ONCE per hover and
  must NOT say `window gone`, and the cell must still read `✓` after the tooltip
  closes. A `window gone` under the cursor means the panel rect
  (`geometry.rs`'s `panel_bounds`, stored on the session at detect) is wrong or
  is not being consulted by `miss_kind`. The hold is CONTINUOUS occlusion, not a
  budget that resets per miss: park the cursor on TAKE ITEM and close the window
  with it. Retire lands at the 15 s cap rounded UP to the next detect tick, plus
  one more tick for the second miss — **so check which cadence you are in
  first**, because it dominates the number. A window still being read
  re-detects every 2 s, so it retires ≈20 s after the close; a window the strip
  already calls `done` is on the 10 s liveness cadence, so it retires ≈40 s
  after the close (up to 10 s to notice, misses at +20 s and +30 s). Time it
  from the close and compare against the cadence you are actually in — a `done`
  window still on screen at 60 s, or a live one past ~25 s, means
  `OcclusionRun` is clearing its run on a counted miss and each cap is
  restarting the clock. If the capture does retire — the window really
  closed, or the cap fired — the next detect of the SAME panel must log
  `confirmation(s) and its header restored` and bring back both the `✓` and the
  mercenary's name and level; a `confirmations were dropped` line there means
  `same_panel_positive` found no positive evidence, which a tick that read
  neither a level nor two skill names is expected to produce.
- **Merc rematch / fast swap**: with a capture on screen, press REMATCH (or
  close the window and open a different mercenary within ~20 s, before the
  liveness check retires the first). The header must switch to the NEW
  mercenary's name, class and level in one step, and the log must say `recruit
  window replaced`. A header that keeps the previous mercenary's name — or an
  old level under a new name — means the panel-identity gate (`panel_replaced` /
  `fold_header`) is not being consulted before the sticky merge. The gate wants
  POSITIVE evidence (two levels that disagree, or two disjoint skill sets), so
  the inverse check matters too: a tick that merely read the panel badly must
  NOT log `recruit window replaced`, because that log line means the session's
  hover confirmations were just thrown away.
- **Game fullscreen on the secondary monitor** (POE-237, item 10 of the POE-223
  smoke list): put PoE fullscreen on a display that is NOT the Windows primary,
  alt-tab into it once so the focus poller resolves it (`app.log` says `game is
  on monitor N at x,y`), then check all three consumers on THAT monitor: the
  temple widget window draws over the game rather than on the primary; a merc
  recruit window is detected and the strip appears beside it; and the Settings
  "Screen geometry" card reports the second display's own resolution. Then drag
  the game to the primary and alt-tab back: one more `game is on monitor` line,
  and the temple window must reappear on the primary within a second — a window
  left behind means the `game-monitor-changed` rebuild is not firing (it is
  `emit_to("main")`, so a bare `listen()` in the layout would never see it).
  Unplug the second display while PoE stays in the foreground and capture must
  NOT stop: one `the game's monitor at x,y is gone — capturing the primary until
  the next alt-tab` line, and OCR keeps running off the primary. Only the one
  line — a repeat every tick means the stored display is not being cleared —
  and the next alt-tab into the game logs a fresh `game is on monitor` line.
- **Two overlapping overlays, which one takes the click** (POE-239): raise two
  registered overlay windows whose hot rects overlap — the temple widget window
  is the whole game monitor, so any other overlay drawing a hot rect over it
  qualifies (the comparator, the merc strip). `app.log` must carry one `overlay
  hot rects overlap:` line per pair per registration. Click inside the overlap:
  the window shown MOST RECENTLY must take it. Bring the older one back — toggle
  it off and on, or let it go from empty to drawing — and the same click must
  switch to it. The inverse matters as much: with both up and neither
  re-shown, let the comparator take a price tick (it re-asserts
  `set_overlay_has_content(label, true)` with no emptiness flip) and the click
  must NOT move to it, because only the false→true EDGE restamps `shown_seq`.
- **Hook re-install after a silent removal** (POE-238, item 9 of the POE-223
  smoke list): with the comparator open on a gem, make Windows drop the
  `WH_MOUSE_LL` hook — hold a debugger pause of at least 1 s on the app, or
  unplug and replug the mouse — then move the cursor. `app.log` must show
  exactly ONE `overlay hook re-installed (#1 consecutive, silent for N ms)`
  line, and the comparator's buttons must work afterwards. `silent for` is the
  age of the newest callback stamp, so it includes time nobody touched the
  mouse and is process uptime while the hook has never fired — it is not a
  measure of how long the hook was gone. A line that KEEPS coming back (about
  every 6 s, not 3 s: the re-install makes the proc stamp once, so one whole
  watchdog period reads alive before the next verdict) has two suspects, in
  this order: (1) the hook proc is chronically exceeding
  `LowLevelHooksTimeout`, so Windows removes each fresh hook as fast as we
  install it — the parked half of POE-238 is the throttle that would fix it,
  and the give-away is that the `#N consecutive` counter keeps climbing;
  (2) the stamp is not landing at all (the re-install "succeeded" onto a proc
  that never runs), which looks identical in the log but leaves the buttons
  dead. No line at all with dead buttons means the watchdog read the cursor as
  still — look for `overlay hook watchdog blind: GetCursorPos failed N time(s)`
  before concluding `hook_is_dead` is wrong. A burst of `overlay hook
  re-install failed` lines is the honest failure: the buttons stay dead until
  one succeeds. All three lines are on the same cadence — the first, then every
  tenth of a run — so a rate of roughly one line per minute in the chronic case
  is the design, not a lost log.
- **The temple captures only when something says so** (POE-242 + POE-246, item
  12 of the POE-223 smoke list): with the temple module on and the game focused,
  stand in a map with no incursion running and no layout panel open. The Temple
  page's status must read **on, waiting for Alva**, and `app.log` must carry
  `Temple: capture armed by the start-up probe` followed a tick or two later by
  ONE `Temple: capture stood down` line — one, not one a second. The probe line
  is POE-246 and is expected: a starting loop runs exactly one detect tick before
  it may believe an empty screen, because the module can be switched on with the
  panel already open and no Client.txt event is coming for that. A SECOND `stood
  down` line is no longer expected either. Since POE-246 the log line is keyed on
  the arm SOURCE while the publish is keyed on armed-ness, so the
  `gate_announcement` re-assertion that corrects the POE-171 finding-15 race — a
  retiring loop's `Stopping → Idle` publish landing after the new loop's
  `Waiting` — fixes the status without repeating the line. What the check is
  actually looking for is a line per SECOND, which means the gate is not latching
  at all. Then start an incursion:
  the log gets
  `Temple: capture armed by Alva` (exactly one line — the capture loop owns the
  arm/disarm line, the Client.txt trigger writes state and says nothing), the
  status returns to `idle`, and opening the layout panel reads the board as
  before. Walk into the temple and out again: the arm survives the whole run (an
  area arm carries no deadline) and the `You have entered` line back into the map
  stands it down. Restart the app INSIDE the temple — the catch-up must log
  `the log's newest area is the temple, armed` rather than leaving the module
  waiting for a line that is never coming. A tail with **no** `You have entered`
  line in it (a quiet log, or one truncated between area changes) seeds
  `Disarmed` by design — Re-arm is the recovery, not a bug to report.
- **The panel on screen is what keeps the capture armed** (POE-246): three
  reads, all with the temple module on and the game focused.

  **What this item is and is not.** POE-246 is RETENTION plus one start-up look:
  it keeps a gate that is already open from closing under a panel the loop can
  still see, and it gives a starting loop one tick to notice a panel that is
  already there. It does NOT acquire a panel from a stood-down state — opening
  the layout panel with the loop already stood down and Alva silent (the hideout
  read, item 3 of the POE-242 list) still needs **Re-arm**, exactly as it did
  after POE-242. An area change out of the temple ends the panel's claim
  immediately, so walking out does not carry two minutes of capture into the next
  zone.

  1. **The panel outlives Client.txt.** Right after Alva speaks — with
     `Temple: capture armed by Alva` in the log, so the loop is known to be armed
     — open the layout panel and leave it open for more than two minutes. The
     advice must stay on screen for as long as the panel is, whatever Client.txt
     said and however long ago. `app.log` may pick up
     `Temple: capture armed by the panel on screen` as the Alva tail expires
     under the open panel — that line IS the fix reporting itself. Measured
     before it, on the laptop 2026-09-03: `layout panel found` 14:36:14 →
     `capture stood down — waiting for Alva` 14:37:00 with the panel still open,
     and the overlay hid with the status.
  2. **A module toggled on over an open panel reads it.** With the panel open and
     Alva silent, switch the temple module off and on. The board must appear
     without pressing Re-arm, and it must appear on the PROBE tick — about a
     second on a screen something has already measured, up to one sweep (~5 s) on
     one nothing has. If it has not appeared by then it will not appear at all
     until Alva speaks or Re-arm is pressed, because the probe is one look and a
     loop that has spent it stands down. Before POE-246 this logged `capture loop
     started` and `capture stood down` in the same second (17:28:31, same laptop)
     and the owner saw the overlay "blink and disappear".
  3. **A closed panel still stands the loop down.** Close the panel and stay in
     the map with Alva quiet. `Temple: capture stood down` must arrive about two
     minutes later (`temple::trigger::PANEL_TAIL_MS`) — and it must ARRIVE,
     because POE-242's whole point is that a closed panel does not keep the loop
     capturing. A stand-down that never comes means something on an empty screen
     is clearing `anchor::NCC_FLOOR` every tick, which is worth a
     `temple_debug_capture` dump rather than a tuning change here. Leaving the
     zone is the fast path to the same place: walk out of the temple (or take a
     portal) and the `You have entered` line must stand the loop down on the next
     iteration, NOT two minutes later — the sighting is a claim about a screen
     the player has left.
- **The panel and diamond crops land on the real panel** (POE-230): the check the
  fixtures cannot make, because the repository has exactly one full-frame temple
  capture and the bug was a property of a frame's SIZE. With a layout panel open,
  read the Temple page: the corridor line must not carry
  `Door markers unread — corridors fall back to the beam read (…)`, and the room
  must show its corridors from the DIAMOND rather than from the beam fallback —
  all six seals, which is the read POE-230's measured failure got 5 of. Then press **Debug capture** and open the dump:
  `report.json`'s notes must carry `panel — 2 architect block(s), …` on a
  two-architect panel (1 is the POE-230 failure exactly: the old crop cut the
  lower-left block off) and `marker_error` must be absent. Cross-check the rects
  in `app.log`'s `Temple: rois panel […] diamond […] remaining […]` line against
  `panel.png` and `diamond.png` in the dump — a crop that is half a panel is
  visible there and nowhere else. Run it on BOTH machines: the rects are keyed on
  the Entrance anchor now, so a screen whose panel sits at a different offset from
  the capture's right edge is the case that used to fail and the case this proves.

  **The honesty half** (POE-223 follow-up audit): run the game WINDOWED, small
  enough that the panel's crop falls off the capture entirely. The Temple page
  must print `Temple: panel ROI [x, y, w, h] is outside the capture — windowed
  client?` as a YELLOW notice — not as the red "Last error", because the read
  completed and the board is real — and `app.log` must carry it ONCE, not once a
  second and not once per position while the window is dragged. Maximise the game
  again and the notice must go away on the next read. Before this the crop was
  stepped over in silence and the panel simply read as having no architects on it.
- **The temple anchors from the shared screen scale, and writes back to it**
  (POE-234): four reads on one machine.

  1. **A cold screen anchors in seconds, with no Debug capture.** Start from a
     fresh install (or press **Recalibrate** in Settings → Screen geometry) so the
     slice is empty, then open a layout panel. The board must be read within a few
     seconds — one pyramid sweep, ~5 s in the release container — and `app.log`
     must NOT show the exhaustive sweep's minutes. Before POE-234 the loop never
     swept at all on the reporting laptop (`detect_cheap` peaked at 0.66 against
     the 0.70 floor) and the only way to a board was the Debug button.
  2. **A remembered scale is used, and said once.** Restart with the panel closed,
     then open it. `app.log` must carry ONE
     `Temple: anchoring on the remembered screen scale (…, ui_scale …) — temple
     scale …, no search` line, not one per second — and none at all when the
     remembered value is the temple's OWN (the line names the cue, and the temple
     does not announce itself back).
  3. **A monitor change re-measures exactly once.** Move the game to another
     display. `app.log` must carry ONE
     `screen is now W×H on monitor N — dropping the remembered scale (was …)` and
     then one fresh anchor — not a line per tick, which is what an un-keyed drop
     produces.
  4. **The card names the temple as the cue.** On a machine whose merc recruit
     window has never been opened, read a temple board and then open Settings →
     Screen geometry. The source row must read `temple Entrance plate` and the
     Verified row `Yes` (not `No — trusted from last session`). This is the half
     that proves the WRITE, and it has one EXPECTED non-failure: an anchor that
     agrees with a `remembered` startup seed inside the acceptance band is
     REFUSED, and the card correctly keeps saying `remembered from a previous
     run` — the temple confirmed the number rather than replacing it. So run this
     read after a Recalibrate, when the slice is empty and there is nothing to
     agree with. A card that stays `remembered` from an EMPTY slice is the real
     failure: `screen_from_anchor` withholds an anchor the capture height does not
     corroborate, so check `app.log` for a
     `temple anchor not corroborated by the capture: unit ratio k=… — the
     measurement was withheld` line before assuming the publish is broken.
- **A one-of-two architect read says so, and the two-block read is right**
  (POE-243): open the layout panel on a board with two architects. The kill
  callout's title must NOT carry `(only architect read)`, and on the PC's
  Armourer's Workshop board (2026-09-03, Quipolatl `upgrade → Armoury` against
  Atmohua `change → Shrine of Empowerment`, standing in a tier-1 room) the advice
  must be the **change → Sanctum of Unity** — that board is pinned as advisor case
  7 at both the suite's constants and the app's own, and `upgrade → Armoury` on
  screen means the PANEL lost Atmohua's block, not that the ranking is wrong. Then
  force a one-of-two read (cover the lower block, or catch a panel the OCR half
  reads): the callout and the Temple page must both mark the headline
  `(only architect read)` rather than showing a forced kill with the confidence of
  a chosen one. Finally press **Debug capture** on a board whose `change` offer
  WRAPS onto a second line and open `ocr-lines.json`: the block must appear in
  `blocks` with its rect and its whole wrapped target, and the engine-order lines
  above it must show which line the grouping attached. A block that stops at the
  wrap is the geometry rule failing, and this file is the only place that is
  visible.
- **The config bar is findable at 1080p and 1440p** (POE-245): press Configure
  in Settings → Overlay Positions → Temple. The whole monitor must dim, and the
  bar must appear next to the widgets with 14 px copy and buttons at least 32 px
  tall. The anchor is the BOUNDING BOX of every widget being arranged, not one
  of them, and config mode arranges only the PLACEABLE widgets — since POE-249
  that is two, `temple.door` at y = 300 and `temple.waiting` at y = 16. The
  cluster's top edge is therefore 16 px from the top of the monitor, a bar plus
  the 16 px gap does not fit above it, and on shipped defaults the bar sits
  BELOW the cluster and centred on the UNION of the two — the second branch of
  `configBarAnchor`, at {405, 531} on 1920×1080 for the 320×56 bar the suite
  measures. The union spans x 40…1090, so that centre is in the empty span
  BETWEEN the two widgets rather than beside either of them. (This item said
  ABOVE between POE-244 and POE-249, when the door was the only placeable
  widget, and BELOW before that, against POE-245's registry where
  `temple.advice` shipped at y = 40. The branch follows whatever is nearest the
  top edge, so a widget added or moved there inverts it again:
  read `configBarAnchor` and the shipped-defaults case in
  `widget-config-bar.test.ts`, not this paragraph.)

  To flip it, drag the WAITING NOTICE down out of the top band — past the door,
  anywhere below y = 300 — and leave the door where it is, then press Configure
  again: the cluster's top is now the door's y = 300, which leaves room for a
  bar plus the gap, and the bar must sit ABOVE the cluster. Drag the notice back
  to the top and re-enter: BELOW again. Then drag a widget to the bottom-right
  corner and re-enter — the bar must stay fully on screen, Save included.

  **Owner judgement, not arithmetic**: with the shipped pair the bar lands in
  the gap between the two widgets rather than beside one of them. The numbers
  are pinned by the test; what this item asks is whether that READS as the
  config bar for both widgets when it is on screen over the game, or whether it
  looks like it belongs to neither. If it reads badly the fix is a product
  decision about the shipped defaults, not a change to `configBarAnchor`.
- **A monitor change that arrives mid-build** (POE-245): with the temple module
  already on in settings, start the app with PoE running on a non-primary
  display but NOT in the foreground, then alt-tab into the game within a couple
  of seconds. The overlay must end up on the game's display. `app.log` carries
  `game is on monitor N at x,y` and then
  `built on a display the game has since left — rebuilding on the one at x,y`;
  it must NOT carry `creation attempt N failed` or
  `giving up until the module is toggled` alongside it, because the correction
  is the driver's own off/on and does not spend the create budget. Before
  POE-245 the notice was discarded — the window was still being built, so
  `built()` was false — and `remember_game_monitor` emits only on a CHANGE, so
  no second notice ever came and the widgets stayed on the primary for the rest
  of the session.

  **The open questions this item exists to answer.** Three reads, in this order:

  1. **The decision read — the one that decides the design.** Open Alva's
     dialogue and the layout panel BEFORE clicking *Enter Incursion*. Does the
     module read the board? Client.txt cannot answer this: the measured file
     shows only that Alva's start line exists, not whether it fires when the
     dialogue opens or when *Enter Incursion* is clicked. If it fires on the
     click, the panel was open while the module was disarmed and this read is
     LOST — and there is **no earlier signal in Client.txt to move to**. The
     owner then picks between the free-running cheap detect POE-242 replaced and
     giving up the decision read. Weak evidence for the good case (the start
     line lands 3–7 s after `[WINDOW] Gained focus`) is not a measurement.
  2. **The post-incursion read.** After the incursion, back in the map, open the
     panel again. Does it read? This one is expected to pass and is measured:
     the incursion instance logs no area change at all, so the arm bought at the
     start survives the whole incursion and `Good job…` extends it.
  3. **The hideout read.** Open the layout panel from Alva IN THE HIDEOUT, with
     no incursion and no voice line. Does the module read it, or does it need
     Re-arm? If it needs Re-arm, a hideout arm is a follow-up — the premise that
     the panel is only ever opened with an Alva line or the temple area in scope
     is UNVERIFIED (see `temple/trigger.rs`).

- **The kill callout sits level with the right block, on both machines**
  (POE-244, arrow removed in POE-248): open a temple layout panel with two
  architect blocks. A box must appear beside the panel reading
  `KILL <architect>` with one reason under it, VERTICALLY CENTRED on the block
  whose name it carries — not on the other one, and not between them. There is
  no arrow any more; the placement is the whole pointer. Check on BOTH machines
  (1920×1080 laptop and the desktop): the block rect is the union of that
  block's OCR line boxes in capture px, and the only step to CSS is the window's
  own scale factor, so a display at anything but 100 % is where a missing or
  doubled conversion shows. Then close one block's read (a one-of-two read,
  which the panel produces on its own often enough — or force it by covering the
  lower block): the box must carry `(only architect read)` inside the title
  line. A read with no block rect at all puts the box at the panel crop's top
  instead, which is correct and not a bug — there is no block to be level with.
- **Nothing is drawn over what the module reads** (POE-244) — the check the
  static gates cannot reach, because the failure is the app reading its own
  overlay back as game pixels. With the callout and the door diamond both on
  screen over a live panel, press **Debug capture** in the Temple page and open
  the dump: the capture is a real screen grab, so the overlay is IN it. Then
  compare `report.json` against a dump taken with the temple module's overlay
  toggled off on the same board — the room title, both architect blocks, the
  incursion count, `current`, `doors` and `unknownRooms` must be identical. Any
  difference is a surface sitting on a crop, and the first suspects are the
  callout's box and a room widget the user has dragged onto a plate.
  `avoidRects` cannot protect the second one —
  once the user places the widget it goes where they put it — so a difference
  that only appears after a drag is the user's placement and not a defect.
- **The room widget survives the incursion, and the stand-down** (POE-244,
  POE-246's arming, rewritten in POE-248): with the panel open and a room read,
  note the outline, the green open and red closed seals, the purple suggested
  seal (and the faint one beside it, when there is a second door), and both cyan
  kill glyphs, then click *Enter Incursion*. Every one of them must STAY for the
  whole timed run — the layout panel and the game's own diamond are gone by then, and
  this widget is the only surface left.
  **And they must still be there after `Temple: capture stood down` appears in
  `app.log`**, which is the POE-248 half. That line arrives roughly two minutes
  after the panel was last on screen — mid-incursion, in the live session that
  produced this item (`12:32:10 capture armed by the panel on screen` …
  `12:39:05 capture stood down`, widget gone). The capture stopping is a
  statement about whether anything is LOOKING; the incursion is not over because
  of it. What DOES take the widget down is a zone change, the next Alva voice
  line after the read, a new read, or the module switch — look for
  `Temple: advice cleared — <reason>` in `app.log` and check the widget went at
  that line and no earlier.
  The shape alone is not a pass, either: an earlier review found the widget
  drawing a room with **no purple seal and no kill**, because the capture loop's
  retire dropped `advice`.
- **The widget is the room, in the game's orientation** (POE-244, reshaped in
  POE-248): with the panel open, hold the widget beside the game's own diamond.
  The two must be the same shape at the same rotation — a RECTANGLE, wider along
  one isometric axis than the other, with its two long walls carrying two doors
  each and its two short walls one each. Green where the game draws green, RED
  where the game draws red — the colours are the game's own semantics, and a
  widget missing its closed seals is the first POE-248 build rather than this
  one. The one corridor drawn nowhere is the kind nothing settled, which
  `doorWarning()` says in words instead. The advisor's door is the bigger purple
  seal, and the cyan glyph sits on the same spot inside the room as the chosen
  architect's icon does in the game's own diamond — top-right for an upgrade,
  bottom-left for a change.
  The Rust side pins the geometry against the committed crops through the
  shipped detector (`the_committed_crops_land_within_seven_px_of_the_room_wall`,
  worst measured case 6.40 px on a 200 px rect), so what this check adds is the
  two things a fixture cannot see: that the OUTLINE reads as the same shape at
  widget size, and that the seals still look like holes in its walls. A widget
  rotated as a whole means `AXIS_X` / `AXIS_Y` were re-fitted; a widget that
  looks SQUARE means `ROOM_ASPECT` was lost and the rhombus is back; seals
  floating off the outline mean something reintroduced the constant ring POE-244
  shipped (the retired `SEAL_RING_FRACTION`), which POE-248 measured as the
  worse fit — rms 5.5 px / max 9.9 px against the rectangle's 3.2 / 6.4 on the
  same detections.
- **The kill glyph is on the right half of the room** (POE-248): with a panel
  open and a kill ranked, compare the widget with the game's own diamond. The
  cyan mark must sit where THAT architect's icon is drawn, and the other half
  must be empty. The half is keyed on the chosen block's own OCR rect — the
  block printed FIRST gets the top-right spot — so the check is: which of the
  two blocks did the advisor pick (the Temple page names it), and is the mark in
  the half the game drew that block's icon in? NO mark at all with a kill ranked
  means the icon spots did not reach the slice: a build from before POE-248, or
  a normalisation that dropped them.
- **A panel with the CHANGE block on top** (POE-248, the falsifier — look for
  one): the positional rule and the one-sample upgrade/change reading disagree
  only there, so it is the board that settles which is right. If the mark lands
  on the block the advisor did NOT pick, the positional rule is wrong and
  `killGlyphs` should key on `kind` instead; record the board either way, because
  one sample is what the current mapping rests on
  (`markers::ARCHITECT_ICON_OFFSET`).
- **A faint purple seal marks the second door** (POE-248): with one key set and
  a kill ranked, the widget carries TWO purple seals — the bright big one is the
  door to open now, the faint smaller one is what a SECOND Stone of Passage would
  buy. Check it against the Temple page, which prints the same answer in words
  under the top recommendation (`second stone: <edge>`): it must name the faint
  seal's corridor and not one of the `open …` doors above it, and that corridor
  must be one the game draws SHUT (opening an open door buys nothing).
  **No faint seal at `keys = 2`**: set the keys control to 2 and the faint seal
  must go, because with two stones in hand there is nothing conditional left —
  the advisor's own answer IS the two-key one. Usually that means a second BRIGHT
  seal appears; on a board where RU declines the second key it stays one bright
  seal and the Temple page says so in the reasons (`RU: …`). Either way, no faint
  one. Set it back to 1 and the faint seal returns.
  No faint seal at all with ONE key is legal and not automatically a defect: the
  room may have only one corridor worth a key, the pair may buy a merge the first
  door already bought, every pair may be RV-excluded, or RU may have declined the
  second key outright — the Temple page's reasons for the top recommendation are
  where to look before filing it.
- **The unchosen kill is drawn faint** (POE-248): with both architect blocks read,
  the widget carries TWO cyan glyphs — the advisor's at full strength, the other
  block's at about a quarter, at the opposite icon spot and with its OWN shape
  (an up-arrow for an `upgrade`, two-way arrows for a `change`). The pair is what
  orients: one mark alone says which half without saying what the halves are.
  Two marks at the SAME strength is the regression — the widget then reads as two
  instructions.
  **Which is which is the OPACITY and the position, never the shape.** The panel
  can print two blocks of the same kind — `panel.rs`'s own `CASE_1` fixture is two
  `change` offers — and the widget then draws two identical two-way glyphs, one
  bright and one faint. So read the strength, and cross-check the half against the
  block the Temple page names.
  That also bounds a pre-existing 50/50: on a TEXT-ONLY read (no OCR boxes, so
  nothing orders the blocks) the half falls back to the offer's `kind`, and with
  two blocks of the SAME kind that fallback cannot tell them apart — the pair is
  drawn in an order the read did not establish. Record such a board rather than
  filing it; the fix is a rect, not a rule.
  And on a one-block read (`1 of 2 architects read` on the Temple page, POE-243)
  there must be exactly ONE glyph: a second mark there would put a kill on screen
  that was never on the panel.
- **The change glyph reads as two-way, not a bar** (POE-248): on a `change`
  kill, the mark is two opposed arrows with daylight between their shafts —
  about 12 px at the shipped widget width. If it reads as one thick bar the
  shafts or the dark halo have been retuned past what the widget's own size
  supports (`SHAFT` and `.kill-shadow` in `TempleDoorDiamond.svelte`), and an
  `upgrade` mark then looks the same as a `change` one at a glance.
- **The leave-the-map banner clears the panel** (POE-244): get a `leaveMap`
  verdict with the layout panel open (R5 fires when the temple has what it needs
  from the map). The yellow banner must not overlap the side panel — at 1920×1080
  a centred banner reaches x 1200 and the panel's crop starts at 1131, so it will
  have moved up or left. Verify with the Debug-capture diff below rather than by
  eye: the panel's title and both architect blocks must still read.
- **The Show checkbox reaches the callout** (POE-244): Settings → Overlay
  Positions → Temple lists BOTH widgets. The kill callout's row shows
  `placed by the game` where the door diamond shows a rectangle, and clearing its
  Show toggle must take the callout off the screen while leaving the room widget
  up. A row that is missing, or a toggle that does nothing, is
  the regression: an anchored widget with no switch is the only overlay surface
  the user cannot turn off.
- **Config mode seeds the door widget where it actually sits** (POE-244): open a
  temple and let the widget be placed beside the game's diamond, then leave the
  temple and press Configure in Settings. The red frame must appear where the
  widget has been sitting, not at the shipped default in the top-left, and the
  bar must NOT carry the "shipped default" note. Then restart the app without
  opening a temple and press Configure: the frame is now at the shipped default
  and the bar must say so. Saving in the first case and re-entering a temple must
  leave the widget where it was saved.
- **The door widget ships somewhere sensible on a fresh profile** (POE-244):
  with no stored placement for `temple.door` (a fresh install, or the row deleted
  from `settings.json`), open a panel. The widget must appear below the panel and
  in the game diamond's column, over nothing the module reads. It follows the
  panel while it is unplaced, which is intended; the moment it is dragged and
  saved it stops moving, and THAT is the check — drag it, Save, re-open the
  panel on a different board, and it must stay where it was put.

## Adding an overlay

Start from the closest existing overlay type, then check applicable integration
points rather than cloning every step blindly:

- window and configuration labels in capabilities;
- settings field/default, getter/setter commands, persistence copy, and survival
  regression test when the overlay is configurable;
- creation, exact physical sizing, startup restoration, toggle, and position
  saving in the app layout;
- settings-page maps and sidebar/category controls when user-configurable;
- Rust focus/lab catch-up show-hide lists only for overlays that follow those
  lifecycles;
- an `/overlay/...` route and state catch-up/reconciliation appropriate to that
  overlay type;
- `set_overlay_clickthrough(label)` — for every overlay, so the hook can repair
  its `WS_EX_TRANSPARENT`;
- `set_overlay_hot_rects` from the window's own page ONLY if it has controls the
  player must be able to click; a display-only overlay declares nothing — and
  its `overlay-click` listener must be `getCurrentWebviewWindow().listen(…)`,
  never a bare `listen()` (see "Overlay types").

Verify desktop Svelte checks/unit tests and Rust tests after overlay changes.
