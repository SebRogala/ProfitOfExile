<script lang="ts">
	import { fetchGemNames, fetchCompare, type CompareGem } from '$lib/api';
	import { METRIC_TOOLTIPS } from '$lib/tooltips';
	import SignalBadge from './SignalBadge.svelte';
	import Sparkline from './Sparkline.svelte';

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];

	let variant = $state('20/20');
	let searchInputs = $state(['', '', '']);
	let suggestions = $state<string[][]>([[], [], []]);
	let selectedGems = $state<string[]>(['Spark of Nova', 'Ball Lightning of Static', 'Arc of Surging']);
	let results = $state<CompareGem[]>([]);
	let showSuggestions = $state([false, false, false]);

	async function loadResults() {
		const active = selectedGems.filter(Boolean);
		if (active.length === 0) {
			results = [];
			return;
		}
		results = await fetchCompare(active, variant);
	}

	async function handleSearch(index: number, query: string) {
		searchInputs[index] = query;
		if (query.length < 2) {
			suggestions[index] = [];
			return;
		}
		suggestions[index] = await fetchGemNames(query);
		showSuggestions[index] = true;
	}

	function selectGem(index: number, name: string) {
		selectedGems[index] = name;
		searchInputs[index] = name;
		showSuggestions[index] = false;
		loadResults();
	}

	function recLabel(rec: string): string {
		if (rec === 'BEST') return '\u2705 BEST';
		if (rec === 'AVOID') return '\ud83d\udeab AVOID';
		return '\ud83d\udc41 OK';
	}

	function recClass(rec: string): string {
		if (rec === 'BEST') return 'rec-best';
		if (rec === 'AVOID') return 'rec-avoid';
		return 'rec-ok';
	}

	function velocityStr(v: number): string {
		if (v > 0) return `\u2191${v}`;
		if (v < 0) return `\u2193${Math.abs(v)}`;
		return '0';
	}

	// Load initial comparison
	loadResults();
</script>

