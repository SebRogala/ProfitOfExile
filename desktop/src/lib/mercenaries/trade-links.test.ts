import { describe, it, expect } from 'vitest';
import { savedSearchUrl } from './trade-links';
import { MERC_SOURCES, allRulesets } from './rulesets';

/**
 * Split a built saved-search URL into its decoded `/trade/search/<league>/<hash>`
 * segments. Asserting on decoded segments rather than on substrings of the raw URL
 * is what makes these mutation-resistant: a re-added `|| 'Allflame'` league
 * fallback, or a hash appended to the wrong segment, changes what comes back here.
 */
function searchPathSegments(url: string): { league: string; hash: string } {
	const { pathname } = new URL(url);
	const prefix = '/trade/search/';
	expect(pathname.startsWith(prefix)).toBe(true);
	const [league, hash, ...extra] = pathname.slice(prefix.length).split('/');
	expect(extra).toEqual([]);
	return { league: decodeURIComponent(league), hash: decodeURIComponent(hash) };
}

describe('savedSearchUrl', () => {
	it('puts the caller-supplied league in the trade search path', () => {
		const url = savedSearchUrl({ league: 'Allflame', hash: 'WvKGjV8Kfm' });
		expect(searchPathSegments(url).league).toBe('Allflame');
	});

	it('emits an empty league segment (not a hardcoded league) for an empty league', () => {
		const url = savedSearchUrl({ league: '', hash: 'WvKGjV8Kfm' });
		const { league } = searchPathSegments(url);
		expect(league).toBe('');
		expect(league).not.toBe('Allflame');
	});

	it('percent-encodes a league containing a space', () => {
		const url = savedSearchUrl({ league: 'Hardcore Allflame', hash: 'WvKGjV8Kfm' });
		expect(url).toContain('/trade/search/Hardcore%20Allflame/');
		expect(searchPathSegments(url).league).toBe('Hardcore Allflame');
	});

	it('addresses the saved search by hash in the segment after the league', () => {
		const url = savedSearchUrl({ league: 'Allflame', hash: 'zbrQyEqah4' });
		expect(searchPathSegments(url).hash).toBe('zbrQyEqah4');
	});

	// The other trade builders in lib/trade-utils.ts encode a whole query into
	// `?q=`. A saved search is addressed by hash alone; appending a query would
	// make the trade site open an unsaved search instead.
	it('builds a bare path with no query string', () => {
		const url = savedSearchUrl({ league: 'Allflame', hash: 'zbrQyEqah4' });
		expect(new URL(url).search).toBe('');
	});

	it('links each declared ruleset to its own saved search', () => {
		const linked = allRulesets().map((r) => searchPathSegments(savedSearchUrl(r.savedSearch)));
		expect(linked).toEqual(allRulesets().map((r) => r.savedSearch));
	});
});

describe('source guide links', () => {
	// Finding 17: pinned as the CURRENT state, not as a permanent rule. When
	// Sebastian supplies the guide URLs this becomes an https + host assertion.
	it('has no guide URL for either source yet', () => {
		expect(MERC_SOURCES.map((s) => [s.id, s.guideUrl])).toEqual([
			['guide-a', null],
			['guide-b', null]
		]);
	});
});
