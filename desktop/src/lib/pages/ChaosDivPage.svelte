<script lang="ts">
	import { onDestroy } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import Button from '$lib/components/Button.svelte';
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte';
	import { currentDivineRate } from '$lib/stores/divine-rate.svelte';
	import {
		CHAOS_DIV_DEFAULTS,
		calculateChaosDiv,
		formatChaosDivWhole,
		type ChaosDivMode,
		type ChaosDivCurrency,
		type ChaosDivOneSide
	} from '$lib/tools/chaos-div';

	let mode = $state<ChaosDivMode>(CHAOS_DIV_DEFAULTS.mode);
	let currency = $state<ChaosDivCurrency>(CHAOS_DIV_DEFAULTS.currency);
	let amount = $state<number | undefined>(CHAOS_DIV_DEFAULTS.amount);
	let ratio = $state<number | undefined>(CHAOS_DIV_DEFAULTS.ratio);
	let oneSide = $state<ChaosDivOneSide>(CHAOS_DIV_DEFAULTS.oneSide);
	const rate = $derived(currentDivineRate());
	const result = $derived(calculateChaosDiv({ mode, currency, amount: Number(amount), ratio: Number(ratio), oneSide, rate }));
	const plain = $derived(result.ratio === 1);
	const selling = $derived(mode === 'sell');
	const amountLabel = $derived(plain ? 'Amount' : selling ? 'Items I have' : 'Items I want');
	const totalLead = $derived(plain ? 'Total' : selling ? 'Worth' : 'Cost');
	const unit = $derived(currency === 'chaos' ? 'c' : 'd');

	let copied = $state<{ key: string; text: string } | null>(null);
	let copyTimer: ReturnType<typeof setTimeout> | undefined;
	let copyGeneration = 0;

	function clearCopy(): void {
		copyGeneration++;
		clearTimeout(copyTimer);
		copied = null;
	}

	async function copy(value: number | string | null, key: string): Promise<void> {
		if (value === null) return;
		const generation = ++copyGeneration;
		const text = String(value);
		try {
			await navigator.clipboard.writeText(text);
			if (generation !== copyGeneration) return;
			clearTimeout(copyTimer);
			copied = { key, text };
			copyTimer = setTimeout(() => { copied = null; }, 1200);
		} catch (error) {
			console.warn('[chaos-div] clipboard write failed:', error);
			invoke('app_log_from_frontend', { msg: `[chaos-div] clipboard write failed: ${String(error)}` })
				.catch((logError) => console.warn('[chaos-div] app log unreachable:', logError));
		}
	}

	function copyLabel(key: string, value: number | string | null): string {
		return copied?.key === key && copied.text === String(value) ? 'Copied' : 'Copy';
	}

	function reset(): void {
		mode = CHAOS_DIV_DEFAULTS.mode;
		currency = CHAOS_DIV_DEFAULTS.currency;
		amount = CHAOS_DIV_DEFAULTS.amount;
		ratio = CHAOS_DIV_DEFAULTS.ratio;
		oneSide = CHAOS_DIV_DEFAULTS.oneSide;
		clearCopy();
	}

	onDestroy(clearCopy);
</script>

