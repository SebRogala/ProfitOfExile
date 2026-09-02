<script lang="ts">
	import { untrack } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { checkForUpdate } from '$lib/updater/check';
	import { relaunch } from '@tauri-apps/plugin-process';
	import { store } from '$lib/stores/status.svelte';
	import { hasFeature, MERC_FEATURE, TEMPLE_FEATURE } from '$lib/stores/entitlements.svelte';
	import { ssot, fetchSsot } from '$lib/stores/ssot.svelte';
	import { screenGeometryView } from '$lib/geometry/view';
	import { MERC_OVERLAY_DEFAULTS, physicalGeometry } from '$lib/overlay/overlay-defaults';
	import { chooseMonitor, type GameMonitorInfo } from '$lib/overlay/monitor-choice';
	import {
		canStartConfigure,
		overlayGroups,
		widgetGeometryText
	} from '$lib/overlay/widgets/overlay-groups';
	import type { WidgetGeometry } from '$lib/overlay/widgets/widget-geometry';
	import type { WidgetSpec } from '$lib/overlay/widgets/widget-registry';
	import Tooltip from '$lib/components/Tooltip.svelte';
	import Toggle from '$lib/components/Toggle.svelte';
	import RangeSlider from '$lib/components/RangeSlider.svelte';
	import Button from '$lib/components/Button.svelte';
	import { getVersion } from '@tauri-apps/api/app';

	// --- Update ---
	let appVersion = $state('...');
	let updateStatus = $state<'idle' | 'checking' | 'available' | 'downloading' | 'error'>(
		store.updateAvailable ? 'available' : 'idle'
	);
	let updateVersion = $state(store.updateVersion || '');
	let updateError = $state('');
	let updateProgress = $state(0);

	// Load version on mount
	$effect(() => {
		getVersion().then(v => { appVersion = v; }).catch(() => {});
	});

	// --- League (SSOT) ---
	// League is owned by the Rust SSOT (see stores/ssot.svelte). This surface only
	// displays the resolved value and asks Rust to re-resolve; it never defaults one.
	// The in-flight state is driven off `ssot.resolving` (the polled SSOT flag),
	// NOT a local sub-frame flag: `refresh_league` returns immediately while the
	// bounded-retry loop runs, so a local flag would flash for one frame and then
	// lie about a still-retrying resolve. While a resolve is in flight, a click
	// WAKES the live loop (immediate retry + backoff reset) rather than spawning;
	// when none is in flight it spawns a fresh resolver. Handled in Rust.
	async function refreshLeague() {
		try {
			await invoke('refresh_league');
		} catch (e) {
			console.warn('[settings] refresh_league failed:', e);
		}
	}

	// --- Screen geometry (SSOT) ---
	// Display-only, like League: the numbers are Rust's (`ssot.screen`, POE-214)
	// and this surface never computes or defaults one. In particular a missing
	// measurement is printed as missing — never as 1.0, which is a REAL
	// measurement (a 1920x1200 screen) and would silently mis-scale every rect
	// on a 1080p machine by 11%.
	let geometryNow = $state(new Date());
	$effect(() => {
		// The measured-at label is relative, so it has to be re-derived off a
		// moving clock or it freezes at "just now" for the rest of the session.
		// 30 s is the resolution of the coarsest unit the helper prints under an
		// hour.
		const tick = setInterval(() => { geometryNow = new Date(); }, 30_000);
		return () => clearInterval(tick);
	});
	// Every rendering decision, including the one that matters, lives in
	// `geometry/view.ts` where a test can reach it.
	const screenGeometry = $derived(screenGeometryView(ssot.screen, geometryNow));

	let recalibrating = $state(false);

	// Drops the remembered screen scale AND the temple's calibration, and forces
	// the temple's next read. Rust owns the whole sequence — see
	// `ssot::geometry_recalibrate` — so this only asks and then re-reads the
	// snapshot, rather than waiting up to a poll interval for the eager nudge.
	async function recalibrateGeometry() {
		recalibrating = true;
		try {
			await invoke('geometry_recalibrate');
			await fetchSsot();
		} catch (e) {
			console.warn('[settings] geometry_recalibrate failed:', e);
		} finally {
			recalibrating = false;
		}
	}

	// Sync: when background checker detects an update, reflect it immediately
	$effect(() => {
		if (store.updateAvailable && updateStatus === 'idle') {
			updateStatus = 'available';
			updateVersion = store.updateVersion;
		}
	});

	async function checkForUpdates() {
		updateStatus = 'checking';
		updateError = '';
		try {
			const update = await checkForUpdate();
			if (update) {
				updateStatus = 'available';
				updateVersion = update.version;
			} else {
				updateStatus = 'idle';
				updateError = 'You are on the latest version.';
			}
		} catch (e: any) {
			updateStatus = 'error';
			updateError = e?.message || String(e);
		}
	}

	async function installUpdate() {
		updateStatus = 'downloading';
		updateError = '';
		try {
			const update = await checkForUpdate();
			if (!update) return;
			await update.downloadAndInstall((progress: { event: string; data?: { contentLength?: number; chunkLength?: number } }) => {
				if (progress.event === 'Started' && progress.data?.contentLength) {
					updateProgress = 0;
				} else if (progress.event === 'Progress') {
					updateProgress += progress.data?.chunkLength ?? 0;
				} else if (progress.event === 'Finished') {
					updateProgress = 0;
				}
			});
			await relaunch();
		} catch (e: any) {
			updateStatus = 'error';
			updateError = e?.message || String(e);
		}
	}

	// Save/Cancel from overlay buttons (overlay-save/overlay-cancel events).
	// Works for OCR region overlays, comparator position, and compass position overlays.
	$effect(() => {
		if (!overlayVisible && !anyPositionOverlayOpen) return;
		const unlistenSave = listen('overlay-save', () => {
			if (overlayVisible) { saveRegion(); return; }
			for (const name of Object.keys(positionOverlays)) {
				if (positionOverlays[name]) { savePositionOverlay(name); return; }
			}
		});
		const unlistenCancel = listen('overlay-cancel', () => {
			if (overlayVisible) { cancelRegion(); return; }
			for (const name of Object.keys(positionOverlays)) {
				if (positionOverlays[name]) { cancelPositionOverlay(name); return; }
			}
		});
		return () => {
			unlistenSave.then(u => u());
			unlistenCancel.then(u => u());
		};
	});

	let overlayWin = $state<any>(null);
	let overlayVisible = $state<string | null>(null); // null = hidden, 'gem' or 'font' = which region

	// Inline editing states
	let editingServerUrl = $state(false);
	let editServerUrlValue = $state('');
	let editingClientTxt = $state(false);
	let editClientTxtValue = $state('');
	// Status is reactive via the shared store — no polling or manual refresh needed.

	// --- Server URL ---
	function startEditServerUrl() {
		editServerUrlValue = store.status?.server_url || '';
		editingServerUrl = true;
	}

	async function saveServerUrl() {
		try {
			await invoke('set_server_url', { url: editServerUrlValue });
			editingServerUrl = false;
			// Status auto-updates via events
		} catch (e) {
			console.error('Failed to save server URL:', e);
		}
	}

	function cancelEditServerUrl() {
		editingServerUrl = false;
	}


	// --- Client.txt Path ---
	function startEditClientTxt() {
		editClientTxtValue = store.status?.client_txt_path || '';
		editingClientTxt = true;
	}

	async function saveClientTxt() {
		try {
			await invoke('set_client_txt_path', { path: editClientTxtValue });
			editingClientTxt = false;
		} catch (e) {
			console.error('Failed to save client.txt path:', e);
		}
	}

	async function browseClientTxt() {
		try {
			await invoke('browse_client_txt');
		} catch (e: any) {
			if (e !== 'No file selected') {
				console.error('Browse failed:', e);
			}
		}
	}

	function cancelEditClientTxt() {
		editingClientTxt = false;
	}

	/** Notify layout that a config overlay is opening/closing. */
	function notifyConfigStart() {
		getCurrentWebviewWindow().emit('overlay-config-start', {}).catch(() => {});
	}
	function notifyConfigEnd() {
		getCurrentWebviewWindow().emit('overlay-config-end', {}).catch(() => {});
	}

	// --- Region Overlay (shared for gem tooltip + font panel) ---
	async function showRegionOverlay(type: 'gem' | 'font') {
		notifyConfigStart();
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');
		if (overlayWin) {
			try { await overlayWin.destroy(); } catch (e) { console.error(e); }
			overlayWin = null;
		}
		// Read fresh from Rust (not store — store may lag behind after save)
		const command = type === 'gem' ? 'get_gem_region' : 'get_font_region';
		const region = await invoke<{ x: number; y: number; w: number; h: number }>(command).catch(() => null);
		const px = region?.x ?? 30;
		const py = region?.y ?? 45;
		const pw = region?.w ?? 550;
		const ph = region?.h ?? (type === 'font' ? 350 : 75);
		// Create without position — constructor DPI conversion is unreliable.
		const win = new WebviewWindow('overlay', {
			url: '/overlay',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: true,
			shadow: false,
			skipTaskbar: true,
			width: 550,
			height: 350,
		});
		win.once('tauri://created', async () => {
			// Set physical position + size (same space as outerPosition used by save)
			await win.setPosition(new PhysicalPosition(px, py))
				.catch(e => console.warn('[region] setPosition failed:', e));
			await win.setSize(new PhysicalSize(pw, ph))
				.catch(e => console.warn('[region] setSize failed:', e));
			overlayWin = win;
			overlayVisible = type;
		});
		win.once('tauri://error', (e: any) => console.error('Overlay failed:', e));
	}

	async function saveRegion() {
		if (!overlayWin || !overlayVisible) return;
		const command = overlayVisible === 'gem' ? 'set_gem_region' : 'set_font_region';
		try {
			const w = overlayWin.window ?? overlayWin;
			const pos = await w.outerPosition();
			const size = await w.outerSize();
			await invoke(command, { x: pos.x, y: pos.y, w: size.width, h: size.height });
		} catch (e) {
			console.error('Save region failed:', e);
			return;
		}
		try { await overlayWin.destroy(); } catch (e) { console.error(e); }
		overlayWin = null;
		overlayVisible = null;
		await reclaimMouse();
	}

	async function cancelRegion() {
		if (!overlayWin) return;
		try { await overlayWin.destroy(); } catch (e) { console.error(e); }
		overlayWin = null;
		overlayVisible = null;
		await reclaimMouse();
	}

	/** After closing a config overlay, emit toggle-reset so the layout
	 *  moves the comparator to its saved position and re-establishes focus. */
	async function reclaimMouse() {
		await getCurrentWebviewWindow().emit('overlay-toggle-reset', {}).catch(() => {});
		notifyConfigEnd();
	}

	function formatRegion(region: any): string {
		if (!region) return 'Not set';
		return `(${region.x}, ${region.y}) ${region.w}\u00d7${region.h}`;
	}

	/** Where the rect beside it came from (POE-233). The row always shows a
	 *  rect — an unset region is a real, derived rect, not a blank — so this is
	 *  the only thing that tells the user whether it follows the screen. */
	function regionSourceLabel(source: 'default' | 'user' | undefined): string {
		if (source === 'user') return '(set by you)';
		if (source === 'default') return '(default, scaled from reference)';
		return '';
	}

	// --- Comparator Overlay Position (red frame for positioning) ---
	// --- Trade Staleness Settings ---
	let tradeStaleWarnSecs = $state(store.status?.trade_stale_warn_secs ?? 120);
	let tradeStaleCriticalSecs = $state(store.status?.trade_stale_critical_secs ?? 600);
	let tradeAutoRefreshSecs = $state(store.status?.trade_auto_refresh_secs ?? 900);
	let editingTradeStaleness = $state(false);
	let tradeStalenessError = $state('');

	// Sync from store when status changes
	$effect(() => {
		if (store.status && !editingTradeStaleness) {
			tradeStaleWarnSecs = store.status.trade_stale_warn_secs ?? 120;
			tradeStaleCriticalSecs = store.status.trade_stale_critical_secs ?? 600;
			tradeAutoRefreshSecs = store.status.trade_auto_refresh_secs ?? 900;
		}
	});

	function startEditTradeStaleness() {
		editingTradeStaleness = true;
	}

	async function saveTradeStaleness() {
		if (tradeStaleWarnSecs >= tradeStaleCriticalSecs) {
			tradeStalenessError = 'Warn threshold must be less than critical threshold.';
			return;
		}
		if (tradeStaleCriticalSecs >= tradeAutoRefreshSecs) {
			tradeStalenessError = 'Critical threshold must be less than auto-refresh interval.';
			return;
		}
		tradeStalenessError = '';
		try {
			await invoke('set_trade_staleness_settings', {
				warnSecs: tradeStaleWarnSecs,
				criticalSecs: tradeStaleCriticalSecs,
				autoRefreshSecs: tradeAutoRefreshSecs,
			});
			editingTradeStaleness = false;
		} catch (e) {
			console.error('Failed to save trade staleness settings:', e);
			tradeStalenessError = 'Failed to save settings. Please try again.';
		}
	}

	function cancelEditTradeStaleness() {
		tradeStaleWarnSecs = store.status?.trade_stale_warn_secs ?? 120;
		tradeStaleCriticalSecs = store.status?.trade_stale_critical_secs ?? 600;
		tradeAutoRefreshSecs = store.status?.trade_auto_refresh_secs ?? 900;
		tradeStalenessError = '';
		editingTradeStaleness = false;
	}

	// --- Generic overlay position config ---
	// DRY: one set of functions for all overlay position configurations.
	interface OverlayConfig {
		label: string;          // window label for position overlay (e.g., 'overlay-comparator-pos')
		syncParam: string;      // URL param (e.g., 'comparator')
		getCommand: string;     // Rust get settings command
		setCommand: string;     // Rust set settings command
		defaultW: number;
		defaultH: number;
		/**
		 * Whether `defaultW`/`defaultH` are CSS pixels needing a scale-factor
		 * conversion before they reach Tauri, or physical pixels already.
		 *
		 * Omitted means physical — the long-standing behaviour of every row
		 * here. Only the merc row declares `'css'`, because its numbers are a
		 * reasoned height budget (a sum of font sizes and padding) and are
		 * shared with the layout that builds the real window. The other four
		 * rows' numbers predate this field and their provenance is not
		 * recorded; they are almost certainly physical figures read off
		 * Sebastian's own display, so converting them would move four windows
		 * on the strength of a guess. Left alone pending a measurement.
		 */
		defaultUnit?: 'css';
	}

	const OVERLAY_CONFIGS: Record<string, OverlayConfig> = {
		comparator: { label: 'overlay-comparator-pos', syncParam: 'comparator', getCommand: 'get_comparator_overlay_settings', setCommand: 'set_comparator_overlay_settings', defaultW: 630, defaultH: 250 },
		compass: { label: 'overlay-compass-pos', syncParam: 'compass', getCommand: 'get_compass_overlay_settings', setCommand: 'set_compass_overlay_settings', defaultW: 300, defaultH: 280 },
		pathstrip: { label: 'overlay-pathstrip-pos', syncParam: 'pathstrip', getCommand: 'get_pathstrip_overlay_settings', setCommand: 'set_pathstrip_overlay_settings', defaultW: 450, defaultH: 180 },
		timer: { label: 'overlay-timer-pos', syncParam: 'timer', getCommand: 'get_timer_overlay_settings', setCommand: 'set_timer_overlay_settings', defaultW: 160, defaultH: 50 },
		// The merc verdict strip (POE-199). It is module-coupled, not toggled
		// here — this row only places it. Configuring while the Merc OCR module
		// is off works and is the normal case: the config window is the one you
		// drag, and the real window picks the geometry up when it is next built.
		//
		// POSITION AND WIDTH ONLY — HEIGHT FOLLOWS CONTENT. The strip sizes its
		// own height to whatever it is drawing (Rust's `fit_overlay_height`), so
		// a height saved from here is written to `mercenary_overlay.height` and
		// then ignored by the real window, which refits on its first paint. The
		// config window is still given a height so it can be seen and dragged:
		// the LIVE height of the running overlay when there is one — which IS
		// the current content height, read through `outerSize()` below — and the
		// constructor seed when the module is off and there is no window to read.
		//
		// Its height is LOCKED rather than dropping it from the resizable list,
		// because width is a real setting the user needs (the guide detail line
		// ellipsises) and Tauri's `resizable` flag has no per-axis form. See
		// `lockConfigHeight` below.
		mercenary: { label: 'overlay-mercenary-pos', syncParam: 'mercenary', getCommand: 'get_mercenary_overlay_settings', setCommand: 'set_mercenary_overlay_settings', defaultW: MERC_OVERLAY_DEFAULTS.w, defaultH: MERC_OVERLAY_DEFAULTS.h, defaultUnit: 'css' },
	};

	/**
	 * The Overlay Positions groups, in display order (POE-226).
	 *
	 * Lab / Merc / Temple, with the five per-window rows unchanged under the
	 * first two and the temple's WIDGETS under the third. A group whose feature
	 * this device lacks is left out entirely rather than disabled — a control
	 * that places an overlay the user can never open is a dead row (POE-203) —
	 * and which groups those are is decided in `$lib/overlay/widgets/overlay-groups`.
	 *
	 * `$derived`, not a constant: the entitlement answer lands after this page is
	 * already mounted.
	 */
	const overlayGroupRows = $derived(
		overlayGroups({ merc: hasFeature(MERC_FEATURE), temple: hasFeature(TEMPLE_FEATURE) })
	);

	/** The modules with widget rows on this page. Read inside handlers as well
	 *  as the load effect, so it is a plain derived rather than a local. */
	const widgetModules = $derived(
		overlayGroupRows
			.map((group) => group.configureModule)
			.filter((module): module is string => module !== null)
	);

	/** The persisted widget placements, by widget id. A widget with no entry has
	 *  never been placed and draws where the registry ships it. */
	let widgetGeometries = $state<Record<string, WidgetGeometry>>({});

	/** The module whose widgets are being arranged in its own overlay window
	 *  right now, or null. Set when Configure is pressed, cleared by the host's
	 *  `widget-config-end`. */
	let widgetConfiguring = $state<string | null>(null);

	/**
	 * The scale factor of the monitor the widget overlay lives on.
	 *
	 * The overlay's display, not this window's: a widget overlay is built on the
	 * GAME's monitor (`routes/(app)/+layout.svelte`, POE-237), so that is the
	 * display a widget's physical coordinates are measured against. Reading this
	 * window's factor instead would be wrong by the ratio between the two
	 * whenever the main window sits on a second display with different scaling —
	 * and silently, since it agrees on a single-monitor machine.
	 *
	 * The CHOICE is `chooseMonitor`'s, shared with the layout rather than
	 * re-spelled, because the two answers have to be the same display: the
	 * layout sizes the canvas from it and this converts the shipped CSS defaults
	 * into coordinates inside that canvas. The primary is the fallback on every
	 * failing path, exactly as the layout falls back.
	 *
	 * Zero until it answers, and the Show toggle declines while it is: creating a
	 * placement row means converting the registry's CSS defaults to the physical
	 * pixels Rust stores, and doing that at zero would write the widget to the
	 * origin.
	 *
	 * RE-RESOLVED on `game-monitor-changed`, not read once at mount: the answer
	 * is a property of whichever display the game is on, and the player moving
	 * PoE to a screen with different scaling changes it. Holding the mount-time
	 * factor would place every widget Show creates from then on by the old
	 * display's ratio, into a canvas the layout has already rebuilt on the new
	 * one.
	 */
	let widgetScaleFactor = $state(0);

	async function resolveWidgetScaleFactor(): Promise<void> {
		const { availableMonitors, currentMonitor, primaryMonitor } = await import(
			'@tauri-apps/api/window'
		);
		const primary =
			(await primaryMonitor().catch(() => null)) ?? (await currentMonitor().catch(() => null));
		const game = await invoke<GameMonitorInfo | null>('get_game_monitor').catch((e: any) => {
			console.warn('[settings] get_game_monitor failed, using the primary monitor:', e);
			return null;
		});
		const listed = await availableMonitors().catch((e: any) => {
			console.warn('[settings] availableMonitors failed, using the primary monitor:', e);
			return [];
		});
		const monitor = chooseMonitor(game, listed, primary);
		if (monitor && monitor.scaleFactor > 0) widgetScaleFactor = monitor.scaleFactor;
		else console.warn('[settings] no monitor scale factor — Show cannot place a widget yet');
	}

	$effect(() => {
		resolveWidgetScaleFactor().catch((e: any) =>
			console.warn('[settings] monitor lookup failed:', e)
		);
	});

	$effect(() => {
		// Window-scoped, because Rust sends this with `emit_to("main")` — a bare
		// `listen` on the global bus never hears it (the guide's webview-scoped
		// rule). The payload is deliberately unused: the id it carries is Rust's
		// enumeration, and what this needs is the SAME display's scale factor out
		// of the webview's enumeration, which is exactly what the resolve above
		// asks `chooseMonitor` for.
		const moved = getCurrentWebviewWindow().listen<GameMonitorInfo>('game-monitor-changed', () => {
			resolveWidgetScaleFactor().catch((e: any) =>
				console.warn('[settings] monitor lookup after a game-monitor-changed failed:', e)
			);
		});
		return () => {
			moved.then((unlisten) => unlisten()).catch(() => {});
		};
	});

	/**
	 * Re-read one or more modules' placements.
	 *
	 * Per module rather than wholesale, because `widget-config-end` names one:
	 * the ids of the module being refreshed are dropped and replaced with what
	 * Rust answers, and every other module's rows are left as they were. The
	 * `"<module>."` prefix is the same rule Rust's `widgets_for_module` uses, and
	 * `widget-registry.test.ts` pins that an id's halves agree with its module.
	 *
	 * The previous map is read through `untrack`: this runs from an effect, and a
	 * tracked read of the state it writes would re-run itself forever.
	 */
	async function loadWidgetGeometries(modules: string[]): Promise<void> {
		for (const module of modules) {
			try {
				const rows = await invoke<{ id: string; geometry: WidgetGeometry }[]>(
					'get_widget_geometries',
					{ module }
				);
				const next: Record<string, WidgetGeometry> = {};
				for (const [id, geometry] of Object.entries(untrack(() => widgetGeometries))) {
					if (!id.startsWith(`${module}.`)) next[id] = geometry;
				}
				for (const row of rows) next[row.id] = row.geometry;
				widgetGeometries = next;
			} catch (e) {
				// The rows fall back to "Not set", which is what a widget with no
				// placement genuinely shows — hence the log: otherwise a dead IPC
				// reads as a user who never configured anything.
				console.warn(`[settings] could not read the ${module} widget placements:`, e);
			}
		}
	}

	$effect(() => {
		const modules = widgetModules;
		if (modules.length > 0) loadWidgetGeometries(modules);
	});

	/**
	 * Show or hide one widget, preserving everything else about its placement.
	 *
	 * A widget with no stored row gets one written from the registry's shipped
	 * defaults, converted to physical pixels — and with a ZERO size, because that
	 * is what the host reads back as "let the content decide" (`placementFor`).
	 * Writing a measured size here would pin a widget the user never resized.
	 *
	 * The rune is updated first so the checkbox does not lag the click, and a
	 * rejected write is undone by re-reading rather than by guessing what Rust
	 * kept.
	 */
	async function setWidgetVisible(spec: WidgetSpec, visible: boolean): Promise<void> {
		const current = widgetGeometries[spec.id];
		let geometry: WidgetGeometry;
		if (current) {
			geometry = { ...current, visible };
		} else {
			if (widgetScaleFactor === 0) {
				console.warn(`[settings] no scale factor yet — not placing ${spec.id}`);
				return;
			}
			const at = physicalGeometry(spec.defaults, widgetScaleFactor);
			geometry = { x: at.x, y: at.y, width: 0, height: 0, visible };
		}
		widgetGeometries = { ...widgetGeometries, [spec.id]: geometry };
		try {
			await invoke('set_widget_geometry', { id: spec.id, geometry });
		} catch (e) {
			console.warn(`[settings] could not save the ${spec.id} visibility:`, e);
			await loadWidgetGeometries([spec.module]);
		}
	}

	/**
	 * Ask the layout to open config mode on a module's overlay window.
	 *
	 * Settings owns none of the three ordering steps itself
	 * (`docs/OVERLAY-GUIDE.md`, "Config-mode ordering contract") — the layout
	 * does, because it is the file that builds and owns that window. This emits
	 * the request and then waits for `widget-config-end`, which the HOST sends
	 * after Save or Cancel and the layout sends if it could not open the window
	 * at all.
	 *
	 * Pressing it again while a session is live is deliberate, not a bug to
	 * guard: it is the user's way out of a window that somehow missed the event,
	 * and both entering config mode and this request are idempotent.
	 */
	/**
	 * How long the button waits for the layout to say config mode is OPEN.
	 *
	 * Not a limit on the arranging session — that ends when the user presses Save
	 * or Cancel, however long they take. This bounds the OPENING only, and it has
	 * to sit above the layout's own bound on the same work: it waits up to 10 s
	 * for the window to be built and then makes three more IPC calls, and every
	 * failure it can see it already reports as `widget-config-end`. What is left
	 * for this timer is the failure the layout cannot report — its listener never
	 * registered, or the emit never arrived — which is exactly the case where
	 * nothing else will ever clear the button.
	 */
	const WIDGET_CONFIG_ACK_MS = 20_000;
	let widgetConfigAckTimer: ReturnType<typeof setTimeout> | null = null;
	/** Whether the layout has said it PICKED THE REQUEST UP (`widget-config-opening`)
	 *  since this button was pressed. */
	let widgetConfigPickedUp = false;

	function clearWidgetConfigAck(): void {
		if (widgetConfigAckTimer === null) return;
		clearTimeout(widgetConfigAckTimer);
		widgetConfigAckTimer = null;
	}

	/**
	 * Arm the opening deadline.
	 *
	 * On expiry there are two different situations and only one of them may be
	 * abandoned. If the layout never said it picked the request up, nothing is
	 * running and nothing else will ever clear the button — end it. If it DID,
	 * a start is still in flight (a hung IPC inside a 10 s window wait, say),
	 * and tearing the session down now would leave that start setting config
	 * mode on a window this very path had torn down. So the deadline is given
	 * one more period, once, and the pick-up is forgotten so the second expiry
	 * ends it either way.
	 */
	function armWidgetConfigAck(module: string): void {
		clearWidgetConfigAck();
		widgetConfigAckTimer = setTimeout(() => {
			widgetConfigAckTimer = null;
			if (widgetConfiguring !== module) return;
			if (widgetConfigPickedUp) {
				widgetConfigPickedUp = false;
				console.warn(`[settings] ${module} config mode is slow to open — waiting once more`);
				armWidgetConfigAck(module);
				return;
			}
			console.warn(`[settings] no answer to widget config mode for ${module} — giving up`);
			widgetConfiguring = null;
			// Told, not just forgotten: if the layout DID force the module on and
			// then went quiet, this is what gets it switched back off.
			getCurrentWebviewWindow().emit('widget-config-end', { module }).catch(() => {});
		}, WIDGET_CONFIG_ACK_MS);
	}

	async function configureWidgets(module: string): Promise<void> {
		widgetConfiguring = module;
		widgetConfigPickedUp = false;
		armWidgetConfigAck(module);
		try {
			await getCurrentWebviewWindow().emit('widget-config-start', { module });
		} catch (e) {
			// Nothing is going to answer, so the button must not sit on
			// "Configuring…" forever.
			console.warn('[settings] could not ask for widget config mode:', e);
			clearWidgetConfigAck();
			widgetConfiguring = null;
		}
	}

	$effect(() => {
		// The layout has the request. It does not mean config mode is open — that
		// is `widget-config-open` below — only that abandoning it now would be
		// abandoning work in progress.
		const picked = listen<{ module?: string }>('widget-config-opening', (event) => {
			if (event.payload?.module === widgetConfiguring) widgetConfigPickedUp = true;
		});
		return () => {
			picked.then((unlisten) => unlisten()).catch(() => {});
		};
	});

	$effect(() => {
		// The layout's acknowledgement that the window is interactive and the host
		// has been told. The button stays on "Configuring…" — that state is now
		// true rather than hopeful — and only the deadline is stood down.
		const opened = listen<{ module?: string }>('widget-config-open', (event) => {
			if (event.payload?.module !== widgetConfiguring) return;
			clearWidgetConfigAck();
			widgetConfigPickedUp = false;
		});
		return () => {
			opened.then((unlisten) => unlisten()).catch(() => {});
		};
	});

	$effect(() => {
		const pending = listen<{ module?: string }>('widget-config-end', (event) => {
			const module = event.payload?.module;
			if (!module) return;
			if (widgetConfiguring === module) {
				widgetConfiguring = null;
				clearWidgetConfigAck();
				widgetConfigPickedUp = false;
			}
			// Save wrote through `set_widget_geometry` in the overlay window, and
			// Cancel may have restored a map this page has a stale copy of.
			loadWidgetGeometries([module]);
		});
		return () => {
			pending.then((unlisten) => unlisten()).catch(() => {});
		};
	});

	// Per-overlay state
	let overlaySettings = $state<Record<string, { x: number; y: number; width: number; height: number } | null>>({
		comparator: null, compass: null, pathstrip: null, timer: null, mercenary: null,
	});
	let positionOverlays = $state<Record<string, any>>({
		comparator: null, compass: null, pathstrip: null, timer: null, mercenary: null,
	});

	// --- Timer appearance ---
	let timerBgOpacity = $state(75);
	let timerTextStroke = $state(true);
	let savedBgOpacity = $state(75);
	let savedTextStroke = $state(true);
	let timerAppearanceDirty = $derived(
		timerBgOpacity !== savedBgOpacity || timerTextStroke !== savedTextStroke
	);

	function saveTimerAppearance() {
		invoke('set_timer_appearance', { bgOpacity: timerBgOpacity / 100, textStroke: timerTextStroke })
			.then(() => {
				savedBgOpacity = timerBgOpacity;
				savedTextStroke = timerTextStroke;
			})
			.catch((e: any) => console.warn('[settings] save timer appearance failed:', e));
	}

	// Load all overlay settings on init
	$effect(() => {
		for (const [name, cfg] of Object.entries(OVERLAY_CONFIGS)) {
			invoke<any>(cfg.getCommand).then((s) => {
				if (s) overlaySettings[name] = s;
			}).catch((e) => console.warn(`[settings] failed to load ${name} overlay settings:`, e));
		}
		invoke<any>('get_timer_appearance').then((a) => {
			if (a) {
				timerBgOpacity = Math.round(a.bg_opacity * 100);
				timerTextStroke = a.text_stroke;
				savedBgOpacity = timerBgOpacity;
				savedTextStroke = a.text_stroke;
			}
		}).catch((e: any) => console.warn('[settings] load timer appearance failed:', e));
	});

	async function showPositionOverlay(name: string) {
		const cfg = OVERLAY_CONFIGS[name];
		if (!cfg) return;
		notifyConfigStart();
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');
		if (positionOverlays[name]) {
			try { await positionOverlays[name].destroy(); } catch (_) {}
			positionOverlays[name] = null;
		}
		// Read live overlay position/size (physical pixels) so the config window
		// matches exactly. This prevents the sync loop from resizing the real overlay.
		const live = await invoke<any>(cfg.getCommand).catch(() => null);
		const realWin = await WebviewWindow.getByLabel(cfg.syncParam);
		// Read up front: a CSS-unit default has to be converted before it can be
		// used as a physical size, and the same factor converts physical →
		// logical for the constructor further down.
		const mainWin = getCurrentWebviewWindow();
		const sf = await mainWin.scaleFactor().catch((e: any) => { console.warn('[settings] scaleFactor failed, using 1:', e); return 1; });
		// A saved geometry is already physical. A shipped default is physical
		// too UNLESS the row says otherwise — see `defaultUnit`. Getting this
		// wrong is invisible at 100 % scaling and clips the window by a third at
		// 150 %, which is why the unit is declared rather than assumed.
		const shipped = cfg.defaultUnit === 'css'
			? physicalGeometry({ x: 0, y: 0, w: cfg.defaultW, h: cfg.defaultH }, sf)
			: { x: 0, y: 0, w: cfg.defaultW, h: cfg.defaultH };
		let physX = live?.x ?? 100, physY = live?.y ?? 100;
		let physW = shipped.w, physH = shipped.h;
		if (realWin) {
			try {
				const pos = await realWin.outerPosition();
				const size = await realWin.outerSize();
				physX = pos.x; physY = pos.y;
				physW = size.width; physH = size.height;
			} catch (e) {
				console.warn(`[settings] failed to read live ${name} overlay position/size, using saved:`, e);
			}
		} else if (live) {
			physW = live.width ?? shipped.w;
			physH = live.height ?? shipped.h;
		}
		// Save pre-configure state (physical pixels) so cancel restores it
		overlaySettings[name] = { x: physX, y: physY, width: physW, height: physH };
		// Constructor takes logical pixels; convert physical → logical.
		// setSize(PhysicalSize) in tauri://created will set the exact physical size.
		const win = new WebviewWindow(cfg.label, {
			url: `/overlay?sync=${cfg.syncParam}`,
			transparent: true, decorations: false, alwaysOnTop: true,
			resizable: ['compass', 'pathstrip', 'timer', 'mercenary'].includes(name), shadow: false, skipTaskbar: true,
			width: Math.round(physW / sf), height: Math.round(physH / sf),
		});
		win.once('tauri://created', async () => {
			await win.setPosition(new PhysicalPosition(physX, physY));
			await win.setSize(new PhysicalSize(physW, physH));
			await lockConfigHeight(name, win, physH / sf);
			positionOverlays[name] = win;
		});
		win.once('tauri://error', (e: any) => console.error(`${name} position overlay failed:`, e));
	}

	/**
	 * Pin a config window's height when the real overlay owns that axis.
	 *
	 * Only the merc strip does: it sizes itself to its content, so a height
	 * dragged here would look like a setting and then be silently discarded on
	 * the overlay's first paint. Width still has to be draggable — the guide
	 * detail line ellipsises, so a user with long guide names genuinely needs a
	 * wider strip — and Tauri's `resizable` flag is per-window, not per-axis.
	 * Constraints are the only way to keep one axis and lose the other.
	 *
	 * Constraints are LOGICAL pixels, unlike everything else on this path.
	 *
	 * A failure here is cosmetic (the height becomes draggable again and the
	 * drag is ignored later), so it is logged rather than aborting the window.
	 */
	async function lockConfigHeight(name: string, win: any, logicalH: number) {
		if (name !== 'mercenary') return;
		try {
			await win.setSizeConstraints({ minHeight: logicalH, maxHeight: logicalH });
		} catch (e) {
			console.warn(`[settings] could not lock the ${name} config window height:`, e);
		}
	}

	async function savePositionOverlay(name: string) {
		const cfg = OVERLAY_CONFIGS[name];
		const win = positionOverlays[name];
		if (!cfg || !win) return;
		try {
			const ref = win.window ?? win;
			const pos = await ref.outerPosition();
			const size = await ref.outerSize();
			await invoke(cfg.setCommand, { x: pos.x, y: pos.y, w: size.width, h: size.height, enabled: true });
			overlaySettings[name] = { x: pos.x, y: pos.y, width: size.width, height: size.height };
		} catch (e) {
			console.error(`Save ${name} position failed:`, e);
			return;
		}
		try { await win.destroy(); } catch (_) {}
		positionOverlays[name] = null;
		await reclaimMouse();
	}

	async function cancelPositionOverlay(name: string) {
		const cfg = OVERLAY_CONFIGS[name];
		const win = positionOverlays[name];
		if (!win) return;
		try { await win.destroy(); } catch (_) {}
		positionOverlays[name] = null;
		// Persist pre-configure state so overlay-toggle-reset (from reclaimMouse)
		// restores the overlay to its exact pre-configure size.
		const pre = overlaySettings[name];
		if (pre && cfg) {
			await invoke(cfg.setCommand, {
				x: pre.x, y: pre.y, w: pre.width, h: pre.height, enabled: true,
			}).catch((e: any) => console.warn(`[settings] failed to restore ${name} pre-configure position:`, e));
		}
		await reclaimMouse();
	}

	// Convenience: check if any position overlay is open (for overlay-save/cancel guard)
	let anyPositionOverlayOpen = $derived(
		Object.values(positionOverlays).some(w => w !== null)
	);

	/** Whether either Configure button in Overlay Positions may be pressed. The
	 *  three flows are mutually exclusive — see `canStartConfigure`. */
	let configureAllowed = $derived(
		canStartConfigure({
			region: !!overlayVisible,
			position: anyPositionOverlayOpen,
			widgets: widgetConfiguring !== null
		})
	);
