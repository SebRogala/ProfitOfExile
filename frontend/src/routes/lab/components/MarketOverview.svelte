<script lang="ts">
	import type { MarketOverviewData } from '$lib/api';

	let { data }: { data: MarketOverviewData } = $props();

	function deltaStr(v: number): string {
		if (v > 0) return `(+${v})`;
		if (v < 0) return `(${v})`;
		return '';
	}

	const temporalLabels: Record<string, string> = {
		none: 'None',
		hourly: 'Hourly',
		weekday_hour: 'Weekday x Hour',
	};

	const confidenceColors: Record<string, string> = {
		SAFE: 'var(--color-lab-green)',
		FAIR: 'var(--color-lab-yellow)',
		RISKY: 'var(--color-lab-red)',
	};

	const signalColors: Record<string, string> = {
		STABLE: 'var(--color-lab-green)',
		UNCERTAIN: 'var(--color-lab-yellow)',
		HERD: 'var(--color-lab-purple)',
		DUMPING: 'var(--color-lab-red)',
		TRAP: 'var(--color-lab-red)',
	};
</script>

<section class="section">
	<div class="section-header">
		<h2 class="section-title">Market Overview</h2>
		<span class="updated">Updated: {new Date().toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}</span>
	</div>
	<div class="overview-grid">
		<div class="stat-item">
			<span class="stat-label">Market avg price (transfigured):</span>
			<span class="stat-value">{data.avgTransPrice}c{#if data.avgTransPriceDelta !== 0} <span class="delta">{deltaStr(data.avgTransPriceDelta)}/12h</span>{/if}</span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Active gems:</span>
			<span class="stat-value">{data.activeGems}</span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Market avg base listings:</span>
			<span class="stat-value">{data.avgBaseListings}{#if data.avgBaseListingsDelta !== 0} <span class="delta">{deltaStr(data.avgBaseListingsDelta)}/12h</span>{/if}</span>
		</div>
		{#if data.divineRate > 0}
		<div class="stat-item">
			<span class="stat-label">Divine rate:</span>
			<span class="stat-value">{data.divineRate}c</span>
		</div>
		{/if}
		<div class="stat-item">
			<span class="stat-label">Temporal normalization:</span>
			<span class="stat-value">{temporalLabels[data.temporalMode] || data.temporalMode}</span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Most volatile:</span>
			<span class="stat-value color-{data.mostVolatileColor.toLowerCase()}">{data.mostVolatileColor} (avg CV: {data.mostVolatileCV}%)</span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Most stable:</span>
			<span class="stat-value color-{data.mostStableColor.toLowerCase()}">{data.mostStableColor} {data.mostStableCV}%</span>
		</div>
		<div class="stat-item spread-row">
			<span class="stat-label">Sell confidence:</span>
			<span class="stat-value spread">
				{#each Object.entries(data.sellConfidenceSpread) as [label, count]}
					<span class="tag" style="color: {confidenceColors[label] || 'var(--color-lab-text)'}">{label}: {count}</span>
				{/each}
			</span>
		</div>
		<div class="stat-item spread-row">
			<span class="stat-label">Signal distribution:</span>
			<span class="stat-value spread">
				{#each Object.entries(data.signalDistribution).filter(([, c]) => c > 0) as [label, count]}
					<span class="tag" style="color: {signalColors[label] || 'var(--color-lab-text)'}">{label}: {count}</span>
				{/each}
			</span>
		</div>
		{#if data.giftCurrentPrice > 0}
			<div class="stat-item spread-row gift-section">
				<span class="stat-label gift-label">Gift to the Goddess:</span>
				<span class="stat-value">{data.giftCurrentPrice}c</span>
			</div>
			{#if data.giftCheapDays}
				<div class="stat-item spread-row">
					<span class="stat-label">Best days to buy:</span>
					<span class="stat-value gift-cheap">{data.giftCheapDays}</span>
				</div>
			{/if}
			{#if data.giftExpensiveDays}
				<div class="stat-item spread-row">
					<span class="stat-label">Avoid buying:</span>
					<span class="stat-value gift-expensive">{data.giftExpensiveDays}</span>
				</div>
			{/if}
			{#if data.giftCheapHours}
				<div class="stat-item spread-row">
					<span class="stat-label">Cheapest hours (UTC):</span>
					<span class="stat-value gift-cheap">{data.giftCheapHours}</span>
				</div>
			{/if}
			{#if data.giftExpensiveHours}
				<div class="stat-item spread-row">
					<span class="stat-label">Expensive hours (UTC):</span>
					<span class="stat-value gift-expensive">{data.giftExpensiveHours}</span>
				</div>
			{/if}
		{/if}
	</div>
</section>

<style>
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 28px;
		margin-bottom: 32px;
	}
	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 16px;
	}
	.section-title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.updated {
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
	}
	.overview-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px 28px;
	}
	.stat-item {
		display: flex;
		gap: 10px;
		align-items: baseline;
		padding: 6px 0;
	}
	.spread-row {
		grid-column: 1 / -1;
	}
	.stat-label {
		color: var(--color-lab-text-secondary);
		font-size: 0.9375rem;
		white-space: nowrap;
	}
	.stat-value {
		color: var(--color-lab-text);
		font-size: 0.9375rem;
		font-weight: 600;
	}
	.delta {
		color: var(--color-lab-text-secondary);
		font-weight: 400;
		font-size: 0.875rem;
	}
	.spread {
		display: flex;
		gap: 14px;
		flex-wrap: wrap;
	}
	.tag {
		font-weight: 600;
		font-size: 0.875rem;
	}
	.color-red { color: var(--color-lab-red); }
	.color-green { color: var(--color-lab-green); }
	.color-blue { color: var(--color-lab-blue); }
	.gift-section {
		margin-top: 8px;
		padding-top: 12px;
		border-top: 1px solid var(--color-lab-border);
	}
	.gift-label {
		color: #fbbf24;
	}
	.gift-cheap {
		color: var(--color-lab-green);
	}
	.gift-expensive {
		color: var(--color-lab-red);
	}
</style>
