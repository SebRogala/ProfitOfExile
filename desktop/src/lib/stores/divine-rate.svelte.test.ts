/**
 * The divine rate store (POE-284): one read at init through the `$lib/api`
 * port, one debounced read per `currency-exchange-updated` burst, a rejected
 * read that marks a held rate stale, periodic outage/recovery reads, and the
 * 30 s clock behind "updated N min ago".
 * The port and Tauri's `listen` are mocked; `Math.random` is pinned so
 * `refetchDelay()` is a known 2000 + 0.5 × 4000 = 4000 ms.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import type { DivineRate } from '$lib/api';

const mocks = vi.hoisted(() => ({
	fetchDivineRate: vi.fn(),
	getApiBase: vi.fn(),
	unlisten: vi.fn(),
	/** The handlers the store registered, by Tauri event name. */
	handlers: {} as Record<string, (event: { payload: unknown }) => void>
}));

vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
		mocks.handlers[name] = handler;
		return mocks.unlisten;
	})
}));
vi.mock('$lib/api', () => ({ fetchDivineRate: mocks.fetchDivineRate, getApiBase: mocks.getApiBase }));

const T0 = Date.parse('2026-09-10T00:00:00Z');
const DELAY_MS = 4000;
const HEALTH_POLL_MS = 75_000; // 60 s + the pinned 0.5 × 30 s session jitter.

function port(over: Partial<DivineRate> = {}): DivineRate {
	return {
		divineChaosRate: 360.07,
		priceHour: '2026-09-09T23:00:00Z',
		updatedAt: new Date(T0 - 6 * 60_000).toISOString(),
		state: 'ready',
		unreachableSince: null,
		...over
	};
}

async function load() {
	return import('./divine-rate.svelte');
}

/** Let the init read and the `listen` registration settle. */
async function settle() {
	await vi.advanceTimersByTimeAsync(0);
}

function fireUpdated() {
	mocks.handlers['currency-exchange-updated']({ payload: null });
}

let cleanup: (() => void) | null = null;

beforeEach(() => {
	vi.resetModules();
	vi.useFakeTimers();
	vi.setSystemTime(T0);
	vi.spyOn(Math, 'random').mockReturnValue(0.5);
	mocks.fetchDivineRate.mockReset();
	mocks.getApiBase.mockReset();
	mocks.getApiBase.mockReturnValue('https://prod.example/api');
	mocks.unlisten.mockReset();
	mocks.handlers = {};
});

afterEach(() => {
	cleanup?.();
	cleanup = null;
	vi.useRealTimers();
	vi.restoreAllMocks();
});

describe('initDivineRateStore', () => {
	it('reads the port once and serves its rate', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port());
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		expect(s.currentDivineRate()).toBe(360.07);
		expect(mocks.fetchDivineRate).toHaveBeenCalledTimes(1);
	});

	it('reports stale from the source and keeps the value', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port({ state: 'stale', unreachableSince: '2026-09-09T21:04:00Z' }));
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		expect([s.divineRateState(), s.currentDivineRate()]).toEqual(['stale', 360.07]);
	});

	it('stores a zero rate from the port as no rate', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port({ divineChaosRate: 0 }));
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		expect([s.currentDivineRate(), s.divineRateState()]).toEqual([null, 'cold']);
	});

	it('does not refetch before the refetch delay after an update event', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port());
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		fireUpdated();
		await vi.advanceTimersByTimeAsync(DELAY_MS - 1);
		expect(mocks.fetchDivineRate).toHaveBeenCalledTimes(1);
	});

	it('refetches once the refetch delay after an update event has passed', async () => {
		mocks.fetchDivineRate.mockResolvedValueOnce(port()).mockResolvedValueOnce(port({ divineChaosRate: 361 }));
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		fireUpdated();
		await vi.advanceTimersByTimeAsync(DELAY_MS);
		expect(s.currentDivineRate()).toBe(361);
	});

	it('folds two update events inside one window into one refetch', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port());
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		fireUpdated();
		await vi.advanceTimersByTimeAsync(1000);
		fireUpdated();
		await vi.advanceTimersByTimeAsync(DELAY_MS * 3);
		expect(mocks.fetchDivineRate).toHaveBeenCalledTimes(2);
	});

	it('restarts the refetch delay on a second update event inside the window', async () => {
		mocks.fetchDivineRate.mockResolvedValueOnce(port()).mockResolvedValueOnce(port({ divineChaosRate: 361 }));
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		fireUpdated();
		await vi.advanceTimersByTimeAsync(1000);
		fireUpdated();
		await vi.advanceTimersByTimeAsync(3999);
		const atT4999 = s.currentDivineRate();
		await vi.advanceTimersByTimeAsync(1);
		expect([atT4999, s.currentDivineRate()]).toEqual([360.07, 361]);
	});

	it('warns with the error when a read is rejected', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const offline = new Error('offline');
		mocks.fetchDivineRate.mockRejectedValueOnce(offline);
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', offline);
	});

	it('marks the last rate stale when an update read is rejected', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const offline = new Error('offline');
		mocks.fetchDivineRate.mockResolvedValueOnce(port()).mockRejectedValueOnce(offline);
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		fireUpdated();
		await vi.advanceTimersByTimeAsync(DELAY_MS);
		expect({ ...s.divineRate, now: 0 }).toEqual({
			...port(),
			state: 'stale',
			unreachableSince: '2026-09-10T00:00:04.000Z',
			now: 0
		});
		expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', offline);
	});

	it('resetDivineRate returns the snapshot to cold', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port());
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		s.resetDivineRate();
		expect({ ...s.divineRate, now: 0 }).toEqual({
			divineChaosRate: null,
			priceHour: null,
			updatedAt: null,
			state: 'cold',
			unreachableSince: null,
			now: 0
		});
	});

	it('moves the age from 6 to 7 minutes after 60 seconds', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port());
		const s = await load();
		const { formatAge } = await import('./divine-rate');
		cleanup = s.initDivineRateStore();
		await settle();
		await vi.advanceTimersByTimeAsync(60_000);
		expect(formatAge(s.divineRate.updatedAt!, s.divineRate.now)).toBe('updated 7 min ago');
	});

	it('cancels the pending refetch on cleanup', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port());
		const s = await load();
		const stop = s.initDivineRateStore();
		await settle();
		fireUpdated();
		stop();
		await vi.advanceTimersByTimeAsync(DELAY_MS * 2);
		expect(mocks.fetchDivineRate).toHaveBeenCalledTimes(1);
	});

	it('stops the clock on cleanup', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port());
		const s = await load();
		const stop = s.initDivineRateStore();
		await settle();
		const before = s.divineRate.now;
		stop();
		await vi.advanceTimersByTimeAsync(60_000);
		expect(s.divineRate.now).toBe(before);
	});

	it('unlistens on cleanup', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port());
		const s = await load();
		const stop = s.initDivineRateStore();
		await settle();
		stop();
		await settle();
		expect(mocks.unlisten).toHaveBeenCalledTimes(1);
	});
});

