import { describe, it, expect } from 'vitest';
import { derivedSearchUrl, rulesetQuery, savedSearchUrl, type TradeQuery } from './trade-links';
import { MERC_SOURCES, allRulesets, type MercRuleset } from './rulesets';
import captureQuery from './__fixtures__/capture-query.expected.json';
import WvKGjV8Kfm from './__fixtures__/WvKGjV8Kfm.json';
import LgkKKmllTn from './__fixtures__/LgkKKmllTn.json';
import n5nd22GvKCa from './__fixtures__/5nd22GvKCa.json';
import n7nRvBzl2S5 from './__fixtures__/7nRvBzl2S5.json';
import BgzkZKGQF8 from './__fixtures__/BgzkZKGQF8.json';
import LgkGrPO5Fn from './__fixtures__/LgkGrPO5Fn.json';
import zbrQyEqah4 from './__fixtures__/zbrQyEqah4.json';

/** Keyed by the hash the ruleset declares — the `rulesets.test.ts` idiom. */
const FIXTURES: Record<string, { id: string; query: unknown }> = {
	WvKGjV8Kfm,
	LgkKKmllTn,
	'5nd22GvKCa': n5nd22GvKCa,
	'7nRvBzl2S5': n7nRvBzl2S5,
	BgzkZKGQF8,
	LgkGrPO5Fn,
	zbrQyEqah4
};

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

/**
 * The one normaliser both sides of the round-trip pass through, owned by the
 * test and NOT by the builder.
 *
 * GGG's saved searches are inconsistent about spelling out `disabled: false`:
 * some groups and filters carry it, most just leave the key out, and the two
 * mean the same thing. Dropping it here is the only difference the oracle
 * forgives — every other key survives on both sides, so a `sort` the builder
 * invented, or a group it silently dropped, still fails the comparison. Putting
 * this inside `rulesetQuery` would let the builder launder its own output into
 * agreement with the fixture.
 */
function withoutExplicitFalses(value: unknown): unknown {
	if (Array.isArray(value)) return value.map(withoutExplicitFalses);
	if (value !== null && typeof value === 'object') {
		return Object.fromEntries(
			Object.entries(value as Record<string, unknown>)
				.filter(([key, inner]) => !(key === 'disabled' && inner === false))
				.map(([key, inner]) => [key, withoutExplicitFalses(inner)])
		);
	}
	return value;
}

describe('round-trip normaliser', () => {
	it('drops only an explicit disabled:false, at any depth', () => {
		expect(
			withoutExplicitFalses({
				stats: [
					{ type: 'not', disabled: false, filters: [{ id: 'a', disabled: false }] },
					{ type: 'and', disabled: true, filters: [{ id: 'b', disabled: true }] }
				],
				sort: { price: 'asc' }
			})
		).toEqual({
			stats: [
				{ type: 'not', filters: [{ id: 'a' }] },
				{ type: 'and', disabled: true, filters: [{ id: 'b', disabled: true }] }
			],
			sort: { price: 'asc' }
		});
	});
});

