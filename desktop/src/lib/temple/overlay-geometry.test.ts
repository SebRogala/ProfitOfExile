/**
 * Where the temple's overlay surfaces go (POE-244).
 *
 * The obstacle geometry below is hand-built and round — a panel crop at
 * 480..680 × 100..400 with the panel's own diamond inside it — so every expected
 * position is arithmetic a reader can redo from the numbers in the comment.
 * That matters more here than in most files: the failure this places against is
 * a box a few pixels into an OCR crop, which on screen is indistinguishable from
 * a box that is fine, and the module then reads its own overlay back as game
 * pixels.
 */
import { describe, expect, it } from 'vitest';
import {
	BANNER_TOP_CSS,
	CALLOUT_GAP_CSS,
	SEAL_RADIUS,
	SEAL_RADIUS_SECONDARY,
	SEAL_RADIUS_SUGGESTED,
	STACK_GAP_CSS,
	bannerPlacement,
	captureToCss,
	diamondGeometry,
	doorDefaultPlacement,
	killGlyphs,
	neverCoverRects,
	offerStackPlacement,
	roiRect,
	sealVisible,
	waitingDefaultPlacement
} from './overlay-geometry';
import { latticeEdges, latticePoints } from './view';
import { rectIsClear } from '$lib/overlay/widgets/widget-avoid';
import type { WidgetRect } from '$lib/overlay/widgets/widget-geometry';
import { widgetsFor } from '$lib/overlay/widgets/widget-registry';
import { TEMPLE_WINDOW_LABEL } from '$lib/overlay/manager';
import type { CaptureRect, DiamondView, LayoutView, RoiView } from './slice';

const HOST = { width: 1920, height: 1080 };

/** The side panel's crop, in CSS px at scale factor 1. */
const PANEL = { x: 480, y: 100, w: 200, h: 300 };
/** The panel's own diamond, inside it. */
const DIAMOND = { x: 600, y: 120, w: 100, h: 100 };
/**
 * The side panel's OCR crop on the committed 1920x1080 frame: the real extent
 * of `panel_rect((960, 713), 1.0)` — x0 `origin.x + 171` = 1131, y0 4, x1 1675,
 * y1 458 — and not the panel's border box.
 *
 * One declaration for the two suites that use it: `offerStackPlacement`'s,
 * where it is the first box's fallback top and one of the 42 read regions, and
 * `waitingDefaultPlacement`'s, where it is the crop the shipped notice has to
 * clear. Two copies of one measured rectangle would let one suite go on
 * passing against a number the other had corrected.
 */
const COMMITTED_PANEL = { x: 1131, y: 4, w: 544, h: 454 };

function layout(rois: RoiView[]): LayoutView {
	return {
		slots: [],
		doors: [],
		uncertain: [],
		unresolvedIncident: [],
		markerError: null,
		current: 'C1',
		scale: 1,
		ncc: 0.95,
		confidence: 'high',
		origin: [0, 0],
		centres: [
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0],
			[0, 0]
		],
		rois,
		diamond: null
	};
}

const ROIS: RoiView[] = [
	{ kind: 'panel', of: null, rect: [480, 100, 200, 300] },
	{ kind: 'diamond', of: null, rect: [600, 120, 100, 100] },
	{ kind: 'plate', of: 'C1', rect: [200, 700, 120, 60] }
];

describe('captureToCss', () => {
	it('divides the capture rectangle by the window scale factor', () => {
		// Capture px are the game monitor's PHYSICAL px and the window covers
		// that monitor, so the only step is the scale factor — a 150 % display
		// draws a 300-px-wide crop as 200 CSS px.
		expect(captureToCss([600, 300, 300, 150], 1.5)).toEqual({ x: 400, y: 200, w: 200, h: 100 });
	});

	it('is the identity at scale factor 1', () => {
		expect(captureToCss([10, 20, 30, 40], 1)).toEqual({ x: 10, y: 20, w: 30, h: 40 });
	});

	it('refuses rather than assuming 1 when the scale factor has not resolved', () => {
		// The window reports 0 until it answers. Substituting 1 would not give
		// "no answer" — it would give a confident wrong one, and every read
		// region would be measured a third of the way toward the origin on a
		// 150 % display.
		expect(captureToCss([10, 20, 30, 40], 0)).toBeNull();
		expect(captureToCss([10, 20, 30, 40], Number.NaN)).toBeNull();
		expect(captureToCss([10, 20, 30, 40], -1)).toBeNull();
	});
});

describe('neverCoverRects', () => {
	it('converts every published read region, keeping their order', () => {
		expect(neverCoverRects(layout(ROIS), 1)).toEqual([
			{ x: 480, y: 100, w: 200, h: 300 },
			{ x: 600, y: 120, w: 100, h: 100 },
			{ x: 200, y: 700, w: 120, h: 60 }
		]);
	});

	it('scales every one of them, not just the first', () => {
		expect(neverCoverRects(layout(ROIS), 2)[2]).toEqual({ x: 100, y: 350, w: 60, h: 30 });
	});

	it('is empty with no layout, which the callers read as "place nothing yet"', () => {
		expect(neverCoverRects(null, 1)).toEqual([]);
	});

	it('is empty while the scale factor is unresolved', () => {
		// The dangerous case: an empty obstacle list makes every position legal,
		// so the callers must treat empty as "not ready" rather than "the screen
		// is free". This pins the half that produces it.
		expect(neverCoverRects(layout(ROIS), 0)).toEqual([]);
	});
});

