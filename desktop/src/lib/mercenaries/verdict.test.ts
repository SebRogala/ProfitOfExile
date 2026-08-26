import { describe, it, expect } from 'vitest';
import { evaluateCapture, type MercGroupResult, type MercVerdict } from './verdict';
import {
	MERC_SOURCES,
	SOURCE_IDS,
	type MercRuleset,
	type MercSource,
	type MercSourceId
} from './rulesets';
import type { MercCapture, MercRow, MercSkillRead, MercSupportRead, ReadState } from './capture';
import mercenaryStats from './__fixtures__/mercenary-stats.json';

/**
 * Captures here are hand-built, because the Rust reader that produces the real
 * ones does not exist yet on this branch. What is NOT hand-built is the names:
 * every read is labelled from GGG's own vocabulary, so a test that talks about
 * "Return (Tier 3)" is talking about the stat id the rulesets actually carry.
 */
const VOCABULARY = new Map(
	(mercenaryStats as { entries: { id: string; text: string }[] }).entries.map((e) => [e.id, e.text])
);

function textOf(id: string): string {
	const text = VOCABULARY.get(id);
	if (!text) throw new Error(`no Mercenary vocabulary entry for ${id}`);
	return text;
}

const ICE_SHOT = 'mercenary.skill_11495';
const VAAL_ICE_SHOT = 'mercenary.skill_16381';
const FROST_BLADES = 'mercenary.skill_22105';
const STATIC_STRIKE = 'mercenary.skill_24931';
const WILD_STRIKE = 'mercenary.skill_40957';
const KINETIC_BLAST_OF_CLUSTERING = 'mercenary.skill_16356';
const GREATER_KINETIC_BLAST = 'mercenary.skill_44258';
const BARRAGE = 'mercenary.skill_1356';
const HASTE = 'mercenary.skill_52155';
const HATRED = 'mercenary.skill_24482';
const HERALD_OF_ICE = 'mercenary.skill_32807';
const INSPIRING_CRY = 'mercenary.skill_65473';
const RETURN = 'mercenary.support_5293';
const GMP = 'mercenary.support_49419';
const COOLDOWN_RECOVERY = 'mercenary.support_48875';
const MULTISTRIKE = 'mercenary.support_62638';
const PIERCE = 'mercenary.support_56267';
const GREATER_FORK = 'mercenary.support_32052';
const CHAIN = 'mercenary.support_31052';
const EDWA = 'mercenary.support_44886';
const GREATER_EDWA = 'mercenary.support_28416';
const GREATER_HYPOTHERMIA = 'mercenary.support_53145';
const GREATER_FASTER_ATTACKS = 'mercenary.support_50485';
const FASTER_ATTACKS = 'mercenary.support_987';
const HYPOTHERMIA = 'mercenary.support_38571';
const CRITICAL_DAMAGE = 'mercenary.support_32189';
const GILDED_EXTRA_TARGETS = ['mercenary.support_58471', 'mercenary.support_37259'];

function skillRead(id: string, state: ReadState = 'matched'): MercSkillRead {
	return { raw: textOf(id), ids: [id], name: textOf(id), score: 0.99, state };
}

/** A row whose name OCR resolved to nothing — the reader's honest "I could not read this". */
function unreadSkill(): MercSkillRead {
	return { raw: 'K1net1c 8last of Clusler1ng', ids: [], name: null, score: 0.41, state: 'unknown' };
}

function supportRead(slot: number, id: string, state: ReadState = 'matched'): MercSupportRead {
	return {
		slot,
		rect: [372 + slot * 49, 593, 44, 44],
		family: textOf(id).replace(/^(Lesser|Greater|Gilded) /, '').replace(/ \(Tier \d\)$/, ''),
		tier: Number(/\(Tier (\d)\)$/.exec(textOf(id))?.[1] ?? 0) || null,
		ids: [id],
		name: textOf(id),
		score: 0.93,
		state,
		candidates: []
	};
}

/** A cell the template store could not name at all — no ids to test membership against. */
function unreadSupport(slot: number): MercSupportRead {
	return {
		slot,
		rect: [372 + slot * 49, 593, 44, 44],
		family: null,
		tier: null,
		ids: [],
		name: null,
		score: 0.52,
		state: 'unknown',
		candidates: []
	};
}

function row(index: number, skill: MercSkillRead, supports: MercSupportRead[] = []): MercRow {
	return { index, skill, supports };
}

function captureOf(rows: MercRow[]): MercCapture {
	return {
		capturedAtMs: 1_755_000_000_000,
		live: true,
		scale: 1,
		screen: [2560, 1440],
		header: { name: 'Cai, the Lout', class: 'Shock Ambusher', level: 70, wager: 1028 },
		rows
	};
}

function supportsOf(ids: string[]): MercSupportRead[] {
	return ids.map((id, slot) => supportRead(slot, id));
}

const ALL_SOURCES: ReadonlySet<MercSourceId> = new Set(SOURCE_IDS);

const LEAGUE = 'Mirage';

function verdictOf(capture: MercCapture, enabled = ALL_SOURCES, league: string | null = LEAGUE): MercVerdict {
	return evaluateCapture(capture, MERC_SOURCES, enabled, league);
}

function sourceOf(verdict: MercVerdict, sourceId: MercSourceId) {
	const found = verdict.sources.find((s) => s.id === sourceId);
	if (!found) throw new Error(`verdict has no source ${sourceId}`);
	return found;
}

function rulesetOf(verdict: MercVerdict, sourceId: MercSourceId, rulesetId: string) {
	const found = sourceOf(verdict, sourceId).rulesets.find((r) => r.id === rulesetId);
	if (!found) throw new Error(`verdict has no ruleset ${rulesetId}`);
	return found;
}

function groupOf(
	verdict: MercVerdict,
	sourceId: MercSourceId,
	rulesetId: string,
	groupId: string
): MercGroupResult {
	const found = rulesetOf(verdict, sourceId, rulesetId).groups.find((g) => g.id === groupId);
	if (!found) throw new Error(`ruleset ${rulesetId} has no group ${groupId}`);
	return found;
}

function positionOf(
	verdict: MercVerdict,
	sourceId: MercSourceId,
	rulesetId: string,
	groupId: string,
	entryId: string
) {
	const found = groupOf(verdict, sourceId, rulesetId, groupId).positions.find(
		(p) => p.entryId === entryId
	);
	if (!found) throw new Error(`group ${groupId} has no position for ${entryId}`);
	return found;
}

/** Decode the `q=` payload of a derived search back into the query the app built. */
function derivedQueryOf(url: string | null): {
	stats: { type: string; disabled?: boolean; value?: { min: number }; filters: { id: string; disabled?: boolean }[] }[];
} {
	if (url === null) throw new Error('expected a derived URL, got null');
	const encoded = new URL(url).searchParams.get('q');
	if (!encoded) throw new Error(`derived URL carries no q parameter: ${url}`);
	return JSON.parse(encoded).query;
}

function derivedGroup(url: string | null, index: number) {
	return derivedQueryOf(url).stats[index];
}

