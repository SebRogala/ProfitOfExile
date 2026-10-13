/**
 * The saved server reaches the divine rate through the real stores (POE-286).
 *
 * `status.svelte.ts` reads `server_url` from the status it stores, and the
 * divine rate asks the server `getApiBase()` names. The order inside the
 * `status-changed` handler decides which URL that read sees, so this runs both
 * real stores and the real `$lib/api` port, mocking only Tauri and `fetch`.
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

const DIVINE_PATH = '/currency-exchange/divine-rate';

/** A promise the test settles by hand. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((r) => {
		resolve = r;
	});
	return { promise, resolve };
}

/** Let queued promise callbacks run. */
async function flush() {
	await new Promise((r) => setTimeout(r, 0));
}

let cleanup: (() => void) | null = null;
let divineRead: ReturnType<typeof deferred<Response>>;
let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
	vi.resetModules();
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
	vi.unstubAllGlobals();
});

describe('the divine rate follows the saved server', () => {
	it('asks the saved server for the divine rate once the status names it', async () => {
		const { initStatusStore } = await import('./status.svelte');
		const { currentDivineRate } = await import('./divine-rate.svelte');
		cleanup = await initStatusStore();

		mocks.handlers['status-changed']({
			payload: { server_url: 'https://saved.example' }
		});

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
});
