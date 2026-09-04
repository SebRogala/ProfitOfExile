import { describe, it, expect } from 'vitest';
import {
	ARCHETYPES,
	MERC_SOURCES,
	SOURCE_IDS,
	TIERS,
	allRulesets,
	ladders,
	oracleFixture,
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
import mercenaryStats from './__fixtures__/mercenary-stats.json';

/**
 * The saved-search JSON as GGG returns it. TypeScript infers a per-file literal
 * type from each import (unions of "has `disabled`" and "doesn't"), so each one is
 * widened through `unknown` into this single shape — the room-presets.ts idiom.
 *
 * The four `guide-c-*.json` and six `guide-e-*.json` files are not GGG's — they
 * are this app's own transcription of CaptainLance's prose and of sushi's
 * archetype notes, written in the same body shape so they go through the same
 * reader. What that buys is a typed-model edit failing against a committed
 * artifact instead of against nothing; what it cannot buy is a check on whether
 * the source was read correctly. See `__fixtures__/README.md`.
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
 * Keyed by the oracle the ruleset declares — its saved-search hash, or the file
 * an authored query names — and NOT by the `id` inside the file. That is the
 * point of the fixture-identity test below: a file copied to the wrong name
 * fails there instead of silently validating a ruleset against another's search.
 */
const FIXTURES: Record<string, RawSavedSearch> = {
	WvKGjV8Kfm: WvKGjV8Kfm as unknown as RawSavedSearch,
	LgkKKmllTn: LgkKKmllTn as unknown as RawSavedSearch,
	'5nd22GvKCa': n5nd22GvKCa as unknown as RawSavedSearch,
	'7nRvBzl2S5': n7nRvBzl2S5 as unknown as RawSavedSearch,
	'4mKr0Jbwh9': n4mKr0Jbwh9 as unknown as RawSavedSearch,
	BgzkZKGQF8: BgzkZKGQF8 as unknown as RawSavedSearch,
	LgkGrPO5Fn: LgkGrPO5Fn as unknown as RawSavedSearch,
	zbrQyEqah4: zbrQyEqah4 as unknown as RawSavedSearch,
	'4mP3V2jQT9': n4mP3V2jQT9 as unknown as RawSavedSearch,
	Z6Em09GmHQ: Z6Em09GmHQ as unknown as RawSavedSearch,
	JBnK2YKRFl: JBnK2YKRFl as unknown as RawSavedSearch,
	d86ymvXRsJ: d86ymvXRsJ as unknown as RawSavedSearch,
	Kld4gv0Pi5: Kld4gv0Pi5 as unknown as RawSavedSearch,
	Kld4gM7yi5: Kld4gM7yi5 as unknown as RawSavedSearch,
	q9l6yK0psg: q9l6yK0psg as unknown as RawSavedSearch,
	OglBJZoQIE: OglBJZoQIE as unknown as RawSavedSearch,
	PPaX7lLqUL: PPaX7lLqUL as unknown as RawSavedSearch,
	'3q6awYZPc5': n3q6awYZPc5 as unknown as RawSavedSearch,
	mkgR2DbeS6: mkgR2DbeS6 as unknown as RawSavedSearch,
	jWRDpypkCX: jWRDpypkCX as unknown as RawSavedSearch,
	bGDrZYZaCL: bGDrZYZaCL as unknown as RawSavedSearch,
	'guide-c-kinetist': guideCKinetist as unknown as RawSavedSearch,
	'guide-c-manyshot': guideCManyshot as unknown as RawSavedSearch,
	'guide-c-blade-ambusher': guideCBladeAmbusher as unknown as RawSavedSearch,
	'guide-c-combatant': guideCCombatant as unknown as RawSavedSearch,
	'guide-e-sniper': guideESniper as unknown as RawSavedSearch,
	'guide-e-kinetist': guideEKinetist as unknown as RawSavedSearch,
	'guide-e-combatant': guideECombatant as unknown as RawSavedSearch,
	'guide-e-manyshot': guideEManyshot as unknown as RawSavedSearch,
	'guide-e-cruel-mistress': guideECruelMistress as unknown as RawSavedSearch,
	'guide-e-stormhand': guideEStormhand as unknown as RawSavedSearch,
	'8r8JqonVIV': n8r8JqonVIV as unknown as RawSavedSearch,
	veYJp9gZhE: veYJp9gZhE as unknown as RawSavedSearch,
	PPGnKVv7UL: PPGnKVv7UL as unknown as RawSavedSearch,
	yYvmr6rjcR: yYvmr6rjcR as unknown as RawSavedSearch,
	d80ePvdvhJ: d80ePvdvhJ as unknown as RawSavedSearch,
	rPogYW44uQ: rPogYW44uQ as unknown as RawSavedSearch
};

/** GGG's Mercenary stat vocabulary: stat id -> display text. */
const VOCABULARY = new Map(
	(mercenaryStats as { entries: { id: string; text: string }[] }).entries.map((e) => [e.id, e.text])
);

/** Everything the transcription is allowed to claim about its query. */
function fromRuleset(ruleset: MercRuleset) {
	return {
		oracle: oracleFixture(ruleset),
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
		oracle: fixture.id,
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

/** `<groupId> <type> enabled=<bool> min=<n|->`, in the saved search's own order. */
function shape(ruleset: MercRuleset): string[] {
	return ruleset.groups.map(
		(g) => `${g.id} ${g.type} enabled=${g.enabledInSearch} min=${g.min ?? '-'}`
	);
}

/**
 * Every switch of one rung as a flat map, so two rungs of a ladder can be
 * diffed key by key: `<groupId>.enabled`, `<groupId>.min`, and
 * `<groupId>/<entryId>.enabled`.
 *
 * Module scope because two ladders are diffed this way — guide-b's Kinetist
 * rungs against the tier table, and guide-f's rung pairs against their one
 * declared lever — and a second copy would let the two notions of "what a rung
 * IS" drift apart while both looked pinned.
 */
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

describe('query transcription', () => {
	for (const ruleset of allRulesets()) {
		describe(ruleset.id, () => {
			it('reads the fixture that carries its own declared oracle name', () => {
				expect(FIXTURES[oracleFixture(ruleset)]?.id).toBe(oracleFixture(ruleset));
			});

			it('matches its fixture group for group and entry for entry', () => {
				expect(fromRuleset(ruleset)).toEqual(fromFixture(FIXTURES[oracleFixture(ruleset)]));
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

describe('guide-c rulesets', () => {
	const GUIDE_C = MERC_SOURCES.find((s) => s.id === 'guide-c') as MercSource;

	const GUIDE_C_GROUP_IDS: Record<string, string[]> = {
		'guide-c-kinetist': ['core', 'deny', 'buffs'],
		'guide-c-manyshot': ['core', 'secondary', 'deny', 'buffs'],
		'guide-c-blade-ambusher': ['core', 'buffs'],
		'guide-c-combatant': ['core', 'secondary', 'buffs']
	};

	// Two of the four archetypes carry no denial at all — the prose says "do not"
	// about Kinetic Bolt and Icicle Rain and about nothing else, and inventing a
	// third deny list would be this app putting words in the author's mouth.
	it('gives each ruleset the group ids its prose is transcribed under', () => {
		expect(Object.fromEntries(GUIDE_C.rulesets.map((r) => [r.id, r.groups.map((g) => g.id)]))).toEqual(
			GUIDE_C_GROUP_IDS
		);
	});

	/**
	 * The modelling ruling, stated as data: in every guide-c `mercenary` group the
	 * SKILL is the one live filter and every support the author listed is switched
	 * off. That is what makes a guide-c pass mean "has the skill row, carries no
	 * denied skill" and every listed link a bonus rather than a gate.
	 *
	 * Pinned here rather than left to the fixture-fidelity test above, because
	 * that test compares this module against a file this module generated — both
	 * sides move together when a switch flips, so it cannot see this rule break.
	 */
	it('leaves the skill as the only live filter of every mercenary group', () => {
		const live = GUIDE_C.rulesets.flatMap((ruleset) =>
			ruleset.groups
				.filter((group) => group.type === 'mercenary')
				.map(
					(group) =>
						`${ruleset.id}/${group.id}: ${group.entries
							.filter((entry) => entry.enabledInSearch)
							.map((entry) => entry.name)
							.join(', ')}`
				)
		);
		expect(live).toEqual([
			'guide-c-kinetist/core: Kinetic Blast of Clustering',
			'guide-c-manyshot/core: Ice Shot',
			'guide-c-manyshot/secondary: Vaal Ice Shot',
			'guide-c-blade-ambusher/core: Spectral Helix of Trarthus',
			'guide-c-combatant/core: Static Strike',
			'guide-c-combatant/secondary: Frost Blades'
		]);
	});

	// A `min` would turn the author's "and these links" into "at least N of
	// these", which is a rule he never wrote — and the derived-query clamp in
	// `trade-links.ts` exists precisely because a `min` over parked filters is
	// the one thing that hands out a dead comp link.
	it('sets no minimum on any group', () => {
		const withMin = GUIDE_C.rulesets.flatMap((ruleset) =>
			ruleset.groups.filter((group) => group.min !== undefined).map((group) => `${ruleset.id}/${group.id}`)
		);
		expect(withMin).toEqual([]);
	});

	// The prose sets no item-level floor, so neither does the transcription. The
	// guide-b Manyshot rungs are the only other searches without one.
	it('sets no item-level floor on any ruleset', () => {
		expect(GUIDE_C.rulesets.map((r) => `${r.id} ilvl=${r.ilvlMin ?? 'none'}`)).toEqual([
			'guide-c-kinetist ilvl=none',
			'guide-c-manyshot ilvl=none',
			'guide-c-blade-ambusher ilvl=none',
			'guide-c-combatant ilvl=none'
		]);
	});

	// The Trarthus transfigure and plain Spectral Helix are different stat ids,
	// and guide-a's Combatant search DENIES the plain one — transcribing the
	// wrong id would make this ruleset ask for a skill its own author rejects.
	it('asks for the Trarthus transfigure of Spectral Helix, not the base skill', () => {
		const core = groupOf(rulesetById('guide-c-blade-ambusher'), 'core');
		expect(core?.entries[0]).toEqual({
			id: 'mercenary.skill_28988',
			name: 'Spectral Helix of Trarthus',
			enabledInSearch: true
		});
	});

	// Untiered by construction: the prose ranks nothing, so the page has to draw
	// these as cards. A rung with no ladder would silently vanish from `ladders`.
	it('declares no tier ladder', () => {
		expect(ladders(GUIDE_C)).toEqual([]);
	});
});

describe('guide-e rulesets', () => {
	const GUIDE_E = MERC_SOURCES.find((s) => s.id === 'guide-e') as MercSource;

	/**
	 * `<groupId> <type> enabled=<bool>` per ruleset — the shape each note block
	 * was transcribed under.
	 *
	 * The `enabled` flag is in here rather than left implicit because on this
	 * source it carries a RULING, twice. Sniper's `tornado-links` is parked
	 * because the links the note hangs on "TS" are wanted rather than required,
	 * and parking them is what keeps a Rain of Arrows Sniper from being asked for
	 * Tornado Shot's links; Combatant's `frost-blades` and `wild-strike` are
	 * parked because "frost blades/wild strike + static strike together" is an OR,
	 * and making all three live would ask for three of the four skills a Combatant
	 * rolls two of — a ruleset no mercenary could answer. Flipping any of those
	 * three to live fails here.
	 *
	 * The Sniper's `core` TYPE carries a third ruling: `count` rather than
	 * `mercenary`, because "rain of arrows works too" is an OR over two skill
	 * rows. A `mercenary` group would scope the pair to one row and demand both.
	 */
	const GUIDE_E_GROUPS: Record<string, string[]> = {
		'guide-e-sniper': [
			'core count enabled=true',
			'tornado-links mercenary enabled=false',
			'deny-supports not enabled=true',
			'totems and enabled=true',
			'auras and enabled=true'
		],
		'guide-e-kinetist': [
			'core mercenary enabled=true',
			'secondary mercenary enabled=true',
			'deny not enabled=true',
			'auras and enabled=true'
		],
		'guide-e-combatant': [
			'core mercenary enabled=true',
			'frost-blades mercenary enabled=false',
			'wild-strike mercenary enabled=false',
			'deny not enabled=true',
			'deny-supports not enabled=true'
		],
		'guide-e-manyshot': [
			'core mercenary enabled=true',
			'secondary mercenary enabled=true',
			'deny not enabled=true'
		],
		'guide-e-cruel-mistress': [
			'core mercenary enabled=true',
			'secondary and enabled=true',
			'auras and enabled=true'
		],
		'guide-e-stormhand': ['core mercenary enabled=true', 'secondary mercenary enabled=true']
	};

	it('gives each ruleset the groups its note block is transcribed under', () => {
		expect(
			Object.fromEntries(
				GUIDE_E.rulesets.map((r) => [
					r.id,
					r.groups.map((g) => `${g.id} ${g.type} enabled=${g.enabledInSearch}`)
				])
			)
		).toEqual(GUIDE_E_GROUPS);
	});

	// Which note block landed under which archetype key — including the two the
	// author spells differently ("kineticist", "Cruel Mistriss"). The label is the
	// app's word so guide-e's Kinetist sits in the same column as guide-b's.
	it('files each note block under the archetype key its class name resolves to', () => {
		expect(GUIDE_E.rulesets.map((r) => `${r.id} ${r.archetype} "${r.label}"`)).toEqual([
			'guide-e-sniper sniper "Sniper"',
			'guide-e-kinetist kinetist "Kinetist"',
			'guide-e-combatant combatant "Combatant"',
			'guide-e-manyshot manyshot "Manyshot"',
			'guide-e-cruel-mistress cruel-mistress "Cruel Mistress"',
			'guide-e-stormhand stormhand "Stormhand"'
		]);
	});

	/**
	 * The guide-c modelling ruling, applied here and stated as data: in every
	 * guide-e `mercenary` group the SKILL is the one live filter and every support
	 * the note lists is switched off. That is what makes a guide-e pass mean "has
	 * the skill row, carries no denied stat" and every listed link a bonus.
	 *
	 * Pinned here rather than left to the fixture-fidelity sweep, because that
	 * sweep compares this module against a file this module generated — both sides
	 * move together when a switch flips, so it cannot see this rule break.
	 *
	 * The Sniper's `core` is not in this list and cannot be: it is the one `count`
	 * group here, two live SKILLS asking for either, which is the OR the note's
	 * "rain of arrows works too" states. The rule below is about row-scoped groups.
	 */
	it('leaves the skill as the only live filter of every mercenary group', () => {
		const live = GUIDE_E.rulesets.flatMap((ruleset) =>
			ruleset.groups
				.filter((group) => group.type === 'mercenary')
				.map(
					(group) =>
						`${ruleset.id}/${group.id}: ${group.entries
							.filter((entry) => entry.enabledInSearch)
							.map((entry) => entry.name)
							.join(', ')}`
				)
		);
		expect(live).toEqual([
			'guide-e-sniper/tornado-links: Tornado Shot',
			'guide-e-kinetist/core: Greater Kinetic Blast',
			'guide-e-kinetist/secondary: Kinetic Blast of Clustering',
			'guide-e-combatant/core: Static Strike',
			'guide-e-combatant/frost-blades: Frost Blades',
			'guide-e-combatant/wild-strike: Wild Strike',
			'guide-e-manyshot/core: Ice Shot',
			'guide-e-manyshot/secondary: Vaal Ice Shot',
			'guide-e-cruel-mistress/core: Soulrend of Reaping',
			'guide-e-stormhand/core: Arc',
			'guide-e-stormhand/secondary: Ball Lightning of Static'
		]);
	});

	/**
	 * The note names no LINK COUNT anywhere — it lists supports, it never says
	 * "two of these" — so no `mercenary` group here carries a `min`. That is the
	 * rule with teeth: a `min` over parked filters is the one thing that hands out
	 * a dead comp link (`trade-links.ts`'s clamp), and every parked filter on this
	 * source lives in a `mercenary` group.
	 *
	 * The Sniper's `core` is the one exception, and it is a `min` the note DOES
	 * state: "rain of arrows works too" is an OR over two skills, which is `min: 1`
	 * of two live entries — the same shape guide-b's Kinetist MV rung uses for
	 * "Greater Kinetic Blast or Barrage". Pinned by value, because dropping the
	 * `min` would silently turn that OR into "either, or none".
	 */
	it('gives no mercenary group a minimum', () => {
		const withMin = GUIDE_E.rulesets.flatMap((ruleset) =>
			ruleset.groups
				.filter((group) => group.type === 'mercenary' && group.min !== undefined)
				.map((group) => `${ruleset.id}/${group.id}`)
		);
		expect(withMin).toEqual([]);
	});

	it('asks the Sniper for one of the two skills the note accepts', () => {
		const core = GUIDE_E.rulesets
			.find((ruleset) => ruleset.id === 'guide-e-sniper')
			?.groups.find((group) => group.id === 'core');
		expect([core?.type, core?.min, core?.entries.map((entry) => entry.name)]).toEqual([
			'count',
			1,
			['Tornado Shot', 'Rain of Arrows of Saturation']
		]);
	});

	// The note sets no item-level floor — it never mentions item level at all — so
	// neither does the transcription.
	it('sets no item-level floor on any ruleset', () => {
		expect(GUIDE_E.rulesets.map((r) => `${r.id} ilvl=${r.ilvlMin ?? 'none'}`)).toEqual([
			'guide-e-sniper ilvl=none',
			'guide-e-kinetist ilvl=none',
			'guide-e-combatant ilvl=none',
			'guide-e-manyshot ilvl=none',
			'guide-e-cruel-mistress ilvl=none',
			'guide-e-stormhand ilvl=none'
		]);
	});

	/**
	 * The shorthand resolutions, pinned by the NAME each chosen id carries in
	 * GGG's vocabulary. The `stat vocabulary` sweep above ties id to name, so
	 * asserting the name here asserts the ID CHOSEN was the intended one — and a
	 * wrong id fails by printing what it actually resolved to, or `<not declared>`
	 * when the entry moved group.
	 *
	 * These are the readings that could have gone another way; everywhere else the
	 * note names its skill outright. Why each:
	 *
	 * - "gilded +2 proj" → Gilded Secondary Shots, the one gilded support whose
	 *   vocabulary text names Tornado Shot's secondary projectiles. Ruled out:
	 *   Gilded Volleys (Bladefall's), Gilded Scattershot (+4 random projectiles),
	 *   Gilded Archers (a minion support).
	 * - "totem" → the two BALLISTA skills of the Sniper pool, not Multiple Totems
	 *   or Gilded Totemic Onslaught: those are supports, and the note names no
	 *   skill for them to support.
	 * - "rain of arrows" and "kinetic rain" → the transfigured forms, which are
	 *   the only forms of either skill in the mercenary vocabulary.
	 * - "elemental hit" → Elemental Hit of Ice, the only Elemental Hit there is,
	 *   although poewiki files it under Mysterious Diver rather than Combatant.
	 *   Transcribed as written; the group comment records the mismatch.
	 * - "summon void" → Summon Seeking Void, NOT Void Sphere: that one is the
	 *   Cruel Mistress's class primary, so every one of them has it.
	 * - "fr totems" → Forbidden Rite Totem.
	 * - "gilded chain on arc" → Gilded Chain Distance, the Chain family's Tier 3
	 *   and the one whose text is Arc-specific.
	 */
	it('resolves the note’s shorthand to the GGG names it was read as', () => {
		const named = (rulesetId: string, groupId: string, entryId: string): string =>
			`${rulesetId}/${groupId}/${entryId} = ${
				entryOf(rulesetById(rulesetId), groupId, entryId)?.name ?? '<not declared>'
			}`;
		expect([
			named('guide-e-sniper', 'tornado-links', 'mercenary.support_18499'),
			named('guide-e-sniper', 'totems', 'mercenary.skill_61903'),
			named('guide-e-sniper', 'totems', 'mercenary.skill_44144'),
			named('guide-e-sniper', 'core', 'mercenary.skill_40759'),
			named('guide-e-kinetist', 'deny', 'mercenary.skill_32089'),
			named('guide-e-combatant', 'deny', 'mercenary.skill_8708'),
			named('guide-e-cruel-mistress', 'secondary', 'mercenary.skill_54144'),
			named('guide-e-cruel-mistress', 'secondary', 'mercenary.skill_29071'),
			named('guide-e-stormhand', 'core', 'mercenary.support_31571')
		]).toEqual([
			'guide-e-sniper/tornado-links/mercenary.support_18499 = Gilded Secondary Shots (Tier 3)',
			'guide-e-sniper/totems/mercenary.skill_61903 = Shrapnel Ballista',
			'guide-e-sniper/totems/mercenary.skill_44144 = Siege Ballista of Trarthus',
			'guide-e-sniper/core/mercenary.skill_40759 = Rain of Arrows of Saturation',
			'guide-e-kinetist/deny/mercenary.skill_32089 = Kinetic Rain of Impact',
			'guide-e-combatant/deny/mercenary.skill_8708 = Elemental Hit of Ice',
			'guide-e-cruel-mistress/secondary/mercenary.skill_54144 = Summon Seeking Void',
			'guide-e-cruel-mistress/secondary/mercenary.skill_29071 = Forbidden Rite Totem',
			'guide-e-stormhand/core/mercenary.support_31571 = Gilded Chain Distance (Tier 3)'
		]);
	});

	// Untiered like guide-c and for the same reason: the note ranks SUPPORTS, not
	// mercenaries, so there is no cheap-to-expensive ladder to draw. The rankings
	// it does publish ride `authorNote`, pinned under "author notes" below.
	it('declares no tier ladder', () => {
		expect(ladders(GUIDE_E)).toEqual([]);
	});
});

describe('the oracle a ruleset is checked against', () => {
	/**
	 * `savedSearch` and `authored` are the two kinds of ground truth, and the type
	 * admits exactly one per ruleset. The split is what stops `savedSearchUrl`
	 * from being handed a guide-c ruleset and building a trade link to a hash GGG
	 * never issued — so which rulesets are on which side is pinned, not implied.
	 */
	it('gives the four saved-search guides a hash and guide-c and guide-e a fixture file', () => {
		const byKind = allRulesets().map(
			(r) => `${r.id} ${r.savedSearch ? `saved=${r.savedSearch.hash}` : `authored=${r.authored?.file}`}`
		);
		expect(byKind.filter((row) => row.includes('authored='))).toEqual([
			'guide-c-kinetist authored=guide-c-kinetist',
			'guide-c-manyshot authored=guide-c-manyshot',
			'guide-c-blade-ambusher authored=guide-c-blade-ambusher',
			'guide-c-combatant authored=guide-c-combatant',
			'guide-e-sniper authored=guide-e-sniper',
			'guide-e-kinetist authored=guide-e-kinetist',
			'guide-e-combatant authored=guide-e-combatant',
			'guide-e-manyshot authored=guide-e-manyshot',
			'guide-e-cruel-mistress authored=guide-e-cruel-mistress',
			'guide-e-stormhand authored=guide-e-stormhand'
		]);
		expect(byKind.filter((row) => row.includes('saved=')).length).toBe(28);
	});

	it('resolves a saved ruleset to its GGG hash', () => {
		expect(oracleFixture(rulesetById('guide-b-kinetist-mv'))).toBe('7nRvBzl2S5');
	});

	it('resolves an authored ruleset to the fixture file it names', () => {
		expect(oracleFixture(rulesetById('guide-c-manyshot'))).toBe('guide-c-manyshot');
	});

	// Every saved search here is Allflame's; an authored query belongs to no
	// league at all, which is why it cannot carry a `savedSearch` with a made-up
	// hash and an inherited league.
	it('binds a saved search to a league and an authored query to none', () => {
		expect(rulesetById('guide-b-kinetist-mv').savedSearch?.league).toBe('Allflame');
		expect(rulesetById('guide-c-manyshot').savedSearch).toBeUndefined();
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

/**
 * The seven slots of a Kinetist rung, in the saved searches' own order. Module
 * scope because TWO sources are transcribed under them — guide-b's four rungs
 * and guide-d's two — and the whole point of a slot id is that a rung of either
 * can be diffed against a rung of the other. Two matching literals in two
 * describes would let one ladder drift and still look pinned.
 */
const KINETIST_GROUP_IDS = [
	'deny',
	'core',
	'secondary',
	'behavior',
	'deny-supports',
	'auras',
	'damage'
];

describe('Kinetist ladder', () => {
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

/**
 * Nerotox's Combatant video publishes both this ladder and the Wild Strike one.
 * Five rungs, ONE six-group skeleton — so what is pinned here is the group-id
 * mapping (invisible to the fidelity tests, since ids exist only in `rulesets.ts`)
 * and the three things that move across the rungs.
 */
describe('Frost Blades ladder', () => {
	const FROST_BLADES_GROUP_IDS = ['required-skills', 'core', 'deny-pierce', 'damage', 'speed', 'return'];

	it('lists five rungs, the two Endgame siblings in the guide’s own order', () => {
		expect(ladderNamed('frost-blades').map((r) => r.id)).toEqual([
			'guide-b-frost-blades-mv',
			'guide-b-frost-blades-mid',
			'guide-b-frost-blades-end-noreturn',
			'guide-b-frost-blades-end-return',
			'guide-b-frost-blades-gg'
		]);
	});

	it('gives every rung the same six group ids in the saved-search order', () => {
		expect(ladderNamed('frost-blades').map((r) => r.groups.map((g) => g.id))).toEqual(
			ladderNamed('frost-blades').map(() => FROST_BLADES_GROUP_IDS)
		);
	});

	it('gives every rung the same entry ids in every group', () => {
		const perGroupIds = (ruleset: MercRuleset) =>
			ruleset.groups.map((g) => g.entries.map((e) => e.id));
		const [first, ...rest] = ladderNamed('frost-blades');
		expect(rest.map(perGroupIds)).toEqual(rest.map(() => perGroupIds(first)));
	});

	it('lifts the damage minimum from two to three at Midgame and holds it there', () => {
		expect(
			ladderNamed('frost-blades').map((r) => `${r.id} ${groupOf(r, 'damage')?.min}`)
		).toEqual([
			'guide-b-frost-blades-mv 2',
			'guide-b-frost-blades-mid 3',
			'guide-b-frost-blades-end-noreturn 3',
			'guide-b-frost-blades-end-return 3',
			'guide-b-frost-blades-gg 3'
		]);
	});

	it('switches the speed group on for Endgame (no return) and GG only', () => {
		expect(
			ladderNamed('frost-blades').map((r) => `${r.id} ${groupOf(r, 'speed')?.enabledInSearch}`)
		).toEqual([
			'guide-b-frost-blades-mv false',
			'guide-b-frost-blades-mid false',
			'guide-b-frost-blades-end-noreturn true',
			'guide-b-frost-blades-end-return false',
			'guide-b-frost-blades-gg true'
		]);
	});

	it('switches the return group on for Endgame (return) and GG only', () => {
		expect(
			ladderNamed('frost-blades').map((r) => `${r.id} ${groupOf(r, 'return')?.enabledInSearch}`)
		).toEqual([
			'guide-b-frost-blades-mv false',
			'guide-b-frost-blades-mid false',
			'guide-b-frost-blades-end-noreturn false',
			'guide-b-frost-blades-end-return true',
			'guide-b-frost-blades-gg true'
		]);
	});

	// The structural fact the whole ladder rests on: the two Endgame links are
	// SIBLINGS, each Midgame plus one switch, and GG is the union of the two. If
	// one of them were Midgame plus BOTH switches, it would be GG under another
	// name and the tie between them would stop being harmless.
	it('separates the two Endgame rungs by the speed and return switches alone', () => {
		const noReturn = shape(rulesetById('guide-b-frost-blades-end-noreturn'));
		const withReturn = shape(rulesetById('guide-b-frost-blades-end-return'));
		const differing = noReturn.filter((line, i) => line !== withReturn[i]);
		expect(differing).toEqual([
			'speed mercenary enabled=true min=2',
			'return mercenary enabled=false min=-'
		]);
	});

	it('reaches the GG rung by switching Midgame’s two parked groups both on', () => {
		const mid = shape(rulesetById('guide-b-frost-blades-mid'));
		const gg = shape(rulesetById('guide-b-frost-blades-gg'));
		expect(gg.filter((line, i) => line !== mid[i])).toEqual([
			'speed mercenary enabled=true min=2',
			'return mercenary enabled=true min=-'
		]);
	});
});

/**
 * `tierLabel` is the rung's own wording, and it is declared exactly where the
 * `TIERS` key cannot name the rung on its own. Two reasons that happens, one per
 * source below, and the sweep is over EVERY ruleset so a label added anywhere
 * else has to be justified here rather than appearing silently in a column head.
 */
describe('rungs whose tier key does not name them', () => {
	it('spells each such rung the way its own guide spells it', () => {
		expect(
			allRulesets()
				.filter((r) => r.tierLabel !== undefined)
				.map((r) => `${r.id} ${r.tierLabel}`)
		).toEqual([
			// One ladder, two rungs, one tier: `TIERS` spells both 'end', and two
			// columns headed 'endgame' would tell the reader the matrix is showing
			// one search twice.
			'guide-b-frost-blades-end-noreturn endgame (no return)',
			'guide-b-frost-blades-end-return endgame (return)',
			// One tier per rung here — what `TIERS` cannot carry is the guide's own
			// naming: XTheFarmerX calls his two searches "budget" and "20D", and
			// 'mv'/'mid' would put words in his mouth about what they cost.
			'guide-d-kinetist-budget budget',
			'guide-d-kinetist-20d 20D',
			// Every guide-f rung carries one: this author publishes exactly two
			// searches per archetype and names them by price, so 'mv'/'end' — which
			// is only how the ladder RANKS them — would be the app's word, not his.
			'guide-f-manyshot-cheap Cheap',
			'guide-f-manyshot-expensive Expensive',
			'guide-f-combatant-cheap Cheap',
			'guide-f-combatant-expensive Expensive',
			'guide-f-kinetist-cheap Cheap',
			'guide-f-kinetist-expensive Expensive'
		]);
	});
});

/**
 * The Combatant video's second ladder. Its group ids are NOT the Frost Blades
 * ones with a different skill: there is no `core` group, the deny list sits
 * after the speed and return groups, and the sixth `greater` group only exists
 * from Midgame up.
 */
describe('Wild Strike ladder', () => {
	it('lists the four rungs cheapest first', () => {
		expect(ladderNamed('wild-strike').map((r) => r.id)).toEqual([
			'guide-b-wild-strike-mv',
			'guide-b-wild-strike-mid',
			'guide-b-wild-strike-end',
			'guide-b-wild-strike-gg'
		]);
	});

	it('opens the Minimum rung with five groups and no core group among them', () => {
		expect(shape(rulesetById('guide-b-wild-strike-mv'))).toEqual([
			'required-skills and enabled=true min=-',
			'damage mercenary enabled=true min=3',
			'speed mercenary enabled=false min=2',
			'return mercenary enabled=false min=-',
			'deny-multistrike not enabled=true min=-'
		]);
	});

	it('appends the greater group from Midgame up, leaving the five below it in place', () => {
		expect(ladderNamed('wild-strike').map((r) => r.groups.map((g) => g.id))).toEqual([
			['required-skills', 'damage', 'speed', 'return', 'deny-multistrike'],
			['required-skills', 'damage', 'speed', 'return', 'deny-multistrike', 'greater'],
			['required-skills', 'damage', 'speed', 'return', 'deny-multistrike', 'greater'],
			['required-skills', 'damage', 'speed', 'return', 'deny-multistrike', 'greater']
		]);
	});

	it('asks the greater group for three at Endgame and two either side of it', () => {
		expect(
			ladderNamed('wild-strike').map((r) => `${r.tier} ${groupOf(r, 'greater')?.min ?? 'no greater group'}`)
		).toEqual(['mv no greater group', 'mid 2', 'end 3', 'gg 2']);
	});

	// The one delta on this ladder that moves an ENTRY rather than a group: Midgame
	// and GG leave the speed group live but park its Tier-2 half, so those two
	// rungs ask for Greater Faster Attacks specifically.
	it('parks Faster Attacks inside the live speed group on Midgame and GG', () => {
		expect(
			ladderNamed('wild-strike').map(
				(r) =>
					`${r.tier} group=${groupOf(r, 'speed')?.enabledInSearch} FA=${entryOf(r, 'speed', 'mercenary.support_987')?.enabledInSearch} GFA=${entryOf(r, 'speed', 'mercenary.support_50485')?.enabledInSearch}`
			)
		).toEqual([
			'mv group=false FA=true GFA=true',
			'mid group=true FA=false GFA=true',
			'end group=true FA=true GFA=true',
			'gg group=true FA=false GFA=true'
		]);
	});

	it('switches the return group on at GG and nowhere below it', () => {
		expect(
			ladderNamed('wild-strike').map((r) => `${r.tier} ${groupOf(r, 'return')?.enabledInSearch}`)
		).toEqual(['mv false', 'mid false', 'end false', 'gg true']);
	});
});

/**
 * XTheFarmerX's two-rung ladder. Same archetype and the same seven slots as
 * Nerotox's, which is the point: the two sources are different opinions about
 * one kind of mercenary, and the matrix can only diff them rung to rung while
 * the slot ids agree.
 */
describe('guide-d Kinetist ladder', () => {
	const GUIDE_D = MERC_SOURCES.find((s) => s.id === 'guide-d') as MercSource;

	function guideDRungs(): MercRuleset[] {
		const found = ladders(GUIDE_D)[0];
		if (!found) throw new Error('guide-d declares no ladder');
		return found;
	}

	// Id AND tier, because `ladders()` orders by `TIERS` and the ids alone would
	// pass on a stable sort that never looked at a tier: the claim is that the
	// budget rung is keyed `mv` and the 20D rung `mid`, and that this is the
	// cheap-to-dear order.
	it('lists its two rungs cheapest first', () => {
		expect(guideDRungs().map((r) => `${r.id} ${r.tier}`)).toEqual([
			'guide-d-kinetist-budget mv',
			'guide-d-kinetist-20d mid'
		]);
	});

	// The constraint that makes the two sources comparable, and the one thing the
	// fixture-fidelity tests cannot see: group ids exist only in `rulesets.ts`.
	// The budget rung's first `not` group denies the Pierce family as well as the
	// three skills, and it is still the `deny` slot — a rung is allowed to put
	// more in a slot, never to rename it.
	it('transcribes both rungs under the same slot ids Nerotox’s rungs use', () => {
		expect(guideDRungs().map((r) => r.groups.map((g) => g.id))).toEqual([
			KINETIST_GROUP_IDS,
			KINETIST_GROUP_IDS
		]);
	});

	// The author publishes no price for either search — the ~5d / ~9d the listings
	// opened at on 2026-08-28 is a live measurement, not his number. A `floor` is
	// the guide author speaking (`verdict.ts` prints it as his), so inventing one
	// from a measurement would attribute a price to someone who never quoted it.
	it('quotes no price floor on either rung', () => {
		expect(guideDRungs().map((r) => `${r.id} ${r.floor ?? 'no floor'}`)).toEqual([
			'guide-d-kinetist-budget no floor',
			'guide-d-kinetist-20d no floor'
		]);
	});
});

/**
 * Path of Evening's three two-rung ladders.
 *
 * What is worth pinning here is the UPGRADE: this author publishes a cheap and
 * an expensive search per archetype and no readable prose about either (the page
 * 403s — see `rulesets.ts`), so the only statement he makes about what a dearer
 * mercenary is worth paying for is the difference between the two searches. That
 * difference is asserted as data — every key that moves, and by implication every
 * key that does not — rather than described in a comment nothing checks.
 *
 * The group ids are pinned as well, for the reason the guide-d block gives: they
 * exist only in `rulesets.ts`, so the fixture-fidelity sweep cannot see them.
 */
describe('guide-f ladders', () => {
	const GUIDE_F = MERC_SOURCES.find((s) => s.id === 'guide-f') as MercSource;

	/** One guide-f ladder's rungs, cheap first. */
	function guideFLadder(key: string): MercRuleset[] {
		const found = ladders(GUIDE_F).find((rungs) => rungs[0].ladder === key);
		if (!found) throw new Error(`guide-f declares no ladder ${key}`);
		return found;
	}

	/** Every switch that differs between a ladder's two rungs, `key: cheap -> expensive`. */
	function upgrade(key: string): string[] {
		const [cheap, expensive] = guideFLadder(key).map(flatten);
		const keys = [...new Set([...Object.keys(cheap), ...Object.keys(expensive)])].sort();
		return keys
			.filter((k) => cheap[k] !== expensive[k])
			.map((k) => `${k}: ${cheap[k] ?? 'absent'} -> ${expensive[k] ?? 'absent'}`);
	}

	it('lists three ladders, each cheap rung before its expensive one', () => {
		expect(ladders(GUIDE_F).map((rungs) => rungs.map((r) => `${r.id} ${r.tier}`))).toEqual([
			['guide-f-manyshot-cheap mv', 'guide-f-manyshot-expensive end'],
			['guide-f-combatant-cheap mv', 'guide-f-combatant-expensive end'],
			['guide-f-kinetist-cheap mv', 'guide-f-kinetist-expensive end']
		]);
	});

	it('slots the Combatant rungs under the same five ids', () => {
		expect(guideFLadder('combatant').map((r) => r.groups.map((g) => g.id))).toEqual([
			['required-skills', 'damage', 'return', 'secondary-links', 'deny-supports'],
			['required-skills', 'damage', 'return', 'secondary-links', 'deny-supports']
		]);
	});

	it('slots the Kineticist rungs under the same five ids', () => {
		expect(guideFLadder('kinetist').map((r) => r.groups.map((g) => g.id))).toEqual([
			['required-skills', 'deny-supports', 'behavior', 'damage', 'return'],
			['required-skills', 'deny-supports', 'behavior', 'damage', 'return']
		]);
	});

	it('gives the expensive Multishot rung a sixth group the cheap one has no slot for', () => {
		expect(guideFLadder('manyshot').map((r) => r.groups.map((g) => g.id))).toEqual([
			['required-skills', 'damage', 'return', 'deny', 'projectiles'],
			['required-skills', 'damage', 'return', 'deny', 'vaal-damage', 'projectiles']
		]);
	});

	// The lever, and the whole lever: the two searches are the same filters with
	// the Frost Blades Return group parked at Cheap and live at Expensive.
	it('upgrades the Combatant search by switching its Return group on', () => {
		expect(upgrade('combatant')).toEqual(['return.enabled: false -> true']);
	});

	it('upgrades the Kineticist search by switching its Return group on', () => {
		expect(upgrade('kinetist')).toEqual(['return.enabled: false -> true']);
	});

	// Multishot is the exception, and the reason the upgrade is asserted per
	// ladder rather than once: Return is live on BOTH its rungs, and what the
	// dearer search adds is Hatred, a Vaal Ice Shot row group of its own, and a
	// projectile group that no longer counts Lesser Chain (Tier 1) — dropped
	// from the search, not parked in it.
	it('upgrades the Multishot search by three levers, none of them the Return group', () => {
		expect(upgrade('manyshot')).toEqual([
			'projectiles/mercenary.support_14317.enabled: true -> absent',
			'required-skills/mercenary.skill_24482.enabled: false -> true',
			'vaal-damage.enabled: absent -> true',
			'vaal-damage.min: absent -> 3',
			'vaal-damage/mercenary.skill_16381.enabled: absent -> true',
			'vaal-damage/mercenary.support_28416.enabled: absent -> true',
			'vaal-damage/mercenary.support_44886.enabled: absent -> true',
			'vaal-damage/mercenary.support_5293.enabled: absent -> true'
		]);
	});

	// The buyer's-list reading, as on guide-d: this author quotes no price for
	// either rung of anything, so nothing here may print one as his.
	it('quotes no price floor on any rung', () => {
		expect(GUIDE_F.rulesets.map((r) => `${r.id} ${r.floor ?? 'no floor'}`)).toEqual([
			'guide-f-manyshot-cheap no floor',
			'guide-f-manyshot-expensive no floor',
			'guide-f-combatant-cheap no floor',
			'guide-f-combatant-expensive no floor',
			'guide-f-kinetist-cheap no floor',
			'guide-f-kinetist-expensive no floor'
		]);
	});
});

/**
 * A fixture is the oracle of a RULESET, not of a source — and since XTheFarmerX
 * republished Nerotox's Kinetist MV link as his own 20D rung, one committed
 * search now answers two of them.
 */
describe('a saved search two sources transcribe', () => {
	/** Oracle names more than one ruleset is checked against. */
	function sharedOracles(): string[] {
		const counts = new Map<string, number>();
		for (const ruleset of allRulesets()) {
			const oracle = oracleFixture(ruleset);
			counts.set(oracle, (counts.get(oracle) ?? 0) + 1);
		}
		return [...counts.entries()].filter(([, n]) => n > 1).map(([oracle]) => oracle);
	}

	it('checks the guide-b MV rung and the guide-d 20D rung against one hash', () => {
		expect(allRulesets().filter((r) => oracleFixture(r) === '7nRvBzl2S5').map((r) => r.id)).toEqual([
			'guide-b-kinetist-mv',
			'guide-d-kinetist-20d'
		]);
	});

	// The other half: sharing is a documented fact about ONE search, so a second
	// shared oracle is a ruleset pointed at the wrong fixture — which the
	// per-ruleset fidelity test above would report as a transcription failure
	// somewhere else entirely.
	it('leaves every other ruleset an oracle of its own', () => {
		expect(sharedOracles()).toEqual(['7nRvBzl2S5']);
	});
});

describe('ladder keys', () => {
	it('groups guide-b into its four ladders, first-declared first', () => {
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
			],
			[
				'guide-b-frost-blades-mv',
				'guide-b-frost-blades-mid',
				'guide-b-frost-blades-end-noreturn',
				'guide-b-frost-blades-end-return',
				'guide-b-frost-blades-gg'
			],
			[
				'guide-b-wild-strike-mv',
				'guide-b-wild-strike-mid',
				'guide-b-wild-strike-end',
				'guide-b-wild-strike-gg'
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
	// Verbatim from the two video descriptions, CaptainLance's four role lines and
	// the two sushi lines that are not switches — the app never paraphrases them.
	// For guide-c and guide-e they are the whole verdict a passing ruleset has to
	// offer: there is no floor and no tier behind them.
	//
	// Guide-e's two are the reason this list matters more than it used to. One
	// holds support RANKINGS ("chain > wed > hypo > faster/gmp") and the shouted
	// "NO PIERCE" whose denial is a live `not` group; the other is a remark about
	// a skill no group asks for ("barrage is not a brick"). The transcription
	// tests cannot see any of it, because none of it is a switch. This is the only
	// place that content is pinned at all.
	it('carries each note exactly as its author wrote it', () => {
		expect(
			allRulesets()
				.filter((r) => r.authorNote !== undefined)
				.map((r) => [r.id, r.authorNote])
		).toEqual([
			[
				'guide-b-kinetist-gg',
				'look at damage links still - these are 4L not even 5L. 5L mercs nearly never exist for KB in gg setups, a 5L with barrage can beat a 4L with greater KB'
			],
			['guide-b-manyshot-gg', 'manually check for clear links on ice shot'],
			['guide-c-kinetist', 'BiS Clear Merc'],
			['guide-c-manyshot', 'Good Clear / Single Target'],
			['guide-c-blade-ambusher', 'Good Bossing / Single Target Merc'],
			[
				'guide-c-combatant',
				'Good All rounder / Starter Merc (better clear option as armour stack setup late game)'
			],
			['guide-e-kinetist', 'barrage is not a brick'],
			[
				'guide-e-combatant',
				'frost blades: chain > wed > hypo > faster/gmp; wild strike: return > wed > hypo > faster; NO PIERCE'
			]
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

	// `ARCHETYPES` is a hand-written list that `MercArchetype` is derived from, so
	// a ruleset naming an archetype the list forgot does not compile. The other
	// direction is silent: nothing at runtime reads `ARCHETYPES` — it exists to
	// derive the type — so a member no ruleset declares (a leftover from a removed
	// source, or one added ahead of the rulesets meant to use it) is dead
	// vocabulary that no compile and no other test would ever flag. This is the
	// only thing that notices.
	it('leaves no archetype in the list that no ruleset declares', () => {
		const declared = new Set(allRulesets().map((r) => r.archetype));
		expect(ARCHETYPES.filter((archetype) => !declared.has(archetype))).toEqual([]);
	});

	// The line under each source's name on the page. It has to say WHOSE rules
	// these are and which side of the trade they are written from, because the
	// sources disagree on purpose and a reader comparing their headlines cannot
	// otherwise tell a seller's floor from a buyer's shopping list.
	it('says whose rules each source carries and which side of the trade they take', () => {
		expect(MERC_SOURCES.map((s) => `${s.label}: ${s.description ?? 'none'}`)).toEqual([
			"ckaiba: ckaiba's seller-side floors — wealthyexile strategy 7062",
			"Nerotox: Nerotox's tiered saved searches — three videos, four ladders",
			"CaptainLance: CaptainLance's buyer-side ideal links for a Luminary merc bot — no prices, no floors",
			"XTheFarmerX: XTheFarmerX's budget life-stacking KB merc — two saved searches, the upper one Nerotox's own link",
			"sushi: sushi's buyer-side archetype notes for Allflame 3.29 — no prices, no floors",
			"Path of Evening: Path of Evening's buyer-side saved searches — three archetypes, a cheap and an expensive rung each"
		]);
	});
});

describe('buyer-contextual entries', () => {
	/**
	 * `buyerContextual` is Sebastian's selling-side ruling, not something the
	 * saved searches say — the transcription tests above cannot see it drift,
	 * because from the fixture's point of view these entries are ordinary enabled
	 * filters. So the flag's exact placement is pinned here: Haste wherever a
	 * guide gates on it, and Barrage on the mapping search that keeps it live.
	 *
	 * The two Barrage rows are that ONE search seen twice. The ruling is about
	 * the search, not about whoever published it, so guide-d's 20D rung — the
	 * same hash as guide-b's MV rung, Barrage live in a `count` of one, byte for
	 * byte — carries it too.
	 *
	 * Gating on Haste is NOT on its own enough to earn the flag: guide-f's two
	 * Kineticist rungs gate on Haste and Inspiring Cry both, and neither is
	 * listed. What the flagged ones have is the AUTHOR saying the aura is the
	 * buyer's call ("depending on your spectres"); Path of Evening publishes no
	 * prose this repo can read, so its switches are transcribed as saved rather
	 * than softened on a guess.
	 */
	it('flags Haste on every ruleset whose author calls it optional, plus Barrage on the MV search', () => {
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
			'guide-b-kinetist-gg/auras/mercenary.skill_52155',
			'guide-d-kinetist-20d/secondary/mercenary.skill_1356',
			'guide-d-kinetist-20d/auras/mercenary.skill_52155'
		]);
	});
});
