import { describe, it, expect } from 'vitest';
import {
	MERC_SOURCES,
	SOURCE_IDS,
	TIERS,
	allRulesets,
	ladders,
	entryKind,
	entryRole,
	entryTier,
	type MercFilterEntry,
	type MercFilterGroup,
	type MercRuleset,
	type MercSource
} from './rulesets';
import WvKGjV8Kfm from './__fixtures__/WvKGjV8Kfm.json';
import LgkKKmllTn from './__fixtures__/LgkKKmllTn.json';
import n5nd22GvKCa from './__fixtures__/5nd22GvKCa.json';
import n7nRvBzl2S5 from './__fixtures__/7nRvBzl2S5.json';
import BgzkZKGQF8 from './__fixtures__/BgzkZKGQF8.json';
import LgkGrPO5Fn from './__fixtures__/LgkGrPO5Fn.json';
import zbrQyEqah4 from './__fixtures__/zbrQyEqah4.json';
import n4mP3V2jQT9 from './__fixtures__/4mP3V2jQT9.json';
import Z6Em09GmHQ from './__fixtures__/Z6Em09GmHQ.json';
import JBnK2YKRFl from './__fixtures__/JBnK2YKRFl.json';
import d86ymvXRsJ from './__fixtures__/d86ymvXRsJ.json';
import mercenaryStats from './__fixtures__/mercenary-stats.json';

/**
 * The saved-search JSON as GGG returns it. TypeScript infers a per-file literal
 * type from each import (unions of "has `disabled`" and "doesn't"), so each one is
 * widened through `unknown` into this single shape — the room-presets.ts idiom.
 */
interface RawFilter {
	id: string;
	disabled?: boolean;
}
interface RawStatGroup {
	type: string;
	value?: { min?: number };
	disabled?: boolean;
	filters: RawFilter[];
}
interface RawSavedSearch {
	id: string;
	query: {
		stats: RawStatGroup[];
		status: { option: string };
		/** Absent on the guide-b Manyshot searches: they set no item-level floor. */
		filters?: { misc_filters?: { filters: { ilvl: { min: number } } } };
	};
}

/**
 * Keyed by the hash the ruleset declares, NOT by the hash inside the file — that
 * is the point of the fixture-identity test below: a file copied to the wrong name
 * fails there instead of silently validating a ruleset against another's search.
 */
const FIXTURES: Record<string, RawSavedSearch> = {
	WvKGjV8Kfm: WvKGjV8Kfm as unknown as RawSavedSearch,
	LgkKKmllTn: LgkKKmllTn as unknown as RawSavedSearch,
	'5nd22GvKCa': n5nd22GvKCa as unknown as RawSavedSearch,
	'7nRvBzl2S5': n7nRvBzl2S5 as unknown as RawSavedSearch,
	BgzkZKGQF8: BgzkZKGQF8 as unknown as RawSavedSearch,
	LgkGrPO5Fn: LgkGrPO5Fn as unknown as RawSavedSearch,
	zbrQyEqah4: zbrQyEqah4 as unknown as RawSavedSearch,
	'4mP3V2jQT9': n4mP3V2jQT9 as unknown as RawSavedSearch,
	Z6Em09GmHQ: Z6Em09GmHQ as unknown as RawSavedSearch,
	JBnK2YKRFl: JBnK2YKRFl as unknown as RawSavedSearch,
	d86ymvXRsJ: d86ymvXRsJ as unknown as RawSavedSearch
};

/** GGG's Mercenary stat vocabulary: stat id -> display text. */
const VOCABULARY = new Map(
	(mercenaryStats as { entries: { id: string; text: string }[] }).entries.map((e) => [e.id, e.text])
);

/** Everything the transcription is allowed to claim about a saved search. */
function fromRuleset(ruleset: MercRuleset) {
	return {
		hash: ruleset.savedSearch.hash,
		status: ruleset.status,
		ilvlMin: ruleset.ilvlMin,
		groups: ruleset.groups.map((group) => ({
			type: group.type,
			enabled: group.enabledInSearch,
			min: group.min ?? null,
			entries: group.entries.map((entry) => ({ id: entry.id, enabled: entry.enabledInSearch }))
		}))
	};
}

