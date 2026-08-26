import { describe, it, expect } from 'vitest';
import {
	columnLabel,
	kindTitle,
	ladderRows,
	quantifier,
	rungOutcomes,
	sharedValue,
	type LadderEntryRow,
	type LadderGroupRow,
	type LadderRow
} from './ladder-view';
import {
	MERC_SOURCES,
	ladders,
	type MercFilterEntry,
	type MercFilterGroup,
	type MercRuleset,
	type MercSource
} from './rulesets';
import type { MercRulesetResult } from './verdict';

function entry(id: string, enabledInSearch = true): MercFilterEntry {
	return { id, name: id, enabledInSearch };
}

function group(overrides: Partial<MercFilterGroup> & { id: string }): MercFilterGroup {
	return {
		label: overrides.id,
		type: 'mercenary',
		enabledInSearch: true,
		entries: [],
		...overrides
	};
}

/** A throwaway rung — only the group skeleton matters to `ladderRows`. */
function rung(id: string, groups: MercFilterGroup[]): MercRuleset {
	return {
		id,
		label: 'Kinetist',
		archetype: 'kinetist',
		tier: 'mv',
		savedSearch: { league: 'Allflame', hash: id },
		status: 'securable',
		groups
	};
}

const GUIDE_B = MERC_SOURCES.find((s) => s.id === 'guide-b') as MercSource;

/** The rungs of one guide-b ladder, cheapest first. */
function ladderNamed(key: string): MercRuleset[] {
	const found = ladders(GUIDE_B).find((rungs) => rungs[0].ladder === key);
	if (!found) throw new Error(`guide-b declares no ladder ${key}`);
	return found;
}

function entryRow(rows: LadderRow[], id: string): LadderEntryRow {
	const row = rows.find((r) => r.kind === 'entry' && r.id === id);
	if (!row || row.kind !== 'entry') throw new Error(`no entry row "${id}" in the matrix`);
	return row;
}

function groupRow(rows: LadderRow[], id: string): LadderGroupRow {
	const row = rows.find((r) => r.kind === 'group' && r.id === id);
	if (!row || row.kind !== 'group') throw new Error(`no group row "${id}" in the matrix`);
	return row;
}

describe('quantifier', () => {
	it('names the minimum when the group sets one', () => {
		expect(quantifier(group({ id: 'damage', min: 2, entries: [entry('a'), entry('b')] }))).toBe(
			'at least 2 of:'
		);
	});

	it('asks for every enabled entry when the group sets no minimum', () => {
		expect(quantifier(group({ id: 'core', entries: [entry('a'), entry('b')] }))).toBe('all of:');
	});

	// Type-first, same rule as entryKind: a parked denial is still a denial, so it
	// must not fall through to the "when enabled:" wording a parked requirement gets.
	it('still reads as a denial when a `not` group is switched off', () => {
		expect(
			quantifier(group({ id: 'deny', type: 'not', enabledInSearch: false, entries: [entry('a')] }))
		).toBe('none of:');
	});

	it('does not claim a requirement when a positive group is switched off', () => {
		expect(
			quantifier(group({ id: 'auras', type: 'and', enabledInSearch: false, entries: [entry('a')] }))
		).toBe('when enabled:');
	});

	it('reads as vocabulary when the group has no entry switched on', () => {
		expect(quantifier(group({ id: 'auras', type: 'and', entries: [entry('a', false)] }))).toBe(
			'bonus vocabulary — none enabled'
		);
	});
});

describe('kindTitle', () => {
	// The glyph and its colour are the only other carriers; if this wording ever
	// read like a requirement, a switched-off row would be sold as a must-have.
	it('names a bonus as switched off rather than as a requirement', () => {
		expect(kindTitle('bonus')).toBe('bonus — off in this search');
	});

	it('calls a `not` group entry denied', () => {
		expect(kindTitle('forbidden')).toBe('denied');
	});
});

describe('sharedValue', () => {
	it('returns the value every element carries', () => {
		expect(sharedValue(['none of:', 'none of:', 'none of:'])).toBe('none of:');
	});

	it('returns null when one element differs', () => {
		expect(sharedValue(['all of:', 'all of:', 'when enabled:'])).toBeNull();
	});

	it('returns null for no elements at all', () => {
		expect(sharedValue([])).toBeNull();
	});
});

describe('columnLabel', () => {
	it('heads a rung column with its tier, not its (shared) ruleset label', () => {
		expect(ladderNamed('kinetist').map(columnLabel)).toEqual(['minimum viable', 'mid', 'endgame', 'GG']);
	});
});

