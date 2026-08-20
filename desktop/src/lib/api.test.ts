import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// api.ts pulls in Tauri + the status store at module load. The pure mapping
// tests need neither; the fetcher tests read both back out of the request, so
// the mock carries a recognisable server URL and device id rather than blanks.
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.0.0-test' }));
vi.mock('$lib/stores/status.svelte', () => ({
	store: { status: { server_url: 'https://server.test', device_id: 'device-abc123' } }
}));

const { displayVariant, signalTransitionLabel, fetchCurrencyExchangePlays } = await import('./api');

import type { CurrencyExchangeLeg, CurrencyExchangeResponse } from './api';

/**
 * The variant strings the UI filters on (ByVariant.svelte, FontEVCompare.svelte).
 * Kept literal here on purpose: if the backend format and this list drift apart
 * again, the "1/0" and "20/0" tabs silently render "No data for this variant".
 */
const UI_VARIANTS = ['1/0', '1/20', '20/0', '20/20'];

describe('displayVariant', () => {
	it('restores the /0 suffix on a level-1 zero-quality variant so the 1/0 tab matches', () => {
		// Backend stores/serves this as "1" (internal/lab/transfigure.go).
		expect(displayVariant('1')).toBe('1/0');
		expect(UI_VARIANTS).toContain(displayVariant('1'));
	});

	it('restores the /0 suffix on a level-20 zero-quality variant so the 20/0 tab matches', () => {
		expect(displayVariant('20')).toBe('20/0');
		expect(UI_VARIANTS).toContain(displayVariant('20'));
	});

	it('leaves variants that already carry a quality untouched', () => {
		expect(displayVariant('1/20')).toBe('1/20');
		expect(displayVariant('20/20')).toBe('20/20');
	});

	it('leaves the corrupted Dedication variant untouched', () => {
		// "21/23" must not become "21/23/0" — Dedication rows are filtered by pool,
		// but the variant is still displayed verbatim.
		expect(displayVariant('21/23')).toBe('21/23');
	});

	it('leaves a missing variant empty rather than inventing "/0"', () => {
		expect(displayVariant('')).toBe('');
	});
});

/**
 * The endpoint serves a gem's ring within the server's 14-day retention window,
 * so a gem that stopped trading answers with old transitions. The overlay
 * renders this label verbatim next to a live price, which is what makes a bare
 * time-of-day on a week-old transition a lie rather than a rounding.
 */
describe('signalTransitionLabel', () => {
	const now = new Date(2026, 7, 5, 18, 30);

	it('shows the time of day for a transition from earlier today', () => {
		const label = signalTransitionLabel(new Date(2026, 7, 5, 14, 23).toISOString(), now);
		// Clock reading, whatever the runner's locale does with 12h/24h.
		expect(label).toMatch(/^\d{1,2}[:.]\d{2}/);
		expect(label).not.toContain('ago');
	});

	it('shows the age instead of a time of day for yesterday', () => {
		// 23:50 yesterday is under 19 hours old, and still not today: the label
		// counts calendar days because that is how "1d ago" is read.
		expect(signalTransitionLabel(new Date(2026, 7, 4, 23, 50).toISOString(), now)).toBe('1d ago');
	});

	it('shows the age for a gem that stopped signalling a week ago', () => {
		expect(signalTransitionLabel(new Date(2026, 6, 29, 14, 23).toISOString(), now)).toBe('7d ago');
	});

	it('shows the age at the far edge of the server retention window', () => {
		// signalHistorySeedMaxDays = 14 — the oldest transition that can be served.
		expect(signalTransitionLabel(new Date(2026, 6, 22, 9, 0).toISOString(), now)).toBe('14d ago');
	});

	it('shows the time of day for a timestamp slightly ahead of the clock', () => {
		// Client/server clock skew must not render as a negative age.
		expect(signalTransitionLabel(new Date(2026, 7, 5, 18, 35).toISOString(), now)).not.toContain(
			'ago'
		);
	});

	it('renders nothing for an unparseable timestamp rather than "Invalid Date"', () => {
		expect(signalTransitionLabel('', now)).toBe('');
		expect(signalTransitionLabel('not-a-date', now)).toBe('');
	});
});

