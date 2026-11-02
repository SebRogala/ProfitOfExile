import { describe, expect, it } from 'vitest';
import { parseDecimalInput } from './parse-number';
import { calculateChaosDiv, CHAOS_DIV_DEFAULTS } from './tools/chaos-div';

describe('decimal inputs', () => {
	it.each(['3,4', '3.4'])('values twenty items at a %s chaos ratio as 68 chaos', (raw) => {
		expect(calculateChaosDiv({ ...CHAOS_DIV_DEFAULTS, amount: 20, ratio: parseDecimalInput(raw), rate: 200 }).totalChaos).toBe(68);
	});
	it.each(['44,9', '44.9'])('keeps whole-budget normalization for %s', (raw) => {
		expect(calculateChaosDiv({ ...CHAOS_DIV_DEFAULTS, mode: 'buy', oneSide: 'left', amountKind: 'budget', amount: parseDecimalInput(raw), ratio: 3.5, rate: 200 }).budget)
			.toEqual({ items: 12, cost: 42, left: 2, belowOneMessage: null });
	});
	it.each(['', ' ', '3,4.5', '3,4,5', '3x', 'Infinity', '0x10'])('rejects invalid decimal input %j', (raw) => {
		expect(parseDecimalInput(raw)).toBeNaN();
	});
	it.each([[' ,5 ', 0.5], ['.5', 0.5], ['-2,5', -2.5], ['1e2', 100], ['0', 0]] as const)('reads %j as %s', (raw, expected) => {
		expect(parseDecimalInput(raw)).toBe(expected);
	});
});
