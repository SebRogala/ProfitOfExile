/**
 * Verdict engine — what a captured mercenary is worth, per source, per rule.
 *
 * Pure: it takes a capture and the ruleset data model and returns a result
 * tree. It never reads the SSOT store, never fetches, and never renders. The
 * page draws what comes back; anything the page needs to explain a verdict has
 * to be IN the result, because the task's Debuggability requirement is that a
 * SKIP names the exact failing position and a WORTH names the tier and the
 * bonuses that fired.
 *
 * Two rules shape everything below.
 *
 * **Sources are evaluated independently.** Guide A and guide B disagree — guide
 * A denies Barrage, guide B's cheapest rung accepts it — and that is not a
 * conflict to resolve: a mercenary failing one guide's gate can still be the
 * other's sale (POE-165, Sebastian's ruling). Rules are never merged across
 * sources.
 *
 * **A read that is not confident is UNKNOWN, never absent.** `matched` and
 * `confirmed` are the confident states; a `low_confidence`, `unknown` or
 * `ambiguous` read could be anything of its kind, so it makes every entry of
 * that kind possible-but-unproven for as long as it stands. A group that cannot
 * reach its minimum on confident reads alone, but could if the unread cells
 * turned out right, is `unknown` — never `fail`. That is what makes
 * "unknown — hover to confirm" honest instead of a silent SKIP.
 */

import {
	TIERS,
	entryKind,
	entryRole,
	isRowAnchor,
	type MercFilterEntry,
	type MercFilterGroup,
	type MercGroupType,
	type MercRuleset,
	type MercSource,
	type MercSourceId
} from './rulesets';
import {
	derivedSearchUrl,
	flipKey,
	rulesetQuery,
	savedSearchUrl,
	type QueryFlips
} from './trade-links';
import type { MercCapture, MercRow, MercSkillRead, MercSupportRead, ReadState } from './capture';

/** Whether the capture proves an entry present, proves it absent, or cannot say. */
export type Presence = 'present' | 'unknown' | 'absent';

/** Per rule position — one row of the debug view. */
export type PositionOutcome =
	| 'pass'
	| 'fail'
	| 'absent'
	| 'bonus-fired'
	| 'parked-denial-present'
	| 'contextual-present'
	| 'contextual-absent'
	| 'unknown'
	| 'not-applied';

export type GroupOutcome = 'pass' | 'fail' | 'unknown' | 'not-applied';
export type RulesetOutcome = 'pass' | 'fail' | 'unknown';
export type SourceHeadline = 'worth' | 'skip' | 'unknown' | 'off';

/** Where in the capture something was seen. `slot` is null for a skill row's name. */
export interface CaptureSite {
	rowIndex: number;
	slot: number | null;
}

export interface MercPosition {
	groupId: string;
	groupLabel: string;
	groupType: MercGroupType;
	entryId: string;
	entryName: string;
	kind: 'required' | 'forbidden' | 'bonus';
	buyerContextual: boolean;
	/** Whether this entry counts toward the group's minimum — bonuses and dropped contextuals do not. */
	counted: boolean;
	presence: Presence;
	outcome: PositionOutcome;
	/** Where the entry was seen, or the unread cell that leaves it unknown. */
	site: CaptureSite | null;
}

export interface MercGroupResult {
	id: string;
	label: string;
	type: MercGroupType;
	/** False when the saved search has the group switched off — its entries are bonuses. */
	applied: boolean;
	min: number | null;
	/** How many of the counted entries the group needs; 0 for a `not` group. */
	need: number;
	/** How many the capture proves present. */
	confident: number;
	outcome: GroupOutcome;
	/** For a row-scoped group, the row this outcome came from. */
	rowIndex: number | null;
	positions: MercPosition[];
}

/** A confidently read skill or support that no entry of the ruleset mentions. */
export interface MercCaptured {
	ids: string[];
	name: string;
	site: CaptureSite;
}

