/**
 * The widget engine's arithmetic, at the two scale factors that matter and at
 * the boundaries a drag actually reaches.
 *
 * Every number here is one an overlay window would produce and nothing else can
 * check: the host has no test harness, so a widget placed a hundred pixels off
 * or one that walks a pixel per save is invisible until it is on screen over
 * the game.
 */
import { describe, expect, it } from 'vitest';
import {
	BASE_EDGE,
	MIN_WIDGET_SIDE_CSS,
	clampToHost,
	cssRect,
	dragged,
	edgeAt,
	edgeFor,
	gestureResized,
	placementFor,
	resized,
	seedRect,
	sizeToPersist,
	widgetGeometry
} from './widget-geometry';
import type { WidgetSpec } from './widget-registry';

const HOST = { width: 1920, height: 1080 };

/** A resizable widget with a shipped default well away from the origin, so a
 *  placement that silently falls back to `{0, 0}` cannot pass. */
const RESIZABLE: WidgetSpec = {
	id: 'test.resizable',
	module: 'test',
	label: 'Resizable',
	defaults: { x: 250, y: 40, w: 400, h: 200 },
	resizable: true
};

const FIXED: WidgetSpec = { ...RESIZABLE, id: 'test.fixed', resizable: false };

describe('physical pixels to CSS pixels', () => {
	it('leaves an unscaled display alone', () => {
		expect(cssRect({ x: 250, y: 40, width: 400, height: 200, visible: true }, 1)).toEqual({
			x: 250,
			y: 40,
			w: 400,
			h: 200
		});
	});

	it('divides every field on a 150 per cent display', () => {
		expect(cssRect({ x: 375, y: 60, width: 600, height: 300, visible: true }, 1.5)).toEqual({
			x: 250,
			y: 40,
			w: 400,
			h: 200
		});
	});

	// The reason CSS is left fractional. 61 physical px at 150 % is 40.667 CSS
	// px; rounding that to 41 and converting back gives 62, so opening and
	// saving config mode without touching anything would walk the widget right
	// by a pixel every time.
	it('keeps a fractional result so a save-without-moving does not drift', () => {
		const rect = cssRect({ x: 61, y: 61, width: 300, height: 300, visible: true }, 1.5);
		expect(widgetGeometry(rect!, 1.5, true).x).toBe(61);
	});

	// FAILS CLOSED, like `hot-rects.ts`. Substituting 1 for an unresolved factor
	// is not "no answer", it is a confident wrong one: on a 150 % display every
	// stored widget would be drawn a third too far out, which reads as a
	// placement the user got wrong rather than as a conversion that never ran.
	it('declines the conversion while the scale factor has not resolved', () => {
		expect(cssRect({ x: 40, y: 40, width: 200, height: 200, visible: true }, 0)).toBeNull();
	});

	it('declines the conversion when the scale factor is not a usable number', () => {
		expect(cssRect({ x: 40, y: 40, width: 200, height: 200, visible: true }, NaN)).toBeNull();
	});
});

describe('CSS pixels to a persistable geometry', () => {
	it('scales and rounds every field on a 150 per cent display', () => {
		expect(widgetGeometry({ x: 250, y: 40, w: 400, h: 200 }, 1.5, true)).toEqual({
			x: 375,
			y: 60,
			width: 600,
			height: 300,
			visible: true
		});
	});

	it('carries the hidden flag through rather than defaulting it', () => {
		expect(widgetGeometry({ x: 0, y: 0, w: 10, h: 10 }, 1, false).visible).toBe(false);
	});

	// `PhysicalSize` is unsigned on the Rust side, so a negative measurement
	// must not be sent as one.
	it('floors a negative measurement at zero rather than sending it to Rust', () => {
		expect(widgetGeometry({ x: 0, y: 0, w: -30, h: -30 }, 1, true)).toMatchObject({
			width: 0,
			height: 0
		});
	});
});

