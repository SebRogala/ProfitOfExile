/**
 * Mercenary rulesets — the declarative data model behind the Mercenaries view page.
 *
 * SIX sources, and they do not all come from the same kind of thing. Guide-a,
 * guide-b, guide-d and guide-f are transcriptions of GGG trade SAVED SEARCHES:
 * the raw responses live in `__fixtures__/<hash>.json` (see that README for
 * provenance and re-fetch commands) and `rulesets.test.ts` asserts this file
 * against them, so the fixtures — not this file — are the ground truth.
 * Guide-c and guide-e are transcriptions of PROSE: CaptainLance's "Ideal Merc
 * Options" names skills and support links in sentences, sushi's archetype notes
 * name them in shorthand, and neither publishes a trade link at all — so those
 * ten rulesets carry an `authored` fixture instead of a `savedSearch` and their
 * fixture is OUR OWN output, not GGG's. It is committed and asserted like the
 * others because it still catches a typed-model edit nobody meant to make, but
 * it cannot catch a MISREADING of the guide — only a human re-reading the source
 * can, and the prose is quoted in the group comments below so that re-read is
 * possible without leaving the file.
 *
 * The two kinds also point in opposite directions. Guide-a's rulesets are
 * seller-side (its author states price floors), guide-b's, guide-d's and
 * guide-f's are buyers' tier ladders — guide-f's read off the absence of a
 * price floor on any of its six rungs, since its prose is unreadable — and
 * guide-c and guide-e are buyers' IDEALS:
 * CaptainLance is telling a Luminary merc-bot player what links to look for,
 * with no prices and no floors, and sushi is doing the same across six
 * archetypes. So a guide-c or guide-e pass says "this mercenary is what the
 * build wants", never "this is what it is worth".
 *
 * Entry `name` values are copied verbatim from `__fixtures__/mercenary-stats.json`
 * (GGG's Mercenary stat vocabulary), `(Tier N)` suffix included. They are display
 * text, not keys: two different ids can share one name.
 *
 * Guide-b is Nerotox's YouTube CHANNEL, not one video: the source URL is the
 * channel and every one of ITS rungs carries the `guideUrl` of the video whose
 * description published its trade link. Three videos, four ladders so far — the
 * Kinetist ladder (2026-08-08), the Manyshot ladder (2026-07-29), and the Frost
 * Blades and Wild Strike ladders that ONE Combatant video (2026-08-08) publishes
 * between them.
 * Audited 2026-08-26: each description's only rules are the links themselves plus
 * prose notes, all reflected — Barrage as an acceptable secondary (the Kinetist
 * `secondary` count group), Haste tuned to the buyer's spectres, and the
 * `authorNote` that two of the four GG rungs carry, which `verdict.ts` relays
 * verbatim on a pass. The Haste
 * ruling is `buyerContextual` wherever the search gates on it — all four
 * Kinetist rungs here, and guide-a's Kinetist aura group; Haste is a plain
 * switched-off bonus where a source simply does not gate on it — the Manyshot
 * mid rung's search, and guide-c's Kinetist buff group.
 *
 * Guide-d is the other shape — one video for both its rungs, so they inherit
 * the source URL and carry no `guideUrl` of their own.
 *
 * The Combatant description carries one prose note and NO rung takes it as an
 * `authorNote`, because it is about the nine searches together rather than any
 * one of them: "Please play around yourself with the trade filters as well to
 * search for greater supports, these are only starting points, you can
 * definitely optimize the searches for whatever you are looking for still
 * (moveskill/auras)." An `authorNote` rides a single rung's verdict, so relaying
 * it there would put a video-wide caveat on whichever rung happened to pass.
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
 *
 * Guide-c and guide-e have no such audit to do: there is no saved search to
 * disagree with the prose, so the prose IS the transcription and every group of
 * theirs carries the sentence — or, for guide-e, the shorthand line — it came
 * from.
 *
 * Guide-d is XTheFarmerX's budget life-stacking Kinetic Blast build, published
 * as ONE video ("5 DIVINE BUDGET LIFE STACKING KB MERC BUILD | Trade Links -
 * Crafting - Merc Warrants", 2026-08-14) with two saved searches in it: a
 * buyer's two-point ladder, and the shortest source here. It publishes no price
 * for either rung — Sebastian measured the live listings 2026-08-28, budget from
 * ~5d and 20D from ~9d against the ~20c the video shows at recording — so
 * neither rung carries a `floor`, the guide-b reading rather than guide-a's.
 *
 * Its 20D rung's hash is `7nRvBzl2S5`, which guide-b already declares: the
 * video's linked sheet republishes Nerotox's own Kinetist MV link six days after
 * it went up, so the two sources transcribe ONE saved search and share ONE
 * fixture file. That is provenance rather than duplication — two guides
 * endorsing one search is a fact about the market, and a mercenary passing it
 * reads WORTH from both of them.
 *
 * Guide-e is sushi's archetype notes, the other authored source and the widest
 * one: six archetypes, three of which — Sniper, Cruel Mistress and Stormhand —
 * no other source here covers. It is also the only source whose text never
 * reached this repo as text: it is a screenshot embedded in a Google Sheet cell,
 * so the quoted lines in its group comments are a reading of a PICTURE and the
 * jargon in them ("TS", "wed", "gilded +2 proj") had to be resolved against
 * GGG's vocabulary and poewiki's class pools before anything could be typed.
 * Exactly two of its rulesets carry an `authorNote`: the Combatant's, holding
 * its two support RANKINGS and the shouted "NO PIERCE" (the denial itself is a
 * live `not` group — what the note keeps is the shout), and the Kineticist's,
 * holding its one remark about a skill no group asks for.
 *
 * Guide-f is the "Path of Evening" mercenary-support build page: ONE page, six
 * saved searches, three archetypes with a cheap and an expensive rung each, so
 * its rungs inherit the source URL the way guide-d's do. It is the only source
 * here whose PROSE this repo cannot read — the page answers 403 to every fetch
 * (Cloudflare), and the six hashes were pasted by the owner 2026-09-04 and then
 * fetched from GGG by hash. The fixtures are therefore as good as any other
 * saved search here; what is missing is the author explaining himself, and that
 * absence is a ruling: no `buyerContextual` anywhere in guide-f. The flag is a
 * selling-side call that needs the author saying an entry is optional, and the
 * Kineticist rungs' Haste and Inspiring Cry — live gates in the lead group —
 * are transcribed exactly as saved rather than softened on a guess.
 *
 * Its two rungs per ladder differ by ONE lever on Combatant and on Kineticist:
 * the `[core skill, Return (Tier 3)]` group, parked at Cheap and live at
 * Expensive, and nothing else. Multishot — this author's spelling of Manyshot —
 * is the exception and moves three things at once; its rung comments say which.
 */

import type { MercAuthoredQuery, MercSavedSearch } from './trade-links';

export const SOURCE_IDS = [
	'guide-a',
	'guide-b',
	'guide-c',
	'guide-d',
	'guide-e',
	'guide-f'
] as const;
export type MercSourceId = (typeof SOURCE_IDS)[number];

export const ARCHETYPES = [
	'manyshot',
	'kinetist',
	'combatant',
	'blade-ambusher',
	'sniper',
	'cruel-mistress',
	'stormhand'
] as const;
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

/**
 * Everything a ruleset says that does not depend on where its query came from.
 *
 * Split out of `MercRuleset` so the saved-versus-authored distinction can be a
 * UNION rather than two optional fields: a ruleset must name exactly one oracle,
 * and "neither" or "both" have to fail to compile. See `MercRuleset`.
 */