<div class="chaos-div-page">
	<header class="page-head">
		<h1>Chaos ⇄ Div</h1>
		<Button onclick={reset}>
			<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 3-6.7" /><path d="M3 4v5h5" /></svg>
			Reset
		</Button>
	</header>

	<section class="input-bar panel" aria-label="Calculator inputs">
		<div class="field">
			<span class="caption">Mode</span>
			<SegmentedButtons value={mode} title="Mode" options={[{ value: 'sell', label: 'Selling' }, { value: 'buy', label: 'Buying' }]} onselect={(value) => { mode = value === 'buy' ? 'buy' : 'sell'; clearCopy(); }} />
		</div>
		<div class="field">
			<span class="caption">Currency</span>
			<SegmentedButtons value={currency} title="Currency" options={[{ value: 'chaos', label: 'Chaos' }, { value: 'div', label: 'Divine' }]} onselect={(value) => { currency = value === 'div' ? 'div' : 'chaos'; clearCopy(); }} />
		</div>
		<div class="field">
			<label class="caption" for="chaos-div-amount">{amountLabel}</label>
			<div class="stepper">
				<button type="button" class="square" aria-label="Decrease amount" onclick={() => { amount = Math.max(1, result.amount - 1); clearCopy(); }}>−</button>
				<input id="chaos-div-amount" class="amount mono" type="number" min="1" max="100000" step="1" bind:value={amount} oninput={clearCopy} onblur={() => { amount = result.amount; }} />
				<button type="button" class="square" aria-label="Increase amount" onclick={() => { amount = Math.min(100000, result.amount + 1); clearCopy(); }}>+</button>
			</div>
		</div>
		<div class="field">
			<label class="caption" for="chaos-div-ratio">Ratio</label>
			<div class="ratio-control mono">
				{#if oneSide === 'left'}<span class="fixed-one">1 :</span>{/if}
				<input id="chaos-div-ratio" class="ratio mono" type="number" min="0.01" step="0.01" bind:value={ratio} oninput={clearCopy} />
				{#if oneSide === 'right'}<span class="fixed-one">: 1</span>{/if}
				<button type="button" class="square" aria-label="Swap which side is 1" title="Swap which side is 1" onclick={() => { oneSide = oneSide === 'left' ? 'right' : 'left'; clearCopy(); }}>
					<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 4 3 8l4 4M3 8h14m0 12 4-4-4-4M21 16H7" /></svg>
				</button>
			</div>
			<div class="per-item">Per item <span class="mono">{result.perItemC}{result.perItemC === '—' ? '' : 'c'} · {result.perItemD}{result.perItemD === '—' ? '' : 'd'}</span></div>
		</div>
	</section>

	<section class="totals" aria-label="Totals">
		<div class="total-card panel">
			<div class="caption">{totalLead} in chaos</div>
			<div class="total-value">
				<span class="number mono">{result.totalChaos === null ? '—' : formatChaosDivWhole(result.totalChaos)}{#if result.totalChaos !== null}<span class="unit">c</span>{/if}</span>
				<Button disabled={result.totalChaos === null} title="Copy chaos total" onclick={() => copy(result.totalChaos, 'chaos')}>{copyLabel('chaos', result.totalChaos)}</Button>
			</div>
		</div>
		<div class="total-card panel">
			<div class="caption">{totalLead} in divine</div>
			<div class="total-value">
				<span class="number mono">{result.totalDivine ?? '—'}{#if result.totalDivine !== null}<span class="unit">d</span>{/if}</span>
				<Button disabled={result.totalDivine === null} title="Copy divine total" onclick={() => copy(result.totalDivine, 'divine')}>{copyLabel('divine', result.totalDivine)}</Button>
			</div>
			<div class="split">Whole split <span class="mono">{result.split ?? '—'}</span></div>
		</div>
	</section>

	{#if result.tableShown}
		<section class="order-panel panel">
			<h2>{selling ? 'Nearest undercuts' : 'Nearest outbids'}</h2>
			<div class="table-scroll">
				<table>
					<thead><tr><th scope="col">Ratio</th><th scope="col">{selling ? 'Undercut' : 'Outbid'}</th><th scope="col">I want</th><th scope="col">I have</th><th scope="col">{selling ? 'Left over' : 'Short of target'}</th></tr></thead>
					<tbody>
						{#each result.rows as row, index (row.ratio)}
							<tr class:first={index === 0} class="mono">
								<td class="order-ratio">{row.ratio}</td>
								<td class="delta">{row.delta}</td>
								<td><div class="order-value"><span>{row.iWant}{selling ? unit : ''}</span><Button title="Copy I want value" onclick={() => copy(row.iWant, `want-${index}`)}>{copyLabel(`want-${index}`, row.iWant)}</Button></div></td>
								<td><div class="order-value"><span>{row.iHave}{selling ? '' : unit}</span><Button title="Copy I have value" onclick={() => copy(row.iHave, `have-${index}`)}>{copyLabel(`have-${index}`, row.iHave)}</Button></div></td>
								<td class="left-over">{row.leftOver || '—'}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		</section>
	{/if}
</div>

<style>
	.chaos-div-page {
		display: flex;
		flex-direction: column;
		gap: 16px;
		max-width: 816px;
		padding: 8px;
		color: var(--color-lab-text);
	}
	.page-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
	h1 { margin: 0; font-size: 20px; font-weight: 600; }
	h2 { margin: 0; font-size: 14px; font-weight: 600; }
	.panel { background: var(--color-lab-surface); border: 1px solid var(--color-lab-border); border-radius: 8px; }
	.input-bar { display: flex; flex-wrap: wrap; align-items: start; gap: 12px; padding: 16px; }
	.field { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
	.caption { font-size: 12px; color: var(--color-lab-text-secondary); }
	.mono { font-family: 'Consolas', 'Monaco', monospace; font-weight: 600; }
	.stepper, .ratio-control { display: flex; align-items: center; gap: 4px; }
	.ratio-control { gap: 6px; font-size: 18px; }
	.fixed-one { color: var(--color-lab-text-secondary); white-space: nowrap; }
	input, .square { height: 44px; border: 1px solid var(--color-lab-border); border-radius: 6px; background: var(--color-lab-bg); color: var(--color-lab-text); font-size: 18px; }
	input { min-width: 0; appearance: textfield; }
	input::-webkit-outer-spin-button, input::-webkit-inner-spin-button { -webkit-appearance: none; margin: 0; }
	.amount { width: 88px; text-align: center; }
	.ratio { width: 96px; padding: 0 10px; }
	.square { width: 44px; flex-shrink: 0; display: inline-flex; align-items: center; justify-content: center; cursor: pointer; }
	.square:hover { border-color: var(--color-lab-blue); }
	input:focus-visible, .square:focus-visible, .chaos-div-page :global(.btn:focus-visible) { outline: 2px solid var(--color-lab-blue); outline-offset: 1px; }
	.per-item, .split { font-size: 12px; color: var(--color-lab-text-secondary); }
	.per-item .mono, .split .mono { color: var(--color-lab-text); }
	.totals { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
	.total-card { display: flex; flex-direction: column; gap: 8px; padding: 16px; min-width: 0; }
	.total-value { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px; }
	.number { font-size: 32px; overflow-wrap: anywhere; }
	.unit { font-size: 18px; color: var(--color-lab-text-secondary); }
	.order-panel { display: flex; flex-direction: column; gap: 8px; padding: 12px 16px; min-width: 0; }
	.table-scroll { overflow-x: auto; overflow-y: hidden; }
	table { width: 100%; border-collapse: collapse; font-size: 13px; white-space: nowrap; }
	th, td { padding: 6px 8px; border-bottom: 1px solid var(--color-lab-border); text-align: left; }
	th { font-weight: 500; color: var(--color-lab-text-secondary); }
	tr.first { background: color-mix(in srgb, var(--color-lab-green) 8%, transparent); }
	.order-ratio { font-size: 15px; }
	.delta { color: var(--color-lab-yellow); }
	.left-over { color: var(--color-lab-text-secondary); }
	.order-value { display: inline-flex; align-items: center; gap: 8px; }
	.order-value > span { min-width: 64px; font-size: 15px; }
	.chaos-div-page :global(.btn) { display: inline-flex; align-items: center; justify-content: center; gap: 6px; height: 36px; padding: 0 14px; border-radius: 6px; border-color: var(--color-lab-border); background: var(--color-lab-bg); color: var(--color-lab-text); font-family: inherit; font-size: 13px; }
	.chaos-div-page :global(.btn:hover:not(:disabled)) { border-color: var(--color-lab-blue); color: var(--color-lab-text); }
	.total-value :global(.btn), .order-value :global(.btn) { background: var(--color-lab-border); }
	.order-value :global(.btn) { height: 32px; padding: 0 10px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; font-size: 12px; }
	.input-bar :global(.segmented) { height: 44px; border-radius: 6px; }
	.input-bar :global(.segmented-btn) { min-width: 68px; padding: 0 12px; font-size: 13px; }
	@media (max-width: 480px) {
		.chaos-div-page { padding: 0; }
		.total-card { padding: 12px; }
	}
</style>
