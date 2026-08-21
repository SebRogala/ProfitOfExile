<script lang="ts">
	/**
	 * The Currency Exchange table's filter bar: the two rule layers, the quality
	 * gates, the numeric bounds, and what they have left on screen.
	 *
	 * Four rows, coarse to fine — the sixteen category pills, then the item chips
	 * that override them, then the collapsed Gates row, then the numbers. An item
	 * rule beats its category and Hide beats Only (`applyRules`); the chips say so
	 * on their face when the two layers disagree, because a chip that reads "Only"
	 * inside a hidden category otherwise looks like a rule that is not working.
	 *
	 * The GATES row (POE-191) ships collapsed and, since POE-193, ships OFF —
	 * with the single exception POE-196 added, the trash-price knob. Five of
	 * the six empty boxes on this bar therefore read the same way, as a filter
	 * that is not running; the sixth is the one place the bar has to say a
	 * DIFFERENT thing out loud, which it does by showing its default in the
	 * placeholder where the others show `off`. The row badges how many knobs the
	 * reader has moved off default — a row nobody opens still hides rows, and the
	 * badge is the only thing on screen that says so. The old server levels are a
	 * recommendation each knob's tooltip names, never a state the reader inherits.
	 *
	 * The search box sits beside the counter on the last row because the two read
	 * as one sentence — what the reader is looking for, and how much of the table
	 * is left. It is a view filter and not a rule: nothing about it is persisted
	 * and Clear does not empty it.
	 *
	 * Presentation only. It holds two pieces of state — whether the Add popover is
	 * open, and whether the Gates row is expanded — and every rule, knob, bound,
	 * query and count is a prop; every change leaves through a callback so the page
	 * owns what is persisted (ADR-013). Neither piece of state is persisted: both
	 * are where a control is on screen, not what it is set to.
	 */
	import { gateDefaults, movedGates, overridesCategory, parseGate } from '$lib/exchange/filters';
	import type {
		CategoryRuleState,
		CategoryRules,
		ExchangeItem,
		Gates,
		GateInputs,
		ItemRule
	} from '$lib/exchange/filters';
	import { iconSrc, type ExchangeUnit } from '$lib/exchange/view';
	import ExchangeCategoryPills from './ExchangeCategoryPills.svelte';
	import ExchangeItemPicker from './ExchangeItemPicker.svelte';
	import ItemIcon from './ItemIcon.svelte';
	import Tooltip from './Tooltip.svelte';
	import { EXCHANGE_TOOLTIPS } from '$lib/tooltips';

	let {
		categories,
		categoryRules,
		itemRules,
		items,
		gateInputs,
		gates,
		investMin,
		investMax,
		unit,
		divineChaosRate,
		search,
		counts,
		apiBase,
		oncategoryrule,
		onitemrule,
		ongate,
		ongatedefaults,
		oninvestmin,
		oninvestmax,
		onunit,
		onsearch,
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
		/**
		 * The six gate knobs as their raw persisted strings — what the boxes show,
		 * so a half-typed number stays put while it is being typed.
		 */
		gateInputs: GateInputs;
		/**
		 * The same six as `parseGates` read them — what the boxes are MEASURED by.
		 * Passed already parsed rather than re-parsed here because the page has to
		 * parse them anyway to filter with, and a second parse in the view is a
		 * second place for the badge and the table to disagree about whether a knob
		 * is at its default.
		 */
		gates: Gates;
		/**
		 * The two investment bounds as their raw persisted strings; "" is "filter
		 * off". They are compared against the play's WORTHWHILE SCALE, not against
		 * one exchange (POE-192) — which is why the page stores them under keys that
		 * say so, and why the row is labelled Run cost rather than Investment: the
		 * table's Investment column is the per-exchange figure, and one word for two
		 * sizes is how a reader types a ceiling two orders of magnitude too low.
		 */
		investMin: string;
		investMax: string;
		unit: ExchangeUnit;
		/** The newest hour's chaos value of one divine; 0 when that hour had none. */
		divineChaosRate: number;
		/** The raw search text, as typed — `matchesSearch` owns the trimming. */
		search: string;
		/**
		 * What survived, out of what arrived, and what took the difference —
		 * counted AFTER the search, which is one of the things hiding rows.
		 *
		 * The gates are counted apart from everything else because they are the one
		 * filter whose controls are behind a collapsed row: the split is what points
		 * at the row that would give those rows back, and a reader who has armed a
		 * knob and forgotten it needs the count more than the pills need theirs.
		 * Everything else — the pills, the chips, the investment bounds and the
		 * search — shares the other figure. Since POE-196 this one is non-zero out
		 * of the box: the trash-price knob ships armed, and its rows are exactly
		 * the ones a reader has to be told are a default rather than an absence.
		 */
		counts: { shown: number; total: number; hiddenByGates: number; hiddenByFilters: number };
		apiBase: string;
		/** The pill's NEXT state, per `cycleCategoryRule`; `undefined` is neutral. */
		oncategoryrule: (category: string, state: CategoryRuleState | undefined) => void;
		/** `undefined` removes the item's rule. */
		onitemrule: (item: { id: string; name: string }, state: CategoryRuleState | undefined) => void;
		/**
		 * One knob's raw box contents. Keyed rather than six `ongateminprofit`
		 * props: the six are one control group set the same way, and the page maps
		 * the key to a preference in one place instead of six one-line callbacks.
		 */
		ongate: (knob: keyof Gates, value: string) => void;
		/**
		 * Puts all six knobs back to unset, which IS the default (`parseGate`):
		 * five gates off and the trash-price floor back at its shipped level
		 * (POE-196). The shipped state, not a blank one.
		 */
		ongatedefaults: () => void;
		oninvestmin: (value: string) => void;
		oninvestmax: (value: string) => void;
		onunit: (unit: ExchangeUnit) => void;
		/** The raw box contents; `''` is the search off. */
		onsearch: (query: string) => void;
		/**
		 * Clears the rules and the investment bounds — never the gates, the sort,
		 * mode or the search. The gates are standing policy rather than a question
		 * the reader asked once, and they have their own Defaults; the search is not
		 * persisted and has its own ×, so sweeping either up here would make Clear
		 * the second control that undoes them.
		 */
		onclear: () => void;
	} = $props();

	/**
	 * The six knobs as the row draws them, in the order a reader meets a play: is
	 * the thing worth anything at all, is the profit worth it, is the market real,
	 * is the price fine enough, is the edge more than rounding, is the return
	 * enough. Labels double as the tooltip keys so the two cannot drift apart.
	 *
	 * The trash-price knob leads because it is the only one that is already doing
	 * something when the row is first opened: a reader who came here to find out
	 * why a sub-chaos play is missing should meet that box before the five that
	 * are empty.
	 *
	 * The placeholder says what an EMPTY box does — `off` for the five that ship
	 * unarmed, and the actual default for the one that does not (`gatePlaceholder`
	 * reads it from `gateDefaults` rather than restating it, so the box cannot
	 * come to claim a level the build has moved). A number in that glyph is what
	 * makes the shipped floor discoverable; a `off` there would be the bar lying
	 * about the one filter the reader did not set. The levels worth typing into
	 * the other five are suggestions rather than states, so they stay in the
	 * tooltips beside the reason to want them.
	 */
	const GATE_FIELDS: {
		knob: keyof Gates;
		label: string;
		aria: string;
		unit: string;
	}[] = [
		{
			knob: 'minItemPrice',
			label: 'Min item price',
			aria: 'Minimum entry price in chaos per exchange',
			unit: 'c each'
		},
		{
			knob: 'minRoiChaos',
			label: 'Min profit',
			aria: 'Minimum profit in chaos per exchange',
			unit: 'c each'
		},
		{
			knob: 'minTurnover',
			label: 'Min turnover',
			aria: 'Minimum market turnover in chaos per hour',
			unit: 'c/h'
		},
		{
			knob: 'maxTickPct',
			label: 'Max price step',
			aria: 'Maximum price step as a percent',
			unit: '%'
		},
		{
			knob: 'minEdgeTickRatio',
			label: 'Edge vs step',
			aria: 'Minimum return as a multiple of the price step',
			unit: '× step'
		},
		{
			knob: 'minRoiPct',
			label: 'Min return',
			aria: 'Minimum return percent',
			unit: '%'
		}
	];

	/** One spelling of "this box is doing nothing", shared by the five that are. */
	const GATE_OFF_PLACEHOLDER = 'off';

	/**
	 * What an empty box actually does, in the box. Derived from `gateDefaults`
	 * rather than written per field: the constant is the single owner of what
	 * unset runs at, and a hand-written `1` here would be a second place to change
	 * when it moves — the exact drift the empty-string persistence exists to
	 * prevent.
	 */
	function gatePlaceholder(knob: keyof Gates): string {
		const shipped = gateDefaults[knob];
		return shipped > 0 ? String(shipped) : GATE_OFF_PLACEHOLDER;
	}

	/**
	 * The shipped trash-price floor, or `null` when this build does not ship one.
	 *
	 * The same guard `gatePlaceholder` applies, hoisted so the collapsed hint
	 * cannot contradict the boxes: if POE-196's level is ever taken back to 0 the
	 * bar is wholly off again, and a hint naming a "0c floor" would be the one
	 * line on a shut row claiming a filter that is not running. A plain const —
	 * `gateDefaults` is a module constant and nothing reactive reaches it.
	 */
	const SHIPPED_ITEM_FLOOR = gateDefaults.minItemPrice > 0 ? gateDefaults.minItemPrice : null;

	let pickerOpen = $state(false);
	/**
	 * Shut on arrival. An armed gate runs whether or not the row is open and the
	 * badge says so, so the row is a place to go when the table is too noisy — not
	 * a wall of five numbers between the reader and the table on every launch. Not
	 * persisted: it is where a control is, not what it is set to.
	 */
	let gatesOpen = $state(false);
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

	/**
	 * The knobs that are not where this build ships them, as a set for the
	 * per-label lookup. The verdict itself is `movedGates` in `filters.ts` — it
	 * shares `gateDefaults` with the parser and the Defaults reset, and it is
	 * MOVED rather than ARMED since POE-196: a reader who typed 0 into the
	 * trash-price knob turned a shipped filter off and is counted, because the
	 * badge exists to say the table is not showing the shipped answer.
	 */
	const movedKnobs = $derived(new Set(movedGates(gates)));
