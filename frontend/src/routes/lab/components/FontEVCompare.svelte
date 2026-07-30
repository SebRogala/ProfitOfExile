<script lang="ts">
	import { fetchFontEV, fetchDedicationEV, DEDICATION_VARIANTS, type FontEVResponse, type FontColor, type DedicationEVResponse, type DedicationColor } from '$lib/api';
	import { baseGemTradeUrl, cheapestCorruptedTradeUrl } from '$lib/trade-utils';
	import { formatPrice, formatPriceSigned } from '$lib/price.svelte';
	import InfoTooltip from './InfoTooltip.svelte';
	import Select from '$lib/components/Select.svelte';

	let { refreshKey = 0, league = '', labMode = 'normal' }: { refreshKey?: number; league?: string; labMode?: 'normal' | 'dedication' } = $props();

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];
	const COLORS = ['RED', 'GREEN', 'BLUE'] as const;

	// --- Normal (Font) mode state ---
	let data = $state<Record<string, FontEVResponse>>({});

	// --- Dedication mode state ---
	// Keyed by variant: 21/23 and 21/20 are separate markets, shown as separate
	// rows of one table rather than one table per selected market.
	let dedicationData = $state<Record<string, DedicationEVResponse>>({});

	let loading = $state(true);
	let loadError = $state(false);

	const isDedication = $derived(labMode === 'dedication');

	// --- Dedication row definitions ---
	const DEDICATION_ROWS = DEDICATION_VARIANTS.flatMap((variant) => [
		{ variant, poolKey: 'skills' as const, label: `${variant} Skill Gems` },
		{ variant, poolKey: 'transfigured' as const, label: `${variant} Transfigured` },
	]);

	// Entry fee is a property of the Dedication offering, not of a gem market —
	// every variant response carries the same number, so the first one answers.
	const entryFee = $derived(
		DEDICATION_VARIANTS.map((v) => dedicationData[v]?.entryFee ?? 0).find((f) => f > 0) ?? 0
	);

	const hasDedicationData = $derived(DEDICATION_VARIANTS.some((v) => dedicationData[v]));

	// Generation guard: a mode switch starts a new load and only the newest one
	// may assign. Both responses match their own request, so without this the
	// slower (older) load wins and nothing downstream can detect it.
	let loadGeneration = 0;

	async function loadAll() {
		const generation = ++loadGeneration;
		loading = true;
		loadError = false;
		try {
			if (isDedication) {
				const results = await Promise.all(
					DEDICATION_VARIANTS.map(async (v) => [v, await fetchDedicationEV(v)] as const)
				);
				if (generation !== loadGeneration) return;
				// The response states the market it was computed over. A server
				// older than this client ignores ?variant= and answers with the
				// default pool, which would otherwise render under the other
				// market's row label.
				const mismatch = results.find(([v, resp]) => resp.variant && resp.variant.replace(/c$/, '') !== v);
				if (mismatch) {
					console.error(`[FontEV] server returned ${mismatch[1].variant} for a ${mismatch[0]} request`);
					dedicationData = {};
					loadError = true;
					loading = false;
					return;
				}
				const next: Record<string, DedicationEVResponse> = {};
				for (const [v, resp] of results) {
					next[v] = resp;
				}
				dedicationData = next;
			} else {
				const results = await Promise.all(
					VARIANTS.map(async (v) => {
						const d = await fetchFontEV(v);
						return [v, d] as const;
					})
				);
				if (generation !== loadGeneration) return;
				for (const [v, d] of results) {
					data[v] = d;
				}
			}
		} catch (err) {
			if (generation !== loadGeneration) return;
			console.error('[FontEV] Failed to load:', err);
			loadError = true;
		}
		if (generation === loadGeneration) loading = false;
	}

	// --- Normal mode helpers ---

	function getColorData(variant: string, color: string, mode: 'safe' | 'premium' | 'jackpot'): FontColor | null {
		const vd = data[variant];
		if (!vd) return null;
		const colors = vd[mode];
		return colors?.find((c) => c.color === color) || null;
	}

	// EV is the same across all modes, just use safe.
	function getEV(variant: string, color: string): number {
		const fc = getColorData(variant, color, 'safe');
		return fc?.evRaw ?? fc?.ev ?? 0;
	}

	function winner(): { variant: string; color: string; ev: number } {
		let best = { variant: '', color: '', ev: -Infinity };
		for (const v of VARIANTS) {
			for (const color of COLORS) {
				const ev = getEV(v, color);
				if (ev > best.ev) {
					best = { variant: v, color, ev };
				}
			}
		}
		return best;
	}

	// --- Dedication mode helpers ---

	function getDedColorData(variant: string, poolKey: 'skills' | 'transfigured', color: string, mode: 'safe' | 'premium' | 'jackpot'): DedicationColor | null {
		const vd = dedicationData[variant];
		if (!vd) return null;
		const pool = vd[poolKey];
		if (!pool) return null;
		const rows = pool[mode];
		return rows?.find((c) => c.color === color) || null;
	}

	function getDedEV(variant: string, poolKey: 'skills' | 'transfigured', color: string): number {
		const dc = getDedColorData(variant, poolKey, color, 'safe');
		return dc?.evRaw ?? 0;
	}

	function getDedInputCost(variant: string, poolKey: 'skills' | 'transfigured', color: string): number {
		const dc = getDedColorData(variant, poolKey, color, 'safe');
		return dc?.inputCost ?? 0;
	}

	function getDedProfit(variant: string, poolKey: 'skills' | 'transfigured', color: string): number {
		const dc = getDedColorData(variant, poolKey, color, 'safe');
		return dc?.profit ?? 0;
	}

	// Best cell across both markets — profit, not EV, since input cost differs
	// per variant and a higher-EV market can still be the worse play.
	//
	// Cells with no analysis row are skipped rather than scored: they read as a
	// profit of 0, which would outrank every real but loss-making cell and put
	// the winner ring on a cell rendering "—".
	function dedWinner(): { variant: string; poolKey: string; color: string; profit: number } {
		let best = { variant: '', poolKey: '', color: '', profit: -Infinity };
		for (const row of DEDICATION_ROWS) {
			for (const color of COLORS) {
				if (getDedEV(row.variant, row.poolKey, color) <= 0) continue;
				const profit = getDedProfit(row.variant, row.poolKey, color);
				if (profit > best.profit) {
					best = { variant: row.variant, poolKey: row.poolKey, color, profit };
				}
			}
		}
		return best;
	}

	// --- Shared helpers ---

	function tierLine(fc: { winners: number; fontsToHit?: number; pWin: number; avgWinRaw?: number; avgWin?: number } | null): string {
		if (!fc || fc.winners === 0 || !fc.fontsToHit) return '\u2014 none';
		const raw = fc.avgWinRaw || fc.avgWin || 0;
		const fth = fc.fontsToHit;
		let ratio: string;
		if (fth <= 1.05) {
			ratio = 'every font';
		} else if (fth < 2) {
			// > 50% hit rate
			const pctFrac = fc.pWin / 100;
			const hits = Math.round(pctFrac * 10);
			const gcd = (a: number, b: number): number => b === 0 ? a : gcd(b, a % b);
			const d = hits > 0 ? gcd(hits, 10) : 1;
			ratio = `${hits / d} in ${10 / d}`;
		} else {
			ratio = `1 in ${Math.round(fth)}`;
		}
		const pct = Math.round(fc.pWin);
		return `${ratio} (${pct}%)  ~${formatPrice(raw)}`;
	}

	/**
	 * Build trade URL for cheapest base gem of a color + variant (Normal mode).
	 */
	function buyBaseUrl(variant: string, color: string): string | null {
		const parts = variant.split('/');
		const level = parseInt(parts[0]) || 0;
		const quality = parts.length > 1 ? parseInt(parts[1]) : 0;

		if (level < 20) return null;

		const reqFilters: Record<string, any> = {};
		if (color === 'RED')   { reqFilters.dex = { max: 97 }; reqFilters.int = { max: 97 }; }
		if (color === 'GREEN') { reqFilters.str = { max: 97 }; reqFilters.int = { max: 97 }; }
		if (color === 'BLUE')  { reqFilters.str = { max: 97 }; reqFilters.dex = { max: 97 }; }

		const miscFilters: Record<string, any> = {
			gem_level: { min: level },
			corrupted: { option: 'false' },
		};
		if (quality > 0) {
			miscFilters.quality = { min: quality };
		} else {
			miscFilters.quality = { max: 0 };
		}

		const q = {
			query: {
				status: { option: 'securable' },
				filters: {
					type_filters: { filters: { category: { option: 'gem.activegem' } } },
					req_filters: { filters: reqFilters },
					misc_filters: { filters: miscFilters },
					trade_filters: { filters: { sale_type: { option: 'priced' }, collapse: { option: 'true' } } },
				},
			},
			sort: { price: 'asc' },
		};
		return `https://www.pathofexile.com/trade/search/${encodeURIComponent(league || 'Mirage')}?q=${encodeURIComponent(JSON.stringify(q))}`;
	}

	function baseGemNoQualityUrl(gemName: string): string {
		const base = gemName.lastIndexOf(' of ') > 0 ? gemName.substring(0, gemName.lastIndexOf(' of ')) : gemName;
		const q = {
			query: {
				type: base,
				status: { option: 'securable' },
				filters: {
					type_filters: { filters: { category: { option: 'gem' } } },
					misc_filters: { filters: { gem_level: { min: 20 }, corrupted: { option: 'false' } } },
					trade_filters: { filters: { sale_type: { option: 'priced' }, collapse: { option: 'true' } } },
				},
			},
			sort: { price: 'asc' },
		};
		return `https://www.pathofexile.com/trade/search/${encodeURIComponent(league || 'Mirage')}?q=${encodeURIComponent(JSON.stringify(q))}`;
	}

	// Probability of getting at least 1 winner in 3 picks without replacement.
	function pWin3(winners: number, total: number): number {
		if (winners <= 0 || total < 3) return 0;
		if (winners >= total) return 1;
		const losers = total - winners;
		return 1 - (losers / total) * ((losers - 1) / (total - 1)) * ((losers - 2) / (total - 2));
	}

	let showPool = $state(false);
	let poolVariant = $state('20/20');
	// One selector over the four (market, pool) combinations — "21/23:skills".
	let dedPoolSel = $state(`${DEDICATION_ROWS[0].variant}:${DEDICATION_ROWS[0].poolKey}`);
	const dedPool = $derived.by(() => {
		const [variant, poolKey] = dedPoolSel.split(':');
		return { variant, poolKey: poolKey as 'skills' | 'transfigured' };
	});

	function getPoolBreakdown(variant: string, color: string): { tier: string; count: number; minPrice: number; maxPrice: number }[] {
		const safe = getColorData(variant, color, 'safe');
		return safe?.poolBreakdown || [];
	}

	function getDedPoolBreakdown(variant: string, poolKey: 'skills' | 'transfigured', color: string): { tier: string; count: number; minPrice: number; maxPrice: number }[] {
		const dc = getDedColorData(variant, poolKey, color, 'safe');
		return dc?.poolBreakdown || [];
	}

	const ALL_TIERS = ['TOP', 'HIGH', 'MID-HIGH', 'MID', 'LOW', 'FLOOR'];
	const TIER_COLORS: Record<string, string> = {
		TOP: '#fbbf24', HIGH: '#fb923c', 'MID-HIGH': '#c084fc',
		MID: '#94a3b8', LOW: '#64748b', FLOOR: '#475569',
	};

	$effect(() => { refreshKey; labMode; loadAll(); });
