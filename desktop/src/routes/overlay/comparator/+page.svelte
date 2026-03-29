<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';
	import type { CompareGem } from '$lib/api';
	import type { TradeLookupResult } from '$lib/tradeApi';


	// --- Comparator data ---
	const SIGNAL_COLORS: Record<string, string> = {
		STABLE: '#5eead4', UNCERTAIN: '#9ca3af', HERD: '#eab308',
		DUMPING: '#ef4444', RECOVERY: '#a855f7', TRAP: '#ef4444',
	};

	const TIER_COLORS: Record<string, string> = {
		TOP: '#fbbf24', HIGH: '#fb923c', 'MID-HIGH': '#c084fc',
		MID: '#94a3b8', LOW: '#64748b', FLOOR: '#475569',
	};

	const REC_COLORS: Record<string, { color: string; bg: string }> = {
		BEST: { color: '#22c55e', bg: 'rgba(34, 197, 94, 0.15)' },
		OK: { color: '#94a3b8', bg: 'rgba(148, 163, 184, 0.1)' },
		AVOID: { color: '#ef4444', bg: 'rgba(239, 68, 68, 0.15)' },
	};

	const MOCK_RESULTS: CompareGem[] = [
		{ name: 'Kinetic Blast of Clustering', variant: '20/20', color: 'GREEN', roi: 1840, roiPercent: 920, weightedRoi: 1750, signal: 'STABLE', cv: 0.08, transListings: 12, transVelocity: 3.2, baseListings: 45, baseVelocity: 8.1, basePrice: 200, transPrice: 1940, liquidityTier: 'LIQUID', windowSignal: 'STABLE', sparkline: [1900, 1920, 1940, 1960, 1940], signalHistory: [], recommendation: 'BEST', priceTier: 'TOP', tierAction: 'Sell now', sellUrgency: 'NOW', sellReason: 'High demand', sellability: 75, sellabilityLabel: 'SAFE', low7d: 1800, high7d: 2100, histPosition: 0.7, sellConfidence: 'HIGH', sellConfidenceReason: 'Stable price', quickSellPrice: 1870, riskAdjustedPrice: 1869 } as CompareGem,
		{ name: 'Cyclone of Tumult', variant: '20/20', color: 'RED', roi: 1605, roiPercent: 803, weightedRoi: 1500, signal: 'UNCERTAIN', cv: 0.15, transListings: 8, transVelocity: 1.8, baseListings: 30, baseVelocity: 5.4, basePrice: 100, transPrice: 1705, liquidityTier: 'MODERATE', windowSignal: 'UNCERTAIN', sparkline: [1650, 1700, 1720, 1690, 1705], signalHistory: [], recommendation: 'OK', priceTier: 'TOP', tierAction: 'Monitor', sellUrgency: 'WATCH', sellReason: 'Uncertain signal', sellability: 55, sellabilityLabel: 'FAIR', low7d: 1500, high7d: 1800, histPosition: 0.6, sellConfidence: 'MEDIUM', sellConfidenceReason: 'Price fluctuating', quickSellPrice: 1580, riskAdjustedPrice: 1584 } as CompareGem,
		{ name: 'Vaal Grace of Phasing', variant: '20/20', color: 'GREEN', roi: 320, roiPercent: 160, weightedRoi: 290, signal: 'UNCERTAIN', cv: 0.18, transListings: 5, transVelocity: 1.1, baseListings: 20, baseVelocity: 3.2, basePrice: 80, transPrice: 420, liquidityTier: 'THIN', windowSignal: 'UNCERTAIN', sparkline: [400, 410, 430, 415, 420], signalHistory: [], recommendation: 'OK', priceTier: 'MID-HIGH', tierAction: 'Monitor', sellUrgency: 'WATCH', sellReason: 'Low liquidity', sellability: 40, sellabilityLabel: 'RISKY', low7d: 350, high7d: 480, histPosition: 0.45, sellConfidence: 'LOW', sellConfidenceReason: 'Thin market', quickSellPrice: 370, riskAdjustedPrice: 365 } as CompareGem,
	];

	let results = $state<CompareGem[]>(MOCK_RESULTS);
	let selectedGem = $state<string | null>(null);
	let serverUrl = $state('https://poe.softsolution.pro');

	// Trade data per gem
	let tradeData = $state<Record<string, TradeLookupResult>>({});
	let tradeLoading = $state<Record<string, boolean>>({});
	let tradeError = $state<Record<string, string>>({});

	$effect(() => {
		if (results.length > 0 && !selectedGem) {
			const best = results.find((g) => g.recommendation === 'BEST');
			selectedGem = best?.name ?? results[0]?.name ?? null;
		}
	});

	function formatPrice(chaos: number): string {
		if (chaos >= 1000) return `${(chaos / 1000).toFixed(1)}k`;
		return `${Math.round(chaos)}c`;
	}

	// Cache-only trade lookup via Go server
	async function fetchTradeCache(gem: CompareGem) {
		try {
			const resp = await fetch(`${serverUrl}/api/trade/lookup`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ gem: gem.name, variant: gem.variant, cacheOnly: true }),
			});
			if (resp.status === 200) {
				tradeData[gem.name] = await resp.json();
			}
			// 204 = cache miss, silently skip
		} catch (_) {}
	}

	// Full trade lookup — try Go server first, fall back to Tauri direct GGG call
	async function fetchTrade(gem: CompareGem) {
		tradeLoading[gem.name] = true;
		tradeError[gem.name] = '';
		try {
			const resp = await fetch(`${serverUrl}/api/trade/lookup`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ gem: gem.name, variant: gem.variant }),
			});
			if (resp.status === 200) {
				tradeData[gem.name] = await resp.json();
				tradeLoading[gem.name] = false;
				return;
			}
			if (resp.status === 202) {
				// Queued — poll for result
				for (let i = 0; i < 10; i++) {
					await new Promise(r => setTimeout(r, 3000));
					const poll = await fetch(`${serverUrl}/api/trade/lookup`, {
						method: 'POST',
						headers: { 'Content-Type': 'application/json' },
						body: JSON.stringify({ gem: gem.name, variant: gem.variant }),
					});
					if (poll.status === 200) {
						tradeData[gem.name] = await poll.json();
						tradeLoading[gem.name] = false;
						return;
					}
				}
			}
			// Fallback to direct Tauri GGG call
			const result = await invoke<TradeLookupResult>('trade_lookup', {
				gem: gem.name, variant: gem.variant,
			});
			tradeData[gem.name] = result;
		} catch (e) {
			tradeError[gem.name] = String(e);
		} finally {
			tradeLoading[gem.name] = false;
		}
	}

	// Auto-fetch cache for all gems when results arrive
	function fetchAllTradeCache() {
		for (const gem of results) {
			if (!tradeData[gem.name]) {
				fetchTradeCache(gem);
			}
		}
	}

	function handlePick() {
		if (!selectedGem) return;
		const gem = results.find((g) => g.name === selectedGem);
		if (gem) {
			getCurrentWebviewWindow().emit('overlay-pick', {
				name: gem.name, variant: gem.variant, roi: gem.roi,
			});
		}
	}

	function handleClear() {
		results = [];
		selectedGem = null;
		tradeData = {};
		tradeLoading = {};
		tradeError = {};
	}

	onMount(async () => {
		// Get server URL from Rust state
		try {
			const status = await invoke<{ server_url: string }>('get_status');
			serverUrl = status.server_url;
		} catch (_) {}

		// Auto-fetch cache for mock data
		fetchAllTradeCache();

		const unlistenResults = await listen<CompareGem[]>('comparator-results', (event) => {
			results = event.payload;
			selectedGem = null;
			tradeData = {};
			tradeLoading = {};
			tradeError = {};
			fetchAllTradeCache();
		});

		const unlistenClear = await listen('gems-cleared', () => {
			handleClear();
		});

		return () => {
			unlistenResults();
			unlistenClear();
		};
	});
