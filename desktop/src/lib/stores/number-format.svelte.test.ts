import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * The store reads and writes the `numberFormat` `ui_prefs` key through
 * `invoke`, so the Tauri core is mocked, and each test imports a fresh store
 * and prefs module (both hold module-level state). Fake timers, as in
 * `navigation.svelte.test.ts`: a pref write is debounced 300 ms.
 */
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));

type Store = typeof import('./number-format.svelte');

let invokeMock: ReturnType<typeof vi.mocked<typeof import('@tauri-apps/api/core').invoke>>;

beforeEach(async () => {
	vi.resetModules();
	vi.clearAllMocks();
	vi.useFakeTimers();
	const core = await import('@tauri-apps/api/core');
	invokeMock = vi.mocked(core.invoke);
});

afterEach(() => {
	vi.clearAllTimers();
	vi.useRealTimers();
});

/** A fresh, initialised store whose prefs load answered with `prefs`. */
async function storeLoadedWith(prefs: Record<string, string>): Promise<Store> {
	invokeMock.mockImplementation(async (command: string) => (command === 'get_ui_prefs' ? prefs : undefined));
	const mod = await import('./number-format.svelte');
	mod.initNumberFormatStore();
	await vi.advanceTimersByTimeAsync(0);
	return mod;
}

describe('number format store', () => {
	it('reads a settings file written before the key existed as Auto', async () => {
		const store = await storeLoadedWith({ navView: 'lab' });

		expect(store.numberFormat()).toBe('auto');
	});

	it('groups with the stored pick once the prefs load lands', async () => {
		const store = await storeLoadedWith({ numberFormat: 'space' });

		expect(store.groupNumber('11225')).toBe('11 225');
	});

	it('reads an unknown stored value as Auto', async () => {
		const store = await storeLoadedWith({ numberFormat: 'dot' });

		expect(store.numberFormat()).toBe('auto');
	});

	it('persists a pick to the numberFormat pref', async () => {
		const store = await storeLoadedWith({});

		store.setNumberFormat('none');
		await vi.advanceTimersByTimeAsync(300);

		expect(invokeMock).toHaveBeenCalledWith('set_ui_pref', { key: 'numberFormat', value: 'none' });
	});
});
