<script lang="ts">
	import { fetchGemNames, fetchCompare, type CompareGem } from '$lib/api';
	import { baseGemName, baseGemTradeUrl } from '$lib/trade-utils';
	import type { TradeLookupResult, TradeSignals } from '$lib/tradeApi';
	import { SIGNAL_TOOLTIPS } from '$lib/tooltips';
	import { store } from '$lib/stores/status.svelte';
	import { listen } from '@tauri-apps/api/event';
	import { invoke } from '@tauri-apps/api/core';
	import SignalBadge from './SignalBadge.svelte';
	import Sparkline from './Sparkline.svelte';
	import GemIcon from './GemIcon.svelte';
	import Select from '$lib/components/Select.svelte';
	import Tooltip from '$lib/components/Tooltip.svelte';

	const SIGNAL_COLORS: Record<string, string> = {
		STABLE: '#5eead4', UNCERTAIN: '#9ca3af', HERD: '#eab308',
		DUMPING: '#ef4444', RECOVERY: '#a855f7', TRAP: '#ef4444',
	};

	const SIGNAL_ICONS: Record<string, string> = {
		STABLE: '\u200B', UNCERTAIN: '?', HERD: '\u26A1',
		DUMPING: '\u23EC', RECOVERY: '\uD83D\uDD04', TRAP: '\uD83D\uDEAB',
	};

	function signalTooltipHtml(signal: string): string {
		const color = SIGNAL_COLORS[signal] || '#9ca3af';
		const desc = SIGNAL_TOOLTIPS[signal] || '';
		return `<b style="color:${color}">${signal}</b>${desc ? ' — ' + desc : ''}`;
	}

	let {
		league = '',
		divineRate = 0,
		onQueueGem,
	}: {
		league?: string;
		divineRate?: number;
		onQueueGem?: (gem: string, variant: string, roi: number, tradeData: TradeLookupResult | null) => void;
	} = $props();

	let selectedForQueue = $state<string | null>(null);

	$effect(() => {
		// Auto-select the BEST gem when results change
		if (results.length > 0) {
			const best = results.find((g) => g.recommendation === 'BEST');
			selectedForQueue = best?.name ?? null;
		} else {
			selectedForQueue = null;
		}
	});

	// Push results + trade data + loading state to Rust for overlay to poll
	$effect(() => {
		invoke('set_comparator_data', {
			payload: { results, tradeData: { ...tradeData }, tradeLoading: { ...tradeLoading }, tradeError: { ...tradeError } },
		}).catch(e => console.warn('[comparator] push to overlay failed:', e));
	});

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];
	const VARIANT_OPTIONS = VARIANTS.map((v) => ({ value: v, label: v }));

	// Listen for gem-detected events from Rust OCR.
	// Rust emits one gem at a time as a string. Accumulate up to 3, then auto-compare.
	// loadResults is debounced with 300ms to collapse rapid OCR detections.
	let loadDebounce: ReturnType<typeof setTimeout> | null = null;
	let compareAbort: AbortController | null = null;

	function debouncedLoadResults() {
		if (loadDebounce) clearTimeout(loadDebounce);
		loadDebounce = setTimeout(() => loadResults(), 300);
	}

	$effect(() => {
		let cancelled = false;
		const gemPromise = listen<string>('gem-detected', (event) => {
			if (cancelled) return;
			const gemName = event.payload;
			if (!gemName || selectedGems.includes(gemName)) return;
			if (selectedGems.length >= 3) return;
			selectedGems = [...selectedGems, gemName];
			debouncedLoadResults();
		});

		// Auto-clear when new font round starts or manual scan restarts
		const clearPromise = listen('gems-cleared', () => {
			if (cancelled) return;
			clearAll();
		});

		// Listen for pick from overlay window
		const pickPromise = listen<{ name: string; variant: string; roi: number }>('overlay-pick', (event) => {
			if (cancelled) return;
			const { name } = event.payload;
			selectedForQueue = name;
			handleNext();
		});

		// Listen for trade refresh request from overlay
		const refreshPromise = listen<{ name: string; variant: string }>('overlay-trade-refresh', (event) => {
			if (cancelled) return;
			refreshTradeData(event.payload.name);
		});

		// Listen for clear from overlay
		const overlayClearPromise = listen('overlay-clear', () => {
			if (cancelled) return;
			clearAll();
		});

		return () => {
			cancelled = true;
			if (loadDebounce) clearTimeout(loadDebounce);
			if (compareAbort) compareAbort.abort();
			gemPromise.then(unlisten => unlisten());
			clearPromise.then(unlisten => unlisten());
			pickPromise.then(unlisten => unlisten());
			refreshPromise.then(unlisten => unlisten());
			overlayClearPromise.then(unlisten => unlisten());
		};
	});

	let variant = $state('20/20');
	let searchQuery = $state('');
	let suggestions = $state<string[]>([]);
	let selectedGems = $state<string[]>([]);
	let results = $state<CompareGem[]>([]);
	let showDropdown = $state(false);
	let highlightedIndex = $state(-1);
	let inputRef = $state<HTMLInputElement | null>(null);

	let tradeData = $state<Record<string, TradeLookupResult | null>>({});
	let tradeLoading = $state<Record<string, boolean>>({});
	let tradeError = $state<Record<string, boolean>>({});
	let tradeExpanded = $state<Record<string, boolean>>({});
	let autoTradeEnabled = $state(store.status?.auto_trade_enabled ?? false);

	/** Trade cache age in milliseconds, or null if no data. */
	function tradeCacheAge(gem: string): number | null {
		const td = tradeData[gem];
		if (!td?.fetchedAt) return null;
		return Date.now() - new Date(td.fetchedAt).getTime();
	}

	function tradeCacheAgeStr(gem: string): string {
		const age = tradeCacheAge(gem);
		if (age == null) return '';
		const mins = Math.floor(age / 60000);
		if (mins < 1) return '<1m ago';
		if (mins < 60) return `${mins}m ago`;
		return `${Math.floor(mins / 60)}h ${mins % 60}m ago`;
	}

	/** Staleness level based on store thresholds: 'normal', 'warn', or 'critical'. */
	function tradeStaleness(gem: string): 'normal' | 'warn' | 'critical' {
		const age = tradeCacheAge(gem);
		if (age == null) return 'normal';
		const warnMs = (store.status?.trade_stale_warn_secs ?? 120) * 1000;
		const critMs = (store.status?.trade_stale_critical_secs ?? 600) * 1000;
		if (age >= critMs) return 'critical';
		if (age >= warnMs) return 'warn';
		return 'normal';
	}

	/** Check if trade data is stale enough to warrant auto-refresh. */
	function isTradeAutoRefreshDue(gem: string): boolean {
		const age = tradeCacheAge(gem);
		if (age == null) return true; // no data at all
		const refreshMs = (store.status?.trade_auto_refresh_secs ?? 900) * 1000;
		return age >= refreshMs;
	}

	async function loadResults() {
		const active = selectedGems.filter(Boolean);
		if (active.length === 0) {
			results = [];
			return;
		}
		// Cancel any in-flight compare request
		if (compareAbort) compareAbort.abort();
		compareAbort = new AbortController();
		try {
			results = await fetchCompare(active, variant, compareAbort.signal);
			// Populate tradeData from compare response (server-side cache enrichment)
			for (const gem of results) {
				if (gem.trade) {
					tradeData[gem.name] = gem.trade;
				}
			}
			// Auto-trade: if enabled, invoke Rust trade_lookup for gems with no/stale trade data
			if (autoTradeEnabled) {
				for (const gem of results) {
					if (!tradeData[gem.name] || isTradeAutoRefreshDue(gem.name)) {
						invokeRustTrade(gem.name);
					}
				}
			}
		} catch (err: any) {
			if (err?.name === 'AbortError') return; // cancelled, not an error
			console.error('[Comparator] Failed to load results:', err);
			results = [];
		}
	}

	/** Invoke Rust-side trade lookup (hits GGG directly, Rust handles server submit). */
	async function invokeRustTrade(gem: string) {
		tradeLoading[gem] = true;
		tradeError[gem] = false;
		try {
			const result = await invoke<TradeLookupResult>('trade_lookup', {
				gem, variant, divineRate: divineRate || undefined,
			});
			tradeData[gem] = result;
		} catch (err) {
			console.warn(`[Trade] Rust lookup failed for ${gem}:`, err);
			tradeError[gem] = true;
		} finally {
			tradeLoading[gem] = false;
		}
	}

	let searchDebounce: ReturnType<typeof setTimeout> | null = null;

	function handleInput(query: string) {
		searchQuery = query;
		if (searchDebounce) clearTimeout(searchDebounce);
		if (query.length < 2) {
			suggestions = [];
			showDropdown = false;
			highlightedIndex = -1;
			return;
		}
		searchDebounce = setTimeout(async () => {
			// Guard: if query changed while waiting, skip stale fetch
			if (searchQuery !== query) return;
			const allNames = await fetchGemNames(query);
			// Guard: if query changed during fetch, skip stale results
			if (searchQuery !== query) return;
			suggestions = allNames.filter((n) => !selectedGems.includes(n));
			showDropdown = suggestions.length > 0;
			highlightedIndex = suggestions.length === 1 ? 0 : -1;
		}, 100);
	}

	function selectGem(name: string) {
		if (selectedGems.length >= 3) return;
		if (selectedGems.includes(name)) return;
		selectedGems = [...selectedGems, name];
		searchQuery = '';
		suggestions = [];
		showDropdown = false;
		highlightedIndex = -1;
		loadResults();
		setTimeout(() => inputRef?.focus(), 0);
	}

	function removeGem(index: number) {
		const removed = selectedGems[index];
		selectedGems = selectedGems.filter((_, i) => i !== index);
		if (removed) {
			delete tradeData[removed];
			delete tradeLoading[removed];
			delete tradeError[removed];
			delete tradeExpanded[removed];
		}
		loadResults();
		setTimeout(() => inputRef?.focus(), 0);
	}

	function clearAll() {
		if (compareAbort) compareAbort.abort();
		if (loadDebounce) clearTimeout(loadDebounce);
		selectedGems = [];
		results = [];
		tradeData = {};
		tradeLoading = {};
		tradeError = {};
		tradeExpanded = {};
		searchQuery = '';
		suggestions = [];
		showDropdown = false;
		highlightedIndex = -1;
		setTimeout(() => inputRef?.focus(), 0);
	}

	function handleVariantChange() {
		loadResults();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			showDropdown = false;
			highlightedIndex = -1;
			return;
		}
		if (e.key === 'Backspace' && searchQuery === '' && selectedGems.length > 0) {
			removeGem(selectedGems.length - 1);
			return;
		}
		if (!showDropdown || suggestions.length === 0) return;
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			highlightedIndex = (highlightedIndex + 1) % suggestions.length;
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			highlightedIndex = highlightedIndex <= 0 ? suggestions.length - 1 : highlightedIndex - 1;
		} else if ((e.key === 'Enter' || e.key === ' ') && highlightedIndex >= 0) {
			e.preventDefault();
			selectGem(suggestions[highlightedIndex]);
		}
	}

	function recLabel(rec: string): string {
		if (rec === 'BEST') return 'BEST';
		if (rec === 'AVOID') return 'AVOID';
		return 'OK';
	}

	function recIcon(rec: string): string {
		if (rec === 'BEST') return '\u2713';
		if (rec === 'AVOID') return '\u2717';
		return '\u2022';
	}

	function recClass(rec: string): string {
		if (rec === 'BEST') return 'rec-best';
		if (rec === 'AVOID') return 'rec-avoid';
		return 'rec-ok';
	}

	function tierClass(tier: string): string {
		if (tier === 'TOP') return 'tier-top';
		if (tier === 'HIGH') return 'tier-high';
		if (tier === 'MID') return 'tier-mid';
		return 'tier-low';
	}

	function urgencyClass(u: string): string {
		if (u === 'SELL_NOW') return 'urgency-sell-now';
		if (u === 'UNDERCUT') return 'urgency-undercut';
		if (u === 'HOLD') return 'urgency-hold';
		return 'urgency-wait';
	}

	function sellabilityClass(label: string): string {
		if (label === 'FAST SELL') return 'slab-fast';
		if (label === 'GOOD') return 'slab-good';
		if (label === 'MODERATE') return 'slab-moderate';
		if (label === 'SLOW') return 'slab-slow';
		return 'slab-unlikely';
	}

	function velocityStr(v: number): string {
		if (v > 0) return `\u2191${v}`;
		if (v < 0) return `\u2193${Math.abs(v)}`;
		return '0';
	}

	function fmtPrice(v: number): string {
		return Number.isInteger(v) ? v.toString() : v.toFixed(1);
	}


	function refreshTradeData(gem: string) {
		// Don't clear existing data — keep showing cached while loading.
		invokeRustTrade(gem);
	}

	function concentrationClass(c: TradeSignals['sellerConcentration']): string {
		if (c === 'MONOPOLY') return 'signal-red';
		if (c === 'CONCENTRATED') return 'signal-yellow';
		return 'signal-green';
	}

	function formatTimeAgo(isoString: string): string {
		const diff = Date.now() - new Date(isoString).getTime();
		const mins = Math.floor(diff / 60000);
		if (mins < 60) return `${mins}m ago`;
		const hours = Math.floor(mins / 60);
		if (hours < 24) return `${hours}h ago`;
		return `${Math.floor(hours / 24)}d ago`;
	}

	function toggleExpanded(gem: string) {
		tradeExpanded[gem] = !tradeExpanded[gem];
	}

	function handleNext() {
		if (!selectedForQueue || !onQueueGem) return;
		const selected = results.find((g) => g.name === selectedForQueue);
		if (!selected) return;
		const trade = tradeData[selectedForQueue] ?? null;
		onQueueGem(selectedForQueue, variant, selected.roi, trade);
		clearAll();
	}