interface MercRulesetFacts {
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
	/**
	 * Column head and verdict wording for a rung whose tier key does not name it
	 * on its own, spelt the way the guide spells it. Absent wherever the tier
	 * already names the rung, and a rung without it reads exactly as it did
	 * before this key existed.
	 *
	 * It exists because a ladder may publish TWO rungs at one tier: Nerotox's
	 * Frost Blades ladder has an "Endgame (no return)" and an "Endgame (return)",
	 * and `TIERS` spells both of them 'endgame'.
	 */
	tierLabel?: string;
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

/**
 * One ruleset, plus the ONE oracle it is checked against.
 *
 * `savedSearch` — the GGG search this is a transcription of, addressed by the
 * hash GGG issued. Re-fetchable, and the fixture under that hash is the ground
 * truth for every switch below.
 *
 * `authored` — a query this app WROTE from a guide's prose, addressed by the
 * fixture file it is committed as (`MercAuthoredQuery`). There is no saved
 * search to link, so `verdict.ts` reports a null `savedUrl` for these and the
 * page draws no "open saved search" link; the DERIVED link still works, because
 * `rulesetQuery` builds from the data model and never needs a hash.
 *
 * Exactly one, enforced by the type: a ruleset naming both would have two
 * disagreeing oracles, and one naming neither could not be checked at all.
 */
export type MercRuleset = MercRulesetFacts &
	(
		| { savedSearch: MercSavedSearch; authored?: never }
		| { authored: MercAuthoredQuery; savedSearch?: never }
	);

export interface MercSource {
	id: MercSourceId;
	label: string;
	/**
	 * What this source IS, in one line, shown under its name on the page.
	 *
	 * Whose rules these are and which side of the trade they are written from —
	 * the sources disagree on purpose (`verdict.ts` never merges them), and a
	 * reader looking at a row of headlines needs to know that guide-a quotes
	 * seller floors while guide-c quotes a buyer's shopping list.
	 */
	description?: string;
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

/**
 * Is this entry the ROW ANCHOR of its group — the skill a `mercenary` group is
 * row-scoped to?
 *
 * A `mercenary` group asks about one skill row (`verdict.ts::rowSatisfies`), and
 * its skill entry is what names that row. Such an entry is present in every
 * capture the group can say anything about, so it carries no information about
 * the mercenary: reading it as a fired bonus made every parked `mercenary` group
 * fire on any capture carrying the skill, which revived the group in the derived
 * search with its `min` intact over the one filter that had actually fired —
 * a query no listing can satisfy (measured 2026-08-26 on Frost Blades Minimum).
 *
 * Only `mercenary` groups. A skill in an `and` or `count` group is a real bonus
 * about the whole mercenary, not a row label — guide-a's parked Manyshot aura
 * trio is exactly that, and it must keep firing.
 */
export function isRowAnchor(groupType: MercGroupType, entryId: string): boolean {
	return groupType === 'mercenary' && entryRole(entryId) === 'skill';
}

/**
 * Which `__fixtures__/<name>.json` is this ruleset's oracle — its saved-search
 * hash, or the file an authored query names.
 *
 * One function rather than one per test file: `rulesets.test.ts` and
 * `trade-links.test.ts` both look a ruleset's fixture up, and two copies of
 * "hash, unless authored" is exactly where a guide-c ruleset would end up
 * validated against a guide-b search.
 */
export function oracleFixture(ruleset: MercRuleset): string {
	return ruleset.savedSearch !== undefined ? ruleset.savedSearch.hash : ruleset.authored.file;
}

const ALLFLAME = 'Allflame';

/** "How to search for a good Kineticist Mercenary | PoE 3.29 Allflame", 2026-08-08. */
const NEROTOX_KINETIST_VIDEO = 'https://www.youtube.com/watch?v=HKTVN4sENvg';
/** "How to search for a good Manyshot Mercenary | PoE 3.29 Allflame", 2026-07-29. */
const NEROTOX_MANYSHOT_VIDEO = 'https://www.youtube.com/watch?v=ljaXlGLdyxM';
/** "How to search for a good Combatant Mercenary | PoE 3.29 Allflame", 2026-08-08. */
const NEROTOX_COMBATANT_VIDEO = 'https://www.youtube.com/watch?v=45aM9242Umo';

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
 * this file spells "Earlygame" with, so all four ladders share one column
 * order.
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

/**
 * Nerotox's Combatant video (2026-08-08, `45aM9242Umo`) publishes TWO ladders;
 * this is the first. Frost Blades, five rungs — the author's own names are
 * Minimum / Midgame / Endgame (no return) / Endgame (return) / GG Merc.
 *
 * It is NOT four rungs one per tier: the two Endgame links are SIBLINGS at
 * `end`, not one nested inside the other. Each is Midgame plus exactly one
 * extra live group — `speed` on one, `return` on the other — and GG is the rung
 * that switches both on, so a mercenary passing both Endgame rungs necessarily
 * passes GG. `tierLabel` is what keeps the two columns and the two verdict
 * lines apart, since `TIERS` alone spells both of them 'endgame'.
 *
 * All five rungs share ONE six-group skeleton in one order, and the only things
 * that move across them are the `damage` minimum and those two switches.
 */
const GUIDE_B_FROST_BLADES_MV: MercRuleset = {
	id: 'guide-b-frost-blades-mv',
	label: 'Frost Blades',
	archetype: 'combatant',
	ladder: 'frost-blades',
	tier: 'mv',
	savedSearch: { league: ALLFLAME, hash: 'Kld4gv0Pi5' },
	guideUrl: NEROTOX_COMBATANT_VIDEO,
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32807', name: 'Herald of Ice', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Frost Blades + Chain',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-pierce',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_27970', name: 'Greater Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Frost Blades + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
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
			id: 'speed',
			label: 'Frost Blades + attack speed links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'return',
			label: 'Frost Blades + Return',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_B_FROST_BLADES_MID: MercRuleset = {
	id: 'guide-b-frost-blades-mid',
	label: 'Frost Blades',
	archetype: 'combatant',
	ladder: 'frost-blades',
	tier: 'mid',
	savedSearch: { league: ALLFLAME, hash: 'Kld4gM7yi5' },
	guideUrl: NEROTOX_COMBATANT_VIDEO,
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32807', name: 'Herald of Ice', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Frost Blades + Chain',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-pierce',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_27970', name: 'Greater Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Frost Blades + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
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
			id: 'speed',
			label: 'Frost Blades + attack speed links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'return',
			label: 'Frost Blades + Return',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_B_FROST_BLADES_END_NORETURN: MercRuleset = {
	id: 'guide-b-frost-blades-end-noreturn',
	label: 'Frost Blades',
	archetype: 'combatant',
	ladder: 'frost-blades',
	tier: 'end',
	tierLabel: 'endgame (no return)',
	savedSearch: { league: ALLFLAME, hash: 'q9l6yK0psg' },
	guideUrl: NEROTOX_COMBATANT_VIDEO,
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32807', name: 'Herald of Ice', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Frost Blades + Chain',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-pierce',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_27970', name: 'Greater Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Frost Blades + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
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
			id: 'speed',
			label: 'Frost Blades + attack speed links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'return',
			label: 'Frost Blades + Return',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_B_FROST_BLADES_END_RETURN: MercRuleset = {
	id: 'guide-b-frost-blades-end-return',
	label: 'Frost Blades',
	archetype: 'combatant',
	ladder: 'frost-blades',
	tier: 'end',
	tierLabel: 'endgame (return)',
	savedSearch: { league: ALLFLAME, hash: 'OglBJZoQIE' },
	guideUrl: NEROTOX_COMBATANT_VIDEO,
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32807', name: 'Herald of Ice', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Frost Blades + Chain',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-pierce',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_27970', name: 'Greater Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Frost Blades + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
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
			id: 'speed',
			label: 'Frost Blades + attack speed links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'return',
			label: 'Frost Blades + Return',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_B_FROST_BLADES_GG: MercRuleset = {
	id: 'guide-b-frost-blades-gg',
	label: 'Frost Blades',
	archetype: 'combatant',
	ladder: 'frost-blades',
	tier: 'gg',
	savedSearch: { league: ALLFLAME, hash: 'PPaX7lLqUL' },
	guideUrl: NEROTOX_COMBATANT_VIDEO,
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32807', name: 'Herald of Ice', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true }
			]
		},
		{
			id: 'core',
			label: 'Frost Blades + Chain',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-pierce',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_27970', name: 'Greater Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Frost Blades + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
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
			id: 'speed',
			label: 'Frost Blades + attack speed links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'return',
			label: 'Frost Blades + Return',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		}
	]
};

/**
 * The Combatant video's second ladder: Wild Strike, four rungs (Minimum /
 * Midgame / Endgame / GG Merc). A different search from the Frost Blades one,
 * not the same search with the skill swapped:
 *
 * - NO group carries the id `core`, so the first live `mercenary` group is the
 *   damage group. Not because no group asks for the skill plus a single link —
 *   `return` is exactly that — but because that group sits below the damage one
 *   and is parked on every rung below GG;
 * - the deny list sits AFTER the speed and return groups instead of before the
 *   damage one, and denies Multistrike rather than Pierce;
 * - the Minimum rung has five groups. Midgame and up add a sixth, `greater`,
 *   asking for the Tier-3 halves of the damage vocabulary on their own.
 *
 * `speed` is the only group that also moves by ENTRY: Minimum parks the whole
 * group, Midgame and GG keep it live while parking Faster Attacks (Tier 2), so
 * those two rungs ask for Greater Faster Attacks specifically.
 */
const GUIDE_B_WILD_STRIKE_MV: MercRuleset = {
	id: 'guide-b-wild-strike-mv',
	label: 'Wild Strike',
	archetype: 'combatant',
	ladder: 'wild-strike',
	tier: 'mv',
	savedSearch: { league: ALLFLAME, hash: '3q6awYZPc5' },
	guideUrl: NEROTOX_COMBATANT_VIDEO,
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32807', name: 'Herald of Ice', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Wild Strike + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
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
			id: 'speed',
			label: 'Wild Strike + attack speed links',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'return',
			label: 'Wild Strike + Return',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-multistrike',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.support_25973',
					name: 'Greater Multistrike (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_62638', name: 'Multistrike (Tier 2)', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_B_WILD_STRIKE_MID: MercRuleset = {
	id: 'guide-b-wild-strike-mid',
	label: 'Wild Strike',
	archetype: 'combatant',
	ladder: 'wild-strike',
	tier: 'mid',
	savedSearch: { league: ALLFLAME, hash: 'mkgR2DbeS6' },
	guideUrl: NEROTOX_COMBATANT_VIDEO,
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32807', name: 'Herald of Ice', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Wild Strike + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
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
			id: 'speed',
			label: 'Wild Strike + attack speed links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'return',
			label: 'Wild Strike + Return',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-multistrike',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.support_25973',
					name: 'Greater Multistrike (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_62638', name: 'Multistrike (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'greater',
			label: 'Wild Strike + greater damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		}
	]
};

const GUIDE_B_WILD_STRIKE_END: MercRuleset = {
	id: 'guide-b-wild-strike-end',
	label: 'Wild Strike',
	archetype: 'combatant',
	ladder: 'wild-strike',
	tier: 'end',
	savedSearch: { league: ALLFLAME, hash: 'jWRDpypkCX' },
	guideUrl: NEROTOX_COMBATANT_VIDEO,
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32807', name: 'Herald of Ice', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Wild Strike + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
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
			id: 'speed',
			label: 'Wild Strike + attack speed links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'return',
			label: 'Wild Strike + Return',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-multistrike',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.support_25973',
					name: 'Greater Multistrike (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_62638', name: 'Multistrike (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'greater',
			label: 'Wild Strike + greater damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		}
	]
};

const GUIDE_B_WILD_STRIKE_GG: MercRuleset = {
	id: 'guide-b-wild-strike-gg',
	label: 'Wild Strike',
	archetype: 'combatant',
	ladder: 'wild-strike',
	tier: 'gg',
	savedSearch: { league: ALLFLAME, hash: 'bGDrZYZaCL' },
	guideUrl: NEROTOX_COMBATANT_VIDEO,
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32807', name: 'Herald of Ice', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Wild Strike + damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
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
			id: 'speed',
			label: 'Wild Strike + attack speed links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			id: 'return',
			label: 'Wild Strike + Return',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'deny-multistrike',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.support_25973',
					name: 'Greater Multistrike (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_62638', name: 'Multistrike (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'greater',
			label: 'Wild Strike + greater damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		}
	]
};

/**
 * CaptainLance9's "Ideal Merc Options" (Mobalytics, Luminary Merc Bot build,
 * league Allflame 3.29) — the source that publishes no trade links.
 *
 * The live page refuses bots; the prose below was pasted by the owner
 * 2026-08-26 and a Wayback snapshot dated 2026-07-28 exists. Each ruleset's
 * groups carry the author's own sentence, so the transcription can be re-checked
 * against the guide without leaving this file — that re-check is the ONLY oracle
 * these four have, because their `__fixtures__` files are this builder's output
 * rather than GGG's.
 *
 * One modelling ruling covers all four (Sebastian, 2026-08-26). The author names
 * a skill and then the links he wants on it, without saying that a mercenary
 * missing one is worthless — so the SKILL is required and every listed support
 * is a switched-off bonus, at every tier its family has. The denials are the two
 * places the prose does say "do not", so those are live `not` groups. Buff skills
 * are the same kind of upside as the supports and ride in a parked `and` group,
 * the shape guide-a already uses for its aura lists.
 *
 * Consequence worth naming: a guide-c pass is a low bar by construction — the
 * skill row and no denied skill. The verdict earns its detail from the bonus
 * list, which is the part that says how close to ideal this mercenary is.
 */
const CAPTAINLANCE_BUILD = 'https://mobalytics.gg/poe/builds/captainlance9-luminary-merc-bot';

const GUIDE_C_KINETIST: MercRuleset = {
	id: 'guide-c-kinetist',
	label: 'Kinetist',
	archetype: 'kinetist',
	authored: { file: 'guide-c-kinetist' },
	authorNote: 'BiS Clear Merc',
	status: 'securable',
	groups: [
		{
			// "Kinetic Blast of Clustering - Multiple Projectiles Support -
			// Returning Projectiles Support - Clear Support(pierce/chain/ect)".
			// The parenthesis is why three whole families are here rather than the
			// one link a saved search would have pinned: the author asks for A
			// clear support and names two examples, so every tier of Pierce, Chain
			// and Fork counts as the thing he asked for.
			id: 'core',
			label: 'Kinetic Blast of Clustering + ideal links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
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
				},
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{ id: 'mercenary.support_6040', name: 'Lesser Pierce (Tier 1)', enabledInSearch: false },
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_27970',
					name: 'Greater Pierce (Tier 3)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_10482', name: 'Gilded Pierce (Tier 3)', enabledInSearch: false },
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: false },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_31571',
					name: 'Gilded Chain Distance (Tier 3)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: false }
			]
		},
		{
			// "do NOT get Kinetic Bolt - this will brick merc ai to not use
			// clustering properly"
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_12583', name: 'Kinetic Bolt', enabledInSearch: true }]
		},
		{
			// "Buff Skills: Haste / Inspiring Cry"
			id: 'buffs',
			label: 'Buff skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: false },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: false }
			]
		}
	]
};

