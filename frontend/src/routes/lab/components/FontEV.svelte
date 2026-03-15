<script lang="ts">
	import type { FontEVData } from '$lib/api';
	import { METRIC_TOOLTIPS } from '$lib/tooltips';

	let { data }: { data: FontEVData } = $props();

	const COLOR_LABELS: Record<string, { emoji: string; cssClass: string }> = {
		RED: { emoji: '\ud83d\udd34', cssClass: 'font-red' },
		GREEN: { emoji: '\ud83d\udfe2', cssClass: 'font-green' },
		BLUE: { emoji: '\ud83d\udd35', cssClass: 'font-blue' },
	};

	function deltaStr(v: number): string {
		if (v > 0) return `+${v}c`;
		if (v < 0) return `${v}c`;
		return '0c';
	}
</script>

<div class="font-ev">
	<h4 class="font-title">Font EV</h4>
	<div class="color-cards">
		{#each data.colors as fc}
			{@const cl = COLOR_LABELS[fc.color]}
			<div class="color-card {cl.cssClass}">
				<div class="color-header">
					<span class="color-emoji">{cl.emoji}</span>
					<span class="color-name">{fc.color}</span>
				</div>
				<div class="color-stats">
					<span class="stat" title={METRIC_TOOLTIPS.EV}>EV: <strong>{fc.ev}c</strong></span>
					<span class="stat" title={METRIC_TOOLTIPS.Pool}>Pool: {fc.pool}</span>
					<span class="stat">Winners: {fc.winners}</span>
					<span class="stat" title={METRIC_TOOLTIPS.pWin}>pWin: {fc.pWin}%</span>
					<span class="stat">Profit: {fc.profit}c</span>
					<span class="stat delta" title={METRIC_TOOLTIPS['\u03942h']}>
						\u03942h: EV {deltaStr(fc.evDelta2h)}
					</span>
				</div>
			</div>
		{/each}
	</div>
	<div class="quality-compare">
		vs Quality avg ROI: {data.qualityAvgRoi}c \u2192 Font {data.bestColor} wins by {data.bestAdvantage}c
	</div>
</div>

<style>
	.font-ev {
		margin-top: 12px;
		border-top: 1px solid var(--color-lab-border);
		padding-top: 10px;
	}
	.font-title {
		font-size: 0.8125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0 0 8px 0;
	}
	.color-cards {
		display: flex;
		gap: 10px;
	}
	.color-card {
		flex: 1;
		border: 1px solid var(--color-lab-border);
		padding: 8px 10px;
		background: var(--color-lab-bg);
	}
	.color-header {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 6px;
	}
	.color-emoji {
		font-size: 0.875rem;
	}
	.color-name {
		font-weight: 700;
		font-size: 0.8125rem;
	}
	.font-red .color-name { color: var(--color-lab-red); }
	.font-green .color-name { color: var(--color-lab-green); }
	.font-blue .color-name { color: var(--color-lab-blue); }
	.color-stats {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 12px;
	}
	.stat {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		cursor: help;
	}
	.stat strong {
		color: var(--color-lab-text);
	}
	.delta {
		color: var(--color-lab-text-secondary);
	}
	.quality-compare {
		margin-top: 8px;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
	}
</style>
