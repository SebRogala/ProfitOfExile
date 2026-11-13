import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { CompareGem } from '$lib/api';
import type { TradeLookupResult } from '$lib/tradeApi';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
let stop = () => {};
let applySnapshot: typeof import('$lib/stores/ssot.svelte').applySnapshot;
let ssot: typeof import('$lib/stores/ssot.svelte').ssot;
let createComparatorController: typeof import('./controller.svelte').createComparatorController;
let createLabController: typeof import('./controller.svelte').createLabController;
beforeEach(async () => {
	// Each test starts a WebView module session; remounts within a test retain its provenance fence.
	vi.resetModules();
	({ applySnapshot, ssot } = await import('$lib/stores/ssot.svelte'));
	({ createComparatorController, createLabController } = await import('./controller.svelte'));
});
afterEach(() => { stop(); vi.useRealTimers(); vi.restoreAllMocks(); });
function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (error: Error) => void;
	const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
	return { promise, resolve, reject };
}
function setup(resolveLeague = true) {
	if (resolveLeague) applySnapshot({ league: { name: 'A', generation: ssot.leagueGeneration + 1 } });
	const compares: ReturnType<typeof deferred<CompareGem[]>>[] = [];
	const trades: ReturnType<typeof deferred<TradeLookupResult>>[] = [];
	const overlays: { results: CompareGem[]; tradeData: Record<string, TradeLookupResult | null>; divineRate: number; tradeLoading: Record<string, boolean>; tradeError: Record<string, boolean> }[] = [];
	const queued: string[] = [];
	const names = deferred<string[]>();
	const handlers: Record<string, (event: { payload: unknown }) => void> = {};
	const controller = createComparatorController({
		names: () => names.promise,
		selection: () => ({ variant: '20/20', mode: 'normal', divineRate: 300, autoTrade: false }),
		compare: () => { const d = deferred<CompareGem[]>(); compares.push(d); return d.promise; },
		trade: () => { const d = deferred<TradeLookupResult>(); trades.push(d); return d.promise; },
		listen: async (event, handler) => { handlers[event] = handler as (event: { payload: unknown }) => void; return () => {}; },
		pushOverlay: (payload) => { overlays.push(payload); },
		queue: (name) => queued.push(name)
	});
	stop = controller.start();
	controller.selectGem('Arc');
	return { controller, compares, trades, overlays, queued, names, handlers };
}
const row = (price: number) => ({ name: 'Arc', variant: '20/20', transPrice: price, history: [price] } as unknown as CompareGem);
const trade = (league: string) => ({ league, variant: '20/20', priceFloor: 99 } as TradeLookupResult);

it('withdraws prices and history from the overlay without queueing the outgoing pick', async () => {
	const { controller, compares, overlays, queued } = setup();
	compares[0].resolve([row(42)]);
	await Promise.resolve(); await Promise.resolve();
	expect(controller.state.results).toEqual([row(42)]);
	controller.state.selectedForQueue = 'Arc';
	const pending = controller.loadResults();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	expect([controller.state.results, controller.state.selectedGems, controller.state.tradeQueue]).toEqual([[], [], null]);
	expect(overlays.at(-1)?.results).toEqual([]);
	expect(overlays.at(-1)?.divineRate).toBe(0);
	expect(queued).toEqual([]);
	compares[1].resolve([row(999)]);
	await pending;
	expect(controller.state.results).toEqual([]);
	expect(overlays.at(-1)?.tradeData).toEqual({});
});

it.each(['success', 'error'] as const)('a late outgoing trade %s leaves the replacement spinner running', async (completion) => {
	const { controller, trades } = setup();
	const old = controller.invokeRustTrade('Arc');
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	controller.selectGem('Arc');
	const next = controller.invokeRustTrade('Arc');
	if (completion === 'success') trades[0].resolve(trade('A'));
	else trades[0].reject(new Error('A offline'));
	await old;
	expect([controller.state.tradeData, controller.state.tradeLoading.Arc]).toEqual([{}, true]);
	trades[1].resolve(trade('B'));
	await next;
	expect([controller.state.tradeData.Arc?.priceFloor, controller.state.tradeLoading.Arc]).toEqual([99, false]);
});

it('identical snapshots retain current prices without starting a replacement', async () => {
	const { controller, compares } = setup();
	compares[0].resolve([row(42)]);
	await Promise.resolve(); await Promise.resolve();
	applySnapshot({ league: { name: 'A', generation: ssot.leagueGeneration } });
	expect(controller.state.results).toEqual([row(42)]);
	// A duplicate load would empty these results when it rejects.
	expect(compares.length).toBe(1);
});

