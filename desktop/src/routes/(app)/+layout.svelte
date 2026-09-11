<script lang="ts">
	import '../../app.css';
	import { invoke } from '@tauri-apps/api/core';
	import { emitTo, listen } from '@tauri-apps/api/event';
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
		LAB_WINDOW_LABEL,
		MERCENARY_MODULE_ID,
		MERCENARY_WINDOW_LABEL,
		TEMPLE_MODULE_ID,
		TEMPLE_WINDOW_LABEL
	} from '$lib/overlay/manager';
	import { clickthroughReport } from '$lib/overlay/clickthrough-report';
	import { moduleOverlayWanted } from '$lib/overlay/module-lifecycle';
	import { createWidgetWindow, type WidgetWindow } from '$lib/overlay/widget-window';
	import { WIDGETS } from '$lib/overlay/widgets/widget-registry';
	import { setWidgetVisible, widgetPlacements } from '$lib/overlay/widgets/widget-placements.svelte';
	import { type GameMonitorInfo } from '$lib/overlay/monitor-choice';
	import {
		widgetConfigEnd,
		widgetConfigLive,
		widgetConfigSessionsInit,
		widgetConfigStart,
		type WidgetConfigSessions
	} from '$lib/overlay/widgets/widget-config-session';
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

	/**
	 * A click-through setup that did not land, reported in the app log as well
	 * as the console (guide guard 6 — a shipped build has no devtools).
	 *
	 * `set_overlay_clickthrough` AWAITS its own setup and returns the failure
	 * now, so there is something real to report: a transparent, always-on-top
	 * window that is not click-through swallows the player's clicks with nothing
	 * on screen to explain it. The separate comparator overlay only REPORTS it — it is a
	 * small, user-positioned rectangle, and destroying it just after the user
	 * switched it on would read as a toggle that does nothing. The two
	 * module-coupled windows (temple, merc) destroy and retry instead, because
	 * one of them is the size of the monitor and a click on the other stops the
	 * capture loop; that path is `module-lifecycle.ts`.
	 *
	 * So its call is attached rather than awaited. The command spends ~1 s
	 * waiting for the WebView2 HWND (guide, runtime-earned observations), and
	 * the "hide if the game is not focused" step that follows every creation
	 * must not sit on screen for that second on a window nobody is looking at.
	 * Nothing downstream reads the result, so the `catch` observes it just as
	 * well as an `await` would.
	 *
	 * Not every failure is a warning: a window destroyed inside that 1 s wait is
	 * what an ordinary toggle-off looks like and has nothing left to eat a
	 * click. `clickthroughReport` is that decision, out where it can be tested.
	 */
	function logClickthroughFailure(label: string, e: unknown): void {
		const report = clickthroughReport(label, e);
		if (report.level === 'error') console.error(`[overlay] ${report.message}`);
		else console.info(`[overlay] ${report.message}`);
		invoke('app_log_from_frontend', { msg: report.message })
			.catch(err => console.error('[overlay] app log unreachable:', err));
	}

	// Comparator overlay state
	let comparatorActive = $state(false);
	let comparatorWin = $state<any>(null);

	let pathstripHasData = $state(false);

	// Lab overlays category toggle
	let labOverlaysActive = $state(true);
	/** Whether `get_lab_overlays_enabled` has answered — until it has, the lab window's desired state is unknown, the same
	 *  not-yet-known the module flags have before the first poll. */
	let labOverlaysLoaded = $state(false);

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
			// Reported, not awaited — see `logClickthroughFailure`.
			void invoke('set_overlay_clickthrough', { label: 'comparator' })
				.catch(e => logClickthroughFailure('comparator', e));
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

	/**
	 * Whether anything currently wants the temple overlay window — the module
	 * flag, or a live widget-config session, both under the feature grant.
	 *
	 * THREE readers since POE-245 (`module-lifecycle.ts`'s `moduleOverlayWanted`
	 * is the rule): the desired-state effect below, and the two places that
	 * rebuild the window onto another display. A rebuild is an unconditional
	 * `setDesired(false)` then `setDesired(true)`, so without this a module the
	 * user switched off while the window was being built would come straight
	 * back — the correction winning over the toggle.
	 */
	function templeOverlayWanted(): boolean {
		return moduleOverlayWanted(
			ssot.modules[TEMPLE_MODULE_ID],
			widgetConfigLive(widgetConfigSessions, TEMPLE_WINDOW_LABEL),
			templeGranted
		);
	}

	const templeWindow = createWidgetWindow({
		label: TEMPLE_WINDOW_LABEL,
		moduleId: TEMPLE_MODULE_ID,
		wanted: templeOverlayWanted,
		configLive: () => widgetConfigLive(widgetConfigSessions, TEMPLE_WINDOW_LABEL),
		debug: () => debugMode
	});

	function mercOverlayWanted(): boolean {
		return moduleOverlayWanted(
			ssot.modules[MERCENARY_MODULE_ID],
			widgetConfigLive(widgetConfigSessions, MERCENARY_WINDOW_LABEL),
			mercGranted
		);
	}

	// Grab exclusion is Rust-side and BY LABEL (`capture.rs` `EXCLUDED_WHILE_GRABBING`),
	// so this window must keep the `mercenary` label.
	const mercenaryWindow = createWidgetWindow({
		label: MERCENARY_WINDOW_LABEL,
		moduleId: MERCENARY_MODULE_ID,
		wanted: mercOverlayWanted,
		configLive: () => widgetConfigLive(widgetConfigSessions, MERCENARY_WINDOW_LABEL),
		// The merc route has no debug surface; the window was never built with `?debug`.
		debug: () => false
	});

	function labOverlayWanted(): boolean {
		return moduleOverlayWanted(
			labOverlaysLoaded ? labOverlaysActive : undefined,
			widgetConfigLive(widgetConfigSessions, LAB_WINDOW_LABEL),
			true
		);
	}

	const labWindow = createWidgetWindow({
		label: LAB_WINDOW_LABEL, moduleId: LAB_WINDOW_LABEL, wanted: labOverlayWanted,
		configLive: () => widgetConfigLive(widgetConfigSessions, LAB_WINDOW_LABEL), debug: () => false
	});
	const LAB_COMPASS_SPEC = WIDGETS.find((spec) => spec.id === 'lab.compass')!;
	const LAB_PATHSTRIP_SPEC = WIDGETS.find((spec) => spec.id === 'lab.pathstrip')!;
	const LAB_TIMER_SPEC = WIDGETS.find((spec) => spec.id === 'lab.timer')!;
	/** Every monitor-sized widget window this layout builds. */
	const WIDGET_WINDOWS: WidgetWindow[] = [templeWindow, mercenaryWindow, labWindow];

	// Window-scoped, because Rust sends this with `emit_to("main")`.
	getCurrentWebviewWindow()
		.listen<GameMonitorInfo>('game-monitor-changed', (event) => {
			for (const w of WIDGET_WINDOWS) w.onGameMonitorChanged(event.payload);
		})
		.catch((e: any) => {
			for (const w of WIDGET_WINDOWS) w.log(`could not listen for game-monitor-changed: ${e}`);
		});

	// Two terms, ORed: the module flag (× its feature grant) and a live
	// widget-config session. `undefined` means the first poll has not answered
	// yet, and is treated as "not yet known" rather than as off — tearing a
	// window down on a value nobody has reported would fight the startup poll.
	// Re-reporting the same value is a no-op inside the driver.
	//
	// The SESSION term is what makes arranging widgets cost no module work
	// (POE-241): the window is raised here, for the length of the session, while
	// the Rust temple loop stays spawned by the module flag alone
	// (`modules.rs` reconcile — untouched by this flow). Dropping the record in
	// `endWidgetConfig` is therefore what tears the window down again when the
	// flag is off, and it is the LAST thing that function does for exactly that
	// reason.
	$effect(() => {
		const enabled = ssot.modules[TEMPLE_MODULE_ID];
		const configuring = widgetConfigLive(widgetConfigSessions, TEMPLE_WINDOW_LABEL);
		// A session wants the window whatever the poll has or has not said, so
		// the not-yet-known bail-out needs BOTH halves: nothing to say AND
		// nothing to take down. A session that opened before the first poll
		// answered leaves a window that only this effect can remove, and bailing
		// out on its end would strand it until the poll lands — which, on a
		// device that never answers, is never.
		if (enabled === undefined && !configuring && !templeWindow.driver.built()) return;
		// The feature gate is ANDed over BOTH terms rather than the flag alone: a
		// device that loses the grant must have the window taken down whether the
		// module flag or a config session is what is holding it up (POE-203).
		// This takes the WINDOW down, not the module — the Rust temple module
		// keeps running on its own flag, because the gate is hiding and not
		// securing.
		// The same rule the two rebuild sites ask, so the three cannot drift.
		// `enabled` and `configuring` are read above as well, which is what keeps
		// this effect subscribed to them.
		templeWindow.driver.setDesired(templeOverlayWanted());
	});

	$effect(() => {
		const enabled = ssot.modules[MERCENARY_MODULE_ID];
		const configuring = widgetConfigLive(widgetConfigSessions, MERCENARY_WINDOW_LABEL);
		if (enabled === undefined && !configuring && !mercenaryWindow.driver.built()) return;
		mercenaryWindow.driver.setDesired(mercOverlayWanted());
	});

	$effect(() => {
		const enabled = labOverlaysActive;
		const configuring = widgetConfigLive(widgetConfigSessions, LAB_WINDOW_LABEL);
		if (!labOverlaysLoaded && !configuring && !labWindow.driver.built()) return;
		labWindow.driver.setDesired(labOverlayWanted());
	});

	// --- Widget config mode (POE-226) ---
	//
	// Settings cannot open config mode itself: the three steps have to happen in
	// one order (`docs/OVERLAY-GUIDE.md`, "Config-mode ordering contract") and
	// the first of them is ensuring the module's WINDOW exists and is on screen,
	// which is this file's job — it owns every overlay window's creation. So
	// Settings emits `widget-config-start {module}` and the work happens here.
	//
	// What makes step 1 more than a `show()` is that a module-coupled overlay
	// exists only while something wants it — normally the module flag — and is
	// hidden by the Rust focus poller whenever the game is not in front, which
	// while the user is looking at Settings it never is. So the SESSION itself
	// is the second thing that wants the window (POE-241): recording it raises
	// the driver's desired state for the length of the session and starts no
	// module work, which is why arranging widget positions costs no capture loop
	// and no OCR. What is left to force is the visibility, and
	// `widget-config-session.ts` is the record of it, because both restores are
	// silent when they are wrong.

	/**
	 * The overlay window behind each module that has widgets. Temple and Merc
	 * both use the monitor-sized widget engine (POE-225 D1, POE-232).
	 *
	 * Keyed by the WINDOW LABEL, because that is what a `WidgetSpec.module` is
	 * and therefore what Settings sends — the distinction `manager.ts` keeps
	 * between a window label and the module id that happens to spell the same
	 * word. The module id itself is no longer stored here: nothing in this flow
	 * touches a module flag any more. Entries come from `WIDGET_WINDOWS`, so each
	 * monitor-sized widget window contributes its own config target.
	 */
	const WIDGET_CONFIG_TARGETS: Record<string, { label: string; built: () => boolean }> = Object.fromEntries(
		WIDGET_WINDOWS.map((w) => [w.label, w.configTarget])
	);

	/**
	 * What each live config session forced — and, since POE-241, the second term
	 * of every module overlay's desired state, which is why it is a rune.
	 *
	 * `$state.raw` and not deep `$state`: the map is only ever REPLACED (both
	 * reducers return a new object), so a proxy would buy nothing and would put
	 * one between `widgetConfigLive` and a plain record read. Nothing renders
	 * from it; its one reactive consumer is the overlay effect above, which
	 * writes nothing, so a session recorded here asks for the window the same
	 * way the module flag does.
	 */
	let widgetConfigSessions: WidgetConfigSessions = $state.raw(widgetConfigSessionsInit());

	/**
	 * How long a session gets to produce its window.
	 *
	 * Recording the session is what BUILDS the window: `widgetConfigSessions` is
	 * a rune, the effect above ORs it into the driver's desired state, and the
	 * driver creates. That path is bounded by the driver's own
	 * `CREATE_TIMEOUT_MS` (10 s) plus a retry, so waiting the same 10 s here
	 * covers a first attempt that succeeds slowly without waiting out a window
	 * that is never coming.
	 */
	const WIDGET_WINDOW_WAIT_MS = 10_000;
	const WIDGET_WINDOW_POLL_MS = 150;

	/**
	 * How long an OPEN config session may run before this file ends it itself.
	 *
	 * Not the same clock as Settings' opening deadline, which bounds the OPENING
	 * only and stands down on `widget-config-open`. This one starts where that
	 * one stops, and it exists because the host can now decline to end a session:
	 * `WidgetHost.svelte` keeps config mode when `set_overlay_config_mode(label,
	 * false)` refuses, which is right for the user in front of the window and
	 * leaves everyone else waiting — Settings on `Configuring…`, the window
	 * force-shown, and a window that only the session record is holding up.
	 * Without a floor under it, a window whose exit can never succeed leaves a
	 * monitor-sized INTERACTIVE rectangle over the game until the app is
	 * restarted: the mouse hook deliberately skips a window in config mode, so
	 * every click where the widgets are stops reaching PoE.
	 *
	 * Ten minutes because it must sit far above real arranging — the session ends
	 * when the user presses Save or Cancel, however long they take, and dragging
	 * widgets around a board is minutes, not seconds — and far below "for the
	 * rest of the session". A user still arranging at ten minutes loses the
	 * frames and presses Configure again; a stuck one gets their clicks back.
	 */
	const WIDGET_CONFIG_SESSION_MAX_MS = 10 * 60_000;

	/**
	 * The armed deadlines, by module. Also the record of which sessions this file
	 * has seen OPEN and not yet seen end — which is what the timer needs and
	 * `widgetConfigSessions` cannot answer, because a session that forced nothing
	 * still has to be reported to Settings.
	 */
	const widgetConfigDeadlines = new Map<string, ReturnType<typeof setTimeout>>();

	function clearWidgetConfigDeadline(module: string): void {
		const timer = widgetConfigDeadlines.get(module);
		if (timer === undefined) return;
		clearTimeout(timer);
		widgetConfigDeadlines.delete(module);
	}

	/** Arm the deadline for a session that is now open. Re-arming on a second
	 *  Configure press restarts the clock, which is the honest reading: the user
	 *  just told us they are still working on this module. */
	function armWidgetConfigDeadline(module: string): void {
		clearWidgetConfigDeadline(module);
		widgetConfigDeadlines.set(
			module,
			setTimeout(() => {
				widgetConfigDeadlines.delete(module);
				logWidgetConfig(
					module,
					`still in configuration after ${Math.round(WIDGET_CONFIG_SESSION_MAX_MS / 60_000)} min — ending the session from here`
				);
				void abandonWidgetConfig(module);
			}, WIDGET_CONFIG_SESSION_MAX_MS)
		);
	}

	/** The starts currently in flight, by module. A second Configure press during
	 *  the wait above would re-derive the session against a half-built world,
	 *  fail, and abandon the FIRST press — disabling the module under it. */
	const widgetConfigStarting = new Set<string>();

	function logWidgetConfig(module: string, msg: string): void {
		console.warn(`[widget-config] ${module}: ${msg}`);
		invoke('app_log_from_frontend', { msg: `[widget-config] ${module}: ${msg}` })
			.catch(e => console.error('[widget-config] app log unreachable:', e));
	}

	async function widgetConfigWindow(label: string): Promise<any | null> {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		return await WebviewWindow.getByLabel(label).catch(e => {
			logWidgetConfig(label, `window lookup failed: ${e}`);
			return null;
		});
	}

	/**
	 * Wait for a module's window to be BUILT, not merely to exist.
	 *
	 * Polling rather than an event because the creation belongs to the driver,
	 * which reports to itself; `built()` is the marker it settles, and the window
	 * lookup is the second half because a create that failed leaves `built()`
	 * false and a destroy leaves the label gone.
	 */
	async function waitForWidgetConfigWindow(
		target: { label: string; built: () => boolean }
	): Promise<any | null> {
		const deadline = Date.now() + WIDGET_WINDOW_WAIT_MS;
		for (;;) {
			if (target.built()) {
				const win = await widgetConfigWindow(target.label);
				if (win) return win;
			}
			if (Date.now() >= deadline) return null;
			await new Promise(r => setTimeout(r, WIDGET_WINDOW_POLL_MS));
		}
	}

	async function startWidgetConfig(module: string): Promise<void> {
		// The guard covers the whole function, the multi-second wait included: a
		// second press then would re-derive against a half-built world, throw, and
		// abandon the press that is still working. It is also what makes a
		// duplicated listener (an HMR reload registers a second one; the layout
		// never unmounts, so nothing removes the first) harmless.
		if (widgetConfigStarting.has(module)) {
			logWidgetConfig(module, 'a config session is already opening — ignoring the second press');
			return;
		}
		widgetConfigStarting.add(module);
		try {
			await openWidgetConfig(module);
		} finally {
			widgetConfigStarting.delete(module);
		}
	}

	async function openWidgetConfig(module: string): Promise<void> {
		// "I have the request and I am working on it." Settings' opening deadline
		// distinguishes a start that is merely SLOW — the window wait alone runs
		// to 10 s — from one that nothing ever picked up, and only the second is
		// safe to abandon: tearing the session down under a start still in flight
		// would leave that start setting config mode on a window just torn down.
		await getCurrentWebviewWindow()
			.emit('widget-config-opening', { module })
			.catch((e: any) => logWidgetConfig(module, `could not report the start to Settings: ${e}`));
		const target = WIDGET_CONFIG_TARGETS[module];
		if (!target) {
			logWidgetConfig(module, 'no overlay window is registered for this module');
			await abandonWidgetConfig(module);
			return;
		}
		let win = await widgetConfigWindow(target.label);
		const pre = { shown: win ? await win.isVisible().catch(() => false) : false };
		const started = widgetConfigStart(widgetConfigSessions, module, pre);
		// The record IS the request for the window (POE-241): the overlay effect
		// above ORs `widgetConfigLive` into the driver's desired state, so this
		// assignment is what builds a window for a module whose flag is off — and
		// it starts no module work, so nothing here turns on a capture loop or an
		// OCR pass. It also has to land before `widget-window.ts`'s `create` runs, which
		// reads the same record: a window built for this session must not run its
		// not-focused-so-hide step.
		widgetConfigSessions = started.sessions;
		try {
			// Whenever there is no window yet — not only when the module was off. A
			// module that reads as on can still be mid-build (a restart, a retry
			// after a failed create), and abandoning immediately would take the
			// session down with it.
			if (!win) win = await waitForWidgetConfigWindow(target);
			if (!win) throw new Error(`the ${target.label} window did not appear`);
			// Step 1: on screen. Unconditional rather than gated on
			// `actions.showWindow`, because a window built during the wait ran its
			// own focus check on the way up; on an already-visible window this is a
			// no-op. The focus poller will not undo it — it acts on transitions,
			// and Rust now also leaves a window in config mode alone.
			await win.show();
			// Step 2: the Rust flag BEFORE the event, so the mouse hook is already
			// leaving the window alone — and so the host's catch-up query cannot
			// answer false for a window created a moment ago.
			await invoke('set_overlay_config_mode', { label: target.label, on: true });
			// Step 3: webview-scoped, so a second module's host never hears it.
			await emitTo(target.label, 'widget-config', { module, on: true });
			// The acknowledgement Settings' deadline waits on. Without it, a
			// listener that never registered (or a start that never ran) leaves the
			// button on "Configuring…" with nothing ever answering.
			await getCurrentWebviewWindow()
				.emit('widget-config-open', { module })
				.catch((e: any) => logWidgetConfig(module, `could not confirm config mode to Settings: ${e}`));
			// Armed where Settings' opening deadline stands down, so exactly one
			// clock is running on this module at any moment.
			armWidgetConfigDeadline(module);
		} catch (e) {
			// Settings is sitting on "Configuring...", and nothing else will
			// answer it: the host that emits `widget-config-end` is in the window
			// that failed to open.
			logWidgetConfig(module, `could not open config mode: ${e}`);
			await abandonWidgetConfig(module);
		}
	}

	async function endWidgetConfig(module: string): Promise<void> {
		// First, and unconditionally: this runs for an end that arrived normally,
		// for one this file's own deadline produced, and for a module that never
		// had a session at all. A timer left armed past any of them would abandon
		// the NEXT session ten minutes in.
		clearWidgetConfigDeadline(module);
		const target = WIDGET_CONFIG_TARGETS[module];
		// The state of the world NOW, not when the session opened: if the game is
		// back in front, the poller has already shown this window and wants it
		// shown, and hiding it would cost the player their overlay until two more
		// focus transitions. A status read that fails is treated as "not focused",
		// which restores the pre-session state — the conservative answer, since the
		// session only ever forced a show when the game was NOT focused.
		const status = await invoke<any>('get_status').catch((e: any) => {
			logWidgetConfig(module, `could not read game focus, assuming unfocused: ${e}`);
			return null;
		});
		const ended = widgetConfigEnd(widgetConfigSessions, module, status?.game_focused === true);
		if (target && ended.actions.hideWindow) {
			const win = await widgetConfigWindow(target.label);
			if (win) await win.hide().catch((e: any) => logWidgetConfig(module, `hide failed: ${e}`));
		}
		// LAST, because dropping the record is what lowers the window's desired
		// state: for a module whose flag is off, the driver tears the window down
		// on this assignment. Hiding first therefore hides a window that still
		// exists, rather than racing a teardown into a "hide failed" line — the
		// same ordering the force-disable used to need, for the same reason.
		widgetConfigSessions = ended.sessions;
	}

	/**
	 * End the session from THIS side: take the window out of config mode, undo
	 * what the session forced, and tell Settings.
	 *
	 * Two callers with the same need. A start that never got as far as a host
	 * (no window, or a command that failed) — and the deadline above, which fires
	 * on a session the host has declined to end. The second is why the window
	 * half exists at all: the host listens for `widget-config` and nothing else,
	 * so restoring the visibility and the button without telling the window would
	 * leave Rust's `config_mode` set — the re-assert in `WidgetHost.exitConfig`
	 * puts it back on a refused exit — and with it a monitor-sized interactive
	 * rectangle the mouse hook deliberately skips, with no Settings button left
	 * to say so.
	 *
	 * The window is dealt with BEFORE the restore, because the restore may hide
	 * or tear the window down, and the three steps are in this order:
	 *
	 * 1. `widget-config {on: false}` to the window. The HOST owns the Rust call
	 *    — its handler runs `exitConfig` — so the normal path keeps one owner and
	 *    the host clears its frames, its draft and its bar itself.
	 * 2. The belt, direct to Rust. It covers a host that is unreachable (no
	 *    listener, a window mid-teardown) and a host whose own exit REFUSED, in
	 *    which case it has just re-asserted config mode and would otherwise leave
	 *    the flag set behind us. On an abandon the layout wins: the session is
	 *    being taken down whether the window agrees or not.
	 * 3. The same event again. Free when the host has already exited (its
	 *    `configMode` guard drops it), and it is what closes the one gap step 2
	 *    opens: a host that refused in step 1 is still drawing frames over a
	 *    window that is click-through again, and this repeat is the retry that
	 *    now succeeds, because step 2 has already put Rust where the host needs
	 *    it. Without it the user is left with painted frames and dead buttons.
	 *
	 * The echo the final `widget-config-end` produces on our own listener is a
	 * no-op — the session is already closed by then.
	 */
	async function abandonWidgetConfig(module: string): Promise<void> {
		const target = WIDGET_CONFIG_TARGETS[module];
		if (target) {
			await emitTo(target.label, 'widget-config', { module, on: false }).catch((e: any) =>
				logWidgetConfig(module, `could not ask the window to leave config mode: ${e}`)
			);
			// Expected to fail when there is no window — this is also the path a
			// start that never built one takes — so it is a note, not an alarm.
			await invoke('set_overlay_config_mode', { label: target.label, on: false }).catch(
				(e: any) => logWidgetConfig(module, `direct config-mode clear did not apply: ${e}`)
			);
			await emitTo(target.label, 'widget-config', { module, on: false }).catch((e: any) =>
				logWidgetConfig(module, `could not repeat the leave request: ${e}`)
			);
		}
		await endWidgetConfig(module);
		await getCurrentWebviewWindow()
			.emit('widget-config-end', { module })
			.catch((e: any) => logWidgetConfig(module, `could not report the failure to Settings: ${e}`));
	}

	// Registered at init with no cleanup, like every other listener in this file:
	// the desktop layout never unmounts. Under `npm run dev` an HMR reload DOES
	// leave the previous registration behind, so both handlers have to survive
	// being called twice for one event — `startWidgetConfig`'s in-flight guard
	// drops the duplicate, and `endWidgetConfig` finds no session the second time
	// and does nothing.
	listen<{ module?: string }>('widget-config-start', (event) => {
		const module = event.payload?.module;
		if (module) void startWidgetConfig(module);
	}).catch(e => console.warn('[widget-config] listen for start failed:', e));

	listen<{ module?: string }>('widget-config-end', (event) => {
		const module = event.payload?.module;
		if (module) void endWidgetConfig(module);
	}).catch(e => console.warn('[widget-config] listen for end failed:', e));

	// --- Lab overlays category toggle ---

	async function toggleLabOverlays() {
		const next = !labOverlaysActive;
		labOverlaysActive = next;
		labOverlaysLoaded = true;
		await invoke('set_lab_overlays_enabled', { enabled: next }).catch(e => console.warn('[overlay] set_lab_overlays_enabled failed:', e));
		if (next) {
			// Enable all — respect the comparator's own enabled state
			if (!comparatorActive) await toggleComparatorOverlay();
		} else {
			// Disable all
			if (comparatorActive) await toggleComparatorOverlay();
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

	// Restore lab overlays category toggle state
	invoke<boolean>('get_lab_overlays_enabled')
		.then((enabled) => { labOverlaysActive = enabled; labOverlaysLoaded = true; })
		.catch(e => console.warn('[overlay] get_lab_overlays_enabled failed:', e));

	// Check if lab layout is available for the sidebar's nodata indicator.
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

</script>

<div class="app-shell">
	<TopBar status={store.status} />
	<div class="app-body">
		<Sidebar open={sidebarOpen} currentPath={viewToPath(visibleView)} onToggle={toggleSidebar}
			comparatorActive={comparatorActive} gameFocused={store.status?.game_focused ?? false} onToggleComparator={toggleComparatorOverlay}
			compassActive={labOverlaysActive && (widgetPlacements.rows['lab.compass']?.visible ?? true)} onToggleCompass={() => setWidgetVisible(LAB_COMPASS_SPEC, !(widgetPlacements.rows['lab.compass']?.visible ?? true))}
			pathstripActive={labOverlaysActive && (widgetPlacements.rows['lab.pathstrip']?.visible ?? true)} pathstripHasData={pathstripHasData} onTogglePathstrip={() => setWidgetVisible(LAB_PATHSTRIP_SPEC, !(widgetPlacements.rows['lab.pathstrip']?.visible ?? true))}
			timerActive={labOverlaysActive && (widgetPlacements.rows['lab.timer']?.visible ?? true)}
			onToggleTimer={() => setWidgetVisible(LAB_TIMER_SPEC, !(widgetPlacements.rows['lab.timer']?.visible ?? true))}
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