function fromFixture(fixture: RawSavedSearch) {
	return {
		hash: fixture.id,
		status: fixture.query.status.option,
		ilvlMin: fixture.query.filters?.misc_filters?.filters.ilvl.min,
		groups: fixture.query.stats.map((group) => ({
			type: group.type,
			enabled: group.disabled !== true,
			min: group.value?.min ?? null,
			entries: group.filters.map((filter) => ({ id: filter.id, enabled: filter.disabled !== true }))
		}))
	};
}

function rulesetById(id: string): MercRuleset {
	const found = allRulesets().find((r) => r.id === id);
	if (!found) throw new Error(`no ruleset declared with id ${id}`);
	return found;
}

const GUIDE_B = MERC_SOURCES.find((s) => s.id === 'guide-b') as MercSource;

/** The rungs of one guide-b ladder, cheapest first. */
function ladderNamed(key: string): MercRuleset[] {
	const found = ladders(GUIDE_B).find((rungs) => rungs[0].ladder === key);
	if (!found) throw new Error(`guide-b declares no ladder ${key}`);
	return found;
}

function groupOf(ruleset: MercRuleset, groupId: string): MercFilterGroup | undefined {
	return ruleset.groups.find((g) => g.id === groupId);
}

function entryOf(
	ruleset: MercRuleset,
	groupId: string,
	entryId: string
): MercFilterEntry | undefined {
	return groupOf(ruleset, groupId)?.entries.find((e) => e.id === entryId);
}

describe('saved-search transcription', () => {
	for (const ruleset of allRulesets()) {
		describe(ruleset.id, () => {
			it('reads the fixture that carries its own declared hash', () => {
				expect(FIXTURES[ruleset.savedSearch.hash]?.id).toBe(ruleset.savedSearch.hash);
			});

			it('matches the saved search group for group and entry for entry', () => {
				expect(fromRuleset(ruleset)).toEqual(fromFixture(FIXTURES[ruleset.savedSearch.hash]));
			});
		});
	}
});

describe('stat vocabulary', () => {
	it('names every entry with the GGG vocabulary text for its id', () => {
		const occurrences = allRulesets().flatMap((ruleset) =>
			ruleset.groups.flatMap((group) =>
				group.entries.map((entry) => ({ where: `${ruleset.id}/${group.id}`, entry }))
			)
		);
		const declared = occurrences.map((o) => `${o.where} ${o.entry.id} = ${o.entry.name}`);
		const vocabulary = occurrences.map(
			(o) =>
				`${o.where} ${o.entry.id} = ${VOCABULARY.get(o.entry.id) ?? '<id absent from vocabulary>'}`
		);
		expect(declared).toEqual(vocabulary);
	});
});

describe('derived entry facts', () => {
	it('reads a support role off the support id prefix', () => {
		expect(entryRole('mercenary.support_5293')).toBe('support');
	});

	it('reads a skill role off the skill id prefix', () => {
		expect(entryRole('mercenary.skill_11495')).toBe('skill');
	});

	it('rejects a stat id that is neither a mercenary skill nor a support', () => {
		expect(() => entryRole('explicit.mod_1234')).toThrow(/explicit\.mod_1234/);
	});

	it('parses the tier out of a support name', () => {
		expect(entryTier('Greater Multiple Projectiles (Tier 3)')).toBe(3);
	});

	it('parses the lowest tier as 1, not as absent', () => {
		expect(entryTier('Multiple Projectiles (Tier 1)')).toBe(1);
	});

	it('reports no tier for an active skill name', () => {
		expect(entryTier('Kinetic Blast of Clustering')).toBeNull();
	});

	it('reports no tier when the suffix is not parenthesised', () => {
		expect(entryTier('Pierce Tier 2')).toBeNull();
	});

	it('gives every support entry a tier and no skill entry one', () => {
		const mismatched = allRulesets().flatMap((ruleset) =>
			ruleset.groups.flatMap((group) =>
				group.entries
					.filter(
						(entry) => (entryRole(entry.id) === 'support') !== (entryTier(entry.name) !== null)
					)
					.map((entry) => `${ruleset.id}/${group.id} ${entry.id} "${entry.name}"`)
			)
		);
		expect(mismatched).toEqual([]);
	});
});

