import { describe, it, expect } from 'vitest';
import * as price from './price.svelte';
import { setDivineRate, formatPrice, formatPriceSigned, DIVINE_DISPLAY_FLOOR } from './price.svelte';

const RATE = 200;

describe('formatPrice', () => {
	it('writes chaos just below the divine floor', () => {
		setDivineRate(RATE);
		const justBelow = DIVINE_DISPLAY_FLOOR * RATE - 1;
		expect(formatPrice(justBelow)).toBe(`${justBelow}c`);
	});

	it('writes divines at the floor', () => {
		setDivineRate(RATE);
		expect(formatPrice(DIVINE_DISPLAY_FLOOR * RATE)).toBe(`${DIVINE_DISPLAY_FLOOR.toFixed(1)}d`);
	});

	it('writes divines above the floor to one decimal', () => {
		setDivineRate(RATE);
		expect(formatPrice(4385)).toBe('21.9d');
	});

	it('stays in chaos when the rate is unknown, however large the amount', () => {
		// A guessed rate would silently misreport every price on screen.
		setDivineRate(0);
		expect(formatPrice(4385)).toBe('4385c');
	});

	it('follows a rate change without any other input changing', () => {
		const justBelow = DIVINE_DISPLAY_FLOOR * RATE - 1;
		setDivineRate(RATE);
		expect(formatPrice(justBelow)).toBe(`${justBelow}c`);
		// Same amount, divine now half the price: the same chaos figure is worth
		// twice as many divines and crosses the floor.
		setDivineRate(RATE / 2);
		expect(formatPrice(justBelow)).toBe(`${(justBelow / (RATE / 2)).toFixed(1)}d`);
	});

	it('renders an unknown amount as a dash, not a zero valuation', () => {
		setDivineRate(RATE);
		expect(formatPrice(NaN)).toBe('\u2014');
	});

	it("writes divines below the floor when the caller asks for that unit", () => {
		setDivineRate(RATE);
		// A table that must not mix units down a column asks for 'divine'; the
		// floor is what an isolated figure uses, not a column.
		expect(formatPrice(300, 'divine')).toBe('1.5d');
	});

	it('carries the chaos figure in brackets below a divine', () => {
		setDivineRate(RATE);
		// 12c is 0.06 divines. Two decimals stop it rounding to "0.1d", and the
		// chaos number is the one a player would actually pay down here.
		expect(formatPrice(12, 'divine')).toBe('0.06d (12c)');
	});

	it('drops the bracket once a price reaches a divine', () => {
		setDivineRate(RATE);
		expect(formatPrice(RATE, 'divine')).toBe('1.0d');
	});

	it('falls back to chaos for the divine unit when the rate is unknown', () => {
		setDivineRate(0);
		expect(formatPrice(300, 'divine')).toBe('300c');
	});

	it('never converts when the caller asks for chaos', () => {
		setDivineRate(RATE);
		expect(formatPrice(4385, 'chaos')).toBe('4385c');
	});

	it('rounds chaos to whole units', () => {
		setDivineRate(RATE);
		expect(formatPrice(12.4)).toBe('12c');
	});
});

describe('formatPriceSigned', () => {
	it('marks a gain with a plus', () => {
		setDivineRate(RATE);
		expect(formatPriceSigned(150)).toBe('+150c');
	});

	it('marks a loss with a minus and no double sign', () => {
		setDivineRate(RATE);
		expect(formatPriceSigned(-150)).toBe('−150c');
	});

	it('passes the unit through to the magnitude', () => {
		setDivineRate(RATE);
		expect(formatPriceSigned(-300, 'divine')).toBe('−1.5d');
		// The sign sits outside the whole figure, brackets included.
		expect(formatPriceSigned(-12, 'divine')).toBe('−0.06d (12c)');
	});

	it('applies the divine floor to the magnitude, not the signed value', () => {
		setDivineRate(RATE);
		// -1200 is 6 divines below zero: the floor test must not compare -1200
		// against +1000 and conclude it is a small number.
		expect(formatPriceSigned(-1200)).toBe('−6.0d');
	});
});

type PriceParts = { amount: string; unit?: 'c' | 'd' };

// Read through the namespace so a missing export fails this table on an
// assertion while the rest of the formatter suite still loads.
const formatPriceParts = (price as unknown as { formatPriceParts?: (chaos: number) => PriceParts })
	.formatPriceParts;

describe('formatPriceParts', () => {
	it.each<{ label: string; rate: number; chaos: number; expected: PriceParts }>([
		{ label: 'a small amount in chaos', rate: RATE, chaos: 110, expected: { amount: '110', unit: 'c' } },
		{ label: 'an amount past the floor in divines', rate: RATE, chaos: 1020, expected: { amount: '5.1', unit: 'd' } },
		{ label: 'a large amount in chaos when the rate is unknown', rate: 0, chaos: 4385, expected: { amount: '4385', unit: 'c' } },
		{ label: 'an unknown amount as a dash with no unit', rate: RATE, chaos: NaN, expected: { amount: '\u2014' } },
	])('keep:happy splits $label, agreeing with formatPrice', ({ rate, chaos, expected }) => {
		setDivineRate(rate);
		expect(formatPriceParts, 'formatPriceParts is exported from price.svelte').toBeTypeOf('function');
		const parts = formatPriceParts!(chaos);
		expect(parts).toStrictEqual(expected);
		expect(parts.amount + (parts.unit ?? '')).toBe(formatPrice(chaos));
	});
});