export interface MercRulesetResult {
	id: string;
	label: string;
	/** The ladder key this rung belongs to, or null for an untiered ruleset. */
	ladder: string | null;
	tier: string | null;
	/**
	 * The rung's own wording for its tier, or null when the tier names it. Two
	 * rungs of one ladder can share a tier, and the verdict has to be able to say
	 * which of them answered.
	 */
	tierLabel: string | null;
	outcome: RulesetOutcome;
	groups: MercGroupResult[];
	notInRules: MercCaptured[];
	reasons: string[];
	floor: string | null;
	/**
	 * The guide's own saved search, or NULL for a ruleset transcribed from PROSE
	 * — guide-c publishes no trade links, so there is nothing to open. Null
	 * rather than a fabricated URL: a hash-shaped link to a search GGG never
	 * saved would 404, and a link back to the article would be a different kind
	 * of thing wearing the "open saved search" label. The derived link below is
	 * unaffected — `rulesetQuery` builds from the data model and needs no hash.
	 */
	savedUrl: string | null;
	/**
	 * The same search with this mercenary's bonuses and contextual entries
	 * flipped, or null when the active league is not known yet — a derived
	 * search has to name a league, and guessing one would comp against the
	 * wrong market.
	 */
	derivedUrl: string | null;
}

export interface MercSourceVerdict {
	id: MercSourceId;
	label: string;
	headline: SourceHeadline;
	/** Ruleset ids to comp against: the highest passing rung of EACH ladder, plus every passing untiered ruleset. */
	best: string[];
	reasons: string[];
	/** Empty when the source is switched off — a disabled source is not evaluated. */
	rulesets: MercRulesetResult[];
}

export interface MercVerdict {
	sources: MercSourceVerdict[];
}

/**
 * The two states a read is TRUSTED in. Exported because the overlay counts the
 * cells that are not in them (`overlay-view.ts`) and a second list of the same
 * two strings is how the engine and the count would drift apart.
 */
export const CONFIDENT_STATES: ReadState[] = ['matched', 'confirmed'];

/** Whether a read proves what it says. Everything else is UNKNOWN, not absent. */
export function isConfident(state: ReadState): boolean {
	return CONFIDENT_STATES.includes(state);
}

interface ScopedRead<T> {
	read: T;
	site: CaptureSite;
}

/** The reads one evaluation may look at, split by what kind of entry they can satisfy. */
interface Scope {
	skills: ScopedRead<MercSkillRead>[];
	supports: ScopedRead<MercSupportRead>[];
}

function scopeOf(rows: MercRow[]): Scope {
	return {
		skills: rows.map((row) => ({ read: row.skill, site: { rowIndex: row.index, slot: null } })),
		supports: rows.flatMap((row) =>
			row.supports.map((support) => ({
				read: support,
				site: { rowIndex: row.index, slot: support.slot }
			}))
		)
	};
}

interface Sighting {
	presence: Presence;
	site: CaptureSite | null;
}

/**
 * Does the scope contain this stat?
 *
 * Membership is on the read's `ids` SET, not on a single id: one display name
 * can resolve to several stat ids, and a hover confirmation carries all of
 * them. An entry only counts as present when a CONFIDENT read carries its id;
 * otherwise any unconfident read of the same kind (skill vs support) leaves it
 * unknown, because such a read tells us nothing about what the cell really is.
 */
function presenceOf(scope: Scope, entryId: string): Sighting {
	const pool: ScopedRead<{ ids: string[]; state: ReadState }>[] =
		entryRole(entryId) === 'skill' ? scope.skills : scope.supports;

	for (const candidate of pool) {
		if (isConfident(candidate.read.state) && candidate.read.ids.includes(entryId)) {
			return { presence: 'present', site: candidate.site };
		}
	}
	// An `ambiguous` read knows WHAT it saw and only which stat id to call it —
	// so it leaves its own candidates unknown and says nothing about anything
	// else. A `low_confidence` or `unknown` read has no such footing: the guess
	// itself is unreliable, so it could be any stat of its kind.
	const unread = pool.find((candidate) => {
		if (isConfident(candidate.read.state)) return false;
		const narrowed = candidate.read.state === 'ambiguous' && candidate.read.ids.length > 0;
		return !narrowed || candidate.read.ids.includes(entryId);
	});
	if (unread) return { presence: 'unknown', site: unread.site };
	return { presence: 'absent', site: null };
}

