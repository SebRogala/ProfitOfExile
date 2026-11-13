/**
 * Where the Divine Font alert stack ships (POE-328).
 *
 * Every font rectangle below is the capture-px `placements.lab.font` Rust
 * publishes for that screen (ssot.rs), and every expected position is
 * arithmetic a reader can redo: right edge of the font region plus the 12 px
 * gap, or left edge minus the gap minus the 260 px box.
 */
import { describe, expect, it } from 'vitest';
import { rectsOverlap } from '$lib/overlay/widgets/widget-avoid';
import { fontAlertsDefaultPlacement } from './font-alerts-geometry';

const BOX = { w: 260, h: 260 };
const HOST_1080 = { width: 1920, height: 1080 };

describe('fontAlertsDefaultPlacement', () => {
	it('places the stack right of the font region at 1920x1080', () => {
		expect(
			fontAlertsDefaultPlacement({
				font: [298, 220, 666, 681],
				gem: [0, 0, 1920, 40],
				scaleFactor: 1,
				box: BOX,
				host: HOST_1080
			})
		).toEqual({ x: 976, y: 220 });
	});

	it('places the stack right of the font region at 1920x1200', () => {
		expect(
			fontAlertsDefaultPlacement({
				font: [224, 244, 740, 757],
				gem: [0, 0, 1920, 44],
				scaleFactor: 1,
				box: BOX,
				host: { width: 1920, height: 1200 }
			})
		).toEqual({ x: 976, y: 244 });
	});

	it('the 1920x1200 placement does not overlap the font region', () => {
		const placed = fontAlertsDefaultPlacement({
			font: [224, 244, 740, 757],
			gem: [0, 0, 1920, 44],
			scaleFactor: 1,
			box: BOX,
			host: { width: 1920, height: 1200 }
		});
		expect(placed).not.toBeNull();
		expect(rectsOverlap({ ...placed!, ...BOX }, { x: 224, y: 244, w: 740, h: 757 })).toBe(false);
	});

	it('converts capture px by the window scale factor', () => {
		// 372/1.25 = 297.6, 888/1.25 = 710.4, 293/1.25 = 234.4.
		expect(
			fontAlertsDefaultPlacement({
				font: [372, 293, 888, 908],
				gem: null,
				scaleFactor: 1.25,
				box: BOX,
				host: { width: 2048, height: 1152 }
			})
		).toEqual({ x: 1020, y: 234.4 });
	});

	it('falls back to the left of the region when the right does not fit', () => {
		expect(
			fontAlertsDefaultPlacement({
				font: [298, 220, 666, 681],
				gem: null,
				scaleFactor: 1,
				box: BOX,
				host: { width: 1100, height: 1080 }
			})
		).toEqual({ x: 26, y: 220 });
	});

	it('offers nothing when neither side fits', () => {
		expect(
			fontAlertsDefaultPlacement({
				font: [100, 220, 666, 681],
				gem: null,
				scaleFactor: 1,
				box: BOX,
				host: { width: 1000, height: 1080 }
			})
		).toBeNull();
	});

	it('offers nothing without a font region', () => {
		expect(
			fontAlertsDefaultPlacement({
				font: null,
				gem: [0, 0, 1920, 40],
				scaleFactor: 1,
				box: BOX,
				host: HOST_1080
			})
		).toBeNull();
	});

	it('offers nothing before the scale factor resolves', () => {
		expect(
			fontAlertsDefaultPlacement({
				font: [298, 220, 666, 681],
				gem: [0, 0, 1920, 40],
				scaleFactor: 0,
				box: BOX,
				host: HOST_1080
			})
		).toBeNull();
	});

	it('keeps the stack off the gem name band', () => {
		expect(
			fontAlertsDefaultPlacement({
				font: [298, 20, 666, 681],
				gem: [0, 0, 1920, 40],
				scaleFactor: 1,
				box: BOX,
				host: HOST_1080
			})
		).toEqual({ x: 976, y: 40 });
	});
});
