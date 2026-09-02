import { describe, expect, it } from 'vitest';
import { hotRectsEqual, physicalHotRect, type HotRect } from './hot-rects';

const rect = (left: number, top: number, right: number, bottom: number) => ({
	left,
	top,
	right,
	bottom,
});

describe('converting a measured element into a hot rect', () => {
	it('reports the rect unchanged at 100 % scaling', () => {
		expect(physicalHotRect(rect(582, 1, 630, 250), 1)).toEqual({ x: 582, y: 1, w: 48, h: 249 });
	});

	// The whole reason the conversion exists: the hook hit-tests against
	// GetWindowRect and a physical cursor, so a CSS rect sent as-is would claim
	// the wrong part of the window on every scaled display.
	it('scales the rect to physical pixels on a 150 % display', () => {
		expect(physicalHotRect(rect(100, 20, 140, 60), 1.5)).toEqual({ x: 150, y: 30, w: 60, h: 60 });
	});

	// Rounding position and size separately would leave a one-pixel seam here
	// that the game receives the click through.
	it('derives the size from the rounded edges so adjacent rects share one', () => {
		const upper = physicalHotRect(rect(10, 10.4, 30, 20.4), 1.25) as HotRect;
		const lower = physicalHotRect(rect(10, 20.4, 30, 30.4), 1.25) as HotRect;

		expect(upper.y + upper.h).toBe(lower.y);
	});

	it('claims nothing for an element with no width', () => {
		expect(physicalHotRect(rect(100, 20, 100, 60), 1)).toBeNull();
	});

	it('claims nothing for an element with no height', () => {
		expect(physicalHotRect(rect(100, 20, 140, 20), 1)).toBeNull();
	});

	// What an unmounted button measures. It must withdraw the claim rather than
	// declare a rect at the viewport origin.
	it('claims nothing for the all-zero rect of an unmounted element', () => {
		expect(physicalHotRect(rect(0, 0, 0, 0), 1)).toBeNull();
	});

	// The route caches `scaleFactor()` and starts at 0 until the promise
	// resolves; converting with it would collapse every rect onto the origin.
	it('claims nothing before the cached scale factor has resolved', () => {
		expect(physicalHotRect(rect(582, 1, 630, 250), 0)).toBeNull();
	});

	it('claims nothing for a scale factor that is not a number', () => {
		expect(physicalHotRect(rect(582, 1, 630, 250), Number.NaN)).toBeNull();
	});

	// An infinite left edge survives the size check — the width comes out
	// positive — so the position is screened as well.
	it('claims nothing for an element measured at an infinite position', () => {
		expect(physicalHotRect(rect(Number.NEGATIVE_INFINITY, 1, 630, 250), 1)).toBeNull();
	});

	// And a NaN edge survives the size check the other way round: NaN is
	// neither greater than nor less than zero.
	it('claims nothing for an element measured with a NaN edge', () => {
		expect(physicalHotRect(rect(100, 20, Number.NaN, 60), 1)).toBeNull();
	});
});

describe('deciding whether a declaration is new', () => {
	it('holds the declaration when every rect is where it was', () => {
		const before: HotRect[] = [{ x: 582, y: 1, w: 48, h: 249 }];
		const after: HotRect[] = [{ x: 582, y: 1, w: 48, h: 249 }];

		expect(hotRectsEqual(before, after)).toBe(true);
	});

	it('sends the declaration when a rect has moved', () => {
		expect(hotRectsEqual([{ x: 582, y: 1, w: 48, h: 249 }], [{ x: 582, y: 2, w: 48, h: 249 }])).toBe(
			false
		);
	});

	it('sends the declaration when a rect has changed size', () => {
		expect(hotRectsEqual([{ x: 582, y: 1, w: 48, h: 249 }], [{ x: 582, y: 1, w: 48, h: 320 }])).toBe(
			false
		);
	});

	// The queue row appearing and disappearing is the comparator's own case.
	it('sends the declaration when a second rect appears', () => {
		const one: HotRect[] = [{ x: 582, y: 1, w: 48, h: 249 }];
		const two: HotRect[] = [...one, { x: 500, y: 230, w: 80, h: 20 }];

		expect(hotRectsEqual(one, two)).toBe(false);
	});

	it('sends the declaration when the last rect is withdrawn', () => {
		expect(hotRectsEqual([{ x: 582, y: 1, w: 48, h: 249 }], [])).toBe(false);
	});

	// Order is the order the route declares its elements in, and the hook
	// answers with the first match — two rects that swapped places are a
	// different claim.
	it('sends the declaration when two rects swap places', () => {
		const a: HotRect = { x: 582, y: 1, w: 48, h: 249 };
		const b: HotRect = { x: 500, y: 230, w: 80, h: 20 };

		expect(hotRectsEqual([a, b], [b, a])).toBe(false);
	});

	it('holds an empty declaration against another empty one', () => {
		expect(hotRectsEqual([], [])).toBe(true);
	});
});