function outcomeOfPosition(
	group: MercFilterGroup,
	entry: MercFilterEntry,
	kind: 'required' | 'forbidden' | 'bonus',
	presence: Presence
): PositionOutcome {
	const groupApplied = group.enabledInSearch;
	const buyerContextual = entry.buyerContextual === true;
	if (kind === 'forbidden') {
		// A parked denial is still a denial (see `entryKind`), but a parked group
		// filters nothing: report the hit and let the verdict stand.
		if (!groupApplied) return presence === 'present' ? 'parked-denial-present' : 'not-applied';
		if (presence === 'present') return 'fail';
		return presence === 'unknown' ? 'unknown' : 'pass';
	}
	// A `mercenary` group's own skill is its ROW ANCHOR, not a bonus it fired
	// (`isRowAnchor`): it reads as plainly present or absent, so nothing
	// downstream can mistake "this is the row" for "this mercenary earned
	// something". Every other parked entry is a bonus as before.
	if (kind === 'bonus' && !isRowAnchor(group.type, entry.id)) {
		if (presence === 'present') return 'bonus-fired';
		return presence === 'unknown' ? 'unknown' : 'absent';
	}
	if (buyerContextual) {
		if (presence === 'present') return 'contextual-present';
		return presence === 'unknown' ? 'unknown' : 'contextual-absent';
	}
	if (presence === 'present') return 'pass';
	return presence === 'unknown' ? 'unknown' : 'absent';
}

interface GroupEvaluation {
	outcome: GroupOutcome;
	positions: MercPosition[];
	confident: number;
	need: number;
}

/**
 * The entries a positive group actually counts, how many of them it needs, and
 * the non-contextual ones that decide whether the group applies at all.
 *
 * A buyer-contextual entry joins the pool only when the mercenary HAS it: then
 * it counts toward the need, flagged. When it is missing it leaves the pool
 * entirely, and the need shrinks with it — so `min: 2` over [A, contextual B]
 * asks for A alone from a mercenary without B. Letting the need stand at 2
 * there would gate the group on B, which is the exact thing this flag exists to
 * prevent, `min` or no `min`.
 */
function countedEntries(
	group: MercFilterGroup,
	presenceOfEntry: (entry: MercFilterEntry) => Presence
): { pool: MercFilterEntry[]; nonContextual: MercFilterEntry[]; need: number } {
	const required = group.entries.filter((entry) => entryKind(group, entry) === 'required');
	const nonContextual = required.filter((entry) => !entry.buyerContextual);
	const contextualPresent = required.filter(
		(entry) => entry.buyerContextual && presenceOfEntry(entry) === 'present'
	);
	const pool = [...nonContextual, ...contextualPresent];
	return { pool, nonContextual, need: Math.min(group.min ?? pool.length, pool.length) };
}

