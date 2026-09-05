/**
 * What `initStatusStore()` is allowed to wait for (POE-203).
 *
 * Entitlements arrive over a `fetch` with no timeout that keeps retrying an
 * unreachable server for the life of the app. The startup path is allowed to
 * give that a bounded head start — the channel decides which update manifests
 * get asked — but it must not hand the app's boot, its cleanup handle, or the
 * registration of the 30-minute update poll to a network that may never come up.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

const mocks = vi.hoisted(() => ({
	loadEntitlements: vi.fn(),
	refreshEntitlements: vi.fn(),
	resetEntitlements: vi.fn(),
	checkForUpdate: vi.fn(),
	/** The `status-changed` / `logs-changed` handlers the store registered. */
	handlers: {} as Record<string, (event: { payload: unknown }) => void>
}));

// The store reaches Rust through `invoke`/`listen` only; the real core and
// event modules cannot load outside a webview. Same shape as ssot.svelte.test.ts.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }));
vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
		mocks.handlers[name] = handler;
		return vi.fn();
	})
}));
vi.mock('$lib/stores/entitlements.svelte', () => ({
	loadEntitlements: mocks.loadEntitlements,
	refreshEntitlements: mocks.refreshEntitlements,
	resetEntitlements: mocks.resetEntitlements
}));
vi.mock('$lib/updater/check', () => ({ checkForUpdate: mocks.checkForUpdate }));

/** The bound `status.svelte.ts` gives entitlements before it checks anyway. */
const ENTITLEMENTS_WAIT_MS = 10_000;
/** The update poll's period. */
const POLL_MS = 30 * 60 * 1000;

/** An entitlements load that never lands — an offline device, all session. */
function neverLands(): Promise<void> {
	return new Promise<void>(() => {});
}

let warn: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
	vi.resetModules();
	vi.useFakeTimers();
	mocks.loadEntitlements.mockReset();
	mocks.refreshEntitlements.mockReset();
	mocks.refreshEntitlements.mockResolvedValue(undefined);
	mocks.resetEntitlements.mockReset();
	mocks.handlers = {};
	mocks.checkForUpdate.mockReset();
	mocks.checkForUpdate.mockResolvedValue(null);
	warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
});

afterEach(() => {
	vi.useRealTimers();
	warn.mockRestore();
});

describe('initStatusStore', () => {
	it('hands back its cleanup handle without waiting for entitlements', async () => {
		// The regression this pins: an awaited load held the caller — the root
		// layout's onMount — open for as long as the OS kept the socket alive.
		mocks.loadEntitlements.mockReturnValue(neverLands());
		const { initStatusStore } = await import('./status.svelte');

		let cleanup: (() => void) | null = null;
		void initStatusStore().then((c) => {
			cleanup = c;
		});
		await vi.advanceTimersByTimeAsync(0);

		expect(cleanup).toBeTypeOf('function');
	});

	it('checks for updates once the entitlements wait runs out', async () => {
		mocks.loadEntitlements.mockReturnValue(neverLands());
		const { initStatusStore } = await import('./status.svelte');

		void initStatusStore();
		await vi.advanceTimersByTimeAsync(ENTITLEMENTS_WAIT_MS);

		expect(mocks.checkForUpdate).toHaveBeenCalledTimes(1);
	});

	it('does not check for updates before the wait is up while entitlements are still landing', async () => {
		// The head start is what stops a beta device's FIRST check from asking
		// the stable manifest alone.
		mocks.loadEntitlements.mockReturnValue(neverLands());
		const { initStatusStore } = await import('./status.svelte');

		void initStatusStore();
		await vi.advanceTimersByTimeAsync(ENTITLEMENTS_WAIT_MS - 1);

		expect(mocks.checkForUpdate).not.toHaveBeenCalled();
	});

	it('checks for updates as soon as entitlements land rather than waiting the bound out', async () => {
		mocks.loadEntitlements.mockResolvedValue(undefined);
		const { initStatusStore } = await import('./status.svelte');

		void initStatusStore();
		await vi.advanceTimersByTimeAsync(0);

		expect(mocks.checkForUpdate).toHaveBeenCalledTimes(1);
	});

	it('keeps polling for updates even when entitlements never landed', async () => {
		// The interval is armed regardless: a device that was offline at launch
		// must still be told about a new build once it is back.
		mocks.loadEntitlements.mockReturnValue(neverLands());
		const { initStatusStore } = await import('./status.svelte');

		void initStatusStore();
		await vi.advanceTimersByTimeAsync(POLL_MS + ENTITLEMENTS_WAIT_MS);

		expect(mocks.checkForUpdate).toHaveBeenCalledTimes(2);
	});

	it('re-asks for entitlements on the update tick so a promote lands without a restart', async () => {
		mocks.loadEntitlements.mockResolvedValue(undefined);
		const { initStatusStore } = await import('./status.svelte');

		void initStatusStore();
		await vi.advanceTimersByTimeAsync(POLL_MS);

		expect(mocks.loadEntitlements).toHaveBeenCalledTimes(2);
	});

	it('stops polling once the cleanup handle is called', async () => {
		mocks.loadEntitlements.mockResolvedValue(undefined);
		const { initStatusStore } = await import('./status.svelte');

		const cleanup = await initStatusStore();
		await vi.advanceTimersByTimeAsync(0);
		cleanup();
		await vi.advanceTimersByTimeAsync(3 * POLL_MS);

		expect(mocks.checkForUpdate).toHaveBeenCalledTimes(1);
	});

	it('records the version the channel-aware check offered', async () => {
		const close = vi.fn(async () => {});
		mocks.loadEntitlements.mockResolvedValue(undefined);
		mocks.checkForUpdate.mockResolvedValue({ version: '1.3.0-beta.1', close });
		const { initStatusStore, store } = await import('./status.svelte');

		void initStatusStore();
		await vi.advanceTimersByTimeAsync(0);

		expect(store.updateVersion).toBe('1.3.0-beta.1');
	});
});

