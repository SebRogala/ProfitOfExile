import { describe, it, expect } from 'vitest';
import { evaluateCapture, type MercGroupResult, type MercVerdict } from './verdict';
import { MERC_SOURCES, SOURCE_IDS, type MercSource, type MercSourceId } from './rulesets';
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
const RETURN = 'mercenary.support_5293';
const GMP = 'mercenary.support_49419';
const COOLDOWN_RECOVERY = 'mercenary.support_48875';
const MULTISTRIKE = 'mercenary.support_62638';
const PIERCE = 'mercenary.support_56267';
const GREATER_FORK = 'mercenary.support_32052';
const CHAIN = 'mercenary.support_31052';
const GREATER_EDWA = 'mercenary.support_28416';
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

	it('speaks only for the rungs it calls best', () => {
		const reasons = sourceOf(verdictOf(kinetistCapture()), 'guide-b').reasons;
		expect(reasons.every((reason) => reason.startsWith('Kinetist (end):'))).toBe(true);
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
