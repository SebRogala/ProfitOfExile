import { describe, expect, it } from 'vitest';
import fixture from './__fixtures__/chaos-div.json';
import {
	calculateChaosDiv,
	CHAOS_DIV_DEFAULTS,
	formatChaosDivWhole,
	formatChaosDivItems,
	normalizeChaosDivAmount,
	switchChaosDivMode,
	type ChaosDivInput
} from './chaos-div';

const defaults = (): ChaosDivInput => ({ ...CHAOS_DIV_DEFAULTS, rate: 420 });

describe('handoff calculation vectors', () => {
	for (const vector of fixture.vectors) {
		describe(vector.case, () => {
			for (const key of ['perItemChaos', 'totalChaos', 'totalDivine', 'split', 'tableShown', 'rows'] as const) {
				it(`produces the specified ${key}`, () => {
					expect(calculateChaosDiv(vector.input as ChaosDivInput)[key]).toEqual(vector.expect[key]);
				});
			}
			if ('minimumOrderMessage' in vector.expect) {
				it('shows the specified minimum whole order message', () => {
					expect(calculateChaosDiv(vector.input as ChaosDivInput).minimumOrderMessage).toBe(vector.expect.minimumOrderMessage);
				});
			}
		});
	}
});

describe('mode changes', () => {
	for (const mode of ['sell', 'buy'] as const) {
		for (const oneSide of ['left', 'right'] as const) {
			it(`keeps ${mode} with the fixed one on the ${oneSide} when the active mode is clicked`, () => {
				expect(switchChaosDivMode({ mode, oneSide }, mode)).toEqual({ mode, oneSide });
			});
		}
	}
	it.each(fixture.modeSwitchVectors)('$case', (vector) => {
		expect(switchChaosDivMode(vector.input as Pick<ChaosDivInput, 'mode' | 'oneSide'>, vector.mode as ChaosDivInput['mode']))
			.toEqual(vector.expect);
	});
});

describe('page defaults', () => {
	it('restores the handoff inputs', () => {
		const { _note, ...expected } = fixture.defaults;
		expect(CHAOS_DIV_DEFAULTS).toEqual(expected);
	});
});

describe('unknown rate', () => {
	it('keeps Chaos totals while cold', () => {
		expect(calculateChaosDiv({ ...defaults(), amount: 645, rate: null }).totalChaos).toBe(645);
	});
	it('withholds the Divine total while cold', () => {
		expect(calculateChaosDiv({ ...defaults(), rate: null }).totalDivine).toBeNull();
	});
	it('withholds the split while cold', () => {
		expect(calculateChaosDiv({ ...defaults(), rate: null }).split).toBeNull();
	});
	it('withholds Divine-currency chaos totals while cold', () => {
		expect(calculateChaosDiv({ ...defaults(), currency: 'div', rate: null }).totalChaos).toBeNull();
	});
	it('prints an unknown Divine-currency per-item chaos value while cold', () => {
		expect(calculateChaosDiv({ ...defaults(), currency: 'div', rate: null }).perItemC).toBe('—');
	});
	it('prints an unknown per-item divine value while cold', () => {
		expect(calculateChaosDiv({ ...defaults(), rate: null }).perItemD).toBe('—');
	});
	it('calculates Divine-currency orders independently of the rate', () => {
		expect(calculateChaosDiv({ ...defaults(), currency: 'div', amount: 3, ratio: 645, rate: null }).rows)
			.toEqual(fixture.vectors[2].expect.rows);
	});
	it.each([0, -1, NaN, Infinity])('treats rate %s as unknown', (rate) => {
		expect(calculateChaosDiv({ ...defaults(), rate }).totalDivine).toBeNull();
	});
});

describe('amount boundaries', () => {
	it.each([0, -1, NaN, Infinity])('normalizes amount %s to one item', (amount) => {
		expect(normalizeChaosDivAmount(amount)).toBe(1);
	});
	it('truncates a fractional amount', () => {
		expect(calculateChaosDiv({ ...defaults(), amount: 3.9, ratio: 10 }).totalChaos).toBe(30);
	});
	it('retains the canvas upper bound', () => {
		expect(normalizeChaosDivAmount(100_000)).toBe(100_000);
	});
	it('clamps above the canvas upper bound', () => {
		expect(calculateChaosDiv({ ...defaults(), amount: 100_001 }).totalChaos).toBe(100_000);
	});
});

