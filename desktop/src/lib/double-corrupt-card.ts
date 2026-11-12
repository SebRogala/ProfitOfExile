import type { CompareGem } from './api';

/**
 * The marker of results whose EV also carries a guessed destroy chance.
 * Mirrors lab.DoubleCorruptModelProvisionalDestroy (internal/lab/doublecorrupt.go).
 */
export const DOUBLE_CORRUPT_MODEL_PROVISIONAL_DESTROY = 'estimated-provisional-destroy';

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

/**
 * The EV line's tooltip. A provisional-destroy result also rests on a guessed
 * destroy chance; rows stored before that outcome existed keep their own copy.
 * Tooltip renders this through innerHTML, so the copy holds no markup characters.
 */
export function doubleCorruptEvTooltip(gem: Pick<CompareGem, 'doubleCorruptModel'>): string {
	if (gem.doubleCorruptModel === DOUBLE_CORRUPT_MODEL_PROVISIONAL_DESTROY) {
		return 'Estimated: double-corruption odds come from community documentation, not from GGG. The chance that the corruption destroys the gem is guessed, not measured (a destroyed gem counts as nothing); it will be corrected when 3.30 data arrives. Read it as a rough expected value, not a price.';
	}
	return 'Estimated: double-corruption odds come from community documentation, not from GGG. Read it as a rough expected value, not a price.';
}

/**
 * The coverage line's tooltip. With the destroy outcome, destroyed gems are part
 * of the unpriced remainder, so "mostly structural" holds only for older rows.
 */
export function doubleCorruptCoverageTooltip(gem: Pick<CompareGem, 'doubleCorruptModel'>): string {
	if (gem.doubleCorruptModel === DOUBLE_CORRUPT_MODEL_PROVISIONAL_DESTROY) {
		return 'The estimate only counts outcomes the corrupted market prices. The rest is gems the corruption destroys, at an unverified guessed rate, and outcomes poe.ninja does not list — it lists no corrupted gem below level 20, and a bad quality roll lands there.';
	}
	return 'The estimate only counts outcomes the corrupted market prices. The rest is mostly structural — poe.ninja lists no corrupted gem below level 20, and a bad quality roll lands there.';
}