describe('keeping a widget inside the window', () => {
	it('leaves a rectangle that already fits where it is', () => {
		expect(clampToHost({ x: 250, y: 40, w: 400, h: 200 }, HOST)).toEqual({
			x: 250,
			y: 40,
			w: 400,
			h: 200
		});
	});

	it('pulls a rectangle past the right edge back so its far edge lands on it', () => {
		expect(clampToHost({ x: 1900, y: 40, w: 400, h: 200 }, HOST).x).toBe(1520);
	});

	it('pulls a rectangle past the bottom edge back so its far edge lands on it', () => {
		expect(clampToHost({ x: 250, y: 1000, w: 400, h: 200 }, HOST).y).toBe(880);
	});

	it('pins a negative origin to the top-left corner', () => {
		expect(clampToHost({ x: -50, y: -50, w: 400, h: 200 }, HOST)).toMatchObject({ x: 0, y: 0 });
	});

	// A widget wider than the window pins to the left and hangs off the right,
	// which is visible and recoverable; narrowing it would look like clipped
	// content.
	it('pins a widget wider than the window to the left edge without shrinking it', () => {
		expect(clampToHost({ x: 300, y: 0, w: 2400, h: 200 }, HOST)).toMatchObject({ x: 0, w: 2400 });
	});
});

describe('which edge the pointer is grabbing', () => {
	const rect = { x: 100, y: 100, w: 400, h: 200 };

	it('reads the interior as a move', () => {
		expect(edgeAt(rect, 200, 100)).toBeNull();
	});

	it.each([
		['North', 200, 2],
		['South', 200, 198],
		['West', 3, 100],
		['East', 397, 100]
	] as const)('reads a %s edge', (edge, ox, oy) => {
		expect(edgeAt(rect, ox, oy)).toBe(edge);
	});

	// The zone is the config window's, so the two drag surfaces behave the same.
	it('leaves the interior alone one pixel inside the grab zone', () => {
		expect(edgeAt(rect, 200, BASE_EDGE)).toBeNull();
	});

	// Corners beat sides, or the diagonal handles would never be reachable.
	it.each([
		['NorthWest', 2, 2],
		['NorthEast', 398, 2],
		['SouthWest', 2, 198],
		['SouthEast', 398, 198]
	] as const)('reads a %s corner rather than either side', (edge, ox, oy) => {
		expect(edgeAt(rect, ox, oy)).toBe(edge);
	});

	// The grab zone is capped at an eighth of the box, so a small widget keeps
	// an interior to drag from instead of being all handle.
	it('leaves the middle of a small widget draggable', () => {
		expect(edgeAt({ x: 0, y: 0, w: 40, h: 40 }, 20, 20)).toBeNull();
	});

	it('reads an edge on a small widget from within its narrowed zone', () => {
		expect(edgeAt({ x: 0, y: 0, w: 40, h: 40 }, 20, 2)).toBe('North');
	});

	// Below 8 px a side there is no zone left at all; every press must be a
	// move, because a zero-wide zone would otherwise make `> h - 0` true for
	// every pointer and turn the whole box into a South edge.
	it('treats a widget too small for any grab zone as all interior', () => {
		expect(edgeAt({ x: 0, y: 0, w: 6, h: 6 }, 5, 5)).toBeNull();
	});
});

describe('whether a widget offers a resize edge at all', () => {
	const rect = { x: 100, y: 100, w: 400, h: 200 };

	it('reads the edge of a resizable widget', () => {
		expect(edgeFor(RESIZABLE, rect, 200, 2)).toBe('North');
	});

	// The spec's `resizable: false` is the whole contract for a content-sized
	// widget: an edge handle there writes a size `placementFor` then refuses to
	// apply, so the widget snaps back and the drag looks broken.
	it('reads every edge of a non-resizable widget as interior, so only moves happen', () => {
		expect(edgeFor(FIXED, rect, 200, 2)).toBeNull();
		expect(edgeFor(FIXED, rect, 398, 198)).toBeNull();
	});

	it('still reads the interior of a resizable widget as a move', () => {
		expect(edgeFor(RESIZABLE, rect, 200, 100)).toBeNull();
	});
});