it('withdraws embedded trade enrichment stamped for a different league', async () => {
	const { controller, compares, overlays } = setup();
	compares[0].resolve([{ ...row(42), trade: trade('other') }]);
	await Promise.resolve(); await Promise.resolve();
	expect(controller.state.results[0].trade).toBeUndefined();
	expect(overlays.at(-1)?.tradeData).toEqual({});
});

it('rejects an other-league trade returned to the current request', async () => {
	const { controller, trades } = setup();
	const pending = controller.invokeRustTrade('Arc');
	trades[0].resolve(trade('other'));
	await pending;
	expect(controller.state.tradeData).toEqual({});
});

it('disposal prevents a pending compare from repopulating rows', async () => {
	const { controller, compares } = setup();
	stop();
	compares[0].resolve([row(99)]);
	await Promise.resolve(); await Promise.resolve();
	expect(controller.state.results).toEqual([]);
});

it('disposal ignores subsequent native OCR delivery', () => {
	const { controller, handlers } = setup();
	stop();
	handlers['gem-detected']({ payload: 'Fireball' });
	expect(controller.state.selectedGems).toEqual(['Arc']);
});

it('a delayed outgoing search cannot reopen the dropdown', async () => {
	vi.useFakeTimers();
	const { controller, names } = setup();
	controller.handleInput('Fi');
	await vi.advanceTimersByTimeAsync(100);
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	names.resolve(['Fireball']);
	await vi.advanceTimersByTimeAsync(0);
	expect([controller.state.searchQuery, controller.state.suggestions, controller.state.showDropdown]).toEqual(['', [], false]);
});

it('outgoing trade progress cannot restore the cleared queue display', async () => {
	const { controller, handlers, trades } = setup();
	const pending = controller.invokeRustTrade('Arc');
	handlers['trade-queue']({ payload: { kind: 'queued', source: 'gem', gem: 'Arc', position: 1, total: 1 } });
	expect(controller.state.tradeQueue?.gem).toBe('Arc');
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	handlers['trade-queue']({ payload: { kind: 'waiting', source: 'gem', gem: 'Arc', position: 1, total: 1, waitSecs: 20 } });
	expect(controller.state.tradeQueue).toBeNull();
	trades[0].resolve(trade('A'));
	await pending;
});

it('uncorrelated same-gem events cannot contaminate a replacement league lookup', async () => {
	const { controller, handlers, trades, compares, overlays } = setup();
	const old = controller.invokeRustTrade('Arc');
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	controller.selectGem('Arc');
	const replacement = controller.invokeRustTrade('Arc');
	compares[1].resolve([row(55)]);
	await Promise.resolve(); await Promise.resolve();
	const assertPendingB = () => {
		expect([controller.state.tradeQueue, controller.state.tradeError.Arc, controller.state.tradeErrorMsg.Arc]).toEqual([null, false, undefined]);
		expect(overlays.at(-1)).toEqual({
			results: [row(55)], tradeData: {}, tradeLoading: { Arc: true }, tradeError: { Arc: false }, divineRate: 300, labMode: 'normal'
		});
	};
	handlers['trade-queue']({ payload: { kind: 'waiting', source: 'gem', gem: 'Arc', position: 1, total: 1, waitSecs: 20 } });
	assertPendingB();
	handlers['trade-queue']({ payload: { kind: 'error', source: 'gem', gem: 'Arc', error: 'outgoing A failed' } });
	assertPendingB();
	trades[0].resolve(trade('A'));
	await old;
	// A's already-emitted events may be delivered after its promise settles; B remains uncorrelated.
	handlers['trade-queue']({ payload: { kind: 'waiting', source: 'gem', gem: 'Arc', position: 1, total: 1, waitSecs: 19 } });
	handlers['trade-queue']({ payload: { kind: 'error', source: 'gem', gem: 'Arc', error: 'delayed A error' } });
	assertPendingB();
	trades[1].resolve(trade('B'));
	await replacement;
	expect([controller.state.tradeQueue, controller.state.tradeError.Arc, controller.state.tradeErrorMsg.Arc, controller.state.tradeData.Arc?.league]).toEqual([null, false, undefined, 'B']);
	expect(overlays.at(-1)).toEqual({
		results: [row(55)], tradeData: { Arc: trade('B') }, tradeLoading: { Arc: false }, tradeError: { Arc: false }, divineRate: 300, labMode: 'normal'
	});
});

