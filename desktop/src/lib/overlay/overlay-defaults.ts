/**
 * Shipped overlay geometry, in ONE place, in CSS pixels.
 *
 * Two windows read the merc strip's default placement — the owning layout
 * (`routes/(app)/+layout.svelte`), which builds the real overlay, and the
 * Settings position flow (`lib/pages/SettingsPage.svelte`), which builds the
 * draggable config window and PERSISTS whatever it is saved at. When the two
 * disagreed, the config window opened at the older size and a Save wrote that
 * size back over the newer default forever. Found in review, 2026-08-25: the
 * layout had been raised to fit the POE-199 status strip and `OVERLAY_CONFIGS`
 * still carried the pre-strip height.
 *
 * So the numbers live here and neither consumer may spell one out. There is a
 * test for that (`overlay-defaults.test.ts`) which reads both sources.
 *
 * # Height is not here for the merc strip
 *
 * `h` below is a CONSTRUCTOR seed, not a setting. The merc verdict overlay
 * sizes itself to its own rendered content (`lib/overlay/content-height.ts` and
 * Rust's `fit_overlay_height`), because a shipped height is wrong on every
 * machine whose display scales and wrong again whenever the strip draws a
 * different number of rows. The seed only decides what the window looks like
 * for the frame between creation and first paint, after which the content
 * replaces it. Do not reason a height budget into it and do not persist it.
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
 * `x`, `y` and `w` are real defaults: the user places and widens the strip in
 * Settings → Overlay Positions and the result is persisted in
 * `mercenary_overlay`. These apply only until they have.
 *
 * `h` is the constructor seed described above — one status line's worth, so a
 * window that somehow never gets a content measurement is a thin strip rather
 * than a large empty box over the game. It is replaced on first paint.
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
