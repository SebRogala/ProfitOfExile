import { describe, expect, it } from 'vitest';
import { lensView, NEEDS_LENS_PRICE, NEEDS_SUPPORT_PRICE } from './font-recipes';
import type { FontLensCraft, LensQuote } from './api';

const NOW = Date.parse('2026-11-02T12:00:00Z');
const MIN = 60_000;
const HOUR = 60 * MIN;

const fmt = (c: number) => `${Math.round(c)}c`;
const fmtSigned = (c: number) => `${c < 0 ? '−' : '+'}${Math.round(Math.abs(c))}c`;

function quote(chaos: number | null, ageMs: number | null): LensQuote {
	return {
		chaos,
		listings: chaos === null ? 0 : 3,
		fetchedAt: ageMs === null ? null : new Date(NOW - ageMs).toISOString(),
		lowConfidence: false,
	};
}

function craft(
	lens: number | null,
	support: number | null,
	ev: number | null,
	ages: [number | null, number | null] = [18 * MIN, 18 * MIN],
): FontLensCraft {
	return { outcome: quote(lens, ages[0]), feeder: { ...quote(support, ages[1]), tradeUrl: null }, ev };
}

const view = (c: FontLensCraft | null | undefined) => lensView(c, NOW, fmt, fmtSigned);

describe('lensView', () => {
	it('prices both rows and shows the server EV signed when priced', () => {
		const v = view(craft(412, 95, 317));
		expect(v.lens).toEqual({ kind: 'price', text: '412c' });
		expect(v.support).toEqual({ kind: 'price', text: '95c' });
		expect(v.ev).toEqual({ kind: 'value', text: '+317c', negative: false });
	});

	it('flags a negative EV and keeps the real minus sign', () => {
		expect(view(craft(60, 95, -35)).ev).toEqual({ kind: 'value', text: '−35c', negative: true });
	});

	it('shows the server EV rather than recomputing it from the prices', () => {
		expect(view(craft(412, 95, 300)).ev).toMatchObject({ text: '+300c' });
	});

	it('reads loading for every price and the EV before the response arrives', () => {
		const v = view(undefined);
		expect([v.lens, v.support, v.ev]).toEqual([{ kind: 'loading' }, { kind: 'loading' }, { kind: 'loading' }]);
		expect(v.age).toBeNull();
	});

	it('treats a 0c quote as a price, not as missing', () => {
		expect(view(craft(0, 95, -95)).lens).toEqual({ kind: 'price', text: '0c' });
	});

	it('asks for the lens price when the lens quote is missing', () => {
		const v = view(craft(null, 95, null));
		expect(v.lens).toEqual({ kind: 'missing' });
		expect(v.ev).toEqual({ kind: 'missing', text: NEEDS_LENS_PRICE });
	});

	it('asks for the support price when only the support quote is missing', () => {
		const v = view(craft(412, null, null));
		expect(v.support).toEqual({ kind: 'missing' });
		expect(v.ev).toEqual({ kind: 'missing', text: NEEDS_SUPPORT_PRICE });
	});

	it('reads "no price" for both rows when the server sent no lens key', () => {
		const v = view(null);
		expect([v.lens, v.support]).toEqual([{ kind: 'missing' }, { kind: 'missing' }]);
		expect(v.ev).toEqual({ kind: 'missing', text: '— needs the lens price' });
		expect(v.age).toBeNull();
	});

	it('ages the card by the older of the two quotes', () => {
		expect(view(craft(412, 95, 317, [5 * MIN, 18 * MIN])).age).toEqual({ text: '18m ago', stale: false });
		expect(view(craft(412, 95, 317, [40 * MIN, 18 * MIN])).age).toEqual({ text: '40m ago', stale: false });
	});

	it('ages by the one quote that has been checked', () => {
		expect(view(craft(412, null, null, [null, 7 * MIN])).age).toEqual({ text: '7m ago', stale: false });
	});

	it.each([
		[59_999, '<1m', false],
		[59 * MIN, '59m ago', false],
		[HOUR, '1h ago', false],
		[2 * HOUR - 1, '1h ago', false],
		[2 * HOUR, '2h ago', true],
		[3 * HOUR, '3h ago', true],
	])('age %i ms reads %s (stale %s)', (ageMs, text, stale) => {
		expect(view(craft(412, 95, 317, [ageMs, 0])).age).toEqual({ text, stale });
	});
});
