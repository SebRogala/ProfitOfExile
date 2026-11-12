<script lang="ts">
	import '../../app.css';
	import { invoke } from '@tauri-apps/api/core';
	import { emitTo, listen } from '@tauri-apps/api/event';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import TopBar from '$lib/components/TopBar.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import { store, initStatusStore } from '$lib/stores/status.svelte';
	import { ssot, startSsotStore } from '$lib/stores/ssot.svelte';
	import { currentDivineRate, initDivineRateStore } from '$lib/stores/divine-rate.svelte';
	import { startMercureStore } from '$lib/stores/mercure.svelte';
	import { initNumberFormatStore } from '$lib/stores/number-format.svelte';
	import { startRunRecorder } from '$lib/run-recorder';
	import { nav, viewToPath, type View } from '$lib/stores/navigation.svelte';
	import {
		hasFeature,
		MERC_FEATURE,
		EXCHANGE_FEATURE,
		TEMPLE_FEATURE,
		BETA_FEATURE
	} from '$lib/stores/entitlements.svelte';
	import {
		LAB_WINDOW_LABEL,
		LOOT_MODULE_ID,
		LOOT_WINDOW_LABEL,
		MERCENARY_MODULE_ID,
		MERCENARY_WINDOW_LABEL,
		TEMPLE_MODULE_ID,
		TEMPLE_WINDOW_LABEL
	} from '$lib/overlay/manager';
	import { createModuleWindows } from '$lib/overlay/module-windows.svelte';
	import { WIDGETS } from '$lib/overlay/widgets/widget-registry';
	import { setWidgetVisible, widgetPlacements } from '$lib/overlay/widgets/widget-placements.svelte';
	import { type GameMonitorInfo } from '$lib/overlay/monitor-choice';
	import { createWidgetConfigOrchestrator } from '$lib/overlay/widgets/widget-config-orchestrator';
	import LabPage from '$lib/pages/LabPage.svelte';
	import SettingsPage from '$lib/pages/SettingsPage.svelte';
	import MercenariesPage from '$lib/pages/MercenariesPage.svelte';
	import TemplePage from '$lib/pages/TemplePage.svelte';
	import CurrencyExchangePage from '$lib/pages/CurrencyExchangePage.svelte';
	import HarvestFlippingPage from '$lib/pages/HarvestFlippingPage.svelte';
	import ChaosDivPage from '$lib/pages/ChaosDivPage.svelte';
	import BossLootPage from '$lib/pages/BossLootPage.svelte';
	import {
		createNativeProfiles, createNativeStats, createNativeRuns,
		createNativeModuleSettings, createNativeDevice, saveNativeCsv
	} from '$lib/boss-loot/native-adapters';
	import { bossLootIcon } from '$lib/boss-loot/art';
	import type { PagePorts } from '$lib/boss-loot/page-controller.svelte';
	import DevPage from '$lib/pages/DevPage.svelte';
	import IdentifyDialog from '$lib/components/IdentifyDialog.svelte';

	let { children } = $props();

	const bossLootPorts: PagePorts = {
		profiles: createNativeProfiles(), stats: createNativeStats(), runs: createNativeRuns(),
		moduleSettings: createNativeModuleSettings(), device: createNativeDevice(),
		exportHistory: async (profile) => saveNativeCsv(profile)
	};

	// Sidebar state: driven by store.status.sidebar_open (persisted in Rust settings).
	let sidebarOpen = $derived(store.status?.sidebar_open ?? true);

	/**
	 * The feature each hidden view is gated on (POE-203). A view absent from this
	 * map is visible to every device; the four listed here are drawn only where
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
		'currency-exchange': EXCHANGE_FEATURE,
		'harvest-flipping': BETA_FEATURE,
		'chaos-div': BETA_FEATURE,
		'boss-loot': BETA_FEATURE
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
	/** Gates the Harvest Flipping page (beta, POE-283; no overlay, no module). */
	let harvestGranted = $derived(viewGranted('harvest-flipping'));
	/** Gates the Chaos ⇄ Div page (beta, POE-316; no overlay, no module). */
	let chaosDivGranted = $derived(viewGranted('chaos-div'));
	/** Page visibility is independent of the boss_loot work flag (ADR-014). */
	let bossLootGranted = $derived(viewGranted('boss-loot'));

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

	let pathstripHasData = $state(false);

	// Lab overlays category toggle
	let labOverlaysActive = $state(true);
	/** Whether `get_lab_overlays_enabled` has answered — until it has, the lab window's desired state is unknown, the same
	 *  not-yet-known the module flags have before the first poll. */
	let labOverlaysLoaded = $state(false);

	// Grab exclusion is Rust-side and BY LABEL (`capture.rs` `EXCLUDED_WHILE_GRABBING`),
	// so the merc row must keep the `mercenary` label.
	// The merc route has no debug surface; the window was never built with `?debug`.
	const moduleWindows = createModuleWindows([
		{
			label: TEMPLE_WINDOW_LABEL, moduleId: TEMPLE_MODULE_ID,
			granted: () => templeGranted, enabled: () => ssot.modules[TEMPLE_MODULE_ID], debug: () => debugMode
		},
		{
			label: MERCENARY_WINDOW_LABEL, moduleId: MERCENARY_MODULE_ID,
			granted: () => mercGranted, enabled: () => ssot.modules[MERCENARY_MODULE_ID], debug: () => false
		},
		{
			label: LAB_WINDOW_LABEL, moduleId: LAB_WINDOW_LABEL,
			granted: () => true, enabled: () => (labOverlaysLoaded ? labOverlaysActive : undefined), debug: () => false
		},
		{
			label: LOOT_WINDOW_LABEL, moduleId: LOOT_MODULE_ID,
			granted: () => bossLootGranted, enabled: () => ssot.modules[LOOT_MODULE_ID], debug: () => false
		}
	]);
	const LAB_COMPARATOR_SPEC = WIDGETS.find((spec) => spec.id === 'lab.comparator')!;
	const LAB_COMPASS_SPEC = WIDGETS.find((spec) => spec.id === 'lab.compass')!;
	const LAB_PATHSTRIP_SPEC = WIDGETS.find((spec) => spec.id === 'lab.pathstrip')!;
	const LAB_TIMER_SPEC = WIDGETS.find((spec) => spec.id === 'lab.timer')!;
	/** Every monitor-sized widget window this layout builds. */
	const WIDGET_WINDOWS = moduleWindows.windows;

	// Window-scoped, because Rust sends this with `emit_to("main")`.
	getCurrentWebviewWindow()
		.listen<GameMonitorInfo>('game-monitor-changed', (event) => {
			for (const w of WIDGET_WINDOWS) w.onGameMonitorChanged(event.payload);
		})
		.catch((e: any) => {
			for (const w of WIDGET_WINDOWS) w.log(`could not listen for game-monitor-changed: ${e}`);
		});

	// --- Widget config mode (POE-226) ---
	//
	// Settings cannot open config mode itself: the three steps have to happen in
	// one order (`docs/OVERLAY-GUIDE.md`, "Config-mode ordering contract") and
	// the first of them is ensuring the module's WINDOW exists and is on screen,
	// which this layout supplies — its row table is what
	// `module-windows.svelte.ts` builds every module overlay window from. So
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

	/** The production ports for the widget-config flow (POE-346 D1): Tauri lives here, the flow does not. */
	const widgetConfig = createWidgetConfigOrchestrator({
		loadWindowApi: async () => { await import('@tauri-apps/api/webviewWindow'); },
		getWindowByLabel: async (label) => {
			const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
			return await WebviewWindow.getByLabel(label);
		},
		invoke: (cmd, args) => invoke(cmd, args),
		emitTo: (label, event, payload) => emitTo(label, event, payload),
		emitToMain: (event, payload) => getCurrentWebviewWindow().emit(event, payload),
		getSessions: moduleWindows.getSessions,
		setSessions: moduleWindows.setSessions,
		targets: () => moduleWindows.configTargets,
		setTimer: (cb, ms) => setTimeout(cb, ms),
		clearTimer: (t) => clearTimeout(t as ReturnType<typeof setTimeout>),
		wait: (ms) => new Promise<void>((r) => setTimeout(r, ms)),
		now: () => Date.now(),
		warn: (...a) => console.warn(...a),
		error: (...a) => console.error(...a)
	});

	// Registered at init with no cleanup, like every other listener in this file:
	// the desktop layout never unmounts. Under `npm run dev` an HMR reload DOES
	// leave the previous registration behind, so both handlers have to survive
	// being called twice for one event — the orchestrator's in-flight guard
	// drops the duplicate, and the orchestrator's `end` finds no session the second time
	// and does nothing.
	listen<{ module?: string }>('widget-config-start', (event) => {
		const module = event.payload?.module;
		if (module) void widgetConfig.start(module);
	}).catch(e => console.warn('[widget-config] listen for start failed:', e));

	listen<{ module?: string }>('widget-config-end', (event) => {
		const module = event.payload?.module;
		if (module) void widgetConfig.end(module);
	}).catch(e => console.warn('[widget-config] listen for end failed:', e));

	// --- Lab overlays category toggle ---

	async function toggleLabOverlays() {
		const next = !labOverlaysActive;
		labOverlaysActive = next;
		labOverlaysLoaded = true;
		await invoke('set_lab_overlays_enabled', { enabled: next }).catch(e => console.warn('[overlay] set_lab_overlays_enabled failed:', e));
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
	initDivineRateStore();
	// The app's one Mercure connection; it opens once store.status arrives.
	startMercureStore();
	// The Number format pick every grouped figure reads (POE-323). Main window
	// only: no overlay route formats a grouped number.
	initNumberFormatStore();

	$effect(() => {
		invoke('merc_set_divine_rate', { divineRate: currentDivineRate() })
			.catch(e => console.error('[layout] merc_set_divine_rate failed:', e));
	});

	// Record every lab run, whether or not the timer overlay is enabled. The
	// recorder used to live in the overlay's webview, so a disabled overlay
	// meant the Runs tab silently collected nothing.
	startRunRecorder();

	// Focus-based overlay show/hide handled by Rust focus poller (GetForegroundWindow)

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
			comparatorActive={labOverlaysActive && (widgetPlacements.rows['lab.comparator']?.visible ?? true)} gameFocused={store.status?.game_focused ?? false} onToggleComparator={() => setWidgetVisible(LAB_COMPARATOR_SPEC, !(widgetPlacements.rows['lab.comparator']?.visible ?? true))}
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
			{#if harvestGranted}
				<div class:view-hidden={visibleView !== 'harvest-flipping'}>
					<HarvestFlippingPage />
				</div>
			{/if}
			{#if import.meta.env.DEV}
				<div class:view-hidden={visibleView !== 'dev'}>
					<DevPage />
				</div>
			{/if}
			{#if bossLootGranted}
				<div class:view-hidden={visibleView !== 'boss-loot'}>
					<BossLootPage ports={bossLootPorts} iconFor={(item) => bossLootIcon(item.id)} />
				</div>
			{/if}
			{#if chaosDivGranted}
				<div class:view-hidden={visibleView !== 'chaos-div'}>
					<ChaosDivPage />
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
