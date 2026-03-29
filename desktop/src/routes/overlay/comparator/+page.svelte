<script lang="ts">
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';
	import type { CompareGem } from '$lib/api';
	import type { TradeLookupResult } from '$lib/tradeApi';
	import GemIcon from '../../(app)/components/GemIcon.svelte';

	const TRADE_STALE_MS = 2 * 60 * 1000;

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

	// Trade data — received from main Comparator, not fetched separately
	let tradeData = $state<Record<string, TradeLookupResult | null>>({});

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

	function tradeCacheAge(trade: TradeLookupResult): string {
		if (!trade.fetchedAt) return '';
		const ms = Date.now() - new Date(trade.fetchedAt).getTime();
		const mins = Math.floor(ms / 60000);
		if (mins < 1) return '<1m';
		if (mins < 60) return `${mins}m`;
		return `${Math.floor(mins / 60)}h${mins % 60}m`;
	}

	function isTradeCacheStale(trade: TradeLookupResult): boolean {
		if (!trade.fetchedAt) return true;
		return Date.now() - new Date(trade.fetchedAt).getTime() >= TRADE_STALE_MS;
	}

	function listingAge(indexedAt: string): string {
		if (!indexedAt) return '';
		const ms = Date.now() - new Date(indexedAt).getTime();
		const mins = Math.floor(ms / 60000);
		if (mins < 60) return `${mins}m`;
		const hrs = Math.floor(mins / 60);
		if (hrs < 24) return `${hrs}h`;
		return `${Math.floor(hrs / 24)}d`;
	}

	// Trade refresh — calls Tauri directly
	async function requestTradeRefresh(gem: CompareGem) {
		try {
			const result = await invoke<TradeLookupResult>('trade_lookup', {
				gem: gem.name, variant: gem.variant,
			});
			tradeData = { ...tradeData, [gem.name]: result };
		} catch (e) {
			console.error('Trade lookup failed:', e);
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
	}

	onMount(async () => {

		const unlistenResults = await listen<{ results: CompareGem[]; tradeData: Record<string, TradeLookupResult | null> }>('comparator-results', (event) => {
			results = event.payload.results;
			tradeData = event.payload.tradeData ?? {};
			selectedGem = null;
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
		<div class="layout">
			<div class="table">
				{#each results as gem (gem.name)}
					{@const rec = REC_COLORS[gem.recommendation] ?? REC_COLORS.OK}
					{@const tierColor = TIER_COLORS[gem.priceTier] ?? '#94a3b8'}
					{@const sigColor = SIGNAL_COLORS[gem.signal] ?? '#9ca3af'}
					{@const trade = tradeData[gem.name]}
					<div class="gem-row" class:selected={selectedGem === gem.name}>
						<div class="row-top">
							<GemIcon name={gem.name} size={24} />
							<span class="gem-name" style="color: {rec.color}">{gem.name}</span>
							<span class="rec" style="color: {rec.color}; background: {rec.bg}">{gem.recommendation}</span>
							<span class="price">{formatPrice(gem.transPrice)}</span>
							<span class="sell" style="color: {sigColor}">{gem.signal}</span>
							<span class="range">wk {formatPrice(gem.low7d)}–{formatPrice(gem.high7d)}</span>
							<span class="tier-age">
								<span class="tier" style="color: {tierColor}">{gem.priceTier}</span>
								{#if trade}
									<span class="cache-age" class:stale={isTradeCacheStale(trade)}>{tradeCacheAge(trade)}</span>
								{/if}
							</span>
						</div>
						<div class="row-bottom">
							{#if trade}
								{#if trade.signals.sellerConcentration !== 'NORMAL'}
									<span class="trade-warn">{trade.signals.sellerConcentration}</span>
								{/if}
								{#each trade.listings as listing, i}
									<span class="listing-col" class:first={i === 0} class:corrupted={listing.corrupted}>
										<span class="listing-price">{formatPrice(listing.chaosPrice)}</span>
										<span class="listing-age">{listingAge(listing.indexedAt)}</span>
									</span>
								{/each}
								<span class="trade-total">({trade.total})</span>
							{:else}
								<span class="trade-nodata">no trade data</span>
							{/if}
						</div>
					</div>
				{/each}
			</div>
			<div class="side">
				{#each results as gem, i (gem.name)}
					<div class="side-row">
						<button class="act-btn pick-btn" class:active={selectedGem === gem.name} onclick={() => { selectedGem = gem.name; handlePick(); }} title="Pick">&#x2713;</button>
						<button class="act-btn" onclick={() => requestTradeRefresh(gem)} title="Refresh trade">&#x21BB;</button>
					</div>
				{/each}
				<button class="clear-act" onclick={handleClear}>clear</button>
			</div>
		</div>
	{:else}
		<div class="table empty">
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
		background: transparent;
	}

	.layout {
		display: flex;
		gap: 6px;
		align-items: flex-start;
	}

	.table {
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
		background: rgba(15, 17, 23, 0.92);
		border: 1px solid rgba(42, 45, 55, 0.8);
		border-radius: 8px;
		padding: 0 10px;
		color: #e4e4e7;
		font-size: 14px;
		backdrop-filter: blur(8px);
		display: inline-block;
		pointer-events: none;
	}

	.table.empty {
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 40px;
		padding: 10px;
	}

	.waiting {
		color: #9ca3af;
		font-style: italic;
	}

	/* --- Gem rows --- */
	.gem-row {
		padding: 8px 0;
		border-bottom: 1px solid rgba(42, 45, 55, 0.4);
	}

	.gem-row:last-of-type {
		border-bottom: none;
	}

	.gem-row.selected {
		margin: 0 -10px;
		padding: 8px 10px;
		background: rgba(34, 197, 94, 0.08);
		border-radius: 4px;
	}

	.row-top {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.row-bottom {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		margin-top: 3px;
		color: #9ca3af;
	}

	.gem-name {
		font-weight: 600;
		font-size: 14px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.tier-age {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		flex-shrink: 0;
		margin-left: auto;
	}

	.tier {
		font-weight: 700;
		font-size: 11px;
	}

	.cache-age {
		font-size: 9px;
		color: #6b7280;
	}

	.cache-age.stale {
		color: #ef4444;
	}

	.price {
		font-weight: 700;
		color: #fbbf24;
		flex-shrink: 0;
	}

	.range {
		color: #9ca3af;
		font-size: 12px;
		flex-shrink: 0;
	}

	.sell {
		font-weight: 600;
		flex-shrink: 0;
	}

	.rec {
		font-size: 11px;
		font-weight: 700;
		padding: 2px 6px;
		border-radius: 3px;
		flex-shrink: 0;
	}

	/* --- Trade row --- */
	.trade-total {
		color: #6b7280;
		flex-shrink: 0;
	}

	.trade-warn {
		color: #ef4444;
		font-weight: 700;
		flex-shrink: 0;
	}

	.trade-nodata {
		color: #4b5563;
		font-size: 11px;
		font-style: italic;
	}

	.listing-col {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 0 5px;
		border-right: 1px solid #2a2d37;
	}

	.listing-col:last-of-type {
		border-right: none;
	}

	.listing-price {
		color: #e4e4e7;
		font-size: 12px;
	}

	.listing-col.first .listing-price {
		color: #22c55e;
		font-weight: 600;
	}

	.listing-col.corrupted .listing-price {
		color: #ef4444;
	}

	.listing-age {
		color: #6b7280;
		font-size: 9px;
	}

	/* --- Side buttons --- */
	.side {
		display: flex;
		flex-direction: column;
		align-items: center;
		pointer-events: auto;
	}

	.side-row {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 2px;
		min-height: 60px;
		margin-bottom: 4px;
	}

	.act-btn {
		all: unset;
		cursor: pointer;
		width: 26px;
		height: 26px;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 4px;
		font-size: 14px;
		background: rgba(15, 17, 23, 0.8);
		color: #9ca3af;
		border: 1px solid rgba(42, 45, 55, 0.6);
	}

	.act-btn:hover {
		background: rgba(255, 255, 255, 0.15);
		color: #e4e4e7;
	}

	.pick-btn.active {
		color: #22c55e;
		border-color: rgba(34, 197, 94, 0.4);
		background: rgba(34, 197, 94, 0.15);
	}

	.clear-act {
		all: unset;
		cursor: pointer;
		font-size: 11px;
		color: #9ca3af;
		padding: 4px 8px;
		border-radius: 4px;
		margin-top: 8px;
		background: rgba(15, 17, 23, 0.8);
		border: 1px solid rgba(42, 45, 55, 0.6);
	}

	.clear-act:hover {
		color: #ef4444;
		border-color: rgba(239, 68, 68, 0.4);
	}
</style>
