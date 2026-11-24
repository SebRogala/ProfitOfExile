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

const view = (c: FontLensCraft | null | undefined, showAllLensFeeders = false) =>
	lensView(c, NOW, fmt, fmtSigned, showAllLensFeeders);

const TRADE_NAME = 'Exceptional Support Gem lv 3+';
const ENHANCE_NAME = 'Enhance Support lv 3';
const ENLIGHTEN_NAME = 'Enlighten Support lv 3';
const EMPOWER_NAME = 'Empower Support lv 3';
const LENS_URL = 'https://www.pathofexile.com/trade/search/Probe%20League?q=lens';
const TRADE_SERVER_URL = 'https://www.pathofexile.com/trade/search/Probe%20League?q=trade-feeder';
const ENHANCE_URL = 'https://www.pathofexile.com/trade/search/Probe%20League?q=enhance';
const ENLIGHTEN_URL = 'https://www.pathofexile.com/trade/search/Probe%20League?q=enlighten';
const EMPOWER_URL = 'https://www.pathofexile.com/trade/search/Probe%20League?q=empower';

type Ages = { lens: number | null; trade: number | null; ninja: number | null };

/**
 * A server payload carrying the four feeder choices, as fetchFontEV maps it:
 * feeders in the server's fixed order trade, Enhance, Enlighten, Empower, the
 * server-chosen `cheapest` index and its `evCheapest`. The legacy `ev` is the
 * trade-only EV the server still sends, so a view that reads it shows a
 * different number than `evCheapest`.
 */
function feederCraft(
	lens: number | null,
	[trade, enhance, enlighten, empower]: [number | null, number | null, number | null, number | null],
	cheapest: number | null,
	evCheapest: number | null,
	ages: Ages = { lens: 5 * MIN, trade: 18 * MIN, ninja: 18 * MIN },
): FontLensCraft {
	const at = (ageMs: number | null) => (ageMs === null ? null : new Date(NOW - ageMs).toISOString());
	return {
		outcome: { ...quote(lens, ages.lens), tradeUrl: LENS_URL },
		feeder: { ...quote(trade, ages.trade), tradeUrl: TRADE_SERVER_URL },
		ev: lens !== null && trade !== null ? lens - trade : null,
		feeders: [
			{ source: 'trade', name: TRADE_NAME, chaos: trade, fetchedAt: at(ages.trade), tradeUrl: TRADE_SERVER_URL },
			{ source: 'poe.ninja', name: 'Enhance Support', chaos: enhance, fetchedAt: at(ages.ninja), tradeUrl: ENHANCE_URL },
			{ source: 'poe.ninja', name: 'Enlighten Support', chaos: enlighten, fetchedAt: at(ages.ninja), tradeUrl: ENLIGHTEN_URL },
			{ source: 'poe.ninja', name: 'Empower Support', chaos: empower, fetchedAt: at(ages.ninja), tradeUrl: EMPOWER_URL },
		],
		cheapest,
		evCheapest,
	};
}

/** The shown cost rows as name, sub and price cell, in display order. */
function rows(v: ReturnType<typeof lensView>) {
	return v.costs?.map(({ name, sub, cell }) => ({ name, sub, cell }));
}

const price = (text: string) => ({ kind: 'price', text }) as const;
const MISSING = { kind: 'missing' } as const;
const LOADING = { kind: 'loading' } as const;

