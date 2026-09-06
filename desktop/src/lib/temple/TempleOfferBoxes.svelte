<script lang="ts">
	/**
	 * The two offer boxes, on the left margin of the temple sheet (POE-249),
	 * rebuilt as the visual explanation of the kill in POE-260.
	 *
	 * The owner's ask, verbatim: *"two offer boxes on the LEFT margin of the
	 * temple sheet, stacked to mirror the side panel's own block order, each
	 * with everything the decision needs: the room the kill builds and its tier,
	 * Vertolka's rating line, the advisor's first reason. The advisor's pick
	 * gets a cyan frame — that frame IS the pointer; no arrows anywhere."*
	 *
	 * What it replaces is the single kill callout (POE-244), which named ONE
	 * block and said one reason about it. The player is choosing BETWEEN two
	 * blocks, and the block the advisor did not take was not on the overlay at
	 * all — so the comparison happened in the game's own panel, in text, which
	 * is the reading the overlay exists to spare.
	 *
	 * # What POE-260 added, and why it is worth the height
	 *
	 * A grade letter and a reason say which block the advisor took. They do not
	 * say *what the room is worth*, and the number POE-257 computes is the whole
	 * of the ranking. So the box now leads with that number in chaos and then
	 * shows what makes it up: the items the room drops, as ICONS with prices,
	 * the quantity / rarity bonus, and the vial upgrade for the line's unique
	 * where there is one. Everything on screen comes from `OfferView.value` —
	 * the same `Valued` row the advisor ranked on (POE-257 D6) — so no box can
	 * justify a number the recommendation did not use.
	 *
	 * The honesty rules are the design's and are not negotiable in markup: an
	 * unpriced item keeps its icon and reads `no price`, a board with no market
	 * at all reads `—` and shows the grade instead of a chaos number, and
	 * nothing is ever drawn as `0c` to fill a gap. `view.ts` decides all of
	 * those words; this file only places them.
	 *
	 * # The column is the panel's own diagonal
	 *
	 * Since 2026-09-06 each box also sits on its block's SIDE of the panel's
	 * diamond: the game prints the first block top-right and the second
	 * bottom-left, so the top box is `STACK_STAGGER_CSS` right of the column and
	 * the lower one on it (owner's ask, redrawn on a screenshot). The side is
	 * read off the block rect in `offerStackPlacement`, never off the index.
	 *
	 * # The frame is the pointer
	 *
	 * The pick carries a 2 px cyan frame — the same cyan as the kill glyph on
	 * the room widget (POE-248), so the two surfaces mark the same architect in
	 * the same colour — at full opacity. The other box is a 1 px muted frame at
	 * reduced opacity: **faint is the alternative**, the rule the room widget's
	 * conditional door and unchosen glyph already follow, so everything at full
	 * strength is a thing to do now. There is no arrow and no line; POE-248
	 * retired those everywhere on this overlay.
	 *
	 * # The rule it must not break
	 *
	 * **It covers no read region.** Every OCR crop and sampled patch reaches the
	 * slice as `layout.rois`, and `offerStackPlacement` refuses any position
	 * that overlaps one — including the one it wanted. A box that cannot be
	 * placed clear is NOT drawn, and the other one still is: the boxes are
	 * placed one at a time and each is its own answer (ADR-019).
	 *
	 * # The measure-then-place frame
	 *
	 * The box's WIDTH is the registry's number applied as `width` on a
	 * `border-box` element, so it is fixed and known before anything renders.
	 * Its HEIGHT is not — how many driver rows a room has, and whether it has a
	 * recipe, are properties of the board — so the box is still rendered before
	 * it is placed and stays `visibility: hidden` until the placement answers.
	 * Hidden rather than `display: none`, because a box that is not laid out
	 * cannot be measured and the frame would never end. Same trick, and the same
	 * reason, as the config bar's in `WidgetHost.svelte`. The sizes are ARRAYS
	 * here, one entry per box, because the stack's arithmetic needs every box's
	 * height to stack the next one under it.
	 *
	 * That frame has to be RE-ENTERED on every new read, and neither the sizes
	 * nor the elements do it on their own. The `{#each}` was keyed on
	 * `offer.index` — 0 and 1 on every board — so a new read reused the same two
	 * elements while `widths`/`heights` still held the last board's numbers, and
	 * `bind:offsetWidth` only writes back after the DOM has updated: the first
	 * frame of every read was placed from the PREVIOUS board's sizes, non-null
	 * and therefore visible. So the arrays are cleared and the nodes are
	 * recreated together, both off the JOINED `signature` below — a pair rather
	 * than a belt and braces, with the same reach and the same identity: a
	 * primitive string, because an array rebuilt on every poll always compares
	 * unequal, and one string for every box, because the reset clears every
	 * box. See the comments there; that is where the reasoning lives.
	 */
	import ItemIcon from '$lib/components/ItemIcon.svelte';
	import { getGemIconUrl } from '$lib/gem-icons';
	import { offersCompact, offerStackPlacement } from './overlay-geometry';
	import { offerBoxSignature } from './view';
	import type { OfferBox, OfferDriver, OfferRecipeItem } from './view';
	import type { HostSize, WidgetRect } from '$lib/overlay/widgets/widget-geometry';

	let {
		boxes,
		blocks,
		panel,
		obstacles,
		host,
		width
	}: {
		/** What each box says, in the panel's own block order. */
		boxes: OfferBox[];
		/** Each offer's block rect in CSS px, `blocks[i]` for `boxes[i]`, or null
		 *  for a read that carried no boxes — that box is then stacked under the
		 *  one above it, or placed at the panel's top when it is the first. */
		blocks: (WidgetRect | null)[];
		/** The side panel's OCR crop in CSS px — the first box's fallback top. */
		panel: WidgetRect | null;
		/** The never-cover set, CSS px. */
		obstacles: readonly WidgetRect[];
		host: HostSize;
		/** The registry's shipped width, and since POE-260 the box's ACTUAL
		 *  width rather than a wrap ceiling — see the `box-sizing` note in the
		 *  style block for why that difference is load-bearing. */
		width: number;
	} = $props();

	let widths = $state<number[]>([]);
	let heights = $state<number[]>([]);

	/**
	 * Whether the pair draws in its compact form (POE-260 design §7).
	 *
	 * Both boxes or neither — two boxes in different forms read as two different
	 * kinds of answer, and the pair exists to be compared. The rule itself is in
	 * `overlay-geometry.ts` because it is arithmetic over the host and the block
	 * rects, and because it has to be testable without a DOM.
	 */
	const compact = $derived(
		offersCompact({
			driverCounts: boxes.map((box) => box.driverCount),
			blocks,
			panel,
			host
		})
	);

	/**
	 * What each box's measurement was taken FOR, in box order. Both the
	 * `{#each}` key and the reset below read it.
	 *
	 * `offerBoxSignature` is in `view.ts` and its rule — SHAPE, never content —
	 * is stated there, with the once-a-minute blink it exists to end. STRINGS
	 * and not the `boxes` array itself, because `boxes` is a `$derived` over the
	 * SSOT poll: its reference changes on every tick even when the read did not,
	 * so anything keyed on the reference would re-key and re-measure several
	 * times a second.
	 */
	const signatures = $derived(boxes.map((box) => offerBoxSignature(box, compact)));

	/**
	 * The same signatures as ONE STRING, and the identity of this value is the
	 * load-bearing part rather than its content.
	 *
	 * The effect below must fire when the read's shape changes and NOT
	 * otherwise. `signatures` cannot be that trigger: it is an array, a
	 * derived's value is compared with `===`, and it is rebuilt from `boxes` on
	 * every SSOT poll — `ssot.temple` is reassigned unconditionally every 3 s
	 * and on every `ssot-changed` nudge, so a fresh array arrives with identical
	 * contents and always reads as changed. An effect on it clears the sizes
	 * every tick, while the `{#each}` key — the same strings, compared BY VALUE
	 * — does not change and so does not recreate the nodes; the observer never
	 * re-reports and the boxes vanish one poll after they appear, for the life
	 * of the board. Joined to a primitive, an unchanged read is `===` and
	 * nothing fires.
	 */
	const signature = $derived(signatures.join(' '));

	// The two halves of one guard, and neither works alone. Clearing the sizes
	// is what keeps the stale frame off the screen: this runs before the browser
	// paints, so the boxes go back to `visibility: hidden` instead of being
	// drawn at the last board's positions. Re-keying the `{#each}` on the same
	// signatures is what guarantees they come BACK — `bind:offsetWidth` is a
	// `ResizeObserver`, which reports a size only when it CHANGES, and hiding a
	// box does not change its layout size, so a new read whose boxes happen to
	// measure the same as the old one's would never be reported and would stay
	// hidden. A newly observed element always gets one initial callback, so
	// recreating the node is what re-arms the measurement.
	//
	// So the two have to move together, and they have to have the SAME reach:
	// the reset clears every box's size, so every box's node has to be
	// recreated when it fires — the `{#each}` key is therefore the joined
	// primitive plus the box's index, not the box's own string. A per-box key
	// (the first cut, caught by the delivery audit) left a box whose own text
	// did not change with its old node, never re-observed, hidden for the rest
	// of the board — reachable on a retry merge that resolves one offer and
	// leaves the other's lines as they were. By value on both sides, so an
	// unchanged read keeps its nodes and does not clear. Give the effect the
	// array instead and the deadlock above comes back — cleared every tick,
	// never re-observed.
	$effect(() => {
		signature;
		widths = [];
		heights = [];
	});

	const placements = $derived(
		offerStackPlacement({
			blocks,
			panel,
			boxes: boxes.map((_, i) => ({ w: widths[i] ?? 0, h: heights[i] ?? 0 })),
			obstacles,
			host
		})
	);

	/** The icon endpoint's URL for one item, or null where there is no item.
	 *  `ItemIcon` renders nothing for a null `src`, which is what the sale row
	 *  wants — its glyph is drawn instead. */
	function iconSrc(name: string | null): string | null {
		return name === null ? null : getGemIconUrl(name);
	}

	/** The recipe's three members in the order they are consumed. */
	function recipeSteps(box: OfferBox): OfferRecipeItem[] {
		const recipe = box.recipe;
		return recipe === null ? [] : [recipe.base, recipe.vial, recipe.upgraded];
	}

	/** Whether this row's price cell is a gain rather than a cost — the sale
	 *  delta is the only one, and it is the only green number in the box. */
	function isGain(driver: OfferDriver): boolean {
		return driver.kind === 'sale' && driver.priced;
	}
