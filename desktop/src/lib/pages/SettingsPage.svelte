<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { createSettingsUpdateController } from '$lib/updater/settings-update.svelte';
	import { store } from '$lib/stores/status.svelte';
	import { hasFeature, MERC_FEATURE, TEMPLE_FEATURE } from '$lib/stores/entitlements.svelte';
	import { ssot, fetchSsot } from '$lib/stores/ssot.svelte';
	import { nav } from '$lib/stores/navigation.svelte';
	import { untrack } from 'svelte';
	import { ocrRectsKey, screenGeometryView } from '$lib/geometry/view';
	import { createOcrPreviewOwner } from '$lib/overlay/ocr-preview';
	import type { GameMonitorInfo } from '$lib/overlay/monitor-choice';
	import {
		canStartConfigure,
		overlayGroups,
		widgetGeometryText
	} from '$lib/overlay/widgets/overlay-groups';
	import {
		loadWidgetGeometries,
		resolveWidgetScaleFactor,
		setWidgetVisible,
		widgetPlacements
	} from '$lib/overlay/widgets/widget-placements.svelte';
	import Tooltip from '$lib/components/Tooltip.svelte';
	import Toggle from '$lib/components/Toggle.svelte';
	import RangeSlider from '$lib/components/RangeSlider.svelte';
	import Button from '$lib/components/Button.svelte';
	import { getVersion } from '@tauri-apps/api/app';
	type OcrRectView = {
		key: string;
		label: string;
		rect: [number, number, number, number] | null;
		rows: [number, number, number, number][];
		source: string;
	};

	const LAB_PREVIEW_KEYS = ['lab.gem', 'lab.font'] as const;
	const TEMPLE_PREVIEW_KEYS = ['temple.panel', 'temple.remaining'] as const;
	const MERC_PREVIEW_KEYS = ['merc.panel'] as const;
	const LAB_GEM_PREVIEW_KEY = LAB_PREVIEW_KEYS[0];
	const LAB_FONT_PREVIEW_KEY = LAB_PREVIEW_KEYS[1];

	// --- Update ---
	let appVersion = $state('...');
	const updateController = createSettingsUpdateController({
		updateAvailable: store.updateAvailable,
		updateVersion: store.updateVersion
	});

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
	const screenGeometry = $derived(screenGeometryView(ssot.screen, geometryNow, ssot.placements));

	let recalibrating = $state(false);

	// Drops the one remembered screen scale, re-arms both modules that measure
	// it, and then MEASURES a base from a fresh grab (POE-278) — resolution,
	// display, origin, client rect, and a `uiScale` derived from the height. Rust
	// owns the whole sequence — see `ssot::geometry_recalibrate` — so this only
	// asks and then re-reads the snapshot, rather than waiting up to a poll
	// interval for the eager nudge.
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
		if (store.updateAvailable && updateController.status === 'idle') {
			updateController.offerBackgroundUpdate(store.updateVersion);
		}
	});

	let ocrRects = $state<OcrRectView[]>([]);
	let ocrRectsLoaded = $state(false);
	const ocrPreview = createOcrPreviewOwner();

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

	// The preview is focusless, so Escape belongs to the Settings window that
	// created it. Component cleanup owns the same destroy path.
	$effect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.key === 'Escape' && ocrPreview.hasPreview) void ocrPreview.destroy();
		};
		window.addEventListener('keydown', onKeyDown);
		return () => {
			window.removeEventListener('keydown', onKeyDown);
			void ocrPreview.destroy();
		};
	});

	function findOcrRect(key: string): OcrRectView | undefined {
		return ocrRects.find((row) => row.key === key);
	}

	function formatOcrRect(rect: OcrRectView['rect']): string {
		return rect ? `[${rect.join(', ')}]` : 'unlocated';
	}

	function ocrPreviewTitle(row: OcrRectView | undefined): string {
		return row?.rect ? `Preview ${row.label}` : 'Not located yet — open the panel once';
	}

	/** Where the rect came from in the shared placement contract. */
	function ocrSourceLabel(source: string | undefined): string {
		if (source === 'seed') return '(seed, scaled from reference)';
		if (source === 'remembered') return '(remembered anchor)';
		if (source === 'unlocated') return '(unlocated)';
		return '';
	}

	async function loadOcrRects(): Promise<void> {
		try {
			ocrRects = await invoke<OcrRectView[]>('get_ocr_rects');
			ocrRectsLoaded = true;
		} catch (e) {
			console.error('[settings] get_ocr_rects failed:', e);
		}
	}

	// Labels and current rectangles come from Rust's five-row projection. The
	// preview action reads a fresh snapshot before opening.
	$effect(() => {
		if (nav.view !== 'settings' || ocrRectsLoaded) return;
		void loadOcrRects();
	});

	// Reload when what the rows are projected from changes, so a Recalibrate or a
	// module measuring the screen replaces "unlocated" without a restart.
	const ocrKey = $derived(ocrRectsKey(ssot.screen, ssot.placements));
	$effect(() => {
		void ocrKey;
		if (nav.view !== 'settings' || !untrack(() => ocrRectsLoaded)) return;
		void loadOcrRects();
	});

	async function previewOcrRegion(key: string): Promise<void> {
		await ocrPreview.destroy();

		try {
			const rows = await invoke<OcrRectView[]>('get_ocr_rects');
			ocrRects = rows;
			const row = rows.find((item) => item.key === key);
			if (!row?.rect) {
				console.warn(`[settings] OCR preview '${key}' is unlocated`);
				return;
			}
			await ocrPreview.preview({
				key,
				label: row.label,
				rect: row.rect,
				rows: row.rows
			});
		} catch (e) {
			console.error('[settings] OCR preview failed:', e);
		}
	}

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

	/**
	 * The Overlay Positions groups, in display order (POE-226).
	 *
	 * Lab / Merc / Temple, with widget rows under each group. A group whose feature
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

	/** The module whose widgets are being arranged in its own overlay window
	 *  right now, or null. Set when Configure is pressed, cleared by the host's
	 *  `widget-config-end`. */
	let widgetConfiguring = $state<string | null>(null);

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

	$effect(() => {
		const modules = widgetModules;
		if (modules.length > 0) loadWidgetGeometries(modules);
	});

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

	// Load timer appearance on init
	$effect(() => {
		invoke<any>('get_timer_appearance').then((a) => {
			if (a) {
				timerBgOpacity = Math.round(a.bg_opacity * 100);
				timerTextStroke = a.text_stroke;
				savedBgOpacity = timerBgOpacity;
				savedTextStroke = a.text_stroke;
			}
		}).catch((e: any) => console.warn('[settings] load timer appearance failed:', e));
	});

	/** Whether a Configure widgets button may be pressed. */
	let configureAllowed = $derived(
		canStartConfigure({
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
				{#if updateController.status === 'checking'}
					<span class="setting-value muted">Checking...</span>
				{:else if updateController.status === 'available'}
					<span class="setting-value update-available">v{updateController.version} available</span>
					<Button variant="save" onclick={updateController.installUpdate}>Install & Restart</Button>
				{:else if updateController.status === 'downloading'}
					<span class="setting-value muted">Downloading... {updateController.progress > 0 ? `(${Math.round(updateController.progress / 1024)}KB)` : ''}</span>
				{:else if updateController.status === 'error'}
					<span class="setting-value update-error">{updateController.error}</span>
					<Button onclick={updateController.checkForUpdates}>Retry</Button>
				{:else}
					{#if updateController.error}
						<span class="setting-value muted">{updateController.error}</span>
					{/if}
					<Button onclick={updateController.checkForUpdates}>Check for Updates</Button>
				{/if}
			</div>
		</section>

		<!-- Overlays -->
		<section>
			<h2>Overlay Positions</h2>

			{#each overlayGroupRows as group (group.id)}
				<h3 class="group-heading">{group.heading}</h3>

				{#each group.widgets as row (row.spec.id)}
					{@const widget = row.spec}
					<div class="setting-row">
						<span class="setting-label">{widget.label}</span>
						<span class="widget-show">
							Show
							<Toggle
								checked={widgetPlacements.rows[widget.id]?.visible ?? true}
								label={widget.label}
								onchange={(next) => setWidgetVisible(widget, next)}
							/>
						</span>
						<!-- A game-anchored widget has no stored rectangle, so the
						     geometry column would print "Not set" forever and the
						     Configure button does not arrange it. The row is here
						     for the Show checkbox alone, which is the user's only
						     switch for that surface. -->
						<span class="setting-value mono">
							{row.placeable
								? widgetGeometryText(widgetPlacements.rows[widget.id], widget)
								: 'placed by the game'}
						</span>
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

		<!-- OCR Regions -->
		<section>
			<h2>OCR Regions</h2>

			{#if store.status?.ocr_language_warning}
				<div class="warning-banner">
					{store.status.ocr_language_warning}
					<!-- The site and the API share a host, so the walkthrough lives on the server this build talks to. -->
					<a class="warning-link" href="{store.status.server_url}/#ocr-language-pack" target="_blank">How to install it ↗</a>
				</div>
			{/if}

			<div class="setting-row">
				<span class="setting-label">{findOcrRect(LAB_GEM_PREVIEW_KEY)?.label ?? 'Gem Tooltip Region'}</span>
				<span class="setting-value mono">{formatOcrRect(findOcrRect(LAB_GEM_PREVIEW_KEY)?.rect ?? null)}</span>
				<span class="region-source">{ocrSourceLabel(findOcrRect(LAB_GEM_PREVIEW_KEY)?.source)}</span>
				<Button
					onclick={() => previewOcrRegion(LAB_GEM_PREVIEW_KEY)}
					disabled={!findOcrRect(LAB_GEM_PREVIEW_KEY)?.rect}
					title={ocrPreviewTitle(findOcrRect(LAB_GEM_PREVIEW_KEY))}
				>Preview</Button>
			</div>

			<div class="setting-row">
				<span class="setting-label">{findOcrRect(LAB_FONT_PREVIEW_KEY)?.label ?? 'Font Panel Region'}</span>
				<span class="setting-value mono">{formatOcrRect(findOcrRect(LAB_FONT_PREVIEW_KEY)?.rect ?? null)}</span>
				<span class="region-source">{ocrSourceLabel(findOcrRect(LAB_FONT_PREVIEW_KEY)?.source)}</span>
				<Button
					onclick={() => previewOcrRegion(LAB_FONT_PREVIEW_KEY)}
					disabled={!findOcrRect(LAB_FONT_PREVIEW_KEY)?.rect}
					title={ocrPreviewTitle(findOcrRect(LAB_FONT_PREVIEW_KEY))}
				>Preview</Button>
			</div>

			{#if hasFeature(TEMPLE_FEATURE)}
				{#each TEMPLE_PREVIEW_KEYS as key}
					{@const row = findOcrRect(key)}
					<div class="setting-row">
						<span class="setting-label">{row?.label ?? key}</span>
						<span class="setting-value mono">{formatOcrRect(row?.rect ?? null)}</span>
						<span class="region-source">{ocrSourceLabel(row?.source)}</span>
						<Button
							onclick={() => previewOcrRegion(key)}
							disabled={!row?.rect}
							title={ocrPreviewTitle(row)}
						>Preview</Button>
					</div>
				{/each}
			{/if}

			{#if hasFeature(MERC_FEATURE)}
				{#each MERC_PREVIEW_KEYS as key}
					{@const row = findOcrRect(key)}
					<div class="setting-row">
						<span class="setting-label">{row?.label ?? key}</span>
						<span class="setting-value mono">{formatOcrRect(row?.rect ?? null)}</span>
						<span class="region-source">{ocrSourceLabel(row?.source)}</span>
						<Button
							onclick={() => previewOcrRegion(key)}
							disabled={!row?.rect}
							title={ocrPreviewTitle(row)}
						>Preview</Button>
					</div>
				{/each}
			{/if}
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

			<div class="setting-row">
				<span class="setting-label">Lab gem</span>
				<span class="setting-value mono">{screenGeometry.placements.labGem}</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Lab font</span>
				<span class="setting-value mono">{screenGeometry.placements.labFont}</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Temple Entrance</span>
				<span class="setting-value mono">{screenGeometry.placements.templeEntrance}</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Merc panel</span>
				<span class="setting-value mono">{screenGeometry.placements.mercPanel}</span>
			</div>

			<p class="setting-note">
				Remembered once measured; verified on use; re-measured only when the screen or
				game-client geometry changes, verification fails, or you press Recalibrate —
				which measures a base from the screen itself, without needing either module
				running. Merc and Temple refine the scale from the game's art when they next run.
			</p>
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
</div>

<style>
	.settings-page {
		max-width: 960px;
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

	.warning-banner .warning-link {
		display: inline-block;
		margin-left: 6px;
		color: #fecaca;
		font-weight: 600;
		text-decoration: underline;
		white-space: nowrap;
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
