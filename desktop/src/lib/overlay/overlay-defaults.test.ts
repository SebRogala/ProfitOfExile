/**
 * The shipped overlay geometry has ONE home, consumed by the merc widget.
 *
 * The merc widget registry and widget geometry code consume this module's
 * shipped defaults. The numeric assertions below pin the shared source and
 * its CSS-to-physical conversion.
 */
import { describe, expect, it } from 'vitest';
import widgetGeometrySource from './widgets/widget-geometry.ts?raw';
import { MERC_OVERLAY_DEFAULTS, physicalGeometry } from './overlay-defaults';

describe('shipped merc geometry', () => {
	// A guard on the constants themselves: emptied, the widget would ship at the origin.
	it('ships a placement with a real position and width', () => {
		expect(MERC_OVERLAY_DEFAULTS.w).toBeGreaterThan(0);
		expect(MERC_OVERLAY_DEFAULTS.x).toBeGreaterThan(0);
		expect(MERC_OVERLAY_DEFAULTS.y).toBeGreaterThan(0);
	});

	// `h` is the config-mode seed and the clamp extent for a content-sized widget,
	// so it stays one line tall.
	it('seeds a height no taller than a single line of the strip', () => {
		expect(MERC_OVERLAY_DEFAULTS.h).toBeGreaterThan(0);
		expect(MERC_OVERLAY_DEFAULTS.h).toBeLessThanOrEqual(60);
	});
});

describe('CSS pixels to physical pixels', () => {
	it('leaves an unscaled display alone', () => {
		expect(physicalGeometry({ x: 40, y: 300, w: 460, h: 40 }, 1)).toEqual({
			x: 40,
			y: 300,
			w: 460,
			h: 40
		});
	});

	// The 150 % Windows display this exists for: shipping 460 as a physical
	// width made the strip a third narrower than its content needed.
	it('scales every field on a 150 per cent display', () => {
		expect(physicalGeometry({ x: 40, y: 300, w: 460, h: 40 }, 1.5)).toEqual({
			x: 60,
			y: 450,
			w: 690,
			h: 60
		});
	});

	it('rounds rather than truncating a fractional result', () => {
		expect(physicalGeometry({ x: 0, y: 0, w: 461, h: 0 }, 1.25).w).toBe(576);
	});

	// `scaleFactor()` failing is handled by callers falling back to 1, but a
	// zero reaching here would collapse the window to nothing.
	it('treats a nonsensical scale factor as unscaled', () => {
		expect(physicalGeometry({ x: 40, y: 300, w: 460, h: 40 }, 0).w).toBe(460);
	});

	// The second consumer of the conversion (POE-225). The widget engine reasons
	// its shipped placements in CSS and persists them physical, which is the
	// same boundary and therefore must be the same rounding — a local
	// `Math.floor(x * sf)` there would put widgets a pixel off the rectangles
	// the settings page and the host each computed separately.
	it('is what the widget engine converts a placement with', () => {
		expect(widgetGeometrySource).toContain("from '../overlay-defaults'");
		expect(widgetGeometrySource).toMatch(/physicalGeometry\(\s*rect,\s*scaleFactor\s*\)/);
	});
});
