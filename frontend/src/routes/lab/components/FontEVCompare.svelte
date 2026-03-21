<script lang="ts">
	import { fetchFontEV, type FontEVResponse, type FontColor } from '$lib/api';
	import InfoTooltip from './InfoTooltip.svelte';

	let { refreshKey = 0 }: { refreshKey?: number } = $props();

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
		return fc?.ev ?? 0;
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
		return `1 in ${Math.round(fc.fontsToHit)} \u00B7 avg ${fc.avgWin}c`;
	}

	$effect(() => { refreshKey; loadAll(); });
</script>

<section class="section">
	<div class="section-header">
		<h2 class="section-title">Font EV<InfoTooltip text="4x3 matrix showing EV per variant and color. Highlighted cell = best combination.<br><br>Each cell shows EV and tier hit rates:<br><b>Safe (LOW+)</b>: Reliable income.<br><b>Premium (MID-HIGH+)</b>: Valuable gems.<br><b>Jackpot (TOP)</b>: Top-tier gems only." /></h2>
	</div>

	{#if loading}
		<span class="loading">Loading...</span>
	{:else}
		{@const best = winner()}
		<table class="ft">
			<thead>
				<tr>
					<th></th>
					{#each COLORS as color}
						<th><span class="c-{color.toLowerCase()}">{'\u25CF'} {color}</span></th>
					{/each}
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
										<div class="tier-row">
											<span class="tier-label t-jackpot">Jackpot</span>
											<span class="tier-val t-jackpot">{tierLine(jackpot)}</span>
										</div>
									</div>
								{:else}
									<span class="nil">{'\u2014'}</span>
								{/if}
							</td>
						{/each}
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
		padding: 20px 28px;
		margin-bottom: 32px;
	}
	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 14px;
	}
	.section-title {
		font-size: 1.0625rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.loading { color: var(--color-lab-text-secondary); font-size: 0.875rem; }

	.ft { width: 100%; border-collapse: collapse; }
	.ft th {
		text-align: center;
		padding: 10px 12px;
		font-size: 1.0625rem;
		font-weight: 700;
		border-bottom: 1px solid var(--color-lab-border);
	}
	.ft td {
		text-align: center;
		padding: 14px 12px;
		vertical-align: top;
	}
	.var {
		text-align: left !important;
		font-weight: 700;
		font-size: 1.0625rem;
		color: var(--color-lab-text);
		width: 80px;
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
	}
	.tier-row {
		display: flex;
		justify-content: center;
		align-items: baseline;
		gap: 6px;
		font-size: 0.8125rem;
	}
	.tier-label {
		font-weight: 600;
		min-width: 48px;
		text-align: right;
	}
	.tier-val {
		min-width: 90px;
		text-align: left;
	}
	.t-safe { color: #5eead4; }
	.t-premium { color: #c084fc; }
	.t-jackpot { color: #fbbf24; }
</style>
