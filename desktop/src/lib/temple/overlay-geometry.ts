/**
 * Where the temple's overlay surfaces go, and what shape the room widget draws
 * (POE-244, POE-248, POE-249, POE-261).
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
import type {
	CaptureRect,
	DiamondView,
	EdgeId,
	ExitLabelView,
	LayoutView,
	SlotId
} from './slice';
import { edgeState, type EdgeState } from './view';

/** Gap between a placed box and the thing it is placed against, CSS px. */
export const CALLOUT_GAP_CSS = 16;

/** Gap between two stacked offer boxes, CSS px (POE-249). Smaller than
 *  [`CALLOUT_GAP_CSS`]: that one separates a surface from the GAME, and this
 *  one separates two boxes of one column, which have to read as a pair. */
export const STACK_GAP_CSS = 8;

/** How far right of the column a box sits when its block is on the RIGHT of
 *  the side panel's diamond, CSS px (owner, 2026-09-06).
 *
 *  The game prints the panel's first block top-RIGHT of its diamond and the
 *  second bottom-LEFT, so a plain column mirrors the panel's ORDER but not its
 *  shape. The owner redrew the top box 175 px to the right on a 1:1 crop of
 *  the 1920×1080 screen and left the lower box where it was — the diagonal the
 *  panel itself draws — and 175 is that measurement. Less than a box is wide
 *  (260), so the two still overlap in x and the stacking floor still applies. */
export const STACK_STAGGER_CSS = 175;

/** How far below the top of the host a top-centred surface wants to sit — the
 *  leave-the-map banner, and since POE-249 the waiting notice's shipped
 *  default. The name is the banner's because it was the first. */
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
 * Every placer states that itself — [`offerStackPlacement`],
 * [`bannerPlacement`], [`doorDefaultPlacement`] and
 * [`waitingDefaultPlacement`] — and the two default-offering callers repeat
 * it. Leaving it to the callers was not enough:
 * the banner's wanted position is a function of the host alone, so it had no
 * anchor to be null and drew top-centre over the panel crop. The waiting
 * notice's default wants that same position, which is why it states the rule
 * too.
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
 * Where the two offer boxes go (POE-249).
 *
 * The owner's ask: two boxes on the **left margin of the temple sheet**,
 * stacked to mirror the side panel's own block order, the advisor's pick framed
 * in cyan — *"that frame IS the pointer; no arrows anywhere"*. So this places a
 * COLUMN, not two independent boxes: one x for the stack, and box `i` level
 * with the architect block it is about.
 *
 * # Why the far-left margin
 *
 * The game draws the incursion sheet across the middle of the screen with its
 * side panel on the right, and the module reads all of it — 42 rectangles. The
 * left margin is the one stretch of the screen that is wide, empty and outside
 * every one of them while the sheet is up: the player's buff bar is at the
 * top-left and the sheet is drawn over it, so the space is free exactly when
 * these boxes exist. `boardLeft` is the leftmost published read region, and the
 * stack sits a `CALLOUT_GAP_CSS` clear of it.
 *
 * MEASURED, on the committed 1920x1080 frame (Entrance centre (960, 713), scale
 * 1.0): `boardLeft` is **556**, the left edge of the D0 plate's crop
 * (960 − 318 − 86). Nothing published reaches further left — the leftmost
 * corridor patch is at 681, and the panel, its diamond and the budget line are
 * all on the right half — so a 260 px box wants x = 556 − 16 − 260 = **280**.
 * That is about 280 px of clear margin to the left of the board and roughly 900
 * px from the blocks the boxes describe, which is what "the left margin" means
 * on this screen: proximity is not the pointer, the cyan frame and the
 * architect's own name in the headline are.
 *
 * On a narrower capture the margin can simply not be there, and the answer is
 * NOT "no box". `boardLeft - CALLOUT_GAP_CSS - widest` goes negative, the clamp
 * below pins the column's x to 8, and `avoidRects` then does what it always
 * does: it takes that position if it is clear, and otherwise moves the box to
 * the nearest free one ANYWHERE in the host. So on a cramped frame these stop
 * being a left-margin column and land wherever there is room — acceptable
 * because proximity was never the pointer: the cyan frame and the architect's
 * own name in the headline are.
 *
 * `null` — the box NOT drawn (ADR-019) — is what comes back when no position in
 * the host is clear of every read region, which on a real board takes a box
 * bigger than the screen. That refusal is still the rule, and it is per BOX:
 * one that cannot be placed is dropped while the other is still drawn.
 *
 * # The side rule
 *
 * Since 2026-09-06 the column is a DIAGONAL: a box whose block sits right of
 * the panel crop's centre — the first block, as the game draws it — is placed
 * [`STACK_STAGGER_CSS`] right of the column, and one whose block sits left of
 * it takes the column. That is the panel's own shape, mirrored, the way the
 * level-with-the-block rule below mirrors its order. The side is READ off the
 * block rect against the panel crop, never assumed from the box's index: a
 * read with no block rect, or no panel crop, has no side and takes the column.
 *
 * # The stacking rule
 *
 * Box `i` wants to be level with block `i`, which is what makes the column
 * mirror the panel without any arrow between them. Two blocks closer together
 * than the upper box is tall would put the boxes on top of each other, so each
 * one is also pushed below the one above it (`STACK_GAP_CSS`), and each goes
 * through `avoidRects` against the read regions AND the boxes already placed —
 * the second box avoids the first, or it is not drawn.
 *
 * A box with no size yet is `null` and does not block the others: it has not
 * measured itself, and treating a zero-size rect as an obstacle would place the
 * box below it at a position that jumps on the next frame.
 *
 * Null everywhere on an EMPTY never-cover set, the rule all four of this file's
 * placers state themselves: empty means the layout is absent or the scale
 * factor has not resolved, which is "place nothing yet" and never "the screen
 * is free". Here it is not even a redundancy — `boardLeft` is a minimum over
 * that set, and over an empty one there is no left edge to be beside.
 */
