/**
 * The widget registry's invariants — the ones that fail as a widget that never
 * appears rather than as a type error.
 *
 * Three of them cross a boundary TypeScript cannot see. The id's module half is
 * how Rust's `get_widget_geometries` finds a module's rows (`widgets_for_module`
 * matches on the `"<module>."` prefix), so an id whose halves disagree with its
 * `module` field persists fine and is never read back. The module is also the
 * WINDOW LABEL, so a module nothing creates a window for has widgets nothing
 * can draw. And the shipped placement has exactly one home, the way
 * `MERC_OVERLAY_DEFAULTS` does — a second copy in the host is how that geometry
 * drifted the first time (`../overlay-defaults.test.ts`).
 */
import { describe, expect, it } from 'vitest';
import hostSource from './WidgetHost.svelte?raw';
import { TEMPLE_WINDOW_LABEL } from '../manager';
import {
	WIDGETS,
	anchoredWidgetsFor,
	placeableWidgetsFor,
	widgetsFor
} from './widget-registry';
import { placementFor, type WidgetGeometry } from './widget-geometry';

describe('every declared widget', () => {
	// The whole registry, not the temple filter below it: a widget added to
	// another module — or a third temple one — is a row in Settings, a
	// persisted rectangle and a snippet branch the host will not find, so it has
	// to be a deliberate edit here rather than something that arrives past a
	// filter that never looked.
	//
	// `temple.board` is deliberately ABSENT since POE-244. The lattice is drawn
	// on the Temple page and no longer over the game, where the board is already
	// on screen behind the window and the copy cost space that has to stay clear
	// of the module's own OCR crops.
	it('ships exactly the two temple widgets, and the board is not one of them', () => {
		expect(WIDGETS.map((widget) => widget.id)).toEqual(['temple.advice', 'temple.door']);
	});

	it.each(WIDGETS.map((widget) => [widget.id, widget] as const))(
		'%s spells its id as its own module plus a widget name',
		(_id, widget) => {
			const [module, ...rest] = widget.id.split('.');
			expect(module).toBe(widget.module);
			expect(rest.join('.')).not.toBe('');
		}
	);

	it('gives every widget a unique id, since the id is the persistence key', () => {
		expect(new Set(WIDGETS.map((widget) => widget.id)).size).toBe(WIDGETS.length);
	});

	it.each(WIDGETS.map((widget) => [widget.id, widget] as const))(
		'%s ships a placement with a real position and size',
		(_id, widget) => {
			// A default emptied to zero would put the widget at the origin at a
			// size nothing can grab, which reads on screen as "not there".
			expect(widget.defaults.x).toBeGreaterThan(0);
			expect(widget.defaults.y).toBeGreaterThan(0);
			expect(widget.defaults.w).toBeGreaterThan(0);
			expect(widget.defaults.h).toBeGreaterThan(0);
		}
	);

	it.each(WIDGETS.map((widget) => [widget.id, widget] as const))(
		'%s has a Settings row label',
		(_id, widget) => {
			expect(widget.label.trim()).not.toBe('');
		}
	);
});

describe('the temple module', () => {
	it('declares the kill callout and the door diamond under its window label', () => {
		expect(widgetsFor(TEMPLE_WINDOW_LABEL).map((widget) => widget.id)).toEqual([
			'temple.advice',
			'temple.door'
		]);
	});

	// The callout is placed by the module against the game, so a resize handle
	// on it would offer a size `placementFor` never applies and a drag the next
	// read would undo. The door diamond is the user's and keeps its handles.
	it('offers resize handles on the user-placed widget only', () => {
		expect(
			widgetsFor(TEMPLE_WINDOW_LABEL).map((widget) => [widget.id, widget.resizable])
		).toEqual([
			['temple.advice', false],
			['temple.door', true]
		]);
	});

	// `ships the advice widget clear of the board` was deleted with POE-244 and
	// deliberately not replaced. It asserted that two widgets did not overlap at
	// their shipped defaults, which was a real invariant while both were placed
	// side by side by the registry; neither half of that survives. The callout is
	// ANCHORED — its shipped position is never used, `calloutPlacement` decides
	// it per read — and the door diamond is the only placeable widget left, so
	// there is no second rectangle for it to be clear of. The overlap rule that
	// replaced it is `avoidRects`, and it is about read regions rather than about
	// other widgets.

	it('answers nothing for a module that declares no widgets', () => {
		expect(widgetsFor('nothing-declares-this')).toEqual([]);
	});
});

