import { describe, expect, it } from 'vitest';
import fixture from './__fixtures__/chaos-div.json';
import {
	calculateChaosDiv,
	CHAOS_DIV_DEFAULTS,
	formatChaosDivWhole,
	normalizeChaosDivAmount,
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
		});
	}
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
