<script lang="ts">
	/**
	 * The two offer boxes, on the left margin of the temple sheet (POE-249).
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
	 * Each box is content-sized, and its size is what decides where it fits, so
	 * it is rendered before it is placed and stays `visibility: hidden` until
	 * the placement answers. Hidden rather than `display: none`, because a box
	 * that is not laid out cannot be measured and the frame would never end.
	 * Same trick, and the same reason, as the config bar's in
	 * `WidgetHost.svelte`. The sizes are ARRAYS here, one entry per box, because
	 * the stack's arithmetic needs every box's height to stack the next one
	 * under it.
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
	import { offerStackPlacement } from './overlay-geometry';
	import type { OfferBox } from './view';
	import type { HostSize, WidgetRect } from '$lib/overlay/widgets/widget-geometry';

	let {
		boxes,
		blocks,
		panel,
		obstacles,
		host,
		maxWidth
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
		/** The registry's shipped width, applied as each box's wrap ceiling. */
		maxWidth: number;
	} = $props();

	let widths = $state<number[]>([]);
	let heights = $state<number[]>([]);

	/**
	 * What each box's measurement was taken FOR — everything that decides its
	 * size, in box order. Both the `{#each}` key and the reset below read it.
	 *
	 * STRINGS and not the `boxes` array itself, because `boxes` is a `$derived`
	 * over the SSOT poll: its reference changes on every tick even when the read
	 * did not, so anything keyed on the reference would re-key and re-measure
	 * several times a second. These change only when the rendered text does.
	 * `pick` is in them because the frame is 2 px on the pick and 1 px
	 * otherwise, which is 2 px of box.
	 */
	const signatures = $derived(
		boxes.map((box) =>
			[
				box.offer.index,
				box.headline,
				box.builds,
				box.rating,
				box.reason,
				box.forced,
				box.pick
			].join(' ')
		)
	);

	/**
	 * The same signatures as ONE STRING, and the identity of this value is the
	 * load-bearing part rather than its content.
	 *
	 * The effect below must fire when the read's text changes and NOT otherwise.
	 * `signatures` cannot be that trigger: it is an array, a derived's value is
	 * compared with `===`, and it is rebuilt from `boxes` on every SSOT poll —
	 * `ssot.temple` is reassigned unconditionally every 3 s and on every
	 * `ssot-changed` nudge, so a fresh array arrives with identical contents and
	 * always reads as changed. An effect on it clears the sizes every tick,
	 * while the `{#each}` key — the same strings, compared BY VALUE — does not
	 * change and so does not recreate the nodes; the observer never re-reports
	 * and the boxes vanish one poll after they appear, for the life of the
	 * board. Joined to a primitive, an unchanged read is `===` and nothing
	 * fires.
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
</script>

{#each boxes as box, i (signature + "|" + box.offer.index)}
	<div
		class="box"
		class:pick={box.pick}
		style="left:{placements[i]?.x ?? 0}px;top:{placements[i]?.y ?? 0}px;max-width:{maxWidth}px;{placements[
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
		{#if box.rating}
			<p class="rating">{box.rating}</p>
		{/if}
		{#if box.reason}
			<p class="reason">{box.reason}</p>
		{/if}
	</div>
{/each}

<style>
	/* The alternative, and it is drawn faint rather than left out: the player is
	   choosing between two blocks and the one not taken is half the decision. */
	.box {
		position: absolute;
		padding: 6px 10px;
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
		font-size: 16px;
		font-weight: 700;
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
		font-size: 13px;
	}

	.rating {
		font-size: 12px;
		color: var(--color-lab-text-secondary);
	}

	.reason {
		font-size: 12px;
		color: var(--color-lab-text-secondary);
	}
</style>
