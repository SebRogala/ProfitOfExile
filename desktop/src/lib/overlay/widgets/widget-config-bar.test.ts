/**
 * Where the widget-config Save/Cancel bar lands (POE-245).
 *
 * The regression behind every case here: the bar was pinned to the bottom edge
 * of a MONITOR-SIZED window, centred on the screen. The widgets being arranged
 * are somewhere else entirely — the owner's own placements sit at y = 27 and
 * y = 430 on a 1080p screen — so the only controls that window has were a
 * thousand pixels from the thing they act on, and the report was that they could
 * not be found at all.
 *
 * These are the numbers an overlay window would produce and nothing else can
 * check: the host has no test harness in this app, so a bar placed off screen or
 * back under the widgets is invisible until it is over the game.
 */
import { describe, expect, it } from 'vitest';
import { CONFIG_BAR_GAP, configBarAnchor } from './widget-config-bar';
import { placeableWidgetsFor } from './widget-registry';
import { TEMPLE_WINDOW_LABEL } from '../manager';

const HOST = { width: 1920, height: 1080 };

/** A plausible measured bar: two lines of 14 px copy plus two 32 px buttons. */
const BAR = { width: 320, height: 56 };

describe('placing the widget-config bar', () => {
	it('puts the bar above the widgets when there is room for it', () => {
		expect(configBarAnchor([{ x: 400, y: 300, w: 200, h: 200 }], HOST, BAR)).toEqual({
			// 400 + 100 (cluster centre) - 160 (half the bar)
			x: 340,
			// 300 - 16 (gap) - 56 (the bar itself)
			y: 228
		});
	});

	/**
	 * The whole point of the change. A screen-centred bar would be at
	 * x = 1920 / 2 - 160 = 800, which is where the shipped one was and is nowhere
	 * near a widget the owner actually placed (the one in this case is at x = 1400).
	 */
	it('centres the bar on the widgets rather than on the screen', () => {
		expect(configBarAnchor([{ x: 1400, y: 400, w: 200, h: 200 }], HOST, BAR).x).toBe(1340);
	});

	/** A widget near the top of the host — which is where a shipped default
	 *  tends to be — so "no room above" is the ordinary case, not an edge one. */
	it('drops the bar below the widgets when they sit too near the top', () => {
		expect(configBarAnchor([{ x: 400, y: 40, w: 200, h: 200 }], HOST, BAR)).toEqual({
			x: 340,
			// 40 + 200 (the cluster's bottom) + 16
			y: 256
		});
	});

	/** The boundary between the two: exactly a bar plus a gap still goes above. */
	it('takes the space above when it is exactly a gap and a bar tall', () => {
		expect(configBarAnchor([{ x: 400, y: 72, w: 200, h: 200 }], HOST, BAR).y).toBe(0);
	});

	it('drops below when one pixel of that space is missing', () => {
		expect(configBarAnchor([{ x: 400, y: 71, w: 200, h: 200 }], HOST, BAR).y).toBe(287);
	});

	/** A cluster that spans the screen leaves nowhere clear. Overlapping a
	 *  widget is survivable — the bar is drawn over them — being off screen is
	 *  not. */
	it('falls back to the top of the host when the widgets leave room on neither side', () => {
		expect(configBarAnchor([{ x: 400, y: 10, w: 200, h: 1060 }], HOST, BAR)).toEqual({
			x: 340,
			y: CONFIG_BAR_GAP
		});
	});

	/** Save hanging off the right of the monitor is the shipped bug again with a
	 *  different cause. */
	it('keeps the whole bar on screen when the widgets hug an edge', () => {
		expect(configBarAnchor([{ x: 1850, y: 400, w: 60, h: 60 }], HOST, BAR).x).toBe(
			HOST.width - BAR.width
		);
	});

	/**
	 * A module with more than one placeable widget: the cluster has to be their
	 * UNION. Two boxes at (40, 40) 200x200 and (250, 40) 400x200 make
	 * 40..650 x 40..240, and because both sit at y = 40 the bar goes BELOW the
	 * cluster. A union that took the minimum of the two right edges instead of
	 * the maximum would centre the bar on the first alone and push it off the
	 * left of the host.
	 */
	it('spans both widgets when the module ships more than one', () => {
		expect(
			configBarAnchor(
				[
					{ x: 40, y: 40, w: 200, h: 200 },
					{ x: 250, y: 40, w: 400, h: 200 }
				],
				HOST,
				BAR
			)
		).toEqual({
			// (40 + 650) / 2 - 160
			x: 185,
			// 240 (the lower of the two bottoms is 240 too) + 16
			y: 256
		});
	});

	/**
	 * The vertical half of the same union, on numbers chosen for it rather than
	 * on the shipped ones: what decides the branch at the shipped defaults is
	 * the cluster's TOP (see the last case in this file), so the bottom rule
	 * needs a case of its own. A union that took the minimum of the two bottoms
	 * would place the bar across the taller widget instead of below the pair.
	 */
	it('clears the lower widget when the two have different bottoms', () => {
		expect(
			configBarAnchor(
				[
					{ x: 40, y: 40, w: 200, h: 200 },
					{ x: 250, y: 40, w: 400, h: 500 }
				],
				HOST,
				BAR
			).y
		).toBe(556);
	});

	/**
	 * A content-sized widget's draft rectangle is `0 x 0` until it has been
	 * measured. Folding one into the cluster would drag the bounding box to the
	 * origin and place the bar against a corner no widget is in.
	 */
	it('ignores a widget with no measured size when it finds the cluster', () => {
		expect(
			configBarAnchor(
				[
					{ x: 0, y: 0, w: 0, h: 0 },
					{ x: 400, y: 300, w: 200, h: 200 }
				],
				HOST,
				BAR
			)
		).toEqual({ x: 340, y: 228 });
	});

	/**
	 * An empty draft, which is an unresolved scale factor and not the Show
	 * checkbox: `seedRect` ignores `visible` and `enterConfig` seeds every spec,
	 * but a widget whose STORED placement cannot be converted is left out. With
	 * every widget stored, the bar is the only thing on screen — so it goes where
	 * a header goes.
	 */
	it('goes to the top centre of the host when there are no widgets to anchor to', () => {
		expect(configBarAnchor([], HOST, BAR)).toEqual({ x: 800, y: CONFIG_BAR_GAP });
	});

	/**
	 * What the SHIPPED defaults actually get, which no other case here checks:
	 * every rectangle above is hand-built, so the registry could ship a widget
	 * the bar lands on top of and nothing would say so.
	 *
	 * The two placeable temple widgets ship at `temple.door` (40, 300) 190x215
	 * and `temple.waiting` (830, 16) 260x40, so the cluster is
	 * 40..1090 x 16..515. There is no room for a bar above y = 16, so the answer
	 * is branch 2 — BELOW the cluster — at y = 16 + 499 + 16 = 531, centred on
	 * the union at 40 + 525 - 160 = 405.
	 *
	 * Pinned as the ANSWER rather than as the branch: a widget added anywhere
	 * near the top or the bottom of the screen changes it, and the point is that
	 * the change is visible here rather than as a Save button under the user's
	 * hand.
	 */
	it('drops below the cluster the shipped temple defaults make', () => {
		const shipped = placeableWidgetsFor(TEMPLE_WINDOW_LABEL).map((widget) => ({
			x: widget.defaults.x,
			y: widget.defaults.y,
			w: widget.defaults.w,
			h: widget.defaults.h
		}));
		expect(configBarAnchor(shipped, HOST, BAR)).toEqual({ x: 405, y: 531 });
	});

	/**
	 * WHICH widget decides that, since the answer above is a property of the
	 * pair and not of either one. `temple.waiting` ships 16 px from the top of
	 * the monitor (POE-249) — a bar plus a gap does not fit above it — and the
	 * door alone would still take branch 1, at 300 - 16 - 56.
	 */
	it('takes the space above again once the top widget is out of the cluster', () => {
		const door = placeableWidgetsFor(TEMPLE_WINDOW_LABEL).find(
			(widget) => widget.id === 'temple.door'
		);
		expect(door).toBeDefined();
		expect(
			configBarAnchor(
				[{ x: door!.defaults.x, y: door!.defaults.y, w: door!.defaults.w, h: door!.defaults.h }],
				HOST,
				BAR
			).y
		).toBe(228);
	});

	/** Totality, not a frame anyone has seen: the host measures itself in an
	 *  on-mount effect. Centring against a zero width would be x = -160 — off the
	 *  left of the monitor — so the rule has to answer it. */
	it('stays in the corner while the host has not measured itself', () => {
		expect(configBarAnchor([{ x: 400, y: 300, w: 200, h: 200 }], { width: 0, height: 0 }, BAR)).toEqual(
			{ x: CONFIG_BAR_GAP, y: CONFIG_BAR_GAP }
		);
	});
});
