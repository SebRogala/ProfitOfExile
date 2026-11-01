import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => vi.fn()) }));

let invokeMock: ReturnType<typeof vi.mocked<typeof import('@tauri-apps/api/core').invoke>>;

beforeEach(async () => {
	vi.resetModules();
	vi.clearAllMocks();
	const core = await import('@tauri-apps/api/core');
	invokeMock = vi.mocked(core.invoke);
});

afterEach(() => vi.clearAllMocks());

// Transport literals mirror LootSlice's serialized fields, not its Rust defaults.
function snapshot(format: string) {
	return { league: { name: null }, bossLoot: {
		enabled: false, settings: { layout: 'grid', autoSaveSeconds: 10, notCounted: [],
			captureShortcut: { key: 2, shift: true, ctrl: false, alt: false },
			previewShortcut: { key: 119, shift: true, ctrl: false, alt: false } },
		numberFormat: format, pending: null, frame: null, rebind: null, nowMs: 2000
	} };
}

async function webview(format: string) {
	invokeMock.mockResolvedValue(snapshot(format));
	const store = await import('./number-format.svelte');
	const { fetchSsot } = await import('./ssot.svelte');
	await fetchSsot();
	return store;
}

describe('typed number format routing', () => {
	it('reads the native pick with the module off', async () => {
		const store = await webview('space');
		expect(store.groupNumber('11225')).toBe('11 225');
	});

	it('uses Auto before the first native snapshot', async () => {
		const store = await import('./number-format.svelte');
		expect(store.numberFormat()).toBe('auto');
	});

	it('narrows an unknown wire pick to Auto', async () => {
		const store = await webview('dot');
		expect(store.numberFormat()).toBe('auto');
	});

	it('writes through the native command instead of a pref', async () => {
		const store = await webview('space');
		invokeMock.mockClear();
		await store.setNumberFormat('comma');
		expect(invokeMock).toHaveBeenCalledWith('set_number_format', { format: 'comma' });
		expect(invokeMock.mock.calls.some(([command]) => command === 'set_ui_pref')).toBe(false);
	});

	it('updates the initiating formatter before the native echo', async () => {
		const store = await webview('space');
		const pending = store.setNumberFormat('none');
		expect(store.groupNumber('1234567')).toBe('1234567');
		await pending;
	});

	it('reads the changed pick in a separate WebView module instance', async () => {
		const main = await webview('space');
		await main.setNumberFormat('comma');
		vi.resetModules();
		const overlay = await webview('comma');
		expect(overlay.groupNumber('1234567')).toBe('1,234,567');
	});

	it('restores the native persisted pick after restart', async () => {
		await webview('comma');
		vi.resetModules();
		const restarted = await webview('space');
		expect(restarted.groupNumber('1234567')).toBe('1 234 567');
	});

	it('reconciles a refused write on the next native snapshot', async () => {
		const store = await webview('space');
		invokeMock.mockRejectedValueOnce('native unavailable');
		const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
		await store.setNumberFormat('none');
		const { fetchSsot } = await import('./ssot.svelte');
		await fetchSsot();
		expect(store.groupNumber('1234567')).toBe('1 234 567');
		expect(warning).toHaveBeenCalledWith('[number-format] set_number_format failed:', 'native unavailable');
		warning.mockRestore();
	});
});
