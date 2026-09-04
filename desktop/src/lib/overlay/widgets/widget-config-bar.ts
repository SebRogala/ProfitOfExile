/**
 * Where the widget-config Save/Cancel bar goes, in CSS px (POE-245).
 *
 * The bar shipped pinned 24 px above the bottom edge of the host, centred. The
 * host is a WHOLE MONITOR, so on a 1080p screen that put an 11 px-tall strip of
 * 11 px text a thousand pixels below the widgets the user was dragging, over
 * whatever the game happened to be drawing there. The owner's report (2026-09-03)
 * was "so small that not visible" — not that it was in the wrong place, that it
 * could not be FOUND.
 *
 * Two halves fix that and only one of them is arithmetic. The bar being legible
 * (its type sizes, its hit targets, the tint that says config mode is on) is CSS
 * and lives in `WidgetHost.svelte`. WHERE it goes is this file, because it is a
 * number and an overlay window has no test harness in this app — the same split
 * `widget-geometry.ts` and `use-hot-rects.ts` are on.
 *
 * # Next to the widgets, not next to the screen
 *
 * The bar is placed against the bounding box of the widgets currently being
 * arranged rather than against the host, so it lands where the user's eyes
 * already are. It is not anchored to ONE widget: config mode arranges every
 * widget of the module at once and there is no "current" one until a drag
 * starts, so a per-widget anchor would jump between them on every press and
 * cover the widget next to whichever one was grabbed.
 *
 * Above the cluster is preferred over below it for one reason: a widget is
 * dragged by its interior, and the pointer spends the gesture INSIDE the box.
 * Below the cluster the bar sits under the hand doing the dragging.
 *
 * Every answer is clamped into the host, so the bar is always fully on screen —
 * a Save button hanging off the edge of the monitor is the shipped bug again
 * with a different cause.
 */
import { clampToHost, type HostSize, type WidgetRect } from './widget-geometry';

/**
 * The breathing room between the bar and the widgets, in CSS px.
 *
 * Also the bar's margin from the host edge in every fallback, so the one number
 * says "not touching" everywhere it is used.
 */
export const CONFIG_BAR_GAP = 16;

/** The bar's own measured box, in CSS px. */
export interface ConfigBarSize {
	width: number;
	height: number;
}

/** Where the host draws the bar — CSS px from the host's top-left. */
export interface ConfigBarAnchor {
	x: number;
	y: number;
}

/** The union of every rectangle, or `null` when there are none. */
function boundingBox(rects: readonly WidgetRect[]): WidgetRect | null {
	let box: WidgetRect | null = null;
	for (const rect of rects) {
		// A degenerate frame is not a position to anchor against; config mode
		// draws a minimum-size frame for a widget whose module is drawing
		// nothing, but the DRAFT can still carry a zero-size rectangle for a
		// content-sized widget that has not been measured yet.
		if (!(rect.w > 0) || !(rect.h > 0)) continue;
		if (!box) {
			box = { ...rect };
			continue;
		}
		const right = Math.max(box.x + box.w, rect.x + rect.w);
		const bottom = Math.max(box.y + box.h, rect.y + rect.h);
		box.x = Math.min(box.x, rect.x);
		box.y = Math.min(box.y, rect.y);
		box.w = right - box.x;
		box.h = bottom - box.y;
	}
	return box;
}

/**
 * Where to draw the bar, given the widgets being arranged and the host.
 *
 * The order of preference, and what each answer is for:
 *
 * 1. **Above the cluster**, horizontally centred on it — the default, and where
 *    the bar is out from under the hand that is dragging.
 * 2. **Below the cluster**, when there is not a bar's height plus a gap above
 *    it. A widget placed near the top of the screen is ordinary — a user drags
 *    one there whenever the game's own chrome leaves the top free — so this is
 *    not an edge case. It is also, since POE-249, **where the shipped defaults
 *    land**: the temple declares two placeable widgets, `temple.door` at
 *    y = 300 and `temple.waiting` at y = 16, so the cluster's top edge is 16 px
 *    from the top of the monitor and no bar fits above it. (Between POE-244 and
 *    POE-249 the door was the only placeable widget and branch 1 was what
 *    shipped defaults took.) The union of that pair spans x 40…1090, so the bar
 *    is centred on a span the widgets themselves do not occupy — it lands
 *    BETWEEN them rather than beside either one, which is arithmetically right
 *    and is a LOOK nobody has judged over the game yet: `docs/OVERLAY-GUIDE.md`
 *    carries it as an owner-judgement smoke item, and a bad verdict there
 *    changes the shipped defaults rather than this function.
 *    `widget-config-bar.test.ts` pins the answer against the registry rather
 *    than against these numbers, so a widget added near the top edge moves the
 *    assertion rather than silently invalidating this paragraph.
 * 3. **The top of the host**, when the cluster leaves room on neither side. The
 *    bar then overlaps a widget, which is survivable — it is drawn after the
 *    widgets and so sits over them — while being off screen is not.
 *
 * With NO widgets to anchor against, the bar goes to the top of the host,
 * centred. That is not the Show checkbox — `seedRect` ignores `visible`, so a
 * hidden widget still gets a frame to place. Nor is it the ANCHORED widgets:
 * `enterConfig` seeds `placeableWidgetsFor(module)`, and an anchored widget
 * (`temple.advice`) is neither persisted nor arranged, so it is absent by
 * design rather than missing. It is an UNRESOLVED SCALE FACTOR: `seedRect`
 * returns `null` for a widget that has a stored placement it cannot convert,
 * and with every placeable widget stored that is an empty draft. There is then
 * nothing on screen but the bar, which is exactly when being findable matters
 * most.
 *
 * An UNMEASURED host (either extent zero) gets the gap as both coordinates.
 * `WidgetHost` measures itself in an on-mount effect, so no frame this function
 * runs in is known to carry one — the guard is here because centring against a
 * zero width is a negative offset, i.e. off the left of the screen, and a
 * placement rule that can answer that is not total.
 */
export function configBarAnchor(
	rects: readonly WidgetRect[],
	host: HostSize,
	bar: ConfigBarSize,
	gap: number = CONFIG_BAR_GAP
): ConfigBarAnchor {
	if (!(host.width > 0) || !(host.height > 0)) {
		return { x: gap, y: gap };
	}
	const box = boundingBox(rects);
	const centred = box ? box.x + box.w / 2 - bar.width / 2 : host.width / 2 - bar.width / 2;

	let y: number;
	if (!box) {
		y = gap;
	} else {
		const above = box.y - gap - bar.height;
		const below = box.y + box.h + gap;
		if (above >= 0) y = above;
		else if (below + bar.height <= host.height) y = below;
		else y = gap;
	}

	// The same clamp a widget's own placement goes through, so the bar cannot
	// leave the host by any route — a cluster centred near an edge, a bar wider
	// than the screen, or the fallbacks above.
	const placed = clampToHost({ x: centred, y, w: bar.width, h: bar.height }, host);
	return { x: placed.x, y: placed.y };
}
