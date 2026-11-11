import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { connectMercure, isLeagueChange, serverLeagueDiffers } from './api';

describe('isLeagueChange', () => {
	it('reports a change when the event names a different league', () => {
		expect(isLeagueChange({ topic: 'poe/league/active', league: 'Keepers', leagueRevision: 3 }, 'Mercenaries')).toBe(true);
	});

	it('reports no change when the event names the held league', () => {
		expect(isLeagueChange({ topic: 'poe/league/active', league: 'Mercenaries', leagueRevision: 3 }, 'Mercenaries')).toBe(false);
	});

	it('reports a change when the page holds no league yet and the event names one', () => {
		expect(isLeagueChange({ league: 'Keepers' }, '')).toBe(true);
	});

	it.each([
		['no league field', { topic: 'poe/league/active' }],
		['a non-string league', { league: 42 }],
		['a null payload', null],
		['an undefined payload', undefined],
	])('reports no change for %s', (_label, data) => {
		expect(isLeagueChange(data, 'Mercenaries')).toBe(false);
	});
});

let originalFetch: typeof globalThis.fetch;
let originalEventSource: typeof globalThis.EventSource;

beforeEach(() => {
	originalFetch = globalThis.fetch;
	originalEventSource = globalThis.EventSource;
});

afterEach(() => {
	vi.useRealTimers();
	globalThis.fetch = originalFetch;
	globalThis.EventSource = originalEventSource;
	vi.restoreAllMocks();
});

function serveStatus(response: { ok: boolean; body?: unknown }) {
	globalThis.fetch = vi.fn(async () => ({
		ok: response.ok,
		status: response.ok ? 200 : 503,
		statusText: response.ok ? 'OK' : 'Service Unavailable',
		json: async () => response.body,
	})) as unknown as typeof fetch;
}

describe('serverLeagueDiffers', () => {
	it('reports a difference when the server now serves another league', async () => {
		serveStatus({ ok: true, body: { league: 'Keepers' } });
		expect(await serverLeagueDiffers('Mercenaries')).toBe(true);
	});

	it('reports no difference when the server still serves the held league', async () => {
		serveStatus({ ok: true, body: { league: 'Mercenaries' } });
		expect(await serverLeagueDiffers('Mercenaries')).toBe(false);
	});

	it('reports no difference when the status read fails', async () => {
		vi.spyOn(console, 'warn').mockImplementation(() => {});
		serveStatus({ ok: false });
		expect(await serverLeagueDiffers('Mercenaries')).toBe(false);
	});
});

describe('connectMercure league hint on stream open', () => {
	let opened: FakeEventSource[];

	class FakeEventSource {
		onopen: (() => void) | null = null;
		onmessage: ((e: MessageEvent) => void) | null = null;
		onerror: (() => void) | null = null;
		constructor(public url: string) {
			opened.push(this);
		}
		close() {}
	}

	beforeEach(() => {
		opened = [];
		globalThis.EventSource = FakeEventSource as unknown as typeof globalThis.EventSource;
		serveStatus({ ok: true, body: { token: 'tok', url: 'https://hub.test/.well-known/mercure' } });
		vi.useFakeTimers();
		vi.spyOn(Math, 'random').mockReturnValue(0);
		vi.spyOn(console, 'warn').mockImplementation(() => {});
	});

	it('asks for no league check on the first open', async () => {
		const onLeagueActive = vi.fn();
		const connection = connectMercure(vi.fn(), undefined, onLeagueActive);
		await vi.advanceTimersByTimeAsync(0);
		opened[0].onopen!();
		expect(onLeagueActive).not.toHaveBeenCalled();
		connection.close();
	});

	it('asks for a league check without a league when the stream reopens', async () => {
		const onUpdate = vi.fn();
		const onLeagueActive = vi.fn();
		const connection = connectMercure(onUpdate, undefined, onLeagueActive);
		await vi.advanceTimersByTimeAsync(0);
		opened[0].onopen!();
		opened[0].onerror!();
		await vi.advanceTimersByTimeAsync(2000);
		expect(opened).toHaveLength(2);
		opened[1].onopen!();
		expect(onLeagueActive.mock.calls).toEqual([[]]);
		expect(onUpdate).not.toHaveBeenCalled();
		connection.close();
	});
});
