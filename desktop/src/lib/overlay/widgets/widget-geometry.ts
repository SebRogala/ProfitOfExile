/**
 * The arithmetic behind placing, dragging and resizing an overlay widget
 * (POE-225) — every part of `WidgetHost.svelte` that can be wrong quietly.
 *
 * The host itself is DOM glue: pointer listeners, absolute positioning, an
 * invoke on Save. Everything that decides a NUMBER lives here, because an
 * overlay window has no test harness in this app and a widget placed a hundred
 * pixels off, or one that comes back at the wrong size on a scaled display,
 * looks exactly like a widget that was never configured. Same split as
 * `../content-height.ts` and `../hot-rects.ts`.
 *
 * # Two units, and which side of the boundary each one is on
 *
 * - **Physical, window-relative pixels** — what Rust persists (`WidgetGeometry`)
 *   and what a game-anchored widget will be placed in, since the window is the
 *   primary monitor and so is every capture.
 * - **CSS pixels** — what the page actually lays out in, and what the shipped
 *   defaults in `widget-registry.ts` are reasoned in.
 *
 * Physical → CSS is [`cssRect`]; CSS → physical is `physicalGeometry` from
 * `../overlay-defaults.ts`, reused rather than re-spelled so the rounding rule
 * is the same one the merc strip's constructor seed goes through.
 *
 * CSS values are deliberately NOT rounded on the way in. A widget stored at 61
 * physical px on a 150 % display is 40.667 CSS px; rounding that to 41 and
 * multiplying back gives 62, so every open-and-save of config mode would walk
 * the widget one pixel to the right.
 */
import { physicalGeometry, type OverlayDefaultGeometry } from '../overlay-defaults';
import type { WidgetSpec } from './widget-registry';

/** Rust's `settings::WidgetGeometry` — physical px, window-relative. */
export interface WidgetGeometry {
	x: number;
	y: number;
	width: number;
	height: number;
	visible: boolean;
}

/** A widget's live rectangle inside the host, in CSS px. Same shape as a
 *  shipped default, because a shipped default IS one. */
export type WidgetRect = OverlayDefaultGeometry;

/** The host's own box, in CSS px — `window.innerWidth`/`innerHeight`. */
export interface HostSize {
	width: number;
	height: number;
}

/** The eight directions a resize can go, spelled as Tauri's `startResizeDragging`
 *  spells them so the config window and the widget frame stay one vocabulary. */
export type ResizeEdge =
	| 'North'
	| 'NorthEast'
	| 'East'
	| 'SouthEast'
	| 'South'
	| 'SouthWest'
	| 'West'
	| 'NorthWest';

/**
 * How wide the grab zone along a widget's border is, in CSS px.
 *
 * The same 10 the position-config window uses (`routes/overlay/+page.svelte`),
 * and shrunk the same way for a small widget: a zone wider than an eighth of
 * the box leaves no interior to drag from, so a small widget could only ever be
 * resized.
 */
export const BASE_EDGE = 10;

/**
 * The smallest a widget may be resized to, in CSS px.
 *
 * One line of text plus its padding — `MIN_OVERLAY_HEIGHT_CSS` in Rust, for the
 * same reason: a box dragged to zero cannot be grabbed again, and config mode
 * has no other way back.
 */
export const MIN_WIDGET_SIDE_CSS = 24;

/** A number pinned into `[lo, hi]`, with `hi` never below `lo`. */
function clamp(value: number, lo: number, hi: number): number {
	return Math.min(Math.max(value, lo), Math.max(lo, hi));
}

/**
 * The persisted rectangle in CSS px, or `null` when the scale factor cannot
 * support the conversion.
 *
 * FAILS CLOSED, the way `../hot-rects.ts`'s `physicalHotRect` does, and for the
 * same reason. `scaleFactor()` is 0 until it answers and stays 0 if it fails,
 * and substituting 1 for it does not produce "no answer" — it produces a
 * CONFIDENT WRONG one: on a 150 % display every stored widget would be drawn a
 * third too far out and a third too large, which looks exactly like a placement
 * the user got wrong rather than like a conversion that never happened. The
 * callers already have a not-drawn state ([`placementFor`] returns `null` for a
 * hidden widget, and the host filters those out) and Save already refuses at
 * scale 0, so declining costs the frames before the factor resolves and nothing
 * else.
 */
