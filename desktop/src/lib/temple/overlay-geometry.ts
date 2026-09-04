/**
 * Where the temple's two overlay surfaces go, and what shape the room widget
 * draws (POE-244, POE-248).
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
 * and an empty set means "do not place anything yet" rather than "everywhere is
 * free". That distinction is the whole guard: an empty obstacle list makes
 * every position legal, which is exactly the wrong answer when the reason it is
 * empty is that the conversion failed.
 *
 * All three placers state that themselves — [`calloutPlacement`],
 * [`bannerPlacement`] and [`doorDefaultPlacement`] — and the door's caller
 * repeats it. Leaving it to the callers was not enough: the banner's wanted
 * position is a function of the host alone, so it had no anchor to be null and
 * drew top-centre over the panel crop.
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
 * on it — a box beside the panel, level with the block it is about.
 * `avoidRects` then slides it off whatever it lands on, and the first thing it
 * lands on is the side panel's own OCR crop, which is what puts the box clear
 * of the panel rather than on top of the text it is naming.
 *
 * There is no longer a LINE from the box to the block (POE-248, owner: no
 * arrows anywhere). Being level with the block, and outside the panel it
 * belongs to, is the whole of what points at it; the kill glyph on the room
 * widget is the pointer that survives the panel closing.
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
 *
 * …and `null` for an EMPTY never-cover set, stated here rather than left to
 * fall out of a null anchor. An empty set means the layout is absent or the
 * scale factor has not resolved, which is "place nothing yet" and never "the
 * screen is free" — and today both of those also null the anchor, so the guard
 * is currently redundant. That redundancy is the point: the anchor going
 * non-null on an unresolved conversion is one refactor away, and a rule that
 * only holds by coincidence is a rule nothing enforces.
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
	if (obstacles.length === 0) return null;
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
 *
 * Null too for an EMPTY never-cover set — the same rule as its two siblings,
 * stated here as well as at the caller (`doorDefaults` in the temple overlay
 * route). Two statements of one rule is the intended shape: the caller's is
 * what keeps `WidgetHost` from being offered a default at all, and this one is
 * what makes the rule a property of the FUNCTION, so a second caller cannot
 * arrive without it. The registry fallback the caller then uses is NOT a
 * violation — see ADR-019: it is a shipped rectangle the user can see and move,
 * not a placement this module derived from the game.
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
	if (obstacles.length === 0) return null;
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
 *
 * Null too for an EMPTY never-cover set, and that one is the whole reason this
 * function exists rather than a `style="left:50%"`. The banner is the only
 * surface here that wants a position the HOST can supply on its own, so it is
 * the only one an empty obstacle list places instead of withholding: with no
 * layout, or with the scale factor unresolved, `avoidRects` found the wanted
 * rect clear because nothing was passed to it, and the banner drew top-centre
 * — straight over where the panel crop is about to be. The door default
 * (`doorDefaults` in the temple overlay route) refuses on the same input; so
 * does [`calloutPlacement`]. Empty means place nothing yet.
 */
export function bannerPlacement(input: {
	box: BoxSize;
	obstacles: readonly WidgetRect[];
	host: HostSize;
}): WidgetRect | null {
	const { box, obstacles, host } = input;
	if (box.w <= 0 || box.h <= 0) return null;
	if (obstacles.length === 0) return null;
	return avoidRects(
		{ x: host.width / 2 - box.w / 2, y: BANNER_TOP_CSS, w: box.w, h: box.h },
		obstacles,
		host
	);
}

// --------------------------------------------------------- the room shape --

/**
 * How big a seal is drawn, as a radius in the room's own units — which are
 * HALF-LONG-WALLS, so a same-row corridor's seal is at exactly 1.0 from the
 * centre and the four diagonals at 0.938 and 1.034
 * (`markers::ROOM_LONG_FRACTION`).
 *
 * The panel's own seals are 11-25 px across in a 200 px rect, i.e. a radius of
 * about 0.15 of the same unit. This is deliberately larger: the widget is
 * roughly a fifth of the panel's size and is glanced at mid-incursion, so
 * fidelity to the game's proportions loses to being able to see the thing. The
 * RATIO to [`SEAL_RADIUS_SUGGESTED`] is what has to be unmistakable.
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
	/** `open` / `unresolved` / `closed` — the SAME rule the board and the page
	 *  use (`view.ts`), never a second reading of `doors`. Which of the three
	 *  actually get DRAWN is [`sealVisible`]. */
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
 * The current room's shape, ready to draw.
 *
 * Every number here comes from Rust: `diamond.corners` is
 * `markers::diamond_corners()` — a rotated RECTANGLE since POE-248, measured
 * off the panel's gold outline — and `seal.pos` is `markers::seal_position()`,
 * the point where each corridor's own direction leaves that rectangle. Nothing
 * in this file re-derives a position: the whole reason the slice carries the
 * shape is that the alternative was a TypeScript copy of `AXIS_X` / `AXIS_Y`
 * that a re-fit would leave behind.
 *
 * The viewBox is the corners' bounding box grown by the largest seal radius, so
 * a suggested seal on a corner is not clipped by the edge of the element. It is
 * what makes the widget's aspect follow the shape rather than a number in a
 * stylesheet — which matters more now that the shape is not symmetric about
 * either screen axis.
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

