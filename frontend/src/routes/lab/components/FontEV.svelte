<script lang="ts">
	import type { FontEVResponse, FontColor } from '$lib/api';
	import { METRIC_TOOLTIPS } from '$lib/tooltips';
	import InfoTooltip from './InfoTooltip.svelte';

	let { data }: { data: FontEVResponse } = $props();

	let mode = $state<'safe' | 'jackpot'>('safe');

	const COLOR_LABELS: Record<string, { icon: string; cssClass: string; colorClass: string }> = {
		RED: { icon: '●', cssClass: 'font-red', colorClass: 'icon-red' },
		GREEN: { icon: '●', cssClass: 'font-green', colorClass: 'icon-green' },
		BLUE: { icon: '●', cssClass: 'font-blue', colorClass: 'icon-blue' },
	};

	const FIXED_ORDER = ['RED', 'GREEN', 'BLUE'];
	let rawColors = $derived(mode === 'safe' ? data.safe : data.jackpot);
	// Always display in fixed RED, GREEN, BLUE order regardless of API response order.
	let activeColors = $derived(
		FIXED_ORDER.map(c => rawColors.find(fc => fc.color === c)).filter(Boolean) as FontColor[]
	);
	let bestColor = $derived(mode === 'safe' ? data.bestColorSafe : data.bestColorJackpot);

	function deltaStr(v: number): string {
		if (v > 0) return `+${v}c`;
		if (v < 0) return `${v}c`;
		return '0c';
	}

	function liquidityBadge(fc: FontColor): string | null {
		if (!fc.liquidityRisk || fc.liquidityRisk === 'LOW') return null;
		return `${fc.liquidityRisk} -- ${fc.thinPoolGems || 0} of ${fc.winners} winners have <5 listings`;
	}
</script>

