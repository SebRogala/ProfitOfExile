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
	checkForUpdate: vi.fn()
}));

// The store reaches Rust through `invoke`/`listen` only; the real core and
// event modules cannot load outside a webview. Same shape as ssot.svelte.test.ts.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => vi.fn()) }));
vi.mock('$lib/stores/entitlements.svelte', () => ({ loadEntitlements: mocks.loadEntitlements }));
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
