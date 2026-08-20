<script lang="ts">
	/**
	 * The Currency Exchange filter bar's "Add" popover: the coarse category layer
	 * on top, a search box, and one row per item the current response's plays are
	 * made of.
	 *
	 * The universe is the response's plays, not the server's whole item asset
	 * (`itemUniverse`): the client is never sent the full list, and a rule on an
	 * item no play touches is a chip that cannot change what is visible.
	 *
	 * Presentation only. The verdict a row is marked with comes from
	 * `effectiveRule`, and the caller owns every rule — this file emits the state
	 * a click asks for and renders what comes back.
	 */
	import { effectiveRule } from '$lib/exchange/filters';
	import type {
		CategoryRuleState,
		CategoryRules,
		ExchangeItem,
		ItemRule
	} from '$lib/exchange/filters';
	import { iconSrc } from '$lib/exchange/view';
	import ExchangeCategoryPills from './ExchangeCategoryPills.svelte';
	import ItemIcon from './ItemIcon.svelte';

	let {
		items,
		categories,
		categoryRules,
		itemRules,
		apiBase,
		anchor = null,
		oncategoryrule,
		onitemrule,
		onclose
	}: {
		/** The distinct items of the current response, from `itemUniverse`. */
		items: ExchangeItem[];
		/** The sixteen sidebar categories, as the response sent them. */
		categories: string[];
		categoryRules: CategoryRules;
		itemRules: ItemRule[];
		apiBase: string;
		/**
		 * The element that opened this popover, counted as part of it for
		 * dismissal — see the `$effect` below. `null` leaves the popover itself
		 * as the whole root, which is the right answer for a caller with no
		 * toggling control to protect.
		 */
		anchor?: HTMLElement | null;
		/** The pill's NEXT state, per `cycleCategoryRule`; `undefined` is neutral. */
		oncategoryrule: (category: string, state: CategoryRuleState | undefined) => void;
		/** `undefined` removes the item's rule. */
		onitemrule: (item: { id: string; name: string }, state: CategoryRuleState | undefined) => void;
		onclose: () => void;
	} = $props();

	let query = $state('');
	let root = $state<HTMLDivElement | null>(null);

	const ruleById = $derived(new Map(itemRules.map((rule) => [rule.id, rule.state])));

	/**
	 * Case-insensitive substring over the display name — what the reader types is
	 * the name they read on the row, never the `Metadata/Items/...` id.
	 */
	const results = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		if (needle === '') return items;
		return items.filter((item) => item.name.toLowerCase().includes(needle));
	});

	/**
	 * The Only/Hide pair is a toggle, not a cycle: clicking the state a row is
	 * already in clears the rule, which is the only way back to neutral from
	 * inside the popover.
	 */
	function toggle(item: ExchangeItem, state: CategoryRuleState) {
		const current = ruleById.get(item.id);
		onitemrule({ id: item.id, name: item.name }, current === state ? undefined : state);
	}

	/**
	 * Dismissal. `pointerdown` rather than `click`, because the click that opened
	 * this popover is still in flight when the listener is registered and would
	 * close it again immediately.
	 *
	 * The opening control counts as inside. Without that, a pointerdown on it
	 * dismisses the popover and the click that follows builds a fresh one —
	 * indistinguishable from "stayed open" except that the search query is gone.
	 */
	$effect(() => {
		const outside = (event: PointerEvent) => {
			const target = event.target as Node;
			if (root?.contains(target) || anchor?.contains(target)) return;
			onclose();
		};
		const escape = (event: KeyboardEvent) => {
			if (event.key === 'Escape') onclose();
		};
		window.addEventListener('pointerdown', outside);
		window.addEventListener('keydown', escape);
		return () => {
			window.removeEventListener('pointerdown', outside);
			window.removeEventListener('keydown', escape);
		};
	});
</script>