/** A promise the test settles by hand, so the order answers arrive in is the test's. */
function deferred(): { promise: Promise<DivineRate>; resolve: (rate: DivineRate) => void; reject: (error: Error) => void } {
	let resolve!: (rate: DivineRate) => void;
	let reject!: (error: Error) => void;
	const promise = new Promise<DivineRate>((r, fail) => {
		resolve = r;
		reject = fail;
	});
	return { promise, resolve, reject };
}

describe('refreshDivineRate and syncDivineRateToServer', () => {
	it('keeps the newest answer when an older read settles last', async () => {
		const older = deferred();
		const newer = deferred();
		mocks.fetchDivineRate.mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise);
		const s = await load();
		const first = s.refreshDivineRate();
		const second = s.refreshDivineRate();

		newer.resolve(port({ divineChaosRate: 351.08 }));
		await second;
		older.resolve(port({ divineChaosRate: 360.07 }));
		await first;

		expect(s.currentDivineRate()).toBe(351.08);
	});

	it('an older cold answer does not erase a newer rate', async () => {
		const older = deferred();
		const newer = deferred();
		mocks.fetchDivineRate.mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise);
		const s = await load();
		const first = s.refreshDivineRate();
		const second = s.refreshDivineRate();

		newer.resolve(port({ divineChaosRate: 351.08 }));
		await second;
		older.resolve(port({ divineChaosRate: 0 }));
		await first;

		expect(s.currentDivineRate()).toBe(351.08);
	});

	it('reset drops a read in flight', async () => {
		const pending = deferred();
		mocks.fetchDivineRate.mockReturnValueOnce(pending.promise);
		const s = await load();
		const read = s.refreshDivineRate();

		s.resetDivineRate();
		pending.resolve(port({ divineChaosRate: 360.07 }));
		await read;

		expect(s.currentDivineRate()).toBeNull();
	});

	it('withdraws a rate answered by another server before asking the new one', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const s = await load();
		mocks.fetchDivineRate.mockResolvedValueOnce(port());
		await s.refreshDivineRate();
		mocks.getApiBase.mockReturnValue('https://other.example/api');
		const offline = new Error('offline');
		mocks.fetchDivineRate.mockRejectedValueOnce(offline);

		await s.syncDivineRateToServer();

		expect(s.currentDivineRate()).toBeNull();
		expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', offline);
	});

	it('keeps the rate while re-asking the same server', async () => {
		const pending = deferred();
		const s = await load();
		mocks.fetchDivineRate.mockResolvedValueOnce(port());
		await s.refreshDivineRate();
		mocks.fetchDivineRate.mockReturnValueOnce(pending.promise);

		const sync = s.syncDivineRateToServer();

		expect(s.currentDivineRate()).toBe(360.07);
		pending.resolve(port({ divineChaosRate: 361 }));
		await sync;
	});
});


