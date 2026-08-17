import { describe, it, expect } from 'vitest';
import {
	MERC_SOURCES,
	SOURCE_IDS,
	TIERS,
	allRulesets,
	kinetistLadder,
	entryKind,
	entryRole,
	entryTier,
	type MercFilterEntry,
	type MercFilterGroup,
	type MercRuleset
} from './rulesets';
import WvKGjV8Kfm from './__fixtures__/WvKGjV8Kfm.json';
import LgkKKmllTn from './__fixtures__/LgkKKmllTn.json';
import n5nd22GvKCa from './__fixtures__/5nd22GvKCa.json';
import n7nRvBzl2S5 from './__fixtures__/7nRvBzl2S5.json';
import BgzkZKGQF8 from './__fixtures__/BgzkZKGQF8.json';
import LgkGrPO5Fn from './__fixtures__/LgkGrPO5Fn.json';
import zbrQyEqah4 from './__fixtures__/zbrQyEqah4.json';
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
		filters: { misc_filters: { filters: { ilvl: { min: number } } } };
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
	zbrQyEqah4: zbrQyEqah4 as unknown as RawSavedSearch
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
		ilvlMin: fixture.query.filters.misc_filters.filters.ilvl.min,
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
		const rows = kinetistLadder().flatMap((ruleset) =>
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
		expect(kinetistLadder().map((r) => r.tier)).toEqual([...TIERS]);
	});

	it('gives every rung the same group ids in the saved-search order', () => {
		expect(kinetistLadder().map((r) => r.groups.map((g) => g.id))).toEqual(
			kinetistLadder().map(() => KINETIST_GROUP_IDS)
		);
	});

	it('gives every rung the same entry ids in every group', () => {
		const perGroupIds = (ruleset: MercRuleset) => ruleset.groups.map((g) => g.entries.map((e) => e.id));
		const [first, ...rest] = kinetistLadder();
		expect(rest.map(perGroupIds)).toEqual(rest.map(() => perGroupIds(first)));
	});

	it('differs between rungs only where the tier table says it does', () => {
		const flattened = kinetistLadder().map(flatten);
		const keys = [...new Set(flattened.flatMap((f) => Object.keys(f)))].sort();
		const actual: Record<string, unknown[]> = {};
		for (const key of keys) {
			const values = flattened.map((f) => f[key]);
			if (values.some((v) => v !== values[0])) actual[key] = values;
		}
		expect(actual).toEqual(DECLARED_DELTAS);
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
