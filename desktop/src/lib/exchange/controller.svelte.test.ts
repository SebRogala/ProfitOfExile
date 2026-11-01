import { afterEach, expect, it, vi } from 'vitest';
import { applySnapshot, ssot } from '$lib/stores/ssot.svelte';
import type { CurrencyExchangeResponse } from '$lib/api';
import { createExchangeController } from './controller.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

let stop = () => {};
afterEach(() => stop());

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (error: Error) => void;
	const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
	return { promise, resolve, reject };
}

function setup() {
	applySnapshot({ league: { name: 'A', generation: ssot.leagueGeneration + 1 } });
	const calls: ReturnType<typeof deferred<CurrencyExchangeResponse>>[] = [];
	const prefs = { mode: 'all', horizon: 'day', sort: 'expected' };
	const controller = createExchangeController({
		load: () => { const d = deferred<CurrencyExchangeResponse>(); calls.push(d); return d.promise; },
		selection: () => prefs,
		listen: async () => () => {}
	});
	stop = controller.start();
	return { controller, calls, prefs };
}

const response = (league: string, plays: unknown[] = []) => ({ league, plays, divineChaosRate: 300 } as CurrencyExchangeResponse);

it('withdraws rows synchronously through a failed B read', async () => {
	const { controller, calls, prefs } = setup();
	calls[0].resolve(response('A', [{ id: 'A-price' }]));
	await Promise.resolve(); await Promise.resolve();
	expect(controller.result?.plays).toEqual([{ id: 'A-price' }]);
	void controller.load();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	expect([controller.result, controller.lastFetchedAt, controller.loading]).toEqual([null, null, true]);
	calls[2].reject(new Error('B offline'));
	await Promise.resolve(); await Promise.resolve();
	calls[1].resolve(response('A', [{ id: 'late-A' }]));
	await Promise.resolve(); await Promise.resolve();
	expect([controller.result, controller.lastError, controller.loading]).toEqual([null, 'B offline', false]);
	expect(prefs).toEqual({ mode: 'all', horizon: 'day', sort: 'expected' });
});

it('rejects a mismatched payload in the current generation', async () => {
	const { controller, calls } = setup();
	calls[0].resolve(response('other', [{ id: 'wrong' }]));
	await Promise.resolve(); await Promise.resolve();
	expect([controller.result, controller.loading]).toEqual([null, false]);
});

it('admits the initial server snapshot before SSOT first resolves', async () => {
	const { controller, calls } = setup();
	ssot.league = null;
	calls[0].resolve(response('boot', [{ id: 'initial' }]));
	await Promise.resolve(); await Promise.resolve();
	expect([controller.result?.plays, controller.loading]).toEqual([[{ id: 'initial' }], false]);
});

it('a late A finalizer cannot stop the pending B spinner', async () => {
	const { controller, calls } = setup();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	calls[0].resolve(response('A', [{ id: 'late-A' }]));
	await Promise.resolve(); await Promise.resolve();
	expect([controller.result, controller.loading]).toEqual([null, true]);
	calls[1].resolve(response('B'));
	await Promise.resolve(); await Promise.resolve();
	expect([controller.result?.plays, controller.loading]).toEqual([[], false]);
});

it('preserves a same-league answer through identical snapshots', async () => {
	const { controller, calls } = setup();
	calls[0].resolve(response('A', [{ id: 'kept' }]));
	await Promise.resolve(); await Promise.resolve();
	applySnapshot({ league: { name: 'A', generation: ssot.leagueGeneration } });
	expect(controller.result?.plays).toEqual([{ id: 'kept' }]);
	expect(controller.loading).toBe(false);
});

it('disposal blocks in-flight answers and later league delivery', async () => {
	const { controller, calls } = setup();
	stop();
	applySnapshot({ league: { name: 'B', generation: ssot.leagueGeneration + 1 } });
	calls[0].resolve(response('A', [{ id: 'disposed' }]));
	await Promise.resolve(); await Promise.resolve();
	expect(controller.result).toBeNull();
});
