import { invoke } from '@tauri-apps/api/core';

/** The saved proxy and the one this process started with (POE-360). */
export interface ProxySettings {
	saved: string;
	inEffect: string;
}

export function getProxySettings(): Promise<ProxySettings> {
	return invoke<ProxySettings>('get_proxy_settings');
}

/** Rejects with the validation message for anything but http/https/socks5. */
export function setProxyUrl(url: string): Promise<void> {
	return invoke('set_proxy_url', { url });
}
