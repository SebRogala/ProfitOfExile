/**
 * Where the temple's two overlay surfaces go, and what shape the door diamond
 * is (POE-244).
 *
 * The sibling of `view.ts`: that file words the advice, this one places it. Both
 * exist because a `.svelte` file has no unit-test harness in this app and an
 * overlay window has no devtools, so a box twenty pixels into an OCR crop looks
 * exactly like a box that is fine.
 *
 * # The two units, and the ONE conversion
 *
 * The slice speaks CAPTURE pixels — whole-game-monitor physical px
 * (`slice.ts`). The overlay window IS that monitor (POE-237), so capture px are
 * window-relative physical px with no origin to subtract, and the only step to
 * CSS px is dividing by the window's own `scaleFactor()`. [`captureToCss`] is
 * that step and it is the only place it happens.
 *
 * It FAILS CLOSED at an unresolved scale factor, the way `cssRect` and
 * `physicalHotRect` do: substituting 1 does not produce "no answer", it produces
 * a confident wrong one, and on a 150 % display every never-cover rect would be
 * a third of the way toward the origin — which is to say the callout would be
 * placed clear of rectangles that are not where it thinks they are.
 *
 * # Nothing here decides what is readable
 *
 * The never-cover set is `layout.rois`, published by Rust from the five
 * functions that own those rectangles. This file converts and filters it. A
 * rect computed here from `origin` and `scale` would be a second answer to
 * where the module is looking, and the two would drift in silence.
 */
import { avoidRects } from '$lib/overlay/widgets/widget-avoid';
import type { HostSize, WidgetRect } from '$lib/overlay/widgets/widget-geometry';
import type { CaptureRect, DiamondView, EdgeId, LayoutView, SlotId } from './slice';
import { edgeState, type EdgeState } from './view';

/** Gap between a placed box and the thing it is placed against, CSS px. */
export const CALLOUT_GAP_CSS = 16;

/**
 * How far short of the architect block the arrow STOPS, CSS px.
 *
 * The arrowhead is drawn at the line's end, so a line that ran all the way to
 * the block's edge put a filled triangle on top of the first glyphs of the
 * block — over OCR input, which is the one thing this whole file exists to
 * prevent. The head is 8 px and its tip reaches half a pixel past the line end,
 * so a 10 px standoff lands the tip about 9 px clear of the text while still
 * unmistakably touching the block.
 */
export const ARROW_STANDOFF_CSS = 10;

/** How far below the top of the host the leave-the-map banner wants to sit. */
export const BANNER_TOP_CSS = 16;

/**
 * A capture-px rectangle in CSS px, or null when the scale factor has not
 * resolved. See the file header for why null rather than a guess.
 */
export function captureToCss(rect: CaptureRect, scaleFactor: number): WidgetRect | null {
	if (!(scaleFactor > 0) || !Number.isFinite(scaleFactor)) return null;
	const [x, y, w, h] = rect;
	return { x: x / scaleFactor, y: y / scaleFactor, w: w / scaleFactor, h: h / scaleFactor };
}

/**
 * Every rectangle the module reads, in CSS px — the set nothing may cover.
 *
 * Empty for a layout that is absent or a scale factor that has not resolved,
 * and the callers treat an empty set as "do not place anything yet" rather than
 * as "everywhere is free". That distinction is the whole guard: an empty
 * obstacle list makes every position legal, which is exactly the wrong answer
 * when the reason it is empty is that the conversion failed.
 */
export function neverCoverRects(
	layout: LayoutView | null,
	scaleFactor: number
): WidgetRect[] {
	if (layout === null) return [];
	const out: WidgetRect[] = [];
	// `?? []` for a snapshot from a build before POE-244. `normaliseTemple`
	// already fills it, and this is the second belt for a caller that built a
	// LayoutView by hand.
	for (const roi of layout.rois ?? []) {
		const rect = captureToCss(roi.rect, scaleFactor);
		if (rect) out.push(rect);
	}
	return out;
}

