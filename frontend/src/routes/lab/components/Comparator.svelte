<script lang="ts">
	import { fetchGemNames, fetchCompare, type CompareGem } from '$lib/api';
	import { lookupTrade, pollTradeResult, registerTradeListener, type TradeLookupResult, type TradeSignals } from '$lib/tradeApi';
	import { METRIC_TOOLTIPS } from '$lib/tooltips';
	import SignalBadge from './SignalBadge.svelte';
	import Sparkline from './Sparkline.svelte';
	import GemIcon from './GemIcon.svelte';
	import Select from '$lib/components/Select.svelte';

	let { league = '', refreshKey = 0 }: { league?: string; refreshKey?: number } = $props();

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];
	const VARIANT_OPTIONS = VARIANTS.map((v) => ({ value: v, label: v }));

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

	async function fetchTradeData(gem: string) {
		tradeLoading[gem] = { loading: true };
		try {
			const { immediate, requestId } = await lookupTrade(gem, variant);
			if (immediate) {
				tradeData[gem] = immediate;
				tradeLoading[gem] = { loading: false };
			} else if (requestId) {
				let resolved = false;
				const unsub = registerTradeListener(requestId, {
					onWait: (s) => { tradeLoading[gem] = { loading: true, waitSeconds: s }; },
					onReady: (data) => { resolved = true; tradeData[gem] = data; tradeLoading[gem] = { loading: false }; unsub(); },
					onError: () => { resolved = true; tradeLoading[gem] = { loading: false }; unsub(); },
				});
				// Polling fallback when Mercure SSE isn't connected
				pollTradeResult(gem, variant).then((data) => {
					if (!resolved && data) {
						resolved = true;
						tradeData[gem] = data;
						tradeLoading[gem] = { loading: false };
						unsub();
					}
				});
			} else {
				tradeLoading[gem] = { loading: false };
			}
		} catch {
			tradeLoading[gem] = { loading: false };
		}
	}

	function fetchTradeDataForAll() {
		for (const gem of selectedGems) {
			fetchTradeData(gem);
		}
	}

	async function loadResults() {
		const active = selectedGems.filter(Boolean);
		if (active.length === 0) {
			results = [];
			return;
		}
		results = await fetchCompare(active, variant);
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
		fetchTradeData(name);
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

	function baseGemName(name: string): string {
		const idx = name.lastIndexOf(' of ');
		return idx > 0 ? name.substring(0, idx) : name;
	}

	function baseGemTradeUrl(name: string): string {
		const base = baseGemName(name);
		const parts = variant.split('/');
		const level = parseInt(parts[0]) || 0;
		const quality = parts.length > 1 ? parseInt(parts[1]) : 0;

		const miscFilters: Record<string, any> = { corrupted: { option: 'false' } };
		if (level >= 20) miscFilters.gem_level = { min: level, max: level };
		if (quality === 20) miscFilters.quality = { min: 20, max: 20 };

		const q = {
			query: {
				type: base,
				status: { option: 'securable' },
				filters: {
					type_filters: { filters: { category: { option: 'gem' } } },
					misc_filters: { filters: miscFilters },
					trade_filters: { filters: { sale_type: { option: 'priced' }, collapse: { option: 'true' } } },
				},
			},
			sort: { price: 'asc' },
		};
		return `https://www.pathofexile.com/trade/search/${encodeURIComponent(league || 'Mirage')}?q=${encodeURIComponent(JSON.stringify(q))}`;
	}

	function refreshTradeData(gem: string) {
		tradeData[gem] = null;
		fetchTradeData(gem);
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

</script>

<section class="section">
	<div class="section-header">
		<h2 class="section-title">Lab Options Comparator</h2>
		<div class="header-controls">
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
				<div class="compare-card">
					<div class="card-name-row">
						<GemIcon name={gem.name} size={40} />
						<span class="card-name">{gem.name}</span>
						<span class="tier-badge {tierClass(gem.priceTier)}">{gem.priceTier}</span>
					</div>
					<div class="card-row">
						<span class="roi" title={METRIC_TOOLTIPS.ROI}>{gem.roi}c</span>
						<SignalBadge signal={gem.signal} />
						<span class="cv" title={METRIC_TOOLTIPS.CV}>CV: {gem.cv}%</span>
					</div>
					<div class="card-row small">
						<span>{gem.transListings} listings</span>
						<span class="velocity-inline" title="Price change over last 12 hours">({gem.transVelocity > 0 ? '+' : ''}{gem.transVelocity * 12}c /12h)</span>
						<span class="liq" title="Liquidity tier">{gem.liquidityTier}</span>
					</div>

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
						{#if tradeLoading[gem.name]?.loading}
							<div class="trade-loading">
								<span class="trade-spinner"></span>
								{#if tradeLoading[gem.name].waitSeconds != null && tradeLoading[gem.name].waitSeconds > 0}
									<span class="trade-loading-text">Waiting {tradeLoading[gem.name].waitSeconds}s...</span>
								{:else}
									<span class="trade-loading-text">Fetching trade data...</span>
								{/if}
							</div>
						{:else if tradeData[gem.name]}
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
										<button class="trade-action-btn" title="Refresh trade data" onclick={() => refreshTradeData(gem.name)}>&#8635;</button>
										{#if gem.name.includes(' of ')}
											<a class="trade-action-btn trade-base-link" href={baseGemTradeUrl(gem.name)} target="_blank" title="Buy base gem: {baseGemName(gem.name)}">Buy Base</a>
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
	.clear-btn:hover {
		background: rgba(239, 68, 68, 0.2);
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
		border: 1px solid var(--color-lab-border);
		padding: 24px;
		background: var(--color-lab-bg);
		min-width: 0;
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
	}
	.tier-top { color: #fbbf24; background: rgba(251, 191, 36, 0.12); }
	.tier-mid { color: #94a3b8; background: rgba(148, 163, 184, 0.12); }
	.tier-low { color: #64748b; background: rgba(100, 116, 139, 0.1); }

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
		gap: 4px;
		margin-left: auto;
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
</style>
