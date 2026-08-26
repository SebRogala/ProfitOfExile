/**
 * Presentation derivations for the Mercenaries page.
 *
 * This is a VIEW module, not part of the data model: `rulesets.ts` declares what
 * the saved searches say, this file reshapes that into what the page draws —
 * quantifier prose, kind wording, and the guide-b tier matrices. Nothing
 * here invents a rule: every per-tier state is `entryKind`'s answer for the
 * matching group/entry in that rung, so the type-first denial rule survives the
 * transposition instead of being re-derived from the switches.
 *
 * It lives outside `MercenariesPage.svelte` because a `.svelte` page is not
 * unit-testable in this project (no component harness), and the matrix
 * derivation is the one part of the page that can be wrong quietly.
 */

import {
	entryKind,
	type MercFilterEntry,
	type MercFilterGroup,
	type MercRuleset,
	type MercTier
} from './rulesets';
import type { MercRulesetResult, RulesetOutcome } from './verdict';

/** Column heads of a tier ladder, in `TIERS` order. */
export const TIER_LABELS: Record<MercTier, string> = {
	mv: 'minimum viable',
	mid: 'mid',
	end: 'endgame',
	gg: 'GG'
};

export type MercEntryKind = ReturnType<typeof entryKind>;

/**
 * One matrix cell. `absent` is not a rule — it means the rung has no such group
 * or no such entry.
 *
 * It renders: the Manyshot ladder's rungs do NOT share one skeleton (its GG rung
 * has no "carries Vaal Ice Shot" group, merges the projectile and damage groups,
 * and drops a parked entry), so those holes are drawn as holes rather than
 * borrowing the neighbouring column's state.
 */
export type LadderCell = MercEntryKind | 'absent';

const ABSENT_QUANTIFIER = '—';

/** Entries the saved search actually has switched on, ignoring the group switch. */
function enabledEntries(group: MercFilterGroup) {
	return group.entries.filter((e) => e.enabledInSearch);
}

/**
 * The quantifier split into its pieces, so the matrix can collapse four rungs
 * that differ only in the number ("at least 2 · 2 · 3 · 2 of:") without parsing
 * the rendered sentence back apart. `min` is null for the wordings that carry no
 * number.
 */
export interface QuantifierParts {
	prefix: string;
	min: number | null;
	suffix: string;
}

/**
 * How many of the group's entries a mercenary has to satisfy, in words.
 *
 * An absent `min` means "all enabled entries must match" on a positive group
 * and "none of these may be present" on a `not` group — the trade site's own
 * semantics, which the raw number alone does not carry.
 */
export function quantifierParts(group: MercFilterGroup): QuantifierParts {
	const words = (prefix: string): QuantifierParts => ({ prefix, min: null, suffix: '' });
	if (enabledEntries(group).length === 0) return words('bonus vocabulary — none enabled');
	if (group.type === 'not') return words('none of:');
	// A switched-off positive group is not applying its requirement — saying
	// "all of:" under an "off in this search" badge would claim it is. (A
	// switched-off `not` group still reads "none of:" — a parked denial is
	// still a denial, same type-first rule as entryKind.)
	if (!group.enabledInSearch) return words('when enabled:');
	if (group.min !== undefined) return { prefix: 'at least', min: group.min, suffix: 'of:' };
	return words('all of:');
}

/** `quantifierParts` rendered as the sentence the page prints. */
export function quantifier(group: MercFilterGroup): string {
	const parts = quantifierParts(group);
	return parts.min === null ? parts.prefix : `${parts.prefix} ${parts.min} ${parts.suffix}`;
}

/** Tooltip naming the kind in words — glyph and colour alone must not be the
 *  only carriers (colourblind + screen-reader parity). */
export function kindTitle(kind: MercEntryKind): string {
	if (kind === 'required') return 'required';
	if (kind === 'forbidden') return 'denied';
	return 'bonus — off in this search';
}

/** The one value every element shares, or null when they differ (or there are none). */
export function sharedValue<T>(values: T[]): T | null {
	if (values.length === 0) return null;
	return values.every((v) => v === values[0]) ? values[0] : null;
}

/** Head text for a rung's column: its tier, falling back to its own label. */
export function columnLabel(ruleset: MercRuleset): string {
	return ruleset.tier ? TIER_LABELS[ruleset.tier] : ruleset.label;
}

/** A group header, spanning the matrix, carrying what the group asks per rung. */
export interface LadderGroupRow {
	kind: 'group';
	/** Row key — the group id, distinct from the `groupId/entryId` entry keys. */
	id: string;
	label: string;
	/** Collapsed when every rung agrees, per-rung (in column order) when they do not. */
	quantifier: string;
	/** Column labels whose rung has this group switched off. */
	offIn: string[];
	varies: boolean;
}

/** One skeleton entry, with its resolved state in each rung's column. */
export interface LadderEntryRow {
	kind: 'entry';
	/** Row key — entry ids recur across sibling groups, so the group id qualifies it. */
	id: string;
	/** The two halves of `id`, carried rather than parsed back out: the page needs
	 *  them to look this row's verdict position up per rung. */
	groupId: string;
	entryId: string;
	name: string;
	/** One per rung, in the order the rungs were passed. */
	cells: LadderCell[];
	varies: boolean;
}

export type LadderRow = LadderGroupRow | LadderEntryRow;