/** The first published rect of one kind, in CSS px, or null. */
export function roiRect(
	layout: LayoutView | null,
	kind: string,
	scaleFactor: number
): WidgetRect | null {
	const roi = layout?.rois?.find((entry) => entry.kind === kind);
	return roi ? captureToCss(roi.rect, scaleFactor) : null;
}

/** A box's size, in CSS px, as measured out of the DOM. */
export interface BoxSize {
	w: number;
	h: number;
}

/**
 * Where the kill callout goes.
 *
 * Wanted position: immediately LEFT of the architect block, vertically centred
 * on it — which is the owner's mock, a box beside the panel with an arrow into
 * the block. `avoidRects` then slides it off whatever it lands on, and the
 * first thing it lands on is the side panel's own OCR crop, which is what puts
 * the box clear of the panel rather than on top of the text the arrow points
 * at.
 *
 * With no block rect (a text-only read, or a read whose OCR carried no boxes)
 * the panel crop is the anchor instead: the box goes to its left, at its top.
 * With neither — no layout at all — there is nothing to be beside and nothing
 * to avoid, and the answer is null rather than a corner, because a callout
 * placed against nothing is a callout placed over whatever is there.
 *
 * `null` also when the avoidance finds no free position. The box is then not
 * drawn: the board is on screen either way, and a callout that costs the module
 * its read is the worse trade.
 */
export function calloutPlacement(input: {
	/** The chosen block's rect in CSS px, or null. */
	target: WidgetRect | null;
	/** The side panel's OCR crop in CSS px, or null. */
	panel: WidgetRect | null;
	/** What the box measures. */
	box: BoxSize;
	/** The never-cover set, CSS px. */
	obstacles: readonly WidgetRect[];
	host: HostSize;
}): WidgetRect | null {
	const { target, panel, box, obstacles, host } = input;
	if (box.w <= 0 || box.h <= 0) return null;
	const anchor = target ?? panel;
	if (anchor === null) return null;
	const wanted: WidgetRect = {
		x: anchor.x - CALLOUT_GAP_CSS - box.w,
		// Centred on the block when there is one; on a whole panel crop that
		// would put the box halfway down the screen, so an anchor that is only
		// the panel is aligned to its top instead.
		y: target ? target.y + target.h / 2 - box.h / 2 : anchor.y,
		w: box.w,
		h: box.h
	};
	return avoidRects(wanted, obstacles, host);
}

/** A straight arrow, CSS px, from a box edge to a block edge. */
export interface CalloutArrow {
	x1: number;
	y1: number;
	x2: number;
	y2: number;
}

/** The four edge midpoints of a rectangle, in N/E/S/W order. */
function edgeMidpoints(rect: WidgetRect): [number, number][] {
	return [
		[rect.x + rect.w / 2, rect.y],
		[rect.x + rect.w, rect.y + rect.h / 2],
		[rect.x + rect.w / 2, rect.y + rect.h],
		[rect.x, rect.y + rect.h / 2]
	];
}

/** The centre of a rectangle. */
function centreOf(rect: WidgetRect): [number, number] {
	return [rect.x + rect.w / 2, rect.y + rect.h / 2];
}

/** Whichever of `points` is nearest `[px, py]`. Ties keep the earlier one, so
 *  the N/E/S/W order above is the tie-break and the result is deterministic. */
function nearest(points: [number, number][], px: number, py: number): [number, number] {
	let best = points[0];
	let bestDistance = Infinity;
	for (const [x, y] of points) {
		const distance = (x - px) ** 2 + (y - py) ** 2;
		if (distance < bestDistance) {
			best = [x, y];
			bestDistance = distance;
		}
	}
	return best;
}