/**
 * The groups of a derived query that ask for more filters than they leave
 * switched ON — `{"value":{"min":2}}` over one enabled filter is a search the
 * trade site can never answer, so a derived link carrying one is a dead comp.
 * Reported as strings so a failure names the offending group instead of a
 * boolean.
 */
function unsatisfiableGroups(rulesetId: string, url: string | null): string[] {
	return derivedQueryOf(url)
		.stats.map((group) => ({
			type: group.type,
			min: group.value?.min ?? 0,
			enabled: group.filters.filter((filter) => filter.disabled !== true).length
		}))
		.filter((group) => group.min > group.enabled)
		.map((group) => `${rulesetId}: ${group.type} asks ${group.min} of ${group.enabled} enabled`);
}

/** Every passing ruleset of one source, in declaration order. */
function passingOf(verdict: MercVerdict, sourceId: MercSourceId) {
	return sourceOf(verdict, sourceId).rulesets.filter((result) => result.outcome === 'pass');
}

/**
 * A mercenary the guide-b ladder wants: Kinetic Blast of Clustering carrying
 * Return, a behaviour link and two damage links, with Greater Kinetic Blast as
 * the second skill. Satisfies MV, Mid and End; GG fails on the Greater Multiple
 * Projectiles its core group requires.
 */
function kinetistCapture(row0Supports: string[] = []): MercCapture {
	return captureOf([
		row(
			0,
			skillRead(KINETIC_BLAST_OF_CLUSTERING),
			supportsOf([RETURN, GREATER_FORK, CHAIN, GREATER_EDWA, CRITICAL_DAMAGE, ...row0Supports])
		),
		row(1, skillRead(GREATER_KINETIC_BLAST))
	]);
}

/** Both Manyshot rows read cleanly: Ice Shot + Return, Vaal Ice Shot + Return. */
function manyshotCapture(row0Supports: string[] = []): MercCapture {
	return captureOf([
		row(0, skillRead(ICE_SHOT), supportsOf([RETURN, ...row0Supports])),
		row(1, skillRead(VAAL_ICE_SHOT), supportsOf([RETURN]))
	]);
}

/**
 * A mercenary the guide-b Manyshot GG rung wants: Ice Shot carrying Return on one
 * row, Vaal Ice Shot carrying Return and two damage links on another, and Hatred
 * for the aura count group that rung leads with.
 */
function manyshotGgCapture(): MercCapture {
	return captureOf([
		row(0, skillRead(ICE_SHOT), supportsOf([RETURN])),
		row(1, skillRead(VAAL_ICE_SHOT), supportsOf([RETURN, GREATER_EDWA, HYPOTHERMIA])),
		row(2, skillRead(HATRED))
	]);
}

/**
 * A Combatant mercenary as Nerotox's Frost Blades ladder reads one: the three
 * skills every rung's `and` group requires, plus a Frost Blades row carrying
 * Chain and the two Tier-3 damage links every rung above Minimum asks for.
 *
 * `frostBladesLinks` is the whole variable: adding Greater Faster Attacks
 * answers the `speed` group, adding Return answers the `return` group, and
 * those two groups are the only difference between the ladder's five rungs.
 */
function frostBladesCapture(frostBladesLinks: string[] = []): MercCapture {
	return captureOf([
		row(
			0,
			skillRead(FROST_BLADES),
			supportsOf([CHAIN, GREATER_EDWA, GREATER_HYPOTHERMIA, ...frostBladesLinks])
		),
		row(1, skillRead(HERALD_OF_ICE)),
		row(2, skillRead(INSPIRING_CRY)),
		row(3, skillRead(STATIC_STRIKE))
	]);
}

/**
 * The same three required skills with a Wild Strike row instead — enough links
 * on it for the GG rung, whose `damage` group wants three of five, `greater`
 * two of three, `speed` two of three and `return` both of its entries.
 */
function wildStrikeCapture(): MercCapture {
	return captureOf([
		row(
			0,
			skillRead(WILD_STRIKE),
			supportsOf([GREATER_EDWA, GREATER_HYPOTHERMIA, GREATER_FASTER_ATTACKS, RETURN])
		),
		row(1, skillRead(HERALD_OF_ICE)),
		row(2, skillRead(INSPIRING_CRY)),
		row(3, skillRead(STATIC_STRIKE))
	]);
}

/**
 * The same Wild Strike mercenary carrying Faster Attacks (Tier 2) instead of the
 * Tier-3 link, and no Return. Midgame parks the Tier-2 entry INSIDE its live
 * `speed` group, so this mercenary answers the rungs either side of Midgame and
 * not Midgame itself.
 */
function wildStrikeTierTwoSpeedCapture(): MercCapture {
	return captureOf([
		row(
			0,
			skillRead(WILD_STRIKE),
			supportsOf([GREATER_EDWA, GREATER_HYPOTHERMIA, FASTER_ATTACKS])
		),
		row(1, skillRead(HERALD_OF_ICE)),
		row(2, skillRead(INSPIRING_CRY)),
		row(3, skillRead(STATIC_STRIKE))
	]);
}

/**
 * The Kinetist ladder's mercenary with a Vaal Ice Shot row bolted on — a capture
 * that is a rung of BOTH guide-b ladders at once (Kinetist End, and the Manyshot
 * Earlygame rung, whose only live gates are "has Vaal Ice Shot" and "no Icicle
 * Rain"). Contrived, and it has to be: the two ladders exist to be answered
 * separately, so the regression they can have needs one capture on both.
 */
function twoLadderCapture(): MercCapture {
	return captureOf([...kinetistCapture().rows, row(2, skillRead(VAAL_ICE_SHOT))]);
}

describe('row scoping of `mercenary` groups', () => {
	it('satisfies the Ice Shot group when the skill and its Return sit in one row', () => {
		const core = groupOf(verdictOf(manyshotCapture()), 'guide-a', 'guide-a-manyshot', 'core');
		expect([core.outcome, core.rowIndex, core.confident, core.need]).toEqual(['pass', 0, 2, 2]);
	});

	// Assumption A1's falsification case: Return IS in the capture, on the Vaal
	// Ice Shot row. If `mercenary` groups were capture-scoped, this would pass.
	it('fails the Ice Shot group when the only Return sits on a different skill row', () => {
		const verdict = verdictOf(
			captureOf([
				row(0, skillRead(ICE_SHOT)),
				row(1, skillRead(VAAL_ICE_SHOT), supportsOf([RETURN]))
			])
		);
		expect(groupOf(verdict, 'guide-a', 'guide-a-manyshot', 'core').outcome).toBe('fail');
		expect(groupOf(verdict, 'guide-a', 'guide-a-manyshot', 'secondary').outcome).toBe('pass');
		expect(rulesetOf(verdict, 'guide-a', 'guide-a-manyshot').reasons).toContain(
			'Ice Shot + links on row 1: needs 2, has 1 — missing Return (Tier 3)'
		);
	});

	it('names the closest row, not the first, when no row satisfies the group', () => {
		// The End rung's damage group needs 3 of its entries. Row 1 reaches 2 of
		// them, row 0 none — so the row worth looking at is the second one.
		const verdict = verdictOf(
			captureOf([
				row(0, skillRead(GREATER_KINETIC_BLAST)),
				row(1, skillRead(KINETIC_BLAST_OF_CLUSTERING), supportsOf([GREATER_EDWA]))
			])
		);
		const damage = groupOf(verdict, 'guide-b', 'guide-b-kinetist-end', 'damage');
		expect([damage.outcome, damage.rowIndex, damage.confident, damage.need]).toEqual([
			'fail',
			1,
			2,
			3
		]);
	});

	it('counts a `count` group across rows, not within one', () => {
		// Greater Kinetic Blast is on row 1 while the guide-b core skill is on row 0.
		const secondary = groupOf(verdictOf(kinetistCapture()), 'guide-b', 'guide-b-kinetist-mv', 'secondary');
		expect([secondary.outcome, secondary.rowIndex]).toEqual(['pass', null]);
	});
});

