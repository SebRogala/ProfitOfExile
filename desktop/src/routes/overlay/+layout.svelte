<script lang="ts">
	import { startSsotStore } from '$lib/stores/ssot.svelte';

	let { children } = $props();

	// Overlay windows deliver SSOT by polling get_ssot (WebView2 cross-window
	// events are unreliable). onMount is unreliable in overlay windows, so start
	// the poll from an $effect and return its cleanup.
	$effect(() => startSsotStore());
</script>

<div class="overlay-root">
	{@render children()}
</div>

<style>
	.overlay-root {
		display: contents;
	}

	:global(*) {
		margin: 0;
		padding: 0;
	}
	:global(html) {
		background: transparent !important;
	}
	:global(body) {
		background: transparent !important;
		overflow: hidden;
	}
</style>
