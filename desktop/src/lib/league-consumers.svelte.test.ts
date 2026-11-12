import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { applySnapshot, ssot } from '$lib/stores/ssot.svelte';
import { createComparatorController, createLabQueueController, createLabController } from '$lib/comparator/controller.svelte';
import { createExchangeController } from '$lib/exchange/controller.svelte';
import { createHarvestController } from '$lib/harvest/controller.svelte';
import { createExchangeFixture } from '$lib/harvest/sources/exchange-fixture';
import { loadHarvestFamilies } from '$lib/harvest/sources/harvest-fixture';
import { currentDivineRate, initDivineRateStore, resetDivineRate, refreshDivineRate } from '$lib/stores/divine-rate.svelte';
import type { CompareGem, CurrencyExchangeResponse, DivineRate, GemPlay, RankingsQuery, StatusData, MarketOverviewData } from '$lib/api';
import type { TradeLookupResult } from '$lib/tradeApi';
import type { QueueItem } from '$lib/session-queue';
import type { ExchangePriceRead } from '$lib/harvest/seam';

const mocks = vi.hoisted(() => ({ rate: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

const stops: (() => void)[] = [];
beforeEach(() => {
	vi.useFakeTimers();
	mocks.rate.mockReset();
	vi.stubGlobal('fetch', vi.fn(() => {
		const reply = mocks.rate() as Promise<DivineRate>;
		return Promise.resolve({ ok: true, json: async () => ({ ...await reply, warm: true }) });
	}));
	resetDivineRate();
	applySnapshot({ league: { name: 'A', generation: ssot.leagueGeneration + 1 } });
});
afterEach(() => { for (const stop of stops.splice(0)) stop(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (error: Error) => void;
	const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
	return { promise, resolve, reject };
}
const settle = () => vi.advanceTimersByTimeAsync(0);
const rate = (value: number) => ({ divineChaosRate: value, state: 'ready', updatedAt: '2026-09-10T00:00:00Z', priceHour: '2026-09-10T00:00:00Z', unreachableSince: null } as DivineRate);
const queueItem = () => ({ gem: 'Arc', variant: '20/20', pickedAt: new Date(), snapshotROI: 10, snapshot: null, snapshotDivineRate: 0 } as QueueItem);
function dashboard() {
	return createLabController({
		status: async () => ({ divinePrice: 0 } as StatusData), rankings: async () => ({ rows: [], total: 0 }),
		market: async () => ({} as MarketOverviewData), rate: () => {}, connected: () => undefined
	});
}

it('an A queue refresh cannot patch a replacement B pick with the same key', async () => {
	const pending = deferred<TradeLookupResult>();
	const queue = { items: [queueItem()] };
	const controller = createLabQueueController({ queue, lookup: () => pending.promise });
	stops.push(dashboard().start(controller.clear));
	const refresh = controller.refresh();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	expect(queue.items).toEqual([]);
	queue.items = [queueItem()];
	pending.resolve({ league: 'A', variant: '20/20', priceFloor: 999 } as TradeLookupResult);
	await refresh;
	expect(queue.items).toEqual([queueItem()]);
});

it('a disposed queue refresh cannot patch a retained pick', async () => {
	const pending = deferred<TradeLookupResult>();
	const queue = { items: [queueItem()] };
	const controller = createLabQueueController({ queue, lookup: () => pending.promise });
	const stopDashboard = dashboard().start(controller.clear);
	const refresh = controller.refresh();
	stopDashboard();
	controller.clear();
	queue.items = [queueItem()];
	pending.resolve({ league: 'A', priceFloor: 999 } as TradeLookupResult);
	await refresh;
	expect(queue.items[0].current).toBeUndefined();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	expect(queue.items.map((item) => item.gem)).toEqual(['Arc']);
});

type RankingsPage = { rows: GemPlay[]; total: number };
const rankingsQuery = (tab: string): RankingsQuery => ({ mode: 'normal', tab, color: 'ALL', budget: 0, pageSize: 20, sort: 'price', showLowConf: true });
const page = (...names: string[]): RankingsPage => ({ rows: names.map((name) => ({ name }) as GemPlay), total: names.length });

it('a rankings response from the previous league is dropped', async () => {
	const ranksA = deferred<RankingsPage>();
	const ranksB = deferred<RankingsPage>();
	const statusA = deferred<StatusData>();
	const statusB = deferred<StatusData>();
	const readings: number[] = [];
	let rankCall = 0;
	let statusCall = 0;
	const controller = createLabController({
		status: () => [statusA, statusB][statusCall++].promise,
		rankings: () => [ranksA, ranksB][rankCall++].promise,
		market: async () => ({} as MarketOverviewData), rate: (value) => readings.push(value), connected: () => undefined
	});
	stops.push(controller.start(() => {}));
	controller.setRankingsQuery(rankingsQuery('20/20'));
	const old = controller.loadAll();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	expect([controller.state.bestPlays, controller.state.status, controller.state.loading, readings]).toEqual([[], null, true, [0]]);
	statusB.reject(new Error('B offline'));
	ranksB.resolve(page());
	await settle();
	statusA.resolve({ divinePrice: 999 } as StatusData);
	ranksA.resolve(page('outgoing'));
	await old;
	expect([controller.state.bestPlays, controller.state.status, controller.state.marketOverview, controller.state.loading, controller.state.error, readings]).toEqual([[], null, null, false, 'B offline', [0]]);
});

it('rankings refetch after a league change uses the stored query', async () => {
	const asked: RankingsQuery[] = [];
	const controller = createLabController({
		status: async () => ({ divinePrice: 0 } as StatusData),
		rankings: async (q) => { asked.push(q); return page('row'); },
		market: async () => ({} as MarketOverviewData), rate: () => {}, connected: () => undefined
	});
	stops.push(controller.start(() => {}));
	const stored = { ...rankingsQuery('1/20'), color: 'BLUE', budget: 40, sort: 'roiPercent' as const };
	controller.setRankingsQuery(stored);
	await settle();
	asked.length = 0;
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	await settle();
	expect(asked).toEqual([stored]);
});

it('a rankings response from a superseded query is dropped', async () => {
	const pages = { '20/20': deferred<RankingsPage>(), '1/0': deferred<RankingsPage>() };
	const controller = createLabController({
		status: async () => ({ divinePrice: 0 } as StatusData),
		rankings: (q) => pages[q.tab as keyof typeof pages].promise,
		market: async () => ({} as MarketOverviewData), rate: () => {}, connected: () => undefined
	});
	controller.setRankingsQuery(rankingsQuery('20/20'));
	controller.setRankingsQuery(rankingsQuery('1/0'));
	pages['1/0'].resolve(page('current'));
	await settle();
	pages['20/20'].resolve(page('superseded'));
	await settle();
	expect(controller.state.bestPlays.map((g) => g.name)).toEqual(['current']);
});

it('a failed rankings page sets bestPlaysError and clears rows', async () => {
	let fail = false;
	const controller = createLabController({
		status: async () => ({ divinePrice: 0 } as StatusData),
		rankings: async () => { if (fail) throw new Error('offline'); return page('kept?'); },
		market: async () => ({} as MarketOverviewData), rate: () => {}, connected: () => undefined
	});
	vi.spyOn(console, 'warn').mockImplementation(() => {});
	controller.setRankingsQuery(rankingsQuery('20/20'));
	await settle();
	fail = true;
	await controller.refreshBestPlays();
	expect([controller.state.bestPlays, controller.state.bestPlaysError]).toEqual([[], 'Rankings failed to load for this market — retry to see current numbers.']);
});

it('a new rankings query clears the previous query rows until its page arrives', async () => {
	const pages = { '20/20': deferred<RankingsPage>(), '1/0': deferred<RankingsPage>() };
	const controller = createLabController({
		status: async () => ({ divinePrice: 0 } as StatusData),
		rankings: (q) => pages[q.tab as keyof typeof pages].promise,
		market: async () => ({} as MarketOverviewData), rate: () => {}, connected: () => undefined
	});
	controller.setRankingsQuery(rankingsQuery('20/20'));
	pages['20/20'].resolve(page('old market'));
	await settle();
	controller.setRankingsQuery(rankingsQuery('1/0'));
	expect(controller.state.bestPlays).toEqual([]);
});

it('a rankings page stores its total', async () => {
	const controller = createLabController({
		status: async () => ({ divinePrice: 0 } as StatusData),
		rankings: async () => ({ rows: [{ name: 'a' } as GemPlay], total: 137 }),
		market: async () => ({} as MarketOverviewData), rate: () => {}, connected: () => undefined
	});
	controller.setRankingsQuery(rankingsQuery('20/20'));
	await settle();
	expect(controller.state.rankingsTotal).toBe(137);
});

it.each(['failed', 'cold'] as const)('withdraws every outgoing consumer while B is %s', async (replacement) => {
	const compareA = deferred<CompareGem[]>();
	const cxCalls: ReturnType<typeof deferred<CurrencyExchangeResponse>>[] = [];
	const harvestCalls: ReturnType<typeof deferred<ExchangePriceRead>>[] = [];
	const overlay: { results: CompareGem[] }[] = [];
	const queued: string[] = [];
	const comparator = createComparatorController({
		names: async () => [],
		selection: () => ({ variant: '20/20', mode: 'normal', divineRate: currentDivineRate() ?? 0, autoTrade: false }),
		compare: () => compareA.promise,
		trade: async () => { throw new Error('unused'); }, listen: async () => () => {},
		pushOverlay: (value) => overlay.push(value), queue: (name) => queued.push(name)
	});
	stops.push(comparator.start());
	comparator.selectGem('Arc');
	compareA.resolve([{ name: 'Arc', variant: '20/20', transPrice: 42, history: [42] } as unknown as CompareGem]);
	const exchange = createExchangeController({
		selection: () => ({ mode: 'all', horizon: 'day' }), listen: async () => () => {},
		load: () => { const d = deferred<CurrencyExchangeResponse>(); cxCalls.push(d); return d.promise; }
	});
	stops.push(exchange.start());
	cxCalls[0].resolve({ league: 'A', plays: [{ id: 'A-row' }], divineChaosRate: 300 } as unknown as CurrencyExchangeResponse);
	const prefs = { picks: { value: '{}' }, horizon: { value: 'day' }, family: { value: 'fossil' } };
	const harvest = createHarvestController({
		prefs, loadHarvestFamilies,
		loadExchangePrices: () => { const d = deferred<ExchangePriceRead>(); harvestCalls.push(d); return d.promise; },
		listen: async () => () => {}, refetchDelay: () => 2000,
		setTimeout, clearTimeout, setInterval, clearInterval,
		writeClipboard: async () => {}, now: () => new Date('2026-09-10T00:00:00Z'), divineRate: currentDivineRate
	});
	stops.push(harvest.start());
	const prices = await createExchangeFixture(() => new Date('2026-09-10T00:00:00Z'))('day');
	harvestCalls[0].resolve({ ...prices, league: 'A' });
	mocks.rate.mockResolvedValueOnce(rate(300));
	stops.push(initDivineRateStore());
	await settle();
	await harvest.copyRegex('keepers');
	expect([comparator.state.results[0].transPrice, exchange.result?.divineChaosRate, harvest.copied.keepers, currentDivineRate()]).toEqual([42, 300, true, 300]);
	harvest.togglePick('Metadata/Items/Currency/CurrencyDelveCraftingDefences');
	const saved = prefs.picks.value;
	const rateA = deferred<DivineRate>();
	mocks.rate.mockReturnValueOnce(rateA.promise);
	const pendingRate = refreshDivineRate();
	const pendingCx = exchange.load();
	const pendingHarvest = harvest.load();
	const rateB = deferred<DivineRate>();
	mocks.rate.mockReturnValueOnce(rateB.promise);
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	expect([comparator.state.results, exchange.result, harvest.exchange, harvest.harvest, currentDivineRate()]).toEqual([[], null, null, null, null]);
	expect(overlay.at(-1)?.results).toEqual([]);
	expect(queued).toEqual([]);
	const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
	if (replacement === 'failed') {
		cxCalls[2].reject(new Error('B offline'));
		harvestCalls[2].reject(new Error('B offline'));
		rateB.reject(new Error('B offline'));
	} else {
		cxCalls[2].resolve({ league: 'B', plays: [], divineChaosRate: 0, warm: false } as unknown as CurrencyExchangeResponse);
		harvestCalls[2].resolve({ ...prices, league: 'B', prices: {}, warm: false, divineChaosRate: 0, priceHour: null });
		rateB.resolve({ ...rate(0), state: 'cold' });
	}
	await settle();
	cxCalls[1].resolve({ league: 'A', plays: [{ id: 'late-A' }] } as unknown as CurrencyExchangeResponse);
	harvestCalls[1].resolve({ ...prices, league: 'A' });
	rateA.resolve(rate(999));
	await Promise.all([pendingRate, pendingCx, pendingHarvest]);
	expect([comparator.state.results, exchange.result?.plays ?? [], harvest.exchange?.prices ?? {}, currentDivineRate()]).toEqual([[], [], {}, null]);
	expect(harvest.copied).toEqual({ feeders: false, keepers: false });
	expect(prefs).toEqual({ picks: { value: saved }, horizon: { value: 'day' }, family: { value: 'fossil' } });
	if (replacement === 'failed') expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', expect.any(Error));
});