const GUIDE_C_MANYSHOT: MercRuleset = {
	id: 'guide-c-manyshot',
	label: 'Manyshot',
	archetype: 'manyshot',
	authored: { file: 'guide-c-manyshot' },
	authorNote: 'Good Clear / Single Target',
	status: 'securable',
	groups: [
		{
			// "Ice Shot - Multiple Projectiles Support - Returning Projectiles
			// Support - Elemental Damage with Attacks Support"
			id: 'core',
			label: 'Ice Shot + ideal links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{
					id: 'mercenary.support_12054',
					name: 'Multiple Projectiles (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// "Vaal Ice Shot(single target needed) - Returning Projectiles Support
			// - Elemental Damage with Attacks Support - Cooldown Recovery Support".
			// "single target needed" is why this row is REQUIRED rather than a
			// second option: the author is naming the skill the merc cannot do the
			// job without, so both rows have to be there.
			id: 'secondary',
			label: 'Vaal Ice Shot + ideal links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_30881',
					name: 'Lesser Cooldown Recovery (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_48875',
					name: 'Cooldown Recovery (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_10608',
					name: 'Greater Cooldown Recovery (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// "AVOID \"icicle rain\" this skill does bad damage and interrupts vaal
			// ice shot from occurring properly"
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_24409', name: 'Icicle Rain', enabledInSearch: true }]
		},
		{
			// "Buff Skills: Grace / Hatred / Frost Bomb"
			id: 'buffs',
			label: 'Buff skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_2792', name: 'Grace', enabledInSearch: false },
				{ id: 'mercenary.skill_24482', name: 'Hatred', enabledInSearch: false },
				{ id: 'mercenary.skill_10557', name: 'Frost Bomb', enabledInSearch: false }
			]
		}
	]
};

const GUIDE_C_BLADE_AMBUSHER: MercRuleset = {
	id: 'guide-c-blade-ambusher',
	label: 'Blade Ambusher',
	archetype: 'blade-ambusher',
	authored: { file: 'guide-c-blade-ambusher' },
	authorNote: 'Good Bossing / Single Target Merc',
	status: 'securable',
	groups: [
		{
			// "Spectral Helix of Trarthus - Multiple Traps Support - Trap and Mine
			// Damage Support - Slower Projectiles Support".
			// The TRARTHUS transfigure (skill_28988), not plain Spectral Helix
			// (skill_37916): different stat ids, and guide-a's Combatant search
			// actively DENIES the plain one.
			id: 'core',
			label: 'Spectral Helix of Trarthus + ideal links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.skill_28988',
					name: 'Spectral Helix of Trarthus',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_2555', name: 'Multiple Traps (Tier 3)', enabledInSearch: false },
				{
					id: 'mercenary.support_49954',
					name: 'Lesser Trap and Mine Damage (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_31171',
					name: 'Trap and Mine Damage (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_59079',
					name: 'Greater Trap and Mine Damage (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_15866',
					name: 'Lesser Slower Projectiles (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_2210',
					name: 'Slower Projectiles (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_44952',
					name: 'Greater Slower Projectiles (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// "Buff Skills: Grace / Summon Skitterbots"
			id: 'buffs',
			label: 'Buff skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_2792', name: 'Grace', enabledInSearch: false },
				{ id: 'mercenary.skill_44296', name: 'Summon Skitterbots', enabledInSearch: false }
			]
		}
	]
};

