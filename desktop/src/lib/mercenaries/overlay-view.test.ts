import { describe, it, expect } from 'vitest';
import {
	IDLE_LINE,
	NO_CELLS_NOTE,
	NO_GUIDES_NOTE,
	SCANNING_LINE,
	SKIP_LINE,
	STALE_VERDICT_SUFFIX,
	UNKNOWN_LINE,
	WINDOW_GONE_NOTE,
	captureRetired,
	guidesLine,
	headerLine,
	liveRowGlyphs,
	overlayShowsVerdict,
	overlayVisible,
	rowGlyphs,
	statusLine,
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

function slice(
	status: MercStatus,
	taken: MercCapture | null,
	burstSpeaker: string | null = null
): MercenarySlice {
	return { ...mercenarySliceDefault(), status, capture: taken, burstSpeaker };
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
	// The smoke complaint: an idle module drew nothing, so "waiting" and "the
	// window never got built" were the same pixels and the only way to tell was
	// to alt-tab.
	it('draws while the module is running with nothing captured yet', () => {
		expect(overlayVisible(slice('idle', null))).toBe(true);
	});

	it('draws while a burst is looking', () => {
		expect(overlayVisible(slice('scanning', null))).toBe(true);
	});

	// The OCR paused; the window and the verdict did not go anywhere. Hiding
	// the strip here would take the finished read off screen at the moment the
	// player is acting on it.
	it('draws while a fully-read window is still on screen', () => {
		expect(overlayVisible(slice('done', capture([row(0, ['matched'])])))).toBe(true);
	});

	// A module the user switched off keeps its last capture in the slice. Drawing
	// it would leave a panel over the game that nothing is updating.
	it('draws nothing when the module is off, capture or not', () => {
		expect(overlayVisible(slice('off', capture([row(0, ['matched'])])))).toBe(false);
	});

	it('draws nothing where capture is unavailable', () => {
		expect(overlayVisible(slice('unavailable', capture([row(0, ['matched'])])))).toBe(false);
	});
});

describe('whether the verdict block draws', () => {
	it('draws while a recruit window is on screen', () => {
		expect(overlayShowsVerdict(slice('live', capture([row(0, ['matched'])])))).toBe(true);
	});

	// The retired-capture case: the loop drops back to `idle` and the strip has
	// to keep the last verdict, which is the whole "window gone" behaviour.
	it('keeps drawing the last verdict once the window is gone', () => {
		expect(overlayShowsVerdict(slice('idle', capture([row(0, ['matched'])])))).toBe(true);
	});

	// The status line still draws here — this gate is only about the block that
	// makes claims ABOUT a capture.
	it('draws no verdict while the module has captured nothing yet', () => {
		expect(overlayShowsVerdict(slice('idle', null))).toBe(false);
	});

	it('draws no verdict where capture is unavailable', () => {
		expect(overlayShowsVerdict(slice('unavailable', capture([row(0, ['matched'])])))).toBe(false);
	});
});