/**
 * Whether a seal is drawn at all (POE-248).
 *
 * The owner's rule, and it is subtractive: the room widget shows the outline,
 * the OPEN doors in the game's own green, and the advisor's door bigger and
 * purple. A closed corridor and one the read could not settle are both DRAWN
 * NOWHERE — *"the closed/uncertain seals add chaos"* — which leaves the widget
 * saying only the two things a player acts on: where the walls are, and which
 * hole in them to buy.
 *
 * Not "hide everything but the suggestion": open is the game's own semantics
 * and the thing the suggestion is read against, so it stays.
 */
export function sealVisible(seal: PlacedSeal): boolean {
	return seal.suggested || seal.state === 'open';
}

/** Which of the two architect blocks a kill is. The wire strings, exactly. */
export type ArchitectKind = 'upgrade' | 'change';

/** The kill, as a mark inside the room. */
export interface KillGlyph {
	/** The icon spot, in `DiamondView.corners`' units — the same space the
	 *  outline and the seals are in, so one transform places all three. */
	position: { x: number; y: number };
	/** Which glyph to draw: an up-arrow for an upgrade, a two-way arrow for a
	 *  change. The component owns the paths; this owns which one. */
	kind: ArchitectKind;
}

/** The half of an offer that placing its glyph needs. */
interface GlyphOffer {
	kind: string;
	rect: CaptureRect | null;
}

/**
 * Where to mark the kill on the room widget, or null when there is nothing to
 * mark (POE-248).
 *
 * The kill used to be a LINE of text under the diamond — `KILL <architect> →
 * <room>`. Owner, after the first live session: a mark, not a sentence. The
 * game's own panel prints one architect icon in each half of the room diamond,
 * so marking the right half says which block to click, and the glyph's SHAPE
 * says which kind of kill it is — both at a glance, with nothing to read.
 *
 * # Which half, and why it is the RECT that decides
 *
 * The two spots are Rust's (`markers::architect_icons`, measured on the crops)
 * and arrive on the diamond as `topIcon` / `bottomIcon`; nothing here
 * re-derives one. What the measurement does NOT settle is which architect's
 * icon the panel draws in which half — the one board it was taken from had the
 * `upgrade` block on top, so "upgrade is the top-right one" and "the top block
 * is the top-right one" are indistinguishable on it.
 *
 * So the POSITIONAL reading is the one used: the panel prints its blocks top to
 * bottom (`panel::reading_order` sorts on the box top) and POE-243 publishes
 * each block's own OCR rect, so the chosen block's `rect` against its siblings'
 * is a fact about THIS panel rather than an assumption carried from another.
 * Two rects are needed for it to mean anything — one block read alone could be
 * either — and `kind` is the fallback below that, which is what a text-only
 * read (no boxes at all) gets.
 *
 * Null when the ranking named no architect, when the offer's `kind` is not one
 * of the two wire strings, or when the payload predates POE-248 and has no
 * spots — a glyph drawn at the origin would sit in the middle of the room and
 * claim a block nobody chose.
 */
export function killGlyph(
	diamond: DiamondView,
	offer: GlyphOffer | null,
	offers: readonly GlyphOffer[] = []
): KillGlyph | null {
	if (offer === null) return null;
	if (offer.kind !== 'upgrade' && offer.kind !== 'change') return null;
	const kind: ArchitectKind = offer.kind;
	// The rect when the read can order the blocks, the kind when it cannot.
	const top = topBlock(offer, offers) ?? kind === 'upgrade';
	const spot = top ? diamond.topIcon : diamond.bottomIcon;
	if (spot === null || spot === undefined) return null;
	return { position: { x: spot[0], y: spot[1] }, kind };
}

/**
 * Whether `offer` is the block the panel printed first, or null when the read
 * cannot say.
 *
 * Identity-free on purpose — it compares the chosen rect's top against the
 * smallest top among every offer that has one, rather than against "the other
 * offers", so a caller that rebuilt the list does not silently get `false`.
 */
function topBlock(offer: GlyphOffer, offers: readonly GlyphOffer[]): boolean | null {
	if (offer.rect === null) return null;
	const tops = offers.map((o) => o.rect).filter((r): r is CaptureRect => r !== null);
	// One rect orders nothing: a single block read could be either of the two
	// the panel drew, and POE-243's `forcedKill` is exactly that case.
	if (tops.length < 2) return null;
	return offer.rect[1] <= Math.min(...tops.map((r) => r[1]));
}
