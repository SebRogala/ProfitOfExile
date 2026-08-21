<script lang="ts">
	/**
	 * One Currency Exchange play as the row's route cell: five fixed-width slots
	 * — Spend, buy, sell, convert, Get — with an arrow between each pair.
	 *
	 * Fixed width on every row is the whole point. A direct play has no convert
	 * step, and letting the later slots slide left would put "what you get" under
	 * the previous row's "what you sell": the slot stays, drawn as an empty
	 * dashed tile, so a column always holds the same kind of step and the eye can
	 * run down it.
	 *
	 * Presentation only. Every string, icon path and flag comes from
	 * `routeSlots`, which is pure and unit-tested — this file decides nothing
	 * about the market and computes no amount. Since POE-193 the amounts are the
	 * whole worthwhile RUN and the ends are in the currency the run is entered
	 * with, so the end slots carry a unit word and a sub-line that comfortable
	 * shows and dense drops.
	 *
	 * SLOT GEOMETRY, mirrored by the table header in `CurrencyExchangePage`: the
	 * header's label spans must carry the same widths as `.slot-*` below
	 * (comfortable 168 / 196 / 164 / 164 / 168 with 22px arrows and a 7px gap;
	 * dense 96 / 220 / 140 / 140 / 96 with 18px arrows and a 6px gap), or the
	 * labels drift off the tiles they name.
	 */
	import type { CurrencyExchangePlay } from '$lib/api';
	import {
		iconSrc,
		routeSlots,
		type ExchangeDensity,
		type RouteEnd,
		type RouteStep
	} from '$lib/exchange/view';
	import ItemIcon from './ItemIcon.svelte';

	let {
		play,
		density,
		apiBase,
		divineChaosRate
	}: {
		play: CurrencyExchangePlay;
		density: ExchangeDensity;
		/** `getApiBase()` — the origin the legs' relative icon paths hang off. */
		apiBase: string;
		/**
		 * The response's newest-hour chaos value of one divine — what a run entered
		 * in divine is read back in chaos with. 0 when that hour carried no
		 * divine/chaos trade, which drops the chaos sub-lines rather than printing
		 * a zero.
		 */
		divineChaosRate: number;
	} = $props();

	const route = $derived(routeSlots(play, divineChaosRate));
	const dense = $derived(density === 'dense');
	/** 24px inside a 30px tile when comfortable; a bare 20px glyph when dense. */
	const iconSize = $derived(dense ? 20 : 24);
</script>