describe('entry kind', () => {
	function kindOf(ruleset: MercRuleset, groupId: string, entryId: string): string {
		const group = groupOf(ruleset, groupId);
		const entry = entryOf(ruleset, groupId, entryId);
		if (!group || !entry) throw new Error(`${ruleset.id} has no ${groupId}/${entryId}`);
		return entryKind(group, entry);
	}

	it('keeps a switched-off deny group forbidding stats another live group requires', () => {
		const mv = rulesetById('guide-b-kinetist-mv');
		const pierce = ['mercenary.support_56267', 'mercenary.support_27970'];
		expect(
			pierce.map(
				(id) =>
					`${id} deny-supports=${kindOf(mv, 'deny-supports', id)} behavior=${kindOf(mv, 'behavior', id)}`
			)
		).toEqual([
			'mercenary.support_56267 deny-supports=forbidden behavior=required',
			'mercenary.support_27970 deny-supports=forbidden behavior=required'
		]);
	});

	it('reads a switched-off entry of a live requiring group as a bonus', () => {
		expect(kindOf(rulesetById('guide-a-manyshot'), 'core', 'mercenary.support_49419')).toBe('bonus');
	});
});

describe('required-versus-denied invariant', () => {
	/**
	 * A stat may appear in both a requiring group and a `not` group as long as one
	 * of the two is switched off — that is how these searches park vocabulary they
	 * are not currently using. What must never happen is a search that requires and
	 * denies the same stat at the same time: it can never match anything.
	 */
	function liveConflicts(ruleset: MercRuleset): string[] {
		const denied = new Set(
			ruleset.groups
				.filter((g) => g.type === 'not' && g.enabledInSearch)
				.flatMap((g) => g.entries.filter((e) => e.enabledInSearch).map((e) => e.id))
		);
		return ruleset.groups
			.filter((g) => g.type !== 'not' && g.enabledInSearch)
			.flatMap((g) =>
				g.entries
					.filter((e) => e.enabledInSearch && denied.has(e.id))
					.map((e) => `${ruleset.id}/${g.id}/${e.id}`)
			);
	}

	/**
	 * Positive control for the sweep below: that sweep is green against a
	 * `liveConflicts` that never reports anything, because the real rulesets are
	 * clean. This synthetic one is the case the helper exists to catch.
	 */
	it('reports the group and entry path of a stat one ruleset requires and denies at once', () => {
		const selfDefeating: MercRuleset = {
			id: 'synthetic-conflict',
			label: 'Synthetic',
			archetype: 'kinetist',
			savedSearch: { league: 'Allflame', hash: 'synthetic' },
			status: 'securable',
			groups: [
				{
					id: 'core',
					label: 'Core skill + links',
					type: 'mercenary',
					enabledInSearch: true,
					entries: [{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }]
				},
				{
					id: 'deny-supports',
					label: 'Denied support links',
					type: 'not',
					enabledInSearch: true,
					entries: [{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }]
				}
			]
		};
		expect(liveConflicts(selfDefeating)).toEqual(['synthetic-conflict/core/mercenary.support_5293']);
	});

	it('never requires and denies the same stat inside one ruleset', () => {
		expect(allRulesets().flatMap(liveConflicts)).toEqual([]);
	});

	it('parks Wild Strike switched off in the Combatant group whose deny list forbids it', () => {
		const combatant = rulesetById('guide-a-combatant');
		expect(entryOf(combatant, 'secondary', 'mercenary.skill_40957')?.enabledInSearch).toBe(false);
		expect(entryOf(combatant, 'deny', 'mercenary.skill_40957')?.enabledInSearch).toBe(true);
	});

	it('carries the Pierce pair in both Kinetist groups, live on at most one side per rung', () => {
		const pair = ['mercenary.support_56267', 'mercenary.support_27970'];
		const rows = ladderNamed('kinetist').flatMap((ruleset) =>
			pair.map((id) => {
				const required =
					groupOf(ruleset, 'behavior')?.enabledInSearch === true &&
					entryOf(ruleset, 'behavior', id)?.enabledInSearch === true;
				const denied =
					groupOf(ruleset, 'deny-supports')?.enabledInSearch === true &&
					entryOf(ruleset, 'deny-supports', id)?.enabledInSearch === true;
				return `${ruleset.tier} ${id} required=${required} denied=${denied}`;
			})
		);
		expect(rows).toEqual([
			'mv mercenary.support_56267 required=true denied=false',
			'mv mercenary.support_27970 required=true denied=false',
			'mid mercenary.support_56267 required=false denied=true',
			'mid mercenary.support_27970 required=false denied=true',
			'end mercenary.support_56267 required=false denied=true',
			'end mercenary.support_27970 required=false denied=true',
			'gg mercenary.support_56267 required=false denied=true',
			'gg mercenary.support_27970 required=false denied=true'
		]);
	});
});