</script>

<div class="surface">
	{#if results.length > 0}
		<div class="overlay">
			{#each results as gem (gem.name)}
				{@const rec = REC_COLORS[gem.recommendation] ?? REC_COLORS.OK}
				{@const tierColor = TIER_COLORS[gem.priceTier] ?? '#94a3b8'}
				{@const sigColor = SIGNAL_COLORS[gem.signal] ?? '#9ca3af'}
				{@const trade = tradeData[gem.name]}
				{@const loading = tradeLoading[gem.name]}
				<button
					class="gem-row"
					class:selected={selectedGem === gem.name}
					onclick={() => selectedGem = gem.name}
				>
					<div class="gem-header">
						<span class="tier" style="color: {tierColor}">{gem.priceTier}</span>
						<span class="gem-name" style="color: {rec.color}">{gem.name}</span>
					</div>
					<div class="gem-details">
						<span class="price">{formatPrice(gem.transPrice)}</span>
						<span class="range">7d: {formatPrice(gem.low7d)}\u2013{formatPrice(gem.high7d)}</span>
						<span class="sell" style="color: {sigColor}">{gem.signal}</span>
						<span class="rec" style="color: {rec.color}; background: {rec.bg}">{gem.recommendation}</span>
					</div>
					{#if trade}
						<div class="trade-row">
							<span class="trade-floor">{formatPrice(trade.priceFloor)}</span>
							<span class="trade-sep">\u2013</span>
							<span class="trade-ceil">{formatPrice(trade.priceCeiling)}</span>
							<span class="trade-total">({trade.total})</span>
							{#if trade.signals.sellerConcentration !== 'NORMAL'}
								<span class="trade-warn">{trade.signals.sellerConcentration}</span>
							{/if}
							<span class="trade-listings-inline">
								{#each trade.listings.slice(0, 5) as listing}
									<span class="listing">{formatPrice(listing.chaosPrice)}{listing.corrupted ? '*' : ''}</span>
								{/each}
							</span>
						</div>
					{:else if tradeError[gem.name]}
						<div class="trade-row trade-err">{tradeError[gem.name]}</div>
					{:else}
						<div class="trade-row">
							<button class="trade-btn" onclick={(e) => { e.stopPropagation(); fetchTrade(gem); }} disabled={loading}>
								{loading ? 'loading...' : 'fetch trade'}
							</button>
						</div>
					{/if}
				</button>
			{/each}

			<div class="actions">
				<button class="btn pick" onclick={handlePick} disabled={!selectedGem}>
					Pick: {selectedGem ?? '\u2014'}
				</button>
				<button class="btn clear" onclick={handleClear}>Clear</button>
			</div>
		</div>
	{:else}
		<div class="overlay empty">
			<span class="waiting">Waiting for gems...</span>
		</div>
	{/if}
</div>

<style>
	:global(html), :global(body) {
		margin: 0;
		padding: 0;
		background: transparent !important;
		overflow: hidden;
	}

	.surface {
		width: 100vw;
		height: 100vh;
		background: transparent;
		position: relative;
		box-sizing: border-box;
	}

	.overlay {
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
		background: rgba(15, 17, 23, 0.92);
		border: 1px solid rgba(42, 45, 55, 0.8);
		border-radius: 8px;
		padding: 8px;
		color: #e4e4e7;
		font-size: 12px;
		backdrop-filter: blur(8px);
	}

	.overlay.empty {
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 40px;
	}

	.waiting {
		color: #9ca3af;
		font-style: italic;
	}

	.gem-row {
		all: unset;
		display: block;
		width: 100%;
		padding: 5px 8px;
		border-radius: 4px;
		cursor: pointer;
		border: 1px solid transparent;
		margin-bottom: 3px;
		box-sizing: border-box;
	}

	.gem-row:hover {
		background: rgba(255, 255, 255, 0.04);
	}

	.gem-row.selected {
		background: rgba(34, 197, 94, 0.08);
		border-color: rgba(34, 197, 94, 0.25);
	}

	.gem-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 1px;
	}

	.gem-name {
		font-weight: 600;
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
		margin-right: 8px;
	}

	.tier {
		font-weight: 700;
		font-size: 10px;
		flex-shrink: 0;
		width: 52px;
		text-align: center;
		margin-right: 6px;
	}

	.gem-details {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 11px;
	}

	.price {
		font-weight: 600;
		color: #fbbf24;
	}

	.range {
		color: #9ca3af;
		font-size: 10px;
	}

	.sell {
		font-weight: 500;
	}

	.rec {
		font-size: 10px;
		font-weight: 700;
		padding: 1px 5px;
		border-radius: 3px;
		flex-shrink: 0;
	}

	/* Trade results */
	.trade-row {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 10px;
		margin-top: 2px;
		color: #9ca3af;
	}

	.trade-floor {
		color: #22c55e;
		font-weight: 600;
	}

	.trade-sep {
		color: #4b5563;
	}

	.trade-ceil {
		color: #fbbf24;
		font-weight: 600;
	}

	.trade-total {
		color: #6b7280;
	}

	.trade-warn {
		color: #ef4444;
		font-weight: 600;
		font-size: 9px;
	}

	.trade-err {
		color: #ef4444;
		font-size: 9px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.trade-listings-inline {
		display: inline-flex;
		gap: 4px;
		margin-left: 4px;
	}

	.listing {
		color: #6b7280;
	}

	.trade-btn {
		all: unset;
		cursor: pointer;
		font-size: 9px;
		color: #3b82f6;
		padding: 0 4px;
		border-radius: 2px;
	}

	.trade-btn:hover:not(:disabled) {
		color: #60a5fa;
		background: rgba(59, 130, 246, 0.1);
	}

	.trade-btn:disabled {
		color: #6b7280;
		cursor: default;
	}

	/* Actions */
	.actions {
		display: flex;
		gap: 6px;
		margin-top: 4px;
		padding-top: 4px;
		border-top: 1px solid rgba(42, 45, 55, 0.6);
	}

	.btn {
		all: unset;
		cursor: pointer;
		padding: 4px 10px;
		border-radius: 4px;
		font-size: 11px;
		font-weight: 600;
		text-align: center;
	}

	.btn.pick {
		flex: 1;
		background: rgba(34, 197, 94, 0.15);
		color: #22c55e;
		border: 1px solid rgba(34, 197, 94, 0.3);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.btn.pick:hover:not(:disabled) {
		background: rgba(34, 197, 94, 0.25);
	}

	.btn.pick:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.btn.clear {
		background: rgba(239, 68, 68, 0.1);
		color: #ef4444;
		border: 1px solid rgba(239, 68, 68, 0.2);
	}

	.btn.clear:hover {
		background: rgba(239, 68, 68, 0.2);
	}
</style>
