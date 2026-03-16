<script lang="ts">
	import type { WindowAlert } from '$lib/api';
	import SignalBadge from './SignalBadge.svelte';
	import GemIcon from './GemIcon.svelte';

	let { alerts }: { alerts: WindowAlert[] } = $props();

	function velocityStr(v: number): string {
		if (v > 0) return `↑${v}`;
		if (v < 0) return `↓${Math.abs(v)}`;
		return '0';
	}
</script>

{#if alerts.length > 0}
	<section class="section">
		<h2 class="section-title">Window Alerts</h2>
		{#each alerts as alert}
			<div class="alert-row">
				<div class="alert-main">
					<SignalBadge signal={alert.windowSignal} type="window" />
					<GemIcon name={alert.name} size={24} />
					<span class="gem-name">{alert.name}</span>
					<span class="variant">({alert.variant})</span>
					{#if alert.roi > 0}<span class="roi">{alert.roi}c</span>{/if}
					<span class="meta">{alert.transListings} listings</span>
					<span class="liq">{alert.liquidityTier}</span>
				</div>
				<div class="alert-detail">
					<span class="action">{alert.action}</span>
					{#if alert.history.length > 0}
						<span class="history">
							History: {alert.history.map(h => `${h.time} ${h.from}→${h.to}`).join(' → ')}
						</span>
					{/if}
				</div>
			</div>
		{/each}
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
	.alert-row {
		border-bottom: 1px solid var(--color-lab-border);
		padding: 14px 0;
	}
	.alert-row:last-child {
		border-bottom: none;
	}
	.alert-main {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}
	.gem-name {
		color: var(--color-lab-text);
		font-weight: 700;
		font-size: 1rem;
	}
	.variant {
		color: var(--color-lab-text-secondary);
		font-size: 0.9375rem;
	}
	.roi {
		color: var(--color-lab-green);
		font-weight: 700;
		font-size: 1rem;
	}
	.meta {
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
	}
	.liq {
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--color-lab-yellow);
	}
	.alert-detail {
		margin-top: 8px;
		padding-left: 10px;
		display: flex;
		gap: 18px;
		flex-wrap: wrap;
	}
	.action {
		color: var(--color-lab-text);
		font-size: 0.9375rem;
	}
	.history {
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
	}
</style>