describe('dragging a widget', () => {
	const start = { x: 250, y: 40, w: 400, h: 200 };

	it('moves it by exactly the pointer delta', () => {
		expect(dragged(start, 100, -20, HOST)).toEqual({ x: 350, y: 20, w: 400, h: 200 });
	});

	it('stops at the right edge instead of leaving the window', () => {
		expect(dragged(start, 5000, 0, HOST)).toMatchObject({ x: 1520, y: 40 });
	});

	// The reducer reads the rectangle the drag STARTED from, so running into an
	// edge and coming back returns to where the pointer is rather than lagging
	// by everything the clamp ate.
	it('returns to the pointer after a drag that ran into an edge and came back', () => {
		expect(dragged(start, 5000, 0, HOST).x).toBe(1520);
		expect(dragged(start, 100, 0, HOST).x).toBe(350);
	});
});

describe('resizing a widget', () => {
	const start = { x: 250, y: 40, w: 400, h: 200 };

	it('grows the width from the east edge without moving the origin', () => {
		expect(resized(start, 'East', 60, 0, HOST)).toEqual({ x: 250, y: 40, w: 460, h: 200 });
	});

	it('grows the height from the south edge without moving the origin', () => {
		expect(resized(start, 'South', 0, 50, HOST)).toEqual({ x: 250, y: 40, w: 400, h: 250 });
	});

	// A west drag moves the origin AND changes the size, and getting the sign
	// wrong on either half moves the widget instead of resizing it.
	it('moves the origin and widens by the same amount from the west edge', () => {
		expect(resized(start, 'West', -60, 0, HOST)).toEqual({ x: 190, y: 40, w: 460, h: 200 });
	});

	it('moves the origin and heightens by the same amount from the north edge', () => {
		expect(resized(start, 'North', 0, -30, HOST)).toEqual({ x: 250, y: 10, w: 400, h: 230 });
	});

	it('resizes both axes from a corner', () => {
		expect(resized(start, 'SouthEast', 60, 50, HOST)).toEqual({
			x: 250,
			y: 40,
			w: 460,
			h: 250
		});
	});

	it('stops at the minimum side when the east edge is dragged past the west one', () => {
		expect(resized(start, 'East', -1000, 0, HOST).w).toBe(MIN_WIDGET_SIDE_CSS);
	});

	// Inverting from the far side is the one that also has to leave the origin
	// somewhere sane: a west drag past the east edge must stop with the widget
	// still `MIN` wide and still ending where it did.
	it('stops at the minimum side when the west edge is dragged past the east one', () => {
		expect(resized(start, 'West', 1000, 0, HOST)).toMatchObject({
			x: 650 - MIN_WIDGET_SIDE_CSS,
			w: MIN_WIDGET_SIDE_CSS
		});
	});

	it('will not grow the east edge past the window', () => {
		expect(resized({ x: 1500, y: 40, w: 400, h: 200 }, 'East', 1000, 0, HOST).w).toBe(420);
	});

	it('will not drag the north edge above the window', () => {
		expect(resized(start, 'North', 0, -1000, HOST)).toMatchObject({ y: 0, h: 240 });
	});
});

