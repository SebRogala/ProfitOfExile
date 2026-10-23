import { describe, expect, it } from 'vitest';
import { comparatorPrice } from './comparator-price';

describe('comparatorPrice', () => {
	it('uses the built trade quote when listings exist', () => {
		// Deliberately inconsistent listing amounts: this layer selects the built
		// quote rather than reimplementing the median or taking the first ask.
		const trade = { listings: [{ price: 1 }], medianTop3: 50 };
		expect(comparatorPrice(45, false, trade)).toEqual({ label: 'price', chaos: 50 });
	});

	it('uses trade for a gem with no ninja data', () => {
		const trade = { listings: [{ price: 40 }], medianTop3: 40 };
		expect(comparatorPrice(0, true, trade)).toEqual({ label: 'price', chaos: 40 });
	});

	it.each([null, undefined])('keeps ninja when trade is %s', (trade) => {
		expect(comparatorPrice(45, false, trade)).toEqual({ label: 'ninja', chaos: 45 });
	});

	it('keeps ninja when a lookup found no listings', () => {
		expect(comparatorPrice(45, false, { listings: [], medianTop3: 0 }))
			.toEqual({ label: 'ninja', chaos: 45 });
	});

	it('leaves a NO_DATA gem unpriced when trade has no listings', () => {
		expect(comparatorPrice(0, true, { listings: [], medianTop3: 0 }))
			.toEqual({ label: 'ninja', chaos: NaN });
	});

	it('selects the new quote for a replacement lookup', () => {
		const first = { listings: [{ price: 40 }], medianTop3: 50 };
		const refreshed = { listings: [{ price: 60 }], medianTop3: 70 };
		const displayed = [first, refreshed].map((trade) => comparatorPrice(45, false, trade));
		expect(displayed).toEqual([{ label: 'price', chaos: 50 }, { label: 'price', chaos: 70 }]);
	});
});
