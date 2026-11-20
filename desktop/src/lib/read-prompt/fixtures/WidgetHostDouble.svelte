<script lang="ts">
	/**
	 * Test double for `WidgetHost.svelte`: the real host needs a live Tauri
	 * webview window, so this replaces only placement and renders the page's
	 * own content snippet for the production `prompt.read` spec in normal mode.
	 */
	import type { Snippet } from 'svelte';
	import { widgetsFor, type WidgetSpec } from '$lib/overlay/widgets/widget-registry';
	import { PROMPT_WINDOW_LABEL } from '$lib/overlay/manager';

	let { content }: { module: string; content: Snippet<[WidgetSpec, boolean]> } = $props();

	const spec = widgetsFor(PROMPT_WINDOW_LABEL).find((s) => s.id === 'prompt.read')!;
</script>

{@render content(spec, false)}
