/**
 * The saved server reaches the divine rate through the real stores (POE-286).
 *
 * The saved server arrives on the SSOT snapshot (POE-383): `applySnapshot`
 * hands it to the server-base carrier, which notifies `status.svelte.ts`, and
 * the divine rate and entitlements ask the server `getApiBase()` names. The
 * carrier must hold the new url BEFORE it notifies, so this runs the real
 * stores and the real `$lib/api` port, mocking only Tauri and `fetch`.
 *
 * A separate file rather than `status.svelte.test.ts`: that file mocks the
 * divine-rate store as a whole, and this test is the one place the two meet.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

const mocks = vi.hoisted(() => ({
	invoke: vi.fn(),
	/** The Tauri handlers the status store registered, by event name. */
	handlers: {} as Record<string, (event: { payload: unknown }) => void>
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
		mocks.handlers[name] = handler;
		return vi.fn();
	})
}));

// Static imports: divine-rate.svelte.ts rate/generation lives for the whole
// file; the server carrier is reset per test.
import { initStatusStore } from './status.svelte';
import { currentDivineRate, refreshDivineRate } from './divine-rate.svelte';
import { applySnapshot, type SsotSnapshot } from './ssot.svelte';
import { resetServerBaseForTests } from './server-base.svelte';

/** A snapshot carrying just the server slice. */
function snapshotFor(url: string): SsotSnapshot {
	return { league: { name: null }, server: { url, defaultUrl: 'https://default.example' } } as SsotSnapshot;
}

const DIVINE_PATH = '/currency-exchange/divine-rate';

/** A promise the test settles by hand. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((r) => {
		resolve = r;
	});
	return { promise, resolve };
}

/** Let queued promise callbacks run; advances only zero-delay timers, never the stores' intervals. */
async function flush() {
	await vi.advanceTimersByTimeAsync(0);
}

let cleanup: (() => void) | null = null;
let divineRead: ReturnType<typeof deferred<Response>>;
let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
	vi.useFakeTimers();
	resetServerBaseForTests();
	mocks.handlers = {};
	// `get_status` and `get_logs` come back empty; the store then waits for an event.
	mocks.invoke.mockReset();
	mocks.invoke.mockResolvedValue(null);
	// Only the divine-rate request is answered; every other request hangs, so
	// entitlements and the update check never settle or retry during the test.
	divineRead = deferred<Response>();
	fetchMock = vi.fn((url: string) =>
		String(url).includes(DIVINE_PATH) ? divineRead.promise : new Promise<Response>(() => {})
	);
	vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
	cleanup?.();
	cleanup = null;
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

describe('the divine rate follows the saved server', () => {
	it('asks the saved server for the divine rate once the snapshot names it', async () => {
		cleanup = await initStatusStore();

		applySnapshot(snapshotFor('https://saved.example'));
		await flush();

		const rateUrls = fetchMock.mock.calls
			.map(([url]) => String(url))
			.filter((url) => url.includes(DIVINE_PATH));
		expect(rateUrls).toEqual(['https://saved.example/api' + DIVINE_PATH]);

		divineRead.resolve({
			ok: true,
			json: async () => ({ divineChaosRate: 351.08, priceHour: '2026-10-13T08:00:00Z', warm: true })
		} as unknown as Response);
		await flush();

		expect(currentDivineRate()).toBe(351.08);
	});

	it('sends the requests a server change fires to the new server', async () => {
		cleanup = await initStatusStore();
		mocks.handlers['status-changed']({ payload: { device_id: 'dev-1', server_url: 'https://a.example' } });
		applySnapshot(snapshotFor('https://a.example'));
		await flush();
		fetchMock.mockClear();

		applySnapshot(snapshotFor('https://b.example'));
		await flush();

		const hosts = (needle: string) =>
			fetchMock.mock.calls.map(([url]) => String(url)).filter((url) => url.includes(needle)).map((url) => new URL(url).host);
		expect(hosts('/device/me')).toEqual(['b.example']);
		expect(hosts(DIVINE_PATH)).toEqual(['b.example']);
	});
});

/**
 * A rate read started before the first server slice waits in `$lib/api`, then
 * goes to that server. It must still be recorded as that server's answer, or a
 * later switch cannot withdraw it (review r1 P2).
 */
describe('a divine rate read made before the server is known', () => {
	it('is withdrawn when the app switches to another server', async () => {
		const answers: Array<() => Promise<Response>> = [
			async () => ({ ok: true, json: async () => ({ divineChaosRate: 351.08, priceHour: '2026-10-13T08:00:00Z', warm: true }) }) as unknown as Response,
			async () => { throw new Error('A down'); },
			async () => { throw new Error('B down'); }
		];
		fetchMock.mockImplementation((url: string) =>
			String(url).includes(DIVINE_PATH) ? answers.shift()!() : new Promise<Response>(() => {})
		);

		const startup = refreshDivineRate();
		applySnapshot(snapshotFor('https://a.example'));
		await startup;
		expect(currentDivineRate()).toBe(351.08);

		cleanup = await initStatusStore();
		await flush();
		applySnapshot(snapshotFor('https://b.example'));
		await flush();

		expect(currentDivineRate()).toBeNull();
	});
});
