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
 * Guide-b is Nerotox's YouTube CHANNEL, not one video: the source URL is the
 * channel and every tiered ruleset carries the `guideUrl` of the video whose
 * description published its trade link. Two ladders are transcribed so far — the
 * Kinetist ladder (video 2026-08-08) and the Manyshot ladder (video 2026-07-29).
 * Audited 2026-08-26: each description's only rules are the links themselves plus
 * prose notes, all reflected — Barrage as an acceptable secondary (the Kinetist
 * `secondary` count group), Haste tuned to the buyer's spectres, and the two GG
 * rungs' `authorNote`, which `verdict.ts` relays verbatim on a pass. The Haste
 * ruling is `buyerContextual` wherever the search gates on it — all four
 * Kinetist rungs here, and guide-a's Manyshot aura group; the only place Haste
 * is a plain switched-off bonus is the Manyshot mid rung, whose search simply
 * does not ask for it.
 *
 * The rungs of a ladder are written out one by one even where they share a group
 * skeleton. Generating them from a shared factory would make "the rungs are the
 * same search with different switches" true by construction, and that is a fact
 * about the saved searches that the tests are supposed to be able to disprove.
 *
 * The floor prose on the guide-a rulesets is the guide author's (ckaiba, wealthyexile
 * strategy 7062, "Worthy Mercenaries" section, last updated 2026-08-01), transcribed —
 * not something derivable from the saved search. Audited against the page 2026-08-26:
 * the three deny lists and required links match; the saved searches carry extras the
 * prose does not mention (Frigid Forkshot and Barrage denied, aura toggles), and the
 * search wins because it is what the author actually comps against.
 */

import type { MercSavedSearch } from './trade-links';

export const SOURCE_IDS = ['guide-a', 'guide-b'] as const;
export type MercSourceId = (typeof SOURCE_IDS)[number];

export const ARCHETYPES = ['manyshot', 'kinetist', 'combatant'] as const;
export type MercArchetype = (typeof ARCHETYPES)[number];

/**
 * Rungs of a guide-b tier ladder, cheapest first — the ranking order for every
 * ladder, not a claim that a ladder has exactly four rungs or one rung per tier.
 */
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
	/**
	 * Set on an entry a BUYER may or may not want, whatever the saved search
	 * switches say — a selling-side ruling, not a fact about the search.
	 *
	 * The verdict engine never lets such an entry fail a group: present it
	 * counts and is flagged, absent it drops out of the group's need. The flag
	 * is additive, so the fixture-fidelity tests above it stay untouched.
	 */
	buyerContextual?: true;
}

