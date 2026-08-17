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
	kinetistLadder,
	type MercFilterEntry,
	type MercFilterGroup,
	type MercRuleset
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
		expect(kinetistLadder().map(columnLabel)).toEqual(['minimum viable', 'mid', 'endgame', 'GG']);
	});
});

describe('ladderRows over the Kinetist ladder', () => {
	const rows = ladderRows(kinetistLadder());

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

	it('reports a hole in the quantifier for a group a rung has dropped', () => {
		const rows = ladderRows([
			rung('full', [group({ id: 'core', entries: [entry('a')] })]),
			rung('short', [])
		]);
		expect(groupRow(rows, 'core').quantifier).toBe('all of: · —');
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