function evaluateGroupOver(rows: MercRow[], group: MercFilterGroup): GroupEvaluation {
	const scope = scopeOf(rows);
	const applied = group.enabledInSearch;
	const sightings = new Map(group.entries.map((entry) => [entry.id, presenceOf(scope, entry.id)]));
	const sightingOf = (entry: MercFilterEntry): Sighting =>
		sightings.get(entry.id) ?? { presence: 'absent', site: null };
	const { pool, nonContextual, need } = countedEntries(
		group,
		(entry) => sightingOf(entry).presence
	);
	const poolIds = new Set(pool.map((entry) => entry.id));

	const positions: MercPosition[] = group.entries.map((entry) => {
		const kind = entryKind(group, entry);
		const { presence, site } = sightingOf(entry);
		return {
			groupId: group.id,
			groupLabel: group.label,
			groupType: group.type,
			entryId: entry.id,
			entryName: entry.name,
			kind,
			buyerContextual: entry.buyerContextual === true,
			counted: poolIds.has(entry.id),
			presence,
			outcome: outcomeOfPosition(group, entry, kind, presence),
			site
		};
	});

	const counted = positions.filter((position) => position.counted);
	const confident = counted.filter((position) => position.presence === 'present').length;
	const possible = confident + counted.filter((position) => position.presence === 'unknown').length;

	if (group.type === 'not') {
		if (!applied) return { outcome: 'not-applied', positions, confident: 0, need: 0 };
		const denied = positions.filter((position) => position.kind === 'forbidden');
		if (denied.some((position) => position.presence === 'present')) {
			return { outcome: 'fail', positions, confident: 0, need: 0 };
		}
		if (denied.some((position) => position.presence === 'unknown')) {
			return { outcome: 'unknown', positions, confident: 0, need: 0 };
		}
		return { outcome: 'pass', positions, confident: 0, need: 0 };
	}

	// Nothing left to require: either the guide switched the whole group off, or
	// every entry it requires is a bonus or buyer-contextual. Guide A's Kinetist
	// Haste-only aura group is the case that matters — it must not read as a pass
	// the mercenary earned, not even when the mercenary has the Haste.
	if (!applied || nonContextual.length === 0) {
		return { outcome: 'not-applied', positions, confident, need };
	}
	if (confident >= need) return { outcome: 'pass', positions, confident, need };
	if (possible >= need) return { outcome: 'unknown', positions, confident, need };
	return { outcome: 'fail', positions, confident, need };
}

/**
 * Does ONE skill row satisfy this group? The single owner of row scoping.
 *
 * **A1 — verified 2026-08-21 against the live trade API.** A `mercenary`
 * group scopes to one skill row. Differential searches (Allflame, status
 * securable): a group with two skills (Spark + Conductivity) returns 0 while
 * the same pair as an `and` group returns 9,945 — every Spark merc carries
 * Conductivity, but never in one row; a same-row skill+support pair
 * (Spark + Return) returns 3,404, so two-entry groups do pass when co-rowed;
 * a cross-row pair (Conductivity + Return) returns 0. Full method and search
 * ids in POE-165 tracker comment. Everything row-scoped goes through this
 * function.
 */
function rowSatisfies(row: MercRow, group: MercFilterGroup): GroupEvaluation {
	return evaluateGroupOver([row], group);
}

const OUTCOME_RANK: Record<GroupOutcome, number> = {
	pass: 3,
	unknown: 2,
	fail: 1,
	'not-applied': 0
};

/**
 * A `mercenary` group is answered by the best row; every other type is answered
 * by the whole capture. When no row satisfies the group, the row that came
 * closest is the one reported — that is the row the user should look at.
 */
function evaluateGroup(capture: MercCapture, group: MercFilterGroup): MercGroupResult {
	const shared = {
		id: group.id,
		label: group.label,
		type: group.type,
		applied: group.enabledInSearch,
		min: group.min ?? null
	};

	if (group.type !== 'mercenary' || capture.rows.length === 0) {
		const evaluation = evaluateGroupOver(capture.rows, group);
		return { ...shared, ...evaluation, rowIndex: null };
	}

	// Structural outcomes do not vary row to row; keep them off the row axis so
	// the result does not claim a row is responsible for them.
	const structural = evaluateGroupOver(capture.rows, group);
	if (structural.outcome === 'not-applied') {
		return { ...shared, ...structural, rowIndex: null };
	}

	const perRow = capture.rows.map((row) => ({ row, evaluation: rowSatisfies(row, group) }));
	const best = perRow.reduce((winner, candidate) => {
		const byOutcome =
			OUTCOME_RANK[candidate.evaluation.outcome] - OUTCOME_RANK[winner.evaluation.outcome];
		if (byOutcome !== 0) return byOutcome > 0 ? candidate : winner;
		return candidate.evaluation.confident > winner.evaluation.confident ? candidate : winner;
	});
	return { ...shared, ...best.evaluation, rowIndex: best.row.index };
}

function nameOfSkill(read: MercSkillRead): string {
	return read.name ?? read.raw;
}

function nameOfSupport(read: MercSupportRead): string {
	return read.name ?? read.family ?? '';
}

