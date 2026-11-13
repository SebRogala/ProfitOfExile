<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { PriceCell } from '$lib/compass/font-alerts-view';
	import type { EvCell } from '$lib/font-recipes';

	interface Row {
		name: string;
		sub?: string;
		cell: PriceCell;
		/** Rendered at the row's end, after the price. */
		action?: Snippet;
	}

	let {
		title,
		option,
		givesLabel,
		gives,
		costs = [],
		ev = null,
		source,
		age,
	}: {
		title: string;
		option: string;
		givesLabel: string;
		gives: Row[];
		costs?: Row[];
		ev?: EvCell | null;
		source: string;
		age: { text: string; stale: boolean } | null;
	} = $props();
</script>

{#snippet row(r: Row)}
	<div class="rc-row" class:with-action={r.action}>
		<span class="rc-name"><span>{r.name}</span>{#if r.sub}<small>{r.sub}</small>{/if}</span>
		{#if r.cell.kind === 'price'}
			<span class="rc-num">{r.cell.text}</span>
		{:else}
			<span class="rc-nil">{r.cell.kind === 'loading' ? 'pricing…' : 'no price'}</span>
		{/if}
		{#if r.action}{@render r.action()}{/if}
	</div>
{/snippet}

<article class="rc" aria-label={title}>
	<div class="rc-head">
		<h3 class="rc-title">{title}</h3>
		<p class="rc-opt">{option}</p>
	</div>

	<div class="rc-group">{givesLabel}</div>
	{#each gives as r (r.name)}{@render row(r)}{/each}

	{#if costs.length > 0}
		<div class="rc-group">Costs</div>
		{#each costs as r (r.name)}{@render row(r)}{/each}
	{/if}

	{#if ev}
		<div class="rc-ev">
			<span class="rc-name"><span class="rc-ev-label">EV per craft</span><small>lens − support</small></span>
			{#if ev.kind === 'value'}
				<span class="rc-ev-value" class:negative={ev.negative}>{ev.text}</span>
			{:else}
				<span class="rc-nil">{ev.kind === 'loading' ? 'pricing…' : ev.text}</span>
			{/if}
		</div>
	{/if}

	<div class="rc-foot">
		<span class="rc-source">{source}</span>
		{#if age}
			{#if age.stale}
				<span class="rc-stale">updated {age.text}</span>
			{:else}
				<span>{age.text}</span>
			{/if}
		{/if}
	</div>
</article>

<style>
	.rc {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		border-radius: 4px;
		padding: 10px 12px;
		display: flex;
		flex-direction: column;
		gap: 6px;
		font-size: 13px;
		min-width: 0;
		color: var(--color-lab-text);
	}
	.rc-head { display: flex; flex-direction: column; gap: 2px; }
	.rc-title { font-size: 14px; font-weight: 700; margin: 0; }
	.rc-opt { font-size: 12px; color: var(--color-lab-text-secondary); margin: 0; line-height: 16px; }
	.rc-group {
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
		margin-top: 4px;
		border-bottom: 1px solid var(--color-lab-border);
		padding-bottom: 3px;
	}
	.rc-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 10px;
		min-height: 28px;
	}
	.rc-row.with-action { grid-template-columns: minmax(0, 1fr) auto auto; }
	.rc-name { display: flex; flex-direction: column; min-width: 0; line-height: 16px; }
	.rc-name small { font-size: 11px; color: var(--color-lab-text-secondary); }
	.rc-num { font-family: 'Consolas', 'Monaco', monospace; font-weight: 700; text-align: right; }
	.rc-nil { font-size: 12px; font-style: italic; color: var(--color-lab-text-secondary); text-align: right; }
	.rc-ev {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 10px;
		border-top: 1px solid var(--color-lab-border);
		padding-top: 6px;
		margin-top: 2px;
	}
	.rc-ev-label { font-weight: 600; }
	.rc-ev-value { font-family: 'Consolas', 'Monaco', monospace; font-weight: 700; font-size: 1.25rem; }
	.rc-ev-value.negative { color: var(--color-lab-red); }
	.rc-foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		white-space: nowrap;
		font-size: 11px;
		color: var(--color-lab-text-secondary);
		margin-top: 2px;
	}
	.rc-source { overflow: hidden; text-overflow: ellipsis; min-width: 0; }
	.rc-stale {
		color: var(--color-lab-yellow);
		background: color-mix(in srgb, var(--color-lab-yellow) 12%, transparent);
		padding: 1px 6px;
		border-radius: 3px;
	}
</style>