<div class="font-ev">
	<div class="font-header">
		<h4 class="font-title">Font EV<InfoTooltip text="Expected income per Font of Divine Skill usage.<br><br><b>EV</b>: Average income picking the best gem from 3 random draws. Includes ALL possible outcomes (hits + misses).<br><br><b>Safe mode (LOW+)</b>: Hit rate for decent+ gems. Reliable income per font.<br><b>Jackpot mode (HIGH+)</b>: Hit rate for premium gems only. Lower chance, bigger payout.<br><br><b>Pool</b>: Total transfigured gems of this color.<br><b>pWin</b>: Chance of seeing at least 1 qualifying gem in 3 picks.<br><b>Winners</b>: How many gems in the pool qualify." /></h4>
		<div class="mode-toggle">
			<button
				class="toggle-btn"
				class:active={mode === 'safe'}
				onclick={() => mode = 'safe'}
			>Safe</button>
			<button
				class="toggle-btn"
				class:active={mode === 'jackpot'}
				onclick={() => mode = 'jackpot'}
			>Jackpot</button>
		</div>
	</div>
	<div class="color-cards">
		{#each activeColors as fc}
			{@const cl = COLOR_LABELS[fc.color]}
			{@const badge = liquidityBadge(fc)}
			{#if cl}
				<div class="color-card {cl.cssClass}" class:best-red={fc.color === bestColor && fc.color === 'RED'} class:best-green={fc.color === bestColor && fc.color === 'GREEN'} class:best-blue={fc.color === bestColor && fc.color === 'BLUE'}>
					<div class="color-header">
						<span class="color-icon {cl.colorClass}">{cl.icon}</span>
						<span class="color-name">{fc.color}</span>
						{#if fc.color === bestColor}
							<span class="best-badge best-badge-{fc.color.toLowerCase()}">BEST</span>
						{/if}
					</div>
					<div class="color-stats">
						<span class="stat" title={METRIC_TOOLTIPS.EV}>EV: <strong>{fc.ev}c</strong></span>
						<span class="stat" title={METRIC_TOOLTIPS.Pool}>Pool: {fc.pool}</span>
						<span class="stat">Winners: {fc.winners}</span>
						<span class="stat" title={METRIC_TOOLTIPS.pWin}>pWin: {fc.pWin}%</span>
						<span class="stat">Profit: {fc.profit}c</span>
						<span class="stat delta" title={METRIC_TOOLTIPS['\u039412h']}>
							Δ12h: EV {deltaStr(fc.evDelta2h)}
						</span>
					</div>
					{#if badge}
						<div class="liquidity-badge risk-{fc.liquidityRisk?.toLowerCase()}">
							{badge}
						</div>
					{/if}
				</div>
			{/if}
		{/each}
	</div>
</div>

<style>
	.font-ev {
		margin-top: 20px;
		border-top: 1px solid var(--color-lab-border);
		padding-top: 18px;
	}
	.font-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 12px;
	}
	.font-title {
		font-size: 1rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.mode-toggle {
		display: flex;
		gap: 0;
	}
	.toggle-btn {
		background: transparent;
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text-secondary);
		padding: 4px 14px;
		font-size: 0.8125rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
		transition: all 0.15s ease;
	}
	.toggle-btn:first-child {
		border-right: none;
	}
	.toggle-btn:hover {
		color: var(--color-lab-text);
		border-color: var(--color-lab-text-secondary);
	}
	.toggle-btn.active {
		color: var(--color-lab-text);
		border-color: var(--color-lab-blue);
		background: rgba(59, 130, 246, 0.15);
	}
	.color-cards {
		display: flex;
		gap: 14px;
	}
	.color-card {
		flex: 1;
		border: 1px solid var(--color-lab-border);
		padding: 16px 18px;
		background: var(--color-lab-bg);
	}
	.color-card.best-red {
		border-color: var(--color-lab-red);
		background: rgba(239, 68, 68, 0.04);
	}
	.color-card.best-green {
		border-color: var(--color-lab-green);
		background: rgba(34, 197, 94, 0.04);
	}
	.color-card.best-blue {
		border-color: var(--color-lab-blue);
		background: rgba(59, 130, 246, 0.04);
	}
	.color-header {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-bottom: 10px;
	}
	.color-icon {
		font-size: 0.875rem;
	}
	.icon-red { color: var(--color-lab-red); }
	.icon-green { color: var(--color-lab-green); }
	.icon-blue { color: var(--color-lab-blue); }
	.color-name {
		font-weight: 700;
		font-size: 0.9375rem;
	}
	.font-red .color-name { color: var(--color-lab-red); }
	.font-green .color-name { color: var(--color-lab-green); }
	.font-blue .color-name { color: var(--color-lab-blue); }
	.best-badge {
		font-size: 0.625rem;
		font-weight: 700;
		padding: 1px 6px;
		letter-spacing: 0.05em;
		border: 1px solid;
	}
	.best-badge-red { color: var(--color-lab-red); border-color: var(--color-lab-red); }
	.best-badge-green { color: var(--color-lab-green); border-color: var(--color-lab-green); }
	.best-badge-blue { color: var(--color-lab-blue); border-color: var(--color-lab-blue); }
	.color-stats {
		display: flex;
		flex-wrap: wrap;
		gap: 8px 16px;
	}
	.stat {
		font-size: 0.875rem;
		color: var(--color-lab-text-secondary);
		cursor: help;
	}
	.stat strong {
		color: var(--color-lab-text);
	}
	.delta {
		color: var(--color-lab-text-secondary);
	}
	.liquidity-badge {
		margin-top: 10px;
		font-size: 0.8125rem;
		padding: 4px 10px;
		border: 1px solid;
	}
	.risk-medium {
		color: var(--color-lab-yellow, #eab308);
		border-color: rgba(234, 179, 8, 0.3);
		background: rgba(234, 179, 8, 0.06);
	}
	.risk-high {
		color: var(--color-lab-red);
		border-color: rgba(239, 68, 68, 0.3);
		background: rgba(239, 68, 68, 0.06);
	}
</style>