export interface MercFilterGroup {
	/**
	 * Stable position key, shared across rulesets that express the same slot —
	 * a ladder's rungs reuse one id per slot so they can be diffed rung to rung,
	 * including where a rung merges two slots into one group (the Manyshot GG
	 * rung's `projectiles`) or drops one entirely.
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
	/**
	 * Which tier ladder this ruleset is a rung of — set on every tiered ruleset,
	 * absent on the untiered ones.
	 *
	 * NOT the archetype: one author can publish two ladders for the same
	 * archetype (Nerotox's Combatant video has a Frost Blades ladder and a Wild
	 * Strike one), so the archetype cannot key the grouping.
	 */
	ladder?: string;
	tier?: MercTier;
	savedSearch: MercSavedSearch;
	/**
	 * The guide page or video that published THIS ruleset's trade link, when it
	 * is not simply the source's own URL. A source whose rulesets all come from
	 * one page leaves this absent and inherits `MercSource.guideUrl`.
	 */
	guideUrl?: string;
	/**
	 * A note the guide's author wrote about this specific search, verbatim.
	 * `verdict.ts` relays it on a pass — it is the author speaking, not a rule
	 * the app derived, so it is never paraphrased and never computed.
	 */
	authorNote?: string;
	/** `query.status.option`, e.g. 'securable'. */
	status: string;
	/** `query.filters.misc_filters.filters.ilvl.min`. */
	ilvlMin?: number;
	/** The guide author's prose price floor for a mercenary matching this ruleset. */
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

/** "How to search for a good Kineticist Mercenary | PoE 3.29 Allflame", 2026-08-08. */
const NEROTOX_KINETIST_VIDEO = 'https://www.youtube.com/watch?v=HKTVN4sENvg';
/** "How to search for a good Manyshot Mercenary | PoE 3.29 Allflame", 2026-07-29. */
const NEROTOX_MANYSHOT_VIDEO = 'https://www.youtube.com/watch?v=ljaXlGLdyxM';

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
			entries: [
				{
					id: 'mercenary.skill_52155',
					name: 'Haste',
					enabledInSearch: true,
					// Provenance: guide A's own saved search gates on Haste — the switch
					// above is transcribed, not softened. The demotion to contextual is
					// Sebastian's selling-side ruling (POE-165 "Kinetist tier ladder",
					// slice-1 comment item 2): guide B's author tunes Haste to their
					// spectres, so a merc without it still sells. Not a fixture fact.
					buyerContextual: true
				}
			]
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
	ladder: 'kinetist',
	tier: 'mv',
	savedSearch: { league: ALLFLAME, hash: '7nRvBzl2S5' },
	guideUrl: NEROTOX_KINETIST_VIDEO,
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
				// The guide calls Barrage its experimental toggle, live only on this
				// rung — a mapping merc with Barrage instead of Greater Kinetic Blast
				// still sells, so it counts toward the minimum but never fails it.
				{
					id: 'mercenary.skill_1356',
					name: 'Barrage',
					enabledInSearch: true,
					buyerContextual: true
				}
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
			// Buyer-contextual on every rung: the guide tells its readers to enable
			// Haste "depending on your spectres", so it widens the buyer pool
			// instead of gating the sale (POE-165 "Kinetist tier ladder").
			entries: [
				{
					id: 'mercenary.skill_52155',
					name: 'Haste',
					enabledInSearch: true,
					buyerContextual: true
				}
			]
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
	ladder: 'kinetist',
	tier: 'mid',
	savedSearch: { league: ALLFLAME, hash: 'BgzkZKGQF8' },
	guideUrl: NEROTOX_KINETIST_VIDEO,
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
			// Buyer-contextual on every rung: the guide tells its readers to enable
			// Haste "depending on your spectres", so it widens the buyer pool
			// instead of gating the sale (POE-165 "Kinetist tier ladder").
			entries: [
				{
					id: 'mercenary.skill_52155',
					name: 'Haste',
					enabledInSearch: true,
					buyerContextual: true
				}
			]
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
	ladder: 'kinetist',
	tier: 'end',
	savedSearch: { league: ALLFLAME, hash: 'LgkGrPO5Fn' },
	guideUrl: NEROTOX_KINETIST_VIDEO,
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
			// Buyer-contextual on every rung: the guide tells its readers to enable
			// Haste "depending on your spectres", so it widens the buyer pool
			// instead of gating the sale (POE-165 "Kinetist tier ladder").
			entries: [
				{
					id: 'mercenary.skill_52155',
					name: 'Haste',
					enabledInSearch: true,
					buyerContextual: true
				}
			]
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
	ladder: 'kinetist',
	tier: 'gg',
	savedSearch: { league: ALLFLAME, hash: 'zbrQyEqah4' },
	guideUrl: NEROTOX_KINETIST_VIDEO,
	// Verbatim from the video description's GG line, joined into one note.
	authorNote:
		'look at damage links still - these are 4L not even 5L. 5L mercs nearly never exist for KB in gg setups, a 5L with barrage can beat a 4L with greater KB',
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
			// Buyer-contextual on every rung: the guide tells its readers to enable
			// Haste "depending on your spectres", so it widens the buyer pool
			// instead of gating the sale (POE-165 "Kinetist tier ladder").
			entries: [
				{
					id: 'mercenary.skill_52155',
					name: 'Haste',
					enabledInSearch: true,
					buyerContextual: true
				}
			]
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

/**
 * Nerotox's Manyshot ladder (video 2026-07-29, `ljaXlGLdyxM`). The author's own
 * rung names are Earlygame / Midgame / Endgame / GG; `mv` is the `TIERS` key
 * this file spells "Earlygame" with, so the two ladders share one column order.
 *
 * Two facts about these searches that the Kinetist ladder does not have:
 *
 * 1. NO ilvl floor. All four omit `filters` entirely, so `ilvlMin` is absent
 *    rather than 83 — the guide-a searches and the Kinetist ladder set one.
 * 2. The Earlygame rung has EVERY `mercenary` group switched off. Its only live
 *    gates are "has Vaal Ice Shot" and "no Icicle Rain", so it passes any Vaal
 *    Ice Shot mercenary. That is what the saved search says, not a transcription
 *    slip: the rung is the author's widest net, and its parked groups are the
 *    vocabulary the higher rungs switch on.
 */
const GUIDE_B_MANYSHOT_MV: MercRuleset = {
	id: 'guide-b-manyshot-mv',
	label: 'Manyshot',
	archetype: 'manyshot',
	ladder: 'manyshot',
	tier: 'mv',
	savedSearch: { league: ALLFLAME, hash: '4mP3V2jQT9' },
	guideUrl: NEROTOX_MANYSHOT_VIDEO,
	status: 'securable',
	groups: [
		{
			id: 'has-vaal',
			label: 'Carries Vaal Ice Shot',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true }]
		},
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_24409', name: 'Icicle Rain', enabledInSearch: true }]
		},
		{
			id: 'vaal-damage',
			label: 'Vaal Ice Shot + damage links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
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
				}
			]
		},
		{
			id: 'vaal-return',
			label: 'Vaal Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'projectiles',
			label: 'Ice Shot + projectile links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Ice Shot + damage links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: true
				}
			]
		}
	]
};