describe('roiRect', () => {
	it('finds a region by the kind Rust published it under', () => {
		expect(roiRect(layout(ROIS), 'diamond', 1)).toEqual(DIAMOND);
	});

	it('is null for a kind this read did not publish', () => {
		expect(roiRect(layout(ROIS), 'remaining', 1)).toBeNull();
	});
});

describe('offerStackPlacement', () => {
	/**
	 * The read regions of the COMMITTED 1920x1080 frame, in CSS px at scale
	 * factor 1 — all 42 of them, derived rather than typed in.
	 *
	 * The three panel-side rects are `run::panel_rect`, `run::diamond_rect` and
	 * `run::remaining_rect` at Entrance centre (960, 713): the same panel crop
	 * `waitingDefaultPlacement`'s suite below measures against, and the budget
	 * line `slice.rs`'s own sample prints as `[810, 771, 300, 46]`. The 13
	 * plates and the 26 corridor patches come off `view.ts`'s lattice — which is
	 * `lattice.rs`'s table, pinned by its own tests on both sides — with
	 * `panel::name_strip` unioned with `panel::numeral_box` for a plate
	 * (`cx - 86`, `cy - 35`, 172 x 77 at scale 1, the halves TRUNCATED the way
	 * `Lattice::plate_half` truncates) and `lattice::PATCH_HALF` for a patch.
	 *
	 * It is a reconstruction and not a capture, which is why nothing here
	 * asserts a rect of it: what the cases below use is its LEFT EDGE, and that
	 * edge is one number a reader can check by hand — 960 - 318 - 86.
	 */
	function committedRegions(): WidgetRect[] {
		const [ox, oy] = [960, 713];
		const out: WidgetRect[] = [
			{ x: 1131, y: 4, w: 544, h: 454 },
			{ x: 1313, y: 117, w: 200, h: 200 },
			{ x: 810, y: 771, w: 300, h: 46 }
		];
		for (const point of latticePoints()) {
			out.push({ x: ox + point.x - 86, y: oy + point.y - 35, w: 172, h: 77 });
		}
		for (const edge of latticeEdges()) {
			// Absolute first, then halved, because Rust halves the two CAPTURE
			// coordinates and floor division of a negative offset would land a
			// pixel off.
			const mx = Math.floor((ox + edge.x1 + (ox + edge.x2)) / 2);
			const my = Math.floor((oy + edge.y1 + (oy + edge.y2)) / 2);
			out.push({ x: mx - 14, y: my - 14, w: 28, h: 28 });
		}
		return out;
	}

	/** Both architect blocks as the panel drew them, inside the panel's crop. */
	const BLOCKS = [
		{ x: 1140, y: 150, w: 280, h: 43 },
		{ x: 1140, y: 260, w: 280, h: 43 }
	];
	/** What one box measures once it has rendered its four lines. */
	const BOX = { w: 260, h: 96 };

	it('stacks both boxes in the left margin, each level with its own block', () => {
		const obstacles = committedRegions();
		expect(obstacles).toHaveLength(42);
		// The measurement the placement is built on: the leftmost thing the
		// module reads on this frame is the D0 plate's crop at 960 - 318 - 86.
		// Every corridor patch is further right (the leftmost is at 681) and the
		// panel, its diamond and the budget line are on the right half.
		expect(Math.min(...obstacles.map((rect) => rect.x))).toBe(556);
		// So the column wants x = 556 - 16 - 260 = 280, and each box takes its
		// own block's y — 110 px apart, further than the 96 + 8 the stack needs,
		// so neither is pushed.
		expect(
			offerStackPlacement({
				blocks: BLOCKS,
				panel: COMMITTED_PANEL,
				boxes: [BOX, BOX],
				obstacles,
				host: HOST
			})
		).toEqual([
			{ x: 280, y: 150, ...BOX },
			{ x: 280, y: 260, ...BOX }
		]);
	});

	it('pushes the lower box below the upper when the blocks are closer than a box is tall', () => {
		// Blocks 20 px apart: level-with-the-block would draw the second box
		// across the first. The stack wins, at 150 + 96 + STACK_GAP_CSS.
		expect(
			offerStackPlacement({
				blocks: [BLOCKS[0], { ...BLOCKS[1], y: 170 }],
				panel: COMMITTED_PANEL,
				boxes: [BOX, BOX],
				obstacles: committedRegions(),
				host: HOST
			})
		).toEqual([
			{ x: 280, y: 150, ...BOX },
			{ x: 280, y: 150 + BOX.h + STACK_GAP_CSS, ...BOX }
		]);
	});

	it('falls back to the top of the panel crop when the read carried no block rects', () => {
		// A text-only read still gets its boxes: there is no block to be level
		// with, so the first goes at the panel crop's top and the second stacks
		// under it. The whole COLUMN survives a read with no boxes, which is
		// what the callout's own panel fallback did for its single box.
		expect(
			offerStackPlacement({
				blocks: [null, null],
				panel: COMMITTED_PANEL,
				boxes: [BOX, BOX],
				obstacles: committedRegions(),
				host: HOST
			})
		).toEqual([
			{ x: 280, y: COMMITTED_PANEL.y, ...BOX },
			{ x: 280, y: COMMITTED_PANEL.y + BOX.h + STACK_GAP_CSS, ...BOX }
		]);
	});

	it('ships the first box at the banner top when the read carried neither a block nor a panel crop', () => {
		// The last rung of the fallback chain, and the only one nothing else
		// pins: no block to be level with, no panel crop to take the top of, and
		// no box above to stack under. `BANNER_TOP_CSS` is the same top edge the
		// leave-the-map banner and the waiting notice ship at, so the boxes
		// appear where the overlay's other top-of-screen surfaces do rather than
		// flush against the window's own edge.
		expect(
			offerStackPlacement({
				blocks: [null],
				panel: null,
				boxes: [BOX],
				obstacles: committedRegions(),
				host: HOST
			})
		).toEqual([{ x: 280, y: BANNER_TOP_CSS, ...BOX }]);
	});

	it('clamps the column to the screen edge on a capture with no margin, and still draws', () => {
		// The narrow-capture case, and the answer is NOT null. A 1374-wide
		// windowed capture puts the leftmost read region at 269, so the column
		// wants x = 269 - 16 - 260 = -7 and the clamp pins it to 8. `avoidRects`
		// then takes that position because it IS clear — 8..268 stops one pixel
		// short of 269 — so both boxes are drawn, level with their blocks, in a
		// margin narrower than the boxes themselves.
		const obstacles = [
			{ x: 269, y: 120, w: 700, h: 500 },
			{ x: 990, y: 4, w: 380, h: 400 }
		];
		expect(
			offerStackPlacement({
				blocks: [
					{ x: 1000, y: 150, w: 280, h: 43 },
					{ x: 1000, y: 300, w: 280, h: 43 }
				],
				panel: { x: 990, y: 4, w: 380, h: 400 },
				boxes: [BOX, BOX],
				obstacles,
				host: { width: 1374, height: 773 }
			})
		).toEqual([
			{ x: 8, y: 150, ...BOX },
			{ x: 8, y: 300, ...BOX }
		]);
	});

	it('relocates a box off the margin when the clamped column is not clear', () => {
		// The other half of that chain: `avoidRects` does not answer null
		// because a position is blocked, it answers the nearest FREE one. Here a
		// read region covers the whole left side of the 1374-wide capture, so
		// the clamped column at x 8 is inside it and both boxes leave the margin
		// altogether — flush past its right edge at 600, still level with their
		// own blocks. They stop being a left-margin column, which is the trade
		// the placement takes: the cyan frame is the pointer, not the distance.
		const obstacles = [
			{ x: 0, y: 0, w: 600, h: 773 },
			{ x: 990, y: 4, w: 380, h: 400 }
		];
		expect(
			offerStackPlacement({
				blocks: [
					{ x: 1000, y: 150, w: 280, h: 43 },
					{ x: 1000, y: 300, w: 280, h: 43 }
				],
				panel: { x: 990, y: 4, w: 380, h: 400 },
				boxes: [BOX, BOX],
				obstacles,
				host: { width: 1374, height: 773 }
			})
		).toEqual([
			{ x: 600, y: 150, ...BOX },
			{ x: 600, y: 300, ...BOX }
		]);
	});

	it('stacks the second box under where the first was PLACED, not where it wanted to be', () => {
		// The first box's wanted position is blocked, so the avoidance slides it
		// down; the second must then follow the ANSWER rather than the wish, or
		// the two overlap. The blocker is a synthetic read region at the screen's
		// own left edge — a shape the committed frame has nothing like, which is
		// the point of the margin — and it moves the column twice: it becomes
		// the leftmost region, so the wanted x is 0 - 16 - 260 clamped up to 8,
		// and it then sits across the first box's wanted band. Sliding DOWN
		// (70 px, to its bottom edge) beats sliding sideways (292 px, to its
		// right edge).
		const obstacles = [...committedRegions(), { x: 0, y: 100, w: 300, h: 120 }];
		const placed = offerStackPlacement({
			blocks: BLOCKS,
			panel: COMMITTED_PANEL,
			boxes: [BOX, BOX],
			obstacles,
			host: HOST
		});
		expect(placed).toEqual([
			{ x: 8, y: 220, ...BOX },
			{ x: 8, y: 220 + BOX.h + STACK_GAP_CSS, ...BOX }
		]);
		// Stated as the property as well as the numbers: whatever the avoidance
		// answers, two drawn boxes never share a pixel.
		const [upper, lower] = placed as WidgetRect[];
		expect(lower.y).toBeGreaterThanOrEqual(upper.y + upper.h);
	});

	it('keeps the second box off the first when the avoidance would slide it back up', () => {
		// The stacking floor alone does NOT make this safe. It puts the second
		// box's WANTED position below the first, and `avoidRects` then MOVES
		// that position — here off a read region across the second box's band,
		// whose nearest escape upward lands on the box already drawn. So the
		// boxes placed so far go in as obstacles too, and the answer is the
		// escape downward instead.
		//
		// A NARROW-margin frame, which is what the collision needs: on the
		// committed 1920 frame the margin is 556 px wide and the column never
		// reaches anything. Here the leftmost region is at 200, so the wanted x
		// is clamped to 8 and the column sits across the board's own left edge —
		// the windowed capture the placement's doc says may simply not fit.
		const obstacles = [
			{ x: 200, y: 250, w: 200, h: 200 },
			{ x: 1131, y: 4, w: 544, h: 454 }
		];
		const placed = offerStackPlacement({
			blocks: BLOCKS,
			panel: COMMITTED_PANEL,
			boxes: [BOX, BOX],
			obstacles,
			host: HOST
		}) as WidgetRect[];
		expect(placed[0]).toEqual({ x: 8, y: 150, ...BOX });
		// Flush under the region it had to clear (250 + 200), and not at y 154,
		// which is 84 px NEARER to what it wanted and four pixels into the box
		// above it — the answer without the first box in the obstacle set.
		expect(placed[1]).toEqual({ x: 8, y: 450, ...BOX });
		expect(rectIsClear(placed[1], [placed[0]])).toBe(true);
	});

	it('withholds a box that has not measured itself, and places the other anyway', () => {
		// The frame between render and measurement. A zero-size box is not an
		// obstacle either — treating it as one would stack the box below it at a
		// position that jumps as soon as the first one measures.
		expect(
			offerStackPlacement({
				blocks: BLOCKS,
				panel: COMMITTED_PANEL,
				boxes: [{ w: 0, h: 0 }, BOX],
				obstacles: committedRegions(),
				host: HOST
			})
		).toEqual([null, { x: 280, y: 260, ...BOX }]);
	});

	it('withholds a box that cannot be placed clear, and places the other anyway', () => {
		// One box per answer, which is the whole reason this returns a list
		// rather than a placement for the stack. The second box is larger than
		// the host in both directions — `avoidRects`'s own documented degenerate
		// input: it is clamped to the origin, it covers the panel's crop
		// wherever it is put, and the answer is null, so it is NOT drawn
		// (ADR-019). The first still is, at x 8: the column's x comes off the
		// WIDEST box and is clamped up rather than sent off the left of the
		// screen.
		//
		// It takes a box that big because the left margin on this frame is
		// genuinely empty — 556 px of it — so no ordinary box fails to fit
		// there. That is the measurement the placement is built on, stated from
		// the other side.
		expect(
			offerStackPlacement({
				blocks: BLOCKS,
				panel: COMMITTED_PANEL,
				boxes: [BOX, { w: 2400, h: 1000 }],
				obstacles: committedRegions(),
				host: HOST
			})
		).toEqual([{ x: 8, y: 150, ...BOX }, null]);
	});

	it('places nothing at all with an empty never-cover set', () => {
		// The rule this file's other three placers state, and the one this
		// placer needs most: `boardLeft` is a minimum over that set, so an empty
		// one has no left edge to be beside. Empty means the layout is absent or
		// the scale factor has not resolved — "place nothing yet", never "the
		// screen is free".
		expect(
			offerStackPlacement({
				blocks: BLOCKS,
				panel: COMMITTED_PANEL,
				boxes: [BOX, BOX],
				obstacles: [],
				host: HOST
			})
		).toEqual([null, null]);
	});
});

