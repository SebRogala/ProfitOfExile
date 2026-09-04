/**
 * What Settings → Overlay Positions lists, and what each row says.
 *
 * Both halves fail SILENTLY on screen, which is why they are tested here rather
 * than left to the page: a group drawn without its feature is a control that
 * places an overlay the device can never open, and a geometry line that reads
 * "Not set" for a widget that is in fact placed sends the user to reconfigure
 * something that was already fine.
 */
import { describe, expect, it } from 'vitest';
import { canStartConfigure, overlayGroups, widgetGeometryText } from './overlay-groups';
import { anchoredWidgetsFor, placeableWidgetsFor } from './widget-registry';
import { TEMPLE_WINDOW_LABEL } from '../manager';

const ALL_GRANTS = { merc: true, temple: true };
const NO_GRANTS = { merc: false, temple: false };

describe('the Overlay Positions groups', () => {
	it('lists Lab, Merc and Temple in that order for a fully granted device', () => {
		expect(overlayGroups(ALL_GRANTS).map((group) => group.heading)).toEqual([
			'Lab',
			'Merc',
			'Temple'
		]);
	});

	it('keeps the four lab window rows under Lab, in their existing order', () => {
		const lab = overlayGroups(ALL_GRANTS).find((group) => group.heading === 'Lab');
		expect(lab?.windows).toEqual([
			{ name: 'comparator', label: 'Gems Compare' },
			{ name: 'compass', label: 'Lab Compass' },
			{ name: 'pathstrip', label: 'Lab Map' },
			{ name: 'timer', label: 'Lab Timer' }
		]);
	});

	it('drops the whole Merc group, verdict row included, without the merc feature', () => {
		const groups = overlayGroups({ merc: false, temple: true });
		expect(groups.map((group) => group.heading)).toEqual(['Lab', 'Temple']);
		// The row itself, not just the heading: the flat list this replaced left
		// the row out entirely, and a heading-only gate would put it back under Lab.
		expect(groups.flatMap((group) => group.windows.map((row) => row.name))).not.toContain(
			'mercenary'
		);
	});

	it('drops the whole Temple group without the temple feature', () => {
		expect(overlayGroups({ merc: true, temple: false }).map((group) => group.heading)).toEqual([
			'Lab',
			'Merc'
		]);
	});

	it('still lists Lab for a device with no features at all', () => {
		// The lab overlays are ungated; a device that lost every grant must not
		// lose the section it always had.
		expect(overlayGroups(NO_GRANTS).map((group) => group.heading)).toEqual(['Lab']);
	});

	it('takes the Temple rows from the widget registry rather than a second list', () => {
		const temple = overlayGroups(ALL_GRANTS).find((group) => group.heading === 'Temple');
		expect(temple?.widgets.map((row) => row.spec.id)).toEqual([
			...placeableWidgetsFor(TEMPLE_WINDOW_LABEL).map((widget) => widget.id),
			...anchoredWidgetsFor(TEMPLE_WINDOW_LABEL).map((widget) => widget.id)
		]);
	});

	it('lists an anchored widget too, so it keeps a Show switch', () => {
		// POE-244 review: the panel-side advice widget is placed by the module,
		// so it has no stored rectangle — but dropping its row took away the
		// only control the user has for that surface, and it became the one
		// overlay thing with no way to switch it off. The row is here; the FLAG
		// is what tells the page not to print a placement it does not have.
		// (POE-249 replaced the kill callout with `temple.offers`; the row's
		// shape is unchanged, and the id is what moved.)
		const temple = overlayGroups(ALL_GRANTS).find((group) => group.heading === 'Temple');
		expect(temple?.widgets.map((row) => [row.spec.id, row.placeable])).toEqual([
			['temple.door', true],
			['temple.waiting', true],
			['temple.offers', false]
		]);
	});

	it('offers Configure only while there is something draggable to arrange', () => {
		// An anchored row must not enable the button on its own: a config session
		// over zero frames is an interactive monitor-sized rectangle with a
		// Save/Cancel bar for nothing.
		const temple = overlayGroups(ALL_GRANTS).find((group) => group.heading === 'Temple');
		expect(temple?.configureModule).toBe(TEMPLE_WINDOW_LABEL);
		expect(temple?.widgets.filter((row) => row.placeable).length).toBeGreaterThan(0);
	});

	it('gives the Temple group no window row of its own', () => {
		// Its overlay IS the monitor and has no persisted rect (POE-225 D8), so a
		// window row there would open a config copy of a fullscreen window.
		expect(overlayGroups(ALL_GRANTS).find((group) => group.heading === 'Temple')?.windows).toEqual(
			[]
		);
	});

	it('gives every group it returns something to draw', () => {
		// There is no empty-group filter, so this is the table's own invariant: a
		// heading with no rows under it reads as a feature that failed to load.
		for (const group of overlayGroups(ALL_GRANTS)) {
			expect(group.windows.length + group.widgets.length).toBeGreaterThan(0);
		}
	});

	it('offers Configure widgets for the temple module only', () => {
		expect(
			overlayGroups(ALL_GRANTS).map((group) => [group.heading, group.configureModule])
		).toEqual([
			['Lab', null],
			['Merc', null],
			['Temple', TEMPLE_WINDOW_LABEL]
		]);
	});
});

