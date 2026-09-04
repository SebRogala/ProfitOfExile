/**
 * The never-cover placement rule (POE-244).
 *
 * Everything here is hand-derived: the obstacle rectangles are round numbers,
 * and each expected position is worked out from them in the comment above the
 * assertion. A test that asserted "the answer does not overlap anything" would
 * pass for a function that returned the origin every time, which is the one
 * failure that looks identical on screen to a placement nobody has configured.
 */
import { describe, expect, it } from 'vitest';
import { avoidRects, rectIsClear, rectsOverlap } from './widget-avoid';

const HOST = { width: 1920, height: 1080 };

describe('rectsOverlap', () => {
	it('is true for two rectangles that share area', () => {
		expect(
			rectsOverlap({ x: 0, y: 0, w: 10, h: 10 }, { x: 9, y: 9, w: 10, h: 10 })
		).toBe(true);
	});

	it('is false for rectangles that only share a border', () => {
		// The flush case, and it has to be legal: the tightest position a box
		// can take beside a read region is exactly against it, and treating a
		// shared edge as a collision would push every placement a pixel further
		// out for nothing.
		expect(
			rectsOverlap({ x: 0, y: 0, w: 10, h: 10 }, { x: 10, y: 0, w: 10, h: 10 })
		).toBe(false);
		expect(
			rectsOverlap({ x: 0, y: 0, w: 10, h: 10 }, { x: 0, y: 10, w: 10, h: 10 })
		).toBe(false);
	});

	it('is false when either rectangle has no area', () => {
		// A degenerate rect cannot be covered, so it cannot block a placement —
		// which is what makes an obstacle list safe to pass through unfiltered.
		expect(
			rectsOverlap({ x: 0, y: 0, w: 10, h: 10 }, { x: 5, y: 5, w: 0, h: 10 })
		).toBe(false);
		expect(
			rectsOverlap({ x: 0, y: 0, w: 0, h: 10 }, { x: 0, y: 0, w: 10, h: 10 })
		).toBe(false);
	});
});

describe('rectIsClear', () => {
	it('is true only when the rectangle misses every obstacle', () => {
		const obstacles = [
			{ x: 100, y: 100, w: 50, h: 50 },
			{ x: 300, y: 300, w: 50, h: 50 }
		];
		expect(rectIsClear({ x: 200, y: 200, w: 20, h: 20 }, obstacles)).toBe(true);
		// Misses the first, hits the second — one hit is enough.
		expect(rectIsClear({ x: 340, y: 340, w: 20, h: 20 }, obstacles)).toBe(false);
	});

	it('is true against an empty obstacle list', () => {
		expect(rectIsClear({ x: 0, y: 0, w: 20, h: 20 }, [])).toBe(true);
	});
});

describe('avoidRects', () => {
	it('leaves a position that is already clear exactly where it was asked for', () => {
		const wanted = { x: 700, y: 700, w: 60, h: 40 };
		expect(avoidRects(wanted, [{ x: 100, y: 100, w: 200, h: 200 }], HOST)).toEqual(wanted);
	});

	it('slides the box to the nearest edge of the obstacle it landed on', () => {
		// Obstacle spans 100..300 on both axes; the box is 60x40 wanted at
		// (150, 150), fully inside it. The four escapes are x=40 (flush left,
		// 110 away), x=300 (flush right, 150), y=60 (flush above, 90) and
		// y=300 (flush below, 150). The nearest is y=60.
		expect(
			avoidRects({ x: 150, y: 150, w: 60, h: 40 }, [{ x: 100, y: 100, w: 200, h: 200 }], HOST)
		).toEqual({ x: 150, y: 60, w: 60, h: 40 });
	});

	it('prefers the near side of an obstacle over the far one', () => {
		// A full-height column at 100..300 leaves no vertical escape, so the box
		// must move in x. From x=250 the right edge is 50 away and the left one
		// 210, and the answer has to be the right — a rule that always picked the
		// first free candidate would put the callout on the wrong side of the
		// panel.
		expect(
			avoidRects({ x: 250, y: 400, w: 60, h: 40 }, [{ x: 100, y: 0, w: 200, h: 1000 }], HOST)
		).toEqual({ x: 300, y: 400, w: 60, h: 40 });
	});

	it('keeps the box its own size while it moves', () => {
		const moved = avoidRects(
			{ x: 150, y: 150, w: 60, h: 40 },
			[{ x: 100, y: 100, w: 200, h: 200 }],
			HOST
		);
		expect([moved?.w, moved?.h]).toEqual([60, 40]);
	});

	it('pulls a position that hangs off the host back inside it', () => {
		// Wanted at x=1900 with a 60-wide box runs 40 px past a 1920 host. The
		// clamp is what stops a game-anchored placement from rendering off the
		// edge of the monitor, where there is nothing to see and no way back.
		expect(avoidRects({ x: 1900, y: 500, w: 60, h: 40 }, [], HOST)).toEqual({
			x: 1860,
			y: 500,
			w: 60,
			h: 40
		});
	});

	it('answers null rather than a position that covers something', () => {
		// A host entirely covered by one read region. The honest answer is that
		// there is nowhere to draw, and the caller draws nothing — a box placed
		// anyway would be OCR input the app wrote itself.
		expect(
			avoidRects({ x: 10, y: 10, w: 20, h: 20 }, [{ x: 0, y: 0, w: 100, h: 100 }], {
				width: 100,
				height: 100
			})
		).toBeNull();
	});

	it('answers null for a box bigger than the host it must fit inside', () => {
		expect(
			avoidRects({ x: 0, y: 0, w: 200, h: 200 }, [{ x: 0, y: 0, w: 10, h: 10 }], {
				width: 100,
				height: 100
			})
		).toBeNull();
	});

	it('places the box flush against an obstacle rather than one pixel clear of it', () => {
		// The boundary case of the strict overlap test above, end to end: with
		// the only escape being the obstacle's left edge, the answer's right
		// edge must land exactly on it.
		const placed = avoidRects(
			{ x: 90, y: 0, w: 60, h: 1080 },
			[{ x: 100, y: 0, w: 1820, h: 1080 }],
			HOST
		);
		expect(placed).toEqual({ x: 40, y: 0, w: 60, h: 1080 });
	});

	it('is not blocked by an obstacle with no area', () => {
		const wanted = { x: 150, y: 150, w: 60, h: 40 };
		expect(avoidRects(wanted, [{ x: 100, y: 100, w: 200, h: 0 }], HOST)).toEqual(wanted);
	});
});
