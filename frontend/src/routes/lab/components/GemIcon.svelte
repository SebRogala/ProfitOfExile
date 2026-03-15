<script lang="ts">
	import { getGemIconUrl } from '$lib/gem-icons';

	let { name, size = 24 }: { name: string; size?: number } = $props();

	let src = $derived(getGemIconUrl(name));
	let errored = $state(false);
</script>

{#if !errored}
	<img
		{src}
		alt={name}
		width={size}
		height={size}
		class="gem-icon"
		onerror={() => { errored = true; }}
	/>
{:else}
	<span class="gem-icon-fallback" style:width="{size}px" style:height="{size}px">?</span>
{/if}

<style>
	.gem-icon {
		display: inline-block;
		vertical-align: middle;
		object-fit: contain;
		border-radius: 2px;
	}
	.gem-icon-fallback {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		vertical-align: middle;
		background: rgba(156, 163, 175, 0.15);
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
		border-radius: 2px;
	}
</style>