const GUIDE_B_MANYSHOT_MID: MercRuleset = {
	id: 'guide-b-manyshot-mid',
	label: 'Manyshot',
	archetype: 'manyshot',
	ladder: 'manyshot',
	tier: 'mid',
	savedSearch: { league: ALLFLAME, hash: 'Z6Em09GmHQ' },
	guideUrl: NEROTOX_MANYSHOT_VIDEO,
	status: 'securable',
	groups: [
		{
			id: 'has-vaal',
			label: 'Carries Vaal Ice Shot',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true }]
		},
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_24409', name: 'Icicle Rain', enabledInSearch: true }]
		},
		{
			id: 'vaal-damage',
			label: 'Vaal Ice Shot + damage links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
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
				}
			]
		},
		{
			id: 'vaal-return',
			label: 'Vaal Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'projectiles',
			label: 'Ice Shot + projectile links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Ice Shot + damage links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'auras',
			label: 'Auras',
			type: 'count',
			enabledInSearch: true,
			min: 1,
			entries: [
				{ id: 'mercenary.skill_2792', name: 'Grace', enabledInSearch: true },
				{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: false },
				{ id: 'mercenary.skill_24482', name: 'Hatred', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_B_MANYSHOT_END: MercRuleset = {
	id: 'guide-b-manyshot-end',
	label: 'Manyshot',
	archetype: 'manyshot',
	ladder: 'manyshot',
	tier: 'end',
	savedSearch: { league: ALLFLAME, hash: 'JBnK2YKRFl' },
	guideUrl: NEROTOX_MANYSHOT_VIDEO,
	status: 'securable',
	groups: [
		{
			id: 'has-vaal',
			label: 'Carries Vaal Ice Shot',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true }]
		},
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_24409', name: 'Icicle Rain', enabledInSearch: true }]
		},
		{
			id: 'vaal-damage',
			label: 'Vaal Ice Shot + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
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
				}
			]
		},
		{
			id: 'vaal-return',
			label: 'Vaal Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'projectiles',
			label: 'Ice Shot + projectile links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Ice Shot + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'auras',
			label: 'Auras',
			type: 'count',
			enabledInSearch: true,
			min: 1,
			entries: [
				{ id: 'mercenary.skill_2792', name: 'Grace', enabledInSearch: true },
				{ id: 'mercenary.skill_24482', name: 'Hatred', enabledInSearch: true }
			]
		}
	]
};

/**
 * The GG rung DRIFTS from the other three, and every difference below is
 * transcribed, not tidied:
 *
 * - the aura `count` group comes FIRST and carries a third option (Frost Bomb);
 * - there is no `and` "carries Vaal Ice Shot" group at all — the live
 *   `vaal-damage` and `vaal-return` groups already require the skill;
 * - `vaal-damage` drops the parked Return entry and asks for 3, not 2;
 * - `projectiles` and `damage` are MERGED into one parked `min: 4` group over
 *   both vocabularies, transcribed under the `projectiles` id it holds the
 *   position of — so the matrix shows the damage entries as holes in the other
 *   three rungs and no `damage` row for this one;
 * - `core` (Ice Shot + Return) is the last group and the only one this rung
 *   switches ON out of the Ice-Shot pair.
 */
