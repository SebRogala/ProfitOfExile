<script lang="ts">
	/**
	 * The global read prompt window (POE-388): one placeable widget,
	 * `prompt.read`, drawn while Client.txt has armed a mechanic. Display only —
	 * the read itself is the global read key (POE-387).
	 */
	import ReadPrompt from '$lib/read-prompt/ReadPrompt.svelte';
	import { readPromptText } from '$lib/read-prompt/view';
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte';
	import { PROMPT_WINDOW_LABEL } from '$lib/overlay/manager';
	import { ssot } from '$lib/stores/ssot.svelte';

	const text = $derived(readPromptText(ssot));
</script>

<WidgetHost module={PROMPT_WINDOW_LABEL}>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'prompt.read' && text}
			<ReadPrompt {text} />
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
