<script lang="ts">
	import '../../app.css';
	import { invoke } from '@tauri-apps/api/core';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import TopBar from '$lib/components/TopBar.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import { page } from '$app/stores';
	import { store, initStatusStore } from '$lib/stores/status.svelte';
	import { initFocusListener, destroyOverlay, isOverlayActive, readOverlayRegion } from '$lib/overlay/manager';

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

			width: 900,
			height: 400,
			x: Math.round(x / dpr),
			y: Math.round(y / dpr),
		});

		win.once('tauri://created', async () => {
			await invoke('set_overlay_clickthrough', { label: 'comparator', interactiveWidth: 48 }).catch(() => {});
			comparatorWin = win;
			comparatorActive = true;
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
			// Position already saved by the overlay's own interval — just destroy
			await destroyComparatorWindow();
			comparatorActive = false;
		} else {
			const settings = await invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_comparator_overlay_settings').catch(() => null);
			console.log('[overlay] toggle ON — loaded settings:', settings);
			await createComparatorOverlay(
				settings?.x ?? 100,
				settings?.y ?? 100,
			);
		}
	}

	// Ctrl+Shift+F12 toggles devtools in production builds
	$effect(() => {
		function handleKeydown(e: KeyboardEvent) {
			if (e.ctrlKey && e.shiftKey && e.key === 'F12') {
				e.preventDefault();
				const win = getCurrentWebviewWindow();
				win.isDevtoolsOpen().then(open => {
					if (open) win.closeDevtools();
					else win.openDevtools();
				}).catch(() => {});
			}
		}
		window.addEventListener('keydown', handleKeydown);
		return () => window.removeEventListener('keydown', handleKeydown);
	});

	// Initialize event listeners — runs on module load (client-side only due to ssr:false)
	// No cleanup needed — desktop app layout never unmounts.
	initStatusStore().catch(e => console.error('[layout] initStatusStore failed:', e));
	initFocusListener().catch(e => console.error('[layout] initFocusListener failed:', e));

	// Auto-restore comparator overlay if it was enabled in previous session
	invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_comparator_overlay_settings')
		.then((settings) => {
			if (settings?.enabled) {
				createComparatorOverlay(settings.x, settings.y);
			}
		})
		.catch(() => {});
</script>

<div class="app-shell">
	<TopBar status={store.status} />
	<div class="app-body">
		<Sidebar open={sidebarOpen} currentPath={$page.url.pathname} onToggle={toggleSidebar}
			comparatorActive={comparatorActive} onToggleComparator={toggleComparatorOverlay} />
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
