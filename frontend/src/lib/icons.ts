/** The source-map category exposed by the server's typed icon route. */
export type IconType = 'gems' | 'items' | 'temple' | 'currency-exchange';

/** Returns the same-origin server endpoint URL for an icon. */
export function getIconUrl(type: IconType, name: string): string {
	// Web client talks to the API on the same origin under "/api" (see
	// frontend/src/lib/api.ts API_BASE).
	return `/api/icon/${type}/${encodeURIComponent(name)}`;
}
