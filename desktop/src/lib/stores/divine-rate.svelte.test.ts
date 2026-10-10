/**
 * The divine rate store (POE-284): one read at init through the `$lib/api`
 * port, one debounced read per `currency-exchange-updated` burst, a rejected
 * read that changes nothing, and the 30 s clock behind "updated N min ago".
 * The port and Tauri's `listen` are mocked; `Math.random` is pinned so
 * `refetchDelay()` is a known 2000 + 0.5 × 4000 = 4000 ms.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import type { DivineRate } from '$lib/api';

const mocks = vi.hoisted(() => ({
	fetchDivineRate: vi.fn(),
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
vi.mock('$lib/api', () => ({ fetchDivineRate: mocks.fetchDivineRate }));

const T0 = Date.parse('2026-09-10T00:00:00Z');
const DELAY_MS = 4000;

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

	it('leaves the snapshot unchanged when a read after a good one is rejected', async () => {
		mocks.fetchDivineRate.mockResolvedValueOnce(port()).mockRejectedValueOnce(new Error('offline'));
		const s = await load();
		cleanup = s.initDivineRateStore();
		await settle();
		fireUpdated();
		await vi.advanceTimersByTimeAsync(DELAY_MS);
		expect({ ...s.divineRate, now: 0 }).toEqual({ ...port(), now: 0 });
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