export function offerStackPlacement(input: {
	/** Each offer's own block rect in CSS px, `blocks[i]` for box `i`, or null
	 *  for a read that carried no boxes. */
	blocks: readonly (WidgetRect | null)[];
	/** The side panel's OCR crop in CSS px — the top the first box falls back
	 *  to when the read carried no block rect. */
	panel: WidgetRect | null;
	/** What each box measures, in the same order. */
	boxes: readonly BoxSize[];
	/** The never-cover set, CSS px. */
	obstacles: readonly WidgetRect[];
	host: HostSize;
}): (WidgetRect | null)[] {
	const { blocks, panel, boxes, obstacles, host } = input;
	if (obstacles.length === 0) return boxes.map(() => null);
	const boardLeft = Math.min(...obstacles.map((rect) => rect.x));
	// ONE column for the stack, off the WIDEST box, so the two boxes start from
	// the same line and the side rule below is the only thing that moves one.
	// Clamped to 8 rather than allowed off screen — `avoidRects` would clamp it
	// to 0 anyway, and a box flush against the screen edge reads as clipped.
	const widest = Math.max(0, ...boxes.map((box) => box.w));
	const column = Math.max(8, boardLeft - CALLOUT_GAP_CSS - widest);
	const centre = panel === null ? null : panel.x + panel.w / 2;
	const placed: (WidgetRect | null)[] = [];
	// The last box that actually GOT a position — what the next one stacks
	// under. A box that was not measured or could not be placed is not one, so
	// it neither pushes the next box down nor blocks it.
	let previous: WidgetRect | null = null;
	for (let i = 0; i < boxes.length; i++) {
		const box = boxes[i];
		if (box.w <= 0 || box.h <= 0) {
			placed.push(null);
			continue;
		}
		const block = blocks[i] ?? null;
		// The block's side of the diamond, read against the panel crop's own
		// centre — see "The side rule" above. No block or no panel is no side.
		const right = block !== null && centre !== null && block.x + block.w / 2 > centre;
		const x = right ? column + STACK_STAGGER_CSS : column;
		// Level with this box's own block. With no block rect the first box
		// takes the panel crop's top — there is no block to be level with — and
		// a later one takes the bottom of the box above it, which is the only
		// order the panel itself states.
		const floor = previous === null ? null : previous.y + previous.h + STACK_GAP_CSS;
		const wantedY = block ? block.y : (floor ?? panel?.y ?? BANNER_TOP_CSS);
		const y = floor === null ? wantedY : Math.max(wantedY, floor);
		const at = avoidRects(
			{ x, y, w: box.w, h: box.h },
			// The boxes already placed are obstacles too: two boxes level with
			// two blocks a hair apart would otherwise overlap after the
			// avoidance moved one of them.
			[...obstacles, ...placed.filter((rect): rect is WidgetRect => rect !== null)],
			host
		);
		placed.push(at);
		if (at !== null) previous = at;
	}
	return placed;
}