describe('bannerPlacement', () => {
	it('centres the banner at the top of the host when that is clear', () => {
		// A published set that is nowhere near the top centre — the plate crop
		// from `ROIS`, bottom-left. "Clear" has to be demonstrated against a
		// real never-cover set: an EMPTY one is not a clear screen, it is a
		// conversion that has not happened, and the case below is what that
		// answers.
		expect(
			bannerPlacement({
				box: { w: 400, h: 30 },
				obstacles: [{ x: 200, y: 700, w: 120, h: 60 }],
				host: HOST
			})
		).toEqual({ x: 760, y: BANNER_TOP_CSS, w: 400, h: 30 });
	});

	it('is null with an empty never-cover set, which is not a free screen', () => {
		// `neverCoverRects` is empty for a layout that is absent and for a scale
		// factor that has not resolved. Placing on that input put the one
		// unmissable line in the overlay top-centre — which on the committed
		// 1920x1080 frame is over the panel crop the very next tick reads. A
		// full board publishes 42 rects, so an empty list is never "the screen
		// happens to be free".
		expect(bannerPlacement({ box: { w: 400, h: 30 }, obstacles: [], host: HOST })).toBeNull();
	});

	it('moves it off the side panel rather than centring over the read', () => {
		// The measured regression (POE-244 review): on the committed 1920x1080
		// frame the panel's crop starts at x 1131 and a ~440 px banner centred
		// at 960 runs to 1180, so "centred and pinned at the top" put the one
		// unmissable line in the overlay straight over the OCR region.
		//
		// The escape here is VERTICAL, and that is the honest answer rather than
		// the one first guessed: the crop starts at y 40, so sliding 6 px up to
		// y = 10 clears it, against 49 px of sideways movement.
		const panel = { x: 1131, y: 40, w: 500, h: 400 };
		expect(
			bannerPlacement({ box: { w: 440, h: 30 }, obstacles: [panel], host: HOST })
		).toEqual({ x: 740, y: 10, w: 440, h: 30 });
	});

	it('slides sideways when the read region reaches the top of the screen', () => {
		// The other escape, pinned separately so the axis is not an accident of
		// one fixture: with the crop starting at y 0 there is no room above it,
		// and the banner goes to 1131 − 440 = 691 — flush against its left edge.
		const panel = { x: 1131, y: 0, w: 500, h: 400 };
		expect(
			bannerPlacement({ box: { w: 440, h: 30 }, obstacles: [panel], host: HOST })
		).toEqual({ x: 691, y: BANNER_TOP_CSS, w: 440, h: 30 });
	});

	it('is null before the banner has measured itself', () => {
		expect(bannerPlacement({ box: { w: 0, h: 0 }, obstacles: [], host: HOST })).toBeNull();
	});
});

