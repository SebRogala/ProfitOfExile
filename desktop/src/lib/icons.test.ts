/**
 * Icon URLs follow the saved server, in every window (POE-383).
 *
 * The lab and temple overlays render icons but never start the status store, so
 * a base read from `store.status` fell through to the build's VITE_SERVER_URL:
 * icons came from the server the binary was built for, not the one saved in
 * Settings. The base now comes from the SSOT snapshot's server slice.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.0.0-test' }));
vi.mock('$lib/stores/status.svelte', () => ({ store: { status: null } }));

const { getIconUrl } = await import('./icons');
const { applyServerSlice, resetServerBaseForTests } = await import('$lib/stores/server-base.svelte');

beforeEach(() => {
	vi.stubEnv('VITE_SERVER_URL', 'https://built.example');
	resetServerBaseForTests();
});

afterEach(() => {
	vi.unstubAllEnvs();
});

describe('getIconUrl', () => {
	it('builds from the saved server in a window without a status store', () => {
		applyServerSlice({ url: 'https://saved.example', defaultUrl: 'https://built.example' });

		expect(getIconUrl('gems', 'Arc of Surging')).toBe('https://saved.example/api/icon/gems/Arc%20of%20Surging');
	});

	it('is null before the server is known', () => {
		expect(getIconUrl('gems', 'Arc of Surging')).toBeNull();
	});
});
