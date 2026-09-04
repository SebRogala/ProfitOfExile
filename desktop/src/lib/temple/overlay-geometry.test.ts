/**
 * Where the temple's overlay surfaces go (POE-244).
 *
 * The obstacle geometry below is hand-built and round — a panel crop at
 * 480..680 × 100..400 with the chosen block inside it — so every expected
 * position is arithmetic a reader can redo from the numbers in the comment.
 * That matters more here than in most files: the failure this places against is
 * a box a few pixels into an OCR crop, which on screen is indistinguishable from
 * a box that is fine, and the module then reads its own overlay back as game
 * pixels.
 */
import { describe, expect, it } from 'vitest';
import {
	ARROW_STANDOFF_CSS,
	BANNER_TOP_CSS,
	CALLOUT_GAP_CSS,
	SEAL_RADIUS,
	SEAL_RADIUS_SUGGESTED,
	bannerPlacement,
	calloutArrow,
	calloutPlacement,
	captureToCss,
	diamondGeometry,
	doorDefaultPlacement,
	neverCoverRects,
	roiRect
} from './overlay-geometry';
import { rectIsClear } from '$lib/overlay/widgets/widget-avoid';
import type { DiamondView, LayoutView, RoiView } from './slice';

const HOST = { width: 1920, height: 1080 };

/** The side panel's crop, in CSS px at scale factor 1. */
const PANEL = { x: 480, y: 100, w: 200, h: 300 };
/** The panel's own diamond, inside it. */
const DIAMOND = { x: 600, y: 120, w: 100, h: 100 };
/** The architect block the advisor chose, inside the panel. */
const BLOCK = { x: 500, y: 200, w: 100, h: 40 };

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

describe('calloutPlacement', () => {
	const obstacles = [PANEL, DIAMOND];

	it('sits beside the architect block, clear of the panel it is drawn in', () => {
		// Wanted: x = 500 − 16 − 60 = 424, y = 200 + 20 − 10 = 210 (centred on
		// the block). That overlaps the panel's crop by 4 px, so the box slides
		// to x = 420, flush against the panel's left edge — the nearest legal
		// position and the one the owner's mock draws.
		expect(
			calloutPlacement({
				target: BLOCK,
				panel: PANEL,
				box: { w: 60, h: 20 },
				obstacles,
				host: HOST
			})
		).toEqual({ x: 420, y: 210, w: 60, h: 20 });
	});

	it('opens a gap rather than touching the block when there is room', () => {
		// A block outside every read region: the box lands exactly
		// CALLOUT_GAP_CSS to its left, vertically centred, with nothing to slide
		// off. 1200 − 16 − 60 = 1124; 500 + 20 − 10 = 510.
		expect(
			calloutPlacement({
				target: { x: 1200, y: 500, w: 100, h: 40 },
				panel: PANEL,
				box: { w: 60, h: 20 },
				obstacles,
				host: HOST
			})
		).toEqual({ x: 1200 - CALLOUT_GAP_CSS - 60, y: 510, w: 60, h: 20 });
	});

	it('falls back to the panel when the read carried no block rect', () => {
		// A text-only read still gets a box; it is anchored to the panel's left
		// edge at the panel's TOP, because centring on a 300-px-tall crop would
		// put it halfway down the screen for no reason.
		expect(
			calloutPlacement({
				target: null,
				panel: PANEL,
				box: { w: 60, h: 20 },
				obstacles,
				host: HOST
			})
		).toEqual({ x: 480 - CALLOUT_GAP_CSS - 60, y: 100, w: 60, h: 20 });
	});

	it('is null with nothing on screen to be beside', () => {
		// No block and no panel means no board. A box placed against nothing is
		// a box placed over whatever happens to be there.
		expect(
			calloutPlacement({
				target: null,
				panel: null,
				box: { w: 60, h: 20 },
				obstacles,
				host: HOST
			})
		).toBeNull();
	});

	it('is null before the box has measured itself', () => {
		// The frame between render and measurement. A zero-sized box would be
		// placed at a position its real size does not fit, and the next frame
		// would jump it.
		expect(
			calloutPlacement({ target: BLOCK, panel: PANEL, box: { w: 0, h: 0 }, obstacles, host: HOST })
		).toBeNull();
	});

	it('is null rather than drawn over a read region when nothing is free', () => {
		expect(
			calloutPlacement({
				target: BLOCK,
				panel: PANEL,
				box: { w: 60, h: 20 },
				obstacles: [{ x: 0, y: 0, w: 1920, h: 1080 }],
				host: HOST
			})
		).toBeNull();
	});

	it('is null with an empty never-cover set even when a block rect is offered', () => {
		// The rule stated at this function rather than left to the anchor. In
		// the shipped route an unresolved scale factor nulls the target and the
		// panel too, so this input cannot arise today — which is exactly why it
		// is pinned: the coincidence is one refactor from ending, and "empty
		// means place nothing yet" must not depend on it.
		expect(
			calloutPlacement({
				target: BLOCK,
				panel: PANEL,
				box: { w: 60, h: 20 },
				obstacles: [],
				host: HOST
			})
		).toBeNull();
	});
});