describe('deciding where a widget goes', () => {
	it('uses the shipped CSS default, content-sized, when nothing is stored', () => {
		expect(placementFor(RESIZABLE, undefined, 1.5, HOST)).toMatchObject({
			x: 250,
			y: 40,
			width: null,
			height: null
		});
	});

	// Fails closed with `cssRect`. The frames before `scaleFactor()` answers are
	// a handful; a widget drawn a third off on a 150 % display for those frames
	// is indistinguishable from one the user placed badly.
	it('renders nothing for a stored widget while the scale factor has not resolved', () => {
		expect(
			placementFor(RESIZABLE, { x: 375, y: 60, width: 600, height: 300, visible: true }, 0, HOST)
		).toBeNull();
	});

	// The unstored branch converts nothing, so an unconfigured widget is drawn
	// at the registry's CSS numbers from the first frame.
	it('still uses the shipped default while the scale factor has not resolved', () => {
		expect(placementFor(RESIZABLE, undefined, 0, HOST)).toMatchObject({ x: 250, y: 40 });
	});

	it('renders nothing for a widget the user has hidden', () => {
		expect(
			placementFor(
				RESIZABLE,
				{ x: 375, y: 60, width: 600, height: 300, visible: false },
				1.5,
				HOST
			)
		).toBeNull();
	});

	it('converts a stored placement to CSS px for a scaled display', () => {
		expect(
			placementFor(RESIZABLE, { x: 375, y: 60, width: 600, height: 300, visible: true }, 1.5, HOST)
		).toMatchObject({ x: 250, y: 40, width: 400, height: 200 });
	});

	// A non-resizable widget's stored size is whatever its content happened to
	// measure when the user last saved; pinning the box to it would clip the
	// content the moment it grew a line.
	it('keeps a non-resizable widget content-sized even with a stored size', () => {
		expect(
			placementFor(FIXED, { x: 375, y: 60, width: 600, height: 300, visible: true }, 1.5, HOST)
		).toMatchObject({ x: 250, y: 40, width: null, height: null });
	});

	// A row written before the widget was resizable, or by a hand edit, has a
	// zero size — applying it would collapse the widget to nothing on screen.
	it('falls back to content sizing when the stored size is empty', () => {
		expect(
			placementFor(RESIZABLE, { x: 375, y: 60, width: 0, height: 0, visible: true }, 1.5, HOST)
		).toMatchObject({ x: 250, y: 40, width: null, height: null });
	});
});

describe('the ceiling a content-sized widget wraps at', () => {
	// Without one, content sizing inside a monitor-sized host is `max-content`:
	// a one-line headline runs the full width of the screen.
	it('caps an unconfigured resizable widget at the width the registry ships', () => {
		expect(placementFor(RESIZABLE, undefined, 1.5, HOST)!.maxWidth).toBe(RESIZABLE.defaults.w);
	});

	it('caps a stored-but-unsized resizable widget at the same width', () => {
		expect(
			placementFor(RESIZABLE, { x: 375, y: 60, width: 0, height: 0, visible: true }, 1.5, HOST)!
				.maxWidth
		).toBe(RESIZABLE.defaults.w);
	});

	// The user's own width is the ceiling once there is one; keeping the shipped
	// number would silently refuse a widget the user made wider.
	it('drops the ceiling once the widget has a size of its own', () => {
		expect(
			placementFor(RESIZABLE, { x: 375, y: 60, width: 900, height: 300, visible: true }, 1.5, HOST)
		).toMatchObject({ width: 600, maxWidth: null });
	});

	it('leaves a non-resizable widget uncapped, since its content decides both axes', () => {
		expect(placementFor(FIXED, undefined, 1.5, HOST)!.maxWidth).toBeNull();
	});
});

describe('a stored placement the current monitor cannot hold', () => {
	// The regression this is written against: a placement saved on a 4K monitor,
	// reopened at 1080p. Nothing on screen, and the Settings row that would fix
	// it shows numbers the user has no way to relate to an empty screen.
	const OFFSCREEN = { x: 3000, y: 1800, width: 0, height: 0, visible: true };

	it('pulls a widget stored past the bottom-right back into the window', () => {
		expect(placementFor(RESIZABLE, OFFSCREEN, 1, HOST)).toMatchObject({
			x: HOST.width - RESIZABLE.defaults.w,
			y: HOST.height - RESIZABLE.defaults.h
		});
	});

	it('clamps against the size that will actually be applied, not the shipped one', () => {
		expect(
			placementFor(RESIZABLE, { x: 1900, y: 40, width: 800, height: 200, visible: true }, 1, HOST)
		).toMatchObject({ x: HOST.width - 800, width: 800 });
	});

	it('leaves a placement that already fits exactly where it was stored', () => {
		expect(
			placementFor(RESIZABLE, { x: 375, y: 60, width: 600, height: 300, visible: true }, 1.5, HOST)
		).toMatchObject({ x: 250, y: 40 });
	});

	// The host measures itself from a `resize` listener, so the first frame has
	// no size at all. Clamping against it would put every widget at the origin.
	it('clamps nothing while the host has not measured itself yet', () => {
		expect(placementFor(RESIZABLE, OFFSCREEN, 1, { width: 0, height: 0 })).toMatchObject({
			x: 3000,
			y: 1800
		});
	});
});

