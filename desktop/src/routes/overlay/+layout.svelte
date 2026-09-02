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

	/* `box-sizing` is deliberately NOT reset here. The widget host needs
	   `border-box` — its boxes are sized in the same pixels their placement is
	   persisted in — but it is the ONE surface that does, and it declares it for
	   itself (`lib/overlay/widgets/WidgetHost.svelte`). This layout is shared by
	   every overlay window, and the five that predate the widget engine were laid
	   out under the default `content-box`: resetting globally silently reflowed
	   them, taking the comparator's `.table` (`width: 560px` + 10 px of padding +
	   a 1 px border) from 582 px to 560. A reset that changes windows the change
	   was not about does not belong in the shared layout. */
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