describe('unread cells', () => {
	it('reports unknown, not fail, when the row Return is only a low-confidence read', () => {
		const verdict = verdictOf(
			captureOf([
				row(0, skillRead(ICE_SHOT), [supportRead(0, RETURN, 'low_confidence')]),
				row(1, skillRead(VAAL_ICE_SHOT), supportsOf([RETURN]))
			])
		);
		expect(groupOf(verdict, 'guide-a', 'guide-a-manyshot', 'core').outcome).toBe('unknown');
		expect(rulesetOf(verdict, 'guide-a', 'guide-a-manyshot').outcome).toBe('unknown');
	});

	it('reports the source unknown when nothing passes and something could not be read', () => {
		const verdict = verdictOf(
			captureOf([
				row(0, skillRead(ICE_SHOT), [supportRead(0, RETURN, 'low_confidence')]),
				row(1, skillRead(VAAL_ICE_SHOT), supportsOf([RETURN]))
			])
		);
		expect(sourceOf(verdict, 'guide-a').headline).toBe('unknown');
	});

	it('cannot rule out a denied support while any cell of the row is unread', () => {
		const capture = captureOf([
			row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING), [
				supportRead(0, RETURN),
				supportRead(1, GREATER_FORK),
				supportRead(2, CHAIN),
				supportRead(3, GREATER_EDWA),
				unreadSupport(4)
			]),
			row(1, skillRead(GREATER_KINETIC_BLAST))
		]);
		const verdict = verdictOf(capture);
		expect(groupOf(verdict, 'guide-b', 'guide-b-kinetist-mid', 'deny-supports').outcome).toBe(
			'unknown'
		);
		// The cells that WERE read still answer for themselves.
		expect(groupOf(verdict, 'guide-b', 'guide-b-kinetist-mid', 'core').outcome).toBe('pass');
	});

	it('keeps a confidently forbidden skill a fail even when other cells are unread', () => {
		const verdict = verdictOf(
			captureOf([
				row(0, skillRead(FROST_BLADES), [supportRead(0, RETURN), unreadSupport(1)]),
				row(1, skillRead(STATIC_STRIKE)),
				row(2, skillRead(WILD_STRIKE))
			])
		);
		expect(rulesetOf(verdict, 'guide-a', 'guide-a-combatant').outcome).toBe('fail');
	});

	it('leaves an unreadable skill row unable to satisfy anything on its own', () => {
		const verdict = verdictOf(
			captureOf([row(0, unreadSkill(), supportsOf([RETURN, GREATER_FORK, CHAIN]))])
		);
		expect(groupOf(verdict, 'guide-b', 'guide-b-kinetist-mv', 'core').outcome).toBe('unknown');
	});
});

describe('buyer-contextual entries', () => {
	it('passes the MV rung with Haste absent', () => {
		const verdict = verdictOf(kinetistCapture());
		expect(rulesetOf(verdict, 'guide-b', 'guide-b-kinetist-mv').outcome).toBe('pass');
		expect(positionOf(verdict, 'guide-b', 'guide-b-kinetist-mv', 'auras', HASTE).outcome).toBe(
			'contextual-absent'
		);
	});

	it('reads a Haste-only aura group as not-applied even when Haste is present', () => {
		const capture = captureOf([
			row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING), supportsOf([RETURN])),
			row(1, skillRead(HASTE))
		]);
		const verdict = verdictOf(capture);
		expect(groupOf(verdict, 'guide-a', 'guide-a-kinetist-v1', 'auras').outcome).toBe('not-applied');
		expect(positionOf(verdict, 'guide-a', 'guide-a-kinetist-v1', 'auras', HASTE).outcome).toBe(
			'contextual-present'
		);
	});

	it('counts a present Barrage toward the MV minimum that Greater Kinetic Blast misses', () => {
		const capture = captureOf([
			row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING), supportsOf([RETURN, GREATER_FORK, CHAIN, GREATER_EDWA, CRITICAL_DAMAGE])),
			row(1, skillRead(BARRAGE))
		]);
		const verdict = verdictOf(capture);
		expect(groupOf(verdict, 'guide-b', 'guide-b-kinetist-mv', 'secondary').outcome).toBe('pass');
		expect(positionOf(verdict, 'guide-b', 'guide-b-kinetist-mv', 'secondary', BARRAGE).outcome).toBe(
			'contextual-present'
		);
	});

	it('does not count Barrage toward the Mid minimum, where the guide switched it off', () => {
		const capture = captureOf([
			row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING), supportsOf([RETURN, GREATER_FORK, CHAIN, GREATER_EDWA, CRITICAL_DAMAGE])),
			row(1, skillRead(BARRAGE))
		]);
		const verdict = verdictOf(capture);
		expect(groupOf(verdict, 'guide-b', 'guide-b-kinetist-mid', 'secondary').outcome).toBe('fail');
		expect(positionOf(verdict, 'guide-b', 'guide-b-kinetist-mid', 'secondary', BARRAGE).outcome).toBe(
			'bonus-fired'
		);
	});
});

describe('denials', () => {
	it('reports a parked denial as present without failing the rung that parked it', () => {
		const verdict = verdictOf(kinetistCapture([PIERCE]));
		expect(positionOf(verdict, 'guide-b', 'guide-b-kinetist-mv', 'deny-supports', PIERCE).outcome).toBe(
			'parked-denial-present'
		);
		expect(rulesetOf(verdict, 'guide-b', 'guide-b-kinetist-mv').outcome).toBe('pass');
	});

	it('fails the Mid rung on the same Pierce the MV rung parks', () => {
		const verdict = verdictOf(kinetistCapture([PIERCE]));
		expect(groupOf(verdict, 'guide-b', 'guide-b-kinetist-mid', 'deny-supports').outcome).toBe('fail');
		expect(rulesetOf(verdict, 'guide-b', 'guide-b-kinetist-mid').outcome).toBe('fail');
	});

	it('leaves the parked deny-supports group parked in the MV derived query', () => {
		const url = rulesetOf(verdictOf(kinetistCapture([PIERCE])), 'guide-b', 'guide-b-kinetist-mv').derivedUrl;
		const denySupports = derivedGroup(url, 4);
		expect(denySupports.type).toBe('not');
		expect(denySupports.disabled).toBe(true);
		expect(denySupports.filters).toEqual([{ id: PIERCE }, { id: 'mercenary.support_27970' }]);
	});
});

