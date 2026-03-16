<script lang="ts">
	import { fetchFontEV, type FontEVData, type FontColor } from '$lib/api';

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];
	const COLORS = ['RED', 'GREEN', 'BLUE'] as const;

	let data = $state<Record<string, FontEVData>>({});
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

	function getColorData(variant: string, color: string): FontColor | null {
		const vd = data[variant];
		if (!vd) return null;
		return vd.colors.find((c) => c.color === color) || null;
	}

	function getBestVariant(color: string): string {
		let best = '';
		let bestEV = -Infinity;
		for (const v of VARIANTS) {
			const cd = getColorData(v, color);
			if (cd && cd.ev > bestEV) {
				bestEV = cd.ev;
				best = v;
			}
		}
		return best;
	}

	function overallBest(): { color: string; variant: string; ev: number } {
		let best = { color: '', variant: '', ev: -Infinity };
		for (const color of COLORS) {
			for (const v of VARIANTS) {
				const cd = getColorData(v, color);
				if (cd && cd.ev > best.ev) {
					best = { color, variant: v, ev: cd.ev };
				}
			}
		}
		return best;
	}

	$effect(() => {
		loadAll();
	});
</script>

<section class="section">
	<div class="section-header">
		<h2 class="section-title">Font of Divine Skill — EV Comparison</h2>
	</div>

	{#if loading}
		<div class="loading">Loading font data...</div>
	{:else}
		{@const winner = overallBest()}
		{#if winner.ev > 0}
			<div class="overall-winner">
				Best pick: <span class="winner-color color-{winner.color.toLowerCase()}">{winner.color}</span>
				<span class="winner-variant">{winner.variant}</span>
				<span class="winner-ev">{winner.ev}c EV</span>
			</div>
		{/if}

		<div class="color-sections">
			{#each COLORS as color}
				{@const best = getBestVariant(color)}
				<div class="color-section">
					<div class="color-header">
						<span class="color-dot color-{color.toLowerCase()}">●</span>
						<span class="color-name color-{color.toLowerCase()}">{color}</span>
						{#if best}<span class="best-tag">Best: {best}</span>{/if}
					</div>
					<div class="variant-grid">
						{#each VARIANTS as variant}
							{@const cd = getColorData(variant, color)}
							{@const isBest = variant === best}
							<div class="variant-cell" class:is-best={isBest}>
								<div class="cell-variant">{variant}</div>
								{#if cd && cd.ev > 0}
									<div class="cell-ev" class:highlight={isBest}>{cd.ev}c</div>
									<div class="cell-stats">
										<span>Pool: {cd.pool}</span>
										<span>Win: {cd.winners}</span>
										<span>pWin: {cd.pWin}%</span>
									</div>
									<div class="cell-profit">Profit: {cd.profit}c</div>
								{:else}
									<div class="cell-empty">No data</div>
								{/if}
							</div>
						{/each}
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
		margin-bottom: 20px;
	}
	.section-title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.loading {
		color: var(--color-lab-text-secondary);
		font-size: 0.9375rem;
		padding: 20px 0;
	}

	.overall-winner {
		background: rgba(34, 197, 94, 0.08);
		border: 1px solid rgba(34, 197, 94, 0.2);
		padding: 12px 18px;
		margin-bottom: 20px;
		font-size: 1rem;
		color: var(--color-lab-text);
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.winner-color {
		font-weight: 700;
		font-size: 1.125rem;
	}
	.winner-variant {
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		padding: 2px 10px;
		font-size: 0.875rem;
		font-weight: 600;
	}
	.winner-ev {
		color: var(--color-lab-green);
		font-weight: 700;
		font-size: 1.125rem;
	}

	.color-sections {
		display: flex;
		flex-direction: column;
		gap: 20px;
	}
	.color-section {
		border: 1px solid var(--color-lab-border);
		padding: 18px 20px;
		background: var(--color-lab-bg);
	}
	.color-header {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-bottom: 14px;
	}
	.color-dot {
		font-size: 1rem;
	}
	.color-name {
		font-weight: 700;
		font-size: 1rem;
	}
	.best-tag {
		margin-left: auto;
		font-size: 0.8125rem;
		color: var(--color-lab-green);
		font-weight: 600;
	}

	.variant-grid {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 12px;
	}
	.variant-cell {
		border: 1px solid var(--color-lab-border);
		padding: 14px 16px;
		background: var(--color-lab-surface);
	}
	.variant-cell.is-best {
		border-color: var(--color-lab-green);
		background: rgba(34, 197, 94, 0.05);
	}
	.cell-variant {
		font-size: 0.875rem;
		font-weight: 700;
		color: var(--color-lab-text-secondary);
		margin-bottom: 8px;
	}
	.cell-ev {
		font-size: 1.25rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin-bottom: 8px;
	}
	.cell-ev.highlight {
		color: var(--color-lab-green);
	}
	.cell-stats {
		display: flex;
		gap: 12px;
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
		margin-bottom: 6px;
	}
	.cell-profit {
		font-size: 0.875rem;
		color: var(--color-lab-text-secondary);
	}
	.cell-empty {
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
		font-style: italic;
	}

	.color-red { color: var(--color-lab-red); }
	.color-green { color: var(--color-lab-green); }
	.color-blue { color: var(--color-lab-blue); }
</style>