export function cssRect(geometry: WidgetGeometry, scaleFactor: number): WidgetRect | null {
	if (!(scaleFactor > 0) || !Number.isFinite(scaleFactor)) return null;
	return {
		x: geometry.x / scaleFactor,
		y: geometry.y / scaleFactor,
		w: geometry.width / scaleFactor,
		h: geometry.height / scaleFactor
	};
}

/**
 * The live rectangle as Rust wants it — physical, rounded, with the Show flag.
 *
 * Width and height are floored at zero: `PhysicalSize` is unsigned, and a
 * negative one from a rectangle the caller measured badly would be a panic on
 * the Rust side rather than a misplaced widget.
 */
export function widgetGeometry(
	rect: WidgetRect,
	scaleFactor: number,
	visible: boolean
): WidgetGeometry {
	const physical = physicalGeometry(rect, scaleFactor);
	return {
		x: physical.x,
		y: physical.y,
		width: Math.max(0, physical.w),
		height: Math.max(0, physical.h),
		visible
	};
}

/**
 * The rectangle moved back inside the host, keeping its size.
 *
 * Size is kept rather than shrunk: a widget wider than the window pins to the
 * left edge and hangs off the right, which is visible and recoverable, whereas
 * silently narrowing it would look like the content had been cut.
 */
export function clampToHost(rect: WidgetRect, host: HostSize): WidgetRect {
	return {
		x: clamp(rect.x, 0, host.width - rect.w),
		y: clamp(rect.y, 0, host.height - rect.h),
		w: rect.w,
		h: rect.h
	};
}

/**
 * Which edge a pointer at `(offsetX, offsetY)` — CSS px from the widget's
 * top-left — is grabbing, or `null` for the interior (a move).
 *
 * Corners win over sides, so the diagonal handles exist at all.
 */
export function edgeAt(rect: WidgetRect, offsetX: number, offsetY: number): ResizeEdge | null {
	const zone = Math.min(BASE_EDGE, Math.floor(rect.h / 8), Math.floor(rect.w / 8));
	if (zone <= 0) return null;
	const top = offsetY < zone;
	const bottom = offsetY > rect.h - zone;
	const left = offsetX < zone;
	const right = offsetX > rect.w - zone;

	if (top && left) return 'NorthWest';
	if (top && right) return 'NorthEast';
	if (bottom && left) return 'SouthWest';
	if (bottom && right) return 'SouthEast';
	if (top) return 'North';
	if (bottom) return 'South';
	if (left) return 'West';
	if (right) return 'East';
	return null;
}

/**
 * Which edge the pointer is grabbing, with the spec's own answer applied first.
 *
 * A widget that is not resizable has no edges at all — it is always sized to
 * its content, so a handle that appeared to change its size would either do
 * nothing or write a size `placementFor` then declines to apply. The host asks
 * this rather than [`edgeAt`] so the rule lives with the other geometry rules
 * instead of in a pointer handler.
 */
export function edgeFor(
	spec: WidgetSpec,
	rect: WidgetRect,
	offsetX: number,
	offsetY: number
): ResizeEdge | null {
	if (!spec.resizable) return null;
	return edgeAt(rect, offsetX, offsetY);
}

/** The CSS cursor for each edge; the interior is a move. */
export const EDGE_CURSORS: Record<ResizeEdge, string> = {
	North: 'ns-resize',
	South: 'ns-resize',
	East: 'ew-resize',
	West: 'ew-resize',
	NorthEast: 'nesw-resize',
	SouthWest: 'nesw-resize',
	NorthWest: 'nwse-resize',
	SouthEast: 'nwse-resize'
};