describe('the status line', () => {
	it('says the module is waiting', () => {
		expect(statusLine(slice('idle', null))).toBe(IDLE_LINE);
	});

	// This window can never be clicked, so the line has to say WHERE the button
	// is rather than looking like one.
	it('points at the manual scan on the page', () => {
		expect(IDLE_LINE).toContain('Scan now');
	});

	it('says a burst is looking', () => {
		expect(statusLine(slice('scanning', null))).toBe(SCANNING_LINE);
	});

	// The 2026-08-25 report: the strip sat on "waiting" through the voice line
	// and then jumped to reading, which looks like a missed trigger. Naming the
	// speaker is what tells the player THEIR mercenary was the one heard.
	it('names the mercenary it heard while it scans', () => {
		expect(statusLine(slice('scanning', null, 'Fennik, of Unshakeable Faith'))).toBe(
			'heard Fennik, of Unshakeable Faith · scanning for the recruit window…'
		);
	});

	// Scan now arms a burst that heard nobody. A prefix there would invent a
	// speaker the trigger never had.
	it('drops the prefix for a scan that heard nobody', () => {
		expect(statusLine(slice('scanning', null, null))).not.toContain('heard');
	});

	it('keeps the stale mark under a named scan', () => {
		expect(statusLine(slice('scanning', capture([row(0, ['matched'])]), 'Fennik'))).toBe(
			'heard Fennik · ' + SCANNING_LINE + STALE_VERDICT_SUFFIX
		);
	});

	// The whole point of the pause: nothing more will change on this strip, so
	// the decision in front of the player is the final one.
	it('says the read is done rather than claiming it is still reading', () => {
		expect(statusLine(slice('done', capture([row(0, ['matched', 'confirmed'])])))).toBe(
			'done · 1 row · all icons read'
		);
	});

	it('counts the rows of a finished read', () => {
		expect(
			statusLine(slice('done', capture([row(0, ['matched']), row(1, ['confirmed'])])))
		).toBe('done · 2 rows · all icons read');
	});

	// `done` is Rust's claim that everything was read. The line checks it
	// against the capture rather than repeating it, so a slice that arrived
	// with both cannot print "all icons read" over an unread cell.
	it('counts an unread icon under a done status rather than repeating the claim', () => {
		expect(statusLine(slice('done', capture([row(0, ['matched', 'unknown'])])))).toBe(
			'done · 1 row · 1 icon unread — hover to confirm'
		);
	});

	it('counts the rows and the icons still needing a hover', () => {
		expect(statusLine(slice('live', capture([row(0, ['matched', 'unknown']), row(1, ['ambiguous'])])))).toBe(
			'reading · 2 rows · 2 icons unread — hover to confirm'
		);
	});

	// Stated rather than left blank: silence is indistinguishable from a line
	// that stopped updating.
	it('says every icon was read rather than going quiet', () => {
		expect(statusLine(slice('live', capture([row(0, ['matched', 'confirmed'])])))).toBe(
			'reading · 1 row · all icons read'
		);
	});

	it('marks a capture whose window has gone', () => {
		expect(statusLine(slice('idle', capture([row(0, ['matched'])])))).toBe(WINDOW_GONE_NOTE);
	});

	// A burst armed after the last window closed is the more current fact, so
	// scanning wins the headline.
	it('reports a new burst rather than the window that already closed', () => {
		expect(statusLine(slice('scanning', capture([row(0, ['matched'])])))).toBe(
			SCANNING_LINE + STALE_VERDICT_SUFFIX
		);
	});

	// The other half of that precedence. The verdict block does NOT disappear
	// while the burst runs, so a plain scanning line would leave a stale verdict
	// on screen with nothing marking it stale — the exact failure the
	// window-gone marker exists to prevent.
	it('marks the verdict below as a previous read while it scans', () => {
		expect(statusLine(slice('scanning', capture([row(0, ['matched'])])))).toContain(
			STALE_VERDICT_SUFFIX
		);
	});

	it('adds no stale mark when the burst has no previous read to qualify', () => {
		expect(statusLine(slice('scanning', null))).not.toContain(STALE_VERDICT_SUFFIX);
	});

	// Unreachable from Rust — `run.rs` writes the status and the capture in one
	// publish — but the type admits it, and "reading · 0 rows" would be a claim
	// about a capture that does not exist.
	it('falls back to waiting when it is live with nothing captured', () => {
		expect(statusLine(slice('live', null))).toBe(IDLE_LINE);
	});

	it('says nothing at all when the module is off', () => {
		expect(statusLine(slice('off', capture([row(0, ['matched'])])))).toBeNull();
	});

	it('says nothing at all where capture is unavailable', () => {
		expect(statusLine(slice('unavailable', null))).toBeNull();
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

	// `done` means READ, not gone: the OCR stopped and the window did not. The
	// window-gone marker over a window the player is looking at would be the
	// same lie in the other direction.
	it('does not mark a capture the module has merely finished reading', () => {
		expect(captureRetired(slice('done', capture([row(0, ['matched'])])))).toBe(false);
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

describe('the one guides line', () => {
	// The smoke complaint: the strip spent two lines saying "Guide A SKIP" and
	// "Guide B SKIP". Which guide said no is not a decision the player makes
	// differently — the page keeps the per-guide breakdown.
	it('says SKIP once however many guides decided against it', () => {
		const line = guidesLine(
			verdict([source('guide-a', 'Guide A', 'skip'), source('guide-b', 'Guide B', 'skip')]),
			capture([])
		);
		expect(line).toEqual({ text: SKIP_LINE, tone: 'fail' });
	});

	// The one case where naming guides earns its line: this is what the player
	// is about to pay for.
	it('names only the guides that said WORTH', () => {
		const line = guidesLine(
			verdict([
				source('guide-a', 'Guide A', 'skip'),
				source('guide-b', 'Guide B', 'worth', ['kinetist-mid'], [
					ruleset('kinetist-mv', 'Kinetist', 'minimum viable'),
					ruleset('kinetist-mid', 'Kinetist', 'mid')
				])
			]),
			capture([])
		);
		expect(line).toEqual({ text: 'WORTH · Guide B (Kinetist mid)', tone: 'pass' });
	});

	it('names every guide that said WORTH when more than one did', () => {
		const line = guidesLine(
			verdict([
				source('guide-a', 'Guide A', 'worth', ['manyshot'], [ruleset('manyshot', 'Manyshot', null)]),
				source('guide-b', 'Guide B', 'worth', ['kinetist-mid'], [
					ruleset('kinetist-mid', 'Kinetist', 'mid')
				])
			]),
			capture([])
		);
		expect(line?.text).toBe('WORTH · Guide A (Manyshot), Guide B (Kinetist mid)');
	});

	// A WORTH outranks a SKIP beside it: one guide paying for this mercenary is
	// the actionable fact, and hiding it behind the other guide's no would lose
	// the play entirely.
	it('reports a WORTH even when another guide said SKIP', () => {
		const line = guidesLine(
			verdict([
				source('guide-a', 'Guide A', 'skip'),
				source('guide-b', 'Guide B', 'worth', ['manyshot'], [ruleset('manyshot', 'Manyshot', null)])
			]),
			capture([])
		);
		expect(line?.tone).toBe('pass');
	});

	// Nothing decided, and the reason is on the same line: a hover would change
	// this answer, which is not true of a SKIP.
	it('says unknown with the unread count when no guide could decide', () => {
		const line = guidesLine(
			verdict([source('guide-a', 'Guide A', 'unknown'), source('guide-b', 'Guide B', 'unknown')]),
			capture([row(0, ['unknown', 'ambiguous'])])
		);
		expect(line).toEqual({ text: 'unknown — 2 icons unread', tone: 'unknown' });
	});

	it('counts one unread icon in the singular', () => {
		const line = guidesLine(
			verdict([source('guide-a', 'Guide A', 'unknown')]),
			capture([row(0, ['unknown', 'matched'])])
		);
		expect(line?.text).toBe('unknown — 1 icon unread');
	});

	// An unknown with nothing to hover is an unread SKILL name, not an unread
	// icon — so the line must not offer a hover that would settle nothing.
	it('leaves the count off an unknown with every icon read', () => {
		const line = guidesLine(
			verdict([source('guide-a', 'Guide A', 'unknown')]),
			capture([row(0, ['matched'])])
		);
		expect(line?.text).toBe(UNKNOWN_LINE);
	});

	// A guide DID decide. The strip's live status line carries the unread count
	// either way, so the shorter wording hides nothing.
	it('reads a mixed SKIP and unknown as a SKIP', () => {
		const line = guidesLine(
			verdict([source('guide-a', 'Guide A', 'skip'), source('guide-b', 'Guide B', 'unknown')]),
			capture([row(0, ['unknown'])])
		);
		expect(line?.text).toBe(SKIP_LINE);
	});

	// The engine's own `off` headline is the one place the enabled set is read,
	// so a switched-off guide takes no part in the line.
	it('ignores a guide the user switched off', () => {
		const line = guidesLine(
			verdict([source('guide-a', 'Guide A', 'off'), source('guide-b', 'Guide B', 'skip')]),
			capture([])
		);
		expect(line?.text).toBe(SKIP_LINE);
	});

	// Drawing SKIP here would put a verdict on screen that no guide gave.
	it('says so when every guide is switched off rather than drawing a SKIP', () => {
		const line = guidesLine(
			verdict([source('guide-a', 'Guide A', 'off'), source('guide-b', 'Guide B', 'off')]),
			capture([])
		);
		expect(line).toEqual({ text: NO_GUIDES_NOTE, tone: 'muted' });
	});

	it('has nothing to say before anything is captured', () => {
		expect(guidesLine(null, null)).toBeNull();
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
		expect(unreadNote(slice('idle', capture([row(0, ['matched'])])))).toBeNull();
	});

	it('asks for a hover and says how many cells need one', () => {
		expect(unreadNote(slice('idle', capture([row(0, ['unknown', 'unknown'])])))).toBe(
			'2 icons unread — hover to confirm'
		);
	});

	it('counts one icon in the singular', () => {
		expect(unreadNote(slice('idle', capture([row(0, ['unknown'])])))).toBe(
			'1 icon unread — hover to confirm'
		);
	});

	// The live status line already carries the count and the glyph rows say
	// which cells. A strip that prints the same total twice teaches the reader
	// to skip both lines.
	it('leaves the count to the status line while the window is live', () => {
		expect(unreadNote(slice('live', capture([row(0, ['unknown', 'unknown'])])))).toBeNull();
	});

	// Same rule under `done`: that line counts too, and a strip that prints the
	// same total twice teaches the reader to skip both lines.
	it('leaves the count to the status line under a finished read too', () => {
		expect(unreadNote(slice('done', capture([row(0, ['unknown', 'unknown'])])))).toBeNull();
	});

	it('has nothing to say before anything is captured', () => {
		expect(unreadNote(slice('idle', null))).toBeNull();
	});
});

describe('the per-row glyphs', () => {
	it('names the skill and marks every support cell', () => {
		expect(rowGlyphs(capture([row(0, ['matched', 'low_confidence', 'unknown'])]))).toEqual([
			{
				index: 0,
				skill: 'Ice Shot',
				glyphs: [
					{ glyph: '✓', tone: 'pass' },
					{ glyph: '?', tone: 'unknown' },
					{ glyph: '✕', tone: 'fail' }
				],
				note: null
			}
		]);
	});

	// The smoke complaint: `✓ ✓ ? ✕` was one colour, so the cell that needed a
	// hover had to be found by reading the run character by character.
	it('paints a read cell, an unsure one and an unread one in three tones', () => {
		const tones = rowGlyphs(capture([row(0, ['matched', 'ambiguous', 'unknown'])]))[0].glyphs.map(
			(cell) => cell.tone
		);
		expect(new Set(tones).size).toBe(3);
	});

	// A hover confirmation reads the same as a match — the point of hovering is
	// that the glyph stops asking for one.
	it('marks a hover-confirmed cell as read', () => {
		expect(rowGlyphs(capture([row(0, ['confirmed'])]))[0].glyphs).toEqual([
			{ glyph: '✓', tone: 'pass' }
		]);
	});

	it('marks an ambiguous cell as unsure rather than as read', () => {
		expect(rowGlyphs(capture([row(0, ['ambiguous'])]))[0].glyphs).toEqual([
			{ glyph: '?', tone: 'unknown' }
		]);
	});

	// A low-confidence read is the same question a hover answers, so it wears
	// the same colour as an ambiguous one and NOT the red of a cell nobody read.
	it('does not paint a low-confidence cell as unread', () => {
		expect(rowGlyphs(capture([row(0, ['low_confidence'])]))[0].glyphs[0].tone).toBe('unknown');
	});

	it('keeps every row of the capture', () => {
		expect(rowGlyphs(capture([row(0, ['matched']), row(1, ['unknown'])]))).toHaveLength(2);
	});

	// The index is what the route keys the `{#each}` on, so a row order that
	// drifted from the capture's would repaint the wrong line.
	it('carries each row index through in the order the reader found them', () => {
		expect(rowGlyphs(capture([row(0, ['matched']), row(1, ['unknown'])])).map((r) => r.index)).toEqual([
			0, 1
		]);
	});

	it('glyphs each row against its own cells', () => {
		expect(
			rowGlyphs(capture([row(0, ['matched']), row(1, ['unknown'])])).map((r) =>
				r.glyphs.map((cell) => cell.glyph).join('')
			)
		).toEqual(['✓', '✕']);
	});

	// An empty glyph run would render as a skill whose cells simply did not
	// draw, which is a different and wrong claim about the recruit window.
	it('marks a row that has no cells rather than drawing an empty run', () => {
		const [only] = rowGlyphs(capture([row(0, [])]));
		expect(only.glyphs).toEqual([]);
		expect(only.note).toBe(NO_CELLS_NOTE);
	});

	// The 2026-08-25 smoke read "no cells read", which claims the reader FAILED
	// on this row. Nothing failed: the panel shows this skill without supports,
	// and the `✕` next to it is what a cell nobody could read looks like.
	it('does not word a row without cells as a failed read', () => {
		expect(NO_CELLS_NOTE).not.toMatch(/read|unread|fail/i);
	});

	it('leaves a row that HAS cells unmarked', () => {
		expect(rowGlyphs(capture([row(0, ['matched'])]))[0].note).toBeNull();
	});

	it('says a skill name was not read rather than printing nothing', () => {
		const unnamed = capture([row(0, ['matched'])]);
		unnamed.rows[0].skill = { ...unnamed.rows[0].skill, name: null };
		expect(rowGlyphs(unnamed)[0].skill).toBe('skill not read');
	});

	it('has nothing to draw for a capture with no rows', () => {
		expect(rowGlyphs(capture([]))).toEqual([]);
	});
});

describe('the on-screen gate on the glyph rows', () => {
	it('draws the rows of a window that is on screen', () => {
		expect(liveRowGlyphs(slice('live', capture([row(0, ['matched'])])))).toHaveLength(1);
	});

	// A fully-read window is still ON SCREEN — the OCR stopped, the window did
	// not. Dropping the rows here would blank the strip at the exact moment the
	// player is deciding on a finished read.
	it('keeps drawing the rows of a capture the module has finished reading', () => {
		expect(liveRowGlyphs(slice('done', capture([row(0, ['matched'])])))).toHaveLength(1);
	});

	// A retired capture's cells cannot be hovered any more, so offering them
	// would be an instruction the player cannot follow.
	it('draws nothing for a capture whose window has gone', () => {
		expect(liveRowGlyphs(slice('idle', capture([row(0, ['unknown'])])))).toEqual([]);
	});

	it('draws nothing while a burst is still looking', () => {
		expect(liveRowGlyphs(slice('scanning', capture([row(0, ['unknown'])])))).toEqual([]);
	});

	it('draws nothing before anything is captured', () => {
		expect(liveRowGlyphs(slice('live', null))).toEqual([]);
	});
});
