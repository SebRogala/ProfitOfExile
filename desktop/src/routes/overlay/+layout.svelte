<script lang="ts">
	// The palette, and ONLY the palette. `app.css` would drag in an opaque
	// `body` background, which is the one thing an overlay window must not have;
	// `tokens.css` is the shared `:root` block both layouts declare from, so a
	// component drawn in both windows (`temple/TempleLattice.svelte`) resolves
	// the same colours in each instead of rendering with unset custom properties
	// out here. `src/lib/overlay/overlay-tokens.test.ts` fails if this import
	// goes away.
	import '../../tokens.css';
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

	/* `box-sizing` is part of the reset rather than a per-panel declaration
	   because an overlay widget's box is sized in the SAME pixels its placement
	   is persisted in: a `.panel` given the registry's 200 px under the default
	   `content-box` renders 220 wide once its padding is added, and the widget
	   would not be the size the user placed. */
	:global(*) {
		box-sizing: border-box;
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
