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
 * Which unit a price is written in.
 *
 * - `auto` — chaos while small, divines past DIVINE_DISPLAY_FLOOR. The default,
 *   and right for a single figure read on its own.
 * - `divine` — always divines. For a table that must not mix units down a
 *   column: one row at 4385c beside another at 12c reads as two scales, and the
 *   eye cannot compare them.
 * - `chaos` — never converts. For values that only make sense in chaos.
 *
 * With no known rate every unit falls back to chaos: converting against a
 * guessed rate would misreport the number.
 */
export type PriceUnit = 'auto' | 'divine' | 'chaos';

/**
 * Format a chaos amount for display: "412c", or "15.4d" once the unit says so.
 * Fractions of a chaos are not meaningful, so chaos is rounded. Divines keep one
 * decimal, or two below a single divine, where one decimal would round a real
 * price to "0.0d".
 */
export function formatPrice(chaos: number, unit: PriceUnit = 'auto'): string {
	// An em dash, not "0c": the rest of the UI renders unknown that way, and a
	// zero here reads as a real valuation of nothing.
	if (!Number.isFinite(chaos)) return '\u2014';
	if (divineRate > 0 && unit !== 'chaos') {
		const divines = chaos / divineRate;
		if (unit === 'divine' || Math.abs(chaos) >= DIVINE_DISPLAY_FLOOR * divineRate) {
			return `${divines.toFixed(Math.abs(divines) >= 1 ? 1 : 2)}${DIVINE_SUFFIX}`;
		}
	}
	return `${Math.round(chaos)}${CHAOS_SUFFIX}`;
}

/**
 * Format a gain or delta with an explicit sign — "+15.4d", "−412c". Uses a
 * minus sign rather than a hyphen so the two line up in a column.
 */
export function formatPriceSigned(chaos: number, unit: PriceUnit = 'auto'): string {
	const sign = chaos < 0 ? '−' : '+';
	return `${sign}${formatPrice(Math.abs(chaos), unit)}`;
}
