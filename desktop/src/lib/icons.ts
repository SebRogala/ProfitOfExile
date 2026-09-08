import { getApiBase } from '$lib/api';

/** The source-map category exposed by the server's typed icon route. */
export type IconType = 'gems' | 'items' | 'temple' | 'currency-exchange';

/** Returns the configured server endpoint URL for an icon. */
export function getIconUrl(type: IconType, name: string): string {
	// getApiBase() already includes the "/api" suffix.
	return `${getApiBase()}/icon/${type}/${encodeURIComponent(name)}`;
}
