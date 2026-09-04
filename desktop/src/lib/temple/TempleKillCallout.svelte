<script lang="ts">
	/**
	 * The kill, as a box beside the architect block it is about (POE-244,
	 * POE-248).
	 *
	 * The owner's ask, verbatim: *"it needs to be pointing to the thing, so I
	 * don't have to read, but see where to go"*. What this replaces is a panel
	 * of prose beside a redrawn copy of a board that is already on screen — the
	 * player had to read it, find the matching block in the game's own side
	 * panel, and only then click.
	 *
	 * # There is no arrow (POE-248)
	 *
	 * POE-244 drew a line with a head into the chosen block. Owner, 2026-09-04:
	 * no arrows anywhere. What points is the PLACEMENT — the box sits level with
	 * the block, immediately outside the panel it belongs to — and, once the
	 * panel closes, the cyan kill glyph on the room widget, which marks the same
	 * architect's own icon spot inside the room. An arrow could only ever point
	 * at something that is on screen, and the moment the player has to act is
	 * the moment it is not.
	 *
	 * # The rule it must not break
	 *
	 * **It covers no read region.** Every OCR crop and sampled patch reaches the
	 * slice as `layout.rois`, and `calloutPlacement` refuses any position that
	 * overlaps one — including the position it wanted. When nothing is free the
	 * box is NOT drawn: the game's own panel is on screen either way, and a
	 * callout that corrupts the module's next read is the worse trade. With the
	 * arrow gone, that is the only geometry rule left here — nothing this
	 * component draws crosses a rect any more.
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
	import { calloutPlacement } from './overlay-geometry';
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
		 *  boxes — the box is then placed against the panel crop instead, which
		 *  is the honest answer when there is no block to be level with. */
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
</script>

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