describe('guide-a rulesets', () => {
	const GUIDE_A_GROUP_IDS: Record<string, string[]> = {
		'guide-a-manyshot': ['auras', 'core', 'secondary', 'deny'],
		'guide-a-kinetist-v1': ['auras', 'core', 'deny'],
		'guide-a-combatant': ['auras', 'core', 'secondary', 'deny']
	};

	it('gives each ruleset the group ids its saved search is transcribed under', () => {
		const guideA = MERC_SOURCES.find((s) => s.id === 'guide-a')?.rulesets ?? [];
		expect(Object.fromEntries(guideA.map((r) => [r.id, r.groups.map((g) => g.id)]))).toEqual(
			GUIDE_A_GROUP_IDS
		);
	});
});

describe('group entry keys', () => {
	// The page keys its chip {#each} on entry.id alone — the same id may recur
	// across sibling groups (Return does, in Manyshot), but a duplicate WITHIN
	// one group would make Svelte's keyed each throw at runtime. Transcription
	// fidelity would faithfully copy such a duplicate out of a future fixture,
	// so uniqueness needs its own gate here.
	it('keeps entry ids unique within every group of every ruleset', () => {
		for (const ruleset of allRulesets()) {
			for (const group of ruleset.groups) {
				const ids = group.entries.map((e) => e.id);
				expect(new Set(ids).size, `${ruleset.id}/${group.id} repeats an entry id`).toBe(
					ids.length
				);
			}
		}
	});
});

describe('Kinetist ladder', () => {
	const KINETIST_GROUP_IDS = [
		'deny',
		'core',
		'secondary',
		'behavior',
		'deny-supports',
		'auras',
		'damage'
	];

	/**
	 * The only differences the four rungs are allowed to have, taken from the POE-165
	 * tier table. Values are indexed by rung, cheapest first: [mv, mid, end, gg].
	 */
	const DECLARED_DELTAS: Record<string, unknown[]> = {
		// Greater Multiple Projectiles on the core skill — a GG-only luxury.
		'core/mercenary.support_49419.enabled': [false, false, false, true],
		// Barrage is an acceptable second skill only while mapping.
		'secondary/mercenary.skill_1356.enabled': [true, false, false, false],
		// Pierce is wanted at MV and actively rejected from Mid up.
		'behavior/mercenary.support_56267.enabled': [true, false, false, false],
		'behavior/mercenary.support_27970.enabled': [true, false, false, false],
		'deny-supports.enabled': [false, true, true, true],
		// Haste is not checked at Mid.
		'auras.enabled': [true, false, true, true],
		'damage.min': [2, 2, 3, 2]
	};

	function flatten(ruleset: MercRuleset): Record<string, unknown> {
		const flat: Record<string, unknown> = {};
		for (const group of ruleset.groups) {
			flat[`${group.id}.enabled`] = group.enabledInSearch;
			flat[`${group.id}.min`] = group.min ?? null;
			for (const entry of group.entries) {
				flat[`${group.id}/${entry.id}.enabled`] = entry.enabledInSearch;
			}
		}
		return flat;
	}

	it('lists the four rungs cheapest first', () => {
		expect(ladderNamed('kinetist').map((r) => r.tier)).toEqual([...TIERS]);
	});

	it('gives every rung the same group ids in the saved-search order', () => {
		expect(ladderNamed('kinetist').map((r) => r.groups.map((g) => g.id))).toEqual(
			ladderNamed('kinetist').map(() => KINETIST_GROUP_IDS)
		);
	});

	it('gives every rung the same entry ids in every group', () => {
		const perGroupIds = (ruleset: MercRuleset) => ruleset.groups.map((g) => g.entries.map((e) => e.id));
		const [first, ...rest] = ladderNamed('kinetist');
		expect(rest.map(perGroupIds)).toEqual(rest.map(() => perGroupIds(first)));
	});

	it('differs between rungs only where the tier table says it does', () => {
		const flattened = ladderNamed('kinetist').map(flatten);
		const keys = [...new Set(flattened.flatMap((f) => Object.keys(f)))].sort();
		const actual: Record<string, unknown[]> = {};
		for (const key of keys) {
			const values = flattened.map((f) => f[key]);
			if (values.some((v) => v !== values[0])) actual[key] = values;
		}
		expect(actual).toEqual(DECLARED_DELTAS);
	});
});

