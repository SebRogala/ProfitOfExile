<script lang="ts">
	import type { WindowAlert } from '$lib/api';
	import SignalBadge from './SignalBadge.svelte';
	import GemIcon from './GemIcon.svelte';

	let { alerts }: { alerts: WindowAlert[] } = $props();
</script>

{#if alerts.length > 0}
	<section class="section">
		<h2 class="section-title">Window Alerts</h2>
		<div class="alerts-grid">
		{#each alerts as alert}
			<div class="alert-row">
				<div class="alert-top">
					<SignalBadge signal={alert.windowSignal} type="window" />
					<GemIcon name={alert.name} size={24} />
					<span class="gem-name">{alert.name}</span>
					<span class="variant">({alert.variant})</span>
					{#if alert.roi > 0}<span class="price">{alert.roi}c</span>{/if}
				</div>
				<div class="alert-stats">
					<span class="stat">{alert.transListings} listings</span>
					{#if alert.priceVelocity !== 0}
						<span class="stat" class:stat-up={alert.priceVelocity > 0} class:stat-down={alert.priceVelocity < 0}>
							Price: {alert.priceVelocity > 0 ? '+' : ''}{alert.priceVelocity}c/h
						</span>
					{/if}
					{#if alert.baseVelocity !== 0}
						<span class="stat" class:stat-up={alert.baseVelocity < 0} class:stat-down={alert.baseVelocity > 0}>
							Base: {alert.baseVelocity > 0 ? '+' : ''}{alert.baseVelocity}/h
						</span>
					{/if}
					{#if alert.signal}
						<SignalBadge signal={alert.signal} />
					{/if}
					<span class="liq" title="Base gem liquidity">{alert.liquidityTier} liq</span>
				</div>
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
		grid-template-columns: 1fr 1fr;
		gap: 14px;
	}
	.alert-row {
		border: 1px solid var(--color-lab-border);
		padding: 16px 18px;
		background: var(--color-lab-bg);
	}
	.alert-top {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
		margin-bottom: 10px;
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
	.price {
		color: var(--color-lab-green);
		font-weight: 700;
		font-size: 1rem;
		margin-left: auto;
	}
	.alert-stats {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
		margin-bottom: 8px;
	}
	.stat {
		font-size: 0.875rem;
		color: var(--color-lab-text-secondary);
	}
	.stat-up {
		color: var(--color-lab-green);
		font-weight: 600;
	}
	.stat-down {
		color: var(--color-lab-red);
		font-weight: 600;
	}
	.liq {
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--color-lab-yellow);
		margin-left: auto;
	}
	.alert-action {
		font-size: 0.875rem;
		color: var(--color-lab-text-secondary);
		font-style: italic;
		padding-top: 6px;
		border-top: 1px solid rgba(42, 45, 55, 0.4);
	}
</style>
