<script lang="ts">
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import type { CompareGem } from '$lib/api';
	import type { TradeLookupResult, TradeQueueEvent, TradeQueueDisplay } from '$lib/tradeApi';
	import GemIcon from '../../(app)/components/GemIcon.svelte';

	// Staleness thresholds — read from polled Rust status, with sensible defaults
	let tradeStaleWarnSecs = $state(120);
	let tradeStaleCriticalSecs = $state(600);

	// Cache the monitor scale factor from Tauri (more reliable than window.devicePixelRatio
	// which can be wrong in transparent overlay WebViews on high-DPI displays).
	let cachedScaleFactor = $state(0);
	getCurrentWebviewWindow().scaleFactor()
		.then(sf => { cachedScaleFactor = sf; })
		.catch(e => {
			console.warn('[overlay] scaleFactor() failed, using devicePixelRatio fallback:', e);
			cachedScaleFactor = window.devicePixelRatio || 1;
		});

	const SIGNAL_COLORS: Record<string, string> = {
		STABLE: '#5eead4', UNCERTAIN: '#9ca3af', HERD: '#eab308',
		DUMPING: '#ef4444', RECOVERY: '#a855f7', DEMAND: '#22c55e', CAUTION: '#eab308',
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

	let results = $state<CompareGem[]>([]);
	let selectedGem = $state<string | null>(null);

	// Trade data + loading/error state — received from main Comparator, not fetched separately
	let tradeData = $state<Record<string, TradeLookupResult | null>>({});
	let tradeLoading = $state<Record<string, boolean>>({});
	let tradeError = $state<Record<string, boolean>>({});

	$effect(() => {
		if (results.length > 0 && !selectedGem) {
			// Default to most expensive — Comparator handles signal-aware scoring
			let best = results[0];
			for (const g of results) {
				if (g.transPrice > best.transPrice) best = g;
			}
			selectedGem = best.name;
		}
		// Tell the mouse hook whether we have content — when empty, clicks pass through to game.
		invoke('set_overlay_has_content', { hasContent: results.length > 0 })
			.catch(e => console.warn('[overlay] set_overlay_has_content failed:', e));
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

	function tradeStaleness(trade: TradeLookupResult): 'normal' | 'warn' | 'critical' {
		if (!trade.fetchedAt) return 'critical';
		const ageMs = Date.now() - new Date(trade.fetchedAt).getTime();
		if (ageMs >= tradeStaleCriticalSecs * 1000) return 'critical';
		if (ageMs >= tradeStaleWarnSecs * 1000) return 'warn';
		return 'normal';
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

	// Trade queue state — driven by Rust trade-queue events.
	// tradeQueueStale: after user cancels, ignore events from the in-flight request
	// until a fresh 'queued' event arrives (new lookup batch).
	let tradeQueue = $state<TradeQueueDisplay | null>(null);
	let tradeQueueStale = false;

	$effect(() => {
		let cancelled = false;
		const unlistenPromise = listen<TradeQueueEvent>('trade-queue', (event) => {
			if (cancelled) return;
			const e = event.payload;
			switch (e.kind) {
				case 'queued':
					tradeQueueStale = false;
					tradeQueue = { position: e.position, total: e.total, status: e.kind, waitSecs: 0 };
					break;
				case 'fetching':
				case 'waiting':
					if (tradeQueueStale) break;
					tradeQueue = {
						position: e.position, total: e.total,
						status: e.kind === 'waiting' ? 'waiting' : 'fetching',
						waitSecs: e.kind === 'waiting' ? (e as any).waitSecs ?? 0 : 0,
					};
					break;
				case 'cancelled':
					tradeQueueStale = true;
					tradeQueue = null;
					break;
				default:
					tradeQueue = null;
			}
		});
		return () => {
			cancelled = true;
			unlistenPromise.then(unlisten => unlisten());
		};
	});

	// Widen the interactive click zone when the queue row is visible.
	$effect(() => {
		const width = tradeQueue ? 140 : 48;
		invoke('set_overlay_interactive_width', { width })
			.catch(e => console.warn('[overlay] set_overlay_interactive_width failed:', e));
	});

	// Trade refresh — request the Comparator to do the lookup via event.
	// Comparator handles it → updates tradeData → pushes to Rust → we pick it up on next poll.
	function requestTradeRefresh(gem: CompareGem) {
		if (tradeLoading[gem.name]) return; // already in progress
		invoke('request_trade_refresh', { gem: gem.name, variant: gem.variant })
			.catch(e => console.error('Trade refresh request failed:', e));
	}

	function handlePick() {
		if (!selectedGem) return;
		const gem = results.find((g) => g.name === selectedGem);
		if (gem) {
			getCurrentWebviewWindow().emit('overlay-pick', {
				name: gem.name, variant: gem.variant, roi: gem.roi,
			}).catch(e => console.error('[overlay] emit overlay-pick failed:', e));
		}
	}

	function handleClear() {
		results = [];
		selectedGem = null;
		tradeData = {};
		// Tell the Comparator to clear too.
		getCurrentWebviewWindow().emit('overlay-clear', {})
			.catch(e => console.error('[overlay] emit overlay-clear failed:', e));
	}

	// Handle clicks forwarded from the mouse hook (overlay is fully click-through).
	// The hook sends overlay-relative {x, y} in physical pixels. We convert to
	// logical pixels and use elementFromPoint to find the target button — no
	// fragile coordinate math, works regardless of DPI/layout.
	$effect(() => {
		let cancelled = false;

		const unlistenPromise = listen<{ x: number; y: number }>('overlay-click', (event) => {
			if (cancelled || results.length === 0 || cachedScaleFactor === 0) return;
			const lx = event.payload.x / cachedScaleFactor;
			const ly = event.payload.y / cachedScaleFactor;

			const el = document.elementFromPoint(lx, ly);
			if (!el) {
				console.warn(`[overlay] elementFromPoint(${lx}, ${ly}) returned null — DPI or layout mismatch?`);
				return;
			}

			const btn = el.closest('[data-action]') as HTMLElement | null;
			if (!btn) return; // clicked on non-interactive area (gap between buttons)

			const action = btn.dataset.action;
			if (action === 'clear') {
				handleClear();
			} else if (action === 'cancel') {
				invoke('trade_cancel').catch(e => console.error('trade_cancel failed:', e));
			} else if (action === 'pick' || action === 'refresh') {
				const rawIndex = btn.dataset.index;
				if (rawIndex == null) return; // no index on this button
				const idx = parseInt(rawIndex, 10);
				if (isNaN(idx) || idx >= results.length) return;
				if (action === 'pick') {
					selectedGem = results[idx].name;
					handlePick();
				} else {
					requestTradeRefresh(results[idx]);
				}
			}
		});

		return () => {
			cancelled = true;
			unlistenPromise.then(unlisten => unlisten());
		};
	});

	// Poll Rust for comparator data (cross-window events unreliable, onMount doesn't fire in overlay)
	let lastJson = '';
	$effect(() => {
		// Fetch staleness thresholds once on mount — they only change when the user edits settings.
		invoke<any>('get_status').then((status) => {
			if (status) {
				tradeStaleWarnSecs = status.trade_stale_warn_secs ?? 120;
				tradeStaleCriticalSecs = status.trade_stale_critical_secs ?? 600;
			}
		}).catch(e => console.warn('[overlay] Failed to fetch staleness thresholds, using defaults:', e));

		const pollInterval = setInterval(async () => {
			try {
				const data = await invoke<{ results: CompareGem[]; tradeData: Record<string, TradeLookupResult | null>; tradeLoading?: Record<string, boolean>; tradeError?: Record<string, boolean> }>('get_comparator_data');
				// Always sync loading + error state (changes frequently, no dedup needed).
				tradeLoading = data.tradeLoading ?? {};
				tradeError = data.tradeError ?? {};

				const gemsJson = JSON.stringify(data.results?.map((r: any) => r.name) ?? []);
				// Include trade fetchedAt timestamps so refreshes are detected.
				const tradeSig = Object.entries(data.tradeData ?? {})
					.map(([k, v]: [string, any]) => `${k}:${v?.fetchedAt || ''}`)
					.sort().join(',');
				const combinedKey = gemsJson + '|' + tradeSig;
				if (combinedKey !== lastJson) {
					lastJson = combinedKey;
					results = data.results ?? [];
					tradeData = data.tradeData ?? {};
					if (results.length > 0 && !selectedGem) {
						const best = results.find((g) => g.recommendation === 'BEST');
						selectedGem = best?.name ?? results[0]?.name ?? null;
					}
					if (results.length === 0) {
						selectedGem = null;
					}
				}
			} catch (e) { console.warn('[overlay] poll failed:', e); }
		}, 500);

		return () => {
			clearInterval(pollInterval);
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
							{#if gem.transPrice === 0 && gem.low7d === 0 && gem.high7d === 0}
								<span class="anomaly">no ninja data</span>
							{:else}
								<span class="sell" style="color: {sigColor}">{gem.signal}</span>
								<span class="range">wk {formatPrice(gem.low7d)}–{formatPrice(gem.high7d)}</span>
								<span class="tier" style="color: {tierColor}">{gem.priceTier}</span>
							{/if}
						</div>
						<div class="row-bottom">
							<span class="variant-label">{gem.variant}</span>
							<span class="price-col">
								<span class="price-label">ninja</span>
								<span class="price">{formatPrice(gem.transPrice)}</span>
							</span>
							{#if trade}
								{#if trade.signals.sellerConcentration !== 'NORMAL'}
									<span class="trade-warn">{trade.signals.sellerConcentration}</span>
								{/if}
								<span class="listings-wrap">
									{#each trade.listings as listing, i}
										<span class="listing-col" class:first={i === 0} class:corrupted={listing.corrupted}>
											<span class="listing-price">{listing.price}{listing.currency === 'divine' ? 'd' : 'c'}</span>
											<span class="listing-age">{listingAge(listing.indexedAt)}</span>
										</span>
									{/each}
								</span>
								<span class="trade-total">
									<span>listings</span>
									<span>{trade.total}</span>
								</span>
								<span class="cache-age" class:warn={tradeStaleness(trade) === 'warn'} class:critical={tradeStaleness(trade) === 'critical'}>{tradeCacheAge(trade)}</span>
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
						<button class="act-btn pick-btn" class:active={selectedGem === gem.name} data-action="pick" data-index={i} title="Pick">&#x2713;</button>
						{#if tradeLoading[gem.name]}
							<span class="act-btn loading-btn"><span class="spin-icon">&#x21BB;</span></span>
						{:else}
							<button class="act-btn" class:error-btn={tradeError[gem.name]} data-action="refresh" data-index={i} title={tradeError[gem.name] ? 'Rate limited — retry' : 'Refresh trade'}>&#x21BB;</button>
						{/if}
					</div>
				{/each}
				<button class="clear-act" data-action="clear">clear</button>
			</div>
		</div>
		{#if tradeQueue}
			<div class="queue-row">
				<span class="queue-status">
					{Math.min(tradeQueue.position, tradeQueue.total)}/{tradeQueue.total}
					{#if tradeQueue.status === 'waiting' && tradeQueue.waitSecs > 0}{Math.ceil(tradeQueue.waitSecs)}s{/if}
				</span>
				<button class="clear-act queue-cancel" data-action="cancel">&times;</button>
			</div>
		{/if}
	{/if}
</div>

<style>
	:global(html), :global(body) {
		margin: 0;
		padding: 0;
		background: transparent !important;
		overflow: hidden;
		user-select: none;
		-webkit-user-select: none;
	}

	.surface {
		background: transparent;
		position: fixed;
		bottom: 30px;
		left: 0;
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
		width: 560px;
		pointer-events: none;
	}

	/* --- Gem rows --- */
	.gem-row {
		padding: 8px 0;
		height: 72px;
		box-sizing: border-box;
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
		height: 28px;
		color: #9ca3af;
	}

	.variant-label {
		font-size: 10px;
		color: #6b7280;
		flex-shrink: 0;
	}

	.gem-name {
		font-weight: 600;
		font-size: 14px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.tier {
		font-weight: 700;
		font-size: 11px;
		flex-shrink: 0;
		margin-left: auto;
	}

	.cache-age {
		font-size: 10px;
		color: #6b7280;
		margin-left: auto;
	}

	.cache-age.warn {
		color: #eab308;
	}

	.cache-age.critical {
		color: #ef4444;
	}

	.price-col {
		display: flex;
		flex-direction: column;
		align-items: center;
		flex-shrink: 0;
	}

	.price-label {
		font-size: 10px;
		color: #6b7280;
	}

	.price {
		font-weight: 700;
		color: #fbbf24;
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
	.listings-wrap {
		display: flex;
		flex: 1;
		min-width: 0;
		overflow: hidden;
	}

	.trade-total {
		display: flex;
		flex-direction: column;
		align-items: center;
		font-size: 10px;
		color: #6b7280;
		margin-left: 6px;
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
		margin-left: 10px;
	}

	.anomaly {
		color: #eab308;
		font-size: 11px;
		font-style: italic;
		margin-left: auto;
	}

	.listing-col {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 0 9px;
		margin-right: 1px;
		border-right: 1px solid rgba(42, 45, 55, 0.5);
	}


	.listing-col:last-of-type {
		border-right: none;
		margin-right: 0;
	}

	.listing-price {
		color: #e4e4e7;
		font-size: 12px;
	}

	.listing-col.first {
		margin-left: 4px;
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

	/* --- Side buttons (clicks come from mouse hook via overlay-click events) ---
	   pointer-events: auto is needed so elementFromPoint can find the buttons.
	   OS-level click-through is handled by WS_EX_TRANSPARENT, not CSS. */
	.side {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: flex-end;
		pointer-events: auto;
		position: fixed;
		right: 0;
		top: 1px;
		bottom: 0;
	}

	.side-row {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 72px;
		gap: 2px;
		box-sizing: border-box;
		padding: 8px 0;
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

	.loading-btn {
		color: #6b7280;
		cursor: default;
	}

	.error-btn {
		color: #ef4444 !important;
		border-color: rgba(239, 68, 68, 0.6) !important;
		background: rgba(239, 68, 68, 0.2) !important;
	}

	.spin-icon {
		display: inline-block;
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
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

	.queue-row {
		display: flex;
		align-items: center;
		gap: 4px;
		pointer-events: auto;
		position: fixed;
		bottom: 0;
		right: 48px;
	}

	.queue-status {
		font-size: 9px;
		color: #eab308;
		white-space: nowrap;
	}

	.queue-cancel {
		font-size: 11px;
		padding: 4px 6px;
		margin-top: 0;
		color: #eab308;
		border-color: rgba(234, 179, 8, 0.3);
	}

	.queue-cancel:hover {
		color: #ef4444;
		border-color: rgba(239, 68, 68, 0.4);
	}
</style>