/**
 * The rectangle config mode opens a widget at.
 *
 * The POSITION is the persisted one whenever there is a persisted row, and the
 * shipped default only when there is not. Seeding a widget that is not on
 * screen — hidden by its Show checkbox, or gated off by its module's own
 * content rule — from the defaults instead is how a Save that touched nothing
 * would move every such widget back to where it shipped.
 *
 * The SIZE is the persisted one only when the widget actually has one; a
 * content-sized widget stores `0 × 0` ([`sizeToPersist`]), and a frame drawn at
 * zero cannot be grabbed at all. Its fallbacks are, in order, what the widget
 * currently measures on screen and the shipped default.
 *
 * A DEGENERATE measurement counts as no measurement. `measured` is absent when
 * the widget is not rendered at all, but a widget that IS rendered and drawing
 * nothing — the temple's, outside a temple — measures `0 × 0`, and taking that
 * as a real size would open config mode on a frame with no interior to drag
 * and no edge to pull, which is the same dead end an unrecoverable widget is.
 *
 * `null` when there IS a stored placement and [`cssRect`] cannot convert it —
 * the host draws no frame for that widget rather than one at a made-up scale,
 * and Save refuses at an unresolved scale factor anyway. A widget with nothing
 * stored is seeded from what it measures or from the registry, neither of which
 * is a conversion, so it is unaffected.
 */
export function seedRect(
	spec: WidgetSpec,
	geometry: WidgetGeometry | undefined,
	measured: WidgetRect | null,
	scaleFactor: number,
	host: HostSize
): WidgetRect | null {
	// `??` would accept a zero here, because zero is not nullish.
	const box = measured && measured.w > 0 && measured.h > 0 ? measured : null;
	if (!geometry) {
		return clampToHost(box ?? { ...spec.defaults }, host);
	}
	const rect = cssRect(geometry, scaleFactor);
	if (!rect) return null;
	return clampToHost(
		{
			x: rect.x,
			y: rect.y,
			w: rect.w > 0 ? rect.w : (box?.w ?? spec.defaults.w),
			h: rect.h > 0 ? rect.h : (box?.h ?? spec.defaults.h)
		},
		host
	);
}

/**
 * Whether a pointer move counts as the user RESIZING the widget.
 *
 * An edge is not enough. A press on the border followed by a release without
 * movement still delivers a move event with a zero delta, and counting it would
 * pin the widget's size ([`sizeToPersist`]) to whatever its content measured
 * that day — the content-sizing contract ended by a click the user would not
 * describe as a resize at all.
 */
export function gestureResized(edge: ResizeEdge | null, dx: number, dy: number): boolean {
	return edge !== null && (dx !== 0 || dy !== 0);
}

/**
 * The rectangle Save persists, with the size dropped unless it is the user's.
 *
 * A widget is CONTENT-SIZED by contract (`widget-registry.ts`) until the user
 * drags an edge. Writing the measured width and height on every Save would end
 * that silently for every widget in the module the first time any one of them
 * was moved: the box would be pinned to whatever the content happened to
 * measure that day and would clip the moment it grew a line. So a zero size is
 * persisted — which `placementFor` reads back as "let the content decide" —
 * unless this config session actually resized the widget, or a real size was
 * already stored from one that did.
 *
 * A non-resizable widget never keeps a size, whatever is stored: its handles do
 * not exist ([`edgeFor`]) and `placementFor` would ignore the number anyway.
 */
export function sizeToPersist(
	spec: WidgetSpec,
	rect: WidgetRect,
	resizedInSession: boolean,
	stored: WidgetGeometry | undefined
): WidgetRect {
	const hadSize = !!stored && stored.width > 0 && stored.height > 0;
	const keep = spec.resizable && (resizedInSession || hadSize);
	return keep ? rect : { x: rect.x, y: rect.y, w: 0, h: 0 };
}

/**
 * A drag reducer: where the widget sits after the pointer has moved
 * `(dx, dy)` CSS px from where the drag started.
 *
 * Written against the rectangle the drag STARTED from rather than the previous
 * frame's, so a drag that runs into the edge and comes back returns to where
 * the pointer says it should be instead of accumulating the clamped-away
 * movement.
 */
export function dragged(start: WidgetRect, dx: number, dy: number, host: HostSize): WidgetRect {
	return clampToHost({ x: start.x + dx, y: start.y + dy, w: start.w, h: start.h }, host);
}

/**
 * A resize reducer, same start-relative contract as [`dragged`].
 *
 * The two axes are independent, so a corner is just both of them. Dragging a
 * North or West edge moves the origin AND changes the size, and the origin is
 * what gets clamped — against 0 on the near side and against the minimum size
 * on the far side, so pushing an edge past its opposite one stops at the
 * minimum instead of inverting the rectangle.
 */