</script>

{#snippet mark(letter: string)}
	<span class="mark" class:guess={letter === 'G' || letter === 'F'}>{letter}</span>
{/snippet}

{#snippet icon(name: string | null, kind: string, size: number)}
	<!-- The frame is an OUTLINE and not a border, so the art keeps its own box:
	     26 px is exactly one third of the 78 px source, and an integer downscale
	     is what stops the icons shimmering against the game's own inventory
	     art. The colours are the GAME's item-frame colours (unique brown, rare
	     yellow) rather than this app's palette, because they are what the player
	     is already reading two inches to the right. -->
	<span
		class="icon {kind}"
		style="width:{size}px;height:{size}px;"
		aria-hidden={name === null ? 'true' : undefined}
	>
		{#if name === null}
			<svg width={size} height={size} viewBox="0 0 24 24" fill="none">
				<circle cx="12" cy="12" r="4.6" stroke="currentColor" stroke-width="1.3" />
				<path
					d="M12 9.4v5.2M10.2 10.9h3.6"
					stroke="currentColor"
					stroke-width="1.2"
					stroke-linecap="round"
				/>
			</svg>
		{:else}
			<ItemIcon src={iconSrc(name)} alt={name} {size} />
		{/if}
	</span>
{/snippet}

{#snippet arrow()}
	<span class="arrow">
		<svg width="10" height="10" viewBox="0 0 12 12" fill="none" aria-hidden="true">
			<path
				d="M1.5 6h8M7 3.5 9.5 6 7 8.5"
				stroke="currentColor"
				stroke-width="1.3"
				stroke-linecap="round"
				stroke-linejoin="round"
			/>
		</svg>
	</span>
{/snippet}

{#each boxes as box, i (signature + "|" + box.offer.index)}
	<div
		class="box"
		class:pick={box.pick}
		class:compact
		style="left:{placements[i]?.x ?? 0}px;top:{placements[i]?.y ?? 0}px;width:{width}px;{placements[
			i
		]
			? ''
			: 'visibility:hidden;'}"
		bind:offsetWidth={widths[i]}
		bind:offsetHeight={heights[i]}
	>
		<p class="headline">
			{box.headline}{#if box.forced}<span class="forced">({box.forced})</span>{/if}
		</p>
		<p class="builds">{box.builds}</p>

		{#if box.valueText !== null}
			<!-- The headline number, and the one row whose height never moves:
			     the chip takes the same slot `per run` does, so a partial sum
			     cannot shift everything under it. -->
			<div class="value" class:ladder={box.value === null} class:stale={box.stale}>
				<span class="num">{box.valueText}</span>
				{#if box.value !== null}<span class="unit">c</span>{/if}
				{#each box.marks as letter (letter)}
					{@render mark(letter)}
				{/each}
				{#if box.chip}
					<span class="chip">{box.chip}</span>
				{:else if !compact && box.value !== null}
					<span class="per">per run</span>
				{/if}
			</div>
		{/if}

		{#if compact}
			<!-- Icons and prices, one row. What the eye uses at arm's length —
			     who, what it builds, what it is worth and what pays for it —
			     with the per-driver counts, the bonus, the recipe and the lead
			     reason dropped. -->
			{#if box.drivers.length > 0}
				<div class="strip">
					{#each box.drivers as driver (driver.name)}
						{@render icon(driver.iconName, driver.kind, 20)}
					{/each}
					{#if box.stripPrices}<span class="prices">{box.stripPrices}</span>{/if}
				</div>
			{/if}
			<div class="foot">{box.foot}</div>
		{:else}
			{#if box.scaleNote}
				<!-- Epic lock L2 in one line: the rows below are the TIER-3
				     room's terms and do not add up to the number above them. -->
				<p class="scale">{box.scaleNote}</p>
			{/if}

			{#if box.drivers.length > 0}
				<div class="drivers">
					{#each box.drivers as driver (driver.name)}
						<div class="row">
							{@render icon(driver.iconName, driver.kind, 26)}
							<span class="nm">{driver.name}</span>
							<span class="pr" class:none={!driver.priced} class:gain={isGain(driver)}>
								{driver.price}
							</span>
							<span class="ct">{driver.perRun ?? ''}</span>
							<span class="marks">
								{#each driver.marks as letter (letter)}
									{@render mark(letter)}
								{/each}
							</span>
						</div>
					{/each}
				</div>
			{/if}

			{#if box.fold}<div class="note">{box.fold}</div>{/if}

			{#if box.bonus}
				<div class="bonus">
					<span>{box.bonus.label}</span>
					{#if box.bonus.amount}<span class="amt">{box.bonus.amount}</span>{/if}
				</div>
			{/if}

			{#if box.note}<div class="note">{box.note}</div>{/if}

			{#if box.recipe}
				<div class="lab">upgrade recipe</div>
				<div class="rec">
					{#each recipeSteps(box) as step, step_i (step.name)}
						{#if step_i > 0}{@render arrow()}{/if}
						{@render icon(step.name, step_i === 1 ? 'vial' : 'unique', 20)}
						<span class="rp" class:none={step.price === '—'}>{step.price}</span>
					{/each}
				</div>
			{/if}

			{#if box.rating}
				<p class="rating">{box.rating}</p>
			{/if}
			{#if box.reason}
				<p class="reason">{box.reason}</p>
			{/if}
			<p class="age" class:warn={box.stale}>{box.market}</p>
		{/if}
	</div>
{/each}

<style>
	/* The alternative, and it is drawn faint rather than left out: the player is
	   choosing between two blocks and the one not taken is half the decision.

	   `box-sizing: border-box` is load-bearing rather than housekeeping: the
	   registry's `w` is applied as `width` here, and without it `offsetWidth` —
	   which is what `offerStackPlacement` measures the column from — would be
	   `w` plus the padding and the border, so the number that decides the
	   placement would not be the number on screen. */
	.box {
		position: absolute;
		box-sizing: border-box;
		display: flex;
		flex-direction: column;
		padding: 8px 10px;
		background: rgb(15 17 23 / 88%);
		border: 1px solid var(--color-lab-text-muted);
		border-radius: 6px;
		color: var(--color-lab-text);
		opacity: 0.75;
		pointer-events: none;
	}

	/* The advisor's pick. The frame IS the pointer (owner: no arrows anywhere),
	   in the same cyan as the room widget's kill glyph so the two surfaces mark
	   the same architect the same way. */
	.box.pick {
		border: 2px solid var(--color-lab-cyan);
		opacity: 1;
	}

	/* The one line the player is meant to SEE — which architect, and which kill.
	   Read at arm's length over a game, so bigger than anything the old advice
	   panel printed. */
	.headline {
		margin: 0;
		font-size: 16px;
		font-weight: 700;
		line-height: 20px;
	}

	.pick .headline {
		color: var(--color-lab-cyan);
	}

	/* Inside the headline, not under it: the point is that the kill was not a
	   ranked choice, and a note the eye reads as a separate line is one it
	   skips. */
	.forced {
		margin-left: 5px;
		font-size: 11px;
		font-weight: 400;
		color: var(--color-lab-yellow);
	}

	/* What the kill actually builds — the resolved room and its tier, never the
	   name the panel printed (`offerBuilds`). */
	.builds {
		margin: 0;
		font-size: 13px;
		line-height: 17px;
	}

	/* --------------------------------------------------- what it is worth -- */

	/* The number the whole ranking is, at the size the ranking deserves.
	   `tabular-nums` so two boxes' figures line up vertically — the comparison
	   is the point, and proportional digits make two three-figure numbers look
	   like different lengths. */
	.value {
		display: flex;
		align-items: baseline;
		gap: 3px;
		margin-top: 8px;
		height: 28px;
	}

	.value .num {
		font-size: 24px;
		font-weight: 700;
		line-height: 28px;
		font-variant-numeric: tabular-nums;
		letter-spacing: -0.02em;
	}

	/* The ladder's answer is a LETTER, not a number, and it is set smaller so
	   the two are not mistaken for each other at a glance. */
	.value.ladder .num {
		font-size: 20px;
	}

	.value .unit {
		font-size: 14px;
		font-weight: 600;
		line-height: 28px;
		color: var(--color-lab-text-secondary);
	}

	/* Marked on the value itself, because that is what a player would otherwise
	   act on. What the slot carries on a stale board is the GRADE and not a
	   chaos figure — Rust prices nothing off a stale snapshot, so the box is in
	   its fallback form (`OfferBox.state`) — and the underline says that the
	   letter standing there is standing in. */
	.value.stale .num {
		border-bottom: 1px dotted var(--color-lab-yellow);
	}

	.value .per {
		margin-left: auto;
		font-size: 10px;
		line-height: 28px;
		color: var(--color-lab-text-muted);
	}

	/* Occupies the same slot as `per run`, so a partial sum does not move a
	   single row under it. */
	.value .chip {
		margin-left: auto;
		align-self: center;
		padding: 0 5px;
		font-size: 10px;
		line-height: 14px;
		color: var(--color-lab-text-secondary);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
	}

	/* --------------------------------------------------------- the rows -- */

	.drivers {
		display: flex;
		flex-direction: column;
		gap: 6px;
		margin-top: 8px;
	}

	.row {
		display: grid;
		grid-template-columns: 26px minmax(0, 1fr) auto auto auto;
		align-items: center;
		gap: 8px;
		height: 26px;
	}

	.row .nm {
		font-size: 12px;
		line-height: 16px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.row .pr {
		font-size: 12px;
		font-weight: 600;
		line-height: 16px;
		font-variant-numeric: tabular-nums;
	}

	/* `no price` and `—` are refusals, not numbers, so they are set at the
	   weight of prose and in the muted colour — a bold `no price` reads as an
	   amount at a glance. */
	.row .pr.none {
		font-weight: 400;
		color: var(--color-lab-text-muted);
	}

	/* The only green number in the box: the sale delta is money the room pays
	   OUT, and everything else is what it drops. */
	.row .pr.gain {
		color: var(--color-lab-green);
	}

	.row .ct {
		font-size: 11px;
		color: var(--color-lab-text-muted);
		font-variant-numeric: tabular-nums;
	}

	.marks {
		display: flex;
		gap: 3px;
	}

	/* One shape, four letters, two colours — the colour says WHY and the letter
	   says WHICH, so nothing here is carried by hue alone (ADR-018's rule about
	   flags, and the same reason `values.ts` letters its table). Yellow: nobody
	   measured it. Muted: measured, but thin. */
	.mark {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 12px;
		height: 12px;
		border: 1px solid currentColor;
		border-radius: 2px;
		font-size: 9px;
		font-weight: 700;
		line-height: 1;
		color: var(--color-lab-text-muted);
	}

	.mark.guess {
		color: var(--color-lab-yellow);
	}

	/* --------------------------------------------------------- the icons -- */

	.icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 2px;
		/* An OUTLINE rather than a border so the art keeps its own box and the
		   26 px downscale stays an exact third of the 78 px source. */
		outline: 1px solid #4b4f5a;
	}

	/* The game's own item-frame colours, deliberately not from `tokens.css`:
	   these are the colours the player is reading in the panel two inches away,
	   and this app's palette has no opinion about item rarity. */
	.icon.unique {
		outline-color: #af6025;
	}

	.icon.vial {
		outline-color: var(--color-lab-purple);
	}

	.icon.mod {
		outline-color: #d6d36a;
	}

	.icon.sale {
		outline-color: var(--color-lab-green);
		color: var(--color-lab-green);
	}

	/* -------------------------------------------------- the other lines -- */

	.scale {
		margin: 6px 0 0;
		height: 15px;
		font-size: 11px;
		line-height: 15px;
		color: var(--color-lab-text-muted);
	}

	.bonus {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 6px;
		height: 15px;
		font-size: 11px;
		line-height: 15px;
		color: var(--color-lab-text-secondary);
	}

	.bonus .amt {
		margin-left: auto;
		color: var(--color-lab-text);
		font-variant-numeric: tabular-nums;
	}

	.note {
		margin-top: 6px;
		height: 15px;
		font-size: 11px;
		line-height: 15px;
		color: var(--color-lab-text-muted);
	}

	.lab {
		margin-top: 8px;
		height: 13px;
		font-size: 10px;
		font-weight: 700;
		line-height: 13px;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--color-lab-text-muted);
	}

	.rec {
		display: flex;
		align-items: center;
		gap: 5px;
		margin-top: 2px;
		height: 24px;
	}

	.rec .rp {
		font-size: 10px;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}

	.rec .rp.none {
		font-weight: 400;
		color: var(--color-lab-text-muted);
	}

	.arrow {
		display: flex;
		color: var(--color-lab-text-muted);
	}

	.rating {
		margin: 8px 0 0;
		font-size: 11px;
		line-height: 14px;
		color: var(--color-lab-text-secondary);
	}

	/* One line, ellipsised. The Temple page shows every reason; this box has
	   room for the first. */
	.reason {
		margin: 2px 0 0;
		font-size: 11px;
		line-height: 14px;
		color: var(--color-lab-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* Where the numbers above came from (POE-258). Faintest line in the box and
	   deliberately so: it qualifies the comparison rather than being part of
	   it, and it is always present, so anything louder would compete with the
	   headline on every board. */
	.age {
		margin: 6px 0 0;
		font-size: 10px;
		line-height: 13px;
		color: var(--color-lab-text-muted);
	}

	.age.warn {
		color: var(--color-lab-yellow);
	}

	/* ------------------------------------------------------- the compact -- */

	.box.compact .value {
		margin-top: 6px;
		height: 24px;
	}

	.box.compact .value .num {
		font-size: 22px;
		line-height: 24px;
	}

	.box.compact .value .unit {
		font-size: 13px;
		line-height: 24px;
	}

	.strip {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-top: 6px;
		height: 24px;
	}

	.strip .prices {
		margin-left: auto;
		font-size: 12px;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}

	/* The compact form's whole footer: who graded the line, and how old the
	   prices are. Two facts the full form gives two rows. */
	.foot {
		margin-top: 6px;
		font-size: 10px;
		line-height: 13px;
		color: var(--color-lab-text-muted);
	}
</style>
