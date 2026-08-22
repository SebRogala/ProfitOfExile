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
	 * Since POE-193 the amounts count ONE SIZE across the whole row and the ends
	 * are in the currency that size is entered with, so the end slots carry a unit
	 * word and a sub-line; comfortable shows both, dense shows neither and hands
	 * them back through the wrapper's `title`. That size is `displayScale` — one
	 * posting of the buy market on a chaos entry, the worthwhile run on a divine
	 * one (owner ruling, 2026-08-22). Steps 1 and 2 read as TOTALS carrying
	 * `≈` — "buy 50 for ≈ 3.16 div", "buy 4 for ≈ 1c" — rather than the
	 * whole-quantity ORDER they used to print, and rather than the per-unit decimal
	 * the in-game exchange has no field for. The market's own posted pair moved to
	 * the `title`: on a step that prints a total the hover is what that market
	 * printed plus what its lot does to a quantity of this size, and it is `null`
	 * only there — when such a leg arrives with no usable pair to word. A step that
	 * shows its market's LINE instead of a total always has a `title`, pair or no
	 * pair: the caveat that the line counts one order while the row's ends count
	 * the row's own size, worded for the pairless fallback too. Step 3 reads as
	 * that size's TOTAL — "≈ 2.97 div → 615c" — and only there has no name line,
	 * its icon naming the currency instead.
	 *
	 * SLOT GEOMETRY, mirrored by the table header in `CurrencyExchangePage`: the
	 * header's label spans must carry the same widths as `.slot-*` below
	 * (comfortable 120 / 208 / 176 / 176 / 168 with 22px arrows and a 7px gap;
	 * dense 80 / 220 / 140 / 140 / 80 with 18px arrows and a 6px gap), or the
	 * labels drift off the tiles they name. The two ends are NOT one width: only
	 * Get carries the profit line the comfortable geometry is sized around. The
	 * three step slots each grew 12px when the lines became `≈` run totals; the
	 * two ends did not move, because the shapes they print did not. Dense did not
	 * move at all: a rate too long for its dense slot ellipsizes there instead,
	 * the name beside it yielding its width first. The measured arithmetic for both
	 * densities is in the CSS, above `.slot-buy` and above `.route.dense
	 * .slot-buy`.
	 *
	 * COLLAPSED VARIANT. `showConvert` false drops the convert slot AND the arrow
	 * that led into it, leaving four slots and three arrows — comfortable
	 * 120 / 208 / 176 / 168, dense 80 / 220 / 140 / 80, at the same arrow and gap
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