export function resized(
	start: WidgetRect,
	edge: ResizeEdge,
	dx: number,
	dy: number,
	host: HostSize
): WidgetRect {
	let { x, y, w, h } = start;

	if (edge.includes('West')) {
		x = clamp(start.x + dx, 0, start.x + start.w - MIN_WIDGET_SIDE_CSS);
		w = start.x + start.w - x;
	} else if (edge.includes('East')) {
		w = clamp(start.w + dx, MIN_WIDGET_SIDE_CSS, host.width - start.x);
	}

	if (edge.includes('North')) {
		y = clamp(start.y + dy, 0, start.y + start.h - MIN_WIDGET_SIDE_CSS);
		h = start.y + start.h - y;
	} else if (edge.includes('South')) {
		h = clamp(start.h + dy, MIN_WIDGET_SIDE_CSS, host.height - start.y);
	}

	return { x, y, w, h };
}

/** Where the host puts one widget. `width`/`height` of `null` mean "let the
 *  content decide", which is what an unconfigured widget always does. */
export interface WidgetPlacement {
	x: number;
	y: number;
	width: number | null;
	height: number | null;
	/**
	 * A ceiling for a content-sized widget, in CSS px, or `null` for none.
	 *
	 * Content sizing with no ceiling is `max-content`: a one-line headline would
	 * stretch across the whole monitor, because the host element IS the whole
	 * monitor and there is nothing else to wrap against. The shipped default
	 * width is that ceiling, so a widget nobody has resized still wraps where the
	 * registry says it should while remaining free to be shorter.
	 */
	maxWidth: number | null;
}

/**
 * Where one widget goes, or `null` when it must not be rendered at all.
 *
 * The three cases, in the order they are decided:
 *
 * 1. A stored placement with `visible: false` — the user's Show checkbox is
 *    off. Nothing is rendered, so nothing can claim a click either.
 * 2. No stored placement — the shipped CSS default, sized to content. A widget
 *    nobody has configured must render at whatever the registry ships TODAY,
 *    which is why nothing seeds the stored map with the defaults.
 * 3. A stored placement — the persisted physical rectangle in CSS px. The SIZE
 *    is only applied for a resizable widget that actually has one: a
 *    non-resizable widget's stored width and height are whatever its content
 *    measured when the user last saved, and pinning the box to that would clip
 *    it the moment the content grew a line.
 *
 * A fourth `null`, before case 3 can be decided: the window's scale factor has
 * not resolved, so the stored physical rectangle cannot be converted
 * ([`cssRect`]). Nothing is drawn for those frames rather than a widget placed
 * at an assumed 1×, which on a scaled display is a widget the user is looking
 * at in the wrong place. An UNSTORED widget (case 2) needs no conversion and
 * still renders.
 */
export function placementFor(
	spec: WidgetSpec,
	geometry: WidgetGeometry | undefined,
	scaleFactor: number,
	host: HostSize
): WidgetPlacement | null {
	if (geometry && !geometry.visible) return null;
	// A resizable widget the user has never resized still wraps at the width the
	// registry ships, as a CEILING rather than a size (see `maxWidth`).
	const ceiling = spec.resizable ? spec.defaults.w : null;
	if (!geometry) {
		return {
			x: spec.defaults.x,
			y: spec.defaults.y,
			width: null,
			height: null,
			maxWidth: ceiling
		};
	}
	const rect = cssRect(geometry, scaleFactor);
	if (!rect) return null;
	const sized = spec.resizable && geometry.width > 0 && geometry.height > 0;
	// Clamped against the window it is about to be drawn in, not against the one
	// it was saved on: a stored placement outlives the monitor it was made on, and
	// a widget whose origin is past the new bottom-right renders entirely
	// off-screen with no way back except a Settings row the user cannot see the
	// effect of. The extent used is the size that will actually be applied, or the
	// shipped one for a content-sized widget, whose real height is not known yet.
	// An unmeasured host (both zero, the frame before the first `resize`) clamps
	// nothing — pinning every widget to the origin would be the worse answer.
	const extent: WidgetRect = {
		x: rect.x,
		y: rect.y,
		w: sized ? rect.w : spec.defaults.w,
		h: sized ? rect.h : spec.defaults.h
	};
	const placed = host.width > 0 && host.height > 0 ? clampToHost(extent, host) : extent;
	return {
		x: placed.x,
		y: placed.y,
		width: sized ? rect.w : null,
		height: sized ? rect.h : null,
		maxWidth: sized ? null : ceiling
	};
}
