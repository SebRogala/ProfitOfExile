<script lang="ts">
	let {
		targetDirection = null as string | null,
		contents = [] as string[],
		timerText = '00:00',
	} = $props();

	const ANGLE_MAP: Record<string, number> = {
		N: 0, NE: 45, E: 90, SE: 135, S: 180, SW: 225, W: 270, NW: 315,
	};

	const arrowRotation = $derived(ANGLE_MAP[targetDirection ?? ''] ?? 0);
</script>

<div class="bar">
	{#if targetDirection}
		<svg viewBox="0 0 24 24" width="20" height="20" class="arrow" style="transform: rotate({arrowRotation}deg);">
			<polygon points="12,2 20,18 12,14 4,18" fill="#10b981" opacity="0.9" />
		</svg>
		<span class="direction">{targetDirection}</span>
	{:else}
		<span class="no-target">---</span>
	{/if}

	{#if contents.length > 0}
		<span class="divider"></span>
		{#each contents as item}
			<span class="content-badge">{item}</span>
		{/each}
	{/if}

	<span class="divider"></span>
	<span class="timer">{timerText}</span>
</div>

<style>
	.bar {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		background: rgba(13, 13, 21, 0.92);
		border: 1px solid rgba(42, 42, 58, 0.6);
		border-radius: 6px;
		padding: 6px 12px;
	}

	.arrow {
		flex-shrink: 0;
	}

	.direction {
		color: #10b981;
		font-size: 12px;
		font-family: system-ui, sans-serif;
		font-weight: 600;
	}

	.no-target {
		color: #4b5563;
		font-size: 12px;
		font-family: system-ui, sans-serif;
	}

	.divider {
		width: 1px;
		height: 16px;
		background: rgba(42, 42, 58, 0.6);
	}

	.content-badge {
		font-size: 9px;
		color: #a78bfa;
		font-family: system-ui, sans-serif;
	}

	.timer {
		font-family: 'JetBrains Mono', monospace;
		font-size: 12px;
		color: #4ade80;
		letter-spacing: 0.5px;
	}
</style>
