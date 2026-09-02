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
	import { nav, viewToPath, type View } from '$lib/stores/navigation.svelte';
	import {
		hasFeature,
		MERC_FEATURE,
		EXCHANGE_FEATURE,
		TEMPLE_FEATURE
	} from '$lib/stores/entitlements.svelte';
	import {
		MERCENARY_MODULE_ID,
		MERCENARY_WINDOW_LABEL,
		TEMPLE_MODULE_ID,
		TEMPLE_WINDOW_LABEL
	} from '$lib/overlay/manager';
	import { moduleOverlayDriver } from '$lib/overlay/module-lifecycle';
	import { MERC_OVERLAY_DEFAULTS, physicalGeometry } from '$lib/overlay/overlay-defaults';
	import LabPage from '$lib/pages/LabPage.svelte';
	import SettingsPage from '$lib/pages/SettingsPage.svelte';
	import MercenariesPage from '$lib/pages/MercenariesPage.svelte';
	import TemplePage from '$lib/pages/TemplePage.svelte';
	import CurrencyExchangePage from '$lib/pages/CurrencyExchangePage.svelte';
	import DevPage from '$lib/pages/DevPage.svelte';
	import IdentifyDialog from '$lib/components/IdentifyDialog.svelte';

	let { children } = $props();

	// Sidebar state: driven by store.status.sidebar_open (persisted in Rust settings).
	let sidebarOpen = $derived(store.status?.sidebar_open ?? true);

	/**
	 * The feature each hidden view is gated on (POE-203). A view absent from this
	 * map is visible to every device; the three listed here are drawn only where
	 * the server granted the named feature — hiding, not securing: the code ships
	 * in every build.
	 *
	 * ONE table, read by both the per-view flags below and by `visibleView`, so a
	 * page mount and the route fallback that protects it cannot disagree about
	 * which feature a view needs.
	 */
	const VIEW_FEATURES: Partial<Record<View, string>> = {
		mercenaries: MERC_FEATURE,
		temple: TEMPLE_FEATURE,
		'currency-exchange': EXCHANGE_FEATURE
	};

	/** Whether this device may see a view. Reactive — `hasFeature` reads the store. */
	function viewGranted(view: View): boolean {
		const feature = VIEW_FEATURES[view];
		return !feature || hasFeature(feature);
	}

	/** Gates the Mercenaries page and the verdict overlay. */
	let mercGranted = $derived(viewGranted('mercenaries'));
	/** Gates the Temple page and the temple overlay. */
	let templeGranted = $derived(viewGranted('temple'));
	/** Gates the Currency Exchange page (no overlay, no module). */
	let exchangeGranted = $derived(viewGranted('currency-exchange'));

	/**
	 * The view actually on screen.
	 *
	 * `nav.view` restores the persisted `navView` pref, which can name a view
	 * this device may no longer see — and does on EVERY launch of an entitled
	 * device, because entitlements default to none until `/api/device/me`
	 * answers. Falling back here rather than calling `nav.go('/')` is deliberate:
	 * a write would persist the fallback and cost an entitled user their last
	 * tool the first time they launched offline.
	 */
	let visibleView = $derived(viewGranted(nav.view) ? nav.view : 'lab');

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
			await invoke('set_overlay_clickthrough', { label: 'comparator' })
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
			await invoke('set_overlay_clickthrough', { label: 'compass' })
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
			await invoke('set_overlay_clickthrough', { label: 'pathstrip' })
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
			await invoke('set_overlay_clickthrough', { label: 'timer' })
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
	//  2/3. physical vs logical — the window IS the primary monitor (POE-225), so
	//    the monitor's physical position and size are divided by ITS scale
	//    factor for the constructor, which takes LOGICAL pixels, and the exact
	//    `PhysicalPosition`/`PhysicalSize` are applied in `tauri://created`.
	//    `window.devicePixelRatio` is not used.
	//  4. move, not recreate — this window is never repositioned, so there is
	//    no destroy/recreate cycle to avoid. It is built and torn down only on
	//    the module flag's transitions and on the bounded creation retry, and
	//    `module-lifecycle.ts` orders all of them so two never overlap.
	//  5. settings survival — the WINDOW persists nothing, because it is the
	//    primary monitor and there is nothing about it a user could choose. What
	//    IS persisted is where the user put each WIDGET inside it
	//    (`Settings.widgets`, keyed `"temple.<widget>"`), and that map is owned
	//    by an `AppState` mutex, so it travels through `from_state` and is
	//    covered by the round-trip tests in `settings.rs` rather than by
	//    `persist_overlay_settings`.
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

	/** The create/destroy ordering, and the bounded retry — see
	 *  `$lib/overlay/module-lifecycle`. The driver owns the mutable state; this
	 *  file owns the Tauri work it orders. */
	const templeOverlay = moduleOverlayDriver(
		{ label: TEMPLE_WINDOW_LABEL, moduleId: TEMPLE_MODULE_ID },
		{ create: createTempleOverlay, destroy: destroyTempleWindow, log: logTemple }
	);

	/**
	 * Build the window. Resolves true only once it is positioned, sized and
	 * click-through — a half-built one resolves false and is torn down.
	 *
	 * The promise settles from `tauri://created` / `tauri://error` rather than
	 * from the constructor returning, which is what makes the driver's
	 * serialisation real: the previous code resolved as soon as the constructor
	 * had been called, so an off toggle could run its destroy sweep before the
	 * window it was meant to remove existed.
	 */
	async function createTempleOverlay(): Promise<boolean> {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');
		const { currentMonitor, primaryMonitor } = await import('@tauri-apps/api/window');

		await destroyTempleWindow();

		// The window IS the primary monitor (POE-225): one fullscreen,
		// click-through canvas per module, with the widgets placed inside it.
		// `currentMonitor()` is the fallback rather than a constant, because a
		// constant would be a guess at a resolution — and a window sized wrong
		// puts every widget's persisted physical coordinate in the wrong place.
		const monitor =
			(await primaryMonitor().catch((e: any) => {
				logTemple(`primaryMonitor failed, trying currentMonitor: ${e}`);
				return null;
			})) ??
			(await currentMonitor().catch((e: any) => {
				logTemple(`currentMonitor failed too: ${e}`);
				return null;
			}));
		if (!monitor) {
			// No monitor, no canvas. Refusing is what the bounded retry in
			// `module-lifecycle.ts` is for; guessing a size would place the
			// widgets somewhere the player never put them.
			logTemple('no monitor to build the overlay on — not creating it');
			return false;
		}
		// Constructor dimensions are LOGICAL; a monitor reports PHYSICAL.
		const sf = monitor.scaleFactor > 0 ? monitor.scaleFactor : 1;
		const win = new WebviewWindow(TEMPLE_WINDOW_LABEL, {
			url: `/overlay/${TEMPLE_WINDOW_LABEL}${debugMode ? '?debug' : ''}`,
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			// Nothing drags or resizes this window: it is the monitor. A
			// resizable frame would only offer an edge that desyncs every
			// widget's persisted position from the canvas it was measured in.
			resizable: false,
			shadow: false,
			skipTaskbar: true,
			// Built without stealing the foreground. Tauri focuses a new window
			// by default, and this one is created while the game may be in
			// front — taking focus would pull the player out of PoE for the
			// second before the click-through thread below runs.
			focus: false,
			x: Math.round(monitor.position.x / sf),
			y: Math.round(monitor.position.y / sf),
			width: Math.round(monitor.size.width / sf),
			height: Math.round(monitor.size.height / sf),
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
					await win.setPosition(new PhysicalPosition(monitor.position.x, monitor.position.y));
					await win.setSize(new PhysicalSize(monitor.size.width, monitor.size.height));
					// Registration, not interactivity: the window declares no hot
					// rects of its own, and `WidgetHost.svelte` declares one per
					// `[data-hot]` element its widgets draw. Registering is what
					// lets the hook repair the WS_EX_TRANSPARENT WebView2 strips.
					//
					// MEASURED: this command is fire-and-forget. Rust spawns a
					// thread that sleeps ~1 s before `set_ignore_cursor_events`
					// (the WebView2 HWND is not available sooner — see the guide's
					// runtime-earned observations), so the await below returns
					// long before click-through is installed and this try/catch
					// cannot see a failure in it. The window is briefly
					// INTERACTIVE after creation; a click landing in that second
					// hits the board instead of the game.
					await invoke('set_overlay_clickthrough', {
						label: TEMPLE_WINDOW_LABEL,
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
	// startup poll. Re-reporting the same flag is a no-op inside the driver.
	$effect(() => {
		const enabled = ssot.modules[TEMPLE_MODULE_ID];
		if (enabled === undefined) return;
		// The feature gate is ANDed in rather than short-circuiting the effect:
		// a device that loses the grant with the module flag still set must have
		// the window taken down, not left standing (POE-203). This takes the
		// WINDOW down, not the module — the Rust temple module keeps running on
		// its own flag, because the gate is hiding and not securing.
		templeOverlay.setDesired(enabled && templeGranted);
	});

	// --- Merc verdict overlay (POE-199) ---
	//
	// The temple overlay's sibling: display-only, coupled to the `mercenary`
	// MODULE flag rather than to an overlay toggle, and driven by the same
	// lifecycle. It differs in ONE thing — its geometry is persisted
	// (`mercenary_overlay` in Rust settings), because the strip is placed by the
	// user in Settings → Overlay Positions and has to come back where they left
	// it. That is guide guard 5, and it is met: `persist_overlay_settings` copies
	// the field and `test_overlay_settings_survive_persist_cycle` covers it.
	//
	// The window is NEVER interactive. The capture loop reads the screen only
	// while the game is the RAW foreground window (`game_in_foreground`), while
	// this window is shown and hidden on the HELD `game_focused` — two reads
	// that are deliberately never unified (see the focus poller in `lib.rs`). A
	// click landing here would take focus, drop the raw flag, and stop the loop
	// that produces the verdict on screen. Hence it declares no hot rects: it
	// registers with the mouse hook (so WebView2 cannot strip its
	// WS_EX_TRANSPARENT unrepaired) and claims not one pixel of the click.
	// The shipped placement lives in `$lib/overlay/overlay-defaults` because the
	// Settings position flow builds a config window from the SAME numbers and
	// persists whatever it is saved at. Two copies meant a Save from Settings
	// could write the older size back over the newer default forever.
	//
	// They are CSS pixels; everything below this line is physical. See
	// `physicalGeometry`.

	/**
	 * Guard 6's channel — see `logTemple`.
	 *
	 * This prefix is the ONLY one a line carries: the driver deliberately does
	 * not add the window label on top of it (`[merc-overlay] mercenary: …` says
	 * the same word twice), and the label has to live here rather than there
	 * because the lines below — a failed `scaleFactor`, a half-built window —
	 * are this file's, not the driver's, and would otherwise be unattributable
	 * in a log with two overlays failing.
	 */
	function logMerc(msg: string): void {
		console.warn(`[overlay] merc: ${msg}`);
		invoke('app_log_from_frontend', { msg: `[merc-overlay] ${msg}` })
			.catch(e => console.error('[overlay] merc: app log unreachable:', e));
	}

	/**
	 * The persisted geometry, or the shipped placement when nothing is stored.
	 *
	 * PHYSICAL pixels either way, which is what the two units in play make
	 * non-obvious: persisted settings are already physical and are returned
	 * untouched, while the shipped defaults are reasoned in CSS pixels (a height
	 * budget is a sum of font sizes) and are converted here. Shipping the CSS
	 * figure as a physical one made the strip a third short of its own budget on
	 * a 150 %-scaled display — the machines where the clipping mattered most.
	 *
	 * The fields mix per-field, deliberately: a user who has saved a position
	 * but whose height Rust never stored gets their x/y and the scaled default
	 * height, rather than one wholesale choice between the two sources.
	 */
	async function mercOverlayGeometry(): Promise<{ x: number; y: number; w: number; h: number }> {
		const settings = await invoke<any>('get_mercenary_overlay_settings').catch(e => {
			logMerc(`settings load failed, using the default placement: ${e}`);
			return null;
		});
		const sf = await getCurrentWebviewWindow()
			.scaleFactor()
			.catch((e: any) => {
				logMerc(`scaleFactor failed while sizing the default placement, using 1: ${e}`);
				return 1;
			});
		const shipped = physicalGeometry(MERC_OVERLAY_DEFAULTS, sf);
		return {
			x: settings?.x ?? shipped.x,
			y: settings?.y ?? shipped.y,
			w: settings?.width ?? shipped.w,
			h: settings?.height ?? shipped.h,
		};
	}

	/**
	 * Build the window at its persisted geometry. Resolves true only once it is
	 * positioned, sized and click-through — a half-built one is torn down.
	 */
	async function createMercenaryOverlay(): Promise<boolean> {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');

		await destroyMercenaryWindow();

		const geometry = await mercOverlayGeometry();
		// Constructor dimensions are LOGICAL; the persisted ones are physical.
		const sf = await getCurrentWebviewWindow()
			.scaleFactor()
			.catch((e: any) => {
				logMerc(`scaleFactor failed, using 1: ${e}`);
				return 1;
			});
		const win = new WebviewWindow(MERCENARY_WINDOW_LABEL, {
			url: `/overlay/${MERCENARY_WINDOW_LABEL}`,
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			// Nothing drags or resizes this window directly — it is click-through,
			// so a resize edge would never receive the mouse. The size is changed
			// through the Settings position overlay, which applies it with
			// `move_overlay` (guide guard 4: move, never recreate).
			resizable: false,
			shadow: false,
			skipTaskbar: true,
			// Built without stealing the foreground. Tauri focuses a new window
			// by default, and this one has teeth: activation makes the game stop
			// being the foreground window, which drops `game_in_foreground` and
			// stops the capture loop — the same failure the click-through note
			// below describes, on the creation path rather than on a click.
			focus: false,
			width: Math.round(geometry.w / sf),
			height: Math.round(geometry.h / sf),
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
					await win.setPosition(new PhysicalPosition(geometry.x, geometry.y));
					await win.setSize(new PhysicalSize(geometry.w, geometry.h));
					// Display-only: interactive width 0, like compass/pathstrip/timer
					// and the temple. Do NOT copy the comparator's right-edge zone —
					// a focused own-window stops the capture loop.
					//
					// MEASURED: this command is fire-and-forget. Rust spawns a
					// thread that sleeps ~1 s before `set_ignore_cursor_events`
					// (the WebView2 HWND is not available sooner), so the await
					// returns long before click-through is installed and the
					// catch below cannot observe a failure in it. For this
					// overlay that second has teeth: the window IS interactive
					// until the thread runs, and a click landing on it takes
					// focus, drops `game_in_foreground`, and stops the capture
					// loop producing the verdict. Left as-is deliberately — the
					// fix belongs in the Rust command (make it await and report),
					// not in a second copy of the wait here. `focus: false` on
					// the constructor covers only the OTHER half of that second:
					// the window no longer ACTIVATES itself on creation, but a
					// click that lands in it still does.
					await invoke('set_overlay_clickthrough', {
						label: MERCENARY_WINDOW_LABEL,
					});
				} catch (e) {
					// Transparent, always-on-top and NOT click-through eats clicks
					// over the game with nothing visible to explain why. Half-built
					// is worse than absent.
					logMerc(`setup failed, destroying the half-built window: ${e}`);
					await destroyMercenaryWindow();
					finish(false);
					return;
				}
				// Soft step: the focus poller only acts on transitions, so a window
				// built while PoE is not in the foreground would sit on the desktop
				// until the next alt-tab. Failing this leaves a visible window, not
				// a broken one — logged, not fatal.
				try {
					const status = await invoke<any>('get_status');
					if (!status?.game_focused) await win.hide();
				} catch (e) {
					logMerc(`initial focus check failed, window left visible: ${e}`);
				}
				finish(true);
			}).catch(e => {
				logMerc(`could not listen for tauri://created: ${e}`);
				finish(false);
			});

			win.once('tauri://error', (e: any) => {
				logMerc(`creation failed: ${JSON.stringify(e?.payload ?? e)}`);
				finish(false);
			}).catch(e => {
				logMerc(`could not listen for tauri://error: ${e}`);
				finish(false);
			});
		});
	}

	/**
	 * Tear the window down. Returns whether the label is gone afterwards.
	 *
	 * It reads NO geometry on the way out. **`mercenary_overlay` in settings is
	 * the only writer of this overlay's geometry**, and the Settings position
	 * flow already writes it — Save persists the config window's rect and Cancel
	 * restores the pre-configure one, both BEFORE `reclaimMouse()` emits the
	 * move — so there is no live placement that settings do not already hold.
	 * A save here would be worse than redundant: this function is also the
	 * pre-create sweep and the half-built rollback, so a creation that timed out
	 * and retried would persist the constructor's own placement over the
	 * position the user chose.
	 */
	async function destroyMercenaryWindow(): Promise<boolean> {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		for (let i = 0; i < 5; i++) {
			const win = await WebviewWindow.getByLabel(MERCENARY_WINDOW_LABEL).catch(e => {
				logMerc(`lookup during destroy failed: ${e}`);
				return null;
			});
			if (!win) return true;
			// Both are attempted: `close` is the polite path and `destroy` the one
			// that actually frees the label. Tauri's cleanup is async, hence the
			// re-check rather than a single call.
			try { await win.close(); } catch (e) { logMerc(`close attempt ${i + 1} failed: ${e}`); }
			try { await win.destroy(); } catch (e) { logMerc(`destroy attempt ${i + 1} failed: ${e}`); }
			await new Promise(r => setTimeout(r, 100));
		}
		logMerc('window still present after 5 close/destroy rounds — giving the label up');
		return false;
	}

	const mercenaryOverlay = moduleOverlayDriver(
		{ label: MERCENARY_WINDOW_LABEL, moduleId: MERCENARY_MODULE_ID },
		{ create: createMercenaryOverlay, destroy: destroyMercenaryWindow, log: logMerc }
	);

	$effect(() => {
		const enabled = ssot.modules[MERCENARY_MODULE_ID];
		if (enabled === undefined) return;
		// The feature gate is ANDed in rather than short-circuiting the effect:
		// a device that loses the grant with the module flag still set must have
		// the window taken down, not left standing (POE-203).
		//
		// This takes the WINDOW down, not the module: the Rust merc module keeps
		// running (and scanning) on its own flag, because the gate is hiding and
		// not securing. Pushing the revocation into Rust is recorded as a
		// follow-up on POE-203, not done here.
		mercenaryOverlay.setDesired(enabled && mercGranted);
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
	// Ctrl+Shift+F11 opens the identify dialog (device alias registration)
	let identifyOpen = $state(false);

	$effect(() => {
		function handleKeydown(e: KeyboardEvent) {
			if (e.ctrlKey && e.shiftKey && e.key === 'F12') {
				e.preventDefault();
				debugMode = !debugMode;
				// Devtools has no JS API in Tauri 2 — `openDevtools()` on the window
				// object is not a function and threw before the .catch could apply.
				invoke('set_devtools', { open: debugMode }).catch((e: any) => console.warn('[debug] set_devtools failed:', e));
				// EVERY press, with the state we want — not only the ON one. The
				// command used to be an argument-less toggle called on the ON
				// transition alone, so Rust's debug flag flipped once per two
				// presses and went silent while this handler said it was on
				// (2026-08-26 smoke). On ON it also force-shows the overlays
				// regardless of game focus.
				invoke('set_debug_mode', { on: debugMode }).catch((e: any) => console.warn('[debug] set_debug_mode failed:', e));
				console.log(debugMode ? '[debug] Debug mode ON — overlays force-shown' : '[debug] Debug mode OFF');
			}
			if (e.ctrlKey && e.shiftKey && e.key === 'F11') {
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
			// The merc strip is here for the same reason as the four above: the
			// config window's Cancel restores the pre-configure geometry into
			// settings, and the live window has to be moved back onto it —
			// with `move_overlay`, never a destroy/recreate (guard 4).
			if (mercenaryOverlay.built()) {
				const mercSettings = await mercOverlayGeometry();
				await invoke('move_overlay', { label: MERCENARY_WINDOW_LABEL, x: mercSettings.x, y: mercSettings.y, w: mercSettings.w, h: mercSettings.h })
					.catch(e => logMerc(`move after config failed: ${e}`));
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
		<Sidebar open={sidebarOpen} currentPath={viewToPath(visibleView)} onToggle={toggleSidebar}
			comparatorActive={comparatorActive} gameFocused={store.status?.game_focused ?? false} onToggleComparator={toggleComparatorOverlay}
			compassActive={compassActive} onToggleCompass={toggleCompassOverlay}
			pathstripActive={pathstripActive} pathstripHasData={pathstripHasData} onTogglePathstrip={togglePathstripOverlay}
			timerActive={timerActive} onToggleTimer={toggleTimerOverlay}
			labOverlaysActive={labOverlaysActive} onToggleLabOverlays={toggleLabOverlays} />
		<main class="content">
			<div class="route-render-placeholder" aria-hidden="true">
				{@render children()}
			</div>
			<div class:view-hidden={visibleView !== 'lab'}>
				<LabPage />
			</div>
			<div class:view-hidden={visibleView !== 'settings'}>
				<SettingsPage />
			</div>
			{#if mercGranted}
				<div class:view-hidden={visibleView !== 'mercenaries'}>
					<MercenariesPage />
				</div>
			{/if}
			{#if templeGranted}
				<div class:view-hidden={visibleView !== 'temple'}>
					<TemplePage />
				</div>
			{/if}
			{#if exchangeGranted}
				<div class:view-hidden={visibleView !== 'currency-exchange'}>
					<CurrencyExchangePage />
				</div>
			{/if}
			{#if import.meta.env.DEV}
				<div class:view-hidden={visibleView !== 'dev'}>
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
