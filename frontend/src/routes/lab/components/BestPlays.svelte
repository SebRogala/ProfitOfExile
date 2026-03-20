<script lang="ts">
	import { fetchSignalHistory, type GemPlay, type SignalTransition } from '$lib/api';
	import { METRIC_TOOLTIPS } from '$lib/tooltips';
	import SignalBadge from './SignalBadge.svelte';
	import Sparkline from './Sparkline.svelte';
	import GemIcon from './GemIcon.svelte';
	import Select from '$lib/components/Select.svelte';

	const SORT_OPTIONS = [
		{ value: 'riskAdjusted', label: 'Risk-Adj ROI' },
		{ value: 'roi', label: 'Raw ROI' },
		{ value: 'roiPercent', label: 'ROI%' },
	];

	let {
		plays,
		title = 'Best Plays Now (ALL variants)',
		showVariantColumn = true,
		league = '',
	}: {
		plays: GemPlay[];
		title?: string;
		showVariantColumn?: boolean;
		league?: string;
	} = $props();

	let sortBy = $state<'riskAdjusted' | 'roi' | 'roiPercent'>('riskAdjusted');
	let budget = $state('');
	let expandedRow = $state<number | null>(null);

	function baseGemName(name: string): string {
		const idx = name.lastIndexOf(' of ');
		return idx > 0 ? name.substring(0, idx) : name;
	}

	function baseGemTradeUrl(gem: GemPlay): string {
		const base = baseGemName(gem.name);
		const parts = gem.variant.split('/');
		const level = parseInt(parts[0]) || 0;
		const quality = parts.length > 1 ? parseInt(parts[1]) : 0;

		const miscFilters: Record<string, any> = { corrupted: { option: 'false' } };
		if (level >= 20) miscFilters.gem_level = { min: level, max: level };
		if (quality === 20) miscFilters.quality = { min: 20, max: 20 };

		const q = {
			query: {
				type: base,
				status: { option: 'securable' },
				filters: {
					type_filters: { filters: { category: { option: 'gem' } } },
					misc_filters: { filters: miscFilters },
					trade_filters: { filters: { sale_type: { option: 'priced' }, collapse: { option: 'true' } } },
				},
			},
			sort: { price: 'asc' },
		};
		return `https://www.pathofexile.com/trade/search/${encodeURIComponent(league || 'Mirage')}?q=${encodeURIComponent(JSON.stringify(q))}`;
	}
	let expandedHistory = $state<SignalTransition[]>([]);
	let historyLoading = $state(false);

	let sorted = $derived.by(() => {
		let filtered = [...plays];
		const b = parseInt(budget);
		if (b > 0) {
			filtered = filtered.filter((p) => p.basePrice <= b);
			if (b <= 50) sortBy = 'roiPercent';
		}
		if (sortBy === 'riskAdjusted') {
			return filtered.sort((a, b) => b.weightedRoi - a.weightedRoi);
		}
		return filtered.sort((a, b) =>
			sortBy === 'roi' ? b.roi - a.roi : b.roiPercent - a.roiPercent
		);
	});

	function velocityStr(v: number): string {
		if (v > 0) return `↑${v}`;
		if (v < 0) return `↓${Math.abs(v)}`;
		return '0';
	}

	let historyError = $state(false);

	async function toggleRow(i: number) {
		if (expandedRow === i) {
			expandedRow = null;
			expandedHistory = [];
			return;
		}
		expandedRow = i;
		expandedHistory = [];
		historyLoading = true;
		historyError = false;
		const gem = sorted[i];
		try {
			expandedHistory = await fetchSignalHistory(gem.name, gem.variant);
		} catch (err) {
			console.warn(`[BestPlays] Signal history failed for ${gem.name}:`, err);
			historyError = true;
		} finally {
			historyLoading = false;
		}
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
			<Select bind:value={sortBy} options={SORT_OPTIONS} />
		</label>
	</div>
</div>

<div class="table-scroll">
<table class="plays-table">
	<thead>
		<tr>
			<th class="col-name" title="Transfigured gem name">Gem</th>
			{#if showVariantColumn}<th class="col-var" title="Gem variant: level/quality (e.g. 20/20 = level 20, 20% quality)">Var</th>{/if}
			<th class="col-tier" title="Price tier — TOP (high value), MID (moderate), LOW (budget). Auto-scales with league economy.">Tier</th>
			<th class="col-num" title={sortBy === 'riskAdjusted' ? 'Risk-adjusted ROI: raw ROI weighted by sellability and market stability' : METRIC_TOOLTIPS.ROI}>{sortBy === 'riskAdjusted' ? 'Adj ROI' : 'ROI'}</th>
			<th class="col-signal" title="Primary signal: STABLE, UNCERTAIN, DUMPING, HERD, RECOVERY, TRAP. Based on price velocity, listing changes, and historical position.">Signal</th>
			<th class="col-sell" title="Sellability score 0-100. How quickly you can sell this gem. Based on listings, demand velocity, and price tier.">Sell</th>
			<th class="col-signals" title="Window: CLOSED, BREWING, OPENING, OPEN, CLOSING, EXHAUSTED. Advanced: BREAKOUT, COMEBACK, POTENTIAL, PRICE_MANIPULATION.">Signals</th>
			<th class="col-num" title="Current transfigured gem listings on trade. Velocity arrow shows listing change direction.">Listings</th>
			<th class="col-spark" title="Price sparkline over the last 12 hours. Shows recent price trend at a glance.">12h</th>
		</tr>
	</thead>
	<tbody>
		{#each sorted as gem, i}
			<tr
				class="play-row"
				class:row-even={i % 2 === 0}
				onclick={() => toggleRow(i)}
			>
				<td class="col-name gem-name">
					<GemIcon name={gem.name} size={40} />
					<span>{gem.name}</span>
				</td>
				{#if showVariantColumn}<td class="col-var">{gem.variant}</td>{/if}
				<td class="col-tier">
					{#if showVariantColumn && gem.globalTier}
						<span class="tier-badge tier-{gem.globalTier.toLowerCase()}">{gem.globalTier}</span>
					{:else}
						<span class="tier-badge tier-{gem.priceTier.toLowerCase()}">{gem.priceTier}</span>
					{/if}
				</td>
				<td class="col-num roi-val">{sortBy === 'riskAdjusted' ? gem.weightedRoi : gem.roi}c</td>
				<td class="col-signal">
					<SignalBadge signal={gem.signal} />
					{#if gem.sellConfidence}
						<SignalBadge signal={gem.sellConfidence} type="confidence" />
					{/if}
				</td>
				<td class="col-sell">
					<span class="sell-score sell-{gem.sellabilityLabel.toLowerCase().replace(' ', '-')}" title="{gem.sellabilityLabel} ({gem.sellability})">{gem.sellability}</span>
				</td>
				<td class="col-signals">
					<SignalBadge signal={gem.windowSignal} type="window" />
					{#if gem.advancedSignal}
						<SignalBadge signal={gem.advancedSignal} type="advanced" />
					{/if}
				</td>
				<td class="col-listings">
					<span class="lst-count">{gem.transListings}</span>
					{#if gem.transVelocity > 0}
						<span class="vel-up">+{gem.transVelocity}</span>
					{:else if gem.transVelocity < 0}
						<span class="vel-down">{gem.transVelocity}</span>
					{/if}
				</td>
				<td class="col-spark"><Sparkline data={gem.sparkline} width={100} height={24} /></td>
			</tr>
			{#if expandedRow === i}
				<tr class="expanded-row">
					<td colspan={showVariantColumn ? 9 : 8} class="expanded-cell">
						<div class="expanded-content">
							<span class="expanded-meta">
								Base: {gem.basePrice}c | Trans: {gem.transPrice}c |
								Liq: {gem.liquidityTier} |
								Color: <span class="color-{gem.color.toLowerCase()}">{gem.color}</span> |
								Sellability: {gem.sellabilityLabel} ({gem.sellability})
								{#if gem.name.includes(' of ')}
									| <a class="buy-base-link" href={baseGemTradeUrl(gem)} target="_blank" title="Buy {baseGemName(gem.name)} ({gem.variant})">Buy Base</a>
								{/if}
							</span>
							{#if gem.sellUrgency}
								<div class="expanded-urgency urgency-{gem.sellUrgency.toLowerCase().replace('_', '-')}">
									{gem.sellUrgency.replace('_', ' ')}: {gem.sellReason}
								</div>
							{/if}
							{#if gem.tierAction}
								<div class="expanded-tier">{gem.priceTier}: {gem.tierAction}</div>
							{/if}
							<div class="expanded-history">
								History:
								{#if historyLoading}
									<span class="hist-loading">Loading history...</span>
								{:else if expandedHistory.length > 0}
									{#each expandedHistory as h}
										<div class="hist-line">
											<span class="hist-time">{h.time}</span>
											<SignalBadge signal={h.from} />
											<span class="hist-arrow">&rarr;</span>
											<SignalBadge signal={h.to} />
											<span class="hist-reason">{h.reason}</span>
											<span class="hist-listings">{h.listings} listings</span>
										</div>
									{/each}
								{:else if historyError}
									<span class="hist-none">History unavailable</span>
								{:else}
									<span class="hist-none">No signal changes recorded</span>
								{/if}
							</div>
						</div>
					</td>
				</tr>
			{/if}
		{/each}
	</tbody>
</table>
</div>

<style>
	.plays-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 16px;
	}
	.plays-title {
		font-size: 1.0625rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.plays-controls {
		display: flex;
		gap: 16px;
		align-items: center;
	}
	.control-label {
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.budget-input {
		width: 90px;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text);
		padding: 5px 10px;
		font-size: 0.875rem;
		font-family: inherit;
	}
	.budget-input::placeholder {
		color: var(--color-lab-text-secondary);
	}
	.table-scroll {
		max-height: 590px;
		overflow-y: auto;
		scrollbar-color: rgba(255, 255, 255, 0.15) transparent;
		scrollbar-width: thin;
	}
	.table-scroll::-webkit-scrollbar {
		width: 6px;
	}
	.table-scroll::-webkit-scrollbar-track {
		background: transparent;
	}
	.table-scroll::-webkit-scrollbar-thumb {
		background: rgba(255, 255, 255, 0.15);
		border-radius: 3px;
	}
	.table-scroll::-webkit-scrollbar-thumb:hover {
		background: rgba(255, 255, 255, 0.25);
	}
	.plays-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.9375rem;
		table-layout: fixed;
	}
	.plays-table th {
		text-align: left;
		color: var(--color-lab-text-secondary);
		font-weight: 600;
		font-size: 0.8125rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		padding: 10px 12px;
		border-bottom: 1px solid var(--color-lab-border);
		cursor: help;
		position: sticky;
		top: 0;
		background: var(--color-lab-surface);
		z-index: 1;
	}
	.plays-table td {
		padding: 14px 12px;
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
		width: 280px;
	}
	.gem-name {
		display: flex;
		align-items: center;
		gap: 10px;
		font-weight: 600;
		white-space: nowrap;
	}
	.col-var {
		width: 55px;
		white-space: nowrap;
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
	}
	.col-num {
		width: 70px;
		text-align: left;
		white-space: nowrap;
	}
	.col-signal {
		width: 180px;
		white-space: nowrap;
	}
	.col-signal :global(.signal-badge + .signal-badge) {
		margin-left: 4px;
	}
	.col-signals {
		width: 180px;
		white-space: nowrap;
	}
	.col-signals :global(.signal-badge + .signal-badge) {
		margin-left: 6px;
	}
	.col-spark {
		width: 110px;
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
	.col-listings {
		white-space: nowrap;
	}
	.lst-count {
		font-weight: 600;
		margin-right: 6px;
	}
	.vel-up {
		font-size: 0.75rem;
		color: var(--color-lab-green);
		font-weight: 700;
		padding: 1px 5px;
		background: rgba(34, 197, 94, 0.1);
	}
	.vel-down {
		font-size: 0.75rem;
		color: var(--color-lab-red);
		font-weight: 700;
		padding: 1px 5px;
		background: rgba(239, 68, 68, 0.1);
	}
	.expanded-row {
		background: var(--color-lab-bg);
	}
	.expanded-cell {
		padding: 14px 18px !important;
	}
	.expanded-content {
		font-size: 0.875rem;
	}
	.expanded-meta {
		color: var(--color-lab-text-secondary);
	}
	.expanded-history {
		margin-top: 8px;
		color: var(--color-lab-text-secondary);
	}
	.hist-line { display: flex; gap: 8px; align-items: center; padding: 4px 0; }
	.hist-time { color: var(--color-lab-text); font-weight: 600; min-width: 42px; font-size: 0.8125rem; }
	.hist-arrow { color: var(--color-lab-text-secondary); font-size: 1.125rem; }
	.hist-reason { color: var(--color-lab-text-secondary); font-size: 0.8125rem; }
	.hist-listings { margin-left: auto; color: var(--color-lab-text-secondary); font-size: 0.8125rem; }
	.hist-loading, .hist-none { color: var(--color-lab-text-secondary); font-size: 0.8125rem; }
	/* Tier badge */
	.col-tier { width: 55px; }
	.tier-badge {
		font-size: 0.75rem;
		font-weight: 700;
		padding: 2px 8px;
		letter-spacing: 0.03em;
	}
	.col-tier { width: 65px; }
	.tier-top { color: #fbbf24; background: rgba(251, 191, 36, 0.12); }
	.tier-high { color: #fb923c; background: rgba(251, 146, 60, 0.12); }
	.tier-mid-high { color: #c084fc; background: rgba(192, 132, 252, 0.12); }
	.tier-mid { color: #94a3b8; background: rgba(148, 163, 184, 0.12); }
	.tier-low { color: #64748b; background: rgba(100, 116, 139, 0.1); }
	.tier-floor { color: #475569; background: rgba(71, 85, 105, 0.08); }

	/* Sellability */
	.col-sell { width: 55px; }
	.sell-score {
		font-weight: 700;
		font-size: 0.875rem;
	}
	.sell-fast-sell { color: var(--color-lab-green); }
	.sell-good { color: #86efac; }
	.sell-moderate { color: var(--color-lab-yellow); }
	.sell-slow { color: #fb923c; }
	.sell-unlikely { color: var(--color-lab-red); }

	/* Expanded urgency */
	.expanded-urgency {
		margin-top: 6px;
		font-size: 0.8125rem;
		font-weight: 600;
	}
	.urgency-sell-now { color: var(--color-lab-red); }
	.urgency-undercut { color: #fb923c; }
	.urgency-hold { color: var(--color-lab-green); }
	.urgency-wait { color: var(--color-lab-text-secondary); }
	.expanded-tier {
		margin-top: 4px;
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
		font-style: italic;
	}

	.color-red { color: var(--color-lab-red); }
	.color-green { color: var(--color-lab-green); }
	.color-blue { color: var(--color-lab-blue); }

	.buy-base-link {
		color: var(--color-lab-blue, #3b82f6);
		text-decoration: none;
		font-weight: 600;
	}
	.buy-base-link:hover {
		color: var(--color-lab-text);
		text-decoration: underline;
	}
</style>
