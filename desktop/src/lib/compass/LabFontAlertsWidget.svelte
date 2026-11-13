<script lang="ts">
	/**
	 * Divine Font craft alerts (POE-328): one panel per alert craft the current
	 * font read offers, stacked in the order Rust lists them. Display-only — no
	 * hot rects, so clicks over the stack reach the game.
	 */
	import { invoke } from '@tauri-apps/api/core';
	import { formatPrice, setDivineRate } from '$lib/price.svelte';
	import {
		alertPanels,
		exceptionalView,
		type AlertPanel,
		type ExceptionalSupportsPayload,
	} from '$lib/compass/font-alerts-view';
	import GemIcon from '../../routes/(app)/components/GemIcon.svelte';

	let { configMode, label }: { configMode: boolean; label: string } = $props();

	const ALERTS_POLL_MS = 500;
	const PRICES_POLL_MS = 5000;

	let panels = $state<AlertPanel[]>([]);
	let prices = $state<ExceptionalSupportsPayload | null>(null);
	let nowMs = $state(Date.now());

	const view = $derived(exceptionalView(prices, nowMs, formatPrice));

	async function pollAlerts() {
		nowMs = Date.now();
		try {
			panels = alertPanels(await invoke<string[]>('get_font_alerts'));
		} catch (e) {
			console.warn('[overlay] get_font_alerts failed:', e);
		}
	}

	async function pollPrices() {
		try {
			prices = await invoke<ExceptionalSupportsPayload | null>('get_exceptional_support_prices');
			// Separate WebView: the rate rides the payload (only the comparator feeds it otherwise).
			setDivineRate(prices?.divineRate ?? 0);
		} catch (e) {
			console.warn('[overlay] get_exceptional_support_prices failed:', e);
		}
	}

	// Overlay onMount is unreliable; poll from an effect with cleanup.
	$effect(() => {
		pollAlerts();
		pollPrices();
		const alerts = setInterval(pollAlerts, ALERTS_POLL_MS);
		const priceTimer = setInterval(pollPrices, PRICES_POLL_MS);
		return () => {
			clearInterval(alerts);
			clearInterval(priceTimer);
		};
	});
</script>

<div class="root">
	{#if panels.length > 0}
		<div class="stack">
			{#each panels as panel (panel)}
				{#if panel === 'exceptional'}
					<section class="panel exceptional" aria-label="Exceptional support craft detected">
						<div class="head">
							<div class="title">Exceptional support</div>
							<div class="sec small">Font option</div>
						</div>
						<div class="sec small sub">Gives one of these at random</div>
						<div>
							{#each view.rows as row (row.name)}
								<div class="row">
									<GemIcon name={row.name} size={22} />
									<span class="name">{row.name}</span>
									{#if row.cell.kind === 'price'}
										<span class="price">{row.cell.text}</span>
									{:else if row.cell.kind === 'loading'}
										<span class="nil">pricing…</span>
									{:else}
										<span class="nil">no price</span>
									{/if}
								</div>
							{/each}
						</div>
						<div class="footer">
							<span class="sec">{view.footer}</span>
							{#if view.age}
								{#if view.age.stale}
									<span class="badge">{view.age.text}</span>
								{:else}
									<span class="sec">{view.age.text}</span>
								{/if}
							{/if}
						</div>
					</section>
				{:else}
					<section class="panel transfigure" aria-label="Transfigure craft detected">
						<div class="head">
							<div class="title">Transfigure gem</div>
							<div class="sec small">Font option</div>
						</div>
						<div class="sec small option">Transform a non-Transfigured Skill Gem to a Transfigured version</div>
						<div class="note">You choose the version</div>
					</section>
				{/if}
			{/each}
		</div>
	{:else if configMode}
		<p class="placeholder">{label}</p>
	{/if}
</div>

<style>
	.root {
		pointer-events: none;
	}

	.stack {
		display: flex;
		flex-direction: column;
		gap: 6px;
		width: 260px;
	}

	.panel {
		box-sizing: border-box;
		width: 260px;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
		font-size: 12px;
		color: var(--color-lab-text);
		background: rgba(15, 17, 23, 0.92);
		border: 1px solid rgba(42, 45, 55, 0.8);
		border-radius: 8px;
		backdrop-filter: blur(8px);
		box-shadow: 0 6px 24px rgba(0, 0, 0, 0.5);
	}

	.exceptional {
		padding: 8px 10px 7px;
	}

	.transfigure {
		padding: 8px 10px 8px;
	}

	.head {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: 8px;
	}

	.title {
		font-size: 14px;
		font-weight: 700;
	}

	.sec {
		color: var(--color-lab-text-secondary);
	}

	.small {
		font-size: 11px;
	}

	.sub {
		margin: 1px 0 4px;
	}

	.row {
		box-sizing: border-box;
		display: grid;
		grid-template-columns: 22px minmax(0, 1fr) auto;
		align-items: center;
		gap: 8px;
		height: 30px;
		border-bottom: 1px solid rgba(42, 45, 55, 0.4);
	}

	.row:last-child {
		border-bottom: none;
	}

	.name {
		font-size: 13px;
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.price {
		font-family: 'Consolas', 'Monaco', monospace;
		font-size: 14px;
		font-weight: 700;
		color: var(--warning);
	}

	.nil {
		font-size: 11px;
		font-style: italic;
		color: var(--color-lab-text-secondary);
	}

	.footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		margin-top: 5px;
		font-size: 11px;
	}

	.badge {
		display: inline-flex;
		align-items: center;
		font-size: 11px;
		line-height: 16px;
		padding: 0 6px;
		border-radius: 8px;
		white-space: nowrap;
		background: rgba(234, 179, 8, 0.14);
		color: var(--color-lab-yellow);
	}

	.option {
		margin-top: 1px;
		line-height: 15px;
	}

	.note {
		font-size: 12px;
		margin-top: 4px;
	}

	.placeholder {
		padding: 4px 8px;
		font-size: 11px;
		color: var(--color-lab-text-muted);
	}
</style>