it('settled promises do not restore native event admission after rollover', async () => {
	const { controller, handlers, trades } = setup();
	const old = controller.invokeRustTrade('Arc');
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	controller.selectGem('Arc');
	const replacement = controller.invokeRustTrade('Arc');
	trades[0].resolve(trade('A'));
	trades[1].resolve(trade('B'));
	await Promise.all([old, replacement]);
	const fresh = controller.invokeRustTrade('Arc');
	handlers['trade-queue']({ payload: { kind: 'waiting', source: 'gem', gem: 'Arc', position: 1, total: 1, waitSecs: 4 } });
	expect(controller.state.tradeQueue).toBeNull();
	trades[2].resolve(trade('B'));
	await fresh;
	expect(controller.state.tradeQueue).toBeNull();
});

it.each([
	{ remount: false, gap: false, outcome: 'success' },
	{ remount: false, gap: false, outcome: 'failure' },
	{ remount: true, gap: false, outcome: 'success' },
	{ remount: true, gap: false, outcome: 'failure' },
	{ remount: true, gap: true, outcome: 'success' },
	{ remount: true, gap: true, outcome: 'failure' }
])('settled A events stay fenced from B $outcome with remount=$remount and unmounted rollover=$gap', async ({ remount, gap, outcome }) => {
	const outgoing = setup();
	const old = outgoing.controller.invokeRustTrade('Arc');
	if (gap) {
		outgoing.trades[0].resolve(trade('A'));
		await old;
		stop();
	}
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	if (!gap) {
		outgoing.trades[0].resolve(trade('A'));
		await old;
		if (remount) stop();
	}
	const incoming = remount ? setup(false) : outgoing;
	if (!remount) incoming.controller.selectGem('Arc');
	const replacement = incoming.controller.invokeRustTrade('Arc');
	incoming.compares[remount ? 0 : 1].resolve([row(55)]);
	await Promise.resolve(); await Promise.resolve();
	const pendingOverlay = {
		results: [row(55)], tradeData: {}, tradeLoading: { Arc: true }, tradeError: { Arc: false }, divineRate: 300, labMode: 'normal'
	};
	const assertPendingB = () => {
		expect([incoming.controller.state.tradeQueue, incoming.controller.state.tradeError.Arc, incoming.controller.state.tradeErrorMsg.Arc, incoming.controller.state.tradeLoading.Arc]).toEqual([null, false, undefined, true]);
		expect(incoming.overlays.at(-1)).toEqual(pendingOverlay);
	};
	assertPendingB();
	incoming.handlers['trade-queue']({ payload: { kind: 'waiting', source: 'gem', gem: 'Arc', position: 1, total: 1, waitSecs: 20 } });
	assertPendingB();
	incoming.handlers['trade-queue']({ payload: { kind: 'error', source: 'gem', gem: 'Arc', error: 'delayed settled A error' } });
	assertPendingB();
	const reply = incoming.trades[remount ? 0 : 1];
	if (outcome === 'failure') {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const error = new Error('B request failed');
		reply.reject(error);
		await replacement;
		expect(warn).toHaveBeenCalledWith('[Trade] Rust lookup failed for Arc:', error);
	} else {
		reply.resolve(trade('B'));
		await replacement;
	}
	const failed = outcome === 'failure';
	expect([incoming.controller.state.tradeQueue, incoming.controller.state.tradeError.Arc, incoming.controller.state.tradeErrorMsg.Arc, incoming.controller.state.tradeLoading.Arc]).toEqual([null, failed, failed ? 'B request failed' : undefined, false]);
	const settledOverlay = {
		...pendingOverlay, tradeData: failed ? {} : { Arc: trade('B') }, tradeLoading: { Arc: false }, tradeError: { Arc: failed }
	};
	expect(incoming.overlays.at(-1)).toEqual(settledOverlay);
	incoming.handlers['trade-queue']({ payload: { kind: 'error', source: 'gem', gem: 'Arc', error: 'even later A error' } });
	expect(incoming.controller.state.tradeErrorMsg.Arc).toBe(failed ? 'B request failed' : undefined);
	expect(incoming.overlays.at(-1)).toEqual(settledOverlay);
});

