<script lang="ts">
	import type { MarketOverviewData } from '$lib/api';

	let { data }: { data: MarketOverviewData } = $props();

	function deltaStr(v: number): string {
		if (v > 0) return `(↑${v})`;
		if (v < 0) return `(↓${Math.abs(v)})`;
		return '(0)';
	}

	function breakdownStr(bd: Record<string, number>): string {
		return Object.entries(bd).map(([k, v]) => `${k}:${v}`).join(', ');
	}
</script>

<section class="section">
	<div class="section-header">
		<h2 class="section-title">Market Overview</h2>
		<span class="updated">Updated: {new Date().toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}</span>
	</div>
	<div class="overview-grid">
		<div class="stat-item">
			<span class="stat-label">Market avg price (transfigured):</span>
			<span class="stat-value">{data.avgTransPrice}c <span class="delta">{deltaStr(data.avgTransPriceDelta)}/2h</span></span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Active gems:</span>
			<span class="stat-value">{data.activeGems}</span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Market avg base listings:</span>
			<span class="stat-value">{data.avgBaseListings} <span class="delta">{deltaStr(data.avgBaseListingsDelta)}/2h</span></span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Weekend premium:</span>
			<span class="stat-value">~{data.weekendPremium}%</span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Gems with WINDOW signals:</span>
			<span class="stat-value">{data.windowGems} ({breakdownStr(data.windowBreakdown)})</span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Gems with TRAP:</span>
			<span class="stat-value trap">{data.trapGems}</span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Most volatile:</span>
			<span class="stat-value color-{data.mostVolatileColor.toLowerCase()}">{data.mostVolatileColor} (avg CV: {data.mostVolatileCV}%)</span>
		</div>
		<div class="stat-item">
			<span class="stat-label">Most stable:</span>
			<span class="stat-value color-{data.mostStableColor.toLowerCase()}">{data.mostStableColor} {data.mostStableCV}%</span>
		</div>
	</div>
</section>

<style>
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 24px;
		margin-bottom: 32px;
	}
	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 12px;
	}
	.section-title {
		font-size: 0.9375rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.updated {
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
	}
	.overview-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 8px 24px;
	}
	.stat-item {
		display: flex;
		gap: 8px;
		align-items: baseline;
		padding: 4px 0;
	}
	.stat-label {
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
	}
	.stat-value {
		color: var(--color-lab-text);
		font-size: 0.8125rem;
		font-weight: 600;
	}
	.delta {
		color: var(--color-lab-text-secondary);
		font-weight: 400;
		font-size: 0.75rem;
	}
	.trap {
		color: var(--color-lab-red);
	}
	.color-red { color: var(--color-lab-red); }
	.color-green { color: var(--color-lab-green); }
	.color-blue { color: var(--color-lab-blue); }
</style>