describe('invalid ratio', () => {
	it.each([0, -1, NaN, Infinity, Number.MIN_VALUE])('withholds totals for ratio %s', (ratio) => {
		expect(calculateChaosDiv({ ...defaults(), ratio, oneSide: 'left' }).totalChaos).toBeNull();
	});
	it.each([0, -1, NaN, Infinity])('hides orders for ratio %s', (ratio) => {
		expect(calculateChaosDiv({ ...defaults(), ratio, amount: 3 }).rows).toEqual([]);
	});
	it('withholds totals whose multiplication overflows', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 1e305, amount: 100_000 }).totalChaos).toBeNull();
	});
});

describe('display formats', () => {
	it('shows four chaos decimals below 0.1c', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 906, oneSide: 'left' }).perItemC).toBe('0.0011');
	});
	it('groups whole chaos from 1,000', () => {
		expect(formatChaosDivWhole(1935)).toBe('1,935');
	});
	it('keeps two chaos decimals below 10c', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 9.994 }).perItemC).toBe('9.99');
	});
	it('prints whole chaos at 10c', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 10 }).perItemC).toBe('10');
	});
	it('keeps four divine decimals below 0.1d', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 41.99 }).perItemD).toBe('0.1000');
	});
	it('keeps two divine decimals at 0.1d', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 42 }).perItemD).toBe('0.10');
	});
	it('rounds total chaos before converting to divine', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 1.49, rate: 3 }).totalDivine).toBe('0.33');
	});
});

describe('order boundaries', () => {
	for (const mode of ['sell', 'buy'] as const) {
		for (const currency of ['chaos', 'div'] as const) {
			it(`uses the exact 49-item minimum for ${mode} in ${currency}`, () => {
				const result = calculateChaosDiv({ ...defaults(), mode, currency, ratio: 49, amount: 49, oneSide: mode === 'sell' ? 'left' : 'right' });
				if (mode === 'sell') {
					expect(result.minimumOrderMessage).toBe(`At this ratio the smallest whole order is 49 items for 1${currency === 'chaos' ? 'c' : 'd'}.`);
				} else {
					expect(result.rows).toEqual([{ ratio: '24.50 : 1', delta: '+100.0%', iWant: 49, iHave: 2, leftOver: 0 }]);
				}
			});
		}
	}
	it('omits buy orders below one whole currency at the typed price', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', ratio: 906, amount: 4 }).rows).toEqual([]);
	});
	it('explains the minimum whole order when buying four items at 906 per chaos', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', ratio: 906, amount: 4 }).minimumOrderMessage)
			.toBe('At this ratio the smallest whole order is 906 items for 1c.');
	});
	it('finds the nearest undercut after snapping a typed 2.005 ratio', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 2.005, amount: 200 }).rows[0])
			.toEqual({ ratio: '2 : 1', delta: '−0.5%', iWant: 400, iHave: 200, leftOver: 0 });
	});
	it('uses the snapped price at the exact 2.015 half-step boundary', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 2.015, amount: 200 }).rows[0])
			.toEqual({ ratio: '2.01 : 1', delta: '−0.5%', iWant: 402, iHave: 200, leftOver: 0 });
	});
	it('hides an undercut table without a positive whole-currency order', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 0.5 }).tableShown).toBe(false);
	});
	it('excludes a candidate displayed as the typed ratio', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 0.995, amount: 200 }).rows.map((row) => row.ratio))
			.not.toContain('0.99 : 1');
	});
	it('swaps the ratio price when the fixed one moves left', () => {
		expect(calculateChaosDiv({ ...defaults(), ratio: 6.12, oneSide: 'left', amount: 100 }).totalChaos).toBe(16);
	});
	it('uses the direct price when buying with the fixed one on the left', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', ratio: 6.12, oneSide: 'left', amount: 100 }).totalChaos).toBe(612);
	});
});


describe('budget handoff vectors', () => {
	for (const vector of fixture.budgetVectors) {
		describe(vector.case, () => {
			for (const key of ['items', 'cost', 'left', 'belowOneMessage'] as const) {
				it(`produces the specified budget.${key}`, () => {
					expect(calculateChaosDiv(vector.input as ChaosDivInput).budget?.[key]).toEqual(vector.expect.budget[key]);
				});
			}
			for (const key of ['totalChaos', 'totalDivine', 'split', 'tableShown', 'rows'] as const) {
				it(`produces the specified ${key}`, () => {
					expect(calculateChaosDiv(vector.input as ChaosDivInput)[key]).toEqual(vector.expect[key]);
				});
			}
		});
	}
});