/**
 * The arrow from the callout to the block it is about.
 *
 * Edge MIDPOINTS at both ends rather than the geometrically shortest segment
 * between the two rectangles: the shortest segment between two boxes that
 * nearly line up is a stub a few pixels long against a corner, which reads as a
 * smudge. A midpoint-to-midpoint line always leaves the box from a side and
 * always arrives at the middle of a side, which is the shape that reads as
 * "this one".
 *
 * The target end is chosen first — nearest the box's centre — and the box end
 * then follows it, so the line does not start on the far side of the box from
 * where it is going.
 */
export function calloutArrow(box: WidgetRect, target: WidgetRect): CalloutArrow {
	const [bcx, bcy] = centreOf(box);
	const [tx, ty] = nearest(edgeMidpoints(target), bcx, bcy);
	const [x1, y1] = nearest(edgeMidpoints(box), tx, ty);
	// Stop short of the block: the head is drawn AT the line's end, and a line
	// that reached the edge put a filled triangle over the block's first
	// glyphs. Pulled back along the line itself, so the direction is unchanged.
	const dx = tx - x1;
	const dy = ty - y1;
	const length = Math.hypot(dx, dy);
	// A degenerate line (the box's chosen edge midpoint IS the target's) has no
	// direction to pull back along; leave it, since there is nothing to draw.
	const pull = length > ARROW_STANDOFF_CSS ? ARROW_STANDOFF_CSS / length : 0;
	return { x1, y1, x2: tx - dx * pull, y2: ty - dy * pull };
}

/**
 * Where the door diamond sits until the user drags it.
 *
 * Below the panel's crop and lined up with the panel's own diamond, which is
 * where the player's eye already is while the panel is open — and clear of
 * every read region, because the shipped position is the one nobody has vetted
 * on this particular screen. `avoidRects` is what guarantees the second half;
 * the wanted position only decides which free spot is nearest.
 *
 * Null when there is no board to place against or nothing free, and the caller
 * then falls back to the registry's shipped CSS default. That fallback is a
 * fixed number that cannot know where this screen's panel is, which is exactly
 * why it is the fallback and not the rule.
 */
export function doorDefaultPlacement(input: {
	/** The panel's OCR crop in CSS px, or null. */
	panel: WidgetRect | null;
	/** The panel's diamond crop in CSS px, or null. */
	diamond: WidgetRect | null;
	box: BoxSize;
	obstacles: readonly WidgetRect[];
	host: HostSize;
}): WidgetRect | null {
	const { panel, diamond, box, obstacles, host } = input;
	if (box.w <= 0 || box.h <= 0) return null;
	if (panel === null) return null;
	const wanted: WidgetRect = {
		// The diamond's own column when it was published, the panel's left edge
		// otherwise — never the screen's, which on a wide monitor is a long way
		// from anything the player is looking at.
		x: diamond ? diamond.x : panel.x,
		y: panel.y + panel.h + CALLOUT_GAP_CSS,
		w: box.w,
		h: box.h
	};
	return avoidRects(wanted, obstacles, host);
}

/**
 * Where the leave-the-map banner goes.
 *
 * Top-centre of the host by preference — it is about the MAP rather than about
 * any one block, so it has nothing to be beside — and then through the same
 * avoidance as everything else. That last part is not decoration: on the
 * committed 1920x1080 fixture the panel's crop starts at x 1131, and a banner
 * ~420-490 px wide centred at x 960 reaches 1200, straight over the side
 * panel's OCR region. Centred-and-pinned was wrong on the one screen size the
 * repository actually has a frame of.
 *
 * Null when the banner has not measured itself yet, or when nothing is free.
 */
export function bannerPlacement(input: {
	box: BoxSize;
	obstacles: readonly WidgetRect[];
	host: HostSize;
}): WidgetRect | null {
	const { box, obstacles, host } = input;
	if (box.w <= 0 || box.h <= 0) return null;
	return avoidRects(
		{ x: host.width / 2 - box.w / 2, y: BANNER_TOP_CSS, w: box.w, h: box.h },
		obstacles,
		host
	);
}

