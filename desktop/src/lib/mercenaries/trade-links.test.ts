import { describe, it, expect } from 'vitest';
import { derivedSearchUrl, rulesetQuery, savedSearchUrl, type TradeQuery } from './trade-links';
import {
	MERC_SOURCES,
	allRulesets,
	oracleFixture,
	type MercRuleset,
	type MercSource
} from './rulesets';
import captureQuery from './__fixtures__/capture-query.expected.json';
import WvKGjV8Kfm from './__fixtures__/WvKGjV8Kfm.json';
import LgkKKmllTn from './__fixtures__/LgkKKmllTn.json';
import n5nd22GvKCa from './__fixtures__/5nd22GvKCa.json';
import n7nRvBzl2S5 from './__fixtures__/7nRvBzl2S5.json';
import n4mKr0Jbwh9 from './__fixtures__/4mKr0Jbwh9.json';
import BgzkZKGQF8 from './__fixtures__/BgzkZKGQF8.json';
import LgkGrPO5Fn from './__fixtures__/LgkGrPO5Fn.json';
import zbrQyEqah4 from './__fixtures__/zbrQyEqah4.json';
import n4mP3V2jQT9 from './__fixtures__/4mP3V2jQT9.json';
import Z6Em09GmHQ from './__fixtures__/Z6Em09GmHQ.json';
import JBnK2YKRFl from './__fixtures__/JBnK2YKRFl.json';
import d86ymvXRsJ from './__fixtures__/d86ymvXRsJ.json';
import Kld4gv0Pi5 from './__fixtures__/Kld4gv0Pi5.json';
import Kld4gM7yi5 from './__fixtures__/Kld4gM7yi5.json';
import q9l6yK0psg from './__fixtures__/q9l6yK0psg.json';
import OglBJZoQIE from './__fixtures__/OglBJZoQIE.json';
import PPaX7lLqUL from './__fixtures__/PPaX7lLqUL.json';
import n3q6awYZPc5 from './__fixtures__/3q6awYZPc5.json';
import mkgR2DbeS6 from './__fixtures__/mkgR2DbeS6.json';
import jWRDpypkCX from './__fixtures__/jWRDpypkCX.json';
import bGDrZYZaCL from './__fixtures__/bGDrZYZaCL.json';
import guideCKinetist from './__fixtures__/guide-c-kinetist.json';
import guideCManyshot from './__fixtures__/guide-c-manyshot.json';
import guideCBladeAmbusher from './__fixtures__/guide-c-blade-ambusher.json';
import guideCCombatant from './__fixtures__/guide-c-combatant.json';
import guideESniper from './__fixtures__/guide-e-sniper.json';
import guideEKinetist from './__fixtures__/guide-e-kinetist.json';
import guideECombatant from './__fixtures__/guide-e-combatant.json';
import guideEManyshot from './__fixtures__/guide-e-manyshot.json';
import guideECruelMistress from './__fixtures__/guide-e-cruel-mistress.json';
import guideEStormhand from './__fixtures__/guide-e-stormhand.json';
import n8r8JqonVIV from './__fixtures__/8r8JqonVIV.json';
import veYJp9gZhE from './__fixtures__/veYJp9gZhE.json';
import PPGnKVv7UL from './__fixtures__/PPGnKVv7UL.json';
import yYvmr6rjcR from './__fixtures__/yYvmr6rjcR.json';
import d80ePvdvhJ from './__fixtures__/d80ePvdvhJ.json';
import rPogYW44uQ from './__fixtures__/rPogYW44uQ.json';

/** Keyed by the oracle the ruleset declares — the `rulesets.test.ts` idiom. */
const FIXTURES: Record<string, { id: string; query: unknown }> = {
	WvKGjV8Kfm,
	LgkKKmllTn,
	'5nd22GvKCa': n5nd22GvKCa,
	'7nRvBzl2S5': n7nRvBzl2S5,
	'4mKr0Jbwh9': n4mKr0Jbwh9,
	BgzkZKGQF8,
	LgkGrPO5Fn,
	zbrQyEqah4,
	'4mP3V2jQT9': n4mP3V2jQT9,
	Z6Em09GmHQ,
	JBnK2YKRFl,
	d86ymvXRsJ,
	Kld4gv0Pi5,
	Kld4gM7yi5,
	q9l6yK0psg,
	OglBJZoQIE,
	PPaX7lLqUL,
	'3q6awYZPc5': n3q6awYZPc5,
	mkgR2DbeS6,
	jWRDpypkCX,
	bGDrZYZaCL,
	'guide-c-kinetist': guideCKinetist,
	'guide-c-manyshot': guideCManyshot,
	'guide-c-blade-ambusher': guideCBladeAmbusher,
	'guide-c-combatant': guideCCombatant,
	'guide-e-sniper': guideESniper,
	'guide-e-kinetist': guideEKinetist,
	'guide-e-combatant': guideECombatant,
	'guide-e-manyshot': guideEManyshot,
	'guide-e-cruel-mistress': guideECruelMistress,
	'guide-e-stormhand': guideEStormhand,
	'8r8JqonVIV': n8r8JqonVIV,
	veYJp9gZhE,
	PPGnKVv7UL,
	yYvmr6rjcR,
	d80ePvdvhJ,
	rPogYW44uQ
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

	it('links each GGG-saved ruleset to its own saved search', () => {
		const saved = allRulesets().filter((ruleset) => ruleset.savedSearch !== undefined);
		expect(saved.map((r) => searchPathSegments(savedSearchUrl(r.savedSearch!)))).toEqual(
			saved.map((r) => r.savedSearch)
		);
	});
});