/**
 * What the panel's own DIAGONAL has room for, CSS px (POE-260 design §1).
 *
 * MEASURED: on the committed 1920x1080 frame the staggered strip — the column
 * plus [`STACK_STAGGER_CSS`] — is clear only down to y 449, where plate C0's
 * crop begins, and the design sizes against a first architect block at y 133.
 * 449 - 133 is the whole of it.
 *
 * This is an ASSUMPTION about the board and not a ceiling on the box: a box
 * taller than this still draws every line it has, and `offerStackPlacement`
 * gives up the diagonal for it (slides it back onto the column) rather than
 * dropping content. So it is not what [`offersCompact`] budgets against —
 * [`FULL_BOX_MAX_CSS`] is — and the two are kept apart because they answer
 * different questions. POE-277 additionally holds FULL_BOX_MAX_CSS AT OR UNDER
 * this budget as a design requirement — `overlay-geometry.test.ts` asserts it
 * — so a row added to the component is a decision about what the box drops,
 * not a number to raise here.
 */
export const DIAGONAL_BUDGET_CSS = 316;

/**
 * The tallest a FULL offer box can actually be, CSS px (POE-277).
 *
 * Summed from `TempleOfferBoxes.svelte`'s own fixed row heights — every row in
 * the full form is a `height` or a `line-height` with a stated margin, which is
 * what makes a box's geometry a function of its SHAPE — for the worst case
 * this new wording can produce: a `market` or `partial` box on the advisor's
 * pick, so a 2 px frame, with the two-line header, a sale row and paired
 * unique/vial row, a ladder, a recipe, a temple mod block with five glyphs and
 * the warning age line.
 *
 *     border 2x2 4 + padding 2x2 4 + header (24 + 15) 39
 *     + drivers (2 + 2x39 + 1x1) 81 + ladder (1 + 23) 24
 *     + recipe (2 + 11, 1 + 39) 53
 *     + mod (2 + 20, 1 + 26) 49 + age (2 + 13) 15 = 269
 *
 * The 316 px diagonal budget therefore has 47 px of spare height. The fresh
 * form loses the 15 px age row; this constant describes the WARN form, where
 * the market note is present. A mod driver is removed from the drop rows
 * before the component receives the box, so `fold` cannot add a fourth row in
 * this form. `note` is not in the sum: instrumental and overridden rooms have
 * no drop rows, so their explanatory line replaces the ladder/row shape rather
 * than extending this maximum.
 *
 * Not measured from the DOM because this app has no DOM harness for a
 * `.svelte` file; `overlay-geometry.test.ts` restates the row table beside the
 * constant so a row added to the component without a number here fails.
 */
export const FULL_BOX_MAX_CSS = 269;