/**
 * The Manyshot ladder is NOT written as one skeleton with switches: its GG rung
 * is a differently shaped search, and pretending otherwise would make the matrix
 * borrow a neighbouring rung's state for the slots GG does not have. So what is
 * pinned here is the group-id MAPPING — the one thing the fixture-fidelity tests
 * above cannot see, because group ids exist only in this file.
 */
describe('Manyshot ladder', () => {
	/** `<groupId> <type> enabled=<bool> min=<n|->`, in the saved search's own order. */
	function shape(ruleset: MercRuleset): string[] {
		return ruleset.groups.map(
			(g) => `${g.id} ${g.type} enabled=${g.enabledInSearch} min=${g.min ?? '-'}`
		);
	}

	it('lists the four rungs cheapest first', () => {
		expect(ladderNamed('manyshot').map((r) => r.id)).toEqual([
			'guide-b-manyshot-mv',
			'guide-b-manyshot-mid',
			'guide-b-manyshot-end',
			'guide-b-manyshot-gg'
		]);
	});

	// Earlygame and Midgame differ by exactly two things — Midgame switches the
	// Vaal-Ice-Shot-plus-Return group on and adds the aura group.
	it('opens the Earlygame rung with every mercenary group parked', () => {
		expect(shape(rulesetById('guide-b-manyshot-mv'))).toEqual([
			'has-vaal and enabled=true min=-',
			'deny not enabled=true min=-',
			'vaal-damage mercenary enabled=false min=2',
			'vaal-return mercenary enabled=false min=2',
			'core mercenary enabled=false min=-',
			'projectiles mercenary enabled=false min=2',
			'damage mercenary enabled=false min=2'
		]);
	});

	it('pins the Midgame rung shape: Earlygame plus a live Vaal-Return group, plus auras', () => {
		expect(shape(rulesetById('guide-b-manyshot-mid'))).toEqual([
			'has-vaal and enabled=true min=-',
			'deny not enabled=true min=-',
			'vaal-damage mercenary enabled=false min=2',
			'vaal-return mercenary enabled=true min=2',
			'core mercenary enabled=false min=-',
			'projectiles mercenary enabled=false min=2',
			'damage mercenary enabled=false min=2',
			'auras count enabled=true min=1'
		]);
	});

	it('pins the Endgame rung shape: the Ice Shot groups live, projectiles lifted to 3', () => {
		expect(shape(rulesetById('guide-b-manyshot-end'))).toEqual([
			'has-vaal and enabled=true min=-',
			'deny not enabled=true min=-',
			'vaal-damage mercenary enabled=true min=2',
			'vaal-return mercenary enabled=true min=2',
			'core mercenary enabled=false min=2',
			'projectiles mercenary enabled=true min=3',
			'damage mercenary enabled=true min=2',
			'auras count enabled=true min=1'
		]);
	});

	// The drift, transcribed rather than tidied: auras lead, the `and`
	// Vaal-Ice-Shot gate is gone, the projectile and damage vocabularies are one
	// parked `min: 4` group, and `core` is last and live.
	it('reshapes the search at GG instead of switching the same groups', () => {
		expect(shape(rulesetById('guide-b-manyshot-gg'))).toEqual([
			'auras count enabled=true min=1',
			'deny not enabled=true min=-',
			'vaal-damage mercenary enabled=true min=3',
			'vaal-return mercenary enabled=true min=2',
			'projectiles mercenary enabled=false min=4',
			'core mercenary enabled=true min=-'
		]);
	});

	it('carries the damage vocabulary inside the GG rung’s merged projectiles group', () => {
		expect(groupOf(rulesetById('guide-b-manyshot-gg'), 'projectiles')?.entries.map((e) => e.id)).toEqual([
			'mercenary.skill_11495',
			'mercenary.support_49419',
			'mercenary.support_32052',
			'mercenary.support_31052',
			'mercenary.support_44886',
			'mercenary.support_28416',
			'mercenary.support_38571',
			'mercenary.support_53145'
		]);
	});

	// Every other rung parks Return inside this group; GG leaves the entry out.
	it('drops the parked Return entry from the GG rung’s Vaal damage group', () => {
		expect(
			ladderNamed('manyshot').map(
				(rung) =>
					`${rung.tier} ${entryOf(rung, 'vaal-damage', 'mercenary.support_5293') === undefined ? 'absent' : 'declared'}`
			)
		).toEqual(['mv declared', 'mid declared', 'end declared', 'gg absent']);
	});

	// The aura group is the reason the matrix skeleton is a union: the cheapest
	// rung has no such group, and the option list grows up the ladder.
	it('grows the aura options up the ladder, from a rung that has none', () => {
		expect(
			ladderNamed('manyshot').map(
				(rung) => `${rung.tier} ${groupOf(rung, 'auras')?.entries.map((e) => e.name).join('/') ?? 'no aura group'}`
			)
		).toEqual([
			'mv no aura group',
			'mid Grace/Haste/Hatred',
			'end Grace/Hatred',
			'gg Hatred/Grace/Frost Bomb'
		]);
	});

	it('sets no item-level floor on any rung, unlike every other saved search', () => {
		expect(ladderNamed('manyshot').map((r) => r.ilvlMin)).toEqual([
			undefined,
			undefined,
			undefined,
			undefined
		]);
	});
});