describe('buying with a whole-currency budget', () => {
	it.each([
		{ amount: 44, items: 12, cost: 42, left: 2 },
		{ amount: 39, items: 11, cost: 39, left: 0 }
	])('fits $items items into $amount div at 3.5 div each', ({ amount, items, cost, left }) => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', currency: 'div', amountKind: 'budget', amount, ratio: 3.5, oneSide: 'left' }).budget)
			.toEqual({ items, cost, left, belowOneMessage: null });
	});
	it('explains a budget below one item', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', currency: 'div', amountKind: 'budget', amount: 3, ratio: 3.5, oneSide: 'left' }).budget)
			.toEqual({ items: 0, cost: 0, left: 3, belowOneMessage: 'Budget below one item (3.5d)' });
	});
	it('drops an outbid that costs more than the budget', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', currency: 'div', amountKind: 'budget', amount: 42, ratio: 3.5, oneSide: 'left' }).rows.map(row => row.iHave))
			.toEqual([39, 32, 25, 36, 29, 22]);
	});
	it('keeps an exact seven-chaos cost at 0.07 chaos per item', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', amountKind: 'budget', amount: 7, ratio: 0.07, oneSide: 'left' }).budget)
			.toEqual({ items: 100, cost: 7, left: 0, belowOneMessage: null });
	});
	it('keeps an exact reciprocal 49-item order within one chaos', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', amountKind: 'budget', amount: 1, ratio: 49, oneSide: 'right' }).budget)
			.toEqual({ items: 49, cost: 1, left: 0, belowOneMessage: null });
	});
	it('converts the whole cost rather than the unrounded item subtotal', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', currency: 'div', amountKind: 'budget', amount: 39, ratio: 3.5, oneSide: 'left' }).totalChaos).toBe(16380);
	});
	it('keeps selected-currency budgeting available while the rate is cold', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', currency: 'div', amountKind: 'budget', amount: 44, ratio: 3.5, oneSide: 'left', rate: null }).budget)
			.toEqual({ items: 12, cost: 42, left: 2, belowOneMessage: null });
	});
	it('fits more than 100000 items without applying the item-input cap', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', amountKind: 'budget', amount: 100, ratio: 2000, oneSide: 'right' }).budget)
			.toEqual({ items: 200000, cost: 100, left: 0, belowOneMessage: null });
	});
});


it('matches a per-item scan above the item-input cap', () => {
	// Pinned from an independent scan of all 200000 item counts at 2000 items/c.
	expect(calculateChaosDiv({ ...defaults(), mode: 'buy', amountKind: 'budget', amount: 100, ratio: 2000, oneSide: 'right' }).rows).toEqual([
		{ ratio: '1999.99 : 1', delta: '+0.0%', iWant: 199999, iHave: 100, leftOver: 1, budgetLeft: 0 },
		{ ratio: '1999.98 : 1', delta: '+0.0%', iWant: 199998, iHave: 100, leftOver: 2, budgetLeft: 0 },
		{ ratio: '1999.97 : 1', delta: '+0.0%', iWant: 199997, iHave: 100, leftOver: 3, budgetLeft: 0 },
		{ ratio: '1999.96 : 1', delta: '+0.0%', iWant: 199996, iHave: 100, leftOver: 4, budgetLeft: 0 },
		{ ratio: '1999.95 : 1', delta: '+0.0%', iWant: 199995, iHave: 100, leftOver: 5, budgetLeft: 0 },
		{ ratio: '1999.94 : 1', delta: '+0.0%', iWant: 199994, iHave: 100, leftOver: 6, budgetLeft: 0 }
	]);
});


describe('budget without an affordable outbid', () => {
	it('explains that no outbid fits even though the typed order fits', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', currency: 'div', amountKind: 'budget', amount: 100, ratio: 100, oneSide: 'left' }).noOutbidMessage)
			.toBe('No outbid fits the budget.');
	});
	it('reserves the minimum whole-order hint for Items input', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', currency: 'div', amountKind: 'budget', amount: 100, ratio: 100, oneSide: 'left' }).minimumOrderMessage).toBeNull();
	});
	it('leaves the below-one-item message in charge when no item fits', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', currency: 'div', amountKind: 'budget', amount: 3, ratio: 3.5, oneSide: 'left' }).noOutbidMessage).toBeNull();
	});
	it('keeps the plain ratio-one conversion free of an outbid notice', () => {
		expect(calculateChaosDiv({ ...defaults(), mode: 'buy', amountKind: 'budget', amount: 1, ratio: 1 }).noOutbidMessage).toBeNull();
	});
});


describe('item count wording', () => {
	it('uses singular for one item in the summary and hint formatter', () => {
		expect(formatChaosDivItems(1)).toBe('1 item');
	});
	it('uses plural for multiple items in the summary and hint formatter', () => {
		expect(formatChaosDivItems(2)).toBe('2 items');
	});
});