describe('failed divine-rate reads', () => {
	it('keeps a failed first read cold', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const offline = new Error('offline');
		mocks.fetchDivineRate.mockRejectedValueOnce(offline);
		const s = await load();

		await s.refreshDivineRate();

		expect([s.currentDivineRate(), s.divineRateState(), s.divineRate.unreachableSince]).toEqual([null, 'cold', null]);
		expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', offline);
	});

	it('keeps the first failure time through repeated failures', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const offline = new Error('offline');
		mocks.fetchDivineRate.mockResolvedValueOnce(port()).mockRejectedValue(offline);
		const s = await load();
		await s.refreshDivineRate();
		vi.setSystemTime(T0 + 60_000);
		await s.refreshDivineRate();
		vi.setSystemTime(T0 + 120_000);

		await s.refreshDivineRate();

		expect(s.divineRate.unreachableSince).toBe('2026-09-10T00:01:00.000Z');
		expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', offline);
	});

	it('clears the outage on a successful recovery read', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const offline = new Error('offline');
		mocks.fetchDivineRate.mockResolvedValueOnce(port()).mockRejectedValueOnce(offline)
			.mockResolvedValueOnce(port({ divineChaosRate: 362 }));
		const s = await load();
		await s.refreshDivineRate();
		await s.refreshDivineRate();

		await s.refreshDivineRate();

		expect([s.currentDivineRate(), s.divineRateState(), s.divineRate.unreachableSince]).toEqual([362, 'ready', null]);
		expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', offline);
	});

	it('ignores an obsolete failure after a newer ready answer', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const offline = new Error('offline');
		const older = deferred();
		mocks.fetchDivineRate.mockReturnValueOnce(older.promise).mockResolvedValueOnce(port({ divineChaosRate: 351 }));
		const s = await load();
		const first = s.refreshDivineRate();
		await s.refreshDivineRate();

		older.reject(offline);
		await first;

		expect([s.currentDivineRate(), s.divineRateState(), s.divineRate.unreachableSince]).toEqual([351, 'ready', null]);
		expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', offline);
	});

	it('ignores an obsolete failure after a reset', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const offline = new Error('offline');
		const pending = deferred();
		mocks.fetchDivineRate.mockResolvedValueOnce(port()).mockReturnValueOnce(pending.promise);
		const s = await load();
		await s.refreshDivineRate();
		const read = s.refreshDivineRate();
		s.resetDivineRate();
		vi.setSystemTime(T0 + 60_000);

		pending.reject(offline);
		await read;

		expect({ ...s.divineRate }).toEqual({
			divineChaosRate: null, priceHour: null, updatedAt: null, state: 'cold', unreachableSince: null, now: T0
		});
		expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', offline);
	});
});

describe('divine-rate health polling', () => {
	it('detects an outage without a market update event', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const offline = new Error('offline');
		mocks.fetchDivineRate.mockResolvedValueOnce(port()).mockRejectedValueOnce(offline);
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();

		await vi.advanceTimersByTimeAsync(HEALTH_POLL_MS);

		const { divineRateView } = await import('./divine-rate');
		expect(divineRateView(s.divineRate, s.divineRate.now, 'UTC')).toEqual({
			state: 'stale',
			chip: '360',
			tooltipHtml: '<div style="font-weight:700">1 divine = 360.1 chaos (stale)</div><div style="color:var(--color-lab-text-secondary)">Last known rate from 23:00; server unreachable since 00:01.</div><div style="color:var(--color-lab-text-secondary)">Every chaos ↔ divine figure in the app uses this rate.</div>'
		});
		expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', offline);
	});

	it('recovers without a market update event', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const offline = new Error('offline');
		mocks.fetchDivineRate.mockResolvedValueOnce(port()).mockRejectedValueOnce(offline)
			.mockResolvedValueOnce(port({ divineChaosRate: 363 }));
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		await vi.advanceTimersByTimeAsync(HEALTH_POLL_MS);

		await vi.advanceTimersByTimeAsync(HEALTH_POLL_MS);

		expect([s.currentDivineRate(), s.divineRateState(), s.divineRate.unreachableSince]).toEqual([363, 'ready', null]);
		expect(warn).toHaveBeenCalledWith('[divine-rate] read failed, keeping the last rate:', offline);
	});

	it('does not supersede a read still in flight', async () => {
		const pending = deferred();
		mocks.fetchDivineRate.mockReturnValueOnce(pending.promise);
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();

		await vi.advanceTimersByTimeAsync(HEALTH_POLL_MS * 3);
		pending.resolve(port({ divineChaosRate: 364 }));
		await settle();

		expect(s.currentDivineRate()).toBe(364);
		expect(mocks.fetchDivineRate).toHaveBeenCalledTimes(1);
	});

	it('stops health reads on cleanup', async () => {
		mocks.fetchDivineRate.mockResolvedValue(port());
		const s = await load();
		const stop = s.initDivineRateStore();
		await settle();

		stop();
		await vi.advanceTimersByTimeAsync(HEALTH_POLL_MS * 3);

		expect(mocks.fetchDivineRate).toHaveBeenCalledTimes(1);
	});
});