describe('sources are evaluated independently', () => {
	it('skips a Barrage Kinetist for guide A while guide B calls it worth', () => {
		const capture = captureOf([
			row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING), supportsOf([RETURN, GREATER_FORK, CHAIN, GREATER_EDWA, CRITICAL_DAMAGE])),
			row(1, skillRead(BARRAGE))
		]);
		const verdict = verdictOf(capture);
		expect(verdict.sources.map((s) => [s.id, s.headline])).toEqual([
			['guide-a', 'skip'],
			['guide-b', 'worth']
		]);
		expect(sourceOf(verdict, 'guide-b').best).toEqual(['guide-b-kinetist-mv']);
	});

	it('reports a source the caller switched off as off, with nothing evaluated', () => {
		const verdict = verdictOf(manyshotCapture(), new Set<MercSourceId>(['guide-b']));
		expect(sourceOf(verdict, 'guide-a').headline).toBe('off');
		expect(sourceOf(verdict, 'guide-a').rulesets).toEqual([]);
	});
});

describe('ladder selection', () => {
	it('names the End rung as guide B best when GG fails on its required GMP', () => {
		const verdict = verdictOf(kinetistCapture());
		expect(sourceOf(verdict, 'guide-b').best).toEqual(['guide-b-kinetist-end']);
		expect(rulesetOf(verdict, 'guide-b', 'guide-b-kinetist-gg').outcome).toBe('fail');
	});

	it('caveats a GG pass with the core row link count', () => {
		// GMP on the core row completes the GG rung: 6 supports on Kinetic Blast.
		// The line says nothing about HOW many links the search asks for — that
		// number is the guide author's, and it travels in the authorNote below.
		const gg = rulesetOf(verdictOf(kinetistCapture([GMP])), 'guide-b', 'guide-b-kinetist-gg');
		expect(gg.outcome).toBe('pass');
		expect(gg.reasons).toContain(
			'GG comps are a floor, not the price: a merc with more links prices above them — this core skill row carries 6 supports'
		);
	});

	it("relays the Kinetist author's GG note verbatim on a GG pass", () => {
		const gg = rulesetOf(verdictOf(kinetistCapture([GMP])), 'guide-b', 'guide-b-kinetist-gg');
		expect(gg.reasons).toContain(
			'Author: look at damage links still - these are 4L not even 5L. 5L mercs nearly never exist for KB in gg setups, a 5L with barrage can beat a 4L with greater KB'
		);
	});

	it("relays the Manyshot author's own GG note, not the Kinetist one", () => {
		const gg = rulesetOf(verdictOf(manyshotGgCapture()), 'guide-b', 'guide-b-manyshot-gg');
		expect(gg.outcome).toBe('pass');
		expect(gg.reasons).toContain('Author: manually check for clear links on ice shot');
		// Spelled out rather than matched on a prefix: a module-level GG note
		// relayed by every ladder is exactly the regression this guards, and it
		// would carry this text verbatim.
		expect(gg.reasons).not.toContain(
			'Author: look at damage links still - these are 4L not even 5L. 5L mercs nearly never exist for KB in gg setups, a 5L with barrage can beat a 4L with greater KB'
		);
	});

	it('does not put the GG link caveat on a lower rung', () => {
		const end = rulesetOf(verdictOf(kinetistCapture()), 'guide-b', 'guide-b-kinetist-end');
		expect(end.outcome).toBe('pass');
		expect(end.reasons.some((reason) => reason.startsWith('GG comps'))).toBe(false);
		expect(end.reasons.some((reason) => reason.startsWith('Author:'))).toBe(false);
	});

	// Nerotox's Combatant video publishes two Endgame links for one ladder, so
	// the ranking cannot assume one rung per tier — and the tie has to resolve to
	// the guide's own declaration order. No real ladder has a tie yet, so this is
	// the only place the rule can be observed.
	it('keeps the earlier-declared rung when two rungs of one ladder share a tier', () => {
		const tiedRung = (id: string): MercRuleset => ({
			id,
			label: 'Synthetic',
			archetype: 'combatant',
			ladder: 'synthetic',
			tier: 'end',
			savedSearch: { league: 'Allflame', hash: id },
			status: 'securable',
			// One live denial the capture below does not trip: enough to make the
			// rung APPLY something, so both rungs land on a pass and tie.
			groups: [
				{
					id: 'deny',
					label: 'Denied skills',
					type: 'not',
					enabledInSearch: true,
					entries: [{ id: WILD_STRIKE, name: textOf(WILD_STRIKE), enabledInSearch: true }]
				}
			]
		});
		const source: MercSource = {
			id: 'guide-b',
			label: 'Synthetic',
			guideUrl: null,
			rulesets: [tiedRung('end-first'), tiedRung('end-second')]
		};
		const verdict = evaluateCapture(
			captureOf([row(0, skillRead(ICE_SHOT))]),
			[source],
			new Set<MercSourceId>(['guide-b']),
			LEAGUE
		);
		expect(verdict.sources[0].best).toEqual(['end-first']);
	});

	// A merc can only be one archetype in practice, but the rule is per ladder:
	// two ladders are two searches, and neither may suppress the other's answer.
	it('names the highest passing rung of every ladder, not one for the whole source', () => {
		expect(sourceOf(verdictOf(twoLadderCapture()), 'guide-b').best).toEqual([
			'guide-b-kinetist-end',
			'guide-b-manyshot-mv'
		]);
	});

	// The Frost Blades ladder seats TWO rungs at `end`, and `bestOf` breaks the
	// tie by declaration order. These three pin why that tie is harmless: the two
	// Endgame rungs are Midgame plus one switch each, so a capture answering both
	// answers GG too and the tie never has to be broken on a real search.
	it('names the Endgame (no return) rung when the merc has the speed link and no Return', () => {
		const verdict = verdictOf(frostBladesCapture([GREATER_FASTER_ATTACKS]));
		expect(sourceOf(verdict, 'guide-b').best).toEqual(['guide-b-frost-blades-end-noreturn']);
	});

	it('fails the Endgame (return) rung on the Return that same merc lacks', () => {
		const verdict = verdictOf(frostBladesCapture([GREATER_FASTER_ATTACKS]));
		expect(rulesetOf(verdict, 'guide-b', 'guide-b-frost-blades-end-return').outcome).toBe('fail');
	});

	it('names the Endgame (return) rung when the merc has Return and no speed link', () => {
		const verdict = verdictOf(frostBladesCapture([RETURN]));
		expect(sourceOf(verdict, 'guide-b').best).toEqual(['guide-b-frost-blades-end-return']);
	});

	it('fails the Endgame (no return) rung on the speed link that same merc lacks', () => {
		const verdict = verdictOf(frostBladesCapture([RETURN]));
		expect(rulesetOf(verdict, 'guide-b', 'guide-b-frost-blades-end-noreturn').outcome).toBe('fail');
	});

	it('names GG, not either Endgame rung, when the merc answers both of them', () => {
		const verdict = verdictOf(frostBladesCapture([GREATER_FASTER_ATTACKS, RETURN]));
		expect(
			['end-noreturn', 'end-return', 'gg'].map(
				(rung) => `${rung} ${rulesetOf(verdict, 'guide-b', `guide-b-frost-blades-${rung}`).outcome}`
			)
		).toEqual(['end-noreturn pass', 'end-return pass', 'gg pass']);
		expect(sourceOf(verdict, 'guide-b').best).toEqual(['guide-b-frost-blades-gg']);
	});

	// The caveat names the row the rung's main skill sits on. The Wild Strike
	// searches declare no `core` group at all, so the row has to come from the
	// first live `mercenary` group — its damage group — instead.
	it('caveats a GG pass on a ladder with no core group using its first live mercenary group', () => {
		const gg = rulesetOf(verdictOf(wildStrikeCapture()), 'guide-b', 'guide-b-wild-strike-gg');
		expect(gg.outcome).toBe('pass');
		expect(gg.reasons).toContain(
			'GG comps are a floor, not the price: a merc with more links prices above them — this core skill row carries 4 supports'
		);
	});

	// The fallback must stay a fallback: the Manyshot GG rung DOES declare `core`
	// but does not lead with it, and its own authorNote sends the reader to the
	// Ice Shot links — the row `core` answers, not the row its first live
	// mercenary group (`vaal-damage`, three supports) answers.
	it('keeps counting the declared core group’s row when a rung leads with another', () => {
		const gg = rulesetOf(verdictOf(manyshotGgCapture()), 'guide-b', 'guide-b-manyshot-gg');
		expect(gg.reasons).toContain(
			'GG comps are a floor, not the price: a merc with more links prices above them — this core skill row carries 1 support'
		);
	});

	// `bestOf` ranks by TIERS and nothing else — it does NOT assume a ladder's
	// rungs are nested. Wild Strike Midgame parks Faster Attacks (Tier 2) inside
	// a live `speed` group, so a mercenary carrying the Tier-2 link and not the
	// Tier-3 one answers the rungs either side of Midgame and not Midgame itself.
	it('passes the Minimum and Endgame rungs while the Midgame rung between them fails', () => {
		const verdict = verdictOf(wildStrikeTierTwoSpeedCapture());
		expect(
			['mv', 'mid', 'end', 'gg'].map(
				(rung) => `${rung} ${rulesetOf(verdict, 'guide-b', `guide-b-wild-strike-${rung}`).outcome}`
			)
		).toEqual(['mv pass', 'mid fail', 'end pass', 'gg fail']);
	});

	it('names the Endgame rung best even though the Midgame rung below it failed', () => {
		expect(sourceOf(verdictOf(wildStrikeTierTwoSpeedCapture()), 'guide-b').best).toEqual([
			'guide-b-wild-strike-end'
		]);
	});

	it('lists every passing archetype for an untiered source', () => {
		// Ice Shot + Vaal Ice Shot passes Manyshot; nothing here passes the other two.
		const verdict = verdictOf(manyshotCapture());
		expect(sourceOf(verdict, 'guide-a').best).toEqual(['guide-a-manyshot']);
	});
});