{#snippet tile(icon: string | null, suspect: boolean, lowLiquidity: boolean = false)}
	<span class="tile" class:suspect class:low-liquidity={lowLiquidity}>
		<ItemIcon src={iconSrc(apiBase, icon)} alt="" size={iconSize} />
	</span>
{/snippet}

<!-- `lowLiquidity` is the PLAY's flag (POE-196), not the leg's — it is passed
     in by the caller rather than read off `slot`, and only the buy and sell
     calls below pass it. The convert step never does: the flag is about the
     traded item's own two legs, not the currency it happened to route
     through. -->
{#snippet step(slot: RouteStep, lowLiquidity: boolean = false)}
	{@render tile(slot.icon, slot.suspect, lowLiquidity)}
	<span class="lines">
		<!-- A step with no name is the convert step showing a total, whose line
		     names both currencies already and whose tile carries the artwork of the
		     one being converted; the name is on its `rateTitle`. -->
		{#if slot.name}
			<span class="name" title={slot.name}>{slot.name}</span>
		{/if}
		<!-- `rateTitle` carries what the line itself no longer claims: on a step
		     that printed a total it is the market's own posted pair plus what
		     that market's lot does to a quantity of this size, and it is `null` only
		     when such a leg arrived with no usable pair to word; on a step showing
		     its market's LINE instead it is always set, saying that the line counts
		     one market's lot — or, where no pair was posted, its bare per-unit
		     rate — while the row's ends count the row's own size. See the header
		     comment.
		     The rate is its own fallback title, but ONLY on that pairless leg: on
		     every other step the hover prints the market's posted pair, never the
		     total the line printed. So a total line cut short by its slot is
		     not recoverable on hover — there is nowhere the printed total is
		     written down a second time — which is why the cut has to be MARKED
		     rather than silent. That is what `.route.dense .name` in the CSS
		     below buys. -->
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

		<span class="slot slot-buy">{@render step(route.buy, !!play.lowLiquidity)}</span>

		{@render arrow(false)}

		<span class="slot slot-sell">{@render step(route.sell, !!play.lowLiquidity)}</span>

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
		   slot being narrower than a long three-currency order. What the clip takes
		   is not always a hover away: a step's `title` is its market's posted pair,
		   not the total the line printed. So the dense step lines ellipsise
		   rather than being sliced, and the mark is the reader's only warning —
		   see `.route.dense .name`. */
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
	/* THE STEP SLOTS, and the model they are sized on — which is NOT the ends'.
	   `.lines` is a COLUMN here, so a step slot holds the item NAME over the
	   RATE and is bound by the WIDER of the two. Both bases are MEASURED, in
	   Chrome, against the two fonts the app actually renders in: the name is
	   0.8125rem Segoe UI at 6.40px a character on "Omen of Amelioration", and
	   the rate is 0.625rem Consolas at 5.498px a character — a shade wider than
	   the ~5.25px the end slots above were sized on, because those measure a
	   PROPORTIONAL sub-line and this is mono. Text budget is the width less the
	   30px tile and the 7px gap.

	   On an ordinary row the name binds and the rate has room to spare: "Omen of
	   Amelioration" alone wants 128px against `buy 12 for ≈ 424c`'s 93.5px. The
	   rate binds only where a long quantity and a fractional divine total meet,
	   which is what the worst cases below are. So the widths chosen below are
	   RATE-derived while the constraint that binds a realistic row is the item
	   NAME — "Ambush Scarab of Discernment" measures 180.3px against buy's 171px
	   text budget and ellipsizes, recovered by its own `title` exactly as it was
	   before. That is deliberate rather than an oversight: the name has no upper
	   bound the geometry could be sized to, so the widths are derived from the
	   shape that does have one.

	   Every step line gained `≈ ` — 2 characters, ~11px — when it became a run
	   total, and the convert line's right amount moved from the Get to the CHAIN
	   END, which is larger by the ROI column less the Exp. ROI column and can
	   therefore carry an extra digit. Sized against those, at a text budget of
	   171px for buy and 139px for sell/convert:

	   - buy: `buy 12,345 for ≈ 1,234.56 div`, 29 chars → 159.5px. Fits, with
	     11.5px — two characters — of slack. The old 196px offered 159px, which
	     did not fit it at all;
	   - sell: `sell 12,345 for ≈ 1,234.56 div`, 30 chars → 165px. Still clips,
	     as it did before this change (the 28-char pre-`≈` form wanted 154px in
	     127px). The whole string stays reachable through the rate's own `title`;
	   - convert: `≈ 1,234.56 div → 181,338c`, 25 chars → 137.5px, which 139px
	     holds by a hair where 127px clipped it. The digit-longer chain end,
	     `≈ 1,234.56 div → 1,181,338c` at 148.5px, still clips by 9.5px — a
	     million-chaos chain end is bought back by the hover rather than by
	     another 12px on the widest cell in the table;
	   - flip counts have no bound: `ceil(100 / expectedRoi)` reaches ×10,000 on
	     a 0.01c expectation. The six characters of `12,345` above already cover
	     a five-digit count, so nothing further is owed;
	   - the two `pairLine` forms — `buy 16 for 1 div`, `convert @ 0.00 c`, ~16
	     characters, ~88px — are inside every budget and never bind. */
	.slot-buy {
		width: 208px;
	}
	.slot-sell,
	.slot-convert {
		width: 176px;
	}

	/* Dense drops the unit word as well as the sub-line, so both ends hold a bare
	   number and take one width again: 20px tile, 5px gap, and ~55px of 0.75rem
	   mono — enough for the eight characters of a grouped `1,010.00`, which is
	   more than the old 96px left once the unit word had taken its share. */
	.route.dense .slot-spend,
	.route.dense .slot-get {
		width: 80px;
	}
	/* Dense step slots did NOT move when the lines gained their `≈`, and the
	   trade that decision makes is measured rather than assumed. `.lines` is a
	   ROW here and the name is zero-base flex (see `.route.dense .name` below),
	   so the name yields its width first and the rate takes what is left. Text
	   budget is the width less the 20px tile and the 5px gap: 195px on buy,
	   115px on sell/convert.

	   Buy absorbs it — 195px holds ~35 characters. Sell and convert do not, and
	   not only at the arithmetic extremes: `sell 12 for ≈ 2.97 div` is 121px,
	   which is over the 115px budget BEFORE the name is given anything, so the
	   name collapses to nothing and the rate is still 11px over the 110px that
	   collapse leaves it (the 5px inter-line gap is charged even at a zero-width
	   name). The same string fitted before this row printed run totals (`sell 12
	   for 3 div`, 93.5px), so the overrun is new, and it lands on an ordinary
	   chaos-entry 1-hop rather than on a contrived one.

	   The widths are kept anyway, because no width removes the class of failure:
	   the rate is unbounded (`ceil(100 / expectedRoi)` reaches five digits), so a
	   wider dense slot only moves which strings overrun, and dense exists to buy
	   width back. What the width cannot fix, the SIGNAL does — the overrun is
	   ellipsised inside the rate rather than sliced off by the slot's `overflow:
	   hidden`, so the line reads `sell 12 for ≈ 2.97 …` and not `sell 12 for ≈
	   2.97 d`: still short by its unit word, but visibly short. That mark is
	   load-bearing here in a way it is not on the ends. The ends hand their whole
	   string back through the wrapper's `title`; a step's hover is its market's
	   posted PAIR, never the total the line printed, so a truncated total is
	   not recoverable by hovering it and the ellipsis is the only warning the
	   reader gets. */
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

	/* The play's newest hour printed no exploitable spread (POE-196). A play-level
	   reading, but drawn on the traded item's own tiles — buy and sell — rather
	   than a ring around the whole row: below some rank nearly every play was
	   flagged, and a row-wide ring turned solid red frames. `:not(.suspect)`
	   is the precedence rule, not a cascade accident: a tile that is BOTH
	   suspect and low-liquidity shows GOLD, because the per-leg warning is the
	   more specific reading and wins on that tile. */
	.tile.low-liquidity:not(.suspect) {
		border-color: var(--color-lab-red);
	}

	.route.dense .tile.low-liquidity:not(.suspect) {
		border-color: var(--color-lab-red);
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

	/* Which of the two dense lines gives way. `.lines` is a ROW in dense, holding
	   the item name beside the rate, and this rule makes the name ZERO-BASE flex:
	   `flex: 1 1 0` gives it a flex base size of 0, so it claims no width of its
	   own and merely grows into whatever the rate leaves.

	   Wherever the RATE fits, this changes nothing measurable — the name still
	   lands on exactly the width it landed on before, by the opposite route. It
	   used to shrink DOWN to the leftover from an `auto` basis, because the rate
	   was `flex-shrink: 0` and would not give any of its own width up; it now
	   grows UP to that same leftover from a basis of 0. Measured against the
	   shipped stylesheet, `sell 50 for ≈ 693c` beside `Chaos Orb` renders name
	   11.031px / rate 98.969px before AND after, and so do the two buy rows
	   (74.531 / 115.469 and 96.531 / 93.469). Pixel-identical, not merely close.

	   On a row that OVERRUNS it changes the failure. Flex shrink is scaled by flex
	   base size, and the name's base is 0, so the name absorbs none of the
	   shortfall and the rate absorbs all of it — by SHRINKING, which with the
	   `text-overflow: ellipsis` above means the rate ellipsizes at its own edge.
	   Previously the rate was `flex-shrink: 0`, kept its full width, and the
	   surplus was sliced off by the slot's `overflow: hidden` — an unmarked cut
	   mid-number. The convert worst case is the clearest reading: `≈ 1,234.56 div
	   → 181,338c` wants 137.5px against a 115px budget, and where it used to leave
	   22px hanging outside the slot it now renders inside it, ellipsised, at slot
	   overflow 0.

	   Zero base is doing real work here and a large `flex-shrink` on the name is
	   NOT the same fix. A name with a non-zero base still leaves the rate a share
	   of any subpixel shortfall: at `flex-shrink: 1000` the FITTING row above
	   hands the name 11.11px instead of 11.03px and takes that ~0.08px off the
	   rate — enough to trip the ellipsis and render `sell 50 for ≈ 69…` on a line
	   that fits. A zero base has no share to give, so fitting rows stay pixel-
	   identical. The rate needs nothing added: `overflow: hidden` above already
	   zeroes its automatic minimum size, so it can shrink the whole way. */
	.route.dense .name {
		flex: 1 1 0;
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