describe('the rectangle config mode opens a widget at', () => {
	const STORED = { x: 900, y: 600, width: 0, height: 0, visible: true };
	const MEASURED = { x: 900, y: 600, w: 312, h: 96 };

	// The regression: a widget that is not on screen has no measured box, and
	// seeding it from the defaults meant any Save — of any widget in the module —
	// wrote it back to where it shipped.
	it('seeds an unrendered widget from its stored position, not from the defaults', () => {
		expect(seedRect(RESIZABLE, STORED, null, 1, HOST)).toMatchObject({ x: 900, y: 600 });
	});

	it('seeds a rendered widget from its stored position too', () => {
		expect(seedRect(RESIZABLE, STORED, MEASURED, 1, HOST)).toMatchObject({ x: 900, y: 600 });
	});

	it('converts the stored position out of physical px', () => {
		expect(
			seedRect(RESIZABLE, { x: 900, y: 600, width: 0, height: 0, visible: true }, null, 1.5, HOST)
		).toMatchObject({ x: 600, y: 400 });
	});

	it('falls back to the shipped default when nothing is stored and nothing is rendered', () => {
		expect(seedRect(RESIZABLE, undefined, null, 1, HOST)).toEqual(RESIZABLE.defaults);
	});

	it('prefers what an unstored widget actually measures on screen', () => {
		expect(seedRect(RESIZABLE, undefined, MEASURED, 1, HOST)).toEqual(MEASURED);
	});

	// A content-sized widget stores 0 × 0, and a frame drawn at zero has no
	// interior to grab and no edge to pull.
	it('takes the size from the measured box when the stored size is empty', () => {
		expect(seedRect(RESIZABLE, STORED, MEASURED, 1, HOST)).toMatchObject({ w: 312, h: 96 });
	});

	it('takes the size from the shipped default when there is neither', () => {
		expect(seedRect(RESIZABLE, STORED, null, 1, HOST)).toMatchObject({
			w: RESIZABLE.defaults.w,
			h: RESIZABLE.defaults.h
		});
	});

	it('uses the stored size when the widget has a real one', () => {
		expect(
			seedRect(RESIZABLE, { x: 900, y: 600, width: 600, height: 300, visible: true }, MEASURED, 1.5, HOST)
		).toMatchObject({ w: 400, h: 200 });
	});

	// The widget IS rendered and drawing nothing — the temple's board outside a
	// temple. `??` accepts a zero, so the box has to be rejected on its area:
	// a frame opened at 0 × 0 has no interior to drag and no edge to pull.
	it('treats a widget that measures nothing as unmeasured and uses the default size', () => {
		expect(seedRect(RESIZABLE, STORED, { x: 900, y: 600, w: 0, h: 0 }, 1, HOST)).toMatchObject({
			w: RESIZABLE.defaults.w,
			h: RESIZABLE.defaults.h
		});
	});

	it('keeps the stored position of a widget that measures nothing', () => {
		expect(seedRect(RESIZABLE, STORED, { x: 0, y: 0, w: 0, h: 0 }, 1, HOST)).toMatchObject({
			x: 900,
			y: 600
		});
	});

	it('falls back to the shipped placement when nothing is stored and nothing measures', () => {
		expect(seedRect(RESIZABLE, undefined, { x: 0, y: 0, w: 0, h: 0 }, 1, HOST)).toEqual(
			RESIZABLE.defaults
		);
	});

	// One zero side is enough: a box with no height is as ungrabbable as one
	// with no area at all.
	it('rejects a measurement with one degenerate side', () => {
		expect(seedRect(RESIZABLE, STORED, { x: 900, y: 600, w: 312, h: 0 }, 1, HOST)).toMatchObject({
			w: RESIZABLE.defaults.w,
			h: RESIZABLE.defaults.h
		});
	});

	// Fails closed with `cssRect`: a frame opened at an assumed 1x is a frame in
	// the wrong place, and Save refuses at an unresolved scale factor anyway.
	it('declines to seed a stored widget while the scale factor has not resolved', () => {
		expect(seedRect(RESIZABLE, STORED, MEASURED, 0, HOST)).toBeNull();
	});

	// The unstored branch converts nothing, so it still answers.
	it('still seeds an unstored widget while the scale factor has not resolved', () => {
		expect(seedRect(RESIZABLE, undefined, MEASURED, 0, HOST)).toEqual(MEASURED);
	});

	it('pulls a seed stored off the current monitor back inside it', () => {
		expect(
			seedRect(RESIZABLE, { x: 4000, y: 3000, width: 0, height: 0, visible: true }, null, 1, HOST)
		).toMatchObject({ x: HOST.width - RESIZABLE.defaults.w, y: HOST.height - RESIZABLE.defaults.h });
	});
});