describe('a widget row geometry line', () => {
	it('says the placement is unset when Rust holds no row for the widget', () => {
		expect(widgetGeometryText(undefined)).toBe('Not set');
	});

	it('prints the stored physical rectangle for a widget the user resized', () => {
		expect(
			widgetGeometryText({ x: 120, y: 64, width: 300, height: 220, visible: true })
		).toBe('(120, 64) 300×220');
	});

	it('calls a zero-size row content-sized rather than printing 0×0', () => {
		// This is what Save writes for a widget that was moved but never resized
		// (`sizeToPersist`), and `placementFor` reads it back as "let the content
		// decide" — so `0×0` would describe a widget as collapsed to nothing.
		expect(widgetGeometryText({ x: 40, y: 40, width: 0, height: 0, visible: true })).toBe(
			'(40, 40) content-sized'
		);
	});

	it('calls a row with only a height content-sized, matching what the host applies', () => {
		// `placementFor` applies a stored size only when BOTH dimensions are
		// positive. A line that called this sized would name a width the widget
		// is not drawn at.
		expect(widgetGeometryText({ x: 40, y: 40, width: 0, height: 220, visible: true })).toBe(
			'(40, 40) content-sized'
		);
	});

	it('prints the placement of a hidden widget rather than hiding the line too', () => {
		// The Show checkbox on the same row already says it is hidden; blanking
		// the geometry as well would lose where it comes back to.
		expect(widgetGeometryText({ x: 12, y: 8, width: 100, height: 50, visible: false })).toBe(
			'(12, 8) 100×50'
		);
	});
});

describe('whether another Configure flow may be started', () => {
	const IDLE = { region: false, position: false, widgets: false };

	it('allows one when no configuration window is up', () => {
		expect(canStartConfigure(IDLE)).toBe(true);
	});

	// Each of the three below is a window that is interactive over the game and
	// ends only through its OWN Save/Cancel. Starting a second flow leaves the
	// first one click-eating behind it, and the page's overlay-save handler
	// dispatches to whichever it finds first — so the second bar the user
	// reaches stands down the wrong window.
	it('refuses one while an OCR region window is on screen', () => {
		expect(canStartConfigure({ ...IDLE, region: true })).toBe(false);
	});

	it('refuses one while a per-window position copy is on screen', () => {
		expect(canStartConfigure({ ...IDLE, position: true })).toBe(false);
	});

	it('refuses one while a widget config session is running', () => {
		expect(canStartConfigure({ ...IDLE, widgets: true })).toBe(false);
	});
});
