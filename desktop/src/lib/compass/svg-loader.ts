/**
 * SVG loader for room preset backgrounds.
 * Resolves area codes to SVG URLs served from /compass/presets/.
 */

import { VALID_AREA_CODES } from './room-presets';

/**
 * Get the URL for a room preset SVG by area code.
 * Returns null for invalid or empty area codes (e.g. Aspirant's Trial rooms).
 */
export function getRoomSvgUrl(areaCode: string): string | null {
	if (!areaCode || !VALID_AREA_CODES.has(areaCode)) return null;
	return `/compass/presets/${areaCode}.svg`;
}

/**
 * Get the fallback SVG URL (disabled/empty room display).
 */
export function getDisabledSvgUrl(): string {
	return '/compass/presets/disabled.svg';
}
