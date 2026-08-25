/**
 * Deciding when a content-driven overlay should ask to be resized.
 *
 * The merc verdict strip has no persisted height (POE-199, owner decision
 * 2026-08-25): a shipped number is wrong on every machine that scales its
 * display, and wrong again whenever the strip draws a different number of rows.
 * The route watches its own panel with a `ResizeObserver` and hands the
 * measured CSS height to Rust's `fit_overlay_height`, which converts, clamps to
 * the work area and applies it.
 *
 * What lives here is the small part of that loop that can be wrong quietly, and
 * the route has no unit-test harness in this app. The observer, the animation
 * frame and the `invoke` are glue and stay there.
 */

/**
 * How much a height has to move before it is worth an IPC call.
 *
 * `ResizeObserver` reports fractional CSS pixels, and font metrics make a panel
 * that is redrawing identical content settle a few hundredths of a pixel away
 * from where it was. Without a threshold that is one Rust call per animation
 * frame, forever, for a window whose size never actually changes.
 *
 * One pixel, because that is the smallest change the PHYSICAL size can express
 * at 100 % scaling — anything finer cannot survive the `ceil` on the other side
 * anyway.
 */
export const HEIGHT_EPSILON_PX = 1;

/**
 * The CSS height to ask for, or null when the observation is not worth sending.
 *
 * Two rejections, and both are load-bearing:
 *
 * - **A non-positive or non-finite height** is the mounting frame. An overlay
 *   route reports 0 for the tick between the element existing and the content
 *   painting, and sending it would collapse the window to Rust's floor and back
 *   — a visible flicker over the game on every module start.
 * - **A change under [`HEIGHT_EPSILON_PX`]** is sub-pixel jitter, not a resize.
 *
 * `lastSent` is the last height this window actually asked for, NOT the last
 * one observed: comparing against the observation would let a slow drift of
 * sub-epsilon steps move the window without ever tripping the threshold.
 */
export function overlayHeightRequest(observed: number, lastSent: number | null): number | null {
	if (!Number.isFinite(observed) || observed <= 0) return null;
	if (lastSent !== null && Math.abs(observed - lastSent) < HEIGHT_EPSILON_PX) return null;
	return observed;
}
