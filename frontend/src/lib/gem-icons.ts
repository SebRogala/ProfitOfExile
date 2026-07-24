/**
 * Gem icon URL resolver.
 *
 * Returns a URL to the server's gem-icon endpoint, which serves the correct
 * poewiki image from a persistent server-side cache (see internal/gemicon).
 * The server responds 404 for genuinely-unknown gems; GemIcon.svelte's onerror
 * handler renders the "?" fallback in that case.
 */

/** Returns the server endpoint URL for a gem's inventory icon. */
export function getGemIconUrl(gemName: string): string {
	// Web client talks to the API on the same origin under "/api" (see
	// frontend/src/lib/api.ts API_BASE).
	return `/api/gem-icon/${encodeURIComponent(gemName)}`;
}
