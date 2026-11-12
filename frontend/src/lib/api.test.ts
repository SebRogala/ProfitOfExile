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
	it('maps riskAdjusted to weightedRoi and roiPercent to roiPct', () => {
		expect(collectiveParams({ ...base, sort: 'riskAdjusted' }).sort).toBe('weightedRoi');
		expect(collectiveParams({ ...base, sort: 'roiPercent' }).sort).toBe('roiPct');
		expect(collectiveParams({ ...base, sort: 'roi' }).sort).toBe('roi');
		expect(collectiveParams({ ...base, sort: 'price' }).sort).toBe('price');
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

	it('sends hideLowConfidence only when the toggle is off', () => {
		expect(collectiveParams({ ...base, showLowConf: false }).hideLowConfidence).toBe('true');
		expect(collectiveParams({ ...base, showLowConf: true })).not.toHaveProperty('hideLowConfidence');
	});

	it('omits colour for ALL and budget for 0', () => {
		expect(collectiveParams({ ...base, color: 'ALL', budget: 0 })).toEqual({
			variant: '20/20',
			pageSize: '20',
			sort: 'price',
		});
		expect(collectiveParams({ ...base, color: 'BLUE', budget: 150, pageSize: 50 })).toEqual({
			variant: '20/20',
			color: 'BLUE',
			budget: '150',
			pageSize: '50',
			sort: 'price',
		});
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

	it('returns total from the response', async () => {
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

		const result = await fetchRankingsPage({ ...base, tab: '20/0', pageSize: 10, sort: 'riskAdjusted', color: 'BLUE' });

		const url = new URL(String((fetchMock.mock.calls[0] as unknown[])[0]));
		expect(url.pathname).toBe('/api/analysis/collective');
		expect(Object.fromEntries(url.searchParams)).toEqual({
			variant: '20/0',
			color: 'BLUE',
			pageSize: '10',
			sort: 'weightedRoi',
		});
		expect(result.total).toBe(137);
		expect(result.rows.map((r) => [r.name, r.variant, r.transPrice, r.color])).toEqual([
			['Arc of Surging', '20/0', 412, 'BLUE'],
		]);
	});
});