/**
 * What a `server_url` change does to the grant on screen.
 *
 * The local server and production hold different roles for the same device,
 * so the modules drawn for one must not survive a switch to the other, and the
 * new server must be asked now rather than on the 30-minute tick.
 */
describe('a server_url change', () => {
	/** A status carrying just what the switch reads. */
	function status(serverUrl: string) {
		return { payload: { server_url: serverUrl } };
	}

	it('withdraws the grant and asks the new server at once', async () => {
		mocks.loadEntitlements.mockResolvedValue(undefined);
		const { initStatusStore } = await import('./status.svelte');
		await initStatusStore();
		mocks.handlers['status-changed'](status('https://prod.example'));

		mocks.handlers['status-changed'](status('https://profitofexile.localhost'));

		expect(mocks.resetEntitlements).toHaveBeenCalledTimes(1);
		expect(mocks.refreshEntitlements).toHaveBeenCalledTimes(1);
	});

	it('does not read the first status as a switch', async () => {
		// Startup asks on its own; a reset here would race the startup load.
		mocks.loadEntitlements.mockResolvedValue(undefined);
		const { initStatusStore } = await import('./status.svelte');
		await initStatusStore();

		mocks.handlers['status-changed'](status('https://prod.example'));

		expect(mocks.resetEntitlements).not.toHaveBeenCalled();
		expect(mocks.refreshEntitlements).not.toHaveBeenCalled();
	});

	it('ignores a status that repeats the url', async () => {
		// `status-changed` fires on every state mutation, not only this one.
		mocks.loadEntitlements.mockResolvedValue(undefined);
		const { initStatusStore } = await import('./status.svelte');
		await initStatusStore();
		mocks.handlers['status-changed'](status('https://prod.example'));

		mocks.handlers['status-changed'](status('https://prod.example'));
		mocks.handlers['status-changed'](status('https://prod.example'));

		expect(mocks.resetEntitlements).not.toHaveBeenCalled();
	});

	it('resets before it refreshes', async () => {
		// The other order would let the new answer land and then be wiped.
		const order: string[] = [];
		mocks.resetEntitlements.mockImplementation(() => order.push('reset'));
		mocks.refreshEntitlements.mockImplementation(async () => { order.push('refresh'); });
		mocks.loadEntitlements.mockResolvedValue(undefined);
		const { initStatusStore } = await import('./status.svelte');
		await initStatusStore();
		mocks.handlers['status-changed'](status('https://prod.example'));

		mocks.handlers['status-changed'](status('https://profitofexile.localhost'));

		expect(order).toEqual(['reset', 'refresh']);
	});
});
