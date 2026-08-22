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
	 * about the market and computes no amount, the end slots' hover included.
	 * Since POE-193 the amounts are the whole worthwhile RUN and the ends are in
	 * the currency the run is entered with, so the end slots carry a unit word and
	 * a sub-line; comfortable shows both, dense shows neither and hands them back
	 * through the wrapper's `title`. Steps 1 and 2 read as whole-quantity ORDERS —
	 * "buy 12 for 420c" — rather than the per-unit decimal the in-game exchange has
	 * no field for, and carry a `title` whenever the printed quantity is not the
	 * one the step was asked for (a market posting in lots that quantity does not
	 * divide by). Step 3 reads as the run's TOTAL — "≈ 2.52 div → 526c" — and has
	 * no name line, its icon naming the currency instead.
	 *
	 * SLOT GEOMETRY, mirrored by the table header in `CurrencyExchangePage`: the
	 * header's label spans must carry the same widths as `.slot-*` below
	 * (comfortable 120 / 196 / 164 / 164 / 168 with 22px arrows and a 7px gap;
	 * dense 80 / 220 / 140 / 140 / 80 with 18px arrows and a 6px gap), or the
	 * labels drift off the tiles they name. The two ends are NOT one width: only
	 * Get carries the profit line the comfortable geometry is sized around.
	 *
	 * COLLAPSED VARIANT. `showConvert` false drops the convert slot AND the arrow
	 * that led into it, leaving four slots and three arrows — comfortable
	 * 120 / 196 / 164 / 168, dense 80 / 220 / 140 / 80, at the same arrow and gap
	 * widths. No remaining slot changes size, so the contract with the header is
	 * one `{#if}` on each side rather than a second set of numbers. The arrow
	 * between sell and Get is a NORMAL arrow there, not the muted one: nothing is
	 * being skipped in the collapsed form, the step is simply not part of any
	 * route on screen. The call is the page's, taken over the whole rendered set
	 * (`anyConvertStep`) — this file never asks it per row, because a row that
	 * collapsed on its own would put its Get under the next row's sell, which is
	 * the one thing the fixed geometry exists to prevent.
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
		divineChaosRate,
		showConvert
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
		/**
		 * Whether the convert slot is drawn at all — the page's `anyConvertStep`
		 * over the RENDERED set, so every row on screen collapses together or none
		 * does. False on a table showing direct plays only, where the slot would be
		 * an empty dashed tile on every row. See the geometry contract above.
		 */
		showConvert: boolean;
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
		<!-- A step with no name is the convert step showing a run total, whose line
		     names both currencies already and whose tile carries the artwork of the
		     one being converted; the name is on its `rateTitle`. -->
		{#if slot.name}
			<span class="name" title={slot.name}>{slot.name}</span>
		{/if}
		<!-- `rateTitle` is set only when the printed order is not the quantity the
		     step was asked for, because the market posts in lots that quantity does
		     not divide by — the one case where the numbers on screen and the
		     amounts at the ends deliberately disagree, and the reader is owed the
		     reason. The rate itself is the fallback title rather than nothing: the
		     slot ellipsizes, and the tail it drops is the unit word, so a hover
		     that recovers the whole string is the only way back to it. -->
		<span class="rate mono" title={slot.rateTitle ?? slot.rate}>{slot.rate}</span>
	</span>
{/snippet}

{#snippet end(slot: RouteEnd, gain: boolean)}
	{@render tile(slot.icon, false)}
	<!-- The unit word and the sub-line ride on the wrapper's `title` as well as
	     their own spans, because dense hides both — and a bare 3.66 with no way to
	     learn it is divine, or a run with no way to reach its profit line, is not
	     a row a dense reader can act on. `routeSlots` composes the string. -->
	<span class="lines" title={slot.title}>
		<span class="amount mono" class:gain>{slot.amount}</span>
		<span class="unit">{slot.unit}</span>
		{#if slot.sub}
			<span class="sub">{slot.sub}</span>
		{/if}
	</span>
{/snippet}

{#if route}
	<div class="route" class:dense>
		<span class="slot slot-spend">{@render end(route.spend, false)}</span>

		{@render arrow(false)}

		<span class="slot slot-buy">{@render step(route.buy)}</span>

		{@render arrow(false)}

		<span class="slot slot-sell">{@render step(route.sell)}</span>

		<!-- The slot is here for the rows that USE it. When no play in the rendered
		     set converts, the page collapses the column for all of them and the
		     arrow into it goes with the slot — see the geometry contract above. -->
		{#if showConvert}
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
		{/if}

		{@render arrow(false)}

		<span class="slot slot-get">
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
		/* The fixed widths below are a contract with the page header, so a string
		   too long for its slot has to be clipped rather than allowed to push the
		   next tile out of its column — dense is where it bites, its 140px convert
		   slot being narrower than a long three-currency order. The rate carries
		   its own `title` in every case, so what the clip takes is a hover away. */
		overflow: hidden;
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
	.slot-get {
		width: 168px;
	}
	/* The two ends are NOT the same width, though they hold the same kind of
	   thing. Only Get carries the profit line above; the widest Spend can print is
	   its amount over a bare `≈ 181,338c` chaos reading — ~57px of mono amount and
	   ~54px of sub-line — so the text column needs ~83px and the slot that plus
	   the tile and its gap. Held at the width of its own content rather than
	   Get's: the route is the widest cell in a table that already scrolls
	   sideways, and 48px of blank tile on every row is 48px the reader pans past
	   to reach the money columns. */
	.slot-spend {
		width: 120px;
	}
	.slot-buy {
		width: 196px;
	}
	.slot-sell,
	.slot-convert {
		width: 164px;
	}

	/* Dense drops the unit word as well as the sub-line, so both ends hold a bare
	   number and take one width again: 20px tile, 5px gap, and ~55px of 0.75rem
	   mono — enough for the eight characters of a grouped `1,010.00`, which is
	   more than the old 96px left once the unit word had taken its share. */
	.route.dense .slot-spend,
	.route.dense .slot-get {
		width: 80px;
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

	/* The unit word goes the way of the sub-lines in dense, reversing the call
	   POE-193 made when it kept it. The fact behind that call still holds — the
	   end amounts are in the currency the run is ENTERED with, so a bare 0.51 is
	   divine on one row where 5,050 is chaos on the next — but the word is not the
	   only thing that says so: the end tile beside it carries that currency's own
	   artwork, which a dense reader scanning a column of rows reads faster than a
	   six-letter word repeated down it. What the word costs is width on the widest
	   cell in the table, twice per row, and dense exists to buy exactly that back.
	   The trade is the same one every other sub-line makes here, and it is paid
	   for the same way: the word is on the slot's `title`, a hover from the number
	   it belongs to. */
	.route.dense .unit {
		display: none;
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