describe('ladder keys', () => {
	it('groups guide-b into its two ladders, first-declared first', () => {
		expect(ladders(GUIDE_B).map((rungs) => rungs.map((r) => r.id))).toEqual([
			[
				'guide-b-kinetist-mv',
				'guide-b-kinetist-mid',
				'guide-b-kinetist-end',
				'guide-b-kinetist-gg'
			],
			[
				'guide-b-manyshot-mv',
				'guide-b-manyshot-mid',
				'guide-b-manyshot-end',
				'guide-b-manyshot-gg'
			]
		]);
	});

	it('reports no ladders for a source whose rulesets carry no tier', () => {
		expect(ladders(MERC_SOURCES.find((s) => s.id === 'guide-a') as MercSource)).toEqual([]);
	});

	/** Tiered rulesets carrying no ladder key — rungs of nothing. */
	function orphanRungs(rulesets: MercRuleset[]): string[] {
		return rulesets.filter((r) => r.tier !== undefined && r.ladder === undefined).map((r) => r.id);
	}

	/**
	 * Positive control for the sweep below: that sweep is green against an
	 * `orphanRungs` that never reports anything, because every real tiered
	 * ruleset carries a key. This synthetic one is the case the helper exists
	 * to catch.
	 */
	it('reports the id of a ruleset that carries a tier without a ladder key', () => {
		const orphan: MercRuleset = {
			id: 'synthetic-orphan',
			label: 'Synthetic',
			archetype: 'kinetist',
			tier: 'gg',
			savedSearch: { league: 'Allflame', hash: 'synthetic' },
			status: 'securable',
			groups: []
		};
		expect(orphanRungs([orphan])).toEqual(['synthetic-orphan']);
	});

	// A rung with a tier but no ladder key has no column set to be compared in:
	// `ladders()` would drop it and the page would silently render it as a card.
	it('gives every tiered ruleset a ladder key', () => {
		expect(orphanRungs(allRulesets())).toEqual([]);
	});

	// Nerotox's Combatant video publishes a Frost Blades ladder AND a Wild Strike
	// ladder — same archetype, two searches. Keying the grouping on the archetype
	// would fuse them into one matrix whose columns come from two different
	// searches, and no real data can catch that yet: guide-b's two ladders happen
	// to have one archetype each.
	it('keeps two ladders of the same archetype apart', () => {
		const rung = (id: string, ladder: string, tier: MercRuleset['tier']): MercRuleset => ({
			id,
			label: ladder,
			archetype: 'combatant',
			ladder,
			tier,
			savedSearch: { league: 'Allflame', hash: id },
			status: 'securable',
			groups: []
		});
		const source: MercSource = {
			id: 'guide-b',
			label: 'Synthetic',
			guideUrl: null,
			rulesets: [
				rung('frost-mv', 'frost-blades', 'mv'),
				rung('wild-mv', 'wild-strike', 'mv'),
				rung('frost-gg', 'frost-blades', 'gg')
			]
		};
		expect(ladders(source).map((rungs) => rungs.map((r) => r.id))).toEqual([
			['frost-mv', 'frost-gg'],
			['wild-mv']
		]);
	});

	it('orders a ladder’s rungs by TIERS even when they are declared out of order', () => {
		const rung = (tier: MercRuleset['tier']): MercRuleset => ({
			id: `synthetic-${tier}`,
			label: 'Synthetic',
			archetype: 'combatant',
			ladder: 'synthetic',
			tier,
			savedSearch: { league: 'Allflame', hash: `synthetic-${tier}` },
			status: 'securable',
			groups: []
		});
		const source: MercSource = {
			id: 'guide-b',
			label: 'Synthetic',
			guideUrl: null,
			rulesets: [rung('gg'), rung('mv'), rung('end'), rung('mid')]
		};
		expect(ladders(source)[0].map((r) => r.tier)).toEqual(['mv', 'mid', 'end', 'gg']);
	});

	// Nerotox's Combatant video publishes two Endgame links for one ladder, so
	// the ranking must not assume one rung per tier — and the tie has to resolve
	// to the guide's own order rather than to whichever the sort happens to move.
	it('keeps two rungs sharing a tier in declaration order', () => {
		const rung = (id: string, tier: MercRuleset['tier']): MercRuleset => ({
			id,
			label: 'Synthetic',
			archetype: 'combatant',
			ladder: 'synthetic',
			tier,
			savedSearch: { league: 'Allflame', hash: id },
			status: 'securable',
			groups: []
		});
		const source: MercSource = {
			id: 'guide-b',
			label: 'Synthetic',
			guideUrl: null,
			rulesets: [rung('end-no-return', 'end'), rung('end-return', 'end'), rung('mv', 'mv')]
		};
		expect(ladders(source)[0].map((r) => r.id)).toEqual(['mv', 'end-no-return', 'end-return']);
	});
});