/** Confidently read skills and supports that this ruleset says nothing about. */
function notInRules(capture: MercCapture, ruleset: MercRuleset): MercCaptured[] {
	const known = new Set(
		ruleset.groups.flatMap((group) => group.entries.map((entry) => entry.id))
	);
	const scope = scopeOf(capture.rows);
	const seen = [
		...scope.skills.map((s) => ({ read: s.read, site: s.site, name: nameOfSkill(s.read) })),
		...scope.supports.map((s) => ({ read: s.read, site: s.site, name: nameOfSupport(s.read) }))
	];
	return seen
		.filter(
			(entry) =>
				isConfident(entry.read.state) &&
				entry.read.ids.length > 0 &&
				!entry.read.ids.some((id) => known.has(id))
		)
		.map((entry) => ({ ids: entry.read.ids, name: entry.name, site: entry.site }));
}

function namesOf(positions: MercPosition[]): string {
	return positions.map((position) => position.entryName).join(', ');
}

/** Row-scoped groups answer for one row; say which, so the user knows where to look. */
function atRow(group: MercGroupResult): string {
	return group.rowIndex === null ? '' : ` on row ${group.rowIndex + 1}`;
}

function groupReasons(group: MercGroupResult): string[] {
	if (group.outcome === 'fail') {
		if (group.type === 'not') {
			return group.positions
				.filter((position) => position.outcome === 'fail')
				.map((position) => `${position.entryName} present — forbidden`);
		}
		// Only what the group counts: a parked bonus it also lacks is not why it failed.
		const missing = group.positions.filter(
			(position) => position.counted && position.presence === 'absent'
		);
		return [
			`${group.label}${atRow(group)}: needs ${group.need}, has ${group.confident}${
				missing.length > 0 ? ` — missing ${namesOf(missing)}` : ''
			}`
		];
	}
	if (group.outcome === 'unknown') {
		const unread =
			group.type === 'not'
				? group.positions.filter((position) => position.outcome === 'unknown')
				: group.positions.filter((position) => position.counted && position.presence === 'unknown');
		const what = unread.length > 0 ? namesOf(unread) : group.label;
		return group.type === 'not'
			? [`${group.label}: cannot rule out ${what} — hover to confirm`]
			: [`${group.label}${atRow(group)}: ${what} not read — hover to confirm`];
	}
	return [];
}

/**
 * On a GG pass, say how many supports the core row actually carries.
 *
 * A GG saved search asks for fewer links than a top mercenary can have, so its
 * comps are a floor rather than the price. HOW MANY fewer is the guide author's
 * claim, not this engine's — the Kinetist ladder's author says "these are 4L not
 * even 5L", the Manyshot one says to eyeball the Ice Shot links — so the number
 * lives in that rung's `authorNote` and this line stays archetype-neutral.
 *
 * The core row is the one answered by the group id `core`, which is the id a
 * ladder gives the group carrying its main skill and that skill's links. Not
 * every ladder has such a group: Nerotox's Wild Strike searches give that id to
 * no group at all, so their first live `mercenary` group is the damage group.
 * (They are not short of a skill-plus-one-link group — `return` is one — it just
 * is not the group they lead with, and every rung below GG parks it.) Those fall
 * back to the first LIVE `mercenary` group, which is row-scoped to the main
 * skill's row for the same reason.
 *
 * `core` is looked for first rather than the fallback being the whole rule,
 * because a rung can declare `core` without leading with it: the Manyshot GG
 * rung's first live `mercenary` group is `vaal-damage`, and the fallback alone
 * would point this line at the Vaal Ice Shot row while that rung's own
 * `authorNote` tells the reader to check the links on Ice Shot.
 */
function ggLinkCaveat(ruleset: MercRuleset, groups: MercGroupResult[], capture: MercCapture): string[] {
	if (ruleset.tier !== 'gg') return [];
	const core =
		groups.find((group) => group.id === 'core') ??
		groups.find((group) => group.type === 'mercenary' && group.applied);
	const row = core?.rowIndex === null || core?.rowIndex === undefined
		? undefined
		: capture.rows.find((candidate) => candidate.index === core.rowIndex);
	const links = row ? row.supports.length : null;
	const carried =
		links === null
			? ''
			: ` — this core skill row carries ${links} support${links === 1 ? '' : 's'}`;
	return [`GG comps are a floor, not the price: a merc with more links prices above them${carried}`];
}