const GUIDE_C_COMBATANT: MercRuleset = {
	id: 'guide-c-combatant',
	label: 'Combatant',
	archetype: 'combatant',
	authored: { file: 'guide-c-combatant' },
	authorNote:
		'Good All rounder / Starter Merc (better clear option as armour stack setup late game)',
	status: 'securable',
	groups: [
		{
			// "Static Strike - Elemental Damage with Attacks Support - More
			// Duration Support - Chain Support"
			id: 'core',
			label: 'Static Strike + ideal links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true },
				{
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_2602',
					name: 'Lesser More Duration (Tier 1)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_50222', name: 'More Duration (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_26568',
					name: 'Greater More Duration (Tier 3)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: false },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_31571',
					name: 'Gilded Chain Distance (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// "Frost Blades - Returning Projectiles Support - Elemental Damage with
			// Attacks Support - Chain Support/".
			// Two skill rows, both required: the author lists them as one merc's
			// setup rather than as alternatives, the way he lists Ice Shot and Vaal
			// Ice Shot. The trailing slash is his, and means nothing.
			id: 'secondary',
			label: 'Frost Blades + ideal links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: false },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_31571',
					name: 'Gilded Chain Distance (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// "Buff Skills: Wrath / Purity of Ice"
			id: 'buffs',
			label: 'Buff skills',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_38326', name: 'Wrath', enabledInSearch: false },
				{ id: 'mercenary.skill_13693', name: 'Purity of Ice', enabledInSearch: false }
			]
		}
	]
};

/**
 * "5 DIVINE BUDGET LIFE STACKING KB MERC BUILD | Trade Links - Crafting - Merc
 * Warrants", 2026-08-14 — the source URL, because BOTH rungs come off this one
 * video. Its description links the sheet the 20D hash was copied from:
 * <https://docs.google.com/spreadsheets/d/1c-9qyowK9jp8OIR0bwh8G0V3qjY8U6lEDAxA6xOUMdU/edit?gid=586502310>
 */
const XTHEFARMERX_KB_VIDEO = 'https://www.youtube.com/watch?v=LXoJCRmUaJI';

/**
 * The cheap rung, hash `4mKr0Jbwh9`.
 *
 * Sebastian TYPED this hash off the video's own trade tab (its Merc Skills and
 * Trade Filters chapters, 9:48–18:56); it is not in the linked sheet, which
 * publishes a sibling "Cheap Starter KB Merc" under `G6PdveWBib`. The two differ
 * only in the last damage group — `G6PdveWBib` lacks Faster Attacks (Tier 2) and
 * Greater Faster Attacks (Tier 3) — so the typed hash resolves to a coherent KB
 * search and is the frame's own, not a mistyped sibling. `G6PdveWBib` is NOT
 * committed: no ruleset transcribes it.
 *
 * Read against Nerotox's Mid rung (`BgzkZKGQF8`), which is the same seven slots:
 * Greater Multiple Projectiles is required on the core skill instead of Return,
 * Barrage is a live secondary rather than a parked one, the whole Pierce family
 * including Lesser is denied by the LIVE deny group rather than by
 * `deny-supports`, and the damage group grows the two Faster Attacks tiers.
 */
const GUIDE_D_KINETIST_BUDGET: MercRuleset = {
	id: 'guide-d-kinetist-budget',
	label: 'Kinetist',
	archetype: 'kinetist',
	ladder: 'kinetist',
	// `TIERS` is a ranking order, not a claim about what a rung costs; the guide
	// spells this rung "budget", which is what `tierLabel` is for.
	tier: 'mv',
	tierLabel: 'budget',
	savedSearch: { league: ALLFLAME, hash: '4mKr0Jbwh9' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			// The only Kinetist deny list here that denies support links as well as
			// skills — the Pierce family is refused outright rather than parked in
			// `deny-supports` the way guide-b's rungs park it.
			id: 'deny',
			label: 'Denied skills and links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_32089', name: 'Kinetic Rain of Impact', enabledInSearch: true },
				{ id: 'mercenary.skill_12583', name: 'Kinetic Bolt', enabledInSearch: true },
				{ id: 'mercenary.skill_26705', name: 'Power Siphon', enabledInSearch: true },
				{ id: 'mercenary.support_6040', name: 'Lesser Pierce (Tier 1)', enabledInSearch: true },
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_27970',
					name: 'Greater Pierce (Tier 3)',
					enabledInSearch: true
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
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
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
			// Parked, and redundant while it is: the live `deny` group above already
			// refuses both of these. Transcribed as the author saved it.
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
			// Haste is switched OFF here, so this search does not gate on it — a
			// plain bonus, not `buyerContextual`. Same reading as the Nerotox Mid
			// rung, and the opposite of the 20D rung below, whose `and` group is on.
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: false }]
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
				{ id: 'mercenary.support_32189', name: 'Critical Damage (Tier 2)', enabledInSearch: true },
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		}
	]
};

/**
 * The upper rung, the sheet's "20D KB Merc" — hash `7nRvBzl2S5`, byte for byte
 * the search `GUIDE_B_KINETIST_MV` transcribes. Written out rather than derived
 * from that constant for the reason the whole file is written out: sharing an
 * object would make "the two sources transcribe the same search" true by
 * construction, and it is exactly the fact the fidelity test is supposed to be
 * able to disprove.
 */
const GUIDE_D_KINETIST_20D: MercRuleset = {
	id: 'guide-d-kinetist-20d',
	label: 'Kinetist',
	archetype: 'kinetist',
	ladder: 'kinetist',
	tier: 'mid',
	tierLabel: '20D',
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
				// `buyerContextual` is Sebastian's selling-side ruling (see the flag's
				// doc comment above and the guide-a Haste entry), and it follows the
				// HASH rather than whoever published it. This rung transcribes the
				// same hash `7nRvBzl2S5` as guide-b's MV rung, so it carries the same flag.
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
			// This search DOES gate on Haste — live `and` group, live filter — so the
			// entry is buyer-contextual, the same ruling guide-b makes on the same
			// search. The budget rung above has the group's only filter switched off
			// and therefore gates on nothing.
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
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
 * sushi's buyer-side archetype notes (TwitchTVSpicysushi#7614), league Allflame
 * 3.29 — the second source here that publishes no trade link, and the only one
 * whose prose reached this repo as an IMAGE.
 *
 * It lives in cell E14 of Sheet1 of a public Google Sheet as an embedded
 * screenshot, captured 2026-08-31. The PNG is NOT committed — the owner keeps it
 * with POE-228 — and the verbatim transcription is in that ticket's description;
 * the video it was screenshotted from is unknown. So the transcription below is
 * a reading of a picture of text, and every group carries the line it came from
 * verbatim — the author's spelling and commas included — because that quote is
 * the only thing a re-checker can hold the picture against.
 *
 * Modelled the guide-c way (the CaptainLance ruling above), because it is the
 * same KIND of source: a buyer naming skills and the links he wants on them,
 * with no prices, no floors and no ranking of mercenaries. So the SKILL is the
 * only live filter of its `mercenary` group, every support the note names is a
 * switched-off bonus at every tier its family has, and each "no ..." line is a
 * live `not` group. A skill the note merely WANTS — an aura, the Sniper's totem,
 * the Cruel Mistress's second skill — rides a live `and` group with every entry
 * parked, the shape guide-a uses for its aura lists; a skill it accepts INSTEAD
 * of another — the Sniper's "rain of arrows works too" — rides a live `count`
 * group of `min: 1` with both skills live, the shape guide-b uses for "Greater
 * Kinetic Blast or Barrage".
 *
 * Two things this source has that guide-c does not. It RANKS supports
 * ("chain > wed > hypo > faster/gmp", twice, on the Combatant) and it SHOUTS one
 * of its denials ("NO PIERCE"). Neither is expressible as a switch — a ranking
 * is not a threshold, and the shout is not a filter although the denial under it
 * IS one, a live `not` group like every other "no ..." line — so both go into
 * the Combatant's `authorNote` verbatim, while every ranked support is ALSO
 * listed as an ordinary bonus. The model says "wanted"; the note says "in this
 * order". The Kineticist's `authorNote` carries the source's one remark about a
 * skill it never asks for ("barrage is not a brick"). Those two notes are the
 * whole of what rides prose here, pinned in `rulesets.test.ts`'s "author notes".
 *
 * Three archetypes arrive with it — Sniper, Cruel Mistress and Stormhand — and
 * no other source here covers any of them.
 *
 * The jargon is the author's own shorthand ("TS", "wed", "gilded +2 proj",
 * "fr totems"). Every resolution and its evidence is tabulated in
 * `__fixtures__/README.md` under "Authored queries (guide-e)"; the group
 * comments below name the resolution beside the quote it came from.
 */
