/**
 * Gem icon URL resolver.
 * Uses the poewiki.net MediaWiki API to resolve gem names to icon URLs.
 * Falls back to a constructed URL pattern for unknown gems.
 */

const WIKI_API = 'https://www.poewiki.net/api.php';

const cache = new Map<string, string>();

/**
 * Returns the wiki icon URL for a gem name.
 * Uses a batch-preloaded cache; falls back to API lookup.
 */
export function getGemIconUrl(gemName: string): string {
	if (cache.has(gemName)) return cache.get(gemName)!;
	// Construct a best-guess URL (won't always work due to hash paths, but serves as fallback)
	const filename = gemName.replace(/ /g, '_').replace(/'/g, '%27') + '_inventory_icon.png';
	return `https://www.poewiki.net/wiki/Special:Filepath/${filename}`;
}

/**
 * Preload icon URLs for a batch of gem names via the MediaWiki API.
 * Call this once when data loads to populate the cache.
 */
export async function preloadGemIcons(gemNames: string[]): Promise<void> {
	const uncached = gemNames.filter((n) => !cache.has(n));
	if (uncached.length === 0) return;

	// API limit: 50 titles per request
	for (let i = 0; i < uncached.length; i += 50) {
		const batch = uncached.slice(i, i + 50);
		const titles = batch
			.map((n) => 'File:' + n.replace(/ /g, '_') + '_inventory_icon.png')
			.join('|');

		try {
			const url = `${WIKI_API}?action=query&titles=${encodeURIComponent(titles)}&prop=imageinfo&iiprop=url&format=json&origin=*`;
			const resp = await fetch(url);
			const data = await resp.json();
			const pages = data?.query?.pages;
			if (!pages) continue;

			for (const page of Object.values(pages) as any[]) {
				if (page.imageinfo?.[0]?.url) {
					const gem = page.title.replace('File:', '').replace(' inventory icon.png', '');
					cache.set(gem, page.imageinfo[0].url);
				}
			}
		} catch {
			// Silently fail — icons are cosmetic
		}
	}
}