describe('rulesetQuery', () => {
	for (const ruleset of allRulesets()) {
		// The oracle is the saved search itself: the builder walks the typed data
		// model, and what comes out has to be the JSON GGG returned for that hash.
		it(`rebuilds the saved search ${ruleset.savedSearch.hash} from the ${ruleset.id} data model`, () => {
			expect(withoutExplicitFalses(rulesetQuery(ruleset))).toEqual(
				withoutExplicitFalses(FIXTURES[ruleset.savedSearch.hash].query)
			);
		});
	}

	const MANYSHOT = allRulesets().find((r) => r.id === 'guide-a-manyshot') as MercRuleset;
	const MV = allRulesets().find((r) => r.id === 'guide-b-kinetist-mv') as MercRuleset;

	it('switches on an entry the flips name', () => {
		const query = rulesetQuery(MANYSHOT, {
			enable: new Set(['core/mercenary.support_49419'])
		});
		expect(query.stats[1].filters).toEqual([
			{ id: 'mercenary.skill_11495' },
			{ id: 'mercenary.support_5293' },
			{ id: 'mercenary.support_49419' }
		]);
	});

	it('switches off an entry the flips name', () => {
		const query = rulesetQuery(MV, { disable: new Set(['auras/mercenary.skill_52155']) });
		expect(query.stats[5].filters).toEqual([{ id: 'mercenary.skill_52155', disabled: true }]);
	});

	it('flips one group only, leaving the same entry id in its sibling group alone', () => {
		// Return sits in both Manyshot `mercenary` groups; a flip is keyed by group.
		const query = rulesetQuery(MANYSHOT, {
			disable: new Set(['core/mercenary.support_5293'])
		});
		expect(query.stats[1].filters[1]).toEqual({ id: 'mercenary.support_5293', disabled: true });
		expect(query.stats[2].filters[1]).toEqual({ id: 'mercenary.support_5293' });
	});

	it('refuses to flip an entry of a denial group', () => {
		const query = rulesetQuery(MV, {
			disable: new Set(['deny/mercenary.skill_1356', 'deny-supports/mercenary.support_56267'])
		});
		expect(query.stats[0].filters).toEqual([
			{ id: 'mercenary.skill_32089' },
			{ id: 'mercenary.skill_12583' },
			{ id: 'mercenary.skill_26705' }
		]);
		expect(query.stats[4].filters).toEqual([
			{ id: 'mercenary.support_56267' },
			{ id: 'mercenary.support_27970' }
		]);
	});

	it('leaves a parked group parked whatever its entries are flipped to', () => {
		const query = rulesetQuery(MV, {
			enable: new Set(['deny-supports/mercenary.support_56267'])
		});
		expect(query.stats[4].disabled).toBe(true);
	});
});

describe('derivedSearchUrl', () => {
	const MANYSHOT = allRulesets().find((r) => r.id === 'guide-a-manyshot') as MercRuleset;

	it('puts the league in the path and the query in the q parameter', () => {
		const url = derivedSearchUrl('Allflame', rulesetQuery(MANYSHOT));
		expect(new URL(url).pathname).toBe('/trade/search/Allflame');
		const q = new URL(url).searchParams.get('q') ?? '';
		expect(JSON.parse(q).query.status).toEqual({ option: 'securable' });
	});

	it('sends the query inside the request body envelope the trade site reads', () => {
		const url = derivedSearchUrl('Allflame', rulesetQuery(MANYSHOT));
		expect(Object.keys(JSON.parse(new URL(url).searchParams.get('q') ?? ''))).toEqual(['query']);
	});

	it('adds no sort, so the derived search orders like the search it came from', () => {
		const url = derivedSearchUrl('Allflame', rulesetQuery(MANYSHOT));
		expect(JSON.parse(new URL(url).searchParams.get('q') ?? '').sort).toBeUndefined();
	});

	it('percent-encodes a league containing a space', () => {
		const url = derivedSearchUrl('Hardcore Allflame', rulesetQuery(MANYSHOT));
		expect(new URL(url).pathname).toBe('/trade/search/Hardcore%20Allflame');
	});
});

/**
 * The capture path's half of the cross-language parity check.
 *
 * The fixture is what Rust's `build_capture_query` produces for one fixed
 * capture (`__fixtures__/README.md` names it), and `search.rs`'s
 * `the_link_carries_the_shared_fixture_query_under_a_bare_query_envelope`
 * asserts the Rust link against the same file. Both sides read one artifact, so
 * a query shape that changes on one side without the other fails here rather
 * than at the trade site.
 *
 * Typed as `TradeQuery` on purpose — the same type `rulesetQuery` returns. That
 * is the part a compile catches: a filter block Rust sends and the TS type
 * cannot express stops `derivedSearchUrl` from being able to link a captured
 * mercenary at all.
 */
describe('captured-mercenary query parity', () => {
	const query: TradeQuery = captureQuery;

	it('links the query object Rust built without changing it', () => {
		const url = derivedSearchUrl('Allflame', query);
		expect(JSON.parse(new URL(url).searchParams.get('q') ?? '')).toEqual({ query: captureQuery });
	});

	it('keeps every row of the capture as its own group in link order', () => {
		const url = derivedSearchUrl('Allflame', query);
		const linked = JSON.parse(new URL(url).searchParams.get('q') ?? '').query as TradeQuery;
		expect(linked.stats.map((group) => group.filters.map((f) => f.id))).toEqual([
			['skill_a', 'sup_a', 'sup_b1', 'sup_b2'],
			['skill_b', 'sup_greater_chain', 'sup_chain']
		]);
		expect(linked.stats.map((group) => group.value?.min)).toEqual([3, 2]);
	});
});
