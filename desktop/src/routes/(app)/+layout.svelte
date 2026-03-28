<script lang="ts">
	import '../../app.css';
	let { children } = $props();
</script>

<div class="app-shell">
	{@render children()}
</div>

<style>
	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
	}
</style>
