import { describe, it, expect } from 'vitest';
import {
	capturedAt,
	describeDebugResult,
	indexGroups,
	indexPositions,
	notInRulesNames,
	parseLearnedTemplate,
	positionKey,
	positionOutcomeLabel,
	SCAN_NOW_TITLE,
	skillText,
	skillTitle,
	STATUS_LABEL,
	STATUS_TONE,
	supportText,
	supportTitle
} from './capture-view';
import type { MercCapture, MercSkillRead, MercSupportRead, ReadState } from './capture';
import type { MercGroupResult, MercPosition, MercRulesetResult } from './verdict';

/**
 * Reads are hand-built here for the same reason `verdict.test.ts` builds its
 * captures by hand — the Rust reader that produces the real ones is not on this
 * branch yet. What these tests pin is the wording contract the task states: a
 * read that is not confident says so in the cell, and the title carries the raw
 * text and the score that produced it.
 */
function skill(overrides: Partial<MercSkillRead> = {}): MercSkillRead {
	return {
		raw: 'Ice Shot',
		ids: ['mercenary.skill_11495'],
		name: 'Ice Shot',
		score: 0.98,
		state: 'matched',
		...overrides
	};
}

function support(overrides: Partial<MercSupportRead> = {}): MercSupportRead {
	return {
		slot: 0,
		rect: [372, 593, 44, 44],
		family: 'Return',
		tier: 3,
		ids: ['mercenary.support_5293'],
		name: 'Return (Tier 3)',
		score: 0.91,
		state: 'matched',
		candidates: [],
		...overrides
	};
}

function capture(rows: MercCapture['rows']): MercCapture {
	return {
		capturedAtMs: 1_700_000_000_000,
		live: true,
		scale: 1,
		screen: [2560, 1440],
		header: { name: 'Cai, the Lout', class: 'Shock Ambusher', level: 70, wager: 1028 },
		rows
	};
}

const ALL_STATES: ReadState[] = ['matched', 'confirmed', 'low_confidence', 'unknown', 'ambiguous'];

describe('skillText', () => {
	it('prints the matched name on its own', () => {
		expect(skillText(skill())).toBe('Ice Shot');
	});

	it('prints a hover-confirmed name on its own', () => {
		expect(skillText(skill({ state: 'confirmed' }))).toBe('Ice Shot');
	});

	it('flags a low-confidence read with its score and the hover hint', () => {
		expect(skillText(skill({ state: 'low_confidence', score: 0.87 }))).toBe(
			'Ice Shot — low confidence 0.87, hover to confirm'
		);
	});

	it('keeps the raw OCR text of a read that matched nothing', () => {
		// The garbled text is what makes a wrong verdict attributable to the
		// capture rather than to the rules.
		expect(skillText(skill({ state: 'unknown', name: null, raw: 'K1net1c 8last' }))).toBe(
			'K1net1c 8last — not read, hover to confirm'
		);
	});

	it('says unknown when a failed read has no text either', () => {
		expect(skillText(skill({ state: 'unknown', name: null, raw: '' }))).toBe(
			'unknown — not read, hover to confirm'
		);
	});

	it('never renders a state as an empty or bare cell', () => {
		for (const state of ALL_STATES) {
			const text = skillText(skill({ state }));
			expect(text.length, `state ${state}`).toBeGreaterThan(0);
		}
	});
});

describe('skillTitle', () => {
	it('carries the raw text, the score and the state in words', () => {
		expect(skillTitle(skill({ raw: 'Vaal lce Shot', score: 0.885, state: 'low_confidence' }))).toBe(
			'OCR read "Vaal lce Shot" · score 0.89 · low confidence — hover to confirm'
		);
	});
});

