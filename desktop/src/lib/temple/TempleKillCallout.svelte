<script lang="ts">
	/**
	 * The kill, as a box with an arrow into the architect block it is about
	 * (POE-244).
	 *
	 * The owner's ask, verbatim: *"it needs to be pointing to the thing, so I
	 * don't have to read, but see where to go"*. What this replaces is a panel
	 * of prose beside a redrawn copy of a board that is already on screen — the
	 * player had to read it, find the matching block in the game's own side
	 * panel, and only then click. So the box is short and the ARROW carries the
	 * instruction.
	 *
	 * # Two rules it must not break
	 *
	 * **It covers no read region.** Every OCR crop and sampled patch reaches the
	 * slice as `layout.rois`, and `calloutPlacement` refuses any position that
	 * overlaps one — including the position it wanted. When nothing is free the
	 * box is NOT drawn: the game's own panel is on screen either way, and a
	 * callout that corrupts the module's next read is the worse trade.
	 *
	 * **The arrow may cross what the box may not, and it stops SHORT of the
	 * block.** A 3 px line crossing a crop is not what breaks an OCR read; a
	 * filled panel sitting on the text is — and so is the arrowHEAD, which is a
	 * solid triangle. So the line is thin, the head is small and in USER units
	 * (see the marker), and `calloutArrow` ends the line `ARROW_STANDOFF_CSS`
	 * before the block's edge so the point lands beside the text rather than on
	 * it. That asymmetry between a thin line and a filled shape is the whole
	 * reason the arrow exists rather than the box being placed on the block.
	 *
	 * # The measure-then-place frame
	 *
	 * The box is content-sized, and its size is what decides where it fits — so
	 * it is rendered before it is placed and stays `visibility: hidden` until
	 * `calloutPlacement` has answered. Hidden rather than `display: none`,
	 * because a box that is not laid out cannot be measured and the frame would
	 * never end. Same trick, and the same reason, as the config bar's in
	 * `WidgetHost.svelte`.
	 */
	import { calloutArrow, calloutPlacement } from './overlay-geometry';
	import type { KillCallout } from './view';
	import type { HostSize, WidgetRect } from '$lib/overlay/widgets/widget-geometry';

	let {
		callout,
		target,
		panel,
		obstacles,
		host,
		maxWidth
	}: {
		/** What the box says. */
		callout: KillCallout;
		/** The chosen block's rect in CSS px, or null for a read that carried no
		 *  boxes — the box then shows with no arrow, because there is nothing on
		 *  screen it could honestly point at. */
		target: WidgetRect | null;
		/** The side panel's OCR crop in CSS px — the fallback anchor. */
		panel: WidgetRect | null;
		/** The never-cover set, CSS px. */
		obstacles: readonly WidgetRect[];
		host: HostSize;
		/** The registry's shipped width, applied as the text's wrap ceiling. */
		maxWidth: number;
	} = $props();

	let boxWidth = $state(0);
	let boxHeight = $state(0);

	const placement = $derived(
		calloutPlacement({
			target,
			panel,
			box: { w: boxWidth, h: boxHeight },
			obstacles,
			host
		})
	);
	const arrow = $derived(placement && target ? calloutArrow(placement, target) : null);
</script>

<!-- The arrow first, so the box is drawn over its own end rather than under it.
     The SVG is the whole window: both ends are window coordinates, and a line
     between two boxes cannot live inside either. -->
{#if arrow}
	<svg class="arrow-layer" aria-hidden="true">
		<defs>
			<!-- `markerUnits="userSpaceOnUse"` is load-bearing: the default is
			     `strokeWidth`, which multiplies every number below by the line's
			     3 px stroke and turned an 8 px head into a 24 px triangle
			     sitting on the block's first glyphs. With `refX` at 7.5 of an
			     8-wide marker the tip reaches half a pixel past the line's end,
			     and `calloutArrow` already stops that end `ARROW_STANDOFF_CSS`
			     short of the block — so the point lands about 9 px clear of the
			     text it is aiming at. -->
			<marker
				id="temple-kill-arrowhead"
				markerUnits="userSpaceOnUse"
				markerWidth="8"
				markerHeight="8"
				refX="7.5"
				refY="4"
				orient="auto"
			>
				<path d="M0,0 L8,4 L0,8 z" fill="var(--color-lab-purple)" />
			</marker>
		</defs>
		<line
			x1={arrow.x1}
			y1={arrow.y1}
			x2={arrow.x2}
			y2={arrow.y2}
			marker-end="url(#temple-kill-arrowhead)"
		/>
	</svg>
{/if}

<div
	class="callout"
	style="left:{placement?.x ?? 0}px;top:{placement?.y ?? 0}px;max-width:{maxWidth}px;{placement
		? ''
		: 'visibility:hidden;'}"
	bind:offsetWidth={boxWidth}
	bind:offsetHeight={boxHeight}
>
	<p class="title">
		{callout.title}{#if callout.forced}<span class="forced">({callout.forced})</span>{/if}
	</p>
	{#if callout.reason}
		<p class="reason">{callout.reason}</p>
	{/if}
</div>

<style>
	.arrow-layer {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		pointer-events: none;
		overflow: visible;
	}

	/* Wide enough to be followed at a glance over a lit game scene, and the same
	   purple the kill is printed in — the line and the words are one statement. */
	.arrow-layer line {
		stroke: var(--color-lab-purple);
		stroke-width: 3;
	}

	.callout {
		position: absolute;
		padding: 6px 10px;
		background: rgb(15 17 23 / 88%);
		border: 2px solid var(--color-lab-purple);
		border-radius: 6px;
		color: var(--color-lab-text);
		pointer-events: none;
	}

	/* The one line the player is meant to SEE. Bigger than anything the old
	   advice panel printed, because it is read at arm's length over a game. */
	.title {
		font-size: 17px;
		font-weight: 700;
		color: var(--color-lab-purple);
	}

	/* Inside the title, not under it — the same rule the Temple page follows:
	   the point is that the kill was not a ranked choice, and a note the eye
	   reads as a separate line is one the eye skips. */
	.forced {
		margin-left: 5px;
		font-size: 11px;
		font-weight: 400;
		color: var(--color-lab-yellow);
	}

	.reason {
		font-size: 12px;
		color: var(--color-lab-text-secondary);
	}
</style>
