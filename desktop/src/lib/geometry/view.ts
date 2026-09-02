/**
 * Rendering decisions for the screen-geometry SSOT slice (POE-227).
 *
 * Pure, and separate from the Settings card that draws it, because the one rule
 * this file exists to keep is a rule about a MISSING value: `null` means nothing
 * has measured a screen, and it must never be rendered — or defaulted — as 1.0.
 * 1.0 is a real measurement (the 1920x1200 reference), so a card that printed it
 * for an unmeasured screen would look identical to one reporting a genuine
 * reference monitor, and every rect derived from it on a 1080p machine would be
 * 11% wrong with nothing on screen to say so.
 *
 * The lifecycle this surface describes is normative in
 * `desktop/src/lib/README.md` → "Screen Geometry (SSOT)".
 */

import type { ScreenSlice } from '$lib/stores/ssot.svelte';
// Reused rather than re-implemented: `formatTimeAgo` already has three homes
// with three wordings (README → Conventions → Relative time), and a fourth would
// be the one that finally makes them disagree.
import { formatTimeAgo } from '$lib/exchange/view';

/**
 * The wire strings Rust's `ScreenScaleSource` serialises to, spelled out for a
 * reader.
 *
 * A LABEL, not a rank: the latest measurement wins whichever cue produced it, so
 * these say what looked, never how much to trust it. An unrecognised string is
 * shown verbatim rather than mapped to a guess — a source this table does not
 * know is a Rust-side addition, and inventing a friendly name for it would hide
 * that.
 */
const SOURCE_LABELS: Record<string, string> = {
	'merc-frame': 'merc support grid',
	'merc-ocr': 'merc OCR line pitch',
	remembered: 'remembered from a previous run'
};

/** The four rows the Settings "Screen geometry" card prints. */
export interface ScreenGeometryView {
	/** `"1920×1200"`, or why there is no answer. */
	resolution: string;
	/** The scale to three decimals, or why there is no answer. */
	uiScale: string;
	/** What measured it, in words. */
	source: string;
	/** How long ago, relative. */
	measured: string;
	/** Nothing has measured a screen — every field above is a REASON, not a
	 *  value, and the card renders them muted. */
	unmeasured: boolean;
}

/**
 * What to print for `screen`, with `now` as the clock the relative age is
 * measured against (passed in so the caller owns the tick and the function stays
 * pure).
 */
export function screenGeometryView(screen: ScreenSlice | null, now: Date): ScreenGeometryView {
	if (screen === null) {
		return {
			resolution: 'Not measured yet',
			// Says the rule out loud on the surface that would otherwise be the
			// easiest place to break it.
			uiScale: 'Not measured yet — never treat as 1.0',
			source: 'Nothing has measured this screen',
			measured: 'Never',
			unmeasured: true
		};
	}
	return {
		resolution: `${screen.width}×${screen.height}`,
		// Three decimals: the publish deadband is 0.01 and one grid step of the
		// merc fit moves the number by 0.0031, so two decimals would hide a real
		// change and four would print noise.
		uiScale: screen.uiScale.toFixed(3),
		source: SOURCE_LABELS[screen.source] ?? screen.source,
		measured: formatTimeAgo(new Date(screen.measuredAtMs), now),
		unmeasured: false
	};
}