</script>

<div class="filter-bar">
	<div class="row">
		<!-- The Only/Hide tooltip is the surface that carries the two-layer
		     semantics AND the "remembered across restarts" sentence the picker's
		     dropped footnote defers to — both layer labels carry it. -->
		<Tooltip text={EXCHANGE_TOOLTIPS['Only / Hide']} position="below">
			<span class="label">Categories</span>
		</Tooltip>
		<ExchangeCategoryPills {categories} {categoryRules} {oncategoryrule} />
	</div>

	<div class="row">
		<Tooltip text={EXCHANGE_TOOLTIPS['Only / Hide']} position="below">
			<span class="label">Items</span>
		</Tooltip>
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
		<Tooltip text={EXCHANGE_TOOLTIPS.Gates} position="below">
			<span class="label">Gates</span>
		</Tooltip>
		<!-- The badge is on the toggle, not inside the row, because it is the whole
		     reason to open a row that is shut: it is the only thing on a collapsed
		     bar that says a knob is somewhere other than where the build put it.
		     A collapsed row with no badge is the shipped bar, which since POE-196
		     is the trash-price floor and nothing else — so the hint beside it names
		     that floor rather than claiming the row filters nothing. -->
		<button class="disclose" aria-expanded={gatesOpen} onclick={() => (gatesOpen = !gatesOpen)}>
			<svg
				class="chevron"
				class:open={gatesOpen}
				width="9"
				height="9"
				viewBox="0 0 12 12"
				fill="none"
				stroke="currentColor"
				stroke-width="1.6"
				stroke-linecap="round"
				stroke-linejoin="round"
				aria-hidden="true"
			>
				<path d="M4 2 L8.5 6 L4 10" />
			</svg>
			<!-- One label in both states: the chevron and `aria-expanded` already say
			     which way the row is, and a text that changes with them is a third
			     spelling of the same fact. -->
			Quality bar
			{#if movedKnobs.size > 0}
				<span class="badge">{movedKnobs.size} changed</span>
			{/if}
		</button>
		{#if !gatesOpen}
			<span class="unit-hint">
				{#if movedKnobs.size > 0}
					item price, profit, turnover, price step, edge vs step, return — your settings run
					whether or not this row is open
				{:else if SHIPPED_ITEM_FLOOR !== null}
					item price, profit, turnover, price step, edge vs step, return — only the {SHIPPED_ITEM_FLOOR}c
					item-price floor is on
				{:else}
					item price, profit, turnover, price step, edge vs step, return — all off until you set one
				{/if}
			</span>
		{/if}
	</div>

	{#if gatesOpen}
		<div class="row gates">
			{#each GATE_FIELDS as field (field.knob)}
				<Tooltip text={EXCHANGE_TOOLTIPS[field.label]} position="below">
					<span class="label gate-label" class:moved={movedKnobs.has(field.knob)}>{field.label}</span
					>
				</Tooltip>
				<input
					class="amount mono"
					type="text"
					inputmode="decimal"
					placeholder={gatePlaceholder(field.knob)}
					aria-label={field.aria}
					value={gateInputs[field.knob]}
					oninput={(e) => ongate(field.knob, e.currentTarget.value)}
					onblur={(e) => {
						// Mid-typing the raw string stays, so "1" en route to "10" is not
						// fought; a value left behind snaps to what the gate actually runs
						// at — "abc" showing while the default filters would be a control
						// lying about itself.
						const raw = e.currentTarget.value;
						if (raw.trim() === '') return;
						const parsed = parseGate(raw, gateDefaults[field.knob]);
						if (String(parsed) !== raw) ongate(field.knob, String(parsed));
					}}
				/>
				<span class="unit-hint gate-unit">{field.unit}</span>
			{/each}

			<div class="spacer"></div>

			<button
				class="clear"
				title={SHIPPED_ITEM_FLOOR !== null
					? `Empties all six boxes, back to the shipped state: the ${SHIPPED_ITEM_FLOOR}c item-price floor on, the other five off — the table shows everything the server served above the trash tier.`
					: 'Empties all six boxes, which turns every gate off — the table shows everything the server served.'}
				onclick={ongatedefaults}>Defaults</button
			>
		</div>
	{/if}

	<div class="row">
		<!-- Run cost, not Investment: since POE-192 these bounds are compared
		     against the Scale column's investment — what the play ties up by the
		     time it has been repeated enough to be worth doing — while the table's
		     Investment column is one exchange. Two sizes, two names. -->
		<Tooltip text={EXCHANGE_TOOLTIPS['Run cost']} position="below">
			<span class="label">Run cost</span>
		</Tooltip>
		<input
			class="amount mono"
			type="text"
			inputmode="decimal"
			placeholder="min"
			aria-label="Minimum investment for the worthwhile run"
			value={investMin}
			oninput={(e) => oninvestmin(e.currentTarget.value)}
		/>
		<span class="to">to</span>
		<input
			class="amount mono"
			type="text"
			inputmode="decimal"
			placeholder="max"
			aria-label="Maximum investment for the worthwhile run"
			value={investMax}
			oninput={(e) => oninvestmax(e.currentTarget.value)}
		/>
		<div class="segmented" role="group" aria-label="Run cost unit">
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

		<!-- The two gain knobs that used to sit here are both gone. Min ROI% left in
		     POE-191 (it is the fifth gate, on the gate side of the file, so it
		     belongs beside the four floors it shares a contract with); Min gain left
		     in POE-192, when the run-level floor became the fixed 100c scale target
		     and the per-flip floor was already the Min profit gate — a third gain
		     knob had nothing of its own left to say. -->

		<div class="spacer"></div>

		<div class="search">
			<svg
				width="12"
				height="12"
				viewBox="0 0 14 14"
				fill="none"
				stroke="currentColor"
				stroke-width="1.6"
				stroke-linecap="round"
				aria-hidden="true"
			>
				<circle cx="6" cy="6" r="4.5" />
				<path d="M9.4 9.4 L12.5 12.5" />
			</svg>
			<input
				type="text"
				placeholder="Search names…"
				aria-label="Search plays by item name"
				value={search}
				oninput={(e) => onsearch(e.currentTarget.value)}
				onkeydown={(e) => {
					// On the input rather than on the window: Escape empties the box
					// the reader is typing in, and a global listener would also fire
					// for a search they cannot see and fight the picker's dismissal.
					// When the box had text, the press is consumed — one Escape does
					// one thing, so it cannot also close an open picker.
					if (e.key === 'Escape' && search !== '') {
						e.stopPropagation();
						onsearch('');
					}
				}}
			/>
			{#if search !== ''}
				<button class="remove" aria-label="Clear the search" onclick={() => onsearch('')}
					>&times;</button
				>
			{/if}
		</div>

		<!-- Attribution, not one lump: the gates hide rows on a bar the reader has
		     never touched, so "hidden by filters" over an empty bar reads as a
		     broken table. Each clause is dropped when it is zero — a counter that
		     says "0 hidden by gates" is noise on the common case. -->
		<Tooltip text={EXCHANGE_TOOLTIPS.Counter} position="above">
			<span class="counter">
				<span class="mono shown">{counts.shown}</span> of {counts.total} plays
				{#if counts.hiddenByGates > 0}
					<span class="sep">&middot;</span> {counts.hiddenByGates} hidden by gates
				{/if}
				{#if counts.hiddenByFilters > 0}
					<span class="sep">&middot;</span> {counts.hiddenByFilters} hidden by filters
				{/if}
			</span>
		</Tooltip>
		<button
			class="clear"
			title="Clears the category and item rules and the run investment bounds. The gates, the search, the sort and the density are left alone."
			onclick={onclear}>Clear</button
		>
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

	/* Each knob's label sits away from the previous knob's unit hint, so the group
	   reads as five pairs rather than ten loose controls. The first one is
	   indented by the same amount, which lines the group up under the toggle
	   above it. */
	.gate-label {
		margin-left: 10px;
	}

	/* A moved knob is named on the row as well as counted on the toggle: the
	   badge says how many, the label says which. */
	.gate-label.moved {
		color: var(--color-lab-text);
	}

	.gate-unit {
		margin-left: -2px;
	}

	.disclose {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: transparent;
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		color: var(--color-lab-text-secondary);
		padding: 3px 9px;
		font-size: 0.75rem;
		font-family: inherit;
		cursor: pointer;
	}

	.disclose:hover {
		color: var(--color-lab-text);
		background: rgba(255, 255, 255, 0.05);
	}

	.disclose:focus-visible {
		outline: 1px solid var(--color-lab-blue);
		outline-offset: -1px;
	}

	.chevron {
		transition: transform 0.12s ease;
	}

	.chevron.open {
		transform: rotate(90deg);
	}

	.badge {
		font-size: 0.5625rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: color-mix(in oklab, var(--color-lab-blue) 70%, var(--color-lab-text));
		border: 1px solid rgba(59, 130, 246, 0.45);
		border-radius: 2px;
		padding: 0 3px;
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

	/* The box is the bordered field, not the input inside it: the magnifier and
	   the × have to sit within the same frame the reader clicks into. */
	.search {
		display: flex;
		align-items: center;
		gap: 6px;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		padding: 2px 6px;
		color: #6b7280;
	}

	.search:focus-within {
		border-color: var(--color-lab-blue);
	}

	.search input {
		width: 140px;
		background: transparent;
		border: none;
		padding: 1px 0;
		font-family: inherit;
		font-size: 0.75rem;
		color: var(--color-lab-text);
	}

	.search input:focus {
		outline: none;
	}

	.counter {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		white-space: nowrap;
	}

	.counter .shown {
		color: var(--color-lab-text);
	}

	.counter .sep {
		opacity: 0.5;
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