/**
 * The other half of that sweep: guide-c's and guide-e's rulesets are transcribed
 * from PROSE, so there is no saved search to link and nothing may render an
 * "open saved search" for them. The DERIVED link is unaffected — `rulesetQuery`
 * builds from the data model and never needs a hash — which is the whole reason
 * the two addressing schemes live in different fields instead of one being a
 * hash nobody can fetch.
 */
describe('rulesets transcribed from prose', () => {
	const AUTHORED = allRulesets().filter((ruleset) => ruleset.authored !== undefined);
	const SAVED = allRulesets().filter((ruleset) => ruleset.savedSearch !== undefined);

	it('names the ten authored rulesets, and only those', () => {
		expect(AUTHORED.map((r) => r.id)).toEqual([
			'guide-c-kinetist',
			'guide-c-manyshot',
			'guide-c-blade-ambusher',
			'guide-c-combatant',
			'guide-e-sniper',
			'guide-e-kinetist',
			'guide-e-combatant',
			'guide-e-manyshot',
			'guide-e-cruel-mistress',
			'guide-e-stormhand'
		]);
	});

	// The positive control the assertion above needs: the other twenty-eight
	// rulesets DO carry a hash, so an empty authored list would not be the two
	// sides agreeing that nothing is authored.
	it('leaves the twenty-eight saved searches addressable by hash', () => {
		expect(SAVED.length).toBe(28);
		expect(SAVED.every((r) => r.authored === undefined)).toBe(true);
	});

	it("carries an authored ruleset's built query through the derived link's q parameter", () => {
		const linked = AUTHORED.map((ruleset) => {
			const url = derivedSearchUrl('Allflame', rulesetQuery(ruleset));
			return JSON.parse(new URL(url).searchParams.get('q') ?? '').query;
		});
		expect(linked).toEqual(AUTHORED.map((ruleset) => rulesetQuery(ruleset)));
	});
});

describe('source guide links', () => {
	// The first three identified 2026-08-26, guide D 2026-08-28, guides E and F
	// 2026-09-04. Guide A's, guide B's and guide D's saved-search hashes are
	// exactly the trade links on that page / in those video descriptions; guide
	// B's URL is the CHANNEL, because its ladders come from different videos of
	// it and pointing the source at one would misattribute the others' links,
	// while guide D's is a single VIDEO because both of its rungs came out of
	// that one. Guide C's page publishes no links at all — the URL is where the
	// PROSE is, so a reader can re-check the transcription against the sentences
	// it came from. Guide E's is the same idea one step further out: the note is
	// an IMAGE embedded in cell E14 of Sheet1 of that spreadsheet, so the URL is
	// where the picture is and the only re-check anyone has. Guide F's page
	// carries all six of its links but answers 403 to this repo, so its URL is
	// the only re-check a reader has and a stale one would leave six searches
	// unattributable.
	it('points each source at the page its rules were taken from', () => {
		expect(MERC_SOURCES.map((s) => [s.id, s.guideUrl])).toEqual([
			['guide-a', 'https://wealthyexile.com/strategies/7062/alchgo_astrolabe__merc_boss_rushing'],
			['guide-b', 'https://www.youtube.com/channel/UCqIRIXItoDOlET2oeFn6WKA'],
			['guide-c', 'https://mobalytics.gg/poe/builds/captainlance9-luminary-merc-bot'],
			['guide-d', 'https://www.youtube.com/watch?v=LXoJCRmUaJI'],
			[
				'guide-e',
				'https://docs.google.com/spreadsheets/d/1EW1JIew9A08RDmZbtWOcLzo3WEokexMdOlldXwRF34Q/htmlview'
			],
			[
				'guide-f',
				'https://mobalytics.gg/poe/builds/mercenary-support-luminary-path-of-evening'
			]
		]);
	});
});

const GUIDE_B = MERC_SOURCES.find((s) => s.id === 'guide-b') as MercSource;