describe('waitingDefaultPlacement', () => {
	/** The registry's shipped box for `temple.waiting`, READ OUT OF the registry
	 *  rather than copied from it. What the route passes is `spec.defaults`, so
	 *  a literal here would be a second copy of the shipped size — and the
	 *  clearance case below, which is this suite's only guard on that size
	 *  growing into the panel's crop, would go on passing against the old
	 *  numbers while the app shipped the new ones. */
	const spec = widgetsFor(TEMPLE_WINDOW_LABEL).find((widget) => widget.id === 'temple.waiting');
	if (!spec) throw new Error('the registry no longer declares temple.waiting');
	const NOTICE = { w: spec.defaults.w, h: spec.defaults.h };
	/** Where a box that size WANTS to go — the top centre of the host, which is
	 *  what the two cases below expect to come back UNTOUCHED. Derived from the
	 *  registry for the same reason `NOTICE` is. */
	const WANTED = { x: HOST.width / 2 - NOTICE.w / 2, y: BANNER_TOP_CSS, ...NOTICE };

	it('ships the notice at the top centre of the host when that is clear', () => {
		// Host centre minus half the box, at `BANNER_TOP_CSS` — 830 on the
		// shipped 260. The obstacle is a real published rect nowhere near the
		// top centre (a plate crop, bottom-left): "clear" has to be shown
		// against a non-empty set, because an EMPTY one is the case below.
		expect(
			waitingDefaultPlacement({
				box: NOTICE,
				obstacles: [{ x: 200, y: 700, w: 120, h: 60 }],
				host: HOST
			})
		).toEqual(WANTED);
	});

	it('clears the committed frame\'s panel crop at the shipped width', () => {
		// The measurement the 260 px default was chosen from (POE-249 A6): a box
		// that wide centred on this host ends at 1090 and the crop starts at
		// 1131, so the wanted position stands UNTOUCHED with 41 px to spare.
		// Both sides come off the registry, so this is the assertion that fails
		// when the shipped default grows past that margin — the failure that
		// otherwise happens silently, on screen, inside the module's own read.
		// (At 400 px wide the wanted rect starts at 760, runs to 1160, and
		// `avoidRects` slides it left to 731: not `WANTED`, and red here.)
		expect(
			waitingDefaultPlacement({ box: NOTICE, obstacles: [COMMITTED_PANEL], host: HOST })
		).toEqual(WANTED);
	});

	it('slides a box too wide for that margin off the crop instead of covering it', () => {
		// 400 px centred runs 760..1160, over a crop that starts at 1131. The
		// escape is sideways and not upward — the crop reaches y 4, so there is
		// no room above it — and the answer is flush against its left edge:
		// 1131 - 400 = 731, at the same top.
		expect(
			waitingDefaultPlacement({
				box: { w: 400, h: 40 },
				obstacles: [COMMITTED_PANEL],
				host: HOST
			})
		).toEqual({ x: 731, y: BANNER_TOP_CSS, w: 400, h: 40 });
	});

	it('is null with an empty never-cover set, which is not a free screen', () => {
		// The same rule its siblings state, and the one this placer needs most:
		// its wanted position is a function of the HOST alone, so with nothing
		// passed to `avoidRects` the top centre comes back "clear" — over the
		// crop the next tick reads. The registry's fixed rectangle is what the
		// caller falls back to (ADR-019's user-owned carve-out), which is the
		// cold start this widget exists for.
		expect(waitingDefaultPlacement({ box: NOTICE, obstacles: [], host: HOST })).toBeNull();
	});

	it('is null before the box has a size', () => {
		// Against a real obstacle set, so what is being pinned is the BOX guard
		// and not the empty-set one: a zero-size rect overlaps nothing, so
		// without this it would answer the top centre for a widget that has no
		// width to be centred.
		expect(
			waitingDefaultPlacement({ box: { w: 0, h: 0 }, obstacles: [COMMITTED_PANEL], host: HOST })
		).toBeNull();
	});
});

