<script lang="ts">
	let { children } = $props();
</script>

{@render children()}

<style>
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
