<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	onMount(() => goto('/lab', { replaceState: true }));
</script>
