import { describe, it, expect, vi, afterEach } from 'vitest';
import { collectiveParams, fetchRankingsPage, type RankingsQuery } from './api';

const base: RankingsQuery = {
	mode: 'normal',
	tab: '20/20',
	color: 'ALL',
	budget: 0,
	pageSize: 20,
	sort: 'price',
	showLowConf: true,
};

describe('collectiveParams', () => {
	it.each([
		['riskAdjusted', 'weightedRoi'],
		['roiPercent', 'roiPct'],
		['roi', 'roi'],
		['price', 'price'],
	] as const)('maps client sort %s to server sort %s', (client, server) => {
		expect(collectiveParams({ ...base, sort: client }).sort).toBe(server);
	});

	it('omits variant for the ALL tab', () => {
		expect(collectiveParams({ ...base, tab: 'ALL' })).toEqual({ pageSize: '20', sort: 'price' });
	});

	it('sends variant and pool for a Dedication tab', () => {
		expect(collectiveParams({ ...base, mode: 'dedication', tab: '21/20', pool: 'transfigured' })).toEqual({
			mode: 'dedication',
			variant: '21/20',
			pool: 'transfigured',
			pageSize: '20',
			sort: 'price',
		});
	});

	it('sends hideLowConfidence=true when the toggle is off', () => {
		expect(collectiveParams({ ...base, showLowConf: false }).hideLowConfidence).toBe('true');
	});

	it('omits hideLowConfidence when the toggle is on', () => {
		expect(collectiveParams({ ...base, showLowConf: true })).not.toHaveProperty('hideLowConfidence');
	});

	it('omits colour for ALL', () => {
		expect(collectiveParams({ ...base, color: 'ALL' })).not.toHaveProperty('color');
	});

	it('sends a picked colour', () => {
		expect(collectiveParams({ ...base, color: 'BLUE' }).color).toBe('BLUE');
	});

	it('omits budget for 0', () => {
		expect(collectiveParams({ ...base, budget: 0 })).not.toHaveProperty('budget');
	});

	it('sends a positive budget', () => {
		expect(collectiveParams({ ...base, budget: 150 }).budget).toBe('150');
	});

	it('sends the row cap as pageSize', () => {
		expect(collectiveParams({ ...base, pageSize: 50 }).pageSize).toBe('50');
	});

	it('for a search sends search and sort but no pageSize-bounded filters', () => {
		const params = collectiveParams({
			...base,
			tab: '1/20',
			color: 'RED',
			budget: 40,
			showLowConf: false,
			sort: 'roiPercent',
			search: 'Arc of Surging',
		});
		expect(params).toEqual({ search: 'Arc of Surging', sort: 'roiPct', pageSize: '20' });
	});
});

describe('fetchRankingsPage', () => {
	const originalFetch = globalThis.fetch;
	afterEach(() => {
		globalThis.fetch = originalFetch;
	});

	function servePage() {
		const fetchMock = vi.fn(async () => ({
			ok: true,
			status: 200,
			statusText: 'OK',
			json: async () => ({
				count: 1,
				total: 137,
				page: 1,
				pageSize: 10,
				data: [{ transfiguredName: 'Arc of Surging', variant: '20', transfiguredPrice: 412.4, gemColor: 'BLUE' }],
			}),
		}));
		globalThis.fetch = fetchMock as unknown as typeof fetch;
		return fetchMock;
	}
	const pageQuery: RankingsQuery = { ...base, tab: '20/0', pageSize: 10, sort: 'riskAdjusted', color: 'BLUE' };

	it('requests the collective endpoint with the mapped params', async () => {
		const fetchMock = servePage();
		await fetchRankingsPage(pageQuery);
		const url = new URL(String((fetchMock.mock.calls[0] as unknown[])[0]));
		expect(url.pathname).toBe('/api/analysis/collective');
		expect(Object.fromEntries(url.searchParams)).toEqual({
			variant: '20/0',
			color: 'BLUE',
			pageSize: '10',
			sort: 'weightedRoi',
		});
	});

	it('returns total from the response', async () => {
		servePage();
		expect((await fetchRankingsPage(pageQuery)).total).toBe(137);
	});

	it('returns the response rows mapped to GemPlay', async () => {
		servePage();
		const result = await fetchRankingsPage(pageQuery);
		expect(result.rows.map((r) => [r.name, r.variant, r.transPrice, r.color])).toEqual([
			['Arc of Surging', '20/0', 412, 'BLUE'],
		]);
	});
});

describe('fetchFontEV lens', () => {
	const originalFetch = globalThis.fetch;
	afterEach(() => {
		globalThis.fetch = originalFetch;
	});

	async function fetchWith(body: Record<string, unknown>) {
		globalThis.fetch = vi.fn(async () => ({
			ok: true,
			status: 200,
			statusText: 'OK',
			json: async () => structuredClone(body),
		})) as unknown as typeof fetch;
		const { fetchFontEV } = await import('./api');
		return fetchFontEV('1/0');
	}

	const TRADE_URL = 'https://www.pathofexile.com/trade/search/Mirage?q=%7B%7D';

	it('maps the server lens object with both quotes, the EV and the feeder trade search', async () => {
		const resp = await fetchWith({
			safe: [],
			lens: {
				outcome: { chaos: 412, listings: 3, fetchedAt: '2026-11-02T11:42:00Z', lowConfidence: false },
				feeder: { chaos: 95, listings: 2, fetchedAt: '2026-11-02T11:40:00Z', lowConfidence: true, tradeUrl: TRADE_URL },
				ev: 317,
			},
		});
		expect(resp.lens).toEqual({
			outcome: { chaos: 412, listings: 3, fetchedAt: '2026-11-02T11:42:00Z', lowConfidence: false },
			feeder: { chaos: 95, listings: 2, fetchedAt: '2026-11-02T11:40:00Z', lowConfidence: true, tradeUrl: TRADE_URL },
			ev: 317,
		});
	});

	it('keeps null prices and a null EV as null, not 0', async () => {
		const resp = await fetchWith({
			lens: {
				outcome: { chaos: null, listings: 0, fetchedAt: null, lowConfidence: false },
				feeder: { chaos: null, listings: 0, fetchedAt: null, lowConfidence: false, tradeUrl: TRADE_URL },
				ev: null,
			},
		});
		expect([resp.lens?.outcome.chaos, resp.lens?.feeder.chaos, resp.lens?.ev]).toEqual([null, null, null]);
		expect(resp.lens?.feeder.tradeUrl).toBe(TRADE_URL);
	});

	it('maps an absent lens key (older server) to null', async () => {
		expect((await fetchWith({ safe: [] })).lens).toBeNull();
	});
});
