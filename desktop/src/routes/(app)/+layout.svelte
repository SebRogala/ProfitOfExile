<script lang="ts">
	import '../../app.css';
	import TopBar from '$lib/components/TopBar.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import { page } from '$app/stores';
	import { store, initStatusStore } from '$lib/stores/status.svelte';

	let { children } = $props();
	let sidebarOpen = $state(true);

	// Initialize event listeners — runs on module load (client-side only due to ssr:false)
	// No cleanup needed — desktop app layout never unmounts.
	initStatusStore().catch(e => console.error('[layout] initStatusStore failed:', e));
</script>

<div class="app-shell">
	<TopBar status={store.status} />
	<div class="app-body">
		<Sidebar open={sidebarOpen} currentPath={$page.url.pathname} onToggle={() => sidebarOpen = !sidebarOpen} />
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