function passReasons(ruleset: MercRuleset, groups: MercGroupResult[], capture: MercCapture): string[] {
	const positions = groups.flatMap((group) => group.positions);
	const reasons: string[] = [];
	if (ruleset.floor) reasons.push(`Floor ${ruleset.floor}`);
	const bonuses = positions.filter((position) => position.outcome === 'bonus-fired');
	if (bonuses.length > 0) reasons.push(`Bonuses fired: ${namesOf(bonuses)}`);
	const contextual = positions.filter((position) => position.outcome === 'contextual-present');
	if (contextual.length > 0) reasons.push(`Buyer-contextual present: ${namesOf(contextual)}`);
	reasons.push(...ggLinkCaveat(ruleset, groups, capture));
	// The author speaking about THIS search, verbatim and last — it is the line
	// that outranks anything computed above it when the two disagree.
	if (ruleset.authorNote) reasons.push(`Author: ${ruleset.authorNote}`);
	return reasons;
}

/**
 * The toggles the derived search should carry for THIS mercenary: every bonus
 * it fired switched on, every buyer-contextual entry it has switched on and
 * every one it lacks switched off.
 */
function flipsFor(groups: MercGroupResult[]): QueryFlips {
	const enable = new Set<string>();
	const disable = new Set<string>();
	const enableGroups = new Set<string>();
	for (const group of groups) {
		if (group.type === 'not') continue;
		const fired = group.positions.filter((position) => position.outcome === 'bonus-fired');
		// A bonus the guide parked by switching its whole group off is invisible
		// to the trade site until the group comes back on — so switch the group on
		// and leave only what actually fired inside it.
		const revive = !group.applied && fired.length > 0;
		if (revive) enableGroups.add(group.id);
		for (const position of group.positions) {
			// The row anchor is never flipped either way: it is the skill the group
			// is scoped to, so switching it off would leave a revived group asking
			// for links on nothing at all.
			if (isRowAnchor(group.type, position.entryId)) continue;
			const key = flipKey(group.id, position.entryId);
			if (position.outcome === 'bonus-fired' || position.outcome === 'contextual-present') {
				enable.add(key);
			} else if (position.outcome === 'contextual-absent' || revive) {
				disable.add(key);
			}
		}
	}
	return { enable, disable, enableGroups };
}

function evaluateRuleset(
	capture: MercCapture,
	ruleset: MercRuleset,
	league: string | null
): MercRulesetResult {
	const groups = ruleset.groups.map((group) => evaluateGroup(capture, group));
	const applied = groups.filter((group) => group.outcome !== 'not-applied');

	// A fail outranks an unknown: a mercenary carrying a forbidden stat is out
	// whatever else could not be read. A ruleset with NOTHING applied — every
	// group parked or contextual — has asked the mercenary for nothing, so it is
	// unknown rather than a pass nobody earned. None of the twenty saved searches
	// is in that state; a future one could be.
	const outcome: RulesetOutcome =
		applied.length === 0
			? 'unknown'
			: applied.some((group) => group.outcome === 'fail')
				? 'fail'
				: applied.some((group) => group.outcome === 'unknown')
					? 'unknown'
					: 'pass';

	const reasons =
		outcome === 'pass'
			? passReasons(ruleset, groups, capture)
			: applied.flatMap((group) => groupReasons(group));

	return {
		id: ruleset.id,
		label: ruleset.label,
		ladder: ruleset.ladder ?? null,
		tier: ruleset.tier ?? null,
		tierLabel: ruleset.tierLabel ?? null,
		outcome,
		groups,
		notInRules: notInRules(capture, ruleset),
		reasons,
		floor: ruleset.floor ?? null,
		// The saved link stays bound to the league its hash was saved in; the
		// derived link is a live search, so it belongs to the league being played.
		savedUrl:
			ruleset.savedSearch === undefined ? null : savedSearchUrl(ruleset.savedSearch),
		derivedUrl:
			league === null ? null : derivedSearchUrl(league, rulesetQuery(ruleset, flipsFor(groups)))
	};
}

