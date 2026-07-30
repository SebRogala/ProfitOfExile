/**
 * How a chaos amount is written for a human — the single place the app decides.
 *
 * Every surface that shows a price we computed (EV, gain, input cost, ninja
 * price, a delta between two of them) formats it through here, so the whole app
 * switches units at the same point and a tuning change lands everywhere at once.
 *
 * NOT for trade listings. A listing carries the currency its seller asked in:
 * "3.5 div" and "450c" are two different asks, and rewriting either one
 * misreports what is actually on offer.
 */

/**
 * Below this many divines a price stays in chaos. Chaos is the number you type
 * into a trade, so it stays useful while it is small; past a couple of divines
 * the digits stop carrying meaning and the divine figure is what a player
 * compares. This is the tuning knob — every surface follows it.
 */
export const DIVINE_DISPLAY_FLOOR = 2;

/** Suffixes. Chaos is "c" and divine is "d", matching in-game shorthand. */
const CHAOS_SUFFIX = 'c';
const DIVINE_SUFFIX = 'd';

/**
 * Current divine price in chaos. Zero means unknown — every price then renders
 * in chaos rather than against a guessed rate.
 */
let divineRate = $state(0);

/** Feed the rate from the server status. Non-positive values read as unknown. */
export function setDivineRate(chaosPerDivine: number): void {
	divineRate = chaosPerDivine > 0 ? chaosPerDivine : 0;
}

/** The rate in use, for surfaces that show it (e.g. "1 div = 230c"). */
export function getDivineRate(): number {
	return divineRate;
}

/**
 * Format a chaos amount for display: "412c" below the divine floor, "15.4d"
 * above it. Fractions of a chaos are not meaningful, so chaos is rounded.
 */
export function formatPrice(chaos: number): string {
	if (!Number.isFinite(chaos)) return `0${CHAOS_SUFFIX}`;
	if (divineRate > 0 && Math.abs(chaos) >= DIVINE_DISPLAY_FLOOR * divineRate) {
		return `${(chaos / divineRate).toFixed(1)}${DIVINE_SUFFIX}`;
	}
	return `${Math.round(chaos)}${CHAOS_SUFFIX}`;
}

/**
 * Format a gain or delta with an explicit sign — "+15.4d", "−412c". Uses a
 * minus sign rather than a hyphen so the two line up in a column.
 */
export function formatPriceSigned(chaos: number): string {
	const sign = chaos < 0 ? '−' : '+';
	return `${sign}${formatPrice(Math.abs(chaos))}`;
}