const SUSHI_SHEET =
	'https://docs.google.com/spreadsheets/d/1EW1JIew9A08RDmZbtWOcLzo3WEokexMdOlldXwRF34Q/htmlview';

const GUIDE_E_SNIPER: MercRuleset = {
	id: 'guide-e-sniper',
	label: 'Sniper',
	archetype: 'sniper',
	authored: { file: 'guide-e-sniper' },
	status: 'securable',
	groups: [
		{
			// "TS with GMP and gilded +2 proj" and "rain of arrows works too" — ONE
			// rule, because the second line names an alternative skill ROW rather
			// than a link on the first. "TS" is Tornado Shot; Rain of Arrows of
			// Saturation is the sole mercenary form of the skill the second line
			// names.
			//
			// So the two skills share one `count` group asking for ONE of them — the
			// shape guide-b's Kinetist MV rung uses for "Greater Kinetic Blast or
			// Barrage". A Sniper running either passes, a Sniper running neither
			// fails here, and neither skill is demanded of the other's mercenary.
			// The alternative is a RULE, not a remark, so nothing about it rides
			// `authorNote`.
			id: 'core',
			label: 'Tornado Shot or Rain of Arrows',
			type: 'count',
			enabledInSearch: true,
			min: 1,
			entries: [
				{ id: 'mercenary.skill_8030', name: 'Tornado Shot', enabledInSearch: true },
				{
					id: 'mercenary.skill_40759',
					name: 'Rain of Arrows of Saturation',
					enabledInSearch: true
				}
			]
		},
		{
			// The links half of "TS with GMP and gilded +2 proj" — the author hangs
			// them on TS, so they are row-scoped to Tornado Shot and asked of nothing
			// else. "gilded +2 proj" is Gilded Secondary Shots (Tier 3), the one
			// gilded support whose vocabulary text names the thing the note names
			// ("Supported Tornado Shot fires +2 additional secondary Projectiles").
			// "GMP" is the Multiple Projectiles family, which has a Tier 1 and a
			// Tier 3 and no Tier 2.
			//
			// Parked, the guide-c way: the note WANTS these links, it does not gate
			// on them. Parked also keeps the group off a Rain of Arrows Sniper's comp
			// link — `verdict.ts::flipsFor` revives a parked `mercenary` group only
			// when the capture proves its anchor skill, so a mercenary without
			// Tornado Shot is never asked for Tornado Shot's links.
			id: 'tornado-links',
			label: 'Tornado Shot + wanted links',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_8030', name: 'Tornado Shot', enabledInSearch: true },
				{
					id: 'mercenary.support_12054',
					name: 'Multiple Projectiles (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_18499',
					name: 'Gilded Secondary Shots (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// "no brutality no arrow nova" — the whole Brutality family, and Arrow
			// Nova, which the vocabulary carries at Tier 3 only.
			id: 'deny-supports',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.support_55807',
					name: 'Lesser Brutality (Tier 1)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_64271', name: 'Brutality (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_59345',
					name: 'Greater Brutality (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_58429', name: 'Arrow Nova (Tier 3)', enabledInSearch: true }
			]
		},
		{
			// "totem" — read as a SKILL, not as a totem support: the two ballistas
			// are the only totem skills in the Sniper pool and the note names no
			// link for them. Either one answers it, so both are parked bonuses of
			// ONE `and` group rather than two live requirements.
			id: 'totems',
			label: 'Ballista totem',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_61903', name: 'Shrapnel Ballista', enabledInSearch: false },
				{
					id: 'mercenary.skill_44144',
					name: 'Siege Ballista of Trarthus',
					enabledInSearch: false
				}
			]
		},
		{
			// "haste aura" — Haste is a skill; the mercenary vocabulary has no aura
			// concept of its own.
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: false }]
		}
	]
};

const GUIDE_E_KINETIST: MercRuleset = {
	id: 'guide-e-kinetist',
	// The note spells the class "kineticist". The app's archetype key is
	// `kinetist`, the spelling guide-b and guide-c use, and the label follows the
	// app so both readings of the class sit in one column on the page.
	label: 'Kinetist',
	archetype: 'kinetist',
	authored: { file: 'guide-e-kinetist' },
	// "barrage is not a brick" — a remark about a skill the note asks for
	// nowhere. It is not a filter, so it stays prose.
	authorNote: 'barrage is not a brick',
	status: 'securable',
	groups: [
		{
			// "greater KBoC & KBoC" and "return with chain/fork wed,gmp,crit dmg,".
			// Greater Kinetic Blast and Kinetic Blast of Clustering are two DISTINCT
			// skills of the Kineticist pool and the "&" asks for both, so each gets
			// its own `mercenary` group.
			//
			// The link line names no row. Rather than pick one, both groups carry
			// the same list as bonuses: nothing gates on the choice, and neither row
			// is denied a link the note might have meant for it.
			id: 'core',
			label: 'Greater Kinetic Blast + wanted links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_44258', name: 'Greater Kinetic Blast', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: false },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_31571',
					name: 'Gilded Chain Distance (Tier 3)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: false },
				{
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
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
				},
				{
					id: 'mercenary.support_30688',
					name: 'Lesser Critical Damage (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_32189',
					name: 'Critical Damage (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_55659',
					name: 'Greater Critical Damage (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// The other half of "greater KBoC & KBoC", carrying the same
			// "return with chain/fork wed,gmp,crit dmg," list for the reason above.
			id: 'secondary',
			label: 'Kinetic Blast of Clustering + wanted links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: false },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_31571',
					name: 'Gilded Chain Distance (Tier 3)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: false },
				{
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
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
				},
				{
					id: 'mercenary.support_30688',
					name: 'Lesser Critical Damage (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_32189',
					name: 'Critical Damage (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_55659',
					name: 'Greater Critical Damage (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// "no kinetic bolt or kinetic rain". Kinetic Rain of Impact is the sole
			// mercenary form of the second one; guide-c denies Kinetic Bolt too, for
			// its own stated reason.
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_12583', name: 'Kinetic Bolt', enabledInSearch: true },
				{ id: 'mercenary.skill_32089', name: 'Kinetic Rain of Impact', enabledInSearch: true }
			]
		},
		{
			// "haste"
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: false }]
		}
	]
};

