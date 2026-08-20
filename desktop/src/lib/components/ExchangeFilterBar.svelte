<script lang="ts">
	/**
	 * The Currency Exchange table's filter bar: the two rule layers, the numeric
	 * bounds, and what they have left on screen.
	 *
	 * Three rows, coarse to fine — the sixteen category pills, then the item
	 * chips that override them, then the numbers. An item rule beats its
	 * category and Hide beats Only (`applyRules`); the chips say so on their face
	 * when the two layers disagree, because a chip that reads "Only" inside a
	 * hidden category otherwise looks like a rule that is not working.
	 *
	 * Presentation only. It holds one piece of state — whether the Add popover is
	 * open — and every rule, bound and count is a prop; every change leaves
	 * through a callback so the page owns what is persisted (ADR-013).
	 */
	import { overridesCategory } from '$lib/exchange/filters';
	import type {
		CategoryRuleState,
		CategoryRules,
		ExchangeItem,
		ItemRule
	} from '$lib/exchange/filters';
	import { iconSrc, type ExchangeUnit } from '$lib/exchange/view';
	import ExchangeCategoryPills from './ExchangeCategoryPills.svelte';
	import ExchangeItemPicker from './ExchangeItemPicker.svelte';
	import ItemIcon from './ItemIcon.svelte';

	let {
		categories,
		categoryRules,
		itemRules,
		items,
		investMin,
		investMax,
		minRoiPct,
		minGain,
		unit,
		divineChaosRate,
		counts,
		apiBase,
		oncategoryrule,
		onitemrule,
		oninvestmin,
		oninvestmax,
		onminroipct,
		onmingain,
		onunit,
		onclear
	}: {
		/** The sidebar taxonomy, straight off the response — never a local copy. */
		categories: string[];
		categoryRules: CategoryRules;
		itemRules: ItemRule[];
		/**
		 * The picker's universe: `itemUniverse` over the UNFILTERED response
		 * plays, never the filtered list. An item hidden by its own rule has no
		 * play left on screen, and listing only what survived would take that
		 * item's row out of the picker — leaving the rule that hid it clearable
		 * from nowhere but its chip.
		 */
		items: ExchangeItem[];
		/** The four bounds as their raw persisted strings; "" is "filter off". */
		investMin: string;
		investMax: string;
		minRoiPct: string;
		minGain: string;
		unit: ExchangeUnit;
		/** The newest hour's chaos value of one divine; 0 when that hour had none. */
		divineChaosRate: number;
		counts: { shown: number; total: number };
		apiBase: string;
		/** The pill's NEXT state, per `cycleCategoryRule`; `undefined` is neutral. */
		oncategoryrule: (category: string, state: CategoryRuleState | undefined) => void;
		/** `undefined` removes the item's rule. */
		onitemrule: (item: { id: string; name: string }, state: CategoryRuleState | undefined) => void;
		oninvestmin: (value: string) => void;
		oninvestmax: (value: string) => void;
		onminroipct: (value: string) => void;
		onmingain: (value: string) => void;
		onunit: (unit: ExchangeUnit) => void;
		/** Clears the rules and the bounds — never the quantity, sort or mode. */
		onclear: () => void;
	} = $props();

	let pickerOpen = $state(false);
	/**
	 * The Add button and the popover, as one dismissal root: a pointerdown on
	 * Add must not read as "outside", or the popover would close on the same
	 * click that is toggling it and lose the query typed into it.
	 */
	let anchor = $state<HTMLDivElement | null>(null);

	/**
	 * The chip's artwork and category, looked up in the current response.
	 *
	 * A rule outlives the response that created it (it is persisted, and the
	 * horizon or mode can change under it), so an item can have a chip and no
	 * entry here — `ItemRule` deliberately stores the name and not the icon. Such
	 * a chip renders as text, which is exactly the rule it represents.
	 */
	const known = $derived(new Map(items.map((item) => [item.id, item])));

	const onlyRules = $derived(itemRules.filter((rule) => rule.state === 'only'));
	const hideRules = $derived(itemRules.filter((rule) => rule.state === 'hide'));

	/** Divine bounds are unconvertible in an hour that carried no divine trade. */
	const divineUsable = $derived(divineChaosRate > 0);

	/** The chip's badge verdict — the rule itself lives in `filters.ts`. */
	function overrides(rule: ItemRule): boolean {
		return overridesCategory(rule, known.get(rule.id)?.category, categoryRules);
	}
