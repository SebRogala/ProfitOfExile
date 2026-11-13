import { getApiBase } from '$lib/api';

/** The source-map category exposed by the server's typed icon route. */
export type IconType = 'gems' | 'items' | 'temple' | 'currency-exchange';

/** The server endpoint URL for an icon, or null while the server is not known yet. */
export function getIconUrl(type: IconType, name: string): string | null {
	const base = getApiBase();
	return base === null ? null : `${base}/icon/${type}/${encodeURIComponent(name)}`;
}