describe('ladderRows over the Kinetist ladder', () => {
	const rows = ladderRows(ladderNamed('kinetist'));

	it('opens with the first rung group followed by its own entries', () => {
		expect(rows.slice(0, 4).map((r) => r.id)).toEqual([
			'deny',
			'deny/mercenary.skill_32089',
			'deny/mercenary.skill_12583',
			'deny/mercenary.skill_26705'
		]);
	});

	// The highlight is the page's whole claim: these are the rows a reader
	// comparing rungs is hunting for. Derived from the rungs, never declared —
	// Haste's entry row varies because the Mid rung parks the group above it.
	it('marks exactly the rows whose state moves between rungs', () => {
		expect(rows.filter((r) => r.varies).map((r) => r.id)).toEqual([
			'core/mercenary.support_49419',
			'secondary/mercenary.skill_1356',
			'behavior/mercenary.support_56267',
			'behavior/mercenary.support_27970',
			'deny-supports',
			'auras',
			'auras/mercenary.skill_52155',
			'damage'
		]);
	});

	it('turns an entry switched off from Mid up into a bonus in those columns only', () => {
		expect(entryRow(rows, 'behavior/mercenary.support_56267').cells).toEqual([
			'required',
			'bonus',
			'bonus',
			'bonus'
		]);
	});

	it('keeps a parked denial forbidden in the column that parked it', () => {
		expect(entryRow(rows, 'deny-supports/mercenary.support_27970').cells).toEqual([
			'forbidden',
			'forbidden',
			'forbidden',
			'forbidden'
		]);
	});

	it('names the rung that switched a group off', () => {
		expect(groupRow(rows, 'deny-supports').offIn).toEqual(['minimum viable']);
	});

	it('leaves offIn empty for a group every rung leaves on', () => {
		expect(groupRow(rows, 'core').offIn).toEqual([]);
	});

	it('collapses a quantifier every rung agrees on to one sentence', () => {
		expect(groupRow(rows, 'deny').quantifier).toBe('none of:');
	});

	it('spreads only the number when the rungs differ by minimum alone', () => {
		expect(groupRow(rows, 'damage').quantifier).toBe('at least 2 · 2 · 3 · 2 of:');
	});

	it('spells out each rung when the disagreement is not just the number', () => {
		// The mid rung PARKS the group; its wording is excluded from the line —
		// the "off in mid" badge already carries that fact, and the group reads
		// "all of:" everywhere it is live.
		expect(groupRow(rows, 'auras').quantifier).toBe('all of:');
	});

	it('falls back to the parked wording when every rung parks the group', () => {
		const parked = group({ id: 'auras', type: 'and', enabledInSearch: false, entries: [entry('a')] });
		const rows = ladderRows([rung('one', [parked]), rung('two', [parked])]);
		expect(groupRow(rows, 'auras').quantifier).toBe('when enabled:');
	});
});

describe('ladderRows over rungs that do not share a skeleton', () => {
	it('reports a hole rather than a state for an entry a rung has dropped', () => {
		const rows = ladderRows([
			rung('full', [group({ id: 'core', entries: [entry('a'), entry('b')] })]),
			rung('short', [group({ id: 'core', entries: [entry('a')] })])
		]);
		expect(entryRow(rows, 'core/b').cells).toEqual(['required', 'absent']);
	});

	// The dropped rung's state is the — cell under it; letting it vote in the
	// wording too printed the hole twice and buried the sentence the live rungs
	// agree on.
	it('leaves a rung that dropped the group out of the quantifier line', () => {
		const rows = ladderRows([
			rung('full', [group({ id: 'core', entries: [entry('a')] })]),
			rung('short', [])
		]);
		expect(groupRow(rows, 'core').quantifier).toBe('all of:');
	});

	it('has no rows at all when there are no rungs', () => {
		expect(ladderRows([])).toEqual([]);
	});

	// The module's own contract comment: lookups go by id, "so a rung that ever
	// stops matching the skeleton reports `absent` … instead of shifting every
	// state one row up". Tail-truncation cannot tell id lookup from positional
	// lookup (both yield undefined past the end) — only a REORDERED rung can.
	it('matches states by id, not position, when a rung reorders the skeleton', () => {
		const rows = ladderRows([
			rung('ordered', [
				group({ id: 'core', entries: [entry('a'), entry('b', false)] }),
				group({ id: 'deny', type: 'not', entries: [entry('x')] })
			]),
			rung('reordered', [
				group({ id: 'deny', type: 'not', entries: [entry('x')] }),
				group({ id: 'core', entries: [entry('b', false), entry('a')] })
			])
		]);
		expect(entryRow(rows, 'core/a').cells).toEqual(['required', 'required']);
		expect(entryRow(rows, 'core/b').cells).toEqual(['bonus', 'bonus']);
		expect(entryRow(rows, 'deny/x').cells).toEqual(['forbidden', 'forbidden']);
	});

	// A group parked in EVERY rung carries no delta — nothing moves between
	// rungs, so it must not claim the varies highlight the reader hunts by.
	it('does not mark a group parked in every rung as varying', () => {
		const parked = group({ id: 'deny', type: 'not', enabledInSearch: false, entries: [entry('x')] });
		const rows = ladderRows([rung('one', [parked]), rung('two', [parked])]);
		expect(groupRow(rows, 'deny').offIn).toEqual(['minimum viable', 'minimum viable']);
		expect(groupRow(rows, 'deny').varies).toBe(false);
	});
});

