/**
 * Mercenary rulesets — the declarative data model behind the Mercenaries view page.
 *
 * Every ruleset here is a transcription of a GGG trade saved search. The raw
 * responses live in `__fixtures__/<hash>.json` (see that directory's README for
 * provenance and re-fetch commands) and `rulesets.test.ts` asserts this file
 * against them, so the fixtures — not this file — are the ground truth.
 *
 * Entry `name` values are copied verbatim from `__fixtures__/mercenary-stats.json`
 * (GGG's Mercenary stat vocabulary), `(Tier N)` suffix included. They are display
 * text, not keys: two different ids can share one name.
 *
 * The four guide-b Kinetist rungs share a group skeleton and are still written out
 * one by one. Generating them from a shared factory would make "the four rungs are
 * the same search with different switches" true by construction, and that is a fact
 * about the saved searches that the tests are supposed to be able to disprove.
 *
 * The floor prose on the guide-a rulesets is Sebastian's own market read (POE-165),
 * not something derivable from the saved search.
 */

import type { MercSavedSearch } from './trade-links';

export const SOURCE_IDS = ['guide-a', 'guide-b'] as const;
export type MercSourceId = (typeof SOURCE_IDS)[number];

export const ARCHETYPES = ['manyshot', 'kinetist', 'combatant'] as const;
export type MercArchetype = (typeof ARCHETYPES)[number];

/** Rungs of the guide-b Kinetist ladder, cheapest first. */
export const TIERS = ['mv', 'mid', 'end', 'gg'] as const;
export type MercTier = (typeof TIERS)[number];

/** `type` of a trade stat group, verbatim from the saved search JSON. */
export const GROUP_TYPES = ['and', 'not', 'mercenary', 'count'] as const;
export type MercGroupType = (typeof GROUP_TYPES)[number];

export interface MercFilterEntry {
	/** GGG stat id, e.g. 'mercenary.skill_11495'. */
	id: string;
	/** Vocabulary text verbatim, e.g. 'Return (Tier 3)'. */
	name: string;
	/** False when the saved search carries `disabled: true` on this filter. */
	enabledInSearch: boolean;
}

export interface MercFilterGroup {
	/**
	 * Stable position key, shared across rulesets that express the same slot —
	 * the four Kinetist rungs use one id sequence so they can be diffed rung to rung.
	 */
	id: string;
	/** Human header for the group. */
	label: string;
	type: MercGroupType;
	/** False when the group itself carries `disabled: true` in the saved search. */
	enabledInSearch: boolean;
	/**
	 * `value.min` when the saved search sets one. ABSENT means the group has no
	 * minimum — every enabled entry must match, not "at least N of".
	 */
	min?: number;
	entries: MercFilterEntry[];
}

export interface MercRuleset {
	id: string;
	label: string;
	archetype: MercArchetype;
	tier?: MercTier;
	savedSearch: MercSavedSearch;
	/** `query.status.option`, e.g. 'securable'. */
	status: string;
	/** `query.filters.misc_filters.filters.ilvl.min`. */
	ilvlMin?: number;
	/** Sebastian's prose price floor for a mercenary matching this ruleset. */
	floor?: string;
	groups: MercFilterGroup[];
}

export interface MercSource {
	id: MercSourceId;
	label: string;
	/** Null until a public guide URL is supplied. */
	guideUrl: string | null;
	rulesets: MercRuleset[];
}

/**
 * Role of a stat id, derived from its GGG prefix — never stored on the entry,
 * so the id stays the single source of truth.
 *
 * Throws on an id that is neither: entries come from this module's own typed
 * data, so an unrecognised prefix is a transcription bug and should be loud.
 */
export function entryRole(id: string): 'skill' | 'support' {
	if (id.startsWith('mercenary.skill_')) return 'skill';
	if (id.startsWith('mercenary.support_')) return 'support';
	throw new Error(`not a mercenary skill or support stat id: ${id}`);
}

/**
 * Support-link tier parsed from the vocabulary name's '(Tier N)' suffix.
 * Null for names without one — active skills carry no tier.
 */
export function entryTier(name: string): number | null {
	const match = /\(Tier (\d+)\)$/.exec(name);
	return match ? Number(match[1]) : null;
}

/**
 * How one entry reads to a buyer, resolved **type first**:
 *
 * 1. a `not` group's entries are always 'forbidden', whatever `enabledInSearch`
 *    says on the group or the entry;
 * 2. otherwise a switched-off group or a switched-off entry makes it a 'bonus';
 * 3. otherwise it is 'required'.
 *
 * Type wins because a switched-off denial is still a denial: the guide author
 * parked the group to widen the search, not because the stat became desirable.
 * Reading the switch first would render the guide-b MV rung's parked
 * `deny-supports` Pierce pair as a bonus — a denial list advertising the very
 * stats it exists to reject.
 */