describe('supportText', () => {
	it('prints the matched support name', () => {
		expect(supportText(support())).toBe('Return (Tier 3)');
	});

	it('falls back to the icon family and its tier badge when no name resolved', () => {
		expect(supportText(support({ name: null }))).toBe('Return (Tier 3)');
	});

	it('drops the tier from the fallback when the badge was not read', () => {
		expect(supportText(support({ name: null, tier: null }))).toBe('Return');
	});

	it('names both candidates of an ambiguous cell instead of picking one', () => {
		expect(
			supportText(
				support({
					state: 'ambiguous',
					name: null,
					candidates: ['Greater Pierce (Tier 3)', 'Gilded Pierce (Tier 3)']
				})
			)
		).toBe('ambiguous: Greater Pierce (Tier 3) | Gilded Pierce (Tier 3)');
	});

	it('still asks for a hover when an ambiguous cell listed no candidates', () => {
		expect(supportText(support({ state: 'ambiguous', name: null, candidates: [] }))).toBe(
			'ambiguous — hover to confirm'
		);
	});

	it('renders an unread cell as unknown rather than as an empty slot', () => {
		// An empty cell and a cell nobody could read are different facts about the
		// mercenary; printing nothing would merge them.
		expect(supportText(support({ state: 'unknown', name: null, family: null, tier: null }))).toBe(
			'unknown — hover to confirm'
		);
	});

	it('shows the score of a low-confidence cell next to its guess', () => {
		expect(supportText(support({ state: 'low_confidence', score: 0.73 }))).toBe(
			'Return (Tier 3) — low confidence 0.73, hover to confirm'
		);
	});

	it('never renders a state as an empty or bare cell', () => {
		for (const state of ALL_STATES) {
			const text = supportText(support({ state }));
			expect(text.length, `state ${state}`).toBeGreaterThan(0);
		}
	});
});

describe('supportTitle', () => {
	it('carries the icon family, the tier badge, the score and the state', () => {
		expect(supportTitle(support({ score: 0.874 }))).toBe(
			'icon Return · tier 3 · score 0.87 · matched'
		);
	});

	it('says which of the two reads failed when the cell was not identified', () => {
		expect(supportTitle(support({ family: null, tier: null, score: 0.4, state: 'unknown' }))).toBe(
			'no icon match · no tier badge · score 0.40 · not read — hover to confirm'
		);
	});

	it('appends the candidates of an ambiguous cell', () => {
		expect(
			supportTitle(
				support({ state: 'ambiguous', score: 0.9, candidates: ['Greater Pierce', 'Gilded Pierce'] })
			)
		).toBe(
			'icon Return · tier 3 · score 0.90 · ambiguous — hover to confirm · candidates: Greater Pierce | Gilded Pierce'
		);
	});
});

describe('capturedAt', () => {
	const shot = capture([
		{ index: 0, skill: skill(), supports: [support({ slot: 0 }), support({ slot: 1 })] },
		{ index: 1, skill: skill({ name: 'Haste', raw: 'Haste' }), supports: [] }
	]);

	it('names the skill of a site with no slot, one-based as the window reads', () => {
		expect(capturedAt(shot, { rowIndex: 1, slot: null })).toBe('row 2 · Haste');
	});

	it('names the support cell of a site with a slot', () => {
		expect(capturedAt(shot, { rowIndex: 0, slot: 1 })).toBe('row 1 slot 2 · Return (Tier 3)');
	});

	it('reports a position the entry was never seen at as not seen', () => {
		expect(capturedAt(shot, null)).toBe('not seen');
	});

	it('reports the bare position when the capture no longer has that row', () => {
		// The verdict was computed against a capture that has since been replaced;
		// saying "not seen" there would read as an absent stat.
		expect(capturedAt(shot, { rowIndex: 4, slot: null })).toBe('row 5');
	});

	it('reports the bare position when the row no longer has that slot', () => {
		expect(capturedAt(shot, { rowIndex: 1, slot: 2 })).toBe('row 2 slot 3');
	});
});

/** A position result carrying only what the index keys on. */
function position(entryId: string, outcome: MercPosition['outcome']): MercPosition {
	return {
		groupId: 'core',
		groupLabel: 'core',
		groupType: 'mercenary',
		entryId,
		entryName: entryId,
		kind: 'required',
		buyerContextual: false,
		counted: true,
		presence: 'present',
		outcome,
		site: null
	};
}

