<script lang="ts">
	import type { DoorExitLocation, ContentLocation } from './room-presets';
	import { getRoomSvgUrl, getDisabledSvgUrl } from './svg-loader';

	let {
		areaCode = '',
		doors = [] as DoorExitLocation[],
		contents = [] as ContentLocation[],
		targetDirection = null as string | null,
		roomName = '',
	} = $props();

	const svgUrl = $derived(getRoomSvgUrl(areaCode) ?? getDisabledSvgUrl());
	const hasFallback = $derived(!areaCode || doors.length === 0);

	function contentColor(direction: string, major: boolean): string {
		const dir = direction.toLowerCase();
		if (dir.includes('shrine') || major) return '#a855f7';
		return '#fbbf24';
	}
</script>

<div class="minimap">
	<img src={svgUrl} alt={roomName} class="room-svg" />

	{#if hasFallback}
		<div class="fallback-label">{roomName || 'Unknown Room'}</div>
	{:else}
		<!-- Door exit markers -->
		{#each doors as door}
			<div
				class="door-marker"
				class:target={door.direction === targetDirection}
				style="left: {door.tileRect.x * 100}%; top: {door.tileRect.y * 100}%; width: {door.tileRect.width * 100}%; height: {door.tileRect.height * 100}%;"
			>
				<span class="door-label">{door.direction}</span>
			</div>
		{/each}

		<!-- Content location markers -->
		{#each contents as content}
			<div
				class="content-marker"
				style="left: {content.tileRect.x * 100}%; top: {content.tileRect.y * 100}%; width: {content.tileRect.width * 100}%; height: {content.tileRect.height * 100}%; --content-color: {contentColor(content.direction, content.major)};"
			>
				<span class="content-dot"></span>
			</div>
		{/each}

		<!-- Room name badge -->
		<div class="room-badge">{roomName}</div>
	{/if}
</div>

<style>
	.minimap {
		position: relative;
		width: 100%;
		height: 100%;
		overflow: hidden;
		background: #0a0a12;
		border-radius: 4px;
	}

	.room-svg {
		width: 100%;
		height: 100%;
		object-fit: contain;
		display: block;
	}

	.fallback-label {
		position: absolute;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		color: #6b7280;
		font-size: 12px;
		font-family: system-ui, sans-serif;
		text-align: center;
	}

	.door-marker {
		position: absolute;
		display: flex;
		align-items: center;
		justify-content: center;
		pointer-events: none;
	}

	.door-label {
		background: rgba(55, 65, 81, 0.85);
		color: #9ca3af;
		font-size: 8px;
		font-family: system-ui, sans-serif;
		font-weight: 600;
		padding: 1px 4px;
		border-radius: 3px;
		border: 1px solid rgba(75, 85, 99, 0.6);
		white-space: nowrap;
	}

	.door-marker.target .door-label {
		background: rgba(5, 150, 105, 0.9);
		color: #ecfdf5;
		border-color: #10b981;
		box-shadow: 0 0 8px rgba(16, 185, 129, 0.4);
	}

	.content-marker {
		position: absolute;
		display: flex;
		align-items: center;
		justify-content: center;
		pointer-events: none;
	}

	.content-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--content-color, #fbbf24);
		box-shadow: 0 0 4px var(--content-color, #fbbf24);
	}

	.room-badge {
		position: absolute;
		top: 4px;
		left: 4px;
		background: rgba(13, 13, 21, 0.9);
		border: 1px solid rgba(42, 42, 58, 0.6);
		border-radius: 3px;
		padding: 1px 5px;
		font-size: 8px;
		color: #6b7280;
		font-family: system-ui, sans-serif;
	}
</style>