describe('doorDefaultPlacement', () => {
	it('ships below the panel and in the game diamond\'s own column', () => {
		// x = the panel diamond's left edge (600); y = the panel's bottom
		// (100 + 300) plus the gap. Nothing else is there, so that is the answer.
		expect(
			doorDefaultPlacement({
				panel: PANEL,
				diamond: DIAMOND,
				box: { w: 190, h: 215 },
				obstacles: [PANEL, DIAMOND],
				host: HOST
			})
		).toEqual({ x: 600, y: 400 + CALLOUT_GAP_CSS, w: 190, h: 215 });
	});

	it('falls back to the panel\'s column when the diamond crop was not published', () => {
		expect(
			doorDefaultPlacement({
				panel: PANEL,
				diamond: null,
				box: { w: 190, h: 215 },
				obstacles: [PANEL],
				host: HOST
			})
		).toEqual({ x: 480, y: 416, w: 190, h: 215 });
	});

	it('moves off a plate that happens to sit where it wanted to ship', () => {
		// A plate crop at 600..630 × 416..436 covers the wanted origin. The two
		// escapes are down to y = 436 (20 px) and right to x = 630 (30 px), so
		// the answer drops below the plate.
		expect(
			doorDefaultPlacement({
				panel: PANEL,
				diamond: DIAMOND,
				box: { w: 190, h: 215 },
				obstacles: [PANEL, { x: 600, y: 416, w: 30, h: 20 }],
				host: HOST
			})
		).toEqual({ x: 600, y: 436, w: 190, h: 215 });
	});

	it('is null with no panel to sit below', () => {
		expect(
			doorDefaultPlacement({
				panel: null,
				diamond: DIAMOND,
				box: { w: 190, h: 215 },
				obstacles: [DIAMOND],
				host: HOST
			})
		).toBeNull();
	});

	it('is null with an empty never-cover set, even with a panel to sit below', () => {
		// The rule stated at the function, not only at `doorDefaults` in the
		// route. The panel rect here is non-null while the obstacle set is
		// empty, which is a combination the shipped caller cannot produce
		// (both come from the same conversion) — and that is the point: a
		// second caller must inherit the refusal rather than have to remember
		// it. The registry's shipped default is what the caller falls back to,
		// and that is a user-visible rectangle rather than a game-derived
		// placement (ADR-019).
		expect(
			doorDefaultPlacement({
				panel: PANEL,
				diamond: DIAMOND,
				box: { w: 190, h: 215 },
				obstacles: [],
				host: HOST
			})
		).toBeNull();
	});

	it('never ships on top of a read region', () => {
		// The property behind the three positions above, checked against the
		// whole published set rather than against the one obstacle each case
		// arranges: this is the rule, and the positions are its instances.
		const obstacles = neverCoverRects(layout(ROIS), 1);
		const placed = doorDefaultPlacement({
			panel: PANEL,
			diamond: DIAMOND,
			box: { w: 190, h: 215 },
			obstacles,
			host: HOST
		});
		expect(placed).not.toBeNull();
		expect(rectIsClear(placed!, obstacles)).toBe(true);
	});
});

