<script lang="ts">
	let { children } = $props();
</script>

<div class="overlay-root">
	{@render children()}
</div>

<style>
	.overlay-root {
		display: contents;
	}

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
