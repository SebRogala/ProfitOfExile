<script lang="ts">
	import type { FontEVData } from '$lib/api';
	import { METRIC_TOOLTIPS } from '$lib/tooltips';

	let { data }: { data: FontEVData } = $props();

	const COLOR_LABELS: Record<string, { icon: string; cssClass: string; colorClass: string }> = {
		RED: { icon: '●', cssClass: 'font-red', colorClass: 'icon-red' },
		GREEN: { icon: '●', cssClass: 'font-green', colorClass: 'icon-green' },
		BLUE: { icon: '●', cssClass: 'font-blue', colorClass: 'icon-blue' },
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
					<span class="color-icon {cl.colorClass}">{cl.icon}</span>
					<span class="color-name">{fc.color}</span>
				</div>
				<div class="color-stats">
					<span class="stat" title={METRIC_TOOLTIPS.EV}>EV: <strong>{fc.ev}c</strong></span>
					<span class="stat" title={METRIC_TOOLTIPS.Pool}>Pool: {fc.pool}</span>
					<span class="stat">Winners: {fc.winners}</span>
					<span class="stat" title={METRIC_TOOLTIPS.pWin}>pWin: {fc.pWin}%</span>
					<span class="stat">Profit: {fc.profit}c</span>
					<span class="stat delta" title={METRIC_TOOLTIPS['\u03942h']}>
						Δ2h: EV {deltaStr(fc.evDelta2h)}
					</span>
				</div>
			</div>
		{/each}
	</div>
	<div class="quality-compare">
		vs Quality avg ROI: {data.qualityAvgRoi}c → Font {data.bestColor} wins by {data.bestAdvantage}c
	</div>
</div>

<style>
	.font-ev {
		margin-top: 16px;
		border-top: 1px solid var(--color-lab-border);
		padding-top: 14px;
	}
	.font-title {
		font-size: 0.8125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0 0 10px 0;
	}
	.color-cards {
		display: flex;
		gap: 12px;
	}
	.color-card {
		flex: 1;
		border: 1px solid var(--color-lab-border);
		padding: 12px 14px;
		background: var(--color-lab-bg);
	}
	.color-header {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 8px;
	}
	.color-icon {
		font-size: 0.75rem;
	}
	.icon-red { color: var(--color-lab-red); }
	.icon-green { color: var(--color-lab-green); }
	.icon-blue { color: var(--color-lab-blue); }
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
		gap: 6px 14px;
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
		margin-top: 10px;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
	}
</style>
