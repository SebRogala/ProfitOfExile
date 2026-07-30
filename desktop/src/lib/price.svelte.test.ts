import { describe, it, expect } from 'vitest';
import { setDivineRate, formatPrice, formatPriceSigned, DIVINE_DISPLAY_FLOOR } from './price.svelte';

const RATE = 200;

describe('formatPrice', () => {
	it('writes chaos below the divine floor', () => {
		setDivineRate(RATE);
		// 4 divines' worth — still the number a player types into a trade.
		expect(formatPrice(800)).toBe('800c');
	});

	it('writes divines at the floor', () => {
		setDivineRate(RATE);
		expect(formatPrice(DIVINE_DISPLAY_FLOOR * RATE)).toBe('5.0d');
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
		setDivineRate(RATE);
		expect(formatPrice(800)).toBe('800c');
		// Same amount, cheaper divine: 800c is now 8 divines, past the floor.
		setDivineRate(100);
		expect(formatPrice(800)).toBe('8.0d');
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

	it('applies the divine floor to the magnitude, not the signed value', () => {
		setDivineRate(RATE);
		// -1200 is 6 divines below zero: the floor test must not compare -1200
		// against +1000 and conclude it is a small number.
		expect(formatPriceSigned(-1200)).toBe('−6.0d');
	});
});
