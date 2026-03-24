<script lang="ts">
	import { fetchFontEV, type FontEVResponse, type FontColor } from '$lib/api';
	import { baseGemTradeUrl } from '$lib/trade-utils';
	import InfoTooltip from './InfoTooltip.svelte';

	let { refreshKey = 0, league = '' }: { refreshKey?: number; league?: string } = $props();

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];
	const COLORS = ['RED', 'GREEN', 'BLUE'] as const;

	let data = $state<Record<string, FontEVResponse>>({});
	let loading = $state(true);

	async function loadAll() {
		loading = true;
		const results = await Promise.all(
			VARIANTS.map(async (v) => {
				const d = await fetchFontEV(v);
				return [v, d] as const;
			})
		);
		for (const [v, d] of results) {
			data[v] = d;
		}
		loading = false;
	}

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

	function tierLine(fc: FontColor | null): string {
		if (!fc || fc.winners === 0 || !fc.fontsToHit) return '\u2014 none';
		const raw = fc.avgWinRaw || fc.avgWin || 0;
		const fth = fc.fontsToHit;
		let ratio: string;
		if (fth <= 1.05) {
			ratio = 'every font';
		} else if (fth < 2) {
			// > 50% hit rate — show as "X in Y" reduced fraction
			const pctFrac = fc.pWin / 100; // pWin is 0-100
			const hits = Math.round(pctFrac * 10);
			const gcd = (a: number, b: number): number => b === 0 ? a : gcd(b, a % b);
			const d = hits > 0 ? gcd(hits, 10) : 1;
			ratio = `${hits / d} in ${10 / d}`;
		} else {
			ratio = `1 in ${Math.round(fth)}`;
		}
		const pct = Math.round(fc.pWin);
		return `${ratio} (${pct}%)  ~${raw}c`;
	}

	/**
	 * Build trade URL for cheapest base gem of a color + variant.
	 * Color filtering uses attribute requirement caps (max 97) to exclude hybrids (98+).
	 * RED = STR gems: cap dex+int. GREEN = DEX: cap str+int. BLUE = INT: cap str+dex.
	 * Level 1 gems have no attribute requirements — can't filter by color, returns null.
	 */
	function buyBaseUrl(variant: string, color: string): string | null {
		const parts = variant.split('/');
		const level = parseInt(parts[0]) || 0;
		const quality = parts.length > 1 ? parseInt(parts[1]) : 0;

		// Level 1 gems have no attribute requirements — no color filter possible.
		if (level < 20) return null;

		// Attribute caps to isolate pure-color gems (hybrids start at 98).
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

	$effect(() => { refreshKey; loadAll(); });
</script>

<section class="section">
	{#if loading}
		<span class="loading">Loading...</span>
	{:else}
		{@const best = winner()}
		<table class="ft">
			<thead>
				<tr>
					<th class="var-header">Font EV<InfoTooltip text="<b>Font of Divine Skill — Expected Value per usage</b><br><br><b>Nc/font</b>: Your average income (listed market price) each time you use the Font. Based on best-of-3 random draws — you always pick the most valuable gem.<br><br><b>Highlighted cell</b>: Best color for highest average income.<br><br><b>Hit tiers</b> (per color pool, using listed prices):<br>• <span style='color:#5eead4'>Safe</span>: Above-average gems in this color. 'X in Y' = how often you see one. '~Nc' = average listed price when you hit.<br>• <span style='color:#c084fc'>Premium</span>: High-value gems within this color pool. Less frequent, bigger payout.<br>• <span style='color:#fbbf24'>Jackpot</span>: Variant-wide TOP outliers (same threshold across all colors). Only shown when TOP gems exist in this color.<br><br>Safe and Premium tiers are computed per color pool (RED/GREEN/BLUE have different price distributions). Jackpot uses variant-wide boundaries so '~1046c Jackpot' means the same value whether it's in GREEN or BLUE.<br><br><b>Tip</b>: With Gift Lab (8 fonts/run), even low Jackpot hit rates become viable through compound probability." /></th>
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
									<span class="ev" class:best-red={isW && color === 'RED'} class:best-green={isW && color === 'GREEN'} class:best-blue={isW && color === 'BLUE'}>{ev}c/font</span>
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
											let html = `<div style="padding:4px 0;border-bottom:1px solid rgba(255,255,255,0.06)"><b>${g.name}</b>: ${Math.round(g.chaos)}c &nbsp;&nbsp;<a href="${baseGemTradeUrl(g.name, variant, league || '')}" target="_blank" style="padding:1px 8px;font-size:0.75rem;font-weight:600;color:#5eead4;border:1px solid rgba(94,234,212,0.4);text-decoration:none;letter-spacing:0.03em">Buy Base</a>`;
											if (g.gcpRecipeCost > 0) {
												html += `<div style="margin-top:3px;font-size:0.75rem;color:#94a3b8">GCP recipe: <b>${Math.round(g.gcpRecipeBase)}c</b> base + ${Math.round(g.gcpRecipeCost - g.gcpRecipeBase)}c GCPs = <b>${Math.round(g.gcpRecipeCost)}c</b> <span style="color:#22c55e">(saves ${Math.round(g.gcpRecipeSaves)}c)</span> &nbsp;<a href="${baseGemNoQualityUrl(g.name)}" target="_blank" style="padding:1px 6px;font-size:0.6875rem;font-weight:600;color:#fbbf24;border:1px solid rgba(251,191,36,0.4);text-decoration:none">Buy 20/0</a></div>`;
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
	.tier-val {
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
</style>
