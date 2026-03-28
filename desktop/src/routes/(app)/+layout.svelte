<script lang="ts">
	import '../../app.css';
	import TopBar from '$lib/components/TopBar.svelte';
	import { invoke } from '@tauri-apps/api/core';

	let { children } = $props();

	let status = $state<any>({ state: 'Loading...', server_url: 'https://poe.softsolution.pro', detected_gems: [] });
	let pairCode = $state('...');
	let sidebarOpen = $state(false);

	setInterval(() => {
		invoke('get_status').then((s) => { status = s; }).catch(() => {});
		invoke('get_pair_code').then((c) => { pairCode = c as string; }).catch(() => {});
	}, 1000);
</script>

<div class="app-shell">
	<TopBar {status} {pairCode} onToggleSidebar={() => sidebarOpen = !sidebarOpen} />
	<div class="app-body">
		{@render children()}
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
</style>