const GUIDE_E_COMBATANT: MercRuleset = {
	id: 'guide-e-combatant',
	label: 'Combatant',
	archetype: 'combatant',
	authored: { file: 'guide-e-combatant' },
	// Two support rankings and a shouted denial, verbatim. A ranking is not a
	// threshold, so none of it is a switch — every support it names is an
	// ordinary bonus in the groups below, and the ORDER lives only here.
	authorNote:
		'frost blades: chain > wed > hypo > faster/gmp; wild strike: return > wed > hypo > faster; NO PIERCE',
	status: 'securable',
	groups: [
		{
			// "frost blades/wild strike + static strike together".
			// The slash is an OR and "+ ... together" is the constant, so Static
			// Strike is the one skill this note requires of every Combatant. Making
			// Frost Blades and Wild Strike live as well would ask for three of the
			// four skills a Combatant rolls TWO of — a ruleset no mercenary could
			// ever answer — so they are parked groups below, and the deny list does
			// the rest: with Spectral Helix forbidden, a Combatant carrying Static
			// Strike has Frost Blades or Wild Strike as its other skill.
			// The note names no link for this row.
			id: 'core',
			label: 'Static Strike',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true }]
		},
		{
			// "frost blades: chain > wed > hypo > faster/gmp" — the first of the two
			// alternatives, parked for the reason in `core`. "faster" is the Faster
			// Attacks family: Frost Blades is a melee attack, and Faster Projectiles
			// is the other family the word could name.
			id: 'frost-blades',
			label: 'Frost Blades + ranked links',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: false },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_31571',
					name: 'Gilded Chain Distance (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_26146',
					name: 'Lesser Hypothermia (Tier 1)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_52447',
					name: 'Lesser Faster Attacks (Tier 1)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
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
			// "wild strike: return > wed > hypo > faster" — the other alternative.
			// Its ranking drops GMP and leads with Return; both differences are the
			// author's, and neither is a switch.
			id: 'wild-strike',
			label: 'Wild Strike + ranked links',
			type: 'mercenary',
			enabledInSearch: false,
			entries: [
				{ id: 'mercenary.skill_40957', name: 'Wild Strike', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_26146',
					name: 'Lesser Hypothermia (Tier 1)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_52447',
					name: 'Lesser Faster Attacks (Tier 1)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// "no spectral helix/elemental hit".
			// Plain Spectral Helix, NOT the Trarthus transfigure guide-c's Blade
			// Ambusher is built around — different ids, different class. And
			// Elemental Hit of Ice is the only Elemental Hit in the mercenary
			// vocabulary, which poewiki puts in the Mysterious Diver pool rather
			// than the Combatant one; the note says it anyway, so it is transcribed
			// anyway. A denial that can never fire costs nothing, and dropping it
			// would be this app overruling the source it is quoting.
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_37916', name: 'Spectral Helix', enabledInSearch: true },
				{ id: 'mercenary.skill_8708', name: 'Elemental Hit of Ice', enabledInSearch: true }
			]
		},
		{
			// "NO PIERCE" — the author's capitals, and the only line he shouts.
			// Four ids: the family's three tiers plus the gilded Tier 3.
			id: 'deny-supports',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_6040', name: 'Lesser Pierce (Tier 1)', enabledInSearch: true },
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{ id: 'mercenary.support_27970', name: 'Greater Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_10482', name: 'Gilded Pierce (Tier 3)', enabledInSearch: true }
			]
		}
	]
};

const GUIDE_E_MANYSHOT: MercRuleset = {
	id: 'guide-e-manyshot',
	label: 'Manyshot',
	archetype: 'manyshot',
	authored: { file: 'guide-e-manyshot' },
	status: 'securable',
	groups: [
		{
			// "ice shot with return on both" and "gmp, wed, hypo, crit".
			// "on both" is the author's own scoping and it attaches to RETURN alone
			// — Ice Shot and Vaal Ice Shot are the two rows a Manyshot has for it —
			// so Return is a bonus here and in `secondary`, while the next line's
			// links stay on this row rather than being credited to both.
			// "crit" is unqualified, so both crit families are listed.
			id: 'core',
			label: 'Ice Shot + wanted links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
				{
					id: 'mercenary.support_12054',
					name: 'Multiple Projectiles (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_44886',
					name: 'Elemental Damage with Attacks (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_28416',
					name: 'Greater Elemental Damage with Attacks (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_26146',
					name: 'Lesser Hypothermia (Tier 1)',
					enabledInSearch: false
				},
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_30688',
					name: 'Lesser Critical Damage (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_32189',
					name: 'Critical Damage (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_55659',
					name: 'Greater Critical Damage (Tier 3)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_23209',
					name: 'Lesser Critical Chance (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_61471',
					name: 'Critical Chance (Tier 2)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_62220',
					name: 'Greater Critical Chance (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// The other half of "return on both": Vaal Ice Shot is the Manyshot's
			// second Ice Shot row, and Return is the only link the note puts on it.
			id: 'secondary',
			label: 'Vaal Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false }
			]
		},
		{
			// "no icicle rain" — guide-c denies the same skill, for its own reason.
			id: 'deny',
			label: 'Denied skills',
			type: 'not',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_24409', name: 'Icicle Rain', enabledInSearch: true }]
		}
	]
};

const GUIDE_E_CRUEL_MISTRESS: MercRuleset = {
	id: 'guide-e-cruel-mistress',
	// The note spells her "Cruel Mistriss"; the class is Cruel Mistress.
	label: 'Cruel Mistress',
	archetype: 'cruel-mistress',
	authored: { file: 'guide-e-cruel-mistress' },
	status: 'securable',
	groups: [
		{
			// "soulrend of reaping w/ return + gmp"
			id: 'core',
			label: 'Soulrend of Reaping + wanted links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_10742', name: 'Soulrend of Reaping', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: false },
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
			// "summon void or fr totems" — an OR, so the two are parked bonuses of
			// one `and` group: either fires on its own and neither gates.
			// "summon void" is Summon Seeking Void, not Void Sphere — that one is
			// her class primary and every Cruel Mistress has it, so asking for it
			// would say nothing about the mercenary.
			id: 'secondary',
			label: 'Second skill — either',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_54144', name: 'Summon Seeking Void', enabledInSearch: false },
				{ id: 'mercenary.skill_29071', name: 'Forbidden Rite Totem', enabledInSearch: false }
			]
		},
		{
			// "envy aura"
			id: 'auras',
			label: 'Auras',
			type: 'and',
			enabledInSearch: true,
			entries: [{ id: 'mercenary.skill_17515', name: 'Envy', enabledInSearch: false }]
		}
	]
};

const GUIDE_E_STORMHAND: MercRuleset = {
	id: 'guide-e-stormhand',
	label: 'Stormhand',
	archetype: 'stormhand',
	authored: { file: 'guide-e-stormhand' },
	status: 'securable',
	groups: [
		{
			// "arc and ball lightning of static" and "chain + gilded chain on arc".
			// Both links sit on the ARC row, the author says so, and the family is
			// exactly three ids: there is no Greater Chain, and the Tier 3 is
			// Gilded Chain Distance, whose vocabulary text is Arc-specific
			// ("Supported Arc has +10% more damage per remaining Chain").
			id: 'core',
			label: 'Arc + chain links',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_59005', name: 'Arc', enabledInSearch: true },
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: false },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_31571',
					name: 'Gilded Chain Distance (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			// "arc and ball lightning of static" — "and", so both skills are wanted,
			// and the Stormhand pool rolls two of seven, so one mercenary can carry
			// both. The note names no link for this row.
			id: 'secondary',
			label: 'Ball Lightning of Static',
			type: 'mercenary',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_30663', name: 'Ball Lightning of Static', enabledInSearch: true }
			]
		}
	]
};

/**
 * The "Path of Evening" mercenary-support build page. Six saved searches on ONE
 * page, so no ruleset here carries a `guideUrl` of its own.
 *
 * The page itself answers 403 to every fetch this repo can make (Cloudflare), so
 * the six hashes were pasted by the owner 2026-09-04 and each search was then
 * fetched from GGG by hash — which is why the fixtures are as trustworthy as any
 * other saved search here while the PROSE around them is unavailable. Nothing
 * below is transcribed from sentences: where a switch needs a reading, it is
 * read off the search, and the author's own wording survives only in the rung
 * labels ("Cheap"/"Expensive") and in the archetype names he uses.
 */
const PATH_OF_EVENING_BUILD =
	'https://mobalytics.gg/poe/builds/mercenary-support-luminary-path-of-evening';

/**
 * The Multishot ladder's cheap rung, hash `8r8JqonVIV`.
 *
 * "Multishot" is this author's spelling of the archetype every other source here
 * calls Manyshot — the `archetype` key is the app's, the spelling is noted so a
 * reader comparing the page to the guide is not looking for a fourth archetype.
 *
 * What it asks for: Vaal Ice Shot and Grace on the mercenary, no Icicle Rain,
 * and on the Ice Shot row three of the elemental-damage and Hypothermia links,
 * Return, and two of the chain/fork/projectile links. Frigid Forkshot and Hatred
 * are parked in the lead group.
 */
const GUIDE_F_MANYSHOT_CHEAP: MercRuleset = {
	id: 'guide-f-manyshot-cheap',
	// `label` carries the app's archetype word so this rung sits in the same
	// column as guide-b's Manyshot ladder; the author's "Multishot" is recorded
	// in the rung comment above rather than put into a label.
	label: 'Manyshot',
	archetype: 'manyshot',
	ladder: 'manyshot',
	// `TIERS` ranks rungs; it does not name them. This author spells his two
	// "Cheap" and "Expensive", and 'mv'/'end' would put a price on searches he
	// never quoted one for.
	tier: 'mv',
	tierLabel: 'Cheap',
	savedSearch: { league: ALLFLAME, hash: '8r8JqonVIV' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills and auras',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.skill_18232', name: 'Frigid Forkshot', enabledInSearch: false },
				{ id: 'mercenary.skill_2792', name: 'Grace', enabledInSearch: true },
				{ id: 'mercenary.skill_24482', name: 'Hatred', enabledInSearch: false }
			]
		},
		{
			id: 'damage',
			label: 'Ice Shot damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
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
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
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
			id: 'return',
			label: 'Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
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
			id: 'projectiles',
			label: 'Ice Shot projectile links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true },
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_12054',
					name: 'Multiple Projectiles (Tier 1)',
					enabledInSearch: true
				}
			]
		}
	]
};

