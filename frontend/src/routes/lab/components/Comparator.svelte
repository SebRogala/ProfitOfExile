<script lang="ts">
	import { fetchGemNames, fetchCompare, type CompareGem } from '$lib/api';
	import { baseGemName, baseGemTradeUrl } from '$lib/trade-utils';
	import { lookupTrade, pollTradeResult, registerTradeListener, type TradeLookupResult, type TradeSignals } from '$lib/tradeApi';
	import { METRIC_TOOLTIPS } from '$lib/tooltips';
	import { getPairCode, clearPairCode, subscribeToDesktopGems } from '$lib/desktopBridge';
	import SignalBadge from './SignalBadge.svelte';
	import Sparkline from './Sparkline.svelte';
	import GemIcon from './GemIcon.svelte';
	import Select from '$lib/components/Select.svelte';

	let {
		league = '',
		refreshKey = 0,
		onQueueGem,
		desktopPair = null,
		onDesktopDisconnect,
	}: {
		league?: string;
		refreshKey?: number;
		onQueueGem?: (gem: string, variant: string, roi: number, tradeData: TradeLookupResult | null) => void;
		desktopPair?: string | null;
		onDesktopDisconnect?: () => void;
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

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];
	const VARIANT_OPTIONS = VARIANTS.map((v) => ({ value: v, label: v }));

	// --- Desktop pairing state ---
	let desktopConnected = $state(false);
	let activePairCode = $derived(desktopPair || getPairCode());

	$effect(() => {
		const code = desktopPair || getPairCode();
		if (!code) return;

		const unsub = subscribeToDesktopGems(
			(gems, detectedVariant) => {
				// Set variant if different
				if (VARIANTS.includes(detectedVariant) && detectedVariant !== variant) {
					variant = detectedVariant;
				}
				// Replace selected gems with detected ones (up to 3)
				selectedGems = gems.slice(0, 3);
				loadResults();
				fetchTradeDataForAll();
			},
			(connected) => {
				desktopConnected = connected;
			}
		);

		return () => {
			unsub();
			desktopConnected = false;
		};
	});

	function disconnectDesktop() {
		clearPairCode();
		desktopConnected = false;
		onDesktopDisconnect?.();
	}

	let variant = $state('20/20');
	let searchQuery = $state('');
	let suggestions = $state<string[]>([]);
	let selectedGems = $state<string[]>([]);
	let results = $state<CompareGem[]>([]);
	let showDropdown = $state(false);
	let highlightedIndex = $state(-1);
	let inputRef = $state<HTMLInputElement | null>(null);

	let tradeData = $state<Record<string, TradeLookupResult | null>>({});
	let tradeLoading = $state<Record<string, { loading: boolean; waitSeconds?: number }>>({});
	let tradeExpanded = $state<Record<string, boolean>>({});
	let tradeGeneration = $state<Record<string, number>>({});
	let autoTradeDisabled = $state(true);

	const TRADE_COOLDOWN_MS = 15 * 60 * 1000; // 15 minutes

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

	const TRADE_STALE_MS = 2 * 60 * 1000; // 2 minutes — visual staleness indicator

	function canRefreshTrade(gem: string): boolean {
		const age = tradeCacheAge(gem);
		if (age == null) return true;
		return age >= TRADE_COOLDOWN_MS;
	}

	function isTradeCacheStale(gem: string): boolean {
		const age = tradeCacheAge(gem);
		if (age == null) return false;
		return age >= TRADE_STALE_MS;
	}

	function tradeCooldownRemaining(gem: string): string {
		const age = tradeCacheAge(gem);
		if (age == null || age >= TRADE_COOLDOWN_MS) return '';
		const remaining = Math.ceil((TRADE_COOLDOWN_MS - age) / 60000);
		return `${remaining}m`;
	}

	/** Fetch trade data — full lookup with async wait for GGG response.
	 *  Uses a generation counter so cancelled requests don't overwrite current state. */
	async function fetchTradeData(gem: string, force = false) {
		const gen = (tradeGeneration[gem] || 0) + 1;
		tradeGeneration[gem] = gen;
		tradeLoading[gem] = { loading: true };
		const isStale = () => tradeGeneration[gem] !== gen;

		try {
			const { immediate, requestId } = await lookupTrade(gem, variant, force);
			if (isStale()) return;
			if (immediate) {
				tradeData[gem] = immediate;
				tradeLoading[gem] = { loading: false };
			} else if (requestId) {
				let resolved = false;
				const unsub = registerTradeListener(requestId, {
					onWait: (s) => { if (!isStale()) tradeLoading[gem] = { loading: true, waitSeconds: s }; },
					onReady: (data) => { resolved = true; if (!isStale()) { tradeData[gem] = data; tradeLoading[gem] = { loading: false }; } unsub(); },
					onError: () => { resolved = true; if (!isStale()) tradeLoading[gem] = { loading: false }; unsub(); },
				});
				// Polling fallback when Mercure SSE isn't connected
				pollTradeResult(gem, variant).then((data) => {
					if (!resolved && !isStale() && data) {
						resolved = true;
						tradeData[gem] = data;
						tradeLoading[gem] = { loading: false };
						unsub();
					}
				}).catch((err) => {
					console.warn(`[Trade] Poll failed for ${gem}:`, err);
				});
			} else {
				tradeLoading[gem] = { loading: false };
			}
		} catch (err) {
			console.warn(`[Trade] Lookup failed for ${gem}:`, err);
			if (!isStale()) tradeLoading[gem] = { loading: false };
		}
	}

	/** Cache-only lookup — returns backend LRU data without triggering a GGG request.
	 *  Returns 204 on cache miss (no API call queued). */
	async function fetchTradeCacheOnly(gem: string) {
		try {
			const { immediate } = await lookupTrade(gem, variant, false, true);
			if (immediate) {
				tradeData[gem] = immediate;
			}
		} catch (err) {
			console.warn(`[Trade] Cache lookup failed for ${gem}:`, err);
		}
	}

	function fetchTradeDataForAll() {
		for (const gem of selectedGems) {
			if (autoTradeDisabled) {
				fetchTradeCacheOnly(gem);
			} else {
				fetchTradeData(gem);
			}
		}
	}

	async function loadResults() {
		const active = selectedGems.filter(Boolean);
		if (active.length === 0) {
			results = [];
			return;
		}
		try {
			results = await fetchCompare(active, variant);
		} catch (err) {
			console.error('[Comparator] Failed to load results:', err);
			results = [];
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
		if (autoTradeDisabled) {
			fetchTradeCacheOnly(name);
		} else {
			fetchTradeData(name);
		}
		setTimeout(() => inputRef?.focus(), 0);
	}

	function removeGem(index: number) {
		const removed = selectedGems[index];
		selectedGems = selectedGems.filter((_, i) => i !== index);
		if (removed) {
			delete tradeData[removed];
			delete tradeLoading[removed];
			delete tradeExpanded[removed];
		}
		loadResults();
		setTimeout(() => inputRef?.focus(), 0);
	}

	function clearAll() {
		selectedGems = [];
		results = [];
		tradeData = {};
		tradeLoading = {};
		tradeExpanded = {};
		searchQuery = '';
		suggestions = [];
		showDropdown = false;
		highlightedIndex = -1;
		setTimeout(() => inputRef?.focus(), 0);
	}

	function handleVariantChange() {
		loadResults();
		fetchTradeDataForAll();
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
		fetchTradeData(gem, true);
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
			{#if activePairCode}
				<span class="desktop-badge" class:desktop-connected={desktopConnected}>
					<span class="desktop-dot"></span>
					Desktop ({activePairCode})
					<button class="desktop-disconnect" onclick={disconnectDesktop} title="Disconnect desktop pairing">&#215;</button>
				</span>
			{/if}
		</div>
		<div class="header-controls">
			{#if selectedForQueue && onQueueGem}
				<button class="next-btn" onclick={handleNext}>&#10003; Next</button>
			{/if}
			{#if selectedGems.length > 0}
				<button class="clear-btn" onclick={clearAll}>Clear All</button>
			{/if}
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
						<span class="price-display" title="Ninja price vs risk-adjusted (sell probability × stability)">
							<span class="price-raw">{gem.transPrice}c</span>
							{#if gem.riskAdjustedPrice > 0}
								<span class="price-risk-adj">(<span class="price-risk-label">Risk-adjusted:</span> {gem.riskAdjustedPrice}c)</span>
							{/if}
						</span>
						<span class="cv" title={METRIC_TOOLTIPS.CV}>CV: {gem.cv}%</span>
					</div>
					<div class="card-row small listings-line">
						<span>{gem.transListings} listings</span>
						<span class="velocity-inline" title="Price change over last 12 hours">({gem.transVelocity > 0 ? '+' : ''}{gem.transVelocity * 12}c /12h)</span>
						<span class="liq" title="Liquidity tier">{gem.liquidityTier}</span>
						<span class="signals-right">
							<SignalBadge signal={gem.signal} />
							{#if gem.sellConfidence}
								<span title={gem.sellConfidenceReason || ''}>
									<SignalBadge signal={gem.sellConfidence} type="confidence" />
								</span>
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
								<span class="range-label">7d floor: {gem.low7d}c</span>
								<span class="range-sep">&middot;</span>
								<span class="range-label">7d high: {gem.high7d}c</span>
								<div class="range-bar">
									<div class="range-fill" style="width: {Math.min(100, Math.max(0, gem.histPosition))}%"></div>
								</div>
							</div>
						{/if}
					</div>
					{#if gem.signal === 'DUMPING' || gem.signal === 'TRAP'}
						<div class="listing-warning">
							{gem.signal === 'DUMPING' ? 'Listings rising — price may drop' : 'Price trap — avoid'}
						</div>
					{/if}

					<div class="urgency-slot">
						{#if gem.sellUrgency}
							<div class="urgency-banner {urgencyClass(gem.sellUrgency)}">
								<span class="urgency-label">{gem.sellUrgency.replace('_', ' ')}</span>
								{#if gem.sellReason}<span class="urgency-reason">{gem.sellReason}</span>{/if}
							</div>
						{/if}
					</div>

					<div class="sellability-row">
						<div class="sellability-bar">
							<div class="sellability-fill {sellabilityClass(gem.sellabilityLabel)}" style="width: {gem.sellability}%"></div>
						</div>
						<span class="sellability-label {sellabilityClass(gem.sellabilityLabel)}">{gem.sellabilityLabel}</span>
						<span class="sellability-score">{gem.sellability}</span>
					</div>

					{#if gem.tierAction}
						<div class="tier-action">{gem.tierAction}</div>
					{/if}

					<!-- Trade Data Section -->
					<div class="trade-section">
						{#if tradeLoading[gem.name]?.loading && !tradeData[gem.name]}
							<div class="trade-loading">
								<span class="trade-spinner"></span>
								{#if tradeLoading[gem.name].waitSeconds != null && tradeLoading[gem.name].waitSeconds > 0}
									<span class="trade-loading-text">Waiting {tradeLoading[gem.name].waitSeconds}s...</span>
								{:else}
									<span class="trade-loading-text">Fetching trade data...</span>
								{/if}
								<button class="trade-cancel-btn" onclick={() => { tradeGeneration[gem.name] = (tradeGeneration[gem.name] || 0) + 1; tradeLoading[gem.name] = { loading: false }; }} title="Cancel trade request">&#215;</button>
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
										<span class="trade-floor-price" title="Cheapest divine listing">{fmtPrice(divFloor.price)} div</span>
									{/if}
									{#if chaosFloor}
										{#if divFloor}<span class="trade-floor-sep">/</span>{/if}
										<span class="trade-floor-price" title="Cheapest chaos listing">{fmtPrice(chaosFloor.chaosPrice)}c</span>
									{/if}
									{#if !divFloor && !chaosFloor}
										<span class="trade-floor-price">{fmtPrice(td.priceFloor)}c</span>
									{/if}
									<span class="trade-floor-label">trade floor</span>
									<span class="trade-delta" class:trade-delta-positive={td.priceFloor - gem.roi > 0} class:trade-delta-negative={td.priceFloor - gem.roi < 0}>
										{td.priceFloor - gem.roi > 0 ? '+' : ''}{fmtPrice(td.priceFloor - gem.roi)}c vs ninja
									</span>
									<span class="trade-signal-badge {concentrationClass(td.signals.sellerConcentration)}"
										title="Seller concentration: {td.signals.uniqueAccounts} unique accounts in top 10">
										{td.signals.sellerConcentration}
									</span>
									{#if td.signals.priceOutlier}
										<span class="trade-signal-badge signal-red" title="Cheapest listing is significantly below median">OUTLIER</span>
									{/if}
								</div>
								<div class="trade-meta-row">
									<span class="trade-listings-badge">{td.total} listings</span>
									<span class="trade-median" title="Median price of top 10 listings">median: {fmtPrice(td.medianTop10)}c</span>
									<span class="trade-actions">
										<span class="trade-cache-age" class:trade-cache-stale={isTradeCacheStale(gem.name)} title="Time since last trade lookup">{tradeCacheAgeStr(gem.name)}</span>
										{#if tradeLoading[gem.name]?.loading}
											<span class="trade-spinner-inline"></span>
											<button class="trade-cancel-btn-inline" onclick={() => { tradeGeneration[gem.name] = (tradeGeneration[gem.name] || 0) + 1; tradeLoading[gem.name] = { loading: false }; }} title="Cancel">&#215;</button>
										{:else}
											<button
												class="trade-action-btn"
												class:trade-refresh-stale={isTradeCacheStale(gem.name)}
												title={canRefreshTrade(gem.name) ? 'Refresh trade data' : `Cooldown: ${tradeCooldownRemaining(gem.name)} remaining`}
												onclick={() => refreshTradeData(gem.name)}
											>&#8635;</button>
										{/if}
										{#if gem.name.includes(' of ')}
											<a class="trade-action-btn trade-base-link" href={baseGemTradeUrl(gem.name, variant, league)} target="_blank" title="Buy base gem: {baseGemName(gem.name)}">Buy Base</a>
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
						<span title="poe.ninja price trend — last 12 hours (30-min snapshots)">
						<Sparkline data={gem.sparkline} width={280} height={32} />
					</span>
					</div>
					<div class="history">
						{#each gem.signalHistory as h}
							<div class="history-line">
								<span class="hist-time">{h.time}</span>
								<SignalBadge signal={h.from} />
								<span class="hist-arrow">&#8594;</span>
								<SignalBadge signal={h.to} />
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
	.desktop-badge {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--color-lab-text-secondary);
		background: rgba(100, 100, 120, 0.15);
		border: 1px solid rgba(100, 100, 120, 0.3);
		padding: 3px 10px;
		border-radius: 12px;
		white-space: nowrap;
	}
	.desktop-badge.desktop-connected {
		color: var(--color-lab-green, #22c55e);
		background: rgba(34, 197, 94, 0.1);
		border-color: rgba(34, 197, 94, 0.3);
	}
	.desktop-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-lab-text-secondary);
	}
	.desktop-connected .desktop-dot {
		background: var(--color-lab-green, #22c55e);
		box-shadow: 0 0 4px rgba(34, 197, 94, 0.5);
	}
	.desktop-disconnect {
		background: none;
		border: none;
		color: inherit;
		font-size: 0.875rem;
		cursor: pointer;
		padding: 0 0 0 2px;
		line-height: 1;
		opacity: 0.6;
	}
	.desktop-disconnect:hover {
		opacity: 1;
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
	.trade-cache-stale {
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
		padding: 4px 10px;
		font-size: 0.8rem;
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
		gap: 10px;
		font-size: 0.8125rem;
	}
	.trade-listings-badge {
		background: rgba(59, 130, 246, 0.12);
		color: #93c5fd;
		padding: 2px 8px;
		font-weight: 600;
		font-size: 0.75rem;
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
</style>