<section class="section">
	<div class="section-header">
		<h2 class="section-title">Lab Options Comparator</h2>
		<div class="variant-select">
			<span class="select-label">Variant:</span>
			<select bind:value={variant} onchange={() => loadResults()} class="select-input">
				{#each VARIANTS as v}
					<option value={v}>{v}</option>
				{/each}
			</select>
		</div>
	</div>

	<div class="search-row">
		{#each [0, 1, 2] as i}
			<div class="search-wrapper">
				<input
					type="text"
					class="search-input"
					placeholder="Search gem {i + 1}..."
					value={searchInputs[i]}
					oninput={(e) => handleSearch(i, (e.target as HTMLInputElement).value)}
					onfocus={() => { if (suggestions[i].length) showSuggestions[i] = true; }}
					onblur={() => setTimeout(() => { showSuggestions[i] = false; }, 200)}
				/>
				{#if showSuggestions[i] && suggestions[i].length > 0}
					<div class="suggestions">
						{#each suggestions[i] as name}
							<button class="suggestion" onmousedown={() => selectGem(i, name)}>
								{name}
							</button>
						{/each}
					</div>
				{/if}
			</div>
		{/each}
	</div>

	{#if results.length > 0}
		<div class="cards-row">
			{#each results as gem}
				<div class="compare-card">
					<div class="card-name">{gem.name}</div>
					<div class="card-row">
						<span class="roi" title={METRIC_TOOLTIPS.ROI}>{gem.roi}c</span>
						<span class="roi-pct" title={METRIC_TOOLTIPS['ROI%']}>({gem.roiPercent}%)</span>
						<span class="color-badge color-{gem.color.toLowerCase()}">{gem.color}</span>
					</div>
					<div class="card-row">
						<SignalBadge signal={gem.signal} />
						<span class="cv" title={METRIC_TOOLTIPS.CV}>CV: {gem.cv}%</span>
					</div>
					<div class="card-row small">
						<span>Trans: {gem.transListings} lst {velocityStr(gem.transVelocity)}/2h</span>
					</div>
					<div class="card-row small">
						<span>Base: {gem.baseListings} lst {velocityStr(gem.baseVelocity)}/2h</span>
						<span class="liq" title="Liquidity tier">{gem.liquidityTier}</span>
					</div>
					<div class="card-row">
						<span class="window-label">Window:</span>
						<SignalBadge signal={gem.windowSignal} type="window" />
					</div>
					<div class="card-row">
						<Sparkline data={gem.sparkline} />
					</div>
					<div class="history">
						{#each gem.signalHistory as h}
							<div class="history-line">{h.time} {h.from}\u2192{h.to} ({h.reason})</div>
						{/each}
					</div>
					<div class="card-rec {recClass(gem.recommendation)}">{recLabel(gem.recommendation)}</div>
				</div>
			{/each}
		</div>
	{/if}
</section>

<style>
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 16px 20px;
		margin-bottom: 16px;
	}
	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 12px;
	}
	.section-title {
		font-size: 0.9375rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.variant-select {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.select-label {
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
	}
	.select-input {
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text);
		padding: 3px 8px;
		font-size: 0.8125rem;
		font-family: inherit;
	}
	.search-row {
		display: flex;
		gap: 12px;
		margin-bottom: 12px;
	}
	.search-wrapper {
		flex: 1;
		position: relative;
	}
	.search-input {
		width: 100%;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text);
		padding: 6px 10px;
		font-size: 0.8125rem;
		font-family: inherit;
		box-sizing: border-box;
	}
	.search-input::placeholder {
		color: var(--color-lab-text-secondary);
	}
	.suggestions {
		position: absolute;
		top: 100%;
		left: 0;
		right: 0;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		z-index: 10;
		max-height: 200px;
		overflow-y: auto;
	}
	.suggestion {
		display: block;
		width: 100%;
		text-align: left;
		padding: 4px 10px;
		color: var(--color-lab-text);
		background: none;
		border: none;
		font-size: 0.8125rem;
		cursor: pointer;
		font-family: inherit;
	}
	.suggestion:hover {
		background: rgba(59, 130, 246, 0.1);
	}
	.cards-row {
		display: flex;
		gap: 12px;
	}
	.compare-card {
		flex: 1;
		border: 1px solid var(--color-lab-border);
		padding: 12px;
		background: var(--color-lab-bg);
	}
	.card-name {
		font-weight: 700;
		font-size: 0.875rem;
		color: var(--color-lab-text);
		margin-bottom: 6px;
	}
	.card-row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 4px;
	}
	.card-row.small {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
	}
	.roi {
		color: var(--color-lab-green);
		font-weight: 700;
		font-size: 0.875rem;
		cursor: help;
	}
	.roi-pct {
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
		cursor: help;
	}
	.color-badge {
		font-size: 0.6875rem;
		font-weight: 700;
		padding: 1px 5px;
	}
	.color-red { color: var(--color-lab-red); background: rgba(239,68,68,0.1); }
	.color-green { color: var(--color-lab-green); background: rgba(34,197,94,0.1); }
	.color-blue { color: var(--color-lab-blue); background: rgba(59,130,246,0.1); }
	.cv {
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
		cursor: help;
	}
	.liq {
		font-size: 0.6875rem;
		font-weight: 600;
		color: var(--color-lab-yellow);
	}
	.window-label {
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
	}
	.history {
		margin: 6px 0;
	}
	.history-line {
		font-size: 0.6875rem;
		color: var(--color-lab-text-secondary);
		line-height: 1.4;
	}
	.card-rec {
		font-size: 0.8125rem;
		font-weight: 700;
		margin-top: 6px;
		padding: 3px 0;
	}
	.rec-best { color: var(--color-lab-green); }
	.rec-ok { color: var(--color-lab-yellow); }
	.rec-avoid { color: var(--color-lab-red); }
</style>