describe('the widgets a host actually places', () => {
	// The two filters partition the module's widgets, which is the invariant
	// that matters now that both halves are non-empty: a widget in neither is a
	// widget nothing draws, and one in both is drawn twice.
	it('splits the module\'s widgets into placed and anchored, with none in both', () => {
		expect(placeableWidgetsFor(TEMPLE_WINDOW_LABEL).map((w) => w.id)).toEqual(['temple.door']);
		expect(anchoredWidgetsFor(TEMPLE_WINDOW_LABEL).map((w) => w.id)).toEqual(['temple.advice']);
		expect(
			placeableWidgetsFor(TEMPLE_WINDOW_LABEL).length +
				anchoredWidgetsFor(TEMPLE_WINDOW_LABEL).length
		).toBe(widgetsFor(TEMPLE_WINDOW_LABEL).length);
	});

	it('keeps an anchored widget out of the placed list, which is what drops its Settings row', () => {
		// `overlay-groups.ts` lists exactly `placeableWidgetsFor`, and Rust
		// persists exactly what config mode saves — so this one filter is what
		// makes `anchored` mean "no row, no rectangle" everywhere at once.
		expect(placeableWidgetsFor(TEMPLE_WINDOW_LABEL).every((widget) => !widget.anchored)).toBe(
			true
		);
	});
});

describe('a placement stored for a widget the registry no longer declares', () => {
	// POE-244 retires `temple.board`. Its persisted rectangle stays in
	// `Settings.widgets` on every machine that ever arranged the widgets — Rust
	// does not validate ids against the frontend registry (`set_widget_geometry`
	// says why), and `get_widget_geometries` returns every row with the module's
	// prefix. The migration is therefore that the row is INERT, not that it is
	// removed, and this is what pins it: the host looks placements up BY SPEC,
	// so a row nothing declares is never read.
	const host = { width: 1920, height: 1080 };
	/** A map with a DECLARED widget's row in it as well as the retired one, so
	 *  the lookup being exercised is a real one. The previous version of this
	 *  test passed `stale[spec.id]`, which is `undefined` for every declared
	 *  spec — it compared `placementFor(spec, undefined)` with itself and would
	 *  have passed against any implementation at all. */
	const stored: Record<string, WidgetGeometry> = {
		'temple.door': { x: 900, y: 700, width: 0, height: 0, visible: true },
		'temple.board': { x: 40, y: 40, width: 200, height: 200, visible: true }
	};

	it('leaves every declared widget on its own stored placement', () => {
		for (const spec of placeableWidgetsFor(TEMPLE_WINDOW_LABEL)) {
			const placed = placementFor(spec, stored[spec.id], 1, host);
			// The door's OWN row is honoured — which is what makes the next
			// assertion meaningful, since a lookup that returned the stale row
			// by mistake would land at (40, 40) instead.
			expect(placed).toEqual({
				x: 900,
				y: 700,
				width: null,
				height: null,
				maxWidth: spec.defaults.w
			});
		}
	});

	it('is invisible to the placement of every widget that still exists', () => {
		// The migration itself: adding the retired row to the map changes
		// nothing, because the host looks up BY SPEC and no spec names it.
		const withStale = { ...stored };
		const withoutStale = { ...stored };
		delete withoutStale['temple.board'];
		for (const spec of placeableWidgetsFor(TEMPLE_WINDOW_LABEL)) {
			expect(placementFor(spec, withStale[spec.id], 1, host)).toEqual(
				placementFor(spec, withoutStale[spec.id], 1, host)
			);
		}
	});

	it('is not something any surface can look up, because nothing declares it', () => {
		expect(WIDGETS.map((widget) => widget.id)).not.toContain('temple.board');
		expect(placeableWidgetsFor(TEMPLE_WINDOW_LABEL).map((w) => w.id)).not.toContain(
			'temple.board'
		);
	});
});

describe('one home for the shipped widget geometry', () => {
	it('keeps the host free of geometry constants of its own', () => {
		// The specific regression `overlay-defaults.test.ts` was written for,
		// applied to the widget defaults: a second copy of the numbers in the
		// consumer, which then drifts from the registry and wins because the
		// consumer is what renders.
		expect(hostSource).not.toMatch(/const\s+\w*WIDGET\w*_DEFAULTS?\b/);
		expect(hostSource).not.toMatch(/defaults\s*:\s*\{/);
	});

	it('makes the host read the registry rather than a list of its own', () => {
		expect(hostSource).toContain("from './widget-registry'");
		expect(hostSource).toContain('placeableWidgetsFor(module)');
		// Both halves, since POE-244: a host that read only the placeable list
		// would drop every anchored widget with no error anywhere.
		expect(hostSource).toContain('anchoredWidgetsFor(module)');
	});
});