describe('per-ruleset guide URLs', () => {
	// The source URL is the CHANNEL; each rung names the video its link came from.
	// Not one video per ladder: the Combatant video publishes the Frost Blades and
	// the Wild Strike links together, so nine rungs share `45aM9242Umo`.
	it('points every guide-b rung at the video whose description published it', () => {
		expect(GUIDE_B.rulesets.map((r) => `${r.id} ${r.guideUrl ?? 'none'}`)).toEqual([
			'guide-b-kinetist-mv https://www.youtube.com/watch?v=HKTVN4sENvg',
			'guide-b-kinetist-mid https://www.youtube.com/watch?v=HKTVN4sENvg',
			'guide-b-kinetist-end https://www.youtube.com/watch?v=HKTVN4sENvg',
			'guide-b-kinetist-gg https://www.youtube.com/watch?v=HKTVN4sENvg',
			'guide-b-manyshot-mv https://www.youtube.com/watch?v=ljaXlGLdyxM',
			'guide-b-manyshot-mid https://www.youtube.com/watch?v=ljaXlGLdyxM',
			'guide-b-manyshot-end https://www.youtube.com/watch?v=ljaXlGLdyxM',
			'guide-b-manyshot-gg https://www.youtube.com/watch?v=ljaXlGLdyxM',
			'guide-b-frost-blades-mv https://www.youtube.com/watch?v=45aM9242Umo',
			'guide-b-frost-blades-mid https://www.youtube.com/watch?v=45aM9242Umo',
			'guide-b-frost-blades-end-noreturn https://www.youtube.com/watch?v=45aM9242Umo',
			'guide-b-frost-blades-end-return https://www.youtube.com/watch?v=45aM9242Umo',
			'guide-b-frost-blades-gg https://www.youtube.com/watch?v=45aM9242Umo',
			'guide-b-wild-strike-mv https://www.youtube.com/watch?v=45aM9242Umo',
			'guide-b-wild-strike-mid https://www.youtube.com/watch?v=45aM9242Umo',
			'guide-b-wild-strike-end https://www.youtube.com/watch?v=45aM9242Umo',
			'guide-b-wild-strike-gg https://www.youtube.com/watch?v=45aM9242Umo'
		]);
	});

	// Guide A's, guide C's, guide D's, guide E's and guide F's rulesets each come
	// off ONE page, video or image, so they inherit the source URL rather than
	// repeating it per ruleset. Guide E is six archetypes in one screenshot and
	// guide F six searches on one page — the most this can carry and still be
	// true.
	it('leaves a one-page source’s rulesets without a URL of their own', () => {
		const onePage = ['guide-a', 'guide-c', 'guide-d', 'guide-e', 'guide-f'].flatMap(
			(id) => (MERC_SOURCES.find((s) => s.id === id) as MercSource).rulesets
		);
		expect(onePage.map((r) => `${r.id} ${r.guideUrl ?? 'inherits the source URL'}`)).toEqual([
			'guide-a-manyshot inherits the source URL',
			'guide-a-kinetist-v1 inherits the source URL',
			'guide-a-combatant inherits the source URL',
			'guide-c-kinetist inherits the source URL',
			'guide-c-manyshot inherits the source URL',
			'guide-c-blade-ambusher inherits the source URL',
			'guide-c-combatant inherits the source URL',
			'guide-d-kinetist-budget inherits the source URL',
			'guide-d-kinetist-20d inherits the source URL',
			'guide-e-sniper inherits the source URL',
			'guide-e-kinetist inherits the source URL',
			'guide-e-combatant inherits the source URL',
			'guide-e-manyshot inherits the source URL',
			'guide-e-cruel-mistress inherits the source URL',
			'guide-e-stormhand inherits the source URL',
			'guide-f-manyshot-cheap inherits the source URL',
			'guide-f-manyshot-expensive inherits the source URL',
			'guide-f-combatant-cheap inherits the source URL',
			'guide-f-combatant-expensive inherits the source URL',
			'guide-f-kinetist-cheap inherits the source URL',
			'guide-f-kinetist-expensive inherits the source URL'
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
		// For the twenty-eight saved searches the oracle is GGG's own JSON: the builder
		// walks the typed data model, and what comes out has to be the response
		// returned for that hash. For guide-c's four and guide-e's six the fixture
		// is this builder's own output — a weaker check, and the only one
		// available, since there is no saved search to disagree with (see
		// `__fixtures__/README.md`).
		it(`rebuilds ${oracleFixture(ruleset)} from the ${ruleset.id} data model`, () => {
			expect(withoutExplicitFalses(rulesetQuery(ruleset))).toEqual(
				withoutExplicitFalses(FIXTURES[oracleFixture(ruleset)].query)
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

	it('keeps the and group and both count groups in link order', () => {
		const url = derivedSearchUrl('Allflame', query);
		const linked = JSON.parse(new URL(url).searchParams.get('q') ?? '').query as TradeQuery;
		expect(linked.stats.map((group) => group.type)).toEqual(['and', 'count', 'count']);
		expect(linked.stats.map((group) => group.filters.map((f) => f.id))).toEqual([
			['skill_a', 'sup_a', 'skill_b'],
			['sup_b1', 'sup_b2'],
			['sup_greater_chain', 'sup_chain']
		]);
		expect(linked.stats.map((group) => group.value?.min)).toEqual([undefined, 1, 1]);
	});
});
