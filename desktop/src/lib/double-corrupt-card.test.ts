import { describe, it, expect } from 'vitest';
import {
	showsDoubleCorruptCard,
	doubleCorruptHeadline,
	doubleCorruptEvTooltip,
	doubleCorruptCoverageTooltip,
	type DoubleCorruptCardGem,
} from './double-corrupt-card';

function cardGem(overrides: Partial<DoubleCorruptCardGem> = {}): DoubleCorruptCardGem {
	return {
		variant: '20/20',
		recommendation: 'OK',
		doubleCorruptModel: 'estimated-provisional-destroy',
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

describe('doubleCorruptEvTooltip', () => {
	it('says the EV rests on an unverified destroy rate for a provisional-destroy result', () => {
		expect(doubleCorruptEvTooltip(cardGem()))
			.toBe('Estimated: double-corruption odds come from community documentation, not from GGG. The chance that the corruption destroys the gem is guessed, not measured (a destroyed gem counts as nothing); it will be corrected when 3.30 data arrives. Read it as a rough expected value, not a price.');
	});

	it('keeps the community-documentation copy for a result estimated before the destroy outcome', () => {
		expect(doubleCorruptEvTooltip(cardGem({ doubleCorruptModel: 'estimated' })))
			.toBe('Estimated: double-corruption odds come from community documentation, not from GGG. Read it as a rough expected value, not a price.');
	});
});

describe('doubleCorruptCoverageTooltip', () => {
	it('counts destroyed gems in the unpriced remainder for a provisional-destroy result', () => {
		expect(doubleCorruptCoverageTooltip(cardGem()))
			.toBe('The estimate only counts outcomes the corrupted market prices. The rest is gems the corruption destroys, at an unverified guessed rate, and outcomes poe.ninja does not list — it lists no corrupted gem below level 20, and a bad quality roll lands there.');
	});

	it('keeps the structural-remainder copy for a result estimated before the destroy outcome', () => {
		expect(doubleCorruptCoverageTooltip(cardGem({ doubleCorruptModel: 'estimated' })))
			.toBe('The estimate only counts outcomes the corrupted market prices. The rest is mostly structural — poe.ninja lists no corrupted gem below level 20, and a bad quality roll lands there.');
	});
});
