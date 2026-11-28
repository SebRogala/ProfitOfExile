import { describe, it, expect, vi, afterEach } from 'vitest';
import { collectiveParams, fetchCompare, fetchRankingsPage, fetchSignalHistory, type RankingsQuery } from './api';

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
	it.each([
		{ name: 'absent', fields: {}, expected: undefined },
		{ name: 'present-empty', fields: { poolBreakdown: [], lowConfidenceGems: [] }, expected: [] }
	])('keep:happy WI-5 absent lists: $name retains its listed-price availability', async ({ fields, expected }) => {
		const row = { color: 'BLUE', pool: 88, pricedGems: 0, ev: null, evRaw: null, ...fields };
		const result = await fetchWith({ safe: [row], premium: [row], jackpot: [row], best: null });
		for (const rows of [result.safe, result.premium, result.jackpot]) {
			expect(rows.map(({ poolBreakdown, lowConfidenceGems }) => ({ poolBreakdown, lowConfidenceGems })))
				.toEqual([{ poolBreakdown: expected, lowConfidenceGems: expected }]);
		}
	});

	it.each([
		{ name: 'null', fields: { ev: null, evRaw: null, profit: null }, expected: null },
		{ name: 'absent', fields: {}, expected: null },
		{ name: 'zero', fields: { ev: 0, evRaw: 0, profit: 0 }, expected: 0 },
		{ name: 'priced', fields: { ev: 12.6, evRaw: 12.6, profit: 12.6 }, expected: 13 }
	])('keep:critical: fetchFontEV keeps a null ev, evRaw and profit null ($name)', async ({ fields, expected }) => {
		// Zero-filling withheld values silently makes WI-5 display a priced 0c cell.
		const row = { color: 'BLUE', pool: 88, ...fields };
		const result = await fetchWith({ safe: [row], premium: [row], jackpot: [row], best: null });
		expect([result.safe, result.premium, result.jackpot].map((rows) =>
			rows.map(({ ev, evRaw, profit }) => ({ ev, evRaw, profit }))
		)).toEqual(Array.from({ length: 3 }, () => [{ ev: expected, evRaw: expected, profit: expected }]));
	});

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

	// keep:regression
	it('maps an older server lens object (no feeders key) to its quotes, EV and feeder trade search', async () => {
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
		// Absent stays absent: an empty or unpriced list would hide the legacy trade/ev branch.
		expect(resp.lens?.feeders).toBeUndefined();
		expect(resp.lens?.outcome.tradeUrl ?? null).toBeNull();
	});

	const LENS_URL = 'https://www.pathofexile.com/trade/search/Mirage?q=lens';
	const ENHANCE_URL = 'https://www.pathofexile.com/trade/search/Mirage?q=enhance';
	const ENLIGHTEN_URL = 'https://www.pathofexile.com/trade/search/Mirage?q=enlighten';
	const EMPOWER_URL = 'https://www.pathofexile.com/trade/search/Mirage?q=empower';
	const TRADE_AT = '2026-11-02T11:40:00Z';
	const NINJA_AT = '2026-11-02T10:00:00Z';

	// keep:happy
	it.each([
		{
			name: 'Empower cheapest with a negative EV',
			outcomeChaos: 5,
			feeders: [
				{ source: 'trade', name: 'Exceptional Support Gem lv 3+', chaos: 30, fetchedAt: TRADE_AT, tradeUrl: TRADE_URL },
				{ source: 'poe.ninja', name: 'Enhance Support', chaos: 12, fetchedAt: NINJA_AT, tradeUrl: ENHANCE_URL },
				{ source: 'poe.ninja', name: 'Enlighten Support', chaos: 20, fetchedAt: NINJA_AT, tradeUrl: ENLIGHTEN_URL },
				{ source: 'poe.ninja', name: 'Empower Support', chaos: 7, fetchedAt: NINJA_AT, tradeUrl: EMPOWER_URL },
			],
			cheapest: 3,
			evCheapest: -2,
		},
		{
			name: 'the trade feeder cheapest with a zero EV',
			outcomeChaos: 95,
			feeders: [
				{ source: 'trade', name: 'Exceptional Support Gem lv 3+', chaos: 95, fetchedAt: TRADE_AT, tradeUrl: TRADE_URL },
				{ source: 'poe.ninja', name: 'Enhance Support', chaos: 120, fetchedAt: NINJA_AT, tradeUrl: ENHANCE_URL },
				{ source: 'poe.ninja', name: 'Enlighten Support', chaos: null, fetchedAt: NINJA_AT, tradeUrl: ENLIGHTEN_URL },
				{ source: 'poe.ninja', name: 'Empower Support', chaos: 140, fetchedAt: NINJA_AT, tradeUrl: EMPOWER_URL },
			],
			cheapest: 0,
			evCheapest: 0,
		},
		{
			name: 'all four feeders unpriced',
			outcomeChaos: null,
			feeders: [
				{ source: 'trade', name: 'Exceptional Support Gem lv 3+', chaos: null, fetchedAt: null, tradeUrl: TRADE_URL },
				{ source: 'poe.ninja', name: 'Enhance Support', chaos: null, fetchedAt: null, tradeUrl: ENHANCE_URL },
				{ source: 'poe.ninja', name: 'Enlighten Support', chaos: null, fetchedAt: null, tradeUrl: ENLIGHTEN_URL },
				{ source: 'poe.ninja', name: 'Empower Support', chaos: null, fetchedAt: null, tradeUrl: EMPOWER_URL },
			],
			cheapest: null,
			evCheapest: null,
		},
	])('maps the four feeders, cheapest, cheapest EV and Lens search for $name', async ({ outcomeChaos, feeders, cheapest, evCheapest }) => {
		const resp = await fetchWith({
			lens: {
				outcome: { chaos: outcomeChaos, listings: 3, fetchedAt: '2026-11-02T11:42:00Z', lowConfidence: false, tradeUrl: LENS_URL },
				feeder: { chaos: feeders[0].chaos, listings: 2, fetchedAt: feeders[0].fetchedAt, lowConfidence: false, tradeUrl: TRADE_URL },
				ev: null,
				feeders,
				cheapest,
				evCheapest,
			},
		});
		expect({
			feeders: resp.lens?.feeders,
			cheapest: resp.lens?.cheapest,
			evCheapest: resp.lens?.evCheapest,
			lensUrl: resp.lens?.outcome.tradeUrl,
		}).toEqual({ feeders, cheapest, evCheapest, lensUrl: LENS_URL });
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

/**
 * The comparator enriches its cards with signal history in one bulk request
 * (POE-410), not one history request per card.
 */
describe('fetchCompare signal history (keep:happy)', () => {
	const originalFetch = globalThis.fetch;
	afterEach(() => {
		globalThis.fetch = originalFetch;
	});

	const ROWS = [
		{ transfiguredName: 'Arc of Surging', variant: '20/20', roi: 10 },
		{ transfiguredName: 'Spark of Nova', variant: '20/20', roi: 20 },
		{ transfiguredName: 'Cold Snap of Power', variant: '1/20', roi: 30 },
	];
	const snap = (time: string, signal: string, currentPrice: number, priceVelocity = 0, currentListings = 5) => ({
		time,
		signal,
		currentPrice,
		priceVelocity,
		currentListings,
	});
	const HISTORY: Record<string, unknown[]> = {
		'Arc of Surging|20/20': [
			snap('2026-11-20T10:00:00Z', 'RISING', 120, 0, 9),
			snap('2026-11-20T09:00:00Z', 'STABLE', 100),
			snap('2026-11-20T08:00:00Z', 'FALLING', 100, 4),
		],
		'Spark of Nova|20/20': [snap('2026-11-20T10:00:00Z', 'HERD', 50), snap('2026-11-20T09:30:00Z', 'STABLE', 50, -3)],
		'Cold Snap of Power|1/20': [snap('2026-11-20T10:00:00Z', 'STABLE', 7)],
	};
	const ok = (body: unknown) => ({ ok: true, status: 200, statusText: 'OK', json: async () => structuredClone(body) });

	function serve(rows: unknown[] = ROWS, bulk?: (keys: string[]) => unknown) {
		const fetchMock = vi.fn(async (input: unknown, _init?: RequestInit) => {
			const url = new URL(String(input));
			if (url.pathname === '/api/analysis/compare') return ok({ count: rows.length, data: rows });
			if (url.pathname === '/api/analysis/history') {
				const key = `${url.searchParams.get('name')}|${url.searchParams.get('variant')}`;
				return ok({ count: (HISTORY[key] ?? []).length, history: HISTORY[key] ?? [] });
			}
			if (url.pathname === '/api/analysis/history/bulk') {
				const keys = url.searchParams.getAll('key');
				if (bulk) return bulk(keys);
				return ok({
					count: keys.length,
					data: keys.map((k) => ({ name: k.split('|')[0], variant: k.split('|')[1], count: (HISTORY[k] ?? []).length, history: HISTORY[k] ?? [] })),
				});
			}
			throw new Error(`unexpected request ${url.pathname}`);
		});
		globalThis.fetch = fetchMock as unknown as typeof fetch;
		return fetchMock;
	}
	const paths = (fetchMock: ReturnType<typeof vi.fn>) => fetchMock.mock.calls.map((c) => new URL(String(c[0])).pathname);

	async function singleRouteHistories() {
		serve();
		return Promise.all(ROWS.map((r) => fetchSignalHistory(r.transfiguredName, r.variant)));
	}

	const bulkUrl = (fetchMock: ReturnType<typeof vi.fn>) =>
		fetchMock.mock.calls.map((c) => new URL(String(c[0]))).find((u) => u.pathname === '/api/analysis/history/bulk');

	it('a three-card compare issues one compare and one bulk history request', async () => {
		const fetchMock = serve();
		await fetchCompare(ROWS.map((r) => r.transfiguredName), '20/20');
		expect(paths(fetchMock)).toEqual(['/api/analysis/compare', '/api/analysis/history/bulk']);
	});

	it('sends each card as a name|variant key in card order', async () => {
		const fetchMock = serve();
		await fetchCompare(ROWS.map((r) => r.transfiguredName), '20/20');
		expect(bulkUrl(fetchMock)?.searchParams.getAll('key')).toEqual(['Arc of Surging|20/20', 'Spark of Nova|20/20', 'Cold Snap of Power|1/20']);
	});

	it("asks for the single route's default depth of 4", async () => {
		const fetchMock = serve();
		await fetchCompare(ROWS.map((r) => r.transfiguredName), '20/20');
		expect(bulkUrl(fetchMock)?.searchParams.get('limit')).toBe('4');
	});

	it('enriches every card with the history the single route derives', async () => {
		const want = await singleRouteHistories();
		expect(want[0]).toHaveLength(2);
		serve();
		const got = await fetchCompare(ROWS.map((r) => r.transfiguredName), '20/20');
		expect(got.map((g) => g.signalHistory)).toEqual(want);
	});

	it('splits more than twelve cards into bulk requests of at most twelve keys', async () => {
		const rows = Array.from({ length: 14 }, (_, i) => ({ transfiguredName: `Gem ${i}`, variant: '20/20' }));
		const fetchMock = serve(rows);
		await fetchCompare(rows.map((r) => r.transfiguredName), '20/20');
		const bulks = fetchMock.mock.calls.map((c) => new URL(String(c[0]))).filter((u) => u.pathname === '/api/analysis/history/bulk');
		expect(bulks.map((u) => u.searchParams.getAll('key').length)).toEqual([12, 2]);
	});

	const failBulk = () => serve(ROWS, () => ({ ok: false, status: 500, statusText: 'Internal Server Error', json: async () => ({}) }));

	it('a failed bulk request keeps the compare rows', async () => {
		failBulk();
		const got = await fetchCompare(ROWS.map((r) => r.transfiguredName), '20/20');
		expect(got.map((g) => [g.name, g.roi])).toEqual([
			['Arc of Surging', 10],
			['Spark of Nova', 20],
			['Cold Snap of Power', 30],
		]);
	});

	it('a failed bulk request maps its cards to []', async () => {
		failBulk();
		const got = await fetchCompare(ROWS.map((r) => r.transfiguredName), '20/20');
		expect(got.map((g) => g.signalHistory)).toEqual([[], [], []]);
	});

	async function batchFailureWarnings() {
		failBulk();
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		await fetchCompare(ROWS.map((r) => r.transfiguredName), '20/20');
		const calls = warn.mock.calls;
		warn.mockRestore();
		return calls;
	}

	it('a failed bulk request warns once', async () => {
		expect(await batchFailureWarnings()).toHaveLength(1);
	});

	it("a failed bulk request's warning names its cards", async () => {
		expect((await batchFailureWarnings())[0][0]).toBe('[Comparator] Signal history failed for Arc of Surging, Spark of Nova, Cold Snap of Power:');
	});

	it("a failed bulk request's warning carries the failure", async () => {
		expect(String((await batchFailureWarnings())[0][1])).toContain('500');
	});

	const serveErrorEntry = () =>
		serve(ROWS, (keys) =>
			ok({
				count: keys.length,
				data: keys.map((k, i) => (i === 0 ? { name: 'Arc of Surging', variant: '20/20', error: 'encode failed' } : { count: 0, history: HISTORY[k] ?? [] })),
			})
		);

	it('an error entry maps only its own card to []', async () => {
		const want = await singleRouteHistories();
		serveErrorEntry();
		const got = await fetchCompare(ROWS.map((r) => r.transfiguredName), '20/20');
		expect(got.map((g) => g.signalHistory)).toEqual([[], want[1], want[2]]);
	});

	it('an error entry warns with its card and error', async () => {
		serveErrorEntry();
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		await fetchCompare(ROWS.map((r) => r.transfiguredName), '20/20');
		const calls = warn.mock.calls;
		warn.mockRestore();
		expect(calls).toEqual([['[Comparator] Signal history failed for Arc of Surging:', 'encode failed']]);
	});
});