export function entryKind(
	group: MercFilterGroup,
	entry: MercFilterEntry
): 'required' | 'forbidden' | 'bonus' {
	if (group.type === 'not') return 'forbidden';
	if (!group.enabledInSearch || !entry.enabledInSearch) return 'bonus';
	return 'required';
}

const ALLFLAME = 'Allflame';

const GUIDE_A_MANYSHOT: MercRuleset = {
	id: 'guide-a-manyshot',
	label: 'Manyshot',
	archetype: 'manyshot',
	savedSearch: { league: ALLFLAME, hash: 'WvKGjV8Kfm' },
	status: 'securable',
	ilvlMin: 83,
	floor: '5d+ with just Return on both',
	groups: [
		{
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_2792', name: 'Grace', enabledInSearch: false },
				{ id: 'mercenary.skill_58425', name: 'Vaal Grace', enabledInSearch: false },
				{ id: 'mercenary.skill_24482', name: 'Hatred', enabledInSearch: false }
			]
		},
		{
			id: 'core',
			label: 'Ice Shot + links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			id: 'secondary',
			label: 'Vaal Ice Shot + links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true },
				{
					id: 'mercenary.support_48875',
					name: 'Cooldown Recovery (Tier 2)',
					enabledInSearch: false
				}
			]
		},
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_24409', name: 'Icicle Rain', enabledInSearch: true },
				{ id: 'mercenary.skill_18232', name: 'Frigid Forkshot', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_A_KINETIST_V1: MercRuleset = {
	id: 'guide-a-kinetist-v1',
	label: 'Kinetist v1',
	archetype: 'kinetist',
	savedSearch: { league: ALLFLAME, hash: 'LgkKKmllTn' },
	status: 'securable',
	ilvlMin: 83,
	floor: '15d+ with just Return',
	groups: [
		{
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: true }]
		},
		{
			id: 'core',
			label: 'Core skill + links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_12583', name: 'Kinetic Bolt', enabledInSearch: true },
				{ id: 'mercenary.skill_32089', name: 'Kinetic Rain of Impact', enabledInSearch: true },
				{ id: 'mercenary.skill_1356', name: 'Barrage', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_A_COMBATANT: MercRuleset = {
	id: 'guide-a-combatant',
	label: 'Combatant',
	archetype: 'combatant',
	savedSearch: { league: ALLFLAME, hash: '5nd22GvKCa' },
	status: 'securable',
	ilvlMin: 83,
	floor: '3d+ with just Return, much more with Multistrike/GMP',
	groups: [
		{
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_38326', name: 'Wrath', enabledInSearch: false },
				{ id: 'mercenary.skill_13693', name: 'Purity of Ice', enabledInSearch: false }
			]
		},
		{
			id: 'core',
			label: 'Frost Blades + links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_62638', name: 'Multistrike (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_25973',
					name: 'Greater Multistrike (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_12054',
					name: 'Multiple Projectiles (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			id: 'secondary',
			label: 'Alternate strike skill',
			type: 'mercenary',
			enabledInSearch: true,
			min: 1,
			entries: [
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true },
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: false }
			]
		},
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_8708', name: 'Elemental Hit of Ice', enabledInSearch: true },
				{ id: 'mercenary.skill_37916', name: 'Spectral Helix', enabledInSearch: true },
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_B_KINETIST_MV: MercRuleset = {
	id: 'guide-b-kinetist-mv',
	label: 'Kinetist',
	archetype: 'kinetist',
	tier: 'mv',
	savedSearch: { league: ALLFLAME, hash: '7nRvBzl2S5' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32089', name: 'Kinetic Rain of Impact', enabledInSearch: true },
				{ id: 'mercenary.skill_12583', name: 'Kinetic Bolt', enabledInSearch: true },
				{ id: 'mercenary.skill_26705', name: 'Power Siphon', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Core skill + links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			id: 'secondary',
			label: 'Additional skills',
			type: 'count',
			enabledInSearch: true,
			min: 1,
			entries: [
				{ id: 'mercenary.skill_44258', name: 'Greater Kinetic Blast', enabledInSearch: true },
				{ id: 'mercenary.skill_1356', name: 'Barrage', enabledInSearch: true }
			]
		},
		{
			id: 'behavior',
			label: 'Projectile behaviour links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_27970',
					name: 'Greater Pierce (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-supports',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_27970',
					name: 'Greater Pierce (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: true }]
		},
		{
			id: 'damage',
			label: 'Damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_55659',
					name: 'Greater Critical Damage (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32189', name: 'Critical Damage (Tier 2)', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_B_KINETIST_MID: MercRuleset = {
	id: 'guide-b-kinetist-mid',
	label: 'Kinetist',
	archetype: 'kinetist',
	tier: 'mid',
	savedSearch: { league: ALLFLAME, hash: 'BgzkZKGQF8' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32089', name: 'Kinetic Rain of Impact', enabledInSearch: true },
				{ id: 'mercenary.skill_12583', name: 'Kinetic Bolt', enabledInSearch: true },
				{ id: 'mercenary.skill_26705', name: 'Power Siphon', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Core skill + links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			id: 'secondary',
			label: 'Additional skills',
			type: 'count',
			enabledInSearch: true,
			min: 1,
			entries: [
				{ id: 'mercenary.skill_44258', name: 'Greater Kinetic Blast', enabledInSearch: true },
				{ id: 'mercenary.skill_1356', name: 'Barrage', enabledInSearch: false }
			]
		},
		{
			id: 'behavior',
			label: 'Projectile behaviour links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_27970',
					name: 'Greater Pierce (Tier 3)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-supports',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_27970',
					name: 'Greater Pierce (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: false,
			entries: [{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: true }]
		},
		{
			id: 'damage',
			label: 'Damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_55659',
					name: 'Greater Critical Damage (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32189', name: 'Critical Damage (Tier 2)', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_B_KINETIST_END: MercRuleset = {
	id: 'guide-b-kinetist-end',
	label: 'Kinetist',
	archetype: 'kinetist',
	tier: 'end',
	savedSearch: { league: ALLFLAME, hash: 'LgkGrPO5Fn' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32089', name: 'Kinetic Rain of Impact', enabledInSearch: true },
				{ id: 'mercenary.skill_12583', name: 'Kinetic Bolt', enabledInSearch: true },
				{ id: 'mercenary.skill_26705', name: 'Power Siphon', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Core skill + links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			id: 'secondary',
			label: 'Additional skills',
			type: 'count',
			enabledInSearch: true,
			min: 1,
			entries: [
				{ id: 'mercenary.skill_44258', name: 'Greater Kinetic Blast', enabledInSearch: true },
				{ id: 'mercenary.skill_1356', name: 'Barrage', enabledInSearch: false }
			]
		},
		{
			id: 'behavior',
			label: 'Projectile behaviour links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_27970',
					name: 'Greater Pierce (Tier 3)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-supports',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_27970',
					name: 'Greater Pierce (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: true }]
		},
		{
			id: 'damage',
			label: 'Damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_55659',
					name: 'Greater Critical Damage (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32189', name: 'Critical Damage (Tier 2)', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_B_KINETIST_GG: MercRuleset = {
	id: 'guide-b-kinetist-gg',
	label: 'Kinetist',
	archetype: 'kinetist',
	tier: 'gg',
	savedSearch: { league: ALLFLAME, hash: 'zbrQyEqah4' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32089', name: 'Kinetic Rain of Impact', enabledInSearch: true },
				{ id: 'mercenary.skill_12583', name: 'Kinetic Bolt', enabledInSearch: true },
				{ id: 'mercenary.skill_26705', name: 'Power Siphon', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Core skill + links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'secondary',
			label: 'Additional skills',
			type: 'count',
			enabledInSearch: true,
			min: 1,
			entries: [
				{ id: 'mercenary.skill_44258', name: 'Greater Kinetic Blast', enabledInSearch: true },
				{ id: 'mercenary.skill_1356', name: 'Barrage', enabledInSearch: false }
			]
		},
		{
			id: 'behavior',
			label: 'Projectile behaviour links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_27970',
					name: 'Greater Pierce (Tier 3)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-supports',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_27970',
					name: 'Greater Pierce (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: true }]
		},
		{
			id: 'damage',
			label: 'Damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_55659',
					name: 'Greater Critical Damage (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32189', name: 'Critical Damage (Tier 2)', enabledInSearch: true }
			]
		}
	]
};

export const MERC_SOURCES: MercSource[] = [
	{
		id: 'guide-a',
		label: 'Guide A',
		guideUrl: null,
		rulesets: [GUIDE_A_MANYSHOT, GUIDE_A_KINETIST_V1, GUIDE_A_COMBATANT]
	},
	{
		id: 'guide-b',
		label: 'Guide B',
		guideUrl: null,
		rulesets: [
			GUIDE_B_KINETIST_MV,
			GUIDE_B_KINETIST_MID,
			GUIDE_B_KINETIST_END,
			GUIDE_B_KINETIST_GG
		]
	}
];

/** Every ruleset across every source, in declaration order. */
export function allRulesets(): MercRuleset[] {
	return MERC_SOURCES.flatMap((source) => source.rulesets);
}

/** The guide-b Kinetist ladder, cheapest rung first — the cross-tier comparison set. */
export function kinetistLadder(): MercRuleset[] {
	return allRulesets().filter((r) => r.tier !== undefined);
}
