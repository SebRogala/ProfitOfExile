<script lang="ts">
	import { isDoorDirection, lootColor, type ContentLocation } from './room-presets';

	let {
		directions = [] as string[],
		targetDirection = null as string | null,
		roomName = '',
		contents = [] as ContentLocation[],
	} = $props();

	const ANGLE_MAP: Record<string, number> = {
		N: 270, NE: 315, E: 0, SE: 45, S: 90, SW: 135, W: 180, NW: 225,
	};

	function markerPos(direction: string): { cx: number; cy: number } {
		const angle = ANGLE_MAP[direction] ?? 0;
		const rad = (angle * Math.PI) / 180;
		const r = 42;
		return { cx: 60 + r * Math.cos(rad), cy: 60 + r * Math.sin(rad) };
	}

	function contentPos(direction: string): { cx: number; cy: number } {
		const rad = ((ANGLE_MAP[direction] ?? 0) * Math.PI) / 180;
		const r = 26;
		return { cx: 60 + r * Math.cos(rad), cy: 60 + r * Math.sin(rad) };
	}

	function arrowEnd(direction: string): { x: number; y: number } {
		const angle = ANGLE_MAP[direction] ?? 0;
		const rad = (angle * Math.PI) / 180;
		const r = 30;
		return { x: 60 + r * Math.cos(rad), y: 60 + r * Math.sin(rad) };
	}
</script>

<div class="compass-container">
	<svg viewBox="0 0 120 120" width="120" height="120" class="compass-svg">
		<defs>
			<radialGradient id="compass-bg">
				<stop offset="0%" stop-color="#1a1a2e" />
				<stop offset="100%" stop-color="#0f0f17" />
			</radialGradient>
			<filter id="target-glow">
				<feGaussianBlur stdDeviation="2" result="b" />
				<feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge>
			</filter>
		</defs>

		<!-- Background -->
		<circle cx="60" cy="60" r="56" fill="url(#compass-bg)" stroke="#2a2a3a" stroke-width="0.8" />
		<circle cx="60" cy="60" r="40" fill="none" stroke="#1a1a28" stroke-width="0.5" />

		<!-- Arrow to target -->
		<!-- A secret passage ('C') is not a door: no arrow. -->
		{#if targetDirection && isDoorDirection(targetDirection)}
			{@const end = arrowEnd(targetDirection)}
			<line x1="60" y1="60" x2={end.x} y2={end.y} stroke="#10b981" stroke-width="1.5" opacity="0.5" stroke-linecap="round" />
		{/if}

		<!-- Exit markers -->
		{#each directions as dir}
			{@const pos = markerPos(dir)}
			{#if dir === targetDirection}
				<g filter="url(#target-glow)">
					<circle cx={pos.cx} cy={pos.cy} r="10" fill="#059669" stroke="#10b981" stroke-width="1" />
					<text x={pos.cx} y={pos.cy + 3} fill="#ecfdf5" font-size="7" text-anchor="middle" font-family="system-ui" font-weight="700">{dir}</text>
				</g>
			{:else}
				<circle cx={pos.cx} cy={pos.cy} r="8" fill="#1f2937" stroke="#374151" stroke-width="0.8" />
				<text x={pos.cx} y={pos.cy + 3} fill="#6b7280" font-size="7" text-anchor="middle" font-family="system-ui" font-weight="600">{dir}</text>
			{/if}
		{/each}

		<!-- Content positions, inside the door ring (upstream DirectionDisplay.qml:31-45: radius 30 of 48) -->
		{#each contents as content}
			{@const pos = contentPos(content.direction)}
			<rect
				class="content-marker"
				x={pos.cx - 3}
				y={pos.cy - 3}
				width="6"
				height="6"
				transform="rotate(45 {pos.cx} {pos.cy})"
				fill={lootColor(content.target)}
				stroke="black"
				stroke-width="0.8"
			/>
		{/each}
	</svg>
	{#if roomName}
		<div class="room-label">{roomName}</div>
	{/if}
</div>

<style>
	.compass-container {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
	}

	.compass-svg {
		display: block;
	}

	.room-label {
		color: #6b7280;
		font-size: 9px;
		font-family: system-ui, sans-serif;
		text-align: center;
	}
</style>