describe('ladderRows entry ids', () => {
	it('carries the group and entry id the verdict lookup needs, not just the joined key', () => {
		// The page looks a rung's position up by (rulesetId, groupId, entryId); if
		// it had to split the row key apart, an id containing the separator would
		// silently address the wrong position.
		const rows = ladderRows([
			rung('mv', [group({ id: 'core', entries: [entry('mercenary.support_49419')] })])
		]);
		const row = entryRow(rows, 'core/mercenary.support_49419');
		expect(row.groupId).toBe('core');
		expect(row.entryId).toBe('mercenary.support_49419');
	});
});

describe('rungOutcomes', () => {
	/** A verdict result carrying only what the header row reads. */
	function result(id: string, outcome: MercRulesetResult['outcome']): MercRulesetResult {
		return {
			id,
			label: id,
			ladder: null,
			tier: null,
			outcome,
			groups: [],
			notInRules: [],
			reasons: [],
			floor: null,
			savedUrl: `https://www.pathofexile.com/trade/search/Allflame/${id}`,
			derivedUrl: null
		};
	}

	it('puts each rung’s outcome in that rung’s column', () => {
		const outcomes = rungOutcomes(
			[rung('mv', []), rung('mid', []), rung('gg', [])],
			// Deliberately out of column order: the matching is by id, not position.
			[result('gg', 'fail'), result('mv', 'pass'), result('mid', 'unknown')]
		);
		expect(outcomes).toEqual(['pass', 'unknown', 'fail']);
	});

	it('leaves a rung with no result blank instead of borrowing a neighbour’s', () => {
		// The state before any capture, and whenever the source is switched off.
		expect(rungOutcomes([rung('mv', []), rung('mid', [])], [result('mv', 'pass')])).toEqual([
			'pass',
			null
		]);
	});

	it('reports no columns at all when there are no rungs', () => {
		expect(rungOutcomes([], [result('mv', 'pass')])).toEqual([]);
	});
});

/**
 * The Manyshot ladder is the drift case in production data: its cheapest rung has
 * no aura group, its GG rung has no "carries Vaal Ice Shot" group and merges the
 * projectile and damage vocabularies into one. Every hole below must render as a
 * hole — a matrix that borrowed the neighbouring column's state here would tell a
 * seller a rung asks for something it does not ask for.
 */
describe('ladderRows over the Manyshot ladder', () => {
	const rows = ladderRows(ladderNamed('manyshot'));

	it('keeps a group only the higher rungs declare, rather than dropping the row', () => {
		expect(rows.filter((r) => r.kind === 'group').map((r) => r.id)).toEqual([
			'has-vaal',
			'deny',
			'vaal-damage',
			'vaal-return',
			'core',
			'projectiles',
			'damage',
			'auras'
		]);
	});

	it('holes the aura row in the rung that has no aura group', () => {
		expect(entryRow(rows, 'auras/mercenary.skill_2792').cells).toEqual([
			'absent',
			'required',
			'required',
			'required'
		]);
	});

	it('holes an aura option in every rung but the one that offers it', () => {
		expect(entryRow(rows, 'auras/mercenary.skill_10557').cells).toEqual([
			'absent',
			'absent',
			'absent',
			'required'
		]);
	});

	it('holes the Vaal Ice Shot gate in the GG rung, which does not carry that group', () => {
		expect(entryRow(rows, 'has-vaal/mercenary.skill_16381').cells).toEqual([
			'required',
			'required',
			'required',
			'absent'
		]);
	});

	it('holes an entry the GG rung leaves out of a group it does declare', () => {
		// Return is parked inside the Vaal damage group on the other three rungs;
		// GG simply has no such filter, which is not the same as parking it.
		expect(entryRow(rows, 'vaal-damage/mercenary.support_5293').cells).toEqual([
			'bonus',
			'bonus',
			'bonus',
			'absent'
		]);
	});

	it('shows the merged GG damage entries as holes under the projectiles group', () => {
		expect(entryRow(rows, 'projectiles/mercenary.support_44886').cells).toEqual([
			'absent',
			'absent',
			'absent',
			'bonus'
		]);
	});

	// The GG rung merged the damage vocabulary into `projectiles` and renamed the
	// group to say so. Heading the merged rows with the first declarer's label
	// alone would tell a seller the GG rung asks only for projectile links.
	it('heads a slot the rungs label differently with every distinct label', () => {
		expect(groupRow(rows, 'projectiles').label).toBe(
			'Ice Shot + projectile links · Ice Shot + projectile and damage links'
		);
	});

	it('collapses the quantifier of a group the GG rung does not carry', () => {
		expect(groupRow(rows, 'has-vaal').quantifier).toBe('all of:');
	});

	it('collapses the quantifier of a group the cheapest rung does not carry', () => {
		expect(groupRow(rows, 'auras').quantifier).toBe('at least 1 of:');
	});

	it('holes the whole damage group in the GG rung that merged it away', () => {
		expect(entryRow(rows, 'damage/mercenary.support_38571').cells).toEqual([
			'bonus',
			'bonus',
			'required',
			'absent'
		]);
	});
});
