/**
 * Presentation derivations for the capture + verdict half of the Mercenaries
 * page — the sibling of `ladder-view.ts`, which does the same job for the
 * rulesets half.
 *
 * Nothing here decides anything: `verdict.ts` owns every outcome and `capture.ts`
 * owns every read, and this file only turns them into the words, glyphs and
 * lookup keys the page prints. It lives outside `MercenariesPage.svelte` because
 * a `.svelte` page has no unit-test harness in this project, and the wording is
 * the part of the page that can be wrong quietly.
 *
 * The wording rule the task pins (POE-165, Debuggability requirement): a read
 * that is not confident says so IN THE CELL — "unknown — hover to confirm",
 * never a blank and never a name presented as if it were read. The glyph and
 * the colour are the fast channel; the `title` carries the raw text, the score
 * and the state in words, which is the channel a colour-blind reader and a
 * screen reader both get.
 */

import type { MercCapture, MercSkillRead, MercSupportRead, ReadState } from './capture';
import type {
	CaptureSite,
	GroupOutcome,
	MercGroupResult,
	MercPosition,
	MercRulesetResult,
	PositionOutcome,
	RulesetOutcome,
	SourceHeadline
} from './verdict';

/** What the page appends to every read it does not trust. */
const HOVER_HINT = 'hover to confirm';

/**
 * One glyph per read state, in the slice-1 vocabulary: ✓ read, ? read but not
 * trusted, ✕ not read at all. Text rather than shapes, so it survives a
 * colour-blind reader; the wording behind it is on the cell's `title`.
 */
export const READ_GLYPH: Record<ReadState, string> = {
	matched: '✓',
	confirmed: '✓',
	low_confidence: '?',
	ambiguous: '?',
	unknown: '✕'
};

/** Colour bucket for a read state — the page's CSS class suffix. */
export type ReadTone = 'read' | 'unsure' | 'unread';

export const READ_TONE: Record<ReadState, ReadTone> = {
	matched: 'read',
	confirmed: 'read',
	low_confidence: 'unsure',
	ambiguous: 'unsure',
	unknown: 'unread'
};

/** The state in words, for the `title` attribute. */
export const READ_STATE_LABEL: Record<ReadState, string> = {
	matched: 'matched',
	confirmed: 'confirmed by hover',
	low_confidence: `low confidence — ${HOVER_HINT}`,
	unknown: `not read — ${HOVER_HINT}`,
	ambiguous: `ambiguous — ${HOVER_HINT}`
};

function score(value: number): string {
	return value.toFixed(2);
}

/** `Return (Tier 3)` from the icon family and its badge, as far as both were read. */
function familyTier(read: MercSupportRead): string | null {
	if (read.family === null) return null;
	return read.tier === null ? read.family : `${read.family} (Tier ${read.tier})`;
}

/**
 * What a skill row's name cell says.
 *
 * The OCR text is kept even when it matched nothing — "K1net1c 8last" is what
 * makes a misread attributable to the capture rather than to the rules.
 */
export function skillText(read: MercSkillRead): string {
	const seen = read.name ?? read.raw;
	if (read.state === 'matched' || read.state === 'confirmed') return seen;
	if (read.state === 'unknown') return `${seen || 'unknown'} — not read, ${HOVER_HINT}`;
	if (read.state === 'ambiguous') return `${seen || 'unknown'} — ambiguous, ${HOVER_HINT}`;
	return `${seen || 'unknown'} — low confidence ${score(read.score)}, ${HOVER_HINT}`;
}

/** Raw text, score and state — the whole basis for what `skillText` printed. */
export function skillTitle(read: MercSkillRead): string {
	return `OCR read "${read.raw}" · score ${score(read.score)} · ${READ_STATE_LABEL[read.state]}`;
}

/**
 * What a support cell says.
 *
 * An ambiguous cell names its candidates (Greater vs Gilded at tier 3 is the
 * real case) instead of picking one; a cell with no template match at all says
 * "unknown", because the alternative — printing nothing — reads as an empty
 * slot, which is a different fact about the mercenary.
 */
