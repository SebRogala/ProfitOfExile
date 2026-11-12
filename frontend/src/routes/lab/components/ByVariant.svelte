<script lang="ts">
	import { untrack } from 'svelte';
	import {
		VARIANTS,
		DEDICATION_VARIANTS,
		DEDICATION_POOLS,
		DEDICATION_POOL_LABELS,
		type DedicationPool,
		fetchGemNames,
		fetchRankingsPage,
		type GemPlay,
		type RankingsQuery,
		type RankingsSort,
	} from '$lib/api';
	import BestPlays from './BestPlays.svelte';
	import InfoTooltip from './InfoTooltip.svelte';
	import Select from '$lib/components/Select.svelte';
	import { persisted } from '$lib/prefs.svelte';

	// rankingsRefresh: bumped by the page on a Mercure update; each bump refetches.
	let { rankingsRefresh = 0, league = '', labMode = 'normal' }: { rankingsRefresh?: number; league?: string; labMode?: 'normal' | 'dedication' } = $props();

	const isDedication = $derived(labMode === 'dedication');

	const TABS = ['ALL', ...VARIANTS];
	// One tab per (market, pool): the four rows of the Dedication EV table, in
	// the same order.
	const DEDICATION_TABS = DEDICATION_VARIANTS.flatMap((variant) =>
		DEDICATION_POOLS.map((pool) => ({
			key: `${variant}:${pool}`,
			variant,
			pool,
			label: `${variant} ${DEDICATION_POOL_LABELS[pool]}`,
		})),
	);
	const COLORS = ['ALL', 'RED', 'GREEN', 'BLUE'];
	const LIMIT_OPTIONS = [
		{ value: '10', label: '10' },
		{ value: '20', label: '20' },
		{ value: '50', label: '50' },
	];

	// Persisted picks (ADR-013): every select on this surface is remembered
	// across visits. Unlike the desktop app there is no SSOT store here, so the
	// tabs persist wholesale.
	const activeTab = persisted('rankingsTab', '20/20');
	const activeDedTab = persisted('rankingsDedTab', DEDICATION_TABS[0].key);
	const colorPref = persisted('rankingsColor', 'ALL');
	const limitPref = persisted('rankingsLimit', '20');
	// Persisted (ADR-013): one key, so every rankings table sorts the same way.
	const sortPref = persisted('rankingsSort', 'price');
	const sortBy = $derived<RankingsSort>(
		(['price', 'riskAdjusted', 'roi', 'roiPercent'] as const).find(s => s === sortPref.value) ?? 'price'
	);
	// Default ON. At 20-level variants a thin transfigured market is normal, not
	// anomalous, so the flag covers most of the pool there — a default that hid it
	// would reproduce the ranking gap this toggle exists to make visible (POE-131).
	// An explicit opt-out is still respected; only an unset value defaults on.
	let showLowConf = $state(typeof localStorage === 'undefined' || localStorage.getItem('poe-show-low-conf') !== 'false');
	$effect(() => {
		localStorage.setItem('poe-show-low-conf', String(showLowConf));
	});

	let activeDedTabInfo = $derived(
		DEDICATION_TABS.find(t => t.key === activeDedTab.value) ?? DEDICATION_TABS[0]
	);

	// --- Gem search (one box for every table below it) ---
	//
	// Search lives here rather than in BestPlays because the tables below are
	// each scoped to one market. A per-table search fetched across every variant
	// and then replaced that table's rows wholesale, so a table headed "Best
	// Plays (20/20)" listed all four variants of the matched gem — with the Var
	// column hidden, as four rows that read as identical. On the ALL tab it was
	// worse: four tables, four independent boxes, so typing in one left the
	// other three showing unrelated gems.
	//
	// One query, held here, feeds every table through the same per-market filter
	// the unsearched rows go through. A market with no match renders its usual
	// empty state, which is the useful answer — it says the gem does not sell at
	// that level/quality.
	// --- Rankings fetch ---
	//
	// The server filters, sorts, counts and caps (ADR-030): every control below
	// becomes a request param and the table shows the returned rows in order.
	// The ALL tab is therefore the global top N under the chosen sort.
	const budgetPref = persisted('rankingsBudget', '');
	const budgetChaos = $derived(parseInt(budgetPref.value) > 0 ? parseInt(budgetPref.value) : 0);
	let rankedRows = $state<GemPlay[]>([]);
	let rankingsError = $state('');
	// Rows are shown only under the query they were fetched for: a control change
	// clears them until its own page arrives. A refresh of the same query keeps them.
	let rankingsPending = $state(true);
	let shownQueryKey = '';
	let selectedGem = $state('');
	let fetchGeneration = 0;
	$effect(() => {
		rankingsRefresh;
		const q: RankingsQuery = {
			mode: isDedication ? 'dedication' : 'normal',
			tab: isDedication ? activeDedTabInfo.variant : activeTab.value,
			pool: isDedication ? activeDedTabInfo.pool : undefined,
			color: colorPref.value,
			budget: budgetChaos,
			pageSize: parseInt(limitPref.value) || 20,
			sort: sortBy,
			showLowConf,
			search: selectedGem || undefined,
		};
		const generation = ++fetchGeneration;
		const queryKey = JSON.stringify({ ...q, search: undefined });
		if (!q.search && queryKey !== shownQueryKey) {
			shownQueryKey = queryKey;
			rankedRows = [];
			rankingsPending = true;
		}
		// Debounced: the budget box is a text input.
		const timer = setTimeout(async () => {
			if (q.search) {
				try {
					// The dedication endpoint answers for ONE market per request: omitting
					// `variant` does not mean "all", it silently defaults to 21/23, so a
					// 21/20 tab found nothing. Fan out and merge. Normal mode does return
					// every variant in one response.
					const rows = q.mode === 'dedication'
						? (await Promise.all(DEDICATION_VARIANTS.map((v) => fetchRankingsPage({ ...q, tab: v })))).flatMap((p) => p.rows)
						: (await fetchRankingsPage(q)).rows;
					if (generation === fetchGeneration) searchResults = rows;
				} catch (err) {
					// Without this the rejection was silent and the UI simply never changed.
					console.warn('[ByVariant] gem search failed:', err);
					if (generation !== fetchGeneration) return;
					searchResults = null;
					searchError = `Search for "${q.search}" failed \u2014 check the connection and try again.`;
				}
				return;
			}
			try {
				const page = await fetchRankingsPage(q);
				if (generation !== fetchGeneration) return;
				rankedRows = page.rows;
				rankingsPending = false;
				rankingsError = '';
			} catch (err) {
				console.warn('[ByVariant] rankings fetch failed:', err);
				if (generation !== fetchGeneration) return;
				rankedRows = [];
				rankingsPending = false;
				// The next control change or refresh must fetch again.
				shownQueryKey = '';
				rankingsError = 'Rankings failed to load \u2014 check the connection and try again.';
			}
		}, 300);
		return () => clearTimeout(timer);
	});

	let searchQuery = $state('');
	let searchResults = $state<GemPlay[] | null>(null);
	let searchError = $state('');
	let suggestions = $state<string[]>([]);
	let showDropdown = $state(false);
	let highlightedIndex = $state(-1);
	let searchDebounce: ReturnType<typeof setTimeout> | null = null;

	function clearSearch() {
		searchQuery = '';
		selectedGem = '';
		searchResults = null;
		searchError = '';
		suggestions = [];
		showDropdown = false;
		highlightedIndex = -1;
	}

	function handleSearchInput(query: string) {
		searchQuery = query;
		if (searchDebounce) clearTimeout(searchDebounce);
		if (!query.trim()) {
			clearSearch();
			return;
		}
		if (query.length < 2) {
			suggestions = [];
			showDropdown = false;
			return;
		}
		searchDebounce = setTimeout(async () => {
			if (searchQuery !== query) return;
			let names: string[];
			try {
				names = await fetchGemNames(query, isDedication ? 'dedication' : undefined);
			} catch (err) {
				// A swallowed rejection here presented as "typing does nothing".
				console.warn('[ByVariant] gem name lookup failed:', err);
				return;
			}
			if (searchQuery !== query) return;
			suggestions = names;
			showDropdown = suggestions.length > 0;
			highlightedIndex = suggestions.length === 1 ? 0 : -1;
		}, 100);
	}

	function selectGem(name: string) {
		searchQuery = name;
		suggestions = [];
		showDropdown = false;
		searchError = '';
		selectedGem = name;
	}

	function handleSearchKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' && showDropdown) {
			e.preventDefault();
			highlightedIndex = Math.min(highlightedIndex + 1, suggestions.length - 1);
		} else if (e.key === 'ArrowUp' && showDropdown) {
			e.preventDefault();
			highlightedIndex = Math.max(highlightedIndex - 1, 0);
		} else if (e.key === 'Enter' && highlightedIndex >= 0) {
			e.preventDefault();
			selectGem(suggestions[highlightedIndex]);
		} else if (e.key === 'Escape') {
			clearSearch();
		}
	}

	// Switching lab mode re-fetches under a different mode, so a result carried
	// across would be from the wrong one.
	$effect(() => {
		isDedication;
		// Unconditional: a query typed but not yet picked leaves `suggestions`
		// full of names from the other mode's pool, and picking one then searches a
		// gem that cannot exist in this mode.
		untrack(() => { if (searchQuery || searchResults) clearSearch(); });
	});

	/**
	 * Why a searched gem is missing from the table in front of you.
	 *
	 * The Dedication picker offers names the rankings can never contain: Vaal gems
	 * are a legal feed and deliberately stay in the autocomplete for the compare
	 * surface (internal/lab/repository.go, CorruptedGemNamesAutocomplete), but
	 * isDedicationOutcome excludes them from every ranking. The picker also spans
	 * both pools, so a non-transfigured name is offerable while the Transfigured
	 * tab is active. Measured 2026-08-08 over 77 suggestions from 8 queries: 36 of
	 * them yield nothing on the 21/23 Transfigured tab — 19 Vaal, 17 wrong-pool.
	 *
	 * "No <gem> in this pool" was a wrong answer for all 36: it reads as "this gem
	 * does not sell here", when the truth is either "it is in the other pool" or
	 * "it is never a craft outcome". Say which.
	 */
	function searchMissReason(): string {
		if (!searchResults) return '';
		if (searchResults.length === 0) {
			return `${searchQuery} is not in the rankings \u2014 Vaal gems are a legal feed but never a craft outcome, so they are never ranked.`;
		}
		const pools = [...new Set(searchResults.map((g) => g.baseName).filter(Boolean))];
		const markets = [...new Set(searchResults.map((g) => g.variant).filter(Boolean))].sort();
		const wherePool = pools
			.map((pool) => DEDICATION_POOL_LABELS[pool as DedicationPool] ?? pool)
			.join(' / ');
		if (isDedication && wherePool) {
			return `${searchQuery} is in ${wherePool}${markets.length ? ` at ${markets.join(', ')}` : ''} \u2014 not this tab.`;
		}
		return markets.length
			? `${searchQuery} is ranked at ${markets.join(', ')} \u2014 not this one.`
			: `${searchQuery} is not in the rankings.`;
	}

	// A search returns its gem across every market; each tab keeps only its own.
	// Browsing shows the server's page as returned.
	function playsForVariant(variant: string): GemPlay[] {
		return searchResults ? searchResults.filter(g => g.variant === variant) : rankedRows;
	}

	function playsForAll(): GemPlay[] {
		return searchResults ?? rankedRows;
	}

	// baseName holds "skill" or "transfigured" for Dedication gems, and each row
	// carries the market it was ranked in, so both halves of the tab filter.
	function playsForPool(pool: string, variant: string): GemPlay[] {
		return searchResults ? searchResults.filter(g => g.baseName === pool && g.variant === variant) : rankedRows;
	}

	function setSort(sort: RankingsSort) {
		sortPref.value = sort;
	}

	function setShowLowConf(show: boolean) {
		showLowConf = show;
	}
