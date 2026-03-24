<script lang="ts">
	import type { MarketOverviewData } from '$lib/api';
	import { getGemIconUrl } from '$lib/gem-icons';
	import OfferingChart from './OfferingChart.svelte';
	import { onDestroy } from 'svelte';

	let { data }: { data: MarketOverviewData } = $props();

	let now = $state(new Date());
	const tickInterval = setInterval(() => { now = new Date(); }, 1000);
	onDestroy(() => clearInterval(tickInterval));

	function timeUntilUTCHour(targetHour: number): string {
		const utcH = now.getUTCHours();
		const utcM = now.getUTCMinutes();
		let hoursLeft = targetHour - utcH;
		if (hoursLeft < 0 || (hoursLeft === 0 && utcM > 0)) hoursLeft += 24;
		const minsLeft = hoursLeft * 60 - utcM;
		const h = Math.floor(minsLeft / 60);
		const m = minsLeft % 60;
		if (h === 0) return `${m}m`;
		return `${h}h ${m}m`;
	}

	function findNextHour(hours: { hour: number; median: number }[]): { hour: number; median: number } | null {
		if (!hours?.length) return null;
		const utcH = now.getUTCHours();
		const utcM = now.getUTCMinutes();
		let best: { hour: number; median: number; dist: number } | null = null;
		for (const h of hours) {
			let dist = h.hour - utcH;
			if (dist < 0 || (dist === 0 && utcM > 0)) dist += 24;
			if (!best || dist < best.dist) best = { ...h, dist };
		}
		return best;
	}

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
	</div>

	{#if data.offerings?.length}
		<div class="offerings-section">
			<div class="offerings-grid">
				{#each data.offerings as off}
					{@const cheapNext = findNextHour(off.cheapHours)}
					{@const expNext = findNextHour(off.expensiveHours)}
					<div class="off-card">
						<div class="off-top-row">
							<div class="off-identity">
								<img src={getGemIconUrl(off.name)} alt={off.name} width="36" height="36" class="off-icon" />
								<span class="off-name">{off.name}</span>
								<span class="off-price">{off.currentPrice}c</span>
							</div>
							<div class="off-timers">
								{#if cheapNext}
									<div class="off-timer">
										<span class="off-timer-label">Buy in</span>
										<span class="off-timer-val off-cheap-val">{timeUntilUTCHour(cheapNext.hour)}</span>
										<span class="off-timer-at">{String(cheapNext.hour).padStart(2, '0')}:00 ~{cheapNext.median}c</span>
									</div>
								{/if}
								{#if expNext}
									<div class="off-timer">
										<span class="off-timer-label">Sell in</span>
										<span class="off-timer-val off-exp-val">{timeUntilUTCHour(expNext.hour)}</span>
										<span class="off-timer-at">{String(expNext.hour).padStart(2, '0')}:00 ~{expNext.median}c</span>
									</div>
								{/if}
							</div>
						</div>

						{#if off.sparkline?.length > 2}
							<div class="off-chart-row">
								<OfferingChart data={off.sparkline} hourlyMedians={off.hourlyMedians} todayHourMedians={off.todayHourMedians} height={140} />
							</div>
						{/if}

						<div class="off-details">
							{#if off.cheapHours?.length}
								<div class="off-detail-col">
									<span class="off-detail-label">Cheap hours</span>
									{#each off.cheapHours as h}
										<span class="off-hour off-cheap-text">{String(h.hour).padStart(2, '0')}:00 <span class="off-median">~{h.median}c</span></span>
									{/each}
								</div>
							{/if}
							{#if off.expensiveHours?.length}
								<div class="off-detail-col">
									<span class="off-detail-label">Expensive hours</span>
									{#each off.expensiveHours as h}
										<span class="off-hour off-exp-text">{String(h.hour).padStart(2, '0')}:00 <span class="off-median">~{h.median}c</span></span>
									{/each}
								</div>
							{/if}
							{#if off.cheapDays?.length}
								<div class="off-detail-col">
									<span class="off-detail-label">Cheap days</span>
									{#each off.cheapDays as d}
										<span class="off-hour off-cheap-text">{d.day} <span class="off-median">~{d.median}c</span></span>
									{/each}
								</div>
							{/if}
							{#if off.expensiveDays?.length}
								<div class="off-detail-col">
									<span class="off-detail-label">Expensive days</span>
									{#each off.expensiveDays as d}
										<span class="off-hour off-exp-text">{d.day} <span class="off-median">~{d.median}c</span></span>
									{/each}
								</div>
							{/if}
						</div>
					</div>
				{/each}
			</div>
		</div>
	{/if}
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
	.offerings-section {
		margin-top: 20px;
		padding-top: 18px;
		border-top: 1px solid var(--color-lab-border);
	}
	.offerings-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
		gap: 14px;
	}
	.off-card {
		border: 1px solid var(--color-lab-border);
		padding: 12px 16px;
		background: rgba(42, 45, 55, 0.3);
	}
	.off-top-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 8px;
	}
	.off-identity {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.off-icon {
		object-fit: contain;
		border-radius: 2px;
	}
	.off-name {
		font-size: 1rem;
		font-weight: 700;
		color: #fbbf24;
	}
	.off-price {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
	}
	.off-timers {
		display: flex;
		gap: 10px;
	}
	.off-chart-row {
		margin-bottom: 8px;
	}
	.off-timer {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 3px 10px;
		border: 1px solid var(--color-lab-border);
		background: rgba(42, 45, 55, 0.5);
	}
	.off-timer-label {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		font-weight: 600;
	}
	.off-timer-val {
		font-size: 1.0625rem;
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}
	.off-cheap-val { color: var(--color-lab-green); }
	.off-exp-val { color: var(--color-lab-red); }
	.off-timer-at {
		font-size: 0.6875rem;
		color: var(--color-lab-text-secondary);
	}
	.off-details {
		display: grid;
		grid-template-columns: 1fr 1fr 1fr 1fr;
		gap: 4px 12px;
	}
	.off-detail-col {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.off-detail-label {
		font-size: 0.625rem;
		font-weight: 700;
		color: var(--color-lab-text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		margin-bottom: 1px;
	}
	.off-hour {
		font-size: 0.8125rem;
	}
	.off-cheap-text { color: var(--color-lab-green); }
	.off-exp-text { color: var(--color-lab-red); }
	.off-median {
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
	}
</style>