/**
 * Which passing rulesets to comp against.
 *
 * EACH ladder is answered by the passing rung with the highest `TIERS` rank,
 * and that is the whole rule — a ladder's rungs are NOT assumed to be nested.
 * Wild Strike is the counter-example: a mercenary carrying Faster Attacks
 * (Tier 2) and no Greater Faster Attacks passes Minimum and Endgame while
 * failing Midgame, whose `speed` group parks the Tier-2 entry inside a live
 * group, and Endgame is still what this names. Two ladders are two different
 * searches, and a pass on one must neither suppress nor be suppressed by the
 * other. Rulesets in no ladder are independent searches too, so every passing
 * one is listed.
 *
 * Ties on `TIERS` index keep the earlier rung: a ladder may publish two rungs at
 * one tier (Nerotox's Combatant video has two Endgame links), and declaration
 * order is the guide's own order.
 */
function bestOf(results: MercRulesetResult[]): string[] {
	const passing = results.filter((result) => result.outcome === 'pass');
	const best: string[] = [];
	const highestPerLadder = new Map<string, MercRulesetResult>();
	for (const result of passing) {
		if (result.ladder === null) {
			best.push(result.id);
			continue;
		}
		const winner = highestPerLadder.get(result.ladder);
		const rank = (r: MercRulesetResult) => TIERS.indexOf(r.tier as (typeof TIERS)[number]);
		if (!winner || rank(result) > rank(winner)) highestPerLadder.set(result.ladder, result);
	}
	return [...best, ...[...highestPerLadder.values()].map((result) => result.id)];
}

/**
 * Who is speaking, in front of every reason line.
 *
 * A rung that spells its own tier out says so — two rungs of the Frost Blades
 * ladder are both `end`, and "Frost Blades (end)" twice would leave the reader
 * unable to tell which search answered. A rung with no `tierLabel` reads exactly
 * as it always has.
 */
function titleOf(result: MercRulesetResult): string {
	if (!result.tier) return result.label;
	return `${result.label} (${result.tierLabel ?? result.tier})`;
}

function evaluateSource(
	capture: MercCapture,
	source: MercSource,
	enabled: boolean,
	league: string | null
): MercSourceVerdict {
	if (!enabled) {
		return { id: source.id, label: source.label, headline: 'off', best: [], reasons: [], rulesets: [] };
	}

	const rulesets = source.rulesets.map((ruleset) => evaluateRuleset(capture, ruleset, league));
	const best = bestOf(rulesets);
	const headline: SourceHeadline =
		best.length > 0
			? 'worth'
			: rulesets.some((result) => result.outcome === 'unknown')
				? 'unknown'
				: 'skip';

	const spoken =
		headline === 'worth'
			? rulesets.filter((result) => best.includes(result.id))
			: headline === 'unknown'
				? rulesets.filter((result) => result.outcome === 'unknown')
				: rulesets;

	return {
		id: source.id,
		label: source.label,
		headline,
		best,
		reasons: spoken.flatMap((result) =>
			result.reasons.map((reason) => `${titleOf(result)}: ${reason}`)
		),
		rulesets
	};
}

/**
 * Verdict one captured mercenary against every source.
 *
 * `enabledSources` is the set the user left switched ON; a source outside it is
 * reported `off` with no results rather than dropped, so the page can show the
 * whole strip and say what it is not evaluating.
 *
 * `league` is the league being played (SSOT), and it is allowed to be null:
 * every verdict still evaluates, and only the derived links come back null. The
 * saved links never depend on it.
 */
export function evaluateCapture(
	capture: MercCapture,
	sources: MercSource[],
	enabledSources: ReadonlySet<MercSourceId>,
	league: string | null
): MercVerdict {
	return {
		sources: sources.map((source) =>
			evaluateSource(capture, source, enabledSources.has(source.id), league)
		)
	};
}
