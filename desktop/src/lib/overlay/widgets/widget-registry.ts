/**
 * The ONE place a module declares the widgets its overlay draws (POE-225).
 *
 * A module's overlay is a single fullscreen, click-through window over the
 * game monitor (POE-237); what the player sees is a handful of small panels placed
 * inside it. This file is the list of those panels. Everything else about a
 * widget is derived from an entry here: `WidgetHost.svelte` renders one
 * absolutely-positioned container per entry, Rust persists its rectangle under
 * the entry's `id`, and Settings (POE-226) draws one row per entry.
 *
 * # Why the shipped numbers are CSS pixels
 *
 * Persisted geometry is PHYSICAL, window-relative pixels — the unit Rust's
 * `WidgetGeometry` stores and the unit a game-anchored widget would have to be
 * placed in, since the window is the GAME monitor and every capture is that
 * same monitor (POE-237). But a shipped default is reasoned the way the panel is
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
	 * Placed against the GAME rather than by the user, and therefore by the
	 * module's own geometry: no Settings row, no persisted rectangle, and no
	 * config-mode frame.
	 *
	 * `temple.advice` is the first (POE-244). The host does not place it —
	 * nothing generic can, because where it goes is a function of where the game
	 * drew the thing it points at — so the host renders anchored widgets through
	 * a SECOND snippet, into a layer the size of the whole window, and the
	 * module positions its own content inside that. What the host still supplies
	 * is everything the two kinds share: the same window, the same `data-hot`
	 * declaration, and the same exclusion from Settings and from config mode.
	 *
	 * `defaults` is not a placement for one of these. Its width is still read —
	 * as the content's wrap ceiling, the same job it does for an unconfigured
	 * placeable widget — and its position is not.
	 */
	anchored?: boolean;
}

/**
 * Every widget every module declares.
 *
 * The temple's two are POE-244's rebuild. `temple.board` — the lattice diagram
 * this file shipped in POE-225 — is GONE from the overlay: the board is already
 * on screen behind it, and redrawing it there cost space that had to be kept
 * clear of the module's own OCR crops. `TempleLattice.svelte` survives on the
 * Temple page, which is the surface for reading.
 *
 * What is left is one of each kind. `temple.advice` is the kill callout, placed
 * against the game (`anchored`), because a box that points at an architect
 * block has to be wherever that block is. `temple.door` is the room widget,
 * placed by the USER, because it is the surface that stays up after the panel is
 * gone — and past the capture standing down (POE-248) — and only the player
 * knows what their screen is free of at that point.
 */
export const WIDGETS: readonly WidgetSpec[] = [
	{
		id: 'temple.advice',
		module: 'temple',
		label: 'Temple kill callout',
		// ANCHORED: only `w` is read, as the wrap ceiling for the callout's own
		// text. A callout wider than this reads as a paragraph over the game,
		// which is the thing POE-244 replaced. The position is never used —
		// `overlay-geometry.ts`'s `calloutPlacement` decides it per read — but
		// the registry's own invariant is that every default is a real
		// rectangle, so the numbers are the ones the box would occupy.
		defaults: { x: 250, y: 40, w: 320, h: 90 },
		resizable: false,
		anchored: true
	},
	{
		id: 'temple.door',
		module: 'temple',
		label: 'Temple door diamond',
		// Small on purpose: it is a shape to glance at, not a panel to read.
		// The shipped POSITION is the last resort — the module offers the host a
		// game-anchored default that clears every read region, and this applies
		// only when there is no board to anchor to (see `WidgetHost`'s
		// `defaultsFor`). The width is also the wrap ceiling for the two text
		// lines the widget still has: the room's name, and `doorWarning`'s one
		// line. The kill is a GLYPH inside the shape since POE-248, not a line
		// under it, so the box is fuller than these numbers were sized for and
		// the height is now slack rather than a fit.
		defaults: { x: 40, y: 300, w: 190, h: 215 },
		resizable: true
	}
];

/** Every widget one module declares, in registry order. */
export function widgetsFor(module: string): WidgetSpec[] {
	return WIDGETS.filter((widget) => widget.module === module);
}

/** The widgets a module PLACES — the anchored ones are the module's own
 *  geometry and are neither persisted nor listed in Settings. */
export function placeableWidgetsFor(module: string): WidgetSpec[] {
	return widgetsFor(module).filter((widget) => !widget.anchored);
}

/** The widgets a module ANCHORS to the game — the complement of
 *  [`placeableWidgetsFor`], so every declared widget is drawn by exactly one of
 *  the host's two paths. */
export function anchoredWidgetsFor(module: string): WidgetSpec[] {
	return widgetsFor(module).filter((widget) => widget.anchored === true);
}
