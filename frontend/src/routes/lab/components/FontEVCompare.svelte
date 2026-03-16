<script lang="ts">
	import { fetchFontEV, type FontEVData, type FontColor } from '$lib/api';

	let { refreshKey = 0 }: { refreshKey?: number } = $props();

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

	function getEV(variant: string, color: string): FontColor | null {
		const vd = data[variant];
		if (!vd) return null;
		return vd.colors.find((c) => c.color === color) || null;
	}

	function winner(): { variant: string; color: string; ev: number } {
		let best = { variant: '', color: '', ev: -Infinity };
		for (const v of VARIANTS) {
			for (const color of COLORS) {
				const cd = getEV(v, color);
				if (cd && cd.ev > best.ev) {
					best = { variant: v, color, ev: cd.ev };
				}
			}
		}
		return best;
	}

	$effect(() => { refreshKey; loadAll(); });
</script>

<section class="section">
	<h2 class="section-title">Font EV</h2>

	{#if loading}
		<span class="loading">Loading...</span>
	{:else}
		{@const best = winner()}
		<table class="ft">
			<thead>
				<tr>
					<th></th>
					{#each COLORS as color}
						<th><span class="c-{color.toLowerCase()}">● {color}</span></th>
					{/each}
				</tr>
			</thead>
			<tbody>
				{#each VARIANTS as variant}
					<tr>
						<td class="var">{variant}</td>
						{#each COLORS as color}
							{@const cd = getEV(variant, color)}
							{@const isW = best.variant === variant && best.color === color}
							<td class:w-red={isW && color === 'RED'} class:w-green={isW && color === 'GREEN'} class:w-blue={isW && color === 'BLUE'}>
								{#if cd && cd.ev > 0}
									<span class="ev" class:best={isW}>{cd.ev}c</span>
									<span class="det">pool {cd.pool} · {cd.pWin}%</span>
								{:else}
									<span class="nil">—</span>
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
	.section-title {
		font-size: 1.0625rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0 0 14px 0;
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
	}
	.var {
		text-align: left !important;
		font-weight: 700;
		font-size: 1.0625rem;
		color: var(--color-lab-text);
		width: 80px;
	}
	.ev {
		font-weight: 700;
		font-size: 1.25rem;
		color: var(--color-lab-text);
		margin-right: 10px;
	}
	.ev.best {
		color: var(--color-lab-green);
		font-size: 1.5rem;
	}
	.det {
		font-size: 0.9375rem;
		color: var(--color-lab-text-secondary);
	}
	.nil { color: var(--color-lab-text-secondary); font-size: 1.125rem; }

	.w-red { border: 2px solid var(--color-lab-red); background: rgba(239, 68, 68, 0.05); }
	.w-green { border: 2px solid var(--color-lab-green); background: rgba(34, 197, 94, 0.05); }
	.w-blue { border: 2px solid var(--color-lab-blue); background: rgba(59, 130, 246, 0.05); }

	.c-red { color: var(--color-lab-red); }
	.c-green { color: var(--color-lab-green); }
	.c-blue { color: var(--color-lab-blue); }
</style>