describe('calloutArrow', () => {
	it('runs from the box side facing the block to the middle of the block side facing back', () => {
		// Box at 420..480 × 210..230, centre (450, 220). The block's four edge
		// midpoints are N (550,200), E (600,220), S (550,240), W (500,220); the
		// nearest to the box centre is W at 50 px. The box end then follows it:
		// of the box's own midpoints, E (480,220) is nearest to (500,220). The
		// line then STOPS 10 px short of that, at x = 490.
		expect(calloutArrow({ x: 420, y: 210, w: 60, h: 20 }, BLOCK)).toEqual({
			x1: 480,
			y1: 220,
			x2: 490,
			y2: 220
		});
	});

	it('points down when the box is above the block', () => {
		// Box centred at (550, 100), directly above the block: the block's
		// nearest midpoint is N (550, 200), the box leaves from its own S edge
		// (550, 120), and the standoff pulls the end back to y = 190.
		expect(calloutArrow({ x: 520, y: 80, w: 60, h: 40 }, BLOCK)).toEqual({
			x1: 550,
			y1: 120,
			x2: 550,
			y2: 190
		});
	});

	it('stops the line short of the block, so the head does not land on the text', () => {
		// The reason the standoff exists: the arrowHEAD is a filled triangle
		// drawn AT the line's end, and a line that reached the block's edge put
		// that triangle on the first glyphs of the OCR crop the module reads.
		// Asserted as the distance rather than as a coordinate, so the claim is
		// the same one whatever direction the arrow runs.
		const arrow = calloutArrow({ x: 420, y: 210, w: 60, h: 20 }, BLOCK);
		const gap = Math.hypot(500 - arrow.x2, 220 - arrow.y2);
		expect(gap).toBeCloseTo(ARROW_STANDOFF_CSS, 6);
		expect(ARROW_STANDOFF_CSS).toBeGreaterThanOrEqual(8);
	});

	it('leaves a line with nowhere to go alone rather than reversing it', () => {
		// A box whose chosen edge midpoint is inside the standoff of the target
		// midpoint. Pulling back further than the line is long would flip its
		// direction and point the head away from the block.
		const touching = calloutArrow({ x: 494, y: 215, w: 6, h: 10 }, BLOCK);
		expect(touching.x2).toBe(500);
		expect(touching.y2).toBe(220);
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
			{ neighbour: 'D2', edge: 'C1-D2', pos: [0, 2] }
		]
	};
	const board = {
		...layout([]),
		// One corridor of each of the four states `edgeState` can report.
		doors: ['C1-C2', 'C1-D2'],
		uncertain: ['C1-D2'],
		unresolvedIncident: ['C0-C1']
	};

	it('draws the outline Rust published, in ring order', () => {
		expect(diamondGeometry(diamond, board, []).outline).toBe('2,0 0,2 -2,0 0,-2');
	});

	it('colours each seal by the same edge state the board and the page use', () => {
		// Four answers, not two. "Reported open but hidden behind the selection
		// frame" and "settled by nothing" must both stay distinguishable from
		// the red the game uses for a door it HAS settled shut — that is
		// POE-171's honesty guard, carried onto a widget that outlives the
		// panel it was read from.
		expect(
			diamondGeometry(diamond, board, []).seals.map((seal) => [seal.edge, seal.state])
		).toEqual([
			['C1-C2', 'open'],
			['B1-C1', 'closed'],
			['C0-C1', 'unresolved'],
			['C1-D2', 'uncertain']
		]);
	});

	it('draws the advisor\'s door larger than the rest', () => {
		const seals = diamondGeometry(diamond, board, ['B1-C1']).seals;
		expect(seals.map((seal) => [seal.edge, seal.suggested])).toEqual([
			['C1-C2', false],
			['B1-C1', true],
			['C0-C1', false],
			['C1-D2', false]
		]);
		expect(seals[1].radius).toBe(SEAL_RADIUS_SUGGESTED);
		expect(seals[0].radius).toBe(SEAL_RADIUS);
		expect(SEAL_RADIUS_SUGGESTED).toBeGreaterThan(SEAL_RADIUS);
	});

	it('places each seal at the position Rust published, with nothing recomputed', () => {
		// The whole reason the slice carries the shape: `markers::seal_position`
		// is a fit against eight measured boards, and a second copy of it here
		// would be a second answer a re-fit leaves behind.
		expect(diamondGeometry(diamond, board, []).seals.map((s) => [s.x, s.y])).toEqual([
			[2, 0],
			[0, -2],
			[-2, 0],
			[0, 2]
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
});
