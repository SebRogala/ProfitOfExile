<script lang="ts">
	/**
	 * Harvest Flipping (POE-283) — the family overview: verdict, Feeders and
	 * Keepers as tier cards, the two stash regexes, cost per reroll and the EV
	 * of the selection (handoff README § Screens 1–7, references 01–08).
	 *
	 * Presentation only. `harvest/controller.svelte.ts` owns the fetch, the
	 * Mercure refetch, the persisted picks and the clipboard; `harvest/view.ts`
	 * projects every string and flag this template prints, so it computes
	 * nothing. Not mounted yet: the nav wiring lands with WI-10.
	 */
	import { untrack } from 'svelte';
	import { getApiBase } from '$lib/api';
	import ItemIcon from '$lib/components/ItemIcon.svelte';
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte';
	import Button from '$lib/components/Button.svelte';
	import { HORIZON_OPTIONS, iconSrc } from '$lib/exchange/view';
	import { createHarvestController } from '$lib/harvest/controller.svelte';
	import HarvestRegexBox from '$lib/harvest/HarvestRegexBox.svelte';
	import HarvestTierCard from '$lib/harvest/HarvestTierCard.svelte';
	import HarvestTypeChip from '$lib/harvest/HarvestTypeChip.svelte';
	import type { Column } from '$lib/harvest/view';

	const controller = createHarvestController();
	const apiBase = $derived(getApiBase());
	const view = $derived(controller.view);
	const body = $derived(view.body);

	$effect(() => untrack(() => controller.start()));

	// A horizon pick or a late prefs restore reloads; `syncHorizon` is idempotent.
	$effect(() => {
		void controller.horizon;
		untrack(() => controller.syncHorizon());
	});
</script>

