import { describe, it, expect } from 'vitest';
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

	it('keeps two decimals for a sub-divine amount so it does not round to nothing', () => {
		setDivineRate(RATE);
		// 12c is 0.06 divines: one decimal would print "0.1d", and a cheap input
		// cost would read as a real fraction of a divine it is not.
		expect(formatPrice(12, 'divine')).toBe('0.06d');
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
	});

	it('applies the divine floor to the magnitude, not the signed value', () => {
		setDivineRate(RATE);
		// -1200 is 6 divines below zero: the floor test must not compare -1200
		// against +1000 and conclude it is a small number.
		expect(formatPriceSigned(-1200)).toBe('−6.0d');
	});
});
