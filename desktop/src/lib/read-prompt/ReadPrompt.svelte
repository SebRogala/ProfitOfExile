<script lang="ts">
	import OverlayNotice, { type OverlayNoticeProps } from '$lib/overlay/OverlayNotice.svelte';

	let { notice }: { notice: OverlayNoticeProps } = $props();
</script>

<OverlayNotice {...notice} />