/**
 * The Multishot ladder's expensive rung, hash `veYJp9gZhE`.
 *
 * The one ladder here whose upper rung is not the cheap search with one switch
 * flipped. It asks for everything the cheap rung asks for and then THREE more
 * things: Hatred goes live in the lead group, a new Vaal Ice Shot row group
 * wants three of the four — Vaal Ice Shot, its two damage links and Return —
 * and the projectile group drops Lesser Chain (Tier 1), so a Tier-1 chain no
 * longer counts toward its two. Return on the plain Ice Shot row is live on
 * BOTH rungs, unlike the other two ladders.
 */
const GUIDE_F_MANYSHOT_EXPENSIVE: MercRuleset = {
	id: 'guide-f-manyshot-expensive',
	label: 'Manyshot',
	archetype: 'manyshot',
	ladder: 'manyshot',
	tier: 'end',
	tierLabel: 'Expensive',
	savedSearch: { league: ALLFLAME, hash: 'veYJp9gZhE' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills and auras',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.skill_18232', name: 'Frigid Forkshot', enabledInSearch: false },
				{ id: 'mercenary.skill_2792', name: 'Grace', enabledInSearch: true },
				// The search spells this one `disabled: false` rather than leaving the
				// key out, which is the same thing — GGG writes both.
				{ id: 'mercenary.skill_24482', name: 'Hatred', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Ice Shot damage links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
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
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
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
			id: 'return',
			label: 'Ice Shot + Return',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
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
			// The rung's own group: the cheap search asks nothing of the Vaal Ice
			// Shot row beyond having it. Return sits inside it rather than in a
			// second Vaal group, so this one slot carries both.
			id: 'vaal-damage',
			label: 'Vaal Ice Shot damage links + Return',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{ id: 'mercenary.skill_16381', name: 'Vaal Ice Shot', enabledInSearch: true },
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
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			// Lesser Chain (Tier 1) is DECLARED nowhere on this rung, not parked:
			// the author removed the filter rather than switching it off.
			id: 'projectiles',
			label: 'Ice Shot projectile links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_11495', name: 'Ice Shot', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true },
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{
					id: 'mercenary.support_49419',
					name: 'Greater Multiple Projectiles (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_12054',
					name: 'Multiple Projectiles (Tier 1)',
					enabledInSearch: true
				}
			]
		}
	]
};

/**
 * The Combatant ladder's cheap rung, hash `PPGnKVv7UL`.
 *
 * What it asks for: Frost Blades, Static Strike and Wrath on the mercenary,
 * neither Pierce nor Multistrike at any tier, four of the eight damage-and-speed
 * links on the Frost Blades row, and two of the Static Strike links. Purity of
 * Ice is parked in the lead group, the three More Duration tiers inside the
 * Static Strike group, and the whole Return group is switched off — which is
 * this ladder's only lever.
 */
const GUIDE_F_COMBATANT_CHEAP: MercRuleset = {
	id: 'guide-f-combatant-cheap',
	label: 'Combatant',
	archetype: 'combatant',
	ladder: 'combatant',
	tier: 'mv',
	tierLabel: 'Cheap',
	savedSearch: { league: ALLFLAME, hash: 'PPGnKVv7UL' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills and auras',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true },
				{ id: 'mercenary.skill_38326', name: 'Wrath', enabledInSearch: true },
				{ id: 'mercenary.skill_13693', name: 'Purity of Ice', enabledInSearch: false }
			]
		},
		{
			// One group where Nerotox's Frost Blades ladder has two: the speed links
			// are counted alongside the damage ones rather than asked for separately,
			// so four of ANY eight satisfies it.
			id: 'damage',
			label: 'Frost Blades damage and speed links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 4,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
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
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			// The lever. Switched off here and on at Expensive, and it is the ONLY
			// difference between the two Combatant searches.
			id: 'return',
			label: 'Frost Blades + Return',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'secondary-links',
			label: 'Static Strike links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true },
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: true },
				{ id: 'mercenary.support_50222', name: 'More Duration (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_2602',
					name: 'Lesser More Duration (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_26568',
					name: 'Greater More Duration (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			id: 'deny-supports',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{ id: 'mercenary.support_6040', name: 'Lesser Pierce (Tier 1)', enabledInSearch: true },
				{ id: 'mercenary.support_27970', name: 'Greater Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_10482', name: 'Gilded Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_62638', name: 'Multistrike (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_25973',
					name: 'Greater Multistrike (Tier 3)',
					enabledInSearch: true
				}
			]
		}
	]
};

/**
 * The Combatant ladder's expensive rung, hash `yYvmr6rjcR`.
 *
 * The cheap search with ONE lever pulled: the `return` group goes live, so the
 * Frost Blades row must carry Return (Tier 3). Everything else — the parked
 * Purity of Ice, the four-of-eight damage minimum, the parked More Duration
 * tiers, the denial list — is the cheap rung's, filter for filter.
 */
const GUIDE_F_COMBATANT_EXPENSIVE: MercRuleset = {
	id: 'guide-f-combatant-expensive',
	label: 'Combatant',
	archetype: 'combatant',
	ladder: 'combatant',
	tier: 'end',
	tierLabel: 'Expensive',
	savedSearch: { league: ALLFLAME, hash: 'yYvmr6rjcR' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills and auras',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true },
				{ id: 'mercenary.skill_38326', name: 'Wrath', enabledInSearch: true },
				{ id: 'mercenary.skill_13693', name: 'Purity of Ice', enabledInSearch: false }
			]
		},
		{
			id: 'damage',
			label: 'Frost Blades damage and speed links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 4,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
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
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_38571', name: 'Hypothermia (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_53145',
					name: 'Greater Hypothermia (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_987', name: 'Faster Attacks (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_50485',
					name: 'Greater Faster Attacks (Tier 3)',
					enabledInSearch: true
				}
			]
		},
		{
			// The lever, pulled. Same group, same two filters, same minimum as the
			// cheap rung's — only the group switch differs.
			id: 'return',
			label: 'Frost Blades + Return',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_22105', name: 'Frost Blades', enabledInSearch: true },
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		},
		{
			id: 'secondary-links',
			label: 'Static Strike links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: 'mercenary.skill_24931', name: 'Static Strike', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true },
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: true },
				{ id: 'mercenary.support_50222', name: 'More Duration (Tier 2)', enabledInSearch: false },
				{
					id: 'mercenary.support_2602',
					name: 'Lesser More Duration (Tier 1)',
					enabledInSearch: false
				},
				{
					id: 'mercenary.support_26568',
					name: 'Greater More Duration (Tier 3)',
					enabledInSearch: false
				}
			]
		},
		{
			id: 'deny-supports',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{ id: 'mercenary.support_6040', name: 'Lesser Pierce (Tier 1)', enabledInSearch: true },
				{ id: 'mercenary.support_27970', name: 'Greater Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_10482', name: 'Gilded Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_62638', name: 'Multistrike (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_25973',
					name: 'Greater Multistrike (Tier 3)',
					enabledInSearch: true
				}
			]
		}
	]
};

/**
 * The Kineticist ladder's cheap rung, hash `d80ePvdvhJ`.
 *
 * "Kineticist" is this author's spelling of the archetype the app keys
 * `kinetist`.
 *
 * What it asks for: Kinetic Blast of Clustering, Greater Kinetic Blast, Haste
 * and Inspiring Cry all on the mercenary, no Pierce at any tier, two of the
 * chain/fork links on the Kinetic Blast row and three of its critical and
 * elemental-damage links. The Return group is switched off — this ladder's only
 * lever.
 *
 * Haste and Inspiring Cry are live GATES here, not bonuses, and they stay that
 * way: `buyerContextual` is a selling-side ruling that needs the author saying
 * the aura is optional (Nerotox's "depending on your spectres"), and this page
 * publishes no prose this repo can read. Transcribed as saved.
 */
