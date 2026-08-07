import type { CompareGem } from './api';

/** The fields the default-selection rule reads. */
export type SelectableGem = Pick<CompareGem, 'name' | 'transPrice' | 'sellabilityLabel'>;

/**
 * How much of its ninja price an UNLIKELY gem keeps when the default is
 * decided. A rival has to clear that share of the dearer gem's price to take
 * the pick.
 */
const UNLIKELY_PRICE_WEIGHT = 0.5;

/** A gem's price as the default rule weighs it. */
function pickWeight(gem: SelectableGem): number {
	return gem.sellabilityLabel === 'UNLIKELY' ? UNLIKELY_PRICE_WEIGHT : 1;
}

/**
 * The comparator overlay's default pick.
 *
 * Most expensive by ninja price, except that a gem the server labels UNLIKELY
 * keeps only half its price here. The dearer gem still wins unless a rival
 * clears that half.
 *
 * The label, not the score, is what the rule reads. The server derives the
 * label from the sellability score at fixed boundaries (UNLIKELY is below 20)
 * and never labels a gem it did not score, so the label separates "scored, and
 * nobody is buying" from "no signal row for this gem" — the score does not: an
 * unpopulated 0 and a computed 0 are the same number.
 *
 * Accepted trade-offs:
 * - Only UNLIKELY is discounted. Between SLOW, MODERATE and up, price decides
 *   alone, so a slow-moving dear gem still beats a fast-moving cheaper one.
 * - The discount is a fixed half, so a dear illiquid gem is still preferred to
 *   a much cheaper liquid one; the rule only changes the pick between gems of
 *   comparable price.
 * - Dedication mode serves no sellability at all — every row comes back
 *   unlabelled — so the pick there is price-only.
 *
 * A gem's `recommendation` is deliberately not consulted — the overlay used to
 * run that as a second, disagreeing rule.
 *
 * An existing selection is kept. This is the *default*, so it only decides what
 * is selected when nothing is, and the user's pick outranks it.
 */
export function defaultSelectedGem(
	results: readonly SelectableGem[],
	current: string | null,
): string | null {
	if (results.length === 0) return null;
	if (current) return current;

	let best = results[0];
	let bestScore = best.transPrice * pickWeight(best);
	for (const gem of results) {
		const score = gem.transPrice * pickWeight(gem);
		if (score > bestScore) {
			best = gem;
			bestScore = score;
		}
	}
	return best.name;
}