<div class="picker" bind:this={root} role="dialog" aria-label="Add an item rule">
	<div class="section">
		<span class="label">Categories — the coarse layer</span>
		<ExchangeCategoryPills {categories} {categoryRules} {oncategoryrule} compact />
	</div>

	<div class="search">
		<svg
			width="13"
			height="13"
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
		<!-- svelte-ignore a11y_autofocus -->
		<input
			type="text"
			placeholder="Search the items in these plays…"
			bind:value={query}
			autofocus
			aria-label="Search items"
		/>
		<span class="count">{results.length} of {items.length} items</span>
	</div>

	<div class="rows">
		{#each results as item (item.id)}
			{@const rule = ruleById.get(item.id)}
			{@const verdict = effectiveRule(rule, item.category, categoryRules)}
			<div class="row" class:row-only={verdict === 'only'} class:row-hide={verdict === 'hide'}>
				<span class="tile">
					<ItemIcon src={iconSrc(apiBase, item.icon)} alt="" size={22} />
				</span>
				<span class="lines">
					<!-- Artboard deviation, deliberate: the artboard leaves the name plain
					     on a row hidden by its category and strikes it only for an item
					     rule. Kept struck for both, because the strike is what says the
					     row is out — and the sub-line below already says which layer
					     did it. -->
					<span class="name" class:struck={verdict === 'hide'}>{item.name}</span>
					<span class="category" class:hidden-by-category={verdict === 'hide' && rule === undefined}>
						{item.category === '' ? 'uncategorised' : item.category}{verdict === 'hide' &&
						rule === undefined
							? ' — hidden by category'
							: ''}
					</span>
				</span>
				<span class="plays">{item.playCount} {item.playCount === 1 ? 'play' : 'plays'}</span>
				<div class="toggle">
					<button
						class="only"
						class:active={rule === 'only'}
						aria-pressed={rule === 'only'}
						aria-label="Only {item.name}"
						onclick={() => toggle(item, 'only')}
					>
						Only
					</button>
					<button
						class="hide"
						class:active={rule === 'hide'}
						aria-pressed={rule === 'hide'}
						aria-label="Hide {item.name}"
						onclick={() => toggle(item, 'hide')}
					>
						Hide
					</button>
				</div>
			</div>
		{:else}
			<div class="empty">
				{#if items.length === 0}
					No plays on screen yet, so there is nothing to rule on.
				{:else}
					No item in these plays matches “{query}”.
				{/if}
			</div>
		{/each}
	</div>

	<!-- Artboard deviation, deliberate: the artboard carries a fourth footnote
	     saying the rules survive a restart. Dropped here — the filter bar's
	     Categories/Items labels carry the Only/Hide tooltip that states it, and
	     a footnote about storage is the one line in this block that is not
	     about what the rules DO. -->
	<div class="footnotes">
		<span>
			<strong>Two layers.</strong> Categories are coarse — the buckets the in-game exchange lists
			down its sidebar. An item rule overrides whatever its category says.
		</span>
		<span>
			<strong>Only</strong> keeps plays that touch at least one match.
			<strong>Hide</strong> drops plays that touch any match. Hide wins on conflict.
		</span>
		<span>An item matches in either role: the thing traded, or the currency it is quoted in.</span>
	</div>
</div>

<style>
	.picker {
		position: absolute;
		top: calc(100% + 6px);
		left: 0;
		z-index: 20;
		width: 560px;
		max-width: 90vw;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		box-shadow: 0 12px 32px rgba(0, 0, 0, 0.55);
	}

	.section {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 9px 12px;
		border-bottom: 1px solid var(--color-lab-border);
	}

	.label {
		font-size: 0.625rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-secondary);
	}

	.search {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 9px 12px;
		border-bottom: 1px solid var(--color-lab-border);
		color: #6b7280;
	}

	.search input {
		flex: 1;
		min-width: 0;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		padding: 3px 8px;
		font-family: inherit;
		font-size: 0.8125rem;
		color: var(--color-lab-text);
	}

	.search input:focus {
		outline: none;
		border-color: var(--color-lab-blue);
	}

	.count {
		font-size: 0.6875rem;
		color: #6b7280;
		white-space: nowrap;
	}

	.rows {
		display: flex;
		flex-direction: column;
		/* Bounded by the plays on screen, but a hundred rows still needs a scroll
		   rather than a popover taller than the window. */
		max-height: 320px;
		overflow-y: auto;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 7px 12px;
		border-bottom: 1px solid var(--color-lab-border);
	}

	.row-only {
		background: rgba(59, 130, 246, 0.08);
	}

	.row-hide {
		background: rgba(239, 68, 68, 0.07);
	}

	.tile {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		flex-shrink: 0;
	}

	.lines {
		display: flex;
		flex-direction: column;
		line-height: 1.25;
		flex: 1;
		min-width: 0;
	}

	.name {
		font-size: 0.8125rem;
		color: var(--color-lab-text);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.name.struck {
		text-decoration: line-through;
		color: var(--color-lab-text-secondary);
	}

	.category {
		font-size: 0.625rem;
		color: #6b7280;
	}

	/* Says WHY a row the reader never touched is struck: the coarse layer did it,
	   and the Only button beside it is the override. */
	.category.hidden-by-category {
		color: color-mix(in oklab, var(--color-lab-red) 70%, var(--color-lab-text));
	}

	.plays {
		font-size: 0.6875rem;
		color: #6b7280;
		white-space: nowrap;
	}

	.toggle {
		display: flex;
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		overflow: hidden;
		flex-shrink: 0;
	}

	.toggle button {
		background: transparent;
		border: none;
		color: #6b7280;
		padding: 2px 10px;
		font-size: 0.6875rem;
		font-weight: 600;
		font-family: inherit;
		cursor: pointer;
	}

	.toggle button:hover {
		background: rgba(255, 255, 255, 0.05);
		color: var(--color-lab-text);
	}

	.toggle button:focus-visible {
		outline: 1px solid var(--color-lab-blue);
		outline-offset: -1px;
	}

	.toggle .only.active {
		background: rgba(59, 130, 246, 0.28);
		color: var(--color-lab-text);
	}

	.toggle .hide.active {
		background: rgba(239, 68, 68, 0.28);
		color: var(--color-lab-text);
	}

	.empty {
		padding: 16px 12px;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		text-align: center;
	}

	.footnotes {
		display: flex;
		flex-direction: column;
		gap: 5px;
		padding: 10px 12px;
		font-size: 0.6875rem;
		color: #6b7280;
		line-height: 1.5;
	}

	.footnotes strong {
		color: var(--color-lab-text-secondary);
	}
</style>