function groupResult(id: string, positions: MercPosition[], outcome: MercGroupResult['outcome']) {
	return {
		id,
		label: id,
		type: 'mercenary' as const,
		applied: true,
		min: null,
		need: 1,
		confident: 1,
		outcome,
		rowIndex: null,
		positions
	};
}

function rulesetResult(id: string, groups: MercGroupResult[]): MercRulesetResult {
	return {
		id,
		label: id,
		tier: null,
		outcome: 'pass',
		groups,
		notInRules: [],
		reasons: [],
		floor: null,
		savedUrl: 'https://www.pathofexile.com/trade/search/Allflame/abc',
		derivedUrl: null
	};
}

describe('indexPositions', () => {
	it('keeps two rungs that share a group and an entry id apart', () => {
		// Guide B's four rungs reuse one group id sequence — keying on less than
		// ruleset + group + entry would show one rung's outcome in another's row.
		const index = indexPositions([
			rulesetResult('mv', [groupResult('core', [position('gmp', 'bonus-fired')], 'pass')]),
			rulesetResult('gg', [groupResult('core', [position('gmp', 'pass')], 'pass')])
		]);
		expect(index.get(positionKey('mv', 'core', 'gmp'))?.outcome).toBe('bonus-fired');
		expect(index.get(positionKey('gg', 'core', 'gmp'))?.outcome).toBe('pass');
	});

	it('keeps sibling groups that repeat an entry id apart', () => {
		const index = indexPositions([
			rulesetResult('mv', [
				groupResult('core', [position('gmp', 'bonus-fired')], 'pass'),
				groupResult('damage', [position('gmp', 'absent')], 'fail')
			])
		]);
		expect(index.get(positionKey('mv', 'core', 'gmp'))?.outcome).toBe('bonus-fired');
		expect(index.get(positionKey('mv', 'damage', 'gmp'))?.outcome).toBe('absent');
	});

	it('has no entry for a position that is not in the results', () => {
		const index = indexPositions([rulesetResult('mv', [])]);
		expect(index.get(positionKey('mv', 'core', 'gmp'))).toBeUndefined();
	});
});

describe('indexGroups', () => {
	it('keeps the same group id in two rulesets apart', () => {
		const index = indexGroups([
			rulesetResult('mv', [groupResult('core', [], 'pass')]),
			rulesetResult('gg', [groupResult('core', [], 'fail')])
		]);
		expect(index.get('mv/core')?.outcome).toBe('pass');
		expect(index.get('gg/core')?.outcome).toBe('fail');
	});
});

describe('positionOutcomeLabel', () => {
	it('reads a passing required entry as present', () => {
		expect(positionOutcomeLabel('required', 'pass')).toBe('present');
	});

	it('reads a passing forbidden entry as absent, not as present', () => {
		// A denial passes by the stat being MISSING. Wording it "present" would
		// tell the reader the mercenary carries the exact stat the search rejects.
		expect(positionOutcomeLabel('forbidden', 'pass')).toBe('absent — clear');
	});

	it('reads a failing forbidden entry as the forbidden stat being present', () => {
		expect(positionOutcomeLabel('forbidden', 'fail')).toBe('present — forbidden');
	});

	it('words an unread denial as one it cannot rule out', () => {
		expect(positionOutcomeLabel('forbidden', 'unknown')).toBe(
			'cannot rule out — hover to confirm'
		);
	});

	it('words an unread requirement as unknown', () => {
		expect(positionOutcomeLabel('required', 'unknown')).toBe('unknown — hover to confirm');
	});

	it('words a fired bonus the same for either kind of rule', () => {
		expect(positionOutcomeLabel('bonus', 'bonus-fired')).toBe('bonus fired');
	});
});