describe('diamondGeometry', () => {
	/** A square diamond with round corners, so every expectation is arithmetic
	 *  rather than a copy of the fitted projection's constants. */
	const diamond: DiamondView = {
		corners: [
			[2, 0],
			[0, 2],
			[-2, 0],
			[0, -2]
		],
		seals: [
			{ neighbour: 'C2', edge: 'C1-C2', pos: [2, 0] },
			{ neighbour: 'B1', edge: 'B1-C1', pos: [0, -2] },
			{ neighbour: 'C0', edge: 'C0-C1', pos: [-2, 0] },
			{ neighbour: 'D2', edge: 'C1-D2', pos: [0, 2] },
			// A second CLOSED corridor, so the advice can sit on one of them
			// and leave the other plain — which is what makes "closed is drawn
			// red at the plain radius" assertable at all.
			{ neighbour: 'B0', edge: 'B0-C1', pos: [-1, -1] }
		],
		// Mirrored through the centre, as `markers::architect_icons` publishes
		// them; round here for the same reason the corners are.
		topIcon: [1, -1],
		bottomIcon: [-1, 1]
	};
	const board = {
		...layout([]),
		// One corridor of each of the three states `edgeState` can report, in
		// the shape a real read publishes: `uncertain` carries every corridor
		// incident to the current room, and the settled `doors` is the verdict
		// over them (POE-248).
		doors: ['C1-C2', 'C1-D2'],
		uncertain: ['C1-C2', 'B1-C1', 'C1-D2'],
		unresolvedIncident: ['C0-C1']
	};

	it('draws the outline Rust published, in ring order', () => {
		expect(diamondGeometry(diamond, board, []).outline).toBe('2,0 0,2 -2,0 0,-2');
	});

	it('classifies each seal by the same edge state the board and the page use', () => {
		// Three answers, not two: "settled by nothing" must stay distinguishable
		// from a door the read HAS settled shut — POE-171's honesty guard,
		// carried onto a widget that outlives the panel it was read from. And
		// the two corridors the beam flagged uncertain but the seals settled are
		// open, which is the POE-248 fix seen from the geometry's side.
		expect(
			diamondGeometry(diamond, board, []).seals.map((seal) => [seal.edge, seal.state])
		).toEqual([
			['C1-C2', 'open'],
			['B1-C1', 'closed'],
			['C0-C1', 'unresolved'],
			['C1-D2', 'open'],
			['B0-C1', 'closed']
		]);
	});

	it('sizes the three kinds of seal in the order a glance reads them', () => {
		// Advice at two strengths and a corridor with none: the door to open
		// NOW is the biggest, the one a SECOND stone would buy sits between, and
		// a corridor the advisor said nothing about is the smallest. The order
		// is the claim — a reader who cannot see the colours still gets the
		// ranking from the sizes.
		const seals = diamondGeometry(diamond, board, ['B1-C1'], 'B0-C1').seals;
		expect(seals.map((seal) => [seal.edge, seal.kind])).toEqual([
			['C1-C2', 'plain'],
			['B1-C1', 'suggested'],
			['C0-C1', 'plain'],
			['C1-D2', 'plain'],
			['B0-C1', 'secondary']
		]);
		expect(seals[1].radius).toBe(SEAL_RADIUS_SUGGESTED);
		expect(seals[4].radius).toBe(SEAL_RADIUS_SECONDARY);
		expect(seals[0].radius).toBe(SEAL_RADIUS);
		expect(SEAL_RADIUS_SUGGESTED).toBeGreaterThan(SEAL_RADIUS_SECONDARY);
		expect(SEAL_RADIUS_SECONDARY).toBeGreaterThan(SEAL_RADIUS);
	});

	it('gives a corridor that is both the primary and the conditional door the primary size', () => {
		// Rust never publishes this — `conditional_second_door` returns the
		// OTHER member of the pair — so it is a guard rather than a case: the
		// door to open now must not be drawn as the one to wait for.
		const seals = diamondGeometry(diamond, board, ['B1-C1'], 'B1-C1').seals;
		expect(seals[1].kind).toBe('suggested');
		expect(seals[1].radius).toBe(SEAL_RADIUS_SUGGESTED);
	});

	it('leaves every seal plain when the advisor named no door', () => {
		expect(
			diamondGeometry(diamond, board, [], null).seals.every((seal) => seal.kind === 'plain')
		).toBe(true);
	});

	it('places each seal at the position Rust published, with nothing recomputed', () => {
		// The whole reason the slice carries the shape: `markers::seal_position`
		// is a fit against eight measured boards, and a second copy of it here
		// would be a second answer a re-fit leaves behind.
		expect(diamondGeometry(diamond, board, []).seals.map((s) => [s.x, s.y])).toEqual([
			[2, 0],
			[0, -2],
			[-2, 0],
			[0, 2],
			[-1, -1]
		]);
	});

	it('leaves room for a suggested seal sitting on a corner', () => {
		// The clipping case: the largest seal is drawn centred ON the outline,
		// so a viewBox that stopped at the corners would cut it in half.
		const [minX, minY, width, height] = diamondGeometry(diamond, board, ['B1-C1'])
			.viewBox.split(' ')
			.map(Number);
		expect(minX).toBeLessThanOrEqual(-2 - SEAL_RADIUS_SUGGESTED);
		expect(minY).toBeLessThanOrEqual(-2 - SEAL_RADIUS_SUGGESTED);
		expect(minX + width).toBeGreaterThanOrEqual(2 + SEAL_RADIUS_SUGGESTED);
		expect(minY + height).toBeGreaterThanOrEqual(2 + SEAL_RADIUS_SUGGESTED);
	});

	describe('sealVisible', () => {
		/** The five seals of the fixture above, already classified. */
		const seals = (secondary: string | null = null) =>
			diamondGeometry(diamond, board, ['B1-C1'], secondary).seals;

		it('draws every corridor the read settled, in the game\'s own two colours', () => {
			// The POE-248 correction, as the list of what survives: both open
			// corridors, both closed ones — the game draws a wall red and the
			// widget replaces the game's diamond during the incursion — and
			// nothing for the one the read could not settle.
			expect(seals().filter(sealVisible).map((seal) => [seal.edge, seal.state])).toEqual([
				['C1-C2', 'open'],
				['B1-C1', 'closed'],
				['C1-D2', 'open'],
				['B0-C1', 'closed']
			]);
		});

		it('draws nothing for a corridor nothing settled', () => {
			// The one the owner did call chaos, and the only one still hidden:
			// "we could not read it" is real, and it is said by `doorWarning` in
			// words rather than by a dot nobody can act on.
			const hidden = seals()
				.filter((seal) => !sealVisible(seal))
				.map((seal) => [seal.edge, seal.state]);
			expect(hidden).toEqual([['C0-C1', 'unresolved']]);
		});

		it('draws the advisor\'s answer even on a corridor nothing settled', () => {
			// The advisor reads an unsettled corridor as closed and can name it.
			// Withholding the answer there would leave the widget silent about
			// the one thing it exists to say — `doorWarning` is already up
			// saying the shape may be wrong.
			const conditional = seals('C0-C1').find((seal) => seal.edge === 'C0-C1');
			expect(conditional).toMatchObject({ state: 'unresolved', kind: 'secondary' });
			expect(sealVisible(conditional!)).toBe(true);
		});
	});

	describe('killGlyphs', () => {
		/** Two blocks with rects, the CHANGE one printed first. */
		const top = { index: 0, kind: 'change', rect: [1200, 100, 300, 40] as CaptureRect };
		const bottom = { index: 1, kind: 'upgrade', rect: [1200, 200, 300, 40] as CaptureRect };
		const both = [top, bottom];

		it('marks the half the chosen block was PRINTED in, not the half its kind implies', () => {
			// The whole point of keying on the rect: on this panel the `change`
			// block is the top one, so its glyph goes in the top-right half —
			// the opposite of what the one measured board's upgrade/change
			// reading would have said.
			expect(killGlyphs(diamond, top, both)[0]).toEqual({
				position: { x: 1, y: -1 },
				kind: 'change',
				chosen: true
			});
			expect(killGlyphs(diamond, bottom, both)[0]).toEqual({
				position: { x: -1, y: 1 },
				kind: 'upgrade',
				chosen: true
			});
		});

		it('draws the other block too, at the complementary spot with its own kind', () => {
			// The pair is what orients: one mark says which half, two say what
			// the halves ARE. The component draws this one faint — "faint is the
			// alternative", the rule the conditional door seal shares.
			expect(killGlyphs(diamond, top, both)).toEqual([
				{ position: { x: 1, y: -1 }, kind: 'change', chosen: true },
				{ position: { x: -1, y: 1 }, kind: 'upgrade', chosen: false }
			]);
			// And from the other side, so neither half is hard-coded: choosing
			// the bottom block swaps both the spots and the kinds.
			expect(killGlyphs(diamond, bottom, both)).toEqual([
				{ position: { x: -1, y: 1 }, kind: 'upgrade', chosen: true },
				{ position: { x: 1, y: -1 }, kind: 'change', chosen: false }
			]);
		});

		it('draws no faint glyph when only one block was read', () => {
			// POE-243's `forcedKill` shape. There is no second block to mark —
			// inventing one would put a kill on the widget that was never on the
			// panel — and the chosen one falls back to its kind for the half,
			// because one rect orders nothing.
			expect(killGlyphs(diamond, top, [top])).toEqual([
				{ position: { x: -1, y: 1 }, kind: 'change', chosen: true }
			]);
		});

		it('falls back to the kind when the read carried no boxes', () => {
			// A text-only read: nothing orders the blocks, so the one-sample
			// mapping is all there is — upgrade top-right, change bottom-left —
			// and the other block still takes the half the chosen one did not.
			const textOnly = [
				{ index: 0, kind: 'upgrade', rect: null },
				{ index: 1, kind: 'change', rect: null }
			];
			expect(killGlyphs(diamond, textOnly[0], textOnly)).toEqual([
				{ position: { x: 1, y: -1 }, kind: 'upgrade', chosen: true },
				{ position: { x: -1, y: 1 }, kind: 'change', chosen: false }
			]);
		});

		it('marks nothing when the ranking named no architect', () => {
			// `kill either` — the advisor ranked the doors and left the kill
			// free. A glyph on one of the two halves would claim a choice that
			// was not made.
			expect(killGlyphs(diamond, null, both)).toEqual([]);
		});

		it('marks nothing for a kind it does not recognise', () => {
			expect(killGlyphs(diamond, { index: 0, kind: 'sacrifice', rect: null }, both)).toEqual(
				[]
			);
		});

		it('keeps the chosen mark when the OTHER block\'s kind is unrecognised', () => {
			// The half that can still be trusted survives: the chosen block is
			// what the player has to click, and a sibling the vocabulary does
			// not know says nothing about it.
			const odd = { index: 1, kind: 'sacrifice', rect: [1200, 200, 300, 40] as CaptureRect };
			expect(killGlyphs(diamond, top, [top, odd])).toEqual([
				{ position: { x: 1, y: -1 }, kind: 'change', chosen: true }
			]);
		});

		it('marks nothing on a payload from before the spots were published', () => {
			// A glyph placed at the origin would sit in the middle of the room
			// and point at neither architect, which is worse than no glyph.
			const older = { ...diamond, topIcon: null, bottomIcon: null };
			expect(killGlyphs(older, top, both)).toEqual([]);
			expect(killGlyphs(older, bottom, both)).toEqual([]);
		});
	});
});