/**
 * The quantifier line for one group across the rungs: a single sentence when they
 * agree, "at least 2 · 2 · 3 · 2 of:" when only the number moves, and the
 * sentences joined in column order for any other disagreement.
 *
 * Two kinds of rung are excluded from the wording, because each already carries
 * its own state elsewhere in the row. A rung that PARKS the group
 * (`enabledInSearch: false` on a positive group) is carried by the "off in
 * <rung>" badge, and letting it vote produced noise like
 * "all of: · when enabled: · all of: · all of:" for a group that reads
 * "all of:" everywhere it is actually live. A rung the group is ABSENT from is
 * carried by the — cells under it, and letting it vote produced the same noise
 * one step worse ("all of: · all of: · all of: · —" for the Manyshot
 * `has-vaal` group, which asks the same thing in every rung that has it).
 */
function ladderQuantifier(perRung: (MercFilterGroup | undefined)[]): string {
	const voting = perRung.filter(
		(g): g is MercFilterGroup => g !== undefined && (g.enabledInSearch || g.type === 'not')
	);
	const sentences = voting.map((g) => quantifier(g));
	if (sentences.length === 0) return quantifier(perRung.find((g) => g !== undefined)!);
	const agreed = sharedValue(sentences);
	if (agreed !== null) return agreed;

	const parts = voting.map((g) => quantifierParts(g));
	const mins = parts.map((p) => p.min);
	const prefix = sharedValue(parts.map((p) => p.prefix));
	const suffix = sharedValue(parts.map((p) => p.suffix));
	if (prefix !== null && suffix !== null && mins.every((m) => m !== null)) {
		return `${prefix} ${mins.join(' · ')} ${suffix}`;
	}
	return sentences.join(' · ');
}

/**
 * The row skeleton: every group id any rung declares, in first-appearance order
 * across the rungs, each carrying every entry id any rung puts under it.
 *
 * The UNION rather than the first rung's own groups, because a rung is allowed
 * to drift: the Manyshot ladder's cheapest rung has no aura group at all, and
 * taking the skeleton from it alone would drop the aura rows off the matrix
 * entirely — silently, since the higher rungs' columns would still render.
 *
 * A slot's `label` is resolved the way `ladderQuantifier` resolves the
 * quantifier, not taken from the first rung that declares the id: the Manyshot
 * `projectiles` group is 'Ice Shot + projectile links' on three rungs and
 * 'Ice Shot + projectile and damage links' on the GG rung that merged the
 * damage vocabulary into it, and heading the merged rows with the first
 * declarer's label would tell the reader the GG rung asks for something it
 * does not.
 */
function skeletonOf(rungs: MercRuleset[]): { id: string; label: string; entries: MercFilterEntry[] }[] {
	const slots = new Map<string, { id: string; labels: string[]; entries: MercFilterEntry[] }>();
	for (const rung of rungs) {
		for (const group of rung.groups) {
			const slot = slots.get(group.id) ?? { id: group.id, labels: [], entries: [] };
			slot.labels.push(group.label);
			for (const entry of group.entries) {
				if (!slot.entries.some((known) => known.id === entry.id)) slot.entries.push(entry);
			}
			slots.set(group.id, slot);
		}
	}
	return [...slots.values()].map((slot) => ({
		id: slot.id,
		label: sharedValue(slot.labels) ?? [...new Set(slot.labels)].join(' · '),
		entries: slot.entries
	}));
}

/**
 * Transpose the ladder into matrix rows: the rungs together supply the skeleton
 * (group order, then entry order), every rung supplies one column.
 *
 * Lookups go by group id then entry id rather than by position, so a rung that
 * does not carry a skeleton slot reports `absent` in its own column instead of
 * shifting every state one row up.
 */
export function ladderRows(rungs: MercRuleset[]): LadderRow[] {
	const rows: LadderRow[] = [];
	for (const group of skeletonOf(rungs)) {
		const perRung = rungs.map((rung) => rung.groups.find((g) => g.id === group.id));
		const offIn = rungs
			.filter((_, i) => perRung[i]?.enabledInSearch === false)
			.map((rung) => columnLabel(rung));
		const sentences = perRung.map((g) => (g ? quantifier(g) : ABSENT_QUANTIFIER));
		rows.push({
			kind: 'group',
			id: group.id,
			label: group.label,
			quantifier: ladderQuantifier(perRung),
			offIn,
			// A group parked in every rung is not a delta — only a switch that moves
			// between rungs is something the reader is hunting for.
			varies: sharedValue(sentences) === null || (offIn.length > 0 && offIn.length < rungs.length)
		});

		for (const entry of group.entries) {
			const cells: LadderCell[] = perRung.map((rungGroup) => {
				const match = rungGroup?.entries.find((e) => e.id === entry.id);
				return rungGroup && match ? entryKind(rungGroup, match) : 'absent';
			});
			rows.push({
				kind: 'entry',
				id: `${group.id}/${entry.id}`,
				groupId: group.id,
				entryId: entry.id,
				name: entry.name,
				cells,
				varies: sharedValue(cells) === null
			});
		}
	}
	return rows;
}

/**
 * The verdict's outcome for each rung, in the matrix's column order — the
 * header row that turns the ladder from "what the rungs ask" into "what THIS
 * mercenary answers".
 *
 * A rung with no result in hand is `null`, not a fabricated outcome: that is
 * the state before any capture has arrived and whenever the source is switched
 * off, and drawing a badge there would claim a verdict nobody computed. Results
 * are matched by ruleset id rather than by position, for the same reason
 * `ladderRows` looks groups up by id: a mismatch must show a hole, never the
 * neighbouring column's answer.
 */
export function rungOutcomes(
	rungs: MercRuleset[],
	results: MercRulesetResult[]
): (RulesetOutcome | null)[] {
	return rungs.map((rung) => results.find((result) => result.id === rung.id)?.outcome ?? null);
}
