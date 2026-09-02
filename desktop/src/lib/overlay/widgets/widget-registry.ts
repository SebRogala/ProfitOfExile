/**
 * The ONE place a module declares the widgets its overlay draws (POE-225).
 *
 * A module's overlay is a single fullscreen, click-through window over the
 * primary monitor; what the player sees is a handful of small panels placed
 * inside it. This file is the list of those panels. Everything else about a
 * widget is derived from an entry here: `WidgetHost.svelte` renders one
 * absolutely-positioned container per entry, Rust persists its rectangle under
 * the entry's `id`, and Settings (POE-226) draws one row per entry.
 *
 * # Why the shipped numbers are CSS pixels
 *
 * Persisted geometry is PHYSICAL, window-relative pixels — the unit Rust's
 * `WidgetGeometry` stores and the unit a game-anchored widget would have to be
 * placed in, since the window is the primary monitor and every capture is the
 * primary monitor too. But a shipped default is reasoned the way the panel is
 * built: font sizes and padding, in CSS. On a 150 %-scaled Windows display
 * those are not the same number, and shipping the CSS figure as a physical one
 * put the merc strip a third short of its own budget (see
 * `../overlay-defaults.ts`). So the defaults here are CSS, typed as that
 * module's `OverlayDefaultGeometry`, and `physicalGeometry` is the one
 * conversion — used when a placement is SAVED, never when it is read back.
 *
 * # What is not here
 *
 * The widget's content. The host renders a snippet per id supplied by the
 * module's overlay route, so adding a widget is an entry here plus a branch
 * there — no new window, no new Rust field, no new command.
 */
import type { OverlayDefaultGeometry } from '../overlay-defaults';

/** One widget a module's overlay draws. */
export interface WidgetSpec {
	/**
	 * `"<module>.<widget>"` — the key Rust persists the placement under.
	 *
	 * The module half must equal `module` below, because `get_widget_geometries`
	 * finds a module's rows by that prefix. `widget-registry.test.ts` pins it.
	 */
	id: `${string}.${string}`;
	/**
	 * The module this widget belongs to — also its overlay WINDOW LABEL and the
	 * `/overlay/<module>` route segment, because one module has one window.
	 */
	module: string;
	/** The row text in Settings → Overlay Positions. */
	label: string;
	/** Shipped placement, in CSS pixels. Applies until the user has configured
	 *  this widget; a configured widget has a persisted physical rectangle and
	 *  this is never consulted again. */
	defaults: OverlayDefaultGeometry;
	/**
	 * Whether config mode offers resize handles.
	 *
	 * A widget that is not resizable is always sized to its own content; a
	 * resizable one is sized to content until the user has dragged an edge, and
	 * to the persisted size afterwards.
	 */
	resizable: boolean;
	/**
	 * Placed by the geometry engine rather than by the user — a door arrow over
	 * a room, say. No Settings row, no persisted rectangle, and the host skips
	 * it. Nothing ships as anchored yet; the flag is here so the host's
	 * filtering is written once rather than retrofitted around the first one.
	 */
	anchored?: boolean;
}

/**
 * Every widget every module declares.
 *
 * The temple's two are the existing overlay panel split in half (POE-225 D10):
 * the lattice on the left, the advice text on the right. No new content — the
 * point of this batch is that the two halves become independently placeable.
 *
 * The numbers below are CSS px chosen so that at scale factor 1 the pair
 * occupies what the old single 620×260 PHYSICAL panel did. They are not that
 * panel's numbers converted: on a scaled display the CSS figure and the
 * physical one differ, and the CSS one is what a shipped default has to be
 * (see the unit note above).
 */
export const WIDGETS: readonly WidgetSpec[] = [
	{
		id: 'temple.board',
		module: 'temple',
		label: 'Temple board',
		// The `.board` column was `flex: 0 0 200px` inside the old panel, and the
		// lattice is square. The width also serves as the widget's wrap ceiling
		// until the user resizes it (`placementFor`'s `maxWidth`).
		defaults: { x: 40, y: 40, w: 200, h: 200 },
		resizable: true
	},
	{
		id: 'temple.advice',
		module: 'temple',
		label: 'Temple advice',
		// To the right of the board at its default placement, with the width the
		// old panel left for the advice column — which is also where its text
		// wraps until the user resizes it (`placementFor`'s `maxWidth`).
		defaults: { x: 250, y: 40, w: 400, h: 200 },
		resizable: true
	}
];

/** Every widget one module declares, in registry order. */
export function widgetsFor(module: string): WidgetSpec[] {
	return WIDGETS.filter((widget) => widget.module === module);
}

/** The widgets a module PLACES — the anchored ones belong to the geometry
 *  engine and are neither persisted nor listed in Settings. */
export function placeableWidgetsFor(module: string): WidgetSpec[] {
	return widgetsFor(module).filter((widget) => !widget.anchored);
}
