/**
 * Trade links for mercenary rulesets.
 *
 * Distinct from `lib/trade-utils.ts`, which builds `?q=<encoded query>` searches:
 * a mercenary ruleset is a *saved* search, addressed by the hash GGG assigned it,
 * so the URL is a bare path with no query string at all.
 */

/** The league + hash pair identifying one GGG saved search. */
export interface MercSavedSearch {
	league: string;
	hash: string;
}

/**
 * Build the trade-site URL for a saved search.
 * `league` MUST be a resolved league — there is no default-league fallback here,
 * on purpose: a saved search belongs to the league it was saved in, and guessing
 * would silently point at a different market.
 */
export function savedSearchUrl(savedSearch: MercSavedSearch): string {
	return `https://www.pathofexile.com/trade/search/${encodeURIComponent(savedSearch.league)}/${savedSearch.hash}`;
}
