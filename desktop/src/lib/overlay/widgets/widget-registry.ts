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
	 * module's own geometry: no Configure placement, no persisted rectangle, and
	 * no config-mode frame. It keeps a Show row in Settings (`overlay-groups.ts`
	 * lists it with `placeable: false`) — an anchored widget the user cannot
	 * switch off would be the one overlay surface with no control at all.
	 *
	 * `temple.offers` is the only one — POE-244's `temple.advice` under a new
	 * id since POE-249. The host does not place it — nothing generic can,
	 * because where it goes is a function of where the game drew the thing it
	 * points at — so the host renders anchored widgets through
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
 * The temple's three are POE-244's rebuild plus POE-249's notice. Each answers
 * a different question the player has at a different moment of one incursion
 * cycle, which is why they are three widgets and not three lines in one box:
 *
 * - `temple.waiting` (POE-249) says the module HEARD Alva and is waiting for
 *   the layout panel. It is the only surface that exists before there is
 *   anything read — the answer to "is this thing on?" while the sheet is still
 *   shut — and it is gone the moment there is a board to look at
 *   (`overlayShowsWaiting`). Placed by the USER.
 * - `temple.offers` is the pair of offer boxes (POE-249): both architect
 *   blocks, stacked in the sheet's left margin, with the advisor's pick
 *   framed in cyan — which architect to click and what taking either one
 *   builds, while the sheet is open. Placed against the GAME (`anchored`),
 *   because a column that mirrors the panel's own block order has to be
 *   wherever the game drew those blocks. It REPLACES `temple.advice`, the
 *   single kill callout, which named one block and left the other — half the
 *   decision — off the overlay.
 * - `temple.door` is the room widget: which door to open, drawn on the room's
 *   own shape. Placed by the USER, because it is the surface that stays up
 *   after the panel is gone — and past the capture standing down (POE-248) —
 *   and only the player knows what their screen is free of at that point.
 *
 * `temple.board` — the lattice diagram this file shipped in POE-225 — is GONE
 * from the overlay: the board is already on screen behind it, and redrawing it
 * there cost space that had to be kept clear of the module's own OCR crops.
 * `TempleLattice.svelte` survives on the Temple page, which is the surface for
 * reading.
 *
 * `temple.advice` — the kill callout POE-249 replaced — is gone the same way,
 * and its persisted ROW goes INERT rather than being migrated out: Rust does
 * not validate widget ids against this registry (`set_widget_geometry` says
 * why) and the host looks placements up BY SPEC, so a row nothing declares is
 * never read. Nothing of it survives, including the one part of a stored
 * anchored row that WAS still live — its `visible` flag. The Show checkbox now
 * writes `temple.offers`'s own row, and a machine that had switched the callout
 * off sees the offer boxes on, which is the honest answer for a surface that
 * did not exist when that switch was flipped.
 *
 * **The order of this list is the order Settings draws the rows in**, within
 * each kind: `widgetsFor` preserves it, and `overlayGroups` lists the placeable
 * rows first and the anchored ones after. So moving an entry here moves a row
 * in Settings → Overlay Positions and nothing else.
 */
export const WIDGETS: readonly WidgetSpec[] = [
	{
		id: 'temple.offers',
		module: 'temple',
		label: 'Temple offer boxes',
		// ANCHORED: only `w` is read, as the wrap ceiling for each box's own
		// text. A box wider than this reads as a paragraph over the game, which
		// is the thing POE-244 replaced and POE-249 kept. The position is never
		// used — `overlay-geometry.ts`'s `offerStackPlacement` decides it per
		// read, in the sheet's left margin — but the registry's own invariant is
		// that every default is a real rectangle, so the numbers are the ones
		// ONE box would occupy: 260 wide, and tall enough for the four lines a
		// box carries (headline, what it builds, the rating, one reason).
		defaults: { x: 40, y: 115, w: 260, h: 200 },
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
	},
	{
		id: 'temple.waiting',
		module: 'temple',
		label: 'Temple waiting notice',
		// TOP-CENTRE on a 1920-wide host (830 = 960 − 260/2), and deliberately
		// not screen-centre, which the owner asked for: at 1920x1080 the middle
		// of the screen is on plates C1/D1/D2, and a box there is OCR input the
		// app wrote itself (ADR-019). Top-centre is measured clear instead.
		//
		// MEASURED, on the ONE full-screen capture this repository holds
		// (`screen-live-1920x1080.png`, Entrance centre (960, 713) at scale
		// 1.0): the crop this has to miss is `panel_rect` — `origin.x +
		// 171·scale`, not the panel's border box — which starts at x 1131, and
		// a 260 px box centred on a 1920-wide host ends at 1090. That 41 px is
		// the clearance any decision to widen this box is budgeted against.
		//
		// NOT measured, and this comment used to claim otherwise: the 27 px and
		// 36 px clearances it quoted for "the 1374 and 1539 captures" were
		// DERIVED at reference scale from a synthetic origin, not read off a
		// monitor. `board-ref-1374.png` (1374×542) and `board-live-1539.png`
		// (1539×613) are BOARD CROPS rather than screens — there is no second
		// screen width in this repository — and the 844 those figures started
		// from is `673 + 171`, the unit tests' own origin. A second row here
		// needs a second real capture.
		//
		// The DRAWN box is SMALLER than this rectangle, deliberately. The widget
		// is not resizable, so `placementFor` returns no width and the component
		// sizes itself to its one line — on the order of 212×31 CSS px at 14 px
		// type, drawing 830…1042 and centred nearer 936 than 960. Oversizing is
		// the SAFE direction: these numbers are what `waitingDefaultPlacement`
		// hands `avoidRects`, so a box declared wider than the ink keeps MORE
		// margin from the crop than the notice needs. Do not "fix" the 260 down
		// to the measured text width; that spends the margin.
		//
		// It is one drag from the centre if the player wants it there, and that
		// drag is then their own placement.
		//
		// The shipped position is also the LAST RESORT here, the same as the
		// door's: `defaultsFor` in the temple route offers `waitingDefaultPlacement`
		// against the last board's read regions whenever there is one, and this
		// fixed rectangle applies when there is not — which is the cold start
		// this widget exists for.
		defaults: { x: 830, y: 16, w: 260, h: 40 },
		resizable: false
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
