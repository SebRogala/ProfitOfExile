/**
 * Placing a widget so that it covers NOTHING it must not cover (POE-244).
 *
 * The temple's kill callout and the shipped position of its door diamond are
 * drawn over the game while the module is still READING that same screen. Every
 * OCR crop and every sampled patch is a rectangle the next tick reads again, so
 * a panel drawn over one is input the app wrote itself — a confident, wrong
 * board with nothing anywhere reporting a failure. The rule is therefore
 * absolute rather than a preference, and this file is the arithmetic that
 * enforces it.
 *
 * It governs BOXES a PLACER derives from the game, and since POE-248 that is
 * everything the temple's own placers produce. There was one exception — the
 * callout's arrow, on the argument that a thin line crossing a crop is not what
 * breaks an OCR read while a filled shape sitting on the glyphs is — and it
 * went with the arrow (ADR-019's amendment). A rectangle the USER placed never
 * came through here and still does not: `placementFor` honours a stored
 * placement without consulting an obstacle set, because refusing to draw a
 * widget where its owner put it would be the app overruling a decision it
 * asked for.
 *
 * Pure, and in its own module rather than inside `widget-geometry.ts`, because
 * that file is about the placement a USER makes and this one is about a
 * placement the user cannot make. They share only `WidgetRect` and `HostSize`.
 *
 * # Units
 *
 * CSS pixels throughout — the unit the host lays out in. Rects that arrive in
 * capture px (the temple's `layout.rois`) are converted by the caller before
 * they reach this file, so there is one conversion site and it is the module's,
 * not this module's.
 */
import type { HostSize, WidgetRect } from './widget-geometry';

/**
 * Whether two rectangles share any area.
 *
 * Strict: a box whose left edge sits exactly on an obstacle's right edge is
 * ADJACENT, not overlapping, and that is the answer the placement below wants —
 * the tightest legal position is flush against the thing it must avoid, and
 * treating a shared border as a collision would push every placement one pixel
 * further out for no reason.
 *
 * A rectangle with no area cannot overlap anything, which is what makes an
 * obstacle list safe to pass through unfiltered.
 */
export function rectsOverlap(a: WidgetRect, b: WidgetRect): boolean {
	if (a.w <= 0 || a.h <= 0 || b.w <= 0 || b.h <= 0) return false;
	return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

/** Whether `rect` is clear of every obstacle. */
export function rectIsClear(rect: WidgetRect, obstacles: readonly WidgetRect[]): boolean {
	return !obstacles.some((obstacle) => rectsOverlap(rect, obstacle));
}

/** A number pinned into `[lo, hi]`, with `hi` never below `lo` — the same rule
 *  `widget-geometry.ts` clamps with, repeated rather than exported from there
 *  because it is three lines and the import would be the only coupling. */
function clamp(value: number, lo: number, hi: number): number {
	return Math.min(Math.max(value, lo), Math.max(lo, hi));
}

/** Every distinct value in `values`, in first-seen order. */
function distinct(values: number[]): number[] {
	return [...new Set(values)];
}

/**
 * The nearest position to `candidate` that is inside the host and clear of
 * every obstacle, or `null` when there is none.
 *
 * `null` is a real answer and the callers act on it: the box is not drawn.
 * Drawing it anyway "because it is mostly clear" is the one outcome this
 * function exists to prevent, and a placement that silently degrades the read
 * is worse than a callout the player did not get — the board is on screen
 * either way.
 *
 * # How the positions are generated
 *
 * The size never changes; only the origin moves. A rectangle that is clear can
 * always be slid until it is FLUSH against something without stopping being
 * clear, so the tightest legal positions all have an edge on an obstacle edge
 * or on a host edge. The candidate x values are therefore `candidate.x`, the two
 * host edges, and for every obstacle the two x values that put the box exactly
 * left of it and exactly right of it; y is the same list on the other axis.
 * Every pair is clamped into the host, deduplicated, and tried nearest-first, so
 * the common case — the wanted position is already clear — costs one overlap
 * scan and the worst case is bounded by the obstacle count rather than by the
 * size of the screen.
 *
 * Distance is measured from the CLAMPED wanted origin, so a candidate that
 * starts off screen is compared against where it could actually have gone.
 *
 * A box LARGER than the host is not a placement problem — `clamp` pins it to
 * the origin, it overlaps whatever is there, and the answer is `null`.
 */
export function avoidRects(
	candidate: WidgetRect,
	obstacles: readonly WidgetRect[],
	host: HostSize
): WidgetRect | null {
	const fit = (x: number, y: number): WidgetRect => ({
		x: clamp(x, 0, host.width - candidate.w),
		y: clamp(y, 0, host.height - candidate.h),
		w: candidate.w,
		h: candidate.h
	});

	const wanted = fit(candidate.x, candidate.y);
	if (rectIsClear(wanted, obstacles)) return wanted;

	const xs = distinct([
		wanted.x,
		0,
		host.width - candidate.w,
		...obstacles.flatMap((o) => [o.x - candidate.w, o.x + o.w])
	]);
	const ys = distinct([
		wanted.y,
		0,
		host.height - candidate.h,
		...obstacles.flatMap((o) => [o.y - candidate.h, o.y + o.h])
	]);

	let best: WidgetRect | null = null;
	let bestDistance = Infinity;
	for (const x of xs) {
		for (const y of ys) {
			const placed = fit(x, y);
			const dx = placed.x - wanted.x;
			const dy = placed.y - wanted.y;
			const distance = dx * dx + dy * dy;
			// `>=` keeps the FIRST position at a given distance, and the lists
			// above are built wanted-value-first, so a tie resolves toward the
			// position the caller asked for rather than toward whichever
			// obstacle happens to be earlier in the list.
			if (distance >= bestDistance) continue;
			if (!rectIsClear(placed, obstacles)) continue;
			best = placed;
			bestDistance = distance;
		}
	}
	return best;
}