describe('author notes', () => {
	// Verbatim from the two video descriptions — the app never paraphrases them.
	it('carries each GG rung’s note exactly as its author wrote it', () => {
		expect(
			allRulesets()
				.filter((r) => r.authorNote !== undefined)
				.map((r) => [r.id, r.authorNote])
		).toEqual([
			[
				'guide-b-kinetist-gg',
				'look at damage links still - these are 4L not even 5L. 5L mercs nearly never exist for KB in gg setups, a 5L with barrage can beat a 4L with greater KB'
			],
			['guide-b-manyshot-gg', 'manually check for clear links on ice shot']
		]);
	});
});

describe('source registry', () => {
	it('declares exactly the sources named in SOURCE_IDS, in that order', () => {
		expect(MERC_SOURCES.map((s) => s.id)).toEqual([...SOURCE_IDS]);
	});

	it('gives every ruleset an id of its own', () => {
		const ids = allRulesets().map((r) => r.id);
		expect([...new Set(ids)]).toEqual(ids);
	});
});

describe('buyer-contextual entries', () => {
	/**
	 * `buyerContextual` is Sebastian's selling-side ruling, not something the
	 * saved searches say — the transcription tests above cannot see it drift,
	 * because from the fixture's point of view these entries are ordinary enabled
	 * filters. So the flag's exact placement is pinned here: Haste wherever a
	 * guide gates on it, and the experimental Barrage toggle on the one rung that
	 * has it switched on.
	 */
	it('flags Haste on every ruleset that gates on it, plus the MV Barrage', () => {
		const flagged = allRulesets().flatMap((ruleset) =>
			ruleset.groups.flatMap((group) =>
				group.entries
					.filter((entry) => entry.buyerContextual)
					.map((entry) => `${ruleset.id}/${group.id}/${entry.id}`)
			)
		);
		expect(flagged).toEqual([
			'guide-a-kinetist-v1/auras/mercenary.skill_52155',
			'guide-b-kinetist-mv/secondary/mercenary.skill_1356',
			'guide-b-kinetist-mv/auras/mercenary.skill_52155',
			'guide-b-kinetist-mid/auras/mercenary.skill_52155',
			'guide-b-kinetist-end/auras/mercenary.skill_52155',
			'guide-b-kinetist-gg/auras/mercenary.skill_52155'
		]);
	});
});
