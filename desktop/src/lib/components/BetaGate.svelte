<script lang="ts">
	import type { Snippet } from 'svelte';
	import { BETA_FEATURE, hasFeature } from '$lib/stores/entitlements.svelte';

	let { children }: { children: Snippet } = $props();
</script>

{#if hasFeature(BETA_FEATURE)}
	{@render children()}
{/if}