describe('whether a pointer move counts as a resize', () => {
	it('counts a drag on an edge', () => {
		expect(gestureResized('East', 60, 0)).toBe(true);
	});

	it('counts a drag on an edge along the other axis too', () => {
		expect(gestureResized('South', 0, 40)).toBe(true);
	});

	// A press on the border and a release without movement still delivers a
	// move event. Counting it would pin the widget to whatever its content
	// measured that day — the content-sizing contract ended by a click.
	it('does not count a border press that never moved', () => {
		expect(gestureResized('East', 0, 0)).toBe(false);
	});

	it('does not count a move gesture, however far it went', () => {
		expect(gestureResized(null, 300, 300)).toBe(false);
	});
});

describe('the size Save writes', () => {
	const RECT = { x: 250, y: 40, w: 460, h: 250 };

	// The contract from `widget-registry.ts`: a widget is content-sized until the
	// user drags an edge. Persisting the measured size on every Save would pin
	// every widget in the module the first time any one of them was moved.
	it('drops the size of a widget this session only moved', () => {
		expect(sizeToPersist(RESIZABLE, RECT, false, undefined)).toEqual({
			x: 250,
			y: 40,
			w: 0,
			h: 0
		});
	});

	it('keeps the size of a widget this session resized', () => {
		expect(sizeToPersist(RESIZABLE, RECT, true, undefined)).toEqual(RECT);
	});

	// A size the user set in an earlier session is still the user's size; a later
	// move-only Save must not throw it away.
	it('keeps a size an earlier session had already stored', () => {
		expect(
			sizeToPersist(RESIZABLE, RECT, false, { x: 0, y: 0, width: 600, height: 300, visible: true })
		).toEqual(RECT);
	});

	it('drops the size again when the stored row is the content-sized zero', () => {
		expect(
			sizeToPersist(RESIZABLE, RECT, false, { x: 0, y: 0, width: 0, height: 0, visible: true })
		).toMatchObject({ w: 0, h: 0 });
	});

	it('never keeps a size for a widget that has no resize handles', () => {
		expect(
			sizeToPersist(FIXED, RECT, true, { x: 0, y: 0, width: 600, height: 300, visible: true })
		).toMatchObject({ w: 0, h: 0 });
	});

	it('leaves the position alone whichever way the size goes', () => {
		expect(sizeToPersist(RESIZABLE, RECT, false, undefined)).toMatchObject({ x: 250, y: 40 });
	});
});
