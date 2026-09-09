import type { CompareGem } from './api';

/** The fields the default-selection rule reads. */
export type SelectableGem = Pick<CompareGem, 'name' | 'transPrice' | 'sellabilityLabel'>;

/**
 * The part of a trade lookup the rule reads: the chaos floor and whether any
 * listing stood behind it. `TradeLookupResult` satisfies it as-is.
 */
export type PriceableLookup = { priceFloor: number; listings: ReadonlyArray<unknown> };

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
 * The chaos price the rule ranks a gem by: the live trade floor once a lookup
 * has come back with listings, else the ninja price. Ninja lags and averages;
 * the floor is what the gem sells for now (owner, 2026-09-09: a 359c ninja
 * price over a 14c floor kept the wrong gem picked after its lookup landed). A
 * lookup with no listings prices nothing and leaves the ninja price standing.
 */
export function pickPrice(gem: SelectableGem, lookup?: PriceableLookup | null): number {
	if (lookup && lookup.listings.length > 0 && lookup.priceFloor > 0) return lookup.priceFloor;
	return gem.transPrice;
}

/**
 * The comparator overlay's default pick.
 *
 * Most expensive by chaos price — the trade floor where a lookup in `lookups`
 * has listings, the ninja price otherwise (`pickPrice`) — except that a gem the
 * server labels UNLIKELY keeps only half its price here. The dearer gem still
 * wins unless a rival clears that half. Lookups land one gem at a time, so the
 * caller re-runs this as they do and the pick moves with them.
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
 * `current` is the player's own pick, never the previous default: it outranks
 * the rule while it is still among the results and is dropped once it is not.
 * The caller must not hand the last default back as `current` — gems arrive
 * one detection at a time, a second or two apart, and a default that kept
 * itself froze on the first gem for the whole run (2026-09-09: a 29c gem stayed
 * picked while a 42c one landed beside it).
 */
export function defaultSelectedGem(
	results: readonly SelectableGem[],
	current: string | null,
	lookups: Readonly<Record<string, PriceableLookup | null | undefined>> = {},
): string | null {
	if (results.length === 0) return null;
	if (current && results.some((gem) => gem.name === current)) return current;

	let best = results[0];
	let bestScore = pickPrice(best, lookups[best.name]) * pickWeight(best);
	for (const gem of results) {
		const score = pickPrice(gem, lookups[gem.name]) * pickWeight(gem);
		if (score > bestScore) {
			best = gem;
			bestScore = score;
		}
	}
	return best.name;
}
