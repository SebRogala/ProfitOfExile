import { describe, it, expect } from 'vitest';
import {
	WINDOW_GONE_NOTE,
	captureRetired,
	guideLines,
	headerLine,
	overlayShowsVerdict,
	unreadIconCount,
	unreadNote
} from './overlay-view';
import { mercenarySliceDefault } from './capture';
import type {
	MercCapture,
	MercRow,
	MercSupportRead,
	MercStatus,
	MercenarySlice,
	ReadState
} from './capture';
import type { MercRulesetResult, MercSourceVerdict, MercVerdict } from './verdict';

function support(slot: number, state: ReadState): MercSupportRead {
	return {
		slot,
		rect: [372 + slot * 49, 593, 44, 44],
		family: state === 'unknown' ? null : 'Pierce',
		tier: state === 'unknown' ? null : 3,
		ids: state === 'unknown' ? [] : ['mercenary.support_56267'],
		name: state === 'unknown' ? null : 'Greater Pierce (Tier 3)',
		score: 0.9,
		state,
		candidates: []
	};
}

function row(index: number, states: ReadState[]): MercRow {
	return {
		index,
		skill: {
			raw: 'Ice Shot',
			ids: ['mercenary.skill_11495'],
			name: 'Ice Shot',
			score: 0.99,
			state: 'matched'
		},
		supports: states.map((state, slot) => support(slot, state))
	};
}

function capture(rows: MercRow[], header: Partial<MercCapture['header']> = {}): MercCapture {
	return {
		capturedAtMs: 1_755_000_000_000,
		live: true,
		scale: 1,
		screen: [2560, 1440],
		header: { name: 'Cai, the Lout', class: 'Shock Ambusher', level: 70, wager: 1028, ...header },
		rows
	};
}

function slice(status: MercStatus, taken: MercCapture | null): MercenarySlice {
	return { ...mercenarySliceDefault(), status, capture: taken };
}

function ruleset(id: string, label: string, tier: string | null): MercRulesetResult {
	return {
		id,
		label,
		tier,
		outcome: 'pass',
		groups: [],
		notInRules: [],
		reasons: [],
		floor: null,
		savedUrl: `/trade/search/Mirage/${id}`,
		derivedUrl: null
	};
}

function source(
	id: string,
	label: string,
	headline: MercSourceVerdict['headline'],
	best: string[] = [],
	rulesets: MercRulesetResult[] = []
): MercSourceVerdict {
	return { id: id as MercSourceVerdict['id'], label, headline, best, reasons: [], rulesets };
}

function verdict(sources: MercSourceVerdict[]): MercVerdict {
	return { sources };
}

describe('whether the strip draws at all', () => {
	it('draws while a recruit window is on screen', () => {
		expect(overlayShowsVerdict(slice('live', capture([row(0, ['matched'])])))).toBe(true);
	});

	// The retired-capture case: the loop drops back to `idle` and the strip has
	// to keep the last verdict, which is the whole "window gone" behaviour.
	it('keeps drawing the last verdict once the window is gone', () => {
		expect(overlayShowsVerdict(slice('idle', capture([row(0, ['matched'])])))).toBe(true);
	});

	it('draws nothing while the module has captured nothing yet', () => {
		expect(overlayShowsVerdict(slice('idle', null))).toBe(false);
	});

	// A module the user switched off keeps its last capture in the slice. Drawing
	// it would leave a panel over the game that nothing is updating.
	it('draws nothing when the module is off, capture or not', () => {
		expect(overlayShowsVerdict(slice('off', capture([row(0, ['matched'])])))).toBe(false);
	});

	it('draws nothing where capture is unavailable', () => {
		expect(overlayShowsVerdict(slice('unavailable', capture([row(0, ['matched'])])))).toBe(false);
	});
});

describe('marking a capture whose window is gone', () => {
	it('does not mark a capture the module is still reading', () => {
		expect(captureRetired(slice('live', capture([row(0, ['matched'])])))).toBe(false);
	});

	it('marks a capture the loop has retired', () => {
		const retired = { ...capture([row(0, ['matched'])]), live: false };
		expect(captureRetired(slice('idle', retired))).toBe(true);
	});

	// Status outranks the flag: nothing clears `live` on app exit, so a capture
	// restored beside a non-live status is stale whatever the flag says.
	it('marks a capture whose flag still says live but whose module does not', () => {
		expect(captureRetired(slice('scanning', capture([row(0, ['matched'])])))).toBe(true);
	});

	it('has nothing to mark when there is no capture', () => {
		expect(captureRetired(slice('idle', null))).toBe(false);
	});

	it('says which read the strip is showing', () => {
		expect(WINDOW_GONE_NOTE).toContain('last read');
	});
});