</script>

<section class="section">
	{#if loading}
		<span class="loading">Loading...</span>
	{:else if loadError}
		<span class="loading">Failed to load data — check connection and refresh.</span>
	{:else if isDedication && hasDedicationData}
		{@const best = dedWinner()}
		{#if entryFee > 0}
			<div class="dedication-header">
				<span class="dedication-title">Dedication Lab — Corrupted Gem Exchange</span>
				<span class="entry-fee">
					Entry fee: <strong>{formatPrice(entryFee)}</strong>
					<InfoTooltip text="<b>Dedication to the Goddess offering price</b><br><br>This is the cost of the offering required to open a Dedication Lab run. It is displayed for reference but <b>NOT included</b> in the profit calculation — profit shows pure gem exchange value minus input gem cost." />
				</span>
			</div>
		{/if}
		<table class="ft ded">
			<thead>
				<tr>
					<th class="var-header">Dedication EV<InfoTooltip text="<b>Dedication Lab — Corrupted Gem Exchange EV</b><br><br>Two font options per run, each at either corrupted variant:<br>• <b>Skill Gems</b>: Non-transfigured corrupted skill gems<br>• <b>Transfigured</b>: Transfigured corrupted skill gems<br><br><b>Nc/font</b>: Expected income per font usage. Based on best-of-3 random draws from the corrupted pool of that color.<br><br><b>Input cost</b>: Average price of the 10 cheapest corrupted gems of that variant in that color pool — this is what you feed into the font.<br><br><b>Profit</b>: EV minus input cost. Entry fee (Dedication offering) is NOT included.<br><br>21/23 and 21/20 are separate markets: pool, tiers and input cost are computed per variant, and the highlighted cell is the best <b>profit</b> across both.<br><br>Thin liquidity is expected for corrupted gems — fewer listings than normal font pools." /></th>
					{#each COLORS as color}
						<th><span class="c-{color.toLowerCase()}">{'\u25CF'} {color}</span></th>
					{/each}
					<th class="buy-header">Buy Input</th>
				</tr>
			</thead>
			<tbody>
				{#each DEDICATION_ROWS as row, i}
					{#if i > 0 && DEDICATION_ROWS[i - 1].variant !== row.variant}
						<tr class="ded-sep"><td colspan={COLORS.length + 2}></td></tr>
					{/if}
					<tr>
						<td class="var">{row.label}</td>
						{#each COLORS as color}
							{@const ev = getDedEV(row.variant, row.poolKey, color)}
							{@const inputCost = getDedInputCost(row.variant, row.poolKey, color)}
							{@const profit = getDedProfit(row.variant, row.poolKey, color)}
							{@const safe = getDedColorData(row.variant, row.poolKey, color, 'safe')}
							{@const premium = getDedColorData(row.variant, row.poolKey, color, 'premium')}
							{@const jackpot = getDedColorData(row.variant, row.poolKey, color, 'jackpot')}
							{@const isW = best.variant === row.variant && best.poolKey === row.poolKey && best.color === color}
							<td class:w-red={isW && color === 'RED'} class:w-green={isW && color === 'GREEN'} class:w-blue={isW && color === 'BLUE'}>
								{#if ev > 0}
									<!-- Headline is gain, not gross: the base gem has to be bought,
									     so EV alone reads as income that nobody actually keeps. -->
									<span class="ev" class:ev-loss={profit <= 0} class:best-red={isW && color === 'RED'} class:best-green={isW && color === 'GREEN'} class:best-blue={isW && color === 'BLUE'}>
										{formatPriceSigned(profit)} <span class="ev-unit">gain</span>
									</span>
									<div class="ded-cost-line">
										<span class="ded-input">base gem: {formatPrice(inputCost)}</span>
										<span class="ded-gross" title="Gross font value — what a usage returns before the base gem is paid for. This is your gain only if the base was self-farmed.">{formatPrice(ev)}/font gross</span>
									</div>
									<div class="tier-lines">
										<div class="tier-row">
											<span class="tier-label t-safe">Safe</span>
											<span class="tier-val t-safe">{tierLine(safe)}</span>
										</div>
										<div class="tier-row">
											<span class="tier-label t-premium">Premium</span>
											<span class="tier-val t-premium">{tierLine(premium)}</span>
										</div>
										{#if jackpot && jackpot.winners > 0}
										{@const gemList = (jackpot.jackpotGems || []).map(g => {
											return `<div style="padding:4px 0;border-bottom:1px solid rgba(255,255,255,0.06)"><b>${g.name}</b>: ${formatPrice(g.chaos)}</div>`;
										}).join('')}
										<div class="tier-row">
											<span class="tier-label t-jackpot">Jackpot</span>
											<span class="tier-val t-jackpot">{tierLine(jackpot)}</span>
											<InfoTooltip text={gemList} />
										</div>
										{/if}
									</div>
								{:else}
									<span class="nil">{'\u2014'}</span>
								{/if}
							</td>
						{/each}
						<td class="buy-col">
							<div class="buy-buttons">
								{#each COLORS as color}
									{@const url = cheapestCorruptedTradeUrl(color, row.poolKey === 'transfigured', league || '', row.variant)}
									<a
										class="buy-btn buy-{color.toLowerCase()}"
										href={url}
										target="_blank"
										title="Buy cheapest corrupted {row.variant} {color} {row.poolKey === 'transfigured' ? 'transfigured ' : ''}gems"
									>{color}</a>
								{/each}
							</div>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>

		<button class="pool-toggle" onclick={() => { showPool = !showPool; }}>
			{showPool ? '\u25BC' : '\u25B6'} Pool Overview
		</button>

		{#if showPool}
			<div class="pool-section">
				<div class="pool-variant-select">
					<span class="pool-select-label">Pool:</span>
					<Select bind:value={dedPoolSel} options={DEDICATION_ROWS.map(r => ({ value: `${r.variant}:${r.poolKey}`, label: r.label }))} />
				</div>
				<div class="pool-grid">
					{#each COLORS as color}
						{@const breakdown = getDedPoolBreakdown(dedPool.variant, dedPool.poolKey, color)}
						{@const safe = getDedColorData(dedPool.variant, dedPool.poolKey, color, 'safe')}
						{@const totalGems = safe?.pool || breakdown.reduce((s, t) => s + t.count, 0)}
						<div class="pool-color-card">
							<div class="pool-color-header c-{color.toLowerCase()}">{color} <span class="pool-count">{totalGems} gems</span></div>
							{#each ALL_TIERS as tierName}
								{@const tier = breakdown.find(t => t.tier === tierName)}
								{@const tierPWin = tier ? Math.round(pWin3(tier.count, totalGems) * 100) : 0}
								<div class="pool-tier-row" class:pool-tier-empty={!tier}>
									<span class="pool-tier-name" style="color: {TIER_COLORS[tierName] || '#94a3b8'}">{tierName}</span>
									{#if tier}
										<span class="pool-tier-count">{tier.count}</span>
										<span class="pool-tier-range">
											{#if tier.minPrice === tier.maxPrice}
												{tier.minPrice}c
											{:else}
												{formatPrice(tier.minPrice)} — {formatPrice(tier.maxPrice)}
											{/if}
										</span>
										<span class="pool-tier-bar">
											<span class="pool-tier-bar-fill" style="width: {tierPWin}%; background: {TIER_COLORS[tierName] || '#94a3b8'}"></span>
										</span>
										<span class="pool-tier-pwin">{tierPWin}%</span>
									{:else}
										<span class="pool-tier-count">—</span>
										<span class="pool-tier-range"></span>
										<span class="pool-tier-bar"></span>
										<span class="pool-tier-pwin"></span>
									{/if}
								</div>
							{/each}
							{#if safe?.lowConfidenceGems?.length}
								{@const lcGems = safe.lowConfidenceGems}
								{@const lcTooltip = lcGems.map(g => `<b>${g.name}</b>: ${formatPrice(g.chaos)} (${g.listings} listings)`).join('<br>')}
								<div class="pool-tier-row pool-risky-row">
									<span class="pool-tier-name pool-risky-name">RISKY</span>
									<span class="pool-tier-count">{lcGems.length}</span>
									<span class="pool-tier-range">excluded from EV</span>
									<InfoTooltip text="<b>Low confidence gems</b> — very few listings relative to normal. Could be a new meta build (demand spike) or price manipulation (buyout). Excluded from EV calculation.<br><br>{lcTooltip}" />
								</div>
							{/if}
						</div>
					{/each}
				</div>
			</div>
		{/if}
	{:else if !isDedication}
		{@const best = winner()}
		<table class="ft">
			<thead>
				<tr>
					<th class="var-header">Font EV<InfoTooltip text="<b>Font of Divine Skill — Expected Value per usage</b><br><br><b>Nc/font</b>: Your average income each time you use the Font. Based on best-of-3 random draws — you always pick the most valuable gem.<br><br><b>Highlighted cell</b>: Best color for highest average income.<br><br><b>Hit tiers</b> (using unified per-variant tier system):<br>• <span style='color:#5eead4'>Safe</span>: Any gem above FLOOR tier. 'X in Y' = probability from 3 picks.<br>• <span style='color:#c084fc'>Premium</span>: HIGH + MID-HIGH + TOP gems. Significant profit when hit.<br>• <span style='color:#fbbf24'>Jackpot</span>: TOP-tier monopoly outliers. Rare but massive payout.<br><br>Low-confidence gems (thin market, unreliable prices) are excluded from EV calculation but counted in pool size.<br><br><b>Pool Overview</b>: Expand below to see per-color tier breakdown with hit probabilities.<br><br><b>Tip</b>: With Gift Lab (8 fonts/run), even low Jackpot hit rates become viable through compound probability." /></th>
					{#each COLORS as color}
						<th><span class="c-{color.toLowerCase()}">{'\u25CF'} {color}</span></th>
					{/each}
					<th class="buy-header">Buy Bases</th>
				</tr>
			</thead>
			<tbody>
				{#each VARIANTS as variant}
					<tr>
						<td class="var">{variant}</td>
						{#each COLORS as color}
							{@const ev = getEV(variant, color)}
							{@const safe = getColorData(variant, color, 'safe')}
							{@const premium = getColorData(variant, color, 'premium')}
							{@const jackpot = getColorData(variant, color, 'jackpot')}
							{@const isW = best.variant === variant && best.color === color}
							<td class:w-red={isW && color === 'RED'} class:w-green={isW && color === 'GREEN'} class:w-blue={isW && color === 'BLUE'}>
								{#if ev > 0}
									<span class="ev" class:best-red={isW && color === 'RED'} class:best-green={isW && color === 'GREEN'} class:best-blue={isW && color === 'BLUE'}>{formatPrice(ev)}/font</span>
									<div class="tier-lines">
										<div class="tier-row">
											<span class="tier-label t-safe">Safe</span>
											<span class="tier-val t-safe">{tierLine(safe)}</span>
										</div>
										<div class="tier-row">
											<span class="tier-label t-premium">Premium</span>
											<span class="tier-val t-premium">{tierLine(premium)}</span>
										</div>
										{#if jackpot && jackpot.winners > 0}
										{@const gemList = (jackpot.jackpotGems || []).map(g => {
											let html = `<div style="padding:4px 0;border-bottom:1px solid rgba(255,255,255,0.06)"><b>${g.name}</b>: ${formatPrice(g.chaos)} &nbsp;&nbsp;<a href="${baseGemTradeUrl(g.name, variant, league || '')}" target="_blank" style="padding:1px 8px;font-size:0.75rem;font-weight:600;color:#5eead4;border:1px solid rgba(94,234,212,0.4);text-decoration:none;letter-spacing:0.03em">Buy Base</a>`;
											if ((g.gcpRecipeCost || 0) > 0) {
												const saves = Math.round(g.gcpRecipeSaves || 0);
												const savesColor = saves >= 0 ? '#22c55e' : '#ef4444';
												const savesText = saves >= 0 ? `saves ${formatPrice(saves)}` : `costs ${formatPrice(Math.abs(saves))} more`;
												html += `<div style="margin-top:3px;font-size:0.75rem;color:#94a3b8">GCP recipe: <b>${Math.round(g.gcpRecipeBase || 0)}c</b> base + ${Math.round((g.gcpRecipeCost || 0) - (g.gcpRecipeBase || 0))}c GCPs = <b>${Math.round(g.gcpRecipeCost || 0)}c</b> <span style="color:${savesColor}">(${savesText})</span> &nbsp;<a href="${baseGemNoQualityUrl(g.name)}" target="_blank" style="padding:1px 6px;font-size:0.6875rem;font-weight:600;color:#fbbf24;border:1px solid rgba(251,191,36,0.4);text-decoration:none">Buy 20/0</a></div>`;
											}
											html += `</div>`;
											return html;
										}).join('')}
										<div class="tier-row">
											<span class="tier-label t-jackpot">Jackpot</span>
											<span class="tier-val t-jackpot">{tierLine(jackpot)}</span>
											<InfoTooltip text={gemList} />
										</div>
										{/if}
									</div>
								{:else}
									<span class="nil">{'\u2014'}</span>
								{/if}
							</td>
						{/each}
						<td class="buy-col">
							{#if parseInt(variant) >= 20}
								<div class="buy-buttons">
									{#each COLORS as color}
										{@const url = buyBaseUrl(variant, color)}
										{#if url}
											<a
												class="buy-btn buy-{color.toLowerCase()}"
												href={url}
												target="_blank"
												title="Buy cheapest {color} base gem ({variant})"
											>{color}</a>
										{/if}
									{/each}
								</div>
							{/if}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>

		<button class="pool-toggle" onclick={() => { showPool = !showPool; }}>
			{showPool ? '\u25BC' : '\u25B6'} Pool Overview
		</button>

		{#if showPool}
			<div class="pool-section">
				<div class="pool-variant-select">
					<span class="pool-select-label">Variant:</span>
					<Select bind:value={poolVariant} options={VARIANTS.map(v => ({ value: v, label: v }))} />
				</div>
				<div class="pool-grid">
					{#each COLORS as color}
						{@const breakdown = getPoolBreakdown(poolVariant, color)}
						{@const safe = getColorData(poolVariant, color, 'safe')}
						{@const totalGems = safe?.pool || breakdown.reduce((s, t) => s + t.count, 0)}
						{@const pricedGems = safe?.pricedGems ?? totalGems}
						<div class="pool-color-card">
							<div class="pool-color-header c-{color.toLowerCase()}">{color}
								{#if pricedGems < totalGems}
									<span class="pool-count pool-count-partial" title="Only {pricedGems} of the {totalGems} {color} gems this Font can roll are priced at {poolVariant}. The rest count as outcomes worth 0c, so the EV here is a floor — missing prices, not worthless gems.">{pricedGems} / {totalGems} priced</span>
								{:else}
									<span class="pool-count">{totalGems} gems</span>
								{/if}
							</div>
							{#each ALL_TIERS as tierName}
								{@const tier = breakdown.find(t => t.tier === tierName)}
								{@const tierPWin = tier ? Math.round(pWin3(tier.count, totalGems) * 100) : 0}
								<div class="pool-tier-row" class:pool-tier-empty={!tier}>
									<span class="pool-tier-name" style="color: {TIER_COLORS[tierName] || '#94a3b8'}">{tierName}</span>
									{#if tier}
										<span class="pool-tier-count">{tier.count}</span>
										<span class="pool-tier-range">
											{#if tier.minPrice === tier.maxPrice}
												{tier.minPrice}c
											{:else}
												{formatPrice(tier.minPrice)} — {formatPrice(tier.maxPrice)}
											{/if}
										</span>
										<span class="pool-tier-bar">
											<span class="pool-tier-bar-fill" style="width: {tierPWin}%; background: {TIER_COLORS[tierName] || '#94a3b8'}"></span>
										</span>
										<span class="pool-tier-pwin">{tierPWin}%</span>
									{:else}
										<span class="pool-tier-count">—</span>
										<span class="pool-tier-range"></span>
										<span class="pool-tier-bar"></span>
										<span class="pool-tier-pwin"></span>
									{/if}
								</div>
							{/each}
							{#if safe?.lowConfidenceGems?.length}
								{@const lcGems = safe.lowConfidenceGems}
								{@const lcTooltip = lcGems.map(g => `<b>${g.name}</b>: ${formatPrice(g.chaos)} (${g.listings} listings)`).join('<br>')}
								<div class="pool-tier-row pool-risky-row">
									<span class="pool-tier-name pool-risky-name">RISKY</span>
									<span class="pool-tier-count">{lcGems.length}</span>
									<span class="pool-tier-range">excluded from EV</span>
									<InfoTooltip text="<b>Low confidence gems</b> — very few listings relative to normal. Could be a new meta build (demand spike) or price manipulation (buyout). Excluded from Font EV calculation.<br><br>{lcTooltip}" />
								</div>
							{/if}
						</div>
					{/each}
				</div>
			</div>
		{/if}
	{/if}
</section>

<style>
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 20px 12px;
		margin-bottom: 32px;
	}
	.loading { color: var(--color-lab-text-secondary); font-size: 0.875rem; }

	/* Dedication header */
	.dedication-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 16px;
		padding: 0 4px;
	}
	.dedication-title {
		font-size: 1.0625rem;
		font-weight: 700;
		color: var(--color-lab-text);
	}
	.entry-fee {
		font-size: 0.9375rem;
		color: var(--color-lab-text-secondary);
	}
	.entry-fee strong {
		color: var(--color-lab-yellow, #eab308);
	}

	/* Dedication cost line */
	.ded-cost-line {
		display: flex;
		align-items: baseline;
		justify-content: center;
		gap: 8px;
		font-size: 0.8125rem;
		margin-bottom: 4px;
	}
	.ded-input {
		color: var(--color-lab-text-secondary);
	}
	.ded-gross {
		color: var(--color-lab-text-secondary);
		border-bottom: 1px dotted var(--color-lab-text-secondary);
		cursor: help;
	}
	.ev-unit {
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--color-lab-text-secondary);
		letter-spacing: 0.04em;
	}
	/* The Dedication table carries four rows of three stacked figures each, so it
	   needs more room per cell than the Font table's single EV line. */
	.ded td { padding: 18px 16px 20px; }
	.ded .ev { margin-bottom: 8px; }
	.ded .ded-cost-line { margin-bottom: 8px; gap: 12px; }
	.ded .tier-lines { gap: 5px; }
	.ded .var { font-size: 1.1875rem; line-height: 1.35; }

	/* Separator between the two corrupted markets. A td border would be drawn
	   per cell and the 4px border-spacing would break it into dashes, so the
	   rule is a spanning row instead. */
	.ded-sep td {
		padding: 0;
		height: 2px;
		background: var(--color-lab-border);
	}

	.ft { width: 100%; border-collapse: separate; border-spacing: 4px; }
	.ft th {
		text-align: center;
		padding: 14px 12px;
		font-size: 1.0625rem;
		font-weight: 700;
		border-bottom: 1px solid var(--color-lab-border);
	}
	.var-header {
		width: 170px;
		border-right: 1px solid var(--color-lab-border);
		font-size: 1.0625rem;
		color: var(--color-lab-text);
		text-align: left;
		padding-left: 16px;
	}
	.ft td {
		text-align: center;
		padding: 10px 12px 14px;
		vertical-align: middle;
	}
	.var {
		text-align: center;
		font-weight: 700;
		font-size: 1.25rem;
		color: var(--color-lab-text);
		width: 170px;
		vertical-align: middle;
		border-right: 1px solid var(--color-lab-border);
	}
	.ev {
		display: block;
		font-weight: 700;
		font-size: 1.25rem;
		color: var(--color-lab-text);
		margin-bottom: 6px;
	}
	.ev.best-red { color: var(--color-lab-red); font-size: 1.4375rem; }
	.ev.best-green { color: var(--color-lab-green); font-size: 1.4375rem; }
	.ev.best-blue { color: var(--color-lab-blue); font-size: 1.4375rem; }
	/* A losing cell reads red regardless of which color won the highlight. */
	.ev.ev-loss { color: var(--color-lab-red); }
	.nil { color: var(--color-lab-text-secondary); font-size: 1.125rem; }

	.w-red { border: 2px solid var(--color-lab-red); background: rgba(239, 68, 68, 0.05); }
	.w-green { border: 2px solid var(--color-lab-green); background: rgba(34, 197, 94, 0.05); }
	.w-blue { border: 2px solid var(--color-lab-blue); background: rgba(59, 130, 246, 0.05); }

	.c-red { color: var(--color-lab-red); }
	.c-green { color: var(--color-lab-green); }
	.c-blue { color: var(--color-lab-blue); }

	.tier-lines {
		display: flex;
		flex-direction: column;
		gap: 2px;
		align-items: center;
	}
	.tier-row {
		display: inline-flex;
		align-items: baseline;
		gap: 6px;
		font-size: 0.875rem;
	}
	.tier-label {
		font-weight: 600;
	}
	.t-safe { color: #5eead4; }
	.t-premium { color: #c084fc; }
	.t-jackpot { color: #fbbf24; }

	.buy-header {
		width: 80px;
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
	}
	.buy-col {
		width: 80px;
	}
	.buy-buttons {
		display: flex;
		flex-direction: column;
		gap: 4px;
		align-items: center;
		justify-content: center;
		height: 100%;
	}
	.buy-btn {
		display: block;
		width: 56px;
		padding: 3px 0;
		font-size: 0.6875rem;
		font-weight: 700;
		text-align: center;
		text-decoration: none;
		letter-spacing: 0.04em;
		border: 1px solid;
		cursor: pointer;
	}
	.buy-red {
		color: var(--color-lab-red);
		border-color: rgba(239, 68, 68, 0.4);
		background: rgba(239, 68, 68, 0.08);
	}
	.buy-red:hover { background: rgba(239, 68, 68, 0.2); }
	.buy-green {
		color: var(--color-lab-green);
		border-color: rgba(34, 197, 94, 0.4);
		background: rgba(34, 197, 94, 0.08);
	}
	.buy-green:hover { background: rgba(34, 197, 94, 0.2); }
	.buy-blue {
		color: var(--color-lab-blue, #3b82f6);
		border-color: rgba(59, 130, 246, 0.4);
		background: rgba(59, 130, 246, 0.08);
	}
	.buy-blue:hover { background: rgba(59, 130, 246, 0.2); }
	.buy-na {
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
		opacity: 0.5;
	}

	.pool-toggle {
		background: none;
		border: none;
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
		font-family: inherit;
		cursor: pointer;
		padding: 8px 0;
		margin-top: 8px;
	}
	.pool-toggle:hover { color: var(--color-lab-text); }
	.pool-section { margin-top: 4px; }
	.pool-variant-select {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 6px;
	}
	.pool-select-label {
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
	}
	.pool-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 8px;
	}
	.pool-color-card {
		border: 1px solid var(--color-lab-border);
		padding: 6px 12px 8px;
		background: rgba(42, 45, 55, 0.3);
	}
	.pool-color-header {
		font-size: 0.875rem;
		font-weight: 700;
		margin-bottom: 4px;
		padding-bottom: 4px;
		border-bottom: 1px solid var(--color-lab-border);
	}
	.pool-count-partial {
		border-bottom: 1px dotted var(--color-lab-text-secondary);
		cursor: help;
	}
	.pool-count {
		font-weight: 400;
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
		margin-left: 6px;
	}
	.pool-tier-row {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 1px 0;
		font-size: 0.8125rem;
		line-height: 1.3;
	}
	.pool-tier-empty {
		opacity: 0.25;
	}
	.pool-risky-row {
		border-top: 1px solid var(--color-lab-border);
		margin-top: 2px;
		padding-top: 3px;
	}
	.pool-risky-name {
		color: var(--color-lab-red) !important;
	}
	.pool-tier-name {
		font-weight: 700;
		width: 70px;
		flex-shrink: 0;
		white-space: nowrap;
		font-size: 0.75rem;
	}
	.pool-tier-count {
		color: var(--color-lab-text);
		font-weight: 600;
		width: 24px;
		text-align: left;
		flex-shrink: 0;
	}
	.pool-tier-range {
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
		width: 100px;
		flex-shrink: 0;
	}
	.pool-tier-bar {
		flex: 1;
		height: 6px;
		background: rgba(255, 255, 255, 0.04);
		border-radius: 3px;
		overflow: hidden;
	}
	.pool-tier-bar-fill {
		display: block;
		height: 100%;
		border-radius: 3px;
		opacity: 0.6;
	}
	.pool-tier-pwin {
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
		font-weight: 600;
		width: 32px;
		text-align: right;
		flex-shrink: 0;
	}
	.pool-empty {
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
	}
</style>
