<script lang="ts">
	import { fetchGemNames, fetchCompare, type CompareGem } from '$lib/api';
	import { METRIC_TOOLTIPS } from '$lib/tooltips';
	import SignalBadge from './SignalBadge.svelte';
	import Sparkline from './Sparkline.svelte';

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];

	let variant = $state('20/20');
	let searchQuery = $state('');
	let suggestions = $state<string[]>([]);
	let selectedGems = $state<string[]>(['Spark of Nova', 'Ball Lightning of Static', 'Arc of Surging']);
	let results = $state<CompareGem[]>([]);
	let showDropdown = $state(false);
	let highlightedIndex = $state(-1);
	let inputRef = $state<HTMLInputElement | null>(null);

	async function loadResults() {
		const active = selectedGems.filter(Boolean);
		if (active.length === 0) {
			results = [];
			return;
		}
		results = await fetchCompare(active, variant);
	}

	async function handleInput(query: string) {
		searchQuery = query;
		highlightedIndex = -1;
		if (query.length < 2) {
			suggestions = [];
			showDropdown = false;
			return;
		}
		const allNames = await fetchGemNames(query);
		suggestions = allNames.filter((n) => !selectedGems.includes(n));
		showDropdown = suggestions.length > 0;
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
		// Re-focus the input after selection
		setTimeout(() => inputRef?.focus(), 0);
	}

	function removeGem(index: number) {
		selectedGems = selectedGems.filter((_, i) => i !== index);
		loadResults();
		setTimeout(() => inputRef?.focus(), 0);
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
		if (rec === 'BEST') return '✓';
		if (rec === 'AVOID') return '✗';
		return '•';
	}

	function recClass(rec: string): string {
		if (rec === 'BEST') return 'rec-best';
		if (rec === 'AVOID') return 'rec-avoid';
		return 'rec-ok';
	}

	function velocityStr(v: number): string {
		if (v > 0) return `↑${v}`;
		if (v < 0) return `↓${Math.abs(v)}`;
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

	<div class="comparator-input">
		<div class="tags-row">
			{#each selectedGems as gem, i}
				<span class="tag">
					{gem}
					<button class="tag-remove" onclick={() => removeGem(i)}>×</button>
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
							<div class="history-line">{h.time} {h.from}→{h.to} ({h.reason})</div>
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
		padding: 24px;
		margin-bottom: 32px;
	}
	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 16px;
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
		gap: 8px;
	}
	.select-label {
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
	}
	.select-input {
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text);
		padding: 4px 10px;
		font-size: 0.8125rem;
		font-family: inherit;
	}

	/* Tag-based multi-select */
	.comparator-input {
		margin-bottom: 16px;
	}
	.tags-row {
		display: flex;
		gap: 8px;
		margin-bottom: 8px;
		flex-wrap: wrap;
	}
	.tag {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: #2a2d37;
		color: #e4e4e7;
		padding: 4px 10px;
		font-size: 0.8125rem;
		font-weight: 600;
		border-radius: 2px;
	}
	.tag-remove {
		background: none;
		border: none;
		color: #e4e4e7;
		font-size: 0.875rem;
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
		padding: 8px 12px;
		font-size: 0.8125rem;
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
		max-height: 200px;
		overflow-y: auto;
	}
	.dropdown-item {
		display: block;
		width: 100%;
		text-align: left;
		padding: 6px 12px;
		color: var(--color-lab-text);
		background: none;
		border: none;
		font-size: 0.8125rem;
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
		font-size: 0.8125rem;
		margin: 0;
	}

	.cards-row {
		display: flex;
		gap: 16px;
	}
	.compare-card {
		flex: 1;
		border: 1px solid var(--color-lab-border);
		padding: 24px;
		background: var(--color-lab-bg);
	}
	.card-name {
		font-weight: 700;
		font-size: 0.875rem;
		color: var(--color-lab-text);
		margin-bottom: 8px;
	}
	.card-row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 6px;
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
		margin: 8px 0;
	}
	.history-line {
		font-size: 0.6875rem;
		color: var(--color-lab-text-secondary);
		line-height: 1.4;
	}
	.card-rec {
		font-size: 0.8125rem;
		font-weight: 700;
		margin-top: 8px;
		padding: 4px 0;
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.rec-icon {
		font-size: 0.875rem;
	}
	.rec-best { color: var(--color-lab-green); }
	.rec-ok { color: var(--color-lab-yellow); }
	.rec-avoid { color: var(--color-lab-red); }
</style>