/** What two FULL boxes and the gap between them need, CSS px —
 * `269 * 2 + STACK_GAP_CSS` = 546 for the current 269 px full form. This is
 * the clearance [`offersCompact`] demands before it lets the pair render full.
 * It uses the WORST case rather than the design's typical one, because the box
 * that gets clamped for want of it is the LOWER one, and a clamped box lands
 * on the box above it and is then moved or dropped entirely. */
export const FULL_PAIR_CSS = FULL_BOX_MAX_CSS * 2 + STACK_GAP_CSS;

/**
 * More driver rows than folding one line can honestly hide (POE-260 design
 * §7.1).
 *
 * At four rows the fold hides one and shows three, which is a summary; at six
 * it hides three and shows three, which is not. The cap sits one row below that
 * crossing, so five — hiding two under three — is the last shape that still
 * reads as a summary.
 *
 * **Unreachable as the wording stands**, and kept anyway. `view.ts`'s
 * `ROW_KINDS` has four entries — sale, unique drop, vial drop, mod item — and a
 * room publishes at most one driver of each, so `driverCount` cannot exceed 4
 * today and this trigger never fires. It fires the day a fifth `ROW_KIND` is
 * added, which is the day the fold would start hiding as much as it shows; a
 * collapse rule that only appeared then would be a rule written under the
 * pressure of the feature that broke it.
 */
export const MAX_FOLDABLE_DRIVERS = 5;

/**
 * Whether the pair of offer boxes draws in its COMPACT form (POE-260).
 *
 * Both tests read inputs that exist BEFORE the boxes render. Neither may read a
 * measured height: the component measures and then places, so a fit test on the
 * measured height would change the height it was testing and oscillate for as
 * long as the board is up.
 *
 * Two triggers, and they are different kinds of "does not fit":
 *
 * 1. **Too many drivers to fold.** More than [`MAX_FOLDABLE_DRIVERS`] rows on
 *    ANY box and the pair goes compact — the fold line would be hiding more
 *    than the rows it sits under show.
 * 2. **Not enough screen under the first block.** The stack starts level with
 *    block 0 (or, with no block rect, at the panel crop's top, or at
 *    [`BANNER_TOP_CSS`]) and grows downward, so what is left of the host below
 *    that point is the whole budget. Under [`FULL_PAIR_CSS`] the pair cannot
 *    both be full.
 *
 * **Together, never one each.** Two boxes in different forms read as two
 * different kinds of answer, and the whole point of the pair is that the player
 * is comparing them.
 *
 * Explicitly NOT a trigger: whether the staggered box clears the first plate.
 * `offerStackPlacement` already answers that by sliding the box back onto the
 * column, and its answer keeps every line of content — the diagonal is what
 * gives there, not the explanation.
 */