const GUIDE_B_MANYSHOT_GG: MercRuleset = {
	id: 'guide-b-manyshot-gg',
	label: 'Manyshot',
	archetype: 'manyshot',
	ladder: 'manyshot',
	tier: 'gg',
	savedSearch: { league: ALLFLAME, hash: 'd86ymvXRsJ' },
	guideUrl: NEROTOX_MANYSHOT_VIDEO,
	authorNote: 'manually check for clear links on ice shot',
	status: 'securable',
	groups: [
		{
			id: 'auras',
			label: 'Auras',
			type: 'count',
			enabledInSearch: true,
			min: 1,
			entries: [
				{ id: 'mercenary.skill_24482', name: 'Hatred', enabledInSearch: true },
				{ id: 'mercenary.skill_2792', name: 'Grace', enabledInSearch: true },
				{ id: 'mercenary.skill_10557', name: 'Frost Bomb', enabledInSearch: true }
			]
		},
		{
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_24409', name: 'Icicle Rain', enabledInSearch: true }]
		},
		{
			id: 'vaal-damage',
			label: 'Vaal Ice Shot + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
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
				}
			]
		},
		{
			id: 'vaal-return',
			label: 'Vaal Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'projectiles',
			label: 'Ice Shot + projectile and damage links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 4,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true },
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
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'core',
			label: 'Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		}
	]
};

export const MERC_SOURCES: MercSource[] = [
	{
		id: 'guide-a',
		label: 'Guide A',
		guideUrl: 'https://wealthyexile.com/strategies/7062/alchgo_astrolabe__merc_boss_rushing',
		rulesets: [GUIDE_A_MANYSHOT, GUIDE_A_KINETIST_V1, GUIDE_A_COMBATANT]
	},
	{
		id: 'guide-b',
		label: 'Guide B',
		// The CHANNEL: this source's ladders come from different videos, and each
		// rung names its own (`MercRuleset.guideUrl`).
		guideUrl: 'https://www.youtube.com/channel/UCqIRIXItoDOlET2oeFn6WKA',
		rulesets: [
			GUIDE_B_KINETIST_MV,
			GUIDE_B_KINETIST_MID,
			GUIDE_B_KINETIST_END,
			GUIDE_B_KINETIST_GG,
			GUIDE_B_MANYSHOT_MV,
			GUIDE_B_MANYSHOT_MID,
			GUIDE_B_MANYSHOT_END,
			GUIDE_B_MANYSHOT_GG
		]
	}
];

/** Every ruleset across every source, in declaration order. */
export function allRulesets(): MercRuleset[] {
	return MERC_SOURCES.flatMap((source) => source.rulesets);
}

/**
 * A source's tier ladders: one array per `ladder` key, rungs cheapest first.
 *
 * Ladders come back in the order their first rung is declared, and the rungs of
 * each are sorted by `TIERS` — a stable sort, so two rungs sharing one tier keep
 * their declaration order rather than one of them winning arbitrarily. Nothing
 * here assumes four rungs or one rung per tier: Nerotox's Combatant video
 * publishes two Endgame links for the same ladder.
 *
 * A ruleset with a tier but no ladder key is NOT a rung of anything — it would
 * have no column set to be compared in. `rulesets.test.ts` pins that no such
 * ruleset is declared.
 */
export function ladders(source: MercSource): MercRuleset[][] {
	const byKey = new Map<string, MercRuleset[]>();
	for (const ruleset of source.rulesets) {
		if (ruleset.ladder === undefined || ruleset.tier === undefined) continue;
		const rungs = byKey.get(ruleset.ladder);
		if (rungs) rungs.push(ruleset);
		else byKey.set(ruleset.ladder, [ruleset]);
	}
	return [...byKey.values()].map((rungs) =>
		[...rungs].sort((a, b) => TIERS.indexOf(a.tier!) - TIERS.indexOf(b.tier!))
	);
}
