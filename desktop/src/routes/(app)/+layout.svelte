<script lang="ts">
	import '../../app.css';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import TopBar from '$lib/components/TopBar.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import { page } from '$app/stores';
	import { store, initStatusStore } from '$lib/stores/status.svelte';
	import { destroyOverlay, isOverlayActive, readOverlayRegion } from '$lib/overlay/manager';

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

	async function createComparatorOverlay(x: number, y: number) {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const dpr = window.devicePixelRatio || 1;

		await destroyComparatorWindow();

		// Window is transparent — oversized so content can grow dynamically
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
			x: Math.round(x / dpr),
			y: Math.round(y / dpr),
		});

		win.once('tauri://created', async () => {
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

	// Destroy the comparator window — same loop that works in createComparatorOverlay
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

	// Ctrl+Shift+F12 toggles debug mode (devtools + force-show overlays)
	let debugMode = $state(false);

	$effect(() => {
		function handleKeydown(e: KeyboardEvent) {
			if (e.ctrlKey && e.shiftKey && e.key === 'F12') {
				e.preventDefault();
				debugMode = !debugMode;
				const win = getCurrentWebviewWindow();
				if (debugMode) {
					win.openDevtools().catch(e => console.warn('[debug] openDevtools failed:', e));
					// Force-show overlays regardless of game focus
					invoke('force_show_overlays').catch(e => console.warn('[debug] force_show_overlays failed:', e));
					console.log('[debug] Debug mode ON — overlays force-shown');
				} else {
					win.closeDevtools().catch(e => console.warn('[debug] closeDevtools failed:', e));
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

	// Toggle comparator off/on when settings page closes a config overlay.
	// Fixes Win32 mouse capture stuck after overlay destroy.
	// Only active while a config overlay is open (overlay-config-start/end events).
	let configOverlayCleanup: (() => void) | null = null;
	listen('overlay-config-start', async () => {
		if (configOverlayCleanup) return; // already listening
		const unlisten = await listen('overlay-toggle-reset', async () => {
			if (!comparatorActive) return;
			await destroyComparatorWindow();
			comparatorActive = false;
			await new Promise(r => setTimeout(r, 100));
			const settings = await invoke<any>('get_comparator_overlay_settings').catch(() => null);
			await createComparatorOverlay(settings?.x ?? 100, settings?.y ?? 100);
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
		.catch(e => console.warn('[overlay] settings operation failed:', e));
</script>

<div class="app-shell">
	<TopBar status={store.status} />
	<div class="app-body">
		<Sidebar open={sidebarOpen} currentPath={$page.url.pathname} onToggle={toggleSidebar}
			comparatorActive={comparatorActive} gameFocused={store.status?.game_focused ?? false} onToggleComparator={toggleComparatorOverlay} />
		<main class="content">
			{#if children}
				{@render children()}
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
</style>
