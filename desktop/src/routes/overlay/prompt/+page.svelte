<script lang="ts">
	/**
	 * The global read prompt window (POE-388): one placeable widget,
	 * `prompt.read`, showing an armed mechanic, a completed no-match or the
	 * Divine Font gem slots a read left uncertain. Display only —
	 * the read itself is the global read key (POE-387).
	 */
	import ReadPrompt from '$lib/read-prompt/ReadPrompt.svelte';
	import { createReadPromptPresentation } from '$lib/read-prompt/presentation.svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte';
	import { PROMPT_WINDOW_LABEL } from '$lib/overlay/manager';
	import { ssot } from '$lib/stores/ssot.svelte';

	const presentation = createReadPromptPresentation({
		source: () => ssot, listen, invoke,
		frame: () => new Promise((resolve) => requestAnimationFrame(() => resolve()))
	});
	const notice = $derived(presentation.notice);
</script>

<WidgetHost module={PROMPT_WINDOW_LABEL}>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'prompt.read' && notice}
			<ReadPrompt {notice} />
		{:else if configMode}
			<p class="placeholder">{spec.label}</p>
		{/if}
	{/snippet}
</WidgetHost>

<style>
	.placeholder {
		margin: 0;
		padding: 6px 10px;
		font-size: 13px;
		color: var(--color-lab-text-muted);
	}
</style>
