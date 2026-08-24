import type { CompareGem } from './api';

/** The fields the double-corrupt card's copy is decided from. */
export type DoubleCorruptCardGem = Pick<
	CompareGem,
	'variant' | 'recommendation' | 'doubleCorruptModel' | 'doubleCorruptProfit' | 'doubleCorruptTiebreak'
>;

/**
 * Whether the comparator shows the double-corrupt card for this gem.
 *
 * `doubleCorruptModel` is the gate rather than the number: an unmodelled gem
 * carries an EV of 0 because the server had no data for it, not because the
 * craft is worth nothing. A non-positive profit means the corrupted market pays
 * less than selling the gem here, which is a reason not to corrupt — nothing to
 * put on a card.
 */
export function showsDoubleCorruptCard(gem: DoubleCorruptCardGem): boolean {
	return gem.doubleCorruptModel !== '' && gem.doubleCorruptProfit > 0;
}

/**
 * The card's headline.
 *
 * "Weak as {variant}" is a claim about the gem's own market, and the server has
 * already ruled on that: an ordinary BEST is the pick precisely because it sells
 * well at this variant. Printing "weak" under a green BEST badge contradicts the
 * badge, so the clause is dropped there and only the double-corrupt claim stays.
 *
 * A tiebreak BEST is not an ordinary winner — it was promoted because nothing in
 * the comparison sold, which is exactly the case the clause describes, so it
 * keeps it.
 */
export function doubleCorruptHeadline(gem: DoubleCorruptCardGem): string {
	const strong = 'strong double-corrupt candidate';
	if (gem.recommendation === 'BEST' && !gem.doubleCorruptTiebreak) {
		return 'Strong double-corrupt candidate';
	}
	return `Weak as ${gem.variant}, ${strong}`;
}