/**
 * The Currency Exchange page keeps whatever it last rendered when a fetch
 * fails, so the two things this fetcher owes it are an exactly-shaped request
 * (an unknown or missing mode is a 400, not a fallback) and a rejection it can
 * turn into the "stale" header rather than a blank table.
 */
describe('fetchCurrencyExchangePlays', () => {
	const PLAYS_RESPONSE: CurrencyExchangeResponse = {
		league: 'Mirage',
		lastUpdated: '2026-08-19T12:00:00.000Z',
		from: null,
		to: null,
		hours: 24,
		warm: true,
		mode: 'direct',
		horizon: 'recent',
		divineChaosRate: 198.97,
		count: 0,
		plays: [],
		// The sidebar's sixteen, in sidebar order, on a body with no plays at
		// all — that is the shape the server guarantees
		// (internal/exchange/items.go categories).
		categories: [
			'Currency',
			'Essences',
			'Delve',
			'Scarabs',
			'Divination Cards',
			'Delirium',
			'Legion',
			'Fragments',
			'Oils',
			'Catalysts',
			'Omens',
			'Tattoos',
			'Expedition',
			'Harvest',
			'Runegrafts',
			'Allflame'
		]
	};

	/**
	 * A play as the decorated handler sends it (POE-177): every leg carries a
	 * display name, a sidebar category on each side, and an icon path only when
	 * the item has artwork. Both shapes appear in the same payload because the
	 * page has to render a row that mixes them.
	 */
	const DECORATED_LEG: CurrencyExchangeLeg = {
		action: 'buy',
		item: 'Metadata/Items/Currency/CurrencyRerollRare',
		quote: 'Metadata/Items/Currency/CurrencyAddModToRare',
		price: 0.004975,
		fair: 0.00512,
		fairOk: true,
		tick: 0.02,
		volume: 1200,
		stock: 40,
		suspect: false,
		itemName: 'Chaos Orb',
		itemIcon: '/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare',
		itemCategory: 'Currency',
		quoteName: 'Exalted Orb',
		quoteIcon: '/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyAddModToRare',
		quoteCategory: 'Currency'
	};

	const ICONLESS_LEG: CurrencyExchangeLeg = {
		action: 'sell',
		item: 'Metadata/Items/Currency/CurrencyAfflictionOrbGeneric',
		quote: 'Metadata/Items/Currency/CurrencyRerollRare',
		// A sell 75% over the hour's volume-weighted price is past the server's
		// 1.5x band, so this leg carries the junk flag — the payload the page has
		// to render mixes a clean leg with a suspect one, as a real one does.
		price: 3.5,
		fair: 2,
		fairOk: true,
		tick: 0.05,
		volume: 90,
		stock: 12,
		suspect: true,
		itemName: 'Delirium Orb',
		itemIcon: null,
		// The two sides sit in different sidebar categories, as a real leg's
		// usually do — the filter has to be able to match on either one.
		itemCategory: 'Delirium',
		quoteName: 'Chaos Orb',
		quoteIcon: '/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare',
		quoteCategory: 'Currency'
	};

	/**
	 * A leg whose item is an id the server's asset does not cover — a currency
	 * added since the last asset regeneration. The server humanises the id into
	 * a name, has no icon URL for it, and sends `""` for the category, which the
	 * filter reads as unfiltered.
	 */
	const UNCATEGORISED_LEG: CurrencyExchangeLeg = {
		action: 'buy',
		item: 'Metadata/Items/Currency/CurrencyNewLeagueOrb',
		quote: 'Metadata/Items/Currency/CurrencyRerollRare',
		price: 12,
		fair: 11.5,
		fairOk: true,
		tick: 0.01,
		volume: 300,
		stock: 25,
		suspect: false,
		itemName: 'New League Orb',
		itemIcon: null,
		itemCategory: '',
		quoteName: 'Chaos Orb',
		quoteIcon: '/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare',
		quoteCategory: 'Currency'
	};

	const DECORATED_RESPONSE: CurrencyExchangeResponse = {
		...PLAYS_RESPONSE,
		count: 1,
		plays: [
			{
				key: 'chaos:exalted',
				mode: 'direct',
				legs: [DECORATED_LEG, ICONLESS_LEG],
				roiPct: 0.12,
				edge: 0.12,
				// The raw extremes read higher than the undercut prices by what
				// the legs' two ticks cost to fill; roiPctRaw is never below roiPct.
				roiPctRaw: 0.2025,
				// roi === roiPct * investment, and the play's tick is the
				// coarsest of its legs' — the shapes the server guarantees.
				roi: 21.6,
				investment: 180,
				turnover: 74000,
				tick: 0.05,
				depth: 90,
				// Any suspect leg makes the play suspect; it is still served.
				suspect: true,
				hoursSeen: 20,
				lastHour: '2026-08-19T12:00:00.000Z'
			}
		]
	};

	let originalFetch: typeof globalThis.fetch;
	let fetchMock: ReturnType<typeof vi.fn>;

	/** The URL and init of the single request the fetcher made. */
	function request(): { url: URL; init: RequestInit } {
		expect(fetchMock).toHaveBeenCalledTimes(1);
		const [url, init] = fetchMock.mock.calls[0];
		return { url: new URL(url as string), init: (init ?? {}) as RequestInit };
	}

	beforeEach(() => {
		originalFetch = globalThis.fetch;
		// `json()` hands back a fresh parse on every real request, so the mock
		// clones too — sharing the fixture object would alias `expect`'s
		// argument with the fetcher's own return value and hide any in-place
		// rewrite of the body.
		fetchMock = vi.fn(async () => ({ ok: true, json: async () => structuredClone(PLAYS_RESPONSE) }));
		globalThis.fetch = fetchMock as unknown as typeof fetch;
	});

	afterEach(() => {
		globalThis.fetch = originalFetch;
	});

	it('requests the plays endpoint under the configured server API base', async () => {
		await fetchCurrencyExchangePlays('direct');

		const { url } = request();
		expect(url.origin + url.pathname).toBe('https://server.test/api/currency-exchange/plays');
	});

	it('sends the selected mode as the mode query parameter', async () => {
		await fetchCurrencyExchangePlays('direct');

		expect(request().url.searchParams.get('mode')).toBe('direct');
	});

	it('sends "all" as an explicit mode rather than omitting the parameter', async () => {
		// The server reads a missing mode as its own default, and the page's
		// picker has to be able to say "all" back to a server whose default is not.
		await fetchCurrencyExchangePlays('all');

		expect(request().url.searchParams.get('mode')).toBe('all');
	});

	it('sends "recent" as an explicit horizon when the caller names none', async () => {
		// The page (POE-184) still calls with a mode only. Riding the server's
		// own default would make the window the page renders change under it the
		// day that default moves, and the response's `horizon` echo would then be
		// the first place anyone noticed.
		await fetchCurrencyExchangePlays('all');

		expect(request().url.searchParams.get('horizon')).toBe('recent');
	});

	it('sends the requested horizon when the caller asks for the day window', async () => {
		await fetchCurrencyExchangePlays('all', 'day');

		expect(request().url.searchParams.get('horizon')).toBe('day');
	});

	it('identifies the device and app version on the request', async () => {
		// The server attributes requests per device (POE-102); a fetcher that
		// reaches the network without these headers is an anonymous client.
		await fetchCurrencyExchangePlays('all');

		expect(request().init.headers).toEqual({
			'X-Device-ID': 'device-abc123',
			'X-App-Version': '0.0.0-test'
		});
	});

	it('returns the parsed body on a successful request', async () => {
		expect(await fetchCurrencyExchangePlays('direct')).toEqual(PLAYS_RESPONSE);
	});

	it('hands the page a leg with its display names and icon paths exactly as the server sent them', async () => {
		// The page joins `itemIcon` onto the API base itself (view.ts `iconSrc`),
		// so the path has to arrive server-relative and with its `%2F` escaping
		// intact — a fetcher that normalised, re-encoded or re-keyed the leg
		// would send every chip to a 404.
		fetchMock.mockResolvedValue({ ok: true, json: async () => structuredClone(DECORATED_RESPONSE) });

		const result = await fetchCurrencyExchangePlays('direct');

		expect(result.plays[0].legs[0]).toEqual(DECORATED_LEG);
	});

	it('keeps a null icon as null rather than dropping the field', async () => {
		// "no artwork" and "field absent" render differently: ItemIcon draws
		// nothing for the first, and `undefined` would reach `iconSrc` as a
		// missing prop instead. A few live ids have no icon (Delirium Orb), so
		// this is the shape of a real payload, not a defensive case.
		fetchMock.mockResolvedValue({ ok: true, json: async () => structuredClone(DECORATED_RESPONSE) });

		const leg = (await fetchCurrencyExchangePlays('direct')).plays[0].legs[1];

		expect(leg.itemIcon).toBeNull();
	});

	it('keeps each side of a leg in its own category field', async () => {
		// The filter matches on whichever side the reader is shopping for, so
		// item and quote categories must not be crossed or collapsed into one:
		// this leg buys a Delirium Orb quoted in chaos, and hiding "Currency"
		// has to leave the Delirium side of it still matchable.
		fetchMock.mockResolvedValue({ ok: true, json: async () => structuredClone(DECORATED_RESPONSE) });

		const leg = (await fetchCurrencyExchangePlays('direct')).plays[0].legs[1];

		expect(leg.itemCategory).toBe('Delirium');
		expect(leg.quoteCategory).toBe('Currency');
	});

	it('keeps an uncovered item\'s empty category as "" rather than substituting a name', async () => {
		// "" is the server's answer for an id its asset does not know, and the
		// filter reads it as unfiltered. A fetcher that defaulted it to a
		// placeholder category would file the item under a row it does not
		// belong to, and one that dropped the falsy field would hand the filter
		// `undefined` instead.
		fetchMock.mockResolvedValue({
			ok: true,
			json: async () =>
				structuredClone({
					...DECORATED_RESPONSE,
					plays: [{ ...DECORATED_RESPONSE.plays[0], legs: [UNCATEGORISED_LEG, ICONLESS_LEG] }]
				})
		});

		const leg = (await fetchCurrencyExchangePlays('direct')).plays[0].legs[0];

		expect(leg.itemCategory).toBe('');
	});

	it('carries the whole sidebar taxonomy in sidebar order on a body with no plays', async () => {
		// The category filter renders from this list, so it cannot be a function
		// of the ranking: a taxonomy derived from the plays in the body would
		// leave the filter empty on a cold server, and one re-sorted on the way
		// through would stop matching the in-game sidebar the reader is reading
		// alongside it.
		const result = await fetchCurrencyExchangePlays('all');

		expect(result.categories).toEqual([
			'Currency',
			'Essences',
			'Delve',
			'Scarabs',
			'Divination Cards',
			'Delirium',
			'Legion',
			'Fragments',
			'Oils',
			'Catalysts',
			'Omens',
			'Tattoos',
			'Expedition',
			'Harvest',
			'Runegrafts',
			'Allflame'
		]);
	});

	it('rejects with the status when the server answers a non-OK response', async () => {
		fetchMock.mockResolvedValue({ ok: false, status: 500, statusText: 'Internal Server Error' });

		await expect(fetchCurrencyExchangePlays('all')).rejects.toThrow(
			'API /currency-exchange/plays: 500 Internal Server Error'
		);
	});

	it('rejects rather than resolving an empty result when the request never lands', async () => {
		// The page tells "server unreachable" from "no plays pass the filters"
		// purely by whether this promise rejected.
		fetchMock.mockRejectedValue(new TypeError('Failed to fetch'));

		await expect(fetchCurrencyExchangePlays('all')).rejects.toThrow('Failed to fetch');
	});
});