</script>

<section class="section">
	<div class="section-header">
		<div class="title-group">
			<h2 class="section-title">Lab Options Comparator</h2>
		</div>
		<div class="header-controls">
			{#if selectedForQueue && onQueueGem}
				<button class="next-btn" onclick={handleNext}>&#10003; Next</button>
			{/if}
			{#if selectedGems.length > 0}
				<button class="clear-btn" onclick={clearAll}>Clear All</button>
			{/if}
			<Tooltip text="When enabled, auto-invokes GGG trade lookup on cache miss or stale data"><label class="auto-trade-toggle">
				<input type="checkbox" bind:checked={autoTradeEnabled} onchange={() => invoke('set_auto_trade', { enabled: autoTradeEnabled }).catch(e => console.warn('set_auto_trade failed:', e))} />
				<span class="toggle-label">Auto-trade</span>
			</label></Tooltip>
			<div class="variant-select">
				<span class="select-label">Variant:</span>
				<Select bind:value={variant} options={VARIANT_OPTIONS} onchange={() => handleVariantChange()} />
			</div>
		</div>
	</div>

	<div class="comparator-input">
		<div class="tags-row">
			{#each selectedGems as gem, i}
				<span class="tag">
					<GemIcon name={gem} size={20} />
					{gem}
					<button class="tag-remove" onclick={() => removeGem(i)}>&#215;</button>
				</span>
			{/each}
		</div>

		{#if selectedGems.length < 3}
			<div class="search-wrapper">
				<input
					bind:this={inputRef}
					type="text"
					class="search-input"
					placeholder={selectedGems.length === 0 ? "Type gem name..." : "Add another gem..."}
					value={searchQuery}
					oninput={(e) => handleInput((e.target as HTMLInputElement).value)}
					onkeydown={handleKeydown}
					onfocus={() => { if (suggestions.length) showDropdown = true; }}
					onblur={() => setTimeout(() => { showDropdown = false; }, 200)}
				/>
				{#if showDropdown && suggestions.length > 0}
					<div class="dropdown">
						{#each suggestions as gem, i}
							<button
								class="dropdown-item"
								class:highlighted={i === highlightedIndex}
								onmousedown={() => selectGem(gem)}
							>
								<GemIcon name={gem} size={36} />
								{gem}
							</button>
						{/each}
					</div>
				{/if}
			</div>
		{:else}
			<p class="max-reached">3 gems selected (max)</p>
		{/if}
	</div>

	{#if results.length > 0}
		<div class="cards-row">
			{#each results as gem}
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="compare-card" class:queue-selected={selectedForQueue === gem.name} onclick={() => { selectedForQueue = gem.name; }}>
					<div class="card-name-row">
						<GemIcon name={gem.name} size={40} />
						<span class="card-name">{gem.name}</span>
						<span class="tier-badge {tierClass(gem.priceTier)}">{gem.priceTier}</span>
					</div>
					<div class="card-row price-line">
						<Tooltip text="Ninja price vs risk-adjusted (sell probability × stability)"><span class="price-display">
							<span class="price-raw">{gem.transPrice}c</span>
							{#if gem.riskAdjustedPrice > 0}
								<span class="price-risk-adj">(<span class="price-risk-label">Risk-adjusted:</span> {gem.riskAdjustedPrice}c)</span>
							{/if}
						</span></Tooltip>
					</div>
					<div class="card-row small listings-line">
						<span>{gem.transListings} listings</span>
						<Tooltip text="Price change over last 12 hours"><span class="velocity-inline">({gem.transVelocity > 0 ? '+' : ''}{gem.transVelocity * 12}c /12h)</span></Tooltip>
						<Tooltip text="Liquidity tier"><span class="liq">{gem.liquidityTier}</span></Tooltip>
					</div>
					<div class="card-row small signals-line">
						<Tooltip text="Sellability score 0-100. How quickly you can sell this gem. Based on listings, demand velocity, and price tier."><span class="sellability-inline {sellabilityClass(gem.sellabilityLabel)}">{gem.sellabilityLabel} &nbsp;{gem.sellability}</span></Tooltip>
						<span class="signals-right">
							<SignalBadge signal={gem.signal} />
							{#if gem.sellConfidence}
								<SignalBadge signal={gem.sellConfidence} type="confidence" />
							{/if}
						</span>
					</div>
					<div class="price-context">
						<div class="price-row">
							<span>Listed: {gem.transPrice}c</span>
							{#if gem.quickSellPrice > 0}
								<span class="quick-sell">Quick-sell: ~{gem.quickSellPrice}c</span>
							{/if}
						</div>
						{#if gem.low7d > 0 || gem.high7d > 0}
							<div class="range-context">
								<span class="range-label">7 days lowest: {gem.low7d}c</span>
								<span class="range-sep">&middot;</span>
								<span class="range-label">7 days highest: {gem.high7d}c</span>
							</div>
						{:else if gem.transPrice === 0}
							<div class="anomaly-banner">No poe.ninja data — check trade listings</div>
						{/if}
					</div>
					{#if gem.signal === 'DUMPING' || gem.signal === 'TRAP'}
						<div class="listing-warning">
							{gem.signal === 'DUMPING' ? 'Listings rising — price may drop' : 'Price trap — avoid'}
						</div>
					{/if}



					{#if gem.tierAction}
						<div class="tier-action">{gem.tierAction}</div>
					{/if}

					<!-- Trade Data Section -->
					<div class="trade-section">
						{#if tradeLoading[gem.name] && !tradeData[gem.name]}
							<div class="trade-loading">
								<span class="trade-spinner"></span>
								<span class="trade-loading-text">Fetching trade data...</span>
							</div>
						{:else if !tradeData[gem.name]}
							<div class="trade-empty">
								<button class="trade-action-btn trade-fetch-btn" onclick={() => refreshTradeData(gem.name)}>&#8635; Fetch trade data</button>
							</div>
						{/if}
						{#if tradeData[gem.name]}
							{@const td = tradeData[gem.name]}
							{@const divFloor = td.listings.find(l => l.currency === 'divine')}
							{@const chaosFloor = td.listings.find(l => l.currency === 'chaos')}
							<div class="trade-data">
								<div class="trade-price-row">
									{#if divFloor}
										<Tooltip text="Cheapest divine listing"><span class="trade-floor-price">{fmtPrice(divFloor.price)} div</span></Tooltip>
									{/if}
									{#if chaosFloor}
										{#if divFloor}<span class="trade-floor-sep">/</span>{/if}
										<Tooltip text="Cheapest chaos listing"><span class="trade-floor-price">{fmtPrice(chaosFloor.chaosPrice)}c</span></Tooltip>
									{/if}
									{#if !divFloor && !chaosFloor}
										<span class="trade-floor-price">{fmtPrice(td.priceFloor)}c</span>
									{/if}
									<span class="trade-floor-label">trade floor</span>
									<span class="trade-delta" class:trade-delta-positive={td.priceFloor - gem.roi > 0} class:trade-delta-negative={td.priceFloor - gem.roi < 0}>
										{td.priceFloor - gem.roi > 0 ? '+' : ''}{fmtPrice(td.priceFloor - gem.roi)}c vs ninja
									</span>
									<Tooltip text="Seller concentration: {td.signals.uniqueAccounts} unique accounts in top 10"><span class="trade-signal-badge {concentrationClass(td.signals.sellerConcentration)}">
										{td.signals.sellerConcentration}
									</span></Tooltip>
									{#if td.signals.priceOutlier}
										<Tooltip text="Cheapest listing is significantly below median"><span class="trade-signal-badge signal-red">OUTLIER</span></Tooltip>
									{/if}
								</div>
								<div class="trade-meta-row">
									<span class="trade-listings-badge">{td.total} listings</span>
									<span class="trade-median">median: {fmtPrice(td.medianTop10)}c</span>
									<span class="trade-meta-right">
										<span class="trade-cache-age" class:trade-cache-warn={tradeStaleness(gem.name) === 'warn'} class:trade-cache-critical={tradeStaleness(gem.name) === 'critical'}>{tradeCacheAgeStr(gem.name)}</span>
										{#if tradeLoading[gem.name]}
											<span class="trade-spinner-inline"></span>
										{:else}
											<button class="trade-action-btn" class:trade-refresh-stale={tradeStaleness(gem.name) !== 'normal'} class:trade-refresh-error={tradeError[gem.name]} onclick={() => refreshTradeData(gem.name)} title={tradeError[gem.name] ? 'Rate limited — click to retry' : 'Refresh trade data'}>&#8635;</button>
										{/if}
										{#if gem.name.includes(' of ')}
											<a class="trade-action-btn trade-base-link" href={baseGemTradeUrl(gem.name, variant, league)} target="_blank" title="Buy base gem: {baseGemName(gem.name)}">Base</a>
										{/if}
									</span>
								</div>
								{#if td.listings.length > 0}
									<div class="trade-listings-table">
										<div class="trade-listings-header">
											<span class="tl-col-price">Price</span>
											<span class="tl-col-detail">Lvl/Qual</span>
											<span class="tl-col-time">Listed</span>
										</div>
										{#each td.listings as listing}
											<div class="trade-listing-row">
												<span class="tl-col-price">
													{#if listing.currency === 'divine'}
														{fmtPrice(listing.price)} div
														<span class="tl-original">({fmtPrice(listing.chaosPrice)}c)</span>
													{:else}
														{fmtPrice(listing.price)}c
													{/if}
												</span>
												<span class="tl-col-detail">
													{listing.gemLevel}/{listing.gemQuality}
													{#if listing.corrupted}<span class="tl-corrupted">C</span>{/if}
												</span>
												<span class="tl-col-time">{formatTimeAgo(listing.indexedAt)}</span>
											</div>
										{/each}
									</div>
								{/if}
							</div>
						{/if}
					</div>

					<div class="sparkline-row">
						<Tooltip text="poe.ninja price trend — last 12 hours (30-min snapshots)"><span>
						<Sparkline data={gem.sparkline} width={280} height={32} />
					</span></Tooltip>
					</div>
					<div class="history history-compact">
						{#each gem.signalHistory as h}
							<div class="history-line">
								<span class="hist-time">{h.time}</span>
								<Tooltip text={signalTooltipHtml(h.from)}>
									<span class="hist-sig {h.from === 'STABLE' ? 'sig-stable' : ''}" style="color: {SIGNAL_COLORS[h.from] || '#9ca3af'}; background: {SIGNAL_COLORS[h.from] || '#9ca3af'}20">{#if h.from !== 'STABLE'}{SIGNAL_ICONS[h.from] || '?'}{/if}</span>
								</Tooltip>
								<span class="hist-arrow">&#8594;</span>
								<Tooltip text={signalTooltipHtml(h.to)}>
									<span class="hist-sig {h.to === 'STABLE' ? 'sig-stable' : ''}" style="color: {SIGNAL_COLORS[h.to] || '#9ca3af'}; background: {SIGNAL_COLORS[h.to] || '#9ca3af'}20">{#if h.to !== 'STABLE'}{SIGNAL_ICONS[h.to] || '?'}{/if}</span>
								</Tooltip>
								<span class="hist-reason">{h.reason}</span>
								<span class="hist-listings">{h.listings} listings</span>
							</div>
						{/each}
					</div>
					<div class="card-rec {recClass(gem.recommendation)}">
						<span class="rec-icon">{recIcon(gem.recommendation)}</span> {recLabel(gem.recommendation)}
					</div>
				</div>
			{/each}
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
		margin-bottom: 20px;
	}
	.section-title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.title-group {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.header-controls {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.clear-btn {
		background: rgba(239, 68, 68, 0.12);
		border: 1px solid rgba(239, 68, 68, 0.3);
		color: var(--color-lab-red);
		padding: 6px 14px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
	}
	.auto-trade-toggle {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		cursor: pointer;
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
		user-select: none;
	}
	.auto-trade-toggle input {
		accent-color: var(--color-lab-yellow, #eab308);
		cursor: pointer;
	}
	.toggle-label {
		white-space: nowrap;
	}
	.clear-btn:hover {
		background: rgba(239, 68, 68, 0.2);
	}
	.next-btn {
		background: rgba(34, 197, 94, 0.15);
		border: 1px solid rgba(34, 197, 94, 0.4);
		color: var(--color-lab-green);
		padding: 6px 14px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
	}
	.next-btn:hover {
		background: rgba(34, 197, 94, 0.25);
	}
	.variant-select {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.select-label {
		color: var(--color-lab-text-secondary);
		font-size: 0.9375rem;
	}

	.comparator-input {
		margin-bottom: 20px;
	}
	.tags-row {
		display: flex;
		gap: 10px;
		margin-bottom: 10px;
		flex-wrap: wrap;
	}
	.tag {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		background: #2a2d37;
		color: #e4e4e7;
		padding: 6px 14px;
		font-size: 0.9375rem;
		font-weight: 600;
		border-radius: 2px;
	}
	.tag-remove {
		background: none;
		border: none;
		color: #e4e4e7;
		font-size: 1rem;
		cursor: pointer;
		padding: 0 2px;
		line-height: 1;
		font-family: inherit;
	}
	.tag-remove:hover {
		color: var(--color-lab-red);
	}
	.search-wrapper {
		position: relative;
	}
	.search-input {
		width: 100%;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text);
		padding: 10px 14px;
		font-size: 0.9375rem;
		font-family: inherit;
		box-sizing: border-box;
	}
	.search-input::placeholder {
		color: var(--color-lab-text-secondary);
	}
	.dropdown {
		position: absolute;
		top: 100%;
		left: 0;
		right: 0;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		z-index: 10;
		max-height: 360px;
		overflow-y: auto;
	}
	.dropdown-item {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		text-align: left;
		padding: 12px 16px;
		color: var(--color-lab-text);
		background: none;
		border: none;
		font-size: 1.0625rem;
		cursor: pointer;
		font-family: inherit;
	}
	.dropdown-item:hover {
		background: rgba(59, 130, 246, 0.1);
	}
	.dropdown-item.highlighted {
		background: #1e40af;
		color: #fff;
	}
	.max-reached {
		color: var(--color-lab-text-secondary);
		font-size: 0.9375rem;
		margin: 0;
	}

	.cards-row {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 16px;
	}
	.compare-card {
		border: 2px solid var(--color-lab-border);
		padding: 24px;
		background: var(--color-lab-bg);
		min-width: 0;
		cursor: pointer;
		transition: border-color 0.15s ease;
	}
	.compare-card.queue-selected {
		border-color: var(--color-lab-green);
	}
	.card-name-row {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-bottom: 16px;
	}
	.card-name {
		font-weight: 700;
		font-size: 1.125rem;
		color: var(--color-lab-text);
	}
	.card-row {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-bottom: 10px;
		flex-wrap: wrap;
	}
	.card-row.small {
		font-size: 0.9375rem;
		color: var(--color-lab-text-secondary);
	}
	.roi {
		color: var(--color-lab-green);
		font-weight: 700;
		font-size: 1.25rem;
		cursor: help;
	}
	.price-line {
		white-space: nowrap;
	}
	.price-display {
		display: inline-flex;
		align-items: baseline;
		gap: 0.35rem;
		cursor: help;
	}
	.price-raw {
		color: var(--color-lab-green);
		font-weight: 700;
		font-size: 1.25rem;
	}
	.price-risk-adj {
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
		font-weight: 500;
	}
	.price-risk-label {
		color: var(--color-lab-text-muted, #888);
		font-weight: 400;
	}
	.listings-line {
		justify-content: flex-start;
	}
	.sellability-inline {
		font-size: 0.75rem;
		font-weight: 700;
	}
	.signals-line {
		justify-content: space-between;
	}
	.signals-right {
		margin-left: auto;
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
	}
	.roi-pct {
		color: var(--color-lab-text-secondary);
		font-size: 1rem;
		cursor: help;
	}
	.velocity-inline {
		color: var(--color-lab-text-secondary);
	}
	.cv {
		color: var(--color-lab-text-secondary);
		font-size: 0.9375rem;
		cursor: help;
	}
	.liq {
		font-size: 0.875rem;
		font-weight: 600;
		color: var(--color-lab-yellow);
	}
	.window-label {
		color: var(--color-lab-text-secondary);
		font-size: 0.9375rem;
	}
	.history {
		margin: 12px 0;
	}
	.history-line {
		font-size: 0.875rem;
		color: var(--color-lab-text-secondary);
		display: flex;
		gap: 8px;
		align-items: center;
		padding: 6px 0;
		border-bottom: 1px solid rgba(42, 45, 55, 0.4);
	}
	.history-line:last-child {
		border-bottom: none;
	}
	/* Compact history — fixed-size icon signals */
	.history-compact .history-line {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.hist-sig.sig-stable {
		font-size: 0;
	}
	.hist-sig.sig-stable::after {
		content: '';
		display: block;
		width: 12px;
		height: 2px;
		background: currentColor;
		border-radius: 1px;
	}
	.hist-sig {
		display: inline-grid;
		place-items: center;
		width: 24px;
		height: 22px;
		font-size: 0.8125rem;
		line-height: 0;
		cursor: help;
	}
	.history-compact .hist-arrow {
		width: 14px;
		text-align: center;
		flex-shrink: 0;
		font-size: 0.75rem;
	}
	.hist-time {
		color: var(--color-lab-text);
		font-weight: 600;
		min-width: 42px;
		font-size: 0.8125rem;
	}
	.hist-arrow {
		color: var(--color-lab-text-secondary);
		font-size: 1.125rem;
	}
	.hist-reason {
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
	}
	.hist-listings {
		margin-left: auto;
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
		white-space: nowrap;
	}
	.card-rec {
		font-size: 1rem;
		font-weight: 700;
		margin-top: 12px;
		padding: 8px 0;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.rec-icon {
		font-size: 1.0625rem;
	}
	.sparkline-row {
		margin: 16px 0;
		width: 100%;
		padding: 8px 0;
	}
	.sparkline-row :global(svg) {
		width: 100%;
		height: 32px;
	}
	/* Tier badge */
	.tier-badge {
		font-size: 0.75rem;
		font-weight: 700;
		padding: 2px 8px;
		margin-left: auto;
		letter-spacing: 0.03em;
		white-space: nowrap;
	}
	.tier-top { color: #fbbf24; background: rgba(251, 191, 36, 0.12); }
	.tier-high { color: #fb923c; background: rgba(251, 146, 60, 0.12); }
	.tier-mid-high { color: #c084fc; background: rgba(192, 132, 252, 0.12); }
	.tier-mid { color: #94a3b8; background: rgba(148, 163, 184, 0.12); }
	.tier-low { color: #64748b; background: rgba(100, 116, 139, 0.1); }
	.tier-floor { color: #475569; background: rgba(71, 85, 105, 0.08); }

	/* Sell urgency slot — always takes space for alignment */
	.urgency-slot {
		min-height: 68px;
		margin: 10px 0;
		display: flex;
		flex-direction: column;
		justify-content: center;
	}
	.urgency-banner {
		padding: 8px 12px;
		font-size: 0.875rem;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.urgency-label {
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}
	.urgency-reason {
		font-size: 0.8125rem;
		font-weight: 400;
		opacity: 0.85;
	}
	.urgency-sell-now { color: #fca5a5; background: rgba(239, 68, 68, 0.15); border-left: 3px solid var(--color-lab-red); }
	.urgency-undercut { color: #fdba74; background: rgba(251, 146, 60, 0.12); border-left: 3px solid #fb923c; }
	.urgency-hold { color: #86efac; background: rgba(34, 197, 94, 0.1); border-left: 3px solid var(--color-lab-green); }
	.urgency-wait { color: var(--color-lab-text-secondary); background: rgba(156, 163, 175, 0.08); border-left: 3px solid var(--color-lab-border); }

	/* Sellability bar */
	.sellability-row {
		display: flex;
		align-items: center;
		gap: 10px;
		margin: 10px 0;
	}
	.sellability-bar {
		flex: 1;
		height: 6px;
		background: rgba(42, 45, 55, 0.8);
		border-radius: 3px;
		overflow: hidden;
	}
	.sellability-fill {
		height: 100%;
		border-radius: 3px;
		transition: width 0.3s ease;
	}
	.sellability-label {
		font-size: 0.75rem;
		font-weight: 700;
		white-space: nowrap;
	}
	.sellability-score {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		min-width: 20px;
		text-align: right;
	}
	.slab-fast { color: var(--color-lab-green); }
	.slab-fast.sellability-fill { background: var(--color-lab-green); }
	.slab-good { color: #86efac; }
	.slab-good.sellability-fill { background: #86efac; }
	.slab-moderate { color: var(--color-lab-yellow); }
	.slab-moderate.sellability-fill { background: var(--color-lab-yellow); }
	.slab-slow { color: #fb923c; }
	.slab-slow.sellability-fill { background: #fb923c; }
	.slab-unlikely { color: var(--color-lab-red); }
	.slab-unlikely.sellability-fill { background: var(--color-lab-red); }

	/* Tier action */
	.tier-action {
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
		font-style: italic;
		margin-bottom: 6px;
	}

	.rec-best { color: var(--color-lab-green); }
	.rec-ok { color: var(--color-lab-yellow); }
	.rec-avoid { color: var(--color-lab-red); }

	/* Trade data section */
	.trade-section {
		margin: 12px 0;
		border-top: 1px solid var(--color-lab-border);
		padding-top: 12px;
	}

	.trade-loading {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 0;
	}
	.trade-spinner {
		width: 16px;
		height: 16px;
		border: 2px solid var(--color-lab-border);
		border-top-color: var(--color-lab-text);
		border-radius: 50%;
		animation: trade-spin 0.8s linear infinite;
	}
	@keyframes trade-spin {
		to { transform: rotate(360deg); }
	}
	.trade-loading-text {
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
	}

	.trade-data {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.trade-price-row {
		display: flex;
		align-items: baseline;
		gap: 8px;
		flex-wrap: wrap;
	}
	.trade-actions {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-left: auto;
	}
	.trade-cancel-btn {
		background: none;
		border: 1px solid rgba(239, 68, 68, 0.3);
		color: var(--color-lab-red);
		padding: 2px 8px;
		font-size: 0.875rem;
		cursor: pointer;
		font-family: inherit;
		margin-left: 8px;
	}
	.trade-cancel-btn:hover {
		background: rgba(239, 68, 68, 0.12);
	}
	.trade-cancel-btn-inline {
		background: none;
		border: none;
		color: var(--color-lab-red);
		font-size: 0.875rem;
		cursor: pointer;
		padding: 0 4px;
		font-family: inherit;
		line-height: 1;
	}
	.trade-cancel-btn-inline:hover {
		color: var(--color-lab-text);
	}
	.trade-empty {
		padding: 6px 0;
	}
	.trade-fetch-btn {
		font-size: 0.8125rem;
		padding: 5px 14px;
	}
	.trade-cache-age {
		color: var(--color-lab-text-muted, #888);
		font-size: 0.75rem;
	}
	.trade-cache-warn {
		color: #eab308;
	}
	.trade-cache-critical {
		color: var(--color-lab-red);
	}
	.trade-refresh-stale {
		color: var(--color-lab-red) !important;
		border-color: rgba(239, 68, 68, 0.4) !important;
		background: rgba(239, 68, 68, 0.12) !important;
	}
	.trade-refresh-stale:hover {
		background: rgba(239, 68, 68, 0.2) !important;
	}
	.trade-refresh-error {
		color: #ef4444 !important;
		border-color: rgba(239, 68, 68, 0.6) !important;
		background: rgba(239, 68, 68, 0.2) !important;
	}
	.trade-refresh-error:hover {
		background: rgba(239, 68, 68, 0.3) !important;
	}
	.trade-spinner-inline {
		display: inline-block;
		width: 14px;
		height: 14px;
		border: 2px solid var(--color-lab-border);
		border-top-color: var(--color-lab-text-secondary);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
		vertical-align: middle;
	}
	.trade-action-btn {
		background: rgba(42, 45, 55, 0.6);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text-secondary);
		padding: 4px 8px;
		font-size: 0.75rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
		font-family: inherit;
		text-decoration: none;
		line-height: 1.4;
	}
	.trade-action-btn:hover {
		color: var(--color-lab-text);
		border-color: var(--color-lab-text-secondary);
	}
	.trade-base-link {
		color: var(--color-lab-blue, #3b82f6);
	}
	.trade-base-link:hover {
		color: var(--color-lab-text);
	}
	.trade-floor-sep {
		font-size: 1.125rem;
		color: var(--color-lab-text-secondary);
		margin: 0 2px;
	}
	.trade-floor-price {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
	}
	.trade-floor-label {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}
	.trade-delta {
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
		margin-left: auto;
	}
	.trade-delta-positive {
		color: var(--color-lab-green);
	}
	.trade-delta-negative {
		color: var(--color-lab-red);
	}

	.trade-meta-row {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 0.6875rem;
	}
	.trade-meta-right {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.trade-listings-badge {
		background: rgba(59, 130, 246, 0.12);
		color: #93c5fd;
		padding: 1px 5px;
		font-weight: 600;
		font-size: 0.6875rem;
	}
	.trade-median {
		color: var(--color-lab-text-secondary);
	}

	.trade-signals-row {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
	}
	.trade-signal-badge {
		font-size: 0.6875rem;
		font-weight: 700;
		padding: 2px 8px;
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}
	.signal-green {
		color: var(--color-lab-green);
		background: rgba(34, 197, 94, 0.1);
	}
	.signal-yellow {
		color: var(--color-lab-yellow);
		background: rgba(250, 204, 21, 0.1);
	}
	.signal-red {
		color: var(--color-lab-red);
		background: rgba(239, 68, 68, 0.12);
	}

	.trade-expand-btn {
		background: none;
		border: none;
		color: #93c5fd;
		font-size: 0.8125rem;
		font-weight: 600;
		cursor: pointer;
		padding: 4px 0;
		font-family: inherit;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.trade-expand-btn:hover {
		color: #bfdbfe;
	}
	.trade-expand-arrow {
		font-size: 0.625rem;
	}

	.trade-listings-table {
		margin-top: 6px;
		font-size: 0.75rem;
		border: 1px solid var(--color-lab-border);
		overflow: hidden;
	}
	.trade-listings-header {
		display: grid;
		grid-template-columns: 1.4fr 0.7fr 0.6fr;
		gap: 4px;
		padding: 6px 8px;
		background: rgba(42, 45, 55, 0.6);
		color: var(--color-lab-text-secondary);
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		font-size: 0.625rem;
	}
	.trade-listing-row {
		display: grid;
		grid-template-columns: 1.4fr 0.7fr 0.6fr;
		gap: 4px;
		padding: 5px 8px;
		border-top: 1px solid rgba(42, 45, 55, 0.4);
		color: var(--color-lab-text);
	}
	.trade-listing-row:hover {
		background: rgba(59, 130, 246, 0.05);
	}
	.tl-col-time {
		color: var(--color-lab-text-secondary);
	}
	.tl-corrupted {
		color: var(--color-lab-red);
		font-weight: 700;
		margin-left: 2px;
	}

	.tl-original {
		color: var(--color-lab-text-secondary);
		font-size: 0.7rem;
		margin-left: 4px;
	}

	/* Price context section */
	.price-context {
		margin: 8px 0;
		font-size: 0.875rem;
	}
	.price-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		color: var(--color-lab-text-secondary);
	}
	.quick-sell {
		color: var(--color-lab-text-secondary);
		opacity: 0.7;
	}
	.range-context {
		margin-top: 6px;
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
	}
	.range-label {
		color: var(--color-lab-text-secondary);
	}
	.range-sep {
		margin: 0 4px;
		opacity: 0.5;
	}
	.range-bar {
		width: 100%;
		height: 4px;
		background: rgba(42, 45, 55, 0.8);
		border-radius: 2px;
		margin-top: 4px;
		overflow: hidden;
	}
	.range-fill {
		height: 100%;
		background: #60a5fa;
		border-radius: 2px;
		transition: width 0.3s ease;
	}
	.listing-warning {
		color: var(--color-lab-red);
		font-size: 0.875rem;
		font-weight: 600;
		margin-top: 8px;
		padding: 6px 10px;
		background: rgba(239, 68, 68, 0.08);
		border-left: 3px solid var(--color-lab-red);
	}
	.anomaly-banner {
		color: var(--color-lab-yellow, #eab308);
		font-size: 0.75rem;
		font-style: italic;
		margin-top: 4px;
	}
</style>