const GUIDE_F_KINETIST_CHEAP: MercRuleset = {
	id: 'guide-f-kinetist-cheap',
	label: 'Kinetist',
	archetype: 'kinetist',
	ladder: 'kinetist',
	tier: 'mv',
	tierLabel: 'Cheap',
	savedSearch: { league: ALLFLAME, hash: 'd80ePvdvhJ' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills and buffs',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.skill_44258', name: 'Greater Kinetic Blast', enabledInSearch: true },
				{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true }
			]
		},
		{
			id: 'deny-supports',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_6040', name: 'Lesser Pierce (Tier 1)', enabledInSearch: true },
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{ id: 'mercenary.support_27970', name: 'Greater Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_10482', name: 'Gilded Pierce (Tier 3)', enabledInSearch: true }
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
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Damage and critical links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_61471', name: 'Critical Chance (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_62220',
					name: 'Greater Critical Chance (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32189', name: 'Critical Damage (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_55659',
					name: 'Greater Critical Damage (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_30688',
					name: 'Lesser Critical Damage (Tier 1)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_23209',
					name: 'Lesser Critical Chance (Tier 1)',
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
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: true
				}
			]
		},
		{
			// The lever. Switched off here and on at Expensive, and it is the ONLY
			// difference between the two Kineticist searches.
			id: 'return',
			label: 'Kinetic Blast + Return',
			type: 'mercenary',
			enabledInSearch: false,
			min: 2,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_5293', name: 'Return (Tier 3)', enabledInSearch: true }
			]
		}
	]
};

/**
 * The Kineticist ladder's expensive rung, hash `rPogYW44uQ`.
 *
 * The cheap search with ONE lever pulled: the `return` group goes live, so the
 * Kinetic Blast row must carry Return (Tier 3). The denial list, both minimums
 * and all four lead-group gates are the cheap rung's, filter for filter.
 */
const GUIDE_F_KINETIST_EXPENSIVE: MercRuleset = {
	id: 'guide-f-kinetist-expensive',
	label: 'Kinetist',
	archetype: 'kinetist',
	ladder: 'kinetist',
	tier: 'end',
	tierLabel: 'Expensive',
	savedSearch: { league: ALLFLAME, hash: 'rPogYW44uQ' },
	status: 'securable',
	ilvlMin: 83,
	groups: [
		{
			id: 'required-skills',
			label: 'Required skills and buffs',
			type: 'and',
			enabledInSearch: true,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.skill_44258', name: 'Greater Kinetic Blast', enabledInSearch: true },
				{ id: 'mercenary.skill_52155', name: 'Haste', enabledInSearch: true },
				{ id: 'mercenary.skill_65473', name: 'Inspiring Cry', enabledInSearch: true }
			]
		},
		{
			id: 'deny-supports',
			label: 'Denied support links',
			type: 'not',
			enabledInSearch: true,
			entries: [
				{ id: 'mercenary.support_6040', name: 'Lesser Pierce (Tier 1)', enabledInSearch: true },
				{ id: 'mercenary.support_56267', name: 'Pierce (Tier 2)', enabledInSearch: true },
				{ id: 'mercenary.support_27970', name: 'Greater Pierce (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_10482', name: 'Gilded Pierce (Tier 3)', enabledInSearch: true }
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
				{ id: 'mercenary.support_32052', name: 'Greater Fork (Tier 3)', enabledInSearch: true },
				{ id: 'mercenary.support_14317', name: 'Lesser Chain (Tier 1)', enabledInSearch: true },
				{ id: 'mercenary.support_31052', name: 'Chain (Tier 2)', enabledInSearch: true }
			]
		},
		{
			id: 'damage',
			label: 'Damage and critical links',
			type: 'mercenary',
			enabledInSearch: true,
			min: 3,
			entries: [
				{
					id: 'mercenary.skill_16356',
					name: 'Kinetic Blast of Clustering',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_61471', name: 'Critical Chance (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_62220',
					name: 'Greater Critical Chance (Tier 3)',
					enabledInSearch: true
				},
				{ id: 'mercenary.support_32189', name: 'Critical Damage (Tier 2)', enabledInSearch: true },
				{
					id: 'mercenary.support_55659',
					name: 'Greater Critical Damage (Tier 3)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_30688',
					name: 'Lesser Critical Damage (Tier 1)',
					enabledInSearch: true
				},
				{
					id: 'mercenary.support_23209',
					name: 'Lesser Critical Chance (Tier 1)',
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
					id: 'mercenary.support_59712',
					name: 'Lesser Elemental Damage with Attacks (Tier 1)',
					enabledInSearch: true
				}
			]
		},
		{
			// The lever, pulled. Same group, same two filters, same minimum as the
			// cheap rung's — only the group switch differs.
			id: 'return',
			label: 'Kinetic Blast + Return',
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
		}
	]
};

export const MERC_SOURCES: MercSource[] = [
	{
		id: 'guide-a',
		label: 'ckaiba',
		description: "ckaiba's seller-side floors — wealthyexile strategy 7062",
		guideUrl: 'https://wealthyexile.com/strategies/7062/alchgo_astrolabe__merc_boss_rushing',
		rulesets: [GUIDE_A_MANYSHOT, GUIDE_A_KINETIST_V1, GUIDE_A_COMBATANT]
	},
	{
		id: 'guide-b',
		label: 'Nerotox',
		description: "Nerotox's tiered saved searches — three videos, four ladders",
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
			GUIDE_B_MANYSHOT_GG,
			GUIDE_B_FROST_BLADES_MV,
			GUIDE_B_FROST_BLADES_MID,
			GUIDE_B_FROST_BLADES_END_NORETURN,
			GUIDE_B_FROST_BLADES_END_RETURN,
			GUIDE_B_FROST_BLADES_GG,
			GUIDE_B_WILD_STRIKE_MV,
			GUIDE_B_WILD_STRIKE_MID,
			GUIDE_B_WILD_STRIKE_END,
			GUIDE_B_WILD_STRIKE_GG
		]
	},
	{
		id: 'guide-c',
		label: 'CaptainLance',
		description:
			"CaptainLance's buyer-side ideal links for a Luminary merc bot — no prices, no floors",
		// One page, one section, four archetypes — so no ruleset here carries a
		// `guideUrl` of its own.
		guideUrl: CAPTAINLANCE_BUILD,
		rulesets: [
			GUIDE_C_KINETIST,
			GUIDE_C_MANYSHOT,
			GUIDE_C_BLADE_AMBUSHER,
			GUIDE_C_COMBATANT
		]
	},
	{
		id: 'guide-d',
		label: 'XTheFarmerX',
		description:
			"XTheFarmerX's budget life-stacking KB merc — two saved searches, the upper one Nerotox's own link",
		// ONE video publishes both rungs, so neither carries a `guideUrl` of its
		// own — unlike guide-b, whose ladders come off different videos.
		guideUrl: XTHEFARMERX_KB_VIDEO,
		rulesets: [GUIDE_D_KINETIST_BUDGET, GUIDE_D_KINETIST_20D]
	},
	{
		id: 'guide-e',
		label: 'sushi',
		description: "sushi's buyer-side archetype notes for Allflame 3.29 — no prices, no floors",
		// ONE image, six archetypes — so no ruleset here carries a `guideUrl` of
		// its own, the guide-c shape. The URL is the SHEET the image is embedded
		// in (Sheet1, cell E14), not a page of prose: there is no text to fetch.
		guideUrl: SUSHI_SHEET,
		rulesets: [
			GUIDE_E_SNIPER,
			GUIDE_E_KINETIST,
			GUIDE_E_COMBATANT,
			GUIDE_E_MANYSHOT,
			GUIDE_E_CRUEL_MISTRESS,
			GUIDE_E_STORMHAND
		]
	},
	{
		id: 'guide-f',
		label: 'Path of Evening',
		// "Buyer-side" is read off the searches, not the prose this repo cannot
		// fetch: no rung of the six quotes a price floor, which `rulesets.test.ts`
		// pins as `quotes no price floor on any rung`.
		description:
			"Path of Evening's buyer-side saved searches — three archetypes, a cheap and an expensive rung each",
		// One page publishes all six searches, so no ruleset here carries a
		// `guideUrl` of its own — the guide-c and guide-d shape, not guide-b's.
		guideUrl: PATH_OF_EVENING_BUILD,
		rulesets: [
			GUIDE_F_MANYSHOT_CHEAP,
			GUIDE_F_MANYSHOT_EXPENSIVE,
			GUIDE_F_COMBATANT_CHEAP,
			GUIDE_F_COMBATANT_EXPENSIVE,
			GUIDE_F_KINETIST_CHEAP,
			GUIDE_F_KINETIST_EXPENSIVE
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
