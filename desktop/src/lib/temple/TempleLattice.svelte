<script lang="ts">
	/**
	 * The temple's 13-slot board, drawn (POE-171).
	 *
	 * Shared by `TemplePage` and the `/overlay/temple` window, which is the
	 * whole reason it is a component: the two must agree about which corridor is
	 * open and which could not be read, and two copies of that mapping would not
	 * stay in step.
	 *
	 * Every coordinate comes from `$lib/temple/view` in the reader's own
	 * reference pixels, and the `viewBox` does the scaling — nothing here
	 * multiplies a coordinate, so there is no second scale factor to keep in
	 * step with `lattice.rs`.
	 */
	import {
		EDGE_STATE_LABEL,
		PLATE_H,
		PLATE_W,
		edgeState,
		latticeEdges,
		latticePoints,
		latticeViewBox,
		plateGlyph
	} from './view';
	import type { EdgeId, LayoutView } from './slice';

	let {
		layout = null,
		/** Corridors the recommendation says to open — drawn on top of their state. */
		highlightDoors = [],
		/** Drop the plate names and tiers. The overlay has no room for them. */
		compact = false
	}: {
		layout?: LayoutView | null;
		highlightDoors?: EdgeId[];
		compact?: boolean;
	} = $props();

	const box = latticeViewBox();
	const edges = latticeEdges();
	const points = latticePoints();

	/** The read for one slot, or null when the board has not been read. */
	const bySlot = $derived(new Map((layout?.slots ?? []).map((s) => [s.slot, s])));

	/** Plate half-sizes, so the rects are written once rather than per plate. */
	const halfW = PLATE_W / 2;
	const halfH = PLATE_H / 2;

	/**
	 * The name to draw on a plate.
	 *
	 * An unread plate says so rather than being left blank: a blank plate reads
	 * as "empty room", which is a different and wrong claim (POE-171 — Unknown
	 * is junk to the advisor, not an absence).
	 */
	function plateName(slot: string): string {
		const read = bySlot.get(slot as never);
		if (!read) return '';
		return read.name ?? 'unread';
	}
</script>

<svg
	class="lattice"
	class:compact
	viewBox="{box.minX} {box.minY} {box.width} {box.height}"
	role="img"
	aria-label="Temple of Atzoatl layout, 13 rooms"
>
	{#each edges as edge (edge.id)}
		{@const state = edgeState(edge.id, layout)}
		{@const recommended = highlightDoors.includes(edge.id)}
		<line
			class="edge edge-{state}"
			class:recommended
			x1={edge.x1}
			y1={edge.y1}
			x2={edge.x2}
			y2={edge.y2}
		>
			<title
				>{edge.id}: {EDGE_STATE_LABEL[state]}{recommended ? ' — recommended' : ''}</title
			>
		</line>
	{/each}

	{#each points as point (point.slot)}
		{@const read = bySlot.get(point.slot)}
		<g
			class="plate"
			class:current={read?.current === true}
			class:unread={read !== undefined && !read.known}
			class:empty={read === undefined}
		>
			<rect
				x={point.x - halfW}
				y={point.y - halfH}
				width={PLATE_W}
				height={PLATE_H}
				rx="6"
			/>
			<title>{point.slot}{read?.name ? ` — ${read.name}` : ''}</title>
			{#if compact}
				<!-- Names and tier lines do not fit out here, but "this plate was
				     never read" must survive the shrink: the glyph is the one
				     character that keeps an unread plate distinguishable. -->
				<text class="slot-key" x={point.x} y={point.y - 4}>{point.slot}</text>
				<text class="plate-glyph" x={point.x} y={point.y + 30}>{plateGlyph(read)}</text>
			{:else}
				<text class="slot-key" x={point.x} y={point.y - 12}>{point.slot}</text>
				<text class="plate-name" x={point.x} y={point.y + 8}>{plateName(point.slot)}</text>
				{#if read?.known && read.tier > 0}
					<text class="plate-tier" x={point.x} y={point.y + 26}>tier {read.tier}</text>
				{/if}
			{/if}
		</g>
	{/each}
</svg>

<style>
	.lattice {
		width: 100%;
		height: auto;
		display: block;
	}

	.edge {
		stroke: var(--color-lab-border);
		stroke-width: 6;
	}

	/* A settled door is the only corridor drawn solid and bright. */
	.edge-open {
		stroke: var(--color-lab-green);
	}

	/* Nothing settled it. Marked loudly: "could not be read" must never look
	   like "closed", which is the whole honesty guard the slice publishes it for. */
	.edge-unresolved {
		stroke: var(--color-lab-yellow);
		stroke-dasharray: 4 8;
		stroke-width: 8;
	}

	.edge-closed {
		stroke: var(--color-lab-border);
		stroke-width: 3;
	}

	.edge.recommended {
		stroke: var(--color-lab-purple);
		stroke-width: 12;
	}

	.plate rect {
		fill: var(--color-lab-surface);
		stroke: var(--color-lab-border);
		stroke-width: 2;
	}

	.plate.empty rect,
	.plate.unread rect {
		fill: none;
		stroke-dasharray: 8 6;
	}

	.plate.current rect {
		stroke: var(--color-lab-blue);
		stroke-width: 5;
	}

	text {
		text-anchor: middle;
		fill: var(--color-lab-text);
		font-size: 18px;
	}

	.slot-key {
		fill: var(--color-lab-text-muted);
		font-size: 16px;
		letter-spacing: 0.08em;
	}

	.compact .slot-key {
		font-size: 26px;
	}

	.plate-name {
		font-size: 19px;
	}

	.plate.unread .plate-name {
		fill: var(--color-lab-yellow);
		font-style: italic;
	}

	.compact .plate-glyph {
		font-size: 34px;
		font-weight: 700;
	}

	/* Same yellow the unresolved corridor uses, and for the same reason: a read
	   that did not happen is marked, never quietly drawn as a normal plate. */
	.plate.unread .plate-glyph {
		fill: var(--color-lab-yellow);
	}

	.plate-tier {
		fill: var(--color-lab-text-secondary);
		font-size: 15px;
	}
</style>
