import { afterEach, expect, it, vi } from 'vitest';
import { applySnapshot, ssot } from '$lib/stores/ssot.svelte';
import type { CurrencyExchangeResponse } from '$lib/api';
import { createExchangeController, type ExchangeQuery } from './controller.svelte';
import { deriveState } from './view';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

let stop = () => {};
afterEach(() => {
	stop();
	vi.useRealTimers();
});

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (error: Error) => void;
	const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
	return { promise, resolve, reject };
}

function setup() {
	applySnapshot({ league: { name: 'A', generation: ssot.leagueGeneration + 1 } });
	const calls: ReturnType<typeof deferred<CurrencyExchangeResponse>>[] = [];
	const requests: unknown[][] = [];
	const query: ExchangeQuery = { mode: 'all', horizon: 'day', params: { pageSize: '50', sort: 'expected' } };
	const controller = createExchangeController({
		load: (...args) => {
			requests.push(args);
			const d = deferred<CurrencyExchangeResponse>();
			calls.push(d);
			return d.promise;
		},
		selection: () => query,
		listen: async () => () => {}
	});
	stop = controller.start();
	return { controller, calls, query, requests };
}

const response = (league: string, plays: unknown[] = []) => ({ league, plays, divineChaosRate: 300 } as CurrencyExchangeResponse);
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };

it('sends the stored page query with its mode and horizon', () => {
	const { requests } = setup();
	expect(requests).toEqual([['all', 'day', { pageSize: '50', sort: 'expected' }]]);
});

it('withdraws rows synchronously through a failed B read', async () => {
	const { controller, calls, query } = setup();
	calls[0].resolve(response('A', [{ id: 'A-price' }]));
	await settle();
	expect(controller.result?.plays).toEqual([{ id: 'A-price' }]);
	void controller.load();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	expect([controller.result, controller.lastFetchedAt, controller.loading]).toEqual([null, null, true]);
	calls[2].reject(new Error('B offline'));
	await settle();
	calls[1].resolve(response('A', [{ id: 'late-A' }]));
	await settle();
	expect([controller.result, controller.lastError, controller.loading]).toEqual([null, 'B offline', false]);
	expect(query.params).toEqual({ pageSize: '50', sort: 'expected' });
});

it('rejects a mismatched payload in the current generation', async () => {
	const { controller, calls } = setup();
	calls[0].resolve(response('other', [{ id: 'wrong' }]));
	await settle();
	expect([controller.result, controller.loading]).toEqual([null, false]);
});

it('admits the initial server snapshot before SSOT first resolves', async () => {
	const { controller, calls } = setup();
	ssot.league = null;
	calls[0].resolve(response('boot', [{ id: 'initial' }]));
	await settle();
	expect([controller.result?.plays, controller.loading]).toEqual([[{ id: 'initial' }], false]);
});

it('a league switch drops the previous league\'s page', async () => {
	const { controller, calls } = setup();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	calls[0].resolve(response('A', [{ id: 'late-A' }]));
	await settle();
	expect([controller.result, controller.loading]).toEqual([null, true]);
	calls[1].resolve(response('B'));
	await settle();
	expect([controller.result?.plays, controller.loading]).toEqual([[], false]);
});

it('preserves a same-league answer through identical snapshots', async () => {
	const { controller, calls } = setup();
	calls[0].resolve(response('A', [{ id: 'kept' }]));
	await settle();
	applySnapshot({ league: { name: 'A', generation: ssot.leagueGeneration } });
	expect(controller.result?.plays).toEqual([{ id: 'kept' }]);
	expect(controller.loading).toBe(false);
});

it('disposal blocks in-flight answers and later league delivery', async () => {
	const { controller, calls } = setup();
	stop();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	calls[0].resolve(response('A', [{ id: 'disposed' }]));
	await settle();
	expect(controller.result).toBeNull();
});

it('a superseded page query response is dropped', async () => {
	const { controller, calls, query } = setup();
	query.params = { pageSize: 'all', sort: 'expected' };
	controller.syncSelection();
	calls[0].resolve(response('A', [{ id: 'old-query' }]));
	await settle();
	expect([controller.result, controller.loading]).toEqual([null, true]);
	calls[1].resolve(response('A', [{ id: 'new-query' }]));
	await settle();
	expect(controller.result?.plays).toEqual([{ id: 'new-query' }]);
});

it('a query change withdraws the previous query\'s rows at once', async () => {
	// Rows are only ever shown under the query they were fetched for.
	const { controller, calls, query } = setup();
	calls[0].resolve(response('A', [{ id: 'capped' }]));
	await settle();
	query.params = { pageSize: '25', sort: 'roi' };
	controller.syncSelection();
	expect([controller.result, controller.loading]).toEqual([null, true]);
});

it('an unchanged query issues no request', async () => {
	const { controller, requests } = setup();
	controller.syncSelection();
	controller.syncSelection({ debounce: true });
	expect(requests).toHaveLength(1);
});

it('a typed query change waits 300 ms before it fetches', async () => {
	vi.useFakeTimers();
	const { controller, query, requests } = setup();
	query.params = { pageSize: '50', sort: 'expected', search: 'Om' };
	controller.syncSelection({ debounce: true });
	vi.advanceTimersByTime(299);
	expect(requests).toHaveLength(1);
	vi.advanceTimersByTime(1);
	expect(requests).toEqual([
		['all', 'day', { pageSize: '50', sort: 'expected' }],
		['all', 'day', { pageSize: '50', sort: 'expected', search: 'Om' }]
	]);
});

it('a failed page fetch keeps the previous page and marks it stale', async () => {
	const { controller, calls } = setup();
	calls[0].resolve(response('A', [{ id: 'kept' }]));
	await settle();
	void controller.load();
	calls[1].reject(new Error('offline'));
	await settle();
	expect(controller.result?.plays).toEqual([{ id: 'kept' }]);
	expect(
		deriveState({
			result: controller.result,
			lastFetchedAt: controller.lastFetchedAt,
			lastError: controller.lastError,
			now: controller.now
		}).kind
	).toBe('stale');
});
