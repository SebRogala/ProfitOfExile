{@render children()}

<style>
	:global(html), :global(body) {
		margin: 0;
		padding: 0;
		background: transparent !important;
		overflow: hidden;
	}
</style>

<script lang="ts">
	let { children } = $props();
</script>