it('same-gem overlap remains uncorrelated across controller disposal', async () => {
	const outgoing = setup();
	const old = outgoing.controller.invokeRustTrade('Arc');
	stop();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	const incoming = setup(false);
	const replacement = incoming.controller.invokeRustTrade('Arc');
	incoming.handlers['trade-queue']({ payload: { kind: 'waiting', source: 'gem', gem: 'Arc', position: 1, total: 1, waitSecs: 20 } });
	incoming.handlers['trade-queue']({ payload: { kind: 'error', source: 'gem', gem: 'Arc', error: 'disposed A error' } });
	expect([incoming.controller.state.tradeQueue, incoming.controller.state.tradeError.Arc, incoming.controller.state.tradeErrorMsg.Arc]).toEqual([null, false, undefined]);
	incoming.trades[0].resolve(trade('B'));
	await replacement;
	outgoing.trades[0].resolve(trade('A'));
	await old;
	expect(incoming.overlays.at(-1)).toMatchObject({ tradeData: { Arc: trade('B') }, tradeLoading: { Arc: false }, tradeError: { Arc: false } });
});

// Lab's Mercure reload: 2000 ms debounce + random() × 4000 ms jitter; random is pinned to 0.5.
const LAB_RELOAD_MS = 4000;
function labSetup() {
	vi.useFakeTimers();
	const reloads: string[] = [];
	const status = vi.fn(async () => ({ lastUpdate: '', nextFetch: '', connected: false, collectorUptime: '', divinePrice: 1 }));
	const lab = createLabController({
		status, rankings: async () => ({ rows: [], total: 0 }), market: async () => ({}) as never,
		connected: () => undefined, onReload: () => reloads.push('reload'), random: () => 0.5
	});
	return { lab, reloads, status };
}
it('lab reloads once after the jittered debounce of a Mercure update', () => {
	const { lab, reloads, status } = labSetup();
	lab.onMercureUpdate();
	vi.advanceTimersByTime(LAB_RELOAD_MS - 1);
	expect(reloads).toEqual([]);
	vi.advanceTimersByTime(1);
	expect(reloads).toEqual(['reload']);
	expect(status).toHaveBeenCalledTimes(1);
});
it('lab collapses a burst of Mercure updates into one reload', () => {
	const { lab, reloads } = labSetup();
	lab.onMercureUpdate();
	vi.advanceTimersByTime(1000);
	lab.onMercureUpdate();
	vi.advanceTimersByTime(1000);
	lab.onMercureUpdate();
	vi.advanceTimersByTime(LAB_RELOAD_MS * 3);
	expect(reloads).toEqual(['reload']);
});
it('lab re-rolls the jitter on every update', () => {
	vi.useFakeTimers();
	const rolls = [0, 1];
	const reloads: number[] = [];
	const lab = createLabController({
		status: async () => ({}) as never, rankings: async () => ({ rows: [], total: 0 }), market: async () => ({}) as never,
		connected: () => undefined, onReload: () => reloads.push(Date.now()), random: () => rolls.shift() ?? 0
	});
	const t0 = Date.now();
	lab.onMercureUpdate();
	vi.advanceTimersByTime(1999);
	lab.onMercureUpdate();
	vi.advanceTimersByTime(6000);
	expect(reloads).toEqual([t0 + 1999 + 6000]);
});
it('lab dispose cancels a pending Mercure reload', () => {
	const { lab, reloads } = labSetup();
	lab.onMercureUpdate();
	lab.dispose();
	vi.advanceTimersByTime(LAB_RELOAD_MS * 2);
	expect(reloads).toEqual([]);
});
it('a league change cancels a pending Mercure reload', () => {
	applySnapshot({ league: { name: 'A', generation: ssot.leagueGeneration + 1 } });
	const { lab, reloads } = labSetup();
	stop = lab.start(() => {});
	lab.onMercureUpdate();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	vi.advanceTimersByTime(LAB_RELOAD_MS * 2);
	expect(reloads).toEqual([]);
});
it('lab setConnected patches a changed connected flag on the loaded status', async () => {
	const { lab } = labSetup();
	await lab.loadAll();
	const before = lab.state.status;
	lab.setConnected(true);
	expect(lab.state.status).toEqual({ ...before, connected: true });
});
it('lab setConnected leaves an equal connected flag unwritten', async () => {
	const { lab } = labSetup();
	await lab.loadAll();
	const before = lab.state.status;
	lab.setConnected(false);
	expect(lab.state.status).toBe(before);
});
it('lab setConnected leaves a null status null', () => {
	const { lab } = labSetup();
	lab.setConnected(true);
	expect(lab.state.status).toBeNull();
});
