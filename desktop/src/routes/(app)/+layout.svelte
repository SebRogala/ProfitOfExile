<script lang="ts">
	import '../../app.css';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import TopBar from '$lib/components/TopBar.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import { store, initStatusStore } from '$lib/stores/status.svelte';
	import { ssot, startSsotStore } from '$lib/stores/ssot.svelte';
	import { startRunRecorder } from '$lib/run-recorder';
	import { nav, viewToPath } from '$lib/stores/navigation.svelte';
	import { TEMPLE_MODULE_ID, TEMPLE_WINDOW_LABEL, destroyOverlay, isOverlayActive, readOverlayRegion } from '$lib/overlay/manager';
	import {
		TEMPLE_CREATE_TIMEOUT_MS,
		TEMPLE_GAVE_UP_NOTE,
		templeBegin,
		templeCreateWithTimeout,
		templeDesired,
		templeLifecycleInit,
		templeNextAction,
		templeRetryDelayMs,
		templeSettle
	} from '$lib/overlay/temple-lifecycle';
	import LabPage from '$lib/pages/LabPage.svelte';
	import SettingsPage from '$lib/pages/SettingsPage.svelte';
	import MercenariesPage from '$lib/pages/MercenariesPage.svelte';
	import TemplePage from '$lib/pages/TemplePage.svelte';
	import DevPage from '$lib/pages/DevPage.svelte';
	import IdentifyDialog from '$lib/components/IdentifyDialog.svelte';

	let { children } = $props();

	// Sidebar state: driven by store.status.sidebar_open (persisted in Rust settings).
	let sidebarOpen = $derived(store.status?.sidebar_open ?? true);

	function toggleSidebar() {
		const next = !sidebarOpen;
		invoke('set_sidebar_open', { open: next }).catch(e => console.error('set_sidebar_open failed:', e));
	}

	// Comparator overlay state
	let comparatorActive = $state(false);
	let comparatorWin = $state<any>(null);

	// Compass overlay state
	let compassActive = $state(false);
	let compassWin = $state<any>(null);

	// Path strip overlay state
	let pathstripActive = $state(false);
	let pathstripHasData = $state(false);
	let pathstripWin = $state<any>(null);

	// Timer overlay state
	let timerActive = $state(false);
	let timerWin = $state<any>(null);

	// Lab overlays category toggle
	let labOverlaysActive = $state(true);

	async function createComparatorOverlay(physX: number, physY: number) {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition } = await import('@tauri-apps/api/dpi');

		await destroyComparatorWindow();

		const win = new WebviewWindow('comparator', {
			url: '/overlay/comparator',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: false,
			shadow: false,
			skipTaskbar: true,
			width: 630,
			height: 250,
		});

		win.once('tauri://created', async () => {
			await win.setPosition(new PhysicalPosition(physX, physY));
			await invoke('set_overlay_clickthrough', { label: 'comparator', interactiveWidth: 48 })
				.catch(e => console.error('[overlay] click-through setup failed:', e));
			comparatorWin = win;
			comparatorActive = true;

			// Hide immediately if game is not focused — the focus poller
			// only handles transitions, so a window created while PoE is
			// not in the foreground would otherwise stay visible.
			try {
				const status = await invoke<any>('get_status');
				if (!status?.game_focused) {
					await win.hide();
				}
			} catch (e) {
				console.warn('[overlay] initial focus check failed:', e);
			}
		});
		win.once('tauri://error', (e: any) => {
			console.error('[overlay] comparator creation failed:', e);
		});
	}

	// Destroy the comparator window — retries up to 5 times for async cleanup.
	async function destroyComparatorWindow() {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		for (let i = 0; i < 5; i++) {
			const existing = await WebviewWindow.getByLabel('comparator');
			if (!existing) break;
			try { await existing.close(); } catch (_) {}
			try { await existing.destroy(); } catch (_) {}
			await new Promise(r => setTimeout(r, 100));
		}
		comparatorWin = null;
	}

	async function toggleComparatorOverlay() {
		if (comparatorActive) {
			await destroyComparatorWindow();
			comparatorActive = false;
			// Save disabled state
			const settings = await invoke<any>('get_comparator_overlay_settings').catch(e => { console.warn('[overlay] settings load failed:', e); return null; });
			await invoke('set_comparator_overlay_settings', {
				x: settings?.x ?? 100, y: settings?.y ?? 100,
				w: settings?.width ?? 600, h: settings?.height ?? 250,
				enabled: false,
			}).catch(e => console.warn('[overlay] settings operation failed:', e));
		} else {
			const settings = await invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_comparator_overlay_settings').catch(e => { console.warn('[overlay] settings load failed:', e); return null; });
			await createComparatorOverlay(
				settings?.x ?? 100,
				settings?.y ?? 100,
			);
			// Save enabled state
			await invoke('set_comparator_overlay_settings', {
				x: settings?.x ?? 100, y: settings?.y ?? 100,
				w: settings?.width ?? 600, h: settings?.height ?? 250,
				enabled: true,
			}).catch(e => console.warn('[overlay] settings operation failed:', e));
		}
	}

	async function createCompassOverlay(physX: number, physY: number, w = 300, h = 280) {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');

		await destroyCompassWindow();

		// Constructor takes logical pixels; w/h are physical — convert for initial size.
		// tauri://created sets exact physical size via PhysicalSize.
		const sf = await getCurrentWebviewWindow().scaleFactor().catch((e: any) => { console.warn('[overlay] scaleFactor failed, using 1:', e); return 1; });
		const win = new WebviewWindow('compass', {
			url: '/overlay/compass',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: true,
			shadow: false,
			skipTaskbar: true,
			width: Math.round(w / sf),
			height: Math.round(h / sf),
		});

		win.once('tauri://created', async () => {
			await win.setPosition(new PhysicalPosition(physX, physY));
			await win.setSize(new PhysicalSize(w, h));
			await invoke('set_overlay_clickthrough', { label: 'compass', interactiveWidth: 0 })
				.catch(e => console.error('[overlay] compass click-through setup failed:', e));
			compassWin = win;
			compassActive = true;

			// Hide immediately if game is not focused
			try {
				const status = await invoke<any>('get_status');
				if (!status?.game_focused) {
					await win.hide();
				}
			} catch (e) {
				console.warn('[overlay] compass initial focus check failed:', e);
			}
		});
		win.once('tauri://error', (e: any) => {
			console.error('[overlay] compass creation failed:', e);
		});
	}

	// Destroy the compass window — retries up to 5 times for async cleanup.
	async function destroyCompassWindow() {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		for (let i = 0; i < 5; i++) {
			const existing = await WebviewWindow.getByLabel('compass');
			if (!existing) break;
			try { await existing.close(); } catch (_) {}
			try { await existing.destroy(); } catch (_) {}
			await new Promise(r => setTimeout(r, 100));
		}
		compassWin = null;
	}

	async function toggleCompassOverlay() {
		if (compassActive) {
			await destroyCompassWindow();
			compassActive = false;
			const settings = await invoke<any>('get_compass_overlay_settings').catch(() => null);
			await invoke('set_compass_overlay_settings', {
				x: settings?.x ?? 100, y: settings?.y ?? 100,
				w: settings?.width ?? 300, h: settings?.height ?? 280,
				enabled: false,
			}).catch(e => console.warn('[overlay] compass settings operation failed:', e));

		} else {
			const settings = await invoke<any>('get_compass_overlay_settings').catch(() => null);
			await createCompassOverlay(settings?.x ?? 100, settings?.y ?? 100, settings?.width ?? 300, settings?.height ?? 280);
			await invoke('set_compass_overlay_settings', {
				x: settings?.x ?? 100, y: settings?.y ?? 100,
				w: settings?.width ?? 300, h: settings?.height ?? 280,
				enabled: true,
			}).catch(e => console.warn('[overlay] compass settings operation failed:', e));
		}
	}

	// --- Path strip overlay ---

	async function createPathstripOverlay(physX: number, physY: number, w = 450, h = 180) {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');

		await destroyPathstripWindow();

		// Constructor takes logical pixels; w/h are physical — convert for initial size.
		// tauri://created sets exact physical size via PhysicalSize.
		const sf = await getCurrentWebviewWindow().scaleFactor().catch((e: any) => { console.warn('[overlay] scaleFactor failed, using 1:', e); return 1; });
		const win = new WebviewWindow('pathstrip', {
			url: '/overlay/pathstrip',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: true,
			shadow: false,
			skipTaskbar: true,
			width: Math.round(w / sf),
			height: Math.round(h / sf),
		});

		win.once('tauri://created', async () => {
			await win.setPosition(new PhysicalPosition(physX, physY));
			await win.setSize(new PhysicalSize(w, h));
			await invoke('set_overlay_clickthrough', { label: 'pathstrip', interactiveWidth: 0 })
				.catch(e => console.error('[overlay] pathstrip click-through setup failed:', e));
			pathstripWin = win;
			pathstripActive = true;

			try {
				const status = await invoke<any>('get_status');
				if (!status?.game_focused) {
					await win.hide();
				}
			} catch (e) {
				console.warn('[overlay] pathstrip initial focus check failed:', e);
			}
		});
		win.once('tauri://error', (e: any) => {
			console.error('[overlay] pathstrip creation failed:', e);
		});
	}

	async function destroyPathstripWindow() {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		for (let i = 0; i < 5; i++) {
			const existing = await WebviewWindow.getByLabel('pathstrip');
			if (!existing) break;
			try { await existing.close(); } catch (_) {}
			try { await existing.destroy(); } catch (_) {}
			await new Promise(r => setTimeout(r, 100));
		}
		pathstripWin = null;
	}

	async function togglePathstripOverlay() {
		if (pathstripActive) {
			await destroyPathstripWindow();
			pathstripActive = false;
			const settings = await invoke<any>('get_pathstrip_overlay_settings').catch(() => null);
			await invoke('set_pathstrip_overlay_settings', {
				x: settings?.x ?? 100, y: settings?.y ?? 300,
				w: settings?.width ?? 450, h: settings?.height ?? 180,
				enabled: false,
			}).catch(e => console.warn('[overlay] pathstrip settings operation failed:', e));
		} else {
			const settings = await invoke<any>('get_pathstrip_overlay_settings').catch(() => null);
			await createPathstripOverlay(settings?.x ?? 100, settings?.y ?? 300, settings?.width ?? 450, settings?.height ?? 180);
			await invoke('set_pathstrip_overlay_settings', {
				x: settings?.x ?? 100, y: settings?.y ?? 300,
				w: settings?.width ?? 450, h: settings?.height ?? 180,
				enabled: true,
			}).catch(e => console.warn('[overlay] pathstrip settings operation failed:', e));
		}
	}

	// --- Timer overlay ---

	async function createTimerOverlay(physX: number, physY: number, w = 160, h = 50) {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');

		await destroyTimerWindow();

		// Constructor takes logical pixels; w/h are physical — convert for initial size.
		// tauri://created sets exact physical size via PhysicalSize.
		const sf = await getCurrentWebviewWindow().scaleFactor().catch((e: any) => { console.warn('[overlay] scaleFactor failed, using 1:', e); return 1; });
		const win = new WebviewWindow('timer', {
			url: '/overlay/timer',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: true,
			shadow: false,
			skipTaskbar: true,
			width: Math.round(w / sf),
			height: Math.round(h / sf),
		});

		win.once('tauri://created', async () => {
			await win.setPosition(new PhysicalPosition(physX, physY));
			await win.setSize(new PhysicalSize(w, h));
			await invoke('set_overlay_clickthrough', { label: 'timer', interactiveWidth: 0 })
				.catch(e => console.error('[overlay] timer click-through setup failed:', e));
			timerWin = win;
			timerActive = true;

			try {
				const status = await invoke<any>('get_status');
				if (!status?.game_focused) {
					await win.hide();
				}
			} catch (e) {
				console.warn('[overlay] timer initial focus check failed:', e);
			}
		});
		win.once('tauri://error', (e: any) => {
			console.error('[overlay] timer creation failed:', e);
		});
	}

	async function destroyTimerWindow() {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		for (let i = 0; i < 5; i++) {
			const existing = await WebviewWindow.getByLabel('timer');
			if (!existing) break;
			try { await existing.close(); } catch (_) {}
			try { await existing.destroy(); } catch (_) {}
			await new Promise(r => setTimeout(r, 100));
		}
		timerWin = null;
	}

	async function toggleTimerOverlay() {
		if (timerActive) {
			await destroyTimerWindow();
			timerActive = false;
			const settings = await invoke<any>('get_timer_overlay_settings').catch(() => null);
			await invoke('set_timer_overlay_settings', {
				x: settings?.x ?? 100, y: settings?.y ?? 500,
				w: settings?.width ?? 160, h: settings?.height ?? 50,
				enabled: false,
			}).catch(e => console.warn('[overlay] timer settings operation failed:', e));
		} else {
			const settings = await invoke<any>('get_timer_overlay_settings').catch(() => null);
			await createTimerOverlay(settings?.x ?? 100, settings?.y ?? 500, settings?.width ?? 160, settings?.height ?? 50);
			await invoke('set_timer_overlay_settings', {
				x: settings?.x ?? 100, y: settings?.y ?? 500,
				w: settings?.width ?? 160, h: settings?.height ?? 50,
				enabled: true,
			}).catch(e => console.warn('[overlay] timer settings operation failed:', e));
		}
	}

	// --- Temple overlay (POE-171) ---
	//
	// Display-only, and coupled to the `temple` MODULE flag rather than to an
	// overlay setting of its own: one switch governs the capture loop AND this
	// window, because a temple overlay with no reader behind it can only ever
	// show a stale board. `docs/OVERLAY-GUIDE.md` guards, and where each is met:
	//
	//  1. capabilities — the `temple` label is in capabilities/default.json.
	//  2/3. physical vs logical — the constructor takes LOGICAL pixels, so the
	//    physical constants below are divided by Tauri's `scaleFactor()`, and
	//    the exact `PhysicalPosition`/`PhysicalSize` are applied in
	//    `tauri://created`. `window.devicePixelRatio` is not used.
	//  4. move, not recreate — this window is never repositioned, so there is
	//    no destroy/recreate cycle to avoid. It is built and torn down only on
	//    the module flag's transitions and on the bounded creation retry, and
	//    `temple-lifecycle.ts` orders all of them so two never overlap.
	//  5. settings survival — DELIBERATELY not applicable: the temple overlay
	//    persists nothing, so `persist_overlay_settings` has no field of ours to
	//    copy and its survival test has nothing to cover. Making the position
	//    persistent means a settings field, a getter/setter pair and an entry in
	//    that regression test, all Rust — deferred, not forgotten.
	//  6. error visibility — every failure below goes through `logTemple`, so it
	//    reaches `app_log_from_frontend` (the LOGS channel, and the only one
	//    readable in a shipped build) with the console as a second copy. Nothing
	//    here swallows a failure, and a creation that cannot be completed tears
	//    down what it half-built rather than leaving it standing.
	//
	// Visibility with the game is NOT decided here. The Rust focus poller shows
	// and hides the `temple` window on the game-focus transition, exactly as it
	// does the comparator; the route decides only whether it has a board worth
	// drawing.
	const TEMPLE_OVERLAY_X = 40;
	const TEMPLE_OVERLAY_Y = 40;
	const TEMPLE_OVERLAY_W = 620;
	const TEMPLE_OVERLAY_H = 260;

	/**
	 * Guard 6's channel.
	 *
	 * `console` alone is a message nobody can read: a shipped build has no
	 * devtools, so an overlay that failed to build would fail invisibly. The app
	 * log is the surface the user can actually open, and the console copy is
	 * kept for `npm run dev`.
	 */
	function logTemple(msg: string): void {
		console.warn(`[overlay] temple: ${msg}`);
		invoke('app_log_from_frontend', { msg: `[temple-overlay] ${msg}` })
			.catch(e => console.error('[overlay] temple: app log unreachable:', e));
	}

	/** Ordering state — see `$lib/overlay/temple-lifecycle`. Deliberately NOT a
	 *  rune: nothing renders from it, and the effect below must depend on the
	 *  module flag alone. */
	let templeLifecycle = templeLifecycleInit();
	/** Serialises create/destroy so two fast module toggles cannot interleave. */
	let templeOverlayWork: Promise<void> = Promise.resolve();
	/** The pending creation retry, so a module toggle can cancel it. */
	let templeRetryTimer: ReturnType<typeof setTimeout> | null = null;

	function cancelTempleRetry(): void {
		if (templeRetryTimer === null) return;
		clearTimeout(templeRetryTimer);
		templeRetryTimer = null;
	}

	/**
	 * Queue another creation attempt, or say out loud that we have stopped.
	 *
	 * A failed creation used to be terminal and silent — the module read as on,
	 * the window was never built, and nothing said so.
	 */
	function scheduleTempleRetry(): void {
		if (templeLifecycle.gaveUp) {
			logTemple(
				`creation failed ${templeLifecycle.attempts} times — ${TEMPLE_GAVE_UP_NOTE}`
			);
			return;
		}
		if (templeNextAction(templeLifecycle) !== 'create') return;
		const delay = templeRetryDelayMs(templeLifecycle.attempts);
		logTemple(`creation attempt ${templeLifecycle.attempts} failed — retrying in ${delay} ms`);
		cancelTempleRetry();
		templeRetryTimer = setTimeout(() => {
			templeRetryTimer = null;
			pumpTempleOverlay();
		}, delay);
	}

	/**
	 * Run whatever the scheduler asks for next, then ask again.
	 *
	 * Every step is appended to `templeOverlayWork`, so the Tauri calls happen
	 * one at a time however fast the flag moves.
	 */
	function pumpTempleOverlay(): void {
		const action = templeNextAction(templeLifecycle);
		if (action === 'none') return;
		templeLifecycle = templeBegin(templeLifecycle, action);
		templeOverlayWork = templeOverlayWork
			.then(async () => {
				const ok =
					action === 'create'
						? // Bounded, not awaited forever: `createTempleOverlay`
							// settles from a Tauri event, and an event that never
							// arrives would leave `pending` set to `'create'` for
							// the life of the process — after which no module-off
							// could tear the window down. See the constant.
							await templeCreateWithTimeout(createTempleOverlay, () =>
								logTemple(
									`creation did not settle within ${TEMPLE_CREATE_TIMEOUT_MS} ms — counting it as failed`
								)
							)
						: await destroyTempleWindow();
				templeLifecycle = templeSettle(templeLifecycle, ok);
				if (action === 'create' && !ok) {
					scheduleTempleRetry();
					return;
				}
				pumpTempleOverlay();
			})
			.catch(e => {
				// Neither step throws by contract — both report failure as
				// `false`. If one ever does, the action still has to be settled
				// or `pending` would stay set and no window would be built again.
				templeLifecycle = templeSettle(templeLifecycle, false);
				logTemple(`lifecycle step '${action}' threw: ${e}`);
			});
	}

	/**
	 * Build the window. Resolves true only once it is positioned, sized and
	 * click-through — a half-built one resolves false and is torn down.
	 *
	 * The promise settles from `tauri://created` / `tauri://error` rather than
	 * from the constructor returning, which is what makes the serialisation
	 * above real: the previous code resolved as soon as the constructor had been
	 * called, so an off toggle could run its destroy sweep before the window it
	 * was meant to remove existed.
	 */
	async function createTempleOverlay(): Promise<boolean> {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');

		await destroyTempleWindow();

		// Constructor dimensions are LOGICAL; the constants are physical.
		const sf = await getCurrentWebviewWindow()
			.scaleFactor()
			.catch((e: any) => {
				logTemple(`scaleFactor failed, using 1: ${e}`);
				return 1;
			});
		const win = new WebviewWindow(TEMPLE_WINDOW_LABEL, {
			url: `/overlay/${TEMPLE_WINDOW_LABEL}`,
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			// Nothing drags or resizes this window: the geometry is fixed by the
			// constants above and no drag handle is drawn, so a resizable frame
			// only offers an edge that would desync the board from its position.
			resizable: false,
			shadow: false,
			skipTaskbar: true,
			width: Math.round(TEMPLE_OVERLAY_W / sf),
			height: Math.round(TEMPLE_OVERLAY_H / sf),
		});

		return await new Promise<boolean>((resolve) => {
			let settled = false;
			const finish = (ok: boolean) => {
				if (settled) return;
				settled = true;
				resolve(ok);
			};

			win.once('tauri://created', async () => {
				try {
					await win.setPosition(new PhysicalPosition(TEMPLE_OVERLAY_X, TEMPLE_OVERLAY_Y));
					await win.setSize(new PhysicalSize(TEMPLE_OVERLAY_W, TEMPLE_OVERLAY_H));
					// Display-only: interactive width 0, like compass/pathstrip/timer.
					// Do NOT copy the comparator's 48px right-edge zone here.
					await invoke('set_overlay_clickthrough', {
						label: TEMPLE_WINDOW_LABEL,
						interactiveWidth: 0,
					});
				} catch (e) {
					// A window that is transparent, always-on-top and NOT
					// click-through eats clicks over the game with nothing
					// visible to explain why. Half-built is worse than absent,
					// so it goes.
					logTemple(`setup failed, destroying the half-built window: ${e}`);
					await destroyTempleWindow();
					finish(false);
					return;
				}
				// Soft step: the focus poller only acts on transitions, so a
				// window built while PoE is not in the foreground would sit on
				// the desktop until the next alt-tab. Failing this leaves a
				// visible window, not a broken one — logged, not fatal.
				try {
					const status = await invoke<any>('get_status');
					if (!status?.game_focused) await win.hide();
				} catch (e) {
					logTemple(`initial focus check failed, window left visible: ${e}`);
				}
				finish(true);
			}).catch(e => {
				logTemple(`could not listen for tauri://created: ${e}`);
				finish(false);
			});

			win.once('tauri://error', (e: any) => {
				logTemple(`creation failed: ${JSON.stringify(e?.payload ?? e)}`);
				finish(false);
			}).catch(e => {
				logTemple(`could not listen for tauri://error: ${e}`);
				finish(false);
			});
		});
	}

	/** Tear the window down. Returns whether the label is gone afterwards. */
	async function destroyTempleWindow(): Promise<boolean> {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		for (let i = 0; i < 5; i++) {
			const existing = await WebviewWindow.getByLabel(TEMPLE_WINDOW_LABEL).catch(e => {
				logTemple(`lookup during destroy failed: ${e}`);
				return null;
			});
			if (!existing) return true;
			// Both are attempted: `close` is the polite path and `destroy` the
			// one that actually frees the label. Tauri's cleanup is async, hence
			// the re-check rather than a single call.
			try { await existing.close(); } catch (e) { logTemple(`close attempt ${i + 1} failed: ${e}`); }
			try { await existing.destroy(); } catch (e) { logTemple(`destroy attempt ${i + 1} failed: ${e}`); }
			await new Promise(r => setTimeout(r, 100));
		}
		logTemple('window still present after 5 close/destroy rounds — giving the label up');
		return false;
	}

	// The module flag is the single switch. `undefined` means the first poll has
	// not answered yet, and is treated as "not yet known" rather than as off —
	// tearing a window down on a value nobody has reported would fight the
	// startup poll.
	$effect(() => {
		const enabled = ssot.modules[TEMPLE_MODULE_ID];
		if (enabled === undefined) return;
		if (templeLifecycle.desired === enabled) return;
		templeLifecycle = templeDesired(templeLifecycle, enabled);
		// A toggle is the one thing that clears a spent retry budget, so a
		// scheduled attempt from the previous flag value has nothing left to do.
		cancelTempleRetry();
		pumpTempleOverlay();
	});

	// --- Lab overlays category toggle ---

	async function toggleLabOverlays() {
		const next = !labOverlaysActive;
		labOverlaysActive = next;
		await invoke('set_lab_overlays_enabled', { enabled: next }).catch(e => console.warn('[overlay] set_lab_overlays_enabled failed:', e));
		if (next) {
			// Enable all — respect each overlay's individual enabled state
			if (!comparatorActive) await toggleComparatorOverlay();
			if (!compassActive) await toggleCompassOverlay();
			if (!pathstripActive) await togglePathstripOverlay();
			if (!timerActive) await toggleTimerOverlay();
		} else {
			// Disable all
			if (comparatorActive) await toggleComparatorOverlay();
			if (compassActive) await toggleCompassOverlay();
			if (pathstripActive) await togglePathstripOverlay();
			if (timerActive) await toggleTimerOverlay();
		}
	}

	// Save current overlay positions/sizes to Rust settings.
	// Called on LabExited (captures user's resize during lab run).
	async function saveOverlayPositions() {
		if (compassWin) {
			try {
				const pos = await compassWin.outerPosition();
				const size = await compassWin.outerSize();
				await invoke('set_compass_overlay_settings', {
					x: pos.x, y: pos.y, w: size.width, h: size.height, enabled: true,
				});
			} catch (e) { console.warn('[overlay] failed to save compass position:', e); }
		}
		if (pathstripWin) {
			try {
				const pos = await pathstripWin.outerPosition();
				const size = await pathstripWin.outerSize();
				await invoke('set_pathstrip_overlay_settings', {
					x: pos.x, y: pos.y, w: size.width, h: size.height, enabled: true,
				});
			} catch (e) { console.warn('[overlay] failed to save pathstrip position:', e); }
		}
		if (timerWin) {
			try {
				const pos = await timerWin.outerPosition();
				const size = await timerWin.outerSize();
				await invoke('set_timer_overlay_settings', {
					x: pos.x, y: pos.y, w: size.width, h: size.height, enabled: true,
				});
			} catch (e) { console.warn('[overlay] failed to save timer position:', e); }
		}
	}

	// Ctrl+Shift+F12 toggles debug mode (devtools + force-show overlays)
	let debugMode = $state(false);
	// Ctrl+Shift+I opens the identify dialog (device alias registration)
	let identifyOpen = $state(false);

	$effect(() => {
		function handleKeydown(e: KeyboardEvent) {
			if (e.ctrlKey && e.shiftKey && e.key === 'F12') {
				e.preventDefault();
				debugMode = !debugMode;
				// Devtools has no JS API in Tauri 2 — `openDevtools()` on the window
				// object is not a function and threw before the .catch could apply.
				invoke('set_devtools', { open: debugMode }).catch((e: any) => console.warn('[debug] set_devtools failed:', e));
				if (debugMode) {
					// Force-show overlays regardless of game focus
					invoke('force_show_overlays').catch((e: any) => console.warn('[debug] force_show_overlays failed:', e));
					console.log('[debug] Debug mode ON — overlays force-shown');
				} else {
					console.log('[debug] Debug mode OFF');
				}
			}
			if (e.ctrlKey && e.shiftKey && e.key === 'I') {
				e.preventDefault();
				identifyOpen = !identifyOpen;
			}
		}
		window.addEventListener('keydown', handleKeydown);
		return () => window.removeEventListener('keydown', handleKeydown);
	});

	// Initialize event listeners — runs on module load (client-side only due to ssr:false)
	// No cleanup needed — desktop app layout never unmounts.
	initStatusStore().catch(e => console.error('[layout] initStatusStore failed:', e));

	// Start the cross-window SSOT poll. The main window may lean on the eager
	// ssot-changed nudge, but polling get_ssot is consistent with the overlays
	// and cheap for a low-churn slice. No cleanup — this layout never unmounts.
	startSsotStore();

	// Record every lab run, whether or not the timer overlay is enabled. The
	// recorder used to live in the overlay's webview, so a disabled overlay
	// meant the Runs tab silently collected nothing.
	startRunRecorder();

	// Reposition comparator overlay when settings page closes a config overlay.
	// The config overlay destroy can leave Win32 mouse capture stuck; this move resets focus.
	// Only active while a config overlay is open (overlay-config-start/end events).
	let configOverlayCleanup: (() => void) | null = null;
	listen('overlay-config-start', async () => {
		if (configOverlayCleanup) return; // already listening
		const unlisten = await listen('overlay-toggle-reset', async () => {
			if (comparatorActive) {
				// Move existing overlay to saved position — no destroy/recreate needed.
				const settings = await invoke<any>('get_comparator_overlay_settings').catch(() => null);
				if (settings) {
					await invoke('move_overlay', { label: 'comparator', x: settings.x, y: settings.y, w: settings.width ?? 630, h: settings.height ?? 250 })
						.catch(e => console.warn('[overlay] comparator move failed:', e));
				}
			}
			if (compassActive) {
				const compassSettings = await invoke<any>('get_compass_overlay_settings').catch(() => null);
				if (compassSettings) {
					await invoke('move_overlay', { label: 'compass', x: compassSettings.x, y: compassSettings.y, w: compassSettings.width ?? 300, h: compassSettings.height ?? 280 })
						.catch(e => console.warn('[overlay] compass move failed:', e));
				}
			}
			if (pathstripActive) {
				const pathstripSettings = await invoke<any>('get_pathstrip_overlay_settings').catch(() => null);
				if (pathstripSettings) {
					await invoke('move_overlay', { label: 'pathstrip', x: pathstripSettings.x, y: pathstripSettings.y, w: pathstripSettings.width ?? 700, h: pathstripSettings.height ?? 80 })
						.catch(e => console.warn('[overlay] pathstrip move failed:', e));
				}
			}
			if (timerActive) {
				const timerSettings = await invoke<any>('get_timer_overlay_settings').catch(() => null);
				if (timerSettings) {
					await invoke('move_overlay', { label: 'timer', x: timerSettings.x, y: timerSettings.y, w: timerSettings.width ?? 160, h: timerSettings.height ?? 50 })
						.catch(e => console.warn('[overlay] timer move failed:', e));
				}
			}
		});
		configOverlayCleanup = unlisten;
	});
	listen('overlay-config-end', () => {
		if (configOverlayCleanup) {
			configOverlayCleanup();
			configOverlayCleanup = null;
		}
	});
	// Focus-based overlay show/hide handled by Rust focus poller (GetForegroundWindow)

	// Auto-restore comparator overlay if it was enabled in previous session
	invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_comparator_overlay_settings')
		.then((settings) => {
			if (settings?.enabled) {
				createComparatorOverlay(settings.x, settings.y);
			}
		})
		.catch(e => console.warn('[overlay] comparator settings operation failed:', e));

	// Restore enabled overlays on startup — created but HIDDEN.
	// 'Enabled' = user wants the overlay (persistent preference).
	// 'Visible' = currently showing (transient, driven by lab events).
	// PlazaEntered → show, LabExited → hide. Toggle button changes 'enabled'.
	invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_compass_overlay_settings')
		.then(async (settings) => {
			if (settings?.enabled) {
				await createCompassOverlay(settings.x, settings.y, settings.width ?? 300, settings.height ?? 280);
				// Start hidden — will show on PlazaEntered
				if (compassWin) await compassWin.hide().catch(() => {});
			}
		})
		.catch(e => console.warn('[overlay] compass restore failed:', e));

	invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_pathstrip_overlay_settings')
		.then(async (settings) => {
			if (settings?.enabled) {
				pathstripActive = true;
				const hasData = await checkPathstripData();
				if (hasData) {
					await createPathstripOverlay(settings!.x, settings!.y, settings!.width ?? 450, settings!.height ?? 180);
					// Start hidden — will show on PlazaEntered
					if (pathstripWin) await pathstripWin.hide().catch(() => {});
				}
			}
		})
		.catch(e => console.warn('[overlay] pathstrip restore failed:', e));

	// Restore timer overlay on startup (hidden, shown on PlazaEntered)
	invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_timer_overlay_settings')
		.then(async (settings) => {
			if (settings?.enabled) {
				await createTimerOverlay(settings.x, settings.y, settings.width ?? 160, settings.height ?? 50);
				if (timerWin) await timerWin.hide().catch(() => {});
			}
		})
		.catch(e => console.warn('[overlay] timer restore failed:', e));

	// Restore lab overlays category toggle state
	invoke<boolean>('get_lab_overlays_enabled')
		.then((enabled) => { labOverlaysActive = enabled; })
		.catch(e => console.warn('[overlay] get_lab_overlays_enabled failed:', e));

	// Check if lab layout is available on the server.
	async function checkPathstripData(): Promise<boolean> {
		try {
			const status = await invoke<any>('get_status');
			const serverUrl = status?.server_url;
			if (!serverUrl) return false;
			for (const diff of ['Uber', 'Merciless', 'Cruel', 'Normal']) {
				const r = await fetch(`${serverUrl}/api/lab/layout/${diff}`);
				if (r.ok) {
					pathstripHasData = true;
					return true;
				}
			}
		} catch (e) {
			console.warn('[pathstrip] data check failed:', e);
		}
		pathstripHasData = false;
		return false;
	}

	// Check on startup with retry — server may not be ready immediately.
	(async () => {
		for (let i = 0; i < 3; i++) {
			if (await checkPathstripData()) return;
			await new Promise(r => setTimeout(r, 2000 * (i + 1)));
		}
	})();

	// Show/hide overlays based on lab events.
	// Overlays are created on startup (hidden). PlazaEntered shows them, LabExited hides them.
	listen('lab-nav', async (event: any) => {
		if (event.payload?.type === 'PlazaEntered') {
			// Show existing overlay windows (or create if not yet created)
			const compassSettings = await invoke<any>('get_compass_overlay_settings').catch(() => null);
			if (compassSettings?.enabled) {
				if (compassWin) {
					await compassWin.show().catch(() => {});
				} else {
					await createCompassOverlay(compassSettings.x, compassSettings.y, compassSettings.width ?? 300, compassSettings.height ?? 280);
				}
			}
			const pathstripSettings = await invoke<any>('get_pathstrip_overlay_settings').catch(() => null);
			if (pathstripSettings?.enabled) {
				if (pathstripWin) {
					await pathstripWin.show().catch(() => {});
				} else if (pathstripHasData) {
					await createPathstripOverlay(pathstripSettings.x, pathstripSettings.y, pathstripSettings.width ?? 450, pathstripSettings.height ?? 180);
				}
			}
			const timerSettings = await invoke<any>('get_timer_overlay_settings').catch(() => null);
			if (timerSettings?.enabled) {
				if (timerWin) {
					await timerWin.show().catch(() => {});
				} else {
					await createTimerOverlay(timerSettings.x, timerSettings.y, timerSettings.width ?? 160, timerSettings.height ?? 50);
				}
			}
		}
		if (event.payload?.type === 'LabExited') {
			// Persist current overlay positions/sizes before hiding
			await saveOverlayPositions();
			if (compassWin) {
				await compassWin.hide().catch(() => {});
			}
			if (pathstripWin) {
				await pathstripWin.hide().catch(() => {});
			}
			if (timerWin) {
				await timerWin.hide().catch(() => {});
			}
		}
	});
