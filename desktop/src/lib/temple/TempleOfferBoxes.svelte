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
	 * POE-277 (2026-09-09) replaced the rating and reason lines with the
	 * area-bonus ladder and the temple-mod block, led the header with the room,
	 * and dropped the unique/vial names from the rows.
	 *
	 * What it replaces is the single kill callout (POE-244), which named ONE
	 * block and said one reason about it. The player is choosing BETWEEN two
	 * blocks, and the block the advisor did not take was not on the overlay at
	 * all — so the comparison happened in the game's own panel, in text, which
	 * is the reading the overlay exists to spare.
	 *
	 * # v5 (2026-09-10) — the owner's verdict on the shipped build
	 *
	 * Verbatim: *"it's crampled, weird padding, weird sizing and not sure what
	 * else... and the background - i'd make it less transparent. icons are
	 * defenetely NOT intuitive..."* Four rounds with the owner and Vertolka
	 * settled a restyle that also changed content. What moved, and why:
	 *
	 * - **Air.** `padding: 2px 10px` gave a 269 px box two pixels of vertical
	 *   air and every block butted against the next. It is `10px 12px` now,
	 *   and the box is FIVE SECTIONS — header, items, room bonuses, temple mod,
	 *   market warning — with a 1 px hairline between each pair that is
	 *   present, never one over nothing. Every margin is drawn from
	 *   `{3, 5, 6}` and stated per row.
	 * - **Ramp.** Seven font sizes with no relationship became six: 22 (the
	 *   value), 17 (the grade letter), 15 (the room name and the ladder rungs),
	 *   13 (prices, sale gain, mod name), 11 (kind, sale name, fold, the ladder's
	 *   stat word, the `+`/`→` connectors, market line), 10 (the three section
	 *   labels, the chips and the `upgraded` caption). The 15 px yellow ladder
	 *   used to out-shout the 16 px room name; it is the room name that is 15
	 *   now, and the rungs keep 15 at the owner's own ask. The compact strip's
	 *   12 px price run is the one figure outside the ramp, and it is a
	 *   different FORM rather than a seventh step.
	 * - **Ground.** `rgb(15 17 23 / 88%)` **and** `opacity: 0.75` on the
	 *   alternative put its ground at an effective 66 % over the game. The
	 *   ground is `rgb(13 15 20 / 96%)` and the `opacity` is gone: **faint is a
	 *   muted frame and a muted text colour**, never a transparency, because
	 *   transparency fades the words as well as the chrome. Both boxes carry a
	 *   2 px frame — cyan on the pick — so the pick buys no height, which is
	 *   why `pick` left `offerBoxSignature`.
	 * - **One item row, `A + B ……→…… C`.** `base → vial → upgraded` under the
	 *   drop pair redrew the row's own two icons at half size underneath
	 *   themselves (Vertolka, 12:19). The recipe ROW is gone and its sentence
	 *   moved into the item row's own layout: the two drops packed left with a
	 *   muted `+` between them, a stretched muted `→`, and the upgrade on the
	 *   right edge. Each price sits CENTRED UNDER its own art rather than
	 *   beside it — a run of icon-then-price left nothing saying which price
	 *   belonged to which item. Two equal HALVES were tried in between and left
	 *   "very weird spacings" (owner, off the in-game look): a cell centred in
	 *   129 px of empty half sits nowhere in particular.
	 * - **The ladder gets a name, at its left.** Two ladder columns sitting
	 *   directly under two ICON columns read as "quant from the gloves, rarity
	 *   from the vial". Named, they read as the ROOM's bonus, which is what
	 *   they are — so `Room bonuses` went in front of them, and after two
	 *   in-game looks it settled INLINE at the row's left with the cells
	 *   keeping their value-over-word shape: the label above them made the
	 *   shortest section on the box the tallest, and value-beside-word on one
	 *   line read as prose. The chaos the rates are worth sits at the far
	 *   right, and it is the row's one droppable part — see `bonusAmount`.
	 * - **One value token.** `39 c ⧠G` was three visual objects for one fact.
	 *   It is `39c` in one span; the `G` box became the muted ` · est.` on the
	 *   header's second line and the `F` box was dropped outright, because the
	 *   value it sat beside already reads `grade C`.
	 * - **The mod name carries the verdict.** Vertolka, 12:23, on the nine grey
	 *   slot silhouettes: *"for player it is information this chest is good
	 *   because of temple mod items in it, it don't need further explanation on
	 *   infographic."* The glyph row went; the name went green or red instead
	 *   (`OfferMod.worth`, Rust's `drops::Worth`). The owner then asked for the
	 *   row BACK as word CHIPS — `WEAPON · BOOTS` reads at arm's length where a
	 *   silhouette needs a legend — so the row is words now and `SlotIcon.svelte`
	 *   has no caller on this box.
	 *
	 * # What the box carries
	 *
	 * The number POE-257 computes is the whole of the ranking. The box leads with
	 * the resolved room and tier, then shows that value in chaos and what makes it
	 * up: the items the room drops, as ICONS with prices, the quantity / rarity
	 * bonus ladder, the vial upgrade for the line's unique where there is one,
	 * and the temple mod at the end. Everything on screen comes from the offer
	 * view — the same `Valued` row the advisor ranked on (POE-257 D6) — so no box can
	 * justify a number the recommendation did not use.
	 *
	 * The honesty rules are the design's and are not negotiable in markup: an
	 * unpriced item keeps its icon and reads `no price`, a board with no market
	 * at all reads `—` and shows the grade instead of a chaos number, and
	 * nothing is ever drawn as `0c` to fill a gap. `view.ts` decides all of
	 * those words; this file only places them.
	 *
	 * # Every row is a fixed height
	 *
	 * Not housekeeping: `overlay-geometry.ts`'s `FULL_BOX_MAX_CSS` is SUMMED
	 * from the row table below, and `overlay-geometry.test.ts` restates it, so
	 * a row added here without a number there leaves the placer budgeting for a
	 * box that is not on screen. Each row is a `height` (or a `line-height`)
	 * plus a stated `margin-top`, and the comment on each rule names both.
	 * `.note` is the one exception and says so where it is declared.
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
	 * the same colour. The other box carries a 2 px MUTED frame and dimmer
	 * text: **faint is the alternative**, the rule the room widget's conditional
	 * door and unchosen glyph already follow, so everything at full strength is
	 * a thing to do now. There is no arrow and no line; POE-248 retired those
	 * everywhere on this overlay.
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
	 * Its HEIGHT is not — how many item cells a room has, and whether it has a
	 * mod, are properties of the board — so the box is still rendered before
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
	import { getIconUrl } from '$lib/icons';
	import { offersCompact, offerStackPlacement } from './overlay-geometry';
	import { SLOT_NAMES } from './slice';
	import { offerBoxSignature } from './view';
	import type { OfferBox, OfferDriver } from './view';
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

	/** One cell of the item row — art and a price, and for a drop or the third one a
	 *  caption. The two sources it unifies say the same thing in different
	 *  shapes: a dropped item is an `OfferDriver`, the upgrade is an
	 *  `OfferRecipeItem`, and the row draws them identically. */
	interface ItemCell {
		iconName: string;
		/** Which item-frame colour the art gets — the GAME's, not this app's. */
		kind: string;
		price: string;
		/** False when `price` is a refusal (`no price`, `—`) rather than a
		 *  number. Read off the view's own `priced` on both sources; matching
		 *  the refusal STRING here would break the day `view.ts` rewords it. */
		priced: boolean;
		/** `upgraded` under the third cell's price, or a rare drop's rate, else null. */
		caption: string | null;
	}

	/** The icon endpoint's URL for one item, or null where there is no item.
	 *  `ItemIcon` renders nothing for a null `src`, which is what the compact
	 *  strip's sale glyph wants — its own glyph is drawn instead. */
	function iconSrc(name: string | null): string | null {
		return name === null ? null : getIconUrl('temple', name);
	}

	/** The sale term, or null. It is the one driver the full form draws as TEXT:
	 *  the design has always described this row as "no art", and rendering the
	 *  null-icon glyph for it reserved 39 px for a row that needs 18. */
	function saleDriver(box: OfferBox): OfferDriver | null {
		return box.drivers.find((driver) => driver.kind === 'sale') ?? null;
	}

	/** One drop cell, or null where the line has no such drop. */
	function dropCell(driver: OfferDriver | null): ItemCell | null {
		if (driver === null || driver.iconName === null) return null;
		return {
			iconName: driver.iconName,
			kind: driver.kind,
			price: driver.price,
			priced: driver.priced,
			caption: driver.caption
		};
	}

	/**
	 * The item row's three COLUMNS, always three, in a fixed order: the line's
	 * unique, the vial its architect rolls for, and what the two become.
	 *
	 * **Position, not packing** (owner, 2026-09-10). An absent drop leaves its
	 * column EMPTY rather than sliding the next one left, so the vial sits in
	 * the same place on every box and the two boxes of a pair can be compared
	 * column by column — which is the whole reason there are two of them.
	 *
	 * **The upgrade rides on the drops.** A recipe is a property of the LINE and
	 * not of either drop cell, so the guard is that the line dropped SOMETHING
	 * — not that the unique in particular is there or priced. With no drop at
	 * all the row would be one lone cell captioned `upgraded`, naming a result
	 * with nothing on screen to be the result of, and that is exactly the
	 * fallback state: no rows, and a recipe whose members are all em dashes.
	 * Branching on `OfferBox.state` would say the same thing, and `view.ts`
	 * states the rule that this file may not: what a state changes is words,
	 * and the words are there.
	 */
	function itemCells(box: OfferBox): (ItemCell | null)[] {
		const unique = dropCell(box.dropPair[0]);
		const vial = dropCell(box.dropPair[1]);
		const drops = unique !== null || vial !== null;
		const upgraded = box.recipe;
		return [
			unique,
			vial,
			upgraded === null || !drops
				? null
				: {
						iconName: upgraded.upgraded.iconName,
						kind: 'unique',
						price: upgraded.upgraded.price,
						priced: upgraded.upgraded.priced,
						caption: 'upgraded'
					}
		];
	}

	/**
	 * Whether one ladder's rungs are wider than the stated ladders ever are.
	 *
	 * Tiers 1 and 2 are a single digit on every line poedb states a rate for,
	 * and tier 3 reaches two (`+4/8/12%`). Anything past that is a line like
	 * Factory, whose quantity stat is printed twice and summed (`+22/44/66%`).
	 */
	function ladderIsWide(text: [string, string, string] | null): boolean {
		if (text === null) return false;
		return text[0].length > 1 || text[1].length > 1 || text[2].length > 2;
	}

	/**
	 * The bonus row's chaos amount, or null where the row has no room for it.
	 *
	 * **The amount is the row's one droppable part, and it is DROPPED rather
	 * than clipped.** The ladders never shrink (`.lc` is `flex: 0 0 auto` and
	 * `nowrap`), because half a rung is worse than no rung, and the label names
	 * the row — so on the one shape that can outgrow 272 px the amount is the
	 * thing that goes. It is also the only part the box says twice: what the
	 * two rates are worth is already inside the value at the top right.
	 *
	 * The test is the RUNG WIDTH and not a measured width, because this file
	 * has no measurement to read at render time and a box that decided its own
	 * content from `offsetWidth` would re-measure on every read.
	 */
	function bonusAmount(box: OfferBox): string | null {
		const amount = box.bonus?.amount ?? null;
		if (amount === null || box.ladder === null) return amount;
		return ladderIsWide(box.ladder.quantText) || ladderIsWide(box.ladder.rarityText)
			? null
			: amount;
	}

	/** Whether the box prints ` · est.` after its chip. The same condition the
	 *  boxed `G` fired on, and deliberately read off `marks` rather than
	 *  recomputed: `view.ts` decides what "guessed" means for a whole box. */
	function guessed(box: OfferBox): boolean {
		return box.marks.includes('G');
	}
</script>

{#snippet icon(name: string | null, kind: string, size: number)}
	<!-- The frame is an OUTLINE and not a border, so the art keeps its own box:
	     39 px is exactly one half of the 78 px source, and an integer downscale
	     is what stops the icons shimmering against the game's own inventory
	     art. The compact strip's 26 px is the other integer downscale — it was
	     20 px, which is neither, and shimmered for that reason. The colours are
	     the GAME's item-frame colours (unique brown, vial purple) rather than
	     this app's palette, because they are what the player is already reading
	     two inches to the right. -->
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

<!-- One bonus cell: the three-rung ladder OVER the stat it is a ladder of, both
     centred. The room's own tier is lit; the other two sit at .42, which is
     where the dimmed rungs had to go to survive the game's background at 96 %
     ground. -->
{#snippet bonusCell(text: [string, string, string], word: string, tier: number)}
	<div class="lc">
		<span class="lv"
			><span class:lit={tier === 1}>+{text[0]}</span>/<span class:lit={tier === 2}>{text[1]}</span
			>/<span class:lit={tier === 3}>{text[2]}%</span></span
		>
		<span class="w">{word}</span>
	</div>
{/snippet}

<!-- One item cell: art over its price, and under the third one its caption. -->
{#snippet itemCell(cell: ItemCell)}
	<div class="item">
		{@render icon(cell.iconName, cell.kind, 39)}
		<span class="pr" class:none={!cell.priced}>{cell.price}</span>
		{#if cell.caption}<span class="cap">{cell.caption}</span>{/if}
	</div>
{/snippet}

{#each boxes as box, i (signature + "|" + box.offer.index)}
	{@const sale = saleDriver(box)}
	{@const cells = itemCells(box)}
	<!-- Two of the four optional sections as booleans (the mod's own `{#if}`
	     is the third and the warning line the fourth), because the hairlines
	     BETWEEN sections are conditional: a rule with nothing under it is a
	     line the box drew for its own sake. Each one is a PRESENCE over fields
	     `offerBoxSignature` already carries, so nothing new re-measures. -->
	{@const hasCells = cells.some((cell) => cell !== null)}
	{@const hasItems = sale !== null || hasCells || box.content !== null || box.fold !== null || box.note !== null}
	{@const hasBonus = box.ladder !== null || box.bonus !== null}
	{@const amount = bonusAmount(box)}
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
		<!-- Two fixed header lines: the value belongs to the room line, while
		     the kind/tier, the completeness chip and the provenance word share
		     the second. -->
		<div class="head-title">
			<span class="room">{box.headline}</span>
			{#if box.forced}<span class="forced">({box.forced})</span>{/if}
			{#if box.valueText !== null}
				<span class="val">
					<span class="num" class:grade={box.value === null} class:stale={box.stale}
						>{box.valueText}{box.value === null ? '' : 'c'}</span
					>
				</span>
			{/if}
		</div>
		<div class="head-sub">
			<span class="kind">{box.builds}</span>
			{#if box.chip}
				<span class="chip">{box.chip}</span>
			{:else if box.valueText !== null && box.value !== null}
				<span class="perrun">per run</span>
			{/if}
			{#if guessed(box)}<span class="est">· est.</span>{/if}
		</div>
		<!-- Gated like every other rule, and for the same reason: an offer whose
		     printed target did not resolve has no value, no rows and no mod, so
		     on a fresh market this is a hairline drawn over nothing. -->
		{#if compact ? box.drivers.length > 0 || box.ageLine !== null : hasItems || hasBonus || box.mod !== null || box.ageLine !== null}
			<div class="rule"></div>
		{/if}

		{#if compact}
			<!-- Icons and prices, one row. What the eye uses at arm's length —
			     who, what it builds, what it is worth and what pays for it —
			     with the bonus ladder, the temple mod and its slots dropped. -->
			{#if box.drivers.length > 0}
				<div class="strip">
					{#each box.drivers as driver (driver.name)}
						{@render icon(driver.iconName, driver.kind, 26)}
					{/each}
					{#if box.stripPrices}<span class="prices">{box.stripPrices}</span>{/if}
				</div>
			{/if}
			{#if box.ageLine}
				{#if box.drivers.length > 0}<div class="rule"></div>{/if}
				<div class="mkt" class:warn={box.stale}>{box.ageLine}</div>
			{/if}
		{:else}
			{#if sale}
				<div class="sale">
					<span class="nm">{sale.name}</span>
					<span class="pr" class:none={!sale.priced}>{sale.price}</span>
				</div>
			{/if}

			{#if box.content !== null}
				<!-- A content line has no recipe chain: its vial, when present, is
				     the left fact and the line's prose is the right fact. The
				     drops-nothing note yields to it; the instrumental and own-number
				     wordings still print under the row (299 px worst case, inside
				     the budget). -->
				<div
					class="items"
					class:aftersale={sale !== null}
					class:captioned={cells.some((cell) => cell !== null && cell.caption !== null)}
				>
					{#if cells[1] !== null}{@render itemCell(cells[1])}{/if}
					<span class="content-spacer"></span>
					<span class="content-cell"><span class="content">{box.content}</span></span>
				</div>
			{:else if hasCells}
				<!-- ONE flex row that reads as the recipe it is (owner, after the
				     in-game look): `A + B ……→…… C`. The two drops pack LEFT with a
				     muted `+` between them, a flexible spacer carries the muted
				     `→` at its own centre, and the upgrade sits on the row's right
				     edge. The spacer is what makes the arrow mean something — it
				     is the only gap on the row wide enough to read as a step from
				     one thing to another. -->
				<div
					class="items"
					class:aftersale={sale !== null}
					class:captioned={cells.some((cell) => cell !== null && cell.caption !== null)}
				>
					{#if cells[0] !== null}{@render itemCell(cells[0])}{/if}
					{#if cells[0] !== null && cells[1] !== null}<span class="join">+</span>{/if}
					{#if cells[1] !== null}{@render itemCell(cells[1])}{/if}
					{#if cells[2] !== null}
						<span class="step"><span class="join">→</span></span>
						{@render itemCell(cells[2])}
					{/if}
				</div>
			{/if}

			{#if box.fold}<div class="fold">{box.fold}</div>{/if}

			{#if box.note}<p class="note">{box.note}</p>{/if}

			{#if hasBonus}
				{#if hasItems}<div class="rule"></div>{/if}
				<!-- The label INLINE on the left (owner, off the in-game look — the
				     alignment was the ask, not the shape), then the two ladders
				     sharing what is left of the row equally, each a value over its
				     word. What the two rates are worth trails the rarity cell, at
				     the row's far right: the label is at the far left now, and an
				     amount parked beside it would sit furthest from the numbers it
				     is about. -->
				<div class="lad">
					<span class="lb">Room bonuses</span>
					{#if box.ladder}
						{@const quant = box.ladder.quantText}
						{@const rarity = box.ladder.rarityText}
						{#if quant !== null}{@render bonusCell(quant, 'quant', box.ladder.tier)}{/if}
						{#if rarity !== null}{@render bonusCell(rarity, 'rarity', box.ladder.tier)}{/if}
					{:else}
						<div class="lc"><span class="w">{box.bonus?.label}</span></div>
					{/if}
					{#if amount !== null}<span class="amt">{amount}</span>{/if}
				</div>
			{/if}

			{#if box.mod}
				{#if hasItems || hasBonus}<div class="rule"></div>{/if}
				<div class="mod" title={box.mod.hint ?? undefined}>
					<span class="lb">Temple mod:</span>
					<span
						class="nm"
						class:good={box.mod.worth === 'good'}
						class:junk={box.mod.worth === 'junk'}>{box.mod.name}</span
					>
					{#if box.mod.price !== 'no price' && box.mod.price !== '—'}
						<span class="pz">({box.mod.price})</span>
					{/if}
				</div>
				{#if box.mod.slots.length > 0}
					<div class="slots">
						<span class="lb">Appears on:</span>
						{#each box.mod.slots as slot (slot)}
							<span class="sc">{SLOT_NAMES[slot]}</span>
						{/each}
					</div>
				{/if}
			{/if}
			{#if box.ageLine}
				{#if hasItems || hasBonus || box.mod !== null}<div class="rule"></div>{/if}
				<p class="mkt" class:warn={box.stale}>{box.ageLine}</p>
			{/if}
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
	   placement would not be the number on screen.

	   The ground is 96 % opaque and the shadow sits OUTSIDE the border box, so
	   it costs no `offsetHeight`. No `backdrop-filter`: this is a transparent
	   Tauri window over the game, so there is nothing in the compositor behind
	   it to blur, and alpha is the only lever there is. */
	.box {
		position: absolute;
		box-sizing: border-box;
		display: flex;
		flex-direction: column;
		padding: 10px 12px;
		background: rgb(13 15 20 / 96%);
		border: 2px solid rgb(148 163 184 / 40%);
		border-radius: 6px;
		color: var(--color-lab-text);
		box-shadow: 0 2px 8px rgb(0 0 0 / 40%);
		pointer-events: none;
	}

	/* The advisor's pick. The frame IS the pointer (owner: no arrows anywhere),
	   in the same cyan as the room widget's kill glyph so the two surfaces mark
	   the same architect the same way. */
	.box.pick {
		border-color: var(--color-lab-cyan);
		box-shadow: 0 3px 12px rgb(0 0 0 / 50%);
	}

	/* Faint by COLOUR, never by opacity. `opacity: 0.75` over an 88 % ground
	   put the alternative's background at an effective 66 % — the owner's "too
	   transparent" — and faded its words along with its chrome. This dims the
	   text and nothing else; the frame above does the rest. Between
	   `--color-lab-text` and `--color-lab-text-secondary`, which is a step the
	   palette does not name. */
	.box:not(.pick) {
		color: #c9cace;
	}

	/* ------------------------------------------------------- the header -- */

	/* Row 1: height 26, margin-top 0. */
	.head-title {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 26px;
		overflow: hidden;
	}

	.room {
		flex: 1 1 auto;
		min-width: 0;
		font-size: 15px;
		font-weight: 700;
		line-height: 26px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.pick .room {
		color: var(--color-lab-cyan);
	}

	.box:not(.pick) .room {
		color: #aeb3bf;
	}

	/* Inside the headline, not under it: the point is that the kill was not a
	   ranked choice, and a note the eye reads as a separate line is one it
	   skips. It squeezes the room name to heavy ellipsis, which is accepted —
	   a forced kill is not a comparison, so there is nothing to read the full
	   room name FOR. */
	.forced {
		flex: 0 0 auto;
		font-size: 11px;
		font-weight: 400;
		color: var(--color-lab-yellow);
	}

	/* --------------------------------------------------- what it is worth -- */

	.val {
		display: flex;
		align-items: center;
		flex: 0 0 auto;
		height: 26px;
	}

	/* ONE token: `39c`, unit at the same size, no space and no separate span.
	   `39 c ⧠G` was three visual objects for one fact. `tabular-nums` so two
	   boxes' figures line up vertically — the comparison is the point, and
	   proportional digits make two three-figure numbers look like different
	   lengths. */
	.num {
		font-size: 22px;
		font-weight: 700;
		line-height: 26px;
		letter-spacing: -0.02em;
		font-variant-numeric: tabular-nums;
	}

	/* The ladder's answer is a LETTER, not a number, and it is set smaller so
	   the two are not mistaken for each other at a glance. No `F` box beside
	   it: the value already reads `grade C`, and the letter was saying twice
	   what the word said once. */
	.num.grade {
		font-size: 17px;
		letter-spacing: 0;
	}

	/* Marked on the value itself, because that is what a player would otherwise
	   act on. Since POE-258's M1 fix `OfferBox.stale` is a CLOCK verdict
	   (`view.ts::marketStale`, the read's own `asOf` against the published
	   `staleAfterMs`), so the slot under this underline carries one of two
	   things and the mark means the same thing about both — do not act on this
	   number:

	   - the GRADE, on a board Rust valued off an already-stale snapshot: it
	     priced nothing, so the box is in its fallback form (`OfferBox.state`)
	     and the underline says the letter standing there is standing in;
	   - a real CHAOS FIGURE, on a board that priced live and has since aged
	     past two hours with nothing re-read: the number is a price that has
	     gone old rather than a stand-in, which is also why `marketNote` drops
	     its `— base values` suffix in exactly this case. */
	.num.stale {
		border-bottom: 1px dotted var(--color-lab-yellow);
	}

	/* Row 2: height 15, margin-top 0. */
	.head-sub {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 15px;
	}

	.kind {
		flex: 1 1 auto;
		min-width: 0;
		font-size: 11px;
		font-weight: 500;
		line-height: 15px;
		color: var(--color-lab-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.perrun {
		flex: 0 0 auto;
		font-size: 10px;
		font-weight: 400;
		line-height: 15px;
		color: var(--color-lab-text-muted);
	}

	/* Occupies the same right-hand slot as the run mode. */
	.chip {
		flex: 0 0 auto;
		box-sizing: border-box;
		height: 15px;
		padding: 0 5px;
		font-size: 10px;
		font-weight: 600;
		line-height: 13px;
		color: var(--color-lab-text-secondary);
		border: 1px solid rgb(228 228 231 / 14%);
		border-radius: 3px;
	}

	/* Where the number came from, in a word — what the boxed `G` used to say
	   beside the value. It rides on line two so the value can be one token, and
	   it fires on the same condition (`offerBoxMarks`' `G`), so the right side
	   reads `per run · est.` or `floor · 1 unpriced · est.`. */
	.est {
		flex: 0 0 auto;
		/* Pulls the row's 8 px gap back to the 5 px §6 calls for. The separator
		   dot is part of this word rather than of the chip before it, so it has
		   to sit closer to the chip than the chip sits to the kind. */
		margin-left: -3px;
		font-size: 10px;
		font-weight: 400;
		line-height: 15px;
		color: var(--color-lab-text-muted);
	}

	/* The section hairline: height 1, margin 6 above and 6 below — 13 px, and
	   ONE rule for all four of them (owner via Vertolka, 2026-09-10). It sits
	   under the header and between each pair of sections that are both present:
	   header · items · room bonuses · temple mod · market warning. Never
	   unconditional: a rule with nothing under it is a line the box drew for
	   its own sake, which is what an unresolved offer on a fresh market got.

	   6 is the spec's own figure for the header hairline, and the first cut of
	   this restyle overrode it at 4 — four rules at 6 were 13 px apiece and put
	   the worst-case box at 324 px against the 316 px the panel's own diagonal
	   admits. The owner's one-line `Room bonuses` row then gave 27 px back, so
	   the override is no longer paid for and the spec's number stands.

	   `--color-lab-border` (#2a2d37) is too dark to register a 1 px edge on a
	   96 %-opaque #0d0f14 ground; alpha-on-text tracks the ground instead. */
	.rule {
		height: 1px;
		margin: 6px 0;
		background: rgb(228 228 231 / 10%);
	}

	/* ---------------------------------------------------------- the rows -- */

	/* Row 4: height 18, margin-top 0. No icon and no placeholder glyph — the
	   design has always described this row as text, and rendering the null-icon
	   SVG at 39 px for it was the single biggest contributor to the cramped
	   feel. The leading `+` and the green carry the "money out" meaning; it is
	   the only green number in the box apart from a mod name worth chasing, and
	   the two agree rather than collide. */
	.sale {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 18px;
	}

	.sale .nm {
		flex: 1 1 auto;
		min-width: 0;
		font-size: 11px;
		font-weight: 500;
		line-height: 18px;
		color: var(--color-lab-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.sale .pr {
		flex: 0 0 auto;
		font-size: 13px;
		font-weight: 600;
		line-height: 18px;
		color: var(--color-lab-green);
		font-variant-numeric: tabular-nums;
	}

	.sale .pr.none {
		font-size: 11px;
		font-weight: 400;
		color: var(--color-lab-text-muted);
	}

	/* Row 5: height 57 (69 captioned), margin-top 0 — 5 under a sale row.
	   ONE row, THREE equal columns: the line's unique, the vial its architect
	   rolls for, and what the two become. The `base → vial → upgraded` chain
	   this replaces redrew the row's own two icons at half size underneath
	   themselves.

	   ONE flex row reading `A + B ……→…… C` (owner, after the in-game look): the
	   two drops packed left on 8 px gaps with a muted `+` between them, then a
	   flexible spacer carrying a muted `→` at its centre, then the upgrade on
	   the row's right edge. Two equal HALVES were tried first and left, in the
	   owner's words, "very weird spacings" — a cell centred in 129 px of empty
	   half sits nowhere in particular, while an edge and a stretched arrow say
	   which items are the room's and which one they buy.

	   Cells are simply omitted when the line has none: the `+` goes with either
	   drop, and the spacer and the arrow go with the upgrade, so a two-item row
	   is `A + B` packed left and nothing else. */
	.items {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		height: 57px;
		/* 6 px of air under the header rule (owner, 2026-09-10): the art sat
		   tighter to the rule above it than to the rule below. */
		margin-top: 6px;
	}

	/* A content line gets the same item-row budget: its vial, if any, stays on
	   the left and the line prose takes the right. The content cell replaces the
	   drops-nothing `.note`, so the fact is not duplicated or left out on
	   fallback; an instrumental or override box still prints its own wording
	   under this row. */
	.content-spacer {
		flex: 1 1 auto;
		min-width: 0;
		height: 39px;
	}

	.content-cell {
		display: flex;
		flex: 0 1 auto;
		align-items: center;
		min-width: 0;
		max-width: 100%;
		height: 39px;
	}

	.content {
		flex: 0 1 auto;
		align-self: center;
		min-width: 0;
		max-width: 100%;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
		text-align: right;
		font-size: 13px;
		font-weight: 500;
		line-height: 15px;
		color: var(--color-lab-text);
	}

	/* The stretch between what the room drops and what those two become. It
	   holds the arrow at its own centre, and being the only `flex: 1` on the
	   row it is also what pins the upgrade to the right edge. */
	.step {
		display: flex;
		flex: 1 1 auto;
		align-items: center;
		justify-content: center;
		min-width: 0;
		height: 39px;
	}

	/* The two connectors, and deliberately the quietest marks on the box: they
	   say how the three items relate, which is a thing the player reads once.
	   Their line box is the ART's 39 px, so they centre on the icons rather
	   than on the cell — a cell is taller than its art by a price and, in the
	   third one, a caption. */
	.join {
		flex: 0 0 auto;
		height: 39px;
		font-size: 11px;
		font-weight: 400;
		line-height: 39px;
		color: var(--color-lab-text-muted);
	}

	.items.captioned {
		height: 69px;
	}

	.items.aftersale {
		margin-top: 8px;
	}

	/* The price sits UNDER its art, centred, rather than beside it: three cells
	   of icon-then-price ran the row into one long strip of alternating art and
	   numerals with nothing saying which price belonged to which item. */
	.item {
		display: flex;
		flex-direction: column;
		align-items: center;
		min-width: 0;
	}

	/* Margins rather than a column `gap`, because the two text lines are one
	   block: 3 px of air under the art, and none between the price and the
	   caption that names it. A uniform gap would push the caption away from the
	   number it is about and read as a third line rather than as its label. */
	.item .pr {
		margin-top: 3px;
		white-space: nowrap;
		font-size: 13px;
		font-weight: 600;
		line-height: 15px;
		font-variant-numeric: tabular-nums;
	}

	/* `no price` and `—` are refusals, not numbers, so they are set at the
	   weight of prose and in the muted colour — a bold `no price` reads as an
	   amount at a glance, and a refusal is never a numeral. */
	.item .pr.none {
		font-size: 11px;
		font-weight: 400;
		color: var(--color-lab-text-muted);
	}

	/* Names the CELL, not a section, so it is deliberately lighter than a
	   section label: 400 where those are 700. Under the price, so every cell's
	   art still sits on the row's own top edge. */
	.item .cap {
		font-size: 10px;
		font-weight: 400;
		line-height: 12px;
		letter-spacing: 0.04em;
		color: var(--color-lab-text-muted);
	}

	/* Row 6: height 13, margin-top 3 — inside the item group, not a new one.
	   Unreachable as the wording stands (`view.ts`'s four `ROW_KINDS` minus the
	   mod cannot exceed `FULL_DRIVER_ROWS`) and styled anyway, because the day
	   a fifth row kind is added is not the day to discover it has no rule. */
	.fold {
		margin-top: 3px;
		height: 13px;
		font-size: 11px;
		font-weight: 500;
		line-height: 13px;
		color: var(--color-lab-text-muted);
	}

	/* --------------------------------------------------------- the icons -- */

	.icon {
		display: inline-flex;
		flex: 0 0 auto;
		align-items: center;
		justify-content: center;
		border-radius: 2px;
		/* An OUTLINE rather than a border so the art keeps its own box and the
		   39 px downscale stays an exact half of the 78 px source. */
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

	/* The compact strip's only null-icon row: the sale term has no item, so it
	   keeps the drawn glyph there. The full form's sale row has no art at all. */
	.icon.sale {
		outline-color: var(--color-lab-green);
		color: var(--color-lab-green);
	}

	/* ---------------------------------------------------- section labels -- */

	/* ONE style for all three: `Room bonuses`, `Temple mod:` and `Appears on:`.
	   Mixed case with no `text-transform` — the uppercase-by-CSS labels of the
	   shipped build read as three different kinds of heading at three different
	   widths. None of the three is its own row: each opens the line it labels,
	   and the rule above it is what separates the section. */
	.lb {
		flex: 0 0 auto;
		font-size: 10px;
		font-weight: 700;
		line-height: 13px;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-muted);
	}

	/* ----------------------------------------------------- room bonuses -- */

	/* Row 7: height 31 (18 + 13), margin-top 0. `Room bonuses` INLINE at the
	   left, then the two ladders sharing the remainder equally — each a value
	   over its word, both centred — and what the two rates are worth at the far
	   right.

	   The label is what fixed the reading this row has been through three
	   times: two columns directly under two ICON columns (the shipped build)
	   read as "quant from the gloves, rarity from the vial"; the label on its
	   own line above them made the shortest section on the box the tallest; and
	   value-beside-word on one line read as prose. Named from the left, with
	   the value over the word it belongs to, the pair reads as the ROOM's bonus
	   — which is what it is.

	   The rungs stay at 15 px. It is one step above the 13 px the type ramp
	   gives a figure, and it is the owner's own ask ("a little larger"): these
	   are the only numbers on the box a player reads as a LADDER rather than as
	   a price, and at 13 the lit rung stopped separating from the dim two. */
	.lad {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 31px;
		color: var(--color-lab-yellow);
		overflow: hidden;
	}

	/* Never shrinks, and the auto margins are what spread the two ladders evenly
	   across the row's remainder instead. `flex: 1 1 0` would have shared that
	   remainder too, but it also lets a cell be squeezed BELOW its content, and
	   a clipped `+4/8/1…` is worse than no amount beside it — which is why the
	   amount is the part that goes when the row runs out of room
	   (`bonusAmount`). */
	.lc {
		display: flex;
		flex: 0 0 auto;
		flex-direction: column;
		align-items: center;
		margin: 0 auto;
		white-space: nowrap;
	}

	.lv {
		font-size: 15px;
		font-weight: 600;
		line-height: 18px;
		font-variant-numeric: tabular-nums;
	}

	/* The room's own tier at full strength, the other two at .42. It was .55,
	   and the lit/dim contrast was too weak to survive the game's background. */
	.lv > span {
		opacity: 0.42;
	}

	.lv > span.lit {
		opacity: 1;
		font-weight: 700;
	}

	.w {
		font-size: 11px;
		font-weight: 400;
		line-height: 13px;
		opacity: 0.72;
	}

	/* What the two percentages are WORTH, at the row's far right, trailing the
	   rarity cell: it is one chaos figure about the pair, not a third ladder,
	   and the label it used to sit beside is now at the opposite edge. Withheld entirely on a
	   tier-scaled box (`view.ts`'s `rowsAreTier3`), so nothing here has to know
	   about the scale. */
	.lad .amt {
		flex: 0 0 auto;
		font-size: 11px;
		font-weight: 600;
		line-height: 13px;
		color: var(--color-lab-text);
		font-variant-numeric: tabular-nums;
	}

	/* ------------------------------------------------- the temple mod -- */

	/* Row 8: height 18, margin-top 0 — first in its section, under a rule. */
	.mod {
		display: flex;
		align-items: baseline;
		gap: 5px;
		height: 18px;
		min-width: 0;
		white-space: nowrap;
		overflow: hidden;
	}

	.mod .nm {
		min-width: 0;
		font-size: 13px;
		font-weight: 600;
		line-height: 18px;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* The name carries Vertolka's verdict on the mod FAMILY (2026-09-10), which
	   `drops.rs` states and `OfferMod.worth` carries; nothing here derives it
	   from a price, because one family out of seven has one. Two reuses, both
	   judged acceptable: green already marks the sale gain — both mean "worth
	   something", so they agree — and red already means a closed seal on the
	   room widget, which is a different surface. */
	.mod .nm.good {
		color: var(--color-lab-green);
	}

	.mod .nm.junk {
		color: var(--color-lab-red);
	}

	/* Only the one priced case (Crucible of Flame / Puhuarte's 30 c guess). A
	   `no price` mod prints no cell at all rather than the words. */
	.mod .pz {
		flex: 0 0 auto;
		font-size: 13px;
		font-weight: 600;
		line-height: 18px;
		color: var(--color-lab-text-secondary);
		font-variant-numeric: tabular-nums;
	}

	/* Row 9: chip rows of 14 with a 4 px row gap, margin-top 5 — 19 for one
	   row and 37 for two. The chips wrap once the row passes 272 px: five
	   (Xopec) always, and four or even three when BODY ARMOUR is among them
	   (Guatelitzi, Tacati). Two rows is the worst case at any count, which the
	   budget carries, and `offerBoxSignature` carries `slots.length` as the
	   cheap proxy for that shape. 5 and not 8: the chips belong to the mod line
	   above them, and the section rules own the rhythm between sections.

	   WORDS and not the nine grey silhouettes this replaces: the owner's
	   verdict on those was that they are "defenetely NOT intuitive", and a
	   glyph that needs a legend has no room for one on an overlay. */
	.slots {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px;
		margin-top: 5px;
	}

	.sc {
		flex: 0 0 auto;
		box-sizing: border-box;
		height: 14px;
		padding: 0 5px;
		font-size: 10px;
		font-weight: 600;
		line-height: 12px;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--color-lab-text-secondary);
		border: 1px solid rgb(228 228 231 / 14%);
		border-radius: 3px;
	}

	/* -------------------------------------------------- the other lines -- */

	/* The one line the rows cannot say — an instrumental line's worth, the
	   player's own number, or a line that drops nothing at all.
	   Content lines say what they drop nothing for in the item row's right-hand
	   `.content` cell, so the drops-nothing wording is never printed under one;
	   the instrumental and own-number wordings are, and the tallest box that
	   makes is 299 px, inside the 311 the sum below budgets.
	   `margin-top: 5`, and the ONE row here with no fixed height: its longest
	   wording wraps to two lines at 272 px, and clipping it would drop the only
	   sentence on a box that has no rows to read instead. It is not in
	   `FULL_BOX_MAX_CSS` for the reason it can afford not to be — on a chest
	   box every state that prints it has no item row and no fold, and the
	   content boxes that print it stop at the 299 above. */
	.note {
		margin: 5px 0 0;
		font-size: 11px;
		font-weight: 500;
		line-height: 15px;
		color: var(--color-lab-text-muted);
	}

	/* Row 10: height 13, margin-top 0 — its own rule sits above it. Warning
	   states only (`OfferBox.ageLine` is null on a fresh read), so a live board
	   spends no height saying its prices are current. */
	.mkt {
		margin: 0;
		height: 13px;
		font-size: 11px;
		font-weight: 400;
		line-height: 13px;
		color: var(--color-lab-text-muted);
	}

	.mkt.warn {
		color: var(--color-lab-yellow);
	}

	/* ------------------------------------------------------- the compact -- */

	/* Height 26, margin-top 0, under the header's rule. The icons are 26 px —
	   the other exact integer downscale of the 78 px source, up from a 20 px
	   that was neither 39 nor 26 and shimmered for it. */
	.strip {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 26px;
	}

	.strip .prices {
		margin-left: auto;
		font-size: 12px;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}
</style>