describe('lensView', () => {
	it('prices both rows and shows the server EV signed when priced', () => {
		const v = view(craft(412, 95, 317));
		expect(v.lens).toEqual({ kind: 'price', text: '412c' });
		expect(rows(v)).toEqual([{ name: TRADE_NAME, sub: 'trade', cell: price('95c') }]);
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
		expect([v.lens, ...(v.costs ?? []).map((r) => r.cell), v.ev]).toEqual([LOADING, LOADING, LOADING]);
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
		expect(rows(v)).toEqual([{ name: TRADE_NAME, sub: 'trade', cell: MISSING }]);
		expect(v.ev).toEqual({ kind: 'missing', text: NEEDS_SUPPORT_PRICE });
	});

	it('reads "no price" for both rows when the server sent no lens key', () => {
		const v = view(null);
		expect([v.lens, ...(v.costs ?? []).map((r) => r.cell)]).toEqual([MISSING, MISSING]);
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

describe('lensView with the four feeder choices', () => {
	// keep:happy
	it.each([
		{
			name: 'collapsed: the cheapest feeder alone, Empower at 7c',
			craft: feederCraft(12400, [30, 12, 20, 7], 3, 12393),
			showAll: false,
			costs: [{ name: EMPOWER_NAME, sub: 'poe.ninja · cheapest of 4', cell: price('7c') }],
			ev: '+12393c',
		},
		{
			name: 'expanded: all four in fixed order, Empower marked cheapest',
			craft: feederCraft(12400, [30, 12, 20, 7], 3, 12393),
			showAll: true,
			costs: [
				{ name: TRADE_NAME, sub: 'trade', cell: price('30c') },
				{ name: ENHANCE_NAME, sub: 'poe.ninja', cell: price('12c') },
				{ name: ENLIGHTEN_NAME, sub: 'poe.ninja', cell: price('20c') },
				{ name: EMPOWER_NAME, sub: 'poe.ninja · cheapest', cell: price('7c') },
			],
			ev: '+12393c',
		},
		{
			name: 'collapsed: a 5c trade quote becomes the chosen row',
			craft: feederCraft(12400, [5, 12, 20, 7], 0, 12395),
			showAll: false,
			costs: [{ name: TRADE_NAME, sub: 'trade · cheapest of 4', cell: price('5c') }],
			ev: '+12395c',
		},
		{
			name: 'expanded: a 5c trade quote moves the cheapest mark to trade',
			craft: feederCraft(12400, [5, 12, 20, 7], 0, 12395),
			showAll: true,
			costs: [
				{ name: TRADE_NAME, sub: 'trade · cheapest', cell: price('5c') },
				{ name: ENHANCE_NAME, sub: 'poe.ninja', cell: price('12c') },
				{ name: ENLIGHTEN_NAME, sub: 'poe.ninja', cell: price('20c') },
				{ name: EMPOWER_NAME, sub: 'poe.ninja', cell: price('7c') },
			],
			ev: '+12395c',
		},
		{
			name: 'expanded: an unpriced trade feeder is never marked cheapest',
			craft: feederCraft(12400, [null, 12, 20, 7], 3, 12393),
			showAll: true,
			costs: [
				{ name: TRADE_NAME, sub: 'trade', cell: MISSING },
				{ name: ENHANCE_NAME, sub: 'poe.ninja', cell: price('12c') },
				{ name: ENLIGHTEN_NAME, sub: 'poe.ninja', cell: price('20c') },
				{ name: EMPOWER_NAME, sub: 'poe.ninja · cheapest', cell: price('7c') },
			],
			ev: '+12393c',
		},
	])('$name', ({ craft, showAll, costs, ev }) => {
		const v = view(craft, showAll);
		expect(v.lens).toEqual(price('12400c'));
		expect(rows(v)).toEqual(costs);
		expect(v.ev).toEqual({ kind: 'value', text: ev, negative: false });
	});

	// keep:happy
	it('links the Lens and each poe.ninja feeder to its server search, under the trade · poe.ninja footer', () => {
		const v = view(feederCraft(12400, [30, 12, 20, 7], 3, 12393), true);
		expect(v.lensUrl).toBe(LENS_URL);
		expect(v.costs?.slice(1).map((r) => r.url)).toEqual([ENHANCE_URL, ENLIGHTEN_URL, EMPOWER_URL]);
		expect(v.source).toBe('trade · poe.ninja');
	});

	// keep:critical — wrong external action: a BUY link carrying a non-PoE server URL
	// opens a malicious page (or runs a javascript: URL) from the desktop webview.
	it('keeps only pathofexile.com trade URLs from the server as Lens and feeder links', () => {
		const withUrl = (url: string) => {
			const c = feederCraft(12400, [30, 12, 20, 7], 3, 12393);
			return {
				...c,
				outcome: { ...c.outcome, tradeUrl: url },
				feeder: { ...c.feeder, tradeUrl: url },
				feeders: c.feeders!.map((f) => ({ ...f, tradeUrl: url })),
			};
		};
		const links = (url: string, showAll: boolean) => {
			const v = view(withUrl(url), showAll);
			return { lensUrl: v.lensUrl, urls: v.costs.map((r) => r.url) };
		};
		for (const rejected of ['javascript:alert(1)', 'https://evil.example/trade/x']) {
			expect(links(rejected, true), `${rejected}, all feeders`).toEqual({ lensUrl: null, urls: [null, null, null, null] });
			expect(links(rejected, false), `${rejected}, cheapest feeder`).toEqual({ lensUrl: null, urls: [null] });
		}
		const mirage = 'https://www.pathofexile.com/trade/search/Mirage?q=%7B%22query%22%3A%7B%22type%22%3A%22Facetor%27s%20Lens%22%7D%7D';
		expect(links(mirage, true)).toEqual({ lensUrl: mirage, urls: [mirage, mirage, mirage, mirage] });
		expect(links(mirage, false)).toEqual({ lensUrl: mirage, urls: [mirage] });
	});

	// keep:regression
	it.each([false, true])(
		'an older server payload (setting %s) keeps its one trade row, server ev, median footer and no Lens search',
		(showAll) => {
			const v = view(craft(412, 95, 317), showAll);
			expect(rows(v)).toEqual([{ name: TRADE_NAME, sub: 'trade', cell: price('95c') }]);
			expect(v.ev).toEqual({ kind: 'value', text: '+317c', negative: false });
			expect(v.source).toBe('trade · median of the 3 lowest');
			expect(v.lensUrl ?? null).toBeNull();
		},
	);

	// keep:critical — silent: a wrong EV, an invented price or a too-fresh age is a
	// plausible card the player trusts when choosing what to buy.
	it.each([
		{
			name: 'no gem snapshot, collapsed: the priced trade feeder is chosen; null asOf leaves the age',
			craft: feederCraft(12400, [30, null, null, null], 0, 12370, { lens: 5 * MIN, trade: 18 * MIN, ninja: null }),
			showAll: false,
			costs: [{ name: TRADE_NAME, sub: 'trade · cheapest of 4', cell: price('30c') }],
			ev: { kind: 'value', text: '+12370c', negative: false },
			age: { text: '18m ago', stale: false },
		},
		{
			name: 'no gem snapshot, expanded: the poe.ninja rows read no price',
			craft: feederCraft(12400, [30, null, null, null], 0, 12370, { lens: 5 * MIN, trade: 18 * MIN, ninja: null }),
			showAll: true,
			costs: [
				{ name: TRADE_NAME, sub: 'trade · cheapest', cell: price('30c') },
				{ name: ENHANCE_NAME, sub: 'poe.ninja', cell: MISSING },
				{ name: ENLIGHTEN_NAME, sub: 'poe.ninja', cell: MISSING },
				{ name: EMPOWER_NAME, sub: 'poe.ninja', cell: MISSING },
			],
			ev: { kind: 'value', text: '+12370c', negative: false },
			age: { text: '18m ago', stale: false },
		},
		{
			name: 'all feeders unpriced, collapsed: one unmarked trade-named row and a feeder-price ask',
			craft: feederCraft(12400, [null, null, null, null], null, null, { lens: 5 * MIN, trade: null, ninja: null }),
			showAll: false,
			costs: [{ name: TRADE_NAME, sub: 'cheapest of 4', cell: MISSING }],
			ev: { kind: 'missing', text: '— needs a feeder price' },
			age: { text: '5m ago', stale: false },
		},
		{
			name: 'all feeders unpriced, expanded: four source-only rows',
			craft: feederCraft(12400, [null, null, null, null], null, null, { lens: 5 * MIN, trade: null, ninja: null }),
			showAll: true,
			costs: [
				{ name: TRADE_NAME, sub: 'trade', cell: MISSING },
				{ name: ENHANCE_NAME, sub: 'poe.ninja', cell: MISSING },
				{ name: ENLIGHTEN_NAME, sub: 'poe.ninja', cell: MISSING },
				{ name: EMPOWER_NAME, sub: 'poe.ninja', cell: MISSING },
			],
			ev: { kind: 'missing', text: '— needs a feeder price' },
			age: { text: '5m ago', stale: false },
		},
		{
			name: 'no lens price and no feeder price: the lens ask wins; all-null times give no age',
			craft: feederCraft(null, [null, null, null, null], null, null, { lens: null, trade: null, ninja: null }),
			showAll: false,
			costs: [{ name: TRADE_NAME, sub: 'cheapest of 4', cell: MISSING }],
			ev: { kind: 'missing', text: NEEDS_LENS_PRICE },
			age: null,
		},
		{
			name: 'a negative cheapest EV stays negative',
			craft: feederCraft(5, [30, 12, 20, 7], 3, -2),
			showAll: false,
			costs: [{ name: EMPOWER_NAME, sub: 'poe.ninja · cheapest of 4', cell: price('7c') }],
			ev: { kind: 'value', text: '−2c', negative: true },
			age: { text: '18m ago', stale: false },
		},
		{
			name: 'loading, collapsed: one trade-named cheapest-of-4 row',
			craft: undefined,
			showAll: false,
			costs: [{ name: TRADE_NAME, sub: 'cheapest of 4', cell: LOADING }],
			ev: LOADING,
			age: null,
		},
		{
			name: 'loading, expanded: the four fixed names with source-only subs',
			craft: undefined,
			showAll: true,
			costs: [
				{ name: TRADE_NAME, sub: 'trade', cell: LOADING },
				{ name: ENHANCE_NAME, sub: 'poe.ninja', cell: LOADING },
				{ name: ENLIGHTEN_NAME, sub: 'poe.ninja', cell: LOADING },
				{ name: EMPOWER_NAME, sub: 'poe.ninja', cell: LOADING },
			],
			ev: LOADING,
			age: null,
		},
		{
			name: 'no lens key (older server), expanded setting: the one legacy trade row, unpriced',
			craft: null,
			showAll: true,
			costs: [{ name: TRADE_NAME, sub: 'trade', cell: MISSING }],
			ev: { kind: 'missing', text: NEEDS_LENS_PRICE },
			age: null,
		},
		{
			name: 'collapsed on the trade row: an older hidden poe.ninja snapshot does not age the card',
			craft: feederCraft(12400, [5, 12, 20, 7], 0, 12395, { lens: 5 * MIN, trade: 18 * MIN, ninja: 40 * MIN }),
			showAll: false,
			costs: [{ name: TRADE_NAME, sub: 'trade · cheapest of 4', cell: price('5c') }],
			ev: { kind: 'value', text: '+12395c', negative: false },
			age: { text: '18m ago', stale: false },
		},
		{
			name: 'expanded: the older shown poe.ninja snapshot ages the card',
			craft: feederCraft(12400, [5, 12, 20, 7], 0, 12395, { lens: 5 * MIN, trade: 18 * MIN, ninja: 40 * MIN }),
			showAll: true,
			costs: [
				{ name: TRADE_NAME, sub: 'trade · cheapest', cell: price('5c') },
				{ name: ENHANCE_NAME, sub: 'poe.ninja', cell: price('12c') },
				{ name: ENLIGHTEN_NAME, sub: 'poe.ninja', cell: price('20c') },
				{ name: EMPOWER_NAME, sub: 'poe.ninja', cell: price('7c') },
			],
			ev: { kind: 'value', text: '+12395c', negative: false },
			age: { text: '40m ago', stale: false },
		},
		{
			name: 'collapsed on Empower: a 3h-old snapshot reads stale',
			craft: feederCraft(12400, [30, 12, 20, 7], 3, 12393, { lens: 5 * MIN, trade: 18 * MIN, ninja: 3 * HOUR }),
			showAll: false,
			costs: [{ name: EMPOWER_NAME, sub: 'poe.ninja · cheapest of 4', cell: price('7c') }],
			ev: { kind: 'value', text: '+12393c', negative: false },
			age: { text: '3h ago', stale: true },
		},
	])('$name', ({ craft, showAll, costs, ev, age }) => {
		const v = view(craft, showAll);
		expect({ costs: rows(v), ev: v.ev, age: v.age }).toEqual({ costs, ev, age });
	});
});