describe('parseLearnedTemplate', () => {
	it('splits the store key into the family and tier the forget command takes', () => {
		expect(parseLearnedTemplate('Return--3')).toEqual({
			family: 'Return',
			tier: 3,
			label: 'Return (Tier 3)'
		});
	});

	it('splits on the last separator so a family containing one survives', () => {
		expect(parseLearnedTemplate('Ele Dmg--with--2')).toEqual({
			family: 'Ele Dmg--with',
			tier: 2,
			label: 'Ele Dmg--with (Tier 2)'
		});
	});

	it('passes an entry with no tier suffix through whole rather than guessing one', () => {
		expect(parseLearnedTemplate('Return')).toEqual({
			family: 'Return',
			tier: null,
			label: 'Return'
		});
	});

	it('treats a non-numeric suffix as part of the family, not as a tier', () => {
		expect(parseLearnedTemplate('Return--x')).toEqual({
			family: 'Return--x',
			tier: null,
			label: 'Return--x'
		});
	});
});

describe('describeDebugResult', () => {
	it('shows a returned dump path as itself', () => {
		expect(describeDebugResult('C:\\merc-debug\\1700000000')).toBe('C:\\merc-debug\\1700000000');
	});

	it('renders a report object whole, including fields it knows nothing about', () => {
		expect(describeDebugResult({ dumpDir: '/tmp/merc', rows: 6 })).toBe(
			'{\n  "dumpDir": "/tmp/merc",\n  "rows": 6\n}'
		);
	});

	it('says so when the command returned nothing at all', () => {
		expect(describeDebugResult(undefined)).toBe('command returned no report');
	});

	it('treats an empty string as no report rather than as an empty dump', () => {
		expect(describeDebugResult('')).toBe('command returned no report');
	});
});

describe('notInRulesNames', () => {
	/** A result whose only interesting part is what it says was not in the rules. */
	function withNotInRules(id: string, names: string[]): MercRulesetResult {
		return {
			...rulesetResult(id, []),
			notInRules: names.map((name, i) => ({
				ids: [`mercenary.skill_${i}`],
				name,
				site: { rowIndex: i, slot: null }
			}))
		};
	}

	it('reports each stat once across rulesets that all name it', () => {
		// The four ladder rungs share one entry skeleton, so their lists repeat —
		// the page must not print the same stat four times.
		expect(
			notInRulesNames([
				withNotInRules('mv', ['Greater Kinetic Blast', 'Haste']),
				withNotInRules('mid', ['Greater Kinetic Blast'])
			])
		).toEqual(['Greater Kinetic Blast', 'Haste']);
	});

	it('keeps a stat only one ruleset reported', () => {
		expect(
			notInRulesNames([withNotInRules('mv', []), withNotInRules('gg', ['Barrage'])])
		).toEqual(['Barrage']);
	});

	it('reports nothing when every ruleset accounted for the capture', () => {
		expect(notInRulesNames([withNotInRules('mv', [])])).toEqual([]);
	});
});

describe('the module status wording', () => {
	/**
	 * The states differ in how much work the module is doing, which is the whole
	 * point of trigger-only capture (POE-198): `idle` runs no OCR, `scanning` is
	 * an armed burst, `live` has the window. Two states sharing a label would
	 * leave the page unable to say which — and "waiting" and "scanning" are the
	 * pair most likely to be copy-pasted into one.
	 */
	it('gives every status its own words', () => {
		const labels = Object.values(STATUS_LABEL);

		expect(new Set(labels).size).toBe(labels.length);
		expect(labels.every((label) => label.length > 0)).toBe(true);
	});

	it('does not word an idle module as one that is looking', () => {
		// `idle` means the loop is asleep. Calling it "watching for a recruit
		// window" (which it was before POE-198) would promise the OCR that was
		// removed, and hide a trigger that never fires.
		expect(STATUS_LABEL.idle).not.toMatch(/watch|scan|look/i);
	});

	/** The button cannot do anything until the game is the foreground window —
	 *  a tooltip that promised an immediate scan would read as a broken button
	 *  to anyone who clicked it and watched nothing happen. */
	it('tells the reader that Scan now waits for the game to be in front', () => {
		expect(SCAN_NOW_TITLE).toMatch(/alt-tab/i);
	});

	it('keeps the captured-window colour for a captured window', () => {
		expect(STATUS_TONE.live).toBe('pass');
		expect(STATUS_TONE.scanning).not.toBe('pass');
	});
});
