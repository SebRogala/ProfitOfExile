<script lang="ts">
	import Tooltip from '$lib/components/Tooltip.svelte';
	let { text }: { text: string } = $props();
</script>

<Tooltip {text} position="below">
	<span class="icon-circle">i</span>
</Tooltip>

<style>
	.icon-circle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		border: 1px solid currentColor;
		border-radius: 50%;
		font-size: 0.6875rem;
		font-weight: 700;
		font-style: italic;
		line-height: 1;
		opacity: 0.6;
		transition: opacity 0.15s ease;
		transform: translateY(-2px);
		color: var(--color-lab-text-secondary);
		margin-left: 6px;
		cursor: help;
	}
	.icon-circle:hover {
		color: var(--color-lab-text);
		opacity: 1;
	}
</style>
