import { describe, it, expect } from 'vitest';
import { showsDoubleCorruptCard, doubleCorruptHeadline, type DoubleCorruptCardGem } from './double-corrupt-card';

function cardGem(overrides: Partial<DoubleCorruptCardGem> = {}): DoubleCorruptCardGem {
	return {
		variant: '20/20',
		recommendation: 'OK',
		doubleCorruptModel: 'estimated',
		doubleCorruptProfit: 137.5,
		doubleCorruptTiebreak: false,
		...overrides,
	};
}

describe('showsDoubleCorruptCard', () => {
	it('shows the card for a modelled gem the craft pays for', () => {
		expect(showsDoubleCorruptCard(cardGem())).toBe(true);
	});

	it('hides the card when the corrupted market pays less than selling here', () => {
		expect(showsDoubleCorruptCard(cardGem({ doubleCorruptProfit: -12 }))).toBe(false);
	});

	it('hides the card for a gem the calculator never modelled, whatever the number says', () => {
		// The marker is the gate, not the profit: without it the fields are absent
		// from the wire, and a number read off an absent field is not an estimate.
		expect(showsDoubleCorruptCard(cardGem({ doubleCorruptModel: '', doubleCorruptProfit: 137.5 }))).toBe(false);
	});

	it('hides the card at exactly break-even', () => {
		// Same boundary the server's tiebreak uses (profit > 0): breaking even is
		// not a reason to hand the gem to the altar.
		expect(showsDoubleCorruptCard(cardGem({ doubleCorruptProfit: 0 }))).toBe(false);
	});
});

describe('doubleCorruptHeadline', () => {
	it('calls a non-winning gem weak at its own variant', () => {
		expect(doubleCorruptHeadline(cardGem({ recommendation: 'OK' })))
			.toBe('Weak as 20/20, strong double-corrupt candidate');
	});

	it('names the gem\'s own variant rather than a fixed one', () => {
		expect(doubleCorruptHeadline(cardGem({ recommendation: 'AVOID', variant: '21/20' })))
			.toBe('Weak as 21/20, strong double-corrupt candidate');
	});

	it('does not call an ordinary BEST weak — the badge already says it sells here', () => {
		expect(doubleCorruptHeadline(cardGem({ recommendation: 'BEST' })))
			.toBe('Strong double-corrupt candidate');
	});

	it('keeps the weak clause on a tiebreak BEST, which was promoted because nothing sold', () => {
		expect(doubleCorruptHeadline(cardGem({ recommendation: 'BEST', doubleCorruptTiebreak: true })))
			.toBe('Weak as 20/20, strong double-corrupt candidate');
	});
});
