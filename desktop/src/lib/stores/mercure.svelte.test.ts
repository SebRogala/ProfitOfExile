/**
 * The mercure store (POE-336): the app's one connection, gated on the first
 * non-null `store.status`, its Tauri relay, the league reconcile, and the
 * in-process subscribers. `connectMercure` is mocked at the `$lib/api`
 * boundary with a mutable fake connection; Tauri `invoke`/`emit` are mocked.
 * The status store is faked with the same `onStatusReady` contract (call once on
 * the first non-null status, unsubscribe cancels) so tests drive readiness.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import type { MercureHandlers } from '$lib/api';

const mocks = vi.hoisted(() => ({
	connectMercure: vi.fn(),
	close: vi.fn(),
	invoke: vi.fn(async () => null),
	emit: vi.fn(async () => {}),
	status: { status: null as unknown, serverConnected: false },
	ready: new Set<() => void>(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ emit: mocks.emit, listen: vi.fn(async () => () => {}) }));
vi.mock('./status.svelte', () => ({
	store: mocks.status,
	onStatusReady: (fn: () => void) => {
		if (mocks.status.status) { fn(); return () => {}; }
		mocks.ready.add(fn);
		return () => { mocks.ready.delete(fn); };
	},
}));
vi.mock('$lib/api', async (importOriginal) => ({
	...await importOriginal<typeof import('$lib/api')>(),
	connectMercure: mocks.connectMercure,
}));

type Store = typeof import('./mercure.svelte');
let m: Store;
const status = mocks.status;
let fake: { close: () => void; connected: boolean };
let stop = () => {};

beforeEach(async () => {
	vi.resetModules();
	mocks.connectMercure.mockReset();
	mocks.close.mockReset();
	mocks.invoke.mockClear();
	mocks.emit.mockClear();
	fake = { close: mocks.close, connected: false };
	mocks.connectMercure.mockImplementation(() => fake);
	mocks.status.status = null;
	mocks.status.serverConnected = false;
	mocks.ready.clear();
	m = await import('./mercure.svelte');
});
afterEach(() => { stop(); stop = () => {}; });

function setStatus(state = 'Idle') {
	const first = status.status === null;
	status.status = { state };
	if (!first) return;
	const ready = [...mocks.ready];
	mocks.ready.clear();
	for (const fn of ready) fn();
}

function startOpen(): MercureHandlers {
	stop = m.startMercureStore();
	setStatus();
	expect(mocks.connectMercure).toHaveBeenCalledTimes(1);
	return mocks.connectMercure.mock.calls[0][0] as MercureHandlers;
}

describe('startMercureStore gate', () => {
	it('opens nothing while store.status is null', () => {
		stop = m.startMercureStore();
		expect(mocks.connectMercure).not.toHaveBeenCalled();
	});

	it('opens one connection once store.status is set', () => {
		stop = m.startMercureStore();
		setStatus();
		expect(mocks.connectMercure).toHaveBeenCalledTimes(1);
	});

	it('duplicate starts while gated open one connection when status arrives', () => {
		const first = m.startMercureStore();
		const second = m.startMercureStore();
		stop = first;
		setStatus();
		expect(second).toBe(first);
		expect(mocks.connectMercure).toHaveBeenCalledTimes(1);
	});

	it('a start after the connection opened returns the same stop and opens nothing', () => {
		startOpen();
		expect(m.startMercureStore()).toBe(stop);
		expect(mocks.connectMercure).toHaveBeenCalledTimes(1);
	});

	it('stop before the gate fires leaves a later status unconnected', () => {
		m.startMercureStore()();
		setStatus();
		expect(mocks.connectMercure).not.toHaveBeenCalled();
	});

	it('later status replacements neither reconnect nor close', () => {
		startOpen();
		setStatus('PickingGems');
		setStatus('Idle');
		expect(mocks.connectMercure).toHaveBeenCalledTimes(1);
		expect(mocks.close).not.toHaveBeenCalled();
	});

	it('stop after open closes the connection', () => {
		startOpen();
		stop();
		expect(mocks.close).toHaveBeenCalledTimes(1);
	});
});

describe('mercure relay', () => {
	it('re-emits a layout update as lab-layout-updated with difficulty and action only', () => {
		startOpen().onLayoutUpdate!({ difficulty: 'Uber', action: 'reset', extra: 1 });
		expect(mocks.emit.mock.calls).toEqual([['lab-layout-updated', { difficulty: 'Uber', action: 'reset' }]]);
	});

	it('re-emits the raw currency exchange payload as currency-exchange-updated', () => {
		const payload = { league: 'A', hour: 3 };
		startOpen().onCurrencyExchangeUpdate!(payload);
		expect(mocks.emit.mock.calls).toEqual([['currency-exchange-updated', payload]]);
	});

	it('re-emits the raw harvest payload as harvest-updated', () => {
		const payload = { horizon: 'day' };
		startOpen().onHarvestUpdate!(payload);
		expect(mocks.emit.mock.calls).toEqual([['harvest-updated', payload]]);
	});

	it('warns instead of throwing when an emit rejects', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		mocks.emit.mockRejectedValueOnce(new Error('gone'));
		startOpen().onHarvestUpdate!({});
		await vi.waitFor(() => expect(warn).toHaveBeenCalledWith('[mercure] failed to emit harvest update:', expect.any(Error)));
		warn.mockRestore();
	});
});

describe('mercure league reconcile', () => {
	it('an open reports an unknown league', () => {
		startOpen().onOpen!();
		expect(mocks.invoke.mock.calls).toEqual([['note_server_league', { observed: null }]]);
	});

	it('a league-active payload reports its league', () => {
		startOpen().onLeagueActive!({ league: 'Mercenaries' });
		expect(mocks.invoke.mock.calls).toEqual([['note_server_league', { observed: 'Mercenaries' }]]);
	});

	it('a non-string league reports null', () => {
		startOpen().onLeagueActive!({ league: 42 });
		expect(mocks.invoke.mock.calls).toEqual([['note_server_league', { observed: null }]]);
	});
});

describe('mercure subscribers', () => {
	it('a connection change writes store.serverConnected', () => {
		const handlers = startOpen();
		handlers.onConnectionChange!(true);
		expect(status.serverConnected).toBe(true);
		handlers.onConnectionChange!(false);
		expect(status.serverConnected).toBe(false);
	});

	it('a connection change reaches connection subscribers', () => {
		const seen: boolean[] = [];
		m.subscribeMercureConnection((c) => seen.push(c));
		const handlers = startOpen();
		handlers.onConnectionChange!(true);
		handlers.onConnectionChange!(false);
		expect(seen).toEqual([true, false]);
	});

	it('update subscribers receive generic updates until they unsubscribe', () => {
		const fn = vi.fn();
		const unsubscribe = m.subscribeMercureUpdate(fn);
		const handlers = startOpen();
		handlers.onUpdate();
		unsubscribe();
		handlers.onUpdate();
		expect(fn).toHaveBeenCalledTimes(1);
	});
});

describe('mercureConnected', () => {
	it('is undefined before a connection exists', () => {
		stop = m.startMercureStore();
		expect(m.mercureConnected()).toBeUndefined();
	});

	it("follows the live connection's connected flag", () => {
		startOpen();
		expect(m.mercureConnected()).toBe(false);
		fake.connected = true;
		expect(m.mercureConnected()).toBe(true);
	});
});
