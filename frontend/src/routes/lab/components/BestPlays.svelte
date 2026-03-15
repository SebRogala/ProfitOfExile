<script lang="ts">
	import type { GemPlay } from '$lib/api';
	import { METRIC_TOOLTIPS } from '$lib/tooltips';
	import SignalBadge from './SignalBadge.svelte';
	import Sparkline from './Sparkline.svelte';

	let {
		plays,
		title = 'Best Plays Now (ALL variants)',
		showVariantColumn = true,
	}: {
		plays: GemPlay[];
		title?: string;
		showVariantColumn?: boolean;
	} = $props();

	let sortBy = $state<'roi' | 'roiPercent'>('roi');
	let budget = $state('');
	let expandedRow = $state<number | null>(null);

	let sorted = $derived.by(() => {
		let filtered = [...plays];
		const b = parseInt(budget);
		if (b > 0) {
			filtered = filtered.filter((p) => p.basePrice <= b);
			if (b <= 50) sortBy = 'roiPercent';
		}
		return filtered.sort((a, b) =>
			sortBy === 'roi' ? b.roi - a.roi : b.roiPercent - a.roiPercent
		);
	});

	function velocityStr(v: number): string {
		if (v > 0) return `\u2191${v}`;
		if (v < 0) return `\u2193${Math.abs(v)}`;
		return '0';
	}

	function toggleRow(i: number) {
		expandedRow = expandedRow === i ? null : i;
	}
</script>

<div class="plays-header">
	<h3 class="plays-title">{title}</h3>
	<div class="plays-controls">
		<label class="control-label">
			Budget:
			<input
				type="text"
				class="budget-input"
				placeholder="unlimited"
				bind:value={budget}
			/>
		</label>
		<label class="control-label">
			Sort:
			<select bind:value={sortBy} class="sort-select">
				<option value="roi">ROI</option>
				<option value="roiPercent">ROI%</option>
			</select>
		</label>
	</div>
</div>

<table class="plays-table">
	<thead>
		<tr>
			<th class="col-name">Gem</th>
			{#if showVariantColumn}<th class="col-var">Var</th>{/if}
			<th class="col-num" title={METRIC_TOOLTIPS.ROI}>ROI</th>
			<th class="col-num" title={METRIC_TOOLTIPS['ROI%']}>ROI%</th>
			<th class="col-signal">Signal</th>
			<th class="col-num" title={METRIC_TOOLTIPS.CV}>CV</th>
			<th class="col-signal">Window</th>
			<th class="col-signal">Adv</th>
			<th class="col-num">Trans</th>
			<th class="col-num">Base</th>
			<th class="col-spark">2h</th>
		</tr>
	</thead>
	<tbody>
		{#each sorted as gem, i}
			<tr
				class="play-row"
				class:row-even={i % 2 === 0}
				onclick={() => toggleRow(i)}
			>
				<td class="col-name gem-name">{gem.name}</td>
				{#if showVariantColumn}<td class="col-var">{gem.variant}</td>{/if}
				<td class="col-num roi-val">{gem.roi}c</td>
				<td class="col-num">{gem.roiPercent}%</td>
				<td class="col-signal"><SignalBadge signal={gem.signal} /></td>
				<td class="col-num cv-val" class:cv-high={gem.cv > 50}>{gem.cv}%</td>
				<td class="col-signal"><SignalBadge signal={gem.windowSignal} type="window" /></td>
				<td class="col-signal">
					{#if gem.advancedSignal}
						<SignalBadge signal={gem.advancedSignal} type="advanced" />
					{/if}
				</td>
				<td class="col-num">
					{gem.transListings}
					<span class="velocity">{velocityStr(gem.transVelocity)}</span>
				</td>
				<td class="col-num">
					{gem.baseListings}
					<span class="velocity">{velocityStr(gem.baseVelocity)}</span>
				</td>
				<td class="col-spark"><Sparkline data={gem.sparkline} width={60} height={16} /></td>
			</tr>
			{#if expandedRow === i}
				<tr class="expanded-row">
					<td colspan={showVariantColumn ? 11 : 10} class="expanded-cell">
						<div class="expanded-content">
							<span class="expanded-meta">
								Base: {gem.basePrice}c | Trans: {gem.transPrice}c |
								Liq: {gem.liquidityTier} |
								Color: <span class="color-{gem.color.toLowerCase()}">{gem.color}</span>
							</span>
							<div class="expanded-history">
								History:
								{#each gem.signalHistory as h}
									<span class="hist-entry">{h.time} {h.from}\u2192{h.to} ({h.reason})</span>
								{/each}
							</div>
						</div>
					</td>
				</tr>
			{/if}
		{/each}
	</tbody>
</table>

<style>
	.plays-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 8px;
	}
	.plays-title {
		font-size: 0.875rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.plays-controls {
		display: flex;
		gap: 12px;
		align-items: center;
	}
	.control-label {
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.budget-input {
		width: 80px;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text);
		padding: 2px 6px;
		font-size: 0.75rem;
		font-family: inherit;
	}
	.budget-input::placeholder {
		color: var(--color-lab-text-secondary);
	}
	.sort-select {
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text);
		padding: 2px 6px;
		font-size: 0.75rem;
		font-family: inherit;
	}
	.plays-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8125rem;
	}
	.plays-table th {
		text-align: left;
		color: var(--color-lab-text-secondary);
		font-weight: 600;
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		padding: 6px 8px;
		border-bottom: 1px solid var(--color-lab-border);
		cursor: help;
	}
	.plays-table td {
		padding: 5px 8px;
		color: var(--color-lab-text);
		border-bottom: 1px solid rgba(42, 45, 55, 0.5);
	}
	.play-row {
		cursor: pointer;
	}
	.play-row:hover {
		background: rgba(59, 130, 246, 0.05);
	}
	.row-even {
		background: rgba(26, 29, 39, 0.5);
	}
	.col-name {
		min-width: 180px;
	}
	.col-var {
		min-width: 45px;
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
	}
	.col-num {
		text-align: right;
		white-space: nowrap;
		font-size: 0.8125rem;
	}
	.col-signal {
		white-space: nowrap;
	}
	.col-spark {
		width: 60px;
	}
	.gem-name {
		font-weight: 600;
	}
	.roi-val {
		color: var(--color-lab-green);
		font-weight: 700;
	}
	.cv-val {
		color: var(--color-lab-text-secondary);
	}
	.cv-high {
		color: var(--color-lab-yellow);
	}
	.velocity {
		font-size: 0.6875rem;
		color: var(--color-lab-text-secondary);
		margin-left: 2px;
	}
	.expanded-row {
		background: var(--color-lab-bg);
	}
	.expanded-cell {
		padding: 8px 12px !important;
	}
	.expanded-content {
		font-size: 0.75rem;
	}
	.expanded-meta {
		color: var(--color-lab-text-secondary);
	}
	.expanded-history {
		margin-top: 4px;
		color: var(--color-lab-text-secondary);
	}
	.hist-entry {
		margin-right: 12px;
	}
	.color-red { color: var(--color-lab-red); }
	.color-green { color: var(--color-lab-green); }
	.color-blue { color: var(--color-lab-blue); }
</style>
