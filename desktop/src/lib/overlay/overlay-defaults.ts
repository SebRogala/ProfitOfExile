/**
 * Shipped overlay geometry, in ONE place, in CSS pixels.
 *
 * The merc widget registry reads this placement and the widget host persists
 * the user's result in `Settings.widgets`. Keeping the numbers here gives the
 * widget one source for its shipped position and width.
 *
 * # Height is not a setting for the merc widget
 *
 * `h` below is a first-frame and config seed, not a setting. The widget host
 * sizes the merc verdict to its rendered content, because a shipped height is
 * wrong whenever the widget draws a different number of rows. Do not reason a
 * height budget into it and do not persist it.
 *
 * # The unit, and why there is a conversion
 *
 * These are **CSS pixels** — the unit the strip's contents are actually sized
 * in, so a height budget can be reasoned about by adding up font sizes and
 * padding. Tauri wants **physical** pixels for `PhysicalSize`/`PhysicalPosition`
 * and for the persisted settings, and on a 150 %-scaled Windows display those
 * are not the same number. Shipping the CSS figure as a physical one made the
 * window a third short of its own budget on exactly the machines that scale.
 *
 * [`physicalGeometry`] is the conversion, and it is the only thing that should
 * ever be handed to Tauri. Persisted geometry is ALREADY physical and must not
 * go through it.
 */

/** One overlay's shipped placement and size, in CSS pixels. */
export interface OverlayDefaultGeometry {
	x: number;
	y: number;
	w: number;
	h: number;
}

/**
 * The merc verdict strip (POE-199).
 *
 * `x`, `y` and `w` are real defaults: the user places and widens the widget in
 * Settings → Overlay Positions and the result is persisted in
 * `Settings.widgets`. These apply only until they have.
 *
 * `h` is the first-frame/config seed described above — one status line's worth,
 * so a widget that somehow never gets a content measurement is a thin strip
 * rather than a large empty box over the game.
 */
export const MERC_OVERLAY_DEFAULTS: OverlayDefaultGeometry = {
	x: 40,
	y: 300,
	w: 460,
	h: 40
};

/**
 * CSS pixels → physical pixels, for the one moment a shipped default is handed
 * to Tauri.
 *
 * Rounded, because `PhysicalSize` is integral and a fractional scale factor
 * (Windows' 125 % / 150 %) does not divide these evenly. Rounding rather than
 * flooring keeps the budget on the safe side of the last glyph row.
 *
 * A scale factor of 0 or less would collapse the window to nothing, and
 * `scaleFactor()` failing is handled by its callers falling back to 1 — this
 * guards the same way rather than trusting that every future caller will.
 */
export function physicalGeometry(
	defaults: OverlayDefaultGeometry,
	scaleFactor: number
): OverlayDefaultGeometry {
	const sf = scaleFactor > 0 ? scaleFactor : 1;
	return {
		x: Math.round(defaults.x * sf),
		y: Math.round(defaults.y * sf),
		w: Math.round(defaults.w * sf),
		h: Math.round(defaults.h * sf)
	};
}
