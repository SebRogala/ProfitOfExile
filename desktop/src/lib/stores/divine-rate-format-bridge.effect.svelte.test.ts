/**
 * The store feeds formatPrice (POE-339): one module lifecycle, so the bridge
 * writes the same `$lib/price.svelte` instance these tests read.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import type { DivineRate } from '$lib/api';
import { initDivineRateStore, resetDivineRate } from './divine-rate.svelte';
import { formatPrice, getDivineRate } from '$lib/price.svelte';

const mocks = vi.hoisted(() => ({ fetchDivineRate: vi.fn(), getApiBase: vi.fn() }));

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('$lib/api', async (importOriginal) => ({
	...await importOriginal<typeof import('$lib/api')>(),
	fetchDivineRate: mocks.fetchDivineRate,
	getApiBase: mocks.getApiBase
}));

const T0 = Date.parse('2026-11-13T10:00:00Z');

function port(divineChaosRate: number | null): DivineRate {
	return {
		divineChaosRate,
		buyChaos: null,
		sellChaos: null,
		source: divineChaosRate === null ? null : 'exchange',
		priceHour: divineChaosRate === null ? null : '2026-11-13T09:00:00Z',
		updatedAt: new Date(T0).toISOString(),
		state: divineChaosRate === null ? 'cold' : 'ready',
		unreachableSince: null
	};
}

let cleanup: (() => void) | null = null;

beforeEach(() => {
	vi.useFakeTimers();
	vi.setSystemTime(T0);
	mocks.fetchDivineRate.mockReset();
	mocks.getApiBase.mockReturnValue('https://prod.example/api');
});

afterEach(() => {
	cleanup?.();
	cleanup = null;
	vi.useRealTimers();
});

describe('divine-rate store → formatPrice', () => {
	it("feeds formatPrice the store's rate once a read lands", async () => {
		mocks.fetchDivineRate.mockResolvedValue(port(351.08));
		cleanup = initDivineRateStore();
		await vi.advanceTimersByTimeAsync(0);

		expect(getDivineRate()).toBe(351.08);
		expect(formatPrice(702.16)).toBe('2.0d');
	});

	it('leaves formatPrice in chaos while the store is cold', async () => {
		resetDivineRate();
		mocks.fetchDivineRate.mockResolvedValue(port(null));
		cleanup = initDivineRateStore();
		await vi.advanceTimersByTimeAsync(0);

		expect(getDivineRate()).toBe(0);
		expect(formatPrice(702.16)).toBe('702c');
	});
});