</script>

<div class="app-shell">
	<TopBar status={store.status} />
	<div class="app-body">
		<Sidebar open={sidebarOpen} currentPath={viewToPath(nav.view)} onToggle={toggleSidebar}
			comparatorActive={comparatorActive} gameFocused={store.status?.game_focused ?? false} onToggleComparator={toggleComparatorOverlay}
			compassActive={compassActive} onToggleCompass={toggleCompassOverlay}
			pathstripActive={pathstripActive} pathstripHasData={pathstripHasData} onTogglePathstrip={togglePathstripOverlay}
			timerActive={timerActive} onToggleTimer={toggleTimerOverlay}
			labOverlaysActive={labOverlaysActive} onToggleLabOverlays={toggleLabOverlays} />
		<main class="content">
			<div class="route-render-placeholder" aria-hidden="true">
				{@render children()}
			</div>
			<div class:view-hidden={nav.view !== 'lab'}>
				<LabPage />
			</div>
			<div class:view-hidden={nav.view !== 'settings'}>
				<SettingsPage />
			</div>
			<div class:view-hidden={nav.view !== 'mercenaries'}>
				<MercenariesPage />
			</div>
			<div class:view-hidden={nav.view !== 'temple'}>
				<TemplePage />
			</div>
			{#if import.meta.env.DEV}
				<div class:view-hidden={nav.view !== 'dev'}>
					<DevPage />
				</div>
			{/if}
		</main>
	</div>
</div>

<IdentifyDialog bind:open={identifyOpen} />

<style>
	/* The app is a fixed 100vh shell whose ONLY scroller is main.content —
	 * the document itself must never scroll, or its scrollbar doubles the
	 * content one (seen on the Mercenaries matrix, where a stray pixel of
	 * document overflow produced a second bar). Scoped here, not app.css,
	 * so overlay windows keep their own scroll behaviour. */
	:global(html),
	:global(body) {
		height: 100%;
		overflow: hidden;
	}

	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
	}

	.app-body {
		display: flex;
		flex-direction: row;
		flex: 1;
		overflow: hidden;
	}

	.content {
		flex: 1;
		overflow-y: auto;
		padding: 16px;
	}

	.view-hidden {
		display: none;
	}

	.route-render-placeholder {
		display: none;
	}
</style>
