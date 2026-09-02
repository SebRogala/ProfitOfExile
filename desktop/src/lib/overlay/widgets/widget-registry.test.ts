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
import { WIDGETS, placeableWidgetsFor, widgetsFor } from './widget-registry';

describe('every declared widget', () => {
	it('declares at least one, so the assertions below are not vacuous', () => {
		expect(WIDGETS.length).toBeGreaterThan(0);
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
	it('declares the board and the advice widget under its window label', () => {
		expect(widgetsFor(TEMPLE_WINDOW_LABEL).map((widget) => widget.id)).toEqual([
			'temple.board',
			'temple.advice'
		]);
	});

	it('ships both of them resizable, so config mode offers edge handles', () => {
		expect(widgetsFor(TEMPLE_WINDOW_LABEL).map((widget) => widget.resizable)).toEqual([
			true,
			true
		]);
	});

	// The two are placed side by side out of the box: the advice column starts
	// to the right of the board rather than on top of it.
	it('ships the advice widget clear of the board', () => {
		const board = WIDGETS.find((widget) => widget.id === 'temple.board');
		const advice = WIDGETS.find((widget) => widget.id === 'temple.advice');
		expect(advice!.defaults.x).toBeGreaterThanOrEqual(board!.defaults.x + board!.defaults.w);
	});

	it('answers nothing for a module that declares no widgets', () => {
		expect(widgetsFor('nothing-declares-this')).toEqual([]);
	});
});

describe('the widgets a host actually places', () => {
	it('leaves out an anchored widget, which the geometry engine owns', () => {
		// Nothing ships anchored yet, so the filter is checked against a spec
		// built here rather than against the registry — otherwise the assertion
		// would be vacuous until the first anchored widget exists, which is
		// exactly when a broken filter would first matter.
		const anchored = { ...WIDGETS[0], id: 'temple.anchored' as const, anchored: true };
		const all = [...WIDGETS, anchored];
		expect(all.filter((widget) => !widget.anchored).map((w) => w.id)).not.toContain(
			'temple.anchored'
		);
		expect(placeableWidgetsFor(TEMPLE_WINDOW_LABEL).every((widget) => !widget.anchored)).toBe(
			true
		);
	});

	it('places every temple widget today, none of which is anchored', () => {
		expect(placeableWidgetsFor(TEMPLE_WINDOW_LABEL).length).toBe(
			widgetsFor(TEMPLE_WINDOW_LABEL).length
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
	});
});
