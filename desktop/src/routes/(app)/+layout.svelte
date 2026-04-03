<script lang="ts">
	import '../../app.css';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import TopBar from '$lib/components/TopBar.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import { store, initStatusStore } from '$lib/stores/status.svelte';
	import { nav } from '$lib/stores/navigation.svelte';
	import { destroyOverlay, isOverlayActive, readOverlayRegion } from '$lib/overlay/manager';
	import LabPage from '$lib/pages/LabPage.svelte';
	import SettingsPage from '$lib/pages/SettingsPage.svelte';
	import DevPage from '$lib/pages/DevPage.svelte';


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
		const { PhysicalPosition } = await import('@tauri-apps/api/dpi');

		await destroyCompassWindow();

		const win = new WebviewWindow('compass', {
			url: '/overlay/compass',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: true,
			shadow: false,
			skipTaskbar: true,
			width: w,
			height: h,
		});

		win.once('tauri://created', async () => {
			await win.setPosition(new PhysicalPosition(physX, physY));
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
		const { PhysicalPosition } = await import('@tauri-apps/api/dpi');

		await destroyPathstripWindow();

		const win = new WebviewWindow('pathstrip', {
			url: '/overlay/pathstrip',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: true,
			shadow: false,
			skipTaskbar: true,
			width: w,
			height: h,
		});

		win.once('tauri://created', async () => {
			await win.setPosition(new PhysicalPosition(physX, physY));
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

	// Ctrl+Shift+F12 toggles debug mode (devtools + force-show overlays)
	let debugMode = $state(false);

	$effect(() => {
		function handleKeydown(e: KeyboardEvent) {
			if (e.ctrlKey && e.shiftKey && e.key === 'F12') {
				e.preventDefault();
				debugMode = !debugMode;
				const win = getCurrentWebviewWindow();
				if (debugMode) {
					(win as any).openDevtools().catch((e: any) => console.warn('[debug] openDevtools failed:', e));
					// Force-show overlays regardless of game focus
					invoke('force_show_overlays').catch((e: any) => console.warn('[debug] force_show_overlays failed:', e));
					console.log('[debug] Debug mode ON — overlays force-shown');
				} else {
					(win as any).closeDevtools().catch((e: any) => console.warn('[debug] closeDevtools failed:', e));
					console.log('[debug] Debug mode OFF');
				}
			}
		}
		window.addEventListener('keydown', handleKeydown);
		return () => window.removeEventListener('keydown', handleKeydown);
	});

	// Initialize event listeners — runs on module load (client-side only due to ssr:false)
	// No cleanup needed — desktop app layout never unmounts.
	initStatusStore().catch(e => console.error('[layout] initStatusStore failed:', e));

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

	// Auto-restore compass overlay if it was enabled in previous session
	invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_compass_overlay_settings')
		.then((settings) => {
			if (settings?.enabled) {
				createCompassOverlay(settings.x, settings.y, settings.width ?? 300, settings.height ?? 280);
			}
		})
		.catch(e => console.warn('[overlay] compass settings operation failed:', e));

	// Auto-restore pathstrip overlay if it was enabled in previous session
	invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_pathstrip_overlay_settings')
		.then((settings) => {
			if (settings?.enabled) {
				pathstripActive = true;
				// Check if layout data exists before creating the overlay
				checkPathstripData().then(hasData => {
					if (hasData) createPathstripOverlay(settings!.x, settings!.y, settings!.width ?? 450, settings!.height ?? 180);
				});
			}
		})
		.catch(e => console.warn('[overlay] pathstrip settings operation failed:', e));

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

	// Auto-show overlays on lab entry (PlazaEntered)
	listen('lab-nav', async (event: any) => {
		if (event.payload?.type === 'PlazaEntered') {
			const compassSettings = await invoke<any>('get_compass_overlay_settings').catch(() => null);
			if (compassSettings?.enabled && !compassWin) {
				await createCompassOverlay(compassSettings.x, compassSettings.y, compassSettings.width ?? 300, compassSettings.height ?? 280);
			}
			const pathstripSettings = await invoke<any>('get_pathstrip_overlay_settings').catch(() => null);
			if (pathstripSettings?.enabled && !pathstripWin) {
				await createPathstripOverlay(pathstripSettings.x, pathstripSettings.y, pathstripSettings.width ?? 450, pathstripSettings.height ?? 180);
			}
		}
	});
</script>

<div class="app-shell">
	<TopBar status={store.status} />
	<div class="app-body">
		<Sidebar open={sidebarOpen} currentPath={nav.view === 'dev' ? '/dev' : nav.view === 'settings' ? '/settings' : '/'} onToggle={toggleSidebar}
			comparatorActive={comparatorActive} gameFocused={store.status?.game_focused ?? false} onToggleComparator={toggleComparatorOverlay}
			compassActive={compassActive} onToggleCompass={toggleCompassOverlay}
			pathstripActive={pathstripActive} pathstripHasData={pathstripHasData} onTogglePathstrip={togglePathstripOverlay} />
		<main class="content">
			<div class:view-hidden={nav.view !== 'lab'}>
				<LabPage />
			</div>
			<div class:view-hidden={nav.view !== 'settings'}>
				<SettingsPage />
			</div>
			{#if import.meta.env.DEV}
				<div class:view-hidden={nav.view !== 'dev'}>
					<DevPage />
				</div>
			{/if}
		</main>
	</div>
</div>

<style>
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
</style>