export function supportText(read: MercSupportRead): string {
	const named = read.name ?? familyTier(read);
	if (read.state === 'matched' || read.state === 'confirmed') return named ?? 'read, unnamed';
	if (read.state === 'ambiguous') {
		return read.candidates.length > 0
			? `ambiguous: ${read.candidates.join(' | ')}`
			: `ambiguous — ${HOVER_HINT}`;
	}
	if (read.state === 'unknown') return `unknown — ${HOVER_HINT}`;
	return `${named ?? 'unknown'} — low confidence ${score(read.score)}, ${HOVER_HINT}`;
}

/** Icon family, tier badge, score and state — the basis for `supportText`. */
export function supportTitle(read: MercSupportRead): string {
	const icon = read.family === null ? 'no icon match' : `icon ${read.family}`;
	const tier = read.tier === null ? 'no tier badge' : `tier ${read.tier}`;
	const base = `${icon} · ${tier} · score ${score(read.score)} · ${READ_STATE_LABEL[read.state]}`;
	return read.candidates.length > 0 ? `${base} · candidates: ${read.candidates.join(' | ')}` : base;
}

/**
 * Where a verdict position was seen, in the capture's own terms.
 *
 * Rows and slots are printed 1-based because that is how the recruit window
 * reads on screen. A site pointing at a row or slot the capture does not have
 * is reported as the bare position rather than dropped: it means the capture
 * moved under the verdict, and hiding that would look like an absent stat.
 */
export function capturedAt(capture: MercCapture, site: CaptureSite | null): string {
	if (site === null) return 'not seen';
	const row = capture.rows.find((candidate) => candidate.index === site.rowIndex);
	const where = `row ${site.rowIndex + 1}`;
	if (site.slot === null) return row ? `${where} · ${skillText(row.skill)}` : where;
	const slot = `${where} slot ${site.slot + 1}`;
	const support = row?.supports.find((candidate) => candidate.slot === site.slot);
	return support ? `${slot} · ${supportText(support)}` : slot;
}

/**
 * Lookup key for one rule position.
 *
 * The ruleset id is part of the key because guide B's four rungs share one group
 * id sequence, and the group id is part of it because sibling groups legitimately
 * repeat an entry id (GMP sits in several). Keying on less than all three would
 * show one rung's outcome in another's row.
 */
export function positionKey(rulesetId: string, groupId: string, entryId: string): string {
	return `${rulesetId}/${groupId}/${entryId}`;
}

/** Same key, minus the entry — for a group's own outcome. */
export function groupKey(rulesetId: string, groupId: string): string {
	return `${rulesetId}/${groupId}`;
}

/** Every rule position of a source's rulesets, by `positionKey`. */
export function indexPositions(rulesets: MercRulesetResult[]): Map<string, MercPosition> {
	const index = new Map<string, MercPosition>();
	for (const ruleset of rulesets) {
		for (const group of ruleset.groups) {
			for (const position of group.positions) {
				index.set(positionKey(ruleset.id, group.id, position.entryId), position);
			}
		}
	}
	return index;
}

/** Every group result of a source's rulesets, by `groupKey`. */
export function indexGroups(rulesets: MercRulesetResult[]): Map<string, MercGroupResult> {
	const index = new Map<string, MercGroupResult>();
	for (const ruleset of rulesets) {
		for (const group of ruleset.groups) index.set(groupKey(ruleset.id, group.id), group);
	}
	return index;
}

/** Colour bucket for an outcome badge — the page's CSS class suffix. */
export type OutcomeTone = 'pass' | 'fail' | 'unknown' | 'bonus' | 'muted';

const POSITION_OUTCOME_LABEL: Record<PositionOutcome, string> = {
	pass: 'present',
	fail: 'present — forbidden',
	absent: 'absent',
	'bonus-fired': 'bonus fired',
	'parked-denial-present': 'present — denial parked in this search',
	'contextual-present': 'buyer-contextual — present',
	'contextual-absent': 'buyer-contextual — absent',
	unknown: `unknown — ${HOVER_HINT}`,
	'not-applied': 'not applied'
};

/**
 * What one rule position says, in the terms of the rule it belongs to.
 *
 * `pass` means opposite things on the two kinds — a required entry passes by
 * being PRESENT, a forbidden one by being ABSENT — so the outcome alone cannot
 * be worded. Labelling both "present" is how the page would tell a reader that
 * the mercenary carries the exact stat the search exists to reject.
 */
