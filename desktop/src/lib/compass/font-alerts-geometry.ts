/**
 * Where the Divine Font alert stack ships for a player who has not placed it
 * (POE-328): right of the font OCR region, top-aligned, a 12 px gap; left of
 * it when the right side does not fit; nowhere when neither does.
 *
 * The font and gem rectangles are Rust's `placements.lab` (capture px), only
 * converted here — never recomputed (ADR-019).
 */
import { captureToCss } from '$lib/temple/overlay-geometry';
import type { CaptureRect } from '$lib/temple/slice';
import { avoidRects } from '$lib/overlay/widgets/widget-avoid';
import type { HostSize, WidgetRect } from '$lib/overlay/widgets/widget-geometry';

/** CSS px, converted from capture px by the window's scale factor — not
 *  `ssot.screen.uiScale`; that is what reproduces (976, 220) at 1080p. */
export const FONT_ALERTS_GAP_CSS = 12;

/**
 * The stack's default origin in CSS px, or null.
 *
 * Null on the empty-set rule (no font region, or a scale factor that has not
 * resolved: unknown is not "everywhere is free"), when neither side of the
 * region has room, or when `avoidRects` finds no clear spot.
 */
export function fontAlertsDefaultPlacement(args: {
	font: CaptureRect | null;
	gem: CaptureRect | null;
	scaleFactor: number;
	box: { w: number; h: number };
	host: HostSize;
}): { x: number; y: number } | null {
	const font = args.font ? captureToCss(args.font, args.scaleFactor) : null;
	if (!font) return null;
	const obstacles: WidgetRect[] = [font];
	const gem = args.gem ? captureToCss(args.gem, args.scaleFactor) : null;
	if (gem) obstacles.push(gem);

	let x = font.x + font.w + FONT_ALERTS_GAP_CSS;
	if (x + args.box.w > args.host.width) {
		x = font.x - FONT_ALERTS_GAP_CSS - args.box.w;
		if (x < 0) return null;
	}
	const placed = avoidRects({ x, y: font.y, w: args.box.w, h: args.box.h }, obstacles, args.host);
	return placed ? { x: placed.x, y: placed.y } : null;
}
