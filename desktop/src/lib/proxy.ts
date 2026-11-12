import { invoke } from '@tauri-apps/api/core';

/** The saved proxy and the one this process started with (POE-360). */
export interface ProxySettings {
	saved: string;
	inEffect: string;
	/** Whether trade requests (pathofexile.com) use the proxy too. */
	savedTrade: boolean;
	tradeInEffect: boolean;
	/** The proxy in effect as the updater takes it (socks5h for SOCKS); empty when none. */
	updaterProxy: string;
}

/** Saved settings differ from the ones this run started with. */
export function proxyRestartNeeded(p: ProxySettings): boolean {
	if (p.saved !== p.inEffect) return true;
	return p.saved !== '' && p.savedTrade !== p.tradeInEffect;
}

export function getProxySettings(): Promise<ProxySettings> {
	return invoke<ProxySettings>('get_proxy_settings');
}

/** Rejects with the validation message for anything but http/https/socks5. */
export function setProxyUrl(url: string): Promise<void> {
	return invoke('set_proxy_url', { url });
}

export function setProxyTrade(enabled: boolean): Promise<void> {
	return invoke('set_proxy_trade', { enabled });
}
