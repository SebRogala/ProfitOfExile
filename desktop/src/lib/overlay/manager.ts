/**
 * The window LABELS and MODULE IDS of the two module-coupled overlays.
 *
 * This file used to be a window manager as well — `showOverlay`,
 * `destroyOverlay`, `hideAllOverlays`, an `initFocusListener` and four more.
 * None of them had a caller (POE-225): every overlay this app builds is built
 * in `routes/(app)/+layout.svelte` or in `lib/pages/SettingsPage.svelte`,
 * because each needs exact physical sizing and its own creation ordering, and
 * game-focus show/hide is Rust's focus poller rather than a JS listener. The
 * dead half is gone; what is left is the pair of constants two windows have to
 * agree on.
 */

/**
 * The temple overlay's WINDOW LABEL.
 *
 * Every label must also appear in `src-tauri/capabilities/default.json`'s
 * `windows` list — guard 1 of `docs/OVERLAY-GUIDE.md`. A label missing there
 * leaves the Tauri APIs unavailable to that window, which fails as a window
 * that renders but cannot talk to Rust rather than as a build error.
 *
 * The four lab overlays are created in `routes/(app)/+layout.svelte` from their
 * persisted settings; this window is created by the same file but from the
 * `temple` MODULE flag instead. The WINDOW persists no geometry of its own — it
 * is the primary monitor (POE-225) — while the widgets inside it are persisted
 * per widget in `Settings.widgets`.
 */
export const TEMPLE_WINDOW_LABEL = 'temple';

/**
 * The temple MODULE's registry id — a different thing that happens to spell the
 * same word.
 *
 * The label above is ours to choose (it names a Tauri window and the
 * `/overlay/…` route under it); this one is `src-tauri/src/modules.rs`'s, and
 * renaming the module there would have to be followed here or the lifecycle
 * effect would read `ssot.modules[…]` as permanently `undefined` — a temple
 * overlay that never appears, with nothing failing. `manager.test.ts` pins it
 * against the Rust registry so that rename fails a test instead.
 */
export const TEMPLE_MODULE_ID = 'temple';

/**
 * The merc verdict overlay's WINDOW LABEL (POE-199).
 *
 * Same two-constant shape as the temple pair above and for the same reasons:
 * this one names a Tauri window and the `/overlay/mercenary` route under it,
 * and must appear in `src-tauri/capabilities/default.json`'s `windows` list.
 */
export const MERCENARY_WINDOW_LABEL = 'mercenary';

/**
 * The merc OCR MODULE's registry id — `src-tauri/src/modules.rs`'s spelling.
 *
 * One switch, three things since POE-199: the capture loop, the page's data and
 * this overlay window. A rename on the Rust side would leave
 * `ssot.modules[…]` permanently `undefined`, which the lifecycle reads
 * (correctly) as "not polled yet" — an overlay that never appears, with nothing
 * failing. `manager.test.ts` pins it against the registry source.
 */
export const MERCENARY_MODULE_ID = 'mercenary';
