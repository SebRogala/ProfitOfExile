<script lang="ts">
	import '../../app.css';
	import TopBar from '$lib/components/TopBar.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import { page } from '$app/stores';
	import { appStatus, initStatusStore } from '$lib/stores/status.svelte';
	import { onMount } from 'svelte';

	let { children } = $props();
	let sidebarOpen = $state(true);

	onMount(() => {
		const cleanup = initStatusStore();
		return () => { cleanup.then(fn => fn()); };
	});
</script>

<div class="app-shell">
	<TopBar status={appStatus} pairCode={appStatus?.pair_code ?? '....'} onToggleSidebar={() => sidebarOpen = !sidebarOpen} />
	<div class="app-body">
		<Sidebar open={sidebarOpen} currentPath={$page.url.pathname} />
		<main class="content">
			{@render children()}
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