</script>

<div class="settings-page">
	<h1>Settings</h1>

		<!-- About & Updates -->
		<section>
			<h2>About</h2>

			<div class="setting-row">
				<span class="setting-label">Version</span>
				<span class="setting-value mono">{appVersion}</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Updates</span>
				{#if updateStatus === 'checking'}
					<span class="setting-value muted">Checking...</span>
				{:else if updateStatus === 'available'}
					<span class="setting-value update-available">v{updateVersion} available</span>
					<Button variant="save" onclick={installUpdate}>Install & Restart</Button>
				{:else if updateStatus === 'downloading'}
					<span class="setting-value muted">Downloading... {updateProgress > 0 ? `(${Math.round(updateProgress / 1024)}KB)` : ''}</span>
				{:else if updateStatus === 'error'}
					<span class="setting-value update-error">{updateError}</span>
					<Button onclick={checkForUpdates}>Retry</Button>
				{:else}
					{#if updateError}
						<span class="setting-value muted">{updateError}</span>
					{/if}
					<Button onclick={checkForUpdates}>Check for Updates</Button>
				{/if}
			</div>
		</section>

		<!-- General -->
		<section>
			<h2>General</h2>

			{#if import.meta.env.DEV}
			<div class="setting-row">
				<span class="setting-label">Server URL</span>
				{#if editingServerUrl}
					<div class="setting-edit">
						<input
							type="text"
							class="setting-input"
							bind:value={editServerUrlValue}
							onkeydown={(e) => { if (e.key === 'Enter') saveServerUrl(); if (e.key === 'Escape') cancelEditServerUrl(); }}
						/>
						<Button variant="save" onclick={saveServerUrl}>Save</Button>
						<Button onclick={cancelEditServerUrl}>Cancel</Button>
					</div>
				{:else}
					<span class="setting-value">{store.status?.server_url ?? '...'}</span>
					<Button onclick={startEditServerUrl}>Edit</Button>
				{/if}
			</div>
			{/if}

			<div class="setting-row">
				<span class="setting-label">League</span>
				{#if ssot.resolving && ssot.unreachable}
					<span class="setting-value muted">Server unreachable — still retrying</span>
					<Button onclick={refreshLeague}>Refresh</Button>
				{:else if ssot.resolving}
					<span class="setting-value muted">Resolving…</span>
					<Button onclick={refreshLeague} disabled>Refresh</Button>
				{:else if ssot.league == null}
					<span class="setting-value muted">Not detected — server may be unreachable</span>
					<Button onclick={refreshLeague}>Refresh</Button>
				{:else}
					<span class="setting-value">{ssot.league}</span>
					<Button onclick={refreshLeague}>Refresh</Button>
				{/if}
			</div>

		</section>

		<!-- Game Integration -->
		<section>
			<h2>Game Integration</h2>

			{#if store.status && !store.status.client_txt_exists}
				<div class="warning-banner">
					Client.txt not found at the configured path. Lab detection, OCR, and compass will not work. Use Browse to locate your Path of Exile Client.txt file.
				</div>
			{/if}

			<div class="setting-row">
				<span class="setting-label">Client.txt Path</span>
				{#if editingClientTxt}
					<div class="setting-edit">
						<input
							type="text"
							class="setting-input"
							bind:value={editClientTxtValue}
							onkeydown={(e) => { if (e.key === 'Enter') saveClientTxt(); if (e.key === 'Escape') cancelEditClientTxt(); }}
						/>
						<Button variant="save" onclick={saveClientTxt}>Save</Button>
						<Button onclick={cancelEditClientTxt}>Cancel</Button>
					</div>
				{:else}
					<span class="setting-value path" class:path-missing={!store.status?.client_txt_exists}>{store.status?.client_txt_path ?? '...'}</span>
					<Button onclick={browseClientTxt}>Browse</Button>
					<Button onclick={startEditClientTxt}>Edit</Button>
					<Button onclick={() => invoke('reset_client_txt_path').catch(e => console.error(e))} title="Auto-detect GGG or Steam install">Reset</Button>
				{/if}
			</div>

		</section>

		<!-- OCR Regions -->
		<section>
			<h2>OCR Regions</h2>

			{#if store.status?.ocr_language_warning}
				<div class="warning-banner">
					{store.status.ocr_language_warning}
				</div>
			{/if}

			<div class="setting-row">
				<span class="setting-label">Gem Tooltip Region</span>
				{#if overlayVisible === 'gem'}
					<span class="setting-value">Positioning overlay...</span>
					<Button variant="save" onclick={saveRegion}>Save</Button>
					<Button onclick={cancelRegion}>Cancel</Button>
				{:else}
					<span class="setting-value mono">{formatRegion(store.status?.gem_region)}</span>
					<span class="region-source">{regionSourceLabel(store.status?.gem_region_source)}</span>
					<Button onclick={() => showRegionOverlay('gem')} disabled={!!overlayVisible}>Configure</Button>
					{#if store.status?.gem_region_source === 'user'}
						<Button onclick={() => invoke('reset_gem_region').catch(e => console.error(e))} title="Follow the measured screen again">Reset to default</Button>
					{/if}
				{/if}
			</div>

			<div class="setting-row">
				<span class="setting-label">Font Panel Region</span>
				{#if overlayVisible === 'font'}
					<span class="setting-value">Positioning overlay...</span>
					<Button variant="save" onclick={saveRegion}>Save</Button>
					<Button onclick={cancelRegion}>Cancel</Button>
				{:else}
					<span class="setting-value mono">{formatRegion(store.status?.font_region)}</span>
					<span class="region-source">{regionSourceLabel(store.status?.font_region_source)}</span>
					<Button onclick={() => showRegionOverlay('font')} disabled={!!overlayVisible}>Configure</Button>
					{#if store.status?.font_region_source === 'user'}
						<Button onclick={() => invoke('reset_font_region').catch(e => console.error(e))} title="Follow the measured screen again">Reset to default</Button>
					{/if}
				{/if}
			</div>
		</section>

		<!-- Screen geometry -->
		<section>
			<h2>Screen geometry</h2>

			<div class="setting-row">
				<span class="setting-label">Resolution</span>
				<span class="setting-value" class:mono={!screenGeometry.unmeasured} class:muted={screenGeometry.unmeasured}>{screenGeometry.resolution}</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">UI scale</span>
				<span class="setting-value" class:mono={!screenGeometry.unmeasured} class:muted={screenGeometry.unmeasured}>{screenGeometry.uiScale}</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Measured by</span>
				<span class="setting-value" class:muted={screenGeometry.unmeasured}>{screenGeometry.source}</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Verified this session</span>
				<span class="setting-value" class:muted={screenGeometry.unmeasured}>{screenGeometry.verified}</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Measured</span>
				<span class="setting-value" class:muted={screenGeometry.unmeasured}>{screenGeometry.measured}</span>
				<Button onclick={recalibrateGeometry} disabled={recalibrating}>Recalibrate</Button>
			</div>

			<p class="setting-note">
				Remembered once measured; verified on use; re-measured only when the screen size
				changes, verification fails, or you press Recalibrate.
			</p>
		</section>

		<!-- Overlays -->
		<section>
			<h2>Overlay Positions</h2>

			{#each overlayGroupRows as group (group.id)}
				<h3 class="group-heading">{group.heading}</h3>

				{#each group.windows as cfg (cfg.name)}
					<div class="setting-row">
						<span class="setting-label">{cfg.label}</span>
						{#if positionOverlays[cfg.name]}
							<span class="setting-value">Drag overlay to position...</span>
							<Button variant="save" onclick={() => savePositionOverlay(cfg.name)}>Save</Button>
							<Button onclick={() => cancelPositionOverlay(cfg.name)}>Cancel</Button>
						{:else}
							{@const s = overlaySettings[cfg.name]}
							<span class="setting-value mono">{s ? `(${s.x}, ${s.y}) ${s.width}\u00d7${s.height}` : 'Not set'}</span>
							<Button onclick={() => showPositionOverlay(cfg.name)} disabled={!configureAllowed}>Configure</Button>
						{/if}
					</div>
				{/each}

				{#each group.widgets as widget (widget.id)}
					<div class="setting-row">
						<span class="setting-label">{widget.label}</span>
						<span class="widget-show">
							Show
							<Toggle
								checked={widgetGeometries[widget.id]?.visible ?? true}
								label={widget.label}
								onchange={(next) => setWidgetVisible(widget, next)}
							/>
						</span>
						<span class="setting-value mono">{widgetGeometryText(widgetGeometries[widget.id])}</span>
					</div>
				{/each}

				{#if group.configureModule}
					{@const module = group.configureModule}
					<div class="setting-row">
						<span class="setting-label"></span>
						<span class="setting-value">
							{widgetConfiguring === module ? 'Save or Cancel in the overlay' : ''}
						</span>
						<Button onclick={() => configureWidgets(module)} disabled={!configureAllowed}>
							{widgetConfiguring === module ? 'Configuring\u2026' : 'Configure widgets'}
						</Button>
					</div>
				{/if}
			{/each}
		</section>

		<!-- Timer Appearance -->
		<section>
			<h2>Timer Appearance</h2>

			<div class="setting-row">
				<span class="setting-label">Background</span>
				<RangeSlider bind:value={timerBgOpacity} min={0} max={100} step={5} formatValue={(v) => `${v}%`} />
			</div>

			<div class="setting-row">
				<span class="setting-label">Text outline</span>
				<Toggle bind:checked={timerTextStroke} />
			</div>

			<div class="setting-row">
				<span class="setting-label"></span>
				<Button variant="save" onclick={saveTimerAppearance} disabled={!timerAppearanceDirty}>Apply</Button>
			</div>
		</section>

		<!-- Trade -->
		<section>
			<h2>Trade</h2>

			<div class="setting-row">
				<Tooltip text="After this many seconds, trade data shows a yellow warning indicator in the comparator and overlay. Signals that the cached prices may be getting outdated.">
					<span class="setting-label">Stale warning (sec)</span>
				</Tooltip>
				{#if editingTradeStaleness}
					<div class="setting-edit">
						<input
							type="number"
							class="setting-input narrow"
							bind:value={tradeStaleWarnSecs}
							min="30"
							max="3600"
						/>
					</div>
				{:else}
					<span class="setting-value mono">{store.status?.trade_stale_warn_secs ?? 120}s</span>
				{/if}
			</div>

			<div class="setting-row">
				<Tooltip text="After this many seconds, trade data shows a red critical indicator. The cached prices are likely outdated and should be refreshed before making decisions.">
					<span class="setting-label">Stale critical (sec)</span>
				</Tooltip>
				{#if editingTradeStaleness}
					<div class="setting-edit">
						<input
							type="number"
							class="setting-input narrow"
							bind:value={tradeStaleCriticalSecs}
							min="60"
							max="7200"
						/>
					</div>
				{:else}
					<span class="setting-value mono">{store.status?.trade_stale_critical_secs ?? 600}s</span>
				{/if}
			</div>

			<div class="setting-row">
				<Tooltip text="When auto-trade is enabled, trade data older than this is automatically re-fetched from GGG when a gem appears in the comparator. Set higher to reduce API calls, lower for fresher data.">
					<span class="setting-label">Auto-refresh (sec)</span>
				</Tooltip>
				{#if editingTradeStaleness}
					<div class="setting-edit">
						<input
							type="number"
							class="setting-input narrow"
							bind:value={tradeAutoRefreshSecs}
							min="60"
							max="7200"
						/>
					</div>
				{:else}
					<span class="setting-value mono">{store.status?.trade_auto_refresh_secs ?? 900}s</span>
				{/if}
			</div>

			<div class="setting-row">
				<span class="setting-label"></span>
				{#if editingTradeStaleness}
					<Button variant="save" onclick={saveTradeStaleness}>Save</Button>
					<Button onclick={cancelEditTradeStaleness}>Cancel</Button>
				{:else}
					<Button onclick={startEditTradeStaleness}>Edit</Button>
				{/if}
			</div>
			{#if tradeStalenessError}
				<div class="setting-row">
					<span class="setting-label"></span>
					<span class="setting-error">{tradeStalenessError}</span>
				</div>
			{/if}
		</section>

		<!-- Danger Zone -->
		<section class="danger-section">
			<h2>Danger Zone</h2>
			<div class="setting-row">
				<span class="setting-label">Reset All Settings</span>
				<span class="setting-value">Deletes settings file and re-detects everything</span>
				<Button variant="danger" onclick={() => {
					if (confirm('Reset all settings to defaults? This will clear all overlay positions, Client.txt path, and trade settings. The app will re-detect your PoE installation.')) {
						invoke('reset_all_settings').then(() => {
							alert('Settings reset. The app will now use fresh defaults.');
						}).catch(e => console.error('Reset failed:', e));
					}
				}}>Reset Everything</Button>
			</div>
		</section>

		<!-- Logs -->
		{#if store.logs.length > 0}
			<section>
				<div class="log-header">
					<h2>Logs</h2>
					<Button onclick={() => { navigator.clipboard.writeText(store.logs.toReversed().join('\n')); }}>Copy</Button>
				</div>
				<div class="log-list">
					{#each store.logs.toReversed() as line}
						<div class="log-line" class:log-error={line.includes('failed') || line.includes('error')}>{line}</div>
					{/each}
				</div>
			</section>
		{/if}

</div>

<style>
	.settings-page {
		max-width: 520px;
		margin: 0 auto;
	}

	h1 {
		font-size: 1.2rem;
		color: var(--accent);
		margin-bottom: 1.5rem;
	}

	section {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 1rem;
		margin-bottom: 1rem;
	}

	h2 {
		font-size: 0.8rem;
		text-transform: uppercase;
		color: var(--text-muted);
		margin-bottom: 0.75rem;
	}

	/* A group inside Overlay Positions (POE-226). Quieter than the section's own
	   h2 — it separates rows that already share a heading, it does not compete
	   with it. It also BREAKS the `.setting-row + .setting-row` rule below, which
	   is what stops a top border being drawn between two groups. */
	.group-heading {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--text-muted);
		margin: 0.75rem 0 0.15rem;
	}

	.group-heading:first-of-type {
		margin-top: 0;
	}

	.widget-show {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		flex-shrink: 0;
		font-size: 0.75rem;
		color: var(--text-muted);
	}



	.setting-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.4rem 0;
		min-height: 32px;
	}

	.setting-row + .setting-row {
		border-top: 1px solid rgba(255, 255, 255, 0.05);
	}

	.setting-label {
		min-width: 140px;
		flex-shrink: 0;
		font-size: 0.85rem;
		color: var(--text);
	}

	.setting-value {
		flex: 1;
		font-size: 0.8rem;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.setting-value.mono {
		font-family: 'Consolas', 'Courier New', monospace;
		letter-spacing: 0.1em;
	}

	.region-source {
		font-size: 0.72rem;
		color: var(--text-muted);
		white-space: nowrap;
	}

	.setting-note {
		margin: 0.35rem 0 0;
		font-size: 0.72rem;
		line-height: 1.4;
		color: var(--text-muted);
	}

	.update-available {
		color: var(--success, #22c55e);
		font-weight: 600;
	}

	.update-error {
		color: var(--color-lab-red, #ef4444);
		font-size: 0.75rem;
	}

	.setting-value.path {
		font-size: 0.7rem;
		font-family: 'Consolas', 'Courier New', monospace;
	}

	.setting-value.path-missing {
		color: var(--accent, #ef4444);
	}

	.warning-banner {
		background: rgba(239, 68, 68, 0.15);
		border: 1px solid rgba(239, 68, 68, 0.4);
		border-radius: 6px;
		padding: 8px 12px;
		margin-bottom: 8px;
		font-size: 0.8rem;
		color: #fca5a5;
		line-height: 1.4;
	}

	.setting-value.muted {
		color: var(--border);
		font-style: italic;
	}

	.setting-edit {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}

	.setting-input {
		flex: 1;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		padding: 0.25rem 0.4rem;
		border-radius: 4px;
		font-size: 0.75rem;
		font-family: 'Consolas', 'Courier New', monospace;
	}

	.setting-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.setting-input.narrow {
		max-width: 100px;
	}

	.setting-error {
		color: var(--color-lab-red, #ef4444);
		font-size: 0.75rem;
	}

	.danger-section {
		border-color: rgba(239, 68, 68, 0.3);
	}

	.danger-section h2 {
		color: #ef4444;
	}

	.log-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.5rem;
	}

	.log-header h2 {
		margin-bottom: 0;
	}

	.log-list {
		max-height: 250px;
		overflow-y: auto;
		font-family: 'Consolas', 'Courier New', monospace;
		font-size: 0.7rem;
		line-height: 1.4;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 8px 12px;
	}
	.log-line {
		color: var(--text-muted);
		padding: 0.1rem 0;
		border-bottom: 1px solid rgba(255, 255, 255, 0.03);
	}
	.log-error {
		color: var(--color-lab-red, #ef4444);
	}
</style>
