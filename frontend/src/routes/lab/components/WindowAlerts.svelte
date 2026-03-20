<script lang="ts">
	import type { WindowAlert } from '$lib/api';
	import SignalBadge from './SignalBadge.svelte';
	import GemIcon from './GemIcon.svelte';

	let { alerts }: { alerts: WindowAlert[] } = $props();

	function tierClass(tier: string): string {
		return { TOP: 'tier-top', HIGH: 'tier-high', MID: 'tier-mid', LOW: 'tier-low' }[tier] || '';
	}
</script>

{#if alerts.length > 0}
	<section class="section">
		<h2 class="section-title">Window Alerts</h2>
		<div class="alerts-grid">
		{#each alerts as alert}
			<div class="alert-card">
				<div class="alert-name">
					<GemIcon name={alert.name} size={40} />
					<span class="gem-name">{alert.name}</span>
					{#if alert.priceTier}
						<span class="tier-badge {tierClass(alert.priceTier)}">{alert.priceTier}</span>
					{/if}
				</div>

				<div class="alert-metrics">
					<SignalBadge signal={alert.windowSignal} type="window" />
					<span class="variant">{alert.variant.includes('/') ? alert.variant : alert.variant + '/0'}</span>
					<span class="right-group">
						<span class="roi">{alert.roi}c</span>
						<SignalBadge signal={alert.signal} />
					</span>
				</div>

				{#if alert.trendUnavailable}
					<div class="trend-divider"></div>
					<span class="trend-unavailable">trend data unavailable</span>
				{:else if alert.priceTrend.length > 1}
					<div class="trend-divider"></div>
					<table class="trend-table"><tbody>
						<tr>
							<td class="trend-label">ROI</td>
							{#each alert.priceTrend as p, i}
								{#if i > 0}<td class="trend-arrow">→</td>{/if}
								<td class="trend-val price">{p}c</td>
							{/each}
						</tr>
						{#if alert.listingsTrend.length > 1}
							<tr>
								<td class="trend-label">listings</td>
								{#each alert.listingsTrend as l, i}
									{#if i > 0}<td class="trend-arrow">→</td>{/if}
									<td class="trend-val listing">{l}</td>
								{/each}
							</tr>
						{/if}
						{#if alert.baseListingsTrend.length > 1}
							<tr>
								<td class="trend-label">base listings</td>
								{#each alert.baseListingsTrend as l, i}
									{#if i > 0}<td class="trend-arrow">→</td>{/if}
									<td class="trend-val base">{l}</td>
								{/each}
							</tr>
						{/if}
					</tbody></table>
				{/if}

				{#if alert.action}
					<div class="alert-action">{alert.action}</div>
				{/if}
			</div>
		{/each}
		</div>
	</section>
{/if}

<style>
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 28px;
		margin-bottom: 32px;
	}
	.section-title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-yellow);
		margin: 0 0 18px 0;
	}
	.alerts-grid {
		display: grid;
		grid-template-columns: 1fr 1fr 1fr;
		gap: 14px;
	}
	.alert-card {
		border: 1px solid var(--color-lab-border);
		padding: 14px 16px;
		background: var(--color-lab-bg);
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.alert-name {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.gem-name {
		color: var(--color-lab-text);
		font-weight: 700;
	}
	.tier-badge {
		font-size: 0.6875rem;
		font-weight: 700;
		padding: 1px 6px;
		border-radius: 3px;
		margin-left: auto;
	}
	.tier-top { background: rgba(234, 179, 8, 0.2); color: #eab308; }
	.tier-high { background: rgba(251, 146, 60, 0.2); color: #fb923c; }
	.tier-mid-high { background: rgba(192, 132, 252, 0.2); color: #c084fc; }
	.tier-mid { background: rgba(148, 163, 184, 0.2); color: #94a3b8; }
	.tier-low { background: rgba(100, 116, 139, 0.15); color: #64748b; }
	.tier-floor { background: rgba(71, 85, 105, 0.1); color: #475569; }

	.alert-metrics {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.roi {
		color: var(--color-lab-green);
		font-weight: 700;
		font-size: 1.125rem;
	}
	.right-group {
		display: flex;
		align-items: center;
		gap: 14px;
		margin-left: auto;
	}
	.variant {
		color: var(--color-lab-text-secondary);
	}

	.trend-table {
		border-collapse: collapse;
		font-size: 1rem;
		width: auto;
	}
	.trend-table td {
		padding: 2px 0;
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.trend-label {
		text-align: left !important;
		color: var(--color-lab-text-secondary);
		font-weight: 600;
		padding-right: 6px !important;
		white-space: nowrap;
	}
	.trend-arrow {
		text-align: center !important;
		color: var(--color-lab-text-secondary);
		opacity: 0.4;
		padding: 2px 3px !important;
	}
	.trend-val.price {
		color: var(--color-lab-green);
		font-weight: 600;
	}
	.trend-val.listing {
		color: var(--color-lab-blue);
		font-weight: 600;
	}
	.trend-val.base {
		color: var(--color-lab-text-secondary);
		font-weight: 600;
	}

	.trend-unavailable {
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
		opacity: 0.6;
	}
	.trend-divider {
		border-bottom: 1px solid rgba(42, 45, 55, 0.4);
		padding-bottom: 4px;
	}
	.alert-action {
		font-size: 1rem;
		color: var(--color-lab-text-secondary);
		padding-top: 6px;
		border-top: 1px solid rgba(42, 45, 55, 0.4);
	}
</style>