</script>

<div class="filter-bar">
	<div class="row">
		<span class="label">Categories</span>
		<ExchangeCategoryPills {categories} {categoryRules} {oncategoryrule} />
	</div>

	<div class="row">
		<span class="label">Items</span>
		{#if onlyRules.length > 0}
			<span class="group-label only-label">Only</span>
			{#each onlyRules as rule (rule.id)}
				<span class="chip chip-only">
					<ItemIcon src={iconSrc(apiBase, known.get(rule.id)?.icon ?? null)} alt="" size={16} />
					<span class="chip-name">{rule.name}</span>
					{#if overrides(rule)}
						<span class="override">overrides category</span>
					{/if}
					<button
						class="remove"
						aria-label="Remove the Only rule on {rule.name}"
						onclick={() => onitemrule(rule, undefined)}>&times;</button
					>
				</span>
			{/each}
		{/if}

		{#if hideRules.length > 0}
			<span class="group-label hide-label">Hide</span>
			{#each hideRules as rule (rule.id)}
				<span class="chip chip-hide">
					<ItemIcon src={iconSrc(apiBase, known.get(rule.id)?.icon ?? null)} alt="" size={16} />
					<span class="chip-name struck">{rule.name}</span>
					{#if overrides(rule)}
						<span class="override">overrides category</span>
					{/if}
					<button
						class="remove"
						aria-label="Remove the Hide rule on {rule.name}"
						onclick={() => onitemrule(rule, undefined)}>&times;</button
					>
				</span>
			{/each}
		{/if}

		<div class="add-anchor" bind:this={anchor}>
			<!-- A true toggle: the popover treats this whole anchor as itself, so
			     the pointerdown on Add never reads as a dismissal and the click
			     that follows closes an open popover instead of recreating it. -->
			<button class="add" aria-expanded={pickerOpen} onclick={() => (pickerOpen = !pickerOpen)}>
				<svg
					width="11"
					height="11"
					viewBox="0 0 12 12"
					fill="none"
					stroke="currentColor"
					stroke-width="1.6"
					stroke-linecap="round"
					aria-hidden="true"
				>
					<path d="M6 1.5 V10.5 M1.5 6 H10.5" />
				</svg>
				Add
			</button>

			{#if pickerOpen}
				<ExchangeItemPicker
					{items}
					{categories}
					{categoryRules}
					{itemRules}
					{apiBase}
					{anchor}
					{oncategoryrule}
					{onitemrule}
					onclose={() => (pickerOpen = false)}
				/>
			{/if}
		</div>
	</div>

	<div class="row">
		<span class="label">Investment</span>
		<input
			class="amount mono"
			type="text"
			inputmode="decimal"
			placeholder="min"
			aria-label="Minimum investment"
			value={investMin}
			oninput={(e) => oninvestmin(e.currentTarget.value)}
		/>
		<span class="to">to</span>
		<input
			class="amount mono"
			type="text"
			inputmode="decimal"
			placeholder="max"
			aria-label="Maximum investment"
			value={investMax}
			oninput={(e) => oninvestmax(e.currentTarget.value)}
		/>
		<div class="segmented" role="group" aria-label="Investment unit">
			<button
				class="segment"
				class:active={unit === 'chaos'}
				aria-pressed={unit === 'chaos'}
				onclick={() => onunit('chaos')}>Chaos</button
			>
			<button
				class="segment"
				class:active={unit === 'divine' && divineUsable}
				aria-pressed={unit === 'divine' && divineUsable}
				disabled={!divineUsable}
				title={divineUsable
					? 'Bounds typed in divine, converted at this hour’s rate.'
					: 'This hour carried no divine/chaos trade, so there is no rate to convert with.'}
				onclick={() => onunit('divine')}>Divine</button
			>
		</div>

		<span class="label spaced">Min ROI%</span>
		<input
			class="amount mono"
			type="text"
			inputmode="decimal"
			placeholder="e.g. 5"
			aria-label="Minimum ROI percent"
			value={minRoiPct}
			oninput={(e) => onminroipct(e.currentTarget.value)}
		/>

		<span class="label">Min gain</span>
		<input
			class="amount mono"
			type="text"
			inputmode="decimal"
			placeholder="e.g. 100"
			aria-label="Minimum gain in chaos"
			value={minGain}
			oninput={(e) => onmingain(e.currentTarget.value)}
		/>
		<span class="unit-hint">chaos</span>

		<div class="spacer"></div>

		<span class="counter">
			<span class="mono shown">{counts.shown}</span> of {counts.total} plays &middot;
			{counts.total - counts.shown} hidden by filters
		</span>
		<button class="clear" onclick={onclear}>Clear</button>
	</div>
</div>

<style>
	.filter-bar {
		display: flex;
		flex-direction: column;
		gap: 8px;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 8px 12px;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 7px;
		flex-wrap: wrap;
	}

	.label {
		font-size: 0.625rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-secondary);
		flex-shrink: 0;
	}

	/* The first row's label column, so the three layers line up under each
	   other; the later labels in a row sit at their natural width. */
	.row > .label:first-child {
		width: 74px;
	}

	.label.spaced {
		margin-left: 10px;
	}

	.group-label {
		font-size: 0.625rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.only-label {
		color: color-mix(in oklab, var(--color-lab-blue) 70%, var(--color-lab-text));
	}

	.hide-label {
		color: color-mix(in oklab, var(--color-lab-red) 70%, var(--color-lab-text));
		margin-left: 4px;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		border-radius: 3px;
		padding: 2px 6px 2px 4px;
		font-size: 0.75rem;
		white-space: nowrap;
	}

	.chip-only {
		background: rgba(59, 130, 246, 0.12);
		border: 1px solid rgba(59, 130, 246, 0.45);
		color: var(--color-lab-text);
	}

	.chip-hide {
		background: rgba(239, 68, 68, 0.1);
		border: 1px solid rgba(239, 68, 68, 0.4);
		color: var(--color-lab-text-secondary);
	}

	.chip-name.struck {
		text-decoration: line-through;
	}

	.override {
		font-size: 0.5625rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: color-mix(in oklab, var(--color-lab-blue) 70%, var(--color-lab-text));
		border: 1px solid rgba(59, 130, 246, 0.45);
		border-radius: 2px;
		padding: 0 3px;
	}

	.remove {
		background: transparent;
		border: none;
		color: var(--color-lab-text-secondary);
		font-family: inherit;
		font-size: 0.875rem;
		line-height: 1;
		padding: 0 1px;
		cursor: pointer;
	}

	.remove:hover {
		color: var(--color-lab-text);
	}

	.add-anchor {
		position: relative;
	}

	.add {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		background: transparent;
		border: 1px dashed var(--color-lab-border);
		border-radius: 3px;
		color: var(--color-lab-text-secondary);
		padding: 3px 9px;
		font-size: 0.75rem;
		font-family: inherit;
		cursor: pointer;
	}

	.add:hover {
		color: var(--color-lab-text);
		background: rgba(255, 255, 255, 0.05);
	}

	.amount {
		width: 68px;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		padding: 3px 8px;
		font-size: 0.75rem;
		color: var(--color-lab-text);
	}

	.amount:focus {
		outline: none;
		border-color: var(--color-lab-blue);
	}

	.to,
	.unit-hint {
		font-size: 0.75rem;
		color: #6b7280;
	}

	/* Same shape as SegmentedButtons, hand-rolled for the one thing it cannot
	   do: disable a single option when its rate is missing. */
	.segmented {
		display: flex;
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		overflow: hidden;
	}

	.segment {
		background: transparent;
		border: none;
		color: var(--color-lab-text-secondary);
		padding: 3px 8px;
		font-size: 0.6875rem;
		font-weight: 600;
		font-family: inherit;
		cursor: pointer;
	}

	.segment:hover:not(:disabled) {
		color: var(--color-lab-text);
		background: rgba(255, 255, 255, 0.05);
	}

	.segment:focus-visible {
		outline: 1px solid var(--color-lab-blue);
		outline-offset: -1px;
	}

	.segment.active {
		color: var(--color-lab-text);
		background: rgba(99, 102, 241, 0.2);
	}

	.segment:disabled {
		color: #4b5563;
		cursor: not-allowed;
	}

	.spacer {
		flex: 1;
	}

	.counter {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		white-space: nowrap;
	}

	.counter .shown {
		color: var(--color-lab-text);
	}

	.clear {
		background: transparent;
		border: none;
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
		font-family: inherit;
		cursor: pointer;
		text-decoration: underline;
		padding: 0;
	}

	.clear:hover {
		color: var(--color-lab-text);
	}

	.mono {
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}
</style>
