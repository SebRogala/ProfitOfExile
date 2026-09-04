<script lang="ts">
	/**
	 * The current room's own diamond, drawn over the game (POE-244).
	 *
	 * The problem it exists for: the door is opened INSIDE the room, during the
	 * timed incursion, and by then the layout panel and the diamond the game
	 * draws next to the room name are both gone. Every surface keyed on the
	 * panel being on screen disappears at exactly the moment the player has to
	 * act on it, and a direction-only arrow was considered and rejected — a
	 * four-to-six-door room has nothing to orient an arrow by.
	 *
	 * So the widget carries the room geometry itself: the SAME isometric shape
	 * and orientation the panel draws, seals coloured the way the game colours
	 * them (green open, red shut), and the one the advisor wants opened drawn
	 * bigger and purple. Small and user-placed, because the player is the only
	 * one who knows what their screen is free of mid-incursion.
	 *
	 * # Every coordinate here is Rust's
	 *
	 * `overlay-geometry.ts`'s `diamondGeometry` maps `layout.diamond` — the
	 * outline from `markers::diamond_corners()` and the seal positions from
	 * `markers::seal_position()` — into a `viewBox` and a set of points. This
	 * file multiplies nothing, the same discipline `TempleLattice.svelte`
	 * follows with the board: the projection is a FIT against eight measured
	 * boards, and a second copy of it here would be a second answer that a
	 * re-fit leaves behind.
	 */
	import { diamondGeometry } from './overlay-geometry';
	import { EDGE_STATE_LABEL } from './view';
	import type { DiamondView, EdgeId, LayoutView } from './slice';

	let {
		diamond,
		layout,
		suggested,
		room,
		kill,
		warning
	}: {
		/** The room's shape and seals, as Rust published them. */
		diamond: DiamondView;
		/** The board the seal states are read off — `edgeState`'s input. */
		layout: LayoutView | null;
		/** The corridors the top recommendation wants opened. */
		suggested: readonly EdgeId[];
		/** The room's name, or null when neither source named it. */
		room: string | null;
		/** The kill, repeated by name — the callout's target block is off screen
		 *  once the player is inside the room, so this is the only place the
		 *  architect is still named. */
		kill: string | null;
		/** The one line that says not to act on these doors, or null. The grey
		 *  seals already say a corridor was not settled; this says the whole
		 *  read was not. See `view.ts`'s `doorWarning`. */
		warning: string | null;
	} = $props();

	const geometry = $derived(diamondGeometry(diamond, layout, suggested));
	/** The corridors to open, as the one line under the shape. Empty is a real
	 *  answer — R3 can recommend a kill with no door — and prints nothing. */
	const doorsLine = $derived(suggested.length === 0 ? null : suggested.join(', '));
</script>

<div class="door panel">
	{#if room}
		<p class="room">{room}</p>
	{/if}
	<svg
		class="shape"
		viewBox={geometry.viewBox}
		style="aspect-ratio:{geometry.aspectRatio}"
		role="img"
		aria-label="the room's doors"
	>
		<polygon
			class="outline"
			points={geometry.outline}
			vector-effect="non-scaling-stroke"
		/>
		{#each geometry.seals as seal (seal.edge)}
			<circle
				class="seal {seal.state}"
				class:suggested={seal.suggested}
				cx={seal.x}
				cy={seal.y}
				r={seal.radius}
				vector-effect="non-scaling-stroke"
			>
				<title>{seal.edge} — {EDGE_STATE_LABEL[seal.state]}</title>
			</circle>
		{/each}
	</svg>
	{#if kill}
		<p class="kill">{kill}</p>
	{/if}
	{#if doorsLine}
		<p class="doors">open {doorsLine}</p>
	{/if}
	{#if warning}
		<!-- Never dropped to make the widget smaller: it is the one line that
		     says the shape above it may be wrong, and it is on the only surface
		     still up once the player is inside the room. -->
		<p class="warn">{warning}</p>
	{/if}
</div>

<style>
	.panel {
		padding: 6px 8px;
		background: rgb(15 17 23 / 82%);
		border: 1px solid var(--color-lab-border);
		border-radius: 6px;
		color: var(--color-lab-text);
	}

	.door {
		display: flex;
		flex-direction: column;
		gap: 2px;
		align-items: center;
		/* The widget box decides the width — the host applies the registry's
		   shipped width as a max-width until the user resizes it, and a fixed
		   number here would ignore that resize. */
		width: 100%;
	}

	.room {
		font-size: 11px;
		color: var(--color-lab-text-secondary);
		text-align: center;
	}

	/* Full width, and the viewBox does all the scaling. The `aspect-ratio` is
	   set from `diamondGeometry` in the markup, not written here: it depends on
	   the fitted outline AND on the largest seal's margin, so a number in this
	   stylesheet would letterbox the shape the first time either moved. */
	.shape {
		width: 100%;
		height: auto;
	}

	.outline {
		fill: rgb(40 34 22 / 55%);
		stroke: var(--color-lab-yellow);
		stroke-width: 2;
	}

	.seal {
		stroke: var(--color-lab-bg);
		stroke-width: 1;
	}

	/* The game's own two colours for a seal, and one for neither. */
	.seal.open {
		fill: var(--color-lab-green);
	}

	.seal.closed {
		fill: var(--color-lab-red);
	}

	/* `uncertain` and `unresolved` are both "the read could not settle this
	   corridor", which must never render as the red the game uses for a door it
	   HAS settled shut. Same yellow-grey the board uses for an unread plate. */
	.seal.uncertain,
	.seal.unresolved {
		fill: var(--color-lab-text-muted);
	}

	/* The advisor's door. Bigger is `SEAL_RADIUS_SUGGESTED` in the geometry —
	   here it is only the colour and the ring, so the size claim lives in one
	   place and is testable. */
	.seal.suggested {
		fill: var(--color-lab-purple);
		stroke: var(--color-lab-text);
		stroke-width: 2;
	}

	.kill {
		font-size: 12px;
		font-weight: 700;
		color: var(--color-lab-purple);
		text-align: center;
	}

	.doors {
		font-size: 11px;
		color: var(--color-lab-text-secondary);
		text-align: center;
	}

	/* The same yellow the unresolved corridor and the unread plate use — one
	   colour for "this is not settled" across every temple surface. */
	.warn {
		font-size: 11px;
		line-height: 1.3;
		color: var(--color-lab-yellow);
		text-align: center;
	}
</style>