describe('bonus channel', () => {
	it('fires the Greater Multiple Projectiles bonus parked in the Manyshot core group', () => {
		const verdict = verdictOf(manyshotCapture([GMP]));
		expect(positionOf(verdict, 'guide-a', 'guide-a-manyshot', 'core', GMP).outcome).toBe('bonus-fired');
	});

	it('switches a fired bonus on in the derived query and leaves an unfired one off', () => {
		const url = rulesetOf(verdictOf(manyshotCapture([GMP])), 'guide-a', 'guide-a-manyshot').derivedUrl;
		expect(derivedGroup(url, 1).filters).toEqual([{ id: ICE_SHOT }, { id: RETURN }, { id: GMP }]);
		expect(derivedGroup(url, 2).filters).toEqual([
			{ id: VAAL_ICE_SHOT },
			{ id: RETURN },
			{ id: COOLDOWN_RECOVERY, disabled: true }
		]);
	});

	it('reads a group whose every entry is switched off as not-applied', () => {
		// Manyshot's aura group is live but all three auras are parked toggles.
		const auras = groupOf(verdictOf(manyshotCapture()), 'guide-a', 'guide-a-manyshot', 'auras');
		expect(auras.outcome).toBe('not-applied');
	});

	it('switches an absent buyer-contextual entry off in the derived query', () => {
		const url = rulesetOf(verdictOf(kinetistCapture()), 'guide-b', 'guide-b-kinetist-mv').derivedUrl;
		expect(derivedGroup(url, 5).filters).toEqual([{ id: HASTE, disabled: true }]);
	});
});

