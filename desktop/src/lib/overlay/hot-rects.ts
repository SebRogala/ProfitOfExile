/**
 * Turning an overlay's own DOM into the rectangles the Windows mouse hook
 * claims clicks in.
 *
 * A click-through overlay is invisible to the mouse at the OS level, so the
 * only clicks its buttons ever receive are the ones the `WH_MOUSE_LL` hook
 * consumes and re-emits as `overlay-click`. The hook decides that from HOT
 * RECTS the page declares (`set_overlay_hot_rects`) — window-relative, in
 * PHYSICAL pixels, because `GetWindowRect` and the hook's cursor position are
 * both physical.
 *
 * What lives here is the conversion and the change test — the two parts that
 * can be wrong quietly, and neither route in this app has a unit-test harness.
 * The `bind:this`, the animation frame and the `invoke` are glue and stay in
 * the route, the way `content-height.ts` splits the merc strip's resize loop.
 */

/** A window-relative rectangle in physical pixels, as Rust's `HotRect`. */
export interface HotRect {
	x: number;
	y: number;
	w: number;
	h: number;
}

/** The part of a `DOMRect` this module reads. */
export interface MeasuredRect {
	left: number;
	top: number;
	right: number;
	bottom: number;
}

/**
 * The physical hot rect for a measured element, or null when there is nothing
 * to claim.
 *
 * Edges are rounded independently and the size derived from them, rather than
 * position and size being rounded separately: two adjacent buttons then share
 * an edge instead of overlapping or leaving a one-pixel seam the game receives
 * the click through.
 *
 * Both rejections are checked on the RESULT rather than on the inputs, so two
 * lines cover every way the answer can be unusable: a `NaN` or infinity
 * arriving from either argument, the empty rect an unmounted or
 * `display: none` element measures, and the all-zero rect the route's cached
 * `scaleFactor()` produces while it is still 0.
 */
export function physicalHotRect(rect: MeasuredRect, scaleFactor: number): HotRect | null {
	const x = Math.round(rect.left * scaleFactor);
	const y = Math.round(rect.top * scaleFactor);
	const w = Math.round(rect.right * scaleFactor) - x;
	const h = Math.round(rect.bottom * scaleFactor) - y;

	if (![x, y, w, h].every(Number.isFinite)) return null;
	if (w <= 0 || h <= 0) return null;

	return { x, y, w, h };
}

/**
 * Whether two declarations describe the same claim.
 *
 * The route re-measures on every frame something could have moved, and the
 * overlay's buttons mostly do not move; without this that is one IPC call per
 * animation frame for a window whose buttons are exactly where they were. Order
 * is significant — it is the order the route declares its elements in, and the
 * hook answers with the first match.
 */
export function hotRectsEqual(a: readonly HotRect[], b: readonly HotRect[]): boolean {
	if (a.length !== b.length) return false;
	return a.every((r, i) => r.x === b[i].x && r.y === b[i].y && r.w === b[i].w && r.h === b[i].h);
}