{#snippet column(col: Column, kept: boolean)}
	{#each col.cards as card (card.tier)}
		<HarvestTierCard
			{card}
			{apiBase}
			onmove={(tier, to) => controller.moveTier(tier, to)}
			onpick={(itemId) => controller.togglePick(itemId)}
		/>
	{/each}
	{#if col.untiered.length > 0}
		<div class="untiered">
			{#each col.untiered as chip (chip.itemId)}
				<HarvestTypeChip {chip} {apiBase} {kept} onpick={(itemId) => controller.togglePick(itemId)} />
			{/each}
		</div>
	{/if}
	{#if col.empty !== null}
		<p class="empty">{col.empty}</p>
	{/if}
{/snippet}

<div class="harvest-page" aria-busy={view.state === 'loading'}>
	<div class="page-head">
		<h1>Harvest Flipping</h1>
		{#if controller.exchange}
			<span class="league">{controller.exchange.league}</span>
		{/if}
		<div class="spacer"></div>
		<span class="control-label">Prices</span>
		<SegmentedButtons
			value={controller.horizon}
			options={HORIZON_OPTIONS}
			onselect={(v) => controller.setHorizon(v)}
		/>
	</div>

	{#if view.tabs.length > 0}
		<div class="tabs" role="tablist">
			{#each view.tabs as tab (tab.id)}
				<button
					type="button"
					role="tab"
					class="tab"
					class:active={tab.active}
					aria-selected={tab.active}
					onclick={() => controller.setFamily(tab.id)}
				>
					{tab.label}
					{#if tab.noData}
						<span class="no-data-mark">NO DATA</span>
					{/if}
				</button>
			{/each}
		</div>
	{/if}

	{#if view.status}
		<div class="status-line">
			{#each view.status.segments as segment, i (i)}
				{#if i > 0}<span class="dot">·</span>{/if}
				<span
					class:warn={view.status.stale && i === 0}
					class:age={view.status.stale ? i === 1 : i === 0}>{segment}</span
				>
			{/each}
		</div>
	{/if}

	{#if view.stateLine !== null}
		<p class="state-line" class:warn={view.state === 'unreachable'}>{view.stateLine}</p>
	{/if}

	{#if view.state === 'loading' && body === null}
		<div class="panel skeleton" aria-hidden="true">
			<span class="bar short"></span>
			<span class="bar"></span>
		</div>
	{/if}

	{#if body?.kind === 'cold'}
		<section class="panel verdict">
			<div class="verdict-text">
				<span class="caption">Is it worth it?</span>
				<h2 class="headline">{body.title}</h2>
				<p class="reason">{body.text}</p>
			</div>
		</section>
	{:else if body?.kind === 'no-data'}
		<section class="panel no-data">
			<h2 class="no-data-title">{body.title}</h2>
			<p class="no-data-text">{body.text}</p>
		</section>
	{:else if body?.kind === 'family'}
		<section class="panel verdict">
			<div class="verdict-text">
				<span class="caption">{body.verdict.question}</span>
				<h2 class="headline">{body.verdict.headline}</h2>
				{#if body.verdict.reason !== null}
					<p class="reason">{body.verdict.reason}</p>
				{/if}
			</div>
			{#if body.verdict.evText !== null}
				<div class="verdict-ev">
					<span class="mono ev-big" class:gain={body.verdict.worth} class:loss={body.verdict.evNegative}
						>{body.verdict.evText}</span
					>
					{#if body.verdict.per !== null}
						<span class="per">{body.verdict.per}</span>
					{/if}
					{#if body.verdict.divineLine !== null}
						<span class="divine-line">{body.verdict.divineLine}</span>
					{/if}
				</div>
			{/if}
		</section>

		<div class="split-row">
			<span class="caption">Keep / feed split</span>
			<span class="split-label" class:moved={body.split.moved}>{body.split.label}</span>
			<span class="spacer"></span>
			<Button disabled={!body.split.resetEnabled} onclick={() => controller.resetFamily()}
				>Reset to computed</Button
			>
		</div>

		<div class="grid">
			<section class="panel column-panel">
				<header class="column-head">
					<span class="column-title">FEEDERS</span>
					<span class="column-sub">reroll these · Loop EV each</span>
				</header>
				{@render column(body.feeders, false)}
			</section>

			<section class="panel column-panel">
				<header class="column-head">
					<span class="column-title">KEEPERS</span>
					<span class="column-sub">stop and sell</span>
				</header>
				{@render column(body.keepers, true)}
			</section>

			<div class="side">
				<section class="panel side-panel">
					<HarvestRegexBox
						label="Feeders regex"
						box={body.regex.feeders}
						copied={controller.copied.feeders}
						oncopy={() => controller.copyRegex('feeders')}
					/>
					<HarvestRegexBox
						label="Keepers regex"
						box={body.regex.keepers}
						copied={controller.copied.keepers}
						oncopy={() => controller.copyRegex('keepers')}
					/>
					<p class="small">{body.regex.caption}</p>
				</section>

				<section class="panel side-panel">
					<span class="caption">Cost per reroll</span>
					<p class="cost-line">
						<span class="mono cost">{body.cost.costText}</span>
						= {body.cost.perReroll} ×
						<ItemIcon src={iconSrc(apiBase, body.cost.lifeforceIcon)} alt={body.cost.lifeforceLabel} size={16} />
						<strong>{body.cost.lifeforceLabel}</strong>
					</p>
					<p class="mono per-divine">{body.cost.perDivine}</p>
					<p class="small">{body.cost.caption}</p>
				</section>

				{#if body.ev}
					<section class="panel side-panel">
						<span class="caption">EV of your selection</span>
						<dl class="ev-rows">
							{#each body.ev.rows as row (row.label)}
								<dt>{row.label}</dt>
								<dd class="mono">{row.value}</dd>
							{/each}
						</dl>
						{#if body.ev.unpricedNote !== null}
							<p class="small warn">{body.ev.unpricedNote}</p>
						{/if}
						<dl class="ev-rows yield">
							{#each body.ev.yield as row (row.label)}
								<dt>{row.label}</dt>
								<dd class="mono">{row.value}</dd>
							{/each}
						</dl>
					</section>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.harvest-page {
		display: flex;
		flex-direction: column;
		gap: 10px;
		color: var(--color-lab-text);
	}

	.page-head {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 10px 16px;
	}

	.page-head h1 {
		font-size: 1rem;
		font-weight: 700;
		margin: 0;
	}

	.league {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
	}

	.spacer {
		flex: 1;
	}

	.control-label,
	.caption {
		font-size: 0.625rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-secondary);
		white-space: nowrap;
	}

	.tabs {
		display: flex;
		flex-wrap: nowrap;
		gap: 2px;
		border-bottom: 1px solid var(--color-lab-border);
		overflow-x: auto;
		overflow-y: hidden;
	}

	.tab {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 8px;
		background: transparent;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
		font-weight: 600;
		white-space: nowrap;
		cursor: pointer;
	}

	.tab.active {
		color: var(--color-lab-text);
		border-bottom-color: var(--color-lab-blue);
	}

	.tab:focus-visible {
		outline: 2px solid var(--color-lab-blue);
		outline-offset: -2px;
	}

	.no-data-mark {
		padding: 0 4px;
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		font-size: 0.5625rem;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-secondary);
	}

	.status-line {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		padding: 0 2px;
	}

	.status-line .age {
		color: var(--color-lab-text);
		font-weight: 600;
	}

	.warn {
		color: var(--color-lab-yellow);
	}

	.dot {
		opacity: 0.5;
	}

	.state-line {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
	}

	.state-line.warn {
		color: var(--color-lab-yellow);
	}

	.panel {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
	}

	.skeleton {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 14px 16px;
	}

	.bar {
		display: block;
		height: 10px;
		width: 75%;
		background: var(--color-lab-border);
	}

	.bar.short {
		width: 50%;
	}

	.verdict {
		display: flex;
		align-items: flex-start;
		gap: 16px;
		padding: 14px 16px;
	}

	.verdict-text {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.headline {
		margin: 0;
		font-size: 1.25rem;
		font-weight: 700;
	}

	.reason {
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
	}

	.verdict-ev {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 2px;
		text-align: right;
	}

	.mono {
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}

	.ev-big {
		font-size: 1.375rem;
	}

	.ev-big.gain {
		color: var(--color-lab-green);
	}

	.ev-big.loss {
		color: var(--color-lab-red);
	}

	.per,
	.divine-line {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
	}

	.divine-line {
		color: var(--color-lab-text);
	}

	.split-row {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
	}

	.split-label.moved {
		color: var(--color-lab-yellow);
	}

	.split-row :global(.btn:focus-visible) {
		outline: 2px solid var(--color-lab-blue);
		outline-offset: 1px;
	}

	.grid {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1.3fr);
		gap: 10px;
		align-items: start;
	}

	.column-head {
		display: flex;
		align-items: baseline;
		gap: 8px;
		padding: 10px 12px;
	}

	.column-title {
		font-size: 0.6875rem;
		font-weight: 700;
		letter-spacing: 0.06em;
	}

	.column-sub {
		font-size: 0.6875rem;
		color: var(--color-lab-text-secondary);
	}

	.untiered {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.35rem;
		padding: 0.6rem 0.75rem;
		border-top: 1px solid var(--color-lab-border);
	}

	.empty {
		margin: 0;
		padding: 10px 12px;
		border-top: 1px solid var(--color-lab-border);
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
	}

	.side {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.side-panel {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 14px;
	}

	.small {
		margin: 0;
		font-size: 0.6875rem;
		color: var(--color-lab-text-secondary);
	}

	.small.warn {
		color: var(--color-lab-yellow);
	}

	.cost-line {
		display: flex;
		align-items: center;
		gap: 4px;
		margin: 0;
		font-size: 0.8125rem;
	}

	.cost {
		font-size: 1rem;
	}

	.per-divine {
		margin: 0;
		font-size: 0.8125rem;
	}

	.ev-rows {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 6px 12px;
		margin: 0;
		font-size: 0.75rem;
	}

	.ev-rows dt {
		color: var(--color-lab-text-secondary);
	}

	.ev-rows dd {
		margin: 0;
		text-align: right;
	}

	.ev-rows.yield {
		padding-top: 8px;
		border-top: 1px solid var(--color-lab-border);
	}

	.no-data {
		padding: 18px 20px;
	}

	.no-data-title {
		margin: 0 0 8px;
		font-size: 0.875rem;
		font-weight: 700;
	}

	.no-data-text {
		margin: 0;
		font-size: 0.8125rem;
		line-height: 1.5;
		color: var(--color-lab-text-secondary);
	}
</style>