describe('the row anchor of a `mercenary` group', () => {
	// A `mercenary` group is row-scoped to its own skill, so that skill is there
	// whenever the group can say anything at all. Reading it as a fired bonus made
	// every parked group of the ladder "fire" on any capture carrying the skill.
	it('reads a parked group’s own skill as present rather than as a fired bonus', () => {
		const verdict = verdictOf(frostBladesCapture());
		expect(groupOf(verdict, 'guide-b', 'guide-b-frost-blades-mv', 'speed').outcome).toBe(
			'not-applied'
		);
		expect(
			positionOf(verdict, 'guide-b', 'guide-b-frost-blades-mv', 'speed', FROST_BLADES).outcome
		).toBe('pass');
	});

	it('leaves the main skill out of the bonuses a passing rung reports', () => {
		const mv = rulesetOf(verdictOf(frostBladesCapture()), 'guide-b', 'guide-b-frost-blades-mv');
		expect(mv.outcome).toBe('pass');
		expect(mv.reasons.filter((reason) => reason.startsWith('Bonuses fired'))).toEqual([]);
	});

	// The rule is scoped to `mercenary` groups: guide-a's Manyshot aura group is
	// an `and` group of three parked SKILLS, and those are real bonuses about the
	// mercenary rather than a label for a row.
	it('still fires a parked skill bonus in an `and` group', () => {
		const verdict = verdictOf(manyshotGgCapture());
		expect(positionOf(verdict, 'guide-a', 'guide-a-manyshot', 'auras', HATRED).outcome).toBe(
			'bonus-fired'
		);
	});

	/**
	 * Every capture this file builds, swept across BOTH sources. The invariant is
	 * general — no derived link may carry a group asking for more filters than it
	 * leaves switched on — so the sweep is over everything rather than the two
	 * ladders whose bugs found it.
	 *
	 * Two ways this went wrong, both measured 2026-08-26: a parked group revived
	 * by its own row anchor (Frost Blades Minimum's `speed`, `min: 2` over one
	 * enabled filter) and a parked group revived by fewer bonuses than its `min`
	 * (the Manyshot GG rung's `projectiles`, `min: 4` over three).
	 */
	const SWEPT_CAPTURES: [string, MercCapture][] = [
		['kinetist', kinetistCapture()],
		['kinetist + GMP', kinetistCapture([GMP])],
		['kinetist + Pierce', kinetistCapture([PIERCE])],
		['manyshot', manyshotCapture()],
		['manyshot + GMP', manyshotCapture([GMP])],
		['manyshot GG', manyshotGgCapture()],
		['frost blades', frostBladesCapture()],
		['frost blades + Faster Attacks', frostBladesCapture([FASTER_ATTACKS])],
		['frost blades + speed', frostBladesCapture([GREATER_FASTER_ATTACKS])],
		['frost blades + Return', frostBladesCapture([RETURN])],
		['frost blades + both', frostBladesCapture([GREATER_FASTER_ATTACKS, RETURN])],
		['wild strike', wildStrikeCapture()],
		['wild strike tier-2 speed', wildStrikeTierTwoSpeedCapture()],
		['two ladders', twoLadderCapture()]
	];

	function sweep(): { visited: string[]; offenders: string[] } {
		const visited: string[] = [];
		const offenders: string[] = [];
		for (const [what, capture] of SWEPT_CAPTURES) {
			const verdict = verdictOf(capture);
			for (const sourceId of SOURCE_IDS) {
				for (const result of passingOf(verdict, sourceId)) {
					visited.push(result.id);
					offenders.push(...unsatisfiableGroups(`${what} / ${result.id}`, result.derivedUrl));
				}
			}
		}
		return { visited, offenders };
	}

	// A sweep that stopped passing anything would report no offenders and look
	// green, so what it reached is asserted as well as what it found. Every
	// guide-b ladder and both untiered guide-a rulesets that any of these
	// captures can answer are in here.
	it('sweeps a passing rung of every ladder and both guide-a archetypes', () => {
		expect([...new Set(sweep().visited)].sort()).toEqual([
			'guide-a-combatant',
			'guide-a-kinetist-v1',
			'guide-a-manyshot',
			'guide-b-frost-blades-end-noreturn',
			'guide-b-frost-blades-end-return',
			'guide-b-frost-blades-gg',
			'guide-b-frost-blades-mid',
			'guide-b-frost-blades-mv',
			'guide-b-kinetist-end',
			'guide-b-kinetist-gg',
			'guide-b-kinetist-mid',
			'guide-b-kinetist-mv',
			'guide-b-manyshot-gg',
			'guide-b-manyshot-mid',
			'guide-b-manyshot-mv',
			'guide-b-wild-strike-end',
			'guide-b-wild-strike-gg',
			'guide-b-wild-strike-mid',
			'guide-b-wild-strike-mv'
		]);
	});

	it('leaves no derived group asking for more filters than it has switched on', () => {
		expect(sweep().offenders).toEqual([]);
	});

	// The clamp, at the one rung that needs it: `projectiles` is parked with a
	// `min` written for all eight of its links, and this mercenary fires two of
	// them, so the revived group asks for the three filters it actually has.
	it('clamps a revived group’s minimum down to the filters that survived the revival', () => {
		const gg = rulesetOf(verdictOf(manyshotGgCapture()), 'guide-b', 'guide-b-manyshot-gg');
		const projectiles = derivedGroup(gg.derivedUrl, 4);
		expect(projectiles.disabled).toBeUndefined();
		expect([projectiles.value, projectiles.filters]).toEqual([
			{ min: 3 },
			[
				{ id: ICE_SHOT },
				{ id: GMP, disabled: true },
				{ id: GREATER_FORK, disabled: true },
				{ id: CHAIN, disabled: true },
				{ id: EDWA, disabled: true },
				{ id: GREATER_EDWA },
				{ id: HYPOTHERMIA },
				{ id: GREATER_HYPOTHERMIA, disabled: true }
			]
		]);
	});

	// A group the flips left alone keeps the guide's own number: the Manyshot GG
	// rung's live `vaal-damage` asks 3 of 5, nothing in it toggled, and it still
	// asks 3.
	it('leaves a group the flips did not touch on the guide’s own minimum', () => {
		const gg = rulesetOf(verdictOf(manyshotGgCapture()), 'guide-b', 'guide-b-manyshot-gg');
		expect(derivedGroup(gg.derivedUrl, 2).value).toEqual({ min: 3 });
	});

	// The revival path itself still works: a REAL bonus inside the parked group
	// switches the group on, and the anchor stays enabled so the `min` can be met.
	it('revives a parked group with its anchor still switched on', () => {
		const mv = rulesetOf(
			verdictOf(frostBladesCapture([FASTER_ATTACKS])),
			'guide-b',
			'guide-b-frost-blades-mv'
		);
		const speed = derivedGroup(mv.derivedUrl, 4);
		expect(speed.disabled).toBeUndefined();
		expect([speed.value, speed.filters]).toEqual([
			{ min: 2 },
			[
				{ id: FROST_BLADES },
				{ id: FASTER_ATTACKS },
				{ id: GREATER_FASTER_ATTACKS, disabled: true }
			]
		]);
	});
});

describe('captures outside the rules', () => {
	it('lists a confidently read support no entry of the ruleset mentions', () => {
		const verdict = verdictOf(manyshotCapture([MULTISTRIKE]));
		expect(rulesetOf(verdict, 'guide-a', 'guide-a-manyshot').notInRules).toEqual([
			{ ids: [MULTISTRIKE], name: 'Multistrike (Tier 2)', site: { rowIndex: 0, slot: 1 } }
		]);
	});

	it('leaves an unread cell out of the not-in-rules list', () => {
		const verdict = verdictOf(
			captureOf([row(0, skillRead(ICE_SHOT), [supportRead(0, RETURN), unreadSupport(1)])])
		);
		expect(rulesetOf(verdict, 'guide-a', 'guide-a-manyshot').notInRules).toEqual([]);
	});
});

describe('multi-id reads', () => {
	// `Gilded Extra Targets (Tier 3)` is the one display name in GGG's Mercenary
	// vocabulary that resolves to two stat ids, so a confirmed hover on that cell
	// carries both. No shipped ruleset uses it — this synthetic source is how the
	// set-membership rule gets exercised at all.
	const SYNTHETIC: MercSource = {
		id: 'guide-a',
		label: 'Synthetic',
		guideUrl: null,
		rulesets: [
			{
				id: 'synthetic-multi-id',
				label: 'Synthetic',
				archetype: 'kinetist',
				savedSearch: { league: 'Allflame', hash: 'synthetic' },
				status: 'securable',
				groups: [
					{
						id: 'core',
						label: 'Core',
						type: 'and',
						enabledInSearch: true,
						entries: [
							{
								id: GILDED_EXTRA_TARGETS[1],
								name: 'Gilded Extra Targets (Tier 3)',
								enabledInSearch: true
							}
						]
					}
				]
			}
		]
	};

	it('counts an entry id carried alongside another id in the same read', () => {
		const cell: MercSupportRead = {
			slot: 0,
			rect: [372, 593, 44, 44],
			family: 'Extra Targets',
			tier: 3,
			ids: GILDED_EXTRA_TARGETS,
			name: 'Gilded Extra Targets (Tier 3)',
			score: 1,
			state: 'confirmed',
			candidates: []
		};
		const verdict = evaluateCapture(
			captureOf([row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING), [cell])]),
			[SYNTHETIC],
			ALL_SOURCES,
			LEAGUE
		);
		expect(groupOf(verdict, 'guide-a', 'synthetic-multi-id', 'core').outcome).toBe('pass');
	});
});