describe('the header line', () => {
	it('names the mercenary, the class and the level', () => {
		expect(headerLine(capture([]))).toBe('Cai, the Lout · Shock Ambusher · lvl 70');
	});

	// Dropping the field instead would print `Cai, the Lout · lvl 70`, which
	// reads as a mercenary with no class rather than as a field nobody read.
	it('says a field was not read rather than leaving it out', () => {
		expect(headerLine(capture([], { class: null }))).toBe(
			'Cai, the Lout · class not read · lvl 70'
		);
	});

	it('says the level was not read rather than printing a zero', () => {
		expect(headerLine(capture([], { level: null }))).toBe(
			'Cai, the Lout · Shock Ambusher · level not read'
		);
	});
});

describe('one line per enabled guide', () => {
	it('words a passing guide as WORTH', () => {
		const lines = guideLines(
			verdict([source('guide-a', 'Guide A', 'worth', ['manyshot'], [ruleset('manyshot', 'Manyshot', null)])])
		);
		expect(lines).toHaveLength(1);
		expect(lines[0].headline).toBe('WORTH');
		expect(lines[0].label).toBe('Guide A');
	});

	// The tier is the point of the ladder: WORTH alone does not say whether this
	// is the cheapest rung or the top one, which is what the player pays on.
	it('names the tier the mercenary passed at', () => {
		const lines = guideLines(
			verdict([
				source(
					'guide-b',
					'Guide B',
					'worth',
					['kinetist-mid'],
					[
						ruleset('kinetist-mv', 'Kinetist', 'minimum viable'),
						ruleset('kinetist-mid', 'Kinetist', 'mid')
					]
				)
			])
		);
		expect(lines[0].detail).toBe('Kinetist (mid)');
	});

	it('names every passing ruleset when more than one applies', () => {
		const lines = guideLines(
			verdict([
				source(
					'guide-a',
					'Guide A',
					'worth',
					['manyshot', 'combatant'],
					[ruleset('manyshot', 'Manyshot', null), ruleset('combatant', 'Combatant', null)]
				)
			])
		);
		expect(lines[0].detail).toBe('Manyshot, Combatant');
	});

	it('leaves a SKIP with nothing to name', () => {
		const lines = guideLines(
			verdict([source('guide-a', 'Guide A', 'skip', [], [ruleset('manyshot', 'Manyshot', null)])])
		);
		expect(lines[0].headline).toBe('SKIP');
		expect(lines[0].detail).toBe('');
	});

	it('words a guide that cannot decide as UNKNOWN rather than as a SKIP', () => {
		const lines = guideLines(verdict([source('guide-a', 'Guide A', 'unknown')]));
		expect(lines[0].headline).toBe('UNKNOWN');
	});

	// The strip has no room for a line that only ever says OFF, and the engine's
	// own `off` headline is the one place the enabled set is read.
	it('leaves out a guide the user switched off', () => {
		const lines = guideLines(
			verdict([source('guide-a', 'Guide A', 'off'), source('guide-b', 'Guide B', 'skip')])
		);
		expect(lines.map((line) => line.id)).toEqual(['guide-b']);
	});

	it('has nothing to say before anything is captured', () => {
		expect(guideLines(null)).toEqual([]);
	});
});

describe('counting the icons nobody read', () => {
	it('counts nothing when every cell was matched', () => {
		expect(unreadIconCount(capture([row(0, ['matched', 'matched'])]))).toBe(0);
	});

	it('counts a cell the template store could not name', () => {
		expect(unreadIconCount(capture([row(0, ['matched', 'unknown'])]))).toBe(1);
	});

	// The three unconfident states are all "not read" to the verdict engine, so
	// counting only `unknown` would hide the cells a hover would settle.
	it('counts a low-confidence and an ambiguous cell too', () => {
		expect(unreadIconCount(capture([row(0, ['low_confidence', 'ambiguous', 'matched'])]))).toBe(2);
	});

	// A hover confirmation is as good as a match — the point of hovering is that
	// the count goes down.
	it('does not count a cell the user confirmed by hovering', () => {
		expect(unreadIconCount(capture([row(0, ['confirmed'])]))).toBe(0);
	});

	it('counts across every row of the capture', () => {
		expect(unreadIconCount(capture([row(0, ['unknown']), row(1, ['unknown', 'matched'])]))).toBe(2);
	});
});

describe('the unread line', () => {
	it('says nothing when there is nothing to hover', () => {
		expect(unreadNote(capture([row(0, ['matched'])]))).toBeNull();
	});

	it('asks for a hover and says how many cells need one', () => {
		expect(unreadNote(capture([row(0, ['unknown', 'unknown'])]))).toBe(
			'2 icons unread — hover to confirm'
		);
	});

	it('counts one icon in the singular', () => {
		expect(unreadNote(capture([row(0, ['unknown'])]))).toBe('1 icon unread — hover to confirm');
	});
});
