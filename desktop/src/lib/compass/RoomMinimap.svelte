<script lang="ts">
	import { minimapLootColor, directionLabel, type DoorExitLocation, type ContentLocation } from './room-presets';
	import { getRoomSvgUrl, getDisabledSvgUrl } from './svg-loader';

	let {
		areaCode = '',
		doors = [] as DoorExitLocation[],
		contents = [] as ContentLocation[],
		targetDirection = null as string | null,
		entryDirection = null as string | null,
		roomName = '',
	} = $props();

	const svgUrl = $derived(getRoomSvgUrl(areaCode) ?? getDisabledSvgUrl());
	const hasFallback = $derived(!areaCode || doors.length === 0);

</script>

<div class="minimap">
	<!-- Markers are fractions of the room image, so they share its 1.3:1 box. -->
	<div class="stage">
		<img src={svgUrl} alt={roomName} class="room-svg" />

		{#if !hasFallback}
			<!-- Door exit markers -->
			{#each doors as door}
				<div
					class="door-marker"
					class:target={door.direction === targetDirection}
					class:entry={door.direction === entryDirection && door.direction !== targetDirection}
					style="left: {door.tileRect.x * 100}%; top: {door.tileRect.y * 100}%; width: {door.tileRect.width * 100}%; height: {door.tileRect.height * 100}%;"
				>
					<span class="door-label">{door.direction}</span>
				</div>
			{/each}

			<!-- Content location markers -->
			{#each contents as content}
				<div
					class="content-marker"
					style="left: {content.tileRect.x * 100}%; top: {content.tileRect.y * 100}%; width: {content.tileRect.width * 100}%; height: {content.tileRect.height * 100}%; --content-color: {minimapLootColor(content.target)};"
				>
					<span class="content-dot"></span>
				</div>
			{/each}
		{/if}
	</div>

	{#if hasFallback}
		<div class="fallback-box">
			<div class="fallback-name">{roomName || 'Unknown Room'}</div>
			{#if targetDirection}
				<div class="fallback-dir">{directionLabel(targetDirection)}</div>
			{/if}
		</div>
	{:else if roomName}
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
		container-type: size;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	/* The largest 1.3:1 box the container holds, like upstream's ratio-locked item. */
	.stage {
		position: relative;
		width: min(100cqw, 100cqh * 1.3);
		height: min(100cqh, 100cqw / 1.3);
	}

	.room-svg {
		width: 100%;
		height: 100%;
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

	.door-marker.entry .door-label {
		background: rgba(30, 41, 59, 0.9);
		color: #cbd5e1;
		border: 1px dashed #64748b;
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