export function positionOutcomeLabel(kind: MercPosition['kind'], outcome: PositionOutcome): string {
	if (kind === 'forbidden') {
		if (outcome === 'pass') return 'absent — clear';
		if (outcome === 'unknown') return `cannot rule out — ${HOVER_HINT}`;
	}
	return POSITION_OUTCOME_LABEL[outcome];
}

export const POSITION_OUTCOME_TONE: Record<PositionOutcome, OutcomeTone> = {
	pass: 'pass',
	fail: 'fail',
	absent: 'muted',
	'bonus-fired': 'bonus',
	'parked-denial-present': 'unknown',
	'contextual-present': 'bonus',
	'contextual-absent': 'muted',
	unknown: 'unknown',
	'not-applied': 'muted'
};

export const RULESET_OUTCOME_LABEL: Record<RulesetOutcome, string> = {
	pass: 'matches',
	fail: 'no match',
	unknown: 'unknown'
};

export const RULESET_OUTCOME_TONE: Record<RulesetOutcome, OutcomeTone> = {
	pass: 'pass',
	fail: 'fail',
	unknown: 'unknown'
};

export const HEADLINE_LABEL: Record<SourceHeadline, string> = {
	worth: 'WORTH',
	skip: 'SKIP',
	unknown: 'UNKNOWN',
	off: 'OFF'
};

export const HEADLINE_TONE: Record<SourceHeadline, OutcomeTone> = {
	worth: 'pass',
	skip: 'fail',
	unknown: 'unknown',
	off: 'muted'
};

/** One entry of the template store, as the page shows it and forgets it. */
export interface LearnedTemplate {
	family: string;
	/** Null when the store entry carries no tier — `merc_forget_template` gets the null. */
	tier: number | null;
	label: string;
}

/**
 * Split a template-store entry into the `(family, tier)` pair
 * `merc_forget_template` takes.
 *
 * The store keys its files `<family>--<tier>.png` (plan D4), so `--<digits>` at
 * the end is the tier and everything before it is the family — which may itself
 * contain a `--`, hence the last separator, not the first. An entry that does
 * not end that way is passed through whole with a null tier rather than being
 * guessed at: forgetting the wrong key would leave the bad template in place
 * while the page claimed otherwise.
 */
export function parseLearnedTemplate(raw: string): LearnedTemplate {
	const match = /^(.*)--(\d+)$/.exec(raw);
	if (!match) return { family: raw, tier: null, label: raw };
	const family = match[1];
	const tier = Number(match[2]);
	return { family, tier, label: `${family} (Tier ${tier})` };
}

/**
 * What `merc_debug_capture` returned, as text.
 *
 * The report is rendered verbatim rather than field by field on purpose: it is
 * a debug dump whose fields exist to be read by a human on the first Windows
 * run, and picking fields here would silently drop whatever the Rust side adds
 * to it. A plain string (a dump path) is shown as itself.
 */
export function describeDebugResult(value: unknown): string {
	if (value === null || value === undefined) return 'command returned no report';
	// An empty string is the same nothing as a missing one — rendering it would
	// leave the report block open and blank, which reads as a dump with no
	// findings rather than as a command that said nothing.
	if (typeof value === 'string') return value === '' ? 'command returned no report' : value;
	return JSON.stringify(value, null, 2);
}

export const GROUP_OUTCOME_LABEL: Record<GroupOutcome, string> = {
	pass: 'satisfied',
	fail: 'not satisfied',
	unknown: 'unknown',
	'not-applied': 'not applied'
};

export const GROUP_OUTCOME_TONE: Record<GroupOutcome, OutcomeTone> = {
	pass: 'pass',
	fail: 'fail',
	unknown: 'unknown',
	'not-applied': 'muted'
};


/**
 * The confidently-read stats no ruleset of this set mentions, deduplicated by
 * name and in capture order.
 *
 * The rungs of a ladder share one entry skeleton, so their `notInRules` lists
 * repeat; the page shows ONE line under the matrix rather than four identical
 * ones. Deduplication is by display name because that is what the line prints —
 * two reads of the same stat on different rows are one thing to report.
 */
export function notInRulesNames(rulesets: MercRulesetResult[]): string[] {
	const names: string[] = [];
	for (const ruleset of rulesets) {
		for (const captured of ruleset.notInRules) {
			if (!names.includes(captured.name)) names.push(captured.name);
		}
	}
	return names;
}
