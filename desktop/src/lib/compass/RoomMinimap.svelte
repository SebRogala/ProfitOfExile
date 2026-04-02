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
		if (major) return '#ef4444'; // darkshrine/major = red (poelab style)
		return '#fbbf24'; // generic content = gold
	}
</script>

<div class="minimap">
	<img src={svgUrl} alt={roomName} class="room-svg" />

	{#if hasFallback}
		<div class="fallback-box">
			<div class="fallback-name">{roomName || 'Unknown Room'}</div>
			{#if targetDirection}
				<div class="fallback-dir">{targetDirection}</div>
			{/if}
		</div>
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
		background: transparent;
		border-radius: 4px;
	}

	.room-svg {
		width: 100%;
		height: 100%;
		object-fit: contain;
		display: block;
	}

	.fallback-box {
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		background: rgba(0, 0, 0, 0.5);
		border-radius: 4px;
	}

	.fallback-name {
		color: #d1d5db;
		font-size: 14px;
		font-family: system-ui, sans-serif;
		text-align: center;
		text-shadow: 0 1px 3px rgba(0, 0, 0, 0.8);
	}

	.fallback-dir {
		color: #10b981;
		font-size: 28px;
		font-family: system-ui, sans-serif;
		font-weight: 900;
		margin-top: 4px;
		text-shadow: 0 2px 4px rgba(0, 0, 0, 0.8);
	}

	.door-marker {
		position: absolute;
		display: flex;
		align-items: center;
		justify-content: center;
		pointer-events: none;
	}

	.door-label {
		background: rgba(0, 0, 0, 0.7);
		color: #9ca3af;
		font-size: 9px;
		font-family: system-ui, sans-serif;
		font-weight: 700;
		padding: 2px 5px;
		border-radius: 3px;
		white-space: nowrap;
		text-shadow: 0 1px 2px rgba(0, 0, 0, 0.8);
	}

	.door-marker.target .door-label {
		background: rgba(5, 150, 105, 0.9);
		color: #ecfdf5;
		box-shadow: 0 0 10px rgba(16, 185, 129, 0.5);
		text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
	}

	.content-marker {
		position: absolute;
		display: flex;
		align-items: center;
		justify-content: center;
		pointer-events: none;
	}

	.content-dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--content-color, #fbbf24);
		opacity: 0.7;
	}

	.room-badge {
		position: absolute;
		top: 4px;
		left: 4px;
		background: rgba(0, 0, 0, 0.75);
		border-radius: 3px;
		padding: 2px 6px;
		font-size: 9px;
		color: #d1d5db;
		font-family: system-ui, sans-serif;
		text-shadow: 0 1px 2px rgba(0, 0, 0, 0.8);
	}
</style>
