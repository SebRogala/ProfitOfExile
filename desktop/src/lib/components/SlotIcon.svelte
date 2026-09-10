<script lang="ts">
	/**
	 * One inline silhouette per architect mod-family slot. There is no network
	 * request and no raster asset: the overlay must draw complete on its first
	 * frame with no request in flight. The monochrome `currentColor` lets the
	 * caller's colour apply, and the keyed record makes a new slot a typecheck
	 * failure.
	 */
	import type { ItemSlotId } from '$lib/temple/slice';

	const SLOT_NAMES: Record<ItemSlotId, string> = {
		ring: 'ring',
		amulet: 'amulet',
		body_armour: 'body armour',
		gloves: 'gloves',
		boots: 'boots',
		weapon: 'weapon'
	};

	const SLOT_PATHS: Record<ItemSlotId, string> = {
		ring: 'M12 2.5a9.5 9.5 0 1 0 0 19 9.5 9.5 0 0 0 0-19Zm0 4a5.5 5.5 0 1 1 0 11 5.5 5.5 0 0 1 0-11Z',
		amulet: 'M7 3.5a5 5 0 0 1 10 0h-1.8a3.2 3.2 0 0 0-6.4 0H7Zm1.2 4.2h7.6l2.1 3.1-5.9 9.7-5.9-9.7 2.1-3.1Zm1.1 1.8-1 1.5 3.7 6.1 3.7-6.1-1-1.5H9.3Z',
		body_armour: 'm8 3 4 2 4-2 4 3-2 4-1-1v12H7V9L6 10 4 6l4-3Zm1 3.2L8 7.5v10.7h3V7.6L9 6.2Zm6 0-2 1.4v11h3V7.5l-1-1.3Z',
		gloves: 'M7 20c-1.1-.8-1.8-2-1.8-3.3V9.5a1.2 1.2 0 0 1 2.4 0v3.1h.8V5.2a1.1 1.1 0 0 1 2.2 0v7.4h.8V4.2a1.1 1.1 0 0 1 2.2 0v8.4h.8V5.8a1.1 1.1 0 0 1 2.2 0v8.7c0 3.3-1.9 5.5-5.2 5.5H9.3L7 20Z',
		boots: 'M7 3h6v9.5l2.5 2.2H20v4.8H4v-3.2l3-2.1V3Zm2 2v10.2l-3 2.1v.2h8.9l-3.2-2.8c-.5-.4-.7-1-.7-1.6V5H9Z',
		weapon: 'm15.5 3 5.5 5.5-1.4 1.4-1.8-1.8-6.1 6.1 1.8 1.8-1.4 1.4-5.5-5.5 1.4-1.4 1.8 1.8 6.1-6.1-1.8-1.8L15.5 3ZM5.6 15.9l2.5 2.5-3.2 2.1-1.4-1.4 2.1-3.2Z'
	};

	let { slot, size = 18 }: { slot: ItemSlotId; size?: number } = $props();
</script>

<svg
	class="slot-icon"
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill="currentColor"
	role="img"
	aria-label={SLOT_NAMES[slot]}
>
	<path fill-rule="evenodd" d={SLOT_PATHS[slot]} />
</svg>

<style>
	.slot-icon {
		display: block;
		flex: 0 0 auto;
		color: var(--color-lab-text-secondary);
	}
</style>
