<script lang="ts">
	/**
	 * The Currency Exchange coarse layer: one tri-state pill per sidebar category.
	 *
	 * Neutral → only → hide → neutral, per `cycleCategoryRule`. The three states
	 * are told apart by the dot's colour, the pill's fill and — for hide — a
	 * struck name, so the state survives being read without colour.
	 *
	 * Shared by the filter bar and the Add popover, which is why the two sizes
	 * live here as one `compact` flag: the pills are the same control saying the
	 * same thing in two places, and a second copy of the markup would let the
	 * popover's row drift from the bar's.
	 *
	 * Presentation only. It holds no state — the pill's state is a prop and the
	 * click leaves through `oncategoryrule`, so the page owns what is persisted
	 * (ADR-013).
	 */
	import { cycleCategoryRule } from '$lib/exchange/filters';
	import type { CategoryRuleState, CategoryRules } from '$lib/exchange/filters';

	let {
		categories,
		categoryRules,
		oncategoryrule,
		compact = false
	}: {
		/** The sidebar taxonomy, straight off the response — never a local copy. */
		categories: string[];
		categoryRules: CategoryRules;
		/** The pill's NEXT state, per `cycleCategoryRule`; `undefined` is neutral. */
		oncategoryrule: (category: string, state: CategoryRuleState | undefined) => void;
		/** The popover's tighter size: smaller padding and type, same markup. */
		compact?: boolean;
	} = $props();

	/**
	 * The pill's accessible name carries its state.
	 *
	 * `aria-pressed` is a two-state contract and this control has three, so the
	 * state is spelled into the name instead — "Currency — hide" reads the same
	 * fact the struck name and the red dot show.
	 */
	function pillLabel(category: string, state: CategoryRuleState | undefined): string {
		return `${category} — ${state ?? 'neutral'}`;
	}
</script>

<div class="pills" class:compact>
	{#each categories as category (category)}
		{@const state = categoryRules[category]}
		<button
			class="pill"
			class:only={state === 'only'}
			class:hide={state === 'hide'}
			title="Click to cycle: neutral → only → hide."
			aria-label={pillLabel(category, state)}
			onclick={() => oncategoryrule(category, cycleCategoryRule(state))}
		>
			<span class="dot"></span>
			<span class="pill-name">{category}</span>
		</button>
	{/each}
</div>

<style>
	.pills {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.pill {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		background: transparent;
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		padding: 2px 8px;
		font-size: 0.75rem;
		font-family: inherit;
		color: var(--color-lab-text-secondary);
		white-space: nowrap;
		cursor: pointer;
	}

	/* The popover's size: padding and type only — every colour, state and glyph
	   below is shared, which is the point of the one component. */
	.pills.compact .pill {
		padding: 2px 7px;
		font-size: 0.6875rem;
	}

	.pill:hover {
		background: rgba(255, 255, 255, 0.05);
	}

	.pill:focus-visible {
		outline: 1px solid var(--color-lab-blue);
		outline-offset: 1px;
	}

	.dot {
		display: inline-block;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		/* Artboard one-off, no token: the neutral dot's grey. */
		background: #3f434f;
	}

	.pill.only {
		background: rgba(59, 130, 246, 0.12);
		border-color: rgba(59, 130, 246, 0.45);
		color: var(--color-lab-text);
	}

	.pill.only .dot {
		background: var(--color-lab-blue);
	}

	.pill.hide {
		background: rgba(239, 68, 68, 0.1);
		border-color: rgba(239, 68, 68, 0.4);
	}

	.pill.hide .dot {
		background: var(--color-lab-red);
	}

	.pill.hide .pill-name {
		text-decoration: line-through;
	}
</style>
