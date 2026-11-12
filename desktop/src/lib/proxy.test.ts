import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { proxyRestartNeeded, type ProxySettings } from './proxy';

const inEffect: ProxySettings = {
	saved: 'socks5://127.0.0.1:1080',
	inEffect: 'socks5://127.0.0.1:1080',
	savedTrade: true,
	tradeInEffect: true,
	updaterProxy: 'socks5h://127.0.0.1:1080',
	blocked: ''
};

describe('proxyRestartNeeded', () => {
	it('is false while the saved URL and trade flag match the ones in effect', () => {
		expect(proxyRestartNeeded(inEffect)).toBe(false);
		expect(proxyRestartNeeded({ ...inEffect, savedTrade: false, tradeInEffect: false })).toBe(false);
	});

	it('is true when the saved URL differs, either way', () => {
		expect(proxyRestartNeeded({ ...inEffect, saved: 'http://127.0.0.1:8080' })).toBe(true);
		expect(proxyRestartNeeded({ ...inEffect, saved: '' })).toBe(true);
		expect(proxyRestartNeeded({ ...inEffect, saved: 'http://p:1', inEffect: '' })).toBe(true);
	});

	it('is true when the saved trade flag differs, either way', () => {
		expect(proxyRestartNeeded({ ...inEffect, savedTrade: false })).toBe(true);
		expect(proxyRestartNeeded({ ...inEffect, savedTrade: true, tradeInEffect: false })).toBe(true);
	});

	it('is false when the trade flags differ but no proxy is saved or in effect', () => {
		const noProxy = { ...inEffect, saved: '', inEffect: '' };
		expect(proxyRestartNeeded({ ...noProxy, savedTrade: false, tradeInEffect: true })).toBe(false);
		expect(proxyRestartNeeded({ ...noProxy, savedTrade: true, tradeInEffect: false })).toBe(false);
	});
});