describe('reasons', () => {
	it('names the forbidden stat that caused a skip', () => {
		const verdict = verdictOf(
			captureOf([
				row(0, skillRead(FROST_BLADES), supportsOf([RETURN])),
				row(1, skillRead(STATIC_STRIKE)),
				row(2, skillRead(WILD_STRIKE))
			])
		);
		expect(sourceOf(verdict, 'guide-a').reasons).toContain(
			'Combatant: Wild Strike present — forbidden'
		);
	});

	it('names what a failing group was missing', () => {
		const verdict = verdictOf(captureOf([row(0, skillRead(ICE_SHOT))]));
		expect(rulesetOf(verdict, 'guide-a', 'guide-a-manyshot').reasons).toContain(
			'Ice Shot + links on row 1: needs 2, has 1 — missing Return (Tier 3)'
		);
	});

	it('names the floor and the bonuses that fired on a worth verdict', () => {
		const verdict = verdictOf(manyshotCapture([GMP]));
		expect(sourceOf(verdict, 'guide-a').reasons).toEqual([
			'Manyshot: Floor 5d+ with just Return on both',
			'Manyshot: Bonuses fired: Greater Multiple Projectiles (Tier 3)'
		]);
	});

	// All four Kinetist rungs pass this capture; only GG is best, and only GG
	// speaks. The rung count is asserted first because `every` over an empty list
	// is true — a silent pass is what this test exists to not do.
	it('speaks only for the rungs it calls best', () => {
		const verdict = verdictOf(kinetistCapture([GMP]));
		expect(passingOf(verdict, 'guide-b').map((result) => result.id)).toEqual([
			'guide-b-kinetist-mv',
			'guide-b-kinetist-mid',
			'guide-b-kinetist-end',
			'guide-b-kinetist-gg'
		]);
		const reasons = sourceOf(verdict, 'guide-b').reasons;
		expect(reasons.length).toBeGreaterThan(0);
		expect(reasons.every((reason) => reason.startsWith('Kinetist (gg):'))).toBe(true);
	});

	// Two rungs of the Frost Blades ladder are both `end`; the tier key alone
	// would head both reason lines 'Frost Blades (end)'. A capture that passes
	// NOTHING is what makes all five rungs speak at once, which is the only way
	// to see the two Endgame titles side by side.
	it('titles a rung’s reasons with its own tier wording when the tier is shared', () => {
		const reasons = sourceOf(verdictOf(captureOf([row(0, skillRead(ICE_SHOT))])), 'guide-b').reasons;
		const titles = reasons
			.filter((reason) => reason.startsWith('Frost Blades'))
			.map((reason) => reason.split(':')[0]);
		expect([...new Set(titles)]).toEqual([
			'Frost Blades (mv)',
			'Frost Blades (mid)',
			'Frost Blades (endgame (no return))',
			'Frost Blades (endgame (return))',
			'Frost Blades (gg)'
		]);
	});

	// The other side of the same rule: a rung that does not spell its tier out
	// still reads exactly as it did before `tierLabel` existed.
	it('titles a rung with no wording of its own with the bare tier key', () => {
		const reasons = sourceOf(verdictOf(manyshotGgCapture()), 'guide-b').reasons;
		expect(reasons.length).toBeGreaterThan(0);
		expect(reasons.every((reason) => reason.startsWith('Manyshot (gg):'))).toBe(true);
	});
});

describe('derived links follow the active league', () => {
	it('builds the derived search for the league being played, not the league the search was saved in', () => {
		const manyshot = rulesetOf(verdictOf(manyshotCapture()), 'guide-a', 'guide-a-manyshot');
		expect(new URL(manyshot.derivedUrl as string).pathname).toBe('/trade/search/Mirage');
		expect(manyshot.savedUrl).toContain('/trade/search/Allflame/WvKGjV8Kfm');
	});

	it('reports no derived link at all while the league is unknown', () => {
		const manyshot = rulesetOf(
			verdictOf(manyshotCapture(), ALL_SOURCES, null),
			'guide-a',
			'guide-a-manyshot'
		);
		expect(manyshot.derivedUrl).toBeNull();
	});

	it('still verdicts the capture when the league is unknown', () => {
		const verdict = verdictOf(manyshotCapture(), ALL_SOURCES, null);
		expect(sourceOf(verdict, 'guide-a').headline).toBe('worth');
	});
});

