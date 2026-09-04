<script lang="ts">
	/**
	 * The current room's own shape, drawn over the game (POE-244, reworked in
	 * POE-248).
	 *
	 * The problem it exists for: the door is opened INSIDE the room, during the
	 * timed incursion, and by then the layout panel and the diamond the game
	 * draws next to the room name are both gone. Every surface keyed on the
	 * panel being on screen disappears at exactly the moment the player has to
	 * act on it, and a direction-only arrow was considered and rejected — a
	 * four-to-six-door room has nothing to orient an arrow by.
	 *
	 * So the widget carries the room geometry itself: the SAME isometric
	 * rectangle the panel draws, at the same rotation, and on it only what the
	 * player acts on.
	 *
	 * # What it draws, and what it deliberately does not
	 *
	 * Owner, after the first live session (2026-09-04): the widget was too
	 * busy. So it is subtractive now.
	 *
	 * - the OUTLINE, always;
	 * - the OPEN doors, green, the way the game colours them;
	 * - the advisor's door, purple and bigger;
	 * - the KILL, as a cyan glyph on the chosen block's own icon spot inside
	 *   the room — an up-arrow for an `upgrade`, a two-way arrow for a
	 *   `change`. Which HALF is keyed on the block's OCR rect, not on its kind;
	 *   see `killGlyph`.
	 *
	 * Gone with POE-248: the red closed seals and the grey unsettled ones
	 * (*"they add chaos"*), the `KILL <architect> → <room>` line and the `open
	 * <edge>` line. The kill is a MARK now, not a sentence: the game's own panel
	 * puts one architect icon in each half of the room diamond, so marking the
	 * right half says which block to click and the glyph's shape says which kind
	 * of kill it is, without anything to read. The callout still spells it out
	 * in words while the panel is up.
	 *
	 * The one line that stayed is `warning` — see its prop.
	 *
	 * # Every coordinate here is Rust's
	 *
	 * `overlay-geometry.ts`'s `diamondGeometry` maps `layout.diamond` — the
	 * outline from `markers::diamond_corners()`, the seal positions from
	 * `markers::seal_position()` and the icon spots from
	 * `markers::architect_icons()` — into a `viewBox` and a set of points. This
	 * file multiplies nothing, the same discipline `TempleLattice.svelte`
	 * follows with the board: the shape is a FIT against measured boards, and a
	 * second copy of it here would be a second answer that a re-fit leaves
	 * behind.
	 */
	import { diamondGeometry, killGlyph, sealVisible, type ArchitectKind } from './overlay-geometry';
	import { EDGE_STATE_LABEL } from './view';
	import type { DiamondView, EdgeId, LayoutView, OfferView } from './slice';

	let {
		diamond,
		layout,
		suggested,
		room,
		offer,
		offers,
		warning
	}: {
		/** The room's shape, its seals and the two icon spots, as Rust
		 *  published them. */
		diamond: DiamondView;
		/** The board the seal states are read off — `edgeState`'s input. */
		layout: LayoutView | null;
		/** The corridors the top recommendation wants opened. */
		suggested: readonly EdgeId[];
		/** The room's name, or null when neither source named it. */
		room: string | null;
		/** The architect block the advisor chose, or null when the ranking named
		 *  none. Its `rect` is what places the glyph — the half of the room the
		 *  game drew this block's icon in — and its `kind` is what shapes it. */
		offer: OfferView | null;
		/** Every block this read parsed, so the chosen one's rect has siblings
		 *  to be above or below. One block alone orders nothing. */
		offers: readonly OfferView[];
		/** The one line that says not to act on these doors, or null. See
		 *  `view.ts`'s `doorWarning`. Now that an unsettled corridor is drawn
		 *  NOWHERE, this is the only thing left saying the shape may be wrong,
		 *  which is why it is still here. */
		warning: string | null;
	} = $props();

	const geometry = $derived(diamondGeometry(diamond, layout, suggested));
	const drawn = $derived(geometry.seals.filter(sealVisible));
	const glyph = $derived(killGlyph(diamond, offer, offers));

	/**
	 * The two kill marks, in the room's own units, centred on the origin.
	 *
	 * Inline paths and not an image: an overlay window loads no assets, and the
	 * game's own icon art would have to be cropped out of the capture to be
	 * used — worth doing as polish, not now.
	 *
	 * `GLYPH_HALF` is the extent either side of the spot: about a fifth of the
	 * room's short wall, which is the size the panel draws its own icons at.
	 */
	const GLYPH_HALF = 0.18;
	/**
	 * How far above and below the centre the change glyph's two shafts sit.
	 *
	 * MEASURED against the shipped widget: at 190 px the viewBox is ~3.72 units
	 * wide, i.e. ~51 px per unit. The first draft put the shafts at ±0.072 —
	 * 7.4 px apart, against 6 px of dark halo — which left about a pixel of
	 * daylight and read as one bar. ±0.12 is 12.3 px apart against a 4 px halo,
	 * so 8 px of gap survives, and the two arrows stay two arrows.
	 */
	const SHAFT = 0.12;
	const GLYPH: Record<ArchitectKind, string> = {
		// An up-arrow: the kill lifts this room a tier.
		upgrade: `M 0 ${GLYPH_HALF} L 0 ${-GLYPH_HALF} M ${-GLYPH_HALF * 0.62} ${-GLYPH_HALF * 0.3} L 0 ${-GLYPH_HALF} L ${GLYPH_HALF * 0.62} ${-GLYPH_HALF * 0.3}`,
		// Two opposed arrows: the kill swaps this room for another line. The
		// upper one points right, the lower one left.
		change: `M ${-GLYPH_HALF * 0.9} ${-SHAFT} L ${GLYPH_HALF * 0.9} ${-SHAFT} M ${GLYPH_HALF * 0.4} ${-SHAFT - GLYPH_HALF * 0.45} L ${GLYPH_HALF * 0.9} ${-SHAFT} L ${GLYPH_HALF * 0.4} ${-SHAFT + GLYPH_HALF * 0.45} M ${GLYPH_HALF * 0.9} ${SHAFT} L ${-GLYPH_HALF * 0.9} ${SHAFT} M ${-GLYPH_HALF * 0.4} ${SHAFT - GLYPH_HALF * 0.45} L ${-GLYPH_HALF * 0.9} ${SHAFT} L ${-GLYPH_HALF * 0.4} ${SHAFT + GLYPH_HALF * 0.45}`
	};
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
		{#each drawn as seal (seal.edge)}
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
		{#if glyph}
			<!-- Drawn twice: a dark stroke underneath so the cyan reads over the
			     game's own gold and dark-red panel art, then the glyph itself.
			     Both are non-scaling, so the widget can be dragged to any size
			     and the mark keeps the weight it was designed at. -->
			<g transform="translate({glyph.position.x} {glyph.position.y})">
				<path
					class="kill-shadow"
					d={GLYPH[glyph.kind]}
					vector-effect="non-scaling-stroke"
				/>
				<path class="kill" d={GLYPH[glyph.kind]} vector-effect="non-scaling-stroke">
					<title>kill the {glyph.kind} architect</title>
				</path>
			</g>
		{/if}
	</svg>
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

	/* The game's own colour for an open door, and the only seal colour left:
	   `sealVisible` draws nothing at all for a closed or unsettled corridor
	   (POE-248), so there is no red and no grey to tell this apart from. */
	.seal.open {
		fill: var(--color-lab-green);
	}

	/* The advisor's door. Bigger is `SEAL_RADIUS_SUGGESTED` in the geometry —
	   here it is only the colour and the ring, so the size claim lives in one
	   place and is testable. */
	.seal.suggested {
		fill: var(--color-lab-purple);
		stroke: var(--color-lab-text);
		stroke-width: 2;
	}

	/* Cyan, and deliberately not red: red is the colour the GAME uses for a
	   closed seal, and a mark that borrows it would read as one more door. */
	.kill {
		fill: none;
		stroke: var(--color-lab-cyan);
		stroke-width: 3;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	.kill-shadow {
		fill: none;
		stroke: rgb(4 6 10 / 90%);
		/* One pixel of dark either side of the 3 px glyph — enough to separate
		   the cyan from the game's gold and dark-red panel art, and thin enough
		   that the change glyph's two shafts keep visible daylight between them
		   at the shipped widget width. 6 px closed that gap. */
		stroke-width: 4;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	/* The same yellow the unread plate uses — one colour for "this is not
	   settled" across every temple surface. */
	.warn {
		font-size: 11px;
		line-height: 1.3;
		color: var(--color-lab-yellow);
		text-align: center;
	}
</style>