export function offersCompact(input: {
	/** Every box's driver-row count, in box order (`OfferBox.driverCount`). */
	driverCounts: readonly number[];
	/** Each offer's block rect in CSS px, `blocks[i]` for box `i`. */
	blocks: readonly (WidgetRect | null)[];
	/** The side panel's OCR crop in CSS px. */
	panel: WidgetRect | null;
	host: HostSize;
}): boolean {
	const { driverCounts, blocks, panel, host } = input;
	if (driverCounts.some((count) => count > MAX_FOLDABLE_DRIVERS)) return true;
	const top = blocks[0]?.y ?? panel?.y ?? BANNER_TOP_CSS;
	return host.height - top < FULL_PAIR_CSS;
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
 * function exists rather than a `style="left:50%"`. The banner WAS THE FIRST
 * surface here that wants a position the HOST can supply on its own — since
 * POE-249 [`waitingDefaultPlacement`] wants the same one — and a surface like
 * that is what an empty obstacle list PLACES instead of withholding: with no
 * layout, or with the scale factor unresolved, `avoidRects` found the wanted
 * rect clear because nothing was passed to it, and the banner drew top-centre
 * — straight over where the panel crop is about to be. That is the violation
 * ADR-019 was written from, and it is why the rule is stated at each placer
 * rather than left to an anchor: [`offerStackPlacement`] and
 * [`doorDefaultPlacement`] refuse on the same input, and so do the route's two
 * default-offering callers (`doorDefaults`, `waitingDefaults`). Empty means
 * place nothing yet.
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

/**
 * Where the "waiting for the temple panel" notice SHIPS (POE-249).
 *
 * This is a shipped DEFAULT and not a placement. `temple.waiting` is a
 * PLACEABLE widget: the route offers this answer through `defaultsFor`, the
 * host uses it exactly where `spec.defaults` would have been, and a user who
 * has dragged the notice anywhere at all never reaches this function again —
 * `placementFor` consults a default only when there is no stored row. The
 * registry's fixed rectangle is what applies when this answers null, which is
 * the ADR-019 carve-out for a user-owned rectangle: a fixed number visible the
 * moment it draws and movable in one drag, not a placer's silent output.
 *
 * Wanted position: the top centre of the host, the same place the leave-the-map
 * banner wants, because the notice is about the CYCLE rather than about any
 * rectangle the game drew — there is nothing on screen for it to be beside, and
 * the sheet it is waiting for is not up yet. Screen-centre, which is what the
 * owner asked for, is where the plates are (`temple.waiting`'s registry
 * comment has the measured numbers), so the eye-level answer would be a box on
 * the module's own OCR crops.
 *
 * Null on an unmeasured box and null on an EMPTY never-cover set — the same
 * rule its three siblings state, and stated here for the same reason: an empty
 * set is a layout that is absent or a scale factor that has not resolved, which
 * is "place nothing yet" and never "the screen is free". This is the placer
 * that rule bit first: the wanted position is a function of the HOST alone, so
 * an empty obstacle list would come back from `avoidRects` clear (see
 * [`bannerPlacement`], the violation ADR-019 was written from).
 *
 * The arithmetic is identical to [`bannerPlacement`]'s today and is
 * deliberately not shared with it. They answer different questions — where one
 * anchored line goes on every frame, versus where an unconfigured widget ships
 * — and either one can move without the other: the banner following the panel
 * downward would not move the notice, and the notice moving to the corner would
 * not move the banner. A helper over both would invent that coupling.
 */
export function waitingDefaultPlacement(input: {
	/** The registry's shipped box for the widget, CSS px. */
	box: BoxSize;
	/** The never-cover set, CSS px. */
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
/**
 * The door a SECOND Stone of Passage would buy — between the two, on purpose.
 *
 * Size is half of what says which seal is which: the conditional door is more
 * than a wall (a plain seal) and less than the instruction (the suggestion),
 * and at a glance the ORDER of the three radii is what reads. Colour carries
 * the other half — see the component's `.seal.secondary`, which is the same
 * purple at half opacity, the shared *"faint = the alternative"* rule the
 * non-chosen kill glyph also follows.
 */
export const SEAL_RADIUS_SECONDARY = 0.27;

/**
 * What a seal is, for the widget's three sizes and three fills.
 *
 * `plain` is a corridor with no advice on it — drawn in its own state's colour
 * (green open, red closed) at [`SEAL_RADIUS`]. `suggested` is the door to open
 * now; `secondary` is the faint one — what a second stone would buy, or the
 * convenience door when the move opens nothing (`faintDoor()` in `view.ts`).
 */
export type SealKind = 'plain' | 'secondary' | 'suggested';

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
	/** Whether the advisor has anything to say about this corridor, and what.
	 *  One field rather than two booleans: the three are mutually exclusive and
	 *  a seal that was both would have two radii. */
	kind: SealKind;
}

/** The radius each kind is drawn at. Total over [`SealKind`], so a kind added
 *  without a size fails `npm run check` rather than drawing at `undefined`. */
const SEAL_RADII: Record<SealKind, number> = {
	plain: SEAL_RADIUS,
	secondary: SEAL_RADIUS_SECONDARY,
	suggested: SEAL_RADIUS_SUGGESTED
};

/**
 * Where the recommended exit's NAME is drawn, as percentages of the shape's own
 * box (POE-261).
 *
 * Percentages and not pixels, and out of the widget's flow: the label is pinned
 * inside the `<svg>`'s own box, so it moves with the seal when the widget is
 * resized and it costs the widget NO height. That second half is what keeps
 * ADR-019 answered without re-measuring anything — the room widget's shipped
 * rectangle is what [`doorDefaultPlacement`] clears the read regions with, and
 * a label that added a line under the shape would grow the drawn box past the
 * rectangle that clearance was computed for.
 *
 * The text runs from the seal toward the room's INTERIOR, never outward: pinned
 * to the far side of the box, so `inset` + `width` is exactly 100 and the
 * label's span is the box from the mark's inner rim to that far edge. It
 * therefore cannot leave the widget's footprint whatever the name is — which is
 * the whole reason the placement is stated in these terms and not as a width in
 * characters.
 */
export interface ExitLabelPlacement {
	/** Which side of the box the label is pinned to — the CSS property the
	 *  widget sets. Always the side OPPOSITE the seal. */
	side: 'left' | 'right';
	/** How far that pinned edge sits from its side of the box, as a percentage
	 *  of the box's width. It clears the seal by one [`SEAL_RADIUS_SUGGESTED`],
	 *  so the text starts at the mark's inner rim rather than under it. */
	inset: number;
	/** The seal's own height in the box, as a percentage. The label is centred
	 *  on it, so the name sits level with the mark it belongs to. */
	top: number;
	/** How much of the box's width is left between the seal and the far edge —
	 *  the label's `max-width`, as a percentage. */
	width: number;
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
	/** Where the recommended exit's name goes, or null when there is no name to
	 *  draw — see [`ExitLabelPlacement`]. Null too when the named door is not a
	 *  seal this shape drew SUGGESTED, which is what makes *"only the solid
	 *  purple exit is labelled"* a property of the geometry rather than of the
	 *  markup. */
	exitLabel: ExitLabelPlacement | null;
}

/**
 * The name's place on the shape, or null (POE-261).
 *
 * Two gates, and both are refusals rather than fallbacks:
 *
 * 1. **No name.** Rust published none — the move opens no door, or the plate
 *    behind it did not resolve (`slice.rs::recommended_exit` owns every reason,
 *    and `view.ts::recommendedExit` is the reader). Nothing here invents one.
 * 2. **The named door is not the solid purple seal.** The label belongs to the
 *    seal the ranking made `suggested`; a name landing on a plain corridor or
 *    on the faint second-stone seal would be an instruction the advisor never
 *    gave. Matching on the door rather than trusting the order is what makes
 *    ONE label the shape's own guarantee.
 */
function exitLabelPlacement(
	seals: readonly PlacedSeal[],
	frame: { x: number; y: number; w: number; h: number },
	exit: ExitLabelView | null
): ExitLabelPlacement | null {
	if (exit === null) return null;
	const seal = seals.find((s) => s.edge === exit.door && s.kind === 'suggested');
	if (seal === undefined) return null;
	// The frame is the corners' bounding box grown by the SAME margin on all
	// four sides, so its 50 % is the room's own centre and this comparison is
	// "which half of the room is this door on".
	const x = ((seal.x - frame.x) / frame.w) * 100;
	const side = x > 50 ? 'right' : 'left';
	// The text starts at the mark's INNER RIM, not at its centre: a label pinned
	// to the seal's own point begins under the seal, and the biggest circle the
	// widget draws is the one it would begin under. One suggested radius of
	// clearance is the smallest offset that cannot do that, and it is taken from
	// [`SEAL_RADIUS_SUGGESTED`] rather than written as a number so a resized seal
	// moves the text with it.
	//
	// It buys the clearance out of the label's own room — `inset` gains it and
	// `width` gives it up — so `inset` + `width` is still exactly 100 and the
	// label still ends at the far edge. The footprint claim is unchanged; what
	// changed is where the span STARTS.
	const clear = (SEAL_RADIUS_SUGGESTED / frame.w) * 100;
	const near = side === 'right' ? 100 - x : x;
	return {
		side,
		inset: near + clear,
		top: ((seal.y - frame.y) / frame.h) * 100,
		width: 100 - near - clear
	};
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
	suggested: readonly EdgeId[],
	secondary: EdgeId | null = null,
	exit: ExitLabelView | null = null
): DiamondGeometry {
	const xs = diamond.corners.map(([x]) => x);
	const ys = diamond.corners.map(([, y]) => y);
	const margin = SEAL_RADIUS_SUGGESTED * 1.35;
	const minX = Math.min(...xs) - margin;
	const minY = Math.min(...ys) - margin;
	const width = Math.max(...xs) + margin - minX;
	const height = Math.max(...ys) + margin - minY;
	const seals: PlacedSeal[] = diamond.seals.map((seal) => {
		// `suggested` wins a corridor that is somehow in both: it is the door
		// to open NOW, and drawing it as the conditional one would tell the
		// player to wait for a stone they do not need.
		const kind: SealKind = suggested.includes(seal.edge)
			? 'suggested'
			: seal.edge === secondary
				? 'secondary'
				: 'plain';
		return {
			edge: seal.edge,
			neighbour: seal.neighbour,
			x: seal.pos[0],
			y: seal.pos[1],
			radius: SEAL_RADII[kind],
			state: edgeState(seal.edge, layout),
			kind
		};
	});
	return {
		aspectRatio: width / height,
		outline: diamond.corners.map(([x, y]) => `${x},${y}`).join(' '),
		seals,
		viewBox: `${minX} ${minY} ${width} ${height}`,
		exitLabel: exitLabelPlacement(seals, { x: minX, y: minY, w: width, h: height }, exit)
	};
}

/**
 * Whether a seal is drawn at all (POE-248).
 *
 * One corridor is hidden and one only: the one the read could not SETTLE.
 * Everything else is drawn in the game's own semantics — green where the game
 * draws a passage, red where it draws a wall — because the widget replaces the
 * panel's diamond during the incursion and a room with half its walls missing
 * is not the room the player is standing in.
 *
 * # The correction this encodes
 *
 * The first cut of POE-248 hid the closed seals too (*"the closed/uncertain
 * seals add chaos"*). The owner checked it in game and reversed that half the
 * same day: what was chaotic was the GREY — a dot for a corridor nobody can act
 * on, on a board where the red ones are half of what tells you where you are.
 * So closed is back, in the game's red and at [`SEAL_RADIUS`], and unsettled
 * stays away with `doorWarning()` saying so in words instead.
 *
 * Advice always draws, whatever the state: a suggestion or a conditional door
 * on a corridor nothing settled is still the advisor's answer for it, and
 * hiding it would leave the widget silent about the only thing it is for.
 */
export function sealVisible(seal: PlacedSeal): boolean {
	return seal.kind !== 'plain' || seal.state !== 'unresolved';
}

/** Which of the two architect blocks a kill is. The wire strings, exactly. */
export type ArchitectKind = 'upgrade' | 'change';

/** One kill mark inside the room. */
export interface KillGlyph {
	/** The icon spot, in `DiamondView.corners`' units — the same space the
	 *  outline and the seals are in, so one transform places all three. */
	position: { x: number; y: number };
	/** Which glyph to draw: an up-arrow for an upgrade, a two-way arrow for a
	 *  change. The component owns the paths; this owns which one. */
	kind: ArchitectKind;
	/** Whether this is the block the advisor CHOSE. The other one is drawn
	 *  faint — see [`killGlyphs`]. */
	chosen: boolean;
}

/** The half of an offer that placing its glyph needs. */
interface GlyphOffer {
	/** Position in the panel, as `PanelView.offers` publishes it. The identity
	 *  the "other" block is found by — see [`killGlyphs`]. */
	index: number;
	kind: string;
	rect: CaptureRect | null;
}

/**
 * Where to mark the two kills on the room widget (POE-248).
 *
 * The kill used to be a LINE of text under the diamond — `KILL <architect> →
 * <room>`. Owner, after the first live session: a mark, not a sentence. The
 * game's own panel prints one architect icon in each half of the room diamond,
 * so marking the right half says which block to click, and the glyph's SHAPE
 * says which kind of kill it is — both at a glance, with nothing to read.
 *
 * # Both blocks, and why the second one is faint
 *
 * Owner again, after the in-game check: the widget draws the OTHER architect
 * too, at its own spot with its own kind, at a quarter opacity the component
 * owns. Two marks orient the player — one mark alone says which half without
 * saying what the halves are — and the faintness is what keeps it from reading
 * as a second instruction. It is the same visual rule the conditional door seal
 * follows (`SEAL_RADIUS_SECONDARY`, half-opacity purple): **faint is the
 * alternative**, across every mark on this widget.
 *
 * The chosen glyph comes first in the returned array, so a caller that draws
 * only `[0]` still draws the right one.
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
 * read (no boxes at all) gets. The other block takes the spot the chosen one
 * did not, whichever way that was decided.
 *
 * # What returns nothing
 *
 * EMPTY when the ranking named no architect, when the offer's `kind` is not one
 * of the two wire strings, or when the payload predates POE-248 and has no
 * spots — a glyph drawn at the origin would sit in the middle of the room and
 * claim a block nobody chose.
 *
 * ONE entry — the chosen block alone, no faint mark — when the read carried no
 * second block to draw. That is POE-243's `forcedKill` shape: the panel prints
 * two and the OCR got one, and inventing a mark for the block nobody read would
 * put a kill on screen that was never on the panel. Also one entry when the
 * second block's `kind` is not a wire string, for the same reason the chosen
 * one's must be.
 */
export function killGlyphs(
	diamond: DiamondView,
	offer: GlyphOffer | null,
	offers: readonly GlyphOffer[] = []
): KillGlyph[] {
	if (offer === null) return [];
	const kind = architectKind(offer);
	if (kind === null) return [];
	// The rect when the read can order the blocks, the kind when it cannot.
	const top = topBlock(offer, offers) ?? kind === 'upgrade';
	const chosen = spotGlyph(diamond, top, kind, true);
	if (chosen === null) return [];

	// The other block, by its panel position: the chosen one's `index` is the
	// identity `chosenOffer()` looked it up by. Exactly one sibling, or none —
	// a list that somehow holds two others cannot say which is THE other, and a
	// mark on the wrong half is worse than no mark.
	const others = offers.filter((other) => other.index !== offer.index);
	if (others.length !== 1) return [chosen];
	const otherKind = architectKind(others[0]);
	if (otherKind === null) return [chosen];
	const other = spotGlyph(diamond, !top, otherKind, false);
	return other === null ? [chosen] : [chosen, other];
}

/** The offer's kind as one of the two wire strings, or null. */
function architectKind(offer: GlyphOffer): ArchitectKind | null {
	return offer.kind === 'upgrade' || offer.kind === 'change' ? offer.kind : null;
}

/** One glyph on one of the diamond's two published spots, or null when the
 *  payload predates them. */
function spotGlyph(
	diamond: DiamondView,
	top: boolean,
	kind: ArchitectKind,
	chosen: boolean
): KillGlyph | null {
	const spot = top ? diamond.topIcon : diamond.bottomIcon;
	if (spot === null || spot === undefined) return null;
	return { position: { x: spot[0], y: spot[1] }, kind, chosen };
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