{#snippet arrow(muted: boolean)}
	<svg
		class="arrow"
		class:muted
		width={dense ? 18 : 22}
		height="8"
		viewBox="0 0 22 8"
		fill="none"
		stroke="currentColor"
		stroke-width="1.4"
		stroke-linecap="round"
		stroke-linejoin="round"
		aria-hidden="true"
	>
		<path d="M0 4 H19 M15.5 1 L19 4 L15.5 7" />
	</svg>
{/snippet}

{#snippet tile(icon: string | null, suspect: boolean)}
	<span class="tile" class:suspect>
		<ItemIcon src={iconSrc(apiBase, icon)} alt="" size={iconSize} />
	</span>
{/snippet}

{#snippet step(slot: RouteStep)}
	{@render tile(slot.icon, slot.suspect)}
	<span class="lines">
		<span class="name" title={slot.name}>{slot.name}</span>
		<span class="rate mono">{slot.rate}</span>
	</span>
{/snippet}

{#snippet end(slot: RouteEnd, gain: boolean)}
	{@render tile(slot.icon, false)}
	<!-- The sub-line rides on the wrapper's `title` as well as its own span,
	     because dense hides the span and the profit line is the one thing on the
	     row a dense reader still has to be able to reach. -->
	<span class="lines" title={slot.sub ?? undefined}>
		<span class="amount mono" class:gain>{slot.amount}</span>
		<span class="unit">{slot.unit}</span>
		{#if slot.sub}
			<span class="sub">{slot.sub}</span>
		{/if}
	</span>
{/snippet}

{#if route}
	<div class="route" class:dense>
		<span class="slot slot-end">{@render end(route.spend, false)}</span>

		{@render arrow(false)}

		<span class="slot slot-buy">{@render step(route.buy)}</span>

		{@render arrow(false)}

		<span class="slot slot-sell">{@render step(route.sell)}</span>

		{@render arrow(route.convert === null)}

		<span class="slot slot-convert">
			{#if route.convert}
				{@render step(route.convert)}
			{:else}
				<!-- The step a direct play does not take, held open so the Get slot
				     never moves under the sell column of the row above. -->
				<span class="tile empty"></span>
				<span class="not-used">not used</span>
			{/if}
		</span>

		{@render arrow(false)}

		<span class="slot slot-end">
			{@render end(route.get, route.positive)}
		</span>
	</div>
{/if}

<style>
	.route {
		display: flex;
		align-items: center;
		gap: 7px;
	}

	.route.dense {
		gap: 6px;
	}

	.slot {
		display: flex;
		align-items: center;
		gap: 7px;
		flex-shrink: 0;
		min-width: 0;
	}

	.route.dense .slot {
		gap: 5px;
	}

	/* The fixed geometry. Mirrored by the page's header row — see the file
	   comment before changing a number here. */
	/* Sized off the longest string the row can hold rather than off the tiles:
	   `keep ≈ 102c (≈ 0.51 div)` is 24 characters at 0.625rem, so the text column
	   needs ~126px and the slot needs that plus the 30px tile and the 7px gap.
	   The tail in parentheses is the whole point of the line — it is what tells a
	   divine-entry reader what they keep in the currency they are holding — so it
	   is not a candidate for the ellipsis, and a table that already scrolls
	   sideways by design can afford the width. */
	.slot-end {
		width: 168px;
	}
	.slot-buy {
		width: 196px;
	}
	.slot-sell,
	.slot-convert {
		width: 164px;
	}

	.route.dense .slot-end {
		width: 96px;
	}
	.route.dense .slot-buy {
		width: 220px;
	}
	.route.dense .slot-sell,
	.route.dense .slot-convert {
		width: 140px;
	}

	.arrow {
		flex-shrink: 0;
		color: #4b5563;
	}

	/* The arrow into a step the play does not take is drawn down to the empty
	   tile's weight, so the row does not read as a broken three-step route. */
	.arrow.muted {
		color: var(--color-lab-border);
	}

	.tile {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		flex-shrink: 0;
	}

	/* Dense drops the frame and keeps the glyph: at 20px the border is most of
	   what the eye sees, and the row is half the height. */
	.route.dense .tile {
		width: 20px;
		height: 20px;
		background: transparent;
		border-color: transparent;
	}

	/* The leg's price sits outside its fair band (POE-188). The mark is on the
	   tile of the step it belongs to, not on the row, so a reader can see WHICH
	   half of the round trip is the doubtful one. */
	.tile.suspect {
		border-color: var(--color-lab-yellow);
	}

	/* Restated at dense's specificity: the rule that strips the frame above would
	   otherwise take the mark with it, and dense is where a reader most needs to
	   know which leg is doubtful. */
	.route.dense .tile.suspect {
		border-color: var(--color-lab-yellow);
	}

	.tile.empty {
		background: transparent;
		border-style: dashed;
		/* Artboard one-off, no token: a dash fainter than --color-lab-border. */
		border-color: #23262f;
	}

	.lines {
		display: flex;
		flex-direction: column;
		line-height: 1.25;
		min-width: 0;
	}

	/* One line, and the sub-line's content moves into the cell tooltips. */
	.route.dense .lines {
		flex-direction: row;
		align-items: baseline;
		gap: 5px;
	}

	.name,
	.amount,
	.rate,
	.unit,
	.sub {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.name,
	.amount {
		font-size: 0.8125rem;
		color: var(--color-lab-text);
	}

	.route.dense .name,
	.route.dense .amount {
		font-size: 0.75rem;
	}

	.amount.gain {
		color: var(--color-lab-green);
	}

	.rate,
	.unit,
	.sub {
		font-size: 0.625rem;
		color: #6b7280;
	}

	.route.dense .rate {
		flex-shrink: 0;
	}

	/* The unit label survives into dense, unlike every other sub-line: since
	   POE-193 the end amounts are in the currency the run is ENTERED with, so a
	   bare 0.51 is 0.51 divine on one row and 5,050 is chaos on the next. Dense
	   lays `.lines` out as a row, so it costs a word beside the number rather
	   than a second line. */
	.route.dense .unit {
		flex-shrink: 0;
	}

	/* The chaos reading of a divine spend, and the run's profit line. Dense drops
	   both, the way it drops every other sub-line; the string stays on the
	   wrapper's `title`, so it is a hover away. */
	.route.dense .sub {
		display: none;
	}

	.not-used {
		font-size: 0.6875rem;
		/* Artboard one-off, no token: the "not used" label, dimmer than any text
		   token, so the empty slot reads as absence rather than as content. */
		color: #2f333d;
		white-space: nowrap;
	}

	.route.dense .tile.empty {
		display: none;
	}

	.mono {
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}
</style>