</script>

<section class="section">
	<div class="section-header">
		<h2 class="section-title">By Variant<InfoTooltip text="<b>Gem Ranking by Variant</b><br><br>Gems sorted by price (default), ROI, or risk-adjusted ROI. Filter by color and toggle low-confidence gems.<br><br><b>Tiers</b> (computed per variant, dynamic boundaries):<br>&nbsp;&nbsp;<span style='color:#fbbf24'>TOP</span> = monopoly outliers (gap-detected from clean pool)<br>&nbsp;&nbsp;<span style='color:#fb923c'>HIGH</span> = premium cluster (within 30% of top gem)<br>&nbsp;&nbsp;<span style='color:#c084fc'>MID-HIGH</span> = worth farming (above 50% of HIGH boundary)<br>&nbsp;&nbsp;<span style='color:#94a3b8'>MID</span> = decent profit<br>&nbsp;&nbsp;<span style='color:#64748b'>LOW</span> = marginal ROI<br>&nbsp;&nbsp;<span style='color:#475569'>FLOOR</span> = below 8% of top-5 average (not worth farming)<br><br><b>Low confidence</b> toggle shows thin-market gems (listings &lt; 40% of median). These may be price manipulation or meta shifts — system can't tell which.<br><br><b>Sort modes</b>: Price (default), Raw ROI, Risk-Adj ROI, ROI%." /></h2>
		<div class="search-wrapper">
			<input
				type="text"
				class="search-input"
				placeholder="Search gem..."
				value={searchQuery}
				oninput={(e) => handleSearchInput(e.currentTarget.value)}
				onkeydown={handleSearchKeydown}
				onfocus={() => { if (suggestions.length) showDropdown = true; }}
				onblur={() => setTimeout(() => { showDropdown = false; }, 200)}
			/>
			{#if searchQuery}
				<button class="search-clear" title="Clear search" onclick={clearSearch}>×</button>
			{/if}
			{#if showDropdown && suggestions.length > 0}
				<div class="dropdown">
					{#each suggestions as gem, i}
						<button
							class="dropdown-item"
							class:highlighted={i === highlightedIndex}
							onmousedown={() => selectGem(gem)}
						>{gem}</button>
					{/each}
				</div>
			{/if}
		</div>
		<label class="budget-label">
			Budget:
			<input
				type="text"
				class="budget-input"
				placeholder="unlimited"
				bind:value={budgetPref.value}
			/>
		</label>
		<div class="limit-select">
			<span class="select-label">Show:</span>
			<Select bind:value={limitPref.value} options={LIMIT_OPTIONS} />
		</div>
		<div class="color-tabs">
			{#each COLORS as color}
				<button
					class="tab color-tab"
					class:active={colorPref.value === color}
					class:c-red={color === 'RED'}
					class:c-green={color === 'GREEN'}
					class:c-blue={color === 'BLUE'}
					onclick={() => { colorPref.value = color; }}
				>
					{#if color !== 'ALL'}<span class="color-dot">●</span>{/if}
					{color}
				</button>
			{/each}
		</div>
		<div class="tabs">
			{#if isDedication}
				{#each DEDICATION_TABS as tab}
					<button
						class="tab"
						class:active={activeDedTab.value === tab.key}
						onclick={() => { activeDedTab.value = tab.key; }}
					>
						{#if activeDedTab.value === tab.key}<span class="tab-dot">●</span>{/if}
						{tab.label}
					</button>
				{/each}
			{:else}
				{#each TABS as tab}
					<button
						class="tab"
						class:active={activeTab.value === tab}
						onclick={() => { activeTab.value = tab; }}
					>
						{#if activeTab.value === tab}<span class="tab-dot">●</span>{/if}
						{tab}
					</button>
				{/each}
			{/if}
		</div>
	</div>

	{#if searchError}
		<div class="search-error">{searchError}</div>
	{:else if rankingsError && !searchResults}
		<div class="search-error">{rankingsError}</div>
	{/if}

	{#if isDedication}
		{@const tab = activeDedTabInfo}
		{@const vd = playsForPool(tab.pool, tab.variant)}
		<div class="variant-block">
			<div class="variant-label">{tab.label}</div>
			<BestPlays plays={vd} title="Dedication Pool ({DEDICATION_POOL_LABELS[tab.pool]}) — {tab.variant}" showVariantColumn={false} {league} sort={sortBy} onSortChange={setSort} {showLowConf} onShowLowConfChange={setShowLowConf} emptyText={searchResults ? searchMissReason() : rankingsPending ? 'Loading…' : 'No data for this pool'} />
		</div>
	{:else if activeTab.value === 'ALL'}
		{@const vd = playsForAll()}
		<div class="variant-block">
			<div class="variant-label">All markets</div>
			<BestPlays plays={vd} title="Best Plays (all markets)" showVariantColumn={true} {league} sort={sortBy} onSortChange={setSort} {showLowConf} onShowLowConfChange={setShowLowConf} emptyText={searchResults ? searchMissReason() : rankingsPending ? 'Loading…' : 'No data available'} />
		</div>
	{:else}
		{@const vd = playsForVariant(activeTab.value)}
		<div class="variant-block">
			<div class="variant-label">{activeTab.value}</div>
			<BestPlays plays={vd} title="Best Plays ({activeTab.value})" showVariantColumn={false} {league} sort={sortBy} onSortChange={setSort} {showLowConf} onShowLowConfChange={setShowLowConf} emptyText={searchResults ? searchMissReason() : rankingsPending ? 'Loading…' : 'No data for this variant'} />
		</div>
	{/if}
</section>

<style>
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 24px;
		margin-bottom: 32px;
	}
	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 16px;
	/* Wrap, and give every child a gap. The row holds five items — title,
	   search, Show, colour tabs, variant tabs — and measured wider than the
	   1024px default window, so a no-wrap row left the search box at 26-82px:
	   two visible characters, with the absolutely-positioned clear button
	   covering all of it. */
	flex-wrap: wrap;
	gap: 8px;
}
	.section-title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.search-wrapper {
		position: relative;
		/* NOT `flex: 1` — that is flex-basis 0, and this row has no free space to
		   distribute, so the box collapsed to its automatic minimum. A real basis
		   keeps it usable and still lets it shrink. */
		flex: 0 1 240px;
		min-width: 180px;
		max-width: 300px;
	}
	.search-input {
		width: 100%;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text);
		padding: 6px 12px;
		font-size: 0.8125rem;
		font-family: inherit;
		box-sizing: border-box;
		outline: none;
	}
	.search-input::placeholder {
		color: var(--color-lab-text-secondary);
	}
	.budget-label {
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.budget-input {
		width: 90px;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text);
		padding: 5px 10px;
		font-size: 0.875rem;
		font-family: inherit;
	}
	.budget-input::placeholder {
		color: var(--color-lab-text-secondary);
	}
	.dropdown {
		position: absolute;
		min-width: 240px;
		top: 100%;
		left: 0;
		right: 0;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		border-top: none;
		max-height: 200px;
		overflow-y: auto;
		z-index: 100;
	}
	.dropdown-item {
		display: block;
		width: 100%;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		padding: 6px 12px;
		text-align: left;
		background: none;
		border: none;
		color: var(--color-lab-text);
		font-size: 0.8125rem;
		cursor: pointer;
	}
	.dropdown-item:hover, .dropdown-item.highlighted {
		background: rgba(255, 255, 255, 0.08);
	}
	.search-clear {
		position: absolute;
		right: 6px;
		top: 50%;
		transform: translateY(-50%);
		background: none;
		border: none;
		color: var(--color-lab-text-secondary);
		font-size: 1rem;
		line-height: 1;
		padding: 0 4px;
		cursor: pointer;
	}
	.search-clear:hover {
		color: var(--color-lab-text);
	}
	.search-error {
		color: #f87171;
		font-size: 0.8125rem;
		margin-bottom: 12px;
	}
	.limit-select {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.select-label {
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
		white-space: nowrap;
	}
	.color-tabs {
		display: flex;
		gap: 4px;
	}
	.color-dot {
		margin-right: 2px;
		font-size: 0.625rem;
	}
	.c-red.active { border-color: var(--color-lab-red); color: var(--color-lab-red); background: rgba(239, 68, 68, 0.1); }
	.c-green.active { border-color: var(--color-lab-green); color: var(--color-lab-green); background: rgba(34, 197, 94, 0.1); }
	.c-blue.active { border-color: var(--color-lab-blue, #3b82f6); color: var(--color-lab-blue, #3b82f6); background: rgba(59, 130, 246, 0.1); }
	.c-red .color-dot { color: var(--color-lab-red); }
	.c-green .color-dot { color: var(--color-lab-green); }
	.c-blue .color-dot { color: var(--color-lab-blue, #3b82f6); }
	.tabs {
		display: flex;
		gap: 4px;
	}
	.tab {
		background: transparent;
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text-secondary);
		padding: 7px 18px;
		font-size: 0.9375rem;
		cursor: pointer;
		font-family: inherit;
	}
	.tab:hover {
		color: var(--color-lab-text);
		border-color: var(--color-lab-text-secondary);
	}
	.tab.active {
		color: var(--color-lab-text);
		border-color: var(--color-lab-blue);
		background: rgba(59, 130, 246, 0.1);
	}
	.tab-dot {
		color: var(--color-lab-blue);
		margin-right: 2px;
		font-size: 0.625rem;
	}
	.variant-block {
		border: 1px solid var(--color-lab-border);
		padding: 24px 28px;
		margin-bottom: 20px;
		background: var(--color-lab-surface);
	}
	.variant-block:last-child {
		margin-bottom: 0;
	}
	.variant-label {
		font-size: 1rem;
		font-weight: 700;
		color: var(--color-lab-blue);
		margin-bottom: 14px;
		border-bottom: 1px solid var(--color-lab-border);
		padding-bottom: 6px;
	}
</style>