describe('synthetic rulesets', () => {
	function oneGroup(group: MercSource['rulesets'][number]['groups'][number]): MercSource {
		return {
			id: 'guide-a',
			label: 'Synthetic',
			guideUrl: null,
			rulesets: [
				{
					id: 'synthetic',
					label: 'Synthetic',
					archetype: 'kinetist',
					savedSearch: { league: 'Allflame', hash: 'synthetic' },
					status: 'securable',
					groups: [group]
				}
			]
		};
	}

	// A `min: 2` over one plain entry and one buyer-contextual entry asks for
	// both only from a mercenary that HAS the contextual one.
	it('drops the minimum with the buyer-contextual entry the mercenary lacks', () => {
		const source = oneGroup({
			id: 'core',
			label: 'Core',
			type: 'and',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: KINETIC_BLAST_OF_CLUSTERING, name: textOf(KINETIC_BLAST_OF_CLUSTERING), enabledInSearch: true },
				{ id: HASTE, name: textOf(HASTE), enabledInSearch: true, buyerContextual: true }
			]
		});
		const verdict = evaluateCapture(
			captureOf([row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING))]),
			[source],
			ALL_SOURCES,
			LEAGUE
		);
		const core = groupOf(verdict, 'guide-a', 'synthetic', 'core');
		expect([core.outcome, core.need, core.confident]).toEqual(['pass', 1, 1]);
	});

	it('keeps the full minimum when the mercenary has the buyer-contextual entry', () => {
		const source = oneGroup({
			id: 'core',
			label: 'Core',
			type: 'and',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: KINETIC_BLAST_OF_CLUSTERING, name: textOf(KINETIC_BLAST_OF_CLUSTERING), enabledInSearch: true },
				{ id: HASTE, name: textOf(HASTE), enabledInSearch: true, buyerContextual: true }
			]
		});
		const verdict = evaluateCapture(
			captureOf([row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING)), row(1, skillRead(HASTE))]),
			[source],
			ALL_SOURCES,
			LEAGUE
		);
		const core = groupOf(verdict, 'guide-a', 'synthetic', 'core');
		expect([core.outcome, core.need, core.confident]).toEqual(['pass', 2, 2]);
	});

	/**
	 * The clamp is keyed on the flips having ALTERED the group, not on the group
	 * having been revived: a group that was live all along still loses a filter
	 * when a buyer-contextual entry the mercenary lacks switches off, and `min: 2`
	 * over the one filter left is the same dead comp a revived group produces.
	 *
	 * Synthetic because no shipped ruleset can show it: the Kinetist `secondary`
	 * count group is the only live group carrying both a `min` and a
	 * buyer-contextual entry, and its `min` is 1, so clamping it is a no-op.
	 */
	function contextualGroupSource(): MercSource {
		return oneGroup({
			id: 'core',
			label: 'Core',
			type: 'count',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: KINETIC_BLAST_OF_CLUSTERING, name: textOf(KINETIC_BLAST_OF_CLUSTERING), enabledInSearch: true },
				{ id: HASTE, name: textOf(HASTE), enabledInSearch: true, buyerContextual: true }
			]
		});
	}

	it('clamps a live group’s minimum when a contextual entry switched off', () => {
		const verdict = evaluateCapture(
			captureOf([row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING))]),
			[contextualGroupSource()],
			ALL_SOURCES,
			LEAGUE
		);
		const core = derivedGroup(rulesetOf(verdict, 'guide-a', 'synthetic').derivedUrl, 0);
		expect(core.disabled).toBeUndefined();
		expect([core.value, core.filters]).toEqual([
			{ min: 1 },
			[{ id: KINETIC_BLAST_OF_CLUSTERING }, { id: HASTE, disabled: true }]
		]);
	});

	// The mirror, and the reason the clamp is conditional at all: this group's
	// `min: 2` already exceeds the one entry its guide switched on, and NOTHING
	// the flips do touches it — the Haste is a parked bonus the mercenary does not
	// have, so no entry moves. The guide's saved search asks 2 of 1, and the
	// derived link reproduces it rather than correcting it.
	it('leaves an untouched group’s minimum alone even where it already exceeds its filters', () => {
		const source = oneGroup({
			id: 'core',
			label: 'Core',
			type: 'count',
			enabledInSearch: true,
			min: 2,
			entries: [
				{ id: KINETIC_BLAST_OF_CLUSTERING, name: textOf(KINETIC_BLAST_OF_CLUSTERING), enabledInSearch: true },
				{ id: HASTE, name: textOf(HASTE), enabledInSearch: false }
			]
		});
		const verdict = evaluateCapture(
			captureOf([row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING))]),
			[source],
			ALL_SOURCES,
			LEAGUE
		);
		const core = derivedGroup(rulesetOf(verdict, 'guide-a', 'synthetic').derivedUrl, 0);
		expect([core.value, core.filters]).toEqual([
			{ min: 2 },
			[{ id: KINETIC_BLAST_OF_CLUSTERING }, { id: HASTE, disabled: true }]
		]);
	});

	// Asking a mercenary for nothing is not the same as a mercenary passing.
	it('reports a ruleset whose every group is parked as unknown, not a pass', () => {
		const source = oneGroup({
			id: 'core',
			label: 'Core',
			type: 'and',
			enabledInSearch: false,
			entries: [
				{ id: KINETIC_BLAST_OF_CLUSTERING, name: textOf(KINETIC_BLAST_OF_CLUSTERING), enabledInSearch: true }
			]
		});
		const verdict = evaluateCapture(
			captureOf([row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING))]),
			[source],
			ALL_SOURCES,
			LEAGUE
		);
		expect(rulesetOf(verdict, 'guide-a', 'synthetic').outcome).toBe('unknown');
	});
});

describe('ambiguous reads', () => {
	/** A cell whose family and tier are certain but whose name maps to two stat ids. */
	function ambiguousCell(slot: number): MercSupportRead {
		return {
			slot,
			rect: [372 + slot * 49, 593, 44, 44],
			family: 'Extra Targets',
			tier: 3,
			ids: GILDED_EXTRA_TARGETS,
			name: null,
			score: 0.88,
			state: 'ambiguous',
			candidates: ['Gilded Extra Targets (Tier 3)', 'Gilded Extra Targets (Tier 3)']
		};
	}

	it('clears a denial the ambiguous candidates do not include', () => {
		const capture = captureOf([
			row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING), [
				supportRead(0, RETURN),
				supportRead(1, GREATER_FORK),
				ambiguousCell(2)
			]),
			row(1, skillRead(GREATER_KINETIC_BLAST))
		]);
		expect(groupOf(verdictOf(capture), 'guide-b', 'guide-b-kinetist-mid', 'deny-supports').outcome).toBe(
			'pass'
		);
	});

	it('leaves its own candidates unknown', () => {
		const capture = captureOf([row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING), [ambiguousCell(0)])]);
		const verdict = evaluateCapture(
			capture,
			[
				{
					id: 'guide-a',
					label: 'Synthetic',
					guideUrl: null,
					rulesets: [
						{
							id: 'synthetic',
							label: 'Synthetic',
							archetype: 'kinetist',
							savedSearch: { league: 'Allflame', hash: 'synthetic' },
							status: 'securable',
							groups: [
								{
									id: 'deny',
									label: 'Denied',
									type: 'not',
									enabledInSearch: true,
									entries: [
										{
											id: GILDED_EXTRA_TARGETS[0],
											name: 'Gilded Extra Targets (Tier 3)',
											enabledInSearch: true
										}
									]
								}
							]
						}
					]
				}
			],
			ALL_SOURCES,
			LEAGUE
		);
		expect(groupOf(verdict, 'guide-a', 'synthetic', 'deny').outcome).toBe('unknown');
	});
});

describe('parked groups in the derived query', () => {
	// Guide B's Mid rung switches its whole aura group off, so an entry flip
	// alone would leave a fired Haste invisible to the trade site.
	it('switches a parked group on when a bonus inside it fired', () => {
		const capture = captureOf([
			row(0, skillRead(KINETIC_BLAST_OF_CLUSTERING), supportsOf([RETURN, GREATER_FORK, CHAIN, GREATER_EDWA, CRITICAL_DAMAGE])),
			row(1, skillRead(GREATER_KINETIC_BLAST)),
			row(2, skillRead(HASTE))
		]);
		const url = rulesetOf(verdictOf(capture), 'guide-b', 'guide-b-kinetist-mid').derivedUrl;
		const auras = derivedGroup(url, 5);
		expect(auras.disabled).toBeUndefined();
		expect(auras.filters).toEqual([{ id: HASTE }]);
	});

	it('leaves a parked group parked when nothing inside it fired', () => {
		const url = rulesetOf(verdictOf(kinetistCapture()), 'guide-b', 'guide-b-kinetist-mid').derivedUrl;
		expect(derivedGroup(url, 5).disabled).toBe(true);
	});
});