// ------------------------------------------------------- the door diamond --

/**
 * How big a seal is drawn, as a radius in the diamond's own units — which are
 * SEAL RING radii, so every seal centre is at exactly 1.0 from the origin
 * (`markers::SEAL_RING_FRACTION`).
 *
 * The panel's own seals are 11-25 px across in a 200 px rect, i.e. a radius of
 * about 0.15 ring radii. This is deliberately larger: the widget is roughly a
 * fifth of the panel's size and is glanced at mid-incursion, so fidelity to the
 * game's proportions loses to being able to see the thing. The RATIO to
 * [`SEAL_RADIUS_SUGGESTED`] is what has to be unmistakable.
 */
export const SEAL_RADIUS = 0.2;
/** The advisor's door, drawn to be found rather than read. */
export const SEAL_RADIUS_SUGGESTED = 0.34;

/** One seal, placed and classified for drawing. */
export interface PlacedSeal {
	edge: EdgeId;
	neighbour: SlotId;
	x: number;
	y: number;
	radius: number;
	/** `open` / `uncertain` / `unresolved` / `closed` — the SAME rule the board
	 *  and the page use (`view.ts`), never a second reading of `doors`. */
	state: EdgeState;
	/** Whether the top recommendation says to open this one. */
	suggested: boolean;
}

/** The diamond as an SVG can draw it. */
export interface DiamondGeometry {
	/** `"x,y x,y x,y x,y"` for a `<polygon points=…>`. */
	outline: string;
	seals: PlacedSeal[];
	/** `"minX minY width height"` — the whole scaling, the way
	 *  `latticeViewBox` does it for the board: coordinates stay in the
	 *  projection's own units and the viewBox maps them onto the element. */
	viewBox: string;
	/** The viewBox's own width ÷ height, for the element's `aspect-ratio`.
	 *
	 *  Derived rather than written into the CSS as a number: the box depends on
	 *  the fitted outline AND on the largest seal's margin, so a constant in a
	 *  stylesheet would be a second answer that silently letterboxes the shape
	 *  the first time either moves. */
	aspectRatio: number;
}

/**
 * The current room's diamond, ready to draw.
 *
 * Every number here comes from Rust: `diamond.corners` is
 * `markers::diamond_corners()` and `seal.pos` is `markers::seal_position()`,
 * which is the fitted projection the door READER measures angles with. Nothing
 * in this file re-derives a position — the whole reason the slice carries the
 * shape is that the alternative was a TypeScript copy of `AXIS_X` / `AXIS_Y`
 * that a re-fit would leave behind.
 *
 * The viewBox is the corners' bounding box grown by the largest seal radius, so
 * a suggested seal on a corner is not clipped by the edge of the element.
 */
export function diamondGeometry(
	diamond: DiamondView,
	layout: LayoutView | null,
	suggested: readonly EdgeId[]
): DiamondGeometry {
	const xs = diamond.corners.map(([x]) => x);
	const ys = diamond.corners.map(([, y]) => y);
	const margin = SEAL_RADIUS_SUGGESTED * 1.35;
	const minX = Math.min(...xs) - margin;
	const minY = Math.min(...ys) - margin;
	const width = Math.max(...xs) + margin - minX;
	const height = Math.max(...ys) + margin - minY;
	return {
		aspectRatio: width / height,
		outline: diamond.corners.map(([x, y]) => `${x},${y}`).join(' '),
		seals: diamond.seals.map((seal) => {
			const isSuggested = suggested.includes(seal.edge);
			return {
				edge: seal.edge,
				neighbour: seal.neighbour,
				x: seal.pos[0],
				y: seal.pos[1],
				radius: isSuggested ? SEAL_RADIUS_SUGGESTED : SEAL_RADIUS,
				state: edgeState(seal.edge, layout),
				suggested: isSuggested
			};
		}),
		viewBox: `${minX} ${minY} ${width} ${height}`
	};
}
