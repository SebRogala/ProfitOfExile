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
	 * with the two exceptions the trash-price knobs are, POE-196's chaos floor and
	 * its divine twin (2026-08-23). Five of the seven empty boxes on this bar
	 * therefore read the same way, as a filter that is not running; the other two
	 * are where the bar has to say a DIFFERENT thing out loud, which it does by
	 * showing their defaults in the placeholder where the others show `off`. The
	 * row badges how many knobs the reader has moved off default — a row nobody
	 * opens still hides rows, and the badge is the only thing on screen that says
	 * so. The old server levels are a recommendation each knob's tooltip names,
	 * never a state the reader inherits.
	 *
	 * The search box sits beside the counter on the last row because the two read
	 * as one sentence — what the reader is looking for, and how much of the table
	 * is left. Since POE-222 the counter also carries the ROW CAP: how much of
	 * that is drawn, and the one click that draws the rest. The cap's own picker
	 * is in the page head with Sort and Density, where the other view-shaping
	 * picks live and where Clear does not reach. It is a view filter and not a rule: nothing about it is persisted
	 * and Clear does not empty it.
	 *
	 * Presentation only. It holds two pieces of state — whether the Add popover is
	 * open, and whether the Gates row is expanded — and every rule, knob, bound,
	 * query and count is a prop; every change leaves through a callback so the page
	 * owns what is persisted (ADR-013). Neither piece of state is persisted: both
	 * are where a control is on screen, not what it is set to.
	 */
	import {
		gateDefaults,
		movedGates,
		overridesCategory,
		snapGateInput
	} from '$lib/exchange/filters';
	import type {
		CategoryRuleState,
		CategoryRules,
		ExchangeItem,
		Gates,
		GateInputs,
		ItemRule
	} from '$lib/exchange/filters';
	import { iconSrc, type ExchangeUnit, type RowCounts } from '$lib/exchange/view';
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
		onshowall,
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
		 * The seven gate knobs as their raw persisted strings — what the boxes show,
		 * so a half-typed number stays put while it is being typed.
		 */
		gateInputs: GateInputs;
		/**
		 * The same seven as `parseGates` read them — what the boxes are MEASURED by.
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
		 * say so.
		 *
		 * The bound reads `runInvestment(play)` — the run's cost — while the table's
		 * Investment column reads `moneyColumns(play).investment`, the size the ROW
		 * displays. Since the owner rulings of 2026-08-22 those are different
		 * questions on EVERY row, divine entries included: the row renders one
		 * posting of its buy market, and the run's cost is printed in the Scale
		 * column's "N c in" sub-line instead. The bound follows the MEANING —
		 * a bankroll ceiling is a run-sized question whatever the row prints — and
		 * the Run cost tooltip says which cell carries the figure it compares
		 * against. The label stays "Run cost" for a second reason too: the GATES row
		 * directly above this one judges PER-EXCHANGE numbers (Min item price, Min
		 * profit), so calling this bound "Investment" would read as the same size as
		 * those. See the inline comment beside the label, which is the live reason.
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
		 * of the box: the trash-price knobs ship armed, and their rows are exactly
		 * the ones a reader has to be told are a default rather than an absence.
		 *
		 * `shown` is what the TABLE DRAWS and `matched` what the filters left, and
		 * since POE-222 those differ: the row cap slices the head of the list. The
		 * counter names the cap whenever `capped` is true and puts All one click
		 * away, which is what makes a shipped cap disclosure rather than a filter
		 * (`applyRowCap`). Neither `hidden` figure counts a capped row — the cap
		 * takes nothing, it just stops drawing.
		 */
		counts: RowCounts;
		apiBase: string;
		/** The pill's NEXT state, per `cycleCategoryRule`; `undefined` is neutral. */
		oncategoryrule: (category: string, state: CategoryRuleState | undefined) => void;
		/** `undefined` removes the item's rule. */
		onitemrule: (item: { id: string; name: string }, state: CategoryRuleState | undefined) => void;
		/**
		 * One knob's raw box contents. Keyed rather than seven `ongateminprofit`
		 * props: the seven are one control group set the same way, and the page maps
		 * the key to a preference in one place instead of seven one-line callbacks.
		 */
		ongate: (knob: keyof Gates, value: string) => void;
		/**
		 * Puts all seven knobs back to unset, which IS the default (`parseGate`):
		 * five gates off and both trash-price floors back at their shipped levels
		 * (POE-196, and the divine twin of 2026-08-23). The shipped state, not a
		 * blank one.
		 */
		ongatedefaults: () => void;
		oninvestmin: (value: string) => void;
		oninvestmax: (value: string) => void;
		onunit: (unit: ExchangeUnit) => void;
		/** The raw box contents; `''` is the search off. */
		onsearch: (query: string) => void;
		/**
		 * Lifts the row cap to All. The counter's own affordance and the only
		 * control on this bar that is not a filter: it changes how much of the
		 * table is drawn, not what is left of it. Rendered only while the cap is
		 * actually holding rows back, because a "show all" over a list that is all
		 * shown is a button that does nothing.
		 */
		onshowall: () => void;
		/**
		 * Clears the rules and the investment bounds — never the gates, the sort,
		 * mode, the row cap or the search. The gates are standing policy rather than a question
		 * the reader asked once, and they have their own Defaults; the search is not
		 * persisted and has its own ×, so sweeping either up here would make Clear
		 * the second control that undoes them.
		 */
		onclear: () => void;
	} = $props();

	/**
	 * The seven knobs as the row draws them, in the order a reader meets a play: is
	 * the thing worth anything at all in chaos, is it worth anything at all in
	 * divine, is the profit worth it, is the market real, is the price fine enough,
	 * is the edge more than rounding, is the return enough. Seven questions for
	 * seven boxes — the two price knobs ask ONE question in two currencies, and
	 * they get a clause each because they are two boxes a reader sets separately.
	 * Labels double as the tooltip keys so the two cannot drift apart.
	 *
	 * The two trash-price knobs lead, and lead ADJACENTLY, because they are the
	 * only ones already doing something when the row is first opened: a reader who
	 * came here to find out why a cheap play is missing should meet those boxes
	 * before the five that are empty. Adjacent because they are one line drawn in
	 * two currencies — a reader who disarms one and still sees rows missing has
	 * the other right beside it, rather than at the far end of the row.
	 *
	 * The placeholder says what an EMPTY box does — `off` for the five that ship
	 * unarmed, and the actual default for the two that do not (`gatePlaceholder`
	 * reads it from `gateDefaults` rather than restating it, so no box can come to
	 * claim a level the build has moved). A number in that glyph is what makes a
	 * shipped floor discoverable; an `off` there would be the bar lying about the
	 * filters the reader did not set. The levels worth typing into the other five
	 * are suggestions rather than states, so they stay in the tooltips beside the
	 * reason to want them.
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
			knob: 'minItemPriceDiv',
			label: 'Min item price (div)',
			aria: 'Minimum entry price in divine per exchange, on divine-quoted plays only',
			unit: 'div each'
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
	 * The shipped trash-price floors as a noun phrase the shut row and the
	 * Defaults button both build a sentence out of — `the 0.5c and 0.4 div
	 * item-price floors` — plus the verb that agrees with it. `null` when this
	 * build ships neither floor.
	 *
	 * The same guard `gatePlaceholder` applies, widened to a pair and hoisted so
	 * the collapsed hint cannot contradict the boxes: if either level is ever
	 * taken back to 0 the phrase drops it and the verb follows, and if both are
	 * taken back the bar is wholly off again. A hint naming a "0c floor" would be
	 * the one line on a shut row claiming a filter that is not running.
	 *
	 * Built from `gateDefaults` rather than written out, for the reason
	 * `gatePlaceholder` gives: the constant is the single owner of what unset runs
	 * at, and a hand-written level here would be a second place to change when it
	 * moves. Plain consts — `gateDefaults` is a module constant and nothing
	 * reactive reaches it.
	 */
	const SHIPPED_FLOOR_LEVELS = [
		gateDefaults.minItemPrice > 0 ? `${gateDefaults.minItemPrice}c` : null,
		gateDefaults.minItemPriceDiv > 0 ? `${gateDefaults.minItemPriceDiv} div` : null
	].filter((level): level is string => level !== null);
	const SHIPPED_FLOORS: { phrase: string; verb: string } | null =
		SHIPPED_FLOOR_LEVELS.length === 0
			? null
			: {
					phrase: `the ${SHIPPED_FLOOR_LEVELS.join(' and ')} item-price ${SHIPPED_FLOOR_LEVELS.length === 1 ? 'floor' : 'floors'}`,
					verb: SHIPPED_FLOOR_LEVELS.length === 1 ? 'is' : 'are'
				};

	/**
	 * How many knobs this build ships OFF — the figure the Defaults button's
	 * wording leans on ("the other five off").
	 *
	 * Counted rather than written, because the sentence is a claim about
	 * `gateDefaults` and a hand-written "five" would survive a level being armed
	 * or disarmed while quietly becoming false.
	 */
	const GATES_SHIPPED_OFF = Object.values(gateDefaults).filter((level) => level === 0).length;

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
	 * MOVED rather than ARMED since POE-196: a reader who typed 0 into either
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
		     is the trash-price floors and nothing else — so the hint beside it names
		     those floors rather than claiming the row filters nothing. -->
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
					item price, item price (div), profit, turnover, price step, edge vs step, return — your
					settings run whether or not this row is open
				{:else if SHIPPED_FLOORS !== null}
					item price, item price (div), profit, turnover, price step, edge vs step, return — only {SHIPPED_FLOORS.phrase}
					{SHIPPED_FLOORS.verb} on
				{:else}
					item price, item price (div), profit, turnover, price step, edge vs step, return — all off
					until you set one
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
						// lying about itself. `snapGateInput` owns the decision, including
						// the one that has to be blank rather than the default's digits.
						const snapped = snapGateInput(e.currentTarget.value, gateDefaults[field.knob]);
						if (snapped !== null) ongate(field.knob, snapped);
					}}
				/>
				<span class="unit-hint gate-unit">{field.unit}</span>
			{/each}

			<div class="spacer"></div>

			<button
				class="clear"
				title={SHIPPED_FLOORS !== null
					? `Empties all seven boxes, back to the shipped state: ${SHIPPED_FLOORS.phrase} on, the other ${GATES_SHIPPED_OFF} off — the table shows everything the server served above the trash tier.`
					: 'Empties all seven boxes, which turns every gate off — the table shows everything the server served.'}
				onclick={ongatedefaults}>Defaults</button
			>
		</div>
	{/if}

	<div class="row">
		<!-- Run cost, not Investment: these bounds are compared against what the
		     play ties up by the time it has been repeated enough to be worth
		     doing (POE-192) — the figure the SCALE column's "N c in" sub-line
		     prints on every row, and NOT the one the Investment column prints,
		     since every row displays one posting of its buy market instead
		     (owner rulings, 2026-08-22). The label is
		     load-bearing for that reason and for one more: the GATES row directly
		     above still judges per-exchange numbers (Min item price, Min profit),
		     and calling this one "Investment" would read as the same size as
		     those. -->
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
		     says "0 hidden by gates" is noise on the common case.

		     The capped sentence is a THIRD figure and not a replacement (POE-222):
		     what is drawn, out of what the filters left, out of what the server
		     sent. A cap that printed only "50 of 1443" would read as 1393 rows
		     taken by something, which is what the gate/filter clauses beside it
		     mean; naming the cap and its two totals is the difference between
		     pagination and a hidden filter, and "show all" is the way past it.
		     Uncapped, the counter says exactly what it said before the cap
		     existed — `shown` and `matched` are the same number then, and a "of
		     50 plays · 1443 served" on a table with nothing held back would be
		     arithmetic nobody asked for. -->
		<Tooltip text={EXCHANGE_TOOLTIPS.Counter} position="above">
			<span class="counter">
				{#if counts.capped}
					<span class="mono shown">{counts.shown}</span> of {counts.matched} matching
					<span class="cap-note">(cap)</span>
					<span class="sep">&middot;</span> {counts.total} served
				{:else}
					<span class="mono shown">{counts.shown}</span> of {counts.total} plays
				{/if}
				{#if counts.hiddenByGates > 0}
					<span class="sep">&middot;</span> {counts.hiddenByGates} hidden by gates
				{/if}
				{#if counts.hiddenByFilters > 0}
					<span class="sep">&middot;</span> {counts.hiddenByFilters} hidden by filters
				{/if}
			</span>
		</Tooltip>
		{#if counts.capped}
			<button
				class="show-all"
				title="Draws every matching play. Sets the Show pick to All, so it stays lifted until you set it back."
				onclick={onshowall}>show all</button
			>
		{/if}
		<button
			class="clear"
			title="Clears the category and item rules and the run investment bounds. The gates, the search, the sort, the row cap and the density are left alone."
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
	   reads as seven pairs rather than fourteen loose controls. The first one is
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

	/* Dimmer than the figures it qualifies: it names the reason the first number
	   is small, and is not itself one of the counts. */
	.counter .cap-note {
		opacity: 0.7;
	}

	/* The counter's own affordance, so it reads as part of that sentence rather
	   than as a second Clear — same underline, lowercase, no gap of its own. */
	.show-all,
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

	.show-all:hover,
	.clear:hover {
		color: var(--color-lab-text);
	}

	.mono {
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}
</style>
